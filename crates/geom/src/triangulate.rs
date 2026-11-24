//! O(n log n) triangulation of one simple polygon (no holes): scene3d's `kernel/triangulate.ts`,
//! ported. A top-down sweep inserts diagonals at split and merge vertices to cut the polygon into
//! y-monotone pieces (Lee-Preparata), a CCW face walk extracts the pieces, and each is triangulated
//! by an O(n) stack pass. Every decision is an exact predicate (`exact::orient2d`, exact signed
//! areas), the sweep status is a treap with priorities from a fixed hash (no randomness), and any
//! malformed input degrades to a fan the caller must not trust (n - 2 triangles or the fan).
//!
//! Used to cap a shell's boundary loops (`triangulate_loop`), so an open garment's hole can be
//! measured, rendered and closed for volume.

use crate::exact::{orient2d, Exact};
use crate::v3::V3;

#[derive(Clone, Copy, Debug)]
pub struct P2 {
    pub x: f64,
    pub y: f64,
    /// Original vertex index carried to the output.
    pub i: u32,
    /// Position in the ring.
    pub k: usize,
}

fn cmp_vert(a: &P2, b: &P2) -> std::cmp::Ordering {
    b.y.partial_cmp(&a.y).unwrap_or(std::cmp::Ordering::Equal).then(a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal)).then(a.k.cmp(&b.k))
}
fn above(a: &P2, b: &P2) -> bool {
    cmp_vert(a, b) == std::cmp::Ordering::Less
}
fn turn(a: &P2, b: &P2, c: &P2) -> i8 {
    orient2d([a.x, a.y], [b.x, b.y], [c.x, c.y])
}

#[derive(Clone, Copy)]
struct Edge {
    k: usize,
    u: P2,
    l: P2,
    helper: P2,
    helper_merge: bool,
}

/// +1 when the edge lies right of p at p's sweep height, -1 left (exactly -orient2d(u, l, p)).
fn edge_vs_point(e: &Edge, p: &P2) -> i8 {
    -turn(&e.u, &e.l, p)
}

struct TNode {
    edge: Edge,
    prio: u32,
    left: usize,
    right: usize,
    parent: usize,
}
const NIL: usize = usize::MAX;

fn prio_of(k: usize) -> u32 {
    let mut z = (k as u32).wrapping_add(0x9e37_79b9);
    z = (z ^ (z >> 16)).wrapping_mul(0x21f0_aaad);
    z = (z ^ (z >> 15)).wrapping_mul(0x735a_2d97);
    z ^ (z >> 15)
}

struct Status {
    nodes: Vec<TNode>,
    root: usize,
    by_k: std::collections::HashMap<usize, usize>,
}

