//! `magician update`: finding out whether there is a newer Magician, and (in `updateapply.rs`) installing it.
//!
//! Nothing here runs unless a person or a script ran `magician update ...`, or passed `doctor --updates`. No other
//! command checks, phones home or prints a nag, and Magician never fetches and runs new code of itself unasked.
//!
//! A release is a few files at one location, the update source: one asset per platform, and the two shared files:
//!
//! | File | Holds |
//! |---|---|
//! | `magician-windows-x64.zip` | the toolkit folder: `magician.exe`, `ingame\`, `tools\`, `mcp\`, `skills\`, `docs\`, the README, the changelog and the licence |
//! | `magician-linux-x64.tar.gz` | the same folder for Linux (`magician`, not `.exe`) |
//! | `SHA256SUMS` | the SHA-256 of every asset, one `<hex>  <name>` line each (the `sha256sum` form) |
//! | `VERSION` | what the release is, in the flat `key = value` form of the config file (below) |
//!
//! `VERSION` carries the release's `version`, `commit` and `date`, and the fingerprints of what does the judging: the
//! grading rubric's rules version (`rubric`), the fingerprint of the built-in policy (`policy`), and the lint rule set
//! (`lint_rules`, the count, and `lint_fingerprint`, a SHA-256 over every rule's code, name and severity). Comparing
//! those with the running build's is how `--check` says whether updating would change what a grade or a lint says.
//! `magician update --describe` prints the `VERSION` of the running build; the release script writes it from there.
//!
//! The source is a folder (local or on the network) or an https URL; see `paths::update_source`. A folder is copied,
//! a URL is fetched with `curl`; `update` picks the asset for the platform it runs on, bounded in time, sending nothing but the request and a `magician/<version>` agent.

use super::*;
use std::process::{Command, Stdio};

/// The release asset for a platform (`std::env::consts` names): `magician-windows-x64.zip`,
/// `magician-linux-x64.tar.gz`. `None` for a platform no release is built for. `tar -xf` reads both.
pub fn asset_for(os: &str, arch: &str) -> Option<String> {
    let arch = match arch {
        "x86_64" => "x64",
        _ => return None,
    };
    match os {
        "windows" => Some(format!("magician-windows-{arch}.zip")),
        "linux" => Some(format!("magician-linux-{arch}.tar.gz")),
        _ => None,
    }
}

/// The asset for the platform this is running on.
pub fn asset() -> Option<String> {
    asset_for(std::env::consts::OS, std::env::consts::ARCH)
}

/// `windows-x86_64`: the platform as a person would name it in a refusal.
pub fn platform() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

/// The SHA-256 a `SHA256SUMS` text lists for `name` (the `sha256sum` form, `*` before a binary name allowed).
pub fn checksum_of(sums: &str, name: &str) -> Option<String> {
    sums.lines().filter_map(|l| l.split_once(char::is_whitespace)).find(|(_, n)| n.trim().trim_start_matches('*') == name).map(|(h, _)| h.trim().to_ascii_lowercase())
}
/// Its checksums.
pub const SUMS: &str = "SHA256SUMS";
/// The release's description.
pub const VERSION_FILE: &str = "VERSION";

/// What a build or a release says about itself. The judgement fields are `None` for a `VERSION` that does not carry
/// them, which is said, never read as "unchanged".
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Release {
    pub version: String,
    pub commit: String,
    pub date: String,
    pub rubric: Option<String>,
    pub policy: Option<String>,
    pub lint_rules: Option<String>,
    pub lint_fingerprint: Option<String>,
}

impl Release {
    /// The running build.
    pub fn this_build() -> Release {
        let mut rules: String = lint::RULES.iter().map(|r| format!("{} {} {}\n", r.code, r.name, jsonout::severity(r.severity))).collect();
        rules.push_str(&format!("judgement {}\n", lint::JUDGEMENT));
        Release {
            version: env!("CARGO_PKG_VERSION").into(),
            commit: env!("MAGICIAN_GIT").into(),
            date: env!("MAGICIAN_GIT_DATE").into(),
            rubric: Some(grade::rubric::RULES_VERSION.into()),
            policy: Some(grade::Policy::default().fingerprint()),
            lint_rules: Some(lint::RULES.len().to_string()),
            lint_fingerprint: Some(grade::sha256::hex(rules.as_bytes())),
        }
    }

