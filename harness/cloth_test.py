"""Magician cloth test: simulate garment panels on a real clip and shoot them from the gameplay camera.

  blender --background --factory-startup --python cloth_test.py -- <model.blend> <out_dir> --anim-glb <hero_with_clips.glb> --action <name>
          --cloth nr_tail_ground            object-name prefix of the panels to simulate (their own mesh is the cloth proxy)
          --follow nr_tail_                 prefix of trims that ride a panel (bound by Surface Deform to the panel on their side: _l / _r)
          --colliders lash_body,nr_leg_,nr_boot_l,nr_boot_r,nr_plate_0
          --pin-z 1.44:1.30                 goal strength 1 above the first height, falling to 0 at the second (rest pose, model units)
          [--mass 0.03] [--bend 0.4] [--stiff 12] [--air 1.0] [--wind x,y,z] [--gravity 13.4]
          [--margin 0.014]                  collision stand-off (cloth to collider), model units
          [--frames 8] [--loops 6] [--slow 2] [--res 420] [--prefix name]
          [--pin-group name]                use this vertex group of each panel as its goal strength instead of --pin-z
                                            (for pieces pinned at different heights, e.g. floating panels)
          [--skip substring]                leave out objects whose name contains this, even if they match --cloth
          (omit --anim-glb to use clips already cached in the .blend by stage_render.py --save-with-clips)

The clip is looped; the cloth settles over the first cycles and the last cycle is shot in --frames steps.
Prints, per shot, how many panel vertices are inside the leg capsules (thigh and shin), so a tangle is a number
and not an opinion. Blender's cloth has one mass for the whole panel: a weighted hem is approximated by --mass
and --air, and the per-region values for Source 2 are worked out from what settles well here.
"""
import bpy, sys, math, os
from mathutils import Vector

argv = sys.argv[sys.argv.index("--") + 1:]
src, out_dir = argv[0], argv[1]
def opt(name, default):
    return argv[argv.index(name) + 1] if name in argv else default
anim_glb, action_name = opt("--anim-glb", ""), opt("--action", "")
cloth_pre, follow_pre = opt("--cloth", "nr_tail_ground"), opt("--follow", "")
coll_pre = tuple(opt("--colliders", "lash_body").split(","))
pin_hi, pin_lo = (float(x) for x in opt("--pin-z", "1.44:1.30").split(":"))
mass, bend, stiff, air = float(opt("--mass", "0.03")), float(opt("--bend", "0.4")), float(opt("--stiff", "12")), float(opt("--air", "1.0"))
wind = [float(x) for x in opt("--wind", "0,0,0").split(",")]
grav = float(opt("--gravity", "13.4"))
n_shots, cycles, res = int(opt("--frames", "8")), int(opt("--loops", "6")), int(opt("--res", "420"))
prefix = opt("--prefix", "cloth")
os.makedirs(out_dir, exist_ok=True)

bpy.ops.wm.open_mainfile(filepath=src)
sc = bpy.context.scene
for o in list(sc.objects):
    if o.type in {"CAMERA", "LIGHT"}:
        bpy.data.objects.remove(o)
arm = next(o for o in sc.objects if o.type == "ARMATURE")
meshes = [o for o in sc.objects if o.type == "MESH"]
pts = [o.matrix_world @ Vector(c) for o in meshes for c in o.bound_box]
lo = Vector((min(p.x for p in pts), min(p.y for p in pts), min(p.z for p in pts)))
hi = Vector((max(p.x for p in pts), max(p.y for p in pts), max(p.z for p in pts)))
center, H = (lo + hi) / 2, hi.z - lo.z

def smooth(e0, e1, x):
    t = min(1.0, max(0.0, (x - e0) / (e1 - e0)))
    return t * t * (3 - 2 * t)
def side_of(name):
    toks = name.split("_")
    return "l" if "l" in toks else ("r" if "r" in toks else "")

# ---- panels: armature (pins follow the pelvis), then cloth
skip_sub, pin_group = opt("--skip", ""), opt("--pin-group", "")
panels = [o for o in meshes if o.name.startswith(cloth_pre) and not (skip_sub and skip_sub in o.name)]
FREE = {}
for o in panels:
    for m in list(o.modifiers):
        if m.type in {"SUBSURF", "SOLIDIFY"}:
            o.modifiers.remove(m)
    vg = o.vertex_groups.new(name="pin")
    if pin_group and pin_group in o.vertex_groups:
        src_g = o.vertex_groups[pin_group].index
        wts = {v.index: next((g.weight for g in v.groups if g.group == src_g), 0.0) for v in o.data.vertices}
        for vi, w in wts.items():
            if w > 0:
                vg.add([vi], w, "REPLACE")
        FREE[o.name] = {vi for vi, w in wts.items() if w < 0.3}
        continue
    for v in o.data.vertices:
        z = (o.matrix_world @ v.co).z
        w = smooth(pin_lo, pin_hi, z)
        if w > 0:
            vg.add([v.index], w, "REPLACE")
    FREE[o.name] = {v.index for v in o.data.vertices if smooth(pin_lo, pin_hi, (o.matrix_world @ v.co).z) < 0.3}

