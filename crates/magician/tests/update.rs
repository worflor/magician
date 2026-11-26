//! `magician update`, held against the built binary on scratch toolkit folders with a local folder (or a server on
//! this machine) as the source: nothing here touches the real toolkit, the real game or the network.
//!
//! A "release" is made here from a fake `magician` (a few lines of Rust, compiled with `rustc`) that only answers
//! `--version`, so a release can say any version and commit. The running updater is a copy of the real binary.

#![allow(clippy::unwrap_used, clippy::expect_used)] // a test that cannot unwrap has failed

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::{Arc, Mutex};

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("magician-update-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

const EXE: &str = if cfg!(windows) { "magician.exe" } else { "magician" };
const ASSET: &str = if cfg!(windows) { "magician-windows-x64.zip" } else { "magician-linux-x64.tar.gz" };

fn put(p: &Path, text: &str) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
}

fn tar() -> PathBuf {
    if cfg!(windows) {
        let p = PathBuf::from(std::env::var_os("SystemRoot").unwrap()).join("System32").join("tar.exe");
        if p.is_file() {
            return p;
        }
    }
    PathBuf::from("tar")
}

/// A program that says it is `magician <version> (git <commit>)` and nothing else.
fn fake_magician(d: &Path, version: &str, commit: &str) -> PathBuf {
    let tag = format!("{version}-{commit}").replace('.', "_").replace('-', "_");
    let src = d.join(format!("fake_{tag}.rs"));
    let out = d.join(format!("fake_{tag}{}", if cfg!(windows) { ".exe" } else { "" }));
    std::fs::write(&src, format!("fn main() {{ if std::env::args().any(|a| a == \"--version\") {{ println!(\"magician {version} (git {commit})\"); }} }}")).unwrap();
    let o = Command::new("rustc").args(["--edition", "2021", "-o"]).arg(&out).arg(&src).output().unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    out
}

/// What the real binary says about itself: the lines of its VERSION.
fn describe() -> String {
    let o = Command::new(env!("CARGO_BIN_EXE_magician")).args(["update", "--describe"]).output().unwrap();
    assert_eq!(o.status.code(), Some(0));
    String::from_utf8(o.stdout).unwrap()
}

fn with_key(text: &str, key: &str, value: &str) -> String {
    let mut done = false;
    let mut out: Vec<String> = text
        .lines()
        .map(|l| {
            if l.starts_with(&format!("{key} =")) {
                done = true;
                format!("{key} = {value}")
            } else {
                l.to_string()
            }
        })
        .collect();
    if !done {
        out.push(format!("{key} = {value}"));
    }
    out.join("\n") + "\n"
}

struct Release {
    version: String,
    commit: String,
    date: String,
    /// Overrides on the VERSION text, `(key, value)`.
    keys: Vec<(&'static str, String)>,
    /// Write a wrong checksum.
    bad_sum: bool,
    /// List no asset for this platform.
    no_asset_line: bool,
}

impl Release {
    fn new(version: &str, commit: &str) -> Release {
        Release { version: version.into(), commit: commit.into(), date: "2099-01-01".into(), keys: vec![], bad_sum: false, no_asset_line: false }
    }

