# Changelog

Newest first. Each release says whether the lint rules, the grading rubric or the built-in policy changed, since that is what `magician update --check` tells you to re-run `lint` and `grade` for. The measurements behind each judgement change are in [docs/research/rule-calibration-6871.md](docs/research/rule-calibration-6871.md).

## Unreleased

- A ranked-safe skin's grade label says `· ranked-safe`, as a freeform one says `· freeform`.
- `magician bundle --target divine` writes a Divine Skins upload kit from the same `bundle.json`.

## 0.4.0 (2026-11-27)

**Judgement changed in the lint rules (91 rules, revision 9) and the grading rubric (M152 is fair play).** Run `magician index` once on each build, then `lint` and `check` on every mod.

### Judgement
- Rules calibrated on Valve's own content and 144 community mods, with what could be settled seen in game on 6880. Valve's files lint clean; on the community corpus, broken skins fell from 78 to 61 of 83.
- **Hero model contract (M062):** compares the model's game data and animation references with Valve's model. It names the A-pose, a missing or misplaced health bar, silent footsteps and a moved camera. Lost bones and lost attachments are judged by who still uses them, and camera attachments must match Valve's exactly.
- **Errors only for what breaks in game:** missing files nothing reads, old AnimGraph 1 references, fonts in a pak, harmless copies of Valve's files, and looks such as albedo and hair coverage are warnings now.
- **New rules:** outdated HUD layouts that crash or blank the HUD (M042), sound format (M065, a note), effects nothing plays (M084), another hero's files (M092), dropped cloth or ragdoll (M160), material settings the shader ignores (M150), a changed material shader (M151), material flags as fair play (M152), error-material draw calls (M153), and which compiler built a file (M170).
- `drift` blames only what a patch moved, never a mod's own choices.

### New
- `magician csdk` describes the installed CSDK: version, binary sets, the game build its files come from, and known gaps. `doctor` flags the ones nothing works around.
- `magician material` inspects, diffs, exports, applies and verifies materials, including Valve's.
- A `look` scenario films a mod's hero from four sides in three lights.
- The store's front page leads with an Add to Deadlock Mod Manager button.
- `install --status` lists paths that installed paks share, and `doctor` checks `gameinfo.gi` against the stock file.

### Fixed
- `build` no longer breaks the CSDK launcher's Create VPK, and `pack` refuses paks over 2 GiB.
- 37 rough edges found by running every command on 20 community mods: wrong counts, dead-end messages, slow cold lints on large paks, help text, and MCP tools (now 19).
- The in-game session learns the hero's abilities on 6880, and hands back a test pak cleanly.

## 0.3.0 (2026-11-27)

**Judgement changed in the lint rules (revisions 4 and 5).** Re-run `lint` and `check` on every mod with a replaced model.

- A moved attachment that stays near the body is a warning with the distance. Lost and reparented bones are judged by what hangs on them.
- The fair-play labels are now ranked-safe and freeform. Scripts keep reading `fair_play.state`.
- `mesh --fairplay` draws a view as lit and as fair play tests it, from one camera.
- `patchday --verify-engine` catches an engine replaced within a build.
- A first `check` says plainly that you can stop at lint; `rules <code>` shows one rule.
- Findings files keep each finding's severity.
- CI and release workflows moved to current GitHub actions.

## 0.2.1 (2026-11-26)

**Judgement changed in `check`.** Re-run `check` on mods proven on the installed build.

- A proof no longer hides a contract that broke between builds, and `prove` refuses to record one past such a break.

## 0.2.0 (2026-11-26)

**Judgement changed in the lint rules (81 rules, revision 3).** Re-run `lint`, and `magician index` once per build.

- Rules measured against Valve's 43 heroes and 75 community mods. Where a rule flagged how Valve itself builds heroes, it now measures against Valve. False alarms fixed in the fair-play rules M062 and M063.
- M124 judges only what can be shot. New note M091 says when the shared-file check couldn't run.
- `check` groups warnings by where a player meets them. `install --status --json`.
- `magician new` works in an existing folder, `cloth` reports how each chain moves, and the top-bar card check, turntable sheets and mesh JSON silhouettes are new.

## 0.1.0 (2026-11-25)

The first public release. **Judgement changed in the lint rules (80 rules, revision 2), the grading rubric (3.1.0) and the policy.** Re-run `lint`, `grade` and `check` on every mod.

- **Judging:** errors mean the game won't load or play something, checked against Valve's own content (`magician corpus`) and the game's log (`magician witness`). Fair play is a label and is enforced only when a policy asks. Grades describe what a mod makes its own and don't rank it. Every judgement number lives in a policy file.
- **Proof:** a proof belongs to exact bytes on a build. `check`, `patchday`, `drift` and the mod source all ask it of the bytes they judge.
- **A mod is a folder:** `new`, `init`, `check`, `diff`, `doctor`, and commands that need nothing after the verb inside a mod's folder.
- **Patch day:** `patchday` and `reprove`, with rig and engine sidecars that name renamed bones and engine changes.
- **The game, driven:** `session`, `witness` and the film kit, bound to the seat that launched the game.
- **Publishing:** `scene`, `bundle`, `source` for Deadlock Mod Manager, and `publish nexus`.
- **For scripts and agents:** one meaning per exit code, `--json` with schemas, strict options, `update`, an MCP server and Agent Skills.
- **Platforms:** Windows and Linux, including compiling under Wine.
- **Foundation:** native readers for VPK, KV3, textures, meshes and cloth, all fuzzed; a safe install ledger; the maker's mark.
