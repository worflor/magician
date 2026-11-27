//! Which CSDK this is, read from its own files: the community's version (12, on CS2's tools, or 13, on Dota 2's),
//! the binary sets it carries (`game/bin`, `bin_tools`, `bin_server`, `bin_cs2`, and the folders under each that
//! hold a resourcecompiler), the game files it was put together from, what its gameinfo allows, and the gaps known
//! for that version, each with what it breaks and whether Magician works round it.
//!
//! Nothing here runs a tool or writes a file. `doctor` prints the description, `build` and `release` print the
//! compiler they chose from it, and `magician csdk` prints all of it (`--json`: magician.csdk/1).
//!
//! What the version is rests on evidence, in this order, and the description names the one it used: the CSDK's
//! `read_me.txt` (the community kits say "Community SDK Version 13"); the binary sets (CSDK 12 ships `bin_tools` and
//! `bin_cs2` beside `bin` and `bin_server`, CSDK 13 only `bin` and `bin_server`, from the community's pages); the
//! folder's name (`Reduced_CSDK_13`). Only CSDK 13 has been run here; what is said of 12 comes from the community's
//! documentation and is marked so.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Version {
    V12,
    V13,
    Unknown,
}

impl Version {
    pub fn label(self) -> &'static str {
        match self {
            Version::V12 => "CSDK 12",
            Version::V13 => "CSDK 13",
            Version::Unknown => "a CSDK of unknown version",
        }
    }
    pub fn id(self) -> &'static str {
        match self {
            Version::V12 => "12",
            Version::V13 => "13",
            Version::Unknown => "unknown",
        }
    }
    /// Whose tools the version was made from.
    pub fn base(self) -> &'static str {
        match self {
            Version::V12 => "Counter-Strike 2's tools",
            Version::V13 => "Dota 2's Workshop Tools merged with Deadlock's own binaries",
            Version::Unknown => "unknown tools",
        }
    }
}

/// One folder of binaries: `bin/win64`, `bin/win64_nm`, `bin_server/win64`...
#[derive(Debug, Clone)]
pub struct BinSet {
    /// `bin/win64`, always with `/`.
    pub name: String,
    pub dir: PathBuf,
    pub compiler: bool,
    /// The date of its resourcecompiler.dll (or .exe), when it has one: the tools it was taken from.
    pub dated: Option<String>,
    pub role: &'static str,
}

/// A known gap of a CSDK: what it is, what it breaks, and what Magician does about it (None: nothing can).
#[derive(Debug, Clone)]
pub struct Gap {
    pub id: &'static str,
    pub what: String,
    pub breaks: String,
    pub handled: Option<String>,
    /// Seen on this machine's files (true), or a community report for this version that the files can't show.
    pub seen: bool,
}

#[derive(Debug, Clone)]
pub struct Csdk {
    pub root: PathBuf,
    pub version: Version,
    pub version_from: String,
    pub bins: Vec<BinSet>,
    /// `ClientVersion` of the CSDK's own `game/citadel/steam.inf`: the game build its files were taken from.
    pub game_build: Option<String>,
    pub modeldoc_features: Vec<String>,
    pub gaps: Vec<Gap>,
}

/// The role a binary set plays, from the community's pages for each version.
fn role(version: Version, set: &str, sub: &str) -> &'static str {
    match (version, set, sub) {
        (_, "bin", "win64_nm") => "win64 with Deadlock's own resourcecompiler.dll and animationsystem.dll: compiles clips with tagged sound events (made by hand, not in the kit)",
        (Version::V13, "bin", _) => "the default: Dota 2's tools; Hammer, the material editor, ModelDoc, the particle editor",
        (Version::V13, "bin_server", _) => "server mode (asset browser, material editor, ModelDoc); previewing any particle crashes",
        (Version::V12, "bin", _) => "the default: CS2's tools; compiles AnimGraph1; some projected-particle previews crash",
        (Version::V12, "bin_tools", _) => "no projected-particle crash, but compiling AnimGraph1 crashes",
        (Version::V12, "bin_server", _) => "server mode: needs the full game files; asset browser, material editor, some vdata and Panorama compiles",
        (Version::V12, "bin_cs2", _) => "CS2's map compiler binaries, which the GUI map compiler uses for lit maps",
        _ => "",
    }
}

