//! Triangle-triangle intersection and coplanar overlap.
//!
//! `properly_intersect` is scene3d's embedding predicate (`kernel/embed.ts`): two triangles cross
//! when they overlap in positive measure (a transversal crossing, Möller's interval test, or a
//! coplanar area overlap by separating axes); a shared vertex or edge, or a graze along an edge,
//! is a measure-zero touch and is not reported. Every decision is the sign of an exact
//! determinant (`exact::orient3d` / `orient2d`), so the verdict is the same on every machine.
//! The crossing interval's endpoints are formed in floating point only to compare their order
//! along the line, which is where scene3d also reads rationals; ties there are resolved towards
//! "touch", never towards "cross".
//!
//! `coplanar_overlap` is scene3d's z-fighting measure (runner `coplanar_overlap`): same-facing,
//! coplanar within a tolerance, and the overlap area by Sutherland-Hodgman clipping in the
//! dominant-axis projection.

use crate::exact::{orient2d, orient3d};
use crate::v3::{self, V3};

pub type Tri = [V3; 3];

fn degenerate(t: &Tri) -> bool {
    // Exact: all three orientations with an off-plane probe vanish iff the points are collinear.
    let n = v3::tri_normal(t[0], t[1], t[2]);
    if n == [0.0, 0.0, 0.0] {
        // could still be a tiny non-collinear triangle whose float cross product underflows;
        // decide in the three coordinate projections exactly
        let p = |i: usize, j: usize| orient2d([t[0][i], t[0][j]], [t[1][i], t[1][j]], [t[2][i], t[2][j]]) == 0;
        return p(0, 1) && p(1, 2) && p(0, 2);
    }
    let p = |i: usize, j: usize| orient2d([t[0][i], t[0][j]], [t[1][i], t[1][j]], [t[2][i], t[2][j]]) == 0;
    p(0, 1) && p(1, 2) && p(0, 2)
}

/// Do two triangles overlap in positive measure (cross or overlap in area)?
pub fn properly_intersect(t1: &Tri, t2: &Tri) -> bool {
    if degenerate(t1) || degenerate(t2) {
        return false;
    }
    let s2: [i8; 3] = [orient3d(t1[0], t1[1], t1[2], t2[0]), orient3d(t1[0], t1[1], t1[2], t2[1]), orient3d(t1[0], t1[1], t1[2], t2[2])];
    if s2 == [0, 0, 0] {
        return coplanar_area_overlap(t1, t2);
    }
    if !(s2.iter().any(|&s| s < 0) && s2.iter().any(|&s| s > 0)) {
        return false;
    }
    let s1: [i8; 3] = [orient3d(t2[0], t2[1], t2[2], t1[0]), orient3d(t2[0], t2[1], t2[2], t1[1]), orient3d(t2[0], t2[1], t2[2], t1[2])];
    if !(s1.iter().any(|&s| s < 0) && s1.iter().any(|&s| s > 0)) {
        return false;
    }
    // one line direction for both intervals (n1 x n2), or the projections disagree in sign
    let dir = v3::cross(v3::tri_normal(t1[0], t1[1], t1[2]), v3::tri_normal(t2[0], t2[1], t2[2]));
    match (cross_interval(t1, t2, dir), cross_interval(t2, t1, dir)) {
        (Some(a), Some(b)) => a.0.max(b.0) < a.1.min(b.1),
        _ => false,
    }
}

/// The interval of `t`'s crossing with `other`'s plane, projected on the planes' intersection line.
fn cross_interval(t: &Tri, other: &Tri, dir: V3) -> Option<(f64, f64)> {
    let n2 = v3::tri_normal(other[0], other[1], other[2]);
    let s: Vec<i8> = t.iter().map(|p| orient3d(other[0], other[1], other[2], *p)).collect();
    let d: Vec<f64> = t.iter().map(|p| v3::dot(v3::sub(*p, other[0]), n2)).collect();
    let mut params = Vec::new();
    for i in 0..3 {
        if s[i] == 0 {
            params.push(v3::dot(t[i], dir));
        }
    }
    for i in 0..3 {
        let j = (i + 1) % 3;
        if (s[i] as i32) * (s[j] as i32) < 0 {
            let w = d[i] / (d[i] - d[j]);
            let pi = v3::dot(t[i], dir);
            let pj = v3::dot(t[j], dir);
            params.push(pi + (pj - pi) * w);
        }
    }
    if params.len() < 2 {
        return None;
    }
    let lo = params.iter().cloned().fold(f64::INFINITY, f64::min);
    let hi = params.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    Some((lo, hi))
}

