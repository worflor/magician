//! `magician patchday` and `magician prove`: the patch-day entry points (pure parts in patchday.rs).

use super::*;
use std::collections::BTreeMap;

/// `mod.toml` files for `patchday`: the `--mod` flags, else every `model-replacements/*/mod.toml` found above the
/// working directory (or above this binary), else the mod.toml of the working folder (modctx.rs).
fn find_mod_tomls(args: &[String]) -> Vec<PathBuf> {
    let given = flags(args, "--mod");
    if !given.is_empty() {
        return given.into_iter().map(PathBuf::from).collect();
    }
    let mut starts: Vec<PathBuf> = std::env::current_dir().into_iter().collect();
    if let Ok(exe) = std::env::current_exe() {
        starts.extend(exe.parent().map(Path::to_path_buf));
    }
    for s in starts {
        for dir in s.ancestors() {
            let mr = dir.join("model-replacements");
            if mr.is_dir() {
                let mut v: Vec<PathBuf> = std::fs::read_dir(&mr).into_iter().flatten().flatten().map(|e| e.path().join("mod.toml")).filter(|p| p.is_file()).collect();
                v.sort();
                return v;
            }
        }
    }
    // no workspace of mods: the mod of this folder, if it is one
    crate::modctx::ModCtx::discover().map(|m| vec![m.toml]).unwrap_or_default()
}

/// A mod manifest as patch day reads it.
struct Manifest {
    path: PathBuf,
    text: String,
    slug: String,
    owners: Vec<String>,
    proven: Vec<String>,
}

