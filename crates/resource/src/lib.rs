//! Source 2 compiled resources (`*.vmdl_c`, `*.vmat_c`, `*.vpcf_c`, ...).
//!
//! Header: file_size u32 | header_version u16 (12) | type_version u16 | block_offset u32 | block_count u32.
//! The block table starts at byte 8 + block_offset. Each block entry is a 4-char tag, then an offset
//! (relative to the offset field itself) and a size.
//!
//! `RERL` (external reference list) is the dependency table: u32 offset (relative to the block start)
//! and u32 count, then `count` records of `id u64 | name_offset i64`, where name_offset is relative
//! to its own field. Names are source paths without the `_c` suffix, e.g.
//! `models/heroes_wip/lash/materials/lash_body.vmat`.

#![forbid(unsafe_code)]
// They read files other people made: an unwrap, an expect or a str sliced at a byte offset is a panic one byte away.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::string_slice)]

pub mod kv3;
pub mod lz4;
pub mod material;
pub mod zstd;
pub mod pixels;
pub mod texture;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub tag: [u8; 4],
    pub offset: usize,
    pub size: usize,
}
impl Block {
    pub fn tag_str(&self) -> String {
        String::from_utf8_lossy(&self.tag).into_owned()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtRef {
    pub id: u64,
    pub name: String,
}
impl ExtRef {
    /// The compiled path the engine will actually look up: lowercase, with the `_c` suffix.
    pub fn compiled_path(&self) -> String {
        let n = self.name.to_ascii_lowercase().replace('\\', "/");
        if n.ends_with("_c") { n } else { format!("{n}_c") }
    }
}

#[derive(Debug)]
pub struct Resource {
    pub file_size: u32,
    pub header_version: u16,
    pub type_version: u16,
    pub blocks: Vec<Block>,
    pub ext_refs: Vec<ExtRef>,
}

impl Resource {
    pub fn block(&self, tag: &[u8; 4]) -> Option<&Block> {
        self.blocks.iter().find(|b| &b.tag == tag)
    }
    pub fn blocks_tagged<'a>(&'a self, tag: &'a [u8; 4]) -> impl Iterator<Item = &'a Block> + 'a {
        self.blocks.iter().filter(move |b| &b.tag == tag)
    }
}

/// The bytes of a block of `data` (the file the resource was parsed from).
pub fn block_bytes<'a>(data: &'a [u8], b: &Block) -> &'a [u8] {
    &data[b.offset..b.offset + b.size]
}

/// Decode the KV3 in the first block with `tag` (`DATA` for particles, sound events, vdata and models).
/// `None` when the file has no such block or the block isn't KV3 (textures keep a binary header in DATA).
pub fn kv3_block(data: &[u8], tag: &[u8; 4]) -> Option<Result<kv3::Value, String>> {
    let r = parse(data).ok()?;
    let b = r.block(tag)?;
    let bytes = block_bytes(data, b);
    if !kv3::is_kv3(bytes) {
        return None;
    }
    Some(kv3::decode(bytes))
}

pub const KNOWN_HEADER_VERSION: u16 = 12;

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    TooShort,
    NotAResource(String),
    OutOfBounds(&'static str, usize),
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::TooShort => write!(f, "file shorter than a resource header"),
            Error::NotAResource(s) => write!(f, "not a compiled resource: {s}"),
            Error::OutOfBounds(what, at) => write!(f, "{what} points outside the file (byte {at})"),
        }
    }
}
impl std::error::Error for Error {}

fn u16_at(b: &[u8], p: usize) -> Option<u16> {
    b.get(p..p + 2).map(|s| u16::from_le_bytes([s[0], s[1]]))
}
fn u32_at(b: &[u8], p: usize) -> Option<u32> {
    b.get(p..p.checked_add(4)?).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}
fn i64_at(b: &[u8], p: usize) -> Option<i64> {
    u64_at(b, p).map(|v| v as i64)
}
fn u64_at(b: &[u8], p: usize) -> Option<u64> {
    b.get(p..p.checked_add(8)?).map(|s| u64::from_le_bytes([s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7]]))
}
fn cstr_at(b: &[u8], p: usize) -> Option<String> {
    let rest = b.get(p..)?;
    let end = rest.iter().position(|&c| c == 0)?;
    Some(String::from_utf8_lossy(&rest[..end]).into_owned())
}

