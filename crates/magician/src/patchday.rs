//! Patch day: re-prove a mod set against a new Deadlock build, offline and in minutes.
//!
//! Three pieces, all pure so they can be tested without a game:
//!   - `Rig`: the rig sidecar (`<build>/rig.tsv`) captured beside the baseline: every skeleton's bone names and every
//!     AnimGraph's control parameters. A baseline's CRCs say a rig file changed; the sidecar says what changed in it.
//!   - `compute`: what changed under a mod between two baselines (the core of `magician drift`).
//!   - `proven_builds` / `add_proven`: the `[proven] builds = [...]` summary in a mod's `mod.toml` (which bytes were
//!     proven is in its proof ledger, proofs.rs).
//!
//! `cmd_patchday` in main.rs strings them together with the install state into the one "check everything" entry.

use lint::{Baseline, ModFile};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};

// ------------------------------------------------------------------------------------------------ rig sidecar

/// Bone names per skeleton (`.vnmskel_c`, `m_boneIDs`) and control parameter names per AnimGraph (`.vnmgraph_c`,
/// `m_controlParameterIDs`) of one build.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Rig {
    pub build: String,
    pub skeletons: BTreeMap<String, Vec<String>>,
    pub graphs: BTreeMap<String, Vec<String>>,
}

pub fn rig_path(reference: &Path, build: &str) -> PathBuf {
    reference.join(build).join("rig.tsv")
}

impl Rig {
    /// Read every skeleton and graph in an archive. Files that don't decode are skipped (a rig file we can't read
    /// can't be compared; the CRC in the baseline still flags it).
    pub fn capture(build: &str, pak: &vpk::Vpk) -> Rig {
        let mut rig = Rig { build: build.to_string(), ..Rig::default() };
        let mut todo: Vec<&vpk::Entry> = pak.entries.iter().filter(|e| e.path.ends_with(".vnmskel_c") || e.path.ends_with(".vnmgraph_c")).collect();
        todo.sort_by_key(|e| (e.archive_index, e.offset)); // physical order: the disk may be an HDD
        for e in todo {
            let Ok(data) = pak.read(e) else { continue };
            let Some(Ok(kv)) = resource::kv3_block(&data, b"DATA") else { continue };
            let (key, into) = if e.path.ends_with(".vnmskel_c") { ("m_boneIDs", &mut rig.skeletons) } else { ("m_controlParameterIDs", &mut rig.graphs) };
            let Some(list) = kv.get(key) else { continue };
            let names: Vec<String> = list.as_array().iter().filter_map(|v| v.as_str().map(String::from)).collect();
            into.insert(e.path.clone(), names);
        }
        rig
    }

    pub fn write(&self, mut w: impl Write) -> std::io::Result<()> {
        writeln!(w, "# magician rig build={} skeletons={} graphs={}", self.build, self.skeletons.len(), self.graphs.len())?;
        for (kind, map) in [("skel", &self.skeletons), ("graph", &self.graphs)] {
            for (path, names) in map {
                writeln!(w, "{kind}\t{path}\t{}", names.join(","))?;
            }
        }
        Ok(())
    }

    pub fn parse(text: &str) -> Result<Rig, String> {
        let mut rig = Rig::default();
        for (n, line) in text.lines().enumerate() {
            let line = line.trim_end();
            if line.is_empty() {
                continue;
            }
            if let Some(rest) = line.strip_prefix('#') {
                if let Some(i) = rest.find("build=") {
                    rig.build = rest[i + 6..].split_whitespace().next().unwrap_or("").to_string();
                }
                continue;
            }
            let mut it = line.splitn(3, '\t');
            let (kind, path, names) = (it.next().unwrap_or(""), it.next(), it.next().unwrap_or(""));
            let path = path.ok_or_else(|| format!("rig line {}: no path", n + 1))?.to_string();
            let names: Vec<String> = names.split(',').filter(|s| !s.is_empty()).map(String::from).collect();
            match kind {
                "skel" => rig.skeletons.insert(path, names),
                "graph" => rig.graphs.insert(path, names),
                other => return Err(format!("rig line {}: unknown kind '{other}'", n + 1)),
            };
        }
        Ok(rig)
    }

    pub fn load(path: &Path) -> Result<Rig, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Rig::parse(&text)
    }
}

/// What a skeleton's bone list lost or gained between two builds. Added bones are harmless (the engine ignores
/// bones a clip has no track for); a missing or renamed bone leaves the mod's tracks and weights pointing at nothing.
fn bone_delta(path: &str, then: &[String], now: &[String]) -> Vec<String> {
    let have: HashSet<&String> = now.iter().collect();
    let had: HashSet<&String> = then.iter().collect();
    let gone: Vec<&String> = then.iter().filter(|b| !have.contains(b)).collect();
    let new: Vec<&String> = now.iter().filter(|b| !had.contains(b)).collect();
    let mut out = Vec::new();
    if gone.len() == 1 && new.len() == 1 {
        out.push(format!("{path}: bone '{}' is now '{}' (renamed); the mod's clips and weights still say '{}'", gone[0], new[0], gone[0]));
    } else {
        for b in &gone {
            out.push(format!("{path}: bone '{b}' removed or renamed; the mod's clips and weights still address it"));
        }
    }
    out
}

fn param_delta(path: &str, then: &[String], now: &[String]) -> Vec<String> {
    let have: HashSet<&String> = now.iter().collect();
    let gone: Vec<&String> = then.iter().filter(|p| !have.contains(p)).collect();
    gone.iter().map(|p| format!("{path}: control parameter '{p}' removed or renamed; anything driving it (an event, a clip selector) goes quiet")).collect()
}

// ------------------------------------------------------------------------------------------------ engine sidecar

