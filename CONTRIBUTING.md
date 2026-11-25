# Contributing to Magician

Magician is a headless toolchain for creating, validating, and maintaining Deadlock mods through automation, scripts, and AI agents. It is a Rust workspace with zero external crates, plus Python and PowerShell around the game. [README.md](README.md) says what it is and how to start using it; this file is for changing it: how the repository is laid out, the rules every change follows, the gotchas the code already handles, and how to verify your work.

Where knowledge lives:

- [docs/PDD.md](docs/PDD.md): what Magician is for (automation first), what it promises its caller, its taste and non-goals.
- [docs/TDD.md](docs/TDD.md): the technical model (builds as targets, mods as programs), crates, locations, the files it owns, the pipeline.
- [docs/reference/engine.md](docs/reference/engine.md): facts about Deadlock, the CSDK compiler and Source 2 Viewer that the code relies on, and the scars behind them.
- [docs/STATUS.md](docs/STATUS.md): current state, loose ends and known gaps. [CHANGELOG.md](CHANGELOG.md): what changed.
- [docs/PATCH-DAY.md](docs/PATCH-DAY.md): the patch-day runbook. [ingame/README.md](ingame/README.md), [mcp/README.md](mcp/README.md), [cinema/README.md](cinema/README.md): their subsystems.
- `docs/briefs/`: bounded plans and investigations while they are open. Drafts, not canonical truth; what proves durable moves into a core doc or a test, and the brief goes. None is open now (grading v2 was the last; its decisions are in [docs/TDD.md](docs/TDD.md), Grading).
- Executable references: `magician --help` (every command and flag, the exit codes, how locations resolve; `magician <command> --help` for one), `magician rules` (the rule catalogue), `magician rubric` (the grading criteria in force), `magician policy` (every judgement number in force and where it came from), `magician paths` (where this machine's game, reference folder and tools are, and the mod of the working folder), `magician doctor` (what is missing on this machine, with each fix).

## Posture

- **Headless first.** The caller is a script or an agent. No prompts, no output that only works in colour, exit codes that mean something (0 clean, 1 broken, 2 usage error, 3 refused on purpose, 4 re-prove; `cli.rs`), stable labels a script can match, one JSON document with a `schema` string where a machine is the reader. A new command or flag earns a line in `magician`'s usage text: options are checked against it, so an undocumented one is refused (and a test fails if a switch is missing from it).
- **A compiler, not a loader.** Never grow a runtime, a mod manager or a launcher. Output must install in other people's managers.
- **The game build is the era.** Baselines, drift and "proven on N of M builds" are the spine; don't hardcode facts about one build where a baseline read will do.
- **Compile-clean is not proof.** Exit code 0 is one input. Visual proof from `ingame/` and `qa/` decides.
- **Fair play is a lint category.** Cosmetic only. A rule that would let a mod make a hero harder to read is a defect in the linter.
- **Say what was found.** A value a command fills in for its caller (from the mod's folder, the config, Steam) is named on stderr with where it came from, and shown by `magician paths` or `magician doctor`; an explicit argument always wins. A new command that can be completed from a mod's folder does it in `modctx::fill`, never inside the command, and a new command goes in `COMMANDS` (main.rs) so a typo of it is answered.
- **Write through `out.rs`.** `outln!`, `out!`, `say!` (the human report, silent under `--json`) and `errln!`, never `println!` or `eprintln!`: a closed pipe must end in the command's own exit code, not a panic. Colour (`style.rs`) and progress lines (`progress.rs`) only ever on a terminal, and the words must read without them.
- **No machine in the code.** Outside locations go through `crates/magician/src/paths.rs` and `ingame/gamepaths.py` (flag, environment, config file, discovery). Never write a drive letter or a user folder into a default.

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
                  initcmd.rs, newcmd.rs (a skin's folder from nothing), frontdoor.rs (session, bundle and source:
                  the Python tools through the CLI), out.rs (every write to stdout and stderr), style.rs, progress.rs, build.rs, lock.rs (per-addon
                  build lock), install.rs (slots, ledger, scene route, seat check), scene.rs (scene patches), patchday.rs and
                  patchcmd.rs (eras, rig sidecar, drift verdicts, prove), proofs.rs (the proof ledger: which
                  bytes were proven on which build, and the one "is it proven here" question), gamefiles.rs, meshcmd.rs, clothcmd.rs, markcmd.rs,
                  jsonout.rs
mcp/              MCP server: the CLI as JSON tools for agents, Python stdlib only; no install or launch tool
skills/           using-magician: the Agent Skill for driving the CLI on a mod
ingame/           retail-game driver, Python stdlib and PowerShell; mod_session.py is the one entry point for a mod's session,
                  takes.py the timed and staged takes, gamelock.py the one-user-at-a-time game lock (the game bound to
                  the seat), gamepaths.py the harness side of paths.rs; scenarios/ holds stock-hero examples
qa/ harness/      offline visual QA and Blender render scripts; qa/heroes/haze.json is the example hero profile (a mod
                  keeps its own and passes its path)
cinema/           the optional, dev-only film kit for trailers and stills (pak90, never installed during QA or play)
tools/            bundle.py (player release bundles with mod-interchange.json), scene_install.ps1 / .py (the player-side
                  scene installers bundles ship), scene_lump.py (re-dress a UI scene map), rip-heroes.ps1 (hero glbs for
                  reference, never committed), s2v_mcp.ps1 (the Source 2 Viewer MCP launcher); test_bundle.py and
                  test_scene_install.py hold the bundler's and the installers' guards
docs/             PDD.md, TDD.md, STATUS.md, PATCH-DAY.md, reference/engine.md, briefs/
```

## Commands

```
cargo build --release            # target\release\magician.exe
cargo test --workspace           # synthetic fixtures only; needs no game and no Valve file
magician --help                  # every command and flag, the exit codes, how locations resolve
magician paths                   # where the game, reference folder, CSDK and viewer resolved to on this machine
magician rules                   # what each rule costs in game
magician patchday                # the patch-day check (read-only without --capture)
python mcp\test_server.py        # the MCP server end to end (packs its own pak; lints it when the game is found)
python ingame\test_gamelock.py   # the game lock, the game bound to the seat
python ingame\test_takes.py      # the timed-take plans and the frozen-hero check, offline
python ingame\test_mod_session.py    # the session driver; its install and restore against a scratch game (release binary)
python cinema\test_replay.py     # the free-camera film's shot list and demo-end check, offline
python -I tools\test_bundle.py   # the bundler's guards (needs the release binary, not the game)
python -I tools\test_scene_install.py  # both player installers and magician on good and malformed patches, offline
python cinema\test_scene.py      # the film kit's language and encode, offline (needs ffmpeg)
python cinema\test_session.py    # the film kit's seat hand-back, on a fake clock
```

Before a change lands, run what CI would:

```
set RUSTFLAGS=-D warnings && cargo test --workspace         # warnings are errors
cargo clippy --workspace --all-targets -- -D warnings       # the lint set below, as errors
```

The lint set lives in the workspace `Cargo.toml` (`[workspace.lints]`, which every crate takes): unsafe code is denied
and each library crate forbids it (the two exceptions, the build lock's process check and the fuzz allocator, say why
where they are); clippy's correctness and suspicious groups are errors; the readers of other people's bytes (`vpk`,
`resource`, `mesh`) also deny `unwrap`, `expect` and slicing a `str` at a byte offset. Dev and test builds check
integer overflow. A lint is fixed, not allowed, unless the allow says why on its line.

Every reader of bytes or text someone else made has a fuzz target in its crate's `tests/fuzz.rs` (`crates/fuzz` is the
harness). `cargo test` runs a short, fixed set of cases; a long run is a time per target:

```
set MAGICIAN_FUZZ=600 && cargo test --release --test fuzz -- --test-threads 1     # every target, 600 s each
set MAGICIAN_FUZZ_GAME=D:\...\Deadlock\game                                     # also mutate real blocks, read only
set MAGICIAN_FUZZ_SEED=7 && set MAGICIAN_FUZZ_CASE=1234                           # one case again
set MAGICIAN_FUZZ_BACKTRACE=1                                                     # where an allocation over the cap came from
```

A case that panics, hangs (10 s) or asks for one allocation over 256 MiB stops the run, names its seed and number, and
saves its input; fix the reader, then copy the input into that target's `tests/fuzz-cases/<target>/` under a name
that says what it found, so every later run replays it first. Inputs there come from synthetic seeds, never from a
game file: a case found with `MAGICIAN_FUZZ_GAME` is a mutation of Valve's bytes, so it is not kept; write a test that
builds the same shape from nothing instead (`typed_null_arrays` in `crates/resource/tests/fuzz.rs` is one).

`--help` after any command prints its usage and runs nothing (`crates/magician/tests/contract.rs` holds that and the exit codes to the built binary). `crates/magician/tests/real_mods.rs` grades real mods when this machine has them installed (Maid Lash from a `model-replacements/maid-lash/` folder above the repository, and any new-character mod with a voice pak named by `MAGICIAN_REAL_NEW_CHARACTER`), and skips otherwise. CI (`.github/workflows/ci.yml`) runs the workspace tests, clippy and the Python tests on a clean Windows runner.

The `dev` profile runs at opt-level 1 because VPK trees are 150k+ entries and unoptimised parsing is needlessly slow.

## Rules for changes

1. **Zero external crates.** Parsers, codecs and decoders are written here and proven against Source 2 Viewer's output. Don't add a dependency to shorten a task. On the Python side the MCP server, the harness, the bundler and the player installers are standard library (the bundler uses Pillow for previews only); the offline QA and render scripts already use Pillow, numpy, scipy and Blender's own Python. Add nothing new to either.
2. **Every lint rule ships with a fixture that breaks it** and a test that the rule fires on that fixture for that reason. `every_rule_has_a_fixture` (crates/lint/src/tests.rs) fails the build if a rule has none. Mesh fixtures are built with `crates/mesh/src/synth.rs`, so every triangle is chosen.
3. **Never weaken a rule to make a mod pass.** Per-site allowances go in the addon's `magician.allow` with a mandatory reason; allowed errors stay visible, stale allowances are reported. Grading follows: an allowed warning that is not fair play is not a defect; an allowed error still counts against depth (max C). Fair-play findings are judged apart and never touch the letter: an allowed M140 (hitbox overflow past 10 units that earns its place as a silhouette piece) holds within its limit and is listed with its measure; M129 (the egregious tier: over 18 units, or over 3.5% of the visible surface beyond 10) is never allowable and makes the grade NSFR. The thresholds live in one constants block, `crates/lint/src/mesh/rules.rs`, and the grading numbers in `crates/grade/src/rubric.rs`; a policy file moves them at run time, never the code. A new judgement number gets a constant there and a key in the policy.
4. **Never write into the Deadlock install except through `install.rs`.** It backs up `gameinfo.gi` per build, is idempotent and keeps a ledger so it never touches files it didn't install.
5. **Judge against Valve's own model at the same path.** Things Valve's file already does are Valve's design, not a finding. The test of it: Valve's own file, packed unchanged and linted deep, is silent, for every hero (a rule that speaks there is wrong, however good its physics: the limit of a new rule is measured on Valve's heroes before it is written down). Every bounded search says when it stops (M100), and every fair-play comparison that could not run says so (M144): unchecked is never clean.
6. **Say what a rule can't see.** Unverified in-game behaviour is marked UNVERIFIED where it is claimed.
7. **Keep the player side standalone.** `tools/scene_install.ps1` and `scene_install.py` ship inside release bundles and run on players' machines: Windows PowerShell 5.1 or Python 3 stdlib, no Magician, no config file, byte-identical output to `magician scene install`.

## Content rules

The repository is kept clean enough to be public.

- Nothing extracted from Valve's files is committed: no VPKs, compiled resources (`*_c`), decompiled sources, textures, glb exports, renders or captures. `.gitignore` enforces the extensions; tests and fixtures are synthetic.
- Paths, CRCs, rule thresholds and numeric findings are facts about the game and may appear in docs and tests; bytes of its content may not.
- Fixtures that need real-looking bytes get them from `synth.rs` or inline constants (for example the KV3 format GUID), never from a game file.
- No personal paths, names or machine details in code or docs. Screenshots and harness failure shots (`harness_fail_*.png`) never go in; look before you `git add` a folder the harness wrote into.
- Commits are plain imperative subjects with a short body.

## Toolkit gotchas

Engine and compiler behaviour is in [docs/reference/engine.md](docs/reference/engine.md). What follows is about this code.

| Area | Issue | Where handled |
|---|---|---|
| Text sources | A UTF-8 BOM makes resourcecompiler reject vtex, vmat, vpcf, vsndevts, vmdl, vdata, vcss, svg, kv3 and dmx. Windows PowerShell 5.1 `Set-Content -Encoding utf8` writes one. | `build.rs` refuses by name before compiling and never rewrites sources. Write files with `[IO.File]::WriteAllText`. |
| Game | A stop freed the seat while `deadlock.exe` was still exiting; the next holder launched, and the first pipeline's fallback (`taskkill /IM deadlock.exe`, `release --force`) killed the new game (2026-11-24). | `gamelock.py`: `launch.ps1` binds the game's pid and start time to the seat; `console.py` and `stop_game.ps1` drive only the bound game of the caller's seat (`may_drive`); a release is refused while the bound game lives; `stop_game.ps1` holds the seat until the process has exited and never forces. `test_gamelock.py` holds each. |
| Compiler | A `DistanceFieldList` only allocates with the `distancefield` ModelDoc feature, which the CSDK's gameinfo lacks. | `build.rs`: `distance_field_game` writes `<csdk>/game/citadel_dstf/gameinfo.gi` (the CSDK's plus that feature, the original untouched) and those documents compile with `-game` pointing at it. |
| Compiler | resourcecompiler hangs after its summary; fed a whole addon it tries every png, dmx and `.gitignore`. | `build.rs` ends it 20 s after the summary and compiles real sources only. |
| Pack | Compiled outputs whose source was deleted stay in the game tree forever. | `build.rs` packs only outputs that map back to a live source. |
| Pack | A path shipped in two paks is won by the lower pak number, so a later release of the other pak can mask or regress it. | An addon's `magician.exclude` (path prefixes, `#` comments) leaves those compiled files out of `build` and `release` paks (listed as `left out N excluded by magician.exclude`). `tools/bundle.py` refuses a bundle where a path is in two paks or a file is byte-identical to Valve's. |
| Build | Two builds of one addon at once (a release overlapping another pipeline's compile) corrupt each other's run in `game\citadel_addons\<addon>\`. | `lock.rs`: a `.magician-build.lock` (pid, start time, holder) in that folder; a second taker waits (`--lock-timeout`) or fails at once (`--no-wait`); a dead or reused pid is stale. |
| Game | Several pipelines drive one retail game; a second `map` or `selecthero` mid-scenario wrecks the first one's shots. | `ingame/gamelock.py`: a lock file naming the holder (`MAGICIAN_GAME_HOLDER`), taken by launch, console and mod_session.py, cleared by stop_game, stale when no deadlock.exe runs. |
| Locations | The CLI's seat check and the harness's game lock must read the same file, or install would swap paks under a running capture. | One resolution order in `paths.rs` and `gamepaths.py` (`MAGICIAN_GAME_LOCK`, config `game_lock`, else `<reference>\work\ingame-game.lock`). Change both or neither. |
| Install | Two installs or uninstalls at once each rewrite the ledger from what they read, and one row is lost. | `install.rs`: the ledger is read, changed and written under `.magician-installed.lock` in `addons`; the pak, `gameinfo.gi` and the ledger are written to a temporary name and renamed into place. |
| Install | Deadlock Mod Manager renumbers every pak in `addons`, so a ledger row can name a file that moved. | `install::reconcile`: a row whose file is missing or changed follows its crc to the one unclaimed pak with those bytes; uninstall never removes bytes that are not the ones it installed. |
| Locks | A lock file created empty and written after can be read, by a taker that has waited, as a stale lock to clear: two holders, or a lock put back after its holder left. | `lock.rs`: a lock is written under its own name and hard-linked into place, whole or not at all, and carries a token so threads of one process tell their locks apart. |
| Install | Two mods default to `pak10_dir.vpk`; copying the second over the first silently uninstalls it. | `install.rs`: the ledger records the owner; another mod's file, a file we didn't install and one changed since are never replaced without `--force`; `--as pakNN_dir.vpk` picks another slot. |
| Install | Installing or uninstalling while another pipeline drives the game swaps the pak under its capture (or under someone's match). | `install::seat_check`: refused while deadlock.exe runs unless the caller holds the game lock (`--holder` / `MAGICIAN_GAME_HOLDER`); `--ignore-game` overrides. |
| KV3 | A binary KV3 write that keeps blobs inline decodes in Magician (it accepts all layouts) but not in Source 2 Viewer. | `kv3::encode` writes blobs in blocks with the closing 0xFFEEDD00; `blobs_live_in_blocks_in_the_layout_the_game_uses` pins the bytes. A round trip through our own decoder proves nothing. |
| Cloth | `PHYS` holds the FeModel: integrator per node, collision radii per non-static node (`node - m_nStaticNodes`), shapes with layer masks. Two decoders would drift. | `mesh::cloth` is the one read; `lint::cloth` (M141) and `magician cloth` both use it. |
| Mesh | Index buffers hold absolute vertex ids; `m_nAppliedIndexOffset` and `m_nVertexCount` are metadata, not offsets to add. Blend indices count in the MDAT's own skeleton, matched by bone name. | `crates/mesh/src/model.rs` |
| Geometry | A triangle-triangle test must share one intersection-line direction for both intervals, or it misses every fold. | `geom::intersect` (found by the M109 fixture) |
| Resource | Big hero models have 100+ blocks; bound the block table by file size, not a constant. | `crates/resource` |
| Textures | Hero colour maps are BC7 stored uncompressed, smallest mip first. | `resource::pixels` |
| Lint | Lint blames a mod only for references it introduced; Valve's own dangling references are not findings. | `crates/lint/src/baseline.rs` |
| Lint | A drill with a fake `--baseline` once wrote `particle_classes.tsv` into the real reference folder under the fake build number. | Every command takes `--reference`; point drills at a scratch folder. |
| Speed | A deep mesh lint takes about 8 s per hero model cold (the census runs on all cores) and about 1 s from the findings cache. The first mesh lint ran the full census by default and took over 15 minutes on one release. | Keep new mesh rules bounded and say when they stop; `mesh_family_cost_is_bounded` holds deep under 20 s and light under 2 s on a 10,800-triangle fixture. |
| Output | `println!` panics when the reader has gone (`magician ls x \| head -1` exited 101). | `out.rs`: a closed stream is dropped for the rest of the run; `a_reader_that_stops_reading_does_not_crash_the_command` holds it. |
| Viewer | Source 2 Viewer prints `--- Wrote 1 files` on stdout and drops `exceptions.txt` in its working folder, which corrupted `grade --json` and littered the user's folder; two grades decompiling one file at once clashed. | `footprint::Sources::text` runs it in a private folder under the cache with stdout closed and renames its file into place. |
| Discovery | A first run on a new build captures its baseline into the reference folder; a drill's fake game must not seed it. | `auto_capture` only for a game found through Steam or the config file, never one from `--game` or `DEADLOCK_GAME`. |
| Paths in strings | A Windows path typed into a Python string or a shell heredoc loses its backslashes to escapes. | Write docs with file tools; raw strings or forward slashes in code. |

## Verifying a change

- `cargo test --workspace` green with warnings as errors, `cargo clippy` clean, and the Python tests above for anything they cover.
- A new reader of bytes or text from outside: a fuzz target beside its others, seeded from synthetic fixtures.
- Anything that writes into the game or a CSDK tree (install, build locks): test against a scratch folder and a copy of `gameinfo.gi`, never the live install.
- A new or changed rule: its fixture, plus `magician mesh ... --glb` or `lint` against a real mod when behaviour on Valve's data matters; report the numbers.
- Anything the game or Blender renders: look at the picture. Quote what you saw, not just the exit code.
- A change to `tools/bundle.py` or the scene installers: build a bundle into a scratch folder before and after and diff them; live releases depend on byte-identical output.

## Where things go

- What Magician is for, its promises to callers, taste, the name: [docs/PDD.md](docs/PDD.md).
- Technical model, crates, locations, owned files, pipeline: [docs/TDD.md](docs/TDD.md).
- Facts about the game, the compiler and the viewer, one fact in one place: [docs/reference/engine.md](docs/reference/engine.md).
- Current state, loose ends, known gaps: [docs/STATUS.md](docs/STATUS.md). Shipped capabilities: [CHANGELOG.md](CHANGELOG.md), newest first.
- Patch-day procedure and what each failure means: [docs/PATCH-DAY.md](docs/PATCH-DAY.md).
- How the in-game harness behaves, and game facts learned by driving it: [ingame/README.md](ingame/README.md).
- How to drive Magician from an AI agent: [skills/using-magician/](skills/using-magician/SKILL.md). Keep it in step when a command, exit code or safety rule changes.
- Bounded investigations, plans and dated decisions: `docs/briefs/`.
