//! Fuzzing without crates, for Magician's tests.
//!
//! Every reader of bytes someone else made (paks, compiled resources, KV3, the LZ4 and zstd blocks, textures, meshes,
//! cloth, the text files Magician reads) is fed mutations of synthetic seeds. Each case must come back, `Ok` or
//! `Err`, without panicking, within `HANG` and without asking for more memory than `CAP` at once (a size field taken
//! on trust). Cases are deterministic: case `n` of a target is built from the seed and `n` alone, so a failure names
//! the one case that reproduces it.
//!
//! `cargo test` runs `DEFAULT_CASES` per target, which takes well under a second each. For a long run:
//!
//! ```text
//! MAGICIAN_FUZZ=600 cargo test --release --test fuzz -- --test-threads 1   # each target for 600 s
//! MAGICIAN_FUZZ_SEED=7 MAGICIAN_FUZZ_CASE=1234 cargo test --test fuzz       # one case again
//! ```

use std::alloc::{GlobalAlloc, Layout, System};
use std::io::Write;
use std::sync::atomic::{AtomicPtr, AtomicU64, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

/// Cases per target in an ordinary `cargo test`.
pub const DEFAULT_CASES: u64 = 2000;
/// One case taking longer than this is a hang.
pub const HANG: Duration = Duration::from_secs(10);
/// The largest single allocation a case may make. Every seed is a few kilobytes, so anything near this is a length
/// field read from the input and believed.
pub const CAP: usize = 256 << 20;

/// splitmix64: small, fast, and the same everywhere.
#[derive(Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed)
    }
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
    /// A number in `0..n` (0 when `n` is 0).
    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next_u64() % n as u64) as usize
        }
    }
    pub fn chance(&mut self, one_in: usize) -> bool {
        self.below(one_in) == 0
    }
    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}

/// Numbers that break length and offset arithmetic.
const INTERESTING: &[u64] = &[0, 1, 2, 3, 4, 7, 8, 15, 16, 31, 32, 63, 64, 127, 128, 255, 256, 1023, 1024, 4095, 4096, 0x7fff, 0x8000, 0xffff, 0x1_0000, 0x7fff_ffff, 0x8000_0000, 0xffff_fffe, 0xffff_ffff, 0x1_0000_0000, 0x7fff_ffff_ffff_ffff, 0xffff_ffff_ffff_ffff];

/// Tokens that break text readers: section and value syntax, comments, separators, quoting, a BOM, NUL, numbers that
/// do not fit, path-shaped names and characters that are more than one byte.
const TOKENS: &[&str] = &[
    "[", "]", "=", "\"", "#", "\t", "\n", "\r\n", " ", ",", "{", "}", "(", ")", ":", ";", "\\", "/", "..", "../", "..\\", "\u{feff}", "\0", "-", "+", "*", "1e309", "-1",
    "NaN", "inf", "99999999999999999999999", "0x10", "true", "false", "é", "€", "𝄞", "C:\\", "CON", "x:stream", "%", "\u{202e}", "''", "\"\"",
];

/// One mutation of `input`, sometimes splicing in a piece of `others`.
pub fn mutate(rng: &mut Rng, input: &[u8], others: &[Vec<u8>]) -> Vec<u8> {
    let mut v = input.to_vec();
    let rounds = 1 + rng.below(4);
    for _ in 0..rounds {
        let len = v.len();
        match rng.below(11) {
            0 if len > 0 => {
                let i = rng.below(len);
                v[i] ^= 1 << rng.below(8);
            }
            1 if len > 0 => {
                let i = rng.below(len);
                v[i] = *rng.pick(&[0u8, 1, 0x7f, 0x80, 0xfe, 0xff]);
            }
            2 if len >= 4 => {
                // a length or offset field: an interesting number at a 4-aligned spot, little-endian
                let i = rng.below(len / 4) * 4;
                let n = (*rng.pick(INTERESTING) as u32).to_le_bytes();
                let w = if rng.chance(3) { 2 } else { 4 };
                v[i..i + w].copy_from_slice(&n[..w]);
            }
            3 if len >= 8 => {
                let i = rng.below(len - 7);
                v[i..i + 8].copy_from_slice(&rng.pick(INTERESTING).to_le_bytes());
            }
            4 => {
                let at = rng.below(len + 1);
                v.truncate(at);
            }
            5 => {
                let n = 1 + rng.below(64);
                let at = rng.below(len + 1);
                let junk: Vec<u8> = (0..n).map(|_| rng.next_u64() as u8).collect();
                v.splice(at..at, junk);
            }
            6 if len > 1 => {
                let a = rng.below(len);
                let b = (a + 1 + rng.below(64)).min(len);
                v.drain(a..b);
            }
            7 if len > 1 => {
                let a = rng.below(len);
                let b = (a + 1 + rng.below(256)).min(len);
                let chunk = v[a..b].to_vec();
                let at = rng.below(v.len() + 1);
                v.splice(at..at, chunk);
            }
            8 if !others.is_empty() => {
                let o = rng.pick(others);
                if !o.is_empty() {
                    let a = rng.below(o.len());
                    let b = (a + 1 + rng.below(256)).min(o.len());
                    let at = rng.below(len + 1);
                    v.splice(at..at, o[a..b].iter().copied());
                }
            }
            9 => {
                let t = rng.pick(TOKENS).as_bytes();
                let at = rng.below(len + 1);
                v.splice(at..at, t.iter().copied());
            }
            _ if len > 0 => {
                // repeat one byte many times: deep nesting, long runs
                let i = rng.below(len);
                let n = 1 + rng.below(512);
                let b = v[i];
                v.splice(i..i, std::iter::repeat_n(b, n));
            }
            _ => v.push(rng.next_u64() as u8),
        }
    }
    v
}

