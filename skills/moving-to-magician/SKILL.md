---
name: moving-to-magician
description: Move an existing Deadlock mod, or a whole folder of them, that was made without Magician onto Magician, then validate it until Magician has a clear verdict on it. Use when someone points you at a pak, a VPK, a zip, an addon's sources, their game's addons folder or "all my mods" and wants them checked, managed, installed or kept current with Magician from now on. Works for any kind of mod (hero skin, voice, HUD, sounds, particles, a hero-page scene). The aim is the same mod, byte for byte, with nothing lost; a person still looks at it in game at the end.
---

# Moving a mod to Magician

A mod is "on Magician" when it is a folder with a `mod.toml`, `magician check` gives it a verdict on the installed build, and (if it is installed) it went into the game through Magician's ledger. That is all a move is. It does not change how the mod looks, and it does not fix it.

Read [using-magician](../using-magician/SKILL.md) first. Its rules apply here in full: read a command's usage instead of guessing a flag, only Magician writes into the game folder, one user of the game at a time, never weaken a check, never record a proof nobody looked at. This page adds only what is particular to a move.

## What you promise

1. **Nothing of theirs is moved, edited or deleted.** You copy. The original pak, zip, sources and whatever sits in the game's addons folder stay exactly where and as they were, until the person removes them.
2. **The same bytes.** A mod moved as it is must be the identical file afterwards, and you show that with its sha256. A mod rebuilt from sources is the same only if every file matches, and where one does not you say which and why. An unexplained difference is never called parity.
3. **Findings are reported, not fixed.** A mod made without Magician was never linted. It will usually have warnings and may have errors. Those were true before you arrived. Say what they are; change nothing unless asked.
4. **A person looks at the end.** Clean output is not proof that it looks right.

## 1. Find out what you were given

```
magician doctor --json        # the game must be found; if `game` fails, stop and say so
magician install --status     # what Magician already installed, and under which names
magician paths                # where the game, the mods folder and the CSDK are on this machine
```

Then list what there is to move, before touching anything. For each candidate, which of these is it?

| What you find | What it is | How it moves |
|---|---|---|
| A `pakNN_dir.vpk`, or any `.vpk`, alone or in a folder | A finished mod with no sources | As it is |
| A `.zip`, `.7z` or `.rar` from a mod site | The same, wrapped | Unpack a copy into the mod's folder, then as it is |
| Several paks that belong together (a skin and its voice pak) | One mod in several files | As it is, one folder, all paks in it |
| `maps/ui/hero_prefabs/<hero>.vpk` | A hero-page scene | As it is, alongside the mod it belongs to |
| A folder under the CSDK's `content/citadel_addons/` | An addon's sources | As it is first; rebuilt only if asked (step 5) |
| Files in the game's `citadel/addons/` that `install --status` does not list | Mods installed by hand or by a mod manager | Copy each out, as it is; see step 4 before installing |

"All my mods" means: every candidate under the folder they named, plus the unlisted paks in the game's addons folder if they said the installed ones count. Tell them the list in a few lines and carry on; ask only when two candidates could be one mod or one could be two.

`magician ls <vpk>` lists a pak's files (path, crc, size). Use it to tell what a pak is for and whether two paks overlap. If Magician cannot read a file as a VPK, that mod is not moved: report the message and go on to the next.

## 2. Give each mod a folder

One mod, one folder, named for the mod: the folder's name becomes its slug and display name. If the mod already has a folder of its own, use that folder; the move adds one file to it. A loose pak, or one copied out of the game, gets a new folder in the mods folder: the `mods` row of `magician paths`, else the folder the person pointed you at, else ask where their mods should live. Never make folders inside the game install.

Copy the pak in. Keep a `pakNN_dir.vpk` name as it is, since the number decides which mod wins a shared file (the lower number wins). A pak with another name can keep it; it will be given a slot when it is installed.

Before anything else, write down what the original is, so you can show later that nothing changed:

```
magician ls <original pak>        # every file in it: path, crc, size. Keep this output.
```

## 3. Describe it, then get the verdict

From the mod's folder:

```
magician init                     # every pakNN_dir.vpk in this folder
magician init "<file>.vpk"        # a pak with any other name
magician check --json             # the verdict: lint, grade, proofs, drift
```

`init` writes the `mod.toml` and says what it decided and from what. Read those lines:

- **`hero  hero_lash (Lash): 644 of 644 files are its own`**: decided from the files. Good.
- **`hero  not decided: ...`**: either the mod is not for one hero (a HUD, sound or particle mod), which is correct and needs nothing, or it is split between heroes and `--hero <name>` settles it. Pass `--hero` only when the files or the person make it plain. A mod with no hero is linted and tracked like any other; it is `not graded`, and that is a statement, not a failure.
- The slug and name come from the folder. Pass `--slug` or `--name` if the folder name is poor. `init` never writes over a `mod.toml` (exit 3); do not `--force` one you did not write.

Leave `[rights] state = "unreviewed"`. Whether the mod may be shipped is the person's call, and for someone else's mod it stays unreviewed.

Now check that it is the same mod. `magician ls` on the pak in the folder must print exactly what it printed for the original: the same paths, crcs and sizes. If it does, the move is exact, and you can say so. `check --json` also gives each pak's `sha256` in its `proof` step; put that in your report as the mod's fingerprint.

The verdict will be one of:

- **`re-prove`** (exit 4). The normal result for a mod that loads: nothing proves it on this build yet. The move is done; what remains is someone looking at it in game.
- **`broken`** (exit 1). Lint errors, or files the current game build no longer has. `magician lint` lists every finding with what it does in game. These are findings about the mod as it already was. Report them by rule code in Magician's own words. Do not add allowances, edit the pak or pick a different check to get a better word.
- **`ok`** (exit 0) only happens once a proof exists, so not on a first move.

A fair-play finding (`NSFR`, `FP?`) on someone's long-standing mod is worth one plain sentence: what the rule saw, and that Magician will refuse to install with lint errors but only warns on this unless they enforce it.

## 4. Put it in the game through the ledger

Only if the mod is meant to be installed. Check the seat and the running game first (using-magician, rule 3).

```
magician install                  # from the mod's folder: lints, then installs under the mod's own name
```

What it may say, and what you do:

- **Installed.** `magician install --status` now shows it with the crc you expect.
- **`... is installed for another mod (...); install under a free slot with --as pakNN_dir.vpk`**: the number is taken by a mod Magician manages. Use the slot it names, and tell the person the mod's number changed, since the number is the load order.
- **`... exists and it wasn't installed by magician; pick another slot (--as ...) or --force`**: the person's own copy is sitting in that slot. This is the usual case when moving an installed mod. Do not install a second copy under another number: two copies of one mod is worse than either. Tell them the file is theirs and ask which they want: they remove their copy and you install, or, when `magician ls` shows the file in the slot is the same pak (same crcs), they agree to `--force` for that one file so Magician takes it over in place. Ask once for the batch, list the files, and use `--force` on those and nothing else.
- **`not a pakNN_dir.vpk ...; rename it, or pass --as pakNN_dir.vpk`**: a pak with a free-form name. Pick a number no one uses (`install --status`, and the files in the addons folder) and pass `--as`.
- **`lint errors: not installing`**: the mod is broken by Magician's rules. That is step 3's report. Do not `--force` it in.

If the person installs this mod with a mod manager, leave installing to the manager. Everything else still works on the folder: `check`, `lint`, `grade` and `drift` read the pak beside the `mod.toml`. Say that Magician's patch-day check only follows paks its own ledger installed.

## 5. Sources: rebuild only if asked

A mod that has compilable sources can stay "as it is" forever: the pak is what ships. Move it onto Magician's build only when the person wants Magician to build it from now on, and `magician doctor` shows the CSDK is set up. The sources must already be an addon under the CSDK's `content/citadel_addons/<addon>/`; set `addon = "<addon>"` in the `mod.toml`.

```
magician ls <original pak>  > before.txt
magician release --out <scratch folder>/pakNN_dir.vpk     # from the mod's folder
magician ls <new pak>       > after.txt
```

Compare the two lists line by line with whatever the shell has (`diff`, or `Compare-Object` in PowerShell). Three kinds of difference, each with its own answer:

- **In the old pak only.** Magician packs only files that still have a source. A file the old pak carried with no source left was stale or hand-added. List them; the person decides whether each mattered.
- **In the new pak only.** A source the old build never shipped. List them.
- **Same path, different crc.** Recompiled. Compare what is inside (`magician dump <vpk> <path>` on both; `magician textures` for images). Identical contents with different bytes is the compiler, and is fine. Different contents is a real change: name the file and what changed.

The rebuilt pak is a new file. It is unproven whatever the old one was, and it is not the mod until the person has seen both in game and said so. Until then the original pak stays the one in the `mod.toml`.

## 6. The look in game

A mod moved as it is, with a matching sha256, will look as it did: it is the same file. An in-game pass is not needed to trust the move. It is needed for the first proof, and after a rebuild.

Run one only if the task includes it, the machine has a desktop, and the seat is free:

```
magician session qa --mod <folder> --out <dir> angles motion
```

`angles` and `motion` are stock scenarios that work for any hero. A mod with no hero has no stock scenario; for those, install it and tell the person what to look at (the HUD element, the sound, the effect). Then open the contact sheets. Say what you saw, or that you could not look. Record a proof only by the rule: frames looked at, the same paks still installed.

## 7. Report

One block per run, verdict first:

```
Deadlock build 6868. 3 mods moved to D:\mods, 1 not moved.
- cool-lash-skin: moved as it is (sha256 matches the original). RE-PROVE: loads, 0 errors, 350 warnings, B Costume, fair play NSFR (M140 on the hat). Installed as pak11 (pak10 is another mod's). Needs: a look in game.
- menu-music: moved as it is. No hero, so not graded. 0 errors. Your own copy is in addons\pak31_dir.vpk; not installed until you say which copy stays.
- fabric-test: moved as it is. BROKEN: 2 x M143 (sheen with detail: the detail layer never shows). Not installed.
- old-skin.vpk: not moved: Magician could not read it as a VPK ("...").
Wrote: three folders under D:\mods, each with a mod.toml; one pak installed. Changed nothing else.
From now on: run `magician check` in a mod's folder, and `magician patchday` from D:\mods after a game update.
```

Always say: what was copied where; for each mod whether the bytes match, the verdict, the error and warning counts with the rule codes that matter, and what it still needs from a person; every install and every refusal; what you did not move and why. If they have many mods in one place, mention that setting it as the mods folder (`magician paths` shows how it is resolved) lets `patchday` and `reprove` find them from anywhere.
