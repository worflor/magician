//! The grading rubric. One home for every number a grade is judged by: the criteria and the group each belongs to,
//! their weights within a group per archetype, the two lines that name a group, the coverage targets, the identity
//! requirements and the fair-play rule classes. A policy file (`policy.rs`) may move any of them; these are the
//! defaults, and a run with no policy uses them.
//!
//! A grade describes, it does not rank: it names the kind of mod and which of the hero's three groups (look, effects,
//! sound) the mod makes its own, `some <group>` where it has begun one, and leaves out what is still Valve's. Coverage
//! is measured *per group of files* (per ability, per moment), not per file: replacing a quarter of an ability's
//! particles with intent is a full reskin of that ability. Hundreds of untouched debris sprites shouldn't hide a
//! reskinned ability, and a mod that replaces 90 files of one ability shouldn't read as covering them all.

use crate::footprint::{Cat, Footprint};
use crate::policy::Policy;
use std::collections::{BTreeMap, BTreeSet};

/// The version of the grading rules a record was judged by. Raise it when a rule's meaning changes, so an old record
/// is never read as if today's rules made it.
pub const RULES_VERSION: &str = "3.1.0";

// ---------------------------------------------------------------------------------------------------- groups

/// The three things a hero mod can make its own, in the order a label names them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Group {
    Look,
    Effects,
    Sound,
}

impl Group {
    pub fn id(self) -> &'static str {
        match self {
            Group::Look => "look",
            Group::Effects => "effects",
            Group::Sound => "sound",
        }
    }
    pub fn all() -> [Group; 3] {
        [Group::Look, Group::Effects, Group::Sound]
    }
}

/// A group is named once its weighted coverage reaches this line.
pub const COVERED: f64 = 0.8;
/// A group reads `some <group>` once its weighted coverage reaches this line (and is above nothing); under it the
/// group is left out, as Valve's.
pub const TOUCHED: f64 = 0.0;

// ---------------------------------------------------------------------------------------------------- criteria

pub struct Criterion {
    pub id: &'static str,
    pub name: &'static str,
    /// The name a group's line uses when the mod covers it.
    pub short: &'static str,
    pub group: Group,
    pub why: &'static str,
}

pub const CRITERIA: &[Criterion] = &[
    Criterion { id: "model", name: "Hero model", short: "model", group: Group::Look, why: "The silhouette players see all match. A character starts here." },
    Criterion { id: "materials", name: "Materials & textures", short: "materials", group: Group::Look, why: "Its own surfaces (new or replaced materials), not just a reshaped mesh. For a variant: every visible material slot of the hero." },
    Criterion { id: "ui", name: "UI art", short: "UI art", group: Group::Look, why: "Hero card, gloat and critical cards, minimap and small icons, name logo, ability icons." },
    Criterion { id: "scenes", name: "Showcase scenes", short: "scenes", group: Group::Look, why: "The hero shop, prefab and postgame portrait scenes." },
    Criterion { id: "vfx", name: "Ability VFX", short: "ability VFX", group: Group::Effects, why: "Every signature ability and the weapon look like *this* character. Measured per ability." },
    Criterion { id: "animation", name: "Animation", short: "animation", group: Group::Effects, why: "Hero-select and showcase clips first (where the fantasy is sold), then ability and other clips." },
    Criterion { id: "sound", name: "Kit sound", short: "kit sound", group: Group::Sound, why: "Abilities, weapon, melee and movement sound like the fantasy. Measured per group." },
    Criterion { id: "voice", name: "Voice & stingers", short: "voice", group: Group::Sound, why: "Voice treatment or stingers layered on his lines (no voice cloning)." },
];

pub fn criterion(id: &str) -> &'static Criterion {
    CRITERIA.iter().find(|c| c.id == id).expect("unknown criterion")
}

// ---------------------------------------------------------------------------------------------------- archetypes

/// Which rubric a mod is judged by. Identity (`identity.rs`) decides it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Archetype {
    /// A new character (Echo) or the same character in new clothes or a new body (Costume).
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
    /// Criteria that always count, and criteria that count once the mod touches them (a sound kit is not described
    /// by the UI it never claimed). A character counts every criterion but its voice, which is extra: a costume's
    /// sound is its kit sound, and a voice counts once the mod treats one.
    pub fn surfaces(self) -> (&'static [&'static str], &'static [&'static str]) {
        match self {
            Archetype::Character => (&["model", "materials", "ui", "scenes", "vfx", "animation", "sound"], &["voice"]),
            Archetype::Variant => (&["materials"], &["ui", "scenes", "vfx", "animation", "sound", "voice"]),
            Archetype::Kit => (&[], &["ui", "vfx", "animation", "sound", "voice"]),
        }
    }
}

