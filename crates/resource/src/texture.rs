//! The header of a compiled texture (`*.vtex_c`): its `DATA` block is a fixed binary struct, not KV3.
//!
//! version u16 | flags u16 | reflectivity f32 x4 | width u16 | height u16 | depth u16 | format u8 |
//! mip count u8 | picmip0 res u32 | extra-data offset u32 | extra-data count u32

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TexInfo {
    pub version: u16,
    pub flags: u16,
    pub width: u16,
    pub height: u16,
    pub depth: u16,
    pub format: u8,
    pub mips: u8,
}

impl TexInfo {
    pub fn format_name(&self) -> &'static str {
        format_name(self.format)
    }
    pub fn is_block_compressed(&self) -> bool {
        matches!(self.format, 1 | 2 | 17 | 18 | 19 | 20 | 21 | 23 | 24 | 25 | 26 | 27)
    }
    pub fn pow2(&self) -> bool {
        self.width.is_power_of_two() && self.height.is_power_of_two()
    }
}

/// Source 2's `VTexFormat` numbering (checked on build 6864: a 512x512 single-mip light cookie of
/// 1,050,516 bytes is 28 = BGRA8888, four bytes a pixel; Lash's 4096x2048 colour map is 20 = BC7).
pub const FORMATS: &[&str] = &[
    "UNKNOWN",
    "DXT1",
    "DXT5",
    "I8",
    "RGBA8888",
    "R16",
    "RG1616",
    "RGBA16161616",
    "R16F",
    "RG1616F",
    "RGBA16161616F",
    "R32F",
    "RG3232F",
    "RGB323232F",
    "RGBA32323232F",
    "JPEG_RGBA8888",
    "PNG_RGBA8888",
    "JPEG_DXT5",
    "PNG_DXT5",
    "BC6H",
    "BC7",
    "ATI2N",
    "IA88",
    "ETC2",
    "ETC2_EAC",
    "R11_EAC",
    "RG11_EAC",
    "ATI1N",
    "BGRA8888",
];

pub const DXT1: u8 = 1;
pub const RGBA8888: u8 = 4;
pub const BGRA8888: u8 = 28;

pub fn format_name(f: u8) -> &'static str {
    FORMATS.get(f as usize).copied().unwrap_or("unknown")
}

/// Read the texture header from a compiled `.vtex_c` file.
pub fn info(data: &[u8]) -> Option<TexInfo> {
    let r = crate::parse(data).ok()?;
    let b = r.block(b"DATA")?;
    let d = crate::block_bytes(data, b);
    if d.len() < 28 || crate::kv3::is_kv3(d) {
        return None;
    }
    let u16_at = |p: usize| u16::from_le_bytes([d[p], d[p + 1]]);
    Some(TexInfo {
        version: u16_at(0),
        flags: u16_at(2),
        width: u16_at(20),
        height: u16_at(22),
        depth: u16_at(24),
        format: d[26],
        mips: d[27],
    })
}

/// A texture header for fixtures.
pub fn synth_header(width: u16, height: u16, format: u8) -> Vec<u8> {
    let mut d = Vec::new();
    d.extend_from_slice(&1u16.to_le_bytes());
    d.extend_from_slice(&0u16.to_le_bytes());
    for _ in 0..4 {
        d.extend_from_slice(&0.5f32.to_le_bytes());
    }
    d.extend_from_slice(&width.to_le_bytes());
    d.extend_from_slice(&height.to_le_bytes());
    d.extend_from_slice(&1u16.to_le_bytes());
    d.push(format);
    d.push(1);
    d.extend_from_slice(&0u32.to_le_bytes());
    d.extend_from_slice(&0u32.to_le_bytes());
    d.extend_from_slice(&0u32.to_le_bytes());
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_round_trip() {
        let b = crate::synth_with(&[], &[(*b"DATA", synth_header(74, 74, 1))]);
        let t = info(&b).unwrap();
        assert_eq!((t.width, t.height, t.format_name()), (74, 74, "DXT1"));
        assert!(!t.pow2());
        assert!(t.is_block_compressed());
    }
}
