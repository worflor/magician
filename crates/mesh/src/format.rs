//! Vertex attribute formats (`m_Format`, DXGI numbering) and Source 2's packed normal frames.

/// Decode one attribute value at `b` (its bytes) into up to four floats. Integer formats come back
/// as their integer values; normalised formats as -1..1 or 0..1. `None` for a format we don't read.
pub fn decode(format: u32, b: &[u8]) -> Option<[f32; 4]> {
    let f32le = |i: usize| f32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
    let u16le = |i: usize| u16::from_le_bytes([b[i], b[i + 1]]);
    let i16le = |i: usize| i16::from_le_bytes([b[i], b[i + 1]]);
    let need = size_of(format)?;
    if b.len() < need {
        return None;
    }
    Some(match format {
        2 => [f32le(0), f32le(4), f32le(8), f32le(12)],
        6 => [f32le(0), f32le(4), f32le(8), 0.0],
        16 => [f32le(0), f32le(4), 0.0, 0.0],
        41 => [f32le(0), 0.0, 0.0, 0.0],
        10 => [half(u16le(0)), half(u16le(2)), half(u16le(4)), half(u16le(6))],
        11 => [u16le(0) as f32 / 65535.0, u16le(2) as f32 / 65535.0, u16le(4) as f32 / 65535.0, u16le(6) as f32 / 65535.0],
        12 => [u16le(0) as f32, u16le(2) as f32, u16le(4) as f32, u16le(6) as f32],
        13 => [snorm16(i16le(0)), snorm16(i16le(2)), snorm16(i16le(4)), snorm16(i16le(6))],
        14 => [i16le(0) as f32, i16le(2) as f32, i16le(4) as f32, i16le(6) as f32],
        34 => [half(u16le(0)), half(u16le(2)), 0.0, 0.0],
        35 => [u16le(0) as f32 / 65535.0, u16le(2) as f32 / 65535.0, 0.0, 0.0],
        36 => [u16le(0) as f32, u16le(2) as f32, 0.0, 0.0],
        37 => [snorm16(i16le(0)), snorm16(i16le(2)), 0.0, 0.0],
        38 => [i16le(0) as f32, i16le(2) as f32, 0.0, 0.0],
        28 => [b[0] as f32 / 255.0, b[1] as f32 / 255.0, b[2] as f32 / 255.0, b[3] as f32 / 255.0],
        87 => [b[2] as f32 / 255.0, b[1] as f32 / 255.0, b[0] as f32 / 255.0, b[3] as f32 / 255.0],
        30 => [b[0] as f32, b[1] as f32, b[2] as f32, b[3] as f32],
        31 => [snorm8(b[0] as i8), snorm8(b[1] as i8), snorm8(b[2] as i8), snorm8(b[3] as i8)],
        32 => [b[0] as i8 as f32, b[1] as i8 as f32, b[2] as i8 as f32, b[3] as i8 as f32],
        42 => {
            let v = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
            [f32::from_bits(v), 0.0, 0.0, 0.0]
        }
        24 => {
            let v = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
            [(v & 1023) as f32 / 1023.0, ((v >> 10) & 1023) as f32 / 1023.0, ((v >> 20) & 1023) as f32 / 1023.0, (v >> 30) as f32 / 3.0]
        }
        _ => return None,
    })
}

/// A vertex's blend indices from the bytes at its attribute, and how many the format holds. Skinning streams do
/// not follow their format's name once a mesh carries eight influences: `R16G16B16A16_UINT` is eight one-byte
/// indices and `R32G32B32A32_SINT` eight 16-bit ones (Abrams, 499 bones), while `R8G8B8A8_UINT` and
/// `R16G16B16A16_SINT` are four and `R16G16_SINT` two. Checked against Source 2 Viewer's reader.
pub fn blend_indices(format: u32, b: &[u8]) -> Option<([u16; 8], usize)> {
    let mut out = [0u16; 8];
    let wide = |b: &[u8], n: usize, out: &mut [u16; 8]| -> Option<()> {
        let b = b.get(..2 * n)?;
        for k in 0..n {
            out[k] = u16::from_le_bytes([b[2 * k], b[2 * k + 1]]);
        }
        Some(())
    };
    let narrow = |b: &[u8], n: usize, out: &mut [u16; 8]| -> Option<()> {
        let b = b.get(..n)?;
        for k in 0..n {
            out[k] = b[k] as u16;
        }
        Some(())
    };
    let n = match format {
        12 => narrow(b, 8, &mut out).map(|_| 8)?,
        30 => narrow(b, 4, &mut out).map(|_| 4)?,
        4 => wide(b, 8, &mut out).map(|_| 8)?,
        14 => wide(b, 4, &mut out).map(|_| 4)?,
        38 | 36 => wide(b, 2, &mut out).map(|_| 2)?,
        _ => {
            let v = decode(format, b.get(..size_of(format)?)?)?;
            for k in 0..4 {
                out[k] = v[k] as u16;
            }
            4
        }
    };
    Some((out, n))
}