    /// The `VERSION` file's text.
    pub fn text(&self) -> String {
        let mut t = String::from("# a Magician release: what it is, and the fingerprints of what does the judging\n");
        let mut line = |k: &str, v: &str| t.push_str(&format!("{k} = {v}\n"));
        line("version", &self.version);
        line("commit", &self.commit);
        line("date", &self.date);
        for (k, v) in [("rubric", &self.rubric), ("policy", &self.policy), ("lint_rules", &self.lint_rules), ("lint_fingerprint", &self.lint_fingerprint)] {
            if let Some(v) = v {
                line(k, v);
            }
        }
        t
    }

    /// Read a `VERSION` file. `version` and `commit` must be there; the judgement fingerprints may be left out.
    pub fn parse(text: &str) -> Result<Release, String> {
        let get = |k: &str| paths::config_lookup(text, k);
        let need = |k: &str| get(k).ok_or_else(|| format!("no `{k} =` line: not a Magician VERSION file"));
        Ok(Release {
            version: need("version")?,
            commit: need("commit")?,
            date: get("date").unwrap_or_else(|| "unknown".into()),
            rubric: get("rubric"),
            policy: get("policy"),
            lint_rules: get("lint_rules"),
            lint_fingerprint: get("lint_fingerprint"),
        })
    }

    pub fn json(&self) -> String {
        let o = |v: &Option<String>| v.as_deref().map_or("null".to_string(), jsonout::s);
        jsonout::Obj::new()
            .str("version", &self.version)
            .str("commit", &self.commit)
            .str("date", &self.date)
            .raw("rubric", o(&self.rubric))
            .raw("policy", o(&self.policy))
            .raw("lint_rules", o(&self.lint_rules))
            .raw("lint_fingerprint", o(&self.lint_fingerprint))
            .build()
    }

    /// `0.1.0 (git 1a2b3c4d5e6f, 2026-11-25)`.
    pub fn label(&self) -> String {
        format!("{} (git {}, {})", self.version, self.commit, self.date)
    }
}

// ------------------------------------------------------------------------------------------------- comparing

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    /// The same release.
    Current,
    /// The latest release is newer: an update is available.
    Behind,
    /// This build is newer than the latest release (a checkout on its main branch, say).
    Ahead,
    /// The source could not be read.
    Unknown,
}

impl State {
    pub fn id(self) -> &'static str {
        match self {
            State::Current => "current",
            State::Behind => "behind",
            State::Ahead => "ahead",
            State::Unknown => "unknown",
        }
    }
}

fn numbers(v: &str) -> Option<Vec<u64>> {
    v.trim().split('.').map(|p| p.chars().take_while(char::is_ascii_digit).collect::<String>().parse().ok()).collect()
}

/// The order of two version strings (`0.1.0`, `1.2`), `None` when either does not read as numbers.
fn version_order(a: &str, b: &str) -> Option<std::cmp::Ordering> {
    let (mut x, mut y) = (numbers(a)?, numbers(b)?);
    let n = x.len().max(y.len());
    x.resize(n, 0);
    y.resize(n, 0);
    Some(x.cmp(&y))
}

fn same_commit(a: &str, b: &str) -> bool {
    let (a, b) = (a.trim_end_matches("-dirty"), b.trim_end_matches("-dirty"));
    a != "unknown" && b != "unknown" && !a.is_empty() && !b.is_empty() && (a.starts_with(b) || b.starts_with(a))
}

