//! A compiled model (`.vmdl_c`) read down to its triangles.
//!
//! Layout (build 6864): the `CTRL` block lists `embedded_meshes`, each naming its `MDAT` block
//! (`m_nDataBlock`), its vertex buffers (`m_vertexBuffers`, each an `MVTX` block index with element
//! count, stride, `m_inputLayoutFields` and the meshopt/zstd flags) and index buffers
//! (`m_indexBuffers`, `MIDX` blocks). Each `MDAT` is a `CRenderMesh`: `m_sceneObjects[].m_drawCalls[]`
//! (material, index range, base vertex, the buffers it binds), `m_skeleton.m_bones` (the bones the
//! mesh's BLENDINDICES count in, with inverse bind poses), attachments and hitbox sets. The model's
//! `DATA` block carries the skeleton (`m_modelSkeleton`), which meshes belong to which LOD
//! (`m_refLODGroupMasks`, bit n = LOD n, indexed by `m_nMeshIndex`), the LOD switch distances and the
//! material groups (skins).
//!
//! Buffers flagged `m_bCompressedZSTD` (Valve's vertex-colour streams) are not decoded: Magician has
//! no zstd decoder. They never carry positions, normals, UVs or skin data on 6864's heroes, and the
//! stream is reported as skipped, never as empty.

use crate::{format, meshopt};
use resource::kv3::Value;

#[derive(Debug, Clone, PartialEq)]
pub struct Attribute {
    /// Upper-cased semantic (`POSITION`, `TEXCOORD`, `NORMAL`, `TANGENT`, `BLENDINDICES`, `BLENDWEIGHT`, `COLOR`).
    pub semantic: String,
    pub index: u32,
    pub format: u32,
    pub offset: u32,
    pub shader_semantic: String,
}

#[derive(Debug, Clone)]
pub struct VertexBuffer {
    pub block: usize,
    pub count: usize,
    pub stride: usize,
    pub attrs: Vec<Attribute>,
    /// Decoded bytes (`count * stride`), or why not.
    pub data: Result<Vec<u8>, String>,
    pub encoded_bytes: usize,
}

#[derive(Debug, Clone)]
pub struct IndexBuffer {
    pub block: usize,
    pub count: usize,
    pub size: usize,
    pub data: Result<Vec<u32>, String>,
}

#[derive(Debug, Clone)]
pub struct Draw {
    pub material: String,
    pub primitive: String,
    pub base_vertex: u32,
    pub vertex_count: u32,
    pub start_index: u32,
    pub index_count: u32,
    pub applied_index_offset: u32,
    pub index_buffer: usize,
    /// (vertex buffer index within the mesh, bind offset in bytes)
    pub vertex_buffers: Vec<(usize, u32)>,
    pub uv_density: f64,
    pub alpha: f64,
}

#[derive(Debug, Clone)]
pub struct MeshBone {
    pub name: String,
    /// Inverse bind pose, 3x4 row-major (model space to bone space).
    pub inv_bind: [f32; 12],
}

