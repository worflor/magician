"""Albedo range: the colours a skin actually puts on its surface, against the range Valve paints the same hero in.

Valve's character-art rule (Dota 2 Workshop character art guide): no absolute white or black, no channel at its
maximum, because the shading and the NPR outline need headroom: near-white flattens and blows out under bloom and
self-illum, near-black loses every fold and reads as a hole. Deadlock's own heroes follow it.

Input: Source 2 Viewer glb exports *with materials* (`--gltf_export_materials`) of the mod's model and Valve's.
Each LOD0 triangle samples its material's colour texture at its UV centroid (nearest texel; palette pieces sample
their swatch exactly) and is weighted by its area. Reported per material and overall: linear-luminance percentiles,
and the area that falls below Valve's 0.5th percentile or above its 99.5th (Valve's own range, measured the same way
on Valve's model). Additive glint and sparkle cards are skipped.

usage: python -I albedo_range.py --ours <ours.glb> --valve <valve.glb> [--out f.json] [--max-out 0.10]
Exit 1 when more than --max-out of our area falls outside Valve's range (either end).
"""
import argparse, json, os, sys
import numpy as np
from PIL import Image
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from glbio import Glb

Image.MAX_IMAGE_PIXELS = None


def lin(c):
    c = np.asarray(c, np.float64) / 255.0
    return np.where(c <= 0.04045, c / 12.92, ((c + 0.055) / 1.055) ** 2.4)


def samples(glb_path):
    g = Glb(glb_path)
    base = os.path.dirname(glb_path)
    mats = g.j["materials"]
    imgs = g.j.get("images", [])
    texs = g.j.get("textures", [])
    cache = {}
    out = {}
    for p in g.primitives():
        name = p["material"]
        if not name or p["uv"] is None or any(k in name for k in ("glint", "sparkle")):
            continue
        mi = next(i for i, m in enumerate(mats) if m.get("name") == name)
        bct = mats[mi].get("pbrMetallicRoughness", {}).get("baseColorTexture")
        if bct is None:
            continue
        uri = imgs[texs[bct["index"]]["source"]]["uri"]
        if uri not in cache:
            cache[uri] = np.asarray(Image.open(os.path.join(base, uri)).convert("RGB"))
        img = cache[uri]
        h, w = img.shape[:2]
        tri = p["idx"]
        uv = p["uv"][tri].mean(1)
        x = np.clip((uv[:, 0] % 1.0) * w, 0, w - 1).astype(int)
        y = np.clip((uv[:, 1] % 1.0) * h, 0, h - 1).astype(int)
        rgb = img[y, x]
        L = lin(rgb) @ np.array([0.2126, 0.7152, 0.0722])
        P = p["pos"][tri]
        A = 0.5 * np.linalg.norm(np.cross(P[:, 1] - P[:, 0], P[:, 2] - P[:, 0]), axis=1)
        o = out.setdefault(name, [[], []])
        o[0].append(L)
        o[1].append(A)
    return {k: (np.concatenate(v[0]), np.concatenate(v[1])) for k, v in out.items()}


def wpct(x, w, q):
    i = np.argsort(x)
    c = np.cumsum(w[i]) / w.sum()
    return [float(x[i][min(np.searchsorted(c, qq / 100.0), len(x) - 1)]) for qq in q]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--ours", required=True)
    ap.add_argument("--valve", required=True)
    ap.add_argument("--out")
    ap.add_argument("--max-out", type=float, default=0.10)
    a = ap.parse_args()
    V, O = samples(a.valve), samples(a.ours)
    vL = np.concatenate([v[0] for v in V.values()])
    vA = np.concatenate([v[1] for v in V.values()])
    lo, hi = wpct(vL, vA, [0.5, 99.5])
    res = {"valve_range_linear": [round(lo, 4), round(hi, 4)], "materials": {}}
    tot = sum(v[1].sum() for v in O.values())
    below = above = 0.0
    for name, (L, A) in sorted(O.items()):
        b, h = float(A[L < lo].sum()), float(A[L > hi].sum())
        below += b
        above += h
        res["materials"][name] = {"area_share": round(float(A.sum() / tot), 4),
                                  "p1_p50_p99": [round(v, 4) for v in wpct(L, A, [1, 50, 99])],
                                  "below_valve": round(b / A.sum(), 4), "above_valve": round(h / A.sum(), 4)}
    res["ours_below_valve"] = round(below / tot, 4)
    res["ours_above_valve"] = round(above / tot, 4)
    res["valve_p1_p50_p99"] = [round(v, 4) for v in wpct(vL, vA, [1, 50, 99])]
    res["ours_p1_p50_p99"] = [round(v, 4) for v in wpct(np.concatenate([v[0] for v in O.values()]),
                                                         np.concatenate([v[1] for v in O.values()]), [1, 50, 99])]
    res["verdict"] = "fail" if below + above > a.max_out * tot else "pass"
    txt = json.dumps(res, indent=1)
    if a.out:
        open(a.out, "w").write(txt)
    print(txt)
    return 1 if res["verdict"] == "fail" else 0


if __name__ == "__main__":
    sys.exit(main())
