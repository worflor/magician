//! What a model and the hero's animation hang on each bone, for M062's lost and reparented bones.
//!
//! A model's bone carries what its files put on it: vertices weighted to it (any mesh, any LOD, any body-group
//! choice), hitboxes, attachments, cloth (the FeModel's control nodes) and ragdoll parts (`PHYS`'s bones). These are the
//! uses the compiler itself marks on each bone (`m_nFlag`), and on Valve's 107 hero models of 6875 (17,500 bones) the two agree on
//! every bone for hitboxes, attachments, cloth and ragdoll parts (the compiler's mesh mark is wider: every bone a mesh
//! lists, weighted or not). A bone none of those name, with nothing lost below it that they name, is a helper or a
//! leftover: nothing a player sees hangs on it.
//!
//! How a hero moves comes from its animation skeleton (`m_vecNmSkeletonRefs`, a `.vnmskel`) and the clips its anim
//! graphs play, not from the model. A model bone takes the model-space pose of the animation bone of the same name
//! (case aside), and one the animation skeleton lacks follows its model parent at its bind offset. Valve's own files
//! hold that: on 6875 Valve's heroes ship models whose hierarchy is not their animation skeleton's (a hero's
//! `weaponHand_L` under `weapon` where the animation holds it under `root_motion`, a hose under `hand_R` where the
//! animation holds it under `weapon_hand_R`), and those heroes play. Source 2 Viewer retargets a clip the same way
//! (`SkeletonRetargeter`). So a reparented bone the animation skeleton has moves as Valve's whatever its parent; one it
//! lacks moves as its new parent does. Seen in Valve's files and its heroes playing, not on a reparented mod in game:
//! UNVERIFIED there.

use crate::mesh::views::VIEW_UNIT;
use resource::kv3::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};

type V = [f64; 3];

