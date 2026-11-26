# Changelog

Newest first. Dates are when the capability was proven, not when the code was written.

Each release's entry says whether the lint rules, the grading rubric or the built-in policy changed (`magician update --describe` shows the fingerprints), because that is what `magician update --check` tells a user to re-run `lint` and `grade` for. An entry that says nothing about them says they did not.

## Unreleased

**Judgement changed in the lint rules (81 now, lint judgement revision 3); the grading rubric and the built-in policy are as in 0.1.0.** Re-run `lint` on every mod, and `magician index` once on each build you lint against: an index from 0.1.0 is not read.

### Judgement changed

The rules measured on Valve's 43 playable heroes, its own files linted against themselves and three finished skins (6871; the record is [docs/research/rule-calibration-6871.md](docs/research/rule-calibration-6871.md)). Where a rule spoke about Valve's way of building a hero, it now measures against Valve; nothing was turned off and no severity moved. On the three skins, with no allowances, the warnings went from 107, 160 and 85 to 44, 31 and 17. Lint judgement revision 3.

- **M109 and M110 judge how much of a model folds or crosses, not each crossing.** Every Valve hero is built from pieces sunk 5 units or more into others at bind, about 250 pairs a hero, with the same spread of crossing length as a skin's, so one crossing cannot tell a clip from construction. Each is now one finding when the model's visible fold or crossing line, in units of its size (the square root of its surface), is more than any Valve hero's: Frank's 4.81 for folds, Hornet's 52.5 for crossings. `magician corpus` measures every hero against both lines on each build.
- **M111 floating-shell speaks from one fair-play pixel.** A gap is a finding from 6 mm, where it was 3 mm: 108 of the 141 loose pieces on Valve's heroes past 3 mm stand under 6 mm. A piece that meets its substrate at the joint between their bones (a hand at its cuff, a finger on the next) and a piece on a bone that carries nothing else (a ring on its ring bone) are articulated, not lifting off. A piece with less than a pixel's area is not judged as floating.
- **M112 coordinate-precision speaks only where float32 snaps a vertex by an eighth of a pixel or more,** at the piece's own distance from the origin. At a hero's reach a step is 1.2e-5 units, and the flat cards it found (18 on one skin) were float32 rounding a plane.
- **M061 judges only what plays.** A dropped particle, sound, sound event, anim graph or clip, or snapshot is still a finding; a material, texture or vector image the original drew with and the replacement draws its own instead of is a restyle, and so is a particle's render model. All 105 M061 findings on the three skins were of that kind (a compiled texture's name carries a hash of its settings, so even Valve's material recompiled names other files).
- **M063 reads only the keys that set how far a sound carries:** the falloff family, `cull_at_distance`, `max_distance_recipient_filter` and occlusion. A replaced event that moved where a sound folds to mono (`mono_downmix_min_distance`), switched a distance filter or changed a limiter was a fair-play violation no allowance could waive.
- **M135 texture-parity leaves a model's morph atlas (`_vmorf`) alone when the mod ships the model it belongs to:** the compiler sizes it to that model's morphs. Shipped without its model, Valve's model reads it, and it is judged.
- Allowances written for findings these rules no longer make are reported stale by `lint`, each by its line.
- **Measured again on 75 community mods** (121 paks: skins, model replacements, HUD, quality-of-life, a map and two gameplay modifications), linted deep with no allowances; every pak finished and none crashed. What they showed:
  - **M062 compares a hitbox or attachment within one rounding step.** A re-export writes Valve's capsule radius 5.774999618 as 5.775000095, which printed 5.78 against Valve's 5.77, and a never-allowable fair-play violation fired on Valve's own hitbox.
  - **The model rules leave alone a model the game never loads** (it replaces nothing and nothing in the mod names it: M031). A skin that left a copy of its hero under `panorama/` failed fair play (M129) on a model nobody sees.
  - **M031 counts what Panorama names in its own text:** a stylesheet's `url("s2r://...")`, a script's or layout's `file://{images}/...` and `file://{resources}/...`, and the bare script paths a compiled layout lists. A HUD mod's images and an FOV slider's script read as never loaded.
  - **M063 does not judge a voice line's event by its length,** as it already did not judge the line's file: Valve's own voices fail that test on 46% of shared lines.
  - **M081 reads the radius an initializer writes.** A particle is born at its constant radius unless an initializer writes the radius (`C_INIT_RandomRadius`, or `C_INIT_InitFloat` on its default output field); Valve's smoke with a constant of 2 drawn at 30 to 40 read as 2, and a copy drawn at 5 to 10 as 2.5 times its size.
  - **M135 judges what shows where Valve draws a texture:** smaller than Valve's (it blurs), a format that loses more (block artefacts), or fewer mips than its size could hold (it shimmers). A larger image, or PNG_RGBA8888 where Valve's is BGRA8888 (both keep every pixel), is not a finding.
