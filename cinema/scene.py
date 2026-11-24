"""The shot language: a scene file (TOML) compiled into frames the retail game can render.

A scene is a list of shots filmed from one SourceTV take. Every frame of every shot is resolved here, offline, to a
demo tick and a camera pose (position, pitch, yaw) plus a digital zoom; `film.py` turns the frames into an alias chain
for the cinema pak and drives the game through it. See README.md, "Scene guide", for the language by example.

    [scene]
    name = "lash_leap"                  # file-safe; the output folder and the chain names
    take = "auto-20261121-1022-new_player_basics-4-player.dem"   # citadel/replays/<take>, or a full path
    fps = 32                            # 32: two ticks a frame, real time exactly (64 ticks a second)
    look = "clean"                      # clean (no HUD, no Panorama at all) | hud (as played)
    world = "map"                       # map | void (black: r_drawworld 0, heroes keep the map's light)

    [[shot]]
    name = "run_in"
    length = 3.0                        # seconds of screen time
    tick = 2760                         # the demo tick of the first frame (or at = "Lash@2760" style is not needed)
    speed = 1.0                         # 1 real time, 0.5 half, 0 frozen (bullet time); see "Time" below
    cut = "cut"                         # how the shot begins: cut | dissolve 0.5 | fade 0.4 (from black) | dip 0.4
    [shot.camera]
    type = "track"                      # fixed | keys | orbit | track | dolly
    actor = "Lash"
    offset = [0, -260, 70]              # forward, left, up from the actor's feet, in its direction of travel
    look_height = 45

Several takes. `[takes]` names more SourceTV takes (`city = "auto-...-dl_midtown-....dem"`; `main` is [scene] take)
and a shot films from one with `take = "city"`. An [actors] alias is one actor name for every take or a table per
take (`faz = { main = "Sinclair#1", city = "Sinclair#0" }`). The renderer plays each take when its shots come up.

Positions are world units (setpos / spec_goto), z up; yaw counter-clockwise from +x; pitch down positive. A camera
position is the spectator's eye (spec_goto z is the eye: measured 2026-11-21). An actor is a hero of the take by
Boon's name (`Lash`, `Sinclair`, `Lash#1`; `track.py list <take>`), placed at its feet, interpolated per frame.

Time. A frame advances the demo by speed * 64 / fps ticks through `demo_step_tick`, which only steps whole ticks: the
frames step the rounded running total, so a speed that is not a whole number of ticks a frame alternates steps
(1.0 at 30 fps: 2, 2, 2, ... 3). Keep speed * 64 / fps whole for even motion: fps 32 at speed 1 or 0.5, fps 64 at 1.
Below one tick a frame the shot is rendered in real time instead (`render = "realtime"`; film.py README).
"""
import json
import math
import os
import sys

try:
    import tomllib
except ImportError:   # Python < 3.11
    tomllib = None

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import track  # noqa: E402

TICK_RATE = 64
# How a shot slower than a tick a frame renders when its scene says nothing (render = "auto"): "realtime" (the demo
# at demo_timescale, screen-recorded) or "substep" (frame by frame through the bridge's host_framerate step).
SLOW_ROUTE = os.environ.get("CINEMA_SLOW_ROUTE", "substep")
EASES = ("linear", "in", "out", "inout", "smooth")
CUTS = ("cut", "dissolve", "fade", "dip", "snip")   # snip: the house scissors cut across the frame (film.py)


class SceneError(ValueError):
    """A mistake in a scene file, with where it is."""


# ---- maths ---------------------------------------------------------------------------------------------------

def ease(u, kind="linear"):
    u = min(1.0, max(0.0, u))
    if kind == "in":
        return u * u
    if kind == "out":
        return 1 - (1 - u) * (1 - u)
    if kind == "inout":
        return u * u * (3 - 2 * u)
    if kind == "smooth":   # smootherstep: zero acceleration at both ends
        return u * u * u * (u * (u * 6 - 15) + 10)
    return u


def catmull(p0, p1, p2, p3, s):
    s2, s3 = s * s, s * s * s
    return [0.5 * (2 * b + (-a + c) * s + (2 * a - 5 * b + 4 * c - d) * s2 + (-a + 3 * b - 3 * c + d) * s3)
            for a, b, c, d in zip(p0, p1, p2, p3)]


def spline(points, times, t):
    """Catmull-Rom through points at times (end points held), at t."""
    if t <= times[0]:
        return list(points[0])
    if t >= times[-1]:
        return list(points[-1])
    i = max(k for k in range(len(times) - 1) if times[k] <= t)
    s = (t - times[i]) / (times[i + 1] - times[i])
    return catmull(points[max(0, i - 1)], points[i], points[i + 1], points[min(len(points) - 1, i + 2)], s)


def look_angles(pos, target):
    dx, dy, dz = (target[k] - pos[k] for k in range(3))
    yaw = math.degrees(math.atan2(dy, dx))
    pitch = -math.degrees(math.atan2(dz, math.hypot(dx, dy)))
    return pitch, yaw % 360


def lerp(a, b, s):
    return a + (b - a) * s


def lerp_angle(a, b, s):
    return a + ((b - a + 180) % 360 - 180) * s


def keyed(value, u, kind="linear"):
    """A number or [from, to] (eased over the shot) or [[u, v], ...] (piecewise linear in shot fraction u)."""
    if isinstance(value, (int, float)):
        return float(value)
    if isinstance(value, list) and len(value) == 2 and all(isinstance(v, (int, float)) for v in value):
        return lerp(value[0], value[1], ease(u, kind))
    if isinstance(value, list) and value and all(isinstance(v, list) and len(v) == 2 for v in value):
        pts = sorted(value)
        if u <= pts[0][0]:
            return float(pts[0][1])
        for (u0, v0), (u1, v1) in zip(pts, pts[1:]):
            if u <= u1:
                return lerp(v0, v1, ease((u - u0) / (u1 - u0) if u1 > u0 else 1, kind))
        return float(pts[-1][1])
    raise SceneError(f"not a number, [from, to] or [[u, value], ...]: {value!r}")