fn sub(a: V, b: V) -> V {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn norm(a: V) -> f64 {
    (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt()
}

/// What one bone of a model carries.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Load {
    /// Vertices with a weight on the bone, over every mesh, LOD and body-group choice.
    pub vertices: usize,
    pub hitboxes: Vec<String>,
    pub attachments: Vec<String>,
    /// A control node of the model's cloth.
    pub cloth: bool,
    /// A part of the model's ragdoll.
    pub ragdoll: bool,
    /// How far from the bone at bind the furthest thing it carries lies (a vertex, an attachment's offset, a
    /// hitbox's corner): the lever a turn of the bone moves it by.
    pub reach: f64,
    /// The same for its weighted vertices alone.
    pub skin: f64,
}

impl Load {
    pub fn is_empty(&self) -> bool {
        self.vertices == 0 && self.hitboxes.is_empty() && self.attachments.is_empty() && !self.cloth && !self.ragdoll
    }

    /// What it carries, as a person reads it: `412 vertices, hitbox 'default/head', attachments 'eye_L' and 'eye_R'`.
    pub fn describe(&self) -> String {
        let mut parts = Vec::new();
        if self.vertices > 0 {
            parts.push(format!("{} {}", self.vertices, if self.vertices == 1 { "vertex" } else { "vertices" }));
        }
        parts.extend(named("hitbox", &self.hitboxes));
        parts.extend(named("attachment", &self.attachments));
        if self.cloth {
            parts.push("cloth".into());
        }
        if self.ragdoll {
            parts.push("a ragdoll part".into());
        }
        match parts.len() {
            0 | 1 => parts.join(""),
            n => format!("{} and {}", parts[..n - 1].join(", "), parts[n - 1]),
        }
    }

    fn add(&mut self, o: &Load) {
        self.vertices += o.vertices;
        for h in &o.hitboxes {
            if !self.hitboxes.contains(h) {
                self.hitboxes.push(h.clone());
            }
        }
        for a in &o.attachments {
            if !self.attachments.contains(a) {
                self.attachments.push(a.clone());
            }
        }
        self.cloth |= o.cloth;
        self.ragdoll |= o.ragdoll;
        self.reach = self.reach.max(o.reach);
        self.skin = self.skin.max(o.skin);
    }
}

/// `kind 'a'`, `kinds 'a', 'b' and 'c'`, or `9 kinds ('a', 'b', 'c' and 6 more)`.
fn named(kind: &str, list: &[String]) -> Option<String> {
    let q: Vec<String> = list.iter().map(|n| format!("'{n}'")).collect();
    match q.len() {
        0 => None,
        1 => Some(format!("{kind} {}", q[0])),
        n if n <= 4 => Some(format!("{kind}s {} and {}", q[..n - 1].join(", "), q[n - 1])),
        n => Some(format!("{n} {kind}s ({} and {} more)", q[..3].join(", "), n - 3)),
    }
}

/// A model's skeleton with what each bone carries.
#[derive(Debug, Clone, Default)]
pub struct Rig {
    pub names: Vec<String>,
    pub parents: Vec<i32>,
    /// Model-space bind transform of each bone (rotation row-major, translation).
    pub world: Vec<([[f32; 3]; 3], [f32; 3])>,
    pub load: Vec<Load>,
    /// Every bone name the model itself names, in lowercase, with what names it: an attachment's influence, a hitbox,
    /// a physics part, a cloth node, a mesh's bone list. A name here that the skeleton lacks is a reference to nothing.
    pub named: BTreeMap<String, BTreeSet<String>>,
    /// The model's attachment names, in lowercase.
    pub attachment_names: BTreeSet<String>,
}

impl Rig {
    pub fn index(&self, name: &str) -> Option<usize> {
        self.names.iter().position(|n| n == name)
    }

    pub fn parent(&self, i: usize) -> Option<usize> {
        self.parents.get(i).and_then(|&p| usize::try_from(p).ok()).filter(|&p| p < self.names.len() && p != i)
    }

    /// Every bone below `i`.
    pub fn below(&self, i: usize) -> Vec<usize> {
        let mut out = Vec::new();
        let mut todo = vec![i];
        while let Some(b) = todo.pop() {
            for c in (0..self.names.len()).filter(|&c| self.parent(c) == Some(b)) {
                if c != i && !out.contains(&c) {
                    out.push(c);
                    todo.push(c);
                }
            }
        }
        out
    }

    fn at(&self, i: usize) -> V {
        self.world.get(i).map_or([0.0; 3], |(_, t)| t.map(f64::from))
    }
}

/// What a compiled model hangs on each of its bones.
pub fn rig(data: &[u8]) -> Result<Rig, String> {
    let m = ::mesh::parse(data)?;
    if m.skeleton.names.is_empty() {
        return Err("the model has no skeleton".into());
    }
    let n = m.skeleton.names.len();
    let mut r = Rig { names: m.skeleton.names.clone(), parents: m.skeleton.parents.clone(), world: m.skeleton.world(), load: vec![Load::default(); n], ..Default::default() };
    let name = |r: &mut Rig, bone: &str, what: String| {
        r.named.entry(bone.to_ascii_lowercase()).or_default().insert(what);
    };
    for me in &m.meshes {
        for b in &me.bones {
            name(&mut r, &b.name, "its mesh's bone list (vertices are bound to it)".into());
        }
    }
    for h in &m.hitboxes {
        name(&mut r, &h.bone, format!("hitbox '{}/{}'", h.set, h.name));
    }
    let at: Vec<V> = (0..n).map(|i| r.at(i)).collect();
    let (surfaces, _) = m.every_surface();
    for s in &surfaces {
        for ((j, w), p) in s.joints.iter().zip(&s.weights).zip(&s.positions) {
            let mut seen: Vec<usize> = Vec::with_capacity(8);
            for (&b, &x) in j.iter().zip(w) {
                let b = b as usize;
                if b < n && x > 0.0 && !seen.contains(&b) {
                    seen.push(b);
                    let l = &mut r.load[b];
                    l.vertices += 1;
                    let d = norm(sub(p.map(f64::from), at[b]));
                    l.reach = l.reach.max(d);
                    l.skin = l.skin.max(d);
                }
            }
        }
    }
    for h in &m.hitboxes {
        if let Some(i) = r.index(&h.bone) {
            let name = format!("{}/{}", h.set, h.name);
            let corner = |c: [f32; 3]| norm(c.map(f64::from));
            let reach = corner(h.min).max(corner(h.max)) + h.radius as f64;
            let l = &mut r.load[i];
            if !l.hitboxes.contains(&name) {
                l.hitboxes.push(name);
            }
            l.reach = l.reach.max(reach);
        }
    }
    let res = resource::parse(data).map_err(|e| e.to_string())?;
    for b in res.blocks_tagged(b"MDAT") {
        let bytes = resource::block_bytes(data, b);
        if !resource::kv3::is_kv3(bytes) {
            continue;
        }
        let Ok(md) = resource::kv3::decode(bytes) else { continue };
        for a in md.get("m_attachments").map(Value::as_array).unwrap_or(&[]) {
            let v = a.get("value").unwrap_or(a);
            let name = v.get("m_name").and_then(Value::as_str).unwrap_or("").to_string();
            let infl: Vec<&str> = v.get("m_influenceNames").map(|x| x.as_array().iter().filter_map(Value::as_str).collect()).unwrap_or_default();
            let weights = v.get("m_influenceWeights").map(Value::numbers).unwrap_or_default();
            let offsets: Vec<Vec<f64>> = v.get("m_vInfluenceOffsets").map(|x| x.as_array().iter().map(Value::numbers).collect()).unwrap_or_default();
            let used = v.get("m_nInfluences").and_then(Value::as_f64).map(|x| x.max(1.0) as usize).unwrap_or(infl.len());
            if !name.is_empty() {
                r.attachment_names.insert(name.to_ascii_lowercase());
            }
            for bone in infl.iter().take(used.min(3)) {
                r.named.entry(bone.to_ascii_lowercase()).or_default().insert(format!("attachment '{name}'"));
            }
            for (k, bone) in infl.iter().enumerate().take(used.min(3)) {
                // a lone influence holds the attachment whatever weight it is written with (Valve writes 0 for some,
                // and the compiler still marks the bone as used by an attachment)
                let w = weights.get(k).copied().unwrap_or(if k == 0 { 1.0 } else { 0.0 });
                if let (Some(i), true) = (r.index(bone), w > 0.0 || used == 1) {
                    let o = offsets.get(k).map_or(0.0, |o| o.iter().take(3).map(|x| x * x).sum::<f64>().sqrt());
                    let l = &mut r.load[i];
                    if !l.attachments.contains(&name) {
                        l.attachments.push(name.clone());
                    }
                    l.reach = l.reach.max(o);
                }
            }
        }
    }
    if let Some(Ok(phys)) = resource::kv3_block(data, b"PHYS") {
        for bone in phys.get("m_boneNames").map(Value::as_array).unwrap_or(&[]).iter().filter_map(Value::as_str) {
            r.named.entry(bone.to_ascii_lowercase()).or_default().insert("a physics part".into());
            if let Some(i) = r.index(bone) {
                r.load[i].ragdoll = true;
            }
        }
        if let Some(c) = ::mesh::cloth::from_value(&phys) {
            for name in &c.names {
                r.named.entry(name.to_ascii_lowercase()).or_default().insert("a cloth node".into());
                if let Some(i) = r.index(name) {
                    r.load[i].cloth = true;
                }
            }
        }
    }
    Ok(r)
}

/// How far the hero's clips take one bone of the animation skeleton from its reference pose, relative to its parent:
/// the most any frame of any clip turns it (degrees), shifts it (units) and scales it.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Moved {
    pub degrees: f64,
    pub units: f64,
    pub scale: f64,
}

impl Moved {
    /// Whether the clips leave it at rest, within the clip format's own rounding (`mesh::rules::BONE_STILL_*`).
    pub fn still(&self) -> bool {
        use crate::mesh::rules::{BONE_STILL_DEGREES, BONE_STILL_UNITS};
        self.degrees <= BONE_STILL_DEGREES && self.units <= BONE_STILL_UNITS && self.scale <= BONE_STILL_UNITS
    }

    /// The most it can carry a point `lever` units from it: its shift, and the arc of its turn and scale at the lever.
    pub fn sweep(&self, lever: f64) -> f64 {
        self.units + lever * (self.degrees.to_radians().min(std::f64::consts::PI) + self.scale)
    }

    /// `turns up to 35.0 degrees and shifts 4.8 units`, or `never moves`.
    pub fn describe(&self) -> String {
        if self.still() {
            return "never moves".into();
        }
        let mut p = Vec::new();
        if self.degrees > crate::mesh::rules::BONE_STILL_DEGREES {
            p.push(format!("turns up to {:.1} degrees", self.degrees));
        }
        if self.units > crate::mesh::rules::BONE_STILL_UNITS {
            p.push(format!("shifts {:.1} units", self.units));
        }
        if self.scale > crate::mesh::rules::BONE_STILL_UNITS {
            p.push(format!("scales by {:.2}", self.scale));
        }
        p.join(" and ")
    }
}

