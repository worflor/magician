//! A compiled model's cloth: the FeModel in its `PHYS` block, read natively (KV3 v4/v5, no Source 2 Viewer).
//!
//! What `resourcecompiler` made of the model's cloth setup. The values here are what the game's solver runs,
//! so they are the only place to check what a document asked for (`goal_strength` g becomes a goal attraction of
//! g^3, `goal_damping` the vertex attraction, `gravity_z` times 360 the gravity, and so on; see the workspace
//! guide's "Cloth compile" fact). `magician cloth` prints it; the cloth lint rules read the same decoder.
//!
//! Layout (checked against Valve's Lash, 75 nodes): nodes are ordered static (pinned to a bone) first, then free
//! (simulated), then position-driven (moved by the animation only, like a rigid piece). `m_NodeIntegrator` has one
//! entry per node; `m_NodeCollisionRadii` one per non-static node (index `node - m_nStaticNodes`);
//! `m_AnimStrayRadii` names a node and its leash length; rods tie two nodes; collision shapes (tapered capsules
//! and spheres) hang on a node and carry a layer mask.

use resource::kv3::Value;
use std::collections::BTreeMap;

/// What the solver does with one node.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Integrator {
    pub damping: f64,
    /// `flAnimationForceAttraction`: how hard the node is pulled to its animated pose (goal strength cubed).
    pub goal: f64,
    /// `flAnimationVertexAttraction`: the goal damping (0 whenever the goal strength is 0).
    pub vertex: f64,
    pub gravity: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rod {
    pub a: usize,
    pub b: usize,
    pub min: f64,
    pub max: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShapeKind {
    Capsule,
    Sphere,
}

/// A collision shape on a node: a tapered capsule (two spheres) or one sphere, and the layers it collides on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shape {
    pub kind: ShapeKind,
    pub node: usize,
    /// `[x, y, z, radius]` of each end (a sphere repeats its one).
    pub ends: [[f64; 4]; 2],
    pub mask: u32,
}

impl Shape {
    /// Distance between the two ends (0 for a sphere).
    pub fn length(&self) -> f64 {
        let d: f64 = (0..3).map(|i| (self.ends[0][i] - self.ends[1][i]).powi(2)).sum();
        d.sqrt()
    }
}

#[derive(Clone, Debug, Default)]
pub struct Cloth {
    pub names: Vec<String>,
    pub node_count: usize,
    pub static_nodes: usize,
    pub rot_lock_static: usize,
    /// First position-driven node (`node_count` when none).
    pub first_driven: usize,
    pub integrator: Vec<Integrator>,
    pub collision_radii: Vec<f64>,
    /// Node to leash length (`m_AnimStrayRadii`).
    pub stray: BTreeMap<usize, f64>,
    pub rods: Vec<Rod>,
    pub shapes: Vec<Shape>,
    /// The per-node collision layer masks of the collision tree (`m_TreeCollisionMasks`).
    pub tree_masks: Vec<u32>,
    /// Each node at rest in model space (`m_InitPose`): position and rotation (x, y, z, w). Empty when the model
    /// carries none.
    pub rest: Vec<Rest>,
    /// `m_NodeInvMasses`, one per node: 0 for a node the solver never moves.
    pub inv_mass: Vec<f64>,
}

/// A node's rest transform in model space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rest {
    pub pos: [f64; 3],
    pub rot: [f64; 4],
}

impl Rest {
    /// A point of this node's frame in model space: `pos + rot * p`.
    pub fn apply(&self, p: [f64; 3]) -> [f64; 3] {
        let [x, y, z, w] = self.rot;
        // v' = v + 2 q x (q x v + w v), q the vector part
        let c = [y * p[2] - z * p[1] + w * p[0], z * p[0] - x * p[2] + w * p[1], x * p[1] - y * p[0] + w * p[2]];
        [self.pos[0] + p[0] + 2.0 * (y * c[2] - z * c[1]), self.pos[1] + p[1] + 2.0 * (z * c[0] - x * c[2]), self.pos[2] + p[2] + 2.0 * (x * c[1] - y * c[0])]
    }
}