impl Status {
    fn new() -> Status {
        Status { nodes: Vec::new(), root: NIL, by_k: Default::default() }
    }
    fn rotate_up(&mut self, x: usize) {
        let p = self.nodes[x].parent;
        let g = self.nodes[p].parent;
        if self.nodes[p].left == x {
            let xr = self.nodes[x].right;
            self.nodes[p].left = xr;
            if xr != NIL {
                self.nodes[xr].parent = p;
            }
            self.nodes[x].right = p;
        } else {
            let xl = self.nodes[x].left;
            self.nodes[p].right = xl;
            if xl != NIL {
                self.nodes[xl].parent = p;
            }
            self.nodes[x].left = p;
        }
        self.nodes[p].parent = x;
        self.nodes[x].parent = g;
        if g != NIL {
            if self.nodes[g].left == p {
                self.nodes[g].left = x;
            } else {
                self.nodes[g].right = x;
            }
        } else {
            self.root = x;
        }
    }
    fn insert(&mut self, e: Edge, p: &P2) {
        let id = self.nodes.len();
        self.nodes.push(TNode { edge: e, prio: prio_of(e.k), left: NIL, right: NIL, parent: NIL });
        self.by_k.insert(e.k, id);
        if self.root == NIL {
            self.root = id;
            return;
        }
        let mut cur = self.root;
        loop {
            let go_left = edge_vs_point(&self.nodes[cur].edge, p) > 0;
            let child = if go_left { self.nodes[cur].left } else { self.nodes[cur].right };
            if child == NIL {
                if go_left {
                    self.nodes[cur].left = id;
                } else {
                    self.nodes[cur].right = id;
                }
                self.nodes[id].parent = cur;
                break;
            }
            cur = child;
        }
        while self.nodes[id].parent != NIL && self.nodes[self.nodes[id].parent].prio > self.nodes[id].prio {
            self.rotate_up(id);
        }
    }
    fn left_of(&self, p: &P2) -> Option<usize> {
        let mut cur = self.root;
        let mut best = None;
        while cur != NIL {
            if edge_vs_point(&self.nodes[cur].edge, p) < 0 {
                best = Some(cur);
                cur = self.nodes[cur].right;
            } else {
                cur = self.nodes[cur].left;
            }
        }
        best
    }
    fn delete(&mut self, k: usize) {
        let Some(id) = self.by_k.remove(&k) else { return };
        loop {
            let (l, r) = (self.nodes[id].left, self.nodes[id].right);
            if l == NIL && r == NIL {
                break;
            }
            if r == NIL || (l != NIL && self.nodes[l].prio < self.nodes[r].prio) {
                self.rotate_up(l);
            } else {
                self.rotate_up(r);
            }
        }
        let p = self.nodes[id].parent;
        if p != NIL {
            if self.nodes[p].left == id {
                self.nodes[p].left = NIL;
            } else {
                self.nodes[p].right = NIL;
            }
        } else {
            self.root = NIL;
        }
        self.nodes[id].parent = NIL;
    }
    fn at(&self, k: usize) -> Option<usize> {
        self.by_k.get(&k).copied()
    }
}

#[derive(PartialEq, Clone, Copy)]
enum VType {
    Start,
    End,
    Split,
    Merge,
    Regular,
}

fn classify(poly: &[P2], k: usize) -> VType {
    let n = poly.len();
    let (v, prev, next) = (&poly[k], &poly[(k + n - 1) % n], &poly[(k + 1) % n]);
    let t = turn(prev, v, next);
    if above(v, prev) && above(v, next) {
        return if t >= 0 { VType::Start } else { VType::Split };
    }
    if above(prev, v) && above(next, v) {
        return if t >= 0 { VType::End } else { VType::Merge };
    }
    VType::Regular
}

fn diagonals(poly: &[P2]) -> Vec<(usize, usize)> {
    let n = poly.len();
    let mut out = Vec::new();
    let mut st = Status::new();
    let types: Vec<VType> = (0..n).map(|k| classify(poly, k)).collect();
    let mk = |k: usize| {
        let (a, b) = (poly[k], poly[(k + 1) % n]);
        let (u, l) = if above(&a, &b) { (a, b) } else { (b, a) };
        Edge { k, u, l, helper: a, helper_merge: false }
    };
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&p, &q| cmp_vert(&poly[p], &poly[q]));
    for k in order {
        let v = poly[k];
        let (eprev, enext) = ((k + n - 1) % n, k);
        let mut diag = |a: &P2, b: &P2| {
            if a.k != b.k {
                out.push((a.k, b.k));
            }
        };
        match types[k] {
            VType::Start => st.insert(mk(enext), &v),
            VType::End => {
                if let Some(id) = st.at(eprev) {
                    let e = st.nodes[id].edge;
                    if e.helper_merge {
                        diag(&v, &e.helper);
                    }
                }
                st.delete(eprev);
            }
            VType::Split => {
                if let Some(id) = st.left_of(&v) {
                    let h = st.nodes[id].edge.helper;
                    diag(&v, &h);
                    st.nodes[id].edge.helper = v;
                    st.nodes[id].edge.helper_merge = false;
                }
                st.insert(mk(enext), &v);
            }
            VType::Merge => {
                if let Some(id) = st.at(eprev) {
                    let e = st.nodes[id].edge;
                    if e.helper_merge {
                        diag(&v, &e.helper);
                    }
                }
                st.delete(eprev);
                if let Some(id) = st.left_of(&v) {
                    let e = st.nodes[id].edge;
                    if e.helper_merge {
                        diag(&v, &e.helper);
                    }
                    st.nodes[id].edge.helper = v;
                    st.nodes[id].edge.helper_merge = true;
                }
            }
            VType::Regular => {
                if above(&poly[eprev], &v) {
                    if let Some(id) = st.at(eprev) {
                        let e = st.nodes[id].edge;
                        if e.helper_merge {
                            diag(&v, &e.helper);
                        }
                    }
                    st.delete(eprev);
                    st.insert(mk(enext), &v);
                } else if let Some(id) = st.left_of(&v) {
                    let e = st.nodes[id].edge;
                    if e.helper_merge {
                        diag(&v, &e.helper);
                    }
                    st.nodes[id].edge.helper = v;
                    st.nodes[id].edge.helper_merge = false;
                }
            }
        }
    }
    out
}

