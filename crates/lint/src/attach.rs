//! Who outside a model names each attachment, and M062's judgement of an attachment a mod's model lost.
//!
//! An attachment is a named point on a model that something places by name: an effect spawned on it, the health bar,
//! a camera pose, the muzzle flash. Lose it and what is placed by it has no point to find. Whether that shows depends
//! on whether anything the game runs for that model looks the name up, so a lost attachment is judged by who names it,
//! as a lost bone is (`carry`).
//!
//! What the index found on 6880 (`magician index` writes `attachment_users.tsv`):
//!
//! - vdata names attachments in its abilities, items and modifiers: where an effect is attached
//!   (`m_strAttachmentName` beside `m_eAttachmentType`, `ability_apply` most often), where an ability casts from
//!   (`m_strCustomAttachmentSource`), a weapon's or a hero's effect point (`m_strEffectsAttachName`), the health bar
//!   (`m_sHealthBarAttachment`), a shot's origin (`m_strShootAttachment`);
//! - particles name attachments almost only in `m_controlPointConfigurations`, the particle editor's preview of where
//!   the effect sits (the configuration Valve names `preview`); the game attaches an effect where vdata or the code
//!   says. A particle function that reads model particles (`C_OP_SetControlPointsToModelParticles`) names the
//!   attachments of the model the particle draws, not the hero's;
//! - anim graphs and clips name no attachment under a key that says so;
//! - the model's own game data names its health bar's attachment, its muzzles, its zipline effect point and its camera
//!   poses;
//! - the client code holds many of the names Valve's hero models carry.

use crate::carry::NOTHING_CHANGES;
use resource::kv3::Value;
use std::collections::{BTreeSet, HashMap};

/// The attachment users file's format; one written in another is not read.
pub const ATTACHMENT_USERS_REV: u32 = 1;

/// A particle function that names the attachments of the models its particles draw, not of the unit it is on.
const MODEL_PARTICLE_FUNCTIONS: &[&str] = &["C_OP_SetControlPointsToModelParticles"];

/// One use of an attachment name outside a model: the kind of file (`vdata`, `particle`, `preview`, `graph`, `clip`,
/// `code`), the key, where (a path, `path:entry` for a vdata entry, the library for the code) and who the game runs it
/// for (owners from the users index; `*` for the code, empty when no one in a match runs it).
#[derive(Debug, Clone)]
pub struct Use {
    pub kind: String,
    pub key: String,
    pub at: String,
    pub owners: Vec<String>,
}

/// The attachment users of a build, by lowercase name.
#[derive(Debug, Clone, Default)]
pub struct AttachmentUsers {
    pub by_name: HashMap<String, Vec<Use>>,
    /// Whether the client library was read when the file was written (`code=yes`).
    pub code_read: bool,
}

impl AttachmentUsers {
    /// Read `attachment_users.tsv`: `None` when it is not one, or is in another format.
    pub fn parse(text: &str) -> Option<AttachmentUsers> {
        let head = text.lines().next()?;
        if !head.starts_with("# magician attachment users ") || !head.split_whitespace().any(|w| w == format!("rev={ATTACHMENT_USERS_REV}")) {
            return None;
        }
        let mut u = AttachmentUsers { code_read: head.split_whitespace().any(|w| w == "code=yes"), ..Default::default() };
        for l in text.lines().skip(1) {
            if let [name, kind, key, at, owners] = l.split('\t').collect::<Vec<_>>()[..] {
                let owners = if owners == "-" { Vec::new() } else { owners.split(',').map(str::to_string).collect() };
                u.by_name.entry(name.to_string()).or_default().push(Use { kind: kind.into(), key: key.into(), at: at.into(), owners });
            }
        }
        Some(u)
    }

    pub fn of(&self, name: &str) -> &[Use] {
        self.by_name.get(&name.to_ascii_lowercase()).map(Vec::as_slice).unwrap_or(&[])
    }
}

/// Every attachment name a value names: the string values under a key that says attach (`m_strAttachmentName`,
/// `m_strEffectsAttachName`, `m_sHealthBarAttachment`, a control point's `m_attachmentName`, a camera pose's
/// `attachment_name`), as `(name, key, preview)`. The key is written `_class/key` inside a particle function, so a
/// function that reads its own render model's attachments can be told apart. `preview` marks a name under a
/// particle's `m_controlPointConfigurations`. Enum values (`PATTACH_POINT_FOLLOW`, `EAttachmentSource_Custom`), paths
/// and sound names are not attachment names.
pub fn attachment_names(v: &Value) -> BTreeSet<(String, String, bool)> {
    fn go(key: &str, class: Option<&str>, preview: bool, v: &Value, out: &mut BTreeSet<(String, String, bool)>) {
        let k = key.to_ascii_lowercase();
        match v {
            Value::Str(s)
                if k.contains("attach")
                    && !k.contains("particle")
                    && !k.contains("sound")
                    && !k.contains("type")
                    && !s.is_empty()
                    && !s.starts_with("PATTACH_")
                    && !s.starts_with("EAttachment")
                    && !s.contains(['/', ' ', '.', '|', ':']) =>
            {
                let key = class.map_or_else(|| key.to_string(), |c| format!("{c}/{key}"));
                out.insert((s.clone(), key, preview));
            }
            Value::Array(a) => a.iter().for_each(|x| go(key, class, preview, x, out)),
            Value::Object(m) => {
                let class = m.iter().find(|(k, _)| k == "_class").and_then(|(_, c)| c.as_str()).or(class);
                let preview = preview || k == "m_controlpointconfigurations";
                m.iter().for_each(|(k, x)| go(k, class, preview, x, out));
            }
            _ => {}
        }
    }
    let mut out = BTreeSet::new();
    go("", None, false, v, &mut out);
    out
}

