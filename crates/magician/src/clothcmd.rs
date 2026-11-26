//! `magician cloth`: what the compiler made of a model's cloth, read natively from the compiled FeModel (the `PHYS`
//! block), ours against Valve's at the same path. The mods' read-back scripts (`maid-lash/.../cloth_check.py`, a skin's own cloth check scripts,
//! `qa/qa_cloth.py`) did this through Source 2 Viewer's `-b PHYS` text; the decoder
//! is `mesh::cloth`, shared with the cloth lint.

use crate::gamefiles::GameFiles;
use mesh::cloth::{self, Chain, Cloth, Kind, Shape, ShapeKind};
use std::path::Path;
use std::process::ExitCode;

pub const USAGE: &str = "cloth <vpk> <model path> [--against valve|none] [--shapes] [--game DIR]";

fn mean_max(it: impl Iterator<Item = f64>) -> (f64, f64, usize) {
    let v: Vec<f64> = it.collect();
    if v.is_empty() {
        return (0.0, 0.0, 0);
    }
    (v.iter().sum::<f64>() / v.len() as f64, v.iter().cloned().fold(f64::MIN, f64::max), v.len())
}

/// The whole-model summary lines, `(label, text)`.
pub fn summary(c: &Cloth) -> Vec<(&'static str, String)> {
    let free: Vec<usize> = (0..c.node_count).filter(|&i| c.kind(i) == Kind::Free).collect();
    let n = |k: Kind| (0..c.node_count).filter(|&i| c.kind(i) == k).count();
    let at = |i: usize| c.integrator.get(i).copied().unwrap_or_default();
    let (gm, gx, _) = mean_max(free.iter().map(|&i| at(i).goal));
    let (vm, vx, _) = mean_max(free.iter().map(|&i| at(i).vertex));
    let (rm, rx, _) = mean_max(free.iter().map(|&i| c.radius(i)));
    let gmin = free.iter().map(|&i| at(i).gravity).fold(f64::MAX, f64::min);
    let gmax = free.iter().map(|&i| at(i).gravity).fold(f64::MIN, f64::max);
    let caps = c.shapes.iter().filter(|s| s.kind == ShapeKind::Capsule).count();
    let masks: std::collections::BTreeSet<u32> = c.shapes.iter().map(|s| s.mask).collect();
    let (chains, anchors) = cloth::chains(c);
    vec![
        ("nodes", format!("{} ({} chains, {} shape anchors)", c.node_count, chains.len(), anchors.len())),
        ("static", format!("{} ({} rotation-locked)", n(Kind::Static), c.rot_lock_static)),
        ("free", format!("{}", free.len())),
        ("position-driven", format!("{} (not simulated; on an extruded chain, joints' bone centres set between side nodes)", n(Kind::Driven))),
        ("goal attraction", if free.is_empty() { "-".into() } else { format!("mean {gm:.4}, max {gx:.4} (goal strength {:.3} mean)", gm.cbrt()) }),
        ("vertex attraction", if free.is_empty() { "-".into() } else { format!("mean {vm:.4}, max {vx:.4}") }),
        ("gravity", if free.is_empty() { "-".into() } else { format!("{gmin:.0} to {gmax:.0}") }),
        ("collision radius", if free.is_empty() { "-".into() } else { format!("mean {rm:.2}, max {rx:.2} (free nodes)") }),
        ("stray radii", format!("{}{}", c.stray.len(), if c.stray.is_empty() { String::new() } else { format!(", mean {:.2}", c.stray.values().sum::<f64>() / c.stray.len() as f64) })),
        ("rods", format!("{}", c.rods.len())),
        ("collision shapes", format!("{caps} capsules, {} spheres; layers {}", c.shapes.len() - caps, masks.iter().map(|&m| cloth::layers(m)).collect::<Vec<_>>().join(" | "))),
    ]
}

const HEAD: &str = "  chain                              nodes stat free drvn   goal mean/max (g)        vertex mean/max   gravity   radius mean/max   stray      rods  shapes (layers)          rebound            fall/leash";

/// The chain's motion, from the solver model M141 judges by (`lint::cloth`): the median free node's rebound
/// frequency and damping ratio, and the leashed node pressed hardest under the hero's fall acceleration
/// (`HERO_FALL`, sv_gravity 800 units/s^2): its steady offset and its leash. A node whose offset is many times its
/// leash spends every move pressed on the leash and stops dead there.
pub struct Motion {
    pub freq: f64,
    pub zeta: f64,
    pub cycles: f64,
    /// (offset, leash) of the leashed node with the largest offset / leash, if any node is leashed.
    pub leash: Option<(f64, f64)>,
}

