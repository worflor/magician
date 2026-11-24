"""QA motion sweep: pose a compiled model on every clip of a set, frame by frame, and find what breaks.

  blender --background --factory-startup --python qa_motion.py -- <model.blend> <out_dir> --hero-cfg <hero.json>
          [--who ours|valve] [--clips a,b,c|all] [--frames 10] [--pieces pieces.json] [--top 8 per detector] [--res 480]
          [--mirror <other motion.json>]     render the same clip, frame and cameras as another run's top offenders
          [--mirror-only]                     with --mirror: no sweep, only those renders (motion.json is left alone)
          [--rerender <this model's motion.json>]  redo that run's top renders: the detectors run again on just those
                                             frames to recover the triangles; motion.json is left alone
          [--clip-source <other model.blend>] pose this mesh with the other model's clips instead of its own (they are
                                             loaded as <clip>@src): our mesh on Valve's clips separates what the mesh
                                             does from what a replaced clip does

Detectors (each finding is an "event" for one pair of pieces, or one piece, on one frame of one clip):
  poke     layer inversion. At rest every vertex is tied to the surface directly over it (a ray along its normal,
           up to 30 cm: the skirt bell over the thigh, the apron over the skirt). On a posed frame the vertex is
           tested against that surface's plane; if it has crossed by more than the tolerance (2 mm under a
           garment, 1 mm between garments) and a ray from the vertex back toward the surface really meets it, the
           inner surface shows through the outer one. Depth = how far through, in mm.
  contact  a triangle-triangle intersection between two pieces that do not intersect at rest (BVH overlap);
           length = an estimate of the intersection curve, in cm. Rest contacts that only slide are not reported
           unless they grow past 2x and 5 cm.
  stretch / crush   an edge longer than 1.6x or shorter than 0.5x its rest length (edges over 2 mm).
  float    a garment piece that hugs the surface under it at rest (within 1 cm, not cloth-driven) lifts off it by
           over 1.5 cm while still over the same spot: an arm band or applique floating off what it is sewn to
  spike    a vertex whose motion differs from its neighbours' mean by over 3 cm and 6 rest edge lengths: a flung
           or torn vertex (a bad skin weight).
Every event carries: the gameplay camera's visible area of its triangles (px at 720x1080, by ray casts against
the whole posed model), the bone region, and the share of its vertices driven by cloth bones (offline the cloth is
not simulated: those vertices sit at their animated goal pose, which the solver then moves in game).

Writes <out>/motion.json (every event, per clip and frame), <out>/clips.tsv (per clip counts) and, for the --top
most visible events of each detector, <out>/top/<rank>_<clip>_f<frame>_gameplay.png (triangles in red, drawn over
everything so they can be found), _close.png (0.7 m away, plain) and _close_hl.png (red, depth-tested).
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
NFR = int(O.get("--frames", "10"))
TOP = int(O.get("--top", "8"))       # per detector
RES = int(O.get("--res", "480"))      # run_qa.py passes half of it in draft; the detectors and their 720x1080 visibility rays are exact either way
MIRROR = O.get("--mirror")
MIRROR_ONLY = O.has("--mirror-only")      # only render another run's top offenders on this model; no sweep
RERENDER = O.get("--rerender")            # a motion.json of this model: redo its top renders only (detectors re-run on those frames)
os.makedirs(os.path.join(OUT, "top"), exist_ok=True)
T0 = time.time()

def log(*a):
    print("[motion %6.0fs]" % (time.time() - T0), *a, flush=True)

sc, arm, meshes = L.open_model(SRC)
M = L.Model(sc, arm, meshes, cfg)
M.name_pieces(O.get("--pieces"))
log("model", WHO, "verts", M.nv, "welded", M.nw, "tris", len(M.tris), "components", M.ncomp,
    "back shells", len(M.shell_of), "named", len(M.piece_name))

clip_arg = O.get("--clips", "all")
SRC_CLIPS = O.get("--clip-source")
SRC_CLIPS = os.path.abspath(SRC_CLIPS) if SRC_CLIPS else None
if SRC_CLIPS:
    want = clip_arg.split(",")
    before = set(bpy.data.actions)
    with bpy.data.libraries.load(SRC_CLIPS, link=False) as (src, dst):
        dst.actions = [a for a in src.actions if a in want]
    for a in set(bpy.data.actions) - before:
        base = a.name.rsplit(".", 1)[0] if a.name[-4:-3] == "." and a.name[-3:].isdigit() else a.name
        a.name = base + "@src"
    clip_arg = ",".join(c + "@src" for c in want)
names = sorted(a.name for a in bpy.data.actions)
clips = [n for n in names if not n.startswith("seq.")] if clip_arg == "all" else [c for c in clip_arg.split(",") if c in bpy.data.actions]
missing = [] if clip_arg == "all" else [c for c in clip_arg.split(",") if c not in bpy.data.actions]
if missing:
    log("clips not in this model:", missing)

W0 = M.wpos(M.rest)
N0 = M.vertex_normals(W0)
fx = M.comp_cls == "fx"
shell = np.zeros(M.ncomp, bool)
shell[list(M.shell_of.keys())] = True
coll_comp = ~fx & ~shell
coll_tri = coll_comp[M.tri_comp] & ~M.degen
coll_vert = coll_comp[M.vcomp]
PIECE = M.piece
OURS_CLS = {"outfit", "cloth"}
lo_b, hi_b = L.body_bounds(M)

# ----------------------------------------------------------------------------------------------- rest ties
tree0, idx0 = M.bvh(W0, coll_tri)
R_MAX = 0.30
ties_v, ties_t, ties_b, ties_d = [], [], [], []
t0 = time.time()
verts = np.nonzero(coll_vert)[0]
for v in verts:
    p = Vector(W0[v]); n = Vector(N0[v])
    if n.length < 0.5:
        continue
    start, travelled = p + n * 1e-4, 0.0
    for _ in range(4):
        hit = tree0.ray_cast(start, n, R_MAX - travelled)
        if hit[0] is None:
            break
        ti = idx0[hit[2]]
        travelled += hit[3]
        if PIECE[M.tri_comp[ti]] == PIECE[M.vcomp[v]]:
            start = hit[0] + n * 1e-4
            continue
        ties_v.append(v); ties_t.append(ti); ties_d.append(travelled)
        a, b, c = (W0[M.tris[ti, k]] for k in range(3))
        q = np.array(hit[0])
        T = np.array([a, b, c]).T
        # barycentric by least squares on the triangle
        e1, e2, e3 = b - a, c - a, q - a
        d11, d12, d22 = e1 @ e1, e1 @ e2, e2 @ e2
        d31, d32 = e3 @ e1, e3 @ e2
        den = d11 * d22 - d12 * d12
        w1 = (d22 * d31 - d12 * d32) / den if den else 0
        w2 = (d11 * d32 - d12 * d31) / den if den else 0
        ties_b.append((1 - w1 - w2, w1, w2))
        break
ties_v = np.array(ties_v, np.int64); ties_t = np.array(ties_t, np.int64)
ties_b = np.array(ties_b); ties_d = np.array(ties_d)
tie_inner = PIECE[M.vcomp[ties_v]]; tie_outer = PIECE[M.tri_comp[ties_t]]
tie_garment = np.isin(M.comp_cls[tie_inner], list(OURS_CLS)) & np.isin(M.comp_cls[tie_outer], list(OURS_CLS))
TOL = np.where(tie_garment, 0.001, 0.002)
HUG = (ties_d < 0.01) & np.isin(M.comp_cls[tie_outer], list(OURS_CLS)) & (M.wcloth[ties_v] < 0.3)

def plane_side(W, tv, tt, tb):
    tri = M.tris[tt]
    a, b, c = W[tri[:, 0]], W[tri[:, 1]], W[tri[:, 2]]
    p = a * tb[:, :1] + b * tb[:, 1:2] + c * tb[:, 2:3]
    n = np.cross(b - a, c - a)
    n /= np.maximum(np.linalg.norm(n, axis=1), 1e-20)[:, None]
    off = W[tv] - p
    s = (off * n).sum(1)
    lat = np.linalg.norm(off - s[:, None] * n, axis=1)
    return s, lat, n

S0, _, _ = plane_side(W0, ties_v, ties_t, ties_b)
SG0 = np.sign(S0); SG0[SG0 == 0] = -1
log("rest ties", len(ties_v), "of", len(verts), "vertices in %.1fs" % (time.time() - t0))

# rest overlaps between pieces
def overlaps(tree, idx):
    pr = np.array(tree.overlap(tree), np.int64).reshape(-1, 2)
    if len(pr) == 0:
        return pr
    pr = idx[pr]
    pa, pb = PIECE[M.tri_comp[pr[:, 0]]], PIECE[M.tri_comp[pr[:, 1]]]
    pr = pr[pa != pb]
    pr.sort(1)
    return np.unique(pr, axis=0)

t0 = time.time()
rest_ov = overlaps(tree0, idx0)
_, area0 = M.tri_geom(W0)
seglen = np.sqrt(2 * np.maximum(area0, 0))   # a triangle's typical size: a crossing segment is about that long

def pair_key(pr):
    a, b = PIECE[M.tri_comp[pr[:, 0]]], PIECE[M.tri_comp[pr[:, 1]]]
    return np.minimum(a, b) * M.ncomp + np.maximum(a, b)

def contact_len(pr):
    return np.minimum(seglen[pr[:, 0]], seglen[pr[:, 1]])

rest_len = {}
if len(rest_ov):
    k = pair_key(rest_ov)
    for kk in np.unique(k):
        rest_len[int(kk)] = M.contact_length(rest_ov[k == kk], seglen)
log("rest contacts: %d triangle pairs over %d piece pairs (%.1fs)" % (len(rest_ov), len(rest_len), time.time() - t0))

# stretch and spike tables
E = M.edges
L0 = np.linalg.norm(W0[E[:, 0]] - W0[E[:, 1]], axis=1)
E_OK = (L0 > 0.002) & coll_vert[E[:, 0]]
deg = np.bincount(E.reshape(-1), minlength=M.nw).astype(float)
meanL = np.zeros(M.nw); np.add.at(meanL, E[:, 0], L0); np.add.at(meanL, E[:, 1], L0)
meanL /= np.maximum(deg, 1)
# a triangle per edge, for highlighting
tri_of_edge = {}

# ----------------------------------------------------------------------------------------------- per frame
def gameplay_cam(shift):
    kind, loc, tgt, lens = L.camera_spec("gameplay", lo_b, hi_b, shift)
    return loc, tgt

def tris_of_verts(vs):
    m = np.zeros(M.nw, bool); m[vs] = True
    return np.nonzero(m[M.tris].any(1) & ~M.degen)[0]

GEOM = {}

def visibility(W, tree_all, idx_all, tris, cam):
    """Visible projected area (px at 720x1080) of these triangles from the gameplay camera, and the visible share."""
    if len(tris) == 0:
        return 0.0, 0.0
    sel = tris if len(tris) <= 160 else tris[np.linspace(0, len(tris) - 1, 160).astype(int)]
    cen = W[M.tris[sel]].mean(1)
    n, area = GEOM["n"], GEOM["area"]
    vis = L.visible(tree_all, cam.location, cen, tol=0.003)
    P = L.project(sc, cam, cen)
    inside = (P[:, 0] >= 0) & (P[:, 0] <= sc.render.resolution_x) & (P[:, 1] >= 0) & (P[:, 1] <= sc.render.resolution_y)
    vis &= inside
    # projected area of a triangle ~ area * (focal px / depth)^2 * |cos|; focal for a 50 mm lens on 36 mm film, 720 px wide
    fpx = 50.0 / 36.0 * 720
    d = np.linalg.norm(cen - np.array(cam.location), axis=1)
    view = (cen - np.array(cam.location)) / d[:, None]
    cosv = np.abs((n[sel] * view).sum(1))
    px = area[sel] * (fpx / d) ** 2 * cosv
    scale = len(tris) / len(sel)
    return float((px * vis).sum() * scale), float(vis.mean())

events = []
per_clip = {}
cam = L.place_camera(sc, "gameplay", lo_b, hi_b, res=720)
sc.render.resolution_x, sc.render.resolution_y = 720, 1080

def describe(kind, a, b, W, tris, extra, clip, fr, tree_all, idx_all, wverts):
    area, vfrac = visibility(W, tree_all, idx_all, tris, cam)
    cl = float(M.wcloth[wverts].mean()) if len(wverts) else 0.0
    cen = W[wverts].mean(0) if len(wverts) else W[M.tris[tris[0]]].mean(0)
    ev = {"clip": clip, "frame": fr, "kind": kind, "a": M.label(a), "b": M.label(b) if b is not None else "",
          "a_cls": M.comp_cls[a], "b_cls": M.comp_cls[b] if b is not None else "", "a_comp": int(a),
          "b_comp": int(b) if b is not None else -1, "region": M.region(wverts) if len(wverts) else "?",
          "vis_px": round(area, 1), "vis_frac": round(vfrac, 3), "cloth": round(cl, 2),
          "centroid": [round(float(x), 4) for x in cen], "ntris": int(len(tris))}
    ev.update(extra)
    ev["_tris"] = tris[:6000]
    return ev

RR_TOP, RR_FRAMES = [], {}
if RERENDER:
    RR_TOP = json.load(open(RERENDER))["top"]
    for e in RR_TOP:
        RR_FRAMES.setdefault(e["clip"], set()).add(e["frame"])
    clips = sorted(RR_FRAMES)
for ci, clip in enumerate([] if MIRROR_ONLY else clips):
    L.clear_clip(arm)
    r = L.set_clip(arm, clip)
    if r is None:
        continue
    act, f0, f1 = r
    frames = sorted(RR_FRAMES[clip]) if RERENDER else L.sample_frames(f0, f1, NFR)
    stats = {"frames": frames, "poke": 0, "contact": 0, "float": 0, "stretch": 0, "crush": 0, "spike": 0, "worst_px": 0.0}
    for fr in frames:
        sc.frame_set(fr)
        W = M.wpos(M.posed())
        shift = L.pelvis_shift(arm)
        loc, tgt = gameplay_cam(shift)
        cam.location = loc
        cam.rotation_euler = (tgt - loc).to_track_quat("-Z", "Y").to_euler()
        bpy.context.view_layer.update()
        tree, idx = M.bvh(W, coll_tri)
        fr_events = []
        # ---- poke: layer inversions
        s, lat, n = plane_side(W, ties_v, ties_t, ties_b)
        depth = -SG0 * s
        cand = np.nonzero((depth > TOL) & (lat < np.maximum(0.05, ties_d)))[0]
        conf = []
        for k in cand:
            v = ties_v[k]
            d = Vector(-np.sign(s[k]) * n[k])
            start, gone = Vector(W[v]), 0.0
            ok = False
            for _ in range(4):
                hit = tree.ray_cast(start, d, depth[k] + 0.005 - gone)
                if hit[0] is None:
                    break
                gone += hit[3]
                if PIECE[M.tri_comp[idx[hit[2]]]] == tie_outer[k]:
                    ok = True
                    break
                start = hit[0] + d * 1e-4
            if ok:
                conf.append(k)
        conf = np.array(conf, np.int64)
        if len(conf):
            pairs = tie_inner[conf] * M.ncomp + tie_outer[conf]
            for pk in np.unique(pairs):
                ks = conf[pairs == pk]
                a, b = divmod(int(pk), M.ncomp)
                vs = ties_v[ks]
                tr = tris_of_verts(vs)
                tr = tr[M.tri_comp[tr] == M.vcomp[vs[0]]] if len(tr) else tr
                fr_events.append(("poke", a, b, tr, {"depth_mm": round(float(depth[ks].max() * 1000), 1),
                                  "nverts": int(len(ks))}, vs))
        # ---- float: a piece that hugs the surface under it at rest (within 1 cm) and lifts off it by over 1.5 cm,
        # still over the same spot: a band or applique floating off the arm or the garment it is sewn to
        gap = -depth - ties_d                      # how much further apart than at rest
        fl = np.nonzero(HUG & (gap > 0.015) & (lat < 0.03))[0]
        if len(fl):
            pairs = tie_outer[fl] * M.ncomp + tie_inner[fl]
            for pk in np.unique(pairs):
                ks = fl[pairs == pk]
                if len(ks) < 3:
                    continue
                a, b = divmod(int(pk), M.ncomp)          # a = the floating piece (outer), b = what it should sit on
                m_ = np.zeros(M.nw, bool); m_[ties_v[ks]] = True
                tr = np.unique(ties_t[ks])
                fr_events.append(("float", a, b, tr, {"gap_mm": round(float(gap[ks].max() * 1000), 1), "nverts": int(len(ks))},
                                  np.unique(M.tris[tr].reshape(-1))))
        # ---- contacts
        ov = overlaps(tree, idx)
        if len(ov):
            k = pair_key(ov)
            for pk in np.unique(k):
                sel = k == pk
                tot = M.contact_length(ov[sel], seglen)
                r0 = rest_len.get(int(pk), 0.0)
                if r0 > 0 and not (tot > 2 * r0 and tot - r0 > 0.05):
                    continue
                if tot < 0.004:
                    continue
                a, b = divmod(int(pk), M.ncomp)
                tr = np.unique(ov[sel].reshape(-1))
                fr_events.append(("contact", a, b, tr, {"length_cm": round(tot * 100, 1), "rest_cm": round(r0 * 100, 1),
                                  "npairs": int(sel.sum())}, np.unique(M.tris[tr].reshape(-1))))
        # ---- stretch / crush
        Lp = np.linalg.norm(W[E[:, 0]] - W[E[:, 1]], axis=1)
        ratio = np.where(E_OK, Lp / np.maximum(L0, 1e-9), 1.0)
        for kind, bad in (("stretch", ratio > 1.6), ("crush", ratio < 0.5)):
            be = np.nonzero(bad)[0]
            if not len(be):
                continue
            comps = PIECE[M.vcomp[E[be, 0]]]
            for c in np.unique(comps):
                e_ = be[comps == c]
                vs = np.unique(E[e_].reshape(-1))
                m_ = np.zeros(M.nw, bool); m_[vs] = True
                tr = np.nonzero((m_[M.tris].sum(1) >= 2) & ~M.degen)[0]
                worst = ratio[e_].max() if kind == "stretch" else ratio[e_].min()
                fr_events.append((kind, int(c), None, tr, {"ratio": round(float(worst), 2), "nedges": int(len(e_))}, vs))
        # ---- spikes
        disp = W - W0
        nb = np.zeros_like(disp)
        np.add.at(nb, E[:, 0], disp[E[:, 1]]); np.add.at(nb, E[:, 1], disp[E[:, 0]])
        nb /= np.maximum(deg, 1)[:, None]
        dev = np.linalg.norm(disp - nb, axis=1)
        sp = np.nonzero((dev > np.maximum(0.03, 6 * meanL)) & coll_vert & (deg > 0))[0]
        if len(sp):
            comps = PIECE[M.vcomp[sp]]
            for c in np.unique(comps):
                vs = sp[comps == c]
                fr_events.append(("spike", int(c), None, tris_of_verts(vs), {"dev_cm": round(float(dev[vs].max() * 100), 1),
                                  "nverts": int(len(vs))}, vs))
        if fr_events:
            GEOM["n"], GEOM["area"] = M.tri_geom(W)
            tree_all, idx_all = tree, idx
            for kind, a, b, tr, extra, vs in fr_events:
                ev = describe(kind, a, b, W, np.asarray(tr, np.int64), extra, clip, fr, tree_all, idx_all, np.asarray(vs, np.int64))
                events.append(ev)
                stats[kind] += 1
                stats["worst_px"] = max(stats["worst_px"], ev["vis_px"])
    per_clip[clip] = stats
    log("clip %d/%d %-34s frames %d  poke %d contact %d float %d stretch %d crush %d spike %d  worst %.0f px" % (
        ci + 1, len(clips), clip, len(frames), stats["poke"], stats["contact"], stats["float"], stats["stretch"], stats["crush"],
        stats["spike"], stats["worst_px"]))

# ----------------------------------------------------------------------------------------------- top offenders
def score(e):
    w = {"poke": 1.0, "contact": 1.0, "float": 1.0, "spike": 3.0, "stretch": 0.6, "crush": 0.4}[e["kind"]]
    return e["vis_px"] * w

events.sort(key=score, reverse=True)
# the top offenders: per detector, the most visible piece pairs (each pair at most twice, from different clips)
top = []
for kind in (("poke", "contact", "float", "spike", "stretch", "crush") if TOP > 0 else ()):
    seen, n = {}, 0
    for e in events:
        if e["kind"] != kind or e["vis_px"] <= 0:
            continue
        k = (e["a_comp"], e["b_comp"])
        if k in seen and (len(seen[k]) >= 2 or e["clip"] in seen[k]):
            continue
        seen.setdefault(k, []).append(e["clip"])
        top.append(e)
        n += 1
        if n >= TOP:
            break

if RERENDER:
    found = {(e["kind"], e["a_comp"], e["b_comp"], e["clip"], e["frame"]): e for e in events}
    top = []
    for e in RR_TOP:
        n = found.get((e["kind"], e["a_comp"], e["b_comp"], e["clip"], e["frame"]))
        if n is not None:
            e = dict(e); e["_tris"] = n["_tris"]; e["centroid"] = n["centroid"]
            top.append(e)
    log("rerender: %d of %d top events found again" % (len(top), len(RR_TOP)))
L.workbench(sc, "TEXTURE", res=RES)
sc.render.resolution_x, sc.render.resolution_y = RES, int(RES * 1.5)

def render_event(e, rank, highlight=True, prefix=""):
    """gameplay view (highlight drawn over everything, so it can be found), then a close-up 0.7 m from the event on
    the camera's side: plain, and with the triangles in red in place (pushed 1.5 mm out, depth-tested)."""
    L.clear_clip(arm)
    L.set_clip(arm, e["clip"])
    sc.frame_set(e["frame"])
    W = M.wpos(M.posed())
    shift = L.pelvis_shift(arm)
    tr = e.get("_tris")
    base = os.path.join(OUT, "top", "%s%02d_%s_f%d" % (prefix, rank, e["clip"], e["frame"]))
    hl = L.highlight_object("qa_hl", W, M.tris[np.asarray(tr)]) if highlight and tr is not None and len(tr) else None
    if hl:
        hl.show_in_front = True
    L.place_camera(sc, "gameplay", lo_b, hi_b, shift, res=RES)
    sc.render.filepath = base + "_gameplay.png"
    bpy.ops.render.render(write_still=True)
    cam = sc.camera
    if tr is not None and len(tr):
        tree_v, idx_v = M.bvh(W)
        cl, ok = L.aim_closeup(cam, W, M.tris, np.asarray(tr), tree_v, idx_v,
                               prefer=(cam.location - Vector(e["centroid"])), dist=0.6, radius=0.08)
        if hl:
            bpy.data.objects.remove(hl)
            hl = L.highlight_object("qa_hl", W, M.tris[cl])
    else:
        c = Vector(e["centroid"])
        dirv = (cam.location - c).normalized()
        cam.data.type = "PERSP"; cam.data.lens = 50
        cam.location = c + dirv * 0.6
        cam.rotation_euler = (c - cam.location).to_track_quat("-Z", "Y").to_euler()
    sc.render.resolution_x, sc.render.resolution_y = RES, RES
    if hl:
        hl.hide_render = True
    sc.render.filepath = base + "_close.png"
    bpy.ops.render.render(write_still=True)
    if hl:
        hl.hide_render = False
        hl.show_in_front = False
        sc.render.filepath = base + "_close_hl.png"
        bpy.ops.render.render(write_still=True)
        bpy.data.objects.remove(hl)
    return base