/// Default weights per archetype: how much each criterion counts within its group (look, effects, sound). Only the
/// ratios inside a group matter.
pub const WEIGHTS_CHARACTER: &[(&str, f64)] = &[("model", 20.0), ("materials", 8.0), ("ui", 10.0), ("scenes", 4.0), ("vfx", 16.0), ("animation", 10.0), ("sound", 12.0), ("voice", 5.0)];
pub const WEIGHTS_VARIANT: &[(&str, f64)] = &[("materials", 65.0), ("ui", 15.0), ("scenes", 5.0), ("vfx", 16.0), ("animation", 10.0), ("sound", 12.0), ("voice", 5.0)];
pub const WEIGHTS_KIT: &[(&str, f64)] = &[("ui", 16.0), ("vfx", 26.0), ("animation", 16.0), ("sound", 20.0), ("voice", 12.0)];

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

/// The fair-play rules: what they find sets compliance, never the description. M062 counts only for hitboxes, the
/// model's camera settings (what a player can see around the hero) and its tagged sounds (footsteps enemies listen
/// for); a lost bone or attachment is the mod not working: a lint error, so `broken`.
pub const FAIR_PLAY_RULES: &[&str] = &["M062", "M063", "M081", "M082", "M124", "M129", "M130", "M131", "M140"];
/// Never allowable: anything that makes an enemy harder to read, or removes or weakens a gameplay cue. A finding of
/// one of these fails fair play whatever an allowance says.
pub const NEVER_ALLOWABLE: &[&str] = &["M062", "M063", "M082", "M124", "M129", "M131"];
/// Advisory: reported in the record, never a violation (a brighter cue is not a hidden one; the wide envelope is the
/// notice below the taste tier).
pub const ADVISORY: &[&str] = &["M081", "M130"];
/// The rule that says a fair-play check could not run: it makes fair play unknown, and no allowance changes that.
pub const UNCHECKED: &str = "M144";

/// Whether a finding is a fair-play finding (M062 only for hitboxes, camera settings and tagged sounds).
pub fn is_fair_play(code: &str, message: &str) -> bool {
    FAIR_PLAY_RULES.contains(&code) && (code != "M062" || ["hitbox", "camera settings", "tagged sounds"].iter().any(|k| message.starts_with(k)))
}

/// Whether a finding belongs to the compliance axis at all (fair play, or a fair-play check that could not run). Such
/// findings never make a mod `broken`.
pub fn is_compliance(code: &str, message: &str) -> bool {
    code == UNCHECKED || is_fair_play(code, message)
}

/// Whether fair play is enforced (`[fair_play] enforce`). Lenient by default: a fair-play finding is a warning, so
/// lint, build and install never stop on one alone, and freeform or fair play not checked is a warning at the release gate; the label and
/// the record say exactly what they say when enforced. Enforced, the same findings are errors and freeform and fair play not checked block.
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
    /// M061 findings without an allowance: an effect or sound of Valve's original that no longer plays.
    pub dropped_deps: usize,
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
    pub group: Group,
    pub weight: f64,
    pub coverage: f64,
    /// Whether the line counts toward its group (an applicable criterion the mod never touched does not).
    pub counted: bool,
    pub detail: String,
    /// What was counted, in the game's terms, for a group's line when the criterion is only partly covered.
    pub fact: String,
    /// Most valuable missing items for this criterion, best first.
    pub missing: Vec<String>,
}

impl Line {
    /// How the group's line names this criterion: its short name when covered, Valve's when untouched, else the count.
    fn phrase(&self) -> String {
        if self.coverage >= 1.0 - 1e-9 {
            self.name_short().to_string()
        } else if self.coverage <= 0.0 {
            format!("Valve's {}", self.name_short())
        } else {
            self.fact.clone()
        }
    }
    fn name_short(&self) -> &'static str {
        criterion(self.id).short
    }
}

/// Where a group stands: the mod covers it, has begun it (`some`), or it is still Valve's (left out of the label).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Covered,
    Some,
    Valve,
}

impl State {
    pub fn id(self) -> &'static str {
        match self {
            State::Covered => "covered",
            State::Some => "some",
            State::Valve => "valve",
        }
    }
}

