//! The mesh readers on hostile bytes: compiled models (skeleton, hitboxes, draws, every LOD's surfaces), the
//! meshoptimizer vertex and index codecs, the cloth FeModel in PHYS, and the glb and JSON a proof reads. Each must
//! return, never panic, hang or allocate from a count it was told. The codecs must also decode what they encode.
//! Seeds come from `mesh::synth`, so every triangle is synthetic. See `crates/fuzz`.

#![allow(clippy::unwrap_used, clippy::expect_used)] // a test that cannot unwrap has failed

use mesh::synth::{SynthDraw, SynthMesh, SynthModel};

#[global_allocator]
static CAP: fuzz::Capped = fuzz::Capped;

fn cases_dir(target: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("fuzz-cases").join(target)
}

/// A box of `material` from `min` to `max`, weighted to bone `joint`.
fn boxd(min: [f32; 3], max: [f32; 3], material: &str, joint: u8) -> SynthDraw {
    let mut d = SynthDraw { material: material.into(), ..Default::default() };
    for i in 0..8 {
        d.positions.push([if i & 1 == 0 { min[0] } else { max[0] }, if i & 2 == 0 { min[1] } else { max[1] }, if i & 4 == 0 { min[2] } else { max[2] }]);
        d.normals.push([0.0, 0.0, 1.0]);
        d.uvs.push([(i % 2) as f32, (i / 4) as f32]);
        d.joints.push([joint, 0, 0, 0]);
        d.weights.push([1.0, 0.0, 0.0, 0.0]);
    }
    d.tris = vec![[0, 2, 3], [0, 3, 1], [4, 5, 7], [4, 7, 6], [0, 1, 5], [0, 5, 4], [2, 6, 7], [2, 7, 3], [0, 4, 6], [0, 6, 2], [1, 3, 7], [1, 7, 5]];
    d
}

fn model(raw: bool, cloth: bool) -> Vec<u8> {
    let bones = vec![("root".to_string(), [0.0; 3]), ("head".to_string(), [0.0, 0.0, 55.0])];
    let m = SynthModel {
        name: "models/heroes_wip/x/x.vmdl".into(),
        meshes: vec![
            SynthMesh { name: "body".into(), lod_mask: 1, draws: vec![boxd([-10.0; 3], [10.0, 10.0, 60.0], "materials/body.vmat", 0), boxd([-4.0, -4.0, 60.0], [4.0, 4.0, 70.0], "materials/head.vmat", 1)], bones: bones.clone(), no_uv: false },
            SynthMesh { name: "body_lod1".into(), lod_mask: 2, draws: vec![boxd([-10.0; 3], [10.0, 10.0, 60.0], "materials/body.vmat", 0)], bones, no_uv: true },
        ],
        skeleton: vec![("root".into(), -1, [0.0; 3]), ("head".into(), 0, [0.0, 0.0, 55.0])],
        raw,
        hitboxes: vec![("chest".into(), "root".into(), [0.0, 0.0, 10.0], [0.0, 0.0, 50.0], 14.0), ("pack".into(), "root".into(), [-4.0, -4.0, 20.0], [4.0, 4.0, 40.0], 0.0)],
        hitbox_shapes: vec![2, 0],
        mesh_group_masks: vec![1, 3],
        extra_materials: vec!["materials/spare.vmat".into()],
        extra_blocks: if cloth {
            let phys = resource::kv3::Value::Object(vec![("m_pFeModel".into(), mesh::cloth::fixture())]);
            vec![(*b"PHYS", resource::kv3::encode(&phys))]
        } else {
            vec![]
        },
    };
    mesh::synth::model(&m)
}

#[test]
fn compiled_models() {
    let mut seeds = vec![model(false, false), model(true, false), model(false, true), mesh::cloth::synth_model(mesh::cloth::fixture())];
    seeds.extend(fuzz::corpus(cases_dir("models")));
    fuzz::run("model", fuzz::DEFAULT_CASES, seeds, |b| {
        if let Ok(m) = mesh::model::parse(b) {
            for lod in 0..=m.lod_count().min(70) {
                let _ = m.surfaces(lod);
            }
            let _ = m.skeleton.world();
        }
        if let Ok(Some(c)) = mesh::cloth::read(b) {
            let _ = mesh::cloth::chains(&c);
            for n in 0..c.node_count.min(64) {
                let _ = (c.kind(n), c.name(n), c.radius(n));
            }
        }
    });
}

/// A codec case: `[count u16][size u8][encoded buffer]`, so the count and size the decoder is told mutate too.
fn framed(count: usize, size: usize, buf: &[u8]) -> Vec<u8> {
    [(count as u16).to_le_bytes().to_vec(), vec![size as u8], buf.to_vec()].concat()
}

