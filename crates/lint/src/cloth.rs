//! M141 cloth-flutter, "crunchy fabric": every simulated cloth node of a compiled model judged as a damped
//! oscillator, from the compiled FeModel in the `PHYS` block.
//!
//! The solver model is the one Maid Lash's and another skin's offline proofs run (docs/reference/engine.md, Cloth): per step,
//! dt = 1/60 s, `x' = x + v + F (a - x) + V (va - v) - g dt^2`, then the rods. F is `flAnimationForceAttraction` (the
//! goal spring), V `flAnimationVertexAttraction` (velocity matching, the only damper), g `flGravity`. Linearised about the
//! animated pose, a node hanging on a length `arc` of cloth (its shortest rod path to a static node) swings with an extra
//! pendulum stiffness `kappa = g dt^2 / arc` per step, so its offset obeys
//!   `e[n+1] = (2 - F - V - kappa) e[n] - (1 - V) e[n-1]`
//! whose roots `r e^(+-i theta)` give, exactly for the discrete solver: frequency `theta / (2 pi dt)`, damping ratio
//! `zeta = -ln r / sqrt(ln^2 r + theta^2)`, and the swing cycles before the envelope falls under 10%,
//! `(ln 0.1 / ln r) theta / (2 pi)`. Real roots: no swing. `V = 0`: it never settles (the goal-0 case, docs/reference/engine.md).
//! Nodes with `F >= 0.9` are held (a pin), not judged.
//!
//! Groups are the connected sets of simulated nodes (rods), so a proxy sheet is one group and each chain its own.


pub const DT: f64 = 1.0 / 60.0;
/// A group rings when its median node swings at least this many times before settling under 10%. Valve's cloth
/// across its heroes (build 6864, 339 groups): median 1.7, 90th percentile 5.4.
pub const RING_CYCLES: f64 = 4.0;
/// ...at a frequency in or above the body's own motion (a walk's stride is about 1 Hz, a sprint's 2): a slower sway
/// is a cape billowing, not flutter.
pub const FLUTTER_HZ: f64 = 1.0;
/// What a fabric that looks heavy needs: one small overshoot (damping ratio 0.35 settles within about one swing).
pub const HEAVY_ZETA: f64 = 0.35;
/// Ours is "worse than Valve's" when it rings this many more cycles than Valve's group at the same place.
pub const WORSE_CYCLES: f64 = 0.5;
/// Valve's settings under a different mesh: the area of render mesh bound to the group grew by this factor.
pub const MESH_GROWTH: f64 = 1.15;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Osc {
    pub freq: f64,
    pub zeta: f64,
    pub cycles: f64,
}

pub fn oscillator(f: f64, v: f64, g: f64, arc: f64) -> Osc {
    let kap = if arc > 0.0 && arc.is_finite() { g * DT * DT / arc } else { 0.0 };
    let b = 2.0 - f - v - kap;
    let c = 1.0 - v;
    if c <= 0.0 || b * b - 4.0 * c >= 0.0 {
        return Osc { freq: 0.0, zeta: 1.0, cycles: 0.0 };
    }
    let r = c.sqrt();
    let th = (b / (2.0 * r)).clamp(-1.0, 1.0).acos();
    let lr = r.ln();
    let freq = th / (2.0 * std::f64::consts::PI * DT);
    if lr >= 0.0 {
        return Osc { freq, zeta: 0.0, cycles: f64::INFINITY };
    }
    Osc { freq, zeta: -lr / (lr * lr + th * th).sqrt(), cycles: (0.1f64.ln() / lr) * th / (2.0 * std::f64::consts::PI) }
}

