//! The whole lint on hostile mod files: a mutated model, particle, sound event, material, texture or data file,
//! linted deep against synthetic Valve originals, must give findings, never a panic, a hang or a buffer sized by
//! the file. And the text files lint reads (`magician.allow`, a findings file, a baseline) must read or refuse
//! with a line, never panic. See `crates/fuzz`.

#![allow(clippy::unwrap_used, clippy::expect_used)] // a test that cannot unwrap has failed

use lint::{Baseline, Game, Input, ModFile};
use mesh::synth::{SynthDraw, SynthMesh, SynthModel};
use resource::kv3;
use std::collections::HashMap;

#[global_allocator]
static CAP: fuzz::Capped = fuzz::Capped;

const MODEL: &str = "models/heroes_wip/lash/lash.vmdl_c";
const PCF: &str = "particles/abilities/lash/lash_flog.vpcf_c";
const EVTS: &str = "soundevents/hero/lash.vsndevts_c";
const MAT: &str = "models/heroes_wip/lash/materials/lash_body.vmat_c";
const TEX: &str = "models/heroes_wip/lash/materials/lash_body_color_png_f6ecf355.vtex_c";
const VDATA: &str = "scripts/heroes.vdata_c";
const SND: &str = "sounds/abilities/lash/lash_strike.vsnd_c";
const PATHS: &[&str] = &[MODEL, PCF, EVTS, MAT, TEX, VDATA, SND];

fn cases_dir(target: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("fuzz-cases").join(target)
}

fn res_kv(refs: &[&str], tag: &[u8; 4], text: &str) -> Vec<u8> {
    resource::synth_with(refs, &[(*tag, kv3::encode(&kv3::parse_text(text).unwrap()))])
}

fn boxd(min: [f32; 3], max: [f32; 3], material: &str) -> SynthDraw {
    let mut d = SynthDraw { material: material.into(), ..Default::default() };
    for i in 0..8 {
        d.positions.push([if i & 1 == 0 { min[0] } else { max[0] }, if i & 2 == 0 { min[1] } else { max[1] }, if i & 4 == 0 { min[2] } else { max[2] }]);
        d.normals.push([0.0, 0.0, 1.0]);
        d.uvs.push([(i % 2) as f32 * 0.5, (i / 4) as f32 * 0.5]);
        d.joints.push([0, 0, 0, 0]);
        d.weights.push([1.0, 0.0, 0.0, 0.0]);
    }
    d.tris = vec![[0, 2, 3], [0, 3, 1], [4, 5, 7], [4, 7, 6], [0, 1, 5], [0, 5, 4], [2, 6, 7], [2, 7, 3], [0, 4, 6], [0, 6, 2], [1, 3, 7], [1, 7, 5]];
    d
}

fn hero(skirt: bool) -> Vec<u8> {
    let mut draws = vec![boxd([-10.0, -10.0, 0.0], [10.0, 10.0, 60.0], "models/heroes_wip/lash/materials/lash_body.vmat")];
    if skirt {
        draws.push(boxd([-30.0, -30.0, 10.0], [30.0, 30.0, 20.0], "models/heroes_wip/lash/materials/lash_body.vmat"));
    }
    let bones = vec![("root".to_string(), [0.0; 3]), ("head".to_string(), [0.0, 0.0, 55.0])];
    let m = SynthModel {
        name: "models/heroes_wip/lash/lash.vmdl".into(),
        meshes: vec![SynthMesh { name: "body".into(), lod_mask: 1, draws, bones, no_uv: false }],
        skeleton: vec![("root".into(), -1, [0.0; 3]), ("head".into(), 0, [0.0, 0.0, 55.0])],
        hitboxes: vec![("chest".into(), "root".into(), [0.0, 0.0, 10.0], [0.0, 0.0, 50.0], 14.0)],
        ..Default::default()
    };
    mesh::synth::model(&m)
}

