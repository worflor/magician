//! `magician update` with no `--check`: apply an update, only when asked, and never leave a half-updated toolkit.
//!
//! Two kinds of toolkit folder, told apart by a `.git` in it:
//!
//! - **A release install** (the folder a release archive unpacks to): read the source's `VERSION`, and when it is newer
//!   fetch the archive and `SHA256SUMS`, check its SHA-256, unpack it beside the toolkit (`<toolkit>.update`), run the
//!   new `magician --version` from there, then swap the entries in. Each live entry moves into
//!   `.magician-previous.partial` inside the toolkit and the staged one takes its place; a step that fails puts every
//!   entry back. When the swap and a second `--version` of the installed executable hold, the folder is renamed
//!   `.magician-previous` and deleted as far as Windows allows: the running `magician.exe` can be renamed but not
//!   deleted, so it stays there until the next `magician update` clears it.
//! - **A git checkout**: follow the branch. Refuse (exit 3) when the tree has local changes, there is no upstream
//!   or it cannot fast-forward, fetch (stalls longer than 10 s end it), fast-forward and `cargo build --release`. The
//!   running executable is renamed aside first when it is the one the build writes, and put back, with the tree, when
//!   the build fails. A checkout follows its branch, not the releases, so the update source plays no part.
//!
//! Both refuse (exit 3) while a game session holds the seat (the game lock), and both leave nothing changed on a
//! refusal. Exit 0 updated or already current, 3 refused or could not fetch, 1 failed after the swap began and was
//! rolled back.

use super::*;
use crate::update::{self, Release, Source, State};
use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// The live folder's old files while a swap is under way; renamed `PREVIOUS` once the new ones are in and verified.
const PARTIAL: &str = ".magician-previous.partial";
const PREVIOUS: &str = ".magician-previous";

fn exe_name() -> String {
    format!("magician{}", std::env::consts::EXE_SUFFIX)
}

// --------------------------------------------------------------------------------------------- running things

struct Ran {
    ok: bool,
    out: String,
    err: String,
}

/// Run a command to its end, or kill it after `secs`. A grandchild that keeps a pipe open after the kill is left.
fn run(cmd: &mut Command, secs: Option<u64>) -> Result<Ran, String> {
    cmd.stdin(Stdio::null());
    let Some(secs) = secs else {
        let o = cmd.output().map_err(|e| e.to_string())?;
        return Ok(Ran { ok: o.status.success(), out: String::from_utf8_lossy(&o.stdout).into_owned(), err: String::from_utf8_lossy(&o.stderr).into_owned() });
    };
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    let mut readers = Vec::new();
    for pipe in [child.stdout.take().map(|p| Box::new(p) as Box<dyn Read + Send>), child.stderr.take().map(|p| Box::new(p) as Box<dyn Read + Send>)] {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut b = Vec::new();
            if let Some(mut p) = pipe {
                let _ = p.read_to_end(&mut b);
            }
            let _ = tx.send(b);
        });
        readers.push(rx);
    }
    let t0 = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) if t0.elapsed() > Duration::from_secs(secs) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("no answer in {secs} s"));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => return Err(e.to_string()),
        }
    };
    let mut text = readers.into_iter().map(|rx| String::from_utf8_lossy(&rx.recv_timeout(Duration::from_secs(2)).unwrap_or_default()).into_owned());
    Ok(Ran { ok: status.success(), out: text.next().unwrap_or_default(), err: text.next().unwrap_or_default() })
}

fn git(root: &Path, args: &[&str], secs: Option<u64>) -> Result<Ran, String> {
    let mut c = Command::new("git");
    c.arg("-C").arg(root).args(args).env("GIT_TERMINAL_PROMPT", "0").env("GCM_INTERACTIVE", "never");
    run(&mut c, secs).map_err(|e| format!("git {}: {e}", args.first().unwrap_or(&"")))
}

