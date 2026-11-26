//! Judgement (scene3d's "judge" stratum): the mesh census of our model against Valve's at the same
//! path. Every threshold is a named constant with its reason; every comparison names Valve's own
//! figure beside ours, and a finding Valve's model shares at the same place is not ours to fix.
//! `SKIN-FLAWS` codes (A1, H1, E2...) name the entry in the author's catalogue of skin flaws, kept with the skins,
//! that each threshold was measured for.

use super::census::{Census, Envelope, Hole, MatInfo};
use crate::policy::FairPlay;
use super::views;
use geom::v3::{self, V3};

pub type Out = Vec<(&'static str, String)>;

/// Our figure may exceed Valve's by this factor before a budget rule speaks (SKIN-FLAWS E2/E3).
pub const BUDGET_RATIO: f64 = 1.5;
/// And by at least this much in absolute terms (a hero is 100k+ triangles; a few thousand is noise).
pub const BUDGET_MIN_TRIS: usize = 5000;
pub const BUDGET_MIN_DRAWS: usize = 4;
/// Fair play (SKIN-FLAWS B2): our in-game silhouette may not be smaller than Valve's by more than
/// this in any named view (pixel noise at 6 mm pixels is well under 1%).
pub const SILHOUETTE_MIN: f64 = 0.97;
/// A cut-body opening is a void when this share of its probes sees through it (cover ignored).
pub const VOID_UNBACKED_SHARE: f64 = 0.25;
/// A cut edge left unbacked under a garment rim along more than this many units shows in motion.
pub const VOID_RISK_LENGTH: f64 = 2.0;
/// Valve's own opening matches ours when its centre is this close and it is at least half as big.
pub const MATCH_DIST: f64 = 8.0;
/// A piece hovers visibly when the gap to its nearest surface is a fair-play pixel (`views::VIEW_UNIT`, 6 mm) or
/// more, and it has a pixel's area to show. SKIN-FLAWS A7 first drew the line at 3 mm; of the 141 pieces past 3 mm on 30 of
/// Valve's heroes (6871), 108 stand under 6 mm off what they sit on, a gap no camera in the game resolves.
pub const FLOAT_GAP: f64 = views::VIEW_UNIT;
pub const FLOAT_MIN_AREA: f64 = views::VIEW_UNIT * views::VIEW_UNIT;
/// A crossing counts once one layer sits this deep in the other (1 mm).
pub const CROSS_DEPTH: f64 = 0.04;
/// M109 and M110: the most visible fold line (a shell through itself) and crossing line (two shells, over 1 mm deep),
/// counting sites over 1 unit long, that any of Valve's 43 playable heroes has at bind (6871), in units of the model's
/// own size (the square root of its surface, so a prop a particle scales down is judged as its hero would be):
/// Frank's folds (4.811) and Hornet's crossings (52.46), each rounded up so that hero is not over its own line. The
/// median hero has 0.4 and 18.8. A model past these crosses more than Valve builds any hero.
pub const FOLD_RATE_VALVE: f64 = 4.82;
pub const CROSS_RATE_VALVE: f64 = 52.5;
/// Coplanar overlap worth a flicker (SKIN-FLAWS A6: more than 1 cm^2).
pub const ZFIGHT_AREA: f64 = 0.155;
/// H1 policy (the author, 2026-11-19): "some overflow where it's tasteful, not 'ok that's a bit much lol'".
/// Three tiers, all in this block:
///   M130 (warn): more than `HITBOX_WARN_SHARE` of the surface beyond `HITBOX_WARN_UNITS`.
///   M140 (warn, the taste tier): any visible surface beyond `HITBOX_TASTE_UNITS`; accept it with a written reason.
///   M129 (error, "a bit much"): the worst visible point beyond `HITBOX_EGREGIOUS_UNITS`, or more than
///   `HITBOX_EGREGIOUS_SHARE` of the visible surface beyond `HITBOX_TASTE_UNITS`. Never allowable.
/// 10 units is a taste line, not a wall. `HITBOX_NOISE_AREA` (u2) keeps a stray sliver from naming a region.
/// These and the fair-play limits below are the defaults of `crate::policy::FairPlay`: a policy file may move them,
/// a run with none judges by these.
pub const HITBOX_TASTE_UNITS: f64 = 10.0;
pub const HITBOX_WARN_UNITS: f64 = 5.0;
pub const HITBOX_WARN_SHARE: f64 = 0.02;
pub const HITBOX_EGREGIOUS_UNITS: f64 = 18.0;
pub const HITBOX_EGREGIOUS_SHARE: f64 = 0.035;   // the author, 2026-11-19: "3-3.5% there. boom." (Maid Lash's bell sits at 3.1%)
pub const HITBOX_NOISE_AREA: f64 = 0.1;
/// Fair play outside the mesh family (the rules in `deep.rs` read these through the policy in force).
/// M081: a replaced particle may be this many times Valve's energy (colour x alpha x overbright), or sprite radius.
pub const PARTICLE_ENERGY_RATIO: f64 = 1.25;
pub const PARTICLE_RADIUS_RATIO: f64 = 1.25;
/// M082: a particle light may exceed Valve's lumens and range by 5%, its colour's luminous output by 10%.
pub const LIGHT_LUMENS_RATIO: f64 = 1.05;
pub const LIGHT_OUTPUT_RATIO: f64 = 1.10;
pub const LIGHT_RANGE_RATIO: f64 = 1.05;
/// M063: a replaced sound event may play 0.05 dB quieter than Valve's (rounding), a sound be 5% shorter, and a sound
/// file's envelope peak sit 1.5 dB lower, before the cue counts as hidden.
pub const SOUND_VOLUME_SLACK_DB: f64 = 0.05;
pub const SOUND_DURATION_RATIO: f64 = 0.95;
pub const SOUND_PEAK_SLACK_DB: f64 = 1.5;
/// M064 (the mix, not fair play): louder by more than 0.05 dB, 10% longer, or a peak 1.5 dB higher than Valve's.
pub const SOUND_LOUDER_SLACK_DB: f64 = 0.05;
pub const SOUND_LONGER_RATIO: f64 = 1.10;
pub const SOUND_PEAK_LOUDER_DB: f64 = 1.5;
/// SKIN-FLAWS A1: share of our area outside Valve's range that warns; absolute black and white.
pub const ALBEDO_OUTSIDE: f64 = 0.10;
pub const ALBEDO_BLACK: f64 = 0.003;
pub const ALBEDO_WHITE: f64 = 0.80;
pub const ALBEDO_ABSOLUTE: f64 = 0.05;
/// SKIN-FLAWS C5: share of mixed tiles whose coverage may move by more than 30%.
pub const COVERAGE_SHARE: f64 = 0.10;

/// M146: a material's triangle count may rise from one LOD to the next by this share (and this many triangles)
/// before it counts as an inversion. Valve's own decimations rise by up to 0.2% and 24 triangles (Ratking's weapon,
/// Baba's pigeon): re-triangulated borders, not detail.
pub const LOD_INVERSION_SHARE: f64 = 0.01;
pub const LOD_INVERSION_MIN: usize = 32;
/// M146: a lower LOD may reach this far (units) outside LOD0's bounding box: two pixels of the fair-play
/// silhouette (`views::VIEW_UNIT`, 6 mm). Valve's worst is 0.12 units (McGinnis).
pub const LOD_SPIKE_UNITS: f64 = 2.0 * views::VIEW_UNIT;
/// M147: the distance (metres) a hero's textures are judged at, the far line of `Coverage`, and the texels of the
/// smallest stored mip that may fall on one pixel there. One texel a pixel is the sampling limit a full mip chain
/// keeps; past two the sampler skips every other texel (an octave past Nyquist), which is where minification shimmer
/// is plain.
pub const MIP_REACH_METRES: f64 = 8.0;
pub const MIP_TEXELS_PER_PIXEL: f64 = 2.0;

/// Area-weighted percentile of (value, weight) samples.
pub fn weighted_pct(s: &mut [(f64, f64)], q: f64) -> f64 {
    s.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let total: f64 = s.iter().map(|x| x.1).sum();
    let mut acc = 0.0;
    for (v, w) in s.iter() {
        acc += w;
        if acc >= q * total {
            return *v;
        }
    }
    s.last().map_or(0.0, |x| x.0)
}

/// float32 keeps 8 quanta across a part's smallest dimension (scene3d coordinate-precision).
pub const MIN_QUANTA: f64 = 8.0;
/// M112 speaks only where float32's step is a visible fraction of the fair-play pixel (`views::VIEW_UNIT`): an
/// eighth of it, 0.03 units, which a coordinate reaches at about 260,000 units from the origin.
pub const PRECISION_VISIBLE_STEP: f64 = views::VIEW_UNIT / MIN_QUANTA;

fn f1(x: f64) -> String {
    format!("{x:.1}")
}

fn at(p: V3) -> String {
    format!("({:.0}, {:.0}, {:.0})", p[0], p[1], p[2])
}

/// Does Valve's census carry an opening like `h` (same place, comparable size)?
fn valve_has_hole(valve: Option<&Census>, h: &Hole, pred: &dyn Fn(&Hole) -> bool) -> bool {
    valve.is_some_and(|v| v.holes.iter().any(|g| pred(g) && v3::dist(g.centroid, h.centroid) < MATCH_DIST && g.area >= 0.5 * h.area))
}

/// A shell of ours that is Valve's own piece, unchanged: every material Valve's, and Valve's model
/// has a shell of the same triangle count within a unit of it.
pub fn valve_own(o: &Census, valve: Option<&Census>, i: usize) -> bool {
    let Some(v) = valve else { return false };
    let s = &o.shells[i];
    s.materials.iter().all(|m| v.materials.contains_key(m)) && v.shells.iter().any(|t| t.tris == s.tris && v3::dist(t.bounds.center(), s.bounds.center()) < 1.0)
}

/// The same pieces in Valve's model (every material Valve's) crossing at the same place: Valve's design.
fn valve_cross(o: &Census, valve: Option<&Census>, c: &super::census::Cross) -> bool {
    let mats = |i: usize| &o.shells[i].materials;
    valve.is_some_and(|v| mats(c.a).iter().chain(mats(c.b)).all(|m| v.materials.contains_key(m)) && v.crosses.iter().any(|x| v3::dist(x.bounds.center(), c.bounds.center()) < MATCH_DIST))
}

/// A model's size for M109 and M110: the square root of its surface.
pub fn model_size(o: &Census) -> f64 {
    o.shells.iter().map(|s| s.area).sum::<f64>().sqrt().max(1e-9)
}

/// M109's sites: visible folds of a shell through itself over 1 unit long that are not Valve's own, longest first.
pub fn folds<'a>(o: &'a Census, valve: Option<&Census>) -> Vec<&'a super::census::Cross> {
    let mut v: Vec<_> = o.crosses.iter().filter(|c| c.a == c.b && c.visible_pairs > 0 && c.length > 1.0 && !valve_own(o, valve, c.a) && !valve_cross(o, valve, c)).collect();
    v.sort_by(|a, b| b.length.partial_cmp(&a.length).unwrap_or(std::cmp::Ordering::Equal));
    v
}