/// A vertex's blend weights, beside `blend_indices`: `R16G16B16A16_UNORM` is eight one-byte weights,
/// `R8G8B8A8_UNORM` four, `R16G16_UNORM` two 16-bit ones.
pub fn blend_weights(format: u32, b: &[u8]) -> Option<[f32; 8]> {
    let mut out = [0f32; 8];
    match format {
        11 => {
            for (k, x) in b.get(..8)?.iter().enumerate() {
                out[k] = *x as f32 / 255.0;
            }
        }
        28 => {
            for (k, x) in b.get(..4)?.iter().enumerate() {
                out[k] = *x as f32 / 255.0;
            }
        }
        35 => {
            let b = b.get(..4)?;
            out[0] = u16::from_le_bytes([b[0], b[1]]) as f32 / 65535.0;
            out[1] = u16::from_le_bytes([b[2], b[3]]) as f32 / 65535.0;
        }
        _ => {
            let v = decode(format, b.get(..size_of(format)?)?)?;
            out[..4].copy_from_slice(&v);
        }
    }
    Some(out)
}

/// Bytes per value of a format, `None` when unknown.
pub fn size_of(format: u32) -> Option<usize> {
    Some(match format {
        2 | 4 => 16,
        6 => 12,
        16 => 8,
        10..=14 => 8,
        24 | 28 | 30 | 31 | 32 | 34 | 35 | 36 | 37 | 38 | 41 | 42 | 87 => 4,
        _ => return None,
    })
}

pub fn name(format: u32) -> &'static str {
    match format {
        2 => "R32G32B32A32_FLOAT",
        4 => "R32G32B32A32_SINT",
        6 => "R32G32B32_FLOAT",
        10 => "R16G16B16A16_FLOAT",
        11 => "R16G16B16A16_UNORM",
        12 => "R16G16B16A16_UINT",
        13 => "R16G16B16A16_SNORM",
        14 => "R16G16B16A16_SINT",
        16 => "R32G32_FLOAT",
        24 => "R10G10B10A2_UNORM",
        28 => "R8G8B8A8_UNORM",
        30 => "R8G8B8A8_UINT",
        31 => "R8G8B8A8_SNORM",
        32 => "R8G8B8A8_SINT",
        34 => "R16G16_FLOAT",
        35 => "R16G16_UNORM",
        36 => "R16G16_UINT",
        37 => "R16G16_SNORM",
        38 => "R16G16_SINT",
        41 => "R32_FLOAT",
        42 => "R32_UINT",
        87 => "B8G8R8A8_UNORM",
        _ => "unknown",
    }
}

fn snorm16(v: i16) -> f32 {
    (v as f32 / 32767.0).max(-1.0)
}
fn snorm8(v: i8) -> f32 {
    (v as f32 / 127.0).max(-1.0)
}

/// IEEE half to f32.
pub fn half(h: u16) -> f32 {
    let s = ((h >> 15) & 1) as u32;
    let e = ((h >> 10) & 31) as i32;
    let m = (h & 1023) as u32;
    let bits = if e == 0 {
        if m == 0 {
            s << 31
        } else {
            // subnormal: m * 2^-24, exact in f32
            let v = m as f32 * (1.0 / 16_777_216.0);
            return if s == 1 { -v } else { v };
        }
    } else if e == 31 {
        (s << 31) | (255 << 23) | (m << 13)
    } else {
        (s << 31) | (((e - 15 + 127) as u32) << 23) | (m << 13)
    };
    f32::from_bits(bits)
}

