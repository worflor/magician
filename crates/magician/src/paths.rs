//! Where Magician finds things outside its own repository: the game, the reference folder, Valve's CSDK, the
//! Source 2 Viewer CLI and the game lock.
//!
//! Each one resolves the same way, first answer wins: a command-line flag, an environment variable, a line in the
//! user's config file, then discovery or a default. `magician paths` prints what every one resolved to and why.
//! The Python and PowerShell side reads the same variables and the same file (`ingame/gamepaths.py`), so the CLI and
//! the harness always agree on where the game and the game lock are.
//!
//! The config file is plain text, one `key = value` per line, `#` comments, quotes optional:
//!
//! ```text
//! game      = D:\SteamLibrary\steamapps\common\Deadlock\game
//! reference = D:\deadlock-reference
//! csdk      = D:\Tools\csdk\Reduced_CSDK_13
//! vrf       = D:\Tools\Source2Viewer\Source2Viewer-CLI.exe
//! ```
//!
//! It lives at `MAGICIAN_CONFIG`, else `%APPDATA%\magician\config.txt` (Windows) or
//! `$XDG_CONFIG_HOME/magician/config.txt` (`~/.config/magician/config.txt`).

use std::path::{Path, PathBuf};

/// One resolved location and where the answer came from (`--game`, `DEADLOCK_GAME`, `config`, `steam`, `default`).
pub struct Found {
    pub path: PathBuf,
    pub source: String,
}

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

/// The config file's location, whether or not it exists.
pub fn config_file() -> Option<PathBuf> {
    if let Some(p) = env("MAGICIAN_CONFIG") {
        return Some(PathBuf::from(p));
    }
    if cfg!(windows) {
        return env("APPDATA").map(|d| PathBuf::from(d).join("magician").join("config.txt"));
    }
    env("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env("HOME").map(|h| PathBuf::from(h).join(".config")))
        .map(|d| d.join("magician").join("config.txt"))
}

/// The value of `key` in a config text: the last `key = value` line wins; `#` starts a comment line.
pub fn config_lookup(text: &str, key: &str) -> Option<String> {
    let mut found = None;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            if k.trim().eq_ignore_ascii_case(key) {
                let v = v.trim().trim_matches('"').trim();
                found = (!v.is_empty()).then(|| v.to_string());
            }
        }
    }
    found
}

fn config(key: &str) -> Option<String> {
    let text = std::fs::read_to_string(config_file()?).ok()?;
    config_lookup(&text, key)
}

/// A flag value, then an environment variable, then a config key.
fn explicit(flag: Option<String>, flag_name: &str, var: &str, key: &str) -> Option<Found> {
    if let Some(v) = flag {
        return Some(Found { path: PathBuf::from(v), source: flag_name.into() });
    }
    if let Some(v) = env(var) {
        return Some(Found { path: PathBuf::from(v), source: var.into() });
    }
    config(key).map(|v| Found { path: PathBuf::from(v), source: format!("config ({key})") })
}

// ------------------------------------------------------------------------------------------------------- the game

fn is_game_dir(p: &Path) -> bool {
    p.join("citadel").join("steam.inf").is_file()
}

/// `p` if it is Deadlock's `game` folder, or its `game` child if `p` is the Deadlock folder above it.
fn game_folder(p: &Path) -> Option<PathBuf> {
    if is_game_dir(p) {
        Some(p.to_path_buf())
    } else {
        Some(p.join("game")).filter(|g| is_game_dir(g))
    }
}

/// Library roots listed in Steam's `libraryfolders.vdf`: the second quoted string of every `"path"` line.
pub fn library_paths(vdf: &str) -> Vec<PathBuf> {
    vdf.lines()
        .filter_map(|l| {
            let q: Vec<&str> = l.split('"').collect();
            (q.len() >= 4 && q[1] == "path").then(|| PathBuf::from(q[3].replace("\\\\", "\\")))
        })
        .collect()
}

