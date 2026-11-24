//! Imperceptibility metrics (PSNR, CIEDE2000) and the image operations a compiler applies (mip chain in
//! linear light, box and bilinear resizing), so robustness can be measured without the compiler.

use crate::image::Image;

fn lin_table() -> &'static [f64; 256] {
    static T: std::sync::OnceLock<[f64; 256]> = std::sync::OnceLock::new();
    T.get_or_init(|| {
        let mut t = [0.0; 256];
        for (i, e) in t.iter_mut().enumerate() {
            let c = i as f64 / 255.0;
            *e = if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) };
        }
        t
    })
}

pub fn to_srgb8(l: f64) -> u8 {
    let c = if l <= 0.003_130_8 { 12.92 * l } else { 1.055 * l.powf(1.0 / 2.4) - 0.055 };
    (c * 255.0).round().clamp(0.0, 255.0) as u8
}

/// sRGB 8-bit to CIELAB (D65).
pub fn lab(p: &[u8]) -> [f64; 3] {
    let t = lin_table();
    let (r, g, b) = (t[p[0] as usize], t[p[1] as usize], t[p[2] as usize]);
    let x = (0.412_456_4 * r + 0.357_576_1 * g + 0.180_437_5 * b) / 0.950_47;
    let y = 0.212_672_9 * r + 0.715_152_2 * g + 0.072_175 * b;
    let z = (0.019_333_9 * r + 0.119_192 * g + 0.950_304_1 * b) / 1.088_83;
    let f = |v: f64| if v > 216.0 / 24389.0 { v.cbrt() } else { (24389.0 / 27.0 * v + 16.0) / 116.0 };
    let (fx, fy, fz) = (f(x), f(y), f(z));
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

/// CIEDE2000 colour difference (kL = kC = kH = 1), Sharma, Wu and Dalal's formulation.
pub fn ciede2000(a: [f64; 3], b: [f64; 3]) -> f64 {
    use std::f64::consts::PI;
    let (l1, a1, b1) = (a[0], a[1], a[2]);
    let (l2, a2, b2) = (b[0], b[1], b[2]);
    let c1 = (a1 * a1 + b1 * b1).sqrt();
    let c2 = (a2 * a2 + b2 * b2).sqrt();
    let cb = (c1 + c2) / 2.0;
    let g = 0.5 * (1.0 - (cb.powi(7) / (cb.powi(7) + 25f64.powi(7))).sqrt());
    let (a1p, a2p) = ((1.0 + g) * a1, (1.0 + g) * a2);
    let (c1p, c2p) = ((a1p * a1p + b1 * b1).sqrt(), (a2p * a2p + b2 * b2).sqrt());
    let hp = |bb: f64, ap: f64| {
        if bb == 0.0 && ap == 0.0 {
            0.0
        } else {
            let h = bb.atan2(ap).to_degrees();
            if h < 0.0 {
                h + 360.0
            } else {
                h
            }
        }
    };
    let (h1p, h2p) = (hp(b1, a1p), hp(b2, a2p));
    let dl = l2 - l1;
    let dc = c2p - c1p;
    let dhp = if c1p * c2p == 0.0 {
        0.0
    } else if (h2p - h1p).abs() <= 180.0 {
        h2p - h1p
    } else if h2p - h1p > 180.0 {
        h2p - h1p - 360.0
    } else {
        h2p - h1p + 360.0
    };
    let dh = 2.0 * (c1p * c2p).sqrt() * (dhp.to_radians() / 2.0).sin();
    let lbp = (l1 + l2) / 2.0;
    let cbp = (c1p + c2p) / 2.0;
    let hbp = if c1p * c2p == 0.0 {
        h1p + h2p
    } else if (h1p - h2p).abs() <= 180.0 {
        (h1p + h2p) / 2.0
    } else if h1p + h2p < 360.0 {
        (h1p + h2p + 360.0) / 2.0
    } else {
        (h1p + h2p - 360.0) / 2.0
    };
    let t = 1.0 - 0.17 * ((hbp - 30.0).to_radians()).cos() + 0.24 * ((2.0 * hbp).to_radians()).cos() + 0.32 * ((3.0 * hbp + 6.0).to_radians()).cos()
        - 0.20 * ((4.0 * hbp - 63.0).to_radians()).cos();
    let dtheta = 30.0 * (-((hbp - 275.0) / 25.0).powi(2)).exp();
    let rc = 2.0 * (cbp.powi(7) / (cbp.powi(7) + 25f64.powi(7))).sqrt();
    let sl = 1.0 + 0.015 * (lbp - 50.0).powi(2) / (20.0 + (lbp - 50.0).powi(2)).sqrt();
    let sc = 1.0 + 0.045 * cbp;
    let sh = 1.0 + 0.015 * cbp * t;
    let rt = -(2.0 * dtheta * PI / 180.0).sin() * rc;
    ((dl / sl).powi(2) + (dc / sc).powi(2) + (dh / sh).powi(2) + rt * (dc / sc) * (dh / sh)).sqrt()
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Diff {
    /// PSNR over RGB in dB (infinite when identical).
    pub psnr: f64,
    /// Mean CIEDE2000 over every pixel.
    pub de_mean: f64,
    /// Mean CIEDE2000 over the pixels that changed.
    pub de_changed: f64,
    pub de_max: f64,
    /// Largest per-channel change in grey levels.
    pub max_abs: u8,
    pub changed: usize,
    /// Pixels whose alpha differs.
    pub alpha_changed: usize,
}

pub fn diff(a: &Image, b: &Image) -> Diff {
    assert_eq!((a.w, a.h), (b.w, b.h));
    let mut se = 0f64;
    let (mut de_sum, mut de_max, mut changed, mut alpha_changed, mut max_abs) = (0f64, 0f64, 0usize, 0usize, 0u8);
    let mut cache: std::collections::HashMap<([u8; 3], [u8; 3]), f64> = Default::default();
    for (p, q) in a.rgba.chunks(4).zip(b.rgba.chunks(4)) {
        if p[3] != q[3] {
            alpha_changed += 1;
        }
        if p[..3] == q[..3] {
            continue;
        }
        changed += 1;
        for c in 0..3 {
            let d = p[c] as f64 - q[c] as f64;
            se += d * d;
            max_abs = max_abs.max((p[c] as i32 - q[c] as i32).unsigned_abs() as u8);
        }
        let k = ([p[0], p[1], p[2]], [q[0], q[1], q[2]]);
        let de = *cache.entry(k).or_insert_with(|| ciede2000(lab(p), lab(q)));
        de_sum += de;
        de_max = de_max.max(de);
    }
    let n = (a.w * a.h) as f64;
    let mse = se / (3.0 * n);
    Diff {
        psnr: if mse == 0.0 { f64::INFINITY } else { 10.0 * (255.0f64 * 255.0 / mse).log10() },
        de_mean: de_sum / n,
        de_changed: if changed > 0 { de_sum / changed as f64 } else { 0.0 },
        de_max,
        max_abs,
        changed,
        alpha_changed,
    }
}

/// One mip step: 2x2 box filter, colour averaged in linear light (an sRGB texture's mips), alpha linearly.
pub fn mip(img: &Image) -> Image {
    let (w, h) = ((img.w / 2).max(1), (img.h / 2).max(1));
    let t = lin_table();
    let mut out = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let mut acc = [0f64; 4];
            let mut k = 0.0;
            for dy in 0..2 {
                for dx in 0..2 {
                    let (sx, sy) = ((2 * x + dx).min(img.w - 1), (2 * y + dy).min(img.h - 1));
                    let p = &img.rgba[4 * (sy * img.w + sx)..];
                    for c in 0..3 {
                        acc[c] += t[p[c] as usize];
                    }
                    acc[3] += p[3] as f64;
                    k += 1.0;
                }
            }
            let o = &mut out[4 * (y * w + x)..4 * (y * w + x) + 4];
            for c in 0..3 {
                o[c] = to_srgb8(acc[c] / k);
            }
            o[3] = (acc[3] / k).round() as u8;
        }
    }
    Image::new(w, h, out)
}