/// Where a node sits in the solve order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Static,
    Free,
    Driven,
}

impl Cloth {
    pub fn kind(&self, node: usize) -> Kind {
        if node < self.static_nodes {
            Kind::Static
        } else if node >= self.first_driven {
            Kind::Driven
        } else {
            Kind::Free
        }
    }
    pub fn name(&self, node: usize) -> &str {
        self.names.get(node).map_or("?", String::as_str)
    }
    /// Collision radius of a non-static node (0 for a static one, which does not collide as a node).
    pub fn radius(&self, node: usize) -> f64 {
        node.checked_sub(self.static_nodes).and_then(|i| self.collision_radii.get(i)).copied().unwrap_or(0.0)
    }
    /// A node's rest position in model space, when the model carries a finite one.
    pub fn rest_pos(&self, node: usize) -> Option<[f64; 3]> {
        self.rest.get(node).map(|r| r.pos).filter(|p| p.iter().all(|x| x.is_finite()))
    }
    /// The layers a node collides on (`m_TreeCollisionMasks`; every layer when the model carries no mask for it).
    pub fn node_mask(&self, node: usize) -> u32 {
        self.tree_masks.get(node).copied().unwrap_or(0xF)
    }
    /// A collision shape's two ends in model space at rest, each `[x, y, z, radius]`: the shape hangs in the frame of
    /// its node. None when that node has no rest transform.
    pub fn shape_at_rest(&self, s: &Shape) -> Option<[[f64; 4]; 2]> {
        let r = self.rest.get(s.node)?;
        let end = |e: [f64; 4]| {
            let p = r.apply([e[0], e[1], e[2]]);
            [p[0], p[1], p[2], e[3]]
        };
        let out = [end(s.ends[0]), end(s.ends[1])];
        out.iter().flatten().all(|x| x.is_finite()).then_some(out)
    }
}

/// Decode the FeModel in a compiled model's `PHYS` block. `Ok(None)` when the model has no cloth (no `PHYS`
/// block, or one with no `m_pFeModel`).
pub fn read(model: &[u8]) -> Result<Option<Cloth>, String> {
    match resource::kv3_block(model, b"PHYS") {
        None => Ok(None),
        Some(Err(e)) => Err(format!("PHYS: {e}")),
        Some(Ok(phys)) => Ok(from_value(&phys)),
    }
}

fn count(v: &Value, key: &str) -> usize {
    v.get(key).and_then(Value::as_f64).map_or(0, |x| x as usize)
}

