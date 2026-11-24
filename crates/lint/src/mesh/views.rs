//! Orthographic views of two models in shared frames (scene3d's ortho/contact read views, without
//! Blender): the fair-play silhouette numbers, LOD silhouette drift, and the PNG x-ray sheet.

use super::census::{Census, Geo};
use geom::image::{Image, Rgba};
use geom::raster::{self, Frame, Raster, NAMED};
use geom::v3::{Aabb, V3};

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