/// M110's sites: visible crossings of two shells over 1 unit long and 1 mm deep that are not Valve's own, longest
/// first.
pub fn crossings<'a>(o: &'a Census, valve: Option<&Census>) -> Vec<&'a super::census::Cross> {
    let mut v: Vec<_> = o
        .crosses
        .iter()
        .filter(|c| c.a != c.b && c.visible_pairs > 0 && c.length > 1.0 && c.depth_a_in_b.max(c.depth_b_in_a) > CROSS_DEPTH)
        .filter(|c| !(valve_cross(o, valve, c) || valve_own(o, valve, c.a) && valve_own(o, valve, c.b)))
        .collect();
    v.sort_by(|a, b| b.length.partial_cmp(&a.length).unwrap_or(std::cmp::Ordering::Equal));
    v
}

/// Mismatch with the substrate above which a resting piece separates in motion.
pub const SKIN_MISMATCH: f64 = 0.5;
/// A piece and its substrate on bones a joint connects, resting on each other this close (units) to that joint's
/// pivot, meet at a hinge: the bend turns one about the point where they touch, so it slides there and does not lift
/// off (a hand at its sleeve's cuff, a foot at its shin).
pub const HINGE_REACH: f64 = 3.0;

/// The distance from `p` to the nearest pivot between bones `a` and `b`: the origin of every bone on the chain from
/// each up to the bone they share (that one's own origin turns both alike). None when the skeleton does not hold both.
pub fn hinge_distance(sk: &::mesh::model::Skeleton, a: &str, b: &str, p: V3) -> Option<f64> {
    let (ia, ib) = (sk.index(a)?, sk.index(b)?);
    let chain = |mut i: usize| {
        let mut c = vec![i];
        while let Some(&q) = sk.parents.get(i) {
            if q < 0 || q as usize >= sk.names.len() || c.len() > sk.names.len() {
                break;
            }
            i = q as usize;
            c.push(i);
        }
        c
    };
    let (ca, cb) = (chain(ia), chain(ib));
    let shared = ca.iter().position(|x| cb.contains(x))?;
    let top = ca[shared];
    let world = sk.world();
    ca.iter().take(shared).chain(cb.iter().take_while(|&&x| x != top))
        .map(|&i| { let t = world[i].1; v3::dist(p, [t[0] as f64, t[1] as f64, t[2] as f64]) })
        .fold(None, |m: Option<f64>, d| Some(m.map_or(d, |m| m.min(d))))
}

/// The cut-body openings M106 reports: skin openings of at least 1 u2 that see through (cover
/// ignored) or are visible now, and that Valve's model does not share at the same place.
pub fn cut_body_voids<'a>(o: &'a Census, valve: Option<&Census>) -> Vec<&'a Hole> {
    o.holes
        .iter()
        .filter(|h| h.skin && !h.sheet && h.area >= 1.0 && (h.void_exposed > 0 || h.risk_length > VOID_RISK_LENGTH))
        .filter(|h| !valve_has_hole(valve, h, &|g| g.skin && (g.void_exposed > 0 || g.risk_length > VOID_RISK_LENGTH)))
        .collect()
}

