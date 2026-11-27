//! Orthographic views of two models in shared frames (scene3d's ortho/contact read views, without
//! Blender): the fair-play silhouette numbers, LOD silhouette drift, the PNG x-ray sheet, and the fair-play view
//! (the model shaded, and what the fair-play rules test, from one camera).

use super::census::{Census, Geo, HitShape};
use geom::image::{Image, Rgba};
use geom::raster::{self, Frame, Raster, View, NAMED};
use geom::v3::{self, Aabb, V3};
use std::collections::HashMap;

/// World units per pixel for the fair-play and LOD views (~6 mm).
pub const VIEW_UNIT: f64 = 0.25;

#[derive(Debug, Clone)]
pub struct ViewCompare {
    pub view: &'static str,
    pub ours: usize,
    pub valve: usize,
    pub iou: f64,
    /// Pixels only we cover (we add) and only Valve covers (we lose).
    pub added: usize,
    pub lost: usize,
}

impl ViewCompare {
    pub fn ratio(&self) -> f64 {
        if self.valve == 0 { f64::INFINITY } else { self.ours as f64 / self.valve as f64 }
    }
}

pub fn union_bounds(a: &Census, b: Option<&Census>) -> Aabb {
    let mut bb = a.bounds;
    if let Some(b) = b {
        bb.union(&b.bounds);
    }
    bb
}

fn two_sided(c: &Census, g: &Geo) -> Vec<bool> {
    g.tsurf.iter().map(|&s| c.materials.get(&g.surfs[s as usize].material).is_some_and(|m| m.two_sided)).collect()
}

/// Rasterize one LOD of a model in a frame.
pub fn render(c: &Census, g: &Geo, f: Frame) -> Raster {
    let ts = two_sided(c, g);
    raster::rasterize(f, &g.tri_soup(), &|t| ts[t as usize])
}

pub fn drawn_mask(r: &Raster) -> Vec<bool> {
    r.drawn.iter().map(|&t| t != raster::NONE).collect()
}

/// Our in-game silhouette against Valve's in the five named views, same frames.
pub fn silhouettes(ours: &Census, valve: &Census) -> Vec<ViewCompare> {
    let bb = union_bounds(ours, Some(valve));
    crate::par::map(&NAMED, |v| {
            let f = Frame::fit(*v, &bb, VIEW_UNIT, 4);
            let a = drawn_mask(&render(ours, &ours.geo, f));
            let b = drawn_mask(&render(valve, &valve.geo, f));
            let (iou, _) = raster::compare_masks(&a, &b);
            let added = a.iter().zip(&b).filter(|(x, y)| **x && !**y).count();
            let lost = a.iter().zip(&b).filter(|(x, y)| !**x && **y).count();
            ViewCompare { view: v.name, ours: a.iter().filter(|x| **x).count(), valve: b.iter().filter(|x| **x).count(), iou, added, lost }
    })
}

/// For each LOD after the first: the worst intersection-over-union of its silhouette with LOD0's
/// over the named views.
pub fn lod_drift(c: &Census) -> Vec<(u32, f64, &'static str)> {
    // LOD0's mask per view is the same for every LOD: render it once; each (LOD, view) render is independent.
    let base: Vec<Vec<bool>> = crate::par::map(&NAMED, |v| drawn_mask(&render(c, &c.geo, Frame::fit(*v, &c.bounds, VIEW_UNIT, 4))));
    let lods: Vec<(u32, Geo)> = (1..c.model.lod_count()).map(|lod| (lod, Geo::of(&c.model, lod).0)).collect();
    let jobs: Vec<(usize, usize)> = (0..lods.len()).flat_map(|l| (0..NAMED.len()).map(move |v| (l, v))).collect();
    let ious = crate::par::map(&jobs, |&(l, v)| {
        let f = Frame::fit(NAMED[v], &c.bounds, VIEW_UNIT, 4);
        let b = drawn_mask(&render(c, &lods[l].1, f));
        raster::compare_masks(&b, &base[v]).0
    });
    let mut out = Vec::new();
    for (l, (lod, _)) in lods.iter().enumerate() {
        let mut worst = (1.0f64, "");
        for v in 0..NAMED.len() {
            let iou = ious[l * NAMED.len() + v];
            if iou < worst.0 {
                worst = (iou, NAMED[v].name);
            }
        }
        out.push((*lod, worst.0, worst.1));
    }
    out
}

const BG: Rgba = [18, 16, 14, 255];
const PANEL: Rgba = [30, 27, 24, 255];
const TEXT: Rgba = [226, 216, 200, 255];
const DIM: Rgba = [138, 128, 116, 255];
const BOTH: Rgba = [150, 142, 132, 255];
const ADDS: Rgba = [232, 140, 48, 255];
const LOSES: Rgba = [64, 196, 220, 255];
const BACKFACE: Rgba = [220, 48, 48, 255];
const MARK: Rgba = [255, 224, 64, 255];

