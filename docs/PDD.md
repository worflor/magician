# Magician: product design

What Magician is trying to be, and the taste that follows. The technical shape is in [TDD.md](TDD.md).

## Who it is for

Magician's first user is not a person at a desk. It is a script on a server, a scheduled job, or an agent woken up because something needs attention. A person can run every command, and the author does, but every decision is made for the caller who can't see a window, can't answer a prompt, and acts on the exit code.

Deadlock mods are treated as programs written against a moving platform. Each game build is a compiler target. A mod either still links against the new target, links but has to be re-tested because something under it changed, or no longer links, and the toolchain says which, with the evidence. That is the whole product: a compiler, a linter and a maintenance system for a language that happens to be made of VPKs.

## The loop it serves

The reference workflow is patch day on an always-on machine:

1. Steam downloads a new build on a box that is running anyway.
2. A script runs `magician patchday --capture`. A few seconds later it knows, per mod, whether everything holds, what needs re-proving in game, and what is broken and why.
3. Nothing wakes an agent unless something needs one. When something does, the agent gets a specific job ("Valve renamed `spine_1` in Lash's skeleton"), not a vague one ("check the mods").
4. The agent fixes, rebuilds and reinstalls what is broken; `magician reprove` films every mod that needs it in the game; once the frames have been looked at, a proof of the installed bytes on the new build is recorded.
5. The person gets one short message: how many mods were checked, how many are fine, which need what.

The runbook is [PATCH-DAY.md](PATCH-DAY.md). Every new capability should make some step of this loop more automatic, more specific, or more trustworthy.

## What the toolchain promises its caller

- **Determinism.** The same mod against the same build gives the same findings in the same order. `pack` gives the same bytes. A cache may make a run faster; it never makes it different.
- **Exit codes that mean something.** 0 clean, 1 broken (someone must act), 2 usage error, 3 refused on purpose, 4 loads but must be re-proven in game, 5 an update is available (`update --check` only). A wrapper decides what to do from the code before it reads a word. `--help` on any command only prints its usage; an option the command does not take is an error, not a guess.
- **Text a script can read.** Stable labels (`ok`, `RE-PROVE`, `BROKEN`, rule codes like `M062`, `Echo (look, effects) · NSFR`), one finding per line, the evidence on the line. One JSON document with a `schema` string wherever a machine is the main reader (`--json`).
- **Specific findings.** A finding names the file, the rule, and what happens in the game: "the original asset shows instead", "the skin vanishes", "enemies shoot cloth that cannot be hit". "Something looks off" is not a finding.
- **Honesty about limits.** What a rule can't see is said where the rule is. Unchecked is never clean: a bounded search says where it stopped. Unverified in-game behaviour is marked UNVERIFIED.
- **Nothing a caller has to know twice.** What a mod's own `mod.toml` already says (its paks, hero, allowances, last proven build) is never asked for again: run inside its folder, a bare verb is enough. Every answer filled in this way is said, with where it came from, and anything given explicitly wins. A typo is answered with what it most likely meant; `magician` alone says what to run next. The same commands and the same JSON serve a script; the help only gets shorter for a person.
- **No surprise writes.** Only `install`, `uninstall` and `scene install` (and the in-game sessions that run them) touch the game, only through a ledger, and never under a game someone else is using. Anything that writes says where.

## Taste

- **Compile-clean is not proof.** Exit code 0 is one input. A mod is proven when it has been seen working in the retail game, and a proof belongs to the bytes that were seen: `prove` records one only after that, and a rebuilt pak starts unproven.
- **Valve's file is the reference.** A mod is judged against Valve's own file at the same path. What Valve already does is design, not a defect.
- **Fair play is a lint category.** Mods are cosmetic. A skin that makes a hero harder to read, smaller than its hitboxes, quieter than its cues or easier to lose in a fight is broken, whatever it looks like.
- **Never weaken a rule to make a mod pass.** Accept a finding per site, in writing, with a reason that stays visible.
- **A grade is a measured claim, in parts that don't bleed into each other.** The description says what kind of mod it is and what it makes its own; `broken` says whether it loads; compliance says whether it is fair to play with; freshness says whether these bytes are proven on this build. A superb overhaul a few units too wide reads `Costume (look, effects, sound) · NSFR`, not like a sloppy one. The label is a projection of a record that holds every measurement behind it.
- **A grade describes, it does not rank.** A mod that reskins one ability is not a lesser mod than a full character; it is a different one, and `Kit (some effects)` says which. The words name what the mod made its own, never whether the art is good. A mod with no rubric yet is "not graded", never a made-up description.
- **Unknown is not a pass.** A fair-play check that could not run is `FP?` until it runs, never a quiet pass; the release gate lists it, and blocks on it when fair play is enforced.
- **The build is the era.** Facts about one build come from that build's baseline, not from constants in the code.
- **Own the reading.** Formats are decoded here, without external crates, and checked against an independent reader. A round trip through our own decoder proves nothing.

## Non-goals

- A GUI, an editor, or a Blender add-on.
- A runtime, a loader, a launcher or a mod manager. Players use Deadlock Mod Manager; Magician produces what it imports.
- Automatic fixing. Magician says what changed and why it matters. Rewriting a mod for a new build is creative work for a person or an agent, not a side effect of a check.
- Gameplay mods. The fair-play line is part of the identity. A policy file can move its numbers for a house, but every grade names the policy it was judged by and the keys it moved, so a moved line is never a quiet one.

## What could be technically right and still feel wrong here

- A command that asks a question on stdin, or waits for a keypress.
- Output that only makes sense in colour, or that changes shape run to run.
- A warning that would wake someone up for something Valve's own file already does.
- A "pass" that came from not looking: a skipped check, a timed-out search, a rule that silently didn't run.
- A convenience that writes into the game install without the ledger.
- A default that is a guess: a command that picks a pak, a hero or an owner without saying which and from where, or folds in one that only looks like it belongs.
- A reader that stops reading (`| head`) turning into a crash, or a progress line or colour code in what a script reads.
- A grade that names more because the mod ships more files, not because it covers more of the hero, or because of Valve's own files shipped back to it.
- A fair-play failure that changes the description, or a description that hides one.
- A judgement number that changes and leaves no trace in the record.
- A green result presented as proof when nobody has looked at the frames.

## The name

Magician, after The Magnificent Sinclair, Deadlock's own magician: a real in-game reference, and a pair with Mycelium, the same kind of toolchain for Minecraft mods. "It's magic" suits a tool whose job is to hide the pipeline. It names the creator-side toolchain (compile, lint, pack, drift-check, prove), not a loader; Deadlock has none to replace.

Names considered and set aside: Séance (spirit into a vessel, the runner-up), Lashout (reads as an angry outburst and as one hero), MAGIK (collides with Marvel), Livelock (means a bug), Vessel and Planchette (better as nouns inside the tool than as its name).