/// One mutation of a text: the byte mutations, or token edits that keep it text more often.
pub fn mutate_text(rng: &mut Rng, input: &str, others: &[String]) -> String {
    if rng.chance(3) {
        let bytes: Vec<Vec<u8>> = others.iter().map(|s| s.as_bytes().to_vec()).collect();
        return String::from_utf8_lossy(&mutate(rng, input.as_bytes(), &bytes)).into_owned();
    }
    let mut s = input.to_string();
    for _ in 0..1 + rng.below(4) {
        let cuts: Vec<usize> = (0..=s.len()).filter(|i| s.is_char_boundary(*i)).collect();
        let at = *rng.pick(&cuts);
        match rng.below(4) {
            0 => s.insert_str(at, rng.pick(TOKENS)),
            1 => {
                // drop up to 40 bytes, cut on character boundaries
                let end = cuts.iter().copied().filter(|c| *c >= at && *c <= at + 40).max().unwrap_or(at);
                s.replace_range(at..end, "");
            }
            2 if !others.is_empty() => {
                let o = rng.pick(others);
                let lines: Vec<&str> = o.lines().collect();
                if !lines.is_empty() {
                    s.insert_str(at, &format!("{}\n", rng.pick(&lines)));
                }
            }
            _ => {
                let lines: Vec<&str> = s.lines().collect();
                if !lines.is_empty() {
                    let l = rng.pick(&lines).to_string();
                    s.insert_str(at, &format!("\n{l}\n"));
                }
            }
        }
    }
    s
}

fn env_u64(name: &str) -> Option<u64> {
    std::env::var(name).ok().and_then(|v| v.trim().parse().ok())
}

/// FNV-1a of the target name: each target gets its own stream by default.
fn name_seed(name: &str) -> u64 {
    name.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ b as u64).wrapping_mul(0x0100_0000_01b3))
}

/// The case the fuzzer is on and its input, for the allocation cap's message (the cap cannot save a file: it runs
/// inside the allocator).
static CASE: AtomicU64 = AtomicU64::new(u64::MAX);
static INPUT: AtomicPtr<u8> = AtomicPtr::new(std::ptr::null_mut());
static INPUT_LEN: AtomicUsize = AtomicUsize::new(0);

