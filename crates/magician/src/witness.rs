//! `magician witness`: lint's findings held against what the game itself said during an in-game pass.
//!
//! Lint predicts; the game's console log testifies. The engine logs a resource it could not load
//! (`[ResourceSystem] Failed loading resource "..."`), an attachment a model was asked for and does not have
//! (`Unable to lookup attachment "..."`, `Attempted to attach attachment ... to an unknown bone`), and, with
//! `snd_showstart 1`, every sound file it starts (`[SoundSystem] [(cl) N] sounds\...vsnd`). A vanilla run of the same
//! scenarios on the same build says what those scenarios make Valve's files do. Each finding is then:
//!
//! - **confirmed**: the game said it (the resource failed to load; the event Valve's files played in the vanilla run
//!   played nothing with the mod);
//! - **contradicted**: the game did the thing the finding says it cannot (played the file it calls missing);
//! - **not reached**: the pass never asked for it, and the line says what would;
//! - **a frame's**: what it predicts is seen, not logged (geometry, placement), and the line names the probe;
//! - **a file fact**: true or false from the files alone; the game cannot say more (a path's case, a texture's pixels).
//!
//! The game's complaints that no finding predicted are listed too: what lint missed. Every outcome is appended to
//! `<reference>/<build>/witness.tsv`, so a rule's record (confirmed so often, contradicted so often) is kept per build and
//! `magician rules` shows it.

use lint::{Finding, Severity};
use resource::kv3::Value;
use std::collections::{BTreeMap, BTreeSet};

/// What one console log says.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Log {
    /// Resources the game failed to load (game paths, as logged).
    pub failed: BTreeSet<String>,
    /// Sound files started (`sounds/x/y.vsnd_c`).
    pub started: BTreeSet<String>,
    /// Attachments looked up and missing: (attachment, model as logged).
    pub no_attachment: BTreeSet<(String, String)>,
    /// Attachments a model put on a bone it does not have: (model, attachment, bone).
    pub no_bone: BTreeSet<(String, String, String)>,
    /// The camera poses each model's attachments gave the hero camera (`VMDL Camera Pose Success! "model"`), with the
    /// nodes the game found too high or low against the standing pivot and zeroed: model to (node to overshoot).
    pub camera: BTreeMap<String, BTreeMap<String, String>>,
    /// Texture slots a material was asked for and does not set: (slot, material as logged).
    pub no_texture: BTreeSet<(String, String)>,
    /// Camera pivots the game found turned the wrong way (`Camera Pivot found (standing_pivot) ... its orientation is
    /// wrong`), with how many times it said so: the hero camera reads them every frame.
    pub pivot_wrong: BTreeMap<String, usize>,
}

fn quoted_after<'a>(line: &'a str, marker: &str) -> Option<&'a str> {
    let rest = &line[line.find(marker)? + marker.len()..];
    let rest = rest.strip_prefix('"')?;
    Some(&rest[..rest.find('"')?])
}

