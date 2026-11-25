//! M148 default-slot-literal: a replaced material whose texture slot Valve leaves to an engine default
//! (`materials/default/...`) now holds a constant texture of a different value.
//!
//! Source 2 Viewer's material decompile prints a slot whose compiled texture is an engine default as a literal
//! colour, and prints it wrong: `"TextureGlassMask1" "[0.000000 0.000000 0.000000 0.000000]"` for a slot that
//! compiled to `materials/default/default_mask_tga_*.vtex` (white). Recompiled, the literal becomes a generated
//! constant texture (`<material>_vmat_g_t<slot>_<hash>.vtex`) holding 0 where Valve's file holds 1. Real case
//! (2026-11-24, build 6865): an F_GLASS material whose glass mask went from 1 to 0 drew a summon as a solid block
//! with the hero inside it invisible, and lint passed it (M061 listed the dropped default as a dependency only).
//!
//! The rule is relational: per slot, Valve's file at the same path names an engine default, ours names a texture
//! that decodes to one colour, and some channel of that colour is more than half the range away from the default's
//! own pixels (read from the game, not assumed). Valve's own file, packed unchanged, names the same defaults and is
//! silent. A slot the mod fills with a real (varying) texture is the mod's design and not judged. When a texture
//! cannot be read the slot is said to be unchecked (M144), never passed.

use resource::kv3::Value;

/// How far (0..1 of the channel range) a constant must sit from Valve's default to be a different value, not
/// compression noise: half the range, so 1 against 0 fires and 0.98 against 1 does not.
pub const LITERAL_GAP: f64 = 0.5;
/// How far texels of a "constant" texture may spread (0..255) and still be one colour.
pub const CONSTANT_SPREAD: u8 = 2;

/// (slot name, texture path as compiled: `..._c`) for every texture parameter of a compiled material.
pub fn texture_slots(vmat: &[u8]) -> Option<Vec<(String, String)>> {
    let d = resource::kv3_block(vmat, b"DATA")?.ok()?;
    Some(
        d.get("m_textureParams")
            .map(Value::as_array)
            .unwrap_or(&[])
            .iter()
            .filter_map(|p| {
                let name = p.get("m_name").and_then(Value::as_str)?.to_string();
                let v = p.get("m_pValue").and_then(Value::as_str)?.to_ascii_lowercase();
                (!v.is_empty()).then(|| (name, crate::deep::compiled(&v)))
            })
            .collect(),
    )
}

pub fn is_engine_default(path: &str) -> bool {
    path.starts_with("materials/default/")
}

/// The one colour a texture holds (RGBA, 0..1), or None when its texels vary (or it does not decode).
pub fn constant_colour(tex: &[u8]) -> Option<[f64; 4]> {
    let (_, _, px) = resource::pixels::decode(tex, 0)?;
    let first: [u8; 4] = px.get(0..4)?.try_into().ok()?;
    let spread_ok = px.as_chunks::<4>().0.iter().all(|c| c.iter().zip(first).all(|(&a, b)| a.abs_diff(b) <= CONSTANT_SPREAD));
    spread_ok.then(|| first.map(|c| c as f64 / 255.0))
}

/// What one comparison found.
pub enum Verdict {
    /// The slot's constant differs from Valve's default: the M148 message.
    Changed(String),
    /// A texture this rule needed did not decode: the M144 message.
    Unchecked(String),
}

/// Judge our material against Valve's at the same path. `read` gives a texture's bytes (the mod's copy first,
/// else the game's).
pub fn judge(ours: &[u8], valve: &[u8], read: &dyn Fn(&str) -> Option<Vec<u8>>) -> Vec<Verdict> {
    let (Some(o), Some(v)) = (texture_slots(ours), texture_slots(valve)) else { return Vec::new() };
    let mut out = Vec::new();
    for (slot, vpath) in &v {
        if !is_engine_default(vpath) {
            continue;
        }
        let Some((_, opath)) = o.iter().find(|(s, _)| s == slot) else { continue };
        if opath == vpath || is_engine_default(opath) {
            continue;
        }
        let (Some(ob), Some(vb)) = (read(opath), read(vpath)) else {
            out.push(Verdict::Unchecked(format!("{slot}: {opath} or Valve's default {vpath} could not be read, so the slot's value was not compared (M148)")));
            continue;
        };
        let Some(oc) = constant_colour(&ob) else { continue }; // a real texture: the mod's own design
        let Some(vc) = constant_colour(&vb) else {
            out.push(Verdict::Unchecked(format!("{slot}: Valve's default {vpath} did not decode to one colour, so the slot's value was not compared (M148)")));
            continue;
        };
        let gap = (0..4).map(|i| (oc[i] - vc[i]).abs()).fold(0.0, f64::max);
        if gap > LITERAL_GAP {
            let show = |c: [f64; 4]| format!("[{:.2} {:.2} {:.2} {:.2}]", c[0], c[1], c[2], c[3]);
            out.push(Verdict::Changed(format!(
                "{slot}: Valve's material leaves it to the engine default {vpath}, which reads {}; ours is the constant {} ({opath}) over the whole surface. A Source 2 Viewer decompile prints such a default as a literal colour, often the wrong one (a glass mask of 0 for Valve's 1 hid a hero inside an ability): name the default texture again, or keep the value with a reason in magician.allow",
                show(vc),
                show(oc)
            )));
        }
    }
    out
}