/// Whether an owner is something any hero can carry: an item (`upgrade_`) or a cosmetic ability (`cosmetic_`). Every
/// other owner is one unit's: a hero's abilities, an NPC's, a boss's (`citadel_ability_tier2boss_laser_beam` names the
/// bull's eyes), a neutral's modifiers.
fn any_hero(o: &str) -> bool {
    o.starts_with("upgrade_") || o.starts_with("cosmetic_")
}

/// Whether the game runs a use on this model: a graph or clip its own graphs play (`played`), or vdata of the unit
/// itself (its own abilities, its hero entry) or of what any hero carries. Another unit's own data is run for that
/// unit, and a clip the model's graphs do not reach is played on another model.
fn runs_for(u: &Use, model: &[String], played: &BTreeSet<String>) -> bool {
    if u.kind == "clip" || u.kind == "graph" {
        return played.contains(&u.at);
    }
    u.owners.iter().any(|o| o == "*" || model.contains(o) || any_hero(o))
}

/// Whether a use places something on the unit's own model at run time: not a particle editor preview, and not a
/// particle function that reads the models its particles draw.
fn at_run_time(u: &Use) -> bool {
    u.kind != "preview" && !MODEL_PARTICLE_FUNCTIONS.iter().any(|f| u.key.starts_with(&format!("{f}/")))
}

/// Whether a use is the client code's, of a name every playable Valve hero's own model carries (`heroes=42/42`). The
/// code holds the names of 65 attachments of Valve's hero models (6880); 17 of them are on all 42 playable heroes (`aim`,
/// `healthbar`, `standing_pivot`, `ability_apply`, `palm_r`, `zipline` and the rest), and most others on one to three
/// (`attach_hitloc` on 8): Valve's own heroes lack those and play, so the code copes without them, and the code
/// alone does not make one an error.
fn every_hero(u: &Use) -> bool {
    u.kind == "code"
        && u.key.strip_prefix("heroes=").and_then(|r| r.split_once('/')).is_some_and(|(n, m)| n == m && m.parse::<usize>().is_ok_and(|m| m > 0))
}

/// `a`, `a and b`, `a, b and c`.
fn list(v: &[String]) -> String {
    match v.len() {
        0 => String::new(),
        1 => v[0].clone(),
        n => format!("{} and {}", v[..n - 1].join(", "), v[n - 1]),
    }
}

/// Up to three examples of a set of uses, then how many more.
fn examples(uses: &[&Use]) -> String {
    let mut shown: Vec<String> = uses.iter().map(|u| format!("{} {}", u.at, u.key.rsplit('/').next().unwrap_or(&u.key))).collect();
    shown.sort();
    shown.dedup();
    let n = shown.len();
    let mut s = shown.into_iter().take(3).collect::<Vec<_>>().join("; ");
    if n > 3 {
        s.push_str(&format!("; and {} more", n - 3));
    }
    s
}

