"""Magician x-ray: render a hero glTF with its skeleton visible through a translucent mesh.

Run headless:
  blender --background --factory-startup --python xray_render.py -- <model.glb> <out_dir> [--views front,side,back,three_quarter]

Bones are turned into real geometry (octahedra between head and tail, plus joint spheres) so they
show up in a final render, not just the viewport overlay. Bone colour encodes the chain:
spine/head = white, left = cyan, right = magenta, cloth/hair/jiggle = amber, IK/offset/helpers = grey.
"""
import bpy, sys, math, os
from mathutils import Vector, Matrix

argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
if len(argv) < 2:
    raise SystemExit("usage: -- <model.glb> <out_dir> [--views a,b]")
glb, out_dir = argv[0], argv[1]
views = ["front", "side", "back", "three_quarter"]
if "--views" in argv:
    views = argv[argv.index("--views") + 1].split(",")
os.makedirs(out_dir, exist_ok=True)

bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.import_scene.gltf(filepath=glb)

arm = next((o for o in bpy.context.scene.objects if o.type == "ARMATURE"), None)
meshes = [o for o in bpy.context.scene.objects if o.type == "MESH"]
if arm is None:
    raise SystemExit("no armature in " + glb)

def mat(name, rgba, emission=0.0):
    m = bpy.data.materials.new(name)
    m.diffuse_color = rgba
    return m

M = {
    "core":  mat("x_core",  (0.95, 0.95, 0.95, 1)),
    "left":  mat("x_left",  (0.15, 0.85, 1.0, 1)),
    "right": mat("x_right", (1.0, 0.25, 0.75, 1)),
    "soft":  mat("x_soft",  (1.0, 0.72, 0.15, 1)),
    "helper": mat("x_helper", (0.45, 0.45, 0.45, 1)),
    "skin":  mat("x_skin",  (0.55, 0.6, 0.7, 0.18)),
}

def classify(name):
    n = name.lower()
    # IK targets, offsets, attach points, and prop/weapon sockets (object_hand_*, weapon_hand_*,
    # weaponHier_*) sit at the origin in bind pose: drawing them as limbs lies about the rig.
    if any(k in n for k in ("iktarget", "offset", "attach", "root_motion", "object_", "weapon")):
        return "helper"
    if any(k in n for k in ("hair", "cloth", "jiggle", "skirt", "coat", "cape", "rope", "tail", "strap")):
        return "soft"
    if n.endswith("_l") or "_l_" in n or n.endswith(".l"):
        return "left"
    if n.endswith("_r") or "_r_" in n or n.endswith(".r"):
        return "right"
    return "core"

# Bones -> geometry, in world space at rest pose.
bones_mesh = bpy.data.meshes.new("xray_bones")
verts, faces, mat_idx = [], [], []
order = ["core", "left", "right", "soft", "helper"]
height = max((arm.matrix_world @ b.head_local).z for b in arm.data.bones) or 1.0
r_base = height * 0.006

def add_octa(h, t, cls):
    d = t - h
    L = d.length
    if L < 1e-5:
        return
    z = d.normalized()
    x = z.orthogonal().normalized()
    y = z.cross(x)
    r = min(L * 0.12, r_base * 2.0)
    mid = h + d * 0.18
    base = len(verts)
    verts.extend([h, mid + x * r, mid + y * r, mid - x * r, mid - y * r, t])
    for a, b in ((1, 2), (2, 3), (3, 4), (4, 1)):
        faces.append((base, base + a, base + b)); mat_idx.append(order.index(cls))
        faces.append((base + 5, base + b, base + a)); mat_idx.append(order.index(cls))

def add_joint(p, cls, r):
    # small octahedron = joint marker
    base = len(verts)
    verts.extend([p + Vector((0, 0, r)), p + Vector((r, 0, 0)), p + Vector((0, r, 0)),
                  p + Vector((-r, 0, 0)), p + Vector((0, -r, 0)), p + Vector((0, 0, -r))])
    for a, b in ((1, 2), (2, 3), (3, 4), (4, 1)):
        faces.append((base, base + a, base + b)); mat_idx.append(order.index(cls))
        faces.append((base + 5, base + b, base + a)); mat_idx.append(order.index(cls))

# glTF bone tails are invented by the importer, so draw the real skeleton: parent joint -> child joint.
# Helper bones (IK targets, offsets, attachWorld) get a joint marker only, never a segment.
for b in arm.data.bones:
    h = arm.matrix_world @ b.head_local
    cls = classify(b.name)
    add_joint(h, cls, r_base * (0.6 if cls == "helper" else 1.0))
    if b.parent is not None and cls != "helper" and classify(b.parent.name) != "helper":
        add_octa(arm.matrix_world @ b.parent.head_local, h, cls)

bones_mesh.from_pydata(verts, [], faces)
bones_obj = bpy.data.objects.new("xray_bones", bones_mesh)
bpy.context.scene.collection.objects.link(bones_obj)
for k in order:
    bones_obj.data.materials.append(M[k])
for poly, mi in zip(bones_obj.data.polygons, mat_idx):
    poly.material_index = mi

for o in meshes:
    o.data.materials.clear()
    o.data.materials.append(M["skin"])

# Workbench render: flat object colour, x-ray on, cavity off. Deterministic and fast.
sc = bpy.context.scene
sc.render.engine = "BLENDER_WORKBENCH"
sh = sc.display.shading
sh.light = "FLAT"
sh.color_type = "MATERIAL"
sh.show_xray = True
sh.xray_alpha = 0.22
sh.show_object_outline = True
sh.object_outline_color = (0.2, 0.22, 0.26)
sc.render.film_transparent = False
sc.world = bpy.data.worlds.new("w") if sc.world is None else sc.world
sc.world.color = (0.035, 0.035, 0.04)
sc.render.resolution_x, sc.render.resolution_y = 1024, 1536

# Frame the whole model.
pts = [o.matrix_world @ Vector(c) for o in meshes for c in o.bound_box] or [Vector((0, 0, 0)), Vector((0, 0, height))]
lo = Vector((min(p.x for p in pts), min(p.y for p in pts), min(p.z for p in pts)))
hi = Vector((max(p.x for p in pts), max(p.y for p in pts), max(p.z for p in pts)))
center, size = (lo + hi) / 2, hi - lo

cam_data = bpy.data.cameras.new("cam")
cam_data.type = "ORTHO"
cam_data.ortho_scale = max(size.z, size.x, size.y) * 1.12
cam = bpy.data.objects.new("cam", cam_data)
sc.collection.objects.link(cam)
sc.camera = cam

# Source/Deadlock models face +X after glTF import (Y-up -> Z-up); views are yaw angles around the model.
yaw = {"front": 0, "three_quarter": 35, "side": 90, "back": 180}
dist = max(size) * 3
for v in views:
    a = math.radians(yaw[v])
    # glTF importer leaves the character facing -Y; the camera sits on that side for "front".
    off = Vector((math.sin(a) * dist, -math.cos(a) * dist, 0))
    cam.location = center + off
    cam.rotation_euler = (math.radians(90), 0, a)
    sc.render.filepath = os.path.join(out_dir, f"xray_{v}.png")
    bpy.ops.render.render(write_still=True)
    print("XRAY_WROTE", sc.render.filepath)

print("XRAY_BONES", len(arm.data.bones), "MESHES", len(meshes), "HEIGHT", round(size.z, 3))