/// The FeModel of a decoded `PHYS` block.
pub fn from_value(phys: &Value) -> Option<Cloth> {
    let fe = phys.get("m_pFeModel")?;
    let names: Vec<String> = fe.get("m_CtrlName").map(|n| n.as_array().iter().filter_map(|s| s.as_str().map(str::to_string)).collect()).unwrap_or_default();
    let node_count = count(fe, "m_nNodeCount").max(if fe.get("m_nNodeCount").is_some() { 0 } else { names.len() });
    let first_driven = match count(fe, "m_nFirstPositionDrivenNode") {
        0 => node_count,
        n => n.min(node_count),
    };
    let nums = |key: &str| -> Vec<f64> { fe.get(key).map(Value::numbers).unwrap_or_default() };
    let f = |o: &Value, k: &str| o.get(k).and_then(Value::as_f64).unwrap_or(0.0);
    let integrator = fe
        .get("m_NodeIntegrator")
        .map(|a| {
            a.as_array()
                .iter()
                .map(|n| Integrator { damping: f(n, "flPointDamping"), goal: f(n, "flAnimationForceAttraction"), vertex: f(n, "flAnimationVertexAttraction"), gravity: f(n, "flGravity") })
                .collect()
        })
        .unwrap_or_default();
    let pair = |o: &Value| -> Option<(usize, usize)> {
        let n = o.get("nNode")?.numbers();
        Some((*n.first()? as usize, n.get(1).map_or(*n.first()? as usize, |x| *x as usize)))
    };
    let stray = fe
        .get("m_AnimStrayRadii")
        .map(|a| a.as_array().iter().filter_map(|s| pair(s).map(|(n, _)| (n, f(s, "flMaxDist")))).collect())
        .unwrap_or_default();
    let rods = fe
        .get("m_Rods")
        .map(|a| a.as_array().iter().filter_map(|r| pair(r).map(|(a, b)| Rod { a, b, min: f(r, "flMinDist"), max: f(r, "flMaxDist") })).collect())
        .unwrap_or_default();
    let mut shapes = Vec::new();
    for s in fe.get("m_TaperedCapsuleRigids").map(Value::as_array).unwrap_or_default() {
        let ends: Vec<[f64; 4]> = s.get("vSphere").map(|v| v.as_array().iter().map(|e| to4(&e.numbers())).collect()).unwrap_or_default();
        if ends.len() == 2 {
            shapes.push(Shape { kind: ShapeKind::Capsule, node: f(s, "nNode") as usize, ends: [ends[0], ends[1]], mask: f(s, "nCollisionMask") as u32 });
        }
    }
    for s in fe.get("m_SphereRigids").map(Value::as_array).unwrap_or_default() {
        if let Some(v) = s.get("vSphere") {
            let e = to4(&v.numbers());
            shapes.push(Shape { kind: ShapeKind::Sphere, node: f(s, "nNode") as usize, ends: [e, e], mask: f(s, "nCollisionMask") as u32 });
        }
    }
    Some(Cloth {
        names,
        node_count,
        static_nodes: count(fe, "m_nStaticNodes"),
        rot_lock_static: count(fe, "m_nRotLockStaticNodes"),
        first_driven,
        integrator,
        collision_radii: nums("m_NodeCollisionRadii"),
        stray,
        rods,
        shapes,
        tree_masks: nums("m_TreeCollisionMasks").into_iter().map(|x| x as u32).collect(),
        rest: fe
            .get("m_InitPose")
            .map(|a| {
                a.as_array()
                    .iter()
                    .map(|t| {
                        let n = t.numbers();
                        let g = |i: usize, d: f64| n.get(i).copied().unwrap_or(d);
                        Rest { pos: [g(0, 0.0), g(1, 0.0), g(2, 0.0)], rot: unit([g(4, 0.0), g(5, 0.0), g(6, 0.0), g(7, 1.0)]) }
                    })
                    .collect()
            })
            .unwrap_or_default(),
        inv_mass: nums("m_NodeInvMasses"),
    })
}

/// A quaternion made unit length (identity when it has none, or is not finite).
fn unit(q: [f64; 4]) -> [f64; 4] {
    let l = q.iter().map(|x| x * x).sum::<f64>().sqrt();
    if l.is_finite() && l > 1e-9 { q.map(|x| x / l) } else { [0.0, 0.0, 0.0, 1.0] }
}

fn to4(n: &[f64]) -> [f64; 4] {
    [n.first().copied().unwrap_or(0.0), n.get(1).copied().unwrap_or(0.0), n.get(2).copied().unwrap_or(0.0), n.get(3).copied().unwrap_or(0.0)]
}