pub fn judge(ours: &Census, valve: Option<&Census>, fp: &FairPlay) -> Out {
    let mut out: Out = Vec::new();
    let o = ours;
    let own = |i: usize| valve_own(o, valve, i);
    // M100: what was not measured is said, never passed.
    for n in o.notes.iter().take(6) {
        out.push(("M100", format!("not measured: {n}")));
    }
    if o.pairs_capped {
        out.push(("M100", format!("the triangle-pair search passed its {} cap: intersection and z-fighting are unchecked past it", super::census::PAIR_CAP)));
    }
    if o.nonfinite > 0 {
        out.push(("M101", format!("{} vertices have non-finite positions: they stretch to infinity on screen", o.nonfinite)));
        out.push(("M144", "the geometry rules were skipped (non-finite positions), so the silhouette and hitbox envelope were not judged (M124, M129, M140)".into()));
        return out;
    }
    if o.geo.tris.is_empty() {
        out.push(("M100", "LOD0 has no decodable triangles: no mesh rule ran".into()));
        out.push(("M144", "LOD0 has no decodable triangles, so the silhouette and hitbox envelope were not judged (M124, M129, M140)".into()));
        return out;
    }
    let vs = |c: &Census| c.shells.len();
    let lbl = |i: usize| o.shells[i].label.clone();

    // M102 open sheets you can see the culled back of (A2), and M106 cut-body voids (A1).
    let uncapped: Vec<&Hole> = o.holes.iter().filter(|h| h.cap_failed && !own(h.shell)).collect();
    if let Some(h) = uncapped.first() {
        out.push((
            "M100",
            format!(
                "not measured: {} openings fold over in every projection and could not be capped for the see-through probe (first [{}], {} u2 at {}); their parts that could be capped were probed",
                uncapped.len(),
                lbl(h.shell),
                f1(h.area),
                at(h.centroid)
            ),
        ));
    }
    for h in o.holes.iter().filter(|h| !h.skin && h.void_exposed >= 4 && h.exposed_share() >= 0.02) {
        if own(h.shell) || valve_has_hole(valve, h, &|g| !g.skin && g.void_exposed > 0) {
            continue;
        }
        out.push((
            "M102",
            format!(
                "[{}] opening of {} u2 ({}, {} side, at {}) shows its culled inside from {} of {} outside views: see-through in game",
                lbl(h.shell),
                f1(h.area),
                h.bone,
                h.side,
                at(h.centroid),
                h.void_exposed,
                h.probes
            ),
        ));
    }
    for h in cut_body_voids(o, valve) {
        let now = if h.void_exposed > 0 { format!("visible now from {} views (most near {})", h.void_exposed, at(h.now_at)) } else { "covered at bind, so it shows when the garment lifts".into() };
        out.push((
            "M106",
            format!(
                "[{}] cut-body opening of {} u2 ({}, {} side, at {}): {} of its {} units of edge run within {} units of a garment rim with nothing behind them (worst near {}; {:.0}% of the outside views that reach the cut once the garment lifts see nothing drawn): {now}. Back it with a dark liner shell there or keep the faces",
                lbl(h.shell),
                f1(h.area),
                h.bone,
                h.side,
                at(h.centroid),
                f1(h.risk_length),
                f1(h.edge_length),
                super::census::RIM_MARGIN,
                at(h.risk_at),
                100.0 * h.unbacked_share()
            ),
        ));
    }
    // M103 winding, M104 flipped normals, M105 degenerate and slivers, against Valve's rates.
    let tot = |c: &Census, f: &dyn Fn(usize) -> usize| (0..vs(c)).map(f).sum::<usize>();
    let rate = |n: usize, c: &Census| n as f64 / c.geo.tris.len().max(1) as f64;
    let wo = tot(o, &|i| o.topo.shells[i].winding_conflicts);
    let wv = valve.map(|v| tot(v, &|i| v.topo.shells[i].winding_conflicts));
    if wo > 10 && wv.is_none_or(|w| rate(wo, o) > BUDGET_RATIO * rate(w, valve.unwrap())) {
        let worst = (0..vs(o)).max_by_key(|&i| o.topo.shells[i].winding_conflicts).unwrap();
        out.push(("M103", format!("{wo} edges where neighbouring faces disagree on winding (Valve's: {}), worst [{}] with {}: one side lights inside-out or is culled", wv.map_or("-".into(), |w| w.to_string()), lbl(worst), o.topo.shells[worst].winding_conflicts)));
    }
    let fo = tot(o, &|i| o.shells[i].flipped_visible);
    let fv = valve.map(|v| tot(v, &|i| v.shells[i].flipped_visible));
    if fo > 20 && fv.is_none_or(|f| rate(fo, o) > BUDGET_RATIO * rate(f, valve.unwrap())) {
        let worst = (0..vs(o)).max_by_key(|&i| o.shells[i].flipped_visible).unwrap();
        out.push(("M104", format!("{fo} visible triangles whose shading normals point against their face (Valve's: {}), worst [{}] with {}: they shade dark or flat, lit from behind", fv.map_or("-".into(), |f| f.to_string()), lbl(worst), o.shells[worst].flipped_visible)));
    }
    let qo = tot(o, &|i| o.shells[i].zero_area + o.shells[i].slivers);
    let qv = valve.map(|v| tot(v, &|i| v.shells[i].zero_area + v.shells[i].slivers));
    if qo > 50 && qv.is_none_or(|q| rate(qo, o) > 2.0 * rate(q, valve.unwrap())) {
        let worst = (0..vs(o)).max_by_key(|&i| o.shells[i].zero_area + o.shells[i].slivers).unwrap();
        out.push((
            "M105",
            format!(
                "{qo} zero-area or sliver (aspect over 20) triangles, {:.2}% of LOD0 (Valve's: {}), worst [{}]: slivers shade and skin badly, zero-area ones flicker",
                100.0 * rate(qo, o),
                qv.map_or("-".into(), |q| format!("{:.2}%", 100.0 * rate(q, valve.unwrap()))),
                lbl(worst)
            ),
        ));
    }
    // M145 a closed piece wound inside out. The sign of a closed shell's signed volume is exact (`geom::mass`), so
    // this is a fact, not a threshold: every face of it points into the piece, the game culls them from outside,
    // and winding and normals agree with each other, so M103 and M104 see nothing wrong.
    for (i, s) in o.shells.iter().enumerate() {
        let single = s.materials.iter().all(|m| o.materials.get(m).is_some_and(|x| x.found && !(x.two_sided || x.additive || x.translucent)));
        // a closed piece with no room inside (two coincident layers welded at the rim) has no inside to turn out:
        // its mean thickness, twice its volume over its area, must clear the depth a crossing shows at
        let thick = s.volume.map_or(0.0, |v| 2.0 * v.abs() / s.area.max(1e-12));
        if s.winding >= 0 || !single || own(i) || o.topo.shells[i].winding_conflicts > 0 || thick <= CROSS_DEPTH {
            continue;
        }
        if valve.is_some_and(|v| v.shells.iter().any(|t| t.winding < 0 && v3::dist(t.bounds.center(), s.bounds.center()) < MATCH_DIST)) {
            continue; // Valve's model has an inside-out piece at the same place
        }
        // A shell that holds another piece of the model inside its box is the inverted-hull outline: the author meant
        // a line around what it wraps, and the game draws a solid shape over it instead.
        let wraps: Vec<usize> = (0..o.shells.len()).filter(|&j| j != i && inside(&o.shells[j].bounds, &s.bounds)).collect();
        let (what, fix) = match wraps.first() {
            Some(&j) => (
                format!(
                    ", and it wraps [{}]{}: an inverted-hull outline. The game does not cull it into a line: it draws an opaque black shape over what it wraps (seen in game on hero materials), while Blender and Source 2 Viewer draw back faces and show the line, so neither preview can be trusted for it",
                    lbl(j),
                    if wraps.len() > 1 { format!(" and {} other piece(s)", wraps.len() - 1) } else { String::new() }
                ),
                "Remove the shell and draw the line with the material's own outline: F_SOLID_COLOR_OUTLINE (Valve's hero materials set it), coloured by g_vSolidOutlineTint, and for a heavier line F_OVERRIDE_NPR_OUTLINE with g_flOverrideNprOutlineThickness",
            ),
            None => (
                ". The game culls them from outside and draws the far inner wall instead, lit from within: the piece reads hollow or flat black (Blender and Source 2 Viewer draw back faces, so they show it right)".to_string(),
                "Flip its faces (and normals); a lining meant to be seen from inside is an open sheet, not a closed piece, and an outline is the material's own (F_SOLID_COLOR_OUTLINE), never a shell",
            ),
        };
        out.push((
            "M145",
            format!(
                "[{}] is a closed piece wound inside out: its signed volume is {} u3 (a closed piece seen from outside encloses a positive one; {thick:.2} units thick on average), so all {} of its faces point inward{what}. {fix}",
                lbl(i),
                s.volume.map_or("-".into(), |v| format!("{:.1}", -v.abs())),
                s.tris
            ),
        ));
    }
    // M147 a texture whose mip chain stops before the size the game draws it at.
    for r in &o.mip_reach {
        if valve.is_some_and(|v| v.textures.contains(&r.texture) || v.materials.contains_key(&r.material)) {
            continue; // Valve's own texture or material
        }
        let per_pixel = r.texels_per_pixel(MIP_REACH_METRES);
        if per_pixel > MIP_TEXELS_PER_PIXEL {
            let full = 32 - r.width.max(r.height).max(1).leading_zeros();
            out.push((
                "M147",
                format!(
                    "{}: {} ({}x{}) stores {} of {} mips; it lies on the model at {:.0} texels per metre, so at {MIP_REACH_METRES} m its smallest mip still puts {:.1} texels on each pixel (limit {MIP_TEXELS_PER_PIXEL}): the sampler skips texels and the surface shimmers and crawls as the hero or the camera moves. Compile it with a full mip chain",
                    r.material,
                    r.texture,
                    r.width,
                    r.height,
                    r.mips,
                    full,
                    r.texels_per_metre,
                    per_pixel
                ),
            ));
        }
    }
    // M107 two-sided back faces seen from outside.
    let bo: f64 = o.backshade.iter().map(|r| r.area).sum();
    let bv: f64 = valve.map_or(0.0, |v| v.backshade.iter().map(|r| r.area).sum());
    if bo > 2.0 * bv + 2.0 {
        let r = &o.backshade[0];
        out.push(("M107", format!("{} u2 of two-sided back faces seen first from outside (Valve's {}), largest [{}] {}, {} side: their normals face away, so they shade flat", f1(bo), f1(bv), lbl(r.shell), r.bone, r.side)));
    }
    // M108 z-fighting between different shells. Two of Valve's own pieces sharing a plane where Valve's model has
    // them sharing one is Valve's design (Lash's gear plates, 20 of Valve's heroes): not ours to fix.
    let valve_zfight = |z: &super::census::ZFight| -> bool {
        let mats = |i: usize| &o.shells[i].materials;
        (own(z.a) && own(z.b))
            || valve.is_some_and(|v| mats(z.a).iter().chain(mats(z.b)).all(|m| v.materials.contains_key(m)) && v.zfights.iter().any(|x| x.a != x.b && v3::dist(x.worst_at, z.worst_at) < MATCH_DIST))
    };
    for z in o.zfights.iter().filter(|z| z.a != z.b && z.area >= ZFIGHT_AREA && !valve_zfight(z)) {
        out.push(("M108", format!("[{}] and [{}] share a plane over {} u2 near {}: they flicker", lbl(z.a), lbl(z.b), f1(z.area), at(z.worst_at))));
    }
    // M109 self-intersection and M110 shell crossings, visible ones, with depth.
    // Valve builds every hero from pieces that pass through each other at bind: on 6871 each of the 43 playable
    // heroes has visible crossings 5 units deep or more (about 250 shell pairs a hero), with the same spread of
    // crossing length as a skin's. One crossing cannot tell a clip from Valve's construction, so M109 and M110 judge
    // how much of the model is crossing: the visible crossing line in units of the model's size (the square root of
    // its surface), against the most any of Valve's heroes has. Valve's own pieces where Valve's model crosses there
    // too are not counted.
    let per = |len: f64| len / model_size(o);
    let folds = folds(o, valve);
    let fold_len: f64 = folds.iter().map(|c| c.length).sum();
    if per(fold_len) > FOLD_RATE_VALVE {
        let worst: Vec<String> = folds.iter().take(3).map(|c| format!("[{}] {} units near {}", lbl(c.a), f1(c.length), at(c.bounds.center()))).collect();
        out.push((
            "M109",
            format!(
                "{} shells fold through themselves: {} units of visible fold line, {:.1} times the model's size (the square root of its surface), more than any of Valve's heroes ({FOLD_RATE_VALVE} at most): the layers flicker along the folds. The most: {}",
                folds.len(),
                f1(fold_len),
                per(fold_len),
                worst.join("; ")
            ),
        ));
    }
    let crossing = crossings(o, valve);
    let cross_len: f64 = crossing.iter().map(|c| c.length).sum();
    if per(cross_len) > CROSS_RATE_VALVE {
        let worst: Vec<String> = crossing
            .iter()
            .take(3)
            .map(|c| {
                let (inner, outer, d) = if c.depth_a_in_b >= c.depth_b_in_a { (c.a, c.b, c.depth_a_in_b) } else { (c.b, c.a, c.depth_b_in_a) };
                format!("[{}] {} units into [{}] along {} units, z {:.0}..{:.0}", lbl(inner), if d >= 4.99 { "5 or more".to_string() } else { format!("{d:.2}") }, lbl(outer), f1(c.length), c.bounds.min[2], c.bounds.max[2])
            })
            .collect();
        out.push((
            "M110",
            format!(
                "{} shell pairs pass through each other at bind: {} units of visible crossing line, {:.1} times the model's size (the square root of its surface), more than any of Valve's heroes ({CROSS_RATE_VALVE} at most): clips that show along the lines. The longest: {}",
                crossing.len(),
                f1(cross_len),
                per(cross_len),
                worst.join("; ")
            ),
        ));
    }
    // M111 floating shells: no contact chain to the hero's skin.
    let roots: Vec<usize> = (0..vs(o)).filter(|&i| o.shells[i].materials.iter().any(|m| o.skin_materials.contains(m))).collect();
    if !roots.is_empty() {
        let mut grounded = vec![false; vs(o)];
        let mut q = roots.clone();
        roots.iter().for_each(|&r| grounded[r] = true);
        while let Some(s) = q.pop() {
            for &t in o.touching.get(s).map(Vec::as_slice).unwrap_or(&[]) {
                if !grounded[t] {
                    grounded[t] = true;
                    q.push(t);
                }
            }
        }
        let total: f64 = o.shells.iter().map(|s| s.area).sum();
        let mut floats: Vec<usize> = (0..vs(o)).filter(|&i| !grounded[i] && o.contacts[i].gap > FLOAT_GAP && o.shells[i].area >= FLOAT_MIN_AREA && o.shells[i].area < 0.05 * total && !own(i)).collect();
        floats.sort_by(|&a, &b| o.contacts[b].gap.partial_cmp(&o.contacts[a].gap).unwrap_or(std::cmp::Ordering::Equal));
        for &i in floats.iter().take(10) {
            let k = &o.contacts[i];
            out.push((
                "M111",
                format!("[{}] touches nothing: {:.2} units from its nearest surface [{}]: it hovers with a visible gap and drifts in motion. Snap it to the surface and weight it from what it rests on", lbl(i), k.gap, k.nearest.map(lbl).unwrap_or_default()),
            ));
        }
    }
    // M111, second cause: resting on a substrate but skinned to other bones.
    if !o.contacts.is_empty() {
        let total: f64 = o.shells.iter().map(|s| s.area).sum();
        let mut drift: Vec<usize> = (0..vs(o))
            .filter(|&i| o.contacts[i].skin_mismatch > SKIN_MISMATCH && o.contacts[i].substrate.is_some() && o.shells[i].area < 0.05 * total && o.shells[i].area > 0.5 && !own(i))
            // Pieces on bones Valve's skeleton lacks are our cloth chains, which move on their own by
            // design; a piece on a bone Valve also hangs a separately moving piece on (a rotating
            // ring) is Valve's design.
            .filter(|&i| {
                let b = &o.shells[i].bone;
                let sub = &o.shells[o.contacts[i].substrate.unwrap()].bone;
                // a piece that meets its substrate at the joint between their bones is articulated, not loose;
                // one on a bone that moves it alone (a ring on its own ring bone, a pump on its pump) is a part the
                // rig moves on purpose, not a piece left on the wrong bones
                let hinge = hinge_distance(&o.model.skeleton, b, sub, o.contacts[i].rest_at).is_some_and(|d| d <= HINGE_REACH);
                let alone = o.shells.iter().filter(|t| &t.bone == b).count() == 1;
                !hinge && !alone && valve.is_none_or(|v| {
                    v.model.skeleton.index(b).is_some()
                        && v.model.skeleton.index(sub).is_some()
                        && !v.shells.iter().zip(&v.contacts).any(|(t, k)| &t.bone == b && k.skin_mismatch > SKIN_MISMATCH)
                })
            })
            .collect();
        drift.sort_by(|&a, &b| (o.contacts[b].skin_mismatch * o.shells[b].area).partial_cmp(&(o.contacts[a].skin_mismatch * o.shells[a].area)).unwrap_or(std::cmp::Ordering::Equal));
        for &i in drift.iter().take(10) {
            let k = &o.contacts[i];
            let sub = k.substrate.unwrap();
            out.push((
                "M111",
                format!("[{}] rests on [{}] but is skinned to other bones ({:.0}% of its weight differs from the surface under it): it lifts off or sinks in as soon as those bones move apart. Weight it from its substrate (skin by position)", lbl(i), lbl(sub), 100.0 * k.skin_mismatch),
            ));
        }
    }
    // M112 coordinate precision: a piece far enough from the origin that float32 snaps its vertices by a visible
    // amount (an eighth of a fair-play pixel or more), with a dimension held in fewer than eight of those steps. The
    // step is the piece's own (its farthest corner), not the model's. Nearer in, the snap is far under a pixel: a
    // hero's reach of 100 units steps by 1.2e-5 units, and a flat card's few quanta of thickness are float32
    // rounding a plane, not a shape being lost (18 such cards on one skin, 2 on Valve's heroes, 6871).
    for (i, s) in o.shells.iter().enumerate() {
        let far = s.bounds.min.iter().chain(s.bounds.max.iter()).fold(0.0f64, |m, x| m.max(x.abs()));
        let step = (far * 2f64.powi(-23)).max(f64::MIN_POSITIVE);
        if step < PRECISION_VISIBLE_STEP {
            continue;
        }
        let d = s.bounds.size();
        // a flat card (thinner than one quantum) is flat by design; only a dimension that exists can be lost
        let thin = d.iter().cloned().filter(|x| *x >= step && *x < step * MIN_QUANTA).fold(f64::INFINITY, f64::min);
        if thin.is_finite() {
            out.push(("M112", format!("[{}] is {thin:.2e} units thin at {} units from the origin, where float32 steps are {step:.2e} (an eighth of a pixel is {:.2e}): it cannot hold its shape", lbl(i), f1(far), PRECISION_VISIBLE_STEP)));
        }
    }
    // M113-M115 UVs, against Valve's worst material.
    let vmax = |f: &dyn Fn(&super::census::UvFacts) -> f64| valve.map(|v| v.uv.iter().map(f).fold(0.0, f64::max));
    let vdeg = vmax(&|u| u.degenerate as f64 / u.tris.max(1) as f64);
    let vover = vmax(&|u| u.overlap.max(u.self_overlap));
    let vstretch = vmax(&|u| u.stretch_over);
    for u in &o.uv {
        let mat = o.materials.get(&u.material).cloned().unwrap_or_default();
        if !mat.textured {
            continue;
        }
        let dr = u.degenerate as f64 / u.tris.max(1) as f64;
        if u.degenerate >= 10 && dr > vdeg.unwrap_or(0.0) * 2.0 + 0.001 {
            out.push(("M113", format!("{}: {} textured triangles with zero UV area inside real islands ({:.2}%, Valve's worst {}): one texel smears over each", u.material, u.degenerate, 100.0 * dr, vdeg.map_or("-".into(), |x| format!("{:.2}%", 100.0 * x)))));
        }
        let ov = u.overlap.max(u.self_overlap);
        if ov > 0.25 && ov > vover.unwrap_or(0.0) + 0.1 {
            out.push(("M114", format!("{}: {:.0}% of its texels are claimed twice (between islands {:.0}%, within one {:.0}%; Valve's worst {}): shared texels show the same paint in two places", u.material, 100.0 * ov, 100.0 * u.overlap, 100.0 * u.self_overlap, vover.map_or("-".into(), |x| format!("{:.0}%", 100.0 * x)))));
        }
        if u.stretch_over > 0.1 && u.stretch_over > vstretch.unwrap_or(0.0) + 0.05 {
            out.push(("M115", format!("{}: {:.0}% of its area is stretched over 4:1 in UV (mean {:.2}; Valve's worst {}): the texture smears along the stretch", u.material, 100.0 * u.stretch_over, u.stretch_mean, vstretch.map_or("-".into(), |x| format!("{:.0}%", 100.0 * x)))));
        }
    }
    // M116-M119 skin weights.
    let s = &o.skin;
    if s.unnormalised > 0 {
        out.push(("M116", format!("{} vertices' weights do not sum to 1 (worst {:.3}): they shrink toward the model origin as the hero moves", s.unnormalised, s.worst_sum)));
    }
    let vtiny = valve.map(|v| v.skin.tiny as f64 / v.skin.vertices.max(1) as f64);
    if s.over_slots > 0 || (s.tiny > 100 && s.tiny as f64 / s.vertices.max(1) as f64 > 2.0 * vtiny.unwrap_or(0.0) + 0.01) {
        out.push(("M117", format!("{} vertices carry more influences than their mesh's {} slots, {} influences are under 1.5/255 (Valve's rate {}): dropped or noise influences jitter", s.over_slots, o.model.meshes.first().map_or(4, |m| m.bone_weight_count), s.tiny, vtiny.map_or("-".into(), |x| format!("{:.2}%", 100.0 * x)))));
    }
    if let Some(v) = valve {
        // Valve's reach per bone, by name.
        let vreach: std::collections::HashMap<&str, f64> = v.skin.reach.iter().map(|(b, d)| (v.model.skeleton.names[*b as usize].as_str(), *d)).collect();
        let mut far: std::collections::BTreeMap<String, (usize, f64, V3)> = std::collections::BTreeMap::new();
        for (p, b, _w, d) in &s.far {
            let name = &o.model.skeleton.names[*b as usize];
            let limit = match vreach.get(name.as_str()) {
                Some(r) => BUDGET_RATIO * r + 2.0,
                None => 20.0,
            };
            if *d > limit {
                let e = far.entry(name.clone()).or_insert((0, 0.0, *p));
                e.0 += 1;
                if *d > e.1 {
                    e.1 = *d;
                    e.2 = *p;
                }
            }
        }
        let mut far: Vec<_> = far.into_iter().collect();
        far.sort_by_key(|f| std::cmp::Reverse(f.1 .0));
        for (bone, (n, d, p)) in far.iter().take(8) {
            out.push(("M118", format!("{n} vertices weighted >= 0.1 to '{bone}' sit up to {} units from it (Valve's farthest: {}), worst at {}: they stretch toward the bone when it moves", f1(*d), vreach.get(bone.as_str()).map_or("none".into(), |r| f1(*r)), at(*p))));
        }
    }
    if s.unweighted > 0 || s.missing_bone > 0 {
        out.push(("M119", format!("{} skinned vertices have no usable influence and {} influences name bones the skeleton lacks: they stay at bind (or the origin) while the body moves", s.unweighted, s.missing_bone)));
    }
    // M120, M122, M123 LODs and budgets (also in the light lint).
    budget_rules(o, valve, &mut out);
    if let Some(v) = valve {
        // M121 LOD silhouette drift against Valve's own drift; M124 fair-play silhouette.
        let ((dro, drv), sil) = crate::par::join(|| crate::par::join(|| views::lod_drift(o), || views::lod_drift(v)), || views::silhouettes(o, v));
        for (lod, iou, view) in &dro {
            if let Some((_, viou, _)) = drv.iter().find(|x| x.0 == *lod) {
                if *iou < viou - 0.05 {
                    out.push(("M121", format!("LOD{lod} silhouette matches LOD0 at {:.0}% in the {view} view, Valve's LOD{lod} at {:.0}%: the shape pops at the switch", 100.0 * iou, 100.0 * viou)));
                }
            }
        }
        for c in sil {
            if c.ratio() < fp.silhouette_min {
                out.push(("M124", format!("{} view: our in-game silhouette is {:.1}% of Valve's ({} vs {} px at 6 mm, {} px lost): the hero is harder to see than the original", c.view, 100.0 * c.ratio(), c.ours, c.valve, c.lost)));
            }
        }
    }
    // M125 cracks along seams.
    let co: usize = o.seams.iter().map(|x| x.2).sum();
    let cv: usize = valve.map_or(0, |v| v.seams.iter().map(|x| x.2).sum());
    if co > cv + 20 {
        let w = o.seams.iter().max_by_key(|x| x.2).unwrap();
        out.push(("M125", format!("{co} boundary vertices sit within {} units of another piece's without welding (Valve's {cv}), worst [{}] x [{}]: a hairline crack shows the background along the seam", super::census::SEAM_TOL, lbl(w.0), lbl(w.1))));
    }
    // M126 textured material without UVs, M127 unreferenced vertices, M128 unused materials.
    for (m, mat) in &o.untextured {
        out.push(("M126", format!("draw call of mesh '{m}' uses textured material {mat} but carries no UVs: the texture samples one texel everywhere")));
    }
    if o.unreferenced > 0 {
        out.push(("M127", format!("{} vertices in LOD0's buffers no triangle uses: memory with no picture", o.unreferenced)));
    }
    for m in &o.unused_materials {
        out.push(("M128", format!("material group 'default' lists {m}, which no draw call uses")));
    }
    // M136/M137 albedo against Valve's own range (SKIN-FLAWS A1).
    if !o.albedo.undecoded.is_empty() {
        out.push(("M100", format!("albedo not measured for {} colour map(s) whose pixels did not decode: {}", o.albedo.undecoded.len(), o.albedo.undecoded.join(", "))));
    }
    if let Some(v) = valve {
        let mut vs: Vec<(f64, f64)> = v.albedo.samples.values().flatten().copied().collect();
        if !vs.is_empty() && !o.albedo.samples.is_empty() {
            let (lo, hi) = (weighted_pct(&mut vs, 0.005), weighted_pct(&mut vs, 0.995));
            let mut all: Vec<(f64, f64)> = Vec::new();
            let mut per: Vec<(String, f64, f64, f64)> = Vec::new();
            for (m, s) in &o.albedo.samples {
                if v.materials.contains_key(m) {
                    continue; // Valve's own material
                }
                let a: f64 = s.iter().map(|x| x.1).sum();
                let below: f64 = s.iter().filter(|x| x.0 < lo).map(|x| x.1).sum();
                let above: f64 = s.iter().filter(|x| x.0 > hi).map(|x| x.1).sum();
                per.push((m.clone(), a, below, above));
                all.extend(s);
            }
            let total: f64 = all.iter().map(|x| x.1).sum();
            if total > 0.0 {
                let outside: f64 = per.iter().map(|x| x.2 + x.3).sum::<f64>() / total;
                let list = |pick: &dyn Fn(&(String, f64, f64, f64)) -> f64| -> String {
                    let mut p: Vec<&(String, f64, f64, f64)> = per.iter().filter(|x| pick(x) > 0.0).collect();
                    p.sort_by(|a, b| pick(b).partial_cmp(&pick(a)).unwrap_or(std::cmp::Ordering::Equal));
                    p.iter().take(4).map(|x| format!("{} {:.0}%", x.0.rsplit('/').next().unwrap_or(&x.0), 100.0 * pick(x) / x.1)).collect::<Vec<_>>().join(", ")
                };
                if outside > ALBEDO_OUTSIDE {
                    out.push(("M136", format!("{:.0}% of our new materials' area is outside Valve's albedo range for this model [{lo:.4}, {hi:.4}] linear (0.5th-99.5th percentile): below {}; above {}", 100.0 * outside, list(&|x| x.2), list(&|x| x.3))));
                }
                let black: f64 = all.iter().filter(|x| x.0 < ALBEDO_BLACK).map(|x| x.1).sum::<f64>() / total;
                let white: f64 = all.iter().filter(|x| x.0 > ALBEDO_WHITE).map(|x| x.1).sum::<f64>() / total;
                if black > ALBEDO_ABSOLUTE || white > ALBEDO_ABSOLUTE {
                    out.push(("M137", format!("{:.1}% of the area is absolute black (under {ALBEDO_BLACK} linear) and {:.1}% absolute white (over {ALBEDO_WHITE}): blacks lose every fold and read as holes, whites flatten and bloom", 100.0 * black, 100.0 * white)));
                }
            }
        }
    }
    // M138/M139 alpha-test coverage drift across mips (SKIN-FLAWS C5).
    for c in &o.coverage {
        if c.tiles == 0 {
            continue;
        }
        let at = |d: f64| c.at.iter().find(|x| x.0 == d).copied().unwrap_or((d, 0, 0.0));
        let (g4, g8) = (at(4.0), at(8.0));
        let msg = |x: (f64, u32, f64)| format!("{}: at {} m the game samples mip {} ({:.0} texels per metre on the model), where {:.0}% of the {} partly cut-out tiles of {} change coverage by more than 30%: the lace turns solid or vanishes as it recedes", c.material, x.0, x.1, c.texels_per_metre, 100.0 * x.2, c.tiles, c.texture);
        if g4.2 > COVERAGE_SHARE {
            out.push(("M139", msg(g4)));
        } else if g8.2 > COVERAGE_SHARE {
            out.push(("M138", msg(g8)));
        }
    }
    // M129, M140, M130: the hitbox envelope (the author's fair-play policy), at the policy's lines.
    if o.envelope.capsules == 0 && valve.is_some_and(|v| v.envelope.capsules > 0) {
        out.push(("M144", "the model has no capsule hitboxes to measure against (Valve's has some), so its hitbox envelope was not judged (M129, M130, M140)".into()));
    }
    out.extend(envelope_verdict(&o.envelope, valve.map(|v| &v.envelope), fp));
    out
}

