"""Shared Blender-side machinery for the QA steps: the model as numpy topology, posing, components, pieces,
cameras, visibility. Imported by qa_geo.py, qa_motion.py and qa_render.py (sys.path gets this folder).

Conventions
- Coordinates are Blender world space in metres (Source 2 Viewer's glb, Source units x 0.0254). The hero faces -Y.
- "Welded" vertices: glTF splits vertices at UV and normal seams; topology here is rebuilt on positions rounded to
  10 um inside each render mesh, so a piece is one connected component and seam edges are not borders.
- A component is a welded connected part of one render mesh. Classes (skin, gear, outfit, cloth, fx) come from the
  hero config. A "back shell" is a component that duplicates another one 0.1-0.6 mm behind it with opposite facing
  (the export's back face for an open sheet): it joins its front's piece and is never a separate collider.
"""
import bpy, json, math, os, sys
import numpy as np
from mathutils import Vector, Matrix
from mathutils.bvhtree import BVHTree
from mathutils.kdtree import KDTree

# draft (default; DEADLOCK_DRAFT=0 or run_qa --full for full): half-size pictures, minimal samples (the ray grid stays: halving it saved nothing).
# Detection on geometry is never touched.
DRAFT = os.environ.get("DEADLOCK_DRAFT", "1") != "0"


def args():
    return sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []


class Opts:
    def __init__(self, argv):
        self.a = argv

    def get(self, name, default=None):
        return self.a[self.a.index(name) + 1] if name in self.a else default

    def has(self, name):
        return name in self.a


# ------------------------------------------------------------------------------------------- loading
def open_model(path):
    if path.lower().endswith(".blend"):
        bpy.ops.wm.open_mainfile(filepath=path)
    else:
        bpy.ops.wm.read_factory_settings(use_empty=True)
        bpy.context.scene.render.fps = 30
        bpy.ops.import_scene.gltf(filepath=path)
    sc = bpy.context.scene
    arm = next(o for o in sc.objects if o.type == "ARMATURE")
    # only skinned render meshes: the glTF importer can add helper shapes (an Icosphere for bone display)
    meshes = sorted([o for o in sc.objects if o.type == "MESH" and "physics" not in o.name.lower()
                     and any(m.type == "ARMATURE" for m in o.modifiers)], key=lambda o: o.name)
    for o in sc.objects:
        if o.type == "MESH" and o not in meshes:
            o.hide_render = True
    # Workbench's texture mode draws a material's ACTIVE image node; after a glTF import that can be the
    # self-illumination map (black), which turns a palette costume black. Make it the base colour's image
    for m in bpy.data.materials:
        if m.node_tree is None:
            continue
        for nd in m.node_tree.nodes:
            if nd.type == "TEX_IMAGE" and any(l.to_socket.name == "Base Color" for o_ in nd.outputs for l in o_.links):
                m.node_tree.nodes.active = nd
                break
    return sc, arm, meshes


def mesh_key(obj):
    return obj.data.name.split(".", 1)[-1] if "." in obj.data.name else obj.name.split(".", 1)[-1]


def set_clip(arm, name):
    """Assign an action by exact name; returns (action, first, last) or None."""
    act = bpy.data.actions.get(name)
    if act is None:
        return None
    ad = arm.animation_data_create()
    ad.action = act
    if hasattr(ad, "action_slot") and getattr(act, "slots", None):
        ad.action_slot = act.slots[0]
    return act, int(round(act.frame_range[0])), int(round(act.frame_range[1]))


def clear_clip(arm):
    if arm.animation_data:
        arm.animation_data.action = None
    for pb in arm.pose.bones:
        pb.location = (0, 0, 0)
        pb.rotation_quaternion = (1, 0, 0, 0)
        pb.rotation_euler = (0, 0, 0)
        pb.scale = (1, 1, 1)


def sample_frames(first, last, n):
    if n == 1:
        return [first]
    if n <= 0 or last - first + 1 <= n:
        return list(range(first, last + 1))
    return sorted(set(int(round(first + (last - first) * k / (n - 1))) for k in range(n)))


