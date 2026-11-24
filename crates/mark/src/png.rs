//! PNG read and write (8-bit and lower-depth greyscale, palette, RGB, with alpha), on our own zlib.
//!
//! Reading decodes every filter type and colour type to RGBA8; Adam7-interlaced and 16-bit files are refused
//! (no colour source here uses them). Writing keeps the source's channel layout where the pixels allow it
//! (greyscale stays greyscale, RGB stays RGB; palette images become RGB or RGBA) and carries the ancillary
//! chunks (sRGB, gAMA, iCCP, pHYs, text) over verbatim, so a re-saved texture compiles the same way.

use crate::zlib;

const SIG: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

#[derive(Debug, Clone)]
pub struct Meta {
    pub color_type: u8,
    pub bit_depth: u8,
    /// Ancillary chunks to write back before IDAT (type, data); tRNS and PLTE are not kept.
    pub ancillary: Vec<([u8; 4], Vec<u8>)>,
}

pub fn crc32(data: &[u8]) -> u32 {
    static TABLE: std::sync::OnceLock<[u32; 256]> = std::sync::OnceLock::new();
    let t = TABLE.get_or_init(|| {
        let mut t = [0u32; 256];
        for (i, e) in t.iter_mut().enumerate() {
            let mut c = i as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            }
            *e = c;
        }
        t
    });
    let mut c = 0xFFFF_FFFFu32;
    for &b in data {
        c = t[((c ^ b as u32) & 0xff) as usize] ^ (c >> 8);
    }
    c ^ 0xFFFF_FFFF
}

/// Decode a PNG to (width, height, RGBA8, metadata).
pub fn read(data: &[u8]) -> Result<(usize, usize, Vec<u8>, Meta), String> {
    if data.len() < 8 || data[..8] != SIG {
        return Err("not a PNG".into());
    }
    let mut p = 8;
    let (mut w, mut h, mut bd, mut ct, mut il) = (0usize, 0usize, 0u8, 0u8, 0u8);
    let mut idat = Vec::new();
    let mut plte: Vec<[u8; 3]> = Vec::new();
    let mut trns: Vec<u8> = Vec::new();
    let mut ancillary = Vec::new();
    let mut seen_ihdr = false;
    while p + 12 <= data.len() {
        let len = u32::from_be_bytes([data[p], data[p + 1], data[p + 2], data[p + 3]]) as usize;
        let ty: [u8; 4] = [data[p + 4], data[p + 5], data[p + 6], data[p + 7]];
        if p + 12 + len > data.len() {
            return Err("PNG chunk overruns the file".into());
        }
        let body = &data[p + 8..p + 8 + len];
        let crc = u32::from_be_bytes([data[p + 8 + len], data[p + 9 + len], data[p + 10 + len], data[p + 11 + len]]);
        if crc32(&data[p + 4..p + 8 + len]) != crc {
            return Err(format!("PNG chunk {} CRC mismatch", String::from_utf8_lossy(&ty)));
        }
        match &ty {
            b"IHDR" => {
                if len < 13 {
                    return Err("short IHDR".into());
                }
                w = u32::from_be_bytes([body[0], body[1], body[2], body[3]]) as usize;
                h = u32::from_be_bytes([body[4], body[5], body[6], body[7]]) as usize;
                bd = body[8];
                ct = body[9];
                il = body[12];
                seen_ihdr = true;
            }
            b"PLTE" => plte = body.chunks(3).filter(|c| c.len() == 3).map(|c| [c[0], c[1], c[2]]).collect(),
            b"tRNS" => trns = body.to_vec(),
            b"IDAT" => idat.extend_from_slice(body),
            b"IEND" => break,
            _ => {
                if ty[0] & 0x20 != 0 {
                    ancillary.push((ty, body.to_vec()));
                }
            }
        }
        p += 12 + len;
    }
    if !seen_ihdr || w == 0 || h == 0 {
        return Err("PNG without a valid IHDR".into());
    }
    if il != 0 {
        return Err("interlaced PNG (Adam7) is not supported".into());
    }
    if bd == 16 {
        return Err("16-bit PNG is not supported".into());
    }
    let channels = match ct {
        0 => 1,
        2 => 3,
        3 => 1,
        4 => 2,
        6 => 4,
        _ => return Err(format!("PNG colour type {ct}")),
    };
    if !matches!((ct, bd), (0, 1 | 2 | 4 | 8) | (3, 1 | 2 | 4 | 8) | (2 | 4 | 6, 8)) {
        return Err(format!("PNG colour type {ct} at bit depth {bd}"));
    }
    let raw = zlib::unzlib(&idat)?;
    let bpp = (channels * bd as usize).div_ceil(8);
    let stride = (w * channels * bd as usize).div_ceil(8);
    if raw.len() < h * (stride + 1) {
        return Err("PNG image data is short".into());
    }
    let mut img = vec![0u8; h * stride];
    for y in 0..h {
        let f = raw[y * (stride + 1)];
        let src = &raw[y * (stride + 1) + 1..(y + 1) * (stride + 1)];
        let (done, rest) = img.split_at_mut(y * stride);
        let prev: &[u8] = if y > 0 { &done[(y - 1) * stride..] } else { &[] };
        let cur = &mut rest[..stride];
        for x in 0..stride {
            let a = if x >= bpp { cur[x - bpp] as i32 } else { 0 };
            let b = if y > 0 { prev[x] as i32 } else { 0 };
            let c = if y > 0 && x >= bpp { prev[x - bpp] as i32 } else { 0 };
            let v = src[x] as i32;
            cur[x] = match f {
                0 => v,
                1 => v + a,
                2 => v + b,
                3 => v + (a + b) / 2,
                4 => v + paeth(a, b, c),
                _ => return Err(format!("PNG filter {f}")),
            } as u8;
        }
    }
    let mut out = vec![0u8; w * h * 4];
    let sample = |row: &[u8], i: usize| -> u8 {
        match bd {
            8 => row[i],
            _ => {
                let per = 8 / bd as usize;
                let byte = row[i / per];
                let shift = 8 - bd as usize * (i % per + 1);
                (byte >> shift) & ((1u8 << bd) - 1)
            }
        }
    };
    let scale = |v: u8| -> u8 {
        match bd {
            1 => v * 255,
            2 => v * 85,
            4 => v * 17,
            _ => v,
        }
    };
    for y in 0..h {
        let row = &img[y * stride..(y + 1) * stride];
        for x in 0..w {
            let o = &mut out[4 * (y * w + x)..4 * (y * w + x) + 4];
            match ct {
                0 => {
                    let g = sample(row, x);
                    let a = if trns.len() >= 2 && u16::from_be_bytes([trns[0], trns[1]]) == g as u16 { 0 } else { 255 };
                    let g = scale(g);
                    o.copy_from_slice(&[g, g, g, a]);
                }
                2 => o.copy_from_slice(&[row[3 * x], row[3 * x + 1], row[3 * x + 2], 255]),
                3 => {
                    let i = sample(row, x) as usize;
                    let c = plte.get(i).copied().unwrap_or([0, 0, 0]);
                    o.copy_from_slice(&[c[0], c[1], c[2], trns.get(i).copied().unwrap_or(255)]);
                }
                4 => o.copy_from_slice(&[row[2 * x], row[2 * x], row[2 * x], row[2 * x + 1]]),
                _ => o.copy_from_slice(&row[4 * x..4 * x + 4]),
            }
        }
    }
    Ok((w, h, out, Meta { color_type: ct, bit_depth: bd, ancillary }))
}

