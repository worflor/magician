"""Rename a Source 2 Viewer glb's clips to short, unique names, and list them.

  python -I glb_clips.py <in.glb> <out.glb> [--tsv clips.tsv]

Source 2 Viewer names AnimGraph2 clips by their resource path (models/heroes_staging/lash_v2/clips/ui_pose).
Blender (4.5 and 5.2 both) truncates ID names to 63 bytes, so long clip paths collide and get .001 suffixes, which
silently swaps clips. Here AnimGraph2 clips become their file name (ui_pose: the names the animation graphs and
ANIM.md use) and the model's legacy sequences get a "seq." prefix, since several share a name with a clip
(ability_flog is both). Only the JSON chunk changes; the binary chunk is copied as is.

The TSV lists every clip: name, original name, kind (ag2 / legacy), frames at the clip's own rate, fps, seconds.
"""
import json, struct, sys, os

args = sys.argv[1:]
src, dst = args[0], args[1]
tsv = args[args.index("--tsv") + 1] if "--tsv" in args else os.path.splitext(dst)[0] + "_clips.tsv"

with open(src, "rb") as f:
    magic, version, _ = struct.unpack("<III", f.read(12))
    jlen, jtype = struct.unpack("<II", f.read(8))
    j = json.loads(f.read(jlen))
    rest = f.read()                 # the BIN chunk header and data, unchanged

def acc_times(i):
    a = j["accessors"][i]
    return a.get("min", [0])[0], a.get("max", [0])[0], a["count"]

used, rows = set(), []
for an in j.get("animations", []):
    orig = an.get("name", "")
    if "/" in orig:
        name, kind = orig.rsplit("/", 1)[-1], "ag2"
    else:
        name, kind = "seq." + orig, "legacy"
    base, k = name, 2
    while name in used:
        name = "%s~%d" % (base, k)
        k += 1
    used.add(name)
    an["name"] = name
    t0, t1, n = 0.0, 0.0, 1
    for s in an.get("samplers", []):
        a, b, c = acc_times(s["input"])
        if c > n:
            t0, t1, n = a, b, c
    dur = t1 - t0
    fps = (n - 1) / dur if dur > 0 else 30.0
    rows.append((name, orig, kind, n, round(fps, 3), round(dur, 4)))

body = json.dumps(j, separators=(",", ":")).encode("utf-8")
body += b" " * ((4 - len(body) % 4) % 4)
with open(dst, "wb") as f:
    f.write(struct.pack("<III", magic, version, 12 + 8 + len(body) + len(rest)))
    f.write(struct.pack("<II", len(body), jtype))
    f.write(body)
    f.write(rest)
with open(tsv, "w", encoding="utf-8") as f:
    f.write("name\toriginal\tkind\tframes\tfps\tseconds\n")
    for r in rows:
        f.write("\t".join(str(x) for x in r) + "\n")
print("GLB_CLIPS", dst, len(rows), "clips,", sum(r[2] == "ag2" for r in rows), "AnimGraph2")
