//! Facts the grade needs a look inside files for: which materials the hero's own compiled model uses, which UI scene
//! maps a release really carries, how our model's geometry and head sit on Valve's, which of Valve's material slots a
//! variant covers, and whether the hero is renamed. The criteria and thresholds stay in `rubric.rs`; this is only
//! the reading, so the grade credits what the hero actually uses and nothing the mod merely contains.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// The `.vmat_c` files the compiled model `model` references (its RERL) that the mod ships, wherever their path is.
/// A material the model names but the mod does not ship is Valve's own: no credit. A material the mod ships that
/// the model does not name is not credited here (the path-token route still counts it).
pub fn model_materials(model: &[u8], shipped: &BTreeSet<String>) -> BTreeSet<String> {
    resource::parse(model)
        .map(|r| lint::ref_targets(&r).into_iter().filter(|t| t.ends_with(".vmat_c") && shipped.contains(t)).collect())
        .unwrap_or_default()
}

/// What a UI scene map must hold to count as a scene: its map, world and entity lump, all intact.
fn required(stem: &str) -> [String; 3] {
    [format!("{stem}.vmap_c"), format!("{stem}/world.vwrld_c"), format!("{stem}/entities/default_ents.vents_c")]
}

/// Of the footprint's scene maps (`maps/ui/hero_prefabs/<hero>.vpk`, `maps/ui/hero_shop/<hero>_shop.vpk`...), the
/// ones the release carries under one of `roots` (`<root>/maps/ui/...`) with their own entity lump: the archive must
/// hold the scene's map, world and entities, every one with a good CRC and a parseable resource, and the entities must
/// differ from Valve's (`stock` opens Valve's map at the same path). A scene identical to Valve's earns nothing: it
/// shows what the game shows anyway.
pub fn carried_scenes(
    scenes: &[String],
    roots: &[PathBuf],
    stock: &dyn Fn(&str) -> Option<vpk::Vpk>,
    say: &mut dyn FnMut(String),
) -> Result<BTreeSet<String>, String> {
    let mut out = BTreeSet::new();
    for scene in scenes {
        let rel = Path::new(scene);
        if !scene.starts_with("maps/ui/") || !scene.ends_with(".vpk") || rel.components().any(|c| !matches!(c, std::path::Component::Normal(_))) {
            return Err(format!("unexpected UI scene path {scene}"));
        }
        let Some(candidate) = roots.iter().map(|r| r.join(rel)).find(|c| c.is_file()) else { continue };
        let archive = vpk::Vpk::open(&candidate).map_err(|e| format!("{}: {e}", candidate.display()))?;
        let stem = scene.trim_end_matches(".vpk");
        for need in required(stem) {
            let entry = archive.find(&need).ok_or_else(|| format!("{} lacks {need}", candidate.display()))?;
            let bytes = archive.read(entry).map_err(|e| e.to_string())?;
            if vpk::crc32(&bytes) != entry.crc {
                return Err(format!("{} contains a corrupt {need}", candidate.display()));
            }
            resource::parse(&bytes).map_err(|e| format!("{}: {need}: {e}", candidate.display()))?;
        }
        let entities = format!("{stem}/entities/default_ents.vents_c");
        let original = stock(scene).ok_or_else(|| format!("the game has no stock scene at {scene}"))?;
        let original_entities = original.find(&entities).ok_or_else(|| format!("stock UI scene lacks {entities}"))?;
        let ours = archive.find(&entities).expect("validated scene entity entry");
        if original_entities.crc != ours.crc {
            say(format!("Separate UI scene: {}", candidate.display()));
            out.insert(scene.clone());
        } else {
            say(format!("UI scene {} carries Valve's own entities: no credit", candidate.display()));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(v: &[&str]) -> BTreeSet<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_models_own_materials_count_wherever_they_live() {
        // Real case: a skin's materials live under materials/<skin>/, so a path-token count saw none of them.
        let model = resource::synth(&[
            "materials/example/shell.vmat",
            "materials/example/palette.vmat",
            "models/heroes_wip/magician/materials/magician_body.vmat", // Valve's own, not shipped
            "models/heroes_wip/magician/magician.vnmskel",
        ]);
        let shipped = set(&["materials/example/shell.vmat_c", "materials/example/palette.vmat_c", "materials/example/unused.vmat_c", "particles/x.vpcf_c"]);
        assert_eq!(model_materials(&model, &shipped), set(&["materials/example/shell.vmat_c", "materials/example/palette.vmat_c"]));
        assert!(model_materials(b"not a resource", &shipped).is_empty());
    }

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("magician_scene_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn scene_vpk(path: &Path, stem: &str, ents: &str, drop: Option<&str>) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut files = vec![
            (format!("{stem}.vmap_c"), resource::synth(&[])),
            (format!("{stem}/world.vwrld_c"), resource::synth(&[])),
            (format!("{stem}/entities/default_ents.vents_c"), resource::synth(&[ents])),
        ];
        files.retain(|(p, _)| Some(p.as_str()) != drop);
        vpk::write(path, &files).unwrap();
    }

    #[test]
    fn a_scene_map_with_its_own_entities_is_carried() {
        let d = scratch("carried");
        let scene = "maps/ui/hero_prefabs/magician.vpk";
        scene_vpk(&d.join("release").join(scene), "maps/ui/hero_prefabs/magician", "ours.vmdl", None);
        let stock_path = d.join("stock").join(scene);
        scene_vpk(&stock_path, "maps/ui/hero_prefabs/magician", "valves.vmdl", None);
        let stock = |_: &str| vpk::Vpk::open(&stock_path).ok();
        let mut said = Vec::new();
        let got = carried_scenes(&[scene.to_string()], &[d.join("elsewhere"), d.join("release")], &stock, &mut |m| said.push(m)).unwrap();
        assert_eq!(got, set(&[scene]));
        assert!(said[0].starts_with("Separate UI scene"));
        // not carried: nothing under the roots
        assert!(carried_scenes(&[scene.to_string()], &[d.join("elsewhere")], &stock, &mut |_| {}).unwrap().is_empty());
        // Valve's own entities earn nothing
        let same = d.join("same").join(scene);
        scene_vpk(&same, "maps/ui/hero_prefabs/magician", "valves.vmdl", None);
        let got = carried_scenes(&[scene.to_string()], &[d.join("same")], &stock, &mut |m| said.push(m)).unwrap();
        assert!(got.is_empty() && said.last().unwrap().contains("no credit"));
        // a scene without its world is broken, not credited
        let broken = d.join("broken").join(scene);
        scene_vpk(&broken, "maps/ui/hero_prefabs/magician", "ours.vmdl", Some("maps/ui/hero_prefabs/magician/world.vwrld_c"));
        let e = carried_scenes(&[scene.to_string()], &[d.join("broken")], &stock, &mut |_| {}).unwrap_err();
        assert!(e.contains("lacks maps/ui/hero_prefabs/magician/world.vwrld_c"), "{e}");
        assert!(carried_scenes(&["maps/dl_midtown.vpk".to_string()], std::slice::from_ref(&d), &stock, &mut |_| {}).is_err());
        let _ = std::fs::remove_dir_all(&d);
    }
}

// ------------------------------------------------------------------------------------------------ geometry

/// Positions within this distance (model units, ~2.5 mm) are the same vertex: a recompile of Valve's own mesh moves
/// nothing further than float rounding.
pub const SAME_VERTEX: f64 = 0.1;

/// How our model's geometry sits on Valve's model at the same path.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Geometry {
    /// LOD0 vertices of ours, and how many sit on one of Valve's.
    pub vertices: usize,
    pub on_valve: usize,
    /// Our vertices weighted most to the head (the `head` bone and every bone below it), how many sit on Valve's head,
    /// and how many head vertices Valve's has.
    pub head: usize,
    pub head_on_valve: usize,
    pub valve_head: usize,
}

