"""Actor tracks: where every hero of a SourceTV take is at every tick, so a shot can follow an actor by name.

    python -I track.py extract <take.dem>          # Boon (its venv) -> work\\cinema\\tracks\\<take>.json
    python -I track.py list <take.dem>             # the actors in a take and the ticks they are seen at
    python -I track.py at <take.dem> <actor> <tick>  # position and heading of an actor at a tick

The parser is Boon (`boon-deadlock`, MIT; Rust core, Python bindings) in its own venv, run as a subprocess, so the
kit itself stays stdlib-only. Boon's `player_ticks` gives the pawn origin (the feet) and the camera angles of every
hero pawn, players and bots alike, at every recorded tick; its tick numbers are the demo's own (`demo_gototick`).
SourceTV records a frame every 64 / tv_snapshotrate ticks (3 at the default rate; `session.py` launches with 64).

Actors are named `<Hero>` (the first pawn of that hero in the take) or `<Hero>#<n>` (n from 0, by first appearance):
`Lash`, `Sinclair`, `Lash#1`. Hero names are Boon's display names (Sinclair is `hero_magician`).
"""
import json
import math
import os
import subprocess
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "ingame"))
import gamepaths  # noqa: E402  (locations as magician.exe resolves them)

HERE = os.path.dirname(os.path.abspath(__file__))
# Boon (pip install boon-deadlock) lives in its own venv: CINEMA_VENV_PY, else the config's cinema_venv
VENV_PY = os.environ.get("CINEMA_VENV_PY") or os.path.join(gamepaths.config("cinema_venv") or "cinema-venv", "Scripts", "python.exe")
WORK = os.environ.get("CINEMA_WORK") or gamepaths.work("cinema")
REPLAYS = os.path.join(gamepaths.try_game(), "citadel", "replays")
TICK_RATE = 64

# Hero ids Boon reports, by the internal names the console uses (citadel_create_unit hero_<name>)
INTERNAL = {"Sinclair": "hero_magician", "Lash": "hero_lash"}

EXTRACT = r'''
import json, sys
from boon import Demo, hero_names
d = Demo(sys.argv[1], preload=False)
df = d.snapshots("player_ticks", every=1)
names = hero_names()
cols = ["tick", "hero_id", "steam_id", "x", "y", "z", "pitch", "yaw", "health", "is_alive"]
rows = df.select(cols).sort("tick").iter_rows()
out = {"take": sys.argv[1], "build": d.build, "map": d.map_name, "total_ticks": d.total_ticks,
       "tick_rate": d.tick_rate, "pawns": {}}
for tick, hid, sid, x, y, z, p, yw, hp, alive in rows:
    # one pawn per (hero, steam id); bots of one hero share a null id, so split them by position continuity later
    key = f"{hid}:{sid}"
    pw = out["pawns"].setdefault(key, {"hero": names.get(hid, str(hid)), "hero_id": hid, "steam_id": sid, "samples": []})
    pw["samples"].append([tick, round(x, 3), round(y, 3), round(z, 3), round(p, 2), round(yw, 2), hp, bool(alive)])
# ability casts with their ticks (what a shot can be anchored to: tick = "cast down_strike")
out["abilities"] = [[t, names.get(h, str(h)), a] for t, h, a in d.abilities.select(["tick", "hero_id", "ability"]).sort("tick").iter_rows()]
json.dump(out, open(sys.argv[2], "w"))
print(len(out["pawns"]), "pawns")
'''


class TrackError(RuntimeError):
    pass


def take_path(take):
    if os.path.isabs(take) and os.path.exists(take):
        return take
    for cand in (os.path.join(REPLAYS, take), os.path.join(REPLAYS, os.path.basename(take)), take):
        if os.path.exists(cand):
            return os.path.abspath(cand)
    raise TrackError(f"take {take!r}: not found (looked in {REPLAYS})")


def cache_path(take):
    return os.path.join(WORK, "tracks", os.path.splitext(os.path.basename(take))[0] + ".json")


