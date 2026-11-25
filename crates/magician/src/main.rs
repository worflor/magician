//! magician: Deadlock mod toolkit CLI.

use lint::{Baseline, Input, ModFile, Severity, RULES};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};

/// Human-readable output. `--json` turns it off: stdout then carries one JSON document and nothing else.
static TEXT: AtomicBool = AtomicBool::new(true);
/// Set while one command runs another's work for its result (`check` running a grade): their reports stay unsaid.
static QUIET: AtomicBool = AtomicBool::new(false);

/// `println!` for the human report: silent under `--json`. Defined before the modules so they can use it.
macro_rules! say {
    ($($t:tt)*) => {
        if crate::TEXT.load(std::sync::atomic::Ordering::Relaxed) && !crate::QUIET.load(std::sync::atomic::Ordering::Relaxed) {
            outln!($($t)*)
        }
    };
}

/// `println!` and `print!` that survive a reader closing the pipe (`magician ... | head`): see out.rs.
macro_rules! outln {
    () => {
        crate::out::stdout("\n")
    };
    ($($t:tt)*) => {
        crate::out::stdout(&format!("{}\n", format_args!($($t)*)))
    };
}
macro_rules! out {
    ($($t:tt)*) => {
        crate::out::stdout(&format!($($t)*))
    };
}
/// `eprintln!` the same way.
macro_rules! errln {
    ($($t:tt)*) => {
        crate::out::stderr(&format!("{}\n", format_args!($($t)*)))
    };
}

mod build;
mod cli;
mod checkcmd;
mod clothcmd;
mod doctor;
mod frontdoor;
mod gamefiles;
mod heroes;
mod initcmd;
mod install;
mod jsonout;
mod lock;
mod markcmd;
mod meshcmd;
mod modctx;
mod newcmd;
mod out;
mod patchcmd;
mod patchday;
mod paths;
mod progress;
mod proofs;
mod scene;
mod style;
mod update;
mod updateapply;

const USAGE: &str = "\
Magician is a headless toolchain for creating, validating, and maintaining Deadlock mods through automation, scripts, and AI agents.

Start here:
  magician doctor [--mod mod.toml] [--updates] [--source LOCATION] [--json]
                                                this machine's setup, one line each with its fix: the toolchain, the game
                                                and its build, the baseline, the viewer, the CSDK, the policy, the mod of
                                                this folder; --updates adds the update check (the only thing doctor
                                                does online; without it doctor never touches the network)
  magician new <hero> [--slug NAME] [--dir DIR] [--name NAME] [--author NAME]
                                                start a skin from nothing: a mod folder (DIR/<slug>, default
                                                <hero>-skin) with a mod.toml whose [target] is the hero's real model and
                                                skeleton, an addon folder at the game's paths with .gitignore files that
                                                keep Valve-derived files out of git, a package for tools/bundle.py, two
                                                in-game scenarios and a README with the next commands; never writes into
                                                a folder that exists, never copies a Valve file
  magician init [<pak> | <addon dir>] [--out DIR] [--hero HERO] [--slug SLUG] [--name NAME] [--force]
                                                write a mod.toml for a pak (or the paks here) or an addon's sources: the
                                                hero from the files it replaces, slug and name from the folder
  magician check [<folder> | <mod.toml>] [--light] [--verbose] [--json]
                                                the mod's one verdict on the installed build: its paks linted and graded
                                                together, drift since its last proven build, its scene patches; exit 1
                                                broken, 4 re-prove (not proven on this build), 0 clean

The toolkit's Python, through the same door (everything after the verb is the tool's own, --help included, with its
stdio and exit code; the toolkit folder is MAGICIAN_HOME, `home =`, else the one above this binary holding
ingame/mod_session.py; Python is MAGICIAN_PYTHON, `python =`, else python, python3 (or py -3 on Windows); exit 2 when
either is not there):
  magician session qa --mod <folder> --out <dir> [scenario]...
                                                ingame/mod_session.py: a mod's in-game session (up, run, film, down,
                                                all, qa); `magician session --help` for the rest
  magician reprove [--mod <folder>]... [--plan] | reprove accept <pass dir>
                                                ingame/reprove.py: patch day in game: the patch-day check, then each mod
                                                that needs it gets a vanilla pass (when the build has no baseline) and
                                                its own pass, and one report gives each a state (exit 0 ready, 4 look,
                                                1 failed); `accept` records a pass that was looked at with `prove`
  magician bundle <bundle.json> [--zip] [--out DIR]
                                                tools/bundle.py: a player release bundle (mod-interchange.json, files,
                                                previews, the scene installers); `magician bundle --help`
  magician source <source.json> [--out DIR] [--prune]
                                                tools/source.py: a static Deadlock Mod Manager mod source from bundles;
                                                `magician source --help`

Inside a mod's folder (one with a mod.toml, or below one) lint, grade, drift, release, install, uninstall, prove,
footprint and scene check|install need nothing after the verb: the paks, hero, addon, allowances, last proven build
and scene patches come from the mod.toml (its [mod] paks, else the paks beside it, else the ones installed for it),
and what was used is said on stderr. Anything given explicitly wins. A hero is named any way: lash, Lash, hero_lash,
\"Lady Geist\" or its model's path.