/// The x-ray sheet: per named view, (1) silhouette diff, ours over Valve's (grey both, orange we
/// add, cyan we lose: the fair-play picture); (2) facing, what the game culls in red; (3) our
/// findings' locations marked. One shared scale, so every panel is to scale with every other.
pub fn xray_sheet(ours: &Census, valve: Option<&Census>, title: &str, marks: &[(V3, String)]) -> Image {
    let bb = union_bounds(ours, valve);
    let views: Vec<Frame> = NAMED.iter().map(|v| Frame::fit(*v, &bb, VIEW_UNIT * 1.5, 6)).collect();
    let cell_w = views.iter().map(|f| f.w).max().unwrap_or(1);
    let cell_h = views.iter().map(|f| f.h).max().unwrap_or(1);
    let (pad, label) = (12usize, 22usize);
    let rows = 2;
    let w = pad + views.len() * (cell_w + pad);
    let h = 40 + rows * (cell_h + label + pad) + 18 * 3;
    let mut im = Image::new(w, h, BG);
    im.text(pad as i64, 10, title, 2, TEXT);
    for (vi, f) in views.iter().enumerate() {
        let ox = (pad + vi * (cell_w + pad)) as i64;
        let oy = 40i64;
        let a = render(ours, &ours.geo, *f);
        let am = drawn_mask(&a);
        let bm = valve.map(|v| drawn_mask(&render(v, &v.geo, *f)));
        // row 1: silhouette diff
        im.rect(ox, oy, cell_w as i64, (cell_h + label) as i64, PANEL);
        let (mut ours_px, mut valve_px) = (0usize, 0usize);
        for y in 0..f.h {
            for x in 0..f.w {
                let i = y * f.w + x;
                let o = am[i];
                let v = bm.as_ref().is_some_and(|b| b[i]);
                ours_px += o as usize;
                valve_px += v as usize;
                let c = match (o, v, bm.is_some()) {
                    (true, true, _) => Some(BOTH),
                    (true, false, true) => Some(ADDS),
                    (true, false, false) => Some(BOTH),
                    (false, true, _) => Some(LOSES),
                    _ => None,
                };
                if let Some(c) = c {
                    im.set(ox + x as i64, oy + label as i64 + y as i64, c);
                }
            }
        }
        let caption = if valve_px > 0 { format!("{} {:.1}%", NAMED[vi].name, 100.0 * ours_px as f64 / valve_px as f64) } else { NAMED[vi].name.to_string() };
        im.text(ox + 4, oy + 4, &caption, 1, TEXT);
        // row 2: facing (back faces seen first in red) and finding marks
        let oy2 = oy + (cell_h + label + pad) as i64;
        im.rect(ox, oy2, cell_w as i64, (cell_h + label) as i64, PANEL);
        for y in 0..f.h {
            for x in 0..f.w {
                let i = y * f.w + x;
                if a.any[i] == raster::NONE {
                    continue;
                }
                let c = if a.any_front[i] {
                    let shade = (120.0 + 100.0 * (1.0 - ((a.any_depth[i] as f64 - (-60.0)) / 120.0).clamp(0.0, 1.0))) as u8;
                    [shade, shade, shade, 255]
                } else {
                    BACKFACE
                };
                im.set(ox + x as i64, oy2 + label as i64 + y as i64, c);
            }
        }
        for (k, (p, _)) in marks.iter().enumerate() {
            let (px, py, _) = f.project(*p);
            let (cx, cy) = (ox + px as i64, oy2 + label as i64 + py as i64);
            im.outline(cx - 5, cy - 5, 11, 11, MARK);
            im.text(cx + 7, cy - 4, &format!("{}", k + 1), 1, MARK);
        }
        im.text(ox + 4, oy2 + 4, &format!("{} facing", NAMED[vi].name), 1, TEXT);
    }
    let ly = (40 + rows * (cell_h + label + pad)) as i64;
    im.text(pad as i64, ly, "grey: both  orange: ours adds  cyan: ours loses (fair play)  red: back face seen first (culled: see-through)", 1, DIM);
    for (k, (_, m)) in marks.iter().enumerate().take(2) {
        im.text(pad as i64, ly + 14 + 14 * k as i64, &format!("{}: {}", k + 1, m), 1, MARK);
    }
    im
}

