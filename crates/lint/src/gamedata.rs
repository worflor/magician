//! The hero model's game data, for M062: what a model document tells the game beyond its bones, attachments and
//! hitboxes, compared with Valve's model at the same path.
//!
//! A compiled model carries three things here that Deadlock's own code reads:
//!
//! - its animation skeleton and AnimGraph 2 references (`m_vecNmSkeletonRefs`, `m_animGraph2Refs` in DATA). Without
//!   them, or with others, the hero stands in his bind pose: the A-pose of the community's model pages.
//! - its game data (`m_modelInfo.m_keyValueText`, KV3 text: ModelDoc's GameDataList). Deadlock's blocks are its own
//!   classes: `CitadelUnitStatusSettings_t` (where the health bar, status and damage numbers sit),
//!   `TaggedSoundSettings_t` (the tagged-sound sets the animations play: footsteps, effort, ability sounds),
//!   `CitadelCameraSettings_t` (the third-person camera's offsets), `AttachmentCameraData` (which attachments are
//!   camera poses), and more. A CSDK 12 compile writes the unit-status block as an array where the game reads one
//!   object: the health bar at the feet.
//! - its camera attachments, the ones the game names: listed in `AttachmentCameraData`, or held in the hero's own
//!   frame and named by the game's client code (`standing_pivot`, `crouching_pivot`). Those are held to Valve's numbers
//!   within `mesh::rules::CAMERA_ATTACH_STEP`, far tighter than the two decimals the contract prints.
//!
//! Valve's own model against itself has nothing to report: every comparison is with Valve's file at the same path.

use crate::deep::Contract;
use resource::kv3::Value;
use std::collections::{BTreeMap, HashSet};

/// The marker that ends an M062 game-data finding that is a warning: the model still loads and plays, the difference
/// shows (`deep::contract_severity`).
pub const STILL_PLAYS: &str = "the model still loads and plays";

/// A model's game data: the KV3 text in `m_modelInfo.m_keyValueText`. `None` when the model has none or it does not
/// parse.
pub fn read(model: &[u8]) -> Option<Value> {
    let d = resource::kv3_block(model, b"DATA")?.ok()?;
    let text = d.at("m_modelInfo.m_keyValueText")?.as_str()?;
    if text.trim().is_empty() {
        return None;
    }
    resource::kv3::parse_text(text).ok()
}

/// The attachments a model's game data lists as camera poses (`AttachmentCameraData`).
pub fn camera_names(gd: &Value) -> Vec<String> {
    gd.get("AttachmentCameraData").map(|a| a.as_array().iter().filter_map(|c| c.get("attachment_name").and_then(Value::as_str)).filter(|s| !s.is_empty()).map(String::from).collect()).unwrap_or_default()
}

/// The animation skeleton and AnimGraph 2 references of a model: skeleton paths, and (identifier, graph) pairs.
pub fn anim_refs(model: &[u8]) -> (Vec<String>, Vec<(String, String)>) {
    let Some(Ok(d)) = resource::kv3_block(model, b"DATA") else { return (Vec::new(), Vec::new()) };
    let skel = d.get("m_vecNmSkeletonRefs").map(|a| a.as_array().iter().filter_map(Value::as_str).filter(|s| !s.is_empty()).map(String::from).collect()).unwrap_or_default();
    let graphs = d
        .get("m_animGraph2Refs")
        .map(|a| {
            a.as_array()
                .iter()
                .map(|g| (g.get("m_sIdentifier").and_then(Value::as_str).unwrap_or("").to_string(), g.get("m_hGraph").and_then(Value::as_str).unwrap_or("").to_string()))
                .filter(|(_, g)| !g.is_empty())
                .collect()
        })
        .unwrap_or_default();
    (skel, graphs)
}

/// What each block of Deadlock's game data does, as a player meets it when the block is gone.
fn what_it_does(block: &str) -> &'static str {
    match block {
        "CCitadelHeroModelGameData_t" => "the hero's turn, look-at, step and zipline-effect settings",
        "CitadelModelSceneData_t" => "the hero page's scene settings (how the cloth settles as the hero is revealed)",
        "CitadelModelParticleSettings_t" => "the scale the effects attached to it play at",
        "CitadelMuzzleSettings_t" => "which attachments the gun's muzzle flashes play from",
        "CitadelEventIDToBodyGroupMapping_t" => "which body groups its abilities switch",
        "CitadelModelDamageNumberSettings_t" => "where its damage numbers show",
        "CPhysicsBodyGameMarkupData" => "the physics markup of its ragdoll bodies",
        "BoneConstraintList" => "the constraints that drive its helper bones (twist and corrective bones stay at bind)",
        "particle_cfg_list" => "the effects the model plays on itself",
        "cloth_ground_plane" => "the ground plane its cloth rests on",
        "AttachmentCameraData" => "which of its attachments are camera poses, and their fields of view",
        "break_list" | "break_command_list" => "the pieces it breaks into",
        _ => "settings the game reads from the model",
    }
}

