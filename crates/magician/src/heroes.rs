//! The heroes of the installed build, read natively from the game (`scripts/heroes.vdata_c` and the hero names in the
//! English localisation), so a hero can be named the way a person would name it: `lash`, `hero_lash`, `Lash`,
//! `"Lady Geist"`, or its model's path. Also which hero a set of mod files belongs to, for `init`.
//!
//! The footprint (crates/grade) still reads the decompiled vdata through Source 2 Viewer; this table needs nothing
//! but the game, so naming a hero, and saying which one was meant when it is misspelled, works on any machine.

use resource::kv3::Value;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hero {
    /// The heroes.vdata key, `hero_lash`.
    pub key: String,
    /// The name players see, `Lash` (the key without `hero_` when the localisation has none).
    pub name: String,
    /// `models/heroes_wip/lash/lash.vmdl`, or empty.
    pub model: String,
    /// Neither disabled nor in development: a hero someone can pick.
    pub playable: bool,
}

impl Hero {
    /// Words a path of this hero's files carries: the key without `hero_`, the model's file stem and its folder (a
    /// trailing `_v2` and the like dropped), each of three letters or more.
    pub fn tokens(&self) -> Vec<String> {
        let mut t = vec![self.key.trim_start_matches("hero_").to_string()];
        let mut parts = self.model.rsplit('/');
        if let Some(file) = parts.next() {
            t.push(file.trim_end_matches(".vmdl").to_string());
        }
        if let Some(dir) = parts.next() {
            let d = match dir.rsplit_once("_v") {
                Some((stem, n)) if !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) => stem,
                _ => dir,
            };
            t.push(d.to_string());
        }
        t.retain(|x| x.len() >= 3);
        t.sort();
        t.dedup();
        t
    }

    /// `hero_lash (Lash)`.
    pub fn label(&self) -> String {
        format!("{} ({})", self.key, self.name)
    }

    /// The model's folder with a trailing `/`, or empty.
    fn model_dir(&self) -> String {
        self.model.rsplit_once('/').map(|(d, _)| format!("{d}/")).unwrap_or_default()
    }
}

#[derive(Debug, Clone, Default)]
pub struct Heroes {
    pub list: Vec<Hero>,
}

const HEROES: &str = "scripts/heroes.vdata_c";
const NAMES: &str = "resource/localization/citadel_gc_hero_names/citadel_gc_hero_names_english.txt";

/// `"hero_lash:n" "Lash"` lines of the hero-names localisation: key to display name.
fn display_names(localization: &str) -> BTreeMap<String, String> {
    let mut names = BTreeMap::new();
    for line in localization.lines() {
        let q: Vec<&str> = line.split('"').skip(1).step_by(2).collect();
        if let [k, v, ..] = q.as_slice() {
            if let Some(key) = k.strip_suffix(":n") {
                names.entry(key.to_string()).or_insert_with(|| v.to_string());
            }
        }
    }
    names
}

fn norm(s: &str) -> String {
    s.trim().to_ascii_lowercase().replace('\\', "/")
}

