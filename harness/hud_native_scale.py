"""Magician HUD native-scale test: ability icons the way a player sees them on the in-match bar.

  python hud_native_scale.py --out sheet.png --ability "Name|valve.png|ours.png" [--ability ...] (one per slot,
         in bar order) [--title TEXT] [--frames DIR] [--json metrics.json]

Every icon is drawn into Valve's real in-match ability button, built from the game's own layout and styles
(build 6861: hud_abilities_entry.vxml + hud_abilities.vcss + hud.vcss), never from a guess:
  - size: .button_container 120 px, x #abilities ui-scale 90 % x #hud_signature ui-scale 90 %  -> 97 px
    at 1080p, icon .ability_image height 50 % -> 49 px. 1440p is x 4/3 (Panorama scales by screen height).
  - ready: ability_frame_0.vsvg washed keybindBGColor #e8e4ea, ability_static_frame_border.vsvg ring washed
    keybindBGColor at 0.8, icon washed offBlack #10130D at 0.95 (the bar washes every icon to one colour,
    so only the silhouette reaches the player).
  - cooldown: frame at 0.05, ring washed black at 0.5, the cooldown mask offBlack&ee at 0.9 cut to the frame,
    icon washed offBlack at 0.6, the timer on top. (In this state the stock bar hides the icon almost
    completely and the timer carries the read; the sheet shows that, so a skin can't be blamed for it.)
  - squint: the ready button in greyscale with a slight blur (sigma = button / 40).
  - glance: the in-combat read: the 1080p button over a busy world, shrunk to 2/3 and softened, as seen
    from the corner of the eye mid-fight.
Each ability gets Valve's icon beside ours; the bar rows put all slots side by side (Valve's bar, ours,
and ours with any slot we left as Valve's), so it shows whether each icon stays distinct from its neighbours.

Metrics (printed, drawn on the sheet, optionally written as JSON), all on the 1080p icon:
  - edges: mean gradient of the washed silhouette (higher = busier; detail that may merge at HUD size)
  - same: similarity of ours to Valve's for the same slot (squinted silhouettes, 0..1): keeps the gesture
  - nearest: highest similarity to any OTHER slot on the same bar (lower = more distinct from neighbours)
The frames come from the live game: if --frames is missing they are decompiled once with Source 2 Viewer
into <reference>\\work\\hud-harness\\valve (reference only, never committed).
Needs Pillow, numpy, OpenCV and Chrome (to rasterise Valve's SVG frames).
"""
import sys, os, json, math, pathlib, shutil, subprocess, tempfile, argparse
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "ingame"))
import gamepaths  # noqa: E402  (locations as magician.exe resolves them)
import numpy as np
import cv2
from PIL import Image, ImageDraw, ImageFont

VRF = gamepaths.vrf()
GAME = os.path.join(gamepaths.try_game(), "citadel", "pak01_dir.vpk")
CHROME = r"C:\Program Files\Google\Chrome\Application\chrome.exe"
DEFAULT_FRAMES = gamepaths.work("hud-harness", "valve")

KEYBIND = np.array([0xE8, 0xE4, 0xEA]) / 255
OFFBLACK = np.array([0x10, 0x13, 0x0D]) / 255
OFFWHITE = np.array([0xFF, 0xEF, 0xD7]) / 255
BUTTON_1080 = 120 * 0.9 * 0.9          # 97.2 px
RES = {"1080p": 1.0, "1440p": 4 / 3}

BG = (20, 22, 30)
TEXT = (226, 220, 206)
DIM = (140, 138, 150)
AMBER = (232, 170, 70)


def font(size):
    for f in ("CascadiaMono.ttf", "consola.ttf"):
        try:
            return ImageFont.truetype(os.path.join(r"C:\Windows\Fonts", f), size)
        except OSError:
            pass
    return ImageFont.load_default()


