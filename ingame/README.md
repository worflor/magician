# In-game harness

How Magician sees the retail game: launch it, type console commands, capture the window, diff the console log against a vanilla run, read crash dumps. Python stdlib and PowerShell. Nothing is injected into the game; the harness types and looks, as a player would. That is also its limit for automation: it needs an interactive Windows desktop with the game window in front, and the frames it captures need someone, a person or an agent that can see, to look at them.

Every script finds the game, the reference folder and the game lock through `gamepaths.py`, which resolves them exactly as `magician.exe` does (`DEADLOCK_GAME`, `MAGICIAN_REFERENCE`, the config file, then Steam; `magician paths` shows the answers). Output goes under `<reference>\work\`. The console key is the player's own bind, read from `citadel\cfg\user_keys_0_slot0.vcfg` (`MAGICIAN_CONSOLE_KEY` or `--key` override it; grave when nothing is bound).

**The one entry point for a mod is `mod_session.py`.** It takes the seat, installs the mod's pak, launches, waits for the map, selects the mod's hero, runs scenarios and timed takes, films a SourceTV take from free cameras, stops the game, gives the seat back once the game has exited, and cuts contact sheets, for any mod with a `mod.toml`:

```
python -I ingame\mod_session.py all  --mod <mod folder> --out <dir> angles motion        # one session, start to finish
python -I ingame\mod_session.py up   --mod <mod folder> --out <dir> [--pak <pak>] [--restore]   # or in parts, steered between
python -I ingame\mod_session.py run  --out <dir> <scenario | plan.plan | take.stage>...
python -I ingame\mod_session.py film --out <dir> <shots.txt>                            # the take, from free cameras
python -I ingame\mod_session.py down --out <dir>
python -I ingame\mod_session.py qa   --mod <mod folder> --out <dir>                     # the mod's [ingame] pass, with crash dumps and logdiff
```

`python -I ingame\mod_session.py` prints every option, the `[ingame]` table a `mod.toml` may carry (`holder`, `pass`, `map`) and the exit codes. Items are looked up in the mod's own folders first (`source/ingame/scenarios`, `source/ingame/plans`, `source/ingame/takes`, `ingame/scenarios`, `scenarios`), then here; `mod_session.py where <name>` says which file a name resolves to. Every verb after `up` checks that the seat is still the session's and the game the one its launch bound, and does nothing otherwise (exit 9). `--restore` keeps copies of the paks it replaces or uninstalls and puts them back at `down`, then compares `magician install --status` with the one taken before. Tests: `python test_session.py` (the install and restore run the real binary against a scratch game).

| Tool | What it does |
|---|---|
| `mod_session.py up\|run\|film\|down\|sheets\|all\|qa\|where` | The session driver above. |
| `takes.py live <plan.plan> <out>` / `stage <take.stage> <out>` | Timed takes: keys down and up on a clock while `capture.py` films (`live`), or the beats of a SourceTV take marked into `marks.tsv` and filmed later from free cameras (`stage`, then `../cinema/replay.py`). Notices a frozen hero: two frames a second apart that are the same picture while a movement key is held (exit 8). Tests: `python test_takes.py`. |
| `launch.ps1 [-Extra "+map new_player_basics"] [-Holder NAME] [-Wait] [-Width 1920 -Height 1080] [-Adopt]` | Takes the game lock, starts Deadlock windowed-borderless with `-condebug`, waits for the window and binds the game it started to the seat. Refuses to adopt a game it did not start (`-Adopt` for your own instance, which binds it). |
| `console.py [--key KEY] "cmd" ...` / `--script file.txt` | Types console commands as hardware scancodes (SendInput). Reads results back from `game\citadel\console.log`. Checks the console's on-screen state before and after each command, so a lost toggle can't desync. Refuses to type into a game the holder did not launch. |
| `capture.py shot <out.png> / burst / sheet / crop` | Captures the game window through PrintWindow, so overlapping windows don't get in. Bursts grab first and encode later to keep frame timing. Also builds contact sheets and zoom crops. |
| `turntable.py` / `orbit.py` | Generate scenarios for a turntable (a bot turned with `ent_setang`) or a ring shoot. |
| `qa_run.ps1 -Mod <mod folder> [-Scenarios a,b] [-Wait]` | `mod_session.py qa` from PowerShell: every scenario into a dated folder, contact sheets, crash dumps, and the warnings the mod adds over vanilla. The pass is the mod's `[ingame] pass`, or `-Scenarios`. |
| `logdiff.py --build N --scene <hero>_<map>` | Diffs the console log against the vanilla baseline of that build and scene (normalised: timestamps, pointers, float noise, ids), so only the warnings the mod adds remain. Baselines are `<reference>\work\ingame-baselinesanilla_<build>_<scene>.log`; `mod_session.py qa --vanilla --restore` saves one. With none for the build it uses the newest older build's for the same scene and says so; it never uses another hero's. Exit 1 new warnings, 3 not compared |
| `stop_game.ps1 [-Holder NAME] [-NoQuit] [-ConsoleKey KEY] [-KeepLock] [-Force]` | For the holder's own game only: asks it to `quit` through its own console and waits up to 45 s, then stops that process (never another `deadlock.exe`); waits until it has exited and released the addon paks, holding the seat throughout; then frees the seat. `-Force` is for a person at the keyboard. |
| `gamelock.py status\|acquire\|bind\|may-drive\|release` | The game lock (below). |
| `crashdump.py --latest` | Minidump without a debugger: exception, module+offset, a stack scan and the strings on the stack. |
| `gamepaths.py game\|console_log\|reference\|work\|game_lock` | Prints a resolved location (used by the PowerShell scripts; importable from Python). |

Exit codes a script can act on: `launch.ps1` 0 launched, 1 timed out, 3 someone else holds the game, 4 a game it did not start is running; `console.py` 0 ok, 1 the run stopped (no game window, the probe got no answer, a `--wait-log` timed out), 3 the game is held, or is not the one this holder launched; `stop_game.ps1` 0 stopped, 1 still running (the seat stays held), 3 not this holder's game; `mod_session.py` and `qa_run.ps1` as `mod_session.py` prints them.

## One user of the game at a time

Pipelines share one game, so the harness has a game lock (`gamelock.py`): a JSON file, `<reference>\work\ingame-game.lock` (`MAGICIAN_GAME_LOCK` overrides; `magician install` reads the same file for its seat check), naming the holder and the scenario it is running.

- **Name your pipeline.** The holder is `-Holder` / `--holder`, else `$env:MAGICIAN_GAME_HOLDER`, else `anonymous`. Set `$env:MAGICIAN_GAME_HOLDER = "my-skin-qa"` once in your shell; every tool below reads it (`mod_session.py` takes it from the mod's `[ingame] holder`, else the slug). All unnamed callers share `anonymous`, so they don't exclude each other, but a named holder excludes them.
- **The game is bound to the seat.** `launch.ps1` records the pid and start time of the `deadlock.exe` it started in the lock (`bind`). Only the holder types into (`console.py`) or stops (`stop_game.ps1`) a game, and only that one: a game someone else launched, or one started by hand, is never driven or killed; `gamelock.py may-drive` says why when it refuses. Adopting your own instance is explicit (`launch.ps1 -Adopt`). On 2026-11-24 a pipeline whose seat had gone typed a stage plan into the next holder's game, and its fallback (`acquire` without checking, `taskkill /F /IM deadlock.exe`, `release --force`) killed that game; none of those steps is possible now.
- **Taking it.** `launch.ps1` takes the lock first and exits 3 while someone else holds it (`-Wait` waits, up to `-WaitSeconds`, default 1800). `console.py` takes it before typing anything (`--wait-lock SECS` waits; `--release-lock` hands the seat back after the run with the game up, and the next holder cannot drive that game). `mod_session.py` and `qa_run.ps1` hold it for the session (`--wait` / `-Wait` to queue) and release it at the end. A refused acquire is an exit, never a step to carry on past.
- **Giving it back.** The seat is free only once the game has exited: `stop_game.ps1` holds it through the whole stop, waits for the bound process to end and for the addon paks to be released, and only then releases it; a holder's `release` is refused while its game is still running or exiting (`--leave-running` hands it back over a live game on purpose). The lock is stale, and cleared by the next caller, when its game has exited and no `deadlock.exe` is running; a launch in progress (`launching`) holds it for at most 5 minutes before the game appears.
- **Handing back with work left.** A session that closes the game and then uninstalls or restores a pak (the cinema kit, `mod_session.py --restore`) must not let a waiting pipeline launch in between: it takes `acquire --hold 600` before the stop, stops with `-KeepLock`, finishes, then releases (2026-11-21: a waiting cinema session's fresh pak90 was uninstalled by the session closing before it).
- **By hand.** `python -I gamelock.py status | acquire | bind | may-drive | release [--holder NAME]`; `status` exits 0 free, 3 held. `acquire --hold SECS` keeps the seat held for that long even with no game running. `release --force` is for a person at the keyboard who has looked at the game; no script passes it. Tests: `python test_gamelock.py`.
- `capture.py` only reads the window and doesn't take the lock; take it (or be the holder) before you drive.

Scenario directives: `@wait s`, `@log text [timeout]`, `@shot name`, `@burst name frames fps [full]` (half resolution unless `full`; sheets pick the cell size), `@hold key name frames fps`, `@key name`, `@down key[+key]` / `@up key[+key]` (real keys held across later directives: motion for a screen recording with no console on screen; `a` and `d` did not strafe this way on this machine, `w`, `s` and `ctrl` do), `@mouse dx dy [secs]` (relative mouse move over the time: turns the inspect camera), `@wheel n`, `@entity <hero>` (the bot the last `citadel_create_unit` made, from the game's own `Created bot N/` line; `{entity}` in later commands), `@turntable steps yaw tag`, `@orbit steps radius cx cy z pitch tag`.

A mod's scenarios, plans and takes live with the mod (`<mod>/source/ingame/scenarios/` and so on; Maid Lash's `lash_*` passes are in its own repository), and its `[ingame] pass` names the default QA pass. The ones in `scenarios/` are worked examples on a stock hero: `enter_haze` puts the game in the sandbox as Haze (for driving `console.py` by hand; `mod_session.py` selects a mod's hero itself), `angles` (gameplay camera from -45 to 89 deg pitch: holes and seams a level camera hides) and `motion` (a run and a jump) work for any hero. `*_tmp.txt` are scratch and git-ignored.

## Recipe: a hero in the sandbox

```
python -I mod_session.py up --mod <mod folder> --out <dir>          # or by hand:
powershell -ExecutionPolicy Bypass -File launch.ps1 -Extra "+map new_player_basics"
python -I console.py --script scenarios\enter_haze.txt
```

- **Turntable:** `citadel_create_unit hero_haze` spawns a bot at the crosshair. `ent_find hero` gives its entity index. Then:
  - `citadel_bot_brain_disable_movement 1`
  - `citadel_bot_brain_disable_attacks 1`
  - `ent_setang <index> 0 <yaw> 0` for each step.
- **Camera:** `citadel_camera_overrides_enabled 1` with `citadel_camera_override_{x,y,z}_offset` and `citadel_camera_override_fov`. The x offset dollies the camera forward along the view, past our own hero.
- **Motion:** `+forward` / `-forward`, `+jump`, `+duck`, `+attack`, `+in_ability1..4`, `+reload`, each run through `console.py` around an `@burst`.

## What the game does (build 6864)

- **Sandbox map:** the Hero Sandbox is `new_player_basics`. `map hero_testing` (the early sandbox) crashes `client.dll` with a null read about 25 s after load, mods or not.
- **Console:** the key is the player's own bind (`cfg\user_keys_0_slot0.vcfg`, which `console.py` reads). Typing as clipboard paste or virtual keys is ignored. The console needs about 0.6 s to open before keys reach it.
- **Injected input:** held scancode keys reach gameplay (`@down w`, `@down ctrl`, checked on film 2026-11-20); virtual-key events do not. `@down` keys are released when the script ends, however it ends, and every run first releases any modifier or movement key still held from an earlier one (a stuck Ctrl turns every typed command into chords: [docs/reference/engine.md](../docs/reference/engine.md), Harness). `+command`s also work, with the console on screen for a moment.
- **Commands that work:** `selecthero hero_lash` works in the sandbox without cheats. `setpos` / `setang` / `ent_setang` / `citadel_create_unit` need `sv_cheats 1`.
- **Dead in retail:** `cl_drawhud`, `thirdperson`, `cam_idealyaw` / `cam_idealdist` and netcon (`-netconport`) do nothing. `citadel_hud_visible 0` hides the HUD; the HP bar on the hero stays.
- **`r_drawpanorama 0` hides the console too** (it is a Panorama panel), so its state can't be read off the screen. `console.py` then drives it blind: after a command containing `r_drawpanorama 0` (or the cinema kit's `cin_clean_full`) it toggles once per open/close and keeps the state in `ingame/.console-blind` until a command shows Panorama again. The file counts only for the game it was written under (it names the pid and must be newer than the game's start): a file left by a closed game is dropped with a note. Delete it if you restore Panorama by hand. The cinema kit (`../cinema/README.md`) has the rest of the clean-frame and camera facts.
- **`-dev`:** don't launch with it.
- **Hero preview:** an entity `hero_preview` exists in the sandbox. It may be usable for the hero-select pose.

- **The probe:** before a run's first command `console.py` types a token and waits for `Unknown command: <token>` in console.log; if it never comes the run stops and saves `harness_fail_game.png` and `harness_fail_desktop.png` in the run folder. `--no-probe` skips it. The game window is checked, and brought back to the front if needed, before every command and key.

## More facts (build 6864, learned the hard way)

- **One line, one frame.** A separate console command costs ~1.5 s (open, type, close). Anything timing-critical goes on one line with semicolons: `setpos 1300 -1450 280;setang 60 54 0;+in_ability1` casts Ground Strike mid-air; typed separately, Lash has landed.
- **Play area.** The sandbox kills or teleports a player set above about z 300 over the street ("out of the play area"); after a few of those the session can mark every position out of bounds. `map new_player_basics` again resets it.
- **Bots.** `ent_setang` on a bot only turns its aim; the body follows the animgraph's turn-in-place, so a bot can't be used as a turntable. `notarget` stops it re-facing you. `bot_mimic` does nothing to `citadel_create_unit` bots. Entity indices are reused, so read the index from the `Created bot N/` log line, never "highest index".
- **Dead or log-silent here:** `cl_particles_dumplist` (prints nothing to the log), `particle_test_start` (attaches to the looked-at entity; the world doesn't count). `thirdperson`, `cam_*`, `cl_drawhud` (as above).
- **Lash's camera-pose warning** (`near_01`/`near_02` "height overshoot ... Zeroing") is Valve's own: vanilla prints the same values. `logdiff.py` exists so findings like this are settled in one run.

- **Casting abilities.** A same-frame `+in_abilityN;-in_abilityN` casts nothing: hold it (`+in_abilityN`, `@wait 0.3`, `-in_abilityN`). Thrown abilities (a hex or a grenade) arm on the key and throw on a held `+attack`. `citadel_ability_cooldown_max 2` caps every cooldown (cheat), so a scenario can re-cast. A sandbox bot has no ult (nothing for an ult-stealing ability to take) until `citadel_bot_give_all_abilities`.
- **Seeing console output.** `find`, `cvarlist` and most command output never reach `console.log`. Run the command, then `@key console` (the console key, whatever it is bound to), `@shot`, `@key console`, and read the screenshot.
- **Spawning a particle directly.** `particle_test_file <path.vpcf>` then `particle_test_start` plays it on the entity under the crosshair, bypassing ability code: it proves a system renders, apart from whether its parent fires.

## Scenario techniques

- **Sound check without audio:** `snd_showstart 1` prints every started `.vsnd` into `console.log` (`[SoundSystem] [(cl) N] sounds\...`). It proves an event fired and when, not that it is audible.
- **Archived camera cvars:** `citadel_camera_override_*` and `citadel_camera_inspect_mode` persist in the user's config; put them back (`-135/-34/0`, fov 90, inspect 0) before quitting.
- **Slow motion on a cast:** cast at real speed with the key held (0.45 s for a deployed summon) and turn slow motion on in the same console line as the release, through an alias.
- **A thrown ability in slow motion:** arm at real speed (0.4 s), wait 0.5 s, then `+attack;host_timescale 0.2` on one line and `-attack` 0.2 s later.
- **Orbit, don't offset:** the inspect orbit (`citadel_camera_inspect_mode 1`, `@mouse dx dy`) frames a hero or a summon where override offsets framed empty street.

## Camera field notes

Dated observations from QA and audit passes looking for free camera angles on build 6864. The scenario and driver files they name are the mods' own and are not in this repository; the techniques are what carries over, and `mod_session.py`, `takes.py` and `../cinema/replay.py` now do the driving they describe. They record what was seen that day; numbers such as mouse units per degree varied between sessions, so re-check before relying on one.

### Free camera for QA: one SourceTV take, many angles (Maid Lash audit, 2026-11-20, build 6864)

The route that gave real free-camera angles on our own hero in motion, in one game session. Tools (Maid Lash's own): `lash_audit_live.txt` (the take), `lash_audit_demo.py` (writes the playback scenarios), `lash_audit_stops.txt`.

- **Record while you play.** `launch.ps1 -Extra "+tv_enable 1 +tv_delay 0 +tv_transmitall 1 +map new_player_basics"` works: the log shows `Starting replay recording` / `Recording SourceTV replay` at GameInProgress and `citadel\replays\auto-<date>-new_player_basics-4-<name>.dem` appears (file creation = recording start). Play every move live with gameplay-camera bursts, `tv_stoprecord`, then `playdemo replays/<file>` in the same session (`@log Playing Demo`). A second `map` in the same session starts a second take, so name the take explicitly afterwards (`lash_audit_demo.py --take`).
- **`echo` does not reach console.log.** Stamp moments some other way. What worked: the burst frames' write times (a burst grabs everything, then encodes; f000's mtime minus frames/fps is the grab start, about a second late) and the `prediction distance N - teleporting` lines every `setpos` prints. Map wall time to demo ticks at 64/s from the take's creation time, then **calibrate once**: on this take the demo ran about 9 s ahead of that estimate (`--offset -9`). Find it with `spec_player 1; spec_mode 4` (the camera of your own hero, HUD included) at two or three candidate ticks and look for the move.
- **`spec_pos` in `spec_mode 4` prints the player's third-person camera**, not the hero: `spec_goto 1475.6 -1301.3 110.8 -45.0 54.0` with the hero standing at `setpos 1494 -1196`. Use it for timing, not for the hero's position.
- **Framing.** The spectator lens is wide: at 170 units a 100-unit hero fills about a quarter of the height, so 100-110 units is a close enemy. `spec_goto` on a spot within about 40-60 units of the hero is refused (the camera stays where it was): no extreme close-ups, crop full-resolution frames instead. A spectator eye below the floor (`spec_goto` z under about -40 at this spot) sees the hero from under the street with the floor not drawn: an "under the hem" view that the gameplay camera cannot give. A camera inside a prop shows the prop's underside; check every new spot with one still first.
- **Motion from a fixed camera.** Per moment: `demo_pause`, `demo_gototick T`, `@wait 4`, `spec_goto ...`, `demo_timescale 0.5`, `demo_resume`, `@burst ... 6` (half resolution, about 45 s a camera with the encode). The cloth is simulated again on playback from the recorded animation, which is what another client sees.
- **Keys held across a burst stay held for the whole encode.** `+forward` before a 12-frame full-resolution burst and `-forward` after it ran the hero 20-25 m into a wall, so the "stop" filmed was a wall stop. Release keys before the burst that films the result (`lash_audit_stops.txt`). `+jump` on its own line before a burst did not jump; `+jump`, `@wait 0.3`, `-jump` (as in Ground Strike) did. Run then `+duck` did not slide at that speed, and Grapple without a target cast nothing.
- **Don't pass an empty argument to an argparse script from PowerShell** (`--skip ""` aborts the whole step); a failed playback step then falls through to whatever the driver does next.
- **`stop_game.ps1`'s console `quit` worked** (`MainLoop returning`, `Source2Shutdown` in the log) but deadlock.exe lingered about a minute after it, past the script's wait; the next pipeline's launch then cleared the lock as stale. `stop_game.ps1` now holds the seat until the bound process has exited (2026-11-24).

### Free camera for costume audits (2026-11-20 and 21, a second skin's audit)

What was tried for a free look at a hero in motion, build 6864:

- **Inspect orbit** (`citadel_camera_inspect_mode 1`, `@mouse dx 0`): about **15.5 mouse units a degree** here (8 x `@mouse 350` turned about 180 degrees), so `@mouse 2800` is the front from behind. `@mouse 0 -250` drops the camera for a low view, `0 500` raises it. `@wheel` does not zoom it. The hero holds his pose while it is on: `+jump`, `+attack2` (melee) and abilities typed in inspect mode do not play; `+reload` does (after a shot, so the magazine is not full).
- **The inspect orbit keeps its angle while the hero moves** (2026-11-21): orbit to the side or the front, then hold `w` (`@down w`), dash (`@key shift`): he runs and dives under a fixed-angle follow camera, which is the side and front view of locomotion without SourceTV. Crouching pulls the camera in onto his back. The mouse turns less under a long lens (`@mouse 2800` at `citadel_camera_override_fov 30` is about 60 degrees, not 180): orbit with the overrides off, then set the lens.
- **Inspect mode 2** did not lock the camera for a run-past in this session (`+forward` under it filmed the normal follow camera). Unproven, not dead.
- **`host_timescale 0.25;+jump` on one line** did not jump; set the timescale on its own line first. `+in_innate1` (the dash) did nothing standing still; it probably needs a held direction (`+forward;+in_innate1`).
- **Override dolly onto a bot** (`citadel_camera_override_x_offset 60`): the camera ends inside the bot's near-fade range and the bot draws see-through, even with `citadel_camera_fade_other_near_dist 0`. Keep a bot 300+ units out and narrow the fov instead.
- **Range shots by `setpos` along the sandbox street:** 500+ units back from `1300 -1450` is "out of the play area" (the blue field). Range and LOD need another street or a SourceTV camera.
- **`kill`** shows the first-person death camera; the body is not in frame. A death needs a SourceTV camera or a bot that dies.
- **SourceTV from the launch line:** `launch.ps1 -Extra "+tv_enable 1 +tv_delay 0 +tv_transmitall 1 +map new_player_basics"` records from map load (`[HLTV Server] Recording SourceTV replay` in console.log gives the start time; ticks are 64 a second of *game* time, so a take with `host_timescale` changes does not map to the wall clock). An 18-minute take with a `citadel_create_unit` bot, slow motion and a death in it broke on playback: the first `demo_gototick` was still skipping ("Current playback is 0") when the second was typed 14 s later, the demo restarted ("Host activate: Playing Demo") and the client logged `FATAL ERROR: CopyNewEntity: invalid class index (893), out of range 0`, stuck on the loading screen with the console dead; `stop_game.ps1` could not quit it through the console and the process had to be killed. Keep takes short, with no bots, at real speed, and seek once, then wait for the skip to finish (`@log "Skipping finished" 30`) before the next command. `echo` lines do not reach console.log, so mark beats with `@shot` (file times) instead.
- **Killing the game** loses the archived cvars set in that session (the user config keeps its last saved values), which is the safe direction.

### One ordinary session instead of a SourceTV take (Maid Lash, audit pass 3, 2026-11-21, build 6864)

What gave the views the audit lacked without a demo (Maid Lash's own `lash_pass4_audit.txt` and `lash_pass5_audit.txt`):

- **A bot dies on command:** `ent_fire !picker sethealth 0` (cheat) kills the unit under the crosshair; it drops, lies about 5 s and respawns where it stood. `ent_fire <index> ...` does nothing.
- **An enemy bot faces you** (`citadel_create_unit hero_lash enemy`): the front of the outfit without an orbit. A unit made without `enemy` is friendly: it keeps its own facing and Death Slam does not take it.
- **Our own hero from the front, crouched:** `+duck`, `citadel_camera_inspect_mode 1`, then `@mouse 700 0 0.5` steps with a still after each. In this session 700 units were 30 degrees (12 steps a full turn; 15.5 units a degree was measured on another day), so shoot a whole ring rather than trusting one number. `@mouse 0 -260` drops the camera for a second ring from low.
- **Slide:** `+forward;+in_innate1` (dash), 0.3 s, `+duck` held: he slides on the flat street. Sprint then duck is a crouched run.
- **The heroless pawn floods console.log** with `OnPostPredictionError ... teleporting`, 64 lines a second, from map load until a hero is selected. `launch.ps1` read the last 60 lines every 3 s for its "loaded" line and timed out on a loaded game (the first session of that audit, 10 minutes lost); it now reads the last 6,000. An enter scenario's `@log ChangeGameState: GameInProgress 300` waits its full 300 s when the state was reached before the script started: wait for the line in the session driver, then select the hero without the `@log`, and refuse to film unless `Loaded hero N/hero_<name>` is in the log (`mod_session.py up` does exactly this).