/// A share as a percentage with up to two decimals: 0.035 -> "3.5", 0.0412 -> "4.12", 0.02 -> "2".
/// Whether box `a` lies within box `b` (to a hundredth of a unit), as a piece wrapped by a hull does.
fn inside(a: &v3::Aabb, b: &v3::Aabb) -> bool {
    !a.is_empty() && (0..3).all(|k| a.min[k] >= b.min[k] - 0.01 && a.max[k] <= b.max[k] + 0.01)
}

fn pct(share: f64) -> String {
    let t = format!("{:.2}", 100.0 * share + 0.0);
    let t = t.trim_end_matches('0').trim_end_matches('.');
    if t == "-0" { "0".into() } else { t.to_string() }
}

/// The facts an M129 or M140 message opens with. `envelope_measures` reads them back, so the two stay side by side.
fn envelope_facts(visible_beyond: f64, share: f64, taste: f64, worst: f64, region: &str, point: V3) -> String {
    format!(
        "{} u2 of visible surface ({}% of it) lies more than {taste} units beyond every hitbox capsule; worst point {:.2} u beyond {region} at {}",
        f1(visible_beyond),
        pct(share),
        worst,
        at(point)
    )
}

/// What an M129 or M140 message measured: the worst visible point's distance beyond the hitbox (units) and the share
/// of the visible surface beyond the taste line. The grade checks an allowance against these.
pub fn envelope_measures(message: &str) -> Option<(f64, f64)> {
    let share = percent(message.split("% of it)").next()?.rsplit('(').next()?)?;
    let worst = message.split("worst point ").nth(1)?.split(' ').next()?.parse::<f64>().ok()?;
    Some((worst, share))
}