pub struct GroupLine {
    pub group: Group,
    /// The weighted coverage of the group's counted criteria, 0 to 1.
    pub coverage: f64,
    pub state: State,
    /// What was counted, in the game's terms: `model, materials, UI art 11 of 13, scenes 1 of 3`, or `Valve's`.
    pub counted: String,
}

pub struct Report {
    pub hero: String,
    pub archetype: Archetype,
    pub lines: Vec<Line>,
    pub groups: Vec<GroupLine>,
    /// Plain facts about the mod working that are not lint errors (those make it `broken`): files that break the hero
    /// on the next update, effects or sounds that no longer play.
    pub notes: Vec<String>,
}

impl Report {
    /// The groups as the label names them: `look, some effects`, or `nothing measured` when every group is Valve's.
    pub fn description(&self) -> String {
        let named: Vec<String> = self
            .groups
            .iter()
            .filter_map(|g| match g.state {
                State::Covered => Some(g.group.id().to_string()),
                State::Some => Some(format!("some {}", g.group.id())),
                State::Valve => None,
            })
            .collect();
        if named.is_empty() {
            "nothing measured".into()
        } else {
            named.join(", ")
        }
    }
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

/// One criterion measured per group of files: its average credit, the detail, the missing groups (named) worst
/// first, and which groups are covered.
struct Grouped {
    coverage: f64,
    detail: String,
    missing: Vec<String>,
    all: Vec<String>,
    done: BTreeSet<String>,
    /// Groups begun but not covered.
    begun: BTreeSet<String>,
}

fn grouped_line(fp: &Footprint, m: &ModFacts, cat: Cat, share: f64, keep: impl Fn(&str) -> bool) -> Grouped {
    let gs = groups(fp, cat, keep);
    if gs.is_empty() {
        return Grouped { coverage: 1.0, detail: "nothing to cover".into(), missing: vec![], all: vec![], done: BTreeSet::new(), begun: BTreeSet::new() };
    }
    let mut sum = 0.0;
    let mut missing = Vec::new();
    let mut done = BTreeSet::new();
    let mut begun = BTreeSet::new();
    for (g, (n, paths)) in &gs {
        let r = paths.iter().filter(|p| m.paths.contains(*p)).count();
        let c = group_credit(r, *n, share);
        sum += c;
        if c >= 1.0 {
            done.insert(g.clone());
        } else {
            if c > 0.0 {
                begun.insert(g.clone());
            }
            // Name the effect or sound itself before its textures and materials.
            let mut todo: Vec<&String> = paths.iter().filter(|p| !m.paths.contains(*p)).collect();
            todo.sort_by_key(|p| (!(p.ends_with(".vpcf_c") || p.ends_with(".vsnd_c")), p.len()));
            let first = todo.first().map(|s| s.to_string()).unwrap_or_default();
            missing.push(format!("{}: {r}/{n} replaced, e.g. {first}", pretty_group(fp, g)));
        }
    }
    Grouped { coverage: sum / gs.len() as f64, detail: format!("{}/{} groups covered", done.len(), gs.len()), missing, all: gs.into_keys().collect(), done, begun }
}

/// The groups of a kit criterion in the game's terms, the ones the mod touches: `4 of 4 abilities (2 in part), the
/// weapon and part of movement`.
fn kit_phrase(g: &Grouped) -> String {
    let named = ["weapon", "melee", "movement"];
    let abilities: Vec<&String> = g.all.iter().filter(|x| !named.contains(&x.as_str())).collect();
    let mut parts = Vec::new();
    if !abilities.is_empty() {
        let done = abilities.iter().filter(|a| g.done.contains(**a)).count();
        let begun = abilities.iter().filter(|a| g.begun.contains(**a)).count();
        let part = if begun > 0 { format!(" ({begun} in part)") } else { String::new() };
        parts.push(format!("{} of {} abilit{}{part}", done + begun, abilities.len(), if abilities.len() == 1 { "y" } else { "ies" }));
    }
    for x in named {
        let name = if x == "weapon" { "the weapon" } else { x };
        if g.done.contains(x) {
            parts.push(name.to_string());
        } else if g.begun.contains(x) {
            parts.push(format!("part of {name}"));
        }
    }
    match parts.len() {
        0 => "none of its parts".into(),
        1 => parts.remove(0),
        _ => {
            let last = parts.pop().unwrap_or_default();
            format!("{} and {last}", parts.join(", "))
        }
    }
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// Whether the mod touches anything of this criterion at all (an applicable criterion counts once it does).
fn touched(coverage: f64) -> bool {
    coverage > 0.0
}

/// Every criterion's coverage for this mod against the hero's footprint.
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
    let mut push = |id: &'static str, coverage: f64, detail: String, fact: String, missing: Vec<String>| {
        let c = criterion(id);
        out.push(Line { id, name: c.name, group: c.group, weight: 0.0, coverage: coverage.clamp(0.0, 1.0), counted: true, detail, fact, missing });
    };