def rasterise(svg, px, cache):
    """Valve's SVG frame as an alpha mask at px. Cached in memory and on disk beside the frames (Chrome can
    take a minute to start on a busy machine, and the frames only change with the game build)."""
    key = (svg, px)
    if key in cache:
        return cache[key]
    disk = os.path.join(os.path.dirname(svg), "_raster", f"{os.path.splitext(os.path.basename(svg))[0]}_{px}.png")
    if os.path.exists(disk) and os.path.getmtime(disk) >= os.path.getmtime(svg):
        a = np.asarray(Image.open(disk).convert("RGBA")).astype(np.float32)[..., 3] / 255
        cache[key] = a
        return a
    out = os.path.join(tempfile.gettempdir(), f"hudns_{abs(hash(key)) % 10**8}.png")
    html = (f'<html><body style="margin:0;background:transparent"><img src="{pathlib.Path(svg).as_uri()}" '
            f'style="width:{px}px;height:{px}px;display:block"></body></html>')
    fd, hp = tempfile.mkstemp(suffix=".html")
    os.write(fd, html.encode())
    os.close(fd)
    subprocess.run([CHROME, "--headless=new", "--disable-gpu", "--hide-scrollbars", "--no-first-run",
                        "--user-data-dir=" + os.path.join(tempfile.gettempdir(), "magician_chrome_profile"), "--default-background-color=00000000",
                    f"--window-size={px},{px}", f"--screenshot={out}", pathlib.Path(hp).as_uri()],
                   capture_output=True, timeout=600, check=True)
    os.remove(hp)
    a = np.asarray(Image.open(out).convert("RGBA")).astype(np.float32)[..., 3] / 255
    os.makedirs(os.path.dirname(disk), exist_ok=True)
    shutil.move(out, disk)
    cache[key] = a
    return a


def ensure_frames(d):
    core = os.path.join(d, "panorama", "images", "hud", "core")
    if not os.path.exists(os.path.join(core, "ability_frame_0.svg")):
        os.makedirs(d, exist_ok=True)
        subprocess.run([VRF, "-i", GAME, "-f", "panorama/images/hud/core/ability_", "-d", "-o", d], capture_output=True)
    return core


def area_resize(a, size):
    return cv2.resize(a, (size, size), interpolation=cv2.INTER_AREA)


def icon_alpha(path, px, ss):
    """The icon's alpha at px (supersampled), the only thing the wash keeps."""
    im = np.asarray(Image.open(path).convert("RGBA")).astype(np.float32)[..., 3] / 255
    return area_resize(cv2.resize(im, (px * ss, px * ss), interpolation=cv2.INTER_AREA), px)


def over(dst, rgb, alpha):
    a = alpha[..., None]
    return dst * (1 - a) + np.asarray(rgb)[None, None, :] * a


def button(icon_path, scale, state, frames, cache, world=None):
    """One ability button at a resolution scale, RGB float, on the given world backdrop."""
    B = int(round(BUTTON_1080 * scale))
    ss = 4
    frame = area_resize(rasterise(os.path.join(frames, "ability_frame_0.svg"), B * ss, cache), B)
    ring = area_resize(rasterise(os.path.join(frames, "ability_static_frame_border.svg"), B * ss, cache), B)
    mask = area_resize(rasterise(os.path.join(frames, "ability_frame_1.svg"), B * ss, cache), B)
    I = int(round(B * 0.5))
    ia = np.zeros((B, B), np.float32)
    o = (B - I) // 2
    ia[o:o + I, o:o + I] = icon_alpha(icon_path, I, ss)
    img = np.ones((B, B, 3), np.float32) * (np.array(world, np.float32) / 255 if world is not None else np.array(BG) / 255)
    if state == "ready":
        img = over(img, KEYBIND, frame)
        img = over(img, KEYBIND, ring * 0.8)
        img = over(img, OFFBLACK, ia * 0.95)
    else:
        img = over(img, KEYBIND, frame * 0.05)
        img = over(img, (0, 0, 0), ring * 0.5)
        img = over(img, OFFBLACK, mask * (0xEE / 255) * 0.9)
        img = over(img, OFFBLACK, ia * 0.6)
        # the timer (.cooldown_timer: 30 px, offWhite, dark glow), the cue a player reads in this state
        t = Image.new("L", (B, B), 0)
        f = font(int(round(30 * 0.81 * scale)))
        ImageDraw.Draw(t).text((B / 2, B / 2), "12", font=f, fill=255, anchor="mm")
        ta = np.asarray(t).astype(np.float32) / 255
        img = over(img, (0, 0, 0), np.clip(cv2.GaussianBlur(ta, (0, 0), 2.0) * 2, 0, 1) * 0.8)
        img = over(img, OFFWHITE, ta)
    return np.clip(img, 0, 1), ia


