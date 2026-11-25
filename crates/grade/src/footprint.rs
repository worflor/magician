//! A hero's footprint: every game file that makes up that hero's look, sound and feel, discovered
//! from the game's own data rather than a hand-kept list.
//!
//! Roots come from the hero's block in `scripts/heroes.vdata` (model, sound-event files, panorama
//! art, UI scene maps, ability slots) and each ability's block in `scripts/abilities.vdata`
//! (particles, sound events, HUD icon). From the model, the compiled dependency tables (RERL) are
//! walked to reach materials, textures, skeleton, anim graphs and clips. Sound-event files keep their
//! `.vsnd` references inside the KV3 data block, so they're read from the decompiled text.
//!
//! "Owned" means the path carries one of the hero's tokens (`lash`, `vampirebat`, ...). Shared files
//! such as `particles/generic/*` belong to everyone, and a hero overhaul isn't graded on them.

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Cat {
    Model,
    Materials,
    /// Must never be shipped: replacing it breaks the hero on the next update.
    Skeleton,
    /// Must never be shipped (AnimGraph2 isn't editable; the hero's own graph has to load).
    AnimGraph,
    /// Gameplay-script data (vdata): must never be shipped.
    Data,
    Animation,
    AbilityVfx,
    Sound,
    Voice,
    UiArt,
    Scene,
}

impl Cat {
    pub fn label(self) -> &'static str {
        match self {
            Cat::Model => "model",
            Cat::Materials => "materials",
            Cat::Skeleton => "skeleton",
            Cat::AnimGraph => "anim graph",
            Cat::Data => "game data",
            Cat::Animation => "animation",
            Cat::AbilityVfx => "ability vfx",
            Cat::Sound => "sound",
            Cat::Voice => "voice",
            Cat::UiArt => "ui art",
            Cat::Scene => "scene",
        }
    }
    pub fn protected(self) -> bool {
        matches!(self, Cat::Skeleton | Cat::AnimGraph | Cat::Data)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Item {
    pub cat: Cat,
    /// What the item belongs to: an ability key, "weapon", "core", "movement", "ui", ...
    pub group: String,
    /// Compiled path as a mod would ship it (or a map VPK path for scenes).
    pub path: String,
}

#[derive(Debug, Default)]
pub struct Footprint {
    pub hero_key: String,
    pub display: String,
    pub tokens: Vec<String>,
    pub abilities: Vec<(String, String)>, // (slot, ability key)
    pub items: Vec<Item>,
    /// The hero's name logos (`panorama/images/heroes/hero_names/...`), whatever their tokens: a new one is a rename.
    pub name_art: Vec<String>,
}

impl Footprint {
    pub fn by_cat(&self, c: Cat) -> impl Iterator<Item = &Item> {
        self.items.iter().filter(move |i| i.cat == c)
    }
    pub fn cat_of(&self, path: &str) -> Option<Cat> {
        self.items.iter().find(|i| i.path == path).map(|i| i.cat)
    }
}

/// All "..." contents in a text block.
pub fn quoted(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(a) = rest.find('"') {
        let after = &rest[a + 1..];
        match after.find('"') {
            Some(b) => {
                out.push(after[..b].to_string());
                rest = &after[b + 1..];
            }
            None => break,
        }
    }
    out
}

/// The block of a top-level key in a decompiled vdata/KV3 file (`\t<key> =` up to the next
/// top-level key).
pub fn top_level_block<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("\n\t{key} =");
    let start = text.find(&needle)? + 1;
    let body = &text[start + needle.len() - 1..];
    let mut end = body.len();
    let mut off = 0;
    for line in body.split_inclusive('\n') {
        if off > 0 && line.starts_with('\t') && !line.starts_with("\t\t") && !line.starts_with("\t}") && line.contains(" =") {
            end = off;
            break;
        }
        off += line.len();
    }
    Some(&text[start..start + (needle.len() - 1) + end])
}

/// Whole-word token match: `lash` matches `lash_flog`, `heroes_wip/lash/` and `lash_v2`, but not
/// `muzzleflashx`, `splash` or `slash`. Letters on either side break the match; digits, `_`, `/`,
/// `.` and `-` don't.
pub fn has_token(path: &str, token: &str) -> bool {
    let p = path.as_bytes();
    let mut from = 0;
    while let Some(i) = path[from..].find(token) {
        let a = from + i;
        let b = a + token.len();
        let before_ok = a == 0 || !p[a - 1].is_ascii_alphabetic();
        let after_ok = b >= p.len() || !p[b].is_ascii_alphabetic();
        if before_ok && after_ok {
            return true;
        }
        from = a + 1;
    }
    false
}