/// Feed `target` the seeds, then mutations of them, as described at the top. `cases` replaces `DEFAULT_CASES` for a
/// slow target. Panics, naming the case and saving its input beside the system's temp folder, on the first case that
/// panics or hangs.
pub fn run<F>(name: &str, cases: u64, seeds: Vec<Vec<u8>>, target: F)
where
    F: Fn(&[u8]) + Send + Sync + 'static,
{
    assert!(!seeds.is_empty(), "{name}: no seeds");
    let seed = env_u64("MAGICIAN_FUZZ_SEED").unwrap_or_else(|| name_seed(name));
    let only = env_u64("MAGICIAN_FUZZ_CASE");
    let deadline = env_u64("MAGICIAN_FUZZ").map(|s| Instant::now() + Duration::from_secs(s));
    let total = if deadline.is_some() { u64::MAX } else { cases };
    let _ = writeln!(std::io::stderr(), "fuzz {name}: seed {seed}, {}", if deadline.is_some() { "timed".to_string() } else { format!("{total} cases") });
    let current: Arc<Mutex<(u64, Vec<u8>)>> = Arc::new(Mutex::new((0, Vec::new())));
    let (tx, rx) = mpsc::channel::<Result<u64, String>>();
    let worker_current = current.clone();
    let seeds = Arc::new(seeds);
    let worker_seeds = seeds.clone();
    std::thread::spawn(move || {
        let range: Box<dyn Iterator<Item = u64>> = match only {
            Some(n) => Box::new(n..=n),
            None => Box::new(0..total),
        };
        for n in range {
            if deadline.is_some_and(|d| Instant::now() > d) {
                break;
            }
            let input = case(&worker_seeds, seed, n);
            *worker_current.lock().unwrap_or_else(|p| p.into_inner()) = (n, input.clone());
            CASE.store(n, Ordering::Relaxed);
            INPUT_LEN.store(input.len(), Ordering::Relaxed);
            INPUT.store(input.as_ptr() as *mut u8, Ordering::Relaxed);
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| target(&input)));
            INPUT.store(std::ptr::null_mut(), Ordering::Relaxed);
            CASE.store(u64::MAX, Ordering::Relaxed);
            let msg = match r {
                Ok(()) => Ok(n),
                Err(p) => Err(p.downcast_ref::<String>().cloned().or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default()),
            };
            let stop = msg.is_err();
            if tx.send(msg).is_err() || stop {
                return;
            }
        }
    });
    let mut done = 0u64;
    loop {
        match rx.recv_timeout(HANG) {
            Ok(Ok(_)) => done += 1,
            Ok(Err(why)) => fail(name, seed, &current, &format!("panicked: {why}")),
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => fail(name, seed, &current, &format!("took longer than {} s", HANG.as_secs())),
        }
    }
    let _ = writeln!(std::io::stderr(), "fuzz {name}: {done} cases clean");
}

/// Case `n`: the seeds first, as they are, then mutations built from `(seed, n)` alone.
pub fn case(seeds: &[Vec<u8>], seed: u64, n: u64) -> Vec<u8> {
    if (n as usize) < seeds.len() {
        return seeds[n as usize].clone();
    }
    let mut rng = Rng::new(seed ^ n.wrapping_mul(0x2545_f491_4f6c_dd1d));
    let base = rng.pick(seeds).clone();
    let mut v = mutate(&mut rng, &base, seeds);
    // now and then mutate a mutation, to get further from the seeds
    while rng.chance(3) {
        v = mutate(&mut rng, &v, seeds);
    }
    v
}

fn fail(name: &str, seed: u64, current: &Mutex<(u64, Vec<u8>)>, why: &str) -> ! {
    let (n, input) = current.lock().map(|c| c.clone()).unwrap_or_else(|p| p.into_inner().clone());
    let saved = std::env::temp_dir().join(format!("magician-fuzz-{name}-{seed}-{n}.bin"));
    let _ = std::fs::write(&saved, &input);
    panic!("fuzz {name}: case {n} {why}\n  input ({} bytes) saved to {}\n  again: MAGICIAN_FUZZ_SEED={seed} MAGICIAN_FUZZ_CASE={n}", input.len(), saved.display());
}

/// Every file in `dir` (sorted by name): the inputs past runs found, kept so each is a seed of every run after. A
/// missing folder is no cases.
pub fn corpus(dir: impl AsRef<std::path::Path>) -> Vec<Vec<u8>> {
    let mut names: Vec<std::path::PathBuf> = std::fs::read_dir(dir).map(|rd| rd.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.is_file()).collect()).unwrap_or_default();
    names.sort();
    names.iter().filter_map(|p| std::fs::read(p).ok()).collect()
}

/// The smallest input, found by cutting pieces out of `input`, that still makes `target` panic: what a regression
/// test pins. Returns `input` unchanged if it does not panic.
pub fn minimize(input: &[u8], target: impl Fn(&[u8])) -> Vec<u8> {
    let panics = |b: &[u8]| std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| target(b))).is_err();
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let mut best = input.to_vec();
    if panics(&best) {
        let mut chunk = best.len().max(1);
        while chunk > 0 {
            let mut i = 0;
            while i < best.len() {
                let mut cut = best.clone();
                cut.drain(i..(i + chunk).min(cut.len()));
                if panics(&cut) {
                    best = cut;
                } else {
                    i += chunk;
                }
            }
            chunk /= 2;
        }
    }
    std::panic::set_hook(hook);
    best
}

/// The same run over text: seeds and mutations stay UTF-8.
pub fn run_text<F>(name: &str, cases: u64, seeds: &[&str], target: F)
where
    F: Fn(&str) + Send + Sync + 'static,
{
    let seeds: Vec<Vec<u8>> = seeds.iter().map(|s| s.as_bytes().to_vec()).collect();
    run(name, cases, seeds, move |b| target(&String::from_utf8_lossy(b)));
}

