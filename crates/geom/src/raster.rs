//! Orthographic views rendered without Blender (scene3d `read/ortho.ts`, `ortho-ascii.ts`): one
//! shared scale for every view, so a unit is the same number of pixels everywhere and ours and
//! Valve's land on the same pixels; a software rasterizer that keeps, per pixel, the nearest
//! surface of any facing and the nearest surface the game would draw (front faces, plus back
//! faces of double-sided materials). From those two buffers come the silhouette the game shows,
//! the back faces it culls (what you see through: holes), and which triangles are visible at all.
//!
//! Model axes are Source 2's: +X forward (heroes face +X), +Y left, +Z up.

use crate::v3::{self, Aabb, V3};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct View {
    pub name: &'static str,
    /// Direction the camera looks along.
    pub dir: V3,
    /// Screen up.
    pub up: V3,
}

impl View {
    pub fn right(&self) -> V3 {
        v3::norm(v3::cross(self.dir, self.up))
    }
}

/// The five named views of the fair-play rule: front, side (the hero's right), back, top, below.
pub const NAMED: [View; 5] = [
    View { name: "front", dir: [-1.0, 0.0, 0.0], up: [0.0, 0.0, 1.0] },
    View { name: "side", dir: [0.0, 1.0, 0.0], up: [0.0, 0.0, 1.0] },
    View { name: "back", dir: [1.0, 0.0, 0.0], up: [0.0, 0.0, 1.0] },
    View { name: "top", dir: [0.0, 0.0, -1.0], up: [1.0, 0.0, 0.0] },
    View { name: "below", dir: [0.0, 0.0, 1.0], up: [1.0, 0.0, 0.0] },
];

/// The named views plus the other side and sixteen obliques (eight around at 35 degrees above,
/// eight at 35 below): the probe set for holes and visibility.
pub fn probe_views() -> Vec<View> {
    let mut v: Vec<View> = NAMED.to_vec();
    v.push(View { name: "left", dir: [0.0, -1.0, 0.0], up: [0.0, 0.0, 1.0] });
    const NAMES_HI: [&str; 8] = ["front-left-high", "left-high", "back-left-high", "back-high", "back-right-high", "right-high", "front-right-high", "front-high"];
    const NAMES_LO: [&str; 8] = ["front-left-low", "left-low", "back-left-low", "back-low", "back-right-low", "right-low", "front-right-low", "front-low"];
    for (k, (hi, lo)) in NAMES_HI.iter().zip(NAMES_LO.iter()).enumerate() {
        let az = (45.0 + 45.0 * k as f64).to_radians();
        for (name, el) in [(*hi, 35f64.to_radians()), (*lo, -35f64.to_radians())] {
            // camera position on the sphere, looking at the centre
            let eye = [az.cos() * el.cos(), az.sin() * el.cos(), el.sin()];
            let dir = v3::scale(eye, -1.0);
            let up = v3::norm(v3::sub([0.0, 0.0, 1.0], v3::scale(dir, v3::dot(dir, [0.0, 0.0, 1.0]))));
            v.push(View { name, dir, up });
        }
    }
    v
}

#[derive(Debug, Clone, Copy)]
pub struct Frame {
    pub view: View,
    /// World point at the centre of the image.
    pub center: V3,
    /// World units per pixel.
    pub unit: f64,
    pub w: usize,
    pub h: usize,
}