#[derive(Debug, Clone)]
pub struct Mesh {
    pub name: String,
    pub mesh_index: usize,
    pub data_block: usize,
    pub lod_mask: u64,
    /// The body-group choices that draw this mesh (`m_refMeshGroupMasks`, one bit per entry of `m_meshGroups`);
    /// every bit when the model records none.
    pub group_mask: u64,
    pub vbs: Vec<VertexBuffer>,
    pub ibs: Vec<IndexBuffer>,
    pub draws: Vec<Draw>,
    pub bones: Vec<MeshBone>,
    pub bone_weight_count: u32,
    pub morph: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Skeleton {
    pub names: Vec<String>,
    pub parents: Vec<i32>,
    pub pos: Vec<[f32; 3]>,
    pub rot: Vec<[f32; 4]>,
}

impl Skeleton {
    pub fn index(&self, name: &str) -> Option<usize> {
        self.names.iter().position(|n| n == name)
    }
    /// Model-space bind transforms (rotation as a 3x3 row-major matrix, translation).
    pub fn world(&self) -> Vec<([[f32; 3]; 3], [f32; 3])> {
        let mut out: Vec<([[f32; 3]; 3], [f32; 3])> = Vec::with_capacity(self.names.len());
        for i in 0..self.names.len() {
            let r = quat_mat(self.rot.get(i).copied().unwrap_or([0.0, 0.0, 0.0, 1.0]));
            let t = self.pos.get(i).copied().unwrap_or([0.0; 3]);
            let p = self.parents.get(i).copied().unwrap_or(-1);
            if p >= 0 && (p as usize) < out.len() {
                let (pr, pt) = out[p as usize];
                let mut m = [[0.0f32; 3]; 3];
                for a in 0..3 {
                    for b in 0..3 {
                        m[a][b] = (0..3).map(|k| pr[a][k] * r[k][b]).sum();
                    }
                }
                let wt = [
                    pt[0] + pr[0][0] * t[0] + pr[0][1] * t[1] + pr[0][2] * t[2],
                    pt[1] + pr[1][0] * t[0] + pr[1][1] * t[1] + pr[1][2] * t[2],
                    pt[2] + pr[2][0] * t[0] + pr[2][1] * t[1] + pr[2][2] * t[2],
                ];
                out.push((m, wt));
            } else {
                out.push((r, t));
            }
        }
        out
    }
}

fn quat_mat(q: [f32; 4]) -> [[f32; 3]; 3] {
    let [x, y, z, w] = q;
    [
        [1.0 - 2.0 * (y * y + z * z), 2.0 * (x * y - z * w), 2.0 * (x * z + y * w)],
        [2.0 * (x * y + z * w), 1.0 - 2.0 * (x * x + z * z), 2.0 * (y * z - x * w)],
        [2.0 * (x * z - y * w), 2.0 * (y * z + x * w), 1.0 - 2.0 * (x * x + y * y)],
    ]
}

#[derive(Debug, Clone)]
pub struct Hitbox {
    pub set: String,
    pub name: String,
    pub bone: String,
    pub min: [f32; 3],
    pub max: [f32; 3],
    pub radius: f32,
    pub shape: i64,
    pub group: i64,
}

#[derive(Debug, Clone, Default)]
pub struct Model {
    pub name: String,
    pub meshes: Vec<Mesh>,
    pub skeleton: Skeleton,
    pub hitboxes: Vec<Hitbox>,
    pub lod_switch: Vec<f32>,
    /// The body-group choices (`m_meshGroups`: Viscous has `default_@0_#&body` and `default_@1_#&inflated`, the
    /// standing hero and the Goo Ball) and the ones drawn by default (`m_nDefaultMeshGroupMask`).
    pub mesh_groups: Vec<String>,
    pub default_group_mask: u64,
    /// Material groups (skins): name, materials.
    pub material_groups: Vec<(String, Vec<String>)>,
    /// What could not be read, by buffer or block (never silent).
    pub notes: Vec<String>,
}

fn num(v: Option<&Value>) -> Option<f64> {
    v.and_then(Value::as_f64)
}
fn int(v: &Value, k: &str) -> i64 {
    num(v.get(k)).unwrap_or(0.0) as i64
}
fn text(v: &Value, k: &str) -> String {
    v.get(k).and_then(Value::as_str).unwrap_or("").to_string()
}
fn vec3(v: Option<&Value>) -> [f32; 3] {
    let n = v.map(Value::numbers).unwrap_or_default();
    [n.first().copied().unwrap_or(0.0) as f32, n.get(1).copied().unwrap_or(0.0) as f32, n.get(2).copied().unwrap_or(0.0) as f32]
}

fn block_kv(data: &[u8], r: &resource::Resource, i: usize) -> Option<Value> {
    let b = r.blocks.get(i)?;
    let bytes = resource::block_bytes(data, b);
    if !resource::kv3::is_kv3(bytes) {
        return None;
    }
    resource::kv3::decode(bytes).ok()
}

/// Parse a compiled model. Buffers that fail to decode are kept with their error; the model itself
/// fails only when it has no readable `CTRL` block.
pub fn parse(data: &[u8]) -> Result<Model, String> {
    let r = resource::parse(data).map_err(|e| e.to_string())?;
    let ctrl_i = r.blocks.iter().position(|b| &b.tag == b"CTRL").ok_or("no CTRL block (not a model with embedded meshes)")?;
    let ctrl = block_kv(data, &r, ctrl_i).ok_or("CTRL block is not KV3")?;
    let model_data = r.blocks.iter().position(|b| &b.tag == b"DATA").and_then(|i| block_kv(data, &r, i));
    let mut m = Model { default_group_mask: u64::MAX, ..Default::default() };
    let masks = |key: &str| -> Vec<u64> {
        model_data
            .as_ref()
            .and_then(|d| d.get(key))
            .map(|v| v.as_array().iter().map(|x| match x {
                Value::UInt(u) => *u,
                other => other.as_f64().unwrap_or(1.0) as u64,
            }).collect())
            .unwrap_or_default()
    };
    let lod_masks = masks("m_refLODGroupMasks");
    let group_masks = masks("m_refMeshGroupMasks");
    if let Some(d) = &model_data {
        m.name = text(d, "m_name");
        m.mesh_groups = d.get("m_meshGroups").map(Value::as_array).unwrap_or(&[]).iter().filter_map(Value::as_str).map(str::to_string).collect();
        if let Some(mask) = d.get("m_nDefaultMeshGroupMask") {
            m.default_group_mask = match mask {
                Value::UInt(u) => *u,
                other => other.as_f64().unwrap_or(-1.0) as i64 as u64,
            };
        }
        m.lod_switch = d.get("m_lodGroupSwitchDistances").map(Value::numbers).unwrap_or_default().iter().map(|&x| x as f32).collect();
        for g in d.get("m_materialGroups").map(Value::as_array).unwrap_or(&[]) {
            let mats = g.get("m_materials").map(Value::as_array).unwrap_or(&[]).iter().filter_map(Value::as_str).map(str::to_string).collect();
            m.material_groups.push((text(g, "m_name"), mats));
        }
        if let Some(s) = d.get("m_modelSkeleton") {
            m.skeleton.names = s.get("m_boneName").map(Value::as_array).unwrap_or(&[]).iter().filter_map(Value::as_str).map(str::to_string).collect();
            m.skeleton.parents = s.get("m_nParent").map(Value::numbers).unwrap_or_default().iter().map(|&x| x as i32).collect();
            m.skeleton.pos = s.get("m_bonePosParent").map(Value::as_array).unwrap_or(&[]).iter().map(|v| vec3(Some(v))).collect();
            m.skeleton.rot = s
                .get("m_boneRotParent")
                .map(Value::as_array)
                .unwrap_or(&[])
                .iter()
                .map(|v| {
                    let n = v.numbers();
                    [n.first().copied().unwrap_or(0.0) as f32, n.get(1).copied().unwrap_or(0.0) as f32, n.get(2).copied().unwrap_or(0.0) as f32, n.get(3).copied().unwrap_or(1.0) as f32]
                })
                .collect();
        }
    } else {
        m.notes.push("model DATA block missing or unreadable: no skeleton, LOD masks or material groups".into());
    }

    for em in ctrl.get("embedded_meshes").map(Value::as_array).unwrap_or(&[]) {
        let name = text(em, "m_Name");
        let mesh_index = int(em, "m_nMeshIndex").max(0) as usize;
        let data_block = int(em, "m_nDataBlock").max(0) as usize;
        let lod_mask = lod_masks.get(mesh_index).copied().unwrap_or(1);
        let mut mesh = Mesh {
            name: name.clone(),
            mesh_index,
            data_block,
            lod_mask,
            group_mask: group_masks.get(mesh_index).copied().unwrap_or(u64::MAX),
            vbs: vec![],
            ibs: vec![],
            draws: vec![],
            bones: vec![],
            bone_weight_count: 0,
            morph: int(em, "m_nMorphBlock") >= 0,
        };
        for vb in em.get("m_vertexBuffers").map(Value::as_array).unwrap_or(&[]) {
            let block = int(vb, "m_nBlockIndex").max(0) as usize;
            let count = int(vb, "m_nElementCount").max(0) as usize;
            let stride = int(vb, "m_nElementSizeInBytes").max(0) as usize;
            let attrs = vb
                .get("m_inputLayoutFields")
                .map(Value::as_array)
                .unwrap_or(&[])
                .iter()
                .map(|a| Attribute {
                    semantic: text(a, "m_pSemanticName").to_ascii_uppercase(),
                    index: int(a, "m_nSemanticIndex") as u32,
                    format: int(a, "m_Format") as u32,
                    offset: int(a, "m_nOffset") as u32,
                    shader_semantic: text(a, "m_szShaderSemantic"),
                })
                .collect();
            let raw = r.blocks.get(block).map(|b| resource::block_bytes(data, b)).unwrap_or(&[]);
            let decoded = if vb.get("m_bCompressedZSTD").and_then(Value::as_bool).unwrap_or(false) {
                Err("zstd-compressed stream (not decoded)".to_string())
            } else if vb.get("m_bMeshoptCompressed").and_then(Value::as_bool).unwrap_or(false) {
                meshopt::decode_vertex_buffer(count, stride, raw)
            } else if count.checked_mul(stride).is_some_and(|n| raw.len() >= n) {
                Ok(raw[..count * stride].to_vec())
            } else {
                Err(format!("raw buffer holds {} bytes, needs {count} x {stride}", raw.len()))
            };
            if let Err(e) = &decoded {
                m.notes.push(format!("{name}: vertex buffer (block {block}): {e}"));
            }
            mesh.vbs.push(VertexBuffer { block, count, stride, attrs, data: decoded, encoded_bytes: raw.len() });
        }
        for ib in em.get("m_indexBuffers").map(Value::as_array).unwrap_or(&[]) {
            let block = int(ib, "m_nBlockIndex").max(0) as usize;
            let count = int(ib, "m_nElementCount").max(0) as usize;
            let size = int(ib, "m_nElementSizeInBytes").max(0) as usize;
            let raw = r.blocks.get(block).map(|b| resource::block_bytes(data, b)).unwrap_or(&[]);
            let decoded = if ib.get("m_bMeshoptCompressed").and_then(Value::as_bool).unwrap_or(false) {
                if ib.get("m_bMeshoptIndexSequence").and_then(Value::as_bool).unwrap_or(false) {
                    meshopt::decode_index_sequence(count, raw)
                } else {
                    meshopt::decode_index_buffer(count, raw)
                }
            } else if (size == 2 || size == 4) && count.checked_mul(size).is_some_and(|n| raw.len() >= n) {
                Ok(raw[..count * size]
                    .chunks(size)
                    .map(|c| if size == 2 { u16::from_le_bytes([c[0], c[1]]) as u32 } else { u32::from_le_bytes([c[0], c[1], c[2], c[3]]) })
                    .collect())
            } else {
                Err(format!("raw index buffer holds {} bytes, needs {count} at {size} per index", raw.len()))
            };
            if let Err(e) = &decoded {
                m.notes.push(format!("{name}: index buffer (block {block}): {e}"));
            }
            mesh.ibs.push(IndexBuffer { block, count, size, data: decoded });
        }
        match block_kv(data, &r, data_block) {
            Some(md) => {
                for so in md.get("m_sceneObjects").map(Value::as_array).unwrap_or(&[]) {
                    for dc in so.get("m_drawCalls").map(Value::as_array).unwrap_or(&[]) {
                        let vbs = dc
                            .get("m_vertexBuffers")
                            .map(Value::as_array)
                            .unwrap_or(&[])
                            .iter()
                            .map(|v| (int(v, "m_hBuffer").max(0) as usize, int(v, "m_nBindOffsetBytes").max(0) as u32))
                            .collect();
                        mesh.draws.push(Draw {
                            material: text(dc, "m_material").to_ascii_lowercase(),
                            primitive: text(dc, "m_nPrimitiveType"),
                            base_vertex: int(dc, "m_nBaseVertex").max(0) as u32,
                            vertex_count: int(dc, "m_nVertexCount").max(0) as u32,
                            start_index: int(dc, "m_nStartIndex").max(0) as u32,
                            index_count: int(dc, "m_nIndexCount").max(0) as u32,
                            applied_index_offset: int(dc, "m_nAppliedIndexOffset").max(0) as u32,
                            index_buffer: dc.get("m_indexBuffer").map(|v| int(v, "m_hBuffer").max(0) as usize).unwrap_or(0),
                            vertex_buffers: vbs,
                            uv_density: num(dc.get("m_flUvDensity")).unwrap_or(0.0),
                            alpha: num(dc.get("m_flAlpha")).unwrap_or(1.0),
                        });
                    }
                }
                if let Some(sk) = md.get("m_skeleton") {
                    mesh.bone_weight_count = int(sk, "m_nBoneWeightCount") as u32;
                    for b in sk.get("m_bones").map(Value::as_array).unwrap_or(&[]) {
                        let n = b.get("m_invBindPose").map(Value::numbers).unwrap_or_default();
                        let mut inv = [0f32; 12];
                        for (i, x) in n.iter().take(12).enumerate() {
                            inv[i] = *x as f32;
                        }
                        mesh.bones.push(MeshBone { name: text(b, "m_boneName"), inv_bind: inv });
                    }
                }
                if m.hitboxes.is_empty() {
                    for set in md.get("m_hitboxsets").map(Value::as_array).unwrap_or(&[]) {
                        let set_name = text(set, "key");
                        let Some(v) = set.get("value") else { continue };
                        for h in v.get("m_HitBoxes").map(Value::as_array).unwrap_or(&[]) {
                            m.hitboxes.push(Hitbox {
                                set: set_name.clone(),
                                name: text(h, "m_name"),
                                bone: text(h, "m_sBoneName"),
                                min: vec3(h.get("m_vMinBounds")),
                                max: vec3(h.get("m_vMaxBounds")),
                                radius: num(h.get("m_flShapeRadius")).unwrap_or(0.0) as f32,
                                shape: int(h, "m_nShapeType"),
                                group: int(h, "m_nGroupId"),
                            });
                        }
                    }
                }
            }
            None => m.notes.push(format!("{name}: MDAT block {data_block} unreadable")),
        }
        m.meshes.push(mesh);
    }
    Ok(m)
}

/// One draw call's geometry, gathered from its buffers: the vertices its triangles use (in
/// first-use order) and the triangles in that local numbering. Joints are model skeleton indices.
#[derive(Debug, Clone, Default)]
pub struct Surface {
    pub mesh: String,
    pub mesh_index: usize,
    pub draw: usize,
    pub material: String,
    pub lod_mask: u64,
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub tangents: Vec<[f32; 4]>,
    pub uvs: Vec<[f32; 2]>,
    /// Up to 8 influences: model bone index (u16::MAX = none) and weight.
    pub joints: Vec<[u16; 8]>,
    pub weights: Vec<[f32; 8]>,
    pub tris: Vec<[u32; 3]>,
    /// The buffer-space vertex index of each local vertex (for de-duplication across draws).
    pub source: Vec<u32>,
    /// Attributes present: position, normal, uv, skin.
    pub has_normal: bool,
    pub has_uv: bool,
    pub has_skin: bool,
    pub skin_weights_implicit: bool,
}

impl Model {
    /// Whether a mesh is drawn with the model's default body-group choices. A model shows one choice of each body
    /// group at a time, so meshes of another choice (Viscous's Goo Ball beside the standing hero) are never on screen
    /// together with these, nor with the default hitbox set. A default mask that selects no mesh is ignored.
    pub fn drawn_by_default(&self, mesh: &Mesh) -> bool {
        let any = self.meshes.iter().any(|m| m.group_mask & self.default_group_mask != 0);
        !any || mesh.group_mask & self.default_group_mask != 0
    }

