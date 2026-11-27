//! `magician mesh`: the full mesh verdict for one compiled model, ours against Valve's.

use crate::gamefiles::GameFiles;
use lint::mesh::{census::Census, rules, views, Env};
use lint::Severity;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

pub const USAGE: &str = "mesh <vpk> <model path> [--against valve|none] [--with <vpk>]... [--out DIR] [--glb FILE] [--no-png] [--fairplay VIEW] [--fairplay-unit U] [--json-out FILE] [--game DIR]";

fn read_from(paks: &[vpk::Vpk], p: &str) -> Option<Vec<u8>> {
    paks.iter().find_map(|v| v.find(p).and_then(|e| v.read(e).ok()))
}

/// `fp`: the fair-play limits in force (the policy), so `mesh` judges a model exactly as `lint --deep` would.
pub fn cmd_mesh(args: &[String], game_dir: &Path, reference: &Path, policy: &grade::Policy) -> Result<ExitCode, String> {
    let fp = &policy.fair_play;
    // Each code at the severity a lint would report it under this policy (fair play is lenient unless enforced).
    let severity = |code: &str| grade::compliance::severity(code, "", lint::rule(code).severity, policy);
    let pos: Vec<&String> = {
        let mut out = Vec::new();
        let mut skip = false;
        for a in args {
            if skip {
                skip = false;
                continue;
            }
            if a.starts_with("--") {
                skip = !matches!(a.as_str(), "--no-png");
                continue;
            }
            out.push(a);
        }
        out
    };
    let vp = pos.first().ok_or(USAGE)?;
    let flag = |n: &str| args.iter().position(|a| a == n).and_then(|i| args.get(i + 1)).cloned();
    let withs: Vec<String> = args.windows(2).filter(|w| w[0] == "--with").map(|w| w[1].clone()).collect();
    let against = flag("--against").unwrap_or_else(|| "valve".into());
    // Every option is read before the model is: a mistyped view or comparison is said before seconds of decoding.
    if against != "valve" && against != "none" {
        return Err(format!("--against {against}: valve or none"));
    }
    let fairplay = match flag("--fairplay") {
        Some(name) => {
            let view = views::view_named(&name).ok_or_else(|| format!("--fairplay {name}: no such view (front, side, back, top, below, left, or an oblique such as front-right-high)"))?;
            let unit: f64 = match flag("--fairplay-unit") {
                Some(u) => u.parse().ok().filter(|u: &f64| *u > 0.0 && u.is_finite()).ok_or_else(|| format!("--fairplay-unit {u}: not a positive number of units a pixel"))?,
                None => 0.1,
            };
            Some((name, view, unit))
        }
        None => None,
    };
    let mut paks = vec![vpk::Vpk::open(vp).map_err(|e| format!("{vp}: {e}"))?];
    for w in &withs {
        paks.push(vpk::Vpk::open(w).map_err(|e| format!("{w}: {e}"))?);
    }
    let path = crate::pak_path(&|x| paks.iter().any(|v| v.find(x).is_some()), pos.get(1).ok_or(USAGE)?).to_ascii_lowercase();
    let game = GameFiles::open(game_dir, None);
    let data = read_from(&paks, &path).ok_or_else(|| crate::not_in(&path, Path::new(vp.as_str())))?;
    let ours_read = |p: &str| read_from(&paks, p).or_else(|| lint::Game::read(&game, p));
    let valve_read = |p: &str| lint::Game::read(&game, p);
    let om = |p: &str| ours_read(&lint::deep::compiled(p)).and_then(|d| lint::mesh::mat_info(&d));
    let ot = |p: &str| ours_read(p).and_then(|d| lint::deep::tex_info(&d));
    let vm = |p: &str| valve_read(&lint::deep::compiled(p)).and_then(|d| lint::mesh::mat_info(&d));
    let vt = |p: &str| valve_read(p).and_then(|d| lint::deep::tex_info(&d));
    let t0 = std::time::Instant::now();
    // Ours and Valve's are measured at the same time: neither reads the other.
    let (ours, valve) = lint::par::join(
        || Census::build(&data, &Env { material: &om, texture: &ot, file: &ours_read }),
        || {
            if against == "valve" {
                lint::Game::read(&game, &path).and_then(|d| Census::build(&d, &Env { material: &vm, texture: &vt, file: &valve_read }).ok())
            } else {
                None
            }
        },
    );
    let ours = ours?;
    let elapsed = t0.elapsed();

    outln!("{path}  ({vp})");
    if let Some(glb) = flag("--glb") {
        let bytes = std::fs::read(&glb).map_err(|e| format!("{glb}: {e}"))?;
        let p = mesh::prove(&ours.model, &bytes)?;
        outln!(
            "decode proof against {glb}: {} meshes, {} draw calls, {} vertices, {} triangles; max error position {:.2e} units, normal {:.2e}, uv {:.2e}, weight {:.2e}; {} joint mismatches: {}",
            p.meshes, p.draws, p.vertices, p.triangles, p.max_position_error, p.max_normal_error, p.max_uv_error, p.max_weight_error, p.joint_mismatches,
            if p.ok() { "PROVEN".to_string() } else { format!("FAILED: {}", p.failures.join("; ")) }
        );
    }
    outln!("\nLODs              ours                         Valve's");
    for (k, l) in ours.lods.iter().enumerate() {
        let v = valve.as_ref().and_then(|v| v.lods.get(k));
        outln!(
            "  LOD{k}  {:>7} tris {:>7} verts {:>3} draws   {}",
            l.tris,
            l.verts,
            l.draws,
            v.map_or("-".into(), |v| format!("{:>7} tris {:>7} verts {:>3} draws", v.tris, v.verts, v.draws))
        );
    }
    outln!(
        "  bones {}  shells {}  textures {} ({} MB){}",
        ours.model.skeleton.names.len(),
        ours.shells.len(),
        ours.textures.len(),
        ours.texture_bytes >> 20,
        valve.as_ref().map_or(String::new(), |v| format!("   Valve's: bones {}, shells {}, textures {} ({} MB)", v.model.skeleton.names.len(), v.shells.len(), v.textures.len(), v.texture_bytes >> 20))
    );
    let e = &ours.envelope;
    if e.area > 0.0 {
        outln!(
            "  hitbox envelope: {:.1}% beyond 2 u, {:.1}% beyond {} u, {:.1}% beyond {} u, 99.9th percentile {:.1} u{}",
            100.0 * e.beyond(2.0) / e.area,
            100.0 * e.beyond(fp.hitbox_warn_units) / e.area,
            fp.hitbox_warn_units,
            100.0 * e.beyond(fp.hitbox_taste_units) / e.area,
            fp.hitbox_taste_units,
            e.p999,
            valve.as_ref().filter(|v| v.envelope.area > 0.0).map_or(String::new(), |v| format!("   Valve's: {:.1}% beyond {} u, 99.9th {:.1} u", 100.0 * v.envelope.beyond(fp.hitbox_warn_units) / v.envelope.area, fp.hitbox_warn_units, v.envelope.p999))
        );
    }
    let silhouettes = valve.as_ref().map(|v| views::silhouettes(&ours, v)).unwrap_or_default();
    if valve.is_some() {
        outln!("\nfair-play silhouettes (in-game, 6 mm pixels, same frames)");
        for c in &silhouettes {
            outln!("  {:<6} {:>6.1}% of Valve's   IoU {:.3}   ours adds {} px, loses {} px", c.view, 100.0 * c.ratio(), c.iou, c.added, c.lost);
        }
    }
    if let Some(s) = &ours.symmetry {
        outln!("  symmetry about {} : mean {:.2} u, max {:.2} u{}", s.axis, s.mean_error, s.max_error, valve.as_ref().and_then(|v| v.symmetry.as_ref()).map_or(String::new(), |v| format!("   Valve's: mean {:.2} u, max {:.2} u", v.mean_error, v.max_error)));
    }
    outln!("  shape families (identical wiring): {}   census {:.1} s ({})", ours.families.len(), elapsed.as_secs_f64(), ours.timings.iter().map(|(n, t)| format!("{n} {t:.1}")).collect::<Vec<_>>().join(", "));

    let mut findings = rules::judge(&ours, valve.as_ref(), fp);
    if let Some(v) = &valve {
        let list = |c: &Census| c.materials.iter().map(|(k, m)| (k.clone(), m.clone())).collect::<Vec<_>>();
        findings.extend(rules::audit_materials(&list(&ours), &list(v), !v.model.hitboxes.is_empty()));
    }
    findings.extend(rules::error_draws(&ours, valve.as_ref()));
    // verdict (scene3d verdict.ts): fail on an error, attention on a warning, else pass; actions by
    // severity, then by how many findings share the code
    let mut by: BTreeMap<&'static str, Vec<String>> = BTreeMap::new();
    for (c, m) in &findings {
        by.entry(c).or_default().push(m.clone());
    }
    let mut codes: Vec<(&'static str, Vec<String>)> = by.into_iter().collect();
    codes.sort_by(|a, b| severity(b.0).cmp(&severity(a.0)).then(b.1.len().cmp(&a.1.len())).then(a.0.cmp(b.0)));
    let worst = codes.iter().map(|(c, _)| severity(c)).max();
    let grade = match worst {
        Some(Severity::Error) => "FAIL",
        Some(Severity::Warn) => "ATTENTION",
        _ => "PASS",
    };
    outln!("\nverdict: {grade}  ({} findings over {} rules)", findings.len(), codes.len());
    if let Some(out) = flag("--json-out") {
        let flat: Vec<(&'static str, String)> = codes.iter().flat_map(|(c, ms)| ms.iter().map(move |m| (*c, m.clone()))).collect();
        crate::write_output("--json-out", &out, &crate::jsonout::mesh_run(&path, vp, grade, &silhouettes, &flat)).map_err(|e| e.text().to_string())?;
    }
    let mut marks = Vec::new();
    for (code, msgs) in &codes {
        let r = lint::rule(code);
        let sev = match severity(code) {
            Severity::Error => "error",
            Severity::Warn => "warn",
            Severity::Info => "info",
        };
        outln!("\n{code} {} ({sev}, {}): {}", r.name, msgs.len(), r.in_game);
        for m in msgs {
            outln!("  - {m}");
        }
    }
    // the worst places to look at, marked on the sheet
    for h in rules::cut_body_voids(&ours, valve.as_ref()).into_iter().take(4) {
        marks.push((h.centroid, format!("cut-body opening {:.0} u2 ({})", h.area, h.bone)));
    }
    for c in ours.crosses.iter().filter(|c| c.a != c.b && c.visible_pairs > 0).take(2) {
        marks.push((c.bounds.center(), format!("crossing {} x {}", ours.shells[c.a].label, ours.shells[c.b].label)));
    }
    if !args.iter().any(|a| a == "--no-png") {
        let out = flag("--out").map(PathBuf::from).unwrap_or_else(|| reference.join("work").join("mesh").join(path.rsplit('/').next().unwrap_or("model").trim_end_matches(".vmdl_c")));
        std::fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;
        let sheet = views::xray_sheet(&ours, valve.as_ref(), &path, &marks);
        let png = out.join("xray.png");
        std::fs::write(&png, sheet.png()).map_err(|e| format!("{}: {e}", png.display()))?;
        outln!("\nx-ray sheet: {}  (grey both, orange ours adds, cyan ours loses, red back faces seen first)", png.display());
    }
    if let Some((name, view, unit)) = fairplay {
        let out = flag("--out").map(PathBuf::from).unwrap_or_else(|| reference.join("work").join("mesh").join(path.rsplit('/').next().unwrap_or("model").trim_end_matches(".vmdl_c")));
        std::fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;
        let lines = [fp.hitbox_warn_units, fp.hitbox_taste_units, fp.hitbox_egregious_units];
        let v = views::fairplay_view(&ours, valve.as_ref(), view, unit, &lines, &ours_read);
        let (shaded, test) = (out.join(format!("fairplay-{name}-shaded.png")), out.join(format!("fairplay-{name}-test.png")));
        for (p, im) in [(&shaded, &v.shaded), (&test, &v.test)] {
            std::fs::write(p, im.png()).map_err(|e| format!("{}: {e}", p.display()))?;
        }
        outln!("\nfair-play view, {name}, {} x {} px at {:.3} u a pixel, one camera for both", v.shaded.w, v.shaded.h, v.unit);
        outln!("  shaded  {}  (ours, lit with its own colour maps{})", shaded.display(), if v.undecoded.is_empty() { String::new() } else { format!("; grey where a map did not decode: {}", v.undecoded.join(", ")) });
        outln!(
            "  test    {}  ({} hitboxes; white our outline, dashed Valve's{}; our surface past every hitbox: {} px beyond {} u, {} beyond {} u, {} beyond {} u)",
            test.display(),
            v.hitboxes,
            if valve.is_some() { "" } else { " (not drawn: --against none)" },
            v.beyond[0],
            lines[0],
            v.beyond[1],
            lines[1],
            v.beyond[2],
            lines[2]
        );
    }
    outln!("\n{}", views::ascii_front(&ours, valve.as_ref(), 48));
    Ok(if worst == Some(Severity::Error) { ExitCode::from(1) } else { ExitCode::SUCCESS })
}