/// A name with its spaces, `_`, `-`, `'` and `.` gone, for matching `Lady Geist`, `lady_geist` and `ladygeist` alike.
fn squash(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

impl Heroes {
    /// The table from a decoded heroes.vdata (its top-level `hero_*` objects) and the hero-names localisation.
    pub fn from_kv3(v: &Value, localization: &str) -> Heroes {
        let names = display_names(localization);
        let mut list = Vec::new();
        for (key, h) in v.members() {
            if !key.starts_with("hero_") || key == "hero_base" || !matches!(h, Value::Object(_)) {
                continue;
            }
            let flag = |k: &str| h.get(k).and_then(Value::as_bool).unwrap_or(false);
            list.push(Hero {
                key: key.clone(),
                name: names.get(key).cloned().unwrap_or_else(|| key.trim_start_matches("hero_").to_string()),
                model: h.get("m_strModelName").and_then(Value::as_str).map(norm).unwrap_or_default(),
                playable: !flag("m_bDisabled") && !flag("m_bInDevelopment"),
            });
        }
        Heroes { list }
    }

    /// The installed build's heroes, read from `game/citadel`.
    pub fn from_game(game: &Path) -> Result<Heroes, String> {
        let pak_path = game.join("citadel").join("pak01_dir.vpk");
        let pak = vpk::Vpk::open(&pak_path).map_err(|e| format!("{}: {e}", pak_path.display()))?;
        let e = pak.find(HEROES).ok_or_else(|| format!("{} has no {HEROES}", pak_path.display()))?;
        let data = pak.read(e).map_err(|e| format!("{HEROES}: {e}"))?;
        let v = resource::kv3_block(&data, b"DATA").ok_or_else(|| format!("{HEROES} has no DATA block"))?.map_err(|e| format!("{HEROES}: {e}"))?;
        let loc = std::fs::read_to_string(game.join("citadel").join(crate::build::native(NAMES))).unwrap_or_default();
        Ok(Heroes::from_kv3(&v, &loc))
    }

    /// The hero `arg` names: its key (`hero_lash`), the key without `hero_` (`lash`), its display name in any case and
    /// spacing (`Lash`, `lady geist`), its model's path (`models/heroes_wip/lash/lash.vmdl`, with or without `_c`), or
    /// a word of its model (`geist`). A playable hero wins over a disabled one that shares the name or model. `Err`
    /// says what was asked, and the nearest hero when there is one.
    pub fn resolve(&self, arg: &str) -> Result<&Hero, String> {
        let a = norm(arg);
        let by = |f: &dyn Fn(&Hero) -> bool| {
            let m: Vec<&Hero> = self.list.iter().filter(|h| f(h)).collect();
            m.iter().find(|h| h.playable).or(m.first()).copied()
        };
        let model = a.trim_end_matches("_c").to_string();
        let found = by(&|h| h.key == a)
            .or_else(|| by(&|h| h.key == format!("hero_{a}")))
            .or_else(|| by(&|h| !a.is_empty() && squash(&h.name) == squash(&a)))
            .or_else(|| by(&|h| a.contains('/') && !h.model.is_empty() && h.model == model))
            .or_else(|| {
                // a word of exactly one playable hero's model (`geist` is Lady Geist's `geist.vmdl`)
                let m: Vec<&Hero> = self.list.iter().filter(|h| h.playable && h.tokens().contains(&a)).collect();
                (m.len() == 1).then(|| m[0])
            });
        found.ok_or_else(|| {
            let words: Vec<String> = self.list.iter().filter(|h| h.playable).flat_map(|h| [h.key.trim_start_matches("hero_").to_string(), h.name.to_ascii_lowercase()]).collect();
            let refs: Vec<&str> = words.iter().map(String::as_str).collect();
            let near = crate::cli::nearest(&a, refs).and_then(|w| self.list.iter().find(|h| h.playable && (h.key.trim_start_matches("hero_") == w || h.name.eq_ignore_ascii_case(w))));
            match near {
                Some(h) => format!("no hero {arg:?} in this build (did you mean {}?)", h.label()),
                None => format!("no hero {arg:?} in this build; a hero is named by its key (hero_lash), its name (Lash) or its model's path"),
            }
        })
    }

    /// Which playable hero a mod's files belong to: each file under a hero's model folder is three votes for it, a
    /// file whose path carries one of its words one. `None` when no file points at a hero.
    pub fn infer<'a>(&self, paths: impl IntoIterator<Item = &'a str>) -> Option<Inference> {
        let heroes: Vec<(&Hero, String, Vec<String>)> = self.list.iter().filter(|h| h.playable).map(|h| (h, h.model_dir(), h.tokens())).collect();
        let mut votes: BTreeMap<&str, (usize, usize)> = BTreeMap::new(); // key -> (score, files)
        let mut files = 0usize;
        // each file and the heroes it points at, with how strongly, for the files that are another hero's
        let mut by_file: Vec<(String, Vec<(&str, usize)>)> = Vec::new();
        for p in paths {
            files += 1;
            let p = norm(p);
            let mut its: Vec<(&str, usize)> = Vec::new();
            for (h, dir, tokens) in &heroes {
                let score = if !dir.is_empty() && p.starts_with(dir.as_str()) { 3 } else if tokens.iter().any(|t| grade::footprint::has_token(&p, t)) { 1 } else { 0 };
                if score > 0 {
                    let v = votes.entry(h.key.as_str()).or_default();
                    v.0 += score;
                    v.1 += 1;
                    its.push((h.key.as_str(), score));
                }
            }
            if !its.is_empty() {
                by_file.push((p, its));
            }
        }
        let mut ranked: Vec<(&str, usize, usize)> = votes.into_iter().map(|(k, (s, n))| (k, s, n)).collect();
        ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        let (key, score, n) = *ranked.first()?;
        let hero = self.list.iter().find(|h| h.key == key)?.clone();
        let runner_up = ranked.get(1).map(|&(k, s, _)| (k.to_string(), s));
        // A file that points at another hero and not at this one is that hero's: the mod changes that hero too.
        let mut others: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (p, its) in by_file {
            if its.iter().any(|(k, _)| *k == key) {
                continue;
            }
            if let Some((k, _)) = its.iter().max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(a.0))) {
                if let Some(h) = self.list.iter().find(|h| h.key == *k) {
                    others.entry(h.label()).or_default().push(p);
                }
            }
        }
        Some(Inference { hero, files: n, of: files, score, runner_up, others: others.into_iter().collect() })
    }
}

/// What `infer` decided, and how sure it is.
#[derive(Debug, Clone)]
pub struct Inference {
    pub hero: Hero,
    /// Files that point at the hero, of all files.
    pub files: usize,
    pub of: usize,
    pub score: usize,
    /// The next hero and its score, when another one got votes.
    pub runner_up: Option<(String, usize)>,
    /// Files that are another hero's and not this one's, by that hero (`label`): `soundevents/hero/atlas.vsndevts_c`
    /// in a skin for someone else changes Abrams too.
    pub others: Vec<(String, Vec<String>)>,
}