/// The hero's animation, read from the files the model names.
#[derive(Debug, Clone, Default)]
pub struct Motion {
    /// The animation skeleton (`.vnmskel`) the model names.
    pub skeleton: String,
    pub names: Vec<String>,
    pub parents: Vec<i32>,
    /// Model-space position and rotation (x, y, z, w) of each animation bone in its reference pose.
    pub at: Vec<V>,
    pub rot: Vec<[f64; 4]>,
    pub moved: Vec<Moved>,
    /// Clips of that skeleton read, and clips that could not be.
    pub clips: usize,
    pub unread: Vec<String>,
    /// Where the bounded walk over the graphs stopped, when it did.
    pub stopped: Option<String>,
    /// The anim graphs read on the way (compiled paths).
    pub graphs: BTreeSet<String>,
}

/// The most anim graphs and clips `motion` reads for one model. A hero's graphs reach about 300 clips (6875).
pub const MOTION_FILES: usize = 4000;

impl Motion {
    /// The animation bone a model bone takes its pose from: the one of its name, case aside, as the game's names are
    /// (a hero's model has `pelvis` where its animation skeleton has `Pelvis`).
    pub fn index(&self, name: &str) -> Option<usize> {
        self.names.iter().position(|n| n == name).or_else(|| self.names.iter().position(|n| n.eq_ignore_ascii_case(name)))
    }

    fn parent(&self, i: usize) -> Option<usize> {
        self.parents.get(i).and_then(|&p| usize::try_from(p).ok()).filter(|&p| p < self.names.len() && p != i)
    }

    /// Why the clips read cannot say how the hero moves, if they cannot: none of the skeleton's was found, the walk
    /// stopped, or a clip in the game did not decode (a clip the game does not have cannot play, and is not counted).
    pub fn incomplete(&self) -> Option<String> {
        let broken = self.unread.iter().filter(|u| !u.ends_with(": not in the game")).count();
        if self.clips == 0 {
            Some("no clip of its animation skeleton was found through its graphs".into())
        } else if let Some(s) = &self.stopped {
            Some(format!("the walk over its graphs stopped {s}"))
        } else if broken > 0 {
            Some(format!("{broken} of its clips could not be decoded"))
        } else {
            None
        }
    }

    /// The animation bones that move `a` against `b` (`None` is the model's origin, which no clip moves: root motion
    /// is kept apart from the pose), with the most each carries a point at `point` (model space, at rest): every bone
    /// from each up to their common ancestor, the ancestor itself left out, that the clips do not leave still.
    pub fn between(&self, a: Option<usize>, b: Option<usize>, point: V) -> Vec<(usize, f64)> {
        let up = |i: Option<usize>| {
            let mut path = Vec::new();
            let mut at = i;
            while let Some(x) = at {
                if path.contains(&x) {
                    break;
                }
                path.push(x);
                at = self.parent(x);
            }
            path
        };
        let (pa, pb) = (up(a), up(b));
        let common = pa.iter().find(|x| pb.contains(x)).copied();
        let side = |p: &[usize]| p.iter().take_while(|&&x| Some(x) != common).copied().collect::<Vec<_>>();
        side(&pa)
            .into_iter()
            .chain(side(&pb))
            .filter_map(|x| {
                let m = self.moved.get(x)?;
                let lever = norm(sub(point, self.at.get(x).copied().unwrap_or([0.0; 3])));
                (!m.still()).then(|| (x, m.sweep(lever)))
            })
            .collect()
    }
}

/// Read the hero's animation for a model: the animation skeleton it names, and every clip of that skeleton its anim
/// graphs reach (graphs through their references, bounded by `MOTION_FILES`). `read` returns a game file's bytes by
/// its compiled path. `Err` when the model names no animation skeleton or the skeleton cannot be read.
pub fn motion(model: &[u8], read: &dyn Fn(&str) -> Option<Vec<u8>>) -> Result<Motion, String> {
    let d = match resource::kv3_block(model, b"DATA") {
        Some(Ok(d)) => d,
        _ => return Err("the model's data block could not be read".into()),
    };
    let skeleton = d.get("m_vecNmSkeletonRefs").map(Value::as_array).unwrap_or(&[]).iter().find_map(Value::as_str).map(str::to_string);
    let Some(skeleton) = skeleton else { return Err("the model names no animation skeleton".into()) };
    let compiled = |p: &str| {
        let p = p.to_ascii_lowercase().replace('\\', "/");
        if p.ends_with("_c") { p } else { format!("{p}_c") }
    };
    let sk_bytes = read(&compiled(&skeleton)).ok_or_else(|| format!("its animation skeleton {skeleton} is not in the game"))?;
    let sk = match resource::kv3_block(&sk_bytes, b"DATA") {
        Some(Ok(v)) => v,
        _ => return Err(format!("its animation skeleton {skeleton} could not be decoded")),
    };
    let names: Vec<String> = sk.get("m_boneIDs").map(Value::as_array).unwrap_or(&[]).iter().filter_map(Value::as_str).map(str::to_string).collect();
    let parents: Vec<i32> = sk.get("m_parentIndices").map(Value::numbers).unwrap_or_default().iter().map(|&x| x as i32).collect();
    // each reference transform is position (3), scale, rotation (x, y, z, w)
    let reference: Vec<Pose> = sk.get("m_parentSpaceReferencePose").map(Value::as_array).unwrap_or(&[]).iter().map(|t| Pose::from_reference(&t.numbers())).collect();
    if names.is_empty() {
        return Err(format!("its animation skeleton {skeleton} has no bones"));
    }
    let mut m = Motion { skeleton: skeleton.clone(), moved: vec![Moved::default(); names.len()], names, parents, ..Default::default() };
    // model-space reference positions, parents first (a parent always comes before its children)
    let mut world: Vec<([f64; 4], V, f64)> = Vec::with_capacity(m.names.len());
    for i in 0..m.names.len() {
        let p = reference.get(i).copied().unwrap_or(Pose::IDENTITY);
        let w = match m.parent(i).filter(|&q| q < world.len()) {
            Some(q) => {
                let (qr, qt, qs) = world[q];
                (qmul(qr, p.rot), add(qt, rotate(qr, p.pos.map(|x| x * qs))), qs * p.scale)
            }
            None => (p.rot, p.pos, p.scale),
        };
        world.push(w);
    }
    m.at = world.iter().map(|w| w.1).collect();
    m.rot = world.iter().map(|w| w.0).collect();
    let mut queue: Vec<String> = d.get("m_animGraph2Refs").map(Value::as_array).unwrap_or(&[]).iter().filter_map(|g| g.get("m_hGraph").and_then(Value::as_str)).map(compiled).collect();
    let mut seen: BTreeSet<String> = queue.iter().cloned().collect();
    let mut walked = 0usize;
    while let Some(path) = queue.pop() {
        if walked >= MOTION_FILES {
            m.stopped = Some(format!("after {MOTION_FILES} anim graphs and clips, with {} more unread", queue.len() + 1));
            break;
        }
        walked += 1;
        let Some(bytes) = read(&path) else {
            if path.ends_with(".vnmclip_c") {
                m.unread.push(format!("{path}: not in the game"));
            }
            continue;
        };
        if path.ends_with(".vnmclip_c") {
            match clip(&bytes, &skeleton, &reference) {
                Ok(Some(per_bone)) => {
                    m.clips += 1;
                    for (acc, x) in m.moved.iter_mut().zip(per_bone) {
                        acc.degrees = acc.degrees.max(x.degrees);
                        acc.units = acc.units.max(x.units);
                        acc.scale = acc.scale.max(x.scale);
                    }
                }
                Ok(None) => {}
                Err(e) => m.unread.push(format!("{path}: {e}")),
            }
            continue;
        }
        m.graphs.insert(path.clone());
        let Ok(r) = resource::parse(&bytes) else { continue };
        for e in &r.ext_refs {
            let p = e.compiled_path();
            if (p.ends_with(".vnmgraph_c") || p.ends_with(".vnmclip_c")) && seen.insert(p.clone()) {
                queue.push(p);
            }
        }
    }
    Ok(m)
}