pub fn motion(c: &Cloth, ch: &Chain, arcs: &[f64]) -> Option<Motion> {
    use lint::cloth::{offset_under, oscillator, HERO_FALL};
    let at = |i: usize| c.integrator.get(i).copied().unwrap_or_default();
    let arc = |i: usize| arcs.get(i).copied().unwrap_or(f64::INFINITY);
    let free: Vec<usize> = ch.nodes.iter().copied().filter(|&i| c.kind(i) == Kind::Free).collect();
    if free.is_empty() {
        return None;
    }
    let mut osc: Vec<lint::cloth::Osc> = free.iter().map(|&i| oscillator(at(i).goal, at(i).vertex, at(i).gravity, arc(i))).collect();
    osc.sort_by(|a, b| a.freq.total_cmp(&b.freq));
    let mid = osc[osc.len() / 2];
    let leash = free
        .iter()
        .filter_map(|&i| c.stray.get(&i).map(|&s| (offset_under(HERO_FALL, at(i).goal, at(i).gravity, arc(i)), s)))
        .filter(|&(_, s)| s > 0.0)
        .max_by(|a, b| (a.0 / a.1).total_cmp(&(b.0 / b.1)));
    Some(Motion { freq: mid.freq, zeta: mid.zeta, cycles: mid.cycles, leash })
}

fn motion_cells(m: Option<&Motion>) -> String {
    let Some(m) = m else { return format!("{:<18} {}", "-", "-") };
    let rebound = if m.freq == 0.0 {
        "no swing".to_string()
    } else if m.cycles.is_infinite() {
        format!("{:.1} Hz, never settles", m.freq)
    } else {
        format!("{:.1} Hz z {:.2}", m.freq, m.zeta)
    };
    let leash = match m.leash {
        None => "-".to_string(),
        Some((o, s)) if !o.is_finite() => format!("inf/{s:.1}"),
        Some((o, s)) if o >= s => format!("{o:.1}/{s:.1} ({:.0}x)", o / s),
        Some((o, s)) => format!("{o:.1}/{s:.1}"),
    };
    format!("{rebound:<18} {leash}")
}

fn row(ch: &Chain) -> String {
    let shapes = if ch.capsules + ch.spheres == 0 {
        "-".to_string()
    } else {
        format!("{}c {}s ({})", ch.capsules, ch.spheres, cloth::layers(ch.layers))
    };
    let free = ch.free > 0;
    let na = |s: String| if free { s } else { "-".into() };
    format!(
        "  {:<34} {:>5} {:>4} {:>4} {:>4}   {:<24} {:<17} {:<9} {:<17} {:<10} {:<5} {}",
        ch.key,
        ch.nodes.len(),
        ch.static_nodes,
        ch.free,
        ch.driven,
        na(format!("{:.4}/{:.4} ({:.2})", ch.goal_mean, ch.goal_max, ch.goal_mean.cbrt())),
        na(format!("{:.4}/{:.4}", ch.vertex_mean, ch.vertex_max)),
        na(format!("{:.0}", ch.gravity_mean)),
        na(format!("{:.2}/{:.2}", ch.radius_mean, ch.radius_max)),
        if ch.stray == 0 { "-".to_string() } else { format!("{} @{:.1}", ch.stray, ch.stray_mean) },
        if ch.rods_out == 0 { ch.rods.to_string() } else { format!("{}+{}", ch.rods, ch.rods_out) },
        shapes,
    )
}

fn shape_line(c: &Cloth, s: &Shape) -> String {
    let geometry = match s.kind {
        ShapeKind::Capsule => format!("capsule r {:.2} -> {:.2}, length {:.2}", s.ends[0][3], s.ends[1][3], s.length()),
        ShapeKind::Sphere => format!("sphere r {:.2}", s.ends[0][3]),
    };
    format!("  {:<30} {:<42} layers {}", format!("{} ({})", c.name(s.node), s.node), geometry, cloth::layers(s.mask))
}

