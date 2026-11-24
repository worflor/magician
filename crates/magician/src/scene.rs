//! `magician scene`: ship a hero-page scene without shipping Valve's files.
//!
//! A hero-page scene is a map VPK, `maps/ui/hero_prefabs/<hero>.vpk`. A skin's scene is Valve's own compiled
//! map with a handful of files changed (the entity lump, a shadow card, maybe a showcase model), and everything
//! else byte for byte Valve's. A *scene patch* carries only the files that are ours, plus the CRCs of the Valve
//! files the scene keeps. `scene compose` builds the full VPK on the player's machine out of the patch and
//! the player's own stock map, and refuses when the stock map is not the one the patch was made against.
//!
//! A patch is a folder:
//!
//! ```text
//! scene-patch.txt      the manifest (below)
//! files/<vpk path>     our files, each at the path it has inside the scene
//! ```
//!
//! The manifest is tab-separated text, one fact per line, `#` starts a comment:
//!
//! ```text
//! magician-scene-patch	1
//! map	maps/ui/hero_prefabs/lash.vpk
//! mod	lash_showcase                          the ledger owner `scene install` records
//! built_for	6864                               the Deadlock build it was made against (informational)
//! result	37a4108b                           CRC-32 of the scene VPK the author proved: compose and install refuse another
//! keep	<path>	<valve crc>	<size>             Valve's file, kept: the player's must match
//! replace	<path>	<valve crc>	<our crc>	<size> ours replaces Valve's: the player's Valve file must match
//! add	<path>	<our crc>	<size>             ours, new to the map
//! ```
//!
//! The scene is exactly `keep` + `replace` + `add`; any other file in Valve's map is left out. Nothing of Valve's
//! is ever written into a patch (`make` only writes files whose CRC differs from Valve's at that path), and
//! nothing is written anywhere when a check fails. Output is deterministic: `vpk::write` sorts its entries.

use super::*;

pub const MANIFEST: &str = "scene-patch.txt";
const FORMAT: &str = "magician-scene-patch";
const VERSION: &str = "1";
const SCENE_DIR: &str = install::SCENE_DIR;

/// One file the manifest names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub path: String,
    /// Valve's CRC for the file at this path (`keep`, `replace`); `None` for `add`.
    pub valve_crc: Option<u32>,
    /// Our CRC (`replace`, `add`); for `keep` it is Valve's.
    pub crc: u32,
    pub size: u64,
    pub kind: Kind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Keep,
    Replace,
    Add,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Patch {
    /// `maps/ui/hero_prefabs/<hero>.vpk`
    pub map: String,
    pub hero: String,
    pub owner: String,
    pub built_for: String,
    pub result: Option<u32>,
    pub items: Vec<Item>,
}

fn is_hero(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// A path inside the scene: lowercase, `/`-separated, no `.`/`..`/empty segments, and under the hero's own
/// `maps/ui/hero_prefabs/<hero>` folder or the map file `<hero>.vmap_c` (the rule `install` enforces too). The patch is
/// downloaded content, and these paths become file reads, so this is the guard against `..\..\x`.
fn check_path(hero: &str, path: &str) -> Result<(), String> {
    let bad = |why: &str| Err(format!("path {path:?} in the patch: {why}"));
    // Valve's own map paths are lowercase ASCII; the installers accept these characters and no others (and sort
    // names the same way only on them)
    if path.is_empty() || !path.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_./-".contains(&b)) {
        return bad("must be a lowercase, forward-slash path inside the map (a-z, 0-9, _ . / -)");
    }
    if path.split('/').any(|s| s.is_empty() || s == "." || s == "..") {
        return bad("has an empty, `.` or `..` segment");
    }
    let own = format!("{SCENE_DIR}/{hero}");
    if !(path.starts_with(&format!("{own}/")) || path == format!("{own}.vmap_c")) {
        return bad(&format!("is outside {own}/ (a scene may only carry its own files)"));
    }
    Ok(())
}

/// A CRC as the manifest writes it: exactly eight hex digits (the installers read no other form either).
fn hex(s: &str, what: &str) -> Result<u32, String> {
    if s.len() != 8 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(format!("{what}: {s:?} is not a hex CRC (8 hex digits)"));
    }
    u32::from_str_radix(s, 16).map_err(|_| format!("{what}: {s:?} is not a hex CRC"))
}

