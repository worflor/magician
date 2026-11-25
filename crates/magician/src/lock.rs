//! A per-addon build lock.
//!
//! `build` and `release` both run resourcecompiler into the addon's game-side output folder
//! (`<csdk>\game\citadel_addons\<addon>\`) and then read it back to package. Two of them at once, or one
//! reading while another compiles, corrupt each other's run (releases overlapped another pipeline's compile).
//! So each takes an exclusive lock: a file in that folder holding the holder's pid, start time and name.
//! A second taker waits with a message, or fails at once with `--no-wait`, or after `--lock-timeout`.
//! A lock whose pid is no longer running (a killed build, a crash) is stale and is cleared.
//!
//! A lock file appears whole or not at all: it is written under a name of its own and hard-linked into place, which
//! fails if a lock is there. A file created empty and written after would be read, by a taker that has waited a while,
//! as an unreadable lock to clear, and two holders would then both run. Each holder also carries a token of its own,
//! so threads of one process (the same pid, the same second) never take each other's lock for theirs.

use crate::cli::Stop;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const LOCK_FILE: &str = ".magician-build.lock";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Holder {
    pub pid: u32,
    /// Unix seconds when the lock was taken.
    pub started: u64,
    pub what: String,
    /// Unique to this taking of the lock (empty in a lock written before tokens).
    pub token: String,
}

impl Holder {
    fn text(&self) -> String {
        format!("pid {}\nstarted {}\nholder {}\ntoken {}\n", self.pid, self.started, self.what, self.token)
    }
    pub fn parse(t: &str) -> Option<Holder> {
        let mut pid = None;
        let mut started = 0;
        let mut what = String::new();
        let mut token = String::new();
        for l in t.lines() {
            match l.split_once(' ') {
                Some(("pid", v)) => pid = v.trim().parse().ok(),
                Some(("started", v)) => started = v.trim().parse().unwrap_or(0),
                Some(("holder", v)) => what = v.trim().to_string(),
                Some(("token", v)) => token = v.trim().to_string(),
                _ => {}
            }
        }
        Some(Holder { pid: pid?, started, what, token })
    }
    pub fn describe(&self) -> String {
        let ago = now_secs().saturating_sub(self.started);
        format!("pid {} ({}), running for {}m{:02}s", self.pid, if self.what.is_empty() { "?" } else { &self.what }, ago / 60, ago % 60)
    }
}

pub fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

#[cfg(windows)]
mod sys {
    use std::ffi::c_void;
    #[repr(C)]
    #[derive(Default, Clone, Copy)]
    pub struct FileTime {
        pub lo: u32,
        pub hi: u32,
    }
    extern "system" {
        pub fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut c_void;
        pub fn CloseHandle(h: *mut c_void) -> i32;
        pub fn GetExitCodeProcess(h: *mut c_void, code: *mut u32) -> i32;
        pub fn GetProcessTimes(h: *mut c_void, c: *mut FileTime, e: *mut FileTime, k: *mut FileTime, u: *mut FileTime) -> i32;
    }
}

/// Is `pid` a running process that was already running at `started` (unix seconds)? A pid the system has
/// since handed to a newer process is not the holder: it started after the lock did.
///
/// The one `unsafe` in the CLI: three kernel32 calls (open, ask, close a process handle), because the standard
/// library cannot ask whether another process is alive and when it started, and Magician takes no crates. The handle
/// never leaves this function and the out-parameters are locals.
#[cfg(windows)]
#[allow(unsafe_code)]
pub fn holder_alive(pid: u32, started: u64) -> bool {
    use sys::*;
    // SAFETY: each call gets a handle it opened (checked for null) or pointers to live locals of the right type.
    unsafe {
        // PROCESS_QUERY_LIMITED_INFORMATION
        let h = OpenProcess(0x1000, 0, pid);
        if h.is_null() {
            // ERROR_ACCESS_DENIED means it exists but isn't ours; anything else (invalid parameter) means gone.
            return std::io::Error::last_os_error().raw_os_error() == Some(5);
        }
        let mut code = 0u32;
        let ok = GetExitCodeProcess(h, &mut code) != 0;
        let (mut c, mut e, mut k, mut u) = (FileTime::default(), FileTime::default(), FileTime::default(), FileTime::default());
        let timed = GetProcessTimes(h, &mut c, &mut e, &mut k, &mut u) != 0;
        CloseHandle(h);
        if !ok || code != 259 {
            return false; // 259 = STILL_ACTIVE
        }
        if timed && started > 0 {
            let ft = ((c.hi as u64) << 32) | c.lo as u64;
            let created = (ft / 10_000_000).saturating_sub(11_644_473_600);
            if created > started.saturating_add(2) {
                return false;
            }
        }
        true
    }
}

