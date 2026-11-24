---
name: using-magician
description: Drive Magician, the headless toolchain for Deadlock (Valve, Source 2) mods, to validate, build, release, install, patch-check, grade or bundle a mod. Use when a task mentions Deadlock mods, VPKs or paks, lint rules (M001-M144), a new Deadlock build or patch day, drift, proving a mod on a build, grading an overhaul, hero-page scenes, or release bundles, whether a person is pairing with you or the whole prompt is something like "validate the lash skins for the newest Deadlock release". Covers what Magician is for, the safe order of operations, exit codes, and how to report.
---

# Using Magician

Magician treats a Deadlock mod as a program and each game build as a compiler target. It builds mods reproducibly, lints them against the installed build (75 rules, each with its in-game consequence), works out what a new build changed under a mod, grades a hero mod (depth, identity, fair play, freshness, release gate) as one reproducible record, and installs or rolls back paks through a ledger. Every command is deterministic and its exit code means something, so you can act on results without a person watching.

**Good for:** checking a mod against the installed build; patch day (did Valve's update break anything, and what must be re-filmed); building and releasing from an addon's sources; installing and removing paks safely; shipping hero-page scenes without Valve's bytes; packaging releases for Deadlock Mod Manager.

**Not for:** making art or fixing a mod for you (it reports, you or the user fix); gameplay mods (fair play is a lint category, so a mod that hides or shrinks a hero is an error, by design); managing a player's mods (that is Deadlock Mod Manager); proving a mod works. Compile-clean is not proof: only frames from the running game, looked at, prove a mod.

## Orient first

Work from the repository root (the folder with `Cargo.toml`). The binary is `target\release\magician.exe`; if it is missing or older than the source, `cargo build --release` (no network needed). Commands here are written for Windows; from a POSIX shell use forward slashes, the same `python -I`, and `powershell -File` for the `.ps1` scripts.

```
magician doctor --json    # every part of the setup (game, build, baseline, viewer, CSDK, policy, the mod here): ok, todo, skip or fail, each with its fix
magician paths            # game, reference folder, CSDK, viewer, game lock, and the mod of this folder: what was found and from where
magician --help           # every command and flag, exit codes, how locations resolve; `magician <command> --help` for one
magician install --status # what is installed, by which mod, and whether the game still mounts it (read-only)
```

If `doctor` fails `game`, stop and say so: nearly everything judges a mod against the installed game. Override with `--game DIR` or `DEADLOCK_GAME` only if the user gave you the location.

A mod is a folder with a `mod.toml`. Run from inside it (or name it: `magician check <folder>`), `check`, `lint`, `grade`, `drift`, `release`, `install`, `prove` and `scene check` need nothing after the verb: the paks, hero, allowances, last proven build and scene patches come from the `mod.toml`, and stderr names every file used and where it was found. Quote those lines when you report what was checked. Explicit arguments always win. A hero can be named any way (`lash`, `Lash`, `hero_lash`, a model path); a misspelt command, option or hero is answered with the nearest real one.

The installed build number is `ClientVersion` in `<game>\citadel\steam.inf`; every report names it.

## Rules that are never bent

1. **Read the usage, don't guess.** `magician <command> --help` prints that command's usage and runs nothing; an option no command knows is refused (exit 2). Never probe a command by trying flags on it.
2. **Only `magician install`, `uninstall` and `scene install` write into the game folder.** Never copy, edit or delete anything under the Deadlock install yourself, `gameinfo.gi` included. Never pass `--force` or `--ignore-game` unless the user asked for it on that specific install (install also refuses when it can't tell whether the game runs: that is a person's call too). Writes to the reference folder (`patchday --capture`, `baseline`, `index`, caches, and the baseline the first `lint` or `check` on a new build captures, which stderr announces) are fine; list them in your report. `magician init` writes a `mod.toml` into a mod's folder: only when the task is to set a mod up.
3. **One user of the game at a time.** Before installing, uninstalling or touching the game, run `python -I ingame\gamelock.py status` (exit 0 free, 3 held). If someone holds it, wait or report; never take it from them. If `deadlock.exe` is running and the seat isn't yours, install refuses: ask the person to close the game, don't override. Name yourself with `MAGICIAN_GAME_HOLDER` (or `--holder`) so others can see who you are. Drive the game through `ingame\mod_session.py` (up, run, film, down): it binds the game it launches to your seat, and nothing in the harness will type into or stop a game that is not bound to you. A refused step is a stop: never fall back to killing `deadlock.exe` or forcing the lock.
4. **Don't launch the game unless the task asks for in-game proof**, the machine has an interactive desktop, and the seat is free. Launching, filming and giving the seat back is in [references/loops.md](references/loops.md#in-game-proof).
5. **Never weaken a check to make a mod pass.** No editing rules, no blanket allowances. A finding is accepted only per site, in the addon's `magician.allow`, with a written reason, and only when the user agrees.
6. **Never run `magician prove`** unless the frames for that build were actually looked at (by the user, or by you if you can see images and did). A green lint is not proof.
7. **Don't edit the user's mod sources, `mod.toml` or `bundle.json`** unless that is the task.
8. **Nothing extracted from Valve's files goes into a repository**: no VPKs, `*_c` files, decompiled sources, textures or captures.

## Exit codes

| Code | Meaning | What you do |
|---|---|---|
| 0 | Clean: nothing to do | Report it as clean |
| 1 | Broken: a contract or rule failed (lint errors, mesh FAIL, drift or patchday BROKEN, a scene patch that no longer fits, a failed release lint, `grade --gate` with a blocker) | This is a result, not a crash. Report the findings; don't retry |
| 2 | Usage error: bad arguments, an option without its value or given twice, a missing file, a tool that is not there, a policy that does not read | Read the line after `magician:` on stderr (with `--json`, the `magician.error/1` document on stdout says the same) and fix your call |
| 3 | Refused on purpose: lint errors at install, a game seat or build lock someone else holds, a slot another mod owns, an uninstall of a file whose bytes are not the ones Magician installed | A decision, not a bug: report it, don't route around it |
| 4 | Re-prove: it loads, but must be proven in game on this build (`drift`, `patchday`) | Report which mods need an in-game pass |

The harness scripts have their own: `gamelock.py status` 3 (the game is held by someone else), `launch.ps1` 4 (a game it did not start is running), `mod_session.py` 9 (the seat or the game is no longer this session's: nothing was done), 8 (a frozen hero), 6 (the demo ended before the film did).

`--json` on `check`, `doctor`, `lint`, `grade`, `drift`, `patchday`, `release` and `rules` prints one JSON document (with a `schema` string) and nothing else; prefer it to reading text. Progress lines and colour appear only on a terminal, never in what you read.

## The loops

Each is a short recipe here; the full versions, with what to read in the output, are in [references/loops.md](references/loops.md).

**Validate a mod** (the default reading of "check", "validate", "lint" a mod):
```
magician check <mod folder> --json       # the one verdict (magician.check/1): lint + grade together, drift since its last proven build, scene patches
magician lint <pak> [--deep]             # one pak's findings in full; --deep adds the mesh family M100-M147 (~8 s per hero model; MAGICIAN_THREADS=2 caps its threads beside a running game)
magician grade <pak>... --mod <mod.toml> --json   # the grade record: label, depth, identity, fair play, freshness, gate
```
`check` exits 0 (`ok`: proven on this build and clean), 4 (`re-prove`: loads, not proven on this build) or 1 (`broken`), and its `next` lines say what remains. It grades only when the Source 2 Viewer CLI is set up; otherwise its `grade` step says why and lint stands alone.

**Patch day** (a new Deadlock build landed):
```
magician patchday --capture              # writes the new build's baseline and rig sidecar if missing, then checks every installed mod
magician drift <pak> --from <old build>  # the full list for any mod flagged
```
Per mod it prints `ok`, `RE-PROVE` (loads, but something under it changed since its last proven build: re-film it) or `BROKEN`, then a `verdict`; it exits 0 clear, 4 re-prove, 1 broken. A mod proven on the installed build is `ok (proven on N)`. `--capture` writes only to the reference folder, never the game. Every failure message and its fix: [../../docs/PATCH-DAY.md](../../docs/PATCH-DAY.md).

**Build, release, install** (only when asked to ship or test in game; check the seat first, rule 3):
```
magician release <addon> --hero <hero>   # compile with the CSDK, one pak, lint, grade, release-report.txt
magician install <pak> --mod <owner>     # lints first; refuses lint errors (fair play only when enforced), a running game you don't hold, another mod's slot
magician uninstall --mod <owner>         # rolls back only what the ledger says that mod installed
```
Reuse the owner `install --status` already shows for that mod (`maid_lash`, which can differ from the slug `maid-lash`); a new mod defaults to its pak's folder name. When someone wants to look at a mod in game, install it and tell them it is in; launch only if they asked you to: [references/loops.md](references/loops.md#install-a-pak-for-someone-to-look-at).

**Hero-page scenes:** `magician scene make | check | compose | install`. `check` is read-only and tells you whether the player's stock map still matches a patch after an update.

**Release bundles:** `python -I tools\bundle.py <bundle.json> --out <scratch dir>`; it refuses a path in two paks or any file byte-identical to Valve's.

**Inspecting:** `doctor`, `paths`, `install --status`, `ls`, `refs`, `dump`, `kv`, `textures`, `mesh`, `cloth`, `rules`, `rubric`, `policy` only read. `grade`, `footprint` and `index` need the Source 2 Viewer CLI; if it isn't installed they exit 2, so report that instead of the grade.

## Reporting

Lead with the verdict, then the evidence, then what needs a person. Keep it to what the reader can act on:

```
Deadlock build 6865 (was 6864). 3 mods checked.
- maid-lash: RE-PROVE. Loads; 45 of Lash's clips and his anim graph changed since 6864. Needs an in-game motion pass, then `magician prove`. Grade: S Costume · stale(6864).
- example-skin: BROKEN. Valve renamed bone `spine_1` -> `spine_1_renamed` in the hero's skeleton; 95 files reference it. Still installed: the rig needs rebuilding by the author.
- another-skin: ok.
Lint (light) on both release paks: 0 errors. Wrote: baseline and rig sidecar for 6865 (reference folder). Not done: in-game proof (no desktop session).
```

Always say: the build number; each mod's patch-day label and grade label (`S Echo · NSFR`, quoted as printed); the release-gate blockers if a release was asked about; error counts with rule codes (`M062`, not "a model problem"); every write you made (baseline captured, a pak installed or removed, files created) and every refusal you hit; what you did not do and why. Quote Magician's own words for findings rather than paraphrasing them. When a command exits 2, give its `magician:` line verbatim.

## Where things are

To explain a finding, read the rule: the catalogue is `crates/lint/src/lib.rs`, the lint thresholds `crates/lint/src/mesh/rules.rs`, the grading numbers `crates/grade/src/rubric.rs`; `magician policy` says whether a policy file moved any of them on this machine. Every failure message of patch day is in `docs/PATCH-DAY.md`; game and compiler facts in `docs/reference/engine.md`. The rest of the repository, and the `mod.toml` format: [references/repo-map.md](references/repo-map.md).