/// ASCII silhouettes (scene3d's ortho-ascii) of ours and Valve's front view, side by side.
pub fn ascii_front(ours: &Census, valve: Option<&Census>, cols: usize) -> String {
    let bb = union_bounds(ours, valve);
    let f = Frame::fit(NAMED[0], &bb, VIEW_UNIT, 2);
    let a = raster::ascii(&drawn_mask(&render(ours, &ours.geo, f)), f.w, f.h, cols);
    let Some(v) = valve else { return a };
    let b = raster::ascii(&drawn_mask(&render(v, &v.geo, f)), f.w, f.h, cols);
    let mut s = format!("{:<w$}   {}\n", "ours (front)", "Valve's (front)", w = cols);
    for (x, y) in a.lines().zip(b.lines()) {
        s.push_str(&format!("{x}   {y}\n"));
    }
    s
}

/// The fair-play test drawn from one camera, as two images of the same size and frame, so either can be laid over
/// the other: `shaded`, our model lit with its own colour maps; `test`, what the fair-play rules measure from there:
/// the hitboxes, our silhouette's outline, Valve's outline faint behind it, and our surface past the hitbox lines
/// marked.
pub struct FairPlayView {
    pub shaded: Image,
    pub test: Image,
    /// World units per output pixel.
    pub unit: f64,
    pub hitboxes: usize,
    /// Output pixels of our drawn surface lying more than each of the lines beyond every hitbox, in `lines` order.
    pub beyond: Vec<usize>,
    /// Colour maps that could not be decoded: those pieces are shaded grey.
    pub undecoded: Vec<String>,
}

/// Supersampling per side: the views are drawn at this many times the size and box-filtered down.
const SS: usize = 2;
const FP_BG: Rgba = [22, 21, 19, 255];
const FP_BODY: Rgba = [38, 36, 34, 255];
const FP_INK: Rgba = [245, 245, 245, 255];
const FP_VALVE: Rgba = [245, 245, 245, 150];
const FP_CAPS_FILL: Rgba = [190, 200, 215, 22];
const FP_CAPS_EDGE: Rgba = [190, 200, 215, 150];
/// The marks past each line: pale amber past the first, amber past the second, red past the third.
const FP_PAST: [Rgba; 3] = [[240, 184, 90, 120], [240, 184, 90, 255], [255, 107, 107, 255]];

/// A named view of `raster::probe_views` (front, side, back, top, below, left and the obliques).
pub fn view_named(name: &str) -> Option<View> {
    raster::probe_views().into_iter().find(|v| v.name == name)
}

/// One decoded colour map, every mip (mip 0 first).
struct Tex(Vec<(usize, usize, Vec<u8>)>);

impl Tex {
    /// Bilinear sample at mip `mip` (wrapping), RGBA 0..1 (colour still in sRGB).
    fn sample(&self, u: f64, v: f64, mip: usize) -> [f64; 4] {
        let (w, h, px) = &self.0[mip.min(self.0.len() - 1)];
        let (w, h) = (*w, *h);
        let x = (u - u.floor()) * w as f64 - 0.5;
        let y = (v - v.floor()) * h as f64 - 0.5;
        let (x0, y0) = (x.floor(), y.floor());
        let (fx, fy) = (x - x0, y - y0);
        let at = |xi: f64, yi: f64, c: usize| {
            let xi = (xi as i64).rem_euclid(w as i64) as usize;
            let yi = (yi as i64).rem_euclid(h as i64) as usize;
            px[4 * (yi * w + xi) + c] as f64 / 255.0
        };
        let mut out = [0.0; 4];
        for (c, o) in out.iter_mut().enumerate() {
            let top = at(x0, y0, c) * (1.0 - fx) + at(x0 + 1.0, y0, c) * fx;
            let bot = at(x0, y0 + 1.0, c) * (1.0 - fx) + at(x0 + 1.0, y0 + 1.0, c) * fx;
            *o = top * (1.0 - fy) + bot * fy;
        }
        out
    }
}

fn to_lin(c: f64) -> f64 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

fn to_srgb(c: f64) -> u8 {
    let c = c.clamp(0.0, 1.0);
    let s = if c <= 0.003_130_8 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 };
    (s * 255.0).round() as u8
}

