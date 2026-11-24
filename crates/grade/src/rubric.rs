//! The grading rubric. One home for every number a grade is judged by: the criteria, their weights per archetype, the
//! letter thresholds, the coverage targets, the identity requirements, the fair-play rule classes and the depth caps.
//! A policy file (`policy.rs`) may move any of them; these are the defaults, and a run with no policy uses them.
//!
//! Depth means completeness, not taste: S means "comprehensively fulfils its archetype", never "looks great", and
//! letters compare within an archetype, never across. Coverage is measured *per group* (per ability, per moment), not
//! per file: replacing a quarter of an ability's particles with intent is a full reskin of that ability. Hundreds of
//! untouched debris sprites shouldn't drag a beautiful ability down, and a mod that replaces 90 files of one ability
//! shouldn't look complete.

use crate::footprint::{Cat, Footprint};
use crate::policy::Policy;
use std::collections::{BTreeMap, BTreeSet};

/// The version of the grading rules a record was judged by. Raise it when a rule's meaning changes, so an old record
/// is never read as if today's rules made it.
pub const RULES_VERSION: &str = "2.0.0";

// ---------------------------------------------------------------------------------------------------- criteria

pub struct Criterion {
    pub id: &'static str,
    pub name: &'static str,
    pub why: &'static str,
}

pub const CRITERIA: &[Criterion] = &[
    Criterion { id: "model", name: "Hero model", why: "The silhouette players see all match. A character starts here." },
    Criterion { id: "materials", name: "Materials & textures", why: "Its own surfaces (new or replaced materials), not just a reshaped mesh. For a variant: every visible material slot of the hero." },
    Criterion { id: "vfx", name: "Ability VFX", why: "Every signature ability and the weapon look like *this* character. Measured per ability." },
    Criterion { id: "sound", name: "Kit sound", why: "Abilities, weapon, melee and movement sound like the fantasy. Measured per group." },
    Criterion { id: "animation", name: "Animation", why: "Hero-select and showcase clips first (where the fantasy is sold), then ability and other clips." },
    Criterion { id: "ui", name: "UI art", why: "Hero card, gloat and critical cards, minimap and small icons, name logo, ability icons." },
    Criterion { id: "scenes", name: "Showcase scenes", why: "The hero shop, prefab and postgame portrait scenes." },
    Criterion { id: "voice", name: "Voice & stingers", why: "Voice treatment or stingers layered on his lines (no voice cloning)." },
    Criterion { id: "craft", name: "Craft", why: "Lint-clean, no dropped dependencies, never ships skeleton/graph/data, no orphan files. Fair-play findings are judged apart (compliance), never here." },
];

pub fn criterion(id: &str) -> &'static Criterion {
    CRITERIA.iter().find(|c| c.id == id).expect("unknown criterion")
}

// ---------------------------------------------------------------------------------------------------- archetypes

/// Which rubric a mod is judged by. Identity (`identity.rs`) decides it; the label word follows from it and the letter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Archetype {
    /// A new character (Echo, Echolite) or the same character in new clothes or a new body (Costume).
    Character,
    /// Valve's mesh kept, new materials and textures.
    Variant,
    /// No model: VFX, sound, UI or voice.
    Kit,
}

impl Archetype {
    pub fn id(self) -> &'static str {
        match self {
            Archetype::Character => "character",
            Archetype::Variant => "variant",
            Archetype::Kit => "kit",
        }
    }
    pub fn all() -> [Archetype; 3] {
        [Archetype::Character, Archetype::Variant, Archetype::Kit]
    }
    /// Criteria that always count, and criteria that count once the mod touches them (a variant without UI art is
    /// not marked down for UI it never claimed). A character counts every criterion, as the v1 rubric did.
    pub fn surfaces(self) -> (&'static [&'static str], &'static [&'static str]) {
        match self {
            Archetype::Character => (&["model", "materials", "vfx", "sound", "animation", "ui", "scenes", "voice", "craft"], &[]),
            Archetype::Variant => (&["materials", "craft"], &["ui", "scenes"]),
            Archetype::Kit => (&["craft"], &["vfx", "sound", "ui", "voice", "animation"]),
        }
    }
}

/// Default weights per archetype; each list totals 100. The character rubric keeps v1's weights. A variant's and a
/// kit's score is over the criteria in its scope only, so craft weighs less there: a clean pak with one sound in it is
/// a fragment of a kit, not half of one.
pub const WEIGHTS_CHARACTER: &[(&str, f64)] =
    &[("model", 20.0), ("materials", 8.0), ("vfx", 16.0), ("sound", 12.0), ("animation", 10.0), ("ui", 10.0), ("scenes", 4.0), ("voice", 5.0), ("craft", 15.0)];
pub const WEIGHTS_VARIANT: &[(&str, f64)] = &[("materials", 65.0), ("craft", 15.0), ("ui", 15.0), ("scenes", 5.0)];
pub const WEIGHTS_KIT: &[(&str, f64)] = &[("vfx", 26.0), ("sound", 20.0), ("animation", 16.0), ("ui", 16.0), ("voice", 12.0), ("craft", 10.0)];

