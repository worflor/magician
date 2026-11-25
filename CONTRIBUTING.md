# Contributing to Magician

Magician is a headless toolchain for creating, validating, and maintaining Deadlock mods through automation, scripts, and AI agents. It is a Rust workspace with zero external crates, plus standard-library Python around the game, and it runs on Windows and Linux alike. [README.md](README.md) says what it is and how to start using it; this file is for changing it: what you need, how the repository is laid out, the rules every change follows, the gotchas the code already handles, how to verify your work, and how to send it.

## What you need

On either system:

- A stable Rust toolchain with clippy (`rustup component add clippy`). The workspace has no external crates, so a build downloads nothing.
- Python 3.11 or newer on PATH, standard library only. The bundler's and the mod-source writer's tests make preview images with Pillow (`python -m pip install pillow`, or `python3-pil` on Debian and Ubuntu); `cinema/test_scene.py` needs ffmpeg.
- git (the commit `--version` names), and `curl` and `tar` for `crates/magician/tests/update.rs`.
- On Linux, to test the harness's X11 side: `xvfb`, `libx11-6` and `libxtst6` (the Debian and Ubuntu names).

Nothing needs the game, Valve's CSDK, Source 2 Viewer or Blender: every test makes its own fixtures. Commands below are written once for both systems: paths with `/`, `python` for the Python 3 on PATH (`python3` on most Linux distributions), and an environment variable the way bash sets it, `NAME=value command` (in PowerShell: `$env:NAME = "value"; command`).

## Where knowledge lives

Each kind of knowledge has one home. Read it there, and put new knowledge there.

