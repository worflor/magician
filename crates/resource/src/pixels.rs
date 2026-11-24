//! Texture pixels: a compiled texture's mip levels decoded to RGBA8, natively.
//!
//! Layout: after the resource blocks, the mips are stored smallest first (so mip 0, the largest, is the
//! tail of the file); this reads them from the end. Formats: BGRA8888, RGBA8888, DXT1 (BC1), DXT5
//! (BC3), ATI1N (BC4), ATI2N (BC5) and BC7, the format of every hero colour map on build 6864. BC6H
//! (HDR) and per-mip LZ4 (`CompressedMips`, which the hero maps don't use) are not decoded: `None`.

use crate::texture::{info, TexInfo};

/// Width and height of mip `mip`: halved per level, never below 1. The mip count is a byte from the file, so a
/// level past the 16th is simply 1 x 1, not a shift out of range.
fn mip_size(t: &TexInfo, mip: u32) -> (usize, usize) {
    let half = |d: u16| (d as usize).checked_shr(mip).unwrap_or(0).max(1);
    (half(t.width), half(t.height))
}

/// Bytes of one mip level.
pub fn mip_bytes(t: &TexInfo, mip: u32) -> Option<usize> {
    let (w, h) = mip_size(t, mip);
    let d = t.depth.max(1) as usize;
    let blocks = w.div_ceil(4) * h.div_ceil(4) * d;
    Some(match t.format {
        1 | 27 => blocks * 8,
        2 | 20 | 21 | 19 => blocks * 16,
        4 | 28 => w * h * d * 4,
        _ => return None,
    })
}

/// Decode mip `mip` of a compiled texture to RGBA8 (row-major, top row first): (width, height, pixels).
pub fn decode(data: &[u8], mip: u32) -> Option<(usize, usize, Vec<u8>)> {
    let t = info(data)?;
    if mip >= t.mips.max(1) as u32 {
        return None;
    }
    let total: usize = (0..t.mips.max(1) as u32).map(|m| mip_bytes(&t, m)).sum::<Option<usize>>()?;
    if data.len() < total {
        return None;
    }
    let base = data.len() - total;
    // smallest mip first: mip m sits after every smaller one
    let start = base + (mip + 1..t.mips.max(1) as u32).map(|m| mip_bytes(&t, m)).sum::<Option<usize>>()?;
    let len = mip_bytes(&t, mip)?;
    let src = &data[start..start + len];
    let (w, h) = mip_size(&t, mip);
    let mut out = vec![0u8; w * h * 4];
    match t.format {
        28 => {
            for i in 0..w * h {
                out[4 * i..4 * i + 4].copy_from_slice(&[src[4 * i + 2], src[4 * i + 1], src[4 * i], src[4 * i + 3]]);
            }
        }
        4 => out.copy_from_slice(&src[..w * h * 4]),
        _ => {
            let bw = w.div_ceil(4);
            let bsize = if matches!(t.format, 1 | 27) { 8 } else { 16 };
            for by in 0..h.div_ceil(4) {
                for bx in 0..bw {
                    let b = &src[(by * bw + bx) * bsize..(by * bw + bx + 1) * bsize];
                    let px = match t.format {
                        1 => bc1(b),
                        2 => bc3(b),
                        27 => bc4(b, false),
                        21 => bc4(b, true),
                        20 => bc7(b),
                        _ => return None,
                    };
                    for y in 0..4 {
                        for x in 0..4 {
                            let (ox, oy) = (bx * 4 + x, by * 4 + y);
                            if ox < w && oy < h {
                                out[4 * (oy * w + ox)..4 * (oy * w + ox) + 4].copy_from_slice(&px[y * 4 + x]);
                            }
                        }
                    }
                }
            }
        }
    }
    Some((w, h, out))
}

fn rgb565(c: u16) -> [u8; 3] {
    let r = ((c >> 11) & 31) as u32;
    let g = ((c >> 5) & 63) as u32;
    let b = (c & 31) as u32;
    [((r * 527 + 23) >> 6) as u8, ((g * 259 + 33) >> 6) as u8, ((b * 527 + 23) >> 6) as u8]
}

