//! Valve VPK archives: the container every Deadlock asset and every mod ships in.
//!
//! Layout (v2, which Deadlock uses): a 28-byte header, then a directory tree grouped
//! extension -> directory -> file name, each list ended by an empty string. A blank name
//! is written as a single space. Each file entry is:
//!   crc32 u32 | preload_len u16 | archive_index u16 | offset u32 | length u32 | 0xFFFF u16 | preload bytes
//! `archive_index == 0x7FFF` means the data lives in the `_dir.vpk` itself, after the tree;
//! anything else points into a chunk file `<name>_NNN.vpk`.

#![forbid(unsafe_code)]
// They read files other people made: an unwrap, an expect or a str sliced at a byte offset is a panic one byte away.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::string_slice)]

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

pub const SIGNATURE: u32 = 0x55AA_1234;
pub const INLINE_ARCHIVE: u16 = 0x7FFF;
/// The most file data write puts in one single-file VPK: 2 GiB, the line the community's CS2 Workshop Manager packs
/// to; its multichunk packer and the CSDK launcher's Create VPK split bigger ones into chunk files.
pub const SINGLE_FILE_LIMIT: usize = 0x7FFF_FFFF;
const TERMINATOR: u16 = 0xFFFF;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Full path exactly as stored (Valve stores lowercase; we do not normalise on read,
    /// because a non-lowercase path in a mod is itself a defect the linter reports).
    pub path: String,
    pub crc: u32,
    pub preload: Vec<u8>,
    pub archive_index: u16,
    pub offset: u32,
    pub length: u32,
}

impl Entry {
    /// Total file size: preload bytes plus archive bytes.
    pub fn size(&self) -> u64 {
        self.preload.len() as u64 + self.length as u64
    }
    pub fn is_inline(&self) -> bool {
        self.archive_index == INLINE_ARCHIVE
    }
}

#[derive(Debug)]
pub struct Vpk {
    pub dir_path: PathBuf,
    pub version: u32,
    pub entries: Vec<Entry>,
    /// Absolute file offset where inline (0x7FFF) data starts in the _dir file.
    data_start: u64,
    /// Open handles per archive index. Reopening per read costs ~5 ms each on Windows (AV scan on
    /// open), which turns a 16k-file scan into minutes.
    handles: std::sync::Mutex<std::collections::HashMap<u16, (File, u64)>>,
    index: std::sync::OnceLock<std::collections::HashMap<String, usize>>,
}

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Format(String),
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(e) => write!(f, "io: {e}"),
            Error::Format(s) => write!(f, "bad vpk: {s}"),
        }
    }
}
impl std::error::Error for Error {}
impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::Io(e)
    }
}
pub type Result<T> = std::result::Result<T, Error>;

struct Cursor<'a> {
    b: &'a [u8],
    p: usize,
}
impl<'a> Cursor<'a> {
    fn need(&self, n: usize) -> Result<()> {
        if self.p + n > self.b.len() {
            return Err(Error::Format(format!("truncated at byte {}", self.p)));
        }
        Ok(())
    }
    fn u16(&mut self) -> Result<u16> {
        self.need(2)?;
        let v = u16::from_le_bytes([self.b[self.p], self.b[self.p + 1]]);
        self.p += 2;
        Ok(v)
    }
    fn u32(&mut self) -> Result<u32> {
        self.need(4)?;
        let s = &self.b[self.p..self.p + 4];
        let v = u32::from_le_bytes([s[0], s[1], s[2], s[3]]);
        self.p += 4;
        Ok(v)
    }
    fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        self.need(n)?;
        let s = &self.b[self.p..self.p + n];
        self.p += n;
        Ok(s)
    }
    fn cstr(&mut self) -> Result<&'a str> {
        let start = self.p;
        let end = self.b[start..]
            .iter()
            .position(|&c| c == 0)
            .ok_or_else(|| Error::Format(format!("unterminated string at byte {start}")))?;
        self.p = start + end + 1;
        std::str::from_utf8(&self.b[start..start + end])
            .map_err(|_| Error::Format(format!("non-utf8 name at byte {start}")))
    }
}

