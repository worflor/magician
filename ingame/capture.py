"""Capture the Deadlock window from outside the game (retail has no screenshot or movie commands).

    python -I capture.py shot  <out.png>                      # one frame
    python -I capture.py burst <out_dir> --frames 24 --fps 12  # a sequence, for motion review (half res; --full)
    python -I capture.py sheet <in_dir> <out.png> --cols 6     # contact sheet of a burst
                               [--prefix TAG_]                 # only the shots named TAG_<n>.png (a turntable)
    python -I capture.py crop  <in.png> <out.png> x0 y0 x1 y1 [--zoom 2]

On Windows it reads the window's own client area through PrintWindow, so a window on top of the game does not get
into the frame; when PrintWindow fails it copies the area off the desktop instead. On X11 it reads the window with
XGetImage (gameos.py). Works windowed and borderless; exclusive fullscreen can come back black, so launch.py starts the
game borderless. Standard library only: ctypes for the system's own capture, zlib for PNG. `sheet` and `crop` read
and write files only, and run anywhere.
"""
import os
import struct
import sys
import time
import zlib

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import gameos  # noqa: E402


def find_window(title_part="Deadlock"):
    """The game's window (on Windows, class SDL_app; under Proton, class steam_app_1422450), or None."""
    return gameos.need_display().find_window(title_part)


def client_rect(hwnd):
    return gameos.need_display().client_rect(hwnd)


def grab(x, y, w, h, hwnd=None):
    """BGRA rows, top-down: the window's own content with hwnd, else a copy of the screen region."""
    return gameos.need_display().grab(x, y, w, h, hwnd)


def write_png(path, w, h, bgra):
    rows = bytearray()
    for y in range(h):
        row = bgra[y * w * 4:(y + 1) * w * 4]
        rgb = bytearray(w * 3)
        rgb[0::3], rgb[1::3], rgb[2::3] = row[2::4], row[1::4], row[0::4]
        rows += b"\x00" + rgb

    def chunk(t, d):
        return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)

    os.makedirs(os.path.dirname(os.path.abspath(path)), exist_ok=True)
    with open(path, "wb") as f:
        f.write(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
                + chunk(b"IDAT", zlib.compress(bytes(rows), 6)) + chunk(b"IEND", b""))


def read_png_rgb(path):
    """Minimal reader for the PNGs this tool writes (8-bit RGB, filter 0 rows)."""
    with open(path, "rb") as f:
        data = f.read()
    pos, idat, w, h = 8, b"", 0, 0
    while pos < len(data):
        n = struct.unpack(">I", data[pos:pos + 4])[0]
        t = data[pos + 4:pos + 8]
        d = data[pos + 8:pos + 8 + n]
        if t == b"IHDR":
            w, h = struct.unpack(">II", d[:8])
        elif t == b"IDAT":
            idat += d
        pos += 12 + n
    raw = zlib.decompress(idat)
    stride = w * 3 + 1
    return w, h, [raw[y * stride + 1:(y + 1) * stride] for y in range(h)]


def window_or_die():
    hwnd = find_window()
    if not hwnd:
        sys.exit("capture: no Deadlock game window is open")
    return (*client_rect(hwnd), None if "--screen" in sys.argv else hwnd)


def shot(out):
    x, y, w, h, hwnd = window_or_die()
    write_png(out, w, h, grab(x, y, w, h, hwnd))
    print(f"CAPTURE {out} {w}x{h}")


def burst(out_dir, frames, fps):
    x, y, w, h, hwnd = window_or_die()
    os.makedirs(out_dir, exist_ok=True)
    grabbed, t0 = [], time.perf_counter()
    for i in range(frames):   # grab everything first, encode after: encoding would wreck the frame timing
        target = t0 + i / fps
        while time.perf_counter() < target:
            time.sleep(0.001)
        grabbed.append((time.perf_counter() - t0, grab(x, y, w, h, hwnd)))
    step = 1 if "--full" in sys.argv else 2   # bursts feed contact sheets: half resolution is 4x less disk (D: is tight)
    ow, oh = w // step, h // step
    for i, (t, px) in enumerate(grabbed):
        if step > 1:
            rows = bytearray()
            for y in range(0, oh * step, step):
                row = px[y * w * 4:(y + 1) * w * 4]
                line = bytearray(ow * 4)
                for c in range(4):
                    line[c::4] = row[c::4 * step][:ow]
                rows += line
            px = bytes(rows)
        write_png(os.path.join(out_dir, f"f{i:03d}.png"), ow, oh, px)
    print(f"BURST {out_dir} {frames} frames {ow}x{oh}, {grabbed[-1][0]:.2f}s")