# ---- camera presets ------------------------------------------------------------------------------------------
# Built in; a scene adds its own as [presets.<name>] tables. A shot's camera names one (`preset = "side_track"`)
# and sets only what differs; `actor` stands for `around` in an orbit. Angles of the orbits are relative to the
# actor's heading at the shot's first tick: 0 in front of it, 90 on its left, -90 on its right.
PRESETS = {
    "side_track": {"type": "track", "offset": [30, -220, 55], "look_height": 55, "lead": 30},
    "chase": {"type": "track", "offset": [-260, 0, 90], "look_height": 60},
    "lead": {"type": "track", "offset": [300, 0, 60], "look_height": 60},
    "low_hero": {"type": "track", "offset": [150, -70, 15], "look_height": 75},
    "hero_orbit": {"type": "orbit", "relative": "heading", "radius": 170, "height": 40, "from": -60, "to": 60,
                   "look_height": 55, "ease": "smooth"},
    "bullet": {"type": "orbit", "relative": "heading", "radius": 180, "height": 35, "from": -100, "to": 80,
               "look_height": 50, "ease": "smooth", "smooth": 0},
    "crane_up": {"type": "orbit", "relative": "heading", "radius": 230, "height": 20, "height_to": 260, "from": 20,
                 "to": 45, "look_height": 50, "ease": "inout"},
    "push_in": {"type": "orbit", "relative": "heading", "radius": 320, "radius_to": 150, "height": 50, "from": 15,
                "to": 0, "look_height": 55, "ease": "inout"},
}


def merge(base, over):
    """A table over another: nested tables merge, everything else replaces."""
    out = dict(base)
    for k, v in over.items():
        out[k] = merge(out[k], v) if isinstance(v, dict) and isinstance(out.get(k), dict) else v
    return out


# ---- the scene -----------------------------------------------------------------------------------------------

class Frame:
    """One film frame: the whole demo tick (`tick`, what lockstep steps to), the exact one (`ft`, fractional: where
    a substep or realtime frame really is), the whole ticks stepped from the frame before, the seconds of demo time
    from the frame before (`rate`: a substep frame's host_framerate), the camera pose and the zoom."""
    __slots__ = ("shot", "k", "tick", "ft", "step", "rate", "pos", "pitch", "yaw", "zoom")

    def __init__(self, shot, k, tick, step, pos, pitch, yaw, zoom, ft=None, rate=0.0):
        self.shot, self.k, self.tick, self.step = shot, k, tick, step
        self.pos, self.pitch, self.yaw, self.zoom = pos, pitch, yaw, zoom
        self.ft = float(tick) if ft is None else ft
        self.rate = rate

    def spec_goto(self):
        return f"spec_goto {self.pos[0]:.2f} {self.pos[1]:.2f} {self.pos[2]:.2f} {self.pitch:.2f} {self.yaw:.2f}"