/// The same question through `/proc`: the process is there and not a zombie, and (when `started` is known) it was
/// created no later than the lock was taken. Its creation time is the boot time (`btime` in `/proc/stat`) plus field
/// 22 of `/proc/<pid>/stat`, in clock ticks of 1/100 s (USER_HZ, which Linux fixes at 100 for user space on the
/// machines the game runs on). Where there is no `/proc`, a process can't be judged dead, so it counts as alive.
#[cfg(not(windows))]
pub fn holder_alive(pid: u32, started: u64) -> bool {
    let dir = format!("/proc/{pid}");
    if !Path::new("/proc/self").exists() {
        return true;
    }
    let Ok(stat) = std::fs::read_to_string(format!("{dir}/stat")) else { return false };
    match proc_created(&stat, &std::fs::read_to_string("/proc/stat").unwrap_or_default()) {
        Some((state, _)) if state == 'Z' || state == 'X' => false,
        Some((_, Some(created))) if started > 0 => created <= started.saturating_add(2),
        _ => true,
    }
}

/// The state letter of a `/proc/<pid>/stat` line, and its creation time (unix seconds) when `/proc/stat` gives the boot.
#[cfg(not(windows))]
fn proc_created(stat: &str, proc_stat: &str) -> Option<(char, Option<u64>)> {
    // the name in brackets may hold spaces and brackets of its own; the fields after the last `)` do not
    let fields: Vec<&str> = stat.rsplit_once(')')?.1.split_whitespace().collect();
    let state = fields.first()?.chars().next()?;
    let btime = proc_stat.lines().find_map(|l| l.strip_prefix("btime ")).and_then(|v| v.trim().parse::<u64>().ok());
    let ticks = fields.get(19).and_then(|v| v.parse::<u64>().ok());
    Some((state, btime.zip(ticks).map(|(b, t)| b + t / 100)))
}

#[derive(Clone, Debug)]
pub struct Options {
    /// Fail at once when the lock is held.
    pub no_wait: bool,
    /// Give up after this long when waiting.
    pub timeout: Duration,
    pub poll: Duration,
    /// Print "waiting" this often.
    pub say_every: Duration,
}

impl Default for Options {
    fn default() -> Self {
        Options { no_wait: false, timeout: Duration::from_secs(30 * 60), poll: Duration::from_secs(1), say_every: Duration::from_secs(15) }
    }
}

/// Held until dropped. Dropping removes the lock file if it is still ours.
pub struct BuildLock {
    path: PathBuf,
    mine: Holder,
}