/// Letter thresholds (score at or above), S down to D; anything lower is F.
pub const LETTERS: [(&str, f64); 5] = [("S", 90.0), ("A", 75.0), ("B", 58.0), ("C", 40.0), ("D", 20.0)];

// ---------------------------------------------------------------------------------------------------- coverage

/// A group (one ability, one moment) is fully covered once this share of its files is replaced (at least one).
pub const GROUP_SHARE: f64 = 0.25;
/// Character materials: this many of the hero's own materials for full credit.
pub const MATERIALS_FULL: f64 = 3.0;
/// Voice: this many treated voice files for full credit.
pub const VOICE_FULL: f64 = 40.0;
/// Kit sound by new files (events rewired to sounds of the mod's own): this many for the custom-sound credit.
pub const NEW_SOUNDS_FULL: f64 = 20.0;
/// Animation: hero-select/showcase clips, ability clips, other clips for full credit in each band.
pub const ANIM_UI_FULL: f64 = 2.0;
pub const ANIM_ABILITIES_FULL: f64 = 4.0;
pub const ANIM_OTHER_FULL: f64 = 6.0;
/// UI: the hero art is fully credited once this share of it is replaced.
pub const UI_HERO_ART_SHARE: f64 = 0.6;
/// A kit's scope is at least this many of VFX, sound, UI and voice, so nobody declares a tiny scope and claims S.
pub const KIT_DEPARTMENTS: f64 = 2.0;

// ---------------------------------------------------------------------------------------------------- identity

/// A new character needs at least this many identity signals.
pub const NEW_CHARACTER_SIGNALS: f64 = 2.0;
/// A new voice identity counts for this much of a signal (we never clone voices; a treatment of the original lines
/// does not count on its own).
pub const VOICE_IDENTITY_WEIGHT: f64 = 0.5;
/// The head counts as Valve's when at least this share of our head-weighted vertices sit on Valve's head.
pub const HEAD_KEPT_SHARE: f64 = 0.5;
/// `new_hero_art`: at least this share of the hero's portrait and card art is replaced.
pub const PORTRAIT_SHARE: f64 = 0.5;
/// A replaced model keeps Valve's mesh (a variant) when at least this share of its vertices sit on Valve's.
pub const VALVE_MESH_SHARE: f64 = 0.98;

// ---------------------------------------------------------------------------------------------------- compliance

/// The fair-play rules: what they find sets compliance, never the letter. M062 counts only for hitboxes (a lost bone or
/// attachment is the mod not working, a depth matter).
pub const FAIR_PLAY_RULES: &[&str] = &["M062", "M063", "M081", "M082", "M124", "M129", "M130", "M131", "M140"];
/// Never allowable: anything that makes an enemy harder to read, or removes or weakens a gameplay cue. A finding of
/// one of these fails fair play whatever an allowance says.
pub const NEVER_ALLOWABLE: &[&str] = &["M062", "M063", "M082", "M124", "M129", "M131"];
/// Advisory: reported in the record, never a violation (a brighter cue is not a hidden one; the wide envelope is the
/// notice below the taste tier).
pub const ADVISORY: &[&str] = &["M081", "M130"];
/// The rule that says a fair-play check could not run: it makes fair play unknown, and no allowance changes that.
pub const UNCHECKED: &str = "M144";

/// Whether a finding is a fair-play finding (M062 only for hitboxes).
pub fn is_fair_play(code: &str, message: &str) -> bool {
    FAIR_PLAY_RULES.contains(&code) && (code != "M062" || message.starts_with("hitbox"))
}

/// Whether a finding belongs to the compliance axis at all (fair play, or a fair-play check that could not run). Such
/// findings never count against depth.
pub fn is_compliance(code: &str, message: &str) -> bool {
    code == UNCHECKED || is_fair_play(code, message)
}

/// Whether fair play is enforced (`[fair_play] enforce`). Lenient by default: a fair-play finding is a warning, so
/// lint, build and install never stop on one alone, and NSFR or FP? is a warning at the release gate; the label and
/// the record say exactly what they say when enforced. Enforced, the same findings are errors and NSFR and FP? block.
pub const FAIR_PLAY_ENFORCED: bool = false;
/// Whether rights must be cleared to pass the release gate (`[rights] enforce`). Lenient by default: `unreviewed` is a
/// warning at the gate; `denied` is a blocker either way.
pub const RIGHTS_ENFORCED: bool = false;

// ---------------------------------------------------------------------------------------------------- the grade

/// What the grader needs to know about the mod.
#[derive(Default)]
pub struct ModFacts {
    /// Every path the mod ships, minus files byte-identical to Valve's at the same path (they change nothing).
    pub paths: BTreeSet<String>,
    /// The materials the hero's own compiled model in the mod references and the mod ships, wherever their path is
    /// (`evidence::model_materials`). The path-token count misses a mod whose materials live in its own folder.
    pub model_materials: BTreeSet<String>,
    /// UI scene map VPKs (`maps/ui/...vpk`, footprint paths) the release carries with entities of their own
    /// (`evidence::carried_scenes`). A map that is not in the pak itself is credited from here.
    pub scene_maps: BTreeSet<String>,
    /// Lint errors that are not fair play, allowed or not: something won't load in game.
    pub lint_errors: usize,
    /// Lint warnings that are not fair play and carry no allowance.
    pub lint_warnings: usize,
    pub dropped_deps: usize,
    pub orphans: usize,
    /// The hero-select pose, shop/postgame poses and deaths are legacy sequences stored inside the
    /// model itself (ANIM/ASEQ blocks), not clip files. True when the mod's model carries different
    /// animation data from Valve's.
    pub model_anim_changed: bool,
    /// For a variant: Valve's visible material slots on the hero model, covered and missing (`evidence::material_slots`).
    pub material_slots: Option<(Vec<String>, Vec<String>)>,
}

