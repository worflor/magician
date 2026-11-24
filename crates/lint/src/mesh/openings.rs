//! Openings: every boundary loop of a single-sided shell, and what a viewer outside sees through it
//! (M102 open-sheet voids, M106 cut-body voids).
//!
//! Garment openings are probed through their cap: sample points on a triangulation of the loop,
//! rays toward a cone of outside viewers around the cap's normal. Two precision guards keep that
//! honest:
//!
//! - **Twin-backed shells are closed.** A single-sided sheet with an opposite-facing layer of another
//!   shell within `PLANE_TOL` behind every triangle is, to the renderer, one double-sided card: from
//!   any side one of the two layers faces the viewer, and the two are closer than the depth buffer
//!   can separate. Its outline is not an opening (scene3d treats a backed sheet the same way). The
//!   cap of a curved card spans its chord, so probing it reads the air beside the curve as a void.
//! - **Caps are triangulations of the actual loop.** A loop that touches itself at a vertex (a
//!   pinched outline) is split there into simple rings; a ring that still folds over in projection
//!   cannot be capped, and the opening is reported as not measured instead of probed through a fan
//!   that spans air.
//!
//! Cut-body openings (the hero's skin) are probed from **reachable views**, not along the cap: a cut
//! across the torso caps as a cross-section whose normal points down the body, and rays along it
//! look up inside the chest, where no camera goes. What a player sees when the garment over the cut
//! lifts is the place the skin was: a point just past the cut edge on the skin's continuation,
//! looked at from a cone around the skin's outward normal, from directions no other surface (the
//! body itself, a hull, a garment that does not lift there) blocks. Behind that point there must be
//! something drawn within `VOID_DEPTH`: a liner, a sunk hull, the restored faces.

use super::census::{ortho, side_of, Census, Hole, PLANE_TOL, PROBE_UNIT, RIM_MARGIN, VOID_DEPTH};
use geom::bvh::{Bvh, Hit};
use geom::v3::{self, V3};
use std::collections::HashMap;

/// Opposite-facing: unit normals at least this anti-aligned (within 25.8 degrees of opposite).
pub const TWIN_COS: f64 = 0.9;
/// From a cut edge to the point a viewer looks at, along the skin's continuation (0.3 units, 7.6 mm:
/// past the edge's own triangles, short of the far side of any limb).
pub const EDGE_STEP: f64 = 0.3;
/// Smallest opening that can show anything: one in-game pixel at gameplay range (PROBE_UNIT^2). A
/// slit between unwelded neighbours is a seam (M125), not a window.
pub const MIN_OPENING: f64 = PROBE_UNIT * PROBE_UNIT;
/// Cut-edge samples per loop (the edge is walked at most this many times).
pub const EDGE_SAMPLES: usize = 160;

/// Triangle `t` has an opposite-facing triangle of another shell within PLANE_TOL behind it, at its
/// centroid or near one of its corners: it is one layer of a double-sided card.
pub fn twin_backed(bvh: &Bvh, shell_of: &[u32], t: usize) -> bool {
    let tr = bvh.tris[t];
    let n = v3::norm(v3::tri_normal(tr[0], tr[1], tr[2]));
    if v3::len(n) == 0.0 {
        return true; // a zero-area triangle shows nothing either way
    }
    let c = v3::centroid(tr[0], tr[1], tr[2]);
    let s = shell_of[t];
    let keep = |u: u32| {
        if shell_of[u as usize] == s {
            return false;
        }
        let o = bvh.tris[u as usize];
        v3::dot(v3::norm(v3::tri_normal(o[0], o[1], o[2])), n) < -TWIN_COS
    };
    let pts = [c, v3::add(c, v3::scale(v3::sub(tr[0], c), 0.8)), v3::add(c, v3::scale(v3::sub(tr[1], c), 0.8)), v3::add(c, v3::scale(v3::sub(tr[2], c), 0.8))];
    pts.iter().any(|&p| bvh.nearest(p, PLANE_TOL, &keep).is_some())
}

/// Split a boundary loop at vertices it visits twice (a pinched outline) into simple rings.
pub fn simple_rings(lp: &[u32]) -> Vec<Vec<u32>> {
    let mut out = Vec::new();
    let mut stack: Vec<u32> = Vec::new();
    let mut at: HashMap<u32, usize> = HashMap::new();
    for &v in lp {
        if let Some(&i) = at.get(&v) {
            let ring: Vec<u32> = stack.drain(i + 1..).collect();
            for r in &ring {
                at.remove(r);
            }
            if ring.len() >= 2 {
                let mut full = vec![v];
                full.extend(ring);
                out.push(full);
            }
        } else {
            at.insert(v, stack.len());
            stack.push(v);
        }
    }
    if stack.len() >= 3 {
        out.push(stack);
    }
    out
}

