//! `magician doctor`: is this machine ready, and what exactly to do where it is not. One line per thing Magician
//! depends on, each `ok`, `todo` (works, but do this), `skip` (optional, not set up) or `FAIL` (commands that need it
//! stop), with the command or the config line that fixes it. Reads only; exit 1 when something fails.

use super::*;
use crate::modctx::ModCtx;
use crate::style;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Ok,
    Todo,
    Skip,
    Fail,
}

impl State {
    pub fn id(self) -> &'static str {
        match self {
            State::Ok => "ok",
            State::Todo => "todo",
            State::Skip => "skip",
            State::Fail => "FAIL",
        }
    }
}

pub struct Check {
    pub name: &'static str,
    pub state: State,
    pub detail: String,
    pub fix: Option<String>,
}

fn c(name: &'static str, state: State, detail: impl Into<String>, fix: Option<String>) -> Check {
    Check { name, state, detail: detail.into(), fix }
}

/// Whether a program named without a folder is on PATH.
fn on_path(name: &Path) -> Option<PathBuf> {
    let exts: Vec<String> = if cfg!(windows) { vec![String::new(), ".exe".into(), ".cmd".into(), ".bat".into()] } else { vec![String::new()] };
    std::env::split_paths(&std::env::var_os("PATH")?).find_map(|d| exts.iter().map(|e| d.join(format!("{}{e}", name.display()))).find(|p| p.is_file()))
}

