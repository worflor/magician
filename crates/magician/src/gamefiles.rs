//! The installed game as lint's `Game`: Valve's files by path (citadel first, then core, as the search
//! path mounts them) and the build's users index.
//!
//! The users index (`magician index` -> `.reference/<build>/users.tsv`) answers "who in the game uses this
//! file": every hero, ability or item whose data reaches it, following the reference graph backwards.
//! Owners come from the game's own data: the top-level keys of `scripts/*.vdata` (an ability named by a
//! hero belongs to that hero), plus hero path tokens (`lash`, `archer`...) for files no vdata names.
//! Heroes Valve has disabled, keeps in development (`hero_skyrunner` wears Lash's model), marks
//! `_not_pickable` (the `hero_base` template) or gives a development state other than release (the target dummy
//! is `EHeroDevState_DebugOnly`) are left out: nobody can pick them in a match.
//!
//! A base animation graph lists every hero's variation of it (`hero_flinch.vnmgraph_c` names
//! `hero_flinch.vnmgraph+lash.vnmgraph_c` and 40 more): that list is not a use. Followed backwards, it made the one
//! hero whose graphs take the base flinch graph unvaried a user of every hero's flinch clips and skeleton, and so a
//! sharer the shared-file rule named on every skin. Those edges are left out.

use grade::footprint::compiled;
use resource::kv3::Value;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::Path;

/// The users index format. An index without this revision in its header was built before base graphs stopped
/// lending their variations' owners (and before the target dummy was left out): it names owners that do not use
/// the file, so it is not read, and lint says so (M091).
pub const INDEX_REV: u32 = 2;

pub struct GameFiles {
    pub paks: Vec<std::sync::Arc<vpk::Vpk>>,
    pub users: HashMap<String, Vec<String>>,
    /// Why the shared-file rule could not run, when it could not: no index, or one in an older format.
    pub index_missing: Option<String>,
    pub particle_classes: Option<lint::deep::ParticleClasses>,
    pub material_params: Option<lint::matparams::MaterialParams>,
    /// The archives' sizes and times: what keys cached results to this install of the game.
    pub identity: String,
    /// The addon content trees the mod was compiled from, for M032.
    pub addon_sources: Option<HashSet<String>>,
}

impl GameFiles {
    pub fn open(game: &Path, index: Option<&Path>) -> GameFiles {
        let paks = ["citadel", "core"].iter().filter_map(|d| vpk::Vpk::open_shared(game.join(d).join("pak01_dir.vpk")).ok()).collect();
        let mut users = HashMap::new();
        let mut index_missing = None;
        match index.map(|p| (p, std::fs::read_to_string(p))) {
            None => index_missing = Some("no users index was given".to_string()),
            Some((p, Err(_))) => index_missing = Some(format!("there is no users index for this build ({}); `magician index` builds it", p.display())),
            Some((p, Ok(text))) if index_rev(&text) < INDEX_REV => {
                index_missing = Some(format!("the users index ({}) is from an older Magician and names users that do not use the file; `magician index` builds it again", p.display()))
            }
            Some((_, Ok(text))) => {
                for line in text.lines().filter(|l| !l.starts_with('#')) {
                    if let Some((t, o)) = line.split_once('\t') {
                        users.insert(t.to_string(), o.split(',').map(str::to_string).collect());
                    }
                }
            }
        }
        let identity = ["citadel", "core"]
            .iter()
            .map(|d| {
                let f = game.join(d).join("pak01_dir.vpk");
                match std::fs::metadata(&f) {
                    Ok(m) => {
                        let t = m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs());
                        format!("{d}:{}:{t}", m.len())
                    }
                    Err(_) => format!("{d}:none"),
                }
            })
            .collect::<Vec<_>>()
            .join(",");
        GameFiles { paks, users, index_missing, particle_classes: None, material_params: None, identity, addon_sources: None }
    }
}