    // Model
    let hero_model = fp.hero_model().map(String::from);
    let has_model = hero_model.as_ref().is_some_and(|p| m.paths.contains(p));
    push("model", if has_model { 1.0 } else { 0.0 }, if has_model { "replaced".into() } else { "not replaced".into() }, String::new(), if has_model { vec![] } else { hero_model.into_iter().collect() });

    // Materials. A variant is judged slot by slot: every visible material slot of Valve's hero model. Everything else
    // counts the hero's own materials: named by its path, or used by its own compiled model (a mod's materials may
    // live under a folder of its own).
    match (&m.material_slots, archetype) {
        (Some((covered, missing)), Archetype::Variant) => {
            let total = covered.len() + missing.len();
            let c = if total == 0 { 0.0 } else { covered.len() as f64 / total as f64 };
            push("materials", c, format!("{}/{total} visible material slots of the hero covered", covered.len()), format!("materials on {} of {total} visible slots", covered.len()), missing.clone());
        }
        _ => {
            let mats = m.paths.iter().filter(|q| q.ends_with(".vmat_c") && (token(q) || m.model_materials.contains(*q))).count();
            let full = cv.materials_full;
            push(
                "materials",
                mats as f64 / full,
                format!("{mats} hero material(s), {} used by the hero's model", m.model_materials.len()),
                plural(mats, "hero material", "hero materials"),
                if mats as f64 >= full { vec![] } else { vec![format!("ship the outfit's own materials ({full}+ for full credit)")] },
            );
        }
    }

