//! The mark's proof on synthetic textures: imperceptibility, survival through BC1/BC3/BC7, mips and
//! resizing, the alpha guarantee, the payload, idempotent signing, and the false-positive bound.

use crate::bc::{self, Format};
use crate::image::Image;
use crate::metrics;
use crate::scheme::{self, EmbedParams, Key, Payload};

pub(crate) struct Rng(u64);
impl Rng {
    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let mut z = self.0;
        z = (z ^ (z >> 33)).wrapping_mul(0xff51afd7ed558ccd);
        z ^ (z >> 33)
    }
    pub fn f(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// Value noise, `octaves` octaves from period `base` px: a stand-in for painted or baked texture detail.
fn noise(w: usize, h: usize, base: usize, octaves: usize, seed: u64) -> Vec<f64> {
    let mut out = vec![0.0; w * h];
    let mut amp = 1.0;
    let mut period = base as f64;
    let mut rng = Rng(seed);
    for _ in 0..octaves {
        let (gw, gh) = ((w as f64 / period).ceil() as usize + 2, (h as f64 / period).ceil() as usize + 2);
        let lat: Vec<f64> = (0..gw * gh).map(|_| rng.f() * 2.0 - 1.0).collect();
        for y in 0..h {
            for x in 0..w {
                let (fx, fy) = (x as f64 / period, y as f64 / period);
                let (ix, iy) = (fx.floor() as usize, fy.floor() as usize);
                let (tx, ty) = (fx - ix as f64, fy - iy as f64);
                let s = |t: f64| t * t * (3.0 - 2.0 * t);
                let (sx, sy) = (s(tx), s(ty));
                let l = |a: usize, b: usize| lat[b * gw + a];
                let v = (l(ix, iy) * (1.0 - sx) + l(ix + 1, iy) * sx) * (1.0 - sy) + (l(ix, iy + 1) * (1.0 - sx) + l(ix + 1, iy + 1) * sx) * sy;
                out[y * w + x] += amp * v;
            }
        }
        amp *= 0.55;
        period /= 2.0;
        if period < 1.0 {
            break;
        }
    }
    out
}

pub(crate) enum Kind {
    /// Woven cloth: fine noise over a coloured base with a thread pattern.
    Cloth,
    /// A palette atlas: flat patches of colour (Maid Lash's cloth map, another skin's props palette).
    Palette,
    /// Near-black leather.
    Dark,
    /// Lace: cloth with alpha-tested holes.
    Lace,
    /// A busy painted map with strong edges.
    Painted,
}

pub(crate) fn synth(kind: Kind, w: usize, h: usize, seed: u64) -> Image {
    let mut rng = Rng(seed ^ 0x5eed);
    let n1 = noise(w, h, 32, 6, seed);
    let n2 = noise(w, h, 8, 3, seed ^ 0xabc);
    let mut px = vec![0u8; w * h * 4];
    let pal: Vec<[f64; 3]> = (0..16).map(|_| [rng.f() * 255.0, rng.f() * 255.0, rng.f() * 255.0]).collect();
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let (n, f) = (n1[i], n2[i]);
            let (mut r, mut g, mut b, mut a): (f64, f64, f64, f64) = match kind {
                Kind::Cloth => {
                    let weave = if (x / 2 + y / 2) % 2 == 0 { 6.0 } else { -6.0 };
                    (150.0 + 40.0 * n + 10.0 * f + weave, 40.0 + 20.0 * n + weave, 60.0 + 25.0 * n + 8.0 * f + weave, 255.0)
                }
                Kind::Palette => {
                    let c = pal[(x * 4 / w) + 4 * (y * 4 / h)];
                    (c[0], c[1], c[2], 255.0)
                }
                Kind::Dark => (12.0 + 10.0 * n + 3.0 * f, 10.0 + 8.0 * n + 3.0 * f, 14.0 + 9.0 * n + 3.0 * f, 255.0),
                Kind::Lace => {
                    let (cx, cy) = ((x % 128) as f64 - 63.5, (y % 128) as f64 - 63.5);
                    let hole = cx * cx + cy * cy < 100.0;
                    (230.0 + 15.0 * n + 5.0 * f, 222.0 + 15.0 * n, 210.0 + 12.0 * n + 5.0 * f, if hole { 0.0 } else { 255.0 })
                }
                Kind::Painted => {
                    let edge = if (n * 3.0).sin() > 0.3 { 60.0 } else { 0.0 };
                    (90.0 + 80.0 * n + 30.0 * f + edge, 110.0 + 60.0 * n + 20.0 * f, 70.0 + 70.0 * f + edge * 0.5, 255.0)
                }
            };
            if matches!(kind, Kind::Lace) && a == 0.0 {
                r = 0.0;
                g = 0.0;
                b = 0.0;
            }
            a = a.clamp(0.0, 255.0);
            px[4 * i..4 * i + 4].copy_from_slice(&[r.round().clamp(0.0, 255.0) as u8, g.round().clamp(0.0, 255.0) as u8, b.round().clamp(0.0, 255.0) as u8, a as u8]);
        }
    }
    Image::new(w, h, px)
}

