"""Film a staged SourceTV take from free cameras (the take staged with ingame/takes.py stage, or any take).

    python -I replay.py casts <take.dem>                                    # the take's casts (tick, hero, ability)
    python -I replay.py write <take.dem> <marks.tsv> <w0> <shots.txt> <out scenario.txt> [--load] [--actor NAME]
    python -I replay.py film  <take.dem> <marks.tsv> <w0> <shots.txt> <out dir> [--load] [--fps 30] [--actor NAME]

`write` makes a console.py scenario of PrintWindow bursts: exact frames, but a full-resolution burst costs two seconds
a frame to encode (a 50-frame shot took over two minutes, 2026-11-24). `film` drives the playback itself and records
each shot off the screen in real time through record.py (ffmpeg, lossless FFV1, <out>/<shot>.mkv): about 25 s a
shot. Each recording opens with the console on screen and the 1.5 s warm-up. The caller holds the game seat (the
game launched for it); the game window must be in front and uncovered. ingame/mod_session.py film runs this for a mod.

A film stops when the demo has ended: once the log says "Demo playback finished" (or a demo command answers "Not
currently playing back a demo"), every later shot would record the main menu (one run filmed the menu for three
shots, 2026-11-24). The shot that was recording when it ended is renamed <shot>.after_end.mkv, the rest are listed as
not filmed, and the run exits 6. A shot that starts past the take's last tick is skipped before seeking.

<marks.tsv> is takes.py stage's (wall time, beat, seconds). <w0> is the wall-clock time of the take's tick 0: take one
cast whose beat and press time you know, w0 = wall time of the press - tick / 64 (the recording starts a few seconds
before the log's "Recording SourceTV replay" line, so do not derive it from the log). Use 0 with tick= shots only.

<shots.txt>, one shot a line (blank lines and # skipped):
    NAME  beat=large_a | tick=12345   [at=0.5] [dur=6] [actor=Haze#0] [dist=110] [bear=h+90 | 144] [height=45]
          [look=40] [fps=10] [scale=0.5] [half] [anchor=2.5] [pos=x,y,z ang=pitch,yaw]
at: seconds into the beat where the film starts (may be negative). dur: seconds of take filmed. The camera stands
<dist> units from the actor's feet at the shot's first tick (or `anchor` seconds into the beat), on bearing <bear>
(degrees; h+N is relative to the way the actor faces: h+0 in front, h+90 on its left, h+180 behind), lens <height>
above the feet, looking at a point <look> above them. pos/ang give the camera outright. scale is demo_timescale;
frames are full resolution unless `half`. actor defaults to --actor, else the take's first actor. The spectator's
lens is fixed at 90 degrees: 110 units is a close view (ingame/README.md). Every shot seeks 1.5 s early and plays up
to its first frame, so cloth arrives simulated (README.md).
"""
import math
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import track as T  # noqa: E402

WARM = 1.5
TICKS = 64
DEMO_ENDED = ("Demo playback finished", "Not currently playing back a demo")


# ---- pure parts (test_replay.py) -----------------------------------------------------------------------------------

def read_marks(text):
    """marks.tsv -> {beat: (wall time, seconds)}; the last beat of a name wins."""
    beats = {}
    for line in text.splitlines():
        if line.strip():
            w, name, secs = line.split("\t")
            beats[name] = (float(w), float(secs))
    return beats


def read_shots(text):
    """shots.txt -> [(name, {key: value}, flags)]."""
    out = []
    for raw in text.splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        w = line.split()
        out.append((w[0], dict(x.split("=", 1) for x in w[1:] if "=" in x), {x for x in w[1:] if "=" not in x}))
    return out


def shot_ticks(k, beats, w0):
    """(first tick of the beat or tick=, the shot's first tick, seconds filmed)."""
    if "tick" in k:
        t0 = int(k["tick"])
    else:
        if k.get("beat") not in beats:
            raise KeyError(f"no beat {k.get('beat')!r} in the marks")
        t0 = int(round((beats[k["beat"]][0] - w0) * TICKS))
    tick = t0 + int(round(float(k.get("at", 0)) * TICKS))
    dur = float(k.get("dur", beats.get(k.get("beat"), (0, 6))[1]))
    return t0, tick, dur


def demo_ended(log_text):
    return any(s in log_text for s in DEMO_ENDED)


def camera(k, tr, t0, tick, default_actor):
    if "pos" in k:
        x, y, z = (float(v) for v in k["pos"].split(","))
        pitch, yaw = (float(v) for v in k["ang"].split(","))
        return x, y, z, pitch, yaw
    act = tr.get(k.get("actor", default_actor))
    ta = t0 + int(round(float(k["anchor"]) * TICKS)) if "anchor" in k else tick
    px, py, pz = act.at(ta)[:3]
    b = k.get("bear", "h+90")
    if b.startswith("h"):
        b = act.heading(ta) + float(b[1:] or 0)
    b = math.radians(float(b))
    d, h, lk = float(k.get("dist", 110)), float(k.get("height", 45)), float(k.get("look", 40))
    x, y, z = px + d * math.cos(b), py + d * math.sin(b), pz + h
    yaw = math.degrees(math.atan2(py - y, px - x))
    pitch = math.degrees(math.atan2(z - (pz + lk), d))
    return x, y, z, pitch, yaw