/// Steam installs to look in: the registry's `SteamPath` and Program Files on Windows; the native, Flatpak and
/// Snap homes elsewhere.
fn steam_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if cfg!(windows) {
        if let Ok(o) = std::process::Command::new("reg").args(["query", r"HKCU\Software\Valve\Steam", "/v", "SteamPath"]).output() {
            if let Some(p) = String::from_utf8_lossy(&o.stdout).lines().find_map(|l| l.split("REG_SZ").nth(1)) {
                roots.push(PathBuf::from(p.trim().replace('/', "\\")));
            }
        }
        for var in ["ProgramFiles(x86)", "ProgramFiles"] {
            if let Some(pf) = env(var) {
                roots.push(PathBuf::from(pf).join("Steam"));
            }
        }
    } else if let Some(h) = env("HOME").map(PathBuf::from) {
        for rel in [".steam/steam", ".local/share/Steam", ".var/app/com.valvesoftware.Steam/.local/share/Steam", "snap/steam/common/.local/share/Steam"] {
            roots.push(h.join(rel));
        }
    }
    roots
}

/// Deadlock's `game` folder through Steam's own library list.
fn steam_game() -> Option<PathBuf> {
    for s in steam_roots() {
        let mut libs = vec![s.clone()];
        if let Ok(t) = std::fs::read_to_string(s.join("steamapps").join("libraryfolders.vdf")) {
            libs.extend(library_paths(&t));
        }
        for l in libs {
            if let Some(g) = game_folder(&l.join("steamapps").join("common").join("Deadlock")) {
                return Some(g);
            }
        }
    }
    None
}

/// Deadlock's `game` folder: `--game`, `DEADLOCK_GAME`, the config's `game`, then Steam's library list. An explicit
/// answer may name the `game` folder or the Deadlock folder above it, and is refused when neither holds a game.
pub fn game(flag: Option<String>) -> Result<Found, String> {
    if let Some(f) = explicit(flag, "--game", "DEADLOCK_GAME", "game") {
        return game_folder(&f.path)
            .map(|path| Found { path, source: f.source.clone() })
            .ok_or_else(|| format!("{} {}: no citadel\\steam.inf there (point it at Deadlock's `game` folder)", f.source, f.path.display()));
    }
    steam_game()
        .map(|path| Found { path, source: "steam".into() })
        .ok_or_else(|| "could not find Deadlock through Steam; pass --game <...\\Deadlock\\game>, set DEADLOCK_GAME, or add `game = ...` to the config file (magician paths shows where)".into())
}

// ----------------------------------------------------------------------------------------------- everything else

/// The reference folder (baselines, rig sidecars, users indexes, and `work\` for harness output):
/// `--reference` (main.rs turns it into `MAGICIAN_REFERENCE`), `MAGICIAN_REFERENCE`, the config's `reference`, else
/// `%LOCALAPPDATA%\magician\reference` (`$XDG_DATA_HOME/magician/reference`, `~/.local/share/...`).
pub fn reference() -> Found {
    if let Some(f) = explicit(None, "--reference", "MAGICIAN_REFERENCE", "reference") {
        return f;
    }
    let base = if cfg!(windows) {
        env("LOCALAPPDATA").map(PathBuf::from)
    } else {
        env("XDG_DATA_HOME").map(PathBuf::from).or_else(|| env("HOME").map(|h| PathBuf::from(h).join(".local").join("share")))
    };
    Found { path: base.unwrap_or_default().join("magician").join("reference"), source: "default".into() }
}

/// Valve's CSDK (the folder holding `content\` and `game\`): `--csdk`, `MAGICIAN_CSDK`, the config's `csdk`. Only
/// `build`, `release` and `sign` need it, so there is no default to guess.
pub fn csdk(flag: Option<String>) -> Result<Found, String> {
    explicit(flag, "--csdk", "MAGICIAN_CSDK", "csdk").ok_or_else(|| {
        "this needs Valve's CSDK (the folder with content\\ and game\\bin\\); pass --csdk DIR, set MAGICIAN_CSDK, or add `csdk = ...` to the config file".into()
    })
}