/// What an M130 message measured: the share of the surface beyond the wide line.
pub fn wide_measure(message: &str) -> Option<f64> {
    percent(message.split("% of the surface").next()?)
}

/// A percentage as printed by `pct`, back to a share (rounded to the six decimals it can carry).
fn percent(text: &str) -> Option<f64> {
    let p = text.trim().parse::<f64>().ok()?;
    Some((p * 1e4).round() / 1e6)
}

/// Our worst point counts as new overflow when it lies this far (units) beyond Valve's own worst point in the same
/// hitbox region: 0.01 units is a quarter of a millimetre, well over the float noise of a recompiled mesh (1e-6).
pub const HITBOX_SAME: f64 = 0.01;

/// M129, M140 and M130 for one model's envelope, judged at the lines in `fp`, against Valve's envelope at the same
/// path when it is known. Only overflow Valve's model does not already have is judged: region by region (the hitbox a
/// triangle lies nearest, matched by name) our area beyond a line less Valve's there, and a worst point only where
/// it lies further out than Valve's own in that region. Valve's surface outside its own hitboxes (a gun, a prop on
/// the ground, a body its hitboxes were never fitted to) is Valve's design, so Valve's file against itself is silent.
/// What this cannot see: overflow of ours that sits inside a region where Valve's has as much area further out.
pub fn envelope_verdict(e: &Envelope, valve: Option<&Envelope>, fp: &FairPlay) -> Out {
    let mut out: Out = Vec::new();
    if e.capsules == 0 || e.area <= 0.0 {
        return out;
    }
    let valve = valve.filter(|v| v.capsules > 0 && v.area > 0.0);
    let (warn, taste, egregious) = (fp.hitbox_warn_units, fp.hitbox_taste_units, fp.hitbox_egregious_units);
    let wide_share = e.excess_beyond(valve, warn, false) / e.area;
    let valve_share = valve.map(|v| v.beyond(warn) / v.area);
    let regions: Vec<String> = e.regions(warn).iter().filter(|r| r.1 > warn).take(5).map(|r| format!("{} worst {} u at {} ({} u2 beyond {warn})", r.0, f1(r.1), at(r.2), f1(r.3))).collect();
    let visible_beyond = e.excess_beyond(valve, taste, true);
    let vis_share = if e.visible_area > 0.0 { visible_beyond / e.visible_area } else { 0.0 };
    // the worst point Valve's own surface does not already reach; with none, the area alone is judged and the
    // model's worst point is quoted for the place
    let new_worst = e.new_worst(valve, HITBOX_SAME, taste).filter(|w| w.0 > taste);
    let theirs = valve.filter(|v| v.visible_beyond(taste) > 0.0).map_or(String::new(), |v| {
        format!(
            " (counted beyond what Valve's own model has there: {} u2 of Valve's visible surface lies past {taste} u, its worst point {:.2} u, and that much is Valve's design)",
            f1(v.visible_beyond(taste)),
            v.visible_worst.0
        )
    });
    if visible_beyond > fp.hitbox_noise_area || new_worst.is_some() {
        let (wd, wp, wr, shift) = new_worst.clone().unwrap_or_else(|| (e.visible_worst.0, e.visible_worst.1, e.visible_worst.2.clone(), 0.0));
        let facts = envelope_facts(visible_beyond, vis_share, taste, wd, &wr, wp);
        let theirs = if shift > 0.0 { format!("{theirs} (that point is {:.2} u from the hitbox; Valve's own surface there reaches {:.2} u past the {taste} u line, so it is judged from where Valve's stops)", wd + shift, shift) } else { theirs };
        if new_worst.as_ref().is_some_and(|w| w.0 > egregious) || vis_share > fp.hitbox_egregious_share {
            out.push((
                "M129",
                format!(
                    "{facts}{theirs}: that is a bit much (worst point over {egregious} u, or over {}% of the visible surface beyond {taste} u). Enemies aim at cloth that can't be hit and the silhouette reads as excessive; pull it in. An allowance does not waive this tier: it is never allowable, and the grade's fair play fails on it. Regions: {}",
                    pct(fp.hitbox_egregious_share),
                    regions.join("; ")
                ),
            ));
        } else if visible_beyond > fp.hitbox_noise_area {
            out.push((
                "M140",
                format!(
                    "{facts}{theirs}, under the {egregious} u / {}% line: tasteful overflow needs a written reason, so accept with a reason in magician.allow if the silhouette piece earns it. Regions: {}",
                    pct(fp.hitbox_egregious_share),
                    regions.join("; ")
                ),
            ));
        }
    }
    if wide_share > fp.hitbox_warn_share {
        out.push((
            "M130",
            format!(
                "{}% of the surface lies more than {warn} units beyond every hitbox capsule (over {}%; Valve's own {}, not counted; precedent: Lady Geist's long dress sits at 0.6% with a worst point of 5.8 u): enemies aim at cloth that can't be hit. Regions: {}",
                pct(wide_share),
                pct(fp.hitbox_warn_share),
                valve_share.map_or("-".into(), |x| format!("{}%", pct(x))),
                regions.join("; ")
            ),
        ));
    }
    out
}