def sheet(in_dir, out, cols, scale, prefix=""):
    files = sorted(f for f in os.listdir(in_dir) if f.endswith(".png") and f.startswith(prefix) and not f.endswith("_sheet.png") and (not prefix or f[len(prefix):-4].isdigit()))
    if not files:
        sys.exit(f"capture: no {prefix}*.png frames in {in_dir}")
    imgs = [read_png_rgb(os.path.join(in_dir, f)) for f in files]
    scale = scale or max(1, imgs[0][0] // 480)   # default: ~480 px cells whatever the frame size
    w, h = imgs[0][0] // scale, imgs[0][1] // scale
    rows_n = (len(imgs) + cols - 1) // cols
    W, H = w * cols, h * rows_n
    canvas = [bytearray(W * 3) for _ in range(H)]
    for k, (iw, ih, rows) in enumerate(imgs):
        ox, oy = (k % cols) * w, (k // cols) * h
        # frames may differ in size (crops from different shots): sample each into the cell by its own scale
        sx, sy = iw / w, ih / h
        for yy in range(h):
            src = rows[min(ih - 1, int(yy * sy))]
            line = canvas[oy + yy]
            for xx in range(w):
                s = min(iw - 1, int(xx * sx)) * 3
                line[(ox + xx) * 3:(ox + xx) * 3 + 3] = src[s:s + 3]
    bgra = bytearray()
    for line in canvas:
        px = bytearray(W * 4)
        px[0::4], px[1::4], px[2::4] = line[2::3], line[1::3], line[0::3]
        bgra += px
    write_png(out, W, H, bytes(bgra))
    print(f"SHEET {out} {len(imgs)} frames")


if __name__ == "__main__":
    a = sys.argv[1:]
    if a[:1] and a[0] in ("--help", "-h", "help"):
        print(__doc__)
        sys.exit(0)

    def opt(name, default):
        return int(a[a.index(name) + 1]) if name in a else default

    if not a:
        sys.exit(__doc__)
    if a[0] == "shot":   # shot <out.png> or shot --out <out.png> (the flag used to be taken as the file name)
        rest = [v for v in a[1:] if v != "--screen"]
        if "--out" in rest:
            rest = rest[rest.index("--out") + 1:]
        if not rest or rest[0].startswith("-"):
            sys.exit("capture: shot needs an output path: capture.py shot <out.png> (or --out <out.png>)")
        shot(rest[0])
    elif a[0] == "burst":
        burst(a[1], opt("--frames", 24), opt("--fps", 12))
    elif a[0] == "crop":   # crop <in.png> <out.png> x0 y0 x1 y1 [--zoom 2]
        w, h, rows = read_png_rgb(a[1])
        x0, y0, x1, y1 = (int(v) for v in a[3:7])
        z = opt("--zoom", 2)
        out_rows = []
        for y in range(y0, y1):
            src = rows[y][x0 * 3:x1 * 3]
            line = bytearray()
            for x in range(0, len(src), 3):
                line += src[x:x + 3] * z
            out_rows += [line] * z
        W, H = (x1 - x0) * z, (y1 - y0) * z
        bgra = bytearray()
        for line in out_rows:
            px = bytearray(W * 4)
            px[0::4], px[1::4], px[2::4] = line[2::3], line[1::3], line[0::3]
            bgra += px
        write_png(a[2], W, H, bytes(bgra))
        print(f"CROP {a[2]} {W}x{H}")
    elif a[0] == "sheet":
        sheet(a[1], a[2], opt("--cols", 6), opt("--scale", 0), a[a.index("--prefix") + 1] if "--prefix" in a else "")
    else:
        sys.exit(__doc__)