/// `YYYY-MM-DD` of a file's modification time.
fn file_date(p: &Path) -> Option<String> {
    let t = std::fs::metadata(p).ok()?.modified().ok()?;
    let secs = t.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs() as i64;
    Some(civil(secs.div_euclid(86_400)))
}

/// A day count from 1970-01-01 as `YYYY-MM-DD` (Howard Hinnant's civil_from_days).
pub fn civil(days: i64) -> String {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

fn list_dirs(p: &Path) -> Vec<(String, PathBuf)> {
    let mut v: Vec<(String, PathBuf)> = std::fs::read_dir(p)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| (e.file_name().to_string_lossy().to_string(), e.path()))
        .collect();
    v.sort();
    v
}

/// The `features` value of the gameinfo's ModelDoc block, split on `;`.
pub fn modeldoc_features(gi: &str) -> Vec<String> {
    let Some(at) = gi.find("ModelDoc") else { return Vec::new() };
    let rest = &gi[at..];
    let end = rest.find('}').unwrap_or(rest.len());
    for line in rest[..end].lines() {
        let t = line.trim();
        if t.trim_start_matches('"').to_ascii_lowercase().starts_with("features") {
            let parts: Vec<&str> = t.split('"').filter(|s| !s.trim().is_empty()).collect();
            if let Some(v) = parts.last() {
                return v.split(';').map(|s| s.trim().to_string()).filter(|s| !s.is_empty() && s != "features").collect();
            }
        }
    }
    Vec::new()
}

/// `ClientVersion` from a steam.inf's text.
pub fn client_version(inf: &str) -> Option<String> {
    inf.lines().find_map(|l| l.trim().strip_prefix("ClientVersion=")).map(|s| s.trim().to_string())
}

/// A path that lies in a folder OneDrive syncs. The community's guides warn that the tools crash there (permissions
/// on synced folders); Steam's own advice is the same for games.
pub fn in_onedrive(p: &Path) -> bool {
    p.components().any(|c| c.as_os_str().to_string_lossy().to_ascii_lowercase().starts_with("onedrive"))
}

/// The version, from the readme, the binary sets, then the folder's name; with the evidence used.
fn version_of(root: &Path, sets: &[String]) -> (Version, String) {
    for name in ["read_me.txt", "readme.txt", "README.txt"] {
        if let Ok(t) = std::fs::read_to_string(root.join(name)) {
            let l = t.to_ascii_lowercase();
            for (v, needles) in [(Version::V13, ["version 13", "csdk 13", "csdk_13"]), (Version::V12, ["version 12", "csdk 12", "csdk_12"])] {
                if needles.iter().any(|n| l.contains(n)) {
                    return (v, format!("{name} names it"));
                }
            }
        }
    }
    if sets.iter().any(|s| s == "bin_cs2" || s == "bin_tools") {
        return (Version::V12, "it has bin_tools or bin_cs2, which only CSDK 12 ships".into());
    }
    let folder = root.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    if folder.contains("csdk_13") || folder.contains("csdk-13") || folder.contains("csdk13") {
        return (Version::V13, "its folder's name".into());
    }
    if folder.contains("csdk_12") || folder.contains("csdk-12") || folder.contains("csdk12") {
        return (Version::V12, "its folder's name".into());
    }
    if !sets.is_empty() && sets.iter().all(|s| s == "bin" || s == "bin_server") {
        return (Version::V13, "only bin and bin_server, the layout of CSDK 13 (no readme names a version)".into());
    }
    (Version::Unknown, "no readme names a version and the layout matches neither".into())
}

