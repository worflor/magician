"""Magician squint test: what a player actually sees mid-match.

  python squint_test.py <render.png> <out.png> [--height 150] [--blur 0.8] [--zoom 4]

Crops the render to the figure (anything that differs from the corner background), shrinks it until the
figure is --height pixels tall, blurs it slightly, and writes one sheet: colour | greyscale value study |
3-level posterised values, each enlarged --zoom times with hard pixels so the read is honest.
The eye should land on the intended focal points in order; anything else that grabs it has to be calmed.
Also prints the brightest and the most saturated regions as a rough ranking (rows from the top, 0..1).
Needs Pillow and numpy (system Python, not Blender's).
"""
import sys
import numpy as np
from PIL import Image, ImageFilter

args = sys.argv[1:]
src, out = args[0], args[1]
def opt(name, default):
    return float(args[args.index(name) + 1]) if name in args else default
height, blur, zoom = int(opt("--height", 150)), opt("--blur", 0.8), int(opt("--zoom", 4))

im = Image.open(src).convert("RGB")
a = np.asarray(im).astype(np.int16)
bg = a[4, 4]
mask = (np.abs(a - bg).sum(axis=2) > 24)
ys, xs = np.where(mask)
y0, y1, x0, x1 = ys.min(), ys.max(), xs.min(), xs.max()
pad = int((y1 - y0) * 0.06)
box = (max(0, x0 - pad), max(0, y0 - pad), min(im.width, x1 + pad), min(im.height, y1 + pad))
fig = im.crop(box)
scale = height / (y1 - y0)
small = fig.resize((max(1, round(fig.width * scale)), max(1, round(fig.height * scale))), Image.LANCZOS)
small = small.filter(ImageFilter.GaussianBlur(blur))
grey = small.convert("L")
g = np.asarray(grey).astype(np.float32) / 255
levels = np.digitize(g, [0.30, 0.62]).astype(np.uint8) * 110 + 20        # dark / mid / light
post = Image.fromarray(levels, "L")

def big(i):
    return i.resize((i.width * zoom, i.height * zoom), Image.NEAREST).convert("RGB")
tiles = [big(small), big(grey), big(post)]
sheet = Image.new("RGB", (sum(t.width for t in tiles) + 16 * (len(tiles) + 1), tiles[0].height + 32), (30, 30, 34))
x = 16
for t in tiles:
    sheet.paste(t, (x, 16))
    x += t.width + 16
sheet.save(out)

# a crude ranking of what pulls the eye: local contrast against the surround, by horizontal band
s = np.asarray(small.convert("HSV")).astype(np.float32) / 255
bands = 10
print("SQUINT", out, "figure", small.size)
for b in range(bands):
    r0, r1 = b * g.shape[0] // bands, (b + 1) * g.shape[0] // bands
    gb, sb = g[r0:r1], (s[r0:r1, :, 1] * s[r0:r1, :, 2])
    print(f"  band {b}: light area {float((gb > 0.62).mean()):.2f}  colour {float((sb > 0.45).mean()):.2f}  contrast {float(gb.std()):.2f}")