class Shot:
    def __init__(self, scene, index, d):
        d = merge(scene.defaults, d)
        self.scene, self.index, self.d = scene, index, d
        where = f"shot {index + 1}" + (f" ({d['name']})" if "name" in d else "")
        self.where = where
        self.name = str(d.get("name", f"shot{index + 1:02d}"))
        if not self.name.replace("_", "").isalnum():
            raise SceneError(f"{where}: name {self.name!r}: letters, digits and _ only")
        self.speed = d.get("speed", 1.0)
        self.render = d.get("render", "auto")
        if self.render not in ("auto", "lockstep", "realtime", "substep"):
            raise SceneError(f"{where}: render {self.render!r}: auto, lockstep, realtime or substep")
        cut = str(d.get("cut", "cut")).split()
        if cut[0] not in CUTS:
            raise SceneError(f"{where}: cut {d.get('cut')!r}: one of {', '.join(CUTS)} (with seconds: \"dissolve 0.5\")")
        self.cut = cut[0]
        self.cut_len = (scene.seconds(" ".join(cut[1:]), where, "cut") if len(cut) > 1
                        else (0.0 if cut[0] == "cut" else 0.5))
        # a card: a title or end card drawn by the encoder (no take, no camera): card = { title = "...", sub = "..." }
        self.card = d.get("card")
        if self.card is not None:
            if not isinstance(self.card, dict) or not isinstance(self.card.get("title", ""), str):
                raise SceneError(f'{where}: card = {{ title = "...", sub = "...", style = "house" }}')
            d = dict(d)
            d.setdefault("tick", 0)
            d.setdefault("camera", {"type": "fixed", "pos": [0, 0, 0], "ang": [0, 0]})
            self.d = d
        self.grade = str(d.get("grade", scene.grade))   # a look of grade.LOOKS, or "none"
        self.zoom = d.get("zoom", 1.0)
        self.zoom_ease = d.get("zoom_ease", "inout")
        cam = d.get("camera")
        if isinstance(cam, str):   # camera = "side_track": a preset with nothing changed
            cam = {"preset": cam}
        if not isinstance(cam, dict):
            raise SceneError(f"{where}: needs a [shot.camera] table (a type, or a preset)")
        cam = dict(cam)
        if "preset" in cam:
            name = cam.pop("preset")
            base = scene.presets.get(name)
            if base is None:
                raise SceneError(f"{where}: camera preset {name!r} unknown; presets: {', '.join(sorted(scene.presets))}")
            cam = merge(base, cam)
        if cam.get("type") == "orbit" and "around" not in cam and "actor" in cam:
            cam["around"] = cam.pop("actor")
        if "type" not in cam:
            raise SceneError(f"{where}: the camera needs a type (fixed, keys, orbit, track, dolly) or a preset")
        self.cam = cam
        self.pre = list(d.get("pre", []))
        # screen-time cues for the edit: cue = "text" (at the start) or cues = [[seconds, "text"], ...]
        self.cues = ([[0.0, d["cue"]]] if isinstance(d.get("cue"), str) else []) + [list(c) for c in d.get("cues", [])]
        for c in self.cues:
            if len(c) != 2 or not isinstance(c[1], str):
                raise SceneError(f'{where}: cues are [seconds, "text"] pairs, not {c!r}')
            c[0] = scene.seconds(c[0], where, "cue")
        self.warmup = float(d.get("warmup", scene.warmup))
        # a layer source: filmed like any shot, never on the timeline; a host shot composites it (layers = [...])
        self.layer_src = bool(d.get("layer", False))
        # head = N: the encode replaces the shot's first N frames with copies of frame N (a first frame that still
        # shows the camera or the moment before, after a seek; the length and the downbeats do not move)
        self.head = int(d.get("head", 0))
        # hits for the encode (the Maid Lash trailer's bomb, cut 2): flash = [[at, strength, decay], ...] washes the
        # frame toward white (weighted to its bright parts, a bloom) by `strength` (1 = white) at `at`, decaying with
        # the time constant `decay`; shake = [[at, pixels, decay, hz], ...] jolts the frame by up to `pixels` (at the
        # output size), decaying likewise. Times are shot time (seconds or musical); the shake crops a margin.
        self.flash, self.shake = [], []
        for key, n, out in (("flash", 3, self.flash), ("shake", 4, self.shake)):
            for e in d.get(key, []):
                if not isinstance(e, list) or len(e) not in (n - 1, n) or not all(isinstance(v, (int, float)) for v in e[1:]):
                    raise SceneError(f"{where}: {key} entries are [at, " + ("strength, decay]" if n == 3 else "pixels, decay, hz]")
                                     + f", not {e!r}")
                e = [scene.seconds(e[0], where, key)] + [float(v) for v in e[1:]]
                out.append(e + ([0.3] if key == "flash" and len(e) == 2 else [12.0] if key == "shake" and len(e) == 3 else []))
        # a crash zoom in the encode: punch = [[in, peak, hold, out, zoom, x, y], ...]: the crop eases from 1 to `zoom`
        # (in to peak), holds, eases back (hold to out), centred on (x, y) in fractions of the frame (kept inside it)
        self.punch = []
        for e in d.get("punch", []):
            if not isinstance(e, list) or len(e) != 7:
                raise SceneError(f"{where}: punch entries are [in, peak, hold, out, zoom, x, y], not {e!r}")
            self.punch.append([scene.seconds(v, where, "punch") for v in e[:4]] + [float(v) for v in e[4:]])
        self.layers = []
        self.take_key = str(d.get("take", "main"))
        if self.take_key not in scene.takes:
            raise SceneError(f"{where}: take {self.take_key!r} is not in [takes] ({', '.join(sorted(scene.takes))})")
        self.tick = self.length = self.n = None   # resolved by the scene: a tick can name the take or the shot before

    @property
    def tracks(self):
        return self.scene.tracks_for(self.take_key)

    def resolve(self, prev):
        """The first tick and the length, now that the take and the shot before are known."""
        sc, d = self.scene, self.d
        if "tick" not in d:
            raise SceneError(f'{self.where}: missing tick (a number, "after", or "cast <ability>")')
        self.tick = sc.tick_of(d["tick"], self, prev)
        if "length" in d:
            self.length = sc.seconds(d["length"], self.where, "length")
            if self.length < 0.04:
                raise SceneError(f"{self.where}: length {self.length} is below 0.04")
        elif "until" in d:
            if not isinstance(self.speed, (int, float)) or self.speed <= 0:
                raise SceneError(f"{self.where}: until needs a constant speed above 0 (use length instead)")
            until = sc.tick_of(d["until"], self, prev)
            if until <= self.tick:
                raise SceneError(f"{self.where}: until (tick {until}) is not after the first tick ({self.tick})")
            self.length = (until - self.tick) / (self.speed * TICK_RATE)
        else:
            raise SceneError(f"{self.where}: missing length (seconds) or until (a tick)")
        self.n = max(1, int(round(self.length * sc.fps)))

    def _num(self, key, default, lo=None):
        v = self.d.get(key, default)
        if v is None:
            raise SceneError(f"{self.where}: missing {key}")
        if not isinstance(v, (int, float)):
            raise SceneError(f"{self.where}: {key} must be a number, not {v!r}")
        if lo is not None and v < lo:
            raise SceneError(f"{self.where}: {key} {v} is below {lo}")
        return float(v)

    def warm_ticks(self):
        """Whole ticks played (two a rendered frame) from the seek to the first frame: the warm-up, even, and never
        before the take's start."""
        return min(2 * int(round(self.warmup * TICK_RATE / 2)), self.tick - self.tick % 2)

    def speed_at(self, u):
        return keyed(self.speed, u, self.d.get("speed_ease", "inout"))

    def fticks(self):
        """The exact demo tick of every frame (fractional): the first tick, then speed * 64 / fps a frame."""
        fps = self.scene.fps
        t = [float(self.tick)]
        for k in range(1, self.n):
            u = (k - 0.5) / max(1, self.n - 1)
            t.append(t[-1] + self.speed_at(u) * TICK_RATE / fps)
        return t

    def ticks(self):
        """The whole demo tick of every frame and the whole ticks each frame steps from the previous one."""
        t = self.fticks()
        whole = [int(round(x)) for x in t]
        steps = [0] + [whole[k] - whole[k - 1] for k in range(1, self.n)]
        return whole, steps

    def mode(self):
        if self.card is not None:
            return "card"
        if self.render != "auto":
            return self.render
        _, steps = self.ticks()
        moving = [s for s in steps[1:]]
        if any(s > 0 for s in moving) and any(s == 0 for s in moving):
            return SLOW_ROUTE   # slower than a tick a frame: lockstep would hold frames
        return "lockstep"