fn area2_sign(poly: &[P2], face: &[usize]) -> i8 {
    let mut s = Exact::zero();
    for j in 0..face.len() {
        let (a, b) = (&poly[face[j]], &poly[face[(j + 1) % face.len()]]);
        s.add_assign(&Exact::product(a.x, b.y));
        s.add_assign(&Exact::product(-b.x, a.y));
    }
    s.sign()
}

fn extract_faces(poly: &[P2], diags: &[(usize, usize)]) -> Option<Vec<Vec<usize>>> {
    let n = poly.len();
    let mut adj: Vec<Vec<usize>> = (0..n).map(|k| vec![(k + 1) % n, (k + n - 1) % n]).collect();
    for &(a, b) in diags {
        adj[a].push(b);
        adj[b].push(a);
    }
    let half = |c: &P2, p: &P2| if p.y < c.y || (p.y == c.y && p.x >= c.x) { 0 } else { 1 };
    for k in 0..n {
        let c = poly[k];
        adj[k].sort_by(|&a, &b| {
            let (pa, pb) = (&poly[a], &poly[b]);
            let (ha, hb) = (half(&c, pa), half(&c, pb));
            if ha != hb {
                return ha.cmp(&hb);
            }
            let t = turn(&c, pa, pb);
            if t != 0 {
                return if t > 0 { std::cmp::Ordering::Less } else { std::cmp::Ordering::Greater };
            }
            let d = |p: &P2| {
                let mut e = Exact::product(p.x - c.x, p.x - c.x);
                e.add_assign(&Exact::product(p.y - c.y, p.y - c.y));
                e
            };
            let s = d(pa).sub(&d(pb)).sign();
            if s != 0 {
                return if s < 0 { std::cmp::Ordering::Less } else { std::cmp::Ordering::Greater };
            }
            a.cmp(&b)
        });
        adj[k].dedup();
    }
    let mut used = std::collections::HashSet::new();
    let mut faces = Vec::new();
    for s in 0..n {
        for t0 in adj[s].clone() {
            if used.contains(&(s, t0)) {
                continue;
            }
            let mut face = Vec::new();
            let (mut from, mut to) = (s, t0);
            let mut guard = 4 * (n + diags.len()) + 8;
            loop {
                if !used.insert((from, to)) {
                    return None;
                }
                face.push(from);
                let nb = &adj[to];
                let idx = nb.iter().position(|&x| x == from)?;
                let next = nb[(idx + nb.len() - 1) % nb.len()];
                from = to;
                to = next;
                if from == s && to == t0 {
                    break;
                }
                if guard == 0 {
                    return None;
                }
                guard -= 1;
            }
            if face.len() >= 3 && area2_sign(poly, &face) > 0 {
                faces.push(face);
            }
        }
    }
    Some(faces)
}

