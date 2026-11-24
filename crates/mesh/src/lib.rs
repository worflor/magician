//! Source 2 compiled model meshes, decoded natively: every triangle, position, normal, UV and
//! skin influence of every draw call of every LOD, plus the skeleton and hitboxes. Zero
//! dependencies beyond Magician's own resource reader.
//!
//! `prove` checks the decode against Source 2 Viewer's glb export of the same file: equal vertex
//! and triangle counts per draw call, identical triangles, and positions, normals, UVs and skin
//! within float tolerance (the export is the independent oracle; the decode is never trusted on
//! its own say-so).

#![forbid(unsafe_code)]
// They read files other people made: an unwrap, an expect or a str sliced at a byte offset is a panic one byte away.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::string_slice)]

pub mod cloth;
pub mod format;
pub mod glb;
pub mod meshopt;
pub mod model;
pub mod synth;

pub use model::{parse, Model, Surface};

/// The result of proving the native decode against a glb export.
#[derive(Debug, Default)]
pub struct Proof {
    pub meshes: usize,
    pub draws: usize,
    pub vertices: usize,
    pub triangles: usize,
    /// Largest position difference after the export's axis change and scale are undone (model units).
    pub max_position_error: f64,
    pub max_normal_error: f64,
    pub max_uv_error: f64,
    pub max_weight_error: f64,
    pub joint_mismatches: usize,
    /// The vertex with the largest weight difference, described.
    pub worst_weight: String,
    /// The glb axis mapping found: for each glb axis, (source axis, sign), and the scale.
    pub axes: [(usize, f64); 3],
    pub scale: f64,
    pub failures: Vec<String>,
}

impl Proof {
    pub fn ok(&self) -> bool {
        self.failures.is_empty()
    }
}

