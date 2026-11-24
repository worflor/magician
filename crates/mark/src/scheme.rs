//! The mark: keyed spread spectrum on 4x4-pixel cells of luminance, blind detection by a whitened
//! correlation with a rigorous false-positive bound, payload by Hadamard code division.
//!
//! **Grid.** A W x H texture carries a grid of Gx x Gy cells, Gx = P(W)/4, Gy = P(H)/4, where P rounds to the
//! nearest power of two (ties up). For the power-of-two textures a compiler accepts, a cell is exactly one
//! 4x4 block of BC1/BC3/BC7, so the mark is a per-block DC offset: the one thing block compression keeps
//! best. Cells are defined in normalised texture coordinates (pixel x belongs to cell floor((x+½)·Gx/W)), so
//! the same grid is found again after resizing: a texture compiled at half size has 2x2-pixel cells, a
//! non-power-of-two source resized to the nearest power of two has 4x4 again.
//!
//! **Pattern.** Every cell i gets three ±1 values from a counter-based hash of (key, grid, cell): the presence
//! chip p_i, and for each payload symbol j a sign b_ji and a 16-bit word g_ji. The codeword for a 16-bit value v
//! is c_ji(v) = b_ji·(−1)^popcount(v ∧ g_ji): 65,536 codewords per symbol, pairwise orthogonal in expectation,
//! all decodable at once by one fast Walsh-Hadamard transform. The cell offset is
//!   m_i = √0.5·p_i + √0.25·c_1i(mod id) + √0.25·c_2i(day)        (unit power)
//! and with a private secondary key s, √0.35·p_i + √0.15·c_1i + √0.15·c_2i + √0.35·p^s_i.
//!
//! **Embedding.** Only cells whose pixels are all fully opaque and 32 pixels clear of any alpha edge (coverage below
//! 255, or a change in a packed metalness map) are marked, so every block straddling an alpha edge at mips 0-3
//! compiles exactly as unsigned; alpha is never written. The amplitude is perceptually shaped: a_i = 1 + min(σ_i/8, 1),
//! σ_i the luminance spread over the 3x3-cell neighbourhood (texture masks a change; flat colour gets the
//! floor), scaled so the cell offsets have an RMS of `target_rms` grey levels (0.9). The offset d_i = s·a_i·m_i
//! is added to R, G and B alike, realised in integers by a 4x4 ordered dither (so a 0.6-level offset is ten of
//! sixteen pixels one level up), then clamped.
//!
//! **Detection** (only the key and the compiled pixels). Luminance Y = (R + 2G + B)/4 (G carries six bits in
//! BC1). Cell means μ_i. Prediction residual r_i = μ_i − μ̂_i, where μ̂_i is an adaptive linear predictor fitted
//! to this image by least squares: a constant plus five symmetric rings of the 5x5 neighbourhood (the 4 edge
//! neighbours, the 4 diagonals, the 4 at distance 2, the 8 knight moves, the 4 far corners), never the cell
//! itself; where the window is incomplete, the mean of the valid 8-neighbours. This removes what the image
//! predicts of itself and keeps the white mark. Weights w_i = r_i / (e_i + ¼), e_i the mean r² over the 5x5
//! window: the locally optimal detector when the noise variance changes across the image (a cell in flat
//! colour says more than one in busy paint), then clipped at 3 robust deviations. Statistic S = Σ w_i p_i,
//! V = Σ w_i², z = S/√V. Cells are taken with and without the ones holding alpha under 250 (alpha is coverage
//! in a lace map, metalness in a colour map that packs it).
//!
//! **False positives.** For a texture without the mark, the weights w are a function of the image alone and
//! the chips p are independent fair signs, so by Hoeffding's inequality for Rademacher sums
//!   P(S ≥ t) ≤ exp(−t²/2V),  i.e. p ≤ exp(−z²/2),
//! with no assumption on the image (no Gaussian model, no independence of pixels). The detector tries 4 grid
//! scales (compiled at 2x, 1x, ½x, ¼x the marked size) and the two cell sets and takes the best, so the
//! per-texture bound is multiplied by the number of tries (at most 8). Across a pack, textures on the same grid
//! share chips, so the combined test pools the weights per grid, W_gi = Σ_textures w_i (each on its marked
//! grid, the cell set chosen from its alpha alone: no search), and applies the same bound to S = Σ W_gi p_gi,
//! V = Σ W_gi²: still exact.
//!
//! **Payload.** For symbol j, h(u) = Σ_{i: g_ji = u} w_i b_ji, then H = WHT(h) gives every codeword's
//! correlation at once, H(v) = Σ_i w_i c_ji(v). The decoded value is the argmax, with z = H(v)/√V and a
//! decoding bound p ≤ 65536·exp(−z²/2) (union over the codebook).