pub fn parse(text: &str) -> Result<Patch, String> {
    // a BOM (an editor's) is not part of the text; the player-side installers read past one too
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut lines = text.lines().enumerate().map(|(i, l)| (i + 1, l.trim_end_matches('\r'))).filter(|(_, l)| !l.trim().is_empty() && !l.trim_start().starts_with('#'));
    match lines.next() {
        Some((_, l)) if l == format!("{FORMAT}\t{VERSION}") => {}
        Some((_, l)) if l.starts_with(FORMAT) => return Err(format!("scene patch format {:?} is not one this magician reads (it reads version {VERSION}); update magician", l.split('\t').nth(1).unwrap_or(""))),
        _ => return Err(format!("not a scene patch: {MANIFEST} must start with {FORMAT}<TAB>{VERSION}")),
    }
    let (mut map, mut owner, mut built_for, mut result) = (None, None, String::new(), None);
    let mut raw: Vec<(usize, Vec<&str>)> = Vec::new();
    for (n, l) in lines {
        let f: Vec<&str> = l.split('\t').collect();
        let want = |k: usize| if f.len() == k { Ok(()) } else { Err(format!("{MANIFEST} line {n}: {:?} takes {} fields, found {}", f[0], k - 1, f.len() - 1)) };
        match f[0] {
            "map" => { want(2)?; map = Some(f[1].to_string()) }
            "mod" => { want(2)?; owner = Some(f[1].to_string()) }
            "built_for" => { want(2)?; built_for = f[1].to_string() }
            "result" => { want(2)?; result = Some(hex(f[1], "result")?) }
            "keep" => { want(4)?; raw.push((n, f)) }
            "replace" => { want(5)?; raw.push((n, f)) }
            "add" => { want(4)?; raw.push((n, f)) }
            other => return Err(format!("{MANIFEST} line {n}: unknown field {other:?}")),
        }
    }
    let map = map.ok_or("the patch names no `map`")?;
    let owner = owner.ok_or("the patch names no `mod` (the ledger owner)")?;
    let hero = map
        .strip_prefix(&format!("{SCENE_DIR}/"))
        .and_then(|f| f.strip_suffix(".vpk"))
        .filter(|h| is_hero(h))
        .ok_or_else(|| format!("map {map:?} is not {SCENE_DIR}/<hero>.vpk"))?
        .to_string();
    let mut items = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (n, f) in raw {
        let size = |s: &str| s.parse::<u64>().map_err(|_| format!("{MANIFEST} line {n}: size {s:?} is not a number"));
        let it = match f[0] {
            "keep" => { let c = hex(f[2], "keep")?; Item { path: f[1].into(), valve_crc: Some(c), crc: c, size: size(f[3])?, kind: Kind::Keep } }
            "replace" => Item { path: f[1].into(), valve_crc: Some(hex(f[2], "replace")?), crc: hex(f[3], "replace")?, size: size(f[4])?, kind: Kind::Replace },
            _ => Item { path: f[1].into(), valve_crc: None, crc: hex(f[2], "add")?, size: size(f[3])?, kind: Kind::Add },
        };
        check_path(&hero, &it.path)?;
        if !seen.insert(it.path.clone()) {
            return Err(format!("{MANIFEST} line {n}: {} is listed twice", it.path));
        }
        items.push(it);
    }
    if !items.iter().any(|i| i.path == format!("{SCENE_DIR}/{hero}.vmap_c")) {
        return Err(format!("the patch has no {SCENE_DIR}/{hero}.vmap_c; not a scene for {hero}"));
    }
    Ok(Patch { map, hero, owner, built_for, result, items })
}

impl Patch {
    pub fn render(&self) -> String {
        let mut s = format!("# Scene patch for {}. Only our files are in files/; `keep` lines name Valve's files by CRC.\n# Build the scene with `magician scene compose` against your own Deadlock. See magician/README.md.\n{FORMAT}\t{VERSION}\nmap\t{}\nmod\t{}\n", self.map, self.map, self.owner);
        if !self.built_for.is_empty() {
            s += &format!("built_for\t{}\n", self.built_for);
        }
        if let Some(r) = self.result {
            s += &format!("result\t{r:08x}\n");
        }
        let mut items: Vec<&Item> = self.items.iter().collect();
        items.sort_by(|a, b| a.path.cmp(&b.path));
        for i in items {
            match i.kind {
                Kind::Keep => s += &format!("keep\t{}\t{:08x}\t{}\n", i.path, i.crc, i.size),
                Kind::Replace => s += &format!("replace\t{}\t{:08x}\t{:08x}\t{}\n", i.path, i.valve_crc.unwrap_or(0), i.crc, i.size),
                Kind::Add => s += &format!("add\t{}\t{:08x}\t{}\n", i.path, i.crc, i.size),
            }
        }
        s
    }
}