impl Frame {
    /// A frame for `view` fitting `bounds`, with `unit` world units per pixel and a margin.
    pub fn fit(view: View, bounds: &Aabb, unit: f64, margin: usize) -> Frame {
        let (r, u) = (view.right(), view.up);
        let corners: Vec<V3> = (0..8).map(|i| [if i & 1 == 0 { bounds.min[0] } else { bounds.max[0] }, if i & 2 == 0 { bounds.min[1] } else { bounds.max[1] }, if i & 4 == 0 { bounds.min[2] } else { bounds.max[2] }]).collect();
        let c = bounds.center();
        let (mut sx, mut sy) = (0.0f64, 0.0f64);
        for p in &corners {
            let d = v3::sub(*p, c);
            sx = sx.max(v3::dot(d, r).abs());
            sy = sy.max(v3::dot(d, u).abs());
        }
        // A frame never exceeds MAX_SIDE pixels a side: a part flung far from the rest (a precision
        // bug, a stray vertex) coarsens the views instead of allocating gigabytes.
        let unit = unit.max(2.0 * sx.max(sy) / MAX_SIDE as f64);
        let w = ((2.0 * sx / unit).ceil() as usize + 2 * margin).max(1);
        let h = ((2.0 * sy / unit).ceil() as usize + 2 * margin).max(1);
        Frame { view, center: c, unit, w, h }
    }
    /// Pixel coordinates (x right, y down) and depth (larger = farther) of a world point.
    pub fn project(&self, p: V3) -> (f64, f64, f64) {
        let d = v3::sub(p, self.center);
        let x = v3::dot(d, self.view.right()) / self.unit + self.w as f64 / 2.0;
        let y = -v3::dot(d, self.view.up) / self.unit + self.h as f64 / 2.0;
        (x, y, v3::dot(d, self.view.dir))
    }
    /// The world-space ray through a pixel centre: (origin far in front, direction).
    pub fn ray(&self, px: usize, py: usize, back: f64) -> (V3, V3) {
        let (r, u) = (self.view.right(), self.view.up);
        let x = (px as f64 + 0.5 - self.w as f64 / 2.0) * self.unit;
        let y = -(py as f64 + 0.5 - self.h as f64 / 2.0) * self.unit;
        let p = v3::add(self.center, v3::add(v3::scale(r, x), v3::scale(u, y)));
        (v3::sub(p, v3::scale(self.view.dir, back)), self.view.dir)
    }
    /// World area of one pixel.
    pub fn pixel_area(&self) -> f64 {
        self.unit * self.unit
    }
}

pub const NONE: u32 = u32::MAX;
/// Largest frame side in pixels.
pub const MAX_SIDE: usize = 2048;

/// Per-pixel nearest surfaces.
#[derive(Debug, Clone)]
pub struct Raster {
    pub frame: Frame,
    /// Nearest triangle of any facing, and its depth.
    pub any: Vec<u32>,
    pub any_depth: Vec<f32>,
    /// Whether that nearest triangle faces the camera.
    pub any_front: Vec<bool>,
    /// Nearest triangle the game draws (front, or a double-sided back), and its depth.
    pub drawn: Vec<u32>,
    pub drawn_depth: Vec<f32>,
}

impl Raster {
    pub fn covered(&self, i: usize) -> bool {
        self.any[i] != NONE
    }
    /// Pixels the game shows something at.
    pub fn silhouette(&self) -> usize {
        self.drawn.iter().filter(|&&t| t != NONE).count()
    }
}

/// Rasterize triangles into a frame. `two_sided(tri)` marks triangles whose material draws back
/// faces. Pixel centres inside or on an edge are covered (a shared edge covers its pixel twice;
/// depth decides).
pub fn rasterize(frame: Frame, tris: &[[V3; 3]], two_sided: &dyn Fn(u32) -> bool) -> Raster {
    let n = frame.w * frame.h;
    let mut r = Raster { frame, any: vec![NONE; n], any_depth: vec![f32::INFINITY; n], any_front: vec![false; n], drawn: vec![NONE; n], drawn_depth: vec![f32::INFINITY; n] };
    for (ti, t) in tris.iter().enumerate() {
        let p: Vec<(f64, f64, f64)> = t.iter().map(|q| frame.project(*q)).collect();
        let area = (p[1].0 - p[0].0) * (p[2].1 - p[0].1) - (p[2].0 - p[0].0) * (p[1].1 - p[0].1);
        if area == 0.0 || !area.is_finite() {
            continue;
        }
        // Screen y points down, so a counter-clockwise (front-facing) triangle has negative area.
        let front = area < 0.0;
        let ds = two_sided(ti as u32);
        let x0 = p.iter().map(|q| q.0).fold(f64::INFINITY, f64::min).floor().max(0.0) as i64;
        let x1 = p.iter().map(|q| q.0).fold(f64::NEG_INFINITY, f64::max).ceil().min(frame.w as f64 - 1.0) as i64;
        let y0 = p.iter().map(|q| q.1).fold(f64::INFINITY, f64::min).floor().max(0.0) as i64;
        let y1 = p.iter().map(|q| q.1).fold(f64::NEG_INFINITY, f64::max).ceil().min(frame.h as f64 - 1.0) as i64;
        if x0 > x1 || y0 > y1 {
            continue;
        }
        let inv = 1.0 / area;
        for y in y0..=y1 {
            let cy = y as f64 + 0.5;
            for x in x0..=x1 {
                let cx = x as f64 + 0.5;
                let w0 = ((p[1].0 - cx) * (p[2].1 - cy) - (p[2].0 - cx) * (p[1].1 - cy)) * inv;
                let w1 = ((p[2].0 - cx) * (p[0].1 - cy) - (p[0].0 - cx) * (p[2].1 - cy)) * inv;
                let w2 = 1.0 - w0 - w1;
                if w0 < -1e-9 || w1 < -1e-9 || w2 < -1e-9 {
                    continue;
                }
                let z = (w0 * p[0].2 + w1 * p[1].2 + w2 * p[2].2) as f32;
                let i = y as usize * frame.w + x as usize;
                if z < r.any_depth[i] {
                    r.any_depth[i] = z;
                    r.any[i] = ti as u32;
                    r.any_front[i] = front;
                }
                if (front || ds) && z < r.drawn_depth[i] {
                    r.drawn_depth[i] = z;
                    r.drawn[i] = ti as u32;
                }
            }
        }
    }
    r
}