fn nums_differ(a: &Value, b: &Value) -> bool {
    let (x, y) = (a.numbers(), b.numbers());
    match (a.as_f64(), b.as_f64()) {
        (Some(p), Some(q)) => (p - q).abs() > crate::mesh::rules::GAMEDATA_STEP,
        _ if !x.is_empty() || !y.is_empty() => x.len() != y.len() || x.iter().zip(&y).any(|(p, q)| (p - q).abs() > crate::mesh::rules::GAMEDATA_STEP),
        _ => a.to_text() != b.to_text(),
    }
}

fn show(v: Option<&Value>) -> String {
    match v {
        None => "none".into(),
        Some(Value::Str(s)) => format!("'{s}'"),
        Some(v) if v.as_f64().is_some() => format!("{}", v.as_f64().unwrap_or(0.0)),
        Some(v) if !v.numbers().is_empty() => format!("[{}]", v.numbers().iter().map(|x| format!("{x}")).collect::<Vec<_>>().join(", ")),
        Some(v) => v.to_text().trim().to_string(),
    }
}

/// The tagged-sound sets a `TaggedSoundSettings_t` block names, lowercased.
fn tagged_sounds(b: &Value) -> Vec<String> {
    b.get("m_taggedSounds").map(|a| a.as_array().iter().filter_map(Value::as_str).map(str::to_ascii_lowercase).collect()).unwrap_or_default()
}

