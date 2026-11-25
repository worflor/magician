//! Installing a mod into the game, the only sanctioned write into the Deadlock install.
//!
//! Two things happen:
//! 1. `gameinfo.gi` gets a `Game citadel/addons` search path just above `Game citadel`. Steam updates
//!    and "verify files" overwrite gameinfo.gi, so the edit is idempotent and re-run after every
//!    build change. The pristine file is backed up once per game build before the first edit.
//! 2. The VPK is copied into `game/citadel/addons/`. A ledger (`addons/.magician-installed.tsv`)
//!    records what we installed (file, crc, and the mod it was installed for), so we never overwrite or
//!    delete a file someone else put there, and never replace one mod's install with another mod's file of
//!    the same name without being told to (`--force`); `--as pakNN_dir.vpk` installs under another slot.
//!
//! Two routes: a `pakNN_dir.vpk` goes to `addons/`; a UI scene map `maps/ui/hero_prefabs/<hero>.vpk` goes to
//! `addons/maps/ui/hero_prefabs/`. Nothing else under `maps/` is installed.
//!
//! Every install and uninstall changes the ledger under a lock (`LEDGER_LOCK`), and every file it writes goes through
//! a temporary file and a rename. Other tools rearrange `addons/`: Deadlock Mod Manager renumbers every pak to make
//! room for its own. So a ledger row whose file is gone or changed follows its crc to the pak that now holds those
//! bytes (`reconcile`), and nothing is removed whose bytes are not the ones Magician installed.

use crate::cli::Stop;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const ADDONS_LINE: &str = "Game\t\t\t\tcitadel/addons";

/// Insert `Game citadel/addons` directly above the first `Game citadel` line inside SearchPaths.
/// Keep the stock MOD and DEFAULT_WRITE_PATH roots when the addon becomes the first Game path.
/// `repair_legacy` is only used for a mount previously written by Magician.
pub fn add_addons_search_path(gameinfo: &str) -> Result<Option<String>, String> {
    configure_addons_search_path(gameinfo, false)
}

pub fn configure_addons_search_path(gameinfo: &str, repair_legacy: bool) -> Result<Option<String>, String> {
    let (sp, end) = search_paths_block(gameinfo)?;
    let lines: Vec<&str> = gameinfo[sp..end].split_inclusive('\n').collect();
    let path_key = |key: &str, path: &str| lines.iter().any(|line| {
        let mut fields = line.split_whitespace().map(|s| s.trim_matches('"'));
        fields.next().is_some_and(|s| s.eq_ignore_ascii_case(key))
            && fields.next().is_some_and(|s| s.eq_ignore_ascii_case(path))
    });
    let explicit_key = |key: &str| lines.iter().any(|line| {
        line.split_whitespace().next().is_some_and(|s| s.trim_matches('"').eq_ignore_ascii_case(key))
    });
    let has = path_key("Game", "citadel/addons");
    if has && (!repair_legacy || !lines.iter().any(|line| line.trim() == ADDONS_LINE)) {
        return Ok(None);
    }
    let crlf = gameinfo.contains("\r\n");
    let nl = if crlf { "\r\n" } else { "\n" };
    let mut offset = sp;
    let mut first_game = None;
    for &line in &lines {
        let t: Vec<&str> = line.split_whitespace().collect();
        if t.len() >= 2 && t[0].trim_matches('"').eq_ignore_ascii_case("Game") {
            let path = t[1].trim_matches('"');
            first_game.get_or_insert(path);
            let target = if has { "citadel/addons" } else { "citadel" };
            if !path.eq_ignore_ascii_case(target) {
                offset += line.len();
                continue;
            }
            let indent: String = line.chars().take_while(|c| *c == ' ' || *c == '\t').collect();
            let mut entries = String::new();
            if first_game.is_some_and(|p| p.eq_ignore_ascii_case(target)) {
                for key in ["Mod", "Write"] {
                    if !explicit_key(key) {
                        entries.push_str(&format!("{indent}{key}\t\t\t\tcitadel{nl}"));
                    }
                }
            }
            if !has {
                entries.push_str(&format!("{indent}{ADDONS_LINE}{nl}"));
            }
            if entries.is_empty() {
                return Ok(None);
            }
            let mut out = String::with_capacity(gameinfo.len() + entries.len());
            out.push_str(&gameinfo[..offset]);
            out.push_str(&entries);
            out.push_str(&gameinfo[offset..]);
            return Ok(Some(out));
        }
        offset += line.len();
    }
    Err("no `Game citadel` line inside SearchPaths".into())
}

/// Where the `SearchPaths` block is in a KeyValues text: the offset of its key and of the `}` that closes it. The key
/// counts only as a key (not inside a `//` comment or a quoted value), and its block ends at the brace that matches its
/// `{`, so a nested block, or a `}` in a comment or a string inside it, does not end it early.
pub fn search_paths_block(text: &str) -> Result<(usize, usize), String> {
    let b = text.as_bytes();
    let (mut i, mut depth) = (0usize, 0usize);
    // the key's offset and the depth inside its block, once found
    let mut key: Option<(usize, usize)> = None;
    // the last token before a `{`: its offset (the quote, if quoted), its text and its depth
    let mut last: Option<(usize, &str, usize)> = None;
    while i < b.len() {
        match b[i] {
            b'/' if b.get(i + 1) == Some(&b'/') => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            }
            b'"' => {
                let at = i;
                i += 1;
                let start = i;
                while i < b.len() && b[i] != b'"' {
                    i += 1;
                }
                last = text.get(start..i).map(|t| (at, t, depth));
                i += 1;
            }
            b'{' => {
                if let Some((at, _, _)) = last.filter(|&(_, t, d)| key.is_none() && d == depth && t.eq_ignore_ascii_case("SearchPaths")) {
                    key = Some((at, depth + 1));
                }
                depth += 1;
                last = None;
                i += 1;
            }
            b'}' => {
                if let Some((at, inside)) = key {
                    if depth == inside {
                        return Ok((at, i));
                    }
                }
                depth = depth.saturating_sub(1);
                last = None;
                i += 1;
            }
            c if c.is_ascii_whitespace() => i += 1,
            _ => {
                let start = i;
                // a bare token ends at white space, a brace, a quote or a comment
                let ends = |i: usize| i >= b.len() || b[i].is_ascii_whitespace() || matches!(b[i], b'{' | b'}' | b'"') || (b[i] == b'/' && b.get(i + 1) == Some(&b'/'));
                while !ends(i) {
                    i += 1;
                }
                last = text.get(start..i).map(|t| (start, t, depth));
            }
        }
    }
    Err(if key.is_some() { "unclosed SearchPaths block in gameinfo.gi".into() } else { "gameinfo.gi has no SearchPaths block".into() })
}

pub struct Paths {
    pub citadel: PathBuf,
}

impl Paths {
    pub fn gameinfo(&self) -> PathBuf {
        self.citadel.join("gameinfo.gi")
    }
    pub fn addons(&self) -> PathBuf {
        self.citadel.join("addons")
    }
    pub fn ledger(&self) -> PathBuf {
        self.addons().join(".magician-installed.tsv")
    }
    pub fn backup(&self, build: &str) -> PathBuf {
        self.citadel.join(format!("gameinfo.gi.magician-backup-{build}"))
    }
}

/// One installed file: its name relative to `addons/` (a `pakNN_dir.vpk`, or `maps/ui/hero_prefabs/<hero>.vpk`),
/// the crc of what we copied, and the mod it was installed for ("?" for rows written before owners were kept).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Installed {
    pub name: String,
    pub crc: String,
    pub owner: String,
}

pub const UNKNOWN_OWNER: &str = "?";

pub fn read_installed(p: &Path) -> Vec<Installed> {
    parse_installed(&std::fs::read_to_string(p).unwrap_or_default())
}

/// The ledger's rows from its text: name, crc, owner, tab-separated.
pub fn parse_installed(text: &str) -> Vec<Installed> {
    text.lines()
        .filter_map(|l| {
            let mut f = l.split('\t');
            let name = f.next().filter(|n| !n.is_empty())?;
            let crc = f.next()?;
            Some(Installed { name: name.into(), crc: crc.into(), owner: f.next().filter(|o| !o.is_empty()).unwrap_or(UNKNOWN_OWNER).into() })
        })
        .collect()
}

pub fn write_installed(p: &Path, rows: &[Installed]) -> std::io::Result<()> {
    let mut s = String::new();
    for r in rows {
        s.push_str(&format!("{}\t{}\t{}\n", r.name, r.crc, r.owner));
    }
    write_atomic(p, s.as_bytes())
}

/// The ledger's lock file, in `addons/`, held for one install's or uninstall's read, change and rewrite of the ledger.
pub const LEDGER_LOCK: &str = ".magician-installed.lock";

