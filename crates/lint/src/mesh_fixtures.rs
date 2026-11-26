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
    let mut g = game();
    let mut fake = Fake::default();
    fake.files.insert(MODEL.into(), ::mesh::synth::model(&valve()));
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

#[test]
fn m109_self_intersection() {
    // three quads in one shell; the third folds back through the first
    let mut d = SynthDraw { material: CLOTH.into(), ..Default::default() };
    let pts: [P3; 8] = [[10.0, 0.0, 20.0], [16.0, 0.0, 20.0], [16.0, 4.0, 20.0], [10.0, 4.0, 20.0], [16.0, 4.0, 24.0], [16.0, 0.0, 24.0], [13.0, 4.0, 16.0], [13.0, 0.0, 16.0]];
    for p in pts {
        d.positions.push(p);
        d.normals.push([0.0, 0.0, 1.0]);
        d.uvs.push([0.5, 0.5]);
        d.joints.push([1, 0, 0, 0]);
        d.weights.push([1.0, 0.0, 0.0, 0.0]);
    }
    d.tris = vec![[0, 1, 2], [0, 2, 3], [1, 5, 4], [1, 4, 2], [5, 7, 6], [5, 6, 4]];
    let f = run_ours(&with(vec![d]));
    assert!(has(&f, "M109", "folds through itself"), "{f:#?}");
}

#[test]
fn m110_shell_intersection() {
    // a pouch sunk 2 units into the body
    let f = run_ours(&with(vec![boxd([8.0, -3.0, 20.0], [14.0, 3.0, 26.0], CLOTH, 1)]));
    assert!(has(&f, "M110", "units into"), "{f:#?}");
}

#[test]
fn m111_floating_shell() {
    // an arm band 2 units off the body
    let f = run_ours(&with(vec![boxd([12.0, -3.0, 20.0], [14.0, 3.0, 22.0], CLOTH, 1)]));
    assert!(has(&f, "M111", "touches nothing"), "{f:#?}");
    // resting on the body but skinned to the head: lifts off when the spine and head part
    let f = run_ours(&with(vec![boxd([10.0, -3.0, 20.0], [12.0, 3.0, 30.0], CLOTH, 2)]));
    assert!(has(&f, "M111", "skinned to other bones"), "{f:#?}");
}

#[test]
fn m112_coordinate_precision() {
    // 0.05 units thick at 1e5 units from the origin, where float32 steps are ~0.012
    let f = run_ours(&with(vec![boxd([100000.0, -3.0, 20.0], [100000.05, 3.0, 26.0], CLOTH, 1)]));
    assert!(has(&f, "M112", "float32"), "{f:#?}");
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
fn m136_m137_albedo() {
    let piece = with(vec![boxd([10.0, -10.0, 5.0], [14.0, 10.0, 55.0], CLOTH, 1)]);
    // pure white: outside Valve's range and absolute white
    let f = mesh_run(&piece, vec![file(&format!("{CLOTH_COLOR}_c"), flat(16, [255, 255, 255, 255]))], hero_vmat());
    assert!(has(&f, "M136", "outside Valve's albedo range"), "{f:#?}");
    assert!(has(&f, "M137", "absolute white"), "{f:#?}");
    // within Valve's band: silent
    let f = mesh_run(&piece, vec![file(&format!("{CLOTH_COLOR}_c"), flat(16, [100, 100, 100, 255]))], hero_vmat());
    assert!(!has(&f, "M136", "") && !has(&f, "M137", ""), "{f:#?}");
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