Every command:
  magician baseline [--game DIR] [--out FILE]   snapshot the installed build's file table (citadel + core) and, beside it,
                                                rig.tsv (every skeleton's bones, every anim graph's control parameters)
                                                and engine.tsv (size and sha256 of every executable, library and shader
                                                archive the build ships, and gameinfo.gi as shipped)
  magician ls <vpk> [prefix]                    list files: path, crc, size
  magician refs <vpk> <path>                    what a compiled resource depends on, and whether each resolves
  magician dump <vpk> <path> [--block TAG]      a compiled file's KV3 (DATA by default) as text, or a texture header
  magician dump --text FILE                     re-print a KV3 text file (e.g. a decompile) in the same form, to diff
  magician index [--game DIR]                   build the users index (who uses each game file) for shared-file lint
  magician kv <vpk> <prefix> <key> [--block TAG]  every scalar under a key containing <key>, across files (evidence)
  magician textures <vpk> [prefix] [--list]     texture formats and non-power-of-two sizes, per top folder
  magician scan <vpk> [prefix]                  parse every compiled resource under a prefix: parse failures and
                                                references that don't resolve
  magician mesh <vpk> <model path> [--against valve|none] [--with <vpk>]... [--out DIR] [--glb FILE] [--no-png]
                                                a compiled model's mesh verdict against Valve's (mesh rules M100-M147),
                                                silhouettes, x-ray sheet; --glb proves the native decode against an export
  magician cloth <vpk> <model path> [--against valve|none] [--shapes]
                                                what the compiler made of a model's cloth, read from the compiled
                                                FeModel (PHYS): per chain static/free counts, goal and vertex attraction,
                                                gravity, collision and stray radii, rods, shapes and their layers, beside
                                                Valve's model at the same path
  magician cache [stats|clear]                  the findings cache (MAGICIAN_CACHE, else the platform's cache folder;
                                                `magician paths` shows it); any command takes --no-cache
  magician lint <mod.vpk> [--baseline FILE] [--built-against FILE] [--allow FILE]... [--deep] [--timings] [--findings-out FILE] [--json-out FILE]
                [--sources DIR]... [--json]     addon content trees: referenced files with no source left are M032
                                                check a mod against a game build (exit 1 on errors); --findings-out
                                                writes the run down for `grade --findings`; --json prints the run as
                                                one JSON document (magician.lint/1) instead of the report
  magician pack <src_dir> <out/pakNN_dir.vpk>   deterministic single-file VPK, then lint it
  magician rules [--json]                       list lint rules and what each means in-game
  magician footprint <hero> [--list]            everything a hero is made of, from the game's own data
  magician grade <pak>... [--hero <hero> | --mod <mod.toml>] [--target BUILD] [--light | --findings FILE] [--allow FILE]...
                [--addon-root DIR]... [--scene-root DIR]... [--gate] [--json] [--record-out FILE]
                                                grade a hero mod (every pak it ships, together; the lower pak number wins
                                                a shared path): what kind of mod it is and which of the hero's look,
                                                effects and sound it makes its own, broken (lint errors), fair play
                                                (NSFR, FP?), freshness (stale(N): the builds a proof holds these paks on), rights and
                                                the release gate, as a label (`Echo (look, some effects) · stale(6864)`) and,
                                                with --json, the record (magician.grade/3; --record-out writes it to a file too);
                                                lints deep unless --light, or judges a lint run written by
                                                `lint --findings-out` (what release does); UI scene map VPKs count
                                                when carried beside a pak or under a --scene-root; exit 0 whatever
                                                the grade, or 1 with --gate when the release gate has a blocker
  magician rubric                               the grading criteria, their groups, weights per archetype, the lines that
                                                name a group, and the fair-play classes of the policy in force
  magician policy [--policy FILE]               every judgement value in force (fair-play limits, allowance limits,
                                                gate, weights, group lines, identity, coverage), where each came from, and
                                                the fingerprint a grade records
  magician install <pakNN_dir.vpk> [--as pakNN_dir.vpk] [--mod NAME] [--force]
                                                lint, back up gameinfo.gi, add the addons search path (idempotent),
                                                copy the VPK into game/citadel/addons (ledger-tracked, with the mod it is
                                                for); refuses to replace another mod's pak of the same name (--as installs
                                                under another slot, --force replaces)
  magician install <.../maps/ui/hero_prefabs/<hero>.vpk>
                                                a UI scene map goes to addons/maps/ui/hero_prefabs/ (same ledger and
                                                backup); nothing else under maps/ installs
  magician install --status                     show the search path, backups and what we installed
  magician uninstall <pakNN_dir.vpk> [--force]  remove a VPK we installed (never touches files we didn't, nor one whose
                                                bytes changed since, unless --force; a pak another tool renamed is
                                                followed by its crc)
  magician uninstall --all | --mod NAME [--force]
                                                roll back every install in the ledger, or one mod's
                                                install and uninstall refuse while deadlock.exe runs unless the caller
                                                holds the game lock (--holder NAME or MAGICIAN_GAME_HOLDER); --ignore-game
                                                overrides
  magician scene make <scene.vpk> --out DIR [--mod NAME]   split a built hero-page scene into a patch: only our files, plus the
                                                crcs of the Valve files it keeps (nothing of Valve's is written)
  magician scene check|compose|install <patch> [--out FILE] [--mod NAME] [--force]
                                                build the full scene from a patch and the player's own stock map (refuses
                                                when Valve's files differ from the patch's crcs); install adds the seat flags;
                                                --game DIR, DEADLOCK_GAME or Steam's library list find the game
  magician build <addon> <out/pakNN_dir.vpk> [--csdk DIR] [--bin DIR] [--no-compile] [--force-compile] [--no-wait] [--lock-timeout SECS] [--timings]
                                                one build per addon at a time (lock file in the addon's game output
                                                folder): a second one waits, or fails at once with --no-wait
                                                compiler: --bin, else the addon's .magician-compiler (a bin folder
                                                name), else win64_nm when a clip uses CNmClipDocEvent_TaggedSound
                                                compile only the addon's real sources (PSDs through a temporary image
                                                list), package outputs that map to a live source (no build-only or
                                                stale outputs), lint with the addon's magician.allow; path prefixes
                                                listed in the addon's magician.exclude are left out of the pak
  magician release <addon> --hero <hero> [--with <addon>]... [--out FILE] [--scene-root DIR]... [--mod <mod.toml>] [--no-compile] [--no-wait] [--timings] [--json]
                                                one VPK from every department, lint (allow file), grade, and a
                                                release-report.txt beside it (with the grade record, grade.json); --json
                                                prints the release as one document (magician.release/1)
  magician sign <addon|dir> [--key NAME] [--mod NAME] [--date YYYY-MM-DD] [--dry-run] [--force]
                                                put the maker's mark into the addon's colour sources under the author's
                                                key (--key, else the mod.toml's [mark] key, else its [mod] author, else
                                                the key the addon was signed with before; no key, no mark); idempotent;
                                                writes the magician.signed manifest beside them
  magician verify <vpk> [--key NAME] [--prefix P] [--sample N] [--summary] [--against <unsigned.vpk>] [--mod NAME]...
                                                detect the mark in every compiled texture: z, p, payload, pooled p (the
                                                key as for sign, from the mod.toml beside the pak or of this folder)
  magician drift <mod.vpk>... --from <build> [--to <build>] [--json]
                                                what changed under a mod since the build it was proven on (bones and graph
                                                parameters too, when both builds have a rig.tsv); exit 1 broken, 4 re-prove
  magician patchday [--from <build>] [--capture] [--mod <mod.toml>]... [--json]
                                                the patch-day check: baseline for the installed build (--capture writes it),
                                                install state, every installed pak's drift grouped by mod and compared from
                                                the last build a proof holds its installed bytes on (proofs.tsv; owners it installs
                                                under other names in [mod] owners = [...]), `proven on N of M`, and per
                                                era whether the engine changed too (`data only` / `engine too`, from
                                                engine.tsv; it never changes the verdict); exit 1 broken, 4 re-prove,
                                                0 clear. The mods: each --mod, else every mod under MAGICIAN_MODS (or
                                                `mods =`), else the nearest folder, from the working one upward, with
                                                mod.toml files one or two levels below it
  magician prove <mod.toml> [--evidence DIR] [--build <build>] [--date YYYY-MM-DD] [--pak FILE]... [--json]
                                                after the in-game pass was looked at: record a proof of the mod's bytes in
                                                proofs.tsv beside the mod.toml (the build, the date, every file the install
                                                ledger holds for its owners with crc and sha256, or the files --pak names,
                                                and the pass folder with its index.md's sha256), and the build in
                                                [proven] builds; refused when nothing is installed for it or what is
                                                installed is not what the ledger installed. Proven anywhere means a proof
                                                holds the bytes judged: a rebuilt pak is unproven, and a build [proven]
                                                lists without one counts for nothing (said as \"claimed for other bytes or
                                                never recorded\")
  magician paths [--mod mod.toml]               where the game, reference folder, CSDK, Source 2 Viewer CLI, game lock,
                                                cache and config file resolved to, and why (tab-separated); for the mod
                                                (--mod, or the one of this folder) its hero, paks, addon, allowance files
                                                and scene patches, each with where it was found
  magician update --check [--source LOCATION] [--json]
                                                is there a newer Magician? Reads the release's VERSION from the update
                                                source and compares it with this build (version, then commit): current,
                                                behind (an update is available), ahead (this build is newer than the
                                                latest release), or unknown (the source could not be read, said as
                                                \"could not check\"). Also says whether updating would change judgement:
                                                the grading rubric, the built-in policy and the lint rule set, so grades
                                                and lint are re-run. Changes nothing; exit 0 current or ahead, 5 an update
                                                is available, 3 could not check. This and `update` are the only commands
                                                that touch the network, each step bounded to 10 s
  magician update [--dry-run] [--source LOCATION] [--json]
                                                apply it, only when asked. A release install (the folder a release zip
                                                unpacks to): fetch the zip and SHA256SUMS, check the zip's SHA-256, unpack
                                                beside the toolkit, swap the files in keeping the previous ones, roll
                                                everything back if a step fails. A git checkout: refuse with local
                                                changes, no upstream or no fast-forward, else fast-forward and
                                                `cargo build --release`, putting the program and the tree back if the
                                                build fails. Refused while a game session holds the seat. Exit 0 updated or
                                                already current, 3 refused or could not fetch (nothing changed), 1 failed
                                                after the swap began and was rolled back; --dry-run says what it would do
  magician update --describe [--json]           this build's own VERSION (what a release says about itself); no network
                                                --source LOCATION (or MAGICIAN_UPDATE_SOURCE, or `update_source =`; on
                                                doctor --updates too) is where a release is: a folder, local or on the
                                                network, or an https URL, holding magician-<platform>.zip|.tar.gz, SHA256SUMS
                                                and VERSION; default is the latest release of this build's repository
  --reference DIR (any command; or MAGICIAN_REFERENCE)
                                                use another reference folder for baselines, rig sidecars and indexes
  --policy FILE (lint, mesh, grade, release, build, pack, install, rubric, policy; or MAGICIAN_POLICY, or `policy =`)
                                                judge by a policy file instead of the built-in lines; an unknown key or
                                                an out-of-range value is a usage error naming it
  magician expr '<expression>' | --vmat FILE [--attr name=lo:hi] [--cap PARAM=max]
                                                prove a material expression's output range, cost and safety

Exit codes: 0 clean, nothing to do; 1 broken, a contract or rule failed and someone must act (lint errors, a mesh
FAIL, drift or patchday BROKEN, a scene patch that no longer fits the game, an expression that fails its proof, a
release whose lint failed, `grade --gate` with a blocker); 2 usage error (bad arguments, a missing file, a tool that is
not there); 3 refused on purpose, nothing changed (lint errors at install, a game seat or build lock someone else
holds, a slot another mod owns, an update that could not be fetched or verified); 4 re-prove: it loads, but must be
proven in game on this build (drift, patchday); 5 an update is available (`update --check`, and nothing else).
The reason for 1, 2 and 3 follows `magician:` on stderr; with --json, a command that stops on one prints it as the one
document on stdout too (schema magician.error/1). `--help` (or -h) after any command prints its usage and
exits 0 without doing anything; `--version` prints the toolchain version.

Locations: the game is --game, DEADLOCK_GAME, `game =` in the config file, else found through Steam; the reference
folder (baselines, indexes, work/) is --reference, MAGICIAN_REFERENCE, `reference =`, else magician/reference in
the platform's data folder (%LOCALAPPDATA% on Windows, ~/.local/share on Linux); the CSDK is --csdk, MAGICIAN_CSDK
or `csdk =` (its compiler is a Windows program: build and release compile on Windows); the Source 2 Viewer CLI is --vrf,
MAGICIAN_VRF, `vrf =`, else Source2Viewer-CLI on PATH; the update source is --source, MAGICIAN_UPDATE_SOURCE,
`update_source =`, else the latest release of this build's repository. `magician paths` prints what each resolved to
and where the config file is.";

fn main() -> ExitCode {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    // The front door: `session`, `bundle` and `source` hand everything after the verb to the toolkit's Python tool
    // (frontdoor.rs), `--help` and `--json` included: those are the tool's own.
    if let Some(verb) = raw.first().filter(|v| frontdoor::script_of(v).is_some()) {
        return match frontdoor::run(verb, &raw[1..]) {
            Ok(c) => c,
            Err(e) => stop(&e, verb, false),
        };
    }
    let mut args: Vec<String> = cli::command_first(&raw);
    // `--help` and `--version` answer and stop: nothing else runs, whatever the rest of the line says.
    if cli::wants_help(&args) {
        let cmd = args.iter().find(|a| !a.starts_with('-') && *a != "help").map(String::as_str);
        out!("{}", cli::help_for(USAGE, cmd));
        return ExitCode::SUCCESS;
    }
    if args.iter().any(|a| a == "--version") {
        outln!("{}", cli::version());
        return ExitCode::SUCCESS;
    }
    let wants_json = args.iter().any(|a| a == "--json");
    let command = args.iter().find(|a| !a.starts_with('-')).cloned().unwrap_or_default();
    if let Err(e) = cli::check_options(&args, USAGE).and_then(|()| cli::check_values(&args, SWITCHES, USAGE)) {
        return stop(&Stop::Usage(e), &command, wants_json);
    }
    if args.iter().any(|a| a == "--json") {
        TEXT.store(false, Ordering::Relaxed);
    }
    // `--no-cache` is a switch on every command; taken out here so it can't swallow a positional argument.
    let no_cache = args.iter().any(|a| a == "--no-cache");
    args.retain(|a| a != "--no-cache" && a != "--json");
    configure_cache(no_cache);
    // `--reference DIR` points every command at another reference folder (baselines, rig sidecars, indexes).
    if let Some(i) = args.iter().position(|a| a == "--reference") {
        if let Some(d) = args.get(i + 1).cloned() {
            std::env::set_var("MAGICIAN_REFERENCE", d);
            args.drain(i..=i + 1);
        }
    }
    if args.is_empty() {
        // No command: a screen that says where things stand and what to run next; the reference is `magician help`.
        // Still exit 2, so a script whose command came out empty does not read as one that ran.
        errln!("{}", orientation());
        return ExitCode::from(cli::USAGE);
    }
    // A verb with nothing after it, run inside a mod's folder, is completed from the mod (modctx.rs).
    let runs = match modctx::fill(&args) {
        Ok(r) => r,
        Err(e) => return stop(&e, &command, wants_json),
    };
    let mut code = ExitCode::SUCCESS;
    for run in &runs {
        match dispatch(run) {
            Ok(c) => code = worst(code, c),
            Err(e) => return stop(&e, &command, wants_json),
        }
    }
    code
}

fn dispatch(args: &[String]) -> R {
    match args.first().map(String::as_str) {
        Some("baseline") => cmd_baseline(&args[1..]),
        Some("ls") => cmd_ls(&args[1..]),
        Some("refs") => cmd_refs(&args[1..]),
        Some("dump") => cmd_dump(&args[1..]),
        Some("kv") => cmd_kv(&args[1..]),
        Some("index") => cmd_index(&args[1..]),
        Some("textures") => cmd_textures(&args[1..]),
        Some("mesh") => cmd_mesh(&args[1..]),
        Some("cloth") => compare_game(&args[1..]).and_then(|g| clothcmd::cmd_cloth(&args[1..], &g)).map_err(Stop::from),
        Some("lint") => cmd_lint(&args[1..]),
        Some("cache") => cmd_cache(&args[1..]),
        Some("pack") => cmd_pack(&args[1..]),
        Some("rules") => cmd_rules(&args[1..]),
        Some("scan") => cmd_scan(&args[1..]),
        Some("footprint") => cmd_footprint(&args[1..]),
        Some("grade") => cmd_grade(&args[1..]),
        Some("rubric") => cmd_rubric(&args[1..]),
        Some("policy") => cmd_policy(&args[1..]),
        Some("expr") => cmd_expr(&args[1..]),
        Some("build") => cmd_build(&args[1..]),
        Some("release") => cmd_release(&args[1..]),
        Some("drift") => cmd_drift(&args[1..]),
        Some("patchday") => patchcmd::cmd_patchday(&args[1..]),
        Some("prove") => patchcmd::cmd_prove(&args[1..]),
        Some("install") => cmd_install(&args[1..]),
        Some("uninstall") => cmd_uninstall(&args[1..]),
        Some("scene") => scene::cmd_scene(&args[1..]),
        Some("sign") => markcmd::cmd_sign(&args[1..]).map_err(Stop::from),
        Some("paths") => cmd_paths(&args[1..]),
        Some("init") => initcmd::cmd_init(&args[1..]),
        Some("new") => newcmd::cmd_new(&args[1..]),
        Some("doctor") => doctor::cmd_doctor(&args[1..]),
        Some("check") => checkcmd::cmd_check(&args[1..]),
        Some("update") => update::cmd_update(&args[1..]),
        Some("verify") => markcmd::cmd_verify(&args[1..]).map_err(Stop::from),
        Some(other) => Err(Stop::Usage(format!("unknown command {other:?}{}; `magician help` lists them", cli::did_you_mean(other, COMMANDS, |c| format!("`magician {c}`"))))),
        None => Err(Stop::Usage("no command".into())),
    }
}

/// What `magician` alone prints: this machine's game and build, the mod of this folder, and the few commands to run
/// next. Quick (it opens no pak): the reference is `magician help`.
fn orientation() -> String {
    let mut o = format!("{}\n\n", USAGE.lines().next().unwrap_or(""));
    match paths::game(None) {
        Ok(g) => {
            let build = game_build(&g.path).unwrap_or_else(|_| "?".into());
            let base = if default_baseline_path(&build).is_file() { "its baseline is on record" } else { "no baseline for it yet" };
            o += &format!("  game  Deadlock build {build}, {} ({}); {base}\n", g.path.display(), g.source);
        }
        Err(_) => o += "  game  not found: `magician doctor` says how to point Magician at it\n",
    }
    let ctx = modctx::ModCtx::discover();
    let game = paths::game(None).ok().map(|g| g.path);
    let no_pak = ctx.as_ref().is_some_and(|m| m.paks(game.as_deref()).is_empty());
    match &ctx {
        Some(m) if no_pak => o += &format!("  mod   {}, {} ({}); no pak yet\n", m.label(), m.toml.display(), m.found),
        Some(m) => o += &format!("  mod   {}, {} ({})\n", m.label(), m.toml.display(), m.found),
        None => o += "  mod   none here (no mod.toml in this folder or above it)\n",
    }
    o += "\nNext:\n";
    o += "  magician doctor              is this machine ready? one line per part, each with its fix\n";
    if no_pak {
        o += "  magician release --out pak10_dir.vpk\n                               build its first pak from its addon (needs the CSDK; README.md says how to link it)\n";
        o += "  magician check               then this mod on the installed build: lint, grade, drift, scenes; one verdict\n";
        o += "  magician install             then put its pak in the game (lints first; never under a running game)\n";
    } else if ctx.is_some() {
        o += "  magician check               this mod on the installed build: lint, grade, drift, scenes; one verdict\n";
        o += "  magician install             put its pak in the game (lints first; never under a running game)\n";
    } else {
        o += "  magician new <hero>          start a skin from nothing: a mod folder for that hero\n";
        o += "  magician init <pak>          describe a mod in a mod.toml, then run magician from its folder\n";
        o += "  magician check <mod folder>  a mod on the installed build: lint, grade, drift, scenes; one verdict\n";
    }
    o += "  magician patchday --capture  after a game update: what it changed under every installed mod\n";
    o += "\n`magician help` lists every command and option; `magician help <command>` shows one.";
    o
}

/// The exit code of several runs of one command (a mod with several paks): broken over re-prove over clean.
fn worst(a: ExitCode, b: ExitCode) -> ExitCode {
    for c in [cli::BROKEN, cli::REPROVE] {
        if a == ExitCode::from(c) || b == ExitCode::from(c) {
            return ExitCode::from(c);
        }
    }
    if a == ExitCode::SUCCESS { b } else { a }
}


/// End the run on a `Stop`: its reason on stderr after `magician:`, and with `--json` the same as the one document on
/// stdout, so a script reading JSON always gets a document (`magician.error/1`), whatever went wrong.
fn stop(e: &Stop, command: &str, json: bool) -> ExitCode {
    errln!("magician: {e}");
    if json {
        let kind = match e {
            Stop::Usage(_) => "usage",
            Stop::Broken(_) => "broken",
            Stop::Refused(_) => "refused",
        };
        outln!("{}", jsonout::Obj::new().str("schema", "magician.error/1").str("command", command).int("exit", e.code() as i64).str("kind", kind).str("error", e.text()).build());
    }
    ExitCode::from(e.code())
}

use cli::Stop;

type R = Result<ExitCode, Stop>;

/// Where derived results are kept between runs (`paths::cache`). `--no-cache` or `MAGICIAN_NO_CACHE=1` turns it off for
/// the run (results are identical either way).
fn cache_dir() -> Option<PathBuf> {
    paths::cache().map(|f| f.path)
}

fn configure_cache(off: bool) {
    let off = off || std::env::var_os("MAGICIAN_NO_CACHE").is_some_and(|v| !v.is_empty() && v != "0");
    lint::cache::set_dir(if off { None } else { cache_dir() });
}

/// `magician cache [stats|clear]`: the findings cache (see docs/TDD.md, Determinism and speed).
fn cmd_cache(args: &[String]) -> R {
    match args.first().map(String::as_str).unwrap_or("stats") {
        "clear" => {
            let n = lint::cache::clear();
            say!("cache cleared: {n} entr{}", if n == 1 { "y" } else { "ies" });
        }
        "stats" => {
            let (n, b) = lint::cache::usage();
            match lint::cache::dir() {
                Some(d) => say!("cache {}: {n} entr{}, {:.1} KB (code {})", d.display(), if n == 1 { "y" } else { "ies" }, b as f64 / 1024.0, lint::cache::CODE),
                None => say!("cache off"),
            }
        }
        other => return Err(format!("cache [stats|clear], not '{other}'").into()),
    }
    Ok(ExitCode::SUCCESS)
}

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1).cloned())
}
/// Every command `main` dispatches, for "did you mean" on a typo.
const COMMANDS: &[&str] = &[
    "baseline", "ls", "refs", "dump", "kv", "index", "textures", "mesh", "cloth", "lint", "cache", "pack", "rules", "scan", "footprint", "grade", "rubric",
    "policy", "expr", "build", "release", "drift", "patchday", "prove", "install", "uninstall", "scene", "sign", "paths", "verify", "init", "new", "doctor", "check", "update", "help", "session", "bundle", "source",
];

/// Options that take no value. Every other `--option` takes the argument after it, so `positional` skips that one.
const SWITCHES: &[&str] = &[
    "--deep", "--light", "--gate", "--timings", "--capture", "--all", "--force", "--status", "--no-compile", "--force-compile", "--no-wait",
    "--ignore-game", "--dry-run", "--summary", "--list", "--shapes", "--no-png", "--verbose", "--check", "--describe", "--updates",
];

/// The arguments that are not options or option values. A switch (`--deep`) takes no value, so `lint --deep x.vpk`
/// keeps `x.vpk`.
fn positional(args: &[String]) -> Vec<&String> {
    let mut out = Vec::new();
    let mut skip = false;
    for a in args {
        if skip {
            skip = false;
            continue;
        }
        if a.starts_with("--") {
            skip = !SWITCHES.contains(&a.as_str());
            continue;
        }
        out.push(a);
    }
    out
}

fn game_build(game: &Path) -> Result<String, String> {
    let inf = game.join("citadel").join("steam.inf");
    let text = std::fs::read_to_string(&inf).map_err(|e| format!("{}: {e}", inf.display()))?;
    text.lines()
        .find_map(|l| l.strip_prefix("ClientVersion="))
        .map(|s| s.trim().to_string())
        .ok_or_else(|| format!("no ClientVersion in {}", inf.display()))
}

fn open(p: &Path) -> Result<vpk::Vpk, String> {
    vpk::Vpk::open(p).map_err(|e| format!("{}: {e}", p.display()))
}

fn installed_baseline(game: &Path) -> Result<Baseline, String> {
    let build = game_build(game)?;
    let citadel = open(&game.join("citadel").join("pak01_dir.vpk"))?;
    let core = open(&game.join("core").join("pak01_dir.vpk"))?;
    Ok(Baseline::from_vpks(&build, &[&citadel, &core]))
}

/// The reference folder (baselines, rig sidecars, users indexes): `--reference DIR` or `MAGICIAN_REFERENCE`, else the
/// default. A drill points it at a scratch copy so nothing real is touched.
fn reference_root() -> PathBuf {
    paths::reference().path
}

/// Deadlock's `game` folder for this command (`--game`, `DEADLOCK_GAME`, the config file, Steam; see paths.rs).
fn game_dir(args: &[String]) -> Result<PathBuf, String> {
    paths::game(flag(args, "--game")).map(|f| f.path)
}

/// The game for `mesh` and `cloth`: required when they compare with Valve's model, optional with `--against none`
/// (an empty path then reads as a game with no files).
fn compare_game(args: &[String]) -> Result<PathBuf, String> {
    match game_dir(args) {
        Err(_) if flag(args, "--against").as_deref() == Some("none") => Ok(PathBuf::new()),
        r => r,
    }
}