/// The Source 2 Viewer CLI: `--vrf`, `MAGICIAN_VRF`, the config's `vrf`, else `Source2Viewer-CLI` from PATH.
pub fn vrf(flag: Option<String>) -> Found {
    explicit(flag, "--vrf", "MAGICIAN_VRF", "vrf").unwrap_or_else(|| Found { path: PathBuf::from("Source2Viewer-CLI"), source: "PATH".into() })
}

/// The folder that holds the mods `patchday` checks, when one is named: `MAGICIAN_MODS`, else `mods =` in the config
/// file. A scheduled job runs from wherever the scheduler starts it, so an always-on machine names the folder once.
pub fn mods() -> Option<Found> {
    explicit(None, "", "MAGICIAN_MODS", "mods")
}

/// The toolkit folder, the one holding `ingame\mod_session.py` and `tools\` (the Python the front-door verbs run):
/// `MAGICIAN_HOME`, the config's `home`, else the nearest folder above this executable that holds
/// `ingame/mod_session.py` (a checkout's `target\release\magician.exe` finds its own checkout). `Err` says what was
/// looked for and how to point at it.
pub fn toolkit() -> Result<Found, String> {
    let marker = |d: &Path| d.join("ingame").join("mod_session.py").is_file();
    if let Some(f) = explicit(None, "", "MAGICIAN_HOME", "home") {
        return if marker(&f.path) { Ok(f) } else { Err(format!("{} ({}) holds no ingame\\mod_session.py: point MAGICIAN_HOME (or `home =` in the config file) at the magician folder", f.path.display(), f.source)) };
    }
    let exe = std::env::current_exe().map_err(|e| format!("this executable's own path can't be read ({e}); set MAGICIAN_HOME to the magician folder"))?;
    exe.ancestors()
        .skip(1)
        .find(|d| marker(d))
        .map(|d| Found { path: d.to_path_buf(), source: "the folder above magician.exe that holds ingame\\mod_session.py".into() })
        .ok_or_else(|| format!("no folder above {} holds ingame\\mod_session.py, so the toolkit's Python tools are not here: set MAGICIAN_HOME (or `home =` in the config file) to the magician folder (the checkout with ingame\\ and tools\\)", exe.display()))
}

/// The Python 3 that runs the toolkit's tools: `MAGICIAN_PYTHON`, the config's `python`, else the first of `python`,
/// `python3` and (Windows) `py -3` on PATH that answers `--version` with Python 3. The program and the arguments that
/// come before a script.
pub fn python() -> Result<(Found, Vec<String>), String> {
    let answers = |prog: &Path, pre: &[&str]| {
        std::process::Command::new(prog)
            .args(pre)
            .arg("--version")
            .stdin(std::process::Stdio::null())
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr)))
            .filter(|v| v.trim_start().starts_with("Python 3"))
    };
    if let Some(f) = explicit(None, "", "MAGICIAN_PYTHON", "python") {
        return match answers(&f.path, &[]) {
            Some(_) => Ok((f, Vec::new())),
            None => Err(format!("{} ({}) does not run as Python 3: point MAGICIAN_PYTHON (or `python =` in the config file) at python.exe", f.path.display(), f.source)),
        };
    }
    let mut tries: Vec<(&str, Vec<&str>)> = vec![("python", vec![]), ("python3", vec![])];
    if cfg!(windows) {
        tries.push(("py", vec!["-3"]));
    }
    for (prog, pre) in tries {
        if answers(Path::new(prog), &pre).is_some() {
            return Ok((Found { path: PathBuf::from(prog), source: "PATH".into() }, pre.iter().map(|s| s.to_string()).collect()));
        }
    }
    Err("no Python 3 on PATH (python, python3, py -3): install Python 3.8 or newer, or set MAGICIAN_PYTHON (or `python =` in the config file) to python.exe".into())
}