/// The code and shader archives a build's files are run by, per build (`<build>/engine.tsv`): the executables and
/// libraries under `bin/win64` and `citadel/bin/win64`, every `shaders_*_dir.vpk` of `citadel` and `core`, and
/// `gameinfo.gi` of both as shipped. A baseline says what data changed; this says whether the engine reading it did,
/// which is what decides how hard an in-game re-prove should look. Paths are relative to the `game` folder, with `/`.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Engine {
    pub build: String,
    /// path -> (size, sha256); `None` when the file is in the set but its shipped bytes can't be known (a `gameinfo.gi`
    /// an install has edited, with no stock backup of this build).
    pub files: BTreeMap<String, Option<(u64, String)>>,
}

pub fn engine_path(reference: &Path, build: &str) -> PathBuf {
    reference.join(build).join("engine.tsv")
}

/// The files of a `game` folder that are the engine, sorted: (path relative to `game`, absolute path).
pub fn engine_files(game: &Path) -> Vec<(String, PathBuf)> {
    let mut out: Vec<(String, PathBuf)> = Vec::new();
    for dir in ["bin/win64", "citadel/bin/win64"] {
        let abs = game.join(dir);
        for e in std::fs::read_dir(&abs).into_iter().flatten().flatten() {
            let name = e.file_name().to_string_lossy().to_ascii_lowercase();
            if e.path().is_file() && (name.ends_with(".dll") || name.ends_with(".exe")) {
                out.push((format!("{dir}/{name}"), e.path()));
            }
        }
    }
    for dir in ["citadel", "core"] {
        for e in std::fs::read_dir(game.join(dir)).into_iter().flatten().flatten() {
            let name = e.file_name().to_string_lossy().to_ascii_lowercase();
            if e.path().is_file() && name.starts_with("shaders_") && name.ends_with("_dir.vpk") {
                out.push((format!("{dir}/{name}"), e.path()));
            }
        }
    }
    out.sort();
    out
}

impl Engine {
    pub fn write(&self, mut w: impl Write) -> std::io::Result<()> {
        writeln!(w, "# magician engine build={} files={}", self.build, self.files.len())?;
        for (path, f) in &self.files {
            match f {
                Some((size, sha)) => writeln!(w, "{path}\t{size}\t{sha}")?,
                None => writeln!(w, "{path}\t-\t-")?,
            }
        }
        Ok(())
    }

    pub fn parse(text: &str) -> Result<Engine, String> {
        let mut e = Engine::default();
        for (n, line) in text.lines().enumerate() {
            let line = line.trim_end();
            if line.is_empty() {
                continue;
            }
            if let Some(rest) = line.strip_prefix('#') {
                if let Some(i) = rest.find("build=") {
                    e.build = rest[i + 6..].split_whitespace().next().unwrap_or("").to_string();
                }
                continue;
            }
            let c: Vec<&str> = line.split('\t').collect();
            let [path, size, sha] = c[..] else { return Err(format!("engine line {}: {} field(s), not 3", n + 1, c.len())) };
            let f = match (size, sha) {
                ("-", "-") => None,
                (s, h) => Some((s.parse::<u64>().map_err(|_| format!("engine line {}: size '{s}'", n + 1))?, h.to_string())),
            };
            e.files.insert(path.to_string(), f);
        }
        Ok(e)
    }

    pub fn load(path: &Path) -> Result<Engine, String> {
        Engine::parse(&std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?)
    }
}

/// What changed in the engine between two builds.
#[derive(Debug, Default, PartialEq)]
pub struct EngineDelta {
    pub changed: Vec<String>,
    pub added: Vec<String>,
    pub removed: Vec<String>,
    /// In both, but one side's bytes are not known: not called the same, not called changed.
    pub unknown: Vec<String>,
}

impl EngineDelta {
    pub fn same(&self) -> bool {
        self.changed.is_empty() && self.added.is_empty() && self.removed.is_empty()
    }
    /// Every file that differs, in path order, for a report line.
    pub fn files(&self) -> Vec<String> {
        let mut v: Vec<String> = self.changed.iter().chain(&self.added).chain(&self.removed).cloned().collect();
        v.sort();
        v
    }
}

pub fn engine_delta(then: &Engine, now: &Engine) -> EngineDelta {
    let mut d = EngineDelta::default();
    for (p, a) in &then.files {
        match (a, now.files.get(p)) {
            (_, None) => d.removed.push(p.clone()),
            (Some(a), Some(Some(b))) if a != b => d.changed.push(p.clone()),
            (None, Some(_)) | (_, Some(None)) => d.unknown.push(p.clone()),
            _ => {}
        }
    }
    d.added = now.files.keys().filter(|p| !then.files.contains_key(*p)).cloned().collect();
    d
}

// ------------------------------------------------------------------------------------------------ drift core

/// What changed under one mod between two builds.
#[derive(Debug, Default)]
pub struct Drift {
    /// Replaced files whose Valve original changed (the mod's copy may lack new fixes, bones or clips).
    pub changed: Vec<String>,
    /// Replaced files Valve removed (the copy replaces nothing now).
    pub removed: Vec<String>,
    /// References that resolved on the old build and don't on the new one: `mod file -> missing target`.
    pub broken: Vec<String>,
    /// Game files the mod references whose CRC changed.
    pub deps: BTreeSet<String>,
    /// Bones removed or renamed in a skeleton the mod depends on (needs the rig sidecar of both builds).
    pub skeleton: Vec<String>,
    /// Control parameters removed or renamed in a graph the mod depends on.
    pub params: Vec<String>,
    /// Model contract breaches against the installed game (bones, attachments, hitboxes); filled by the caller.
    pub contract: Vec<String>,
    /// Whether the contract check ran (it needs Valve's current models, so only when the checked build is installed).
    pub contract_checked: bool,
    /// Contract errors left out because they are the mod's own choice, not something the patch did: Valve's model at
    /// that path is the same bytes on both builds, so nothing under the mod moved. Lint judges them.
    pub contract_own: usize,
}

/// Whether Valve's file at `path` differs between two builds: another CRC or size, or missing from either side, which
/// cannot be told apart from a change.
pub fn original_changed(then: &Baseline, now: &Baseline, path: &str) -> bool {
    match (then.get(path), now.get(path)) {
        (Some(a), Some(b)) => a.crc != b.crc || a.size != b.size,
        _ => true,
    }
}