fn bc1_colors(b: &[u8], four: bool) -> [[u8; 4]; 16] {
    let c0 = u16::from_le_bytes([b[0], b[1]]);
    let c1 = u16::from_le_bytes([b[2], b[3]]);
    let (a, z) = (rgb565(c0), rgb565(c1));
    let mut pal = [[a[0], a[1], a[2], 255], [z[0], z[1], z[2], 255], [0; 4], [0; 4]];
    let mix = |p: u32, q: u32, wp: u32, wq: u32, d: u32| ((p * wp + q * wq) / d) as u8;
    if c0 > c1 || four {
        for k in 0..3 {
            pal[2][k] = mix(a[k] as u32, z[k] as u32, 2, 1, 3);
            pal[3][k] = mix(a[k] as u32, z[k] as u32, 1, 2, 3);
        }
        pal[2][3] = 255;
        pal[3][3] = 255;
    } else {
        for k in 0..3 {
            pal[2][k] = mix(a[k] as u32, z[k] as u32, 1, 1, 2);
        }
        pal[2][3] = 255;
        pal[3] = [0, 0, 0, 0];
    }
    let bits = u32::from_le_bytes([b[4], b[5], b[6], b[7]]);
    let mut out = [[0u8; 4]; 16];
    for i in 0..16 {
        out[i] = pal[((bits >> (2 * i)) & 3) as usize];
    }
    out
}

fn bc1(b: &[u8]) -> [[u8; 4]; 16] {
    bc1_colors(b, false)
}

fn alpha_block(b: &[u8]) -> [u8; 16] {
    let (a0, a1) = (b[0] as u32, b[1] as u32);
    let mut pal = [0u8; 8];
    pal[0] = a0 as u8;
    pal[1] = a1 as u8;
    if a0 > a1 {
        for k in 1..7 {
            pal[k + 1] = (((7 - k) as u32 * a0 + k as u32 * a1) / 7) as u8;
        }
    } else {
        for k in 1..5 {
            pal[k + 1] = (((5 - k) as u32 * a0 + k as u32 * a1) / 5) as u8;
        }
        pal[6] = 0;
        pal[7] = 255;
    }
    let mut bits = 0u64;
    for k in 0..6 {
        bits |= (b[2 + k] as u64) << (8 * k);
    }
    let mut out = [0u8; 16];
    for i in 0..16 {
        out[i] = pal[((bits >> (3 * i)) & 7) as usize];
    }
    out
}

fn bc3(b: &[u8]) -> [[u8; 4]; 16] {
    let a = alpha_block(&b[0..8]);
    let mut c = bc1_colors(&b[8..16], true);
    for i in 0..16 {
        c[i][3] = a[i];
    }
    c
}

fn bc4(b: &[u8], two: bool) -> [[u8; 4]; 16] {
    let r = alpha_block(&b[0..8]);
    let g = if two { alpha_block(&b[8..16]) } else { [0; 16] };
    let mut out = [[0u8; 4]; 16];
    for i in 0..16 {
        out[i] = [r[i], g[i], 0, 255];
    }
    out
}

// ---- BC7 ----

const P2: [u16; 64] = [
    0xcccc, 0x8888, 0xeeee, 0xecc8, 0xc880, 0xfeec, 0xfec8, 0xec80, 0xc800, 0xffec, 0xfe80, 0xe800, 0xffe8, 0xff00, 0xfff0, 0xf000, 0xf710, 0x008e, 0x7100, 0x08ce, 0x008c, 0x7310, 0x3100,
    0x8cce, 0x088c, 0x3110, 0x6666, 0x366c, 0x17e8, 0x0ff0, 0x718e, 0x399c, 0xaaaa, 0xf0f0, 0x5a5a, 0x33cc, 0x3c3c, 0x55aa, 0x9696, 0xa55a, 0x73ce, 0x13c8, 0x324c, 0x3bdc, 0x6996, 0xc33c,
    0x9966, 0x0660, 0x0272, 0x04e4, 0x4e40, 0x2720, 0xc936, 0x936c, 0x39c6, 0x639c, 0x9336, 0x9cc6, 0x817e, 0xe718, 0xccf0, 0x0fcc, 0x7744, 0xee22,
];