/// M062 findings for a model's animation references and game data against Valve's model at the same path. `ours_has`
/// says whether our model has an attachment of a name.
pub fn judge(ours: &[u8], valve: &[u8], ours_has: &dyn Fn(&str) -> bool) -> Vec<String> {
    let mut out = Vec::new();
    // the animation skeleton and graphs
    let ((os, og), (vs, vg)) = (anim_refs(ours), anim_refs(valve));
    if let Some(v) = vs.first() {
        let lower = |x: &[String]| x.iter().map(|s| s.to_ascii_lowercase()).collect::<Vec<_>>();
        if os.is_empty() {
            out.push(format!("animation skeleton: none, Valve's model names '{v}' (m_vecNmSkeletonRefs): its clips have no skeleton to play on, so the model stands in its bind pose (the A-pose); compile the final model with its AnimGraph 2 references, which write it (a known kit cause: CSDK 12 cannot compile AnimGraph 2 references into a model, the community pages say; CSDK 13 can)"));
        } else if lower(&os) != lower(&vs) {
            out.push(format!("animation skeleton: '{}', Valve's model names '{}': its clips are posed on another skeleton than the one its graphs were made for, and it can stand in its bind pose or twist (the A-pose); name Valve's skeleton", os.join("', '"), vs.join("', '")));
        }
    }
    for (id, g) in &vg {
        if !og.iter().any(|(_, o)| o.eq_ignore_ascii_case(g)) {
            let what = if id.eq_ignore_ascii_case("ui") { "the hero page and the shop show it unanimated, in its bind pose" } else { "it does not animate in a match: the A-pose, and it does not turn" };
            let named = if id.is_empty() { String::new() } else { format!(" (identifier '{id}')") };
            out.push(format!("anim graph '{g}'{named} missing from m_animGraph2Refs, which Valve's model names: {what}; compile the final model with Valve's AnimGraph 2 references (a known kit cause: CSDK 12 cannot compile them into a model, the community pages say; CSDK 13 can)"));
        }
    }

    // the game data blocks
    let Some(vgd) = read(valve) else { return out };
    let ogd = read(ours).unwrap_or(Value::Object(Vec::new()));
    for (block, vb) in vgd.members() {
        let ob = ogd.get(block);
        match block.as_str() {
            "CitadelUnitStatusSettings_t" => match ob {
                None => out.push(format!("game data: {block} missing, which Valve's model has: the game has no place for the unit's health bar, status and damage numbers and draws them at its feet; copy Valve's block into the model's GameDataList")),
                Some(Value::Array(_)) => out.push(format!("game data: {block} is a list where the game reads one block (what a CSDK 12 compile writes): the game does not read it, and the health bar, status and damage numbers sit at the unit's feet; write it as one block, as Valve's model has it")),
                Some(ob) => {
                    let key = "m_strUnitStatusAttachmentName";
                    let (oa, va) = (ob.get(key).and_then(Value::as_str).unwrap_or(""), vb.get(key).and_then(Value::as_str).unwrap_or(""));
                    // an attachment of Valve's own name that the model lost is the contract's own finding (a missing
                    // attachment); one the mod renamed to a name it does not have is this one's
                    if !oa.is_empty() && oa != va && !ours_has(oa) {
                        out.push(format!("game data: the health bar is placed at the attachment '{oa}', which the model does not have (Valve's names '{va}'): the game places the bar at the model's origin, its feet; add the attachment or name Valve's"));
                    } else {
                        let mut moved: Vec<String> = Vec::new();
                        if oa != va {
                            let theirs = if va.is_empty() { "none".to_string() } else { format!("'{va}'") };
                            let ours_s = if oa.is_empty() { "none, so the bar is placed from the model's origin and does not follow the unit when it crouches or rides a zipline".to_string() } else { format!("'{oa}'") };
                            moved.push(format!("its attachment {ours_s} (Valve's {theirs})"));
                        }
                        for k in ["m_vUnitStatusOffset", "m_vHealthbarOffset", "m_vDamageNumbersOffset", "m_vStatusEffectsOffset"] {
                            match (ob.get(k), vb.get(k)) {
                                (Some(a), Some(b)) if nums_differ(a, b) => moved.push(format!("{k} {} (Valve's {})", show(Some(a)), show(Some(b)))),
                                (None, Some(b)) => moved.push(format!("{k} none (Valve's {})", show(Some(b)))),
                                _ => {}
                            }
                        }
                        if !moved.is_empty() {
                            out.push(format!("game data: the health bar is placed otherwise than Valve's: {}: players see the bar, status or damage numbers somewhere else over the unit (an older build's game data, or a compile that dropped Valve's); copy Valve's block unless the body needs its own; {STILL_PLAYS}", moved.join(", ")));
                        }
                    }
                }
            },
            "TaggedSoundSettings_t" => {
                let want = tagged_sounds(vb);
                let have = ob.map(tagged_sounds).unwrap_or_default();
                let lost: Vec<&String> = want.iter().filter(|w| !have.contains(w)).collect();
                // footsteps are a cue enemies listen for: losing them is fair play (`rubric::is_fair_play`)
                if ob.is_none() {
                    out.push(format!("tagged sounds: {block} missing, which Valve's model has ({}): the footsteps, landings, mantles and melee swings its animations tag make no sound, so enemies cannot hear it coming (a hidden cue); copy Valve's block into the model's GameDataList", want.join(", ")));
                } else if !lost.is_empty() {
                    out.push(format!("tagged sounds: {block} lacks {} that Valve's model plays from: the footsteps and movement sounds those sets give its animations make no sound, so enemies cannot hear it coming (a hidden cue); copy Valve's list", lost.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")));
                }
            }
            "CitadelCameraSettings_t" => match ob {
                None => out.push(format!("camera settings: {block} missing, which Valve's model has: the third-person camera takes the game's defaults instead of Valve's offsets for it, and a player sees more or less around it than Valve's camera shows; copy Valve's block")),
                Some(ob) => {
                    let changed: Vec<String> = vb
                        .members()
                        .iter()
                        .filter_map(|(k, v)| match ob.get(k) {
                            Some(o) if nums_differ(o, v) => Some(format!("{k} {} (Valve's {})", show(Some(o)), show(Some(v)))),
                            None => Some(format!("{k} none (Valve's {})", show(Some(v)))),
                            _ => None,
                        })
                        .collect();
                    if !changed.is_empty() {
                        out.push(format!("camera settings: {}: the third-person camera sits elsewhere than Valve's for this model, so a player sees more or less around it than Valve's camera shows; copy Valve's block", changed.join(", ")));
                    }
                }
            },
            _ if ob.is_none() => {
                out.push(format!("game data: {block} missing, which Valve's model has: the game uses its defaults for {}; copy Valve's block into the model's GameDataList; {STILL_PLAYS}", what_it_does(block)));
            }
            _ => {}
        }
    }
    out
}

/// Every attachment of a model with the numbers it is written with, per copy (every mesh repeats its attachments): the
/// offset and then the rotation of each influence it uses, the rotation's sign made canonical (w >= 0).
pub fn attachment_numbers(model: &[u8]) -> BTreeMap<String, Vec<Vec<f64>>> {
    let mut out: BTreeMap<String, Vec<Vec<f64>>> = BTreeMap::new();
    let Ok(r) = resource::parse(model) else { return out };
    for b in r.blocks_tagged(b"MDAT") {
        let bytes = resource::block_bytes(model, b);
        if !resource::kv3::is_kv3(bytes) {
            continue;
        }
        let Ok(m) = resource::kv3::decode(bytes) else { continue };
        for a in m.get("m_attachments").map(Value::as_array).unwrap_or(&[]) {
            let v = a.get("value").unwrap_or(a);
            let name = v.get("m_name").and_then(Value::as_str).unwrap_or("").to_string();
            let used = v.get("m_nInfluences").and_then(Value::as_f64).unwrap_or(1.0).max(1.0) as usize;
            let mut n = Vec::new();
            for o in v.get("m_vInfluenceOffsets").map(Value::as_array).unwrap_or(&[]).iter().take(used) {
                n.extend(o.numbers());
            }
            for q in v.get("m_vInfluenceRotations").map(Value::as_array).unwrap_or(&[]).iter().take(used) {
                let mut q = q.numbers();
                let lead = [3usize, 0, 1, 2].into_iter().filter_map(|i| q.get(i).copied()).find(|x| x.abs() > 1e-9).unwrap_or(0.0);
                if lead < 0.0 {
                    q.iter_mut().for_each(|x| *x = -*x);
                }
                n.extend(q);
            }
            out.entry(name).or_default().push(n);
        }
    }
    out
}

/// The camera attachments of Valve's model: the ones its game data lists as camera poses, and the ones held in the
/// hero's own frame (on a root bone or the root transform) whose name the game's client code names (`code`).
pub fn camera_attachments(valve: &[u8], contract: &Contract, code: Option<&HashSet<String>>) -> Vec<(String, &'static str)> {
    let mut out: Vec<(String, &'static str)> = read(valve).map(|g| camera_names(&g)).unwrap_or_default().into_iter().map(|n| (n, "a camera pose the model's AttachmentCameraData lists")).collect();
    if let Some(code) = code {
        let roots: HashSet<&str> = contract.bones.iter().filter(|(_, p)| p.is_empty()).map(|(n, _)| n.as_str()).collect();
        for (n, sig) in &contract.attachments {
            let on = sig.strip_prefix("on ").and_then(|s| s.split(" at ").next()).unwrap_or("");
            let hero_frame = on.is_empty() || on.split('+').any(|b| roots.contains(b));
            if hero_frame && code.contains(n) && !out.iter().any(|(x, _)| x == n) {
                out.push((n.clone(), "held in the hero's own frame and named by the game's client code"));
            }
        }
    }
    out
}

/// The angle in degrees between two rotations written x, y, z, w.
fn turn_degrees(a: &[f64], b: &[f64]) -> f64 {
    let d: f64 = a.iter().zip(b).map(|(x, y)| x * y).sum::<f64>().abs().min(1.0);
    2.0 * d.acos().to_degrees()
}

/// M062 findings for camera attachments that differ from Valve's by more than `mesh::rules::CAMERA_ATTACH_STEP` in any
/// number while the contract's two printed decimals call them the same. `reported` names the attachments the contract
/// already reported changed or missing.
pub fn camera_breaches(ours: &[u8], valve: &[u8], cams: &[(String, &'static str)], reported: &dyn Fn(&str) -> bool) -> Vec<String> {
    let step = crate::mesh::rules::CAMERA_ATTACH_STEP;
    let (o, v) = (attachment_numbers(ours), attachment_numbers(valve));
    let mut out = Vec::new();
    for (name, why) in cams {
        if reported(name) {
            continue;
        }
        let (Some(oc), Some(vc)) = (o.get(name), v.get(name)) else { continue };
        // each of our copies against the nearest of Valve's
        let worst = oc
            .iter()
            .filter_map(|x| vc.iter().filter(|y| y.len() == x.len()).map(|y| (x.iter().zip(y).map(|(p, q)| (p - q).abs()).fold(0.0, f64::max), x, y)).min_by(|a, b| a.0.total_cmp(&b.0)))
            .max_by(|a, b| a.0.total_cmp(&b.0));
        let Some((d, x, y)) = worst else { continue };
        if d <= step {
            continue;
        }
        let k = x.len() / 7 * 3;
        let moved = if k >= 3 { x[..3].iter().zip(&y[..3]).map(|(p, q)| (p - q) * (p - q)).sum::<f64>().sqrt() } else { 0.0 };
        let turned = if x.len() >= k + 4 { turn_degrees(&x[k..k + 4], &y[k..k + 4]) } else { 0.0 };
        out.push(format!(
            "attachment '{name}' ({why}) differs from Valve's by up to {d:.6} in its numbers (moved {moved:.4} units, turned {turned:.3} degrees): the contract's two decimals read it as Valve's, but the game places the camera from it, and it is past {step}, the most a Source 2 Viewer round trip of Valve's numbers leaves (6.1e-5 on the community corpus): the camera sits off its place, up to the camera in the middle of the screen; copy Valve's numbers (a Source 2 Viewer older than 20.0 rounds a pivot of -90 degrees to -89.972)"
        ));
    }
    out
}