pub(crate) fn key() -> Key {
    Key::new("example-author")
}

pub(crate) fn params() -> EmbedParams {
    EmbedParams { key: key(), secret: None, payload: Payload { mod_id: scheme::mod_id("maid_lash"), day: scheme::parse_date("2026-11-19").unwrap() }, target_rms: scheme::TARGET_RMS }
}

fn signed(kind: Kind, w: usize, h: usize, seed: u64) -> (Image, Image) {
    let src = synth(kind, w, h, seed);
    let mut s = src.clone();
    scheme::embed(&mut s, None, &params()).unwrap();
    (src, s)
}

fn z(img: &Image) -> (f64, f64, i32) {
    let f = scheme::detect(&img.rgba, img.w, img.h, key(), None).unwrap();
    (f.z, f.log10_p, f.step)
}

#[test]
fn imperceptible_on_every_kind() {
    for (name, kind, seed) in [("cloth", Kind::Cloth, 1), ("palette", Kind::Palette, 2), ("dark", Kind::Dark, 3), ("lace", Kind::Lace, 4), ("painted", Kind::Painted, 5)] {
        let (src, s) = signed(kind, 256, 256, seed);
        let d = metrics::diff(&src, &s);
        assert!(d.psnr >= 45.0, "{name}: PSNR {:.1}", d.psnr);
        assert!(d.de_mean < 1.0, "{name}: mean dE2000 {:.3}", d.de_mean);
        assert!(d.max_abs <= 3, "{name}: max change {} levels", d.max_abs);
        assert_eq!(d.alpha_changed, 0, "{name}: alpha never written");
        // and at the mips the game samples at distance
        let (mut a, mut b) = (src.clone(), s.clone());
        for m in 1..=3 {
            a = metrics::mip(&a);
            b = metrics::mip(&b);
            let dm = metrics::diff(&a, &b);
            assert!(dm.de_mean < 1.0 && dm.psnr >= 45.0, "{name} mip {m}: dE {:.3}, PSNR {:.1}", dm.de_mean, dm.psnr);
        }
    }
}

#[test]
fn flat_colour_moves_at_most_two_levels() {
    let src = Image::new(256, 256, [128u8, 96, 64, 255].repeat(256 * 256));
    let mut s = src.clone();
    scheme::embed(&mut s, None, &params()).unwrap();
    let d = metrics::diff(&src, &s);
    assert!(d.max_abs <= 2, "flat colour moved {} levels", d.max_abs);
    assert!(d.de_max < 1.0, "worst pixel dE2000 {:.3}", d.de_max);
    // the mark keeps hue: R, G, B move together
    for (p, q) in src.rgba.chunks(4).zip(s.rgba.chunks(4)) {
        let dd: Vec<i32> = (0..3).map(|c| q[c] as i32 - p[c] as i32).collect();
        assert!(dd[0] == dd[1] && dd[1] == dd[2]);
    }
}

#[test]
fn detects_raw_and_decodes_payload() {
    let (src, s) = signed(Kind::Cloth, 512, 512, 11);
    let (z0, _, _) = z(&src);
    let f = scheme::detect(&s.rgba, s.w, s.h, key(), None).unwrap();
    assert!(z0 < 4.0, "unsigned z {z0:.2}");
    assert!(f.z > 30.0 && f.step == 0, "signed z {:.1} step {}", f.z, f.step);
    let p = f.payload.unwrap();
    assert_eq!(p.mod_id.value, scheme::mod_id("maid_lash"));
    assert_eq!(scheme::date_text(p.day.value), "2026-11-19");
    assert!(p.log10_p() < -9.0);
    // the wrong key sees nothing
    let other = scheme::detect(&s.rgba, s.w, s.h, Key::new("someone_else"), None).unwrap();
    assert!(other.z < 4.0, "wrong key z {:.2}", other.z);
}