const P3: [[u8; 16]; 64] = [
    [0, 0, 1, 1, 0, 0, 1, 1, 0, 2, 2, 1, 2, 2, 2, 2], [0, 0, 0, 1, 0, 0, 1, 1, 2, 2, 1, 1, 2, 2, 2, 1], [0, 0, 0, 0, 2, 0, 0, 1, 2, 2, 1, 1, 2, 2, 1, 1], [0, 2, 2, 2, 0, 0, 2, 2, 0, 0, 1, 1, 0, 1, 1, 1],
    [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 2, 2, 1, 1, 2, 2], [0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 2, 2, 0, 0, 2, 2], [0, 0, 2, 2, 0, 0, 2, 2, 1, 1, 1, 1, 1, 1, 1, 1], [0, 0, 1, 1, 0, 0, 1, 1, 2, 2, 1, 1, 2, 2, 1, 1],
    [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2], [0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2], [0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 2, 2, 2, 2], [0, 0, 1, 2, 0, 0, 1, 2, 0, 0, 1, 2, 0, 0, 1, 2],
    [0, 1, 1, 2, 0, 1, 1, 2, 0, 1, 1, 2, 0, 1, 1, 2], [0, 1, 2, 2, 0, 1, 2, 2, 0, 1, 2, 2, 0, 1, 2, 2], [0, 0, 1, 1, 0, 1, 1, 2, 1, 1, 2, 2, 1, 2, 2, 2], [0, 0, 1, 1, 2, 0, 0, 1, 2, 2, 0, 0, 2, 2, 2, 0],
    [0, 0, 0, 1, 0, 0, 1, 1, 0, 1, 1, 2, 1, 1, 2, 2], [0, 1, 1, 1, 0, 0, 1, 1, 2, 0, 0, 1, 2, 2, 0, 0], [0, 0, 0, 0, 1, 1, 2, 2, 1, 1, 2, 2, 1, 1, 2, 2], [0, 0, 2, 2, 0, 0, 2, 2, 0, 0, 2, 2, 1, 1, 1, 1],
    [0, 1, 1, 1, 0, 1, 1, 1, 0, 2, 2, 2, 0, 2, 2, 2], [0, 0, 0, 1, 0, 0, 0, 1, 2, 2, 2, 1, 2, 2, 2, 1], [0, 0, 0, 0, 0, 0, 1, 1, 0, 1, 2, 2, 0, 1, 2, 2], [0, 0, 0, 0, 1, 1, 0, 0, 2, 2, 1, 0, 2, 2, 1, 0],
    [0, 1, 2, 2, 0, 1, 2, 2, 0, 0, 1, 1, 0, 0, 0, 0], [0, 0, 1, 2, 0, 0, 1, 2, 1, 1, 2, 2, 2, 2, 2, 2], [0, 1, 1, 0, 1, 2, 2, 1, 1, 2, 2, 1, 0, 1, 1, 0], [0, 0, 0, 0, 0, 1, 1, 0, 1, 2, 2, 1, 1, 2, 2, 1],
    [0, 0, 2, 2, 1, 1, 0, 2, 1, 1, 0, 2, 0, 0, 2, 2], [0, 1, 1, 0, 0, 1, 1, 0, 2, 0, 0, 2, 2, 2, 2, 2], [0, 0, 1, 1, 0, 1, 2, 2, 0, 1, 2, 2, 0, 0, 1, 1], [0, 0, 0, 0, 2, 0, 0, 0, 2, 2, 1, 1, 2, 2, 2, 1],
    [0, 0, 0, 0, 0, 0, 0, 2, 1, 1, 2, 2, 1, 2, 2, 2], [0, 2, 2, 2, 0, 0, 2, 2, 0, 0, 1, 2, 0, 0, 1, 1], [0, 0, 1, 1, 0, 0, 1, 2, 0, 0, 2, 2, 0, 2, 2, 2], [0, 1, 2, 0, 0, 1, 2, 0, 0, 1, 2, 0, 0, 1, 2, 0],
    [0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 0, 0, 0, 0], [0, 1, 2, 0, 1, 2, 0, 1, 2, 0, 1, 2, 0, 1, 2, 0], [0, 1, 2, 0, 2, 0, 1, 2, 1, 2, 0, 1, 0, 1, 2, 0], [0, 0, 1, 1, 2, 2, 0, 0, 1, 1, 2, 2, 0, 0, 1, 1],
    [0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 0, 0, 0, 0, 1, 1], [0, 1, 0, 1, 0, 1, 0, 1, 2, 2, 2, 2, 2, 2, 2, 2], [0, 0, 0, 0, 0, 0, 0, 0, 2, 1, 2, 1, 2, 1, 2, 1], [0, 0, 2, 2, 1, 1, 2, 2, 0, 0, 2, 2, 1, 1, 2, 2],
    [0, 0, 2, 2, 0, 0, 1, 1, 0, 0, 2, 2, 0, 0, 1, 1], [0, 2, 2, 0, 1, 2, 2, 1, 0, 2, 2, 0, 1, 2, 2, 1], [0, 1, 0, 1, 2, 2, 2, 2, 2, 2, 2, 2, 0, 1, 0, 1], [0, 0, 0, 0, 2, 1, 2, 1, 2, 1, 2, 1, 2, 1, 2, 1],
    [0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 2, 2, 2, 2], [0, 2, 2, 2, 0, 1, 1, 1, 0, 2, 2, 2, 0, 1, 1, 1], [0, 0, 0, 2, 1, 1, 1, 2, 0, 0, 0, 2, 1, 1, 1, 2], [0, 0, 0, 0, 2, 1, 1, 2, 2, 1, 1, 2, 2, 1, 1, 2],
    [0, 2, 2, 2, 0, 1, 1, 1, 0, 1, 1, 1, 0, 2, 2, 2], [0, 0, 0, 2, 1, 1, 1, 2, 1, 1, 1, 2, 0, 0, 0, 2], [0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 2, 2, 2, 2], [0, 0, 0, 0, 0, 0, 0, 0, 2, 1, 1, 2, 2, 1, 1, 2],
    [0, 1, 1, 0, 0, 1, 1, 0, 2, 2, 2, 2, 2, 2, 2, 2], [0, 0, 2, 2, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 2, 2], [0, 0, 2, 2, 1, 1, 2, 2, 1, 1, 2, 2, 0, 0, 2, 2], [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 1, 1, 2],
    [0, 0, 0, 2, 0, 0, 0, 1, 0, 0, 0, 2, 0, 0, 0, 1], [0, 2, 2, 2, 1, 2, 2, 2, 0, 2, 2, 2, 1, 2, 2, 2], [0, 1, 0, 1, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2], [0, 1, 1, 1, 2, 0, 1, 1, 2, 2, 0, 1, 2, 2, 2, 0],
];

