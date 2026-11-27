//! One fixture per rule. Each builds the smallest mod that should trip exactly that rule and
//! asserts the rule fires (for that path) and nothing unexpected fires alongside it.

use super::*;
use std::collections::{BTreeSet, HashMap};

const MODEL: &str = "models/heroes_wip/lash/lash.vmdl_c";
const MAT: &str = "models/heroes_wip/lash/materials/lash_body.vmat_c";
const TEX: &str = "models/heroes_wip/lash/materials/lash_body_color_png_f6ecf355.vtex_c";

fn base(build: &str, files: &[(&str, u32)]) -> Baseline {
    let mut b = Baseline { build: build.into(), ..Default::default() };
    for (p, crc) in files {
        b.files.insert(p.to_string(), FileInfo { crc: *crc, size: 1 });
    }
    b
}

fn game() -> Baseline {
    base("6861", &[(MODEL, 1), (MAT, 2), (TEX, 3)])
}

fn file(path: &str, data: Vec<u8>) -> ModFile {
    ModFile { path: path.into(), stored_crc: vpk::crc32(&data), inline: true, data }
}

fn res(refs: &[&str]) -> Vec<u8> {
    resource::synth(refs)
}

fn run(files: Vec<ModFile>, base: Option<&Baseline>, then: Option<&Baseline>) -> Vec<Finding> {
    lint(&Input { vpk_name: "pak03_dir.vpk".into(), files, base, built_against: then, original_refs: no_originals(), game: None, deep: true, fair_play: &policy::DEFAULT })
}

fn codes(f: &[Finding]) -> BTreeSet<&'static str> {
    f.iter().map(|x| x.code).collect()
}

/// A correct mod: replaces Lash's body material, which points at a texture that exists in the game.
fn clean() -> Vec<ModFile> {
    vec![file(MAT, res(&["models/heroes_wip/lash/materials/lash_body_color_png_f6ecf355.vtex"]))]
}

/// Codes every fixture below proves. Checked against RULES so a rule can't be added without one.
const PROVEN: &[&str] = &[
    "M001", "M002", "M010", "M011", "M012", "M020", "M021", "M030", "M031", "M040", "M041", "M050", "M061", "M070", "M071", "M051", "M052", "M060",
    "M013", "M032", "M033", "M042", "M062", "M063", "M064", "M065", "M072", "M073", "M081", "M082", "M083", "M084", "M090", "M091", "M092",
    // the mesh family: fixtures in mesh_fixtures.rs
    "M100", "M101", "M102", "M103", "M104", "M105", "M106", "M107", "M108", "M109", "M110", "M111", "M112", "M113",
    "M114", "M115", "M116", "M117", "M118", "M119", "M120", "M121", "M122", "M123", "M124", "M125", "M126", "M127",
    "M128", "M129", "M130", "M131", "M132", "M133", "M134", "M135", "M136", "M137", "M138", "M139", "M140",
    "M141", "M142", "M143", "M144", "M145", "M146", "M147", "M148", "M149", "M150", "M151", "M152", "M153", "M160", "M170",
];

#[path = "mesh_fixtures.rs"]
mod mesh_fixtures;

#[test]
fn every_rule_has_a_fixture() {
    let rules: BTreeSet<_> = RULES.iter().map(|r| r.code).collect();
    let proven: BTreeSet<_> = PROVEN.iter().copied().collect();
    assert_eq!(rules, proven, "every rule needs a fixture in tests.rs, and every fixture a rule");
}

#[test]
fn clean_mod_is_silent() {
    let g = game();
    assert!(run(clean(), Some(&g), None).is_empty());
}

#[test]
fn m001_uppercase_path() {
    let g = game();
    let m001 = |f: &[Finding]| f.iter().find(|x| x.code == "M001").cloned().unwrap();
    // a file the game would look for (it replaces one), never found with its capitals: an error
    let f = run(vec![file("models/heroes_wip/lash/materials/Lash_Body.vmat_c", res(&[]))], Some(&g), None);
    assert_eq!(m001(&f).severity, Severity::Error, "{f:?}");
    // a new file the mod names: an error too
    let f = run(vec![file(MODEL, res(&["models/heroes_wip/lash/materials/lash_new.vmat"])), file("models/heroes_wip/lash/materials/Lash_New.vmat_c", res(&[]))], Some(&g), None);
    assert_eq!(m001(&f).severity, Severity::Error, "{f:?}");
    // a leftover copy nothing names: never read, a warning that says to remove it
    let f = run(vec![file("sounds/vo/lash/hello - Copy.vsnd_c", res(&[]))], Some(&g), None);
    let x = m001(&f);
    assert!(x.severity == Severity::Warn && x.message.contains("never looks this file up"), "{x:?}");
    // one beside its lowercase twin: the lookup finds the twin
    let f = run(vec![file("models/heroes_wip/lash/materials/lash_body.vmat_c", res(&[])), file("models/heroes_wip/lash/materials/Lash_Body.vmat_c", res(&[]))], Some(&g), None);
    let x = m001(&f);
    assert!(x.severity == Severity::Warn && x.message.contains("a copy of"), "{x:?}");
    // with no game build to look in, it stays an error
    let f = run(vec![file("sounds/vo/lash/hello - Copy.vsnd_c", res(&[]))], None, None);
    assert_eq!(m001(&f).severity, Severity::Error);
    // a font in a pak: Panorama reads fonts from disk through fontconfig, so its case changes nothing; a warning that
    // says so and does not ask for a rename that would not help
    let f = run(vec![file("panorama/fonts/Mine-Bold.ttf", b"font".to_vec())], Some(&g), None);
    let x = m001(&f);
    assert!(x.severity == Severity::Warn && x.message.starts_with(crate::FONT_IN_PAK) && x.message.contains("changes nothing"), "{x:?}");
    // a capitalised file beside the fonts that the mod names is still looked up: an error
    let f = run(vec![file(MODEL, res(&["panorama/fonts/Named.vtex"])), file("panorama/fonts/Named.vtex_c", res(&[]))], Some(&g), None);
    assert_eq!(m001(&f).severity, Severity::Error, "{f:?}");
}

#[test]
fn m170_names_the_compiler_that_built_a_file() {
    let clip = |v: i64| resource::synth_with(&[], &[resource::red2_block(&[("Animation Clip Compiler Version", v)])]);
    let valve_clip = "models/heroes_wip/lash/clips/idle.vnmclip_c";
    let mut g = game();
    with(&mut g, &[valve_clip]);
    let fake = Fake { files: HashMap::from([(valve_clip.to_string(), clip(21))]), ..Default::default() };
    let at = |files: Vec<ModFile>| lint(&Input { vpk_name: "pak03_dir.vpk".into(), files, base: Some(&g), built_against: None, original_refs: no_originals(), game: Some(&fake), deep: false, fair_play: &policy::DEFAULT });
    // two new clips an older kit compiled: one note, with the count, the versions and the kit's known cause
    let f = at(vec![file("models/mine/a.vnmclip_c", clip(14)), file("models/mine/b.vnmclip_c", clip(14))]);
    let m: Vec<&Finding> = f.iter().filter(|x| x.code == "M170").collect();
    assert!(m.len() == 1 && m[0].severity == Severity::Info && m[0].message.contains("2 .vnmclip_c files") && m[0].message.contains("Animation Clip Compiler Version 14 where the game's own") && m[0].message.contains("carry 21") && m[0].message.contains("CSDK 12"), "{m:?}");
    // clips of the game's own version, and Valve's own file against itself: nothing said
    assert!(!at(vec![file("models/mine/a.vnmclip_c", clip(21))]).iter().any(|x| x.code == "M170"));
    assert!(!at(vec![file(valve_clip, clip(21))]).iter().any(|x| x.code == "M170"));
}

#[test]
fn m030_an_animgraph_1_reference_is_a_warning() {
    let g = game();
    let f = run(vec![file(MODEL, res(&["models/heroes_wip/lash/lash.vanmgrph", "models/heroes_wip/lash/gone.vmat"]))], Some(&g), None);
    let m: Vec<&Finding> = f.iter().filter(|x| x.code == "M030").collect();
    assert!(m.iter().any(|x| x.severity == Severity::Warn && x.message.contains("AnimGraph 1") && x.message.contains("lash.vanmgrph_c")), "{m:?}");
    assert!(m.iter().any(|x| x.severity == Severity::Error && x.message == "needs 'models/heroes_wip/lash/gone.vmat_c'"), "a missing material stays an error: {m:?}");
    // an AnimGraph 1 include model the model names in m_refAnimIncludeModels: a warning; the same model referenced
    // for anything else stays an error
    let backup = "models/heroes_wip/lash/lash_backup.vmdl";
    let data = Value::Object(vec![("m_refAnimIncludeModels".into(), Value::Array(vec![Value::Str(backup.into())]))]);
    let f = run(vec![file(MODEL, res_kv(&[backup], &[(b"DATA", data)]))], Some(&g), None);
    let m: Vec<&Finding> = f.iter().filter(|x| x.code == "M030").collect();
    assert!(m.len() == 1 && m[0].severity == Severity::Warn && m[0].message.contains("AnimGraph 1 include model"), "{m:?}");
    let f = run(vec![file(MODEL, res(&[backup]))], Some(&g), None);
    assert!(f.iter().any(|x| x.code == "M030" && x.severity == Severity::Error), "{f:?}");
}

#[test]
fn m002_backslash_path() {
    let g = game();
    let f = run(vec![file("models\\heroes_wip\\lash.vmdl_c", res(&[]))], Some(&g), None);
    assert!(codes(&f).contains("M002"));
}

#[test]
fn m010_uncompiled_source() {
    let g = game();
    let f = run(vec![file("models/heroes_wip/lash/lash.vmdl", b"<!-- kv3 -->".to_vec())], Some(&g), None);
    assert_eq!(codes(&f), BTreeSet::from(["M010"]));
}

#[test]
fn m011_loose_media() {
    let g = game();
    let f = run(vec![file("sounds/vo/lash/hello.wav", b"RIFF".to_vec())], Some(&g), None);
    assert_eq!(codes(&f), BTreeSet::from(["M011"]));
}

#[test]
fn m012_noise_file() {
    let g = game();
    let mut files = clean();
    files.push(file("addoninfo.txt", b"\"AddonInfo\" {}".to_vec()));
    assert_eq!(codes(&run(files, Some(&g), None)), BTreeSet::from(["M012"]));
}

#[test]
fn m012_does_not_fire_on_localization() {
    let g = game();
    let mut files = clean();
    files.push(file("resource/localization/citadel_gc_hero_names/citadel_gc_hero_names_english.txt", b"\"lang\"{}".to_vec()));
    assert!(run(files, Some(&g), None).is_empty());
}

#[test]
fn m020_crc_mismatch() {
    let g = game();
    let mut files = clean();
    files[0].stored_crc ^= 1;
    assert_eq!(codes(&run(files, Some(&g), None)), BTreeSet::from(["M020"]));
}

#[test]
fn m021_not_a_resource() {
    let g = game();
    let f = run(vec![file(MAT, b"definitely not a resource".to_vec())], Some(&g), None);
    assert_eq!(codes(&f), BTreeSet::from(["M021"]));
}

#[test]
fn m030_dangling_reference() {
    let g = game();
    let f = run(vec![file(MAT, res(&["models/heroes_wip/lash/materials/maid_apron_color.vtex"]))], Some(&g), None);
    assert_eq!(codes(&f), BTreeSet::from(["M030"]));
    assert!(f[0].message.contains("maid_apron_color.vtex_c"));
}

#[test]
fn m030_satisfied_by_a_file_in_the_mod() {
    let g = game();
    let files = vec![
        file(MAT, res(&["models/heroes_wip/lash/materials/maid_apron_color.vtex"])),
        file("models/heroes_wip/lash/materials/maid_apron_color.vtex_c", res(&[])),
    ];
    assert!(run(files, Some(&g), None).is_empty(), "a new texture referenced by a replaced material is fine");
}

#[test]
fn m030_ignores_engine_internal_names() {
    let g = game();
    assert!(run(vec![file(MAT, res(&["_rt_scene_color"]))], Some(&g), None).is_empty());
}

#[test]
fn m031_orphan_new_file() {
    let g = game();
    let mut files = clean();
    files.push(file("models/heroes_wip/lash/materials/forgotten.vtex_c", res(&[])));
    assert_eq!(codes(&run(files, Some(&g), None)), BTreeSet::from(["M031"]));
}

#[test]
fn m031_says_when_a_voice_line_is_one_valve_records_for_other_heroes() {
    // Real shape (a voice mod): lines named as Valve names them, for pings this hero has no line for
    let g = base("6868", &[("sounds/vo/astro/ping/astro_ping_see_yamato.vsnd_c", 1), ("sounds/vo/fencer/ping/fencer_ping_affermative_01.vsnd_c", 2)]);
    let f = run(vec![
        file("sounds/vo/fencer/ping/fencer_ping_see_yamato.vsnd_c", res(&[])),
        file("sounds/vo/fencer/ping/fencer_ping_affermative_02.vsnd_c", res(&[])),
    ], Some(&g), None);
    let mut m031: Vec<&str> = f.iter().filter(|x| x.code == "M031").map(|x| x.message.as_str()).collect();
    m031.sort();
    assert_eq!(m031, vec![
        "neither replaces a game file nor is referenced by the mod",
        "neither replaces a game file nor is referenced by the mod: a line Valve records for other heroes (sounds/vo/astro/ping/astro_ping_see_yamato.vsnd), not for fencer, so no sound event plays it",
    ]);
}

#[test]
fn m040_original_changed_since_build() {
    let then = base("6800", &[(MODEL, 1), (MAT, 99), (TEX, 3)]);
    let now = game();
    assert_eq!(codes(&run(clean(), Some(&now), Some(&then))), BTreeSet::from(["M040"]));
}

#[test]
fn m041_original_removed() {
    let then = base("6800", &[(MAT, 2), (TEX, 3), ("models/heroes_wip/lash/materials/old.vmat_c", 5)]);
    let now = game();
    let files = vec![
        file(MAT, res(&["models/heroes_wip/lash/materials/old.vmat"])),
        file("models/heroes_wip/lash/materials/old.vmat_c", res(&[])),
    ];
    assert!(codes(&run(files, Some(&now), Some(&then))).contains("M041"));
}

#[test]
fn m050_bad_vpk_name() {
    let g = game();
    for bad in ["maid_lash.vpk", "pak00_dir.vpk", "pak1_dir.vpk", "pak03.vpk"] {
        let f = lint(&Input { vpk_name: bad.into(), files: clean(), base: Some(&g), built_against: None, original_refs: no_originals(), game: None, deep: true, fair_play: &policy::DEFAULT });
        assert_eq!(codes(&f), BTreeSet::from(["M050"]), "{bad}");
    }
}

#[test]
fn m051_chunked_data() {
    let g = game();
    let mut files = clean();
    files[0].inline = false;
    assert_eq!(codes(&run(files, Some(&g), None)), BTreeSet::from(["M051"]));
}

#[test]
fn m052_empty_vpk() {
    assert_eq!(codes(&run(vec![], None, None)), BTreeSet::from(["M052"]));
}

#[test]
fn m060_model_document_replaced() {
    let g = game();
    let f = run(vec![file(MODEL, res(&["models/heroes_wip/lash/materials/lash_body.vmat"]))], Some(&g), None);
    assert_eq!(codes(&f), BTreeSet::from(["M060"]));
}

fn run_with_originals(files: Vec<ModFile>, originals: &HashMap<String, Vec<String>>) -> Vec<Finding> {
    let g = game();
    lint(&Input { vpk_name: "pak03_dir.vpk".into(), files, base: Some(&g), built_against: None, original_refs: originals, game: None, deep: true, fair_play: &policy::DEFAULT })
}

#[test]
fn m030_not_blamed_for_valves_own_dangling_ref() {
    // Valve's original already points at a file the game doesn't ship (real case: particles/abilities/viscous).
    let ghost = "models/heroes_wip/viscous/viscous_tourist_hat_shape.vmdl";
    let originals = HashMap::from([(MAT.to_string(), vec![format!("{ghost}_c")])]);
    let files = vec![file(MAT, res(&[ghost]))];
    assert!(run_with_originals(files, &originals).is_empty());
}

#[test]
fn m061_dropped_dependency() {
    // The original Lash model references his flog hand particle; a from-scratch replacement forgot it.
    let flog = "particles/abilities/lash/lash_flog_cast_hand_left.vpcf_c";
    let mut g = game();
    g.files.insert(flog.into(), FileInfo { crc: 7, size: 1 });
    let originals = HashMap::from([(MODEL.to_string(), vec![MAT.to_string(), flog.to_string()])]);
    let files = vec![file(MODEL, res(&["models/heroes_wip/lash/materials/lash_body.vmat"]))];
    let f = lint(&Input { vpk_name: "pak03_dir.vpk".into(), files, base: Some(&g), built_against: None, original_refs: &originals, game: None, deep: true, fair_play: &policy::DEFAULT });
    assert_eq!(codes(&f), BTreeSet::from(["M060", "M061"]));
    assert!(f.iter().any(|x| x.code == "M061" && x.message.contains("lash_flog_cast_hand_left")));
}

#[test]
fn m061_silent_when_dropped_ref_was_already_dead() {
    let dead = "models/heroes_wip/haze/materials/haze_blob.vmat_c";
    let originals = HashMap::from([(MAT.to_string(), vec![dead.to_string()])]);
    assert!(run_with_originals(clean(), &originals).is_empty());
}

