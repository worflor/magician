# Cinema engine

An optional, dev-only film kit for trailers and stills. Write a video down as a scene file (shots, actors, camera
moves, timing, cuts, slow motion, cues), see it at once as an animatic, then have the engine render it from SourceTV
takes of the retail game with your mods installed, frame by frame and exactly, into an edit-ready video. The whole
city is the stage (`dl_midtown`), the heroes in the takes are the actors, and one scene can cut between several takes.

It stands on the harness's seam to the system (`ingame/gameos.py`): on Windows it records with Desktop Duplication or GDI, on Linux with ffmpeg's `x11grab` of the game's window, and its frame grabs are the harness's own. On Linux that is proven against a test window (`test_capture.py`), not yet with the game ([README.md](../README.md#platforms)). Nothing else in Magician depends on it, and it depends on more than the rest: Python 3.11 or newer (scene files are
TOML), ffmpeg on PATH, a Python venv with [Boon](https://pypi.org/project/boon-deadlock/) for actor tracks
(`CINEMA_VENV_PY`, or `cinema_venv = <dir>` in the config file), the CSDK to build its pak, and the in-game harness.
Output goes to `<reference>/work/cinema/` (`CINEMA_WORK` moves it). The commands below run from this folder.

**Dev only.** The `cinema_dev` pak (pak90) is installed for a session and uninstalled before the seat is handed back;
it never ships with a mod and is never installed while anyone plays a real match or runs a mod's QA pass. Every
session takes the game seat as `cinema` (`ingame/gamelock.py`), runs `exec cinema_reset` and puts the player's
`fps_max` back before the game quits, and keeps the seat until pak90 is verified gone.

Everything marked proven was tried in the retail game on build 6864.

**Films** made with it, each a lossless `master.mkv`, a `preview.mp4` and edit files under `films/<scene>/`. A mod's
scenes live with the mod they are for; this repository carries the neutral example:

| Film | Length | What it shows |
|---|---|---|
| `example_run` | about 10 s | A stock-hero example (`scenes/example_run.toml`): Haze runs the Hero Sandbox street, a bullet-time orbit, then slow motion away |
| The Maid Lash trailer | 56 s | Six cuts, from a `[blocking]` animatic to the final cut: the tempo map, sound design, grades, cards, layers and the splice below were all built for it. Its scenes and production notes live with the skin |

## Scene guide

### A scene in one screen

```toml
[scene]
name = "example_city"                                         # the output folder (letters, digits, _)
take = "auto-20261121-1003-dl_midtown-4-player.dem"          # the main SourceTV take (citadel/replays)
fps = 32                                                     # 32 = two demo ticks a frame = real time exactly

[takes]                      # more takes, by name: a shot films from one with take = "other"
other = "auto-20261121-1017-dl_midtown-4-player.dem"

[actors]                     # your names for the heroes (track.py list <take> shows them), one for every take
maid = { main = "Lash#0", other = "Lash#1" }                 # or a table per take
haze = "Haze#0"

[[shot]]
name = "bridge_run"
tick = 6276                  # the demo tick the shot starts on
length = 2.3                 # seconds on screen
[shot.camera]                # a preset plus what differs (or just: camera = "chase")
preset = "chase"
actor = "maid"

[[shot]]
name = "hex"
take = "other"
tick = 10410                          # or "cast lash down_strike -24": the take's first cast of it
length = 3.0
speed = 0.25                          # quarter speed, frame by frame (paced)
cut = "dissolve 0.3"
cue = "music: the drop"     # lands in the edit's cue track
[shot.camera]
type = "dolly"
from = [2395, 385, 482]
to = [2440, 352, 470]
look = "haze"
```

`scenes/example_run.toml` is a complete scene on one take. Its header says how to record the take it needs
(`magician session up` records SourceTV by default) and which ticks to fill in; `check` refuses it until the take's
file name is yours.

### The workflow

```
python -I film.py check      scenes\x.toml               # every frame resolved; mistakes name the shot and the key
python -I film.py animatic   scenes\x.toml               # timing and framing as a video, no game (films\x\animatic.mp4)
python -I film.py plan       scenes\x.toml               # a top-down map of the camera paths and actors of one take (plan.png)
python -I film.py render     scenes\x.toml --stills 3    # in game: three frames a shot, no pak rebuild
python -I film.py photomatic scenes\x.toml               # the stills at the film's timing: graded, cut, scored (photomatic.mp4)
python -I film.py make       scenes\x.toml [y.toml ...]  # in game: build, render, encode; several scenes share one session
```

Write, `check`, watch the `animatic`, fix; then `--stills 3` for the real light and walls (`--shots a,b` for the
shots you changed); `photomatic` to see them at the film's timing with the music (the cut to approve); then `make`.
`make` is `build` (the alias chains into pak90), `render` and `encode`, which `film.py --help` lists one by one with
`lossy` (the preview and phone copies again from the master). A stills pass of a 30 s scene is about 7 minutes in
game; a full render of it about 12 (0.3-0.5 s a lockstep frame, 0.25 s a paced frame, 20 s a change of take).

### Time

| Write | Means |
|---|---|
| `tick = 4228` | the demo tick of the first frame |
| `tick = "after"` | one step past the last frame of the shot before (continuous action across a cut) |
| `tick = "cast down_strike -16"`, `"cast maid down_strike"` | the first cast of that ability in the shot's take (by that actor), offset in ticks |
| `length = 2.5`, or `until = "cast maid down_strike +40"` | seconds on screen, or the tick to end on (constant speed) |
| `speed = 1`, `0.5`, `0.25`, `0`, `[[0, 0.5], [0.4, 0.5], [1, 1]]` | real time, half, quarter, frozen, or a ramp over the shot |
| `take = "other"` | the shot films from that take of `[takes]` (`main` is `[scene] take`) |
| `warmup = 1.5` (`[scene]` or a shot) | seconds of the take played continuously up to the first frame, so cloth arrives simulated (default 1.5; 0 starts cold from the seek) |

Each frame advances `speed * 64 / fps` ticks. At fps 32, speeds 1 and 0.5 are whole ticks (lockstep). Anything
between ticks (0.25, a ramp) renders **paced**: frame by frame, the demo moved exactly that fraction of a tick per
frame (`render = "substep"`, the default for such shots; `render = "realtime"` forces the screen-recorded route).

### Cameras

| Preset | What |
|---|---|
| `side_track` | travels beside the actor, from its right, a little ahead |
| `chase` | behind and above |
| `lead` | in front, backing away |
| `low_hero` | low, front three-quarter, looking up |
| `hero_orbit` | a 120 degree arc round the actor's front |
| `bullet` | a 180 degree arc, exact follow (with the demo moving; at speed 0 the lens holds and only turns: below) |
| `crane_up` | rises from street level to 260 above |
| `push_in` | moves in from 320 to 150 in front |

A shot's `[shot.camera]` names a preset and overrides any key; a scene adds its own as `[presets.<name>]` tables.
The types underneath:
- `fixed`: `pos`, and `look` (a point or an actor: a tripod that pans) or `ang = [pitch, yaw]`.
- `dolly`: `from`, `to`, `look`.
- `keys`: `keys = [{t = 0, pos = [...], look = "maid"}, ...]`, a smooth path through them; looks may mix actors and
  points (a tilt from an actor up to the skyline), or every key gives `ang`. `path = "segments"` goes key to key
  instead of through a spline: each segment eased on its own (a key's `ease` is the ease of the segment arriving at
  it), the position and the view angles lerped, held before the first key and after the last. That is a move that
  starts and settles exactly on its keys, one way only; a Catmull-Rom between two looks leaves and arrives at speed,
  so it cannot settle.
- `orbit`: `around`/`actor`, `radius`(`_to`), `height`(`_to`), `from`, `to` (degrees; `relative = "heading"`: 0 in
  front, 90 its left), `look_height`.
- `track`: `actor`, `offset = [forward, left, up]` (`offset_to`), `lead`.
- Every camera: `ease` (linear, in, out, inout, smooth), `look_height` (default 45), `smooth` (ticks of averaging on
  an actor; 0 for fast moves).

Points are world units `[x, y, z]`, z up; the camera position is the lens. Valve's composed cameras drop in as
`fixed` (`pos` and `ang` from `python -I stage.py camera dl_midtown intro_camera_amber_1`) or as the first key of a
push.

### The rest

- `cut = "cut" | "dissolve 0.4" | "fade 0.4" | "dip 0.4"` (from black / through black).
- Hits in the encode: `flash = [[at, strength, decay]]` (a white hit weighted to the bright parts, decaying), `shake = [[at, pixels, decay, hz]]` (a jolt on decaying sines over a cropped margin), `punch = [[in, peak, hold, out, zoom, x, y]]` (a crash zoom onto a point of the frame). Shot time, seconds or musical.
- `zoom = 1.5` or `[1, 2]`: a crop (the spectator has no lens control). There is no supersampling: the game keeps
  its window at the monitor's size (below), so a crop costs resolution.
- `[defaults]` (and `[defaults.camera]`) apply to every shot.
- Cues: `cue = "..."` at a shot's start, `cues = [[1.0, "sfx: slam"]]` within it, `[[cue]] at = 0.0 text = "..."` on
  the film clock. They become `cues.srt` beside the film.
- `[scene] look = "clean" | "hud"`, `world = "map" | "void"`, `output = [1920, 1080]`.

### Music, sound, grade, cards and the plan

- **A tempo map** shared with the score: `[music] bpm = 120, meter = 4, start = 0.0, file = "score.wav", gain = -2,
  snap = true, grid = "beat"`. Then any length, cut length, cue or sfx time may be musical: `length = "1 bar 2 beats"`,
  `cut = "dissolve 1 beat"`, `at = "bar 5 beat 3"`. `check` warns when a cut misses the grid by more than half a frame
  (a dissolve or a snip overlaps the shot before: give its beat back to that shot), and the summary prints each shot's
  bar.beat. At 120 bpm and fps 32 a beat is exactly 16 frames.
- **Sound as designed events**: `[[sfx]] at = "bar 8", file = "...", gain = -1` (or `shot = "name"` and `at` within
  it). The music and every event are mixed (no normalisation: the gains are the mix, then a limiter) into
  `master.mkv` (FLAC), `preview.mp4` (AAC) and the animatic, and listed in `cues.srt`. The frames carry no game audio,
  so the game's own effects and voice lines come in as decoded files (dev only: Valve's audio never ships). Paths are
  relative to the scene file, `$VARS` expanded.
- **Grades**: `[scene] grade = "parlour"`, a shot's `grade = "brawler"` (or `"none"`): named ffmpeg looks in
  `grade.py` (`parlour`, `brawler`, `fever`), applied per shot before the transitions. `python -I grade.py stills
  out.jpg a.png b.png` makes a look book.
- **The snip**: `cut = "snip 1 beat"`: a gold thread is cut across the frame with a pair of scissors riding it, then
  the outgoing shot parts top and bottom onto the next. It is one xfade with a custom expression, written without
  `st()`/`ld()` registers: xfade runs its expression on several slice threads, and shared registers rendered as noise.
- **Cards**: a shot with `card = { title = "...", sub = "...", small = "...", style = "house" | "brawler" | "end" }`
  and a length is drawn by the encoder (no take, no camera); the animatic shows the real card.
- **Layers** (the split screen, picture-in-picture, comic panels): a shot with `layer = true` is filmed like any
  other but takes no screen time; a host shot composites it with `[[shot.layers]]` tables: `src` (the layer
  shot), `crop = [x, y, w, h]` of its frame, `size = [w, h]` of the output, `keys = [[t, x, y, ease], ...]` (the
  panel's top-left in fractions of the output, eased between keys: linear, in, out, inout), `border` and
  `border_color`, `hold = true` to keep its last frame on screen once it ends. The layer's first frame is the
  host's first frame; later layers draw on top. Times may count frames: `"2 bars 6 frames"`.
- **Prints** (a collage): a layer with `card = "polaroid"` is mounted on a print (an ivory border, a deeper foot
  with its `caption` in Palatino italic, a soft shadow) and its keys may carry a scale and a rotation,
  `[t, x, y, ease, scale, degrees]`, with x, y the card's centre. It is drawn from its first key on (it snaps onto
  the frame there) and may fly off the frame. `image = "x.png"` takes a still from a file instead of a layer shot
  (held for the whole host). The warp is ffmpeg's `perspective` on a transparent frame the size of the output, so a
  card can turn and scale on any key; a white `flash` on the host hits the cards too. Two ffmpeg traps found building
  it: `fps` after an endless stream (a looped or padded source) waits forever, and `tpad` before `fps` pads nothing;
  the card source is looped, then trimmed to the host's frames, then timed.
- **The plan before the take**: `[scene] take = "blocking"` and a `[blocking]` table (actors' keyed paths in seconds
  of the future take, `casts` for `tick = "cast ..."`, `[[blocking.set]]` wall outlines and `[[blocking.props]]`
  boxes, both drawn by the animatic with the actors' names) give a full animatic and `check` before anything is
  staged. `render` refuses a planned take.

Beside every film: `master.mkv` (FFV1, lossless), `preview.mp4` (x264), `edit.edl` (CMX 3600, one event a shot),
`shots.csv`, `slate.srt` (shot names over their shots) and `cues.srt`.

### Framing rules learned the hard way

Keep cameras in the open: a camera in a wall shows one colour (the render log warns per frame) and `spec_goto` refuses
spots within about 50 units of a hero (`check` warns). Cut rather than fly through buildings. Follow fast actors with
`smooth = 0`. Check a frozen shot's middle still, not only its ends. In the city:
- **Look at stills before you trust a camera.** Nine of the eleven cameras of one city film moved after the first
  stills pass: a chase that starts behind a runner who has not set off yet sits in the wall behind her; a camera 16
  units over the bridge walkway looked at it from the street below; an orbit at height 30 went through the balustrade.
- **Stills are warmed** like shots: a still seeks `warmup` early and plays up to its tick. A cold seek drops every
  effect an event spawned before it: cold stills of a Death Slam showed the hero and none of what it threw.
- **Frame where the ability lands, not where the hero stands.** Death Slam throws its catch toward his aim: aimed
  level it lands about 1,200 units ahead, 38 degrees down about 350, and he lands back on his own mark 70 ticks
  later. Grapple pulls him up and over its target (z 376 to 750 in a second, 800 units west in one take).
- **A frozen shot cannot travel.** In a paused demo `spec_goto` turns the spectator but does not move it: the frames
  of a frozen orbit (50 to 115 degrees round Lash) matched a lens held at the orbit's first position and turning, to
  within 20-50 px of the projected actors, and the "orbit" swung past its subject to a wall. `check` warns when a
  lockstep frame that does not step the demo moves the camera. Hold the position and pan (`keys` with
  `path = "segments"`), crop with `zoom`, or let the demo move.
- **Freeze where the actor is slow.** A bullet-time frame mid-dive (2,000 units a second) puts her off-centre:
  the seek and Boon disagree by a tick or two. The top of a leap holds still.
- **The tracks are the feet; heroes in the air sit higher than Boon says**: raise `look_height` (115 at the apex).
- A walker who has not started yet has his standing yaw as `heading`: `relative = "heading"` orbits and chases turn
  with the camera he looked through, not with the run to come.

### Staging a take in the city

`session.py --serve <inbox>` holds the game and runs step files dropped into the inbox (`.txt` console lines and
directives, `.py` with `s` the session) until `quit` or an idle timeout. A city take is staged in these steps:

1. Before the map: `citadel_solo_bot_match 1;citadel_spawn_practice_bots 0;citadel_active_lane 0;citadel_allow_duplicate_heroes 1`,
   then `map dl_midtown`; wait for `ChangeGameState: GameInProgress` (SourceTV starts recording there).
2. The Choose Hero overlay: click a team (The Hidden King at 760, 583; The Archmother at 1160, 583, in the 1920x1080
   client), then a hero in the grid, which is alphabetical (columns at x 111 + 83 n, rows at y 213, 345, 478, 612,
   745): Lash at (194, 478), Sinclair at (609, 612).
3. `sv_cheats 1;god;notarget;citadel_ability_cooldown_max 2;citadel_hero_testing_enabled 1` (the last brings the
   Testing Tools panel to the city), `citadel_give_gold 60000` (level 35, 32 ability points), then hold Tab and click
   each ability icon (x 825, 915, 1003, 1093 at y 1020) twice: every ability unlocked.
4. Act: `setpos`, `setang`, `+forward`, `setpos ... 950;setang 45 50 0;+in_ability1` (a Ground Strike from the sky),
   `citadel_create_unit hero_lash enemy` (a bot where you aim), `citadel_bot_brain_disable_movement 1`, ... Then
   `tv_stoprecord`.
5. **One hero a take.** `selecthero` does nothing in a city match: stage each hero in its own take and cut between
   them (`[takes]`).
6. Stage at home: an enemy shrine's guardians kill even a `god` player (Lash died in the Archmother's shrine, 2,552
   damage); a hero on the Archmother's team walked it safely.

Bots for staging (proven by Boon's tracks): `citadel_bot_zig_zag 1` moves bots side to side (x 1234 to 1623 at a
fixed y), `citadel_bot_forward_back 1` back and forth (y 6662 to 7691), `citadel_bot_use_ability 1` with attacks on
makes them cast (one bot threw three bolts, 320 ticks apart). They act on **every** bot at once: switch them on only
for a beat, then `citadel_bot_brain_disable_movement 1`. Also there and untried: `citadel_bot_jump`,
`citadel_bot_crouch`, `citadel_bot_melee`, `citadel_bot_shoot`, `citadel_spawn_trooper_wave` (troopers in every
lane), `citadel_assume_pawn_control` (take over the pawn you aim at), `citadel_bot_record` ("bots mimic your commands
as well as record them to be replayed"; `citadel_bot_playrecording`).

## How a frame is made

**Every shot starts warm.** A seek is a teleport, and the client's cloth solver is never told: a skirt stretched
across the gap for a frame or two and arrived lagging the legs. So no shot starts at its seek: the driver seeks
`warmup` seconds early (1.5 by default) and plays the take continuously up to the shot's first tick, and nothing
inside a shot ever seeks. After every seek the driver also waits until the picture holds still (a forward seek can
report nothing and play on for seconds), and every chain opens with a lead alias that sets the first camera before
anything is captured (the first tap after a seek did not always land). After each shot `film.jumps` checks the
frames: one that jumps away and snaps back (or a pair) is flagged in the render log before the encode.

**Lockstep (whole ticks, proven).** The demo is paused. Each frame is one alias of a chain baked into the cinema pak
(`cfg/cinema/<scene>_<shot>.cfg`): `demo_step_tick N` and `spec_goto x y z pitch yaw`. A key bound to `cin_next` runs
the next alias; the driver taps it, waits the settle time (0.15 s plus the ticks stepped), captures the window
(PrintWindow) and pipes the pixels into a lossless FFV1 file per shot (`FrameSink`). Its warm-up is one
`demo_step_tick <warmup ticks>` from the early seek.

**Paced (between ticks, proven).** `host_framerate` drives demo playback: with it set, every rendered frame moves the
demo exactly that many seconds, whatever the wall clock does. The driver slows the game to about 5 rendered frames a
second (`fps_max 6`), and the **Panorama bridge** plays the shot one sample a rendered frame: the camera and
`host_framerate <the frame's step>` on one line, after a 4-frame pre-roll that resumes the demo and the warm-up (two
ticks a rendered frame at the player's own frame rate, then `fps_max 6` from the bridge). Each frame the bridge logs
`cinema_bridge: frame <k>`; the picture of frame k is on screen two rendered frames later (`PACED_LAG` 3, measured),
and the driver grabs it half a frame period after that line. Quarter speed is half a tick a frame with every frame
distinct (64 frames, 0.23 s each), and a ramp is a list of steps.

**Realtime (the fallback).** Seek, set `demo_timescale`, park the camera on a slate, start `record.py` (60 fps), play
the shot through the bridge (a camera every rendered frame, F8) or by key taps, stop; the shot is cut from the
recording from the slate's jump. A paced shot whose bridge does not answer falls back to it.

**The bridge.** `bridge/cinema_bridge.js`, baked into the pak with every paced and realtime shot of the scenes built
(`cinema_paths.js`). It rides Valve's replay HUD (`replay/citadel_hud_replay`, part of every HUD: any map's demo,
loaded already at the main menu) and the sandbox's Testing Tools layout; where both load, the second copy stands down
(a flag on the root panel), so a key press is taken once. Keys: F11 arm the first path, F9 the next, F8 play/stop,
F10 self-test. It answers under the clean look (`r_drawpanorama 0`).

**Typed.** The same frames typed through `console.py` (3.3 s a frame, about 10 s a still with its seek): stills and
fixes without a pak rebuild.

**The animatic (offline).** `animatic.py` draws every frame from the same data: a ground grid and the actors as
pillars through the camera (the spectator's lens: 90 degrees at 4:3), shot slate and cues burned in, assembled with
the same cuts. 30 s of film in about 20 s. It cannot show the map (the take has no geometry) or animation.

## Proven and measured (build 6864)

| What | Result |
|---|---|
| The drift, solved | The free camera crept into the sky on every typed path. Cause: **Left Shift is the demo spectator's Fly Up**, read even while the console is open; every underscore typed with Shift kicked the camera up, and `spec_goto` keeps the velocity (32 underscores: z 100 to 1136). `console.py` types with **Right Shift**: `spec_pos` read back exactly 100.0 |
| `spec_goto` z is the eye | An old "+61" offset was the drift: `spec_goto ... z 2` puts the lens at street level |
| Key route, full 10 s move | 300 frames, no drift, **0.25 s a frame** (86 s) |
| A whole scene | a 270-frame scene 150 s in game, seeks and set-up included; a 243-frame one 131 s |
| A full `make` through the FFV1 sink | A 552-frame scene in 6 shots at 0.31-0.54 s a frame (3.6 min in game, seeks included), master 752.7 MB FFV1 for 17 s, preview 22.7 MB, the edit files beside it; shot frames never touch the disk as PNGs |
| `demo_step_tick N` | Plays N ticks and pauses: time moves frame by frame without a seek |
| Seek | 1-2 s; wait for `Demo Skipping finished`. **After `demo_step_tick` a long forward seek stalls**: `demo_resume;demo_pause` on one line first clears it. A forward seek can also print nothing and play on to its tick for seconds (a frozen dive once filmed the demo catching up from before the cast to the landing), so the driver waits for a still picture. A short forward seek (a few ticks) does not skip at all: the demo plays to its tick and on past it, so `film.py` steps a forward move of up to 320 ticks from a known paused tick (`plan_seek`) and sets the shot's first camera before the still check |
| Demo start | A pause or seek before `Host activate: Playing Demo` crashed the client (`CopyNewEntity: invalid class index`); `film.py` waits for that line |
| Boon | `boon-deadlock` 0.19.0 (in its own venv): its ticks are `demo_gototick` ticks; tracked heroes land centred; casts by tick; reads `dl_midtown` takes (21,602 ticks, 3 heroes, 8 casts in one) |
| `tv_snapshotrate 64` | The take records every tick |
| Clean frame | `cin_clean_full` hides HUD, corner text and every Panorama panel; the pak's override removes every overhead bar. The "Teleport to Hideout" door label still draws in demos |
| Panorama bridge, everywhere | Loads from the replay HUD at the main menu and in sandbox and `dl_midtown` demos (`loaded in hud_replay`; the Testing Tools copy logs that it stands down). **Global** key binds fire on injected keys; `$.DispatchEvent("CitadelConCommand", ...)` runs a console command every rendered frame |
| The bridge under `r_drawpanorama 0` | Arms and plays with Panorama hidden: keys and per-frame commands still run |
| Realtime by the bridge | A quarter-speed orbit: 684 cameras in 4.002 s (171 a second, worst gap 19.6 ms), clean; the cut shot repeats 7 of 119 frames (key taps: 14 of 119; lockstep: none); `record.py` holds the shot about 60 s after the take while the FFV1 recording drains |
| `host_framerate` in a demo | 1 ms a rendered frame for 5.4 s of wall time moved the demo about 55 ticks (unforced: 345); the demo follows it frame by frame |
| Paced slow motion | Quarter speed, 64 frames at 0.23 s a frame, `fps_max 6` measured at a 200 ms period: every frame distinct (frame-to-frame change 0.4-2.5 of 255, no repeat), frame 4j matching the lockstep frame j of the same ticks; the picture trails the bridge's line by two rendered frames (calibrated: `PACED_LAG` 3) |
| Two heroes in one shot, in the city | Maid Lash's Ground Strike and Death Slam beside another hero's bot on the bridge; that hero's thrown ability at a Maid Lash bot |
| `dl_midtown` | Staged as a solo bot match with SourceTV, filmed through Valve's cameras and the engine's; recipe above |
| Valve's cameras in the city | All 18 framed in a take, FOV 90 each. The four intro cameras frame the Patrons' shrines (`intro_camera_amber_1/2`, `intro_camera_sapphire_1/2`: the best views of the map); `screenshot_camera_02`, `03`, `04` and `1288310_screenshot_camera_00`, `01`, `03` frame usable streets and rooms; the rest look into props or walls (`1288310_..._05` is inside a crate). Two more, `patron_camera_amber/sapphire` (`point_camera_vertical_fov`), look down on the Patrons from z 2048 |
| Cloth: the outfit or the engine | A skirt's thighs came through its front on the stride: seen **live** (no demo, no stepping: an enemy bot of the hero running at the camera) and in the same side-view stride filmed by lockstep, paced, real-time playback (speed 1, no stepping, no seek in the shot) and with `sc_force_lod_level 0` alike. That was the outfit, not the engine and not LOD; before blaming the engine for cloth, film it live. The stretch for a frame or two after a seek is the engine's: fixed by the warm-up and the lead alias, caught by `film.jumps` |
| The warm-up, before and after | The same city shots rendered from the seek and warm: from the seek, frame 0 of five shots kept the shot before's camera and one opened on a flared, stretched skirt; warm, every first frame is the shot's own camera, the skirt is in step, and Death Slam's chains are already in the air (a seek had cleared them). `film.jumps` flags none of the 11 shots |
| `sc_force_lod_level 0` | Accepted (scenesystem); no visible change on a hero at 220 units |
| Extras in a city take | `citadel_create_unit hero_bebop enemy`, `hero_tengu friendly` (the team word works), `hero_vampirebat enemy`: Bebop, Ivy and Mina standing in Crowley's diner beside Lash, brains off |
| Setting a bot's health | `ent_fire !picker sethealth 30` took Viscous from 780 to 30 (Boon's health track); after `ent_setname cin_v1` (aimed at the new bot) `ent_fire cin_v1 sethealth 100` set Haze to 100. Health regenerates about 2 a second |
| One Death Slam, two bots | Two enemy bots 220 units east of Lash outside Crowley's, health set to 60 each by aiming at them from 90 units (`setpos` in front, `setang 12 0 0`, `ent_fire !picker sethealth 60`): both latched and lifted together (2.4 s after the press, z 376 to 600 in 0.6 s), slammed 1.6 s later about 960 units in the direction he faces, about 142 damage each, and died one tick apart. A bot at 57 or 100 dies, one at 730 lives. The blue Walker east of Crowley's shoots enemy bots that land near it |
| A remapped clip plays | With a skin's emote at `weapon_crouch_idle` in pak90, `hurtme 20` then `+duck` showed the hero standing upright in the emote instead of crouching |
| `+jump` typed through the console | Did not jump in three tries (Boon's z flat), while `+forward;+in_innate1` dashed: to get Lash airborne for the air dash, `setpos` him into the air |
| `prop_dynamic_create <path>` | Takes the path without `models/` and `.vmdl` (it adds both). A model the map has not loaded spawns as the red ERROR model ("Unserialized nonresident asset"); one it has loaded spawns (`npc/ghosts_passive/lady_fancy`) |
| Props by model remap (`acting.py`) | A model put at the path of one the map's entities load is resident, so it spawns: `props_city/newspaper_stack01a/b` and `props_signage/signage_street_blank_a` carried new props with no warning, and they render on playback with pak90 installed. `props_city/pigeon_coop01` and `abilities/baba_model_teapot_fx` are not resident |
| The emote key in a match | `+in_cosmetic1` played nothing (Lash, `dl_midtown`) |

## Not possible in retail, and why

| Wanted | Why not |
|---|---|
| Lens (FOV) on the spectator | `citadel_camera_spectator_fov`, `fov_desired`, `citadel_camera_override_fov`: no effect on the demo spectator. `zoom` crops instead |
| A window larger than the monitor (supersampling) | `launch.ps1 -Width 3840 -Height 2160` on 1920x1080 monitors: the game's client area stays 1920x1080. `mat_viewportscale 1` or `1.5` and `r_citadel_upscaling 0` from the console changed nothing measurable (edge sharpness 172.5, 176.3, 162.2, 176.8 against 173.6 restored). Quality comes from the video menu |
| Stepping a paused demo by less than a tick | `demo_step_tick` steps whole ticks; `demo_resume` and `demo_pause` one rendered frame apart, with `host_framerate` set, moved nothing (64 steps, under a tick in all); `demo_resume; wait; demo_pause` advanced nothing. The demo has to keep running: the paced route |
| A camera inside geometry or within about 50 units of a hero | Pushed out, or refused |
| Console to Panorama events | `panorama_dispatch_event` builds engine event types only. Keys are the bridge's input |
| A free camera live, or in a POV demo | `freecamera_*`, `thirdperson`, `cam_*` are dead; POV demos lock the view. SourceTV takes |
| `startmovie`, `screenshot` | Development-only; capture through the OS |
| Depth of field, render quality from the console | Development-only cvars, set by the video menu |
| Placing a light | `ent_create` takes no keyvalues. A studio rig (`studio_lights.py`) started on a bot with `particle_test_file` and `particle_test_start` lit nothing visible in the city |
| Switching hero inside a city match | `selecthero` to another hero left Lash in place in `dl_midtown`; one take a hero, cut between them |
| Choosing the starting hero or lane from the console | `citadel_player_starting_hero`, `citadel_force_assigned_lane`: unknown commands on 6864. The overlay and the grid are clicked |

## Lessons

- **Pause before you seek, wait for the skip, then wait for a still picture**, and resume-and-pause first after
  `demo_step_tick`.
- **Follow fast actors exactly**; check a frozen shot's middle frame.
- **A bot looks at the player**, brain switches or not: extras that must hold still are posed props of the hero's
  model (`prop_dynamic_create heroes_staging/bebop/bebop` once that hero is in the match), placed by index with
  `ent_setpos` and `ent_setang` (`ent_setname` picks, `ent_find` gives the index).
- **A posed prop plays a fixed stance, not the graph's idles**: remapping Mina's stand idles changed nothing on her
  prop (umbrella up). A friendly bot with its brain off plays the graph (the remap made her kneel) and keeps its place.
- **`ent_setname` names what the crosshair is on, and a small prop is never under it**: a glass spawned on Bebop
  named Bebop. Find a fresh small prop by its index instead (`ent_find prop_dynamic` before and after: the new
  unnamed index) and place it by index. After a second `prop_dynamic_create`, `ent_setname` can rename the previous
  prop: spawn each prop with the crosshair on a fresh spot, name it, check with `ent_find cin_` that the name is on a
  new index, then place it. Naming a fresh bot that way missed two times in three.
- **A hero's arms are the graph's in combat.** Lash's hero graph has IK on both hands gated by ID events, and the
  combat layers (the aim additive, the upper-body override, the action masks) sit over the locomotion: a clip on the
  combat crouch moves the body but not the arms. Act out of combat (the skin's own idles move the arms freely there)
  and give the clip the ID events Valve's free-arm clips carry (`wall_attach_*`: `TAG_aim_OFF`,
  `TAG_WeaponHAndIK_L_DISABLE` ...). When those out-of-combat clips belong to a skin's own pak (pak10), the acting
  file installs the cinema pak below it for that shoot (`[pak] install_as = "pak09_dir.vpk"`; `acting.py` checks
  that no other kit file collides with the paks it then beats).
- **Archived cvars persist**: `session.py` always runs `exec cinema_reset`, puts `fps_max` back (read from
  `machine_convars.vcfg`), uninstalls pak90 and only then releases the seat.
- **A launch with no map** answers the console only once the main menu has loaded: `session.py` probes until it does.
- **The game lingers after `quit`** (about a minute), so a session's hand-back takes 2-3 minutes; the next session
  waits for the seat rather than racing it.
- **The seat is released only after the cinema pak is verified gone.** A session closing while another waited once
  let the waiting one launch and install pak90, which the closing one then uninstalled. Now a hold keeps the seat
  from the stop to the release however long the game takes to close (`gamelock.py acquire --hold`, `stop_game.ps1
  -KeepLock`: [../ingame/README.md](../ingame/README.md), "One user of the game at a time"); a stop that hangs is cut
  off and the game killed; and the uninstall is checked with `magician install --status` and the addons folder
  (`test_session.py`) before the seat is free.
- **Source 2 Viewer holding the addon paks** makes a session's uninstall fail and keeps the seat: quit the viewer
  first ([../docs/reference/engine.md](../docs/reference/engine.md), "Source 2 Viewer").

## The stage

`python -I stage.py list <map>` reads a map's landmarks from its compiled entity lump (KV3, zstd-compressed:
magician's own zstd decoder, `crates/resource/src/zstd.rs`), cached in `work/cinema/maps/` (Valve's data: not
committed). `stage.py camera <map> <name>` prints one of Valve's cameras as a `spec_goto` line.

| Map | Take and free camera | What is there |
|---|---|---|
| `new_player_basics` (Hero Sandbox) | proven | Bigger than its street: team spawns round (1400, -1260) and (0, 3230), a Walker at (-2560, 832), Patrons at (0, -3072) and (0, 4864), a shop at (1408, -864), teleporters at (2592, -2144) and (1952, -2144), a bounce pad at (2624, -640), ziplines from (1536, -1408), neutral camps at (2272, -2815) and (2720, -2624) |
| `dl_midtown` (the city) | **proven** | 18 point cameras and 2 Patron cameras, 13 team spawns, 12 Patron guards, 6 Walkers, 2 Patrons (`npc_boss_tier3` at (1280, 8048, 632) and (-1280, -8034, 632)), 17 shops, 52 neutral camps, 5 ziplines, 21 teleporters and launch pads, 4 generators |
| `dl_hideout` | untested | `intro_camera` (10851, -6084, -168) and `pre_intro_camera` |
| `hero_testing` | | crashes client.dll about 25 s after load |

Places in `dl_midtown` that work (floor z from Boon):

| Place | Where | What it gives |
|---|---|---|
| The Hidden King's shrine aisle | x -1280, y -7450 (the Patron end) to -5683, floor 640-646 | Valve's `intro_camera_amber_1` (-1280, -6391, 753), pitch -14, yaw 270: the Patron, barrels and banners; a hero running toward the lens; at -5683 the archway with curtains (turn the hero to face south) |
| The Archmother's shrine aisle | x 1280, y 7750 to 6032, floor 640-645 | `intro_camera_sapphire_1` (1280, 6522, 791), yaw 90: the blue beam and the Patron; at 6032 a platform under the statues |
| Crowley's diner | the Archmother's side of mid: x -448..973, y 3500..4520, floor 384 (world nodes `signage_diner_01`, `diner_metal_01`, `tile_floor_checker_02`) | a 1950s diner: chrome-stool counter, red booths, checker floor, pendant globes, portraits, CROWLEY'S and DINER signs; the street in front, the blue Walker at (1288, 3472) in the plaza, the `plaza_rink_03` neutral camp at (-832, 3904). A camera at z 450 under the ceiling sees its black ceiling blocks: frame lower or from outside |
| The bridge | walkway from (2150, -520) to (2612, 55), z 376-400; street below | `screenshot_camera_03` (2897, 409, 405) looks down it (its pillar hides a runner: chase instead); a leap from `setpos 2330 -300 950` lands at (2612, 55); the skyline's spires lie at yaw 70 |

Valve's cameras in `dl_midtown` (origin, then pitch and yaw; all in `stage.py list dl_midtown --class point_camera`):

| Camera | spec_goto | Frames |
|---|---|---|
| `intro_camera_amber_1` | -1280 -6391 753 -14.0 270.0 | the Hidden King's Patron down the aisle |
| `intro_camera_amber_2` | -1280 -6841 826 -36.4 270.0 | the same, looking up |
| `intro_camera_sapphire_1` | 1280 6522 791 -0.4 90.0 | the Archmother's Patron down the aisle |
| `intro_camera_sapphire_2` | 1280 6785 816 -32.0 90.0 | the same, looking up into the beam |
| `screenshot_camera_02` | -8404 -1458 351 -4.8 56.4 | an alley with crates |
| `screenshot_camera_03` | 2897 409 405 -4.8 230.8 | the bridge and the lamps |
| `screenshot_camera_04` | 8516 1822 363 1.2 246.8 | a park with trees and a cart |
| `1288310_screenshot_camera_00` | -6176 -5549 782 -7.6 110.0 | a garage interior (Megabrand motor oil) |
| `1288310_screenshot_camera_01` | -8584 -1973 415 -13.2 66.0 | an alley with a cart |

## Pieces

| File | What |
|---|---|
| `film.py` | The CLI: check, plan, animatic, photomatic, build, render, encode, lossy, make; lockstep, paced and realtime renderers; the frame sink and the edit graph |
| `scene.py` | The shot language: presets, aliases, defaults, ticks, cues, takes; resolves every frame (whole and exact ticks, each frame's step of demo time) |
| `animatic.py` | The offline animatic |
| `track.py` | Actor tracks and casts from a take through Boon (a subprocess in its venv), cached |
| `stage.py` | A map's landmarks and Valve's cameras from its entity lump |
| `session.py` | One game session with the seat, the pak, the reset and `fps_max` guaranteed; `--script` or `--serve` runs step files |
| `record.py` | Real-time screen capture (ddagrab, constant 60 fps), for the realtime fallback |
| `replay.py`, `frames.py` | A staged take (`ingame/takes.py stage`) filmed from free cameras, recorded in real time per shot, and stills and contact sheets from those recordings; `magician session film` and `sheets` run them |
| `bridge/` | Panorama JS (per-frame console commands from baked paths: played, or paced with `host_framerate`), its builder (the replay HUD and Testing Tools layouts) and its node test |
| `studio_lights.py` | Particle systems that are nothing but dynamic lights, for a light rig on a hero |
| `acting.py`, `vpkx.py` | Acting for a shoot: clips and models remapped onto paths the actors trigger or the map loads, folded into pak90 after `cinema.py build` (`check` refuses a target a lower pak masks); `vpkx.py` reads files out of any VPK |
| `grade.py` | The colour looks and the look book |
| `splice.py` | Splices re-rendered shots into a delivered master and mixes the sound again, so a late fix to one shot doesn't mean rendering the whole film |
| `cinema.py` | build / install / uninstall / status of pak90 (`cinema.ps1` is its PowerShell spelling) |
| `addon/` | The CSDK addon `cinema_dev`: cfgs (aliases, keypad, reset), the overhead-bar override, studio lights, the bridge layouts and chains (generated, git-ignored) |
| `scenes/`, `scenarios/` | `example_run` (a worked example on a stock hero; a mod's own scenes live with the mod) and the bridge's probe scenario |
| `shoot.py`, `shots/` | The first renderer, kept for `shots/example_orbit.json` |
| `test_scene.py`, `test_replay.py`, `test_session.py` | Offline tests: the language, takes and the encode (needs ffmpeg); the free-camera film's shot list and demo-end check; the seat hand-back on a fake clock |

## Not tried yet

1. **Paced at real speed for cloth-heavy shots.** Lockstep pauses the demo between frames; the cloth probe showed
   the same clipping in every route, but a skirt's flare differs between paused, paced and real playback (paced
   and real-time are closer to each other). Compare a whole film paced against lockstep and choose per actor.
2. **A take with life in it.** `citadel_spawn_trooper_wave` and the neutral camps for the city's background; a 6v6
   bot match (`citadel_botmatch_6v6_spectator.cfg`) as a take of battles to cut from; `citadel_bot_jump`, `_crouch`,
   `_melee`, `_shoot` on a bot for a beat, then off.
3. **Choreography for two.** `citadel_bot_record` and `citadel_bot_playrecording`: record a move as one hero with a
   bot of the same hero mimicking, play it back while the player acts the other part: two choreographed actors in
   one take. And `citadel_assume_pawn_control`: a second hero in the same take without `selecthero`.
4. **Paced everywhere.** Paced frames cost 0.25 s like lockstep: render whole-tick shots paced too and compare with
   lockstep (cloth and particles step with the demo time either way); try `fps_max 10` for a faster render.
5. **A light that shows.** The particle rig lit nothing on a bot in the city; try it on the player in the sandbox
   (where it was built), brighter, and check whether `particle_test_start` is recorded into a take at all.
6. **`SetAnimation` on a posed `prop_dynamic`** (aim at a map prop, `ent_fire !picker SetAnimation <sequence>`):
   props as extras.
7. **The intro camera language.** Valve's four intro cameras are the city's best frames; `citadel_match_intro_force_enabled`
   and `citadel_match_intro_style 2` may play Valve's own match intro into a take.
8. **Modlock** (github.com/paralin/modlock, MIT; read, nothing downloaded or run): game-native camera curves on a
   hero from a local server. An option, not adopted.