pub fn parse_log(text: &str) -> Log {
    let mut l = Log::default();
    let mut zeroed: BTreeMap<String, String> = BTreeMap::new();
    for line in text.lines() {
        if let Some(i) = line.find("Camera Pivot found (") {
            let rest = &line[i + "Camera Pivot found (".len()..];
            if line.contains("orientation is wrong") {
                if let Some(j) = rest.find(')') {
                    *l.pivot_wrong.entry(rest[..j].to_string()).or_default() += 1;
                }
            }
        } else if let Some(node) = quoted_after(line, "Camera Pose Node ") {
            let v = line.split("standing pivot (").nth(1).and_then(|r| r.split(')').next()).unwrap_or("").trim();
            zeroed.insert(node.to_string(), v.to_string());
        } else if let Some(m) = quoted_after(line, "VMDL Camera Pose Success! ") {
            l.camera.insert(m.to_ascii_lowercase(), std::mem::take(&mut zeroed));
        } else if let (Some(i), Some(j)) = (line.find("[MaterialSystem] Texture "), line.find(" doesn't exist in ")) {
            let slot = line[i + "[MaterialSystem] Texture ".len()..j.max(i + "[MaterialSystem] Texture ".len())].to_string();
            let mat = line[j + " doesn't exist in ".len()..].trim().trim_end_matches('!').to_ascii_lowercase();
            l.no_texture.insert((slot, mat));
        } else if let Some(p) = quoted_after(line, "Failed loading resource ") {
            l.failed.insert(p.replace('\\', "/").to_ascii_lowercase());
        } else if let Some(i) = line.find("[SoundSystem] [(") {
            let rest = &line[i..];
            if let Some(s) = rest.find("] sounds\\").or_else(|| rest.find("] sounds/")) {
                let p = &rest[s + 2..];
                let p = p.split(" : ").next().unwrap_or(p).trim();
                l.started.insert(format!("{}_c", p.replace('\\', "/").to_ascii_lowercase()));
            }
        } else if let Some(a) = quoted_after(line, "Unable to lookup attachment ") {
            let m = quoted_after(line, " on model ").unwrap_or("");
            l.no_attachment.insert((a.to_string(), m.to_string()));
        } else if let (Some(i), Some(j)) = (line.find("[MeshSystem] Model "), line.find(": Attempted to attach attachment ")) {
            let model = &line[i + "[MeshSystem] Model ".len()..j.max(i + "[MeshSystem] Model ".len())];
            let rest = &line[j + ": Attempted to attach attachment ".len()..];
            if let Some((a, b)) = rest.split_once(" to an unknown bone ") {
                l.no_bone.insert((model.to_string(), a.to_string(), b.trim_end_matches('!').trim().to_string()));
            }
        }
    }
    l
}

/// Two logged numbers within 0.01 of each other (the engine prints float noise: -3.225014 and -3.224983).
fn same_number(a: &str, b: &str) -> bool {
    match (a.trim().parse::<f64>(), b.trim().parse::<f64>()) {
        (Ok(x), Ok(y)) => (x - y).abs() <= 0.01,
        _ => a == b,
    }
}

/// Every event's sound files in a sound-event file: event name to its `.vsnd_c` paths.
pub fn event_files(v: &Value) -> BTreeMap<String, Vec<String>> {
    let mut out = BTreeMap::new();
    for (name, e) in v.members() {
        out.insert(name.clone(), lint::deep::vsnd_refs(e));
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Outcome {
    Confirmed,
    Contradicted,
    NotReached,
    Frame,
    FileFact,
}

impl Outcome {
    pub fn word(self) -> &'static str {
        match self {
            Outcome::Confirmed => "confirmed",
            Outcome::Contradicted => "contradicted",
            Outcome::NotReached => "not reached",
            Outcome::Frame => "a frame's",
            Outcome::FileFact => "a file fact",
        }
    }
}

/// What the passes and the files give the judge.
pub struct Evidence<'a> {
    pub log: &'a Log,
    pub vanilla: Option<&'a Log>,
    /// Sound-event file path to its events' files: ours (the mod's copy) and Valve's.
    pub ours: &'a BTreeMap<String, BTreeMap<String, Vec<String>>>,
    pub valve: &'a BTreeMap<String, BTreeMap<String, Vec<String>>>,
    /// The mod's model files (to tell the model a logged attachment lookup was on).
    pub models: &'a [String],
}

fn quote(m: &str, after: &str) -> Option<String> {
    let rest = &m[m.find(after)? + after.len()..];
    Some(rest[..rest.find('\'')?].to_string())
}

/// Which of the hero's actions plays an event, as a hint for the probe that would reach it.
fn action_of(event: &str) -> &'static str {
    let e = event.to_ascii_lowercase();
    if e.contains("melee") && (e.contains("charged") || e.contains("heavy")) {
        "a heavy melee (+in_weapon1 held a second)"
    } else if e.contains("melee") {
        "a light melee (+in_weapon1 tapped)"
    } else if e.contains("hit") || e.contains("impact") || e.contains("target") || e.contains("explode") {
        "the ability landing on a bot (cast it at one)"
    } else if e.contains("armed") || e.contains("arming") || e.contains("select") {
        "the ability's whole cycle: selected, placed and left to arm"
    } else {
        "the ability or action this event belongs to"
    }
}

/// A file the game started and loaded: `snd_showstart` prints a file it starts even when the load then fails.
fn played(log: &Log, f: &str) -> bool {
    log.started.contains(f) && !log.failed.contains(f)
}

