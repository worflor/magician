"""Pose a compiled model without Blender: numpy and scipy only (python -I posed_np.py is not a script; import it).

The QA motion step (qa_motion.py) poses the compiled model in Blender 5.2: 30 to 40 minutes for 48 clips x 10 frames,
most of it the depsgraph and per-frame BVH builds. Everything a fit measurement needs is linear algebra on the export
Source 2 Viewer already makes, so it is done here directly:

  Glb       Source 2 Viewer's glb (our compiled mesh, Valve's skeleton, our clips when staged as run_qa.py stages them):
            accessors, the one skin, the node tree and every animation (LINEAR, translation/rotation/scale).
  Rig       rest and posed node matrices; skin matrices (joint world x inverse bind); a clip sampled at a frame (30 fps,
            the frame numbers Blender's importer gives, so they match qa_motion.py's).
  Model     the render meshes in Blender's space (Z up, Lash faces -Y, metres: the space of the designer's blend and of
            lash_body), welded per mesh by exact position (1e-5), components, classes (hero config), back shells and the
            designer's piece names (pieces.json): the same rules as qa_lib.Model, so labels match the QA reports.
            skin(M, rest=None, weights=None) poses any rest positions / weights by linear blend skinning, which is how a
            solver tries a change without rebuilding: the compiled vertex is exactly the skinned rest vertex.
  TriGrid   a uniform-grid ray caster (rest ties: a ray along each vertex normal to the first surface of another piece).
  Camera / raster   the QA cameras (qa_lib.camera_spec: gameplay, front, back, ...) and a z-buffer with triangle ids,
            back faces culled as the game culls them: visibility in pixels, and flat-shaded proof renders (PNG).
"""
import json, struct, math, os, zlib
import numpy as np
from scipy.spatial import cKDTree
from scipy.sparse import coo_matrix
from scipy.sparse.csgraph import connected_components

G2B = np.array([[1.0, 0, 0], [0, 0, -1.0], [0, 1.0, 0]])     # glTF (Y up) -> Blender (Z up), what the importer does
FPS = 30.0