#[test]
fn meshoptimizer_codecs() {
    let mut rng = fuzz::Rng::new(9);
    let mut seeds = Vec::new();
    for (count, size) in [(1usize, 4usize), (16, 12), (300, 16), (64, 3), (257, 48)] {
        let data: Vec<u8> = (0..count * size).map(|i| if rng.chance(3) { rng.next_u64() as u8 } else { (i / size) as u8 }).collect();
        seeds.push(framed(count, size, &mesh::meshopt::encode_vertex_buffer_v0(&data, count, size)));
    }
    let tris: Vec<u32> = (0..90u32).map(|i| (i * 7) % 31).collect();
    seeds.push(framed(tris.len(), 0, &mesh::meshopt::encode_index_buffer_explicit(&tris)));
    seeds.extend(fuzz::corpus(cases_dir("meshopt")));
    fuzz::run("meshopt", fuzz::DEFAULT_CASES * 2, seeds, |b| {
        if b.len() < 3 {
            return;
        }
        let (count, size, buf) = (u16::from_le_bytes([b[0], b[1]]) as usize, b[2] as usize, &b[3..]);
        let _ = mesh::meshopt::decode_vertex_buffer(count, size, buf);
        let _ = mesh::meshopt::decode_vertex_buffer(count << 8, size.max(1), buf);
        let _ = mesh::meshopt::decode_index_buffer(count, buf);
        let _ = mesh::meshopt::decode_index_buffer((count * 3) << 6, buf);
        let _ = mesh::meshopt::decode_index_sequence(count, buf);
    });
}

#[test]
fn the_codecs_decode_what_they_encode() {
    let mut rng = fuzz::Rng::new(21);
    for _ in 0..300 {
        let count = 1 + rng.below(400);
        let size = 1 + rng.below(64);
        let data: Vec<u8> = (0..count * size).map(|_| rng.next_u64() as u8 >> rng.below(8)).collect();
        let enc = mesh::meshopt::encode_vertex_buffer_v0(&data, count, size);
        assert_eq!(mesh::meshopt::decode_vertex_buffer(count, size, &enc).unwrap(), data, "{count} x {size}");
        let n = 3 * (1 + rng.below(100));
        let span = 1 + rng.below(70000);
        let idx: Vec<u32> = (0..n).map(|_| rng.below(span) as u32).collect();
        let enc = mesh::meshopt::encode_index_buffer_explicit(&idx);
        assert_eq!(mesh::meshopt::decode_index_buffer(n, &enc).unwrap(), idx);
    }
}

/// A glb with one triangle: JSON chunk (accessors, buffer views) and BIN chunk.
fn glb() -> Vec<u8> {
    let json = br#"{"asset":{"version":"2.0"},"buffers":[{"byteLength":42}],"bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":36},{"buffer":0,"byteOffset":36,"byteLength":6}],"accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"},{"bufferView":1,"componentType":5123,"count":3,"type":"SCALAR"}],"meshes":[{"primitives":[{"attributes":{"POSITION":0},"indices":1}]}]}"#;
    let mut j = json.to_vec();
    while !j.len().is_multiple_of(4) {
        j.push(b' ');
    }
    let mut bin: Vec<u8> = [0.0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0].iter().flat_map(|f| f.to_le_bytes()).collect();
    bin.extend([0u16, 1, 2].iter().flat_map(|i| i.to_le_bytes()));
    bin.extend([0, 0]);
    let mut out = b"glTF".to_vec();
    out.extend(2u32.to_le_bytes());
    out.extend(((12 + 8 + j.len() + 8 + bin.len()) as u32).to_le_bytes());
    out.extend((j.len() as u32).to_le_bytes());
    out.extend(b"JSON");
    out.extend(&j);
    out.extend((bin.len() as u32).to_le_bytes());
    out.extend(b"BIN\0");
    out.extend(&bin);
    out
}

#[test]
fn glb_and_json() {
    let g = glb();
    let parsed = mesh::glb::Glb::read(&g).unwrap();
    assert_eq!(parsed.indices(1).unwrap(), vec![0, 1, 2]);
    let mut seeds = vec![g];
    seeds.extend(fuzz::corpus(cases_dir("glb")));
    fuzz::run("glb", fuzz::DEFAULT_CASES * 2, seeds, |b| {
        if let Ok(g) = mesh::glb::Glb::read(b) {
            for i in 0..4 {
                let _ = g.accessor(i);
                let _ = g.indices(i);
            }
        }
        let _ = mesh::glb::parse_json(&String::from_utf8_lossy(b));
    });
}

#[test]
fn a_vertex_count_the_buffer_cannot_hold_is_refused_before_it_sizes_the_output() {
    let data = vec![7u8; 16 * 160];
    let enc = mesh::meshopt::encode_vertex_buffer_v0(&data, 16, 160);
    // ten million vertices of 160 bytes (1.6 GB) claimed for a buffer of a few hundred bytes
    let e = mesh::meshopt::decode_vertex_buffer(10_000_000, 160, &enc).unwrap_err();
    assert!(e.contains("10000000 vertices need more"), "{e}");
    assert!(mesh::meshopt::decode_vertex_buffer(usize::MAX / 2, 160, &enc).is_err());
    assert_eq!(mesh::meshopt::decode_vertex_buffer(16, 160, &enc).unwrap(), data);
}

