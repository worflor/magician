//! Fixtures for the mesh family (M100-M135): one deliberately bad model (or material, particle,
//! texture) per rule, built with `mesh::synth` against a synthetic "Valve" hero: a 20 x 20 x 60
//! skinned body box with a capsule hitbox, two LODs, a skin material with the status proxy and NPR.
//! Each test asserts the rule fires for that reason; the clean copy proves the family is silent on
//! an unchanged model.

use super::*;
use ::mesh::synth::{SynthDraw, SynthMesh, SynthModel};
use crate::mesh::census::Envelope;
use crate::mesh::rules;

const BODY: &str = "models/heroes_wip/lash/materials/lash_body.vmat";
const CLOTH: &str = "materials/test/cloth.vmat";

type P3 = [f32; 3];

fn with_game(g: &mut Baseline, paths: &[&str]) {
    super::with(g, paths)
}

const CLOTH_COLOR: &str = "materials/test/cloth_color.vtex";
const BODY_COLOR: &str = "models/heroes_wip/lash/materials/lash_body_color.vtex";

/// A BGRA8888 texture whose mips are box-filtered from `px` (w x h RGBA), stored smallest first.
fn vtex(w: usize, h: usize, px: Vec<[u8; 4]>) -> Vec<u8> {
    let mut mips = vec![(w, h, px)];
    while mips.last().unwrap().0 > 1 || mips.last().unwrap().1 > 1 {
        let (pw, ph, p) = mips.last().unwrap().clone();
        let (nw, nh) = ((pw / 2).max(1), (ph / 2).max(1));
        let mut n = vec![[0u8; 4]; nw * nh];
        for y in 0..nh {
            for x in 0..nw {
                let mut acc = [0u32; 4];
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let (sx, sy) = ((2 * x + dx).min(pw - 1), (2 * y + dy).min(ph - 1));
                    for c in 0..4 {
                        acc[c] += p[sy * pw + sx][c] as u32;
                    }
                }
                n[y * nw + x] = [(acc[0] / 4) as u8, (acc[1] / 4) as u8, (acc[2] / 4) as u8, (acc[3] / 4) as u8];
            }
        }
        mips.push((nw, nh, n));
    }
    let mut hdr = resource::texture::synth_header(w as u16, h as u16, 28);
    hdr[27] = mips.len() as u8;
    let mut f = resource::synth_with(&[], &[(*b"DATA", hdr)]);
    for (_, _, p) in mips.iter().rev() {
        for c in p {
            f.extend_from_slice(&[c[2], c[1], c[0], c[3]]);
        }
    }
    f
}

fn flat(w: usize, c: [u8; 4]) -> Vec<u8> {
    vtex(w, w, vec![c; w * w])
}

fn vmat(flags: &[&str], textured: bool, outline: Option<f64>) -> Vec<u8> {
    vmat_with(flags, if textured { CLOTH_COLOR } else { "materials/default/default_color_tga_96b3aa03.vtex" }, outline)
}

fn vmat_with(flags: &[&str], color: &str, outline: Option<f64>) -> Vec<u8> {
    let ints: Vec<String> = flags.iter().map(|f| format!("{{ m_name = \"{f}\" m_nValue = 1 }}")).collect();
    let tex = format!("{{ m_name = \"g_tColor\" m_pValue = \"{color}\" }}");
    let vec = outline.map(|o| format!("{{ m_name = \"TextureNprOutlineMask1\" m_value = [ {o}, {o}, {o}, 0.0 ] }}")).unwrap_or_default();
    res_kv(&[], &[(b"DATA", kv(&format!("{{ m_shaderName = \"pbr.vfx\" m_intParams = [ {} ] m_textureParams = [ {tex} ] m_vectorParams = [ {vec} ] }}", ints.join(" "))))])
}

fn hero_vmat() -> Vec<u8> {
    vmat(&["F_USE_STATUS_EFFECTS_PROXY", "F_USE_NPR_LIGHTING"], true, None)
}

/// Mid-grey (sRGB 110, ~0.16 linear), the albedo every fixture texture has unless a test says otherwise.
const GREY: [u8; 4] = [110, 110, 110, 255];

/// A box as six 4-vertex faces, outward, flat normals, each face its own UV strip.
fn boxd(min: P3, max: P3, material: &str, joint: u8) -> SynthDraw {
    let mut d = SynthDraw { material: material.into(), ..Default::default() };
    let c = |i: usize| [if i & 1 == 0 { min[0] } else { max[0] }, if i & 2 == 0 { min[1] } else { max[1] }, if i & 4 == 0 { min[2] } else { max[2] }];
    let faces: [([usize; 4], P3); 6] = [
        ([0, 2, 3, 1], [0.0, 0.0, -1.0]),
        ([4, 5, 7, 6], [0.0, 0.0, 1.0]),
        ([0, 1, 5, 4], [0.0, -1.0, 0.0]),
        ([2, 6, 7, 3], [0.0, 1.0, 0.0]),
        ([0, 4, 6, 2], [-1.0, 0.0, 0.0]),
        ([1, 3, 7, 5], [1.0, 0.0, 0.0]),
    ];
    for (k, (q, n)) in faces.iter().enumerate() {
        let b = d.positions.len() as u32;
        let u0 = k as f32 / 6.0;
        // conformal: the face's own aspect, fitted into a 0.15 x 0.8 strip
        let (fw, fh) = (dist(c(q[0]), c(q[1])), dist(c(q[1]), c(q[2])));
        let sc = (0.15 / fw.max(1e-6)).min(0.8 / fh.max(1e-6));
        for (j, &i) in q.iter().enumerate() {
            d.positions.push(c(i));
            d.normals.push(*n);
            d.uvs.push([u0 + if j == 1 || j == 2 { fw * sc } else { 0.0 }, 0.1 + if j >= 2 { fh * sc } else { 0.0 }]);
            d.joints.push([joint, 0, 0, 0]);
            d.weights.push([1.0, 0.0, 0.0, 0.0]);
        }
        d.tris.push([b, b + 1, b + 2]);
        d.tris.push([b, b + 2, b + 3]);
    }
    d
}

fn dist(a: P3, b: P3) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// A box with every face cut into n x n quads (for budgets).
fn dense_box(min: P3, max: P3, material: &str, n: usize) -> SynthDraw {
    let mut d = SynthDraw { material: material.into(), ..Default::default() };
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    for axis in 0..3 {
        for side in [0, 1] {
            let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
            let b = d.positions.len() as u32;
            for i in 0..=n {
                for j in 0..=n {
                    let mut p = [0.0f32; 3];
                    p[axis] = if side == 0 { min[axis] } else { max[axis] };
                    p[u] = lerp(min[u], max[u], i as f32 / n as f32);
                    p[v] = lerp(min[v], max[v], j as f32 / n as f32);
                    let mut nrm = [0.0f32; 3];
                    nrm[axis] = if side == 0 { -1.0 } else { 1.0 };
                    d.positions.push(p);
                    d.normals.push(nrm);
                    d.uvs.push([(axis * 2 + side) as f32 / 6.0 + 0.15 * i as f32 / n as f32, 0.1 + 0.8 * j as f32 / n as f32]);
                    d.joints.push([1, 0, 0, 0]);
                    d.weights.push([1.0, 0.0, 0.0, 0.0]);
                }
            }
            let id = |i: usize, j: usize| b + (i * (n + 1) + j) as u32;
            for i in 0..n {
                for j in 0..n {
                    // (u, v) x (axis) orientation: u x v = +axis, so side 1 keeps it, side 0 flips
                    let (a, bb, c, e) = (id(i, j), id(i + 1, j), id(i + 1, j + 1), id(i, j + 1));
                    if side == 1 {
                        d.tris.push([a, bb, c]);
                        d.tris.push([a, c, e]);
                    } else {
                        d.tris.push([a, c, bb]);
                        d.tris.push([a, e, c]);
                    }
                }
            }
        }
    }
    d
}

fn bones() -> Vec<(String, [f32; 3])> {
    vec![("root".into(), [0.0; 3]), ("spine".into(), [0.0, 0.0, 30.0]), ("head".into(), [0.0, 0.0, 55.0]), ("ghost".into(), [0.0, 0.0, 0.0])]
}

fn body() -> SynthDraw {
    boxd([-10.0, -10.0, 0.0], [10.0, 10.0, 60.0], BODY, 1)
}

/// A head on the head bone, standing clear above the body.
fn head() -> SynthDraw {
    boxd([-4.0, -4.0, 70.0], [4.0, 4.0, 78.0], CLOTH, 2)
}

fn hero(lod0: Vec<SynthDraw>, lod1: Option<Vec<SynthDraw>>) -> SynthModel {
    let mut meshes = vec![SynthMesh { name: "body".into(), lod_mask: 1, draws: lod0, bones: bones(), no_uv: false }];
    if let Some(l1) = lod1 {
        meshes.push(SynthMesh { name: "body_lod1".into(), lod_mask: 2, draws: l1, bones: bones(), no_uv: false });
    }
    SynthModel {
        name: "models/heroes_wip/lash/lash.vmdl".into(),
        meshes,
        skeleton: vec![("root".into(), -1, [0.0; 3]), ("spine".into(), 0, [0.0, 0.0, 30.0]), ("head".into(), 1, [0.0, 0.0, 25.0])],
        hitboxes: vec![("chest".into(), "root".into(), [0.0, 0.0, 10.0], [0.0, 0.0, 50.0], 14.0)],
        ..Default::default()
    }
}

fn valve() -> SynthModel {
    hero(vec![body()], Some(vec![body()]))
}

/// Our model: Valve's body plus `extra` pieces on LOD0.
fn with(extra: Vec<SynthDraw>) -> SynthModel {
    let mut l0 = vec![body()];
    l0.extend(extra);
    hero(l0, Some(vec![body()]))
}

fn mesh_run(ours: &SynthModel, extra_files: Vec<ModFile>, cloth: Vec<u8>) -> Vec<Finding> {
    mesh_run_at(ours, extra_files, cloth, &policy::DEFAULT)
}

/// `mesh_run` judged at other fair-play limits.
fn mesh_run_at(ours: &SynthModel, extra_files: Vec<ModFile>, cloth: Vec<u8>, fp: &policy::FairPlay) -> Vec<Finding> {
    mesh_run_against(ours, &valve(), extra_files, cloth, fp)
}

/// `mesh_run_at` against another Valve model at the same path.
fn mesh_run_against(ours: &SynthModel, theirs: &SynthModel, extra_files: Vec<ModFile>, cloth: Vec<u8>, fp: &policy::FairPlay) -> Vec<Finding> {
    let mut g = game();
    let mut fake = Fake::default();
    fake.files.insert(MODEL.into(), ::mesh::synth::model(theirs));
    fake.files.insert(format!("{BODY}_c"), vmat_with(&["F_USE_STATUS_EFFECTS_PROXY", "F_USE_NPR_LIGHTING"], BODY_COLOR, None));
    // Valve's body texture: greys from sRGB 60 to 160, so its range is a band, not a point
    let mut grad = Vec::new();
    for i in 0..16 * 16 {
        let v = 60 + (i % 16) as u8 * 6;
        grad.push([v, v, v, 255]);
    }
    fake.files.insert(format!("{BODY_COLOR}_c"), vtex(16, 16, grad));
    with_game(&mut g, &[MODEL, &format!("{BODY}_c"), &format!("{BODY_COLOR}_c")]);
    let mut files = vec![file(MODEL, ::mesh::synth::model(ours)), file(&format!("{CLOTH}_c"), cloth)];
    if !extra_files.iter().any(|f| f.path == format!("{CLOTH_COLOR}_c")) {
        files.push(file(&format!("{CLOTH_COLOR}_c"), flat(16, GREY)));
    }
    files.extend(extra_files);
    lint(&Input { vpk_name: "pak03_dir.vpk".into(), files, base: Some(&g), built_against: None, original_refs: no_originals(), game: Some(&fake), deep: true, fair_play: fp })
}

fn run_ours(ours: &SynthModel) -> Vec<Finding> {
    mesh_run(ours, vec![], hero_vmat())
}

/// Our model against a Valve model of the test's own making (both on Valve's body material and the cloth).
fn run_against(ours: &SynthModel, valve: &SynthModel) -> Vec<Finding> {
    let mut g = game();
    let mut fake = Fake::default();
    fake.files.insert(MODEL.into(), ::mesh::synth::model(valve));
    fake.files.insert(format!("{BODY}_c"), vmat_with(&["F_USE_STATUS_EFFECTS_PROXY", "F_USE_NPR_LIGHTING"], BODY_COLOR, None));
    fake.files.insert(format!("{BODY_COLOR}_c"), flat(16, GREY));
    fake.files.insert(format!("{CLOTH}_c"), hero_vmat());
    fake.files.insert(format!("{CLOTH_COLOR}_c"), flat(16, GREY));
    with_game(&mut g, &[MODEL, &format!("{BODY}_c"), &format!("{BODY_COLOR}_c"), &format!("{CLOTH}_c"), &format!("{CLOTH_COLOR}_c")]);
    let files = vec![file(MODEL, ::mesh::synth::model(ours))];
    lint(&Input { vpk_name: "pak03_dir.vpk".into(), files, base: Some(&g), built_against: None, original_refs: no_originals(), game: Some(&fake), deep: true, fair_play: &policy::DEFAULT })
}

/// A piece turned inside out: every triangle rewound and every normal reversed, so the two still agree.
fn inside_out(mut d: SynthDraw) -> SynthDraw {
    for t in &mut d.tris {
        t.swap(1, 2);
    }
    for n in &mut d.normals {
        *n = [-n[0], -n[1], -n[2]];
    }
    d
}

fn has(f: &[Finding], code: &str, needle: &str) -> bool {
    f.iter().any(|x| x.code == code && x.message.contains(needle))
}

fn mesh_codes(f: &[Finding]) -> BTreeSet<&'static str> {
    f.iter().map(|x| x.code).filter(|c| c.as_bytes()[1] == b'1').collect()
}

#[test]
fn mesh_clean_copy_is_silent() {
    let f = run_ours(&valve());
    assert!(mesh_codes(&f).is_empty(), "{f:#?}");
    // a garment that sits on the body, single-sided, textured, weighted like it: still silent
    let f = run_ours(&with(vec![boxd([10.0, -3.0, 20.0], [12.0, 3.0, 30.0], CLOTH, 1)]));
    assert!(mesh_codes(&f).is_empty(), "{f:#?}");
}

