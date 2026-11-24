"""QA holes: back faces the gameplay camera sees, clip by clip, frame by frame.

  blender --background --factory-startup --python qa_holes.py -- <model.blend> <out_dir> --hero-cfg <hero.json>
          [--who ours|valve] [--clips a,b|all] [--frames 10] [--grid 160] [--cams gameplay,front,below]
          [--pieces pieces.json] [--top 10] [--res 480]

The game culls back faces. Wherever a camera's first hit is the back of a triangle, the game draws what is behind
it instead: a hole. That is how an open sheet shows its missing side, how a flipped face vanishes, and how a body
with its covered faces removed shows its inside the moment the garment over the cut lifts away. Detected with a
ray grid (--grid px wide, 2:3) per camera on every sampled frame; per frame the hole area (px at 720x1080) by
piece. The worst frames are rendered: the model's textures, and the facing style (back faces red) from the same
camera. Linings: two-faced sheets (a back shell in another colour than its front: navy outside, sky inside) have an
inner face; where a ray's first hit is that inner face, drawn, the camera sees the lining colour: at a seam, an edge,
or a tail that turned over. Recorded beside the holes (kind "lining"). Writes <out>/holes.json, <out>/top/<rank>_<clip>_f<frame>_<cam>_{tex,facing}.png.
"""
import bpy, sys, os, json, time
import numpy as np
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import qa_lib as L
from mathutils import Vector

O = L.Opts(L.args())
SRC, OUT = O.a[0], O.a[1]
cfg = json.load(open(O.get("--hero-cfg")))
WHO = O.get("--who", "ours")
NFR, GRID, TOP, RES = int(O.get("--frames", "10")), int(O.get("--grid", "160")), int(O.get("--top", "10")), int(O.get("--res", "480"))   # draft keeps the grid: halving it saved no time (measured) and moved the hole areas
CAMS = O.get("--cams", "gameplay,front,below").split(",")
os.makedirs(os.path.join(OUT, "top"), exist_ok=True)
T0 = time.time()

sc, arm, meshes = L.open_model(SRC)
M = L.Model(sc, arm, meshes, cfg)
M.name_pieces(O.get("--pieces"))
lo_b, hi_b = L.cam_bounds(arm)
fx = M.comp_cls == "fx"
# only shells of another colour than their outside (two-faced materials) count as a visible lining: an open sheet's
# plain back face in the same colour is what the export added it for
TWO = L.two_faced_shells(M)
SHELLS = set(TWO)
clip_arg = O.get("--clips", "all")
clips = sorted(a.name for a in bpy.data.actions if not a.name.startswith("seq.")) if clip_arg == "all" else \
    [c for c in clip_arg.split(",") if c in bpy.data.actions]
sc.render.resolution_x, sc.render.resolution_y = 720, 1080
cam = L.place_camera(sc, "gameplay", lo_b, hi_b, res=720)
sc.render.resolution_x, sc.render.resolution_y = 720, 1080

def grid_rays(cam):
    fr = [cam.matrix_world @ v for v in cam.data.view_frame(scene=sc)]
    tr, br, bl, tl = fr
    w, h = GRID, int(GRID * 1.5)
    ortho = cam.data.type == "ORTHO"
    fwd = (cam.matrix_world.to_3x3() @ Vector((0, 0, -1))).normalized()
    out = []
    for j in range(h):
        v = (j + 0.5) / h
        left, right = tl.lerp(bl, v), tr.lerp(br, v)
        for i in range(w):
            p = left.lerp(right, (i + 0.5) / w)
            out.append((p - fwd * 5.0, fwd) if ortho else (cam.location.copy(), (p - cam.location).normalized()))
    return out, (720.0 / w) ** 2

def holes(W, mask_for=None):
    """Per camera: hole px and lining px by piece. mask_for=(cam, kind, piece) also returns that event's pixels
    on the ray grid (row-major from the top), for the overlay on the renders."""
    tree, idx = M.bvh(W, ~fx[M.tri_comp])
    res, mask = {}, []
    for cv in ([mask_for[0]] if mask_for else CAMS):
        L.place_camera(sc, cv, lo_b, hi_b, L.pelvis_shift(arm), res=720)
        sc.render.resolution_x, sc.render.resolution_y = 720, 1080
        rays, pxw = grid_rays(sc.camera)
        per, lin = {}, {}
        for ri, (o_, d) in enumerate(rays):
            hit = tree.ray_cast(o_, d, 50.0)
            if hit[0] is None:
                continue
            raw = int(M.tri_comp[idx[hit[2]]])
            c = int(M.piece[raw])
            if hit[1].dot(d) > 0.02:
                per[c] = per.get(c, 0) + pxw
                if mask_for and mask_for[1] == "hole" and c == mask_for[2]:
                    mask.append(ri)
            if hit[1].dot(d) < -0.02 and raw in SHELLS:
                # the drawn inner face of a two-faced sheet: the lining colour, seen from where the camera is
                lin[c] = lin.get(c, 0) + pxw
                if mask_for and mask_for[1] == "lining" and c == mask_for[2]:
                    mask.append(ri)
        res[cv] = per
        res[cv + ":lining"] = lin
    return (res, mask) if mask_for else res

frames_out, events = {}, []
L.clear_clip(arm)
sc.frame_set(0)
rest = holes(M.wpos(M.posed()))
# the bind pose is a row of its own (rest_px 0), so what is wrong standing still ranks with what motion reveals
for cv, per in rest.items():
    for c, px in per.items():
        events.append({"clip": "bind", "frame": 0, "cam": cv.split(":")[0], "kind": "lining" if cv.endswith(":lining") else "hole",
                       "comp": c, "label": M.label(c), "cls": M.comp_cls[c], "px": round(px, 1), "rest_px": 0.0})