    /// The source folder: the asset, SHA256SUMS, VERSION.
    fn write(&self, d: &Path, name: &str) -> PathBuf {
        let src = d.join(name);
        let stage = d.join(format!("{name}-stage"));
        std::fs::create_dir_all(&src).unwrap();
        std::fs::copy(fake_magician(d, &self.version, &self.commit), stage_path(&stage, EXE)).unwrap();
        put(&stage.join("ingame").join("mod_session.py"), "new session");
        put(&stage.join("docs").join("a.md"), "new doc");
        let asset = src.join(ASSET);
        let mut c = Command::new(tar());
        if ASSET.ends_with(".zip") {
            c.args(["-a", "-c", "-f"]);
        } else {
            c.args(["-czf"]);
        }
        let o = c.arg(&asset).arg("-C").arg(&stage).arg(".").output().unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        let mut sum = grade::sha256::hex(&std::fs::read(&asset).unwrap());
        if self.bad_sum {
            sum = "0".repeat(64);
        }
        let mut sums = format!("{}  VERSION\n", "1".repeat(64));
        if !self.no_asset_line {
            sums += &format!("{sum}  {ASSET}\n");
        }
        std::fs::write(src.join("SHA256SUMS"), sums).unwrap();
        let mut v = with_key(&describe(), "version", &self.version);
        v = with_key(&v, "commit", &self.commit);
        v = with_key(&v, "date", &self.date);
        // the commit time agrees with the date: midnight of it, as Unix seconds (2099-01-01 for every fixture)
        if self.date == "2099-01-01" {
            v = with_key(&v, "time", "4070908800");
        }
        for (k, val) in &self.keys {
            v = with_key(&v, k, val);
        }
        std::fs::write(src.join("VERSION"), v).unwrap();
        src
    }
}

fn stage_path(stage: &Path, name: &str) -> PathBuf {
    std::fs::create_dir_all(stage).unwrap();
    stage.join(name)
}

/// A release install: a copy of the real binary beside `ingame\`, `docs\` and a file of the user's own.
fn toolkit(d: &Path) -> PathBuf {
    let tk = d.join("toolkit");
    std::fs::create_dir_all(&tk).unwrap();
    std::fs::copy(env!("CARGO_BIN_EXE_magician"), tk.join(EXE)).unwrap();
    put(&tk.join("ingame").join("mod_session.py"), "old session");
    put(&tk.join("docs").join("a.md"), "old doc");
    put(&tk.join("mine.txt"), "the user's own");
    tk
}

fn magician(d: &Path, tk: &Path, source: &str, args: &[&str]) -> Output {
    Command::new(tk.join(EXE))
        .args(args)
        .env("MAGICIAN_HOME", tk)
        .env("MAGICIAN_UPDATE_SOURCE", source)
        .env("MAGICIAN_CONFIG", d.join("config.txt"))
        .env("MAGICIAN_GAME_LOCK", d.join("game.lock"))
        .env("MAGICIAN_REFERENCE", d.join("reference"))
        .env("DEADLOCK_GAME", d.join("no-game"))
        .env("MAGICIAN_NO_CACHE", "1")
        .env_remove("CARGO_TARGET_DIR")
        .output()
        .unwrap()
}

fn text(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned() + &String::from_utf8_lossy(&o.stderr)
}

fn out(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn files(root: &Path) -> Vec<(String, Vec<u8>)> {
    fn walk(base: &Path, d: &Path, out: &mut Vec<(String, Vec<u8>)>) {
        for e in std::fs::read_dir(d).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(base, &p, out);
            } else {
                out.push((p.strip_prefix(base).unwrap().to_string_lossy().replace('\\', "/"), std::fs::read(&p).unwrap()));
            }
        }
    }
    let mut v = Vec::new();
    walk(root, root, &mut v);
    v.sort();
    v
}

