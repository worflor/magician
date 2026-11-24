"""The animatic: a scene's timing and framing as a quick video, with no game. Every frame of every shot is drawn from
the same data the render uses (film.py plan): the camera's view of a ground grid and the actors as coloured
pillars, shot names and cues burned in, transitions as the edit will make them.

    python -I film.py animatic <scene.toml> [--size 640x360]     -> films\\<scene>\\animatic.mp4 (+ edit files)

What it can show: where each actor is in frame, how the camera moves, how long every shot holds, where the cuts
and cues fall. What it can't: the map (walls, props: the take holds no geometry), animation, effects, and the exact
lens (drawn at the spectator's assumed 90 degree 4:3 field of view, about 106 degrees across 16:9).
"""
import math
import os
import subprocess

GRID = 100          # units between ground lines
REACH = 2400        # how far the grid is drawn round the camera
PILLAR = (90, 18)   # an actor: height, half width (units)
COLOURS = [(255, 120, 90), (90, 190, 255), (130, 230, 110), (250, 205, 70), (215, 130, 255), (80, 225, 205)]


def basis(pitch, yaw):
    p, y = math.radians(pitch), math.radians(yaw)
    f = (math.cos(p) * math.cos(y), math.cos(p) * math.sin(y), -math.sin(p))
    r = (math.sin(y), -math.cos(y), 0.0)
    u = (r[1] * f[2] - r[2] * f[1], r[2] * f[0] - r[0] * f[2], r[0] * f[1] - r[1] * f[0])
    return f, r, u


class View:
    def __init__(self, w, h, pos, pitch, yaw, zoom=1.0):
        self.w, self.h = w, h
        self.pos = pos
        self.f, self.r, self.u = basis(pitch, yaw)
        self.focal = (h / 2) / math.tan(math.radians(73.74 / 2)) * zoom   # 90 degrees across 4:3 = 73.74 vertical

    def cam(self, q):
        d = (q[0] - self.pos[0], q[1] - self.pos[1], q[2] - self.pos[2])
        return (sum(a * b for a, b in zip(d, self.r)), sum(a * b for a, b in zip(d, self.u)),
                sum(a * b for a, b in zip(d, self.f)))

    def screen(self, c):
        return (self.w / 2 + c[0] * self.focal / c[2], self.h / 2 - c[1] * self.focal / c[2])


class Canvas:
    def __init__(self, w, h):
        self.w, self.h = w, h
        self.px = bytearray(w * h * 3)

    def fill(self, rgb):
        self.px[:] = bytes(rgb) * (self.w * self.h)

    def dot(self, x, y, rgb):
        if 0 <= x < self.w and 0 <= y < self.h:
            o = (y * self.w + x) * 3
            self.px[o:o + 3] = bytes(rgb)

    def line(self, a, b, rgb):
        (x0, y0), (x1, y1) = a, b
        n = int(max(abs(x1 - x0), abs(y1 - y0), 1))
        if n > 4 * (self.w + self.h):
            return
        for k in range(n + 1):
            self.dot(int(round(x0 + (x1 - x0) * k / n)), int(round(y0 + (y1 - y0) * k / n)), rgb)

    def rect(self, x0, y0, x1, y1, rgb):
        for y in range(max(0, int(y0)), min(self.h, int(y1) + 1)):
            o = (y * self.w + max(0, int(x0))) * 3
            n = min(self.w, int(x1) + 1) - max(0, int(x0))
            if n > 0:
                self.px[o:o + 3 * n] = bytes(rgb) * n


def segment(view, canvas, p, q, rgb, near=8.0):
    a, b = view.cam(p), view.cam(q)
    if a[2] < near and b[2] < near:
        return
    if a[2] < near or b[2] < near:   # clip to the near plane
        s = (near - a[2]) / (b[2] - a[2])
        m = tuple(a[i] + (b[i] - a[i]) * s for i in range(3))
        a, b = (m, b) if a[2] < near else (a, m)
    canvas.line(view.screen(a), view.screen(b), rgb)