/// Take the ledger for one read-modify-write; two installs at once would each rewrite it from what they read and
/// lose the other's row.
fn lock_ledger(ip: &Paths) -> Result<crate::lock::BuildLock, Stop> {
    let opt = crate::lock::Options { no_wait: false, timeout: Duration::from_secs(120), poll: Duration::from_millis(25), say_every: Duration::from_secs(10) };
    crate::lock::acquire_file(&ip.addons(), LEDGER_LOCK, "magician install", &opt, &mut |m| say!("{m}"))
}

/// Write through a temporary file beside `path` and a rename: a kill or a full disk mid-write leaves the old file or
/// the new one, never half of either. The temporary name is not a `pakNN_dir.vpk`, so the game never loads one.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = path.with_file_name(format!("{name}.magician-partial"));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

/// A name Magician installs into `addons/`: `pakNN_dir.vpk`, or `maps/ui/hero_prefabs/<hero>.vpk`. A ledger row
/// naming anything else (`..\gameinfo.gi`, an absolute path) is never acted on.
pub fn is_install_name(n: &str) -> bool {
    is_pak_name(n) || n.strip_prefix(&format!("{SCENE_DIR}/")).and_then(|f| f.strip_suffix(".vpk")).is_some_and(|h| !h.is_empty() && h.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'))
}

fn crc_of(p: &Path) -> Option<String> {
    std::fs::read(p).ok().map(|b| format!("{:08x}", vpk::crc32(&b)))
}

/// Re-point rows whose file is missing or holds other bytes at the one pak in `addons` that holds the row's bytes and
/// that no other row accounts for: the file another tool renamed. Returns (old name, new name) for each row moved.
/// Scene maps are left alone (mod managers don't touch `addons/maps/`), and a row with no single match stays as it
/// is (missing, or changed: it is reported, never guessed at).
pub fn reconcile(ledger: &mut [Installed], addons: &Path) -> Vec<(String, String)> {
    let mut crcs: HashMap<String, Option<String>> = HashMap::new();
    let mut crc = |n: &str| crcs.entry(n.to_string()).or_insert_with(|| crc_of(&addons.join(n))).clone();
    let stale: Vec<usize> = (0..ledger.len()).filter(|&i| is_pak_name(&ledger[i].name) && crc(&ledger[i].name).as_deref() != Some(ledger[i].crc.as_str())).collect();
    if stale.is_empty() {
        return Vec::new();
    }
    let mut claimed: Vec<String> = (0..ledger.len()).filter(|i| !stale.contains(i)).map(|i| ledger[i].name.clone()).collect();
    let mut files: Vec<String> = std::fs::read_dir(addons)
        .map(|rd| rd.filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().into_owned())).filter(|n| is_pak_name(n)).collect())
        .unwrap_or_default();
    files.sort();
    let mut moved = Vec::new();
    for i in stale {
        let matches: Vec<String> = files.iter().filter(|f| !claimed.contains(f) && crc(f).as_deref() == Some(ledger[i].crc.as_str())).cloned().collect();
        if let [new] = matches.as_slice() {
            moved.push((ledger[i].name.clone(), new.clone()));
            ledger[i].name = new.clone();
            claimed.push(new.clone());
        }
    }
    moved
}

/// A mod name goes into the ledger as one tab-separated column: no tabs, newlines or other control characters.
fn check_owner(owner: &str) -> Result<(), Stop> {
    if owner.is_empty() || owner.chars().any(char::is_control) {
        return Err(Stop::Usage(format!("--mod {owner:?}: a mod name is one line of text, no tabs")));
    }
    Ok(())
}

/// `pakNN_dir.vpk` with NN in 01..99, lowercase.
pub fn is_pak_name(n: &str) -> bool {
    let b = n.as_bytes();
    n.len() == 13 && n.starts_with("pak") && n.ends_with("_dir.vpk") && b[3].is_ascii_digit() && b[4].is_ascii_digit() && &n[3..5] != "00"
}

pub const SCENE_DIR: &str = "maps/ui/hero_prefabs";

/// Where an install goes, relative to `addons/`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Route {
    Pak(String),
    UiScene(String),
}

impl Route {
    pub fn rel(&self) -> String {
        match self {
            Route::Pak(n) => n.clone(),
            Route::UiScene(n) => format!("{SCENE_DIR}/{n}"),
        }
    }
}

fn normal_parts(p: &Path, lower: bool) -> Vec<String> {
    p.components()
        .filter_map(|c| match c {
            std::path::Component::Normal(s) => Some(if lower { s.to_string_lossy().to_ascii_lowercase() } else { s.to_string_lossy().into_owned() }),
            _ => None,
        })
        .collect()
}