pub struct Line {
    pub id: &'static str,
    pub name: &'static str,
    pub weight: f64,
    pub coverage: f64,
    /// Whether the line counts toward the score (an applicable criterion the mod never touched does not).
    pub counted: bool,
    pub detail: String,
    /// Most valuable missing items for this criterion, best first.
    pub missing: Vec<String>,
}

pub struct Cap {
    pub max: &'static str,
    pub why: String,
}

pub struct Report {
    pub hero: String,
    pub archetype: Archetype,
    pub score: f64,
    pub letter: &'static str,
    pub uncapped: &'static str,
    pub caps: Vec<Cap>,
    pub lines: Vec<Line>,
}

/// The letter a score earns under the policy's thresholds.
pub fn letter(score: f64, p: &Policy) -> &'static str {
    LETTERS.iter().zip(p.letters.iter()).find(|(_, min)| score >= **min - 1e-9).map(|((l, _), _)| *l).unwrap_or("F")
}

const ORDER: [&str; 6] = ["S", "A", "B", "C", "D", "F"];

fn rank(l: &str) -> usize {
    ORDER.iter().position(|x| *x == l).unwrap_or(5)
}

/// Credit for one group: full once `share` of its files are replaced (at least one).
fn group_credit(replaced: usize, total: usize, share: f64) -> f64 {
    if total == 0 {
        return 0.0;
    }
    let need = ((total as f64) * share).ceil().max(1.0);
    (replaced as f64 / need).min(1.0)
}

fn groups(fp: &Footprint, cat: Cat, filter: impl Fn(&str) -> bool) -> BTreeMap<String, (usize, Vec<String>)> {
    let mut m: BTreeMap<String, (usize, Vec<String>)> = BTreeMap::new();
    for i in fp.by_cat(cat).filter(|i| filter(&i.group)) {
        let e = m.entry(i.group.clone()).or_default();
        e.0 += 1;
        e.1.push(i.path.clone());
    }
    m
}

fn pretty_group(fp: &Footprint, g: &str) -> String {
    fp.abilities
        .iter()
        .find(|(_, a)| a == g)
        .map(|(slot, a)| format!("{a} ({slot})"))
        .unwrap_or_else(|| g.to_string())
}

/// Average group credit over the given groups, and the missing groups (named) worst first.
fn grouped_line(fp: &Footprint, m: &ModFacts, cat: Cat, share: f64, keep: impl Fn(&str) -> bool) -> (f64, String, Vec<String>) {
    let gs = groups(fp, cat, keep);
    if gs.is_empty() {
        return (1.0, "nothing to cover".into(), vec![]);
    }
    let mut sum = 0.0;
    let mut missing = Vec::new();
    let mut done = 0;
    for (g, (n, paths)) in &gs {
        let r = paths.iter().filter(|p| m.paths.contains(*p)).count();
        let c = group_credit(r, *n, share);
        sum += c;
        if c >= 1.0 {
            done += 1;
        } else {
            // Name the effect or sound itself before its textures and materials.
            let mut todo: Vec<&String> = paths.iter().filter(|p| !m.paths.contains(*p)).collect();
            todo.sort_by_key(|p| (!(p.ends_with(".vpcf_c") || p.ends_with(".vsnd_c")), p.len()));
            let first = todo.first().map(|s| s.to_string()).unwrap_or_default();
            missing.push(format!("{}: {r}/{n} replaced, e.g. {first}", pretty_group(fp, g)));
        }
    }
    (sum / gs.len() as f64, format!("{done}/{} groups covered", gs.len()), missing)
}

/// Whether the mod touches anything of this criterion at all (an applicable criterion counts once it does).
fn touched(id: &str, coverage: f64) -> bool {
    id == "craft" || coverage > 0.0
}