/// The segment where two crossing triangles meet (for curve length), in floating point.
pub fn crossing_segment(t1: &Tri, t2: &Tri) -> Option<(V3, V3)> {
    let mut pts: Vec<V3> = Vec::new();
    for (a, b) in [(t1, t2), (t2, t1)] {
        let n = v3::tri_normal(b[0], b[1], b[2]);
        for i in 0..3 {
            let (p, q) = (a[i], a[(i + 1) % 3]);
            let dp = v3::dot(v3::sub(p, b[0]), n);
            let dq = v3::dot(v3::sub(q, b[0]), n);
            if (dp < 0.0) != (dq < 0.0) && dp != dq {
                let x = v3::lerp(p, q, dp / (dp - dq));
                if inside_tri(x, b, n) {
                    pts.push(x);
                }
            }
        }
    }
    if pts.len() < 2 {
        return None;
    }
    let (mut best, mut bd) = ((pts[0], pts[1]), -1.0);
    for i in 0..pts.len() {
        for j in i + 1..pts.len() {
            let d = v3::dist(pts[i], pts[j]);
            if d > bd {
                bd = d;
                best = (pts[i], pts[j]);
            }
        }
    }
    Some(best)
}

fn inside_tri(x: V3, t: &Tri, n: V3) -> bool {
    let tol = -1e-9 * v3::dot(n, n);
    (0..3).all(|i| v3::dot(v3::cross(v3::sub(t[(i + 1) % 3], t[i]), v3::sub(x, t[i])), n) >= tol)
}

/// Coplanar triangles overlapping in area: separating-axis test in the dominant projection.
fn coplanar_area_overlap(t1: &Tri, t2: &Tri) -> bool {
    let n = v3::tri_normal(t1[0], t1[1], t1[2]);
    let ax = [n[0].abs(), n[1].abs(), n[2].abs()];
    let drop = if ax[0] >= ax[1] && ax[0] >= ax[2] { 0 } else if ax[1] >= ax[2] { 1 } else { 2 };
    let (i, j) = match drop {
        0 => (1, 2),
        1 => (0, 2),
        _ => (0, 1),
    };
    let p2 = |p: V3| [p[i], p[j]];
    let separated = |a: &Tri, b: &Tri| {
        for e in 0..3 {
            let (p, q, r) = (p2(a[e]), p2(a[(e + 1) % 3]), p2(a[(e + 2) % 3]));
            let inner = orient2d(p, q, r);
            if inner == 0 {
                continue;
            }
            if b.iter().all(|v| orient2d(p, q, p2(*v)) != inner) {
                return true;
            }
        }
        false
    };
    !(separated(t1, t2) || separated(t2, t1))
}

/// Same-facing coplanar overlap of two triangles: the overlap area, or 0. `plane_tol` is the
/// allowed distance between the planes; `cos_tol` the minimum dot of the unit normals.
pub fn coplanar_overlap(t1: &Tri, t2: &Tri, plane_tol: f64, cos_tol: f64) -> f64 {
    let n1 = v3::norm(v3::tri_normal(t1[0], t1[1], t1[2]));
    let n2 = v3::norm(v3::tri_normal(t2[0], t2[1], t2[2]));
    if v3::dot(n1, n2) < cos_tol {
        return 0.0;
    }
    for p in t2 {
        if v3::dot(n1, v3::sub(*p, t1[0])).abs() > plane_tol {
            return 0.0;
        }
    }
    let ax = [n1[0].abs(), n1[1].abs(), n1[2].abs()];
    let drop = if ax[0] >= ax[1] && ax[0] >= ax[2] { 0 } else if ax[1] >= ax[2] { 1 } else { 2 };
    let (i, j) = match drop {
        0 => (1, 2),
        1 => (2, 0),
        _ => (0, 1),
    };
    let flip = n1[drop] < 0.0;
    let proj = |t: &Tri| -> Vec<[f64; 2]> {
        let mut v: Vec<[f64; 2]> = t.iter().map(|p| [p[i], p[j]]).collect();
        if flip {
            v.reverse();
        }
        v
    };
    let poly = clip_convex(&proj(t1), &proj(t2));
    // Divide out the projection's foreshortening to get the area in the plane.
    shoelace(&poly).abs() / ax[drop].max(1e-12)
}