/// Choose the route for a source file. A `pakNN_dir.vpk` is a pak (`--as` renames it); a `<hero>.vpk` that sits under
/// `maps/ui/hero_prefabs/` is a UI scene map; anything else under `maps/`, or any other name, is refused.
pub fn route_for(src: &Path, as_name: Option<&str>) -> Result<Route, String> {
    let parts = normal_parts(src, true);
    let file = parts.last().ok_or("bad path")?.clone();
    let dirs = &parts[..parts.len() - 1];
    let in_scene_dir = dirs.len() >= 3 && dirs[dirs.len() - 3..] == ["maps", "ui", "hero_prefabs"];
    if in_scene_dir && !is_pak_name(&file) {
        if as_name.is_some() {
            return Err("--as renames a pakNN_dir.vpk; a UI scene map keeps its hero's name".into());
        }
        let ok = file.strip_suffix(".vpk").is_some_and(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'));
        return if ok { Ok(Route::UiScene(file)) } else { Err(format!("{file}: a UI scene map is <hero>.vpk (lowercase letters, digits, underscores)")) };
    }
    if !is_pak_name(&file) {
        if let Some(i) = dirs.iter().rposition(|d| d == "maps") {
            return Err(format!(
                "refusing {}: under maps/ only maps/ui/hero_prefabs/<hero>.vpk installs (this is maps/{})",
                src.display(),
                parts[i + 1..].join("/")
            ));
        }
    }
    if !is_pak_name(&file) && !as_name.is_some_and(is_pak_name) {
        return Err(format!("{file}: not a pakNN_dir.vpk (NN 01..99) and not maps/ui/hero_prefabs/<hero>.vpk; rename it, or pass --as pakNN_dir.vpk"));
    }
    match as_name {
        None => Ok(Route::Pak(file)),
        Some(a) if is_pak_name(a) => Ok(Route::Pak(a.to_string())),
        Some(a) => Err(format!("--as {a}: wants pakNN_dir.vpk with NN in 01..99, lowercase")),
    }
}

/// The mod an install is for: `--mod`, else the folder the file sits in (`release\maid_lash\pak10_dir.vpk` is
/// `maid_lash`; for a scene map, the folder above `maps`).
pub fn owner_for(src: &Path, explicit: Option<&str>) -> String {
    if let Some(m) = explicit.map(str::trim).filter(|m| !m.is_empty()) {
        return m.to_string();
    }
    let parts = normal_parts(src, false);
    let dirs = &parts[..parts.len().saturating_sub(1)];
    let pick = match dirs.iter().rposition(|d| d.eq_ignore_ascii_case("maps")) {
        Some(i) if i > 0 && dirs.len() - i == 3 => dirs.get(i - 1),
        _ => dirs.last(),
    };
    pick.cloned().unwrap_or_else(|| UNKNOWN_OWNER.into())
}

/// What the slot already holds.
#[derive(Debug, PartialEq, Eq)]
pub enum Slot {
    /// Nothing there.
    Free,
    /// Our install of the same bytes.
    Same,
    /// Our install, for this mod, older content: replace it.
    Update,
    /// Another mod's install (or one whose owner we don't know) with different content.
    OtherMod { owner: String, crc: String },
    /// A file we didn't install, or one that changed since we did.
    Foreign { why: String },
}

/// Decide what installing `new_crc` for `owner` into `rel` would replace. `on_disk` is the crc of the file there now.
pub fn check_slot(ledger: &[Installed], rel: &str, new_crc: &str, owner: &str, on_disk: Option<&str>) -> Slot {
    let Some(disk) = on_disk else { return Slot::Free };
    let Some(row) = ledger.iter().find(|r| r.name == rel) else {
        return Slot::Foreign { why: "it wasn't installed by magician".into() };
    };
    if row.crc != disk {
        return Slot::Foreign { why: format!("it changed since magician installed it (ledger crc {}, file crc {disk})", row.crc) };
    }
    if disk == new_crc {
        return Slot::Same;
    }
    if row.owner != UNKNOWN_OWNER && row.owner == owner {
        return Slot::Update;
    }
    Slot::OtherMod { owner: row.owner.clone(), crc: row.crc.clone() }
}

/// The first `pakNN_dir.vpk` from `from` up that neither the ledger nor the folder uses.
pub fn free_slot(ledger: &[Installed], addons: &Path, from: u32) -> Option<String> {
    (from.max(1)..=99).map(|n| format!("pak{n:02}_dir.vpk")).find(|n| !ledger.iter().any(|r| &r.name == n) && !addons.join(n).exists())
}

/// A UI scene map must be a readable archive holding the hero's `<hero>.vmap_c` and nothing outside
/// `maps/ui/hero_prefabs/<hero>` (a scene that carried other paths would replace files it does not own).
pub fn validate_scene(path: &Path, file: &str) -> Result<(), String> {
    let stem = file.trim_end_matches(".vpk");
    let v = vpk::Vpk::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let own = format!("{SCENE_DIR}/{stem}");
    if v.find(&format!("{own}.vmap_c")).is_none() {
        return Err(format!("{}: no {own}.vmap_c inside; not a UI scene map for {stem}", path.display()));
    }
    let strays: Vec<&str> = v.entries.iter().map(|e| e.path.as_str()).filter(|p| !(p.starts_with(&format!("{own}/")) || p.starts_with(&format!("{own}.")))).collect();
    if !strays.is_empty() {
        return Err(format!("{}: {} file(s) outside {own} (first: {}); a scene map installs only its own paths", path.display(), strays.len(), strays[0]));
    }
    Ok(())
}

pub struct Done {
    pub dst: PathBuf,
    pub crc: String,
    pub notes: Vec<String>,
}

/// The collision check first (nothing is touched when it refuses), then the `gameinfo.gi` edit with its per-build
/// backup, then the copy and the ledger. `force` overrides a refusal and says so.
/// A slot that belongs to another mod, or a file Magician did not install, is a refusal (`Stop::Refused`, exit 3).
pub fn install_vpk(ip: &Paths, build: &str, src: &Path, route: &Route, owner: &str, force: bool) -> Result<Done, Stop> {
    check_owner(owner)?;
    let mut notes = Vec::new();
    let rel = route.rel();
    let bytes = std::fs::read(src).map_err(|e| format!("{}: {e}", src.display()))?;
    let new_crc = format!("{:08x}", vpk::crc32(&bytes));
    let dst = ip.addons().join(&rel);
    let _held = lock_ledger(ip)?;
    let mut ledger = read_installed(&ip.ledger());
    let moved = reconcile(&mut ledger, &ip.addons());
    for (old, new) in &moved {
        notes.push(format!("ledger: {old} is now {new} (the same bytes, renamed by another tool)"));
    }
    // The mod's pak was renamed under the ledger: a second copy under the old name would load beside it.
    if let Some(row) = moved.iter().filter_map(|(_, new)| ledger.iter().find(|r| &r.name == new)).find(|r| r.owner == owner && r.name != rel && matches!(route, Route::Pak(_))) {
        if row.crc == new_crc {
            write_installed(&ip.ledger(), &ledger).map_err(|e| e.to_string())?;
            notes.push(format!("{owner} is already installed as {} with these exact bytes; nothing copied", row.name));
            return Ok(Done { dst: ip.addons().join(&row.name), crc: new_crc, notes });
        }
        let msg = format!("{owner} is installed as {} (another tool renamed it from the ledger's name); a second copy as {rel} would load beside it, and the lower number wins", row.name);
        if !force {
            return Err(Stop::Refused(format!("{msg}. Update it in place with --as {}, or uninstall --mod {owner} first (--force installs the second copy)", row.name)));
        }
        notes.push(format!("WARNING: --force: {msg}"));
    }
    let on_disk = if dst.exists() {
        // a file we can't read can't be checked, so it is not overwritten
        Some(crc_of(&dst).ok_or_else(|| format!("{} exists but can't be read to check whose it is; not overwriting it", dst.display()))?)
    } else {
        None
    };
    match check_slot(&ledger, &rel, &new_crc, owner, on_disk.as_deref()) {
        Slot::Free | Slot::Update => {}
        Slot::Same => notes.push(format!("{rel} already holds this exact file")),
        Slot::OtherMod { owner: other, crc } => {
            let hint = match route {
                Route::Pak(n) => {
                    let from = n[3..5].parse::<u32>().unwrap_or(0) + 1;
                    free_slot(&ledger, &ip.addons(), from).map(|f| format!("; install under a free slot with --as {f}")).unwrap_or_default()
                }
                Route::UiScene(_) => "; a scene map has one slot per hero".into(),
            };
            let msg = format!("{rel} is installed for another mod ({other}, crc {crc}); this is {owner} (crc {new_crc}){hint}");
            if !force {
                return Err(Stop::Refused(format!("{msg}. Not overwriting (--force replaces it)")));
            }
            notes.push(format!("WARNING: --force replaced another mod's install: {msg}"));
        }
        Slot::Foreign { why } => {
            if !force {
                return Err(Stop::Refused(format!("{} exists and {why}; pick another slot (--as pakNN_dir.vpk) or --force", dst.display())));
            }
            notes.push(format!("WARNING: --force replaced {} ({why})", dst.display()));
        }
    }

    let gi = ip.gameinfo();
    let gi_text = std::fs::read_to_string(&gi).map_err(|e| format!("{}: {e}", gi.display()))?;
    let repair_legacy = ip.backup(build).is_file() && !ledger.is_empty();
    if let Some(new) = configure_addons_search_path(&gi_text, repair_legacy)? {
        let bak = ip.backup(build);
        if !bak.exists() {
            write_atomic(&bak, gi_text.as_bytes()).map_err(|e| e.to_string())?;
            notes.push(format!("backed up gameinfo.gi -> {}", bak.display()));
        }
        write_atomic(&gi, new.as_bytes()).map_err(|e| e.to_string())?;
        notes.push("gameinfo.gi: configured the addons mount and preserved the stock config roots".into());
    } else {
        notes.push("gameinfo.gi: addons search path already present".into());
    }

    if let Some(d) = dst.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    write_atomic(&dst, &bytes).map_err(|e| format!("copy to {}: {e}", dst.display()))?;
    ledger.retain(|r| r.name != rel);
    ledger.push(Installed { name: rel, crc: new_crc.clone(), owner: owner.to_string() });
    write_installed(&ip.ledger(), &ledger).map_err(|e| e.to_string())?;
    Ok(Done { dst, crc: new_crc, notes })
}

/// Remove an install we made, by its ledger name (or the name it had before another tool renamed it). Returns the path
/// removed. Refuses (exit 3, nothing changed) a name the ledger doesn't hold, and a file whose bytes are no longer
/// the ones Magician installed unless `force`.
pub fn uninstall(ip: &Paths, name: &str, force: bool) -> Result<PathBuf, Stop> {
    let name = name.replace('\\', "/");
    remove_rows(ip, force, &mut |r, moved_from| r.name == name || moved_from == Some(name.as_str()), &format!("{name} isn't in magician's ledger; not touching it"))
        .map(|mut v| v.remove(0))
}

/// Remove everything the ledger holds (or only one mod's installs with `owner`): the rollback. Files that aren't in
/// the ledger are never touched. Returns the paths removed. `gameinfo.gi` is left alone: with no addon paks the
/// `Game citadel/addons` mount is an empty folder and costs nothing; restoring it is the runbook's optional last step.
pub fn uninstall_matching(ip: &Paths, owner: Option<&str>, force: bool) -> Result<Vec<PathBuf>, Stop> {
    match remove_rows(ip, force, &mut |r, _| owner.is_none_or(|o| r.owner == o), "") {
        Err(Stop::Refused(e)) if e.is_empty() => Ok(Vec::new()),
        other => other,
    }
}

/// Remove the rows `pick` chooses, all or none: every one is checked (an install name, its bytes still ours) before
/// any file goes. `none` is the refusal when nothing matches (empty: no refusal, an empty list).
fn remove_rows(ip: &Paths, force: bool, pick: &mut dyn FnMut(&Installed, Option<&str>) -> bool, none: &str) -> Result<Vec<PathBuf>, Stop> {
    let _held = lock_ledger(ip)?;
    let mut ledger = read_installed(&ip.ledger());
    let moved = reconcile(&mut ledger, &ip.addons());
    let mut chosen: Vec<usize> = (0..ledger.len()).filter(|&i| {
        let from = moved.iter().find(|(_, new)| *new == ledger[i].name).map(|(old, _)| old.as_str());
        pick(&ledger[i], from)
    }).collect();
    // a rollback passes over a row that names no install (a hand-edited or damaged ledger) and leaves it be
    if none.is_empty() {
        chosen.retain(|&i| {
            let ok = is_install_name(&ledger[i].name);
            if !ok {
                say!("skipping ledger row {:?}: not a file magician installs into addons, so not acted on", ledger[i].name);
            }
            ok
        });
    }
    if chosen.is_empty() {
        return Err(Stop::Refused(none.to_string()));
    }
    let mut problems = Vec::new();
    for &i in &chosen {
        let r = &ledger[i];
        if !is_install_name(&r.name) {
            problems.push(format!("ledger row {:?} is not a file magician installs into addons", r.name));
            continue;
        }
        let dst = ip.addons().join(&r.name);
        if dst.exists() && !force {
            match crc_of(&dst) {
                Some(c) if c == r.crc => {}
                Some(c) => problems.push(format!("{} changed since magician installed it (ledger crc {}, file crc {c}): another tool's file now, or edited", r.name, r.crc)),
                None => problems.push(format!("{} can't be read to check it is still magician's", r.name)),
            }
        }
    }
    if !problems.is_empty() {
        return Err(Stop::Refused(format!("not removing anything: {} (--force removes a changed file anyway; a row that is not an install name is never acted on)", problems.join("; "))));
    }
    let mut removed = Vec::new();
    for &i in &chosen {
        let dst = ip.addons().join(&ledger[i].name);
        if dst.exists() {
            std::fs::remove_file(&dst).map_err(|e| format!("{}: {e}", dst.display()))?;
        }
        removed.push(dst);
    }
    let mut i = 0;
    ledger.retain(|_| {
        let keep = !chosen.contains(&i);
        i += 1;
        keep
    });
    write_installed(&ip.ledger(), &ledger).map_err(|e| e.to_string())?;
    Ok(removed)
}

/// The `"holder"` of a gamelock.py lock file (a flat JSON object). None when absent or unreadable.
pub fn lock_holder(lock_json: &str) -> Option<String> {
    let k = lock_json.find("\"holder\"")?;
    let rest = &lock_json[k + 8..];
    let rest = rest[rest.find(':')? + 1..].trim_start();
    let rest = rest.strip_prefix('"')?;
    let mut out = String::new();
    let mut chars = rest.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return if out.is_empty() { None } else { Some(out) },
            '\\' => out.push(chars.next()?),
            c => out.push(c),
        }
    }
    None
}

