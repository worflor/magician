//! M142 distance-field-dropped: a replaced model whose Valve original carries a DSTF block (model distance
//! fields: one voxel grid per bone, used for ambient occlusion) while ours has none, or fewer.
//!
//! The compiler writes the block only from ModelDoc `DistanceFieldList` nodes, and Source 2 Viewer's vmdl
//! decompile does not emit them, so a model built from a decompile loses them without a word (Maid Lash's
//! lash.vmdl_c: Valve 25 fields, ours none; a replaced magician_cloneturret_model.vmdl_c: Valve 22, ours none;
//! build 6864). Fair-play neutral: a warning, accepted with a reason in `magician.allow`.

use resource::kv3::Value;
use std::collections::BTreeSet;

/// The seed the engine hashes a field's parent bone name with (decoded by a research pass; Lash's `spine_1`
/// hashes to 110709982, the value its DSTF carries).
pub const BONE_HASH_SEED: u32 = 0x3141_5926;

/// MurmurHash2 (32-bit) of a bone name.
pub fn murmur2(data: &[u8], seed: u32) -> u32 {
    const M: u32 = 0x5bd1_e995;
    let mut h = seed ^ data.len() as u32;
    let (words, r) = data.as_chunks::<4>();
    for c in words {
        let mut k = u32::from_le_bytes(*c).wrapping_mul(M);
        k ^= k >> 24;
        k = k.wrapping_mul(M);
        h = h.wrapping_mul(M) ^ k;
    }
    if r.len() >= 3 {
        h ^= (r[2] as u32) << 16;
    }
    if r.len() >= 2 {
        h ^= (r[1] as u32) << 8;
    }
    if !r.is_empty() {
        h ^= r[0] as u32;
        h = h.wrapping_mul(M);
    }
    h ^= h >> 13;
    h = h.wrapping_mul(M);
    h ^ (h >> 15)
}

/// What a model's DSTF block holds: the parent-bone hash of every field.
pub struct Fields {
    pub hashes: Vec<u32>,
}

/// `Ok(None)`: no DSTF block. `Err`: the block is there and does not decode (unchecked is never clean).
pub fn read(data: &[u8]) -> Result<Option<Fields>, String> {
    let Some(d) = resource::kv3_block(data, b"DSTF") else { return Ok(None) };
    let d = d?;
    let hashes = d
        .get("m_distanceFields")
        .map(Value::as_array)
        .unwrap_or(&[])
        .iter()
        .map(|f| f.get("m_nParentBoneNameHash").and_then(Value::as_f64).unwrap_or(0.0) as i64 as u32)
        .collect();
    Ok(Some(Fields { hashes }))
}

fn bone_names(data: &[u8]) -> Vec<String> {
    let Some(Ok(d)) = resource::kv3_block(data, b"DATA") else { return Vec::new() };
    d.get("m_modelSkeleton")
        .and_then(|s| s.get("m_boneName"))
        .map(|a| a.as_array().iter().filter_map(|x| x.as_str().map(String::from)).collect())
        .unwrap_or_default()
}

/// The M142 message for our model against Valve's at the same path, if it drops distance fields.
pub fn judge(ours: &[u8], valve: &[u8]) -> Option<String> {
    let v = match read(valve) {
        Ok(Some(v)) if !v.hashes.is_empty() => v,
        _ => return None, // Valve's has none (or unreadable): nothing to lose
    };
    let o = match read(ours) {
        Ok(o) => o.map_or(Vec::new(), |f| f.hashes),
        Err(_) => return None, // reported as unreadable by `unreadable`
    };
    let have: BTreeSet<u32> = o.iter().copied().collect();
    let missing: BTreeSet<u32> = v.hashes.iter().copied().filter(|h| !have.contains(h)).collect();
    if o.len() >= v.hashes.len() && missing.is_empty() {
        return None;
    }
    let names = bone_names(valve);
    let name_of = |h: u32| {
        names
            .iter()
            .find(|n| murmur2(n.as_bytes(), BONE_HASH_SEED) == h || murmur2(n.to_ascii_lowercase().as_bytes(), BONE_HASH_SEED) == h)
            .cloned()
            .unwrap_or_else(|| format!("bone {h:#010x}"))
    };
    let list: Vec<String> = missing.iter().map(|h| name_of(*h)).collect();
    let (n, m) = (o.len(), v.hashes.len());
    let ours_has = if n == 0 { "ours has none".to_string() } else { format!("ours has {n}") };
    let cover = if missing.is_empty() { String::new() } else if n == 0 { format!(", on {}", list.join(", ")) } else { format!(", with none on {} of Valve's {} bones ({})", missing.len(), v.hashes.iter().collect::<BTreeSet<_>>().len(), list.join(", ")) };
    Some(format!(
        "Valve's model carries {m} distance field(s) (the ambient occlusion each bone casts on its neighbours and the world), {ours_has}{cover}: the model loses that occlusion and its lighting contact shadows, and the compiler said nothing (a model built from a Source 2 Viewer decompile drops them; author ModelDoc DistanceFieldList nodes to keep them, or accept this with a reason in magician.allow)"
    ))
}

/// A DSTF block we could not decode: the rule above did not run on it.
pub fn unreadable(ours: &[u8]) -> Option<String> {
    read(ours).err().map(|e| format!("distance fields not checked (M142): the DSTF block does not decode ({e})"))
}
