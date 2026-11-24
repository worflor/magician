"""The poke detector of qa_motion.py, without Blender: layer inversions on a posed model, with depth and visibility.

  ties      at rest every collidable vertex (not fx, not a back shell) is tied to the first surface of another piece
            along its normal, within 30 cm (the skirt over the thigh, the corset over the ribs, the apron over the skirt):
            triangle and barycentric point, and the distance.
  frame()   on a posed frame each tied vertex is tested against that triangle's plane at the tied point; crossed by more
            than the tolerance (2 mm under a garment, 1 mm between garments) and still over it (lateral offset under
            max(5 cm, the rest distance)), and a segment from the vertex back toward the surface really meets the outer
            piece near the tie: the inner surface shows through the outer one. Depth = how far through.
  visible() the events' triangles counted in a z-buffer (back faces culled) from the gameplay, front and back cameras,
            in pixels at 720x1080, as qa_motion.py scores them.

Same rules and constants as qa_motion.py, so its numbers and these can be read side by side; the ray caster and the
confirmation are numpy (posed_np.TriGrid, the outer piece's triangles within 4 cm of the tie).
"""
import numpy as np
from scipy.spatial import cKDTree
import posed_np as P

OURS_CLS = ("outfit", "cloth")
R_MAX = 0.30
NBR = 24                 # outer-piece triangles kept per tie for the confirmation segment


class Pokes:
    def __init__(self, M, log=print):
        self.M = M
        W0 = M.wpos(M.rest)
        N0 = M.vertex_normals(W0)
        fx = M.comp_cls == "fx"
        shell = np.zeros(M.ncomp, bool)
        shell[list(M.shell_of.keys())] = True
        coll_comp = ~fx & ~shell
        self.coll_tri = coll_comp[M.tri_comp] & ~M.degen
        coll_vert = coll_comp[M.vcomp]
        PIECE = M.piece
        tri_ids = np.nonzero(self.coll_tri)[0]
        grid = P.TriGrid(W0, M.tris, tri_ids, h=0.02)
        verts = np.nonzero(coll_vert & (np.linalg.norm(N0, axis=1) > 0.5))[0]
        tri_piece = PIECE[M.tri_comp[tri_ids]]
        t, tri, u, v = grid.cast(W0[verts] + N0[verts] * 1e-4, N0[verts], R_MAX,
                                 ray_tag=PIECE[M.vcomp[verts]], tri_tag=tri_piece)
        hit = np.isfinite(t)
        self.v = verts[hit]
        self.t = tri[hit]
        self.b = np.stack([1 - u[hit] - v[hit], u[hit], v[hit]], 1)
        self.d = t[hit] + 1e-4
        self.inner = PIECE[M.vcomp[self.v]]
        self.outer = PIECE[M.tri_comp[self.t]]
        cls = M.comp_cls
        self.garment = np.isin(cls[self.inner], OURS_CLS) & np.isin(cls[self.outer], OURS_CLS)
        self.skin = (cls[self.inner] == "skin") & np.isin(cls[self.outer], OURS_CLS)
        self.tol = np.where(self.garment, 0.001, 0.002)
        s0, _, _ = self.plane_side(W0)
        self.sg0 = np.where(np.sign(s0) == 0, -1, np.sign(s0))
        # the outer piece's triangles round each tie, for the confirmation segment
        cen = W0[M.tris[tri_ids]].mean(1)
        kd = cKDTree(cen)
        p_tie = self.tie_point(W0)
        dd, kk = kd.query(p_tie, k=96, distance_upper_bound=0.04)
        kk = np.where(np.isfinite(dd), kk, len(tri_ids) - 1)
        cand = tri_ids[kk]
        ok = np.isfinite(dd) & (PIECE[M.tri_comp[cand]] == self.outer[:, None])
        self.nbr = np.full((len(self.v), NBR), -1)
        for i in range(len(self.v)):
            c = cand[i][ok[i]][:NBR - 1]
            self.nbr[i, 0] = self.t[i]
            self.nbr[i, 1:1 + len(c)] = c
        log("rest ties %d of %d vertices (skin under a garment %d, garment over garment %d)"
            % (len(self.v), len(verts), int(self.skin.sum()), int(self.garment.sum())))

    def tie_point(self, W):
        tri = self.M.tris[self.t]
        return W[tri[:, 0]] * self.b[:, :1] + W[tri[:, 1]] * self.b[:, 1:2] + W[tri[:, 2]] * self.b[:, 2:3]

    def plane_side(self, W, sel=slice(None)):
        tri = self.M.tris[self.t[sel]]
        a, b, c = W[tri[:, 0]], W[tri[:, 1]], W[tri[:, 2]]
        bb = self.b[sel]
        p = a * bb[:, :1] + b * bb[:, 1:2] + c * bb[:, 2:3]
        n = np.cross(b - a, c - a)
        n /= np.maximum(np.linalg.norm(n, axis=1), 1e-20)[:, None]
        off = W[self.v[sel]] - p
        s = (off * n).sum(1)
        lat = np.linalg.norm(off - s[:, None] * n, axis=1)
        return s, lat, n

    def frame(self, W):
        """Per tie: depth through the outer surface where confirmed (0 elsewhere). W = welded posed positions."""
        s, lat, n = self.plane_side(W)
        depth = -self.sg0 * s
        cand = np.nonzero((depth > self.tol) & (lat < np.maximum(0.05, self.d)))[0]
        out = np.zeros(len(self.v))
        if len(cand) == 0:
            return out
        dirn = -np.sign(s[cand])[:, None] * n[cand]
        O = W[self.v[cand]]
        L = depth[cand] + 0.005
        nb = self.nbr[cand]
        okn = nb >= 0
        ci, kj = np.nonzero(okn)
        tt = nb[ci, kj]
        t, u, v, hit = P.ray_tri(O[ci], dirn[ci], W, self.M.tris[tt])
        hit &= (t >= -1e-4) & (t <= L[ci])
        conf = np.zeros(len(cand), bool)
        conf[ci[hit]] = True
        out[cand[conf]] = depth[cand[conf]]
        return out

    def events(self, depth):
        """{(inner piece, outer piece): tie indices} for the ties with a confirmed depth."""
        k = np.nonzero(depth > 0)[0]
        ev = {}
        if len(k) == 0:
            return ev
        key = self.inner[k] * self.M.ncomp + self.outer[k]
        for kk in np.unique(key):
            ev[divmod(int(kk), self.M.ncomp)] = k[key == kk]
        return ev

    def event_tris(self, ties):
        """The inner piece's triangles touching the poked vertices (what qa_motion highlights)."""
        M = self.M
        m = np.zeros(M.nw, bool)
        m[self.v[ties]] = True
        tr = np.nonzero(m[M.tris].any(1) & ~M.degen)[0]
        return tr[M.piece[M.tri_comp[tr]] == self.inner[ties[0]]]


def visible_px(cams_rasters, tris_sel, ntris):
    """Pixels at 720x1080 of these triangles in each camera's id buffer: {view: px}."""
    mask = np.zeros(ntris, bool)
    mask[tris_sel] = True
    out = {}
    for view, (cam, ids) in cams_rasters.items():
        out[view] = float(mask[ids[ids >= 0]].sum()) * cam.px_scale()
    return out