/// Map a source path to the compiled path Deadlock looks up.
/// Panorama art gets its extension folded into the name: `x.psd` → `x_psd.vtex_c`, `x.svg` → `x.vsvg_c`.
pub fn compiled(src: &str) -> String {
    let s = src.trim_start_matches("file://{images}/");
    let s = if src.starts_with("file://{images}/") { format!("panorama/images/{s}") } else { s.to_string() };
    let lower = s.to_ascii_lowercase();
    for ext in ["psd", "png", "tga", "jpg"] {
        if let Some(stem) = lower.strip_suffix(&format!(".{ext}")) {
            return format!("{stem}_{ext}.vtex_c");
        }
    }
    if let Some(stem) = lower.strip_suffix(".svg") {
        return format!("{stem}.vsvg_c");
    }
    if lower.ends_with("_c") { lower } else { format!("{lower}_c") }
}

pub struct Sources {
    pub game: PathBuf,
    pub decompiled: PathBuf,
    pub vrf_cli: PathBuf,
}

impl Sources {
    fn vpk(&self) -> PathBuf {
        self.game.join("citadel").join("pak01_dir.vpk")
    }
    /// Decompiled text of a game file, decompiling it with Source 2 Viewer on first use.
    pub fn text(&self, compiled_path: &str) -> Result<String, String> {
        let src = compiled_path.trim_end_matches("_c");
        let out = self.decompiled.join(src.replace('/', std::path::MAIN_SEPARATOR_STR));
        if !out.exists() {
            // Source 2 Viewer writes exceptions.txt into its working folder when a decompile fails: that is the
            // reference folder's business, never the folder the user ran Magician from (a mod's own repository).
            // Every path it is given is absolute, so the change of folder can't change what they name.
            // Two grades at once (two pipelines, or tests in parallel) decompiling the same file into the cache would
            // each have the viewer write the other's file: each decompiles into a folder of its own, and the file is
            // renamed into the cache, whole, by whichever finishes first.
            let abs = |p: &Path| std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf());
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let dir = abs(&self.decompiled).join(format!(".decompiling-{}-{n}", std::process::id()));
            std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
            let cli = if self.vrf_cli.components().count() > 1 { abs(&self.vrf_cli) } else { self.vrf_cli.clone() };
            let st = std::process::Command::new(&cli)
                .current_dir(&dir)
                .args(["-i"]).arg(abs(&self.vpk()))
                .args(["-f", compiled_path, "-d", "-o"]).arg(&dir)
                .arg("--quiet")
                // it still says "--- Wrote 1 files" on stdout, which is the caller's: one JSON document under --json
                .stdout(std::process::Stdio::null())
                .status();
            let made = dir.join(src.replace('/', std::path::MAIN_SEPARATOR_STR));
            let moved = match &st {
                Ok(s) if s.success() && made.is_file() => {
                    if let Some(parent) = out.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    std::fs::rename(&made, &out).is_ok() || out.is_file()
                }
                _ => false,
            };
            let _ = std::fs::remove_dir_all(&dir);
            st.map_err(|e| {
                // A bare name that no PATH folder holds can come back as "permission denied" (an unreadable folder
                // on PATH, as Windows' own folders are under WSL): say it is missing, which is what it is.
                let bare = self.vrf_cli.components().count() <= 1;
                let found = bare && std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(&self.vrf_cli).is_file() || d.join(format!("{}.exe", self.vrf_cli.display())).is_file()));
                if bare && !found {
                    format!("no {} on PATH: install Source 2 Viewer's CLI and set MAGICIAN_VRF (or `vrf =` in the config file) to it", self.vrf_cli.display())
                } else {
                    format!("running {}: {e}", self.vrf_cli.display())
                }
            })?;
            if !moved {
                return Err(format!("could not decompile {compiled_path}"));
            }
        }
        std::fs::read_to_string(&out).map_err(|e| format!("{}: {e}", out.display()))
    }
}