/// M062's lost attachments judged by who names them: `ours` and `valve` are the two models, `model_owners` who the game
/// runs the model for (the users index's users of its path), `played` the graphs and clips its own graphs reach
/// (`carry::played`). A lost attachment is an error when something the game runs for this model names it, and the
/// message says what and where: the model's own game data (its health bar, muzzles, zipline effect point, camera
/// poses), the vdata of this unit's abilities or of what any hero carries (items, cosmetic abilities), an animation
/// event in a clip its graphs play, or the client code for a name every playable hero carries (what the code looks up
/// by it is UNVERIFIED). A name Valve's game data gives a role (the health bar's attachment, a camera pose) that this
/// model's game data gives no attachment, or another, is not looked up on this model: the game-data finding says what
/// moves. Otherwise it is a warning that says who does name it: a particle editor preview, another unit's own data,
/// animation played on another model, nothing. With no attachment users index the users are unknown, and unknown is
/// not clean: an error that says so. Every other breach is returned as it is.
pub fn judge(breaches: Vec<String>, ours: &[u8], valve: &[u8], model_owners: &[String], played: &BTreeSet<String>, users: Option<&AttachmentUsers>) -> Vec<String> {
    let lost = |m: &str| m.strip_prefix("attachment '").and_then(|r| r.strip_suffix("' missing")).map(str::to_string);
    if !breaches.iter().any(|m| lost(m).is_some()) {
        return breaches;
    }
    // what the model's own game data names: the game reads it from this model, whatever Valve's said
    let names = |m: &[u8]| -> BTreeSet<(String, String)> { crate::gamedata::read(m).map(|gd| attachment_names(&gd).into_iter().map(|(n, k, _)| (n.to_ascii_lowercase(), k)).collect()).unwrap_or_default() };
    let (own, valves) = (names(ours), names(valve));
    breaches
        .into_iter()
        .map(|msg| {
            let Some(name) = lost(&msg) else { return msg };
            let lower = name.to_ascii_lowercase();
            let mut who: Vec<String> = Vec::new();
            let mine: Vec<String> = own.iter().filter(|(n, _)| *n == lower).map(|(_, k)| k.clone()).collect();
            if !mine.is_empty() {
                who.push(format!("this model's own game data ({})", list(&mine)));
            }
            let Some(users) = users else {
                let also = if who.is_empty() { String::new() } else { format!("; {} names it", who[0]) };
                return format!("{msg}: who else names it could not be checked: there is no attachment users index for this build (`magician index` builds it){also}");
            };
            let uses = users.of(&name);
            let run: Vec<&Use> = uses.iter().filter(|u| u.kind != "code" && at_run_time(u) && runs_for(u, model_owners, played)).collect();
            let mine = |u: &Use| u.kind == "clip" || u.kind == "graph" || u.owners.iter().any(|o| model_owners.contains(o));
            let own_data: Vec<&Use> = run.iter().copied().filter(|u| mine(u)).collect();
            let shared: Vec<&Use> = run.iter().copied().filter(|u| !mine(u)).collect();
            if !own_data.is_empty() {
                who.push(format!("this unit's own vdata and effects ({})", examples(&own_data)));
            }
            if !shared.is_empty() {
                who.push(format!("what any hero can carry, items among them ({})", examples(&shared)));
            }
            // a role Valve's game data gives this name, which this model's game data gives to no attachment or another:
            // the game takes the role from this model's data, so it does not look this name up here
            let roles: Vec<String> = valves.iter().filter(|(n, _)| *n == lower).map(|(_, k)| k.clone()).collect();
            if who.is_empty() && !roles.is_empty() {
                let mut keys = roles.clone();
                keys.dedup();
                let code = if uses.iter().any(|u| u.kind == "code") { "; the client code holds the name too (what it looks up by it is UNVERIFIED)" } else { "" };
                return format!(
                    "{msg}: Valve's model's game data names it ({}), and this model's game data names no attachment of that name, so the game takes that place from this model's own data and does not look the name up here (the game-data finding says what moves){code}; {NOTHING_CHANGES}",
                    list(&keys)
                );
            }
            if uses.iter().any(every_hero) {
                who.push("the game's client code (it holds the name, and every playable hero's model carries it; whether the code looks it up on this model is UNVERIFIED)".into());
            }
            if who.is_empty() {
                let previews: Vec<&Use> = uses.iter().filter(|u| u.kind == "preview").collect();
                let others: Vec<&Use> = uses.iter().filter(|u| u.kind == "vdata" && !u.owners.is_empty()).collect();
                let elsewhere: Vec<&Use> = uses.iter().filter(|u| u.kind == "clip" || u.kind == "graph").collect();
                let models: Vec<&Use> = uses.iter().filter(|u| !at_run_time(u) && u.kind != "preview").collect();
                let mut said = Vec::new();
                if !previews.is_empty() {
                    said.push(format!("the particle editor's preview of {} effect(s) names it ({}), which places the effect in the editor, not in the game", previews.len(), examples(&previews)));
                }
                if !others.is_empty() {
                    said.push(format!("only other units' own data names it ({})", examples(&others)));
                }
                if !elsewhere.is_empty() {
                    said.push(format!("animation this model's graphs do not play names it ({})", examples(&elsewhere)));
                }
                if !models.is_empty() {
                    said.push(format!("a particle names it on the model the particle draws ({})", examples(&models)));
                }
                let code: Vec<&Use> = uses.iter().filter(|u| u.kind == "code").collect();
                if let Some(c) = code.first() {
                    said.push(format!("the client code holds the name, but only some of Valve's heroes carry it ({}), and those without it play", c.key.trim_start_matches("heroes=")));
                }
                if !users.code_read {
                    return format!("{msg}: whether the game's client code names it could not be checked: the attachment users index was written without the client library{}", if said.is_empty() { String::new() } else { format!("; {}", said.join("; ")) });
                }
                let said = if said.is_empty() { "nothing in the game names it".to_string() } else { said.join("; ") };
                return format!("{msg}: nothing the game runs for this model looks it up by name, and the model's own game data names it nowhere; {said}; {NOTHING_CHANGES}");
            }
            format!(
                "{msg}, and {} name{} it: what is placed by it has no point to find, so it spawns at the model's origin, the feet, or not at all (UNVERIFIED in game); put the attachment back under Valve's name",
                who.join(", and "),
                if who.len() == 1 { "s" } else { "" }
            )
        })
        .collect()
}
