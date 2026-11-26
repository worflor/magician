//! The mesh census: everything measured about one compiled model, before any judgement (scene3d's
//! "measure, judge, emit": this is measure). Rules in `rules.rs` read it and Valve's census.

use geom::bvh::Bvh;
use geom::intersect;
use geom::quality;
use geom::raster::{self, Frame, View};
use geom::topo::Topology;
use geom::v3::{self, Aabb, V3};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// Coplanar within this distance (model units, inches) z-fights: SKIN-FLAWS A6, 0.3 mm, the depth
/// precision at gameplay distance for two layers of one model.
pub const PLANE_TOL: f64 = 0.012;
/// Same-facing: unit normals at least this aligned.
pub const COS_TOL: f64 = 0.999;
/// Surfaces this close are in contact (2.5 mm).
pub const CONTACT_TOL: f64 = 0.1;
/// Probe raster resolution: world units per pixel.
pub const PROBE_UNIT: f64 = 0.25;
/// Pair traversal cap (triangle-box pairs); past it the pair rules report unchecked.
pub const PAIR_CAP: usize = 40_000_000;
/// Unwelded vertices this close across shells are a crack on a seam.
pub const SEAM_TOL: f64 = 0.01;

#[derive(Debug, Clone, Default)]
pub struct MatInfo {
    /// The material file was found and read (an unknown material is judged by nothing).
    pub found: bool,
    pub shader: String,
    pub additive: bool,
    pub status_proxy: bool,
    pub npr: bool,
    pub self_illum: bool,
    /// An NPR outline mask is set (a non-zero `TextureNprOutlineMask1`, or a mask texture with no
    /// constant): on an alpha-tested material the outline traces every cut-out.
    pub outline: bool,
    pub alpha_ref: f64,
    pub two_sided: bool,
    pub alpha_test: bool,
    pub translucent: bool,
    pub textured: bool,
    pub color: Option<String>,
    pub textures: Vec<String>,
}

/// Read the flags a material's compiled DATA carries.
pub fn mat_info(vmat: &[u8]) -> Option<MatInfo> {
    use resource::kv3::Value;
    let d = resource::kv3_block(vmat, b"DATA")?.ok()?;
    let mut m = MatInfo { found: true, ..Default::default() };
    for p in d.get("m_intParams").map(Value::as_array).unwrap_or(&[]) {
        let on = p.get("m_nValue").and_then(Value::as_f64).unwrap_or(0.0) != 0.0;
        match p.get("m_name").and_then(Value::as_str).unwrap_or("") {
            "F_RENDER_BACKFACES" => m.two_sided |= on,
            "F_ALPHA_TEST" => m.alpha_test |= on,
            "F_TRANSLUCENT" => m.translucent |= on,
            "F_ADDITIVE_BLEND" => m.additive |= on,
            "F_USE_STATUS_EFFECTS_PROXY" => m.status_proxy |= on,
            "F_USE_NPR_LIGHTING" => m.npr |= on,
            "F_SELF_ILLUM" => m.self_illum |= on,
            _ => {}
        }
    }
    m.shader = d.get("m_shaderName").and_then(Value::as_str).unwrap_or("").to_string();
    m.alpha_ref = 0.5;
    let mut mask_const: Option<f64> = None;
    for p in d.get("m_vectorParams").map(Value::as_array).unwrap_or(&[]) {
        if p.get("m_name").and_then(Value::as_str) == Some("TextureNprOutlineMask1") {
            let v = p.get("m_value").map(Value::numbers).unwrap_or_default();
            mask_const = Some(v.iter().take(3).cloned().fold(0.0, f64::max));
        }
    }
    for p in d.get("m_floatParams").map(Value::as_array).unwrap_or(&[]) {
        if p.get("m_name").and_then(Value::as_str) == Some("g_flAlphaTestReference1") {
            m.alpha_ref = p.get("m_flValue").and_then(Value::as_f64).unwrap_or(0.5);
        }
    }
    let mut mask_tex = false;
    for p in d.get("m_textureParams").map(Value::as_array).unwrap_or(&[]) {
        if p.get("m_name").and_then(Value::as_str) == Some("g_tNprOutlineMask") && p.get("m_pValue").and_then(Value::as_str).is_some_and(|x| !x.is_empty()) {
            mask_tex = true;
        }
    }
    m.outline = match mask_const {
        Some(v) => v > 0.0,
        None => mask_tex,
    };
    for p in d.get("m_textureParams").map(Value::as_array).unwrap_or(&[]) {
        let name = p.get("m_name").and_then(Value::as_str).unwrap_or("");
        let v = p.get("m_pValue").and_then(Value::as_str).unwrap_or("").to_ascii_lowercase();
        if v.is_empty() {
            continue;
        }
        if !v.starts_with("materials/default/") {
            m.textured = true;
            m.textures.push(crate::deep::compiled(&v));
        }
        if name == "g_tColor" {
            m.color = Some(crate::deep::compiled(&v));
        }
    }
    Some(m)
}

/// What the census may read beyond the model: materials (by `.vmat` path as the model names them)
/// and texture headers (compiled path).
pub struct Env<'a> {
    pub material: &'a (dyn Fn(&str) -> Option<MatInfo> + Sync),
    pub texture: &'a (dyn Fn(&str) -> Option<resource::texture::TexInfo> + Sync),
    /// A compiled file's bytes (texture pixels for the albedo and alpha-coverage measures).
    pub file: &'a (dyn Fn(&str) -> Option<Vec<u8>> + Sync),
}

pub fn no_env() -> Env<'static> {
    Env { material: &|_| None, texture: &|_| None, file: &|_| None }
}

