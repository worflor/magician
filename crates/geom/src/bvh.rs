//! Bounding-volume hierarchy over triangles: ray casts, nearest-surface queries and pair overlap.
//!
//! scene3d leans on Blender's `BVHTree` (contact separation, wall thickness) and on sweep-and-prune
//! (z-fighting, embedding). Compiled characters are 100k-300k triangles, so every pairwise question
//! here goes through this tree instead: median split on the longest centroid axis, 4 triangles a
//! leaf, built deterministically (stable sort, no randomness).

use crate::v3::{self, Aabb, V3};

#[derive(Debug, Clone)]
struct Node {
    bb: Aabb,
    /// Leaf: first triangle and count; inner: left child at `start`, right at `start + 1` (count 0).
    start: u32,
    count: u32,
}

#[derive(Debug, Clone)]
pub struct Bvh {
    nodes: Vec<Node>,
    /// Triangle ids in leaf order.
    pub order: Vec<u32>,
    pub tris: Vec<[V3; 3]>,
}

#[derive(Debug, Clone, Copy)]
pub struct Hit {
    pub tri: u32,
    pub t: f64,
    /// +1 the ray met the triangle's front (counter-clockwise) side, -1 its back.
    pub facing: f64,
}

const LEAF: usize = 4;

impl Bvh {
    pub fn new(tris: Vec<[V3; 3]>) -> Bvh {
        let n = tris.len();
        let mut order: Vec<u32> = (0..n as u32).collect();
        let boxes: Vec<Aabb> = tris
            .iter()
            .map(|t| {
                let mut b = Aabb::empty();
                t.iter().for_each(|p| b.grow(*p));
                b
            })
            .collect();
        let cents: Vec<V3> = boxes.iter().map(|b| b.center()).collect();
        let mut nodes = vec![Node { bb: Aabb::empty(), start: 0, count: 0 }];
        if n > 0 {
            build(&mut nodes, 0, &mut order, 0, n, &boxes, &cents);
        }
        Bvh { nodes, order, tris }
    }

    pub fn len(&self) -> usize {
        self.tris.len()
    }
    pub fn is_empty(&self) -> bool {
        self.tris.is_empty()
    }
    pub fn bounds(&self) -> Aabb {
        self.nodes[0].bb
    }

    /// Nearest hit along the ray within `tmin..tmax` that `keep(tri)` accepts.
    pub fn ray(&self, o: V3, d: V3, tmin: f64, tmax: f64, keep: &dyn Fn(u32) -> bool) -> Option<Hit> {
        if self.tris.is_empty() {
            return None;
        }
        let inv = [1.0 / d[0], 1.0 / d[1], 1.0 / d[2]];
        let mut best: Option<Hit> = None;
        let mut limit = tmax;
        let mut stack = vec![0usize];
        while let Some(ni) = stack.pop() {
            let nd = &self.nodes[ni];
            if !slab(&nd.bb, o, inv, tmin, limit) {
                continue;
            }
            if nd.count > 0 {
                for &ti in &self.order[nd.start as usize..(nd.start + nd.count) as usize] {
                    let t = &self.tris[ti as usize];
                    if let Some((tt, _, _, f)) = v3::ray_triangle(o, d, t[0], t[1], t[2]) {
                        if tt > tmin && tt < limit && keep(ti) {
                            limit = tt;
                            best = Some(Hit { tri: ti, t: tt, facing: f });
                        }
                    }
                }
            } else {
                stack.push(nd.start as usize);
                stack.push(nd.start as usize + 1);
            }
        }
        best
    }

    /// Every hit along the ray (unsorted), for parity and layer counts.
    pub fn ray_all(&self, o: V3, d: V3, tmin: f64, tmax: f64, out: &mut Vec<Hit>) {
        out.clear();
        if self.tris.is_empty() {
            return;
        }
        let inv = [1.0 / d[0], 1.0 / d[1], 1.0 / d[2]];
        let mut stack = vec![0usize];
        while let Some(ni) = stack.pop() {
            let nd = &self.nodes[ni];
            if !slab(&nd.bb, o, inv, tmin, tmax) {
                continue;
            }
            if nd.count > 0 {
                for &ti in &self.order[nd.start as usize..(nd.start + nd.count) as usize] {
                    let t = &self.tris[ti as usize];
                    if let Some((tt, _, _, f)) = v3::ray_triangle(o, d, t[0], t[1], t[2]) {
                        if tt > tmin && tt < tmax {
                            out.push(Hit { tri: ti, t: tt, facing: f });
                        }
                    }
                }
            } else {
                stack.push(nd.start as usize);
                stack.push(nd.start as usize + 1);
            }
        }
    }