/// `magician paths`: where every outside location resolved to, and why (flag, environment, config file, discovery).
fn cmd_paths(args: &[String]) -> R {
    let row = |name: &str, f: Result<paths::Found, String>| match f {
        Ok(f) => say!("{name}	{}	{}", f.path.display(), f.source),
        Err(e) => say!("{name}	-	not found: {e}"),
    };
    row("game", paths::game(flag(args, "--game")));
    row("reference", Ok(paths::reference()));
    row("csdk", paths::csdk(flag(args, "--csdk")));
    row("vrf", Ok(paths::vrf(flag(args, "--vrf"))));
    row("game_lock", Ok(paths::game_lock()));
    match paths::mods() {
        Some(f) => say!("mods	{}	{}", f.path.display(), f.source),
        None => say!("mods	-	not set: patchday looks below the working folder and the folders above it"),
    }
    row("update_source", Ok(paths::update_source(flag(args, "--source"))));
    row("toolkit", paths::toolkit());
    row("python", paths::python().map(|(f, _)| f));
    match paths::policy(flag(args, "--policy")) {
        Some(f) => say!("policy	{}	{}", f.path.display(), f.source),
        None => say!("policy	-	built-in"),
    }
    let cache = cache_dir().map(|d| d.display().to_string()).unwrap_or_else(|| "-".into());
    say!("cache	{cache}	{}", if std::env::var_os("MAGICIAN_CACHE").is_some() { "MAGICIAN_CACHE" } else { "default" });
    match paths::config_file() {
        Some(c) => say!("config	{}	{}", c.display(), if c.is_file() { "present" } else { "absent" }),
        None => say!("config	-	no location (set MAGICIAN_CONFIG)"),
    }
    // The mod: --mod, else the one of this folder. Every answer a bare verb would take from it, with its source.
    let ctx = match flag(args, "--mod") {
        Some(m) => Some(modctx::ModCtx::load(Path::new(&m), "--mod")?),
        None => modctx::ModCtx::discover(),
    };
    if let Some(m) = ctx {
        say!("mod\t{}\t{}", m.toml.display(), m.found);
        match m.hero() {
            Some(h) => say!("hero\t{h}\t[mod] hero"),
            None => say!("hero\t-\tnone named ([mod] hero)"),
        }
        let game = paths::game(flag(args, "--game")).ok().map(|f| f.path);
        let paks = m.paks(game.as_deref());
        for p in &paks {
            say!("pak\t{}\t{}", p.path.display(), p.source);
        }
        if paks.is_empty() {
            say!("pak\t-\tnone ([mod] paks, beside the mod.toml, or installed for {})", m.owners().join(", "));
        }
        let csdk = paths::csdk(flag(args, "--csdk")).ok().map(|f| f.path);
        if let Some(a) = m.addon(csdk.as_deref()) {
            say!("addon\t{}\t{}", a.path.display(), a.source);
        }
        let (found, looked) = paths::mod_allowances(&m.toml, &m.text, csdk.as_deref());
        for f in &found {
            say!("allow\t{}\t{}", f.path.display(), f.source);
        }
        if found.is_empty() {
            say!("allow\t-\tnone found (looked in {})", looked.iter().map(|l| l.display().to_string()).collect::<Vec<_>>().join("; "));
        }
        for sc in m.scenes() {
            say!("scene\t{}\t{}", sc.path.display(), sc.source);
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn default_baseline_path(build: &str) -> PathBuf {
    reference_root().join(build).join("baseline.tsv")
}

fn cmd_baseline(args: &[String]) -> R {
    let game = game_dir(args)?;
    let b = installed_baseline(&game)?;
    let out = flag(args, "--out").map(PathBuf::from).unwrap_or_else(|| default_baseline_path(&b.build));
    write_era(&game, &b, &out)?;
    Ok(ExitCode::SUCCESS)
}

/// Write the baseline and, beside it, the rig sidecar (`rig.tsv`: every skeleton's bones, every AnimGraph's control
/// parameters) that lets a later `drift` say what changed in a rig file, not only that its CRC did.
fn write_era(game: &Path, b: &Baseline, out: &Path) -> Result<(), String> {
    for line in capture_era(game, b, out)? {
        say!("{line}");
    }
    Ok(())
}

/// `write_era` without the report: the lines it would print. Each file is written whole (a temporary name, then a
/// rename), so a run that stops halfway never leaves a baseline that reads as a short build.
fn capture_era(game: &Path, b: &Baseline, out: &Path) -> Result<Vec<String>, String> {
    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let mut text = Vec::new();
    b.write(&mut text).map_err(|e| e.to_string())?;
    install::write_atomic(out, &text).map_err(|e| format!("{}: {e}", out.display()))?;
    let mut lines = vec![format!("build {} : {} files -> {}", b.build, b.files.len(), out.display())];
    lines.push(capture_rig(game, &b.build, &out.with_file_name("rig.tsv"))?);
    lines.push(capture_engine(game, &b.build, &out.with_file_name("engine.tsv"))?);
    Ok(lines)
}

/// The engine sidecar (`engine.tsv`): size and sha256 of every executable, library and shader archive the build ships
/// (patchday.rs, `Engine`), and of `gameinfo.gi` as shipped: the citadel one from its stock backup of this build, or
/// as it is when no install has edited it; otherwise recorded as not known, never guessed.
fn capture_engine(game: &Path, build: &str, out: &Path) -> Result<String, String> {
    let mut e = patchday::Engine { build: build.to_string(), ..Default::default() };
    for (rel, abs) in patchday::engine_files(game) {
        let bytes = std::fs::read(&abs).map_err(|err| format!("{}: {err}", abs.display()))?;
        e.files.insert(rel, Some((bytes.len() as u64, grade::sha256::hex(&bytes))));
    }
    let ip = install::Paths { citadel: game.join("citadel") };
    let stock = |p: &Path| std::fs::read(p).ok().map(|b| (b.len() as u64, grade::sha256::hex(&b)));
    let citadel_gi = if ip.backup(build).is_file() {
        stock(&ip.backup(build))
    } else {
        std::fs::read_to_string(ip.gameinfo()).ok().filter(|t| install::add_addons_search_path(t).ok().flatten().is_some()).and_then(|_| stock(&ip.gameinfo()))
    };
    let mut unknown = "";
    if ip.gameinfo().is_file() {
        if citadel_gi.is_none() {
            unknown = "; citadel/gameinfo.gi not known as shipped (an install edited it and no stock backup of this build exists)";
        }
        e.files.insert("citadel/gameinfo.gi".into(), citadel_gi);
    }
    if let Some(core) = stock(&game.join("core").join("gameinfo.gi")) {
        e.files.insert("core/gameinfo.gi".into(), Some(core));
    }
    let mut text = Vec::new();
    e.write(&mut text).map_err(|err| err.to_string())?;
    install::write_atomic(out, &text).map_err(|err| format!("{}: {err}", out.display()))?;
    Ok(format!("engine {build} : {} files (executables, libraries, shader archives, gameinfo){unknown} -> {}", e.files.len(), out.display()))
}

fn write_rig(game: &Path, build: &str, out: &Path) -> Result<(), String> {
    let line = capture_rig(game, build, out)?;
    say!("{line}");
    Ok(())
}

fn capture_rig(game: &Path, build: &str, out: &Path) -> Result<String, String> {
    let citadel = open(&game.join("citadel").join("pak01_dir.vpk"))?;
    let rig = patchday::Rig::capture(build, &citadel);
    let mut text = Vec::new();
    rig.write(&mut text).map_err(|e| e.to_string())?;
    install::write_atomic(out, &text).map_err(|e| format!("{}: {e}", out.display()))?;
    Ok(format!("rig {build} : {} skeletons, {} anim graphs -> {}", rig.skeletons.len(), rig.graphs.len(), out.display()))
}

/// The first time a command needs the installed build's baseline and the reference folder has none, capture it (and
/// the rig sidecar) there, as `magician baseline` would: it only reads the game, and the next drift or patch day needs
/// it. Only for this machine's own game (found through Steam or named in the config file): a game named by a flag
/// or the environment is often a drill's copy, whose build number must not seed the real reference folder. Says on
/// stderr what it wrote, or why it could not (the command then goes on with the baseline read live).
fn auto_capture(game: &paths::Found, live: &Baseline) {
    if !(game.source == "steam" || game.source.starts_with("config")) {
        return;
    }
    let out = default_baseline_path(&live.build);
    if out.exists() {
        return;
    }
    let hint = progress::start(&format!("first run on build {}: capturing its baseline and rig sidecar", live.build));
    let r = capture_era(&game.path, live, &out);
    drop(hint);
    match r {
        Ok(_) => errln!("magician: note: first run on build {}: captured its baseline and rig sidecar into {} (what `magician baseline` writes)", live.build, out.parent().map(|d| d.display().to_string()).unwrap_or_default()),
        Err(e) => errln!("magician: note: build {} has no baseline in the reference folder and capturing one failed ({e}); this run reads the game instead", live.build),
    }
}

fn cmd_ls(args: &[String]) -> R {
    let p = positional(args);
    let v = open(Path::new(p.first().ok_or("ls <vpk> [prefix]")?))?;
    let prefix = p.get(1).map(|s| s.as_str()).unwrap_or("");
    let mut n = 0;
    for e in v.entries.iter().filter(|e| e.path.starts_with(prefix)) {
        say!("{}\t{:08x}\t{}", e.path, e.crc, e.size());
        n += 1;
    }
    errln!("{n} files");
    Ok(ExitCode::SUCCESS)
}

fn cmd_refs(args: &[String]) -> R {
    let p = positional(args);
    let vp = Path::new(p.first().ok_or("refs <vpk> <path>")?);
    let path = p.get(1).ok_or("refs <vpk> <path>")?;
    let v = open(vp)?;
    let e = v.find(path).ok_or_else(|| format!("{path} not in {}", vp.display()))?;
    let data = v.read(e).map_err(|e| e.to_string())?;
    let r = resource::parse(&data).map_err(|e| e.to_string())?;
    let blocks: Vec<_> = r.blocks.iter().map(|b| b.tag_str()).collect();
    say!("{path}\n  blocks: {}\n  references ({}):", blocks.join(" "), r.ext_refs.len());
    for x in &r.ext_refs {
        let t = x.compiled_path();
        let mark = if v.find(&t).is_some() { "ok " } else { "-- " };
        say!("  {mark}{t}");
    }
    Ok(ExitCode::SUCCESS)
}

/// Print a compiled resource's KV3 block as text (decoded natively), or a texture's header.
fn cmd_dump(args: &[String]) -> R {
    if let Some(t) = flag(args, "--text") {
        // Re-print a KV3 text file (e.g. Source 2 Viewer's decompile) in magician's form, to diff against a decode.
        let src = std::fs::read_to_string(&t).map_err(|e| format!("{t}: {e}"))?;
        out!("{}", resource::kv3::parse_text(&src)?.to_text());
        return Ok(ExitCode::SUCCESS);
    }
    let p = positional(args);
    let vp = Path::new(p.first().ok_or("dump <vpk> <path> [--block TAG]")?);
    let path = p.get(1).ok_or("dump <vpk> <path> [--block TAG]")?;
    let v = open(vp)?;
    let e = v.find(path).ok_or_else(|| format!("{path} not in {}", vp.display()))?;
    let data = v.read(e).map_err(|e| e.to_string())?;
    if let Some(t) = resource::texture::info(&data) {
        say!("{path}: {}x{}x{} {} (format {}, {} mips, flags {:#x}, {} bytes)", t.width, t.height, t.depth, t.format_name(), t.format, t.mips, t.flags, data.len());
        return Ok(ExitCode::SUCCESS);
    }
    let tag = flag(args, "--block").unwrap_or("DATA".into());
    let tag: [u8; 4] = tag.as_bytes().try_into().map_err(|_| "--block takes a 4-letter tag")?;
    match resource::kv3_block(&data, &tag) {
        Some(Ok(kv)) => out!("{}", kv.to_text()),
        Some(Err(e)) => return Err(e.into()),
        None => return Err(format!("{path} has no KV3 {} block", String::from_utf8_lossy(&tag)).into()),
    }
    Ok(ExitCode::SUCCESS)
}

/// Entries under a prefix with a suffix, in physical order (archive, offset): the disk here is an HDD.
fn physical<'a>(v: &'a vpk::Vpk, prefix: &str, suffix: &str) -> Vec<&'a vpk::Entry> {
    let mut todo: Vec<&vpk::Entry> = v.entries.iter().filter(|e| e.path.starts_with(prefix) && e.path.ends_with(suffix)).collect();
    todo.sort_by_key(|e| (e.archive_index, e.offset));
    todo
}

fn scalar_text(v: &resource::kv3::Value) -> Option<String> {
    use resource::kv3::Value as V;
    match v {
        V::Str(s) => Some(format!("\"{s}\"")),
        V::Array(a) if a.iter().all(|x| x.as_f64().is_some()) && !a.is_empty() && a.len() <= 4 => {
            Some(format!("{:?}", v.numbers()))
        }
        V::Object(_) | V::Array(_) | V::Blob(_) => None,
        x => x.as_f64().map(|f| f.to_string()).or(Some("null".into())),
    }
}

fn cmd_kv(args: &[String]) -> R {
    let p = positional(args);
    let usage = "kv <vpk> <prefix> <key-substring> [--block TAG]";
    let v = open(Path::new(p.first().ok_or(usage)?))?;
    let prefix = p.get(1).ok_or(usage)?;
    let key = p.get(2).ok_or(usage)?.to_ascii_lowercase();
    let tag = flag(args, "--block").unwrap_or("DATA".into());
    let tag: [u8; 4] = tag.as_bytes().try_into().map_err(|_| "--block takes a 4-letter tag")?;
    let (mut files, mut hits, mut failed) = (0, 0, 0);
    for e in physical(&v, prefix, "_c") {
        let data = v.read(e).map_err(|e| e.to_string())?;
        match resource::kv3_block(&data, &tag) {
            Some(Ok(kv)) => {
                files += 1;
                kv.walk(&mut |k, x| {
                    if k.to_ascii_lowercase().contains(&key) {
                        if let Some(t) = scalar_text(x) {
                            hits += 1;
                            say!("{}	{k}	{t}", e.path);
                        }
                    }
                });
            }
            Some(Err(_)) => failed += 1,
            None => {}
        }
    }
    errln!("{files} files decoded, {failed} failed, {hits} values");
    Ok(ExitCode::SUCCESS)
}

fn cmd_textures(args: &[String]) -> R {
    let p = positional(args);
    let v = open(Path::new(p.first().ok_or("textures <vpk> [prefix]")?))?;
    let prefix = p.get(1).map(|s| s.as_str()).unwrap_or("");
    let list = args.iter().any(|a| a == "--list");
    let mut by: std::collections::BTreeMap<(String, &'static str), (usize, usize)> = Default::default();
    for e in physical(&v, prefix, ".vtex_c") {
        // the header sits in the DATA block near the start; 4 KB covers the header and block table
        let data = v.read_range(e, 0, 1 << 16).map_err(|e| e.to_string())?;
        let Some(t) = resource::texture::info(&data).or_else(|| resource::texture::info(&v.read(e).ok()?)) else { continue };
        let top: String = e.path.split('/').take(if e.path.starts_with("materials/") { 2 } else { 1 }).collect::<Vec<_>>().join("/");
        let slot = by.entry((top, t.format_name())).or_default();
        slot.0 += 1;
        if !t.pow2() {
            slot.1 += 1;
            if list {
                say!("{}	{}x{}	{}", e.path, t.width, t.height, t.format_name());
            }
        }
    }
    say!("folder	format	textures	non-pow2");
    for ((top, f), (n, np)) in by {
        say!("{top}	{f}	{n}	{np}");
    }
    Ok(ExitCode::SUCCESS)
}

fn sev_label(s: Severity) -> &'static str {
    match s {
        Severity::Error => "error",
        Severity::Warn => "warn ",
        Severity::Info => "info ",
    }
}

/// Print findings after applying per-site allowances. Allowed findings are listed with their reason;
/// stale allowances (matching nothing) are reported so the file can't rot.
/// `--json-out FILE` writes the run as JSON beside the report; `--json` prints it on stdout instead of the report.
/// One lint run after its allowances.
pub(crate) struct LintOut {
    pub r: lint::allow::Applied,
    pub deep: bool,
    pub build: String,
    pub vpk: String,
}

impl LintOut {
    pub fn errors(&self) -> usize {
        self.r.kept.iter().filter(|x| x.severity == Severity::Error).count()
    }
    pub fn warnings(&self) -> usize {
        self.r.kept.iter().filter(|x| x.severity == Severity::Warn).count()
    }
}

fn apply_allowances(f: Vec<lint::Finding>, allowances: &[lint::allow::Allowance], deep: bool, build: &str, vpk: &str, json_out: Option<&str>) -> Result<LintOut, Stop> {
    let r = lint::allow::apply(f, allowances, deep);
    // A file the caller asked for and did not get is a failed run (exit 2), not a note: a release reading it would
    // otherwise report the previous run's file, or none.
    if let Some(out) = json_out {
        write_output("--json-out", out, &jsonout::lint_run(&r, build, vpk, deep))?;
    }
    Ok(LintOut { r, deep, build: build.to_string(), vpk: vpk.to_string() })
}

fn print_lint(o: &LintOut) -> R {
    let (r, deep, build, vpk) = (&o.r, o.deep, o.build.as_str(), o.vpk.as_str());
    if json_mode() {
        out!("{}", jsonout::lint_run(r, build, vpk, deep));
    }
    for x in &r.kept {
        let at = x.path.as_deref().unwrap_or("(vpk)");
        say!("{} {} {:<26} {at}
        {}", sev_label(x.severity), x.code, x.name, x.message);
    }
    if !r.allowed.is_empty() {
        say!("allowed ({}):", r.allowed.len());
        for (x, why) in &r.allowed {
            say!("  {} {} {}  -- {}", x.code, x.path.as_deref().unwrap_or("(vpk)"), x.message, why);
        }
    }
    for a in &r.stale {
        say!("stale allowance (line {}): {} {} '{}' matched nothing; remove it", a.line, a.code, a.path, a.needle);
    }
    let errors = r.kept.iter().filter(|x| x.severity == Severity::Error).count();
    let warns = r.kept.iter().filter(|x| x.severity == Severity::Warn).count();
    say!("{errors} error(s), {warns} warning(s), {} note(s), {} allowed, {} stale", r.kept.len() - errors - warns, r.allowed.len(), r.stale.len());
    Ok(if errors > 0 { ExitCode::from(1) } else { ExitCode::SUCCESS })
}

/// Write a file an output flag (`--json-out`, `--findings-out`, `--record-out`) names; a write that fails stops the
/// command with the flag, the path and the reason.
pub(crate) fn write_output(flag: &str, path: &str, text: &str) -> Result<(), Stop> {
    std::fs::write(path, text).map_err(|e| Stop::Usage(format!("{flag} {path}: could not write it ({e}); nothing else was written there")))
}

fn load_allowances(args: &[String], default: Option<PathBuf>) -> Result<Vec<lint::allow::Allowance>, String> {
    let mut files: Vec<PathBuf> = flags(args, "--allow").into_iter().map(PathBuf::from).collect();
    if files.is_empty() {
        files.extend(default.filter(|p| p.exists()));
    }
    let mut all = Vec::new();
    for p in files {
        let text = std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?;
        let a = lint::allow::parse(&text).map_err(|e| format!("{}: {e}", p.display()))?;
        say!("allowances: {} from {}", a.len(), p.display());
        all.extend(a);
    }
    Ok(all)
}

fn load_base(args: &[String]) -> Result<Baseline, String> {
    match flag(args, "--baseline") {
        Some(p) => Baseline::load(p),
        None => {
            let found = paths::game(flag(args, "--game"))?;
            let build = game_build(&found.path)?;
            let cached = default_baseline_path(&build);
            if cached.exists() {
                return Baseline::load(cached);
            }
            let live = installed_baseline(&found.path)?;
            auto_capture(&found, &live);
            Ok(live)
        }
    }
}

/// The addon content trees named by `--sources DIR` (repeatable) and `--addon-root DIR`, as one set of
/// relative, lowercase paths: what M032 checks compiled files against. `None` when none is given.
fn content_trees(args: &[String]) -> Result<Option<std::collections::HashSet<String>>, String> {
    let mut dirs: Vec<String> = flags(args, "--sources");
    dirs.extend(flag(args, "--addon-root"));
    if dirs.is_empty() {
        return Ok(None);
    }
    let mut all = std::collections::HashSet::new();
    for d in dirs {
        let root = PathBuf::from(&d);
        let mut rels = Vec::new();
        build::rel_files(&root, &root, &mut rels).map_err(|e| format!("{d}: {e}"))?;
        all.extend(rels.into_iter().map(|r| r.to_ascii_lowercase()));
    }
    Ok(Some(all))
}

fn lint_vpk(path: &Path, args: &[String]) -> R {
    print_lint(&lint_run(path, args)?)
}

/// `lint`'s work, without printing the findings (`--findings-out` and `--json-out` are still written).
pub(crate) fn lint_run(path: &Path, args: &[String]) -> Result<LintOut, Stop> {
    // The limits the run judges by: read first, so a policy that does not read stops the run before anything else.
    let policy = load_policy(args)?;
    let mut setup: Vec<(&str, f64)> = Vec::new();
    let mut lap = std::time::Instant::now();
    let mut mark = |what: &'static str, setup: &mut Vec<(&str, f64)>| {
        setup.push((what, lap.elapsed().as_secs_f64()));
        lap = std::time::Instant::now();
    };
    // The game side (baseline, archives, particle classes) and the mod's files don't depend on each other.
    let (game_side, mod_side) = lint::par::join(
        || -> Result<(Baseline, gamefiles::GameFiles), String> {
            let base = load_base(args)?;
            let mut gf = game_files(args, &base.build)?;
            gf.addon_sources = content_trees(args)?;
            Ok((base, gf))
        },
        || -> Result<Vec<ModFile>, String> {
            let v = open(path)?;
            ModFile::from_vpk(&v).map_err(|e| e.to_string())
        },
    );
    let (base, gf) = game_side?;
    let files = mod_side?;
    mark("game side + mod files", &mut setup);
    let then = flag(args, "--built-against").map(Baseline::load).transpose()?;
    let name = path.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    say!("lint {} against build {} ({} files)", path.display(), base.build, files.len());
    let originals = original_refs(&files, args)?;
    mark("originals' references", &mut setup);
    let deep = args.iter().any(|a| a == "--deep");
    let t0 = std::time::Instant::now();
    let mut f = lint::lint(&Input { vpk_name: name, files, base: Some(&base), built_against: then.as_ref(), original_refs: &originals, game: Some(&gf), deep, fair_play: &policy.fair_play });
    // Fair play is lenient unless the policy enforces it: its findings are warnings, so lint, build and install don't
    // stop on them alone.
    grade::compliance::set_severities(&mut f, &policy);
    if args.iter().any(|a| a == "--timings") {
        say!("timings: total {:.2} s", t0.elapsed().as_secs_f64());
        for (what, s) in &setup {
            say!("  setup: {what:<22} {s:>7.2} s");
        }
        let (hit, miss) = lint::cache::stats();
        if hit + miss > 0 {
            say!("  model cache: {hit} hit, {miss} miss");
        }
        for (fam, s) in lint::timings() {
            say!("  {fam:<24} {s:>7.2} s");
        }
    }
    if !deep {
        say!("mesh family: cheap checks only (budgets, LODs, materials); `--deep` (run by `magician release`) or `magician mesh` for the geometry");
    }
    if let Some(out) = flag(args, "--findings-out") {
        let vpk_name = path.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let text = lint::report::write(&f, lint::report::Depth::from_flag(deep), &base.build, &vpk_name);
        write_output("--findings-out", &out, &text)?;
    }
    let default_allow = flag(args, "--addon-root").map(|r| PathBuf::from(r).join("magician.allow"));
    let allowances = load_allowances(args, default_allow)?;
    let json_out = flag(args, "--json-out");
    apply_allowances(f, &allowances, deep, &base.build, &name_for_json(path), json_out.as_deref())
}

fn name_for_json(path: &Path) -> String {
    path.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Read the game's originals of every compiled file the mod replaces, and collect their references.
fn original_refs(files: &[ModFile], args: &[String]) -> Result<std::collections::HashMap<String, Vec<String>>, String> {
    let game = game_dir(args)?;
    let mut out = std::collections::HashMap::new();
    let paks: Vec<std::sync::Arc<vpk::Vpk>> = ["citadel", "core"]
        .iter()
        .filter_map(|d| vpk::Vpk::open_shared(game.join(d).join("pak01_dir.vpk")).ok())
        .collect();
    for f in files.iter().filter(|f| f.path.ends_with("_c")) {
        for pak in &paks {
            if let Some(e) = pak.find(&f.path) {
                let data = pak.read(e).map_err(|e| e.to_string())?;
                if let Ok(r) = resource::parse(&data) {
                    out.insert(f.path.clone(), lint::ref_targets(&r));
                }
                break;
            }
        }
    }
    Ok(out)
}

fn users_index_path(build: &str) -> PathBuf {
    reference_root().join(build).join("users.tsv")
}

/// The game as lint's deep rules read it. Without a users index for the build, the shared-file rule
/// (M090) can't run, and lint says so.
fn game_files(args: &[String], build: &str) -> Result<gamefiles::GameFiles, String> {
    let game = game_dir(args)?;
    let idx = flag(args, "--users").map(PathBuf::from).unwrap_or_else(|| users_index_path(build));
    let mut gf = gamefiles::GameFiles::open(&game, Some(&idx));
    if !gf.has_index {
        say!("note: no users index for build {build} ({}); run `magician index` to check shared files (M090)", idx.display());
    }
    let cache = reference_root().join(build).join("particle_classes.tsv");
    match gamefiles::particle_classes(&game, &cache, build) {
        Ok(c) => gf.particle_classes = Some(c),
        Err(e) => say!("note: Valve's particle classes unavailable ({e}); particle class lists (M083) not checked"),
    }
    Ok(gf)
}

fn cmd_index(args: &[String]) -> R {
    let game = game_dir(args)?;
    let build = game_build(&game)?;
    let out = flag(args, "--out").map(PathBuf::from).unwrap_or_else(|| users_index_path(&build));
    let t = std::time::Instant::now();
    let s = gamefiles::build_index(&game, &out, &build, &grade_sources(args)?)?;
    say!(
        "build {build}: {} compiled files, {} references, {} owners, {} files with users -> {} ({:.0}s)",
        s.files, s.edges, s.owners, s.targets, out.display(), t.elapsed().as_secs_f64()
    );
    Ok(ExitCode::SUCCESS)
}

fn cmd_lint(args: &[String]) -> R {
    let p = positional(args);
    lint_vpk(Path::new(p.first().ok_or("lint <mod.vpk>")?), args)
}

fn cmd_pack(args: &[String]) -> R {
    let p = positional(args);
    let src = PathBuf::from(p.first().ok_or("pack <src_dir> <out.vpk>")?);
    let out = PathBuf::from(p.get(1).ok_or("pack <src_dir> <out.vpk>")?);
    let mut files = Vec::new();
    walk(&src, &src, &mut files).map_err(|e| e.to_string())?;
    files.sort_by(|a, b| a.0.cmp(&b.0));
    vpk::write(&out, &files).map_err(|e| e.to_string())?;
    say!("packed {} files -> {}", files.len(), out.display());
    lint_vpk(&out, args)
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, Vec<u8>)>) -> std::io::Result<()> {
    walk_at(root, dir, out, 0)
}

fn walk_at(root: &Path, dir: &Path, out: &mut Vec<(String, Vec<u8>)>, depth: usize) -> std::io::Result<()> {
    if depth > build::MAX_DEPTH {
        return Err(build::too_deep(dir));
    }
    for e in std::fs::read_dir(dir)? {
        let p = e?.path();
        if p.is_dir() {
            walk_at(root, &p, out, depth + 1)?;
        } else {
            // Paths are packed exactly as they are on disk; case problems surface as M001, not silently fixed.
            let rel = p.strip_prefix(root).unwrap_or(&p).to_string_lossy().replace('\\', "/");
            out.push((rel, std::fs::read(&p)?));
        }
    }
    Ok(())
}

/// Parse every compiled resource under a prefix in a game VPK and resolve its references against the
/// installed build. Proves the resource reader on real data and maps where the engine itself has
/// references that don't resolve to files (which the dangling-ref rule must not flag).
fn cmd_scan(args: &[String]) -> R {
    let p = positional(args);
    let vp = Path::new(p.first().ok_or("scan <vpk> [prefix]")?);
    let prefix = p.get(1).map(|s| s.as_str()).unwrap_or("");
    let v = open(vp)?;
    let base = load_base(args)?;
    let (mut ok, mut bad, mut refs) = (0usize, 0usize, 0usize);
    let mut unresolved: std::collections::BTreeMap<String, Vec<String>> = Default::default();
    let mut parse_errors: std::collections::BTreeMap<String, usize> = Default::default();
    // Read in physical order (archive, offset). The game lives on a spinning disk here; path order
    // turns every read into a random seek across 26 GB of chunks (~14 ms each).
    let mut todo: Vec<&vpk::Entry> = v.entries.iter().filter(|e| e.path.starts_with(prefix) && e.path.ends_with("_c")).collect();
    todo.sort_by_key(|e| (e.archive_index, e.offset));
    for e in todo {
        let data = v.read(e).map_err(|e| e.to_string())?;
        match resource::parse(&data) {
            Err(err) => {
                bad += 1;
                *parse_errors.entry(format!("{} ({})", err, e.path.rsplit('.').next().unwrap_or(""))).or_default() += 1;
            }
            Ok(r) => {
                ok += 1;
                for x in &r.ext_refs {
                    refs += 1;
                    let t = x.compiled_path();
                    if !x.name.is_empty() && !x.name.starts_with('_') && !base.contains(&t) {
                        unresolved.entry(t).or_default().push(e.path.clone());
                    }
                }
            }
        }
    }
    say!("{ok} parsed, {bad} failed, {refs} references, {} unresolved targets", unresolved.len());
    for (k, n) in &parse_errors {
        say!("  parse error x{n}: {k}");
    }
    for (t, from) in unresolved.iter().take(40) {
        say!("  unresolved {t}  (from {} file(s), e.g. {})", from.len(), from[0]);
    }
    Ok(ExitCode::SUCCESS)
}

fn grade_sources(args: &[String]) -> Result<grade::Sources, String> {
    let game = game_dir(args)?;
    let build = game_build(&game)?;
    Ok(grade::Sources {
        decompiled: reference_root().join(&build).join("decompiled"),
        vrf_cli: paths::vrf(flag(args, "--vrf")).path,
        game,
    })
}

/// The heroes.vdata key of a hero named any way a person names one (`lash`, `Lash`, `hero_lash`, a model path), read
/// from the game's own hero table. Where that table can't be read the name goes on as given, for the footprint to
/// resolve; a name the table does not hold is a usage error that says which hero was most likely meant.
fn hero_key(arg: &str, args: &[String]) -> Result<String, Stop> {
    let Ok(game) = game_dir(args) else { return Ok(arg.to_string()) };
    match heroes::Heroes::from_game(&game) {
        Ok(t) => t.resolve(arg).map(|h| h.key.clone()).map_err(Stop::Usage),
        Err(_) => Ok(arg.to_string()),
    }
}

fn cmd_footprint(args: &[String]) -> R {
    let p = positional(args);
    let hero = hero_key(p.first().ok_or("footprint <hero>")?, args)?;
    let fp = grade::footprint::build(&grade_sources(args)?, &hero)?;
    say!("{} ({}), tokens {:?}", fp.display, fp.hero_key, fp.tokens);
    for (slot, ab) in &fp.abilities {
        say!("  slot {slot:<22} {ab}");
    }
    let mut last = None;
    for ((cat, group), n) in grade::footprint::summary(&fp) {
        if last != Some(cat) {
            say!("{}{}", cat.label(), if cat.protected() { "  (never ship)" } else { "" });
            last = Some(cat);
        }
        say!("    {group:<36} {n}");
    }
    if flag(args, "--list").is_some() || args.iter().any(|a| a == "--list") {
        for i in &fp.items {
            say!("{}	{}	{}", i.cat.label(), i.group, i.path);
        }
    }
    say!("{} files", fp.items.len());
    Ok(ExitCode::SUCCESS)
}

/// `magician mesh`, judged at the fair-play limits in force.
fn cmd_mesh(args: &[String]) -> R {
    let game = compare_game(args)?;
    let policy = load_policy(args)?;
    Ok(meshcmd::cmd_mesh(args, &game, &reference_root(), &policy)?)
}

/// `magician rubric`: the criteria, their groups, the effective weights and lines, and the fair-play classes (the
/// policy in force).
fn cmd_rubric(args: &[String]) -> R {
    let p = load_policy(args)?;
    say!("Grading rubric {} (policy {}, fingerprint {})", grade::rubric::RULES_VERSION, p.file.as_deref().unwrap_or("built-in"), &p.fingerprint()[..12]);
    say!("A grade names the kind of mod and which of the hero's look, effects and sound it makes its own.");
    say!("Kinds: new character -> Echo; same character, new model -> Costume; Valve's mesh kept -> Variant; no model -> Kit; not a hero mod -> not graded");
    say!("Criteria:");
    for g in grade::Group::all() {
        for c in grade::CRITERIA.iter().filter(|c| c.group == g) {
            say!("  {:<8} {:<10} {:<22} {}", g.id(), c.id, c.name, c.why);
        }
    }
    say!(
        "A group is named once its weighted coverage reaches {} (groups.covered), reads \"some <group>\" from {} (groups.touched; above nothing), and is left out while Valve's",
        jsonout::num(p.groups.covered),
        jsonout::num(p.groups.touched)
    );
    for a in grade::Archetype::all() {
        let (_, app) = a.surfaces();
        let w: Vec<String> = p.weights(a).iter().map(|(id, w)| format!("{id} {w}{}", if app.contains(&id.as_str()) { "*" } else { "" })).collect();
        say!("{} weights within each group: {}{}", a.id(), w.join(", "), if app.is_empty() { String::new() } else { "  (* counts once the mod touches it)".into() });
    }
    say!("Flags: broken (lint errors that are not fair play, as `magician check` calls it), NSFR or FP? (fair play), stale(N) or unproven (freshness)");
    say!("Fair play: never allowable {}; advisory {}; M140 allowable up to {} u worst point and {}% of the visible surface", p.compliance.never.join(" "), p.compliance.advisory.join(" "), jsonout::num(p.m140_limits().0), jsonout::num(100.0 * p.m140_limits().1));
    say!(
        "Fair play is {}; rights are {}",
        if p.enforce { "enforced: its findings are errors, and NSFR or FP? blocks the release gate" } else { "lenient (fair_play.enforce = false): its findings are warnings, and NSFR or FP? is a warning at the release gate" },
        if p.rights_enforce { "enforced: unreviewed blocks the release gate" } else { "lenient (rights.enforce = false): unreviewed is a warning at the release gate" }
    );
    say!(
        "Release gate: lint errors that are not fair play, Valve's own files, {}not proven on the target build, rights {}",
        if p.enforce { "fair play unknown, NSFR, " } else { "" },
        if p.rights_enforce { "not cleared" } else { "denied" }
    );
    Ok(ExitCode::SUCCESS)
}

/// The policy in force: `--policy FILE`, `MAGICIAN_POLICY`, `policy =` in the config file, else the built-in
/// defaults. A policy that names a missing file or does not read is a usage error, never ignored.
fn load_policy(args: &[String]) -> Result<grade::Policy, Stop> {
    let Some(f) = paths::policy(flag(args, "--policy")) else { return Ok(grade::Policy::default()) };
    let text = std::fs::read_to_string(&f.path).map_err(|e| format!("policy {} (from {}): {e}", f.path.display(), f.source))?;
    let name = f.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| f.path.display().to_string());
    grade::policy::parse(&text, &name).map_err(|e| Stop::Usage(format!("policy {} (from {}): {e}", f.path.display(), f.source)))
}