fn file_under(dir: &Path, rel: &str) -> PathBuf {
    rel.split('/').fold(dir.join("files"), |p, s| p.join(s))
}

/// `scene make`: split a built scene into a patch. Valve's identical files become `keep` lines (CRC only);
/// everything else is written under `files/`. Returns the patch and how many of Valve's files the scene left out.
pub fn make(scene: &vpk::Vpk, valve: &vpk::Vpk, map: &str, owner: &str, built_for: &str, scene_file_crc: Option<u32>, out: &Path) -> Result<(Patch, usize), String> {
    let hero = map.strip_prefix(&format!("{SCENE_DIR}/")).and_then(|f| f.strip_suffix(".vpk")).filter(|h| is_hero(h)).ok_or_else(|| format!("{map} is not {SCENE_DIR}/<hero>.vpk"))?;
    let mut entries: Vec<&vpk::Entry> = scene.entries.iter().collect();
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    let mut items = Vec::new();
    let mut ours: Vec<(String, Vec<u8>)> = Vec::new();
    for e in entries {
        check_path(hero, &e.path).map_err(|m| format!("the scene is not a clean map for {hero}: {m}"))?;
        let bytes = scene.read(e).map_err(|x| format!("{}: {x}", e.path))?;
        let crc = vpk::crc32(&bytes);
        let size = bytes.len() as u64;
        match valve.find(&e.path) {
            Some(v) if v.crc == crc && v.size() == size => items.push(Item { path: e.path.clone(), valve_crc: Some(crc), crc, size, kind: Kind::Keep }),
            Some(v) => {
                items.push(Item { path: e.path.clone(), valve_crc: Some(v.crc), crc, size, kind: Kind::Replace });
                ours.push((e.path.clone(), bytes));
            }
            None => {
                items.push(Item { path: e.path.clone(), valve_crc: None, crc, size, kind: Kind::Add });
                ours.push((e.path.clone(), bytes));
            }
        }
    }
    let dropped = valve.entries.iter().filter(|v| scene.find(&v.path).is_none()).count();
    let patch = Patch { map: map.to_string(), hero: hero.to_string(), owner: owner.to_string(), built_for: built_for.to_string(), result: scene_file_crc, items };
    // `--out ""` (an unset variable) is the working folder, whose `files/` the next line would delete.
    if out.as_os_str().is_empty() {
        return Err("--out names no folder".into());
    }
    // Never overwrite a folder that is not a patch of ours.
    if out.exists() && std::fs::read_dir(out).map_err(|e| e.to_string())?.next().is_some() && !out.join(MANIFEST).is_file() {
        return Err(format!("{} exists, is not empty and is not a scene patch; pick another --out", out.display()));
    }
    let _ = std::fs::remove_dir_all(out.join("files"));
    std::fs::create_dir_all(out).map_err(|e| e.to_string())?;
    for (path, bytes) in &ours {
        let dst = file_under(out, path);
        std::fs::create_dir_all(dst.parent().unwrap()).map_err(|e| e.to_string())?;
        std::fs::write(&dst, bytes).map_err(|e| format!("{}: {e}", dst.display()))?;
    }
    std::fs::write(out.join(MANIFEST), patch.render()).map_err(|e| e.to_string())?;
    Ok((patch, dropped))
}

fn cap(mut lines: Vec<String>) -> String {
    let more = lines.len().saturating_sub(8);
    lines.truncate(8);
    if more > 0 {
        lines.push(format!("  ... and {more} more"));
    }
    lines.join("\n")
}