/// The full text report for one model (and Valve's beside it when given).
pub fn report(path: &str, ours: &Cloth, valve: Option<&Cloth>, shapes: bool) -> Vec<String> {
    let mut out = Vec::new();
    let (so, sv) = (summary(ours), valve.map(summary));
    out.push(format!("{:<20} {:<62} {}", "", "ours", if valve.is_some() { "Valve's" } else { "" }));
    for (i, (label, text)) in so.iter().enumerate() {
        out.push(format!("{label:<20} {text:<62} {}", sv.as_ref().map_or("", |v| v[i].1.as_str())));
    }
    for (side, c) in [("ours", Some(ours)), ("Valve's", valve)] {
        let Some(c) = c else { continue };
        let (chains, anchors) = cloth::chains(c);
        out.push(String::new());
        out.push(format!("{side}: {} chains ({path}); goal in parentheses is the goal strength g, goal attraction = g^3; vertex attraction is the goal damping; drvn (position-driven) nodes are not simulated: on an extruded chain they are the joints' bone centres, set between their two simulated side nodes (as on Valve's rope), not nodes that follow the skeleton; rebound is the median free node's frequency and damping ratio z in the solver model M141 judges by (z near 0.1 rings on, 0.3 to 0.4 swings once and settles); fall/leash is, for the leashed node pressed hardest, its steady offset while the hero accelerates at his fall (800 units/s^2) against its stray radius: past 1x it reaches the leash under that push, and the larger the multiple the more of every move it spends pressed on the leash and the deader it stops there", chains.len()));
        out.push(HEAD.to_string());
        let arcs = lint::cloth::arcs(c);
        let w = HEAD.find("rebound").unwrap_or(0).saturating_sub(1);
        out.extend(chains.iter().map(|ch| format!("{:<w$} {}", row(ch), motion_cells(motion(c, ch, &arcs).as_ref()))));
        if !anchors.is_empty() {
            let anchor = |n: usize| {
                let on: Vec<&Shape> = c.shapes.iter().filter(|s| s.node == n).collect();
                match on.len() {
                    0 => c.name(n).to_string(),
                    k => format!("{} ({k}, layers {})", c.name(n), cloth::layers(on.iter().fold(0, |m, s| m | s.mask))),
                }
            };
            out.push(format!("  shape anchors (one static node each, shapes hang on them): {}", anchors.iter().map(|&n| anchor(n)).collect::<Vec<_>>().join(", ")));
        }
    }
    if shapes {
        for (side, c) in [("ours", Some(ours)), ("Valve's", valve)] {
            let Some(c) = c else { continue };
            out.push(String::new());
            out.push(format!("{side}: collision shapes ({}), node masks of the collision tree: {}", c.shapes.len(), {
                let mut m: std::collections::BTreeMap<u32, usize> = Default::default();
                c.tree_masks.iter().for_each(|&x| *m.entry(x).or_default() += 1);
                if m.is_empty() { "none".into() } else { m.iter().map(|(k, n)| format!("{} x{n}", cloth::layers(*k))).collect::<Vec<_>>().join(", ") }
            }));
            out.extend(c.shapes.iter().map(|s| shape_line(c, s)));
        }
    }
    out
}

