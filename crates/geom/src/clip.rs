//! Half-space clipping and plane sections (scene3d `kernel/clip.ts`).
//!
//! `clip` keeps the part of a closed mesh on the `n . x <= d` side and caps the cut so the result
//! is still closed: a vertex's side is decided exactly (`exact::Exact` on float inputs), a
//! straddling edge splits at the crossing computed from its two endpoints in a fixed order, so two
//! faces sharing that edge compute the same crossing and the cap welds with no tolerance; the cap
//! boundary is the residue of unmatched half-edges, assembled into loops and triangulated by
//! `triangulate::triangulate_loop`. An input that was already open along the cut is returned
//! uncapped (never a guessed cap over a hole).
//!
//! `section` returns the segments where a plane cuts a triangle soup: the cross-section used for
//! girth and silhouette-by-height comparisons.

use crate::exact::Exact;
use crate::triangulate::triangulate_loop;
use crate::v3::{self, V3};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy)]
pub struct Plane {
    pub n: V3,
    pub d: f64,
}

/// Exact sign of n . p - d.
pub fn side(pl: &Plane, p: V3) -> i8 {
    let mut e = Exact::from(-pl.d);
    for k in 0..3 {
        e.add_assign(&Exact::product(pl.n[k], p[k]));
    }
    e.sign()
}

fn crossing(a: V3, b: V3, pl: &Plane) -> V3 {
    // Order the endpoints canonically so a shared edge yields bit-identical points.
    let (p, q) = if (a[0], a[1], a[2]) <= (b[0], b[1], b[2]) { (a, b) } else { (b, a) };
    let fp = v3::dot(pl.n, p) - pl.d;
    let fq = v3::dot(pl.n, q) - pl.d;
    v3::lerp(p, q, fp / (fp - fq))
}

fn key(p: V3) -> [u64; 3] {
    let k = |x: f64| if x == 0.0 { 0 } else { x.to_bits() };
    [k(p[0]), k(p[1]), k(p[2])]
}

/// Clip a closed mesh to `n . x <= d` and cap it. Returns (positions, triangles, capped).
pub fn clip(pos: &[V3], tris: &[[u32; 3]], pl: &Plane) -> (Vec<V3>, Vec<[u32; 3]>, bool) {
    let sides: Vec<i8> = pos.iter().map(|p| side(pl, *p)).collect();
    let mut pts: Vec<V3> = Vec::new();
    let mut pool: HashMap<[u64; 3], u32> = HashMap::new();
    let mut intern = |p: V3, pts: &mut Vec<V3>| -> u32 {
        *pool.entry(key(p)).or_insert_with(|| {
            pts.push(p);
            pts.len() as u32 - 1
        })
    };
    let mut faces: Vec<Vec<u32>> = Vec::new();
    for t in tris {
        let s: Vec<i8> = t.iter().map(|&i| sides[i as usize]).collect();
        let any_out = s.iter().any(|&x| x > 0);
        let any_in = s.iter().any(|&x| x < 0);
        if !any_out && !any_in {
            // coplanar: keep only if it faces the removed side (it is then part of the cap)
            let n = v3::tri_normal(pos[t[0] as usize], pos[t[1] as usize], pos[t[2] as usize]);
            if v3::dot(n, pl.n) > 0.0 {
                faces.push(t.iter().map(|&i| intern(pos[i as usize], &mut pts)).collect());
            }
            continue;
        }
        if !any_out {
            faces.push(t.iter().map(|&i| intern(pos[i as usize], &mut pts)).collect());
            continue;
        }
        if !any_in {
            continue;
        }
        let mut out = Vec::new();
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            let (sa, sb) = (sides[a as usize], sides[b as usize]);
            if sa <= 0 {
                out.push(intern(pos[a as usize], &mut pts));
            }
            if (sa < 0 && sb > 0) || (sa > 0 && sb < 0) {
                out.push(intern(crossing(pos[a as usize], pos[b as usize], pl), &mut pts));
            }
        }
        if out.len() >= 3 {
            faces.push(out);
        }
    }
    // Residual half-edges are the cap boundary.
    let mut open: HashMap<(u32, u32), ()> = HashMap::new();
    for f in &faces {
        for k in 0..f.len() {
            let (a, b) = (f[k], f[(k + 1) % f.len()]);
            if a == b {
                continue;
            }
            if open.remove(&(b, a)).is_none() {
                open.insert((a, b), ());
            }
        }
    }
    let mut out_tris: Vec<[u32; 3]> = Vec::new();
    for f in &faces {
        for k in 1..f.len() - 1 {
            out_tris.push([f[0], f[k], f[k + 1]]);
        }
    }
    let mut cap: Vec<(u32, u32)> = open.keys().map(|&(a, b)| (b, a)).collect();
    cap.sort();
    if cap.iter().any(|(a, b)| side(pl, pts[*a as usize]) != 0 || side(pl, pts[*b as usize]) != 0) {
        return (pts, out_tris, false);
    }
    let mut by_tail: HashMap<u32, Vec<usize>> = HashMap::new();
    for (i, (t, _)) in cap.iter().enumerate() {
        by_tail.entry(*t).or_default().push(i);
    }
    let mut used = vec![false; cap.len()];
    let mut capped = true;
    for s in 0..cap.len() {
        if used[s] {
            continue;
        }
        let start = cap[s].0;
        let mut lp = Vec::new();
        let mut e = Some(s);
        let mut closed = false;
        while let Some(i) = e {
            if used[i] {
                break;
            }
            used[i] = true;
            lp.push(cap[i].0);
            let head = cap[i].1;
            if head == start {
                closed = true;
                break;
            }
            e = by_tail.get(&head).and_then(|v| v.iter().copied().find(|&j| !used[j]));
        }
        if !closed || lp.len() < 3 {
            capped = false;
            continue;
        }
        let (t, ok) = triangulate_loop(&lp, &pts);
        capped &= ok;
        out_tris.extend(t);
    }
    (pts, out_tris, capped)
}