/// Every check, in the order a new machine would meet them. `args` are the command's (`--game`, `--csdk`, `--vrf`,
/// `--policy`, `--mod`).
pub fn checks(args: &[String]) -> Vec<Check> {
    let mut out = Vec::new();
    let config = paths::config_file();
    let config_line = |key: &str, example: &str| match &config {
        Some(p) => format!("add `{key} = {example}` to {}", p.display()),
        None => format!("set MAGICIAN_CONFIG to a file and add `{key} = {example}` to it"),
    };
    match &config {
        Some(p) if p.is_file() => out.push(c("config", State::Ok, p.display().to_string(), None)),
        Some(p) => out.push(c("config", State::Ok, format!("none at {} (every location is found or defaulted; a line there overrides one)", p.display()), None)),
        None => out.push(c("config", State::Skip, "no location for one (set MAGICIAN_CONFIG)", None)),
    }

    // The game, its build and whether it mounts the addons folder.
    let game = paths::game(flag(args, "--game"));
    let build = match &game {
        Ok(g) => {
            out.push(c("game", State::Ok, format!("{} ({})", g.path.display(), g.source), None));
            match game_build(&g.path) {
                Ok(b) => {
                    out.push(c("build", State::Ok, format!("Deadlock build {b}"), None));
                    Some(b)
                }
                Err(e) => {
                    out.push(c("build", State::Fail, e, Some("verify the game's files in Steam".into())));
                    None
                }
            }
        }
        Err(e) => {
            out.push(c("game", State::Fail, e.clone(), Some(format!("pass --game <...\\Deadlock\\game>, set DEADLOCK_GAME, or {}", config_line("game", "<...>\\steamapps\\common\\Deadlock\\game")))));
            None
        }
    };
    if let Ok(g) = &game {
        let gi = g.path.join("citadel").join("gameinfo.gi");
        match std::fs::read_to_string(&gi).map_err(|e| e.to_string()).and_then(|t| install::add_addons_search_path(&t)) {
            Ok(None) => out.push(c("mounted", State::Ok, "gameinfo.gi loads citadel/addons", None)),
            Ok(Some(_)) => out.push(c("mounted", State::Todo, "gameinfo.gi does not load citadel/addons (a game update resets it)", Some("the next `magician install` adds it, with a backup".into()))),
            Err(e) => out.push(c("mounted", State::Fail, format!("{}: {e}", gi.display()), Some("verify the game's files in Steam".into()))),
        }
        let state = match install::game_running() {
            Ok(true) => {
                let holder = std::fs::read_to_string(paths::game_lock().path).ok().and_then(|t| install::lock_holder(&t));
                format!("running{}", holder.map(|h| format!(", held by {h} (game lock)")).unwrap_or_else(|| ", nobody holds the game lock (someone is playing): install waits for it to close".into()))
            }
            Ok(false) => "not running".into(),
            Err(e) => format!("can't tell ({e}); install and uninstall will ask for --ignore-game"),
        };
        out.push(c("running", State::Ok, state, None));
    }

    // The reference folder and what it holds for the installed build.
    let reference = paths::reference();
    out.push(c("reference", State::Ok, format!("{} ({})", reference.path.display(), reference.source), None));
    if let Some(b) = &build {
        let base = default_baseline_path(b);
        if base.is_file() {
            out.push(c("baseline", State::Ok, format!("build {b}: {}", base.display()), None));
        } else {
            out.push(c("baseline", State::Todo, format!("none for build {b} yet (lint reads the game live until there is; drift and patch day need it)"), Some("magician baseline   (a few seconds; a lint or check on this machine's game captures it too)".into())));
        }
        let rig = patchday::rig_path(&reference.path, b);
        if !rig.is_file() && base.is_file() {
            out.push(c("rig", State::Todo, format!("no rig sidecar for build {b}: drift can't name a renamed bone"), Some("magician baseline".into())));
        }
        if !patchday::engine_path(&reference.path, b).is_file() && base.is_file() {
            out.push(c("engine", State::Todo, format!("no engine sidecar for build {b}: the next patch day can't say whether the engine changed"), Some("magician patchday --capture   (writes only what is missing)".into())));
        }
        let users = users_index_path(b);
        if users.is_file() {
            out.push(c("users", State::Ok, format!("users index for build {b}"), None));
        } else {
            out.push(c("users", State::Skip, format!("no users index for build {b}: the shared-file rule (M090) does not run"), Some("magician index   (optional; a few minutes, needs Source 2 Viewer)".into())));
        }
        let on_record = patchday::builds_on_record(&reference.path);
        let older: Vec<&String> = on_record.iter().filter(|x| *x != b).collect();
        out.push(c("history", State::Ok, if older.is_empty() { "no earlier build on record (drift has nothing to compare from until the next patch)".to_string() } else { format!("earlier builds on record: {}", older.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")) }, None));
    }

    // The tools only some commands need.
    let vrf = paths::vrf(flag(args, "--vrf"));
    let vrf_at = if vrf.path.components().count() > 1 { Some(vrf.path.clone()).filter(|p| p.is_file()) } else { on_path(&vrf.path) };
    match vrf_at {
        Some(p) => out.push(c("viewer", State::Ok, format!("Source 2 Viewer CLI {} ({})", p.display(), vrf.source), None)),
        None => out.push(c("viewer", State::Skip, format!("no Source 2 Viewer CLI at {} ({}): grade, footprint and index need it", vrf.path.display(), vrf.source), Some(format!("install Source 2 Viewer 21.1's CLI, then {}", config_line("vrf", "<...>\\Source2Viewer-CLI.exe"))))),
    }
    match paths::csdk(flag(args, "--csdk")) {
        Ok(f) => {
            // the compiler is in one of the CSDK's bin folders (win64, win64_nm; build.rs picks per addon)
            let bin = f.path.join("game").join("bin");
            let has_rc = std::fs::read_dir(&bin).into_iter().flatten().flatten().any(|e| e.path().join("resourcecompiler.exe").is_file());
            if has_rc {
                out.push(c("csdk", State::Ok, format!("{} ({})", f.path.display(), f.source), None));
            } else {
                out.push(c("csdk", State::Fail, format!("{} ({}) has no resourcecompiler.exe in a folder under game\\bin", f.path.display(), f.source), Some("point it at the CSDK 13 folder that holds content\\ and game\\".into())));
            }
        }
        Err(_) => out.push(c("csdk", State::Skip, "no CSDK: only build, release and sign need it", Some(config_line("csdk", "<...>\\Reduced_CSDK_13")))),
    }
    match load_policy(args) {
        Ok(p) => out.push(c("policy", State::Ok, format!("{} ({})", p.file.as_deref().unwrap_or("built-in"), if p.overrides().is_empty() { "the defaults".to_string() } else { format!("{} key(s) moved", p.overrides().len()) }), None)),
        Err(e) => out.push(c("policy", State::Fail, e.text().to_string(), Some("fix the line it names, or remove `policy =` from the config".into()))),
    }

    // The mod of this folder.
    let ctx = match flag(args, "--mod") {
        Some(m) => ModCtx::load(Path::new(&m), "--mod").map(Some).unwrap_or_else(|e| {
            out.push(c("mod", State::Fail, e, None));
            None
        }),
        None => ModCtx::discover(),
    };
    match ctx {
        None => out.push(c("mod", State::Skip, "no mod.toml in this folder or above it", Some("magician init <pak>   (then run magician from that folder)".into()))),
        Some(m) => {
            out.push(c("mod", State::Ok, format!("{}: {} ({})", m.label(), m.toml.display(), m.found), None));
            let table = game.as_ref().ok().and_then(|g| heroes::Heroes::from_game(&g.path).ok());
            match (m.hero(), &table) {
                (None, _) => out.push(c("hero", State::Todo, "the mod.toml names no hero", Some("add hero = \"<hero>\" under [mod]".into()))),
                (Some(h), Some(t)) => match t.resolve(&h) {
                    Ok(x) => out.push(c("hero", State::Ok, x.label(), None)),
                    Err(e) => out.push(c("hero", State::Fail, e, Some("fix [mod] hero in the mod.toml".into()))),
                },
                (Some(h), None) => out.push(c("hero", State::Ok, format!("{h} (not checked: no hero table without the game)"), None)),
            }
            let paks = m.paks(game.as_ref().ok().map(|g| g.path.as_path()));
            if paks.is_empty() {
                out.push(c("paks", State::Todo, "none found: not in [mod] paks, not beside the mod.toml, not installed", Some("add paks = [\"pak33_dir.vpk\"] under [mod], or `magician release` / `install` it".into())));
            }
            for p in &paks {
                let state = if p.path.is_file() { State::Ok } else { State::Fail };
                out.push(c("pak", state, format!("{} ({}){}", p.path.display(), p.source, if state == State::Fail { ": not there" } else { "" }), None));
            }
            let kin = m.unclaimed_kin(game.as_ref().ok().map(|g| g.path.as_path()));
            if !kin.is_empty() {
                out.push(c("owners", State::Todo, format!("the install ledger holds files for {} too, which this mod.toml does not claim", kin.join(", ")), Some(format!("if they are this mod's: owners = [{}] under [mod]", kin.iter().map(|k| format!("\"{k}\"")).collect::<Vec<_>>().join(", ")))));
            }
            let csdk = paths::csdk(flag(args, "--csdk")).ok().map(|f| f.path);
            let (allow, _) = paths::mod_allowances(&m.toml, &m.text, csdk.as_deref());
            for a in &allow {
                out.push(c("allow", State::Ok, format!("{} ({})", a.path.display(), a.source), None));
            }
            for s in m.scenes() {
                out.push(c("scene", State::Ok, format!("{} ({})", s.path.display(), s.source), None));
            }
            // Proven is asked of the bytes: the mod's own paks, else what is installed for it (proofs.rs).
            let game_path = game.as_ref().ok().map(|g| g.path.as_path());
            let ledger = crate::proofs::Ledger::for_toml(&m.toml);
            let bytes = crate::proofs::mod_bytes(&m, game_path);
            match (ledger, bytes) {
                (Err(e), _) => out.push(c("proven", State::Fail, e, Some(format!("fix or remove {}", crate::proofs::FILE)))),
                (_, Err(e)) => out.push(c("proven", State::Todo, format!("the installed files can't be held to a proof: {e}"), Some("magician install --status; reinstall the mod".into()))),
                (Ok(l), Ok(files)) => {
                    let b = crate::proofs::Binding::of(&l, &m.proven(), &crate::proofs::shas(&files));
                    let note = b.unbound_note();
                    let note = if note.is_empty() { String::new() } else { format!("; {note}") };
                    let bad = if l.problems.is_empty() { String::new() } else { format!("; {} line(s) of {} do not read", l.problems.len(), crate::proofs::FILE) };
                    let fix = "the in-game pass, looked at, then magician prove --evidence <its folder>";
                    match (&build, b.last()) {
                        (Some(v), Some(_)) if b.holds(v) => out.push(c("proven", State::Ok, format!("these bytes on build {v} ({} in all){note}{bad}", b.bound.len()), None)),
                        (Some(v), Some(last)) => out.push(c("proven", State::Todo, format!("these bytes last on build {last}, not on {v}{note}{bad}"), Some(format!("magician check, then {fix}")))),
                        (_, None) => out.push(c("proven", State::Todo, format!("these bytes on no build yet{note}{bad}"), Some(fix.into()))),
                        (None, Some(last)) => out.push(c("proven", State::Ok, format!("these bytes last on build {last}{note}{bad}"), None)),
                    }
                }
            }
        }
    }
    out
}

pub fn cmd_doctor(args: &[String]) -> R {
    let checks = checks(args);
    let fail = checks.iter().filter(|c| c.state == State::Fail).count();
    let todo = checks.iter().filter(|c| c.state == State::Todo).count();
    let build = checks.iter().find(|c| c.name == "build" && c.state == State::Ok).map(|c| c.detail.clone());
    if json_mode() {
        let rows = checks.iter().map(|c| jsonout::Obj::new().str("name", c.name).str("state", &c.state.id().to_ascii_lowercase()).str("detail", &c.detail).opt_str("fix", c.fix.as_deref()).build());
        outln!("{}", jsonout::Obj::new().str("schema", "magician.doctor/1").str("toolchain", &cli::version()).int("failing", fail as i64).int("todo", todo as i64).raw("checks", jsonout::arr(rows)).build());
    } else {
        let verdict = if fail > 0 { style::paint(&format!("{fail} failing"), style::Tone::Bad) } else if todo > 0 { style::paint(&format!("{todo} to do"), style::Tone::Warn) } else { style::paint("ready", style::Tone::Good) };
        say!("magician doctor: {verdict}{}", build.map(|b| format!("  ({b})")).unwrap_or_default());
        say!("");
        for c in &checks {
            say!("  {} {:<10} {}", style::word(c.state.id(), 5), c.name, c.detail);
            if let Some(f) = &c.fix {
                if c.state != State::Ok {
                    say!("  {:<5} {:<10} {} {f}", "", "", style::paint("->", style::Tone::Dim));
                }
            }
        }
    }
    Ok(if fail > 0 { ExitCode::from(cli::BROKEN) } else { ExitCode::SUCCESS })
}
