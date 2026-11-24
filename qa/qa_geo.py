"""QA geometry and topology: what is wrong with a compiled model before anything moves.

  blender --background --factory-startup --python qa_geo.py -- <model.blend> <out_dir> --hero-cfg <hero.json>
          [--who ours|valve] [--pieces pieces.json] [--top 6] [--res 480]

Checks, per component (welded loose part; see qa_lib), at rest:
  nonmanifold   welded edges with 3 or more faces
  winding       an interior edge walked the same way by both faces: a neighbour is flipped (inconsistent normals)
  flipped       triangles whose shading (corner) normals point against their winding (dot < -0.2): lit from behind
  zero_area     triangles under 1e-9 m2 (the compiler drops some; the rest shade as sparkles)
  uv_degenerate triangles with real area whose UV area is zero (one texel smeared over the face) or not finite
  texel         texel density (texture px per metre, from the material's colour texture size) per triangle;
                outliers below a quarter or above 4x the material's median, on triangles over 1 mm2
  backface      open sheets (with border edges and no back shell) whose BACK side is seen from a QA camera
                (gameplay, front, side, back, below): the game culls back faces, so those pixels show through
                to whatever is behind: holes. Area in px at 480x720 summed over the cameras.
  *_px          for flipped, UV-smeared, winding and non-manifold triangles: the drawn area a QA camera sees,
                so a flaw buried inside the model ranks below one on the surface
  rest_contact  pieces that intersect at rest (BVH triangle overlap), by estimated intersection length
  flat_swatches palette pieces (one colour, UVs collapsed by design) by the area a camera sees, with the colour:
                a big flat swatch reads as an unshaded patch (the flat cyan tail lining)
  floating_at_rest  small sewn-on pieces (under 40 cm, not cloth) whose inner side has nothing under it: the median
                inward gap over 15 mm, or under a fifth of the inward rays meeting any surface within 15 cm
  chain_pieces_skinned_rigid  pieces the designer classed "chain" with under 30% weight on cloth bones: they move as
                a block with their parent bone instead of flying (rigid tail fins)
Writes <out>/geo.json and close-ups of the worst few per check under <out>/img/.
"""
import bpy, sys, os, json, math, time
import numpy as np
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import qa_lib as L
from mathutils import Vector

O = L.Opts(L.args())
SRC, OUT = O.a[0], O.a[1]
cfg = json.load(open(O.get("--hero-cfg")))
WHO = O.get("--who", "ours")
TOP = int(O.get("--top", "6"))
RES = int(O.get("--res", "480"))
os.makedirs(os.path.join(OUT, "img"), exist_ok=True)
T0 = time.time()

sc, arm, meshes = L.open_model(SRC)
M = L.Model(sc, arm, meshes, cfg)
M.name_pieces(O.get("--pieces"))
W = M.wpos(M.rest)
n_geo, area = M.tri_geom(W)
NC = M.ncomp
print("[geo] model", WHO, "tris", len(M.tris), "components", NC, flush=True)
fx = M.comp_cls == "fx"
shell = np.zeros(NC, bool); shell[list(M.shell_of)] = True
tc = M.tri_comp

def per_comp(mask):
    return np.bincount(tc[mask], minlength=NC)

R = {"who": WHO, "model": SRC, "tris": int(len(M.tris)), "components": int(NC), "back_shells": len(M.shell_of),
     "named": len(M.piece_name), "checks": {}}

