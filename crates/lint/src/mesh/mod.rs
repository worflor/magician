//! The mesh rule family (M100-M140): compiled models decoded natively (`mesh` crate), measured with
//! the geometry kernel (`geom` crate), judged against Valve's original at the same path.

pub mod census;
pub mod openings;
pub mod rules;
pub mod views;

pub use census::{mat_info, Census, Env, MatInfo};
use crate::policy::FairPlay;

/// Mesh-family codes the light pass emits (`judge_light`, `audit_materials`, `particle_prop`); every other
/// M101-M140 rule needs the geometry census and runs only under `--deep`.
const LIGHT: &[&str] = &["M100", "M120", "M122", "M123", "M131", "M132", "M133", "M134", "M146"];

/// Mesh-family codes numbered past the family's first block (M100-M140).
const LATER: &[&str] = &["M145", "M146", "M147"];

/// Whether `code` runs only in a deep lint.
pub fn deep_only(code: &str) -> bool {
    (code.strip_prefix('M').and_then(|n| n.parse::<u32>().ok()).is_some_and(|n| (100..=140).contains(&n)) || LATER.contains(&code)) && !LIGHT.contains(&code)
}

/// Every mesh and material rule for one compiled model: ours against Valve's at the same path when
/// there is one. `read_ours` resolves a compiled path from the mod first, then the game; `read_valve`
/// from the game only. A model with no embedded meshes (no CTRL block: geometry in referenced
/// `.vmesh` files) is outside this family; a model whose meshes fail to decode reports M100. Where the deep run's
/// fair-play geometry (M124, M129, M130, M140) cannot be judged, M144 says so: unchecked is never a pass.
pub fn check_model(ours: &[u8], valve: Option<&[u8]>, read_ours: &(dyn Fn(&str) -> Option<Vec<u8>> + Sync), read_valve: &(dyn Fn(&str) -> Option<Vec<u8>> + Sync), deep: bool, fp: &FairPlay) -> Vec<(&'static str, String)> {
    let has_ctrl = resource::parse(ours).is_ok_and(|r| r.block(b"CTRL").is_some());
    if !has_ctrl {
        let valve_has_meshes = valve.and_then(|d| resource::parse(d).ok()).is_some_and(|r| r.block(b"CTRL").is_some());
        if deep && valve_has_meshes {
            return vec![("M144", "the model carries no embedded meshes where Valve's does (its geometry is in referenced files the mesh family does not read), so its silhouette and hitbox envelope were not judged (M124, M129, M140)".into())];
        }
        return Vec::new();
    }
    let om = |p: &str| read_ours(&crate::deep::compiled(p)).and_then(|d| mat_info(&d));
    let ot = |p: &str| read_ours(p).and_then(|d| crate::deep::tex_info(&d));
    let build = |d: &[u8], env: &Env| if deep { Census::build(d, env) } else { Census::light(d, env) };
    let vm = |p: &str| read_valve(&crate::deep::compiled(p)).and_then(|d| mat_info(&d));
    let vt = |p: &str| read_valve(p).and_then(|d| crate::deep::tex_info(&d));
    // Ours and Valve's are measured at the same time: neither reads the other.
    let build_ours = || build(ours, &Env { material: &om, texture: &ot, file: read_ours });
    let build_valve = || valve.and_then(|d| resource::parse(d).ok().filter(|r| r.block(b"CTRL").is_some()).and_then(|_| build(d, &Env { material: &vm, texture: &vt, file: read_valve }).ok()));
    let (o, v) = crate::par::join(build_ours, build_valve);
    let o = match o {
        Ok(c) => c,
        Err(e) => {
            let mut out = vec![("M100", format!("the model's meshes could not be read: {e}"))];
            if deep {
                out.push(("M144", "the model's meshes could not be read, so its silhouette and hitbox envelope were not judged (M124, M129, M140)".into()));
            }
            return out;
        }
    };
    let mut out = if deep { rules::judge(&o, v.as_ref(), fp) } else { rules::judge_light(&o, v.as_ref()) };
    if deep && valve.is_some() && v.is_none() {
        out.push(("M144", "Valve's model at this path could not be measured, so our silhouette was not compared with it (M124)".into()));
    }
    if let Some(v) = &v {
        let list = |c: &Census| c.materials.iter().map(|(k, m)| (k.clone(), m.clone())).collect::<Vec<_>>();
        out.extend(rules::audit_materials(&list(&o), &list(v)));
    }
    out
}

/// Materials a particle system draws as models (`C_OP_RenderModels` `m_hOverrideMaterial`).
pub fn particle_model_materials(kv: &resource::kv3::Value) -> Vec<String> {
    let mut out = Vec::new();
    kv.walk(&mut |_, v| {
        if v.get("_class").and_then(resource::kv3::Value::as_str) == Some("C_OP_RenderModels") {
            if let Some(m) = v.get("m_hOverrideMaterial").and_then(resource::kv3::Value::as_str) {
                if !m.is_empty() {
                    out.push(m.to_ascii_lowercase());
                }
            }
        }
    });
    out
}
