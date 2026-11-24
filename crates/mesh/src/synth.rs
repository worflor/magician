//! Write a compiled model from plain geometry, in the layout `resourcecompiler` produces (CTRL with
//! embedded meshes, MVTX/MIDX/MDAT per mesh, a DATA block with skeleton, LOD masks and material
//! groups). Lint fixtures are built with it, so every mesh rule is proven on a file whose every
//! triangle we chose. Vertex buffers are meshopt-encoded (codec v0) unless `raw` is set.

use resource::kv3::Value as V;

#[derive(Debug, Clone, Default)]
pub struct SynthDraw {
    pub material: String,
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    /// Mesh-local bone indices and weights (four influences).
    pub joints: Vec<[u8; 4]>,
    pub weights: Vec<[f32; 4]>,
    pub tris: Vec<[u32; 3]>,
}

#[derive(Debug, Clone, Default)]
pub struct SynthMesh {
    pub name: String,
    pub lod_mask: u64,
    pub draws: Vec<SynthDraw>,
    /// Bones the mesh's blend indices count in: (name, model-space bind position).
    pub bones: Vec<(String, [f32; 3])>,
    /// Leave TEXCOORD out of the vertex layout.
    pub no_uv: bool,
}

#[derive(Debug, Clone, Default)]
pub struct SynthModel {
    pub name: String,
    pub meshes: Vec<SynthMesh>,
    /// Model skeleton: name, parent, bind position relative to the parent (identity rotations).
    pub skeleton: Vec<(String, i32, [f32; 3])>,
    pub raw: bool,
    /// Capsule hitboxes (set "default"): name, bone, end points in bone space, radius.
    pub hitboxes: Vec<(String, String, [f32; 3], [f32; 3], f32)>,
    /// The shape of each hitbox above, by position: 2 a capsule (the default when the list is shorter), 1 a sphere
    /// at the first point, 0 a box between the two points in the bone's frame.
    pub hitbox_shapes: Vec<i64>,
    /// Body-group choice of each mesh, by position (`m_refMeshGroupMasks`; choice 1 is the default): empty for a
    /// model with no body groups.
    pub mesh_group_masks: Vec<u64>,
    /// Materials listed in the default material group beyond the ones drawn.
    pub extra_materials: Vec<String>,
    /// More blocks after DATA (e.g. a PHYS block with a cloth FeModel), as (tag, bytes).
    pub extra_blocks: Vec<([u8; 4], Vec<u8>)>,
}