    /// Nearest point on the surface to `p` among triangles `keep` accepts, within `max_dist`:
    /// (triangle, point, distance).
    pub fn nearest(&self, p: V3, max_dist: f64, keep: &dyn Fn(u32) -> bool) -> Option<(u32, V3, f64)> {
        if self.tris.is_empty() {
            return None;
        }
        let mut best_d2 = max_dist * max_dist;
        let mut best = None;
        let mut stack = vec![0usize];
        while let Some(ni) = stack.pop() {
            let nd = &self.nodes[ni];
            if nd.bb.dist2(p) > best_d2 {
                continue;
            }
            if nd.count > 0 {
                for &ti in &self.order[nd.start as usize..(nd.start + nd.count) as usize] {
                    if !keep(ti) {
                        continue;
                    }
                    let t = &self.tris[ti as usize];
                    let q = v3::closest_on_triangle(p, t[0], t[1], t[2]);
                    let d2 = v3::dot(v3::sub(q, p), v3::sub(q, p));
                    if d2 < best_d2 {
                        best_d2 = d2;
                        best = Some((ti, q));
                    }
                }
            } else {
                // Visit the nearer child first.
                let (l, r) = (nd.start as usize, nd.start as usize + 1);
                let (dl, dr) = (self.nodes[l].bb.dist2(p), self.nodes[r].bb.dist2(p));
                if dl < dr {
                    stack.push(r);
                    stack.push(l);
                } else {
                    stack.push(l);
                    stack.push(r);
                }
            }
        }
        best.map(|(t, q)| (t, q, best_d2.sqrt()))
    }

    /// Triangles whose boxes overlap `bb` (padded).
    pub fn query(&self, bb: &Aabb, pad: f64, out: &mut Vec<u32>) {
        out.clear();
        if self.tris.is_empty() {
            return;
        }
        let mut stack = vec![0usize];
        while let Some(ni) = stack.pop() {
            let nd = &self.nodes[ni];
            if !nd.bb.overlaps(bb, pad) {
                continue;
            }
            if nd.count > 0 {
                for &ti in &self.order[nd.start as usize..(nd.start + nd.count) as usize] {
                    let t = &self.tris[ti as usize];
                    let mut tb = Aabb::empty();
                    t.iter().for_each(|p| tb.grow(*p));
                    if tb.overlaps(bb, pad) {
                        out.push(ti);
                    }
                }
            } else {
                stack.push(nd.start as usize);
                stack.push(nd.start as usize + 1);
            }
        }
    }

    /// Every pair of triangles (a from self, b from other) whose boxes overlap within `pad`.
    /// With `same` (other is self), each unordered pair is reported once with a < b.
    /// Stops after `cap` pairs and returns false (the caller must report the cap).
    pub fn pairs(&self, other: &Bvh, pad: f64, same: bool, cap: usize, f: &mut dyn FnMut(u32, u32)) -> bool {
        if self.tris.is_empty() || other.tris.is_empty() {
            return true;
        }
        let mut n = 0usize;
        let mut stack = vec![(0usize, 0usize)];
        while let Some((a, b)) = stack.pop() {
            let (na, nb) = (&self.nodes[a], &other.nodes[b]);
            if !na.bb.overlaps(&nb.bb, pad) {
                continue;
            }
            match (na.count > 0, nb.count > 0) {
                (true, true) => {
                    for &ta in &self.order[na.start as usize..(na.start + na.count) as usize] {
                        for &tb in &other.order[nb.start as usize..(nb.start + nb.count) as usize] {
                            if same && ta >= tb && a == b {
                                continue;
                            }
                            if same && a != b && ta == tb {
                                continue;
                            }
                            let (x, y) = if same && ta > tb { (tb, ta) } else { (ta, tb) };
                            let (t1, t2) = (&self.tris[x as usize], &other.tris[y as usize]);
                            let (mut b1, mut b2) = (Aabb::empty(), Aabb::empty());
                            t1.iter().for_each(|p| b1.grow(*p));
                            t2.iter().for_each(|p| b2.grow(*p));
                            if b1.overlaps(&b2, pad) {
                                n += 1;
                                if n > cap {
                                    return false;
                                }
                                f(x, y);
                            }
                        }
                    }
                }
                (false, true) => {
                    stack.push((na.start as usize, b));
                    stack.push((na.start as usize + 1, b));
                }
                (true, false) => {
                    stack.push((a, nb.start as usize));
                    stack.push((a, nb.start as usize + 1));
                }
                (false, false) => {
                    let (l1, r1, l2, r2) = (na.start as usize, na.start as usize + 1, nb.start as usize, nb.start as usize + 1);
                    if same && a == b {
                        stack.push((l1, l2));
                        stack.push((l1, r2));
                        stack.push((r1, r2));
                    } else {
                        stack.push((l1, l2));
                        stack.push((l1, r2));
                        stack.push((r1, l2));
                        stack.push((r1, r2));
                    }
                }
            }
        }
        true
    }
}