fn s(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

#[test]
fn check_says_current_behind_ahead_or_unknown_with_the_exit_code_for_each() {
    let d = scratch("check");
    let tk = toolkit(&d);
    // the release this build describes: current (exit 0)
    let same = d.join("same");
    std::fs::create_dir_all(&same).unwrap();
    std::fs::write(same.join("VERSION"), describe()).unwrap();
    let o = magician(&d, &tk, &s(&same), &["update", "--check", "--json"]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    assert!(out(&o).starts_with("{\"schema\":\"magician.update/1\",\"action\":\"check\",\"state\":\"current\""), "{}", out(&o));
    // a newer version: behind, exit 5, and the next step named
    let behind = Release::new("99.0.0", "aaaaaaaaaaaa").write(&d, "behind");
    let o = magician(&d, &tk, &s(&behind), &["update", "--check", "--json"]);
    assert_eq!(o.status.code(), Some(5), "{}", text(&o));
    let j = out(&o);
    assert!(j.contains("\"state\":\"behind\"") && j.contains("\"next\":\"magician update\"") && j.contains("\"latest\":{\"version\":\"99.0.0\""), "{j}");
    assert_eq!(j.lines().count(), 1, "one document: {j}");
    let o = magician(&d, &tk, &s(&behind), &["update", "--check"]);
    assert_eq!(o.status.code(), Some(5));
    assert!(out(&o).contains("magician update: an update is available") && out(&o).contains("next         magician update"), "{}", out(&o));
    // an older release than this build: ahead, exit 0
    let ahead = Release::new("0.0.1", "aaaaaaaaaaaa").write(&d, "ahead");
    let o = magician(&d, &tk, &s(&ahead), &["update", "--check", "--json"]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    assert!(out(&o).contains("\"state\":\"ahead\""), "{}", out(&o));
    // no release there: unknown, exit 3, and a document on stdout that parses as one line
    let o = magician(&d, &tk, &s(&d.join("nothing-here")), &["update", "--check", "--json"]);
    assert_eq!(o.status.code(), Some(3), "{}", text(&o));
    let j = out(&o);
    assert!(j.contains("\"state\":\"unknown\"") && j.contains("\"latest\":null") && j.contains("\"error\":\"") && j.contains("could not check"), "{j}");
    assert!(String::from_utf8_lossy(&o.stderr).contains("magician: could not check"), "{}", text(&o));
    // a VERSION that is not one
    put(&d.join("junk").join("VERSION"), "hello\n");
    let o = magician(&d, &tk, &s(&d.join("junk")), &["update", "--check"]);
    assert_eq!(o.status.code(), Some(3), "{}", text(&o));
    assert!(text(&o).contains("not a Magician VERSION file"), "{}", text(&o));
    // check changes nothing
    assert_eq!(std::fs::read_to_string(tk.join("docs").join("a.md")).unwrap(), "old doc");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_changed_rubric_or_rule_set_is_reported_as_changed_judgement() {
    let d = scratch("judgement");
    let tk = toolkit(&d);
    let mut r = Release::new("99.0.0", "aaaaaaaaaaaa");
    r.keys = vec![("rubric", "9.9.9".into()), ("lint_rules", "999".into()), ("lint_fingerprint", "0".repeat(64))];
    let src = r.write(&d, "changed");
    let o = magician(&d, &tk, &s(&src), &["update", "--check", "--json"]);
    let j = out(&o);
    assert_eq!(o.status.code(), Some(5), "{}", text(&o));
    assert!(j.contains("\"judgement\":{\"changed\":true,\"differences\":[\"the grading rubric is 9.9.9") && j.contains("\"rerun\":[\"lint\",\"grade\"]"), "{j}");
    let o = magician(&d, &tk, &s(&src), &["update", "--check"]);
    let t = out(&o);
    assert!(t.contains("judgement    would change: the grading rubric is 9.9.9") && t.contains("the lint rule set is 999 rules") && t.contains("re-run `magician lint` and `magician grade`"), "{t}");
    // the same judgement: said unchanged
    let same = Release::new("99.0.0", "aaaaaaaaaaaa").write(&d, "same-judgement");
    let o = magician(&d, &tk, &s(&same), &["update", "--check", "--json"]);
    assert!(out(&o).contains("\"judgement\":{\"changed\":false,\"differences\":[],\"rerun\":[]}"), "{}", out(&o));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn an_update_swaps_the_toolkit_in_and_keeps_what_is_not_the_releases() {
    let d = scratch("apply");
    let tk = toolkit(&d);
    let src = Release::new("99.0.0", "aaaaaaaaaaaa").write(&d, "src");
    let before = files(&tk);
    // --dry-run says what it would do and changes nothing, not even beside the toolkit
    let o = magician(&d, &tk, &s(&src), &["update", "--dry-run"]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    assert!(out(&o).contains("would update") && out(&o).contains("would        fetch"), "{}", out(&o));
    assert_eq!(files(&tk), before);
    assert!(!d.join("toolkit.update").exists());
    let o = magician(&d, &tk, &s(&src), &["update", "--dry-run", "--json"]);
    assert!(out(&o).contains("\"action\":\"update\",\"state\":\"behind\"") && out(&o).contains("\"outcome\":\"would-update\"") && out(&o).contains("\"dry_run\":true"), "{}", out(&o));
    // the update
    let o = magician(&d, &tk, &s(&src), &["update", "--json"]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    let j = out(&o);
    assert!(j.contains("\"outcome\":\"updated\"") && j.contains("\"install\":\"release\"") && j.contains("\"previous\":"), "{j}");
    assert_eq!(std::fs::read_to_string(tk.join("docs").join("a.md")).unwrap(), "new doc");
    assert_eq!(std::fs::read_to_string(tk.join("ingame").join("mod_session.py")).unwrap(), "new session");
    assert_eq!(std::fs::read_to_string(tk.join("mine.txt")).unwrap(), "the user's own");
    let v = Command::new(tk.join(EXE)).arg("--version").output().unwrap();
    assert_eq!(String::from_utf8_lossy(&v.stdout).trim(), "magician 99.0.0 (git aaaaaaaaaaaa)", "the program is the release's");
    assert!(!d.join("toolkit.update").exists(), "the staging folder is gone");
    assert!(!tk.join(".magician-previous.partial").exists());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn an_update_report_names_old_and_new_and_the_judgement() {
    let d = scratch("report");
    let tk = toolkit(&d);
    let mut r = Release::new("99.0.0", "bbbbbbbbbbbb");
    r.keys = vec![("rubric", "9.9.9".into())];
    let src = r.write(&d, "src");
    let o = magician(&d, &tk, &s(&src), &["update"]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    let t = out(&o);
    assert!(t.contains("magician update: updated") && t.contains("-> 99.0.0 (git bbbbbbbbbbbb") && t.contains("judgement    changed: the grading rubric is 9.9.9"), "{t}");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn nothing_to_do_when_current_or_ahead() {
    let d = scratch("nothing");
    let tk = toolkit(&d);
    let ahead = Release::new("0.0.1", "aaaaaaaaaaaa").write(&d, "ahead");
    let before = files(&tk);
    let o = magician(&d, &tk, &s(&ahead), &["update"]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    assert!(out(&o).contains("already current"), "{}", out(&o));
    assert_eq!(files(&tk), before);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_bad_checksum_is_refused_and_nothing_changes() {
    let d = scratch("badsum");
    let tk = toolkit(&d);
    let mut r = Release::new("99.0.0", "aaaaaaaaaaaa");
    r.bad_sum = true;
    let src = r.write(&d, "src");
    let before = files(&tk);
    let o = magician(&d, &tk, &s(&src), &["update"]);
    assert_eq!(o.status.code(), Some(3), "{}", text(&o));
    assert!(text(&o).contains("checksum") && text(&o).contains("does not match") && text(&o).contains("nothing was changed"), "{}", text(&o));
    assert_eq!(files(&tk), before, "the toolkit is untouched");
    assert!(!d.join("toolkit.update").exists() && !tk.join(".magician-previous.partial").exists());
    // with --json the refusal is the one document, magician.error/1
    let o = magician(&d, &tk, &s(&src), &["update", "--json"]);
    assert!(out(&o).starts_with("{\"schema\":\"magician.error/1\",\"command\":\"update\",\"exit\":3,\"kind\":\"refused\""), "{}", out(&o));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_release_that_does_not_match_its_description_is_refused() {
    let d = scratch("mismatch");
    let tk = toolkit(&d);
    let src = Release::new("99.0.0", "aaaaaaaaaaaa").write(&d, "src");
    // VERSION claims another commit than the program in the zip reports
    let v = std::fs::read_to_string(src.join("VERSION")).unwrap();
    std::fs::write(src.join("VERSION"), with_key(&v, "commit", "cccccccccccc")).unwrap();
    let before = files(&tk);
    let o = magician(&d, &tk, &s(&src), &["update"]);
    assert_eq!(o.status.code(), Some(3), "{}", text(&o));
    assert!(text(&o).contains("does not match its own description"), "{}", text(&o));
    assert_eq!(files(&tk), before);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_source_with_no_build_for_this_platform_refuses_the_update_but_still_checks() {
    let d = scratch("noasset");
    let tk = toolkit(&d);
    let mut r = Release::new("99.0.0", "aaaaaaaaaaaa");
    r.no_asset_line = true;
    let src = r.write(&d, "src");
    let before = files(&tk);
    let o = magician(&d, &tk, &s(&src), &["update"]);
    assert_eq!(o.status.code(), Some(3), "{}", text(&o));
    assert!(text(&o).contains("could not update: no build for ") && text(&o).contains("build from source"), "{}", text(&o));
    assert_eq!(files(&tk), before);
    let o = magician(&d, &tk, &s(&src), &["update", "--check"]);
    assert_eq!(o.status.code(), Some(5), "--check only reads VERSION: {}", text(&o));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_game_session_holding_the_seat_refuses_the_update() {
    let d = scratch("seat");
    let tk = toolkit(&d);
    let src = Release::new("99.0.0", "aaaaaaaaaaaa").write(&d, "src");
    std::fs::write(d.join("game.lock"), "{\"holder\":\"lash-fix\",\"what\":\"angles\",\"since\":1}").unwrap();
    let before = files(&tk);
    let o = magician(&d, &tk, &s(&src), &["update"]);
    assert_eq!(o.status.code(), Some(3), "{}", text(&o));
    assert!(text(&o).contains("a game session holds the seat ('lash-fix'"), "{}", text(&o));
    assert_eq!(files(&tk), before);
    // --check is read-only and does not care
    assert_eq!(magician(&d, &tk, &s(&src), &["update", "--check"]).status.code(), Some(5));
    let _ = std::fs::remove_dir_all(&d);
}

/// Windows will not rename a folder with a file open inside it: a swap that fails for real, halfway.
#[cfg(windows)]
#[test]
fn a_swap_that_fails_for_real_is_rolled_back_completely() {
    let d = scratch("lockedswap");
    let tk = toolkit(&d);
    let src = Release::new("99.0.0", "aaaaaaaaaaaa").write(&d, "src");
    let before = files(&tk);
    // `docs` swaps first (names sort), `ingame` after it: hold a file open in `ingame` so its rename fails
    use std::os::windows::fs::OpenOptionsExt;
    let held = std::fs::OpenOptions::new().read(true).share_mode(1).open(tk.join("ingame").join("mod_session.py")).unwrap();
    let o = magician(&d, &tk, &s(&src), &["update"]);
    drop(held);
    assert_eq!(o.status.code(), Some(1), "{}", text(&o));
    assert!(text(&o).contains("the update failed") && text(&o).contains("is back as it was"), "{}", text(&o));
    assert_eq!(files(&tk), before, "every entry is as it was");
    assert!(!tk.join(".magician-previous.partial").exists() && !d.join("toolkit.update").exists());
    let v = Command::new(tk.join(EXE)).arg("--version").output().unwrap();
    assert!(String::from_utf8_lossy(&v.stdout).contains(env!("CARGO_PKG_VERSION")), "the program is still this one");
    let _ = std::fs::remove_dir_all(&d);
}

// ------------------------------------------------------------------------------------------------- git checkouts

fn git(dir: &Path, args: &[&str]) -> String {
    let o = Command::new("git").arg("-C").arg(dir).args(["-c", "user.name=t", "-c", "user.email=t@example.org", "-c", "commit.gpgsign=false"]).args(args).output().unwrap();
    assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8_lossy(&o.stdout).trim().to_string()
}

/// An origin and a clone of it: a tiny crate named `magician` that prints a version, and `ingame/mod_session.py`.
/// The clone's `target/release/magician` is the real binary, so it is the running program the update replaces.
fn checkout(d: &Path) -> (PathBuf, PathBuf) {
    let seed = d.join("seed");
    put(&seed.join("Cargo.toml"), "[package]\nname = \"magician\"\nversion = \"0.0.1\"\nedition = \"2021\"\n[workspace]\n");
    put(&seed.join("src").join("main.rs"), "fn main() { println!(\"magician 1 (git one)\"); }\n");
    put(&seed.join("ingame").join("mod_session.py"), "marker");
    put(&seed.join(".gitignore"), "target/\n");
    git(d, &["init", "-q", "-b", "main", "seed"]);
    git(&seed, &["add", "."]);
    git(&seed, &["commit", "-q", "-m", "one"]);
    let origin = d.join("origin.git");
    git(d, &["clone", "-q", "--bare", &s(&seed), &s(&origin)]);
    let tk = d.join("toolkit");
    git(d, &["clone", "-q", &s(&origin), &s(&tk)]);
    let rel = tk.join("target").join("release");
    std::fs::create_dir_all(&rel).unwrap();
    std::fs::copy(env!("CARGO_BIN_EXE_magician"), rel.join(EXE)).unwrap();
    (seed, tk)
}

fn run_checkout(d: &Path, tk: &Path, args: &[&str]) -> Output {
    Command::new(tk.join("target").join("release").join(EXE))
        .args(args)
        .env("MAGICIAN_HOME", tk)
        .env("MAGICIAN_CONFIG", d.join("config.txt"))
        .env("MAGICIAN_GAME_LOCK", d.join("game.lock"))
        .env("MAGICIAN_REFERENCE", d.join("reference"))
        .env_remove("CARGO_TARGET_DIR")
        .env_remove("RUSTFLAGS")
        .output()
        .unwrap()
}

fn push_commit(seed: &Path, main_rs: &str, msg: &str) {
    put(&seed.join("src").join("main.rs"), main_rs);
    git(seed, &["commit", "-q", "-am", msg]);
    git(seed, &["push", "-q", &s(&seed.parent().unwrap().join("origin.git")), "main"]);
}

#[test]
fn a_checkout_fast_forwards_and_rebuilds_and_replaces_the_running_program() {
    let d = scratch("co-ok");
    let (seed, tk) = checkout(&d);
    let old = git(&tk, &["rev-parse", "HEAD"]);
    push_commit(&seed, "fn main() { println!(\"magician 2 (git two)\"); }\n", "two");
    let new = git(&seed, &["rev-parse", "HEAD"]);
    let o = run_checkout(&d, &tk, &["update", "--dry-run", "--json"]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    assert!(out(&o).contains("\"install\":\"checkout\"") && out(&o).contains("\"outcome\":\"would-update\""), "{}", out(&o));
    assert_eq!(git(&tk, &["rev-parse", "HEAD"]), old, "a dry run moves nothing");
    let o = run_checkout(&d, &tk, &["update", "--json"]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    assert!(out(&o).contains("\"outcome\":\"updated\"") && out(&o).contains(&format!("\"to\":\"{}\"", &new[..12])), "{}", out(&o));
    assert_eq!(git(&tk, &["rev-parse", "HEAD"]), new);
    let v = Command::new(tk.join("target").join("release").join(EXE)).output().unwrap();
    assert!(String::from_utf8_lossy(&v.stdout).contains("magician 2 (git two)"), "rebuilt: {}", String::from_utf8_lossy(&v.stdout));
    // already level now (the fake program does not know `update`, so ask with the old one kept beside it)
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_checkout_whose_build_fails_goes_back_to_where_it_was() {
    let d = scratch("co-fail");
    let (seed, tk) = checkout(&d);
    let old = git(&tk, &["rev-parse", "HEAD"]);
    push_commit(&seed, "fn main() { this does not compile }\n", "broken");
    let o = run_checkout(&d, &tk, &["update"]);
    assert_eq!(o.status.code(), Some(1), "{}", text(&o));
    assert!(text(&o).contains("rolled back") && text(&o).contains("cargo build --release failed"), "{}", text(&o));
    assert_eq!(git(&tk, &["rev-parse", "HEAD"]), old, "the tree is back");
    assert_eq!(git(&tk, &["status", "--porcelain", "--untracked-files=no"]), "");
    // the program that was running is back and is still the real one
    let v = Command::new(tk.join("target").join("release").join(EXE)).arg("--version").output().unwrap();
    assert!(String::from_utf8_lossy(&v.stdout).contains(env!("CARGO_PKG_VERSION")), "{}", String::from_utf8_lossy(&v.stdout));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_dirty_checkout_is_refused_and_so_is_one_that_cannot_fast_forward() {
    let d = scratch("co-refuse");
    let (seed, tk) = checkout(&d);
    push_commit(&seed, "fn main() { println!(\"magician 2 (git two)\"); }\n", "two");
    let old = git(&tk, &["rev-parse", "HEAD"]);
    // local changes to a tracked file
    put(&tk.join("ingame").join("mod_session.py"), "edited by hand");
    let o = run_checkout(&d, &tk, &["update"]);
    assert_eq!(o.status.code(), Some(3), "{}", text(&o));
    assert!(text(&o).contains("has local changes (1 file)") && text(&o).contains("nothing was changed"), "{}", text(&o));
    assert_eq!(git(&tk, &["rev-parse", "HEAD"]), old);
    assert_eq!(std::fs::read_to_string(tk.join("ingame").join("mod_session.py")).unwrap(), "edited by hand", "the edit is still there");
    let o = run_checkout(&d, &tk, &["update", "--json"]);
    assert!(out(&o).starts_with("{\"schema\":\"magician.error/1\",\"command\":\"update\",\"exit\":3,\"kind\":\"refused\""), "{}", out(&o));
    // a local commit the upstream does not have, and an upstream one it does not: diverged
    git(&tk, &["checkout", "-q", "--", "."]);
    put(&tk.join("local.txt"), "mine");
    git(&tk, &["add", "local.txt"]);
    git(&tk, &["commit", "-q", "-m", "local"]);
    let o = run_checkout(&d, &tk, &["update"]);
    assert_eq!(o.status.code(), Some(3), "{}", text(&o));
    assert!(text(&o).contains("cannot fast-forward") && text(&o).contains("1 commit(s)"), "{}", text(&o));
    // no upstream at all
    git(&tk, &["branch", "-q", "--unset-upstream"]);
    let o = run_checkout(&d, &tk, &["update"]);
    assert_eq!(o.status.code(), Some(3), "{}", text(&o));
    assert!(text(&o).contains("follows no upstream"), "{}", text(&o));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_checkout_level_with_its_upstream_is_current() {
    let d = scratch("co-level");
    let (_seed, tk) = checkout(&d);
    let o = run_checkout(&d, &tk, &["update"]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    assert!(out(&o).contains("already current"), "{}", out(&o));
    let _ = std::fs::remove_dir_all(&d);
}

// ---------------------------------------------------------------------------------------------------- the network

/// A server on this machine: records each request's head, answers `VERSION` with `body` (or hangs when `body` is
/// `None`, to hold a client past its time limit).
fn server(body: Option<String>) -> (String, Arc<Mutex<Vec<String>>>) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://127.0.0.1:{}/releases", l.local_addr().unwrap().port());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    std::thread::spawn(move || {
        for stream in l.incoming() {
            let Ok(mut st) = stream else { continue };
            let log = log.clone();
            let body = body.clone();
            std::thread::spawn(move || {
                let mut buf = [0u8; 4096];
                let n = st.read(&mut buf).unwrap_or(0);
                log.lock().unwrap().push(String::from_utf8_lossy(&buf[..n]).into_owned());
                match body {
                    Some(b) => {
                        let _ = st.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{b}", b.len()).as_bytes());
                    }
                    None => std::thread::sleep(std::time::Duration::from_secs(30)),
                }
            });
        }
    });
    (url, seen)
}

#[test]
fn a_url_source_is_fetched_with_curl_sending_only_a_plain_agent() {
    let d = scratch("url");
    let tk = toolkit(&d);
    let (url, seen) = server(Some(with_key(&describe(), "version", "99.0.0")));
    let o = magician(&d, &tk, &url, &["update", "--check", "--json"]);
    assert_eq!(o.status.code(), Some(5), "{}", text(&o));
    let reqs = seen.lock().unwrap().clone();
    assert_eq!(reqs.len(), 1, "{reqs:?}");
    let head = reqs[0].to_ascii_lowercase();
    assert!(head.starts_with("get /releases/version http/"), "{head}");
    assert!(head.contains(&format!("user-agent: magician/{}\r\n", env!("CARGO_PKG_VERSION"))), "{head}");
    for header in head.lines().skip(1).filter(|l| !l.is_empty()).map(|l| l.split(':').next().unwrap().to_string()) {
        assert!(["host", "user-agent", "accept"].contains(&header.as_str()), "a header that says more than it must: {header}");
    }
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_server_that_never_answers_is_could_not_check_not_a_hang() {
    let d = scratch("stall");
    let tk = toolkit(&d);
    let (url, _) = server(None);
    let t0 = std::time::Instant::now();
    let o = Command::new(tk.join(EXE))
        .args(["update", "--check", "--json"])
        .env("MAGICIAN_HOME", &tk)
        .env("MAGICIAN_UPDATE_SOURCE", &url)
        .env("MAGICIAN_UPDATE_TIMEOUT", "2")
        .env("MAGICIAN_CONFIG", d.join("config.txt"))
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(3), "{}", text(&o));
    assert!(t0.elapsed().as_secs() < 8, "took {:?}", t0.elapsed());
    assert!(out(&o).contains("\"state\":\"unknown\"") && text(&o).contains("could not check"), "{}", text(&o));
    // and a port nobody listens on, and an address that is not https
    let o = magician(&d, &tk, "http://127.0.0.1:9/x", &["update", "--check"]);
    assert_eq!(o.status.code(), Some(3), "{}", text(&o));
    let o = magician(&d, &tk, "http://updates.example.org/x", &["update", "--check"]);
    assert_eq!(o.status.code(), Some(3), "{}", text(&o));
    assert!(text(&o).contains("must come over https"), "{}", text(&o));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn only_update_and_doctor_updates_touch_the_network() {
    let d = scratch("offline");
    let tk = toolkit(&d);
    let (url, seen) = server(Some(describe()));
    for args in [&["doctor", "--json"][..], &["paths"], &["rules"], &["--version"], &["help"], &["update", "--describe"], &["update", "--help"], &["lint", "x.vpk"]] {
        let _ = magician(&d, &tk, &url, args);
    }
    assert!(seen.lock().unwrap().is_empty(), "something went online: {:?}", seen.lock().unwrap());
    // doctor with the flag asks once, and an unreadable source is a note
    let o = magician(&d, &tk, &url, &["doctor", "--updates", "--json"]);
    assert_eq!(seen.lock().unwrap().len(), 1, "{}", text(&o));
    assert!(out(&o).contains("{\"name\":\"update\",\"state\":\"ok\""), "{}", out(&o));
    let o = magician(&d, &tk, &s(&d.join("nope")), &["doctor", "--updates", "--json"]);
    assert!(out(&o).contains("{\"name\":\"update\",\"state\":\"skip\",\"detail\":\"could not check"), "{}", out(&o));
    assert!(out(&o).contains("{\"name\":\"toolchain\",\"state\":\"ok\""), "doctor names the toolchain: {}", out(&o));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn update_is_in_the_help_the_exit_codes_and_the_options_it_takes() {
    let d = scratch("help");
    let tk = toolkit(&d);
    let o = magician(&d, &tk, "x", &["help", "update"]);
    let t = out(&o);
    assert_eq!(o.status.code(), Some(0));
    assert!(t.contains("magician update --check") && t.contains("magician update [--dry-run]") && t.contains("magician update --describe") && !t.contains("magician lint"), "{t}");
    assert!(t.contains("5 an update is available"), "the exit code is documented: {t}");
    for args in [&["update", "--chek"][..], &["update", "extra"], &["update", "--check", "--dry-run"], &["update", "--source"]] {
        assert_eq!(magician(&d, &tk, "x", args).status.code(), Some(2), "{args:?}");
    }
    let o = magician(&d, &tk, "x", &["update", "--describe", "--json"]);
    assert!(out(&o).starts_with("{\"schema\":\"magician.update/1\",\"action\":\"describe\",\"current\":{\"version\":"), "{}", out(&o));
    let o = magician(&d, &tk, "x", &["update", "--describe"]);
    let t = out(&o);
    assert!(t.contains("version = ") && t.contains("commit = ") && t.contains("rubric = ") && t.contains("lint_fingerprint = "), "{t}");
    let _ = std::fs::remove_dir_all(&d);
}
