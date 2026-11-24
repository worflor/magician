//! Block-compression encoders (BC1 = DXT1, BC3 = DXT5, BC7 mode 6), so the mark's survival can be measured
//! without the compiler, and a compiled-texture container to decode them with, so the round trip goes
//! through the same decoder (`resource::pixels`) that reads a shipped VPK.
//!
//! The encoders are honest, not optimal: principal-axis endpoints, nearest-palette indices and one
//! least-squares refit (BC1/BC3); BC7 uses mode 6 only (one subset, 7-bit RGBA endpoints with p-bits,
//! 16 levels). The real compiler's encoders are better, so these are the harsher test.

use crate::image::Image;

fn block(img: &Image, bx: usize, by: usize) -> [[f64; 4]; 16] {
    let mut b = [[0.0; 4]; 16];
    for y in 0..4 {
        for x in 0..4 {
            let (px, py) = ((bx * 4 + x).min(img.w - 1), (by * 4 + y).min(img.h - 1));
            let p = &img.rgba[4 * (py * img.w + px)..];
            b[y * 4 + x] = [p[0] as f64, p[1] as f64, p[2] as f64, p[3] as f64];
        }
    }
    b
}

/// Endpoints along the principal axis of `dims` channels.
fn principal_ends(px: &[[f64; 4]; 16], dims: usize) -> ([f64; 4], [f64; 4]) {
    let mut mean = [0.0; 4];
    for p in px {
        for c in 0..dims {
            mean[c] += p[c] / 16.0;
        }
    }
    let mut cov = [[0.0; 4]; 4];
    for p in px {
        for i in 0..dims {
            for j in 0..dims {
                cov[i][j] += (p[i] - mean[i]) * (p[j] - mean[j]);
            }
        }
    }
    let mut axis = [1.0, 1.0, 1.0, 1.0];
    for _ in 0..8 {
        let mut n = [0.0; 4];
        for i in 0..dims {
            for j in 0..dims {
                n[i] += cov[i][j] * axis[j];
            }
        }
        let len = n[..dims].iter().map(|v| v * v).sum::<f64>().sqrt();
        if len < 1e-9 {
            break;
        }
        for i in 0..dims {
            axis[i] = n[i] / len;
        }
    }
    let (mut lo, mut hi) = (f64::MAX, f64::MIN);
    for p in px {
        let t: f64 = (0..dims).map(|c| (p[c] - mean[c]) * axis[c]).sum();
        lo = lo.min(t);
        hi = hi.max(t);
    }
    let mut a = [0.0; 4];
    let mut b = [0.0; 4];
    for c in 0..dims {
        a[c] = (mean[c] + lo * axis[c]).clamp(0.0, 255.0);
        b[c] = (mean[c] + hi * axis[c]).clamp(0.0, 255.0);
    }
    (a, b)
}

fn to565(c: &[f64; 4]) -> u16 {
    let r = (c[0] * 31.0 / 255.0).round() as u16;
    let g = (c[1] * 63.0 / 255.0).round() as u16;
    let b = (c[2] * 31.0 / 255.0).round() as u16;
    (r << 11) | (g << 5) | b
}

fn from565(c: u16) -> [f64; 3] {
    // the decoder's own expansion (resource::pixels::rgb565)
    let (r, g, b) = (((c >> 11) & 31) as u32, ((c >> 5) & 63) as u32, (c & 31) as u32);
    [((r * 527 + 23) >> 6) as f64, ((g * 259 + 33) >> 6) as f64, ((b * 527 + 23) >> 6) as f64]
}