pub fn cmd_cloth(args: &[String], game_dir: &Path) -> Result<ExitCode, String> {
    let mut pos: Vec<&String> = Vec::new();
    let mut skip = false;
    for a in args {
        if skip {
            skip = false;
        } else if a.starts_with("--") {
            skip = !matches!(a.as_str(), "--shapes");
        } else {
            pos.push(a);
        }
    }
    let vp = pos.first().ok_or(USAGE)?;
    let flag = |n: &str| args.iter().position(|a| a == n).and_then(|i| args.get(i + 1)).cloned();
    let against = flag("--against").unwrap_or_else(|| "valve".into());
    let pak = vpk::Vpk::open(vp).map_err(|e| format!("{vp}: {e}"))?;
    let path = crate::pak_path(&|x| pak.find(x).is_some(), pos.get(1).ok_or(USAGE)?).to_ascii_lowercase();
    let entry = pak.find(&path).ok_or_else(|| crate::not_in(&path, Path::new(vp.as_str())))?;
    let data = pak.read(entry).map_err(|e| e.to_string())?;
    let ours = cloth::read(&data)?;
    outln!("{path}  ({vp})");
    let Some(ours) = ours else {
        outln!("no cloth: the model has no FeModel in its PHYS block");
        return Ok(ExitCode::SUCCESS);
    };
    let valve = if against == "valve" {
        let game = GameFiles::open(game_dir, None);
        match lint::Game::read(&game, &path) {
            Some(d) => cloth::read(&d)?,
            None => {
                outln!("(the game has no file at this path: nothing to put beside it)");
                None
            }
        }
    } else {
        None
    };
    if against == "valve" && valve.is_none() {
        outln!("(Valve's model at this path has no cloth)");
    }
    outln!();
    for l in report(&path, &ours, valve.as_ref(), args.iter().any(|a| a == "--shapes")) {
        outln!("{l}");
    }
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_report_puts_ours_beside_valves_with_every_figure() {
        let fe = mesh::cloth::fixture();
        let c = cloth::from_value(&resource::kv3::Value::Object(vec![("m_pFeModel".into(), fe)])).unwrap();
        let lines = report("models/x/x.vmdl_c", &c, Some(&c), true);
        let text = lines.join("\n");
        // summary: counts by kind, rods, shapes and layers, both columns
        assert!(text.lines().any(|l| l.starts_with("nodes") && l.matches("8 (").count() == 2), "{text}");
        assert!(text.contains("static") && text.contains("3 (1 rotation-locked)"), "{text}");
        assert!(text.contains("collision shapes") && text.contains("1 capsules, 1 spheres; layers 0,2 | 0-3"), "{text}");
        assert!(text.contains("rods") && text.lines().any(|l| l.starts_with("rods") && l.contains('3')), "{text}");
        // the chain tables: skirt with its free nodes' goal (mean/max and g), vertex attraction, gravity, radius, leash, rods, shape
        let skirt = text.lines().find(|l| l.trim_start().starts_with("skirt ")).expect("a skirt row");
        assert!(skirt.contains("0.0760/0.1250") && skirt.contains("0.3750/0.5000") && skirt.contains("270") && skirt.contains("2.50/3.50") && skirt.contains("1 @4.0"), "{skirt}");
        assert!(skirt.contains("0c 1s (0,2)"), "{skirt}");
        assert_eq!(text.lines().filter(|l| l.trim_start().starts_with("skirt ")).count(), 2, "ours and Valve's tables");
        assert!(text.contains("shape anchors") && text.contains("glove"), "{text}");
        // --shapes lists every shape with its layers
        assert!(text.contains("glove (0)") && text.contains("capsule r 5.00 -> 6.50, length 13.00") && text.contains("layers 0-3"), "{text}");
        assert!(text.contains("skirt_1 (2)") && text.contains("sphere r 4.00") && text.contains("layers 0,2"), "{text}");
        // every chain row ends in its motion: the median free node's rebound, and the leashed node's offset under the fall
        assert!(skirt.contains(" Hz z ") && skirt.contains("/4.0"), "{skirt}");
        // without Valve's model there is one column and one table
        let solo = report("p", &c, None, false).join("\n");
        assert_eq!(solo.lines().filter(|l| l.trim_start().starts_with("skirt ")).count(), 1);
        assert!(!solo.contains("collision shapes (2)"));
    }

    #[test]
    fn motion_is_the_solver_model_of_the_free_nodes_and_the_leash_pressed_hardest() {
        let fe = mesh::cloth::fixture();
        let c = cloth::from_value(&resource::kv3::Value::Object(vec![("m_pFeModel".into(), fe)])).unwrap();
        let (chains, _) = cloth::chains(&c);
        let skirt = chains.iter().find(|ch| ch.key == "skirt").expect("the skirt chain");
        let arcs = lint::cloth::arcs(&c);
        let m = motion(&c, skirt, &arcs).expect("the skirt has free nodes");
        assert!(m.freq > 0.0 && m.zeta > 0.0 && m.zeta < 1.0 && m.cycles.is_finite(), "{} {} {}", m.freq, m.zeta, m.cycles);
        // node 3 is the leashed one (4.0); its offset is the steady state of the solver under 800 units/s^2
        let it = c.integrator[3];
        let want = lint::cloth::offset_under(lint::cloth::HERO_FALL, it.goal, it.gravity, arcs[3]);
        assert_eq!(m.leash, Some((want, 4.0)));
        // a chain with no free node has no motion to report
        assert!(chains.iter().filter(|ch| ch.free == 0).all(|ch| motion(&c, ch, &arcs).is_none()));
        assert_eq!(motion_cells(None).split_whitespace().collect::<Vec<_>>(), ["-", "-"]);
        // the cells: a multiple once the offset passes the leash, the bare figures before
        let cell = |o: f64| motion_cells(Some(&Motion { freq: 2.0, zeta: 0.2, cycles: 1.5, leash: Some((o, 2.0)) }));
        assert!(cell(5.0).ends_with("5.0/2.0 (2x)") && cell(1.0).ends_with("1.0/2.0") && cell(f64::INFINITY).ends_with("inf/2.0"));
        assert!(motion_cells(Some(&Motion { freq: 0.0, zeta: 1.0, cycles: 0.0, leash: None })).starts_with("no swing"));
        assert!(motion_cells(Some(&Motion { freq: 1.0, zeta: 0.0, cycles: f64::INFINITY, leash: None })).contains("never settles"));
    }
}