/// `magician policy`: every judgement value in force, where each came from, and the fingerprint a grade records.
fn cmd_policy(args: &[String]) -> R {
    let p = load_policy(args)?;
    match paths::policy(flag(args, "--policy")) {
        Some(f) => say!("policy\t{}\t{}", f.path.display(), f.source),
        None => say!("policy\t-\tbuilt-in (no --policy, MAGICIAN_POLICY or `policy =` in the config file)"),
    }
    say!("fingerprint\t{}\t{}", p.fingerprint(), if p.overrides().is_empty() { "the defaults".to_string() } else { format!("{} key(s) moved: {}", p.overrides().len(), p.overrides().join(", ")) });
    for (k, v, src) in p.entries() {
        say!("{k}\t{v}\t{src}");
    }
    Ok(ExitCode::SUCCESS)
}

/// A pak's slot number for "the lower pak number wins": pak06 before pak12; any other name after them, in given order.
fn slot(p: &Path) -> u32 {
    let n = p.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    n.strip_prefix("pak").and_then(|r| r.get(..2)).and_then(|d| d.parse().ok()).unwrap_or(100)
}

/// The allowances a grade honours: every `--allow FILE` and the `magician.allow` of every `--addon-root DIR`; with
/// neither, the mod's own files found from its `mod.toml` (`paths::mod_allowances`). Returns the allowances and one
/// line per file read (name, SHA-256, where it was found) for the record, or one line saying none was found and where
/// the grade looked, which is also said on stderr: a grade without its allowances can read NSFR for a reason the
/// author already wrote down.
fn grade_allowances(args: &[String], toml: Option<(&Path, &str)>) -> Result<(Vec<lint::allow::Allowance>, Vec<String>), String> {
    let mut files: Vec<paths::Found> = flags(args, "--allow").into_iter().map(|p| paths::Found { path: PathBuf::from(p), source: "--allow".into() }).collect();
    let roots = flags(args, "--addon-root");
    files.extend(roots.iter().map(|r| PathBuf::from(r).join("magician.allow")).filter(|p| p.exists()).map(|path| paths::Found { path, source: "--addon-root".into() }));
    let mut looked: Vec<String> = Vec::new();
    if files.is_empty() && roots.is_empty() {
        if let Some((p, text)) = toml {
            let csdk = paths::csdk(flag(args, "--csdk")).ok().map(|f| f.path);
            let (found, places) = paths::mod_allowances(p, text, csdk.as_deref());
            files = found;
            looked = places.iter().map(|l| l.display().to_string()).collect();
            if csdk.is_none() {
                looked.push("no CSDK configured, so no CSDK addon".into());
            }
        }
    } else if files.is_empty() {
        looked = roots.iter().map(|r| PathBuf::from(r).join("magician.allow").display().to_string()).collect();
    }
    let mut all = Vec::new();
    let mut names = Vec::new();
    for f in files {
        let p = &f.path;
        let text = std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?;
        let a = lint::allow::parse(&text).map_err(|e| format!("{}: {e}", p.display()))?;
        say!("allowances: {} from {} ({})", a.len(), p.display(), f.source);
        names.push(format!("allowances {} (sha256 {}, {})", p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(), &grade::sha256::hex(text.as_bytes())[..12], f.source));
        all.extend(a);
    }
    if names.is_empty() {
        let why = if looked.is_empty() { "no --allow, --addon-root or --mod".to_string() } else { format!("looked in {}", looked.join("; ")) };
        errln!("magician: note: no allowance file, so every finding counts ({why}); pass --allow FILE if the mod has one");
        names.push(if toml.is_some() { "allowances: none found beside the mod.toml or in its CSDK addon".to_string() } else { "allowances: none given".to_string() });
    }
    Ok((all, names))
}