- **New rule M091 shared-file-unchecked (a note).** Without a users index for the build, the shared-file rule M090 cannot run. Lint said so only in a note on stderr, which `--json` silences; now the run carries an M091 finding that names how many replaced files went unchecked and why, as M144 does for a fair-play comparison. `magician doctor` says the same.
- **M063 reads a sound event's base in both of Valve's forms.** `base` can be one event's name or a list of `{ event_name field_name }`, and Valve writes both. Lint read only the name, so an event that kept Valve's base in list form read as having none, and every such event was M063 "the base stack sets falloff and perspective". The list is now read as its events: the same base in either form is no finding, and a list that keeps Valve's base and adds one is judged by what the added base sets (a level is M063 quieter or M064 louder, a reach key is M063).
- **M090 no longer names a hero that does not use the file.** A base animation graph lists every hero's variation of it, and the users index followed that list backwards: the one hero whose graphs take the base flinch graph unvaried became a user of every hero's flinch clips and skeleton, and two others of about 1,400 hero files each, so M090 named them on nearly every skin. The target dummy (`EHeroDevState_DebugOnly`) was counted as a hero too. On 6868 the files under hero model folders shared by more than one owner fell from 3,808 to 2,333. An index from before the fix carries no format revision and is not read (M091 says why).

### Other changes