#[test]
fn m061_silent_when_a_replacement_draws_with_its_own_art() {
    // Real case (6871, three skins, all 105 findings): a restyled material no longer names Valve's compiled textures
    // (whose names carry a hash of their settings), a model draws its own materials, a particle renders its own model.
    let mut g = game();
    let vtex = "materials/particle/noise/noise_shredded_psd_51ae548.vtex_c";
    let fx = "particles/abilities/lash/lash_ground_strike.vpcf_c";
    let prop = "models/particle/rock_chunk.vmdl_c";
    let svg = "panorama/images/hud/lash_icon_psd.vsvg_c";
    for p in [vtex, fx, prop, svg] {
        g.files.insert(p.into(), FileInfo { crc: 7, size: 1 });
    }
    let originals = HashMap::from([
        (MAT.to_string(), vec![vtex.to_string()]),
        (MODEL.to_string(), vec![MAT.to_string()]),
        (fx.to_string(), vec![prop.to_string(), "materials/particle/sparks.vmat_c".to_string(), svg.to_string()]),
    ]);
    g.files.insert("materials/particle/sparks.vmat_c".into(), FileInfo { crc: 7, size: 1 });
    let files = vec![
        file(MAT, res(&["models/heroes_wip/lash/materials/maid_color_psd_0a1b2c3d.vtex"])),
        file(MODEL, res(&["models/heroes_wip/lash/materials/maid_cloth.vmat"])),
        file(fx, res(&[])),
    ];
    let f = lint(&Input { vpk_name: "pak03_dir.vpk".into(), files, base: Some(&g), built_against: None, original_refs: &originals, game: None, deep: true, fair_play: &policy::DEFAULT });
    assert!(!codes(&f).contains("M061"), "{f:?}");
}

#[test]
fn m061_still_fires_when_a_particle_loses_a_child_or_a_model_its_effect() {
    // The true catch: something that plays is gone (a child effect, an event's sound, an anim clip).
    let mut g = game();
    let fx = "particles/abilities/lash/lash_ground_strike.vpcf_c";
    let child = "particles/abilities/lash/lash_ground_strike_dust.vpcf_c";
    let clip = "animgraphs/lash/lash_flog.vnmclip_c";
    for p in [fx, child, clip] {
        g.files.insert(p.into(), FileInfo { crc: 7, size: 1 });
    }
    let originals = HashMap::from([(fx.to_string(), vec![child.to_string()]), (MODEL.to_string(), vec![MAT.to_string(), clip.to_string()])]);
    let files = vec![file(fx, res(&[])), file(MODEL, res(&["models/heroes_wip/lash/materials/lash_body.vmat"]))];
    let f = lint(&Input { vpk_name: "pak03_dir.vpk".into(), files, base: Some(&g), built_against: None, original_refs: &originals, game: None, deep: true, fair_play: &policy::DEFAULT });
    let m: Vec<&str> = f.iter().filter(|x| x.code == "M061").map(|x| x.message.as_str()).collect();
    assert_eq!(m.len(), 2, "{m:?}");
    assert!(m.iter().any(|s| s.contains("lash_ground_strike_dust")) && m.iter().any(|s| s.contains("lash_flog.vnmclip_c")), "{m:?}");
}