/// Resolve "lash", "hero_lash", or a display name ("Sinclair") to a heroes.vdata key.
pub fn resolve_hero(heroes: &str, localization: &str, arg: &str) -> Option<(String, String)> {
    let a = arg.to_ascii_lowercase();
    let mut names: HashMap<String, String> = HashMap::new();
    for line in localization.lines() {
        let q = quoted(line);
        if q.len() >= 2 {
            if let Some(k) = q[0].strip_suffix(":n") {
                names.entry(k.to_string()).or_insert_with(|| q[1].clone());
            }
        }
    }
    let key = if heroes.contains(&format!("\n\t{a} =")) {
        a.clone()
    } else if heroes.contains(&format!("\n\thero_{a} =")) {
        format!("hero_{a}")
    } else {
        names.iter().find(|(k, v)| v.to_ascii_lowercase() == a && heroes.contains(&format!("\n\t{k} =")))?.0.clone()
    };
    let display = names.get(&key).cloned().unwrap_or_else(|| key.trim_start_matches("hero_").to_string());
    Some((key, display))
}

pub fn build(src: &Sources, arg: &str) -> Result<Footprint, String> {
    let heroes = src.text("scripts/heroes.vdata_c")?;
    let abilities = src.text("scripts/abilities.vdata_c")?;
    let loc_path = src.game.join("citadel/resource/localization/citadel_gc_hero_names/citadel_gc_hero_names_english.txt");
    let loc = std::fs::read_to_string(&loc_path).unwrap_or_default();
    let (key, display) = resolve_hero(&heroes, &loc, arg).ok_or_else(|| format!("no hero '{arg}' in heroes.vdata"))?;
    let block = top_level_block(&heroes, &key).ok_or_else(|| format!("no block for {key}"))?;

    let mut fp = Footprint { hero_key: key.clone(), display, ..Default::default() };
    let strings = quoted(block);

    // Tokens: hero key suffix, model folder and stem.
    let mut tokens: BTreeSet<String> = BTreeSet::new();
    tokens.insert(key.trim_start_matches("hero_").to_string());
    let model_src = strings.iter().find(|s| s.ends_with(".vmdl")).cloned().ok_or("hero has no model")?;
    let mp = Path::new(&model_src);
    if let Some(stem) = mp.file_stem() {
        tokens.insert(stem.to_string_lossy().into_owned());
    }
    if let Some(dir) = mp.parent().and_then(|d| d.file_name()) {
        tokens.insert(dir.to_string_lossy().trim_end_matches("_v2").trim_end_matches("_v3").to_string());
    }
    tokens.retain(|t| t.len() >= 3);
    fp.tokens = tokens.iter().cloned().collect();
    let owned = |p: &str| tokens.iter().any(|t| has_token(p, t));

    let mut items: BTreeSet<Item> = BTreeSet::new();
    fn add(items: &mut BTreeSet<Item>, cat: Cat, group: &str, path: String) {
        items.insert(Item { cat, group: group.to_string(), path });
    }

    // Ability slots, in declaration order.
    for line in block.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("ESlot_") {
            if let Some((slot, val)) = rest.split_once('=') {
                let v = quoted(val);
                if let Some(ab) = v.first() {
                    if abilities.contains(&format!("\n\t{ab} =")) {
                        fp.abilities.push((slot.trim().to_string(), ab.clone()));
                    }
                }
            }
        }
    }

    // Hero-level art and scenes.
    let mut sound_files: Vec<(String, bool)> = Vec::new();
    for s in &strings {
        if s.starts_with("file://{images}/heroes/hero_names/") {
            fp.name_art.push(compiled(s));
        }
        if s.starts_with("file://{images}/") && owned(s) {
            add(&mut items, Cat::UiArt, "ui", compiled(s));
        } else if s.ends_with(".vmap") && owned(s) {
            // UI scenes ship as their own map VPKs (maps/ui/hero_shop/lash_shop.vpk), not inside pak01.
            // Valve has switched some off (team reveal: empty folder, missing clip), so a scene only
            // counts if the game actually ships its map.
            let vpk_rel = s.trim_end_matches(".vmap").to_string() + ".vpk";
            if src.game.join("citadel").join(&vpk_rel).exists() {
                add(&mut items, Cat::Scene, "ui", vpk_rel);
            }
        } else if s.ends_with(".vsndevts") {
            sound_files.push((compiled(s), s.contains("/vo/")));
        }
    }

    // Abilities: particles, ability models, HUD icons, and which sound events each one uses.
    let mut event_group: HashMap<String, String> = HashMap::new();
    for (slot, ab) in &fp.abilities {
        let group = if slot.starts_with("Weapon") { "weapon".to_string() } else { ab.clone() };
        if let Some(ab_block) = top_level_block(&abilities, ab) {
            for s in quoted(ab_block) {
                if s.ends_with(".vpcf") && owned(&s) {
                    add(&mut items, Cat::AbilityVfx, &group, compiled(&s));
                } else if s.ends_with(".vmdl") && owned(&s) {
                    add(&mut items, Cat::Model, &group, compiled(&s));
                } else if s.starts_with("file://{images}/") && owned(&s) {
                    add(&mut items, Cat::UiArt, &group, compiled(&s));
                } else if s.contains('.') && !s.contains('/') && !s.contains(' ') && s.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
                    event_group.insert(s.clone(), group.clone());
                }
            }
        }
    }

    // Walk compiled dependencies from the model and the ability particles (owned files only).
    let vpk = vpk::Vpk::open(src.vpk()).map_err(|e| e.to_string())?;
    let core = vpk::Vpk::open(src.game.join("core").join("pak01_dir.vpk")).ok();
    let mut queue: VecDeque<(String, String)> = VecDeque::new();
    queue.push_back((compiled(&model_src), "core".into()));
    let vfx_roots: Vec<Item> = items.iter().filter(|i| i.cat == Cat::AbilityVfx).cloned().collect();
    for it in &vfx_roots {
        queue.push_back((it.path.clone(), it.group.clone()));
    }
    let mut seen: BTreeSet<String> = BTreeSet::new();
    while let Some((path, group)) = queue.pop_front() {
        if !seen.insert(path.clone()) {
            continue;
        }
        let entry = vpk.find(&path).map(|e| (&vpk, e)).or_else(|| core.as_ref().and_then(|c| c.find(&path).map(|e| (c, e))));
        let Some((pak, e)) = entry else { continue };
        if let Some(cat) = classify(&path) {
            let g = if cat == Cat::Animation { clip_group(&path) } else { group.clone() };
            add(&mut items, cat, &g, path.clone());
        }
        if let Ok(data) = pak.read(e) {
            if let Ok(r) = resource::parse(&data) {
                for t in lint::ref_targets(&r) {
                    if owned(&t) {
                        queue.push_back((t, group.clone()));
                    }
                }
            }
        }
    }

    // Sounds: decompile the hero's sound-event files and group each event's vsnd files.
    for (sf, is_vo) in &sound_files {
        let Ok(text) = src.text(sf) else { continue };
        let mut current = String::new();
        for line in text.lines() {
            if line.starts_with('\t') && !line.starts_with("\t\t") && line.trim_end().ends_with('=') {
                current = line.trim().trim_end_matches('=').trim().trim_matches('"').to_string();
            }
            for s in quoted(line) {
                if s.ends_with(".vsnd") {
                    let cat = if *is_vo || s.contains("/vo/") { Cat::Voice } else { Cat::Sound };
                    let group = if cat == Cat::Voice {
                        voice_group(&s, &fp.tokens)
                    } else {
                        event_group.get(&current).cloned().unwrap_or_else(|| sound_group(&current))
                    };
                    add(&mut items, cat, &group, compiled(&s));
                }
            }
        }
    }

    fp.items = items.into_iter().collect();
    Ok(fp)
}