/// Encode a 4-colour BC1 block; returns (bytes, squared error).
fn bc1_block(px: &[[f64; 4]; 16]) -> ([u8; 8], f64) {
    let (a, b) = principal_ends(px, 3);
    let try_ends = |e0: u16, e1: u16| -> ([u8; 8], f64, [u8; 16]) {
        let (mut c0, mut c1) = (e0, e1);
        if c0 < c1 {
            std::mem::swap(&mut c0, &mut c1);
        }
        let (p0, p1) = (from565(c0), from565(c1));
        let mut pal = [[0.0; 3]; 4];
        pal[0] = p0;
        pal[1] = p1;
        for k in 0..3 {
            pal[2][k] = ((2.0 * p0[k] + p1[k]) / 3.0).floor();
            pal[3][k] = ((p0[k] + 2.0 * p1[k]) / 3.0).floor();
        }
        if c0 == c1 {
            for k in 0..3 {
                pal[2][k] = ((p0[k] + p1[k]) / 2.0).floor();
            }
            pal[3] = [0.0; 3];
        }
        let mut idx = [0u8; 16];
        let mut err = 0.0;
        let mut bits = 0u32;
        for (i, p) in px.iter().enumerate() {
            let (mut bi, mut be) = (0, f64::MAX);
            let n = if c0 == c1 { 3 } else { 4 };
            for (k, q) in pal.iter().enumerate().take(n) {
                let e = (0..3).map(|c| (p[c] - q[c]).powi(2)).sum::<f64>();
                if e < be {
                    be = e;
                    bi = k;
                }
            }
            idx[i] = bi as u8;
            err += be;
            bits |= (bi as u32) << (2 * i);
        }
        let mut out = [0u8; 8];
        out[0..2].copy_from_slice(&c0.to_le_bytes());
        out[2..4].copy_from_slice(&c1.to_le_bytes());
        out[4..8].copy_from_slice(&bits.to_le_bytes());
        (out, err, idx)
    };
    let (mut best, mut best_err, idx) = try_ends(to565(&a), to565(&b));
    // one least-squares refit of the endpoints given the indices
    let wt = [1.0, 0.0, 2.0 / 3.0, 1.0 / 3.0];
    let (mut aa, mut ab, mut bb) = (0.0, 0.0, 0.0);
    let mut ax = [0.0; 3];
    let mut bx = [0.0; 3];
    let c0_is_a = to565(&a) >= to565(&b);
    for (i, p) in px.iter().enumerate() {
        let al = wt[idx[i] as usize];
        let be = 1.0 - al;
        aa += al * al;
        ab += al * be;
        bb += be * be;
        for c in 0..3 {
            ax[c] += al * p[c];
            bx[c] += be * p[c];
        }
    }
    let det = aa * bb - ab * ab;
    if det.abs() > 1e-9 {
        let mut e0 = [0.0; 4];
        let mut e1 = [0.0; 4];
        for c in 0..3 {
            e0[c] = ((ax[c] * bb - bx[c] * ab) / det).clamp(0.0, 255.0);
            e1[c] = ((bx[c] * aa - ax[c] * ab) / det).clamp(0.0, 255.0);
        }
        let _ = c0_is_a;
        let (o, e, _) = try_ends(to565(&e0), to565(&e1));
        if e < best_err {
            best = o;
            best_err = e;
        }
    }
    (best, best_err)
}

fn alpha_block(px: &[[f64; 4]; 16]) -> [u8; 8] {
    let (mut lo, mut hi) = (255.0f64, 0.0f64);
    for p in px {
        lo = lo.min(p[3]);
        hi = hi.max(p[3]);
    }
    let (a0, a1) = (hi.round() as u32, lo.round() as u32);
    let mut pal = [0u32; 8];
    pal[0] = a0;
    pal[1] = a1;
    if a0 > a1 {
        for k in 1..7u32 {
            pal[k as usize + 1] = ((7 - k) * a0 + k * a1) / 7;
        }
    } else {
        for k in 1..5u32 {
            pal[k as usize + 1] = ((5 - k) * a0 + k * a1) / 5;
        }
        pal[6] = 0;
        pal[7] = 255;
    }
    let mut bits = 0u64;
    for (i, p) in px.iter().enumerate() {
        let (mut bi, mut be) = (0, f64::MAX);
        for (k, &q) in pal.iter().enumerate() {
            let e = (p[3] - q as f64).abs();
            if e < be {
                be = e;
                bi = k;
            }
        }
        bits |= (bi as u64) << (3 * i);
    }
    let mut out = [0u8; 8];
    out[0] = a0 as u8;
    out[1] = a1 as u8;
    out[2..8].copy_from_slice(&bits.to_le_bytes()[..6]);
    out
}

const W4: [u32; 16] = [0, 4, 9, 13, 17, 21, 26, 30, 34, 38, 43, 47, 51, 55, 60, 64];

fn bc7_mode6_block(px: &[[f64; 4]; 16]) -> [u8; 16] {
    let (a, b) = principal_ends(px, 4);
    // quantise each endpoint to 7 bits + a p-bit shared by its four channels
    let quant = |e: &[f64; 4]| -> ([u32; 4], u32) {
        let mut best = ([0u32; 4], 0u32, f64::MAX);
        for p in 0..2u32 {
            let mut q = [0u32; 4];
            let mut err = 0.0;
            for c in 0..4 {
                let v = ((e[c] - p as f64) / 2.0).round().clamp(0.0, 127.0) as u32;
                q[c] = v;
                err += (((v << 1) | p) as f64 - e[c]).powi(2);
            }
            if err < best.2 {
                best = (q, p, err);
            }
        }
        (best.0, best.1)
    };
    let (mut q0, mut p0) = quant(&a);
    let (mut q1, mut p1) = quant(&b);
    let full = |q: &[u32; 4], p: u32| -> [u32; 4] { [(q[0] << 1) | p, (q[1] << 1) | p, (q[2] << 1) | p, (q[3] << 1) | p] };
    let pick = |e0: [u32; 4], e1: [u32; 4]| -> [u32; 16] {
        let mut idx = [0u32; 16];
        for (i, p) in px.iter().enumerate() {
            let (mut bi, mut be) = (0, f64::MAX);
            for (k, &wgt) in W4.iter().enumerate() {
                let e: f64 = (0..4).map(|c| ((((64 - wgt) * e0[c] + wgt * e1[c] + 32) >> 6) as f64 - p[c]).powi(2)).sum();
                if e < be {
                    be = e;
                    bi = k as u32;
                }
            }
            idx[i] = bi;
        }
        idx
    };
    let mut idx = pick(full(&q0, p0), full(&q1, p1));
    if idx[0] >= 8 {
        std::mem::swap(&mut q0, &mut q1);
        std::mem::swap(&mut p0, &mut p1);
        for v in idx.iter_mut() {
            *v = 15 - *v;
        }
    }
    let mut bits: Vec<(u32, u32)> = vec![(1 << 6, 7)];
    for c in 0..4 {
        bits.push((q0[c], 7));
        bits.push((q1[c], 7));
    }
    bits.push((p0, 1));
    bits.push((p1, 1));
    for (i, &v) in idx.iter().enumerate() {
        bits.push((v, if i == 0 { 3 } else { 4 }));
    }
    let mut out = [0u8; 16];
    let mut pos = 0;
    for (v, n) in bits {
        for k in 0..n {
            if (v >> k) & 1 == 1 {
                out[pos / 8] |= 1 << (pos % 8);
            }
            pos += 1;
        }
    }
    debug_assert_eq!(pos, 128);
    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Bc1,
    Bc3,
    Bc7,
}

