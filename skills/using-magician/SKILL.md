---
name: using-magician
description: Drive Magician, the headless toolchain for Deadlock (Valve, Source 2) mods, to validate, build, release, install, patch-check, re-prove, grade, bundle or start a mod. Use when a task mentions Deadlock mods, VPKs or paks, lint rules (M001-M160), a new Deadlock build or patch day, drift, proving a mod on a build, grading an overhaul, hero-page scenes, release bundles or a mod source, whether a person is pairing with you or the whole prompt is something like "validate the lash skins for the newest Deadlock release". Covers what Magician is for, the safe order of operations, exit codes, and how to report.
---

# Using Magician

Magician treats a Deadlock mod as a program and each game build as a compiler target. It builds mods reproducibly, lints them against the installed build (88 rules, each with its in-game consequence; `magician rules` lists them), works out what a new build changed under a mod, grades a hero mod (what it makes its own, identity, fair play, freshness, release gate) as one reproducible record, drives the retail game to film a mod, records which bytes were proven on which build, and installs or rolls back paks through a ledger. Every command is deterministic and its exit code means something, so you can act on results without a person watching.

**Good for:** checking a mod against the installed build; patch day (did Valve's update break anything, and what must be re-filmed); the in-game pass that re-proves a mod; building and releasing from an addon's sources; installing and removing paks safely; starting a mod folder for a hero; shipping hero-page scenes without Valve's bytes; packaging releases and a mod source for Deadlock Mod Manager.

**Not for:** making art or fixing a mod for you (it reports, you or the user fix); gameplay mods (fair play is a lint category, so a mod that hides or shrinks a hero is a finding, by design); managing a player's mods (that is Deadlock Mod Manager); proving a mod works by itself. Compile-clean is not proof: only frames from the running game, looked at, prove a mod.

## Orient first

One program does everything: `magician` (`target/release/magician` in a checkout, `magician.exe` on Windows; if it is missing or older than the source, `cargo build --release`, no network needed). The parts written in Python run through it too (`magician session`, `reprove`, `bundle`, `source`, `publish`), so you never need to know which is which. Commands here read the same on Windows and Linux: paths use `/`, and `python` is the Python 3 on PATH (`python3` on most Linux systems).

```
magician                  # the game it found, the mod of this folder, and what to run next
magician doctor --json    # every part of the setup (game, build, baseline, viewer, CSDK, toolkit, the mod here): ok, todo, skip or fail, each with its fix
magician paths            # where the game, reference folder, CSDK, viewer, mods folder and game lock resolved to, and from what
magician help <command>   # one command's usage; `magician help` lists them all
magician install --status # what is installed, by which mod, whether the game still mounts it, and which paks share paths (read-only)
```

If `doctor` fails `game`, stop and say so: nearly everything judges a mod against the installed game. Override with `--game DIR` or `DEADLOCK_GAME` only if the user gave you the location.

A mod is a folder with a `mod.toml`. Run from inside it (or name it: `magician check <folder>`) and `check`, `lint`, `grade`, `drift`, `release`, `install`, `prove` and `scene check` need nothing after the verb: the paks, hero, allowances, proofs and scene patches come from the `mod.toml`, and stderr names every file used and where it was found. Quote those lines when you report what was checked. Explicit arguments always win. A hero can be named any way (`lash`, `Lash`, `hero_lash`, a model path); a misspelt command, option or hero is answered with the nearest real one.

Several mods: `patchday` and `reprove` take the ones `--mod` names, else every mod under the folder `magician paths` shows as `mods`, else the nearest folder, from the working one upward, that has mods one or two levels below it. Run them from the folder that holds the mods, or from inside any one of them.

The installed build number is `ClientVersion` in `<game>/citadel/steam.inf`; every report names it.

## Rules that are never bent

1. **Read the usage, don't guess.** `magician help <command>` prints that command's usage, every option it takes, and runs nothing; an option the command does not take is refused before anything runs (exit 2), a typo or another command's option alike, and the message names the commands that do take it. There is no `--dry-run` on a command whose help does not list one. Never probe a command by trying flags on it.
2. **Only `magician install`, `uninstall`, `scene install` and the sessions they run inside write into the game folder.** Never copy, edit or delete anything under the Deadlock install yourself, `gameinfo.gi` included. Never pass `--force` or `--ignore-game` unless the user asked for it on that specific install. Writes to the reference folder (a new build's baseline and sidecars, indexes, caches, in-game passes) are fine; list them in your report. `magician new` and `magician init` write a mod folder or a `mod.toml`: only when the task is to set a mod up.
3. **One user of the game at a time.** The game seat is a lock: `python -I ingame/gamelock.py status` (exit 0 free, 3 held). If someone holds it, wait or report; never take it from them. If `deadlock.exe` is running and the seat isn't yours, install refuses: ask the person to close the game, don't override. Drive the game only through `magician session` or `magician reprove`: they take the seat under a name, bind the game they launch to it, and nothing in the harness will type into or stop a game that is not bound to you. A refused step is a stop: never fall back to killing `deadlock.exe` or forcing the lock.
4. **Don't launch the game unless the task asks for in-game proof**, the machine has an interactive desktop, and the seat is free.
5. **Never weaken a check to make a mod pass.** No editing rules, no blanket allowances. A finding is accepted only per site, in the addon's `magician.allow`, with a written reason, and only when the user agrees.
6. **Never record a proof (`magician prove`, `magician reprove accept`) unless the frames for that build were actually looked at**, by the user, or by you if you can see images and did. A green lint is not proof, and neither is a pass that ran. A proof belongs to the bytes installed when it is recorded (`proofs.tsv` beside the `mod.toml`), so record it with the same paks installed that the pass ran on, never after a reinstall or rebuild. A rebuilt pak is unproven whatever `[proven] builds` lists. Never edit `proofs.tsv` or `[proven] builds` by hand.
7. **Don't edit the user's mod sources, `mod.toml` or `bundle.json`** unless that is the task.
8. **Nothing extracted from Valve's files goes into a repository**: no VPKs, `*_c` files, decompiled sources, textures or captures.

## Exit codes

| Code | Meaning | What you do |
|---|---|---|
| 0 | Clean: nothing to do | Report it as clean |
| 1 | Broken: a contract or rule failed (lint errors, mesh FAIL, drift or patchday BROKEN, a scene patch that no longer fits, a failed release lint, `grade --gate` with a blocker, a re-prove pass that failed) | This is a result, not a crash. Report the findings; don't retry |
| 2 | Usage error: bad arguments, an option without its value or given twice, a missing file, a tool that is not there, a policy that does not read | Read the line after `magician:` on stderr (with `--json`, the `magician.error/1` document on stdout says the same) and fix your call |
| 3 | Refused on purpose: lint errors at install, a game seat or build lock someone else holds, a slot another mod owns, an uninstall of a file whose bytes are not the ones Magician installed, a proof of nothing installed, a publish the release gate blocks | A decision, not a bug: report it, don't route around it |
| 4 | Re-prove: it loads, but must be proven in game on this build (`check`, `drift`, `patchday`); from `reprove`, at least one mod's pass needs a look | Report which mods need an in-game pass, or which passes to read |
| 5 | An update is available (`update --check` only) | Report it. Apply it only on the operator's standing instruction (below) |

A session has a few of its own, and they are the session's alone (`magician session`, `reprove`; 5 from `update --check` is not one of them): 9 (the seat or the game is no longer this session's: nothing was done), 8 (a frozen hero), 7 (the hero never loaded), 6 (the demo ended before the film did), 5 (a game this session did not start is running).

`--json` on `check`, `doctor`, `lint`, `grade`, `drift`, `patchday`, `prove`, `release`, `rules`, `update`, `install --status`, `csdk`, `material` and `reprove` prints one JSON document (with a `schema` string) and nothing else; prefer it to reading text. Progress lines and colour appear only on a terminal, never in what you read.

## The loops

Each is a short recipe here; the full versions, with what to read in the output, are in [references/loops.md](references/loops.md).

**Validate a mod** (the default reading of "check", "validate", "lint" a mod):
```
magician check <mod folder> --json       # the one verdict (magician.check/1): lint + grade together, drift since its last proven build, its proofs, scene patches
magician lint <pak> [--deep]             # one pak's findings in full; --deep adds the mesh family M100-M148 (~8 s per hero model; MAGICIAN_THREADS=2 caps its threads beside a running game)
magician grade <pak>... --mod <mod.toml> --json   # the grade record: label, description, identity, fair play, freshness, gate
```
`check` exits 0 (`ok`: proven on this build and clean), 4 (`re-prove`: loads, not proven on this build) or 1 (`broken`), and its `next` lines say what remains. A proof answers a re-prove, never a break: a drift that broke since the build these bytes were proven on before stays `broken` after a proof on this build (the drift step carries `proven_here`), and only new bytes fix it. Under its lint line it counts the findings by rule (errors, warnings, fair play, and fair play's advisories, which never fail it; `by_rule` in the JSON): report those, and read `magician lint --deep` for the sites (`check` lints deep; a bare `lint` is light and lists less). On a first look at a mod nobody has proven, `re-prove` with no errors is the good answer: the files hold nothing that breaks it, and its `next` line says so before it offers the in-game pass. It grades only when the Source 2 Viewer CLI is set up; otherwise its `grade` step says why and lint stands alone.

**Patch day** (a new Deadlock build landed):
```
magician patchday --capture              # writes the new build's baseline and sidecars if missing, then checks every installed mod
magician drift <pak> --from <old build>  # the full list for any mod flagged
```
Per mod it prints `ok`, `RE-PROVE` (it loads, and these bytes are not proven on this build) or `BROKEN`, then a `verdict`; it exits 0 clear, 4 re-prove, 1 broken. It also says whether the update changed the engine (`engine too`, with the files) or `data only`, and for each mod whether any name in its hero's data moved; neither changes the verdict, both tell you how hard to look. `--capture` writes only to the reference folder, never the game. Every message and its fix: [../../docs/PATCH-DAY.md](../../docs/PATCH-DAY.md).

**Re-prove in game** (the verdict was re-prove and the task includes in-game proof; rules 3 and 4 first):
```
magician reprove --plan                  # what would run for each mod, and what cannot; launches nothing
magician reprove                         # every mod to re-prove: a vanilla pass when the build has no baseline, the mod's pass, one report
magician reprove accept <pass folder>    # only after the contact sheets were looked at (rule 6)
```
It holds the seat only while it plays, puts back any pak it took out, and gives each mod `ready`, `look` or `failed` (exit 0, 4, 1) in a report under `<reference>/work/passes/`. None of the three is proof: it judges warnings, not pictures. Open the contact sheets and say what you saw, or say that you could not look. `accept` refuses a pass that is not whole, not on the installed build, or not of the installed paks, then records the proof with the pass as evidence. One mod by hand: `magician session qa --mod <folder> --out <dir>`.

**Is a finding true?** (a mod is BROKEN and someone asks whether it really is; rules 3 and 4 first):
```
magician session qa --mod <folder> --out <dir> --pak <pak> --as pak02_dir.vpk --restore [--force] witness   # the mod in, the stock probe, the pak out again
magician session qa --mod <folder> --out <vanilla dir> witness                                              # the same probe without the mod (a mod already installed for that hero: --vanilla --restore)
magician witness <pak> --log <dir>/console_witness.txt --vanilla <vanilla dir>/console_witness.txt
```
Each finding comes back confirmed (the game said it), contradicted, not reached (with what would reach it), a frame's (look at the frames it names) or a file fact, then the game's complaints no finding predicted. `--force` installs a pak lint refuses, only for that look; say that you used it. `magician rules` shows each rule's record on this build.

**Build, release, install** (only when asked to ship or test in game; check the seat first, rule 3):
```
magician release <addon> --hero <hero>   # compile with the CSDK, one pak, lint, grade, release-report.txt
magician install <pak> --mod <owner>     # lints first; refuses lint errors (fair play only when enforced), a running game you don't hold, another mod's slot
magician uninstall --mod <owner>         # rolls back only what the ledger says that mod installed
```
Reuse the owner `install --status` already shows for that mod (`maid_lash`, which can differ from the slug `maid-lash`); a new mod defaults to its pak's folder name. When someone wants to look at a mod in game, install it and tell them it is in; launch only if they asked you to: [references/loops.md](references/loops.md#install-a-pak-for-someone-to-look-at).

**Start a mod:** `magician new <hero> [--slug NAME] [--dir DIR] [--author NAME]` writes a mod folder from the installed game's own data: a `mod.toml` with the hero's real model and skeleton, the addon folders with the ignore files that keep Valve-derived files out of git, a package folder, two starter scenarios. It copies no Valve file and never writes into a folder that exists. `magician init` writes only the `mod.toml`, for a pak or an addon that already exists; given a download or paks from elsewhere it copies the paks into the mod's folder first, each proven the same bytes. A mod made without Magician: [moving-to-magician](../moving-to-magician/SKILL.md).

**Staying current** (Magician only goes online when you run `update` or `doctor --updates`; nothing else checks):
```
magician update --check --json           # magician.update/1: state current|behind|ahead|unknown, judgement.changed, judgement.rerun; exit 0, 5 an update is available, 3 could not check
magician update --dry-run                # what applying would do, changing nothing
magician update                          # apply it
```
Checking is always fine to do and to report ("behind: 0.2.0 is out"; "could not check" is a note, not a failure). **Apply an update only when the operator's standing instruction says to**, never because a check said one exists: a grade is only worth something while the judge stays the same, and the update replaces the toolkit you are running. It refuses (exit 3, nothing changed) while a game session holds the seat, so never force past it. When `judgement.changed` is true, grades and lint from before the update were made by a different judge: after updating, re-run `magician check` (or `lint` and `grade`) on every mod you report on, and say that you did. Exit 1 means the update failed and was rolled back; report the `magician:` line.

**Materials** (someone wants a material's settings, wants one carried to another mod, or asks whether a compile kept what the source asked for):
```
magician material inspect <x.vmat | x.vmat_c | pak.vpk:path | valve:path> --json   # shader, features, params, each slot's format, mips and channel meanings
magician material diff <a> --against valve                                         # a material beside Valve's at its path, in compiled terms (exit 1 when they differ)
magician material export <material> --out setup.preset                             # the setup, paths written against the material's own
magician material apply setup.preset --to <content>/materials/<x>/new.vmat         # the setup for a new material; names the images it still needs
magician material verify <addon>                                                   # after a build: every compiled material against its source, down to the packed channels' pixels
```
`verify` exits 1 when a compiled material does not hold what its source asked for. A WARN for M150 is a name no Valve material of the shader sets: most likely a typo or a scene setting that does nothing; fix it in whatever generates the `.vmat`, never by hand in a compiled file. A preset from a compiled file guesses its inputs from the images it was compiled from and says so on each line; say that when you hand one over. `magician csdk` says which kit compiles (12 or 13) and what it cannot do; read it before blaming the mod for a compile that a kit gap explains.

**Hero-page scenes:** `magician scene make | check | compose | install`. `check` is read-only and tells you whether the player's stock map still matches a patch after an update.

**Publishing:** `magician bundle <bundle.json> --out <scratch dir>` makes a Deadlock Mod Manager release bundle; it refuses a path in two paks or any file byte-identical to Valve's. With `--target divine` it writes a Divine Skins upload kit instead (the zip, the pictures, `listing.txt` with the form's fields and AI answers); it submits nothing. `magician source <source.json>` writes a static mod source and its pages from bundles; it states only the builds a proof holds the zip's own paks on. Neither publishes anything: putting the result on a host is the user's step. `magician publish nexus <bundle.json>` does publish: it uploads the bundle's zip to a Nexus Mods page the user made on the site, as its first file or a new version of it (the page and file in bundle.json's `nexus` block, the key in `NEXUSMODS_API_KEY`; after a first file, tell the user the printed file id belongs in that block), and refuses with 3 when the release gate on the zip's own paks has a blocker. `--page` rewrites the page's summary and description. Never run it, `--page` or `--ignore-gate` least of all, without the operator's instruction for that release; `--dry-run` sends nothing, so show its output first. Nexus requires each page to declare generative-AI use; the author sets it on the page, never the tool or you.

**Inspecting:** `doctor`, `paths`, `install --status`, `ls`, `diff` (two paks: the same mod or not, exit 1 when not), `refs`, `dump`, `kv`, `textures`, `mesh`, `cloth`, `rules`, `rubric`, `policy` only read. `grade`, `footprint` and `index` need the Source 2 Viewer CLI; if it isn't installed they exit 2, so report that instead of the grade.

## Reporting

Lead with the verdict, then the evidence, then what needs a person. Keep it to what the reader can act on:

```
Deadlock build 6865 (was 6864), data only. 3 mods checked.
- maid-lash: RE-PROVE. Loads; 45 of Lash's clips and his anim graph changed since 6864. Pass run: ready, no warnings over vanilla; sheets read, cloth follows the new clips. Proof recorded. Grade: Costume (look, effects, sound).
- example-skin: BROKEN. Valve renamed bone `spine_1` -> `spine_1_renamed` in the hero's skeleton; 95 files reference it. Still installed: the rig needs rebuilding by the author.
- another-skin: ok.
Wrote: baseline and sidecars for 6865, two passes under work/passes (reference folder); maid-lash proofs.tsv and mod.toml (uncommitted).
```

Always say: the build number; each mod's patch-day label and grade label (`Echo (look, some effects) · freeform`, quoted as printed); the release-gate blockers if a release was asked about; error counts with rule codes (`M062`, not "a model problem"); what you saw in the frames, or that nobody looked; every write you made (a baseline captured, a pak installed or removed, a proof recorded, files created) and every refusal you hit; what you did not do and why. Quote Magician's own words for findings rather than paraphrasing them. When a command exits 2, give its `magician:` line verbatim.

## Where things are

To explain a finding, read the rule: the catalogue is `crates/lint/src/lib.rs`, the lint thresholds `crates/lint/src/mesh/rules.rs`, the grading numbers `crates/grade/src/rubric.rs`; `magician policy` says whether a policy file moved any of them on this machine. Every patch-day message is in `docs/PATCH-DAY.md`; game and compiler facts in `docs/reference/engine.md`. The rest of the repository, and the `mod.toml` format: [references/repo-map.md](references/repo-map.md).