# ---- the game ------------------------------------------------------------------------------------------------------

def main(a):
    if not a or a[0] not in ("casts", "write", "film") or (a[0] != "casts" and len(a) < 6):
        sys.exit(__doc__)
    tr = T.Tracks(a[1])
    if a[0] == "casts":
        for c in tr.abilities:
            print(*c)
        print("actors:", ", ".join(tr.names()))
        return 0
    take, marks, w0, shots, out = a[1], a[2], float(a[3]), a[4], a[5]
    actor = a[a.index("--actor") + 1] if "--actor" in a else (tr.names() or ["?"])[0]
    beats = read_marks(open(marks, encoding="utf-8").read()) if os.path.exists(marks) else {}
    film = a[0] == "film"
    name_of_take = os.path.splitext(os.path.basename(take))[0]
    o = []
    if film:
        import subprocess
        import time
        sys.path.insert(0, os.path.join(os.path.dirname(HERE), "ingame"))
        import console as C
        os.makedirs(out, exist_ok=True)
        rfps = a[a.index("--fps") + 1] if "--fps" in a else "30"
        con = C.Console(a[a.index("--key") + 1] if "--key" in a else None)
        C.release_stuck()
        C.focus_game()
        started = C.log_size()     # demo-end lines are looked for from here on

        def ended():
            return demo_ended(C.log_since(started))
    if "--load" in a:
        o += ["playdemo replays/" + name_of_take, '@log "Playing Demo" 120', "@wait 8"]
        if film:
            con.run("playdemo replays/" + name_of_take)
            C.wait_log("Playing Demo", 120, started)
            time.sleep(8)
    o += ["citadel_hud_visible 0", "demo_pause", "spec_mode 6"]
    if film:
        for c in o[-3:]:
            con.run(c)
    plan = read_shots(open(shots, encoding="utf-8").read())
    for i, (name, k, flags) in enumerate(plan):
        t0, tick, dur = shot_ticks(k, beats, w0)
        scale, fps = float(k.get("scale", 0.5)), int(k.get("fps", 10))
        if tick > tr.total_ticks:
            print(f"{name}: skipped, tick {tick} is past the take's last ({tr.total_ticks})", flush=True)
            continue
        x, y, z, pitch, yaw = camera(k, tr, t0, tick, actor)
        if film:
            if ended():
                left = [n for n, _, _ in plan[i:]]
                print(f"DEMO ENDED before {name}: not filmed: {', '.join(left)}", flush=True)
                return 6
            con.run("demo_pause")
            off = C.log_size()
            con.run("demo_gototick %d" % max(1, tick - int(WARM * TICKS)))
            C.wait_log("Skipping finished", 90, off)
            time.sleep(1.5)
            con.run("spec_goto %.1f %.1f %.1f %.1f %.1f" % (x, y, z, pitch, yaw))
            time.sleep(0.4)
            secs = 1.0 + (WARM + dur) / scale + 0.5
            mkv = os.path.join(out, name + ".mkv")
            r = subprocess.run([sys.executable, "-I", os.path.join(HERE, "record.py"), mkv, "--seconds", "%.1f" % secs,
                                "--fps", rfps, "--then", "demo_timescale %g;demo_resume" % scale], capture_output=True, text=True)
            if ended():
                # the demo ran out while this shot was recording: what it holds after the end is the menu
                if os.path.exists(mkv):
                    os.replace(mkv, os.path.join(out, name + ".after_end.mkv"))
                left = [n for n, _, _ in plan[i + 1:]]
                print(f"DEMO ENDED during {name} (kept as {name}.after_end.mkv)"
                      + (f"; not filmed: {', '.join(left)}" if left else ""), flush=True)
                return 6
            con.run("demo_pause")
            print(name, (r.stdout.strip().splitlines() or ["?"])[-1], flush=True)
            continue
        frames = int(round(dur / scale * fps))
        o += ["# " + name + " " + " ".join(f"{a_}={b_}" for a_, b_ in k.items()),
              "demo_pause", "demo_gototick %d" % max(1, tick - int(WARM * TICKS)), '@log "Skipping finished" 90', "@wait 2",
              "spec_goto %.1f %.1f %.1f %.1f %.1f" % (x, y, z, pitch, yaw), "@wait 0.5",
              "demo_timescale 1;demo_resume", "@wait %.2f" % max(0.1, WARM - 1.4),      # the next command costs about 1.4 s
              "demo_timescale %g" % scale,
              "@burst %s %d %d%s" % (name, frames, fps, "" if "half" in flags else " full"),
              "demo_pause"]
    if film:
        print("FILM_DONE", out)
        return 0
    with open(out, "w", encoding="utf-8", newline="\n") as f:
        f.write("\n".join(o) + "\n")
    print(out, len(o), "lines")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