/// What a `magician` at `exe` says for `--version`, or why it could not be asked.
fn version_of(exe: &Path) -> Result<String, String> {
    let r = run(Command::new(exe).arg("--version"), Some(30)).map_err(|e| format!("{} did not run: {e}", exe.display()))?;
    if !r.ok {
        return Err(format!("{} --version failed: {}", exe.display(), r.err.trim()));
    }
    Ok(r.out.trim().to_string())
}

// ------------------------------------------------------------------------------------------------ the checks

/// Refuse while a game session holds the seat (the game lock names a holder): the session runs this toolkit's Python,
/// and the game it launched is its to stop. A running game nobody holds the seat for is not this command's business.
fn seat() -> Result<(), Stop> {
    let lock = paths::game_lock();
    match std::fs::read_to_string(&lock.path).ok().as_deref().and_then(install::lock_holder) {
        Some(h) => Err(Stop::Refused(format!(
            "a game session holds the seat ('{h}', game lock {}): not replacing the toolkit its scripts run from. Wait until it hands the seat back (`magician session down`), then update",
            lock.path.display()
        ))),
        None => Ok(()),
    }
}

fn remove_any(p: &Path) -> std::io::Result<()> {
    match p.symlink_metadata() {
        Ok(m) if m.is_dir() => std::fs::remove_dir_all(p),
        Ok(_) => std::fs::remove_file(p),
        Err(_) => Ok(()),
    }
}

/// Clear what an earlier update left: the previous version, and a renamed-aside executable. Never the `.partial` one.
fn clear_leftovers(root: &Path, exes: &[&Path]) {
    let _ = remove_any(&root.join(PREVIOUS));
    for e in exes {
        let _ = remove_any(&aside(e));
    }
}

fn aside(exe: &Path) -> PathBuf {
    let mut n = exe.as_os_str().to_os_string();
    n.push(".old");
    PathBuf::from(n)
}