impl Drop for BuildLock {
    fn drop(&mut self) {
        if std::fs::read_to_string(&self.path).ok().and_then(|t| Holder::parse(&t)).as_ref() == Some(&self.mine) {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

/// Take the lock in `dir` (created if missing). `what` names the caller (`build maid_lash`).
/// Another live holder is a refusal (`Stop::Refused`, exit 3), with `--no-wait` at once or after the timeout.
pub fn acquire(dir: &Path, what: &str, opt: &Options, say: &mut dyn FnMut(&str)) -> Result<BuildLock, Stop> {
    acquire_file(dir, LOCK_FILE, what, opt, say)
}

/// Create `path` holding `h`, all at once: the text goes into a file of this holder's own, which is hard-linked to
/// `path` (that fails if `path` exists, as `create_new` does). Where the filesystem has no hard links, a
/// `create_new` and a write.
fn create_whole(path: &Path, h: &Holder) -> std::io::Result<()> {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let own = path.with_file_name(format!("{name}.{}", h.token));
    std::fs::write(&own, h.text())?;
    let linked = std::fs::hard_link(&own, path);
    let _ = std::fs::remove_file(&own);
    match linked {
        Err(e) if e.kind() != std::io::ErrorKind::AlreadyExists && e.kind() != std::io::ErrorKind::PermissionDenied => {
            use std::io::Write;
            let mut f = std::fs::OpenOptions::new().write(true).create_new(true).open(path)?;
            f.write_all(h.text().as_bytes())
        }
        other => other,
    }
}

/// `acquire` with a lock file of another name (the install ledger's, `install::LEDGER_LOCK`).
pub fn acquire_file(dir: &Path, file: &str, what: &str, opt: &Options, say: &mut dyn FnMut(&str)) -> Result<BuildLock, Stop> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let path = dir.join(file);
    static TAKEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.subsec_nanos());
    let token = format!("{}-{nanos}-{}", std::process::id(), TAKEN.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
    let mine = Holder { pid: std::process::id(), started: now_secs(), what: what.to_string(), token };
    let begun = Instant::now();
    let mut last_said: Option<Instant> = None;
    loop {
        match create_whole(&path, &mine) {
            Ok(()) => return Ok(BuildLock { path, mine }),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists || e.kind() == std::io::ErrorKind::PermissionDenied => {}
            Err(e) => return Err(format!("{}: {e}", path.display()).into()),
        }
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        match Holder::parse(&text) {
            Some(h) if holder_alive(h.pid, h.started) => {
                if opt.no_wait {
                    return Err(Stop::Refused(format!("{} is locked by {} (lock file {}); not waiting (--no-wait)", dir.display(), h.describe(), path.display())));
                }
                if begun.elapsed() >= opt.timeout {
                    return Err(Stop::Refused(format!(
                        "gave up after {}s waiting for {} (held by {}; lock file {})",
                        begun.elapsed().as_secs(),
                        dir.display(),
                        h.describe(),
                        path.display()
                    )));
                }
                if last_said.is_none_or(|t| t.elapsed() >= opt.say_every) {
                    say(&format!("{} is in use by {}; waiting for it to finish", dir.display(), h.describe()));
                    last_said = Some(Instant::now());
                }
                std::thread::sleep(opt.poll);
            }
            // Dead holder, or a lock file that is not a lock: clear it and take the lock. One that can't be read right
            // now (its holder is removing it; Windows keeps a file being deleted for a moment) or is empty is looked at
            // again: every lock is written whole, so only an old empty file (a crash mid-write, before locks were
            // written whole) is cleared.
            _ => {
                if text.is_empty() {
                    let old = std::fs::metadata(&path).and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|age| age > Duration::from_secs(10));
                    if !old {
                        std::thread::sleep(opt.poll.min(Duration::from_millis(50)));
                        continue;
                    }
                }
                say(&format!("clearing a stale lock in {} ({})", dir.display(), Holder::parse(&text).map_or("unreadable".into(), |h| h.describe())));
                // Rename first so that of several waiters exactly one clears it, then check it was the stale one.
                let grave = dir.join(format!("{file}.stale-{}", mine.token));
                if std::fs::rename(&path, &grave).is_ok() {
                    if std::fs::read_to_string(&grave).unwrap_or_default() != text {
                        // we grabbed a fresh lock by mistake: put it back, unless a lock is there again
                        let _ = std::fs::hard_link(&grave, &path);
                    }
                    let _ = std::fs::remove_file(&grave);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("magician_lock_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn quick() -> Options {
        Options { no_wait: false, timeout: Duration::from_secs(5), poll: Duration::from_millis(20), say_every: Duration::from_millis(100) }
    }

    #[test]
    fn a_second_taker_is_refused_with_no_wait_and_names_the_holder() {
        let d = scratch("held");
        let first = acquire(&d, "build maid_lash", &quick(), &mut |_| {}).unwrap();
        let e = acquire(&d, "release maid_lash", &Options { no_wait: true, ..quick() }, &mut |_| {}).err().unwrap();
        assert!(e.contains("locked by") && e.contains("build maid_lash") && e.contains(&format!("pid {}", std::process::id())), "{e}");
        drop(first);
        assert!(!d.join(LOCK_FILE).exists(), "dropping the lock removes the file");
        drop(acquire(&d, "again", &Options { no_wait: true, ..quick() }, &mut |_| {}).unwrap());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_waiter_says_so_then_takes_the_lock_when_it_is_released() {
        let d = scratch("wait");
        let first = acquire(&d, "build a", &quick(), &mut |_| {}).unwrap();
        let t = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(400));
            drop(first);
        });
        let mut said = Vec::new();
        let got = acquire(&d, "build b", &quick(), &mut |m| said.push(m.to_string())).unwrap();
        t.join().unwrap();
        assert!(said.iter().any(|m| m.contains("waiting for it to finish") && m.contains("build a")), "{said:?}");
        drop(got);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_wait_times_out() {
        let d = scratch("timeout");
        let _first = acquire(&d, "build a", &quick(), &mut |_| {}).unwrap();
        let e = acquire(&d, "build b", &Options { timeout: Duration::from_millis(200), ..quick() }, &mut |_| {}).err().unwrap();
        assert!(e.contains("gave up") && e.contains("build a"), "{e}");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_lock_whose_process_is_gone_is_cleared() {
        let d = scratch("stale");
        std::fs::create_dir_all(&d).unwrap();
        // A pid no process has: the holder is dead.
        let dead = Holder { pid: 0x7fff_fff0, started: now_secs() - 60, what: "build crashed".into(), token: String::new() };
        assert!(!holder_alive(dead.pid, dead.started));
        std::fs::write(d.join(LOCK_FILE), dead.text()).unwrap();
        let mut said = Vec::new();
        let got = acquire(&d, "build next", &Options { no_wait: true, ..quick() }, &mut |m| said.push(m.to_string())).unwrap();
        assert!(said.iter().any(|m| m.contains("stale") && m.contains("build crashed")), "{said:?}");
        assert!(std::fs::read_to_string(d.join(LOCK_FILE)).unwrap().contains("build next"));
        drop(got);
        // A garbage lock file is stale too.
        std::fs::write(d.join(LOCK_FILE), "not a lock").unwrap();
        drop(acquire(&d, "x", &Options { no_wait: true, ..quick() }, &mut |_| {}).unwrap());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn this_process_is_alive_and_a_pid_started_later_than_the_lock_is_not_the_holder() {
        assert!(holder_alive(std::process::id(), now_secs()));
        #[cfg(any(windows, target_os = "linux"))]
        assert!(!holder_alive(std::process::id(), 1_000_000), "a process created after the lock was taken is a reused pid");
        // a /proc stat line: the name may hold a space and a bracket; a zombie is not alive
        #[cfg(not(windows))]
        {
            let line = |state: &str| format!("4242 (build (x) y) {state} 1 4242 4242 0 -1 4194560 100 0 0 0 1 2 0 0 20 0 1 0 123456 1000 10\n");
            assert_eq!(proc_created(&line("S"), "cpu 1 2 3\nbtime 1700000000\n"), Some(('S', Some(1_700_000_000 + 1234))));
            assert_eq!(proc_created(&line("Z"), ""), Some(('Z', None)));
            assert_eq!(proc_created("garbage", ""), None);
        }
        // a lock file whose start time is the largest number it can hold: a live holder, not an overflow
        assert!(holder_alive(std::process::id(), u64::MAX));
        let h = Holder::parse("pid 1\nstarted 18446744073709551615\nholder x\n").unwrap();
        assert!(h.describe().contains("running for 0m00s"), "{}", h.describe());
    }

    #[test]
    fn one_holder_at_a_time_however_many_take_it_at_once() {
        // threads of one process: the same pid and second, so only the token tells their locks apart
        let d = scratch("many");
        let inside = std::sync::atomic::AtomicUsize::new(0);
        let most = std::sync::atomic::AtomicUsize::new(0);
        let opt = Options { no_wait: false, timeout: Duration::from_secs(60), poll: Duration::from_millis(1), say_every: Duration::from_secs(60) };
        std::thread::scope(|s| {
            for _ in 0..16 {
                s.spawn(|| {
                    for _ in 0..10 {
                        let _l = acquire(&d, "same name", &opt, &mut |_| {}).unwrap();
                        let now = inside.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
                        most.fetch_max(now, std::sync::atomic::Ordering::SeqCst);
                        std::thread::sleep(Duration::from_micros(200));
                        inside.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
                    }
                });
            }
        });
        assert_eq!(most.load(std::sync::atomic::Ordering::SeqCst), 1, "two holders at once");
        let left: Vec<_> = std::fs::read_dir(&d).unwrap().filter_map(|e| e.ok()).map(|e| e.file_name()).collect();
        assert!(left.is_empty(), "no lock or scratch file left: {left:?}");
        let _ = std::fs::remove_dir_all(&d);
    }
}
