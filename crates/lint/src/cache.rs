//! The findings cache: what the expensive per-model rules said, keyed by what they read.
//!
//! One entry per (model, depth): the findings of the model rules (M062 contract, the mesh family M100-M140
//! and its material audit). An entry is valid when
//!  - the code that produced it is the code that is running (`CODE`: a hash of the rule, mesh, geometry,
//!    resource and VPK sources, fixed at build time), so editing a rule retires every entry;
//!  - the model's bytes hash the same;
//!  - the game is the same installed build (`Game::identity`: build string, archive sizes and times);
//!  - every file the rules read while judging the model (materials, textures) is, in the mod, the same file
//!    (VPK CRC and size) or still not in the mod, so the game's copy stands.
//!
//! Entries are plain text under the cache directory (`%LOCALAPPDATA%\magician\cache` by default, set by the
//! binary with `set_dir`); a missing, unreadable or stale entry is a miss, never an error. Results are
//! identical with or without the cache: a hit returns exactly the findings a run would compute.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

/// Fingerprint of the code behind the findings (see `build.rs`).
pub const CODE: &str = env!("LINT_CODE_HASH");

static DIR: Mutex<Option<PathBuf>> = Mutex::new(None);
static HITS: AtomicUsize = AtomicUsize::new(0);
static MISSES: AtomicUsize = AtomicUsize::new(0);

/// Where entries live; `None` turns the cache off.
pub fn set_dir(dir: Option<PathBuf>) {
    if let Ok(mut d) = DIR.lock() {
        *d = dir;
    }
}

pub fn dir() -> Option<PathBuf> {
    DIR.lock().ok().and_then(|d| d.clone())
}

/// (hits, misses) of this process's model lookups.
pub fn stats() -> (usize, usize) {
    (HITS.load(Ordering::Relaxed), MISSES.load(Ordering::Relaxed))
}

/// 64-bit hash of a byte string (8 bytes at a time, multiply-fold: fast enough for a 30 MB model).
pub fn hash64(data: &[u8], seed: u64) -> u64 {
    const K: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut h = seed ^ (data.len() as u64).wrapping_mul(K);
    let (words, rest) = data.as_chunks::<8>();
    for c in words {
        let w = u64::from_le_bytes(*c);
        let m = (h ^ w) as u128 * K as u128;
        h = (m as u64) ^ ((m >> 64) as u64);
    }
    if !rest.is_empty() {
        let mut b = [0u8; 8];
        b[..rest.len()].copy_from_slice(rest);
        let m = (h ^ u64::from_le_bytes(b)) as u128 * K as u128;
        h = (m as u64) ^ ((m >> 64) as u64);
    }
    h ^ (h >> 29)
}

/// What a mod file looked like to the rules: its VPK CRC and size, or `None` when the mod has no such file.
pub type Fingerprint = Option<(u32, usize)>;

/// One cached verdict on a model.
pub struct Entry {
    /// Every path the rules read through the mod-first lookup, with what the mod had at that path.
    pub deps: BTreeMap<String, Fingerprint>,
    pub findings: Vec<(String, String)>,
}

/// The cache key of a model's findings.
/// `policy` is the fingerprint of the fair-play limits in force: findings judged at other limits are another entry.
pub fn key(path: &str, model: &[u8], deep: bool, game: &str, valve_present: bool, policy: &str) -> String {
    let mut h = hash64(CODE.as_bytes(), 1);
    h = hash64(policy.as_bytes(), h);
    h = hash64(path.as_bytes(), h);
    h = hash64(game.as_bytes(), h);
    h = hash64(&[deep as u8, valve_present as u8], h);
    let m = hash64(model, h);
    format!("{:016x}{:016x}", m, hash64(model, m ^ 0xA5A5_A5A5_5A5A_5A5A))
}

fn file_of(dir: &Path, key: &str) -> PathBuf {
    dir.join("model").join(format!("{key}.tsv"))
}

fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c => o.push(c),
        }
    }
    o
}

fn unesc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c != '\\' {
            o.push(c);
            continue;
        }
        match it.next() {
            Some('n') => o.push('\n'),
            Some('r') => o.push('\r'),
            Some('t') => o.push('\t'),
            Some(x) => o.push(x),
            None => {}
        }
    }
    o
}

/// The stored entry for `key`, when one is there and its code fingerprint is ours. The caller still checks
/// the dependencies against the mod it is linting (`Entry::valid_for`).
pub fn load(key: &str) -> Option<Entry> {
    let dir = dir()?;
    let text = std::fs::read_to_string(file_of(&dir, key)).ok()?;
    let mut lines = text.lines();
    if lines.next()? != format!("# magician model cache v1 code={CODE}") {
        return None;
    }
    let mut e = Entry { deps: BTreeMap::new(), findings: Vec::new() };
    for l in lines {
        let mut f = l.split('\t');
        match f.next()? {
            "dep" => {
                let p = unesc(f.next()?);
                let fp = match f.next()? {
                    "-" => None,
                    crc => Some((u32::from_str_radix(crc, 16).ok()?, f.next()?.parse().ok()?)),
                };
                e.deps.insert(p, fp);
            }
            "find" => e.findings.push((f.next()?.to_string(), unesc(f.next()?))),
            "end" => return Some(e),
            _ => return None,
        }
    }
    None // no `end` line: a write that didn't finish
}