/// Where this build stands against the latest release, and why: the version first, then the commit. Two commits of
/// one version are ordered by their dates (a release's `date` is its commit's); on the same day, or with a date that
/// is not known, a different commit counts as an update, since the order can't be told.
pub fn compare(local: &Release, latest: &Release) -> (State, String) {
    use std::cmp::Ordering::{Equal, Greater, Less};
    match version_order(&local.version, &latest.version) {
        Some(Less) => return (State::Behind, format!("version {} is older than the latest release's {}", local.version, latest.version)),
        Some(Greater) => return (State::Ahead, format!("version {} is newer than the latest release's {}", local.version, latest.version)),
        Some(Equal) => {}
        None if local.version != latest.version => return (State::Behind, format!("version {} is not the latest release's {} (they do not compare as numbers)", local.version, latest.version)),
        None => {}
    }
    if same_commit(&local.commit, &latest.commit) {
        let note = if local.commit.ends_with("-dirty") { ", built from a tree with local changes" } else { "" };
        return (State::Current, format!("the latest release, commit {}{note}", latest.commit));
    }
    let day = |d: &str| d.len() == 10 && d.as_bytes()[4] == b'-' && d.as_bytes()[7] == b'-';
    if day(&local.date) && day(&latest.date) {
        match latest.date.as_str().cmp(local.date.as_str()) {
            Greater => return (State::Behind, format!("commit {} ({}) is older than the latest release's {} ({})", local.commit, local.date, latest.commit, latest.date)),
            Less => return (State::Ahead, format!("commit {} ({}) is newer than the latest release's {} ({})", local.commit, local.date, latest.commit, latest.date)),
            Equal => {}
        }
    }
    (State::Behind, format!("commit {} is not the latest release's {} (same version, and their dates do not order them)", local.commit, latest.commit))
}

/// What differs between this build's judgement and a release's.
#[derive(Clone, Debug, PartialEq)]
pub struct Judgement {
    /// `None` when the release does not say (its `VERSION` carries none of the fingerprints).
    pub changed: Option<bool>,
    pub differences: Vec<String>,
    /// What to run again after updating.
    pub rerun: Vec<&'static str>,
}

pub fn judgement(local: &Release, latest: &Release) -> Judgement {
    let mut differences = Vec::new();
    let (mut grade_moved, mut lint_moved) = (false, false);
    let mut said = 0;
    if let (Some(a), Some(b)) = (&local.rubric, &latest.rubric) {
        said += 1;
        if a != b {
            grade_moved = true;
            differences.push(format!("the grading rubric is {b} (this build: {a})"));
        }
    }
    if let (Some(a), Some(b)) = (&local.policy, &latest.policy) {
        said += 1;
        if a != b {
            grade_moved = true;
            differences.push(format!("the built-in policy differs (fingerprint {}, this build: {})", &b[..b.len().min(12)], &a[..a.len().min(12)]));
        }
    }
    if let (Some(a), Some(b)) = (&local.lint_rules, &latest.lint_rules) {
        said += 1;
        if a != b {
            lint_moved = true;
            differences.push(format!("the lint rule set is {b} rules (this build: {a})"));
        }
    }
    if let (Some(a), Some(b)) = (&local.lint_fingerprint, &latest.lint_fingerprint) {
        said += 1;
        if a != b && !lint_moved {
            lint_moved = true;
            differences.push("the lint rules differ (the same count, other codes, names or severities)".to_string());
        }
    }
    let mut rerun = Vec::new();
    if lint_moved {
        rerun.push("lint");
    }
    if grade_moved || lint_moved {
        rerun.push("grade");
    }
    Judgement { changed: if said == 0 { None } else { Some(!differences.is_empty()) }, differences, rerun }
}

impl Judgement {
    pub fn json(&self) -> String {
        jsonout::Obj::new()
            .raw("changed", self.changed.map_or("null".into(), |c| c.to_string()))
            .strs("differences", &self.differences)
            .strs("rerun", &self.rerun)
            .build()
    }

    /// The line a person reads. `future` is whether the update has not been applied yet.
    pub fn line(&self, future: bool) -> String {
        match self.changed {
            None => "the release does not say what it judges by (its VERSION carries no fingerprints)".to_string(),
            Some(false) => "unchanged: the same grading rubric, built-in policy and lint rules".to_string(),
            Some(true) => format!(
                "{}: {}; re-run {} on your mods ({})",
                if future { "would change" } else { "changed" },
                self.differences.join("; "),
                self.rerun.iter().map(|c| format!("`magician {c}`")).collect::<Vec<_>>().join(" and "),
                if self.rerun.contains(&"lint") { "`magician check` does both, per mod" } else { "`magician check` covers it, per mod" }
            ),
        }
    }
}

// --------------------------------------------------------------------------------------------------- the source

/// Where the release is: a folder or an https URL, and where that answer came from.
#[derive(Clone, Debug)]
pub struct Source {
    pub at: String,
    pub from: String,
}

pub fn source(args: &[String]) -> Source {
    let f = paths::update_source(flag(args, "--source"));
    Source { at: f.path.to_string_lossy().into_owned(), from: f.source }
}

