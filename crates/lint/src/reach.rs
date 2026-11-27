//! Where a model's attachments sit against its body, for M062: each attachment's bind position in model space, how
//! far it is from the nearest point of the model's skinned LOD0 surface and from the nearest bone, and the model's
//! size (the square root of that surface's area, the measure M109 and M110 judge by).
//!
//! The model file says how each attachment is held. One on a body bone rides the body: an effect point (a palm, a
//! muzzle, the idol on the back). One on the root bone, or on the model's root transform, is placed in the hero's own
//! frame and does not follow the body: on Valve's 43 heroes (6873 and 6875) those are the camera poses and pivots, the aim
//! point and the zipline hook, and they sit up to 3.1 times the hero's size away from it by design. A moved body
//! attachment is cosmetic while it stays as near the body as Valve keeps any (`mesh::rules::ATTACH_OFF_BODY_VALVE`);
//! a moved one in the hero's frame moves where the game positions something, and is judged as a contract break.

use geom::v3::{self, V3};
use resource::kv3::Value;

/// One attachment of a model, placed at bind.
#[derive(Debug, Clone, PartialEq)]
pub struct Point {
    pub name: String,
    /// Whether it is held in the hero's own frame (on the root bone or the root transform) rather than on the body.
    pub hero_frame: bool,
    /// Model-space position at bind.
    pub at: V3,
    /// Distance to the nearest point of the skinned LOD0 surface, and to the nearest bone.
    pub surface: f64,
    pub bone: f64,
}

impl Point {
    /// How far the attachment sits off the body: the nearer of the surface and the skeleton.
    pub fn off_body(&self) -> f64 {
        self.surface.min(self.bone)
    }
}

/// The attachments of a model against its body.
#[derive(Debug, Clone, Default)]
pub struct Reach {
    /// The square root of the skinned LOD0 surface's area.
    pub size: f64,
    pub points: Vec<Point>,
    /// Attachments that could not be placed, with why (an influence the skeleton lacks).
    pub unplaced: Vec<(String, String)>,
}

impl Reach {
    /// The placed copies of an attachment (every mesh of a model repeats its attachments; copies at one place are
    /// one point).
    pub fn of<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Point> + 'a {
        self.points.iter().filter(move |p| p.name == name)
    }
}

/// The model's attachments placed against its body, or why they can't be: a model with no skeleton or no surface to
/// measure against (`Err`).
pub fn read(data: &[u8]) -> Result<Reach, String> {
    let m = ::mesh::parse(data)?;
    if m.skeleton.names.is_empty() {
        return Err("the model has no skeleton".into());
    }
    let world = m.skeleton.world();
    // A root bone's attachments are placed in the hero's own frame, whatever rotation the root has at bind: Valve's
    // root bone is turned 120 degrees about (1, 1, 1) at bind, and its attachments only make sense unturned (Haze's
    // aim 63.9 units up, the standing camera pivot 86.2 up, the zipline hook 107 up; turned, all three lie on the
    // ground in front of the hero).
    let bone_at = |n: &str| -> Option<(Frame, bool)> {
        let i = m.skeleton.index(n)?;
        let (rot, t) = world.get(i).copied()?;
        let root = m.skeleton.parents.get(i).copied().unwrap_or(-1) < 0;
        Some((if root { (IDENTITY, t) } else { (rot, t) }, root))
    };
    let (surfaces, _) = m.surfaces(0);
    let skinned = surfaces.iter().any(|s| s.has_skin);
    let mut tris: Vec<[V3; 3]> = Vec::new();
    let mut area = 0.0;
    for s in surfaces.iter().filter(|s| s.has_skin || !skinned) {
        let p = |i: u32| s.positions.get(i as usize).map(|x| [x[0] as f64, x[1] as f64, x[2] as f64]);
        for t in &s.tris {
            if let (Some(a), Some(b), Some(c)) = (p(t[0]), p(t[1]), p(t[2])) {
                area += 0.5 * v3::len(v3::cross(v3::sub(b, a), v3::sub(c, a)));
                tris.push([a, b, c]);
            }
        }
    }
    if tris.is_empty() || area <= 0.0 {
        return Err("the model has no LOD0 surface to measure its attachments against".into());
    }
    let bones: Vec<V3> = world.iter().map(|(_, t)| [t[0] as f64, t[1] as f64, t[2] as f64]).collect();
    let mut r = Reach { size: area.sqrt(), ..Default::default() };
    let res = resource::parse(data).map_err(|e| e.to_string())?;
    let mut seen: Vec<(String, [i64; 3])> = Vec::new();
    for b in res.blocks_tagged(b"MDAT") {
        let bytes = resource::block_bytes(data, b);
        if !resource::kv3::is_kv3(bytes) {
            continue;
        }
        let Ok(md) = resource::kv3::decode(bytes) else { continue };
        for a in md.get("m_attachments").map(Value::as_array).unwrap_or(&[]) {
            let v = a.get("value").unwrap_or(a);
            let name = v.get("m_name").and_then(Value::as_str).unwrap_or("").to_string();
            match place(v, &bone_at) {
                Ok((at, hero_frame)) => {
                    let key = at.map(|x| (x * 100.0).round() as i64);
                    if seen.iter().any(|(n, k)| *n == name && *k == key) {
                        continue;
                    }
                    seen.push((name.clone(), key));
                    let surface = tris.iter().map(|[a, b, c]| v3::dist(at, v3::closest_on_triangle(at, *a, *b, *c))).fold(f64::INFINITY, f64::min);
                    let bone = bones.iter().map(|b| v3::dist(at, *b)).fold(f64::INFINITY, f64::min);
                    r.points.push(Point { name, hero_frame, at, surface, bone });
                }
                Err(why) => {
                    if !r.unplaced.iter().any(|(n, _)| *n == name) {
                        r.unplaced.push((name, why));
                    }
                }
            }
        }
    }
    Ok(r)
}