/// Whether the screen point (x, y) lies inside the hitbox's outline on screen (orthographic, so a capsule is a
/// stadium of its own radius, a box the hull of its corners).
fn covers(f: &Frame, shape: &HitShape, x: f64, y: f64) -> bool {
    match *shape {
        HitShape::Capsule(a, b, r) => {
            let (ax, ay, _) = f.project(a);
            let (bx, by, _) = f.project(b);
            let (dx, dy) = (bx - ax, by - ay);
            let l = dx * dx + dy * dy;
            let t = if l > 1e-12 { (((x - ax) * dx + (y - ay) * dy) / l).clamp(0.0, 1.0) } else { 0.0 };
            let (px, py) = (ax + t * dx - x, ay + t * dy - y);
            (px * px + py * py).sqrt() <= r / f.unit
        }
        HitShape::Box { rot, org, lo, hi } => {
            let mut pts: Vec<(f64, f64)> = (0..8)
                .map(|i| {
                    let q = [if i & 1 == 0 { lo[0] } else { hi[0] }, if i & 2 == 0 { lo[1] } else { hi[1] }, if i & 4 == 0 { lo[2] } else { hi[2] }];
                    let w = [0, 1, 2].map(|k| rot[0][k] * q[0] + rot[1][k] * q[1] + rot[2][k] * q[2] + org[k]);
                    let (sx, sy, _) = f.project(w);
                    (sx, sy)
                })
                .collect();
            pts.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let cross = |o: (f64, f64), a: (f64, f64), b: (f64, f64)| (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0);
            // monotone chain: the lower hull, then the upper
            let mut hull: Vec<(f64, f64)> = Vec::new();
            for pass in 0..2 {
                let start = hull.len();
                let order: Vec<(f64, f64)> = if pass == 0 { pts.clone() } else { pts.iter().rev().copied().collect() };
                for p in order {
                    while hull.len() >= start + 2 && cross(hull[hull.len() - 2], hull[hull.len() - 1], p) <= 0.0 {
                        hull.pop();
                    }
                    hull.push(p);
                }
                hull.pop();
            }
            hull.len() >= 3 && (0..hull.len()).all(|i| cross(hull[i], hull[(i + 1) % hull.len()], (x, y)) >= 0.0)
        }
    }
}

/// Pixels of `mask` within `r` pixels of a pixel outside it: the mask's outline, drawn inward.
fn edge(mask: &[bool], w: usize, h: usize, r: i64) -> Vec<bool> {
    let mut out = vec![false; mask.len()];
    for y in 0..h as i64 {
        for x in 0..w as i64 {
            if !mask[y as usize * w + x as usize] {
                continue;
            }
            'near: for dy in -r..=r {
                for dx in -r..=r {
                    let (nx, ny) = (x + dx, y + dy);
                    if nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64 || !mask[ny as usize * w + nx as usize] {
                        out[y as usize * w + x as usize] = true;
                        break 'near;
                    }
                }
            }
        }
    }
    out
}

/// What the game draws of a model in a frame, its additive cards (glints, glows) left out as the hitbox measure
/// leaves them out: light, not a shape.
fn solid_mask(c: &Census, f: Frame) -> Vec<bool> {
    let g = &c.geo;
    let ts = two_sided(c, g);
    let additive = |t: usize| c.materials.get(&g.surfs[g.tsurf[t] as usize].material).is_some_and(|m| m.additive);
    let soup: Vec<[V3; 3]> = (0..g.tris.len()).map(|t| if additive(t) { [[0.0; 3]; 3] } else { g.tri(t as u32) }).collect();
    drawn_mask(&raster::rasterize(f, &soup, &|t| ts[t as usize]))
}

/// Box-filter an image down by `SS` per side.
fn shrink(im: &Image) -> Image {
    let (w, h) = (im.w / SS, im.h / SS);
    let mut out = Image::new(w, h, [0, 0, 0, 255]);
    for y in 0..h {
        for x in 0..w {
            let mut acc = [0u32; 4];
            for dy in 0..SS {
                for dx in 0..SS {
                    let c = im.get(x * SS + dx, y * SS + dy);
                    for k in 0..4 {
                        acc[k] += c[k] as u32;
                    }
                }
            }
            let n = (SS * SS) as u32;
            out.set(x as i64, y as i64, [0, 1, 2, 3].map(|k| ((acc[k] + n / 2) / n) as u8));
        }
    }
    out
}

