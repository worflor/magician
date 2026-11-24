"""Render a hero's menu pose through the menu's own camera. Blender, headless.

  blender -b --factory-startup -P menu_pose_render.py -- <model.blend> <anim dir> <jobs.json> <out dir>

<anim dir> holds the mod's dmxanim.py (the keyvalues2 DMX reader both anim pipelines share).
jobs.json: [{"name": "select_ours", "dmx": "<kv2 dmx>", "frames": [0, 10, 29],
             "camera": {"pos": [x, y, z], "ang": [pitch, yaw, roll], "fov": 40},       # Source units / QAngle
             "prop": {"pos": [...], "ang": [...]} | null,                               # the model's placement
             "res": [w, h], "engine": "workbench" | "eevee", "bg": [r, g, b],
             "lights": [{"pos": [...], "ang": [...], "color": [r, g, b], "lumens": 445, "kind": "spot",
                         "cone": 40}],                                                 # Source space
             "props": [{"blend_obj": "<name in an appended .blend>", ...}] (unused yet)}]

The cameras are the ones the game uses (decompiled from the UI maps, Source space): `hero_select.vpk`'s
point_camera, and each hero prefab's (`maps/ui/hero_prefabs/<hero>.vpk`) with its prop_dynamic's transform.
Source to Blender is the glTF rip's mapping: B = (Sy, -Sx, Sz) * 0.0254. A QAngle (pitch, yaw, roll) is
Rz(yaw) Ry(pitch) Rx(roll), positive pitch looking down; a Source camera looks along its +X with +Z up.
The pose transfer is the anim pipelines' preview: each bone's change since bind, carried into Blender space.
Writes <out>/<name>_fNN.png.
"""
import bpy, sys, os, json, math
import numpy as np
from mathutils import Matrix, Vector

argv = sys.argv[sys.argv.index("--") + 1:]
blend, anim_dir, jobs_path, out_dir = argv[:4]
sys.path.insert(0, anim_dir)
import dmxanim as D

os.makedirs(out_dir, exist_ok=True)
bpy.ops.wm.open_mainfile(filepath=blend)
sc = bpy.context.scene
for o in list(sc.objects):
    if o.type in {"CAMERA", "LIGHT"}:
        bpy.data.objects.remove(o)
arm = next(o for o in sc.objects if o.type == "ARMATURE")
arm.animation_data_clear()
arm.data.pose_position = "POSE"
for o in sc.objects:
    for m in getattr(o, "modifiers", []):
        if m.type == "SUBSURF":
            m.show_render = False; m.show_viewport = False

S2B = np.array([[0, 1, 0], [-1, 0, 0], [0, 0, 1]], float)
INCH = 0.0254
C4 = np.eye(4); C4[:3, :3] = S2B * INCH
C4i = np.linalg.inv(C4)
armw = np.array(arm.matrix_world)
rest = {b.name: np.array(b.matrix_local) for b in arm.data.bones}


def mat(q, p):
    x, y, z, w = q / np.linalg.norm(q)
    R = np.array([[1 - 2 * (y * y + z * z), 2 * (x * y - z * w), 2 * (x * z + y * w)],
                  [2 * (x * y + z * w), 1 - 2 * (x * x + z * z), 2 * (y * z - x * w)],
                  [2 * (x * z - y * w), 2 * (y * z + x * w), 1 - 2 * (x * x + y * y)]])
    M = np.eye(4); M[:3, :3] = R; M[:3, 3] = p
    return M


def world(a, local):
    W = {}
    for b in a.order:
        L = local[b]
        p = a.parent.get(b)
        W[b] = W[p] @ L if p in W else L
    return W


def apply(a, t):
    bind = world(a, {b: mat(a.bind_rot[b], a.bind_pos[b]) for b in a.order})
    loc = {}
    for b in a.order:
        q = a.sample(b, t) if a.has(b) else a.bind_rot[b]
        p = a.sample(b, t, "position") if a.has(b, "position") else a.bind_pos[b]
        loc[b] = mat(np.asarray(q, float), np.asarray(p, float))
    cur = world(a, loc)
    target = {}
    for b in a.order:
        if b in rest:
            target[b] = np.linalg.inv(armw) @ (C4 @ (cur[b] @ np.linalg.inv(bind[b])) @ C4i) @ armw @ rest[b]
    for pb in arm.pose.bones:
        b = pb.name
        if b not in target:
            pb.matrix_basis = Matrix.Identity(4)
            continue
        par = pb.parent
        if par is not None and par.name in target:
            Rl = np.linalg.inv(rest[par.name]) @ rest[b]
            basis = np.linalg.inv(Rl) @ np.linalg.inv(target[par.name]) @ target[b]
        else:
            basis = np.linalg.inv(rest[b]) @ target[b]
        pb.matrix_basis = Matrix(basis.tolist())
    bpy.context.view_layer.update()


def qangle(p, y, r):
    p, y, r = (math.radians(v) for v in (p, y, r))
    Rz = np.array([[math.cos(y), -math.sin(y), 0], [math.sin(y), math.cos(y), 0], [0, 0, 1]])
    Ry = np.array([[math.cos(p), 0, math.sin(p)], [0, 1, 0], [-math.sin(p), 0, math.cos(p)]])
    Rx = np.array([[1, 0, 0], [0, math.cos(r), -math.sin(r)], [0, math.sin(r), math.cos(r)]])
    return Rz @ Ry @ Rx


