//! `magician check`: everything Magician can say about one mod on the installed build, as one verdict.
//!
//! For the mod of the working folder (or the folder or `mod.toml` given): its paks are linted and graded together
//! (deep unless `--light`; a lint alone when Source 2 Viewer is not there to grade with), compared with the build it
//! was last proven on, and its scene patches are checked against the player's stock maps. The answer comes first, the
//! evidence after it, then what to do next:
//!
//! ```text
//! maid-lash: RE-PROVE  Costume (look, effects, sound) · stale(6864)  (build 6865)
//!   lint    0 errors, 272 warnings, 19 allowed (deep)
//!   ...
//! ```
//!
//! Exit 1 when something is broken (lint errors, a drift that breaks the mod, a scene patch that no longer fits), 4
//! when it loads but is not proven on this build, 0 when it is proven here and clean. `--json` prints the same as one
//! document (`magician.check/1`, the grade record inside it); `--verbose` adds the full grade and drift reports.

use super::*;
use crate::modctx::ModCtx;
use crate::style::{self, Tone};

/// One line of evidence: what was looked at, its state (`ok`, `warn`, `broken`, `skip`) and what was found.
struct Item {
    what: &'static str,
    state: &'static str,
    text: String,
    json: String,
}

/// Why a mod with no hero is not graded, in terms of what it is: `a prop mod (models/props/crate.vmdl)`.
pub(crate) fn no_hero_why(paks: &[PathBuf]) -> String {
    let files: Vec<String> = paks.iter().filter_map(|p| vpk::Vpk::open(p).ok()).flat_map(|v| v.entries.into_iter().map(|e| e.path)).collect();
    format!("{}; grades are for hero mods (`[mod] hero` names one if it is for a hero)", what_it_is(&files))
}

/// What a mod that is for no hero is, from its files: its kind, and for a prop the models it replaces.
pub(crate) fn what_it_is(files: &[String]) -> String {
    let kind = grade::identity::other_kind(files.iter());
    let models = grade::identity::models_of(files.iter());
    let indefinite = if kind.starts_with("effects") { "an" } else { "a" };
    if kind == "prop mod" && !models.is_empty() {
        let more = if models.len() > 3 { format!(" and {} more", models.len() - 3) } else { String::new() };
        format!("{indefinite} {kind} ({}{more})", models.iter().take(3).cloned().collect::<Vec<_>>().join(", "))
    } else if kind == "not a hero mod" {
        kind
    } else {
        format!("{indefinite} {kind}")
    }
}

