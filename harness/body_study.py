"""Magician body study: strip a hero to the modelled skin and take a seamstress's measurements.

  blender --background --factory-startup --python body_study.py -- <hero.glb> <out_dir> [--keep glove]

Removes every face whose material isn't the skin material (Valve models skin under clothing on most
heroes, at least on the torso), keeps any material named in --keep, renders a turnaround, and prints
circumferences at the landmarks a pattern is drafted from. Circumference is the convex-hull perimeter
of the skin at that height (what a tape measure reads), in model units and relative to height.
"""
import bpy, bmesh, sys, math, os
from mathutils import Vector

argv = sys.argv[sys.argv.index("--") + 1:]
src, out = argv[0], argv[1]
keep = argv[argv.index("--keep") + 1].split(",") if "--keep" in argv else []
os.makedirs(out, exist_ok=True)

bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.import_scene.gltf(filepath=src)
sc = bpy.context.scene
arm = next(o for o in sc.objects if o.type == "ARMATURE")
body = max((o for o in sc.objects if o.type == "MESH"), key=lambda o: len(o.data.vertices))
me = body.data
mw = body.matrix_world
H = max((mw @ v.co).z for v in me.vertices) - min((mw @ v.co).z for v in me.vertices)   # full height, before stripping
skin_slots = {i for i, s in enumerate(body.material_slots) if s.material and ("body" in s.material.name or any(k in s.material.name for k in keep))}

bm = bmesh.new(); bm.from_mesh(me); bm.faces.ensure_lookup_table()
bmesh.ops.delete(bm, geom=[f for f in bm.faces if f.material_index not in skin_slots], context="FACES")
bm.to_mesh(me); bm.free()
for o in list(sc.objects):
    if o.type == "MESH" and o is not body:
        bpy.data.objects.remove(o)

def bone(n):
    return arm.matrix_world @ arm.data.bones[n].head_local

V = [mw @ v.co for v in me.vertices]
print("STUDY skin modelled from z=%.3f to %.3f" % (min(p.z for p in V), max(p.z for p in V)))

def hull_perimeter(pts):
    pts = sorted(set((round(p[0], 5), round(p[1], 5)) for p in pts))
    if len(pts) < 3:
        return 0.0
    def cross(o, a, b):
        return (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
    lo, hi = [], []
    for p in pts:
        while len(lo) >= 2 and cross(lo[-2], lo[-1], p) <= 0: lo.pop()
        lo.append(p)
    for p in reversed(pts):
        while len(hi) >= 2 and cross(hi[-2], hi[-1], p) <= 0: hi.pop()
        hi.append(p)
    h = lo[:-1] + hi[:-1]
    return sum(math.dist(h[i], h[(i + 1) % len(h)]) for i in range(len(h)))

def girth(z, band=0.012, xmax=None, about=None):
    pts = [(p.x, p.y) for p in V if abs(p.z - z) < band and (xmax is None or abs(p.x - (about.x if about else 0)) < xmax)]
    return hull_perimeter(pts)

def limb_girth(b0, b1, f, radius=0.2):
    a, b = bone(b0), bone(b1)
    axis = (b - a).normalized()
    c = a + (b - a) * f
    ref = axis.orthogonal().normalized(); ref2 = axis.cross(ref)
    pts = []
    for p in V:
        q = p - c
        if abs(q.dot(axis)) < 0.012 and (q - axis * q.dot(axis)).length < radius:
            pts.append((q.dot(ref), q.dot(ref2)))
    return hull_perimeter(pts)

L = {n: bone(n) for n in ("neck_0", "spine_3", "spine_2", "spine_1", "pelvis", "arm_upper_L", "arm_upper_R", "head")}
rows = [
    ("neck", girth(L["neck_0"].z - 0.01, xmax=0.12, about=L["neck_0"])),
    ("chest (over pecs)", girth(L["spine_3"].z - 0.03, xmax=0.30)),
    ("underbust", girth(L["spine_2"].z, xmax=0.30)),
    ("natural waist", girth(1.68, xmax=0.30)),
    ("high hip", girth(1.55, xmax=0.32)),
    ("shoulder width (acromion to acromion)", (L["arm_upper_L"] - L["arm_upper_R"]).length),
    ("bicep", limb_girth("arm_upper_L", "arm_lower_L", 0.45, 0.16)),
    ("forearm", limb_girth("arm_lower_L", "hand_L", 0.3, 0.14)),
    ("wrist", limb_girth("arm_lower_L", "hand_L", 0.9, 0.10)),
    ("head (hat band)", girth(L["head"].z + 0.07, xmax=0.14, about=L["head"])),
]
print("STUDY height", round(H, 3))
for name, v in rows:
    print("STUDY %-40s %.3f  (%.1f%% of height)" % (name, v, 100 * v / H))
w, hp = dict(rows)["natural waist"], dict(rows)["high hip"]
ch, sh = dict(rows)["chest (over pecs)"], dict(rows)["shoulder width (acromion to acromion)"]
print("STUDY ratio chest/waist %.2f  shoulder width / waist width %.2f" % (ch / max(w, 1e-6), sh / max(w / math.pi, 1e-6)))

# turnaround, neutral clay so the form reads (texture kept on a second pass)
import importlib.util
spec = importlib.util.spec_from_file_location("lookdev", os.path.join(os.path.dirname(__file__), "lookdev_render.py"))
blend = os.path.join(out, "body_study.blend")
bpy.ops.wm.save_as_mainfile(filepath=blend)
print("STUDY_SAVED", blend)