    /// The meshes left out by `drawn_by_default`, by name, each once.
    pub fn other_choice_meshes(&self) -> Vec<&str> {
        let mut out: Vec<&str> = Vec::new();
        for m in self.meshes.iter().filter(|m| !self.drawn_by_default(m)) {
            if !out.contains(&m.name.as_str()) {
                out.push(&m.name);
            }
        }
        out
    }

    /// Every draw call of the default body-group choice's meshes whose LOD mask has bit `lod`, as surfaces. Errors
    /// are per draw and carried in `notes` of the returned tuple.
    pub fn surfaces(&self, lod: u32) -> (Vec<Surface>, Vec<String>) {
        let mut out = Vec::new();
        let mut notes = Vec::new();
        let model_index = |name: &str| self.skeleton.index(name);
        for mesh in &self.meshes {
            if mesh.lod_mask & (1u64 << lod) == 0 || !self.drawn_by_default(mesh) {
                continue;
            }
            // Mesh-local bone index -> model bone index.
            let remap: Vec<u16> = mesh.bones.iter().map(|b| model_index(&b.name).map(|i| i as u16).unwrap_or(u16::MAX)).collect();
            for (di, d) in mesh.draws.iter().enumerate() {
                match surface(mesh, di, d, &remap) {
                    Ok(s) => out.push(s),
                    Err(e) => notes.push(format!("{} draw {di} ({}): {e}", mesh.name, d.material)),
                }
            }
        }
        (out, notes)
    }