/// The policy file (every judgement number a lint and a grade use): `--policy`, `MAGICIAN_POLICY`, the config's
/// `policy`. `None` means the built-in defaults; there is no file to discover.
pub fn policy(flag: Option<String>) -> Option<Found> {
    explicit(flag, "--policy", "MAGICIAN_POLICY", "policy")
}

/// Where `magician update` looks for a release (the three files `VERSION`, `SHA256SUMS` and the zip): `--source`,
/// `MAGICIAN_UPDATE_SOURCE`, the config's `update_source`, else the `releases/latest/download` address of the
/// repository this build came from. A folder (local or on the network) is as good as a URL. The location is kept as
/// text, since it is often not a path.
pub fn update_source(flag: Option<String>) -> Found {
    explicit(flag, "--source", "MAGICIAN_UPDATE_SOURCE", "update_source").unwrap_or_else(|| Found {
        path: PathBuf::from(format!("{}/releases/latest/download", env!("CARGO_PKG_REPOSITORY").trim_end_matches('/'))),
        source: "default (this build's repository)".into(),
    })
}

/// An addon is named by one folder in the CSDK's `citadel_addons`: not empty, no separator, drive or `..`. The name is
/// joined onto the CSDK's content and game trees, where a build writes its lock, log and outputs, so `""` (an unset
/// variable) would mean every addon at once, and `C:\x` or `..` somewhere outside the CSDK.
pub fn addon_name(a: &str) -> Result<(), String> {
    if a.is_empty() || a == "." || a == ".." || a.contains(['/', '\\', ':']) || a.trim() != a {
        return Err(format!("addon {a:?}: an addon is one folder name in the CSDK's content\\citadel_addons"));
    }
    Ok(())
}

/// Where a mod's own allowance files are, from its `mod.toml`: `addon/magician.allow` beside it (the layout
/// `magician new` writes), `magician.allow` beside it, then `content/citadel_addons/<addon>/magician.allow`
/// in the CSDK for the addon its slug names (`maid-lash` is `maid_lash`) and every `[mod] owners` addon. Each file
/// once, however many of these lead to it (the CSDK addon is often a junction to the mod's own `addon/`). Returns the
/// files found and every place looked.
pub fn mod_allowances(toml: &Path, toml_text: &str, csdk: Option<&Path>) -> (Vec<Found>, Vec<PathBuf>) {
    let dir = toml.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let mut candidates = vec![(dir.join("addon").join("magician.allow"), "beside the mod.toml, in addon".to_string()), (dir.join("magician.allow"), "beside the mod.toml".to_string())];
    if let Some(c) = csdk {
        let mut addons: Vec<String> = grade::manifest::string(toml_text, "mod", "slug").map(|s| s.replace('-', "_")).into_iter().collect();
        addons.extend(grade::manifest::list(toml_text, "mod", "owners"));
        for a in addons {
            if addon_name(&a).is_err() {
                continue;
            }
            candidates.push((c.join("content").join("citadel_addons").join(&a).join("magician.allow"), format!("the CSDK addon {a}")));
        }
    }
    let mut found: Vec<Found> = Vec::new();
    let mut seen: Vec<PathBuf> = Vec::new();
    let mut looked = Vec::new();
    for (p, source) in candidates {
        looked.push(p.clone());
        if !p.is_file() {
            continue;
        }
        let canon = std::fs::canonicalize(&p).unwrap_or_else(|_| p.clone());
        if seen.contains(&canon) {
            continue;
        }
        seen.push(canon);
        found.push(Found { path: p, source });
    }
    (found, looked)
}