/// Intersection-over-union and the area ratio of two equal-size masks.
pub fn compare_masks(a: &[bool], b: &[bool]) -> (f64, f64) {
    let (mut i, mut u, mut na, mut nb) = (0usize, 0usize, 0usize, 0usize);
    for (x, y) in a.iter().zip(b) {
        na += *x as usize;
        nb += *y as usize;
        i += (*x && *y) as usize;
        u += (*x || *y) as usize;
    }
    (if u > 0 { i as f64 / u as f64 } else { 1.0 }, if nb > 0 { na as f64 / nb as f64 } else { f64::INFINITY })
}

/// A coverage mask as ASCII art (scene3d's `ortho-ascii`): one character per `cell` x `cell`
/// block, '#' covered, '.' not, so a text-only reader sees the silhouette.
pub fn ascii(mask: &[bool], w: usize, h: usize, cols: usize) -> String {
    let cell = (w as f64 / cols as f64).max(1.0);
    let rows = ((h as f64 / cell) / 2.0).ceil() as usize; // characters are ~2:1
    let mut s = String::new();
    for ry in 0..rows {
        for cx in 0..cols {
            let (x0, x1) = ((cx as f64 * cell) as usize, (((cx + 1) as f64 * cell) as usize).min(w));
            let (y0, y1) = ((ry as f64 * cell * 2.0) as usize, (((ry + 1) as f64 * cell * 2.0) as usize).min(h));
            let mut c = 0;
            let mut t = 0;
            for y in y0..y1 {
                for x in x0..x1 {
                    t += 1;
                    c += mask[y * w + x] as usize;
                }
            }
            s.push(if t == 0 || c == 0 { '.' } else if c * 2 >= t { '#' } else { '+' });
        }
        s.push('\n');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::topo::tests::cube;

    #[test]
    fn cube_front_view_and_culling() {
        let (p, t) = cube([0.0; 3], 10.0);
        let tris: Vec<[V3; 3]> = t.iter().map(|x| [p[x[0] as usize], p[x[1] as usize], p[x[2] as usize]]).collect();
        let mut bb = Aabb::empty();
        p.iter().for_each(|q| bb.grow(*q));
        let f = Frame::fit(NAMED[0], &bb, 0.5, 2);
        assert_eq!((f.w, f.h), (24, 24));
        let r = rasterize(f, &tris, &|_| false);
        assert_eq!(r.silhouette(), 400, "10x10 units at 0.5 = 20x20 px");
        // every covered pixel's nearest surface faces the camera (closed, outward)
        assert!((0..r.any.len()).all(|i| r.any[i] == NONE || r.any_front[i]));
        // remove the front face (x = 10 plane): we now see the inside back faces, culled
        let open: Vec<[V3; 3]> = tris.iter().filter(|tr| !tr.iter().all(|q| q[0] == 10.0)).cloned().collect();
        let r = rasterize(f, &open, &|_| false);
        assert_eq!(r.silhouette(), 0, "inside of an open box is culled: see-through");
        assert_eq!(r.any.iter().filter(|&&x| x != NONE).count(), 400);
        let r2 = rasterize(f, &open, &|_| true);
        assert_eq!(r2.silhouette(), 400, "double-sided draws the inside");
    }

    #[test]
    fn views_are_unit_and_orthogonal() {
        for v in probe_views() {
            assert!((v3::len(v.dir) - 1.0).abs() < 1e-12 && v3::dot(v.dir, v.up).abs() < 1e-12, "{}", v.name);
        }
        assert_eq!(probe_views().len(), 22);
        let m = [true, true, false, false];
        assert_eq!(compare_masks(&m, &[true, false, false, false]), (0.5, 2.0));
    }
}