pub fn cone(axis: V3) -> Vec<V3> {
    let (u, w) = ortho(axis);
    let mut dirs = vec![axis];
    for (deg, k) in [(40f64, 6usize), (70f64, 6usize)] {
        let (s, c) = deg.to_radians().sin_cos();
        for i in 0..k {
            let a = (i as f64 + if deg > 50.0 { 0.5 } else { 0.0 }) / k as f64 * std::f64::consts::TAU;
            dirs.push(v3::norm(v3::add(v3::scale(axis, c), v3::add(v3::scale(u, s * a.cos()), v3::scale(w, s * a.sin())))));
        }
    }
    dirs
}

/// What the viewer looking along -d at `p` sees beyond it: a void when nothing drawn faces them, or
/// only a surface farther than VOID_DEPTH that no probe view ever sees from outside.
pub fn sees_void(bvh: &Bvh, p: V3, d: V3, two_sided: &[bool], visible: &[bool], hits: &mut Vec<Hit>) -> (bool, bool) {
    let back = v3::scale(d, -1.0);
    bvh.ray_all(p, back, 1e-3, 1e4, hits);
    let first = hits.iter().filter(|x| x.facing > 0.0 || two_sided[x.tri as usize]).min_by(|a, b| a.t.partial_cmp(&b.t).unwrap_or(std::cmp::Ordering::Equal)).copied();
    match first {
        None => (true, true),
        Some(x) => (x.t > VOID_DEPTH && !visible[x.tri as usize], false),
    }
}