for ci, clip in enumerate(clips):
    L.clear_clip(arm)
    r = L.set_clip(arm, clip)
    if r is None:
        continue
    _, f0, f1 = r
    worst = 0
    for fr in L.sample_frames(f0, f1, NFR):
        sc.frame_set(fr)
        h = holes(M.wpos(M.posed()))
        for cv, per in h.items():
            for c, px in per.items():
                events.append({"clip": clip, "frame": fr, "cam": cv.split(":")[0], "kind": "lining" if cv.endswith(":lining") else "hole",
                               "comp": c, "label": M.label(c), "cls": M.comp_cls[c],
                               "px": round(px, 1), "rest_px": round(rest.get(cv, {}).get(c, 0), 1)})
                worst = max(worst, px)
    print("[holes] %d/%d %-32s worst %.0f px (%.0fs)" % (ci + 1, len(clips), clip, worst, time.time() - T0), flush=True)

# the worst frames per piece (gameplay first): rendered with textures and in the facing style
events.sort(key=lambda e: (-(e["cam"] == "gameplay"), -e["px"]))
top, seen = [], set()
for e in sorted(events, key=lambda e: -(e["px"] - e["rest_px"]) * (1.5 if e["cam"] == "gameplay" else 1)):
    if (e["comp"], e["cam"], e["kind"]) in seen or e["px"] < 30:
        continue
    seen.add((e["comp"], e["cam"], e["kind"]))
    top.append(e)
    if len(top) >= 2 * TOP:
        break
def facing_mat():
    m = bpy.data.materials.new("qa_facing"); m.use_nodes = True; nt = m.node_tree
    for nd in list(nt.nodes):
        nt.nodes.remove(nd)
    out = nt.nodes.new("ShaderNodeOutputMaterial"); g = nt.nodes.new("ShaderNodeNewGeometry")
    mix = nt.nodes.new("ShaderNodeMix"); mix.data_type = "RGBA"
    mix.inputs["A"].default_value = (0.30, 0.31, 0.34, 1); mix.inputs["B"].default_value = (1.0, 0.03, 0.03, 1)
    nt.links.new(g.outputs["Backfacing"], mix.inputs["Factor"])
    e = nt.nodes.new("ShaderNodeEmission"); nt.links.new(mix.outputs["Result"], e.inputs["Color"])
    nt.links.new(e.outputs[0], out.inputs["Surface"])
    m.use_backface_culling = False
    return m
FM = facing_mat()
for o in meshes:
    if L.mesh_key(o) in cfg["classes"]["mesh"] and cfg["classes"]["mesh"][L.mesh_key(o)] == "fx":
        o.hide_render = True
for rank, e in enumerate(top, 1):
    L.clear_clip(arm)
    if e["clip"] != "bind":
        L.set_clip(arm, e["clip"])
    sc.frame_set(e["frame"])
    base = os.path.join(OUT, "top", "%02d_%s_f%d_%s" % (rank, e["clip"], e["frame"], e["cam"]))
    L.place_camera(sc, e["cam"], lo_b, hi_b, L.pelvis_shift(arm), res=RES)
    sc.view_layers[0].material_override = None
    L.workbench(sc, "TEXTURE", res=RES)
    sc.render.resolution_x, sc.render.resolution_y = RES, int(RES * 1.5)
    sc.render.filepath = base + "_tex.png"; bpy.ops.render.render(write_still=True)
    sc.render.engine = "BLENDER_EEVEE" if "BLENDER_EEVEE" in bpy.types.RenderSettings.bl_rna.properties["engine"].enum_items.keys() else "BLENDER_EEVEE_NEXT"
    sc.view_layers[0].material_override = FM
    sc.render.filepath = base + "_facing.png"; bpy.ops.render.render(write_still=True)
    sc.view_layers[0].material_override = None
    # the event's own pixels from the ray grid, as a red mask the report lays over the textured render
    _, mk = holes(M.wpos(M.posed()), (e["cam"], e["kind"], e["comp"]))
    gw, gh = GRID, int(GRID * 1.5)
    rgba = np.zeros((gh * gw, 4), np.float32)
    rgba[mk] = (1.0, 0.05, 0.05, 1.0)
    img = bpy.data.images.new("qa_mask", gw, gh, alpha=True)
    img.pixels.foreach_set(rgba.reshape(gh, gw, 4)[::-1].reshape(-1))
    img.filepath_raw = base + "_mask.png"; img.file_format = "PNG"; img.save()
    bpy.data.images.remove(img)
    e["image"] = os.path.basename(base)
    print("[holes] TOP %2d %-6s %-28s f%-3d %-8s %6.0f px (rest %4.0f)  %s" % (rank, e["kind"], e["clip"], e["frame"], e["cam"], e["px"], e["rest_px"], e["label"]), flush=True)
json.dump({"who": WHO, "cams": CAMS, "grid": GRID, "two_faced": {M.label(k): v for k, v in TWO.items()}, "rest": {cv: {M.label(c): round(px, 1) for c, px in per.items()} for cv, per in rest.items()},
           "events": events, "top": top}, open(os.path.join(OUT, "holes.json"), "w"), indent=1,
          default=lambda x: x.item() if hasattr(x, "item") else str(x))
print("HOLES_DONE", WHO, len(events), "events")
