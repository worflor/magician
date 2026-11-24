//! Identity: what kind of mod this is, decided by signals and stored as data. The label word is a projection of
//! identity and depth (`word`); identity never changes with the letter, so an Echolite becomes an Echo the moment its
//! depth reaches S.
//!
//! | Identity | Rubric | Word |
//! |---|---|---|
//! | new character (at least `identity.new_character_signals` signals) | character | Echo at S, Echolite below |
//! | same character | character | Costume |
//! | same model: Valve's mesh kept, new materials | variant | Variant |
//! | no model: VFX, sound, UI or voice | kit | Kit |
//! | not a hero mod | none | not graded |
//!
//! The signals of a new character: `head_replaced` (the head and face geometry is not Valve's), `renamed` (a new name
//! logo or a new name in the mod's localisation), `new_hero_art` (the portrait and card art replaced) and
//! `voice_identity` (a new voice identity, declared in `mod.toml`, worth half a signal by default: bytes can't tell a
//! new voice from a treatment of the original lines, and a treatment does not count on its own). A new character
//! needs its model replaced.

use crate::evidence::Geometry;
use crate::policy::Policy;
use crate::rubric::Archetype;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    New,
    Same,
    SameModel,
    NoModel,
    NotHero,
}

impl Kind {
    pub fn id(self) -> &'static str {
        match self {
            Kind::New => "new",
            Kind::Same => "same",
            Kind::SameModel => "same_model",
            Kind::NoModel => "no_model",
            Kind::NotHero => "not_hero",
        }
    }
    pub fn archetype(self) -> Option<Archetype> {
        match self {
            Kind::New | Kind::Same => Some(Archetype::Character),
            Kind::SameModel => Some(Archetype::Variant),
            Kind::NoModel => Some(Archetype::Kit),
            Kind::NotHero => None,
        }
    }
}

/// The label word for an identity at a letter.
pub fn word(kind: Kind, letter: &str) -> &'static str {
    match kind {
        Kind::New if letter == "S" => "Echo",
        Kind::New => "Echolite",
        Kind::Same => "Costume",
        Kind::SameModel => "Variant",
        Kind::NoModel => "Kit",
        Kind::NotHero => "not graded",
    }
}