/// Which of a replaced model's contract findings drift calls breaks: the errors, not fair play (a hitbox the mod
/// reshaped is a label, never broken), and only when Valve's model moved between the builds (`valve_moved`); when it
/// did not, nothing under the mod changed, and its errors are the mod's own choices, which lint judges. Returns the
/// breaks and how many errors were the mod's own.
pub fn contract_breaks(findings: Vec<String>, valve_moved: bool) -> (Vec<String>, usize) {
    let errors: Vec<String> = findings.into_iter().filter(|m| lint::deep::contract_severity(m) == lint::Severity::Error && !grade::rubric::is_fair_play("M062", m)).collect();
    if valve_moved {
        (errors, 0)
    } else {
        let n = errors.len();
        (Vec::new(), n)
    }
}

impl Drift {
    pub fn rig_deps(&self) -> Vec<&String> {
        self.deps.iter().filter(|d| d.ends_with(".vnmskel_c") || d.ends_with(".vnmgraph_c") || d.ends_with(".vmdl_c")).collect()
    }
    /// Problems that stop the mod loading or working: exit 1.
    pub fn bad(&self) -> usize {
        self.removed.len() + self.broken.len() + self.contract.len() + self.skeleton.len()
    }
    /// `broken`, `re-prove` or `clean`: the same judgement as `verdict`, as a label for scripts.
    pub fn state(&self) -> &'static str {
        if self.bad() > 0 {
            "broken"
        } else if self.changed.is_empty() && self.rig_deps().is_empty() && self.params.is_empty() {
            "clean"
        } else {
            "re-prove"
        }
    }
    pub fn verdict(&self, from: &str, to: &str) -> String {
        if self.bad() > 0 {
            format!("BROKEN on {to}: {} problem(s) above", self.bad())
        } else if self.changed.is_empty() && self.rig_deps().is_empty() && self.params.is_empty() {
            format!("no drift from {from} to {to}")
        } else {
            format!("loads on {to}; re-prove the {} changed original(s) and {} changed rig file(s) in game", self.changed.len(), self.rig_deps().len())
        }
    }
}

/// The references a mod file declares, for drift: its resource references, a sound event file's `.vsnd` names and a
/// model's AnimGraph skeleton references. `extra` is how the caller reads the KV3 data (kept out of here so the
/// core stays testable on synthesized files).
fn refs_of(f: &ModFile) -> Vec<String> {
    let Ok(r) = resource::parse(&f.data) else { return Vec::new() };
    let mut refs = lint::ref_targets(&r);
    if f.path.ends_with(".vsndevts_c") {
        if let Some(Ok(kv)) = resource::kv3_block(&f.data, b"DATA") {
            refs.extend(lint::deep::vsnd_refs(&kv));
        }
    }
    if f.path.ends_with(".vmdl_c") {
        if let Some(Ok(kv)) = resource::kv3_block(&f.data, b"DATA") {
            for s in kv.get("m_vecNmSkeletonRefs").map(|a| a.as_array().to_vec()).unwrap_or_default() {
                if let Some(s) = s.as_str() {
                    refs.push(lint::deep::compiled(s));
                }
            }
        }
    }
    refs
}

/// Compare a mod's files with the two builds' baselines (and, when both exist, their rig sidecars).
pub fn compute(files: &[ModFile], then: &Baseline, now: &Baseline, rigs: Option<(&Rig, &Rig)>) -> Drift {
    let mod_paths: HashSet<&str> = files.iter().map(|f| f.path.as_str()).collect();
    let mut d = Drift::default();
    for f in files {
        match (then.get(&f.path), now.get(&f.path)) {
            (Some(a), Some(b)) if a.crc != b.crc => d.changed.push(f.path.clone()),
            (Some(_), None) => d.removed.push(f.path.clone()),
            _ => {}
        }
        for t in refs_of(f) {
            if mod_paths.contains(t.as_str()) {
                continue;
            }
            match (then.get(&t), now.get(&t)) {
                (Some(_), None) => d.broken.push(format!("{} -> {t}", f.path)),
                (Some(a), Some(b)) if a.crc != b.crc => {
                    d.deps.insert(t);
                }
                _ => {}
            }
        }
    }
    if let Some((rt, rn)) = rigs {
        for dep in &d.deps {
            if let (Some(a), Some(b)) = (rt.skeletons.get(dep), rn.skeletons.get(dep)) {
                d.skeleton.extend(bone_delta(dep, a, b));
            }
            if let (Some(a), Some(b)) = (rt.graphs.get(dep), rn.graphs.get(dep)) {
                d.params.extend(param_delta(dep, a, b));
            }
            // A skeleton that vanished from the sidecar entirely is a removal the baseline already reports as broken.
        }
    }
    d
}

/// `mod file -> missing target` lines grouped by target, most-referenced first: a removed skeleton that ninety clips
/// point at is one finding with a count, not ninety lines.
pub fn group_broken(broken: &[String]) -> Vec<String> {
    let mut by: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for b in broken {
        if let Some((from, to)) = b.split_once(" -> ") {
            by.entry(to).or_default().push(from);
        }
    }
    let mut v: Vec<(&str, Vec<&str>)> = by.into_iter().collect();
    v.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then(a.0.cmp(b.0)));
    v.into_iter()
        .map(|(to, mut from)| {
            from.sort();
            from.dedup();
            match from.as_slice() {
                [one] => format!("{one} -> {to}"),
                _ => format!("{to} is gone; {} mod files point at it (e.g. {})", from.len(), from[0]),
            }
        })
        .collect()
}

// ------------------------------------------------------------------------------------------------ builds on record

/// Builds that have a baseline in the reference folder, ascending.
pub fn builds_on_record(reference: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(reference)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().join("baseline.tsv").is_file())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
        .collect();
    v.sort_by_key(|n| n.parse::<u64>().unwrap_or(0));
    v
}