impl Geometry {
    pub fn mesh_share(&self) -> f64 {
        if self.vertices == 0 { 0.0 } else { self.on_valve as f64 / self.vertices as f64 }
    }
    pub fn head_share(&self) -> f64 {
        if self.head == 0 { 0.0 } else { self.head_on_valve as f64 / self.head as f64 }
    }
}

/// LOD0 vertex positions of a model, each with whether its strongest influence is the head or a bone below it.
fn vertices(data: &[u8]) -> Result<Vec<([f32; 3], bool)>, String> {
    let m = mesh::parse(data)?;
    let sk = &m.skeleton;
    let head = sk.index("head");
    let below_head = |mut b: usize| -> bool {
        for _ in 0..sk.names.len() {
            if Some(b) == head {
                return true;
            }
            match sk.parents.get(b) {
                Some(&p) if p >= 0 => b = p as usize,
                _ => return false,
            }
        }
        false
    };
    let (surfaces, _) = m.surfaces(0);
    let mut out = Vec::new();
    for s in &surfaces {
        for (i, p) in s.positions.iter().enumerate() {
            let strongest = s.joints.get(i).zip(s.weights.get(i)).and_then(|(j, w)| {
                (0..8).filter(|&k| j[k] != u16::MAX && w[k] > 0.0).max_by(|&a, &b| w[a].partial_cmp(&w[b]).unwrap_or(std::cmp::Ordering::Equal).then(b.cmp(&a))).map(|k| j[k] as usize)
            });
            out.push((*p, strongest.is_some_and(below_head)));
        }
    }
    Ok(out)
}