    // UI art: hero art (70%) + ability icons (30%).
    let hero_art: Vec<&String> = fp.by_cat(Cat::UiArt).filter(|i| i.group == "ui").map(|i| &i.path).collect();
    let icons: Vec<&String> = fp.by_cat(Cat::UiArt).filter(|i| i.group != "ui").map(|i| &i.path).collect();
    let ha = hero_art.iter().filter(|q| m.paths.contains(**q)).count();
    let ic = icons.iter().filter(|q| m.paths.contains(**q)).count();
    // a hero with no ability icons (or no hero art) in its footprint is judged on the part it has
    let parts = [(0.7, hero_art.len(), (ha as f64) / (hero_art.len() as f64 * cv.ui_hero_art_share)), (0.3, icons.len(), (ic as f64) / (icons.len() as f64))];
    let (got, of) = parts.iter().filter(|p| p.1 > 0).fold((0.0, 0.0), |(g, o), &(w, _, c)| (g + w * c.min(1.0), o + w));
    let ui = if of > 0.0 { got / of } else { 0.0 };
    let ui_missing: Vec<String> = hero_art.iter().chain(icons.iter()).filter(|q| !m.paths.contains(**q)).map(|q| q.to_string()).collect();
    push("ui", ui, format!("hero art {ha}/{}, ability icons {ic}/{}", hero_art.len(), icons.len()), format!("UI art {} of {}", ha + ic, hero_art.len() + icons.len()), ui_missing);

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
        format!("scenes {sc} of {}", scenes.len()),
        scenes.iter().filter(|s| !covered.contains(s)).map(|s| s.to_string()).collect(),
    );

    // Ability VFX, per signature ability + weapon.
    let v = grouped_line(fp, m, Cat::AbilityVfx, cv.group_share, |g| signature.contains(g));
    push("vfx", v.coverage, v.detail.clone(), format!("VFX for {}", kit_phrase(&v)), v.missing.clone());

    // Animation: UI clips weigh most.
    let clips = |g: &str| {
        let all: Vec<&String> = fp.by_cat(Cat::Animation).filter(|i| i.group == g).map(|i| &i.path).collect();
        let r = all.iter().filter(|q| m.paths.contains(**q)).count();
        (r, all.len())
    };
    let (ui_r, ui_n) = clips("clips:ui");
    let (ab_r, ab_n) = clips("clips:abilities");
    let others = ["clips:melee", "clips:reactions", "clips:locomotion", "clips:aim"].map(clips);
    let (other_r, other_n): (usize, usize) = (others.iter().map(|c| c.0).sum(), others.iter().map(|c| c.1).sum());
    let all_clips = fp.by_cat(Cat::Animation).count();
    let all_r = fp.by_cat(Cat::Animation).filter(|i| m.paths.contains(&i.path)).count();
    // the select and death poses inside the model always exist, and count as two hero-select clips
    let ui_r = ui_r + if m.model_anim_changed { 2 } else { 0 };
    // A band is full at its target or at every clip the hero has in it, whichever is fewer; a hero with no clips in
    // a band is not judged on it.
    let bands = [(0.5, ui_r, ui_n + 2, cv.anim_ui_full), (0.3, ab_r, ab_n, cv.anim_abilities_full), (0.2, other_r, other_n, cv.anim_other_full)];
    let (got, of) = bands.iter().filter(|b| b.2 > 0).fold((0.0, 0.0), |(g, o), &(w, r, n, full)| (g + w * (r as f64 / full.min(n as f64)).min(1.0), o + w));
    let anim = if of > 0.0 { got / of } else { 0.0 };
    let mut anim_missing = vec![];
    for (what, (_, r, n, full)) in ["hero-select/showcase clips (the in-model poses count as two)", "ability clips", "idle/locomotion/melee/reaction clips"].iter().zip(bands) {
        let need = full.min(n as f64);
        if n > 0 && (r as f64) < need {
            anim_missing.push(format!("{what}: {r}/{n} replaced ({need} for full credit)"));
        }
    }
    push(
        "animation",
        anim,
        format!("ui {ui_r}{}, abilities {ab_r}, other {other_r}", if m.model_anim_changed { " (incl. in-model select/death poses)" } else { "" }),
        format!("{all_r} of {all_clips} clips{}", if m.model_anim_changed { " and the in-model poses" } else { "" }),
        anim_missing,
    );

    // Kit sound: signature abilities, weapon, melee, movement. New sound files count too.
    let s = grouped_line(fp, m, Cat::Sound, cv.group_share, |g| signature.contains(g) || g == "melee" || g == "movement");
    let new_sounds = m.paths.iter().filter(|q| q.ends_with(".vsnd_c") && fp.cat_of(q).is_none() && token(q)).count();
    let events_replaced = m.paths.iter().any(|q| q.starts_with("soundevents/hero/") && token(q));
    let custom = if events_replaced { (new_sounds as f64 / cv.new_sounds_full).min(1.0) * 0.85 } else { 0.0 };
    push(
        "sound",
        s.coverage.max(custom),
        format!("{}; {new_sounds} new sound file(s){}", s.detail, if events_replaced { ", events rewired" } else { "" }),
        format!("sound for {}{}", kit_phrase(&s), if events_replaced && new_sounds > 0 { format!(" ({})", plural(new_sounds, "new sound", "new sounds")) } else { String::new() }),
        s.missing,
    );

    // Voice & stingers: any VO-side treatment.
    let vo = m.paths.iter().filter(|q| fp.cat_of(q) == Some(Cat::Voice)).count();
    let vo_all = fp.by_cat(Cat::Voice).count();
    push(
        "voice",
        vo as f64 / cv.voice_full,
        format!("{vo} voice file(s) treated"),
        format!("voice {vo} of {vo_all} lines"),
        if vo as f64 >= cv.voice_full { vec![] } else { vec!["stingers on kill/select/emote lines (vo:kill, vo:select, vo:emote)".into()] },
    );

    // Weights and scope for the archetype.
    let (required, applicable) = archetype.surfaces();
    let weights = p.weights(archetype);
    out.retain(|l| required.contains(&l.id) || applicable.contains(&l.id));
    for l in &mut out {
        l.weight = weights.iter().find(|(id, _)| id == l.id).map_or(0.0, |(_, w)| *w);
        l.counted = required.contains(&l.id) || touched(l.coverage);
    }
    out
}

/// Files a mod must never ship: they replace the hero's real skeleton, graph or game data.
fn protected<'a>(fp: &Footprint, m: &'a ModFacts) -> Vec<&'a String> {
    m.paths.iter().filter(|q| fp.cat_of(q).is_some_and(|c| c.protected()) || q.ends_with(".vnmskel_c") || q.ends_with(".vnmgraph_c") || q.ends_with(".vdata_c")).collect()
}