#[test]
fn m070_ships_protected_file() {
    let (skel, graph, vdata) = ("models/heroes_wip/lash/lash.vnmskel_c", "animgraphs/animgraph2/hero/lash/lash_loco_set_idle.vnmgraph_c", "scripts/heroes.vdata_c");
    let data = |t: &str| res_kv(&[], &[(b"DATA", kv(t))]);
    let valve = [
        (skel, data(r#"{ m_boneIDs = [ "root", "pelvis", "head" ] }"#)),
        (graph, data(r#"{ m_controlParameterIDs = [ "forward_speed", "crouch_fraction" ] }"#)),
        (vdata, data(r#"{ _include = [ "scripts/other.vdata" ] hero_lash = { m_bDisabled = false } hero_haze = { m_bDisabled = false } }"#)),
    ];
    let mut g = game();
    with(&mut g, &[skel, graph, vdata]);
    let fake = Fake { files: valve.iter().map(|(p, d)| (p.to_string(), d.clone())).collect(), ..Default::default() };
    let one = |p: &str, d: Vec<u8>, base: &Baseline, game: Option<&Fake>| -> Finding {
        let mut files = clean();
        files.push(file(p, d));
        let f = lint(&Input { vpk_name: "pak03_dir.vpk".into(), files, base: Some(base), built_against: None, original_refs: no_originals(), game: game.map(|x| x as &dyn Game), deep: false, fair_play: &policy::DEFAULT });
        f.into_iter().find(|x| x.code == "M070").unwrap_or_else(|| panic!("{p}: no M070"))
    };
    // a copy that holds all Valve's current file has plays today: a warning that names patch day
    for (p, d) in [
        (skel, data(r#"{ m_boneIDs = [ "root", "pelvis", "head" ] }"#)),
        (graph, data(r#"{ m_controlParameterIDs = [ "Forward_Speed", "crouch_fraction", "mine" ] }"#)),
        (vdata, data(r#"{ hero_lash = { m_bDisabled = true } hero_haze = { m_bDisabled = false } }"#)),
    ] {
        let x = one(p, d, &g, Some(&fake));
        assert!(x.severity == Severity::Warn && x.message.contains("plays today") && x.message.contains("patchday"), "{p}: {x:?}");
    }
    // one that already lacks part of it is out of date now: an error that names what
    let x = one(skel, data(r#"{ m_boneIDs = [ "root", "head" ] }"#), &g, Some(&fake));
    assert!(x.severity == Severity::Error && x.message.contains("1 of Valve's bones missing (pelvis)"), "{x:?}");
    let x = one(skel, data(r#"{ m_boneIDs = [ "root", "head", "pelvis" ] }"#), &g, Some(&fake));
    assert!(x.severity == Severity::Error && x.message.contains("another order"), "{x:?}");
    let x = one(graph, data(r#"{ m_controlParameterIDs = [ "forward_speed" ] }"#), &g, Some(&fake));
    assert!(x.severity == Severity::Error && x.message.contains("lacks 1 control parameters the game sets") && x.message.contains("crouch_fraction"), "{x:?}");
    let x = one(vdata, data(r#"{ hero_lash = { } }"#), &g, Some(&fake));
    assert!(x.severity == Severity::Error && x.message.contains("lacks 1 entries") && x.message.contains("hero_haze"), "{x:?}");
    // with no game to compare against it stays an error; at a path Valve does not ship it replaces nothing
    let x = one(vdata, data("{ }"), &g, None);
    assert!(x.severity == Severity::Error && x.message.contains("could not be compared"), "{x:?}");
    let x = one("scripts/my_own.vdata_c", data("{ }"), &g, Some(&fake));
    assert!(x.severity == Severity::Warn && x.message.contains("a path Valve does not ship"), "{x:?}");
}

#[test]
fn m071_engine_default_byproduct() {
    // Real case: resourcecompiler wrote materials/default/default_color_tga_96b3aa03.vtex_c (2,232 bytes; Valve's is 2,184).
    let g = game();
    let mut files = clean();
    files.push(file("materials/default/default_color_tga_96b3aa03.vtex_c", res(&[])));
    assert_eq!(codes(&run(files, Some(&g), None)), BTreeSet::from(["M071"]));
}

#[test]
fn m071_is_an_error_only_when_the_default_draws_another_picture() {
    // Seen on three published mods: their materials/default/ copies decode to Valve's own pixels, or sit at paths Valve
    // does not ship. Neither changes anything in the game; a copy with other pixels changes every fallback.
    let p = "materials/default/default_mask_tga_344101f8.vtex_c";
    let mut g = game();
    with(&mut g, &[p]);
    let white = colour_tex(&[[255, 255, 255, 255]]);
    let fake = Fake { files: [(p.to_string(), white.clone())].into(), ..Default::default() };
    let same = run_game(vec![file(p, white.clone())], &g, &fake);
    let m = same.iter().find(|x| x.code == "M071").unwrap_or_else(|| panic!("{same:?}"));
    assert_eq!(m.severity, Severity::Warn);
    assert!(m.message.contains("same pixels") && m.message.contains("nothing in the game changes"), "{}", m.message);
    let other = run_game(vec![file(p, colour_tex(&[[0, 0, 0, 255]]))], &g, &fake);
    let m = other.iter().find(|x| x.code == "M071").unwrap_or_else(|| panic!("{other:?}"));
    assert_eq!(m.severity, Severity::Error);
    assert!(m.message.contains("pixels differ"), "{}", m.message);
    let elsewhere = "materials/default/default_mask_png_5af4843d.vtex_c";
    let not_valves = run_game(vec![file(elsewhere, white)], &game(), &fake);
    let m = not_valves.iter().find(|x| x.code == "M071").unwrap_or_else(|| panic!("{not_valves:?}"));
    assert_eq!(m.severity, Severity::Warn);
    assert!(m.message.contains("Valve does not ship"), "{}", m.message);
}

#[test]
fn m001_is_not_said_of_a_file_the_game_never_reads() {
    // A published mod's empty README.txt: M012 says it is noise; its case costs nothing, since nothing looks it up.
    let g = game();
    let f = run(vec![file("README.txt", Vec::new())], Some(&g), None);
    assert_eq!(codes(&f), BTreeSet::from(["M012"]), "{f:?}");
}

// ------------------------------------------------------------------ rules that read inside the files

use resource::kv3::{parse_text, Value};

/// The game as the deep rules see it: Valve's originals by path, and the users index.
#[derive(Default)]
struct Fake {
    files: HashMap<String, Vec<u8>>,
    users: HashMap<String, Vec<String>>,
    classes: Option<deep::ParticleClasses>,
    params: Option<matparams::MaterialParams>,
    sources: Option<std::collections::HashSet<String>>,
    /// Set: the game has no users index to read (`users_unchecked`).
    no_index: Option<String>,
    /// The names the game's client code holds (`code_names`).
    code: Option<std::collections::HashSet<String>>,
    /// The particles nothing in the game names (`unreached_particles`).
    unreached: Option<std::collections::HashSet<String>>,
    /// Who outside a model names each bone (`bone_users`).
    bones: Option<crate::carry::BoneUsers>,
}
impl Game for Fake {
    fn read(&self, path: &str) -> Option<Vec<u8>> {
        self.files.get(path).cloned()
    }
    fn users(&self, path: &str) -> Vec<String> {
        self.users.get(path).cloned().unwrap_or_default()
    }
    fn particle_classes(&self) -> Option<&deep::ParticleClasses> {
        self.classes.as_ref()
    }
    fn material_params(&self) -> Option<&matparams::MaterialParams> {
        self.params.as_ref()
    }
    fn addon_sources(&self) -> Option<&std::collections::HashSet<String>> {
        self.sources.as_ref()
    }
    fn users_unchecked(&self) -> Option<String> {
        self.no_index.clone()
    }
    fn code_names(&self) -> Option<&std::collections::HashSet<String>> {
        self.code.as_ref()
    }
    fn unreached_particles(&self) -> Option<&std::collections::HashSet<String>> {
        self.unreached.as_ref()
    }
    fn bone_users(&self) -> Option<&crate::carry::BoneUsers> {
        self.bones.as_ref()
    }
}

#[test]
fn m032_stale_leftover() {
    // Real case: a released skin shipped a bolt shock ring whose source was deleted, because a live particle named it.
    let bolt = "particles/example/magician_bolt.vpcf_c";
    let mut g = game();
    with(&mut g, &[bolt]); // the referrer replaces a game file, so it isn't an orphan
    let ring = "particles/example/magician_bolt_shock_ring.vpcf_c";
    let files = || vec![file(bolt, pcf(&["particles/example/magician_bolt_shock_ring.vpcf"])), file(ring, res(&[]))];
    let tree = |paths: &[&str]| Some(paths.iter().map(|s| s.to_string()).collect::<std::collections::HashSet<String>>());
    let fake = Fake { files: valve_pcf(bolt), sources: tree(&["particles/example/magician_bolt.vpcf"]), ..Default::default() };
    let f = run_game(files(), &g, &fake);
    assert_eq!(codes(&f), BTreeSet::from(["M032"]), "{f:?}");
    assert_eq!(f[0].path.as_deref(), Some(ring));
    assert!(f[0].message.contains(bolt), "names the referrer: {f:?}");
    // with its source restored, nothing fires
    let fake = Fake { files: valve_pcf(bolt), sources: tree(&["particles/example/magician_bolt.vpcf", "particles/example/magician_bolt_shock_ring.vpcf"]), ..Default::default() };
    assert!(run_game(files(), &g, &fake).is_empty());
    // and without a content tree lint can't tell, so it says nothing
    assert!(run_game(files(), &g, &Fake { files: valve_pcf(bolt), ..Default::default() }).is_empty());
    // a leftover reached only through another leftover is not reported again: the root of the stale part is
    let tex = "particles/example/magician_bolt_shock_ring_color_png_0badf00d.vtex_c";
    let chain = vec![
        file(bolt, pcf(&["particles/example/magician_bolt_shock_ring.vpcf"])),
        file(ring, res(&["particles/example/magician_bolt_shock_ring_color_png_0badf00d.vtex"])),
        file(tex, res(&[])),
    ];
    let fake = Fake { files: valve_pcf(bolt), sources: tree(&["particles/example/magician_bolt.vpcf"]), ..Default::default() };
    let f = run_game(chain, &g, &fake);
    assert_eq!(f.iter().filter(|x| x.code == "M032").map(|x| x.path.as_deref().unwrap()).collect::<Vec<_>>(), vec![ring], "{f:?}");
}

#[test]
fn m033_dev_leftover_in_a_release() {
    // Real case: a debug probe child leaked into a skin's release, spawned by a live particle on every cast.
    let bolt = "particles/example/magician_bolt.vpcf_c";
    let probe = "particles/example/magician_bolt_probe.vpcf_c";
    let mut g = game();
    with(&mut g, &[bolt]);
    let files = || vec![file(bolt, pcf(&["particles/example/magician_bolt_probe.vpcf"])), file(probe, res(&[]))];
    let f = run(files(), Some(&g), None);
    assert_eq!(codes(&f), BTreeSet::from(["M033"]), "{f:?}");
    assert_eq!(f[0].path.as_deref(), Some(probe));
    assert_eq!(f[0].severity, Severity::Warn);
    assert!(f[0].message.contains("'probe'") && f[0].message.contains(bolt), "names the marker and the referrer: {f:?}");
    // a folder, a trailing counter and the other markers all count
    for p in [
        "materials/example/debug/swatch.vmat_c",
        "materials/example/test_swatch.vmat_c",
        "particles/example/probe2.vpcf_c",
        "materials/example/tmp-card.vmat_c",
        "materials/example/swatch_scratch.vmat_c",
    ] {
        let f = run(vec![file(p, res(&[]))], Some(&g), None);
        assert!(f.iter().any(|x| x.code == "M033" && x.path.as_deref() == Some(p)), "{p}: {f:?}");
    }
    // whole tokens only: these are real names
    for p in [
        "particles/example/attempt.vpcf_c",
        "particles/example/contest_testing.vpcf_c",
        "materials/example/temperature_ramp.vmat_c",
        "models/heroes_wip/lash/materials/maid_cloth.vmat_c",
        "particles/example/probeless.vpcf_c",
    ] {
        assert!(!run(vec![file(p, res(&[]))], Some(&g), None).iter().any(|x| x.code == "M033"), "{p}");
    }
    // an exporter's name on the material the model draws with is the art on screen, not a leftover; the same material
    // that nothing draws with is still one
    let model = "models/props/example/thing.vmdl_c";
    let mat = "materials/example/exporter_dummy_mat.vmat_c";
    let drawn = run(vec![file(model, res(&[mat.trim_end_matches("_c")])), file(mat, res(&[]))], Some(&g), None);
    assert!(!drawn.iter().any(|x| x.code == "M033"), "{drawn:?}");
    assert!(run(vec![file(mat, res(&[]))], Some(&g), None).iter().any(|x| x.code == "M033"));
    // Valve's own paths are Valve's names: replacing one is not a leftover
    let mut g2 = game();
    with(&mut g2, &["particles/generic/test_beam.vpcf_c"]);
    assert!(!run(vec![file("particles/generic/test_beam.vpcf_c", res(&[]))], Some(&g2), None).iter().any(|x| x.code == "M033"));
    // a debug-named target that dangles is M030's error, and a stale one is M032's: not reported again here
    let f = run(vec![file(bolt, pcf(&["particles/example/missing_probe.vpcf"]))], Some(&g), None);
    assert!(codes(&f).contains("M030") && !codes(&f).contains("M033"), "{f:?}");
    let fake = Fake { files: valve_pcf(bolt), sources: Some(["particles/example/magician_bolt.vpcf".to_string()].into_iter().collect()), ..Default::default() };
    let f = run_game(files(), &g, &fake);
    assert_eq!(codes(&f), BTreeSet::from(["M032"]), "{f:?}");
}

#[test]
fn m083_particle_class_in_the_wrong_list() {
    // Real case: a skin's generator put C_OP_WorldTraceConstraint in m_Operators in 13 particles; the game logged
    // "Function \"C_OP_WorldTraceConstraint\" not found while loading particle system" and crashed in particles.dll.
    let valve_tree = kv(r#"{ _class = "CParticleSystemDefinition"
        m_Operators = [ { _class = "C_OP_Decay" }, { _class = "C_OP_BasicMovement" } ]
        m_Initializers = [ { _class = "C_INIT_RandomColor" } ]
        m_Constraints = [ { _class = "C_OP_WorldTraceConstraint" } ] }"#);
    let mut classes = deep::ParticleClasses::new();
    deep::collect_particle_classes(&valve_tree, &mut classes);
    let classes = deep::parse_particle_classes(&deep::particle_classes_tsv(&classes, "6864"));
    let fake = Fake { classes: Some(classes), ..Default::default() };
    let p = "particles/abilities/magician/magician_hex_projectile.vpcf_c";
    let mut g = game();
    with(&mut g, &[p]);
    let sys = |ops: &str| res_kv(&[], &[(b"DATA", kv(&format!("{{ _class = \"CParticleSystemDefinition\" m_Operators = [ {ops} ] }}")))]);
    let fake_valve = |f: &mut Fake| {
        f.files.insert(p.into(), sys(r#"{ _class = "C_OP_Decay" }"#));
    };
    let mut fake = fake;
    fake_valve(&mut fake);

    let bad = run_game(vec![file(p, sys(r#"{ _class = "C_OP_Decay" }, { _class = "C_OP_WorldTraceConstraint" }"#))], &g, &fake);
    assert_eq!(codes(&bad), BTreeSet::from(["M083"]), "{bad:?}");
    assert!(bad[0].message.contains("m_Operators[1] is C_OP_WorldTraceConstraint, which belongs in m_Constraints"), "{bad:?}");
    assert!(bad[0].message.contains("crashes particles.dll"));

    let misplaced = run_game(vec![file(p, sys(r#"{ _class = "C_INIT_RandomColor" }"#))], &g, &fake);
    assert!(misplaced[0].message.contains("which Valve only uses in m_Initializers"), "{misplaced:?}");

    let unknown = run_game(vec![file(p, sys(r#"{ _class = "C_OP_MadeUpThing" }"#))], &g, &fake);
    assert!(unknown[0].message.contains("the game doesn't know this class"), "{unknown:?}");

    let fine = run_game(vec![file(p, sys(r#"{ _class = "C_OP_Decay" }, { _class = "C_OP_BasicMovement" }"#))], &g, &fake);
    assert!(fine.is_empty(), "{fine:?}");
}

fn kv(text: &str) -> Value {
    parse_text(text).expect("fixture KV3")
}

/// A particle system that decodes (a DATA block with nothing in it), referencing `refs`.
fn pcf(refs: &[&str]) -> Vec<u8> {
    res_kv(refs, &[(b"DATA", kv(r#"{ _class = "CParticleSystemDefinition" }"#))])
}

/// The game's files holding Valve's own (empty) particle at `path`, so a replacement of it can be compared.
fn valve_pcf(path: &str) -> HashMap<String, Vec<u8>> {
    HashMap::from([(path.to_string(), pcf(&[]))])
}

/// A compiled resource with a DATA block (and optional extra blocks such as CTRL or MDAT).
fn res_kv(refs: &[&str], blocks: &[(&[u8; 4], Value)]) -> Vec<u8> {
    let extra: Vec<([u8; 4], Vec<u8>)> = blocks.iter().map(|(t, v)| (**t, resource::kv3::encode(v))).collect();
    resource::synth_with(refs, &extra)
}

fn tex(w: u16, h: u16, format: u8) -> Vec<u8> {
    resource::synth_with(&[], &[(*b"DATA", resource::texture::synth_header(w, h, format))])
}

fn run_game(files: Vec<ModFile>, g: &Baseline, fake: &Fake) -> Vec<Finding> {
    lint(&Input { vpk_name: "pak03_dir.vpk".into(), files, base: Some(g), built_against: None, original_refs: no_originals(), game: Some(fake), deep: true, fair_play: &policy::DEFAULT })
}

fn with(g: &mut Baseline, paths: &[&str]) {
    for (i, p) in paths.iter().enumerate() {
        g.files.insert(p.to_string(), FileInfo { crc: 100 + i as u32, size: 1 });
    }
}

#[test]
fn m013_image_list_leaks_into_a_release() {
    // Real case: panorama/maid_lash_ui_images.vdata, the CSDK hero_image_template pattern, compiled with the PSDs.
    let g = game();
    let list = kv(r#"{ generic_data_type = "panorama_image_list" image_list = [ "file://{images}/heroes/lash_card.psd" ] }"#);
    let mut files = clean();
    files.push(file("panorama/maid_lash_ui_images.vdata_c", res_kv(&[], &[(b"DATA", list)])));
    let f = run(files, Some(&g), None);
    assert!(f.iter().any(|x| x.code == "M013" && x.message.contains("panorama_image_list")), "{f:?}");
    assert!(codes(&f).contains("M070"), "the data-file rule still fires");
}

#[test]
fn m013_toolchain_files() {
    let g = game();
    for p in ["sounds/maid_lash/cache.kv3", "maps/ui/hero_shop/lash_shop.vpk", "resource/cache.sqlite3"] {
        let mut files = clean();
        files.push(file(p, b"x".to_vec()));
        assert_eq!(codes(&run(files, Some(&g), None)), BTreeSet::from(["M013"]), "{p}");
    }
}

#[test]
fn m012_repo_files_are_noise() {
    let g = game();
    for p in ["magician.allow", "materials/.gitignore", "mod.toml"] {
        let mut files = clean();
        files.push(file(p, b"x".to_vec()));
        assert_eq!(codes(&run(files, Some(&g), None)), BTreeSet::from(["M012"]), "{p}");
    }
}

const EVTS: &str = "soundevents/hero/lash.vsndevts_c";

fn events(volume: f64, files: &[&str]) -> Value {
    let list: Vec<String> = files.iter().map(|f| format!("\"{f}\"")).collect();
    kv(&format!(r#"{{ Lash.A1.GroundStrike.Explosion = {{ base = "Base.Ability" volume = {volume} vsnd_files = [ {} ] vsnd_duration = 2.0 }} }}"#, list.join(", ")))
}

#[test]
fn m030_vsndevts_string_reference_dangling() {
    // A pool names a take that isn't in the mod or the game: the event plays silence for that pick.
    let mut g = game();
    with(&mut g, &[EVTS]);
    let mut fake = Fake::default();
    fake.files.insert(EVTS.into(), res_kv(&[], &[(b"DATA", events(3.0, &[]))]));
    let files = vec![file(EVTS, res_kv(&[], &[(b"DATA", events(3.0, &["sounds/maid_lash/abilities/lash_ground_strike_impact_02.vsnd"]))]))];
    let f = run_game(files, &g, &fake);
    assert_eq!(codes(&f), BTreeSet::from(["M030"]), "{f:?}");
    assert!(f[0].message.contains("lash_ground_strike_impact_02.vsnd_c"));
}

#[test]
fn m031_silent_for_a_take_named_by_the_event_file() {
    // The class behind 20 Maid Lash allowances: pool files referenced only by vsndevts strings.
    let mut g = game();
    with(&mut g, &[EVTS]);
    let mut fake = Fake::default();
    fake.files.insert(EVTS.into(), res_kv(&[], &[(b"DATA", events(3.0, &[]))]));
    let take = "sounds/maid_lash/abilities/lash_ground_strike_impact_02.vsnd";
    let files = vec![
        file(EVTS, res_kv(&[], &[(b"DATA", events(3.0, &[take]))])),
        file(&format!("{take}_c"), res(&[])),
    ];
    assert!(run_game(files, &g, &fake).is_empty());
}

#[test]
fn m063_event_quieter_or_removed() {
    let mut g = game();
    with(&mut g, &[EVTS]);
    let mut fake = Fake::default();
    fake.files.insert(EVTS.into(), res_kv(&[], &[(b"DATA", events(3.0, &[]))]));
    let quiet = vec![file(EVTS, res_kv(&[], &[(b"DATA", events(-4.0, &[]))]))];
    let f = run_game(quiet, &g, &fake);
    assert_eq!(codes(&f), BTreeSet::from(["M063"]), "{f:?}");
    assert!(f[0].message.contains("volume -4"));
    let gone = vec![file(EVTS, res_kv(&[], &[(b"DATA", kv("{ Lash.Other = { volume = 0 } }"))]))];
    assert!(run_game(gone, &g, &fake).iter().any(|x| x.code == "M063" && x.message.contains("event removed")));
}

#[test]
fn m063_reads_only_the_keys_that_set_how_far_a_sound_carries() {
    // Real keys from Valve's 160 event files (6871). Where a sound folds to mono, whether a distance filter colours it
    // and how far a limiter counts instances name a distance but are not reach: changing or adding them hides no cue.
    let valve = kv("{ Lash.Cast = { volume = 0.0 mono_downmix_min_distance = 400.0 distance_lpf_on = 1.0 limiter_global_event_distance_max = 2000.0 volume_falloff_max = 3000.0 } }");
    let ours = kv("{ Lash.Cast = { volume = 0.0 mono_downmix_min_distance = 100.0 mono_downmix_max_distance = 300.0 distance_lpf_on = 0.0 limiter_global_event_distance_max = 500.0 volume_falloff_max = 3000.0 } }");
    let (less, _) = deep::sound_events(&ours, &valve, &policy::DEFAULT, &|_| None);
    assert!(less.is_empty(), "{less:?}");
    // the true catches still fire: a shorter falloff, a cut-off added, a falloff curve dropped, occlusion sooner
    let valve = kv("{ Lash.Cast = { volume = 0.0 volume_falloff_max = 3000.0 volume_falloff_curve_db = [ 0.0, -6.0 ] occlusion_radius = 40.0 } }");
    let ours = kv("{ Lash.Cast = { volume = 0.0 volume_falloff_max = 1500.0 cull_at_distance = 1000.0 occlusion_radius = 10.0 } }");
    let (less, _) = deep::sound_events(&ours, &valve, &policy::DEFAULT, &|_| None);
    for want in ["volume_falloff_max 1500", "cull_at_distance added", "volume_falloff_curve_db removed", "occlusion_radius 10"] {
        assert!(less.iter().any(|l| l.contains(want)), "{want}: {less:?}");
    }
}

#[test]
fn m063_a_voice_lines_event_is_not_judged_by_its_length() {
    // Real case (6871, community voice packs, 2,241 findings): a recorded line under sounds/vo/ is shorter than
    // Valve's, which Valve's own voices are against each other on 46% of shared lines. Its file is not judged by length,
    // so neither is its event; a shorter ability sound still is.
    let valve = kv(r#"{ Atlas.Kill = { vsnd_files = [ "sounds/vo/atlas/atlas_kill_01.vsnd" ] vsnd_duration = 2.28 } Atlas.Cast = { vsnd_files = "sounds/abilities/atlas/cast.vsnd" vsnd_duration = 1.0 } }"#);
    let ours = kv(r#"{ Atlas.Kill = { vsnd_files = [ "sounds/vo/atlas/atlas_kill_01.vsnd" ] vsnd_duration = 1.95 } Atlas.Cast = { vsnd_files = "sounds/abilities/atlas/cast.vsnd" vsnd_duration = 0.5 } }"#);
    let (less, _) = deep::sound_events(&ours, &valve, &policy::DEFAULT, &|_| None);
    assert_eq!(less, vec!["Atlas.Cast: plays 0.50 s, Valve's 1.00 s".to_string()]);
}

#[test]
fn m063_sound_file_shortened() {
    let snd = "sounds/abilities/lash/a1_ground_strike/lash_ground_strike_impact.vsnd_c";
    let ctrl = |dur: f64, peak: f64| {
        kv(&format!(
            "{{ m_vSound = {{ m_nRate = 44100 m_flDuration = {dur} }} m_pEnvelopeAnalyzer = {{ m_curve = {{ m_spline = [ {{ x = 0 y = {peak} }} ] }} }} }}"
        ))
    };
    let mut g = game();
    with(&mut g, &[snd]);
    let mut fake = Fake::default();
    fake.files.insert(snd.into(), res_kv(&[], &[(b"CTRL", ctrl(2.49, 0.5))]));
    let f = run_game(vec![file(snd, res_kv(&[], &[(b"CTRL", ctrl(1.2, 0.5))]))], &g, &fake);
    assert_eq!(codes(&f), BTreeSet::from(["M063"]), "{f:?}");
    let same = run_game(vec![file(snd, res_kv(&[], &[(b"CTRL", ctrl(2.49, 0.52))]))], &g, &fake);
    assert!(same.is_empty(), "same length, within 1.5 dB: {same:?}");
}

#[test]
fn m063_a_base_valve_inherits_from_differs_only_by_what_lies_between() {
    // Seen on a published mod's old sound-event file: casts on "Base.Ability" where Valve's are on "Base.Ability.Cast",
    // which is Base.Ability plus one flag that sets no level and no reach; melee on "Base.Movement.Melee" where Valve's
    // is on a default that is Base.Movement.Melee at -2 dB. The first changes nothing heard, the second plays louder.
    let base = "soundevents/base/example_base_events.vsndevts_c";
    let mut g = game();
    with(&mut g, &[EVTS, base]);
    let mut fake = Fake::default();
    fake.files.insert(base.into(), res_kv(&[], &[(b"DATA", kv(r#"{ Base.Ability = { type = "x" falloff_far = 2000.0 }
        Base.Ability.Cast = { base = "Base.Ability" volume_enemy_targeting_boost_disable_update = 1.0 }
        Base.Melee = { type = "x" } Hero.Melee = { base = "Base.Melee" volume = -2.0 }
        Base.Other = { falloff_far = 100.0 } }"#))]));
    fake.files.insert(EVTS.into(), res_kv(&[], &[(b"DATA", kv(r#"{ Lash.Cast = { base = "Base.Ability.Cast" } Lash.Swing = { base = "Hero.Melee" } }"#))]));
    let ours = |text: &str| run_game(vec![file(EVTS, res_kv(&[], &[(b"DATA", kv(text))]))], &g, &fake);
    let f = ours(r#"{ Lash.Cast = { base = "Base.Ability" } Lash.Swing = { base = "Base.Melee" } }"#);
    assert!(!f.iter().any(|x| x.code == "M063"), "an ancestor whose difference is no level or reach: {f:?}");
    let m = f.iter().find(|x| x.code == "M064").unwrap_or_else(|| panic!("{f:?}"));
    assert!(m.message.contains("Lash.Swing") && m.message.contains("2.0 dB louder"), "{}", m.message);
    // a base on another branch is still read as Valve's stack replaced
    let f = ours(r#"{ Lash.Cast = { base = "Base.Other" } Lash.Swing = { base = "Hero.Melee" } }"#);
    assert!(f.iter().any(|x| x.code == "M063" && x.message.contains("the base stack sets falloff")), "{f:?}");
}

#[test]
fn m063_a_list_of_bases_is_read_as_its_events() {
    // Valve writes `base` as one name or as a list of { event_name field_name }: the same base in either form is the
    // same stack, and a list that keeps Valve's base and adds one differs only by what the added base sets
    let base = "soundevents/base/example_base_events.vsndevts_c";
    let mut g = game();
    with(&mut g, &[EVTS, base]);
    let mut fake = Fake::default();
    fake.files.insert(base.into(), res_kv(&[], &[(b"DATA", kv(r#"{ Base.Ability.Cast = { type = "x" falloff_far = 2000.0 }
        Base.Quiet = { volume = -3.0 } Base.Flag = { some_flag = 1.0 } }"#))]));
    fake.files.insert(EVTS.into(), res_kv(&[], &[(b"DATA", kv(r#"{ Lash.Cast = { base = "Base.Ability.Cast" } }"#))]));
    let ours = |text: &str| run_game(vec![file(EVTS, res_kv(&[], &[(b"DATA", kv(text))]))], &g, &fake);
    let same = ours(r#"{ Lash.Cast = { base = [ { event_name = "Base.Ability.Cast" field_name = "x" } ] } }"#);
    assert!(!same.iter().any(|x| x.code == "M063"), "the same base as a list: {same:?}");
    let flag = ours(r#"{ Lash.Cast = { base = [ { event_name = "Base.Ability.Cast" } { event_name = "Base.Flag" } ] } }"#);
    assert!(!flag.iter().any(|x| x.code == "M063"), "an added base that sets no level or reach: {flag:?}");
    let quiet = ours(r#"{ Lash.Cast = { base = [ { event_name = "Base.Ability.Cast" } { event_name = "Base.Quiet" } ] } }"#);
    assert!(quiet.iter().any(|x| x.code == "M063" && x.message.contains("3.0 dB quieter")), "an added base that lowers it: {quiet:?}");
    let other = ours(r#"{ Lash.Cast = { base = [ { event_name = "Base.Flag" } ] } }"#);
    assert!(other.iter().any(|x| x.code == "M063" && x.message.contains("the base stack sets falloff")), "Valve's base dropped: {other:?}");
}

#[test]
fn m063_a_voice_line_is_not_judged_by_its_length() {
    // Valve's own voices fail the duration test against each other on 46% of shared lines (`magician corpus`, 6868).
    let line = "sounds/vo/lash/emote/lash_effort_dash_02.vsnd_c";
    let ctrl = |dur: f64| kv(&format!("{{ m_vSound = {{ m_flDuration = {dur} }} }}"));
    let mut g = game();
    with(&mut g, &[line]);
    let mut fake = Fake::default();
    fake.files.insert(line.into(), res_kv(&[], &[(b"CTRL", ctrl(0.8))]));
    for d in [0.3, 5.0] {
        let f = run_game(vec![file(line, res_kv(&[], &[(b"CTRL", ctrl(d))]))], &g, &fake);
        assert!(!f.iter().any(|x| x.code == "M063" || x.code == "M064"), "{d} s: {f:?}");
    }
}

#[test]
fn m144_a_fair_play_comparison_that_could_not_run() {
    let mut g = game();
    with(&mut g, &[EVTS, "particles/abilities/lash/lash_flog.vpcf_c"]);
    // a replaced sound event whose data does not decode, against a Valve original that does
    let mut fake = Fake::default();
    fake.files.insert(EVTS.into(), res_kv(&[], &[(b"DATA", events(3.0, &[]))]));
    let f = run_game(vec![file(EVTS, res(&[]))], &g, &fake);
    assert_eq!(codes(&f), BTreeSet::from(["M144"]), "{f:?}");
    assert!(f[0].message.contains("its data did not decode") && f[0].message.contains("M063"), "{f:?}");
    // the other way round: ours decodes, Valve's original does not (a new format after a patch)
    let mut newer = Fake::default();
    newer.files.insert(EVTS.into(), res(&[]));
    let f = run_game(vec![file(EVTS, res_kv(&[], &[(b"DATA", events(3.0, &[]))]))], &g, &newer);
    assert!(f.iter().any(|x| x.code == "M144" && x.message.contains("Valve's original")), "{f:?}");
    // a replaced particle with no decodable data, and one whose Valve original is unreadable
    let pcf_path = "particles/abilities/lash/lash_flog.vpcf_c";
    let f = run_game(vec![file(pcf_path, res(&[]))], &g, &Fake { files: valve_pcf(pcf_path), ..Default::default() });
    assert!(f.iter().any(|x| x.code == "M144" && x.message.contains("M081")), "{f:?}");
    let f = run_game(vec![file(pcf_path, pcf(&[]))], &g, &Fake::default());
    assert!(f.iter().any(|x| x.code == "M144" && x.message.contains("Valve's original")), "{f:?}");
    // with no game to compare against there is no comparison to have skipped
    assert!(run(vec![file(EVTS, res(&[]))], Some(&g), None).iter().all(|x| x.code != "M144"));
}

#[test]
fn fair_play_limits_outside_the_mesh_move_with_the_policy() {
    let mut g = game();
    with(&mut g, &[EVTS]);
    let mut fake = Fake::default();
    fake.files.insert(EVTS.into(), res_kv(&[], &[(b"DATA", events(3.0, &[]))]));
    let at = |slack: f64, ours: f64| {
        let mut p = policy::DEFAULT;
        p.sound_volume_slack_db = slack;
        let files = vec![file(EVTS, res_kv(&[], &[(b"DATA", events(ours, &[]))]))];
        lint(&Input { vpk_name: "pak03_dir.vpk".into(), files, base: Some(&g), built_against: None, original_refs: no_originals(), game: Some(&fake), deep: true, fair_play: &p })
    };
    // Valve's event plays at 3 dB; ours at 1 dB is 2 dB quieter: a hidden cue under a 1.99 dB slack, not at 2 or 2.01
    assert!(at(1.99, 1.0).iter().any(|x| x.code == "M063"));
    assert!(at(2.0, 1.0).iter().all(|x| x.code != "M063"));
    assert!(at(2.01, 1.0).iter().all(|x| x.code != "M063"));
    assert!(at(policy::DEFAULT.sound_volume_slack_db, 1.0).iter().any(|x| x.code == "M063"), "the default still catches it");
}

#[test]
fn m064_event_louder() {
    let mut g = game();
    with(&mut g, &[EVTS]);
    let mut fake = Fake::default();
    fake.files.insert(EVTS.into(), res_kv(&[], &[(b"DATA", events(3.0, &[]))]));
    let f = run_game(vec![file(EVTS, res_kv(&[], &[(b"DATA", events(9.0, &[]))]))], &g, &fake);
    assert_eq!(codes(&f), BTreeSet::from(["M064"]), "{f:?}");
}

fn model(radius: f64, attach_z: f64, extra_bone: bool) -> Vec<u8> {
    model_q(radius, attach_z, extra_bone, "[ 0, 0, 0, 1 ]")
}

fn model_q(radius: f64, attach_z: f64, extra_bone: bool, q: &str) -> Vec<u8> {
    let bones = if extra_bone { r#"[ "root_motion", "pelvis", "head", "maid_streamer_0" ]"# } else { r#"[ "root_motion", "pelvis", "head" ]"# };
    let parents = if extra_bone { "[ -1, 0, 1, 1 ]" } else { "[ -1, 0, 1 ]" };
    let data = kv(&format!("{{ m_name = \"x\" m_modelSkeleton = {{ m_boneName = {bones} m_nParent = {parents} }} }}"));
    let mesh = kv(&format!(
        r#"{{ _class = "CRenderMesh"
            m_attachments = [ {{ key = "aim" value = {{ m_name = "aim" m_influenceNames = [ "root_motion", "", "" ]
                m_vInfluenceOffsets = [ [ 0.0, 0.0, {attach_z} ], [ 0, 0, 0 ], [ 0, 0, 0 ] ]
                m_vInfluenceRotations = [ {q}, [ 0, 0, 0, 1 ], [ 0, 0, 0, 1 ] ] }} }} ]
            m_hitboxsets = [ {{ key = "default" value = {{ m_name = "default" m_HitBoxes = [
                {{ m_name = "head_capsule" m_sBoneName = "head" m_vMinBounds = [ 2.75, 3.63, 0 ] m_vMaxBounds = [ 7.89, 0, 0 ]
                   m_flShapeRadius = {radius} m_nShapeType = 2 m_nGroupId = 1 }} ] }} }} ] }}"#
    ));
    res_kv(&[], &[(b"DATA", data), (b"MDAT", mesh)])
}

#[test]
fn m062_model_contract() {
    let mut g = game();
    let mut fake = Fake::default();
    fake.files.insert(MODEL.into(), model(6.70, 89.88, false));
    with(&mut g, &[MODEL]);
    // Added bones (cloth chains) are fine; the contract holds.
    let ok = run_game(vec![file(MODEL, model(6.70, 89.88, true))], &g, &fake);
    assert_eq!(codes(&ok), BTreeSet::from(["M060"]), "{ok:?}");
    // A fatter head hitbox and a moved aim attachment break it.
    let bad = run_game(vec![file(MODEL, model(7.5, 80.0, false))], &g, &fake);
    assert_eq!(codes(&bad), BTreeSet::from(["M060", "M062"]));
    assert!(bad.iter().any(|x| x.code == "M062" && x.message.contains("hitbox 'default/head_capsule' changed")));
    assert!(bad.iter().any(|x| x.code == "M062" && x.message.contains("attachment 'aim' changed")));
    // with no mesh to measure against, a moved attachment cannot be shown to be cosmetic: it stays an error
    assert!(bad.iter().filter(|x| x.code == "M062").all(|x| x.severity == Severity::Error), "{bad:?}");
}

#[test]
fn m062_a_float32_step_across_a_rounding_line_is_not_a_change() {
    // Real case (6871, a community skin): Valve's pelvis capsule radius 5.774999618 re-exported as 5.775000095, which
    // writes 5.78 where Valve's writes 5.77. Never allowable, so a false one cannot be waived: it must not fire.
    let mut g = game();
    let mut fake = Fake::default();
    fake.files.insert(MODEL.into(), model(5.774999618530273, 89.88, false));
    with(&mut g, &[MODEL]);
    let same = run_game(vec![file(MODEL, model(5.775000095367432, 89.88, false))], &g, &fake);
    assert_eq!(codes(&same), BTreeSet::from(["M060"]), "{same:?}");
    // two hundredths of a unit wider is a changed hitbox
    let wider = run_game(vec![file(MODEL, model(5.795, 89.88, false))], &g, &fake);
    assert!(wider.iter().any(|x| x.code == "M062" && x.message.contains("hitbox 'default/head_capsule' changed")), "{wider:?}");
}

#[test]
fn m062_says_when_an_attachment_is_valve_s_with_its_axes_turned() {
    // Real shape (two mods from one exporter): every attachment on the root at Valve's numbers in another order,
    // zipline 10.43,10.89,90.36 written as 10.89,90.36,10.43.
    let c = |at: &str| deep::Contract { bones: vec![], attachments: vec![("zipline".into(), format!("on root_motion at {at};0.00,0.00,0.00 rot 0.00,0.00,0.00,1.00"))], hitboxes: vec![] };
    let valve = c("10.43,10.89,90.36");
    let turned = deep::contract_breaches(&c("10.89,90.36,10.43"), &valve);
    assert_eq!(turned.len(), 1);
    assert_eq!(turned[0], format!("attachment 'zipline' changed: on root_motion at 10.89,90.36,10.43 rot 0.00,0.00,0.00,1.00 (Valve's on root_motion at 10.43,10.89,90.36 rot 0.00,0.00,0.00,1.00); {}", deep::AXES_TURNED), "the unused influence slots are left out");
    // moved, not turned: the finding alone
    let moved = deep::contract_breaches(&c("10.43,10.89,80.00"), &valve);
    assert!(moved.len() == 1 && moved[0].ends_with(")"), "{moved:?}");
    // At the origin it is the rotation that shows it: Valve's turned by the same 120 degrees about (1, 1, 1), which
    // the exporter put on every attachment (real numbers: Valve's identity as -0.5,-0.5,-0.5,0.5, and Valve's
    // 0,0.71,0,0.71 as 0,0,-0.71,0.71).
    let r = |at: &str, rot: &str| deep::Contract { attachments: vec![("a".into(), format!("on root_motion at {at};0.00,0.00,0.00 rot {rot};0.00,0.00,0.00,1.00"))], ..Default::default() };
    let o = "0.00,0.00,0.00";
    assert!(deep::contract_breaches(&r(o, "-0.50,-0.50,-0.50,0.50"), &r(o, "0.00,0.00,0.00,1.00"))[0].ends_with(deep::AXES_TURNED));
    assert!(deep::contract_breaches(&r(o, "0.00,0.00,-0.71,0.71"), &r(o, "0.00,0.71,0.00,0.71"))[0].ends_with(deep::AXES_TURNED));
    assert!(deep::contract_breaches(&r("-33.12,82.80,-82.80", "-0.50,-0.50,-0.50,0.50"), &r("-82.80,-33.12,82.80", "0.00,0.00,0.00,1.00"))[0].ends_with(deep::AXES_TURNED));
    // another rotation at the origin is a change, not a turn
    assert!(deep::contract_breaches(&r(o, "0.71,0.00,0.00,0.71"), &r(o, "0.00,0.00,0.00,1.00"))[0].ends_with(")"));
}

#[test]
fn m062_hitboxes_that_share_a_name_are_compared_as_a_set() {
    // Real case (Haze, Kelvin, Seven on 6865): two hitboxes of one name, a box and a capsule. Valve's own file
    // against itself used to read as "changed", the second against the first.
    let c = |boxes: &[(&str, &str)]| deep::Contract { bones: vec![], attachments: vec![], hitboxes: boxes.iter().map(|(n, s)| (n.to_string(), s.to_string())).collect() };
    let valve = c(&[("default/spine_1", "capsule r 10.35"), ("default/spine_1", "box 17 x 13 x 16"), ("default/head", "sphere r 5.4")]);
    assert!(deep::contract_breaches(&valve, &valve).is_empty());
    assert!(deep::contract_breaches(&c(&[("default/spine_1", "box 17 x 13 x 16"), ("default/spine_1", "capsule r 10.35"), ("default/head", "sphere r 5.4")]), &valve).is_empty(), "in any order");
    // one of the two changed, and one of the two dropped, are both said
    let fat = deep::contract_breaches(&c(&[("default/spine_1", "capsule r 12"), ("default/spine_1", "box 17 x 13 x 16"), ("default/head", "sphere r 5.4")]), &valve);
    assert_eq!(fat, vec!["hitbox 'default/spine_1' changed: capsule r 12 (Valve's capsule r 10.35)"]);
    let one = deep::contract_breaches(&c(&[("default/spine_1", "capsule r 10.35"), ("default/head", "sphere r 5.4")]), &valve);
    assert_eq!(one, vec!["hitbox 'default/spine_1' missing: Valve's model has another of that name, box 17 x 13 x 16"]);
    // a model with more meshes repeats an unchanged one more often: copies are not a change
    let twice = c(&[("default/spine_1", "capsule r 10.35"), ("default/spine_1", "box 17 x 13 x 16"), ("default/head", "sphere r 5.4"), ("default/head", "sphere r 5.4")]);
    assert!(deep::contract_breaches(&twice, &valve).is_empty() && deep::contract_breaches(&valve, &twice).is_empty());
}

#[test]
fn m062_quaternion_sign_is_not_a_change() {
    // Real case (Maid Lash vs Valve's lash.vmdl): weapon_r_fx stored as (0,0,1,0) by us, (-0,-0,-1,-0) by Valve.
    let mut g = game();
    let mut fake = Fake::default();
    fake.files.insert(MODEL.into(), model_q(6.70, 89.88, false, "[ -0.0000002, 0.7071, -0.0000001, -0.7071 ]"));
    with(&mut g, &[MODEL]);
    let f = run_game(vec![file(MODEL, model_q(6.70, 89.88, false, "[ 0.0000001, -0.7071, 0.0000002, 0.7071 ]"))], &g, &fake);
    assert_eq!(codes(&f), BTreeSet::from(["M060"]), "{f:?}");
}

#[test]
fn m062_compiler_cloth_nodes_are_not_contract() {
    // Real case (a skin's clone turret): Valve's model carries 554 `$cloth_m0p<n>` nodes generated from its cloth.
    use resource::kv3::Value as V;
    let skel = |names: &[&str]| {
        let names: Vec<V> = names.iter().map(|n| V::Str(n.to_string())).collect();
        let parents: Vec<V> = (0..names.len()).map(|i| V::Int(i as i64 - 1)).collect();
        res_kv(&[], &[(b"DATA", V::Object(vec![("m_modelSkeleton".into(), V::Object(vec![("m_boneName".into(), V::Array(names)), ("m_nParent".into(), V::Array(parents))]))]))])
    };
    let mut g = game();
    let mut fake = Fake::default();
    fake.files.insert(MODEL.into(), skel(&["root_motion", "pelvis", "$cloth_m0p318"]));
    with(&mut g, &[MODEL]);
    assert_eq!(codes(&run_game(vec![file(MODEL, skel(&["root_motion", "pelvis"]))], &g, &fake)), BTreeSet::from(["M060"]));
    let f = run_game(vec![file(MODEL, skel(&["root_motion", "spine"]))], &g, &fake);
    assert!(f.iter().any(|x| x.code == "M062" && x.message.contains("bone 'pelvis' missing")), "{f:?}");
}

/// Valve's game data for the hero fixture, as Haze's model writes it on 6875 (shortened): the unit status on the
/// `healthbar` attachment, one tagged-sound set, the camera, the camera poses and a bone constraint list.
const GAMEDATA: &str = r#"<!-- kv3 encoding:text:version{e21c7f3c-8a33-41c5-9977-a76d3a32aa0d} format:vmdlkeys4:version{9f5c1002-f19f-470c-bd42-c3c26c063d33} -->
{
    CitadelUnitStatusSettings_t = { m_vUnitStatusOffset = [ 10.0, 0.0, 0.0 ] m_vHealthbarOffset = [ 0.0, 0.0, 0.0 ] m_vDamageNumbersOffset = [ 0.0, 15.0, 82.0 ] m_vStatusEffectsOffset = [ 0.0, 0.0, 0.0 ] m_strUnitStatusAttachmentName = "healthbar" }
    TaggedSoundSettings_t = { m_taggedSounds = [ resource:"scripts/tagged_sounds/haze_anim_sounds.vdata" ] }
    CitadelCameraSettings_t = { m_flCameraSideOffset = -34.0 m_flCameraBackOffset = 103.0 m_flCameraHeightStanding = 86.0 }
    AttachmentCameraData = [ { camera_name = "" attachment_name = "near_00" screen_width = 1920 screen_height = 1080 fov = 90 } ]
    BoneConstraintList = [ ]
}"#;

/// A hero model with game data `gd` (none when empty), animation skeleton `skel`, graphs `graphs` (identifier, path),
/// and two attachments on the root: the camera pose `near_00` turned `near_deg` about z, and `standing_pivot` turned
/// `pivot_deg`, and a `healthbar` on the head.
fn hero(gd: &str, skel: &[&str], graphs: &[(&str, &str)], near_deg: f64, pivot_deg: f64) -> Vec<u8> {
    use resource::kv3::Value as V;
    let s = |x: &str| V::Str(x.to_string());
    let mut data = vec![
        ("m_name".to_string(), s("x")),
        ("m_modelSkeleton".to_string(), V::Object(vec![("m_boneName".into(), V::Array(vec![s("root_motion"), s("pelvis"), s("head")])), ("m_nParent".into(), V::Array(vec![V::Int(-1), V::Int(0), V::Int(1)]))])),
        ("m_vecNmSkeletonRefs".to_string(), V::Array(skel.iter().map(|x| s(x)).collect())),
        ("m_animGraph2Refs".to_string(), V::Array(graphs.iter().map(|(i, g)| V::Object(vec![("m_sIdentifier".into(), s(i)), ("m_hGraph".into(), s(g))])).collect())),
    ];
    if !gd.is_empty() {
        data.push(("m_modelInfo".to_string(), V::Object(vec![("m_keyValueText".into(), s(gd))])));
    }
    let q = |deg: f64| {
        let h = deg.to_radians() / 2.0;
        format!("[ 0.0, 0.0, {}, {} ]", h.sin(), h.cos())
    };
    let att = |name: &str, bone: &str, at: &str, rot: &str| format!(r#"{{ key = "{name}" value = {{ m_name = "{name}" m_influenceNames = [ "{bone}", "", "" ] m_vInfluenceOffsets = [ {at}, [ 0, 0, 0 ], [ 0, 0, 0 ] ] m_vInfluenceRotations = [ {rot}, [ 0, 0, 0, 1 ], [ 0, 0, 0, 1 ] ] m_nInfluences = 1 }} }}"#);
    let mesh = kv(&format!(
        r#"{{ _class = "CRenderMesh" m_attachments = [ {}, {}, {} ] }}"#,
        att("near_00", "root_motion", "[ -16.1, -16.1, 86.25 ]", &q(near_deg)),
        att("standing_pivot", "root_motion", "[ 0.0, 0.0, 86.2 ]", &q(pivot_deg)),
        att("healthbar", "head", "[ 0.0, 0.0, 10.0 ]", "[ 0, 0, 0, 1 ]"),
    ));
    res_kv(&[], &[(b"DATA", V::Object(data)), (b"MDAT", mesh)])
}

const HERO_SKEL: &str = "models/heroes_staging/haze/haze.vnmskel";
const HERO_GRAPHS: &[(&str, &str)] = &[("", "animgraphs/animgraph2/hero/hero.vnmgraph+haze.vnmgraph"), ("ui", "animgraphs/animgraph2/hero/hero_ui.vnmgraph+haze.vnmgraph")];

/// The M062 findings of a hero fixture against Valve's (the full one), with `code` as the client's names.
fn hero_findings(ours: Vec<u8>, code: Option<&[&str]>) -> Vec<Finding> {
    let mut g = game();
    with(&mut g, &[MODEL]);
    let mut fake = Fake::default();
    fake.files.insert(MODEL.into(), hero(GAMEDATA, &[HERO_SKEL], HERO_GRAPHS, 90.0, 90.0));
    fake.code = code.map(|c| c.iter().map(|s| s.to_string()).collect());
    run_game(vec![file(MODEL, ours)], &g, &fake).into_iter().filter(|x| x.code == "M062").collect()
}

#[test]
fn m062_reads_the_hero_s_game_data_against_valve_s() {
    // Valve's own model against itself: nothing
    assert!(hero_findings(hero(GAMEDATA, &[HERO_SKEL], HERO_GRAPHS, 90.0, 90.0), Some(&["standing_pivot"])).is_empty());
    // the same data written by another compile (other order, integers for floats, an identifier in capitals, keys
    // Valve's has not): the same contract
    let reworded = GAMEDATA.replace("m_flCameraSideOffset = -34.0 m_flCameraBackOffset = 103.0", "m_flCameraBackOffset = 103 m_flCameraSideOffset = -34 m_vCameraParrotOffset = [ -10, -10, 10 ]");
    let caps: Vec<(&str, &str)> = vec![HERO_GRAPHS[0], ("UI", HERO_GRAPHS[1].1)];
    assert!(hero_findings(hero(&reworded, &[HERO_SKEL], &caps, 90.0, 90.0), None).is_empty(), "{:?}", hero_findings(hero(&reworded, &[HERO_SKEL], &caps, 90.0, 90.0), None));

    // a CSDK 12 compile: the unit status written as a list. No place for the bar (seen on 6880 for a missing block): an error
    let list = GAMEDATA.replace("CitadelUnitStatusSettings_t = {", "CitadelUnitStatusSettings_t = [ {").replace("\"healthbar\" }", "\"healthbar\" } ]");
    let f = hero_findings(hero(&list, &[HERO_SKEL], HERO_GRAPHS, 90.0, 90.0), None);
    assert!(f.len() == 1 && f[0].message.contains("is a list") && f[0].message.contains("no place for its health bar") && f[0].severity == Severity::Error, "{f:?}");
    // the tagged sounds gone: footsteps silent, a hidden cue (fair play: `rubric::is_fair_play` reads "tagged sounds")
    let silent = GAMEDATA.replace(r#"TaggedSoundSettings_t = { m_taggedSounds = [ resource:"scripts/tagged_sounds/haze_anim_sounds.vdata" ] }"#, "");
    let f = hero_findings(hero(&silent, &[HERO_SKEL], HERO_GRAPHS, 90.0, 90.0), None);
    assert!(f.len() == 1 && f[0].message.starts_with("tagged sounds: ") && f[0].message.contains("footsteps") && f[0].message.contains("haze_anim_sounds.vdata") && f[0].severity == Severity::Error, "{f:?}");
    // an older build's unit status: no attachment, the bar 95 units over the origin. It plays, placed otherwise: a warning
    let old = GAMEDATA.replace("m_vUnitStatusOffset = [ 10.0, 0.0, 0.0 ]", "m_vUnitStatusOffset = [ 0.0, 0.0, 95.0 ]").replace(r#"m_strUnitStatusAttachmentName = "healthbar""#, "");
    let f = hero_findings(hero(&old, &[HERO_SKEL], HERO_GRAPHS, 90.0, 90.0), None);
    assert!(f.len() == 1 && f.iter().all(|x| x.severity == Severity::Warn && x.message.ends_with(gamedata::STILL_PLAYS)), "{f:?}");
    assert!(f.iter().any(|x| x.message.contains("does not follow the unit when it crouches")) && f.iter().any(|x| x.message.contains("m_vUnitStatusOffset [0, 0, 95] (Valve's [10, 0, 0])")), "{f:?}");
    // the bar on an attachment the model does not have: at the origin, an error
    let gone = GAMEDATA.replace(r#""healthbar""#, r#""overhead""#);
    let f = hero_findings(hero(&gone, &[HERO_SKEL], HERO_GRAPHS, 90.0, 90.0), None);
    assert!(f.iter().any(|x| x.message.contains("'overhead', which the model does not have") && x.severity == Severity::Error), "{f:?}");
    // a camera further back: fair play (`rubric::is_fair_play` reads "camera settings")
    let far = GAMEDATA.replace("m_flCameraBackOffset = 103.0", "m_flCameraBackOffset = 125.0");
    let f = hero_findings(hero(&far, &[HERO_SKEL], HERO_GRAPHS, 90.0, 90.0), None);
    assert!(f.len() == 1 && f[0].message.starts_with("camera settings: m_flCameraBackOffset 125 (Valve's 103)"), "{f:?}");
    // another block Valve's model has and ours lacks: a warning that names what falls back
    let f = hero_findings(hero(&GAMEDATA.replace("BoneConstraintList = [ ]", ""), &[HERO_SKEL], HERO_GRAPHS, 90.0, 90.0), None);
    assert!(f.len() == 1 && f[0].severity == Severity::Warn && f[0].message.contains("helper bones"), "{f:?}");
    // no game data at all: every block of Valve's is said
    let f = hero_findings(hero("", &[HERO_SKEL], HERO_GRAPHS, 90.0, 90.0), None);
    assert_eq!(f.len(), 5, "{f:?}");

    // the A-pose: no animation skeleton, or the match graph gone
    let f = hero_findings(hero(GAMEDATA, &[], HERO_GRAPHS, 90.0, 90.0), None);
    assert!(f.len() == 1 && f[0].message.contains("A-pose") && f[0].severity == Severity::Error, "{f:?}");
    let f = hero_findings(hero(GAMEDATA, &[HERO_SKEL], &HERO_GRAPHS[1..], 90.0, 90.0), None);
    assert!(f.len() == 1 && f[0].message.contains("does not animate in a match"), "{f:?}");
    let f = hero_findings(hero(GAMEDATA, &[HERO_SKEL], &HERO_GRAPHS[..1], 90.0, 90.0), None);
    assert!(f.len() == 1 && f[0].message.contains("hero page"), "{f:?}");
}

#[test]
fn m062_holds_the_camera_attachments_the_game_names_to_valve_s_numbers() {
    // A Source 2 Viewer older than 20.0 wrote a pivot of -90 degrees as -89.972 (1.7e-4 in the rotation), which the
    // printed contract's 0.01 passes: the camera pose is off, an error
    let f = hero_findings(hero(GAMEDATA, &[HERO_SKEL], HERO_GRAPHS, 89.972, 90.0), None);
    assert!(f.len() == 1 && f[0].message.starts_with("attachment 'near_00' (a camera pose") && f[0].message.contains("turned 0.028 degrees") && f[0].severity == Severity::Error, "{f:?}");
    // what a round trip of Valve's numbers leaves (6.1e-5 on the community corpus) is not a change
    assert!(hero_findings(hero(GAMEDATA, &[HERO_SKEL], HERO_GRAPHS, 90.007, 90.0), None).is_empty());
    // a pivot in the hero's frame the client code names is held as tightly; one the code does not name keeps the
    // contract's own step
    let f = hero_findings(hero(GAMEDATA, &[HERO_SKEL], HERO_GRAPHS, 90.0, 89.972), Some(&["standing_pivot"]));
    assert!(f.len() == 1 && f[0].message.contains("'standing_pivot' (held in the hero's own frame and named by the game's client code)"), "{f:?}");
    assert!(hero_findings(hero(GAMEDATA, &[HERO_SKEL], HERO_GRAPHS, 90.0, 89.972), None).is_empty());
    // turned further, the contract itself says it changed, once
    let f = hero_findings(hero(GAMEDATA, &[HERO_SKEL], HERO_GRAPHS, 80.0, 90.0), Some(&["standing_pivot"]));
    assert!(f.len() == 1 && f[0].message.starts_with("attachment 'near_00' changed"), "{f:?}");
}

/// A compiled Panorama layout whose panels carry `ids`, as the layout compiler writes its tree (a LaCo block).
fn layout(ids: &[&str]) -> Vec<u8> {
    let panels: Vec<String> = ids
        .iter()
        .map(|id| format!(r#"{{ eType = "PANEL" name = "Panel" child = {{ eType = "PANEL_ATTRIBUTE" name = "id" child = {{ eType = "PANEL_ATTRIBUTE_VALUE" name = "{id}" }} }} }}"#))
        .collect();
    let tree = kv(&format!(r#"{{ m_AST = {{ m_pRoot = {{ eType = "ROOT" vecChildren = [ {{ eType = "PANEL" name = "Panel" vecChildren = [ {} ] }} ] }} }} }}"#, panels.join(", ")));
    resource::synth_with(&[], &[(*b"LaCo", resource::kv3::encode(&tree))])
}

#[test]
fn m042_a_replaced_layout_lacks_an_id_valve_s_current_one_has() {
    // Real shape (a HUD mod made before a patch, 6877): the top bar's copy lacks GameTime, which the client code asks
    // for by name, and ScoreCounter, which it does not name
    const HUD: &str = "panorama/layout/citadel_hud_top_bar.vxml_c";
    let mut g = game();
    with(&mut g, &[HUD]);
    let mut fake = Fake::default();
    fake.files.insert(HUD.into(), layout(&["TopBar", "GameTime", "ScoreCounter"]));
    fake.code = Some(["GameTime".to_string(), "TopBar".to_string()].into());
    let f = run_game(vec![file(HUD, layout(&["TopBar", "MyClock"]))], &g, &fake);
    let m042: Vec<&Finding> = f.iter().filter(|x| x.code == "M042").collect();
    assert_eq!(m042.len(), 2, "{f:?}");
    let err = m042.iter().find(|x| x.severity == Severity::Error).expect("the id the code names");
    assert!(err.message.contains("(GameTime)") && err.message.contains("FATAL ERROR: Unable to find child") && !err.message.contains("ScoreCounter"), "{}", err.message);
    let warn = m042.iter().find(|x| x.severity == Severity::Warn).expect("the id it does not");
    assert!(warn.message.contains("(ScoreCounter)") && warn.message.contains("does not name them"), "{}", warn.message);
    // every id of Valve's kept, ids of the mod's own added: nothing
    assert!(run_game(vec![file(HUD, layout(&["TopBar", "GameTime", "ScoreCounter", "MyClock"]))], &g, &fake).iter().all(|x| x.code != "M042"));
    // Valve's layout against itself: nothing
    assert!(run_game(vec![file(HUD, layout(&["TopBar", "GameTime", "ScoreCounter"]))], &g, &fake).iter().all(|x| x.code != "M042"));
    // with no client library to read, nothing is an error, and the warning says why
    fake.code = None;
    let f = run_game(vec![file(HUD, layout(&["TopBar"]))], &g, &fake);
    assert!(f.iter().filter(|x| x.code == "M042").all(|x| x.severity == Severity::Warn && x.message.contains("could not be read")), "{f:?}");
    // an older compile keeps the XML text in DATA: its ids are read too
    let old = resource::synth_with(&[], &[(*b"DATA", br#"<root><Panel id="TopBar"><Label id="GameTime" /></Panel></root>"#.to_vec())]);
    assert_eq!(layout::ids(&old), Some(BTreeSet::from(["TopBar".to_string(), "GameTime".to_string()])));
}

/// A compiled layout whose root holds one panel of each type, ids `p0`, `p1`, ...
fn typed_layout(types: &[&str]) -> Vec<u8> {
    let panels: Vec<String> = types
        .iter()
        .enumerate()
        .map(|(i, ty)| format!(r#"{{ eType = "PANEL" name = "{ty}" child = {{ eType = "PANEL_ATTRIBUTE" name = "id" child = {{ eType = "PANEL_ATTRIBUTE_VALUE" name = "p{i}" }} }} }}"#))
        .collect();
    let tree = kv(&format!(r#"{{ m_AST = {{ m_pRoot = {{ eType = "ROOT" vecChildren = [ {} ] }} }} }}"#, panels.join(", ")));
    resource::synth_with(&[], &[(*b"LaCo", resource::kv3::encode(&tree))])
}

#[test]
fn m042_a_replaced_layout_uses_a_panel_type_the_game_no_longer_has() {
    // Seen on 6880: a HUD mod's hud.xml from an older build uses CitadelHudChat and seven more types the client no
    // longer registers; the game logs 'Unrecognized panel type' and stops at launch on the required layout
    const HUD: &str = "panorama/layout/hud.vxml_c";
    let mut g = game();
    with(&mut g, &[HUD]);
    let mut fake = Fake::default();
    fake.files.insert(HUD.into(), typed_layout(&["Panel", "CitadelHudChatV2"]));
    fake.code = Some(["Panel".to_string(), "Label".to_string(), "CitadelHudChatV2".to_string()].into());
    let f = run_game(vec![file(HUD, typed_layout(&["Panel", "CitadelHudChat", "Label"]))], &g, &fake);
    let e: Vec<&Finding> = f.iter().filter(|x| x.code == "M042" && x.message.contains("panel type")).collect();
    assert!(e.len() == 1 && e[0].severity == Severity::Error, "{f:?}");
    assert!(e[0].message.starts_with("uses 1 panel type(s) the game no longer registers (CitadelHudChat)") && e[0].message.contains("FATAL ERROR"), "{}", e[0].message);
    // a type Valve's layout uses, or one the client names, is no finding of this kind; Valve's layout against itself is silent
    assert!(run_game(vec![file(HUD, typed_layout(&["Panel", "CitadelHudChatV2", "Label"]))], &g, &fake).iter().all(|x| !x.message.contains("panel type")));
    assert!(run_game(vec![file(HUD, typed_layout(&["Panel", "CitadelHudChatV2"]))], &g, &fake).iter().all(|x| x.code != "M042"));
    // with no client library to read, the types are not judged
    fake.code = None;
    assert!(run_game(vec![file(HUD, typed_layout(&["Panel", "CitadelHudChat"]))], &g, &fake).iter().all(|x| !x.message.contains("panel type")));
}

/// A compiled sound with the CTRL block the sound compiler writes: rate, channels, length.
fn vsnd(rate: u32, channels: u32, seconds: f64) -> Vec<u8> {
    resource::synth_with(&[], &[(*b"CTRL", resource::kv3::encode(&kv(&format!("{{ _class = \"CVoiceContainerDefault\" m_vSound = {{ m_nRate = {rate} m_nFormat = \"MP3\" m_nChannels = {channels} m_flDuration = {seconds} }} }}"))))])
}

#[test]
fn m065_a_replaced_sound_encoded_unlike_valve_s() {
    // Real shape (a voice pack, 6877): Valve's line mono at 44,100 Hz, the mod's stereo at 48,000; the event that plays
    // it raises the pitch
    const SND: &str = "sounds/vo/haze/haze_select_01.vsnd_c";
    const EV: &str = "soundevents/vo/haze.vsndevts_c";
    let mut g = game();
    with(&mut g, &[SND, EV]);
    let mut fake = Fake::default();
    fake.files.insert(SND.into(), vsnd(44100, 1, 2.5));
    fake.files.insert(EV.into(), res_kv(&[], &[(b"DATA", kv(r#"{ Haze.Select = { pitch = 1.1 volume = 0.8 vsnd_files = [ "sounds/vo/haze/haze_select_01.vsnd" ] } }"#))]));
    let f = run_game(vec![file(SND, vsnd(48000, 2, 2.5))], &g, &fake);
    let m: Vec<&Finding> = f.iter().filter(|x| x.code == "M065").collect();
    assert!(m.len() == 1 && m[0].severity == Severity::Info && m[0].message.contains("changes nothing a player hears"), "{f:?}");
    assert!(m[0].message.starts_with("48000 Hz, Valve's 44100 Hz, stereo, Valve's mono; 2.50 s long, Valve's 2.50 s; the game plays it from Haze.Select at pitch 1.1, volume 0.8"), "{}", m[0].message);
    // Valve's format, another take: nothing (its length is M063's and M064's)
    assert!(run_game(vec![file(SND, vsnd(44100, 1, 2.5))], &g, &fake).iter().all(|x| x.code != "M065"));
}

/// A model with only a PHYS block: a ragdoll of `parts` and, when `nodes` is not 0, a cloth of that many nodes.
fn physics(parts: &[&str], nodes: usize) -> Vec<u8> {
    let names: Vec<String> = parts.iter().map(|p| format!("\"{p}\"")).collect();
    let cloth = if nodes > 0 { format!("m_pFeModel = {{ m_CtrlName = [ {} ] m_nNodeCount = {nodes} m_nStaticNodes = 0 }}", (0..nodes).map(|i| format!("\"cloth_{i}\"")).collect::<Vec<_>>().join(", ")) } else { String::new() };
    res_kv(&[], &[(b"DATA", kv(r#"{ m_name = "x" }"#)), (b"PHYS", kv(&format!("{{ m_boneNames = [ {} ] {cloth} }}", names.join(", "))))])
}

#[test]
fn m160_valve_s_cloth_or_ragdoll_dropped() {
    // Real shapes (community skins, 6877): a decompiled hero with no cloth where Valve's has 252 nodes; a ragdoll one
    // part short
    let mut g = game();
    with(&mut g, &[MODEL]);
    let mut fake = Fake::default();
    fake.files.insert(MODEL.into(), physics(&["pelvis", "spine_1", "head"], 3));
    let f = run_game(vec![file(MODEL, physics(&["pelvis", "head"], 0))], &g, &fake);
    let m: Vec<&Finding> = f.iter().filter(|x| x.code == "M160").collect();
    assert_eq!(m.len(), 2, "{f:?}");
    assert!(m.iter().any(|x| x.message.contains("simulates cloth (3 nodes) and ours has none")), "{m:?}");
    assert!(m.iter().any(|x| x.message.contains("physics of 3 parts (its ragdoll, or a prop's body) and ours lacks 1 of them (spine_1)")), "{m:?}");
    // what the lost part does was seen on 6880: it keeps its last pose (a gun left hanging in the air)
    assert!(m.iter().any(|x| x.message.contains("keeps its last pose (seen on 6880") && !x.message.contains("UNVERIFIED")), "{m:?}");
    // Valve's own physics, and a skin's own cloth of another size: nothing
    assert!(run_game(vec![file(MODEL, physics(&["pelvis", "spine_1", "head"], 3))], &g, &fake).iter().all(|x| x.code != "M160"));
    assert!(run_game(vec![file(MODEL, physics(&["pelvis", "spine_1", "head", "tail"], 40))], &g, &fake).iter().all(|x| x.code != "M160"));
}

const GOBO: &str = "materials/maid_lash/fx/lace_gobo.vtex";
const LIGHT: &str = "particles/abilities/lash/final_strike_impact_light.vpcf_c";

fn light(lumens: f64, color: [u8; 3], cookie: Option<&str>, extra_light: bool) -> Value {
    let c = cookie.map(|c| format!("m_hLightCookie = \"{c}\"")).unwrap_or_default();
    let two = if extra_light { r#"{ _class = "C_OP_RenderOmni2Light" m_flBrightnessLumens = 500 }"# } else { "" };
    kv(&format!(
        r#"{{ _class = "CParticleSystemDefinition" m_nMaxParticles = 1
            m_Renderers = [ {{ _class = "C_OP_RenderOmni2Light"
                m_vColorBlend = {{ m_nType = "PVEC_TYPE_LITERAL_COLOR" m_LiteralColor = [ {}, {}, {} ] }}
                m_flBrightnessLumens = {{ m_nType = "PF_TYPE_LITERAL" m_flLiteralValue = {lumens} }} {c} }}, {two} ] }}"#,
        color[0], color[1], color[2]
    ))
}

#[test]
fn m072_light_cookie() {
    // Real cases, both from the game log on Maid Lash's lace_gobo.vtex: first DXT1 ("wrong format DXT1 - expected
    // BGRA8888!"), then BGRA8888 at 256x256 ("has no mip levels of size 512x512").
    let mut g = game();
    with(&mut g, &[LIGHT]);
    let mut fake = Fake::default();
    fake.files.insert(LIGHT.into(), res_kv(&[], &[(b"DATA", light(2500.0, [27, 137, 255], None, false))]));
    let ours = |size: u16, mips: u8, format: u8| {
        let mut header = resource::texture::synth_header(size, size, format);
        header[27] = mips;
        vec![
            file(LIGHT, res_kv(&[GOBO], &[(b"DATA", light(2500.0, [27, 137, 255], Some(GOBO), false))])),
            file(&format!("{GOBO}_c"), resource::synth_with(&[], &[(*b"DATA", header)])),
        ]
    };
    use resource::texture::{BGRA8888, DXT1};
    let f = run_game(ours(512, 1, DXT1), &g, &fake);
    assert_eq!(codes(&f), BTreeSet::from(["M072"]), "{f:?}");
    assert_eq!(f.len(), 1, "512x512 is right; only the format is wrong: {f:?}");
    assert_eq!(f[0].path.as_deref(), Some("materials/maid_lash/fx/lace_gobo.vtex_c"));
    assert!(f[0].message.contains("m_hLightCookie") && f[0].message.contains("wrong format DXT1 - expected BGRA8888"));
    let small = run_game(ours(256, 7, BGRA8888), &g, &fake);
    assert_eq!(small.len(), 1, "{small:?}");
    assert!(small[0].message.contains("has no mip levels of size 512x512"), "{small:?}");
    assert_eq!(run_game(ours(256, 7, DXT1), &g, &fake).len(), 2, "both problems are reported");
    assert!(run_game(ours(512, 1, BGRA8888), &g, &fake).is_empty(), "512x512 BGRA8888, like Valve's cookies");
    assert!(run_game(ours(1024, 2, BGRA8888), &g, &fake).is_empty(), "a 1024 texture whose second mip is 512 qualifies");
    assert_eq!(run_game(ours(1024, 1, BGRA8888), &g, &fake).len(), 1, "but not without that mip");
    assert_eq!(run_game(ours(512, 1, resource::texture::RGBA8888), &g, &fake).len(), 1, "RGBA8888 is not BGRA8888");
}

#[test]
fn m073_texture_not_pow2() {
    // Real case: a 74x74 roughness bake (the compiler refused it); Valve ships none outside panorama/.
    let g = game();
    let t = "models/heroes_wip/lash/materials/maid_apron_rough.vtex_c";
    let files = vec![file(MAT, res(&["models/heroes_wip/lash/materials/maid_apron_rough.vtex"])), file(t, tex(74, 74, 1))];
    assert_eq!(codes(&run(files, Some(&g), None)), BTreeSet::from(["M073"]));
    // Panorama art comes in any size.
    let ui = "panorama/images/heroes/lash_card_psd.vtex_c";
    let files = vec![file(MAT, res(&["panorama/images/heroes/lash_card_psd.vtex"])), file(ui, tex(280, 380, 28))];
    assert!(run(files, Some(&g), None).is_empty());
}

fn sprite(overbright: f64, radius: f64) -> Value {
    kv(&format!(
        r#"{{ _class = "CParticleSystemDefinition" m_ConstantColor = [ 255, 255, 255, 255 ]
            m_Initializers = [ {{ _class = "C_INIT_RandomColor" m_ColorMin = [ 210, 160, 120 ] m_ColorMax = [ 255, 200, 160 ] }},
                               {{ _class = "C_INIT_RandomRadius" m_flRadiusMin = 2 m_flRadiusMax = {radius} }} ]
            m_Renderers = [ {{ _class = "C_OP_RenderSprites" m_flOverbrightFactor = {overbright} }} ] }}"#
    ))
}

#[test]
fn m081_particle_energy() {
    // Real case: the muzzle powder puff at 8.9x the flame's light; the whip sparks Valve drew at overbright 15.
    let p = "particles/weapon_fx/lash/lash_muzzle_flash_fiery.vpcf_c";
    let mut g = game();
    with(&mut g, &[p]);
    let mut fake = Fake::default();
    fake.files.insert(p.into(), res_kv(&[], &[(b"DATA", sprite(2.0, 12.0))]));
    let bright = run_game(vec![file(p, res_kv(&[], &[(b"DATA", sprite(9.0, 12.0))]))], &g, &fake);
    assert_eq!(codes(&bright), BTreeSet::from(["M081"]), "{bright:?}");
    let big = run_game(vec![file(p, res_kv(&[], &[(b"DATA", sprite(2.0, 20.0))]))], &g, &fake);
    assert!(big[0].message.contains("radius"), "{big:?}");
    let dimmer = run_game(vec![file(p, res_kv(&[], &[(b"DATA", sprite(1.5, 12.0))]))], &g, &fake);
    assert!(dimmer.is_empty(), "dimmer is always fine");
}

#[test]
fn m082_particle_light_caps() {
    // Real case: Death Slam's gobo kept Valve's 2500 lm but went from (27,137,255) to (255,214,200): 2.7x.
    let mut g = game();
    with(&mut g, &[LIGHT]);
    let mut fake = Fake::default();
    fake.files.insert(LIGHT.into(), res_kv(&[], &[(b"DATA", light(2500.0, [27, 137, 255], None, false))]));
    let warm = run_game(vec![file(LIGHT, res_kv(&[], &[(b"DATA", light(2500.0, [255, 214, 200], None, false))]))], &g, &fake);
    assert_eq!(codes(&warm), BTreeSet::from(["M082"]), "{warm:?}");
    assert!(warm[0].message.contains("luminous output"));
    // The fix that shipped: the colour keeps Valve's lightness.
    let kept = run_game(vec![file(LIGHT, res_kv(&[], &[(b"DATA", light(2500.0, [171, 124, 135], None, false))]))], &g, &fake);
    assert!(kept.is_empty(), "{kept:?}");
    let added = run_game(vec![file(LIGHT, res_kv(&[], &[(b"DATA", light(2500.0, [27, 137, 255], None, true))]))], &g, &fake);
    assert!(added.iter().any(|x| x.code == "M082" && x.message.contains("2 light renderer(s)")));
    // A light smuggled in through a new child system counts against the parent's tree.
    let child = "particles/maid_lash/death_slam_extra_light.vpcf";
    let parent = kv(&format!(
        r#"{{ _class = "CParticleSystemDefinition" m_Children = [ {{ m_ChildRef = resource:"{child}" }} ]
            m_Renderers = [ {{ _class = "C_OP_RenderOmni2Light" m_vColorBlend = {{ m_LiteralColor = [ 27, 137, 255 ] }}
                m_flBrightnessLumens = 2500 }} ] }}"#
    ));
    let files = vec![
        file(LIGHT, res_kv(&[child], &[(b"DATA", parent)])),
        file(&format!("{child}_c"), res_kv(&[], &[(b"DATA", light(800.0, [255, 255, 255], None, false))])),
    ];
    let f = run_game(files, &g, &fake);
    assert!(f.iter().any(|x| x.code == "M082" && x.message.contains("children")), "{f:?}");
}

#[test]
fn m090_shared_file() {
    // Real case: Lash's Ground Strike icon is also the Majestic Leap item's.
    let icon = "panorama/images/hud/abilities/lash/lash_death_slam_psd.vtex_c";
    let mut g = game();
    with(&mut g, &[icon]);
    let mut fake = Fake::default();
    fake.users.insert(icon.into(), vec!["hero_lash".into(), "upgrade_rocket_booster".into()]);
    let files = vec![file(icon, tex(128, 128, 28))];
    let f = run_game(files.clone(), &g, &fake);
    assert_eq!(codes(&f), BTreeSet::from(["M090"]), "{f:?}");
    assert!(f[0].message.contains("upgrade_rocket_booster"));
    fake.users.insert(icon.into(), vec!["hero_lash".into()]);
    assert!(run_game(files, &g, &fake).is_empty(), "one owner: the hero's own file");
}

#[test]
fn m092_a_hero_mod_replaces_another_hero_s_file() {
    // Real case (the community's error pages): a template's sample image left in a skin's addon changes another
    // hero's icon. Here a Lash skin carries Haze's card.
    let card = "panorama/images/heroes/haze_card_psd.vtex_c";
    let mut g = game();
    with(&mut g, &[card]);
    let mut fake = Fake::default();
    fake.users.insert(MODEL.into(), vec!["hero_lash".into()]);
    fake.users.insert(card.into(), vec!["hero_haze".into()]);
    fake.users.insert(TEX.into(), vec!["hero_lash".into()]);
    let skin = |extra: &str| vec![file(MODEL, res(&[])), file(TEX, tex(128, 128, 28)), file(extra, tex(128, 128, 28))];
    let f = run_game(skin(card), &g, &fake);
    let m: Vec<&Finding> = f.iter().filter(|x| x.code == "M092").collect();
    assert!(m.len() == 1 && m[0].path.as_deref() == Some(card) && m[0].message.contains("used only by hero_haze") && m[0].message.contains("model of hero_lash"), "{f:?}");
    // the hero's own files, and a mod that replaces no hero's model (a UI pack of every hero's cards), say nothing
    assert!(run_game(skin(TEX), &g, &fake).iter().all(|x| x.code != "M092"));
    assert!(run_game(vec![file(card, tex(128, 128, 28)), file(TEX, tex(128, 128, 28))], &g, &fake).iter().all(|x| x.code != "M092"));
}

#[test]
fn m084_a_replaced_particle_nothing_plays() {
    // Real shape (6877): Valve ships 3,758 particles nothing names, and skins recolour some of them
    let old = "particles/abilities/lash/lash_ultimate_locked_aura.vpcf_c";
    let mut g = game();
    with(&mut g, &[old]);
    let mut fake = Fake { files: valve_pcf(old), ..Default::default() };
    fake.unreached = Some([old.to_string()].into());
    let f = run_game(vec![file(old, pcf(&[]))], &g, &fake);
    assert!(f.iter().any(|x| x.code == "M084" && x.message.contains("never plays it") && x.severity == Severity::Warn), "{f:?}");
    // the mod's own particle names it: it plays
    let parent = "particles/abilities/lash/lash_flog.vpcf_c";
    with(&mut g, &[parent]);
    fake.files.extend(valve_pcf(parent));
    let f = run_game(vec![file(old, pcf(&[])), file(parent, pcf(&["particles/abilities/lash/lash_ultimate_locked_aura.vpcf"]))], &g, &fake);
    assert!(f.iter().all(|x| x.code != "M084"), "{f:?}");
    // a particle the game names, and a build with no list: nothing
    fake.unreached = Some(std::collections::HashSet::new());
    assert!(run_game(vec![file(old, pcf(&[]))], &g, &fake).iter().all(|x| x.code != "M084"));
    fake.unreached = None;
    assert!(run_game(vec![file(old, pcf(&[]))], &g, &fake).iter().all(|x| x.code != "M084"));
}

#[test]
fn m091_shared_file_unchecked() {
    // Without a users index the shared-file check cannot run: that is said as a finding, not passed in silence.
    let icon = "panorama/images/hud/abilities/lash/lash_death_slam_psd.vtex_c";
    let mut g = game();
    with(&mut g, &[icon]);
    let fake = Fake { no_index: Some("there is no users index for this build".into()), ..Default::default() };
    let f = run_game(vec![file(icon, tex(128, 128, 28))], &g, &fake);
    assert_eq!(codes(&f), BTreeSet::from(["M091"]), "{f:?}");
    assert!(f[0].message.contains("the 1 file(s) this mod replaces were not checked") && f[0].message.contains("no users index"), "{}", f[0].message);
    assert!(f[0].path.is_none() && f[0].severity == Severity::Info);
    // a mod that replaces nothing of Valve's has nothing to check
    let new = "panorama/images/hud/abilities/lash/lash_new_icon_psd.vtex_c";
    assert!(run_game(vec![file(new, tex(128, 128, 28))], &g, &fake).iter().all(|x| x.code != "M091"));
}

// ------------------------------------------------------------------------------------------ M141 cloth-flutter

/// A PHYS block holding a cloth FeModel: one static node (pelvis), then a chain of simulated nodes `(name, F, V, radius)`
/// 5 units apart (rods), gravity 360, and optionally one capsule on `capsule_mask` layers.
pub(crate) fn fe_phys(nodes: &[(&str, f64, f64, f64)], capsule_mask: Option<u32>) -> Value {
    let names: Vec<String> = std::iter::once("\"pelvis\"".to_string()).chain(nodes.iter().map(|n| format!("\"{}\"", n.0))).collect();
    let integ: Vec<String> = std::iter::once("{ flPointDamping = 0.0 flAnimationForceAttraction = 0.0 flAnimationVertexAttraction = 0.0 flGravity = 0.0 }".to_string())
        .chain(nodes.iter().map(|n| format!("{{ flPointDamping = 0.0 flAnimationForceAttraction = {} flAnimationVertexAttraction = {} flGravity = 360.0 }}", n.1, n.2)))
        .collect();
    let radii: Vec<String> = nodes.iter().map(|n| format!("{}", n.3)).collect();
    let rods: Vec<String> = (1..=nodes.len())
        .map(|i| format!("{{ nNode = [ {}, {} ] flMaxDist = 5.0 flMinDist = 3.75 flWeight0 = 0.5 flRelaxationFactor = 1.0 }}", i - 1, i))
        .collect();
    let masks: Vec<String> = (0..=nodes.len()).map(|_| "65535".to_string()).collect();
    let caps = capsule_mask.map_or(String::new(), |m| {
        format!("{{ vSphere = [ [ 0, 0, 0, 5.0 ], [ 0, 0, 20, 5.0 ] ] nNode = 0 nCollisionMask = {m} nVertexMapIndex = 65535 nFlags = 0 }}")
    });
    kv(&format!(
        "{{ m_parts = [ ] m_pFeModel = {{ m_CtrlName = [ {} ] m_nNodeCount = {} m_nStaticNodes = 1 m_nFirstPositionDrivenNode = {} \
         m_NodeIntegrator = [ {} ] m_NodeCollisionRadii = [ {} ] m_TreeCollisionMasks = [ {} ] m_Rods = [ {} ] \
         m_TaperedCapsuleRigids = [ {caps} ] m_SphereRigids = [ ] }} }}",
        names.join(", "),
        nodes.len() + 1,
        nodes.len() + 1,
        integ.join(", "),
        radii.join(", "),
        masks.join(", "),
        rods.join(", ")
    ))
}

fn cloth_model(nodes: &[(&str, f64, f64, f64)], capsule_mask: Option<u32>) -> Vec<u8> {
    let data = kv(r#"{ m_name = "x" m_modelSkeleton = { m_boneName = [ "root_motion", "pelvis" ] m_nParent = [ -1, 0 ] } }"#);
    res_kv(&[], &[(b"DATA", data), (b"PHYS", fe_phys(nodes, capsule_mask))])
}

const TAIL: [&str; 4] = ["coattail_1", "coattail_2", "coattail_3", "coattail_4"];
/// Valve's Sinclair-like proxy numbers: goal 0.215 (F 0.00994), V from 0.00994 at the seam rising down the tail, no radius.
fn crunchy(names: &[&'static str], capsule: Option<u32>) -> Vec<u8> {
    let n: Vec<(&str, f64, f64, f64)> = names.iter().enumerate().map(|(i, n)| (*n, 0.00994, 0.00994 + 0.005 * i as f64, 0.0)).collect();
    cloth_model(&n, capsule)
}
/// The same chain as a lined wool: same goal, velocity matching 0.11, radius 1, a capsule on its layer.
fn heavy(names: &[&'static str]) -> Vec<u8> {
    let n: Vec<(&str, f64, f64, f64)> = names.iter().map(|n| (*n, 0.0106, 0.11, 1.0)).collect();
    cloth_model(&n, Some(2))
}

#[test]
fn m141_crunchy_cloth_where_valve_was_heavy() {
    let mut g = game();
    let mut fake = Fake::default();
    fake.files.insert(MODEL.into(), heavy(&TAIL));
    with(&mut g, &[MODEL]);
    let f = run_game(vec![file(MODEL, crunchy(&TAIL, None))], &g, &fake);
    let m = f.iter().find(|x| x.code == "M141").unwrap_or_else(|| panic!("{f:#?}"));
    assert_eq!(m.severity, Severity::Warn);
    assert!(m.message.contains("coattail* (4 nodes)") && m.message.contains("paper-thin flutter"), "{}", m.message);
    assert!(m.message.contains("passes through the body"), "no radius and no shape: {}", m.message);
    assert!(m.message.contains("Valve's cloth here rings"), "{}", m.message);
    assert!(m.message.contains("needs vertex attraction"), "{}", m.message);
    // the heavy version against the same Valve file is clean
    let ok = run_game(vec![file(MODEL, heavy(&TAIL))], &g, &fake);
    assert!(!ok.iter().any(|x| x.code == "M141"), "{ok:#?}");
}

#[test]
fn m141_valves_own_numbers_are_not_blamed() {
    // Valve's own cloth rings like this (Sinclair's coat tails, the detective's tie): the same numbers at the same path
    // are Valve's design, with no bigger mesh on them
    let mut g = game();
    let mut fake = Fake::default();
    fake.files.insert(MODEL.into(), crunchy(&TAIL, Some(15)));
    with(&mut g, &[MODEL]);
    let f = run_game(vec![file(MODEL, crunchy(&TAIL, Some(15)))], &g, &fake);
    assert!(!f.iter().any(|x| x.code == "M141"), "{f:#?}");
}

#[test]
fn m141_new_cloth_judged_on_its_own() {
    // cloth Valve's model does not have: a goal-0.1 streamer with no damping beyond the goal's own
    let mut g = game();
    let mut fake = Fake::default();
    fake.files.insert(MODEL.into(), heavy(&TAIL));
    with(&mut g, &[MODEL]);
    let n = [("streamer_1", 0.001, 0.001, 0.0), ("streamer_2", 0.001, 0.001, 0.0), ("streamer_3", 0.001, 0.001, 0.0)];
    let f = run_game(vec![file(MODEL, cloth_model(&n, Some(15)))], &g, &fake);
    let m = f.iter().find(|x| x.code == "M141").unwrap_or_else(|| panic!("{f:#?}"));
    assert!(m.message.contains("streamer* (3 nodes)") && m.message.contains("no collision radius"), "{}", m.message);
    assert!(!m.message.contains("Valve's cloth here"), "{}", m.message);
    // a slow, damped new chain is fine
    let ok = [("streamer_1", 0.0106, 0.11, 0.5), ("streamer_2", 0.0106, 0.11, 0.5)];
    assert!(!run_game(vec![file(MODEL, cloth_model(&ok, Some(15)))], &g, &fake).iter().any(|x| x.code == "M141"));
}

#[test]
fn m141_cloth_with_no_damper_never_settles_however_slow() {
    // four goal-0 joints, 5 to 20 units of cloth below the pin: the median node sways under 1 Hz, so it is not
    // flutter, but with vertex attraction 0 nothing ever takes the swing out
    let mut g = game();
    let mut fake = Fake::default();
    fake.files.insert(MODEL.into(), heavy(&TAIL));
    with(&mut g, &[MODEL]);
    let n = [("train_1", 0.0, 0.0, 0.5), ("train_2", 0.0, 0.0, 0.5), ("train_3", 0.0, 0.0, 0.5), ("train_4", 0.0, 0.0, 0.5)];
    let f = run_game(vec![file(MODEL, cloth_model(&n, Some(15)))], &g, &fake);
    let m = f.iter().find(|x| x.code == "M141").unwrap_or_else(|| panic!("{f:#?}"));
    assert!(m.message.contains("train* (4 nodes)") && m.message.contains("never settles") && m.message.contains("sqrt(1 - V) = 1"), "{}", m.message);
    assert!(!m.message.contains("paper-thin"), "under 1 Hz it is not flutter: {}", m.message);
    // the smallest damper there is ends it: the same chain with a goal and its damping is a slow, settling sway
    let ok = [("train_1", 0.001, 0.05, 0.5), ("train_2", 0.001, 0.05, 0.5), ("train_3", 0.001, 0.05, 0.5), ("train_4", 0.001, 0.05, 0.5)];
    assert!(!run_game(vec![file(MODEL, cloth_model(&ok, Some(15)))], &g, &fake).iter().any(|x| x.code == "M141"));
    // and Valve's own undamped chain at the same path (Haze's hoses, Yamato's tassels) is Valve's design
    fake.files.insert(MODEL.into(), cloth_model(&n, Some(15)));
    assert!(!run_game(vec![file(MODEL, cloth_model(&n, Some(15)))], &g, &fake).iter().any(|x| x.code == "M141"));
}

// ------------------------------------------------------------------------------------ M142 distance-field-dropped

/// A model with a skeleton and (optionally) a DSTF block holding one field per bone name given.
fn dstf_model(fields_on: Option<&[&str]>) -> Vec<u8> {
    let data = kv(r#"{ m_name = "x" m_modelSkeleton = { m_boneName = [ "root_motion", "pelvis", "spine_1", "head" ] m_nParent = [ -1, 0, 1, 2 ] } }"#);
    let mut blocks = vec![(b"DATA", data)];
    if let Some(bones) = fields_on {
        let f: Vec<String> = bones
            .iter()
            .map(|b| format!("{{ m_nResX = 4 m_nResY = 4 m_nResZ = 4 m_nParentBoneNameHash = {} m_bUseForOcclusion = true }}", distfield::murmur2(b.as_bytes(), distfield::BONE_HASH_SEED)))
            .collect();
        blocks.push((b"DSTF", kv(&format!("{{ m_distanceFields = [ {} ] }}", f.join(" ")))));
    }
    res_kv(&[], &blocks)
}

#[test]
fn murmur2_matches_the_engines_bone_hash() {
    // Valve's Lash: the field on `spine_1` carries 110709982; `pelvis` 162587645 (the research pass's decode)
    assert_eq!(distfield::murmur2(b"spine_1", distfield::BONE_HASH_SEED), 110709982);
    assert_eq!(distfield::murmur2(b"pelvis", distfield::BONE_HASH_SEED), 162587645);
}

#[test]
fn m142_distance_fields_dropped() {
    let mut g = game();
    let mut fake = Fake::default();
    fake.files.insert(MODEL.into(), dstf_model(Some(&["pelvis", "spine_1", "head"])));
    with(&mut g, &[MODEL]);
    // ours has none
    let f = run_game(vec![file(MODEL, dstf_model(None))], &g, &fake);
    let m = f.iter().find(|x| x.code == "M142").unwrap_or_else(|| panic!("{f:#?}"));
    assert_eq!(m.severity, Severity::Warn);
    assert!(m.message.contains("3 distance field(s)") && m.message.contains("ours has none, on "), "{}", m.message);
    assert!(m.message.contains("pelvis, spine_1, head") || (m.message.contains("pelvis") && m.message.contains("spine_1") && m.message.contains("head")), "{}", m.message);
    assert!(m.message.contains("occlusion"), "{}", m.message);
    // ours covers only one bone: the other two are named
    let f = run_game(vec![file(MODEL, dstf_model(Some(&["pelvis"])))], &g, &fake);
    let m = f.iter().find(|x| x.code == "M142").unwrap_or_else(|| panic!("{f:#?}"));
    assert!(m.message.contains("ours has 1") && m.message.contains("spine_1") && !m.message.contains("(pelvis"), "{}", m.message);
    // the same coverage is clean, and so is a Valve model without fields
    let ok = run_game(vec![file(MODEL, dstf_model(Some(&["pelvis", "spine_1", "head"])))], &g, &fake);
    assert!(!ok.iter().any(|x| x.code == "M142"), "{ok:#?}");
    fake.files.insert(MODEL.into(), dstf_model(None));
    assert!(!run_game(vec![file(MODEL, dstf_model(None))], &g, &fake).iter().any(|x| x.code == "M142"));
    assert_eq!(rule("M142").severity, Severity::Warn);
}

// ------------------------------------------------------------------------------------------ M143 pbr-combo-invalid

fn pbr_mat(shader: &str, flags: &[&str]) -> Vec<u8> {
    let ints: Vec<String> = flags.iter().map(|f| format!("{{ m_name = \"{f}\" m_nValue = 1 }}")).collect();
    res_kv(&[], &[(b"DATA", kv(&format!("{{ m_shaderName = \"{shader}\" m_intParams = [ {} ] }}", ints.join(" "))))])
}

fn combo(shader: &str, flags: &[&str]) -> Vec<Finding> {
    let g = game();
    run(vec![file(MAT, pbr_mat(shader, flags))], Some(&g), None).into_iter().filter(|f| f.code == "M143").collect()
}

#[test]
fn m143_sheen_with_detail_compiles_but_drops_the_detail() {
    let f = combo("pbr.vfx", &["F_SHEEN", "F_DETAIL", "F_USE_NPR_LIGHTING", "F_USE_STATUS_EFFECTS_PROXY"]);
    assert_eq!(f.len(), 1, "{f:#?}");
    assert_eq!(f[0].severity, Severity::Error);
    assert_eq!(f[0].path.as_deref(), Some(MAT));
    assert!(f[0].message.contains("F_SHEEN together with F_DETAIL") && f[0].message.contains("g_tDetail"), "{}", f[0].message);
    // each of the other exclusions in the shader's Allow1 groups
    for other in ["F_GLASS", "F_CLOAK", "F_PAINT_VERTEX_COLORS", "F_TRANSLUCENT", "F_ADVANCED_TRANSLUCENCY", "F_UNLIT"] {
        let f = combo("pbr.vfx", &["F_SHEEN", other]);
        assert!(f.iter().any(|x| x.message.contains(&format!("F_SHEEN together with {other}"))), "{other}: {f:#?}");
    }
    for other in ["F_UNLIT", "F_PAINT_VERTEX_COLORS"] {
        assert!(combo("pbr.vfx", &["F_DETAIL", other]).iter().any(|x| x.message.contains(&format!("F_DETAIL together with {other}"))), "{other}");
    }
}

#[test]
fn m143_requirements() {
    // the status proxy needs NPR, unlit or self-illum, and excludes the cloak
    assert!(combo("pbr.vfx", &["F_USE_STATUS_EFFECTS_PROXY"]).iter().any(|x| x.message.contains("needs one of F_USE_NPR_LIGHTING, F_UNLIT, F_SELF_ILLUM")));
    assert!(combo("pbr.vfx", &["F_USE_STATUS_EFFECTS_PROXY", "F_SELF_ILLUM", "F_CLOAK"]).iter().any(|x| x.message.contains("F_USE_STATUS_EFFECTS_PROXY together with F_CLOAK")));
    // front-face normals need alpha test and backfaces
    let f = combo("pbr.vfx", &["F_USE_FRONT_FACE_NORMALS_FOR_BACK_FACES", "F_ALPHA_TEST"]);
    assert!(f.iter().any(|x| x.message.contains("needs F_RENDER_BACKFACES on")), "{f:#?}");
}

#[test]
fn m143_legal_combos_are_silent() {
    // Valve's common hero cases: sheen with NPR and the status proxy; lace (alpha test + backfaces + sheen); detail alone
    assert!(combo("pbr.vfx", &["F_SHEEN", "F_USE_NPR_LIGHTING", "F_USE_STATUS_EFFECTS_PROXY"]).is_empty());
    assert!(combo("pbr.vfx", &["F_ALPHA_TEST", "F_RENDER_BACKFACES", "F_SHEEN", "F_USE_NPR_LIGHTING", "F_USE_STATUS_EFFECTS_PROXY"]).is_empty());
    assert!(combo("pbr.vfx", &["F_DETAIL", "F_SELF_ILLUM", "F_USE_STATUS_EFFECTS_PROXY"]).is_empty());
    assert!(combo("pbr.vfx", &["F_USE_FRONT_FACE_NORMALS_FOR_BACK_FACES", "F_ALPHA_TEST", "F_RENDER_BACKFACES"]).is_empty());
    // another shader's flags of the same names are not judged by pbr's rules
    assert!(combo("particles.vfx", &["F_SHEEN", "F_DETAIL"]).is_empty());
    assert_eq!(rule("M143").severity, Severity::Error);
}

/// A 1x1 BGRA8888 texture of one colour (B, G, R, A), or a 2x1 one of two.
fn colour_tex(px: &[[u8; 4]]) -> Vec<u8> {
    let mut hdr = resource::texture::synth_header(px.len() as u16, 1, 28);
    hdr[27] = 1;
    let mut f = resource::synth_with(&[], &[(*b"DATA", hdr)]);
    for p in px {
        f.extend_from_slice(p);
    }
    f
}

/// A pbr material naming `slots` (name, texture path without `_c`).
fn slot_mat(slots: &[(&str, &str)]) -> Vec<u8> {
    let tex: Vec<String> = slots.iter().map(|(n, v)| format!("{{ m_name = \"{n}\" m_pValue = \"{v}\" }}")).collect();
    res_kv(&[], &[(b"DATA", kv(&format!("{{ m_shaderName = \"pbr.vfx\" m_textureParams = [ {} ] }}", tex.join(" "))))])
}

#[test]
fn m148_default_slot_literal() {
    // Real case (2026-11-24): a decompile printed the engine-default glass mask (white) as [0 0 0 0]; recompiled, the
    // glass material drew a summon solid with the hero inside it invisible
    const DEFAULT: &str = "materials/default/default_mask_tga_fe1b4b1.vtex";
    const OURS: &str = "models/heroes_wip/lash/materials/lash_body_vmat_g_tglass_3a2b1c0d.vtex";
    let valve_mat = slot_mat(&[("g_tColor", "models/heroes_wip/lash/materials/lash_body_color_png_f6ecf355.vtex"), ("g_tGlass", DEFAULT)]);
    let fake = Fake { files: HashMap::from([(MAT.to_string(), valve_mat.clone()), (format!("{DEFAULT}_c"), colour_tex(&[[255; 4]]))]), ..Default::default() };
    let g = game();
    let ours_mat = slot_mat(&[("g_tColor", "models/heroes_wip/lash/materials/lash_body_color_png_f6ecf355.vtex"), ("g_tGlass", OURS)]);
    let with_tex = |px: &[[u8; 4]]| run_game(vec![file(MAT, ours_mat.clone()), file(&format!("{OURS}_c"), colour_tex(px))], &g, &fake);

    let f = with_tex(&[[0, 0, 0, 0]]);
    let m = f.iter().find(|x| x.code == "M148").unwrap_or_else(|| panic!("{f:#?}"));
    assert_eq!(m.path.as_deref(), Some(MAT));
    assert!(m.message.contains("g_tGlass") && m.message.contains("[1.00 1.00 1.00 1.00]") && m.message.contains("[0.00 0.00 0.00 0.00]"), "{}", m.message);
    // the same value within compression noise, a real texture of the mod's own, and Valve's own file: silent
    assert!(!codes(&with_tex(&[[250, 250, 250, 252]])).contains("M148"));
    assert!(!codes(&with_tex(&[[0, 0, 0, 0], [255, 255, 255, 255]])).contains("M148"), "a varying texture is the mod's design");
    assert!(!codes(&run_game(vec![file(MAT, valve_mat.clone())], &g, &fake)).contains("M148"), "Valve's own material is never blamed");
    // a default it cannot read is unchecked, never clean
    let blind = Fake { files: HashMap::from([(MAT.to_string(), valve_mat)]), ..Default::default() };
    let f = run_game(vec![file(MAT, ours_mat.clone()), file(&format!("{OURS}_c"), colour_tex(&[[0; 4]]))], &g, &blind);
    assert!(f.iter().any(|x| x.code == "M144" && x.message.contains("(M148)")), "{f:#?}");
    assert!(!codes(&f).contains("M148"));
}

#[test]
fn m149_a_texture_slot_valves_material_has_and_ours_dropped() {
    // Seen in game (6868): a published mod's bow material had lost g_tAltColor_Reveal; the game logged "Texture
    // g_tAltColor_Reveal doesn't exist in ...archer_bow.vmat!" and "returning error texture" 50 times in one pass.
    let color = ("g_tColor", "models/heroes_wip/lash/materials/lash_body_color_png_f6ecf355.vtex");
    let valve_mat = slot_mat(&[color, ("g_tAltColor_Reveal", "models/heroes_wip/lash/materials/lash_body_reveal_png_1.vtex")]);
    let fake = Fake { files: HashMap::from([(MAT.to_string(), valve_mat.clone())]), ..Default::default() };
    let g = game();
    let f = run_game(vec![file(MAT, slot_mat(&[color]))], &g, &fake);
    let m = f.iter().find(|x| x.code == "M149").unwrap_or_else(|| panic!("{f:#?}"));
    assert!(m.message.contains("g_tAltColor_Reveal") && m.message.contains("error texture"), "{}", m.message);
    assert_eq!(m.severity, Severity::Warn);
    assert!(!codes(&run_game(vec![file(MAT, valve_mat)], &g, &fake)).contains("M149"), "Valve's own slots, all there");
}

#[test]
fn every_rule_is_seen_somewhere_a_report_names() {
    for r in RULES {
        assert!(SEEN.contains(&seen(r.code)), "{}", r.code);
    }
    assert_eq!((seen("M110"), seen("M061"), seen("M090"), seen("M123"), seen("M030"), seen("M031")), ("on the hero", "effects, sound", "other heroes", "frame cost", "loading", "the pak"));
}

#[test]
fn m031_counts_what_panorama_names_in_its_own_text() {
    // Real case (6871, a community HUD mod, 229 findings): a stylesheet draws `url("s2r://panorama/images/custom/x.vsvg")`
    // and a script sets `file://{images}/minecraft/heart_png.png`; neither is in the compiled file's reference list,
    // so the images read as never loaded.
    let css = r#"#Heal { background-image: url("s2r://panorama/images/custom/healing_sum_cross.vsvg"); }"#;
    let js = r#"panel.SetImage("file://{images}/minecraft/heart_8x_png.png"); $.Load("file://{resources}/layout/qol.xml");"#;
    // a compiled layout lists its scripts as bare paths (an FOV slider's settings page, 6871)
    assert_eq!(panorama_refs(b"\x00TS panorama/scripts/fov_unlock.vts PANEL"), vec!["panorama/scripts/fov_unlock.vts_c".to_string()]);
    assert_eq!(
        panorama_refs(format!("{css}\n{js}").as_bytes()),
        vec!["panorama/images/custom/healing_sum_cross.vsvg_c".to_string(), "panorama/images/minecraft/heart_8x_png_png.vtex_c".into(), "panorama/layout/qol.vxml_c".into()]
    );
    let g = game();
    let image = "panorama/images/custom/healing_sum_cross.vsvg_c";
    let sheet = |text: &str| file("panorama/styles/qol_hud.vcss_c", resource::synth_with(&[], &[(*b"DATA", text.as_bytes().to_vec())]));
    let named = run(vec![sheet(css), file(image, res(&[]))], Some(&g), None);
    assert!(!named.iter().any(|x| x.code == "M031" && x.path.as_deref() == Some(image)), "{named:?}");
    // an image nothing names is still never loaded
    let unnamed = run(vec![sheet("#Heal { }"), file(image, res(&[]))], Some(&g), None);
    assert!(unnamed.iter().any(|x| x.code == "M031" && x.path.as_deref() == Some(image)), "{unnamed:?}");
}

#[test]
fn m031_counts_the_scripts_a_compiled_layout_tree_names() {
    // Real case (6880, a community HUD mod, 20 scripts read as never loaded): the layout's LaCo block is LZ4 in the
    // game's compile, so `panorama/scripts/aspect_ratio.vjs_c` under SCRIPTS is broken up in the raw bytes and only the
    // decoded tree names it. (The fixture's KV3 is written uncompressed, so this holds the read, not the codec.)
    let g = game();
    let script = "panorama/scripts/aspect_ratio.vjs_c";
    let tree = kv(&format!(r#"{{ m_AST = {{ eType = "ROOT" children = [ {{ eType = "SCRIPTS" children = [ {{ eType = "INCLUDE" name = "{script}" }} ] }} ] }} }}"#));
    let layout = file("panorama/layout/my_hud.vxml_c", res_kv(&[], &[(b"LaCo", tree)]));
    let f = run(vec![layout, file(script, res(&[]))], Some(&g), None);
    assert!(!f.iter().any(|x| x.code == "M031" && x.path.as_deref() == Some(script)), "{f:?}");
    // a script no layout or other file names is still never loaded
    let f = run(vec![file(script, res(&[]))], Some(&g), None);
    assert!(f.iter().any(|x| x.code == "M031" && x.path.as_deref() == Some(script)), "{f:?}");
}
#[test]
fn m081_reads_the_radius_an_initializer_writes() {
    // Real case (6871, a community skin, the smoke at a cast): Valve's constant radius is 2 and an InitFloat on the
    // default output field (the radius) draws 30 to 40; the copy leaves the constant out (5) and draws 5 to 10. It
    // read as 2.5x Valve's size; it is a quarter of it.
    let smoke = |constant: &str, max: f64| {
        kv(&format!(
            r#"{{ _class = "CParticleSystemDefinition" {constant}
                m_Initializers = [ {{ _class = "C_INIT_InitFloat" m_InputValue = {{ m_nType = "PF_TYPE_RANDOM_BIASED" m_flRandomMin = 1.0 m_flRandomMax = {max} }} }},
                                   {{ _class = "C_INIT_InitFloat" m_nOutputField = 4 m_InputValue = {{ m_nType = "PF_TYPE_RANDOM_UNIFORM" m_flRandomMax = 360.0 }} }} ]
                m_Renderers = [ {{ _class = "C_OP_RenderSprites" }} ] }}"#
        ))
    };
    let valve = deep::particle_profile(&smoke("m_flConstantRadius = 2.0", 40.0));
    assert_eq!(valve.radius, 40.0, "the initializer's radius, not the constant's 2 nor the rotation's 360");
    let ours = deep::particle_profile(&smoke("", 10.0));
    assert!(deep::particle_energy(&ours, &valve, &policy::DEFAULT).is_empty());
    // the true catch: the same smoke drawn at up to 80 is twice Valve's size
    let big = deep::particle_profile(&smoke("", 80.0));
    assert!(deep::particle_energy(&big, &valve, &policy::DEFAULT).iter().any(|m| m.contains("radius 80")), "{:?}", deep::particle_energy(&big, &valve, &policy::DEFAULT));
}

#[test]
fn m150_a_material_name_no_valve_material_of_its_shader_sets() {
    // Measured (CSDK 13, 2026-11-27): the compiler keeps any parameter and feature a .vmat gives it
    // (g_flMagicianProbe1, F_MAGICIAN_PROBE, g_flRimLightScale1 all came out in the vmat_c), so a name is in the
    // compiled file whether the shader reads it or not.
    let mut params = matparams::MaterialParams::new();
    let valve = resource::material::Material { shader: "pbr.vfx".into(), floats: vec![("g_flSelfIllumScale1".into(), 1.0)], ints: vec![("F_SHEEN".into(), 1)], compiled: true, ..Default::default() };
    matparams::collect(&valve, &mut params);
    let mat = |extra: &str| res_kv(&[], &[(b"DATA", kv(&format!("{{ m_shaderName = \"pbr.vfx\" m_intParams = [ {{ m_name = \"F_SHEEN\" m_nValue = 1 }} ] m_floatParams = [ {{ m_name = \"g_flSelfIllumScale1\" m_flValue = 2.0 }} {extra} ] }}")))]);
    let fake = Fake { params: Some(params), ..Default::default() };
    let g = game();
    let f = run_game(vec![file("materials/x/new.vmat_c", mat("{ m_name = \"g_flRimLightScale1\" m_flValue = 3.0 }"))], &g, &fake);
    let m = f.iter().find(|x| x.code == "M150").unwrap_or_else(|| panic!("{f:#?}"));
    assert!(m.message.contains("g_flRimLightScale1") && m.message.contains("pbr.vfx") && m.message.contains("the scene's"), "{}", m.message);
    assert!(!m.message.contains("g_flSelfIllumScale1"), "a name Valve sets is read: {}", m.message);
    assert_eq!(m.severity, Severity::Warn);
    // only names Valve sets: silent; and without Valve's names nothing is judged
    assert!(!codes(&run_game(vec![file("materials/x/new.vmat_c", mat(""))], &g, &fake)).contains("M150"));
    assert!(!codes(&run_game(vec![file("materials/x/new.vmat_c", mat("{ m_name = \"g_flX\" m_flValue = 3.0 }"))], &g, &Fake::default())).contains("M150"));
}

#[test]
fn m151_a_replaced_material_on_another_shader() {
    let shader = |s: &str| res_kv(&[], &[(b"DATA", kv(&format!("{{ m_shaderName = \"{s}\" }}")))]);
    let fake = Fake { files: HashMap::from([(MAT.to_string(), shader("pbr.vfx"))]), ..Default::default() };
    let g = game();
    let f = run_game(vec![file(MAT, shader("environment_texture_set.vfx"))], &g, &fake);
    let m = f.iter().find(|x| x.code == "M151").unwrap_or_else(|| panic!("{f:#?}"));
    assert!(m.message.contains("environment_texture_set.vfx") && m.message.contains("Material Editor") && m.message.contains("pbr.vfx"), "{}", m.message);
    assert!(!codes(&run_game(vec![file(MAT, shader("pbr.vfx"))], &g, &fake)).contains("M151"), "Valve's own shader");
}

/// An attachment users index naming each `(name, kind, key, where, owners)`.
fn attachment_users(uses: &[(&str, &str, &str, &str, &str)]) -> crate::attach::AttachmentUsers {
    let mut text = format!("# magician attachment users build=6880 rev={} code=yes names=0\n", crate::attach::ATTACHMENT_USERS_REV);
    for (n, k, key, at, o) in uses {
        text.push_str(&format!("{n}\t{k}\t{key}\t{at}\t{o}\n"));
    }
    crate::attach::AttachmentUsers::parse(&text).unwrap()
}

#[test]
fn m062_a_lost_attachment_is_judged_by_who_names_it() {
    use crate::attach::judge;
    let model = hero(GAMEDATA, &[HERO_SKEL], HERO_GRAPHS, 90.0, 90.0);
    let me = vec!["hero_haze".to_string()];
    let played: std::collections::BTreeSet<String> = ["models/heroes/haze/run.vnmclip_c".to_string()].into();
    let users = attachment_users(&[
        ("ability_apply", "vdata", "m_strAttachmentName", "scripts/abilities.vdata_c:ability_haze_dagger", "hero_haze"),
        ("head_fx", "vdata", "m_sHealthBarAttachment", "scripts/abilities.vdata_c:upgrade_magic_shield", "upgrade_magic_shield"),
        ("standing_pivot", "code", "heroes=42/42", "client.dll", "*"),
        ("attach_hitloc", "code", "heroes=8/42", "client.dll", "*"),
        ("shoulder_fx", "preview", "m_attachmentName", "particles/haze/dagger.vpcf_c", "hero_haze"),
        ("rattyguard_shield_a", "vdata", "m_strKnockoffAttachment", "scripts/abilities.vdata_c:ability_rat", "hero_other"),
        ("muzzle_fx", "particle", "C_OP_SetControlPointsToModelParticles/m_AttachmentName", "particles/haze/gun.vpcf_c", "hero_haze"),
        ("foot_fx", "clip", "m_attachmentName", "models/heroes/haze/run.vnmclip_c", "hero_haze"),
        ("dead_fx", "vdata", "m_strAttachmentName", "scripts/abilities.vdata_c:ability_cut", "-"),
        ("spawn_fx", "clip", "m_attachmentPoint0", "models/heroes/haze/summon/spawn.vnmclip_c", "hero_haze"),
        ("bull_eye_fx", "vdata", "m_strBeamStartAttachmentPoint", "scripts/abilities.vdata_c:citadel_ability_boss_beam", "citadel_ability_boss_beam"),
        ("palm_r", "vdata", "m_strCustomAttachmentSource", "scripts/abilities.vdata_c:upgrade_capacitor", "upgrade_capacitor"),
    ]);
    let one = |name: &str, users: Option<&crate::attach::AttachmentUsers>| {
        let m = judge(vec![format!("attachment '{name}' missing")], &model, &model, &me, &played, users).pop().unwrap();
        (deep::contract_severity(&m), m)
    };
    // something the game runs for this hero names it: an error that says who and what happens
    for (name, who) in [
        ("healthbar", "this model's own game data (m_strUnitStatusAttachmentName)"),
        ("near_00", "this model's own game data (attachment_name)"),
        ("ability_apply", "this unit's own vdata and effects (scripts/abilities.vdata_c:ability_haze_dagger m_strAttachmentName)"),
        ("head_fx", "what any hero can carry"),
        ("palm_r", "scripts/abilities.vdata_c:upgrade_capacitor"),
        ("standing_pivot", "the game's client code"),
        ("foot_fx", "models/heroes/haze/run.vnmclip_c m_attachmentName"),
    ] {
        let (s, m) = one(name, Some(&users));
        assert!(s == Severity::Error && m.contains(who) && m.contains("model's origin") && m.contains("UNVERIFIED"), "{name}: {m}");
    }
    // only the particle editor's preview, another hero's own data, a particle's own render model, data no one runs,
    // or nothing at all: a warning that says so
    for (name, said) in [
        ("shoulder_fx", "the particle editor's preview of 1 effect(s) names it"),
        ("rattyguard_shield_a", "only other units' own data names it"),
        ("bull_eye_fx", "only other units' own data names it"),
        ("spawn_fx", "animation this model's graphs do not play names it"),
        ("muzzle_fx", "on the model the particle draws"),
        ("dead_fx", "nothing in the game names it"),
        ("attach_hitloc", "only some of Valve's heroes carry it (8/42)"),
        ("never_named", "nothing in the game names it"),
    ] {
        let (s, m) = one(name, Some(&users));
        assert!(s == Severity::Warn && m.contains(said) && m.ends_with(crate::carry::NOTHING_CHANGES), "{name}: {m}");
    }
    // the same name in a preview and in this hero's vdata: the vdata decides
    let both = attachment_users(&[("shoulder_fx", "preview", "m_attachmentName", "particles/a.vpcf_c", "hero_haze"), ("shoulder_fx", "vdata", "m_strEffectsAttachName", "scripts/heroes.vdata_c:hero_haze", "hero_haze")]);
    assert_eq!(one("shoulder_fx", Some(&both)).0, Severity::Error);
    // no index, or one written without the client library: unknown, never clean
    let (s, m) = one("never_named", None);
    assert!(s == Severity::Error && m.contains("could not be checked") && m.contains("magician index"), "{m}");
    let blind = crate::attach::AttachmentUsers::default();
    let (s, m) = one("never_named", Some(&blind));
    assert!(s == Severity::Error && m.contains("without the client library"), "{m}");
    // another breach of the same name is not this judgement's, and Valve's model against itself has none
    let other = "attachment 'healthbar' missing: Valve's model has another of that name, on head at 0.00,0.00,10.00 rot 0.00,0.00,0.00,1.00".to_string();
    assert_eq!(judge(vec![other.clone()], &model, &model, &me, &played, Some(&users)), vec![other]);
    assert!(hero_findings(model.clone(), Some(&["standing_pivot"])).is_empty());
    // an older build's game data that places the health bar by no attachment: the game does not look the name up on
    // this model, whatever the code holds; the game-data finding says where the bar goes
    let older = hero(&GAMEDATA.replace(r#" m_strUnitStatusAttachmentName = "healthbar""#, ""), &[HERO_SKEL], HERO_GRAPHS, 90.0, 90.0);
    let code = attachment_users(&[("healthbar", "code", "heroes=42/42", "client.dll", "*")]);
    let m = judge(vec!["attachment 'healthbar' missing".into()], &older, &model, &me, &played, Some(&code)).pop().unwrap();
    assert!(deep::contract_severity(&m) == Severity::Warn && m.contains("Valve's model's game data names it (m_strUnitStatusAttachmentName)") && m.contains("client code holds the name too"), "{m}");
    // the same data still naming it: the model's own game data looks it up, an error
    let m = judge(vec!["attachment 'healthbar' missing".into()], &model, &model, &me, &played, Some(&code)).pop().unwrap();
    assert!(deep::contract_severity(&m) == Severity::Error && m.contains("this model's own game data"), "{m}");
}

#[test]
fn attachment_users_read_back_only_in_their_own_format() {
    let u = attachment_users(&[("palm_r", "vdata", "m_strEffectsAttachName", "scripts/heroes.vdata_c:hero_haze", "hero_haze"), ("aim", "code", "heroes=42/42", "client.dll", "*"), ("x", "preview", "k", "p.vpcf_c", "-")]);
    assert!(u.code_read && u.of("PALM_R")[0].owners == ["hero_haze"] && u.of("aim")[0].kind == "code" && u.of("x")[0].owners.is_empty());
    assert!(crate::attach::AttachmentUsers::parse("# magician attachment users build=6880 rev=0 code=yes names=0\n").is_none(), "another format");
    assert!(crate::attach::AttachmentUsers::parse("palm_r\tvdata\n").is_none(), "not the file");
}

#[test]
fn attachment_names_are_what_a_key_that_says_attach_holds() {
    let v = resource::kv3::parse_text(
        r#"{ m_controlPointConfigurations = [ { m_name = "preview" m_drivers = [ { m_iAttachType = "PATTACH_POINT_FOLLOW" m_attachmentName = "ability_apply" } ] } ]
            m_Operators = [ { _class = "C_OP_SetControlPointsToModelParticles" m_AttachmentName = "muzzle_fx" } ]
            m_eAttachmentSourceType = "EAttachmentSource_Custom" m_strCustomAttachmentSource = "palm_R" m_BombAttachedParticle = "particles/x.vpcf"
            m_strRatAttachSound = "Rat.Attach" m_strShootAttachment = "" m_flAttachOffset = 70.0 }"#,
    )
    .unwrap();
    let got: Vec<(String, String, bool)> = crate::attach::attachment_names(&v).into_iter().collect();
    assert_eq!(
        got,
        vec![
            ("ability_apply".to_string(), "m_attachmentName".to_string(), true),
            ("muzzle_fx".to_string(), "C_OP_SetControlPointsToModelParticles/m_AttachmentName".to_string(), false),
            ("palm_R".to_string(), "m_strCustomAttachmentSource".to_string(), false),
        ]
    );
}

#[test]
fn m030_in_a_file_the_game_never_reads_is_a_warning() {
    let g = game();
    // the model replaces Valve's, so the game reads it, and the material it draws with: a missing texture is an error
    let mat = "models/heroes_wip/lash/materials/mine.vmat_c";
    let f = run(vec![file(MODEL, res(&["models/heroes_wip/lash/materials/mine.vmat"])), file(mat, res(&["models/heroes_wip/lash/materials/gone_png_1.vtex"]))], Some(&g), None);
    let m: Vec<&Finding> = f.iter().filter(|x| x.code == "M030").collect();
    assert!(m.len() == 1 && m[0].severity == Severity::Error && m[0].path.as_deref() == Some(mat), "{m:?}");
    // the same material named only by a model at a new path that nothing names: never read, a warning that says so
    let f = run(vec![file("models/mine/old_copy.vmdl_c", res(&["models/heroes_wip/lash/materials/mine.vmat"])), file(mat, res(&["models/heroes_wip/lash/materials/gone_png_1.vtex"]))], Some(&g), None);
    let m: Vec<&Finding> = f.iter().filter(|x| x.code == "M030").collect();
    assert!(m.len() == 1 && m[0].severity == Severity::Warn && m[0].message.ends_with(crate::NEVER_READ), "{m:?}");
    // a file the game reads directly keeps its error
    let f = run(vec![file(MODEL, res(&["models/heroes_wip/lash/gone.vmat"]))], Some(&g), None);
    assert!(f.iter().any(|x| x.code == "M030" && x.severity == Severity::Error), "{f:?}");
}
