# Magician loops in full

Load the section you need. `magician` is `target/release/magician` in a checkout (`magician.exe` on Windows). Run a mod's commands from its folder, and `patchday` and `reprove` from the folder that holds the mods (or from inside any one of them).

## Contents

- [A headless run, start to finish](#a-headless-run-start-to-finish)
- [Validate a mod](#validate-a-mod)
- [Patch day](#patch-day)
- [Build and release](#build-and-release)
- [Install and roll back](#install-and-roll-back)
- [Install a pak for someone to look at](#install-a-pak-for-someone-to-look-at)
- [Hero-page scenes](#hero-page-scenes)
- [Release bundles](#release-bundles)
- [In-game proof](#in-game-proof)

## A headless run, start to finish

The prompt is one line, say *"validate the lash skins for the newest Deadlock release"*, and nobody will answer questions. Do this:

1. `magician doctor --json`. A check whose `state` is `fail` (most often `game`): report its `detail` and `fix`, and stop. The `build` check names the installed build; "newest release" means the build Steam has installed (Magician never downloads one).
2. Find the mods. "The lash skins" means every mod whose hero is Lash: of the mods `patchday` checks (the `manifests` of its JSON; `magician paths` shows a `mods` folder when one is set), the ones whose `mod.toml` has `[mod] hero = "hero_lash"` (or `lash`, `Lash`: Magician reads a hero any way), and `magician install --status` for what is installed and under which owner. If nothing matches, say what you searched.
3. `magician patchday --capture --json`. It writes the new build's baseline and its rig and engine sidecars into the reference folder if they are missing (never into the game), then checks every installed mod: `mods[].state`, `mods[].from` (the build each was compared from), `engine` (data only, or the files that changed) and `verdict`. If it says there is no older baseline on record, drift could not be checked: say so.
4. For each mod in scope, `magician check <its folder> --json`: its paks (from its `mod.toml`: `[mod] paks`, else the paks beside it, else the ones the ledger holds for its owners, which is what the game loads) linted deep and graded together, its drift since its last proven build, its scene patches against the stock maps. Report `verdict` (`ok`, `re-prove`, `broken`), `label`, the `lint` step's counts and every `next` line. stderr names every file it used and where it found it. If a `next` line says the ledger holds files for owners the mod.toml does not claim (a voice pak, a scene), report it: the grade left them out, and only the author can say they belong (`[mod] owners`).
5. For anything broken, `magician drift --json` from inside the mod's folder (it compares from the last proven build) or `magician lint <pak>` for the full findings. Do not uninstall unless the prompt or a standing instruction says to contain broken mods.
6. Stop before anything that needs judgement or the game: no fixes, no installs, no proofs, no launch. If the prompt does ask for in-game proof, that is [In-game proof](#in-game-proof), and its last step still needs the frames looked at.
7. Report (format in SKILL.md): build, each mod's verdict and label, lint counts with rule codes, every write you made (a captured baseline), what remains (in-game re-prove for `re-prove` mods, a fix for broken ones).

With a person pairing, do the same but ask before anything that writes outside the reference folder.

## Validate a mod

```
magician check <mod folder or mod.toml> [--light] [--json]   the one verdict: lint + grade + drift + scenes
magician lint <pak>                          light
magician lint <pak> --deep                   plus the mesh family (M100-M148)
magician lint <pak> --json                  one JSON document on stdout, same findings (--json-out FILE writes it beside the report)
magician rules                               the catalogue: code, name, severity, in-game consequence
```

Inside a mod's folder every command here can drop its pak (and `grade` its `--mod`): the mod.toml supplies them, and stderr says which files were used. Explicit arguments win.

A finding line reads `<severity>  <code> <name>  <path>`, with a hint indented under it, and the run ends with `N error(s), N warning(s), N note(s), N allowed, N stale`. Exit 1 means at least one error that is not allowed.

- Findings are judged against Valve's file at the same path; what Valve's own file does is never a finding. A mod is blamed only for references it introduced.
- **Allowances** live in the addon's `magician.allow` (one site, one rule, a written reason). Allowed findings still print, as `allowed`. `stale` means an allowance no longer matches anything: report it, the author should remove it.
- `M129` (egregious hitbox overflow) and other fair-play findings are not style notes. They mean the mod makes a hero harder to read or hit fairly. `M144` means a fair-play comparison could not run on a file: fair play is unknown there, not clean. Fair play is lenient unless the policy enforces it (`[fair_play] enforce = true`): until then these print as warnings, lint exits 0 on them alone and install goes ahead. Report them anyway, with their codes; a warning here is not a clean bill.
- Without a users index for the build, or with one from an older Magician, the shared-file rule M090 does not run and lint says so as an M091 note in the findings. `magician index` builds it (needs the Source 2 Viewer CLI).
- One model in detail: `magician mesh <pak> <model path>` (verdict PASS / ATTENTION / FAIL, silhouettes, an x-ray sheet in `--out`), `magician cloth <pak> <model path>` (the compiled cloth beside Valve's).

**Grade** (hero mods; a mod that touches nothing of the hero is "not graded", and says so):

```
magician grade <pak>... --mod <mod.toml> [--allow <magician.allow>]... [--json] [--gate]   # without --allow it finds the mod's own magician.allow
magician grade <pak> --hero <hero> [--light | --findings <file>]   # the file a lint --findings-out wrote
```

Give every pak of the mod at once (a skin pak and its voice pak grade as one; alone, the skin pak lacks the voice). The label reads `<Kind> (<groups>)[ · broken][ · freeform | · fair play not checked][ · stale(N) | · unproven]`:

- the kind is identity: Echo (a new character), Costume (the same character, a new model), Variant (Valve's mesh, new materials), Kit (no model);
- the groups are what the mod makes its own: look (model, materials, UI art, scenes), effects (ability VFX, animation), sound (kit sound, and voice once treated); `some <group>` when begun, left out while Valve's. It describes the mod, it does not rank it: don't present it as a quality verdict or a score. The report's three lines under the label say what was counted;
- `broken` is lint errors that are not fair play, the same word `check` uses;
- `freeform` means a measurement went past a fair-play limit (the skin took liberties with how the hero reads; a skin inside every limit is ranked-safe and has no flag), `fair play not checked` is a check that could not run (a `--light` lint over a model reads `fair play not checked`: grade lints deep by default), and neither ever changes the description;
- `stale(N)` means these paks are not proven on this build, last proven on N; `unproven`, no proof holds these bytes (a rebuilt pak, whatever `[proven] builds` lists; the record's `freshness.unbound` names the builds claimed for other bytes).

In the record (`--json`, schema `magician.grade/3`) the useful parts are `description.groups` (each group's state and what was counted), `description.notes`, `release_gate.blockers` (what stops publishing: lint errors that are not fair play, Valve's own files, not proven, rights denied; and fair play not checked or freeform, and unreviewed rights, when the policy enforces them; otherwise those are in `release_gate.warnings`), `allowance_files` (which `magician.allow` the grade read, or that none was found: an unexplained freeform is often a missing one), `fair_play.violations` and `fair_play.allowances` (each with its measured value and limit), `identity.evidence` and `next_steps` (they name files). `rights: unreviewed` is a person's call: never set `[rights] state` yourself. `--gate` exits 1 when the gate blocks.

## Patch day

```
magician patchday --capture           the check; writes a missing baseline and its sidecars
magician patchday --from <build>      compare against a specific older build
magician patchday --mod <mod.toml>    check named mods (otherwise the mods folder, or the mods found from the working folder)
magician drift <pak>... --from <old> [--to <new>]
```

Sections: `[1] baseline` (MISSING, ok, or STALE: same build number, different bytes, a hotfix), `[2] install state` (is `Game citadel/addons` still in `gameinfo.gi`; ledger rows against disk), `[3] drift` per mod from the last build a proof holds its bytes on, with an `engine` line (`data only`, or `engine too` and the files), `[4] mods` (`[target]` paths, `its installed bytes proven on N of M builds`, and whether any name in the hero's data moved), then `verdict`: `clear` (exit 0), `loads, but re-prove in game: <mods>` (exit 4), or `BROKEN: n problem(s)` (exit 1). `--json` gives all of it as one document.

What to do with each, briefly (the full table is `docs/PATCH-DAY.md`, "What each failure means"):

| You see | It means | Your move |
|---|---|---|
| `addons search path MISSING` | Steam reset `gameinfo.gi`; mods silently don't load | Report. The fix is `magician install` of any installed pak (idempotent), which is a write: only if asked |
| `RE-PROVE` | Loads; Valve changed something the mod replaces or uses | Needs an in-game pass. Headless: report as pending |
| `bone 'a' is now 'b'`, `skeleton bones removed` | BROKEN: the mod's rig names a bone Valve renamed or dropped | Report the bone names; the rig must be rebuilt by the author |
| `replaced files Valve removed`, `X is gone; n mod files point at it` | BROKEN: a path moved | Report; `magician ls <game pak> <prefix>` can find the new path |
| `contract breach(es)` | BROKEN: a replaced model lost a bone, attachment or hitbox Valve's has (M062) | Report; never weaken the rule |
| `STALE` | Baseline differs from the game under the same build number | Let Steam finish; re-run. Never overwrite a baseline by hand |

A pak whose owner no `mod.toml` claims (a voice pak, a showcase scene installed under its own name) is compared from the newest older baseline, and its line says so; a `mod.toml` claims such owners with `[mod] owners = [...]`.

## Build and release

Needs the CSDK (`magician paths` shows `csdk`). Addon sources live in `<csdk>/content/citadel_addons/<addon>/`.

```
magician build <addon> <out/pakNN_dir.vpk>        compile real sources, pack outputs that map to live sources, lint
magician release <addon> --hero <hero> [--with <addon>]... [--out FILE]
```

- One build per addon at a time: a second waits for the lock; `--no-wait` fails at once instead (use it when unattended).
- A text source with a UTF-8 BOM is refused by name; the fix is to rewrite the file without one, not to bypass the check.
- `magician.exclude` in the addon keeps listed paths out of the pak (when another pak owns them).
- `release` writes `release-report.txt`, `lint.json` and `grade.json` beside the pak (lint, grade, mark; `--mod <mod.toml>` gives the grade its proof and rights) and exits 1 if lint failed. `--json` prints the whole release as one document. Default output is `<reference>/work/release/<addon>/pak10_dir.vpk`.

## Install and roll back

```
magician install --status
magician install <pak> [--mod NAME] [--as pakNN_dir.vpk]
magician uninstall <file> | --mod NAME | --all
```

- Install lints first and refuses (exit 3) on lint errors.
- It refuses while `deadlock.exe` runs unless you hold the game lock (`--holder NAME` or `MAGICIAN_GAME_HOLDER` equal to the lock's holder).
- It never replaces another mod's file, a file it didn't install, or one changed since; the message names the owner and a free slot for `--as`. Don't reach for `--force`.
- It backs up `gameinfo.gi` once per build and adds the addons line idempotently. Uninstall removes only ledger files; a pak someone else put in `addons/` stays.
- After installing, `install --status` should show the crc you meant. Deadlock Mod Manager renumbers paks it didn't install; Magician follows a moved pak by its crc and `install --status` says `(the ledger says pakNN_dir.vpk: renamed by another tool)`. Report the move. An update of a moved pak is refused with the `--as` that replaces it in place: use that rather than installing a second copy.
- A running Source 2 Viewer MCP session keeps the addon paks mapped and install fails with `os error 1224`; stop the viewer (`python tools/s2v_mcp.py stop`) first.

## Install a pak for someone to look at

The person says "I rebuilt my pak, check it and put it in the game so I can look."

1. Lint it (light, and `--deep` if it replaces a model). Errors: report them and stop; install would refuse anyway.
2. `magician install --status`: is a version of this mod installed, under which owner and file name?
3. `python -I ingame/gamelock.py status` and whether `deadlock.exe` is running. A running game you don't hold: ask them to close it (a running game holds the paks and install refuses).
4. `magician install <pak> --mod <the owner from step 2>` (same file name, so it replaces their previous build of the same mod). If it refuses because another mod owns that slot, report the free slot it suggests and ask before using `--as`.
5. `magician install --status` again: the crc should be the new pak's.
6. Tell them it is installed and that they can launch. Don't launch for them unless asked; if asked, follow [In-game proof](#in-game-proof) and give the seat back when they're done looking.

## Hero-page scenes

```
magician scene check <patch dir>                          read-only: does this game's stock map match the patch?
magician scene compose <patch dir> --out <file.vpk>       build it somewhere, install nothing
magician scene install <patch dir> [--mod NAME]           compose and install (same rules as install)
magician scene make <scene.vpk> --out <patch dir>         author side: split a built scene into a patch
```

After a game update, `check` exiting 1 (and `compose` or `install` refusing with 3) with "A game update changed Valve's map" is the design working: the patch must be re-made against the new map with `scene make` and the hero page re-proven. Report it per hero.

## Release bundles

```
magician bundle <bundle.json> --out <dir> [--zip]
magician source <source.json> [--out <dir>]
magician publish nexus <bundle.json> --dry-run        what would go to Nexus Mods; sends nothing
```

`bundle` builds a Deadlock Mod Manager bundle (`mod-interchange.json`, paks, previews, scene patches with standalone installers, `SHA256SUMS.txt`). It refuses a path shipped in two paks and any file byte-identical to Valve's. `source` writes a static mod source from bundles: the catalogue the manager reads, a page per mod, a release feed. It states only the builds a proof holds the zip's own paks on. Write to a scratch `--out` unless told where releases go; never overwrite a published bundle or site, and never push one: publishing is the user's step. `publish nexus` without `--dry-run` uploads to Nexus Mods: run it only when the operator says to publish that release, after showing them the dry run; exit 3 is the release gate refusing, and is reported, not overridden.

## In-game proof

Only when the task asks for it, the machine has a desktop the game is shown on (an interactive Windows session, or an X display on Linux; `python ingame/gameos.py` says whether this one has what the harness needs), and you can look at the frames (or a person will). The harness types into the game like a player and captures its window; `ingame/README.md` has the details.

After a game update, for every mod that needs it:

```
magician reprove --plan                  what would run, and what cannot; launches nothing
magician reprove                         the patch-day check, then each mod's passes, one after another
magician reprove accept <pass folder>    record a pass that was looked at
```

- It takes the game seat under each mod's name only while that mod plays (`MAGICIAN_GAME_HOLDER` names one holder for the whole run), and exits 3 at once if someone else holds it.
- A mod's first pass on a build is two: a vanilla pass (the mod's paks out, the same scenarios, the paks put back exactly) gives the console log every later pass on that build is read against. A vanilla pass that cannot put the install back stops the whole run.
- The report (`reprove-<build>-<date>-<time>.md` under `<reference>/work/passes/`, and `--json`) gives each mod `ready` (every scenario ran, no warning over vanilla), `look` (it adds warnings, or they could not be compared: listed) or `failed` (a scenario failed, a crash dump, no `[ingame] pass` in its `mod.toml`). Exit 0, 4, 1.
- None of those is proof. Open the pass's contact sheets (`*_sheet.png`, listed in its `index.md`). Say what you saw (a T-pose, a pink material, cloth snapping, an ability that reads wrong), not that the run exited 0. If you cannot see images, stop here and report the pass folders for a person.
- `accept` refuses a vanilla pass, a pass with a failed scenario, a pass on another build, and a pass whose paks are not the ones installed now. Otherwise it records the proof (`magician prove <mod.toml> --evidence <pass folder>`). The mod's `mod.toml` and `proofs.tsv` change: say so, and commit them only if that is the task.

One mod, or a pak that is not installed yet:

```
magician session qa --mod <mod folder> --out <dir> [scenario ...]        the mod's [ingame] pass, or the scenarios named
magician session qa --mod <mod folder> --out <dir> --pak <pak> --restore  a candidate pak over the mod's slot, the old one back at the end
magician session up | run | film | down --out <dir> ...                    the same session in parts, steered between
```

- A session exits 5 when a game it did not start is running (it may carry other paks): don't adopt it, report. It exits 9 when the seat or the game stopped being its own: nothing was done.
- `python -I ingame/gamelock.py status` says whether the seat is free (0) or held (3). Check it before saying the game is free.