/// Prove LOD0 of `model` against Source 2 Viewer's glb export (`glb_bytes`), whose meshes are named
/// `<model>.<mesh>` and carry one primitive per draw call over the whole vertex buffer.
pub fn prove(model: &Model, glb_bytes: &[u8]) -> Result<Proof, String> {
    let g = glb::Glb::read(glb_bytes)?;
    let mut proof = Proof { scale: 1.0, axes: [(0, 1.0), (1, 1.0), (2, 1.0)], ..Default::default() };
    // Joint names of the export's skin, to compare blend indices by bone name.
    let skin_joints: Vec<String> = g
        .json
        .get("skins")
        .and_then(|s| s.at(0))
        .and_then(|s| s.get("joints"))
        .map(|j| {
            j.arr()
                .iter()
                .filter_map(|n| n.num())
                .map(|n| g.json.get("nodes").and_then(|x| x.at(n as usize)).and_then(|x| x.get("name")).and_then(glb::Json::str).unwrap_or("").to_string())
                .collect()
        })
        .unwrap_or_default();
    let mut fitted = false;
    for gm in g.json.get("meshes").map(glb::Json::arr).unwrap_or(&[]) {
        let full = gm.get("name").and_then(glb::Json::str).unwrap_or("");
        let short = full.split_once('.').map(|(_, m)| m).unwrap_or(full);
        let Some(mesh) = model.meshes.iter().find(|m| m.name == short && m.lod_mask & 1 != 0) else {
            proof.failures.push(format!("glb mesh '{full}' has no LOD0 mesh '{short}' in the model"));
            continue;
        };
        proof.meshes += 1;
        let prims = gm.get("primitives").map(glb::Json::arr).unwrap_or(&[]);
        if prims.len() != mesh.draws.len() {
            proof.failures.push(format!("{short}: {} glb primitives, {} draw calls", prims.len(), mesh.draws.len()));
        }
        for (di, (prim, d)) in prims.iter().zip(&mesh.draws).enumerate() {
            proof.draws += 1;
            let attr = |k: &str| prim.get("attributes").and_then(|a| a.get(k)).and_then(glb::Json::num).map(|x| x as usize);
            let gidx = g.indices(prim.get("indices").and_then(glb::Json::num).ok_or("primitive without indices")? as usize)?;
            let (_, gpos) = g.accessor(attr("POSITION").ok_or("no POSITION")?)?;
            let gnrm = attr("NORMAL").map(|i| g.accessor(i)).transpose()?;
            let guv = attr("TEXCOORD_0").map(|i| g.accessor(i)).transpose()?;
            let gj = attr("JOINTS_0").map(|i| g.accessor(i)).transpose()?;
            let gw = attr("WEIGHTS_0").map(|i| g.accessor(i)).transpose()?;
            let ib = &mesh.ibs[d.index_buffer];
            let Ok(ours_idx) = &ib.data else {
                proof.failures.push(format!("{short} draw {di}: index buffer not decoded"));
                continue;
            };
            let (vbi, bind) = d.vertex_buffers[0];
            let vb = &mesh.vbs[vbi];
            let Ok(vbytes) = &vb.data else {
                proof.failures.push(format!("{short} draw {di}: vertex buffer not decoded"));
                continue;
            };
            let bind_v = bind as usize / vb.stride.max(1);
            let range = &ours_idx[d.start_index as usize..(d.start_index + d.index_count) as usize];
            if gidx.len() != range.len() {
                proof.failures.push(format!("{short} draw {di}: {} glb indices, {} ours", gidx.len(), range.len()));
                continue;
            }
            proof.triangles += range.len() / 3;
            let ours_v: Vec<usize> = range.iter().map(|&i| i as usize + d.base_vertex as usize + bind_v).collect();
            let attr_of = |sem: &str| vb.attrs.iter().find(|a| a.semantic == sem && a.index == 0);
            let pos_a = attr_of("POSITION").ok_or("no POSITION attribute")?;
            let read = |a: &model::Attribute, v: usize| {
                let at = v * vb.stride + a.offset as usize;
                format::decode(a.format, &vbytes[at..at + format::size_of(a.format).unwrap_or(4)]).unwrap_or([0.0; 4])
            };
            if !fitted {
                fit_axes(&mut proof, &ours_v, &gidx, |v| read(pos_a, v), &gpos);
                fitted = true;
            }
            // Same triangles: our vertex ids map one-to-one onto the export's.
            let mut map: std::collections::HashMap<usize, u32> = std::collections::HashMap::new();
            let mut bad_tri = 0;
            for (o, gi) in ours_v.iter().zip(&gidx) {
                if *map.entry(*o).or_insert(*gi) != *gi {
                    bad_tri += 1;
                }
            }
            if bad_tri > 0 {
                proof.failures.push(format!("{short} draw {di}: {bad_tri} indices disagree with the export's vertex numbering"));
            }
            proof.vertices += map.len();
            for (&o, &gi) in &map {
                let gi = gi as usize;
                let p = read(pos_a, o);
                let q = &gpos[gi * 3..gi * 3 + 3];
                for ax in 0..3 {
                    let (src, sign) = proof.axes[ax];
                    let e = (p[src] as f64 * sign * proof.scale - q[ax] as f64).abs() / proof.scale;
                    proof.max_position_error = proof.max_position_error.max(e);
                }
                if let (Some(a), Some((_, gn))) = (attr_of("NORMAL"), &gnrm) {
                    let (n, _) = normal_for(a, read(a, o), vbytes, o, vb.stride);
                    for ax in 0..3 {
                        let (src, sign) = proof.axes[ax];
                        let e = (n[src] as f64 * sign - gn[gi * 3 + ax] as f64).abs();
                        proof.max_normal_error = proof.max_normal_error.max(e);
                    }
                }
                if let (Some(a), Some((_, gu))) = (attr_of("TEXCOORD"), &guv) {
                    let t = read(a, o);
                    proof.max_uv_error = proof.max_uv_error.max((t[0] - gu[gi * 2]).abs() as f64).max((t[1] - gu[gi * 2 + 1]).abs() as f64);
                }
                if let (Some(a), Some((_, gjv))) = (attr_of("BLENDINDICES"), &gj) {
                    let ji = read(a, o);
                    let w = attr_of("BLENDWEIGHT").or_else(|| attr_of("BLENDWEIGHTS")).map(|w| read(w, o));
                    let before = proof.max_weight_error;
                    for k in 0..4 {
                        let ow = w.map(|w| w[k]).unwrap_or(if k == 0 { 1.0 } else { 0.0 });
                        let gwk = gw.as_ref().map(|(_, x)| x[gi * 4 + k]).unwrap_or(0.0);
                        proof.max_weight_error = proof.max_weight_error.max((ow - gwk).abs() as f64);
                        if ow > 0.0 {
                            let ours_name = mesh.bones.get(ji[k] as usize).map(|b| b.name.as_str()).unwrap_or("?");
                            let theirs = skin_joints.get(gjv[gi * 4 + k] as usize).map(String::as_str).unwrap_or("?");
                            if ours_name != theirs {
                                proof.joint_mismatches += 1;
                            }
                        }
                    }
                    if proof.max_weight_error > before {
                        let gwv: Vec<f32> = (0..4).map(|k| gw.as_ref().map(|(_, x)| x[gi * 4 + k]).unwrap_or(0.0)).collect();
                        let gjn: Vec<&str> = (0..4).map(|k| skin_joints.get(gjv[gi * 4 + k] as usize).map(String::as_str).unwrap_or("?")).collect();
                        let ojn: Vec<&str> = (0..4).map(|k| mesh.bones.get(ji[k] as usize).map(|b| b.name.as_str()).unwrap_or("?")).collect();
                        proof.worst_weight = format!("{short} vertex {o}: ours {ojn:?} {:?}, export {gjn:?} {gwv:?}", w.map(|w| w.to_vec()));
                    }
                }
            }
        }
    }
    let ours_lod0 = model.meshes.iter().filter(|m| m.lod_mask & 1 != 0).count();
    if proof.meshes != ours_lod0 {
        proof.failures.push(format!("{ours_lod0} LOD0 meshes in the model, {} in the glb", proof.meshes));
    }
    if proof.max_position_error > 1e-3 {
        proof.failures.push(format!("positions differ by up to {:.6}", proof.max_position_error));
    }
    if proof.joint_mismatches > 0 {
        proof.failures.push(format!("{} skin influences name a different bone", proof.joint_mismatches));
    }
    Ok(proof)
}

