# Magician

Magician is a headless toolchain for creating, validating, and maintaining Deadlock mods through automation, scripts, and AI agents.

It treats a Deadlock mod the way a compiler treats a program, and each game build the way a compiler treats a target. A mod is built reproducibly from its sources, linted against the build that is installed, re-linked when Valve ships a new one, graded for how much of a hero it actually reaches, and installed or rolled back without touching a file Magician didn't put there. Every one of those steps is a command with a deterministic answer and an exit code that means something, so a script or an agent can run the whole loop with nobody at the desk. A person can run the same commands; they just aren't where the design starts.

Named for The Magnificent Sinclair, Deadlock's own magician, and a sibling to [Mycelium](https://github.com/worflor/mycelium), which does the same job for Minecraft mods.

## What it is not

- **Not a GUI.** No windows and no prompts. Commands read files and print text, and JSON where a machine consumes it.
- **Not a Blender add-on.** Blender appears only as an offline renderer for visual QA (`qa/`, `harness/`).
- **Not a mod loader or manager.** Deadlock has no runtime mod API: "loading" is a VPK in an addons folder plus one search-path line in `gameinfo.gi`, and [Deadlock Mod Manager](https://github.com/deadlock-mod-manager/deadlock-mod-manager) already does that well. Magician's install step exists so an automated pipeline can put a mod in the game and take it out again. For players it writes what the manager reads: `magician bundle` makes a release bundle with its `mod-interchange.json`, and `magician source` turns bundles into a static mod source players add to the manager once for one-click installs and updates ([docs/reference/mod-source.md](docs/reference/mod-source.md)).
- **Not an auto-updater.** It won't rewrite your mod for a new build. It tells you exactly what the build changed underneath the mod and whether that breaks it. The fix is still yours, or your agent's.

Most Deadlock modding tools are built for a person at a desk: a GUI, a Blender extension, or a thin layer over the game files. Magician is built for the machine that is still running when nobody is.

## Getting started

You need Windows 10 or 11, a stable Rust toolchain, and Deadlock installed through Steam. The workspace has no external crates, so the build downloads nothing.

```
git clone https://github.com/worflor/magician
cd magician
cargo build --release              # target\release\magician.exe
cargo test --workspace             # synthetic fixtures only: needs no game and no Valve file
```

Then let it tell you where you stand:

```
magician                           # the game it found, the mod of this folder, and what to run next
magician doctor                    # every part of the setup, one line each, with the fix for any gap
```

### A mod is a folder with a `mod.toml`

To start a skin from nothing, `magician new <hero>` writes that folder from the installed game's own data: a `mod.toml` whose `[target]` is the hero's real model and skeleton, an `addon\` laid out at the game's paths with `.gitignore` files that keep Valve-derived work (a model document made from Valve's, meshes on Valve's skeleton, renders on Valve's body, mixes over Valve's audio) out of git, a `package\` that `magician bundle` takes once there is a pak, two in-game scenarios for the hero, and a README with the next commands. It copies no Valve file and never writes into a folder that exists.

```
magician new lash --author you     # .\lash-skin\: mod.toml, addon\, package\, source\ingame\scenarios\, README.md
cd lash-skin
magician                           # "no pak yet", and the command that builds the first one
magician release --out pak10_dir.vpk
```

If you have a pak and nothing else, `magician init` writes the `mod.toml`, taking the hero from the files the pak replaces. Either way, every command run in that folder (or below it) knows the mod, its paks, its hero and its allowances, and says on stderr what it used:

```
cd my-lash-skin                    # holds pak33_dir.vpk
magician init                      # writes mod.toml: hero_lash (Lash), 644 of 644 files are its own
magician check                     # lint + grade + drift + scene patches: one verdict, then the evidence
magician install                   # lints first, backs up gameinfo.gi, never under a running game
```

```
my-lash-skin: RE-PROVE  B Costume · NSFR · unproven  (build 6865)
  lint   warn   0 error(s), 365 warning(s), 0 allowed (deep; fair play fail)
  grade  warn   B Costume · NSFR · unproven for Lash; release gate blocked: freshness: not proven on 6865 (no proven build on record for these bytes)
  proof  warn   these bytes proven on none
  drift  warn   these bytes are proven on no build yet: nothing to compare from
next: the in-game pass on build 6865 (`magician session qa --mod C:\mods\my-lash-skin --out <dir>`), look at the frames, then `magician prove --evidence <dir>`
```

`check` exits 1 when something is broken, 4 when the mod loads but is not proven on the installed build, 0 when it is proven here and clean. The same bare verbs work for `lint`, `grade`, `drift` (from the mod's last proven build), `release` (its addon), `uninstall`, `prove`, `footprint` and `scene check`; whatever you pass explicitly wins. `magician help` lists every command and option, `magician help <command>` shows one, and a mistyped command, option or hero is answered with what it most likely meant. A hero can be named the way you would say it: `lash`, `Lash`, `hero_lash`, `"Lady Geist"`, or its model's path.

A `mod.toml` is plain text, read line by line: flat tables of strings, numbers and lists, the shape `new` and `init` write. The keys Magician reads:

```
[mod]
slug  = "maid-lash"                 # the install-ledger owner is the slug with - as _
name  = "Maid Lash"
author = "you"                      # also the key of the maker's mark (`sign`, `verify`)
hero  = "hero_lash"
paks  = ["pak10_dir.vpk"]           # optional: else the paks beside it, else the ones installed for it
addon = "maid_lash"                 # optional: the CSDK addon `release` builds (default: the slug with _)
owners = ["lash_showcase"]          # other ledger names its paks or scenes install under
[target]
model    = "models/heroes_wip/lash/lash.vmdl"      # patch day checks these still exist in a new build
skeleton = "models/heroes_wip/lash/lash.vnmskel"
[proven]
builds = [6864]                     # the summary `magician prove` keeps; the proofs themselves are in proofs.tsv
[rights]
state = "cleared"                   # unreviewed | cleared | denied: a person's call
[ingame]
pass = ["lash_angles", "lash_motion"]   # the scenarios an in-game pass of this mod runs
```

### Where it finds things

Magician finds Deadlock through Steam's own library list. Everything it derives from a build (baselines, sidecars, indexes, in-game passes) goes to a reference folder. Each location is a flag, then an environment variable, then a line in a config file, then discovery:

| Location | Flag | Variable | Config key | Otherwise |
|---|---|---|---|---|
| Deadlock's `game` folder | `--game` | `DEADLOCK_GAME` | `game` | found through Steam |
| Reference folder | `--reference` | `MAGICIAN_REFERENCE` | `reference` | `%LOCALAPPDATA%\magician\reference` |
| Valve's CSDK | `--csdk` | `MAGICIAN_CSDK` | `csdk` | none: `build`, `release` and `sign` ask for it |
| Source 2 Viewer CLI | `--vrf` | `MAGICIAN_VRF` | `vrf` | `Source2Viewer-CLI` on PATH |
| The mods `patchday` checks | `--mod` (each one) | `MAGICIAN_MODS` | `mods` | the nearest folder, from the working one upward, with mods one or two levels below it |
| Game lock | | `MAGICIAN_GAME_LOCK` | `game_lock` | `<reference>\work\ingame-game.lock` |
| Policy (the judgement numbers, below) | `--policy` | `MAGICIAN_POLICY` | `policy` | the built-in defaults |
| The toolkit folder | | `MAGICIAN_HOME` | `home` | the folder above `magician.exe` that holds `ingame\mod_session.py` |
| Python | | `MAGICIAN_PYTHON` | `python` | `python`, `python3` or `py -3` on PATH |

The config file is `%APPDATA%\magician\config.txt` (or wherever `MAGICIAN_CONFIG` points), one `key = value` per line:

```
# where Magician finds things on this machine
game      = E:\Games\SteamLibrary\steamapps\common\Deadlock\game
reference = E:\deadlock-reference
csdk      = E:\Tools\Reduced_CSDK_13
vrf       = E:\Tools\Source2Viewer-21.1\cli\Source2Viewer-CLI.exe
mods      = E:\mods
```

`magician paths` prints every location and where the answer came from. The Python and PowerShell around the game read the same file through `ingame/gamepaths.py`, so the harness and the CLI always agree on where the game and the game lock are.

The first command run on a new build captures that build's baseline into the reference folder, as `magician baseline` would, so later drift checks have it. It does that only for this machine's own game (found through Steam or named in the config file), never for one a flag or `DEADLOCK_GAME` points at, which is often a drill's copy.

### One front door

Everything goes through `magician`. The parts written in Python run through it too, with your arguments, stdio and exit code passed through unchanged, so a script names one program:

| Verb | Runs | For |
|---|---|---|
| `magician session qa --mod <folder> --out <dir>` | `ingame\mod_session.py` | a mod's in-game session: seat, install, launch, scenarios, takes, stop, contact sheets (`up`, `run`, `film`, `down`, `all`, `qa`) |
| `magician reprove [--plan]`, `magician reprove accept <pass>` | `ingame\reprove.py` | patch day in game: every mod that needs a re-prove gets its pass, one report says where to look; `accept` records a pass that was looked at |
| `magician bundle <bundle.json> [--zip]` | `tools\bundle.py` | a player release bundle with `mod-interchange.json` and the scene installers |
| `magician source <source.json>` | `tools\source.py` | a static Deadlock Mod Manager mod source, with its pages, from bundles |

Everything after the verb belongs to the tool (`magician bundle --help` is the bundler's help). When the toolkit folder or Python is missing the verb says which and how to point at it, and exits 2; `magician doctor` has a line for both.

### What each part needs

| You want to | You need |
|---|---|
| Inspect a pak (`ls`, `refs`, `dump`, `kv`, `textures`, `verify`), prove an expression (`expr`) | nothing else |
| Lint, mesh and cloth reports, `pack` (it lints what it packs), drift, `patchday`, `install`, `scene`, `new` | Deadlock installed |
| `footprint`, `grade`, `index` | Deadlock and the [Source 2 Viewer](https://github.com/ValveResourceFormat/ValveResourceFormat) CLI (21.1; flags change between versions, so pin it). The decompiled files are cached per build in the reference folder |
| `build`, `release`, `sign` | Valve's CSDK 13, the community-distributed build that carries `resourcecompiler.exe` (setup notes in [docs/reference/engine.md](docs/reference/engine.md#the-compiler-csdk-13)) |
| `session`, `reprove`, `bundle`, `source`, the MCP server | Python 3.11 or newer, standard library only; `bundle` and `source` use Pillow for preview images |
| Offline visual QA (`qa/`, `harness/`) | Blender 5.2 and the Source 2 Viewer CLI; some scripts use Pillow, numpy and scipy |
| The film kit (`cinema/`) | ffmpeg, a Python venv with Boon, and the CSDK; see its README |

### Platforms

The CLI is built and tested on Windows. On Linux the workspace builds and its tests pass; it finds Steam's libraries, sees a game running under Proton (its seat check reads `/proc`), and refuses to swap paks when it cannot tell whether the game is running. The in-game harness is Windows only: it types with SendInput and captures the game's window. Players don't need Magician at all: a release bundle installs its hero-page scene with `install-scene.ps1` (Windows PowerShell) or `install-scene.py` (Python 3, proven on Ubuntu and meant for the Steam Deck).

## Patch day on an always-on machine

The case Magician is shaped around: a spare Windows box that is on anyway, with Deadlock installed and your mods in its game folder.

1. Valve ships a build. Steam on that box downloads it. Magician doesn't watch Steam; your scheduler does (a changed `ClientVersion` in `game\citadel\steam.inf` is the signal).
2. A script runs one command, offline, in a few seconds:

   ```
   magician patchday --capture
   ```

   It snapshots the new build (every path and CRC, every skeleton's bones and every anim graph's parameters, and a hash of every executable, library and shader archive), checks that the game still mounts the addons folder, works out what changed under every installed pak since the last build a proof holds its bytes on, checks each mod's `mod.toml` targets, and ends with a verdict. On a patch where Valve re-exported Lash's clips it reads:

   ```
   [3] drift: each mod from its last proven build -> installed 6864
     example_voice: ok (compared from 6861): no mod.toml claims this owner
       pak06_dir.vpk: no drift from 6861 to 6864
     maid_lash: RE-PROVE (last proven on 6861)
       pak10_dir.vpk: loads on 6864; re-prove the 45 changed original(s) and 1 changed rig file(s) in game
   ...
   verdict
     loads, but re-prove in game: example-skin, maid-lash
   ```

   and exits 4: everything loads, something must be re-proven in game. When Valve renames a bone a skin depends on, the same check says `bone 'spine_1' is now 'spine_1_renamed'`, prints `BROKEN`, and exits 1. A patch that leaves every mod proven on the installed build exits 0. An `engine` line says whether the update changed only data (`data only`) or the code that runs the mods' files too (`engine too`, with the files), and each mod's line says whether anything its hero's data names (model, abilities, sounds, particles) moved. Neither changes the verdict; both say how hard to look. `--json` prints the same report as one document.
3. Only now does anything wake up an agent, and only for the mods that need one. For a broken mod it reads the full list with `magician drift <pak> --from <old build>`, fixes the sources, rebuilds with `magician release` and swaps the pak with `magician install`, or takes it out with `magician uninstall --mod NAME` if it can't fix it yet.
4. `magician reprove` plays every mod that needs a re-prove, one after another, and writes one report: each mod `ready`, `look` or `failed`, with its contact sheets. It judges warnings, not pictures, so someone looks at the sheets: a person, or an agent that can read images.
5. `magician reprove accept <pass folder>` records the proof of the installed bytes on the new build. From then on `patchday` reports that mod current, and a pak rebuilt later is unproven again until its own pass.

What reaches your phone is short and specific: *new Deadlock build; 13 mods checked; 12 load and only need their motion re-filmed; one is broken because Valve renamed a bone in Lash's skeleton.*

The full runbook, with every failure message and its fix, is [docs/PATCH-DAY.md](docs/PATCH-DAY.md).

## Driving it from a script

Every exit code has one meaning:

| Exit code | Meaning |
|---|---|
| 0 | Clean: nothing to do. |
| 1 | Broken: a contract or rule failed and a person or agent must act (lint errors, a mesh FAIL, `drift` or `patchday` BROKEN, a scene patch that no longer fits the game, a release whose lint failed, `grade --gate` with a blocker). |
| 2 | Usage error: bad arguments, a missing file, a tool that is not there, a policy that does not read. |
| 3 | Refused on purpose, nothing changed: an install with lint errors, a game seat or build lock someone else holds, a pak slot another mod owns, an uninstall of a file the ledger doesn't hold or whose bytes are no longer the ones Magician installed, a proof of nothing installed. |
| 4 | Re-prove: it loads, but must be proven in game on this build (`check`, `drift`, `patchday`). |

The reason for 1, 2 and 3 follows `magician:` on stderr. `--help` (or `-h`) after any command prints its usage and does nothing else, `--version` names the toolchain and commit, and an option no command knows is a usage error rather than something silently ignored, as is an option given without its value, a single-valued option given twice, and an option whose value would be the next option.

`--json` on `check`, `doctor`, `lint`, `grade`, `drift`, `patchday`, `prove`, `release` and `rules` prints one JSON document on stdout and no human text. Each starts with a `schema` string (`magician.check/1`, `magician.grade/2`, `magician.patchday/1`, ...) so a reader can tell what it holds. A command that stops (exit 1, 2 or 3 with a reason) prints the reason as that one document, `{"schema":"magician.error/1","command":...,"exit":2,"kind":"usage","error":...}`, so stdout always parses. Also machine-readable: `lint --json-out FILE`, `mesh --json-out FILE`, `grade --record-out FILE`, `paths` and `policy` (tab-separated), and `lint --findings-out FILE` (a lint run that `grade --findings` judges without linting again). Results are deterministic: `pack` writes byte-identical VPKs from identical input, lint findings come out in a fixed order, a grade record is byte-for-byte the same for the same paks, build, policy and toolchain, and the findings cache only ever returns what a fresh run would compute.

For agents there is an MCP server, [mcp/](mcp/README.md): seventeen tools (`check`, `lint`, `mesh`, `grade`, `release`, `build`, `drift`, `patchday`, `policy`, `refs`, `ls`, `dump`, `cloth`, `verify`, `rules`, read-only `doctor` and `install_status`) that return JSON, the commands' own documents where they have one. Installing, uninstalling and launching the game are deliberately not among them; those stay CLI calls that something has to choose to make. An agent that drives the CLI itself can load the skill in [skills/using-magician/](skills/using-magician/SKILL.md): the loops, the safety rules and how to report, in the Agent Skills format.

What it leaves to the host:

- **Noticing the update.** Magician reads the installed build. Watching Steam, scheduling the check and sending the message are the scheduler's.
- **A desktop for the game.** The in-game pass types into the retail game and captures its window, so it needs an interactive Windows session with the game in front. It can't run as a service with nobody logged in.
- **The look.** A pass ends in contact sheets, and something that can see has to read them before a proof is recorded.

## Lint

`magician lint <mod.vpk>` runs 79 rules (M001 to M148) against the installed build. Every rule says what happens in the game when it fires, not just that something is "wrong": `magician rules` prints the catalogue. Findings are judged against Valve's own file at the same path, so what Valve already does is Valve's design, not a finding, and a mod is blamed only for references it introduced. Fair play is a category: a silhouette smaller than Valve's, a hidden VFX cue or a dropped sound is a finding, and a fair-play comparison that could not run (a file, or Valve's original, that did not decode) is said, M144, never passed in silence. Fair play is lenient by default and enforced with one switch: until a policy sets `[fair_play] enforce = true`, its findings are warnings, so `lint` doesn't fail and `install` doesn't refuse on them alone; enforced, they are errors. Every other error (a broken reference, a protected file) is an error either way. `--deep` adds the mesh family (M100 to M148), which decodes every triangle of a replaced model and compares it with Valve's.

A finding can be accepted only per site, in the addon's `magician.allow`, with a written reason. Allowed findings stay in the report, and an allowance that no longer matches anything is reported as stale. `grade --mod mod.toml` finds the mod's own file without `--allow`: `addon/magician.allow` or `magician.allow` beside the `mod.toml`, then the CSDK addon named by its slug and its `[mod] owners`. The record lists the files it read, or says none was found, and `magician paths --mod mod.toml` shows where they are.

## Grading

`magician grade <pak>... --hero <hero>` (or `--mod <mod.toml>`, whose `[mod] hero` names the hero) grades every pak of one mod together, as the game loads them: where two paks ship a path, the lower pak number wins. The grade is a measured claim about separate properties of the mod, and its label is a short projection of the record behind it:

```
S Echo                      new character, everything done, fair, current
A Echolite · FP?            new character in progress, fair play couldn't be fully checked
S Costume · NSFR            the same character, complete, a confirmed fair-play violation
A Kit · stale(6864)         a strong kit, last proven on 6864
not graded (HUD mod)        no rubric for it, said so instead of a fake F
```

Three axes that never touch each other:

- **Depth** is the letter, S to F: how completely the mod fulfils its archetype. It means completeness, not taste: S is "it did it all", never "it looks great", and letters compare within an archetype, never across. A character (Echo, Echolite, Costume) is judged on nine weighted criteria against the hero's footprint, every file the hero is made of, discovered from the game's own data (2,201 files for Lash on build 6864): hero model, materials, ability VFX, kit sound, animation, UI art, showcase scenes, voice and craft. A variant (Valve's mesh, new materials) is judged on every visible material slot of the hero; a kit (no model) on the departments it touches, at least two of VFX, sound, UI and voice, each complete per ability, weapon and movement before it can be S. Caps keep it honest about the mod working: shipping a skeleton, graph or data file caps it at D, lint errors that are not fair play at C, dropped dependencies at B.
- **Compliance** is fair play: nothing in the label when it passes, `NSFR` (not safe for ranked) on a confirmed violation, `FP?` when a check that should have run could not (a light lint over a model, or M144). Unknown never becomes pass, and a violation never lowers the letter. Each fair-play rule is never allowable (a hidden cue, a smaller silhouette, M129's egregious hitbox overflow), advisory (reported, never a violation: M081, M130), or allowable within a limit (M140, accepted with a reason up to the M129 line); the record lists every allowance with its measured value and limit.
- **Freshness** is `stale(<last proven build>)` when the paks graded are not proven on the build they are judged for, `unproven` when no proof holds them, nothing when current. It is asked of the bytes: the builds a recorded proof holds every graded pak on.

Identity is decided by signals and stored in the record: a new character needs two of `head_replaced` (our head's vertices against Valve's), `renamed` (a new name in the mod's localisation, or a new name logo for a name that is not the hero's), `new_hero_art` (the portrait and card art) and a declared new voice, worth half a signal. A new character with all but the voice grades `S Echo` with its voice pak (`A Echolite` from the skin pak alone); Maid Lash keeps Valve's head and is still Lash, `S Costume`, with its M140 allowance listed and fair play `pass`. A new voice is declared in the `mod.toml` (`[identity] voice = "new"`), because bytes can't tell a new voice from a treatment of the old one.

The **release gate** is separate from the label: it decides whether automated packaging or publishing may go ahead. Its blockers are lint errors that are not fair play, files byte-identical to Valve's at the same path, not proven on the target build, and rights denied. Fair play and rights are lenient by default: NSFR, FP? and unreviewed rights are listed as gate warnings, and the label and record still say them. A policy enforces each with one switch (`[fair_play] enforce = true`, `[rights] enforce = true`), and then they block. Rights are a person's call, never Magician's: `[rights] state = "cleared"` (or `"denied"`, default `"unreviewed"`) in the `mod.toml`. `grade` exits 0 whatever the grade; `--gate` makes it exit 1 when the gate has a blocker.

`--json` prints the record (`magician.grade/2`): the toolchain and commit, the build, a fingerprint of the baseline and of the policy, every pak's SHA-256, every criterion with its weight and coverage, the caps, the identity signals with what was measured for each, every fair-play check run or not run, violation, allowance and advisory, freshness, rights, the gate, and the next steps. Whoever reads it later can explain the grade from it alone. `grade` lints deep unless `--light` (fair play then reads `FP?` for a mod with a model). `magician rubric` prints the criteria, weights, letters and caps in force; their single home is `crates/grade/src/rubric.rs`. There are rubrics for hero mods only: a map, HUD or music mod is `not graded`, and says so.

## Policy

Every number that encodes a judgement can be moved without rebuilding Magician: the fair-play limits (silhouette, the three hitbox-envelope tiers, the particle, light and sound cue limits), the allowance limits and which rules are never allowable or advisory, whether fair play and rights are enforced, the weights per archetype, the letter thresholds, the identity requirements and the coverage targets. With no policy every value is the built-in default from its one home (`crates/grade/src/rubric.rs`, and the constants block in `crates/lint/src/mesh/rules.rs`).

A policy is a plain-text file in the shape `mod.toml` uses; a key left out keeps its default:

```
# a stricter house line
[fair_play]
enforce = true
hitbox_egregious_units = 15
[compliance]
m140_max_worst_units = 12
[rights]
enforce = true
```

It is found like every other location: `--policy FILE`, then `MAGICIAN_POLICY`, then `policy = ...` in the config file, else the built-in defaults. `magician policy` prints every value in force and where it came from (`default`, or `house.toml line 3`), and every grade record carries the policy's fingerprint and the keys it moved, so a grade is reproducible. A key the reader does not know, a value of the wrong kind or out of range, or weights that don't total 100 are a usage error (exit 2) naming the line and the key. The findings cache keys on the limits in force.

## Proof

Compile-clean is not proof. A mod is done when it has been seen working in the game.

- [`ingame/`](ingame/README.md) drives the retail game: launch, type console commands as hardware scancodes, capture the window, diff the console log against a vanilla run of the same build, read crash dumps. `magician session` is one session for any mod with a `mod.toml`: the seat, its pak, the launch, its hero, scenarios and timed takes, a SourceTV take filmed from free cameras, the stop and the hand-back, contact sheets. `magician reprove` runs those sessions for every mod a game update left unproven. A game lock keeps two pipelines from driving one game at once, and binds the game to the holder that launched it, so nobody types into or stops another's.
- `qa/` and `harness/` check and render models offline in Blender: x-ray, lookdev, motion and hole detection, contact sheets.
- [`cinema/`](cinema/README.md) is an optional, dev-only film kit for trailers and stills: a scene file becomes an animatic, then a frame-exact render from a SourceTV take of the retail game. Nothing else depends on it, and it is never installed during QA or play.

A mod's scenarios, QA configs and scene patches live with the mod; this repository carries worked examples on a stock hero (Haze). [Maid Lash](https://woflo.dev/deadlock-skins/maid-lash/), a released Lash skin, is built, graded, proven and shipped with this loop.

### A proof belongs to bytes

`magician prove` is the step after an in-game pass was looked at. It records, in `proofs.tsv` beside the `mod.toml`, the game build, the date, every file the install ledger holds for the mod (owner, file name, crc32, sha256) and the folder the pass was written into (`--evidence DIR`, with the sha256 of its `index.md`), and adds the build to `[proven] builds`. Everything that asks whether a mod is proven on a build (`check`, `grade`'s freshness and release gate, `patchday`, `drift`, `doctor`, the mod-source page) asks it of the bytes it is judging: a recorded proof of that build that holds every one of them. So a rebuilt pak starts unproven, whatever `[proven] builds` lists, and a build listed there with no proof for these bytes is shown as "claimed for other bytes or never recorded" and counts for nothing; the list itself is never rewritten or trimmed. `prove` refuses (exit 3, nothing written) when nothing is installed for the mod, or what is installed is not the bytes the install ledger put there.

```
magician prove --evidence <pass folder>                                    in the mod's folder, after the pass was looked at
magician prove mod.toml --build 6865 --date 2026-11-24 --evidence <file>  a pass on an earlier build, the same files installed
magician prove mod.toml --build 6864 --pak <release copy>\pak10_dir.vpk    bytes that are no longer installed
```

The ledger is plain text, one row per file per proof, sorted, and deterministic:

```
build  date        proof             owner          file                           crc32     sha256       evidence                       evidence_sha256
6866   2026-11-24  03abbf6e4c9a86a3  lash_showcase  maps/ui/hero_prefabs/lash.vpk  d287ca64  81f9345a...  ...\work\passes\maid-lash\...  c0314ba1...
6866   2026-11-24  03abbf6e4c9a86a3  maid_lash      pak10_dir.vpk                  24994007  8fdb0ea4...  ...\work\passes\maid-lash\...  c0314ba1...
```

A row stands on its own, so two machines' copies merge as a union of their lines (`merge=union` in git does it; so does keeping both sides of a conflict). The `proof` id is a hash of the build and the proof's (owner, sha256) pairs: the same bytes proven on one build from two machines are one proof with two dates, and a proof that lost a row in a merge no longer adds up to its id and counts for nothing. A line that does not read is reported, never dropped, and `prove` will not rewrite the file until it is fixed.

## Shipping a hero-page scene

A skin's hero-page scene is a map, `maps/ui/hero_prefabs/<hero>.vpk`, and it is mostly Valve's: of Maid Lash's 25 files, 22 are Valve's compiled map byte for byte. What is ours is the entity lump (the lights), a shadow-card model and, for Lash, the showcase model. Shipping the whole VPK would hand out Valve's files, so `magician scene` ships only ours and lets the player's own game supply the rest.

A **scene patch** is a folder: `scene-patch.txt` (the target map and, for every Valve file the scene keeps, its CRC) and `files/`, holding only our own files at their paths inside the scene. The player runs one command and the stock map comes from their Deadlock, every Valve file it keeps is checked against the CRC the patch was made with, ours are added, and the scene VPK is written.

```
magician scene make <scene.vpk> --out <patch dir> [--mod NAME]   author: split a built scene into a patch
magician scene check   <patch dir>                               does this game's stock map match the patch? (reads only)
magician scene compose <patch dir> --out <file.vpk>              build the scene somewhere, install nothing
magician scene install <patch dir> [--mod NAME] [--force]        compose, then install it as `install` would
```

`make` is for the author. Every file of the scene that is byte-identical to Valve's at that path becomes a `keep` line (a CRC and a size, no bytes); every other file is copied into `files/`. Nothing Valve-made is ever written into a patch, and `magician bundle` checks that again when it assembles a release.

`compose`, `check` and `install` are for the player. They read the stock `game\citadel\maps\ui\hero_prefabs\<hero>.vpk`, not the addons copy, and they refuse, writing nothing, when it is not what the patch expects: after a game update the message lists the files that changed or went missing and says the patch needs a new release. That is on purpose. A scene built on a changed Valve map could light a room that no longer exists. They also refuse a patch whose own files do not match the manifest's CRCs, and a composed scene whose CRC is not the `result` the author proved (`--force` writes it anyway), as the player installers do. The result is deterministic (the entries are sorted), so composing the same patch on any machine gives the same bytes, and the patch records the CRC of the scene its author proved so `compose` can say "identical to the one the author proved".

`install` goes through the same code as `magician install`: the seat check while the game runs, the ledger, the `gameinfo.gi` backup, and one slot per hero, so it will not overwrite another mod's scene without `--force`. Run `magician uninstall maps/ui/hero_prefabs/<hero>.vpk` to take it out.

A release bundle carries the patch under `scene/<hero>/` with installers that need neither Magician nor Python on Windows (`install-scene.cmd` runs `install-scene.ps1`) and only Python 3 elsewhere (`install-scene.sh` runs `install-scene.py`). Both produce the same bytes as `magician scene install`. The patch format is documented at the top of `crates/magician/src/scene.rs`.

## Reading the game natively

Everything Magician knows about a build it reads itself, with zero external crates: VPK read and write; the Source 2 resource block table and reference lists; binary KV3 (v4 and v5, LZ4 and zstd blocks) read and written; texture headers and pixels (BC7, DXT, ATI); compiled model meshes down to every triangle (meshoptimizer codecs v0 and v1), skeletons, hitboxes and the cloth FeModel. The decoders are checked against Source 2 Viewer's output: meshes through `mesh --glb`, cloth to within 4e-7, KV3 writes line for line. Valve's files themselves are never part of this repository, and nothing Magician ships contains them.

## Commands

| Command | Does |
|---|---|
| `magician doctor` | This machine's setup, one line per part, each with its fix |
| `magician new <hero> [--slug NAME] [--dir DIR] [--author NAME]` | Start a mod from nothing: a folder with the `mod.toml`, addon layout, package and starter scenarios for that hero |
| `magician init [<pak> \| <addon dir>]` | Write a `mod.toml` for a pak or an addon's sources (the hero from the files it replaces) |
| `magician check [<folder> \| <mod.toml>]` | One mod's verdict on the installed build: lint and grade together, drift since its last proven build, its scene patches (exit 1 broken, 4 re-prove) |
| `magician patchday [--capture] [--from BUILD] [--mod mod.toml]...` | The patch-day check: baseline, install state, each mod's drift from its last proven build, whether the engine changed, a verdict (exit 1 broken, 4 re-prove) |
| `magician baseline` | Snapshot the installed build to `<reference>\<build>\baseline.tsv`, with `rig.tsv` and `engine.tsv` beside it |
| `magician drift <mod.vpk>... --from BUILD [--to BUILD]` | What a patch changed under a mod, down to bones and anim graph parameters (exit 1 broken, 4 re-prove) |
| `magician reprove [--mod FOLDER]... [--plan]` / `reprove accept <pass>` | Patch day in game: a pass for every mod that needs one and one report; `accept` records a pass that was looked at |
| `magician session up \| run \| film \| down \| all \| qa` | One in-game session for a mod: seat, pak, launch, scenarios, takes, stop, contact sheets |
| `magician prove <mod.toml> [--evidence DIR] [--build BUILD] [--date DATE] [--pak FILE]...` | Record a proof of the mod's installed bytes (or the files `--pak` names) on a build in its `proofs.tsv`; refused when nothing is installed for it (exit 3) |
| `magician lint <mod.vpk>` | All rules; exit 1 on errors |
| `magician mesh <vpk> <model>` / `cloth <vpk> <model>` | One compiled model's mesh verdict, or its cloth, beside Valve's model at the same path |
| `magician grade <pak>... --hero HERO \| --mod mod.toml` / `rubric` / `footprint HERO` | Grade a hero mod (depth, identity, fair play, freshness, release gate); the rubric in force; everything a hero is made of |
| `magician policy` | Every judgement value in force, where it came from, and its fingerprint |
| `magician build <addon> <out.vpk>` | Compile an addon's real sources with Valve's resourcecompiler, pack deterministically, lint |
| `magician release <addon> --hero HERO [--mod mod.toml]` | One VPK from every department, linted and graded, with a release report and the grade record |
| `magician install <pak>` / `install --status` / `uninstall <pak> \| --mod NAME \| --all` | The only write into the game install: lint first, back up `gameinfo.gi`, ledger every file (under a lock, written atomically, following a pak a mod manager renamed by its crc), roll back from the ledger, never removing bytes it did not install |
| `magician scene make \| check \| compose \| install` | Ship a hero-page scene as a patch of only our files and build it from the player's own game |
| `magician bundle <bundle.json>` / `source <source.json>` | A player release bundle for Deadlock Mod Manager; a static mod source and its pages from bundles |
| `magician sign <addon>` / `verify <vpk>` | Put an invisible authorship mark in a mod's colour textures, and detect it in a compiled pak. The key is the author's: `--key`, else `[mark] key` or `[mod] author` in the mod's `mod.toml` |
| `magician paths` | Where every outside location resolved to, and why |
| `magician ls`, `refs`, `scan`, `dump`, `kv`, `textures`, `index`, `expr`, `pack`, `cache` | Inspection, evidence queries, material expression proofs, raw packing, the findings cache |

## Layout

```
crates/   vpk, resource, mesh, geom, lint, grade, expr, mark, magician (the CLI)
mcp/      the MCP server: the CLI as JSON tools for agents
skills/   using-magician, an Agent Skill for driving the CLI
ingame/   drive the retail game: sessions, patch-day passes, console, capture, log diff, crash dumps, example scenarios
qa/       offline visual QA and detectors (Blender, glb)
harness/  Blender render scripts: x-ray, lookdev, contact sheets
cinema/   the optional film kit for trailers and stills
tools/    release bundles, the mod-source writer, player scene installers, UI scene re-dressing, hero rips, the Source 2 Viewer MCP launcher
docs/     design, status, the patch-day runbook, engine and mod-source reference
```

## More

- [docs/PDD.md](docs/PDD.md): what Magician is for, the taste that follows from it, and the name.
- [docs/TDD.md](docs/TDD.md): how it is built.
- [docs/PATCH-DAY.md](docs/PATCH-DAY.md): the patch-day runbook.
- [docs/reference/engine.md](docs/reference/engine.md): the Deadlock, compiler and Source 2 Viewer facts Magician relies on.
- [docs/reference/mod-source.md](docs/reference/mod-source.md): publishing through Deadlock Mod Manager from your own site.
- [docs/STATUS.md](docs/STATUS.md): current state and loose ends. [CHANGELOG.md](CHANGELOG.md): history.
- [CONTRIBUTING.md](CONTRIBUTING.md): working inside the repository.

## Licence

See [LICENSE](LICENSE).