- [docs/PDD.md](docs/PDD.md): what Magician is for (automation first), what it promises its caller, its taste, its non-goals and its name.
- [docs/TDD.md](docs/TDD.md): the technical model (builds as targets, mods as programs), crates, locations, the files it owns, the pipeline, grading.
- [docs/reference/engine.md](docs/reference/engine.md): facts about Deadlock, the CSDK compiler and Source 2 Viewer that the code relies on, and the scars behind them. One fact in one place.
- [docs/STATUS.md](docs/STATUS.md): current state, loose ends and known gaps. [CHANGELOG.md](CHANGELOG.md): what shipped, newest first.
- [docs/PATCH-DAY.md](docs/PATCH-DAY.md): the patch-day runbook and what each failure means.
- [ingame/README.md](ingame/README.md), [mcp/README.md](mcp/README.md), [cinema/README.md](cinema/README.md): their subsystems. The in-game harness's README is also the home of game facts learned by driving it.
- [skills/using-magician/](skills/using-magician/SKILL.md): how an agent drives Magician on a mod. Keep it in step when a command, exit code or safety rule changes.
- [skills/moving-to-magician/](skills/moving-to-magician/SKILL.md): how an agent brings a mod made without Magician under it. It quotes `init`, `check` and `install` messages word for word; keep them in step.
- Plans in progress: an open plan or investigation is one file under `docs/briefs/` while it is open (there is none today). A brief is a draft, not canonical truth; what proves durable moves into a core doc or a test, and the brief is deleted.
- Executable references: `magician --help` (every command and flag, the exit codes, how locations resolve; `magician <command> --help` for one), `magician rules` (the rule catalogue), `magician rubric` (the grading criteria in force), `magician policy` (every judgement number in force and where it came from), `magician paths` (where this machine's game, reference folder and tools are, and the mod of the working folder), `magician doctor` (what is missing on this machine, with each fix).

## Posture

- **Headless first.** The caller is a script or an agent. No prompts, no output that only works in colour, exit codes that mean something (0 clean, 1 broken, 2 usage error, 3 refused on purpose, 4 re-prove, 5 an update is available; `cli.rs`), stable labels a script can match, one JSON document with a `schema` string where a machine is the reader. A new command or flag earns a line in `magician`'s usage text, because a command accepts only the options declared for it and refuses every other before anything runs: a flag only one command takes goes in that command's own paragraphs of the usage text (`USAGE_HEAD` in main.rs); one several commands take goes in `SHARED` (main.rs) with the commands that take it, never in two places; a command that runs another's work with its own arguments, as `install` lints, is named in `INHERITS`. A flag that takes no value also goes in `SWITCHES`. `magician help <command>` is written from the same declaration, and `option_tests.rs` fails when the code reads a flag no command declares, a command's help and what it accepts differ, or a call in the toolkit's tools or a command line in the docs passes a flag its command does not take (the toolkit's calls are listed there: add a new one).
- **A compiler, not a loader.** Never grow a runtime, a mod manager or a launcher. Output must install in other people's managers.
- **The game build is the era.** Baselines, drift and "proven on N of M builds" are the spine; don't hardcode facts about one build where a baseline read will do.
- **Compile-clean is not proof.** Exit code 0 is one input. Visual proof from `ingame/` and `qa/` decides.
- **Fair play is a lint category.** Cosmetic only. A rule that would let a mod make a hero harder to read is a defect in the linter.
- **Say what was found.** A value a command fills in for its caller (from the mod's folder, the config, Steam) is named on stderr with where it came from, and shown by `magician paths` or `magician doctor`; an explicit argument always wins. A new command that can be completed from a mod's folder does it in `modctx::fill`, never inside the command, and a new command goes in `COMMANDS` (main.rs) so a typo of it is answered.
- **Write through `out.rs`.** `outln!`, `out!`, `say!` (the human report, silent under `--json`) and `errln!`, never `println!` or `eprintln!`: a closed pipe must end in the command's own exit code, not a panic. Colour (`style.rs`) and progress lines (`progress.rs`) only ever on a terminal, and the words must read without them.
- **The network only on request.** `magician update ...` and `doctor --updates` are the only code that touches it, each step bounded to about 10 s and a failure said as "could not check". A new command that goes online needs the same explicit flag, never a check on the way past. Updating never fetches and runs new code unasked (`update.rs`, `updateapply.rs`).
- **No machine in the code.** Outside locations go through `crates/magician/src/paths.rs` and `ingame/gamepaths.py` (flag, environment, config file, discovery). Never write a drive letter or a user folder into a default.
- **Both systems, one implementation.** Windows and Linux run the same code. Where the system itself is asked something there is one seam for it: `paths.rs` for where things live (each platform's own folders), `install.rs` for whether the game runs and from where, `lock.rs` for whether a process is alive, `ingame/gameos.py` for everything the harness asks of the system (processes, the window, keys, mouse, capture, Steam). A new need from the system goes into its seam with a backend for each platform, or a refusal that says what is missing; never a second script for the other system, and never a quiet answer of "not running" or "free" because the question could not be asked. What runs on one system only says so once, in [README.md](README.md#platforms).

## Map

```
crates/vpk        VPK read/write, deterministic pack
crates/resource   block table, RERL, native KV3 v4/v5 read and write (LZ4, zstd), texture headers and pixels
crates/mesh       compiled model meshes (meshoptimizer v0/v1), skeleton, hitboxes, glb proof, synth.rs (every mesh fixture),
                  cloth.rs (the FeModel in PHYS: the one decoder for `magician cloth` and the cloth lint)
crates/geom       geometry kernel (exact predicates, BVH, raster, topology)
crates/lint       rules (deep.rs reads inside files; mesh/ is the M1xx family; mesh/rules.rs holds every judgement constant),
                  policy.rs (the fair-play limits a run is handed), allow.rs, baseline.rs, cache.rs
crates/grade      hero footprint, the rubric (rubric.rs is the only home for every grading number), identity, compliance,
                  the record and label, the policy reader, the mod.toml reader, SHA-256 and the JSON writer
crates/expr       DynamicParams parser and interval proof
crates/mark       maker's mark: keyed invisible authorship watermark in a mod's colour textures (sign and detect)
crates/fuzz       the fuzz harness, a dev-dependency only: seeded cases, byte and text mutators, a hang watchdog and an
                  allocation cap; each crate's tests/fuzz.rs holds its targets and tests/fuzz-cases/ the inputs they found
crates/magician   the CLI: main.rs, cli.rs (--help, --version, options, exit codes, the nearest real name for a typo),
                  paths.rs (where the game, reference folder, tools and policy are), modctx.rs (the mod of the working
                  folder, and filling a bare verb in from it), heroes.rs (the game's hero table), checkcmd.rs, doctor.rs,
                  initcmd.rs, newcmd.rs (a skin's folder from nothing), frontdoor.rs (session, reprove, bundle and
                  source: the Python tools through the CLI), update.rs (`update --check`, the release's `VERSION`, the update source and the curl fetch) and updateapply.rs (applying one: the checksum, the swap and its rollback, the git fast-forward and rebuild), out.rs (every write to stdout and stderr), style.rs,
                  progress.rs, build.rs, lock.rs (per-addon build lock), install.rs (slots, ledger, scene route, seat
                  check), scene.rs (scene patches), patchday.rs and patchcmd.rs (eras, the rig and engine sidecars,
                  drift verdicts, prove), proofs.rs (the proof ledger: which bytes were proven on which build, and the
                  one "is it proven here" question), gamefiles.rs, meshcmd.rs, clothcmd.rs, markcmd.rs, jsonout.rs
mcp/              MCP server: the CLI as JSON tools for agents, Python stdlib only; no install or launch tool
skills/           the Agent Skills: using-magician (driving the CLI on a mod), moving-to-magician (bringing a mod under it)
ingame/           retail-game driver, Python stdlib: mod_session.py is the one driver of a mod's session and reprove.py
                  patch day in game (`magician session`, `magician reprove`), takes.py the timed and staged takes,
                  logdiff.py the warnings a mod adds over vanilla, gamelock.py the one-user-at-a-time game lock (the
                  game bound to the seat), gamepaths.py the harness side of paths.rs, gameos.py everything the harness
                  asks of the system (a Windows and an X11 backend), launch.py and stop_game.py the game's start and
                  stop (launch.ps1 and stop_game.ps1 call them, for old callers); scenarios/ holds stock-hero examples
qa/ harness/      offline visual QA and Blender render scripts; qa/heroes/haze.json is the example hero profile (a mod
                  keeps its own and passes its path)
cinema/           the optional, dev-only film kit for trailers and stills (pak90, never installed during QA or play)
tools/            bundle.py (player release bundles with mod-interchange.json) and source.py (a static mod source from
                  bundles), behind `magician bundle` and `magician source`; make-release.ps1 (the release files: the asset for this platform, VERSION, SHA256SUMS; Windows PowerShell 5.1 or PowerShell 7), scene_install.ps1 / .py (the player-side
                  scene installers bundles ship), scene_lump.py (re-dress a UI scene map), rip_heroes.py (hero glbs
                  for reference, never committed) and s2v_mcp.py (the Source 2 Viewer MCP launcher), each with a .ps1 caller; test_bundle.py,
                  test_source.py and test_scene_install.py hold their guards
docs/             PDD.md, TDD.md, STATUS.md, PATCH-DAY.md, reference/
```

## Commands

```
cargo build --release            # target/release/magician (magician.exe on Windows)
cargo test --workspace           # synthetic fixtures only; needs no game and no Valve file
magician --help                  # every command and flag, the exit codes, how locations resolve
magician paths                   # where the game, reference folder, CSDK and viewer resolved to on this machine
magician rules                   # what each rule costs in game
magician patchday                # the patch-day check (read-only without --capture)
python mcp/test_server.py        # the MCP server end to end (packs its own pak; lints it when the game is found)
python ingame/test_gamelock.py   # the game lock, the game bound to the seat
python ingame/test_gameos.py     # the harness's seam to the system: processes everywhere, and on Linux an X11 window of
                                 # its own (under Xvfb: xvfb-run -a python ingame/test_gameos.py)
python ingame/test_takes.py      # the timed-take plans and the frozen-hero check, offline
python ingame/test_mod_session.py    # the session driver; its install and restore against a scratch game (release binary)
python ingame/test_logdiff.py    # baselines by build and scene, and what is and is not compared
python ingame/test_reprove.py    # patch day in game: the plan, the states and accept, on a fake session
python cinema/test_replay.py     # the free-camera film's shot list and demo-end check, offline
python cinema/test_capture.py    # the film kit's grabs and recording on X11, against its own window (Xvfb and ffmpeg)
python -I tools/test_bundle.py   # the bundler's guards (needs the release binary, not the game)
python -I tools/test_source.py   # the mod-source writer, offline
python -I tools/test_scene_install.py  # both player installers and magician on good and malformed patches, offline
python cinema/test_scene.py      # the film kit's language and encode, offline (needs ffmpeg)
python cinema/test_session.py    # the film kit's seat hand-back, on a fake clock
```

The Python tests and tools that run the program find the release build where cargo put it (`CARGO_TARGET_DIR`, else `target/`); `MAGICIAN_BIN=<a program>` names another. The program finds its toolkit above itself, else in the checkout it was built from.

Before a change lands, run what CI would:

```
RUSTFLAGS="-D warnings" cargo test --workspace              # warnings are errors
cargo clippy --workspace --all-targets -- -D warnings       # the lint set below, as errors
```

The lint set lives in the workspace `Cargo.toml` (`[workspace.lints]`, which every crate takes): unsafe code is denied
and each library crate forbids it (the two exceptions, the build lock's process check and the fuzz allocator, say why
where they are); clippy's correctness and suspicious groups are errors; the readers of other people's bytes (`vpk`,
`resource`, `mesh`) also deny `unwrap`, `expect` and slicing a `str` at a byte offset. Every profile checks integer
overflow. A lint is fixed, not allowed, unless the allow says why on its line.

Every reader of bytes or text someone else made has a fuzz target in its crate's `tests/fuzz.rs` (`crates/fuzz` is the
harness). `cargo test` runs a short, fixed set of cases; a long run is a time per target:

```
MAGICIAN_FUZZ=600 cargo test --release --test fuzz -- --test-threads 1     # every target, 600 s each
MAGICIAN_FUZZ_GAME=<.../Deadlock/game>                                     # with the line above: also mutate real blocks, read only
MAGICIAN_FUZZ_SEED=7 MAGICIAN_FUZZ_CASE=1234                               # with the line above: one case again
MAGICIAN_FUZZ_BACKTRACE=1                                                  # where an allocation over the cap came from
```

A case that panics, hangs (10 s) or asks for one allocation over 256 MiB stops the run, names its seed and number, and
saves its input; fix the reader, then copy the input into that target's `tests/fuzz-cases/<target>/` under a name
that says what it found, so every later run replays it first. Inputs there come from synthetic seeds, never from a
game file: a case found with `MAGICIAN_FUZZ_GAME` is a mutation of Valve's bytes, so it is not kept; write a test that
builds the same shape from nothing instead (`typed_null_arrays` in `crates/resource/tests/fuzz.rs` is one).

`--help` after any command prints its usage and runs nothing (`crates/magician/tests/update.rs` holds `magician update` (scratch toolkits and checkouts, a local folder or a server on this machine as the source; it needs `git`, `curl`, `tar` and `rustc`), and `crates/magician/tests/contract.rs` holds that and the exit codes to the built binary). `crates/magician/tests/real_mods.rs` grades real mods when the machine has them (Maid Lash, from a `model-replacements/maid-lash/` folder in any folder above the repository, and a new-character mod with a voice pak, named by `MAGICIAN_REAL_NEW_CHARACTER`) and skips otherwise; without them nothing is lost but those two grades. CI (`.github/workflows/ci.yml`) builds a fresh clone on clean Windows and Linux runners and runs the workspace tests, clippy and the Python tests on both, the harness's X11 side under Xvfb.

The `dev` profile runs at opt-level 1 because VPK trees are 150k+ entries and unoptimised parsing is needlessly slow.

## Cutting a release

A release is its asset per platform (`magician-windows-x64.zip`, `magician-linux-x64.tar.gz`), `SHA256SUMS` and `VERSION`, which `magician update` reads ([README.md](README.md), "Staying current"). Nothing is generated by hand.

1. Make the commit to release on `main`, with the change's entry in [CHANGELOG.md](CHANGELOG.md) under a heading named for the version. Say in that entry whether the lint rules, the grading rubric or the built-in policy changed: that is what `update --check` tells a user, and `magician update --describe` (the `rubric`, `policy`, `lint_rules` and `lint_fingerprint` lines) shows what the build says. Bump `version` in the workspace `Cargo.toml` when the release is a new version; two commits of one version are told apart by their dates.
2. Prove the files locally, into a scratch folder: `tools/make-release.ps1 -Out <scratch>` (on Linux, `pwsh tools/make-release.ps1 -Out <scratch>`; each system builds its own asset). It builds the release executable (into `CARGO_TARGET_DIR` when set), writes `VERSION` from the built program, packs the tracked toolkit files, and writes `SHA256SUMS`. It refuses a tree with local changes and a program that was not built from HEAD, and uploads nothing. Then try it as a user would, on a scratch copy of a toolkit folder: `magician update --check --source <scratch>`, and an `update` into the copy.
3. Tag the commit `vX.Y.Z` and push the tag. `.github/workflows/release.yml` runs the same script on Windows and Linux runners (`-Tag` refuses a tag that is not `v` and the program's version), gathers the assets, writes one `SHA256SUMS` over them and attaches the files to the release. Nobody uploads by hand. The workflow has not run on GitHub yet (nor has `ci.yml`); the script has made both assets locally, and each has updated a scratch install from a folder: look at the first release's files before telling anyone.
4. `releases/latest/download` is the default update source, so the repository's latest published release is what every `update --check` sees. A draft or a pre-release is not "latest".

## Rules for changes

1. **Zero external crates.** Parsers, codecs and decoders are written here and proven against Source 2 Viewer's output. Don't add a dependency to shorten a task. On the Python side the MCP server, the harness, the bundler, the mod-source writer and the player installers are standard library (the bundler and the mod-source writer use Pillow, for images only); the offline QA and render scripts use Pillow, numpy, scipy and Blender's own Python. Add nothing new to either.
2. **Every lint rule ships with a fixture that breaks it** and a test that the rule fires on that fixture for that reason. `every_rule_has_a_fixture` (crates/lint/src/tests.rs) fails the build if a rule has none. Mesh fixtures are built with `crates/mesh/src/synth.rs`, so every triangle is chosen.
3. **Never weaken a rule to make a mod pass.** Per-site allowances go in the addon's `magician.allow` with a mandatory reason; allowed errors stay visible, stale allowances are reported. Grading follows: an allowed warning that is not fair play is not a defect; an allowed error still makes the mod `broken`. Fair-play findings are judged apart and never make it broken or change its description: an allowed M140 (hitbox overflow past 10 units that earns its place as a silhouette piece) holds within its limit and is listed with its measure; M129 (the egregious tier: over 18 units, or over 3.5% of the visible surface beyond 10) is never allowable and makes the grade NSFR. The thresholds live in one constants block, `crates/lint/src/mesh/rules.rs`, and the grading numbers in `crates/grade/src/rubric.rs`; a policy file moves them at run time, never the code. A new judgement number gets a constant there and a key in the policy.
4. **Never write into the Deadlock install except through `install.rs`.** It backs up `gameinfo.gi` per build, is idempotent and keeps a ledger so it never touches files it didn't install.
5. **Judge against Valve's own model at the same path.** Things Valve's file already does are Valve's design, not a finding. The test of it: Valve's own file, packed unchanged and linted deep, is silent, for every hero (a rule that speaks there is wrong, however good its physics: the limit of a new rule is measured on Valve's heroes before it is written down). Every bounded search says when it stops (M100), and every fair-play comparison that could not run says so (M144): unchecked is never clean.
6. **Say what a rule can't see.** A claim about in-game behaviour that nobody has seen in the running game is marked UNVERIFIED where it is made.
7. **Keep the player side standalone.** `tools/scene_install.ps1` and `scene_install.py` ship inside release bundles and run on players' machines: Windows PowerShell 5.1 or Python 3 stdlib, no Magician, no config file, byte-identical output to `magician scene install`.

## Content rules

The repository is public, so it holds only what may be shared:

- Nothing extracted from Valve's files is committed: no VPKs, compiled resources (`*_c`), decompiled sources, textures, glb exports, renders or captures. `.gitignore` enforces the extensions; tests and fixtures are synthetic.
- Paths, CRCs, rule thresholds and numeric findings are facts about the game and may appear in docs and tests; bytes of its content may not.
- Fixtures that need real-looking bytes get them from `synth.rs` or inline constants (for example the KV3 format GUID), never from a game file.
- No personal paths, names or machine details in code or docs. Screenshots and harness failure shots (`harness_fail_*.png`) never go in; look before you `git add` a folder the harness wrote into.
- The repository carries no mod of its own. Examples are neutral, on a stock hero (Haze); a mod's scenarios, QA profile, scenes and scene patches live with the mod.
- Commits are plain imperative subjects with a short body.

## Toolkit gotchas

What the engine, the compiler and the viewer do is in [docs/reference/engine.md](docs/reference/engine.md), with how Magician handles each. What follows is about this code: a trap it has already fallen into, and the place that holds the line.

| Area | Issue | Where handled |
|---|---|---|
| Compiler | A `DistanceFieldList` only allocates with the `distancefield` ModelDoc feature, which the CSDK's gameinfo lacks. | `build.rs`: `distance_field_game` writes `<csdk>/game/citadel_dstf/gameinfo.gi` (the CSDK's plus that feature, the original untouched) and those documents compile with `-game` pointing at it. |
| Pack | A path shipped in two paks is won by the lower pak number, so a later release of the other pak can mask or regress it. | An addon's `magician.exclude` (path prefixes, `#` comments) leaves those compiled files out of `build` and `release` paks (listed as `left out N excluded by magician.exclude`). `tools/bundle.py` refuses a bundle where a path is in two paks or a file is byte-identical to Valve's. |
| Build | Two builds of one addon at once (a release overlapping another pipeline's compile) corrupt each other's run in `game/citadel_addons/<addon>/`. | `lock.rs`: a `.magician-build.lock` (pid, start time, holder) in that folder; a second taker waits (`--lock-timeout`) or fails at once (`--no-wait`); a dead or reused pid is stale. |
| Game | Several pipelines drive one retail game. A second `map` or `selecthero` mid-scenario wrecks the first one's shots, and a seat freed while `deadlock.exe` is still exiting lets the next holder launch beside a game that the first pipeline's fallback then kills. | `ingame/gamelock.py`: a lock file naming the holder (`MAGICIAN_GAME_HOLDER`); `launch.py` binds the pid and start time of the game it started to the seat; `console.py` and `stop_game.py` drive only that game (`may_drive`); a release is refused while it lives, and `stop_game.py` holds the seat until the process has exited. `test_gamelock.py` holds each. The rules are in [ingame/README.md](ingame/README.md), "One user of the game at a time". |
| Locations | The CLI's seat check and the harness's game lock must read the same file, or install would swap paks under a running capture. | One resolution order in `paths.rs` and `gamepaths.py` (`MAGICIAN_GAME_LOCK`, config `game_lock`, else `<reference>/work/ingame-game.lock`). Change both or neither. |
| Platform | Off Windows the game lock listed processes with `tasklist`, found none, and called every lock stale and every pid dead: a pipeline would have cleared another's seat under a running game. A build lock took a reused pid for its holder, and the findings cache was off for want of `%LOCALAPPDATA%`. | `ingame/gameos.py` and `lock.rs` read `/proc` (start times too, so a reused pid is not the holder); `paths.rs` has each platform's folders; a question the system can't answer is refused, never answered "no". `test_gameos.py` and `lock.rs`'s tests hold them. |
| Line ends | A checkout's line ends follow the system and its git settings, so a bundle made on Linux differed from one made on Windows in its two scene installers. | `bundle.py` writes `install-scene.ps1` with CRLF and `install-scene.py` with LF, whatever the checkout has; `test_bundle.py` holds it. |
| Install | Two installs or uninstalls at once each rewrite the ledger from what they read, and one row is lost. | `install.rs`: the ledger is read, changed and written under `.magician-installed.lock` in `addons`; the pak, `gameinfo.gi` and the ledger are written to a temporary name and renamed into place. |
| Install | Deadlock Mod Manager renumbers every pak in `addons`, so a ledger row can name a file that moved. | `install::reconcile`: a row whose file is missing or changed follows its crc to the one unclaimed pak with those bytes; uninstall never removes bytes that are not the ones it installed. |
| Locks | A lock file created empty and written after can be read, by a taker that has waited, as a stale lock to clear: two holders, or a lock put back after its holder left. | `lock.rs`: a lock is written under its own name and hard-linked into place, whole or not at all, and carries a token so threads of one process tell their locks apart. |
| Install | Two mods default to `pak10_dir.vpk`; copying the second over the first silently uninstalls it. | `install.rs`: the ledger records the owner; another mod's file, a file we didn't install and one changed since are never replaced without `--force`; `--as pakNN_dir.vpk` picks another slot. |
| Install | Installing or uninstalling while another pipeline drives the game swaps the pak under its capture (or under someone's match). | `install::seat_check`: refused while deadlock.exe runs unless the caller holds the game lock (`--holder` / `MAGICIAN_GAME_HOLDER`); `--ignore-game` overrides. |
| KV3 | A binary KV3 write that keeps blobs inline decodes in Magician (it accepts all layouts) but not in Source 2 Viewer. | `kv3::encode` writes blobs in blocks with the closing 0xFFEEDD00; `blobs_live_in_blocks_in_the_layout_the_game_uses` pins the bytes. A round trip through our own decoder proves nothing. |
| Cloth | `PHYS` holds the FeModel: integrator per node, collision radii per non-static node (`node - m_nStaticNodes`), shapes with layer masks. Two decoders would drift. | `mesh::cloth` is the one read; `lint::cloth` (M141) and `magician cloth` both use it. |
| Geometry | A triangle-triangle test must share one intersection-line direction for both intervals, or it misses every fold. | `geom::intersect` (found by the M109 fixture) |
| Lint | A drill with a fake `--baseline` wrote `particle_classes.tsv` into the real reference folder under the fake build number. | Every command takes `--reference`; point drills at a scratch folder. |
| Speed | A deep mesh lint takes about 8 s per hero model cold (the census runs on all cores) and about 1 s from the findings cache. Run unbounded, the full census took over 15 minutes on one release. | Keep new mesh rules bounded and say when they stop; `mesh_family_cost_is_bounded` holds deep under 20 s and light under 2 s on a 10,800-triangle fixture. |
| Output | `println!` panics when the reader has gone (`magician ls x \| head -1` exited 101). | `out.rs`: a closed stream is dropped for the rest of the run; `a_reader_that_stops_reading_does_not_crash_the_command` holds it. |
| Viewer | Source 2 Viewer prints `--- Wrote 1 files` on stdout and drops `exceptions.txt` in its working folder, which corrupts `grade --json` and litters the user's folder; two grades decompiling one file at once clash. | `footprint::Sources::text` runs it in a private folder under the cache with stdout closed and renames its file into place. |
| Discovery | A first run on a new build captures its baseline into the reference folder; a drill's fake game must not seed it. | `auto_capture` only for a game found through Steam or the config file, never one from `--game` or `DEADLOCK_GAME`. |
| Paths in strings | A Windows path typed into a Python string or a shell heredoc loses its backslashes to escapes (`\f`, `\v`, `\n`). | Write docs with file tools; raw strings or forward slashes in code. A search for control characters in `*.md` (`[\x00-\x08\x0b\x0c]`, or a lone `\r`) finds the damage. |
| Visual QA | After a glTF import, Blender's Workbench texture mode draws each material's active image node, which can be the self-illumination map: a palette costume renders black. | `qa/qa_lib.py` `open_model` makes the image linked to Base Color the active node. |

## Verifying a change

- `cargo test --workspace` green with warnings as errors, `cargo clippy` clean, and the Python tests above for anything they cover.
- A new reader of bytes or text from outside: a fuzz target beside its others, seeded from synthetic fixtures.
- `magician update`: only on a scratch copy of a toolkit folder with a folder as the source, never on the real toolkit or the program on PATH, and never against the live GitHub address.
- Anything that writes into the game or a CSDK tree (install, build locks): test against a scratch folder and a copy of `gameinfo.gi`, never the live install.
- A new or changed rule: its fixture, plus `magician mesh ... --glb` or `lint` against a real mod when behaviour on Valve's data matters; report the numbers.
- Anything the game or Blender renders: look at the picture. Quote what you saw, not just the exit code.
- A change to `tools/bundle.py` or the scene installers: build a bundle into a scratch folder before and after and diff them; live releases depend on byte-identical output.
- A change to how the harness meets the system (`ingame/gameos.py`, `launch.py`, `stop_game.py`): `test_gameos.py` on Linux under Xvfb, and a real session on Windows (`magician session qa` on a stock scenario), with the seat taken and given back.
- Anything with a platform branch: build and test on both systems, or say which one you could not run.

## Sending a change

Changes are welcome, from anyone, as pull requests against `main`.

- For a new lint rule, a new crate or a new command, open an issue first and say what it would catch or do, and on which real mod or build you saw the need. A rule is judged on Valve's own heroes before it is written (rule 5 above), so the issue is where its limit gets agreed.
- A change comes with its test: a fixture that breaks a new rule, a synthetic case for a reader, a scratch folder for anything that writes. `cargo test` and `cargo clippy -D warnings` pass on your system, and the pull request says which system you ran them on.
- Keep a change to one thing, with the docs it touches updated in the same change (each fact in its one home, above).
- The owner reviews every pull request and may ask for changes or take the idea in another shape. Contributions are under the repository's MIT licence.