/// Every criterion's coverage for this mod against the hero's footprint, before weighting.
fn lines(fp: &Footprint, m: &ModFacts, archetype: Archetype, p: &Policy) -> Vec<Line> {
    let cv = &p.coverage;
    let token = |path: &str| fp.tokens.iter().any(|t| crate::footprint::has_token(path, t));
    let signature: BTreeSet<String> = fp
        .abilities
        .iter()
        .filter(|(slot, _)| slot.starts_with("Signature") || slot.starts_with("Weapon"))
        .map(|(_, a)| a.clone())
        .chain(["weapon".to_string()])
        .collect();
    let mut out = Vec::new();
    let mut push = |id: &'static str, coverage: f64, detail: String, missing: Vec<String>| {
        out.push(Line { id, name: criterion(id).name, weight: 0.0, coverage: coverage.clamp(0.0, 1.0), counted: true, detail, missing });
    };

    // Model
    let hero_model = fp.by_cat(Cat::Model).find(|i| i.group == "core").map(|i| i.path.clone());
    let has_model = hero_model.as_ref().is_some_and(|p| m.paths.contains(p));
    push("model", if has_model { 1.0 } else { 0.0 }, if has_model { "replaced".into() } else { "not replaced".into() }, if has_model { vec![] } else { hero_model.into_iter().collect() });

    // Materials. A variant is judged slot by slot: every visible material slot of Valve's hero model. Everything else
    // counts the hero's own materials: named by its path, or used by its own compiled model (a mod's materials may
    // live under a folder of its own).
    match (&m.material_slots, archetype) {
        (Some((covered, missing)), Archetype::Variant) => {
            let total = covered.len() + missing.len();
            let c = if total == 0 { 0.0 } else { covered.len() as f64 / total as f64 };
            push("materials", c, format!("{}/{total} visible material slots of the hero covered", covered.len()), missing.clone());
        }
        _ => {
            let mats = m.paths.iter().filter(|q| q.ends_with(".vmat_c") && (token(q) || m.model_materials.contains(*q))).count();
            let full = cv.materials_full;
            push(
                "materials",
                mats as f64 / full,
                format!("{mats} hero material(s), {} used by the hero's model", m.model_materials.len()),
                if mats as f64 >= full { vec![] } else { vec![format!("ship the outfit's own materials ({full}+ for full credit)")] },
            );
        }
    }

    // Ability VFX, per signature ability + weapon.
    let (c, d, miss) = grouped_line(fp, m, Cat::AbilityVfx, cv.group_share, |g| signature.contains(g));
    push("vfx", c, d, miss);

    // Kit sound: signature abilities, weapon, melee, movement. New sound files count too.
    let (gc, gd, gmiss) = grouped_line(fp, m, Cat::Sound, cv.group_share, |g| signature.contains(g) || g == "melee" || g == "movement");
    let new_sounds = m.paths.iter().filter(|q| q.ends_with(".vsnd_c") && fp.cat_of(q).is_none() && token(q)).count();
    let events_replaced = m.paths.iter().any(|q| q.starts_with("soundevents/hero/") && token(q));
    let custom = if events_replaced { (new_sounds as f64 / cv.new_sounds_full).min(1.0) * 0.85 } else { 0.0 };
    push("sound", gc.max(custom), format!("{gd}; {new_sounds} new sound file(s){}", if events_replaced { ", events rewired" } else { "" }), gmiss);

    // Animation: UI clips weigh most.
    let clips = |g: &str| {
        let all: Vec<&String> = fp.by_cat(Cat::Animation).filter(|i| i.group == g).map(|i| &i.path).collect();
        let r = all.iter().filter(|q| m.paths.contains(**q)).count();
        (r, all.len())
    };
    let (ui_r, ui_n) = clips("clips:ui");
    let (ab_r, ab_n) = clips("clips:abilities");
    let other_r: usize = ["clips:melee", "clips:reactions", "clips:locomotion", "clips:aim"].iter().map(|g| clips(g).0).sum();
    let ui_r = ui_r + if m.model_anim_changed { 2 } else { 0 };
    let anim = 0.5 * (ui_r as f64 / cv.anim_ui_full).min(1.0) + 0.3 * (ab_r as f64 / cv.anim_abilities_full).min(1.0) + 0.2 * (other_r as f64 / cv.anim_other_full).min(1.0);
    let mut anim_missing = vec![];
    if (ui_r as f64) < cv.anim_ui_full {
        anim_missing.push(format!("hero-select/showcase clips: {ui_r}/{ui_n} replaced ({} for full credit)", cv.anim_ui_full));
    }
    if (ab_r as f64) < cv.anim_abilities_full {
        anim_missing.push(format!("ability clips: {ab_r}/{ab_n} replaced ({} for full credit)", cv.anim_abilities_full));
    }
    if (other_r as f64) < cv.anim_other_full {
        anim_missing.push(format!("idle/locomotion/melee/reaction clips: {other_r} replaced ({} for full credit)", cv.anim_other_full));
    }
    push("animation", anim, format!("ui {ui_r}{}, abilities {ab_r}, other {other_r}", if m.model_anim_changed { " (incl. in-model select/death poses)" } else { "" }), anim_missing);

    // UI art: hero art (70%) + ability icons (30%).
    let hero_art: Vec<&String> = fp.by_cat(Cat::UiArt).filter(|i| i.group == "ui").map(|i| &i.path).collect();
    let icons: Vec<&String> = fp.by_cat(Cat::UiArt).filter(|i| i.group != "ui").map(|i| &i.path).collect();
    let ha = hero_art.iter().filter(|q| m.paths.contains(**q)).count();
    let ic = icons.iter().filter(|q| m.paths.contains(**q)).count();
    let ui = 0.7 * ((ha as f64) / (hero_art.len().max(1) as f64 * cv.ui_hero_art_share)).min(1.0) + 0.3 * ((ic as f64) / (icons.len().max(1) as f64)).min(1.0);
    let ui_missing: Vec<String> = hero_art.iter().chain(icons.iter()).filter(|q| !m.paths.contains(**q)).map(|q| q.to_string()).collect();
    push("ui", ui, format!("hero art {ha}/{}, ability icons {ic}/{}", hero_art.len(), icons.len()), ui_missing);

    // Scenes
    let scenes: Vec<&String> = fp.by_cat(Cat::Scene).map(|i| &i.path).collect();
    let covered: Vec<&&String> = scenes
        .iter()
        .filter(|q| {
            let stem = q.trim_end_matches(".vpk");
            m.scene_maps.contains(q.as_str()) || m.paths.iter().any(|x| x.starts_with(stem))
        })
        .collect();
    let sc = covered.len();
    push(
        "scenes",
        if scenes.is_empty() { 1.0 } else { sc as f64 / scenes.len() as f64 },
        format!("{sc}/{}", scenes.len()),
        scenes.iter().filter(|s| !covered.contains(s)).map(|s| s.to_string()).collect(),
    );

    // Voice & stingers: any VO-side treatment.
    let vo = m.paths.iter().filter(|q| fp.cat_of(q) == Some(Cat::Voice)).count();
    push(
        "voice",
        vo as f64 / cv.voice_full,
        format!("{vo} voice file(s) treated"),
        if vo as f64 >= cv.voice_full { vec![] } else { vec!["stingers on kill/select/emote lines (vo:kill, vo:select, vo:emote)".into()] },
    );

    // Craft
    let protected = protected(fp, m);
    let craft = (if m.lint_errors == 0 { 6.0 } else { 0.0 })
        + (if m.lint_warnings == 0 { 2.0 } else { 0.0 })
        + (if m.dropped_deps == 0 { 3.0 } else { 0.0 })
        + (if protected.is_empty() { 2.0 } else { 0.0 })
        + (if m.orphans == 0 { 2.0 } else { 0.0 });
    let mut craft_missing = vec![];
    if m.lint_errors > 0 {
        craft_missing.push(format!("{} lint error(s)", m.lint_errors));
    }
    if m.lint_warnings > 0 {
        craft_missing.push(format!("{} lint warning(s)", m.lint_warnings));
    }
    if m.dropped_deps > 0 {
        craft_missing.push(format!("{} dropped dependency(ies) (M061)", m.dropped_deps));
    }
    for q in &protected {
        craft_missing.push(format!("ships protected file {q}"));
    }
    if m.orphans > 0 {
        craft_missing.push(format!("{} orphan file(s) (M031)", m.orphans));
    }
    push("craft", craft / 15.0, format!("{craft:.0}/15"), craft_missing);

    // Weights and scope for the archetype.
    let (required, applicable) = archetype.surfaces();
    let weights = p.weights(archetype);
    out.retain(|l| required.contains(&l.id) || applicable.contains(&l.id));
    for l in &mut out {
        l.weight = weights.iter().find(|(id, _)| id == l.id).map_or(0.0, |(_, w)| *w);
        l.counted = required.contains(&l.id) || touched(l.id, l.coverage);
    }
    // A kit's scope is never smaller than `kit_departments` of VFX, sound, UI and voice: the untouched ones with the
    // most weight join it at zero coverage.
    if archetype == Archetype::Kit {
        let depts = ["vfx", "sound", "ui", "voice"];
        let need = cv.kit_departments.round() as usize;
        let mut have = out.iter().filter(|l| depts.contains(&l.id) && l.counted).count();
        let mut idle: Vec<usize> = (0..out.len()).filter(|&i| depts.contains(&out[i].id) && !out[i].counted).collect();
        idle.sort_by(|&a, &b| out[b].weight.partial_cmp(&out[a].weight).unwrap_or(std::cmp::Ordering::Equal).then(a.cmp(&b)));
        for i in idle {
            if have >= need {
                break;
            }
            out[i].counted = true;
            out[i].missing.insert(0, format!("a kit covers at least {need} of VFX, sound, UI and voice"));
            have += 1;
        }
    }
    out
}

