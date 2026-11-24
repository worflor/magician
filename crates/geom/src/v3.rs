//! Small vector helpers on `[f64; 3]`.

pub type V3 = [f64; 3];

#[inline]
pub fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
#[inline]
pub fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
#[inline]
pub fn scale(a: V3, s: f64) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}
#[inline]
pub fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
#[inline]
pub fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
#[inline]
pub fn len(a: V3) -> f64 {
    dot(a, a).sqrt()
}
#[inline]
pub fn dist(a: V3, b: V3) -> f64 {
    len(sub(a, b))
}
#[inline]
pub fn norm(a: V3) -> V3 {
    let l = len(a);
    if l > 0.0 {
        scale(a, 1.0 / l)
    } else {
        [0.0, 0.0, 0.0]
    }
}
#[inline]
pub fn lerp(a: V3, b: V3, t: f64) -> V3 {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}
/// Twice the area vector of triangle a, b, c (its unnormalised normal).
#[inline]
pub fn tri_normal(a: V3, b: V3, c: V3) -> V3 {
    cross(sub(b, a), sub(c, a))
}
pub fn tri_area(a: V3, b: V3, c: V3) -> f64 {
    0.5 * len(tri_normal(a, b, c))
}
pub fn centroid(a: V3, b: V3, c: V3) -> V3 {
    [(a[0] + b[0] + c[0]) / 3.0, (a[1] + b[1] + c[1]) / 3.0, (a[2] + b[2] + c[2]) / 3.0]
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aabb {
    pub min: V3,
    pub max: V3,
}

impl Default for Aabb {
    fn default() -> Aabb {
        Aabb::empty()
    }
}

impl Aabb {
    pub fn empty() -> Aabb {
        Aabb { min: [f64::INFINITY; 3], max: [f64::NEG_INFINITY; 3] }
    }
    pub fn grow(&mut self, p: V3) {
        for k in 0..3 {
            self.min[k] = self.min[k].min(p[k]);
            self.max[k] = self.max[k].max(p[k]);
        }
    }
    pub fn union(&mut self, o: &Aabb) {
        for k in 0..3 {
            self.min[k] = self.min[k].min(o.min[k]);
            self.max[k] = self.max[k].max(o.max[k]);
        }
    }
    pub fn is_empty(&self) -> bool {
        self.min[0] > self.max[0]
    }
    pub fn overlaps(&self, o: &Aabb, pad: f64) -> bool {
        (0..3).all(|k| self.min[k] - pad <= o.max[k] && o.min[k] - pad <= self.max[k])
    }
    pub fn size(&self) -> V3 {
        sub(self.max, self.min)
    }
    pub fn center(&self) -> V3 {
        scale(add(self.min, self.max), 0.5)
    }
    /// Squared distance from a point to the box (0 inside).
    pub fn dist2(&self, p: V3) -> f64 {
        let mut d = 0.0;
        for k in 0..3 {
            let v = if p[k] < self.min[k] {
                self.min[k] - p[k]
            } else if p[k] > self.max[k] {
                p[k] - self.max[k]
            } else {
                0.0
            };
            d += v * v;
        }
        d
    }
    /// Largest per-axis gap between two boxes (negative when they overlap on every axis).
    pub fn gap(&self, o: &Aabb) -> f64 {
        (0..3).map(|k| (o.min[k] - self.max[k]).max(self.min[k] - o.max[k])).fold(f64::NEG_INFINITY, f64::max)
    }
}

/// Closest point on triangle (a, b, c) to p (Ericson, Real-Time Collision Detection 5.1.5).
pub fn closest_on_triangle(p: V3, a: V3, b: V3, c: V3) -> V3 {
    let ab = sub(b, a);
    let ac = sub(c, a);
    let ap = sub(p, a);
    let d1 = dot(ab, ap);
    let d2 = dot(ac, ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = sub(p, b);
    let d3 = dot(ab, bp);
    let d4 = dot(ac, bp);
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = d1 / (d1 - d3);
        return add(a, scale(ab, v));
    }
    let cp = sub(p, c);
    let d5 = dot(ab, cp);
    let d6 = dot(ac, cp);
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = d2 / (d2 - d6);
        return add(a, scale(ac, w));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return add(b, scale(sub(c, b), w));
    }
    let denom = 1.0 / (va + vb + vc);
    let v = vb * denom;
    let w = vc * denom;
    add(a, add(scale(ab, v), scale(ac, w)))
}

