"""Hitbox envelope: how much of a hero's visible surface lies outside its hitboxes, ours against Valve's.

A skin that adds visible volume where there is no hitbox makes enemies shoot cloth that cannot be hit (a fair-play
and readability defect: CS2 agent skins were pulled from a Major over exactly this). Hitboxes come from the compiled
model (`magician dump <vpk> <model> --block MDAT`: capsules on bones, end points in bone space, Source units); the
surface from Source 2 Viewer's glb export of the same compiled model at bind pose.

For every LOD0 triangle (area-weighted, centroid sampled) the distance outside the nearest capsule is measured in
Source units. Reported per model: the area fraction outside by more than 0, 2, 5, 10 and 20 units, and where (by
height band and by the nearest hitbox), then ours minus Valve's.

usage: python -I hitbox_envelope.py --ours <ours.glb> --ours-mdat <dump.txt> --valve <valve.glb> --valve-mdat <dump.txt>
                                    [--out findings.json] [--lod0-tris N]
Exit 1 when ours puts more than --max-excess (default 0.05) more of its area over 5 units outside than Valve's.
"""
import argparse, json, os, re, sys
import numpy as np
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from glbio import Glb, INCH

BANDS = (0, 2, 5, 10, 20)


def parse_hitboxes(path):
    t = open(path, encoding="utf-8", errors="replace").read()
    i = t.find("m_HitBoxes")
    j = t.find("]", t.find("m_nHitBoxIndex", i))
    out, seen = [], set()
    for m in re.finditer(r'm_name = "([^"]+)"\s*m_sSurfaceProperty = "[^"]*"\s*m_sBoneName = "([^"]+)"\s*'
                         r'm_vMinBounds = \[([^\]]+)\]\s*m_vMaxBounds = \[([^\]]+)\]\s*m_flShapeRadius = ([-\d.e]+)'
                         r'(?:.|\n)*?m_nShapeType = (\d+)', t[i:]):
        name = m.group(1)
        if name in seen:          # every mesh carries the same set; keep the first
            break
        seen.add(name)
        out.append({"name": name, "bone": m.group(2),
                    "a": np.array([float(x) for x in m.group(3).split(",")]),
                    "b": np.array([float(x) for x in m.group(4).split(",")]),
                    "r": float(m.group(5)), "shape": int(m.group(6))})
    return out


def capsules_world(hbs, bones):
    """Capsule end points in glb space (metres) from bone-space Source units."""
    caps = []
    for h in hbs:
        M = bones.get(h["bone"])
        if M is None:
            continue
        a = M[:3, :3] @ (h["a"] * INCH) + M[:3, 3]
        b = M[:3, :3] @ (h["b"] * INCH) + M[:3, 3]
        caps.append((h["name"], a, b, h["r"] * INCH))
    return caps


def outside(points, caps):
    """Distance (Source units) outside the nearest capsule, and that capsule's index."""
    best = np.full(len(points), np.inf)
    arg = np.zeros(len(points), np.int64)
    for k, (_, a, b, r) in enumerate(caps):
        ab = b - a
        L = float(ab @ ab)
        t = np.clip(((points - a) @ ab) / L, 0, 1) if L > 1e-12 else np.zeros(len(points))
        d = np.linalg.norm(points - (a + t[:, None] * ab), axis=1) - r
        better = d < best
        best[better], arg[better] = d[better], k
    return best / INCH, arg


def surface(glb_path, lod0_tris=None):
    g = Glb(glb_path)
    P, A = [], []
    total = 0
    for p in g.primitives():
        if "sparkles" in p["mesh"] or "glint" in p["mesh"]:
            continue                       # additive cards: no volume
        tri = p["pos"][p["idx"]]
        total += len(tri)
        P.append(tri.mean(1))
        A.append(0.5 * np.linalg.norm(np.cross(tri[:, 1] - tri[:, 0], tri[:, 2] - tri[:, 0]), axis=1))
    return g, np.concatenate(P), np.concatenate(A), total


def measure(glb_path, mdat_path):
    g, C, A, n = surface(glb_path)
    caps = capsules_world(parse_hitboxes(mdat_path), g.bones())
    d, k = outside(C, caps)
    keep = A > 0
    C, A, d, k = C[keep], A[keep], d[keep], k[keep]
    tot = A.sum()
    res = {"triangles": int(n), "area_m2": round(float(tot), 4), "hitboxes": len(caps),
           "outside": {str(b): round(float(A[d > b].sum() / tot), 4) for b in BANDS},
           "worst_units": round(float(np.percentile(d, 99.9)), 1)}
    # where: by nearest hitbox, area outside by > 5 units
    by = {}
    for i, (name, *_ ) in enumerate(caps):
        s = A[(k == i) & (d > 5)].sum()
        if s > 0:
            by[name] = round(float(s / tot), 4)
    res["outside_5_by_hitbox"] = dict(sorted(by.items(), key=lambda x: -x[1]))
    # height bands (glb Y is up), fraction of each band's area outside by > 5
    y = C[:, 1]
    lo, hi = float(y.min()), float(y.max())
    bands = {}
    for b in range(10):
        m = (y >= lo + (hi - lo) * b / 10) & (y < lo + (hi - lo) * (b + 1) / 10 + 1e-9)
        if A[m].sum() > 0:
            bands["%d-%d%%" % (b * 10, b * 10 + 10)] = round(float(A[m & (d > 5)].sum() / A[m].sum()), 3)
    res["outside_5_by_height"] = bands
    return res


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--ours", required=True)
    ap.add_argument("--ours-mdat", required=True)
    ap.add_argument("--valve", required=True)
    ap.add_argument("--valve-mdat", required=True)
    ap.add_argument("--out")
    ap.add_argument("--max-excess", type=float, default=0.05)
    a = ap.parse_args()
    ours, valve = measure(a.ours, a.ours_mdat), measure(a.valve, a.valve_mdat)
    excess = {b: round(ours["outside"][b] - valve["outside"][b], 4) for b in ours["outside"]}
    out = {"ours": ours, "valve": valve, "excess": excess,
           "verdict": "fail" if excess["5"] > a.max_excess else "pass", "max_excess_at_5_units": a.max_excess}
    txt = json.dumps(out, indent=1)
    if a.out:
        open(a.out, "w").write(txt)
    print(txt)
    return 1 if out["verdict"] == "fail" else 0


if __name__ == "__main__":
    sys.exit(main())