#[test]
fn m100_undecodable_buffer() {
    let mut bytes = ::mesh::synth::model(&with(vec![]));
    let r = resource::parse(&bytes).unwrap();
    let b = r.blocks[0].clone();
    for x in &mut bytes[b.offset + 1..b.offset + b.size] {
        *x = 0x55;
    }
    let mut g = game();
    let mut fake = Fake::default();
    fake.files.insert(MODEL.into(), ::mesh::synth::model(&valve()));
    with_game(&mut g, &[MODEL]);
    let f = run_game(vec![file(MODEL, bytes)], &g, &fake);
    assert!(f.iter().any(|x| x.code == "M100" && x.message.contains("vertex buffer")), "{f:#?}");
}

#[test]
fn m101_non_finite_vertex() {
    let mut p = boxd([10.0, -3.0, 20.0], [12.0, 3.0, 30.0], CLOTH, 1);
    p.positions[0][1] = f32::NAN;
    let f = run_ours(&with(vec![p]));
    assert!(has(&f, "M101", "non-finite"), "{f:#?}");
    assert!(has(&f, "M144", "non-finite"), "the geometry that was skipped is fair play left unjudged: {f:#?}");
}

#[test]
fn m102_open_sheet_void() {
    // a single-sided bucket against the body, its top face missing: from above you look at its
    // culled inside and through it
    let mut p = boxd([10.0, -3.0, 20.0], [16.0, 3.0, 26.0], CLOTH, 1);
    p.tris.retain(|t| !(4..8).contains(&t[0]));
    let f = run_ours(&with(vec![p]));
    assert!(has(&f, "M102", "culled inside"), "{f:#?}");
}

/// `d` turned inside out and pulled `inset` units toward `c` along its normals: the back layer of
/// a double-sided piece made the way artists make one (duplicate, flip, push in).
fn twin_of(d: &SynthDraw, inset: f32) -> SynthDraw {
    let mut t = d.clone();
    for (p, n) in t.positions.iter_mut().zip(t.normals.iter_mut()) {
        for k in 0..3 {
            p[k] -= n[k] * inset;
            n[k] = -n[k];
        }
    }
    t.tris.iter_mut().for_each(|x| x.swap(1, 2));
    t
}

/// A curved single-sided train card hanging behind the body: an arc of radius 13 around the hero's
/// axis from 150 to 210 degrees, z 10..40, facing out, with a swallowtail notch cut from its hem.
fn train_card() -> SynthDraw {
    let mut d = SynthDraw { material: CLOTH.into(), ..Default::default() };
    let (cols, rows) = (8usize, 10usize);
    for i in 0..=cols {
        let a = (150.0 + 60.0 * i as f32 / cols as f32).to_radians();
        for j in 0..=rows {
            d.positions.push([13.0 * a.cos(), 13.0 * a.sin(), 10.0 + 3.0 * j as f32]);
            d.normals.push([a.cos(), a.sin(), 0.0]);
            d.uvs.push([0.05 + 0.9 * i as f32 / cols as f32, 0.05 + 0.9 * j as f32 / rows as f32]);
            d.joints.push([1, 0, 0, 0]);
            d.weights.push([1.0, 0.0, 0.0, 0.0]);
        }
    }
    let id = |i: usize, j: usize| (i * (rows + 1) + j) as u32;
    for i in 0..cols {
        for j in 0..rows {
            // the swallowtail: a V notch in the middle of the hem
            if j < 3 && (i == 3 || i == 4) || j < 1 && (i == 2 || i == 5) {
                continue;
            }
            // (+angle) x (+z) is the outward radial direction: wind counter-clockwise from outside
            let (a, b, c, e) = (id(i, j), id(i + 1, j), id(i + 1, j + 1), id(i, j + 1));
            d.tris.push([a, b, c]);
            d.tris.push([a, c, e]);
        }
    }
    d
}

#[test]
fn m102_twin_backed_sheets_are_closed() {
    // The bucket of m102 with an inward-facing lining 0.1 mm inside it: through its open top you see
    // the lining, never the culled inside. Its rims sit 0.1 mm apart unwelded, stacked: no crack.
    let mut p = boxd([10.5, -3.0, 20.0], [16.5, 3.0, 26.0], CLOTH, 1);
    p.tris.retain(|t| !(4..8).contains(&t[0]));
    let f = run_ours(&with(vec![p.clone()]));
    assert!(has(&f, "M102", "culled inside"), "the lone bucket shows its culled inside: {f:#?}");
    let mut lining = boxd([10.504, -2.996, 20.004], [16.496, 2.996, 26.0], CLOTH, 1);
    lining.tris.retain(|t| !(4..8).contains(&t[0]));
    let lining = SynthDraw { positions: lining.positions.clone(), ..twin_of(&lining, 0.0) };
    let f = run_ours(&with(vec![p.clone(), lining]));
    assert!(!f.iter().any(|x| ["M102", "M125", "M108", "M110"].contains(&x.code)), "{f:#?}");
    // A curved swallowtail train card: alone it shows its culled back; with its twin back layer
    // (0.1 mm behind, every triangle backed) it is one double-sided card.
    let card = train_card();
    let f = run_ours(&with(vec![card.clone()]));
    assert!(has(&f, "M102", "culled inside"), "the lone card must show its culled back: {f:#?}");
    let f = run_ours(&with(vec![card.clone(), twin_of(&card, 0.004)]));
    assert!(!f.iter().any(|x| ["M102", "M125"].contains(&x.code)), "{f:#?}");
    // Half a twin is not a twin: a back layer over half the card leaves the other half see-through.
    let mut half = twin_of(&card, 0.004);
    half.tris.truncate(half.tris.len() / 2);
    let f = run_ours(&with(vec![card, half]));
    assert!(has(&f, "M102", "culled inside"), "{f:#?}");
}

#[test]
fn m102_pinched_outlines_split_into_rings() {
    // a loop that visits vertex 1 twice (a figure eight) is two simple rings
    let rings = mesh::openings::simple_rings(&[0, 1, 2, 3, 1, 4, 5]);
    assert_eq!(rings, vec![vec![1, 2, 3], vec![0, 1, 4, 5]]);
    assert_eq!(mesh::openings::simple_rings(&[0, 1, 2, 3]), vec![vec![0, 1, 2, 3]]);
}

#[test]
fn m103_inconsistent_winding() {
    let mut extra = Vec::new();
    for k in 0..4 {
        let z = 10.0 + 10.0 * k as f32;
        let mut p = dense_box([10.0, -3.0, z], [12.0, 3.0, z + 6.0], CLOTH, 2);
        for t in p.tris.iter_mut().step_by(3) {
            t.swap(1, 2);
        }
        extra.push(p);
    }
    let f = run_ours(&with(extra));
    assert!(has(&f, "M103", "disagree on winding"), "{f:#?}");
}

#[test]
fn m104_flipped_normals() {
    let mut p = dense_box([10.0, -3.0, 20.0], [12.0, 3.0, 30.0], CLOTH, 3);
    p.normals.iter_mut().for_each(|n| *n = [-n[0], -n[1], -n[2]]);
    let f = run_ours(&with(vec![p]));
    assert!(has(&f, "M104", "shading normals"), "{f:#?}");
}

#[test]
fn m105_slivers() {
    // 60 needle triangles (10 units long, 0.05 wide) laid on the body like stitching
    let mut d = SynthDraw { material: CLOTH.into(), ..Default::default() };
    for i in 0..60u32 {
        let z = 5.0 + i as f32 * 0.8;
        for p in [[10.05, -5.0, z], [10.05, 5.0, z], [10.05, 0.0, z + 0.05]] {
            d.positions.push(p);
            d.normals.push([1.0, 0.0, 0.0]);
            d.uvs.push([0.9, 0.5]);
            d.joints.push([1, 0, 0, 0]);
            d.weights.push([1.0, 0.0, 0.0, 0.0]);
        }
        d.tris.push([3 * i, 3 * i + 1, 3 * i + 2]);
    }
    let f = run_ours(&with(vec![d]));
    assert!(has(&f, "M105", "sliver"), "{f:#?}");
}

#[test]
fn m106_cut_body_void() {
    // Valve's body with its +x face cut away and nothing behind the cut
    let mut b = body();
    b.tris.retain(|t| t[0] < 20);
    let f = run_ours(&hero(vec![b], Some(vec![body()])));
    assert!(has(&f, "M106", "cut-body opening"), "{f:#?}");
    assert_eq!(rule("M106").severity, Severity::Warn, "seen, not a model that fails to work");
}

/// Valve's body cut at z 40 (the top and everything above it gone: a ring of open edges, like the
/// torso cut under a bodice), with a single-sided band around it whose lower rim is 1.5 units below
/// the cut. A closed inner hull sunk 8 mm (0.31 units) under the original surface, if given.
fn ring_cut(hull: bool) -> SynthModel {
    let mut b = dense_box([-10.0, -10.0, 0.0], [10.0, 10.0, 60.0], BODY, 6);
    let keep: Vec<[u32; 3]> = b.tris.iter().copied().filter(|t| t.iter().map(|&v| b.positions[v as usize][2]).sum::<f32>() / 3.0 < 40.0).collect();
    b.tris = keep;
    let mut band = boxd([-10.15, -10.15, 38.5], [10.15, 10.15, 46.0], CLOTH, 1);
    band.tris.retain(|t| t[0] >= 8);
    let mut l0 = vec![b, band];
    if hull {
        l0.push(boxd([-9.69, -9.69, 0.31], [9.69, 9.69, 50.0], CLOTH, 1));
    }
    hero(l0, Some(vec![body()]))
}

#[test]
fn m106_reachable_views_and_hulls() {
    // Nothing behind the ring cut: once the band lifts, a player looking at the hero's side sees
    // into the hollow body.
    let f = run_ours(&ring_cut(false));
    assert!(has(&f, "M106", "cut-body opening"), "{f:#?}");
    // A closed hull sunk 8 mm under the cut backs every view a player can reach (the views along
    // the cut's cross-section, up inside the body, are not views a player has).
    let f = run_ours(&ring_cut(true));
    assert!(!f.iter().any(|x| x.code == "M106"), "{f:#?}");
}

#[test]
fn m107_two_sided_back_faces() {
    let mut p = boxd([10.0, -4.0, 20.0], [16.0, 4.0, 28.0], CLOTH, 1);
    p.tris.iter_mut().for_each(|t| t.swap(1, 2));
    let f = mesh_run(&with(vec![p]), vec![], vmat(&["F_USE_STATUS_EFFECTS_PROXY", "F_USE_NPR_LIGHTING", "F_RENDER_BACKFACES"], true, None));
    assert!(has(&f, "M107", "two-sided back faces"), "{f:#?}");
}

#[test]
fn m108_z_fighting() {
    // a patch lying exactly on the body's +x face, facing the same way
    let mut d = SynthDraw { material: CLOTH.into(), ..Default::default() };
    for (y, z) in [(-4.0f32, 20.0f32), (4.0, 20.0), (4.0, 28.0), (-4.0, 28.0)] {
        d.positions.push([10.0, y, z]);
        d.normals.push([1.0, 0.0, 0.0]);
        d.uvs.push([(y + 4.0) / 8.0 * 0.15 + 0.8, (z - 20.0) / 8.0 * 0.8 + 0.1]);
        d.joints.push([1, 0, 0, 0]);
        d.weights.push([1.0, 0.0, 0.0, 0.0]);
    }
    d.tris = vec![[0, 1, 2], [0, 2, 3]];
    let f = run_ours(&with(vec![d]));
    assert!(has(&f, "M108", "share a plane"), "{f:#?}");
}

/// Three quads in one shell, offset by `dy`; the third folds back through the first along 4 units.
fn fold(dy: f32) -> SynthDraw {
    let mut d = SynthDraw { material: CLOTH.into(), ..Default::default() };
    let pts: [P3; 8] = [[10.0, 0.0, 20.0], [16.0, 0.0, 20.0], [16.0, 4.0, 20.0], [10.0, 4.0, 20.0], [16.0, 4.0, 24.0], [16.0, 0.0, 24.0], [13.0, 4.0, 16.0], [13.0, 0.0, 16.0]];
    for p in pts {
        d.positions.push([p[0], p[1] + dy, p[2]]);
        d.normals.push([0.0, 0.0, 1.0]);
        d.uvs.push([0.5, 0.5]);
        d.joints.push([1, 0, 0, 0]);
        d.weights.push([1.0, 0.0, 0.0, 0.0]);
    }
    d.tris = vec![[0, 1, 2], [0, 2, 3], [1, 5, 4], [1, 4, 2], [5, 7, 6], [5, 6, 4]];
    d
}

/// The mesh verdict on our pouch model (the body and a pouch, about 5,800 u2 of surface, 76 units of size) measured,
/// with `n` visible crossings of `len` units each put in between shells `a` and `b` (the same shell for a fold).
fn judged_with_crossings(a: usize, b: usize, n: usize, len: f64) -> Vec<&'static str> {
    use crate::mesh::census::{no_env, Census, Cross};
    let m = with(vec![boxd([8.0, -3.0, 20.0], [14.0, 3.0, 26.0], CLOTH, 1)]);
    let mut c = Census::build(&::mesh::synth::model(&m), &no_env()).unwrap();
    let mut bounds = geom::v3::Aabb::empty();
    bounds.grow([10.0, -3.0, 20.0]);
    bounds.grow([10.0, 3.0, 26.0]);
    c.crosses = (0..n).map(|_| Cross { a, b, pairs: 40, length: len, bounds, depth_a_in_b: 2.0, depth_b_in_a: 0.0, visible_pairs: 20 }).collect();
    rules::judge(&c, None, &policy::DEFAULT).into_iter().map(|x| x.0).collect()
}

#[test]
fn m109_self_intersection() {
    // the line sits at Valve's most (Frank's 4.81 sizes of fold line): just past it fires, just under is Valve's own
    let size = judged_size().sqrt();
    assert!(judged_with_crossings(1, 1, 10, 0.1 * 4.9 * size).contains(&"M109"));
    assert!(!judged_with_crossings(1, 1, 10, 0.1 * 4.7 * size).contains(&"M109"));
}

#[test]
fn m109_silent_where_valve_folds_as_much() {
    // one fold on a hero-sized body: under Valve's own rate (the median hero has 0.4 sizes of fold line)
    let f = run_ours(&with(vec![fold(0.0)]));
    assert!(!f.iter().any(|x| x.code == "M109"), "{f:#?}");
}

#[test]
fn m110_shell_intersection() {
    // the line sits at Valve's most (Hornet's 52.5 sizes of crossing line): past it fires, under it is Valve's own
    let size = judged_size().sqrt();
    assert!(judged_with_crossings(1, 0, 40, 53.5 * size / 40.0).contains(&"M110"));
    assert!(!judged_with_crossings(1, 0, 40, 51.5 * size / 40.0).contains(&"M110"));
}