/// May this caller change the installed paks right now? Swapping a pak under a running game changes what
/// another pipeline is capturing (or the match someone is playing). Allowed when the game is closed, or when the
/// caller (`MAGICIAN_GAME_HOLDER` / `--holder`) is the pipeline holding the game lock (e.g. removing its own probe pak).
pub fn seat_check(game_running: bool, lock_json: Option<&str>, me: Option<&str>) -> Result<(), String> {
    if !game_running {
        return Ok(());
    }
    match (lock_json.and_then(lock_holder), me) {
        (Some(h), Some(m)) if h == m => Ok(()),
        (Some(h), _) => Err(format!(
            "the game is running and held by '{h}' (game lock): not changing installed paks under it. Wait until \
             it stops the game, or pass --ignore-game if you really mean to")),
        (None, _) => Err("the game is running with no pipeline holding the game lock (someone is playing): not \
             changing installed paks under it. Close the game first, or pass --ignore-game".into()),
    }
}

/// The same check when this machine could not tell whether the game runs (`why` says what failed): allowed only to
/// the caller holding the game lock, since a closed game can't be told from a running one.
pub fn seat_check_unknown(why: &str, lock_json: Option<&str>, me: Option<&str>) -> Result<(), String> {
    match (lock_json.and_then(lock_holder), me) {
        (Some(h), Some(m)) if h == m => Ok(()),
        _ => Err(format!(
            "can't tell whether Deadlock is running ({why}), so not changing installed paks. Make sure the game is              closed, then pass --ignore-game")),
    }
}

/// Whether the game runs on this machine: `tasklist` on Windows, `/proc` on Linux (the game runs under Proton there, as
/// `deadlock.exe`, or as `project8`, its old internal name). `Err` says why it could not be told.
pub fn game_running() -> Result<bool, String> {
    if cfg!(windows) {
        let o = std::process::Command::new("tasklist")
            .args(["/FI", "IMAGENAME eq deadlock.exe", "/NH"])
            .output()
            .map_err(|e| format!("tasklist did not run: {e}"))?;
        if !o.status.success() {
            return Err(format!("tasklist failed ({})", o.status));
        }
        return Ok(String::from_utf8_lossy(&o.stdout).to_ascii_lowercase().contains("deadlock.exe"));
    }
    if cfg!(target_os = "linux") {
        let dir = std::fs::read_dir("/proc").map_err(|e| format!("/proc: {e}"))?;
        let mut procs = Vec::new();
        for e in dir.flatten() {
            let p = e.path();
            if !e.file_name().to_string_lossy().bytes().all(|b| b.is_ascii_digit()) {
                continue;
            }
            // a process that ended between the listing and the read is simply not there
            let comm = std::fs::read_to_string(p.join("comm")).unwrap_or_default();
            let cmdline = std::fs::read(p.join("cmdline")).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
            procs.push((comm, cmdline));
        }
        if procs.is_empty() {
            return Err("/proc lists no processes".into());
        }
        return Ok(procs.iter().any(|(c, l)| is_game_process(c, l)));
    }
    Err("no process list Magician can read on this system".into())
}

/// Whether a running game is the one in `game` (the `game` folder an install changes): one of `exes`, the running
/// games' executables, lies under it. An executable whose path could not be read counts as this game's: a running
/// game that can't be placed is not assumed to be somewhere else. No running game is never this one.
pub fn runs_from(game: &Path, exes: &[Option<PathBuf>]) -> bool {
    let norm = |p: &Path| {
        let p = std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
        p.to_string_lossy().replace('/', "\\").trim_start_matches(r"\\?\").trim_end_matches('\\').to_ascii_lowercase()
    };
    let root = norm(game);
    exes.iter().any(|e| match e {
        None => true,
        Some(x) => {
            let x = norm(x);
            x.starts_with(&format!("{root}\\"))
        }
    })
}

/// The executables of the running games (`deadlock.exe`), each `None` when its path can't be read (another user's or
/// an elevated process, or a Wine drive that can't be placed). Windows asks the system; Linux reads `/proc`, where the
/// game under Proton names its program by a Wine path (`wine_to_unix`). Elsewhere every running game is unplaced.
pub fn running_game_exes() -> Vec<Option<PathBuf>> {
    if cfg!(target_os = "linux") {
        let mut v = Vec::new();
        for e in std::fs::read_dir("/proc").into_iter().flatten().flatten() {
            if !e.file_name().to_string_lossy().bytes().all(|b| b.is_ascii_digit()) {
                continue;
            }
            let p = e.path();
            let comm = std::fs::read_to_string(p.join("comm")).unwrap_or_default();
            let cmdline = std::fs::read(p.join("cmdline")).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
            if !is_game_process(&comm, &cmdline) {
                continue;
            }
            let environ = std::fs::read(p.join("environ")).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
            let prefix = environ.split('\0').find_map(|kv| kv.strip_prefix("WINEPREFIX=")).map(PathBuf::from);
            v.push(game_program(&cmdline).and_then(|prog| wine_to_unix(prog, prefix.as_deref())));
        }
        return if v.is_empty() { vec![None] } else { v };
    }
    if !cfg!(windows) {
        return vec![None];
    }
    let o = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", "Get-CimInstance Win32_Process -Filter \"Name='deadlock.exe'\" | ForEach-Object { if ($_.ExecutablePath) { $_.ExecutablePath } else { '?' } }"])
        .output();
    match o {
        Ok(o) if o.status.success() => {
            let v: Vec<Option<PathBuf>> = String::from_utf8_lossy(&o.stdout).lines().map(str::trim).filter(|l| !l.is_empty()).map(|l| (l != "?").then(|| PathBuf::from(l))).collect();
            if v.is_empty() { vec![None] } else { v }
        }
        _ => vec![None],
    }
}

/// The game's program in a `/proc/<pid>/cmdline`: the first argument, or a Wine loader's second.
fn game_program(cmdline: &str) -> Option<&str> {
    let mut args = cmdline.split('\0').filter(|a| !a.is_empty());
    let first = args.next()?;
    let base = first.rsplit(['/', '\\']).next().unwrap_or("");
    if base.starts_with("wine") || base.contains("preloader") { args.next() } else { Some(first) }
}

/// A Wine path as the Linux path it names: `Z:\home\u\...` is `/home/u/...` (Wine's Z: is the root), another drive is
/// its link in the prefix's `dosdevices`, and a path that is already a Linux one is itself. `None` when it can't be
/// placed.
pub fn wine_to_unix(p: &str, prefix: Option<&Path>) -> Option<PathBuf> {
    if p.starts_with('/') {
        return Some(PathBuf::from(p));
    }
    let b = p.as_bytes();
    if b.len() < 3 || !b[0].is_ascii_alphabetic() || b[1] != b':' || !matches!(b[2], b'\\' | b'/') {
        return None;
    }
    let rest: Vec<&str> = p[3..].split(['\\', '/']).filter(|s| !s.is_empty()).collect();
    let drive = b[0].to_ascii_lowercase() as char;
    let root = if drive == 'z' { PathBuf::from("/") } else { std::fs::canonicalize(prefix?.join("dosdevices").join(format!("{drive}:"))).ok()? };
    Some(rest.iter().fold(root, |acc, s| acc.join(s)))
}