/// The anim graphs and clips the game can play on a model: its AnimGraph 2 graphs (`m_animGraph2Refs`) and every graph
/// and clip they reach through their references, as compiled paths. Only graphs are read; at most `MOTION_FILES` of
/// them. Empty when the model names no graph.
pub fn played(model: &[u8], read: &dyn Fn(&str) -> Option<Vec<u8>>) -> BTreeSet<String> {
    let compiled = |p: &str| {
        let p = p.to_ascii_lowercase().replace('\\', "/");
        if p.ends_with("_c") { p } else { format!("{p}_c") }
    };
    let Some(Ok(d)) = resource::kv3_block(model, b"DATA") else { return BTreeSet::new() };
    let mut queue: Vec<String> = d.get("m_animGraph2Refs").map(Value::as_array).unwrap_or(&[]).iter().filter_map(|g| g.get("m_hGraph").and_then(Value::as_str)).map(compiled).collect();
    let mut seen: BTreeSet<String> = queue.iter().cloned().collect();
    let mut walked = 0usize;
    while let Some(path) = queue.pop() {
        if walked >= MOTION_FILES || path.ends_with(".vnmclip_c") {
            continue;
        }
        walked += 1;
        let Some(r) = read(&path).and_then(|b| resource::parse(&b).ok()) else { continue };
        for e in &r.ext_refs {
            let p = e.compiled_path();
            if (p.ends_with(".vnmgraph_c") || p.ends_with(".vnmclip_c")) && seen.insert(p.clone()) {
                queue.push(p);
            }
        }
    }
    seen
}

fn add(a: V, b: V) -> V {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

/// The Hamilton product of two quaternions written x, y, z, w.
fn qmul(a: [f64; 4], b: [f64; 4]) -> [f64; 4] {
    let ([ax, ay, az, aw], [bx, by, bz, bw]) = (a, b);
    [aw * bx + ax * bw + ay * bz - az * by, aw * by - ax * bz + ay * bw + az * bx, aw * bz + ax * by - ay * bx + az * bw, aw * bw - ax * bx - ay * by - az * bz]
}

/// `v` turned by the unit quaternion `q`.
fn rotate(q: [f64; 4], v: V) -> V {
    let r = qmul(qmul(q, [v[0], v[1], v[2], 0.0]), [-q[0], -q[1], -q[2], q[3]]);
    [r[0], r[1], r[2]]
}

/// A local transform: position, rotation (x, y, z, w), uniform scale.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Pose {
    pos: V,
    rot: [f64; 4],
    scale: f64,
}

impl Pose {
    const IDENTITY: Pose = Pose { pos: [0.0; 3], rot: [0.0, 0.0, 0.0, 1.0], scale: 1.0 };

    fn from_reference(n: &[f64]) -> Pose {
        let g = |i: usize, d: f64| n.get(i).copied().unwrap_or(d);
        Pose { pos: [g(0, 0.0), g(1, 0.0), g(2, 0.0)], scale: g(3, 1.0), rot: [g(4, 0.0), g(5, 0.0), g(6, 0.0), g(7, 1.0)] }
    }

    /// How far `self` is from `to`: the angle between the rotations, the distance between the positions, the
    /// difference of the scales.
    fn from(&self, to: &Pose) -> Moved {
        let len = |q: &[f64; 4]| q.iter().map(|x| x * x).sum::<f64>().sqrt().max(1e-12);
        let dot = self.rot.iter().zip(&to.rot).map(|(a, b)| a * b).sum::<f64>() / (len(&self.rot) * len(&to.rot));
        Moved { degrees: 2.0 * dot.abs().min(1.0).acos().to_degrees(), units: norm(sub(self.pos, to.pos)), scale: (self.scale - to.scale).abs() }
    }
}