def extract(take, force=False):
    take = take_path(take)
    out = cache_path(take)
    if os.path.exists(out) and not force and os.path.getmtime(out) >= os.path.getmtime(take):
        with open(out, encoding="utf-8") as f:
            if '"abilities"' in f.read():   # caches from before abilities were kept are read again
                return out
    if not os.path.exists(VENV_PY):
        raise TrackError(f"no Boon venv at {VENV_PY}: python -m venv <dir>; <dir>\\Scripts\\pip install boon-deadlock; "
                         "then set CINEMA_VENV_PY or add `cinema_venv = <dir>` to the config file")
    os.makedirs(os.path.dirname(out), exist_ok=True)
    env = dict(os.environ, PYTHONIOENCODING="utf-8")
    r = subprocess.run([VENV_PY, "-I", "-c", EXTRACT, take, out], capture_output=True, text=True, env=env)
    if r.returncode != 0:
        raise TrackError(f"Boon could not read {take}:\n{r.stderr[-1500:]}")
    return out


def _split_bots(pawn):
    """Bots of one hero share a steam id (null): split the sample stream into pawns by position continuity
    (several samples at the same tick are different units)."""
    by_tick = {}
    for s in pawn["samples"]:
        by_tick.setdefault(s[0], []).append(s)
    tracks = []   # each: list of samples
    for tick in sorted(by_tick):
        free = list(by_tick[tick])
        for tr in tracks:
            last = tr[-1]
            if not free or last[0] == tick:
                continue
            j = min(range(len(free)), key=lambda i: math.dist(free[i][1:4], last[1:4]))
            if math.dist(free[j][1:4], last[1:4]) < 64 * max(1, tick - last[0]) + 400:   # a teleport is a new life, still the unit
                tr.append(free.pop(j))
        tracks.extend([s] for s in free)
    return tracks


class Tracks:
    """Every actor of a take: name -> Track."""

    def __init__(self, take):
        self.take = take_path(take)
        with open(extract(self.take), encoding="utf-8") as fh:
            data = json.load(fh)
        self.total_ticks = data["total_ticks"]
        self.map = data.get("map")
        self.abilities = [tuple(a) for a in data.get("abilities", [])]   # (tick, hero, ability)
        streams = []
        for pawn in data["pawns"].values():
            for samples in (_split_bots(pawn) if pawn["steam_id"] is None else [pawn["samples"]]):
                streams.append((samples[0][0], pawn["hero"], samples))
        streams.sort(key=lambda s: s[0])
        self.actors = {}
        count = {}
        for first, hero, samples in streams:
            n = count.get(hero, 0)
            count[hero] = n + 1
            t = Track(f"{hero}#{n}", hero, samples)
            self.actors[t.name] = t
            if n == 0:
                self.actors[hero] = t

    def get(self, name):
        if name in self.actors:
            return self.actors[name]
        low = {k.lower(): v for k, v in self.actors.items()}
        if name.lower() in low:
            return low[name.lower()]
        raise TrackError(f"actor {name!r} is not in {os.path.basename(self.take)}; it has: "
                         + ", ".join(sorted(k for k in self.actors if "#" in k)))

    def names(self):
        return sorted(k for k in self.actors if "#" in k)