/// Ray/triangle (Möller-Trumbore, no culling). Returns (t, u, v, det sign: +1 front, -1 back).
pub fn ray_triangle(o: V3, d: V3, a: V3, b: V3, c: V3) -> Option<(f64, f64, f64, f64)> {
    let e1 = sub(b, a);
    let e2 = sub(c, a);
    let p = cross(d, e2);
    let det = dot(e1, p);
    if det.abs() < 1e-18 {
        return None;
    }
    let inv = 1.0 / det;
    let s = sub(o, a);
    let u = dot(s, p) * inv;
    if !(-1e-12..=1.0 + 1e-12).contains(&u) {
        return None;
    }
    let q = cross(s, e1);
    let v = dot(d, q) * inv;
    if v < -1e-12 || u + v > 1.0 + 1e-12 {
        return None;
    }
    let t = dot(e2, q) * inv;
    // det > 0: the ray meets the counter-clockwise (front) side.
    Some((t, u, v, if det > 0.0 { 1.0 } else { -1.0 }))
}

/// Cyclic Jacobi eigen-decomposition of a symmetric 3x3 (scene3d's `_principal_axes`).
/// Returns eigenvalues and unit eigenvectors as columns (vecs[k] is the k-th vector).
pub fn jacobi3(m: [[f64; 3]; 3]) -> ([f64; 3], [V3; 3]) {
    let mut c = m;
    let mut v = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    for _ in 0..50 {
        let (mut p, mut q) = (0, 1);
        if c[0][2].abs() > c[p][q].abs() {
            p = 0;
            q = 2;
        }
        if c[1][2].abs() > c[p][q].abs() {
            p = 1;
            q = 2;
        }
        if c[p][q].abs() < 1e-15 * (c[0][0].abs() + c[1][1].abs() + c[2][2].abs() + 1e-300) {
            break;
        }
        let theta = 0.5 * (2.0 * c[p][q]).atan2(c[q][q] - c[p][p]);
        let (s, co) = theta.sin_cos();
        for k in 0..3 {
            let (cp, cq) = (c[k][p], c[k][q]);
            c[k][p] = co * cp - s * cq;
            c[k][q] = s * cp + co * cq;
        }
        for k in 0..3 {
            let (cp, cq) = (c[p][k], c[q][k]);
            c[p][k] = co * cp - s * cq;
            c[q][k] = s * cp + co * cq;
        }
        for k in 0..3 {
            let (vp, vq) = (v[k][p], v[k][q]);
            v[k][p] = co * vp - s * vq;
            v[k][q] = s * vp + co * vq;
        }
    }
    ([c[0][0], c[1][1], c[2][2]], [[v[0][0], v[1][0], v[2][0]], [v[0][1], v[1][1], v[2][1]], [v[0][2], v[1][2], v[2][2]]])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn closest_point_regions() {
        let (a, b, c) = ([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
        let q = closest_on_triangle([0.2, 0.2, 5.0], a, b, c);
        assert!(dist(q, [0.2, 0.2, 0.0]) < 1e-12, "{q:?}");
        assert_eq!(closest_on_triangle([-1.0, -1.0, 0.0], a, b, c), a);
        assert_eq!(closest_on_triangle([2.0, 0.0, 0.0], a, b, c), b);
        let p = closest_on_triangle([1.0, 1.0, 0.0], a, b, c);
        assert!((p[0] - 0.5).abs() < 1e-12 && (p[1] - 0.5).abs() < 1e-12);
    }
    #[test]
    fn ray_hits_front_and_back() {
        let (a, b, c) = ([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
        let (t, _, _, s) = ray_triangle([0.2, 0.2, 1.0], [0.0, 0.0, -1.0], a, b, c).unwrap();
        assert!((t - 1.0).abs() < 1e-12 && s > 0.0);
        let (_, _, _, s) = ray_triangle([0.2, 0.2, -1.0], [0.0, 0.0, 1.0], a, b, c).unwrap();
        assert!(s < 0.0);
        assert!(ray_triangle([2.0, 2.0, 1.0], [0.0, 0.0, -1.0], a, b, c).is_none());
    }
    #[test]
    fn jacobi_diagonalises() {
        let (vals, vecs) = jacobi3([[2.0, 1.0, 0.0], [1.0, 2.0, 0.0], [0.0, 0.0, 5.0]]);
        let mut s = vals;
        s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        assert!((s[0] - 1.0).abs() < 1e-12 && (s[1] - 3.0).abs() < 1e-12 && (s[2] - 5.0).abs() < 1e-12);
        for v in vecs {
            assert!((len(v) - 1.0).abs() < 1e-12);
        }
    }
}