/// How far one clip takes each bone of `skeleton` from its reference pose (`reference`), the most over its frames.
/// An additive clip is a change laid over another pose, so it is measured from no change, and its scale track (a
/// change of scale, written as 0 for none) is not read as a scale. `Ok(None)` for a clip of another skeleton.
fn clip(bytes: &[u8], skeleton: &str, reference: &[Pose]) -> Result<Option<Vec<Moved>>, String> {
    let d = match resource::kv3_block(bytes, b"DATA") {
        Some(Ok(d)) => d,
        _ => return Err("its data could not be decoded".into()),
    };
    if !d.get("m_skeleton").and_then(Value::as_str).is_some_and(|s| s.eq_ignore_ascii_case(skeleton)) {
        return Ok(None);
    }
    let additive = d.get("m_bIsAdditive").and_then(Value::as_bool).unwrap_or(false);
    let range = |t: &Value, k: &str| {
        let r = t.get(k);
        (r.and_then(|r| r.get("m_flRangeStart")).and_then(Value::as_f64).unwrap_or(0.0), r.and_then(|r| r.get("m_flRangeLength")).and_then(Value::as_f64).unwrap_or(0.0))
    };
    struct Track {
        x: (f64, f64),
        y: (f64, f64),
        z: (f64, f64),
        s: (f64, f64),
        rot: [f64; 4],
        rot_static: bool,
        pos_static: bool,
        scale_static: bool,
    }
    let tracks: Vec<Track> = d
        .get("m_trackCompressionSettings")
        .map(Value::as_array)
        .unwrap_or(&[])
        .iter()
        .map(|t| {
            let q = t.get("m_constantRotation").map(Value::numbers).unwrap_or_default();
            let flag = |k: &str| t.get(k).and_then(Value::as_bool).unwrap_or(true);
            Track {
                x: range(t, "m_translationRangeX"),
                y: range(t, "m_translationRangeY"),
                z: range(t, "m_translationRangeZ"),
                s: range(t, "m_scaleRange"),
                rot: [q.first().copied().unwrap_or(0.0), q.get(1).copied().unwrap_or(0.0), q.get(2).copied().unwrap_or(0.0), q.get(3).copied().unwrap_or(1.0)],
                rot_static: flag("m_bIsRotationStatic"),
                pos_static: flag("m_bIsTranslationStatic"),
                scale_static: flag("m_bIsScaleStatic"),
            }
        })
        .collect();
    let bytes_of = |v: &Value| -> Vec<u8> {
        match v {
            Value::Blob(b) => b.clone(),
            other => other.numbers().iter().map(|&x| x as u8).collect(),
        }
    };
    let raw = d.get("m_compressedPoseData").map(bytes_of).unwrap_or_default();
    let words: Vec<u16> = raw.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect();
    let offsets: Vec<usize> = d.get("m_compressedPoseOffsets").map(Value::numbers).unwrap_or_default().iter().map(|&x| x.max(0.0) as usize).collect();
    let unorm = |u: u16, (start, len): (f64, f64)| start + len * (u as f64 / 65535.0);
    let mut out = vec![Moved::default(); tracks.len()];
    let base = |i: usize| if additive { Pose::IDENTITY } else { reference.get(i).copied().unwrap_or(Pose::IDENTITY) };
    // a clip whose every track is static has one pose, whatever its frame count
    let frames: Vec<usize> = if offsets.is_empty() { vec![0] } else { offsets };
    for &at in &frames {
        let mut cursor = at;
        let mut take = |k: usize| -> Result<Vec<u16>, String> {
            let s = words.get(cursor..cursor + k).ok_or("its pose data ends inside a frame")?.to_vec();
            cursor += k;
            Ok(s)
        };
        for (i, t) in tracks.iter().enumerate() {
            let mut p = Pose { pos: [t.x.0, t.y.0, t.z.0], rot: t.rot, scale: t.s.0 };
            if !t.rot_static {
                p.rot = quaternion(&take(3)?);
            }
            if !t.pos_static {
                let v = take(3)?;
                p.pos = [unorm(v[0], t.x), unorm(v[1], t.y), unorm(v[2], t.z)];
            }
            if !t.scale_static {
                p.scale = unorm(take(1)?[0], t.s);
            }
            let mut x = p.from(&base(i));
            if additive {
                x.scale = 0.0;
            }
            if let Some(acc) = out.get_mut(i) {
                acc.degrees = acc.degrees.max(x.degrees);
                acc.units = acc.units.max(x.units);
                acc.scale = acc.scale.max(x.scale);
            }
        }
    }
    Ok(Some(out))
}

/// A quaternion stored in three 16-bit words: the three smaller components in 15 bits over ±1/√2, the index of the
/// largest (rebuilt from the unit length) in the top bits of the first two words. Source 2 Viewer's `AnimationClip`
/// decodes it the same way.
fn quaternion(w: &[u16]) -> [f64; 4] {
    let (lo, len) = (-std::f64::consts::FRAC_1_SQRT_2, std::f64::consts::SQRT_2);
    let g = |i: usize| w.get(i).copied().unwrap_or(0);
    let v = [(g(0) & 0x7fff) as f64, (g(1) & 0x7fff) as f64, g(2) as f64].map(|x| lo + x * len / 32767.0);
    let rest = (1.0 - v.iter().map(|x| x * x).sum::<f64>()).max(0.0).sqrt();
    match ((g(0) >> 14) & 2) | (g(1) >> 15) {
        0 => [rest, v[0], v[1], v[2]],
        1 => [v[0], rest, v[1], v[2]],
        2 => [v[0], v[1], rest, v[2]],
        _ => [v[0], v[1], v[2], rest],
    }
}

// ------------------------------------------------------------------------------- who outside the model names a bone

/// The bone users of a build (`magician index` writes them beside the users index, `bone_users.tsv`): every name a
/// file outside a model names as a bone, with the kind of file (`graph`, `clip`, `particle`, `vdata`, or `code` for
/// a string the game's client library holds), the key it sits under and the file. Names are lowercase: the engine
/// finds a bone by name, case aside.
///
/// What the index found on 6880: only the anim graphs name bones (`m_boneID`, `m_effectorBoneID`,
/// `m_followTargetBone`, `m_bone`, `m_endEffectorBoneID`: IK targets, follow targets, root motion); particles, vdata
/// and clips name attachments and hitbox sets, never a bone. A graph poses the animation skeleton (`.vnmskel`), not
/// the model: Valve's own hero models lack bones their graphs name (`weaponHand_L` and `weaponHand_R` on most heroes,
/// leg IK targets on one) and those heroes play. The client code holds 154 of the names on Valve's hero models, and
/// Valve's models carry 153 of them (the other is an attachment's name on an effect model).
#[derive(Debug, Clone, Default)]
pub struct BoneUsers {
    pub by_name: HashMap<String, Vec<(String, String, String)>>,
    /// Whether the client library was read when the file was written (`code=yes`): without it, whether the code names
    /// a bone is unknown.
    pub code_read: bool,
}

/// The bone users file's format; one written in another is not read.
pub const BONE_USERS_REV: u32 = 1;