/// The commit the mod's sources (the folder of its mod.toml) are at, when that folder is a git checkout.
fn source_commit(toml: &Path) -> Option<String> {
    let dir = toml.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let o = std::process::Command::new("git").arg("-C").arg(dir).args(["rev-parse", "HEAD"]).output().ok()?;
    o.status.success().then(|| String::from_utf8_lossy(&o.stdout).trim().to_string()).filter(|s| !s.is_empty())
}

/// SHA-256 of the baseline the grade judged against (its canonical text) and the build's rig sidecar, when there is one.
fn baseline_fingerprint(base: &Baseline) -> Result<String, String> {
    let mut h = grade::sha256::Sha256::new();
    let mut text = Vec::new();
    base.write(&mut text).map_err(|e| e.to_string())?;
    h.update(&text);
    if let Ok(rig) = std::fs::read(patchday::rig_path(&reference_root(), &base.build)) {
        h.update(&rig);
    }
    Ok(h.hex())
}

/// `magician grade`: what one mod (every pak it ships) makes its own of the hero, identity, broken, fair play,
/// freshness, rights and the release gate, as a label and a record. Exit 0 whatever the grade (it is information); with `--gate`, exit 1 when the
/// release gate has a blocker.
fn cmd_grade(args: &[String]) -> R {
    let g = grade_run(args)?;
    if json_mode() {
        outln!("{}", g.json);
    } else {
        out!("{}", g.text);
    }
    let gate = args.iter().any(|a| a == "--gate");
    Ok(if gate && !g.gate_passes { ExitCode::from(cli::BROKEN) } else { ExitCode::SUCCESS })
}

/// A grade, as the commands that report one (`grade`, `check`) read it.
pub(crate) struct GradeOut {
    pub text: String,
    pub json: String,
    pub label: String,
    pub gate_passes: bool,
    /// Lint errors that are not fair play: the grade's `broken` flag, and check's verdict.
    pub broken: bool,
    pub blockers: Vec<String>,
    pub hero: String,
    pub fair_play: &'static str,
    pub lint_errors: usize,
    pub lint_warnings: usize,
    pub lint_allowed: usize,
    pub lint_depth: &'static str,
}

/// `grade`'s work, without printing the record (`--record-out` is still written).
pub(crate) fn grade_run(args: &[String]) -> Result<GradeOut, Stop> {
    let usage = "grade <pak>... [--hero HERO | --mod mod.toml] [--target BUILD] [--light | --findings FILE] [--allow FILE]... [--addon-root DIR]... [--scene-root DIR]... [--gate] [--json]";
    let policy = load_policy(args)?;
    let mut paks: Vec<PathBuf> = positional(args).iter().map(PathBuf::from).collect();
    if paks.is_empty() {
        return Err(usage.into());
    }
    let toml_path = flag(args, "--mod").map(PathBuf::from);
    let toml = match &toml_path {
        Some(p) => Some(std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?),
        None => None,
    };
    let t = toml.as_deref().unwrap_or("");
    let hero = flag(args, "--hero").or_else(|| grade::manifest::string(t, "mod", "hero")).ok_or("--hero <hero> is required (or a --mod whose [mod] hero names it)")?;
    let hero = hero_key(&hero, args)?;
    let rights = grade::record::Rights::parse(grade::manifest::string(t, "rights", "state").as_deref())?;

    // Every pak of the mod as one: where two ship a path, the lower pak number wins, as in game.
    paks.sort_by_key(|p| slot(p));
    let mut files: Vec<ModFile> = Vec::new();
    let mut owner: std::collections::BTreeMap<String, String> = Default::default();
    let mut shadowed: Vec<String> = Vec::new();
    let mut pak_records = Vec::new();
    for p in &paks {
        let v = open(p)?;
        let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let these = ModFile::from_vpk(&v).map_err(|e| e.to_string())?;
        pak_records.push(grade::record::Pak { name: name.clone(), sha256: grade::sha256::file(p).map_err(|e| format!("{}: {e}", p.display()))?, files: these.len() });
        for f in these {
            match owner.get(&f.path) {
                Some(w) => shadowed.push(format!("{} ({w} wins over {name})", f.path)),
                None => {
                    owner.insert(f.path.clone(), name.clone());
                    files.push(f);
                }
            }
        }
    }
    let first_name = pak_records.first().map(|p| p.name.clone()).unwrap_or_default();
    let base = load_base(args)?;
    let target = flag(args, "--target").unwrap_or_else(|| base.build.clone());
    let originals = original_refs(&files, args)?;
    let game = game_dir(args)?;
    let valve_game = gamefiles::GameFiles::open(&game, None);

    // The lint the grade judges: the release's own run when it hands one over (`--findings`), else a run made here,
    // deep unless `--light` (fair play then stays unknown for a model: the mesh rules did not run).
    let (findings, depth, lint_source) = match flag(args, "--findings") {
        Some(ff) => {
            if paks.len() > 1 {
                return Err("--findings is one pak's lint run; to grade several paks, let grade lint them together".into());
            }
            let text = std::fs::read_to_string(&ff).map_err(|e| format!("{ff}: {e}"))?;
            let (d, f) = lint::report::read(&text).map_err(|e| format!("{ff}: {e}"))?;
            let name = Path::new(&ff).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            (f, d, format!("the release's own run, {name}"))
        }
        None => {
            let deep = !args.iter().any(|a| a == "--light");
            let gf = game_files(args, &base.build)?;
            let mut f = lint::lint(&Input { vpk_name: first_name.clone(), files: files.clone(), base: Some(&base), built_against: None, original_refs: &originals, game: Some(&gf), deep, fair_play: &policy.fair_play });
            grade::compliance::set_severities(&mut f, &policy);
            (f, lint::report::Depth::from_flag(deep), "linted here".to_string())
        }
    };
    let (allowances, allow_names) = grade_allowances(args, toml_path.as_deref().zip(toml.as_deref()))?;
    let lf = grade::lintfacts::lint_facts(findings.clone(), &allowances, depth);

    // What the grade credits: every path, less files byte-identical to Valve's at the same path (they change nothing).
    let valve_files: Vec<String> = files.iter().filter(|f| base.get(&f.path).is_some_and(|b| b.crc == f.stored_crc && b.size == f.data.len() as u64)).map(|f| f.path.clone()).collect();
    let paths: std::collections::BTreeSet<String> = files.iter().map(|f| f.path.clone()).filter(|p| !valve_files.contains(p)).collect();
    let by_path: std::collections::HashMap<&str, &ModFile> = files.iter().map(|f| (f.path.as_str(), f)).collect();

    let sources = grade_sources(args)?;
    let fp = grade::footprint::build(&sources, &hero)?;
    // UI scenes are separate map VPKs: looked for beside each pak (`<dir>/maps/ui/...`, where `release` leaves them)
    // and under every `--scene-root`. Only a scene with entities of its own counts.
    let mut scene_roots: Vec<PathBuf> = flags(args, "--scene-root").into_iter().map(PathBuf::from).collect();
    for p in &paks {
        if let Some(d) = p.parent().filter(|d| !d.as_os_str().is_empty()) {
            if !scene_roots.contains(&d.to_path_buf()) {
                scene_roots.push(d.to_path_buf());
            }
        }
    }
    let game_citadel = game.join("citadel");
    let scene_list: Vec<String> = fp.by_cat(grade::footprint::Cat::Scene).map(|i| i.path.clone()).collect();
    let scene_maps = grade::evidence::carried_scenes(&scene_list, &scene_roots, &|rel| vpk::Vpk::open(game_citadel.join(rel)).ok(), &mut |m| say!("{m}"))?;
    let core = fp.by_cat(grade::footprint::Cat::Model).find(|i| i.group == "core").map(|i| i.path.clone()).unwrap_or_default();
    let ours_model = by_path.get(core.as_str()).filter(|_| paths.contains(&core)).map(|f| f.data.clone());
    let valve_model = lint::Game::read(&valve_game, &core);
    let model_materials = ours_model.as_deref().map(|m| grade::evidence::model_materials(m, &paths)).unwrap_or_default();

    // Identity: what kind of mod this is.
    let portrait: Vec<&grade::Item> = fp.by_cat(grade::footprint::Cat::UiArt).filter(|i| i.group == "ui" && !i.path.contains("/hero_names/") && !i.path.contains("/guns/")).collect();
    let loc: Vec<(String, String)> = files.iter().filter(|f| f.path.starts_with("resource/localization/") && f.path.ends_with(".txt")).map(|f| (f.path.clone(), String::from_utf8_lossy(&f.data).into_owned())).collect();
    let declared_name = grade::manifest::string(t, "identity", "name").or_else(|| grade::manifest::string(t, "mod", "name"));
    let rename = grade::evidence::renamed(&fp.hero_key, &fp.display, &fp.name_art, &paths, &loc, declared_name.as_deref());
    let signals = grade::identity::Signals {
        model_replaced: ours_model.is_some(),
        geometry: match (&ours_model, &valve_model) {
            (Some(o), Some(v)) => Some(grade::evidence::compare_models(o, v)),
            _ => None,
        },
        renamed: rename.0,
        rename_notes: rename.1,
        portrait: (portrait.iter().filter(|i| paths.contains(&i.path)).count(), portrait.len()),
        voice_declared: grade::manifest::string(t, "identity", "voice").as_deref() == Some("new"),
        materials_replaced: fp.by_cat(grade::footprint::Cat::Materials).any(|i| paths.contains(&i.path)),
        touches_hero: fp.items.iter().any(|i| paths.contains(&i.path)) || paths.iter().any(|p| fp.tokens.iter().any(|t| grade::footprint::has_token(p, t))),
        other_kind: grade::identity::other_kind(paths.iter()),
    };
    let identity = grade::identity::decide(&signals, &policy);
    let material_slots = match (identity.kind, &valve_model) {
        (grade::identity::Kind::SameModel, Some(v)) => grade::evidence::material_slots(v, ours_model.as_deref(), &paths).ok(),
        _ => None,
    };

    // Fair play, from the same run.
    let replaced = |ext: &str| paths.iter().any(|p| p.ends_with(ext) && base.contains(p));
    let applicable = grade::compliance::Applicable { models: replaced(".vmdl_c"), particles: replaced(".vpcf_c"), sounds: replaced(".vsndevts_c") || replaced(".vsnd_c") };
    let fair_play = grade::compliance::judge(&findings, &allowances, depth, applicable, &policy);

    let facts = grade::ModFacts {
        model_anim_changed: model_anim_changed(&files, args).unwrap_or(false),
        paths,
        model_materials,
        scene_maps: scene_maps.clone(),
        dropped_deps: lf.dropped_deps,
        material_slots,
    };
    // Freshness is asked of these paks' bytes: the builds a recorded proof holds every one of them on (proofs.rs).
    let freshness = match &toml_path {
        Some(p) => {
            let ledger = proofs::Ledger::for_toml(p)?;
            for problem in &ledger.problems {
                errln!("magician: note: {}: {problem} (the line counts for nothing)", ledger.path.as_ref().map(|p| p.display().to_string()).unwrap_or_default());
            }
            let shas: Vec<String> = pak_records.iter().map(|p| p.sha256.clone()).collect();
            let b = proofs::Binding::of(&ledger, &grade::manifest::proven(t), &shas);
            grade::record::Freshness::bound(&b.bound, &b.unbound(), &target)
        }
        None => grade::record::Freshness::new(&[], &target),
    };
    let mut evidence: Vec<String> = grade::manifest::list(t, "proven", "evidence");
    evidence.extend(scene_maps.iter().map(|s| format!("scene {s}")));
    let record = grade::record::build(grade::record::Inputs {
        toolchain: cli::version(),
        build: base.build.clone(),
        baseline_fingerprint: baseline_fingerprint(&base)?,
        policy: &policy,
        slug: grade::manifest::string(t, "mod", "slug"),
        name: grade::manifest::string(t, "mod", "name"),
        source_commit: toml_path.as_deref().and_then(source_commit),
        paks: pak_records,
        footprint: &fp,
        facts,
        lint: &lf,
        lint_source,
        fair_play,
        identity,
        valve_files,
        shadowed,
        freshness,
        rights,
        rights_notes: grade::manifest::string(t, "rights", "notes").or_else(|| grade::manifest::string(t, "rights", "policy")).unwrap_or_default(),
        allowance_files: allow_names,
        evidence,
    });
    if let Some(out) = flag(args, "--record-out") {
        write_output("--record-out", &out, &format!("{}\n", record.json()))?;
    }
    Ok(GradeOut {
        text: record.text(&style::mark),
        json: record.json(),
        label: record.label.clone(),
        gate_passes: record.gate_passes(),
        broken: record.broken(),
        blockers: record.blockers.clone(),
        hero: record.i.footprint.display.clone(),
        fair_play: record.i.fair_play.state.id(),
        lint_errors: record.i.lint.errors,
        lint_warnings: record.i.lint.warnings,
        lint_allowed: record.i.lint.allowed_by_code.values().sum(),
        lint_depth: record.i.lint.depth.label(),
    })
}

