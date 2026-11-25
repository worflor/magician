//! `magician patchday` and `magician prove`: the patch-day entry points (pure parts in patchday.rs).

use super::*;
use std::collections::BTreeMap;

/// `mod.toml` files for `patchday`: the `--mod` flags; else every mod under the mods folder when one is named
/// (`MAGICIAN_MODS`, `mods =`); else the mods of the nearest folder, from the working one upward (then from this
/// binary's), that has any one or two levels below it; else the mod.toml of the working folder (modctx.rs). So a
/// folder of mods is found from inside any of them, from the folder itself and from the one above it, whatever the
/// folders are called.
fn find_mod_tomls(args: &[String]) -> Vec<PathBuf> {
    let given = flags(args, "--mod");
    if !given.is_empty() {
        return given.into_iter().map(PathBuf::from).collect();
    }
    if let Some(f) = paths::mods() {
        return patchday::mod_tomls_under(&f.path);
    }
    let mut starts: Vec<PathBuf> = std::env::current_dir().into_iter().collect();
    if let Ok(exe) = std::env::current_exe() {
        starts.extend(exe.parent().map(Path::to_path_buf));
    }
    for s in starts {
        for dir in s.ancestors() {
            let v = patchday::mod_tomls_under(dir);
            if !v.is_empty() {
                return v;
            }
        }
    }
    // no folder of mods: the mod of this folder, if it is one
    crate::modctx::ModCtx::discover().map(|m| vec![m.toml]).unwrap_or_default()
}

/// A mod manifest as patch day reads it.
struct Manifest {
    path: PathBuf,
    text: String,
    slug: String,
    owners: Vec<String>,
    /// `[proven] builds`: the summary, which says nothing about bytes.
    claimed: Vec<String>,
    ledger: proofs::Ledger,
    /// The mod's files as installed, when every one the ledger holds for it reads as installed.
    files: Option<Vec<proofs::FileId>>,
    binding: proofs::Binding,
}

/// The files the install ledger holds for `owners`, with their bytes' sha256, when every one of them reads as the ledger
/// installed it; `None` when one is missing or replaced (the game then holds bytes no proof could speak for).
fn installed_files(ledger: &[install::Installed], rows: &[(String, String, Option<String>)], owners: &[String], sha: &BTreeMap<String, String>) -> Option<Vec<proofs::FileId>> {
    let mut v = Vec::new();
    for (r, (_, crc, disk)) in ledger.iter().zip(rows).filter(|(r, _)| owners.contains(&r.owner)) {
        if disk.as_deref() != Some(crc.as_str()) {
            return None;
        }
        v.push(proofs::FileId { owner: r.owner.clone(), file: r.name.clone(), crc: crc.clone(), sha256: sha.get(&r.name)?.clone() });
    }
    v.sort();
    Some(v)
}