# --- nonmanifold + open borders
E, F = M.edges, M.edge_faces
nm_e = E[F > 2]
nm_c = np.bincount(M.vcomp[nm_e[:, 0]], minlength=NC) if len(nm_e) else np.zeros(NC, int)
border_c = np.bincount(M.vcomp[E[F == 1][:, 0]], minlength=NC) if (F == 1).any() else np.zeros(NC, int)
# --- winding
dd = M.dir_dupe
wind_c = np.bincount(M.vcomp[dd // M.nw], minlength=NC) if len(dd) else np.zeros(NC, int)
# --- flipped shading normals
flip = np.zeros(len(M.tris), bool)
off = 0
for oi, o in enumerate(M.objs):
    m = o.data
    nt = len(m.loop_triangles)
    cn = np.empty(len(m.loops) * 3, np.float32)
    try:
        m.corner_normals.foreach_get("vector", cn)
    except AttributeError:
        m.calc_normals_split(); m.loops.foreach_get("normal", cn)
    cn = cn.reshape(-1, 3)
    lt = np.empty(nt * 3, np.int64); m.loop_triangles.foreach_get("loops", lt)
    mw = np.array(o.matrix_world)[:3, :3]
    tn = cn[lt.reshape(-1, 3)].mean(1) @ mw.T
    tn /= np.maximum(np.linalg.norm(tn, axis=1), 1e-12)[:, None]
    flip[off:off + nt] = (tn * n_geo[off:off + nt]).sum(1) < -0.2
    off += nt
flip &= ~M.degen & (area > 1e-8)
flip_c = per_comp(flip)
# --- zero area
zero = (area < 1e-9) | M.degen
zero_c = per_comp(zero)
# --- UVs
uv = M.tri_uv.astype(np.float64)
uva = 0.5 * np.abs((uv[:, 1, 0] - uv[:, 0, 0]) * (uv[:, 2, 1] - uv[:, 0, 1]) - (uv[:, 2, 0] - uv[:, 0, 0]) * (uv[:, 1, 1] - uv[:, 0, 1]))
uvbad = (~np.isfinite(uv).all((1, 2))) | ((uva < 1e-12) & (area > 1e-7))
uvbad &= ~fx[tc]
# a piece mapped onto a colour palette has every triangle's UVs collapsed to one swatch by design: only a piece
# that is mostly textured but has some collapsed triangles is smearing a texel over real faces
coll_share = per_comp(uvbad) / np.maximum(per_comp(np.ones(len(M.tris), bool)), 1)
palette = coll_share > 0.9
uvbad &= ~palette[tc]
uv_c = per_comp(uvbad)
R["palette_pieces"] = int((palette & ~fx).sum())
# --- texel density
texres = {}
for o in M.objs:
    for s in o.material_slots:
        mat = s.material
        if not mat or not mat.use_nodes:
            continue
        best = 0
        for nd in mat.node_tree.nodes:
            if nd.type == "TEX_IMAGE" and nd.image is not None:
                try:
                    best = max(best, max(nd.image.size))
                except Exception:
                    pass
        texres[mat.name] = best
res_t = np.array([texres.get(M.mat_names[i], 0) for i in range(len(M.mat_names))])[M.tri_mat].astype(float)
dens = np.sqrt(uva * res_t ** 2 / np.maximum(area, 1e-12))
dmask = (area > 1e-6) & (res_t > 1) & ~uvbad & ~fx[tc]
tex_out = np.zeros(len(M.tris), bool)
tex_stats = {}
for mi in np.unique(M.tri_mat[dmask]):
    sel = dmask & (M.tri_mat == mi)
    med = float(np.median(dens[sel]))
    o_ = sel & ((dens < med / 4) | (dens > med * 4))
    tex_out |= o_
    name = M.mat_names[mi]
    st = tex_stats.setdefault(name, {"texture_px": int(texres.get(name, 0)), "median_px_per_m": 0, "tris": 0, "outliers": 0,
                                    "p5": 0, "p95": 0})
    st["median_px_per_m"] = round(med, 1); st["tris"] += int(sel.sum()); st["outliers"] += int(o_.sum())
    st["p5"] = round(float(np.percentile(dens[sel], 5)), 1); st["p95"] = round(float(np.percentile(dens[sel], 95)), 1)
tex_c = per_comp(tex_out)
R["texel_density"] = tex_stats
print("[geo] topology/uv done %.0fs" % (time.time() - T0), flush=True)

# --- back faces seen
lo_b, hi_b = L.body_bounds(M, W)
tree_all, idx_all = M.bvh(W, ~fx[tc])
open_comp = (border_c > 0) & ~shell & ~fx
has_shell = np.zeros(NC, bool); has_shell[list(M.shell_of.values())] = True
cand = open_comp & ~has_shell
cam_views = ["gameplay", "front", "side", "back", "below"]

def seen(tri_ids, back, per_tri=None):
    """Pixels (480x720, summed over the QA cameras) of these triangles that a camera sees: the back side when
    back=True (culled in game: a hole), the front side otherwise (drawn: the flaw shows). Per component, and the
    seen triangles."""
    px, tris_seen = np.zeros(NC), {}
    tri_ids = np.asarray(tri_ids, np.int64)
    tri_ids = tri_ids[~M.degen[tri_ids] & (area[tri_ids] > 1e-8)]
    if not len(tri_ids):
        return px, tris_seen
    cen = W[M.tris[tri_ids]].mean(1)
    for view in cam_views:
        kind, loc, tgt, k = L.camera_spec(view, lo_b, hi_b)
        loc = np.array(loc)
        if kind == "ORTHO":
            d = np.array(tgt) - loc; d /= np.linalg.norm(d)
            dirs = np.repeat(d[None], len(tri_ids), 0)
            origins = cen - dirs * 3.0
            scale = np.full(len(tri_ids), (RES / k) ** 2)
        else:
            dirs = cen - loc
            dist = np.linalg.norm(dirs, axis=1); dirs /= dist[:, None]
            origins = np.repeat(loc[None], len(tri_ids), 0)
            scale = (k / 36.0 * RES / dist) ** 2
        facing = (n_geo[tri_ids] * dirs).sum(1)          # > 0: the camera sees the back side
        sel = np.nonzero(facing > 0.05 if back else facing < -0.05)[0]
        for j in sel:
            hit = tree_all.ray_cast(Vector(origins[j]), Vector(dirs[j]), 10.0)
            if hit[0] is not None and idx_all[hit[2]] == tri_ids[j]:
                c = tc[tri_ids[j]]
                px[c] += area[tri_ids[j]] * abs(facing[j]) * scale[j]
                if per_tri is not None:
                    per_tri[tri_ids[j]] += area[tri_ids[j]] * abs(facing[j]) * scale[j]
                tris_seen.setdefault(int(c), set()).add(int(tri_ids[j]))
    return px, tris_seen

back_px, back_tris = seen(np.nonzero(cand[tc])[0], back=True)
flip_px, _ = seen(np.nonzero(flip & ~fx[tc])[0], back=False)
uv_px, _ = seen(np.nonzero(uvbad)[0], back=False)
def tris_on_edges(edges):
    m_ = np.zeros(M.nw, bool); m_[np.asarray(edges).reshape(-1)] = True
    return np.nonzero((m_[M.tris].sum(1) >= 2) & ~M.degen)[0]
wind_e = np.stack([M.dir_dupe // M.nw, M.dir_dupe % M.nw], 1) if len(M.dir_dupe) else np.zeros((0, 2), np.int64)
wind_px, _ = seen(tris_on_edges(wind_e), back=False)
nm_px, _ = seen(tris_on_edges(nm_e), back=False)
print("[geo] backfaces done %.0fs" % (time.time() - T0), flush=True)

# --- rest contacts
pr = np.array(tree_all.overlap(tree_all), np.int64).reshape(-1, 2)
contacts = {}
if len(pr):
    pr = idx_all[pr]
    pa, pb = M.piece[tc[pr[:, 0]]], M.piece[tc[pr[:, 1]]]
    keep = pa != pb
    pr, pa, pb = pr[keep], pa[keep], pb[keep]
    seg = np.sqrt(2 * np.maximum(area, 0))
    swap = pa > pb
    pr[swap] = pr[swap][:, ::-1]
    key = np.minimum(pa, pb) * NC + np.maximum(pa, pb)
    for k in np.unique(key):
        sel = key == k
        a, b = divmod(int(k), NC)
        contacts[(a, b)] = (M.contact_length(pr[sel], seg), np.unique(pr[sel].reshape(-1)))
print("[geo] rest contacts %d piece pairs %.0fs" % (len(contacts), time.time() - T0), flush=True)

# --- tables
rows = []
for c in range(NC):
    if fx[c] or shell[c]:
        continue
    r = {"comp": c, "label": M.label(c), "cls": M.comp_cls[c], "tris": int((tc == c).sum()),
         "nonmanifold": int(nm_c[c]), "winding": int(wind_c[c]), "flipped": int(flip_c[c]), "zero_area": int(zero_c[c]),
         "uv_degenerate": int(uv_c[c]), "texel_outliers": int(tex_c[c]), "open_border_edges": int(border_c[c]),
         "back_shell": bool(has_shell[c]), "backface_px": round(float(back_px[c]), 1),
         "flipped_px": round(float(flip_px[c]), 1), "uv_px": round(float(uv_px[c]), 1), "winding_px": round(float(wind_px[c]), 1),
         "nonmanifold_px": round(float(nm_px[c]), 1)}
    if any(r[k] for k in ("nonmanifold", "winding", "flipped", "zero_area", "uv_degenerate", "texel_outliers")) or r["backface_px"] > 0:
        rows.append(r)
R["components_flagged"] = rows

# --- flat swatches: pieces drawn in one palette colour (UVs collapsed by design), by the area a camera sees. A big
# flat swatch reads as an unshaded hole in the costume (no weave, no fold detail); the colour is read from the texture
TCOL = L.tri_colours(M)
flat_t = np.zeros(len(M.tris))
flat_px, _ = seen(np.nonzero(palette[tc] & ~fx[tc])[0], back=False, per_tri=flat_t)
# per piece and swatch colour: the area seen, so "the sky lining of the tails" is one row
sw = {}
for t in np.nonzero(flat_t > 0)[0]:
    cc = TCOL[t]
    if np.isnan(cc).any():
        continue
    key = (int(M.piece[tc[t]]), tuple(np.round(cc, 2)))
    sw[key] = sw.get(key, 0.0) + flat_t[t]
rows_sw = []
for (c, cc), px_ in sw.items():
    lum = 0.2126 * cc[0] + 0.7152 * cc[1] + 0.0722 * cc[2]
    rows_sw.append({"label": M.label(c), "colour_rgb": list(cc), "luminance": round(float(lum), 3), "seen_px": round(float(px_), 1)})
# light flat colours show the missing shading most (a dark satin keeps its sheen highlight): those first
R["flat_swatches"] = sorted(rows_sw, key=lambda r: -(r["seen_px"] * (0.2 + r["luminance"])))[:40]

# --- chain pieces skinned rigid: the designer marked a piece "chain" (it should fly) but its vertices carry little
# weight on cloth bones, so it moves as one block with its parent bone
mech = {}
if O.get("--pieces") and os.path.exists(O.get("--pieces")):
    mech = {p_["name"]: p_.get("mech", "") for p_ in json.load(open(O.get("--pieces")))["pieces"]}
rigid_chain = []
for c in range(NC):
    nm = M.piece_name.get(int(M.piece[c]), "").rstrip("?")
    if mech.get(nm) != "chain":
        continue
    vs = np.nonzero(M.vcomp == c)[0]
    share = float(M.wcloth[vs].mean()) if len(vs) else 0.0
    if share < 0.3:
        rigid_chain.append({"label": M.label(c), "cloth_weight": round(share, 2), "verts": int(len(vs)),
                            "size_cm": round(float(np.linalg.norm(M.comp_hi[c] - M.comp_lo[c])) * 100, 1)})
R["chain_pieces_skinned_rigid"] = sorted(rigid_chain, key=lambda r: -r["size_cm"])

# --- floating at rest: small sewn-on pieces (bands, garters, ties, buttons, appliques; under 40 cm, not cloth) whose
# inner side has nothing under it. From each vertex of the piece's drawn sheet (back shells left out) a ray goes
# inward (against the normal) up to 15 cm to the first other surface. A piece that sits on something has short
# gaps; "floating" = the median gap over 15 mm, or under a fifth of its rays meet anything at all
tree_in, idx_in = M.bvh(W, ~fx[tc] & ~shell[tc])
Nw = M.vertex_normals(W)
floating = []
groups = {}
for c in range(NC):
    if fx[c] or shell[c] or M.comp_cls[c] not in ("outfit", "cloth"):
        continue
    groups.setdefault(int(M.piece[c]), []).append(c)
for pc, comps in groups.items():
    vs = np.nonzero(np.isin(M.vcomp, comps))[0]
    ext = float(np.linalg.norm(W[vs].max(0) - W[vs].min(0))) if len(vs) else 0
    if len(vs) < 6 or ext > 0.40 or float(M.wcloth[vs].mean()) > 0.3:
        continue
    d = []
    for v in vs[:: max(1, len(vs) // 60)]:
        o_ = Vector(W[v]) - Vector(Nw[v]) * 2e-4
        hit = tree_in.ray_cast(o_, -Vector(Nw[v]), 0.15)
        # skip hits on the piece itself (a tube's far wall)
        while hit[0] is not None and M.piece[tc[idx_in[hit[2]]]] == pc:
            o_ = hit[0] - Vector(Nw[v]) * 2e-4
            hit = tree_in.ray_cast(o_, -Vector(Nw[v]), 0.15)
        d.append((hit[0] - Vector(W[v])).length if hit[0] is not None else np.nan)
    d = np.array(d)
    hitf = float(np.mean(~np.isnan(d)))
    med = float(np.nanmedian(d)) if hitf > 0 else None
    if (med is not None and med > 0.015 and hitf >= 0.2) or hitf < 0.2:
        floating.append({"label": M.label(pc), "size_cm": round(ext * 100, 1), "rays_hitting": round(hitf, 2),
                         "median_gap_mm": round(med * 1000, 1) if med is not None else None})
R["floating_at_rest"] = sorted(floating, key=lambda r: -(r["median_gap_mm"] or 999))

R["totals"] = {k: int(sum(r[k] for r in rows)) for k in ("nonmanifold", "winding", "flipped", "zero_area", "uv_degenerate", "texel_outliers")}
R["totals"]["backface_px"] = round(float(back_px.sum()), 1)
for k_, a_ in (("flipped_px", flip_px), ("uv_px", uv_px), ("winding_px", wind_px), ("nonmanifold_px", nm_px)):
    R["totals"][k_] = round(float(a_[~fx].sum()), 1)
R["totals"]["open_sheets_without_back"] = int(cand.sum())
R["totals"]["flat_swatch_px"] = round(float(flat_px.sum()), 1)
R["totals"]["chain_pieces_skinned_rigid"] = len(rigid_chain)
R["totals"]["floating_at_rest"] = len(floating)
cl = sorted(contacts.items(), key=lambda kv: -kv[1][0])
R["rest_contacts"] = [{"a": M.label(a), "b": M.label(b), "a_cls": M.comp_cls[a], "b_cls": M.comp_cls[b],
                       "length_cm": round(v[0] * 100, 1), "tris": int(len(v[1]))} for (a, b), v in cl[:200]]
R["totals"]["rest_contact_pairs"] = len(contacts)
R["totals"]["rest_contact_cm"] = round(sum(v[0] for v in contacts.values()) * 100, 1)
R["by_class"] = {}
for (a, b), v in contacts.items():
    k = "-".join(sorted([M.comp_cls[a], M.comp_cls[b]]))
    R["by_class"][k] = round(R["by_class"].get(k, 0) + v[0] * 100, 1)

# --- images: the worst few per check
L.workbench(sc, "TEXTURE", res=RES)
cam = L.place_camera(sc, "front", lo_b, hi_b, res=RES)
tree_vis, idx_vis = M.bvh(W)          # everything that renders: what can block a close-up

def shoot(name, tris, view_from=None):
    """Close-up of one cluster of the flagged triangles: plain, then with the cluster in red."""
    tris = np.asarray(sorted(tris))
    if not len(tris):
        return None
    cl, ok = L.aim_closeup(cam, W, M.tris, tris, tree_vis, idx_vis, prefer=view_from)
    hl = L.highlight_object("qa_hl", W, M.tris[cl])
    sc.render.resolution_x = sc.render.resolution_y = RES
    out = os.path.join(OUT, "img", name)
    hl.hide_render = True
    sc.render.filepath = out + ".png"; bpy.ops.render.render(write_still=True)
    hl.hide_render = False
    sc.render.filepath = out + "_hl.png"; bpy.ops.render.render(write_still=True)
    bpy.data.objects.remove(hl)
    return os.path.basename(out)

R["images"] = []
for key, mask, cnt, pxa in (("flipped", flip, flip_c, flip_px), ("winding", None, wind_c, wind_px), ("uv_degenerate", uvbad, uv_c, uv_px),
                       ("texel_outliers", tex_out, tex_c, None), ("nonmanifold", None, nm_c, nm_px)):
    order_ = np.lexsort((-cnt, -(pxa if pxa is not None else np.zeros(NC))))
    for rank, c in enumerate(order_[:TOP]):
        if cnt[c] == 0 or fx[c]:
            break
        if mask is not None:
            tris = np.nonzero(mask & (tc == c))[0]
        else:
            vs = np.unique((M.dir_dupe[M.vcomp[M.dir_dupe // M.nw] == c] // M.nw)) if key == "winding" else np.unique(nm_e[M.vcomp[nm_e[:, 0]] == c].reshape(-1))
            m_ = np.zeros(M.nw, bool); m_[vs] = True
            tris = np.nonzero(m_[M.tris].any(1) & (tc == c))[0]
        # a flipped face is drawn from the side its winding faces and shaded as if from the other: look at it
        # from the drawn side
        vf = None
        if key == "flipped" and len(tris):
            vf = n_geo[tris].mean(0); vf = vf / max(np.linalg.norm(vf), 1e-9)
        img = shoot("%s_%d" % (key, rank + 1), tris, vf)
        R["images"].append({"check": key, "rank": rank + 1, "label": M.label(c), "count": int(cnt[c]),
                            "seen_px": round(float(pxa[c]), 1) if pxa is not None else None, "image": img})
for rank, c in enumerate(np.argsort(-back_px)[:TOP]):
    if back_px[c] <= 0:
        break
    tris = back_tris[int(c)]
    # look at it from where the back is seen: the reverse of the mean normal
    nn = -n_geo[list(tris)].mean(0); nn /= max(np.linalg.norm(nn), 1e-9)
    img = shoot("backface_%d" % (rank + 1), tris, nn)
    R["images"].append({"check": "backface", "rank": rank + 1, "label": M.label(c), "count": round(float(back_px[c]), 1), "image": img})
for rank, ((a, b), v) in enumerate(cl[:TOP]):
    img = shoot("rest_contact_%d" % (rank + 1), v[1])
    R["images"].append({"check": "rest_contact", "rank": rank + 1, "label": "%s | %s" % (M.label(a), M.label(b)),
                        "count": round(v[0] * 100, 1), "image": img})
# every component's label, so reports can name pieces in other steps' output by component id
json.dump({int(c): M.label(c) for c in range(NC)}, open(os.path.join(OUT, "components.json"), "w"))
json.dump(R, open(os.path.join(OUT, "geo.json"), "w"), indent=1, default=lambda x: x.item() if hasattr(x, "item") else str(x))
print("[geo] GEO_DONE", WHO, json.dumps(R["totals"]), "%.0fs" % (time.time() - T0), flush=True)
