"""QA renders: fixed-camera clip sequences, and diagnostic view styles, framed from the skeleton so ours and Valve's
land on the same pixels.

  blender --background --factory-startup --python qa_render.py -- <model.blend> <out_dir> --hero-cfg <hero.json>
          --mode sequences --clips a,b,c [--frames 8] [--cams gameplay,front,side,below] [--res 320]
  blender ... -- <model.blend> <out_dir> --hero-cfg <hero.json> --mode views [--poses bind,weapon_stand_idle:0]
          [--cams front,side,back,gameplay] [--styles ...] [--pieces pieces.json] [--res 480]

sequences: <out>/<clip>/<cam>_f<frame>.png, Workbench with the model's textures, the camera following the pelvis.
views (RGBA, transparent background, so qa_diff.py can compare silhouettes): <out>/<pose>/<style>_<cam>.png
  lookdev    EEVEE, the model's own materials, three area lights
  xray       translucent skin with the skeleton as geometry (core white, left cyan, right magenta, cloth amber)
  wire       every triangle edge (EEVEE wireframe node, 1 px)
  facing     front faces grey, BACK faces red, backface culling off: a red patch the camera can see is a hole
             in game (open sheet seen from behind, or a flipped face)
  normals    world-space shading normal as colour
  uv         a UV checker on every material (palette pieces show one flat swatch)
  matid      a fixed colour per material
  texel      texture px per metre: blue = a quarter of 1024 px/m, green = 1024, red = 4x; grey = palette swatch
  motion     the motion class: skin tan, Valve kit slate, rigid outfit blue, firm orange, proxy magenta, glint white,
             and every vertex blended toward cyan by its weight on cloth bones (what the cloth solver moves)
  overdraw   front-facing surfaces crossed per pixel (ray cast through the whole model; back faces are culled by the
             GPU so they do not count): 1 blue .. 16+ red. The depth complexity the GPU rasterises before depth
             rejection; stats per camera in views.json
"""
import bpy, sys, os, json, math, time, hashlib
import numpy as np
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import qa_lib as L
from mathutils import Vector

O = L.Opts(L.args())
SRC, OUT = O.a[0], O.a[1]
cfg = json.load(open(O.get("--hero-cfg")))
MODE = O.get("--mode", "views")
RES = int(O.get("--res", "320" if MODE == "sequences" else "480")) // (2 if L.DRAFT and not O.has("--res") else 1)
os.makedirs(OUT, exist_ok=True)
T0 = time.time()

sc, arm, meshes = L.open_model(SRC)
lo_b, hi_b = L.cam_bounds(arm)

def render(path, transparent=False):
    sc.render.film_transparent = transparent
    sc.render.image_settings.color_mode = "RGBA" if transparent else "RGB"
    sc.render.filepath = path
    bpy.ops.render.render(write_still=True)

def pose(spec):
    L.clear_clip(arm)
    if spec in ("bind", "rest"):
        sc.frame_set(0)
        return L.pelvis_shift(arm) * 0
    clip, fr = spec.split(":")
    if L.set_clip(arm, clip) is None:
        return None
    sc.frame_set(int(fr))
    bpy.context.view_layer.update()
    return L.pelvis_shift(arm)

# ------------------------------------------------------------------------------------------------ sequences
if MODE == "sequences":
    cams = O.get("--cams", "gameplay,front,side,below").split(",")
    n = int(O.get("--frames", "8"))
    L.workbench(sc, "TEXTURE", res=RES)
    done = {}
    for clip in O.get("--clips", "").split(","):
        r = L.set_clip(arm, clip)
        if r is None:
            print("[seq] no clip", clip)
            continue
        _, f0, f1 = r
        frames = L.sample_frames(f0, f1, n)
        os.makedirs(os.path.join(OUT, clip), exist_ok=True)
        for fr in frames:
            sc.frame_set(fr)
            shift = L.pelvis_shift(arm)
            for cv in cams:
                L.place_camera(sc, cv, lo_b, hi_b, shift, res=RES)
                render(os.path.join(OUT, clip, "%s_f%03d.png" % (cv, fr)))
        done[clip] = frames
        print("[seq] %-30s %d frames x %d cameras (%.0fs)" % (clip, len(frames), len(cams), time.time() - T0), flush=True)
    json.dump({"clips": done, "cams": cams, "res": RES}, open(os.path.join(OUT, "sequences.json"), "w"), indent=1)
    print("SEQ_DONE", len(done))
    sys.exit(0)