pub fn parse(b: &[u8]) -> Result<Resource, Error> {
    if b.len() < 16 {
        return Err(Error::TooShort);
    }
    let file_size = u32_at(b, 0).ok_or(Error::TooShort)?;
    if file_size == 0x55AA_1234 {
        return Err(Error::NotAResource("this is a VPK".into()));
    }
    let header_version = u16_at(b, 4).ok_or(Error::TooShort)?;
    let type_version = u16_at(b, 6).ok_or(Error::TooShort)?;
    if header_version != KNOWN_HEADER_VERSION {
        return Err(Error::NotAResource(format!("header version {header_version}, expected {KNOWN_HEADER_VERSION}")));
    }
    let block_offset = u32_at(b, 8).ok_or(Error::TooShort)? as usize;
    let block_count = u32_at(b, 12).ok_or(Error::TooShort)? as usize;
    let table = 8 + block_offset;
    // Big hero models carry 100+ blocks (one MVTX/MIDX/MDAT set per mesh), so the bound is the file
    // itself, not a guess: the block table has to fit.
    if table + block_count * 12 > b.len() {
        return Err(Error::NotAResource(format!("block table ({block_count} blocks) runs past end of file")));
    }
    let mut blocks = Vec::with_capacity(block_count);
    for i in 0..block_count {
        let at = table + i * 12;
        let t = b.get(at..at + 4).ok_or(Error::OutOfBounds("block table", at))?;
        let tag = [t[0], t[1], t[2], t[3]];
        let rel = u32_at(b, at + 4).ok_or(Error::OutOfBounds("block offset", at + 4))? as usize;
        let size = u32_at(b, at + 8).ok_or(Error::OutOfBounds("block size", at + 8))? as usize;
        let offset = at + 4 + rel;
        if offset + size > b.len() {
            return Err(Error::OutOfBounds("block", offset));
        }
        blocks.push(Block { tag, offset, size });
    }

    let ext_refs = match blocks.iter().find(|bl| &bl.tag == b"RERL") {
        Some(rerl) => parse_rerl(&b[rerl.offset..rerl.offset + rerl.size])?,
        None => Vec::new(),
    };
    Ok(Resource { file_size, header_version, type_version, blocks, ext_refs })
}

/// The records of a RERL block (its own bytes: every offset in it is relative to the block).
pub fn parse_rerl(b: &[u8]) -> Result<Vec<ExtRef>, Error> {
    let mut ext_refs = Vec::new();
    if b.len() < 8 {
        return Ok(ext_refs);
    }
    let (Some(rel), Some(count)) = (u32_at(b, 0), u32_at(b, 4)) else { return Ok(ext_refs) };
    let (rel, count) = (rel as usize, count as usize);
    let start = rel;
    for i in 0..count {
        let rec = start + i * 16;
        let id = u64_at(b, rec).ok_or(Error::OutOfBounds("RERL record", rec))?;
        let name_rel = i64_at(b, rec + 8).ok_or(Error::OutOfBounds("RERL name", rec + 8))?;
        let name_at = (rec as i64 + 8 + name_rel) as usize;
        let name = cstr_at(b, name_at).ok_or(Error::OutOfBounds("RERL string", name_at))?;
        ext_refs.push(ExtRef { id, name });
    }
    Ok(ext_refs)
}

/// A file's references read through a range reader (`read(offset, len)`), touching only the header,
/// the block table and the RERL block. Used to index the whole game without reading every byte.
pub fn refs_by_range(read: &mut dyn FnMut(u64, u64) -> Option<Vec<u8>>) -> Option<Vec<ExtRef>> {
    let h = read(0, 16)?;
    if h.len() < 16 || u16_at(&h, 4)? != KNOWN_HEADER_VERSION {
        return None;
    }
    let table = 8 + u32_at(&h, 8)? as u64;
    let n = u32_at(&h, 12)? as u64;
    if n > 4096 {
        return None;
    }
    let t = read(table, n * 12)?;
    for i in 0..n as usize {
        let at = i * 12;
        if t.get(at..at + 4)? == b"RERL" {
            let off = table + at as u64 + 4 + u32_at(&t, at + 4)? as u64;
            let size = u32_at(&t, at + 8)? as u64;
            return parse_rerl(&read(off, size)?).ok();
        }
    }
    Some(Vec::new())
}

/// Build a minimal valid resource with the given external refs. Used by tests and lint fixtures,
/// so every rule that reads references can be proven against a file we fully control.
pub fn synth(ext_refs: &[&str]) -> Vec<u8> {
    synth_with(ext_refs, &[])
}