if MIRROR:
    other = json.load(open(MIRROR))
    for rank, e in enumerate(other.get("top", []), 1):
        if e["clip"] in bpy.data.actions:
            e2 = dict(e); e2["_tris"] = None
            render_event(e2, rank, highlight=False, prefix="mirror_")
for rank, e in enumerate(top, 1):
    e["image"] = os.path.basename(render_event(e, rank))
    log("TOP %2d %-8s %-28s f%-3d %8.0f px  %s | %s" % (rank, e["kind"], e["clip"], e["frame"], e["vis_px"], e["a"], e["b"]))

if MIRROR_ONLY or RERENDER:
    log("MOTION_DONE", WHO, "renders only")
    sys.exit(0)
for e in events:
    e.pop("_tris", None)
# every event counts in the per-clip numbers; only the ones the gameplay camera sees are listed one by one
hidden = sum(1 for e in events if e["vis_px"] <= 0 and e["kind"] != "spike")
events = [e for e in events if e["vis_px"] > 0 or e["kind"] == "spike"]
json.dump({"who": WHO, "model": SRC, "clips": per_clip, "missing_clips": missing, "events": events, "top": top,
           "hidden_events": hidden,
           "rest_contacts": {"tri_pairs": int(len(rest_ov)), "piece_pairs": len(rest_len)},
           "ties": int(len(ties_v)), "components": int(M.ncomp), "back_shells": len(M.shell_of),
           "named_components": len(M.piece_name),
           "pieces": {int(c): M.label(c) for c in set([e["a_comp"] for e in events] + [e["b_comp"] for e in events if e["b_comp"] >= 0])}},
          open(os.path.join(OUT, "motion.json"), "w"), indent=1, default=lambda x: x.item() if hasattr(x, "item") else str(x))
with open(os.path.join(OUT, "clips.tsv"), "w") as f:
    f.write("clip\tframes\tpoke\tcontact\tstretch\tcrush\tspike\tworst_px\n")
    for c, s in per_clip.items():
        f.write("%s\t%d\t%d\t%d\t%d\t%d\t%d\t%.0f\n" % (c, len(s["frames"]), s["poke"], s["contact"], s["stretch"], s["crush"], s["spike"], s["worst_px"]))
log("MOTION_DONE", WHO, "events", len(events), "top", len(top))