fn started_any(log: &Log, files: &[String]) -> bool {
    files.iter().any(|f| played(log, f))
}

/// One finding judged: its outcome and why, in words.
pub fn judge(f: &Finding, ev: &Evidence) -> (Outcome, String) {
    let path = f.path.clone().unwrap_or_default();
    match f.code {
        "M030" => {
            let Some(t) = quote(&f.message, "needs '") else { return (Outcome::NotReached, "no target named".into()) };
            let t = t.to_ascii_lowercase();
            if ev.log.failed.contains(&t) {
                return (Outcome::Confirmed, format!("the game: Failed loading resource \"{t}\""));
            }
            if played(ev.log, &t) {
                return (Outcome::Contradicted, format!("the game played {t}"));
            }
            if t.ends_with(".vsnd_c") {
                let empty = BTreeMap::new();
                let ours = ev.ours.get(&path).unwrap_or(&empty);
                // the event that names the file (the first, when several do)
                if let Some((e, files)) = ours.iter().find(|(_, fs)| fs.contains(&t)) {
                    if let Some(played) = files.iter().find(|x| played(ev.log, x)) {
                        return (Outcome::Confirmed, format!("{e} fired in this pass ({played} started) and never played this file, which no build ships"));
                    }
                    if let Some(tried) = files.iter().find(|x| ev.log.started.contains(*x)) {
                        return (Outcome::Confirmed, format!("{e} fired in this pass ({tried} started and failed to load) and nothing of it played: silent"));
                    }
                    let valve_files = ev.valve.get(&path).and_then(|m| m.get(e)).cloned().unwrap_or_default();
                    match ev.vanilla {
                        Some(v) if started_any(v, &valve_files) || started_any(v, files) => {
                            return (Outcome::Confirmed, format!("the vanilla run of the same scenarios played {e}; with the mod it played nothing: silent"));
                        }
                        Some(_) => return (Outcome::NotReached, format!("{e} never played, with the mod or without; reached by {}", action_of(e))),
                        None => return (Outcome::NotReached, format!("{e} played nothing; a vanilla run of the same scenarios would say whether it was asked for ({})", action_of(e))),
                    }
                }
            }
            (Outcome::NotReached, format!("nothing in the pass loaded or asked for {t}"))
        }
        "M063" if f.message.contains(": event removed") => {
            let e = f.message.split(": event removed").next().unwrap_or("").to_string();
            let valve_files = ev.valve.get(&path).and_then(|m| m.get(&e)).cloned().unwrap_or_default();
            match ev.vanilla {
                Some(v) if started_any(v, &valve_files) => {
                    if started_any(ev.log, &valve_files) {
                        (Outcome::Contradicted, format!("{e}'s sounds played with the mod too"))
                    } else {
                        (Outcome::Confirmed, format!("the vanilla run played {e}; with the mod it played nothing: the cue is gone"))
                    }
                }
                Some(_) => (Outcome::NotReached, format!("{e} never played in the vanilla run either; reached by {}", action_of(&e))),
                None => (Outcome::NotReached, format!("needs a vanilla run of the same scenarios to see {e} play ({})", action_of(&e))),
            }
        }
        "M063" | "M064" => (Outcome::FileFact, "a level or length: the log says a sound started, not how loud or long; the files are the measure".into()),
        "M062" => {
            if let Some(a) = quote(&f.message, "attachment '") {
                let a = a.split("..").next().unwrap_or(&a).to_string();
                let asked = ev.log.no_attachment.iter().any(|(x, m)| x.starts_with(&a) && (m.is_empty() || ev.models.iter().any(|p| p.trim_end_matches("_c") == m)));
                if asked {
                    return (Outcome::Confirmed, format!("the game: Unable to lookup attachment \"{a}\""));
                }
                if let Some(n) = ev.log.pivot_wrong.get(&a).filter(|_| !ev.vanilla.is_some_and(|v| v.pivot_wrong.contains_key(&a))) {
                    return (Outcome::Confirmed, format!("the game: Camera Pivot found ({a}) ... its orientation is wrong ({n} times): the hero camera is built on it"));
                }
                // a camera pose node (near_, far_, gunaim_): the game reads it as the hero camera's pose and says when
                // one sits off the standing pivot, so a moved node shows as a pose the vanilla run did not zero
                let model = path.trim_end_matches("_c").to_ascii_lowercase();
                if let (Some(m), Some(v)) = (ev.log.camera.get(&model), ev.vanilla.and_then(|v| v.camera.get(&model))) {
                    match (m.get(&a), v.get(&a)) {
                        (Some(x), y) if !y.is_some_and(|y| same_number(x, y)) => {
                            return (Outcome::Confirmed, format!("the game zeroed camera pose \"{a}\" (overshoot {x} against the standing pivot; Valve's model: {})", y.map_or("not zeroed".to_string(), |y| y.clone())));
                        }
                        _ => {}
                    }
                }
                return (Outcome::Frame, format!("the game logged no lookup of '{a}' in this pass; where it shows is seen in a frame (the stock `witness` scenario: a bot of the hero, its overhead bar, abilities cast)"));
            }
            if f.message.starts_with("bone ") {
                return (Outcome::Frame, "a bone: animation in a frame (the stock `motion` scenario)".into());
            }
            (Outcome::Frame, "a hitbox: what bullets hit (shots at a bot)".into())
        }
        "M149" => {
            let slot = quote(&f.message, "texture '").unwrap_or_default();
            let mat = path.trim_end_matches("_c").to_ascii_lowercase();
            if ev.log.no_texture.contains(&(slot.clone(), mat.clone())) {
                (Outcome::Confirmed, format!("the game: Texture {slot} doesn't exist in {mat}, and drew its error texture"))
            } else {
                (Outcome::NotReached, format!("nothing in the pass sampled {slot}: draw the model it is on"))
            }
        }
        "M106" | "M102" | "M110" | "M111" | "M104" | "M107" | "M109" | "M125" | "M145" => {
            (Outcome::Frame, "geometry: seen, not logged (the stock `angles` and `witness` scenarios: the gameplay camera at every pitch, the head close)".into())
        }
        "M124" | "M129" | "M140" | "M130" => (Outcome::Frame, "the silhouette: measured in frames against vanilla (a still of the hero beside a vanilla still from the same place)".into()),
        "M137" | "M136" | "M135" | "M147" | "M138" | "M139" => (Outcome::Frame, "how the surface reads in the game's light (a still in the world)".into()),
        _ => (Outcome::FileFact, "the files settle it; the game's log has nothing to add".into()),
    }
}