impl Format {
    pub fn vtex_code(self) -> u8 {
        match self {
            Format::Bc1 => 1,
            Format::Bc3 => 2,
            Format::Bc7 => 20,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Format::Bc1 => "BC1/DXT1",
            Format::Bc3 => "BC3/DXT5",
            Format::Bc7 => "BC7",
        }
    }
}

/// Block-compress one image.
pub fn encode(img: &Image, f: Format) -> Vec<u8> {
    let (bw, bh) = (img.w.div_ceil(4), img.h.div_ceil(4));
    let mut out = Vec::with_capacity(bw * bh * 16);
    for by in 0..bh {
        for bx in 0..bw {
            let px = block(img, bx, by);
            match f {
                Format::Bc1 => out.extend_from_slice(&bc1_block(&px).0),
                Format::Bc3 => {
                    out.extend_from_slice(&alpha_block(&px));
                    out.extend_from_slice(&bc1_block(&px).0);
                }
                Format::Bc7 => out.extend_from_slice(&bc7_mode6_block(&px)),
            }
        }
    }
    out
}

/// A compiled texture (`.vtex_c` layout: resource blocks, then mips smallest first) holding `mips`.
pub fn vtex_c(mips: &[Image], f: Format) -> Vec<u8> {
    let mut hdr = resource::texture::synth_header(mips[0].w as u16, mips[0].h as u16, f.vtex_code());
    hdr[27] = mips.len() as u8;
    let mut out = resource::synth_with(&[], &[(*b"DATA", hdr)]);
    for m in mips.iter().rev() {
        out.extend(encode(m, f));
    }
    out
}

/// Compress and decode back through `resource::pixels`: what a texture looks like after the compiler.
pub fn round_trip(img: &Image, f: Format) -> Image {
    let data = vtex_c(std::slice::from_ref(img), f);
    let (w, h, px) = resource::pixels::decode(&data, 0).expect("decode own encoding");
    Image::new(w, h, px)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gradient(w: usize, h: usize) -> Image {
        let mut v = Vec::new();
        for y in 0..h {
            for x in 0..w {
                v.extend_from_slice(&[(x * 255 / w) as u8, (y * 255 / h) as u8, ((x + y) * 127 / (w + h)) as u8, if x < w / 2 { 255 } else { (y * 255 / h) as u8 }]);
            }
        }
        Image::new(w, h, v)
    }

    #[test]
    fn encoders_decode_close_to_source() {
        let img = gradient(64, 32);
        for f in [Format::Bc1, Format::Bc3, Format::Bc7] {
            let back = round_trip(&img, f);
            let d = crate::metrics::diff(&img, &back);
            assert!(d.psnr > 35.0, "{} psnr {:.1}", f.name(), d.psnr);
            if f != Format::Bc1 {
                let ae: f64 = img.rgba.chunks(4).zip(back.rgba.chunks(4)).map(|(a, b)| (a[3] as f64 - b[3] as f64).abs()).sum::<f64>() / (64.0 * 32.0);
                assert!(ae < 3.0, "{} alpha error {ae}", f.name());
            }
        }
    }

    #[test]
    fn mip_chain_container_reads_back() {
        let img = gradient(32, 32);
        let m1 = crate::metrics::mip(&img);
        let data = vtex_c(&[img.clone(), m1.clone()], Format::Bc7);
        let (w, h, _) = resource::pixels::decode(&data, 1).unwrap();
        assert_eq!((w, h), (16, 16));
        let (w, h, _) = resource::pixels::decode(&data, 0).unwrap();
        assert_eq!((w, h), (32, 32));
    }
}
