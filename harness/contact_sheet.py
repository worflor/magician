"""Magician contact sheet: tile numbered frames into one image.

  python contact_sheet.py <out.png> <frame_00.png> <frame_01.png> ... [--cols 4] [--label "text"]

Needs Pillow (system Python).
"""
import sys
from PIL import Image, ImageDraw

args = sys.argv[1:]
cols, label = 4, ""
if "--cols" in args:
    i = args.index("--cols"); cols = int(args[i + 1]); del args[i:i + 2]
if "--label" in args:
    i = args.index("--label"); label = args[i + 1]; del args[i:i + 2]
out, frames = args[0], [Image.open(p).convert("RGB") for p in args[1:]]
w, h = frames[0].size
rows = (len(frames) + cols - 1) // cols
top = 28 if label else 0
sheet = Image.new("RGB", (cols * w, rows * h + top), (30, 30, 34))
d = ImageDraw.Draw(sheet)
if label:
    d.text((8, 7), label, fill=(230, 230, 230))
for k, f in enumerate(frames):
    x, y = (k % cols) * w, (k // cols) * h + top
    sheet.paste(f, (x, y))
    d.text((x + 8, y + 6), str(k + 1), fill=(255, 220, 120))
sheet.save(out)
print("SHEET", out, len(frames), "frames")