impl Inference {
    /// Clear when the hero has at least twice the runner-up's votes.
    pub fn clear(&self) -> bool {
        self.runner_up.as_ref().is_none_or(|(_, s)| self.score >= 2 * s)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A heroes.vdata in KV3 text: Lash, a disabled hero sharing Lash's model, Lady Geist, Sinclair and the base block.
    pub const VDATA: &str = r#"{
        generic_data_type = "CitadelHeroData_t"
        hero_base = { m_bDisabled = false }
        hero_lash = { m_bDisabled = false m_bInDevelopment = false m_strModelName = "models/heroes_wip/lash/lash.vmdl" }
        hero_skyrunner = { m_bDisabled = true m_bInDevelopment = true m_strModelName = "models/heroes_wip/lash/lash.vmdl" }
        hero_ghost = { m_bDisabled = false m_strModelName = "models/heroes_wip/geist/geist.vmdl" }
        hero_magician = { m_strModelName = "models/heroes_staging/magician_v2/magician.vmdl" }
    }"#;
    pub const NAMES: &str = "\"lang\"\n{\n\t\"Tokens\"\n\t{\n\t\t\"hero_lash:n\"\t\"Lash\"\n\t\t\"hero_ghost:n\"\t\"Lady Geist\"\n\t\t\"hero_magician:n\"\t\"Sinclair\"\n\t}\n}\n";

    pub fn table() -> Heroes {
        Heroes::from_kv3(&resource::kv3::parse_text(VDATA).unwrap(), NAMES)
    }

    #[test]
    fn a_hero_is_named_the_way_a_person_names_it() {
        let h = table();
        assert_eq!(h.list.len(), 4, "hero_base and generic_data_type are not heroes");
        for arg in ["lash", "hero_lash", "Lash", "LASH", "models/heroes_wip/lash/lash.vmdl", "models\\heroes_wip\\lash\\lash.vmdl_c"] {
            assert_eq!(h.resolve(arg).map(|x| x.key.as_str()), Ok("hero_lash"), "{arg}");
        }
        for arg in ["Lady Geist", "lady_geist", "ladygeist", "ghost", "geist"] {
            assert_eq!(h.resolve(arg).map(|x| x.key.as_str()), Ok("hero_ghost"), "{arg}");
        }
        assert_eq!(h.resolve("Sinclair").map(|x| x.name.as_str()), Ok("Sinclair"));
        assert_eq!(h.resolve("magician").map(|x| x.key.as_str()), Ok("hero_magician"));
        assert_eq!(h.resolve("skyrunner").map(|x| x.playable), Ok(false), "a disabled hero is still a hero by its key");
        let e = h.resolve("lahs").unwrap_err();
        assert!(e.contains("did you mean hero_lash (Lash)?"), "{e}");
        let e = h.resolve("sinclar").unwrap_err();
        assert!(e.contains("hero_magician (Sinclair)"), "{e}");
        assert!(!h.resolve("zzzzzz").unwrap_err().contains("did you mean"));
        assert!(h.resolve("").is_err());
    }

    #[test]
    fn the_hero_a_pak_is_for_comes_from_the_files_it_replaces() {
        let h = table();
        let lash = ["models/heroes_wip/lash/lash.vmdl_c", "models/heroes_wip/lash/materials/lash_body.vmat_c", "particles/abilities/lash/lash_slam.vpcf_c", "sounds/vo/lash/lash_01.vsnd_c", "materials/ui/generic.vtex_c"];
        let i = h.infer(lash).unwrap();
        assert_eq!((i.hero.key.as_str(), i.files, i.of), ("hero_lash", 4, 5));
        assert!(i.clear());
        // half one hero and half another is not a clear answer
        let mixed = ["models/heroes_wip/lash/lash.vmdl_c", "models/heroes_wip/geist/geist.vmdl_c"];
        assert!(!h.infer(mixed).unwrap().clear());
        assert!(h.infer(["materials/ui/generic.vtex_c"]).is_none());
        assert!(h.infer(std::iter::empty()).is_none());
        assert_eq!(h.list[3].tokens(), vec!["magician".to_string()], "magician_v2 is the magician folder");
        assert!(i.others.is_empty());
    }

    #[test]
    fn a_file_of_another_hero_in_a_mod_is_named_with_that_hero() {
        // a Lash skin that also ships Lady Geist's sound events changes her too
        let h = table();
        let files = ["models/heroes_wip/lash/lash.vmdl_c", "particles/abilities/lash/lash_slam.vpcf_c", "soundevents/hero/geist.vsndevts_c", "materials/default/default_mask.vtex_c"];
        let i = h.infer(files).unwrap();
        assert_eq!(i.hero.key, "hero_lash");
        assert_eq!(i.others, vec![("hero_ghost (Lady Geist)".to_string(), vec!["soundevents/hero/geist.vsndevts_c".to_string()])]);
    }
}