/// A synthesized resource with extra blocks after the RERL (e.g. a `DATA` block holding KV3 from
/// `kv3::encode`, or a texture header from `texture::synth_header`).
pub fn synth_with(ext_refs: &[&str], extra: &[([u8; 4], Vec<u8>)]) -> Vec<u8> {
    let mut b = synth_rerl(ext_refs);
    if extra.is_empty() {
        return b;
    }
    // Rebuild with a larger block table: header | table (12 per block) | RERL | extras.
    let rerl_old_start = 16 + 12;
    let rerl = b[rerl_old_start..].to_vec();
    let n = 1 + extra.len();
    let table = 16usize;
    let mut out = vec![0u8; table + 12 * n];
    let mut pos = out.len();
    let mut entries = Vec::new();
    // The RERL's internal offsets are relative to its own start, so it moves as one piece.
    entries.push((*b"RERL", pos, rerl.len()));
    out.extend_from_slice(&rerl);
    pos = out.len();
    for (tag, data) in extra {
        while !out.len().is_multiple_of(16) {
            out.push(0);
        }
        pos = out.len();
        entries.push((*tag, pos, data.len()));
        out.extend_from_slice(data);
    }
    let _ = pos;
    let total = out.len();
    out[0..4].copy_from_slice(&(total as u32).to_le_bytes());
    out[4..6].copy_from_slice(&KNOWN_HEADER_VERSION.to_le_bytes());
    out[8..12].copy_from_slice(&((table - 8) as u32).to_le_bytes());
    out[12..16].copy_from_slice(&(n as u32).to_le_bytes());
    for (i, (tag, at, size)) in entries.iter().enumerate() {
        let e = table + i * 12;
        out[e..e + 4].copy_from_slice(tag);
        out[e + 4..e + 8].copy_from_slice(&((at - (e + 4)) as u32).to_le_bytes());
        out[e + 8..e + 12].copy_from_slice(&(*size as u32).to_le_bytes());
    }
    b.clear();
    out
}

fn synth_rerl(ext_refs: &[&str]) -> Vec<u8> {
    // header (16) | block table: RERL (12) | RERL block: offset,count (8) | records (16*n) | strings
    let n = ext_refs.len();
    let table = 16usize;
    let rerl_start = table + 12;
    let records = rerl_start + 8;
    let strings = records + 16 * n;
    let mut s = Vec::new();
    let mut name_pos = Vec::new();
    for r in ext_refs {
        name_pos.push(strings + s.len());
        s.extend_from_slice(r.as_bytes());
        s.push(0);
    }
    let total = strings + s.len();
    let rerl_size = total - rerl_start;
    let mut b = Vec::with_capacity(total);
    b.extend_from_slice(&(total as u32).to_le_bytes());
    b.extend_from_slice(&KNOWN_HEADER_VERSION.to_le_bytes());
    b.extend_from_slice(&0u16.to_le_bytes());
    b.extend_from_slice(&((table - 8) as u32).to_le_bytes());
    b.extend_from_slice(&1u32.to_le_bytes());
    b.extend_from_slice(b"RERL");
    b.extend_from_slice(&((rerl_start - (table + 4)) as u32).to_le_bytes());
    b.extend_from_slice(&(rerl_size as u32).to_le_bytes());
    b.extend_from_slice(&8u32.to_le_bytes()); // records start right after offset+count
    b.extend_from_slice(&(n as u32).to_le_bytes());
    for (i, r) in ext_refs.iter().enumerate() {
        let rec = records + i * 16;
        b.extend_from_slice(&(hash_name(r)).to_le_bytes());
        b.extend_from_slice(&((name_pos[i] as i64) - (rec as i64 + 8)).to_le_bytes());
    }
    b.extend_from_slice(&s);
    b
}

/// Stand-in id for synthesized refs (Valve uses MurmurHash2-64 of the name; the id is not checked here).
fn hash_name(s: &str) -> u64 {
    s.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, c| (h ^ c as u64).wrapping_mul(0x100_0000_01b3))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synth_round_trips() {
        let b = synth(&["materials/a.vmat", "models/x/y.vmdl"]);
        let r = parse(&b).unwrap();
        assert_eq!(r.blocks.len(), 1);
        assert_eq!(r.blocks[0].tag_str(), "RERL");
        let names: Vec<_> = r.ext_refs.iter().map(|e| e.compiled_path()).collect();
        assert_eq!(names, ["materials/a.vmat_c", "models/x/y.vmdl_c"]);
    }

    #[test]
    fn synth_with_extra_blocks() {
        let v = kv3::Value::Object(vec![("m_flRange".into(), kv3::Value::Double(236.0))]);
        let b = synth_with(&["materials/a.vmat"], &[(*b"DATA", kv3::encode(&v))]);
        let r = parse(&b).unwrap();
        assert_eq!(r.ext_refs[0].compiled_path(), "materials/a.vmat_c");
        assert_eq!(kv3_block(&b, b"DATA").unwrap().unwrap(), v);
    }

    #[test]
    fn rejects_vpk_and_short() {
        assert_eq!(parse(&[0u8; 4]).unwrap_err(), Error::TooShort);
        let mut v = vec![0u8; 16];
        v[..4].copy_from_slice(&0x55AA_1234u32.to_le_bytes());
        assert!(matches!(parse(&v), Err(Error::NotAResource(_))));
    }
}
