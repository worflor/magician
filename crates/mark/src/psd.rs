//! PSD read and write for the flattened composite: 8-bit RGB with an optional alpha channel, raw or
//! PackBits (RLE). The colour-mode, image-resource and layer sections are carried over byte for byte;
//! only the composite image data is re-encoded, with the source's compression.
//!
//! A PSD with layers is refused for writing: a reader that composites the layers itself would never see
//! a mark put into the merged image. The UI pass's PSDs (`build_ui.ps1`) are single-composite files.

#[derive(Debug, Clone)]
pub struct Meta {
    /// Everything before the image data section: header, colour mode data, image resources, layers.
    pub head: Vec<u8>,
    pub channels: usize,
    pub compression: u16,
    pub has_layers: bool,
}

fn be16(d: &[u8], p: usize) -> Result<u16, String> {
    d.get(p..p + 2).map(|b| u16::from_be_bytes([b[0], b[1]])).ok_or_else(|| "short PSD".to_string())
}
fn be32(d: &[u8], p: usize) -> Result<u32, String> {
    d.get(p..p + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]])).ok_or_else(|| "short PSD".to_string())
}

fn unpack_bits(src: &[u8], want: usize) -> Result<Vec<u8>, String> {
    // two bytes of PackBits give at most 128 bytes
    let mut out = Vec::with_capacity(want.min(src.len().saturating_mul(64)));
    let mut i = 0;
    while out.len() < want {
        let n = *src.get(i).ok_or("PSD RLE row is short")? as i8;
        i += 1;
        if n >= 0 {
            let k = n as usize + 1;
            out.extend_from_slice(src.get(i..i + k).ok_or("PSD RLE row is short")?);
            i += k;
        } else if n != -128 {
            let b = *src.get(i).ok_or("PSD RLE row is short")?;
            i += 1;
            out.extend(std::iter::repeat_n(b, (1 - n as isize) as usize));
        }
    }
    out.truncate(want);
    Ok(out)
}

fn pack_bits(src: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < src.len() {
        let mut run = 1;
        while i + run < src.len() && run < 128 && src[i + run] == src[i] {
            run += 1;
        }
        if run >= 2 {
            out.push((1i16 - run as i16) as i8 as u8);
            out.push(src[i]);
            i += run;
        } else {
            let start = i;
            let mut lit = 0;
            while i < src.len() && lit < 128 {
                if i + 1 < src.len() && src[i + 1] == src[i] {
                    break;
                }
                i += 1;
                lit += 1;
            }
            if lit == 0 {
                continue;
            }
            out.push((lit - 1) as u8);
            out.extend_from_slice(&src[start..start + lit]);
        }
    }
    out
}

pub fn read(d: &[u8]) -> Result<(usize, usize, Vec<u8>, Meta), String> {
    if d.len() < 26 || &d[..4] != b"8BPS" {
        return Err("not a PSD".into());
    }
    if be16(d, 4)? != 1 {
        return Err("PSB (large document) is not supported".into());
    }
    let channels = be16(d, 12)? as usize;
    let h = be32(d, 14)? as usize;
    let w = be32(d, 18)? as usize;
    let depth = be16(d, 22)?;
    let mode = be16(d, 24)?;
    if depth != 8 || mode != 3 || !(3..=8).contains(&channels) {
        return Err(format!("PSD depth {depth}, colour mode {mode}, {channels} channels: only 8-bit RGB is supported"));
    }
    let mut p = 26;
    p += 4 + be32(d, p)? as usize; // colour mode data
    p += 4 + be32(d, p)? as usize; // image resources
    let layer_len = be32(d, p)? as usize;
    // a layer section whose layer count is 0 (or absent) carries no layers
    let has_layers = layer_len >= 6 && {
        let info_len = be32(d, p + 4)?;
        info_len >= 2 && be16(d, p + 8)? != 0
    };
    p += 4 + layer_len;
    let head = d.get(..p).ok_or("short PSD")?.to_vec();
    let compression = be16(d, p)?;
    p += 2;
    // the size is the header's word: a plane can't be larger than the file could unpack to
    let plane = w.checked_mul(h).filter(|n| *n <= d.len().saturating_mul(128)).ok_or_else(|| format!("PSD of {w} x {h} is larger than its {} bytes could hold", d.len()))?;
    let mut planes: Vec<Vec<u8>> = Vec::with_capacity(channels);
    match compression {
        0 => {
            for c in 0..channels {
                planes.push(d.get(p + c * plane..p + (c + 1) * plane).ok_or("PSD image data is short")?.to_vec());
            }
        }
        1 => {
            let counts: Vec<usize> = (0..channels * h).map(|i| be16(d, p + 2 * i).map(|v| v as usize)).collect::<Result<_, _>>()?;
            let mut q = p + 2 * channels * h;
            for c in 0..channels {
                let mut pl = Vec::with_capacity(plane);
                for y in 0..h {
                    let n = counts[c * h + y];
                    pl.extend(unpack_bits(d.get(q..q + n).ok_or("PSD RLE data is short")?, w)?);
                    q += n;
                }
                planes.push(pl);
            }
        }
        c => return Err(format!("PSD compression {c} (zip) is not supported")),
    }
    let mut rgba = vec![255u8; plane * 4];
    for i in 0..plane {
        for c in 0..channels.min(4) {
            rgba[4 * i + c] = planes[c][i];
        }
    }
    Ok((w, h, rgba, Meta { head, channels, compression, has_layers }))
}