# ------------------------------------------------------------------------------------------- the model
class Model:
    """Static topology of every render mesh, welded, with components, classes and skin-weight facts."""

    def __init__(self, sc, arm, meshes, cfg, weld_tol=1e-5):
        self.sc, self.arm, self.objs, self.cfg = sc, arm, meshes, cfg
        cls_mesh, cls_mat = cfg["classes"]["mesh"], cfg["classes"]["material"]
        cls_default = cfg["classes"]["default"]
        self.obj_off, offs = [], 0
        tris, tri_obj, tri_mat, tri_loops = [], [], [], []
        mat_names = []
        uv_list = []
        for oi, o in enumerate(meshes):
            m = o.data
            m.calc_loop_triangles()
            n = len(m.loop_triangles)
            vt = np.empty(n * 3, np.int64)
            m.loop_triangles.foreach_get("vertices", vt)
            lt = np.empty(n * 3, np.int64)
            m.loop_triangles.foreach_get("loops", lt)
            mi = np.empty(n, np.int64)
            m.loop_triangles.foreach_get("material_index", mi)
            names = [s.material.name if s.material else "none" for s in o.material_slots] or ["none"]
            base = len(mat_names)
            mat_names += names
            tris.append(vt.reshape(-1, 3) + offs)
            tri_loops.append((oi, lt.reshape(-1, 3)))
            tri_obj.append(np.full(n, oi))
            tri_mat.append(np.clip(mi, 0, len(names) - 1) + base)
            uvl = m.uv_layers.active
            if uvl is not None:
                uv = np.empty(len(m.loops) * 2, np.float32)
                uvl.data.foreach_get("uv", uv)
                uv_list.append(uv.reshape(-1, 2)[lt.reshape(-1, 3)])
            else:
                uv_list.append(np.zeros((n, 3, 2), np.float32))
            self.obj_off.append(offs)
            offs += len(m.vertices)
        self.nv = offs
        self.tris_raw = np.concatenate(tris)
        self.tri_obj = np.concatenate(tri_obj)
        self.tri_mat = np.concatenate(tri_mat)
        self.tri_uv = np.concatenate(uv_list)              # (T,3,2) per corner
        self.tri_loops = tri_loops                         # per object: (object index, (t,3) loop indices)
        self.mat_names = mat_names
        self.vert_obj = np.zeros(self.nv, np.int64)
        for oi, o in enumerate(meshes):
            self.vert_obj[self.obj_off[oi]:self.obj_off[oi] + len(o.data.vertices)] = oi

        # rest positions (armature at REST, evaluated: exactly what the skin produces with no clip)
        self.rest = self.posed(rest=True)
        q = np.round(self.rest / weld_tol).astype(np.int64)
        key = np.concatenate([q, self.vert_obj[:, None]], 1)
        _, first, inv = np.unique(key, axis=0, return_index=True, return_inverse=True)
        self.weld = inv.reshape(-1)                         # raw vertex -> welded id
        self.wfirst = first                                 # welded id -> one raw vertex
        self.nw = len(first)
        self.tris = self.weld[self.tris_raw]               # welded triangles
        t = self.tris
        self.degen = (t[:, 0] == t[:, 1]) | (t[:, 1] == t[:, 2]) | (t[:, 0] == t[:, 2])
        self.wobj = self.vert_obj[first]
        self._components()
        # classes per triangle and component
        mesh_cls = [cls_mesh.get(mesh_key(o), None) for o in meshes]
        mc = np.array([cls_mat.get(self.mat_names[i], cls_default) for i in range(len(self.mat_names))], dtype=object)
        tc = mc[self.tri_mat]
        for oi, c in enumerate(mesh_cls):
            if c:
                tc[self.tri_obj == oi] = c
        self.tri_cls = tc
        self.comp_cls = np.empty(self.ncomp, dtype=object)
        for c in range(self.ncomp):
            sel = self.tri_cls[self.tri_comp == c]
            vals, cnt = np.unique(sel.astype(str), return_counts=True) if len(sel) else (np.array([cls_default]), np.array([1]))
            self.comp_cls[c] = vals[cnt.argmax()]
        self.weights()
        self.back_shells()
        self.piece = np.arange(self.ncomp)                  # component -> piece representative
        for b, f in self.shell_of.items():
            self.piece[b] = f
        self.piece_name = {}

    # --------------------------------------------------------------------------------------- posing
    def posed(self, rest=False):
        if rest:
            self.arm.data.pose_position = "REST"
        bpy.context.view_layer.update()
        dg = bpy.context.evaluated_depsgraph_get()
        out = np.empty((self.nv, 3), np.float64)
        for oi, o in enumerate(self.objs):
            ev = o.evaluated_get(dg)
            m = ev.to_mesh()
            co = np.empty(len(m.vertices) * 3, np.float32)
            m.vertices.foreach_get("co", co)
            mw = np.array(ev.matrix_world, np.float64)
            out[self.obj_off[oi]:self.obj_off[oi] + len(m.vertices)] = co.reshape(-1, 3) @ mw[:3, :3].T + mw[:3, 3]
            ev.to_mesh_clear()
        if rest:
            self.arm.data.pose_position = "POSE"
            bpy.context.view_layer.update()
        return out

    def wpos(self, raw):
        return raw[self.wfirst]

    # --------------------------------------------------------------------------------------- topology
    def _components(self):
        t = self.tris[~self.degen]
        lab = np.arange(self.nw)
        e = np.concatenate([t[:, [0, 1]], t[:, [1, 2]], t[:, [0, 2]]])
        while True:
            m = np.minimum(lab[e[:, 0]], lab[e[:, 1]])
            old = lab.copy()
            np.minimum.at(lab, e[:, 0], m)
            np.minimum.at(lab, e[:, 1], m)
            for _ in range(8):
                lab = lab[lab]
            if np.array_equal(lab, old):
                break
        _, comp = np.unique(lab, return_inverse=True)
        self.vcomp = comp.reshape(-1)
        self.ncomp = int(self.vcomp.max()) + 1
        self.tri_comp = self.vcomp[self.tris[:, 0]]
        # unique welded edges, with face counts (1 = open border)
        und = np.sort(e, 1)
        self.edges, self.edge_faces = np.unique(und, axis=0, return_counts=True)
        # directed edges, for winding consistency: an interior edge must be walked once each way
        d = np.concatenate([t[:, [0, 1]], t[:, [1, 2]], t[:, [2, 0]]])
        dk = d[:, 0] * self.nw + d[:, 1]
        u, c = np.unique(dk, return_counts=True)
        self.dir_dupe = u[c > 1]                           # same direction twice: flipped neighbour

    def weights(self):
        """Per welded vertex: dominant bone, and the weight share on cloth bones (hero config prefixes)."""
        pref = tuple(self.cfg.get("cloth_bones", []))
        self.wcloth = np.zeros(self.nw)
        self.wbone = np.full(self.nw, -1)
        self.bone_names = [b.name for b in self.arm.data.bones]
        bidx = {n: i for i, n in enumerate(self.bone_names)}
        for oi, o in enumerate(self.objs):
            gname = {g.index: g.name for g in o.vertex_groups}
            is_cloth = {gi: n.startswith(pref) for gi, n in gname.items()}
            off = self.obj_off[oi]
            wf = self.weld
            for v in o.data.vertices:
                if not v.groups:
                    continue
                best, bw, cw, tw = -1, -1.0, 0.0, 0.0
                for g in v.groups:
                    tw += g.weight
                    if is_cloth.get(g.group):
                        cw += g.weight
                    if g.weight > bw:
                        best, bw = g.group, g.weight
                w = wf[off + v.index]
                self.wcloth[w] = cw / tw if tw > 0 else 0
                self.wbone[w] = bidx.get(gname.get(best, ""), -1)

    def back_shells(self):
        """Map back-shell component -> its front component."""
        self.shell_of = {}
        W = self.wpos(self.rest)
        ctri = np.bincount(self.tri_comp, minlength=self.ncomp)
        lo = np.full((self.ncomp, 3), np.inf); hi = np.full((self.ncomp, 3), -np.inf)
        np.minimum.at(lo, self.vcomp, W); np.maximum.at(hi, self.vcomp, W)
        self.comp_lo, self.comp_hi = lo, hi
        nrm = self.vertex_normals(W)
        by = {}
        for c in range(self.ncomp):
            by.setdefault(int(ctri[c]), []).append(c)
        verts_of = self.comp_verts()
        for n, cs in by.items():
            if len(cs) < 2 or n == 0:
                continue
            cs = np.array(cs)
            cen = (lo[cs] + hi[cs]) / 2
            ext = hi[cs] - lo[cs]
            dc = np.abs(cen[:, None, :] - cen[None, :, :]).max(2)
            de = np.abs(ext[:, None, :] - ext[None, :, :]).max(2)
            ii, jj = np.nonzero(np.triu((dc < 0.001) & (de < 0.0015), 1))
            for i, j in zip(ii, jj):
                a, b = int(cs[i]), int(cs[j])
                if a in self.shell_of or b in self.shell_of:
                    continue
                va, vb = verts_of[a], verts_of[b]
                kd = KDTree(len(vb))
                for k, p in enumerate(W[vb]):
                    kd.insert(p, k)
                kd.balance()
                s = va[:: max(1, len(va) // 12)]
                near = [kd.find(W[x]) for x in s]
                if max(d for _, _, d in near) > 0.0008:
                    continue
                # b behind a (against a's normals) -> b is a's back shell
                side = np.mean([np.dot(W[vb[k]] - W[x], nrm[x]) for x, (_, k, _) in zip(s, near)])
                if side < 0:
                    self.shell_of[b] = a
                else:
                    self.shell_of[a] = b

    def comp_verts(self):
        order = np.argsort(self.vcomp, kind="stable")
        bounds = np.searchsorted(self.vcomp[order], np.arange(self.ncomp + 1))
        return [order[bounds[c]:bounds[c + 1]] for c in range(self.ncomp)]

    def comp_tris(self):
        order = np.argsort(self.tri_comp, kind="stable")
        bounds = np.searchsorted(self.tri_comp[order], np.arange(self.ncomp + 1))
        return [order[bounds[c]:bounds[c + 1]] for c in range(self.ncomp)]

    # --------------------------------------------------------------------------------------- geometry
    def tri_geom(self, W):
        t = self.tris
        a, b, c = W[t[:, 0]], W[t[:, 1]], W[t[:, 2]]
        cr = np.cross(b - a, c - a)
        area2 = np.linalg.norm(cr, axis=1)
        n = cr / np.maximum(area2, 1e-20)[:, None]
        return n, area2 / 2

    def vertex_normals(self, W):
        n, area = self.tri_geom(W)
        vn = np.zeros((self.nw, 3))
        for k in range(3):
            np.add.at(vn, self.tris[:, k], n * area[:, None])
        return vn / np.maximum(np.linalg.norm(vn, axis=1), 1e-20)[:, None]

    def bvh(self, W, tri_mask=None):
        idx = np.nonzero(~self.degen if tri_mask is None else (tri_mask & ~self.degen))[0]
        tree = BVHTree.FromPolygons(W.tolist(), self.tris[idx].tolist(), all_triangles=True)
        return tree, idx

    # --------------------------------------------------------------------------------------- pieces
    def name_pieces(self, index_path):
        """Name every component after the designer piece whose surface samples it lies on (mm score kept)."""
        self.piece_name, self.piece_score = {}, {}
        if not index_path or not os.path.exists(index_path):
            return
        idx = json.load(open(index_path))["pieces"]
        trees = []
        idx = [p for p in idx if not p["name"].startswith(self.cfg["hero"] + ".")]     # Valve's own meshes in the blend
        for p in idx:
            pts = np.array(p["samples"])
            kd = KDTree(len(pts))
            for k, q in enumerate(pts):
                kd.insert(q, k)
            kd.balance()
            # how far apart this piece's samples are: a component on a sparsely sampled big piece sits that far
            # from its nearest sample even when it is the piece
            sp = float(np.median([kd.find_n(q, 2)[1][2] for q in pts[:: max(1, len(pts) // 60)]])) if len(pts) > 1 else 0.0
            trees.append((np.array(p["lo"]) - 0.02, np.array(p["hi"]) + 0.02, kd, p, sp))
        W = self.wpos(self.rest)
        verts_of = self.comp_verts()
        for c in range(self.ncomp):
            if self.comp_cls[c] not in ("outfit", "cloth"):     # Valve's body and kit keep material@bone labels
                continue
            vs = verts_of[c]
            if len(vs) == 0:
                continue
            s = W[vs[:: max(1, len(vs) // 24)]]
            cen = s.mean(0)
            cext = self.comp_hi[c] - self.comp_lo[c]
            best, bscore = None, 1e9
            for lo, hi, kd, p, sp in trees:
                if np.any(cen < lo) or np.any(cen > hi):
                    continue
                d = np.mean([kd.find(q)[2] for q in s]) - 0.5 * sp
                # a loose part is never bigger than its piece: a ring-shaped flounce and the bed inside it share a
                # centre, and only the flounce is big enough to hold the flounce's part
                d += 0.25 * float(np.maximum(cext - (np.array(p["hi"]) - np.array(p["lo"])) - 0.01, 0).max())
                if d < bscore:
                    best, bscore = p["name"], d
            if best is not None and bscore < 0.03:
                self.piece_name[c] = best
                self.piece_score[c] = bscore
            elif best is not None and bscore < 0.08:
                # a likely match (a big piece sampled sparsely, or a part the decimation moved): marked with "?"
                self.piece_name[c] = best + "?"
                self.piece_score[c] = bscore
        # loose parts of one designer piece are one piece: how they meet is the designer's construction
        first = {}
        for c in sorted(self.piece_name):
            r = int(self.piece[c])
            nm = self.piece_name.get(r, self.piece_name[c])
            first.setdefault(nm, r)
        for c in range(self.ncomp):
            r = int(self.piece[c])
            nm = self.piece_name.get(r)
            if nm is not None:
                self.piece[c] = first[nm]

    def contact_length(self, pairs, seg):
        """Intersection curve length for triangle pairs between two pieces: a curve of length L crosses about
        L / size triangles on each side, so take the smaller of the two sides' summed triangle sizes."""
        t = pairs.reshape(-1)
        p = self.piece[self.tri_comp[t]]
        lo = p == p.min()
        a = np.unique(t[lo]); b = np.unique(t[~lo])
        if len(b) == 0:
            return float(seg[a].sum())
        return float(min(seg[a].sum(), seg[b].sum()))

    def label(self, c):
        c = int(self.piece[c])
        nm = self.piece_name.get(c)
        cls = self.comp_cls[c]
        if nm:
            return "%s (%s)" % (nm, cls)
        lo, hi = self.comp_lo[c], self.comp_hi[c]
        mats = self.tri_mat[self.tri_comp == c]
        mat = self.mat_names[np.bincount(mats).argmax()] if len(mats) else "?"
        return "%s:%s@%s#%d (%.0f cm)" % (cls, mat, self.bone_label(c), c, 100 * np.linalg.norm(hi - lo))

    def bone_label(self, c):
        vs = np.nonzero(self.vcomp == c)[0]
        b = self.wbone[vs]
        b = b[b >= 0]
        if len(b) == 0:
            return "?"
        return self.bone_names[np.bincount(b).argmax()]

    def region(self, wverts):
        b = self.wbone[wverts]
        b = b[b >= 0]
        return self.bone_names[np.bincount(b).argmax()] if len(b) else "?"


# ------------------------------------------------------------------------------------------- cameras
def body_bounds(model, W=None):
    """Bounds of Valve's body (skin + gear): identical for ours and Valve's, so cameras match pixel for pixel."""
    W = model.wpos(model.rest) if W is None else W
    sel = np.isin(model.comp_cls[model.vcomp], ["skin", "gear"])
    P = W[sel]
    return P.min(0), P.max(0)


def pelvis_shift(arm):
    if "pelvis" not in arm.pose.bones:
        return Vector((0, 0, 0))
    return (arm.matrix_world @ arm.pose.bones["pelvis"].matrix.translation) - (arm.matrix_world @ arm.data.bones["pelvis"].head_local)


CAMS = ("gameplay", "front", "side", "back", "below", "three_quarter")


def camera_spec(view, lo, hi, shift=Vector((0, 0, 0))):
    """(type, location, target or rotation, ortho scale or lens). gameplay = behind and above the right shoulder."""
    lo, hi = Vector(lo), Vector(hi)
    c = (lo + hi) / 2 + shift
    l = lo + shift
    H = hi.z - lo.z
    size = max(hi - lo)
    if view == "gameplay":
        return ("PERSP", Vector((c.x - 0.95, c.y + 4.0, l.z + H * 1.05)), Vector((c.x + 0.10, c.y, l.z + H * 0.56)), 50)
    if view == "below":
        return ("PERSP", Vector((c.x + 0.7, c.y - 3.3, l.z - H * 0.42)), Vector((c.x, c.y, l.z + H * 0.47)), 50)
    yaw = {"front": 0, "three_quarter": 35, "side": 90, "back": 180}[view]
    a = math.radians(yaw)
    d = size * 3
    loc = c + Vector((math.sin(a) * d, -math.cos(a) * d, 0))
    return ("ORTHO", loc, c, size * 1.25)


def place_camera(sc, view, lo, hi, shift=Vector((0, 0, 0)), res=480):
    cam = sc.objects.get("qa_cam")
    if cam is None:
        cam = bpy.data.objects.new("qa_cam", bpy.data.cameras.new("qa_cam"))
        sc.collection.objects.link(cam)
    sc.camera = cam
    kind, loc, tgt, k = camera_spec(view, lo, hi, shift)
    cam.data.type = kind
    if kind == "ORTHO":
        cam.data.ortho_scale = k
    else:
        cam.data.lens = k
    cam.data.clip_start, cam.data.clip_end = 0.01, 100
    cam.location = loc
    cam.rotation_euler = (tgt - loc).to_track_quat("-Z", "Y").to_euler()
    sc.render.resolution_x, sc.render.resolution_y = res, int(res * 1.5)
    sc.render.resolution_percentage = 100
    bpy.context.view_layer.update()
    return cam


def project(sc, cam, P):
    """World points -> pixel coordinates (x right, y down) and depth, for the scene's render size."""
    from bpy_extras.object_utils import world_to_camera_view
    W, H = sc.render.resolution_x, sc.render.resolution_y
    out = []
    for p in P:
        v = world_to_camera_view(sc, cam, Vector(p))
        out.append((v.x * W, (1 - v.y) * H, v.z))
    return np.array(out)


def visible(tree, cam_loc, P, tol=0.004):
    """Which points the camera sees: the first hit along the ray to each point is the point itself."""
    cam_loc = Vector(cam_loc)
    vis = np.zeros(len(P), bool)
    for i, p in enumerate(P):
        p = Vector(p)
        d = p - cam_loc
        L = d.length
        hit = tree.ray_cast(cam_loc, d / L, L + tol)
        vis[i] = hit[0] is None or hit[3] >= L - tol
    return vis


# ------------------------------------------------------------------------------------------- render helpers
def workbench(sc, color="TEXTURE", res=480, xray=False, cavity=False, bg=(0.035, 0.035, 0.04)):
    sc.render.engine = "BLENDER_WORKBENCH"
    sh = sc.display.shading
    sh.light = "STUDIO"
    sh.color_type = color
    sh.show_xray = xray
    sh.xray_alpha = 0.25
    sh.show_cavity = cavity
    sh.show_shadows = False
    sh.show_backface_culling = True
    sh.show_object_outline = False
    sc.display.render_aa = "FXAA" if DRAFT else "8"
    if sc.world is None:
        sc.world = bpy.data.worlds.new("w")
    sc.world.color = bg
    sc.render.film_transparent = False
    sc.render.image_settings.file_format = "PNG"
    sc.view_settings.view_transform = "Standard"


def flat_material(name, rgba, emit=False):
    m = bpy.data.materials.get(name) or bpy.data.materials.new(name)
    m.diffuse_color = rgba
    m.use_nodes = True
    p = m.node_tree.nodes.get("Principled BSDF")
    if p:
        p.inputs["Base Color"].default_value = rgba
        if emit:
            p.inputs["Emission Color"].default_value = rgba
            p.inputs["Emission Strength"].default_value = 3.0
    return m


def highlight_object(name, W, tris, rgba=(1, 0.05, 0.05, 1), push=0.0015):
    """A new mesh of the given welded triangles, pushed out along their normals, in a flat colour."""
    old = bpy.data.objects.get(name)
    if old:
        bpy.data.objects.remove(old)
    if len(tris) == 0:
        return None
    t = np.asarray(tris)
    a, b, c = W[t[:, 0]], W[t[:, 1]], W[t[:, 2]]
    n = np.cross(b - a, c - a)
    n /= np.maximum(np.linalg.norm(n, axis=1), 1e-20)[:, None]
    verts = np.concatenate([a, b, c]) + np.concatenate([n, n, n]) * push
    k = len(t)
    faces = np.stack([np.arange(k), np.arange(k) + k, np.arange(k) + 2 * k], 1)
    me = bpy.data.meshes.new(name)
    me.from_pydata(verts.tolist(), [], faces.tolist())
    ob = bpy.data.objects.new(name, me)
    bpy.context.scene.collection.objects.link(ob)
    me.materials.append(flat_material(name + "_m", rgba))
    ob.color = rgba
    return ob


def aim_closeup(cam, W, tris_all, flagged, tree, idx, prefer=None, dist=0.45, radius=0.06):
    """Point a perspective camera at one cluster of flagged triangles, from a direction that sees it.

    The cluster is every flagged triangle within `radius` of the biggest one. Directions are tried in order (the
    preferred one, then the six axes) and the first whose ray from the camera reaches the cluster wins. Returns the
    cluster's triangles (to highlight) and whether a clear view was found."""
    flagged = np.asarray(flagged)
    cen = W[tris_all[flagged]].mean(1)
    a = W[tris_all[flagged, 1]] - W[tris_all[flagged, 0]]
    b = W[tris_all[flagged, 2]] - W[tris_all[flagged, 0]]
    cr = np.cross(a, b)
    ar = np.linalg.norm(cr, axis=1)
    seed = int(np.argmax(ar))
    near = np.linalg.norm(cen - cen[seed], axis=1) < radius
    cl = flagged[near]
    tgt = Vector(cen[near].mean(0))
    nrm = (cr[near]).sum(0)
    nrm = Vector(nrm / max(np.linalg.norm(nrm), 1e-12))
    dirs = []
    if prefer is not None:
        dirs.append(Vector(prefer).normalized())
    dirs += [nrm, -nrm, Vector((0, 1, 0)), Vector((0, -1, 0)), Vector((1, 0, 0)), Vector((-1, 0, 0)),
             Vector((0, 0, 1)), Vector((0, 0, -1))]
    clset = set(int(t) for t in cl)
    # far enough to see the whole cluster with some context around it
    ext = float(np.linalg.norm(cen[near] - np.array(tgt), axis=1).max()) if near.sum() > 1 else 0.0
    dist = max(dist, 2.8 * ext + 0.15)
    ok = False
    for d in dirs:
        loc = tgt + d * dist
        v = tgt - loc
        hit = tree.ray_cast(loc, v.normalized(), dist + 0.01)
        if hit[0] is None or int(idx[hit[2]]) in clset or (hit[0] - tgt).length < 0.012:
            ok = True
            break
    cam.data.type = "PERSP"
    cam.data.lens = 50
    cam.location = loc
    cam.rotation_euler = (tgt - loc).to_track_quat("-Z", "Y").to_euler()
    return cl, ok


def texel_density(M, W=None):
    """Per triangle: texture px per metre (colour texture size x sqrt(UV area / world area)), and the palette mask
    (components whose UVs are collapsed onto one swatch by design)."""
    W = M.wpos(M.rest) if W is None else W
    _, area = M.tri_geom(W)
    texres = {}
    for o in M.objs:
        for s in o.material_slots:
            mat = s.material
            if not mat or not mat.use_nodes:
                continue
            best = 0
            for nd in mat.node_tree.nodes:
                if nd.type == "TEX_IMAGE" and nd.image is not None:
                    try:
                        best = max(best, max(nd.image.size))
                    except Exception:
                        pass
            texres[mat.name] = best
    uv = M.tri_uv.astype(np.float64)
    uva = 0.5 * np.abs((uv[:, 1, 0] - uv[:, 0, 0]) * (uv[:, 2, 1] - uv[:, 0, 1]) - (uv[:, 2, 0] - uv[:, 0, 0]) * (uv[:, 1, 1] - uv[:, 0, 1]))
    res_t = np.array([texres.get(M.mat_names[i], 0) for i in range(len(M.mat_names))])[M.tri_mat].astype(float)
    dens = np.sqrt(uva * res_t ** 2 / np.maximum(area, 1e-12))
    coll = (uva < 1e-12) & (area > 1e-7)
    share = np.bincount(M.tri_comp, weights=coll.astype(float), minlength=M.ncomp) / np.maximum(np.bincount(M.tri_comp, minlength=M.ncomp), 1)
    palette = (share > 0.9)[M.tri_comp]
    return dens, palette, texres


def set_corner_colors(M, tri_rgba, name="qa_col"):
    """Write one RGBA per triangle as a corner colour attribute on every render mesh and make it active."""
    off = 0
    for oi, lt in M.tri_loops:
        o = M.objs[oi]
        me = o.data
        a = me.color_attributes.get(name) or me.color_attributes.new(name, "FLOAT_COLOR", "CORNER")
        col = np.zeros((len(me.loops), 4), np.float32)
        col[:, 3] = 1
        n = len(lt)
        col[lt.reshape(-1)] = np.repeat(tri_rgba[off:off + n], 3, axis=0)
        a.data.foreach_set("color", col.reshape(-1))
        me.color_attributes.active_color = a
        try:
            me.color_attributes.render_color_index = me.color_attributes.active_color_index
        except Exception:
            pass
        off += n


def heat(x):
    """0..1 -> blue, cyan, green, yellow, red."""
    x = np.clip(np.asarray(x, float), 0, 1)
    stops = np.array([[0.1, 0.2, 0.9], [0.0, 0.8, 0.9], [0.1, 0.85, 0.2], [0.95, 0.85, 0.1], [0.95, 0.1, 0.1]])
    f = x * 4
    i = np.minimum(f.astype(int), 3)
    t = (f - i)[..., None]
    return stops[i] * (1 - t) + stops[i + 1] * t


def bones_object(arm, name="qa_bones", radius=0.006):
    """The skeleton as real geometry in the current pose: octahedra parent->child, joint markers, coloured by
    chain (core white, left cyan, right magenta, cloth/soft amber, helpers grey)."""
    old = bpy.data.objects.get(name)
    if old:
        bpy.data.objects.remove(old)
    def cls(n):
        n = n.lower()
        if any(k in n for k in ("iktarget", "offset", "attach", "root_motion", "object_", "weapon")):
            return 4
        if any(k in n for k in ("hair", "cloth", "jiggle", "skirt", "coat", "cape", "rope", "tail", "strap", "maid_", "necktie", "wrap", "$cc")):
            return 3
        if n.endswith("_l") or "_l_" in n:
            return 1
        if n.endswith("_r") or "_r_" in n:
            return 2
        return 0
    verts, faces, mi = [], [], []
    def octa(h, t, c, r):
        d = t - h
        L_ = d.length
        if L_ < 1e-5:
            return
        z = d.normalized(); x = z.orthogonal().normalized(); y = z.cross(x)
        r = min(L_ * 0.12, r)
        mid = h + d * 0.18
        b = len(verts)
        verts.extend([h, mid + x * r, mid + y * r, mid - x * r, mid - y * r, t])
        for a1, a2 in ((1, 2), (2, 3), (3, 4), (4, 1)):
            faces.append((b, b + a1, b + a2)); mi.append(c)
            faces.append((b + 5, b + a2, b + a1)); mi.append(c)
    def joint(p, c, r):
        b = len(verts)
        verts.extend([p + Vector((0, 0, r)), p + Vector((r, 0, 0)), p + Vector((0, r, 0)), p + Vector((-r, 0, 0)), p + Vector((0, -r, 0)), p + Vector((0, 0, -r))])
        for a1, a2 in ((1, 2), (2, 3), (3, 4), (4, 1)):
            faces.append((b, b + a1, b + a2)); mi.append(c)
            faces.append((b + 5, b + a2, b + a1)); mi.append(c)
    mw = arm.matrix_world
    for pb in arm.pose.bones:
        h = mw @ pb.head
        c = cls(pb.name)
        joint(h, c, radius * (0.6 if c == 4 else 1.0))
        if pb.parent is not None and c != 4 and cls(pb.parent.name) != 4:
            octa(mw @ pb.parent.head, h, c, radius * 2)
    me = bpy.data.meshes.new(name)
    me.from_pydata([tuple(v) for v in verts], [], faces)
    ob = bpy.data.objects.new(name, me)
    bpy.context.scene.collection.objects.link(ob)
    cols = [(0.95, 0.95, 0.95, 1), (0.15, 0.85, 1.0, 1), (1.0, 0.25, 0.75, 1), (1.0, 0.72, 0.15, 1), (0.45, 0.45, 0.45, 1)]
    for k, c in enumerate(cols):
        me.materials.append(flat_material("qa_bone_%d" % k, c))
    me.polygons.foreach_set("material_index", mi)
    return ob


def cam_bounds(arm):
    """Framing bounds from the skeleton at rest: the same bones in ours and Valve's, so matched cameras are pixel for
    pixel the same. Helper bones (IK targets, attachments at the origin) are left out; the box is padded to the skin."""
    pts = []
    for b in arm.data.bones:
        n = b.name.lower()
        if any(k in n for k in ("iktarget", "offset", "attach", "root_motion", "object_", "weapon", "maid_", "$cc")):
            continue
        pts.append(arm.matrix_world @ b.head_local)
    P = np.array([tuple(p) for p in pts])
    lo, hi = P.min(0), P.max(0)
    H = hi[2] - lo[2]
    lo = lo - np.array([0.08, 0.08, 0.0]) * H / 2.4 - np.array([0, 0, 0.04])
    hi = hi + np.array([0.08, 0.08, 0.10]) * H / 2.4
    return lo, hi


def tri_colours(M):
    """Base colour per triangle at its UV centroid, from each material's base-colour texture (palettes are small)."""
    col = np.full((len(M.tris), 3), np.nan)
    uvc = M.tri_uv.mean(1) % 1.0
    done = set()
    for o in M.objs:
        for sl in o.material_slots:
            mat = sl.material
            if not mat or mat.name in done or not mat.use_nodes:
                continue
            done.add(mat.name)
            for nd in mat.node_tree.nodes:
                base = any(l.to_socket.name == "Base Color" for o_ in nd.outputs for l in o_.links)
                if nd.type == "TEX_IMAGE" and base and nd.image is not None and max(nd.image.size) <= 2048:
                    w_, h_ = nd.image.size
                    px_ = np.array(nd.image.pixels[:], np.float32).reshape(h_, w_, -1)
                    sel = np.array([M.mat_names[i] == mat.name for i in M.tri_mat])
                    u = uvc[sel]
                    col[sel] = px_[(u[:, 1] * (h_ - 1)).astype(int), (u[:, 0] * (w_ - 1)).astype(int), :3]
    return col

def two_faced_shells(M, col=None, threshold=0.15):
    """Two-faced sheets: a back shell whose colour differs from its front's (navy outside, sky inside). Returns the
    INNER face of each pair (the one facing the body's axis), keyed by component: seeing it is seeing the lining.
    A same-colour shell (an open sheet's plain back face) is harmless to see and is not returned."""
    col = tri_colours(M) if col is None else col
    out = {}
    for b, f in M.shell_of.items():
        cb = np.nanmean(col[M.tri_comp == b], 0) if (M.tri_comp == b).any() else None
        cf = np.nanmean(col[M.tri_comp == f], 0) if (M.tri_comp == f).any() else None
        if cb is None or cf is None or np.isnan(cb).any() or np.isnan(cf).any():
            continue
        d = float(np.abs(cb - cf).max())
        if d <= threshold:
            continue
        # which of the two faces looks inward (toward the body's vertical axis) is the lining; which one the export
        # called "front" depends on the piece's winding (a mirrored piece faces the other way)
        W = M.wpos(M.rest)
        n, area = M.tri_geom(W)
        axis = np.array([np.mean(W[:, 0]), np.mean(W[:, 1])])
        def inward(c):
            t = np.nonzero(M.tri_comp == c)[0]
            cen = W[M.tris[t]].mean(1)
            out_dir = np.zeros_like(cen); out_dir[:, :2] = cen[:, :2] - axis
            out_dir /= np.maximum(np.linalg.norm(out_dir, axis=1), 1e-9)[:, None]
            return float(np.average((n[t] * out_dir).sum(1), weights=area[t]))
        inner, outer = (b, f) if inward(b) < inward(f) else (f, b)
        ci = cb if inner == b else cf
        co = cf if inner == b else cb
        out[int(inner)] = {"outside": int(outer), "lining_rgb": [round(float(x), 2) for x in ci],
                           "outside_rgb": [round(float(x), 2) for x in co], "diff": round(d, 2)}
    return out