/// Compare the ANIM/ASEQ blocks of every hero model in the mod with Valve's original. The select,
/// shop and postgame poses and the deaths live there as legacy sequences.
fn model_anim_changed(files: &[ModFile], args: &[String]) -> Result<bool, String> {
    let game = game_dir(args)?;
    let pak_path = game.join("citadel").join("pak01_dir.vpk");
    let pak = vpk::Vpk::open_shared(&pak_path).map_err(|e| format!("{}: {e}", pak_path.display()))?;
    let blocks = |data: &[u8]| -> Vec<Vec<u8>> {
        resource::parse(data)
            .map(|r| r.blocks.iter().filter(|b| &b.tag == b"ANIM" || &b.tag == b"ASEQ").map(|b| data[b.offset..b.offset + b.size].to_vec()).collect())
            .unwrap_or_default()
    };
    for f in files.iter().filter(|f| f.path.ends_with(".vmdl_c")) {
        let Some(orig) = pak.find(&f.path) else { continue };
        let a = blocks(&f.data);
        let b = blocks(&pak.read(orig).map_err(|x| x.to_string())?);
        if !a.is_empty() && a != b {
            return Ok(true);
        }
    }
    Ok(false)
}

fn cmd_expr(args: &[String]) -> R {
    let mut attrs = expr::default_attrs();
    let mut caps: Vec<(String, f64)> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--attr" || args[i] == "--cap" {
            let v = args.get(i + 1).ok_or("--attr name=lo:hi / --cap PARAM=max")?;
            let (k, r) = v.split_once('=').ok_or("expected name=value")?;
            if args[i] == "--attr" {
                let (lo, hi) = r.split_once(':').ok_or("--attr name=lo:hi")?;
                attrs.insert(k.to_string(), expr::Iv::new(lo.parse().map_err(|_| "bad lo")?, hi.parse().map_err(|_| "bad hi")?));
            } else {
                caps.push((k.to_string(), r.parse().map_err(|_| "bad cap")?));
            }
            i += 2;
        } else {
            i += 1;
        }
    }
    let items: Vec<(String, String)> = match flag(args, "--vmat") {
        Some(f) => expr::vmat_dynamic_params(&std::fs::read_to_string(&f).map_err(|e| format!("{f}: {e}"))?),
        None => vec![("expr".into(), positional(args).first().ok_or("expr \"<expression>\"")?.to_string())],
    };
    let mut bad = 0;
    for (param, src) in &items {
        match expr::parse(src) {
            Err(e) => {
                bad += 1;
                say!("{param}: PARSE ERROR {e}");
            }
            Ok(p) => {
                let a = expr::analyze(&p, &attrs);
                let r = a.value.range();
                let mut flags = Vec::new();
                for n in &a.notes {
                    flags.push(match n {
                        expr::Note::DivideByZero => { bad += 1; "DIVIDE-BY-ZERO possible".to_string() }
                        expr::Note::UnknownAttr(x) => format!("unknown input ${x} (assumed unbounded; give --attr {x}=lo:hi)"),
                        expr::Note::Random => "random() per frame".to_string(),
                        expr::Note::UsesTime => "time()".to_string(),
                        expr::Note::UnknownFunction(f) => { bad += 1; format!("UNKNOWN FUNCTION {f}") }
                    });
                }
                let mut verdict = if r.bounded() { "bounded" } else { bad += 1; "UNBOUNDED" };
                for (cp, max) in &caps {
                    if param == cp && r.hi > *max {
                        bad += 1;
                        verdict = "EXCEEDS CAP";
                    }
                }
                say!("{param}: {}  range {}  cost {} ops  [{verdict}]", a.value, r, a.cost);
                for f in flags {
                    say!("    - {f}");
                }
            }
        }
    }
    Ok(if bad > 0 { ExitCode::from(1) } else { ExitCode::SUCCESS })
}

/// Every value of a repeatable flag (`--with a --with b`).
fn flags(args: &[String], name: &str) -> Vec<String> {
    args.windows(2).filter(|w| w[0] == name).map(|w| w[1].clone()).collect()
}

/// `--no-wait` fails at once when another build holds the addon; `--lock-timeout SECS` bounds the wait (default 1800).
fn lock_options(args: &[String]) -> Result<lock::Options, String> {
    let mut o = lock::Options::default();
    o.no_wait = args.iter().any(|a| a == "--no-wait");
    if let Some(t) = flag(args, "--lock-timeout") {
        o.timeout = std::time::Duration::from_secs(t.parse().map_err(|_| format!("--lock-timeout wants seconds, got {t}"))?);
    }
    Ok(o)
}

struct Built {
    package: build::Package,
    content: PathBuf,
}