# ------------------------------------------------------------------------------------------------ views
M = L.Model(sc, arm, meshes, cfg)
M.name_pieces(O.get("--pieces"))
mech_of = {}
if O.get("--pieces") and os.path.exists(O.get("--pieces")):
    mech_of = {p["name"]: p.get("mech", "") for p in json.load(open(O.get("--pieces")))["pieces"]}
cams = O.get("--cams", "front,side,back,gameplay").split(",")
poses = O.get("--poses", "bind,weapon_stand_idle:0").split(",")
styles = O.get("--styles", "lookdev,wire,facing,uv,normals,texel,motion,matid,xray,overdraw").split(",")
orig_mats = {o.name: [s.material for s in o.material_slots] for o in meshes}
for o in sc.objects:
    if o.type in ("LIGHT",):
        bpy.data.objects.remove(o)

def eevee():
    sc.render.engine = "BLENDER_EEVEE" if "BLENDER_EEVEE" in bpy.types.RenderSettings.bl_rna.properties["engine"].enum_items.keys() else "BLENDER_EEVEE_NEXT"
    try:
        sc.eevee.taa_render_samples = 4 if L.DRAFT else 16
    except Exception:
        pass
    sc.view_settings.view_transform = "Standard"

def node_mat(name, build):
    m = bpy.data.materials.get(name)
    if m:
        return m
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    nt = m.node_tree
    for nd in list(nt.nodes):
        nt.nodes.remove(nd)
    out = nt.nodes.new("ShaderNodeOutputMaterial")
    build(nt, out)
    m.use_backface_culling = False
    return m

def emit(nt, col_socket_or_rgba, out, strength=1.0):
    e = nt.nodes.new("ShaderNodeEmission")
    if isinstance(col_socket_or_rgba, tuple):
        e.inputs["Color"].default_value = col_socket_or_rgba
    else:
        nt.links.new(col_socket_or_rgba, e.inputs["Color"])
    e.inputs["Strength"].default_value = strength
    nt.links.new(e.outputs[0], out.inputs["Surface"])

def build_facing(nt, out):
    g = nt.nodes.new("ShaderNodeNewGeometry")
    mix = nt.nodes.new("ShaderNodeMix"); mix.data_type = "RGBA"
    mix.inputs["A"].default_value = (0.30, 0.31, 0.34, 1); mix.inputs["B"].default_value = (1.0, 0.03, 0.03, 1)
    nt.links.new(g.outputs["Backfacing"], mix.inputs["Factor"])
    lw = nt.nodes.new("ShaderNodeLayerWeight"); lw.inputs["Blend"].default_value = 0.5
    sh = nt.nodes.new("ShaderNodeMix"); sh.data_type = "RGBA"; sh.blend_type = "MULTIPLY"
    sh.inputs["Factor"].default_value = 0.7
    nt.links.new(mix.outputs["Result"], sh.inputs["A"]); nt.links.new(lw.outputs["Facing"], sh.inputs["B"])
    emit(nt, sh.outputs["Result"], out)

def build_wire(nt, out):
    w = nt.nodes.new("ShaderNodeWireframe"); w.use_pixel_size = True; w.inputs["Size"].default_value = 1.0
    mix = nt.nodes.new("ShaderNodeMix"); mix.data_type = "RGBA"
    mix.inputs["A"].default_value = (0.08, 0.085, 0.1, 1); mix.inputs["B"].default_value = (0.85, 0.9, 1.0, 1)
    nt.links.new(w.outputs[0], mix.inputs["Factor"])
    emit(nt, mix.outputs["Result"], out)

def build_uv(nt, out):
    img = bpy.data.images.get("qa_uvgrid") or bpy.data.images.new("qa_uvgrid", 1024, 1024)
    img.generated_type = "COLOR_GRID"
    t = nt.nodes.new("ShaderNodeTexImage"); t.image = img; t.interpolation = "Closest"
    emit(nt, t.outputs["Color"], out)