class PlannedTracks:
    """A take that has not been shot: the scene's `[blocking]` table, the actors' planned paths, standing in for
    a SourceTV take so a scene can be checked and seen as an animatic before anything is staged. Same face as
    Tracks (names, get, actors, abilities, total_ticks, map).

        [blocking]
        map = "dl_midtown"
        length = 40.0                           # seconds of planned take (tick = 64 * seconds)
        casts = [[30.5, "maid", "ultimate"]]    # (seconds, actor, ability): for tick = "cast maid ultimate"
        [blocking.actors.maid]
        hero = "Lash"
        keys = [{ t = 0.0, pos = [x, y, z], yaw = 90 }, { t = 2.0, pos = [...] }]   # yaw held when omitted

    Between keys the feet follow a Catmull-Rom spline (positions) and the yaw turns the short way. Two keys at
    the same t are a teleport (an actor re-placed between takes)."""

    def __init__(self, blocking, label="blocking"):
        if not isinstance(blocking, dict) or not isinstance(blocking.get("actors"), dict) or not blocking["actors"]:
            raise TrackError("[blocking] needs [blocking.actors.<name>] tables with keys = [{t, pos, yaw}]")
        self.take = label
        self.map = blocking.get("map", "planned")
        length = float(blocking.get("length", 0))
        self.actors = {}
        last = 0.0
        for name, a in blocking["actors"].items():
            keys = a.get("keys") if isinstance(a, dict) else None
            if not isinstance(keys, list) or not keys:
                raise TrackError(f"[blocking.actors.{name}] needs keys = [{{t = 0, pos = [x, y, z]}}, ...]")
            ts, ps, ys = [], [], []
            yaw = float(keys[0].get("yaw", 0.0))
            for k in keys:
                if "t" not in k or not (isinstance(k.get("pos"), list) and len(k["pos"]) == 3):
                    raise TrackError(f"[blocking.actors.{name}] key {k!r}: t (seconds) and pos [x, y, z]")
                if ts and float(k["t"]) < ts[-1]:
                    raise TrackError(f"[blocking.actors.{name}]: key times must not go back ({k['t']} after {ts[-1]})")
                yaw = float(k.get("yaw", yaw))
                ts.append(float(k["t"]))
                ps.append([float(v) for v in k["pos"]])
                ys.append(yaw)
            last = max(last, ts[-1])
            self.actors[str(name)] = Track(str(name), str(a.get("hero", name)), _plan_samples(ts, ps, ys, length))
        self.total_ticks = int(round(max(length, last) * TICK_RATE))
        self.abilities = []
        for c in blocking.get("casts", []):
            if not (isinstance(c, list) and len(c) == 3 and str(c[1]) in self.actors):
                raise TrackError(f"[blocking] casts entry {c!r}: [seconds, actor, ability] with an actor of [blocking.actors]")
            self.abilities.append((int(round(float(c[0]) * TICK_RATE)), self.actors[str(c[1])].hero, str(c[2])))
        self.abilities.sort()

    def get(self, name):
        if name in self.actors:
            return self.actors[name]
        raise TrackError(f"actor {name!r} is not in the blocking; it has: {', '.join(sorted(self.actors))}")

    def names(self):
        return sorted(self.actors)


def _plan_samples(ts, ps, ys, length):
    """One sample a tick from planned keys: [tick, x, y, z, pitch, yaw, hp, alive], held before the first key and
    after the last up to `length` seconds."""
    def cat(p0, p1, p2, p3, s):
        s2, s3 = s * s, s * s * s
        return [0.5 * (2 * b + (-a + c) * s + (2 * a - 5 * b + 4 * c - d) * s2 + (-a + 3 * b - 3 * c + d) * s3)
                for a, b, c, d in zip(p0, p1, p2, p3)]

    end = int(round(max(length, ts[-1]) * TICK_RATE))
    out = []
    for tick in range(0, end + 1):
        t = tick / TICK_RATE
        if t <= ts[0] or len(ts) == 1:
            p, y = ps[0], ys[0]
        elif t >= ts[-1]:
            p, y = ps[-1], ys[-1]
        else:
            i = max(k for k in range(len(ts) - 1) if ts[k] <= t)
            while i + 1 < len(ts) and ts[i + 1] == ts[i]:
                i += 1
            span = ts[i + 1] - ts[i]
            s = (t - ts[i]) / span if span > 0 else 1.0
            p0 = ps[i - 1] if i > 0 and ts[i - 1] < ts[i] else ps[i]
            p3 = ps[i + 2] if i + 2 < len(ps) and ts[i + 2] > ts[i + 1] else ps[i + 1]
            p = cat(p0, ps[i], ps[i + 1], p3, s)
            y = ys[i] + ((ys[i + 1] - ys[i] + 180) % 360 - 180) * s
        out.append([tick, round(p[0], 3), round(p[1], 3), round(p[2], 3), 0.0, round(y % 360, 2), 100, True])
    return out