/// Compile one CSDK addon (unless `--no-compile`) and choose what ships. See `build.rs` for the rules.
/// `explicit_bin` is `--bin`, honoured only for a single-addon build: a release compiles several addons, and each
/// gets its own compiler from its `.magician-compiler` marker or from its clips (build::choose_bin).
fn compile_addon(addon: &str, args: &[String], compile: bool, explicit_bin: Option<&str>) -> Result<Built, Stop> {
    paths::addon_name(addon)?;
    let csdk = paths::csdk(flag(args, "--csdk"))?.path;
    let content = csdk.join("content").join("citadel_addons").join(addon);
    let game_out = csdk.join("game").join("citadel_addons").join(addon);
    if !content.is_dir() {
        return Err(format!("no addon sources at {}", content.display()).into());
    }
    // One run at a time per addon: the compiler writes into game_out and packaging reads it back.
    let _lock = lock::acquire(
        &game_out,
        &format!("magician {} {addon}", std::env::args().nth(1).unwrap_or_default()),
        &lock_options(args)?,
        &mut |m| say!("{addon}: {m}"),
    )?;
    let mut rels = Vec::new();
    build::rel_files(&content, &content, &mut rels).map_err(|e| format!("{}: {e}", content.display()))?;
    let marker = std::fs::read_to_string(content.join(build::COMPILER_MARKER))
        .ok()
        .and_then(|t| t.trim_start_matches('\u{feff}').lines().map(str::trim).find(|l| !l.is_empty() && !l.starts_with('#')).map(str::to_string));
    let needs = build::needs_nm(&content, &rels);
    let (bin, why) = build::choose_bin(&csdk.join("game").join("bin"), explicit_bin, marker.as_deref(), needs.as_ref())
        .map_err(|e| format!("{addon}: {e}"))?;
    let rc = bin.join("resourcecompiler.exe");
    if compile
        && !rc.is_file() {
            return Err(format!("{addon}: no compiler at {} ({why})", rc.display()).into());
        }
    say!("{addon}: compiler {} ({why}){}", bin.display(), if compile { "" } else { ", not run (--no-compile)" });
    let mut sources = build::classify(&rels);
    // Nothing changed since the last good compile (same sources, same compiler, same outputs): resourcecompiler
    // would spend ~10 s starting up to report "up to date", so it isn't started. `--force-compile` runs it anyway.
    let stamp_path = game_out.join(build::BUILD_STAMP);
    let in_fp = if compile { build::tree_fingerprint(&content, &rels, &build::file_id(&rc)) } else { 0 };
    let up_to_date = compile
        && !args.iter().any(|a| a == "--force-compile")
        && std::fs::read_to_string(&stamp_path).is_ok_and(|t| {
            let outs = build::compiled_outputs(&game_out);
            t.trim() == format!("{in_fp:016x} {:016x}", build::tree_fingerprint(&game_out, &outs, "")) && !outs.is_empty()
        });
    if up_to_date {
        say!("compiled {addon}: up to date, no source changed since the last compile (resourcecompiler not started; --force-compile runs it)");
    }
    if compile && !up_to_date && !cfg!(windows) {
        // Valve's compiler is a Windows program (README.md, Platforms): say so here rather than fail to start it.
        return Err(Stop::Usage(format!(
            "{addon}: its sources changed since the last compile, and resourcecompiler.exe runs on Windows only: compile \
             on Windows, or package what is already compiled with --no-compile"
        )));
    }
    if compile && !up_to_date {
        // A UTF-8 BOM makes resourcecompiler reject a text source. Refuse rather than strip: the sources belong to
        // the departments (often mid-edit by another tool), the build never writes into them, and a silent fix
        // would hide the editor setting that keeps adding the BOM.
        let boms = build::bom_sources(&content, &rels);
        if !boms.is_empty() {
            for b in &boms {
                say!("  BOM: {}", content.join(build::native(b)).display());
            }
            return Err(Stop::Broken(format!(
                "{} source file(s) start with a UTF-8 byte-order mark, which resourcecompiler rejects (\"Expecting a .vtex file\"). \
                 Re-save without one: [IO.File]::WriteAllText(path, text) in Windows PowerShell, or Set-Content -Encoding utf8NoBOM \
                 in PowerShell 7; never Set-Content -Encoding utf8 in Windows PowerShell 5.1",
                boms.len()
            )));
        }
        let started = std::time::Instant::now();
        std::fs::create_dir_all(&game_out).map_err(|e| e.to_string())?;
        let log_path = game_out.join("magician_build.log");
        let mut log = std::fs::File::create(&log_path).map_err(|e| format!("{}: {e}", log_path.display()))?;
        // PSDs no image list in the tree names get a temporary list of their own.
        let has_list = sources.roots.iter().any(|r| {
            r.ends_with(".vdata") && std::fs::read_to_string(content.join(r)).is_ok_and(|t| t.contains("panorama_image_list"))
        });
        let temp_list = "panorama/_magician_image_list.vdata";
        if !sources.loose_psds.is_empty() && !has_list {
            std::fs::create_dir_all(content.join("panorama")).map_err(|e| e.to_string())?;
            std::fs::write(content.join(temp_list), build::image_list(&sources.loose_psds)).map_err(|e| e.to_string())?;
            sources.roots.push(temp_list.into());
        }
        let paths: Vec<PathBuf> = sources.roots.iter().map(|r| content.join(build::native(r))).collect();
        let (skels, rest): (Vec<PathBuf>, Vec<PathBuf>) = paths.into_iter().partition(|p| p.extension().is_some_and(|e| e == "vnmskel"));
        // model documents with distance fields compile against the gameinfo that allows them (build::distance_field_game)
        let dstf: Vec<PathBuf> = build::needs_distance_fields(&content, &sources.roots).iter().map(|r| content.join(build::native(r))).collect();
        let dstf_game = if dstf.is_empty() { None } else { build::distance_field_game(&csdk).map_err(|e| format!("{addon}: {e}"))? };
        if let Some(g) = &dstf_game {
            say!("{addon}: {} model document(s) with distance fields compile with -game {}", dstf.len(), g.display());
        }
        let (dstf, rest): (Vec<PathBuf>, Vec<PathBuf>) = rest.into_iter().partition(|p| dstf_game.is_some() && dstf.contains(p));
        let mut summary = build::CompileSummary::default();
        let mut result = Ok(());
        let plain = [skels].into_iter().filter(|b| !b.is_empty()).chain(build::batches(&rest)).map(|b| (b, None));
        let with_fields = build::batches(&dstf).into_iter().map(|b| (b, dstf_game.as_deref()));
        for (batch, game) in plain.chain(with_fields) {
            if let Err(e) = build::compile_batch(&rc, &batch, game, &mut log, &mut summary) {
                result = Err(e);
                break;
            }
        }
        // The temporary list and its compiled form exist only for the compile.
        let _ = std::fs::remove_file(content.join(temp_list));
        let _ = std::fs::remove_file(game_out.join(format!("{temp_list}_c")));
        result?;
        let ignored: Vec<String> = sources.ignored.iter().map(|(e, n)| format!("{n} .{e}")).collect();
        say!(
            "compiled {addon} in {:.0}s: {} source(s) given to resourcecompiler ({} compiled, {} up to date, {} failed); {} input(s) compiled through them; not sources: {}",
            started.elapsed().as_secs_f64(),
            sources.roots.len(),
            summary.compiled,
            summary.skipped,
            summary.failed,
            sources.inputs,
            if ignored.is_empty() { "none".into() } else { ignored.join(", ") }
        );
        if summary.warnings > 0 {
            say!("  {} compiler warning(s); full log {}", summary.warnings, log_path.display());
        }
        if summary.hung_killed > 0 {
            say!("  resourcecompiler hung after its summary {} time(s) and was ended (known CSDK exit hang)", summary.hung_killed);
        }
        if summary.failed > 0 || !summary.errors.is_empty() {
            say!("  resourcecompiler errors ({}):", summary.errors.len());
            for e in summary.errors.iter().take(30) {
                say!("    {e}");
            }
            if summary.errors.len() > 30 {
                say!("    ... {} more in the log", summary.errors.len() - 30);
            }
            let _ = std::fs::remove_file(&stamp_path);
            return Err(Stop::Broken(format!("{} file(s) failed to compile; log {}", summary.failed.max(1), log_path.display())));
        }
        let outs = build::compiled_outputs(&game_out);
        let _ = std::fs::write(&stamp_path, format!("{in_fp:016x} {:016x}
", build::tree_fingerprint(&game_out, &outs, "")));
    }
    // A live output naming a leftover whose source was deleted fails here, unless the game ships that path
    // (then the reference falls back to Valve's file and the leftover is just left out).
    let base = load_base(args)?;
    let mut package = build::package_with(&game_out, &sources, &|p| base.contains(p)).map_err(|e| format!("{addon}: {e}"))?;
    // Console configs ship byte for byte (no compiler involved); a BOM would reach `exec` as junk on line 1.
    for v in &sources.verbatim {
        let data = std::fs::read(content.join(build::native(v))).map_err(|e| format!("{v}: {e}"))?;
        if build::has_bom(&data) {
            return Err(Stop::Broken(format!("{v} starts with a UTF-8 byte-order mark; re-save it without one")));
        }
        package.files.push((v.to_ascii_lowercase(), data));
    }
    // `magician.exclude`: path prefixes (one per line, `#` comments) that ship in another pak. They are compiled and
    // linted like everything else but left out of this pak, so a path never lives in two paks (the lower pak
    // number wins in game, and a later release of this pak could mask or regress the other's copy).
    let excl = excluded_prefixes(&content);
    if !excl.is_empty() {
        let (gone, kept): (Vec<_>, Vec<_>) = package.files.into_iter().partition(|(p, _)| excl.iter().any(|x| p.starts_with(x.as_str())));
        package.files = kept;
        if !gone.is_empty() {
            package
                .left_out
                .entry("excluded by magician.exclude (ship in another pak)")
                .or_default()
                .extend(gone.into_iter().map(|(p, _)| p));
        }
    }
    Ok(Built { package, content })
}

/// The path prefixes in an addon's `magician.exclude`, lowercased with forward slashes.
fn excluded_prefixes(content: &Path) -> Vec<String> {
    std::fs::read_to_string(content.join("magician.exclude")).map(|t| exclude_lines(&t)).unwrap_or_default()
}

/// The path prefixes of a `magician.exclude` text: one per line, `#` comments, `\` read as `/`, lowercase.
fn exclude_lines(t: &str) -> Vec<String> {
    // a BOM (Windows PowerShell's Set-Content writes one) is not part of the first prefix
    t.trim_start_matches('\u{feff}').lines().map(|l| l.split('#').next().unwrap_or("").trim().replace('\\', "/").to_ascii_lowercase()).filter(|l| !l.is_empty()).collect()
}

fn print_left_out(left: &std::collections::BTreeMap<&'static str, Vec<String>>) {
    for (why, list) in left {
        let shown: Vec<&str> = list.iter().take(4).map(String::as_str).collect();
        say!("  left out {} {why}: {}{}", list.len(), shown.join(", "), if list.len() > 4 { ", ..." } else { "" });
    }
}

/// Compile a CSDK content addon and package the result.
/// Sources live in `<csdk>/content/citadel_addons/<addon>/`; resourcecompiler writes to
/// `<csdk>/game/citadel_addons/<addon>/`. Only real sources are compiled, and only outputs that map back
/// to a live source ship; build-only outputs and stale outputs are listed and left out (build.rs).
fn cmd_build(args: &[String]) -> R {
    let p = positional(args);
    let addon = p.first().ok_or("build <addon> <out/pakNN_dir.vpk>")?;
    let out = PathBuf::from(p.get(1).ok_or("build <addon> <out/pakNN_dir.vpk>")?);
    let compile = !args.iter().any(|a| a == "--no-compile");
    let timings = args.iter().any(|a| a == "--timings");
    let t = std::time::Instant::now();
    let b = compile_addon(addon, args, compile, flag(args, "--bin").as_deref())?;
    let t_compile = t.elapsed().as_secs_f64();
    let mut files = b.package.files;
    files.sort_by(|a, b| a.0.cmp(&b.0));
    say!("packaging {} resource(s) -> {}", files.len(), out.display());
    print_left_out(&b.package.left_out);
    if let Some(d) = out.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    let t = std::time::Instant::now();
    vpk::write(&out, &files).map_err(|e| e.to_string())?;
    let t_write = t.elapsed().as_secs_f64();
    let t = std::time::Instant::now();
    for l in markcmd::mark_report(&files, std::slice::from_ref(&b.content)) {
        say!("{l}");
    }
    if timings {
        say!("build timings: compile + package {t_compile:.2} s, write vpk {t_write:.2} s, maker's mark {:.2} s", t.elapsed().as_secs_f64());
    }
    let mut a = args.to_vec();
    if flag(args, "--allow").is_none() {
        a.push("--addon-root".into());
        a.push(b.content.to_string_lossy().into_owned());
    }
    lint_vpk(&out, &a)
}

/// One release VPK from every department: the addon's compiled files plus any `--with` addons (e.g. a
/// separate animation addon), linted with the addon's allow file and graded, with a report beside it.
fn cmd_release(args: &[String]) -> R {
    let p = positional(args);
    let usage = "release <addon> --hero <hero> [--with <addon>]... [--out pakNN_dir.vpk] [--scene-root DIR]... [--mod mod.toml] [--no-compile] [--json]";
    let addon = p.first().ok_or(usage)?.to_string();
    // the hero is checked before anything is compiled, so a misspelling costs nothing
    let hero = hero_key(&flag(args, "--hero").ok_or(usage)?, args)?;
    // The lint and the grade run as their own processes with this policy: a policy that does not read stops the
    // release here, before anything is compiled.
    load_policy(args)?;
    let compile = !args.iter().any(|a| a == "--no-compile");
    let out = flag(args, "--out")
        .map(PathBuf::from)
        .unwrap_or_else(|| reference_root().join("work").join("release").join(&addon).join("pak10_dir.vpk"));
    let mut all: std::collections::BTreeMap<String, (Vec<u8>, String)> = Default::default();
    let mut left: std::collections::BTreeMap<&'static str, Vec<String>> = Default::default();
    if flag(args, "--bin").is_some() {
        return Err(format!("release compiles several addons: give each its compiler with a {} file in the addon, not --bin", build::COMPILER_MARKER).into());
    }
    let timings = args.iter().any(|a| a == "--timings");
    let mut phases: Vec<(&str, f64)> = Vec::new();
    let mut lap = std::time::Instant::now();
    let main = compile_addon(&addon, args, compile, None)?;
    let content = main.content.clone();
    let mut addons = vec![(addon.clone(), main)];
    for w in flags(args, "--with") {
        let b = compile_addon(&w, args, compile, None)?;
        addons.push((w, b));
    }
    let roots: Vec<PathBuf> = addons.iter().map(|(_, b)| b.content.clone()).collect();
    for (name, b) in addons {
        for (path, data) in b.package.files {
            if let Some((_, from)) = all.get(&path) {
                return Err(format!("{path} is built by both {from} and {name}: one department must own it").into());
            }
            all.insert(path, (data, name.clone()));
        }
        for (why, l) in b.package.left_out {
            left.entry(why).or_default().extend(l);
        }
    }
    phases.push(("compile + package", lap.elapsed().as_secs_f64()));
    lap = std::time::Instant::now();
    let files: Vec<(String, Vec<u8>)> = all.iter().map(|(p, (d, _))| (p.clone(), d.clone())).collect();
    if let Some(d) = out.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    vpk::write(&out, &files).map_err(|e| e.to_string())?;
    phases.push(("write vpk", lap.elapsed().as_secs_f64()));

    let mut report = String::new();
    let mut line = |s: String| {
        say!("{s}");
        report.push_str(&s);
        report.push('\n');
    };
    line(format!("# Release: {addon} for {hero}"));
    line(format!("vpk {}  ({} files, {:.1} MB)", out.display(), files.len(), std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0) as f64 / 1e6));
    let mut dept: std::collections::BTreeMap<&str, (usize, u64)> = Default::default();
    for (p, d) in &files {
        let e = dept.entry(build::department(p)).or_default();
        e.0 += 1;
        e.1 += d.len() as u64;
    }
    line("department  files  MB".into());
    for (d, (n, b)) in &dept {
        line(format!("{d:<11} {n:>5}  {:.1}", *b as f64 / 1e6));
    }
    for (why, l) in &left {
        line(format!("left out {} {why}", l.len()));
    }
    let me = std::env::current_exe().map_err(|e| e.to_string())?;
    let allow = content.join("magician.allow");
    let mut common: Vec<String> = Vec::new();
    for k in ["--game", "--baseline", "--policy"] {
        if let Some(v) = flag(args, k) {
            common.push(k.into());
            common.push(v);
        }
    }
    /// A Magician sub-command: whether it ran clean (exit 0), its stdout, and a refusal when it could not run at all
    /// (exit 2 or 3, a crash), so a failed lint is told from a lint that never started.
    fn run_sub(me: &Path, sub: &[String], ok_codes: &[i32]) -> Result<(bool, String), Stop> {
        let o = std::process::Command::new(me).args(sub).output().map_err(|e| e.to_string())?;
        let code = o.status.code();
        if !code.is_some_and(|c| ok_codes.contains(&c)) {
            let err = String::from_utf8_lossy(&o.stderr);
            let why = err.lines().rev().find(|l| l.starts_with("magician:")).unwrap_or(err.trim());
            return Err(Stop::Usage(format!("release: `magician {}` did not run (exit {}): {why}", sub[0], code.map_or("none".to_string(), |c| c.to_string()))));
        }
        Ok((o.status.success(), String::from_utf8_lossy(&o.stdout).into_owned()))
    }
    // The grade judges this exact run (same depth, same findings) instead of linting again at its own depth.
    let findings_file = out.with_file_name("lint-findings.tsv");
    let lint_json = out.with_file_name("lint.json");
    let grade_json = out.with_file_name("grade.json");
    // A previous release's files would otherwise be read as this one's if a step dies before writing its own.
    for stale in [&findings_file, &lint_json, &grade_json] {
        if stale.exists() {
            std::fs::remove_file(stale).map_err(|e| format!("{}: {e}", stale.display()))?;
        }
    }
    let mut lint_args = vec![
        "lint".to_string(),
        out.to_string_lossy().into_owned(),
        "--deep".to_string(),
        "--timings".to_string(),
        "--findings-out".to_string(),
        findings_file.to_string_lossy().into_owned(),
        "--json-out".to_string(),
        lint_json.to_string_lossy().into_owned(),
    ];
    if allow.exists() {
        lint_args.extend(["--allow".to_string(), allow.to_string_lossy().into_owned()]);
    }
    // Every department's content tree, so lint can tell a live file from a compiled leftover (M032).
    for r in &roots {
        lint_args.extend(["--sources".to_string(), r.to_string_lossy().into_owned()]);
    }
    lint_args.extend(common.iter().cloned());
    // The lint (its own process, reading the VPK just written) runs while the maker's mark is measured here:
    // neither reads the other's result. Their output is still printed in the report's order.
    lap = std::time::Instant::now();
    let lint_job = std::thread::spawn({
        let (me, lint_args) = (me.clone(), lint_args.clone());
        move || run_sub(&me, &lint_args, &[0, 1])
    });
    line(String::new());
    line("## Maker's mark".into());
    for l in markcmd::mark_report(&files, &roots) {
        line(l);
    }
    phases.push(("maker's mark", lap.elapsed().as_secs_f64()));
    let (lint_ok, lint_out) = lint_job.join().map_err(|_| "the lint thread panicked".to_string())??;
    phases.push(("lint (with the mark)", lap.elapsed().as_secs_f64()));
    lap = std::time::Instant::now();
    line(String::new());
    line("## Lint".into());
    for l in lint_out.lines() {
        line(l.to_string());
    }
    let mut grade_args = vec!["grade".to_string(), out.to_string_lossy().into_owned(), "--hero".into(), hero.clone()];
    for r in flags(args, "--scene-root") {
        grade_args.extend(["--scene-root".to_string(), r]);
    }
    grade_args.extend(["--addon-root".to_string(), content.to_string_lossy().into_owned()]);
    grade_args.extend(["--findings".to_string(), findings_file.to_string_lossy().into_owned()]);
    grade_args.extend(["--record-out".to_string(), grade_json.to_string_lossy().into_owned()]);
    if let Some(m) = flag(args, "--mod") {
        grade_args.extend(["--mod".to_string(), m]);
    }
    grade_args.extend(common.iter().cloned());
    let (_, grade_out) = run_sub(&me, &grade_args, &[0])?;
    phases.push(("grade", lap.elapsed().as_secs_f64()));
    line(String::new());
    line("## Grade".into());
    for l in grade_out.lines() {
        line(l.to_string());
    }
    // A proof belongs to bytes: a new pak inherits none from the one it replaces. Only the same bytes again (the pack
    // is deterministic) keep theirs.
    line(String::new());
    line("## Proof".into());
    let proof_line = match flag(args, "--mod") {
        None => "no mod.toml (--mod): no proof ledger read; a pak is proven only by `magician prove` after its in-game pass".to_string(),
        Some(m) => {
            let toml = PathBuf::from(&m);
            let ledger = proofs::Ledger::for_toml(&toml)?;
            let claimed = std::fs::read_to_string(&toml).map(|t| grade::manifest::proven(&t)).unwrap_or_default();
            let sha = grade::sha256::file(&out).map_err(|e| format!("{}: {e}", out.display()))?;
            let b = proofs::Binding::of(&ledger, &claimed, std::slice::from_ref(&sha));
            let note = b.unbound_note();
            if b.bound.is_empty() {
                format!(
                    "these bytes (sha256 {sha}) are proven on no build: no proof is carried over from an earlier pak{}. Install it, run its in-game pass, then `magician prove --evidence <the pass folder>`",
                    if note.is_empty() { String::new() } else { format!(" ([proven] builds {note})") }
                )
            } else {
                format!("these bytes (sha256 {sha}) are the ones already proven on {}: the same pak again, so its proofs hold", b.bound_text())
            }
        }
    };
    line(proof_line);
    let rp = out.with_file_name("release-report.txt");
    std::fs::write(&rp, &report).map_err(|e| e.to_string())?;
    say!("report -> {}", rp.display());
    if json_mode() {
        // The lint run and the grade record as their own commands wrote them, inside one document.
        let raw = |p: &Path| std::fs::read_to_string(p).map(|t| t.trim().to_string()).ok().filter(|t| t.starts_with('{')).unwrap_or_else(|| "null".into());
        let doc = jsonout::Obj::new()
            .str("schema", "magician.release/1")
            .str("toolchain", &cli::version())
            .str("addon", &addon)
            .str("hero", &hero)
            .str("vpk", &out.display().to_string())
            .int("files", files.len() as i64)
            .raw("departments", jsonout::arr(dept.iter().map(|(d, (n, b))| jsonout::Obj::new().str("name", d).int("files", *n as i64).int("bytes", *b as i64).build())))
            .raw("left_out", jsonout::arr(left.iter().map(|(why, l)| jsonout::Obj::new().str("why", why).int("count", l.len() as i64).build())))
            .bool("lint_ok", lint_ok)
            .raw("lint", raw(&lint_json))
            .raw("grade", raw(&grade_json))
            .str("report", &rp.display().to_string())
            .build();
        outln!("{doc}");
    }
    if timings {
        say!("release timings: {}", phases.iter().map(|(n, t)| format!("{n} {t:.2} s")).collect::<Vec<_>>().join(", "));
    }
    Ok(if lint_ok { ExitCode::SUCCESS } else { ExitCode::from(1) })
}

/// The two baselines and rig sidecars a drift compares. A missing baseline is an error that says how to get it; a
/// missing rig sidecar only turns the bone and parameter comparison off, and says so.
struct Eras {
    from: String,
    to: String,
    then: Baseline,
    now: Baseline,
    rigs: Option<(patchday::Rig, patchday::Rig)>,
}

fn load_eras(from: &str, to: &str) -> Result<Eras, String> {
    let load = |b: &str| {
        let p = default_baseline_path(b);
        if !p.is_file() {
            return Err(format!("no baseline for build {b} at {}: capture it with the game on that build (`magician baseline`), or point --reference at the folder that holds it", p.display()));
        }
        Baseline::load(&p)
    };
    let (then, now) = (load(from)?, load(to)?);
    let rigs = match (patchday::Rig::load(&patchday::rig_path(&reference_root(), from)), patchday::Rig::load(&patchday::rig_path(&reference_root(), to))) {
        (Ok(a), Ok(b)) => Some((a, b)),
        _ => None,
    };
    Ok(Eras { from: from.to_string(), to: to.to_string(), then, now, rigs })
}

/// Drift of one pak against the eras, plus the model-contract check against the installed game (only possible when
/// `to` is the installed build: it needs Valve's current models).
fn drift_vpk(vp: &Path, eras: &Eras, gf: Option<&gamefiles::GameFiles>) -> Result<patchday::Drift, String> {
    let v = open(vp)?;
    let files = ModFile::from_vpk(&v).map_err(|e| e.to_string())?;
    let mut d = patchday::compute(&files, &eras.then, &eras.now, eras.rigs.as_ref().map(|(a, b)| (a, b)));
    if let Some(gf) = gf {
        d.contract_checked = true;
        for f in files.iter().filter(|f| f.path.ends_with(".vmdl_c") && eras.now.contains(&f.path)) {
            if let (Some(a), Some(b)) = (lint::deep::contract(&f.data), lint::Game::read(gf, &f.path).and_then(|d| lint::deep::contract(&d))) {
                for m in lint::deep::contract_breaches(&a, &b) {
                    d.contract.push(format!("{}: {m}", f.path));
                }
            }
        }
    }
    Ok(d)
}

fn print_drift(label: &str, nfiles: usize, eras: &Eras, d: &patchday::Drift) {
    say!("drift {label} : proven on {}, checking {} ({nfiles} files)", eras.from, eras.to);
    let show = |title: &str, l: &[String]| {
        say!("  {title}: {}", l.len());
        for x in l.iter().take(25) {
            say!("    {x}");
        }
        if l.len() > 25 {
            say!("    ... {} more", l.len() - 25);
        }
    };
    show("replaced files whose Valve original changed", &d.changed);
    show("replaced files Valve removed", &d.removed);
    show("references that no longer resolve", &patchday::group_broken(&d.broken));
    let deps_v: Vec<String> = d.deps.iter().cloned().collect();
    show("game files the mod uses that changed", &deps_v);
    if d.contract_checked {
        say!("  rig: {} skeleton/graph/model dependency change(s); {} contract breach(es) against the current models", d.rig_deps().len(), d.contract.len());
    } else {
        say!("  rig: {} skeleton/graph/model dependency change(s); model contract (bones, attachments, hitboxes) not checked: it needs Valve's models, so only when the checked build is the installed one", d.rig_deps().len());
    }
    for r in d.contract.iter().take(25) {
        say!("    {r}");
    }
    if eras.rigs.is_some() {
        show("skeleton bones removed or renamed", &d.skeleton);
        show("anim graph parameters removed or renamed", &d.params);
    } else if !d.rig_deps().is_empty() {
        say!("  note: no rig.tsv for {} and/or {} (`magician baseline` writes it), so a changed skeleton or graph can't be said to have lost a bone or parameter", eras.from, eras.to);
    }
    say!("  verdict: {}", d.verdict(&eras.from, &eras.to));
}

/// Compare a mod against the build it was proven on and the build installed now. Exit 1 when something is broken,
/// 4 when it loads but must be re-proven, 0 when nothing under it changed.
fn cmd_drift(args: &[String]) -> R {
    let p = positional(args);
    let usage = "drift <mod.vpk>... --from <build> [--to <build>] [--reference DIR]";
    if p.is_empty() {
        return Err(usage.into());
    }
    let from = flag(args, "--from").ok_or(usage)?;
    let game = game_dir(args)?;
    let installed = game_build(&game)?;
    let to = flag(args, "--to").unwrap_or(installed.clone());
    if to == installed && !default_baseline_path(&to).exists() {
        if let Ok(found) = paths::game(flag(args, "--game")) {
            auto_capture(&found, &installed_baseline(&found.path)?);
        }
    }
    let eras = load_eras(&from, &to)?;
    let gf = gamefiles::GameFiles::open(&game, None);
    let current_is_installed = to == installed;
    let mut states = Vec::new();
    let mut paks = Vec::new();
    for vp in &p {
        let d = drift_vpk(Path::new(vp.as_str()), &eras, current_is_installed.then_some(&gf))?;
        let n = open(Path::new(vp.as_str()))?.entries.len();
        print_drift(vp, n, &eras, &d);
        states.push(d.state());
        paks.push(jsonout::drift(vp, n, &d, &eras.from, &eras.to));
    }
    let state = if states.contains(&"broken") { "broken" } else if states.contains(&"re-prove") { "re-prove" } else { "clean" };
    if json_mode() {
        let doc = jsonout::Obj::new()
            .str("schema", "magician.drift/1")
            .str("from", &eras.from)
            .str("to", &eras.to)
            .str("installed", &installed)
            .bool("rig_compared", eras.rigs.is_some())
            .str("state", state)
            .raw("paks", jsonout::arr(paks))
            .build();
        outln!("{doc}");
    }
    Ok(state_code(state))
}

/// The exit code of a drift state: 1 broken, 4 re-prove, 0 clean.
fn state_code(state: &str) -> ExitCode {
    ExitCode::from(match state {
        "broken" => cli::BROKEN,
        "re-prove" => cli::REPROVE,
        _ => cli::CLEAN,
    })
}

/// Whether this run prints JSON (`--json`) instead of the human report.
fn json_mode() -> bool {
    !TEXT.load(Ordering::Relaxed)
}

fn install_paths(args: &[String]) -> Result<install::Paths, String> {
    Ok(install::Paths { citadel: game_dir(args)?.join("citadel") })
}

/// Refuse to swap installed paks under a running game that another pipeline (or a player) is using. The game that
/// matters is the one in the folder being changed: a running game elsewhere (the real one, while a test changes a
/// scratch copy) does not block it; a running game whose executable can't be placed does.
fn game_seat(args: &[String]) -> Result<(), Stop> {
    if args.iter().any(|a| a == "--ignore-game") {
        return Ok(());
    }
    let lock_path = paths::game_lock().path;
    let lock = std::fs::read_to_string(&lock_path).ok();
    let me = flag(args, "--holder").or_else(|| std::env::var("MAGICIAN_GAME_HOLDER").ok());
    match install::game_running() {
        Ok(true) => {
            let here = game_dir(args).map(|g| install::runs_from(&g, &install::running_game_exes())).unwrap_or(true);
            install::seat_check(here, lock.as_deref(), me.as_deref())
        }
        Ok(running) => install::seat_check(running, lock.as_deref(), me.as_deref()),
        Err(why) => install::seat_check_unknown(&why, lock.as_deref(), me.as_deref()),
    }
    .map_err(Stop::Refused)
}

fn cmd_install(args: &[String]) -> R {
    let ip = install_paths(args)?;
    let game = game_dir(args)?;
    let build = game_build(&game)?;
    let gi_text = std::fs::read_to_string(ip.gameinfo()).map_err(|e| format!("{}: {e}", ip.gameinfo().display()))?;
    if args.iter().any(|a| a == "--status") {
        let has = install::add_addons_search_path(&gi_text)?.is_none();
        say!("build {build}: addons search path {}", if has { "present" } else { "MISSING (Steam reset gameinfo.gi?)" });
        say!("backup: {}", if ip.backup(&build).exists() { ip.backup(&build).display().to_string() } else { "none for this build".into() });
        let mut ledger = install::read_installed(&ip.ledger());
        let moved = install::reconcile(&mut ledger, &ip.addons());
        for r in ledger {
            let present = ip.addons().join(&r.name).exists();
            let was = moved.iter().find(|(_, new)| *new == r.name).map(|(old, _)| format!(" (the ledger says {old}: renamed by another tool)")).unwrap_or_default();
            say!("  {}  crc {}  for {}  {}{was}", r.name, r.crc, r.owner, if present { "installed" } else { "missing" });
        }
        return Ok(ExitCode::SUCCESS);
    }
    let p = positional(args);
    let src = PathBuf::from(p.first().ok_or("install <pakNN_dir.vpk | maps/ui/hero_prefabs/<hero>.vpk> [--as pakNN_dir.vpk] [--mod NAME] [--force]")?);
    let force = args.iter().any(|a| a == "--force");
    game_seat(args)?;
    let route = install::route_for(&src, flag(args, "--as").as_deref())?;
    let owner = install::owner_for(&src, flag(args, "--mod").as_deref());

    // 1. Lint first: nothing with errors goes into the game. A UI scene map is a map archive, not a pak of
    // resources: it is checked for being the hero's own scene instead.
    match &route {
        install::Route::Pak(slot) => {
            let code = lint_vpk(&src, args)?;
            if code != ExitCode::SUCCESS && !force {
                return Err(Stop::Refused("lint errors: not installing (use --force to override)".into()));
            }
            let from = src.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase());
            if from.as_deref() != Some(slot.as_str()) {
                say!("installing {} under the slot {slot} (--as); lower pak numbers win collisions", src.display());
            }
        }
        install::Route::UiScene(file) => {
            install::validate_scene(&src, file)?;
            say!("UI scene map {file}: holds its own scene and nothing else (the pak lint rules don't apply to a map)");
        }
    }

    // 2 and 3. Slot check, gameinfo.gi (backed up once per build), copy, ledger.
    let done = install::install_vpk(&ip, &build, &src, &route, &owner, force)?;
    for n in &done.notes {
        say!("{n}");
    }
    say!("installed {} for {owner} (crc {}); lower pak numbers win collisions", done.dst.display(), done.crc);
    Ok(ExitCode::SUCCESS)
}