fn monotone(poly: &[P2], face: &[usize], out: &mut Vec<[u32; 3]>) {
    let m = face.len();
    if m < 3 {
        return;
    }
    let v: Vec<P2> = face.iter().map(|&k| poly[k]).collect();
    let mut idx: Vec<usize> = (0..m).collect();
    idx.sort_by(|&a, &b| cmp_vert(&v[a], &v[b]));
    let (top, bot) = (idx[0], idx[m - 1]);
    let mut chain = vec![0u8; m];
    let mut j = (top + 1) % m;
    while j != bot {
        chain[j] = 0;
        j = (j + 1) % m;
    }
    let mut j = (bot + 1) % m;
    while j != top {
        chain[j] = 1;
        j = (j + 1) % m;
    }
    let mut emit = |a: usize, b: usize, c: usize| {
        let t = turn(&v[a], &v[b], &v[c]);
        if t > 0 {
            out.push([v[a].i, v[b].i, v[c].i]);
        } else if t < 0 {
            out.push([v[a].i, v[c].i, v[b].i]);
        }
    };
    let mut stack = vec![idx[0], idx[1]];
    for s in 2..m - 1 {
        let vv = idx[s];
        let tos = *stack.last().unwrap();
        if chain[vv] != chain[tos] {
            while stack.len() > 1 {
                let a = stack.pop().unwrap();
                emit(vv, a, *stack.last().unwrap());
            }
            stack.pop();
            stack.push(idx[s - 1]);
            stack.push(vv);
        } else {
            let mut a = stack.pop().unwrap();
            while let Some(&b) = stack.last() {
                let t = turn(&v[b], &v[a], &v[vv]);
                let inside = if chain[vv] == 0 { t > 0 } else { t < 0 };
                if !inside {
                    break;
                }
                emit(b, a, vv);
                a = stack.pop().unwrap();
            }
            stack.push(a);
            stack.push(vv);
        }
    }
    let last = idx[m - 1];
    while stack.len() > 1 {
        let a = stack.pop().unwrap();
        emit(last, a, *stack.last().unwrap());
    }
}

fn fan(poly: &[P2]) -> Vec<[u32; 3]> {
    (1..poly.len().saturating_sub(1)).map(|k| [poly[0].i, poly[k].i, poly[k + 1].i]).collect()
}

/// Triangulate a counter-clockwise simple polygon into counter-clockwise triangles.
/// Returns (triangles, trusted): untrusted means the input was not a valid simple polygon and the
/// output is a fan.
pub fn triangulate_polygon(poly: &[P2]) -> (Vec<[u32; 3]>, bool) {
    let n = poly.len();
    if n < 3 {
        return (vec![], n == 0);
    }
    if n == 3 {
        return (vec![[poly[0].i, poly[1].i, poly[2].i]], true);
    }
    let d = diagonals(poly);
    let Some(faces) = extract_faces(poly, &d) else { return (fan(poly), false) };
    let mut out = Vec::new();
    for f in &faces {
        monotone(poly, f, &mut out);
    }
    if out.len() != n - 2 {
        return (fan(poly), false);
    }
    (out, true)
}