fn protected<'a>(fp: &Footprint, m: &'a ModFacts) -> Vec<&'a String> {
    m.paths.iter().filter(|q| fp.cat_of(q).is_some_and(|c| c.protected()) || q.ends_with(".vnmskel_c") || q.ends_with(".vnmgraph_c") || q.ends_with(".vdata_c")).collect()
}

/// The depth grade of a mod judged as `archetype` against the hero's footprint.
pub fn grade(fp: &Footprint, m: &ModFacts, archetype: Archetype, p: &Policy) -> Report {
    let lines = lines(fp, m, archetype, p);
    let total: f64 = lines.iter().filter(|l| l.counted).map(|l| l.weight).sum();
    let got: f64 = lines.iter().filter(|l| l.counted).map(|l| l.weight * l.coverage).sum();
    let score = if total > 0.0 { 100.0 * got / total } else { 0.0 };
    let uncapped = letter(score, p);
    // Caps: about the mod working in game, never about fair play (that is compliance) or taste.
    let mut caps = Vec::new();
    if !protected(fp, m).is_empty() {
        caps.push(Cap { max: "D", why: "ships skeleton/anim-graph/game-data files; the hero breaks on the next update".into() });
    }
    if m.lint_errors > 0 {
        caps.push(Cap { max: "C", why: "lint errors mean something won't load in game".into() });
    }
    if m.dropped_deps > 0 {
        caps.push(Cap { max: "B", why: "dropped dependencies silently remove effects or sounds".into() });
    }
    // Minimum coverage for S: nobody declares a tiny scope and claims S.
    match archetype {
        Archetype::Variant => {
            if let Some(l) = lines.iter().find(|l| l.id == "materials").filter(|l| l.coverage < 1.0 - 1e-9) {
                caps.push(Cap { max: "A", why: format!("an S variant covers every visible material slot of the hero ({})", l.detail) });
            }
        }
        Archetype::Kit => {
            let depts = ["vfx", "sound", "ui", "voice"];
            let need = p.coverage.kit_departments.round() as usize;
            let touched_depts = lines.iter().filter(|l| depts.contains(&l.id) && touched(l.id, l.coverage)).count();
            if touched_depts < need {
                caps.push(Cap { max: "A", why: format!("an S kit covers at least {need} of VFX, sound, UI and voice ({touched_depts} touched)") });
            }
            let partial: Vec<&str> = lines.iter().filter(|l| l.id != "craft" && l.counted && touched(l.id, l.coverage) && l.coverage < 1.0 - 1e-9).map(|l| l.name).collect();
            if !partial.is_empty() {
                caps.push(Cap { max: "A", why: format!("an S kit covers every ability, the weapon and movement in each department it touches (not yet: {})", partial.join(", ")) });
            }
        }
        Archetype::Character => {}
    }
    let worst = caps.iter().map(|c| rank(c.max)).max().unwrap_or(0).max(rank(uncapped));
    Report { hero: fp.display.clone(), archetype, score, letter: ORDER[worst], uncapped, caps, lines }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::footprint::Item;

    fn fp() -> Footprint {
        let mut f = Footprint { hero_key: "hero_lash".into(), display: "Lash".into(), tokens: vec!["lash".into()], ..Default::default() };
        f.abilities = vec![("Signature_1".into(), "strike".into()), ("Signature_3".into(), "flog".into())];
        let mut add = |cat, g: &str, p: &str| f.items.push(Item { cat, group: g.into(), path: p.into() });
        add(Cat::Model, "core", "models/heroes_wip/lash/lash.vmdl_c");
        add(Cat::Skeleton, "core", "models/heroes_wip/lash/lash.vnmskel_c");
        for i in 0..8 {
            add(Cat::AbilityVfx, "strike", &format!("particles/abilities/lash/strike_{i}.vpcf_c"));
            add(Cat::AbilityVfx, "flog", &format!("particles/abilities/lash/flog_{i}.vpcf_c"));
            add(Cat::Sound, "strike", &format!("sounds/abilities/lash/strike_{i}.vsnd_c"));
        }
        add(Cat::Animation, "clips:ui", "models/heroes_staging/lash_v2/clips/ui_hero_select.vnmclip_c");
        add(Cat::UiArt, "ui", "panorama/images/heroes/lash_card_psd.vtex_c");
        add(Cat::Scene, "ui", "maps/ui/hero_shop/lash_shop.vpk");
        f
    }

    fn facts(paths: &[&str]) -> ModFacts {
        ModFacts { paths: paths.iter().map(|s| s.to_string()).collect(), ..Default::default() }
    }

    fn policy() -> Policy {
        Policy::default()
    }

    fn line<'a>(r: &'a Report, id: &str) -> &'a Line {
        r.lines.iter().find(|l| l.id == id).unwrap()
    }

    #[test]
    fn letters_cover_the_range_and_every_archetype_totals_100() {
        let p = policy();
        assert_eq!(letter(100.0, &p), "S");
        assert_eq!(letter(90.0, &p), "S");
        assert_eq!(letter(89.99, &p), "A");
        assert_eq!(letter(80.0, &p), "A");
        assert_eq!(letter(0.0, &p), "F");
        for a in Archetype::all() {
            let w: f64 = p.weights(a).iter().map(|(_, w)| w).sum();
            assert!((w - 100.0).abs() < 1e-9, "{}: weights must total 100, got {w}", a.id());
            let (req, app) = a.surfaces();
            for (id, _) in p.weights(a) {
                assert!(req.contains(&id.as_str()) || app.contains(&id.as_str()), "{}: weight for {id}, which it does not judge", a.id());
            }
        }
    }

    #[test]
    fn a_quarter_of_an_ability_is_a_full_reskin_of_it() {
        assert_eq!(group_credit(2, 8, GROUP_SHARE), 1.0);
        assert_eq!(group_credit(1, 8, GROUP_SHARE), 0.5);
        assert_eq!(group_credit(1, 1, GROUP_SHARE), 1.0);
    }

    #[test]
    fn model_only_skin_grades_as_a_skin() {
        let r = grade(&fp(), &facts(&["models/heroes_wip/lash/lash.vmdl_c", "models/heroes_wip/lash/materials/a.vmat_c", "models/heroes_wip/lash/materials/b.vmat_c", "models/heroes_wip/lash/materials/c.vmat_c"]), Archetype::Character, &policy());
        assert!(r.score >= 40.0 && r.score < 58.0, "score {}", r.score);
        assert_eq!(r.letter, "C");
    }

    #[test]
    fn shipping_the_skeleton_caps_at_d() {
        let r = grade(&fp(), &facts(&["models/heroes_wip/lash/lash.vmdl_c", "models/heroes_wip/lash/lash.vnmskel_c"]), Archetype::Character, &policy());
        assert_eq!(r.letter, "D");
        assert!(r.caps.iter().any(|c| c.why.contains("skeleton")));
    }

    fn everything() -> (Footprint, ModFacts) {
        let mut p: Vec<String> = fp().items.iter().filter(|i| !i.cat.protected()).map(|i| i.path.clone()).collect();
        p.extend(["models/heroes_wip/lash/materials/a.vmat_c", "models/heroes_wip/lash/materials/b.vmat_c", "models/heroes_wip/lash/materials/c.vmat_c"].map(String::from));
        p.extend((0..6).map(|i| format!("models/heroes_staging/lash_v2/clips/extra_{i}.vnmclip_c")));
        let mut f = ModFacts { paths: p.into_iter().collect(), ..Default::default() };
        f.paths.extend((0..40).map(|i| format!("sounds/vo/lash/lash_kill_{i}.vsnd_c")));
        let mut fpx = fp();
        for i in 0..40 {
            fpx.items.push(Item { cat: Cat::Voice, group: "vo:kill".into(), path: format!("sounds/vo/lash/lash_kill_{i}.vsnd_c") });
        }
        fpx.items.push(Item { cat: Cat::Animation, group: "clips:ui".into(), path: "models/heroes_staging/lash_v2/clips/ui_hero_reveal.vnmclip_c".into() });
        f.paths.insert("models/heroes_staging/lash_v2/clips/ui_hero_reveal.vnmclip_c".into());
        for i in 0..4 {
            let p = format!("models/heroes_staging/lash_v2/clips/ability_{i}.vnmclip_c");
            fpx.items.push(Item { cat: Cat::Animation, group: "clips:abilities".into(), path: p.clone() });
            f.paths.insert(p);
        }
        for i in 0..6 {
            let p = format!("models/heroes_staging/lash_v2/clips/extra_{i}.vnmclip_c");
            fpx.items.push(Item { cat: Cat::Animation, group: "clips:locomotion".into(), path: p });
        }
        (fpx, f)
    }

    #[test]
    fn everything_clean_is_s() {
        let (fpx, f) = everything();
        let r = grade(&fpx, &f, Archetype::Character, &policy());
        assert_eq!(r.letter, "S", "score {}", r.score);
        assert!(r.caps.is_empty());
    }

    #[test]
    fn a_policy_moves_the_letter_lines_and_the_weights() {
        let (fpx, mut f) = everything();
        f.lint_warnings = 1; // craft 13/15
        let r = grade(&fpx, &f, Archetype::Character, &policy());
        assert!(r.score < 100.0 && r.letter == "S", "{} {}", r.score, r.letter);
        // the S line just under the score, at it, and just over it
        let at = r.score;
        for (s, want) in [(at - 0.01, "S"), (at, "S"), (at + 0.01, "A")] {
            let mut p = policy();
            p.letters[0] = s;
            assert_eq!(grade(&fpx, &f, Archetype::Character, &p).letter, want, "S at {s}");
        }
        // moving weight onto craft lowers the same mod's score
        let mut p = policy();
        p.weights_character = vec![("model".into(), 10.0), ("materials".into(), 8.0), ("vfx".into(), 16.0), ("sound".into(), 12.0), ("animation".into(), 10.0), ("ui".into(), 10.0), ("scenes".into(), 4.0), ("voice".into(), 5.0), ("craft".into(), 25.0)];
        assert!(grade(&fpx, &f, Archetype::Character, &p).score < r.score);
    }

    #[test]
    fn materials_the_models_own_compile_references_count_wherever_they_live() {
        // Real case: a skin's materials are under materials/<skin>/, so the path-token count gave 0/8.
        let shipped = ["materials/outfit/shell.vmat_c", "materials/outfit/palette.vmat_c", "materials/outfit/henry.vmat_c", "materials/outfit/spare.vmat_c"];
        let mut f = facts(&["models/heroes_wip/lash/lash.vmdl_c"]);
        f.paths.extend(shipped.map(String::from));
        let g = |f: &ModFacts| grade(&fp(), f, Archetype::Character, &policy());
        assert_eq!(line(&g(&f), "materials").coverage, 0.0, "no token in the path, and nothing says the model uses them");
        f.model_materials = shipped[..3].iter().map(|s| s.to_string()).collect();
        assert_eq!(line(&g(&f), "materials").coverage, 1.0, "{}", line(&g(&f), "materials").detail);
        f.model_materials.remove(shipped[2]);
        assert!((line(&g(&f), "materials").coverage - 2.0 / 3.0).abs() < 1e-9);
        // a stray entry that is not among the mod's paths is not counted
        f.model_materials.insert("materials/outfit/valves_own.vmat_c".into());
        assert!((line(&g(&f), "materials").coverage - 2.0 / 3.0).abs() < 1e-9);
    }

    #[test]
    fn a_shipped_ui_scene_map_is_credited_and_only_for_the_hero_s_own_scenes() {
        let scene = "maps/ui/hero_shop/lash_shop.vpk";
        let g = |f: &ModFacts| grade(&fp(), f, Archetype::Character, &policy());
        assert_eq!(line(&g(&facts(&["models/heroes_wip/lash/lash.vmdl_c"])), "scenes").coverage, 0.0);
        let mut f = facts(&["models/heroes_wip/lash/lash.vmdl_c"]);
        f.scene_maps.insert(scene.into());
        assert_eq!(line(&g(&f), "scenes").coverage, 1.0);
        assert!(line(&g(&f), "scenes").missing.is_empty(), "a covered scene is not listed as missing");
        let mut other = facts(&["models/heroes_wip/lash/lash.vmdl_c"]);
        other.scene_maps.insert("maps/ui/hero_prefabs/abrams.vpk".into());
        assert_eq!(line(&g(&other), "scenes").coverage, 0.0);
    }

    #[test]
    fn lint_errors_cap_depth_and_fair_play_is_never_a_depth_matter() {
        let (fpx, mut f) = everything();
        f.lint_errors = 1;
        let r = grade(&fpx, &f, Archetype::Character, &policy());
        assert_eq!(r.letter, "C");
        assert!(r.caps.iter().any(|c| c.why.contains("won't load")));
        // which findings are compliance, not depth
        assert!(is_fair_play("M129", "x") && is_fair_play("M140", "x") && is_fair_play("M062", "hitbox 'default/head' changed"));
        assert!(!is_fair_play("M062", "bone 'pelvis' missing"), "a lost bone is the mod not working");
        assert!(!is_fair_play("M061", "x") && is_compliance("M144", "x"));
    }

    #[test]
    fn one_replaced_sound_is_not_an_s_kit() {
        let f = facts(&["sounds/abilities/lash/strike_0.vsnd_c"]);
        let r = grade(&fp(), &f, Archetype::Kit, &policy());
        assert_ne!(r.letter, "S", "score {}", r.score);
        assert!(r.score < 40.0, "a single sound is a fragment of a kit: {}", r.score);
        assert!(r.caps.iter().any(|c| c.why.contains("at least 2 of VFX, sound, UI and voice")), "{:?}", r.caps.iter().map(|c| &c.why).collect::<Vec<_>>());
        // the scope was padded with the heaviest untouched department, at zero
        assert!(line(&r, "vfx").counted && line(&r, "vfx").coverage == 0.0);
        assert!(!line(&r, "animation").counted, "an applicable department the kit never touched is not counted");
    }

    #[test]
    fn a_complete_kit_in_two_departments_is_s_and_a_partial_one_is_not() {
        let fpx = fp();
        let mut paths: Vec<String> = fpx.items.iter().filter(|i| matches!(i.cat, Cat::AbilityVfx | Cat::Sound)).map(|i| i.path.clone()).collect();
        let full = ModFacts { paths: paths.iter().cloned().collect(), ..Default::default() };
        let r = grade(&fpx, &full, Archetype::Kit, &policy());
        assert_eq!((r.letter, r.caps.len()), ("S", 0), "score {}", r.score);
        // drop every flog particle: VFX is touched but no longer complete, so S is out of reach whatever the score
        paths.retain(|p| !p.contains("flog_"));
        let partial = ModFacts { paths: paths.into_iter().collect(), ..Default::default() };
        let r = grade(&fpx, &partial, Archetype::Kit, &policy());
        assert_ne!(r.letter, "S");
        assert!(r.caps.iter().any(|c| c.max == "A" && c.why.contains("Ability VFX")), "{:?}", r.caps.iter().map(|c| &c.why).collect::<Vec<_>>());
    }

    #[test]
    fn a_variant_needs_every_visible_slot_for_s() {
        let slots = |covered: &[&str], missing: &[&str]| Some((covered.iter().map(|s| s.to_string()).collect(), missing.iter().map(|s| s.to_string()).collect()));
        let mut f = facts(&["models/heroes_wip/lash/materials/lash_body.vmat_c", "models/heroes_wip/lash/materials/lash_gear.vmat_c"]);
        f.material_slots = slots(&["lash_body", "lash_gear"], &[]);
        let r = grade(&fp(), &f, Archetype::Variant, &policy());
        assert_eq!(r.letter, "S", "score {}", r.score);
        assert!(!line(&r, "ui").counted && !line(&r, "scenes").counted, "a variant is not marked down for UI it never claimed");
        f.material_slots = slots(&["lash_body", "lash_gear", "lash_glove"], &["lash_bg"]);
        let r = grade(&fp(), &f, Archetype::Variant, &policy());
        assert_ne!(r.letter, "S");
        assert!(r.caps.iter().any(|c| c.why.contains("every visible material slot")));
    }
}