/// The velocity matching V that gives damping ratio `zeta` for a node (bisection on `oscillator`).
pub fn v_for_zeta(f: f64, g: f64, arc: f64, zeta: f64) -> f64 {
    let (mut lo, mut hi) = (0.0f64, 0.95f64);
    for _ in 0..60 {
        let mid = 0.5 * (lo + hi);
        if oscillator(f, mid, g, arc).zeta < zeta {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

#[derive(Debug, Clone, Default)]
pub struct Node {
    pub name: String,
    pub f: f64,
    pub v: f64,
    pub g: f64,
    pub radius: f64,
    pub mask: u32,
    pub arc: f64,
}

#[derive(Debug, Clone, Default)]
pub struct Cloth {
    /// Simulated nodes only (static and position-driven ones left out).
    pub nodes: Vec<Node>,
    /// Groups of indices into `nodes`.
    pub groups: Vec<Vec<usize>>,
    /// OR of every collision shape's layer mask, and how many shapes.
    pub shape_mask: u32,
    pub shapes: usize,
}

/// Why a model's cloth can't be judged, when it has a PHYS block Magician can't decode (zstd: Lady Geist's, Celeste's).
pub fn unreadable(data: &[u8]) -> Option<String> {
    mesh::cloth::read(data).err().map(|e| format!("cloth not checked (M141): the PHYS block does not decode ({e})"))
}

/// Every node's arc: its shortest rod path (rest lengths) to a static node; infinite where no rod path reaches one.
pub fn arcs(fe: &mesh::cloth::Cloth) -> Vec<f64> {
    let n = fe.node_count.max(fe.names.len());
    let ns = fe.static_nodes.min(n);
    let adj = adjacency(fe, n);
    let mut dist = vec![f64::INFINITY; n];
    let mut done = vec![false; n];
    for d in dist.iter_mut().take(ns) {
        *d = 0.0;
    }
    loop {
        let mut best = None;
        for i in 0..n {
            if !done[i] && dist[i].is_finite() && best.is_none_or(|b: usize| dist[i] < dist[b]) {
                best = Some(i);
            }
        }
        let Some(u) = best else { break };
        done[u] = true;
        for &(w, l) in &adj[u] {
            if dist[u] + l < dist[w] {
                dist[w] = dist[u] + l;
            }
        }
    }
    dist
}

/// Each node's rods: (the other end, the rod's longest length).
fn adjacency(fe: &mesh::cloth::Cloth, n: usize) -> Vec<Vec<(usize, f64)>> {
    let mut adj: Vec<Vec<(usize, f64)>> = vec![Vec::new(); n];
    for r in &fe.rods {
        if r.a < n && r.b < n {
            adj[r.a].push((r.b, r.max));
            adj[r.b].push((r.a, r.max));
        }
    }
    adj
}

/// How far a node sits off its animated pose while the hero accelerates at `accel` units/s^2, in the solver model
/// above: the steady offset `accel dt^2 / (F + kappa)`. Infinite for a node with neither goal nor pendulum (it never
/// comes back). Against a node's leash (stray radius) it says whether the node rides the leash through a move: a
/// ratio over 1 is a node pressed to its leash the whole time the hero accelerates, which reads as stiff, however soft
/// its goal.
pub fn offset_under(accel: f64, f: f64, g: f64, arc: f64) -> f64 {
    let kap = if arc > 0.0 && arc.is_finite() { (g * DT * DT / arc).max(0.0) } else { 0.0 };
    let k = f + kap;
    if k <= 0.0 {
        return f64::INFINITY;
    }
    accel * DT * DT / k
}

/// The simulated nodes and their groups, from the native FeModel decoder (`mesh::cloth`), or None when the model has
/// no cloth (or its PHYS block does not decode: see `unreadable`).
pub fn read(data: &[u8]) -> Option<Cloth> {
    let fe = mesh::cloth::read(data).ok()??;
    let n = fe.node_count.max(fe.names.len());
    let (ns, fpd) = (fe.static_nodes.min(n), fe.first_driven.min(n));
    let dist = arcs(&fe);
    let adj = adjacency(&fe, n);
    let mut c = Cloth::default();
    let mut index = vec![usize::MAX; n];
    for i in ns..fpd {
        let it = fe.integrator.get(i).copied().unwrap_or_default();
        index[i] = c.nodes.len();
        c.nodes.push(Node {
            name: fe.name(i).to_string(),
            f: it.goal,
            v: it.vertex,
            g: it.gravity,
            radius: fe.radius(i),
            mask: fe.tree_masks.get(i).copied().unwrap_or(0xF),
            arc: dist[i],
        });
    }
    let mut seen = vec![false; n];
    for i in ns..fpd {
        if seen[i] {
            continue;
        }
        let (mut grp, mut stack) = (Vec::new(), vec![i]);
        seen[i] = true;
        while let Some(u) = stack.pop() {
            grp.push(index[u]);
            for &(w, _) in &adj[u] {
                if w >= ns && w < fpd && !seen[w] {
                    seen[w] = true;
                    stack.push(w);
                }
            }
        }
        grp.sort_unstable();
        c.groups.push(grp);
    }
    for s in &fe.shapes {
        c.shape_mask |= s.mask;
        c.shapes += 1;
    }
    Some(c)
}

#[derive(Debug, Clone, Default)]
pub struct Verdict {
    pub label: String,
    pub nodes: usize,
    pub freq: f64,
    pub zeta: f64,
    pub cycles: f64,
    pub cycles_max: f64,
    pub has_radius: bool,
    pub shapes_on_layer: bool,
    /// Median V a heavy-looking fabric needs (zeta HEAVY_ZETA) and the median V it has.
    pub v_needed: f64,
    pub v_now: f64,
    pub names: Vec<String>,
    pub params: Vec<[f64; 4]>,
}

fn median(mut a: Vec<f64>) -> f64 {
    if a.is_empty() {
        return 0.0;
    }
    a.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
    a[a.len() / 2]
}

/// One verdict per group (held nodes left out; a group of held nodes only is not judged).
pub fn verdicts(c: &Cloth) -> Vec<Verdict> {
    let mut out = Vec::new();
    for g in &c.groups {
        let free: Vec<&Node> = g.iter().map(|&i| &c.nodes[i]).filter(|n| n.f < 0.9).collect();
        if free.is_empty() {
            continue;
        }
        let osc: Vec<Osc> = free.iter().map(|n| oscillator(n.f, n.v, n.g, n.arc)).collect();
        let mut names: Vec<String> = free.iter().map(|n| n.name.clone()).collect();
        names.sort();
        out.push(Verdict {
            label: label(&names),
            nodes: free.len(),
            freq: median(osc.iter().map(|o| o.freq).collect()),
            zeta: median(osc.iter().map(|o| o.zeta).collect()),
            cycles: median(osc.iter().map(|o| o.cycles.min(999.0)).collect()),
            cycles_max: osc.iter().map(|o| o.cycles.min(999.0)).fold(0.0, f64::max),
            has_radius: free.iter().any(|n| n.radius > 0.0),
            shapes_on_layer: c.shapes > 0 && free.iter().any(|n| n.mask & c.shape_mask & 0xF != 0),
            v_needed: median(free.iter().map(|n| v_for_zeta(n.f, n.g, n.arc, HEAVY_ZETA).max(n.v)).collect()),
            v_now: median(free.iter().map(|n| n.v).collect()),
            names,
            params: free.iter().map(|n| [n.f, n.v, n.g, n.radius]).collect(),
        });
    }
    out
}

/// A readable name for a group: the chain's common prefix, or the proxy's first node and the count.
fn label(names: &[String]) -> String {
    let first = names.first().cloned().unwrap_or_default();
    if names.len() == 1 {
        return first;
    }
    let mut p = first.clone();
    for n in names {
        while !n.starts_with(&p) {
            p.pop();
        }
    }
    let p = p.trim_end_matches(|c: char| c == '_' || c.is_ascii_digit()).to_string();
    if p.len() >= 3 && !p.starts_with("$cloth") {
        format!("{p}* ({} nodes)", names.len())
    } else {
        format!("{first} and {} more", names.len() - 1)
    }
}

fn rings(v: &Verdict) -> bool {
    v.cycles >= RING_CYCLES && v.freq >= FLUTTER_HZ
}

/// A group whose median node has no damper at all. The two roots of a swinging node multiply to `1 - V`, so its
/// swing shrinks by `sqrt(1 - V)` each step whatever the goal, gravity and length are: at `V = 0` that is exactly 1 and
/// the sway never dies, however slow it is. The compiler writes `V = 0` for every joint whose goal strength is 0.
fn never_settles(v: &Verdict) -> bool {
    v.freq > 0.0 && v.zeta == 0.0
}

/// Valve's group at the same place: the one sharing most node names (at least half of ours).
fn counterpart<'a>(ours: &Verdict, valve: &'a [Verdict]) -> Option<&'a Verdict> {
    valve
        .iter()
        .map(|v| (v, ours.names.iter().filter(|n| v.names.binary_search(n).is_ok()).count()))
        .filter(|(_, k)| *k * 2 >= ours.names.len() && *k > 0)
        .max_by_key(|(_, k)| *k)
        .map(|(v, _)| v)
}

fn same_numbers(a: &Verdict, b: &Verdict) -> bool {
    if a.names != b.names || a.params.len() != b.params.len() {
        return false;
    }
    let mut pa = a.params.clone();
    let mut pb = b.params.clone();
    let key = |x: &[f64; 4], y: &[f64; 4]| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal);
    pa.sort_by(key);
    pb.sort_by(key);
    pa.iter().zip(&pb).all(|(x, y)| x.iter().zip(y).all(|(p, q)| (p - q).abs() <= 1e-3 * q.abs().max(1e-3)))
}