/// Check the player's stock map against the patch and read everything the scene needs. Touches nothing on disk.
/// `Err` carries the full refusal text.
pub fn assemble(patch: &Patch, valve: &vpk::Vpk, patch_dir: &Path, game_build: &str) -> Result<Vec<(String, Vec<u8>)>, String> {
    let mut wrong: Vec<String> = Vec::new();
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    for it in &patch.items {
        match it.kind {
            Kind::Keep | Kind::Replace => {
                let want = it.valve_crc.unwrap();
                match valve.find(&it.path) {
                    None => wrong.push(format!("  missing  {} (the patch expects crc {want:08x})", it.path)),
                    Some(v) if v.crc != want => wrong.push(format!("  changed  {} (the patch expects crc {want:08x}, yours is {:08x})", it.path, v.crc)),
                    Some(v) => match valve.read(v) {
                        Err(e) => wrong.push(format!("  unreadable  {}: {e}", it.path)),
                        Ok(b) if vpk::crc32(&b) != want => wrong.push(format!("  damaged  {} (its bytes do not match its own directory entry; verify Deadlock's files in Steam)", it.path)),
                        Ok(b) => {
                            if it.kind == Kind::Keep {
                                if b.len() as u64 != it.size {
                                    wrong.push(format!("  changed  {} (size {} bytes, the patch expects {})", it.path, b.len(), it.size));
                                } else {
                                    files.push((it.path.clone(), b));
                                }
                            }
                        }
                    },
                }
            }
            Kind::Add => {
                if valve.find(&it.path).is_some() {
                    wrong.push(format!("  new in your game  {} (the patch adds it; Valve's map now has a file there)", it.path));
                }
            }
        }
    }
    if !wrong.is_empty() {
        let kept = patch.items.iter().filter(|i| i.kind != Kind::Add).count();
        return Err(format!(
            "your Deadlock's {} does not match the scene this patch was made against (patch: build {}, yours: {game_build}). {} of {kept} Valve files differ:\n{}\nA game update changed Valve's map, so this patch needs a new release from its author. Nothing was written.",
            patch.map,
            if patch.built_for.is_empty() { "unknown" } else { &patch.built_for },
            wrong.len(),
            cap(wrong)
        ));
    }
    // Our own files: they must be the bytes the manifest names, or the patch is damaged or tampered with.
    let mut broken: Vec<String> = Vec::new();
    for it in patch.items.iter().filter(|i| i.kind != Kind::Keep) {
        let p = file_under(patch_dir, &it.path);
        match std::fs::read(&p) {
            Err(e) => broken.push(format!("  {}: {e}", it.path)),
            Ok(b) if b.len() as u64 != it.size || vpk::crc32(&b) != it.crc => broken.push(format!("  {}: size/crc differ from the manifest (got {} bytes, crc {:08x}; expected {} bytes, crc {:08x})", it.path, b.len(), vpk::crc32(&b), it.size, it.crc)),
            Ok(b) => files.push((it.path.clone(), b)),
        }
    }
    if !broken.is_empty() {
        return Err(format!("the scene patch in {} is damaged (re-download it):\n{}", patch_dir.display(), cap(broken)));
    }
    Ok(files)
}

#[cfg(test)]
fn write_atomic(out: &Path, files: &[(String, Vec<u8>)]) -> Result<u32, String> {
    write_checked(out, files, None, false).map_err(|e| e.text().to_string())
}

/// Pack the scene to `out` through a temporary file. With `want` (the patch's `result`), a scene whose crc is not the
/// one its author proved is refused and nothing is written, as the player-side installers refuse it; `force` writes
/// it anyway. Returns the crc written.
fn write_checked(out: &Path, files: &[(String, Vec<u8>)], want: Option<u32>, force: bool) -> Result<u32, Stop> {
    if let Some(d) = out.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    let tmp = out.with_extension("vpk.partial");
    vpk::write(&tmp, files).map_err(|e| e.to_string())?;
    let crc = match std::fs::read(&tmp) {
        Ok(b) => vpk::crc32(&b),
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            return Err(format!("{}: {e}", tmp.display()).into());
        }
    };
    if let Some(r) = want.filter(|r| *r != crc && !force) {
        let _ = std::fs::remove_file(&tmp);
        return Err(Stop::Refused(format!(
            "the composed scene has crc {crc:08x}, not the {r:08x} its author proved: every file checked out, but the              bytes are not the scene that was tested. Nothing was written. --force writes it anyway"
        )));
    }
    std::fs::rename(&tmp, out).map_err(|e| format!("{}: {e}", out.display()))?;
    Ok(crc)
}

// ----------------------------------------------------------------------------------------------------- the command

struct Opts {
    pos: Vec<String>,
    out: Option<String>,
    owner: Option<String>,
    game: Option<String>,
    force: bool,
}

fn opts(args: &[String]) -> Result<Opts, String> {
    let mut o = Opts { pos: Vec::new(), out: None, owner: None, game: None, force: false };
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut val = |name: &str| it.next().cloned().ok_or(format!("{name} wants a value"));
        match a.as_str() {
            "--out" => o.out = Some(val("--out")?),
            "--mod" => o.owner = Some(val("--mod")?),
            "--game" => o.game = Some(val("--game")?),
            "--holder" => {
                val("--holder")?; // read by game_seat from the full argument list
            }
            "--force" => o.force = true,
            "--ignore-game" => {}
            s if s.starts_with("--") => return Err(format!("unknown option {s}")),
            _ => o.pos.push(a.clone()),
        }
    }
    Ok(o)
}

