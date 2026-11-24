"""Camera moves for the retail game, rendered frame by frame (the cinema kit's motion-control rig).

    python -I shoot.py plan   <shot.json>                     # print every frame's console line, check the path
    python -I shoot.py render <shot.json> --out <dir> [--from N] [--to N] [--settle 0.25] [--route keys|console]
    python -I shoot.py encode <dir> [--fps 30]                # frames -> preview.mp4 (x264) + master.mkv (FFV1, lossless)
    python -I shoot.py cfg    <shot.json> --write   # the frames as an alias chain, addon/cfg/cinema/<name>.cfg (build the pak after)

Retail Deadlock has no camera-path tool, no movie recorder and no smooth free camera a script can steer. What it
does have: a demo can be paused at a tick (`demo_pause`, `demo_gototick`, `demo_step_tick`), and while it plays,
the spectator camera goes wherever `spec_goto x y z pitch yaw` puts it, instantly. So a move is rendered as stills:
for each frame, set the demo's tick and the camera, let the frame settle, capture the window through PrintWindow.
Slow (about 2 s a frame through the console), exact (every frame is the same tick and pose on every run).

A shot file (JSON):

    {
      "name": "lash_orbit",
      "fps": 30, "seconds": 10,
      "camera": "spec",                 spec: spec_goto (demo playback); player: setpos_exact/setang_exact
      "time": {"mode": "frozen", "tick": 4200},
                                        frozen: one tick for the whole move (bullet time)
                                        demo:   {"mode": "demo", "tick": 4200, "ticks_per_frame": 2} (60 ticks/s at 30 fps)
                                        live:   leave time alone (host_timescale is yours)
      "path": {"type": "orbit", "center": [1494, -1196, 40], "radius": 170, "height": 30,
               "yaw_from": 200, "yaw_to": 380, "look_height": 0, "ease": "inout"},
          or {"type": "keys", "ease": "inout", "keys": [
               {"t": 0,  "pos": [x, y, z], "look": [x, y, z]},
               {"t": 10, "pos": [x, y, z], "ang": [pitch, yaw]} ]},
      "fov": 60,                        optional: citadel_camera_override_fov (only where the camera honours it)
      "pre": ["demo_pause", "demo_gototick 4200", "@wait 5", "cin_demo_clean"],   console lines (or @wait s) before frame 0
      "post": ["cin_demo_dirty"]          console lines after the last frame
    }

Positions are world units (setpos/getpos), angles degrees (pitch down positive, yaw counter-clockwise from +X).
Keyed paths are Catmull-Rom through the positions; "look" points are splined the same way, "ang" keys are
interpolated on the shortest arc. Ease: linear, in, out, inout (smoothstep on the whole move's time).
"""
import json
import math
import os
import subprocess
import sys
import threading
import time

HERE = os.path.dirname(os.path.abspath(__file__))
KEY_NEXT = "scrolllock"   # the key the alias-chain route taps per frame (console.py scancode name; bound as SCROLLLOCK)
SPEC_EYE = 0.0    # spec_goto z is the eye (2026-11-21: z 2 puts the lens at street level); the old 61 was the fly-up drift
INGAME = os.path.join(os.path.dirname(HERE), "ingame")
sys.path.insert(0, INGAME)


def ease(u, kind):
    u = min(1.0, max(0.0, u))
    if kind == "in":
        return u * u
    if kind == "out":
        return 1 - (1 - u) * (1 - u)
    if kind == "inout":
        return u * u * (3 - 2 * u)
    return u


def catmull(p0, p1, p2, p3, s):
    s2, s3 = s * s, s * s * s
    return [0.5 * (2 * b + (-a + c) * s + (2 * a - 5 * b + 4 * c - d) * s2 + (-a + 3 * b - 3 * c + d) * s3)
            for a, b, c, d in zip(p0, p1, p2, p3)]


def spline(points, times, t):
    """Catmull-Rom through points at times (ends duplicated), evaluated at t."""
    if t <= times[0]:
        return list(points[0])
    if t >= times[-1]:
        return list(points[-1])
    i = max(k for k in range(len(times) - 1) if times[k] <= t)
    s = (t - times[i]) / (times[i + 1] - times[i])
    p = [points[max(0, i - 1)], points[i], points[i + 1], points[min(len(points) - 1, i + 2)]]
    return catmull(*p, s)


def look_angles(pos, target):
    dx, dy, dz = (target[k] - pos[k] for k in range(3))
    yaw = math.degrees(math.atan2(dy, dx))
    pitch = -math.degrees(math.atan2(dz, math.hypot(dx, dy)))
    return pitch, yaw % 360


def lerp_angle(a, b, s):
    d = (b - a + 180) % 360 - 180
    return a + d * s