class Scene:
    def __init__(self, path):
        self.path = os.path.abspath(path)
        try:
            if path.endswith(".json"):
                with open(path, encoding="utf-8") as fh:
                    d = json.load(fh)
            else:
                if tomllib is None:
                    raise SceneError("TOML needs Python 3.11+ (tomllib); write the scene as JSON instead")
                with open(path, "rb") as fh:
                    d = tomllib.load(fh)
        except (OSError, ValueError) as e:   # tomllib.TOMLDecodeError and json errors are ValueErrors
            raise SceneError(f"{path}: {e}")
        sc = d.get("scene")
        if not isinstance(sc, dict):
            raise SceneError(f"{path}: needs a [scene] table (name, take, fps)")
        self.d = d
        self.name = str(sc.get("name") or os.path.splitext(os.path.basename(path))[0])
        if not self.name.replace("_", "").isalnum():
            raise SceneError(f"[scene] name {self.name!r}: letters, digits and _ only")
        self.take = sc.get("take")
        if not self.take:
            raise SceneError("[scene] take: the SourceTV demo the scene is filmed from (citadel/replays/<file>.dem)")
        self.fps = int(sc.get("fps", 32))
        if not 1 <= self.fps <= 120:
            raise SceneError(f"[scene] fps {self.fps}: 1..120")
        self.look = sc.get("look", "clean")
        if self.look not in ("clean", "hud"):
            raise SceneError(f"[scene] look {self.look!r}: clean or hud")
        self.world = sc.get("world", "map")
        if self.world not in ("map", "void"):
            raise SceneError(f"[scene] world {self.world!r}: map or void")
        self.settle = float(sc.get("settle", 0.15))
        # seconds of the take played continuously before a shot's first frame, so cloth (Maid Lash's skirt and any
        # coat tails) arrives simulated instead of starting from the seek's teleport; a shot may set its own
        self.warmup = float(sc.get("warmup", 1.5))
        self.pre = list(sc.get("pre", []))
        self.post = list(sc.get("post", []))
        self.grade = str(sc.get("grade", "none"))
        self.music = Music(d["music"], self.path) if d.get("music") is not None else None
        self.window = self._size(sc.get("window", [1920, 1080]), "window")   # the game window the frames render at
        self.output = self._size(sc.get("output", [1920, 1080]), "output")   # the delivered video
        self.takes = {"main": str(self.take)}
        if self.takes["main"] == "blocking" and "blocking" not in d:
            raise SceneError('[scene] take = "blocking" needs a [blocking] table (the planned actors)')
        for k, v in d.get("takes", {}).items():
            if not isinstance(v, str):
                raise SceneError(f"[takes] {k}: a take file name (citadel/replays/<file>.dem)")
            self.takes[str(k)] = v
        self.aliases = {}
        for k, v in d.get("actors", {}).items():
            if isinstance(v, dict):
                bad = [t for t in v if t not in self.takes]
                if bad:
                    raise SceneError(f"[actors] {k}: take(s) {', '.join(bad)} not in [takes]")
                self.aliases[str(k)] = {str(t): str(n) for t, n in v.items()}
            else:
                self.aliases[str(k)] = str(v)
        self.presets = merge(PRESETS, d.get("presets", {}))
        self.defaults = d.get("defaults", {})
        self.cue_table = []
        for c in d.get("cue", []):
            if not isinstance(c, dict) or "at" not in c or "text" not in c:
                raise SceneError(f"[[cue]] needs at (seconds of the film) and text: {c!r}")
            self.cue_table.append([self.seconds(c["at"], "[[cue]]", "at", absolute=True), str(c["text"])])
        self._tracks = {}
        shots = d.get("shot")
        if not isinstance(shots, list) or not shots:
            raise SceneError(f"{path}: no [[shot]] tables")
        every = [Shot(self, i, s) for i, s in enumerate(shots)]
        names = [s.name for s in every]
        dup = {n for n in names if names.count(n) > 1}
        if dup:
            raise SceneError(f"shot names repeat: {', '.join(sorted(dup))}")
        # the timeline is the shots that are not layer sources; layer sources are filmed and composited by a host
        self.shots = [s for s in every if not s.layer_src]
        self.layer_shots = [s for s in every if s.layer_src]
        if not self.shots:
            raise SceneError(f"{path}: every [[shot]] is a layer source: nothing on the timeline")
        for i, s in enumerate(self.shots + self.layer_shots):
            s.index = i
        prev = {False: None, True: None}
        for shot in every:   # "after" follows the shot before in the same group (timeline or layer sources)
            shot.resolve(prev[shot.layer_src])
            prev[shot.layer_src] = shot
        for shot in self.shots:
            shot.layers = [self._layer(shot, k, v) for k, v in enumerate(shot.d.get("layers", []))]
        names = [s.name for s in self.shots]
        # sound effects as designed events: [[sfx]] at = film time (or shot = "name" and at within it), file, gain
        self.sfx = []
        for e in d.get("sfx", []):
            if not isinstance(e, dict) or "file" not in e or "at" not in e:
                raise SceneError(f"[[sfx]] needs at and file: {e!r}")
            if "shot" in e:
                if e["shot"] not in names:
                    raise SceneError(f"[[sfx]] {e.get('name', e['file'])}: shot {e['shot']!r} is not a shot of the scene")
                at = self.shot_start(e["shot"]) + self.seconds(e["at"], f"[[sfx]] {e['shot']}", "at")
            else:
                at = self.seconds(e["at"], "[[sfx]]", "at", absolute=True)
            self.sfx.append({"at": at, "file": self.resolve_file(e["file"]), "gain": float(e.get("gain", 0.0)),
                             "name": str(e.get("name", os.path.splitext(os.path.basename(e["file"]))[0]))})
        self.sfx.sort(key=lambda e: e["at"])

    def all_shots(self):
        """Every shot that is filmed: the timeline and the layer sources."""
        return self.shots + self.layer_shots

    def _layer(self, host, k, v):
        """One layer of a host shot: another (layer = true) shot, or a still image, cropped, scaled and moved over the host.

            layers = [{ src = "lash_top", crop = [0, 0.25, 1, 0.5], size = [1, 0.5],
                        keys = [[0, 0, -0.5], ["3 frames", 0, 0, "out"], ["2 beats", 0, 0], ["2 beats 4 frames", 0, -0.5, "in"]],
                        border = 3, border_color = "0xc9a75a" }]

        crop: the window of the source [x, y, w, h], fractions of its frame; size: the panel [w, h], fractions of
        the output; keys: [time within the host, x, y (the panel's top-left, fractions of the output), ease of
        the move that ends on this key (linear, in, out, inout; default inout)]; hold = true keeps the source's
        last frame on screen once it ends (a freeze), else the layer vanishes there. The layer's first frame is
        the host's first frame; layers draw in order, the last on top.

        A card (the collage of cut 3): `card = "polaroid"` mounts the panel on a print (an ivory border, a deeper
        foot, a soft shadow, `caption` set in the foot), and its keys may carry a scale and a rotation:
        [time, x, y, ease, scale, degrees]. A card's x, y are its centre, it is drawn only from its first key on (so
        it snaps onto the frame there) and it may leave the frame. `image = "path.png"` takes a still from a file
        (relative to the scene file) instead of a layer shot."""
        where = f"{host.where} layer {k + 1}"
        if not isinstance(v, dict) or ("src" not in v and "image" not in v):
            raise SceneError(f'{where}: a layer is {{ src = "<layer shot>", keys = [[t, x, y], ...] }}')
        src, image = None, None
        if "image" in v:
            image = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(self.path)),
                                                  os.path.expandvars(str(v["image"]))))
            if not os.path.isfile(image):
                raise SceneError(f"{where}: image {image} not found")
        else:
            src = next((s for s in self.layer_shots if s.name == v["src"]), None)
            if src is None:
                raise SceneError(f"{where}: src {v['src']!r} is not a shot with layer = true")
        def quad(name, default, n):
            q = v.get(name, default)
            if not (isinstance(q, list) and len(q) == n and all(isinstance(x, (int, float)) for x in q)):
                raise SceneError(f"{where}: {name} is {n} numbers (fractions), not {q!r}")
            return [float(x) for x in q]
        crop = quad("crop", [0, 0, 1, 1], 4)
        size = quad("size", [1, 1], 2)
        if not (0 <= crop[0] < 1 and 0 <= crop[1] < 1 and 0 < crop[2] <= 1 - crop[0] + 1e-9 and 0 < crop[3] <= 1 - crop[1] + 1e-9):
            raise SceneError(f"{where}: crop {crop} leaves the source frame")
        card = str(v.get("card", "none"))
        if card not in ("none", "polaroid"):
            raise SceneError(f"{where}: card {card!r}: none or polaroid")
        keys = []
        for key in v.get("keys", [[0, 0, 0]]):
            if not (isinstance(key, list) and len(key) in (3, 4, 6)):
                raise SceneError(f"{where}: a key is [time, x, y], [time, x, y, ease] or [time, x, y, ease, scale, "
                                 f"degrees], not {key!r}")
            t = self.seconds(key[0], where, "key time")
            e = str(key[3]) if len(key) >= 4 else "inout"
            if e not in ("linear", "in", "out", "inout"):
                raise SceneError(f"{where}: ease {e!r}: linear, in, out or inout")
            sc, rot = (float(key[4]), float(key[5])) if len(key) == 6 else (1.0, 0.0)
            if sc <= 0:
                raise SceneError(f"{where}: a key's scale must be above 0, not {sc}")
            keys.append((t, float(key[1]), float(key[2]), e, sc, rot))
        if any(b[0] < a[0] for a, b in zip(keys, keys[1:])):
            raise SceneError(f"{where}: key times must not go back")
        transform = card != "none" or any(len(key) == 6 for key in v.get("keys", []))
        return {"src": src, "image": image, "crop": crop, "size": size, "keys": keys, "border": int(v.get("border", 0)),
                "hold": bool(v.get("hold", image is not None)), "card": card, "transform": transform,
                "caption": str(v.get("caption", "")),
                "border_color": str(v.get("border_color", "0xf3e6c8")),
                "grade": str(v.get("grade", src.grade if src else "none")), "where": where}

    # ---- time: seconds, or the music's bars and beats ------------------------------------------------------
    def seconds(self, v, where, what, absolute=False):
        """A time: seconds (a number), or with a [music] tempo map "2 bars", "3 beats", "1 bar 2 beats" (a
        duration), or "bar 5" / "bar 5 beat 3" (a position on the film clock: bar 1 beat 1 is [music] start)."""
        if isinstance(v, (int, float)) and not isinstance(v, bool):
            return float(v)
        if not isinstance(v, str):
            raise SceneError(f"{where}: {what} must be seconds or a musical time, not {v!r}")
        words = v.lower().split()
        if any(w in ("frame", "frames") for w in words):   # "4 frames", "1 beat 4 frames": frames at the scene's fps
            extra, rest, i = 0.0, [], 0
            while i < len(words):
                if i + 1 < len(words) and words[i + 1] in ("frame", "frames"):
                    try:
                        extra += float(words[i]) / self.fps
                    except ValueError:
                        raise SceneError(f"{where}: {what} {v!r}: frames are counted as \"4 frames\"")
                    i += 2
                else:
                    rest.append(words[i])
                    i += 1
            return extra + (self.seconds(" ".join(rest), where, what, absolute) if rest else
                            (self.music.start if (absolute and self.music) else 0.0))
        if self.music is None:
            try:
                return float(v)
            except ValueError:
                raise SceneError(f"{where}: {what} {v!r} is musical: the scene needs a [music] table (bpm)")
        return self.music.seconds(v, where, what, absolute)

    def resolve_file(self, f):
        """A file a scene names: environment variables expanded, relative to the scene file."""
        p = os.path.expandvars(str(f))
        if not os.path.isabs(p):
            p = os.path.join(os.path.dirname(self.path), p)
        return os.path.normpath(p)

    def timeline(self):
        """(shot, film start, film end) for every shot; a dissolve or a snip overlaps the shot before."""
        out, t = [], 0.0
        for s in self.shots:
            if s.index > 0 and s.cut in ("dissolve", "snip"):
                t -= min(s.cut_len, s.length)
            out.append((s, t, t + s.length))
            t += s.length
        return out

    def shot_start(self, name):
        return next(a for s, a, _ in self.timeline() if s.name == name)

    @staticmethod
    def _size(v, what):
        if not (isinstance(v, list) and len(v) == 2 and all(isinstance(x, int) and 64 <= x <= 8192 for x in v)):
            raise SceneError(f"[scene] {what} must be [width, height] in pixels, not {v!r}")
        return v

    def actor(self, name, shot=None):
        """An actor by name, through the scene's [actors] aliases, in the shot's take (the main take without one)."""
        key = shot.take_key if shot is not None else "main"
        alias = self.aliases.get(name, name)
        if isinstance(alias, dict):
            if key not in alias:
                raise SceneError(f"actor {name!r} has no name in take {key!r} ([actors] {name} = {{ {key} = ... }})")
            alias = alias[key]
        return self.tracks_for(key).get(alias)

    def tick_of(self, v, shot, prev):
        """A tick: a number; "after" (where the shot before ended, plus one step); or a cast in the take,
        "cast <ability> [+-N]" or "cast <actor> <ability> [+-N]" (Boon's ability names; a part is enough)."""
        where = shot.where
        if isinstance(v, (int, float)) and not isinstance(v, bool):
            if v < 0:
                raise SceneError(f"{where}: tick {v} is negative")
            return int(round(v))
        if not isinstance(v, str):
            raise SceneError(f'{where}: tick must be a number, "after" or "cast <ability>", not {v!r}')
        words = v.replace("+", " +").replace(" - ", " -").split()
        off = 0
        if words and words[-1][:1] in "+-" and words[-1][1:].isdigit():
            off = int(words.pop())
        if words == ["after"]:
            if prev is None:
                raise SceneError(f'{where}: tick "after" on the first shot')
            whole, steps = prev.ticks()
            return whole[-1] + max(1, steps[-1] if len(steps) > 1 else 1) + off
        if words and words[0] == "cast" and len(words) in (2, 3):
            who = self.actor(words[1], shot) if len(words) == 3 else None
            what = words[-1].lower()
            hits = [t for t, hero, ab in shot.tracks.abilities if what in ab.lower() and (who is None or hero == who.hero)]
            if not hits:
                names = sorted({ab for _, _, ab in shot.tracks.abilities})
                raise SceneError(f"{where}: no cast of {what!r} in the take; casts: {', '.join(names) or 'none'}")
            return hits[0] + off
        raise SceneError(f'{where}: tick {v!r}: a number, "after", or "cast <ability> [+-N]"')

    def cues(self):
        """Every cue in film seconds: the scene's [[cue]] table, each shot's cues and the [[sfx]] events, sorted."""
        out = list(self.cue_table)
        for s, a, _ in self.timeline():
            out += [[a + c[0], c[1]] for c in s.cues]
        out += [[e["at"], f"sfx: {e['name']}"] for e in getattr(self, "sfx", [])]
        return sorted(out)

    def tracks_for(self, key):
        if key not in self._tracks:
            if self.planned(key):   # planned, not shot: check and the animatic work; render refuses
                self._tracks[key] = track.PlannedTracks(self.d.get("blocking"))
            else:
                self._tracks[key] = track.Tracks(self.takes[key])
        return self._tracks[key]

    def planned(self, key="main"):
        return self.takes[key] == "blocking"

    @property
    def tracks(self):
        """The main take's tracks."""
        return self.tracks_for("main")

    def take_path(self, key="main"):
        if self.planned(key):
            raise track.TrackError(f"take {key} is the [blocking] plan: shoot it, then name its SourceTV take")
        return track.take_path(self.takes[key])

    # ---- compile ---------------------------------------------------------------------------------------------
    def frames(self, shot):
        """Every frame of a shot: Frame(tick, step, pos, pitch, yaw, zoom, ft, rate). Lockstep frames sit on whole
        ticks; substep and realtime frames between them, so their cameras follow the actors at the exact tick."""
        whole, steps = shot.ticks()
        ft = shot.fticks()
        exact = shot.mode() in ("substep", "realtime")
        cam = Camera(self, shot)
        out = []
        for k in range(shot.n):
            u = k / max(1, shot.n - 1)
            tick = whole[k]
            pos, pitch, yaw = cam.at(u, k / self.fps, ft[k] if exact else tick)
            zoom = keyed(shot.zoom, u, shot.zoom_ease)
            if zoom < 1.0:
                raise SceneError(f"{shot.where}: zoom {zoom:.2f} below 1 (zoom is a crop of the frame, so 1 is the widest)")
            rate = (ft[k] - ft[k - 1]) / TICK_RATE if k > 0 else 0.0
            out.append(Frame(shot, k, tick, steps[k], pos, pitch, yaw, zoom, ft[k], rate))
        return out

    def check(self):
        """Compile every shot and return warnings (the errors raise)."""
        warn = self.beat_check()
        for shot in self.shots:
            for L in shot.layers:
                if L["src"] is not None and not L["hold"] and L["src"].length + 1e-6 < min(shot.length, L["keys"][-1][0]):
                    warn.append(f"{L['where']}: its source {L['src'].name} ({L['src'].length:.2f} s) ends before the layer "
                                f"leaves (it vanishes there)")
        for shot in self.all_shots():
            total = shot.tracks.total_ticks
            fr = self.frames(shot)
            last = fr[-1].tick
            if last > total:
                raise SceneError(f"{shot.where}: ends at tick {last}, past the take's end ({total})")
            _, steps = shot.ticks()
            moving = steps[1:]
            if moving and len(set(moving)) > 1 and shot.mode() == "lockstep" and min(moving) > 0:
                warn.append(f"{shot.where}: steps {sorted(set(moving))} ticks a frame (speed {shot.speed} at {self.fps} fps "
                            f"is not a whole number of ticks): motion is a little uneven; fps 32 or 64 is exact")
            if shot.mode() == "lockstep":
                for p, f in zip(fr, fr[1:]):
                    if f.step == 0 and math.dist(p.pos, f.pos) > 0.5:
                        warn.append(f"{shot.where} frame {f.k}: the camera travels while the demo holds still. In a paused "
                                    "demo spec_goto turns the spectator but does not move it: the shot keeps the first "
                                    "position and only turns (cut 5's frozen orbit, s20, swung past its subject). Hold "
                                    "the position and pan (keys, path = \"segments\"), or let the demo move")
                        break
            for f in fr:
                for name in Camera.actors_of(shot.cam):
                    a = self.actor(name, shot).at(f.tick)
                    d = math.dist(a, f.pos)
                    if d < 55:
                        warn.append(f"{shot.where} frame {f.k}: camera {d:.0f} units from {name}: spec_goto refuses "
                                    "spots this close to a hero (measured 40-60); use zoom instead")
                        break
            if shot.cut != "cut" and shot.index == 0 and shot.cut == "dissolve":
                warn.append(f"{shot.where}: a dissolve into the first shot has nothing to dissolve from: it fades from black")
        return warn

    def beat_check(self):
        """Warnings for cuts (and the end) that miss the [music] grid by more than half a frame."""
        if not self.music or not self.music.snap:
            return []
        warn = []
        tl = self.timeline()
        for s, a, b in tl:
            off = self.music.off_grid(a, self.fps)
            if off > 0.5:
                warn.append(f"{s.where}: starts at {a:.3f} s ({self.music.where(a)}), {off:.1f} frames off the "
                            f"{self.music.grid} grid")
        end = tl[-1][2]
        if self.music.off_grid(end, self.fps) > 0.5:
            warn.append(f"the film ends at {end:.3f} s ({self.music.where(end)}), off the {self.music.grid} grid")
        return warn

    def summary(self):
        lines = [f"scene {self.name}: {len(self.shots)} shot(s), {self.fps} fps, look {self.look}, world {self.world}"]
        for key in [k for k in self.takes if k == "main" or any(s.take_key == k for s in self.shots)]:
            tr = self.tracks_for(key)
            label = "the [blocking] plan" if self.planned(key) else os.path.basename(self.take_path(key))
            lines.append(f"take {key}: {label} ({tr.map}, {tr.total_ticks} ticks, actors "
                         + ", ".join(tr.names()) + ")")
        for s, t, _ in self.timeline():
            whole, steps = s.ticks()
            lines.append(f"  {s.index + 1:>2} {s.name:<16} {t:6.2f}s +{s.length:5.2f}s"
                         + (f"  {self.music.where(t)}" if self.music else "")
                         + ("  card" if s.card is not None else f"  ticks {whole[0]}-{whole[-1]}")
                         + f"  speed {s.speed}  {s.cam['type']:<6} {s.mode():<8} cut {s.cut}"
                         + (f" {s.cut_len:g}s" if s.cut != "cut" else "") + (f"  zoom {s.zoom}" if s.zoom != 1.0 else "")
                         + (f"  take {s.take_key}" if s.take_key != "main" else "")
                         + (f"  grade {s.grade}" if s.grade != "none" else ""))
        end = self.timeline()[-1][2]
        lines.append(f"  total {end:.2f} s, {sum(s.n for s in self.shots)} frames"
                     + (f", {self.music.bpm:g} bpm, ends {self.music.where(end)}" if self.music else "")
                     + (f", {len(self.sfx)} sfx" if self.sfx else ""))
        return "\n".join(lines)