/// Describe the CSDK at `root`. Reads only.
pub fn describe(root: &Path) -> Csdk {
    let game = root.join("game");
    let sets: Vec<(String, PathBuf)> = list_dirs(&game).into_iter().filter(|(n, _)| n == "bin" || n.starts_with("bin_")).collect();
    let set_names: Vec<String> = sets.iter().map(|(n, _)| n.clone()).collect();
    let (version, version_from) = version_of(root, &set_names);
    let mut bins = Vec::new();
    for (set, dir) in &sets {
        for (sub, sd) in list_dirs(dir) {
            let rc = sd.join("resourcecompiler.exe");
            let dll = sd.join("resourcecompiler.dll");
            if !rc.is_file() && !dll.is_file() {
                continue;
            }
            bins.push(BinSet {
                name: format!("{set}/{sub}"),
                dated: file_date(&dll).or_else(|| file_date(&rc)),
                compiler: rc.is_file(),
                role: role(version, set, &sub),
                dir: sd,
            });
        }
    }
    let gi = std::fs::read_to_string(game.join("citadel").join("gameinfo.gi")).unwrap_or_default();
    let modeldoc = modeldoc_features(&gi);
    let game_build = std::fs::read_to_string(game.join("citadel").join("steam.inf")).ok().and_then(|t| client_version(&t));
    let mut gaps = Vec::new();
    if !gi.is_empty() && !modeldoc.iter().any(|f| f == "distancefield") {
        gaps.push(Gap {
            id: "distancefield",
            what: "the gameinfo's ModelDoc features lack distancefield".into(),
            breaks: "a model document with a DistanceFieldList fails to compile (\"Failed to allocate an instance of class 'DistanceFieldList'\")".into(),
            handled: Some("build and release compile those documents against game/citadel_dstf, a copy of the gameinfo with the feature".into()),
            seen: true,
        });
    }
    let has_nm = bins.iter().any(|b| b.name == "bin/win64_nm" && b.compiler);
    if version == Version::V13 {
        gaps.push(Gap {
            id: "tagged-sound",
            what: "bin/win64 is Dota 2's compiler, which can't allocate CNmClipDocEvent_TaggedSound".into(),
            breaks: "an animation clip with a tagged sound event fails to compile".into(),
            handled: Some(if has_nm {
                "build picks bin/win64_nm for such clips".into()
            } else {
                "build needs bin/win64_nm for such clips and this CSDK has none: make it as a copy of bin/win64 with Deadlock's own resourcecompiler.dll and animationsystem.dll".into()
            }),
            seen: true,
        });
        gaps.push(Gap {
            id: "sh2-lum",
            what: "Hammer's light baker lacks the SH2_lum lightmap channel (the kit's readme, Oct 20)".into(),
            breaks: "maps baked here get dark lightmaps; hero skins are lit by the game, not by a bake, so they are not touched".into(),
            handled: None,
            seen: false,
        });
        gaps.push(Gap {
            id: "particle-preview",
            what: "the particle editor's preview crashes on particles with lights, and bin_server crashes on any particle preview".into(),
            breaks: "particles can't be looked at in the tools".into(),
            handled: Some("author particles as text; look at them in the game (magician session)".into()),
            seen: false,
        });
    }
    if version == Version::V12 {
        gaps.push(Gap {
            id: "v12-untested",
            what: "CSDK 12 is described from the community's pages; Magician has not run its compiler".into(),
            breaks: "nothing known, but no CSDK 12 output has been compared with CSDK 13's here".into(),
            handled: Some("build uses bin/win64 as it does for 13 and says it is CSDK 12; check the first build with lint and in game".into()),
            seen: false,
        });
        gaps.push(Gap {
            id: "v12-animgraph",
            what: "bin_tools crashes compiling AnimGraph1 (community pages)".into(),
            breaks: "a graph compile under bin_tools".into(),
            handled: Some("build never picks bin_tools on its own".into()),
            seen: false,
        });
    }
    if root.join("game").join("citadel").join("readonly_tools_asset_info.binOFF").is_file() && !root.join("game").join("citadel").join("readonly_tools_asset_info.bin").is_file() {
        gaps.push(Gap {
            id: "asset-info",
            what: "game/citadel/readonly_tools_asset_info.binOFF has not been renamed to .bin".into(),
            breaks: "the tools do not mount the game's VPKs (the kit's setup step)".into(),
            handled: None,
            seen: true,
        });
    }
    if in_onedrive(root) {
        gaps.push(Gap {
            id: "onedrive",
            what: "the CSDK is inside a folder OneDrive syncs".into(),
            breaks: "the tools lose permissions on synced files and crash (community guides)".into(),
            handled: None,
            seen: true,
        });
    }
    Csdk { root: root.to_path_buf(), version, version_from, bins, game_build, modeldoc_features: modeldoc, gaps }
}

