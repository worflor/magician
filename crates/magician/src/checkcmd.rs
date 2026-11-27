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
//! when it loads but is not proven on this build, 0 when it is proven here and clean. A proof on this build answers a
//! re-prove, not a break: drift is still compared from the build these bytes were proven on before it. `--json` prints the same as one
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

/// The label's first word and what it means (`Echo`, "a new character"): said once beside the label, since the word
/// alone tells a first-time reader nothing.
fn kind_gloss(label: &str) -> Option<(&'static str, &'static str)> {
    use grade::identity::{means, word, Kind};
    let first = label.split_whitespace().next()?;
    [Kind::New, Kind::Same, Kind::SameModel, Kind::NoModel].into_iter().find(|k| word(*k) == first).map(|k| (word(k), means(k)))
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
    let mut stale: Vec<String> = Vec::new();
    drop(hint);
    match graded {
        Some(Ok(g)) => {
            // broken is the grade's own word for lint errors that are not fair play (`LintFacts::broken`)
            let lint_state = if g.broken { "broken" } else if g.lint_warnings > 0 { "warn" } else { "ok" };
            // fair play stops lint and install only when the policy enforces it, and then it is a gate blocker
            broken |= g.broken || g.blockers.iter().any(|b| b.starts_with("fair play"));
            // the grade counts the warnings that are not fair play; the line counts them all, as `magician lint` and the
            // rows under it do, so the numbers a reader adds up agree (the JSON keeps the grade's count)
            let warnings = g.kept.iter().filter(|f| f.severity == Severity::Warn).count();
            items.push(Item {
                what: "lint",
                state: lint_state,
                text: format!("{} error(s), {} warning(s), {} allowed{} ({}; fair play {})", g.lint_errors, warnings, g.lint_allowed, if g.lint_stale.is_empty() { String::new() } else { format!(", {} stale", g.lint_stale.len()) }, g.lint_depth, match g.fair_play { "fail" => style::mark("freeform"), "unknown" => "not checked".to_string(), _ => style::mark("ranked-safe") }),
                json: jsonout::Obj::new().str("state", lint_state).int("errors", g.lint_errors as i64).int("warnings", g.lint_warnings as i64).int("allowed", g.lint_allowed as i64).strs("stale", &g.lint_stale).str("depth", g.lint_depth).str("fair_play", g.fair_play).str("fair_play_name", match g.fair_play { "fail" => "freeform", "unknown" => "fair play not checked", _ => "ranked-safe" }).build(),
            });
            items.push(Item {
                what: "grade",
                state: if g.gate_passes { "ok" } else { "warn" },
                text: format!(
                    "{} for {}{}; release gate {}",
                    style::label(&g.label),
                    g.hero,
                    kind_gloss(&g.label).map(|(w, m)| format!(" ({w}: {m})")).unwrap_or_default(),
                    if g.gate_passes { "pass".to_string() } else { format!("blocked: {}", g.blockers.join("; ")) }
                ),
                json: jsonout::Obj::new().str("state", if g.gate_passes { "ok" } else { "blocked" }).str("label", &g.label).bool("gate", g.gate_passes).strs("blockers", &g.blockers).build(),
            });
            stale = g.lint_stale.clone();
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

    // The lint line's findings by rule: errors first, then the warnings by where a player meets them (on the hero,
    // effects and sound, other heroes, frame cost, loading, the pak alone), then fair play, which the policy judges apart.
    let tally = rule_tally(&kept);
    let fair = |t: &&Tally| kept.iter().any(|f| f.code == t.code && grade::rubric::is_compliance(f.code, &f.message));
    let mut by_rule: Vec<(&str, String)> = Vec::new();
    let errors: Vec<&Tally> = tally.iter().filter(|t| t.severity == Severity::Error && !fair(t)).collect();
    if !errors.is_empty() {
        by_rule.push(("errors", tally_line(&errors)));
    }
    let first_error: Option<&str> = errors.first().map(|t| t.code);
    let warned = tally.iter().any(|t| t.severity == Severity::Warn);
    for seen in lint::SEEN {
        let these: Vec<&Tally> = tally.iter().filter(|t| t.severity == Severity::Warn && !fair(t) && lint::seen(t.code) == *seen).collect();
        if !these.is_empty() {
            by_rule.push((seen, tally_line(&these)));
        }
    }
    // Fair play's advisory rules (M081 by default) are reported and never a violation: a row of their own, so a
    // "fair play pass" above a row of them reads as what it is.
    let policy = load_policy(args)?;
    let advisory = |t: &&Tally| policy.compliance.advisory.iter().any(|a| a == t.code);
    let fp: Vec<&Tally> = tally.iter().filter(|t| t.severity != Severity::Info && fair(t) && !advisory(t)).collect();
    if !fp.is_empty() {
        by_rule.push(("fair play", tally_line(&fp)));
    }
    if !stale.is_empty() {
        by_rule.push(("stale", format!("{} (an allowance no pak of the mod uses: remove it)", stale.join("; "))));
    }
    let adv: Vec<&Tally> = tally.iter().filter(|t| t.severity != Severity::Info && fair(t) && advisory(t)).collect();
    if !adv.is_empty() {
        by_rule.push(("advisory", format!("{} (fair play's advisories: reported, never a violation)", tally_line(&adv))));
    }
    let by_rule_json = jsonout::arr(tally.iter().map(|t| {
        jsonout::Obj::new()
            .str("code", t.code)
            .str("name", t.name)
            .str("severity", sev_label(t.severity).trim())
            .str("seen", lint::seen(t.code))
            .bool("fair_play", fair(&t))
            .bool("advisory", fair(&t) && advisory(&t))
            .int("count", t.count as i64)
            .int("files", t.files as i64)
            .build()
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
    // A mod nobody has proven anywhere: say what a proof is, rather than "proven on none" to someone who never heard of one.
    let never_proven = binding.bound.is_empty() && unbound.is_empty();
    let proof_text = format!(
        "{}{}{}",
        if never_proven { "not proven on any build yet (a proof is recorded once someone has seen these bytes work in game)".to_string() } else { format!("these bytes proven on {}", binding.bound_text()) },
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
    // Drift is compared from the last build these bytes are proven on. When that is the installed build, it is still
    // compared from the one proven before it: the proof here answers a re-prove (someone looked), never a break, which
    // only new bytes fix. With no earlier proof of these bytes there is nothing to compare.
    let mut drift_text = String::new();
    let drift_from: Option<&String> = if reprove { binding.last() } else { proven_before(&binding.bound, &build) };
    match drift_from {
        // nothing is wrong here, there is only nothing yet to compare: the proof line above carries the warning
        None if reprove => items.push(Item { what: "drift", state: "skip", text: "nothing to compare yet: drift starts from the first build these bytes are proven on".into(), json: jsonout::Obj::new().str("state", "unproven").build() }),
        None => items.push(Item { what: "drift", state: "ok", text: format!("proven on {build}, the installed build"), json: jsonout::Obj::new().str("state", "current").str("from", &build).str("to", &build).build() }),
        Some(from) => {
            if !default_baseline_path(&build).exists() {
                auto_capture(&game, &installed_baseline(&game.path)?);
            }
            let hint = progress::start(&format!("comparing {from} with {build}"));
            let gf = gamefiles::GameFiles::open(&game.path, None);
            let r = mod_drift(&paks.iter().map(|p| p.path.clone()).collect::<Vec<_>>(), from, &build, Some(&gf));
            drop(hint);
            let here = !reprove;
            match r {
                Err(e) if here => items.push(Item {
                    what: "drift",
                    state: "ok",
                    text: format!("proven on {build}, the installed build; whether anything broke since {from}, the build these bytes were proven on before, was not compared: {e}"),
                    json: jsonout::Obj::new().str("state", "current").str("from", from).str("to", &build).bool("proven_here", true).str("compared", "unknown").str("why", &e).build(),
                }),
                Err(e) => items.push(Item { what: "drift", state: "skip", text: format!("not compared from {from}: {e}"), json: jsonout::Obj::new().str("state", "unknown").str("from", from).str("to", &build).str("why", &e).build() }),
                Ok(md) => {
                    let (bad, changed, rig) = (md.bad(), md.changed(), md.rig());
                    let compared = md.state();
                    broken |= bad > 0;
                    // proven here: a re-prove or a clean comparison is answered by that proof, and reads as current
                    let state = if here && compared != "broken" { "current" } else { compared };
                    let text = match (compared, here) {
                        ("broken", false) => format!("BROKEN since {from}: {}", md.broken_text(3)),
                        ("broken", true) => format!("BROKEN since {from}: these bytes are proven on {build}, but their contract broke since {from}, and only new bytes fix that: {}", md.broken_text(3)),
                        ("re-prove", false) => format!("{from} -> {build}: loads; {changed} replaced original(s) and {rig} rig file(s) changed under it"),
                        ("re-prove", true) => format!("proven on {build}, the installed build; {from} -> {build}: {changed} replaced original(s) and {rig} rig file(s) changed under it, which that proof answers"),
                        (_, false) => format!("{from} -> {build}: nothing under it changed"),
                        (_, true) => format!("proven on {build}, the installed build; {from} -> {build}: nothing under it changed"),
                    };
                    for (p, d) in &md.paks {
                        drift_text.push_str(&format!("{}: {}\n", p.display(), d.verdict(from, &build)));
                    }
                    let mut j = jsonout::Obj::new().str("state", state).str("from", from).str("to", &build);
                    if here {
                        j = j.bool("proven_here", true).str("compared", compared);
                    }
                    items.push(Item {
                        what: "drift",
                        state: match state { "broken" => "broken", "re-prove" => "warn", _ => "ok" },
                        text,
                        json: j.int("problems", bad as i64).int("changed", changed as i64).int("rig", rig as i64).build(),
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
        // name only the list that holds what broke it: a drift with nothing to compare from has nothing to list
        let drift_broke = items.iter().any(|i| i.what == "drift" && i.state == "broken");
        let lint_broke = items.iter().any(|i| i.what == "lint" && i.state == "broken");
        // `lint` alone is light; name the run that lists what this check counted
        let lint = if light { "magician lint" } else { "magician lint --deep" };
        let lists = match (lint_broke, drift_broke, drift_from) {
            (true, true, Some(from)) => format!("`{lint}` lists every finding with its file, `magician drift --from {from}` every change"),
            (false, true, Some(from)) => format!("`magician drift --from {from}` lists every change"),
            _ => format!("`{lint}` lists every finding with its file"),
        };
        let rule = first_error.filter(|_| lint_broke).map(|c| format!("`magician rules {c}`")).unwrap_or_else(|| "`magician rules <code>`".into());
        next.push(format!("fix what is BROKEN above{}; {lists}, and {rule} says what a rule costs in game", if lint_broke { " (the errors row is what the game will not load or play)" } else { "" }));
    } else if reprove && never_proven {
        // A first look at a mod: lint is as far as the files go, and a mod author may want no more than that.
        next.push(format!(
            "nothing in its files breaks the mod{}. That is as far as lint can see: if it is what you wanted to know, you can stop here, and RE-PROVE says only that nobody has seen the mod in game on {build} yet. To prove it: the in-game pass (`magician session qa --mod {} --out <dir>`, which launches the game on this machine), a look at its frames, then `magician prove --evidence <dir>`",
            if warned { format!(", and its warnings are what a player might notice (`{}` lists them)", if light { "magician lint" } else { "magician lint --deep" }) } else { String::new() },
            ctx.dir.display()
        ));
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
                    say!("         {what:<14} {line}");
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