type Frame = ([[f32; 3]; 3], [f32; 3]);
const IDENTITY: [[f32; 3]; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

/// An attachment's model-space position at bind (each influence's offset carried into model space by its bone's bind
/// transform, weighted), and whether any influence holds it in the hero's own frame.
fn place(v: &Value, bone_at: &dyn Fn(&str) -> Option<(Frame, bool)>) -> Result<(V3, bool), String> {
    let names: Vec<&str> = v.get("m_influenceNames").map(|x| x.as_array().iter().filter_map(Value::as_str).collect()).unwrap_or_default();
    let offsets: Vec<Vec<f64>> = v.get("m_vInfluenceOffsets").map(|x| x.as_array().iter().map(Value::numbers).collect()).unwrap_or_default();
    let weights: Vec<f64> = v.get("m_influenceWeights").map(Value::numbers).unwrap_or_default();
    let root: Vec<bool> = v.get("m_bInfluenceRootTransform").map(|x| x.as_array().iter().map(|b| b.as_bool().unwrap_or(false)).collect()).unwrap_or_default();
    let used = v.get("m_nInfluences").and_then(Value::as_f64).map(|n| n.max(1.0) as usize).unwrap_or_else(|| names.iter().filter(|n| !n.is_empty()).count().max(1));
    let (mut at, mut total, mut hero_frame) = ([0.0; 3], 0.0, false);
    for i in 0..used.min(3) {
        let n = names.get(i).copied().unwrap_or("");
        let w = weights.get(i).copied().unwrap_or(if i == 0 { 1.0 } else { 0.0 });
        // an influence on the model's root transform (no bone: `m_bInfluenceRootTransform`) is in model space
        let on_root_transform = n.is_empty() && root.get(i).copied().unwrap_or(false);
        if w <= 0.0 || (n.is_empty() && !on_root_transform) {
            continue;
        }
        let o = offsets.get(i).map(|o| [o.first().copied().unwrap_or(0.0), o.get(1).copied().unwrap_or(0.0), o.get(2).copied().unwrap_or(0.0)]).unwrap_or([0.0; 3]);
        let ((rot, t), frame) = if on_root_transform { ((IDENTITY, [0.0; 3]), true) } else { bone_at(n).ok_or_else(|| format!("its bone '{n}' is not in the skeleton"))? };
        hero_frame |= frame;
        let p: V3 = std::array::from_fn(|k| t[k] as f64 + (0..3).map(|j| rot[k][j] as f64 * o[j]).sum::<f64>());
        at = v3::add(at, v3::scale(p, w));
        total += w;
    }
    if total <= 0.0 {
        return Err("it names no bone".into());
    }
    Ok((v3::scale(at, 1.0 / total), hero_frame))
}

/// What a moved attachment is: a cosmetic move on the body, one that leaves the body, one in the hero's frame, or one
/// that could not be measured.
#[derive(Debug, Clone, PartialEq)]
pub enum Move {
    /// On the body, `off` units off it, under `line`; moved `by` units from Valve's place.
    Cosmetic { by: f64, off: f64, line: f64 },
    /// On the body, `off` units off it, past `line`.
    OffBody { by: f64, off: f64, line: f64 },
    /// Held in the hero's own frame (ours or Valve's).
    HeroFrame { by: f64 },
    Unmeasured(String),
}

/// Judge one changed attachment `name` of our model against Valve's: `rate` is the line in units of our model's size.
pub fn judge(name: &str, ours: &Result<Reach, String>, valve: &Result<Reach, String>, rate: f64) -> Move {
    let (o, v) = match (ours, valve) {
        (Ok(o), Ok(v)) => (o, v),
        (Err(e), _) => return Move::Unmeasured(format!("our model: {e}")),
        (_, Err(e)) => return Move::Unmeasured(format!("Valve's model: {e}")),
    };
    let why = |r: &Reach| r.unplaced.iter().find(|(n, _)| n == name).map(|(_, w)| w.clone());
    let Some(worst) = o.of(name).max_by(|a, b| a.off_body().total_cmp(&b.off_body())) else {
        return Move::Unmeasured(why(o).map_or_else(|| "it is not in our model".into(), |w| format!("ours could not be placed: {w}")));
    };
    let by = v.of(name).map(|p| v3::dist(p.at, worst.at)).min_by(f64::total_cmp);
    let Some(by) = by else {
        return Move::Unmeasured(why(v).map_or_else(|| "it is not in Valve's model".into(), |w| format!("Valve's could not be placed: {w}")));
    };
    if worst.hero_frame || v.of(name).any(|p| p.hero_frame) || o.of(name).any(|p| p.hero_frame) {
        return Move::HeroFrame { by };
    }
    let (off, line) = (worst.off_body(), rate * o.size);
    if off > line {
        Move::OffBody { by, off, line }
    } else {
        Move::Cosmetic { by, off, line }
    }
}