pub fn cmd_check(args: &[String]) -> R {
    let pos = positional(args);
    let ctx = match pos.first().map(|p| p.as_str()).or(flag(args, "--mod").as_deref().map(|_| "")) {
        Some("") => ModCtx::load(Path::new(&flag(args, "--mod").unwrap_or_default()), "--mod")?,
        Some(p) => ModCtx::load(Path::new(p), "given")?,
        None => ModCtx::discover().ok_or_else(|| {
            Stop::Usage("no mod.toml in this folder or above it: run `magician check` from a mod's folder, give one (`magician check <folder or mod.toml>`), or write one with `magician init <pak>`".into())
        })?,
    };
    let verbose = args.iter().any(|a| a == "--verbose");
    let game = paths::game(flag(args, "--game"))?;
    let build = game_build(&game.path)?;
    let paks = ctx.paks(Some(&game.path));
    if paks.is_empty() {
        // A mod started with `magician new` has sources and no pak yet: say that, and how to make the first one.
        let has_addon = ctx.dir.join("addon").is_dir() || grade::manifest::string(&ctx.text, "mod", "addon").is_some();
        return Err(Stop::Usage(if has_addon {
            format!("{} has no pak yet, so there is nothing to check: build the first one from its addon with `magician release --out pak10_dir.vpk` in {} (it lands beside the mod.toml, where check finds it)", ctx.label(), ctx.dir.display())
        } else {
            format!("{} names no pak: list it under [mod] as paks = [...], put it beside its mod.toml, or install it", ctx.label())
        }));
    }
    errln!("magician: {}: {} ({})", ctx.label(), ctx.toml.display(), ctx.found);
    for p in &paks {
        errln!("magician:   pak {} ({})", p.path.display(), p.source);
    }
    // The other commands' arguments: the ones this run was given that they take, and the mod's own.
    let mut common: Vec<String> = Vec::new();
    for k in ["--game", "--policy", "--csdk", "--vrf", "--baseline"] {
        if let Some(v) = flag(args, k) {
            common.extend([k.to_string(), v]);
        }
    }
    let csdk = paths::csdk(flag(args, "--csdk")).ok().map(|f| f.path);
    let allow: Vec<PathBuf> = paths::mod_allowances(&ctx.toml, &ctx.text, csdk.as_deref()).0.into_iter().map(|f| f.path).collect();
    let mut items: Vec<Item> = Vec::new();
    let mut broken = false;
    let mut label: Option<String> = None;
    let mut grade_json = "null".to_string();
    let mut grade_text = String::new();
    let mut gate: Option<(bool, Vec<String>)> = None;
    // Every finding no allowance accepted, for the lint line's breakdown by rule.
    let mut kept: Vec<lint::Finding> = Vec::new();

    // 1. Lint and grade, together: the grade lints (deep unless --light) and its record carries the counts.
    QUIET.store(true, Ordering::Relaxed);
    let light = args.iter().any(|a| a == "--light");
    let grade_args = {
        let mut a: Vec<String> = paks.iter().map(|p| p.path.to_string_lossy().into_owned()).collect();
        a.extend(["--mod".into(), ctx.toml.to_string_lossy().into_owned()]);
        if light {
            a.push("--light".into());
        }
        a.extend(common.iter().cloned());
        a
    };
    let hint = progress::start(&format!("grading {} ({} lint, footprint)", ctx.label(), if light { "light" } else { "deep" }));
    let graded = if ctx.hero().is_some() { Some(grade_run(&grade_args)) } else { None };
    drop(hint);
    match graded {
        Some(Ok(g)) => {
            // broken is the grade's own word for lint errors that are not fair play (`LintFacts::broken`)
            let lint_state = if g.broken { "broken" } else if g.lint_warnings > 0 { "warn" } else { "ok" };
            // fair play stops lint and install only when the policy enforces it, and then it is a gate blocker
            broken |= g.broken || g.blockers.iter().any(|b| b.starts_with("fair play"));
            items.push(Item {
                what: "lint",
                state: lint_state,
                text: format!("{} error(s), {} warning(s), {} allowed ({}; fair play {})", g.lint_errors, g.lint_warnings, g.lint_allowed, g.lint_depth, style::mark(match g.fair_play { "fail" => "NSFR", "unknown" => "FP?", x => x })),
                json: jsonout::Obj::new().str("state", lint_state).int("errors", g.lint_errors as i64).int("warnings", g.lint_warnings as i64).int("allowed", g.lint_allowed as i64).str("depth", g.lint_depth).str("fair_play", g.fair_play).build(),
            });
            items.push(Item {
                what: "grade",
                state: if g.gate_passes { "ok" } else { "warn" },
                text: format!("{} for {}; release gate {}", style::label(&g.label), g.hero, if g.gate_passes { "pass".to_string() } else { format!("blocked: {}", g.blockers.join("; ")) }),
                json: jsonout::Obj::new().str("state", if g.gate_passes { "ok" } else { "blocked" }).str("label", &g.label).bool("gate", g.gate_passes).strs("blockers", &g.blockers).build(),
            });
            label = Some(g.label.clone());
            gate = Some((g.gate_passes, g.blockers.clone()));
            grade_json = g.json.trim().to_string();
            grade_text = g.text;
            kept = g.kept;
        }
        other => {
            // No grade (no hero named, or the grade could not run: most often no Source 2 Viewer): lint each pak.
            let why = match other {
                Some(Err(e)) => e.text().to_string(),
                _ => no_hero_why(&paks.iter().map(|p| p.path.clone()).collect::<Vec<_>>()),
            };
            let (mut errors, mut warnings, mut allowed) = (0, 0, 0);
            for p in &paks {
                let mut a: Vec<String> = vec![p.path.to_string_lossy().into_owned()];
                if !light {
                    a.push("--deep".into());
                }
                for f in &allow {
                    a.extend(["--allow".into(), f.to_string_lossy().into_owned()]);
                }
                a.extend(common.iter().filter(|x| *x != "--csdk" && *x != "--vrf").cloned());
                let hint = progress::start(&format!("linting {}", p.path.display()));
                let o = lint_run(&p.path, &a);
                drop(hint);
                let o = o.inspect_err(|_| QUIET.store(false, Ordering::Relaxed))?;
                errors += o.errors();
                warnings += o.warnings();
                allowed += o.r.allowed.len();
                kept.extend(o.r.kept);
            }
            broken |= errors > 0;
            let state = if errors > 0 { "broken" } else if warnings > 0 { "warn" } else { "ok" };
            items.push(Item {
                what: "lint",
                state,
                text: format!("{errors} error(s), {warnings} warning(s), {allowed} allowed ({})", if light { "light" } else { "deep" }),
                json: jsonout::Obj::new().str("state", state).int("errors", errors as i64).int("warnings", warnings as i64).int("allowed", allowed as i64).str("depth", if light { "light" } else { "deep" }).build(),
            });
            items.push(Item { what: "grade", state: "skip", text: format!("not graded: {why}"), json: jsonout::Obj::new().str("state", "skip").str("why", &why).build() });
        }
    }

    // The lint line's findings by rule: what is not fair play by severity, then fair play, which the policy judges apart.
    let tally = rule_tally(&kept);
    let fair = |t: &&Tally| kept.iter().any(|f| f.code == t.code && grade::rubric::is_compliance(f.code, &f.message));
    let mut by_rule: Vec<(&str, String)> = Vec::new();
    for (what, pick) in [("errors", Severity::Error), ("warnings", Severity::Warn)] {
        let these: Vec<&Tally> = tally.iter().filter(|t| t.severity == pick && !fair(t)).collect();
        if !these.is_empty() {
            by_rule.push((what, tally_line(&these)));
        }
    }
    let fp: Vec<&Tally> = tally.iter().filter(|t| t.severity != Severity::Info && fair(t)).collect();
    if !fp.is_empty() {
        by_rule.push(("fair play", tally_line(&fp)));
    }
    let by_rule_json = jsonout::arr(tally.iter().map(|t| {
        jsonout::Obj::new().str("code", t.code).str("name", t.name).str("severity", sev_label(t.severity).trim()).bool("fair_play", fair(&t)).int("count", t.count as i64).int("files", t.files as i64).build()
    }));
    if let Some(i) = items.iter_mut().find(|i| i.what == "lint") {
        i.json = format!("{},\"by_rule\":{by_rule_json}}}", &i.json[..i.json.len() - 1]);
    }

    // 2. Proof and drift, asked of the bytes judged: the mod's own paks, else every file installed for it (proofs.rs).
    let (bytes, unreadable) = match proofs::mod_bytes(&ctx, Some(&game.path)) {
        Ok(b) => (b, None),
        Err(e) => (Vec::new(), Some(e)),
    };
    let ledger = proofs::Ledger::for_toml(&ctx.toml)?;
    let binding = proofs::Binding::of(&ledger, &ctx.proven(), &proofs::shas(&bytes));
    let mut reprove = !binding.holds(&build);
    let unbound = binding.unbound_note();
    let mut proof_lines: Vec<String> = Vec::new();
    if let Some(e) = &unreadable {
        proof_lines.push(format!("its installed files do not read as installed ({e}): no proof can hold them"));
    }
    if reprove {
        proof_lines.extend(proofs::differences(&ledger, &build, &bytes));
    }
    proof_lines.extend(ledger.problems.iter().map(|p| format!("{}: {p} (the line counts for nothing)", proofs::FILE)));
    proof_lines.extend(ledger.incomplete().into_iter().map(|(b, id)| format!("{}: proof {id} of build {b} lost a row (a merge?) and counts for nothing", proofs::FILE)));
    let proof_text = format!(
        "these bytes proven on {}{}{}",
        binding.bound_text(),
        if unbound.is_empty() { String::new() } else { format!("; {unbound}") },
        if proof_lines.is_empty() { String::new() } else { format!("; {}", proof_lines.join("; ")) }
    );
    items.push(Item {
        what: "proof",
        state: if !reprove { "ok" } else { "warn" },
        text: proof_text,
        json: jsonout::Obj::new()
            .str("state", if !reprove { "current" } else if binding.bound.is_empty() { "unproven" } else { "stale" })
            .strs("proven", &binding.bound)
            .strs("unbound", &binding.unbound())
            .strs("notes", &proof_lines)
            .raw("files", jsonout::arr(bytes.iter().map(|f| jsonout::Obj::new().str("owner", &f.owner).str("file", &f.file).str("crc", &f.crc).str("sha256", &f.sha256).build())))
            .build(),
    });
    let mut drift_text = String::new();
    match binding.last() {
        None => items.push(Item { what: "drift", state: "warn", text: "these bytes are proven on no build yet: nothing to compare from".into(), json: jsonout::Obj::new().str("state", "unproven").build() }),
        Some(_) if !reprove => items.push(Item { what: "drift", state: "ok", text: format!("proven on {build}, the installed build"), json: jsonout::Obj::new().str("state", "current").str("from", &build).str("to", &build).build() }),
        Some(from) => {
            if !default_baseline_path(&build).exists() {
                auto_capture(&game, &installed_baseline(&game.path)?);
            }
            let hint = progress::start(&format!("comparing {from} with {build}"));
            let eras = load_eras(from, &build);
            let r = eras.and_then(|eras| {
                let gf = gamefiles::GameFiles::open(&game.path, None);
                paks.iter().map(|p| drift_vpk(&p.path, &eras, Some(&gf)).map(|d| (p.path.clone(), d))).collect::<Result<Vec<_>, String>>()
            });
            drop(hint);
            match r {
                Err(e) => items.push(Item { what: "drift", state: "skip", text: format!("not compared from {from}: {e}"), json: jsonout::Obj::new().str("state", "unknown").str("from", from).str("to", &build).str("why", &e).build() }),
                Ok(ds) => {
                    let bad: usize = ds.iter().map(|(_, d)| d.bad()).sum();
                    let changed: usize = ds.iter().map(|(_, d)| d.changed.len()).sum();
                    let rig: usize = ds.iter().map(|(_, d)| d.rig_deps().len()).sum();
                    let state = if bad > 0 { "broken" } else if changed + rig > 0 { "re-prove" } else { "clean" };
                    broken |= bad > 0;
                    let text = match state {
                        "broken" => {
                            let mut first: Vec<String> = ds.iter().flat_map(|(_, d)| d.removed.iter().map(|x| format!("Valve removed {x}")).chain(patchday::group_broken(&d.broken)).chain(d.skeleton.iter().cloned()).chain(d.contract.iter().cloned())).collect();
                            let more = first.len().saturating_sub(3);
                            first.truncate(3);
                            format!("BROKEN since {from}: {}{}", first.join("; "), if more > 0 { format!("; and {more} more") } else { String::new() })
                        }
                        "re-prove" => format!("{from} -> {build}: loads; {changed} replaced original(s) and {rig} rig file(s) changed under it"),
                        _ => format!("{from} -> {build}: nothing under it changed"),
                    };
                    for (p, d) in &ds {
                        drift_text.push_str(&format!("{}: {}\n", p.display(), d.verdict(from, &build)));
                    }
                    items.push(Item {
                        what: "drift",
                        state: if state == "clean" { "ok" } else if state == "broken" { "broken" } else { "warn" },
                        text,
                        json: jsonout::Obj::new().str("state", state).str("from", from).str("to", &build).int("problems", bad as i64).int("changed", changed as i64).int("rig", rig as i64).build(),
                    });
                }
            }
        }
    }

    // 3. Scene patches against the player's stock maps.
    for s in ctx.scenes() {
        let name = s.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let r = scene::load_patch(&s.path).and_then(|patch| scene::stock_map(&game.path, &patch).and_then(|v| scene::assemble(&patch, &v, &s.path, &build)).map(|_| patch));
        match r {
            Ok(p) => items.push(Item { what: "scene", state: "ok", text: format!("{}: the stock map matches the patch", p.hero), json: jsonout::Obj::new().str("patch", &s.path.display().to_string()).str("state", "ok").build() }),
            Err(e) => {
                broken = true;
                let first = e.lines().next().unwrap_or("").to_string();
                items.push(Item { what: "scene", state: "broken", text: format!("{name}: {first}"), json: jsonout::Obj::new().str("patch", &s.path.display().to_string()).str("state", "broken").str("why", &e).build() });
            }
        }
    }
    QUIET.store(false, Ordering::Relaxed);

    // The verdict, and what to do next.
    reprove &= !broken;
    let verdict = if broken { "BROKEN" } else if reprove { "RE-PROVE" } else { "ok" };
    let mut next: Vec<String> = Vec::new();
    if broken {
        next.push("fix what is BROKEN above; `magician lint` and `magician drift` list every finding".into());
    } else if reprove {
        let quiet_patch = items.iter().any(|i| i.what == "drift" && i.state == "ok");
        next.push(format!(
            "{}the in-game pass on build {build} (`magician session qa --mod {} --out <dir>`), look at the frames, then `magician prove --evidence <dir>`",
            if quiet_patch { "nothing under it changed since its last proven build, so a quick look: " } else { "" },
            ctx.dir.display()
        ));
    }
    let kin = ctx.unclaimed_kin(Some(&game.path));
    if !kin.is_empty() {
        next.push(format!(
            "the install ledger also holds files for {}, which this mod.toml does not claim; if they are its paks, add owners = [{}] under [mod] so they are graded and drift-checked with it",
            kin.join(", "),
            kin.iter().map(|k| format!("\"{k}\"")).collect::<Vec<_>>().join(", ")
        ));
    }
    if let Some((false, b)) = &gate {
        if !broken && b.iter().any(|x| !x.starts_with("freshness")) {
            next.push("the release gate has blockers besides freshness: `magician grade` lists them".into());
        }
    }
    if json_mode() {
        let doc = jsonout::Obj::new()
            .str("schema", "magician.check/1")
            .str("toolchain", &cli::version())
            .str("mod", &ctx.label())
            .str("toml", &ctx.toml.display().to_string())
            .str("build", &build)
            .str("verdict", &verdict.to_ascii_lowercase())
            .opt_str("label", label.as_deref())
            .strs("paks", &paks.iter().map(|p| p.path.display().to_string()).collect::<Vec<_>>())
            .raw("steps", jsonout::arr(items.iter().map(|i| format!("{{\"step\":{},{}", jsonout::s(i.what), &i.json[1..]))))
            .raw("grade", grade_json)
            .strs("next", &next)
            .build();
        outln!("{doc}");
    } else {
        let tone = match verdict { "BROKEN" => Tone::Bad, "RE-PROVE" => Tone::Warn, _ => Tone::Good };
        say!("{}: {}  {}(build {build})", ctx.label(), style::paint(verdict, tone), label.as_deref().map(|l| format!("{}  ", style::label(l))).unwrap_or_default());
        for i in &items {
            say!("  {:<6} {} {}", i.what, style::word(i.state, 6), i.text);
            if i.what == "lint" {
                for (what, line) in &by_rule {
                    say!("         {what:<9} {line}");
                }
            }
        }
        for n in &next {
            say!("next: {n}");
        }
        if verbose {
            if !grade_text.is_empty() {
                say!("\n{}", grade_text.trim_end());
            }
            if !drift_text.is_empty() {
                say!("\n{}", drift_text.trim_end());
            }
        }
    }
    Ok(ExitCode::from(if broken { cli::BROKEN } else if reprove { cli::REPROVE } else { cli::CLEAN }))
}
