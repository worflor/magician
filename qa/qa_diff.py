"""QA pixel diffs: Valve's view against ours, same camera, same pixels (qa_render.py --mode views).

  python -I qa_diff.py <views dir>        (expects <dir>/valve/<pose>/<style>_<cam>.png and <dir>/ours/...)

For every image pair:
  silhouette  from the alpha: grey = both, orange = ours only, cyan = Valve's only. The share of Valve's silhouette
              each side adds or loses is the fair-play number (a model harder to see than Valve's is a bug).
  diff        |ours - Valve's| on the dark background, x3, so unchanged pixels are black.
  backface    for the facing style, the red pixels on each side: back faces the camera sees (holes in game).
Writes <dir>/diff/<pose>/<style>_<cam>.png (Valve | ours | silhouette | diff), a sheet per pose and camera
(<dir>/diff/sheet_<pose>_<cam>.png: one row per style) and <dir>/diff/diff.json.
"""
import sys, os, json, glob
import numpy as np
from PIL import Image, ImageDraw

root = sys.argv[1]
BG = np.array([40, 40, 46], float)
out = os.path.join(root, "diff")
os.makedirs(out, exist_ok=True)

def load(p):
    im = np.asarray(Image.open(p).convert("RGBA"), float)
    a = im[..., 3:4] / 255.0
    return im[..., :3] * a + BG * (1 - a), im[..., 3] > 8

def label(img, text):
    d = ImageDraw.Draw(img)
    d.rectangle([0, 0, img.width, 16], fill=(20, 20, 24))
    d.text((4, 2), text, fill=(235, 225, 200))
    return img

stats = {}
for vpath in sorted(glob.glob(os.path.join(root, "valve", "*", "*.png"))):
    pose = os.path.basename(os.path.dirname(vpath))
    name = os.path.basename(vpath)
    opath = os.path.join(root, "ours", pose, name)
    if not os.path.exists(opath):
        continue
    vr, va = load(vpath)
    orr, oa = load(opath)
    if vr.shape != orr.shape:
        continue
    sil = np.zeros(vr.shape, np.uint8) + BG.astype(np.uint8)
    sil[va & oa] = (150, 150, 155)
    sil[oa & ~va] = (255, 150, 30)
    sil[va & ~oa] = (40, 210, 255)
    diff = np.clip(np.abs(orr - vr) * 3, 0, 255).astype(np.uint8)
    nv = int(va.sum())
    st = {"valve_px": nv, "ours_px": int(oa.sum()), "ours_only_pct": round(100.0 * (oa & ~va).sum() / max(nv, 1), 2),
          "valve_only_pct": round(100.0 * (va & ~oa).sum() / max(nv, 1), 2),
          "mean_abs_diff": round(float(np.abs(orr - vr)[va | oa].mean()) if (va | oa).any() else 0.0, 2)}
    if name.startswith("facing"):
        # back faces the camera sees (pure red in the facing style): holes in game, where back faces are culled
        red = lambda im: int(((im[..., 0] > 150) & (im[..., 1] < 60) & (im[..., 2] < 60)).sum())
        st["backface_px_ours"], st["backface_px_valve"] = red(orr), red(vr)
    stats["%s/%s" % (pose, name[:-4])] = st
    tiles = [label(Image.fromarray(vr.astype(np.uint8)), "Valve"), label(Image.fromarray(orr.astype(np.uint8)), "ours"),
             label(Image.fromarray(sil), "silhouette +%.1f%% -%.1f%%" % (st["ours_only_pct"], st["valve_only_pct"])),
             label(Image.fromarray(diff), "|diff| x3")]
    w, h = tiles[0].size
    row = Image.new("RGB", (w * 4, h))
    for k, t in enumerate(tiles):
        row.paste(t, (k * w, 0))
    os.makedirs(os.path.join(out, pose), exist_ok=True)
    row.save(os.path.join(out, pose, name))

# one sheet per pose and camera: a row per style, scaled to 1600 px wide
by = {}
for key in stats:
    pose, sc = key.split("/")
    style, cam = sc.rsplit("_", 1)
    by.setdefault((pose, cam), []).append(style)
order = ["lookdev", "xray", "wire", "facing", "normals", "uv", "matid", "texel", "motion", "overdraw"]
for (pose, cam), styles in by.items():
    styles = sorted(styles, key=lambda s: order.index(s) if s in order else 99)
    rows = [Image.open(os.path.join(out, pose, "%s_%s.png" % (s, cam))) for s in styles]
    sc_ = 1600 / rows[0].width
    rows = [r.resize((1600, int(r.height * sc_))) for r in rows]
    sheet = Image.new("RGB", (1600, sum(r.height for r in rows) + 24), (20, 20, 24))
    ImageDraw.Draw(sheet).text((6, 5), "%s, %s camera: %s" % (pose, cam, ", ".join(styles)), fill=(235, 225, 200))
    y = 24
    for r in rows:
        sheet.paste(r, (0, y)); y += r.height
    sheet.save(os.path.join(out, "sheet_%s_%s.png" % (pose, cam)))
json.dump(stats, open(os.path.join(out, "diff.json"), "w"), indent=1)
print("DIFF_DONE", len(stats), "pairs")
