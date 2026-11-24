"""QA cache: import a Source 2 Viewer glb (model + materials + every clip) once, save it as a .blend.

  blender --background --factory-startup --python qa_cache.py -- <renamed.glb> <out.blend> [--fps 30] [--clips-tsv <out.tsv>]

Importing a hero with ~500 clips takes minutes; opening the .blend takes seconds, so every later QA step opens
the cache. Feed it a glb renamed by glb_clips.py: Blender truncates ID names to 63 bytes, and Source 2 Viewer's
clip paths collide past that. The scene runs at --fps (default 30, Source 2's clip rate) before the import, so a
Blender frame is a clip frame: frame numbers in every QA report are the clip's own.

Writes a TSV of every action: name, first frame, last frame, fps.
"""
import bpy, sys, os

argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
glb, out = argv[0], argv[1]
tsv = argv[argv.index("--clips-tsv") + 1] if "--clips-tsv" in argv else os.path.splitext(out)[0] + "_clips.tsv"

fps_in = float(argv[argv.index("--fps") + 1]) if "--fps" in argv else 30.0
bpy.ops.wm.read_factory_settings(use_empty=True)
sc = bpy.context.scene
sc.render.fps, sc.render.fps_base = int(round(fps_in)), 1.0
bpy.ops.import_scene.gltf(filepath=glb)
arm = next(o for o in sc.objects if o.type == "ARMATURE")
# glTF import leaves the last clip assigned; the cache is stored at rest with no clip on the rig
if arm.animation_data:
    arm.animation_data.action = None
for a in bpy.data.actions:
    a.use_fake_user = True
fps = sc.render.fps / sc.render.fps_base
rows = []
for a in sorted(bpy.data.actions, key=lambda a: a.name):
    rows.append((a.name, int(a.frame_range[0]), int(a.frame_range[1]), fps))
os.makedirs(os.path.dirname(os.path.abspath(out)), exist_ok=True)
with open(tsv, "w", encoding="utf-8") as f:
    f.write("name\tfirst\tlast\tfps\n")
    for r in rows:
        f.write("\t".join(str(x) for x in r) + "\n")
dupes = [a.name for a in bpy.data.actions if a.name[-4:-3] == "." and a.name[-3:].isdigit()]
bpy.ops.wm.save_as_mainfile(filepath=out, compress=True)
print("QA_CACHE", out, "clips", len(rows), "meshes", sum(1 for o in sc.objects if o.type == "MESH"),
      "bones", len(arm.data.bones), "suffixed_names", len(dupes))