impl lint::Game for GameFiles {
    fn read(&self, path: &str) -> Option<Vec<u8>> {
        for p in &self.paks {
            if let Some(e) = p.find(path) {
                return p.read(e).ok();
            }
        }
        None
    }
    fn users(&self, path: &str) -> Vec<String> {
        self.users.get(path).cloned().unwrap_or_default()
    }
    fn users_unchecked(&self) -> Option<String> {
        self.index_missing.clone()
    }
    fn particle_classes(&self) -> Option<&lint::deep::ParticleClasses> {
        self.particle_classes.as_ref()
    }
    fn material_params(&self) -> Option<&lint::matparams::MaterialParams> {
        self.material_params.as_ref()
    }
    fn addon_sources(&self) -> Option<&HashSet<String>> {
        self.addon_sources.as_ref()
    }
    fn identity(&self) -> String {
        self.identity.clone()
    }
}

/// The particle classes Valve uses per function list, from `cache` when present, else by decoding every
/// game particle once (about a minute on this HDD) and writing the cache. One cache per build.
pub fn particle_classes(game: &Path, cache: &Path, build: &str) -> Result<lint::deep::ParticleClasses, String> {
    if let Ok(text) = std::fs::read_to_string(cache) {
        if text.starts_with(&format!("# magician particle classes build={build}")) {
            return Ok(lint::deep::parse_particle_classes(&text));
        }
    }
    say!("note: collecting Valve's particle classes for build {build} (once per build) -> {}", cache.display());
    let pak = vpk::Vpk::open(game.join("citadel").join("pak01_dir.vpk")).map_err(|e| e.to_string())?;
    let mut todo: Vec<&vpk::Entry> = pak.entries.iter().filter(|e| e.path.ends_with(".vpcf_c")).collect();
    todo.sort_by_key(|e| (e.archive_index, e.offset));
    let mut c = lint::deep::ParticleClasses::new();
    let mut n = 0;
    for e in todo {
        if let Some(v) = pak.read(e).ok().and_then(|d| data_of(&d)) {
            lint::deep::collect_particle_classes(&v, &mut c);
            n += 1;
        }
    }
    if n == 0 {
        return Err("no game particles decoded".into());
    }
    if let Some(d) = cache.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    std::fs::write(cache, lint::deep::particle_classes_tsv(&c, build)).map_err(|e| format!("{}: {e}", cache.display()))?;
    Ok(c)
}

/// The names Valve's materials set per shader (lint::matparams), from `cache` when present, else by reading every
/// compiled material of the game once and writing the cache. One cache per build.
pub fn material_params(game: &Path, cache: &Path, build: &str) -> Result<lint::matparams::MaterialParams, String> {
    if let Ok(text) = std::fs::read_to_string(cache) {
        if text.starts_with(&format!("{}\n", lint::matparams::header(build))) {
            return Ok(lint::matparams::parse_tsv(&text));
        }
    }
    say!("note: collecting the parameters Valve's materials set for build {build} (once per build) -> {}", cache.display());
    let mut p = lint::matparams::MaterialParams::new();
    let mut n = 0;
    for pak in ["citadel", "core"] {
        let Ok(v) = vpk::Vpk::open(game.join(pak).join("pak01_dir.vpk")) else { continue };
        let mut todo: Vec<&vpk::Entry> = v.entries.iter().filter(|e| e.path.ends_with(".vmat_c")).collect();
        todo.sort_by_key(|e| (e.archive_index, e.offset));
        for e in todo {
            if let Some(m) = v.read(e).ok().and_then(|d| resource::material::from_compiled(&d).ok()) {
                lint::matparams::collect(&m, &mut p);
                n += 1;
            }
        }
    }
    if n == 0 {
        return Err("no game materials decoded".into());
    }
    if let Some(d) = cache.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    std::fs::write(cache, lint::matparams::to_tsv(&p, build)).map_err(|e| format!("{}: {e}", cache.display()))?;
    Ok(p)
}
/// The format revision a users index's header names (`rev=N`): 1 for an index written before the header carried one,
/// 0 for a file that is not an index.
pub fn index_rev(text: &str) -> u32 {
    let head = text.lines().next().unwrap_or("");
    if !head.starts_with("# magician users ") {
        return 0;
    }
    head.split_whitespace().find_map(|w| w.strip_prefix("rev=")).and_then(|r| r.parse().ok()).unwrap_or(1)
}