/// The build to compare against by default: the newest baseline older than `installed`, else `installed` itself.
pub fn default_from(builds: &[String], installed: &str) -> String {
    let inst = installed.parse::<u64>().unwrap_or(0);
    builds.iter().rfind(|b| b.parse::<u64>().is_ok_and(|n| n < inst)).cloned().unwrap_or_else(|| installed.to_string())
}

/// The build a mod is compared from on patch day: the last build its installed bytes are proven on (`proven`, bound by
/// the proof ledger) at or before `installed` (a mod proven on the installed build is current), when that build's
/// baseline is on record. Otherwise the newest older baseline, with the
/// reason, because a drift needs both eras.
pub fn era_for(proven: &[String], builds: &[String], installed: &str) -> (String, Option<String>) {
    let inst = installed.parse::<u64>().unwrap_or(0);
    let last = proven.iter().filter(|b| b.parse::<u64>().is_ok_and(|n| n <= inst)).max_by_key(|b| b.parse::<u64>().unwrap_or(0));
    let fallback = default_from(builds, installed);
    match last {
        Some(b) if b == installed => (b.clone(), None),
        Some(b) if builds.contains(b) => (b.clone(), None),
        Some(b) => (fallback.clone(), Some(format!("last proven on {b}, which has no baseline on record; compared from {fallback}"))),
        None => (fallback.clone(), Some(format!("no proven build on record for these bytes; compared from {fallback}"))),
    }
}

// ------------------------------------------------------------------------------------------------ proven ledger

fn proven_section_line(toml: &str) -> Option<(usize, usize)> {
    // (byte start, byte end) of the `builds = [...]` line inside [proven].
    let mut in_proven = false;
    let mut at = 0;
    for line in toml.split_inclusive('\n') {
        let t = line.trim();
        if t.starts_with('[') {
            in_proven = grade::manifest::section_of(t) == Some("proven");
        } else if in_proven && t.split_once('=').is_some_and(|(k, _)| k.trim() == "builds") {
            return Some((at, at + line.len()));
        }
        at += line.len();
    }
    None
}

/// The builds a mod's `mod.toml` says it was proven on (`[proven] builds = [6861, "6864"]`).
pub fn proven_builds(toml: &str) -> Vec<String> {
    grade::manifest::proven(toml)
}

/// `mod.toml` with `build` added to `[proven] builds`, keeping the rest of the file and the line's own comment.
/// Only the one-line array form is supported; anything else is refused rather than guessed at.
pub fn add_proven(toml: &str, build: &str) -> Result<String, String> {
    if build.is_empty() || !build.bytes().all(|b| b.is_ascii_digit()) {
        return Err(format!("'{build}' is not a build number"));
    }
    let (a, b) = proven_section_line(toml).ok_or("mod.toml has no `builds = [...]` line under [proven]")?;
    let line = &toml[a..b];
    let (key, rest) = line.split_once('=').unwrap();
    let (body, comment) = match rest.split_once('#') {
        Some((x, c)) => (x, Some(c)),
        None => (rest, None),
    };
    if !body.contains('[') || !body.contains(']') {
        return Err("the `builds` array must sit on one line (`builds = [6864]`)".into());
    }
    let mut builds = proven_builds(toml);
    if !builds.iter().any(|x| x == build) {
        builds.push(build.to_string());
    }
    builds.sort_by_key(|n| n.parse::<u64>().unwrap_or(0));
    let eol = if line.ends_with("\r\n") { "\r\n" } else if line.ends_with('\n') { "\n" } else { "" };
    let list = builds.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(", ");
    let new = match comment {
        Some(c) => format!("{}= [{list}]  #{}", key, c.trim_end_matches(['\r', '\n'])),
        None => format!("{}= [{list}]", key),
    };
    Ok(format!("{}{}{}{}", &toml[..a], new.trim_end(), eol, &toml[b..]))
}

/// The `slug` and `[target]` paths of a mod.toml (line-based; the file is flat enough that a TOML parser isn't worth
/// a dependency).
pub fn mod_slug(toml: &str) -> Option<String> {
    grade::manifest::string(toml, "mod", "slug")
}

/// The mods of a folder: every `mod.toml` one or two levels below it that reads as one (a `[mod] slug`), sorted. Two
/// levels takes a folder of mods (`<dir>/<mod>/mod.toml`) and a workspace that keeps them in one (`<dir>/<any name>/
/// <mod>/mod.toml`); it goes no deeper, so a mod's own subfolders and a whole disk are never walked.
pub fn mod_tomls_under(dir: &Path) -> Vec<PathBuf> {
    let subdirs = |d: &Path| -> Vec<PathBuf> {
        let mut v: Vec<PathBuf> = std::fs::read_dir(d).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
        v.sort();
        v
    };
    let is_mod = |p: &Path| std::fs::read_to_string(p).is_ok_and(|t| mod_slug(&t).is_some());
    let mut out = Vec::new();
    for child in subdirs(dir) {
        let toml = child.join("mod.toml");
        if is_mod(&toml) {
            out.push(toml);
            continue; // a mod's own subfolders are its business
        }
        out.extend(subdirs(&child).into_iter().map(|g| g.join("mod.toml")).filter(|p| is_mod(p)));
    }
    out.sort();
    out
}

/// The heroes.vdata key a mod.toml names (`[mod] hero = "hero_lash"`), when it names one.
pub fn mod_hero(toml: &str) -> Option<String> {
    grade::manifest::string(toml, "mod", "hero")
}

// ------------------------------------------------------------------------------------------------ hero data

/// Where a build's decompiled hero data is kept in the reference folder (the grader's cache; `patchday --capture`
/// fills it so the next patch has a "before").
pub fn hero_data_path(reference: &Path, build: &str) -> PathBuf {
    reference.join(build).join("decompiled").join("scripts").join("heroes.vdata")
}