/// How fast a hero falls, in units/s²: `sv_gravity` on build 6868, read from the console (ability dives fall faster).
pub const HERO_FALL: f64 = 800.0;

/// A simulated group judged in the air (`magician corpus`: a calibration, not a rule).
#[derive(Debug, Clone)]
pub struct Airborne {
    pub label: String,
    pub names: Vec<String>,
    /// Median node's lift (below), and the worst node's.
    pub lift: f64,
    pub worst: f64,
    pub gravity: f64,
    pub goal: f64,
}

/// A node's airborne lift: while the hero falls at `HERO_FALL` the cloth is solved in a frame that falls with him, so a
/// node of gravity g feels `HERO_FALL - g` upward. The pendulum stiffness of the solver model above turns to
/// `(g - HERO_FALL) dt^2 / arc`, and the node keeps hanging only while `F` plus that stays above zero (the solver's
/// recurrence at rest). Lift is `(HERO_FALL - g) dt^2 / (arc F)`: over 1 the goal spring cannot hold the node down and
/// it swings up over its anchor for as long as the hero is airborne. A node heavier than the fall (g over 800) never
/// rises; a node with no goal at all (F 0) rises whenever it is lighter.
pub fn lift(f: f64, g: f64, arc: f64) -> f64 {
    let up = (HERO_FALL - g) * DT * DT;
    if up <= 0.0 {
        return 0.0;
    }
    if !(arc > 0.0 && arc.is_finite()) {
        return 0.0;
    }
    if f <= 0.0 {
        return 999.0;
    }
    (up / (arc * f)).min(999.0)
}