# ------------------------------------------------------------------------------------------------ glb
class Glb:
    def __init__(self, path):
        with open(path, "rb") as f:
            magic, ver, length = struct.unpack("<III", f.read(12))
            jlen, _ = struct.unpack("<II", f.read(8))
            self.j = json.loads(f.read(jlen))
            blen, _ = struct.unpack("<II", f.read(8))
            self.bin = f.read(blen)
        self.path = path

    def acc(self, i, as_float=True):
        a = self.j["accessors"][i]
        bv = self.j["bufferViews"][a["bufferView"]]
        dt = {5126: np.float32, 5125: np.uint32, 5123: np.uint16, 5121: np.uint8, 5122: np.int16, 5120: np.int8}[a["componentType"]]
        nc = {"SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4, "MAT4": 16}[a["type"]]
        off = bv.get("byteOffset", 0) + a.get("byteOffset", 0)
        isz = np.dtype(dt).itemsize
        stride = bv.get("byteStride") or nc * isz
        if stride == nc * isz:
            arr = np.frombuffer(self.bin, dt, a["count"] * nc, off).reshape(a["count"], nc)
        else:
            arr = np.lib.stride_tricks.as_strided(np.frombuffer(self.bin, np.uint8, offset=off).view(dt),
                                                  (a["count"], nc), (stride, isz)).copy()
        if a.get("normalized") and as_float:
            arr = arr.astype(np.float64) / float(np.iinfo(dt).max)
        return arr


def quat_mat(q):
    """(n,4) xyzw -> (n,3,3)."""
    q = q / np.maximum(np.linalg.norm(q, axis=1, keepdims=True), 1e-20)
    x, y, z, w = q[:, 0], q[:, 1], q[:, 2], q[:, 3]
    return np.stack([np.stack([1 - 2 * (y * y + z * z), 2 * (x * y - z * w), 2 * (x * z + y * w)], 1),
                     np.stack([2 * (x * y + z * w), 1 - 2 * (x * x + z * z), 2 * (y * z - x * w)], 1),
                     np.stack([2 * (x * z - y * w), 2 * (y * z + x * w), 1 - 2 * (x * x + y * y)], 1)], 1)


class Rig:
    def __init__(self, g):
        nodes = g.j["nodes"]
        self.n = len(nodes)
        self.names = [nd.get("name", "") for nd in nodes]
        self.parent = np.full(self.n, -1)
        for i, nd in enumerate(nodes):
            for c in nd.get("children", []):
                self.parent[c] = i
        order, seen = [], set()
        def visit(i):
            if i in seen:
                return
            if self.parent[i] >= 0:
                visit(self.parent[i])
            seen.add(i); order.append(i)
        for i in range(self.n):
            visit(i)
        self.order = order
        self.T = np.array([nd.get("translation", [0, 0, 0]) for nd in nodes], float)
        self.R = np.array([nd.get("rotation", [0, 0, 0, 1]) for nd in nodes], float)
        self.S = np.array([nd.get("scale", [1, 1, 1]) for nd in nodes], float)
        for i, nd in enumerate(nodes):
            if "matrix" in nd:
                raise SystemExit("node %d has a matrix; TRS expected" % i)
        sk = g.j["skins"][0]
        self.joints = np.array(sk["joints"])
        self.ibm = g.acc(sk["inverseBindMatrices"]).reshape(-1, 4, 4).transpose(0, 2, 1).astype(np.float64)
        self.joint_names = [self.names[i] for i in self.joints]
        # clip names as glb_clips.py gives them (and so as the QA reports and ANIM.md use them): AnimGraph2 clips by
        # file name, the model's legacy sequences as "seq.<name>", repeats "~2"
        self.clips, self.clip_kind = {}, {}
        for an in g.j.get("animations", []):
            orig = an.get("name", "")
            name, kind = (orig.rsplit("/", 1)[-1], "ag2") if "/" in orig else ("seq." + orig, "legacy")
            base, k = name, 2
            while name in self.clips:
                name = "%s~%d" % (base, k); k += 1
            ch = []
            for c in an["channels"]:
                s = an["samplers"][c["sampler"]]
                ch.append((c["target"]["node"], c["target"]["path"], s["input"], s["output"]))
            self.clips[name] = ch
            self.clip_kind[name] = kind
        self._g = g
        self._cache = {}

    def clip_data(self, name):
        if name not in self._cache:
            out = []
            for node, path, ia, oa in self.clips[name]:
                t = self._g.acc(ia).reshape(-1).astype(np.float64)
                v = self._g.acc(oa).astype(np.float64)
                out.append((node, path, t, v))
            self._cache = {name: out}           # one clip at a time
        return self._cache[name]

    def clip_frames(self, name):
        """(first, last) frame at 30 fps, as Blender's importer keys them."""
        t1 = 0.0
        for node, path, ia, oa in self.clips[name]:
            a = self._g.j["accessors"][ia]
            t1 = max(t1, a.get("max", [0])[0])
        return 0, int(round(t1 * FPS))

    def local(self, T, R, S):
        M = np.zeros((self.n, 4, 4))
        M[:, :3, :3] = quat_mat(R) * S[:, None, :]
        M[:, :3, 3] = T
        M[:, 3, 3] = 1
        return M

    def world(self, T=None, R=None, S=None):
        L = self.local(self.T if T is None else T, self.R if R is None else R, self.S if S is None else S)
        W = np.empty_like(L)
        for i in self.order:
            p = self.parent[i]
            W[i] = L[i] if p < 0 else W[p] @ L[i]
        return W

    def pose(self, clip=None, frame=0):
        """Node world matrices (glTF space) at a clip frame; None = the rest pose (every node at its file TRS)."""
        T, R, S = self.T.copy(), self.R.copy(), self.S.copy()
        if clip is not None:
            t = frame / FPS
            for node, path, ts, vs in self.clip_data(clip):
                if len(ts) == 1 or t <= ts[0]:
                    v = vs[0]
                elif t >= ts[-1]:
                    v = vs[-1]
                else:
                    k = int(np.searchsorted(ts, t, side="right")) - 1
                    a = (t - ts[k]) / max(ts[k + 1] - ts[k], 1e-12)
                    v0, v1 = vs[k], vs[k + 1]
                    if path == "rotation":
                        if np.dot(v0, v1) < 0:
                            v1 = -v1
                        v = v0 * (1 - a) + v1 * a
                    else:
                        v = v0 * (1 - a) + v1 * a
                if path == "translation":
                    T[node] = v
                elif path == "rotation":
                    R[node] = v
                elif path == "scale":
                    S[node] = v
        return self.world(T, R, S)

    def skin_mats(self, W):
        """(J,3,4) skin matrices in Blender's space: B M_gl B^-1 applied to Blender points."""
        M = W[self.joints] @ self.ibm
        out = np.empty((len(M), 3, 4))
        out[:, :, :3] = G2B @ M[:, :3, :3] @ G2B.T
        out[:, :, 3] = M[:, :3, 3] @ G2B.T
        return out

    def joint_pos(self, W, name):
        i = self.names.index(name)
        return G2B @ W[i, :3, 3]


# ------------------------------------------------------------------------------------------------ model
def mesh_key(name):
    return name.split(".", 1)[-1] if "." in name else name


class Model:
    """The render meshes, Blender space, welded and classed as qa_lib.Model does."""

    def __init__(self, g, rig, cfg, materials=None, weld_tol=1e-5):
        """materials: {mesh index: [material name per primitive]} (Source 2 Viewer's glb has none without
        --gltf_export_materials; see prim_materials())."""
        self.cfg, self.rig = cfg, rig
        cls_mesh, cls_mat, cls_default = cfg["classes"]["mesh"], cfg["classes"]["material"], cfg["classes"]["default"]
        nodes = g.j["nodes"]
        mesh_nodes = [i for i in g.j["scenes"][0]["nodes"] if "mesh" in nodes[i]]
        P, J, Wt, NRM, tris, tri_obj, tri_mat = [], [], [], [], [], [], []
        self.obj_names, self.mat_names = [], []
        off = 0
        vert_obj = []
        for oi, ni in enumerate(mesh_nodes):
            mi = nodes[ni]["mesh"]
            me = g.j["meshes"][mi]
            self.obj_names.append(me.get("name", nodes[ni].get("name", "")))
            base = {}
            for k, pr in enumerate(me["primitives"]):
                pa = pr["attributes"]["POSITION"]
                if pa not in base:
                    base[pa] = off
                    p = g.acc(pa).astype(np.float64)
                    P.append(p @ G2B.T)
                    NRM.append(g.acc(pr["attributes"]["NORMAL"]).astype(np.float64) @ G2B.T)
                    J.append(g.acc(pr["attributes"]["JOINTS_0"], as_float=False).astype(np.int64))
                    w = g.acc(pr["attributes"]["WEIGHTS_0"]).astype(np.float64)
                    Wt.append(w / np.maximum(w.sum(1, keepdims=True), 1e-12))
                    vert_obj.append(np.full(len(p), oi))
                    off += len(p)
                idx = g.acc(pr["indices"], as_float=False).reshape(-1, 3).astype(np.int64) + base[pa]
                tris.append(idx)
                tri_obj.append(np.full(len(idx), oi))
                mname = (materials or {}).get(mi, [None] * len(me["primitives"]))[k] or "none"
                if mname not in self.mat_names:
                    self.mat_names.append(mname)
                tri_mat.append(np.full(len(idx), self.mat_names.index(mname)))
        self.rest = np.concatenate(P)
        self.rest_nrm = np.concatenate(NRM)
        self.J = np.concatenate(J)
        self.Wt = np.concatenate(Wt)
        self.vert_obj = np.concatenate(vert_obj)
        self.tris_raw = np.concatenate(tris)
        self.tri_obj = np.concatenate(tri_obj)
        self.tri_mat = np.concatenate(tri_mat)
        self.nv = len(self.rest)
        q = np.round(self.rest / weld_tol).astype(np.int64)
        key = np.concatenate([q, self.vert_obj[:, None]], 1)
        _, first, inv = np.unique(key, axis=0, return_index=True, return_inverse=True)
        self.weld = inv.reshape(-1)
        self.wfirst = first
        self.nw = len(first)
        self.tris = self.weld[self.tris_raw]
        t = self.tris
        self.degen = (t[:, 0] == t[:, 1]) | (t[:, 1] == t[:, 2]) | (t[:, 0] == t[:, 2])
        self.wobj = self.vert_obj[first]
        self._components()
        mesh_cls = [cls_mesh.get(mesh_key(n)) for n in self.obj_names]
        mc = np.array([cls_mat.get(m, cls_default) for m in self.mat_names], dtype=object)
        tc = mc[self.tri_mat]
        for oi, c in enumerate(mesh_cls):
            if c:
                tc[self.tri_obj == oi] = c
        self.tri_cls = tc
        self.comp_cls = np.empty(self.ncomp, dtype=object)
        order = np.argsort(self.tri_comp, kind="stable")
        bounds = np.searchsorted(self.tri_comp[order], np.arange(self.ncomp + 1))
        for c in range(self.ncomp):
            sel = self.tri_cls[order[bounds[c]:bounds[c + 1]]]
            if len(sel) == 0:
                self.comp_cls[c] = cls_default
                continue
            vals, cnt = np.unique(sel.astype(str), return_counts=True)
            self.comp_cls[c] = vals[cnt.argmax()]
        self._weights()
        self.back_shells()
        self.piece = np.arange(self.ncomp)
        for b, f in self.shell_of.items():
            self.piece[b] = f
        self.piece_name = {}

    # ---- skinning
    def skin(self, M, rest=None, J=None, Wt=None):
        """Raw vertex positions for skin matrices M (J,3,4): sum_k w_k (M_jk p). rest/J/Wt override the model's."""
        rest = self.rest if rest is None else rest
        J = self.J if J is None else J
        Wt = self.Wt if Wt is None else Wt
        out = np.zeros_like(rest)
        for k in range(J.shape[1]):
            Mk = M[J[:, k]]
            out += Wt[:, k:k + 1] * (np.einsum("nij,nj->ni", Mk[:, :, :3], rest) + Mk[:, :, 3])
        return out

    def wpos(self, raw):
        return raw[self.wfirst]

    # ---- topology
    def _components(self):
        t = self.tris[~self.degen]
        e = np.concatenate([t[:, [0, 1]], t[:, [1, 2]], t[:, [0, 2]]])
        g = coo_matrix((np.ones(len(e), np.int8), (e[:, 0], e[:, 1])), shape=(self.nw, self.nw))
        _, lab = connected_components(g, directed=False)
        # qa_lib numbers components by their smallest vertex: same order here
        first = np.full(lab.max() + 1, self.nw)
        np.minimum.at(first, lab, np.arange(self.nw))
        rank = np.argsort(np.argsort(first))
        self.vcomp = rank[lab]
        self.ncomp = int(self.vcomp.max()) + 1
        self.tri_comp = self.vcomp[self.tris[:, 0]]
        und = np.sort(e, 1)
        self.edges, self.edge_faces = np.unique(und, axis=0, return_counts=True)

    def _weights(self):
        pref = tuple(self.cfg.get("cloth_bones", []))
        jn = self.rig.joint_names
        is_cloth = np.array([n.startswith(pref) for n in jn])
        J, Wt = self.J[self.wfirst], self.Wt[self.wfirst]
        self.wcloth = (Wt * is_cloth[J]).sum(1)
        self.wbone = J[np.arange(len(J)), Wt.argmax(1)]
        self.bone_names = jn

    def comp_verts(self):
        order = np.argsort(self.vcomp, kind="stable")
        bounds = np.searchsorted(self.vcomp[order], np.arange(self.ncomp + 1))
        return [order[bounds[c]:bounds[c + 1]] for c in range(self.ncomp)]

    def tri_geom(self, W):
        t = self.tris
        a, b, c = W[t[:, 0]], W[t[:, 1]], W[t[:, 2]]
        cr = np.cross(b - a, c - a)
        a2 = np.linalg.norm(cr, axis=1)
        return cr / np.maximum(a2, 1e-20)[:, None], a2 / 2

    def vertex_normals(self, W):
        n, area = self.tri_geom(W)
        vn = np.zeros((self.nw, 3))
        for k in range(3):
            np.add.at(vn, self.tris[:, k], n * area[:, None])
        return vn / np.maximum(np.linalg.norm(vn, axis=1), 1e-20)[:, None]

    def back_shells(self):
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
                kd = cKDTree(W[vb])
                s = va[:: max(1, len(va) // 12)]
                d, k = kd.query(W[s])
                if d.max() > 0.0008:
                    continue
                side = np.mean(((W[vb[k]] - W[s]) * nrm[s]).sum(1))
                if side < 0:
                    self.shell_of[b] = a
                else:
                    self.shell_of[a] = b

    def name_pieces(self, index_path):
        """qa_lib.Model.name_pieces, with scipy's KD tree."""
        self.piece_name, self.piece_score = {}, {}
        if not index_path or not os.path.exists(index_path):
            return
        idx = json.load(open(index_path))["pieces"]
        idx = [p for p in idx if not p["name"].startswith(self.cfg["hero"] + ".")]
        trees = []
        for p in idx:
            pts = np.array(p["samples"])
            kd = cKDTree(pts)
            if len(pts) > 1:
                sub = pts[:: max(1, len(pts) // 60)]
                sp = float(np.median(kd.query(sub, 2)[0][:, 1]))
            else:
                sp = 0.0
            trees.append((np.array(p["lo"]) - 0.02, np.array(p["hi"]) + 0.02, kd, p, sp))
        W = self.wpos(self.rest)
        verts_of = self.comp_verts()
        for c in range(self.ncomp):
            if self.comp_cls[c] not in ("outfit", "cloth"):
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
                d = float(np.mean(kd.query(s)[0])) - 0.5 * sp
                d += 0.25 * float(np.maximum(cext - (np.array(p["hi"]) - np.array(p["lo"])) - 0.01, 0).max())
                if d < bscore:
                    best, bscore = p["name"], d
            if best is not None and bscore < 0.03:
                self.piece_name[c] = best; self.piece_score[c] = bscore
            elif best is not None and bscore < 0.08:
                self.piece_name[c] = best + "?"; self.piece_score[c] = bscore
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

    def bone_label(self, c):
        b = self.wbone[self.vcomp == c]
        return self.bone_names[np.bincount(b).argmax()] if len(b) else "?"

    def label(self, c):
        c = int(self.piece[c])
        nm = self.piece_name.get(c)
        if nm:
            return "%s (%s)" % (nm, self.comp_cls[c])
        lo, hi = self.comp_lo[c], self.comp_hi[c]
        mats = self.tri_mat[self.tri_comp == c]
        mat = self.mat_names[np.bincount(mats).argmax()] if len(mats) else "?"
        return "%s:%s@%s#%d (%.0f cm)" % (self.comp_cls[c], mat, self.bone_label(c), c, 100 * np.linalg.norm(hi - lo))

    def short(self, c):
        """The piece name without the class, or the material@bone label for Valve's body and kit."""
        c = int(self.piece[c])
        nm = self.piece_name.get(c)
        if nm:
            return nm
        mats = self.tri_mat[self.tri_comp == c]
        mat = self.mat_names[np.bincount(mats).argmax()] if len(mats) else "?"
        return "%s:%s" % (self.comp_cls[c], mat)

    def body_bounds(self, W=None):
        W = self.wpos(self.rest) if W is None else W
        sel = np.isin(self.comp_cls[self.vcomp], ["skin", "gear"])
        return W[sel].min(0), W[sel].max(0)


def prim_materials(dump_text, g):
    """Material names for mesh 0's primitives (the body mesh, where skin and kit are told apart by material) from
    `magician dump <vpk> <model> --block MDAT` (the first MDAT block is the first mesh): its draw calls in order, each
    m_nIndexCount then m_material, matched to the glb primitives by index count."""
    counts, mats, cur = [], [], None
    for line in dump_text.splitlines():
        s = line.strip()
        if s.startswith("m_nIndexCount"):
            cur = int(s.split("=")[1])
        elif s.startswith("m_material =") and cur is not None:
            m = s.split("=", 1)[1].strip().strip('"')
            counts.append(cur); mats.append(os.path.splitext(os.path.basename(m))[0]); cur = None
    prims = g.j["meshes"][0]["primitives"]
    pc = [g.j["accessors"][p["indices"]]["count"] for p in prims]
    if pc != counts[:len(pc)]:
        raise SystemExit("MDAT draw calls %s do not match mesh 0's primitives %s" % (counts, pc))
    return {0: mats[:len(pc)]}


# ------------------------------------------------------------------------------------------------ ray casting
class TriGrid:
    """Triangles in a uniform grid of cell h (padded AABBs); rays sampled every h/3 gather candidates, then an exact
    Moller-Trumbore test. For rest ties (once per model), not per frame."""

    def __init__(self, W, tris, tri_ids, h=0.02):
        self.W, self.tris, self.ids, self.h = W, tris, tri_ids, h
        A, B, C = W[tris[tri_ids, 0]], W[tris[tri_ids, 1]], W[tris[tri_ids, 2]]
        lo = np.minimum(np.minimum(A, B), C) - 0.34 * h
        hi = np.maximum(np.maximum(A, B), C) + 0.34 * h
        self.o = lo.min(0) - h
        c0 = np.floor((lo - self.o) / h).astype(np.int64)
        c1 = np.floor((hi - self.o) / h).astype(np.int64)
        self.dims = c1.max(0) + 2
        span = c1 - c0 + 1
        cnt = span.prod(1)
        rep = np.repeat(np.arange(len(tri_ids)), cnt)
        start = np.repeat(np.cumsum(cnt) - cnt, cnt)
        k = np.arange(len(rep)) - start
        sx, sy = span[rep, 0], span[rep, 1]
        cx = c0[rep, 0] + k % sx
        cy = c0[rep, 1] + (k // sx) % sy
        cz = c0[rep, 2] + k // (sx * sy)
        key = (cx * self.dims[1] + cy) * self.dims[2] + cz
        o = np.argsort(key, kind="stable")
        self.keys = key[o]
        self.cell_tri = rep[o]
        self.ukeys, self.ustart, self.ucount = np.unique(self.keys, return_index=True, return_counts=True)

    def _cell(self, P):
        c = np.floor((P - self.o) / self.h).astype(np.int64)
        c = np.clip(c, 0, self.dims - 1)
        return (c[..., 0] * self.dims[1] + c[..., 1]) * self.dims[2] + c[..., 2]

    def cast(self, O, D, tmax, ray_tag=None, tri_tag=None, chunk=3000):
        """First hit of each ray (origin O, unit direction D) within tmax whose triangle's tag differs from the ray's
        tag (ray_tag / tri_tag: piece ids; None = any). Returns (t, triangle index into tris, barycentric u, v);
        t = inf where nothing is hit."""
        n = len(O)
        tbest = np.full(n, np.inf); tri_best = np.full(n, -1); ub = np.zeros(n); vb = np.zeros(n)
        steps = int(math.ceil(tmax / (self.h / 3))) + 1
        s = np.linspace(0, tmax, steps)
        for a in range(0, n, chunk):
            b = min(n, a + chunk)
            Pc = O[a:b, None, :] + D[a:b, None, :] * s[None, :, None]
            keys = self._cell(Pc).reshape(-1)
            ray = np.repeat(np.arange(a, b), steps)
            rk = np.unique(ray.astype(np.int64) * (1 << 40) + keys)
            ray, keys = rk >> 40, rk & ((1 << 40) - 1)
            pos = np.searchsorted(self.ukeys, keys)
            pos = np.clip(pos, 0, len(self.ukeys) - 1)
            ok = self.ukeys[pos] == keys
            ray, pos = ray[ok], pos[ok]
            cnt = self.ucount[pos]
            rr = np.repeat(ray, cnt)
            st = np.repeat(self.ustart[pos], cnt)
            k = np.arange(len(rr)) - np.repeat(np.cumsum(cnt) - cnt, cnt)
            tt = self.cell_tri[st + k]
            pk = np.unique(rr * len(self.ids) + tt)
            rr, tt = pk // len(self.ids), pk % len(self.ids)
            if ray_tag is not None:
                keep = tri_tag[tt] != ray_tag[rr]
                rr, tt = rr[keep], tt[keep]
            t, u, v, hit = ray_tri(O[rr], D[rr], self.W, self.tris[self.ids[tt]])
            hit &= (t > 0) & (t <= tmax)
            rr, tt, t, u, v = rr[hit], tt[hit], t[hit], u[hit], v[hit]
            if len(rr) == 0:
                continue
            o = np.lexsort((t, rr))
            rr, tt, t, u, v = rr[o], tt[o], t[o], u[o], v[o]
            first = np.r_[True, rr[1:] != rr[:-1]]
            rr, tt, t, u, v = rr[first], tt[first], t[first], u[first], v[first]
            tbest[rr] = t; tri_best[rr] = self.ids[tt]; ub[rr] = u; vb[rr] = v
        return tbest, tri_best, ub, vb


def ray_tri(O, D, W, T):
    a, b, c = W[T[:, 0]], W[T[:, 1]], W[T[:, 2]]
    e1, e2 = b - a, c - a
    p = np.cross(D, e2)
    det = (e1 * p).sum(1)
    ok = np.abs(det) > 1e-14
    inv = np.where(ok, 1.0 / np.where(ok, det, 1), 0)
    s = O - a
    u = (s * p).sum(1) * inv
    q = np.cross(s, e1)
    v = (D * q).sum(1) * inv
    t = (e2 * q).sum(1) * inv
    hit = ok & (u >= -1e-9) & (v >= -1e-9) & (u + v <= 1 + 1e-9)
    return t, u, v, hit


# ------------------------------------------------------------------------------------------------ cameras
class Camera:
    """qa_lib.camera_spec, as numbers: 'gameplay' (behind and above the right shoulder, 50 mm), 'below', and the
    orthographic 'front', 'three_quarter', 'side', 'back'. shift = the pelvis's displacement from rest."""

    def __init__(self, view, lo, hi, shift=np.zeros(3), W=360, H=540):
        lo, hi = np.asarray(lo, float), np.asarray(hi, float)
        c = (lo + hi) / 2 + shift
        l = lo + shift
        Hb = hi[2] - lo[2]
        size = float(max(hi - lo))
        self.W, self.H, self.view = W, H, view
        if view == "gameplay":
            loc, tgt, self.kind, self.k = np.array([c[0] - 0.95, c[1] + 4.0, l[2] + Hb * 1.05]), np.array([c[0] + 0.10, c[1], l[2] + Hb * 0.56]), "P", 50.0
        elif view == "below":
            loc, tgt, self.kind, self.k = np.array([c[0] + 0.7, c[1] - 3.3, l[2] - Hb * 0.42]), np.array([c[0], c[1], l[2] + Hb * 0.47]), "P", 50.0
        else:
            a = math.radians({"front": 0, "three_quarter": 35, "side": 90, "back": 180}[view])
            d = size * 3
            loc, tgt, self.kind, self.k = c + np.array([math.sin(a) * d, -math.cos(a) * d, 0]), c, "O", size * 1.25
        f = tgt - loc; f /= np.linalg.norm(f)
        r = np.cross(f, [0, 0, 1.0]); r /= np.linalg.norm(r)
        u = np.cross(r, f)
        self.loc, self.f, self.r, self.u = loc, f, r, u
        if self.kind == "P":
            self.fpx = self.k / 36.0 * max(W, H)         # Blender sensor fit AUTO: the larger dimension
        else:
            self.mpp = self.k / max(W, H)

    def project(self, P):
        d = P - self.loc
        x, y, z = d @ self.r, d @ self.u, d @ self.f
        if self.kind == "P":
            zs = np.maximum(z, 1e-6)
            return np.stack([self.W / 2 + self.fpx * x / zs, self.H / 2 - self.fpx * y / zs, z], 1)
        return np.stack([self.W / 2 + x / self.mpp, self.H / 2 - y / self.mpp, z], 1)

    def facing(self, W, tris):
        """+1 for triangles facing the camera (drawn), -1 for back faces (culled)."""
        a, b, c = W[tris[:, 0]], W[tris[:, 1]], W[tris[:, 2]]
        n = np.cross(b - a, c - a)
        view = (a + b + c) / 3 - self.loc if self.kind == "P" else np.broadcast_to(self.f, a.shape)
        return -np.sign((n * view).sum(1))

    def px_scale(self):
        """Multiply a pixel count at this size by this to get pixels at 720x1080 (qa_motion's unit)."""
        return (720.0 / self.W) * (1080.0 / self.H)


class FrameView:
    """One camera on one posed frame, projected once: ids(window) rasterizes only the triangles whose box meets the
    window (pixel bounds x0, x1, y0, y1), which is all a visibility count for one event needs."""

    def __init__(self, cam, W, tris, cull=True, tn=None, cen=None):
        """tn, cen: the posed frame's triangle normals (unnormalised) and centroids, shared by every camera."""
        self.cam, self.W, self.tris = cam, W, tris
        self._full = None
        if tn is None:
            a, b, c = W[tris[:, 0]], W[tris[:, 1]], W[tris[:, 2]]
            tn, cen = np.cross(b - a, c - a), (a + b + c) / 3
        sel = np.nonzero(~((tris[:, 0] == tris[:, 1]) | (tris[:, 1] == tris[:, 2]) | (tris[:, 0] == tris[:, 2])))[0]
        if cull:
            view = cen[sel] - cam.loc if cam.kind == "P" else cam.f
            sel = sel[(tn[sel] * view).sum(1) < 0]
        self.S = cam.project(W)
        p = self.S[tris[sel]]                           # (n,3,3)
        ok = (p[:, :, 2] > 0.01).all(1)
        x0 = np.ceil(p[:, :, 0].min(1) - 0.5).astype(np.int64)
        x1 = np.floor(p[:, :, 0].max(1) - 0.5).astype(np.int64)
        y0 = np.ceil(p[:, :, 1].min(1) - 0.5).astype(np.int64)
        y1 = np.floor(p[:, :, 1].max(1) - 0.5).astype(np.int64)
        x0, y0 = np.maximum(x0, 0), np.maximum(y0, 0)
        x1, y1 = np.minimum(x1, cam.W - 1), np.minimum(y1, cam.H - 1)
        ok &= (x1 >= x0) & (y1 >= y0)
        self.sel = sel[ok]                              # only triangles that hold a pixel centre can be seen
        self.x0, self.x1, self.y0, self.y1 = x0[ok], x1[ok], y0[ok], y1[ok]

    def full(self):
        """The whole frame's id buffer (computed once)."""
        if self._full is None:
            self._full = _fill(self.cam.W, self.cam.H, self.S.astype(np.float32), self.tris, self.sel,
                               self.x0, self.x1, self.y0, self.y1)
        return self._full

    def window_of(self, tri_ids, pad=2):
        p = self.S[self.tris[tri_ids]]
        return (int(np.floor(p[:, :, 0].min())) - pad, int(np.ceil(p[:, :, 0].max())) + pad,
                int(np.floor(p[:, :, 1].min())) - pad, int(np.ceil(p[:, :, 1].max())) + pad)

    def ids(self, window=None):
        cam = self.cam
        x0, x1, y0, y1 = window if window else (0, cam.W - 1, 0, cam.H - 1)
        x0, y0 = max(x0, 0), max(y0, 0)
        x1, y1 = min(x1, cam.W - 1), min(y1, cam.H - 1)
        if x1 < x0 or y1 < y0:
            return np.full(0, -1), (x0, x1, y0, y1)
        m = (self.x1 >= x0) & (self.x0 <= x1) & (self.y1 >= y0) & (self.y0 <= y1)
        _, ids = raster(cam, self.W, self.tris, tri_ids=self.sel[m], cull=False, clip=(x0, x1, y0, y1))
        return ids[y0:y1 + 1, x0:x1 + 1], (x0, x1, y0, y1)


def _fill(Ww, Hh, S, tris, sel, x0, x1, y0, y1):
    """Id buffer from projected vertices S and each triangle's pixel-centre box (all already clipped and non-empty)."""
    bw, bh = x1 - x0 + 1, y1 - y0 + 1
    B_all = np.maximum(bw, bh)
    pix_l, dep_l, tri_l = [], [], []
    T = tris[sel]
    for B in (1, 2, 4, 8, 16, 32, 64, 128, 1 << 30):
        g = np.nonzero((B_all <= B) & (B_all > B // 2 if B > 1 else True))[0] if B < (1 << 30) else np.nonzero(B_all > 128)[0]
        if len(g) == 0:
            continue
        if B >= (1 << 30):
            B = int(B_all[g].max())
        oy, ox = np.divmod(np.arange(B * B), B)
        chunk = max(1, 300000 // (B * B))
        for a0 in range(0, len(g), chunk):
            gg = g[a0:a0 + chunk]
            px = x0[gg, None] + ox[None, :]
            py = y0[gg, None] + oy[None, :]
            inb = (px <= x1[gg, None]) & (py <= y1[gg, None])
            cx, cy = px.astype(np.float32) + 0.5, py.astype(np.float32) + 0.5
            A0, A1, A2 = S[T[gg, 0]], S[T[gg, 1]], S[T[gg, 2]]
            w0 = (A2[:, None, 0] - A1[:, None, 0]) * (cy - A1[:, None, 1]) - (A2[:, None, 1] - A1[:, None, 1]) * (cx - A1[:, None, 0])
            w1 = (A0[:, None, 0] - A2[:, None, 0]) * (cy - A2[:, None, 1]) - (A0[:, None, 1] - A2[:, None, 1]) * (cx - A2[:, None, 0])
            w2 = (A1[:, None, 0] - A0[:, None, 0]) * (cy - A0[:, None, 1]) - (A1[:, None, 1] - A0[:, None, 1]) * (cx - A0[:, None, 0])
            area = w0 + w1 + w2
            inside = inb & (np.abs(area) > 1e-12) & (((w0 >= 0) & (w1 >= 0) & (w2 >= 0)) | ((w0 <= 0) & (w1 <= 0) & (w2 <= 0)))
            ii = np.nonzero(inside)
            if len(ii[0]) == 0:
                continue
            r, k = ii
            ar = area[r, k]
            dep = (w0[r, k] * A0[r, 2] + w1[r, k] * A1[r, 2] + w2[r, k] * A2[r, 2]) / ar
            pix_l.append(py[r, k].astype(np.int64) * Ww + px[r, k]); dep_l.append(dep); tri_l.append(sel[gg][r])
    ids = np.full(Hh * Ww, -1)
    if pix_l:
        pix, dep, tri = np.concatenate(pix_l), np.concatenate(dep_l), np.concatenate(tri_l)
        o = np.lexsort((dep, pix))
        pix, tri = pix[o], tri[o]
        first = np.r_[True, pix[1:] != pix[:-1]]
        ids[pix[first]] = tri[first]
    return ids.reshape(Hh, Ww)


def raster(cam, W, tris, tri_mask=None, cull=True, tri_ids=None, clip=None):
    """Z-buffer with triangle ids (index into tris; -1 = background) for posed welded positions W. Back faces are
    culled (the game culls them) unless cull=False. Returns (depth (H,W), ids (H,W))."""
    Hh, Ww = cam.H, cam.W
    if tri_ids is not None:
        sel = tri_ids
    else:
        sel = np.nonzero(~((tris[:, 0] == tris[:, 1]) | (tris[:, 1] == tris[:, 2]) | (tris[:, 0] == tris[:, 2])))[0]
        if tri_mask is not None:
            sel = sel[tri_mask[sel]]
    if cull:
        sel = sel[cam.facing(W, tris[sel]) > 0]
    cx0, cx1, cy0, cy1 = clip if clip else (0, Ww - 1, 0, Hh - 1)
    if tri_ids is not None:
        used = np.unique(tris[sel].reshape(-1))
        S = np.zeros((len(W), 3)); S[used] = cam.project(W[used])
    else:
        S = cam.project(W)
    T = tris[sel]
    p0, p1, p2 = S[T[:, 0]], S[T[:, 1]], S[T[:, 2]]
    good = (p0[:, 2] > 0.01) & (p1[:, 2] > 0.01) & (p2[:, 2] > 0.01)
    # the pixel centres (i + 0.5) inside each triangle's box: most triangles cover one or none at draft size
    x0 = np.ceil(np.minimum(np.minimum(p0[:, 0], p1[:, 0]), p2[:, 0]) - 0.5).astype(np.int64)
    x1 = np.floor(np.maximum(np.maximum(p0[:, 0], p1[:, 0]), p2[:, 0]) - 0.5).astype(np.int64)
    y0 = np.ceil(np.minimum(np.minimum(p0[:, 1], p1[:, 1]), p2[:, 1]) - 0.5).astype(np.int64)
    y1 = np.floor(np.maximum(np.maximum(p0[:, 1], p1[:, 1]), p2[:, 1]) - 0.5).astype(np.int64)
    x0, y0 = np.maximum(x0, cx0), np.maximum(y0, cy0)
    x1, y1 = np.minimum(x1, cx1), np.minimum(y1, cy1)
    good &= (x1 >= x0) & (y1 >= y0)
    bw, bh = x1 - x0 + 1, y1 - y0 + 1
    allpix, alldep, alltri = [], [], []
    for B in (1, 2, 4, 8, 16, 32, 64, 128, 1 << 30):
        g = np.nonzero(good & (np.maximum(bw, bh) <= B))[0]
        good[g] = False
        if len(g) == 0:
            continue
        if B > 128:
            B = int(np.maximum(bw[g], bh[g]).max())
        for a0 in range(0, len(g), max(1, 400000 // (B * B))):
            gg = g[a0:a0 + max(1, 400000 // (B * B))]
            ox, oy = np.meshgrid(np.arange(B), np.arange(B))
            px = x0[gg, None] + ox.reshape(1, -1)
            py = y0[gg, None] + oy.reshape(1, -1)
            cx, cy = px + 0.5, py + 0.5
            A0, A1, A2 = p0[gg], p1[gg], p2[gg]
            def ef(P, Q):
                return (Q[:, None, 0] - P[:, None, 0]) * (cy - P[:, None, 1]) - (Q[:, None, 1] - P[:, None, 1]) * (cx - P[:, None, 0])
            w0, w1, w2 = ef(A1, A2), ef(A2, A0), ef(A0, A1)
            area = w0 + w1 + w2
            inside = ((w0 >= 0) & (w1 >= 0) & (w2 >= 0)) | ((w0 <= 0) & (w1 <= 0) & (w2 <= 0))
            inside &= np.abs(area) > 1e-12
            inside &= (px >= 0) & (px < Ww) & (py >= 0) & (py < Hh)
            ar = np.where(np.abs(area) > 1e-12, area, 1)
            dep = (w0 * A0[:, None, 2] + w1 * A1[:, None, 2] + w2 * A2[:, None, 2]) / ar
            ii = np.nonzero(inside)
            allpix.append(py[ii] * Ww + px[ii]); alldep.append(dep[ii]); alltri.append(sel[gg][ii[0]])
    depth = np.full(Hh * Ww, np.inf); ids = np.full(Hh * Ww, -1)
    if allpix:
        pix, dep, tri = np.concatenate(allpix), np.concatenate(alldep), np.concatenate(alltri)
        o = np.lexsort((dep, pix))
        pix, dep, tri = pix[o], dep[o], tri[o]
        first = np.r_[True, pix[1:] != pix[:-1]]
        depth[pix[first]] = dep[first]; ids[pix[first]] = tri[first]
    return depth.reshape(Hh, Ww), ids.reshape(Hh, Ww)


def write_png(path, arr):
    h, w, c = arr.shape
    ct = {1: 0, 3: 2, 4: 6}[c]
    raw = b"".join(b"\x00" + arr[y].tobytes() for y in range(h))
    def chunk(t, d):
        return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xffffffff)
    with open(path, "wb") as f:
        f.write(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, ct, 0, 0, 0))
                + chunk(b"IDAT", zlib.compress(raw, 6)) + chunk(b"IEND", b""))


def shade(cam, W, tris, ids, tri_rgb, hl=None, hl_rgb=(1.0, 0.08, 0.06), bg=(0.035, 0.035, 0.04)):
    """Flat-shaded picture from a raster: each pixel its triangle's colour times a head-light term; hl = boolean mask
    over triangles drawn in hl_rgb."""
    n = np.cross(W[tris[:, 1]] - W[tris[:, 0]], W[tris[:, 2]] - W[tris[:, 0]])
    n /= np.maximum(np.linalg.norm(n, axis=1, keepdims=True), 1e-20)
    lam = np.abs(n @ cam.f) * 0.75 + 0.25
    img = np.empty(ids.shape + (3,))
    img[:] = bg
    m = ids >= 0
    t = ids[m]
    col = tri_rgb[t] * lam[t, None]
    if hl is not None:
        col = np.where(hl[t, None], np.array(hl_rgb) * (0.6 + 0.4 * lam[t, None]), col)
    img[m] = col
    img = np.clip(img, 0, 1) ** (1 / 2.2)
    return (img * 255 + 0.5).astype(np.uint8)