/// M120, M122, M123: LOD count, vertex economy per LOD, LOD0 budget against Valve's. Cheap: counts only.
pub fn budget_rules(o: &Census, valve: Option<&Census>, out: &mut Out) {
    let Some(v) = valve else {
        lod_inversions(o, None, out);
        return;
    };
        let (lo, lv) = (o.model.lod_count(), v.model.lod_count());
        if lo < lv {
            out.push(("M120", format!("{lo} LOD levels, Valve's model has {lv}: past the last one the engine keeps drawing the heavier mesh, or nothing")));
        }
        for (k, (a, b)) in o.lods.iter().zip(&v.lods).enumerate() {
            let vpt = |l: &super::census::LodStats| l.verts as f64 / l.tris.max(1) as f64;
            if b.tris == 0 {
                continue; // Valve's model draws nothing at this LOD (or its meshes are in other files): no figure to hold ours to
            }
            if vpt(a) > BUDGET_RATIO * vpt(b) && a.verts > b.verts + BUDGET_MIN_TRIS / 2 {
                out.push(("M122", format!("LOD{k}: {:.2} vertices per triangle, Valve's {:.2}: UV and normal splits cost vertex work with no visible gain", vpt(a), vpt(b))));
            }
            if k > 0 && a.tris as f64 > BUDGET_RATIO * b.tris as f64 && a.tris > b.tris + BUDGET_MIN_TRIS {
                out.push(("M122", format!("LOD{k}: {} triangles, Valve's {} ({:.1}x): the distant hero costs more than Valve's", a.tris, b.tris, a.tris as f64 / b.tris.max(1) as f64)));
            }
        }
        // how far each lower LOD is cut down, against how far Valve cuts its own at this path (a model Valve
        // never decimates, one mesh for every LOD, sets no limit)
        for k in 1..o.lods.len().min(v.lods.len()) {
            let keep = |l: &[super::census::LodStats]| l[k].verts as f64 / l[0].verts.max(1) as f64;
            let (a, b) = (keep(&o.lods), keep(&v.lods));
            if v.lods[k].verts > 0 && a > BUDGET_RATIO * b && o.lods[k].verts > v.lods[k].verts + BUDGET_MIN_TRIS / 2 {
                out.push(("M122", format!("LOD{k} keeps {:.0}% of LOD0's vertices, Valve's keeps {:.0}% (limit {BUDGET_RATIO}x Valve's): the distant hero is barely cheaper than the near one", 100.0 * a, 100.0 * b)));
            }
        }
        lod_inversions(o, Some(v), out);
        let (a, b) = (&o.lods[0], &v.lods[0]);
        let mut over = Vec::new();
        if a.tris as f64 > BUDGET_RATIO * b.tris as f64 && a.tris > b.tris + BUDGET_MIN_TRIS {
            over.push(format!("{} triangles vs {}", a.tris, b.tris));
        }
        if a.draws as f64 > 2.0 * b.draws as f64 && a.draws >= b.draws + BUDGET_MIN_DRAWS {
            over.push(format!("{} draw calls vs {}", a.draws, b.draws));
        }
        if o.texture_bytes as f64 > 2.0 * v.texture_bytes as f64 && v.texture_bytes > 0 && o.texture_bytes > v.texture_bytes + (16 << 20) {
            over.push(format!("{} MB of textures vs {}", o.texture_bytes >> 20, v.texture_bytes >> 20));
        }
        let (bo, bv) = (o.model.skeleton.names.len(), v.model.skeleton.names.len());
        if bo as f64 > BUDGET_RATIO * bv as f64 {
            over.push(format!("{bo} bones vs {bv}"));
        }
        if !over.is_empty() {
            out.push(("M123", format!("over Valve's budget at LOD0: {}: frame cost in team fights", over.join(", "))));
        }
}