/// The pouch model's surface.
fn judged_size() -> f64 {
    let m = with(vec![boxd([8.0, -3.0, 20.0], [14.0, 3.0, 26.0], CLOTH, 1)]);
    let c = crate::mesh::census::Census::build(&::mesh::synth::model(&m), &crate::mesh::census::no_env()).unwrap();
    c.shells.iter().map(|s| s.area).sum()
}

#[test]
fn m110_silent_on_construction_like_valves() {
    // Real case (6871): every one of Valve's 43 heroes has pieces sunk 5 units or more into others at bind (a
    // pouch into the body, a buckle into a belt), about 250 pairs a hero. One sunk pouch is construction.
    let f = run_ours(&with(vec![boxd([8.0, -3.0, 20.0], [14.0, 3.0, 26.0], CLOTH, 1)]));
    assert!(!f.iter().any(|x| x.code == "M110"), "{f:#?}");
}

#[test]
fn m111_floating_shell() {
    // an arm band 2 units off the body
    let f = run_ours(&with(vec![boxd([12.0, -3.0, 20.0], [14.0, 3.0, 22.0], CLOTH, 1)]));
    assert!(has(&f, "M111", "touches nothing"), "{f:#?}");
    // resting on the body but skinned to the head (which carries the head itself): lifts off when the spine and
    // head part
    let f = run_ours(&with(vec![boxd([10.0, -3.0, 20.0], [12.0, 3.0, 30.0], CLOTH, 2), head()]));
    assert!(has(&f, "M111", "skinned to other bones"), "{f:#?}");
    // a ring on a bone of its own is a part the rig moves on purpose
    let f = run_ours(&with(vec![boxd([10.0, -3.0, 20.0], [12.0, 3.0, 30.0], CLOTH, 2)]));
    assert!(!has(&f, "M111", "skinned to other bones"), "{f:#?}");
}

#[test]
fn m111_silent_under_a_pixel_and_at_a_hinge() {
    // 5 mm off the body: under one fair-play pixel, as 108 of Valve's own pieces stand (6871)
    let f = run_ours(&with(vec![boxd([10.2, -3.0, 20.0], [12.0, 3.0, 22.0], CLOTH, 1)]));
    assert!(!f.iter().any(|x| x.code == "M111"), "{f:#?}");
    // a collar on the head bone sitting on the body right at the neck pivot: the bend turns it about where it
    // touches (a hand at its cuff, a finger on the next), so it does not lift off
    let collar = || boxd([-3.0, -3.0, 60.0], [3.0, 3.0, 64.0], CLOTH, 2);
    let mut m = with(vec![collar(), head()]);
    m.skeleton[2].2 = [0.0, 0.0, 30.0];
    let f = run_ours(&m);
    assert!(!has(&f, "M111", "skinned to other bones"), "{f:#?}");
    // the same collar with the neck pivot 25 units down still lifts off
    let f = run_ours(&with(vec![collar(), head()]));
    assert!(has(&f, "M111", "skinned to other bones"), "{f:#?}");
}

#[test]
fn m112_coordinate_precision() {
    // 0.3 units thick at 5e5 units from the origin, where float32 steps are ~0.06: under eight steps, each a
    // quarter of a pixel
    let f = run_ours(&with(vec![boxd([500000.0, -3.0, 20.0], [500000.3, 3.0, 26.0], CLOTH, 1)]));
    assert!(has(&f, "M112", "float32"), "{f:#?}");
}

#[test]
fn m112_silent_on_a_flat_card_near_the_origin() {
    // Real case (6871): a flat lace card on a skin's bodice, horizontal at z 80 and 105 units from the origin, is
    // 1.6e-5 units thick: float32 rounding a plane, never a shape a player could see lost.
    let card = boxd([2.0, -3.0, 80.0], [8.0, 3.0, 80.000016], CLOTH, 1);
    let f = run_ours(&with(vec![card]));
    assert!(!f.iter().any(|x| x.code == "M112"), "{f:#?}");
}

#[test]
fn m113_degenerate_uvs() {
    let mut d = SynthDraw { material: CLOTH.into(), ..Default::default() };
    let n = 13;
    for i in 0..=n {
        for j in 0..=1 {
            d.positions.push([10.05, -6.0 + i as f32, 20.0 + 4.0 * j as f32]);
            d.normals.push([1.0, 0.0, 0.0]);
            d.uvs.push([0.82 + 0.012 * i as f32, 0.2 + 0.5 * j as f32]);
            d.joints.push([1, 0, 0, 0]);
            d.weights.push([1.0, 0.0, 0.0, 0.0]);
        }
    }
    for i in 0..n as u32 {
        let (a, b, c, e) = (2 * i, 2 * i + 2, 2 * i + 3, 2 * i + 1);
        d.tris.push([a, b, c]);
        d.tris.push([a, c, e]);
    }
    // collapse the UV of every second column's top vertex onto its bottom one: zero UV area
    for i in 1..n {
        if i % 2 == 0 {
            let bottom = d.uvs[2 * i];
            d.uvs[2 * i + 1] = bottom;
        }
    }
    let f = run_ours(&with(vec![d]));
    assert!(has(&f, "M113", "zero UV area"), "{f:#?}");
}

#[test]
fn m114_uv_overlap() {
    let mut a = boxd([10.0, -3.0, 20.0], [12.0, 3.0, 30.0], CLOTH, 1);
    let mut b = boxd([10.0, -3.0, 32.0], [12.0, 3.0, 42.0], CLOTH, 1);
    for d in [&mut a, &mut b] {
        for (k, uv) in d.uvs.iter_mut().enumerate() {
            *uv = [if k % 4 == 1 || k % 4 == 2 { 0.9 } else { 0.1 }, if k % 4 >= 2 { 0.9 } else { 0.1 }];
        }
    }
    let f = run_ours(&with(vec![a, b]));
    assert!(has(&f, "M114", "claimed twice"), "{f:#?}");
}

#[test]
fn m115_uv_stretch() {
    let mut a = boxd([10.0, -10.0, 5.0], [12.0, 10.0, 55.0], CLOTH, 1);
    for (k, uv) in a.uvs.iter_mut().enumerate() {
        uv[0] = 0.5 + if k % 4 == 1 || k % 4 == 2 { 0.003 } else { 0.0 };
    }
    let f = run_ours(&with(vec![a]));
    assert!(has(&f, "M115", "stretched over 4:1"), "{f:#?}");
}

#[test]
fn m116_unnormalised_weights() {
    let mut p = boxd([10.0, -3.0, 20.0], [12.0, 3.0, 30.0], CLOTH, 1);
    p.weights.iter_mut().for_each(|w| *w = [0.5, 0.0, 0.0, 0.0]);
    let f = run_ours(&with(vec![p]));
    assert!(has(&f, "M116", "do not sum to 1"), "{f:#?}");
}

#[test]
fn m117_noise_influences() {
    let mut p = dense_box([10.0, -3.0, 20.0], [12.0, 3.0, 30.0], CLOTH, 5);
    p.joints.iter_mut().for_each(|j| *j = [1, 2, 0, 0]);
    p.weights.iter_mut().for_each(|w| *w = [0.996, 0.004, 0.0, 0.0]);
    let f = run_ours(&with(vec![p]));
    assert!(has(&f, "M117", "under 1.5/255"), "{f:#?}");
}

#[test]
fn m118_distant_bone() {
    // a hem at the ankles weighted half to the head
    let mut p = boxd([10.0, -3.0, 2.0], [12.0, 3.0, 6.0], CLOTH, 1);
    p.joints.iter_mut().for_each(|j| *j = [1, 2, 0, 0]);
    p.weights.iter_mut().for_each(|w| *w = [0.5, 0.5, 0.0, 0.0]);
    let f = run_ours(&with(vec![p]));
    assert!(has(&f, "M118", "'head'"), "{f:#?}");
}

#[test]
fn m119_unknown_bone() {
    let mut p = boxd([10.0, -3.0, 20.0], [12.0, 3.0, 30.0], CLOTH, 3);
    p.joints.iter_mut().for_each(|j| *j = [3, 0, 0, 0]);
    let f = run_ours(&with(vec![p]));
    assert!(has(&f, "M119", "skeleton lacks"), "{f:#?}");
}

#[test]
fn m120_missing_lod() {
    let f = run_ours(&hero(vec![body()], None));
    assert!(has(&f, "M120", "1 LOD levels"), "{f:#?}");
}

#[test]
fn m121_lod_silhouette() {
    let f = run_ours(&hero(vec![body()], Some(vec![boxd([-5.0, -5.0, 0.0], [5.0, 5.0, 60.0], BODY, 1)])));
    assert!(has(&f, "M121", "silhouette matches LOD0"), "{f:#?}");
}

#[test]
fn m122_lod_budget() {
    let f = run_ours(&hero(vec![body()], Some(vec![dense_box([-10.0, -10.0, 0.0], [10.0, 10.0, 60.0], BODY, 30)])));
    assert!(has(&f, "M122", "LOD1"), "{f:#?}");
}

#[test]
fn m123_mesh_budget() {
    let f = run_ours(&hero(vec![dense_box([-10.0, -10.0, 0.0], [10.0, 10.0, 60.0], BODY, 30)], Some(vec![body()])));
    assert!(has(&f, "M123", "triangles vs"), "{f:#?}");
}

#[test]
fn m124_silhouette_fair_play() {
    let f = run_ours(&hero(vec![boxd([-8.0, -8.0, 0.0], [8.0, 8.0, 60.0], BODY, 1)], Some(vec![body()])));
    assert!(has(&f, "M124", "harder to see"), "{f:#?}");
    assert_eq!(rule("M124").severity, Severity::Error);
}

#[test]
fn m124_judges_what_bullets_can_hit() {
    // Real case (6871, community mods): a thinner trooper is a target harder to see; a thinner ability prop (a
    // teapot, a magazine, a trooper's break-off piece) is an effect, and Valve's model for it has no hitboxes.
    let thin = hero(vec![boxd([-8.0, -8.0, 0.0], [8.0, 8.0, 60.0], BODY, 1)], Some(vec![body()]));
    let target = mesh_run_against(&thin, &valve(), vec![], hero_vmat(), &policy::DEFAULT);
    assert!(has(&target, "M124", "target is harder to see"), "{target:#?}");
    let mut prop = valve();
    prop.hitboxes.clear();
    let effect = mesh_run_against(&thin, &prop, vec![], hero_vmat(), &policy::DEFAULT);
    assert!(!has(&effect, "M124", ""), "{effect:#?}");
}

#[test]
fn m125_seam_crack() {
    let strip = |x0: f32, x1: f32| {
        let mut d = SynthDraw { material: CLOTH.into(), ..Default::default() };
        let n = 30;
        for i in 0..=n {
            for x in [x0, x1] {
                d.positions.push([x, 10.05, 10.0 + i as f32]);
                d.normals.push([0.0, 1.0, 0.0]);
                d.uvs.push([0.9, 0.5]);
                d.joints.push([1, 0, 0, 0]);
                d.weights.push([1.0, 0.0, 0.0, 0.0]);
            }
        }
        for i in 0..n as u32 {
            d.tris.push([2 * i, 2 * i + 2, 2 * i + 3]);
            d.tris.push([2 * i, 2 * i + 3, 2 * i + 1]);
        }
        d
    };
    let f = run_ours(&with(vec![strip(0.0, 2.0), strip(2.005, 4.0)]));
    assert!(has(&f, "M125", "without welding"), "{f:#?}");
}

#[test]
fn m126_texture_without_uv() {
    let mut m = with(vec![]);
    m.meshes.push(SynthMesh { name: "pouch".into(), lod_mask: 1, draws: vec![boxd([10.0, -3.0, 20.0], [12.0, 3.0, 30.0], CLOTH, 1)], bones: bones(), no_uv: true });
    let f = run_ours(&m);
    assert!(has(&f, "M126", "carries no UVs"), "{f:#?}");
}

#[test]
fn m127_unreferenced_vertices() {
    let mut p = boxd([10.0, -3.0, 20.0], [12.0, 3.0, 30.0], CLOTH, 1);
    for _ in 0..5 {
        p.positions.push([11.0, 0.0, 25.0]);
        p.normals.push([1.0, 0.0, 0.0]);
        p.uvs.push([0.5, 0.5]);
        p.joints.push([1, 0, 0, 0]);
        p.weights.push([1.0, 0.0, 0.0, 0.0]);
    }
    let f = run_ours(&with(vec![p]));
    assert!(has(&f, "M127", "no triangle uses"), "{f:#?}");
}

#[test]
fn m128_unused_material() {
    let mut m = with(vec![]);
    m.extra_materials.push("materials/test/orphan.vmat".into());
    let f = run_ours(&m);
    assert!(has(&f, "M128", "orphan.vmat"), "{f:#?}");
}

#[test]
fn m129_hitbox_envelope() {
    // a flared hem 15 units beyond the capsule, visible: the worst point is under the 18-unit line
    // and the area is a sliver, so it is the taste tier (M140), not the error
    let f = run_ours(&with(vec![boxd([26.0, -3.0, 20.0], [30.0, 3.0, 26.0], CLOTH, 1)]));
    assert!(has(&f, "M140", "magician.allow"), "{f:#?}");
    assert!(!has(&f, "M129", ""), "{f:#?}");
    // a train dragging 20+ units out: a bit much, whatever its area
    let f = run_ours(&with(vec![boxd([31.0, -1.0, 20.0], [33.0, 1.0, 22.0], CLOTH, 1)]));
    assert!(has(&f, "M129", "can't be hit"), "{f:#?}");
    assert!(has(&f, "M129", "bit much"), "{f:#?}");
    assert!(has(&f, "M129", "does not waive"), "{f:#?}");
    assert!(!has(&f, "M140", ""), "one finding per model: the error replaces the warning: {f:#?}");
    assert_eq!(rule("M129").severity, Severity::Error);
    assert_eq!(rule("M140").severity, Severity::Warn);
}

#[test]
fn m140_hitbox_taste_tier() {
    // a small piece 12 units out on about 0.5% of the area: taste warning only, naming region, point and share
    let f = run_ours(&with(vec![boxd([25.5, -1.5, 20.0], [26.5, 1.5, 23.0], CLOTH, 1)]));
    let m = f.iter().find(|x| x.code == "M140").unwrap_or_else(|| panic!("{f:#?}"));
    assert!(m.message.contains("capsule") && m.message.contains("worst point") && m.message.contains("accept with a reason in magician.allow if the silhouette piece earns it"), "{}", m.message);
    assert!(m.message.contains("worst point 12.50 u") && m.message.contains("(0.53% of it)") && m.message.contains("under the 18 u / 3.5% line"), "{}", m.message);
    assert!(!has(&f, "M129", ""), "{f:#?}");
}