fn cmd_uninstall(args: &[String]) -> R {
    let ip = install_paths(args)?;
    let p = positional(args);
    let all = args.iter().any(|a| a == "--all");
    let owner = flag(args, "--mod");
    if all || owner.is_some() {
        // Rollback: every install in the ledger (--all), or one mod's (--mod NAME). Anything not in the ledger stays.
        game_seat(args)?;
        let removed = install::uninstall_matching(&ip, owner.as_deref(), args.iter().any(|a| a == "--force"))?;
        for r in &removed {
            say!("removed {}", r.display());
        }
        say!("{} file(s) removed; gameinfo.gi left as it is (an addons mount with no paks costs nothing; docs/PATCH-DAY.md has the optional restore)", removed.len());
        return Ok(ExitCode::SUCCESS);
    }
    let name = p.first().ok_or("uninstall <pakNN_dir.vpk | maps/ui/hero_prefabs/<hero>.vpk> | uninstall --all | uninstall --mod NAME")?;
    game_seat(args)?;
    let dst = install::uninstall(&ip, name, args.iter().any(|a| a == "--force"))?;
    say!("removed {}", dst.display());
    Ok(ExitCode::SUCCESS)
}

fn cmd_rules(_args: &[String]) -> R {
    if json_mode() {
        out!("{}", jsonout::rules());
        return Ok(ExitCode::SUCCESS);
    }
    for r in RULES {
        say!("{} {:<26} {:?}\n     {}", r.code, r.name, r.severity, r.in_game);
    }
    say!("\nThe severities are each rule's own. Fair play is lenient unless the policy enforces it ([fair_play] enforce = true): until then the fair-play rules ({}; M062 for hitboxes only) and M144 report as warnings.", grade::rubric::FAIR_PLAY_RULES.join(" "));
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod fuzz_tests;

/// Fuzz test binaries refuse any allocation a length in the input asked for (`crates/fuzz`).
#[cfg(test)]
#[global_allocator]
static CAP: fuzz::Capped = fuzz::Capped;

#[cfg(test)]
mod exclude_tests {
    use super::*;

    #[test]
    fn switches_take_no_value_so_positionals_survive_them() {
        let a: Vec<String> = ["--deep", "x.vpk", "--hero", "lash", "y.vpk", "--light"].iter().map(|s| s.to_string()).collect();
        assert_eq!(positional(&a), vec!["x.vpk", "y.vpk"]);
        // every switch is an option the usage text documents
        let known = cli::known_options(USAGE);
        for s in SWITCHES {
            assert!(known.contains(*s), "{s} is not in the usage text");
        }
        // every command a typo can be matched to is one the usage documents
        for c in COMMANDS.iter().filter(|c| **c != "help") {
            assert!(USAGE.contains(&format!("\n  magician {c} ")) || USAGE.contains(&format!("\n  magician {c}\n")), "{c} is not in the usage text");
        }
    }

    #[test]
    fn a_grade_reads_the_mods_own_allowance_file_and_says_when_there_is_none() {
        let d = std::env::temp_dir().join(format!("magician-gradeallow-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("addon")).unwrap();
        let toml = d.join("mod.toml");
        std::fs::write(&toml, "[mod]
slug = \"x\"
").unwrap();
        std::fs::write(d.join("addon").join("magician.allow"), "M140	models/x.vmdl_c	*	the skirt is the silhouette
").unwrap();
        let csdk = d.join("no-csdk").to_string_lossy().into_owned();
        let args: Vec<String> = vec!["--csdk".into(), csdk.clone()];
        let (a, names) = grade_allowances(&args, Some((&toml, "[mod]
slug = \"x\"
"))).unwrap();
        assert_eq!(a.len(), 1);
        assert!(names.len() == 1 && names[0].starts_with("allowances magician.allow (sha256 ") && names[0].ends_with(", beside the mod.toml, in addon)"), "{names:?}");
        // an explicit --addon-root wins over discovery, even when it holds no file
        let empty = d.join("empty");
        std::fs::create_dir_all(&empty).unwrap();
        let with_root: Vec<String> = vec!["--csdk".into(), csdk.clone(), "--addon-root".into(), empty.to_string_lossy().into_owned()];
        let (a, names) = grade_allowances(&with_root, Some((&toml, ""))).unwrap();
        assert!(a.is_empty() && names == vec!["allowances: none found beside the mod.toml or in its CSDK addon".to_string()], "{names:?}");
        std::fs::remove_dir_all(d.join("addon")).unwrap();
        let (a, names) = grade_allowances(&args, Some((&toml, ""))).unwrap();
        assert!(a.is_empty() && names[0].starts_with("allowances: none found"), "{names:?}");
        let (_, names) = grade_allowances(&[], None).unwrap();
        assert_eq!(names, vec!["allowances: none given".to_string()]);
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn exclude_file_lists_prefixes() {
        let d = std::env::temp_dir().join(format!("magician-excl-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("magician.exclude"), "# sound ships in pak06\nSounds\\VO/  # voices\n\nsoundevents/\n").unwrap();
        assert_eq!(excluded_prefixes(&d), vec!["sounds/vo/".to_string(), "soundevents/".to_string()]);
        std::fs::remove_dir_all(&d).unwrap();
        assert!(excluded_prefixes(&d).is_empty());
        // written by Set-Content -Encoding utf8: a BOM in front of the first prefix
        assert_eq!(exclude_lines("\u{feff}sounds/vo/\nsoundevents/\n"), vec!["sounds/vo/".to_string(), "soundevents/".to_string()]);
    }
}