/// The game lock `ingame/gamelock.py` writes: `MAGICIAN_GAME_LOCK`, the config's `game_lock`, else
/// `<reference>\work\ingame-game.lock`.
pub fn game_lock() -> Found {
    explicit(None, "", "MAGICIAN_GAME_LOCK", "game_lock")
        .unwrap_or_else(|| Found { path: reference().path.join("work").join("ingame-game.lock"), source: "reference".into() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_lines_last_wins_comments_and_quotes() {
        let t = "# where things are\ngame = C:\\old\n reference=\"D:\\ref\"\nGAME = C:\\new\nempty =\n";
        assert_eq!(config_lookup(t, "game").as_deref(), Some("C:\\new"));
        assert_eq!(config_lookup(t, "reference").as_deref(), Some("D:\\ref"));
        assert_eq!(config_lookup(t, "empty"), None);
        assert_eq!(config_lookup("# game = x\n", "game"), None);
    }

    #[test]
    fn a_mods_allowance_files_are_found_beside_it_and_in_its_csdk_addon_each_once() {
        let d = std::env::temp_dir().join(format!("magician-modallow-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let csdk = d.join("csdk");
        let addons = csdk.join("content").join("citadel_addons");
        // The mod's folder is the CSDK addon itself (as a junction makes it): one file, two routes to it.
        let lash = addons.join("maid_lash");
        std::fs::create_dir_all(&lash).unwrap();
        std::fs::write(lash.join("mod.toml"), "[mod]
slug = \"maid-lash\"
owners = [\"lash_voice\", \"..\", \"a/b\"]
").unwrap();
        std::fs::write(lash.join("magician.allow"), "").unwrap();
        std::fs::create_dir_all(addons.join("lash_voice")).unwrap();
        std::fs::write(addons.join("lash_voice").join("magician.allow"), "").unwrap();
        std::fs::write(addons.join("magician.allow"), "").unwrap(); // what an owner of ".." would reach
        let toml = lash.join("mod.toml");
        let text = std::fs::read_to_string(&toml).unwrap();
        let (found, looked) = mod_allowances(&toml, &text, Some(&csdk));
        let got: Vec<(PathBuf, &str)> = found.iter().map(|f| (f.path.clone(), f.source.as_str())).collect();
        assert_eq!(got, vec![(lash.join("magician.allow"), "beside the mod.toml"), (addons.join("lash_voice").join("magician.allow"), "the CSDK addon lash_voice")]);
        assert!(looked.contains(&addons.join("maid_lash").join("magician.allow")), "{looked:?}");
        // `addon/` beside the mod.toml comes first; without a CSDK only the mod's own folder is looked in
        std::fs::create_dir_all(lash.join("addon")).unwrap();
        std::fs::write(lash.join("addon").join("magician.allow"), "").unwrap();
        let (found, looked) = mod_allowances(&toml, &text, None);
        assert_eq!(found.iter().map(|f| f.source.as_str()).collect::<Vec<_>>(), vec!["beside the mod.toml, in addon", "beside the mod.toml"]);
        assert_eq!(looked.len(), 2);
        // nothing anywhere: no files, and the places looked are named
        let bare = d.join("bare");
        std::fs::create_dir_all(&bare).unwrap();
        let (found, looked) = mod_allowances(&bare.join("mod.toml"), "", Some(&csdk));
        assert!(found.is_empty() && looked.len() == 2, "{looked:?}");
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn steam_library_list() {
        let vdf = "\"libraryfolders\"\n{\n\t\"0\"\n\t{\n\t\t\"path\"\t\t\"C:\\\\Program Files (x86)\\\\Steam\"\n\t\t\"label\"\t\t\"\"\n\t}\n\t\"1\"\n\t{\n\t\t\"path\"\t\t\"E:\\\\Games\\\\SteamLibrary\"\n\t}\n}\n";
        assert_eq!(library_paths(vdf), vec![PathBuf::from(r"C:\Program Files (x86)\Steam"), PathBuf::from(r"E:\Games\SteamLibrary")]);
    }
}