#[test]
fn a_model_the_game_never_loads_is_not_judged_by_the_model_rules() {
    // Real case (6871, a community skin): a copy of the hero under panorama/ that nothing names and no game file is.
    // M031 says it never loads; M129 said it was a bit much, a fair-play failure no allowance waives, on a model no one
    // sees. Named by a particle, the same model draws in game and is judged.
    let train = with(vec![boxd([31.0, -1.0, 20.0], [33.0, 1.0, 22.0], CLOTH, 1)]);
    let copy = "panorama/images/heroes/hero_names/lash_default.vmdl_c";
    let fx = "particles/abilities/lash/lash_ult.vpcf_c";
    let mut g = game();
    with_game(&mut g, &[MODEL, fx]);
    let run = |files: Vec<ModFile>| {
        let mut fake = Fake::default();
        fake.files.insert(MODEL.into(), ::mesh::synth::model(&valve()));
        lint(&Input { vpk_name: "pak03_dir.vpk".into(), files, base: Some(&g), built_against: None, original_refs: no_originals(), game: Some(&fake), deep: true, fair_play: &policy::DEFAULT })
    };
    let alone = run(vec![file(copy, ::mesh::synth::model(&train)), file(&format!("{CLOTH}_c"), hero_vmat())]);
    assert!(has(&alone, "M031", "") && !alone.iter().any(|x| x.code.starts_with("M1")), "{alone:#?}");
    let named = run(vec![file(copy, ::mesh::synth::model(&train)), file(&format!("{CLOTH}_c"), hero_vmat()), file(fx, res(&["panorama/images/heroes/hero_names/lash_default.vmdl"]))]);
    assert!(has(&named, "M129", "bit much"), "{named:#?}");
}

#[test]
fn m129_share_beyond_ten_is_egregious() {
    // a broad panel 11-12 units out (worst point under 18) over about 4% of the visible surface
    let f = run_ours(&with(vec![boxd([25.0, -6.0, 20.0], [25.2, 6.0, 30.0], CLOTH, 1)]));
    assert!(has(&f, "M129", "(4.25% of it)") && has(&f, "M129", "over 3.5% of the visible surface"), "{f:#?}");
    assert!(has(&f, "M129", "bit much"), "{f:#?}");
    assert!(!has(&f, "M140", ""), "{f:#?}");
}

/// The worst point and the visible share an M129 or M140 finding measured.
fn envelope_of(f: &[Finding]) -> (f64, f64) {
    let m = f.iter().find(|x| x.code == "M129" || x.code == "M140").unwrap_or_else(|| panic!("{f:#?}"));
    rules::envelope_measures(&m.message).unwrap_or_else(|| panic!("unreadable: {}", m.message))
}

#[test]
fn the_egregious_line_moves_with_the_policy_just_under_at_and_just_over() {
    // the taste-tier hem of m140_hitbox_taste_tier: worst point 12.50 u, 0.53% of the visible surface
    let hem = with(vec![boxd([25.5, -1.5, 20.0], [26.5, 1.5, 23.0], CLOTH, 1)]);
    let (worst, share) = envelope_of(&mesh_run(&hem, vec![], hero_vmat()));
    assert!((worst - 12.5).abs() < 1e-9 && (share - 0.0053).abs() < 1e-9, "{worst} {share}");
    let at = |units: f64| {
        let mut p = policy::DEFAULT;
        p.hitbox_egregious_units = units;
        mesh_run_at(&hem, vec![], hero_vmat(), &p)
    };
    // the line just under the worst point: a bit much (M129); just over: the taste tier (M140). The point itself sits a
    // float's width above 12.5 here, so "exactly at the line" is pinned on a hand-built envelope below.
    let under = at(12.49);
    assert!(has(&under, "M129", "worst point over 12.49 u") && !has(&under, "M140", ""), "{under:#?}");
    for units in [12.51, 13.0] {
        let f = at(units);
        assert!(has(&f, "M140", &format!("under the {units} u / 3.5% line")) && !has(&f, "M129", ""), "{units}: {f:#?}");
    }
    // the share line the same way: just under the hem's 0.53% fails, just over passes
    let share_at = |s: f64| {
        let mut p = policy::DEFAULT;
        p.hitbox_egregious_share = s;
        mesh_run_at(&hem, vec![], hero_vmat(), &p)
    };
    assert!(has(&share_at(0.0052), "M129", "over 0.52% of the visible surface"));
    assert!(has(&share_at(0.0054), "M140", "/ 0.54% line"));
    // and the taste line itself: past the hem's worst point nothing is said at all
    let mut p = policy::DEFAULT;
    p.hitbox_taste_units = 12.6;
    let f = mesh_run_at(&hem, vec![], hero_vmat(), &p);
    assert!(!has(&f, "M140", "") && !has(&f, "M129", ""), "{f:#?}");
}

#[test]
fn the_silhouette_line_moves_with_the_policy() {
    let thin = hero(vec![boxd([-8.0, -8.0, 0.0], [8.0, 8.0, 60.0], BODY, 1)], Some(vec![body()]));
    assert!(has(&mesh_run(&thin, vec![], hero_vmat()), "M124", "harder to see"));
    let mut p = policy::DEFAULT;
    p.silhouette_min = 0.5;
    assert!(!has(&mesh_run_at(&thin, vec![], hero_vmat(), &p), "M124", ""), "a policy that accepts half of Valve's silhouette");
}

#[test]
fn envelope_messages_read_back_exactly() {
    let e = Envelope {
        area: 100.0,
        visible_area: 100.0,
        visible_worst: (15.0, [1.0, 2.0, 3.0], "chest".into()),
        capsules: 1,
        tris: vec![(15.0, 2.0, true, 0), (1.0, 98.0, true, 0)],
        caps: vec![("chest".into(), 15.0, [1.0, 2.0, 3.0])],
        visible_caps: vec![(15.0, [1.0, 2.0, 3.0])],
        ..Default::default()
    };
    let f = rules::envelope_verdict(&e, None, &policy::DEFAULT);
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].0, "M140");
    assert_eq!(rules::envelope_measures(&f[0].1), Some((15.0, 0.02)));
    // the same envelope at a 14.99 u line is the error, and a 15 u line keeps it the taste tier (strictly over)
    let mut p = policy::DEFAULT;
    p.hitbox_egregious_units = 14.99;
    assert_eq!(rules::envelope_verdict(&e, None, &p)[0].0, "M129");
    p.hitbox_egregious_units = 15.0;
    assert_eq!(rules::envelope_verdict(&e, None, &p)[0].0, "M140");
    // M130's share reads back too
    let wide = Envelope { tris: vec![(6.0, 30.0, true, 0), (1.0, 70.0, true, 0)], visible_worst: (6.0, [0.0; 3], "chest".into()), visible_caps: vec![(6.0, [0.0; 3])], ..e };
    let w = rules::envelope_verdict(&wide, None, &policy::DEFAULT);
    assert_eq!(w.iter().map(|x| x.0).collect::<Vec<_>>(), vec!["M130"]);
    assert_eq!(rules::wide_measure(&w[0].1), Some(0.3));
    assert!(w[0].1.contains("Valve's own -, not counted;"), "no Valve envelope: a dash: {}", w[0].1);
    let with_valve = rules::envelope_verdict(&wide, Some(&Envelope { area: 10.0, capsules: 1, ..Default::default() }), &policy::DEFAULT);
    assert!(with_valve[0].1.contains("Valve's own 0%, not counted;"), "{}", with_valve[0].1);
}

#[test]
fn m144_unjudged_geometry() {
    // a hero model without capsule hitboxes where Valve's has them: the envelope cannot be measured
    let mut bare = valve();
    bare.hitboxes.clear();
    let f = run_ours(&bare);
    assert!(has(&f, "M144", "no capsule hitboxes"), "{f:#?}");
    // a replaced hero model whose geometry is not embedded, where Valve's is
    let mut g = game();
    with_game(&mut g, &[MODEL]);
    let mut fake = Fake::default();
    fake.files.insert(MODEL.into(), ::mesh::synth::model(&valve()));
    let f = run_game(vec![file(MODEL, res(&[]))], &g, &fake);
    assert!(has(&f, "M144", "no embedded meshes where Valve's does"), "{f:#?}");
    assert_eq!(rule("M144").severity, Severity::Warn);
}

#[test]
fn m130_hitbox_envelope_wide() {
    // a broad panel 6-9 units beyond the capsule over a quarter of the surface
    let f = run_ours(&with(vec![boxd([20.0, -10.0, 10.0], [21.0, 10.0, 50.0], CLOTH, 1)]));
    assert!(has(&f, "M130", "Lady Geist"), "{f:#?}");
    assert!(!has(&f, "M129", "") && !has(&f, "M140", ""), "6-9 units is the M130 band, not the 10-unit taste line");
}

#[test]
fn m131_m132_m133_material_audit() {
    let piece = with(vec![boxd([10.0, -3.0, 20.0], [12.0, 3.0, 30.0], CLOTH, 1)]);
    let f = mesh_run(&piece, vec![], vmat(&["F_USE_NPR_LIGHTING"], true, None));
    assert!(has(&f, "M131", "F_USE_STATUS_EFFECTS_PROXY"), "{f:#?}");
    let f = mesh_run(&piece, vec![], vmat(&["F_USE_STATUS_EFFECTS_PROXY"], true, None));
    assert!(has(&f, "M132", "F_USE_NPR_LIGHTING"), "{f:#?}");
    let f = mesh_run(&piece, vec![], vmat(&["F_USE_STATUS_EFFECTS_PROXY", "F_USE_NPR_LIGHTING", "F_ALPHA_TEST"], true, Some(1.0)));
    assert!(has(&f, "M133", "outline mask"), "{f:#?}");
    let f = mesh_run(&piece, vec![], vmat(&["F_USE_STATUS_EFFECTS_PROXY", "F_USE_NPR_LIGHTING", "F_ALPHA_TEST"], true, Some(0.0)));
    assert!(!has(&f, "M133", ""), "a zero outline mask is the fix");
}

#[test]
fn m152_a_material_flag_on_the_avoid_list() {
    let piece = with(vec![boxd([10.0, -3.0, 20.0], [12.0, 3.0, 30.0], CLOTH, 1)]);
    // a new material that glows on a hero whose Valve materials never do: fair play, and the message says the fix
    let f = mesh_run(&piece, vec![], vmat(&["F_USE_STATUS_EFFECTS_PROXY", "F_USE_NPR_LIGHTING", "F_SELF_ILLUM"], true, None));
    assert!(f.iter().any(|x| x.code == "M152" && x.message.contains("F_SELF_ILLUM on") && x.message.contains("the surface glows") && x.message.contains("turn it off")), "{f:#?}");
    // the same material without it, and Valve's own model against itself: silent
    assert!(!has(&run_ours(&piece), "M152", ""));
    assert!(!has(&run_ours(&valve()), "M152", ""));
    // a flag one of Valve's materials on the model has is Valve's design; a model with no hitboxes is no target
    let info = |flags: &[&str]| crate::mesh::MatInfo { found: true, flags: flags.iter().map(|s| s.to_string()).collect(), ..Default::default() };
    let ours = vec![("materials/new.vmat".to_string(), info(&["F_TRANSLUCENT", "F_DISABLE_NPR_OUTLINE"]))];
    let valve_mats = vec![("materials/valve.vmat".to_string(), info(&["F_TRANSLUCENT"]))];
    let got: Vec<String> = rules::audit_materials(&ours, &valve_mats, true).into_iter().filter(|x| x.0 == "M152").map(|x| x.1).collect();
    assert!(got.len() == 1 && got[0].contains("F_DISABLE_NPR_OUTLINE") && got[0].contains("loses the outline"), "{got:?}");
    assert!(rules::audit_materials(&ours, &valve_mats, false).iter().all(|x| x.0 != "M152"), "not a target");
    // Valve's own material kept at the model is never judged
    let kept = vec![("materials/valve.vmat".to_string(), info(&["F_SELF_ILLUM"]))];
    assert!(rules::audit_materials(&kept, &valve_mats, true).iter().all(|x| x.0 != "M152"));
}

#[test]
fn m153_a_draw_with_no_material() {
    let at = |m: &str| with(vec![boxd([10.0, -3.0, 20.0], [12.0, 3.0, 30.0], m, 1)]);
    for (m, why) in [("nada.vmat", "a bare name with no folder"), ("materials/error.vmat", "the engine's error material"), ("", "no material at all")] {
        let f = run_ours(&at(m));
        assert!(f.iter().any(|x| x.code == "M153" && x.severity == Severity::Error && x.message.contains(why) && x.message.contains("assign the piece a real material")), "{m}: {f:#?}");
    }
    // a material with a folder that is missing is M030's, not this rule's; a real material is silent
    assert!(!has(&run_ours(&at("materials/test/gone.vmat")), "M153", ""));
    assert!(!has(&run_ours(&at(CLOTH)), "M153", ""));
    // a draw Valve's model at the same path makes with the same material is Valve's design
    let f = mesh_run_against(&at("materials/error.vmat"), &at("materials/error.vmat"), vec![], hero_vmat(), &policy::DEFAULT);
    assert!(!has(&f, "M153", ""), "{f:#?}");
}