/// What a skin can lean on in a hero's block of heroes.vdata: every quoted value that names something (a file, an
/// ability, a sound event, a particle, a localisation token), which is every quoted value with a `/`, `.` or `_` in
/// it. Numbers are never quoted and a plain word ("Normal") names nothing, so a balance patch leaves this set alone
/// however much of the block it rewrites. None when the hero has no block.
pub fn hero_names(heroes_text: &str, hero_key: &str) -> Option<BTreeSet<String>> {
    let block = grade::footprint::top_level_block(heroes_text, hero_key)?;
    Some(grade::footprint::quoted(block).into_iter().filter(|s| s.contains(['/', '.', '_']) && !s.contains(char::is_whitespace)).collect())
}

/// A hero's names in two builds: how many stayed, which are gone, which are new.
#[derive(Debug, PartialEq)]
pub struct HeroChange {
    pub same: usize,
    pub gone: Vec<String>,
    pub new: Vec<String>,
}

impl HeroChange {
    pub fn moved(&self) -> bool {
        !self.gone.is_empty() || !self.new.is_empty()
    }
}

pub fn hero_change(then: &BTreeSet<String>, now: &BTreeSet<String>) -> HeroChange {
    HeroChange { same: then.intersection(now).count(), gone: then.difference(now).cloned().collect(), new: now.difference(then).cloned().collect() }
}

/// The install-ledger owners a mod.toml speaks for: its slug (`maid-lash` installs as `maid_lash`) and any listed in
/// `[mod] owners = [...]` (a voice pak or a showcase scene installed under a name of its own).
pub fn mod_owners(toml: &str) -> Vec<String> {
    let mut out: Vec<String> = mod_slug(toml).map(|s| s.replace('-', "_")).into_iter().collect();
    for o in grade::manifest::list(toml, "mod", "owners") {
        if !out.contains(&o) {
            out.push(o);
        }
    }
    out
}

pub fn target_paths(toml: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut in_target = false;
    for line in toml.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_target = grade::manifest::section_of(t) == Some("target");
            continue;
        }
        if !in_target {
            continue;
        }
        if let Some((k, v)) = t.split_once('=') {
            let v = v.split('#').next().unwrap_or("").trim();
            if let Some(p) = v.strip_prefix('"').and_then(|x| x.split('"').next()) {
                if p.contains('/') && p.contains('.') {
                    out.push((k.trim().to_string(), p.to_string()));
                }
            }
        }
    }
    out
}

/// A source-compiled target path (`models/x/y.vmdl`) as the game stores it (`.vmdl_c`).
pub fn compiled_target(p: &str) -> String {
    if p.ends_with("_c") { p.to_string() } else { format!("{p}_c") }
}

/// Whether a mod.toml `[target]` value names a game resource (a source path the game stores compiled).
pub fn is_game_resource(p: &str) -> bool {
    ["vmdl", "vnmskel", "vnmgraph", "vnmclip", "vmat", "vpcf", "vsndevts", "vtex", "vmap"].iter().any(|e| p.ends_with(&format!(".{e}")) || p.ends_with(&format!(".{e}_c")))
}

/// The ledger rows whose file on disk is missing or isn't the file we installed: `(name, why)`.
pub fn ledger_problems(rows: &[(String, String, Option<String>)]) -> Vec<(String, String)> {
    rows.iter()
        .filter_map(|(name, crc, disk)| match disk {
            None => Some((name.clone(), "missing from addons (removed by hand, or a verify-files pass)".to_string())),
            Some(d) if d != crc => Some((name.clone(), format!("on disk crc {d}, ledger says {crc}: something else replaced our file"))),
            _ => None,
        })
        .collect()
}

// ------------------------------------------------------------------------------------------------ tests

#[cfg(test)]
mod tests {
    use super::*;
    use lint::baseline::FileInfo;