def lights_on(shift):
    for o in [o for o in sc.objects if o.type == "LIGHT"]:
        bpy.data.objects.remove(o)
    c = Vector(((lo_b + hi_b) / 2).tolist()) + shift
    H = float(hi_b[2] - lo_b[2])
    for nm, off, en, col in (("key", (-1.2, -1.4, 0.9), 140, (1, .96, .9)), ("fill", (1.4, -1.0, 0.2), 45, (.85, .9, 1)),
                             ("rim", (0.4, 1.6, 1.0), 110, (1, 1, 1)), ("bk", (-1.0, 1.5, 1.0), 120, (1, .97, .92))):
        d = bpy.data.lights.new(nm, "AREA"); d.energy = en * H * H; d.color = col; d.size = H
        ob = bpy.data.objects.new(nm, d); sc.collection.objects.link(ob)
        ob.location = c + Vector(off) * H
        ob.rotation_euler = (c - ob.location).to_track_quat("-Z", "Y").to_euler()
    w = sc.world or bpy.data.worlds.new("w"); sc.world = w; w.use_nodes = True
    bg = w.node_tree.nodes.get("Background")
    bg.inputs[0].default_value = (0.045, 0.045, 0.05, 1); bg.inputs[1].default_value = 0.6

def lights_off():
    for o in [o for o in sc.objects if o.type == "LIGHT"]:
        bpy.data.objects.remove(o)

def mat_color(name):
    h = hashlib.md5(name.encode()).digest()
    return (0.2 + 0.8 * h[0] / 255, 0.2 + 0.8 * h[1] / 255, 0.2 + 0.8 * h[2] / 255, 1)

MECH_COL = {"skin": (0.80, 0.58, 0.45), "gear": (0.30, 0.34, 0.42), "rigid": (0.12, 0.25, 0.80), "outfit": (0.12, 0.25, 0.80),
            "firm": (1.0, 0.62, 0.0), "proxy": (0.90, 0.05, 0.45), "chain": (0.12, 0.25, 0.80), "cloth": (0.12, 0.25, 0.80),
            "glint": (1, 1, 1), "fx": (1, 1, 1), "body": (0.80, 0.58, 0.45)}

def overdraw(W, cam, path, res=120 if L.DRAFT else 240):
    """Surfaces crossed per pixel: every triangle that renders, alpha-tested ones included."""
    tree, idx = M.bvh(W)
    w, h = res, int(res * 1.5)
    sc.render.resolution_x, sc.render.resolution_y = w, h
    fr = [cam.matrix_world @ v for v in cam.data.view_frame(scene=sc)]   # tr, br, bl, tl
    tr, br, bl, tl = fr
    cnt = np.zeros((h, w), np.int32)
    ortho = cam.data.type == "ORTHO"
    fwd = (cam.matrix_world.to_3x3() @ Vector((0, 0, -1))).normalized()
    for j in range(h):
        v = (j + 0.5) / h
        left = tl.lerp(bl, v); right = tr.lerp(br, v)
        for i in range(w):
            p = left.lerp(right, (i + 0.5) / w)
            if ortho:
                o_, d = p - fwd * 5.0, fwd
            else:
                o_, d = cam.location.copy(), (p - cam.location).normalized()
            n = 0
            for _ in range(64):
                hit = tree.ray_cast(o_, d, 50.0)
                if hit[0] is None:
                    break
                if hit[1].dot(d) < 0:          # front-facing: rasterised and shaded; back faces are culled
                    n += 1
                o_ = hit[0] + d * 2e-4
            cnt[j, i] = n
    col = L.heat((cnt - 1) / 15.0)
    rgba = np.concatenate([col, (cnt > 0)[..., None].astype(float)], 2)[::-1]
    img = bpy.data.images.new("qa_od", w, h, alpha=True)
    img.pixels.foreach_set(rgba.astype(np.float32).reshape(-1))
    img.filepath_raw = path; img.file_format = "PNG"; img.save()
    bpy.data.images.remove(img)
    cov = cnt[cnt > 0]
    return {"pixels": int(cov.size), "mean_layers": round(float(cov.mean()), 2) if cov.size else 0,
            "p95_layers": int(np.percentile(cov, 95)) if cov.size else 0, "max_layers": int(cnt.max()),
            "px_4plus": int((cnt >= 4).sum()), "px_8plus": int((cnt >= 8).sum())}