/// Whether the engine changed between two builds, from their engine sidecars: a report line and its JSON. A build with
/// no sidecar is said to have none, never guessed at.
fn engine_report(reference: &Path, from: &str, to: &str) -> (String, String) {
    let j = jsonout::Obj::new().str("from", from).str("to", to);
    let (a, b) = (patchday::Engine::load(&patchday::engine_path(reference, from)), patchday::Engine::load(&patchday::engine_path(reference, to)));
    let (a, b) = match (a, b) {
        (Ok(a), Ok(b)) => (a, b),
        (a, _) => {
            let missing = if a.is_err() { from } else { to };
            return (format!("engine {from} -> {to}: no engine record for build {missing}, so whether the engine changed is not known"), j.str("state", "unknown").str("no_record", missing).build());
        }
    };
    let d = patchday::engine_delta(&a, &b);
    let unknown = if d.unknown.is_empty() { String::new() } else { format!(" ({} not known as shipped on one side: {})", d.unknown.len(), d.unknown.join(", ")) };
    let j = j.strs("changed", &d.changed).strs("added", &d.added).strs("removed", &d.removed).strs("unknown", &d.unknown);
    if d.same() {
        (format!("engine {from} -> {to}: data only: the same {} executables, libraries and shader archives{unknown}", b.files.len() - d.unknown.len()), j.str("state", "data-only").build())
    } else {
        let files = d.files();
        let shown: Vec<&str> = files.iter().take(8).map(String::as_str).collect();
        (
            format!(
                "engine {from} -> {to}: engine too: {} file(s) changed, added or removed: {}{}{unknown}. The mods' files are run by new code: look harder in game",
                files.len(),
                shown.join(", "),
                if files.len() > 8 { format!(" and {} more (--json lists them)", files.len() - 8) } else { String::new() }
            ),
            j.str("state", "engine-too").build(),
        )
    }
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
    // The engine sidecar: the executables and shader archives the data is run by.
    let epath = patchday::engine_path(&reference, &installed);
    let engine_state = if epath.is_file() {
        say!("  engine sidecar present: {}", epath.display());
        "present"
    } else if capture {
        say!("  {}", capture_engine(&game, &installed, &epath)?);
        "captured"
    } else {
        say!("  no engine sidecar {}: whether the engine changed can't be said next patch; `magician patchday --capture` writes it", epath.display());
        "missing"
    };
    // The hero data as text, kept per build like the baseline: the next patch compares each mod's hero against it.
    let hpath = patchday::hero_data_path(&reference, &installed);
    if capture && !hpath.is_file() {
        match grade_sources(args).and_then(|src| src.text("scripts/heroes.vdata_c")) {
            Ok(_) => say!("  hero data captured: {}", hpath.display()),
            Err(e) => say!("  hero data not captured ({e}): the next patch can't say what moved for a hero; it needs Source 2 Viewer"),
        }
    }
    let doc = doc
        .raw("baseline", baseline.build())
        .raw("rig", jsonout::Obj::new().str("state", rig_state).str("path", &rpath.display().to_string()).build())
        .raw("engine_sidecar", jsonout::Obj::new().str("state", engine_state).str("path", &epath.display().to_string()).build());

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
    // Each file is read once: its crc against the ledger's, and its sha256 for the proofs (a proof belongs to bytes).
    let mut sha: BTreeMap<String, String> = BTreeMap::new();
    let rows: Vec<(String, String, Option<String>)> = ledger
        .iter()
        .map(|r| {
            let p = ip.addons().join(&r.name);
            let bytes = if p.is_file() { std::fs::read(&p).ok() } else { None };
            let crc = bytes.as_ref().map(|b| format!("{:08x}", vpk::crc32(b)));
            if let Some(b) = &bytes {
                sha.insert(r.name.clone(), grade::sha256::hex(b));
            }
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

    // The mods' manifests: which owners each speaks for, its files as installed, and the builds a proof holds them on.
    let manifests: Vec<Manifest> = find_mod_tomls(args)
        .into_iter()
        .filter_map(|path| {
            let text = std::fs::read_to_string(&path).ok()?;
            let slug = patchday::mod_slug(&text).unwrap_or_else(|| path.display().to_string());
            let owners = patchday::mod_owners(&text);
            let claimed = patchday::proven_builds(&text);
            let proof_ledger = proofs::Ledger::for_toml(&path).unwrap_or_else(proofs::Ledger::unreadable);
            let files = installed_files(&ledger, &rows, &owners, &sha);
            let binding = proofs::Binding::of(&proof_ledger, &claimed, &files.as_deref().map(proofs::shas).unwrap_or_default());
            Some(Manifest { owners, claimed, ledger: proof_ledger, files, binding, slug, text, path })
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
                    Some(m) => patchday::era_for(&m.binding.bound, &builds, &installed),
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
    // Per era: did the engine change too, or only the data? It decides how hard to look in game, never the verdict.
    let mut era_list: Vec<String> = eras.keys().cloned().chain(std::iter::once(previous.clone())).filter(|e| *e != installed).collect();
    era_list.sort_by_key(|b| b.parse::<u64>().unwrap_or(0));
    era_list.dedup();
    let mut engine_json: Vec<String> = Vec::new();
    for from in &era_list {
        let (text, j) = engine_report(&reference, from, &installed);
        say!("  {text}");
        engine_json.push(j);
    }
    let doc = doc.raw("engine", jsonout::arr(engine_json));
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
    // Mods found here with nothing installed: listed, never judged, and the verdict says so.
    let mut idle: Vec<String> = Vec::new();
    let mut manifests_json: Vec<String> = Vec::new();
    let mut now_heroes: Option<Result<String, String>> = None;
    for m in &manifests {
        let is_installed = m.owners.iter().any(|o| by_owner.contains_key(o));
        let bound = &m.binding.bound;
        let first = bound.first().map(|f| f.parse::<u64>().unwrap_or(0)).unwrap_or(0);
        let of = builds.iter().filter(|b| b.parse::<u64>().unwrap_or(0) >= first).count().max(bound.len());
        let here = m.binding.holds(&installed);
        let unbound = m.binding.unbound_note();
        say!(
            "  {}: its installed bytes proven on {} of {of} builds on record ({}){}; build {installed}: {}{}",
            m.slug,
            bound.len(),
            m.binding.bound_text(),
            if unbound.is_empty() { String::new() } else { format!("; {unbound}") },
            if here { "proven" } else { "NOT proven" },
            if is_installed { "" } else { " (no installed pak in the ledger)" }
        );
        if is_installed && m.files.is_none() {
            say!("    its installed files do not all read as the install ledger has them ([2]), so no proof can hold them");
        }
        if let Some(files) = m.files.as_ref().filter(|_| !here) {
            for d in proofs::differences(&m.ledger, &installed, files) {
                say!("    {d}");
            }
        }
        for p in &m.ledger.problems {
            say!("    {}: {p} (the line counts for nothing)", proofs::FILE);
        }
        for (b, id) in m.ledger.incomplete() {
            say!("    {}: proof {id} of build {b} has lost a row (a merge?): it no longer adds up to its id and counts for nothing", proofs::FILE);
        }
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
        if !is_installed {
            idle.push(m.slug.clone());
        }
        // The hero's own data: did anything the skin can lean on move since the build the mod is compared from?
        let mut hero_json: Option<String> = None;
        let (era, _) = patchday::era_for(&m.binding.bound, &builds, &installed);
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
                .strs("proven", &m.binding.bound)
                .strs("claimed", &m.claimed)
                .strs("unbound", &m.binding.unbound())
                .str("proofs", &m.path.with_file_name(proofs::FILE).display().to_string())
                .strs("proof_problems", &m.ledger.problems)
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
        say!("  then, per mod: `magician session qa --mod <its folder> --out <dir>`, look at the frames, and `magician prove <mod.toml> --evidence <dir>` to record its installed bytes on build {installed}");
        "re-prove"
    } else {
        say!("  clear: every installed pak holds on {installed} and every installed mod is proven on it");
        "clear"
    };
    if !idle.is_empty() {
        idle.sort();
        say!("  not installed, so not judged: {} (`magician check <its folder>` judges a mod's own paks)", idle.join(", "));
    }
    if json_mode() {
        let listed: &[String] = if state == "re-prove" { &who } else { &[] };
        let verdict = jsonout::Obj::new().str("state", state).int("problems", hard as i64).strs("reprove", listed).strs("not_installed", &idle);
        outln!("{}", doc.raw("verdict", verdict.build()).build());
    }
    Ok(state_code(if state == "clear" { "clean" } else { state }))
}

/// `magician prove <mod.toml> [--build N] [--date YYYY-MM-DD] [--evidence DIR] [--pak FILE]...`: record that the
/// mod's bytes were seen working in game on a build. Run it only after the in-game pass for that build was looked at
/// (a green exit code is not proof; docs/PDD.md).
///
/// The proof goes into the mod's proof ledger (`proofs.tsv` beside its `mod.toml`, proofs.rs): every file the install
/// ledger holds for the mod's owners, as installed now (owner, name, crc, sha256), or the files `--pak` names (a
/// release copy proven earlier), and the session folder the pass was written into. The build is also added to
/// `[proven] builds`, the summary a person reads. Refused, with nothing written, when the mod has no installed file,
/// or what is installed is not what the install ledger says it installed.
pub fn cmd_prove(args: &[String]) -> R {
    let usage = "prove <mod.toml> [--build <build>] [--date YYYY-MM-DD] [--evidence DIR] [--pak FILE]...";
    let p = positional(args);
    let ctx = crate::modctx::ModCtx::load(Path::new(p.first().ok_or(usage)?.as_str()), "given")?;
    let game = game_dir(args);
    let installed = game.as_ref().ok().and_then(|g| game_build(g).ok());
    let build = match (flag(args, "--build"), &installed) {
        (Some(b), _) => b,
        (None, Some(b)) => b.clone(),
        (None, None) => return Err(Stop::Usage(format!("no installed game to read the build from ({}); pass --build", game.err().unwrap_or_default()))),
    };
    let date = match flag(args, "--date") {
        Some(d) if mark::scheme::parse_date(&d).is_some() => d,
        Some(d) => return Err(Stop::Usage(format!("--date {d} is not a date (YYYY-MM-DD)"))),
        None => mark::scheme::date_text(mark::scheme::today()),
    };
    let named = flags(args, "--pak");
    let files: Vec<proofs::FileId> = if named.is_empty() {
        let g = game.as_ref().map_err(|e| Stop::Usage(format!("{e}; pass --pak FILE to prove files that are not installed")))?;
        let owners = ctx.owners();
        let files = proofs::installed(g, &owners).map_err(|e| Stop::Refused(format!("{}: what is installed is not readable as the install ledger has it, so nothing was recorded: {e}", ctx.label())))?;
        if files.is_empty() {
            return Err(Stop::Refused(format!(
                "{}: the install ledger holds no file for {} in {}, so there are no bytes to prove; install the mod (`magician install`), run its in-game pass, then prove it (or name released files with --pak). Nothing was recorded",
                ctx.label(),
                owners.join(", "),
                g.display()
            )));
        }
        files
    } else {
        let mut v = Vec::new();
        for n in &named {
            let path = Path::new(n);
            v.push(proofs::named(path, &ctx.owner_for(path, game.as_deref().ok())).map_err(|e| Stop::Usage(format!("--pak {e}")))?);
        }
        v
    };
    let evidence = match flag(args, "--evidence") {
        None => None,
        Some(e) => {
            let path = std::path::absolute(Path::new(&e)).unwrap_or_else(|_| PathBuf::from(&e));
            let index = if path.is_dir() { path.join("index.md") } else { path.clone() };
            if !path.exists() {
                return Err(Stop::Usage(format!("--evidence {}: not there (the folder the in-game pass wrote, with its index.md, or one file of it)", path.display())));
            }
            let sha = if index.is_file() { Some(grade::sha256::file(&index).map_err(|err| format!("{}: {err}", index.display()))?) } else { None };
            Some((path.display().to_string(), sha))
        }
    };
    let text = std::fs::read_to_string(&ctx.toml).map_err(|e| format!("{}: {e}", ctx.toml.display()))?;
    // The summary first, so a mod.toml it cannot be written into refuses before anything changes.
    let summary = patchday::add_proven(&text, &build)?;
    let mut ledger = proofs::Ledger::for_toml(&ctx.toml)?;
    let before = ledger.text();
    let id = ledger.add(&build, &date, &files, evidence.as_ref().map(|(p, s)| (p.as_str(), s.as_deref()))).map_err(Stop::Refused)?;
    let lpath = ctx.toml.with_file_name(proofs::FILE);
    let fresh = ledger.text() != before;
    if fresh {
        install::write_atomic(&lpath, ledger.text().as_bytes()).map_err(|e| format!("{}: {e}", lpath.display()))?;
    }
    if summary != text {
        install::write_atomic(&ctx.toml, summary.as_bytes()).map_err(|e| format!("{}: {e}", ctx.toml.display()))?;
    }
    let claimed = patchday::proven_builds(&summary);
    let binding = proofs::Binding::of(&ledger, &claimed, &proofs::shas(&files));
    say!("{}: proof {id} of build {build} ({date}) {} {}", ctx.label(), if fresh { "recorded in" } else { "was already in" }, lpath.display());
    for f in &files {
        say!("  {:<20} {:<32} crc {}  sha256 {}", f.owner, f.file, f.crc, f.sha256);
    }
    if named.is_empty() {
        for o in ctx.owners().iter().filter(|o| !files.iter().any(|f| &f.owner == *o)) {
            say!("  note: nothing is installed for {o}, so this proof says nothing about its files");
        }
    }
    match &evidence {
        Some((p, Some(s))) if Path::new(p).is_dir() => say!("  evidence {p} (index.md sha256 {s})"),
        Some((p, Some(s))) => say!("  evidence {p} (sha256 {s})"),
        Some((p, None)) => say!("  evidence {p} (no index.md in it: only the path is recorded)"),
        None => say!("  no evidence folder named (--evidence DIR records the session the pass was written into)"),
    }
    if let Some(i) = installed.as_ref().filter(|i| **i != build) {
        say!("  note: build {build} is not the installed build {i}: recorded as given");
    }
    say!("  these bytes are proven on {}; [proven] builds lists {}{}", binding.bound_text(), claimed.join(", "), if binding.unbound().is_empty() { String::new() } else { format!("; {}", binding.unbound_note()) });
    if json_mode() {
        let rows = files.iter().map(|f| jsonout::Obj::new().str("owner", &f.owner).str("file", &f.file).str("crc", &f.crc).str("sha256", &f.sha256).build());
        let doc = jsonout::Obj::new()
            .str("schema", "magician.prove/1")
            .str("mod", &ctx.label())
            .str("build", &build)
            .str("date", &date)
            .str("proof", &id)
            .bool("recorded", fresh)
            .str("ledger", &lpath.display().to_string())
            .raw("files", jsonout::arr(rows))
            .opt_str("evidence", evidence.as_ref().map(|(p, _)| p.as_str()))
            .opt_str("evidence_sha256", evidence.as_ref().and_then(|(_, s)| s.as_deref()))
            .strs("proven", &binding.bound)
            .strs("unbound", &binding.unbound())
            .build();
        outln!("{doc}");
    }
    Ok(ExitCode::SUCCESS)
}