def src_xf(pos, ang):
    M = np.eye(4); M[:3, :3] = qangle(*ang); M[:3, 3] = pos
    return M


# A Blender camera/light looks down its -Z with +Y up; a Source one along +X with +Z up.
CAM_AXES = np.array([[0, 0, -1], [-1, 0, 0], [0, 1, 0]], float)   # columns: blender X, Y, Z in Source-local terms


def to_blender(M_src, prop_inv):
    """A Source-space frame, seen from the model's own frame, as a Blender world matrix."""
    L = prop_inv @ M_src
    out = np.eye(4)
    out[:3, :3] = S2B @ L[:3, :3] @ CAM_AXES
    out[:3, 3] = S2B @ L[:3, 3] * INCH
    return Matrix((armw @ out).tolist())


cam_d = bpy.data.cameras.new("cam"); cam = bpy.data.objects.new("cam", cam_d)
sc.collection.objects.link(cam); sc.camera = cam
cam_d.sensor_fit = "HORIZONTAL"
cam_d.clip_start = 0.02; cam_d.clip_end = 400


def look(engine, bg, color_type="TEXTURE", wire=False):
    if engine == "eevee":
        sc.render.engine = "BLENDER_EEVEE_NEXT"
        sc.view_settings.view_transform = "AgX"
        try:
            sc.eevee.taa_render_samples = 64
            sc.eevee.use_shadows = True
        except Exception:
            pass
    else:
        sc.render.engine = "BLENDER_WORKBENCH"
        sc.display.shading.light = "STUDIO"
        sc.display.shading.color_type = color_type             # "TEXTURE", or "SINGLE" for clay
        sc.display.shading.show_xray = False
        try:
            sc.display.shading.show_wireframes = wire
        except Exception:
            pass
        sc.display.shading.show_shadows = True
        sc.display.shading.shadow_intensity = 0.35
        sc.display.shading.show_cavity = True
        sc.display.shading.cavity_type = "WORLD"
        sc.display.shading.background_type = "VIEWPORT"
        sc.display.shading.background_color = tuple(bg)
        sc.view_settings.view_transform = "Standard"
    w = sc.world or bpy.data.worlds.new("w"); sc.world = w; w.use_nodes = True
    w.node_tree.nodes["Background"].inputs[0].default_value = (*bg, 1)
    w.node_tree.nodes["Background"].inputs[1].default_value = 0.35
    for m in bpy.data.materials:
        if m.use_nodes:
            bsdf = next((n for n in m.node_tree.nodes if n.type == "BSDF_PRINCIPLED"), None)
            if bsdf is not None and not bsdf.inputs["Base Color"].is_linked:
                m.diffuse_color = bsdf.inputs["Base Color"].default_value
            elif bsdf is not None:
                src = bsdf.inputs["Base Color"].links[0].from_node
                while src is not None and src.type != "TEX_IMAGE" and src.inputs and any(i.is_linked for i in src.inputs):
                    src = next(i.links[0].from_node for i in src.inputs if i.is_linked)
                if src is not None and src.type == "TEX_IMAGE":
                    m.node_tree.nodes.active = src


made_lights = []
for job in json.load(open(jobs_path)):
    a = D.AnimDMX(job["dmx"])
    prop = job.get("prop")
    P = src_xf(prop["pos"], prop["ang"]) if prop else np.eye(4)
    Pinv = np.linalg.inv(P)
    c = job["camera"]
    cam.matrix_world = to_blender(src_xf(c["pos"], c["ang"]), Pinv)
    cam_d.angle = math.radians(c["fov"])
    w, h = job.get("res", [640, 640])
    sc.render.resolution_x, sc.render.resolution_y = w, h
    sc.render.film_transparent = False
    look(job.get("engine", "workbench"), job.get("bg", [0.11, 0.105, 0.12]), job.get("color_type", "TEXTURE"), job.get("wire", False))
    for lo in made_lights:
        bpy.data.objects.remove(lo)
    made_lights = []
    for i, L in enumerate(job.get("lights", [])):
        ld = bpy.data.lights.new("L%d" % i, "SPOT" if L.get("kind", "spot") == "spot" else "POINT")
        ld.color = [x / 255.0 for x in L["color"]]
        ld.energy = L.get("watts", L.get("lumens", 400) / 683.0 * 4 * math.pi * 6)
        if ld.type == "SPOT":
            ld.spot_size = math.radians(L.get("cone", 60)); ld.spot_blend = L.get("blend", 0.5)
        ld.shadow_soft_size = L.get("size", 0.1)
        lo = bpy.data.objects.new("L%d" % i, ld); sc.collection.objects.link(lo)
        lo.matrix_world = to_blender(src_xf(L["pos"], L.get("ang", [0, 0, 0])), Pinv)
        made_lights.append(lo)
    for fr in job["frames"]:
        apply(a, min(fr / a.frame_rate, a.duration))
        sc.render.filepath = os.path.join(out_dir, "%s_f%02d.png" % (job["name"], fr))
        bpy.ops.render.render(write_still=True)
    print("MENU_RENDER", job["name"], len(job["frames"]), flush=True)
print("MENU_RENDER_DONE", flush=True)