    /// LOD count: highest set bit over all meshes' masks, plus one.
    pub fn lod_count(&self) -> u32 {
        let all = self.meshes.iter().fold(0u64, |a, m| a | m.lod_mask);
        64 - all.leading_zeros()
    }
}

fn surface(mesh: &Mesh, di: usize, d: &Draw, remap: &[u16]) -> Result<Surface, String> {
    if !d.primitive.is_empty() && d.primitive != "RENDER_PRIM_TRIANGLES" {
        return Err(format!("primitive {} not read", d.primitive));
    }
    let ib = mesh.ibs.get(d.index_buffer).ok_or("index buffer out of range")?;
    let idx = ib.data.as_ref().map_err(|e| e.clone())?;
    let start = d.start_index as usize;
    let end = start + d.index_count as usize;
    if end > idx.len() {
        return Err(format!("index range {start}..{end} past {} indices", idx.len()));
    }
    let &(vbi, bind) = d.vertex_buffers.first().ok_or("no vertex buffer bound")?;
    let vb = mesh.vbs.get(vbi).ok_or("vertex buffer out of range")?;
    let bytes = vb.data.as_ref().map_err(|e| e.clone())?;
    let bind_v = (bind as usize).checked_div(vb.stride).unwrap_or(0);
    let find = |sem: &str, i: u32| vb.attrs.iter().find(|a| a.semantic == sem && a.index == i);
    let pos = find("POSITION", 0).ok_or("no POSITION attribute")?;
    let nrm = find("NORMAL", 0);
    let tan = find("TANGENT", 0);
    let uv = find("TEXCOORD", 0);
    let bi0 = find("BLENDINDICES", 0);
    let bi1 = find("BLENDINDICES", 1);
    let bw0 = find("BLENDWEIGHT", 0).or_else(|| find("BLENDWEIGHTS", 0));
    let bw1 = find("BLENDWEIGHT", 1).or_else(|| find("BLENDWEIGHTS", 1));
    let mut s = Surface {
        mesh: mesh.name.clone(),
        mesh_index: mesh.mesh_index,
        draw: di,
        material: d.material.clone(),
        lod_mask: mesh.lod_mask,
        has_normal: nrm.is_some(),
        has_uv: uv.is_some(),
        has_skin: bi0.is_some(),
        skin_weights_implicit: bi0.is_some() && bw0.is_none(),
        ..Default::default()
    };
    let mut local: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
    let read = |a: &Attribute, v: usize| -> Option<[f32; 4]> {
        let at = v * vb.stride + a.offset as usize;
        format::decode(a.format, bytes.get(at..at + format::size_of(a.format)?)?)
    };
    let mut tri = [0u32; 3];
    for (k, &raw) in idx[start..end].iter().enumerate() {
        let v = raw as usize + d.base_vertex as usize + bind_v;
        if v >= vb.count {
            return Err(format!("index {raw} (+ base {}) past {} vertices", d.base_vertex, vb.count));
        }
        let li = match local.get(&(v as u32)) {
            Some(&li) => li,
            None => {
                let li = s.positions.len() as u32;
                local.insert(v as u32, li);
                let p = read(pos, v).ok_or_else(|| format!("POSITION format {} not read", pos.format))?;
                s.positions.push([p[0], p[1], p[2]]);
                s.source.push(v as u32);
                if let Some(a) = nrm {
                    let (n, t) = normal_of(a, read(a, v).unwrap_or([0.0; 4]), bytes, v, vb.stride);
                    s.normals.push(n);
                    if let Some(t) = t {
                        s.tangents.push(t);
                    }
                }
                if let (Some(a), true) = (tan, s.tangents.len() < s.positions.len()) {
                    let t = read(a, v).unwrap_or([0.0; 4]);
                    s.tangents.push(t);
                }
                if let Some(a) = uv {
                    let t = read(a, v).unwrap_or([0.0; 4]);
                    s.uvs.push([t[0], t[1]]);
                }
                if let Some(a) = bi0 {
                    let mut j = [u16::MAX; 8];
                    let mut w = [0f32; 8];
                    let raw = |a: &Attribute| bytes.get(v * vb.stride + a.offset as usize..).unwrap_or(&[]);
                    let (i0, n0) = format::blend_indices(a.format, raw(a)).unwrap_or(([0; 8], 4));
                    let w0 = bw0.and_then(|a| format::blend_weights(a.format, raw(a)));
                    for k in 0..n0 {
                        j[k] = remap.get(i0[k] as usize).copied().unwrap_or(u16::MAX);
                        w[k] = match w0 {
                            Some(w0) => w0[k],
                            None => if k == 0 { 1.0 } else { 0.0 },
                        };
                    }
                    // a second stream carries influences 5 to 8 when the first holds four
                    if let (Some(b), true) = (bi1, n0 <= 4) {
                        let (i1, n1) = format::blend_indices(b.format, raw(b)).unwrap_or(([0; 8], 4));
                        let w1 = bw1.and_then(|a| format::blend_weights(a.format, raw(a)));
                        for k in 0..n1.min(4) {
                            j[4 + k] = remap.get(i1[k] as usize).copied().unwrap_or(u16::MAX);
                            w[4 + k] = w1.map(|w1| w1[k]).unwrap_or(0.0);
                        }
                    }
                    s.joints.push(j);
                    s.weights.push(w);
                }
                li
            }
        };
        tri[k % 3] = li;
        if k % 3 == 2 {
            s.tris.push(tri);
        }
    }
    Ok(s)
}

/// The four bytes of a packed normal: the attribute's offset is the model's word, so past the buffer there is none.
pub(crate) fn packed4(a: &Attribute, bytes: &[u8], vi: usize, stride: usize) -> Option<[u8; 4]> {
    let at = vi.checked_mul(stride)?.checked_add(a.offset as usize)?;
    bytes.get(at..at.checked_add(4)?).map(|b| [b[0], b[1], b[2], b[3]])
}

fn normal_of(a: &Attribute, v: [f32; 4], bytes: &[u8], vi: usize, stride: usize) -> ([f32; 3], Option<[f32; 4]>) {
    match (a.format, packed4(a, bytes, vi, stride)) {
        (42, Some(b)) => {
            let (n, t) = format::unpack_tangent_frame(u32::from_le_bytes(b));
            (n, Some(t))
        }
        (28, Some(b)) => {
            let n = format::unpack_byte_normal(b[0], b[1]);
            let t = format::unpack_byte_normal(b[2], b[3]);
            (n, Some([t[0], t[1], t[2], 1.0]))
        }
        _ => ([v[0], v[1], v[2]], None),
    }
}