pub fn holes(c: &Census, bvh: &Bvh) -> Vec<Hole> {
    let g = &c.geo;
    let mat = |t: usize| c.materials.get(&g.surfs[g.tsurf[t] as usize].material);
    let open_ok: Vec<bool> = (0..g.tris.len()).map(|t| mat(t).is_some_and(|m| m.two_sided || m.alpha_test)).collect();
    let two_sided: Vec<bool> = (0..g.tris.len()).map(|t| mat(t).is_some_and(|m| m.two_sided)).collect();
    let skin_tri: Vec<bool> = (0..g.tris.len()).map(|t| c.skin_materials.contains(&g.surfs[g.tsurf[t] as usize].material)).collect();
    let visible = &c.tri_visible;
    let shell_of = &c.topo.shell_of;
    // garment rims: boundary edges of non-skin shells (with their shell), hashed into every 2-unit
    // cell they pass near; boundary edges of skin shells, with their one face
    let mut rim: HashMap<(i64, i64, i64), Vec<(V3, V3, u32)>> = HashMap::new();
    let mut edge_face: HashMap<(u32, u32), u32> = HashMap::new();
    let cell = |p: V3| ((p[0] / RIM_MARGIN).floor() as i64, (p[1] / RIM_MARGIN).floor() as i64, (p[2] / RIM_MARGIN).floor() as i64);
    for e in &c.topo.edges {
        if e.faces.len() != 1 {
            continue;
        }
        let t = e.faces[0].0;
        if skin_tri[t as usize] {
            edge_face.insert((e.a.min(e.b), e.a.max(e.b)), t);
            continue;
        }
        let (p, q) = (c.topo.pos[e.a as usize], c.topo.pos[e.b as usize]);
        let (lo, hi) = (cell([p[0].min(q[0]), p[1].min(q[1]), p[2].min(q[2])]), cell([p[0].max(q[0]), p[1].max(q[1]), p[2].max(q[2])]));
        if (hi.0 - lo.0 + 1) * (hi.1 - lo.1 + 1) * (hi.2 - lo.2 + 1) > 4096 {
            continue; // a degenerate span: no garment edge is 30 units long
        }
        for x in lo.0..=hi.0 {
            for y in lo.1..=hi.1 {
                for z in lo.2..=hi.2 {
                    rim.entry((x, y, z)).or_default().push((p, q, shell_of[t as usize]));
                }
            }
        }
    }
    // the garment shells whose rim comes within RIM_MARGIN of p (they can lift off it), and the nearest
    let rim_near = |p: V3| -> (f64, Vec<u32>) {
        let k = cell(p);
        let mut best = f64::INFINITY;
        let mut near: Vec<u32> = Vec::new();
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    for (a, b, s) in rim.get(&(k.0 + dx, k.1 + dy, k.2 + dz)).map(Vec::as_slice).unwrap_or(&[]) {
                        let ab = v3::sub(*b, *a);
                        let l2 = v3::dot(ab, ab);
                        let u = if l2 > 0.0 { (v3::dot(v3::sub(p, *a), ab) / l2).clamp(0.0, 1.0) } else { 0.0 };
                        let d = v3::dist(p, v3::add(*a, v3::scale(ab, u)));
                        best = best.min(d);
                        if d <= RIM_MARGIN && !near.contains(s) {
                            near.push(*s);
                        }
                    }
                }
            }
        }
        (best, near)
    };
    // Which shells to probe: single-sided ones with openings; a garment shell that is a twin-backed
    // double-sided card in full has no opening to look into.
    let shells: Vec<usize> = (0..c.topo.shells.len()).filter(|&si| !c.topo.shells[si].loops.is_empty()).collect();
    let probe_it: Vec<bool> = crate::par::map(&shells, |&si| {
        let sh = &c.topo.shells[si];
        if sh.tris.iter().all(|&t| open_ok[t as usize]) {
            return false;
        }
        let skin = sh.tris.iter().any(|&t| skin_tri[t as usize]);
        skin || !sh.tris.iter().all(|&t| open_ok[t as usize] || twin_backed(bvh, shell_of, t as usize))
    });
    let jobs: Vec<(usize, usize)> = shells.iter().zip(&probe_it).filter(|(_, &p)| p).flat_map(|(&si, _)| (0..c.topo.shells[si].loops.len()).map(move |li| (si, li))).collect();
    let found = crate::par::map(&jobs, |&(si, li)| -> Option<Hole> {
        let sh = &c.topo.shells[si];
        let lp = &sh.loops[li];
        if lp.verts.len() < 3 || lp.area < MIN_OPENING || v3::len(lp.normal) == 0.0 {
            return None;
        }
        let n = lp.normal;
        let mut h = Hole { shell: si, perimeter: lp.perimeter, area: lp.area, centroid: lp.centroid, normal: n, ..Default::default() };
        h.skin = sh.tris.iter().any(|&t| skin_tri[t as usize]);
        h.sheet = lp.area >= 0.9 * sh.area;
        let mut hits = Vec::new();
        if h.skin && !h.sheet {
            cut_edge_probe(c, bvh, &lp.verts, &mut h, &edge_face, &rim_near, &two_sided, visible, &mut hits);
        } else {
            // the cap: each simple ring of the loop, triangulated; a ring that folds over in projection
            // is not capped (and the opening says so)
            let mut cap: Vec<[u32; 3]> = Vec::new();
            for ring in simple_rings(&lp.verts) {
                let (t, ok) = geom::triangulate::triangulate_loop(&ring, &c.topo.pos);
                if ok {
                    cap.extend(t);
                } else {
                    h.cap_failed = true;
                }
            }
            let mut samples: Vec<(V3, f64)> = cap
                .iter()
                .map(|t| {
                    let (a, b, cc) = (c.topo.pos[t[0] as usize], c.topo.pos[t[1] as usize], c.topo.pos[t[2] as usize]);
                    (v3::centroid(a, b, cc), v3::tri_area(a, b, cc))
                })
                .collect();
            samples.sort_by(|x, y| y.1.partial_cmp(&x.1).unwrap_or(std::cmp::Ordering::Equal));
            samples.truncate(48);
            let mut worst = 0usize;
            for d in cone(n) {
                let mut voids_here = 0;
                for &(p, _) in &samples {
                    h.probes += 1;
                    let o = v3::add(p, v3::scale(d, 1e-3));
                    // Seen from the viewer, a surface it draws faces it: this outward ray meets that
                    // surface's back (or the surface is two-sided).
                    bvh.ray_all(o, d, 0.0, 1e4, &mut hits);
                    let covered = hits.iter().any(|x| x.facing < 0.0 || two_sided[x.tri as usize]);
                    if !covered {
                        h.exposed += 1;
                    }
                    let (void, through) = sees_void(bvh, v3::add(p, v3::scale(d, -1e-3)), d, &two_sided, visible, &mut hits);
                    if through {
                        h.see_through += 1;
                    }
                    if void {
                        h.unbacked += 1;
                        if !covered {
                            h.void_exposed += 1;
                            voids_here += 1;
                        }
                    }
                }
                if voids_here > worst {
                    worst = voids_here;
                    h.void_dir = Some(d);
                }
            }
        }
        let mut bone_w: HashMap<u16, f64> = HashMap::new();
        for &v in &lp.verts {
            let i = c.first_input[v as usize] as usize;
            for k in 0..8 {
                let (j, wt) = (g.joints[i][k], g.weights[i][k]);
                if j != u16::MAX && wt > 0.0 {
                    *bone_w.entry(j).or_default() += wt as f64;
                }
            }
        }
        h.bone = bone_w.iter().max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal).then(b.0.cmp(a.0))).and_then(|(j, _)| c.model.skeleton.names.get(*j as usize)).cloned().unwrap_or_else(|| "unskinned".into());
        h.side = side_of(n);
        Some(h)
    });
    let mut out: Vec<Hole> = found.into_iter().flatten().collect();
    out.sort_by(|a, b| b.void_exposed.cmp(&a.void_exposed).then(b.unbacked.cmp(&a.unbacked)).then(b.area.partial_cmp(&a.area).unwrap_or(std::cmp::Ordering::Equal)));
    out
}