/// Whether `target` is one of `from`'s variations: a base graph `a/x.vnmgraph_c` lists `a/x.vnmgraph+hero.vnmgraph_c`
/// for every hero that varies it, which is not a use of them.
fn variation_of(from: &str, target: &str) -> bool {
    from.strip_suffix("_c").is_some_and(|base| base.ends_with(".vnmgraph") && !base.contains('+') && target.strip_prefix(base).is_some_and(|rest| rest.starts_with('+')))
}

fn data_of(bytes: &[u8]) -> Option<Value> {
    resource::kv3_block(bytes, b"DATA").and_then(Result::ok)
}

/// Every string in a value that names a game file, as the compiled path the engine looks up.
fn named_files(v: &Value) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for s in v.strings() {
        let l = s.to_ascii_lowercase();
        let looks = l.starts_with("file://{images}/")
            || [".vmdl", ".vpcf", ".vsndevts", ".vmat", ".vtex", ".psd", ".png", ".svg", ".vsnd", ".vcss", ".vxml", ".vjs", ".vnmgraph", ".vnmskel"]
                .iter()
                .any(|e| l.ends_with(e));
        if looks && !l.contains(' ') {
            out.insert(compiled(&l));
        }
    }
    out
}

/// Where a hero token first appears in a path as a whole word (letters on neither side).
fn token_pos(path: &str, token: &str) -> Option<usize> {
    let p = path.as_bytes();
    let mut from = 0;
    while let Some(i) = path[from..].find(token) {
        let a = from + i;
        let b = a + token.len();
        if (a == 0 || !p[a - 1].is_ascii_alphabetic()) && (b >= p.len() || !p[b].is_ascii_alphabetic()) {
            return Some(a);
        }
        from = a + 1;
    }
    None
}

/// The hero a path is named for: the leftmost hero token. `sounds/vo/lash/lash_kill_wraith_01` is Lash's
/// line about Wraith, not Wraith's.
fn owner_by_token<'a>(path: &str, tokens: &'a [(String, String)]) -> Option<&'a String> {
    tokens.iter().filter_map(|(t, h)| token_pos(path, t).map(|p| (p, h))).min_by_key(|(p, _)| *p).map(|(_, h)| h)
}

pub struct IndexStats {
    pub files: usize,
    pub edges: usize,
    pub owners: usize,
    pub targets: usize,
}