/// What was measured about the mod, for the decision.
#[derive(Debug, Clone, Default)]
pub struct Signals {
    /// The hero's own model (the footprint's core model) is in the mod.
    pub model_replaced: bool,
    /// Our model against Valve's at the same path; `Err` says why it could not be compared.
    pub geometry: Option<Result<Geometry, String>>,
    /// Evidence of a rename (`evidence::renamed`); empty for none.
    pub renamed: Vec<String>,
    /// What was seen but does not count as a rename (a new logo that keeps the hero's name).
    pub rename_notes: Vec<String>,
    /// Portrait and card art: replaced of total.
    pub portrait: (usize, usize),
    /// `mod.toml [identity] voice = "new"`.
    pub voice_declared: bool,
    /// The mod ships the hero's own materials or textures at Valve's paths.
    pub materials_replaced: bool,
    /// The mod touches anything of the hero at all (a footprint file, or a path with the hero's name).
    pub touches_hero: bool,
    /// What a mod that touches nothing of the hero looks like, for "not graded (...)".
    pub other_kind: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Identity {
    pub kind: Kind,
    /// The signals present, by name.
    pub signals: Vec<&'static str>,
    pub score: f64,
    pub required: f64,
    /// Every signal checked, with what was measured (true or false), in a fixed order.
    pub evidence: Vec<(&'static str, String)>,
    /// For a mod that is not graded: what it is ("HUD mod").
    pub not_graded: Option<String>,
}

pub fn decide(s: &Signals, p: &Policy) -> Identity {
    let ip = &p.identity;
    let mut signals = Vec::new();
    let mut score = 0.0;
    let mut evidence = Vec::new();
    // head_replaced
    let head = match (&s.geometry, s.model_replaced) {
        (_, false) => (false, "the hero's model is not replaced".to_string()),
        (None, true) => (false, "not measured: Valve's model was not available to compare".to_string()),
        (Some(Err(e)), true) => (false, format!("not measured: {e}")),
        (Some(Ok(g)), true) if g.head == 0 && g.valve_head > 0 => (true, format!("none of our vertices weight to the head, Valve's has {} that do", g.valve_head)),
        (Some(Ok(g)), true) if g.head == 0 => (false, "not measured: neither model weights vertices to a head bone".to_string()),
        (Some(Ok(g)), true) => {
            let share = g.head_share();
            let kept = share >= ip.head_kept_share;
            let verdict = if kept { format!("Valve's head: {:.0}% or more", 100.0 * ip.head_kept_share) } else { format!("replaced: under {:.0}%", 100.0 * ip.head_kept_share) };
            (!kept, format!("{:.0}% of our {} head-weighted vertices sit on Valve's head ({verdict})", 100.0 * share, g.head))
        }
    };
    if head.0 {
        signals.push("head_replaced");
        score += 1.0;
    }
    evidence.push(("head_replaced", head.1));
    // renamed
    if s.renamed.is_empty() {
        let none = "no new name logo, and no localisation of the mod renames the hero".to_string();
        evidence.push(("renamed", if s.rename_notes.is_empty() { none } else { s.rename_notes.join("; ") }));
    } else {
        signals.push("renamed");
        score += 1.0;
        evidence.push(("renamed", s.renamed.join("; ")));
    }
    // new_hero_art
    let (r, n) = s.portrait;
    let art = n > 0 && r as f64 >= ip.portrait_share * n as f64;
    if art {
        signals.push("new_hero_art");
        score += 1.0;
    }
    evidence.push(("new_hero_art", format!("{r} of the hero's {n} portrait and card images replaced ({:.0}% needed)", 100.0 * ip.portrait_share)));
    // voice_identity
    if s.voice_declared {
        signals.push("voice_identity");
        score += ip.voice_identity_weight;
        evidence.push(("voice_identity", format!("declared in mod.toml ([identity] voice = \"new\"), worth {} of a signal", ip.voice_identity_weight)));
    } else {
        evidence.push(("voice_identity", "not declared (a treatment of the original lines does not count on its own)".to_string()));
    }

    let mesh_kept = match &s.geometry {
        Some(Ok(g)) => g.mesh_share() >= ip.valve_mesh_share,
        _ => false,
    };
    let (kind, not_graded) = if s.model_replaced {
        if score >= ip.new_character_signals - 1e-9 {
            (Kind::New, None)
        } else if mesh_kept {
            (Kind::SameModel, None)
        } else {
            (Kind::Same, None)
        }
    } else if s.materials_replaced {
        (Kind::SameModel, None)
    } else if s.touches_hero {
        (Kind::NoModel, None)
    } else {
        (Kind::NotHero, Some(s.other_kind.clone()))
    };
    if let Some(Ok(g)) = &s.geometry {
        let verdict = if mesh_kept { format!("Valve's mesh kept: {:.0}% or more", 100.0 * ip.valve_mesh_share) } else { format!("a new mesh: under {:.0}%", 100.0 * ip.valve_mesh_share) };
        evidence.push(("valve_mesh", format!("{:.1}% of our {} vertices sit on Valve's mesh ({verdict})", 100.0 * g.mesh_share(), g.vertices)));
    }
    Identity { kind, signals, score, required: ip.new_character_signals, evidence, not_graded }
}

/// What a mod that touches nothing of the hero is, from its paths: a map, HUD or music mod, or something else.
pub fn other_kind<'a>(paths: impl Iterator<Item = &'a String>) -> String {
    let (mut maps, mut hud, mut music, mut n) = (0, 0, 0, 0);
    for p in paths {
        n += 1;
        if p.starts_with("maps/") {
            maps += 1;
        } else if p.starts_with("panorama/") || p.starts_with("resource/") {
            hud += 1;
        } else if p.starts_with("sounds/music/") || p.starts_with("soundevents/music") {
            music += 1;
        }
    }
    let most = maps.max(hud).max(music);
    if n == 0 || most * 2 < n {
        "not a hero mod".into()
    } else if most == maps {
        "map mod".into()
    } else if most == hud {
        "HUD mod".into()
    } else {
        "music mod".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn geometry(head: usize, head_on: usize, verts: usize, on: usize) -> Option<Result<Geometry, String>> {
        Some(Ok(Geometry { vertices: verts, on_valve: on, head, head_on_valve: head_on, valve_head: head }))
    }

    fn p() -> Policy {
        Policy::default()
    }

    #[test]
    fn a_new_character_and_echo_only_at_s() {
        // head replaced, renamed, new hero art: three signals
        let s = Signals { model_replaced: true, geometry: geometry(4000, 120, 90000, 20000), renamed: vec!["replaces the name logo x".into()], portrait: (5, 6), ..Default::default() };
        let id = decide(&s, &p());
        assert_eq!(id.kind, Kind::New);
        assert_eq!(id.signals, vec!["head_replaced", "renamed", "new_hero_art"]);
        assert_eq!((id.score, id.required), (3.0, 2.0));
        assert_eq!((word(id.kind, "S"), word(id.kind, "B")), ("Echo", "Echolite"));
        assert_eq!(id.kind.archetype(), Some(Archetype::Character));
    }

    #[test]
    fn maid_lash_keeps_valve_s_head_and_is_a_costume() {
        // Valve's head, new card art: one signal
        let s = Signals { model_replaced: true, geometry: geometry(3000, 2950, 120000, 40000), portrait: (8, 9), ..Default::default() };
        let id = decide(&s, &p());
        assert_eq!(id.kind, Kind::Same);
        assert_eq!(id.signals, vec!["new_hero_art"]);
        assert_eq!(word(id.kind, "S"), "Costume");
        assert!(id.evidence[0].1.contains("sit on Valve's head (Valve's head: 50% or more)"), "{:?}", id.evidence);
    }

    #[test]
    fn a_declared_voice_is_half_a_signal() {
        let s = Signals { model_replaced: true, geometry: geometry(3000, 2950, 100, 10), portrait: (6, 6), voice_declared: true, ..Default::default() };
        let id = decide(&s, &p());
        assert_eq!((id.kind, id.score), (Kind::Same, 1.5), "art and a voice are not a new character");
        let policy = crate::policy::parse("[identity]\nvoice_identity_weight = 1\n", "p").unwrap();
        assert_eq!(decide(&s, &policy).kind, Kind::New, "a policy that counts a voice as a whole signal");
    }

    #[test]
    fn the_signal_requirement_moves_with_the_policy() {
        let s = Signals { model_replaced: true, geometry: geometry(10, 0, 100, 0), portrait: (6, 6), ..Default::default() };
        assert_eq!(decide(&s, &p()).kind, Kind::New);
        let strict = crate::policy::parse("[identity]\nnew_character_signals = 3\n", "p").unwrap();
        assert_eq!(decide(&s, &strict).kind, Kind::Same);
        // the head line: 40% of our head on Valve's is a new head by default, Valve's at a 30% line
        let s = Signals { model_replaced: true, geometry: geometry(100, 40, 1000, 100), ..Default::default() };
        assert!(decide(&s, &p()).signals.contains(&"head_replaced"));
        let lenient = crate::policy::parse("[identity]\nhead_kept_share = 0.3\n", "p").unwrap();
        assert!(!decide(&s, &lenient).signals.contains(&"head_replaced"));
    }

    #[test]
    fn valve_s_mesh_kept_is_a_variant_and_no_model_is_a_kit() {
        let s = Signals { model_replaced: true, geometry: geometry(3000, 3000, 100000, 99500), ..Default::default() };
        assert_eq!(decide(&s, &p()).kind, Kind::SameModel);
        assert_eq!(decide(&Signals { materials_replaced: true, ..Default::default() }, &p()).kind, Kind::SameModel);
        assert_eq!(decide(&Signals { touches_hero: true, ..Default::default() }, &p()).kind, Kind::NoModel);
        let none = decide(&Signals { other_kind: "HUD mod".into(), ..Default::default() }, &p());
        assert_eq!((none.kind, none.not_graded.as_deref(), none.kind.archetype()), (Kind::NotHero, Some("HUD mod"), None));
        // a new name and new art without a model are no new character: there is no new body
        let kit = Signals { touches_hero: true, renamed: vec!["x".into()], portrait: (6, 6), ..Default::default() };
        assert_eq!(decide(&kit, &p()).kind, Kind::NoModel);
    }

    #[test]
    fn an_unmeasured_head_is_said_and_counts_for_nothing() {
        let s = Signals { model_replaced: true, geometry: Some(Err("our model: no CTRL block".into())), ..Default::default() };
        let id = decide(&s, &p());
        assert!(!id.signals.contains(&"head_replaced"));
        assert!(id.evidence[0].1.starts_with("not measured: our model"), "{:?}", id.evidence);
    }

    #[test]
    fn what_an_unrelated_mod_is() {
        let v = |p: &[&str]| p.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(other_kind(v(&["panorama/layout/hud.vxml_c", "panorama/styles/hud.vcss_c"]).iter()), "HUD mod");
        assert_eq!(other_kind(v(&["maps/dl_midtown.vpk"]).iter()), "map mod");
        assert_eq!(other_kind(v(&["sounds/music/menu.vsnd_c"]).iter()), "music mod");
        assert_eq!(other_kind(v(&["models/props/crate.vmdl_c"]).iter()), "not a hero mod");
    }
}