/// Linear luminance of an sRGB8 colour.
pub fn luminance(c: [u8; 3]) -> f64 {
    let lin = |v: u8| {
        let x = v as f64 / 255.0;
        if x <= 0.04045 { x / 12.92 } else { ((x + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * lin(c[0]) + 0.7152 * lin(c[1]) + 0.0722 * lin(c[2])
}

/// Albedo samples (SKIN-FLAWS A1): per material, (linear luminance, area) at each LOD0 triangle's
/// UV centroid, nearest texel of the colour map's mip 0. Additive cards are skipped.
#[derive(Debug, Clone, Default)]
pub struct Albedo {
    pub samples: BTreeMap<String, Vec<(f64, f64)>>,
    /// Colour maps that could not be decoded (format or layout), named.
    pub undecoded: Vec<String>,
}

/// Alpha-test coverage across the mips the game samples (SKIN-FLAWS C5), per material.
/// A texture of a material against the mip the game needs for it at range (M147).
#[derive(Debug, Clone, Default)]
pub struct MipReach {
    pub material: String,
    pub texture: String,
    pub width: u32,
    pub height: u32,
    pub mips: u32,
    /// Texels of this texture per metre of the model's surface (area-weighted over the material's triangles).
    pub texels_per_metre: f64,
}

impl MipReach {
    /// Texels of the smallest stored mip that fall on one screen pixel at `metres` (1080 rows, a 60 degree
    /// vertical field of view: the frame `Coverage` uses). Over 1, the sampler steps past texels it never reads.
    pub fn texels_per_pixel(&self, metres: f64) -> f64 {
        let pixels_per_metre = 1080.0 / (2.0 * metres * 30f64.to_radians().tan());
        self.texels_per_metre / pixels_per_metre / 2f64.powi(self.mips.max(1) as i32 - 1)
    }
}

#[derive(Debug, Clone, Default)]
pub struct Coverage {
    pub material: String,
    pub texture: String,
    pub texels_per_metre: f64,
    /// (distance m, mip, share of mixed tiles whose coverage changes by more than 30%)
    pub at: Vec<(f64, u32, f64)>,
    pub tiles: usize,
}

#[derive(Debug, Clone, Default)]
pub struct SurfMeta {
    pub mesh: String,
    pub material: String,
    pub draw: usize,
    pub has_uv: bool,
    pub has_skin: bool,
    pub has_normal: bool,
    pub verts: usize,
    pub tris: usize,
}

/// One LOD flattened: every draw call's vertices and triangles in one numbering.
#[derive(Debug, Clone, Default)]
pub struct Geo {
    pub pos: Vec<V3>,
    pub nrm: Vec<[f32; 3]>,
    pub uv: Vec<[f32; 2]>,
    pub joints: Vec<[u16; 8]>,
    pub weights: Vec<[f32; 8]>,
    pub vsurf: Vec<u32>,
    pub tris: Vec<[u32; 3]>,
    pub tsurf: Vec<u32>,
    pub surfs: Vec<SurfMeta>,
}

impl Geo {
    pub fn of(model: &mesh::Model, lod: u32) -> (Geo, Vec<String>) {
        let (surfaces, notes) = model.surfaces(lod);
        let mut g = Geo::default();
        for (si, s) in surfaces.iter().enumerate() {
            let base = g.pos.len() as u32;
            for (k, p) in s.positions.iter().enumerate() {
                g.pos.push([p[0] as f64, p[1] as f64, p[2] as f64]);
                g.nrm.push(s.normals.get(k).copied().unwrap_or([0.0, 0.0, 0.0]));
                g.uv.push(s.uvs.get(k).copied().unwrap_or([0.0, 0.0]));
                g.joints.push(s.joints.get(k).copied().unwrap_or([u16::MAX; 8]));
                g.weights.push(s.weights.get(k).copied().unwrap_or([0.0; 8]));
                g.vsurf.push(si as u32);
            }
            for t in &s.tris {
                g.tris.push([t[0] + base, t[1] + base, t[2] + base]);
                g.tsurf.push(si as u32);
            }
            g.surfs.push(SurfMeta {
                mesh: s.mesh.clone(),
                material: s.material.clone(),
                draw: s.draw,
                has_uv: s.has_uv,
                has_skin: s.has_skin,
                has_normal: s.has_normal,
                verts: s.positions.len(),
                tris: s.tris.len(),
            });
        }
        (g, notes)
    }
    pub fn tri(&self, t: u32) -> [V3; 3] {
        let i = self.tris[t as usize];
        [self.pos[i[0] as usize], self.pos[i[1] as usize], self.pos[i[2] as usize]]
    }
    pub fn tri_soup(&self) -> Vec<[V3; 3]> {
        (0..self.tris.len() as u32).map(|t| self.tri(t)).collect()
    }
}

#[derive(Debug, Clone, Default)]
pub struct LodStats {
    pub lod: u32,
    pub tris: usize,
    pub verts: usize,
    pub draws: usize,
    pub materials: BTreeSet<String>,
    pub meshes: Vec<String>,
    /// Triangles per material at this LOD.
    pub material_tris: BTreeMap<String, usize>,
    /// The box round every vertex a triangle of this LOD uses.
    pub bounds: Aabb,
}

#[derive(Debug, Clone, Default)]
pub struct ShellFact {
    pub label: String,
    pub mesh: String,
    pub materials: Vec<String>,
    pub bone: String,
    pub tris: usize,
    pub area: f64,
    pub bounds: Aabb,
    pub zero_area: usize,
    pub slivers: usize,
    pub worst_aspect: f64,
    pub flipped_normals: usize,
    pub flipped_visible: usize,
    pub visible_boundary_edges: usize,
    pub visible_nonmanifold_edges: usize,
    pub volume: Option<f64>,
    pub winding: i8,
}

#[derive(Debug, Clone, Default)]
pub struct Region {
    pub shell: usize,
    pub tris: Vec<u32>,
    /// Largest single-view area (units^2) and the views it shows in.
    pub area: f64,
    pub views: Vec<&'static str>,
    pub bone: String,
    pub side: &'static str,
    pub z: (f64, f64),
    pub material: String,
    /// Through the hole: background (true see-through) pixels vs other model surface.
    pub to_background: usize,
    pub to_model: usize,
}

/// A boundary loop (an opening in a shell) and what a viewer outside sees through it.
#[derive(Debug, Clone, Default)]
pub struct Hole {
    pub shell: usize,
    pub perimeter: f64,
    pub area: f64,
    pub centroid: V3,
    pub normal: V3,
    pub bone: String,
    pub side: &'static str,
    /// (sample, direction) probes from which an outside viewer could look at the opening.
    pub probes: usize,
    /// Probes with nothing drawn between the opening and the viewer.
    pub exposed: usize,
    /// Probes whose view through the opening is a void: background (the model is see-through
    /// there), or a surface farther than VOID_DEPTH behind it that no probe view ever sees from
    /// outside (a garment's inside, the far wall of a cut body). Counted with and without the
    /// cover in front: `unbacked` ignores whatever stands between the opening and the viewer, so
    /// it says what shows the moment that cover lifts (a garment in a bend).
    pub void_exposed: usize,
    pub unbacked: usize,
    pub see_through: usize,
    /// The viewer direction with the most void probes.
    pub void_dir: Option<V3>,
    /// Hero-skin material (a cut body) rather than a garment opening.
    pub skin: bool,
    /// The loop outlines its whole shell (a liner card, a strap): a sheet, not an opening.
    pub sheet: bool,
    /// Length of the loop's edge that a garment rim comes within RIM_MARGIN of (the garment can
    /// lift off it) and that nothing backs (rays into the opening there see nothing drawn within
    /// VOID_DEPTH): where the cut shows when the garment moves. With the worst point.
    pub risk_length: f64,
    pub risk_at: V3,
    pub edge_length: f64,
    /// The risky edge points (for reports and the x-ray marks).
    pub risk_points: Vec<V3>,
    /// Part of the loop folds over in every projection, so it could not be capped and probed: the
    /// opening is reported as not measured (M100), never passed.
    pub cap_failed: bool,
    /// Where a cut is visible now (the cut-edge sample with the most void views nothing blocks).
    pub now_at: V3,
}

impl Hole {
    pub fn exposed_share(&self) -> f64 {
        if self.probes == 0 { 0.0 } else { self.void_exposed as f64 / self.probes as f64 }
    }
    pub fn unbacked_share(&self) -> f64 {
        if self.probes == 0 { 0.0 } else { self.unbacked as f64 / self.probes as f64 }
    }
}

/// Through an opening, a drawn surface farther than this behind it is not what closes the hole.
pub const VOID_DEPTH: f64 = 1.5;
/// A garment rim this close to a cut edge can uncover it when the garment moves (SKIN-FLAWS A1:
/// keep a 1-2 cm skirt of kept faces under every garment edge; 2 units = 5 cm of lift allowed for).
pub const RIM_MARGIN: f64 = 2.0;

/// How far the surface reaches beyond Valve's hitbox capsules (SKIN-FLAWS H1).
#[derive(Debug, Clone, Default)]
pub struct Envelope {
    pub area: f64,
    pub p999: f64,
    /// Area of the visible (not hidden inside the body or another layer) surface.
    pub visible_area: f64,
    /// The worst visible point: distance beyond the nearest capsule, the point, and that capsule's name.
    pub visible_worst: (f64, V3, String),
    pub capsules: usize,
    /// Every judged triangle: distance beyond its nearest capsule, area, visible, the capsule (index into `caps`).
    /// Kept whole so the rules can draw their lines at whatever distance the policy in force says.
    pub tris: Vec<(f64, f64, bool, u16)>,
    /// Per capsule: name, worst distance beyond it, and that point.
    pub caps: Vec<(String, f64, V3)>,
    /// Per capsule, over the visible triangles only: worst distance beyond it and that point.
    pub visible_caps: Vec<(f64, V3)>,
}

impl Envelope {
    /// Area lying more than `units` beyond every capsule.
    pub fn beyond(&self, units: f64) -> f64 {
        self.tris.iter().filter(|t| t.0 > units).fold(0.0, |a, t| a + t.1)
    }
    /// Visible area lying more than `units` beyond every capsule.
    pub fn visible_beyond(&self, units: f64) -> f64 {
        self.tris.iter().filter(|t| t.2 && t.0 > units).fold(0.0, |a, t| a + t.1)
    }
    /// Area beyond `units` per hitbox region (the hitbox each triangle lies nearest), in `caps` order.
    pub fn region_areas(&self, units: f64, visible_only: bool) -> Vec<f64> {
        let mut area = vec![0.0; self.caps.len()];
        for t in self.tris.iter().filter(|t| t.0 > units && (t.2 || !visible_only)) {
            if let Some(a) = area.get_mut(t.3 as usize) {
                *a += t.1;
            }
        }
        area
    }
    /// The overflow past `units` that Valve's model at the same path does not already have: region by region
    /// (regions are matched by hitbox name; hitboxes that share a name, Baba's bench, are one region), our area
    /// beyond `units` less Valve's there, never below zero. With no Valve envelope it is all of ours.
    pub fn excess_beyond(&self, valve: Option<&Envelope>, units: f64, visible_only: bool) -> f64 {
        let by_name = |e: &Envelope| -> std::collections::BTreeMap<String, f64> {
            let mut m = std::collections::BTreeMap::new();
            for (c, a) in e.caps.iter().zip(e.region_areas(units, visible_only)) {
                *m.entry(c.0.clone()).or_insert(0.0) += a;
            }
            m
        };
        let theirs = valve.map(by_name).unwrap_or_default();
        by_name(self).into_iter().map(|(name, a)| (a - theirs.get(&name).copied().unwrap_or(0.0)).max(0.0)).sum()
    }
    /// The worst visible point of ours that lies further out than Valve's own surface does in the same hitbox
    /// region (by more than `slack`), judged from where Valve's surface stops: where Valve's own worst point in a
    /// region lies past `line`, that region's hitbox is not what Valve fitted its surface to, and our distance is
    /// counted less the stretch Valve's goes past the line. Returns that judged distance, the point, the region, and
    /// the stretch taken off (0 where Valve's surface stays inside the line). None when no region's worst is ours.
    pub fn new_worst(&self, valve: Option<&Envelope>, slack: f64, line: f64) -> Option<(f64, V3, String, f64)> {
        let worst_of = |e: &Envelope, name: &str| e.caps.iter().zip(&e.visible_caps).filter(|(c, _)| c.0 == name).map(|(_, v)| v.0).fold(f64::NEG_INFINITY, f64::max);
        let mut best: Option<(f64, V3, String, f64)> = None;
        for (i, (name, _, _)) in self.caps.iter().enumerate() {
            let Some(&(d, p)) = self.visible_caps.get(i) else { continue };
            let theirs = valve.map_or(f64::NEG_INFINITY, |v| worst_of(v, name));
            let shift = (theirs - line).max(0.0);
            let judged = d - shift;
            if d.is_finite() && d > theirs + slack && best.as_ref().is_none_or(|b| judged > b.0) {
                best = Some((judged, p, name.clone(), shift));
            }
        }
        best
    }
    /// Per nearest capsule, worst first: name, worst distance, its point, and the area beyond `units`.
    pub fn regions(&self, units: f64) -> Vec<(String, f64, V3, f64)> {
        let mut area = vec![0.0; self.caps.len()];
        for t in self.tris.iter().filter(|t| t.0 > units) {
            area[t.3 as usize] += t.1;
        }
        let mut out: Vec<(String, f64, V3, f64)> = self.caps.iter().zip(area).map(|((n, d, p), a)| (n.clone(), *d, *p, a)).collect();
        out.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        out
    }
}

/// One hitbox in model space at bind.
#[derive(Debug, Clone, Copy)]
pub enum HitShape {
    /// Two end points and a radius; a sphere is a capsule of no length.
    Capsule(V3, V3, f64),
    /// A box in its bone's frame: the bone's axes (rows) and origin, and the box's corners in that frame.
    Box { rot: [V3; 3], org: V3, lo: V3, hi: V3 },
}

impl HitShape {
    /// Signed distance from `p` to the shape's surface: negative inside.
    pub fn distance(&self, p: V3) -> f64 {
        match *self {
            HitShape::Capsule(s0, s1, r) => {
                let ab = v3::sub(s1, s0);
                let l = v3::dot(ab, ab);
                let t = if l > 1e-12 { (v3::dot(v3::sub(p, s0), ab) / l).clamp(0.0, 1.0) } else { 0.0 };
                v3::dist(p, v3::add(s0, v3::scale(ab, t))) - r
            }
            HitShape::Box { rot, org, lo, hi } => {
                // into the bone's frame (the rotation is orthonormal, so its transpose inverts it)
                let d = v3::sub(p, org);
                let q: V3 = [0, 1, 2].map(|k| rot[0][k] * d[0] + rot[1][k] * d[1] + rot[2][k] * d[2]);
                let (mut out2, mut inside) = (0.0, f64::NEG_INFINITY);
                for k in 0..3 {
                    let e = (lo[k] - q[k]).max(q[k] - hi[k]);
                    if e > 0.0 {
                        out2 += e * e;
                    }
                    inside = inside.max(e);
                }
                if out2 > 0.0 { out2.sqrt() } else { inside }
            }
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ZFight {
    pub a: usize,
    pub b: usize,
    pub pairs: usize,
    pub area: f64,
    pub worst_at: V3,
}

#[derive(Debug, Clone, Default)]
pub struct Cross {
    pub a: usize,
    pub b: usize,
    pub pairs: usize,
    pub length: f64,
    pub bounds: Aabb,
    /// Deepest vertex of a behind b's surface near the crossing, and of b behind a.
    pub depth_a_in_b: f64,
    pub depth_b_in_a: f64,
    pub visible_pairs: usize,
}

#[derive(Debug, Clone, Default)]
pub struct Contact {
    pub shell: usize,
    /// Nearest other shell and the closest approach to it.
    pub nearest: Option<usize>,
    pub gap: f64,
    /// Median distance from the shell's inner side (along -normal) to another surface, over the
    /// vertices whose ray finds one within reach; and the fraction of vertices with no surface at all.
    pub inward_median: f64,
    pub inward_none: f64,
    /// Reaches the body (largest shell) through contacts within CONTACT_TOL.
    pub grounded: bool,
    /// Skin mismatch with what the shell rests on: the mean half-L1 distance between each sampled
    /// vertex's weights and the weights of the nearest substrate point (0 = moves with it,
    /// 1 = on entirely different bones). A piece on other bones than its substrate lifts off or
    /// sinks in as soon as those bones move apart (SKIN-FLAWS A7).
    pub skin_mismatch: f64,
    /// The shell it rests on (nearest other shell at the sampled vertices) and that shell's dominant bone.
    pub substrate: Option<usize>,
    /// Where it rests: the mean of the sampled vertices within reach of a substrate (the mismatch's own samples).
    pub rest_at: V3,
}

#[derive(Debug, Clone, Default)]
pub struct UvFacts {
    pub material: String,
    pub tris: usize,
    pub islands: usize,
    /// Triangles wound against their island's majority (the island folds over itself).
    pub folded: usize,
    /// Textured triangles with zero UV area but real surface area: a texel smeared over a face.
    pub degenerate: usize,
    pub mirrored: usize,
    /// Fraction of covered texels claimed by two islands, and by one island twice.
    pub overlap: f64,
    pub self_overlap: f64,
    /// Area-weighted mean Sander stretch (1 = conformal) and the fraction of area above 4.
    pub stretch_mean: f64,
    pub stretch_over: f64,
    /// Texels per model unit, area-weighted, when the colour texture's size is known.
    pub density: Option<f64>,
    pub out_of_tile: f64,
}

#[derive(Debug, Clone, Default)]
pub struct SkinFacts {
    pub vertices: usize,
    pub unnormalised: usize,
    pub worst_sum: f32,
    pub over_slots: usize,
    pub tiny: usize,
    pub unweighted: usize,
    /// Influences on bones the model skeleton lacks.
    pub missing_bone: usize,
    /// Per bone (model index): farthest vertex weighted >= 0.1 to it.
    pub reach: BTreeMap<u16, f64>,
    /// (vertex position, bone, weight, distance) of influences far from their bone.
    pub far: Vec<(V3, u16, f32, f64)>,
    pub max_influences: usize,
}

#[derive(Debug, Clone, Default)]
pub struct Census {
    pub name: String,
    pub model: mesh::Model,
    pub lods: Vec<LodStats>,
    pub notes: Vec<String>,
    pub geo: Geo,
    pub topo: Topology,
    pub shells: Vec<ShellFact>,
    pub bounds: Aabb,
    pub bone_pos: Vec<V3>,
    pub nonfinite: usize,
    pub tri_visible: Vec<bool>,
    pub tri_seen: Vec<bool>,
    pub voids: Vec<Region>,
    pub backshade: Vec<Region>,
    pub zfights: Vec<ZFight>,
    pub crosses: Vec<Cross>,
    pub pairs_capped: bool,
    pub contacts: Vec<Contact>,
    /// Per shell, the shells within CONTACT_TOL of it (symmetric).
    pub touching: Vec<Vec<usize>>,
    pub seams: Vec<(usize, usize, usize, f64)>,
    pub uv: Vec<UvFacts>,
    pub skin: SkinFacts,
    pub unreferenced: usize,
    pub unused_materials: Vec<String>,
    pub untextured: Vec<(String, String)>,
    pub texture_bytes: u64,
    pub textures: BTreeSet<String>,
    pub tex_sizes: BTreeMap<String, (u32, u32)>,
    /// welded vertex -> first input vertex (to read its attributes)
    pub first_input: Vec<u32>,
    pub materials: BTreeMap<String, MatInfo>,
    pub symmetry: Option<geom::shape::Symmetry>,
    pub adj: Vec<Vec<(u32, bool)>>,
    pub holes: Vec<Hole>,
    pub envelope: Envelope,
    /// Every texture of every textured material with UVs, with its density on the model (M147).
    pub mip_reach: Vec<MipReach>,
    /// Materials that are the hero's skin (from the caller: Valve's body materials).
    pub skin_materials: BTreeSet<String>,
    pub albedo: Albedo,
    pub coverage: Vec<Coverage>,
    pub timings: Vec<(String, f64)>,
    pub families: Vec<Vec<usize>>,
    pub body: usize,
    pub reach: f64,
}

/// Barycentric coordinates of p (on or near triangle a, b, c).
pub fn barycentric(p: V3, a: V3, b: V3, c: V3) -> [f64; 3] {
    let (v0, v1, v2) = (v3::sub(b, a), v3::sub(c, a), v3::sub(p, a));
    let (d00, d01, d11, d20, d21) = (v3::dot(v0, v0), v3::dot(v0, v1), v3::dot(v1, v1), v3::dot(v2, v0), v3::dot(v2, v1));
    let den = d00 * d11 - d01 * d01;
    if den.abs() < 1e-30 {
        return [1.0, 0.0, 0.0];
    }
    let v = (d11 * d20 - d01 * d21) / den;
    let w = (d00 * d21 - d01 * d20) / den;
    [1.0 - v - w, v, w]
}

/// Two unit vectors orthogonal to `n` and each other.
pub fn ortho(n: V3) -> (V3, V3) {
    let a = if n[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
    let u = v3::norm(v3::cross(n, a));
    (u, v3::cross(n, u))
}

/// The side of the hero a direction points to (+X is the hero's front).
pub fn side_of(n: V3) -> &'static str {
    if n[0].abs() >= n[1].abs().max(n[2].abs()) {
        if n[0] > 0.0 { "front" } else { "back" }
    } else if n[1].abs() >= n[2].abs() {
        if n[1] > 0.0 { "left" } else { "right" }
    } else if n[2] > 0.0 {
        "top"
    } else {
        "underside"
    }
}

fn stem(p: &str) -> &str {
    let f = p.rsplit('/').next().unwrap_or(p);
    f.split('.').next().unwrap_or(f)
}

impl Census {
    pub fn build(data: &[u8], env: &Env) -> Result<Census, String> {
        let t0 = std::time::Instant::now();
        let mut c = Census::light(data, env)?;
        c.timings.push(("decode".into(), t0.elapsed().as_secs_f64()));
        c.measure(env);
        Ok(c)
    }

    /// Decode, LOD statistics, skeleton, materials and texture memory: what the cheap (default lint)
    /// rules read. No geometry census.
    pub fn light(data: &[u8], env: &Env) -> Result<Census, String> {
        let model = mesh::parse(data)?;
        let mut c = Census { name: model.name.clone(), notes: model.notes.clone(), ..Default::default() };
        for lod in 0..model.lod_count().max(1) {
            let (g, notes) = Geo::of(&model, lod);
            c.notes.extend(notes);
            let mut st = LodStats { lod, tris: g.tris.len(), verts: g.pos.len(), draws: g.surfs.len(), ..Default::default() };
            for s in &g.surfs {
                st.materials.insert(s.material.clone());
                if !st.meshes.contains(&s.mesh) {
                    st.meshes.push(s.mesh.clone());
                }
            }
            st.bounds = Aabb::empty();
            for (t, &si) in g.tris.iter().zip(&g.tsurf) {
                *st.material_tris.entry(g.surfs[si as usize].material.clone()).or_insert(0) += 1;
                for &v in t {
                    let p = g.pos[v as usize];
                    if p.iter().all(|x| x.is_finite()) {
                        st.bounds.grow(p);
                    }
                }
            }
            c.lods.push(st);
        }
        let (geo, _) = Geo::of(&model, 0);
        c.geo = geo;
        c.model = model;
        let w = c.model.skeleton.world();
        c.bone_pos = w.iter().map(|(_, t)| [t[0] as f64, t[1] as f64, t[2] as f64]).collect();
        for s in &c.geo.surfs {
            if !c.materials.contains_key(&s.material) {
                let m = (env.material)(&s.material).unwrap_or_default();
                c.materials.insert(s.material.clone(), m);
            }
        }
        // The hero's skin: Valve names it <hero>_body / _head / _face / _skin.
        for m in c.materials.keys() {
            let st = stem(m);
            if st.ends_with("_body") || st.ends_with("_head") || st.ends_with("_face") || st.contains("skin") {
                c.skin_materials.insert(m.clone());
            }
        }
        for m in c.materials.values() {
            for t in &m.textures {
                if c.textures.insert(t.clone()) {
                    if let Some(ti) = (env.texture)(t) {
                        c.texture_bytes += tex_bytes(&ti);
                        c.tex_sizes.insert(t.clone(), (ti.width as u32, ti.height as u32));
                    }
                }
            }
        }
        Ok(c)
    }

    /// The geometry census. After the shared groundwork (topology, shells, the probe views) the families of
    /// measures read it and write disjoint results, so they run side by side; the contact graph waits for
    /// the triangle-pair pass it reads. Every phase is the same code as before, only scheduled across threads,
    /// and each result lands in the same field, so the census is identical to a sequential run.
    fn measure(&mut self, env: &Env) {
        let g = &self.geo;
        self.nonfinite = g.pos.iter().filter(|p| !p.iter().all(|x| x.is_finite())).count();
        if self.nonfinite > 0 {
            self.notes.push(format!("{} non-finite vertex positions: geometry rules skipped", self.nonfinite));
            let t1 = std::time::Instant::now();
            let (albedo, coverage) = self.textures_measure(env);
            self.albedo = albedo;
            self.coverage = coverage;
            self.timings.push(("textures".into(), t1.elapsed().as_secs_f64()));
            return;
        }
        let t0 = std::time::Instant::now();
        self.topo = Topology::build(&g.pos, &g.tris);
        self.timings.push(("topology".into(), t0.elapsed().as_secs_f64()));
        self.first_input = vec![u32::MAX; self.topo.pos.len()];
        for (i, &w) in self.topo.weld.iter().enumerate() {
            if self.first_input[w as usize] == u32::MAX {
                self.first_input[w as usize] = i as u32;
            }
        }
        let mut bb = Aabb::empty();
        g.pos.iter().for_each(|p| bb.grow(*p));
        self.bounds = bb;
        self.reach = g.pos.iter().flat_map(|p| p.iter().map(|x| x.abs())).fold(0.0, f64::max);
        let t0 = std::time::Instant::now();
        self.label_shells();
        self.timings.push(("label".into(), t0.elapsed().as_secs_f64()));
        self.body = (0..self.shells.len()).max_by(|&a, &b| self.shells[a].area.partial_cmp(&self.shells[b].area).unwrap_or(std::cmp::Ordering::Equal)).unwrap_or(0);
        self.adj = self.topo.adjacency();
        let soup = self.geo.tri_soup();
        let bvh = Bvh::new(soup.clone());
        let mut times: Vec<(String, f64)> = Vec::new();
        let s = std::time::Instant::now();
        self.quality();
        times.push(("quality".into(), s.elapsed().as_secs_f64()));
        let s = std::time::Instant::now();
        self.probe(&soup);
        times.push(("probe".into(), s.elapsed().as_secs_f64()));

        // From here the census is read-only; every phase returns its own result.
        type Inventory = (usize, Vec<String>, Vec<(String, String)>);
        type Pairs = (Vec<ZFight>, Vec<Cross>, bool);
        type Contacts = (Vec<Contact>, Vec<Vec<usize>>);
        type Timed = Vec<(String, f64)>;
        let this: &Census = self;
        let (bvh, soup) = (&bvh, &soup);
        fn lap(t: &mut Timed, name: &str, start: std::time::Instant) {
            t.push((name.to_string(), start.elapsed().as_secs_f64()));
        }
        // group A: the triangle-pair pass, then the contact graph that reads its crossings
        let group_a = move || -> (Pairs, Contacts, Timed) {
            let mut t = Vec::new();
            let s = std::time::Instant::now();
            let pairs = this.pairs(bvh, soup);
            lap(&mut t, "pairs", s);
            let s = std::time::Instant::now();
            let contacts = this.contacts(bvh, &pairs.1);
            lap(&mut t, "contacts", s);
            (pairs, contacts, t)
        };
        // group B: the hole probes and the hitbox envelope
        let group_b = move || -> (Vec<Hole>, Envelope, Timed) {
            let mut t = Vec::new();
            let s = std::time::Instant::now();
            let holes = this.holes(bvh);
            lap(&mut t, "holes", s);
            let s = std::time::Instant::now();
            let envelope = this.envelope();
            lap(&mut t, "envelope", s);
            (holes, envelope, t)
        };
        // group C: the texture decodes (the heaviest I/O and decode of the census)
        let group_c = move || -> ((Albedo, Vec<Coverage>), f64) {
            let s = std::time::Instant::now();
            let r = this.textures_measure(env);
            (r, s.elapsed().as_secs_f64())
        };
        // group D: everything cheap that reads only the groundwork
        #[allow(clippy::type_complexity)]
        let group_d = move || -> (Vec<(usize, usize, usize, f64)>, Vec<UvFacts>, SkinFacts, Inventory, Option<geom::shape::Symmetry>, Vec<Vec<usize>>, Timed) {
            let mut t = Vec::new();
            let s = std::time::Instant::now();
            let seams = this.seams();
            lap(&mut t, "seams", s);
            let s = std::time::Instant::now();
            let uv = this.uv();
            lap(&mut t, "uv", s);
            let s = std::time::Instant::now();
            let skin = this.skin();
            lap(&mut t, "skin", s);
            let s = std::time::Instant::now();
            let inventory = this.inventory();
            lap(&mut t, "inventory", s);
            let s = std::time::Instant::now();
            let symmetry = geom::shape::symmetry(&this.topo.pos);
            lap(&mut t, "symmetry", s);
            let s = std::time::Instant::now();
            let families = this.families();
            lap(&mut t, "families", s);
            (seams, uv, skin, inventory, symmetry, families, t)
        };
        // through `par::join`, so the groups count against the thread cap like every other fan-out
        let ((d, a), (b, c)) = crate::par::join(|| crate::par::join(group_d, group_a), || crate::par::join(group_b, group_c));
        let ((pairs, contacts, mut ta), (holes, envelope, tb), (textures, tt), (seams, uv, skin, inventory, symmetry, families, td)) = (a, b, c, d);
        ta.extend(tb);
        ta.push(("textures".into(), tt));
        ta.extend(td);
        self.holes = holes;
        (self.zfights, self.crosses, self.pairs_capped) = pairs;
        (self.contacts, self.touching) = contacts;
        self.seams = seams;
        self.uv = uv;
        self.skin = skin;
        self.envelope = envelope;
        self.mip_reach = self.mip_reach(env);
        (self.unreferenced, self.unused_materials, self.untextured) = inventory;
        self.symmetry = symmetry;
        self.families = families;
        (self.albedo, self.coverage) = textures;
        let order = ["pairs", "contacts", "holes", "envelope", "seams", "uv", "skin", "inventory", "symmetry", "families", "textures"];
        ta.sort_by_key(|(n, _)| order.iter().position(|o| o == n).unwrap_or(usize::MAX));
        times.extend(ta);
        self.timings.extend(times);
    }

    fn label_shells(&mut self) {
        let g = &self.geo;
        let names = &self.model.skeleton.names;
        let mut per_mesh: HashMap<String, usize> = HashMap::new();
        for sh in &self.topo.shells {
            let mut mats: Vec<String> = Vec::new();
            let mut bone_w: HashMap<u16, f64> = HashMap::new();
            let mut mesh = String::new();
            for &t in &sh.tris {
                let s = &g.surfs[g.tsurf[t as usize] as usize];
                if mesh.is_empty() {
                    mesh = s.mesh.clone();
                }
                if !mats.contains(&s.material) {
                    mats.push(s.material.clone());
                }
                for &v in &g.tris[t as usize] {
                    for k in 0..8 {
                        let (j, wt) = (g.joints[v as usize][k], g.weights[v as usize][k]);
                        if j != u16::MAX && wt > 0.0 {
                            *bone_w.entry(j).or_default() += wt as f64;
                        }
                    }
                }
            }
            let bone = bone_w.iter().max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal).then(b.0.cmp(a.0))).and_then(|(j, _)| names.get(*j as usize)).cloned().unwrap_or_else(|| "unskinned".into());
            let n = per_mesh.entry(mesh.clone()).or_default();
            *n += 1;
            let label = format!(
                "{}/{} #{} ({} tris, {}, z {:.0}..{:.0})",
                mesh,
                mats.iter().map(|m| stem(m)).collect::<Vec<_>>().join("+"),
                n,
                sh.tris.len(),
                bone,
                sh.bounds.min[2],
                sh.bounds.max[2]
            );
            let (volume, winding) = if sh.closed {
                let tris: Vec<[u32; 3]> = sh.tris.iter().map(|&t| self.topo.tris[t as usize]).collect();
                match geom::mass::mass(&self.topo.pos, &tris) {
                    Some(m) => (Some(m.volume), m.winding),
                    None => (None, 0),
                }
            } else {
                (None, 0)
            };
            self.shells.push(ShellFact { label, mesh, materials: mats, bone, tris: sh.tris.len(), area: sh.area, bounds: sh.bounds, volume, winding, ..Default::default() });
        }
    }

    fn quality(&mut self) {
        let g = &self.geo;
        for (si, sh) in self.topo.shells.iter().enumerate() {
            let f = &mut self.shells[si];
            for &t in &sh.tris {
                let [a, b, c] = g.tri(t);
                let q = quality::quality(a, b, c);
                if q.zero_area {
                    f.zero_area += 1;
                } else {
                    if q.aspect > quality::HERO_MAX_ASPECT {
                        f.slivers += 1;
                    }
                    f.worst_aspect = f.worst_aspect.max(q.aspect);
                    // shading normals against the face: lit from the wrong side
                    let tr = g.tris[t as usize];
                    let fnrm = v3::tri_normal(a, b, c);
                    let mut avg = [0.0; 3];
                    for &v in &tr {
                        let n = g.nrm[v as usize];
                        avg = v3::add(avg, [n[0] as f64, n[1] as f64, n[2] as f64]);
                    }
                    if v3::len(avg) > 0.0 && v3::dot(v3::norm(avg), v3::norm(fnrm)) < 0.0 {
                        f.flipped_normals += 1;
                    }
                }
            }
        }
        for &t in &self.topo.collapsed {
            let s = self.topo.shell_of[t as usize];
            if s != u32::MAX {
                self.shells[s as usize].zero_area += 1;
            }
        }
    }

    /// Render every probe view; record which triangles the game draws (visible), which are seen
    /// first from behind (seen back), and cluster what shows through as void regions.
    fn probe(&mut self, tris: &[[V3; 3]]) {
        let g = &self.geo;
        let two_sided: Vec<bool> = g.tsurf.iter().map(|&s| self.materials.get(&g.surfs[s as usize].material).is_some_and(|m| m.two_sided)).collect();
        let n = tris.len();
        let mut visible = vec![false; n];
        let mut seen = vec![false; n];
        let mut void_px: Vec<Vec<(usize, u32)>> = vec![Vec::new(); n];
        let mut shade_px: Vec<Vec<(usize, u32)>> = vec![Vec::new(); n];
        let mut through: Vec<(u32, u32)> = vec![(0, 0); n];
        let views = raster::probe_views();
        // Each view renders on its own thread; what a view found is merged in view order.
        struct ViewHit {
            drawn: Vec<u32>,
            seen: Vec<u32>,
            vcount: HashMap<u32, u32>,
            scount: HashMap<u32, u32>,
            through: HashMap<u32, (u32, u32)>,
        }
        let bounds = self.bounds;
        let per_view = crate::par::map(&views, |view| {
            let f = Frame::fit(*view, &bounds, PROBE_UNIT, 2);
            let r = raster::rasterize(f, tris, &|t| two_sided[t as usize]);
            let mut h = ViewHit { drawn: Vec::new(), seen: Vec::new(), vcount: HashMap::new(), scount: HashMap::new(), through: HashMap::new() };
            for i in 0..r.any.len() {
                if r.drawn[i] != raster::NONE {
                    h.drawn.push(r.drawn[i]);
                }
                let t = r.any[i];
                if t == raster::NONE {
                    continue;
                }
                h.seen.push(t);
                if !r.any_front[i] {
                    if two_sided[t as usize] {
                        *h.scount.entry(t).or_default() += 1;
                    } else {
                        *h.vcount.entry(t).or_default() += 1;
                        let th = h.through.entry(t).or_default();
                        if r.drawn[i] == raster::NONE {
                            th.0 += 1;
                        } else {
                            th.1 += 1;
                        }
                    }
                }
            }
            h
        });
        for (vi, h) in per_view.into_iter().enumerate() {
            for t in h.drawn {
                visible[t as usize] = true;
            }
            for t in h.seen {
                seen[t as usize] = true;
            }
            for (t, (bg, md)) in h.through {
                through[t as usize].0 += bg;
                through[t as usize].1 += md;
            }
            for (t, c) in h.vcount {
                void_px[t as usize].push((vi, c));
            }
            for (t, c) in h.scount {
                shade_px[t as usize].push((vi, c));
            }
        }
        // Visible open and non-manifold edges.
        for e in &self.topo.edges {
            if e.faces.len() == 2 {
                continue;
            }
            if e.faces.iter().any(|(t, _)| seen[*t as usize]) {
                let s = self.topo.shell_of[e.faces[0].0 as usize] as usize;
                if e.faces.len() == 1 {
                    self.shells[s].visible_boundary_edges += 1;
                } else {
                    self.shells[s].visible_nonmanifold_edges += 1;
                }
            }
        }
        for (si, sh) in self.topo.shells.iter().enumerate() {
            let f = &mut self.shells[si];
            f.flipped_visible = 0;
            for &t in &sh.tris {
                let [a, b, c] = g.tri(t);
                let fnrm = v3::tri_normal(a, b, c);
                let tr = g.tris[t as usize];
                let mut avg = [0.0; 3];
                for &v in &tr {
                    let n = g.nrm[v as usize];
                    avg = v3::add(avg, [n[0] as f64, n[1] as f64, n[2] as f64]);
                }
                if visible[t as usize] && v3::len(avg) > 0.0 && v3::len(fnrm) > 0.0 && v3::dot(v3::norm(avg), v3::norm(fnrm)) < 0.0 {
                    f.flipped_visible += 1;
                }
            }
        }
        self.voids = self.regions(&void_px, &views, &through);
        self.backshade = self.regions(&shade_px, &views, &vec![(0, 0); n]);
        self.tri_visible = visible;
        self.tri_seen = seen;
    }

    fn regions(&self, px: &[Vec<(usize, u32)>], views: &[View], through: &[(u32, u32)]) -> Vec<Region> {
        let adj = &self.adj;
        let mut done = vec![false; px.len()];
        let mut out = Vec::new();
        let g = &self.geo;
        let pixel = PROBE_UNIT * PROBE_UNIT;
        for start in 0..px.len() {
            if done[start] || px[start].is_empty() {
                continue;
            }
            let mut stack = vec![start as u32];
            done[start] = true;
            let mut tris = Vec::new();
            while let Some(t) = stack.pop() {
                tris.push(t);
                for &(o, _) in &adj[t as usize] {
                    if !done[o as usize] && !px[o as usize].is_empty() {
                        done[o as usize] = true;
                        stack.push(o);
                    }
                }
            }
            let mut per_view: BTreeMap<usize, u32> = BTreeMap::new();
            let (mut bg, mut md) = (0usize, 0usize);
            let mut bone_w: HashMap<u16, f64> = HashMap::new();
            let mut nsum = [0.0; 3];
            let (mut zlo, mut zhi) = (f64::INFINITY, f64::NEG_INFINITY);
            let mut mats: BTreeMap<String, usize> = BTreeMap::new();
            for &t in &tris {
                for &(v, c) in &px[t as usize] {
                    *per_view.entry(v).or_default() += c;
                }
                bg += through[t as usize].0 as usize;
                md += through[t as usize].1 as usize;
                let [a, b, c] = g.tri(t);
                nsum = v3::add(nsum, v3::tri_normal(a, b, c));
                for p in [a, b, c] {
                    zlo = zlo.min(p[2]);
                    zhi = zhi.max(p[2]);
                }
                *mats.entry(g.surfs[g.tsurf[t as usize] as usize].material.clone()).or_default() += 1;
                for &v in &g.tris[t as usize] {
                    for k in 0..8 {
                        let (j, wt) = (g.joints[v as usize][k], g.weights[v as usize][k]);
                        if j != u16::MAX && wt > 0.0 {
                            *bone_w.entry(j).or_default() += wt as f64;
                        }
                    }
                }
            }
            let best = per_view.values().copied().max().unwrap_or(0) as f64 * pixel;
            let mut vs: Vec<(usize, u32)> = per_view.into_iter().collect();
            vs.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
            let bone = bone_w.iter().max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal).then(b.0.cmp(a.0))).and_then(|(j, _)| self.model.skeleton.names.get(*j as usize)).cloned().unwrap_or_else(|| "unskinned".into());
            // The faces seen from behind face away from the viewer: their outward normal is the
            // side of the body they belong to (+X is the hero's front).
            let side = if nsum[0].abs() >= nsum[1].abs().max(nsum[2].abs()) {
                if nsum[0] > 0.0 { "front" } else { "back" }
            } else if nsum[1].abs() >= nsum[2].abs() {
                if nsum[1] > 0.0 { "left" } else { "right" }
            } else if nsum[2] > 0.0 {
                "top"
            } else {
                "underside"
            };
            let material = mats.into_iter().max_by_key(|(_, n)| *n).map(|(m, _)| m).unwrap_or_default();
            let shell = self.topo.shell_of[tris[0] as usize] as usize;
            out.push(Region { shell, tris, area: best, views: vs.iter().take(4).map(|(v, _)| views[*v].name).collect(), bone, side, z: (zlo, zhi), material, to_background: bg, to_model: md });
        }
        out.sort_by(|a, b| b.area.partial_cmp(&a.area).unwrap_or(std::cmp::Ordering::Equal));
        out
    }

    /// Every boundary loop of a single-sided shell, probed (`openings.rs`): garment openings through
    /// their cap, cut-body openings from the views that reach the cut. Twin-backed cards have none.
    fn holes(&self, bvh: &Bvh) -> Vec<Hole> {
        super::openings::holes(self, bvh)
    }

    /// One pass over every pair of nearby triangles: coplanar same-facing overlap (z-fighting) and
    /// proper intersection (self and shell-shell), then penetration depths for crossing shell pairs.
    fn pairs(&self, bvh: &Bvh, tris: &[[V3; 3]]) -> (Vec<ZFight>, Vec<Cross>, bool) {
        let degenerate: Vec<bool> = tris.iter().map(|t| quality::quality(t[0], t[1], t[2]).zero_area).collect();
        let shell_of = &self.topo.shell_of;
        let mut zf: BTreeMap<(usize, usize), ZFight> = BTreeMap::new();
        let mut cr: BTreeMap<(usize, usize), Cross> = BTreeMap::new();
        let visible = &self.tri_visible;
        // The traversal collects the candidate pairs (cheap box tests, in a fixed order); the exact tests on
        // them are independent, so they run in chunks across threads; the results are folded back in the
        // traversal's order, which keeps every sum and "last pair" field exactly what a single pass gives.
        let mut cand: Vec<(u32, u32)> = Vec::new();
        let ok = bvh.pairs(bvh, PLANE_TOL, true, PAIR_CAP, &mut |a, b| cand.push((a, b)));
        enum Hit {
            None,
            Z { key: (usize, usize), area: f64, at: V3 },
            X { key: (usize, usize), visible: bool, seg: Option<(V3, V3)> },
        }
        let judge = |&(a, b): &(u32, u32)| -> Hit {
            if degenerate[a as usize] || degenerate[b as usize] {
                return Hit::None;
            }
            let (sa, sb) = (shell_of[a as usize], shell_of[b as usize]);
            if sa == u32::MAX || sb == u32::MAX {
                return Hit::None;
            }
            let (ta, tb) = (&tris[a as usize], &tris[b as usize]);
            let area = intersect::coplanar_overlap(ta, tb, PLANE_TOL, COS_TOL);
            let key = (sa.min(sb) as usize, sa.max(sb) as usize);
            if area > 1e-6 {
                return Hit::Z { key, area, at: v3::centroid(ta[0], ta[1], ta[2]) };
            }
            // neighbours meeting at a vertex or edge only cross if they fold (coplanar overlap, handled
            // above) or pierce through each other near the shared corner: either way, a proper intersection
            if !intersect::properly_intersect(ta, tb) {
                return Hit::None;
            }
            Hit::X { key, visible: visible[a as usize] || visible[b as usize], seg: intersect::crossing_segment(ta, tb) }
        };
        let chunks: Vec<&[(u32, u32)]> = cand.chunks(4096).collect();
        let hits: Vec<Vec<Hit>> = crate::par::map(&chunks, |ch| ch.iter().map(&judge).collect());
        for hit in hits.into_iter().flatten() {
            match hit {
                Hit::None => {}
                Hit::Z { key, area, at } => {
                    let z = zf.entry(key).or_insert_with(|| ZFight { a: key.0, b: key.1, ..Default::default() });
                    z.pairs += 1;
                    z.area += area;
                    z.worst_at = at;
                }
                Hit::X { key, visible, seg } => {
                    let c = cr.entry(key).or_insert_with(|| Cross { a: key.0, b: key.1, bounds: Aabb::empty(), ..Default::default() });
                    c.pairs += 1;
                    if visible {
                        c.visible_pairs += 1;
                    }
                    if let Some((p, q)) = seg {
                        c.length += v3::dist(p, q);
                        c.bounds.grow(p);
                        c.bounds.grow(q);
                    }
                }
            }
        }
        let capped = !ok;
        // Penetration depths for shell-shell crossings.
        let mut by_shell: Vec<Vec<u32>> = vec![Vec::new(); self.topo.shells.len()];
        for (t, &s) in shell_of.iter().enumerate() {
            if s != u32::MAX {
                by_shell[s as usize].push(t as u32);
            }
        }
        // per-shell vertex lists and BVHs, built once and only for shells that cross visibly
        let mut verts_of: HashMap<usize, Vec<u32>> = HashMap::new();
        let mut bvh_of: HashMap<usize, (Bvh, Vec<u32>)> = HashMap::new();
        for c in cr.values() {
            if c.a == c.b || c.bounds.is_empty() || c.visible_pairs == 0 {
                continue;
            }
            for s in [c.a, c.b] {
                verts_of.entry(s).or_insert_with(|| {
                    let mut v: Vec<u32> = by_shell[s].iter().flat_map(|&t| self.topo.tris[t as usize]).collect();
                    v.sort();
                    v.dedup();
                    v
                });
                bvh_of.entry(s).or_insert_with(|| (Bvh::new(by_shell[s].iter().map(|&t| tris[t as usize]).collect()), by_shell[s].clone()));
            }
        }
        for c in cr.values_mut() {
            if c.a == c.b || c.bounds.is_empty() || c.visible_pairs == 0 {
                continue;
            }
            let near = |inner: usize, outer: usize| -> f64 {
                // vertices within a few units of the crossing line: a sunk piece's corners sit as
                // deep as it is sunk
                let mut bb = c.bounds;
                bb.min = v3::sub(bb.min, [4.0; 3]);
                bb.max = v3::add(bb.max, [4.0; 3]);
                let (ob, _) = &bvh_of[&outer];
                let mut deepest = 0.0f64;
                for &v in &verts_of[&inner] {
                    let p = self.topo.pos[v as usize];
                    if bb.dist2(p) > 0.0 {
                        continue;
                    }
                    if let Some((ot, q, d)) = ob.nearest(p, 5.0, &|_| true) {
                        let tr = &ob.tris[ot as usize];
                        let n = v3::norm(v3::tri_normal(tr[0], tr[1], tr[2]));
                        if v3::dot(v3::sub(p, q), n) < 0.0 {
                            deepest = deepest.max(d);
                        }
                    }
                }
                deepest
            };
            c.depth_a_in_b = near(c.a, c.b);
            c.depth_b_in_a = near(c.b, c.a);
        }
        let _ = shell_of;
        (zf.into_values().collect(), cr.into_values().collect(), capped)
    }

    /// Contact graph between shells (scene3d's support closure, with the body as the ground) and
    /// each shell's inward gap.
    fn contacts(&self, bvh: &Bvh, crosses: &[Cross]) -> (Vec<Contact>, Vec<Vec<usize>>) {
        let n = self.topo.shells.len();
        if n == 0 {
            return (Vec::new(), Vec::new());
        }
        let shell_of = self.topo.shell_of.clone();
        // Each shell's measures read the shared surface and write only its own row: shells run on separate
        // threads and come back in shell order.
        let shell_ids: Vec<usize> = (0..n).collect();
        let rows = crate::par::map(&shell_ids, |&si| -> (Contact, BTreeSet<usize>) {
            let sh = &self.topo.shells[si];
            let mut adj_si: BTreeSet<usize> = BTreeSet::new();
            let mut verts: Vec<u32> = sh.tris.iter().flat_map(|&t| self.topo.tris[t as usize]).collect();
            verts.sort();
            verts.dedup();
            let stride = (verts.len() / 400).max(1);
            let (mut gap, mut nearest) = (f64::INFINITY, None);
            let others = |t: u32| shell_of[t as usize] != si as u32 && shell_of[t as usize] != u32::MAX;
            // closest approach: every vertex for small shells, a stride for big ones, then refine
            // the best by alternating projection (scene3d's contact measure)
            for &v in verts.iter().step_by(stride) {
                let p = self.topo.pos[v as usize];
                if let Some((t, _, d)) = bvh.nearest(p, gap.min(50.0), &others) {
                    if d < gap {
                        gap = d;
                        nearest = Some(shell_of[t as usize] as usize);
                    }
                }
            }
            // contacts: any vertex within tolerance of each other shell
            for &v in &verts {
                let p = self.topo.pos[v as usize];
                if let Some((t, _, _)) = bvh.nearest(p, CONTACT_TOL, &others) {
                    adj_si.insert(shell_of[t as usize] as usize);
                }
            }
            if !adj_si.is_empty() {
                gap = gap.min(CONTACT_TOL);
            }
            // inward gaps along each vertex's shading normal reversed
            let mut gaps = Vec::new();
            let mut none = 0usize;
            let mut tried = 0usize;
            for &v in verts.iter().step_by(stride) {
                let p = self.topo.pos[v as usize];
                // find an input vertex for the welded one to read its normal
                let n = self.normal_of_welded(v);
                if v3::len(n) == 0.0 {
                    continue;
                }
                tried += 1;
                let d = v3::scale(n, -1.0);
                match bvh.ray(p, d, 1e-4, 6.0, &others) {
                    Some(h) => gaps.push(h.t),
                    None => none += 1,
                }
            }
            gaps.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let median = gaps.get(gaps.len() / 2).copied().unwrap_or(f64::INFINITY);
            // skin mismatch against the nearest substrate surface (within 1 unit)
            let (mut mis, mut nmis) = (0.0, 0usize);
            let mut rest_sum = [0.0; 3];
            let mut sub_votes: HashMap<usize, usize> = HashMap::new();
            for &v in verts.iter().step_by(stride) {
                let p = self.topo.pos[v as usize];
                let Some((t, q, _)) = bvh.nearest(p, 1.0, &others) else { continue };
                *sub_votes.entry(shell_of[t as usize] as usize).or_default() += 1;
                let wa = self.weights_of(self.first_input[v as usize] as usize);
                let tr = self.geo.tris[t as usize];
                let (a, b, c) = (self.geo.pos[tr[0] as usize], self.geo.pos[tr[1] as usize], self.geo.pos[tr[2] as usize]);
                let bc = barycentric(q, a, b, c);
                let mut wb: HashMap<u16, f64> = HashMap::new();
                for k in 0..3 {
                    for (j, w) in self.weights_of(tr[k] as usize) {
                        *wb.entry(j).or_default() += w * bc[k];
                    }
                }
                let mut l1 = 0.0;
                let mut keys: Vec<u16> = wa.iter().map(|x| x.0).collect();
                keys.extend(wb.keys().copied());
                keys.sort();
                keys.dedup();
                for j in keys {
                    let x = wa.iter().find(|y| y.0 == j).map_or(0.0, |y| y.1);
                    let y = wb.get(&j).copied().unwrap_or(0.0);
                    l1 += (x - y).abs();
                }
                if !wa.is_empty() && !wb.is_empty() {
                    mis += 0.5 * l1;
                    nmis += 1;
                    rest_sum = v3::add(rest_sum, p);
                }
            }
            let substrate = sub_votes.into_iter().max_by_key(|x| (x.1, std::cmp::Reverse(x.0))).map(|x| x.0);
            let contact = Contact {
                shell: si,
                nearest,
                gap,
                inward_median: median,
                inward_none: if tried > 0 { none as f64 / tried as f64 } else { 1.0 },
                grounded: false,
                skin_mismatch: if nmis > 0 { mis / nmis as f64 } else { 0.0 },
                substrate,
                rest_at: if nmis > 0 { v3::scale(rest_sum, 1.0 / nmis as f64) } else { [0.0; 3] },
            };
            (contact, adj_si)
        });
        let mut adjacent: Vec<BTreeSet<usize>> = Vec::with_capacity(n);
        let mut out: Vec<Contact> = Vec::with_capacity(n);
        for (c, a) in rows {
            out.push(c);
            adjacent.push(a);
        }
        // shells that cross each other are in contact (their vertices can all sit away from the
        // other's surface while their faces pass through it)
        for c in crosses {
            if c.a != c.b {
                adjacent[c.a].insert(c.b);
                adjacent[c.b].insert(c.a);
                for k in out.iter_mut().filter(|k| k.shell == c.a || k.shell == c.b) {
                    k.gap = 0.0;
                }
            }
        }
        // symmetric closure from the body
        for a in 0..n {
            let items: Vec<usize> = adjacent[a].iter().copied().collect();
            for b in items {
                adjacent[b].insert(a);
            }
        }
        let touching: Vec<Vec<usize>> = adjacent.iter().map(|a| a.iter().copied().collect()).collect();
        let mut grounded = vec![false; n];
        let mut q = vec![self.body];
        grounded[self.body] = true;
        while let Some(s) = q.pop() {
            for &o in &adjacent[s] {
                if !grounded[o] {
                    grounded[o] = true;
                    q.push(o);
                }
            }
        }
        for c in out.iter_mut() {
            c.grounded = grounded[c.shell];
        }
        (out, touching)
    }

    /// A vertex's skin as (bone, weight) pairs with positive weight.
    fn weights_of(&self, v: usize) -> Vec<(u16, f64)> {
        let (j, w) = (self.geo.joints[v], self.geo.weights[v]);
        (0..8).filter(|&k| w[k] > 0.0 && j[k] != u16::MAX).map(|k| (j[k], w[k] as f64)).collect()
    }

    fn normal_of_welded(&self, w: u32) -> V3 {
        let i = self.first_input[w as usize] as usize;
        let n = self.geo.nrm[i];
        [n[0] as f64, n[1] as f64, n[2] as f64]
    }

    /// Boundary vertices of different shells within SEAM_TOL that did not weld: cracks.
    fn seams(&self) -> Vec<(usize, usize, usize, f64)> {
        let mut bverts: Vec<(u32, u32)> = Vec::new(); // (welded vertex, shell)
        for e in &self.topo.edges {
            if e.faces.len() == 1 {
                let s = self.topo.shell_of[e.faces[0].0 as usize];
                bverts.push((e.a, s));
                bverts.push((e.b, s));
            }
        }
        bverts.sort();
        bverts.dedup();
        // each boundary vertex's outward direction: the summed normals of its boundary faces
        let mut bnorm: HashMap<(u32, u32), V3> = HashMap::new();
        for e in &self.topo.edges {
            if e.faces.len() == 1 {
                let t = e.faces[0].0;
                let s = self.topo.shell_of[t as usize];
                let tr = self.geo.tri(t);
                let n = v3::tri_normal(tr[0], tr[1], tr[2]);
                for v in [e.a, e.b] {
                    let x = bnorm.entry((v, s)).or_insert([0.0; 3]);
                    *x = v3::add(*x, n);
                }
            }
        }
        let cell = SEAM_TOL;
        let mut grid: HashMap<(i64, i64, i64), Vec<usize>> = HashMap::new();
        let k = |p: V3| ((p[0] / cell).floor() as i64, (p[1] / cell).floor() as i64, (p[2] / cell).floor() as i64);
        for (i, (v, _)) in bverts.iter().enumerate() {
            grid.entry(k(self.topo.pos[*v as usize])).or_default().push(i);
        }
        let mut found: BTreeMap<(usize, usize), (usize, f64)> = BTreeMap::new();
        for (i, (v, s)) in bverts.iter().enumerate() {
            let p = self.topo.pos[*v as usize];
            let c = k(p);
            for dx in -1..=1 {
                for dy in -1..=1 {
                    for dz in -1..=1 {
                        if let Some(l) = grid.get(&(c.0 + dx, c.1 + dy, c.2 + dz)) {
                            for &j in l {
                                if j <= i {
                                    continue;
                                }
                                let (w, t) = bverts[j];
                                if w == *v || t == *s {
                                    continue;
                                }
                                let d = v3::dist(p, self.topo.pos[w as usize]);
                                // Opposite-facing rims are the edge of a double-layer card (a front
                                // and its twin back): stacked, not side by side, so face-on nothing
                                // shows between them.
                                let twin_rim = || {
                                    let (n1, n2) = (bnorm.get(&(*v, *s)).copied().unwrap_or([0.0; 3]), bnorm.get(&(w, t)).copied().unwrap_or([0.0; 3]));
                                    v3::dot(v3::norm(n1), v3::norm(n2)) < -super::openings::TWIN_COS
                                };
                                if d > 0.0 && d < SEAM_TOL && !twin_rim() {
                                    let key = ((*s).min(t) as usize, (*s).max(t) as usize);
                                    let e = found.entry(key).or_insert((0, 0.0));
                                    e.0 += 1;
                                    e.1 = e.1.max(d);
                                }
                            }
                        }
                    }
                }
            }
        }
        found.into_iter().map(|((a, b), (n, d))| (a, b, n, d)).collect()
    }

    fn uv(&self) -> Vec<UvFacts> {
        let g = &self.geo;
        let mut uv_out: Vec<UvFacts> = Vec::new();
        let mut by_mat: BTreeMap<String, Vec<u32>> = BTreeMap::new();
        for (t, &s) in g.tsurf.iter().enumerate() {
            let sm = &g.surfs[s as usize];
            if sm.has_uv {
                by_mat.entry(sm.material.clone()).or_default().push(t as u32);
            }
        }
        for (mat, tlist) in by_mat {
            let info = self.materials.get(&mat).cloned().unwrap_or_default();
            let mut f = UvFacts { material: mat.clone(), tris: tlist.len(), ..Default::default() };
            // UV islands: triangles sharing an edge whose two corners match in position and UV.
            let mut uf: Vec<u32> = (0..tlist.len() as u32).collect();
            fn find(uf: &mut [u32], x: u32) -> u32 {
                let mut r = x;
                while uf[r as usize] != r {
                    r = uf[r as usize];
                }
                let mut x = x;
                while uf[x as usize] != r {
                    let n = uf[x as usize];
                    uf[x as usize] = r;
                    x = n;
                }
                r
            }
            let key = |v: u32| {
                let p = g.pos[v as usize];
                let u = g.uv[v as usize];
                (p[0].to_bits(), p[1].to_bits(), p[2].to_bits(), u[0].to_bits(), u[1].to_bits())
            };
            let mut emap: HashMap<((u64, u64, u64, u32, u32), (u64, u64, u64, u32, u32)), u32> = HashMap::new();
            for (li, &t) in tlist.iter().enumerate() {
                let tr = g.tris[t as usize];
                for k in 0..3 {
                    let (a, b) = (key(tr[k]), key(tr[(k + 1) % 3]));
                    let e = if a < b { (a, b) } else { (b, a) };
                    match emap.get(&e) {
                        Some(&o) => {
                            let (x, y) = (find(&mut uf, li as u32), find(&mut uf, o));
                            if x != y {
                                uf[x.max(y) as usize] = x.min(y);
                            }
                        }
                        None => {
                            emap.insert(e, li as u32);
                        }
                    }
                }
            }
            let mut island_sum: HashMap<u32, f64> = HashMap::new();
            let mut signed = vec![0.0f64; tlist.len()];
            let mut stretch_area = 0.0;
            let mut stretch_sum = 0.0;
            let mut stretch_over = 0.0;
            let mut total_area = 0.0;
            let (mut dens_w, mut dens_s) = (0.0, 0.0);
            let tex = info.color.as_ref().and_then(|c| self.texture_size(c));
            let mut oob = 0usize;
            let mut degenerate_tris: Vec<usize> = Vec::new();
            for (li, &t) in tlist.iter().enumerate() {
                let tr = g.tris[t as usize];
                let (u0, u1, u2) = (g.uv[tr[0] as usize], g.uv[tr[1] as usize], g.uv[tr[2] as usize]);
                let s = ((u1[0] - u0[0]) as f64 * (u2[1] - u0[1]) as f64 - (u2[0] - u0[0]) as f64 * (u1[1] - u0[1]) as f64) / 2.0;
                signed[li] = s;
                let r = find(&mut uf, li as u32);
                *island_sum.entry(r).or_default() += s;
                let [a, b, c] = g.tri(t);
                let area = v3::tri_area(a, b, c);
                total_area += area;
                for u in [u0, u1, u2] {
                    if !(-1.0..=2.0).contains(&u[0]) || !(-1.0..=2.0).contains(&u[1]) {
                        oob += 1;
                    }
                }
                if info.textured && s.abs() < 1e-12 && area > 1e-6 {
                    degenerate_tris.push(li);
                }
                if s.abs() > 1e-14 && area > 1e-9 {
                    // Sander's stretch: singular values of the uv -> 3D Jacobian
                    let (s1, t1, s2, t2, s3, t3) = (u0[0] as f64, u0[1] as f64, u1[0] as f64, u1[1] as f64, u2[0] as f64, u2[1] as f64);
                    let det = (s2 - s1) * (t3 - t1) - (s3 - s1) * (t2 - t1);
                    let ss = v3::scale(v3::add(v3::add(v3::scale(a, t2 - t3), v3::scale(b, t3 - t1)), v3::scale(c, t1 - t2)), 1.0 / det);
                    let st = v3::scale(v3::add(v3::add(v3::scale(a, s3 - s2), v3::scale(b, s1 - s3)), v3::scale(c, s2 - s1)), 1.0 / det);
                    let (x, y, z) = (v3::dot(ss, ss), v3::dot(ss, st), v3::dot(st, st));
                    let disc = ((x - z) * (x - z) + 4.0 * y * y).max(0.0).sqrt();
                    let (mx, mn) = (0.5 * (x + z + disc), 0.5 * (x + z - disc));
                    if mn > 1e-20 {
                        let an = (mx / mn).sqrt();
                        stretch_sum += an * area;
                        stretch_area += area;
                        if an > 4.0 {
                            stretch_over += area;
                        }
                    }
                    if let Some((w, h)) = tex {
                        dens_s += (s.abs() * w as f64 * h as f64 / area).sqrt() * area;
                        dens_w += area;
                    }
                }
            }
            let _ = total_area;
            f.islands = island_sum.len();
            // A palette island (its whole UV area collapsed: it samples one swatch) is a design,
            // not a smear; only zero-area triangles inside a real unwrap count.
            let mut island_abs: HashMap<u32, f64> = HashMap::new();
            for (li, s) in signed.iter().enumerate() {
                *island_abs.entry(find(&mut uf, li as u32)).or_default() += s.abs();
            }
            for &li in &degenerate_tris {
                let r = find(&mut uf, li as u32);
                if island_abs[&r] > 1e-9 {
                    f.degenerate += 1;
                }
            }
            for (li, s) in signed.iter().enumerate() {
                let r = find(&mut uf, li as u32);
                let maj = island_sum[&r];
                if s.abs() > 1e-14 && maj.abs() > 1e-14 && s.signum() != maj.signum() {
                    f.folded += 1;
                }
                if *s < -1e-14 {
                    f.mirrored += 1;
                }
            }
            f.stretch_mean = if stretch_area > 0.0 { stretch_sum / stretch_area } else { 1.0 };
            f.stretch_over = if stretch_area > 0.0 { stretch_over / stretch_area } else { 0.0 };
            f.density = if dens_w > 0.0 { Some(dens_s / dens_w) } else { None };
            f.out_of_tile = oob as f64 / (3 * tlist.len()).max(1) as f64;
            // overlap: rasterize each triangle into a 512^2 texel grid (wrapped into the unit tile)
            const N: usize = 512;
            let mut owner = vec![u32::MAX; N * N];
            let mut cross_hits = 0usize;
            let mut self_hits = 0usize;
            let mut covered = 0usize;
            let mut owner_tri = vec![u32::MAX; N * N];
            let mut flag = vec![0u8; N * N];
            let adj = &self.adj;
            for (li, &t) in tlist.iter().enumerate() {
                let r = find(&mut uf, li as u32);
                let tr = g.tris[t as usize];
                let us: Vec<[f32; 2]> = tr.iter().map(|&v| g.uv[v as usize]).collect();
                let cu = ((us[0][0] + us[1][0] + us[2][0]) / 3.0).floor();
                let cv = ((us[0][1] + us[1][1] + us[2][1]) / 3.0).floor();
                let p: Vec<(f64, f64)> = us.iter().map(|u| (((u[0] - cu) as f64) * N as f64, ((u[1] - cv) as f64) * N as f64)).collect();
                let area = (p[1].0 - p[0].0) * (p[2].1 - p[0].1) - (p[2].0 - p[0].0) * (p[1].1 - p[0].1);
                if area.abs() < 1e-12 {
                    continue;
                }
                let x0 = p.iter().map(|q| q.0).fold(f64::INFINITY, f64::min).floor().max(0.0) as usize;
                let x1 = (p.iter().map(|q| q.0).fold(f64::NEG_INFINITY, f64::max).ceil() as usize).min(N - 1);
                let y0 = p.iter().map(|q| q.1).fold(f64::INFINITY, f64::min).floor().max(0.0) as usize;
                let y1 = (p.iter().map(|q| q.1).fold(f64::NEG_INFINITY, f64::max).ceil() as usize).min(N - 1);
                for y in y0..=y1 {
                    for x in x0..=x1 {
                        let (cx, cy) = (x as f64 + 0.5, y as f64 + 0.5);
                        let w0 = ((p[1].0 - cx) * (p[2].1 - cy) - (p[2].0 - cx) * (p[1].1 - cy)) / area;
                        let w1 = ((p[2].0 - cx) * (p[0].1 - cy) - (p[0].0 - cx) * (p[2].1 - cy)) / area;
                        let w2 = 1.0 - w0 - w1;
                        if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                            continue;
                        }
                        let i = y * N + x;
                        match owner[i] {
                            u32::MAX => {
                                owner[i] = r;
                                owner_tri[i] = t;
                                covered += 1;
                            }
                            o if o != r => {
                                if flag[i] & 1 == 0 {
                                    flag[i] |= 1;
                                    cross_hits += 1;
                                }
                            }
                            _ => {
                                let ot = owner_tri[i];
                                if ot != t && flag[i] & 2 == 0 && !adj[t as usize].iter().any(|(n, _)| *n == ot) {
                                    flag[i] |= 2;
                                    self_hits += 1;
                                }
                            }
                        }
                    }
                }
            }
            f.overlap = if covered > 0 { cross_hits as f64 / covered as f64 } else { 0.0 };
            f.self_overlap = if covered > 0 { self_hits as f64 / covered as f64 } else { 0.0 };
            uv_out.push(f);
        }
        uv_out
    }

    fn texture_size(&self, tex: &str) -> Option<(u32, u32)> {
        self.tex_sizes.get(tex).copied()
    }

    fn skin(&self) -> SkinFacts {
        let g = &self.geo;
        let nb = self.bone_pos.len();
        let mut s = SkinFacts::default();
        let slots: HashMap<String, u32> = self.model.meshes.iter().map(|m| (m.name.clone(), m.bone_weight_count)).collect();
        for (v, w) in g.weights.iter().enumerate() {
            let sm = &g.surfs[g.vsurf[v] as usize];
            if !sm.has_skin {
                continue;
            }
            s.vertices += 1;
            let j = g.joints[v];
            let sum: f32 = w.iter().sum();
            let n_inf = w.iter().filter(|x| **x > 0.0).count();
            s.max_influences = s.max_influences.max(n_inf);
            if (sum - 1.0).abs() > 2.0 / 255.0 + 1e-4 {
                s.unnormalised += 1;
                if (sum - 1.0).abs() > (s.worst_sum - 1.0).abs() {
                    s.worst_sum = sum;
                }
            }
            let cap = slots.get(&sm.mesh).copied().unwrap_or(4).max(1) as usize;
            if n_inf > cap {
                s.over_slots += 1;
            }
            s.tiny += w.iter().filter(|x| **x > 0.0 && **x < 1.5 / 255.0).count();
            let mut valid = false;
            for k in 0..8 {
                if w[k] <= 0.0 {
                    continue;
                }
                if j[k] == u16::MAX || j[k] as usize >= nb {
                    s.missing_bone += 1;
                    continue;
                }
                valid = true;
                if w[k] >= 0.1 {
                    let d = v3::dist(g.pos[v], self.bone_pos[j[k] as usize]);
                    let e = s.reach.entry(j[k]).or_insert(0.0);
                    *e = e.max(d);
                }
            }
            if !valid {
                s.unweighted += 1;
            }
        }
        // the far list is filled against Valve's reach in rules.rs; keep per-influence data small
        for (v, w) in g.weights.iter().enumerate() {
            if !g.surfs[g.vsurf[v] as usize].has_skin {
                continue;
            }
            for k in 0..8 {
                let j = g.joints[v][k];
                if w[k] >= 0.1 && j != u16::MAX && (j as usize) < nb {
                    let d = v3::dist(g.pos[v], self.bone_pos[j as usize]);
                    if d > 8.0 {
                        s.far.push((g.pos[v], j, w[k], d));
                    }
                }
            }
        }
        s
    }

    /// Albedo samples and alpha-test coverage, from the colour maps' decoded pixels.
    fn textures_measure(&self, env: &Env) -> (Albedo, Vec<Coverage>) {
        let g = &self.geo;
        let mut coverage_out: Vec<Coverage> = Vec::new();
        if g.tris.is_empty() {
            return (Albedo::default(), coverage_out);
        }
        // decode each colour map once, up front (a 4096^2 map is 64 MB: never copy it per triangle)
        // (the maps decode on separate threads: each is independent)
        let mut wanted: Vec<String> = Vec::new();
        for s in &g.surfs {
            if let Some(c) = self.materials.get(&s.material).and_then(|m| m.color.clone()) {
                if !c.starts_with("materials/default/") && !wanted.contains(&c) {
                    wanted.push(c);
                }
            }
        }
        let decoded = crate::par::map(&wanted, |c| (env.file)(c).and_then(|d| resource::pixels::decode(&d, 0)));
        let cache: HashMap<String, Option<(usize, usize, Vec<u8>)>> = wanted.into_iter().zip(decoded).collect();
        let mut albedo = Albedo::default();
        for t in 0..g.tris.len() {
            let sm = &g.surfs[g.tsurf[t] as usize];
            let Some(m) = self.materials.get(&sm.material) else { continue };
            if m.additive || !sm.has_uv {
                continue;
            }
            let Some(color) = &m.color else { continue };
            if color.starts_with("materials/default/") {
                continue;
            }
            let Some((w, h, px)) = cache.get(color).and_then(|x| x.as_ref()).map(|(w, h, p)| (*w, *h, p)) else {
                if !albedo.undecoded.contains(color) {
                    albedo.undecoded.push(color.clone());
                }
                continue;
            };
            let tr = g.tris[t];
            let u = (g.uv[tr[0] as usize][0] + g.uv[tr[1] as usize][0] + g.uv[tr[2] as usize][0]) / 3.0;
            let v = (g.uv[tr[0] as usize][1] + g.uv[tr[1] as usize][1] + g.uv[tr[2] as usize][1]) / 3.0;
            let x = ((u - u.floor()) * w as f32) as usize % w;
            let y = ((v - v.floor()) * h as f32) as usize % h;
            let i = 4 * (y * w + x);
            let [a, b, c] = g.tri(t as u32);
            albedo.samples.entry(sm.material.clone()).or_default().push((luminance([px[i], px[i + 1], px[i + 2]]), v3::tri_area(a, b, c)));
        }
        // Alpha coverage per alpha-tested material, on the mips the compiler stored.
        let mats: Vec<(String, MatInfo)> = self.materials.iter().filter(|(_, m)| m.alpha_test && m.color.is_some()).map(|(k, m)| (k.clone(), m.clone())).collect();
        let covs = crate::par::map(&mats, |(name, m)| -> Option<Coverage> {
            let name = name.clone();
            let color = m.color.clone().unwrap();
            let data = (env.file)(&color)?;
            let ti = resource::texture::info(&data)?;
            let (w0, h0, mip0) = resource::pixels::decode(&data, 0)?;
            // texel density on the model, texels per metre
            let (mut dens, mut wsum) = (0.0, 0.0);
            for t in 0..g.tris.len() {
                if g.surfs[g.tsurf[t] as usize].material != name {
                    continue;
                }
                let tr = g.tris[t];
                let (u0, u1, u2) = (g.uv[tr[0] as usize], g.uv[tr[1] as usize], g.uv[tr[2] as usize]);
                let uv_area = (((u1[0] - u0[0]) * (u2[1] - u0[1]) - (u2[0] - u0[0]) * (u1[1] - u0[1])) as f64).abs() / 2.0;
                let [a, b, c] = g.tri(t as u32);
                let area = v3::tri_area(a, b, c);
                if area > 1e-9 && uv_area > 1e-12 {
                    dens += (uv_area * (w0 * h0) as f64 / area).sqrt() * area;
                    wsum += area;
                }
            }
            if wsum == 0.0 {
                return None;
            }
            let per_metre = dens / wsum / 0.0254;
            let reference = (m.alpha_ref * 255.0).round() as u8;
            let tile = 256usize.min(w0).min(h0).max(1);
            let cov = |w: usize, h: usize, px: &[u8], ts: usize, tx: usize, ty: usize| -> f64 {
                let (mut on, mut n) = (0usize, 0usize);
                for y in ty * ts..((ty + 1) * ts).min(h) {
                    for x in tx * ts..((tx + 1) * ts).min(w) {
                        n += 1;
                        on += (px[4 * (y * w + x) + 3] >= reference) as usize;
                    }
                }
                if n == 0 { 0.0 } else { on as f64 / n as f64 }
            };
            let (tw, th) = (w0.div_ceil(tile), h0.div_ceil(tile));
            let base: Vec<f64> = (0..th).flat_map(|ty| (0..tw).map(move |tx| (tx, ty))).map(|(tx, ty)| cov(w0, h0, &mip0, tile, tx, ty)).collect();
            let mixed: Vec<usize> = (0..base.len()).filter(|&i| base[i] > 0.05 && base[i] < 0.95).collect();
            let mut out = Coverage { material: name.clone(), texture: color.clone(), texels_per_metre: per_metre, tiles: mixed.len(), ..Default::default() };
            for d in [4.0f64, 8.0] {
                let screen = 1080.0 / (2.0 * d * 30f64.to_radians().tan());
                let mip = ((per_metre / screen).log2().round().max(0.0) as u32).min(ti.mips.max(1) as u32 - 1);
                let share = if mip == 0 || mixed.is_empty() {
                    0.0
                } else {
                    match resource::pixels::decode(&data, mip) {
                        Some((w, h, px)) => {
                            let ts = (tile >> mip).max(1);
                            let changed = mixed.iter().filter(|&&i| {
                                let (tx, ty) = (i % tw, i / tw);
                                let c = cov(w, h, &px, ts, tx, ty);
                                (c / base[i] - 1.0).abs() > 0.3
                            }).count();
                            changed as f64 / mixed.len() as f64
                        }
                        None => 0.0,
                    }
                };
                out.at.push((d, mip, share));
            }
            Some(out)
        });
        coverage_out.extend(covs.into_iter().flatten());
        (albedo, coverage_out)
    }

    /// Per textured material: how densely its UVs lay a texture on the model, and what each of its textures stores.
    /// Headers only, no pixels.
    fn mip_reach(&self, env: &Env) -> Vec<MipReach> {
        let g = &self.geo;
        // UV units per world unit, area-weighted, per material
        let mut dens: BTreeMap<&str, (f64, f64)> = BTreeMap::new();
        for t in 0..g.tris.len() {
            let sm = &g.surfs[g.tsurf[t] as usize];
            if !sm.has_uv {
                continue;
            }
            let tr = g.tris[t];
            let (u0, u1, u2) = (g.uv[tr[0] as usize], g.uv[tr[1] as usize], g.uv[tr[2] as usize]);
            let uv_area = (((u1[0] - u0[0]) * (u2[1] - u0[1]) - (u2[0] - u0[0]) * (u1[1] - u0[1])) as f64).abs() / 2.0;
            let [a, b, c] = g.tri(t as u32);
            let area = v3::tri_area(a, b, c);
            if area > 1e-9 && uv_area > 1e-12 && uv_area.is_finite() {
                let e = dens.entry(sm.material.as_str()).or_insert((0.0, 0.0));
                e.0 += (uv_area / area).sqrt() * area;
                e.1 += area;
            }
        }
        let mut out = Vec::new();
        for (name, (d, w)) in dens {
            let Some(m) = self.materials.get(name) else { continue };
            if w <= 0.0 || !m.textured {
                continue;
            }
            for tex in m.textures.iter().filter(|t| !t.starts_with("materials/default/")) {
                let Some(ti) = (env.texture)(tex) else { continue };
                let side = ((ti.width as f64) * (ti.height as f64)).sqrt();
                out.push(MipReach {
                    material: name.to_string(),
                    texture: tex.clone(),
                    width: ti.width as u32,
                    height: ti.height as u32,
                    mips: ti.mips as u32,
                    texels_per_metre: d / w * side / 0.0254,
                });
            }
        }
        out
    }

    /// SKIN-FLAWS H1: each LOD0 triangle's centroid against every hitbox of the default set (bind pose), the
    /// distance outside the nearest one, area-weighted. Additive cards are skipped. A hitbox is a capsule (shape 2),
    /// a sphere (1: Viscous's head, Dynamo's body) or a box in its bone's frame (0: Haze's torso, Dynamo's pack).
    pub fn envelope(&self) -> Envelope {
        let w = self.model.skeleton.world();
        let mut caps: Vec<(String, HitShape)> = Vec::new();
        for h in self.model.hitboxes.iter().filter(|h| (0..=2).contains(&h.shape) && (h.set == "default" || h.set.is_empty())) {
            let Some(bi) = self.model.skeleton.index(&h.bone) else { continue };
            let (r, t) = w[bi];
            let rot: [V3; 3] = [0, 1, 2].map(|a| [r[a][0] as f64, r[a][1] as f64, r[a][2] as f64]);
            let org: V3 = [t[0] as f64, t[1] as f64, t[2] as f64];
            let xf = |p: [f32; 3]| -> V3 {
                let q = [p[0] as f64, p[1] as f64, p[2] as f64];
                [v3::dot(rot[0], q) + org[0], v3::dot(rot[1], q) + org[1], v3::dot(rot[2], q) + org[2]]
            };
            let lo: V3 = [0, 1, 2].map(|k| h.min[k].min(h.max[k]) as f64);
            let hi: V3 = [0, 1, 2].map(|k| h.min[k].max(h.max[k]) as f64);
            let shape = match h.shape {
                0 => HitShape::Box { rot, org, lo, hi },
                1 => HitShape::Capsule(xf(h.min), xf(h.min), h.radius as f64),
                _ => HitShape::Capsule(xf(h.min), xf(h.max), h.radius as f64),
            };
            caps.push((h.name.clone(), shape));
        }
        let mut e = Envelope { capsules: caps.len(), ..Default::default() };
        if caps.is_empty() {
            return e;
        }
        let g = &self.geo;
        e.caps = caps.iter().map(|c| (c.0.clone(), f64::NEG_INFINITY, [0.0; 3])).collect();
        e.visible_caps = vec![(f64::NEG_INFINITY, [0.0; 3]); caps.len()];
        for t in 0..g.tris.len() as u32 {
            let m = self.materials.get(&g.surfs[g.tsurf[t as usize] as usize].material);
            if m.is_some_and(|m| m.additive) {
                continue;
            }
            let [a, b, c] = g.tri(t);
            let area = v3::tri_area(a, b, c);
            let p = v3::centroid(a, b, c);
            let (mut best, mut arg) = (f64::INFINITY, 0usize);
            for (k, (_, shape)) in caps.iter().enumerate() {
                let d = shape.distance(p);
                if d < best {
                    best = d;
                    arg = k;
                }
            }
            e.area += area;
            let visible = self.tri_visible.get(t as usize).copied().unwrap_or(true);
            if visible {
                e.visible_area += area;
                if best > e.visible_worst.0 {
                    e.visible_worst = (best, p, caps[arg].0.clone());
                }
                if best > e.visible_caps[arg].0 {
                    e.visible_caps[arg] = (best, p);
                }
            }
            if best > e.caps[arg].1 {
                e.caps[arg].1 = best;
                e.caps[arg].2 = p;
            }
            e.tris.push((best, area, visible, arg as u16));
        }
        let mut dists: Vec<(f64, f64)> = e.tris.iter().map(|t| (t.0, t.1)).collect();
        dists.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        let target = 0.999 * e.area;
        let mut acc = 0.0;
        for (d, a) in &dists {
            acc += a;
            if acc >= target {
                e.p999 = *d;
                break;
            }
        }
        e
    }

    fn inventory(&self) -> (usize, Vec<String>, Vec<(String, String)>) {
        let (mut unreferenced, mut unused_materials, mut untextured) = (0usize, Vec::<String>::new(), Vec::<(String, String)>::new());
        for m in &self.model.meshes {
            if m.lod_mask & 1 == 0 {
                continue;
            }
            // the vertices each buffer really holds: a count the model claims but did not decode sizes nothing
            let held = |v: &mesh::model::VertexBuffer| v.data.as_ref().map_or(0, |d| d.len() / v.stride.max(1));
            let mut used = vec![false; m.vbs.iter().map(held).sum()];
            let offs: Vec<usize> = m.vbs.iter().scan(0, |a, v| {
                let o = *a;
                *a += held(v);
                Some(o)
            }).collect();
            for d in &m.draws {
                if let (Some(ib), Some(&(vbi, bind))) = (m.ibs.get(d.index_buffer), d.vertex_buffers.first()) {
                    if let (Ok(idx), Some(vb)) = (&ib.data, m.vbs.get(vbi)) {
                        let bv = bind as usize / vb.stride.max(1);
                        for &i in idx.iter().skip(d.start_index as usize).take(d.index_count as usize) {
                            let k = offs[vbi] + i as usize + d.base_vertex as usize + bv;
                            if k < used.len() {
                                used[k] = true;
                            }
                        }
                    }
                }
            }
            unreferenced += used.iter().filter(|u| !**u).count();
        }
        let drawn: BTreeSet<String> = self.model.meshes.iter().flat_map(|m| m.draws.iter().map(|d| d.material.clone())).collect();
        if let Some((_, mats)) = self.model.material_groups.first() {
            for mt in mats {
                let l = mt.to_ascii_lowercase();
                if !drawn.contains(&l) && !unused_materials.contains(&l) {
                    unused_materials.push(l);
                }
            }
        }
        for s in &self.geo.surfs {
            if !s.has_uv && self.materials.get(&s.material).is_some_and(|m| m.textured) {
                untextured.push((s.mesh.clone(), s.material.clone()));
            }
        }
        (unreferenced, unused_materials, untextured)
    }

    /// Shells with the same uniform-Laplacian spectrum (same wiring): copies and mirrored pairs.
    fn families(&self) -> Vec<Vec<usize>> {
        let mut sig: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        let mut cache: HashMap<(usize, Vec<(u32, u32)>), Result<Vec<f64>, String>> = HashMap::new();
        for (si, sh) in self.topo.shells.iter().enumerate() {
            if sh.verts > geom::shape::SPECTRUM_VERT_CAP || sh.verts < 4 {
                continue;
            }
            let mut ids: HashMap<u32, u32> = HashMap::new();
            let mut edges = Vec::new();
            for &t in &sh.tris {
                let tr = self.topo.tris[t as usize];
                for k in 0..3 {
                    let n = ids.len() as u32;
                    let a = *ids.entry(tr[k]).or_insert(n);
                    let n = ids.len() as u32;
                    let b = *ids.entry(tr[(k + 1) % 3]).or_insert(n);
                    edges.push((a, b));
                }
            }
            // scene3d caches the spectrum by exact wiring: copies share it, and the solve is the
            // census's most expensive fact
            let mut key_edges: Vec<(u32, u32)> = edges.iter().map(|&(a, b)| if a < b { (a, b) } else { (b, a) }).collect();
            key_edges.sort();
            key_edges.dedup();
            let key = (ids.len(), key_edges);
            let spec = match cache.get(&key) {
                Some(s) => s.clone(),
                None => {
                    let s = geom::shape::spectrum(ids.len(), &edges);
                    cache.insert(key, s.clone());
                    s
                }
            };
            if let Ok(spec) = spec {
                sig.entry(format!("{}:{:?}", ids.len(), spec)).or_default().push(si);
            }
        }
        sig.into_values().filter(|v| v.len() > 1).collect()
    }
}

/// Decoded GPU memory of a texture (all mips; block-compressed formats at their block rate).
pub fn tex_bytes(t: &resource::texture::TexInfo) -> u64 {
    let bpp_x8: u64 = match t.format {
        1 | 27 => 4,      // DXT1, ATI1N: 4 bits
        2 | 19 | 20 | 21 => 8, // DXT5, BC6H, BC7, ATI2N: 8 bits
        28 | 4 => 32,
        _ => 32,
    };
    let mut total = 0u64;
    let (mut w, mut h) = (t.width as u64, t.height as u64);
    for _ in 0..t.mips.max(1) {
        let (bw, bh) = if t.is_block_compressed() { (w.div_ceil(4) * 4, h.div_ceil(4) * 4) } else { (w, h) };
        total += bw * bh * t.depth.max(1) as u64 * bpp_x8 / 8;
        w = (w / 2).max(1);
        h = (h / 2).max(1);
    }
    total
}