fn paeth(a: i32, b: i32, c: i32) -> i32 {
    let p = a + b - c;
    let (pa, pb, pc) = ((p - a).abs(), (p - b).abs(), (p - c).abs());
    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}

/// The output colour type for these pixels: the source's layout where it still fits.
pub fn fit_color_type(src: u8, rgba: &[u8]) -> u8 {
    let grey = rgba.chunks(4).all(|p| p[0] == p[1] && p[1] == p[2]);
    let opaque = rgba.chunks(4).all(|p| p[3] == 255);
    match (src, grey, opaque) {
        (0, true, true) => 0,
        (4, true, _) | (0, true, false) => 4,
        (2, _, true) | (3, _, true) | (0, false, true) => 2,
        (6, _, _) => 6,
        (_, _, true) => 2,
        _ => 6,
    }
}

/// Encode RGBA8 as an 8-bit PNG of colour type `ct` (0, 2, 4 or 6), filters chosen per row by the
/// minimum-sum-of-absolute-differences heuristic, with the given ancillary chunks.
pub fn write(w: usize, h: usize, rgba: &[u8], ct: u8, ancillary: &[([u8; 4], Vec<u8>)]) -> Vec<u8> {
    let ch = match ct {
        0 => 1,
        2 => 3,
        4 => 2,
        _ => 4,
    };
    let stride = w * ch;
    let mut px = vec![0u8; h * stride];
    for i in 0..w * h {
        let s = &rgba[4 * i..4 * i + 4];
        let d = &mut px[i * ch..(i + 1) * ch];
        match ct {
            0 => d[0] = s[0],
            2 => d.copy_from_slice(&s[..3]),
            4 => {
                d[0] = s[0];
                d[1] = s[3];
            }
            _ => d.copy_from_slice(s),
        }
    }
    let mut raw = Vec::with_capacity(h * (stride + 1));
    let mut cand = vec![0u8; stride];
    let mut best = vec![0u8; stride];
    for y in 0..h {
        let cur = &px[y * stride..(y + 1) * stride];
        let prev = if y > 0 { Some(&px[(y - 1) * stride..y * stride]) } else { None };
        let mut best_f = 0u8;
        let mut best_cost = u64::MAX;
        for f in 0..5u8 {
            for x in 0..stride {
                let a = if x >= ch { cur[x - ch] as i32 } else { 0 };
                let b = prev.map_or(0, |p| p[x] as i32);
                let c = if x >= ch { prev.map_or(0, |p| p[x - ch] as i32) } else { 0 };
                let v = cur[x] as i32;
                cand[x] = match f {
                    0 => v,
                    1 => v - a,
                    2 => v - b,
                    3 => v - (a + b) / 2,
                    _ => v - paeth(a, b, c),
                } as u8;
            }
            let cost: u64 = cand.iter().map(|&b| (b as i8).unsigned_abs() as u64).sum();
            if cost < best_cost {
                best_cost = cost;
                best_f = f;
                best.copy_from_slice(&cand);
            }
        }
        raw.push(best_f);
        raw.extend_from_slice(&best);
    }
    let mut out = SIG.to_vec();
    let mut chunk = |ty: &[u8; 4], body: &[u8]| {
        out.extend_from_slice(&(body.len() as u32).to_be_bytes());
        let mut c = ty.to_vec();
        c.extend_from_slice(body);
        out.extend_from_slice(&c);
        out.extend_from_slice(&crc32(&c).to_be_bytes());
    };
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&(w as u32).to_be_bytes());
    ihdr.extend_from_slice(&(h as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, ct, 0, 0, 0]);
    chunk(b"IHDR", &ihdr);
    for (ty, body) in ancillary {
        if ty != b"tRNS" && ty != b"PLTE" {
            chunk(ty, body);
        }
    }
    chunk(b"IDAT", &zlib::zlib(&raw));
    chunk(b"IEND", &[]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_image(w: usize, h: usize) -> Vec<u8> {
        let mut v = Vec::with_capacity(w * h * 4);
        for y in 0..h {
            for x in 0..w {
                v.extend_from_slice(&[(x * 7 + y) as u8, (y * 3) as u8, ((x ^ y) * 5) as u8, if (x + y) % 11 == 0 { 0 } else { 255 }]);
            }
        }
        v
    }

    #[test]
    fn round_trip_every_layout() {
        let rgba = test_image(37, 23);
        for ct in [2u8, 6] {
            let png = write(37, 23, &rgba, ct, &[(*b"sRGB", vec![0])]);
            let (w, h, back, meta) = read(&png).unwrap();
            assert_eq!((w, h, meta.color_type), (37, 23, ct));
            assert_eq!(meta.ancillary, vec![(*b"sRGB", vec![0])]);
            for i in 0..w * h {
                assert_eq!(back[4 * i..4 * i + 3], rgba[4 * i..4 * i + 3]);
                assert_eq!(back[4 * i + 3], if ct == 6 { rgba[4 * i + 3] } else { 255 });
            }
        }
        let grey: Vec<u8> = rgba.chunks(4).flat_map(|p| [p[0], p[0], p[0], p[3]]).collect();
        let (_, _, back, _) = read(&write(37, 23, &grey, 4, &[])).unwrap();
        assert_eq!(back, grey);
        let (_, _, back, _) = read(&write(37, 23, &grey, 0, &[])).unwrap();
        assert!(back.chunks(4).zip(grey.chunks(4)).all(|(a, b)| a[0] == b[0] && a[3] == 255));
    }

    #[test]
    fn reads_a_reference_png() {
        // 2x2 RGB written by Python's zlib + hand-built chunks (the layout toolkit/ingame/capture.py writes):
        // pixels red, green / blue, white, filter 0 rows
        let raw = [0u8, 255, 0, 0, 0, 255, 0, 0, 0, 0, 255, 255, 255, 255];
        let mut png = SIG.to_vec();
        let mut chunk = |ty: &[u8; 4], body: &[u8]| {
            png.extend_from_slice(&(body.len() as u32).to_be_bytes());
            let mut c = ty.to_vec();
            c.extend_from_slice(body);
            png.extend_from_slice(&c);
            png.extend_from_slice(&crc32(&c).to_be_bytes());
        };
        chunk(b"IHDR", &[0, 0, 0, 2, 0, 0, 0, 2, 8, 2, 0, 0, 0]);
        chunk(b"IDAT", &zlib::zlib(&raw));
        chunk(b"IEND", &[]);
        let (w, h, px, _) = read(&png).unwrap();
        assert_eq!((w, h), (2, 2));
        assert_eq!(px, vec![255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255]);
        assert_eq!(crc32(b"IEND"), 0xAE42_6082);
    }
}