/// Parse a directory tree (shared by reader and tests).
fn parse_tree(tree: &[u8]) -> Result<Vec<Entry>> {
    let mut c = Cursor { b: tree, p: 0 };
    let mut out = Vec::new();
    loop {
        let ext = c.cstr()?;
        if ext.is_empty() {
            break;
        }
        loop {
            let dir = c.cstr()?;
            if dir.is_empty() {
                break;
            }
            loop {
                let name = c.cstr()?;
                if name.is_empty() {
                    break;
                }
                let crc = c.u32()?;
                let preload_len = c.u16()?;
                let archive_index = c.u16()?;
                let offset = c.u32()?;
                let length = c.u32()?;
                let term = c.u16()?;
                if term != TERMINATOR {
                    return Err(Error::Format(format!("entry {name}: terminator {term:#06x}")));
                }
                let preload = c.bytes(preload_len as usize)?.to_vec();
                out.push(Entry {
                    path: join_path(dir, name, ext),
                    crc,
                    preload,
                    archive_index,
                    offset,
                    length,
                });
            }
        }
    }
    Ok(out)
}

fn join_path(dir: &str, name: &str, ext: &str) -> String {
    let mut s = String::new();
    if dir != " " {
        s.push_str(dir);
        s.push('/');
    }
    if name != " " {
        s.push_str(name);
    }
    if ext != " " {
        s.push('.');
        s.push_str(ext);
    }
    s
}

/// Split "a/b/c.ext" into ("a/b", "c", "ext") using Valve's blank-is-space convention.
fn split_path(path: &str) -> (String, String, String) {
    let (dir, file) = path.rsplit_once('/').unwrap_or(("", path));
    let (name, ext) = file.rsplit_once('.').unwrap_or((file, ""));
    let sp = |s: &str| if s.is_empty() { " ".to_string() } else { s.to_string() };
    (sp(dir), sp(name), sp(ext))
}

impl Vpk {
    pub fn open(dir_path: impl AsRef<Path>) -> Result<Vpk> {
        let dir_path = dir_path.as_ref().to_path_buf();
        let mut f = File::open(&dir_path)?;
        let len = f.metadata()?.len();
        if len < 12 {
            return Err(Error::Format(format!("{len} bytes, too short for a VPK's header: not a pak (an empty or cut-off download?)")));
        }
        let mut head = [0u8; 28];
        f.read_exact(&mut head[..12])?;
        let word = |i: usize| u32::from_le_bytes([head[i], head[i + 1], head[i + 2], head[i + 3]]);
        let sig = word(0);
        if sig != SIGNATURE {
            return Err(Error::Format(format!("signature {sig:#010x}, expected {SIGNATURE:#010x}")));
        }
        let version = word(4);
        let tree_size = word(8) as usize;
        let header_len = match version {
            1 => 12,
            2 if len < 28 => return Err(Error::Format(format!("{len} bytes, too short for a version 2 VPK's header: a cut-off file"))),
            2 => {
                f.read_exact(&mut head[12..28])?;
                28
            }
            v => return Err(Error::Format(format!("unsupported version {v}"))),
        };
        // the tree's size is the header's word: it has to fit in the file before it sizes a buffer
        let file_len = f.metadata()?.len();
        if (header_len + tree_size) as u64 > file_len {
            return Err(Error::Format(format!("directory tree of {tree_size} bytes runs past the end of the {file_len}-byte file")));
        }
        let mut tree = vec![0u8; tree_size];
        f.read_exact(&mut tree)?;
        let entries = parse_tree(&tree)?;
        Ok(Vpk {
            dir_path,
            version,
            entries,
            data_start: (header_len + tree_size) as u64,
            handles: Default::default(),
            index: Default::default(),
        })
    }

    /// `open`, once per process per path: the game's 150k-entry directory is parsed the first time a command
    /// needs it and shared after that (`Vpk` reads through one handle per archive, behind a lock).
    pub fn open_shared(dir_path: impl AsRef<Path>) -> Result<std::sync::Arc<Vpk>> {
        static OPEN: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<PathBuf, std::sync::Arc<Vpk>>>> = std::sync::OnceLock::new();
        let path = dir_path.as_ref().to_path_buf();
        let map = OPEN.get_or_init(Default::default);
        let mut m = map.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(v) = m.get(&path) {
            return Ok(v.clone());
        }
        let v = std::sync::Arc::new(Vpk::open(&path)?);
        m.insert(path, v.clone());
        Ok(v)
    }