fn particle(radius: f64) -> Vec<u8> {
    res_kv(&[], b"DATA", &format!(r#"{{ _class = "CParticleSystemDefinition" m_flConstantRadius = {radius} m_ConstantColor = [ 255, 200, 100, 255 ] m_Renderers = [ {{ _class = "C_OP_RenderSprites" }}, {{ _class = "C_OP_RenderDeferredLight" m_flRadiusScale = 2.0 m_flAlphaScale = 1.0 m_flStartFalloff = 0.1 }} ] m_Children = [ {{ m_ChildRef = resource:"particles/x.vpcf" }} ] }}"#))
}

fn events(volume: f64) -> Vec<u8> {
    res_kv(&[], b"DATA", &format!(r#"{{ Lash.A1.Strike = {{ base = "Base.Ability" volume = {volume} vsnd_files = [ "sounds/abilities/lash/lash_strike.vsnd" ] vsnd_duration = 2.0 }} }}"#))
}

fn sound(dur: f64) -> Vec<u8> {
    res_kv(&[], b"CTRL", &format!("{{ m_vSound = {{ m_nRate = 44100 m_flDuration = {dur} }} m_pEnvelopeAnalyzer = {{ m_curve = {{ m_spline = [ {{ x = 0 y = 0.5 }} ] }} }} }}"))
}

fn material() -> Vec<u8> {
    res_kv(&["models/heroes_wip/lash/materials/lash_body_color_png_f6ecf355.vtex"], b"DATA", r#"{ m_shaderName = "hero.vfx" m_textureParams = [ { m_name = "g_tColor" m_pValue = resource:"models/heroes_wip/lash/materials/lash_body_color_png_f6ecf355.vtex" } ] m_floatParams = [ { m_name = "g_flMetalness" m_flValue = 0.0 } ] m_dynamicParams = [ ] }"#)
}

fn texture() -> Vec<u8> {
    let mut t = resource::synth_with(&[], &[(*b"DATA", resource::texture::synth_header(8, 8, 28))]);
    t.extend(vec![0x80u8; 8 * 8 * 4]);
    t
}

/// Valve's side: the originals at every path, as a game build would hold them.
struct Valve(HashMap<String, Vec<u8>>);
impl Game for Valve {
    fn read(&self, path: &str) -> Option<Vec<u8>> {
        self.0.get(path).cloned()
    }
}

fn valve() -> Valve {
    Valve(HashMap::from([
        (MODEL.to_string(), hero(false)),
        (PCF.to_string(), particle(4.0)),
        (EVTS.to_string(), events(3.0)),
        (MAT.to_string(), material()),
        (TEX.to_string(), texture()),
        (SND.to_string(), sound(2.0)),
        (VDATA.to_string(), res_kv(&[], b"DATA", r#"{ hero_lash = { m_strModelName = "models/heroes_wip/lash/lash.vmdl" } }"#)),
    ]))
}

fn baseline() -> Baseline {
    let mut t = String::from("# magician baseline build=6864\n");
    for (i, p) in PATHS.iter().enumerate() {
        t.push_str(&format!("{p}\t{:08x}\t{}\n", 100 + i, 10 + i));
    }
    Baseline::parse(&t).unwrap()
}

#[test]
fn a_whole_lint_over_one_hostile_file() {
    // a case is the path's index, then the file's bytes
    let mut seeds = Vec::new();
    for (i, data) in [hero(true), particle(40.0), events(-6.0), material(), texture(), res_kv(&[], b"DATA", "{ hero_lash = { m_strModelName = \"models/x.vmdl\" } }"), sound(0.5)].into_iter().enumerate() {
        seeds.push([vec![i as u8], data].concat());
    }
    seeds.extend(fuzz::corpus(cases_dir("lint")));
    let (game, base) = (valve(), baseline());
    // the seeds reach the rules they are meant to: the skirt past the hitboxes, the louder particle, the quiet event
    let seen: std::collections::BTreeSet<&str> = seeds[..3]
        .iter()
        .flat_map(|s| {
            let files = vec![ModFile { path: PATHS[s[0] as usize].into(), stored_crc: 0, inline: true, data: s[1..].to_vec() }];
            lint::lint(&Input { vpk_name: "pak10_dir.vpk".into(), files, base: Some(&base), built_against: None, original_refs: lint::no_originals(), game: Some(&game), deep: true, fair_play: &lint::policy::DEFAULT })
        })
        .map(|f| f.code)
        .collect();
    for code in ["M063", "M081"] {
        assert!(seen.contains(code), "{code} not reached: {seen:?}");
    }
    assert!(seen.iter().any(|c| c.starts_with("M1")), "the mesh family not reached: {seen:?}");
    fuzz::run("lint", fuzz::DEFAULT_CASES / 4, seeds, move |b| {
        let Some((&which, data)) = b.split_first() else { return };
        let path = PATHS[which as usize % PATHS.len()];
        let mut files = vec![ModFile { path: path.into(), stored_crc: vpk::crc32(data), inline: true, data: data.to_vec() }];
        if path != MAT {
            files.push(ModFile { path: MAT.into(), stored_crc: 1, inline: true, data: material() });
        }
        for deep in [false, true] {
            let _ = lint::lint(&Input { vpk_name: "pak10_dir.vpk".into(), files: files.clone(), base: Some(&base), built_against: Some(&base), original_refs: lint::no_originals(), game: Some(&game), deep, fair_play: &lint::policy::DEFAULT });
        }
    });
}

#[test]
fn allowances_findings_files_and_baselines() {
    let allow = "# reasons are mandatory\nM140\tmodels/heroes_wip/lash/lash.vmdl_c\t*\tthe bell skirt is the silhouette\nM031\tsounds/x.vsnd_c\torphan\tnamed by the event file only\n";
    let findings = "# magician findings depth=deep build=6864 vpk=pak10_dir.vpk\nM140\tmodels/heroes_wip/lash/lash.vmdl_c\tworst 14.2 u past the taste line\nM050\t\t'x.vpk' should be pakNN_dir.vpk\n";
    let base = "# magician baseline build=6864 files=2\nmodels/a.vmdl_c\t0034b4e1\t12\nmaterials/b.vmat_c\tffffffff\t0\nmodels/heroes_wip/lash/lash.vmdl_c CRC:006711352c size:5490631\n";
    fuzz::run_text("lint-text", fuzz::DEFAULT_CASES * 2, &[allow, findings, base], |t| {
        if let Ok(a) = lint::allow::parse(t) {
            let _ = lint::allow::apply(Vec::new(), &a, true);
        }
        if let Ok((d, f)) = lint::report::read(t) {
            let again = lint::report::write(&f, d, "6864", "pak10_dir.vpk");
            assert_eq!(lint::report::read(&again).map(|(_, g)| g.len()), Ok(f.len()), "a findings file reads back what it wrote");
        }
        if let Ok(b) = Baseline::parse(t) {
            let mut out = Vec::new();
            b.write(&mut out).unwrap();
            let back = Baseline::parse(&String::from_utf8(out).unwrap()).unwrap();
            assert_eq!(back.files.len(), b.files.len(), "a baseline reads back what it wrote");
        }
    });
}

#[test]
fn a_baseline_reads_back_what_it_writes() {
    let mut rng = fuzz::Rng::new(4);
    for _ in 0..200 {
        let mut b = Baseline { build: format!("{}", rng.below(10000)), ..Default::default() };
        for _ in 0..rng.below(20) {
            let path = format!("{}/{}.vmdl_c", rng.pick(&["models", "materials", "sounds/vo", "é"]), rng.below(1000));
            b.files.insert(path, lint::baseline::FileInfo { crc: rng.next_u64() as u32, size: rng.next_u64() >> rng.below(64) });
        }
        let mut out = Vec::new();
        b.write(&mut out).unwrap();
        let back = Baseline::parse(&String::from_utf8(out).unwrap()).unwrap();
        assert_eq!((back.build, back.files), (b.build, b.files));
    }
}

#[test]
fn a_model_with_vertices_that_are_not_numbers_is_judged_not_a_crash() {
    // NaN and infinite positions: a float stream of someone else's model can hold anything
    let (game, base) = (valve(), baseline());
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let mut skirt = boxd([-30.0, -30.0, 10.0], [30.0, 30.0, 20.0], "models/heroes_wip/lash/materials/lash_body.vmat");
        skirt.positions[3] = [bad, 0.0, bad];
        skirt.positions[5][1] = bad;
        skirt.uvs[2] = [bad, bad];
        skirt.normals[1] = [bad, 0.0, 0.0];
        skirt.weights[4] = [bad, 0.0, 0.0, 0.0];
        let bones = vec![("root".to_string(), [0.0; 3]), ("head".to_string(), [0.0, 0.0, 55.0])];
        let m = SynthModel {
            name: "models/heroes_wip/lash/lash.vmdl".into(),
            meshes: vec![SynthMesh { name: "body".into(), lod_mask: 1, draws: vec![boxd([-10.0, -10.0, 0.0], [10.0, 10.0, 60.0], "models/heroes_wip/lash/materials/lash_body.vmat"), skirt], bones, no_uv: false }],
            skeleton: vec![("root".into(), -1, [0.0; 3]), ("head".into(), 0, [0.0, 0.0, 55.0])],
            hitboxes: vec![("chest".into(), "root".into(), [0.0, 0.0, 10.0], [0.0, 0.0, 50.0], 14.0)],
            raw: true,
            ..Default::default()
        };
        let files = vec![ModFile { path: MODEL.into(), stored_crc: 0, inline: true, data: mesh::synth::model(&m) }, ModFile { path: MAT.into(), stored_crc: 1, inline: true, data: material() }];
        let f = lint::lint(&Input { vpk_name: "pak10_dir.vpk".into(), files, base: Some(&base), built_against: None, original_refs: lint::no_originals(), game: Some(&game), deep: true, fair_play: &lint::policy::DEFAULT });
        assert!(!f.is_empty(), "{bad}");
    }
}