fn slab(bb: &Aabb, o: V3, inv: V3, tmin: f64, tmax: f64) -> bool {
    let mut t0 = tmin;
    let mut t1 = tmax;
    for k in 0..3 {
        let mut a = (bb.min[k] - o[k]) * inv[k];
        let mut b = (bb.max[k] - o[k]) * inv[k];
        if a.is_nan() || b.is_nan() {
            // ray parallel to the slab and on its boundary: keep
            if o[k] < bb.min[k] || o[k] > bb.max[k] {
                return false;
            }
            continue;
        }
        if a > b {
            std::mem::swap(&mut a, &mut b);
        }
        t0 = t0.max(a);
        t1 = t1.min(b);
        if t0 > t1 * (1.0 + 4.0 * f64::EPSILON) + 1e-12 {
            return false;
        }
    }
    true
}

fn build(nodes: &mut Vec<Node>, ni: usize, order: &mut [u32], start: usize, end: usize, boxes: &[Aabb], cents: &[V3]) {
    let mut bb = Aabb::empty();
    let mut cb = Aabb::empty();
    for &t in &order[start..end] {
        bb.union(&boxes[t as usize]);
        cb.grow(cents[t as usize]);
    }
    nodes[ni].bb = bb;
    let n = end - start;
    if n <= LEAF {
        nodes[ni].start = start as u32;
        nodes[ni].count = n as u32;
        return;
    }
    let s = cb.size();
    let axis = if s[0] >= s[1] && s[0] >= s[2] { 0 } else if s[1] >= s[2] { 1 } else { 2 };
    let mid = start + n / 2;
    order[start..end].sort_by(|a, b| cents[*a as usize][axis].partial_cmp(&cents[*b as usize][axis]).unwrap_or(std::cmp::Ordering::Equal).then(a.cmp(b)));
    let l = nodes.len();
    nodes.push(Node { bb: Aabb::empty(), start: 0, count: 0 });
    nodes.push(Node { bb: Aabb::empty(), start: 0, count: 0 });
    nodes[ni].start = l as u32;
    nodes[ni].count = 0;
    build(nodes, l, order, start, mid, boxes, cents);
    build(nodes, l + 1, order, mid, end, boxes, cents);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(n: usize, z: f64) -> Vec<[V3; 3]> {
        let mut t = Vec::new();
        for i in 0..n {
            for j in 0..n {
                let (x, y) = (i as f64, j as f64);
                t.push([[x, y, z], [x + 1.0, y, z], [x + 1.0, y + 1.0, z]]);
                t.push([[x, y, z], [x + 1.0, y + 1.0, z], [x, y + 1.0, z]]);
            }
        }
        t
    }

    #[test]
    fn ray_and_nearest_match_brute_force() {
        let mut tris = grid(10, 0.0);
        tris.extend(grid(10, 3.0));
        let b = Bvh::new(tris.clone());
        let h = b.ray([2.3, 3.6, 10.0], [0.0, 0.0, -1.0], 0.0, 100.0, &|_| true).unwrap();
        assert!((h.t - 7.0).abs() < 1e-9 && h.facing > 0.0);
        let mut all = Vec::new();
        b.ray_all([2.3, 3.6, 10.0], [0.0, 0.0, -1.0], 0.0, 100.0, &mut all);
        assert_eq!(all.len(), 2);
        let (_, q, d) = b.nearest([4.2, 4.7, 1.0], 50.0, &|_| true).unwrap();
        assert!((d - 1.0).abs() < 1e-9 && q[2] == 0.0, "{d} {q:?}");
        let (_, _, d) = b.nearest([4.2, 4.7, 2.0], 50.0, &|t| t >= 200).unwrap();
        assert!((d - 1.0).abs() < 1e-9);
    }

    #[test]
    fn pairs_find_every_overlap_once() {
        let tris = grid(6, 0.0);
        let b = Bvh::new(tris.clone());
        let mut got = std::collections::BTreeSet::new();
        assert!(b.pairs(&b, 0.0, true, usize::MAX, &mut |x, y| {
            assert!(x < y);
            assert!(got.insert((x, y)));
        }));
        let mut brute = std::collections::BTreeSet::new();
        for i in 0..tris.len() {
            for j in i + 1..tris.len() {
                let (mut a, mut c) = (Aabb::empty(), Aabb::empty());
                tris[i].iter().for_each(|p| a.grow(*p));
                tris[j].iter().for_each(|p| c.grow(*p));
                if a.overlaps(&c, 0.0) {
                    brute.insert((i as u32, j as u32));
                }
            }
        }
        assert_eq!(got, brute);
        let mut k = 0;
        assert!(!b.pairs(&b, 0.0, true, 5, &mut |_, _| k += 1));
        assert_eq!(k, 5);
    }
}