#[test]
fn survives_block_compression() {
    // (kind, size, format, minimum z after compression); 512^2 = 16,384 cells
    for (name, f) in [
        ("cloth", Format::Bc1),
        ("cloth", Format::Bc3),
        ("cloth", Format::Bc7),
        ("painted", Format::Bc1),
        ("painted", Format::Bc7),
        ("dark", Format::Bc1),
        ("dark", Format::Bc7),
        ("palette", Format::Bc1),
        ("palette", Format::Bc7),
    ] {
        let k = match name {
            "cloth" => Kind::Cloth,
            "painted" => Kind::Painted,
            "dark" => Kind::Dark,
            _ => Kind::Palette,
        };
        let (src, s) = signed(k, 512, 512, 21);
        let c = bc::round_trip(&s, f);
        let (zs, lp, step) = z(&c);
        let (zu, _, _) = z(&bc::round_trip(&src, f));
        eprintln!("{name:8} {:9} signed z {zs:6.1} (log10 p {lp:7.1}, step {step}), unsigned z {zu:5.2}", f.name());
        assert!(lp < -9.0, "{name} {}: z {zs:.1}", f.name());
        assert!(zu < 4.0, "{name} {}: unsigned z {zu:.2}", f.name());
    }
}

#[test]
fn survives_mips_and_resizing() {
    let (_, s) = signed(Kind::Cloth, 512, 512, 31);
    // compiled at half size (a max-size clamp), then BC7
    let half = bc::round_trip(&metrics::mip(&s), Format::Bc7);
    let (zh, lp, step) = z(&half);
    assert!(step == 1 && lp < -9.0, "half size: z {zh:.1} step {step}");
    // quarter size
    let q = metrics::mip(&metrics::mip(&s));
    let (zq, lp, step) = z(&q);
    assert!(step == 2 && lp < -6.0, "quarter size: z {zq:.1} step {step}");
    // a non-power-of-two source (296) marked on the 256 grid, resized to 256 by a compiler, then BC1
    let src = synth(Kind::Cloth, 296, 296, 32);
    let mut s = src.clone();
    let r = scheme::embed(&mut s, None, &params()).unwrap();
    assert_eq!(r.grid, scheme::Grid { gx: 64, gy: 64 });
    let pot = bc::round_trip(&metrics::resize(&s, 256, 256), Format::Bc1);
    let (zp, lp, step) = z(&pot);
    assert!(step == 0 && lp < -9.0, "npot -> pot: z {zp:.1} step {step}");
    // the same source compiled at its own npot size
    let (zn, lp, _) = z(&bc::round_trip(&s, Format::Bc7));
    assert!(lp < -9.0, "npot kept: z {zn:.1}");
}

#[test]
fn alpha_coverage_is_untouched() {
    let (src, s) = signed(Kind::Lace, 512, 512, 41);
    assert_eq!(src.alpha(), s.alpha());
    // no colour change within COVERAGE_GUARD pixels of a non-opaque pixel
    let a = src.alpha();
    for y in 0..s.h {
        for x in 0..s.w {
            if s.rgba[4 * (y * s.w + x)..4 * (y * s.w + x) + 3] != src.rgba[4 * (y * s.w + x)..4 * (y * s.w + x) + 3] {
                for yy in y.saturating_sub(scheme::COVERAGE_GUARD)..(y + scheme::COVERAGE_GUARD + 1).min(s.h) {
                    for xx in x.saturating_sub(scheme::COVERAGE_GUARD)..(x + scheme::COVERAGE_GUARD + 1).min(s.w) {
                        assert_eq!(a[yy * s.w + xx], 255, "changed pixel ({x},{y}) near an alpha edge");
                    }
                }
            }
        }
    }
    // after compression and at every mip, coverage at the alpha-test cutoff is identical
    let (mut a, mut b) = (src.clone(), s.clone());
    for m in 0..5 {
        for f in [Format::Bc3, Format::Bc7] {
            let (ca, cb) = (bc::round_trip(&a, f), bc::round_trip(&b, f));
            let cov = |i: &Image| i.rgba.chunks(4).map(|p| p[3] >= 128).collect::<Vec<_>>();
            assert_eq!(cov(&ca), cov(&cb), "mip {m} {}: coverage changed", f.name());
            if f == Format::Bc3 {
                assert_eq!(ca.alpha(), cb.alpha(), "mip {m} BC3: alpha is a separate block, bit-identical");
            }
        }
        a = metrics::mip(&a);
        b = metrics::mip(&b);
    }
    // and the lace still detects after BC3
    let (zl, lp, _) = z(&bc::round_trip(&s, Format::Bc3));
    assert!(lp < -9.0, "lace z {zl:.1}");
}