/// A grid of positions for "is there one of these within `SAME_VERTEX`?".
struct Grid(std::collections::HashMap<(i64, i64, i64), Vec<[f32; 3]>>);

impl Grid {
    fn cell(p: [f32; 3]) -> (i64, i64, i64) {
        let c = |x: f32| (x as f64 / SAME_VERTEX).floor() as i64;
        (c(p[0]), c(p[1]), c(p[2]))
    }
    fn new<'a>(points: impl Iterator<Item = &'a [f32; 3]>) -> Grid {
        let mut g: std::collections::HashMap<(i64, i64, i64), Vec<[f32; 3]>> = Default::default();
        for p in points {
            g.entry(Grid::cell(*p)).or_default().push(*p);
        }
        Grid(g)
    }
    fn near(&self, p: [f32; 3]) -> bool {
        let (x, y, z) = Grid::cell(p);
        let d2 = SAME_VERTEX * SAME_VERTEX;
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    if let Some(v) = self.0.get(&(x + dx, y + dy, z + dz)) {
                        if v.iter().any(|q| (0..3).map(|k| (p[k] - q[k]) as f64).map(|d| d * d).sum::<f64>() <= d2) {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }
}

/// Our model's LOD0 geometry against Valve's model at the same path: the whole mesh, and the head on its own. Both
/// are read with the native mesh decoder; a model that does not decode is an error, never a guess.
pub fn compare_models(ours: &[u8], valve: &[u8]) -> Result<Geometry, String> {
    let o = vertices(ours).map_err(|e| format!("our model: {e}"))?;
    let v = vertices(valve).map_err(|e| format!("Valve's model: {e}"))?;
    let all = Grid::new(v.iter().map(|x| &x.0));
    let head = Grid::new(v.iter().filter(|x| x.1).map(|x| &x.0));
    let mut g = Geometry { vertices: o.len(), valve_head: v.iter().filter(|x| x.1).count(), ..Default::default() };
    for (p, is_head) in &o {
        if all.near(*p) {
            g.on_valve += 1;
        }
        if *is_head {
            g.head += 1;
            if head.near(*p) {
                g.head_on_valve += 1;
            }
        }
    }
    Ok(g)
}

/// Valve's visible material slots on the hero model (the distinct materials its LOD0 draws), each covered when the mod
/// ships that material at Valve's path, or when our model, in the same mesh and draw, draws a material the mod ships.
/// Returns (covered, missing), each by Valve's material path.
pub fn material_slots(valve: &[u8], ours: Option<&[u8]>, shipped: &BTreeSet<String>) -> Result<(Vec<String>, Vec<String>), String> {
    let compiled = |m: &str| {
        let m = m.to_ascii_lowercase();
        if m.ends_with("_c") { m } else { format!("{m}_c") }
    };
    let (vs, _) = mesh::parse(valve)?.surfaces(0);
    let ours = match ours {
        Some(d) => Some(mesh::parse(d)?.surfaces(0).0),
        None => None,
    };
    let mut slots: BTreeMap<String, bool> = BTreeMap::new();
    for s in &vs {
        let mat = compiled(&s.material);
        let mine = ours.as_ref().and_then(|o| o.iter().find(|x| x.mesh == s.mesh && x.draw == s.draw)).map(|x| compiled(&x.material));
        let covered = shipped.contains(&mat) || mine.is_some_and(|m| shipped.contains(&m));
        *slots.entry(mat).or_insert(false) |= covered;
    }
    let (c, m): (Vec<_>, Vec<_>) = slots.into_iter().partition(|(_, c)| *c);
    Ok((c.into_iter().map(|x| x.0).collect(), m.into_iter().map(|x| x.0).collect()))
}

/// Evidence that the mod gives the hero a new name, and notes on what does not count. A localisation file of the mod
/// that names the hero something other than Valve's display name is a rename. A replaced name logo is one only when
/// the mod says what it reads (`mod.toml [identity] name`, else `[mod] name`) and that name does not keep the hero's
/// own: bytes can't read a logo, and "Maid Lash" on a new logo is still Lash. `files` are the mod's localisation
/// files (path, text). Returns (evidence that counts, notes that do not).
pub fn renamed(hero_key: &str, display: &str, name_art: &[String], paths: &BTreeSet<String>, files: &[(String, String)], declared: Option<&str>) -> (Vec<String>, Vec<String>) {
    let (mut yes, mut notes) = (Vec::new(), Vec::new());
    let key = format!("{hero_key}:n");
    for (path, text) in files {
        for line in text.lines() {
            let q = crate::footprint::quoted(line);
            if q.len() >= 2 && q[0].eq_ignore_ascii_case(&key) && q[1] != display {
                yes.push(format!("{path} names the hero \"{}\" (Valve's \"{display}\")", q[1]));
            }
        }
    }
    let logos: Vec<&String> = name_art.iter().filter(|p| paths.contains(*p)).collect();
    if let Some(first) = logos.first() {
        let keeps = |name: &str| name.split(|c: char| !c.is_alphanumeric()).any(|w| w.eq_ignore_ascii_case(display));
        match declared {
            Some(name) if !keeps(name) => yes.push(format!("replaces the name logo {first} for \"{name}\" (Valve's \"{display}\")")),
            Some(name) => notes.push(format!("replaces the name logo {first}, but the mod's name \"{name}\" keeps \"{display}\": the same character")),
            None => notes.push(format!("replaces the name logo {first}, but nothing says it reads a new name (set [mod] name in mod.toml)")),
        }
    }
    (yes, notes)
}

#[cfg(test)]
mod geometry_tests {
    use super::*;
    use mesh::synth::{SynthDraw, SynthMesh, SynthModel};

    /// A two-box hero: a body weighted to the pelvis and a head box weighted to the jaw (below `head`).
    fn hero(head_at: f32, body_mat: &str) -> SynthModel {
        let cube = |min: [f32; 3], s: f32, joint: u8, mat: &str| {
            let mut d = SynthDraw { material: mat.into(), ..Default::default() };
            for i in 0..8 {
                d.positions.push([min[0] + if i & 1 == 0 { 0.0 } else { s }, min[1] + if i & 2 == 0 { 0.0 } else { s }, min[2] + if i & 4 == 0 { 0.0 } else { s }]);
                d.normals.push([0.0, 0.0, 1.0]);
                d.uvs.push([0.5, 0.5]);
                d.joints.push([joint, 0, 0, 0]);
                d.weights.push([1.0, 0.0, 0.0, 0.0]);
            }
            for t in [[0, 1, 3], [0, 3, 2], [4, 6, 7], [4, 7, 5], [0, 4, 5], [0, 5, 1], [2, 3, 7], [2, 7, 6], [0, 2, 6], [0, 6, 4], [1, 5, 7], [1, 7, 3]] {
                d.tris.push(t);
            }
            d
        };
        let bones = vec![("pelvis".to_string(), [0.0, 0.0, 30.0]), ("jaw".to_string(), [0.0, 0.0, 65.0])];
        SynthModel {
            name: "hero".into(),
            meshes: vec![SynthMesh { name: "body".into(), lod_mask: 1, draws: vec![cube([-10.0, -10.0, 0.0], 20.0, 0, body_mat), cube([-4.0, -4.0, head_at], 8.0, 1, "materials/x/head.vmat")], bones, no_uv: false }],
            skeleton: vec![("pelvis".into(), -1, [0.0, 0.0, 30.0]), ("neck".into(), 0, [0.0, 0.0, 25.0]), ("head".into(), 1, [0.0, 0.0, 5.0]), ("jaw".into(), 2, [0.0, 0.0, 2.0])],
            ..Default::default()
        }
    }

    #[test]
    fn valve_s_own_head_and_mesh_sit_on_valve_s() {
        let valve = mesh::synth::model(&hero(60.0, "materials/x/body.vmat"));
        let g = compare_models(&valve, &valve).unwrap();
        assert_eq!((g.vertices, g.on_valve, g.head, g.head_on_valve, g.valve_head), (16, 16, 8, 8, 8));
        assert_eq!((g.mesh_share(), g.head_share()), (1.0, 1.0));
    }

    #[test]
    fn a_new_head_does_not_sit_on_valve_s() {
        let valve = mesh::synth::model(&hero(60.0, "materials/x/body.vmat"));
        let ours = mesh::synth::model(&hero(70.0, "materials/x/body.vmat"));
        let g = compare_models(&ours, &valve).unwrap();
        assert_eq!((g.head, g.head_on_valve), (8, 0), "{g:?}");
        assert_eq!(g.mesh_share(), 0.5, "the body is still Valve's");
        assert!(compare_models(b"not a model", &valve).unwrap_err().starts_with("our model"));
    }

    #[test]
    fn material_slots_count_valve_s_paths_and_our_draws_in_the_same_place() {
        let valve = mesh::synth::model(&hero(60.0, "materials/x/body.vmat"));
        let shipped: BTreeSet<String> = ["materials/x/body.vmat_c".to_string()].into_iter().collect();
        assert_eq!(material_slots(&valve, None, &shipped).unwrap(), (vec!["materials/x/body.vmat_c".to_string()], vec!["materials/x/head.vmat_c".to_string()]));
        // our model draws its own material in the body's place: that slot is covered by our draw, wherever it lives
        let ours = mesh::synth::model(&hero(60.0, "materials/outfit/silk.vmat"));
        let shipped: BTreeSet<String> = ["materials/outfit/silk.vmat_c".to_string()].into_iter().collect();
        let (covered, missing) = material_slots(&valve, Some(&ours), &shipped).unwrap();
        assert_eq!((covered.len(), missing.len()), (1, 1));
    }

    #[test]
    fn a_rename_is_a_new_localised_name_or_a_new_logo_for_a_new_name() {
        let art = vec!["panorama/images/heroes/hero_names/sinclair.vsvg_c".to_string()];
        let mut paths = BTreeSet::new();
        assert_eq!(renamed("hero_magician", "Sinclair", &art, &paths, &[], Some("Example Skin")), (vec![], vec![]));
        paths.insert(art[0].clone());
        // a new logo for a name that is not Sinclair's
        let (yes, _) = renamed("hero_magician", "Sinclair", &art, &paths, &[], Some("Example Skin"));
        assert_eq!(yes, vec!["replaces the name logo panorama/images/heroes/hero_names/sinclair.vsvg_c for \"Example Skin\" (Valve's \"Sinclair\")"]);
        // Maid Lash: a new logo, but the name keeps the hero's
        let lash = vec!["panorama/images/heroes/hero_names/lash.vsvg_c".to_string()];
        let lp: BTreeSet<String> = lash.iter().cloned().collect();
        let (yes, notes) = renamed("hero_lash", "Lash", &lash, &lp, &[], Some("Maid Lash"));
        assert!(yes.is_empty() && notes[0].contains("keeps \"Lash\""), "{notes:?}");
        // no name to read: noted, not counted
        let (yes, notes) = renamed("hero_magician", "Sinclair", &art, &paths, &[], None);
        assert!(yes.is_empty() && notes[0].contains("set [mod] name"), "{notes:?}");
        // a localisation rename counts on its own
        let loc = vec![("resource/localization/citadel_gc_hero_names/citadel_gc_hero_names_english.txt".to_string(), "		\"hero_magician:n\"	\"Example Skin\"
		\"hero_lash:n\"	\"Lash\"
".to_string())];
        let (yes, _) = renamed("hero_magician", "Sinclair", &[], &BTreeSet::new(), &loc, None);
        assert!(yes.len() == 1 && yes[0].contains("\"Example Skin\" (Valve's \"Sinclair\")"), "{yes:?}");
        assert!(renamed("hero_lash", "Lash", &[], &BTreeSet::new(), &loc, None).0.is_empty(), "Valve's own name is no rename");
    }
}