/// Group for sound events no ability claims, by their own naming ("Lash.Wpn.Fire" → weapon).
fn sound_group(event: &str) -> String {
    let e = event.to_ascii_lowercase();
    if e.contains(".wpn") || e.contains("weapon") {
        "weapon".into()
    } else if e.contains("foot") || e.contains("jump") || e.contains("land") || e.contains("slide") || e.contains("dash") || e.contains("mantle") {
        "movement".into()
    } else if e.contains("melee") {
        "melee".into()
    } else if e.contains(".vo") || e.contains("generated.") {
        "voice".into()
    } else {
        "misc".into()
    }
}

/// Voice lines grouped by what they're for: kill, ping, match, use (ability callouts), emote, select...
/// Files are `sounds/vo/<hero>/<hero>_<category>_...` or `sounds/vo/<category>/<hero>/...`.
fn voice_group(path: &str, tokens: &[String]) -> String {
    let rest = path.trim_start_matches("sounds/vo/");
    let mut parts = rest.split('/');
    let first = parts.next().unwrap_or("");
    if !tokens.iter().any(|t| t == first) {
        return format!("vo:{first}");
    }
    let file = rest.rsplit('/').next().unwrap_or("");
    let mut f = file;
    for t in tokens {
        f = f.strip_prefix(&format!("{t}_")).unwrap_or(f);
    }
    format!("vo:{}", f.split('_').next().unwrap_or("misc"))
}