#[test]
fn m134_particle_prop_unlit() {
    let shard = "materials/test/shard.vmat";
    let pcf = kv(&format!(r#"{{ _class = "CParticleSystemDefinition" m_Renderers = [ {{ _class = "C_OP_RenderModels" m_hOverrideMaterial = "{shard}" }} ] }}"#));
    let g = game();
    let files = vec![file("particles/test/shards.vpcf_c", res_kv(&[], &[(b"DATA", pcf.clone())])), file(&format!("{shard}_c"), vmat(&["F_USE_STATUS_EFFECTS_PROXY"], true, None))];
    let f = run_game(files, &g, &Fake::default());
    assert!(has(&f, "M134", "renders black"), "{f:#?}");
    let files = vec![file("particles/test/shards.vpcf_c", res_kv(&[], &[(b"DATA", pcf)])), file(&format!("{shard}_c"), vmat(&["F_USE_NPR_LIGHTING"], true, None))];
    assert!(!has(&run_game(files, &g, &Fake::default()), "M134", ""), "the NPR path is the fix");
}

#[test]
fn m135_texture_parity() {
    let card = "panorama/images/heroes/lash_card_psd.vtex_c";
    let mut g = game();
    with_game(&mut g, &[card]);
    let mut hdr = resource::texture::synth_header(256, 256, 28);
    hdr[27] = 9;
    let mut fake = Fake::default();
    fake.files.insert(card.into(), resource::synth_with(&[], &[(*b"DATA", hdr)]));
    let f = run_game(vec![file(card, tex(256, 256, 28))], &g, &fake);
    assert!(has(&f, "M135", "1 mips, Valve's 9"), "{f:#?}");
}

#[test]
fn m135_judges_what_shows_where_valve_draws_it() {
    use resource::texture::{info, BGRA8888, DXT1};
    let t = |w: u16, format: u8, mips: u8| {
        let mut h = resource::texture::synth_header(w, w, format);
        h[27] = mips;
        info(&resource::synth_with(&[], &[(*b"DATA", h)])).unwrap()
    };
    let valve = t(256, BGRA8888, 9);
    let png = resource::texture::FORMATS.iter().position(|f| *f == "PNG_RGBA8888").unwrap() as u8;
    // Real case (6871, 148 community UI images): PNG_RGBA8888 where Valve's is BGRA8888 keeps every pixel; larger is fine
    assert_eq!(rules::texture_parity("p", &t(256, png, 9), &valve), None);
    assert_eq!(rules::texture_parity("p", &t(512, BGRA8888, 10), &valve), None);
    // the true catches: a lossier format, a smaller image, a missing mip chain
    assert!(rules::texture_parity("p", &t(256, DXT1, 9), &valve).is_some_and(|x| x.1.contains("block artefacts")));
    assert!(rules::texture_parity("p", &t(128, BGRA8888, 8), &valve).is_some_and(|x| x.1.contains("blurs") && !x.1.contains("mips")));
    assert!(rules::texture_parity("p", &t(512, BGRA8888, 1), &valve).is_some_and(|x| x.1.contains("1 mips, Valve's 9")));
}

#[test]
fn m135_silent_on_a_morph_atlas_shipped_with_its_model() {
    // Real case (6871, a third skin): a replaced face's morph atlas is 186x186 where Valve's is 388x388, because the
    // compiler packs that model's own morph targets. Shipped with the model it is right; alone it is Valve's model
    // reading another atlas, and still judged.
    let atlas = "models/heroes_wip/lash/lash/head_vmorf.vtex_c";
    let mut g = game();
    with_game(&mut g, &[atlas, MODEL]);
    let mut fake = Fake::default();
    fake.files.insert(atlas.into(), resource::synth_with(&[], &[(*b"DATA", resource::texture::synth_header(388, 388, 28))]));
    let alone = run_game(vec![file(atlas, tex(186, 186, 28))], &g, &fake);
    assert!(has(&alone, "M135", "186x186"), "{alone:#?}");
    let with_model = run_game(vec![file(atlas, tex(186, 186, 28)), file(MODEL, res(&[]))], &g, &fake);
    assert!(!has(&with_model, "M135", ""), "{with_model:#?}");
}


#[test]
fn m136_m137_albedo() {
    let piece = with(vec![boxd([10.0, -10.0, 5.0], [14.0, 10.0, 55.0], CLOTH, 1)]);
    // pure white: outside Valve's range and absolute white
    let f = mesh_run(&piece, vec![file(&format!("{CLOTH_COLOR}_c"), flat(16, [255, 255, 255, 255]))], hero_vmat());
    assert!(has(&f, "M136", "outside Valve's albedo range"), "{f:#?}");
    assert!(has(&f, "M137", "absolute white"), "{f:#?}");
    // within Valve's band: silent
    let f = mesh_run(&piece, vec![file(&format!("{CLOTH_COLOR}_c"), flat(16, [100, 100, 100, 255]))], hero_vmat());
    assert!(!has(&f, "M136", "") && !has(&f, "M137", ""), "{f:#?}");
    // a look, not a fault: 12 of Valve's 78 hero models with hitboxes are past this line on their own art (6880)
    assert_eq!(rule("M137").severity, Severity::Warn);
}

fn checker(w: usize) -> Vec<u8> {
    vtex(w, w, (0..w * w).map(|i| [200, 200, 200, if (i % w + i / w).is_multiple_of(2) { 255 } else { 0 }]).collect())
}

#[test]
fn m138_m139_alpha_coverage() {
    let lace = vmat(&["F_USE_STATUS_EFFECTS_PROXY", "F_USE_NPR_LIGHTING", "F_ALPHA_TEST"], true, Some(0.0));
    let piece = with(vec![boxd([10.0, -1.0, 20.0], [12.0, 1.0, 30.0], CLOTH, 1)]);
    // a 1-texel checker: 50% coverage at mip 0, none once box-filtered to mip 1
    let f = mesh_run(&piece, vec![file(&format!("{CLOTH_COLOR}_c"), checker(256))], lace.clone());
    assert!(has(&f, "M139", "at 4 m"), "{f:#?}");
    let f = mesh_run(&piece, vec![file(&format!("{CLOTH_COLOR}_c"), checker(128))], lace.clone());
    assert!(has(&f, "M138", "at 8 m") && !has(&f, "M139", ""), "{f:#?}");
    // solid alpha keeps its coverage at every mip
    let f = mesh_run(&piece, vec![file(&format!("{CLOTH_COLOR}_c"), flat(256, [200, 200, 200, 255]))], lace);
    assert!(!has(&f, "M138", "") && !has(&f, "M139", ""), "{f:#?}");
    // a look the game draws as it is given: a warning, as M138 is
    assert_eq!(rule("M139").severity, Severity::Warn);
}


#[test]
fn mesh_family_cost_is_bounded() {
    // A 10,800-triangle hero against Valve's: the deep family must stay well inside a lint budget,
    // and the default (light) lint must not run the geometry census at all.
    let ours = hero(vec![dense_box([-10.0, -10.0, 0.0], [10.0, 10.0, 60.0], BODY, 30), boxd([10.0, -3.0, 20.0], [12.0, 3.0, 30.0], CLOTH, 1)], Some(vec![body()]));
    let t = std::time::Instant::now();
    let f = run_ours(&ours);
    let deep = t.elapsed();
    // 0.3 s on 12 threads, 0.8 s on one (MAGICIAN_THREADS=1): 5 s is a regression, not noise
    assert!(deep.as_secs_f64() < 5.0, "deep mesh family took {deep:?}");
    assert!(has(&f, "M123", ""), "{f:#?}");
    let mut g = game();
    let mut fake = Fake::default();
    fake.files.insert(MODEL.into(), ::mesh::synth::model(&valve()));
    with_game(&mut g, &[MODEL]);
    let t = std::time::Instant::now();
    let light = lint(&Input { vpk_name: "pak03_dir.vpk".into(), files: vec![file(MODEL, ::mesh::synth::model(&ours))], base: Some(&g), built_against: None, original_refs: no_originals(), game: Some(&fake), deep: false, fair_play: &policy::DEFAULT });
    let lt = t.elapsed();
    assert!(lt.as_secs_f64() < 1.0, "light mesh checks took {lt:?}");
    assert!(has(&light, "M123", ""), "budgets run in the light lint: {light:#?}");
    assert!(!light.iter().any(|x| ["M106", "M108", "M110", "M124", "M129"].contains(&x.code)), "geometry rules are deep-only: {light:#?}");
}

/// The findings cache returns exactly what the run computes, in a fraction of the time, and retires the
/// entry when the model, a file the rules read, or the game changes.
#[test]
fn findings_cache_is_exact_fast_and_invalidated_by_what_it_read() {
    static ONE_AT_A_TIME: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = ONE_AT_A_TIME.lock().unwrap_or_else(|p| p.into_inner());
    let dir = std::env::temp_dir().join(format!("magician-cache-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    cache::set_dir(Some(dir.clone()));

    let ours = hero(vec![dense_box([-10.0, -10.0, 0.0], [10.0, 10.0, 60.0], BODY, 24), boxd([10.0, -3.0, 20.0], [12.0, 3.0, 30.0], CLOTH, 1)], Some(vec![body()]));
    let mut g = game();
    let mut fake = Fake::default();
    fake.files.insert(MODEL.into(), ::mesh::synth::model(&valve()));
    fake.files.insert(format!("{BODY}_c"), vmat_with(&["F_USE_STATUS_EFFECTS_PROXY", "F_USE_NPR_LIGHTING"], BODY_COLOR, None));
    fake.files.insert(format!("{BODY_COLOR}_c"), vtex(16, 16, vec![[120, 120, 120, 255]; 256]));
    with_game(&mut g, &[MODEL, &format!("{BODY}_c"), &format!("{BODY_COLOR}_c")]);
    let files = |cloth_tex: Vec<u8>| {
        vec![
            file(MODEL, ::mesh::synth::model(&ours)),
            file(&format!("{CLOTH}_c"), hero_vmat()),
            file(&format!("{CLOTH_COLOR}_c"), cloth_tex),
        ]
    };
    let go = |files: Vec<ModFile>| {
        let t = std::time::Instant::now();
        let f = lint(&Input { vpk_name: CACHE_TEST_VPK.into(), files, base: Some(&g), built_against: None, original_refs: no_originals(), game: Some(&fake), deep: true, fair_play: &policy::DEFAULT });
        (format!("{f:#?}"), t.elapsed())
    };

    let (cold, cold_t) = go(files(flat(16, GREY)));
    let (_, misses0) = cache::stats();
    let (warm, warm_t) = go(files(flat(16, GREY)));
    let (hits1, misses1) = cache::stats();
    assert_eq!(cold, warm, "a cached verdict must be the computed one");
    assert_eq!(misses0, misses1, "the second run measured again");
    assert!(hits1 >= 1, "the second run did not read the cache");
    assert!(warm_t < cold_t, "cached {warm_t:?}, computed {cold_t:?}");
    assert!(warm_t.as_secs_f64() < 1.0, "a cached deep lint took {warm_t:?}");

    // a file the rules read changed in the mod (the cloth's colour texture): measured again, not served
    let (_, _) = go(files(flat(16, [250, 250, 250, 255])));
    let (_, misses2) = cache::stats();
    assert!(misses2 > misses1, "a changed texture was served from the cache");

    // another game (different archives): measured again
    let mut other = Fake::default();
    other.files = fake.files.clone();
    let before = cache::stats().1;
    let _ = lint(&Input { vpk_name: CACHE_TEST_VPK.into(), files: files(flat(16, GREY)), base: Some(&g), built_against: None, original_refs: no_originals(), game: Some(&IdentifiedGame(other, "build 7000".into())), deep: true, fair_play: &policy::DEFAULT });
    assert!(cache::stats().1 > before, "a different game build was served from the cache");

    // two keys: this model on the fake game, and on the other build (the changed texture reuses the first key)
    assert_eq!(cache::usage().0, 2, "entries of this test only");
    assert_eq!(cache::clear(), 2);
    assert_eq!(cache::usage().0, 0);
    cache::set_dir(None);
    let _ = std::fs::remove_dir_all(&dir);
}

struct IdentifiedGame(Fake, String);
impl Game for IdentifiedGame {
    fn read(&self, path: &str) -> Option<Vec<u8>> {
        self.0.read(path)
    }
    fn identity(&self) -> String {
        self.1.clone()
    }
}

/// M141's mesh case: Valve's own ringing cloth numbers, unchanged, but the mod hangs a bigger mesh on them (a skin's
/// release: 1,091 u2 of coat, inset and cable on Valve's proxy where Valve's model hangs 869 u2 of coat tail).
#[test]
fn m141_valves_cloth_under_a_bigger_mesh() {
    let nodes: Vec<(&str, f64, f64, f64)> = vec![("$cloth_a", 0.00994, 0.00994, 0.0), ("$cloth_b", 0.00994, 0.0149, 0.0), ("$cloth_c", 0.00994, 0.0199, 0.0)];
    let phys = resource::kv3::encode(&super::fe_phys(&nodes, Some(15)));
    let tail = |w: f32| -> SynthModel {
        // a flat tail panel w units wide and 20 tall, bound to the three cloth nodes down its length
        let mut d = SynthDraw { material: BODY.into(), ..Default::default() };
        for (k, z) in [40.0f32, 30.0, 20.0].iter().enumerate() {
            for x in [-w / 2.0, w / 2.0] {
                d.positions.push([x, -12.0, *z]);
                d.normals.push([0.0, -1.0, 0.0]);
                d.uvs.push([0.5, 0.5]);
                d.joints.push([k as u8 + 2, 0, 0, 0]);
                d.weights.push([1.0, 0.0, 0.0, 0.0]);
            }
        }
        d.tris = vec![[0, 1, 3], [0, 3, 2], [2, 3, 5], [2, 5, 4]];
        let names = ["root", "spine", "$cloth_a", "$cloth_b", "$cloth_c"]; // body() is skinned to bone 1
        SynthModel {
            name: "models/heroes_wip/lash/lash.vmdl".into(),
            meshes: vec![SynthMesh {
                name: "body".into(),
                lod_mask: 1,
                draws: vec![body(), d],
                bones: names.iter().map(|n| (n.to_string(), [0.0, -12.0, 40.0])).collect(),
                no_uv: false,
            }],
            skeleton: names.iter().enumerate().map(|(i, n)| (n.to_string(), if i == 0 { -1 } else { 0 }, [0.0f32, -12.0, 40.0])).collect(),
            hitboxes: vec![("chest".into(), "root".into(), [0.0, 0.0, 10.0], [0.0, 0.0, 50.0], 14.0)],
            extra_blocks: vec![(*b"PHYS", phys.clone())],
            ..Default::default()
        }
    };
    let mut g = game();
    let mut fake = Fake::default();
    fake.files.insert(MODEL.into(), ::mesh::synth::model(&tail(10.0)));
    with_game(&mut g, &[MODEL]);
    let run = |w: f32| {
        lint(&Input {
            vpk_name: "pak03_dir.vpk".into(),
            files: vec![file(MODEL, ::mesh::synth::model(&tail(w)))],
            base: Some(&g),
            built_against: None,
            original_refs: no_originals(),
            game: Some(&fake),
            deep: false,
            fair_play: &policy::DEFAULT,
        })
    };
    let big = run(13.0);
    let m = big.iter().find(|x| x.code == "M141").unwrap_or_else(|| panic!("{big:#?}"));
    assert!(m.message.contains("Valve's own numbers") && m.message.contains("+30%"), "{}", m.message);
    // the same mesh as Valve's: Valve's design, silent
    let same = run(10.0);
    assert!(!same.iter().any(|x| x.code == "M141"), "{same:#?}");
}

#[test]
fn hitboxes_of_every_shape_bound_the_envelope() {
    // the hem of m129_hitbox_envelope, 12 to 16 units past the chest capsule: the taste tier
    let hem = || boxd([26.0, -3.0, 20.0], [30.0, 3.0, 26.0], CLOTH, 1);
    assert!(has(&run_ours(&with(vec![hem()])), "M140", ""));
    // a sphere hitbox round it (Viscous's head, Dynamo's body are spheres): inside a hitbox, nothing to say
    let mut m = with(vec![hem()]);
    m.hitboxes.push(("orb".into(), "root".into(), [28.0, 0.0, 23.0], [28.0, 0.0, 23.0], 8.0));
    m.hitbox_shapes = vec![2, 1];
    let f = run_ours(&m);
    assert!(!has(&f, "M140", "") && !has(&f, "M129", "") && !has(&f, "M130", ""), "{f:#?}");
    // a box hitbox in the bone's frame (Haze's torso, Dynamo's pack) does the same; a box that stops short does not
    let mut m = with(vec![hem()]);
    m.hitboxes.push(("pack".into(), "root".into(), [25.0, -4.0, 19.0], [31.0, 4.0, 27.0], 0.0));
    m.hitbox_shapes = vec![2, 0];
    let f = run_ours(&m);
    assert!(!has(&f, "M140", "") && !has(&f, "M129", ""), "{f:#?}");
    let mut m = with(vec![hem()]);
    m.hitboxes.push(("pack".into(), "root".into(), [14.0, -4.0, 19.0], [17.0, 4.0, 27.0], 0.0));
    m.hitbox_shapes = vec![2, 0];
    let f = run_ours(&m);
    assert!(has(&f, "M129", "beyond pack") || has(&f, "M140", "beyond pack"), "13 units past the box's face: {f:#?}");
}

#[test]
fn hit_shapes_measure_signed_distance() {
    use crate::mesh::census::HitShape;
    let cap = HitShape::Capsule([0.0, 0.0, 0.0], [0.0, 0.0, 10.0], 2.0);
    assert!((cap.distance([5.0, 0.0, 5.0]) - 3.0).abs() < 1e-12 && (cap.distance([0.0, 0.0, 14.0]) - 2.0).abs() < 1e-12);
    assert!((cap.distance([0.0, 0.0, 5.0]) + 2.0).abs() < 1e-12, "on the axis: a radius inside");
    // a box turned a quarter about z: its local x is the model's y
    let b = HitShape::Box { rot: [[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]], org: [10.0, 0.0, 0.0], lo: [0.0, -1.0, -1.0], hi: [4.0, 1.0, 1.0] };
    assert!((b.distance([10.0, 7.0, 0.0]) - 3.0).abs() < 1e-12, "3 units past the far end along local x");
    assert!((b.distance([10.0, 2.0, 0.0]) + 1.0).abs() < 1e-12, "inside, 1 unit from the nearest face");
    assert!((b.distance([13.0, 7.0, 0.0]) - (9.0f64 + 4.0).sqrt()).abs() < 1e-12, "off a corner: the corner's distance");
}

#[test]
fn valves_own_overflow_is_not_ours() {
    // Valve's model itself carries a prop 20 units past its hitbox (a gun, a bench, a body the hitboxes were never
    // fitted to: 24 of 47 heroes on 6865): the unchanged model is silent, and so is a mod that keeps it
    let prop = || boxd([34.0, -1.0, 20.0], [36.0, 1.0, 22.0], BODY, 1);
    let stock = hero(vec![body(), prop()], Some(vec![body()]));
    let f = run_against(&stock, &stock);
    assert!(!has(&f, "M129", "") && !has(&f, "M140", "") && !has(&f, "M130", ""), "{f:#?}");
    // against a Valve without the prop it is the error, so the silence above is the comparison's
    assert!(has(&run_against(&stock, &valve()), "M129", "bit much"));
    // a mod that adds its own piece further out is judged on what it added, with Valve's share named
    let ours = hero(vec![body(), prop(), boxd([40.0, -1.0, 20.0], [42.0, 1.0, 22.0], CLOTH, 1)], Some(vec![body()]));
    let f = run_against(&ours, &stock);
    // Valve's prop reaches 22 units, 12 past the taste line, so the piece at 28 is judged as 16: with 24 u2 of new
    // area, the taste tier and not the error
    assert!(!has(&f, "M129", ""), "{f:#?}");
    let m = f.iter().find(|x| x.code == "M140").unwrap_or_else(|| panic!("{f:#?}"));
    assert!(m.message.contains("counted beyond what Valve's own model has there") && m.message.contains("24.0 u2 of visible surface") && m.message.contains("worst point 16.00 u"), "{}", m.message);
    assert!(m.message.contains("that point is 28.00 u from the hitbox") && m.message.contains("12.00 u past the 10 u line"), "{}", m.message);
    // a hand of ours a hair past Valve's own far-out hand (Viscous, 21.95 against 21.22 units) is nothing at all
    let ours = hero(vec![body(), boxd([34.7, -1.0, 20.0], [36.7, 1.0, 22.0], BODY, 1)], Some(vec![body()]));
    let f = run_against(&ours, &stock);
    assert!(!has(&f, "M129", "") && !has(&f, "M140", ""), "{f:#?}");
    // 20 units past Valve's prop is the error whatever Valve's own does
    let ours = hero(vec![body(), prop(), boxd([54.0, -1.0, 20.0], [56.0, 1.0, 22.0], CLOTH, 1)], Some(vec![body()]));
    assert!(has(&run_against(&ours, &stock), "M129", "bit much"));
}

#[test]
fn m108_valves_own_coplanar_pieces_are_valves() {
    // two of Valve's plates sharing a plane (Lash's gear): on the unchanged model nobody is blamed
    let plate = || {
        let mut d = SynthDraw { material: BODY.into(), ..Default::default() };
        for (y, z) in [(-4.0f32, 20.0f32), (4.0, 20.0), (4.0, 28.0), (-4.0, 28.0)] {
            d.positions.push([10.0, y, z]);
            d.normals.push([1.0, 0.0, 0.0]);
            d.uvs.push([(y + 4.0) / 8.0 * 0.15 + 0.8, (z - 20.0) / 8.0 * 0.8 + 0.1]);
            d.joints.push([1, 0, 0, 0]);
            d.weights.push([1.0, 0.0, 0.0, 0.0]);
        }
        d.tris = vec![[0, 1, 2], [0, 2, 3]];
        d
    };
    let stock = hero(vec![body(), plate()], Some(vec![body()]));
    assert!(has(&run_against(&stock, &valve()), "M108", "share a plane"), "the fixture does z-fight");
    let f = run_against(&stock, &stock);
    assert!(!has(&f, "M108", ""), "{f:#?}");
}

#[test]
fn m145_inside_out_shell() {
    // a closed cuff on the arm with every face and normal reversed: winding and normals agree, the volume is negative
    let cuff = || boxd([10.0, -3.0, 20.0], [12.0, 3.0, 30.0], CLOTH, 1);
    let f = run_ours(&with(vec![inside_out(cuff())]));
    let m = f.iter().find(|x| x.code == "M145").unwrap_or_else(|| panic!("{f:#?}"));
    assert!(m.message.contains("signed volume is -120.0 u3") && m.message.contains("all 12 of its faces point inward"), "{}", m.message);
    assert!(m.message.contains("hollow or flat black") && !m.message.contains("inverted-hull"), "a lone piece is no outline: {}", m.message);
    // the inverted-hull outline: a slightly larger copy wound inside out around the cuff, which the game draws as a
    // solid black shape over it; the finding says so and names the material's own outline as the route
    let hull = inside_out(boxd([9.8, -3.2, 19.8], [12.2, 3.2, 30.2], CLOTH, 1));
    let f = run_ours(&with(vec![cuff(), hull]));
    let m = f.iter().find(|x| x.code == "M145").unwrap_or_else(|| panic!("{f:#?}"));
    for want in ["inverted-hull outline", "opaque black shape over what it wraps", "F_SOLID_COLOR_OUTLINE", "F_OVERRIDE_NPR_OUTLINE"] {
        assert!(m.message.contains(want), "{want}: {}", m.message);
    }
    assert_eq!(f.iter().filter(|x| x.code == "M145").count(), 1, "the cuff inside is the right way out: {f:#?}");
    // a closed piece with no room in it (a card's two layers, 0.02 units apart) has no inside to turn out
    let card = inside_out(boxd([10.0, -3.0, 20.0], [10.02, 3.0, 30.0], CLOTH, 1));
    assert!(!has(&run_ours(&with(vec![card])), "M145", ""));
    assert!(!has(&f, "M103", "") && !has(&f, "M104", ""), "neither the winding nor the normal rule can see it: {f:#?}");
    assert_eq!(rule("M145").severity, Severity::Warn);
    // the same cuff the right way out is silent, and so is a two-sided one (both sides are drawn)
    assert!(!has(&run_ours(&with(vec![cuff()])), "M145", ""));
    let two = mesh_run(&with(vec![inside_out(cuff())]), vec![], vmat(&["F_USE_STATUS_EFFECTS_PROXY", "F_USE_NPR_LIGHTING", "F_RENDER_BACKFACES"], true, None));
    assert!(!has(&two, "M145", ""), "{two:#?}");
    // Valve's own inside-out piece at the same place is Valve's
    let stock = hero(vec![body(), inside_out(boxd([10.0, -3.0, 20.0], [12.0, 3.0, 30.0], BODY, 1))], Some(vec![body()]));
    assert!(!has(&run_against(&stock, &stock), "M145", ""));
}

#[test]
fn m146_lod_inversion() {
    // LOD1 carries the body cut 4 x 4 a face (192 triangles) where LOD0 has 12: the near mesh is the cheap one
    let m = hero(vec![body()], Some(vec![dense_box([-10.0, -10.0, 0.0], [10.0, 10.0, 60.0], BODY, 4)]));
    let f = run_ours(&m);
    let x = f.iter().find(|x| x.code == "M146").unwrap_or_else(|| panic!("{f:#?}"));
    assert!(x.message.contains("192 triangles at LOD1, 12 at LOD0"), "{}", x.message);
    // a decimated vertex flung 3 units out of LOD0's box
    let mut far = body();
    far.positions[4][2] = 63.0;
    let f = run_ours(&hero(vec![body()], Some(vec![far])));
    assert!(has(&f, "M146", "LOD1 reaches 3.00 units outside LOD0's bounding box"), "{f:#?}");
    // a LOD that stays inside and gets no heavier is silent; so is Valve's own inversion kept as it is
    assert!(!has(&run_ours(&valve()), "M146", ""));
    assert!(!has(&run_against(&m, &m), "M146", "triangles at LOD1"));
    // it needs no census: the light lint says it too
    assert!(!crate::mesh::deep_only("M146") && crate::mesh::deep_only("M145") && crate::mesh::deep_only("M147"));
}

#[test]
fn m147_texture_mip_starved() {
    // a 512 x 512 colour map with one mip on a small garment: about 500 texels a metre where 8 m shows 117 pixels
    let one_mip = || {
        let mut hdr = resource::texture::synth_header(512, 512, 28);
        hdr[27] = 1;
        let mut f = resource::synth_with(&[], &[(*b"DATA", hdr)]);
        f.resize(f.len() + 512 * 512 * 4, 110);
        f
    };
    let garment = || with(vec![boxd([10.0, -3.0, 20.0], [12.0, 3.0, 30.0], CLOTH, 1)]);
    let f = mesh_run(&garment(), vec![file(&format!("{CLOTH_COLOR}_c"), one_mip())], hero_vmat());
    let m = f.iter().find(|x| x.code == "M147").unwrap_or_else(|| panic!("{f:#?}"));
    assert!(m.message.contains("stores 1 of 10 mips") && m.message.contains("texels on each pixel (limit 2)"), "{}", m.message);
    // the same map with its whole chain is silent
    let full = mesh_run(&garment(), vec![file(&format!("{CLOTH_COLOR}_c"), flat(512, GREY))], hero_vmat());
    assert!(!has(&full, "M147", ""), "{full:#?}");
    let r = crate::mesh::census::MipReach { mips: 1, texels_per_metre: 468.0, ..Default::default() };
    assert!((r.texels_per_pixel(8.0) - 468.0 / (1080.0 / (16.0 * 30f64.to_radians().tan()))).abs() < 1e-9);
    assert!((crate::mesh::census::MipReach { mips: 3, ..r.clone() }.texels_per_pixel(8.0) * 4.0 - r.texels_per_pixel(8.0)).abs() < 1e-9, "each mip halves it");
}

#[test]
fn m122_decimation_is_judged_against_valves_own() {
    // Valve's LOD1 here is its LOD0 (one mesh for every LOD, as the Cube has): keeping every vertex is Valve's design
    let f = run_ours(&valve());
    assert!(!has(&f, "M122", "keeps"), "{f:#?}");
    // Valve decimates to a quarter and ours keeps everything: 100% against Valve's 6%
    let fine = |n| dense_box([-10.0, -10.0, 0.0], [10.0, 10.0, 60.0], BODY, n);
    let stock = hero(vec![fine(40)], Some(vec![fine(10)]));
    let ours = hero(vec![fine(40)], Some(vec![fine(40)]));
    let f = run_against(&ours, &stock);
    assert!(has(&f, "M122", "LOD1 keeps 100% of LOD0's vertices, Valve's keeps 7%"), "{f:#?}");
}

#[test]
fn another_body_group_choice_is_not_measured_with_the_default_one() {
    // Viscous's model holds the standing hero and the Goo Ball as two choices of one body group; the ball is never
    // drawn with the standing hitboxes, nor through the standing body. Here: a 100-unit ball round the hero.
    let ball = || SynthMesh { name: "inflated".into(), lod_mask: 1, draws: vec![boxd([-50.0, -50.0, -20.0], [50.0, 50.0, 80.0], BODY, 1)], bones: bones(), no_uv: false };
    let mut m = valve();
    m.meshes.push(ball());
    // with no body groups every mesh is drawn at once, and the ball is far outside the chest capsule
    assert!(has(&run_ours(&m), "M129", "bit much"));
    // as another choice it is left out: the default choice is what the default hitbox set and the camera see
    m.mesh_group_masks = vec![1, 1, 2];
    let f = run_ours(&m);
    assert!(mesh_codes(&f).is_empty(), "{f:#?}");
    let parsed = ::mesh::parse(&::mesh::synth::model(&m)).unwrap();
    assert_eq!(parsed.other_choice_meshes(), vec!["inflated"]);
    assert_eq!(parsed.surfaces(0).0.len(), 1, "the body alone");
}

/// The synthetic hero with these attachments, on Valve's body.
fn attached(atts: &[(&str, &str, P3)]) -> SynthModel {
    let mut m = valve();
    m.attachments = atts.iter().map(|(n, b, at)| (n.to_string(), b.to_string(), *at)).collect();
    m
}

fn m062(f: &[Finding]) -> Vec<&Finding> {
    f.iter().filter(|x| x.code == "M062").collect()
}

#[test]
fn m062_a_moved_attachment_on_the_body_is_a_warning_that_says_how_far() {
    // The body is a 20 x 20 x 60 box (size: the square root of 5,600, 74.8 units), so the line is 0.094 of it, 7.0
    // units off the body. Valve's idol sits on the back at (-10, 0, 50), riding the spine at height 30.
    let valve = attached(&[("idol", "spine", [-10.0, 0.0, 20.0])]);
    assert!(m062(&run_against(&valve, &valve)).is_empty(), "Valve's own model against itself");
    // a skin with another body shape moves the idol 4 units back: an effect point that still sits by the body
    let near = run_against(&attached(&[("idol", "spine", [-14.0, 0.0, 20.0])]), &valve);
    let w = m062(&near);
    assert_eq!(w.len(), 1, "{near:#?}");
    assert_eq!(w[0].severity, Severity::Warn);
    assert!(w[0].message.starts_with("attachment 'idol' moved 4.0 units, 4.0 off the body: on spine at -14.00,0.00,20.00"), "{}", w[0].message);
    assert!(w[0].message.contains("a cosmetic move, within 7.0 units of the body (9.4% of the model's size"), "{}", w[0].message);
    // 20 units off the body is past the line: what spawns there shows away from the hero
    let far = run_against(&attached(&[("idol", "spine", [-30.0, 0.0, 20.0])]), &valve);
    let e = m062(&far);
    assert_eq!(e.len(), 1, "{far:#?}");
    assert_eq!(e[0].severity, Severity::Error);
    assert!(e[0].message.starts_with("attachment 'idol' moved 20.0 units, 20.0 off the body:") && e[0].message.contains("past 7.0 units off the body"), "{}", e[0].message);
    // at Valve's place with only its rotation changed, it turned in place: a cosmetic change too
    let bytes = ::mesh::synth::model(&valve);
    let turned = deep::judge_breaches(vec!["attachment 'idol' changed: on spine at -10.00,0.00,20.00 rot 0.71,0.00,0.00,0.71 (Valve's on spine at -10.00,0.00,20.00 rot 0.00,0.00,0.00,1.00)".into()], &bytes, &bytes);
    assert!(turned[0].starts_with("attachment 'idol' turned in place, 0.0 off the body: on spine"), "{turned:?}");
    assert_eq!(deep::contract_severity(&turned[0]), Severity::Warn);
    // just inside and just outside the line
    assert_eq!(m062(&run_against(&attached(&[("idol", "spine", [-16.9, 0.0, 20.0])]), &valve))[0].severity, Severity::Warn);
    assert_eq!(m062(&run_against(&attached(&[("idol", "spine", [-17.2, 0.0, 20.0])]), &valve))[0].severity, Severity::Error);
}

#[test]
fn m062_a_moved_attachment_in_the_hero_s_frame_and_a_missing_one_stay_errors() {
    // On the root an attachment is placed in the hero's own frame (Valve's camera poses, aim and zipline points), not
    // on the body: moved, it stays a contract break, however near the body it lands.
    let valve = attached(&[("idol", "spine", [-10.0, 0.0, 20.0]), ("zipline", "root", [0.0, 0.0, 70.0])]);
    let f = run_against(&attached(&[("idol", "spine", [-10.0, 0.0, 20.0]), ("zipline", "root", [0.0, 0.0, 50.0])]), &valve);
    let e = m062(&f);
    assert!(e.len() == 1 && e[0].severity == Severity::Error && e[0].message.starts_with("attachment 'zipline' changed:") && e[0].message.contains("it moved 20.0 units in the hero's own frame"), "{f:#?}");
    // so is one on the model's root transform (no bone)
    let valve = attached(&[("shoot_test", "", [0.0, -15.0, 75.0])]);
    let e = run_against(&attached(&[("shoot_test", "", [0.0, -15.0, 70.0])]), &valve);
    assert!(m062(&e).iter().all(|x| x.severity == Severity::Error) && m062(&e).len() == 1, "{e:#?}");
    // an attachment the game asks for that is gone is an error
    let valve = attached(&[("idol", "spine", [-10.0, 0.0, 20.0]), ("palm_r", "spine", [10.0, 0.0, 0.0])]);
    let f = run_against(&attached(&[("idol", "spine", [-12.0, 0.0, 20.0])]), &valve);
    let mut got: Vec<(Severity, &str)> = m062(&f).iter().map(|x| (x.severity, x.message.as_str())).collect();
    got.sort();
    assert_eq!(got.len(), 2, "{f:#?}");
    assert!(got[0].0 == Severity::Warn && got[0].1.starts_with("attachment 'idol' moved 2.0 units"), "{got:?}");
    assert_eq!(got[1], (Severity::Error, "attachment 'palm_r' missing"));
}

#[test]
fn m062_a_hitbox_change_beside_a_cosmetic_move_is_still_an_error() {
    let valve = attached(&[("idol", "spine", [-10.0, 0.0, 20.0])]);
    let mut ours = attached(&[("idol", "spine", [-12.0, 0.0, 20.0])]);
    ours.hitboxes[0].4 = 15.0;
    let f = run_against(&ours, &valve);
    let hit: Vec<&Finding> = m062(&f).into_iter().filter(|x| x.message.starts_with("hitbox")).collect();
    assert_eq!(hit.len(), 1, "{f:#?}");
    assert_eq!(hit[0].severity, Severity::Error);
    assert!(hit[0].message.starts_with("hitbox 'default/chest' changed:"), "{}", hit[0].message);
    assert!(m062(&f).iter().any(|x| x.severity == Severity::Warn && x.message.starts_with("attachment 'idol' moved")));
    // and a lost bone is still an error, whatever the attachments do
    assert_eq!(deep::contract_severity("bone 'pelvis' missing"), Severity::Error);
    assert_eq!(deep::contract_severity("hitbox 'default/chest' changed: x (Valve's y)"), Severity::Error);
}

// ------------------------------------------------------------------------------- M062: lost and reparented bones

const NM: &str = "models/heroes_wip/lash/lash.vnmskel";
const GRAPH: &str = "animgraphs/animgraph2/hero/hero.vnmgraph+lash.vnmgraph";
const CLIP: &str = "models/heroes_wip/lash/clips/idle.vnmclip";

/// A bone: name, parent (empty: a root), position relative to the parent.
type Bone = (&'static str, &'static str, P3);

/// Valve's rig for the bone tests: the fixture hero's root, spine and head; a ponytail weighted below the head; a hip
/// holding a hitbox, a bone holding an attachment and a ragdoll bone; a strap the animation skeleton lacks; and two
/// that carry nothing, an end bone and an IK target.
fn rig_bones() -> Vec<Bone> {
    vec![
        ("root", "", [0.0; 3]),
        ("spine", "root", [0.0, 0.0, 30.0]),
        ("head", "spine", [0.0, 0.0, 25.0]),
        ("ponytail", "head", [-6.0, 0.0, 5.0]),
        ("head_end", "head", [0.0, 0.0, 10.0]),
        ("hip", "root", [0.0, 0.0, 10.0]),
        ("idol_bone", "spine", [-10.0, 0.0, 10.0]),
        ("rag", "spine", [10.0, 0.0, 0.0]),
        ("strap", "spine", [0.0, 10.0, 0.0]),
        ("ik_target", "root", [0.0, 20.0, 0.0]),
    ]
}

/// The bones without `lost`; a bone whose parent is lost hangs from the lost bone's parent, where it was.
fn lose(bones: &[Bone], lost: &[&str]) -> Vec<Bone> {
    let find = |n: &str| bones.iter().find(|b| b.0 == n).copied();
    bones
        .iter()
        .filter(|b| !lost.contains(&b.0))
        .map(|&(n, mut p, mut at)| {
            while lost.contains(&p) {
                let up = find(p).unwrap();
                at = [at[0] + up.2[0], at[1] + up.2[1], at[2] + up.2[2]];
                p = up.1;
            }
            (n, p, at)
        })
        .collect()
}

/// The bones with `name` hung from `parent` at `at`.
fn hang(bones: &[Bone], name: &'static str, parent: &'static str, at: P3) -> Vec<Bone> {
    bones.iter().map(|&b| if b.0 == name { (name, parent, at) } else { b }).collect()
}

/// A model with this skeleton: the fixture body on the spine and head on the head, a 2-unit box on each bone of
/// `boxes` (at that model-space point), the chest and hip hitboxes, the idol attachment, a ragdoll part on `rag`, and
/// the animation the hero's files name. Hitboxes and attachments on a bone the skeleton lacks are left out.
fn rigged(bones: &[Bone], boxes: &[(&str, P3)]) -> SynthModel {
    let index = |n: &str| bones.iter().position(|b| b.0 == n);
    let world = |i: usize| {
        let mut at = [0.0f32; 3];
        let mut k = Some(i);
        while let Some(x) = k {
            let b = bones[x];
            at = [at[0] + b.2[0], at[1] + b.2[1], at[2] + b.2[2]];
            k = index(b.1);
        }
        at
    };
    let mesh_bones: Vec<(String, P3)> = (0..bones.len()).map(|i| (bones[i].0.to_string(), world(i))).collect();
    let joint = |n: &str| index(n).unwrap() as u8;
    let mut draws = vec![boxd([-10.0, -10.0, 0.0], [10.0, 10.0, 60.0], BODY, joint("spine"))];
    if index("head").is_some() {
        draws.push(boxd([-4.0, -4.0, 70.0], [4.0, 4.0, 78.0], CLOTH, joint("head")));
    }
    for (b, c) in boxes {
        if index(b).is_some() {
            draws.push(boxd([c[0] - 1.0, c[1] - 1.0, c[2] - 1.0], [c[0] + 1.0, c[1] + 1.0, c[2] + 1.0], CLOTH, joint(b)));
        }
    }
    let mut hitboxes = vec![("chest".to_string(), "root".to_string(), [0.0, 0.0, 10.0], [0.0, 0.0, 50.0], 14.0)];
    if index("hip").is_some() {
        hitboxes.push(("hip".into(), "hip".into(), [0.0, -5.0, 0.0], [0.0, 5.0, 0.0], 6.0));
    }
    let phys = resource::kv3::Value::Object(vec![("m_boneNames".into(), resource::kv3::Value::Array(vec![resource::kv3::Value::Str("rag".into())]))]);
    SynthModel {
        name: "models/heroes_wip/lash/lash.vmdl".into(),
        meshes: vec![SynthMesh { name: "body".into(), lod_mask: 1, draws, bones: mesh_bones, no_uv: false }],
        skeleton: bones.iter().map(|b| (b.0.to_string(), index(b.1).map_or(-1, |p| p as i32), b.2)).collect(),
        hitboxes,
        attachments: if index("idol_bone").is_some() { vec![("idol".into(), "idol_bone".into(), [0.0; 3])] } else { vec![] },
        extra_blocks: vec![(*b"PHYS", resource::kv3::encode(&phys))],
        nm_skeleton: NM.into(),
        anim_graphs: vec![GRAPH.into()],
        ..Default::default()
    }
}

/// Valve's model for the bone tests.
fn rig_valve() -> SynthModel {
    rigged(&rig_bones(), &[("ponytail", [-6.0, 0.0, 60.0]), ("strap", [0.0, 10.0, 30.0])])
}

/// The hero's animation: its skeleton (every bone of `rig_bones` but the end bone and the strap, at the same
/// reference pose) and one clip of two frames, the first at rest and the second with each bone of `moves` turned
/// about x by its degrees and shifted by its offset.
fn rig_anim(moves: &[(&str, f32, P3)]) -> HashMap<String, Vec<u8>> {
    let bones: Vec<Bone> = rig_bones().into_iter().filter(|b| b.0 != "head_end" && b.0 != "strap").collect();
    let index = |n: &str| bones.iter().position(|b| b.0 == n).map_or(-1, |i| i as i32);
    let nm: Vec<::mesh::synth::NmBone> = bones.iter().map(|b| (b.0, index(b.1), b.2, [0.0, 0.0, 0.0, 1.0])).collect();
    let rest: Vec<([f32; 4], P3)> = bones.iter().map(|b| ([0.0, 0.0, 0.0, 1.0], b.2)).collect();
    let mut moved = rest.clone();
    for (n, deg, by) in moves {
        let i = index(n) as usize;
        let h = deg.to_radians() / 2.0;
        moved[i] = ([h.sin(), 0.0, 0.0, h.cos()], [rest[i].1[0] + by[0], rest[i].1[1] + by[1], rest[i].1[2] + by[2]]);
    }
    HashMap::from([
        (format!("{NM}_c"), ::mesh::synth::nm_skeleton(NM, &nm)),
        (format!("{GRAPH}_c"), ::mesh::synth::nm_graph(&[CLIP])),
        (format!("{CLIP}_c"), ::mesh::synth::nm_clip(NM, &[rest, moved], false)),
    ])
}

/// M062's bone findings of `ours` against `valve`, with the hero's animation moving as `moves` says, and a bone users
/// index in which nothing outside the model names a bone.
fn bone_findings(ours: &SynthModel, valve: &SynthModel, moves: &[(&str, f32, P3)]) -> Vec<(Severity, String)> {
    bone_findings_with(ours, valve, moves, Some(&crate::carry::BoneUsers { code_read: true, ..Default::default() }))
}

/// `bone_findings` with the bone users given (`None`: no index for the build).
fn bone_findings_with(ours: &SynthModel, valve: &SynthModel, moves: &[(&str, f32, P3)], users: Option<&crate::carry::BoneUsers>) -> Vec<(Severity, String)> {
    let (o, v) = (::mesh::synth::model(ours), ::mesh::synth::model(valve));
    let files = rig_anim(moves);
    let read = |p: &str| files.get(p).cloned();
    let breaches = deep::contract_breaches(&deep::contract(&o).unwrap(), &deep::contract(&v).unwrap());
    crate::carry::judge(breaches, &o, &v, &read, &|_: &str| false, users).into_iter().filter(|m| m.starts_with("bone '")).map(|m| (deep::contract_severity(&m), m)).collect()
}

/// A bone users index naming each `(bone, kind, key, path)`.
fn bone_users(uses: &[(&str, &str, &str, &str)]) -> crate::carry::BoneUsers {
    let mut u = crate::carry::BoneUsers { code_read: true, ..Default::default() };
    for (b, k, key, p) in uses {
        u.by_name.entry(b.to_ascii_lowercase()).or_default().push((k.to_string(), key.to_string(), p.to_string()));
    }
    u
}

/// The one bone finding about `bone`.
fn bone_finding(f: &[(Severity, String)], bone: &str) -> (Severity, String) {
    let mine: Vec<_> = f.iter().filter(|(_, m)| m.starts_with(&format!("bone '{bone}'"))).cloned().collect();
    assert_eq!(mine.len(), 1, "{f:#?}");
    mine[0].clone()
}

#[test]
fn m062_a_lost_bone_is_judged_by_who_still_names_it() {
    let valve = rig_valve();
    assert!(bone_findings(&valve, &valve, &[]).is_empty(), "Valve's own model against itself");
    let ours = |names: &[&str]| rigged(&lose(&rig_bones(), names), &[("ponytail", [-6.0, 0.0, 60.0])]);
    let lost = |names: &[&str], moves: &[(&str, f32, P3)]| bone_findings(&ours(names), &valve, moves);
    // an end bone nothing hangs on, which the animation does not have: a warning that says so
    let (s, m) = bone_finding(&lost(&["head_end"], &[]), "head_end");
    assert_eq!(s, Severity::Warn, "{m}");
    assert!(m.starts_with("bone 'head_end' missing: nothing outside the model looks it up") && m.contains("Valve's model hangs nothing on it") && m.contains("its animation skeleton has no such bone") && m.ends_with(crate::carry::NOTHING_CHANGES), "{m}");
    // an IK target the clips move 20 units: the animation skeleton still has it, the model hangs nothing on it
    let (s, m) = bone_finding(&lost(&["ik_target"], &[("ik_target", 0.0, [0.0, 0.0, 20.0])]), "ik_target");
    assert_eq!(s, Severity::Warn, "{m}");
    assert!(m.contains("in its 1 animation clip it shifts 20.0 units"), "{m}");
    // what Valve's model hung on the bone is the mod's own now: vertices, a hitbox (compared on its own), an
    // attachment (judged on its own) are said, and nothing a player sees changes for the bone
    for (bone, what) in [("ponytail", "Valve's model hangs 24 vertices on it"), ("hip", "hitbox 'default/hip'"), ("idol_bone", "attachment 'idol'")] {
        let (s, m) = bone_finding(&lost(&[bone], &[]), bone);
        assert_eq!(s, Severity::Warn, "{m}");
        assert!(m.contains(what) && m.ends_with(crate::carry::NOTHING_CHANGES), "{bone}: {m}");
    }
    // the model's own physics still names the lost ragdoll bone: a part with no bone, an error
    let (s, m) = bone_finding(&lost(&["rag"], &[]), "rag");
    assert!(s == Severity::Error && m.contains("this model itself (a physics part) still names it") && m.contains("Valve's model hangs a ragdoll part on it"), "{m}");
    // the hitbox itself keeps its own finding and its full strength
    let f = run_against_anim(&ours(&["hip"]), &valve, &[]);
    assert!(m062(&f).iter().any(|x| x.severity == Severity::Error && x.message.starts_with("hitbox 'default/hip' missing")), "{f:#?}");
    // the model's own attachment still riding the lost bone: a reference to nothing, an error that names it
    let mut riding = ours(&["idol_bone"]);
    riding.attachments.push(("idol".into(), "idol_bone".into(), [0.0; 3]));
    let (s, m) = bone_finding(&bone_findings(&riding, &valve, &[]), "idol_bone");
    assert_eq!(s, Severity::Error, "{m}");
    assert!(m.contains("this model itself (attachment 'idol') still names it") && m.contains("put the bone back"), "{m}");
    // the client code holds the bone's name: an error, unless the model has an attachment of that name to find
    let code = bone_users(&[("ponytail", "code", "-", "client.dll")]);
    let (s, m) = bone_finding(&bone_findings_with(&ours(&["ponytail"]), &valve, &[], Some(&code)), "ponytail");
    assert!(s == Severity::Error && m.contains("the game's client code") && m.contains("UNVERIFIED"), "{m}");
    let mut named = ours(&["ponytail"]);
    named.attachments.push(("ponytail".into(), "spine".into(), [0.0; 3]));
    assert_eq!(bone_finding(&bone_findings_with(&named, &valve, &[], Some(&code)), "ponytail").0, Severity::Warn, "the code's lookup finds the attachment");
    // the hero's graphs naming it pose the animation skeleton, not the model: said, not an error
    let graph = bone_users(&[("ik_target", "graph", "m_boneID", &format!("{GRAPH}_c").to_ascii_lowercase())]);
    let (s, m) = bone_finding(&bone_findings_with(&ours(&["ik_target"]), &valve, &[], Some(&graph)), "ik_target");
    assert!(s == Severity::Warn && m.contains("the hero's anim graphs name it (m_boneID)"), "{m}");
    // a graph the hero does not play is not said
    let other = bone_users(&[("ik_target", "graph", "m_boneID", "animgraphs/someone_else.vnmgraph_c")]);
    assert!(!bone_finding(&bone_findings_with(&ours(&["ik_target"]), &valve, &[], Some(&other)), "ik_target").1.contains("anim graphs"));
    // no bone users index, or one written without the client library: who names it is unknown, never clean
    let (s, m) = bone_finding(&bone_findings_with(&ours(&["head_end"]), &valve, &[], None), "head_end");
    assert!(s == Severity::Error && m.contains("could not be checked") && m.contains("magician index"), "{m}");
    let blind = crate::carry::BoneUsers::default();
    let (s, m) = bone_finding(&bone_findings_with(&ours(&["head_end"]), &valve, &[], Some(&blind)), "head_end");
    assert!(s == Severity::Error && m.contains("without the client library"), "{m}");
}

#[test]
fn bone_users_read_back_only_in_their_own_format() {
    let text = format!("# magician bone users build=6880 rev={} code=yes names=2\nhead\tcode\t-\tclient.dll\nweaponhand_l\tgraph\tm_boneID\tanimgraphs/a.vnmgraph_c\n", crate::carry::BONE_USERS_REV);
    let u = crate::carry::BoneUsers::parse(&text).unwrap();
    assert!(u.code_read && u.by_name["head"][0].0 == "code" && u.by_name["weaponhand_l"][0].1 == "m_boneID");
    assert!(crate::carry::BoneUsers::parse("# magician bone users build=6880 rev=0 code=yes names=0\n").is_none(), "another format");
    assert!(!crate::carry::BoneUsers::parse(&text.replace("code=yes", "code=no")).unwrap().code_read);
    assert!(crate::carry::BoneUsers::parse("head\tcode\n").is_none(), "not the file");
}

#[test]
fn m062_a_reparented_bone_breaks_only_where_it_moves_differently() {
    let valve = rig_valve();
    let boxes = [("ponytail", [-6.0, 0.0, 60.0]), ("strap", [0.0, 10.0, 30.0])];
    let ours = |bones: Vec<Bone>| rigged(&bones, &boxes);
    let one = |m: &SynthModel, moves: &[(&str, f32, P3)], bone: &str| bone_finding(&bone_findings(m, &valve, moves), bone);
    // the ponytail hung from the spine where it was: the animation poses it by name, so it moves as Valve's
    let (s, m) = one(&ours(hang(&rig_bones(), "ponytail", "spine", [-6.0, 0.0, 30.0])), &[("spine", 20.0, [0.0; 3])], "ponytail");
    assert_eq!(s, Severity::Warn, "{m}");
    assert!(m.starts_with("bone 'ponytail' reparented: 'spine', Valve's 'head'; at bind it sits 0.00 units") && m.contains("its animation poses it by name") && m.contains("the 24 vertices this model weights to it land within a fair-play pixel"), "{m}");
    // bound a fair-play pixel (0.25 units) or more from Valve's bone, its vertices land out of place
    let (s, m) = one(&ours(hang(&rig_bones(), "ponytail", "spine", [-6.0, 0.0, 30.3])), &[], "ponytail");
    assert_eq!(s, Severity::Error, "{m}");
    assert!(m.contains("at bind it sits 0.30 units and 0.0 degrees from Valve's") && m.contains("the 24 vertices this model weights to it were bound where it sits here, and land up to 0.3 units from there"), "{m}");
    assert_eq!(one(&ours(hang(&rig_bones(), "ponytail", "spine", [-6.0, 0.0, 30.2])), &[], "ponytail").0, Severity::Warn, "under a pixel");
    // an IK target the animation poses, 5 units off at bind with no vertex on it: nothing lands anywhere
    let (s, m) = one(&ours(hang(&rig_bones(), "ik_target", "spine", [0.0, 20.0, -25.0])), &[], "ik_target");
    assert!(s == Severity::Warn && m.contains("at bind it sits 5.00 units") && m.contains("this model weights no vertex to it"), "{m}");
    // the strap, which the animation skeleton lacks, follows its parent: hung from the head where it was, it moves
    // as Valve's while the head is still against the spine...
    let strap = ours(hang(&rig_bones(), "strap", "head", [0.0, 10.0, -25.0]));
    let (s, m) = one(&strap, &[("spine", 20.0, [0.0; 3])], "strap");
    assert!(s == Severity::Warn && m.contains("it follows 'head' and Valve's 'spine', which move as one in its 1 animation clip"), "{m}");
    // ...and apart once the head turns: 10 degrees at 27 units is 4.7 units
    let (s, m) = one(&strap, &[("head", 10.0, [0.0; 3])], "strap");
    assert_eq!(s, Severity::Error, "{m}");
    assert!(m.contains("they move apart ('head' turns up to 10.0 degrees)") && m.contains("what this model hangs on it (24 vertices) moves up to 4.7 units"), "{m}");
    // the line is one fair-play pixel at the strap: half a degree is 0.23 units there, six tenths 0.28
    assert_eq!(one(&strap, &[("head", 0.5, [0.0; 3])], "strap").0, Severity::Warn);
    assert_eq!(one(&strap, &[("head", 0.6, [0.0; 3])], "strap").0, Severity::Error);
    // the end bone, which nothing of this model hangs on, hung from the spine: the head moves it apart, unseen
    let (s, m) = one(&ours(hang(&rig_bones(), "head_end", "spine", [0.0, 0.0, 35.0])), &[("head", 10.0, [0.0; 3])], "head_end");
    assert!(s == Severity::Warn && m.contains("this model hangs nothing on it or on a bone that follows it"), "{m}");
    // a mod that ships the animation skeleton moves the bone its own way: not Valve's files to judge
    let (o, v) = (::mesh::synth::model(&ours(hang(&rig_bones(), "ponytail", "spine", [-6.0, 0.0, 30.0]))), ::mesh::synth::model(&valve));
    let files = rig_anim(&[]);
    let breaches = deep::contract_breaches(&deep::contract(&o).unwrap(), &deep::contract(&v).unwrap());
    let got = crate::carry::judge(breaches, &o, &v, &|p: &str| files.get(p).cloned(), &|p: &str| p == format!("{NM}_c"), None);
    assert!(got.iter().any(|m| m.starts_with("bone 'ponytail'") && m.contains("ships its own animation skeleton") && deep::contract_severity(m) == Severity::Error), "{got:?}");
    // with no clip of the skeleton to read, how the strap moves is not known: never a quiet "moves as one"
    let o = ::mesh::synth::model(&strap);
    let mut files = rig_anim(&[]);
    files.remove(&format!("{CLIP}_c"));
    let breaches = deep::contract_breaches(&deep::contract(&o).unwrap(), &deep::contract(&v).unwrap());
    let got = crate::carry::judge(breaches, &o, &v, &|p: &str| files.get(p).cloned(), &|_: &str| false, None);
    let m = got.iter().find(|m| m.starts_with("bone 'strap'")).unwrap();
    assert!(m.contains("could not be measured: no clip of its animation skeleton was found") && deep::contract_severity(m) == Severity::Error, "{m}");
}
/// `run_against` with the hero's animation in the game.
fn run_against_anim(ours: &SynthModel, valve: &SynthModel, moves: &[(&str, f32, P3)]) -> Vec<Finding> {
    let mut g = game();
    let mut fake = Fake { bones: Some(crate::carry::BoneUsers { code_read: true, ..Default::default() }), ..Default::default() };
    fake.files.insert(MODEL.into(), ::mesh::synth::model(valve));
    fake.files.insert(format!("{BODY}_c"), vmat_with(&["F_USE_STATUS_EFFECTS_PROXY", "F_USE_NPR_LIGHTING"], BODY_COLOR, None));
    fake.files.insert(format!("{BODY_COLOR}_c"), flat(16, GREY));
    fake.files.insert(format!("{CLOTH}_c"), hero_vmat());
    fake.files.insert(format!("{CLOTH_COLOR}_c"), flat(16, GREY));
    fake.files.extend(rig_anim(moves));
    with_game(&mut g, &[MODEL, &format!("{BODY}_c"), &format!("{BODY_COLOR}_c"), &format!("{CLOTH}_c"), &format!("{CLOTH_COLOR}_c")]);
    let files = vec![file(MODEL, ::mesh::synth::model(ours))];
    lint(&Input { vpk_name: "pak03_dir.vpk".into(), files, base: Some(&g), built_against: None, original_refs: no_originals(), game: Some(&fake), deep: true, fair_play: &policy::DEFAULT })
}

#[test]
fn m062_a_helper_bone_lost_in_a_whole_lint_is_a_warning() {
    let valve = rig_valve();
    assert!(m062(&run_against_anim(&valve, &valve, &[])).is_empty(), "Valve's own model against itself");
    let f = run_against_anim(&rigged(&lose(&rig_bones(), &["head_end"]), &[("ponytail", [-6.0, 0.0, 60.0]), ("strap", [0.0, 10.0, 30.0])]), &valve, &[]);
    let m = m062(&f);
    assert!(m.len() == 1 && m[0].severity == Severity::Warn && m[0].message.starts_with("bone 'head_end' missing:"), "{f:#?}");
    // a full skin's own mesh replaces what Valve's hung on the ponytail: a warning
    let f = run_against_anim(&rigged(&lose(&rig_bones(), &["ponytail"]), &[("strap", [0.0, 10.0, 30.0])]), &valve, &[]);
    assert!(m062(&f).iter().any(|x| x.severity == Severity::Warn && x.message.starts_with("bone 'ponytail' missing:")), "{f:#?}");
    // the model's own physics still naming the lost ragdoll bone: an error
    let f = run_against_anim(&rigged(&lose(&rig_bones(), &["rag"]), &[("ponytail", [-6.0, 0.0, 60.0]), ("strap", [0.0, 10.0, 30.0])]), &valve, &[]);
    assert!(m062(&f).iter().any(|x| x.severity == Severity::Error && x.message.starts_with("bone 'rag' missing, and this model itself")), "{f:#?}");
}