/// The chain a node belongs to: its bone name without the joint counter, the compiler's `$cc` prefix of an extruded
/// joint, its side marker moved to the end and a trailing `_end`: `$ccmaid_train_l_3_0` and `maid_train_l_end` are both
/// `maid_train_l`; `hair_0_L` is `hair_L`; the proxy sheet nodes `$cloth_m0p12` are all `$cloth_m0`.
pub fn chain_key(name: &str) -> String {
    // the compiler's proxy sheet of a mesh (`$cloth_m0p12`: mesh 0, particle 12) is one group
    if let Some((m, p)) = name.strip_prefix("$cloth_m").and_then(|r| r.split_once('p')) {
        if !m.is_empty() && !p.is_empty() && m.bytes().chain(p.bytes()).all(|c| c.is_ascii_digit()) {
            return format!("$cloth_m{m}");
        }
    }
    let n = name.strip_prefix("$cc").unwrap_or(name);
    let (body, side) = match n.rsplit_once('_') {
        Some((b, s)) if s.len() == 1 && "LRlr".contains(s) => (b, n.get(b.len()..).unwrap_or("")),
        _ => (n, ""),
    };
    let mut b = body;
    while let Some((head, tail)) = b.rsplit_once('_') {
        if !tail.is_empty() && tail.bytes().all(|c| c.is_ascii_digit()) {
            b = head;
        } else {
            break;
        }
    }
    let key = if b.len() != body.len() { format!("{b}{side}") } else { n.to_string() };
    if let Some(p) = key.strip_suffix("_end") {
        p.to_string()
    } else if let Some(p) = key.strip_suffix("_end_L") {
        format!("{p}_L")
    } else if let Some(p) = key.strip_suffix("_end_R") {
        format!("{p}_R")
    } else {
        key
    }
}

/// The nodes of one chain and how they are driven.
#[derive(Clone, Debug, Default)]
pub struct Chain {
    pub key: String,
    pub nodes: Vec<usize>,
    pub static_nodes: usize,
    pub driven: usize,
    pub free: usize,
    /// Over the free nodes: goal attraction (mean, max), vertex attraction (mean, max), gravity (mean).
    pub goal_mean: f64,
    pub goal_max: f64,
    pub vertex_mean: f64,
    pub vertex_max: f64,
    pub gravity_mean: f64,
    /// Over the free nodes: collision radius (mean, max).
    pub radius_mean: f64,
    pub radius_max: f64,
    /// Leashed nodes and their mean leash length.
    pub stray: usize,
    pub stray_mean: f64,
    /// Rods with both ends in the chain, and with one end in it and the other elsewhere.
    pub rods: usize,
    pub rods_out: usize,
    pub capsules: usize,
    pub spheres: usize,
    /// Union of the layer masks of the shapes on the chain's nodes.
    pub layers: u32,
}

/// Nodes grouped into chains, in order of first appearance, with their solver values. A lone static node is a
/// body bone that a collision shape hangs on (an anchor), not a chain: it is returned in the second list.
pub fn chains(c: &Cloth) -> (Vec<Chain>, Vec<usize>) {
    let mut order: Vec<String> = Vec::new();
    let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for i in 0..c.node_count.min(c.names.len()) {
        let key = chain_key(&c.names[i]);
        if !groups.contains_key(&key) {
            order.push(key.clone());
        }
        groups.entry(key).or_default().push(i);
    }
    let key_of = |n: usize| c.names.get(n).map(|s| chain_key(s));
    let (mut out, mut anchors) = (Vec::new(), Vec::new());
    for key in order {
        let nodes = groups.remove(&key).unwrap_or_default();
        if nodes.len() == 1 && c.kind(nodes[0]) == Kind::Static {
            anchors.push(nodes[0]);
            continue;
        }
        let mut ch = Chain { key: key.clone(), nodes: nodes.clone(), ..Default::default() };
        let (mut goal, mut vertex, mut grav, mut rad, mut stray) = (0.0, 0.0, 0.0, 0.0, 0.0);
        for &n in &nodes {
            match c.kind(n) {
                Kind::Static => ch.static_nodes += 1,
                Kind::Driven => ch.driven += 1,
                Kind::Free => {
                    ch.free += 1;
                    let it = c.integrator.get(n).copied().unwrap_or_default();
                    goal += it.goal;
                    vertex += it.vertex;
                    grav += it.gravity;
                    ch.goal_max = ch.goal_max.max(it.goal);
                    ch.vertex_max = ch.vertex_max.max(it.vertex);
                    let r = c.radius(n);
                    rad += r;
                    ch.radius_max = ch.radius_max.max(r);
                }
            }
            if let Some(s) = c.stray.get(&n) {
                ch.stray += 1;
                stray += s;
            }
        }
        if ch.free > 0 {
            let k = ch.free as f64;
            (ch.goal_mean, ch.vertex_mean, ch.gravity_mean, ch.radius_mean) = (goal / k, vertex / k, grav / k, rad / k);
        }
        if ch.stray > 0 {
            ch.stray_mean = stray / ch.stray as f64;
        }
        for r in &c.rods {
            match (key_of(r.a).as_deref() == Some(&key), key_of(r.b).as_deref() == Some(&key)) {
                (true, true) => ch.rods += 1,
                (true, false) | (false, true) => ch.rods_out += 1,
                _ => {}
            }
        }
        for s in c.shapes.iter().filter(|s| key_of(s.node).as_deref() == Some(&key)) {
            match s.kind {
                ShapeKind::Capsule => ch.capsules += 1,
                ShapeKind::Sphere => ch.spheres += 1,
            }
            ch.layers |= s.mask;
        }
        out.push(ch);
    }
    (out, anchors)
}