class Music:
    """The scene's tempo map and music track, shared with the score so cuts land on the music:

        [music]
        bpm = 120
        meter = 4                    # beats a bar
        start = 0.0                  # film seconds of bar 1 beat 1
        file = "score.wav"           # the score (relative to the scene, $VARS expanded); mixed into the edit
        gain = 0.0                   # dB
        snap = true                  # check warns when a cut is off the beat grid (by more than half a frame)
        grid = "beat"                # what a cut must land on: beat | bar | half (half a beat)

    Shot lengths, cut lengths, cue and sfx times then take "2 bars", "3 beats", "1 bar 2 beats", "0.5 beats";
    positions on the film clock take "bar 5" or "bar 5 beat 3"."""

    def __init__(self, d, scene_path):
        if not isinstance(d, dict) or not isinstance(d.get("bpm"), (int, float)) or not 20 <= d["bpm"] <= 300:
            raise SceneError("[music] needs bpm (20..300)")
        self.bpm = float(d["bpm"])
        self.meter = int(d.get("meter", 4))
        self.start = float(d.get("start", 0.0))
        self.gain = float(d.get("gain", 0.0))
        self.snap = bool(d.get("snap", True))
        self.grid = str(d.get("grid", "beat"))
        if self.grid not in ("beat", "bar", "half"):
            raise SceneError(f"[music] grid {self.grid!r}: beat, bar or half")
        f = d.get("file")
        self.file = None
        if f:
            p = os.path.expandvars(str(f))
            self.file = os.path.normpath(p if os.path.isabs(p) else os.path.join(os.path.dirname(scene_path), p))
        self.beat = 60.0 / self.bpm

    def seconds(self, v, where, what, absolute):
        words = v.lower().replace(",", " ").split()
        try:
            if words and words[0] == "bar":   # a position: bar N [beat M]
                bar = float(words[1])
                beat = float(words[3]) if len(words) >= 4 and words[2] == "beat" else 1.0
                if len(words) not in (2, 4):
                    raise ValueError
                return self.start + ((bar - 1) * self.meter + (beat - 1)) * self.beat
            total, i = 0.0, 0
            while i < len(words):
                n, unit = float(words[i]), words[i + 1]
                if unit in ("bar", "bars"):
                    total += n * self.meter * self.beat
                elif unit in ("beat", "beats"):
                    total += n * self.beat
                elif unit in ("s", "sec", "seconds"):
                    total += n
                else:
                    raise ValueError
                i += 2
            return (self.start + total) if absolute else total
        except (ValueError, IndexError):
            raise SceneError(f'{where}: {what} {v!r}: seconds, "2 bars", "3 beats", "1 bar 2 beats" or "bar 5 beat 3"')

    def where(self, t):
        """A film time as bar.beat (1-based), with a + when it is off the beat."""
        b = (t - self.start) / self.beat
        whole = round(b)
        mark = "" if abs(b - whole) < 1e-6 else "+"
        k = whole if not mark else int(math.floor(b))
        return f"bar {k // self.meter + 1}.{k % self.meter + 1}{mark}"

    def off_grid(self, t, fps):
        """How far t is from the grid, in frames (0 on it)."""
        step = self.beat * {"beat": 1, "bar": self.meter, "half": 0.5}[self.grid]
        x = (t - self.start) / step
        return abs(x - round(x)) * step * fps