pub fn store(key: &str, e: &Entry) {
    let Some(dir) = dir() else { return };
    let file = file_of(&dir, key);
    let Some(parent) = file.parent() else { return };
    if std::fs::create_dir_all(parent).is_err() {
        return;
    }
    let mut t = format!("# magician model cache v1 code={CODE}\n");
    for (p, fp) in &e.deps {
        match fp {
            Some((crc, len)) => t.push_str(&format!("dep\t{}\t{crc:08x}\t{len}\n", esc(p))),
            None => t.push_str(&format!("dep\t{}\t-\n", esc(p))),
        }
    }
    for (c, m) in &e.findings {
        t.push_str(&format!("find\t{c}\t{}\n", esc(m)));
    }
    t.push_str("end\n");
    // write then rename: a reader never sees half an entry, and two runs storing the same key just agree
    let tmp = file.with_extension(format!("tmp{}", std::process::id()));
    if std::fs::write(&tmp, t).is_ok() && std::fs::rename(&tmp, &file).is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    prune(parent);
}

/// Keeps the directory bounded: past `MAX_ENTRIES`, the oldest quarter goes.
const MAX_ENTRIES: usize = 400;

fn prune(dir: &Path) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut files: Vec<(std::time::SystemTime, PathBuf)> =
        rd.filter_map(|e| e.ok()).filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path()))).collect();
    if files.len() <= MAX_ENTRIES {
        return;
    }
    files.sort();
    for (_, p) in files.iter().take(files.len() / 4) {
        let _ = std::fs::remove_file(p);
    }
}

/// Deletes every entry; returns how many files went.
pub fn clear() -> usize {
    let Some(dir) = dir() else { return 0 };
    let mut n = 0;
    if let Ok(rd) = std::fs::read_dir(dir.join("model")) {
        for e in rd.filter_map(|e| e.ok()) {
            if std::fs::remove_file(e.path()).is_ok() {
                n += 1;
            }
        }
    }
    n
}

/// (entries, bytes) on disk.
pub fn usage() -> (usize, u64) {
    let Some(dir) = dir() else { return (0, 0) };
    let Ok(rd) = std::fs::read_dir(dir.join("model")) else { return (0, 0) };
    rd.filter_map(|e| e.ok()).filter_map(|e| e.metadata().ok()).fold((0, 0), |(n, b), m| (n + 1, b + m.len()))
}

pub fn note_hit() {
    HITS.fetch_add(1, Ordering::Relaxed);
}

pub fn note_miss() {
    MISSES.fetch_add(1, Ordering::Relaxed);
}

impl Entry {
    /// Whether the mod still has, at every path the rules read, what it had when this entry was made.
    pub fn valid_for(&self, mod_has: &dyn Fn(&str) -> Fingerprint) -> bool {
        self.deps.iter().all(|(p, fp)| mod_has(p) == *fp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_escapes_round_trip() {
        for s in ["plain", "tab\there", "line\nbreak\r\n", "back\\slash \\n literal", "", "M106 [a/b #1 (5 tris)] x\ty"] {
            assert_eq!(unesc(&esc(s)), s);
            assert!(!esc(s).contains(['\t', '\n', '\r']));
        }
    }

    #[test]
    fn an_entry_is_valid_only_while_the_mod_still_has_what_it_read() {
        let mut deps = BTreeMap::new();
        deps.insert("materials/a.vmat_c".to_string(), Some((0xDEAD_BEEF, 100)));
        deps.insert("materials/b_color.vtex_c".to_string(), None);
        let e = Entry { deps, findings: vec![] };
        let same = |p: &str| match p {
            "materials/a.vmat_c" => Some((0xDEAD_BEEF, 100)),
            _ => None,
        };
        assert!(e.valid_for(&same));
        // the mod's copy changed
        assert!(!e.valid_for(&|p: &str| if p == "materials/a.vmat_c" { Some((1, 100)) } else { None }));
        // a file that was the game's now exists in the mod
        assert!(!e.valid_for(&|p: &str| if p == "materials/b_color.vtex_c" { Some((2, 3)) } else { same(p) }));
        // a file the mod had is gone
        assert!(!e.valid_for(&|_: &str| None));
    }

    #[test]
    fn keys_follow_every_input() {
        let policy = crate::policy::DEFAULT.fingerprint();
        let k = |path: &str, data: &[u8], deep: bool, game: &str, valve: bool| key(path, data, deep, game, valve, &policy);
        let base = k("m.vmdl_c", b"model bytes", true, "g1", true);
        assert_eq!(base, k("m.vmdl_c", b"model bytes", true, "g1", true));
        assert_ne!(base, k("n.vmdl_c", b"model bytes", true, "g1", true));
        assert_ne!(base, k("m.vmdl_c", b"model byteZ", true, "g1", true));
        assert_ne!(base, k("m.vmdl_c", b"model bytes", false, "g1", true));
        assert_ne!(base, k("m.vmdl_c", b"model bytes", true, "g2", true));
        assert_ne!(base, k("m.vmdl_c", b"model bytes", true, "g1", false));
        // the same model judged at other fair-play limits is another entry
        let mut other = crate::policy::DEFAULT;
        other.hitbox_egregious_units = 20.0;
        assert_ne!(base, key("m.vmdl_c", b"model bytes", true, "g1", true, &other.fingerprint()));
    }
}