/// Every simulated group's lift in the air (held nodes left out).
pub fn airborne(c: &Cloth) -> Vec<Airborne> {
    let mut out = Vec::new();
    for g in &c.groups {
        let free: Vec<&Node> = g.iter().map(|&i| &c.nodes[i]).filter(|n| n.f < 0.9 && n.arc.is_finite() && n.arc > 0.0).collect();
        if free.is_empty() {
            continue;
        }
        let lifts: Vec<f64> = free.iter().map(|n| lift(n.f, n.g, n.arc)).collect();
        let mut names: Vec<String> = free.iter().map(|n| n.name.clone()).collect();
        names.sort();
        out.push(Airborne {
            label: label(&names),
            names,
            lift: median(lifts.clone()),
            worst: lifts.iter().cloned().fold(0.0, f64::max),
            gravity: median(free.iter().map(|n| n.g).collect()),
            goal: median(free.iter().map(|n| n.f).collect()),
        });
    }
    out
}

/// Render-mesh area bound to a set of nodes, by group label: filled in by the caller from the decoded meshes.
pub type BoundArea<'a> = &'a dyn Fn(&[String]) -> Option<f64>;

/// The M141 findings for our model against Valve's at the same path. `ours_area`/`valve_area` give the render mesh
/// area (square units, LOD0) bound to a group's nodes, when the meshes decode.
pub fn judge(ours: &[u8], valve: Option<&[u8]>, ours_area: BoundArea, valve_area: BoundArea) -> Vec<String> {
    let Some(oc) = read(ours) else { return Vec::new() };
    let vv = valve.and_then(read).map(|c| verdicts(&c)).unwrap_or_default();
    let mut out = Vec::new();
    for o in verdicts(&oc) {
        if !rings(&o) && !never_settles(&o) {
            continue;
        }
        let cp = counterpart(&o, &vv);
        let mut note = String::new();
        if let Some(v) = cp {
            if same_numbers(&o, v) {
                // Valve's own settings: Valve's design, unless the mod hangs a different (bigger) mesh on them
                let (a, b) = (ours_area(&o.names), valve_area(&v.names));
                match (a, b) {
                    (Some(a), Some(b)) if b > 0.0 && a > b * MESH_GROWTH => {
                        note = format!(
                            " These are Valve's own numbers for this cloth, unchanged, but {:.0} u2 of the mod's mesh rides it where Valve's model hangs {:.0} u2 ({:+.0}%): judge the motion against the look of the new mesh.",
                            a, b, (a / b - 1.0) * 100.0
                        );
                    }
                    _ => continue,
                }
            } else if o.cycles <= v.cycles + WORSE_CYCLES && o.freq <= v.freq * 1.25 {
                continue; // no worse than Valve's at the same place
            } else {
                note = format!(" Valve's cloth here rings {:.1} cycles at {:.2} Hz.", v.cycles, v.freq);
            }
        }
        let through = match (o.has_radius, o.shapes_on_layer) {
            (false, false) => " Its nodes carry no collision radius and no shape shares their layers: it passes through the body.",
            (false, true) => " Its nodes carry no collision radius: the cloth between them passes into the body's capsules.",
            (true, false) => " No collision shape shares its layers: nothing stops it at the body.",
            (true, true) => "",
        };
        let what = if never_settles(&o) {
            format!(
                "never settles: its median node has vertex attraction 0, so each step keeps all of the swing (it shrinks by sqrt(1 - V) = 1) and the piece sways at {:.2} Hz until something stops it. A joint with goal strength 0 compiles to no damping at all; give it a goal above 0.",
                o.freq
            )
        } else {
            format!("rings {:.1} cycles before settling under 10% (median node; worst {:.1}) at {:.2} Hz, damping ratio {:.3}: paper-thin flutter.", o.cycles, o.cycles_max, o.freq, o.zeta)
        };
        out.push(format!(
            "cloth group {}: {what}{through}{note} A fabric that looks like a heavy mesh (one small overshoot, damping ratio {HEAVY_ZETA}) needs vertex attraction about {:.3} at these goals (median now {:.4}, {:.1}x).",
            o.label, o.v_needed, o.v_now, o.v_needed / o.v_now.max(1e-6)
        ));
    }
    out
}