fn s(x: &str) -> V {
    V::Str(x.into())
}
fn i(x: i64) -> V {
    V::Int(x)
}
fn b(x: bool) -> V {
    V::Bool(x)
}
fn f(x: f64) -> V {
    V::Double(x)
}
fn obj(m: Vec<(&str, V)>) -> V {
    V::Object(m.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

fn field(name: &str, format: i64, offset: i64) -> V {
    obj(vec![("m_pSemanticName", s(name)), ("m_nSemanticIndex", i(0)), ("m_Format", i(format)), ("m_nOffset", i(offset)), ("m_nSlot", i(0)), ("m_nSlotType", s("RENDER_SLOT_PER_VERTEX")), ("m_szShaderSemantic", s(""))])
}

/// Pack a unit normal into Source 2's 32-bit tangent frame (octahedral, tangent angle 0).
pub fn pack_normal(n: [f32; 3]) -> u32 {
    let l = n[0].abs() + n[1].abs() + n[2].abs();
    let (mut x, mut y) = if l > 0.0 { (n[0] / l, n[1] / l) } else { (0.0, 0.0) };
    if n[2] < 0.0 {
        let (ox, oy) = (x, y);
        x = (1.0 - oy.abs()) * if ox >= 0.0 { 1.0 } else { -1.0 };
        y = (1.0 - ox.abs()) * if oy >= 0.0 { 1.0 } else { -1.0 };
    }
    let q = |v: f32| (((v + 1.0) * 0.5 * 1023.0).round() as u32).min(1023);
    (q(y) << 22) | (q(x) << 12)
}

/// The bytes of a compiled `.vmdl_c`.
pub fn model(m: &SynthModel) -> Vec<u8> {
    let mut blocks: Vec<([u8; 4], Vec<u8>)> = Vec::new();
    let mut embedded = Vec::new();
    for (mi, mesh) in m.meshes.iter().enumerate() {
        // One vertex buffer and one index buffer per mesh; draws are ranges in them.
        let stride = 28usize;
        let mut vdata = Vec::new();
        let mut idata: Vec<u32> = Vec::new();
        let mut draws = Vec::new();
        let mut vcount = 0u32;
        for d in &mesh.draws {
            let start_index = idata.len() as i64;
            for (k, p) in d.positions.iter().enumerate() {
                for c in p {
                    vdata.extend_from_slice(&c.to_le_bytes());
                }
                let uv = d.uvs.get(k).copied().unwrap_or([0.0, 0.0]);
                for c in uv {
                    vdata.extend_from_slice(&((c.clamp(-1.0, 1.0) * 32767.0).round() as i16).to_le_bytes());
                }
                let n = d.normals.get(k).copied().unwrap_or([0.0, 0.0, 1.0]);
                vdata.extend_from_slice(&pack_normal(n).to_le_bytes());
                vdata.extend_from_slice(&d.joints.get(k).copied().unwrap_or([0, 0, 0, 0]));
                let w = d.weights.get(k).copied().unwrap_or([1.0, 0.0, 0.0, 0.0]);
                for c in w {
                    vdata.push((c.clamp(0.0, 1.0) * 255.0).round() as u8);
                }
            }
            for t in &d.tris {
                idata.extend(t.iter().map(|&x| x + vcount));
            }
            draws.push(obj(vec![
                ("m_nPrimitiveType", s("RENDER_PRIM_TRIANGLES")),
                ("m_nBaseVertex", i(0)),
                ("m_nVertexCount", i((vcount + d.positions.len() as u32) as i64)),
                ("m_nStartIndex", i(start_index)),
                ("m_nIndexCount", i((d.tris.len() * 3) as i64)),
                ("m_nAppliedIndexOffset", i(vcount as i64)),
                ("m_indexBuffer", obj(vec![("m_hBuffer", i(0)), ("m_nBindOffsetBytes", i(0))])),
                ("m_material", s(&d.material)),
                ("m_vertexBuffers", V::Array(vec![obj(vec![("m_hBuffer", i(0)), ("m_nBindOffsetBytes", i(0))])])),
                ("m_flUvDensity", f(0.0)),
                ("m_flAlpha", f(1.0)),
            ]));
            vcount += d.positions.len() as u32;
        }
        let vblock = blocks.len();
        let venc = if m.raw { vdata.clone() } else { crate::meshopt::encode_vertex_buffer_v0(&vdata, vcount as usize, stride) };
        blocks.push((*b"MVTX", venc));
        let iblock = blocks.len();
        let ienc = if m.raw {
            idata.iter().flat_map(|x| x.to_le_bytes()).collect()
        } else {
            crate::meshopt::encode_index_buffer_explicit(&idata)
        };
        blocks.push((*b"MIDX", ienc));
        let bones: Vec<V> = mesh
            .bones
            .iter()
            .map(|(n, p)| {
                obj(vec![
                    ("m_boneName", s(n)),
                    ("m_parentName", s("")),
                    ("m_invBindPose", V::Array([1.0, 0.0, 0.0, -p[0], 0.0, 1.0, 0.0, -p[1], 0.0, 0.0, 1.0, -p[2]].iter().map(|&x| f(x as f64)).collect())),
                ])
            })
            .collect();
        let mut mdat_fields = vec![
            ("_class", s("CRenderMesh")),
            ("m_sceneObjects", V::Array(vec![obj(vec![("m_drawCalls", V::Array(draws))])])),
            ("m_skeleton", obj(vec![("m_bones", V::Array(bones)), ("m_nBoneWeightCount", i(4))])),
        ];
        if mi == 0 && !m.hitboxes.is_empty() {
            let v3 = |p: [f32; 3]| V::Array(p.iter().map(|&c| f(c as f64)).collect());
            let hbs: Vec<V> = m
                .hitboxes
                .iter()
                .enumerate()
                .map(|(k, (n, b, a, z, r))| obj(vec![("m_name", s(n)), ("m_sBoneName", s(b)), ("m_vMinBounds", v3(*a)), ("m_vMaxBounds", v3(*z)), ("m_flShapeRadius", f(*r as f64)), ("m_nShapeType", i(m.hitbox_shapes.get(k).copied().unwrap_or(2))), ("m_nGroupId", i(1))]))
                .collect();
            mdat_fields.push(("m_hitboxsets", V::Array(vec![obj(vec![("key", s("default")), ("value", obj(vec![("m_name", s("default")), ("m_HitBoxes", V::Array(hbs))]))])])));
        }
        let mdat = obj(mdat_fields);
        let dblock = blocks.len();
        blocks.push((*b"MDAT", resource::kv3::encode(&mdat)));
        let compressed = !m.raw;
        embedded.push(obj(vec![
            ("m_Name", s(&mesh.name)),
            ("m_nMeshIndex", i(mi as i64)),
            ("m_nDataBlock", i(dblock as i64)),
            ("m_nMorphBlock", i(-1)),
            (
                "m_vertexBuffers",
                V::Array(vec![obj(vec![
                    ("m_nBlockIndex", i(vblock as i64)),
                    ("m_nElementCount", i(vcount as i64)),
                    ("m_nElementSizeInBytes", i(stride as i64)),
                    ("m_bMeshoptCompressed", b(compressed)),
                    ("m_bMeshoptIndexSequence", b(false)),
                    ("m_bCompressedZSTD", b(false)),
                    (
                        "m_inputLayoutFields",
                        V::Array(
                            [field("POSITION", 6, 0), field("TEXCOORD", 37, 12), field("NORMAL", 42, 16), field("BLENDINDICES", 30, 20), field("BLENDWEIGHT", 28, 24)]
                                .into_iter()
                                .enumerate()
                                .filter(|(k, _)| !(mesh.no_uv && *k == 1))
                                .map(|(_, x)| x)
                                .collect(),
                        ),
                    ),
                ])]),
            ),
            (
                "m_indexBuffers",
                V::Array(vec![obj(vec![
                    ("m_nBlockIndex", i(iblock as i64)),
                    ("m_nElementCount", i(idata.len() as i64)),
                    ("m_nElementSizeInBytes", i(4)),
                    ("m_bMeshoptCompressed", b(compressed)),
                    ("m_bMeshoptIndexSequence", b(false)),
                    ("m_bCompressedZSTD", b(false)),
                ])]),
            ),
        ]));
    }
    blocks.push((*b"CTRL", resource::kv3::encode(&obj(vec![("embedded_meshes", V::Array(embedded))]))));
    let mats: Vec<V> = {
        let mut seen = Vec::new();
        for mesh in &m.meshes {
            for d in &mesh.draws {
                if !seen.contains(&d.material) {
                    seen.push(d.material.clone());
                }
            }
        }
        for x in &m.extra_materials {
            seen.push(x.clone());
        }
        seen.into_iter().map(|x| s(&x)).collect()
    };
    let mut groups = Vec::new();
    if !m.mesh_group_masks.is_empty() {
        groups.push(("m_refMeshGroupMasks", V::Array(m.meshes.iter().enumerate().map(|(k, _)| V::UInt(m.mesh_group_masks.get(k).copied().unwrap_or(1))).collect())));
        groups.push(("m_nDefaultMeshGroupMask", V::UInt(1)));
    }
    let mut data = vec![
        ("m_name", s(&m.name)),
        ("m_refLODGroupMasks", V::Array(m.meshes.iter().map(|x| V::UInt(x.lod_mask)).collect())),
        ("m_lodGroupSwitchDistances", V::Array(vec![f(0.0), f(14.0), f(35.0)])),
        ("m_materialGroups", V::Array(vec![obj(vec![("m_name", s("default")), ("m_materials", V::Array(mats))])])),
        (
            "m_modelSkeleton",
            obj(vec![
                ("m_boneName", V::Array(m.skeleton.iter().map(|x| s(&x.0)).collect())),
                ("m_nParent", V::Array(m.skeleton.iter().map(|x| i(x.1 as i64)).collect())),
                ("m_bonePosParent", V::Array(m.skeleton.iter().map(|x| V::Array(x.2.iter().map(|&c| f(c as f64)).collect())).collect())),
                ("m_boneRotParent", V::Array(m.skeleton.iter().map(|_| V::Array(vec![f(0.0), f(0.0), f(0.0), f(1.0)])).collect())),
            ]),
        ),
    ];
    data.extend(groups);
    let data = obj(data);
    blocks.push((*b"DATA", resource::kv3::encode(&data)));
    blocks.extend(m.extra_blocks.iter().cloned());
    synth_blocks(&blocks)
}

/// A resource whose block indices are exactly the order given (CTRL's block indices rely on it).
fn synth_blocks(blocks: &[([u8; 4], Vec<u8>)]) -> Vec<u8> {
    let n = blocks.len();
    let table = 16usize;
    let mut out = vec![0u8; table + 12 * n];
    let mut entries = Vec::new();
    for (tag, data) in blocks {
        while !out.len().is_multiple_of(16) {
            out.push(0);
        }
        entries.push((*tag, out.len(), data.len()));
        out.extend_from_slice(data);
    }
    let total = out.len();
    out[0..4].copy_from_slice(&(total as u32).to_le_bytes());
    out[4..6].copy_from_slice(&resource::KNOWN_HEADER_VERSION.to_le_bytes());
    out[8..12].copy_from_slice(&((table - 8) as u32).to_le_bytes());
    out[12..16].copy_from_slice(&(n as u32).to_le_bytes());
    for (k, (tag, at, size)) in entries.iter().enumerate() {
        let e = table + k * 12;
        out[e..e + 4].copy_from_slice(tag);
        out[e + 4..e + 8].copy_from_slice(&((at - (e + 4)) as u32).to_le_bytes());
        out[e + 8..e + 12].copy_from_slice(&(*size as u32).to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quad(material: &str, z: f32) -> SynthDraw {
        SynthDraw {
            material: material.into(),
            positions: vec![[0.0, 0.0, z], [1.0, 0.0, z], [1.0, 1.0, z], [0.0, 1.0, z]],
            normals: vec![[0.0, 0.0, 1.0]; 4],
            uvs: vec![[0.0, 0.0], [0.5, 0.0], [0.5, 0.5], [0.0, 0.5]],
            joints: vec![[1, 0, 0, 0]; 4],
            weights: vec![[1.0, 0.0, 0.0, 0.0]; 4],
            tris: vec![[0, 1, 2], [0, 2, 3]],
        }
    }

    fn sample(raw: bool) -> SynthModel {
        SynthModel {
            name: "models/test/x.vmdl".into(),
            meshes: vec![
                SynthMesh { name: "body".into(), lod_mask: 1, draws: vec![quad("materials/a.vmat", 0.0), quad("materials/b.vmat", 2.0)], bones: vec![("root".into(), [0.0; 3]), ("spine".into(), [0.0, 0.0, 1.0])], no_uv: false },
                SynthMesh { name: "body_lod1".into(), lod_mask: 2, draws: vec![quad("materials/a.vmat", 0.0)], bones: vec![("root".into(), [0.0; 3])], no_uv: false },
            ],
            skeleton: vec![("root".into(), -1, [0.0; 3]), ("spine".into(), 0, [0.0, 0.0, 1.0])],
            raw,
            ..Default::default()
        }
    }

    #[test]
    fn synth_round_trips_through_the_reader() {
        for raw in [false, true] {
            let bytes = model(&sample(raw));
            let m = crate::parse(&bytes).unwrap();
            assert!(m.notes.is_empty(), "{:?}", m.notes);
            assert_eq!(m.meshes.len(), 2);
            assert_eq!(m.lod_count(), 2);
            let (lod0, notes) = m.surfaces(0);
            assert!(notes.is_empty(), "{notes:?}");
            assert_eq!(lod0.len(), 2);
            assert_eq!(lod0[1].material, "materials/b.vmat");
            assert_eq!(lod0[1].positions[2], [1.0, 1.0, 2.0]);
            assert_eq!(lod0[1].tris, vec![[0, 1, 2], [0, 2, 3]]);
            assert!((lod0[0].normals[0][2] - 1.0).abs() < 1e-3);
            assert!((lod0[0].uvs[1][0] - 0.5).abs() < 1e-4);
            // blend index 1 in the mesh's bone list is the model's "spine"
            assert_eq!(lod0[0].joints[0][0], 1);
            assert_eq!(m.surfaces(1).0.len(), 1);
            assert_eq!(m.skeleton.world()[1].1, [0.0, 0.0, 1.0]);
        }
    }

    #[test]
    fn packed_normals_survive() {
        for n in [[0.0, 0.0, 1.0], [0.0, 0.0, -1.0], [0.6, 0.0, 0.8], [-0.36, 0.48, -0.8]] {
            let (u, _) = crate::format::unpack_tangent_frame(pack_normal(n));
            let d = u[0] * n[0] + u[1] * n[1] + u[2] * n[2];
            assert!(d > 0.998, "{n:?} -> {u:?}");
        }
    }
}
