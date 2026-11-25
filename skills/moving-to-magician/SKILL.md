---
name: moving-to-magician
description: Move an existing Deadlock mod, or a whole folder of them, that was made without Magician onto Magician, then validate it until Magician has a clear verdict on it. Use when someone points you at a pak, a VPK, a zip or 7z from a mod site, an addon's sources, their game's addons folder or "all my mods" and wants them checked, managed, installed or kept current with Magician from now on. Works for any kind of mod (hero skin, voice, HUD, sounds, particles, a world prop, a hero-page scene). The aim is the same mod, byte for byte, with nothing lost; a person still looks at it in game at the end.
---

# Moving a mod to Magician

A mod is "on Magician" when it is a folder with a `mod.toml`, `magician check` gives it a verdict on the installed build, and (if it is installed) it went into the game through Magician's ledger. That is all a move is. It does not change how the mod looks, and it does not fix it.

Read [using-magician](../using-magician/SKILL.md) first. Its rules apply here in full: read a command's usage instead of guessing a flag, only Magician writes into the game folder, one user of the game at a time, never weaken a check, never record a proof nobody looked at. This page adds only what is particular to a move.

## What you promise

1. **Nothing of theirs is moved, edited or deleted.** You copy. The original pak, download, sources and whatever sits in the game's addons folder stay exactly where and as they were, until the person removes them.
2. **The same bytes.** A mod moved as it is must be the identical file afterwards, and you show that with its sha256. A mod rebuilt from sources is the same only if every file matches, and where one does not you say which and why. An unexplained difference is never called parity.
3. **Findings are reported, not fixed.** A mod made without Magician was never linted. It will usually have warnings and may have errors. Those were true before you arrived. Say what they are; change nothing unless asked.
4. **A person looks at the end.** Clean output is not proof that it looks right.

## 1. Find out what you were given

```
magician doctor --json        # the game must be found; if `game` fails, stop and say so
magician install --status     # what Magician already installed, and under which names
magician paths                # where the game, the mods folder and the CSDK are on this machine
```

Then list what there is to move, before touching anything. A mod is what one author published as one thing: one download, or the files one release page offers together (a skin and its voice pak, a model and its particles). Tell the person the list in a few lines and carry on; ask only when two candidates could be one mod or one could be two.

| What you find | How it moves |
|---|---|
| A `.zip`, `.7z` or `.rar` from a mod site | `magician init <archive>`: it takes the paks out |
| One or more `.vpk` files, `pakNN_dir.vpk` or any other name | `magician init <pak>... --out <folder>`: it copies them in |
| A pak in pieces (`pak01_dir.vpk`, `pak01_000.vpk`, ...) | The same; give it the `_dir.vpk` and the pieces come with it |
| `maps/ui/hero_prefabs/<hero>.vpk` | A hero-page scene: alongside the mod it belongs to |
| A folder of loose compiled files (`models/...`, `particles/...`, ending in `_c`) | `magician pack <folder> <scratch>/pak01_dir.vpk`, then as a pak; say that you packed it |
| A folder under the CSDK's `content/citadel_addons/` | An addon's sources: `magician init <addon folder>`; rebuilt only if asked (step 5) |
| Files in the game's `citadel/addons/` that `install --status` does not list | Installed by hand or by a manager: copy each out with `init --out`; see step 4 before installing |

"All my mods" means: every candidate under the folder they named, plus the unlisted paks in the game's addons folder if they said the installed ones count. If Magician cannot read a file as a VPK, that mod is not moved: report the message and go on to the next.

## 2. Make its folder and describe it

One mod, one folder, named for the mod: the folder's name becomes its slug and display name. `init` makes the folder, copies the paks into it and writes the `mod.toml`:

```
magician init "<download>.zip" --out <mods folder>/<mod name>       # a download: only its paks come out
magician init <pak> <other pak>... --out <mods folder>/<mod name>   # paks from anywhere, one mod
magician init                                                       # in a folder that already holds the paks
```

The mods folder is the `mods` row of `magician paths`, else the folder the person pointed you at, else ask where their mods should live. Never make folders inside the game install. Without `--out`, a download gets a new folder beside it, named for it. Leave the pak names as they are: a `pakNN_dir.vpk` number decides which mod wins a shared file (the lower wins), and a pak with another name is given a slot when it is installed.