#[test]
fn guard_map_from_a_translucency_texture() {
    // colour map opaque, the material's translucency packed into alpha has holes: the guard keeps clear
    let src = synth(Kind::Cloth, 256, 256, 51);
    let holes = synth(Kind::Lace, 256, 256, 52).alpha();
    let mut s = src.clone();
    scheme::embed(&mut s, Some(&holes), &params()).unwrap();
    for i in 0..256 * 256 {
        if holes[i] < 255 {
            assert_eq!(s.rgba[4 * i..4 * i + 3], src.rgba[4 * i..4 * i + 3]);
        }
    }
}

#[test]
fn packed_metalness_edges_stay_clean() {
    // metal on the left and a metal ring on the right, packed by the compiler into the colour map's alpha
    let src = synth(Kind::Cloth, 256, 256, 55);
    let metal: Vec<u8> = (0..256 * 256)
        .map(|i| {
            let (x, y) = (i % 256, i / 256);
            let (dx, dy) = (x as f64 - 190.0, y as f64 - 128.0);
            if x < 100 || (dx * dx + dy * dy).sqrt() < 30.0 {
                255
            } else {
                0
            }
        })
        .collect();
    let mut s = src.clone();
    let r = scheme::embed_guarded(&mut s, None, Some(&metal), &params()).unwrap();
    assert!(r.marked > 2000, "{} cells marked", r.marked);
    for y in 0..256usize {
        for x in 0..256usize {
            if s.rgba[4 * (y * 256 + x)..4 * (y * 256 + x) + 3] != src.rgba[4 * (y * 256 + x)..4 * (y * 256 + x) + 3] {
                let v = metal[y * 256 + x];
                for yy in y.saturating_sub(scheme::GUARD)..(y + scheme::GUARD + 1).min(256) {
                    for xx in x.saturating_sub(scheme::GUARD)..(x + scheme::GUARD + 1).min(256) {
                        assert_eq!(metal[yy * 256 + xx], v, "changed ({x},{y}) near a metalness edge");
                    }
                }
            }
        }
    }
    // with the metalness in alpha, BC7 (colour and alpha coded together) moves it by at most the p-bit's one
    // level here; the real compiler's multi-subset modes moved an unguarded skin shell's by up to 10
    // levels at metal edges, which is what the guard is for
    let pack = |i: &Image| {
        let mut o = i.clone();
        for (p, m) in o.rgba.chunks_mut(4).zip(&metal) {
            p[3] = *m;
        }
        o
    };
    let (a, b) = (bc::round_trip(&pack(&src), Format::Bc7), bc::round_trip(&pack(&s), Format::Bc7));
    let worst = a.rgba.chunks(4).zip(b.rgba.chunks(4)).map(|(p, q)| p[3].abs_diff(q[3])).max().unwrap();
    assert!(worst <= 1, "packed metalness moved {worst} levels");
}

#[test]
fn false_positive_bound_holds_on_unsigned_textures() {
    // 300 unsigned textures of every kind: the Hoeffding bound says P(z >= t) <= 4 exp(-t^2/2) per texture
    let mut zs = Vec::new();
    for i in 0..300u64 {
        let k = match i % 5 {
            0 => Kind::Cloth,
            1 => Kind::Palette,
            2 => Kind::Dark,
            3 => Kind::Lace,
            _ => Kind::Painted,
        };
        let img = synth(k, 128, 128, 1000 + i);
        if let Some(f) = scheme::detect(&img.rgba, img.w, img.h, key(), None) {
            zs.push(f.z);
        }
    }
    let n = zs.len() as f64;
    for t in [2.0f64, 2.5, 3.0] {
        let frac = zs.iter().filter(|&&z| z >= t).count() as f64 / n;
        let bound = (4.0 * (-t * t / 2.0).exp()).min(1.0);
        assert!(frac <= bound, "P(z >= {t}) = {frac:.3} exceeds the bound {bound:.3}");
    }
    let max = zs.iter().cloned().fold(f64::MIN, f64::max);
    assert!(max < 4.5, "largest unsigned z {max:.2}");
}

