"""QA piece index: the designer's garment pieces at rest, so a compiled mesh's loose parts can be named.

  blender --background --factory-startup --python piece_index.py -- <design.blend> <out.json> [--samples 500] [--stamp text]

A compiled model merges hundreds of garment pieces into a few render meshes and loses their names. The QA
splits each compiled mesh back into welded connected components and matches every component to the piece it
came from by surface samples (qa_lib.match_pieces). This writes, for every piece that renders: its name, the
"mech" motion class, materials, triangle count, rest-pose world bounds and up to --samples surface points.

What renders follows the export's rule: render-hidden objects, wire or bounds display, and names or mech values
marking collision/collider/guide/helper are skipped (and listed under "skipped").
"""
import bpy, sys, json, os
import numpy as np

argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
src, out = argv[0], argv[1]
nsamp = int(argv[argv.index("--samples") + 1]) if "--samples" in argv else 500
stamp = argv[argv.index("--stamp") + 1] if "--stamp" in argv else ""

bpy.ops.wm.open_mainfile(filepath=src)
sc = bpy.context.scene
for o in sc.objects:
    if o.type == "ARMATURE":
        o.data.pose_position = "REST"
bpy.context.view_layer.update()
dg = bpy.context.evaluated_depsgraph_get()
HELPER = ("collision", "collider", "guide", "helper")
pieces, skipped = [], []
for o in sc.objects:
    if o.type != "MESH":
        continue
    n = o.name.lower()
    mech = str(o.get("mech", ""))
    if (o.hide_render or o.display_type in {"WIRE", "BOUNDS"} or any(k in n for k in HELPER)
            or mech in ("collision", "collider", "guide", "helper") or not o.visible_get()):
        skipped.append(o.name)
        continue
    ev = o.evaluated_get(dg)
    m = ev.to_mesh()
    nv = len(m.vertices)
    if nv == 0:
        ev.to_mesh_clear()
        continue
    co = np.empty(nv * 3, np.float32)
    m.vertices.foreach_get("co", co)
    co = co.reshape(-1, 3)
    mw = np.array(ev.matrix_world, np.float64)
    w = co @ mw[:3, :3].T + mw[:3, 3]
    m.calc_loop_triangles()
    ntri = len(m.loop_triangles)
    step = max(1, nv // nsamp)
    pieces.append({
        "name": o.name, "mech": mech, "materials": [s.material.name for s in o.material_slots if s.material],
        "tris": ntri, "verts": nv, "lo": w.min(0).round(5).tolist(), "hi": w.max(0).round(5).tolist(),
        "samples": w[::step][:nsamp].round(5).tolist(),
        "collections": [c.name for c in o.users_collection],
    })
    ev.to_mesh_clear()
os.makedirs(os.path.dirname(os.path.abspath(out)), exist_ok=True)
json.dump({"stamp": stamp, "blend": src, "pieces": pieces, "skipped": skipped}, open(out, "w"))
print("PIECE_INDEX", out, "pieces", len(pieces), "skipped", len(skipped))
