//! Topology of a triangle soup, welded by exact position: edges and their faces, shells, boundary
//! loops, manifoldness, winding, Euler characteristic, genus and exact homology.
//!
//! Compiled meshes split vertices wherever UVs, normals or materials change, so connectivity is
//! recovered by welding vertices at **identical** float positions (scene3d's "exact-coordinate
//! weld, an integer permutation", never a distance tolerance: a tolerance weld would close a real
//! crack and hide it). Near-coincident unwelded vertices are a separate fact (`seams`).
//!
//! Homology (scene3d `kernel/homology.ts`): Betti numbers from the ranks of the boundary operators,
//! b0 = |V| - rank d1, b1 = |E| - rank d1 - rank d2, b2 = |F| - rank d2. scene3d takes the ranks by
//! dense rational elimination, which cannot run on a 100k-face shell; here they are taken
//! structurally and stay exact: rank d1 = |V| - components (union-find); for d2, faces joined by
//! two-face edges form patches whose 2-cycle coefficient is one scalar per orientable patch (zero
//! for a non-orientable one), a one-face edge forces its patch's scalar to zero, and each edge with
//! three or more faces is one integer equation over patch scalars, solved by exact fraction-free
//! elimination (i128 with row gcd; a modular rank, flagged uncertified, only if that overflows).

use crate::v3::{self, V3};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Edge {
    pub a: u32,
    pub b: u32,
    /// (triangle, traversed a->b)
    pub faces: Vec<(u32, bool)>,
}