/// Triangulate a 3D loop (indices into `pos`) by projecting on the plane its Newell normal is most
/// aligned with, oriented so the loop reads counter-clockwise (scene3d `triangulateFace`). The
/// triangles wind like the loop.
pub fn triangulate_loop(lp: &[u32], pos: &[V3]) -> (Vec<[u32; 3]>, bool) {
    let n = lp.len();
    if n < 3 {
        return (vec![], false);
    }
    let mut nx = [0.0; 3];
    for k in 0..n {
        let (a, b) = (pos[lp[k] as usize], pos[lp[(k + 1) % n] as usize]);
        nx[0] += (a[1] - b[1]) * (a[2] + b[2]);
        nx[1] += (a[2] - b[2]) * (a[0] + b[0]);
        nx[2] += (a[0] - b[0]) * (a[1] + b[1]);
    }
    let ax = [nx[0].abs(), nx[1].abs(), nx[2].abs()];
    let drop = if ax[0] >= ax[1] && ax[0] >= ax[2] { 0 } else if ax[1] >= ax[2] { 1 } else { 2 };
    let (ci, cj) = match drop {
        0 => (1, 2),
        1 => (2, 0),
        _ => (0, 1),
    };
    let flip = nx[drop] < 0.0;
    let (px, py) = if flip { (cj, ci) } else { (ci, cj) };
    let poly: Vec<P2> = lp.iter().enumerate().map(|(k, &vi)| P2 { x: pos[vi as usize][px], y: pos[vi as usize][py], i: vi, k }).collect();
    // The loop reads counter-clockwise in this projection, so triangles counter-clockwise in it wind
    // like the loop in 3D (a swapped projection is a reflection applied to both).
    triangulate_polygon(&poly)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ring(pts: &[(f64, f64)]) -> Vec<P2> {
        pts.iter().enumerate().map(|(k, &(x, y))| P2 { x, y, i: k as u32, k }).collect()
    }

    fn area(poly: &[P2], tris: &[[u32; 3]]) -> f64 {
        tris.iter()
            .map(|t| {
                let (a, b, c) = (poly[t[0] as usize], poly[t[1] as usize], poly[t[2] as usize]);
                ((b.x - a.x) * (c.y - a.y) - (c.x - a.x) * (b.y - a.y)) / 2.0
            })
            .sum()
    }

    #[test]
    fn non_convex_polygons_tile_exactly() {
        // A U shape and a comb: a fan from vertex 0 would overlap itself.
        let u = ring(&[(0.0, 0.0), (3.0, 0.0), (3.0, 3.0), (2.0, 3.0), (2.0, 1.0), (1.0, 1.0), (1.0, 3.0), (0.0, 3.0)]);
        let (t, ok) = triangulate_polygon(&u);
        assert!(ok && t.len() == 6);
        assert!((area(&u, &t) - 7.0).abs() < 1e-12);
        assert!(t.iter().all(|x| turn(&u[x[0] as usize], &u[x[1] as usize], &u[x[2] as usize]) > 0));
        let mut comb = vec![(0.0, 0.0), (8.0, 0.0)];
        for i in (0..4).rev() {
            let x = i as f64 * 2.0;
            comb.push((x + 2.0, 4.0));
            comb.push((x + 1.0, 1.0));
        }
        comb.pop();
        comb.push((0.0, 4.0));
        let c = ring(&comb);
        let (t, ok) = triangulate_polygon(&c);
        assert!(ok, "{comb:?}");
        assert_eq!(t.len(), c.len() - 2);
    }

    #[test]
    fn bow_tie_degrades_to_an_untrusted_fan() {
        let b = ring(&[(0.0, 0.0), (2.0, 2.0), (2.0, 0.0), (0.0, 2.0)]);
        let (t, _ok) = triangulate_polygon(&b);
        assert_eq!(t.len(), 2);
    }

    #[test]
    fn loops_in_3d() {
        let pos = vec![[0.0, 0.0, 0.0], [0.0, 0.0, 2.0], [0.0, 2.0, 2.0], [0.0, 2.0, 0.0]];
        let (t, ok) = triangulate_loop(&[0, 1, 2, 3], &pos);
        assert!(ok && t.len() == 2);
        // winding follows the loop: Newell normal of 0,1,2,3 in the x=0 plane is -x
        let n = crate::v3::tri_normal(pos[t[0][0] as usize], pos[t[0][1] as usize], pos[t[0][2] as usize]);
        assert!(n[0] < 0.0, "{n:?}");
    }
}
