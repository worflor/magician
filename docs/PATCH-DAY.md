# Patch day

Valve shipped a new Deadlock build, and every installed mod has to be checked against it. The game build is the era: a build is a baseline (`<build>/baseline.tsv`, every path and CRC of `pak01_dir.vpk`) plus a rig sidecar (`<build>/rig.tsv`, every skeleton's bones and every AnimGraph's control parameters), both in the reference folder. Drift is what changed under a mod's paks between two eras.

The worked example throughout is Maid Lash (ledger owner `maid_lash`), with its hero-page scene, and a second mod, `example-skin` (`example_skin`, with a voice pak `example_voice`). Swap in your own mods; nothing below depends on those names.

Everything is offline except the in-game re-prove. `M` is `target\release\magician.exe`; `magician paths` says which game and reference folder it will use (`--reference DIR` or `MAGICIAN_REFERENCE` points any command elsewhere). `patchday` finds mods through a `model-replacements/*/mod.toml` layout above the working folder (else the `mod.toml` of the working folder), or takes them with `--mod <mod.toml>`. For one mod in depth, `magician check <its folder>` adds its lint, grade and scene patches to its drift, as one verdict with the same exit codes.

Drilled on 2026-11-21 against build 6864 and the real 6861 to 6864 patch; the synthetic-drift results are in "What the drill showed". The steps are written for a person or an agent working through them; "Unattended" below maps them onto a scheduled job.

## Unattended

What a script on an always-on machine can do without anyone, and where it has to hand over:

| Step | Command | Decide on |
|---|---|---|
| Notice the update | Steam (the client or SteamCMD) downloads it; the host's scheduler watches `ClientVersion` in `game\citadel\steam.inf` | Magician does not watch Steam |
| Check | `M patchday --capture` (add `--json` for one document) | exit 0: clear, stop here. Exit 4: everything loads, the mods in `loads, but re-prove in game: <mods>` (`verdict.reprove` in JSON) need an in-game pass. Exit 1: something is BROKEN, read the lines above `verdict`. Exit 2: could not run (stderr) |
| Contain | `M uninstall --mod <owner>` for each broken mod | players are not left with a broken hero while it waits for a fix |
| Detail | `M drift <pak> --from <old>` | the full lists, for whoever fixes it |
| Hand over | wake an agent or send a message with the per-mod lines | everything after this needs judgement or the game |

`M patchday --json` prints the whole check as one document (`magician.patchday/1`): the baseline and rig state, the install state and ledger, per mod its state (`ok`, `re-prove`, `broken`), the build it was compared from and every pak's drift, the manifests, and the verdict. The text keeps one `<owner>: ok | RE-PROVE | BROKEN` line per mod and the `verdict` block. The re-prove, the fix and `prove` are the steps below, done by a person or an agent at a desktop where the game can run in front.

## The commands, in order

```
1.  M install --status                      what is installed, is the addons path still in gameinfo.gi
2.  M patchday --capture                    THE entry point (below). Captures the new build's baseline + rig, then checks everything
3.  M patchday --from <build>               re-run with every mod compared from one older era (default: each mod's last proven build)
4.  M drift <pak> --from <old>              full lists for any pak patchday flagged (paks are in game\citadel\addons\)
5.  fix, rebuild, M release ...             only for BROKEN (next section); a clean patch skips this
6.  re-prove in game                        the scenarios below, then LOOK at the frames
7.  M prove <mod.toml> --evidence <pass>    record a proof of the installed bytes on this build (proofs.tsv), per mod, then commit
                                            proofs.tsv and the mod.toml
8.  ingame\stop_game.ps1                    hand the game back
```

`M patchday --capture` does, in order:

1. **Baseline** for the installed build (`ClientVersion` in `game\citadel\steam.inf`). Missing: written (`--capture`) or refused with the command to run. Present: compared with the live game, and a mismatch is reported as STALE (same build number, different bytes, so a hotfix). It never overwrites an existing baseline. With `--capture` it also writes a missing `rig.tsv` beside it; without, it only says the sidecar is missing. About 1 s for the baseline, a few seconds for the rig (107 skeletons, 1,972 graphs on 6864).
2. **Install state**: the `Game citadel/addons` line in `gameinfo.gi`, the per-build backup, and every ledger row against the file on disk (missing file, or a crc that is not ours).
3. **Drift** of every ledger pak (skin pak, pose pak, voice pak, hero-page scenes) from the last build a proof holds its mod's installed bytes on (the mod's `proofs.tsv`) to the installed one, grouped by mod, with the model-contract check (bones, attachments, hitboxes against Valve's current models). The ledger owner is matched to a `mod.toml` by its slug (`maid-lash` installs as `maid_lash`) or `[mod] owners = [...]` (a voice pak or a showcase scene installed under its own name); an owner no manifest claims is compared from the newest older baseline, and says so.
4. **Mods**: each `model-replacements\*\mod.toml`: do its `[target]` model and skeleton still exist in the new build, and `its installed bytes proven on N of M builds`, with any build `[proven] builds` lists that no proof binds to these bytes said as `claimed for other bytes or never recorded`, and the file that differs from the proof of the installed build when one does. When the mod names its hero (`[mod] hero`) and the hero data was edited since the build the mod is compared from, a line says whether any name in the hero's block moved (the hero data is kept as text per build under `<build>\decompiled\scripts\heroes.vdata`; `--capture` writes it, which needs Source 2 Viewer).
5. A **verdict**: `clear` (exit 0), `loads, but re-prove in game: <mods>` (exit 4), or `BROKEN: n problem(s)` (exit 1).

A mod whose installed bytes are proven on the installed build reads `ok (proven on 6864)`: there is nothing to re-film for it, whatever changed between older eras. Proven means a recorded proof of that build holds the sha256 of every file installed for the mod; a pak rebuilt since is unproven, whatever `[proven] builds` lists. A mod whose last proven build has no baseline on record, or whose bytes no proof holds, is compared from the newest older one, and the line says so.

Whole check: 2 to 15 s on 6864 depending on whether the disk cache is warm (the paks are 110 MB and 104 MB). Run it before launching anything.

Also useful: `M lint <pak>` (uses the cached baseline of the installed build; without one it reads the live game), `M refs <vpk> <path>`, `M grade`, `M verify` (the maker's mark, build-independent, so it never needs re-proving on a patch).

## What each failure means, and the fix

`patchday` prints the first eight problems per pak; `M drift <pak> --from <old>` prints 25 of each kind.

| Message | Meaning | Fix |
|---|---|---|
| `MISSING ...baseline.tsv` | No baseline for the installed build | `M patchday --capture` |
| `STALE: n file(s) differ ...` | Baseline has the right build number but not the game's bytes (a hotfix without a number bump, or a partial update) | Let Steam finish, verify files, and re-run. If it persists, capture to a new folder (`M baseline --out FILE`) and diff by hand. The check never overwrites a baseline |
| `addons search path MISSING` | The update (or Steam "verify files") reset `gameinfo.gi`. The paks are still in `addons\` but the game no longer mounts them: the skins silently vanish | `M install <any one of our paks>`: it re-applies the path (idempotent) and backs up the new stock file. Re-run `patchday` |
| `<pak> is now <other pak> (the same bytes; renamed by another tool ...)` | A mod manager renumbered `addons`. Not a problem: the pak is checked under its new name | Nothing; the next install or uninstall writes the new name into the ledger |
| `<pak>: missing from addons` / `crc ... something else replaced our file` | Ledger row without its file (and no other pak holds its bytes), or a file that is not the one we installed (a mod manager, a verify pass) | Reinstall that pak from its release copy (`release` writes it to `<reference>\work\release\<addon>\` unless `--out` says otherwise); if another tool owns the slot, install ours under a free one with `--as` |
| `replaced files whose Valve original changed: n` (RE-PROVE) | Valve changed an original our pak replaces (here: clips). Our copy still loads and still wins, but it may lack a new bone, a fix or a retime the original got | In-game re-prove (step 6). If the clip set is the animation Valve is iterating on, expect to re-derive it |
| `game files the mod uses that changed` (RE-PROVE) | A skeleton, graph or model our files reference has a new CRC | Check the two lines below it: `skeleton bones removed or renamed` and `anim graph parameters removed or renamed`. With neither, re-prove in game only |
| `skeleton bones removed or renamed` (BROKEN) | A bone our clips and weights name is gone. `bone 'a' is now 'b'` means one rename | Rebuild the mod's rig against the new skeleton, keeping Valve's bone names ([reference/engine.md](reference/engine.md), Rig). The bone list differences come from `rig.tsv`; `M dump <pak01_dir.vpk> models/.../x.vnmskel_c` shows the full new list |
| `anim graph parameters removed or renamed` (warning) | A control parameter the graph exposed is gone. Not fatal to our files, but whatever drives it (an event, a clip selector) goes quiet | Look at the graph (`M dump <pak01_dir.vpk> animgraphs/animgraph2/hero/hero.vnmgraph+<hero>.vnmgraph_c`); re-prove the affected motion in game |
| `replaced files Valve removed` (BROKEN) | Our file replaces a path that no longer exists: it does nothing now, and the hero's new path is unreplaced | Find the new path (`M ls <pak01_dir.vpk> <prefix>`; `M footprint <hero>`), move the file to the new path, rebuild |
| `X is gone; n mod files point at it` (BROKEN) | A game file our files reference was removed or moved (grouped by target). Missing skeleton or material: error model, T-pose, pink | Same as above: find the new path, repoint the references (rebuild from source) |
| `contract breach(es)` (BROKEN) | A replaced `.vmdl_c` lost or changed a bone, attachment or hitbox Valve's current model has (lint M062 shows the same) | Rebuild the model against the new Valve model; do not weaken the rule |
| `model contract ... not checked` | `--to` is not the installed build: Valve's models are not available | Informational; only on synthetic or historical checks |
| `target model ...: not in build N` | `mod.toml [target]` names a model or skeleton Valve moved | Same as "Valve removed" |
| `magician scene check <patch>` exits 1 (and `scene compose` / `install` refuse, exit 3): "A game update changed Valve's map" | A Valve file a hero-page scene patch keeps or replaces changed in the update, so the composer refuses (by design) | Rebuild that mod's showcase scene against the new map, run `magician scene make <scene.vpk> --out <patch dir>`, re-prove the hero page in game, rebuild the bundle |
| `hero data: <hero> was edited since N; none of the n names a skin leans on moved` | Valve edited the hero's block in `heroes.vdata` (numbers, a table), and every file, ability, sound event and particle it names is the same | Nothing beyond the re-prove. A balance patch reads like this |
| `hero data: <hero> since N: n name(s) gone, n new`, with `-` and `+` lines | The hero's block names something else now: a moved model, a renamed ability or sound event, a new particle | `M footprint <hero>` for the full picture; a `-` line the mod replaces is a file that no longer does anything. Compare with `mod.toml` targets and the kit docs |
| `hero data: none on record for build N` | The older build's hero data was never kept, so there is no "before" | Compare by hand with `M footprint <hero>`. `patchday --capture` keeps it from then on |
| `<file> (<owner>): crc X is not the bytes proven on N` (RE-PROVE) | That file was rebuilt or replaced since the proof: the combination now installed was never seen in game | The in-game pass on the installed bytes, then `M prove --evidence <pass>` |
| `N (claimed for other bytes or never recorded)` | `[proven] builds` lists N, and no proof binds N to the bytes installed: a pak proven before a rebuild, or a build recorded before proofs were kept | Nothing to fix; it counts for nothing. Record a proof of the bytes that were really seen on N, if the evidence exists: `M prove <mod.toml> --build N --date D --evidence <that pass> [--pak <release copy>]` |
| `proofs.tsv: line N: ...` / `proof X of build N has lost a row` | A hand merge left a line that does not read, or dropped a row of a proof | Fix the line by hand (rows stand alone: keep both sides of a conflict). `prove` refuses to rewrite the file until it reads |

Exit code 1 means at least one BROKEN line. RE-PROVE is exit 4: the mod loads, and the proof is in game. Exit 0 is clear.

When something is BROKEN, uninstall that mod while you fix it (next section) so players are not left with a broken hero. If other pipelines share the machine, check `python -I ingame\gamelock.py status` first.

## Re-proving in game, quickly

One command plays every mod that needs it and writes one report:

```
python -I ingame\reprove.py --plan       # what would run, and what cannot; launches nothing
python -I ingame\reprove.py              # the mods `patchday` says to re-prove (or --mod <folder> for each one you name)
```

It runs the patch-day check itself first (`patchday --capture --json`, so a build seen for the first time gets its baseline, and nothing is launched while something is BROKEN). Then, one mod after another, holding the game seat only while it plays:

1. a vanilla pass, when this build has no vanilla log for the mod's hero and map yet: the mod's paks out, the same scenarios, the log saved as `vanilla_<build>_<hero>_<map>.log`, the paks put back exactly (`install --status` is compared with before; a pass that cannot put them back stops the whole run);
2. the mod's pass: the `[ingame] pass` list of its `mod.toml`, into `<reference>\work\passes\<slug>\<build>-<date>-<time>\` with contact sheets, crash dumps and `index.md`;
3. the pass's console warnings against the vanilla log (`logdiff.py`).

The report (`reprove-<build>-<date>-<time>.md`, and `.json`, schema `magician.reprove/1`, in the passes folder) gives each mod a state:

| State | Means | Exit code |
|---|---|---|
| `ready` | every scenario ran and the mod adds no warning over vanilla on this build | 0 when every mod is |
| `look` | it ran, and it adds warnings or they could not be compared; the report lists them | 4 |
| `failed` | a scenario failed (a frozen hero is code 8), the game crashed, the session could not run, or the mod has no `[ingame] pass` | 1 |

None of the three proves a mod. The pictures are not judged (below), so someone looks at the contact sheets: a person, or an agent that can read images. Patch day changes animation clips and graphs first, so look at motion first: cloth following the new clips with no snap, no T-pose or error model, ability poses that still read. Compile-clean is not proof, and neither is `ready`.

Then record it:

```
python -I ingame\reprove.py accept <pass folder>
```

`accept` refuses a vanilla pass, a pass with a failed scenario, a pass filmed on another build, and a pass whose paks are not the ones installed now. Otherwise it runs `magician prove <mod.toml> --evidence <pass folder>` for the pass's mod, which records the proof against the bytes of the paks installed now, and leaves an `ACCEPTED` note in the folder. Commit the mod's `mod.toml` and `proofs.tsv` afterwards.

About 5 minutes of game time for a pass of four scenarios; a mod's first pass on a build costs two (the vanilla one). The pieces can be run by hand: `python -I ingame\mod_session.py qa --mod <mod folder> --out <dir> [--vanilla --restore]`, `python -I ingame\logdiff.py --build N --scene <hero>_<map> --mod <console log>`. A scenario is looked up in the mod's own folders first, then in `ingame\scenarios` (the stock-hero examples `enter_haze.txt`, `angles.txt`, `motion.txt`).

What a pass does not cover: the hero-page scene is a map archive, re-proved by hovering the hero in the menu (the first hover can draw nothing for about 8 s; give it one warm-up hover). When drift touched geometry, a rig or hero data, run every scenario the mod keeps, not only its pass list.

### Why stills are not compared

Comparing a pass's stills with the last accepted pass's was built and measured on build 6866 (2026-11-24) and taken out again. Each 1920x1080 still was cut into a 32 by 18 grid and a cell counted as changed when it differed by more than 12 grey levels. Two passes of the same pak, minutes apart, differed in 3 to 13% of the cells of the same still. The skin against vanilla Lash, the largest change a pass can show, differed in 3 to 17%. Comparing each cell's average colour instead narrowed the first and not enough: cells over 30 levels apart were 0 to 2.1% between identical passes and 1.9 to 8.5% between skin and vanilla.

The scenarios fix the camera and the hero's place; they do not fix the moment. The idle sway moves every high-contrast edge of the hero by a few pixels, and the neon signs, the sky and the embers never hold still. No threshold separates that from a real change, and a comparison that passes a broken material would be worse than none. It would take scenarios that freeze the hero at a known frame of a known clip to make stills comparable; until then the look is a person's or an agent's.

## Rolling back

Rollback is removing our paks. Nothing else in the game folder is ours.

```
M install --status                    what the ledger holds
M uninstall --mod maid_lash           one mod (and its scene: --mod lash_showcase)
M uninstall --mod example_skin        owners are whatever --mod named at install; `install --status` lists them
M uninstall --all                     everything in the ledger
M uninstall pak10_dir.vpk             a single file
```

Only ledger files are removed; a pak someone else put in `addons\` stays. The commands refuse while `deadlock.exe` runs unless you hold the game lock (`--ignore-game` overrides; do not).

`gameinfo.gi` is left as it is: with no paks the `Game citadel/addons` mount points at an empty folder and costs nothing. To restore the stock file anyway: `game\citadel\gameinfo.gi.magician-backup-<build>` is the stock `gameinfo.gi` of that build, taken once, before our first edit, so it contains no addons line. Copy it over `gameinfo.gi` **only the one for the build that is installed now** (`steam.inf` `ClientVersion`). Never restore an older build's backup: Valve edits this file between builds (the 6861 and 6864 backups differ in `sv_minrate` and the net `min` rate). Steam "verify files" does the same restore for free. After a restore, reinstall means re-running `M install`, which re-applies the line.

## When the game updates mid-session

What the toolkit guarantees (tested in `install.rs`, scratch copies only):

- **A running game blocks install and uninstall.** `seat_check` refuses while `deadlock.exe` runs, unless the caller holds the game lock (`MAGICIAN_GAME_HOLDER` or `--holder` equal to the lock's holder). Otherwise the message names the holder, or says someone is playing. `--ignore-game` overrides it. So a patch cannot be answered by swapping paks under a game someone is capturing.
- **The build is read from `steam.inf` on every command.** An install after the update lints against the new build (its cached baseline, or the live game if no baseline was captured yet) and takes its own backup. Lint at install time catches Valve removing a file we replace (M041), but only if you pass `--built-against <old baseline>`; `patchday` is the check that always compares eras.
- **`gameinfo.gi` is repaired by the next install, not by itself.** The update replaces it with the new stock file and drops our line; the paks stay in `addons\`. `install --status` and `patchday` say `MISSING (Steam reset gameinfo.gi?)`. The next `install` re-applies the line (idempotent) and writes `gameinfo.gi.magician-backup-<new build>` from the new stock file. The old build's backup stays.
- **Backups are always stock.** One backup per build, written only when the path is missing, so a backup never contains our line.

What Steam does (observed behaviour, not tested here): it does not replace files under a running game and applies the update after it quits (possibly with "update required" on next launch). A game running during the update keeps the old build's bytes in memory; its `steam.inf` on disk may already say the new build, so the harness's build number can disagree with the running game until a restart. After any update: `stop_game.ps1`, run `patchday`, then launch.

The harness has one more guard: `launch.ps1` refuses (exit 4) to adopt a game it did not start, because it may carry other paks; run `stop_game.ps1`, check `install --status`, launch again.

## What the drill showed (2026-11-21)

Real: 6861 to 6864 on all six installed files. Both skins load; `maid_lash` flags 45 changed original clips and one changed graph (`hero.vnmgraph+lash`), a second skin 23 clips and one graph. Scene, pose and voice paks: no drift. No bone or parameter detail for that real patch: the 6861 rig sidecar does not exist (rig sidecars start with 6864).

Synthetic (altered baselines under a scratch reference folder, real files untouched):

| Altered build | Result |
|---|---|
| CRCs changed on one Lash clip, one clip of a second skin, `heroes.vdata` | RE-PROVE each (exit 0 at the time; RE-PROVE exits 4 now) |
| A clip removed | `replaced files Valve removed: 1`, BROKEN, exit 1 (lint M041 says the same: "existed in 6864, gone in 6865") |
| `lash.vnmskel` removed | `lash.vnmskel_c is gone; 95 mod files point at it`, BROKEN |
| Bone `spine_1` renamed in Lash's skeleton; `hand_L`, `hand_R` dropped from a second hero's; `crouch_fraction` dropped from both graphs | `bone 'spine_1' is now 'spine_1_renamed'`, `bone 'hand_L' removed or renamed`, graph parameter warnings; BROKEN, exit 1 |
| Both skins' `.vmdl_c` removed | `replaced files Valve removed` per skin, BROKEN |

Gotchas found:

- **`lint --baseline <fake>` wrote `particle_classes.tsv` into the real reference folder under the fake build number.** Any drill must pass `--reference <scratch>` (now supported on every command). Remove a stray `<reference>\<fake build>` folder if you ever see one.
- `drift` before this drill only read the default reference folder and only compared CRCs of rig files; it now takes `--reference` and names changed bones and parameters once both builds have a `rig.tsv`.
- Not checkable offline: an AnimGraph parameter our clips are wired to via events (our paks carry no graph); what a changed clip does to the cloth. Those are what the in-game pass is for.

## Time

Clean patch (RE-PROVE only, like 6861 to 6864): capture and check 1 minute; launch and both skins' minimum passes 30 to 40 minutes including the game starting (about 3 minutes) and reading the sheets; `prove` and commits 5 minutes. About 45 to 60 minutes for two skins, almost all in-game. A rebuild after a bone rename or moved path: add the mod's `build` or `release` plus a full pass, hours, and the creative work is the cost, not the toolkit.