/// M146: each LOD against the one before it and against LOD0. A material that gains triangles at a lower LOD, or a
/// lower LOD that reaches outside LOD0's bounding box, is a decimation gone wrong (pieces starved to a few triangles at
/// LOD0 while LOD1 kept them, a collapsed vertex flung out of the mesh). Valve's model at the same path sets what is
/// by design: a rise Valve's own material has at that LOD, and Valve's own reach outside its LOD0 box.
pub fn lod_inversions(o: &Census, valve: Option<&Census>, out: &mut Out) {
    for k in 1..o.lods.len() {
        let (prev, cur) = (&o.lods[k - 1], &o.lods[k]);
        for (m, &n) in &cur.material_tris {
            let before = prev.material_tris.get(m).copied().unwrap_or(0);
            if before == 0 || n <= before + LOD_INVERSION_MIN.max((LOD_INVERSION_SHARE * before as f64) as usize) {
                continue; // a material that first appears at a lower LOD is a stand-in, not an inversion
            }
            let valve_rises = valve.is_some_and(|v| v.lods.get(k).zip(v.lods.get(k - 1)).is_some_and(|(c, p)| c.material_tris.get(m).copied().unwrap_or(0) > p.material_tris.get(m).copied().unwrap_or(usize::MAX)));
            if valve_rises {
                continue;
            }
            out.push(("M146", format!("{m}: {n} triangles at LOD{k}, {before} at LOD{} ({:+.0}%): the nearer mesh is the cheaper one, so the piece gains detail as it recedes and pops at the switch", k - 1, 100.0 * (n as f64 / before as f64 - 1.0))));
        }
        let reach = |c: &Census, k: usize| -> f64 {
            let (a, b) = (&c.lods[0].bounds, &c.lods[k].bounds);
            if a.is_empty() || b.is_empty() {
                return 0.0;
            }
            (0..3).map(|i| (a.min[i] - b.min[i]).max(b.max[i] - a.max[i])).fold(0.0, f64::max)
        };
        let ours = reach(o, k);
        let theirs = valve.filter(|v| k < v.lods.len()).map(|v| reach(v, k));
        if ours > LOD_SPIKE_UNITS + theirs.unwrap_or(0.0) {
            out.push((
                "M146",
                format!(
                    "LOD{k} reaches {ours:.2} units outside LOD0's bounding box (limit {LOD_SPIKE_UNITS}, two silhouette pixels; Valve's LOD{k} reaches {}): a decimated vertex left the mesh, a spike that appears at the switch distance",
                    theirs.map_or("-".into(), |t| format!("{t:.2}"))
                ),
            ));
        }
    }
}