def busy_world(h, w, seed=3):
    """A stand-in for the match behind the bar: mid-value, colourful, high-frequency."""
    rng = np.random.default_rng(seed)
    n = cv2.GaussianBlur(rng.random((h, w, 3)).astype(np.float32), (0, 0), 3)
    n = (n - n.mean()) * 4 + 0.42
    stripes = 0.08 * np.sin(np.arange(w)[None, :, None] / 5.0)
    return np.clip(n + stripes, 0, 1) * 255


def squint(rgb, B):
    g = rgb @ np.array([0.2126, 0.7152, 0.0722], np.float32)
    g = cv2.GaussianBlur(g, (0, 0), max(0.6, B / 40))
    return np.repeat(g[..., None], 3, -1)


def glance(icon_path, frames, cache):
    B = int(round(BUTTON_1080))
    world = busy_world(B, B)
    rgb, _ = button(icon_path, 1.0, "ready", frames, cache, world=world.mean((0, 1)))
    # the world shows round the frame
    fr = area_resize(rasterise(os.path.join(frames, "ability_frame_0.svg"), B * 4, cache), B)[..., None]
    a = rgb * fr + (world / 255) * (1 - fr)
    small = cv2.resize(a, (B * 2 // 3, B * 2 // 3), interpolation=cv2.INTER_AREA)
    return np.clip(cv2.GaussianBlur(small, (0, 0), 0.7), 0, 1)


def sim(a, b):
    """Similarity of two silhouettes after a squint blur: normalised cross-correlation, 0..1."""
    s = max(0.8, a.shape[0] / 25)
    a = cv2.GaussianBlur(a, (0, 0), s).ravel()
    b = cv2.GaussianBlur(b, (0, 0), s).ravel()
    a, b = a - a.mean(), b - b.mean()
    return float(max(0.0, (a @ b) / (np.linalg.norm(a) * np.linalg.norm(b) + 1e-9)))


def edges(a):
    gx, gy = cv2.Sobel(a, cv2.CV_32F, 1, 0), cv2.Sobel(a, cv2.CV_32F, 0, 1)
    return float(np.sqrt(gx * gx + gy * gy).mean())


def to_pil(a):
    return Image.fromarray((np.clip(a, 0, 1) * 255 + 0.5).astype(np.uint8), "RGB")


def pixels(im, k):
    return im.resize((im.width * k, im.height * k), Image.NEAREST)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", required=True)
    ap.add_argument("--ability", action="append", required=True, help="Name|valve.png|ours.png")
    ap.add_argument("--title", default="HUD native scale")
    ap.add_argument("--frames", default=DEFAULT_FRAMES)
    ap.add_argument("--json")
    args = ap.parse_args()
    frames = ensure_frames(args.frames)
    cache = {}
    slots = [s.split("|") for s in args.ability]

    # metrics on the 1080p icon silhouettes
    I = int(round(BUTTON_1080 * 0.5))
    valve_a = [icon_alpha(v, I, 4) for _, v, _ in slots]
    ours_a = [icon_alpha(o, I, 4) for _, _, o in slots]
    metrics = []
    for i, (name, v, o) in enumerate(slots):
        changed = os.path.abspath(v) != os.path.abspath(o)
        m = {"ability": name, "changed": changed,
             "edges_valve": round(edges(valve_a[i]), 4), "edges_ours": round(edges(ours_a[i]), 4),
             "same": round(sim(valve_a[i], ours_a[i]), 3),
             "nearest_valve": round(max(sim(valve_a[i], valve_a[j]) for j in range(len(slots)) if j != i), 3),
             "nearest_ours": round(max(sim(ours_a[i], ours_a[j]) for j in range(len(slots)) if j != i), 3)}
        metrics.append(m)
        print(json.dumps(m))
    if args.json:
        os.makedirs(os.path.dirname(os.path.abspath(args.json)), exist_ok=True)
        with open(args.json, "w", encoding="utf-8") as f:
            json.dump({"icon_px_1080p": I, "button_px_1080p": round(BUTTON_1080, 1), "slots": metrics}, f, indent=1)

    W = 2400
    canvas = Image.new("RGB", (W, 4000), BG)
    d = ImageDraw.Draw(canvas)
    F1, F2, F3 = font(28), font(18), font(14)
    y = 24
    d.text((30, y), args.title, font=F1, fill=AMBER)
    d.text((30, y + 38), f"Valve's real in-match button: {BUTTON_1080:.0f} px at 1080p (icon {I} px), "
           f"{BUTTON_1080 * 4 / 3:.0f} px at 1440p (icon {round(I * 4 / 3)} px). Pixels shown 1:1, then x3 nearest "
           "neighbour so nothing is smoothed for you.", font=F3, fill=DIM)
    y += 80

    # ---- per ability rows ----
    cols = ["ready 1080p", "cooldown 1080p", "ready 1440p", "squint 1080p", "glance", "1080p pixels x3"]
    for i, (name, v, o) in enumerate(slots):
        m = metrics[i]
        d.rectangle([20, y, W - 20, y + 2], fill=AMBER)
        d.text((30, y + 8), name + ("" if m["changed"] else "   (unchanged: Valve's icon kept)"), font=F2,
               fill=TEXT if m["changed"] else DIM)
        d.text((760, y + 10), f"edges valve {m['edges_valve']:.3f} / ours {m['edges_ours']:.3f}    "
               f"same-as-valve {m['same']:.2f}    nearest neighbour valve {m['nearest_valve']:.2f} / ours {m['nearest_ours']:.2f}",
               font=F3, fill=DIM)
        y += 40
        x = 30
        for c in cols:
            d.text((x, y), c, font=F3, fill=DIM)
            x += 390 if c == "1080p pixels x3" else 300
        y += 22
        x = 30
        tiles = []
        for who, p in (("valve", v), ("ours", o)):
            r1, _ = button(p, 1.0, "ready", frames, cache)
            c1, _ = button(p, 1.0, "cooldown", frames, cache)
            r2, _ = button(p, 4 / 3, "ready", frames, cache)
            sq = squint(r1, r1.shape[0])
            gl = glance(p, frames, cache)
            tiles.append((r1, c1, r2, sq, gl))
        for k, imgs in enumerate(zip(*tiles)):
            for j, a in enumerate(imgs):
                im = to_pil(a)
                canvas.paste(im, (x + j * (im.width + 8), y))
            x += 300
        pv, po = to_pil(tiles[0][0]), to_pil(tiles[1][0])
        canvas.paste(pixels(pv, 3), (x, y))
        canvas.paste(pixels(po, 3), (x + pv.width * 3 + 10, y))
        d.text((x, y + pv.height * 3 + 4), "valve", font=F3, fill=DIM)
        d.text((x + pv.width * 3 + 10, y + pv.height * 3 + 4), "ours", font=F3, fill=AMBER)
        d.text((30, y + 135), "valve | ours in each pair", font=F3, fill=DIM)
        y += max(pv.height * 3 + 30, 160)

    # ---- the bar: neighbours side by side ----
    d.rectangle([20, y, W - 20, y + 2], fill=AMBER)
    d.text((30, y + 8), "THE BAR: every slot beside its neighbours (ready, then squint), 1080p and 1440p", font=F2, fill=TEXT)
    y += 44
    for res, sc in RES.items():
        for who, idx in (("valve", 1), ("ours", 2)):
            x = 30
            d.text((x, y + 30), f"{who} {res}", font=F3, fill=AMBER if who == "ours" else DIM)
            x += 130
            row = [button(s[idx], sc, "ready", frames, cache)[0] for s in slots]
            for a in row:
                canvas.paste(to_pil(a), (x, y))
                x += a.shape[1] - 6
            x += 40
            for a in row:
                canvas.paste(to_pil(squint(a, a.shape[0])), (x, y))
                x += a.shape[1] - 6
            x += 40
            for s in slots:
                g = glance(s[idx], frames, cache)
                canvas.paste(to_pil(g), (x, y + 10))
                x += g.shape[1] + 2
            y += int(BUTTON_1080 * sc) + 10
        y += 10
    canvas = canvas.crop((0, 0, W, y + 20))
    os.makedirs(os.path.dirname(os.path.abspath(args.out)), exist_ok=True)
    canvas.save(args.out, optimize=True)
    print("hud native scale", args.out, canvas.size)


if __name__ == "__main__":
    main()