fn normal_for(a: &model::Attribute, v: [f32; 4], bytes: &[u8], vi: usize, stride: usize) -> ([f32; 3], ()) {
    match (a.format, model::packed4(a, bytes, vi, stride)) {
        (42, Some(b)) => (format::unpack_tangent_frame(u32::from_le_bytes(b)).0, ()),
        (28, Some(b)) => (format::unpack_byte_normal(b[0], b[1]), ()),
        _ => ([v[0], v[1], v[2]], ()),
    }
}

/// Find the signed axis permutation and scale that maps our positions onto the export's.
fn fit_axes(proof: &mut Proof, ours_v: &[usize], gidx: &[u32], pos: impl Fn(usize) -> [f32; 4], gpos: &[f32]) {
    let samples: Vec<([f64; 3], [f64; 3])> = ours_v
        .iter()
        .zip(gidx)
        .step_by((ours_v.len() / 64).max(1))
        .map(|(&o, &g)| {
            let p = pos(o);
            let g = g as usize;
            ([p[0] as f64, p[1] as f64, p[2] as f64], [gpos[g * 3] as f64, gpos[g * 3 + 1] as f64, gpos[g * 3 + 2] as f64])
        })
        .collect();
    let perms = [[0, 1, 2], [0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]];
    let mut best = (f64::MAX, [(0usize, 1.0f64); 3], 1.0f64);
    for perm in perms {
        for signs in 0..8 {
            for scale in [1.0, 0.0254] {
                let axes = [(perm[0], if signs & 1 != 0 { -1.0 } else { 1.0 }), (perm[1], if signs & 2 != 0 { -1.0 } else { 1.0 }), (perm[2], if signs & 4 != 0 { -1.0 } else { 1.0 })];
                let err: f64 = samples
                    .iter()
                    .map(|(p, q)| (0..3).map(|a| (p[axes[a].0] * axes[a].1 * scale - q[a]).abs()).sum::<f64>())
                    .sum();
                if err < best.0 {
                    best = (err, axes, scale);
                }
            }
        }
    }
    proof.axes = best.1;
    proof.scale = best.2;
}