/// Segments where the plane cuts the triangles.
pub fn section(pos: &[V3], tris: &[[u32; 3]], pl: &Plane) -> Vec<(V3, V3)> {
    let mut out = Vec::new();
    for t in tris {
        let p: Vec<V3> = t.iter().map(|&i| pos[i as usize]).collect();
        let s: Vec<i8> = p.iter().map(|q| side(pl, *q)).collect();
        let mut x = Vec::new();
        for k in 0..3 {
            let j = (k + 1) % 3;
            if s[k] == 0 {
                x.push(p[k]);
            }
            if (s[k] as i32) * (s[j] as i32) < 0 {
                x.push(crossing(p[k], p[j], pl));
            }
        }
        if x.len() == 2 {
            out.push((x[0], x[1]));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mass::mass;
    use crate::topo::{tests::cube, Topology};

    #[test]
    fn clip_cube_in_half_stays_closed() {
        let (p, t) = cube([0.0; 3], 2.0);
        let (q, u, capped) = clip(&p, &t, &Plane { n: [1.0, 0.0, 0.0], d: 0.5 });
        assert!(capped);
        let topo = Topology::build(&q, &u);
        assert_eq!(topo.shells.len(), 1);
        assert!(topo.shells[0].closed, "{:?}", topo.shells[0]);
        let m = mass(&q, &u).unwrap();
        assert!((m.volume - 2.0).abs() < 1e-12, "{}", m.volume);
        assert_eq!(m.winding, 1);
        // a diagonal cut
        let (q, u, capped) = clip(&p, &t, &Plane { n: [1.0, 1.0, 1.0], d: 3.0 });
        assert!(capped);
        let m = mass(&q, &u).unwrap();
        assert!((m.volume - 4.0).abs() < 1e-9, "{}", m.volume);
    }

    #[test]
    fn section_of_a_cube_is_its_square() {
        let (p, t) = cube([0.0; 3], 2.0);
        let s = section(&p, &t, &Plane { n: [0.0, 0.0, 1.0], d: 1.0 });
        let len: f64 = s.iter().map(|(a, b)| v3::dist(*a, *b)).sum();
        assert!((len - 8.0).abs() < 1e-12, "{len}");
    }
}