/// Render-mesh area (LOD0, square units) bound to a set of cloth nodes: triangles whose vertices' weights on those
/// nodes average at least one half. The meshes are decoded once, on the first question.
pub struct MeshArea<'a> {
    data: &'a [u8],
    cell: std::cell::OnceCell<Option<(Vec<String>, Vec<mesh::Surface>)>>,
}

impl<'a> MeshArea<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        MeshArea { data, cell: std::cell::OnceCell::new() }
    }
    pub fn area(&self, nodes: &[String]) -> Option<f64> {
        let (names, surfs) = self
            .cell
            .get_or_init(|| mesh::parse(self.data).ok().map(|m| (m.skeleton.names.clone(), m.surfaces(0).0)))
            .as_ref()?;
        let on: Vec<bool> = names.iter().map(|n| nodes.binary_search(n).is_ok()).collect();
        let mut total = 0.0;
        for s in surfs {
            let w = |k: usize| -> f64 {
                let (j, wt) = (s.joints.get(k), s.weights.get(k));
                match (j, wt) {
                    (Some(j), Some(wt)) => j.iter().zip(wt).filter(|(b, _)| on.get(**b as usize).copied().unwrap_or(false)).map(|(_, x)| *x as f64).sum(),
                    _ => 0.0,
                }
            };
            for t in &s.tris {
                let [a, b, c] = [t[0] as usize, t[1] as usize, t[2] as usize];
                if (w(a) + w(b) + w(c)) / 3.0 < 0.5 {
                    continue;
                }
                let p = |k: usize| s.positions[k].map(|x| x as f64);
                let (pa, pb, pc) = (p(a), p(b), p(c));
                let u = [pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2]];
                let v = [pc[0] - pa[0], pc[1] - pa[1], pc[2] - pa[2]];
                let x = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
                total += 0.5 * (x[0] * x[0] + x[1] * x[1] + x[2] * x[2]).sqrt();
            }
        }
        Some(total)
    }
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn offset_under_a_push_is_the_steady_state_of_the_solver() {
        // goal spring alone: a dt^2 / F
        assert!((offset_under(800.0, 0.2, 0.0, f64::INFINITY) - 800.0 / 3600.0 / 0.2).abs() < 1e-12);
        // a hanging node is held by its pendulum too: a dt^2 / (F + g dt^2 / arc)
        let k = 0.01 + 360.0 / 3600.0 / 5.0;
        assert!((offset_under(800.0, 0.01, 360.0, 5.0) - 800.0 / 3600.0 / k).abs() < 1e-12);
        // nothing holds it: it never comes back
        assert_eq!(offset_under(800.0, 0.0, 0.0, f64::INFINITY), f64::INFINITY);
        // a stiffer goal sits nearer its pose
        assert!(offset_under(800.0, 0.2, 0.0, 1.0) < offset_under(800.0, 0.02, 0.0, 1.0));
    }

    #[test]
    fn arcs_are_the_shortest_rod_paths_to_a_static_node() {
        let fe = mesh::cloth::fixture();
        let c = mesh::cloth::from_value(&resource::kv3::Value::Object(vec![("m_pFeModel".into(), fe)])).unwrap();
        let a = arcs(&c);
        assert!(a[..c.static_nodes].iter().all(|&d| d == 0.0), "{a:?}");
        assert!(a[c.static_nodes..].iter().any(|d| d.is_finite() && *d > 0.0), "{a:?}");
    }

    #[test]
    fn oscillator_matches_the_python_lens_on_valves_sinclair() {
        // $cloth_m0p22 of Valve's magician.vmdl_c (build 6864): F 0.00994 V 0.00994 g 360, arc 3.3 -> 1.92 Hz,
        // zeta 0.025, 14.7 cycles (cloth_lens.py)
        let o = oscillator(0.00994, 0.00994, 360.0, 3.3);
        assert!((o.freq - 1.92).abs() < 0.02 && (o.zeta - 0.025).abs() < 0.002 && (o.cycles - 14.7).abs() < 0.2, "{o:?}");
        // a held node does not swing; no damping never settles
        assert_eq!(oscillator(0.0, 0.0, 360.0, 10.0).cycles, f64::INFINITY);
        assert!(oscillator(0.5, 0.9, 360.0, 10.0).cycles < 0.25);
        // no damper: the swing keeps its size whatever its frequency (a 20-unit streamer sways under 1 Hz)
        let slow = oscillator(0.0, 0.0, 360.0, 20.0);
        assert!(slow.freq > 0.5 && slow.freq < 1.0 && slow.zeta == 0.0 && slow.cycles == f64::INFINITY, "{slow:?}");
        let v = v_for_zeta(0.0106, 381.0, 20.0, HEAVY_ZETA);
        assert!((oscillator(0.0106, v, 381.0, 20.0).zeta - HEAVY_ZETA).abs() < 1e-6);
    }
}
