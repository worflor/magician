"""Magician stage: the shots a turnaround can't give. Gameplay camera, pose tests, motion map.

  blender --background --factory-startup --python stage_render.py -- <model.blend> <out_dir>
          [--shots gameplay,below,below_back,front,three_quarter,side,back] [--res 1024] [--prefix name]
          [--anim-glb <hero_with_clips.glb> --action <name substring> --frame N]   pose the rig on one frame of a clip
          [--list-actions]                                                          print the clip names and exit
          [--save-with-clips <out.blend>]                                           cache the clips into a .blend (then pass
                                                                                    --action without --anim-glb: seconds, not minutes)
          [--motionmap]                                                             colour every mesh by its "mech" property

gameplay: a perspective camera behind and above the right shoulder, the way Deadlock frames the hero.
Pose tests: the clip is read from a second glb of the same hero (same bone names) and played on the rig in
the .blend, so everything bound to that rig is posed with it. Cloth is not simulated here.
Motion map: objects carry a custom property "mech" (rigid / firm / proxy / chain / glint / body).
"""
import bpy, sys, math, os
from mathutils import Vector

argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
src, out_dir = argv[0], argv[1]
def opt(name, default):
    return argv[argv.index(name) + 1] if name in argv else default
shots = opt("--shots", "gameplay").split(",")
res = int(opt("--res", "1024"))
prefix = opt("--prefix", "stage")
anim_glb, action_name, frame = opt("--anim-glb", ""), opt("--action", ""), int(opt("--frame", "0"))
os.makedirs(out_dir, exist_ok=True)

bpy.ops.wm.open_mainfile(filepath=src)
sc = bpy.context.scene
for o in list(sc.objects):
    if o.type in {"CAMERA", "LIGHT"}:
        bpy.data.objects.remove(o)
arm = next(o for o in sc.objects if o.type == "ARMATURE")
meshes = [o for o in sc.objects if o.type == "MESH" and o.visible_get() and "physics" not in o.name.lower()]

# Framing and light are measured on the rest pose, so every shot of one model matches.
pts = [o.matrix_world @ Vector(c) for o in meshes for c in o.bound_box]
lo = Vector((min(p.x for p in pts), min(p.y for p in pts), min(p.z for p in pts)))
hi = Vector((max(p.x for p in pts), max(p.y for p in pts), max(p.z for p in pts)))
center, size = (lo + hi) / 2, hi - lo
H = size.z

if anim_glb:
    before = set(bpy.data.objects)
    bpy.ops.import_scene.gltf(filepath=anim_glb)
    for o in set(bpy.data.objects) - before:
        bpy.data.objects.remove(o)
if "--list-actions" in argv:
    for n in sorted(a.name for a in bpy.data.actions):
        a = bpy.data.actions[n]
        print("ACTION", n, int(a.frame_range[0]), int(a.frame_range[1]))
    sys.exit(0)
if "--save-with-clips" in argv:
    # Importing a hero's clips takes minutes (hundreds of bones x hundreds of clips); cache them once.
    for a in bpy.data.actions:
        a.use_fake_user = True
    bpy.ops.wm.save_as_mainfile(filepath=opt("--save-with-clips", ""))
    print("STAGE_SAVED", opt("--save-with-clips", ""))
    sys.exit(0)
if action_name:
    names = sorted(a.name for a in bpy.data.actions)
    match = [n for n in names if action_name.lower() in n.lower()]
    exact = [n for n in match if n.lower() == action_name.lower()]
    act = bpy.data.actions[(exact or match)[0]]
    ad = arm.animation_data_create()
    ad.action = act
    if hasattr(ad, "action_slot") and getattr(act, "slots", None):
        ad.action_slot = act.slots[0]
    sc.frame_set(frame)
    bpy.context.view_layer.update()
    print("POSED", act.name, "frame", frame)

if "--motionmap" in argv:
    COL = {"rigid": (0.10, 0.22, 0.75), "firm": (1.0, 0.62, 0.0), "proxy": (0.90, 0.02, 0.40), "chain": (0.0, 0.75, 0.80),
           "glint": (1.0, 1.0, 1.0), "body": (0.035, 0.035, 0.04)}
    mats = {}
    for k, c in COL.items():
        m = bpy.data.materials.new("mech_" + k)
        m.use_nodes = True
        p = m.node_tree.nodes["Principled BSDF"]
        p.inputs["Base Color"].default_value = (*c, 1)
        p.inputs["Roughness"].default_value = 0.75
        if k == "glint":
            p.inputs["Emission Color"].default_value = (1, 1, 1, 1)
            p.inputs["Emission Strength"].default_value = 4.0
        mats[k] = m
    for o in meshes:
        if "sparkle" in o.name.lower():
            o.hide_render = True
            continue
        k = o.get("mech", "body")
        for s in o.material_slots:
            s.material = mats.get(k, mats["rigid"])

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

# follow the hero if the clip moves him
piv = arm.matrix_world @ arm.pose.bones["pelvis"].matrix.translation if "pelvis" in arm.pose.bones else center
rest_piv = arm.matrix_world @ arm.data.bones["pelvis"].head_local if "pelvis" in arm.data.bones else center
shift = piv - rest_piv
center = center + shift
lo = lo + shift

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
if "gameplay" in shots:
    # the player's side of the hero is his back: light it like the front
    light("back_key", "AREA", center + Vector((-1.0 * H, 1.5 * H, 1.0 * H)), 120 * H * H, (1.0, 0.97, 0.92), H)
    light("back_fill", "AREA", center + Vector((1.3 * H, 1.1 * H, 0.2 * H)), 40 * H * H, (0.85, 0.9, 1.0), H)

cam_data = bpy.data.cameras.new("cam")
cam = bpy.data.objects.new("cam", cam_data)
sc.collection.objects.link(cam)
sc.camera = cam
yaw = {"front": 0, "three_quarter": 35, "side": 90, "back": 180}
dist = max(size) * 3
for v in shots:
    if v == "gameplay":
        cam_data.type = "PERSP"
        cam_data.lens = 50
        target = Vector((center.x + 0.10, center.y, lo.z + H * 0.56))
        cam.location = Vector((center.x - 0.95, center.y + 4.0, lo.z + H * 1.05))
        cam.rotation_euler = (target - cam.location).to_track_quat("-Z", "Y").to_euler()
    elif v in ("below", "below_back"):
        # an enemy looking up at him mid-dive: what is under the skirt is part of the costume
        sgn_ = -1 if v == "below" else 1
        light("under_" + v, "AREA", center + Vector((0.3 * H, sgn_ * 1.0 * H, -0.75 * H)), 90 * H * H, (1.0, 0.98, 0.95), H)
        cam_data.type = "PERSP"
        cam_data.lens = 50
        target = Vector((center.x, center.y, lo.z + H * 0.47))
        cam.location = Vector((center.x + 0.7, center.y + sgn_ * 3.3, lo.z - H * 0.42))
        cam.rotation_euler = (target - cam.location).to_track_quat("-Z", "Y").to_euler()
    else:
        cam_data.type = "ORTHO"
        cam_data.ortho_scale = max(size.z, size.x, size.y) * 1.12
        a = math.radians(yaw[v])
        cam.location = center + Vector((math.sin(a) * dist, -math.cos(a) * dist, 0))
        cam.rotation_euler = (math.radians(90), 0, a)
    sc.render.filepath = os.path.join(out_dir, f"{prefix}_{v}.png")
    bpy.ops.render.render(write_still=True)
    print("STAGE_WROTE", sc.render.filepath)
