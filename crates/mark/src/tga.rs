//! TGA read and write: true-colour images (types 2 and 10, 24 or 32 bits, either origin) and greyscale
//! (types 3 and 11). Writing keeps the type, depth and origin, so a re-saved texture compiles the same way.

#[derive(Debug, Clone)]
pub struct Meta {
    pub image_type: u8,
    pub bpp: u8,
    pub descriptor: u8,
    pub id: Vec<u8>,
}

pub fn read(d: &[u8]) -> Result<(usize, usize, Vec<u8>, Meta), String> {
    if d.len() < 18 {
        return Err("short TGA".into());
    }
    let idlen = d[0] as usize;
    let cmap = d[1];
    let ty = d[2];
    let w = u16::from_le_bytes([d[12], d[13]]) as usize;
    let h = u16::from_le_bytes([d[14], d[15]]) as usize;
    let bpp = d[16];
    let desc = d[17];
    if cmap != 0 || !matches!(ty, 2 | 3 | 10 | 11) {
        return Err(format!("TGA type {ty} (colour-mapped {cmap}) is not supported"));
    }
    let bytes = (bpp as usize).div_ceil(8);
    if !matches!((ty, bpp), (2 | 10, 24 | 32) | (3 | 11, 8)) {
        return Err(format!("TGA type {ty} at {bpp} bits is not supported"));
    }
    let mut p = 18 + idlen;
    let id = d.get(18..18 + idlen).ok_or("short TGA")?.to_vec();
    let n = w * h;
    // the size is the header's word: a raw image needs its pixels in the file, and an RLE packet of at least two
    // bytes gives at most 128 pixels
    let left = d.len().saturating_sub(p);
    if (ty == 2 || ty == 3) && n * bytes > left || n > left.saturating_mul(128) {
        return Err(format!("TGA of {w} x {h} is larger than its {left} bytes of data"));
    }
    let mut pix = Vec::with_capacity(n * bytes);
    if ty == 2 || ty == 3 {
        pix.extend_from_slice(d.get(p..p + n * bytes).ok_or("TGA pixel data is short")?);
    } else {
        while pix.len() < n * bytes {
            let hdr = *d.get(p).ok_or("TGA RLE data is short")?;
            p += 1;
            let count = (hdr & 0x7f) as usize + 1;
            if hdr & 0x80 != 0 {
                let px = d.get(p..p + bytes).ok_or("TGA RLE data is short")?;
                for _ in 0..count {
                    pix.extend_from_slice(px);
                }
                p += bytes;
            } else {
                pix.extend_from_slice(d.get(p..p + count * bytes).ok_or("TGA RLE data is short")?);
                p += count * bytes;
            }
        }
        pix.truncate(n * bytes);
    }
    let top = desc & 0x20 != 0;
    let mut out = vec![0u8; n * 4];
    for y in 0..h {
        let sy = if top { y } else { h - 1 - y };
        for x in 0..w {
            let s = &pix[(sy * w + x) * bytes..(sy * w + x + 1) * bytes];
            let o = &mut out[4 * (y * w + x)..4 * (y * w + x) + 4];
            match bytes {
                1 => o.copy_from_slice(&[s[0], s[0], s[0], 255]),
                3 => o.copy_from_slice(&[s[2], s[1], s[0], 255]),
                _ => o.copy_from_slice(&[s[2], s[1], s[0], s[3]]),
            }
        }
    }
    Ok((w, h, out, Meta { image_type: ty, bpp, descriptor: desc, id }))
}

pub fn write(w: usize, h: usize, rgba: &[u8], m: &Meta) -> Vec<u8> {
    let bytes = (m.bpp as usize).div_ceil(8);
    let top = m.descriptor & 0x20 != 0;
    let mut pix = Vec::with_capacity(w * h * bytes);
    for y in 0..h {
        let sy = if top { y } else { h - 1 - y };
        for x in 0..w {
            let s = &rgba[4 * (sy * w + x)..4 * (sy * w + x) + 4];
            match bytes {
                1 => pix.push(s[0]),
                3 => pix.extend_from_slice(&[s[2], s[1], s[0]]),
                _ => pix.extend_from_slice(&[s[2], s[1], s[0], s[3]]),
            }
        }
    }
    let mut out = vec![0u8; 18];
    out[0] = m.id.len() as u8;
    out[2] = m.image_type;
    out[12..14].copy_from_slice(&(w as u16).to_le_bytes());
    out[14..16].copy_from_slice(&(h as u16).to_le_bytes());
    out[16] = m.bpp;
    out[17] = m.descriptor;
    out.extend_from_slice(&m.id);
    if m.image_type == 2 || m.image_type == 3 {
        out.extend_from_slice(&pix);
    } else {
        // RLE packets never cross a scanline (the spec's recommendation)
        for row in pix.chunks(w * bytes) {
            let px: Vec<&[u8]> = row.chunks(bytes).collect();
            let mut i = 0;
            while i < px.len() {
                let mut run = 1;
                while i + run < px.len() && run < 128 && px[i + run] == px[i] {
                    run += 1;
                }
                if run > 1 {
                    out.push(0x80 | (run - 1) as u8);
                    out.extend_from_slice(px[i]);
                    i += run;
                } else {
                    let mut lit = 1;
                    while i + lit < px.len() && lit < 128 && (i + lit + 1 >= px.len() || px[i + lit] != px[i + lit + 1]) {
                        lit += 1;
                    }
                    out.push((lit - 1) as u8);
                    for p in &px[i..i + lit] {
                        out.extend_from_slice(p);
                    }
                    i += lit;
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_types() {
        let (w, h) = (19, 7);
        let rgba: Vec<u8> = (0..w * h).flat_map(|i| [(i * 3) as u8, (i / 5) as u8, 9, if i % 4 == 0 { 255 } else { 40 }]).collect();
        for (ty, bpp, desc) in [(2u8, 32u8, 0x28u8), (10, 32, 0x08), (2, 24, 0x20), (10, 24, 0)] {
            let m = Meta { image_type: ty, bpp, descriptor: desc, id: b"x".to_vec() };
            let t = write(w, h, &rgba, &m);
            let (w2, h2, back, m2) = read(&t).unwrap();
            assert_eq!((w2, h2, m2.image_type, m2.bpp), (w, h, ty, bpp));
            for i in 0..w * h {
                assert_eq!(back[4 * i..4 * i + 3], rgba[4 * i..4 * i + 3], "type {ty} bpp {bpp}");
                assert_eq!(back[4 * i + 3], if bpp == 32 { rgba[4 * i + 3] } else { 255 });
            }
        }
    }
}