    /// Path of chunk `index` next to a `<name>_dir.vpk`.
    pub fn chunk_path(&self, index: u16) -> PathBuf {
        let s = self.dir_path.to_string_lossy();
        let base = s.strip_suffix("_dir.vpk").unwrap_or(s.trim_end_matches(".vpk"));
        PathBuf::from(format!("{base}_{index:03}.vpk"))
    }

    pub fn read(&self, e: &Entry) -> Result<Vec<u8>> {
        let mut out = Vec::with_capacity(e.preload.len());
        out.extend_from_slice(&e.preload);
        if e.length > 0 {
            let off = if e.is_inline() { self.data_start + e.offset as u64 } else { e.offset as u64 };
            let mut handles = self.handles.lock().unwrap_or_else(|p| p.into_inner());
            let f = match handles.entry(e.archive_index) {
                std::collections::hash_map::Entry::Occupied(o) => o.into_mut(),
                std::collections::hash_map::Entry::Vacant(v) => {
                    let path = if e.is_inline() { self.dir_path.clone() } else { self.chunk_path(e.archive_index) };
                    let f = File::open(&path)?;
                    let len = f.metadata()?.len();
                    v.insert((f, len))
                }
            };
            let (f, size) = (&mut f.0, f.1);
            fits(size, off, e.length as u64, &e.path)?;
            f.seek(SeekFrom::Start(off))?;
            let start = out.len();
            out.resize(start + e.length as usize, 0);
            f.read_exact(&mut out[start..])?;
        }
        Ok(out)
    }

    /// Several entries' bytes, in the order given, read in the order they lie on disk (archive, then offset). On a
    /// spinning disk the directory's order is a seek per file: a 608 MB pak of 1,835 files read in 40 s that way.
    pub fn read_many(&self, entries: &[&Entry]) -> Vec<Result<Vec<u8>>> {
        let mut order: Vec<usize> = (0..entries.len()).collect();
        order.sort_by_key(|&i| (entries[i].archive_index, entries[i].offset));
        let mut out: Vec<Option<Result<Vec<u8>>>> = (0..entries.len()).map(|_| None).collect();
        for i in order {
            out[i] = Some(self.read(entries[i]));
        }
        out.into_iter().map(|r| r.unwrap_or_else(|| Ok(Vec::new()))).collect()
    }

    /// Bytes `[start, start + len)` of a file, without reading the rest (clamped to the file's size).
    /// Reading only a resource's header and one block keeps a whole-game index pass off most of 26 GB.
    pub fn read_range(&self, e: &Entry, start: u64, len: u64) -> Result<Vec<u8>> {
        let end = start.saturating_add(len).min(e.size());
        if start >= end {
            return Ok(Vec::new());
        }
        let mut out = Vec::with_capacity((end - start) as usize);
        let pre = e.preload.len() as u64;
        if start < pre {
            out.extend_from_slice(&e.preload[start as usize..end.min(pre) as usize]);
        }
        if end > pre {
            let from = start.max(pre) - pre;
            let n = (end - pre - from) as usize;
            let base = if e.is_inline() { self.data_start + e.offset as u64 } else { e.offset as u64 };
            let mut handles = self.handles.lock().unwrap_or_else(|p| p.into_inner());
            let f = match handles.entry(e.archive_index) {
                std::collections::hash_map::Entry::Occupied(o) => o.into_mut(),
                std::collections::hash_map::Entry::Vacant(v) => {
                    let path = if e.is_inline() { self.dir_path.clone() } else { self.chunk_path(e.archive_index) };
                    let f = File::open(&path)?;
                    let len = f.metadata()?.len();
                    v.insert((f, len))
                }
            };
            let (f, size) = (&mut f.0, f.1);
            fits(size, base + from, n as u64, &e.path)?;
            f.seek(SeekFrom::Start(base + from))?;
            let at = out.len();
            out.resize(at + n, 0);
            f.read_exact(&mut out[at..])?;
        }
        Ok(out)
    }

    pub fn find(&self, path: &str) -> Option<&Entry> {
        let idx = self.index.get_or_init(|| self.entries.iter().enumerate().map(|(i, e)| (e.path.clone(), i)).collect());
        idx.get(path).map(|&i| &self.entries[i])
    }
}