impl BoneUsers {
    /// Read `bone_users.tsv`: `None` when it is not one, or is in another format.
    pub fn parse(text: &str) -> Option<BoneUsers> {
        let head = text.lines().next()?;
        if !head.starts_with("# magician bone users ") || !head.split_whitespace().any(|w| w == format!("rev={BONE_USERS_REV}")) {
            return None;
        }
        let mut u = BoneUsers { code_read: head.split_whitespace().any(|w| w == "code=yes"), ..Default::default() };
        for l in text.lines().skip(1) {
            let p: Vec<&str> = l.split('\t').collect();
            if let [name, kind, key, path] = p[..] {
                u.by_name.entry(name.to_string()).or_default().push((kind.to_string(), key.to_string(), path.to_string()));
            }
        }
        Some(u)
    }

    fn of(&self, name: &str) -> &[(String, String, String)] {
        self.by_name.get(&name.to_ascii_lowercase()).map(Vec::as_slice).unwrap_or(&[])
    }
}

// ------------------------------------------------------------------------------- the judgement

/// Said of a lost or reparented bone whose loss or new parent changes nothing a player sees: the mark of an M062 finding
/// that is a warning (`deep::contract_severity`).
pub const NOTHING_CHANGES: &str = "nothing a player sees changes";

/// The bones a model bone's motion comes from, in one rig: itself when the animation skeleton has it, else the
/// nearest model ancestor it has (`None`: the model's origin). Bones between follow at their bind offsets.
fn anchor(r: &Rig, m: &Motion, i: usize) -> Option<usize> {
    let mut at = Some(i);
    let mut guard = 0;
    while let Some(x) = at {
        if let Some(a) = m.index(&r.names[x]) {
            return Some(a);
        }
        guard += 1;
        if guard > r.names.len() {
            break;
        }
        at = r.parent(x);
    }
    None
}

/// Bone `i` of a rig and every bone below it that follows it: one the animation skeleton lacks, reached through
/// others it lacks.
fn following(r: &Rig, m: &Motion, i: usize) -> Vec<usize> {
    let mut out = vec![i];
    let mut todo = vec![i];
    while let Some(b) = todo.pop() {
        for c in (0..r.names.len()).filter(|&c| r.parent(c) == Some(b)) {
            if m.index(&r.names[c]).is_none() && !out.contains(&c) {
                out.push(c);
                todo.push(c);
            }
        }
    }
    out
}

fn rot_angle(a: &[[f32; 3]; 3], b: &[[f32; 3]; 3]) -> f64 {
    let tr: f64 = (0..3).map(|i| (0..3).map(|k| a[k][i] as f64 * b[k][i] as f64).sum::<f64>()).sum();
    ((tr - 1.0) / 2.0).clamp(-1.0, 1.0).acos().to_degrees()
}

/// A bone Valve's model has and ours lacks, judged by who still names it: a full skin brings its own mesh, cloth and
/// ragdoll, so what Valve's model hung on the bone is not in the mod, and what can break is something that looks the
/// bone up by name. That is this model itself (an attachment riding it, a hitbox, a physics part or cloth node, a
/// mesh's bone list: each a reference to a bone the skeleton no longer has), or the game's client code, which holds
/// the name as a string (what it looks up by it is UNVERIFIED), unless the model has an attachment of that name for
/// the lookup to find. Either is an error that names what. The hero's anim graphs name bones too, but they pose the
/// animation skeleton, which the model does not change, and Valve's own models lack bones their graphs name: said,
/// never an error. With no bone users index, who names the bone is unknown, and unknown is not clean: an error that
/// says so.
fn lost_bone(msg: &str, name: &str, ours: Option<&Rig>, v: &Rig, vi: usize, motion: &Result<Motion, String>, users: Option<&BoneUsers>) -> String {
    let clips = |m: &Motion| format!("its {} animation {}", m.clips, if m.clips == 1 { "clip" } else { "clips" });
    let lower = name.to_ascii_lowercase();
    let anim = match motion {
        Ok(m) => match (m.index(name), m.incomplete()) {
            (None, _) => "its animation skeleton has no such bone".into(),
            (Some(_), Some(why)) => format!("how its clips move it is not known: {why}"),
            (Some(a), None) => format!("in {} it {}", clips(m), m.moved[a].describe()),
        },
        Err(e) => format!("its animation could not be read: {e}"),
    };
    let hung = &v.load[vi];
    let valve_had = if hung.is_empty() {
        "Valve's model hangs nothing on it".to_string()
    } else {
        let mut why = Vec::new();
        if hung.vertices > 0 || hung.cloth || hung.ragdoll {
            why.push("its mesh, cloth and ragdoll are the mod's own now");
        }
        if !hung.attachments.is_empty() {
            why.push("an attachment is judged on its own");
        }
        if !hung.hitboxes.is_empty() {
            why.push("the hitboxes are compared on their own, as fair play");
        }
        format!("Valve's model hangs {} on it ({})", hung.describe(), why.join("; "))
    };
    // what in this model still names it: a reference to a bone its skeleton lacks
    let inner: Vec<String> = ours.and_then(|o| o.named.get(&lower)).map(|s| s.iter().cloned().collect()).unwrap_or_default();
    let Some(users) = users else {
        let also = if inner.is_empty() { String::new() } else { format!("; this model still names it in {}", list(&inner)) };
        return format!(
            "{msg}: {valve_had}, and who outside the model names it could not be checked: there is no bone users index for this build (`magician index` builds it){also}; {anim}"
        );
    };
    let named = users.of(name);
    let attachment_here = ours.is_some_and(|o| o.attachment_names.contains(&lower));
    let code = named.iter().any(|(k, _, _)| k == "code") && !attachment_here;
    let in_play = |p: &str| motion.as_ref().map_or(true, |m| m.graphs.contains(p));
    let mut graph_keys: Vec<String> = named.iter().filter(|(k, _, p)| k == "graph" && in_play(p)).map(|(_, key, _)| key.clone()).collect();
    graph_keys.sort();
    graph_keys.dedup();
    let graphs = if graph_keys.is_empty() {
        String::new()
    } else {
        format!(
            "; the hero's anim graphs name it ({}), but they pose the animation skeleton, which the model does not change, and Valve's own models lack bones their graphs name and play",
            graph_keys.join(", ")
        )
    };
    let mut who = Vec::new();
    if !inner.is_empty() {
        who.push(format!("this model itself ({})", list(&inner)));
    }
    if code {
        who.push("the game's client code (it holds the name as a string and may look the bone up by it: UNVERIFIED)".to_string());
    }
    if who.is_empty() && !users.code_read {
        return format!(
            "{msg}: {valve_had}, and whether the game's client code names it could not be checked: the bone users index was written without the client library; {anim}"
        );
    }
    if who.is_empty() {
        return format!("{msg}: nothing outside the model looks it up as a bone, and the model names it nowhere; {valve_had}{graphs}; {anim}; {NOTHING_CHANGES}");
    }
    format!(
        "{msg}, and {} still name{} it: what is placed by it has no bone to follow and goes to the model's origin or is not found (UNVERIFIED in game); {}; {valve_had}{graphs}; {anim}",
        who.join(", and "),
        if who.len() == 1 { "s" } else { "" },
        if inner.is_empty() { "put the bone back under Valve's name" } else { "put the bone back under Valve's name, or move what names it onto a bone the model has" }
    )
}

