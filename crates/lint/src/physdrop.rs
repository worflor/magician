//! M160 physics-dropped: a replaced model whose Valve original simulates cloth or carries a ragdoll, where ours has no
//! cloth at all, or lacks ragdoll parts Valve's has.
//!
//! Both live in the model's PHYS block: the cloth as the FeModel (`mesh::cloth`), the ragdoll as its parts, one per
//! bone (`m_boneNames`). A model built from a Source 2 Viewer decompile has no cloth simulation (a modder's report on
//! the community's model tutorial, 2026-07-12), and one mod builder re-injects ragdoll joints after a decompile because
//! they are lost the same way. Like distance fields (M142), the compile says nothing. A skin's own cloth with other
//! node counts is its design and is judged by M141; only cloth gone altogether is said here.

use resource::kv3::Value;

/// What a model's PHYS block holds: the ragdoll's parts by bone, and the cloth's node count. `None`: no PHYS block, or
/// one that does not decode.
pub fn read(data: &[u8]) -> Option<(Vec<String>, usize)> {
    let p = resource::kv3_block(data, b"PHYS")?.ok()?;
    let parts = p.get("m_boneNames").map(|a| a.as_array().iter().filter_map(Value::as_str).map(String::from).collect()).unwrap_or_default();
    let cloth = ::mesh::cloth::from_value(&p).map_or(0, |c| c.node_count);
    Some((parts, cloth))
}

fn names(list: &[String]) -> String {
    match list.len() {
        0 => String::new(),
        n if n <= 6 => list.join(", "),
        n => format!("{} and {} more", list[..5].join(", "), n - 5),
    }
}

/// The M160 messages for our model against Valve's at the same path.
pub fn judge(ours: &[u8], valve: &[u8]) -> Vec<String> {
    let Some((vparts, vcloth)) = read(valve) else { return Vec::new() };
    let (oparts, ocloth) = read(ours).unwrap_or_default();
    let mut out = Vec::new();
    if vcloth > 0 && ocloth == 0 {
        out.push(format!(
            "Valve's model simulates cloth ({vcloth} nodes) and ours has none: the coat tails, hair and straps that swing on Valve's hero hang stiff on their bones (a model built from a Source 2 Viewer decompile has no cloth); author the cloth in ModelDoc, or accept this with a reason in magician.allow"
        ));
    }
    let lost: Vec<String> = vparts.iter().filter(|b| !oparts.iter().any(|o| o.eq_ignore_ascii_case(b))).cloned().collect();
    if !vparts.is_empty() && !lost.is_empty() {
        let ours_has = if oparts.is_empty() { "ours has none".to_string() } else { format!("ours lacks {} of them ({})", lost.len(), names(&lost)) };
        let parts = if vparts.len() == 1 { "1 part".to_string() } else { format!("{} parts", vparts.len()) };
        out.push(format!(
            "Valve's model has physics of {parts} (its ragdoll, or a prop's body) and {ours_has}: when it dies, falls or breaks, what those parts hold is not simulated and keeps its last pose (seen on 6880: a skin without Valve's weapon part left the hero's gun hanging in the air where his hand was while the body was gone, until he came back); keep Valve's physics (a decompile loses its joints), or accept this with a reason in magician.allow"
        ));
    }
    out
}