def draw(view, canvas, actors, ground):
    canvas.fill((22, 24, 30))
    # the horizon: where the ground plane meets the sky at infinity
    fx, fy = view.f[0], view.f[1]
    if abs(fx) + abs(fy) > 1e-6:
        far = (view.pos[0] + fx * 1e6, view.pos[1] + fy * 1e6, view.pos[2])
        c = view.cam(far)
        if c[2] > 0:
            hy = view.screen(c)[1]
            canvas.rect(0, 0, canvas.w - 1, min(canvas.h - 1, hy), (34, 38, 52))
    cx, cy = view.pos[0], view.pos[1]
    x0, y0 = int((cx - REACH) // GRID) * GRID, int((cy - REACH) // GRID) * GRID
    for k in range(0, 2 * REACH // GRID + 2):
        g = (66, 72, 84) if (x0 + k * GRID) % 500 else (110, 118, 134)
        segment(view, canvas, (x0 + k * GRID, y0, ground), (x0 + k * GRID, y0 + 2 * REACH, ground), g)
        g = (66, 72, 84) if (y0 + k * GRID) % 500 else (110, 118, 134)
        segment(view, canvas, (x0, y0 + k * GRID, ground), (x0 + 2 * REACH, y0 + k * GRID, ground), g)
    # actors far to near, as pillars with a head
    order = sorted(actors, key=lambda a: -view.cam(a[1])[2])
    for rgb, feet in order:
        top = (feet[0], feet[1], feet[2] + PILLAR[0])
        neck = (feet[0], feet[1], feet[2] + PILLAR[0] * 0.8)
        c0, c1, c2 = view.cam(feet), view.cam(neck), view.cam(top)
        if min(c0[2], c1[2], c2[2]) < 10:
            continue
        (sx0, sy0), (sx1, sy1), (sx2, sy2) = view.screen(c0), view.screen(c1), view.screen(c2)
        half = PILLAR[1] * view.focal / c0[2]
        canvas.rect(sx0 - half, sy1, sx0 + half, sy0, rgb)                                   # body: feet to neck
        canvas.rect(sx2 - half * 0.7, sy2, sx2 + half * 0.7, sy1, tuple(min(255, v + 50) for v in rgb))   # head


# a 3x5 pixel font for labels over the actors and props (capitals, digits, a few marks)
FONT = {
    "A": "010101111101101", "B": "110101110101110", "C": "011100100100011", "D": "110101101101110",
    "E": "111100110100111", "F": "111100110100100", "G": "011100101101011", "H": "101101111101101",
    "I": "111010010010111", "J": "001001001101010", "K": "101101110101101", "L": "100100100100111",
    "M": "101111111101101", "N": "110101101101101", "O": "010101101101010", "P": "110101110100100",
    "Q": "010101101110011", "R": "110101110101101", "S": "011100010001110", "T": "111010010010010",
    "U": "101101101101111", "V": "101101101101010", "W": "101101111111101", "X": "101101010101101",
    "Y": "101101010010010", "Z": "111001010100111", "0": "111101101101111", "1": "010110010010111",
    "2": "110001010100111", "3": "110001010001110", "4": "101101111001001", "5": "111100110001110",
    "6": "011100111101111", "7": "111001010010010", "8": "111101111101111", "9": "111101111001110",
    "_": "000000000000111", "-": "000000111000000", " ": "000000000000000", "#": "101111101111101",
}


def label(canvas, x, y, text, rgb, scale=2):
    """Text centred on (x, y) in the 3x5 font."""
    text = str(text).upper()
    w = len(text) * 4 * scale
    x0, y0 = int(x - w / 2), int(y - 5 * scale / 2)
    for i, ch in enumerate(text):
        bits = FONT.get(ch, FONT[" "])
        for r in range(5):
            for c in range(3):
                if bits[r * 3 + c] == "1":
                    canvas.rect(x0 + (i * 4 + c) * scale, y0 + r * scale, x0 + (i * 4 + c) * scale + scale - 1,
                                y0 + r * scale + scale - 1, rgb)


def draw_set(view, canvas, blocking, tick):
    """The [blocking] set dressing: walls as wireframe ([[blocking.set]] poly, z, h) and props as boxes
    ([[blocking.props]] name, pos, size, from, until in seconds of the planned take), labelled."""
    t = tick / 64.0
    for st in blocking.get("set", []):
        poly, z, h = st.get("poly", []), float(st.get("z", 0)), float(st.get("h", 0))
        rgb = tuple(st.get("rgb", (120, 104, 150)))
        for i in range(len(poly)):
            a, b = poly[i], poly[(i + 1) % len(poly)]
            segment(view, canvas, (a[0], a[1], z), (b[0], b[1], z), rgb)
            if h:
                segment(view, canvas, (a[0], a[1], z + h), (b[0], b[1], z + h), rgb)
                segment(view, canvas, (a[0], a[1], z), (a[0], a[1], z + h), rgb)
    for pr in blocking.get("props", []):
        if not (float(pr.get("from", -1e9)) <= t <= float(pr.get("until", 1e9))):
            continue
        x, y, z = pr["pos"]
        w, d, h = pr.get("size", [30, 30, 40])
        rgb = tuple(pr.get("rgb", (200, 200, 210)))
        c = [(x + sx * w / 2, y + sy * d / 2, z + sz * h) for sz in (0, 1) for sy in (-1, 1) for sx in (-1, 1)]
        for i, j in ((0, 1), (1, 3), (3, 2), (2, 0), (4, 5), (5, 7), (7, 6), (6, 4), (0, 4), (1, 5), (2, 6), (3, 7)):
            segment(view, canvas, c[i], c[j], rgb)
        top = view.cam((x, y, z + h + 12))
        if top[2] > 20 and pr.get("name"):
            sx, sy = view.screen(top)
            label(canvas, sx, sy, pr["name"], rgb, 1)


def render(scene, out, size=(640, 360), srt=None):
    """The animatic of a scene into `out` (mp4). Shots are joined with the scene's cuts by the encoder's filter."""
    w, h = size
    colours = {}
    keys = sorted({s.take_key for s in scene.shots})
    for k in keys:
        for n in scene.tracks_for(k).names():
            colours.setdefault(n, COLOURS[len(colours) % len(COLOURS)])
    tmp = []
    blocking = scene.d.get("blocking", {})
    for shot in scene.all_shots():
        if shot.card is not None:   # drawn by the encoder (film.card_input)
            tmp.append(None)
            continue
        path = out + f".{shot.index:02d}.mkv"
        tmp.append(path)
        p = subprocess.Popen(["ffmpeg", "-y", "-hide_banner", "-loglevel", "error", "-f", "rawvideo", "-pix_fmt", "rgb24",
                              "-s", f"{w}x{h}", "-r", str(scene.fps), "-i", "-", "-c:v", "ffv1", path], stdin=subprocess.PIPE)
        canvas = Canvas(w, h)
        for f in scene.frames(shot):
            view = View(w, h, f.pos, f.pitch, f.yaw, f.zoom)
            actors = []
            tracks = shot.tracks
            for n in tracks.names():
                tr = tracks.actors[n]
                a, b = tr.span()
                if a <= f.tick <= b:
                    actors.append((colours[n], tr.at(f.tick)))
            ground = float(blocking.get("floor", min([a[1][2] for a in actors] + [0.0])))
            draw(view, canvas, actors, ground)
            if scene.planned(shot.take_key):
                draw_set(view, canvas, blocking, f.tick)
            for n, (rgb, feet) in zip([n for n in tracks.names() if tracks.actors[n].span()[0] <= f.tick
                                       <= tracks.actors[n].span()[1]], actors):
                c = view.cam((feet[0], feet[1], feet[2] + PILLAR[0] + 14))
                if c[2] > 20:
                    sx, sy = view.screen(c)
                    label(canvas, sx, sy, n, rgb, 2)
            p.stdin.write(bytes(canvas.px))
        p.stdin.close()
        if p.wait() != 0:
            raise RuntimeError(f"ffmpeg failed on the animatic of {shot.name}")
    return tmp, colours
