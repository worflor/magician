"""Alpha-test coverage across mips: does alpha-tested lace keep its holes and threads at the distances it is seen?

An alpha-tested texture (F_ALPHA_TEST, cut at g_flAlphaTestReference1) is box-filtered into mips by the compiler.
Averaging moves alpha toward the tile's mean: holes smaller than a texel fill in (lace turns to solid cloth) and
threads thinner than a texel fall under the cut (lace vanishes and shimmers as it moves). The fix in the literature
is coverage-preserving mips (per mip, rescale alpha so the fraction above the cut matches mip 0; Castano,
"Computing Alpha Mipmaps"; NVTT scaleAlphaToCoverage; Unity mipMapsPreserveCoverage) or alpha-to-coverage.

Input: the material's colour texture as the compiler stored it (alpha = coverage; decode the compiled vtex with
Source 2 Viewer, or use the glb export's PNG), the glb with materials (for the texel density on the model), the alpha
reference. For each tile of the texture that holds lace (coverage between 5% and 95% at mip 0), the coverage at
mip k is compared with mip 0. The mip each viewing distance selects comes from the material's median texel density
on the model and the screen's pixels per metre at that distance (1080 lines, vertical FOV 60 degrees by default).

usage: python -I alpha_coverage.py --texture <color.png> --glb <ours.glb> --material maid_lace [--ref 0.5]
                                   [--tile 256] [--fov 60] [--lines 1080] [--out f.json]
Exit 1 when, at the gameplay distance (--gameplay, default 4 m), more than 10% of lace tiles change coverage by
more than 30% (in either direction).
"""
import argparse, json, math, os, sys
import numpy as np
from PIL import Image
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from glbio import Glb

Image.MAX_IMAGE_PIXELS = None


def density(glb_path, material, size):
    g = Glb(glb_path)
    d = []
    for p in g.primitives():
        if p["material"] != material or p["uv"] is None:
            continue
        P, U = p["pos"][p["idx"]], p["uv"][p["idx"]] * size
        a3 = 0.5 * np.linalg.norm(np.cross(P[:, 1] - P[:, 0], P[:, 2] - P[:, 0]), axis=1)
        a2 = 0.5 * np.abs((U[:, 1, 0] - U[:, 0, 0]) * (U[:, 2, 1] - U[:, 0, 1]) - (U[:, 2, 0] - U[:, 0, 0]) * (U[:, 1, 1] - U[:, 0, 1]))
        ok = (a3 > 1e-9) & (a2 > 1e-6)
        d.append(np.sqrt(a2[ok] / a3[ok]))
    return np.concatenate(d) if d else np.array([])


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--texture", required=True)
    ap.add_argument("--glb", required=True)
    ap.add_argument("--material", required=True)
    ap.add_argument("--ref", type=float, default=0.5)
    ap.add_argument("--tile", type=int, default=256)
    ap.add_argument("--fov", type=float, default=60.0)
    ap.add_argument("--lines", type=int, default=1080)
    ap.add_argument("--gameplay", type=float, default=4.0)
    ap.add_argument("--out")
    a = ap.parse_args()
    al = np.asarray(Image.open(a.texture).convert("RGBA"))[..., 3].astype(np.float32) / 255.0
    n = al.shape[0]
    dens = density(a.glb, a.material, n)
    med = float(np.median(dens)) if len(dens) else float("nan")
    ppm = lambda d: a.lines / (2 * d * math.tan(math.radians(a.fov) / 2))

    def blocks(m, b):
        k = m.shape[0] // b
        return m[:k * b, :k * b].reshape(k, b, k, b).transpose(0, 2, 1, 3).reshape(k, k, -1)
    c0 = (blocks(al, a.tile) > a.ref).mean(-1)
    lace = (c0 > 0.05) & (c0 < 0.95)
    mips, m = [], al
    for lvl in range(1, 9):
        if m.shape[0] < 2 or (a.tile >> lvl) < 1:
            break
        m = m.reshape(m.shape[0] // 2, 2, m.shape[1] // 2, 2).mean(axis=(1, 3))
        ck = (blocks(m, a.tile >> lvl) > a.ref).mean(-1)
        r = ck[lace] / c0[lace]
        mips.append({"mip": lvl, "size": m.shape[0], "tiles": int(lace.sum()),
                     "ratio_p5_p50_p95": [round(float(x), 2) for x in np.percentile(r, [5, 50, 95])],
                     "share_changed_30pct": round(float(((r < 0.7) | (r > 1.3)).mean()), 3),
                     "share_vanished": round(float((r < 0.1).mean()), 3)})
    dist = {}
    for d in (2.0, a.gameplay, 8.0, 15.0, 30.0):
        lvl = max(0.0, math.log2(med / ppm(d))) if med == med else float("nan")
        row = mips[min(int(round(lvl)), len(mips)) - 1] if lvl >= 0.5 and mips else {"mip": 0, "share_changed_30pct": 0.0}
        dist["%g m" % d] = {"mip": round(lvl, 2), "share_changed_30pct": row["share_changed_30pct"],
                            "share_vanished": row.get("share_vanished", 0.0)}
    res = {"texture": a.texture, "size": n, "ref": a.ref, "texel_density_px_per_m_median": round(med, 1),
           "texel_density_p5_p95": [round(float(x), 1) for x in np.percentile(dens, [5, 95])] if len(dens) else [],
           "mips": mips, "by_distance": dist}
    gp = dist["%g m" % a.gameplay]["share_changed_30pct"]
    res["verdict"] = "fail" if gp > 0.10 else "pass"
    txt = json.dumps(res, indent=1)
    if a.out:
        open(a.out, "w").write(txt)
    print(txt)
    return 1 if res["verdict"] == "fail" else 0


if __name__ == "__main__":
    sys.exit(main())