/// A group's weighted coverage, its state under the policy's lines, and what was counted. The line names every
/// criterion of the group in the rubric's scope, a part not yet counted as Valve's (`kit sound, Valve's voice`).
fn group_line(group: Group, lines: &[Line], p: &Policy) -> GroupLine {
    let parts: Vec<&Line> = lines.iter().filter(|l| l.group == group && l.counted).collect();
    let scope: Vec<&Line> = lines.iter().filter(|l| l.group == group).collect();
    let total: f64 = parts.iter().map(|l| l.weight).sum();
    let coverage = if total > 0.0 { parts.iter().map(|l| l.weight * l.coverage).sum::<f64>() / total } else { 0.0 };
    let state = if coverage <= 0.0 {
        State::Valve
    } else if coverage >= p.groups.covered - 1e-9 {
        State::Covered
    } else if coverage >= p.groups.touched - 1e-9 {
        State::Some
    } else {
        State::Valve
    };
    let counted = if coverage <= 0.0 {
        "Valve's".to_string()
    } else {
        let said: Vec<String> = scope.iter().map(|l| l.phrase()).collect();
        said.join(if said.iter().any(|s| s.contains(',')) { "; " } else { ", " })
    };
    GroupLine { group, coverage, state, counted }
}

/// What a mod judged as `archetype` makes its own of the hero, against the hero's footprint.
pub fn grade(fp: &Footprint, m: &ModFacts, archetype: Archetype, p: &Policy) -> Report {
    let lines = lines(fp, m, archetype, p);
    let groups = Group::all().into_iter().map(|g| group_line(g, &lines, p)).collect();
    let mut notes = Vec::new();
    let prot = protected(fp, m);
    if let Some(first) = prot.first() {
        notes.push(format!("ships {}, which replace the hero's own and break it on the next update (e.g. {first})", plural(prot.len(), "skeleton, anim graph or game data file", "skeleton, anim graph or game data files")));
    }
    if m.dropped_deps > 0 {
        notes.push(format!("{} (M061): effects or sounds of Valve's originals that no longer play", plural(m.dropped_deps, "dropped dependency", "dropped dependencies")));
    }
    Report { hero: fp.display.clone(), archetype, lines, groups, notes }
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

    fn group(r: &Report, g: Group) -> &GroupLine {
        r.groups.iter().find(|x| x.group == g).unwrap()
    }

    const MATS: [&str; 3] = ["models/heroes_wip/lash/materials/a.vmat_c", "models/heroes_wip/lash/materials/b.vmat_c", "models/heroes_wip/lash/materials/c.vmat_c"];

    #[test]
    fn every_criterion_has_a_group_and_every_weight_a_criterion_it_judges() {
        let p = policy();
        for g in Group::all() {
            assert!(CRITERIA.iter().any(|c| c.group == g), "{} has no criterion", g.id());
        }
        // the criteria are in label order: look, then effects, then sound
        assert!(CRITERIA.windows(2).all(|w| w[0].group <= w[1].group));
        for a in Archetype::all() {
            let (req, app) = a.surfaces();
            for (id, _) in p.weights(a) {
                assert!(req.contains(&id.as_str()) || app.contains(&id.as_str()), "{}: weight for {id}, which it does not judge", a.id());
            }
            for id in req.iter().chain(app) {
                assert!(p.weights(a).iter().any(|(w, _)| w == id), "{}: no weight for {id}", a.id());
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
    fn a_model_and_its_materials_are_some_look_and_the_rest_is_valve_s() {
        let mut paths = vec!["models/heroes_wip/lash/lash.vmdl_c"];
        paths.extend(MATS);
        let r = grade(&fp(), &facts(&paths), Archetype::Character, &policy());
        assert_eq!(r.description(), "some look");
        let look = group(&r, Group::Look);
        assert_eq!((look.state, look.counted.as_str()), (State::Some, "model, materials, Valve's UI art, Valve's scenes"));
        assert_eq!(group(&r, Group::Effects).counted, "Valve's");
        assert!(r.notes.is_empty());
    }

    #[test]
    fn the_hero_s_model_is_the_one_heroes_vdata_names_not_the_first_core_model() {
        // Paige's core group also holds an effect model whose path sorts before hers (models/abilities/...)
        let mut f = fp();
        f.items.insert(0, Item { cat: Cat::Model, group: "core".into(), path: "models/abilities/lash_butterfly_model.vmdl_c".into() });
        f.model = "models/heroes_wip/lash/lash.vmdl_c".into();
        assert_eq!(f.hero_model(), Some("models/heroes_wip/lash/lash.vmdl_c"));
        let r = grade(&f, &facts(&["models/heroes_wip/lash/lash.vmdl_c"]), Archetype::Character, &policy());
        assert_eq!(line(&r, "model").detail, "replaced");
        // a footprint made by hand, with no model named, still finds its one core model
        assert_eq!(fp().hero_model(), Some("models/heroes_wip/lash/lash.vmdl_c"));
    }

    #[test]
    fn shipping_the_skeleton_is_a_note_not_a_lower_description() {
        let r = grade(&fp(), &facts(&["models/heroes_wip/lash/lash.vmdl_c", "models/heroes_wip/lash/lash.vnmskel_c"]), Archetype::Character, &policy());
        assert_eq!(r.description(), "some look");
        assert!(r.notes[0].starts_with("ships 1 skeleton, anim graph or game data file") && r.notes[0].contains("lash.vnmskel_c"), "{:?}", r.notes);
        let mut f = facts(&["models/heroes_wip/lash/lash.vmdl_c"]);
        f.dropped_deps = 3;
        assert_eq!(grade(&fp(), &f, Archetype::Character, &policy()).notes, vec!["3 dropped dependencies (M061): effects or sounds of Valve's originals that no longer play".to_string()]);
    }

    fn everything() -> (Footprint, ModFacts) {
        let mut p: Vec<String> = fp().items.iter().filter(|i| !i.cat.protected()).map(|i| i.path.clone()).collect();
        p.extend(MATS.map(String::from));
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
            fpx.items.push(Item { cat: Cat::Animation, group: "clips:locomotion".into(), path: p.clone() });
            f.paths.insert(p);
        }
        (fpx, f)
    }

    #[test]
    fn everything_is_look_effects_sound_in_plain_names() {
        let (fpx, f) = everything();
        let r = grade(&fpx, &f, Archetype::Character, &policy());
        assert_eq!(r.description(), "look, effects, sound");
        let said: Vec<&str> = r.groups.iter().map(|g| g.counted.as_str()).collect();
        assert_eq!(said, ["model, materials, UI art, scenes", "ability VFX, animation", "kit sound, voice"]);
    }

    #[test]
    fn a_partial_group_says_what_was_counted_in_the_game_s_terms() {
        let (fpx, mut f) = everything();
        // every flog particle and most clips back to Valve's, and a quarter of the voice
        f.paths.retain(|p| !p.contains("flog_") && !p.contains("extra_") && !p.contains("ability_"));
        f.paths.retain(|p| p.strip_prefix("sounds/vo/lash/lash_kill_").and_then(|n| n.strip_suffix(".vsnd_c")).is_none_or(|n| n.parse::<usize>().unwrap() < 10));
        let r = grade(&fpx, &f, Archetype::Character, &policy());
        let effects = group(&r, Group::Effects);
        assert_eq!(effects.counted, "VFX for 1 of 2 abilities, 2 of 12 clips");
        assert_eq!(effects.state, State::Some, "{}", effects.coverage);
        assert_eq!(group(&r, Group::Sound).counted, "kit sound, voice 10 of 40 lines");
        assert_eq!(r.description(), "look, some effects, some sound");
    }

    #[test]
    fn a_character_s_voice_counts_once_touched_and_shows_either_way() {
        let (fpx, mut f) = everything();
        f.paths.retain(|p| !p.contains("/vo/"));
        let r = grade(&fpx, &f, Archetype::Character, &policy());
        assert_eq!(r.description(), "look, effects, sound", "an untouched voice does not hold sound back");
        assert_eq!(group(&r, Group::Sound).counted, "kit sound, Valve's voice");
        assert!(!line(&r, "voice").counted);
        // a voice begun counts: a quarter of the lines treated
        f.paths.extend((0..10).map(|i| format!("sounds/vo/lash/lash_kill_{i}.vsnd_c")));
        let r = grade(&fpx, &f, Archetype::Character, &policy());
        assert_eq!((r.description().as_str(), group(&r, Group::Sound).counted.as_str()), ("look, effects, some sound", "kit sound, voice 10 of 40 lines"));
    }

    #[test]
    fn the_two_lines_are_the_policy_s() {
        let (fpx, mut f) = everything();
        f.paths.retain(|p| !p.contains("flog_") && !p.contains("extra_") && !p.contains("ability_"));
        let r = grade(&fpx, &f, Archetype::Character, &policy());
        let c = group(&r, Group::Effects).coverage;
        assert!(c > 0.1 && c < COVERED, "{c}");
        let at = |covered: f64, touched: f64| {
            let p = crate::policy::parse(&format!("[groups]\ncovered = {covered}\ntouched = {touched}\n"), "p").unwrap();
            grade(&fpx, &f, Archetype::Character, &p).groups[1].state
        };
        assert_eq!(at(c, 0.0), State::Covered, "a group is named at the line");
        assert_eq!(at(c + 0.01, 0.0), State::Some);
        assert_eq!(at(c + 0.02, c + 0.01), State::Valve, "under the touched line it is left out");
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
        assert_eq!(line(&g(&f), "materials").phrase(), "2 hero materials");
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
    fn which_findings_are_fair_play() {
        assert!(is_fair_play("M129", "x") && is_fair_play("M140", "x") && is_fair_play("M062", "hitbox 'default/head' changed"));
        assert!(!is_fair_play("M062", "bone 'pelvis' missing"), "a lost bone is the mod not working");
        assert!(is_fair_play("M062", "camera settings: m_flCameraBackOffset 125 (Valve's 111): the third-person camera sits elsewhere"), "what a player sees around the hero");
        assert!(is_fair_play("M062", "tagged sounds: TaggedSoundSettings_t missing"), "silent footsteps are a hidden cue");
        assert!(!is_fair_play("M062", "game data: CitadelUnitStatusSettings_t is a list"), "a health bar at the feet is the mod not working");
        assert!(!is_fair_play("M061", "x") && is_compliance("M144", "x"));
    }

    #[test]
    fn a_kit_is_described_by_what_it_touches() {
        // one replaced sound: some sound, nothing else
        let r = grade(&fp(), &facts(&["sounds/abilities/lash/strike_0.vsnd_c"]), Archetype::Kit, &policy());
        assert_eq!(r.description(), "some sound");
        assert_eq!(group(&r, Group::Sound).counted, "sound for 1 of 1 ability (1 in part), Valve's voice");
        assert!(!line(&r, "vfx").counted && !line(&r, "voice").counted, "a kit is not described by what it never touched");
        // every particle and sound of the kit
        let fpx = fp();
        let mut paths: Vec<String> = fpx.items.iter().filter(|i| matches!(i.cat, Cat::AbilityVfx | Cat::Sound)).map(|i| i.path.clone()).collect();
        let full = ModFacts { paths: paths.iter().cloned().collect(), ..Default::default() };
        let r = grade(&fpx, &full, Archetype::Kit, &policy());
        assert_eq!(r.description(), "effects, sound");
        // drop every flog particle: half the abilities' effects
        paths.retain(|p| !p.contains("flog_"));
        let partial = ModFacts { paths: paths.into_iter().collect(), ..Default::default() };
        let r = grade(&fpx, &partial, Archetype::Kit, &policy());
        assert_eq!(r.description(), "some effects, sound");
        // nothing of the hero's own measured
        let r = grade(&fpx, &facts(&["sounds/lash/new_whip.vsnd_c"]), Archetype::Kit, &policy());
        assert_eq!(r.description(), "nothing measured");
    }

    #[test]
    fn a_variant_is_described_slot_by_slot_and_by_whatever_else_it_touches() {
        let slots = |covered: &[&str], missing: &[&str]| Some((covered.iter().map(|s| s.to_string()).collect(), missing.iter().map(|s| s.to_string()).collect()));
        let mut f = facts(&["models/heroes_wip/lash/materials/lash_body.vmat_c", "models/heroes_wip/lash/materials/lash_gear.vmat_c"]);
        f.material_slots = slots(&["lash_body", "lash_gear"], &[]);
        let r = grade(&fp(), &f, Archetype::Variant, &policy());
        assert_eq!(r.description(), "look");
        assert!(!line(&r, "ui").counted && !line(&r, "scenes").counted, "a variant is not described by UI it never claimed");
        f.material_slots = slots(&["lash_body"], &["lash_bg", "lash_glove"]);
        let r = grade(&fp(), &f, Archetype::Variant, &policy());
        assert_eq!((r.description().as_str(), group(&r, Group::Look).counted.as_str()), ("some look", "materials on 1 of 3 visible slots, Valve's UI art, Valve's scenes"));
        // a recolour that also reskins an ability's particles says so
        f.paths.extend((0..2).map(|i| format!("particles/abilities/lash/strike_{i}.vpcf_c")));
        assert_eq!(grade(&fp(), &f, Archetype::Variant, &policy()).description(), "some look, some effects");
    }
}