/// `a`, `a and b`, `a, b and c`.
fn list(v: &[String]) -> String {
    match v.len() {
        0 => String::new(),
        1 => v[0].clone(),
        n => format!("{} and {}", v[..n - 1].join(", "), v[n - 1]),
    }
}

/// M062's bone breaches judged by what hangs on the bone: `ours` and `valve` are the two models' bytes, `read` the
/// game's files (for the hero's animation). A lost bone that Valve's model hangs nothing on, with nothing it hangs
/// anything on lost below it, says so and is a warning; one that carries something says what, and stays an error. A
/// reparented bone is judged by what this model hangs on it and on the bones that follow it: one the animation poses
/// by name moves as Valve's whatever its parent, so only vertices bound to it at another place than Valve's bone can
/// land elsewhere; one it does not pose follows its parent, so what hangs on it moves differently when the new parent
/// and Valve's move apart in the clips or it sits elsewhere at bind. Within a fair-play pixel, or with nothing on it,
/// it is a warning; past that, an error that says how far and what. A mod that ships the animation skeleton itself
/// (`ships`, a path in the mod) moves the bone its own way, which Valve's files cannot judge: its reparented bones stay
/// errors that say so. Every other breach is returned as it is.
pub fn judge(breaches: Vec<String>, ours: &[u8], valve: &[u8], read: &dyn Fn(&str) -> Option<Vec<u8>>, ships: &dyn Fn(&str) -> bool, users: Option<&BoneUsers>) -> Vec<String> {
    let bone = |m: &str| m.strip_prefix("bone '").and_then(|r| r.split_once('\'')).map(|(n, rest)| (n.to_string(), rest.starts_with(" missing")));
    if !breaches.iter().any(|m| bone(m).is_some()) {
        return breaches;
    }
    let (or, vr) = (rig(ours), rig(valve));
    let motion = motion(valve, read);
    let clips = |m: &Motion| format!("its {} animation {}", m.clips, if m.clips == 1 { "clip" } else { "clips" });
    breaches
        .into_iter()
        .map(|msg| {
            let Some((name, lost)) = bone(&msg) else { return msg };
            // a lost bone is judged by Valve's model alone; a reparented one by both
            let unread = match (&or, &vr) {
                (_, Err(e)) => Some(format!("Valve's model: {e}")),
                (Err(e), _) if !lost => Some(format!("our model: {e}")),
                _ => None,
            };
            if let Some(why) = unread {
                return format!("{msg}; what hangs on it could not be read ({why})");
            }
            let Ok(v) = &vr else { return msg };
            let Some(vi) = v.index(&name) else { return msg };
            if lost {
                return lost_bone(&msg, &name, or.as_ref().ok(), v, vi, &motion, users);
            }
// reparented: judged by what this model hangs on it, since that is what can move differently
            let Ok(o) = &or else { return msg };
            let Some(oi) = o.index(&name) else { return msg };
            let by = norm(sub(o.at(oi), v.at(vi)));
            let turn = match (o.world.get(oi), v.world.get(vi)) {
                (Some(a), Some(b)) => rot_angle(&a.0, &b.0),
                _ => 0.0,
            };
            let bind = format!("at bind it sits {by:.2} units and {turn:.1} degrees from Valve's");
            let m = match &motion {
                Ok(m) => m,
                Err(e) => return format!("{msg}; {bind}, and whether it moves as Valve's could not be measured: its animation could not be read ({e})"),
            };
            let own_skeleton = format!("{}_c", m.skeleton.to_ascii_lowercase());
            if ships(&own_skeleton) {
                return format!("{msg}; {bind}, and the mod ships its own animation skeleton ({own_skeleton}), so how the bone moves is the mod's, which Valve's files cannot judge");
            }
            let bones = following(o, m, oi);
            let mut mine = Load::default();
            for &b in &bones {
                mine.add(&o.load[b]);
            }
            let lever = |skin: bool| {
                bones
                    .iter()
                    .filter(|&&b| if skin { o.load[b].vertices > 0 } else { !o.load[b].is_empty() })
                    .map(|&b| norm(sub(o.at(b), o.at(oi))) + if skin { o.load[b].skin } else { o.load[b].reach })
                    .fold(0.0, f64::max)
            };
            if m.index(&name).is_some() {
                // posed by name in model space: only the vertices bound to it here can land elsewhere, by the
                // difference of the two binds
                let how = "its animation poses it by name, as Valve's, whatever its parent";
                let sweep = by + lever(true) * turn.to_radians();
                return if mine.vertices > 0 && sweep > VIEW_UNIT {
                    format!("{msg}; {bind}, and {how}: the {} vertices this model weights to it were bound where it sits here, and land up to {sweep:.1} units from there", mine.vertices)
                } else if mine.vertices > 0 {
                    format!("{msg}; {bind}, and {how}, so the {} vertices this model weights to it land within a fair-play pixel of where they were made; {NOTHING_CHANGES}", mine.vertices)
                } else {
                    format!("{msg}; {bind}, and {how}; this model weights no vertex to it, so {NOTHING_CHANGES}")
                };
            }
            // the animation skeleton lacks it: it follows its parent, so it moves as Valve's while the two anchors
            // move as one and it sits where Valve's does
            if let Some(why) = m.incomplete() {
                return if mine.is_empty() {
                    format!("{msg}; {bind}, and how its clips move it is not known ({why}); this model hangs nothing on it or on a bone that follows it, so {NOTHING_CHANGES}")
                } else {
                    format!("{msg}; {bind}, and whether it moves as Valve's could not be measured: {why}")
                };
            }
            let (ao, av) = (anchor(o, m, oi), anchor(v, m, vi));
            let side = |a: Option<usize>| a.map_or("the model's origin".to_string(), |a| format!("'{}'", m.names[a]));
            let mv = m.between(ao, av, v.at(vi));
            let apart: f64 = mv.iter().map(|(_, s)| s).sum();
            let how = if apart <= VIEW_UNIT {
                format!("it follows {} and Valve's {}, which move as one in {}", side(ao), side(av), clips(m))
            } else {
                let shown: Vec<String> = mv.iter().take(3).map(|&(x, _)| format!("'{}' {}", m.names[x], m.moved[x].describe())).collect();
                format!("it follows {} where Valve's follows {}, and in {} they move apart ({})", side(ao), side(av), clips(m), shown.join(", "))
            };
            let sweep = (by + lever(false) * turn.to_radians()).max(if apart <= VIEW_UNIT { 0.0 } else { apart });
            if mine.is_empty() {
                format!("{msg}; {bind}, and {how}; this model hangs nothing on it or on a bone that follows it, so {NOTHING_CHANGES}")
            } else if sweep <= VIEW_UNIT {
                format!("{msg}; {bind}, and {how}, so what this model hangs on it ({}) moves as Valve's within a fair-play pixel; {NOTHING_CHANGES}", mine.describe())
            } else {
                format!("{msg}; {bind}, and {how}: what this model hangs on it ({}) moves up to {sweep:.1} units from where Valve's would", mine.describe())
            }
        })        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::mesh::synth;
    use std::collections::HashMap;

    #[test]
    fn a_clip_quaternion_reads_back_whichever_component_is_largest() {
        for q in [[0.9f32, 0.1, -0.3, 0.3], [0.1, -0.95, 0.2, 0.2], [0.2, 0.1, 0.97, -0.1], [0.0, 0.0, 0.258_819, 0.965_926], [-0.5, -0.5, -0.5, 0.5]] {
            let q = q.map(f64::from);
            let n = q.iter().map(|x| x * x).sum::<f64>().sqrt();
            let back = quaternion(&synth::nm_quaternion(q.map(|x| (x / n) as f32)));
            let dot: f64 = q.iter().zip(&back).map(|(a, b)| a * b).sum::<f64>() / n;
            assert!(2.0 * dot.abs().min(1.0).acos().to_degrees() < 0.01, "{q:?} read back as {back:?}");
        }
    }

    #[test]
    fn motion_reads_how_far_every_clip_takes_each_bone() {
        let bones: Vec<synth::NmBone> = vec![("root_motion", -1, [0.0; 3], [0.0, 0.0, 0.0, 1.0]), ("Pelvis", 0, [0.0, 0.0, 40.0], [0.0, 0.0, 0.0, 1.0]), ("ik", 0, [0.0, 10.0, 0.0], [0.0, 0.0, 0.0, 1.0])];
        let rest: Vec<([f32; 4], [f32; 3])> = bones.iter().map(|b| (b.3, b.2)).collect();
        let h = 15f32.to_radians();
        let mut turned = rest.clone();
        turned[1].0 = [h.sin(), 0.0, 0.0, h.cos()];
        turned[2].1 = [0.0, 10.0, 12.5];
        // an additive clip of no change, and one of a skeleton the model does not name, move nothing
        let still = vec![([0.0, 0.0, 0.0, 1.0], [0.0; 3]); 3];
        let files: HashMap<String, Vec<u8>> = HashMap::from([
            ("a/h.vnmskel_c".to_string(), synth::nm_skeleton("a/h.vnmskel", &bones)),
            ("g/top.vnmgraph_c".to_string(), synth::nm_graph(&["g/sub.vnmgraph", "c/run.vnmclip"])),
            ("g/sub.vnmgraph_c".to_string(), synth::nm_graph(&["c/add.vnmclip", "c/other.vnmclip", "c/gone.vnmclip"])),
            ("c/run.vnmclip_c".to_string(), synth::nm_clip("a/h.vnmskel", &[rest.clone(), turned, rest.clone()], false)),
            ("c/add.vnmclip_c".to_string(), synth::nm_clip("a/h.vnmskel", &[still.clone(), still], true)),
            ("c/other.vnmclip_c".to_string(), synth::nm_clip("b/other.vnmskel", &[rest.clone(), rest.iter().map(|(_, p)| ([1.0, 0.0, 0.0, 0.0], *p)).collect()], false)),
        ]);
        let model = synth::model(&synth::SynthModel { nm_skeleton: "a/h.vnmskel".into(), anim_graphs: vec!["g/top.vnmgraph".into()], ..Default::default() });
        let m = motion(&model, &|p: &str| files.get(p).cloned()).unwrap();
        assert_eq!((m.clips, m.unread.len()), (2, 1), "{:?}", m.unread);
        assert!(m.unread[0].starts_with("c/gone.vnmclip_c"));
        let p = m.index("pelvis").unwrap();
        assert_eq!(m.names[p], "Pelvis", "a model's bone takes the pose of its name, case aside");
        assert!((m.moved[p].degrees - 30.0).abs() < 0.01 && m.moved[p].units < 1e-6, "{:?}", m.moved[p]);
        let ik = m.index("ik").unwrap();
        assert!((m.moved[ik].units - 12.5).abs() < 0.01 && m.moved[ik].degrees < 1e-6, "{:?}", m.moved[ik]);
        assert!(m.moved[0].still() && !m.moved[p].still());
        assert_eq!(m.at[p], [0.0, 0.0, 40.0]);
        // the pelvis carries a point 10 units off it by its turn; the IK target moves nothing between root and pelvis
        let mv = m.between(Some(p), None, [0.0, 10.0, 40.0]);
        assert!(mv.len() == 1 && mv[0].0 == p && (mv[0].1 - 10.0 * 30f64.to_radians()).abs() < 0.01, "{mv:?}");
        assert!(m.between(Some(0), None, [0.0; 3]).is_empty());
        // no animation skeleton named, or one not in the game: said, never a quiet "still"
        assert!(motion(&synth::model(&synth::SynthModel::default()), &|_: &str| None).unwrap_err().contains("names no animation skeleton"));
        assert!(motion(&model, &|_: &str| None).unwrap_err().contains("is not in the game"));
    }
}