fn is_url(at: &str) -> bool {
    let l = at.to_ascii_lowercase();
    l.starts_with("https://") || l.starts_with("http://")
}

fn loopback(url: &str) -> bool {
    let rest = url.split_once("://").map_or("", |(_, r)| r);
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = host.rsplit_once('@').map_or(host, |(_, h)| h);
    let host = if let Some(v6) = host.strip_prefix('[') { v6.split(']').next().unwrap_or("") } else { host.split(':').next().unwrap_or("") };
    host == "localhost" || host == "127.0.0.1" || host == "::1"
}

impl Source {
    fn join(&self, name: &str) -> String {
        if is_url(&self.at) {
            format!("{}/{name}", self.at.trim_end_matches('/'))
        } else {
            Path::new(&self.at).join(name).to_string_lossy().into_owned()
        }
    }

    pub fn label(&self) -> String {
        format!("{} ({})", self.at, self.from)
    }

    /// Whether this source is a URL, which only `https` may be (a plain `http` one only on this machine, for a test).
    fn usable(&self) -> Result<(), String> {
        if is_url(&self.at) && !self.at.to_ascii_lowercase().starts_with("https://") && !loopback(&self.at) {
            return Err(format!("{} is plain http: an update replaces the toolkit, so it must come over https (or from a folder)", self.at));
        }
        Ok(())
    }
}

/// How long a step may take before it is a "could not reach" and not a hang: 10 s, or `MAGICIAN_UPDATE_TIMEOUT`.
pub fn timeout_secs() -> u64 {
    std::env::var("MAGICIAN_UPDATE_TIMEOUT").ok().and_then(|v| v.trim().parse().ok()).filter(|n| *n > 0).unwrap_or(10)
}

/// A program that ships with the system, by its system folder on Windows (so a tool of the same name earlier on PATH,
/// Git's GNU `tar` which cannot read a zip, is not the one run).
pub fn system_tool(name: &str) -> PathBuf {
    if cfg!(windows) {
        if let Some(root) = std::env::var_os("SystemRoot") {
            let p = PathBuf::from(root).join("System32").join(format!("{name}.exe"));
            if p.is_file() {
                return p;
            }
        }
    }
    PathBuf::from(name)
}

fn curl(url: &str, out: Option<&Path>, whole_secs: u64, stall_secs: Option<u64>) -> Result<Vec<u8>, String> {
    let t = timeout_secs().to_string();
    let mut cmd = Command::new(system_tool("curl"));
    cmd.args(["--silent", "--show-error", "--fail", "--location", "--max-redirs", "5", "--proto", "=https,http", "--proto-redir", "=https,http"]);
    cmd.args(["--connect-timeout", &t, "--max-time", &whole_secs.to_string(), "--user-agent", &format!("magician/{}", env!("CARGO_PKG_VERSION"))]);
    if let Some(s) = stall_secs {
        cmd.args(["--speed-limit", "1000", "--speed-time", &s.to_string()]);
    }
    if loopback(url) {
        cmd.args(["--noproxy", "*"]);
    }
    match out {
        Some(p) => cmd.arg("--output").arg(p),
        None => cmd.args(["--output", "-"]),
    };
    cmd.arg(url).stdin(Stdio::null());
    let o = cmd.output().map_err(|e| format!("curl did not start ({e}); it ships with Windows 10 and Linux, or point the update source at a folder"))?;
    if !o.status.success() {
        let why = String::from_utf8_lossy(&o.stderr).trim().to_string();
        let hint = if why.contains("404") { " (nothing is published there: no release yet, or the repository is private)" } else { "" };
        return Err(format!("{url}: {}{hint}", if why.is_empty() { format!("curl {}", o.status) } else { why }));
    }
    Ok(o.stdout)
}

/// A small file of the release, as text.
pub fn read_text(src: &Source, name: &str) -> Result<String, String> {
    src.usable()?;
    let at = src.join(name);
    if is_url(&src.at) {
        let t = timeout_secs();
        let bytes = curl(&at, None, t, None)?;
        return String::from_utf8(bytes).map_err(|_| format!("{at}: not text"));
    }
    // a folder on a network share can hang in the operating system: read it on a thread and stop waiting
    let (tx, rx) = std::sync::mpsc::channel();
    let p = at.clone();
    std::thread::spawn(move || {
        let _ = tx.send(std::fs::read_to_string(&p));
    });
    match rx.recv_timeout(std::time::Duration::from_secs(timeout_secs())) {
        Ok(Ok(t)) => Ok(t),
        Ok(Err(e)) => Err(format!("{at}: {e}")),
        Err(_) => Err(format!("{at}: no answer in {} s", timeout_secs())),
    }
}