/// The game's complaints in the pass that no finding predicted and the vanilla run (when given) does not make: what
/// lint missed, or the vanilla game's own noise when there is no vanilla run to subtract.
pub fn unpredicted(ev: &Evidence, findings: &[Finding]) -> Vec<String> {
    let said: String = findings.iter().map(|f| f.message.to_ascii_lowercase()).collect::<Vec<_>>().join("\n");
    let mut out = Vec::new();
    for p in &ev.log.failed {
        if ev.vanilla.is_some_and(|v| v.failed.contains(p)) || said.contains(p.as_str()) {
            continue;
        }
        out.push(format!("Failed loading resource \"{p}\""));
    }
    for (a, m) in &ev.log.no_attachment {
        if ev.vanilla.is_some_and(|v| v.no_attachment.contains(&(a.clone(), m.clone()))) || said.contains(&format!("'{}'", a.to_ascii_lowercase())) {
            continue;
        }
        out.push(format!("Unable to lookup attachment \"{a}\" on model \"{m}\""));
    }
    for (s, m) in &ev.log.no_texture {
        if ev.vanilla.is_some_and(|v| v.no_texture.contains(&(s.clone(), m.clone()))) || said.contains(&format!("'{}'", s.to_ascii_lowercase())) {
            continue;
        }
        out.push(format!("Texture {s} doesn't exist in {m} (the game drew its error texture)"));
    }
    for (p, n) in &ev.log.pivot_wrong {
        if ev.vanilla.is_some_and(|v| v.pivot_wrong.contains_key(p)) || said.contains(&format!("'{}'", p.to_ascii_lowercase())) {
            continue;
        }
        out.push(format!("Camera Pivot found ({p}) but its orientation is wrong ({n} times)"));
    }
    for (m, a, b) in &ev.log.no_bone {
        if ev.vanilla.is_some_and(|v| v.no_bone.contains(&(m.clone(), a.clone(), b.clone()))) {
            continue;
        }
        out.push(format!("Model {m}: attachment {a} on an unknown bone {b}"));
    }
    out
}