/// Clips grouped by the moment they serve. UI clips (hero select, reveal, pose, shop) are where a
/// skin's fantasy is sold first.
pub fn clip_group(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    let has = |k: &str| name.contains(k);
    if has("ui_") || has("hero_pose") || has("idle_loadout") || has("shopping") || has("roster") || has("countdown") {
        "clips:ui".into()
    } else if has("ability_") || has("cast_") {
        "clips:abilities".into()
    } else if has("melee") || has("parry") {
        "clips:melee".into()
    } else if has("death") || has("flinch") || has("stun") || has("knockdown") || has("stagger") || has("hard_landing") {
        "clips:reactions".into()
    } else if has("aim_") {
        "clips:aim".into()
    } else {
        "clips:locomotion".into()
    }
}

fn classify(path: &str) -> Option<Cat> {
    let ext = path.rsplit('.').next().unwrap_or("");
    Some(match ext {
        "vmdl_c" => Cat::Model,
        "vmat_c" | "vtex_c" if path.starts_with("models/") => Cat::Materials,
        "vmat_c" | "vtex_c" => Cat::AbilityVfx,
        "vnmskel_c" => Cat::Skeleton,
        "vnmgraph_c" => Cat::AnimGraph,
        "vnmclip_c" => Cat::Animation,
        "vpcf_c" => Cat::AbilityVfx,
        "vdata_c" => Cat::Data,
        _ => return None,
    })
}

/// Summary counts, for `magician footprint`.
pub fn summary(fp: &Footprint) -> BTreeMap<(Cat, String), usize> {
    let mut m = BTreeMap::new();
    for i in &fp.items {
        *m.entry((i.cat, i.group.clone())).or_insert(0) += 1;
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_whole_words() {
        for yes in ["models/heroes_wip/lash/lash.vmdl_c", "particles/abilities/lash/lash_flog_cast.vpcf_c", "models/heroes_staging/lash_v2/clips/x.vnmclip_c", "sounds/vo/lash/lash_kill_01.vsnd_c"] {
            assert!(has_token(yes, "lash"), "{yes}");
        }
        for no in ["materials/effects/muzzleflashx.vtex_c", "particles/generic/splash.vpcf_c", "sounds/slash.vsnd_c", "x/flashlight.vtex_c"] {
            assert!(!has_token(no, "lash"), "{no}");
        }
    }

    #[test]
    fn compiled_paths() {
        assert_eq!(compiled("file://{images}/heroes/lash_sm.psd"), "panorama/images/heroes/lash_sm_psd.vtex_c");
        assert_eq!(compiled("file://{images}/heroes/hero_names/lash.svg"), "panorama/images/heroes/hero_names/lash.vsvg_c");
        assert_eq!(compiled("particles/abilities/lash/lash_flog_cast.vpcf"), "particles/abilities/lash/lash_flog_cast.vpcf_c");
        assert_eq!(compiled("sounds/weapons/lash/Fire_01.vsnd"), "sounds/weapons/lash/fire_01.vsnd_c");
    }

    #[test]
    fn blocks_and_quotes() {
        let text = "{\n\thero_a = \n\t{\n\t\tm = resource_name:\"models/a/a.vmdl\"\n\t}\n\thero_b = \n\t{\n\t\tx = \"y\"\n\t}\n}";
        let a = top_level_block(text, "hero_a").unwrap();
        assert!(a.contains("models/a/a.vmdl"));
        assert!(!a.contains("hero_b"));
        assert_eq!(quoted(a), vec!["models/a/a.vmdl"]);
    }

    #[test]
    fn resolves_display_names() {
        let heroes = "{\n\thero_magician = \n\t{\n\t}\n}";
        let loc = "\t\t\"hero_magician:n\" \"Sinclair\"\n";
        assert_eq!(resolve_hero(heroes, loc, "Sinclair"), Some(("hero_magician".into(), "Sinclair".into())));
        assert_eq!(resolve_hero(heroes, loc, "magician"), Some(("hero_magician".into(), "Sinclair".into())));
        assert_eq!(resolve_hero(heroes, loc, "nobody"), None);
    }
}