Read every line `init` prints:

- **`pak    <name>  N files, sha256 ...`**, with **`(copied from <original>, the same bytes)`** or **`(from <download>)`**: the copy is proven identical, and that sha256 is the mod's fingerprint for your report. To show it again later, `magician diff <original> <copy>` prints `the same file` and exits 0.
- **`from   <download>: ...; left in it: ...`**: what else was in the archive (a readme, a preview). It never belongs in the folder.
- **`left out <pak>: all N of its files are in <other>, byte for byte, so it adds nothing`**: the download held an older part of the same mod (a particles pak that a later full release includes). Say so; nothing is lost.
- **`note   <a> and <b> both ship N file(s) with different bytes`**: two paks disagree on a file, and only one loads. Tell the person; which they meant is theirs to say.
- **`hero   hero_lash (Lash): 644 of 644 files are its own`**: decided from the files. Good.
- **`also   1 file(s) of hero_atlas (Abrams): soundevents/hero/abrams.vsndevts_c; the mod changes that hero too`**: the mod ships a file of another hero's. Usually a leftover of the author's; worth a sentence in the report, since installing the mod changes that hero as well.
- **`hero   none: no file is one of a hero's, so a prop mod (models/...)`**, or an effects, sound, HUD, map or music mod: correct, and needs nothing. It is linted and checked like any other; it is `not graded`, which is a statement, not a failure.
- **`hero   not decided: ... (and ...): pass --hero`**: split between heroes. `--hero <name>` settles it, only when the files or the person make it plain.

`init` never writes over a `mod.toml` (exit 3) or over a different file of the same name; do not `--force` one you did not write. Pass `--slug` or `--name` if the folder name is poor. Leave `[rights] state = "unreviewed"`: whether the mod may be shipped is the person's call, and for someone else's mod it stays unreviewed.

## 3. Get the verdict

From the mod's folder:

```
magician check                    # the verdict: lint, grade, proofs, drift (--json for one document)
magician lint                     # every finding, ending with the count by rule
```

`check` prints the verdict, then under its lint line the findings by rule: errors, warnings, fair play, and fair play's advisories (reported, never a violation, so `fair play pass` above a row of them is right). `lint` prints a run of one rule on one file that differs only in a counting number as one line (`swing_long_01..25.vsnd_c' (25 findings)`), and an explanation several of a block's findings share once, marked `(*)`. Report by rule code in Magician's own words, a count per rule, and the few findings that matter most with what each does in game (`magician rules` says it for every code). The verdict will be one of:

- **`re-prove`** (exit 4). The normal result for a mod that loads: nothing proves it on this build yet. The move is done; what remains is someone looking at it in game.
- **`broken`** (exit 1). Lint errors, or files the current game build no longer has. These are findings about the mod as it already was. Do not add allowances, edit the pak or pick a different check to get a better word.
- **`ok`** (exit 0) only happens once a proof exists, so not on a first move.

Some findings are about the game, not about taste, and are worth a sentence to the author: a reference to a file neither the mod nor the current build has (M030: that sound or material is silent or missing now), a copy of the engine's own files (M071) or of another hero's (init's `also` line), attachments or hitboxes that are not Valve's (M062; `(*)` marks the ones that are Valve's turned by an exporter, one setting to fix), Valve's effects the mod's file no longer references (M061), files nothing plays (M031). A fair-play finding (`NSFR`, `FP?`) on someone's long-standing mod is worth one plain sentence: what the rule saw, and that Magician will refuse to install with lint errors but only warns on this unless they enforce it.

## 4. Put it in the game through the ledger

Only if the mod is meant to be installed. Check the seat and the running game first (using-magician, rule 3).

```
magician install                  # from the mod's folder: lints first, then installs under the mod's own name
```

What it may say, and what you do:

- **Installed.** `magician install --status` now shows it with the crc you expect.
- **`... is installed for another mod (...); install under a free slot with --as pakNN_dir.vpk`**: the number is taken by a mod Magician manages. Use the slot it names, and tell the person the mod's number changed, since the number is the load order.
- **`... exists and it wasn't installed by magician; pick another slot (--as ...) or --force`**: the person's own copy is sitting in that slot. This is the usual case when moving an installed mod. Do not install a second copy under another number: two copies of one mod is worse than either. Tell them the file is theirs and ask which they want: they remove their copy and you install, or, when `magician diff` says the file in the slot is the same file, they agree to `--force` for that one file so Magician takes it over in place. Ask once for the batch, list the files, and use `--force` on those and nothing else.
- **`not a pakNN_dir.vpk ...; rename it, or pass --as pakNN_dir.vpk`**: a pak with a free-form name. Pick a number no one uses (`install --status`, and the files in the addons folder) and pass `--as`.
- **`lint errors: not installing`**: the mod is broken by Magician's rules. That is step 3's report. Do not `--force` it in.