# ---- trims ride the panel on their side (bound in the rest pose)
bpy.context.view_layer.update()
if follow_pre:
    for o in meshes:
        if o.name.startswith(follow_pre) and o not in panels:
            tgt = next((p for p in panels if side_of(p.name) == side_of(o.name)), None)
            if tgt is None:
                continue
            for m in list(o.modifiers):
                if m.type in {"ARMATURE", "SUBSURF", "SOLIDIFY"}:
                    o.modifiers.remove(m)
            sd = o.modifiers.new("ride", "SURFACE_DEFORM")
            sd.target = tgt
            with bpy.context.temp_override(object=o, active_object=o, selected_objects=[o]):
                bpy.ops.object.surfacedeform_bind(modifier="ride")

def dress():
    for o in panels:
        c = o.modifiers.new("cloth", "CLOTH")
        s = c.settings
        s.quality = 10
        s.mass = mass
        s.air_damping = air
        s.tension_stiffness = s.compression_stiffness = s.shear_stiffness = stiff
        s.bending_stiffness = bend
        s.tension_damping = s.compression_damping = s.shear_damping = 5
        s.vertex_group_mass = "pin"
        s.pin_stiffness = 1.0
        cs = c.collision_settings
        cs.use_collision = True
        cs.distance_min = 0.014
        cs.collision_quality = 4
        cs.use_self_collision = False

    for o in meshes:
        if o.name.split(".")[-1].startswith(coll_pre) or o.name.startswith(coll_pre):
            for m in list(o.modifiers):
                if m.type in {"SUBSURF", "SOLIDIFY"}:
                    o.modifiers.remove(m)
            cm = o.modifiers.new("collide", "COLLISION")
            o.collision.thickness_outer = float(opt("--margin", "0.014"))
            o.collision.thickness_inner = 0.05
            o.collision.cloth_friction = 4
            o.collision.damping = 0.3


# ---- the clip, looped
if anim_glb:
    before = set(bpy.data.objects)
    bpy.ops.import_scene.gltf(filepath=anim_glb)
    for o in set(bpy.data.objects) - before:
        bpy.data.objects.remove(o)
names = sorted(a.name for a in bpy.data.actions)
match = [n for n in names if action_name.lower() in n.lower()]
exact = [n for n in match if n.lower() == action_name.lower()]
act = bpy.data.actions[(exact or match)[0]]
ad = arm.animation_data_create()
slow = float(opt("--slow", "1"))               # play the clip this many times slower (the rip's frame rate is not the game's)
raw_len = max(2, int(act.frame_range[1] - act.frame_range[0]))
clip_len = max(2, int(round(raw_len * slow)))
BLEND = 16                                     # ease from the rest pose into the clip, so the cloth starts clean and is carried in
track = ad.nla_tracks.new()
strip = track.strips.new("clip", 1, act)
if hasattr(strip, "action_slot") and getattr(act, "slots", None):
    strip.action_slot = act.slots[0]
strip.scale = slow
strip.repeat = cycles + 2
strip.blend_in = BLEND
strip.extrapolation = "NOTHING"
end = BLEND + clip_len * cycles
# The clips carry root travel (a sprint covers four body-lengths a cycle and snaps back on the loop).
# In game the hero's travel adds no force to cloth (his cloth has wind off), so hold him in place: the
# rig is moved against the pelvis' horizontal travel, frame by frame. Vertical motion is kept.
arm_home = arm.location.copy()
travel = []
for f in range(0, end + 1):
    sc.frame_set(f)
    bpy.context.view_layer.update()
    pw = arm.matrix_world @ arm.pose.bones["pelvis"].matrix.translation
    travel.append(Vector((pw.x, pw.y, 0)))
travel = [t_ - travel[0] for t_ in travel]
def hold(f):
    arm.location = arm_home - travel[max(0, min(end, f))]
hold(0)
sc.frame_set(0)
dress()
sc.render.fps = 30
sc.gravity = (0, 0, -grav)
sc.frame_start, sc.frame_end = 0, end
for o in panels:
    pc = o.modifiers["cloth"].point_cache
    pc.frame_start, pc.frame_end = 0, end
if any(abs(w) > 1e-6 for w in wind):
    wv = Vector(wind)
    bpy.ops.object.effector_add(type="WIND", location=center - wv.normalized() * 6)
    wo = bpy.context.active_object
    wo.rotation_euler = wv.normalized().to_track_quat("Z", "Y").to_euler()
    wo.field.strength = wv.length
    wo.field.flow = 0.0

