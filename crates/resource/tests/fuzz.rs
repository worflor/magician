//! Every reader in `resource` on hostile bytes: the block table and RERL, binary KV3 v4 and v5 (raw, LZ4, zstd),
//! the LZ4 and zstd decoders on their own, texture headers and pixels, and KV3 text. Each must return, never panic,
//! hang or allocate from a size it was told; KV3 must also read back what it writes. Seeds are synthetic; with
//! `MAGICIAN_FUZZ_GAME=<...\Deadlock\game>` a long run also mutates real blocks read from the game at run time
//! (never committed). See `crates/fuzz`.

#![allow(clippy::unwrap_used, clippy::expect_used)] // a test that cannot unwrap has failed

use resource::kv3::{self, Value};

#[global_allocator]
static CAP: fuzz::Capped = fuzz::Capped;

/// Inputs earlier runs found, one folder per target: each is replayed first, every run.
fn cases_dir(target: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("fuzz-cases").join(target)
}

fn text(t: &str) -> Value {
    kv3::parse_text(t).unwrap()
}

/// KV3 documents covering every value type, nesting and blobs.
fn values() -> Vec<Value> {
    vec![
        text("{ a = 1 }"),
        text(r#"{ _class = "CParticleSystemDefinition" m_flConstantRadius = 4.5 m_Children = [ { m_ChildRef = resource:"particles/x.vpcf" } ] }"#),
        text(r#"{ Lash.A1.GroundStrike.Explosion = { base = "Base.Ability" volume = 3.0 vsnd_files = [ "sounds/a.vsnd", "sounds/b.vsnd" ] vsnd_duration = 2.0 } }"#),
        Value::Object(vec![
            ("n".into(), Value::Null),
            ("t".into(), Value::Bool(true)),
            ("i".into(), Value::Int(-7)),
            ("u".into(), Value::UInt(u64::MAX)),
            ("d".into(), Value::Double(-0.25)),
            ("s".into(), Value::Str("é𝄞".into())),
            ("b".into(), Value::Blob(vec![1, 2, 3, 0, 255])),
            ("e".into(), Value::Blob(vec![])),
            ("a".into(), Value::Array(vec![Value::Int(1), Value::Array(vec![]), Value::Object(vec![])])),
        ]),
    ]
}

/// A random document: the structure-aware half of the KV3 fuzz.
fn random_value(rng: &mut fuzz::Rng, depth: usize) -> Value {
    match rng.below(if depth > 4 { 7 } else { 9 }) {
        0 => Value::Null,
        1 => Value::Bool(rng.chance(2)),
        2 => Value::Int(rng.next_u64() as i64 >> rng.below(64)),
        3 => Value::UInt(rng.next_u64() >> rng.below(64)),
        4 => Value::Double(f64::from_bits(rng.next_u64()).clamp(-1e300, 1e300)),
        5 => Value::Str((0..rng.below(12)).map(|_| *rng.pick(&['a', 'Z', '_', '.', ' ', 'é', '"'])).collect()),
        6 => Value::Blob((0..rng.below(40)).map(|_| rng.next_u64() as u8).collect()),
        7 => Value::Array((0..rng.below(5)).map(|_| random_value(rng, depth + 1)).collect()),
        _ => Value::Object((0..rng.below(5)).map(|i| (format!("k{i}"), random_value(rng, depth + 1))).collect()),
    }
}

/// An LZ4 block of literals only, and one that copies a match.
fn lz4_seeds() -> Vec<Vec<u8>> {
    let lit = |s: &[u8]| {
        let mut v = Vec::new();
        if s.len() < 15 {
            v.push((s.len() as u8) << 4);
        } else {
            v.push(0xf0);
            let mut n = s.len() - 15;
            while n >= 255 {
                v.push(255);
                n -= 255;
            }
            v.push(n as u8);
        }
        v.extend_from_slice(s);
        v
    };
    vec![lit(b"hello"), lit(&[7u8; 300]), vec![0x35, b'a', b'b', b'c', 3, 0], vec![0x3f, b'x', b'y', b'z', 1, 0, 255, 3]]
}

/// zstd frames: a raw block, an RLE block, and a compressed block of raw literals and no sequences, with and without
/// the checksum.
fn zstd_seeds() -> Vec<Vec<u8>> {
    let frame = |fhd: u8, fcs: u8, blocks: &[Vec<u8>], checksum: Option<u32>| {
        let mut v = 0xFD2F_B528u32.to_le_bytes().to_vec();
        v.push(fhd);
        v.push(fcs);
        for b in blocks {
            v.extend_from_slice(b);
        }
        if let Some(c) = checksum {
            v.extend_from_slice(&c.to_le_bytes());
        }
        v
    };
    let header = |last: bool, kind: u32, size: u32| {
        let h = (size << 3) | (kind << 1) | last as u32;
        h.to_le_bytes()[..3].to_vec()
    };
    let raw = |data: &[u8]| [header(true, 0, data.len() as u32), data.to_vec()].concat();
    let rle = |b: u8, n: u32| [header(true, 1, n), vec![b]].concat();
    let compressed = |data: &[u8]| {
        let body = [vec![(data.len() as u8) << 3], data.to_vec(), vec![0]].concat();
        [header(true, 2, body.len() as u32), body].concat()
    };
    let hello = b"hello, zstd";
    let sum = (resource::zstd::xxh64(hello, 0) & 0xffff_ffff) as u32;
    vec![
        frame(0x20, hello.len() as u8, &[raw(hello)], None),
        frame(0x20, 40, &[rle(b'z', 40)], None),
        frame(0x20, hello.len() as u8, &[compressed(hello)], None),
        frame(0x24, hello.len() as u8, &[raw(hello)], Some(sum)),
    ]
}

/// Real blocks from the installed game, when `MAGICIAN_FUZZ_GAME` names it: the first DATA blocks of some compiled
/// particles, sounds, models and entity lumps (KV3 v5, LZ4 and zstd as Valve writes them). Read only, kept in memory.
fn game_seeds(want: usize) -> Vec<Vec<u8>> {
    let Ok(game) = std::env::var("MAGICIAN_FUZZ_GAME") else { return vec![] };
    let Ok(pak) = vpk::Vpk::open(std::path::Path::new(&game).join("citadel").join("pak01_dir.vpk")) else { return vec![] };
    let mut out = Vec::new();
    for ext in [".vpcf_c", ".vsndevts_c", ".vmat_c", ".vtex_c", ".vmdl_c", ".vents_c", ".vdata_c"] {
        for e in pak.entries.iter().filter(|e| e.path.ends_with(ext)).step_by(97).take(want) {
            if e.size() < 2 << 20 {
                if let Ok(d) = pak.read(e) {
                    out.push(d);
                }
            }
        }
    }
    eprintln!("fuzz: {} seeds from the game", out.len());
    out
}

#[test]
fn resources_and_everything_inside_them() {
    let mut seeds: Vec<Vec<u8>> = vec![resource::synth(&[]), resource::synth(&["materials/a.vmat", "models/b/c.vmdl"])];
    for v in values() {
        seeds.push(resource::synth_with(&["particles/x.vpcf"], &[(*b"DATA", kv3::encode(&v)), (*b"CTRL", kv3::encode(&v))]));
    }
    for f in [1u8, 2, 4, 20, 21, 27, 28] {
        let mut data = resource::synth_with(&[], &[(*b"DATA", resource::texture::synth_header(8, 8, f))]);
        data.extend(std::iter::repeat_n(0x5a, 8 * 8 * 4 + 64));
        seeds.push(data);
    }
    seeds.extend(game_seeds(20));
    seeds.extend(fuzz::corpus(cases_dir("resource")));
    fuzz::run("resource", fuzz::DEFAULT_CASES, seeds, |b| {
        let Ok(r) = resource::parse(b) else { return };
        for blk in &r.blocks {
            let bytes = resource::block_bytes(b, blk);
            let _ = kv3::decode(bytes);
            let _ = resource::parse_rerl(bytes);
        }
        let _ = resource::kv3_block(b, b"DATA");
        let _ = resource::texture::info(b);
        for mip in [0, 1, 7, 64, 255] {
            let _ = resource::pixels::decode(b, mip);
        }
        let mut whole = |off: u64, len: u64| b.get(off as usize..(off as usize).checked_add(len as usize)?).map(<[u8]>::to_vec);
        let _ = resource::refs_by_range(&mut whole);
    });
}

#[test]
fn binary_kv3() {
    let mut seeds: Vec<Vec<u8>> = values().iter().map(kv3::encode).collect();
    let mut rng = fuzz::Rng::new(3);
    seeds.extend((0..8).map(|_| kv3::encode(&random_value(&mut rng, 0))));
    for d in game_seeds(30) {
        if let Ok(r) = resource::parse(&d) {
            seeds.extend(r.blocks.iter().map(|b| resource::block_bytes(&d, b).to_vec()).filter(|b| kv3::is_kv3(b)));
        }
    }
    seeds.extend(fuzz::corpus(cases_dir("kv3")));
    fuzz::run("kv3", fuzz::DEFAULT_CASES * 2, seeds, |b| {
        let _ = kv3::decode(b);
    });
}

#[test]
fn kv3_reads_back_what_it_writes() {
    let mut rng = fuzz::Rng::new(11);
    for v in values().into_iter().chain((0..2000).map(|_| random_value(&mut rng, 0))) {
        // the binary form holds an object at the top
        let v = match v {
            Value::Object(_) => v,
            other => Value::Object(vec![("root".into(), other)]),
        };
        let back = kv3::decode(&kv3::encode(&v)).unwrap_or_else(|e| panic!("{e}: {v:?}"));
        assert_eq!(back, v);
    }
}

#[test]
fn lz4_blocks() {
    fuzz::run("lz4", fuzz::DEFAULT_CASES * 2, lz4_seeds(), |b| {
        let mut out = Vec::new();
        let _ = resource::lz4::decompress_into(b, &mut out);
        // the expected size comes from the file too
        let claimed = b.first().map_or(0, |&x| (x as usize) << (x % 40));
        let _ = resource::lz4::decompress(b, claimed);
        let _ = resource::lz4::decompress(b, usize::MAX >> 1);
    });
}

#[test]
fn zstd_frames() {
    let mut seeds = zstd_seeds();
    seeds.extend(fuzz::corpus(cases_dir("zstd")));
    for name in ["small_l1", "small_l19", "runs_l1", "runs_l19", "binary_l1", "binary_l19"] {
        if let Ok(z) = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("zstd").join(format!("{name}.zst"))) {
            seeds.push(z);
        }
    }
    fuzz::run("zstd", fuzz::DEFAULT_CASES * 2, seeds, |b| {
        let _ = resource::zstd::decompress(b);
        let _ = resource::zstd::decompress_exact(b, 1 << 40);
    });
}

#[test]
fn zstd_seeds_decode_as_built() {
    let s = zstd_seeds();
    assert_eq!(resource::zstd::decompress(&s[0]).unwrap(), b"hello, zstd");
    assert_eq!(resource::zstd::decompress(&s[1]).unwrap(), vec![b'z'; 40]);
    assert_eq!(resource::zstd::decompress(&s[2]).unwrap(), b"hello, zstd");
    assert_eq!(resource::zstd::decompress(&s[3]).unwrap(), b"hello, zstd");
    let mut out = Vec::new();
    resource::lz4::decompress_into(&lz4_seeds()[2], &mut out).unwrap();
    assert_eq!(out, b"abcabcabcabc");
}

#[test]
fn kv3_text() {
    let seeds = [
        "{ a = 1 b = \"x\" c = [ 1, 2.5, -3 ] d = { e = true f = null } }",
        "<!-- kv3 encoding:text:version{e21c7f3c-8a33-41c5-9977-a76d3a32aa0d} format:generic:version{7412167c-06e9-4698-aff2-e63eb59037e7} -->\n{ m_Children = [ { m_ChildRef = resource:\"particles/x.vpcf\" } ] g = #[ 01 02 ff ] }",
        "{ s = \"\"\"multi\nline\"\"\" // comment\n /* block */ k = 0x10 }",
    ];
    fuzz::run_text("kv3-text", fuzz::DEFAULT_CASES * 2, &seeds, |t| {
        if let Ok(v) = kv3::parse_text(t) {
            let _ = v.to_text();
            let _ = kv3::decode(&kv3::encode(&v));
        }
    });
}

#[test]
fn sizes_from_the_file_never_size_a_buffer_alone() {
    // LZ4: a block of one literal told it decodes to half the address space
    assert!(resource::lz4::decompress(&[0x10, b'a'], usize::MAX >> 1).unwrap_err().contains("decoded 1 bytes"));
    // KV3 v5: a blob count of four billion in the header (offset 56), for a document with none
    let mut b = kv3::encode(&values()[3]);
    b[56..60].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(kv3::decode(&b).is_err());
    // zstd: a frame that says it holds a terabyte (an 8-byte content size) and holds five bytes
    let mut z = 0xFD2F_B528u32.to_le_bytes().to_vec();
    z.push(0xe0);
    z.extend((1u64 << 40).to_le_bytes());
    z.extend([0x29, 0, 0]);
    z.extend(b"hello");
    assert!(resource::zstd::decompress(&z).unwrap_err().contains("header says"));
    // a texture whose mip count byte says 255: levels past the 16th are 1 x 1, not a shift out of range
    let mut t = resource::synth_with(&[], &[(*b"DATA", resource::texture::synth_header(8, 8, 28))]);
    let at = resource::parse(&t).unwrap().block(b"DATA").unwrap().offset + 27;
    t[at] = 255;
    assert_eq!(resource::texture::info(&t).unwrap().mips, 255);
    t.extend(vec![0u8; 8 * 8 * 4 * 2 + 255 * 4]);
    assert!(resource::pixels::decode(&t, 0).is_some());
}

/// A binary KV3 v5 document, uncompressed, whose root is an array of `e` typed arrays of 255 nulls: each costs two
/// type bytes and one count byte, and decodes to 255 values.
fn typed_null_arrays(e: usize) -> Vec<u8> {
    let mut b1 = vec![255u8; e]; // bytes stream: each typed array's count
    while !b1.len().is_multiple_of(4) {
        b1.push(0);
    }
    b1.extend((e as u32).to_le_bytes()); // ints stream: the root array's length
    let types_at = b1.len();
    b1.push(8); // the root: an array
    for _ in 0..e {
        b1.extend([24, 1]); // a typed array whose elements are null
    }
    let types_size = b1.len() - types_at;
    b1.extend(0xFFEE_DD00u32.to_le_bytes());
    let b0 = 0u32.to_le_bytes().to_vec(); // no strings
    let mut h = vec![5, b'3', b'V', b'K'];
    h.extend([0u8; 16]);
    let u32s = |h: &mut Vec<u8>, v: &[u32]| v.iter().for_each(|x| h.extend(x.to_le_bytes()));
    u32s(&mut h, &[0]); // method: none
    h.extend([0u8; 4]); // dictionary, frame
    u32s(&mut h, &[0, 1, 0, types_size as u32]); // buffer 0 bytes, ints, doubles; the type stream's size
    h.extend([0u8; 4]); // object and array counts (u16 each)
    u32s(&mut h, &[0, 0, 0, 0, 0, 0]); // totals, blocks, buffer 0 shorts, blob frame sizes
    u32s(&mut h, &[b0.len() as u32, b0.len() as u32, b1.len() as u32, b1.len() as u32]);
    u32s(&mut h, &[e as u32, 0, 1, 0]); // buffer 1 bytes, shorts, ints, doubles
    u32s(&mut h, &[0, 0, 0, 0]);
    [h, b0, b1].concat()
}

#[test]
fn typed_arrays_of_constants_decode_to_no_more_than_the_data_could_hold() {
    // built right: ten typed arrays of 255 nulls each
    let v = kv3::decode(&typed_null_arrays(10)).unwrap();
    assert_eq!(v.as_array().len(), 10);
    assert_eq!(v.as_array()[9].as_array().len(), 255);
    // two million nulls from 25 kB (found by the long fuzz, as a typed array nested to decode to gigabytes)
    let e = kv3::decode(&typed_null_arrays(8200)).unwrap_err();
    assert!(e.contains("more than the data could hold"), "{e}");
}

#[test]
fn material_sources_and_compiled_materials() {
    // a .vmat source in KeyValues 1 and in KV3 text, and a compiled .vmat_c, each on hostile input
    let seeds = [
        "// x\n\"Layer0\"\n{\n\t\"shader\"\t\"pbr.vfx\"\n\t\"F_SHEEN\"\t\"1\"\n\t\"TextureColor1\"\t\"materials/x_color.png\"\n\t\"TextureNprTramsissiveColor1\"\t\"[0 0.5 0 0]\"\n\t\"g_flA1\"\t\"1.0\"\n\t\"g_vB1\"\t\"[1 2 3 4]\"\n\t\"DynamicParams\"\n\t{\n\t\t\"g_vB1\"\t\"return float3(1,1,1);\"\n\t}\n}\n",
        "<!-- kv3 encoding:text:version{e21c7f3c-8a33-41c5-9977-a76d3a32aa0d} format:generic:version{7412167c-06e9-4698-aff2-e63eb59037e7} -->\n{ Layer0 = { shader = \"pbr.vfx\" F_SHEEN = 1 TextureColor1 = \"materials/x.png\" } }",
        "\"Layer0\" { \"shader\" \"pbr.vfx\" \"a\" { \"b\" { \"c\" \"\\\"q\\\"\" } } }",
    ];
    fuzz::run_text("vmat", fuzz::DEFAULT_CASES * 2, &seeds, |t| {
        if let Ok(m) = resource::material::from_source(t) {
            for (k, _) in &m.textures {
                let _ = resource::material::expected_slot(&m, k);
            }
        }
    });
    let data = text(r#"{ m_materialName = "materials/x.vmat" m_shaderName = "pbr.vfx" m_intParams = [ { m_name = "F_SHEEN" m_nValue = 1 } ] m_floatParams = [ { m_name = "g_flA" m_flValue = 0.5 } ] m_vectorParams = [ { m_name = "g_vB" m_value = [ 1.0, 2.0, 3.0, 4.0 ] } ] m_textureParams = [ { m_name = "g_tColor" m_pValue = "materials/x_color_png_1.vtex" } ] m_dynamicParams = [ { m_name = "g_vB" m_value = #[ 01 02 ] } ] m_renderAttributesUsed = [ "$ent_abs_velocity" ] }"#);
    let red = text(r#"{ m_ArgumentDependencies = [ { m_ParameterName = "materials/x_color.png" m_ParameterType = "ImageArg" } ] }"#);
    let seed = resource::synth_with(&[], &[(*b"RED2", kv3::encode(&red)), (*b"DATA", kv3::encode(&data))]);
    assert!(resource::material::from_compiled(&seed).is_ok());
    fuzz::run("vmat_c", fuzz::DEFAULT_CASES, vec![seed], |b| {
        let _ = resource::material::from_compiled(b);
    });
}