#[derive(Debug, Clone, Default)]
pub struct Loop {
    pub verts: Vec<u32>,
    pub perimeter: f64,
    /// Area of the loop's Newell polygon (the hole it outlines).
    pub area: f64,
    pub centroid: V3,
    /// Unit Newell normal: the direction a face capping the loop would face (outwards for a
    /// hole in an outward-wound shell).
    pub normal: V3,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Homology {
    pub b0: i64,
    pub b1: i64,
    pub b2: i64,
    /// false when the d2 rank was taken modulo a prime after exact elimination overflowed.
    pub certified: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Shell {
    pub tris: Vec<u32>,
    pub verts: usize,
    pub edges: usize,
    pub boundary_edges: usize,
    pub nonmanifold_edges: usize,
    pub winding_conflicts: usize,
    pub nonmanifold_verts: usize,
    pub euler: i64,
    pub closed: bool,
    pub orientable: bool,
    pub loops: Vec<Loop>,
    pub homology: Homology,
    pub area: f64,
    pub bounds: v3::Aabb,
}

impl Shell {
    /// Genus of the surface (handles), from chi = 2 - 2g - (boundary loops), when it is a manifold.
    pub fn genus(&self) -> Option<i64> {
        if self.nonmanifold_edges > 0 {
            return None;
        }
        let g2 = 2 - self.euler - self.loops.len() as i64;
        if g2 >= 0 && g2 % 2 == 0 && self.orientable {
            Some(g2 / 2)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Topology {
    /// input vertex -> welded vertex
    pub weld: Vec<u32>,
    pub pos: Vec<V3>,
    /// triangles over welded vertices
    pub tris: Vec<[u32; 3]>,
    /// triangles that collapsed in the weld (two corners on one position)
    pub collapsed: Vec<u32>,
    pub edges: Vec<Edge>,
    /// per triangle, its three edge ids (u32::MAX for a collapsed triangle)
    pub tri_edges: Vec<[u32; 3]>,
    /// per triangle, its shell
    pub shell_of: Vec<u32>,
    pub shells: Vec<Shell>,
}

fn key(p: V3) -> [u64; 3] {
    // -0.0 and 0.0 are one position.
    let k = |x: f64| if x == 0.0 { 0u64 } else { x.to_bits() };
    [k(p[0]), k(p[1]), k(p[2])]
}

struct Uf(Vec<u32>);
impl Uf {
    fn new(n: usize) -> Uf {
        Uf((0..n as u32).collect())
    }
    fn find(&mut self, x: u32) -> u32 {
        let mut r = x;
        while self.0[r as usize] != r {
            r = self.0[r as usize];
        }
        let mut x = x;
        while self.0[x as usize] != r {
            let n = self.0[x as usize];
            self.0[x as usize] = r;
            x = n;
        }
        r
    }
    fn union(&mut self, a: u32, b: u32) {
        let (a, b) = (self.find(a), self.find(b));
        if a != b {
            let (lo, hi) = if a < b { (a, b) } else { (b, a) };
            self.0[hi as usize] = lo;
        }
    }
}

impl Topology {
    pub fn build(positions: &[V3], tris: &[[u32; 3]]) -> Topology {
        let mut t = Topology::default();
        let mut map: HashMap<[u64; 3], u32> = HashMap::with_capacity(positions.len());
        for p in positions {
            let n = map.len() as u32;
            let id = *map.entry(key(*p)).or_insert(n);
            if id as usize == t.pos.len() {
                t.pos.push(*p);
            }
            t.weld.push(id);
        }
        t.tris = tris.iter().map(|tr| [t.weld[tr[0] as usize], t.weld[tr[1] as usize], t.weld[tr[2] as usize]]).collect();
        let mut emap: HashMap<(u32, u32), u32> = HashMap::with_capacity(tris.len() * 2);
        t.tri_edges = vec![[u32::MAX; 3]; t.tris.len()];
        for (ti, tr) in t.tris.iter().enumerate() {
            if tr[0] == tr[1] || tr[1] == tr[2] || tr[0] == tr[2] {
                t.collapsed.push(ti as u32);
                continue;
            }
            for k in 0..3 {
                let (u, v) = (tr[k], tr[(k + 1) % 3]);
                let (a, b) = if u < v { (u, v) } else { (v, u) };
                let n = t.edges.len() as u32;
                let e = *emap.entry((a, b)).or_insert(n);
                if e == n {
                    t.edges.push(Edge { a, b, faces: Vec::with_capacity(2) });
                }
                t.edges[e as usize].faces.push((ti as u32, u == a));
                t.tri_edges[ti][k] = e;
            }
        }
        // Shells: triangles joined through shared edges.
        let mut uf = Uf::new(t.tris.len());
        for e in &t.edges {
            for w in e.faces.windows(2) {
                uf.union(w[0].0, w[1].0);
            }
        }
        let mut root_shell: HashMap<u32, u32> = HashMap::new();
        t.shell_of = vec![u32::MAX; t.tris.len()];
        for ti in 0..t.tris.len() as u32 {
            if t.tri_edges[ti as usize][0] == u32::MAX {
                continue;
            }
            let r = uf.find(ti);
            let n = root_shell.len() as u32;
            let s = *root_shell.entry(r).or_insert(n);
            if s as usize == t.shells.len() {
                t.shells.push(Shell { bounds: v3::Aabb::empty(), orientable: true, ..Default::default() });
            }
            t.shell_of[ti as usize] = s;
            t.shells[s as usize].tris.push(ti);
        }
        t.measure();
        t
    }

    fn measure(&mut self) {
        let ns = self.shells.len();
        let mut verts_of: Vec<Vec<u32>> = vec![Vec::new(); ns];
        let mut seen: HashMap<(u32, u32), ()> = HashMap::new();
        for (si, sh) in self.shells.iter_mut().enumerate() {
            for &ti in &sh.tris {
                let tr = self.tris[ti as usize];
                let p = [self.pos[tr[0] as usize], self.pos[tr[1] as usize], self.pos[tr[2] as usize]];
                sh.area += v3::tri_area(p[0], p[1], p[2]);
                for (k, q) in p.iter().enumerate() {
                    sh.bounds.grow(*q);
                    if seen.insert((si as u32, tr[k]), ()).is_none() {
                        verts_of[si].push(tr[k]);
                    }
                }
            }
            sh.verts = verts_of[si].len();
        }
        let mut edges_of: Vec<Vec<u32>> = vec![Vec::new(); ns];
        for (ei, e) in self.edges.iter().enumerate() {
            let s = self.shell_of[e.faces[0].0 as usize] as usize;
            edges_of[s].push(ei as u32);
            let sh = &mut self.shells[s];
            sh.edges += 1;
            match e.faces.len() {
                1 => sh.boundary_edges += 1,
                2 => {
                    if e.faces[0].1 == e.faces[1].1 {
                        sh.winding_conflicts += 1;
                    }
                }
                _ => sh.nonmanifold_edges += 1,
            }
        }
        for si in 0..ns {
            let f = self.shells[si].tris.len() as i64;
            let sh = &mut self.shells[si];
            sh.euler = sh.verts as i64 - sh.edges as i64 + f;
            sh.closed = sh.boundary_edges == 0 && sh.nonmanifold_edges == 0;
        }
        self.orientability();
        self.nonmanifold_vertices(&verts_of);
        self.loops();
        let adj = self.adjacency();
        for si in 0..ns {
            self.shells[si].homology = self.homology(si, &edges_of[si], &adj);
        }
    }

    /// Can each shell's faces be flipped into a consistent winding? (BFS parity over two-face edges.)
    fn orientability(&mut self) {
        let mut flip: Vec<i8> = vec![-1; self.tris.len()];
        let adj = self.adjacency();
        for sh in self.shells.iter_mut() {
            let mut ok = true;
            for &start in &sh.tris {
                if flip[start as usize] >= 0 {
                    continue;
                }
                flip[start as usize] = 0;
                let mut q = vec![start];
                while let Some(ti) = q.pop() {
                    for &(other, same_dir) in &adj[ti as usize] {
                        // same direction on a shared edge = they disagree unless one is flipped
                        let want = flip[ti as usize] ^ (same_dir as i8);
                        if flip[other as usize] < 0 {
                            flip[other as usize] = want;
                            q.push(other);
                        } else if flip[other as usize] != want {
                            ok = false;
                        }
                    }
                }
            }
            sh.orientable = ok;
        }
    }

    /// Two-face-edge neighbours of each triangle, with "traverses the shared edge the same way".
    pub fn adjacency(&self) -> Vec<Vec<(u32, bool)>> {
        let mut adj: Vec<Vec<(u32, bool)>> = vec![Vec::new(); self.tris.len()];
        for e in &self.edges {
            if e.faces.len() == 2 {
                let ((f0, d0), (f1, d1)) = (e.faces[0], e.faces[1]);
                adj[f0 as usize].push((f1, d0 == d1));
                adj[f1 as usize].push((f0, d0 == d1));
            }
        }
        adj
    }

    fn nonmanifold_vertices(&mut self, verts_of: &[Vec<u32>]) {
        // A vertex is non-manifold when its incident faces form more than one fan (scene3d's pinch
        // test, counted only for vertices with two or more faces).
        let mut vfaces: HashMap<u32, Vec<u32>> = HashMap::new();
        for (ti, tr) in self.tris.iter().enumerate() {
            if self.tri_edges[ti][0] == u32::MAX {
                continue;
            }
            for &v in tr {
                vfaces.entry(v).or_default().push(ti as u32);
            }
        }
        let _ = verts_of;
        let mut count = vec![0usize; self.shells.len()];
        for (v, faces) in &vfaces {
            if faces.len() < 2 {
                continue;
            }
            let idx: HashMap<u32, usize> = faces.iter().enumerate().map(|(i, f)| (*f, i)).collect();
            let mut uf = Uf::new(faces.len());
            for &f in faces {
                for &e in &self.tri_edges[f as usize] {
                    let ed = &self.edges[e as usize];
                    if (ed.a == *v || ed.b == *v) && ed.faces.len() <= 2 {
                        for w in ed.faces.windows(2) {
                            if let (Some(&x), Some(&y)) = (idx.get(&w[0].0), idx.get(&w[1].0)) {
                                uf.union(x as u32, y as u32);
                            }
                        }
                    }
                }
            }
            let fans = (0..faces.len() as u32).filter(|&i| uf.find(i) == i).count();
            if fans > 1 {
                count[self.shell_of[faces[0] as usize] as usize] += 1;
            }
        }
        for (s, c) in count.into_iter().enumerate() {
            self.shells[s].nonmanifold_verts = c;
        }
    }

    fn loops(&mut self) {
        // Boundary half-edges in face order (tail -> head).
        let mut out: HashMap<u32, Vec<(u32, u32)>> = HashMap::new(); // tail -> (head, edge)
        let mut used = vec![false; self.edges.len()];
        let mut bedges = Vec::new();
        for (ei, e) in self.edges.iter().enumerate() {
            if e.faces.len() == 1 {
                let (t, h) = if e.faces[0].1 { (e.a, e.b) } else { (e.b, e.a) };
                out.entry(t).or_default().push((h, ei as u32));
                bedges.push((t, h, ei as u32));
            }
        }
        for v in out.values_mut() {
            v.sort();
        }
        for &(t0, h0, e0) in &bedges {
            if used[e0 as usize] {
                continue;
            }
            used[e0 as usize] = true;
            let mut verts = vec![t0];
            let mut cur = h0;
            let mut closed = false;
            for _ in 0..bedges.len() {
                if cur == t0 {
                    closed = true;
                    break;
                }
                verts.push(cur);
                let next = out.get(&cur).and_then(|v| v.iter().find(|(_, e)| !used[*e as usize]).copied());
                match next {
                    Some((h, e)) => {
                        used[e as usize] = true;
                        cur = h;
                    }
                    None => break,
                }
            }
            if !closed || verts.len() < 2 {
                // an open chain (through a non-manifold vertex): still a boundary, measured as is
            }
            let n = verts.len();
            let mut per = 0.0;
            let mut nx = [0.0; 3];
            let mut c = [0.0; 3];
            for k in 0..n {
                let a = self.pos[verts[k] as usize];
                let b = self.pos[verts[(k + 1) % n] as usize];
                per += v3::dist(a, b);
                nx[0] += (a[1] - b[1]) * (a[2] + b[2]);
                nx[1] += (a[2] - b[2]) * (a[0] + b[0]);
                nx[2] += (a[0] - b[0]) * (a[1] + b[1]);
                c = v3::add(c, a);
            }
            let area = 0.5 * v3::len(nx);
            let shell = self.edges[e0 as usize].faces[0].0;
            let s = self.shell_of[shell as usize] as usize;
            // A hole's Newell normal from the faces' traversal points into the surface's back
            // side; the cap that would close it faces the reverse.
            let normal = v3::norm(v3::scale(nx, -1.0));
            self.shells[s].loops.push(Loop { verts, perimeter: per, area, centroid: v3::scale(c, 1.0 / n as f64), normal });
        }
    }

    fn homology(&self, si: usize, edges: &[u32], adj: &[Vec<(u32, bool)>]) -> Homology {
        let sh = &self.shells[si];
        let (v, e, f) = (sh.verts as i64, sh.edges as i64, sh.tris.len() as i64);
        let rank_d1 = v - 1; // one component (shells are edge-connected)
        let (b2, certified) = if sh.nonmanifold_edges == 0 {
            (if sh.boundary_edges == 0 && sh.orientable { 1 } else { 0 }, true)
        } else {
            self.cycles(sh, edges, adj)
        };
        let rank_d2 = f - b2;
        Homology { b0: 1, b1: e - rank_d1 - rank_d2, b2, certified }
    }

    /// dim ker d2 for a shell with non-manifold edges (see the module comment).
    fn cycles(&self, sh: &Shell, edges: &[u32], adj: &[Vec<(u32, bool)>]) -> (i64, bool) {
        // Patches: union over two-face edges; each face gets a sign relative to its patch root.
        let local: HashMap<u32, usize> = sh.tris.iter().enumerate().map(|(i, t)| (*t, i)).collect();
        let n = sh.tris.len();
        let mut patch = vec![usize::MAX; n];
        let mut sign = vec![0i64; n];
        let mut patch_ok: Vec<bool> = Vec::new();
        for i in 0..n {
            if patch[i] != usize::MAX {
                continue;
            }
            let p = patch_ok.len();
            patch_ok.push(true);
            patch[i] = p;
            sign[i] = 1;
            let mut q = vec![i];
            while let Some(x) = q.pop() {
                for &(o, same) in &adj[sh.tris[x] as usize] {
                    let Some(&oi) = local.get(&o) else { continue };
                    // A 2-chain is a cycle on this edge when the coefficients cancel: opposite
                    // traversal needs equal signs, same traversal opposite signs.
                    let want = if same { -sign[x] } else { sign[x] };
                    if patch[oi] == usize::MAX {
                        patch[oi] = p;
                        sign[oi] = want;
                        q.push(oi);
                    } else if sign[oi] != want {
                        patch_ok[p] = false;
                    }
                }
            }
        }
        // Constraints over patch scalars: one row per edge with != 2 faces.
        let np = patch_ok.len();
        let mut rows: Vec<Vec<i128>> = Vec::new();
        for (p, ok) in patch_ok.iter().enumerate() {
            if !ok {
                let mut r = vec![0i128; np];
                r[p] = 1;
                rows.push(r);
            }
        }
        for &ei in edges {
            let ed = &self.edges[ei as usize];
            if ed.faces.len() == 2 {
                continue;
            }
            let mut r = vec![0i128; np];
            for &(t, fwd) in &ed.faces {
                let i = local[&t];
                r[patch[i]] += sign[i] as i128 * if fwd { 1 } else { -1 };
            }
            if r.iter().any(|x| *x != 0) {
                rows.push(r);
            }
        }
        match rank_exact(rows.clone(), np) {
            Some(r) => ((np - r) as i64, true),
            None => ((np - rank_mod(rows, np)) as i64, false),
        }
    }

    /// Triangles of a shell.
    pub fn shell_tris(&self, s: usize) -> &[u32] {
        &self.shells[s].tris
    }
}

fn gcd(a: i128, b: i128) -> i128 {
    let (mut a, mut b) = (a.abs(), b.abs());
    while b != 0 {
        let t = a % b;
        a = b;
        b = t;
    }
    a
}

/// Exact rank by fraction-free elimination; None on overflow.
fn rank_exact(mut rows: Vec<Vec<i128>>, cols: usize) -> Option<usize> {
    let mut rank = 0;
    for c in 0..cols {
        let Some(p) = (rank..rows.len()).find(|&r| rows[r][c] != 0) else { continue };
        rows.swap(rank, p);
        let piv = rows[rank].clone();
        for r in 0..rows.len() {
            if r == rank || rows[r][c] == 0 {
                continue;
            }
            let f = rows[r][c];
            let mut g = 0i128;
            for k in 0..cols {
                let v = rows[r][k].checked_mul(piv[c])?.checked_sub(f.checked_mul(piv[k])?)?;
                rows[r][k] = v;
                g = gcd(g, v);
            }
            if g > 1 {
                rows[r].iter_mut().for_each(|x| *x /= g);
            }
        }
        rank += 1;
    }
    Some(rank)
}

fn rank_mod(rows: Vec<Vec<i128>>, cols: usize) -> usize {
    const P: i128 = (1 << 61) - 1;
    let mut m: Vec<Vec<i128>> = rows.into_iter().map(|r| r.into_iter().map(|x| x.rem_euclid(P)).collect()).collect();
    let pow = |mut b: i128, mut e: i128| {
        let mut r = 1i128;
        b %= P;
        while e > 0 {
            if e & 1 == 1 {
                r = r * b % P;
            }
            b = b * b % P;
            e >>= 1;
        }
        r
    };
    let mut rank = 0;
    for c in 0..cols {
        let Some(p) = (rank..m.len()).find(|&r| m[r][c] != 0) else { continue };
        m.swap(rank, p);
        let inv = pow(m[rank][c], P - 2);
        let piv: Vec<i128> = m[rank].iter().map(|x| x * inv % P).collect();
        m[rank] = piv.clone();
        for r in 0..m.len() {
            if r != rank && m[r][c] != 0 {
                let f = m[r][c];
                for k in 0..cols {
                    m[r][k] = (m[r][k] - f * piv[k] % P).rem_euclid(P);
                }
            }
        }
        rank += 1;
    }
    rank
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// A closed unit cube: 8 corners, 12 outward triangles.
    pub fn cube(o: V3, s: f64) -> (Vec<V3>, Vec<[u32; 3]>) {
        let p: Vec<V3> = (0..8).map(|i| [o[0] + s * (i & 1) as f64, o[1] + s * ((i >> 1) & 1) as f64, o[2] + s * ((i >> 2) & 1) as f64]).collect();
        let quads = [[0, 2, 3, 1], [4, 5, 7, 6], [0, 1, 5, 4], [2, 6, 7, 3], [0, 4, 6, 2], [1, 3, 7, 5]];
        let mut t = Vec::new();
        for q in quads {
            t.push([q[0], q[1], q[2]]);
            t.push([q[0], q[2], q[3]]);
        }
        (p, t)
    }

    #[test]
    fn closed_cube_is_a_sphere() {
        let (p, t) = cube([0.0; 3], 1.0);
        let topo = Topology::build(&p, &t);
        assert_eq!(topo.shells.len(), 1);
        let s = &topo.shells[0];
        assert!(s.closed && s.orientable && s.winding_conflicts == 0);
        assert_eq!(s.euler, 2);
        assert_eq!(s.genus(), Some(0));
        assert_eq!(s.homology, Homology { b0: 1, b1: 0, b2: 1, certified: true });
    }

    #[test]
    fn split_vertices_weld_and_a_hole_is_one_loop() {
        // Duplicate every vertex per triangle (as compiled meshes do at seams), drop one face.
        let (p, t) = cube([0.0; 3], 2.0);
        let mut pos = Vec::new();
        let mut tris = Vec::new();
        for tr in t.iter().skip(2) {
            let b = pos.len() as u32;
            pos.extend(tr.iter().map(|&i| p[i as usize]));
            tris.push([b, b + 1, b + 2]);
        }
        let topo = Topology::build(&pos, &tris);
        assert_eq!(topo.shells.len(), 1);
        let s = &topo.shells[0];
        assert!(!s.closed);
        assert_eq!(s.boundary_edges, 4);
        assert_eq!(s.loops.len(), 1);
        assert_eq!(s.loops[0].verts.len(), 4);
        assert!((s.loops[0].area - 4.0).abs() < 1e-12 && (s.loops[0].perimeter - 8.0).abs() < 1e-12);
        // The removed face was the z=0 bottom (normal -z): the cap normal faces -z.
        assert!(s.loops[0].normal[2] < -0.99, "{:?}", s.loops[0].normal);
        assert_eq!(s.homology, Homology { b0: 1, b1: 0, b2: 0, certified: true });
        assert_eq!(s.genus(), Some(0));
    }

    #[test]
    fn flipped_face_is_a_winding_conflict_but_orientable() {
        let (p, mut t) = cube([0.0; 3], 1.0);
        t[3] = [t[3][0], t[3][2], t[3][1]];
        let topo = Topology::build(&p, &t);
        let s = &topo.shells[0];
        assert_eq!(s.winding_conflicts, 3);
        assert!(s.orientable);
        assert_eq!(s.homology.b2, 1);
    }

    #[test]
    fn torus_has_genus_one() {
        // 4x4 quad torus grid
        let (n, m) = (6usize, 5usize);
        let mut p = Vec::new();
        for i in 0..n {
            for j in 0..m {
                let (u, v) = (i as f64 / n as f64 * std::f64::consts::TAU, j as f64 / m as f64 * std::f64::consts::TAU);
                p.push([(3.0 + v.cos()) * u.cos(), (3.0 + v.cos()) * u.sin(), v.sin()]);
            }
        }
        let id = |i: usize, j: usize| ((i % n) * m + (j % m)) as u32;
        let mut t = Vec::new();
        for i in 0..n {
            for j in 0..m {
                t.push([id(i, j), id(i + 1, j), id(i + 1, j + 1)]);
                t.push([id(i, j), id(i + 1, j + 1), id(i, j + 1)]);
            }
        }
        let topo = Topology::build(&p, &t);
        let s = &topo.shells[0];
        assert_eq!(s.euler, 0);
        assert_eq!(s.genus(), Some(1));
        assert_eq!(s.homology, Homology { b0: 1, b1: 2, b2: 1, certified: true });
    }

    #[test]
    fn two_cubes_on_an_edge_are_non_manifold_with_two_voids() {
        let (p1, t1) = cube([0.0; 3], 1.0);
        let (p2, t2) = cube([1.0, 1.0, 0.0], 1.0);
        let mut p = p1.clone();
        p.extend(p2);
        let mut t = t1.clone();
        t.extend(t2.iter().map(|tr| [tr[0] + 8, tr[1] + 8, tr[2] + 8]));
        let topo = Topology::build(&p, &t);
        assert_eq!(topo.shells.len(), 1, "they share the edge x=1,y=1");
        let s = &topo.shells[0];
        assert_eq!(s.nonmanifold_edges, 1);
        assert_eq!(s.homology.b2, 2);
        assert!(s.homology.certified);
        // chi = b0 - b1 + b2
        assert_eq!(s.euler, s.homology.b0 - s.homology.b1 + s.homology.b2);
    }

    #[test]
    fn mobius_strip_is_not_orientable() {
        let n = 8;
        let mut p = Vec::new();
        for i in 0..n {
            let a = i as f64 / n as f64 * std::f64::consts::TAU;
            for s in [-1.0, 1.0] {
                let w = s * 0.5;
                p.push([(2.0 + w * (a / 2.0).cos()) * a.cos(), (2.0 + w * (a / 2.0).cos()) * a.sin(), w * (a / 2.0).sin()]);
            }
        }
        let mut t = Vec::new();
        for i in 0..n {
            let (a0, a1) = (2 * i as u32, 2 * i as u32 + 1);
            let (b0, b1) = if i + 1 == n { (1, 0) } else { (2 * (i as u32 + 1), 2 * (i as u32 + 1) + 1) };
            t.push([a0, b0, b1]);
            t.push([a0, b1, a1]);
        }
        let topo = Topology::build(&p, &t);
        assert!(!topo.shells[0].orientable);
        assert_eq!(topo.shells[0].homology.b2, 0);
    }
}