/// The fair-play view of our model in `view` at `unit` world units per output pixel, with Valve's outline behind
/// it when given. `lines`: distances beyond every hitbox to mark (the policy's warn, taste and egregious lines).
/// `file` reads a compiled file's bytes, for the colour maps. Deterministic: the same model draws the same pixels.
pub fn fairplay_view(ours: &Census, valve: Option<&Census>, view: View, unit: f64, lines: &[f64], file: &(dyn Fn(&str) -> Option<Vec<u8>> + Sync)) -> FairPlayView {
    let bb = union_bounds(ours, valve);
    let mut f = Frame::fit(view, &bb, unit / SS as f64, 12 * SS);
    f.w = f.w.div_ceil(SS) * SS;
    f.h = f.h.div_ceil(SS) * SS;
    let (w, h) = (f.w, f.h);
    let g = &ours.geo;
    let mat = |t: usize| ours.materials.get(&g.surfs[g.tsurf[t] as usize].material);
    let uv = |i: u32| g.uv.get(i as usize).map_or([0.0; 2], |p| [p[0] as f64, p[1] as f64]);

    // the colour maps, every mip, decoded once
    let mut wanted: Vec<String> = Vec::new();
    for s in &g.surfs {
        if let Some(c) = ours.materials.get(&s.material).and_then(|m| m.color.clone()) {
            if !c.starts_with("materials/default/") && !wanted.contains(&c) {
                wanted.push(c);
            }
        }
    }
    let decoded: Vec<Option<Tex>> = crate::par::map(&wanted, |c| {
        let d = file(c)?;
        let n = resource::texture::info(&d)?.mips.max(1) as u32;
        let mips: Vec<(usize, usize, Vec<u8>)> = (0..n).map_while(|m| resource::pixels::decode(&d, m)).collect();
        (!mips.is_empty()).then_some(Tex(mips))
    });
    let undecoded: Vec<String> = wanted.iter().zip(&decoded).filter(|(_, d)| d.is_none()).map(|(c, _)| c.clone()).collect();
    let tex: HashMap<String, Tex> = wanted.into_iter().zip(decoded).filter_map(|(c, d)| d.map(|d| (c, d))).collect();
    let color_of = |t: usize| mat(t).and_then(|m| m.color.as_ref()).and_then(|c| tex.get(c));

    // our drawn surface: per pixel the nearest triangle the game draws, its barycentrics and depth, with the alpha
    // test applied (a cut-out texel lets the surface behind it through)
    let n = w * h;
    let mut tri = vec![raster::NONE; n];
    let mut bary = vec![[0f64; 3]; n];
    let mut depth = vec![f64::INFINITY; n];
    let mut back = vec![false; n];
    let mut mips = vec![0usize; g.tris.len()];
    for (t, idx) in g.tris.iter().enumerate() {
        let Some(m) = mat(t) else { continue };
        if m.additive {
            continue;
        }
        let p: Vec<(f64, f64, f64)> = idx.iter().map(|&i| f.project(g.pos[i as usize])).collect();
        let area = (p[1].0 - p[0].0) * (p[2].1 - p[0].1) - (p[2].0 - p[0].0) * (p[1].1 - p[0].1);
        if area == 0.0 || !area.is_finite() {
            continue;
        }
        let front = area < 0.0;
        if !front && !m.two_sided {
            continue;
        }
        let ct = color_of(t);
        // the mip a renderer would pick: texels per screen pixel over this triangle
        if let Some(c) = ct {
            let (a, b, cc) = (uv(idx[0]), uv(idx[1]), uv(idx[2]));
            let ta = ((b[0] - a[0]) * (cc[1] - a[1]) - (cc[0] - a[0]) * (b[1] - a[1])).abs() * c.0[0].0 as f64 * c.0[0].1 as f64;
            mips[t] = (0.5 * (ta / area.abs()).max(1.0).log2()).floor() as usize;
        }
        let cut = (m.alpha_test || m.translucent) && ct.is_some();
        let x0 = p.iter().map(|q| q.0).fold(f64::INFINITY, f64::min).floor().max(0.0) as i64;
        let x1 = p.iter().map(|q| q.0).fold(f64::NEG_INFINITY, f64::max).ceil().min(w as f64 - 1.0) as i64;
        let y0 = p.iter().map(|q| q.1).fold(f64::INFINITY, f64::min).floor().max(0.0) as i64;
        let y1 = p.iter().map(|q| q.1).fold(f64::NEG_INFINITY, f64::max).ceil().min(h as f64 - 1.0) as i64;
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
                let z = w0 * p[0].2 + w1 * p[1].2 + w2 * p[2].2;
                let i = y as usize * w + x as usize;
                if z >= depth[i] {
                    continue;
                }
                if let (true, Some(tx)) = (cut, ct) {
                    let (a, b, c) = (uv(idx[0]), uv(idx[1]), uv(idx[2]));
                    if tx.sample(w0 * a[0] + w1 * b[0] + w2 * c[0], w0 * a[1] + w1 * b[1] + w2 * c[1], mips[t])[3] < m.alpha_ref.max(0.02) {
                        continue;
                    }
                }
                depth[i] = z;
                tri[i] = t as u32;
                bary[i] = [w0, w1, w2];
                back[i] = !front;
            }
        }
    }

    // shaded: the colour map under a key light from above the camera's left shoulder, a soft fill and a rim
    let (rt, up, dir) = (view.right(), view.up, view.dir);
    let key = v3::norm(v3::add(v3::add(v3::scale(dir, -0.75), v3::scale(up, 0.55)), v3::scale(rt, -0.4)));
    let mut shaded = Image::new(w, h, FP_BG);
    let mut lit: Vec<Option<[f64; 3]>> = vec![None; n];
    for i in 0..n {
        if tri[i] == raster::NONE {
            continue;
        }
        let t = tri[i] as usize;
        let idx = g.tris[t];
        let b = bary[i];
        let nrm = |k: usize| g.nrm.get(idx[k] as usize).map_or([0.0; 3], |n| [n[0] as f64, n[1] as f64, n[2] as f64]);
        let sum = v3::add(v3::add(v3::scale(nrm(0), b[0]), v3::scale(nrm(1), b[1])), v3::scale(nrm(2), b[2]));
        let mut nv = if v3::len(sum) > 1e-6 {
            v3::norm(sum)
        } else {
            let [a, bb, c] = g.tri(t as u32);
            v3::norm(v3::tri_normal(a, bb, c))
        };
        if back[i] {
            nv = v3::scale(nv, -1.0);
        }
        let albedo = match color_of(t) {
            Some(tx) => {
                let (a, bb, c) = (uv(idx[0]), uv(idx[1]), uv(idx[2]));
                let s = tx.sample(b[0] * a[0] + b[1] * bb[0] + b[2] * c[0], b[0] * a[1] + b[1] * bb[1] + b[2] * c[1], mips[t]);
                [to_lin(s[0]), to_lin(s[1]), to_lin(s[2])]
            }
            None => [0.3, 0.3, 0.3],
        };
        let wrap = (v3::dot(nv, key) * 0.5 + 0.5).clamp(0.0, 1.0);
        let diffuse = 0.28 + 0.92 * wrap * wrap;
        let facing = v3::dot(nv, v3::scale(dir, -1.0)).abs();
        let rim = 0.12 * (1.0 - facing).powi(3);
        let glow = if mat(t).is_some_and(|m| m.self_illum) { 0.4 } else { 0.0 };
        lit[i] = Some(albedo.map(|a| a * (diffuse + glow) + rim));
    }
    // additive cards (sparkles, glows) add their colour where nothing nearer hides them
    for (t, idx) in g.tris.iter().enumerate() {
        let Some(m) = mat(t).filter(|m| m.additive) else { continue };
        let Some(tx) = color_of(t) else { continue };
        let p: Vec<(f64, f64, f64)> = idx.iter().map(|&i| f.project(g.pos[i as usize])).collect();
        let area = (p[1].0 - p[0].0) * (p[2].1 - p[0].1) - (p[2].0 - p[0].0) * (p[1].1 - p[0].1);
        if area == 0.0 || !area.is_finite() || (area > 0.0 && !m.two_sided) {
            continue;
        }
        let (a, b, c) = (uv(idx[0]), uv(idx[1]), uv(idx[2]));
        let x0 = p.iter().map(|q| q.0).fold(f64::INFINITY, f64::min).floor().max(0.0) as i64;
        let x1 = p.iter().map(|q| q.0).fold(f64::NEG_INFINITY, f64::max).ceil().min(w as f64 - 1.0) as i64;
        let y0 = p.iter().map(|q| q.1).fold(f64::INFINITY, f64::min).floor().max(0.0) as i64;
        let y1 = p.iter().map(|q| q.1).fold(f64::NEG_INFINITY, f64::max).ceil().min(h as f64 - 1.0) as i64;
        let inv = 1.0 / area;
        for y in y0..=y1 {
            for x in x0..=x1 {
                let (cx, cy) = (x as f64 + 0.5, y as f64 + 0.5);
                let w0 = ((p[1].0 - cx) * (p[2].1 - cy) - (p[2].0 - cx) * (p[1].1 - cy)) * inv;
                let w1 = ((p[2].0 - cx) * (p[0].1 - cy) - (p[0].0 - cx) * (p[2].1 - cy)) * inv;
                let w2 = 1.0 - w0 - w1;
                let i = y as usize * w + x as usize;
                if w0 < -1e-9 || w1 < -1e-9 || w2 < -1e-9 || w0 * p[0].2 + w1 * p[1].2 + w2 * p[2].2 >= depth[i] {
                    continue;
                }
                let s = tx.sample(w0 * a[0] + w1 * b[0] + w2 * c[0], w0 * a[1] + w1 * b[1] + w2 * c[1], 0);
                let base = lit[i].unwrap_or([to_lin(FP_BG[0] as f64 / 255.0), to_lin(FP_BG[1] as f64 / 255.0), to_lin(FP_BG[2] as f64 / 255.0)]);
                lit[i] = Some([0, 1, 2].map(|k| base[k] + to_lin(s[k]) * s[3]));
            }
        }
    }
    for (i, l) in lit.iter().enumerate() {
        if let Some(l) = l {
            let c = l.map(to_srgb);
            shaded.set((i % w) as i64, (i / w) as i64, [c[0], c[1], c[2], 255]);
        }
    }

    // test: our silhouette (the raster the silhouette rule counts), the hitboxes, the surface past the lines, and
    // the outlines
    let ours_mask = solid_mask(ours, f);
    let valve_mask = valve.map(|v| solid_mask(v, f));
    let shapes = ours.hitbox_shapes();
    let mut test = Image::new(w, h, FP_BG);
    let put = |im: &mut Image, i: usize, c: Rgba| im.set((i % w) as i64, (i / w) as i64, c);
    for (i, &m) in ours_mask.iter().enumerate() {
        if m {
            put(&mut test, i, FP_BODY);
        }
    }
    // the hitboxes: their union filled faintly, each one outlined
    let mut any_cap = vec![false; n];
    let mut cap_edges = vec![false; n];
    for (_, s) in &shapes {
        let mask: Vec<bool> = (0..n).map(|i| covers(&f, s, (i % w) as f64 + 0.5, (i / w) as f64 + 0.5)).collect();
        for (e, x) in cap_edges.iter_mut().zip(edge(&mask, w, h, 1)) {
            *e |= x;
        }
        for (a, m) in any_cap.iter_mut().zip(&mask) {
            *a |= *m;
        }
    }
    for (i, &a) in any_cap.iter().enumerate() {
        if a {
            put(&mut test, i, FP_CAPS_FILL);
        }
    }
    // the surface past each line, judged at the point the game draws in each pixel
    let mut beyond = vec![0usize; lines.len()];
    for i in 0..n {
        if tri[i] == raster::NONE {
            continue;
        }
        let sx = ((i % w) as f64 + 0.5 - w as f64 / 2.0) * f.unit;
        let sy = -((i / w) as f64 + 0.5 - h as f64 / 2.0) * f.unit;
        let p = v3::add(f.center, v3::add(v3::add(v3::scale(rt, sx), v3::scale(up, sy)), v3::scale(dir, depth[i])));
        let d = shapes.iter().map(|(_, s)| s.distance(p)).fold(f64::INFINITY, f64::min);
        let mut mark = None;
        for (k, l) in lines.iter().enumerate() {
            if d > *l {
                beyond[k] += 1;
                mark = Some(FP_PAST[k.min(FP_PAST.len() - 1)]);
            }
        }
        if let Some(c) = mark {
            put(&mut test, i, c);
        }
    }
    for (i, &e) in cap_edges.iter().enumerate() {
        if e {
            put(&mut test, i, FP_CAPS_EDGE);
        }
    }
    if let Some(vm) = &valve_mask {
        // dashed, so it reads as the other model's line
        for (i, e) in edge(vm, w, h, 1).into_iter().enumerate() {
            if e && ((i % w) / (3 * SS) + (i / w) / (3 * SS)).is_multiple_of(2) {
                put(&mut test, i, FP_VALVE);
            }
        }
    }
    for (i, e) in edge(&ours_mask, w, h, SS as i64).into_iter().enumerate() {
        if e {
            put(&mut test, i, FP_INK);
        }
    }
    let per = (SS * SS) as f64;
    FairPlayView {
        shaded: shrink(&shaded),
        test: shrink(&test),
        unit: f.unit * SS as f64,
        hitboxes: shapes.len(),
        beyond: beyond.iter().map(|b| (*b as f64 / per).round() as usize).collect(),
        undecoded,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::census::{mat_info, Env};
    use ::mesh::synth::{SynthDraw, SynthMesh, SynthModel};

    const SKIN: &str = "materials/test/skin.vmat";
    const SKIN_COLOR: &str = "materials/test/skin_color.vtex_c";

    /// A box, outward, four vertices a face, each face's UVs over the middle of the map.
    fn cube(min: [f32; 3], max: [f32; 3]) -> SynthDraw {
        let mut d = SynthDraw { material: SKIN.into(), ..Default::default() };
        let c = |i: usize| [if i & 1 == 0 { min[0] } else { max[0] }, if i & 2 == 0 { min[1] } else { max[1] }, if i & 4 == 0 { min[2] } else { max[2] }];
        let faces: [([usize; 4], [f32; 3]); 6] = [
            ([0, 2, 3, 1], [0.0, 0.0, -1.0]),
            ([4, 5, 7, 6], [0.0, 0.0, 1.0]),
            ([0, 1, 5, 4], [0.0, -1.0, 0.0]),
            ([2, 6, 7, 3], [0.0, 1.0, 0.0]),
            ([0, 4, 6, 2], [-1.0, 0.0, 0.0]),
            ([1, 3, 7, 5], [1.0, 0.0, 0.0]),
        ];
        for (q, nr) in faces {
            let b = d.positions.len() as u32;
            for (j, &i) in q.iter().enumerate() {
                d.positions.push(c(i));
                d.normals.push(nr);
                d.uvs.push([if j == 1 || j == 2 { 0.9 } else { 0.1 }, if j >= 2 { 0.9 } else { 0.1 }]);
                d.joints.push([0, 0, 0, 0]);
                d.weights.push([1.0, 0.0, 0.0, 0.0]);
            }
            d.tris.push([b, b + 1, b + 2]);
            d.tris.push([b, b + 2, b + 3]);
        }
        d
    }

    /// A body box on one capsule (z 10 to 50, radius 14), and whatever else is given.
    fn model(extra: Vec<SynthDraw>) -> Vec<u8> {
        let mut draws = vec![cube([-10.0, -10.0, 0.0], [10.0, 10.0, 60.0])];
        draws.extend(extra);
        ::mesh::synth::model(&SynthModel {
            name: "models/test/hero.vmdl".into(),
            meshes: vec![SynthMesh { name: "body".into(), lod_mask: 1, draws, bones: vec![("root".into(), [0.0; 3])], no_uv: false }],
            skeleton: vec![("root".into(), -1, [0.0; 3])],
            hitboxes: vec![("chest".into(), "root".into(), [0.0, 0.0, 10.0], [0.0, 0.0, 50.0], 14.0)],
            ..Default::default()
        })
    }

    /// One flat colour as a 4 x 4 BGRA8888 map, one mip.
    fn color_map(c: [u8; 4]) -> Vec<u8> {
        let mut f = resource::synth_with(&[], &[(*b"DATA", resource::texture::synth_header(4, 4, 28))]);
        for _ in 0..16 {
            f.extend_from_slice(&[c[2], c[1], c[0], c[3]]);
        }
        f
    }

    fn files(p: &str) -> Option<Vec<u8>> {
        match p {
            SKIN_COLOR => Some(color_map([200, 40, 40, 255])),
            "materials/test/skin.vmat_c" => {
                let body = format!("{{ m_shaderName = \"pbr.vfx\" m_intParams = [ ] m_textureParams = [ {{ m_name = \"g_tColor\" m_pValue = \"{}\" }} ] m_vectorParams = [ ] }}", SKIN_COLOR.trim_end_matches("_c"));
                Some(resource::synth_with(&[], &[(*b"DATA", resource::kv3::encode(&resource::kv3::parse_text(&body).unwrap()))]))
            }
            _ => None,
        }
    }

    fn census(data: &[u8]) -> Census {
        let mat = |p: &str| files(&crate::deep::compiled(p)).and_then(|d| mat_info(&d));
        let tex = |p: &str| files(p).and_then(|d| crate::deep::tex_info(&d));
        Census::build(data, &Env { material: &mat, texture: &tex, file: &files }).unwrap()
    }

    #[test]
    fn the_two_images_share_one_frame_and_mark_what_reaches_past_the_hitboxes() {
        let lines = [5.0, 10.0, 18.0];
        let plain = census(&model(vec![]));
        let v = fairplay_view(&plain, Some(&plain), NAMED[0], 0.5, &lines, &files);
        assert_eq!((v.shaded.w, v.shaded.h), (v.test.w, v.test.h));
        assert_eq!(v.hitboxes, 1);
        // the body box's corners lie a little past the capsule's sides (14.1 against 14), never 5 units
        assert_eq!(v.beyond, vec![0, 0, 0]);
        // a plank reaching 40 units out to the side: past every line, less past each further one
        let wide = census(&model(vec![cube([-2.0, 10.0, 28.0], [2.0, 40.0, 32.0])]));
        let w = fairplay_view(&wide, Some(&plain), NAMED[0], 0.5, &lines, &files);
        assert!(w.beyond[0] > w.beyond[1] && w.beyond[1] > w.beyond[2] && w.beyond[2] > 0, "{:?}", w.beyond);
        // the same model draws the same pixels
        let again = fairplay_view(&wide, Some(&plain), NAMED[0], 0.5, &lines, &files);
        assert_eq!(w.shaded.png(), again.shaded.png());
        assert_eq!(w.test.png(), again.test.png());
    }

    #[test]
    fn the_shaded_view_is_coloured_by_the_models_own_map() {
        let c = census(&model(vec![]));
        let v = fairplay_view(&c, None, NAMED[0], 0.5, &[10.0], &files);
        assert!(v.undecoded.is_empty(), "{:?}", v.undecoded);
        let mid = v.shaded.get(v.shaded.w / 2, v.shaded.h / 2);
        // a red map: red well over green and blue, lit, not the background
        assert!(mid[0] > 120 && mid[0] as u32 > 2 * mid[1] as u32 && mid[1] == mid[2], "{mid:?}");
        assert_eq!(v.shaded.get(0, 0), FP_BG);
        assert_eq!(v.test.get(0, 0), FP_BG);
        // the test image draws the capsule over the body: its middle is the faint hitbox fill, not the body's grey
        assert_ne!(v.test.get(v.test.w / 2, v.test.h / 2), FP_BODY);
    }

    #[test]
    fn a_view_is_found_by_name() {
        assert!(view_named("front").is_some() && view_named("front-right-high").is_some());
        assert!(view_named("sideways").is_none());
    }
}