/// Build the users index for the installed build: reverse references over every compiled file, owners from
/// vdata and hero tokens, written as `target<TAB>owner,owner,...`.
/// `src` decompiles what the native reader can't: build 6864 ships `heroes.vdata` and `abilities.vdata` (only
/// those 2 of its 84 vdata files) zstd-compressed.
pub fn build_index(game: &Path, out: &Path, build: &str, src: &grade::footprint::Sources) -> Result<IndexStats, String> {
    let pak = vpk::Vpk::open(game.join("citadel").join("pak01_dir.vpk")).map_err(|e| e.to_string())?;
    let mut rev: HashMap<String, Vec<u32>> = HashMap::new();
    let mut names: Vec<String> = Vec::new();
    let mut id_of: HashMap<String, u32> = HashMap::new();
    let mut intern = |s: &str, names: &mut Vec<String>| -> u32 {
        if let Some(&i) = id_of.get(s) {
            return i;
        }
        names.push(s.to_string());
        id_of.insert(s.to_string(), names.len() as u32 - 1);
        names.len() as u32 - 1
    };
    // Textures and sound files reference nothing (no RERL), and they are 110k of the 160k files and 33 of the
    // 35 GB: skipping them keeps the pass to small files and one seek each.
    let mut todo: Vec<&vpk::Entry> = pak
        .entries
        .iter()
        .filter(|e| e.path.ends_with("_c") && !e.path.ends_with(".vtex_c") && !e.path.ends_with(".vsnd_c"))
        .collect();
    todo.sort_by_key(|e| (e.archive_index, e.offset));
    let mut edges = 0usize;
    for e in &todo {
        let refs: Vec<String> = if e.path.ends_with(".vsndevts_c") {
            let Ok(d) = pak.read(e) else { continue };
            let mut r: Vec<String> = resource::parse(&d).map(|r| lint::ref_targets(&r)).unwrap_or_default();
            if let Some(v) = data_of(&d) {
                r.extend(lint::deep::vsnd_refs(&v));
            }
            r
        } else if e.size() <= 256 * 1024 {
            // small file: one read
            match pak.read(e).ok().and_then(|d| resource::parse(&d).ok()) {
                Some(r) => lint::ref_targets(&r),
                None => continue,
            }
        } else {
            let mut read = |off: u64, len: u64| pak.read_range(e, off, len).ok();
            match resource::refs_by_range(&mut read) {
                Some(r) => r.iter().filter(|x| !x.name.is_empty() && !x.name.starts_with('_')).map(|x| x.compiled_path()).collect(),
                None => continue,
            }
        };
        let from = intern(&e.path, &mut names);
        for t in refs {
            if t != e.path && !variation_of(&e.path, &t) {
                rev.entry(t).or_default().push(from);
                edges += 1;
            }
        }
    }

    // Owners from vdata.
    let read = |p: &str| -> Result<Value, String> {
        if let Some(v) = pak.find(p).and_then(|e| pak.read(e).ok()).and_then(|d| data_of(&d)) {
            return Ok(v);
        }
        resource::kv3::parse_text(&src.text(p)?)
    };
    let heroes = read("scripts/heroes.vdata_c")?;
    let abilities = read("scripts/abilities.vdata_c")?;
    let ability_keys: HashSet<&str> = abilities.members().iter().map(|(k, _)| k.as_str()).collect();
    let mut direct: HashMap<String, BTreeSet<String>> = HashMap::new();
    let mut token_owner: Vec<(String, String)> = Vec::new();
    let mut ability_hero: HashMap<String, BTreeSet<String>> = HashMap::new();
    let mut dead_abilities: HashSet<String> = HashSet::new();
    let mut dead_tokens: Vec<String> = Vec::new();
    for (key, h) in heroes.members() {
        if !key.starts_with("hero_") {
            continue;
        }
        let off = |k: &str| h.get(k).and_then(Value::as_bool).unwrap_or(false);
        // `_not_pickable` marks templates (hero_base) and test units (hero_targetdummy).
        let pickable = h.get("_not_pickable").and_then(Value::as_f64).unwrap_or(0.0) == 0.0;
        // the target dummy is `EHeroDevState_DebugOnly`; every hero in a match is `EHeroDevState_Release`
        let released = h.get("m_eHeroDevelopmentState").and_then(Value::as_str).is_none_or(|s| s == "EHeroDevState_Release");
        let live = pickable && released && !off("m_bDisabled") && !off("m_bInDevelopment");
        let strs: Vec<&str> = h.strings();
        for s in &strs {
            if ability_keys.contains(s) {
                if live {
                    ability_hero.entry(s.to_string()).or_default().insert(key.clone());
                } else {
                    dead_abilities.insert(s.to_string());
                }
            }
        }
        if !live {
            dead_tokens.push(key.trim_start_matches("hero_").to_string());
            continue;
        }
        for f in named_files(h) {
            direct.entry(f).or_default().insert(key.clone());
        }
        let mut tokens = vec![key.trim_start_matches("hero_").to_string()];
        if let Some(m) = h.get("m_strModelName").and_then(Value::as_str) {
            let mp = Path::new(m);
            if let Some(s) = mp.file_stem() {
                tokens.push(s.to_string_lossy().into_owned());
            }
            if let Some(d) = mp.parent().and_then(|d| d.file_name()) {
                tokens.push(d.to_string_lossy().trim_end_matches("_v2").trim_end_matches("_v3").to_string());
            }
        }
        for t in tokens.into_iter().filter(|t| t.len() >= 3) {
            if !token_owner.iter().any(|(x, _)| x == &t) {
                token_owner.push((t, key.clone()));
            }
        }
    }
    // Abilities a hero's abilities name (sub-abilities: a clone's turret, a curse's projectile) are the hero's too.
    for _ in 0..3 {
        let mut add: Vec<(String, BTreeSet<String>)> = Vec::new();
        for (key, a) in abilities.members() {
            if let Some(h) = ability_hero.get(key) {
                for s in a.strings() {
                    if ability_keys.contains(s) && s != key && !ability_hero.contains_key(s) {
                        add.push((s.to_string(), h.clone()));
                    }
                }
            }
        }
        if add.is_empty() {
            break;
        }
        for (k, h) in add {
            ability_hero.entry(k).or_default().extend(h);
        }
    }
    for (key, a) in abilities.members() {
        let owners: BTreeSet<String> = match ability_hero.get(key) {
            Some(h) => h.clone(),
            None if dead_abilities.contains(key) => continue,
            // An unbound ability named for a hero (`ability_magician_escape`) is that hero's; one named for a hero
            // nobody can pick (`ability_boho_*`) is nobody's.
            None => match owner_by_token(key, &token_owner) {
                Some(h) => BTreeSet::from([h.clone()]),
                None if dead_tokens.iter().any(|t| token_pos(key, t).is_some()) => continue,
                None => BTreeSet::from([key.clone()]),
            },
        };
        for f in named_files(a) {
            direct.entry(f).or_default().extend(owners.iter().cloned());
        }
    }
    for e in pak.entries.iter().filter(|e| e.path.starts_with("scripts/") && e.path.ends_with(".vdata_c")) {
        if e.path == "scripts/heroes.vdata_c" || e.path == "scripts/abilities.vdata_c" {
            continue;
        }
        let Some(v) = pak.read(e).ok().and_then(|d| data_of(&d)) else { continue };
        let stem = e.path.trim_start_matches("scripts/").trim_end_matches(".vdata_c").to_string();
        for (key, x) in v.members() {
            for f in named_files(x) {
                direct.entry(f).or_default().insert(format!("{stem}:{key}"));
            }
        }
    }

    // Transitive owners over the reverse graph.
    fn owners_of(
        t: &str,
        rev: &HashMap<String, Vec<u32>>,
        names: &[String],
        direct: &HashMap<String, BTreeSet<String>>,
        tokens: &[(String, String)],
        memo: &mut HashMap<String, BTreeSet<String>>,
        busy: &mut HashSet<String>,
        depth: usize,
    ) -> BTreeSet<String> {
        if let Some(m) = memo.get(t) {
            return m.clone();
        }
        if depth > 24 || !busy.insert(t.to_string()) {
            return BTreeSet::new();
        }
        let mut o = direct.get(t).cloned().unwrap_or_default();
        if let Some(h) = owner_by_token(t, tokens) {
            o.insert(h.clone());
        }
        if let Some(fr) = rev.get(t) {
            for &f in fr {
                let from = &names[f as usize];
                // A sound file is played by the events that name it, and an event file's events belong to the
                // hero it is named for, whoever else loads the file (Doorman's data names lash.vsndevts).
                if from.ends_with(".vsndevts_c") {
                    if let Some(h) = owner_by_token(from, tokens) {
                        o.insert(h.clone());
                        continue;
                    }
                }
                let up = owners_of(from, rev, names, direct, tokens, memo, busy, depth + 1);
                o.extend(up);
            }
        }
        busy.remove(t);
        memo.insert(t.to_string(), o.clone());
        o
    }
    let mut memo = HashMap::new();
    let mut busy = HashSet::new();
    let mut all: BTreeSet<String> = pak.entries.iter().filter(|e| e.path.ends_with("_c")).map(|e| e.path.clone()).collect();
    all.extend(direct.keys().cloned());
    let mut lines = Vec::new();
    let mut owner_set = BTreeSet::new();
    for t in &all {
        let o = owners_of(t, &rev, &names, &direct, &token_owner, &mut memo, &mut busy, 0);
        if !o.is_empty() {
            owner_set.extend(o.iter().cloned());
            lines.push(format!("{t}\t{}", o.into_iter().collect::<Vec<_>>().join(",")));
        }
    }
    if let Some(d) = out.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    let mut text = format!("# magician users build={build} rev={INDEX_REV} targets={}\n", lines.len());
    text.push_str(&lines.join("\n"));
    text.push('\n');
    std::fs::write(out, text).map_err(|e| format!("{}: {e}", out.display()))?;
    Ok(IndexStats { files: todo.len(), edges, owners: owner_set.len(), targets: lines.len() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leftmost_hero_token_names_the_owner() {
        let t = vec![("wraith".to_string(), "hero_wraith".to_string()), ("lash".to_string(), "hero_lash".to_string())];
        assert_eq!(owner_by_token("sounds/vo/lash/lash_kill_wraith_01.vsnd_c", &t).map(String::as_str), Some("hero_lash"));
        assert_eq!(owner_by_token("particles/abilities/wraith/x.vpcf_c", &t).map(String::as_str), Some("hero_wraith"));
        assert_eq!(owner_by_token("particles/generic/splash.vpcf_c", &t), None, "whole words only");
    }

    #[test]
    fn a_base_graph_does_not_use_its_variations() {
        let base = "animgraphs/animgraph2/hero/hero_flinch.vnmgraph_c";
        assert!(variation_of(base, "animgraphs/animgraph2/hero/hero_flinch.vnmgraph+lash.vnmgraph_c"));
        assert!(!variation_of(base, "models/heroes_wip/lash/lash.vnmskel_c"), "what the base graph itself uses");
        assert!(!variation_of(base, "animgraphs/animgraph2/hero/hero_flinch_heavy.vnmgraph+lash.vnmgraph_c"), "another graph");
        let variation = "animgraphs/animgraph2/hero/hero_flinch.vnmgraph+lash.vnmgraph_c";
        assert!(!variation_of(variation, "animgraphs/animgraph2/hero/hero_flinch.vnmgraph+lash.vnmgraph+x.vnmgraph_c"), "only a base graph lists variations");
    }

    #[test]
    fn an_index_from_before_the_revision_is_not_read() {
        assert_eq!(index_rev("# magician users build=6868 targets=3\na\thero_lash\n"), 1);
        assert_eq!(index_rev(&format!("# magician users build=6868 rev={INDEX_REV} targets=3\n")), INDEX_REV);
        assert_eq!(index_rev("a\thero_lash\n"), 0, "not an index");
        let dir = std::env::temp_dir().join(format!("magician-users-rev-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let idx = dir.join("users.tsv");
        std::fs::write(&idx, "# magician users build=6868 targets=1\na.vtex_c\thero_lash,hero_haze\n").unwrap();
        let g = GameFiles::open(&dir, Some(&idx));
        assert!(g.users.is_empty());
        assert!(g.index_missing.as_deref().is_some_and(|m| m.contains("older Magician")), "{:?}", g.index_missing);
        std::fs::write(&idx, format!("# magician users build=6868 rev={INDEX_REV} targets=1\na.vtex_c\thero_lash,hero_haze\n")).unwrap();
        let g = GameFiles::open(&dir, Some(&idx));
        assert!(g.index_missing.is_none() && g.users.len() == 1);
        let g = GameFiles::open(&dir, Some(&dir.join("missing.tsv")));
        assert!(g.index_missing.as_deref().is_some_and(|m| m.contains("no users index")), "{:?}", g.index_missing);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