const A2: [u8; 64] = [
    15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 2, 8, 2, 2, 8, 8, 15, 2, 8, 2, 2, 8, 8, 2, 2, 15, 15, 6, 8, 2, 8, 15, 15, 2, 8, 2, 2, 2, 15, 15, 6, 6, 2, 6, 8, 15, 15, 2, 2, 15,
    15, 15, 15, 15, 2, 2, 15,
];
const A3A: [u8; 64] = [
    3, 3, 15, 15, 8, 3, 15, 15, 8, 8, 6, 6, 6, 5, 3, 3, 3, 3, 8, 15, 3, 3, 6, 10, 5, 8, 8, 6, 8, 5, 15, 15, 8, 15, 3, 5, 6, 10, 8, 15, 15, 3, 15, 5, 15, 15, 15, 15, 3, 15, 5, 5, 5, 8, 5, 10, 5, 10,
    8, 13, 15, 12, 3, 3,
];
const A3B: [u8; 64] = [
    15, 8, 8, 3, 15, 15, 3, 8, 15, 15, 15, 15, 15, 15, 15, 8, 15, 8, 15, 3, 15, 8, 15, 8, 3, 15, 6, 10, 15, 15, 10, 8, 15, 3, 15, 10, 10, 8, 9, 10, 6, 15, 8, 15, 3, 6, 6, 8, 15, 3, 15, 15, 15, 15,
    15, 15, 15, 15, 15, 15, 3, 15, 15, 8,
];