def frames(shot):
    """[(k, pos, pitch, yaw)] for every frame of the shot."""
    fps, seconds = shot.get("fps", 30), shot.get("seconds", 10)
    n = int(round(fps * seconds))
    path = shot["path"]
    out = []
    for k in range(n):
        u = k / max(1, n - 1)
        if path["type"] == "orbit":
            e = ease(u, path.get("ease", "linear"))
            cx, cy, cz = path["center"]
            yaw_deg = path.get("yaw_from", 0) + (path.get("yaw_to", 360) - path.get("yaw_from", 0)) * e
            r = path.get("radius", 170) + (path.get("radius_to", path.get("radius", 170)) - path.get("radius", 170)) * e
            h = path.get("height", 0) + (path.get("height_to", path.get("height", 0)) - path.get("height", 0)) * e
            a = math.radians(yaw_deg)
            pos = [cx + r * math.cos(a), cy + r * math.sin(a), cz + h]
            pitch, yaw = look_angles(pos, [cx, cy, cz + path.get("look_height", 0)])
        else:
            keys = path["keys"]
            times = [kk["t"] for kk in keys]
            t = times[0] + (times[-1] - times[0]) * ease(u, path.get("ease", "linear"))
            pos = spline([kk["pos"] for kk in keys], times, t)
            if all("look" in kk for kk in keys):
                pitch, yaw = look_angles(pos, spline([kk["look"] for kk in keys], times, t))
            else:
                angs = [kk.get("ang") or list(look_angles(kk["pos"], kk["look"])) for kk in keys]
                i = max(j for j in range(len(times)) if times[j] <= t) if t > times[0] else 0
                j = min(i + 1, len(times) - 1)
                s = 0 if i == j else (t - times[i]) / (times[j] - times[i])
                pitch = angs[i][0] + (angs[j][0] - angs[i][0]) * s
                yaw = lerp_angle(angs[i][1], angs[j][1], s) % 360
        out.append((k, pos, pitch, yaw))
    return out


def frame_line(shot, k, pos, pitch, yaw):
    cam = shot.get("camera", "spec")
    if cam == "spec":
        # spec_goto places the spectator's origin: its eye (the camera) is 61 units higher (spec_pos reads back z + 61)
        z = pos[2] - shot.get("spec_eye", SPEC_EYE)
        line = f"spec_goto {pos[0]:.2f} {pos[1]:.2f} {z:.2f} {pitch:.2f} {yaw:.2f}"
    elif cam == "player":
        line = f"setpos_exact {pos[0]:.2f} {pos[1]:.2f} {pos[2]:.2f};setang_exact {pitch:.2f} {yaw:.2f} 0"
    else:
        raise ValueError(f"camera {cam!r}: spec or player")
    tm = shot.get("time", {"mode": "live"})
    if tm["mode"] == "demo":
        tick = tm["tick"] + int(round(k * tm.get("ticks_per_frame", 2)))
        line = f"demo_gototick {tick};demo_pause;" + line
    elif tm["mode"] == "frozen" and k == 0:
        line = f"demo_gototick {tm['tick']};demo_pause;" + line
    return line


def plan(shot):
    lines = list(shot.get("pre", []))
    if "fov" in shot:
        lines.append(f"citadel_camera_override_fov {shot['fov']}")
    for k, pos, pitch, yaw in frames(shot):
        lines.append(frame_line(shot, k, pos, pitch, yaw))
    return lines


def chain_cfg(shot):
    """The frames as an alias chain: bind a key to cin_next and every press is the next frame (no typing)."""
    name = shot["name"]
    fr = frames(shot)
    out = [f"// generated by shoot.py cfg: {name}, {len(fr)} frames. exec cinema/{name}, then press the bound key per frame",
           f'alias cin_next "cin_{name}_00000"']
    for k, pos, pitch, yaw in fr:
        nxt = f"cin_{name}_{k + 1:05d}" if k + 1 < len(fr) else "cin_done"
        out.append(f'alias cin_{name}_{k:05d} "{frame_line(shot, k, pos, pitch, yaw)};alias cin_next {nxt}"')
    out.append('alias cin_done "echo cinema: last frame"')
    out.append(f'echo "cinema: {name} loaded ({len(fr)} frames); bind a key to cin_next"')
    return "\n".join(out) + "\n"