/// IEEE CRC-32, the checksum VPK stores per file.
/// Whether `len` bytes at `off` are in the archive: an entry's offset and length are the pak's word, checked before
/// they size a buffer.
fn fits(size: u64, off: u64, len: u64, path: &str) -> Result<()> {
    if off.checked_add(len).is_none_or(|end| end > size) {
        return Err(Error::Format(format!("{path}: {len} bytes at offset {off} run past the end of the {size}-byte archive")));
    }
    Ok(())
}

pub fn crc32(data: &[u8]) -> u32 {
    // slice-by-8: eight table lookups per 8 input bytes (the byte-at-a-time loop is ~8x slower, and a
    // lint reads the CRC of every file in a 90 MB release)
    static TABLES: std::sync::OnceLock<Box<[[u32; 256]; 8]>> = std::sync::OnceLock::new();
    let t = TABLES.get_or_init(|| {
        let mut t = Box::new([[0u32; 256]; 8]);
        for i in 0..256 {
            let mut c = i as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            }
            t[0][i] = c;
        }
        for i in 0..256 {
            for k in 1..8 {
                let prev = t[k - 1][i];
                t[k][i] = t[0][(prev & 0xFF) as usize] ^ (prev >> 8);
            }
        }
        t
    });
    let mut c = !0u32;
    let (words, rest) = data.as_chunks::<8>();
    for ch in words {
        let lo = u32::from_le_bytes([ch[0], ch[1], ch[2], ch[3]]) ^ c;
        let hi = u32::from_le_bytes([ch[4], ch[5], ch[6], ch[7]]);
        c = t[7][(lo & 0xFF) as usize]
            ^ t[6][((lo >> 8) & 0xFF) as usize]
            ^ t[5][((lo >> 16) & 0xFF) as usize]
            ^ t[4][(lo >> 24) as usize]
            ^ t[3][(hi & 0xFF) as usize]
            ^ t[2][((hi >> 8) & 0xFF) as usize]
            ^ t[1][((hi >> 16) & 0xFF) as usize]
            ^ t[0][(hi >> 24) as usize];
    }
    for &b in rest {
        c = t[0][((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8);
    }
    !c
}

/// Offsets and sizes are u32 and the community's single-file packers stop at 2 GiB; past that a pak needs chunk files,
/// which `write` does not make. Refused rather than written with offsets that wrap.
pub fn fits_one_file(total: usize) -> Result<()> {
    if total > SINGLE_FILE_LIMIT {
        return Err(Error::Format(format!("{total} bytes of files: a single-file VPK holds at most {SINGLE_FILE_LIMIT} (2 GiB), and Magician does not write chunk files; split the mod over two paks")));
    }
    Ok(())
}

/// Write a single-file v2 VPK: every file inline (archive 0x7FFF), no MD5 or signature sections.
/// Paths are written exactly as given; normalisation is the caller's (and the linter's) job.
/// Output is deterministic: entries are sorted, so identical inputs give byte-identical archives.
pub fn write(out: impl AsRef<Path>, files: &[(String, Vec<u8>)]) -> Result<()> {
    fits_one_file(files.iter().map(|f| f.1.len()).sum())?;
    // ext -> dir -> [(name, index into files)]
    let mut tree: BTreeMap<String, BTreeMap<String, Vec<(String, usize)>>> = BTreeMap::new();
    for (i, (path, _)) in files.iter().enumerate() {
        let (dir, name, ext) = split_path(path);
        tree.entry(ext).or_default().entry(dir).or_default().push((name, i));
    }
    let mut t = Vec::new();
    let mut data = Vec::new();
    for (ext, dirs) in &tree {
        t.extend_from_slice(ext.as_bytes());
        t.push(0);
        for (dir, names) in dirs {
            t.extend_from_slice(dir.as_bytes());
            t.push(0);
            let mut names = names.clone();
            names.sort();
            for (name, i) in names {
                let bytes = &files[i].1;
                t.extend_from_slice(name.as_bytes());
                t.push(0);
                t.extend_from_slice(&crc32(bytes).to_le_bytes());
                t.extend_from_slice(&0u16.to_le_bytes()); // no preload
                t.extend_from_slice(&INLINE_ARCHIVE.to_le_bytes());
                t.extend_from_slice(&(data.len() as u32).to_le_bytes());
                t.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
                t.extend_from_slice(&TERMINATOR.to_le_bytes());
                data.extend_from_slice(bytes);
            }
            t.push(0);
        }
        t.push(0);
    }
    t.push(0);
    let mut f = File::create(out)?;
    f.write_all(&SIGNATURE.to_le_bytes())?;
    f.write_all(&2u32.to_le_bytes())?;
    f.write_all(&(t.len() as u32).to_le_bytes())?;
    f.write_all(&(data.len() as u32).to_le_bytes())?; // file data section
    f.write_all(&0u32.to_le_bytes())?; // archive MD5 section
    f.write_all(&0u32.to_le_bytes())?; // other MD5 section
    f.write_all(&0u32.to_le_bytes())?; // signature section
    f.write_all(&t)?;
    f.write_all(&data)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pak_past_two_gib_is_refused_not_wrapped() {
        assert!(fits_one_file(SINGLE_FILE_LIMIT).is_ok());
        assert!(fits_one_file(SINGLE_FILE_LIMIT + 1).unwrap_err().to_string().contains("2 GiB"));
    }

    #[test]
    fn a_file_too_short_for_a_header_says_so_and_many_reads_come_back_in_the_order_asked() {
        let dir = std::env::temp_dir().join(format!("magician-vpk-short-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // an empty download was "io: failed to fill whole buffer"
        let empty = dir.join("empty.vpk");
        std::fs::write(&empty, b"").unwrap();
        assert!(Vpk::open(&empty).unwrap_err().to_string().contains("0 bytes, too short for a VPK's header"));
        let p = dir.join("pak01_dir.vpk");
        let files: Vec<(String, Vec<u8>)> = (0..20).map(|n| (format!("z/{:02}.vtex_c", 19 - n), vec![n as u8; 100 + n])).collect();
        write(&p, &files).unwrap();
        let mut cut = std::fs::read(&p).unwrap();
        cut.truncate(20);
        std::fs::write(dir.join("cut.vpk"), &cut).unwrap();
        assert!(Vpk::open(dir.join("cut.vpk")).unwrap_err().to_string().contains("too short for a version 2 VPK's header"));
        let v = Vpk::open(&p).unwrap();
        let mut asked: Vec<&Entry> = v.entries.iter().collect();
        asked.reverse();
        let got = v.read_many(&asked);
        for (e, data) in asked.iter().zip(got) {
            assert_eq!(data.unwrap(), v.read(e).unwrap(), "{}", e.path);
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn crc32_known_vector() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    /// The fast path (eight bytes a step) agrees with the plain bitwise definition at every length and offset.
    #[test]
    fn crc32_matches_the_bitwise_definition() {
        fn bitwise(data: &[u8]) -> u32 {
            let mut c = !0u32;
            for &b in data {
                c ^= b as u32;
                for _ in 0..8 {
                    c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
                }
            }
            !c
        }
        let mut x = 0x1234_5678u32;
        let data: Vec<u8> = (0..300).map(|_| { x = x.wrapping_mul(1664525).wrapping_add(1013904223); (x >> 24) as u8 }).collect();
        for start in 0..9 {
            for len in 0..(data.len() - start) {
                assert_eq!(crc32(&data[start..start + len]), bitwise(&data[start..start + len]), "start {start} len {len}");
            }
        }
    }

    #[test]
    fn round_trip_including_blank_dir_and_ext() {
        let dir = std::env::temp_dir().join(format!("magician-vpk-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("pak01_dir.vpk");
        let files = vec![
            ("models/heroes_wip/lash/lash.vmdl_c".to_string(), b"model".to_vec()),
            ("addoninfo.txt".to_string(), b"root file".to_vec()),
            ("scripts/noext".to_string(), b"no extension".to_vec()),
            ("empty.vtex_c".to_string(), Vec::new()),
        ];
        write(&p, &files).unwrap();
        let v = Vpk::open(&p).unwrap();
        assert_eq!(v.version, 2);
        assert_eq!(v.entries.len(), files.len());
        for (path, bytes) in &files {
            let e = v.find(path).unwrap_or_else(|| panic!("missing {path}"));
            assert_eq!(&v.read(e).unwrap(), bytes, "{path}");
            assert_eq!(e.crc, crc32(bytes), "{path}");
        }
        // determinism: same input, same bytes
        let p2 = dir.join("pak02_dir.vpk");
        write(&p2, &files).unwrap();
        assert_eq!(std::fs::read(&p).unwrap(), std::fs::read(&p2).unwrap());
        std::fs::remove_dir_all(&dir).ok();
    }
}