#[test]
fn pooling_combines_weak_textures() {
    // twelve 96^2 textures of busy paint, each too weak alone after BC1, add up together
    let mut pool = scheme::Pool::default();
    let mut single = Vec::new();
    for i in 0..12u64 {
        let (_, s) = signed(Kind::Painted, 96, 96, 60 + i);
        let c = bc::round_trip(&s, Format::Bc1);
        let f = scheme::detect(&c.rgba, c.w, c.h, key(), None).unwrap();
        // the pool takes every texture on the grid it was marked on (no search, so the bound stays exact)
        let (ss, vv) = scheme::presence(key(), f.canonical.as_ref().unwrap());
        single.push(ss / vv.sqrt());
        pool.add(f.canonical.as_ref().unwrap());
    }
    let (zp, lp) = pool.presence(key());
    let best = single.iter().cloned().fold(f64::MIN, f64::max);
    let mean = single.iter().sum::<f64>() / single.len() as f64;
    eprintln!("12 weak textures: single z mean {mean:.2}, best {best:.2}; pooled z {zp:.2}");
    // independent noise, coherent mark: the pooled z grows like sqrt(12) x the mean single z
    assert!(zp > best && zp > 0.8 * (12f64).sqrt() * mean, "pooled z {zp:.1} vs best single {best:.1}, mean {mean:.2}");
    assert!(lp < -3.0, "pooled log10 p {lp:.1}");
    let d = pool.payload(key()).unwrap();
    assert_eq!(d.mod_id.value, scheme::mod_id("maid_lash"));
    // a pool of unsigned textures stays quiet
    let mut quiet = scheme::Pool::default();
    for i in 0..12u64 {
        let img = synth(Kind::Painted, 96, 96, 90 + i);
        quiet.add(scheme::detect(&img.rgba, img.w, img.h, key(), None).unwrap().canonical.as_ref().unwrap());
    }
    assert!(quiet.presence(key()).0 < 4.0);
}

#[test]
fn secondary_key_is_a_separate_claim() {
    let src = synth(Kind::Cloth, 256, 256, 71);
    let mut s = src.clone();
    let secret = Key::new("a private phrase only the owner knows");
    let mut p = params();
    p.secret = Some(secret);
    scheme::embed(&mut s, None, &p).unwrap();
    let c = bc::round_trip(&s, Format::Bc7);
    let f = scheme::detect(&c.rgba, c.w, c.h, key(), Some(secret)).unwrap();
    assert!(f.z > 8.0 && f.secret_z.unwrap() > 8.0, "public z {:.1}, secret z {:.1}", f.z, f.secret_z.unwrap());
    let g = scheme::detect(&c.rgba, c.w, c.h, key(), Some(Key::new("a guess"))).unwrap();
    assert!(g.secret_z.unwrap() < 4.0);
}