- `magician check` groups the warnings under its lint line by where a player meets them: on the hero, effects and sound, other heroes, frame cost, loading, the pak itself; fair play keeps its own rows. `NSFR` is spelled out as "not safe for ranked" and `FP?` as "not checked". `--json` carries each rule's group as `seen` in `by_rule`.
- `magician install --status --json`: the ledger as one document (`magician.install-status/1`: the build, whether the addons search path is there, the backup, and each pak's name, crc, owner, state and the name it had before another tool renamed it), for tools that read what is installed. `--json` on an install itself is refused.
- No perceptual loudness rule for voices: measured first on Valve's own voices (400 lines two heroes share, EBU R128 against the 50 ms envelope peak lint compares), a quarter of the pairs whose peaks agree within 1.5 dB differ by more than 1.5 LU, and 3 to 5% by more than 3 LU, so a rule at those limits would fire on Valve against Valve. The measurement is in the engine reference.
- `build` and `release` say before compiling when a model document replaces a Valve model that carries distance fields and asks for none (a document patched from a decompile), with how many fields it drops; M142 said it only after. A document that has a `DistanceFieldList` already compiled against a gameinfo that allows it, so a skin's own build need not compile it first. The engine reference now holds the distance-field authoring facts.
- An in-game session cuts a contact sheet for a turntable or an orbit shot frame by frame (`@turntable`, `orbit.py`: `<tag>_<degrees>.png`), beside the sheets for bursts and films; before, those views had to be opened one by one. `capture.py sheet --prefix` sheets one such sequence.
- `harness/hud_topbar.py` judges a hero card where a player sees it: in the in-match top bar at true size (70 x 95 px at 1080p, cut by the badge mask over the team-colour disc, red at low health), among Valve's cards on a strip of a real capture, at 1080p and 1440p and the 1080p pixels at 4x, with each card's fill and contrast beside Valve's median. For any hero; Pillow only; `harness/test_hud_topbar.py` holds it in CI. The engine reference has the top bar's facts, read from the game's layout and styles.
- `VERSION` carries the release commit's time (`time`, Unix seconds), and `update --check` orders two commits of one version by it, so a source build one commit past a release made the same day reads as ahead rather than "an update is available". A `VERSION` without it, 0.1.0's included, still reads and is ordered by its date as before.
- `magician new` scaffolds into a folder that is there, the case of a skin that began as sketches and notes before it had a manifest: it writes only the files the folder lacks, keeps and names the ones it has (a `.gitignore` of its own with the scaffold's lines it lacks), and takes back what it wrote if a write fails. A folder with a `mod.toml`, or inside a mod, is still refused.
- M145 inside-out-shell tells an inverted-hull outline (an inside-out copy wrapped around another piece) from a piece that is merely turned out, and says what the game does with it: it is not culled into a line but draws an opaque black shape over what it wraps, though Blender and Source 2 Viewer both show the line. The finding points to the material's own outline (`F_SOLID_COLOR_OUTLINE`, heavier with `F_OVERRIDE_NPR_OUTLINE`). Where it fires and its severity are unchanged; the engine reference has the facts.
- `magician mesh --json-out` carries the fair-play silhouettes, one entry per view with its share of Valve's (`percent_of_valve`, the number M124 judges), IoU and the pixels added and lost, as the text report and the MCP `mesh` tool's `silhouettes` do.
- `magician cloth` ends each chain's row with how it moves: the rebound of its median free node (frequency and damping ratio, in the solver model M141 judges by) and the steady offset of its most-pressed leashed node under the hero's fall against its leash. A chain several times over its leash moves and stops dead in game, which its clips alone do not show.
- Builds clean under Rust 1.100 with warnings as errors, and two Linux tests no longer assume older system tools.

## 0.1.0 (2026-11-25)

The first public release. **If you ran a copy from before it, judgement has changed in all three places: the lint rules (80 now, lint judgement revision 2), the grading rubric (3.1.0) and the built-in policy. Re-run `lint`, `grade` and `check` on every mod.** Those changes come first; what is new follows, by what it is for, and the foundation it was built on is last.

### Judgement changed

Lint:

- **An error means the game will not load or play something.** Rules are held to that by two records kept per build, which `magician rules` prints under each rule: `magician corpus` runs a rule's test over Valve's own content, which works by definition, and `magician witness` holds a mod's findings against the game's own log (both below). What moved on that evidence (seven published mods by another modder, each put in the game on 6868, and the corpus):
  - M106 cut-body-void is a warning: nine openings it reported on a published model were not visible from the gameplay camera at any pitch, or close behind the head.
  - M071 is an error only when the shipped default's pixels differ from Valve's: every copy in three published mods decoded to Valve's own pixels, or sat at a path Valve does not ship.
  - M063 and M064 do not judge a voice line (`sounds/vo/`) by its length: Valve's own voices fail that test against each other on 46% of the 79,572 pairs of lines two heroes share.
  - M063 reads a changed sound-event base through the game's base events: `Base.Ability` for `Base.Ability.Cast` (one flag, no level or reach) is no finding, and `Base.Movement.Melee` for `Hero.Default.Melee.Swing` (Valve's 2 dB quieter) is M064 louder.
  - M001 is not said of a file the game never reads (M012). M033 does not report a material or texture the mod draws with (an exporter's `dummy` name on the real art).
  - A candidate cloth rule (a loose chain lighter than the hero's fall rises while he is airborne) was measured first and not written: it fires on 59 of Valve's 200 cloth groups.
- **Valve's own heroes lint clean.** Packed unchanged and linted deep, 24 of the game's 47 hero models drew the never-allowable M129 and six drew skinning errors. Now a mesh with eight influences is read as eight (M116, M119); the hitbox envelope measures spheres and boxes as well as capsules; a model is measured as the game draws it, one choice of each body group; and M129, M130 and M140 count only the overflow Valve's model at the same path does not already have, with Valve's share named in the message. M108 leaves alone two of Valve's pieces sharing a plane where Valve's model has them sharing one, M122 judges a lower LOD against how far Valve cuts its own (1.5x Valve's share), and M062 compares hitboxes that share a name as a set.
- **Fair play is lenient by default, enforced with one switch.** Until a policy sets `[fair_play] enforce = true`, every fair-play finding is a warning: `lint` does not exit 1 on one and `install` goes ahead. Enforced, they are errors. The label and the record say `NSFR` or `FP?` either way. The hitbox envelope is tiered: M129 (over 18 units, or over 3.5% of the visible surface beyond 10) is never allowable, M140 below it can be allowed at one site within its limit.
- **New rules.** M033 dev-leftover; M141 cloth-flutter, which also reports a group with no damper at all; M142 distance-field-dropped; M143 pbr-combo-invalid; M144 fair-play-unchecked (a fair-play comparison that could not run is said, never passed in silence); M145 inside-out-shell; M146 lod-inversion; M147 texture-mip-starved; M148 default-slot-literal (a decompile's literal colour where Valve leaves an engine default: a glass mask recompiled as 0 for Valve's 1 drew a summon solid); M149 material-texture-dropped (the game logged the missing slot and drew its error texture 50 times in one pass of a published mod).
- **Messages that say more.** M062 says when a changed attachment is Valve's with its axes in another order, the mark of an exporter's Y-up/Z-up swap, and knows a turned attachment at the origin by its rotation. M031 says when a voice line is one Valve records for other heroes and not for this one.

Grading (rubric 3.1.0, record `magician.grade/3`):

- **A grade describes what a mod makes its own, and does not rank it.** The label is the kind of mod (Echo, Costume, Variant, Kit, decided from identity signals: the head against Valve's, a rename, new hero art, a declared voice) and the groups of the hero it covers: look, effects, sound, each named when covered, `some <group>` when begun, left out while Valve's. Under the label the report says what was counted, in the game's terms. There is no letter and no score, and the tier titles of 0.1.0 are gone.
- **Separate flags, none of which changes the description.** `broken` is lint errors that are not fair play, the same test `check` uses. `NSFR` is a confirmed fair-play violation, `FP?` a fair-play check that could not run. `stale(N)` and `unproven` are asked of the bytes (Proof, below). Rights are a person's `[rights] state`. The release gate lists every blocker, and `grade --gate` exits 1 on one.
- **Every pak of a mod is graded together**, as the game loads them, and `grade --mod` finds the mod's own `magician.allow`. Materials count wherever the hero's compiled model references them, and a showcase scene counts when the release carries one with entities of its own. The hero's model is the one `heroes.vdata` names, and a hero's own block names its art whatever it is called: two heroes' mods were graded wrongly before.
- **A mod for no hero is `not graded` and says what it is**: a map, HUD, music, prop, effects or sound mod.

Policy:

- **Every number that encodes a judgement is a key of a plain-text policy file** (`--policy`, `MAGICIAN_POLICY`, `policy =`): the fair-play limits, the allowance limits, which rules are never allowable or advisory, the two enforce switches, the weights within a group, the lines that name a group, identity and coverage targets. `magician policy` prints every value and where it came from, and a grade record carries the policy's fingerprint and the keys it moved.

Proof:

- **A proof belongs to the bytes it was a proof of.** `magician prove` records each proof in `proofs.tsv` beside the `mod.toml`: the build, the date, every file installed for the mod with its crc32 and sha256, and the pass it was seen in. `check`, `grade`, `patchday`, `drift`, `doctor` and a mod source's page ask "proven on N" of the bytes they judge, so a rebuilt pak starts unproven, and a build `[proven] builds` lists with no proof for those bytes counts for nothing. Rows stand alone, so two machines' ledgers merge as a union of lines.

### A mod is a folder

- `magician new <hero>` starts a skin from the installed game's own data: `mod.toml`, an addon laid out at the game's paths, a package, starter scenarios. It copies no Valve file and never writes into a folder that exists.
- `magician init` writes a `mod.toml` for paks or an addon's sources, the hero decided from the files. Given a download (a zip is read here; a .7z or .rar goes through bsdtar) or paks from elsewhere, it copies only the paks into the mod's folder, each proven the same bytes and printed with its sha256; a pak whose every file another ships is left out, and files of another hero are named.
- `magician check` is one mod's verdict on the installed build, answer first: lint and grade together, its proofs, drift since its last proven build, its scene patches. Exit 1 broken, 4 re-prove, 0 clean. Under the lint line the findings are counted by rule, fair play and its advisories on rows of their own (`by_rule` in the JSON).
- `magician diff <a.vpk> <b.vpk>` says whether two paks are the same mod: the proof of a move or a rebuild. Recompiled files (only the compiler's record differs) are told from different ones.
- Inside a mod's folder `lint`, `ls`, `grade`, `drift`, `release`, `install`, `uninstall`, `prove`, `footprint` and `scene check` need nothing after the verb. Every answer filled in is said on stderr with where it came from, and anything given explicitly wins.
- `magician doctor` is the machine one line at a time, each with its fix. `magician` alone says where things stand and what to run next. A hero is named any way (`lash`, `"Lady Geist"`, a model path), and a mistyped command, option or hero is answered with the nearest real one.
- `lint` prints a run of one rule on numbered siblings as one line, and an explanation several findings share once; its JSON keeps every finding.
- `skills/moving-to-magician` takes an agent through bringing a mod made without Magician under it, as the same bytes.

### Patch day

- `magician patchday [--capture]` is the whole check: the build's baseline, the install state, each mod's drift from the last build a proof holds its bytes on, its `mod.toml` targets, a verdict (exit 0 clear, 4 re-prove, 1 broken).
- A baseline has two sidecars. `rig.tsv` (every skeleton's bones, every anim graph's parameters) lets `drift` name a renamed bone. `engine.tsv` (every executable, library and shader archive) lets `patchday` say whether a patch changed the engine or only the data.
- `patchday` says what a hero-data edit means for each mod: whether any name in its hero's block moved, and which.
- `magician reprove` is patch day in game: for every mod that needs it a vanilla pass when the build has none, the mod's own pass, and one report (`ready`, `look`, `failed`). `reprove accept <pass>` records a pass that was looked at. Stills are not compared between passes; the measurements that ruled it out are in [docs/PATCH-DAY.md](docs/PATCH-DAY.md).
- `patchday` and `reprove` find a folder of mods whatever it is called (`--mod`, `MAGICIAN_MODS`, else the nearest folder with mods below it). The first command on a new build captures its baseline, for this machine's own game only.

### The game, driven

- `magician session` is one in-game session for any mod with a `mod.toml`, in parts or whole: the seat, the mod's pak over its slot (`--pak`, `--vanilla`, `--restore`), the launch, the hero, scenarios, timed takes, a SourceTV take filmed from free cameras, the stop, contact sheets.
- The game is bound to the seat that launched it: nothing in the harness types into or stops another holder's game, and the seat is free only once the game has exited.
- `magician witness <pak>... --log <console.log> [--vanilla <console.log>]` makes each finding confirmed, contradicted, not reached (with what would reach it), a frame's or a file fact, and lists the game's complaints no finding predicted. The stock `witness` scenario is the probe. `session --force` (with `--pak` and `--restore`) looks at a pak lint refuses.
- A vanilla log baseline belongs to a build and a scene; a pass is never read against another hero's or, silently, another build's.
- `ingame/prove_harness.py` runs the whole harness once on a real game and writes a report.
- `cinema/` is an optional, dev-only film kit for trailers and stills: a scene file becomes an animatic, then a frame-exact render from a SourceTV take.

### Publishing

- `magician scene make | check | compose | install` ships a hero-page scene as a patch of only our files and builds it from the player's own game, refusing a map a game update changed. Players need neither Magician nor Python on Windows (`install-scene.cmd`), and only Python 3 elsewhere.
- `magician bundle` writes a Deadlock Mod Manager release bundle and a reproducible zip, refusing a path in two paks or a file byte-identical to Valve's.
- `magician source` writes a static mod source for Deadlock Mod Manager from bundles, with a page per mod and a release feed. A listing says the builds its zip's own paks were proven on. The pages carry schema.org data and a sitemap, a link's `kind` says it is the same mod elsewhere or a video of it, and `authors` names an author's site ([docs/reference/mod-source.md](docs/reference/mod-source.md)).
- `magician publish nexus <bundle.json>` puts a bundle's zip on a Nexus Mods page made on the site: its first file, then a version per release, with the release's changelog; `--page` sets the page's text. The zip's own paks go through the release gate first (exit 3 on a blocker), and `--dry-run` sends nothing. The key is `NEXUSMODS_API_KEY`, from the environment only ([docs/reference/nexus-mods.md](docs/reference/nexus-mods.md)).

### For scripts and agents

- Exit codes have one meaning each: 0 clean, 1 broken, 2 usage error, 3 refused on purpose, 4 re-prove, 5 an update is available. `--help` after any command prints its usage and runs nothing.
- **A command accepts only its own options.** One it does not take is a usage error before anything runs, a typo or another command's option alike: `magician install --dry-run` used to install. `magician help <command>` lists every option that command takes, and every command line in the docs is held to it by a test.
- `--json` on `check`, `doctor`, `lint`, `grade`, `drift`, `patchday`, `prove`, `release`, `rules`, `update` and `reprove` prints one document with a `schema` string and no human text; a command that stops prints `magician.error/1` as that document.
- One front door: `magician session`, `reprove`, `bundle`, `source` and `publish` run the Python tools with the caller's arguments, stdio and exit code.
- Locations are found, not assumed: the game, the reference folder, the CSDK, the viewer and the rest each resolve from a flag, an environment variable, a config file, then discovery, and `magician paths` prints each answer and its source.
- `magician update --check` says whether there is a newer Magician and whether it would change judgement (exit 5 when there is one); `magician update` applies it to a release install or a git checkout and puts back what fails. Nothing else goes online except `doctor --updates` and the upload `publish nexus` makes.
- The MCP server (`mcp/`) is the CLI as seventeen JSON tools, with no install, uninstall or launch tool. `skills/using-magician` is an Agent Skill for driving the CLI on a mod.
- A reader that stops reading (`| head`) no longer crashes a command. Progress and colour appear only on a terminal.

### Linux

- Magician is first party on Linux as well as Windows: everything that reads or writes files is proven on Ubuntu 24.04, with the game found in Steam's native, Flatpak and Snap homes.
- `build` and `release` compile on Linux, Valve's compiler running under Wine: Maid Lash's addon gave 550 of its 551 files byte for byte as on Windows, the one left differing between any two compiles. `grade`, `footprint` and `index` give the same records with Source 2 Viewer's Linux CLI.
- The in-game harness stands on one seam to the system, `ingame/gameos.py`, with a Windows and an X11 backend; the film kit and the hero-rip and viewer scripts are Python on both. The X11 side is proven against a test window, not yet with the game under Proton ([README.md](README.md#platforms)).

### Install and build

- An install records the mod it is for and never replaces another mod's file, a file it did not install, or one changed since; `--as pakNN_dir.vpk` picks another slot, whatever the pak's own name. A hero-page scene map installs to its own route under the same ledger.
- `install` and `uninstall` refuse under a running game unless the caller holds the game lock, and only for the game folder they were asked to change. `uninstall --mod NAME` and `--all` roll back from the ledger, never removing bytes Magician did not install.
- The ledger is changed under a lock and written atomically, and a pak a mod manager renamed is followed by its crc.
- `build` and `release` take a lock per addon, compile a model with distance fields against a gameinfo that has the feature, and leave out the paths an addon's `magician.exclude` lists.
- The maker's mark signs under the mod's own author (`--key`, `[mark] key`, `[mod] author`); there is no built-in key.
- `MAGICIAN_THREADS=N` caps a lint's threads, for a run beside the game.

### Reading the game

- `magician cloth` reads the compiled FeModel natively, beside Valve's model at the same path. `refs`, `dump`, `mesh` and `cloth` take a path in a pak as its source (`models/x.vmdl`).
- `tools/s2v_mcp.py` starts, stops and calls the Source 2 Viewer GUI's MCP server, for repeatable asset renders without the game.
- A zstd decoder (RFC 8878), so KV3 v5 zstd blocks decode; `kv3::encode` writes blobs the way the game does.
- Every reader of bytes or text someone else made is fuzzed, a short run in every `cargo test`. It found, among others, buffers sized from header fields in LZ4, KV3, zstd, the VPK tree and meshoptimizer, and `prove`, `grade` and `patchday` deciding `mod.toml` sections three slightly different ways.
- The workspace builds under one lint set (unsafe code denied, no `unwrap` in the readers, overflow checks in every profile), and `.github/workflows/ci.yml` runs the Rust and Python suites on Windows and Linux runners.

### The foundation

- `vpk`, `resource` (block table, RERL, native KV3 v4/v5 with LZ4, texture headers and pixels), `mesh` (compiled model meshes, meshoptimizer v0/v1, glb proof), `geom` (geometry kernel), `lint` (rule catalogue with a breaking fixture per rule, mesh family M100-M139), `grade` (hero footprint and overhaul rubric), `expr` (DynamicParams range proof), `mark` (the maker's mark: `sign`, `verify`), the findings cache, `magician` CLI (baseline, scan, dump, kv, index, drift, lint, mesh, pack, build, release, install, footprint, grade).
- Idempotent install step with a `gameinfo.gi` backup per build and an install ledger.
- In-game harness (launch, console as scancodes, PrintWindow capture, log diff, crash dumps) and offline Blender QA.