    fn base(build: &str, rows: &[(&str, u32)]) -> Baseline {
        let mut b = Baseline { build: build.into(), ..Baseline::default() };
        for (p, c) in rows {
            b.files.insert(p.to_string(), FileInfo { crc: *c, size: 1 });
        }
        b
    }
    fn mod_file(path: &str, refs: &[&str]) -> ModFile {
        ModFile { path: path.into(), stored_crc: 0, inline: true, data: resource::synth(refs) }
    }
    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }
    const CLIP: &str = "models/heroes/x/clips/run.vnmclip_c";
    const SKEL: &str = "models/heroes/x/x.vnmskel_c";
    const GRAPH: &str = "animgraphs/animgraph2/hero/hero.vnmgraph+x.vnmgraph_c";
    const PARTICLE: &str = "particles/x/fx.vpcf_c";

    fn files() -> Vec<ModFile> {
        vec![mod_file(CLIP, &["models/heroes/x/x.vnmskel"]), mod_file("models/heroes/x/x.vmdl_c", &[PARTICLE.trim_end_matches("_c")])]
    }

    #[test]
    fn identical_builds_are_no_drift() {
        let b = base("1", &[(CLIP, 1), ("models/heroes/x/x.vmdl_c", 2), (SKEL, 3), (PARTICLE, 4)]);
        let d = compute(&files(), &b, &b, None);
        assert_eq!(d.bad(), 0);
        assert!(d.verdict("1", "1").starts_with("no drift"), "{}", d.verdict("1", "1"));
    }

    #[test]
    fn a_changed_original_asks_for_a_reprove_but_still_loads() {
        let then = base("1", &[(CLIP, 1), (SKEL, 3)]);
        let now = base("2", &[(CLIP, 9), (SKEL, 3)]);
        let d = compute(&files(), &then, &now, None);
        assert_eq!(d.changed, vec![CLIP.to_string()]);
        assert_eq!(d.bad(), 0);
        assert!(d.verdict("1", "2").starts_with("loads on 2"), "{}", d.verdict("1", "2"));
    }

    #[test]
    fn drift_breaks_only_on_what_the_patch_moved() {
        // Real case (6880, the dogfood lane): a skin's long-standing hitbox changes read BROKEN on patch day though
        // Valve's model had not changed. Hitboxes are fair play, and a model Valve left alone moved nothing.
        let model = "models/heroes_wip/lash/lash.vmdl_c";
        let (then, same, moved) = (base("1", &[(model, 5)]), base("2", &[(model, 5)]), base("2", &[(model, 6)]));
        assert!(!original_changed(&then, &same, model) && original_changed(&then, &moved, model));
        assert!(original_changed(&then, &base("2", &[]), model), "gone from one side is not 'the same'");
        let found = || vec!["hitbox 'default/head' changed".to_string(), "attachment 'eye_L' missing".to_string()];
        let (breaks, own) = contract_breaks(found(), true);
        assert_eq!((breaks, own), (vec!["attachment 'eye_L' missing".to_string()], 0), "a hitbox is fair play, never a break");
        assert_eq!(contract_breaks(found(), false), (Vec::<String>::new(), 1), "Valve's model unchanged: the mod's own choice");
    }

    #[test]
    fn a_removed_original_and_a_vanished_reference_break_the_mod() {
        let then = base("1", &[(CLIP, 1), (SKEL, 3), (PARTICLE, 4)]);
        let now = base("2", &[]);
        let d = compute(&files(), &then, &now, None);
        assert_eq!(d.removed, vec![CLIP.to_string()]);
        assert!(d.broken.iter().any(|b| b.ends_with(&format!("-> {SKEL}"))), "{:?}", d.broken);
        assert!(d.verdict("1", "2").starts_with("BROKEN on 2"));
    }

    #[test]
    fn a_renamed_bone_in_a_skeleton_the_mod_uses_breaks_it() {
        let then = base("1", &[(CLIP, 1), (SKEL, 3)]);
        let now = base("2", &[(CLIP, 1), (SKEL, 4)]);
        let mut rt = Rig { build: "1".into(), ..Rig::default() };
        rt.skeletons.insert(SKEL.into(), names(&["root", "pelvis", "spine_0"]));
        let mut rn = Rig { build: "2".into(), ..Rig::default() };
        rn.skeletons.insert(SKEL.into(), names(&["root", "pelvis", "spine_a"]));
        let d = compute(&files(), &then, &now, Some((&rt, &rn)));
        assert_eq!(d.skeleton.len(), 1, "{:?}", d.skeleton);
        assert!(d.skeleton[0].contains("'spine_0' is now 'spine_a'"), "{}", d.skeleton[0]);
        assert_eq!(d.bad(), 1);
        // An added bone alone is harmless.
        rn.skeletons.insert(SKEL.into(), names(&["root", "pelvis", "spine_0", "extra"]));
        assert_eq!(compute(&files(), &then, &now, Some((&rt, &rn))).bad(), 0);
    }

    #[test]
    fn a_removed_graph_parameter_is_a_warning_not_a_break() {
        let f = vec![mod_file("models/heroes/x/x.vmdl_c", &["animgraphs/animgraph2/hero/hero.vnmgraph+x.vnmgraph"])];
        let then = base("1", &[(GRAPH, 1)]);
        let now = base("2", &[(GRAPH, 2)]);
        let mut rt = Rig::default();
        rt.graphs.insert(GRAPH.into(), names(&["move_speed", "crouch_fraction"]));
        let mut rn = Rig::default();
        rn.graphs.insert(GRAPH.into(), names(&["move_speed"]));
        let d = compute(&f, &then, &now, Some((&rt, &rn)));
        assert_eq!(d.params.len(), 1);
        assert!(d.params[0].contains("'crouch_fraction'"));
        assert_eq!(d.bad(), 0);
        assert!(d.verdict("1", "2").starts_with("loads on 2"));
    }

    #[test]
    fn broken_references_group_by_missing_target() {
        let b = vec!["a.vnmclip_c -> s.vnmskel_c".to_string(), "b.vnmclip_c -> s.vnmskel_c".to_string(), "b.vnmclip_c -> s.vnmskel_c".to_string(), "m.vmdl_c -> p.vpcf_c".to_string()];
        let g = group_broken(&b);
        assert_eq!(g.len(), 2);
        assert!(g[0].starts_with("s.vnmskel_c is gone; 2 mod files"), "{}", g[0]);
        assert_eq!(g[1], "m.vmdl_c -> p.vpcf_c");
    }

    #[test]
    fn rig_sidecar_round_trips() {
        let mut r = Rig { build: "6864".into(), ..Rig::default() };
        r.skeletons.insert(SKEL.into(), names(&["a", "b"]));
        r.graphs.insert(GRAPH.into(), names(&["p"]));
        let mut out = Vec::new();
        r.write(&mut out).unwrap();
        assert_eq!(Rig::parse(&String::from_utf8(out).unwrap()).unwrap(), r);
        assert!(Rig::parse("bogus\tx\ty\n").is_err());
    }

    fn engine(build: &str, rows: &[(&str, Option<(u64, &str)>)]) -> Engine {
        Engine { build: build.into(), files: rows.iter().map(|(p, f)| (p.to_string(), f.map(|(s, h)| (s, h.to_string())))).collect() }
    }

    #[test]
    fn the_engine_sidecar_round_trips_and_says_data_only_or_engine_too() {
        let then = engine("6866", &[("bin/win64/engine2.dll", Some((10, "aa"))), ("citadel/bin/win64/client.dll", Some((20, "bb"))), ("citadel/gameinfo.gi", None), ("core/shaders_pc_dir.vpk", Some((5, "cc")))]);
        let mut out = Vec::new();
        then.write(&mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.starts_with("# magician engine build=6866 files=4\n") && text.contains("citadel/gameinfo.gi\t-\t-\n"), "{text}");
        assert_eq!(Engine::parse(&text).unwrap(), then);
        assert!(Engine::parse("a\tb\n").is_err() && Engine::parse("a\tx\tff\n").is_err());
        // data only: every executable and shader archive the same; an unknown gameinfo is not called a change
        let d = engine_delta(&then, &engine("6868", &[("bin/win64/engine2.dll", Some((10, "aa"))), ("citadel/bin/win64/client.dll", Some((20, "bb"))), ("citadel/gameinfo.gi", Some((3, "dd"))), ("core/shaders_pc_dir.vpk", Some((5, "cc")))]));
        assert!(d.same() && d.unknown == ["citadel/gameinfo.gi"], "{d:?}");
        // engine too: a changed library, a new one, a removed shader archive
        let now = engine("6868", &[("bin/win64/engine2.dll", Some((11, "ab"))), ("bin/win64/new.dll", Some((1, "ee"))), ("citadel/bin/win64/client.dll", Some((20, "bb"))), ("citadel/gameinfo.gi", None)]);
        let d = engine_delta(&then, &now);
        assert!(!d.same());
        assert_eq!((d.changed.clone(), d.added.clone(), d.removed.clone()), (names(&["bin/win64/engine2.dll"]), names(&["bin/win64/new.dll"]), names(&["core/shaders_pc_dir.vpk"])));
        assert_eq!(d.files(), names(&["bin/win64/engine2.dll", "bin/win64/new.dll", "core/shaders_pc_dir.vpk"]));
    }

    #[test]
    fn the_engine_is_the_libraries_executables_and_shader_archives_that_ship() {
        let g = std::env::temp_dir().join(format!("magician-engine-files-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&g);
        for d in ["bin/win64", "citadel/bin/win64", "core"] {
            std::fs::create_dir_all(g.join(d)).unwrap();
        }
        for f in ["bin/win64/engine2.dll", "bin/win64/deadlock.exe", "bin/win64/crash_0001.mdmp", "bin/win64/steam_appid.txt", "citadel/bin/win64/Server.dll", "citadel/shaders_vulkan_dir.vpk", "citadel/shaders_vulkan_000.vpk", "citadel/pak01_dir.vpk", "core/shaders_pc_dir.vpk"] {
            std::fs::write(g.join(f), f).unwrap();
        }
        let got: Vec<String> = engine_files(&g).into_iter().map(|(p, _)| p).collect();
        assert_eq!(got, names(&["bin/win64/deadlock.exe", "bin/win64/engine2.dll", "citadel/bin/win64/server.dll", "citadel/shaders_vulkan_dir.vpk", "core/shaders_pc_dir.vpk"]));
        let _ = std::fs::remove_dir_all(&g);
    }

    #[test]
    fn builds_sort_numerically_and_from_defaults_to_the_one_before() {
        let b = names(&["6861", "6864", "10000"]);
        assert_eq!(default_from(&b, "10000"), "6864");
        assert_eq!(default_from(&b, "6864"), "6861");
        assert_eq!(default_from(&b, "6861"), "6861", "nothing older: compare with itself");
    }

    #[test]
    fn proven_ledger_reads_and_appends_in_place() {
        let toml = "[mod]\nslug = \"a\"\n\n[proven]\nbuilds = []   # filled by the harness\n\n[fair_play]\nnotes = \"\"\n";
        assert!(proven_builds(toml).is_empty());
        let t = add_proven(toml, "6864").unwrap();
        assert_eq!(proven_builds(&t), vec!["6864".to_string()]);
        assert!(t.contains("# filled by the harness") && t.contains("[fair_play]") && t.contains("notes = \"\""));
        let t = add_proven(&add_proven(&t, "6861").unwrap(), "6864").unwrap();
        assert_eq!(proven_builds(&t), vec!["6861".to_string(), "6864".to_string()], "sorted, no duplicates");
        assert!(add_proven("[mod]\nslug = \"a\"\n", "6864").is_err());
        assert!(add_proven(toml, "x1").is_err());
        assert!(add_proven("[proven]\nbuilds = [\n  6861,\n]\n", "6864").is_err(), "multi-line arrays are refused, not guessed at");
        // found by the long fuzz: a header grade read as [proven] and prove did not, so the build prove wrote was not
        // the one grade and patchday read
        for t in ["[[proven]\nbuilds = [6861]\n[proven]\nbuilds = [6861, 6864]\n", "[ proven ]  # spaced\nbuilds = [6861]\n"] {
            let new = add_proven(t, "6865").unwrap();
            assert!(proven_builds(&new).contains(&"6865".to_string()), "{new}");
        }
    }

    #[test]
    fn targets_and_slug_come_out_of_mod_toml() {
        let toml = "[mod]\nslug    = \"maid-lash\"\n[target]\nmodel = \"models/heroes_wip/lash/lash.vmdl\"  # note\nmaterials_dir = \"models/heroes_wip/lash/materials\"\n[proven]\nbuilds = []\n";
        assert_eq!(mod_slug(toml).as_deref(), Some("maid-lash"));
        let t = target_paths(toml);
        assert_eq!(t[0], ("model".to_string(), "models/heroes_wip/lash/lash.vmdl".to_string()));
        assert_eq!(compiled_target(&t[0].1), "models/heroes_wip/lash/lash.vmdl_c");
        assert!(is_game_resource(&t[0].1) && !is_game_resource("evidence/6861/asset-catalog.tsv") && !is_game_resource("models/heroes_wip/lash/materials"));
    }

    #[test]
    fn a_folder_of_mods_is_found_one_or_two_levels_down_whatever_it_is_called() {
        let d = std::env::temp_dir().join(format!("magician-mods-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let put = |rel: &str, text: &str| {
            let p = d.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, text).unwrap();
        };
        put("skins/b-skin/mod.toml", "[mod]\nslug = \"b-skin\"\n");
        put("skins/a-skin/mod.toml", "[mod]\nslug = \"a-skin\"\n");
        put("skins/a-skin/source/old/mod.toml", "[mod]\nslug = \"nested\"\n"); // inside a mod: not another mod
        put("skins/notes/mod.toml", "not a manifest\n");
        put("tool/crates/deep/down/mod.toml", "[mod]\nslug = \"too-deep\"\n");
        let names = |v: Vec<PathBuf>| v.iter().map(|p| p.parent().unwrap().file_name().unwrap().to_string_lossy().into_owned()).collect::<Vec<_>>();
        assert_eq!(names(mod_tomls_under(&d.join("skins"))), ["a-skin", "b-skin"], "the folder of mods itself");
        assert_eq!(names(mod_tomls_under(&d)), ["a-skin", "b-skin"], "the folder above it");
        assert!(mod_tomls_under(&d.join("tool")).is_empty(), "never deeper than two levels");
        assert!(mod_tomls_under(&d.join("missing")).is_empty());
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A heroes.vdata with one hero: `extra` goes into the block, `model` is its model.
    fn heroes_text(model: &str, extra: &str) -> String {
        format!(
            "<!-- kv3 -->\n{{\n\thero_base = \n\t{{\n\t\tm_strModelName = resource_name:\"models/base.vmdl\"\n\t}}\n\thero_x = \n\t{{\n\t\t_class = \"CitadelHeroData_t\"\n\t\tm_strModelName = resource_name:\"{model}\"\n\t\tm_mapBoundAbilities = \n\t\t{{\n\t\t\tESlot_Signature_1 = \"citadel_ability_x_one\"\n\t\t}}\n\t\tm_flHealth = 550.0\n{extra}\t}}\n\thero_y = \n\t{{\n\t\tm_strModelName = resource_name:\"models/y.vmdl\"\n\t}}\n}}\n"
        )
    }

    #[test]
    fn a_balance_patch_changes_none_of_the_names_a_skin_leans_on() {
        // The real 6865 -> 6866 patch: Valve took a table of item weights out of every hero's block. Half the block
        // went, and nothing a skin names moved.
        let table = "\t\tm_mapItemWeights = \n\t\t{\n\t\t\tupgrade_a = \n\t\t\t{\n\t\t\t\tm_strBucket = \"Normal\"\n\t\t\t\tm_flWeight = 1.0\n\t\t\t}\n\t\t}\n";
        let then = hero_names(&heroes_text("models/x/x.vmdl", table), "hero_x").unwrap();
        let now = hero_names(&heroes_text("models/x/x.vmdl", ""), "hero_x").unwrap();
        assert!(then.contains("models/x/x.vmdl") && then.contains("citadel_ability_x_one") && !then.contains("Normal"), "{then:?}");
        assert!(!then.contains("models/y.vmdl") && !then.contains("models/base.vmdl"), "only the hero's own block: {then:?}");
        let c = hero_change(&then, &now);
        assert!(!c.moved(), "{c:?}");
        assert_eq!(c.same, then.len());
    }

    #[test]
    fn a_moved_model_or_a_renamed_ability_is_named() {
        let then = hero_names(&heroes_text("models/x/x.vmdl", ""), "hero_x").unwrap();
        let now_text = heroes_text("models/x_v2/x.vmdl", "").replace("citadel_ability_x_one", "citadel_ability_x_uno");
        let c = hero_change(&then, &hero_names(&now_text, "hero_x").unwrap());
        assert_eq!(c.gone, ["citadel_ability_x_one", "models/x/x.vmdl"]);
        assert_eq!(c.new, ["citadel_ability_x_uno", "models/x_v2/x.vmdl"]);
        assert!(hero_names(&now_text, "hero_gone").is_none(), "a hero Valve removed has no block");
        assert_eq!(mod_hero("[mod]\nslug = \"s\"\nhero    = \"hero_x\"   # key\n").as_deref(), Some("hero_x"));
    }

    #[test]
    fn a_mod_is_compared_from_its_last_proven_build() {
        let on_record = names(&["6861", "6863", "6864", "6865"]);
        // proven on the installed build: nothing to compare, it is current
        assert_eq!(era_for(&names(&["6861", "6865"]), &on_record, "6865"), ("6865".to_string(), None));
        // proven on an older build that is on record: from there, not from the newest older baseline
        assert_eq!(era_for(&names(&["6861"]), &on_record, "6865"), ("6861".to_string(), None));
        assert_eq!(era_for(&names(&["6861", "6863"]), &on_record, "6865").0, "6863");
        // a later proof than the installed build (the game went back) does not count
        assert_eq!(era_for(&names(&["6863", "6866"]), &on_record, "6865").0, "6863");
        // the last proven build has no baseline: the newest older one, and the reason
        let (from, why) = era_for(&names(&["6862"]), &on_record, "6865");
        assert_eq!(from, "6864");
        assert!(why.unwrap().contains("6862"));
        // never proven: the newest older baseline, said so
        let (from, why) = era_for(&[], &on_record, "6865");
        assert_eq!(from, "6864");
        assert!(why.unwrap().contains("no proven build"));
    }

    #[test]
    fn a_mod_toml_claims_its_slug_and_listed_owners() {
        let toml = "[mod]
slug = \"example-skin\"
owners = [\"example_voice\", \"example_showcase\"]  # paks it installs
[proven]
builds = [6864]
";
        assert_eq!(mod_owners(toml), names(&["example_skin", "example_voice", "example_showcase"]));
        assert_eq!(mod_owners("[mod]
slug = \"maid-lash\"
"), names(&["maid_lash"]));
        assert!(mod_owners("[proven]
builds = []
").is_empty());
    }

    #[test]
    fn drift_states_match_their_verdicts() {
        let mut d = Drift::default();
        assert_eq!(d.state(), "clean");
        d.changed.push(CLIP.into());
        assert_eq!(d.state(), "re-prove");
        d.removed.push(CLIP.into());
        assert_eq!(d.state(), "broken");
        let mut p = Drift::default();
        p.params.push("x".into());
        assert_eq!(p.state(), "re-prove", "a lost graph parameter is a warning: re-prove, not broken");
    }

    #[test]
    fn ledger_problems_name_a_missing_and_a_replaced_file() {
        let rows = vec![
            ("pak10_dir.vpk".to_string(), "aa".to_string(), Some("aa".to_string())),
            ("pak11_dir.vpk".to_string(), "bb".to_string(), None),
            ("pak12_dir.vpk".to_string(), "cc".to_string(), Some("dd".to_string())),
        ];
        let p = ledger_problems(&rows);
        assert_eq!(p.len(), 2);
        assert!(p[0].1.contains("missing") && p[1].1.contains("something else"));
    }
}