/// Re-encode the composite. Channels past the fourth (spot or extra alpha) are not kept by `read`, so a
/// file with more than four is refused rather than truncated.
pub fn write(w: usize, h: usize, rgba: &[u8], m: &Meta) -> Result<Vec<u8>, String> {
    if m.channels > 4 {
        return Err(format!("PSD with {} channels: extra channels would be lost", m.channels));
    }
    let mut out = m.head.clone();
    out.extend_from_slice(&m.compression.to_be_bytes());
    let plane = |c: usize| -> Vec<u8> { (0..w * h).map(|i| rgba[4 * i + c]).collect() };
    if m.compression == 0 {
        for c in 0..m.channels {
            out.extend(plane(c));
        }
    } else {
        let mut rows: Vec<Vec<u8>> = Vec::with_capacity(m.channels * h);
        for c in 0..m.channels {
            let pl = plane(c);
            for y in 0..h {
                rows.push(pack_bits(&pl[y * w..(y + 1) * w]));
            }
        }
        for r in &rows {
            if r.len() > u16::MAX as usize {
                return Err("PSD RLE row exceeds 65535 bytes".into());
            }
            out.extend_from_slice(&(r.len() as u16).to_be_bytes());
        }
        for r in rows {
            out.extend(r);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn synth(w: usize, h: usize, rgba: &[u8], compression: u16) -> Vec<u8> {
        let mut head = b"8BPS".to_vec();
        head.extend_from_slice(&1u16.to_be_bytes());
        head.extend_from_slice(&[0; 6]);
        head.extend_from_slice(&4u16.to_be_bytes());
        head.extend_from_slice(&(h as u32).to_be_bytes());
        head.extend_from_slice(&(w as u32).to_be_bytes());
        head.extend_from_slice(&8u16.to_be_bytes());
        head.extend_from_slice(&3u16.to_be_bytes());
        head.extend_from_slice(&[0; 12]);
        write(w, h, rgba, &Meta { head, channels: 4, compression, has_layers: false }).unwrap()
    }

    #[test]
    fn packbits_round_trip() {
        let rows: [&[u8]; 5] = [b"", b"a", b"aaaaaaaaaaaaaaaaaaaaab", b"abcdefg", &[7u8; 300]];
        for r in rows {
            assert_eq!(unpack_bits(&pack_bits(r), r.len()).unwrap(), r);
        }
        // Apple's PackBits example
        let tn = [0xFEu8, 0xAA, 0x02, 0x80, 0x00, 0x2A, 0xFD, 0xAA, 0x03, 0x80, 0x00, 0x2A, 0x22, 0xF7, 0xAA];
        let want = [0xAAu8, 0xAA, 0xAA, 0x80, 0x00, 0x2A, 0xAA, 0xAA, 0xAA, 0xAA, 0x80, 0x00, 0x2A, 0x22, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA];
        assert_eq!(unpack_bits(&tn, want.len()).unwrap(), want);
    }

    #[test]
    fn round_trip_raw_and_rle() {
        let (w, h) = (23, 9);
        let rgba: Vec<u8> = (0..w * h).flat_map(|i| [(i % 7) as u8 * 30, 3, (i / 9) as u8, if i % 5 == 0 { 0 } else { 255 }]).collect();
        for comp in [0u16, 1] {
            let f = synth(w, h, &rgba, comp);
            let (w2, h2, back, m) = read(&f).unwrap();
            assert_eq!((w2, h2, m.compression, m.has_layers), (w, h, comp, false));
            assert_eq!(back, rgba);
            assert_eq!(write(w, h, &back, &m).unwrap(), f, "byte-identical re-save");
        }
    }
}