/// A cut in the hero's skin, probed from where a player can stand. At each sampled edge of the
/// loop: the point EDGE_STEP past the edge in the plane of the kept face (where the skin was), and a
/// cone of viewers around that face's outward normal. A view counts when nothing drawn blocks it
/// now (`exposed`, voids there are `void_exposed`: visible at bind), or when only garments whose rim
/// comes within RIM_MARGIN of the edge block it (`probes`, voids `unbacked`: what shows once they
/// lift). Edge within RIM_MARGIN of a rim where most such views see a void is risk length.
#[allow(clippy::too_many_arguments)]
fn cut_edge_probe(
    c: &Census,
    bvh: &Bvh,
    lp: &[u32],
    h: &mut Hole,
    edge_face: &HashMap<(u32, u32), u32>,
    rim_near: &dyn Fn(V3) -> (f64, Vec<u32>),
    two_sided: &[bool],
    visible: &[bool],
    hits: &mut Vec<Hit>,
) {
    let pos = &c.topo.pos;
    let shell_of = &c.topo.shell_of;
    let k = lp.len();
    let step = k.div_ceil(EDGE_SAMPLES).max(1);
    let mut worst = 0.0f64;
    let mut best_now = 0usize;
    for i in (0..k).step_by(step) {
        let (va, vb) = (lp[i], lp[(i + 1) % k]);
        let (a, b) = (pos[va as usize], pos[vb as usize]);
        // the edge this sample stands for: the loop's own edges up to the next sample
        let seg: f64 = (i..(i + step).min(k)).map(|j| v3::dist(pos[lp[j] as usize], pos[lp[(j + 1) % k] as usize])).sum();
        h.edge_length += seg;
        let Some(&f) = edge_face.get(&(va.min(vb), va.max(vb))) else { continue };
        let ft = bvh.tris[f as usize];
        let s = v3::norm(v3::tri_normal(ft[0], ft[1], ft[2]));
        if v3::len(s) == 0.0 {
            continue;
        }
        let third = *ft.iter().max_by(|x, y| (v3::dist(**x, a) + v3::dist(**x, b)).partial_cmp(&(v3::dist(**y, a) + v3::dist(**y, b))).unwrap_or(std::cmp::Ordering::Equal)).unwrap();
        let mut m = v3::norm(v3::cross(v3::sub(b, a), s));
        if v3::dot(m, v3::sub(third, a)) > 0.0 {
            m = v3::scale(m, -1.0);
        }
        let mid = v3::scale(v3::add(a, b), 0.5);
        let p = v3::add(mid, v3::scale(m, EDGE_STEP));
        let (rim_d, lifting) = rim_near(mid);
        let near_rim = rim_d <= RIM_MARGIN;
        let (mut reach, mut voids, mut now_voids) = (0usize, 0usize, 0usize);
        for d in cone(s) {
            bvh.ray_all(p, d, 1e-3, 1e4, hits);
            let mut now_blocked = false;
            let mut lift_blocked = false;
            for x in hits.iter().filter(|x| x.facing < 0.0 || two_sided[x.tri as usize]) {
                now_blocked = true;
                if !lifting.contains(&shell_of[x.tri as usize]) {
                    lift_blocked = true;
                    break;
                }
            }
            if lift_blocked {
                continue;
            }
            let (void, through) = sees_void(bvh, p, d, two_sided, visible, hits);
            h.probes += 1;
            if through {
                h.see_through += 1;
            }
            if void {
                h.unbacked += 1;
            }
            if !now_blocked {
                h.exposed += 1;
                if void {
                    h.void_exposed += 1;
                    now_voids += 1;
                }
            }
            if near_rim {
                reach += 1;
                voids += void as usize;
            }
        }
        if now_voids > best_now {
            best_now = now_voids;
            h.void_dir = Some(s);
            h.now_at = mid;
        }
        if near_rim && reach > 0 && voids * 2 > reach {
            h.risk_length += seg;
            h.risk_points.push(mid);
            let score = voids as f64 / reach as f64;
            if score >= worst {
                worst = score;
                h.risk_at = mid;
            }
        }
    }
}