# ---- stage
sc.render.engine = "BLENDER_EEVEE_NEXT" if "BLENDER_EEVEE_NEXT" in bpy.types.RenderSettings.bl_rna.properties["engine"].enum_items.keys() else "BLENDER_EEVEE"
sc.render.resolution_x, sc.render.resolution_y = res, int(res * 1.5)
sc.view_settings.view_transform = "AgX"
w = sc.world or bpy.data.worlds.new("w")
sc.world = w
w.use_nodes = True
bg = w.node_tree.nodes.get("Background")
bg.inputs[0].default_value = (0.045, 0.045, 0.05, 1)
bg.inputs[1].default_value = 0.6
rig = bpy.data.objects.new("stage", None)
sc.collection.objects.link(rig)
def light(name, loc, energy, color, sz):
    d = bpy.data.lights.new(name, "AREA")
    d.energy, d.color, d.size = energy, color, sz
    o = bpy.data.objects.new(name, d)
    sc.collection.objects.link(o)
    o.location = loc
    o.rotation_euler = (center - Vector(loc)).to_track_quat("-Z", "Y").to_euler()
    o.parent = rig
light("key", center + Vector((-1.2 * H, -1.4 * H, 0.9 * H)), 140 * H * H, (1.0, 0.96, 0.9), H)
light("back_key", center + Vector((-1.0 * H, 1.5 * H, 1.0 * H)), 130 * H * H, (1.0, 0.97, 0.92), H)
light("back_fill", center + Vector((1.3 * H, 1.1 * H, 0.2 * H)), 45 * H * H, (0.85, 0.9, 1.0), H)
cam_data = bpy.data.cameras.new("cam")
cam_data.lens = 50
cam = bpy.data.objects.new("cam", cam_data)
sc.collection.objects.link(cam)
sc.camera = cam
cam.parent = rig
target = Vector((center.x + 0.10, center.y, lo.z + H * 0.50))
cam.location = Vector((center.x - 0.95, center.y + 4.0, lo.z + H * 1.05))
cam.rotation_euler = (target - cam.location).to_track_quat("-Z", "Y").to_euler()
rest_piv = arm.matrix_world @ arm.data.bones["pelvis"].head_local

def capsule_hits():
    """Panel vertices inside the leg capsules (thigh, shin), both legs."""
    dg = bpy.context.evaluated_depsgraph_get()
    pb = arm.pose.bones
    segs = []
    for sd in ("L", "R"):
        hip, knee, ank = (arm.matrix_world @ pb[f"{n}_{sd}"].head for n in ("leg_upper", "leg_lower", "ankle"))
        segs += [(hip, knee, 0.105), (knee, ank, 0.060)]      # well inside the limb: contact and drape do not count, only real penetration
    n = tot = 0
    for o in panels:
        ev = o.evaluated_get(dg)
        free = FREE[o.name]
        for v in ev.data.vertices:
            if v.index not in free:
                continue                      # pinned cloth goes where the skinned garment goes; only free cloth can tangle
            p = ev.matrix_world @ v.co
            tot += 1
            for a, b, r in segs:
                ab = b - a
                t = max(0.0, min(1.0, (p - a).dot(ab) / ab.length_squared))
                if (p - (a + ab * t)).length < r:
                    n += 1
                    break
    return n, tot

shots = [end - clip_len + round(clip_len * k / n_shots) for k in range(n_shots)]
for f in range(0, end + 1):
    hold(f)
    sc.frame_set(f)
    if "--debug" in argv and f % 3 == 0:
        dg_ = bpy.context.evaluated_depsgraph_get()
        for o_ in panels[:1]:
            ev_ = o_.evaluated_get(dg_)
            zs_ = [(ev_.matrix_world @ v.co) for v in ev_.data.vertices]
            print("CLOTH_DBG", f, o_.name, "zmin %.3f zmax %.3f ymax %.3f" % (min(p.z for p in zs_), max(p.z for p in zs_), max(p.y for p in zs_)), o_.modifiers["cloth"].point_cache.info, [m.type for m in o_.modifiers])
    if f in shots:
        bpy.context.view_layer.update()
        pz = (arm.matrix_world @ arm.pose.bones["pelvis"].matrix.translation) - rest_piv
        rig.location = (pz.x, pz.y, pz.z * 0.5)
        k = shots.index(f)
        hits, tot = capsule_hits()
        sc.render.filepath = os.path.join(out_dir, f"{prefix}_{k:02d}.png")
        bpy.ops.render.render(write_still=True)
        print(f"CLOTH_SHOT {prefix} frame {f} clip-frame {round(((f - 1) % clip_len) / slow)}: {hits} of {tot} free panel vertices inside a leg capsule")
print("CLOTH_DONE", prefix, "clip", act.name, "slow", slow, "len", clip_len, "mass", mass, "bend", bend, "stiff", stiff, "air", air, "wind", wind, "pin", pin_hi, pin_lo, "margin", opt("--margin", "0.014"))