/// A file of the release (the archive) written to `dest`: copied from a folder, or fetched, giving up when it stalls.
pub fn fetch(src: &Source, name: &str, dest: &Path) -> Result<(), String> {
    src.usable()?;
    let at = src.join(name);
    if is_url(&src.at) {
        curl(&at, Some(dest), 900, Some(timeout_secs()))?;
        return Ok(());
    }
    std::fs::copy(&at, dest).map(|_| ()).map_err(|e| format!("{at}: {e}"))
}

// --------------------------------------------------------------------------------------------------- the report

/// This build against a source's latest release.
pub struct Report {
    pub local: Release,
    pub source: Source,
    pub latest: Result<Release, String>,
}

/// Read the source's `VERSION`. The only network step of `--check`.
pub fn look(source: Source) -> Report {
    let latest = read_text(&source, VERSION_FILE).and_then(|t| Release::parse(&t).map_err(|e| format!("{}: {e}", source.join(VERSION_FILE))));
    Report { local: Release::this_build(), source, latest }
}

impl Report {
    pub fn state(&self) -> (State, String) {
        match &self.latest {
            Ok(l) => compare(&self.local, l),
            Err(e) => (State::Unknown, format!("could not check: {e}")),
        }
    }

    pub fn judgement(&self) -> Option<Judgement> {
        self.latest.as_ref().ok().map(|l| judgement(&self.local, l))
    }

    /// The fields every `magician.update/1` document has.
    pub fn doc(&self, action: &str) -> jsonout::Obj {
        let (state, why) = self.state();
        jsonout::Obj::new()
            .str("schema", "magician.update/1")
            .str("action", action)
            .str("state", state.id())
            .str("why", &why)
            .raw("source", jsonout::Obj::new().str("at", &self.source.at).str("from", &self.source.from).build())
            .raw("current", self.local.json())
            .raw("latest", self.latest.as_ref().map_or("null".into(), Release::json))
            .raw("judgement", self.judgement().map_or("null".into(), |j| j.json()))
            .opt_str("error", self.latest.as_ref().err().map(String::as_str))
    }

    /// The report a person reads, for `--check` and what an update starts from.
    fn say(&self) {
        let (state, why) = self.state();
        let head = match state {
            State::Current => style::paint("current", style::Tone::Good),
            State::Behind => style::paint("an update is available", style::Tone::Warn),
            State::Ahead => style::paint("ahead of the latest release", style::Tone::Dim),
            State::Unknown => style::paint("could not check", style::Tone::Warn),
        };
        say!("magician update: {head}");
        say!("  this build   {}", self.local.label());
        match &self.latest {
            Ok(l) => {
                say!("  latest       {}", l.label());
                say!("  source       {}", self.source.label());
                say!("  why          {why}");
                if state != State::Current || self.judgement().is_some_and(|j| j.changed == Some(true)) {
                    if let Some(j) = self.judgement() {
                        say!("  judgement    {}", j.line(state == State::Behind));
                    }
                }
                if state == State::Behind {
                    say!("  next         magician update   (what it would do first: magician update --dry-run)");
                }
            }
            Err(e) => {
                say!("  source       {}", self.source.label());
                say!("  why          {e}");
            }
        }
    }
}

// ------------------------------------------------------------------------------------------------- the command

pub fn cmd_update(args: &[String]) -> R {
    let has = |f: &str| args.iter().any(|a| a == f);
    let (check, describe, dry) = (has("--check"), has("--describe"), has("--dry-run"));
    if let Some(extra) = crate::positional(args).first() {
        return Err(Stop::Usage(format!("`magician update` takes no argument, not {extra:?}; `magician help update` lists its options")));
    }
    if [check, describe, dry].iter().filter(|b| **b).count() > 1 {
        return Err(Stop::Usage("--check (only reads), --dry-run (says what an update would do) and --describe (this build's VERSION) are three questions: ask one".into()));
    }
    if describe {
        let me = Release::this_build();
        if json_mode() {
            outln!("{}", jsonout::Obj::new().str("schema", "magician.update/1").str("action", "describe").raw("current", me.json()).build());
        } else {
            out!("{}", me.text());
        }
        return Ok(ExitCode::SUCCESS);
    }
    let src = source(args);
    if check {
        return Ok(cmd_check(look(src)));
    }
    crate::updateapply::apply(src, dry)
}