/// A global allocator for fuzz test binaries that refuses any single allocation over `CAP`, saying which case asked,
/// so a length field believed from the input aborts the run instead of passing on a machine with memory to spare.
///
/// The one place in Magician that needs `unsafe`: `GlobalAlloc` is an unsafe trait, and this forwards to `System`
/// unchanged below the cap. It is only ever installed in test binaries (`#[global_allocator]` in `tests/fuzz.rs`).
pub struct Capped;

#[allow(unsafe_code)]
unsafe impl GlobalAlloc for Capped {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if layout.size() > CAP {
            over_cap(layout.size());
            return std::ptr::null_mut();
        }
        // SAFETY: the caller's contract for `alloc` is passed on to `System` as it is.
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if layout.size() > CAP {
            over_cap(layout.size());
            return std::ptr::null_mut();
        }
        // SAFETY: as for `alloc`.
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: `ptr` came from `System` through this allocator, with this layout.
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if new_size > CAP {
            over_cap(new_size);
            return std::ptr::null_mut();
        }
        // SAFETY: as for `dealloc`; `new_size` is checked above and non-zero per the caller's contract.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

/// Say which case asked for `size` bytes, and its input in hex, without allocating (this runs inside the allocator).
#[allow(unsafe_code)]
fn over_cap(size: usize) {
    fn digits(mut n: u64, buf: &mut [u8; 20]) -> &[u8] {
        let mut i = buf.len();
        loop {
            i -= 1;
            buf[i] = b'0' + (n % 10) as u8;
            n /= 10;
            if n == 0 {
                return &buf[i..];
            }
        }
    }
    let mut e = std::io::stderr();
    let (mut a, mut b) = ([0u8; 20], [0u8; 20]);
    let _ = e.write_all(b"\nfuzz: an allocation of ");
    let _ = e.write_all(digits(size as u64, &mut a));
    let _ = e.write_all(b" bytes, over the cap, in case ");
    let _ = e.write_all(digits(CASE.load(Ordering::Relaxed), &mut b));
    let _ = e.write_all(b" (a length taken from the input on trust; rerun with MAGICIAN_FUZZ_CASE)\n");
    let p = INPUT.load(Ordering::Relaxed);
    if !p.is_null() {
        // SAFETY: `INPUT` points at the case's input for exactly as long as the target runs, and only the fuzz
        // worker thread sets it; the allocation that brought us here happened inside that call.
        let input = unsafe { std::slice::from_raw_parts(p, INPUT_LEN.load(Ordering::Relaxed)) };
        let _ = e.write_all(b"fuzz: input hex ");
        for &byte in input {
            let hex = b"0123456789abcdef";
            let _ = e.write_all(&[hex[(byte >> 4) as usize], hex[(byte & 15) as usize]]);
        }
        let _ = e.write_all(b"\n");
    }
    // where it was asked for, when MAGICIAN_FUZZ_BACKTRACE is set (the capture allocates, well under the cap)
    if std::env::var_os("MAGICIAN_FUZZ_BACKTRACE").is_some() {
        let _ = writeln!(e, "{}", std::backtrace::Backtrace::force_capture());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cases_are_reproducible_from_the_seed_and_their_number() {
        let seeds = vec![b"hello world, a seed".to_vec(), vec![1, 2, 3, 4, 5, 6, 7, 8]];
        for n in [0, 1, 2, 17, 999] {
            assert_eq!(case(&seeds, 42, n), case(&seeds, 42, n));
        }
        assert_eq!(case(&seeds, 42, 1), seeds[1], "the seeds come first, unchanged");
        assert!((2..200).any(|n| case(&seeds, 42, n) != case(&seeds, 43, n)), "another seed, other cases");
    }

    #[test]
    fn a_panicking_target_is_reported_with_its_case() {
        let r = std::panic::catch_unwind(|| run("selftest", 50, vec![vec![0u8; 8]], |b| assert!(b.len() < 9, "grew")));
        let msg = r.err().and_then(|p| p.downcast_ref::<String>().cloned()).unwrap_or_default();
        assert!(msg.contains("fuzz selftest: case") && msg.contains("panicked: grew") && msg.contains("MAGICIAN_FUZZ_CASE="), "{msg}");
    }

    #[test]
    fn text_mutations_stay_text() {
        let mut rng = Rng::new(1);
        for _ in 0..500 {
            let t = mutate_text(&mut rng, "[mod]\nslug = \"x\"\n", &["[rights]\nstate = \"cleared\"\n".to_string()]);
            assert!(std::str::from_utf8(t.as_bytes()).is_ok());
        }
    }
}