fn patch_dir(arg: &str) -> PathBuf {
    let p = PathBuf::from(arg);
    if p.is_file() { p.parent().map(Path::to_path_buf).unwrap_or_default() } else { p }
}

pub(crate) fn load_patch(dir: &Path) -> Result<Patch, String> {
    let text = std::fs::read_to_string(dir.join(MANIFEST)).map_err(|e| format!("{}: {e} (a scene patch is a folder with {MANIFEST} and files/)", dir.join(MANIFEST).display()))?;
    parse(&text)
}

pub(crate) fn stock_map(game: &Path, patch: &Patch) -> Result<vpk::Vpk, String> {
    let p = game.join("citadel").join(crate::build::native(&patch.map));
    if !p.is_file() {
        return Err(format!("{} not found: this Deadlock has no stock {} (is the game installed and up to date? In Steam: Properties > Installed Files > Verify)", p.display(), patch.map));
    }
    open(&p)
}

const SCENE_USAGE: &str = "scene make <scene.vpk> --out DIR [--mod NAME] | scene check <patch> | scene compose <patch> --out FILE | scene install <patch> [--mod NAME] [--force] [--ignore-game] [--holder NAME]  (all take --game DIR)";

pub fn cmd_scene(args: &[String]) -> R {
    let verb = args.first().map(String::as_str).ok_or(SCENE_USAGE)?;
    let o = opts(&args[1..])?;
    let arg = o.pos.first().ok_or(SCENE_USAGE)?;
    let game = paths::game(o.game.clone())?.path;
    let build = game_build(&game)?;
    match verb {
        "make" => {
            let scene_path = PathBuf::from(arg);
            let file = scene_path.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
            let map = format!("{SCENE_DIR}/{file}");
            let out = PathBuf::from(o.out.as_deref().ok_or("scene make wants --out DIR")?);
            let valve = open(&game.join("citadel").join(crate::build::native(&map))).map_err(|e| format!("Valve's stock map: {e}"))?;
            let scene = open(&scene_path)?;
            let hero = file.trim_end_matches(".vpk");
            let owner = o.owner.clone().unwrap_or_else(|| format!("{hero}_showcase"));
            let crc = std::fs::read(&scene_path).ok().map(|b| vpk::crc32(&b));
            let (p, dropped) = make(&scene, &valve, &map, &owner, &build, crc, &out)?;
            let n = |k: Kind| p.items.iter().filter(|i| i.kind == k).count();
            let bytes: u64 = p.items.iter().filter(|i| i.kind != Kind::Keep).map(|i| i.size).sum();
            outln!("scene patch for {map} (build {build}) -> {}", out.display());
            outln!("  kept {} of Valve's files by crc only; ours: {} replaced, {} added ({:.1} MB in files/)", n(Kind::Keep), n(Kind::Replace), n(Kind::Add), bytes as f64 / 1e6);
            if dropped > 0 {
                outln!("  note: {dropped} file(s) in Valve's map are not in this scene and will be left out of the composed one");
            }
            Ok(ExitCode::SUCCESS)
        }
        "check" | "compose" | "install" => {
            let dir = patch_dir(arg);
            let patch = load_patch(&dir)?;
            let valve = stock_map(&game, &patch)?;
            // A patch that no longer fits the player's game: `check` found something (exit 1); `compose` and
            // `install` refuse to build on it (exit 3). Either way nothing is written.
            let files = assemble(&patch, &valve, &dir, &build).map_err(|e| if verb == "check" { Stop::Broken(e) } else { Stop::Refused(e) })?;
            outln!(
                "build {build}: your {} matches the patch ({} Valve files verified by crc, {} of ours intact)",
                patch.map,
                patch.items.iter().filter(|i| i.kind != Kind::Add).count(),
                patch.items.iter().filter(|i| i.kind != Kind::Keep).count()
            );
            if verb == "check" {
                return Ok(ExitCode::SUCCESS);
            }
            let note_result = |crc: u32| match patch.result {
                Some(r) if r == crc => outln!("scene {} crc {crc:08x}: identical to the one the author proved", patch.map),
                Some(r) => outln!("scene {} crc {crc:08x}: NOT the {r:08x} the author proved; written because of --force", patch.map),
                None => outln!("scene {} crc {crc:08x} (the patch records no result to compare with)", patch.map),
            };
            if verb == "compose" {
                let out = PathBuf::from(o.out.as_deref().ok_or("scene compose wants --out FILE")?);
                let crc = write_checked(&out, &files, patch.result, o.force)?;
                outln!("composed {} files -> {}", files.len(), out.display());
                note_result(crc);
                return Ok(ExitCode::SUCCESS);
            }
            // install: compose into a scratch tree (route_for wants the maps/ui/hero_prefabs/<hero>.vpk path), then the
            // ordinary install: seat check, collision check against the ledger, gameinfo.gi, copy, ledger.
            game_seat(args)?;
            let scratch = std::env::temp_dir().join(format!("magician-scene-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&scratch);
            let composed = scratch.join(crate::build::native(&patch.map));
            let run = || -> R {
                let crc = write_checked(&composed, &files, patch.result, o.force)?;
                note_result(crc);
                let file = format!("{}.vpk", patch.hero);
                install::validate_scene(&composed, &file)?;
                let route = install::Route::UiScene(file);
                let owner = o.owner.clone().unwrap_or_else(|| patch.owner.clone());
                let ip = install::Paths { citadel: game.join("citadel") };
                let done = install::install_vpk(&ip, &build, &composed, &route, &owner, o.force)?;
                for n in &done.notes {
                    outln!("{n}");
                }
                outln!("installed {} for {owner} (crc {})", done.dst.display(), done.crc);
                Ok(ExitCode::SUCCESS)
            };
            let r = run();
            let _ = std::fs::remove_dir_all(&scratch);
            r
        }
        _ => Err(format!("unknown scene command {verb:?}: {SCENE_USAGE}").into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("magician-scene-test-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }
    const H: &str = "maps/ui/hero_prefabs/lash";
    fn f(name: &str, body: &str) -> (String, Vec<u8>) {
        (format!("{H}{name}"), body.as_bytes().to_vec())
    }
    /// Valve's map: a map file, a lump, a shadow card, two textures.
    fn valve_files() -> Vec<(String, Vec<u8>)> {
        vec![f(".vmap_c", "map"), f("/entities/default_ents.vents_c", "valve lump"), f("/worldnodes/card.vmdl_c", "valve card"), f("/cubemaps/cube.vtex_c", "cube"), f("/lightmaps/atlas.vtex_c", "atlas")]
    }
    /// Ours: the same map with the lump and card changed and a showcase model added.
    fn scene_files() -> Vec<(String, Vec<u8>)> {
        vec![f(".vmap_c", "map"), f("/entities/default_ents.vents_c", "OUR lump"), f("/worldnodes/card.vmdl_c", "OUR card"), f("/cubemaps/cube.vtex_c", "cube"), f("/lightmaps/atlas.vtex_c", "atlas"), f("/showcase/maid.vmdl_c", "our model")]
    }
    fn write_vpk(dir: &Path, name: &str, files: &[(String, Vec<u8>)]) -> PathBuf {
        let p = dir.join(name);
        vpk::write(&p, files).unwrap();
        p
    }

    #[test]
    fn make_then_compose_reproduces_the_scene_byte_for_byte_and_ships_no_valve_file() {
        let d = tmp("round");
        let valve = vpk::Vpk::open(write_vpk(&d, "valve.vpk", &valve_files())).unwrap();
        let scene_path = write_vpk(&d, "lash.vpk", &scene_files());
        let scene = vpk::Vpk::open(&scene_path).unwrap();
        let out = d.join("patch");
        let want = vpk::crc32(&std::fs::read(&scene_path).unwrap());
        let (p, dropped) = make(&scene, &valve, "maps/ui/hero_prefabs/lash.vpk", "lash_showcase", "6864", Some(want), &out).unwrap();
        assert_eq!(dropped, 0);
        assert_eq!(p.items.iter().filter(|i| i.kind == Kind::Keep).count(), 3);
        // only our three files are on disk, and none equals Valve's at its path
        let mut on_disk: Vec<String> = Vec::new();
        fn walk(d: &Path, root: &Path, out: &mut Vec<String>) {
            for e in std::fs::read_dir(d).unwrap().flatten() {
                if e.path().is_dir() {
                    walk(&e.path(), root, out)
                } else {
                    out.push(e.path().strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/"))
                }
            }
        }
        walk(&out.join("files"), &out.join("files"), &mut on_disk);
        on_disk.sort();
        assert_eq!(on_disk, vec![format!("{H}/entities/default_ents.vents_c"), format!("{H}/showcase/maid.vmdl_c"), format!("{H}/worldnodes/card.vmdl_c")]);
        for rel in &on_disk {
            let b = std::fs::read(file_under(&out, rel)).unwrap();
            if let Some(v) = valve.find(rel) {
                assert_ne!(vpk::crc32(&b), v.crc, "{rel}: a Valve file leaked into the patch");
            }
        }
        // the manifest round-trips, and composing gives the very same archive
        let back = load_patch(&out).unwrap();
        assert_eq!(back, p);
        let files = assemble(&back, &valve, &out, "6864").unwrap();
        let composed = d.join("composed").join("lash.vpk");
        let got = write_atomic(&composed, &files).unwrap();
        assert_eq!(got, want);
        assert_eq!(std::fs::read(&composed).unwrap(), std::fs::read(&scene_path).unwrap());
        let again = d.join("again.vpk");
        write_atomic(&again, &assemble(&back, &valve, &out, "6864").unwrap()).unwrap();
        assert_eq!(std::fs::read(&composed).unwrap(), std::fs::read(&again).unwrap(), "deterministic");
        // a second make into the same folder replaces it cleanly
        make(&scene, &valve, "maps/ui/hero_prefabs/lash.vpk", "lash_showcase", "6864", Some(want), &out).unwrap();
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_changed_missing_or_added_valve_file_refuses_with_the_files_named_and_writes_nothing() {
        let d = tmp("mismatch");
        let valve_ok = vpk::Vpk::open(write_vpk(&d, "valve.vpk", &valve_files())).unwrap();
        let scene = vpk::Vpk::open(write_vpk(&d, "lash.vpk", &scene_files())).unwrap();
        let out = d.join("patch");
        let (p, _) = make(&scene, &valve_ok, "maps/ui/hero_prefabs/lash.vpk", "m", "6864", None, &out).unwrap();
        // the game updated: the cubemap changed, the atlas is gone, the card changed, and Valve now ships our showcase path
        let mut upd = valve_files();
        upd[3].1 = b"new cube".to_vec();
        upd.remove(4);
        upd[2].1 = b"newer valve card".to_vec();
        upd.push(f("/showcase/maid.vmdl_c", "valve's own"));
        let valve_new = vpk::Vpk::open(write_vpk(&d, "valve2.vpk", &upd)).unwrap();
        let e = assemble(&p, &valve_new, &out, "6900").unwrap_err();
        for want in ["build 6864", "yours: 6900", "changed  maps/ui/hero_prefabs/lash/cubemaps/cube.vtex_c", "missing  maps/ui/hero_prefabs/lash/lightmaps/atlas.vtex_c", "worldnodes/card.vmdl_c", "new in your game", "needs a new release", "Nothing was written"] {
            assert!(e.contains(want), "{want:?} not in:\n{e}");
        }
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_scene_that_is_not_the_one_the_author_proved_is_refused_unless_forced() {
        let d = tmp("result");
        let files = scene_files();
        let out = d.join("lash.vpk");
        let crc = write_checked(&out, &files, None, false).unwrap();
        std::fs::remove_file(&out).unwrap();
        assert_eq!(write_checked(&out, &files, Some(crc), false), Ok(crc));
        std::fs::remove_file(&out).unwrap();
        let e = write_checked(&out, &files, Some(crc ^ 1), false).unwrap_err();
        assert_eq!(e.code(), crate::cli::REFUSED);
        assert!(e.contains("not the") && e.contains("Nothing was written") && e.contains("--force"), "{e}");
        assert!(!out.exists() && !out.with_extension("vpk.partial").exists(), "a refusal leaves nothing behind");
        assert_eq!(write_checked(&out, &files, Some(crc ^ 1), true), Ok(crc));
        assert!(out.is_file());
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_damaged_patch_file_is_refused() {
        let d = tmp("damaged");
        let valve = vpk::Vpk::open(write_vpk(&d, "valve.vpk", &valve_files())).unwrap();
        let scene = vpk::Vpk::open(write_vpk(&d, "lash.vpk", &scene_files())).unwrap();
        let out = d.join("patch");
        let (p, _) = make(&scene, &valve, "maps/ui/hero_prefabs/lash.vpk", "m", "6864", None, &out).unwrap();
        std::fs::write(file_under(&out, &format!("{H}/showcase/maid.vmdl_c")), b"tampered").unwrap();
        let e = assemble(&p, &valve, &out, "6864").unwrap_err();
        assert!(e.contains("damaged") && e.contains("showcase/maid.vmdl_c"), "{e}");
        std::fs::remove_file(file_under(&out, &format!("{H}/worldnodes/card.vmdl_c"))).unwrap();
        assert!(assemble(&p, &valve, &out, "6864").unwrap_err().contains("card.vmdl_c"));
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn the_manifest_is_strict() {
        let head = "magician-scene-patch\t1\nmap\tmaps/ui/hero_prefabs/lash.vpk\nmod\tx\n";
        let ok = format!("{head}keep\t{H}.vmap_c\tdeadbeef\t10\n");
        assert!(parse(&ok).is_ok());
        let bad = |body: &str| parse(&format!("{head}{body}")).unwrap_err();
        assert!(bad("keep\tmaps/ui/hero_prefabs/lash/../../../x\t00000001\t1\n").contains("`..`"));
        assert!(bad("add\t../evil\t00000001\t1\n").contains("`..`"));
        assert!(bad("add\tmaps/ui/hero_prefabs/lash/a\\b\t00000001\t1\n").contains("forward-slash"));
        assert!(bad("add\tC:/x\t00000001\t1\n").contains("forward-slash"));
        assert!(bad("add\tmaps/ui/hero_prefabs/abrams/a.vtex_c\t00000001\t1\n").contains("outside"));
        assert!(bad("add\tmaps/ui/hero_prefabs/lash/A.vtex_c\t00000001\t1\n").contains("lowercase"));
        assert!(bad("keep\tx\n").contains("takes"));
        assert!(bad("frob\tx\n").contains("unknown field"));
        assert!(parse(&format!("{ok}keep\t{H}.vmap_c\t00000001\t1\n")).unwrap_err().contains("twice"));
        assert!(parse("magician-scene-patch\t2\n").unwrap_err().contains("update magician"));
        assert!(parse("hello").unwrap_err().contains("not a scene patch"));
        assert!(parse(&format!("{head}add\t{H}/a\t00000001\t1\n")).unwrap_err().contains("vmap_c"));
        assert!(parse("magician-scene-patch\t1\nmap\tmaps/dl_midtown.vpk\nmod\tx\n").unwrap_err().contains("is not maps/ui/hero_prefabs"));
    }

    #[test]
    fn make_refuses_a_folder_that_is_not_a_patch() {
        let d = tmp("notpatch");
        let valve = vpk::Vpk::open(write_vpk(&d, "valve.vpk", &valve_files())).unwrap();
        let scene = vpk::Vpk::open(write_vpk(&d, "lash.vpk", &scene_files())).unwrap();
        let out = d.join("precious");
        std::fs::create_dir_all(&out).unwrap();
        std::fs::write(out.join("notes.txt"), "keep me").unwrap();
        assert!(make(&scene, &valve, "maps/ui/hero_prefabs/lash.vpk", "m", "1", None, &out).unwrap_err().contains("not a scene patch"));
        assert!(out.join("notes.txt").is_file());
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn the_manifest_reads_as_the_player_side_installers_read_it() {
        let ok = "magician-scene-patch\t1\nmap\tmaps/ui/hero_prefabs/lash.vpk\nmod\tm\nkeep\tmaps/ui/hero_prefabs/lash.vmap_c\t0a1b2c3d\t3\n";
        assert!(parse(ok).is_ok());
        // a BOM in front is read past, as scene_install.py and .ps1 do
        assert_eq!(parse(&format!("\u{feff}{ok}")), parse(ok));
        // a crc is eight hex digits: what the installers compare, and what `make` writes
        for bad in ["a1b2", "+a1b2c3d", "0x1b2c3d", "0a1b2c3dd"] {
            let e = parse(&ok.replace("0a1b2c3d", bad)).unwrap_err();
            assert!(e.contains("8 hex digits"), "{bad}: {e}");
        }
        // a path is lowercase ASCII, which the installers accept and sort alike
        for bad in ["maps/ui/hero_prefabs/lash/\u{e9}.vtex_c", "maps/ui/hero_prefabs/lash/a b.vtex_c", "maps/ui/hero_prefabs/lash/a+b.vtex_c"] {
            let e = parse(&format!("{ok}add\t{bad}\t0a1b2c3d\t3\n")).unwrap_err();
            assert!(e.contains("a-z, 0-9"), "{bad}: {e}");
        }
        assert!(parse(&format!("{ok}add\tmaps/ui/hero_prefabs/lash/fx-2/x_1.vtex_c\t0a1b2c3d\t3\n")).is_ok());
    }
}