/// A layer mask as the layers it names: `0xF` is `0-3`, `0x5` is `0,2`, 0 is `none`.
pub fn layers(mask: u32) -> String {
    if mask == 0 {
        return "none".into();
    }
    let mut parts: Vec<String> = Vec::new();
    let mut i = 0u32;
    while i < 32 {
        if mask >> i & 1 == 1 {
            let start = i;
            while i + 1 < 32 && mask >> (i + 1) & 1 == 1 {
                i += 1;
            }
            parts.push(if i > start + 1 { format!("{start}-{i}") } else if i == start + 1 { format!("{start},{i}") } else { start.to_string() });
        }
        i += 1;
    }
    parts.join(",")
}

/// A synthetic compiled model with a `PHYS` block holding the given FeModel, for fixtures (`kv3::encode` writes the
/// block, `resource::synth_with` wraps it).
pub fn synth_model(fe: Value) -> Vec<u8> {
    let phys = Value::Object(vec![("m_pFeModel".into(), fe)]);
    resource::synth_with(&[], &[(*b"PHYS", resource::kv3::encode(&phys))])
}

// ---- fixtures ----

fn obj(m: &[(&str, Value)]) -> Value {
    Value::Object(m.iter().map(|(k, v)| (k.to_string(), v.clone())).collect())
}
fn nums(v: &[f64]) -> Value {
    Value::Array(v.iter().map(|x| Value::Double(*x)).collect())
}
fn int(i: i64) -> Value {
    Value::Int(i)
}
fn integ(goal: f64, vertex: f64, gravity: f64) -> Value {
    obj(&[("flPointDamping", Value::Double(0.0)), ("flAnimationForceAttraction", Value::Double(goal)), ("flAnimationVertexAttraction", Value::Double(vertex)), ("flGravity", Value::Double(gravity))])
}