/// The default lint's share of the mesh family: decoding plus counts (M100 for undecodable buffers,
/// M120, M122, M123) and the material audit. No geometry census: `--deep` or `magician mesh` runs that.
pub fn judge_light(o: &Census, valve: Option<&Census>) -> Out {
    let mut out: Out = Vec::new();
    for n in o.notes.iter().take(6) {
        out.push(("M100", format!("not measured: {n}")));
    }
    budget_rules(o, valve, &mut out);
    out
}

/// Material audit of a hero model's materials against Valve's on the same model (SKIN-FLAWS V01-V03).
pub fn audit_materials(ours: &[(String, MatInfo)], valve: &[(String, MatInfo)]) -> Out {
    let mut out = Vec::new();
    let valve: Vec<(String, MatInfo)> = valve.iter().filter(|(_, m)| m.found).cloned().collect();
    if valve.is_empty() {
        return out;
    }
    let every_proxy = valve.iter().all(|(_, m)| m.status_proxy);
    let vopaque: Vec<&MatInfo> = valve.iter().map(|(_, m)| m).filter(|m| !(m.translucent || m.additive)).collect();
    let npr_valve = !vopaque.is_empty() && vopaque.iter().all(|m| m.npr);
    for (p, m) in ours.iter().filter(|(_, m)| m.found) {
        if valve.iter().any(|(vp, _)| vp == p) {
            continue; // Valve's own material, kept
        }
        let translucent = m.translucent || m.additive;
        if every_proxy && !m.status_proxy {
            out.push(("M131", format!("{p}: no F_USE_STATUS_EFFECTS_PROXY; every one of Valve's {} materials on this model has it: the piece stays visible under invisibility and misses damage flashes", valve.len())));
        }
        if !translucent && npr_valve && !m.npr {
            out.push(("M132", format!("{p}: opaque without F_USE_NPR_LIGHTING; Valve's opaque materials on this model all use it: the piece is lit differently from the rest of the hero")));
        }
        if m.alpha_test && m.outline {
            out.push(("M133", format!("{p}: alpha-tested with a non-zero NPR outline mask (ref {:.2}): the outline traces every cut-out and turns lace into a mesh of dark lines", m.alpha_ref)));
        }
    }
    out
}

/// SKIN-FLAWS V20: a lit opaque pbr.vfx material drawn by a particle's C_OP_RenderModels without
/// the NPR path or self-illum renders black.
pub fn particle_prop(material: &str, m: &MatInfo, particle: &str) -> Option<(&'static str, String)> {
    let lit_opaque = m.shader == "pbr.vfx" && !(m.translucent || m.additive || m.self_illum);
    (lit_opaque && !m.npr).then(|| ("M134", format!("{material}: lit opaque pbr.vfx drawn by C_OP_RenderModels in {particle} without F_USE_NPR_LIGHTING or self-illum: particle models get no normal lighting, it renders black")))
}

/// SKIN-FLAWS T1: a replaced texture against Valve's file at the same path.
pub fn texture_parity(path: &str, ours: &resource::texture::TexInfo, valve: &resource::texture::TexInfo) -> Option<(&'static str, String)> {
    let mut why = Vec::new();
    if ours.mips < valve.mips {
        why.push(format!("{} mips, Valve's {}", ours.mips, valve.mips));
    }
    if ours.format != valve.format {
        why.push(format!("{} where Valve's is {}", ours.format_name(), valve.format_name()));
    }
    if (ours.width, ours.height) != (valve.width, valve.height) {
        why.push(format!("{}x{} where Valve's is {}x{}", ours.width, ours.height, valve.width, valve.height));
    }
    (!why.is_empty()).then(|| ("M135", format!("{path}: {}: Valve's panels and materials draw it at Valve's sizes, so it aliases or shows block artefacts when drawn small", why.join(", "))))
}