def render(shot, out_dir, first, last, settle, key, route="keys"):
    from console import Console, focus_game, tap
    from capture import find_window, client_rect, grab, write_png
    os.makedirs(os.path.join(out_dir, "frames"), exist_ok=True)
    focus_game()
    con = Console(key)
    for line in shot.get("pre", []):
        if line.startswith("@wait"):
            time.sleep(float(line.split()[1]))
            continue
        con.run(line)
        time.sleep(0.5)
    if "fov" in shot:
        con.run(f"citadel_camera_override_fov {shot['fov']}")
    # A seek keeps skipping (and the spectator camera keeps gliding) for a few seconds after the console returns:
    # park the camera on frame 0 and let it all come to rest before the first capture.
    if first == 0 or shot.get("pre"):
        k0 = frames(shot)[first]
        con.run(frame_line(shot, *k0).split("demo_pause;")[-1])
        time.sleep(shot.get("pre_wait", 4.0))
    hwnd = find_window()
    x, y, w, h = client_rect(hwnd)
    writers, t0 = [], time.perf_counter()
    fr = [f for f in frames(shot) if first <= f[0] <= last]
    if route == "keys":
        # The frames are an alias chain in the cinema pak (cfg/cinema/<name>.cfg, from `shoot.py cfg --write` before
        # the build): one tap of a bound key per frame, no console in between. Typing a line per frame through the
        # console proved unsafe over a long move (a 300-frame render drifted from frame 1: build 6864).
        con.run(f"exec cinema/{shot['name']};alias cin_next cin_{shot['name']}_{first:05d};bind {KEY_NEXT} cin_next")
        time.sleep(0.5)
    for i, (k, pos, pitch, yaw) in enumerate(fr):
        line = frame_line(shot, k, pos, pitch, yaw)
        if route == "keys":
            tap(KEY_NEXT)
        else:
            con.run(line)
        # a seek (and the first frame, which leaves the chase camera) needs longer to settle than a camera move
        time.sleep(settle + (1.0 if i == 0 or "demo_gototick" in line else 0))
        px = grab(x, y, w, h, hwnd)
        path = os.path.join(out_dir, "frames", f"f{k:05d}.png")
        th = threading.Thread(target=write_png, args=(path, w, h, px))
        th.start()
        writers.append(th)
        if len(writers) > 6:
            writers.pop(0).join()
        if i % 10 == 0:
            el = time.perf_counter() - t0
            print(f"frame {k} ({i + 1}/{len(fr)}), {el / (i + 1):.2f} s/frame")
    for th in writers:
        th.join()
    if route == "keys":
        con.run(f"unbind {KEY_NEXT}")
    for line in shot.get("post", []):   # put back what pre changed (archived cvars persist in the user's config)
        con.run(line)
        time.sleep(0.3)
    print(f"RENDER {len(fr)} frames {w}x{h} in {time.perf_counter() - t0:.0f} s -> {out_dir}")


def encode(out_dir, fps):
    src = os.path.join(out_dir, "frames", "f%05d.png")
    first = min(int(f[1:6]) for f in os.listdir(os.path.join(out_dir, "frames")) if f.endswith(".png"))
    common = ["ffmpeg", "-y", "-hide_banner", "-loglevel", "error", "-framerate", str(fps), "-start_number", str(first), "-i", src]
    subprocess.run(common + ["-c:v", "libx264", "-preset", "slow", "-crf", "12", "-pix_fmt", "yuv420p",
                             "-movflags", "+faststart", os.path.join(out_dir, "preview.mp4")], check=True)
    subprocess.run(common + ["-c:v", "ffv1", "-level", "3", "-pix_fmt", "bgr0", os.path.join(out_dir, "master.mkv")], check=True)
    for f in ("preview.mp4", "master.mkv"):
        p = os.path.join(out_dir, f)
        print(f"ENCODE {p} {os.path.getsize(p) / 1e6:.1f} MB")


if __name__ == "__main__":
    a = sys.argv[1:]

    def opt(name, default, cast=str):
        return cast(a[a.index(name) + 1]) if name in a else default

    if len(a) < 2:
        sys.exit(__doc__)
    if a[0] == "encode":
        encode(a[1], opt("--fps", 30, int))
        sys.exit(0)
    shot = json.load(open(a[1], encoding="utf-8"))
    if a[0] == "plan":
        print("\n".join(plan(shot)))
    elif a[0] == "cfg":
        if "--write" in a:   # into the addon, for the next `cinema.ps1 build`
            out = os.path.join(HERE, "addon", "cfg", "cinema", shot["name"] + ".cfg")
            os.makedirs(os.path.dirname(out), exist_ok=True)
            with open(out, "w", encoding="utf-8", newline="\n") as f:
                f.write(chain_cfg(shot))
            print(f"wrote {out}")
        else:
            sys.stdout.write(chain_cfg(shot))
    elif a[0] == "render":
        render(shot, opt("--out", os.path.join(os.getcwd(), shot["name"])), opt("--from", 0, int), opt("--to", 10 ** 9, int),
               opt("--settle", 0.25, float), opt("--key", None), opt("--route", "keys"))
    else:
        sys.exit(__doc__)