#[test]
fn a_buffer_count_that_overflows_its_size_is_an_error_not_a_panic() {
    // a raw vertex buffer of 2^62 elements of 32 bytes, and an index buffer of 2^62 four-byte indices
    let ctrl = r#"{ embedded_meshes = [ { m_Name = "body" m_nMeshIndex = 0 m_nDataBlock = 3
        m_vertexBuffers = [ { m_nBlockIndex = 2 m_nElementCount = 4611686018427387904 m_nElementSizeInBytes = 32 m_bMeshoptCompressed = false } ]
        m_indexBuffers = [ { m_nBlockIndex = 2 m_nElementCount = 4611686018427387904 m_nElementSizeInBytes = 4 m_bMeshoptCompressed = false } ] } ] }"#;
    let ctrl = resource::kv3::encode(&resource::kv3::parse_text(ctrl).unwrap());
    let b = resource::synth_with(&[], &[(*b"CTRL", ctrl), (*b"MVTX", vec![0u8; 64])]);
    let m = mesh::model::parse(&b).unwrap();
    assert!(m.meshes[0].vbs[0].data.as_ref().unwrap_err().contains("needs 4611686018427387904 x 32"), "{:?}", m.meshes[0].vbs[0].data);
    assert!(m.meshes[0].ibs[0].data.is_err());
}

#[test]
fn a_glb_accessor_with_counts_and_offsets_past_the_buffer_is_an_error() {
    let g = String::from_utf8_lossy(&glb()).into_owned();
    for (from, to) in [("\"count\":3,", "\"count\":1e30,"), ("\"byteOffset\":36,", "\"byteOffset\":1e30,"), ("\"count\":3,", "\"count\":4611686018427387904,")] {
        let json = r#"{"asset":{"version":"2.0"},"buffers":[{"byteLength":42}],"bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":36},{"buffer":0,"byteOffset":36,"byteLength":6}],"accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","byteOffset":1e30},{"bufferView":1,"componentType":5123,"count":3,"type":"SCALAR"}]}"#.replacen(from, to, 1);
        let parsed = mesh::glb::Glb { json: mesh::glb::parse_json(&json).unwrap(), bin: vec![0u8; 42] };
        for i in 0..2 {
            assert!(parsed.accessor(i).is_err() || parsed.accessor(i).is_ok(), "{from} -> {to}");
        }
        assert!(parsed.accessor(0).is_err(), "{to}: {g:.0}");
    }
}

#[test]
fn a_normal_whose_offset_is_past_the_vertex_is_read_as_absent_not_a_panic() {
    // one triangle, raw buffers: POSITION (three floats) at 0 and a packed NORMAL said to sit at byte 1000 of 12-byte vertices
    let ctrl = r#"{ embedded_meshes = [ { m_Name = "body" m_nMeshIndex = 0 m_nDataBlock = 4
        m_vertexBuffers = [ { m_nBlockIndex = 2 m_nElementCount = 3 m_nElementSizeInBytes = 12 m_bMeshoptCompressed = false
            m_inputLayoutFields = [ { m_pSemanticName = "POSITION" m_nSemanticIndex = 0 m_Format = 6 m_nOffset = 0 },
                                    { m_pSemanticName = "NORMAL" m_nSemanticIndex = 0 m_Format = 42 m_nOffset = 1000 } ] } ]
        m_indexBuffers = [ { m_nBlockIndex = 3 m_nElementCount = 3 m_nElementSizeInBytes = 2 m_bMeshoptCompressed = false } ] } ] }"#;
    let mdat = r#"{ m_sceneObjects = [ { m_drawCalls = [ { m_material = "materials/x.vmat" m_nPrimitiveType = "RENDER_PRIM_TRIANGLES"
        m_nStartIndex = 0 m_nIndexCount = 3 m_nBaseVertex = 0 m_nVertexCount = 3 m_indexBuffer = { m_hBuffer = 0 } m_vertexBuffers = [ { m_hBuffer = 0 m_nBindOffsetBytes = 0 } ] } ] } ] }"#;
    let enc = |t: &str| resource::kv3::encode(&resource::kv3::parse_text(t).unwrap());
    let vb: Vec<u8> = [0.0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0].iter().flat_map(|f| f.to_le_bytes()).collect();
    let ib: Vec<u8> = [0u16, 1, 2].iter().flat_map(|i| i.to_le_bytes()).collect();
    let b = resource::synth_with(&[], &[(*b"CTRL", enc(ctrl)), (*b"MVTX", vb), (*b"MIDX", ib), (*b"MDAT", enc(mdat))]);
    let m = mesh::model::parse(&b).unwrap();
    let (surfaces, notes) = m.surfaces(0);
    assert_eq!(surfaces.len(), 1, "{notes:?}");
    assert_eq!(surfaces[0].positions.len(), 3);
}