od_stats = {}
dens, palette, texres = L.texel_density(M)
for ps in poses:
    shift = pose(ps)
    if shift is None:
        print("[views] no pose", ps)
        continue
    W = M.wpos(M.posed())
    n_t, _ = M.tri_geom(W)
    tag = ps.replace(":", "_f")
    od = os.path.join(OUT, tag)
    os.makedirs(od, exist_ok=True)
    for st in styles:
        bones = None
        for o in meshes:                     # glint and sparkle cards only in the lookdev and the overdraw
            o.hide_render = st not in ("lookdev", "overdraw") and L.mesh_key(o) in cfg["classes"]["mesh"] and cfg["classes"]["mesh"][L.mesh_key(o)] == "fx"
        sc.view_layers[0].material_override = None
        for o in meshes:
            for k, s in enumerate(o.material_slots):
                s.material = orig_mats[o.name][k]
        if st == "lookdev":
            eevee(); lights_on(shift)
        elif st in ("facing", "wire", "uv"):
            eevee(); lights_off()
            sc.world.node_tree.nodes["Background"].inputs[1].default_value = 0.0
            sc.view_layers[0].material_override = node_mat("qa_" + st, {"facing": build_facing, "wire": build_wire, "uv": build_uv}[st])
        elif st in ("normals", "texel", "motion"):
            if st == "normals":
                col = n_t * 0.5 + 0.5
            elif st == "texel":
                x = (np.log2(np.maximum(dens, 1e-3) / 1024.0) + 2) / 4
                col = L.heat(x)
                col[palette | (dens <= 0)] = (0.45, 0.45, 0.45)
            else:
                col = np.zeros((len(M.tris), 3))
                for c in range(M.ncomp):
                    nm = M.piece_name.get(int(M.piece[c]))
                    key = mech_of.get(nm, "") if nm else ""
                    key = key or M.comp_cls[c]
                    col[M.tri_comp == c] = MECH_COL.get(key, (0.5, 0.5, 0.5))
                wc = M.wcloth[M.tris].mean(1)[:, None]
                col = col * (1 - wc) + np.array([0.0, 0.95, 0.95]) * wc
            L.set_corner_colors(M, np.concatenate([col, np.ones((len(col), 1))], 1).astype(np.float32))
            L.workbench(sc, "VERTEX", res=RES)
            sc.display.shading.light = "FLAT" if st != "normals" else "FLAT"
        elif st == "matid":
            for o in meshes:
                for s in o.material_slots:
                    if s.material:
                        s.material.diffuse_color = mat_color(s.material.name)
            L.workbench(sc, "MATERIAL", res=RES)
        elif st == "xray":
            for o in meshes:
                for s in o.material_slots:
                    if s.material:
                        s.material.diffuse_color = (0.55, 0.6, 0.7, 1)
            bones = L.bones_object(arm, radius=0.006 * float(hi_b[2] - lo_b[2]) / 2.4)
            L.workbench(sc, "MATERIAL", res=RES, xray=True)
            sc.display.shading.light = "FLAT"
        for cv in cams:
            cam = L.place_camera(sc, cv, lo_b, hi_b, shift, res=RES)
            path = os.path.join(od, "%s_%s.png" % (st, cv))
            if st == "overdraw":
                od_stats["%s/%s" % (tag, cv)] = overdraw(W, cam, path)
            else:
                render(path, transparent=True)
        if bones:
            bpy.data.objects.remove(bones)
        print("[views] %s %-8s (%.0fs)" % (tag, st, time.time() - T0), flush=True)
json.dump({"overdraw": od_stats, "texres": texres, "styles": styles, "cams": cams, "poses": poses},
          open(os.path.join(OUT, "views.json"), "w"), indent=1)
print("VIEWS_DONE")