/// Per rule: (rule, severity, counts by outcome).
pub fn tally(judged: &[(Finding, Outcome, String)]) -> Vec<(&'static str, Severity, BTreeMap<Outcome, usize>)> {
    let mut by: BTreeMap<&'static str, (Severity, BTreeMap<Outcome, usize>)> = BTreeMap::new();
    for (f, o, _) in judged {
        let e = by.entry(f.code).or_insert((f.severity, BTreeMap::new()));
        e.0 = e.0.max(f.severity);
        *e.1.entry(*o).or_default() += 1;
    }
    let mut v: Vec<_> = by.into_iter().map(|(c, (s, m))| (c, s, m)).collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    v
}

/// The rows a pass adds to `witness.tsv`: build, rule, outcome, the finding's file and message, the pass.
pub fn ledger_rows(build: &str, pass: &str, judged: &[(Finding, Outcome, String)]) -> String {
    let clean = |s: &str| s.replace(['\t', '\n', '\r'], " ");
    judged
        .iter()
        .map(|(f, o, _)| format!("{build}\t{}\t{}\t{}\t{}\t{}\n", f.code, o.word(), clean(f.path.as_deref().unwrap_or("")), clean(&f.message), clean(pass)))
        .collect()
}

/// A rule's record from `witness.tsv`, for one build: (confirmed, contradicted, not reached, by frame or files).
pub fn record(text: &str, build: &str) -> BTreeMap<String, [usize; 4]> {
    let mut seen = BTreeSet::new();
    let mut out: BTreeMap<String, [usize; 4]> = BTreeMap::new();
    for line in text.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        // the same finding witnessed twice by one pass counts once
        if f.len() < 6 || f[0] != build || !seen.insert(line.to_string()) {
            continue;
        }
        let r = out.entry(f[1].to_string()).or_default();
        match f[2] {
            "confirmed" => r[0] += 1,
            "contradicted" => r[1] += 1,
            "not reached" => r[2] += 1,
            _ => r[3] += 1,
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOG: &str = r#"11/25 14:18:44 [SoundSystem] [(cl) 30] sounds\abilities\example\charge_01.vsnd : src 0 : vol 1.00 : time 65.723
11/25 14:18:45 [ResourceSystem] Failed loading resource "sounds/player/example/swing_long_01.vsnd_c" (ERROR_FILEOPEN: File not found)
11/25 14:18:46 CGameParticleManager::SetParticleControlEnt: Unable to lookup attachment "healthbar" on model "models/heroes/example/example.vmdl" for entity "player"
11/25 14:18:47 [MeshSystem] Model models/npc/x.vmdl: Attempted to attach attachment HandL_IK to an unknown bone L_Hand_IK!
11/25 14:18:48 [MaterialSystem] Texture g_tAltColor_Reveal doesn't exist in models/heroes/example/bow.vmat!
11/25 14:18:49 [Client] Camera Pivot found (standing_pivot), and loaded, but seems as though its orientation is wrong? Missing "root motion" bone?
 Camera Pivot found (standing_pivot), and loaded, but seems as though its orientation is wrong? Missing "root motion" bone?
"#;

    fn f(code: &'static str, path: &str, msg: &str) -> Finding {
        let r = lint::rule(code);
        Finding { code: r.code, name: r.name, severity: r.severity, path: Some(path.into()), message: msg.into() }
    }

    #[test]
    fn the_log_is_read_for_what_the_engine_says() {
        let l = parse_log(LOG);
        assert!(l.started.contains("sounds/abilities/example/charge_01.vsnd_c"));
        assert!(l.failed.contains("sounds/player/example/swing_long_01.vsnd_c"));
        assert!(l.no_attachment.contains(&("healthbar".into(), "models/heroes/example/example.vmdl".into())));
        assert!(l.no_bone.contains(&("models/npc/x.vmdl".into(), "HandL_IK".into(), "L_Hand_IK".into())));
        assert!(l.no_texture.contains(&("g_tAltColor_Reveal".into(), "models/heroes/example/bow.vmat".into())));
        assert_eq!(l.pivot_wrong.get("standing_pivot"), Some(&2));
    }

    #[test]
    fn findings_are_confirmed_contradicted_or_not_reached_by_what_the_game_did() {
        let evts = "soundevents/hero/example.vsndevts_c";
        let mut ours = BTreeMap::new();
        ours.insert(evts.to_string(), BTreeMap::from([
            ("Ex.Swing.Charged".to_string(), vec!["sounds/player/example/swing_gone_01.vsnd_c".to_string()]),
            ("Ex.Charge".to_string(), vec!["sounds/abilities/example/charge_01.vsnd_c".to_string(), "sounds/abilities/example/charge_02.vsnd_c".to_string()]),
            ("Ex.Hit".to_string(), vec!["sounds/abilities/example/hit_gone.vsnd_c".to_string()]),
        ]));
        let mut valve = BTreeMap::new();
        valve.insert(evts.to_string(), BTreeMap::from([
            ("Ex.Swing.Charged".to_string(), vec!["sounds/player/example/swing_charged_01.vsnd_c".to_string()]),
            ("Ex.Armed".to_string(), vec!["sounds/abilities/example/armed.vsnd_c".to_string()]),
            ("Ex.Hit".to_string(), vec!["sounds/abilities/example/hit.vsnd_c".to_string()]),
        ]));
        let log = parse_log(LOG);
        let vanilla = parse_log("[SoundSystem] [(cl) 1] sounds\\player\\example\\swing_charged_01.vsnd : src 0\n[SoundSystem] [(cl) 2] sounds\\abilities\\example\\armed.vsnd : src 0\n");
        let models = vec!["models/heroes/example/example.vmdl_c".to_string()];
        let ev = Evidence { log: &log, vanilla: Some(&vanilla), ours: &ours, valve: &valve, models: &models };
        let o = |x: &Finding| judge(x, &ev).0;
        assert_eq!(o(&f("M030", evts, "needs 'sounds/player/example/swing_long_01.vsnd_c'")), Outcome::Confirmed, "the engine said it failed");
        assert_eq!(o(&f("M030", evts, "needs 'sounds/player/example/swing_gone_01.vsnd_c'")), Outcome::Confirmed, "vanilla played the event, the mod nothing");
        assert_eq!(o(&f("M030", evts, "needs 'sounds/abilities/example/charge_01.vsnd_c'")), Outcome::Contradicted, "the game played it");
        assert_eq!(o(&f("M030", evts, "needs 'sounds/abilities/example/charge_02.vsnd_c'")), Outcome::Confirmed, "its event played other takes");
        let (out, why) = judge(&f("M030", evts, "needs 'sounds/abilities/example/hit_gone.vsnd_c'"), &ev);
        assert_eq!(out, Outcome::NotReached);
        assert!(why.contains("landing on a bot"), "{why}");
        assert_eq!(o(&f("M063", evts, "Ex.Armed: event removed")), Outcome::Confirmed);
        assert_eq!(o(&f("M063", evts, "Ex.Gone: event removed")), Outcome::NotReached);
        assert_eq!(o(&f("M062", "models/heroes/example/example.vmdl_c", "attachment 'healthbar' missing")), Outcome::Confirmed);
        assert_eq!(o(&f("M062", "models/heroes/example/example.vmdl_c", "attachment 'hero_attach_0..4' missing (5 findings)")), Outcome::Frame);
        assert_eq!(o(&f("M106", "models/heroes/example/example.vmdl_c", "cut-body opening")), Outcome::Frame);
        assert_eq!(o(&f("M001", "README.txt", "rename")), Outcome::FileFact);
        assert_eq!(o(&f("M149", "models/heroes/example/bow.vmat_c", "texture 'g_tAltColor_Reveal' dropped")), Outcome::Confirmed);
        assert_eq!(o(&f("M062", "models/heroes/example/example.vmdl_c", "attachment 'standing_pivot' changed: on root_motion at 0,82.8,0")), Outcome::Confirmed, "the game said the pivot is turned");
        assert_eq!(o(&f("M149", "models/heroes/example/bow.vmat_c", "texture 'g_tOther' dropped")), Outcome::NotReached);
        // without a vanilla run an event that played nothing is not reached, never confirmed
        let ev2 = Evidence { vanilla: None, ..ev };
        assert_eq!(judge(&f("M030", evts, "needs 'sounds/player/example/swing_gone_01.vsnd_c'"), &ev2).0, Outcome::NotReached);
        // what the game complained of that no finding said
        let miss = unpredicted(&ev2, &[f("M062", "x", "attachment 'healthbar' missing")]);
        assert!(miss.iter().any(|m| m.contains("swing_long_01")) && miss.iter().any(|m| m.contains("HandL_IK")), "{miss:?}");
        assert!(!miss.iter().any(|m| m.contains("healthbar")), "{miss:?}");
        assert!(unpredicted(&Evidence { vanilla: Some(&log), ..ev2 }, &[]).is_empty(), "vanilla says the same: the game's own");
    }

    #[test]
    fn a_camera_pose_the_game_zeroed_where_vanilla_did_not_confirms_a_moved_attachment() {
        let cam = |v: &str| format!("[Client] Camera Pose Node \"near_00\" has a noticeable height overshoot vs the standing pivot ( {v} ), which can result in bad game feel. Zeroing.\n VMDL Camera Pose Success! \"models/heroes/example/example.vmdl\" has loaded camera poses\n");
        let log = parse_log(&cam("-60.2"));
        assert_eq!(log.camera["models/heroes/example/example.vmdl"]["near_00"], "-60.2");
        let vanilla = parse_log(" VMDL Camera Pose Success! \"models/heroes/example/example.vmdl\" has loaded camera poses\n");
        let none = BTreeMap::new();
        let ev = Evidence { log: &log, vanilla: Some(&vanilla), ours: &none, valve: &none, models: &[] };
        let (o, why) = judge(&f("M062", "models/heroes/example/example.vmdl_c", "attachment 'near_00' changed: on root_motion at 1,2,3"), &ev);
        assert_eq!(o, Outcome::Confirmed, "{why}");
        let same = parse_log(&cam("-60.2"));
        let ev = Evidence { vanilla: Some(&same), ..ev };
        assert_eq!(judge(&f("M062", "models/heroes/example/example.vmdl_c", "attachment 'near_00' changed"), &ev).0, Outcome::Frame, "vanilla zeroed it the same: Valve's own");
    }

    #[test]
    fn a_file_started_and_then_failed_to_load_did_not_play() {
        // seen on 6868: snd_showstart prints a missing take, then the resource system says it failed
        let log = parse_log("[SoundSystem] [(cl) 105] sounds\\player\\x\\gone_24.vsnd : src 0\n[ResourceSystem] Failed loading resource \"sounds/player/x/gone_24.vsnd_c\" (ERROR_FILEOPEN: File not found)\n");
        let evts = "soundevents/hero/example.vsndevts_c";
        let ours = BTreeMap::from([(evts.to_string(), BTreeMap::from([("Ex.Swing".to_string(), vec!["sounds/player/x/gone_24.vsnd_c".to_string(), "sounds/player/x/gone_25.vsnd_c".to_string()])]))]);
        let none = BTreeMap::new();
        let ev = Evidence { log: &log, vanilla: None, ours: &ours, valve: &none, models: &[] };
        assert_eq!(judge(&f("M030", evts, "needs 'sounds/player/x/gone_24.vsnd_c'"), &ev).0, Outcome::Confirmed);
        let (o, why) = judge(&f("M030", evts, "needs 'sounds/player/x/gone_25.vsnd_c'"), &ev);
        assert_eq!(o, Outcome::Confirmed, "the event fired, its sibling failed to load, nothing of it played: {why}");
    }

    #[test]
    fn the_record_counts_each_outcome_once_per_build() {
        let j = vec![(f("M030", "a", "needs 'x'"), Outcome::Confirmed, String::new()), (f("M030", "a", "needs 'y'"), Outcome::NotReached, String::new())];
        let rows = ledger_rows("6868", "pass1", &j);
        let text = format!("{rows}{rows}6867\tM030\tcontradicted\ta\tm\tp\n");
        assert_eq!(record(&text, "6868").get("M030"), Some(&[1, 0, 1, 0]));
        assert_eq!(record(&text, "6867").get("M030"), Some(&[0, 1, 0, 0]));
        let t = tally(&j);
        assert_eq!(t[0].0, "M030");
        assert_eq!(t[0].2.get(&Outcome::Confirmed), Some(&1));
    }
}