If the person installs this mod with a mod manager, leave installing to the manager. Everything else still works on the folder: `check`, `lint`, `grade` and `drift` read the paks beside the `mod.toml`. Say that Magician's patch-day check only follows paks its own ledger installed.

## 5. Sources: rebuild only if asked

A mod that has compilable sources can stay "as it is" forever: the pak is what ships. Move it onto Magician's build only when the person wants Magician to build it from now on, and `magician doctor` shows the CSDK is set up. The sources must already be an addon under the CSDK's `content/citadel_addons/<addon>/`; set `addon = "<addon>"` in the `mod.toml`.

```
magician release --out <scratch folder>/pakNN_dir.vpk     # from the mod's folder
magician diff <original pak> <scratch folder>/pakNN_dir.vpk
```

`diff` sorts every path: only in the old pak (no source left: stale or hand-added; list them, the person decides whether each mattered), only in the new one (a source the old build never shipped; list them), recompiled (only the compiler's record differs: the same contents, fine), and different (name each file and the blocks `diff` says differ; `magician dump` on both shows how).

The rebuilt pak is a new file. It is unproven whatever the old one was, and it is not the mod until the person has seen both in game and said so. Until then the original pak stays the one in the `mod.toml`.

## 6. The look in game

A mod moved as it is, with a matching sha256, will look as it did: it is the same file. An in-game pass is not needed to trust the move. It is needed for the first proof, and after a rebuild.

Run one only if the task includes it, the machine has a desktop, and the seat is free:

```
magician session qa --mod <folder> --out <dir> angles motion
```

`angles` and `motion` are stock scenarios that work for any hero. A mod with no hero has no stock scenario; for those, install it and tell the person what to look at (the prop, the HUD element, the sound, the effect). Then open the contact sheets. Say what you saw, or that you could not look. Record a proof only by the rule: frames looked at, the same paks still installed.

## 7. Report

One block per run, verdict first:

```
Deadlock build 6868. 3 mods moved to ~/mods, 1 not moved.
- cool-lash-skin: moved as it is from cool-lash-skin.zip (sha256 1f0c...; the download's older particles pak left out, every file of it in the main pak). RE-PROVE: loads, 0 errors, 350 warnings (M110 x200, M064 x150), Costume (look, some effects) · NSFR (M124 on the model). Installed as pak11 (pak10 is another mod's). Needs: a look in game.
- menu-music: moved as it is. A music mod, not graded. 0 errors. Your own copy is in addons/pak31_dir.vpk; not installed until you say which copy stays.
- soul-jar: moved as it is. A prop mod (models/props_gameplay/...), not graded. BROKEN: 3 x M071 (it ships its own copies of the engine's default masks). Not installed.
- old-skin.vpk: not moved: Magician could not read it as a VPK ("...").
Wrote: three folders under ~/mods, each with its paks and a mod.toml; one pak installed. Changed nothing else.
From now on: run `magician check` in a mod's folder, and `magician patchday` from ~/mods after a game update.
```

Always say: what was copied where; for each mod its sha256, the verdict, the error and warning counts with the rule codes that matter, and what it still needs from a person; every install and every refusal; what you did not move and why. If they have many mods in one place, mention that setting it as the mods folder (`magician paths` shows how it is resolved) lets `patchday` and `reprove` find them from anywhere.