class Track:
    def __init__(self, name, hero, samples):
        self.name, self.hero = name, hero
        self.ticks = [s[0] for s in samples]
        self.pos = [s[1:4] for s in samples]
        self.yaw = [s[5] for s in samples]
        self.alive = [s[7] for s in samples]

    def span(self):
        return self.ticks[0], self.ticks[-1]

    def _index(self, tick):
        lo, hi = 0, len(self.ticks) - 1
        if tick <= self.ticks[0]:
            return 0, 0.0
        if tick >= self.ticks[-1]:
            return hi, 0.0
        while hi - lo > 1:
            mid = (lo + hi) // 2
            if self.ticks[mid] <= tick:
                lo = mid
            else:
                hi = mid
        return lo, (tick - self.ticks[lo]) / (self.ticks[lo + 1] - self.ticks[lo])

    def at(self, tick):
        """Feet position at a (fractional) tick: Catmull-Rom through the samples (a jump between samples of more
        than 300 units, a teleport or a respawn, is not smoothed across)."""
        i, s = self._index(tick)
        n = len(self.pos)
        p1 = self.pos[i]
        if s == 0.0 or i + 1 >= n:
            return list(p1)
        p2 = self.pos[i + 1]
        if math.dist(p1, p2) > 300:
            return list(p1 if s < 0.5 else p2)
        p0 = self.pos[i - 1] if i > 0 and math.dist(self.pos[i - 1], p1) < 300 else p1
        p3 = self.pos[i + 2] if i + 2 < n and math.dist(self.pos[i + 2], p2) < 300 else p2
        s2, s3 = s * s, s * s * s
        return [0.5 * (2 * b + (-a + c) * s + (2 * a - 5 * b + 4 * c - d) * s2 + (-a + 3 * b - 3 * c + d) * s3)
                for a, b, c, d in zip(p0, p1, p2, p3)]

    def smooth(self, tick, window):
        """The position averaged over +-window ticks (a steadier subject for a camera to follow)."""
        if window <= 0:
            return self.at(tick)
        k = 9
        pts = [self.at(tick + window * (2 * j / (k - 1) - 1)) for j in range(k)]
        w = [1 - abs(2 * j / (k - 1) - 1) + 0.25 for j in range(k)]
        tw = sum(w)
        return [sum(p[c] * ww for p, ww in zip(pts, w)) / tw for c in range(3)]

    def heading(self, tick, window=16, still_speed=40.0):
        """Direction of travel in degrees (yaw), from the velocity over +-window ticks; standing still (under
        still_speed units/s), the pawn's camera yaw."""
        a, b = self.at(tick - window), self.at(tick + window)
        dx, dy = b[0] - a[0], b[1] - a[1]
        speed = math.hypot(dx, dy) / (2 * window / TICK_RATE)
        if speed < still_speed:
            i, _ = self._index(tick)
            return self.yaw[i] % 360
        return math.degrees(math.atan2(dy, dx)) % 360

    def speed(self, tick, window=8):
        a, b = self.at(tick - window), self.at(tick + window)
        return math.dist(a, b) / (2 * window / TICK_RATE)


def main(a):
    if len(a) < 2:
        sys.exit(__doc__)
    if a[0] == "extract":
        print(extract(a[1], force="--force" in a))
    elif a[0] == "list":
        tr = Tracks(a[1])
        print(f"{os.path.basename(tr.take)}: {tr.total_ticks} ticks ({tr.total_ticks / TICK_RATE:.0f} s), map {tr.map}")
        for n in tr.names():
            t = tr.actors[n]
            a0, a1 = t.span()
            print(f"  {n:<16} ticks {a0}-{a1}  first at ({t.pos[0][0]:.0f}, {t.pos[0][1]:.0f}, {t.pos[0][2]:.0f})"
                  f"  {len(t.ticks)} samples")
    elif a[0] == "at":
        t = Tracks(a[1]).get(a[2])
        tick = float(a[3])
        p = t.at(tick)
        print(f"{t.name} at tick {tick:g}: ({p[0]:.1f}, {p[1]:.1f}, {p[2]:.1f}) heading {t.heading(tick):.0f} "
              f"speed {t.speed(tick):.0f} u/s")
    else:
        sys.exit(__doc__)


if __name__ == "__main__":
    main(sys.argv[1:])