fn cmd_check(rep: Report) -> ExitCode {
    let (state, _) = rep.state();
    if json_mode() {
        let doc = rep.doc("check").opt_str("next", (state == State::Behind).then_some("magician update")).build();
        outln!("{doc}");
    } else {
        rep.say();
    }
    ExitCode::from(match state {
        State::Current | State::Ahead => cli::CLEAN,
        State::Behind => cli::UPDATE_AVAILABLE,
        State::Unknown => {
            errln!("magician: {}", rep.state().1);
            cli::REFUSED
        }
    })
}

/// For `doctor --updates`: the check as one line. A source that can't be read is a note, not a failure.
pub fn doctor_check(args: &[String]) -> crate::doctor::Check {
    use crate::doctor::{Check, State as S};
    let rep = look(source(args));
    let (state, why) = rep.state();
    let (st, detail, fix) = match (&rep.latest, state) {
        (Ok(_), State::Current) => (S::Ok, format!("the latest release, {} ({})", rep.local.label(), rep.source.at), None),
        (Ok(l), State::Behind) => {
            let j = judgement(&rep.local, l);
            (S::Todo, format!("{} is out, this is {}; {}", l.label(), rep.local.version, j.line(true)), Some("magician update --dry-run, then magician update".to_string()))
        }
        (Ok(l), _) => (S::Ok, format!("ahead of the latest release {} ({why})", l.label()), None),
        (Err(e), _) => (S::Skip, format!("could not check: {e}"), Some("point MAGICIAN_UPDATE_SOURCE (or `update_source =` in the config file) at the release folder or URL".to_string())),
    };
    Check { name: "update", state: st, detail, fix }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rel(version: &str, commit: &str, date: &str) -> Release {
        Release { version: version.into(), commit: commit.into(), date: date.into(), ..Release::default() }
    }

    #[test]
    fn a_release_reads_back_from_its_text() {
        let me = Release::this_build();
        assert_eq!(Release::parse(&me.text()).unwrap(), me);
        assert!(me.text().contains("lint_rules = 79\n") || me.text().contains("lint_rules = "), "{}", me.text());
        assert!(Release::parse("commit = abc\n").unwrap_err().contains("version"));
        assert!(Release::parse("# nothing\n").is_err());
        // the judgement lines may be missing
        let bare = Release::parse("version = 0.2.0\ncommit = abc\n").unwrap();
        assert_eq!((bare.rubric, bare.date.as_str()), (None, "unknown"));
    }

    #[test]
    fn the_version_decides_first_then_the_commit() {
        let here = rel("0.1.0", "aaaaaaaaaaaa", "2026-11-25");
        assert_eq!(compare(&here, &rel("0.2.0", "bbbbbbbbbbbb", "2026-11-20")).0, State::Behind, "a newer version wins over an older date");
        assert_eq!(compare(&here, &rel("0.0.9", "bbbbbbbbbbbb", "2027-01-01")).0, State::Ahead);
        assert_eq!(compare(&here, &rel("0.1", "aaaaaaaaaaaa", "2026-11-25")).0, State::Current, "0.1 is 0.1.0");
        assert_eq!(compare(&rel("0.10.0", "a", "x"), &rel("0.9.0", "b", "y")).0, State::Ahead, "numbers, not text");
        // one version, two commits: their dates order them
        assert_eq!(compare(&here, &rel("0.1.0", "bbbbbbbbbbbb", "2026-12-01")).0, State::Behind);
        assert_eq!(compare(&here, &rel("0.1.0", "bbbbbbbbbbbb", "2026-11-01")).0, State::Ahead);
        // the same day, or no date: an update, said with why
        let (s, why) = compare(&here, &rel("0.1.0", "bbbbbbbbbbbb", "2026-11-25"));
        assert_eq!(s, State::Behind);
        assert!(why.contains("do not order them"), "{why}");
        assert_eq!(compare(&here, &rel("0.1.0", "bbbbbbbbbbbb", "unknown")).0, State::Behind);
        // a build that does not know its commit can't be told from the release
        assert_eq!(compare(&rel("0.1.0", "unknown", "unknown"), &rel("0.1.0", "unknown", "unknown")).0, State::Behind);
    }

    #[test]
    fn a_dirty_build_of_the_release_commit_is_current_and_says_so() {
        let (s, why) = compare(&rel("0.1.0", "aaaaaaaaaaaa-dirty", "2026-11-25"), &rel("0.1.0", "aaaaaaaaaaaa", "2026-11-25"));
        assert_eq!(s, State::Current);
        assert!(why.contains("local changes"), "{why}");
    }

    #[test]
    fn judgement_names_what_differs_and_what_to_run_again() {
        let a = Release::this_build();
        let same = judgement(&a, &a);
        assert_eq!((same.changed, same.differences.len(), same.rerun.len()), (Some(false), 0, 0));
        assert!(same.line(true).starts_with("unchanged"), "{}", same.line(true));
        let mut b = a.clone();
        b.rubric = Some("9.9.9".into());
        let j = judgement(&a, &b);
        assert_eq!((j.changed, j.rerun.clone()), (Some(true), vec!["grade"]));
        assert!(j.differences[0].contains("grading rubric is 9.9.9") && j.line(true).starts_with("would change"), "{j:?}");
        let mut c = a.clone();
        c.lint_rules = Some("80".into());
        c.lint_fingerprint = Some("0".repeat(64));
        let j = judgement(&a, &c);
        assert_eq!(j.rerun, vec!["lint", "grade"]);
        assert_eq!(j.differences.len(), 1, "a changed count already says the rules differ: {j:?}");
        let mut d = a.clone();
        d.lint_fingerprint = Some("0".repeat(64));
        assert!(judgement(&a, &d).differences[0].contains("same count"));
        // a VERSION that says nothing about judgement is not "unchanged"
        let none = judgement(&a, &Release { version: "1".into(), commit: "x".into(), ..Release::default() });
        assert_eq!(none.changed, None);
        assert!(none.line(true).contains("does not say"));
    }

    #[test]
    fn each_platform_has_its_own_asset_and_an_unbuilt_one_has_none() {
        assert_eq!(asset_for("windows", "x86_64").as_deref(), Some("magician-windows-x64.zip"));
        assert_eq!(asset_for("linux", "x86_64").as_deref(), Some("magician-linux-x64.tar.gz"));
        assert_eq!(asset_for("macos", "aarch64"), None);
        assert_eq!(asset_for("linux", "aarch64"), None);
        let sums = "aa11  magician-windows-x64.zip
BB22 *magician-linux-x64.tar.gz
00  VERSION
";
        assert_eq!(checksum_of(sums, "magician-linux-x64.tar.gz").as_deref(), Some("bb22"));
        assert_eq!(checksum_of(sums, "magician-macos-x64.tar.gz"), None);
    }

    #[test]
    fn only_https_or_this_machine_may_serve_an_update() {
        let s = |at: &str| Source { at: at.into(), from: "test".into() };
        assert!(s("https://example.org/releases").usable().is_ok());
        assert!(s(r"D:\releases\magician").usable().is_ok());
        assert!(s("http://127.0.0.1:8080").usable().is_ok() && s("http://localhost/x").usable().is_ok() && s("http://[::1]:9/x").usable().is_ok());
        assert!(s("http://example.org/releases").usable().unwrap_err().contains("https"));
        assert!(s("http://127.0.0.1.example.org/").usable().is_err(), "a name that starts like loopback is not loopback");
        assert!(s("http://user@evil.example/").usable().is_err());
        assert_eq!(s("https://x.org/a/").join("VERSION"), "https://x.org/a/VERSION");
    }

    #[test]
    fn the_default_source_is_this_builds_repository() {
        let f = paths::update_source(None);
        if std::env::var_os("MAGICIAN_UPDATE_SOURCE").is_none() {
            assert!(f.path.to_string_lossy().ends_with("/releases/latest/download") && f.path.to_string_lossy().starts_with("https://"), "{:?}", f.path);
        }
        assert_eq!(paths::update_source(Some("D:\\x".into())).source, "--source");
    }
}
