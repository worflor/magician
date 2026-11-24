"""Magician close-up: detail shots of a .blend or .glb under the lookdev studio light (Eevee).

  blender --background --factory-startup --python closeup_render.py -- <model.glb|.blend> <out_dir> --shots name:yaw:z:height[:x],... [--res 1024] [--prefix name]

Each shot is an orthographic frame `height` model units tall (2:3 portrait), centred at height z (and
optional x offset along the camera's right), seen from `yaw` degrees (0 front, 90 side, 180 back).
Lighting is the same three-point rig as lookdev_render.py, so close-ups match the turnaround.
"""
import bpy, sys, math, os
from mathutils import Vector

argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
src, out_dir = argv[0], argv[1]
def opt(name, default):
    return argv[argv.index(name) + 1] if name in argv else default
shots = [s.split(":") for s in opt("--shots", "bust:0:2.0:0.9").split(",")]
res = int(opt("--res", "1024"))
prefix = opt("--prefix", "closeup")
os.makedirs(out_dir, exist_ok=True)

if src.lower().endswith(".blend"):
    bpy.ops.wm.open_mainfile(filepath=src)
else:
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.ops.import_scene.gltf(filepath=src)

sc = bpy.context.scene
for o in list(sc.objects):
    if o.type in {"CAMERA", "LIGHT"}:
        bpy.data.objects.remove(o)
meshes = [o for o in sc.objects if o.type == "MESH" and o.visible_get() and "physics" not in o.name.lower()]

sc.render.engine = "BLENDER_EEVEE_NEXT" if "BLENDER_EEVEE_NEXT" in bpy.types.RenderSettings.bl_rna.properties["engine"].enum_items.keys() else "BLENDER_EEVEE"
sc.render.resolution_x, sc.render.resolution_y = res, int(res * 1.5)
sc.render.film_transparent = False
sc.view_settings.view_transform = "AgX" if "AgX" in [i.identifier for i in sc.view_settings.bl_rna.properties["view_transform"].enum_items] else "Filmic"
w = sc.world or bpy.data.worlds.new("w")
sc.world = w
w.use_nodes = True
bg = w.node_tree.nodes.get("Background")
bg.inputs[0].default_value = (0.045, 0.045, 0.05, 1)
bg.inputs[1].default_value = 0.6

pts = [o.matrix_world @ Vector(c) for o in meshes for c in o.bound_box]
lo = Vector((min(p.x for p in pts), min(p.y for p in pts), min(p.z for p in pts)))
hi = Vector((max(p.x for p in pts), max(p.y for p in pts), max(p.z for p in pts)))
center, size = (lo + hi) / 2, hi - lo
H = size.z

def light(name, kind, loc, energy, color=(1, 1, 1), sz=None):
    d = bpy.data.lights.new(name, kind)
    d.energy = energy
    d.color = color
    if sz is not None:
        d.size = sz
    o = bpy.data.objects.new(name, d)
    sc.collection.objects.link(o)
    o.location = loc
    o.rotation_euler = (center - Vector(loc)).to_track_quat("-Z", "Y").to_euler()
    return o

light("key", "AREA", center + Vector((-1.2 * H, -1.4 * H, 0.9 * H)), 140 * H * H, (1.0, 0.96, 0.9), H)
light("fill", "AREA", center + Vector((1.4 * H, -1.0 * H, 0.2 * H)), 45 * H * H, (0.85, 0.9, 1.0), H)
light("rim", "AREA", center + Vector((0.4 * H, 1.6 * H, 1.0 * H)), 110 * H * H, (1.0, 1.0, 1.0), H * 0.6)

cam_data = bpy.data.cameras.new("cam")
cam_data.type = "ORTHO"
cam = bpy.data.objects.new("cam", cam_data)
sc.collection.objects.link(cam)
sc.camera = cam
dist = max(size) * 3
for shot in shots:
    name, yaw, z, height = shot[0], float(shot[1]), float(shot[2]), float(shot[3])
    xoff = float(shot[4]) if len(shot) > 4 else 0.0
    a = math.radians(yaw)
    right = Vector((math.cos(a), math.sin(a), 0))
    target = Vector((0, 0, z)) + right * xoff
    cam_data.ortho_scale = height          # the long (vertical) side of the portrait frame
    cam.location = target + Vector((math.sin(a) * dist, -math.cos(a) * dist, 0))
    cam.rotation_euler = (math.radians(90), 0, a)
    sc.render.filepath = os.path.join(out_dir, f"{prefix}_{name}.png")
    bpy.ops.render.render(write_still=True)
    print("CLOSEUP_WROTE", sc.render.filepath)