impl Csdk {
    /// One line: version, base, compilers and the game build its files came from.
    pub fn summary(&self) -> String {
        let compilers: Vec<&str> = self.bins.iter().filter(|b| b.compiler).map(|b| b.name.as_str()).collect();
        format!(
            "{} ({}; {}), compilers {}{}",
            self.version.label(),
            self.version.base(),
            self.version_from,
            if compilers.is_empty() { "none".to_string() } else { compilers.join(", ") },
            self.game_build.as_ref().map(|b| format!(", its game files from build {b}")).unwrap_or_default()
        )
    }
    /// The gaps that stop something a mod's build needs and that Magician does not work round (SH2_lum touches baked
    /// maps only, so it is described, never raised).
    pub fn open_gaps(&self) -> impl Iterator<Item = &Gap> {
        self.gaps.iter().filter(|g| g.id != "sh2-lum" && (g.handled.is_none() || g.handled.as_deref().is_some_and(|h| h.contains("has none"))))
    }
}

/// `magician csdk [--csdk DIR] [--json]`: the description, the game build beside it, and each gap.
pub fn cmd_csdk(args: &[String]) -> crate::R {
    let found = crate::paths::csdk(crate::flag(args, "--csdk"))?;
    if !found.path.is_dir() {
        return Err(crate::Stop::Usage(format!("{} ({}) is not a folder", found.path.display(), found.source)));
    }
    let c = describe(&found.path);
    let installed = crate::paths::game(crate::flag(args, "--game")).ok().and_then(|g| crate::game_build(&g.path).ok());
    if !crate::TEXT.load(std::sync::atomic::Ordering::Relaxed) {
        outln!("{}", to_json(&c, installed.as_deref()));
        return Ok(std::process::ExitCode::SUCCESS);
    }
    outln!("csdk\t{} ({})", c.root.display(), found.source);
    outln!("version\t{} ({}): {}", c.version.label(), c.version_from, c.version.base());
    for b in &c.bins {
        outln!("bin\t{}\t{}{}\t{}", b.name, if b.compiler { "resourcecompiler.exe" } else { "no resourcecompiler.exe" }, b.dated.as_ref().map(|d| format!(", dated {d}")).unwrap_or_default(), b.role);
    }
    match (&c.game_build, &installed) {
        (Some(a), Some(b)) if a != b => outln!("game files\tbuild {a}; the installed game is {b} (the compiler reads its own copies of Valve's files, so a material or model compiled here sees build {a}'s shaders and defaults)"),
        (Some(a), _) => outln!("game files\tbuild {a}"),
        (None, _) => outln!("game files\tno game/citadel/steam.inf"),
    }
    outln!("modeldoc\t{}", if c.modeldoc_features.is_empty() { "no features line".to_string() } else { c.modeldoc_features.join(";") });
    for g in &c.gaps {
        outln!("gap\t{}\t{}: {}{}\t{}", g.id, g.what, g.breaks, if g.seen { "" } else { " (community report, not seen here)" }, g.handled.as_deref().map(|h| format!("handled: {h}")).unwrap_or_else(|| "not handled".into()));
    }
    Ok(std::process::ExitCode::SUCCESS)
}