class Camera:
    """A shot's camera, evaluated at shot fraction u (0..1), shot seconds t and demo tick."""

    TYPES = ("fixed", "keys", "orbit", "track", "dolly")

    def __init__(self, scene, shot):
        self.scene, self.shot, self.c = scene, shot, shot.cam
        self.type = self.c["type"]
        if self.type not in self.TYPES:
            raise SceneError(f"{shot.where}: camera type {self.type!r}: one of {', '.join(self.TYPES)}")
        self.ease = self.c.get("ease", "inout" if self.type in ("orbit", "dolly", "keys") else "linear")
        if self.ease not in EASES:
            raise SceneError(f"{shot.where}: ease {self.ease!r}: one of {', '.join(EASES)}")
        self.smooth = float(self.c.get("smooth", 4))   # ticks of averaging on an actor the camera follows (0: exact)
        for name in self.actors_of(self.c):
            scene.actor(name, shot)   # raises a clear error for an unknown actor
        if self.type == "orbit":
            self.base_heading = None

    @staticmethod
    def actors_of(c):
        out = []
        for key in ("actor", "around", "look"):
            v = c.get(key)
            if isinstance(v, str):
                out.append(v)
        return out

    def _need(self, key):
        if key not in self.c:
            raise SceneError(f"{self.shot.where}: camera {self.type} needs {key}")
        return self.c[key]

    def _point(self, v, tick, what):
        """[x, y, z] or an actor name (its feet, smoothed)."""
        if isinstance(v, str):
            return self.scene.actor(v, self.shot).smooth(tick, self.smooth)
        if isinstance(v, list) and len(v) == 3 and all(isinstance(x, (int, float)) for x in v):
            return [float(x) for x in v]
        raise SceneError(f"{self.shot.where}: {what} must be [x, y, z] or an actor name, not {v!r}")

    def _look(self, tick, default=None):
        v = self.c.get("look", default)
        if v is None:
            return None
        p = self._point(v, tick, "look")
        if isinstance(v, str):
            p = [p[0], p[1], p[2] + float(self.c.get("look_height", 45))]
        return p

    def _segments(self, keys, times, t, tick):
        """keys with path = "segments": key to key in shot seconds, each segment eased on its own (the arriving key's
        `ease`, else the camera's), the position lerped and the view angles lerped (yaw the short way round). Before
        the first key and after the last the camera holds: a move that starts and settles exactly where its keys say,
        with no spline overshoot (cut 6 of the Maid Lash trailer: a Catmull-Rom between two looks leaves and arrives
        at speed, so a pan cannot settle on a subject)."""
        poses = []
        for k in keys:
            pos = self._point(k.get("pos"), tick, "pos") if "pos" in k else None
            if pos is None:
                raise SceneError(f"{self.shot.where}: every key needs pos")
            if "look" in k:
                p = self._point(k["look"], tick, "look")
                if isinstance(k["look"], str):
                    p = [p[0], p[1], p[2] + float(k.get("look_height", self.c.get("look_height", 45)))]
                pitch, yaw = look_angles(pos, p)
            elif "ang" in k:
                pitch, yaw = float(k["ang"][0]), float(k["ang"][1]) % 360
            else:
                raise SceneError(f"{self.shot.where}: every key needs look or ang")
            kind = k.get("ease", self.ease)
            if kind not in EASES:
                raise SceneError(f"{self.shot.where}: key ease {kind!r}: one of {', '.join(EASES)}")
            poses.append((pos, pitch, yaw, kind))
        if t <= times[0]:
            pos, pitch, yaw, _ = poses[0]
            return list(pos), pitch, yaw
        if t >= times[-1]:
            pos, pitch, yaw, _ = poses[-1]
            return list(pos), pitch, yaw
        i = max(j for j in range(len(times) - 1) if times[j] <= t)
        span = times[i + 1] - times[i]
        a, b = poses[i], poses[i + 1]
        s = ease((t - times[i]) / span if span > 0 else 1.0, b[3])
        return [lerp(a[0][j], b[0][j], s) for j in range(3)], lerp(a[1], b[1], s), lerp_angle(a[2], b[2], s) % 360

    def at(self, u, t, tick):
        c = self.c
        e = ease(u, self.ease)
        if self.type == "fixed":
            pos = self._point(self._need("pos"), tick, "pos")
            look = self._look(tick)
            if look is None:
                ang = c.get("ang") or self._need("look")
                return pos, float(ang[0]), float(ang[1]) % 360
            p, y = look_angles(pos, look)
            return pos, p, y
        if self.type == "dolly":
            a = self._point(self._need("from"), tick, "from")
            b = self._point(self._need("to"), tick, "to")
            pos = [lerp(a[i], b[i], e) for i in range(3)]
            look = self._look(tick)
            if look is None:
                raise SceneError(f"{self.shot.where}: dolly needs look (a point or an actor)")
            p, y = look_angles(pos, look)
            return pos, p, y
        if self.type == "keys":
            keys = self._need("keys")
            if not isinstance(keys, list) or len(keys) < 2:
                raise SceneError(f"{self.shot.where}: keys needs at least two {{t, pos, look|ang}} keys")
            times = [float(k.get("t", i * self.shot.length / (len(keys) - 1))) for i, k in enumerate(keys)]
            if times != sorted(times):
                raise SceneError(f"{self.shot.where}: key times must increase: {times}")
            path = c.get("path", "spline")
            if path == "segments":
                return self._segments(keys, times, t, tick)
            if path != "spline":
                raise SceneError(f"{self.shot.where}: keys path {path!r}: spline or segments")
            tt = times[0] + (times[-1] - times[0]) * e
            pos = spline([self._point(k["pos"], tick, "pos") for k in keys], times, tt)
            if all("look" in k for k in keys):
                looks = []
                for k in keys:
                    p = self._point(k["look"], tick, "look")
                    if isinstance(k["look"], str):
                        p = [p[0], p[1], p[2] + float(k.get("look_height", c.get("look_height", 45)))]
                    looks.append(p)
                p, y = look_angles(pos, spline(looks, times, tt))
                return pos, p, y
            if all("ang" in k for k in keys):
                i = max(j for j in range(len(times)) if times[j] <= tt) if tt > times[0] else 0
                j = min(i + 1, len(times) - 1)
                s = 0 if i == j else (tt - times[i]) / (times[j] - times[i])
                a0, a1 = keys[i]["ang"], keys[j]["ang"]
                return pos, lerp(a0[0], a1[0], s), lerp_angle(a0[1], a1[1], s) % 360
            raise SceneError(f"{self.shot.where}: every key needs look (all of them) or ang (all of them)")
        if self.type == "orbit":
            centre = self._point(self._need("around"), tick, "around")
            frm, to = float(c.get("from", 0)), float(c.get("to", 360))
            base = 0.0
            if c.get("relative", "world") == "heading" and isinstance(c.get("around"), str):
                tr = self.scene.actor(c["around"], self.shot)
                base = tr.heading(self.shot.tick)   # the actor's heading at the shot's first tick, held
            ang = math.radians(base + lerp(frm, to, e))
            r = lerp(float(c.get("radius", 170)), float(c.get("radius_to", c.get("radius", 170))), e)
            h = lerp(float(c.get("height", 50)), float(c.get("height_to", c.get("height", 50))), e)
            pos = [centre[0] + r * math.cos(ang), centre[1] + r * math.sin(ang), centre[2] + h]
            p, y = look_angles(pos, [centre[0], centre[1], centre[2] + float(c.get("look_height", 45))])
            return pos, p, y
        if self.type == "track":
            name = self._need("actor")
            tr = self.scene.actor(name, self.shot)
            feet = tr.smooth(tick, self.smooth)
            frame = c.get("frame", "heading")
            if frame == "heading":
                # the heading is held steady: averaged over a long window so a camera on a runner does not swing
                hd = math.radians(tr.heading(tick, window=int(c.get("heading_window", 24))))
            elif frame == "world":
                hd = 0.0
            else:
                raise SceneError(f"{self.shot.where}: track frame {frame!r}: heading or world")
            off = c.get("offset", [-250, 0, 80])
            off_to = c.get("offset_to", off)
            if not (isinstance(off, list) and len(off) == 3 and isinstance(off_to, list) and len(off_to) == 3):
                raise SceneError(f"{self.shot.where}: offset / offset_to: [forward, left, up]")
            o = [lerp(off[i], off_to[i], e) for i in range(3)]
            fx, fy = math.cos(hd), math.sin(hd)
            pos = [feet[0] + o[0] * fx - o[1] * fy, feet[1] + o[0] * fy + o[1] * fx, feet[2] + o[2]]
            look = self._look(tick, default=name)
            lead = float(c.get("lead", 0))   # look this many units ahead of the actor along its travel
            look = [look[0] + lead * fx, look[1] + lead * fy, look[2]]
            p, y = look_angles(pos, look)
            return pos, p, y
        raise SceneError(f"{self.shot.where}: camera type {self.type!r}")


def load(path):
    return Scene(path)


if __name__ == "__main__":
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    sc = load(sys.argv[1])
    print(sc.summary())
    for w in sc.check():
        print("warning:", w)