/// A small FeModel for fixtures (here and in `magician cloth`): nodes 0 `glove` (anchor), 1-2 `skirt_0/1` static, 3-4 `skirt_2/3` free, 5 `$ccribbon_0_0` free, 6 `$ccribbon_0_1` free, 7 `skirt_end` driven.
pub fn fixture() -> Value {
    let names = ["glove", "skirt_0", "skirt_1", "skirt_2", "skirt_3", "$ccribbon_0_0", "$ccribbon_0_1", "skirt_end"];
    obj(&[
        ("m_CtrlName", Value::Array(names.iter().map(|s| Value::Str(s.to_string())).collect())),
        ("m_nNodeCount", int(8)),
        ("m_nStaticNodes", int(3)),
        ("m_nRotLockStaticNodes", int(1)),
        ("m_nFirstPositionDrivenNode", int(7)),
        (
            "m_NodeIntegrator",
            Value::Array(vec![integ(0.0, 0.0, 360.0), integ(0.0, 0.0, 360.0), integ(0.0, 0.0, 360.0), integ(0.125, 0.25, 360.0), integ(0.0270000007, 0.5, 180.0), integ(0.5, 0.125, 90.0), integ(0.5, 0.125, 90.0), integ(1.0, 0.0, 0.0)]),
        ),
        ("m_NodeCollisionRadii", nums(&[1.5, 3.5, 2.0, 2.0, 0.0])),
        ("m_AnimStrayRadii", Value::Array(vec![obj(&[("nNode", Value::Array(vec![int(3), int(2)])), ("flMaxDist", Value::Double(4.0))])])),
        (
            "m_Rods",
            Value::Array(vec![
                obj(&[("nNode", Value::Array(vec![int(1), int(2)])), ("flMinDist", Value::Double(9.0)), ("flMaxDist", Value::Double(10.0))]),
                obj(&[("nNode", Value::Array(vec![int(2), int(3)])), ("flMinDist", Value::Double(9.0)), ("flMaxDist", Value::Double(9.5))]),
                obj(&[("nNode", Value::Array(vec![int(3), int(4)])), ("flMinDist", Value::Double(8.0)), ("flMaxDist", Value::Double(8.0))]),
            ]),
        ),
        (
            "m_TaperedCapsuleRigids",
            Value::Array(vec![obj(&[
                ("vSphere", Value::Array(vec![nums(&[0.0, 0.0, 0.0, 5.0]), nums(&[0.0, 12.0, 5.0, 6.5])])),
                ("nNode", int(0)),
                ("nCollisionMask", int(15)),
            ])]),
        ),
        ("m_SphereRigids", Value::Array(vec![obj(&[("vSphere", nums(&[1.0, 2.0, 3.0, 4.0])), ("nNode", int(2)), ("nCollisionMask", int(5))])])),
        ("m_TreeCollisionMasks", nums(&[15.0, 15.0, 5.0])),
        // rest: the glove at (0, 0, 40) turned a quarter about z, the others down the z axis
        (
            "m_InitPose",
            Value::Array(
                std::iter::once(nums(&[0.0, 0.0, 40.0, 1.0, 0.0, 0.0, std::f64::consts::FRAC_1_SQRT_2, std::f64::consts::FRAC_1_SQRT_2]))
                    .chain((1..8).map(|k| nums(&[0.0, 0.0, 40.0 - 5.0 * k as f64, 1.0, 0.0, 0.0, 0.0, 1.0])))
                    .collect(),
            ),
        ),
        ("m_NodeInvMasses", nums(&[0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0])),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_fe_model_of_a_compiled_model() {
        let model = synth_model(fixture());
        let c = read(&model).unwrap().unwrap();
        assert_eq!((c.node_count, c.static_nodes, c.rot_lock_static, c.first_driven), (8, 3, 1, 7));
        assert_eq!(c.names[5], "$ccribbon_0_0");
        assert_eq!(c.integrator[3], Integrator { damping: 0.0, goal: 0.125, vertex: 0.25, gravity: 360.0 });
        assert_eq!(c.collision_radii, vec![1.5, 3.5, 2.0, 2.0, 0.0]);
        assert_eq!(c.radius(3), 1.5);
        assert_eq!(c.radius(1), 0.0, "a static node has no collision radius of its own");
        assert_eq!(c.stray.get(&3), Some(&4.0));
        assert_eq!(c.rods.len(), 3);
        assert_eq!(c.shapes.len(), 2);
        let cap = c.shapes[0];
        assert_eq!((cap.kind, cap.node, cap.mask), (ShapeKind::Capsule, 0, 15));
        assert!((cap.length() - 13.0).abs() < 1e-9, "(0,0,0) to (0,12,5)");
        assert_eq!(c.shapes[1].kind, ShapeKind::Sphere);
        assert_eq!(c.tree_masks, vec![15, 15, 5]);
        // the rest pose: a shape hangs in its node's frame, so the glove's capsule end (0, 12, 5) turns with it
        assert_eq!((c.rest.len(), c.inv_mass.len()), (8, 8));
        assert_eq!(c.rest_pos(3), Some([0.0, 0.0, 25.0]));
        let at = c.shape_at_rest(&cap).unwrap();
        let near = |a: [f64; 4], b: [f64; 4]| a.iter().zip(&b).all(|(x, y)| (x - y).abs() < 1e-9);
        assert!(near(at[0], [0.0, 0.0, 40.0, 5.0]) && near(at[1], [-12.0, 0.0, 45.0, 6.5]), "{at:?}");
        assert_eq!((c.node_mask(2), c.node_mask(6)), (5, 15), "a node past the mask list collides on every layer");
        assert_eq!((c.kind(0), c.kind(3), c.kind(7)), (Kind::Static, Kind::Free, Kind::Driven));
        // a model with no PHYS block, or a PHYS with no FeModel, has no cloth; garbage is an error
        assert!(read(&resource::synth(&[])).unwrap().is_none());
        assert!(from_value(&Value::Object(vec![])).is_none());
        let mut bad = model.clone();
        let at = bad.windows(4).position(|w| w == b"\x053VK").unwrap();
        bad[at + 1] = b'X';
        assert!(read(&bad).is_ok(), "an unreadable block is not KV3, so it is not cloth");
    }

    #[test]
    fn nodes_group_into_chains_with_the_solver_values_of_their_free_nodes() {
        let c = from_value(&Value::Object(vec![("m_pFeModel".into(), fixture())])).unwrap();
        let (chains, anchors) = chains(&c);
        assert_eq!(anchors, vec![0], "a lone static node is a shape's anchor, not a chain");
        let by = |k: &str| chains.iter().find(|c| c.key == k).unwrap_or_else(|| panic!("no chain {k}: {:?}", chains.iter().map(|c| &c.key).collect::<Vec<_>>()));
        let skirt = by("skirt");
        assert_eq!((skirt.nodes.len(), skirt.static_nodes, skirt.free, skirt.driven), (5, 2, 2, 1));
        assert!((skirt.goal_mean - (0.125 + 0.027) / 2.0).abs() < 1e-9 && (skirt.goal_max - 0.125).abs() < 1e-9);
        assert!((skirt.vertex_mean - 0.375).abs() < 1e-9 && skirt.vertex_max == 0.5);
        assert!((skirt.gravity_mean - 270.0).abs() < 1e-9);
        assert!((skirt.radius_mean - 2.5).abs() < 1e-9 && skirt.radius_max == 3.5, "radii of nodes 3 and 4 are {:?}", (c.radius(3), c.radius(4)));
        assert_eq!((skirt.stray, skirt.stray_mean), (1, 4.0));
        assert_eq!((skirt.rods, skirt.rods_out), (3, 0));
        assert_eq!(skirt.layers, 5, "the sphere on node 2 collides on layers 0 and 2");
        assert_eq!((skirt.capsules, skirt.spheres), (0, 1));
        let rib = by("ribbon");
        assert_eq!((rib.nodes.clone(), rib.free), (vec![5, 6], 2), "an extruded joint's two side nodes are one chain");
        assert!((rib.goal_mean - 0.5).abs() < 1e-9);
    }

    #[test]
    fn chain_keys_drop_counters_prefixes_and_ends() {
        for (n, k) in [
            ("$ccmaid_train_l_3_0", "maid_train_l"),
            ("maid_train_l_3", "maid_train_l"),
            ("maid_train_l_end", "maid_train_l"),
            ("hair_0_L", "hair_L"),
            ("hair_end_L", "hair_L"),
            ("$cloth_m0p12", "$cloth_m0"),
            ("$cloth_m1p3", "$cloth_m1"),
            ("$cloth_mesh", "$cloth_mesh"),
            ("glove", "glove"),
            ("arm_lower_R", "arm_lower_R"),
            ("neck_0", "neck"),
            ("x_1_2_3", "x"),
        ] {
            assert_eq!(chain_key(n), k, "{n}");
        }
    }

    #[test]
    fn layer_masks_read_as_layers() {
        assert_eq!(layers(0), "none");
        assert_eq!(layers(15), "0-3");
        assert_eq!(layers(5), "0,2");
        assert_eq!(layers(1), "0");
        assert_eq!(layers(0b11), "0,1");
        assert_eq!(layers(0b1101), "0,2,3");
    }
}