pub fn to_json(c: &Csdk, installed: Option<&str>) -> String {
    use crate::jsonout::Obj;
    let bins: Vec<String> = c
        .bins
        .iter()
        .map(|b| Obj::new().str("name", &b.name).str("dir", &b.dir.display().to_string()).bool("compiler", b.compiler).opt_str("dated", b.dated.as_deref()).str("role", b.role).build())
        .collect();
    let gaps: Vec<String> = c
        .gaps
        .iter()
        .map(|g| Obj::new().str("id", g.id).str("what", &g.what).str("breaks", &g.breaks).opt_str("handled", g.handled.as_deref()).bool("seen_here", g.seen).build())
        .collect();
    Obj::new()
        .str("schema", "magician.csdk/1")
        .str("root", &c.root.display().to_string())
        .str("version", c.version.id())
        .str("base", c.version.base())
        .str("version_from", &c.version_from)
        .opt_str("game_build", c.game_build.as_deref())
        .opt_str("installed_build", installed)
        .strs("modeldoc_features", &c.modeldoc_features)
        .raw("bins", crate::jsonout::arr(bins))
        .raw("gaps", crate::jsonout::arr(gaps))
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("magician-csdk-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn compiler(root: &Path, set: &str) {
        let d = root.join("game").join(set.replace('/', std::path::MAIN_SEPARATOR_STR));
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("resourcecompiler.exe"), b"MZ").unwrap();
        std::fs::write(d.join("resourcecompiler.dll"), b"MZ").unwrap();
    }

    #[test]
    fn a_csdk_13_is_known_by_its_readme_and_its_gaps_are_listed() {
        let r = tmp("13");
        std::fs::write(r.join("read_me.txt"), "Community SDK Version 13 put together by ...").unwrap();
        compiler(&r, "bin/win64");
        compiler(&r, "bin_server/win64");
        let gi = r.join("game").join("citadel");
        std::fs::create_dir_all(&gi).unwrap();
        std::fs::write(gi.join("gameinfo.gi"), "ModelDoc\n{\n\t\"features\"\t\"modelconfig;gamepreview\"\n}\n").unwrap();
        std::fs::write(gi.join("steam.inf"), "ClientVersion=6836\nServerVersion=6836\n").unwrap();
        let c = describe(&r);
        assert_eq!(c.version, Version::V13);
        assert_eq!(c.version_from, "read_me.txt names it");
        assert_eq!(c.bins.iter().map(|b| b.name.as_str()).collect::<Vec<_>>(), ["bin/win64", "bin_server/win64"]);
        assert_eq!(c.game_build.as_deref(), Some("6836"));
        let ids: Vec<&str> = c.gaps.iter().map(|g| g.id).collect();
        assert!(ids.contains(&"distancefield") && ids.contains(&"tagged-sound") && ids.contains(&"sh2-lum"), "{ids:?}");
        // without win64_nm, the tagged-sound gap is open
        assert!(c.open_gaps().any(|g| g.id == "tagged-sound"));
        compiler(&r, "bin/win64_nm");
        assert!(!describe(&r).open_gaps().any(|g| g.id == "tagged-sound"));
        let _ = std::fs::remove_dir_all(&r);
    }

    #[test]
    fn a_csdk_12_is_known_by_its_binary_sets() {
        let r = tmp("12");
        for s in ["bin/win64", "bin_tools/win64", "bin_server/win64", "bin_cs2/win64"] {
            compiler(&r, s);
        }
        let c = describe(&r);
        assert_eq!(c.version, Version::V12, "{}", c.version_from);
        assert_eq!(c.bins.len(), 4);
        assert!(c.bins.iter().find(|b| b.name == "bin_tools/win64").unwrap().role.contains("AnimGraph1"));
        assert!(c.gaps.iter().any(|g| g.id == "v12-untested" && !g.seen));
        let _ = std::fs::remove_dir_all(&r);
    }

    #[test]
    fn onedrive_folders_and_dates() {
        assert!(in_onedrive(Path::new(r"C:\Users\x\OneDrive - Work\Desktop\Reduced_CSDK_13")));
        assert!(in_onedrive(Path::new("/home/x/OneDrive/csdk")));
        assert!(!in_onedrive(Path::new(r"E:\kits\Reduced_CSDK_13")));
        assert_eq!(civil(0), "1970-01-01");
        assert_eq!(civil(20_381), "2025-10-20");
        assert_eq!(modeldoc_features("ModelDoc\n{\n\t\"models_gamedata\"\t\"x.fgd\"\n\t\"features\"\t\t\"a;distancefield\"\n}"), ["a", "distancefield"]);
        assert_eq!(client_version("x=1\nClientVersion=6875\n").as_deref(), Some("6875"));
    }
}