use crate::image::Image;

pub const CELL: usize = 4;
/// Margin (px) the mark keeps from any alpha edge: coverage below 255, or a change in a packed map (metalness).
/// A BC block at mip k is built from 4·2^k source pixels, so 32 px keeps every block that straddles an alpha
/// edge at mips 0-3 free of the mark by construction, and the compiler codes those blocks, alpha included,
/// exactly as it would without it. At 8 px, BC7 (colour and alpha coded together) moved Maid Lash's lace
/// coverage across the 0.5 cutoff in 2 of 22 million texels (mips 3 and 6) and its packed metalness by up to 20.
pub const GUARD: usize = 32;
/// The same margin for coverage (kept as its own name for the tests and docs that speak of coverage).
pub const COVERAGE_GUARD: usize = GUARD;
pub const TARGET_RMS: f64 = 0.9;
/// Alpha at or above this counts as opaque when the detector chooses cells.
const DETECT_OPAQUE: u8 = 250;

const TAG_PRESENCE: u64 = 0x7072_6573_656e_6365;
const TAG_MOD: u64 = 0x6d6f_6469_6400_0001;
const TAG_DAY: u64 = 0x6461_7900_0000_0002;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Key(pub u64);

impl Key {
    pub fn new(name: &str) -> Key {
        let mut h = 0xcbf2_9ce4_8422_2325u64;
        for b in b"magician-mark/1\0".iter().chain(name.as_bytes()) {
            h ^= *b as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
        Key(mix(h))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Grid {
    pub gx: u32,
    pub gy: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Payload {
    pub mod_id: u16,
    pub day: u16,
}

fn mix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

#[inline]
fn cell_hash(k: Key, tag: u64, g: Grid, cx: u32, cy: u32) -> u64 {
    mix(mix(mix(k.0 ^ tag) ^ ((g.gx as u64) << 32 | g.gy as u64)) ^ ((cx as u64) << 32 | cy as u64))
}

#[inline]
pub fn chip(k: Key, g: Grid, cx: u32, cy: u32) -> f64 {
    if cell_hash(k, TAG_PRESENCE, g, cx, cy) >> 63 == 1 {
        1.0
    } else {
        -1.0
    }
}

#[inline]
fn symbol_parts(k: Key, tag: u64, g: Grid, cx: u32, cy: u32) -> (f64, u16) {
    let h = cell_hash(k, tag, g, cx, cy);
    (if h >> 63 == 1 { 1.0 } else { -1.0 }, (h & 0xffff) as u16)
}

#[inline]
fn codeword(k: Key, tag: u64, g: Grid, cx: u32, cy: u32, v: u16) -> f64 {
    let (b, gw) = symbol_parts(k, tag, g, cx, cy);
    if (v & gw).count_ones() & 1 == 1 {
        -b
    } else {
        b
    }
}

/// Nearest power of two, ties up.
pub fn pow2_near(n: usize) -> usize {
    if n <= 1 {
        return 1;
    }
    let up = n.next_power_of_two();
    let down = up / 2;
    if n - down < up - n {
        down
    } else {
        up
    }
}

/// The grid a source of this size is marked on.
pub fn grid_for(w: usize, h: usize) -> Grid {
    Grid { gx: (pow2_near(w) / CELL).max(1) as u32, gy: (pow2_near(h) / CELL).max(1) as u32 }
}

/// Which cell each pixel column (or row) belongs to.
fn cell_index(n: usize, g: u32) -> Vec<u32> {
    (0..n).map(|x| (((2 * x + 1) as u64 * g as u64) / (2 * n as u64)) as u32).collect()
}

#[inline]
fn luma(p: &[u8]) -> f64 {
    (p[0] as f64 + 2.0 * p[1] as f64 + p[2] as f64) * 0.25
}

const BAYER: [[u8; 4]; 4] = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];

#[derive(Clone, Copy, Debug)]
pub struct EmbedParams {
    pub key: Key,
    pub secret: Option<Key>,
    pub payload: Payload,
    pub target_rms: f64,
}

#[derive(Clone, Debug)]
pub struct EmbedReport {
    pub grid: Grid,
    pub cells: usize,
    pub marked: usize,
    /// Pixels whose RGB changed.
    pub changed: usize,
    /// RMS of the cell offsets actually applied (grey levels).
    pub rms: f64,
}

/// Mark `img` in place. `guard_alpha`, when given, is the coverage the compiled texture will carry (a
/// translucency map the material packs into this texture's alpha); otherwise the image's own alpha.
pub fn embed(img: &mut Image, guard_alpha: Option<&[u8]>, p: &EmbedParams) -> Result<EmbedReport, String> {
    embed_guarded(img, guard_alpha, None, p)
}

/// As `embed`, plus `packed`: a non-coverage map the material packs into this texture's alpha (metalness).
/// Block compressors that code colour and alpha together (BC7) may re-choose a block that straddles one of
/// its edges, so the mark keeps GUARD pixels from wherever the packed value changes, as it does from
/// coverage below 255.
pub fn embed_guarded(img: &mut Image, guard_alpha: Option<&[u8]>, packed: Option<&[u8]>, p: &EmbedParams) -> Result<EmbedReport, String> {
    let (w, h) = (img.w, img.h);
    let g = grid_for(w, h);
    if g.gx < 8 || g.gy < 8 {
        return Err(format!("{w}x{h} is too small for a {}x{} cell grid", g.gx, g.gy));
    }
    let own_alpha;
    let alpha: &[u8] = match guard_alpha {
        Some(a) => a,
        None => {
            own_alpha = img.alpha();
            &own_alpha
        }
    };
    // integral images of coverage below 255 and of packed-map edges: a pixel is safe when none lies within
    // COVERAGE_GUARD / GUARD of it
    let mut integ = vec![0u32; (w + 1) * (h + 1)];
    let mut integ_e = vec![0u32; (w + 1) * (h + 1)];
    let edge = |x: usize, y: usize| -> bool {
        packed.is_some_and(|m| {
            let v = m[y * w + x];
            (x + 1 < w && m[y * w + x + 1] != v) || (y + 1 < h && m[(y + 1) * w + x] != v)
        })
    };
    for y in 0..h {
        let (mut row, mut row_e) = (0u32, 0u32);
        for x in 0..w {
            row += (alpha[y * w + x] != 255) as u32;
            row_e += edge(x, y) as u32;
            integ[(y + 1) * (w + 1) + x + 1] = integ[y * (w + 1) + x + 1] + row;
            integ_e[(y + 1) * (w + 1) + x + 1] = integ_e[y * (w + 1) + x + 1] + row_e;
        }
    }
    let clear = |t: &[u32], x: usize, y: usize, g: usize| -> bool {
        let (x0, y0) = (x.saturating_sub(g), y.saturating_sub(g));
        let (x1, y1) = ((x + g + 1).min(w), (y + g + 1).min(h));
        t[y1 * (w + 1) + x1] + t[y0 * (w + 1) + x0] - t[y0 * (w + 1) + x1] - t[y1 * (w + 1) + x0] == 0
    };
    let safe = |x: usize, y: usize| -> bool {
        clear(&integ, x, y, COVERAGE_GUARD) && (packed.is_none() || clear(&integ_e, x, y, GUARD))
    };
    let (cxs, cys) = (cell_index(w, g.gx), cell_index(h, g.gy));
    let (gx, gy) = (g.gx as usize, g.gy as usize);
    let n = gx * gy;
    let mut ok = vec![true; n];
    let mut cnt = vec![0f64; n];
    let mut sum = vec![0f64; n];
    let mut sq = vec![0f64; n];
    for y in 0..h {
        for x in 0..w {
            let c = cys[y] as usize * gx + cxs[x] as usize;
            let l = luma(&img.rgba[4 * (y * w + x)..]);
            cnt[c] += 1.0;
            sum[c] += l;
            sq[c] += l * l;
            if !safe(x, y) {
                ok[c] = false;
            }
        }
    }
    // perceptual amplitude from the luminance spread over the 3x3-cell neighbourhood
    let mut raw = vec![0f64; n];
    let mut acc = 0f64;
    let mut marked = 0usize;
    let mix_w: [f64; 4] = if p.secret.is_some() { [0.35f64.sqrt(), 0.15f64.sqrt(), 0.15f64.sqrt(), 0.35f64.sqrt()] } else { [0.5f64.sqrt(), 0.5, 0.5, 0.0] };
    for cy in 0..gy {
        for cx in 0..gx {
            let i = cy * gx + cx;
            if !ok[i] || cnt[i] == 0.0 {
                continue;
            }
            let (mut c, mut s, mut q) = (0.0, 0.0, 0.0);
            for ny in cy.saturating_sub(1)..(cy + 2).min(gy) {
                for nx in cx.saturating_sub(1)..(cx + 2).min(gx) {
                    let j = ny * gx + nx;
                    c += cnt[j];
                    s += sum[j];
                    q += sq[j];
                }
            }
            let sigma = (q / c - (s / c) * (s / c)).max(0.0).sqrt();
            let a = 1.0 + (sigma / 8.0).min(1.0);
            let (ux, uy) = (cx as u32, cy as u32);
            let mut m = mix_w[0] * chip(p.key, g, ux, uy)
                + mix_w[1] * codeword(p.key, TAG_MOD, g, ux, uy, p.payload.mod_id)
                + mix_w[2] * codeword(p.key, TAG_DAY, g, ux, uy, p.payload.day);
            if let Some(s) = p.secret {
                m += mix_w[3] * chip(s, g, ux, uy);
            }
            raw[i] = a * m;
            acc += raw[i] * raw[i];
            marked += 1;
        }
    }
    if marked == 0 {
        return Ok(EmbedReport { grid: g, cells: n, marked: 0, changed: 0, rms: 0.0 });
    }
    let scale = p.target_rms / (acc / marked as f64).sqrt();
    let mut changed = 0usize;
    let mut applied = 0f64;
    let mut seen = vec![false; n];
    for y in 0..h {
        for x in 0..w {
            let c = cys[y] as usize * gx + cxs[x] as usize;
            if !ok[c] {
                continue;
            }
            let d = scale * raw[c];
            if !seen[c] {
                seen[c] = true;
                applied += d * d;
            }
            let fl = d.floor();
            let th = (BAYER[y & 3][x & 3] as f64 + 0.5) / 16.0;
            let delta = fl as i32 + ((d - fl) > th) as i32;
            if delta == 0 {
                continue;
            }
            let px = &mut img.rgba[4 * (y * w + x)..4 * (y * w + x) + 3];
            let before = [px[0], px[1], px[2]];
            for v in px.iter_mut() {
                *v = (*v as i32 + delta).clamp(0, 255) as u8;
            }
            if before != [px[0], px[1], px[2]] {
                changed += 1;
            }
        }
    }
    Ok(EmbedReport { grid: g, cells: n, marked, changed, rms: (applied / marked as f64).sqrt() })
}

/// Whitened, clipped cell residuals of one image on one grid: the detector's weights.
#[derive(Clone, Debug)]
pub struct Weights {
    pub g: Grid,
    pub w: Vec<f64>,
    pub valid: usize,
}

/// `mask_alpha`: leave out cells with any texel under alpha 250 (coverage). A compiled colour map's alpha is
/// not always coverage (a material may pack metalness or a tint mask into it), so `detect` tries both.
pub fn weights(rgba: &[u8], w: usize, h: usize, g: Grid, mask_alpha: bool) -> Option<Weights> {
    let (gx, gy) = (g.gx as usize, g.gy as usize);
    if gx < 8 || gy < 8 || gx > w || gy > h {
        return None;
    }
    let (cxs, cys) = (cell_index(w, g.gx), cell_index(h, g.gy));
    let n = gx * gy;
    let mut cnt = vec![0u32; n];
    let mut sum = vec![0f64; n];
    let mut ok = vec![true; n];
    for y in 0..h {
        let row = cys[y] as usize * gx;
        for x in 0..w {
            let p = &rgba[4 * (y * w + x)..4 * (y * w + x) + 4];
            let c = row + cxs[x] as usize;
            cnt[c] += 1;
            sum[c] += luma(p);
            if mask_alpha && p[3] < DETECT_OPAQUE {
                ok[c] = false;
            }
        }
    }
    let mean: Vec<f64> = (0..n).map(|i| if cnt[i] > 0 { sum[i] / cnt[i] as f64 } else { 0.0 }).collect();
    let valid_cell: Vec<bool> = (0..n).map(|i| ok[i] && cnt[i] > 0).collect();
    let r = residuals(&mean, &valid_cell, gx, gy);
    let use_: Vec<bool> = r.iter().map(|x| x.is_some()).collect();
    let r: Vec<f64> = r.into_iter().map(|x| x.unwrap_or(0.0)).collect();
    let valid = use_.iter().filter(|u| **u).count();
    if valid < 64 {
        return None;
    }
    // locally inverse-variance weighting: a cell in busy texture says less than one in flat colour
    // (the locally optimal detector for noise whose variance changes across the image)
    let floor = 0.25f64;
    let mut sq = vec![0f64; n];
    for i in 0..n {
        if use_[i] {
            sq[i] = r[i] * r[i];
        }
    }
    let local = box_mean(&sq, &use_, gx, gy, 2);
    let mut wv: Vec<f64> = (0..n).map(|i| if use_[i] { r[i] / (local[i] + floor) } else { 0.0 }).collect();
    let mut mags: Vec<f64> = (0..n).filter(|&i| use_[i]).map(|i| wv[i].abs()).collect();
    let mid = valid / 2;
    mags.select_nth_unstable_by(mid, |a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let clip = 3.0 * 1.4826 * mags[mid];
    if clip > 0.0 {
        for x in wv.iter_mut() {
            *x = x.clamp(-clip, clip);
        }
    }
    Some(Weights { g, w: wv, valid })
}

/// Mean of `v` over the valid cells of a (2k+1)^2 window.
fn box_mean(v: &[f64], valid: &[bool], gx: usize, gy: usize, k: usize) -> Vec<f64> {
    let mut out = vec![0f64; v.len()];
    // separable: rows then columns, carrying sums and counts
    let mut rs = vec![0f64; v.len()];
    let mut rc = vec![0f64; v.len()];
    for y in 0..gy {
        for x in 0..gx {
            let (mut s, mut c) = (0.0, 0.0);
            for xx in x.saturating_sub(k)..(x + k + 1).min(gx) {
                let j = y * gx + xx;
                if valid[j] {
                    s += v[j];
                    c += 1.0;
                }
            }
            rs[y * gx + x] = s;
            rc[y * gx + x] = c;
        }
    }
    for y in 0..gy {
        for x in 0..gx {
            let (mut s, mut c) = (0.0, 0.0);
            for yy in y.saturating_sub(k)..(y + k + 1).min(gy) {
                s += rs[yy * gx + x];
                c += rc[yy * gx + x];
            }
            out[y * gx + x] = if c > 0.0 { s / c } else { 0.0 };
        }
    }
    out
}

/// The cell-mean residuals after removing what the neighbours predict: an adaptive linear predictor
/// (five symmetric neighbour rings of a 5x5 window plus a constant, fitted to this image by least squares)
/// where the whole window is valid, else the mean of the valid 8-neighbours (at least 3). The cell itself
/// never enters its own prediction, so the mark in it stays in the residual.
fn residuals(mean: &[f64], valid: &[bool], gx: usize, gy: usize) -> Vec<Option<f64>> {
    const RINGS: [&[(i32, i32)]; 5] = [
        &[(1, 0), (-1, 0), (0, 1), (0, -1)],
        &[(1, 1), (1, -1), (-1, 1), (-1, -1)],
        &[(2, 0), (-2, 0), (0, 2), (0, -2)],
        &[(1, 2), (2, 1), (-1, 2), (-2, 1), (1, -2), (2, -1), (-1, -2), (-2, -1)],
        &[(2, 2), (2, -2), (-2, 2), (-2, -2)],
    ];
    let n = gx * gy;
    let at = |x: i32, y: i32| -> Option<usize> {
        if x < 0 || y < 0 || x >= gx as i32 || y >= gy as i32 {
            None
        } else {
            let j = y as usize * gx + x as usize;
            valid[j].then_some(j)
        }
    };
    let features = |cx: i32, cy: i32| -> Option<[f64; 6]> {
        let mut f = [1.0, 0.0, 0.0, 0.0, 0.0, 0.0];
        for (k, ring) in RINGS.iter().enumerate() {
            let mut s = 0.0;
            for (dx, dy) in ring.iter() {
                s += mean[at(cx + dx, cy + dy)?];
            }
            f[k + 1] = s / ring.len() as f64;
        }
        Some(f)
    };
    // least squares fit over the cells with a complete window
    let mut ata = [[0f64; 6]; 6];
    let mut atb = [0f64; 6];
    let mut fitted = 0usize;
    for cy in 0..gy as i32 {
        for cx in 0..gx as i32 {
            let Some(i) = at(cx, cy) else { continue };
            let Some(f) = features(cx, cy) else { continue };
            for a in 0..6 {
                for b in 0..6 {
                    ata[a][b] += f[a] * f[b];
                }
                atb[a] += f[a] * mean[i];
            }
            fitted += 1;
        }
    }
    let coef = if fitted >= 256 { solve6(ata, atb) } else { None };
    let mut out = vec![None; n];
    for cy in 0..gy as i32 {
        for cx in 0..gx as i32 {
            let Some(i) = at(cx, cy) else { continue };
            if let (Some(c), Some(f)) = (coef, features(cx, cy)) {
                let pred: f64 = (0..6).map(|k| c[k] * f[k]).sum();
                out[i] = Some(mean[i] - pred);
                continue;
            }
            let (mut s, mut k) = (0.0, 0);
            for dy in -1..=1 {
                for dx in -1..=1 {
                    if (dx, dy) != (0, 0) {
                        if let Some(j) = at(cx + dx, cy + dy) {
                            s += mean[j];
                            k += 1;
                        }
                    }
                }
            }
            if k >= 3 {
                out[i] = Some(mean[i] - s / k as f64);
            }
        }
    }
    out
}

/// Solve a 6x6 symmetric system by Gaussian elimination with partial pivoting and a tiny ridge.
fn solve6(mut a: [[f64; 6]; 6], mut b: [f64; 6]) -> Option<[f64; 6]> {
    let tr: f64 = (0..6).map(|i| a[i][i]).sum();
    for (i, row) in a.iter_mut().enumerate() {
        row[i] += 1e-9 * tr;
    }
    for c in 0..6 {
        let p = (c..6).max_by(|&x, &y| a[x][c].abs().partial_cmp(&a[y][c].abs()).unwrap_or(std::cmp::Ordering::Equal))?;
        if a[p][c].abs() < 1e-12 {
            return None;
        }
        a.swap(c, p);
        b.swap(c, p);
        for r in c + 1..6 {
            let f = a[r][c] / a[c][c];
            for k in c..6 {
                a[r][k] -= f * a[c][k];
            }
            b[r] -= f * b[c];
        }
    }
    let mut x = [0f64; 6];
    for c in (0..6).rev() {
        let s: f64 = (c + 1..6).map(|k| a[c][k] * x[k]).sum();
        x[c] = (b[c] - s) / a[c][c];
    }
    Some(x)
}

/// log10 of the Hoeffding bound exp(−z²/2), times `tries`, capped at 1.
pub fn log10_p(z: f64, tries: f64) -> f64 {
    if z <= 0.0 {
        return 0.0;
    }
    (tries.log10() - z * z / (2.0 * std::f64::consts::LN_10)).min(0.0)
}

/// Presence statistic (S, V) for one key.
pub fn presence(k: Key, wt: &Weights) -> (f64, f64) {
    let gx = wt.g.gx as usize;
    let (mut s, mut v) = (0.0, 0.0);
    for (i, &w) in wt.w.iter().enumerate() {
        if w != 0.0 {
            s += w * chip(k, wt.g, (i % gx) as u32, (i / gx) as u32);
            v += w * w;
        }
    }
    (s, v)
}

fn symbol_hist(k: Key, tag: u64, wt: &Weights, hist: &mut [f64]) {
    let gx = wt.g.gx as usize;
    for (i, &w) in wt.w.iter().enumerate() {
        if w != 0.0 {
            let (b, u) = symbol_parts(k, tag, wt.g, (i % gx) as u32, (i / gx) as u32);
            hist[u as usize] += w * b;
        }
    }
}

/// In-place fast Walsh-Hadamard transform (unnormalised): a[v] = Σ_u a[u]·(−1)^popcount(u∧v).
pub fn fwht(a: &mut [f64]) {
    let n = a.len();
    let mut h = 1;
    while h < n {
        for i in (0..n).step_by(2 * h) {
            for j in i..i + h {
                let (x, y) = (a[j], a[j + h]);
                a[j] = x + y;
                a[j + h] = x - y;
            }
        }
        h *= 2;
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Symbol {
    pub value: u16,
    pub z: f64,
    /// Second-best codeword's z: the margin the decode has.
    pub runner_up: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct Decoded {
    pub mod_id: Symbol,
    pub day: Symbol,
}

impl Decoded {
    /// log10 of the union bound on a wrong decode of the weaker symbol: 65536·exp(−z²/2).
    pub fn log10_p(&self) -> f64 {
        log10_p(self.mod_id.z.min(self.day.z), 65536.0)
    }
}

fn best_symbol(hist: &mut [f64], v: f64) -> Symbol {
    fwht(hist);
    let sd = v.sqrt().max(1e-12);
    let (mut b, mut bz, mut r) = (0usize, f64::MIN, f64::MIN);
    for (i, &c) in hist.iter().enumerate() {
        let z = c / sd;
        if z > bz {
            r = bz;
            bz = z;
            b = i;
        } else if z > r {
            r = z;
        }
    }
    Symbol { value: b as u16, z: bz, runner_up: r }
}

pub fn decode_payload(k: Key, wts: &[&Weights]) -> Decoded {
    let mut hm = vec![0f64; 65536];
    let mut hd = vec![0f64; 65536];
    let mut v = 0.0;
    for wt in wts {
        symbol_hist(k, TAG_MOD, wt, &mut hm);
        symbol_hist(k, TAG_DAY, wt, &mut hd);
        v += wt.w.iter().map(|w| w * w).sum::<f64>();
    }
    Decoded { mod_id: best_symbol(&mut hm, v), day: best_symbol(&mut hd, v) }
}

/// The grids a compiled texture of this size may carry: the source grid at resize factors 2, 1, ½, ¼.
pub fn candidates(w: usize, h: usize) -> Vec<(i32, Grid)> {
    let base = grid_for(w, h);
    let mut out = Vec::new();
    for k in [0i32, 1, -1, 2] {
        let f = |g: u32| if k >= 0 { g << k } else { g >> (-k) };
        let g = Grid { gx: f(base.gx), gy: f(base.gy) };
        if g.gx >= 8 && g.gy >= 8 && g.gx as usize <= w && g.gy as usize <= h {
            out.push((k, g));
        }
    }
    out
}

#[derive(Clone, Debug)]
pub struct Found {
    /// Scale step: 0 = the size it was marked at, 1 = compiled at half size, -1 = double, 2 = quarter.
    pub step: i32,
    pub grid: Grid,
    pub cells: usize,
    pub z: f64,
    /// log10 of the per-texture bound, Bonferroni over the grids and cell sets tried.
    pub log10_p: f64,
    pub secret_z: Option<f64>,
    pub payload: Option<Decoded>,
    /// The canonical-grid weights, for pooling across a pack (absent when the canonical grid does not fit).
    pub canonical: Option<Weights>,
}

/// z at or above which a texture's payload is decoded on its own.
pub const DECODE_Z: f64 = 4.0;

/// Blind detection on RGBA8 pixels.
pub fn detect(rgba: &[u8], w: usize, h: usize, key: Key, secret: Option<Key>) -> Option<Found> {
    let cands = candidates(w, h);
    // with any alpha under the threshold, try the cells with and without it; the pooled (canonical) choice
    // is fixed by the image alone: mask when under half the texels are below it (coverage holes), else not
    // (alpha that carries another map)
    let low = rgba.chunks(4).filter(|p| p[3] < DETECT_OPAQUE).count();
    let variants: &[bool] = if low == 0 { &[true] } else { &[true, false] };
    let canonical_mask = low == 0 || low * 2 < w * h;
    let tries = (cands.len() * variants.len()).max(1) as f64;
    let mut best: Option<(f64, i32, Weights)> = None;
    let mut canonical = None;
    for (k, g) in cands {
        for &mask in variants {
            let Some(wt) = weights(rgba, w, h, g, mask) else { continue };
            let (s, v) = presence(key, &wt);
            let z = if v > 0.0 { s / v.sqrt() } else { 0.0 };
            if k == 0 && mask == canonical_mask {
                canonical = Some(wt.clone());
            }
            if best.as_ref().is_none_or(|b| z > b.0) {
                best = Some((z, k, wt));
            }
        }
    }
    let (z, step, wt) = best?;
    let secret_z = secret.map(|s| {
        let (ss, v) = presence(s, &wt);
        if v > 0.0 {
            ss / v.sqrt()
        } else {
            0.0
        }
    });
    let payload = if z >= DECODE_Z { Some(decode_payload(key, &[&wt])) } else { None };
    Some(Found { step, grid: wt.g, cells: wt.valid, z, log10_p: log10_p(z, tries), secret_z, payload, canonical })
}

/// Pooled evidence over a pack: weights summed per grid (same grid = same chips), so the Hoeffding bound
/// stays exact for the combined statistic.
#[derive(Default)]
pub struct Pool {
    groups: std::collections::BTreeMap<Grid, Vec<f64>>,
    pub textures: usize,
}

impl Pool {
    pub fn add(&mut self, wt: &Weights) {
        let e = self.groups.entry(wt.g).or_insert_with(|| vec![0.0; wt.w.len()]);
        for (a, b) in e.iter_mut().zip(&wt.w) {
            *a += b;
        }
        self.textures += 1;
    }
    fn as_weights(&self) -> Vec<Weights> {
        self.groups.iter().map(|(g, w)| Weights { g: *g, w: w.clone(), valid: w.iter().filter(|x| **x != 0.0).count() }).collect()
    }
    /// (z, log10 p) of the combined presence test for a key.
    pub fn presence(&self, k: Key) -> (f64, f64) {
        let (mut s, mut v) = (0.0, 0.0);
        for wt in self.as_weights() {
            let (a, b) = presence(k, &wt);
            s += a;
            v += b;
        }
        let z = if v > 0.0 { s / v.sqrt() } else { 0.0 };
        (z, log10_p(z, 1.0))
    }
    pub fn payload(&self, k: Key) -> Option<Decoded> {
        if self.groups.is_empty() {
            return None;
        }
        let ws = self.as_weights();
        Some(decode_payload(k, &ws.iter().collect::<Vec<_>>()))
    }
}

// ------------------------------------------------------------------------------------------- dates and ids

/// Days since 2024-01-01 for a civil date.
pub fn day_number(y: i64, m: i64, d: i64) -> i64 {
    days_from_civil(y, m, d) - days_from_civil(2024, 1, 1)
}

/// The civil date of a day number.
pub fn date_of(day: u16) -> (i64, i64, i64) {
    civil_from_days(day as i64 + days_from_civil(2024, 1, 1))
}

pub fn date_text(day: u16) -> String {
    let (y, m, d) = date_of(day);
    format!("{y:04}-{m:02}-{d:02}")
}

pub fn parse_date(s: &str) -> Option<u16> {
    let p: Vec<i64> = s.split('-').map(|x| x.parse().ok()).collect::<Option<_>>()?;
    if p.len() != 3 || !(1..=12).contains(&p[1]) || !(1..=31).contains(&p[2]) {
        return None;
    }
    u16::try_from(day_number(p[0], p[1], p[2])).ok()
}

pub fn today() -> u16 {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let days = (secs / 86_400) as i64 - (days_from_civil(2024, 1, 1) - days_from_civil(1970, 1, 1));
    days.clamp(0, u16::MAX as i64) as u16
}

// Howard Hinnant's civil calendar algorithms.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// A mod's 16-bit id: FNV-1a over its addon name, folded.
pub fn mod_id(name: &str) -> u16 {
    let mut h = 0x811c_9dc5u32;
    for b in name.to_ascii_lowercase().bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    ((h >> 16) ^ (h & 0xffff)) as u16
}