const W2: [u32; 4] = [0, 21, 43, 64];
const W3: [u32; 8] = [0, 9, 18, 27, 37, 46, 55, 64];
const W4: [u32; 16] = [0, 4, 9, 13, 17, 21, 26, 30, 34, 38, 43, 47, 51, 55, 60, 64];

struct Bits<'a> {
    b: &'a [u8],
    p: usize,
}
impl Bits<'_> {
    fn get(&mut self, n: usize) -> u32 {
        let mut v = 0;
        for i in 0..n {
            let bit = (self.b[(self.p + i) / 8] >> ((self.p + i) % 8)) & 1;
            v |= (bit as u32) << i;
        }
        self.p += n;
        v
    }
}

fn subset_of(ns: usize, part: usize, i: usize) -> usize {
    match ns {
        1 => 0,
        2 => ((P2[part] >> i) & 1) as usize,
        _ => P3[part][i] as usize,
    }
}

fn is_anchor(ns: usize, part: usize, i: usize) -> bool {
    i == 0
        || match ns {
            2 => i == A2[part] as usize,
            3 => i == A3A[part] as usize || i == A3B[part] as usize,
            _ => false,
        }
}

fn interp(e0: u32, e1: u32, w: u32) -> u8 {
    (((64 - w) * e0 + w * e1 + 32) >> 6) as u8
}

fn expand(v: u32, bits: u32) -> u32 {
    let v = v << (8 - bits);
    v | (v >> bits)
}