/// A process is the game when its name, or its program in its command line, is `deadlock.exe` (Windows, or Proton) or
/// `project8`. `cmdline` is `/proc/<pid>/cmdline`: arguments separated by NULs; only the program (the first one, or a
/// Wine loader's second) counts, so an editor with `deadlock.exe` in an argument is not the game.
pub fn is_game_process(comm: &str, cmdline: &str) -> bool {
    let is_game = |s: &str| {
        let s = s.trim().to_ascii_lowercase();
        let name = s.rsplit(['/', '\\']).next().unwrap_or("");
        name == "deadlock.exe" || name == "project8" || name == "project8.exe"
    };
    let mut args = cmdline.split('\0');
    let program = args.next().unwrap_or("");
    let loader = program.rsplit(['/', '\\']).next().is_some_and(|n| n.starts_with("wine") || n.contains("preloader"));
    is_game(comm) || is_game(program) || (loader && args.next().is_some_and(is_game))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_game_that_cant_be_seen_is_refused_except_to_the_holder() {
        let lock = r#"{"holder": "lash-fix"}"#;
        let e = seat_check_unknown("tasklist did not run", None, None).unwrap_err();
        assert!(e.contains("can't tell whether Deadlock is running (tasklist did not run)") && e.contains("--ignore-game"), "{e}");
        assert!(seat_check_unknown("x", Some(lock), Some("other")).is_err());
        assert!(seat_check_unknown("x", Some(lock), Some("lash-fix")).is_ok());
    }

    #[test]
    fn a_running_game_blocks_only_the_install_it_runs_from() {
        let real = Path::new(r"D:\SteamLibrary\steamapps\common\Deadlock\game");
        let exe = Some(PathBuf::from(r"D:\SteamLibrary\steamapps\common\Deadlock\game\bin\win64\deadlock.exe"));
        assert!(runs_from(real, std::slice::from_ref(&exe)), "the real install, under the running game");
        assert!(runs_from(Path::new(r"d:/steamlibrary/steamapps/common/deadlock/game/"), std::slice::from_ref(&exe)), "case and separators do not matter");
        let scratch = Path::new(r"C:\Temp\magician-test\game");
        assert!(!runs_from(scratch, std::slice::from_ref(&exe)), "a scratch game folder is not the running game's");
        assert!(!runs_from(Path::new(r"D:\SteamLibrary\steamapps\common\Deadlock\gam"), std::slice::from_ref(&exe)), "a prefix of a folder name is not the folder");
        assert!(runs_from(scratch, &[exe.clone(), None]), "a game whose path can't be read counts as this one");
        assert!(!runs_from(scratch, &[]), "no game running");
    }

    #[test]
    fn the_game_is_found_under_proton_by_its_program_not_its_arguments() {
        assert!(is_game_process("deadlock.exe\n", ""));
        assert!(is_game_process("wine64-preload", "/home/u/.steam/proton/files/bin/wine64-preload\0Z:\\home\\u\\Deadlock\\game\\bin\\win64\\deadlock.exe\0-novid\0"));
        assert!(is_game_process("", "Z:\\steamapps\\common\\Deadlock\\game\\bin\\win64\\project8.exe\0"));
        assert!(is_game_process("project8", ""));
        assert!(!is_game_process("vim", "vim\0notes/deadlock.exe\0"));
        assert!(!is_game_process("bash", "bash\0-c\0echo\0deadlock.exe"));
        assert!(!is_game_process("steam", "/usr/bin/steam\0"));
        assert!(!is_game_process("", ""));
    }

    #[test]
    fn a_game_under_proton_is_placed_by_its_wine_path() {
        let cmd = "/home/u/.steam/proton/files/bin/wine64-preload\0Z:\\home\\u\\Steam\\steamapps\\common\\Deadlock\\game\\bin\\win64\\deadlock.exe\0-novid\0";
        let prog = game_program(cmd).unwrap();
        assert_eq!(wine_to_unix(prog, None), Some(Path::new("/").join("home").join("u").join("Steam").join("steamapps").join("common").join("Deadlock").join("game").join("bin").join("win64").join("deadlock.exe")));
        assert_eq!(game_program("/usr/bin/deadlock.exe\0-x\0"), Some("/usr/bin/deadlock.exe"));
        assert_eq!(wine_to_unix("/opt/Deadlock/game/bin/win64/deadlock.exe", None), Some(PathBuf::from("/opt/Deadlock/game/bin/win64/deadlock.exe")));
        assert_eq!(wine_to_unix("D:\\Games\\deadlock.exe", None), None, "another drive needs the prefix");
        assert_eq!(wine_to_unix("deadlock.exe", None), None);
        // a drive in the prefix is its dosdevices link
        let d = std::env::temp_dir().join(format!("magician-wine-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("pfx").join("dosdevices")).unwrap();
        std::fs::create_dir_all(d.join("lib")).unwrap();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(d.join("lib"), d.join("pfx").join("dosdevices").join("d:")).unwrap();
            let lib = std::fs::canonicalize(d.join("lib")).unwrap();
            assert_eq!(wine_to_unix("D:\\steamapps\\x.exe", Some(&d.join("pfx"))), Some(lib.join("steamapps").join("x.exe")));
            // and that is what the seat check compares with the game folder an install changes
            assert!(runs_from(&lib.join("steamapps"), &[Some(lib.join("steamapps").join("x.exe"))]));
        }
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn seat_check_allows_a_closed_game_and_the_holder() {
        let lock = r#"{"holder": "lash-fix", "what": "console", "since": 1.0, "launching": false, "pid": 1}"#;
        assert!(seat_check(false, Some(lock), None).is_ok());
        assert!(seat_check(true, Some(lock), Some("lash-fix")).is_ok());
    }

    #[test]
    fn seat_check_refuses_under_another_pipeline_or_a_player() {
        let lock = r#"{"holder": "example-polish", "what": "s1.txt"}"#;
        let e = seat_check(true, Some(lock), Some("lash-fix")).unwrap_err();
        assert!(e.contains("example-polish"), "{e}");
        assert!(seat_check(true, Some(lock), None).is_err());
        assert!(seat_check(true, None, Some("lash-fix")).unwrap_err().contains("no pipeline"));
    }

    #[test]
    fn lock_holder_reads_gamelock_json() {
        assert_eq!(lock_holder(r#"{"holder":"a\"b","x":1}"#).as_deref(), Some("a\"b"));
        assert_eq!(lock_holder(r#"{"what": "x"}"#), None);
        assert_eq!(lock_holder("not json"), None);
    }

    const STOCK: &str = "\"GameInfo\"\r\n{\r\n\tFileSystem\r\n\t{\r\n\t\tSearchPaths\r\n\t\t{\r\n\t\t\tGame_LowViolence\tcitadel_lv\r\n\r\n\t\t\tGame\t\t\t\tcitadel\r\n\t\t\tGame\t\t\t\tcore\r\n\t\t}\r\n\t}\r\n}\r\n";

    #[test]
    fn inserts_above_game_citadel_with_same_indent_and_crlf() {
        let out = add_addons_search_path(STOCK).unwrap().unwrap();
        assert!(out.contains("\t\t\tGame\t\t\t\tcitadel/addons\r\n\t\t\tGame\t\t\t\tcitadel\r\n"), "{out}");
        assert_eq!(out.matches("citadel/addons").count(), 1);
    }

    #[test]
    fn idempotent() {
        let once = add_addons_search_path(STOCK).unwrap().unwrap();
        assert_eq!(add_addons_search_path(&once).unwrap(), None);
        assert_eq!(configure_addons_search_path(&once, true).unwrap(), None);
    }

    #[test]
    fn preserves_stock_mod_and_write_roots_before_mounting() {
        let expected = "\t\t\tMod\t\t\t\tcitadel\r\n\t\t\tWrite\t\t\t\tcitadel\r\n";
        let mounted = add_addons_search_path(STOCK).unwrap().unwrap();
        assert!(mounted.contains(&format!("{expected}\t\t\t{ADDONS_LINE}\r\n")));
        let legacy = mounted.replace(expected, "");
        assert_eq!(configure_addons_search_path(&legacy, true).unwrap().unwrap(), mounted);
        let custom = STOCK.replace("Game_LowViolence", "Mod custom\r\n\t\t\tGame_LowViolence");
        let mounted = add_addons_search_path(&custom).unwrap().unwrap();
        assert!(mounted.contains("Mod custom"));
        assert!(!mounted.contains("Mod\t\t\t\tcitadel"));
    }

    #[test]
    fn respects_an_existing_manual_or_manager_edit() {
        let manual = STOCK.replace("\t\t\tGame\t\t\t\tcitadel\r\n", "\t\t\tGame citadel/addons\r\n\t\t\tGame\t\t\t\tcitadel\r\n");
        assert_eq!(add_addons_search_path(&manual).unwrap(), None);
    }

    #[test]
    fn does_not_match_game_language_or_core_lines() {
        let weird = "SearchPaths\n{\nGame_UILanguage citadel_*LANGUAGE*\nGame core\nGame citadel\n}\n";
        let out = add_addons_search_path(weird).unwrap().unwrap();
        assert!(out.contains("Game core\nGame\t\t\t\tcitadel/addons\nGame citadel\n"), "{out}");
    }

    #[test]
    fn errors_without_search_paths() {
        assert!(add_addons_search_path("\"GameInfo\" {}").is_err());
        assert!(add_addons_search_path("// SearchPaths { Game citadel }\n\"GameInfo\" {}").is_err(), "a comment is not the block");
        assert!(search_paths_block("SearchPaths\n{\n\tGame citadel\n").unwrap_err().contains("unclosed"));
    }

    #[test]
    fn the_search_paths_block_ends_at_its_own_brace() {
        // SearchPaths named in a comment first, a `}` in a comment and in a quoted value, and a nested block, all
        // before the `Game citadel` line
        let gi = "\"GameInfo\"\n{\n\t// SearchPaths are below }\n\tFileSystem\n\t{\n\t\tSearchPaths\n\t\t{\n\t\t\tGame_LowViolence\tcitadel_lv // }\n\t\t\t\"Platform\" \"x}y\"\n\t\t\tExtra\n\t\t\t{\n\t\t\t\tGame\tsomewhere\n\t\t\t}\n\t\t\tGame\t\t\t\tcitadel\n\t\t\tGame\t\t\t\tcore\n\t\t}\n\t}\n}\n";
        let (at, end) = search_paths_block(gi).unwrap();
        assert!(gi[at..].starts_with("SearchPaths\n") && gi[..=end].ends_with("core\n\t\t}"), "{}", &gi[at..=end]);
        let out = add_addons_search_path(gi).unwrap().unwrap();
        assert!(out.contains("\t\t\tGame\t\t\t\tcitadel/addons\n\t\t\tGame\t\t\t\tcitadel\n"), "{out}");
        assert_eq!(add_addons_search_path(&out).unwrap(), None);
        // the stock file, and a quoted key, read as before
        assert_eq!(search_paths_block(STOCK).map(|(a, _)| &STOCK[a..a + 11]), Ok("SearchPaths"));
        assert!(add_addons_search_path(&STOCK.replace("SearchPaths", "\"SearchPaths\"")).unwrap().is_some());
    }

    // ---- slots, owners and the UI scene route (all against a scratch copy of gameinfo.gi, never the game) ----

    fn scratch(name: &str) -> (PathBuf, Paths) {
        let d = std::env::temp_dir().join(format!("magician_install_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("gameinfo.gi"), STOCK).unwrap();
        (d.clone(), Paths { citadel: d })
    }

    fn pak(dir: &Path, owner: &str, name: &str, body: &str) -> PathBuf {
        let d = dir.join("src").join(owner);
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join(name);
        vpk::write(&p, &[("particles/x.vpcf_c".to_string(), body.as_bytes().to_vec())]).unwrap();
        p
    }

    #[test]
    fn routes() {
        // the paths are written as Windows writes them; elsewhere a backslash is a file name character
        let native = |p: &str| if cfg!(windows) { p.to_string() } else { p.replace('\\', "/") };
        let r = |p: &str, a: Option<&str>| route_for(Path::new(&native(p)), a);
        assert_eq!(r(r"D:\work\release\maid_lash\pak10_dir.vpk", None), Ok(Route::Pak("pak10_dir.vpk".into())));
        assert_eq!(r(r"D:\work\release\maid_lash\pak10_dir.vpk", Some("pak11_dir.vpk")), Ok(Route::Pak("pak11_dir.vpk".into())));
        for bad in ["pak1_dir.vpk", "pak00_dir.vpk", "Pak11_dir.vpk", "pak11.vpk", "../x/pak11_dir.vpk"] {
            assert!(r(r"D:\a\pak10_dir.vpk", Some(bad)).is_err(), "{bad}");
        }
        assert_eq!(r(r"D:\scenes\maps\ui\hero_prefabs\lash.vpk", None), Ok(Route::UiScene("lash.vpk".into())));
        assert_eq!(Route::UiScene("lash.vpk".into()).rel(), "maps/ui/hero_prefabs/lash.vpk");
        assert!(r(r"D:\scenes\maps\ui\hero_prefabs\lash.vpk", Some("pak11_dir.vpk")).is_err(), "--as is for paks");
        // anything else under maps/ is refused, and so is any other name
        for bad in [r"D:\s\maps\ui\hero_shop\lash_shop.vpk", r"D:\s\maps\dl_midtown.vpk", r"D:\s\maps\ui\hero_prefabs\sub\lash.vpk", r"D:\s\maps\ui\hero_prefabs\Lash Two.vpk", r"D:\s\lash.vpk"] {
            assert!(r(bad, None).is_err(), "{bad}");
        }
        assert!(r(r"D:\s\maps\ui\hero_shop\lash_shop.vpk", None).unwrap_err().contains("under maps/ only maps/ui/hero_prefabs"));
        // a download's pak keeps its own name; --as installs it under a slot, as the refusal says
        assert_eq!(r(r"D:\s\Some Mod v2.vpk", Some("pak02_dir.vpk")), Ok(Route::Pak("pak02_dir.vpk".into())));
        assert!(r(r"D:\s\maps\dl_midtown.vpk", Some("pak02_dir.vpk")).is_err(), "a map is never a pak, --as or not");
        assert_eq!(owner_for(Path::new(&native(r"D:\work\release\example_skin\pak10_dir.vpk")), None), "example_skin");
        assert_eq!(owner_for(Path::new(&native(r"D:\work\lash_scenes\maps\ui\hero_prefabs\lash.vpk")), None), "lash_scenes");
        assert_eq!(owner_for(Path::new(&native(r"D:\a\pak10_dir.vpk")), Some(" Example ")), "Example");
    }

    #[test]
    fn a_slot_is_never_replaced_silently() {
        let row = |n: &str, c: &str, o: &str| Installed { name: n.into(), crc: c.into(), owner: o.into() };
        let led = vec![row("pak10_dir.vpk", "aaaa", "maid_lash")];
        assert_eq!(check_slot(&led, "pak10_dir.vpk", "bbbb", "x", None), Slot::Free);
        assert_eq!(check_slot(&led, "pak10_dir.vpk", "aaaa", "example", Some("aaaa")), Slot::Same);
        assert_eq!(check_slot(&led, "pak10_dir.vpk", "bbbb", "maid_lash", Some("aaaa")), Slot::Update);
        assert_eq!(check_slot(&led, "pak10_dir.vpk", "bbbb", "example", Some("aaaa")), Slot::OtherMod { owner: "maid_lash".into(), crc: "aaaa".into() });
        assert!(matches!(check_slot(&led, "pak10_dir.vpk", "bbbb", "maid_lash", Some("cccc")), Slot::Foreign { .. }), "changed on disk since we installed it");
        assert!(matches!(check_slot(&led, "pak12_dir.vpk", "bbbb", "x", Some("dddd")), Slot::Foreign { .. }), "someone else's file");
        let legacy = vec![row("pak10_dir.vpk", "aaaa", UNKNOWN_OWNER)];
        assert!(matches!(check_slot(&legacy, "pak10_dir.vpk", "bbbb", UNKNOWN_OWNER, Some("aaaa")), Slot::OtherMod { .. }), "an unknown owner is not assumed to be us");
    }

    #[test]
    fn two_mods_defaulting_to_pak10_do_not_overwrite_each_other() {
        // Real case: Maid Lash's and another skin's releases both default to pak10_dir.vpk.
        let (d, ip) = scratch("slots");
        let lash = pak(&d, "maid_lash", "pak10_dir.vpk", "lash");
        let faz = pak(&d, "example_skin", "pak10_dir.vpk", "example");
        let route = |p: &Path, a: Option<&str>| route_for(p, a).unwrap();
        let first = install_vpk(&ip, "6864", &lash, &route(&lash, None), &owner_for(&lash, None), false).unwrap();
        assert!(first.notes.iter().any(|n| n.contains("backed up gameinfo.gi")), "{:?}", first.notes);
        assert_eq!(std::fs::read(ip.backup("6864")).unwrap(), STOCK.as_bytes(), "pristine backup");
        assert!(std::fs::read_to_string(ip.gameinfo()).unwrap().contains("citadel/addons"));
        let installed = std::fs::read(ip.addons().join("pak10_dir.vpk")).unwrap();
        assert_eq!(installed, std::fs::read(&lash).unwrap());

        // the other mod is refused, nothing changes, and the message names the owner and a free slot
        let e = install_vpk(&ip, "6864", &faz, &route(&faz, None), &owner_for(&faz, None), false).err().unwrap();
        assert!(e.contains("maid_lash") && e.contains("example_skin") && e.contains("--as pak11_dir.vpk") && e.contains("Not overwriting"), "{e}");
        assert_eq!(std::fs::read(ip.addons().join("pak10_dir.vpk")).unwrap(), installed);
        assert_eq!(read_installed(&ip.ledger()).len(), 1);

        // under its own slot it installs beside the first
        let second = install_vpk(&ip, "6864", &faz, &route(&faz, Some("pak11_dir.vpk")), "example_skin", false).unwrap();
        assert!(second.dst.ends_with("pak11_dir.vpk"));
        assert!(second.notes.iter().any(|n| n.contains("already present")), "the search path is idempotent: {:?}", second.notes);
        let led = read_installed(&ip.ledger());
        assert_eq!(led.iter().map(|r| (r.name.as_str(), r.owner.as_str())).collect::<Vec<_>>(), vec![("pak10_dir.vpk", "maid_lash"), ("pak11_dir.vpk", "example_skin")]);

        // an updated build of the same mod replaces its own install; the same bytes again is a no-op
        let lash2 = pak(&d, "maid_lash", "pak10_dir.vpk", "lash, rebuilt");
        install_vpk(&ip, "6864", &lash2, &route(&lash2, None), "maid_lash", false).unwrap();
        assert_eq!(std::fs::read(ip.addons().join("pak10_dir.vpk")).unwrap(), std::fs::read(&lash2).unwrap());
        let again = install_vpk(&ip, "6864", &lash2, &route(&lash2, None), "maid_lash", false).unwrap();
        assert!(again.notes.iter().any(|n| n.contains("already holds")));

        // --force replaces it and says so
        let forced = install_vpk(&ip, "6864", &faz, &route(&faz, None), "example_skin", true).unwrap();
        assert!(forced.notes.iter().any(|n| n.contains("WARNING") && n.contains("maid_lash")), "{:?}", forced.notes);
        assert_eq!(read_installed(&ip.ledger()).iter().find(|r| r.name == "pak10_dir.vpk").unwrap().owner, "example_skin");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn rollback_removes_only_what_the_ledger_holds_and_can_be_scoped_to_one_mod() {
        let (d, ip) = scratch("rollback");
        let lash = pak(&d, "maid_lash", "pak10_dir.vpk", "lash");
        let faz = pak(&d, "example_skin", "pak11_dir.vpk", "faz");
        install_vpk(&ip, "6864", &lash, &route_for(&lash, None).unwrap(), "maid_lash", false).unwrap();
        install_vpk(&ip, "6864", &faz, &route_for(&faz, None).unwrap(), "example_skin", false).unwrap();
        std::fs::write(ip.addons().join("pak20_dir.vpk"), b"someone else's").unwrap();
        let gone = uninstall_matching(&ip, Some("maid_lash"), false).unwrap();
        assert_eq!(gone.len(), 1);
        assert!(!ip.addons().join("pak10_dir.vpk").exists() && ip.addons().join("pak11_dir.vpk").exists());
        assert_eq!(uninstall_matching(&ip, None, false).unwrap().len(), 1);
        assert!(!ip.addons().join("pak11_dir.vpk").exists());
        assert!(ip.addons().join("pak20_dir.vpk").exists(), "a file we did not install stays");
        assert!(read_installed(&ip.ledger()).is_empty());
        assert!(uninstall_matching(&ip, None, false).unwrap().is_empty(), "nothing left to remove is not an error");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_game_update_that_resets_gameinfo_is_repaired_by_the_next_install_with_a_new_backup() {
        let (d, ip) = scratch("update");
        let lash = pak(&d, "maid_lash", "pak10_dir.vpk", "lash");
        install_vpk(&ip, "6864", &lash, &route_for(&lash, None).unwrap(), "maid_lash", false).unwrap();
        let with_path = std::fs::read_to_string(ip.gameinfo()).unwrap();
        assert!(with_path.contains("citadel/addons"));
        // Steam updates: gameinfo.gi is replaced by the new build's stock file, the paks stay where they were.
        let new_stock = STOCK.replace("citadel_lv", "citadel_lv2");
        std::fs::write(ip.gameinfo(), &new_stock).unwrap();
        assert!(add_addons_search_path(&std::fs::read_to_string(ip.gameinfo()).unwrap()).unwrap().is_some(), "the reset is detectable");
        let again = install_vpk(&ip, "6865", &lash, &route_for(&lash, None).unwrap(), "maid_lash", false).unwrap();
        assert!(std::fs::read_to_string(ip.gameinfo()).unwrap().contains("citadel/addons"), "path re-applied");
        assert!(again.notes.iter().any(|n| n.contains("backed up gameinfo.gi")), "{:?}", again.notes);
        assert_eq!(std::fs::read(ip.backup("6865")).unwrap(), new_stock.as_bytes(), "the new build's stock file, not ours");
        assert_eq!(std::fs::read(ip.backup("6864")).unwrap(), STOCK.as_bytes(), "the old build's backup is kept");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_file_we_did_not_install_is_not_overwritten() {
        let (d, ip) = scratch("foreign");
        std::fs::create_dir_all(ip.addons()).unwrap();
        std::fs::write(ip.addons().join("pak10_dir.vpk"), b"another manager pak").unwrap();
        let m = pak(&d, "mod_a", "pak10_dir.vpk", "a");
        let e = install_vpk(&ip, "6864", &m, &Route::Pak("pak10_dir.vpk".into()), "mod_a", false).err().unwrap();
        assert!(e.contains("wasn't installed by magician") && e.contains("--as"), "{e}");
        assert_eq!(std::fs::read(ip.addons().join("pak10_dir.vpk")).unwrap(), b"another manager pak");
        assert!(!ip.backup("6864").exists(), "a refused install touches nothing, gameinfo.gi included");
        assert_eq!(std::fs::read(ip.gameinfo()).unwrap(), STOCK.as_bytes());
        // a ledger row from before owners were kept reads as an unknown owner
        std::fs::write(ip.ledger(), format!("pak10_dir.vpk\t{:08x}\n", vpk::crc32(b"another manager pak"))).unwrap();
        assert_eq!(read_installed(&ip.ledger())[0].owner, UNKNOWN_OWNER);
        let e = install_vpk(&ip, "6864", &m, &Route::Pak("pak10_dir.vpk".into()), "mod_a", false).err().unwrap();
        assert!(e.contains("another mod (?"), "{e}");
        let _ = std::fs::remove_dir_all(&d);
    }

    fn scene(dir: &Path, hero: &str, extra: &[&str]) -> PathBuf {
        let p = dir.join("scenes").join("maps").join("ui").join("hero_prefabs").join(format!("{hero}.vpk"));
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        let mut files = vec![
            (format!("{SCENE_DIR}/{hero}.vmap_c"), b"map".to_vec()),
            (format!("{SCENE_DIR}/{hero}/world.vwrld_c"), b"world".to_vec()),
            (format!("{SCENE_DIR}/{hero}/entities/default_ents.vents_c"), b"ents".to_vec()),
        ];
        for e in extra {
            files.push((e.to_string(), b"x".to_vec()));
        }
        vpk::write(&p, &files).unwrap();
        p
    }

    #[test]
    fn a_ui_scene_map_installs_under_maps_with_the_same_ledger_and_backup() {
        let (d, ip) = scratch("scene");
        let s = scene(&d, "lash", &[]);
        let route = route_for(&s, None).unwrap();
        validate_scene(&s, "lash.vpk").unwrap();
        let done = install_vpk(&ip, "6864", &s, &route, &owner_for(&s, None), false).unwrap();
        assert_eq!(done.dst, ip.addons().join("maps").join("ui").join("hero_prefabs").join("lash.vpk"));
        assert_eq!(std::fs::read(&done.dst).unwrap(), std::fs::read(&s).unwrap());
        assert!(ip.backup("6864").is_file(), "gameinfo.gi backed up as for a pak");
        let led = read_installed(&ip.ledger());
        assert_eq!(led[0].name, "maps/ui/hero_prefabs/lash.vpk");
        assert_eq!(led[0].owner, "scenes");
        // another mod's scene for the same hero is refused; the same mod's update goes in
        let other = scene(&d.join("other"), "lash", &[&format!("{SCENE_DIR}/lash/lightmaps/a.vtex_c")]);
        let e = install_vpk(&ip, "6864", &other, &route_for(&other, None).unwrap(), "someone_else", false).err().unwrap();
        assert!(e.contains("one slot per hero") && e.contains("scenes"), "{e}");
        install_vpk(&ip, "6864", &other, &route_for(&other, None).unwrap(), "scenes", false).unwrap();
        // uninstall by the ledger name (either slash)
        let gone = uninstall(&ip, r"maps\ui\hero_prefabs\lash.vpk", false).unwrap();
        assert!(!gone.exists() && read_installed(&ip.ledger()).is_empty());
        assert!(uninstall(&ip, "maps/ui/hero_prefabs/lash.vpk", false).is_err());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_scene_map_with_other_paths_or_no_map_is_refused() {
        let (d, _ip) = scratch("scenebad");
        let stray = scene(&d, "lash", &["materials/default/default_color_tga_96b3aa03.vtex_c"]);
        let e = validate_scene(&stray, "lash.vpk").unwrap_err();
        assert!(e.contains("outside maps/ui/hero_prefabs/lash"), "{e}");
        let p = d.join("scenes2/maps/ui/hero_prefabs/abrams.vpk");
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        vpk::write(&p, &[("particles/x.vpcf_c".to_string(), b"x".to_vec())]).unwrap();
        assert!(validate_scene(&p, "abrams.vpk").unwrap_err().contains("no maps/ui/hero_prefabs/abrams.vmap_c"));
        let _ = std::fs::remove_dir_all(&d);
    }
}

#[cfg(test)]
mod ledger_tests {
    use super::*;

    fn scratch(name: &str) -> (PathBuf, Paths) {
        let d = std::env::temp_dir().join(format!("magician_ledger_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("addons")).unwrap();
        std::fs::write(d.join("gameinfo.gi"), "\"GameInfo\"\n{\n\tFileSystem\n\t{\n\t\tSearchPaths\n\t\t{\n\t\t\tGame\t\t\t\tcitadel\n\t\t}\n\t}\n}\n").unwrap();
        (d.clone(), Paths { citadel: d })
    }

    fn pak(dir: &Path, owner: &str, name: &str, body: &str) -> PathBuf {
        let d = dir.join("src").join(owner);
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join(name);
        vpk::write(&p, &[("particles/x.vpcf_c".to_string(), body.as_bytes().to_vec())]).unwrap();
        p
    }

    fn names(ip: &Paths) -> Vec<(String, String)> {
        read_installed(&ip.ledger()).into_iter().map(|r| (r.name, r.owner)).collect()
    }

    #[test]
    fn a_pak_a_mod_manager_renamed_is_followed_by_its_crc() {
        // Deadlock Mod Manager 2.3.0 renumbers every pak in addons: our pak10 becomes pak03, and its own pak takes pak10
        let (d, ip) = scratch("renamed");
        let lash = pak(&d, "maid_lash", "pak10_dir.vpk", "lash");
        install_vpk(&ip, "6864", &lash, &route_for(&lash, None).unwrap(), "maid_lash", false).unwrap();
        std::fs::rename(ip.addons().join("pak10_dir.vpk"), ip.addons().join("pak03_dir.vpk")).unwrap();
        std::fs::write(ip.addons().join("pak10_dir.vpk"), b"the manager's own mod").unwrap();

        // installing the same release again finds it under its new name and copies nothing
        let again = install_vpk(&ip, "6864", &lash, &route_for(&lash, None).unwrap(), "maid_lash", false).unwrap();
        assert!(again.dst.ends_with("pak03_dir.vpk") && again.notes.iter().any(|n| n.contains("pak10_dir.vpk is now pak03_dir.vpk")), "{:?}", again.notes);
        assert_eq!(names(&ip), vec![("pak03_dir.vpk".to_string(), "maid_lash".to_string())]);
        assert_eq!(std::fs::read(ip.addons().join("pak10_dir.vpk")).unwrap(), b"the manager's own mod", "the manager's file is untouched");

        // a rollback removes our bytes under their new name and leaves the manager's file
        let gone = uninstall_matching(&ip, Some("maid_lash"), false).unwrap();
        assert_eq!(gone, vec![ip.addons().join("pak03_dir.vpk")]);
        assert!(ip.addons().join("pak10_dir.vpk").exists() && names(&ip).is_empty());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn an_update_to_a_renamed_pak_is_refused_rather_than_installed_twice() {
        let (d, ip) = scratch("renamed-update");
        let lash = pak(&d, "maid_lash", "pak10_dir.vpk", "lash");
        install_vpk(&ip, "6864", &lash, &route_for(&lash, None).unwrap(), "maid_lash", false).unwrap();
        std::fs::rename(ip.addons().join("pak10_dir.vpk"), ip.addons().join("pak03_dir.vpk")).unwrap();
        let newer = pak(&d, "maid_lash", "pak10_dir.vpk", "lash, rebuilt");
        let e = install_vpk(&ip, "6864", &newer, &route_for(&newer, None).unwrap(), "maid_lash", false).err().unwrap();
        assert!(matches!(e, Stop::Refused(_)) && e.contains("installed as pak03_dir.vpk") && e.contains("--as pak03_dir.vpk"), "{e}");
        assert!(!ip.addons().join("pak10_dir.vpk").exists(), "no second copy");
        // in place, as the message says
        install_vpk(&ip, "6864", &newer, &route_for(&newer, Some("pak03_dir.vpk")).unwrap(), "maid_lash", false).unwrap();
        assert_eq!(std::fs::read(ip.addons().join("pak03_dir.vpk")).unwrap(), std::fs::read(&newer).unwrap());
        assert_eq!(names(&ip), vec![("pak03_dir.vpk".to_string(), "maid_lash".to_string())]);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn uninstall_never_removes_bytes_that_are_not_the_ones_it_installed() {
        let (d, ip) = scratch("changed");
        let lash = pak(&d, "maid_lash", "pak10_dir.vpk", "lash");
        install_vpk(&ip, "6864", &lash, &route_for(&lash, None).unwrap(), "maid_lash", false).unwrap();
        std::fs::write(ip.addons().join("pak10_dir.vpk"), b"another tool's file now").unwrap();
        for e in [uninstall(&ip, "pak10_dir.vpk", false).unwrap_err(), uninstall_matching(&ip, None, false).unwrap_err()] {
            assert!(matches!(e, Stop::Refused(_)) && e.contains("changed since magician installed it"), "{e}");
        }
        assert!(ip.addons().join("pak10_dir.vpk").exists() && names(&ip).len() == 1, "refused: nothing changed");
        assert!(matches!(uninstall(&ip, "pak11_dir.vpk", false), Err(Stop::Refused(_))), "not in the ledger: refused, exit 3");
        uninstall(&ip, "pak10_dir.vpk", true).unwrap();
        assert!(!ip.addons().join("pak10_dir.vpk").exists() && names(&ip).is_empty());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_ledger_row_that_is_not_an_install_name_is_never_acted_on() {
        let (d, ip) = scratch("forged");
        std::fs::write(ip.citadel.join("keep.txt"), b"precious").unwrap();
        write_installed(&ip.ledger(), &[Installed { name: "../keep.txt".into(), crc: format!("{:08x}", vpk::crc32(b"precious")), owner: "x".into() }]).unwrap();
        assert!(uninstall(&ip, "../keep.txt", true).unwrap_err().contains("not a file magician installs"));
        assert!(uninstall_matching(&ip, None, true).unwrap().is_empty());
        assert!(ip.citadel.join("keep.txt").exists());
        // and a mod name can't add a row or a column to the ledger
        let lash = pak(&d, "maid_lash", "pak10_dir.vpk", "lash");
        for bad in ["a\tb", "a\nb", ""] {
            assert!(matches!(install_vpk(&ip, "6864", &lash, &route_for(&lash, None).unwrap(), bad, false), Err(Stop::Usage(_))), "{bad:?}");
        }
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn installs_at_the_same_moment_keep_every_ledger_row() {
        let (d, ip) = scratch("concurrent");
        let srcs: Vec<PathBuf> = (20..36).map(|n| pak(&d, &format!("m{n}"), &format!("pak{n}_dir.vpk"), &format!("mod {n}"))).collect();
        std::thread::scope(|s| {
            for p in &srcs {
                let ip = Paths { citadel: ip.citadel.clone() };
                s.spawn(move || install_vpk(&ip, "6864", p, &route_for(p, None).unwrap(), &owner_for(p, None), false).unwrap());
            }
        });
        assert_eq!(read_installed(&ip.ledger()).len(), srcs.len());
        assert!(!ip.addons().join(LEDGER_LOCK).exists(), "the lock goes with the last install");
        let _ = std::fs::remove_dir_all(&d);
    }
}