/// Whether the running program's file can't be overwritten while it runs (Windows), so it is moved aside first.
fn program_locked_while_running() -> bool {
    cfg!(windows)
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

// ---------------------------------------------------------------------------------------------------- the swap

/// Put each entry of `staged` (by `names`) into `root`, the live entry of that name going to `partial` first. A step
/// that fails puts every entry back, the one under way included (with real renames, whatever `mv` does), and says so.
/// `mv` is the rename, so a test can fail one.
fn swap(root: &Path, staged: &Path, partial: &Path, names: &[String], mv: &dyn Fn(&Path, &Path) -> std::io::Result<()>) -> Result<(), String> {
    std::fs::create_dir_all(partial).map_err(|e| format!("{}: {e}", partial.display()))?;
    let mut done: Vec<(&String, bool)> = Vec::new();
    for name in names {
        let (live, old, new) = (root.join(name), partial.join(name), staged.join(name));
        let had = live.symlink_metadata().is_ok();
        let step = (|| {
            if had {
                mv(&live, &old).map_err(|e| format!("could not move {} aside: {e}", live.display()))?;
            }
            mv(&new, &live).map_err(|e| {
                if had {
                    let _ = std::fs::rename(&old, &live);
                }
                format!("could not put the new {} in place: {e}", live.display())
            })
        })();
        match step {
            Ok(()) => done.push((name, had)),
            Err(why) => {
                let left = put_back(root, partial, &done);
                return Err(match left {
                    Ok(()) => format!("{why}; every entry moved so far is back as it was"),
                    Err(e) => format!("{why}; AND the old files could not all be put back ({e}): they are in {}", partial.display()),
                });
            }
        }
    }
    Ok(())
}

/// Undo a swap: the new entry out, the old one back, last first.
fn put_back(root: &Path, partial: &Path, done: &[(&String, bool)]) -> Result<(), String> {
    let mut problems = Vec::new();
    for (name, had) in done.iter().rev() {
        let (live, old) = (root.join(name), partial.join(name));
        if let Err(e) = remove_any(&live) {
            problems.push(format!("{}: {e}", live.display()));
            continue;
        }
        if *had {
            if let Err(e) = std::fs::rename(&old, &live) {
                problems.push(format!("{}: {e}", old.display()));
            }
        }
    }
    if problems.is_empty() {
        let _ = std::fs::remove_dir(partial);
        Ok(())
    } else {
        Err(problems.join("; "))
    }
}

// ------------------------------------------------------------------------------------------------ the command

pub fn apply(src: Source, dry: bool) -> R {
    let toolkit = paths::toolkit().map_err(|e| Stop::Usage(format!("`magician update` replaces the toolkit folder, and {e}")))?;
    let root = toolkit.path;
    let exe = std::env::current_exe().map_err(|e| Stop::Usage(format!("this executable's own path can't be read ({e})")))?;
    if root.join(".git").exists() {
        checkout(&root, &exe, dry)
    } else {
        release_install(&root, &exe, src, dry)
    }
}

/// The document a successful apply prints, and the words for a person.
fn finish(doc: jsonout::Obj) {
    if json_mode() {
        outln!("{}", doc.build());
    }
}

fn release_install(root: &Path, exe: &Path, src: Source, dry: bool) -> R {
    let rep = update::look(src.clone());
    let latest = rep.latest.clone().map_err(|e| Stop::Refused(format!("could not read the update source, so nothing was changed: {e}")))?;
    let (state, why) = rep.state();
    let doc = |outcome: &str| rep.doc("update").bool("dry_run", dry).str("install", "release").str("outcome", outcome);
    if state != State::Behind {
        say!("magician update: already current: {} ({why})", rep.local.label());
        finish(doc("current"));
        if !dry {
            clear_leftovers(root, &[exe]);
        }
        return Ok(ExitCode::SUCCESS);
    }
    let name = exe_name();
    if exe.file_name().is_none_or(|n| n.to_string_lossy() != name) || !exe.parent().is_some_and(|p| same_file(p, root)) {
        return Err(Stop::Refused(format!(
            "this magician ({}) is not the {name} at the top of the toolkit folder {}: run that one, so the update replaces the program that is running",
            exe.display(),
            root.display()
        )));
    }
    if root.join(PARTIAL).exists() {
        return Err(Stop::Refused(format!(
            "an earlier update stopped halfway: the files it moved aside are in {}. Move them back over the toolkit (or delete that folder if the toolkit works), then update",
            root.join(PARTIAL).display()
        )));
    }
    let staging = match (root.parent().filter(|p| !p.as_os_str().is_empty()), root.file_name()) {
        (Some(p), Some(n)) => p.join(format!("{}.update", n.to_string_lossy())),
        _ => return Err(Stop::Refused(format!("{} has no folder above it to unpack an update beside", root.display()))),
    };
    let Some(asset) = update::asset() else {
        return Err(Stop::Refused(format!("could not update: no build for {} at {}; build from source (the README says how); nothing was changed", update::platform(), src.label())));
    };
    let sums = update::read_text(&src, update::SUMS).map_err(|e| Stop::Refused(format!("could not read the checksums, so nothing was changed: {e}")))?;
    let Some(want) = update::checksum_of(&sums, &asset) else {
        return Err(Stop::Refused(format!("could not update: no build for {} at {}: {} lists no {asset}; build from source (the README says how); nothing was changed", update::platform(), src.label(), update::SUMS)));
    };
    seat()?;
    let judge = rep.judgement().map(|j| j.line(dry));
    if dry {
        say!("magician update: would update {} -> {}", rep.local.label(), latest.label());
        say!("  source       {}", src.label());
        for step in [
            format!("fetch {asset} from the source (its checksum is already read from {})", update::SUMS),
            "check the archive's SHA-256 against SHA256SUMS".to_string(),
            format!("unpack it into {} and run its {name} --version", staging.display()),
            format!("move each entry of the toolkit {} aside into {PARTIAL} and put the new one in its place, putting everything back if a step fails", root.display()),
            format!("run the installed {name} --version, keep the old files as {PREVIOUS} until the next update"),
        ] {
            say!("  would        {step}");
        }
        if let Some(j) = &judge {
            say!("  judgement    {j}");
        }
        finish(doc("would-update"));
        return Ok(ExitCode::SUCCESS);
    }
    clear_leftovers(root, &[exe]);
    // a staging folder left by an update that did not finish is ours to clear: nothing else writes there
    let _ = remove_any(&staging);
    let outcome = fetch_and_swap(root, &staging, &src, &latest, &name, &asset, &want);
    let _ = remove_any(&staging);
    outcome?;
    say!("magician update: updated {} -> {}", rep.local.label(), latest.label());
    say!("  source       {}", src.label());
    if let Some(j) = &judge {
        say!("  judgement    {j}");
    }
    say!("  kept         the previous files, as {} (cleared by the next update; the old {name} can't be deleted while it runs)", root.join(PREVIOUS).display());
    finish(doc("updated").str("previous", &root.join(PREVIOUS).to_string_lossy()));
    Ok(ExitCode::SUCCESS)
}

/// Everything from the fetch to the verified swap. `Refused` means nothing was changed; `Broken` means the swap began,
/// failed, and was rolled back.
fn fetch_and_swap(root: &Path, staging: &Path, src: &Source, latest: &Release, name: &str, asset: &str, want: &str) -> Result<(), Stop> {
    let refuse = |m: String| Stop::Refused(format!("{m}; nothing was changed"));
    std::fs::create_dir_all(staging).map_err(|e| Stop::Broken(format!("could not make {} to unpack into: {e}; nothing was changed", staging.display())))?;
    let zip = staging.join(asset);
    say!("  fetching     {asset} from {}", src.at);
    update::fetch(src, asset, &zip).map_err(|e| refuse(format!("could not fetch the release: {e}")))?;
    let got = grade::sha256::file(&zip).map_err(|e| refuse(format!("could not read back {}: {e}", zip.display())))?;
    if got != want {
        return Err(refuse(format!("the checksum of {} does not match {}: it is {got}, the list says {want} (a damaged or altered download)", asset, update::SUMS)));
    }
    say!("  checksum     ok ({})", &got[..12]);
    let new = staging.join("new");
    unpack(&zip, &new).map_err(refuse)?;
    for must in [name.to_string(), "ingame/mod_session.py".to_string()] {
        if !new.join(&must).is_file() {
            return Err(refuse(format!("the release does not hold {must}: not a toolkit folder")));
        }
    }
    let expect = format!("magician {} (git {})", latest.version, latest.commit);
    let said = version_of(&new.join(name)).map_err(refuse)?;
    if said != expect {
        return Err(refuse(format!("the {name} in the release says {said:?}, and its VERSION says {expect:?}: a release that does not match its own description")));
    }
    let mut names: Vec<String> = std::fs::read_dir(&new).map_err(|e| refuse(format!("{}: {e}", new.display())))?.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    let partial = root.join(PARTIAL);
    swap(root, &new, &partial, &names, &|a, b| std::fs::rename(a, b)).map_err(|e| Stop::Broken(format!("the update failed: {e}")))?;
    let installed = version_of(&root.join(name)).and_then(|v| if v == expect { Ok(v) } else { Err(format!("the installed {name} says {v:?}, not {expect:?}")) });
    if let Err(e) = installed {
        let done: Vec<(&String, bool)> = names.iter().map(|n| (n, partial.join(n).symlink_metadata().is_ok())).collect();
        return Err(Stop::Broken(match put_back(root, &partial, &done) {
            Ok(()) => format!("the update failed after the swap ({e}) and was rolled back: the toolkit is as it was"),
            Err(p) => format!("the update failed after the swap ({e}), AND the old files could not all be put back ({p}): they are in {}", partial.display()),
        }));
    }
    say!("  swapped      {} entries ({})", names.len(), names.join(", "));
    let _ = std::fs::rename(&partial, root.join(PREVIOUS));
    let _ = remove_any(&root.join(PREVIOUS));
    Ok(())
}

/// `tar -xf` reads a zip on Windows 10 and Linux (bsdtar). Its default refuses paths that climb out of the folder.
fn unpack(zip: &Path, into: &Path) -> Result<(), String> {
    std::fs::create_dir_all(into).map_err(|e| format!("{}: {e}", into.display()))?;
    let r = run(Command::new(update::system_tool("tar")).arg("-xf").arg(zip).arg("-C").arg(into), Some(300))
        .map_err(|e| format!("tar did not run ({e}); an update needs a tar that reads zips (Windows 10's tar.exe, or bsdtar)"))?;
    if !r.ok {
        return Err(format!("tar could not unpack {}: {}; an update needs a tar that reads zips (Windows 10's tar.exe, or bsdtar)", zip.display(), r.err.trim()));
    }
    Ok(())
}

// --------------------------------------------------------------------------------------------- git checkouts

fn checkout(root: &Path, exe: &Path, dry: bool) -> R {
    let refuse = |m: String| Stop::Refused(format!("{m}; nothing was changed"));
    let say_git = |args: &[&str], secs: Option<u64>| -> Result<String, Stop> {
        let r = git(root, args, secs).map_err(|e| refuse(format!("git is needed to update a checkout ({e})")))?;
        if r.ok { Ok(r.out.trim().to_string()) } else { Err(refuse(format!("git {} failed in {}: {}", args.join(" "), root.display(), r.err.trim()))) }
    };
    let dirty = say_git(&["status", "--porcelain", "--untracked-files=no"], Some(60))?;
    if !dirty.is_empty() {
        let n = dirty.lines().count();
        return Err(refuse(format!("the checkout at {} has local changes ({n} file{}): commit or stash them, then update", root.display(), if n == 1 { "" } else { "s" })));
    }
    let branch = git(root, &["symbolic-ref", "--short", "-q", "HEAD"], Some(30)).ok().filter(|r| r.ok).map(|r| r.out.trim().to_string()).filter(|b| !b.is_empty());
    let Some(branch) = branch else {
        return Err(refuse(format!("the checkout at {} is on no branch (a detached HEAD): check out the branch to follow, then update", root.display())));
    };
    let upstream = git(root, &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"], Some(30)).ok().filter(|r| r.ok).map(|r| r.out.trim().to_string()).filter(|b| !b.is_empty());
    let Some(upstream) = upstream else {
        return Err(refuse(format!("the branch {branch} follows no upstream: `git branch --set-upstream-to=origin/{branch}`, then update")));
    };
    let remote = upstream.split('/').next().unwrap_or("origin").to_string();
    let t = update::timeout_secs().to_string();
    say!("  fetching     {remote} (a stall of over {t} s ends it)");
    let fetch_args = ["-c", "http.lowSpeedLimit=1000", "-c", &format!("http.lowSpeedTime={t}"), "fetch", "--quiet", &remote];
    let fetched = git(root, &fetch_args, Some(120)).map_err(|e| refuse(format!("could not fetch {remote}: {e}")))?;
    if !fetched.ok {
        return Err(refuse(format!("could not fetch {remote}: {}", fetched.err.trim())));
    }
    let old = say_git(&["rev-parse", "HEAD"], Some(30))?;
    let new = say_git(&["rev-parse", "@{u}"], Some(30))?;
    let counts = say_git(&["rev-list", "--left-right", "--count", "HEAD...@{u}"], Some(30))?;
    let (ahead, behind): (u64, u64) = counts.split_once(char::is_whitespace).and_then(|(a, b)| Some((a.trim().parse().ok()?, b.trim().parse().ok()?))).unwrap_or((0, 0));
    let short = |h: &str| h.chars().take(12).collect::<String>();
    let doc = |outcome: &str| {
        jsonout::Obj::new()
            .str("schema", "magician.update/1")
            .str("action", "update")
            .bool("dry_run", dry)
            .str("install", "checkout")
            .str("outcome", outcome)
            .str("branch", &branch)
            .str("upstream", &upstream)
            .str("from", &short(&old))
            .str("to", &short(&new))
    };
    if behind == 0 {
        say!("magician update: already current: {branch} is level with {upstream} at {}{}", short(&old), if ahead > 0 { format!(" ({ahead} commit(s) ahead of it)") } else { String::new() });
        finish(doc("current"));
        if !dry {
            clear_leftovers(root, &[exe]);
        }
        return Ok(ExitCode::SUCCESS);
    }
    if ahead > 0 {
        return Err(refuse(format!("{branch} cannot fast-forward to {upstream}: it has {ahead} commit(s) {upstream} does not, and {upstream} has {behind} it does not. Rebase or merge them yourself, then update")));
    }
    if !run(Command::new("cargo").arg("--version"), Some(30)).is_ok_and(|r| r.ok) {
        return Err(refuse("cargo is needed to rebuild a checkout, and it did not run".to_string()));
    }
    seat()?;
    let target = std::env::var_os("CARGO_TARGET_DIR").filter(|d| !d.is_empty()).map(PathBuf::from).map(|d| if d.is_absolute() { d } else { root.join(d) }).unwrap_or_else(|| root.join("target"));
    let built = target.join("release").join(exe_name());
    let running_is_built = same_file(exe, &built);
    // the one place the platforms differ: Windows will not let a running program be overwritten, Linux will
    let move_aside = running_is_built && program_locked_while_running();
    if dry {
        say!("magician update: would fast-forward {branch} {} -> {} ({behind} commit(s) from {upstream}) and run `cargo build --release`", short(&old), short(&new));
        say!("  would        {}", if move_aside { format!("rename the running {} aside first, and put it back (with {branch} at {}) if the build fails", built.display(), short(&old)) } else { format!("write the build to {}; the running {} is not that file", built.display(), exe.display()) });
        finish(doc("would-update"));
        return Ok(ExitCode::SUCCESS);
    }
    clear_leftovers(root, &[exe, &built]);
    let moved = aside(&built);
    if move_aside {
        std::fs::rename(&built, &moved).map_err(|e| refuse(format!("could not move the running {} aside ({e})", built.display())))?;
    }
    let undo = |why: String| -> Stop {
        let tree = git(root, &["reset", "--hard", "--quiet", &old], Some(120)).is_ok_and(|r| r.ok);
        let program = if move_aside { remove_any(&built).and_then(|()| std::fs::rename(&moved, &built)).is_ok() } else { true };
        match (tree, program) {
            (true, true) => Stop::Broken(format!("the update failed and was rolled back ({branch} is at {} again, and the program is the one that was running): {why}", short(&old))),
            _ => Stop::Broken(format!("the update failed ({why}), AND the rollback was not complete (tree back: {tree}, program back: {program}): `git -C {} reset --hard {old}` and rebuild", root.display())),
        }
    };
    say!("  fast-forward {branch} {} -> {} ({behind} commit(s))", short(&old), short(&new));
    match git(root, &["merge", "--ff-only", "--quiet", "@{u}"], Some(120)) {
        Ok(r) if r.ok => {}
        Ok(r) => return Err(undo(format!("git merge --ff-only: {}", r.err.trim()))),
        Err(e) => return Err(undo(e)),
    }
    say!("  building     cargo build --release (in {})", root.display());
    let hint = progress::start("cargo build --release");
    let mut cargo = Command::new("cargo");
    cargo.args(["build", "--release"]).current_dir(root);
    let r = run(&mut cargo, None);
    drop(hint);
    match r {
        Ok(r) if r.ok => {}
        Ok(r) => return Err(undo(format!("cargo build --release failed: {}", r.err.lines().rev().take(12).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join(" | ")))),
        Err(e) => return Err(undo(format!("cargo did not run: {e}"))),
    }
    let said = match version_of(&built) {
        Ok(v) => v,
        Err(e) => return Err(undo(format!("the new build does not run: {e}"))),
    };
    let after = run(Command::new(&built).args(["update", "--describe"]), Some(30)).ok().filter(|r| r.ok).and_then(|r| Release::parse(&r.out).ok());
    let judge = after.as_ref().map(|a| update::judgement(&Release::this_build(), a));
    say!("magician update: updated {branch} {} -> {} and rebuilt {}", short(&old), short(&new), built.display());
    say!("  new build    {said}");
    say!("  judgement    {}", judge.as_ref().map_or("not known: the new build did not describe itself".to_string(), |j| j.line(false)));
    if !running_is_built {
        say!("  note         the program that ran this update, {}, is not the one built: run {} from now on", exe.display(), built.display());
    }
    if move_aside {
        say!("  kept         the old program as {} (it can't be deleted while it runs; the next update clears it)", moved.display());
    }
    finish(doc("updated").str("built", &built.to_string_lossy()).str("version", &said).raw("judgement", judge.map_or("null".into(), |j| j.json())));
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("magician-updateapply-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn put(p: &Path, text: &str) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }

    fn tree(root: &Path) -> Vec<(String, String)> {
        let mut out = Vec::new();
        fn walk(base: &Path, d: &Path, out: &mut Vec<(String, String)>) {
            for e in std::fs::read_dir(d).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk(base, &p, out);
                } else {
                    out.push((p.strip_prefix(base).unwrap().to_string_lossy().replace('\\', "/"), std::fs::read_to_string(&p).unwrap()));
                }
            }
        }
        walk(root, root, &mut out);
        out.sort();
        out
    }

    fn fixture(d: &Path) -> (PathBuf, PathBuf, Vec<String>) {
        let (root, staged) = (d.join("toolkit"), d.join("new"));
        put(&root.join("magician.exe"), "old exe");
        put(&root.join("ingame/mod_session.py"), "old session");
        put(&root.join("docs/a.md"), "old doc");
        put(&root.join("mine/notes.txt"), "not in the release");
        put(&staged.join("magician.exe"), "new exe");
        put(&staged.join("ingame/mod_session.py"), "new session");
        put(&staged.join("docs/a.md"), "new doc");
        put(&staged.join("tools/new.py"), "an entry the old toolkit had no copy of");
        (root, staged, ["docs", "ingame", "magician.exe", "tools"].iter().map(|s| s.to_string()).collect())
    }

    #[test]
    fn a_swap_puts_every_new_entry_in_and_keeps_every_old_one() {
        let d = scratch("swap-ok");
        let (root, staged, names) = fixture(&d);
        swap(&root, &staged, &root.join(PARTIAL), &names, &|a, b| std::fs::rename(a, b)).unwrap();
        let live = tree(&root);
        assert!(live.contains(&("magician.exe".into(), "new exe".into())) && live.contains(&("tools/new.py".into(), "an entry the old toolkit had no copy of".into())), "{live:?}");
        assert!(live.contains(&("mine/notes.txt".into(), "not in the release".into())), "an entry the release does not hold is left alone");
        let kept = tree(&root.join(PARTIAL));
        assert_eq!(kept, vec![("docs/a.md".into(), "old doc".into()), ("ingame/mod_session.py".into(), "old session".into()), ("magician.exe".into(), "old exe".into())]);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_swap_that_fails_at_any_step_puts_everything_back() {
        // fail the Nth rename, for every N: the toolkit must read the same afterwards, and say so
        for n in 1..=7 {
            let d = scratch(&format!("swap-fail-{n}"));
            let (root, staged, names) = fixture(&d);
            let before = tree(&root);
            let calls = Cell::new(0);
            let r = swap(&root, &staged, &root.join(PARTIAL), &names, &|a, b| {
                calls.set(calls.get() + 1);
                if calls.get() == n { Err(std::io::Error::other("injected")) } else { std::fs::rename(a, b) }
            });
            let e = r.unwrap_err();
            assert!(e.contains("injected") && e.contains("is back as it was"), "step {n}: {e}");
            assert_eq!(tree(&root), before, "step {n} left the toolkit changed");
            assert!(!root.join(PARTIAL).exists(), "step {n} left the partial folder");
            let _ = std::fs::remove_dir_all(&d);
        }
    }
}