/// The patch-day check: one command that answers "did the new build break our skins, and what must be re-proven".
/// Each installed mod is compared from its own last proven build (its `mod.toml`), so a mod proven on the installed
/// build is current. Exit 1 when something is broken, 4 when everything loads but something must be re-proven in
/// game, 0 when it is all clear. `--json` prints the same report as one document.
pub fn cmd_patchday(args: &[String]) -> R {
    let game = game_dir(args)?;
    let installed = game_build(&game)?;
    let reference = reference_root();
    let capture = args.iter().any(|a| a == "--capture");
    let mut hard = 0usize; // things that stop the check or break a mod: exit 1
    let doc = jsonout::Obj::new()
        .str("schema", "magician.patchday/1")
        .str("toolchain", &cli::version())
        .str("installed", &installed)
        .str("reference", &reference.display().to_string());
    say!("patch-day check: installed build {installed}, reference {}", reference.display());

    // 1. The baseline and rig sidecar of the installed build.
    say!("\n[1] baseline for build {installed}");
    let bpath = default_baseline_path(&installed);
    let rpath = patchday::rig_path(&reference, &installed);
    let live = installed_baseline(&game)?;
    let mut baseline = jsonout::Obj::new().str("path", &bpath.display().to_string());
    if !bpath.is_file() {
        if capture {
            write_era(&game, &live, &bpath)?;
            baseline = baseline.str("state", "captured").int("files", live.files.len() as i64);
        } else {
            say!("  MISSING {}\n  -> run `magician patchday --capture` (or `magician baseline`) once, before anything else", bpath.display());
            if json_mode() {
                let verdict = jsonout::Obj::new().str("state", "broken").int("problems", 1).raw("reprove", "[]".into());
                outln!("{}", doc.raw("baseline", baseline.str("state", "missing").build()).raw("verdict", verdict.build()).build());
            }
            return Ok(ExitCode::from(cli::BROKEN));
        }
    } else {
        let stored = Baseline::load(&bpath)?;
        let differ = live.files.iter().filter(|(p, f)| stored.get(p).is_none_or(|s| s.crc != f.crc)).count() + stored.files.keys().filter(|p| !live.contains(p)).count();
        if differ == 0 {
            say!("  ok: {} files, matches the installed game exactly", stored.files.len());
            baseline = baseline.str("state", "ok").int("files", stored.files.len() as i64);
        } else {
            say!("  STALE: {differ} file(s) differ between {} and the installed game (same build number, different bytes: a hotfix?). Not overwriting a baseline; capture to a new folder with `magician baseline --out FILE` and compare by hand", bpath.display());
            baseline = baseline.str("state", "stale").int("files", stored.files.len() as i64).int("differ", differ as i64);
            hard += 1;
        }
    }
    let rig_state = if !rpath.is_file() {
        if capture {
            write_rig(&game, &installed, &rpath)?;
            "captured"
        } else {
            say!("  no rig sidecar {}: bone and parameter changes can't be named next patch; `magician patchday --capture` writes it", rpath.display());
            "missing"
        }
    } else {
        say!("  rig sidecar present: {}", rpath.display());
        "present"
    };
    // The hero data as text, kept per build like the baseline: the next patch compares each mod's hero against it.
    let hpath = patchday::hero_data_path(&reference, &installed);
    if capture && !hpath.is_file() {
        match grade_sources(args).and_then(|src| src.text("scripts/heroes.vdata_c")) {
            Ok(_) => say!("  hero data captured: {}", hpath.display()),
            Err(e) => say!("  hero data not captured ({e}): the next patch can't say what moved for a hero; it needs Source 2 Viewer"),
        }
    }
    let doc = doc.raw("baseline", baseline.build()).raw("rig", jsonout::Obj::new().str("state", rig_state).str("path", &rpath.display().to_string()).build());

    // 2. Install state.
    say!("\n[2] install state");
    let ip = install_paths(args)?;
    let gi_text = std::fs::read_to_string(ip.gameinfo()).map_err(|e| format!("{}: {e}", ip.gameinfo().display()))?;
    let search_path = install::add_addons_search_path(&gi_text)?.is_none();
    if search_path {
        say!("  gameinfo.gi: addons search path present");
    } else {
        say!("  gameinfo.gi: addons search path MISSING (the update reset it). `magician install <pak>` for any one of the paks below re-applies the path and backs up the new stock file");
        hard += 1;
    }
    let bak = ip.backup(&installed);
    say!("  backup for build {installed}: {}", if bak.exists() { bak.display().to_string() } else { "none yet (the first install on this build takes it)".into() });
    // A pak another tool renamed (Deadlock Mod Manager renumbers them) is followed by its crc, read only here: it is
    // where the game loads it from, so its drift is checked under the name it has now.
    let mut ledger = install::read_installed(&ip.ledger());
    let moved = install::reconcile(&mut ledger, &ip.addons());
    for (old, new) in &moved {
        say!("  {old} is now {new} (the same bytes; renamed by another tool, so the ledger's name is out of date until the next install or uninstall)");
    }
    let rows: Vec<(String, String, Option<String>)> = ledger
        .iter()
        .map(|r| {
            let p = ip.addons().join(&r.name);
            let crc = if p.is_file() { std::fs::read(&p).ok().map(|b| format!("{:08x}", vpk::crc32(&b))) } else { None };
            (r.name.clone(), r.crc.clone(), crc)
        })
        .collect();
    let problems = patchday::ledger_problems(&rows);
    for (n, why) in &problems {
        say!("  {n}: {why}");
    }
    hard += problems.len();
    say!("  {} installed file(s) in the ledger, {} problem(s)", ledger.len(), problems.len());
    let ledger_json: Vec<String> = ledger
        .iter()
        .zip(&rows)
        .map(|(r, (_, crc, disk))| {
            let state = match disk {
                None => "missing",
                Some(d) if d != crc => "replaced",
                _ => "installed",
            };
            let from = moved.iter().find(|(_, new)| *new == r.name).map(|(old, _)| old.as_str());
            jsonout::Obj::new().str("name", &r.name).str("crc", &r.crc).str("owner", &r.owner).str("state", state).opt_str("moved_from", from).build()
        })
        .collect();
    let install_json = jsonout::Obj::new()
        .bool("search_path", search_path)
        .opt_str("backup", bak.exists().then(|| bak.display().to_string()).as_deref())
        .raw("ledger", jsonout::arr(ledger_json))
        .raw("problems", jsonout::arr(problems.iter().map(|(n, w)| jsonout::Obj::new().str("name", n).str("why", w).build())))
        .build();
    let doc = doc.raw("install", install_json);

    // The mods' manifests: which owners each speaks for, and where each was last proven.
    let manifests: Vec<Manifest> = find_mod_tomls(args)
        .into_iter()
        .filter_map(|path| {
            let text = std::fs::read_to_string(&path).ok()?;
            let slug = patchday::mod_slug(&text).unwrap_or_else(|| path.display().to_string());
            Some(Manifest { owners: patchday::mod_owners(&text), proven: patchday::proven_builds(&text), slug, text, path })
        })
        .collect();

    // 3. Drift of every installed pak, from the build its mod was last proven on.
    let builds = patchday::builds_on_record(&reference);
    let forced = flag(args, "--from");
    let forced_note = forced.as_deref().map(|f| format!(" (--from {f} for every mod)")).unwrap_or_default();
    say!("\n[3] drift: each mod from its last proven build -> installed {installed}{forced_note}");
    let now = Baseline::load(&bpath)?;
    let previous = patchday::default_from(&builds, &installed);
    if previous != installed {
        if let Ok(then) = Baseline::load(default_baseline_path(&previous)) {
            if then.get("scripts/heroes.vdata_c").map(|f| f.crc) != now.get("scripts/heroes.vdata_c").map(|f| f.crc) {
                say!("  note: scripts/heroes.vdata_c changed since {previous}: what moved for each mod's hero is under [4]");
            }
        }
    }
    let gf = gamefiles::GameFiles::open(&game, None);
    let mut eras: BTreeMap<String, Eras> = BTreeMap::new();
    let mut by_owner: BTreeMap<String, Vec<(String, usize, patchday::Drift)>> = BTreeMap::new();
    let mut owner_era: BTreeMap<String, (String, Option<String>)> = BTreeMap::new();
    for r in ledger.iter().filter(|r| rows.iter().any(|x| x.0 == r.name && x.2.is_some())) {
        let era = owner_era
            .entry(r.owner.clone())
            .or_insert_with(|| match &forced {
                Some(f) => (f.clone(), None),
                None => match manifests.iter().find(|m| m.owners.contains(&r.owner)) {
                    Some(m) => patchday::era_for(&m.proven, &builds, &installed),
                    None => (previous.clone(), Some(format!("no mod.toml claims this owner; compared from {previous}"))),
                },
            })
            .0
            .clone();
        if !eras.contains_key(&era) {
            eras.insert(era.clone(), load_eras(&era, &installed)?);
        }
        let p = ip.addons().join(&r.name);
        // one pak that does not read (damaged, or not a pak) is a problem in the report, not the end of the check
        let read = drift_vpk(&p, &eras[&era], Some(&gf)).and_then(|d| Ok((open(&p)?.entries.len(), d)));
        match read {
            Ok((n, d)) => by_owner.entry(r.owner.clone()).or_default().push((r.name.clone(), n, d)),
            Err(e) => {
                say!("  {}: can't be read ({e}); BROKEN: reinstall it", r.name);
                hard += 1;
            }
        }
    }
    let mut reprove: Vec<String> = Vec::new();
    let mut mods_json: Vec<String> = Vec::new();
    for (owner, paks) in &by_owner {
        let (from, note) = &owner_era[owner];
        let bad: usize = paks.iter().map(|(_, _, d)| d.bad()).sum();
        let changed: usize = paks.iter().map(|(_, _, d)| d.changed.len() + d.rig_deps().len() + d.params.len()).sum();
        let state = if bad > 0 { "BROKEN" } else if changed > 0 { "RE-PROVE" } else { "ok" };
        let since = match (note, forced.is_some()) {
            (None, false) if from == &installed => format!("proven on {installed}"),
            (None, false) => format!("last proven on {from}"),
            _ => format!("compared from {from}"),
        };
        say!("  {owner}: {state} ({since}){}", note.as_deref().map(|n| format!(": {}", n.split(';').next().unwrap_or(n))).unwrap_or_default());
        for (name, _, d) in paks {
            say!("    {name}: {}", d.verdict(from, &installed));
            let lines: Vec<String> = d
                .removed
                .iter()
                .map(|x| format!("replaced file Valve removed: {x}"))
                .chain(patchday::group_broken(&d.broken).into_iter().map(|x| format!("reference no longer resolves: {x}")))
                .chain(d.skeleton.iter().cloned())
                .chain(d.contract.iter().cloned())
                .collect();
            for l in lines.iter().take(8) {
                say!("      {l}");
            }
            if lines.len() > 8 {
                say!("      ... {} more: `magician drift {} --from {from}` lists them", lines.len() - 8, ip.addons().join(name).display());
            }
            for l in d.params.iter().take(4) {
                say!("      warn: {l}");
            }
        }
        hard += bad;
        if bad == 0 && changed > 0 {
            reprove.push(owner.replace('_', "-"));
        }
        mods_json.push(
            jsonout::Obj::new()
                .str("owner", owner)
                .str("state", &state.to_ascii_lowercase())
                .str("from", from)
                .opt_str("note", note.as_deref())
                .raw("paks", jsonout::arr(paks.iter().map(|(name, n, d)| jsonout::drift(name, *n, d, from, &installed))))
                .build(),
        );
    }
    let doc = doc.raw("mods", jsonout::arr(mods_json));

    // 4. Mod manifests: targets still exist, proven-on ledger.
    say!("\n[4] mods");
    if manifests.is_empty() {
        say!("  no mod.toml found (pass --mod <mod.toml>, or run from inside the workspace)");
    }
    let mut unproven: Vec<String> = Vec::new();
    let mut manifests_json: Vec<String> = Vec::new();
    let mut now_heroes: Option<Result<String, String>> = None;
    for m in &manifests {
        let is_installed = m.owners.iter().any(|o| by_owner.contains_key(o));
        let first = m.proven.first().map(|f| f.parse::<u64>().unwrap_or(0)).unwrap_or(0);
        let of = builds.iter().filter(|b| b.parse::<u64>().unwrap_or(0) >= first).count().max(m.proven.len());
        let here = m.proven.contains(&installed);
        say!(
            "  {}: proven on {} of {of} builds on record ({}); build {installed}: {}{}",
            m.slug,
            m.proven.len(),
            if m.proven.is_empty() { "none".to_string() } else { m.proven.join(", ") },
            if here { "proven" } else { "NOT proven" },
            if is_installed { "" } else { " (no installed pak in the ledger)" }
        );
        let mut missing: Vec<String> = Vec::new();
        for (k, p) in patchday::target_paths(&m.text) {
            // Only game resources: `materials_dir` is a folder, `catalog` an evidence file.
            if !patchday::is_game_resource(&p) {
                continue;
            }
            if !now.contains(&patchday::compiled_target(&p)) {
                say!("    target {k} {p}: not in build {installed}: Valve moved or removed it, the mod now replaces nothing{}", if is_installed { "" } else { " (not installed, not counted)" });
                missing.push(jsonout::Obj::new().str("key", &k).str("path", &p).build());
                if is_installed {
                    hard += 1;
                }
            }
        }
        if is_installed && !here {
            unproven.push(m.slug.clone());
        }
        // The hero's own data: did anything the skin can lean on move since the build the mod is compared from?
        let mut hero_json: Option<String> = None;
        let (era, _) = patchday::era_for(&m.proven, &builds, &installed);
        if let (Some(hero), true) = (patchday::mod_hero(&m.text), era != installed) {
            if now_heroes.is_none() {
                now_heroes = Some(std::fs::read_to_string(patchday::hero_data_path(&reference, &installed)).or_else(|_| grade_sources(args).and_then(|src| src.text("scripts/heroes.vdata_c"))));
            }
            let then_text = std::fs::read_to_string(patchday::hero_data_path(&reference, &era));
            let mut j = jsonout::Obj::new().str("hero", &hero).str("from", &era);
            match (then_text, now_heroes.as_ref().unwrap()) {
                (Ok(then), Ok(now_text)) if then == *now_text => {
                    j = j.str("state", "unchanged");
                }
                (Ok(then), Ok(now_text)) => match (patchday::hero_names(&then, &hero), patchday::hero_names(now_text, &hero)) {
                    (Some(a), Some(b)) => {
                        let c = patchday::hero_change(&a, &b);
                        if c.moved() {
                            say!("    hero data: {hero} since {era}: {} name(s) gone, {} new ({} the same). Check the skin still replaces what the hero uses: `magician footprint {hero}`", c.gone.len(), c.new.len(), c.same);
                            for g in c.gone.iter().take(8) {
                                say!("      - {g}");
                            }
                            for n in c.new.iter().take(8) {
                                say!("      + {n}");
                            }
                            if c.gone.len() > 8 || c.new.len() > 8 {
                                say!("      ... ({} gone, {} new in all; --json has every one)", c.gone.len(), c.new.len());
                            }
                        } else {
                            say!("    hero data: {hero} was edited since {era}; none of the {} names a skin leans on moved (model, abilities, sounds, particles)", c.same);
                        }
                        j = j.str("state", if c.moved() { "moved" } else { "names-unchanged" }).int("same", c.same as i64).strs("gone", &c.gone).strs("new", &c.new);
                    }
                    (_, None) => {
                        say!("    hero data: {hero} is not in build {installed}'s heroes.vdata: Valve removed or renamed the hero");
                        j = j.str("state", "hero-missing");
                    }
                    (None, Some(_)) => {
                        say!("    hero data: {hero} is new since {era}: nothing to compare");
                        j = j.str("state", "hero-new");
                    }
                },
                (Err(_), _) => {
                    say!("    hero data: none on record for build {era} (`patchday --capture` keeps it from now on); compare by hand with `magician footprint {hero}`");
                    j = j.str("state", "no-record");
                }
                (_, Err(e)) => {
                    say!("    hero data: could not read build {installed}'s ({e}); compare by hand with `magician footprint {hero}`");
                    j = j.str("state", "unreadable");
                }
            }
            hero_json = Some(j.build());
        }
        manifests_json.push(
            jsonout::Obj::new()
                .str("slug", &m.slug)
                .str("path", &m.path.display().to_string())
                .strs("owners", &m.owners)
                .strs("proven", &m.proven)
                .bool("proven_on_installed", here)
                .bool("installed", is_installed)
                .raw("missing_targets", jsonout::arr(missing))
                .raw("hero_data", hero_json.unwrap_or_else(|| "null".to_string()))
                .build(),
        );
    }
    let doc = doc.raw("manifests", jsonout::arr(manifests_json));

    say!("\nverdict");
    let mut who: Vec<String> = reprove.into_iter().chain(unproven).collect();
    who.sort();
    who.dedup();
    let state = if hard > 0 {
        say!("  BROKEN: {hard} problem(s) above. Fix them first (docs/PATCH-DAY.md, \"What each failure means\").");
        "broken"
    } else if !who.is_empty() {
        say!("  loads, but re-prove in game: {}", who.join(", "));
        say!("  then: `ingame\\qa_run.ps1 -Mod <slug> -Launch`, look at the frames, and `magician prove <mod.toml>` to record build {installed}");
        "re-prove"
    } else {
        say!("  clear: every installed pak holds on {installed} and every mod is proven on it");
        "clear"
    };
    if json_mode() {
        let listed: &[String] = if state == "re-prove" { &who } else { &[] };
        let verdict = jsonout::Obj::new().str("state", state).int("problems", hard as i64).strs("reprove", listed);
        outln!("{}", doc.raw("verdict", verdict.build()).build());
    }
    Ok(state_code(if state == "clear" { "clean" } else { state }))
}

/// `magician prove <mod.toml> [--build N]`: write a build into the mod's `[proven] builds`. Run it only after the
/// in-game pass for that build was looked at (a green exit code is not proof; docs/PDD.md).
pub fn cmd_prove(args: &[String]) -> R {
    let p = positional(args);
    let path = PathBuf::from(p.first().ok_or("prove <mod.toml> [--build <build>]")?);
    let build = match flag(args, "--build") {
        Some(b) => b,
        None => game_build(&game_dir(args)?)?,
    };
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let new = patchday::add_proven(&text, &build)?;
    if new == text {
        say!("{}: already proven on {build}", path.display());
    } else {
        std::fs::write(&path, &new).map_err(|e| format!("{}: {e}", path.display()))?;
        say!("{}: proven on {}", path.display(), patchday::proven_builds(&new).join(", "));
    }
    Ok(ExitCode::SUCCESS)
}