/// Decode one BC7 block to 16 RGBA pixels (row-major in the 4x4 block).
pub fn bc7(b: &[u8]) -> [[u8; 4]; 16] {
    let mode = (0..8).find(|&m| (b[0] >> m) & 1 == 1);
    let Some(mode) = mode else { return [[0; 4]; 16] };
    // (subsets, partition bits, rotation bits, index-selection bits, colour bits, alpha bits, endpoint
    // p-bits, shared p-bits, index bits, second index bits)
    const MODES: [(usize, usize, usize, usize, u32, u32, usize, usize, usize, usize); 8] = [
        (3, 4, 0, 0, 4, 0, 1, 0, 3, 0),
        (2, 6, 0, 0, 6, 0, 0, 1, 3, 0),
        (3, 6, 0, 0, 5, 0, 0, 0, 2, 0),
        (2, 6, 0, 0, 7, 0, 1, 0, 2, 0),
        (1, 0, 2, 1, 5, 6, 0, 0, 2, 3),
        (1, 0, 2, 0, 7, 8, 0, 0, 2, 2),
        (1, 0, 0, 0, 7, 7, 1, 0, 4, 0),
        (2, 6, 0, 0, 5, 5, 1, 0, 2, 0),
    ];
    let (ns, pb, rb, isb, cb, ab, epb, spb, ib, ib2) = MODES[mode];
    let mut r = Bits { b, p: mode + 1 };
    let part = r.get(pb) as usize;
    let rot = r.get(rb);
    let idxsel = r.get(isb);
    let mut ep = [[[0u32; 4]; 2]; 3]; // subset, endpoint, channel
    for ch in 0..3 {
        for s in 0..ns {
            for e in 0..2 {
                ep[s][e][ch] = r.get(cb as usize);
            }
        }
    }
    if ab > 0 {
        for s in 0..ns {
            for e in 0..2 {
                ep[s][e][3] = r.get(ab as usize);
            }
        }
    }
    let (mut cbits, mut abits) = (cb, ab);
    if epb > 0 {
        for s in 0..ns {
            for e in 0..2 {
                let p = r.get(1);
                for ch in 0..4 {
                    ep[s][e][ch] = (ep[s][e][ch] << 1) | p;
                }
            }
        }
        cbits += 1;
        if ab > 0 {
            abits += 1;
        }
    } else if spb > 0 {
        for s in 0..ns {
            let p = r.get(1);
            for e in 0..2 {
                for ch in 0..4 {
                    ep[s][e][ch] = (ep[s][e][ch] << 1) | p;
                }
            }
        }
        cbits += 1;
        if ab > 0 {
            abits += 1;
        }
    }
    for s in 0..ns {
        for e in 0..2 {
            for ch in 0..3 {
                ep[s][e][ch] = expand(ep[s][e][ch], cbits);
            }
            ep[s][e][3] = if ab > 0 { expand(ep[s][e][3], abits) } else { 255 };
        }
    }
    let mut idx = [0u32; 16];
    for i in 0..16 {
        let anchor = is_anchor(ns, part, i);
        idx[i] = r.get(ib - anchor as usize);
    }
    let mut idx2 = [0u32; 16];
    if ib2 > 0 {
        for i in 0..16 {
            idx2[i] = r.get(ib2 - (i == 0) as usize);
        }
    }
    let wt = |bits: usize, k: u32| match bits {
        2 => W2[k as usize],
        3 => W3[k as usize],
        _ => W4[k as usize],
    };
    let mut out = [[0u8; 4]; 16];
    for i in 0..16 {
        let s = subset_of(ns, part, i);
        let (e0, e1) = (ep[s][0], ep[s][1]);
        let (cw, aw) = if ib2 == 0 {
            (wt(ib, idx[i]), wt(ib, idx[i]))
        } else if idxsel == 0 {
            (wt(ib, idx[i]), wt(ib2, idx2[i]))
        } else {
            (wt(ib2, idx2[i]), wt(ib, idx[i]))
        };
        let mut px = [interp(e0[0], e1[0], cw), interp(e0[1], e1[1], cw), interp(e0[2], e1[2], cw), interp(e0[3], e1[3], aw)];
        match rot {
            1 => px.swap(0, 3),
            2 => px.swap(1, 3),
            3 => px.swap(2, 3),
            _ => {}
        }
        out[i] = px;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partition_tables_are_consistent() {
        for p in 0..64 {
            assert_eq!(P2[p] & 1, 0, "pixel 0 is subset 0 in 2-subset partition {p}");
            assert_eq!((P2[p] >> A2[p]) & 1, 1, "2-subset anchor {p}");
            assert_eq!(P3[p][0], 0, "3-subset partition {p} pixel 0");
            assert_eq!(P3[p][A3A[p] as usize], 1, "3-subset anchor a {p}");
            assert_eq!(P3[p][A3B[p] as usize], 2, "3-subset anchor b {p}");
        }
    }

    #[test]
    fn bc7_mode6_solid_block() {
        // mode 6: endpoints all 127 + pbit 1 = 255 -> white, alpha 255
        let mut b = [0u8; 16];
        let mut bits: Vec<u8> = vec![0, 0, 0, 0, 0, 0, 1];
        for _ in 0..8 {
            bits.extend([1u8; 7]);
        }
        bits.push(1);
        bits.push(1);
        for (i, v) in bits.iter().enumerate() {
            b[i / 8] |= v << (i % 8);
        }
        let px = bc7(&b);
        assert_eq!(px[0], [255, 255, 255, 255]);
        assert_eq!(px[15], [255, 255, 255, 255]);
    }

    #[test]
    fn bc1_endpoints() {
        let b = [0x00, 0xf8, 0x1f, 0x00, 0, 0, 0, 0x55];
        let px = bc1(&b);
        assert_eq!(px[0], [255, 0, 0, 255]);
        assert_eq!(px[15], [0, 0, 255, 255]);
    }
}
