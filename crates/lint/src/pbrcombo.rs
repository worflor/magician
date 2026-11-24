//! M143 pbr-combo-invalid: a `pbr.vfx` material whose static combo breaks the shader's own rules.
//!
//! The material compiler accepts any combination (exit 0, flags kept) and then silently drops the inputs of
//! the feature the rules forbid: F_SHEEN with F_DETAIL compiles and loses `g_tDetail` (proven here, CSDK 13,
//! `tools/fabric_lab.py`). The game has no shader variant for a forbidden pair, so the dropped feature never
//! shows.
//!
//! SOURCE OF THE TABLE (the rules are encoded here, not parsed from the shader at lint time): the compiled
//! shader `shaders/vfx/pbr_pc_50_features.vcs` in `shaders_pc_dir.vpk`, read with Source 2 Viewer, build 6864
//! (2026-11-19): the static combo's `Allow1` exclusivity groups (at most one member on) and `Requires` rules.
//! It agrees with the tracked reconstruction `game/citadel/shaders_vulkan_dir/shaders/vfx/pbr.slang`
//! (GameTracking-Deadlock). Parsing the .vcs at lint time was judged out of scope; when a build changes the
//! shader, re-read it and update `EXCLUDES` / `REQUIRES` (the date above is the check).

use resource::kv3::Value;

/// Pairs that may not both be on.
pub const EXCLUDES: &[(&str, &[&str])] = &[
    ("F_SHEEN", &["F_GLASS", "F_CLOAK", "F_PAINT_VERTEX_COLORS", "F_TRANSLUCENT", "F_ADVANCED_TRANSLUCENCY", "F_DETAIL", "F_UNLIT"]),
    ("F_DETAIL", &["F_UNLIT", "F_PAINT_VERTEX_COLORS"]),
    ("F_USE_STATUS_EFFECTS_PROXY", &["F_CLOAK"]),
];

/// A flag that needs every `all` and at least one of `any` (empty: no such requirement).
pub struct Requires {
    pub flag: &'static str,
    pub all: &'static [&'static str],
    pub any: &'static [&'static str],
}

pub const REQUIRES: &[Requires] = &[
    Requires { flag: "F_USE_FRONT_FACE_NORMALS_FOR_BACK_FACES", all: &["F_ALPHA_TEST", "F_RENDER_BACKFACES"], any: &[] },
    Requires { flag: "F_USE_STATUS_EFFECTS_PROXY", all: &[], any: &["F_USE_NPR_LIGHTING", "F_UNLIT", "F_SELF_ILLUM"] },
];

const DETAIL_LOST: &str = ": the compiler exits 0 but drops g_tDetail from the compiled material, so the detail layer never shows";
const GENERIC: &str = ": the compiler exits 0 but silently drops the inputs of one of the features, and the game has no shader variant for it, so the material does not render as authored";

/// The problems of one compiled material, `[]` when it is not a pbr material or its combo is legal.
pub fn problems(vmat: &[u8]) -> Vec<String> {
    let Some(Ok(d)) = resource::kv3_block(vmat, b"DATA") else { return Vec::new() };
    if !d.get("m_shaderName").and_then(Value::as_str).unwrap_or("").starts_with("pbr") {
        return Vec::new();
    }
    let on: Vec<&str> = d
        .get("m_intParams")
        .map(Value::as_array)
        .unwrap_or(&[])
        .iter()
        .filter(|p| p.get("m_nValue").and_then(Value::as_f64).unwrap_or(0.0) != 0.0)
        .filter_map(|p| p.get("m_name").and_then(Value::as_str))
        .collect();
    let is_on = |f: &str| on.contains(&f);
    let mut out = Vec::new();
    for (a, bs) in EXCLUDES {
        if !is_on(a) {
            continue;
        }
        for b in bs.iter().filter(|b| is_on(b)) {
            out.push(format!("{a} together with {b} is not a combination the shader allows (at most one of them)"));
        }
    }
    for r in REQUIRES {
        if !is_on(r.flag) {
            continue;
        }
        let lacking: Vec<&str> = r.all.iter().copied().filter(|f| !is_on(f)).collect();
        if !lacking.is_empty() {
            out.push(format!("{} needs {} on", r.flag, lacking.join(" and ")));
        }
        if !r.any.is_empty() && !r.any.iter().any(|f| is_on(f)) {
            out.push(format!("{} needs one of {} on", r.flag, r.any.join(", ")));
        }
    }
    out.into_iter().map(|m| format!("{m}{}", if m.starts_with("F_SHEEN together with F_DETAIL") { DETAIL_LOST } else { GENERIC })).collect()
}