/// Source 2's 32-bit packed tangent frame (`CompressedTangentFrame`, `R32_UINT`): bit 0 the
/// bitangent sign, bits 1..11 the tangent's angle around the normal, bits 12..21 and 22..31 the
/// normal in octahedral form (x, y in 0..1023 mapped to -1..1; the lower hemisphere folded).
pub fn unpack_tangent_frame(v: u32) -> ([f32; 3], [f32; 4]) {
    let sign = v & 1;
    let tbits = (v >> 1) & 0x7ff;
    let xbits = (v >> 12) & 0x3ff;
    let ybits = (v >> 22) & 0x3ff;
    let x = xbits as f32 / 1023.0 * 2.0 - 1.0;
    let y = ybits as f32 / 1023.0 * 2.0 - 1.0;
    let z = 1.0 - x.abs() - y.abs();
    let neg = (-z).clamp(0.0, 1.0);
    let x = if x >= 0.0 { x - neg } else { x + neg };
    let y = if y >= 0.0 { y - neg } else { y + neg };
    let n = normalize([x, y, z]);
    // Tangent: an angle around the normal from a reference axis built from the normal itself.
    let angle = tbits as f32 / 2047.0 * std::f32::consts::TAU;
    let (t1, t2) = basis(n);
    let (s, c) = angle.sin_cos();
    let t = normalize([t1[0] * c + t2[0] * s, t1[1] * c + t2[1] * s, t1[2] * c + t2[2] * s]);
    (n, [t[0], t[1], t[2], if sign == 0 { 1.0 } else { -1.0 }])
}

/// Source 2's older 4-byte compressed normal + tangent (`R8G8B8A8_UNORM` "CompressedNormal"): two
/// bytes per vector in a custom octahedral-like encoding (VRF's `DecompressNormal`).
pub fn unpack_byte_normal(x: u8, y: u8) -> [f32; 3] {
    let mut x = x as f32 - 128.0;
    let mut y = y as f32 - 128.0;
    let zsb = if x < 0.0 { 1.0 } else { 0.0 };
    let tsb = if y < 0.0 { 1.0 } else { 0.0 };
    let zs = -(2.0 * zsb - 1.0);
    let ts = -(2.0 * tsb - 1.0);
    x = x * zs - zsb;
    y = y * ts - tsb;
    x -= 64.0;
    y -= 64.0;
    let xsb = if x < 0.0 { 1.0 } else { 0.0 };
    let ysb = if y < 0.0 { 1.0 } else { 0.0 };
    let xs = -(2.0 * xsb - 1.0);
    let ys = -(2.0 * ysb - 1.0);
    x = (x * xs - xsb) / 63.0;
    y = (y * ys - ysb) / 63.0;
    let z = 1.0 - x - y;
    let ool = 1.0 / (x * x + y * y + z * z).sqrt();
    [x * ool * xs, y * ool * ys, z * ool * zs]
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if l > 0.0 {
        [v[0] / l, v[1] / l, v[2] / l]
    } else {
        [0.0, 0.0, 1.0]
    }
}

/// Two unit vectors orthogonal to `n` (and each other), chosen continuously from `n`'s components.
fn basis(n: [f32; 3]) -> ([f32; 3], [f32; 3]) {
    // Duff et al. 2017 branchless ONB.
    let s = if n[2] >= 0.0 { 1.0 } else { -1.0 };
    let a = -1.0 / (s + n[2]);
    let b = n[0] * n[1] * a;
    ([1.0 + s * n[0] * n[0] * a, s * b, -s * n[0]], [b, s + n[1] * n[1] * a, -n[1]])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn halves() {
        assert_eq!(half(0x3c00), 1.0);
        assert_eq!(half(0xc000), -2.0);
        assert_eq!(half(0x3555), 0.33325195);
        assert_eq!(half(0x0001), 5.9604645e-8);
    }
    #[test]
    fn packed_normal_axes() {
        // x = y = 511.5 is the +z pole; pack x=1023,y=511 approximately +x.
        let pack = |x: u32, y: u32| (y << 22) | (x << 12);
        let (n, _) = unpack_tangent_frame(pack(1023, 512));
        assert!(n[0] > 0.99, "{n:?}");
        let (n, _) = unpack_tangent_frame(pack(0, 512));
        assert!(n[0] < -0.99, "{n:?}");
        let (n, _) = unpack_tangent_frame(pack(512, 512));
        assert!(n[2] > 0.99, "{n:?}");
        let (n, t) = unpack_tangent_frame(pack(700, 300) | (1000 << 1) | 1);
        let d = n[0] * t[0] + n[1] * t[1] + n[2] * t[2];
        assert!(d.abs() < 1e-5 && t[3] == -1.0);
    }
}