/// Sutherland-Hodgman: `subject` clipped by the convex counter-clockwise polygon `clip`.
pub fn clip_convex(subject: &[[f64; 2]], clip: &[[f64; 2]]) -> Vec<[f64; 2]> {
    let mut out = subject.to_vec();
    let n = clip.len();
    for k in 0..n {
        if out.is_empty() {
            break;
        }
        let (a, b) = (clip[k], clip[(k + 1) % n]);
        let side = |p: [f64; 2]| (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]);
        let mut next = Vec::with_capacity(out.len() + 2);
        for m in 0..out.len() {
            let cur = out[m];
            let prev = out[(m + out.len() - 1) % out.len()];
            let (sc, sp) = (side(cur), side(prev));
            if sc >= 0.0 {
                if sp < 0.0 {
                    let t = sp / (sp - sc);
                    next.push([prev[0] + t * (cur[0] - prev[0]), prev[1] + t * (cur[1] - prev[1])]);
                }
                next.push(cur);
            } else if sp >= 0.0 {
                let t = sp / (sp - sc);
                next.push([prev[0] + t * (cur[0] - prev[0]), prev[1] + t * (cur[1] - prev[1])]);
            }
        }
        out = next;
    }
    out
}

/// Signed area of a 2D polygon (counter-clockwise positive).
pub fn shoelace(p: &[[f64; 2]]) -> f64 {
    if p.len() < 3 {
        return 0.0;
    }
    let mut s = 0.0;
    for k in 0..p.len() {
        let (a, b) = (p[k], p[(k + 1) % p.len()]);
        s += a[0] * b[1] - b[0] * a[1];
    }
    s / 2.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crossing_touching_and_coplanar() {
        let a: Tri = [[0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [0.0, 2.0, 0.0]];
        // pierces a vertically
        let b: Tri = [[0.5, 0.5, -1.0], [0.5, 0.5, 1.0], [1.5, -1.0, 0.0]];
        assert!(properly_intersect(&a, &b));
        let (p, q) = crossing_segment(&a, &b).unwrap();
        assert!(p[2].abs() < 1e-12 && q[2].abs() < 1e-12);
        // shares an edge, hinged: a touch, not a crossing
        let c: Tri = [[0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [0.0, 0.0, 2.0]];
        assert!(!properly_intersect(&a, &c));
        // rests on a with one vertex: a touch
        let d: Tri = [[0.5, 0.5, 0.0], [0.5, 0.5, 1.0], [1.0, 0.0, 1.0]];
        assert!(!properly_intersect(&a, &d));
        // coplanar overlap in area, and the measured area
        let e: Tri = [[0.5, 0.5, 0.0], [2.5, 0.5, 0.0], [0.5, 2.5, 0.0]];
        assert!(properly_intersect(&a, &e));
        let area = coplanar_overlap(&a, &e, 1e-6, 0.999);
        assert!((area - 0.5).abs() < 1e-12, "{area}");
        // coplanar, sharing an edge only
        let f: Tri = [[2.0, 0.0, 0.0], [0.0, 2.0, 0.0], [2.0, 2.0, 0.0]];
        assert!(!properly_intersect(&a, &f));
        assert!(coplanar_overlap(&a, &f, 1e-6, 0.999) < 1e-12);
        // opposite facing coincident faces do not z-fight under culling
        let g: Tri = [a[0], a[2], a[1]];
        assert_eq!(coplanar_overlap(&a, &g, 1e-6, 0.999), 0.0);
        // a fold: crossing intervals that start at the same point (regression: the line direction
        // must be shared by both intervals)
        let t1: Tri = [[10.0, 0.0, 20.0], [16.0, 0.0, 20.0], [16.0, 4.0, 20.0]];
        let t2: Tri = [[16.0, 0.0, 24.0], [13.0, 0.0, 16.0], [13.0, 4.0, 16.0]];
        assert!(properly_intersect(&t1, &t2) && properly_intersect(&t2, &t1));
        // degenerate never crosses
        let z: Tri = [[0.0, 0.0, -1.0], [1.0, 1.0, 1.0], [2.0, 2.0, 3.0]];
        assert!(!properly_intersect(&a, &z));
    }
}