/// Bilinear resample to any size (a compiler's power-of-two or max-size resize), in linear light.
pub fn resize(img: &Image, w: usize, h: usize) -> Image {
    let t = lin_table();
    let mut out = vec![0u8; w * h * 4];
    let (sx, sy) = (img.w as f64 / w as f64, img.h as f64 / h as f64);
    // a downscale averages a footprint (box over the source span), an upscale interpolates
    for y in 0..h {
        for x in 0..w {
            let mut acc = [0f64; 4];
            let mut wsum = 0.0;
            if sx > 1.0 || sy > 1.0 {
                let (x0, x1) = (x as f64 * sx, (x + 1) as f64 * sx);
                let (y0, y1) = (y as f64 * sy, (y + 1) as f64 * sy);
                for py in y0.floor() as usize..(y1.ceil() as usize).min(img.h) {
                    let wy = (y1.min(py as f64 + 1.0) - y0.max(py as f64)).max(0.0);
                    for px in x0.floor() as usize..(x1.ceil() as usize).min(img.w) {
                        let wx = (x1.min(px as f64 + 1.0) - x0.max(px as f64)).max(0.0);
                        let wgt = wx * wy;
                        let p = &img.rgba[4 * (py * img.w + px)..];
                        for c in 0..3 {
                            acc[c] += wgt * t[p[c] as usize];
                        }
                        acc[3] += wgt * p[3] as f64;
                        wsum += wgt;
                    }
                }
            } else {
                let fx = ((x as f64 + 0.5) * sx - 0.5).max(0.0);
                let fy = ((y as f64 + 0.5) * sy - 0.5).max(0.0);
                let (ix, iy) = (fx.floor() as usize, fy.floor() as usize);
                let (ax, ay) = (fx - ix as f64, fy - iy as f64);
                for (dy, wy) in [(0, 1.0 - ay), (1, ay)] {
                    for (dx, wx) in [(0, 1.0 - ax), (1, ax)] {
                        let (px, py) = ((ix + dx).min(img.w - 1), (iy + dy).min(img.h - 1));
                        let p = &img.rgba[4 * (py * img.w + px)..];
                        let wgt = wx * wy;
                        for c in 0..3 {
                            acc[c] += wgt * t[p[c] as usize];
                        }
                        acc[3] += wgt * p[3] as f64;
                        wsum += wgt;
                    }
                }
            }
            let o = &mut out[4 * (y * w + x)..4 * (y * w + x) + 4];
            for c in 0..3 {
                o[c] = to_srgb8(acc[c] / wsum);
            }
            o[3] = (acc[3] / wsum).round() as u8;
        }
    }
    Image::new(w, h, out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ciede2000_reference_pairs() {
        // Sharma, Wu, Dalal (2005) test data, pairs 1, 7, 17, 25
        let cases = [
            ([50.0, 2.6772, -79.7751], [50.0, 0.0, -82.7485], 2.0425),
            ([50.0, 0.0, 0.0], [50.0, -1.0, 2.0], 2.3669),
            ([50.0, 2.5, 0.0], [73.0, 25.0, -18.0], 27.1492),
            ([60.2574, -34.0099, 36.2677], [60.4626, -34.1751, 39.4387], 1.2644),
        ];
        for (a, b, want) in cases {
            let got = ciede2000(a, b);
            assert!((got - want).abs() < 1e-4, "{got} vs {want}");
        }
    }

    #[test]
    fn mip_and_resize_keep_flat_colour() {
        let img = Image::new(8, 8, [120u8, 40, 200, 255].repeat(64));
        let m = mip(&img);
        assert_eq!((m.w, m.h), (4, 4));
        assert!(m.rgba.chunks(4).all(|p| p == [120, 40, 200, 255]));
        let r = resize(&img, 5, 3);
        assert!(r.rgba.chunks(4).all(|p| p == [120, 40, 200, 255]));
        let r = resize(&img, 16, 16);
        assert!(r.rgba.chunks(4).all(|p| p == [120, 40, 200, 255]));
    }
}