#[test]
fn signing_is_idempotent_through_the_container() {
    let dir = std::env::temp_dir().join(format!("magician_mark_test_{}", std::process::id()));
    let mat = dir.join("materials").join("t");
    std::fs::create_dir_all(&mat).unwrap();
    let img = synth(Kind::Cloth, 256, 256, 81);
    std::fs::write(mat.join("cloth_color.png"), crate::png::write(256, 256, &img.rgba, 2, &[])).unwrap();
    std::fs::write(mat.join("cloth_rough.png"), crate::png::write(256, 256, &img.rgba, 2, &[])).unwrap();
    std::fs::write(mat.join("cloth.vmat"), "\"Layer0\"\n{\n\t\"TextureColor1\"\t\"materials/t/cloth_color.png\"\n\t\"TextureRoughness1\"\t\"materials/t/cloth_rough.png\"\n}\n").unwrap();
    let o = crate::addon::SignOptions { key: key(), secret: None, mod_name: "maid_lash".into(), day: 1000, dry_run: false, force: false, target_rms: scheme::TARGET_RMS };
    // a small card of busy paint compiled as DXT1 cannot carry a mark provable on its own: skipped, not written
    let busy = synth(Kind::Painted, 192, 192, 82);
    std::fs::write(mat.join("card.png"), crate::png::write(192, 192, &busy.rgba, 2, &[])).unwrap();
    std::fs::write(mat.join("card.vtex"), "\"m_fileName\" \"string\" \"materials/t/card.png\"\n\"m_colorSpace\" \"string\" \"srgb\"\n\"m_outputFormat\" \"string\" \"DXT1\"\n").unwrap();
    let first = crate::addon::sign_addon(&dir, &o, |_| {}).unwrap();
    assert_eq!(first.len(), 2, "the colour map and the card are colour sources; roughness is not");
    let card = first.iter().find(|e| e.path.ends_with("card.png")).unwrap();
    assert_eq!(card.status, crate::addon::Status::Skipped, "{}", card.note);
    assert!(card.note.contains("DXT1"), "{}", card.note);
    let first: Vec<_> = first.into_iter().filter(|e| e.path.ends_with("cloth_color.png")).collect();
    assert_eq!(first[0].status, crate::addon::Status::Signed);
    let after1 = std::fs::read(mat.join("cloth_color.png")).unwrap();
    crate::addon::write_manifest(&dir, &first, &o, "example-author").unwrap();
    assert!(crate::addon::check_sources(&dir).changed.is_empty());
    let second = crate::addon::sign_addon(&dir, &o, |_| {}).unwrap();
    let second: Vec<_> = second.into_iter().filter(|e| e.path.ends_with("cloth_color.png")).collect();
    assert_eq!(second[0].status, crate::addon::Status::Already);
    assert_eq!(std::fs::read(mat.join("cloth_color.png")).unwrap(), after1, "a second sign changes nothing");
    assert_eq!(std::fs::read(mat.join("cloth_rough.png")).unwrap(), crate::png::write(256, 256, &img.rgba, 2, &[]), "roughness untouched");
    // a generator overwriting the source is caught by the manifest check
    std::fs::write(mat.join("cloth_color.png"), crate::png::write(256, 256, &img.rgba, 2, &[])).unwrap();
    assert_eq!(crate::addon::check_sources(&dir).changed, vec!["materials/t/cloth_color.png".to_string()]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn dates_and_ids() {
    assert_eq!(scheme::parse_date("2024-01-01"), Some(0));
    assert_eq!(scheme::date_text(scheme::parse_date("2026-11-19").unwrap()), "2026-11-19");
    assert_eq!(scheme::date_text(scheme::parse_date("2028-02-29").unwrap()), "2028-02-29");
    assert_ne!(scheme::mod_id("maid_lash"), scheme::mod_id("example_skin"));
    assert_eq!(scheme::pow2_near(48), 64);
    assert_eq!(scheme::pow2_near(280), 256);
    assert_eq!(scheme::pow2_near(380), 256);
    assert_eq!(scheme::pow2_near(512), 512);
}

#[test]
fn compiled_output_names_map_to_sources() {
    let mut m = std::collections::BTreeMap::new();
    m.insert("materials/a/x_color.png".to_string(), "materials/a/x_color.png".to_string());
    m.insert("panorama/images/heroes/lash_card.psd".to_string(), "panorama/images/heroes/lash_card.psd".to_string());
    m.insert("materials/a/plume.vtex".to_string(), "materials/a/plume.png".to_string());
    assert_eq!(crate::addon::source_for_output("materials/a/x_color_png_706a99c7.vtex_c", &m).as_deref(), Some("materials/a/x_color.png"));
    assert_eq!(crate::addon::source_for_output("panorama/images/heroes/lash_card_psd.vtex_c", &m).as_deref(), Some("panorama/images/heroes/lash_card.psd"));
    assert_eq!(crate::addon::source_for_output("materials/a/plume.vtex_c", &m).as_deref(), Some("materials/a/plume.png"));
    assert_eq!(crate::addon::source_for_output("materials/a/x_vmat_g_tcolor_12345678.vtex_c", &m), None);
}

/// z per kind, size and format, raw and through our encoders: `cargo test -p magician-mark -- --ignored robustness_table --nocapture`.
#[test]
#[ignore]
fn robustness_table() {
    for size in [256usize, 512, 1024] {
        for (name, k) in [("cloth", 0), ("palette", 1), ("dark", 2), ("lace", 3), ("painted", 4)] {
            let mk = || match k {
                0 => Kind::Cloth,
                1 => Kind::Palette,
                2 => Kind::Dark,
                3 => Kind::Lace,
                _ => Kind::Painted,
            };
            let (src, s) = signed(mk(), size, size, 7 + k as u64);
            let d = metrics::diff(&src, &s);
            let mut row = format!("{size:5} {name:8} psnr {:5.1} dE {:.3} max {} | raw {:6.1}", d.psnr, d.de_mean, d.max_abs, z(&s).0);
            for f in [Format::Bc1, Format::Bc3, Format::Bc7] {
                row += &format!(" {} {:6.1}", f.name(), z(&bc::round_trip(&s, f)).0);
            }
            row += &format!(" | half+BC7 {:6.1} | unsigned BC7 {:5.2}", z(&bc::round_trip(&metrics::mip(&s), Format::Bc7)).0, z(&bc::round_trip(&src, Format::Bc7)).0);
            eprintln!("{row}");
        }
    }
}

/// Writes the synthetic sources for the real-compiler round trip (CSDK addon `magician_marktest`):
/// `MARK_FIXTURE_OUT=<dir> cargo test -p magician-mark -- --ignored write_compiler_fixtures`.
#[test]
#[ignore]
fn write_compiler_fixtures() {
    let Ok(dir) = std::env::var("MARK_FIXTURE_OUT") else { return };
    let dir = std::path::Path::new(&dir);
    std::fs::create_dir_all(dir).unwrap();
    let put = |name: &str, img: &Image, ct: u8| std::fs::write(dir.join(name), crate::png::write(img.w, img.h, &img.rgba, ct, &[])).unwrap();
    put("cloth_bc1.png", &synth(Kind::Cloth, 512, 512, 101), 2);
    put("cloth_bc3.png", &synth(Kind::Cloth, 512, 512, 102), 2);
    put("cloth_bc7.png", &synth(Kind::Cloth, 512, 512, 103), 2);
    put("painted_bc1.png", &synth(Kind::Painted, 512, 512, 104), 2);
    put("painted_bc7.png", &synth(Kind::Painted, 512, 512, 105), 2);
    put("dark_bc1.png", &synth(Kind::Dark, 512, 512, 106), 2);
    put("palette_bc7.png", &synth(Kind::Palette, 512, 512, 107), 2);
    put("cloth_half.png", &synth(Kind::Cloth, 512, 512, 108), 2);
    put("npot_cloth.png", &synth(Kind::Cloth, 296, 296, 109), 2);
    put("lace_rgba_bc3.png", &synth(Kind::Lace, 512, 512, 110), 6);
    put("lace_rgba_bc7.png", &synth(Kind::Lace, 512, 512, 111), 6);
    put("lace_color.png", &synth(Kind::Cloth, 512, 512, 112), 2);
    let holes = synth(Kind::Lace, 512, 512, 113);
    let grey: Vec<u8> = holes.rgba.chunks(4).flat_map(|p| [p[3], p[3], p[3], 255]).collect();
    put("lace_trans.png", &Image::new(512, 512, grey), 0);
}

/// What the mark adds on top of the compiler's own error: each compiled texture of the signed and the
/// unsigned build against the unsigned source. `MARK_SRC` (unsigned fixture sources), `MARK_UNSIGNED`,
/// `MARK_SIGNED` (the two VPKs); `cargo test -p magician-mark -- --ignored compiled_error_budget`.
#[test]
#[ignore]
fn compiled_error_budget() {
    let (Ok(src), Ok(u), Ok(sg)) = (std::env::var("MARK_SRC"), std::env::var("MARK_UNSIGNED"), std::env::var("MARK_SIGNED")) else { return };
    let (u, sg) = (vpk::Vpk::open(&u).unwrap(), vpk::Vpk::open(&sg).unwrap());
    for e in u.entries.iter().filter(|e| e.path.ends_with(".vtex_c")) {
        let name = e.path.rsplit('/').next().unwrap();
        let stem = name.trim_end_matches(".vtex_c");
        let stem = stem.split("_png_").next().unwrap();
        let Ok((img, _)) = crate::image::load(&std::path::Path::new(&src).join(format!("{stem}.png"))) else { continue };
        let dec = |v: &vpk::Vpk| {
            let d = v.read(v.find(&e.path).unwrap()).unwrap();
            let (w, h, px) = resource::pixels::decode(&d, 0).unwrap();
            Image::new(w, h, px)
        };
        let (cu, cs) = (dec(&u), dec(&sg));
        if (cu.w, cu.h) != (img.w, img.h) {
            eprintln!("{stem:16} compiled {}x{} (resized): skipped", cu.w, cu.h);
            continue;
        }
        let mut src_rgb = img.clone();
        for (p, q) in src_rgb.rgba.chunks_mut(4).zip(cu.rgba.chunks(4)) {
            p[3] = q[3];
        }
        let (a, b) = (metrics::diff(&src_rgb, &cu), metrics::diff(&src_rgb, &cs));
        eprintln!("{stem:16} unsigned build vs source: PSNR {:5.1} dB, dE {:.3} | signed build vs unsigned source: PSNR {:5.1} dB, dE {:.3}", a.psnr, a.de_mean, b.psnr, b.de_mean);
    }
}

/// A compiled texture against a source image: `MARK_VPK`, `MARK_PATH` (in the VPK), `MARK_FILE` (png/tga/psd).
#[test]
#[ignore]
fn compiled_vs_source() {
    let (Ok(v), Ok(p), Ok(f)) = (std::env::var("MARK_VPK"), std::env::var("MARK_PATH"), std::env::var("MARK_FILE")) else { return };
    let v = vpk::Vpk::open(&v).unwrap();
    let d = v.read(v.find(&p).unwrap()).unwrap();
    let t = resource::texture::info(&d).unwrap();
    let (w, h, px) = crate::addon::decode_colour(&d).unwrap();
    let (src, _) = crate::image::load(std::path::Path::new(&f)).unwrap();
    eprintln!("compiled {}x{} {} mips {} flags {:#x}; source {}x{}", w, h, t.format_name(), t.mips, t.flags, src.w, src.h);
    let c = Image::new(w, h, px);
    if (w, h) == (src.w, src.h) {
        let dd = metrics::diff(&src, &c);
        eprintln!("colour PSNR {:.1} dB, dE {:.3}, alpha differs {}", dd.psnr, dd.de_mean, dd.alpha_changed);
        let hist = |i: &Image| { let mut hh = [0usize; 4]; for q in i.rgba.chunks(4) { hh[match q[3] { 0 => 0, 255 => 3, x if x < 128 => 1, _ => 2 }] += 1; } hh };
        eprintln!("alpha 0/<128/>=128/255: source {:?} compiled {:?}", hist(&src), hist(&c));
    }
    eprintln!("source z {:.1}, compiled {:?}", crate::addon::z_of(&src, crate::scheme::Key::new("example-author")), scheme::detect(&c.rgba, w, h, Key::new("example-author"), None).map(|f| (f.z, f.step)));
}

#[test]
fn ycocg_textures_decode_to_colour_and_detect() {
    // what Panorama's image-list route stores for small images: Y in alpha, Co and Cg offset by 128, scale 1
    let (_, s) = signed(Kind::Cloth, 256, 256, 91);
    let mut stored = Vec::with_capacity(s.rgba.len());
    for p in s.rgba.chunks(4) {
        let (r, g, b) = (p[0] as f64, p[1] as f64, p[2] as f64);
        let y = (r + 2.0 * g + b) / 4.0;
        let co = (r - b) / 2.0 + 128.0;
        let cg = (-r + 2.0 * g - b) / 4.0 + 128.0;
        stored.extend_from_slice(&[co.round() as u8, cg.round() as u8, 0, y.round() as u8]);
    }
    let mut data = resource::synth_with(&[], &[(*b"DATA", resource::texture::synth_header(256, 256, 4)), (*b"REDI", b"Texture Compiler Version Image YCoCg Conversion".to_vec())]);
    data.extend_from_slice(&stored);
    let (w, h, px) = crate::addon::decode_colour(&data).unwrap();
    let back = Image::new(w, h, px);
    let d = metrics::diff(&s, &back);
    assert!(d.psnr > 44.0 && d.max_abs <= 2, "YCoCg round trip PSNR {:.1}, max {}", d.psnr, d.max_abs);
    let r = crate::addon::verify_vtex("t.vtex_c", &data, key(), None).unwrap();
    assert!(r.signed(), "z {:?}", r.found.map(|f| f.z));
    // read as plain RGBA, the same file is noise to the detector
    let (_, _, raw) = resource::pixels::decode(&data, 0).unwrap();
    assert!(scheme::detect(&raw, 256, 256, key(), None).map_or(0.0, |f| f.z) < 5.0);
}

#[test]
fn a_manifest_names_the_key_it_was_signed_with() {
    let t = "# magician sign: ...\n# key Jane Example (public identity) | mod example_skin (1a2b) | date 2026-11-24 | secondary key no\n# path\tstatus\n";
    assert_eq!(crate::addon::manifest_key_of(t), Some(("Jane Example".to_string(), "example_skin".to_string())));
    assert_eq!(crate::addon::manifest_key_of("# path\tstatus\n"), None);
    assert_eq!(crate::addon::manifest_key_of("# key  (public identity) | mod x (0000)\n"), None, "an empty key is no key");
}
