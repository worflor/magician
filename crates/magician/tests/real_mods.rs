//! Grading on real mods, when this machine has them, installed in the game through Magician (so the install ledger
//! says which pak is whose), with their addons' `magician.allow` in the CSDK: Maid Lash (a released skin, its `mod.toml`
//! in a `model-replacements/maid-lash/` folder above this repository), and any new-character mod with a voice pak named
//! by `MAGICIAN_REAL_NEW_CHARACTER`. Every location comes from `magician paths`; with any of it missing a test says what
//! it skipped and passes. Reads only: nothing is written into the game, the CSDK or the mods.

#![allow(clippy::unwrap_used, clippy::expect_used)] // a test that cannot unwrap has failed

use std::path::{Path, PathBuf};
use std::process::Command;

struct Machine {
    game: PathBuf,
    csdk: Option<PathBuf>,
}

/// The binary as this machine runs it (its own config file, game and reference folder), stdout and stderr.
fn magician(args: &[&str]) -> (Option<i32>, String, String) {
    let o = Command::new(env!("CARGO_BIN_EXE_magician")).args(args).env_remove("MAGICIAN_POLICY").output().unwrap();
    (o.status.code(), String::from_utf8_lossy(&o.stdout).into_owned(), String::from_utf8_lossy(&o.stderr).into_owned())
}

fn machine() -> Option<Machine> {
    let (_, out, _) = magician(&["paths"]);
    let get = |k: &str| out.lines().find_map(|l| l.strip_prefix(&format!("{k}\t"))).and_then(|r| r.split('\t').next()).filter(|p| *p != "-").map(PathBuf::from);
    Some(Machine { game: get("game")?, csdk: get("csdk") })
}

/// The installed pak the ledger says belongs to `owner`.
fn installed(m: &Machine, owner: &str) -> Option<PathBuf> {
    let addons = m.game.join("citadel").join("addons");
    let ledger = std::fs::read_to_string(addons.join(".magician-installed.tsv")).ok()?;
    ledger.lines().filter_map(|l| {
        let c: Vec<&str> = l.split('\t').collect();
        (c.len() >= 3 && c[2] == owner && c[0].ends_with("_dir.vpk")).then(|| addons.join(c[0]))
    }).find(|p| p.is_file())
}

fn mod_toml(slug: &str) -> Option<PathBuf> {
    Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().map(|d| d.join("model-replacements").join(slug).join("mod.toml")).find(|p| p.is_file())
}

fn allow(m: &Machine, addon: &str) -> Option<PathBuf> {
    Some(m.csdk.as_ref()?.join("content").join("citadel_addons").join(addon).join("magician.allow")).filter(|p| p.is_file())
}

/// The grade record of these paks, or why the case was skipped.
/// The grade record of these paks; with `allow` `None`, the grade finds the mod's allowance file from its `mod.toml`.
fn record(paks: &[PathBuf], toml: &Path, allow: Option<&Path>, policy: Option<&Path>) -> String {
    let mut args: Vec<String> = vec!["grade".into()];
    args.extend(paks.iter().map(|p| p.to_string_lossy().into_owned()));
    args.extend(["--mod".into(), toml.to_string_lossy().into_owned(), "--json".into()]);
    // Judged for the build the skin was last proven on: a newer game on this machine makes it stale, which is the
    // grade's freshness axis doing its job, not a change in what these tests hold.
    let text = std::fs::read_to_string(toml).unwrap();
    if let Some(b) = text.lines().find(|l| l.trim_start().starts_with("builds")).and_then(|l| l.split(|c: char| !c.is_ascii_digit()).rfind(|s| !s.is_empty())) {
        args.extend(["--target".into(), b.to_string()]);
    }
    if let Some(a) = allow {
        args.extend(["--allow".into(), a.to_string_lossy().into_owned()]);
    }
    if let Some(p) = policy {
        args.extend(["--policy".into(), p.to_string_lossy().into_owned()]);
    }
    let a: Vec<&str> = args.iter().map(String::as_str).collect();
    let (code, out, err) = magician(&a);
    assert_eq!(code, Some(0), "grade exits 0 whatever the grade: {err}");
    assert!(out.starts_with("{\"schema\":\"magician.grade/3\""), "{out}");
    out
}

/// A number field after `key` in the record text (the first one).
fn number_after(json: &str, key: &str) -> f64 {
    let rest = &json[json.find(key).unwrap_or_else(|| panic!("{key} not in {json}")) + key.len()..];
    rest.split(|c: char| !(c.is_ascii_digit() || c == '.')).next().unwrap().parse().unwrap()
}

macro_rules! need {
    ($e:expr, $what:expr) => {
        match $e {
            Some(v) => v,
            None => {
                eprintln!("skipped: {}", $what);
                return;
            }
        }
    };
}

/// A new-character mod with two paks (a skin and a voice pak), named by `MAGICIAN_REAL_NEW_CHARACTER` (its folder,
/// holding its `mod.toml` with `[mod] owners` naming the voice pak's ledger owner): judged together it is an Echo
/// whose sound is its own, a new character by its head, name and hero art, fair play passing, and both paks recorded
/// lowest slot first; the skin pak alone is still a new character, its voice Valve's and said so; and with only `--mod` the grade
/// finds the same allowance file and gives the same label.
#[test]
fn a_new_character_with_its_voice_pak_is_an_echo_and_its_skin_pak_alone_still_new() {
    let m = need!(machine(), "Deadlock not found (magician paths)");
    let dir = need!(std::env::var_os("MAGICIAN_REAL_NEW_CHARACTER").map(PathBuf::from), "MAGICIAN_REAL_NEW_CHARACTER is not set");
    let toml = dir.join("mod.toml");
    let text = need!(std::fs::read_to_string(&toml).ok(), "no mod.toml in MAGICIAN_REAL_NEW_CHARACTER");
    let field = |key: &str| text.lines().find_map(|l| l.trim().strip_prefix(key).map(|r| r.trim_start_matches([' ', '='])).map(str::to_string));
    let slug = need!(field("slug").map(|s| s.trim_matches('"').replace('-', "_")), "the mod.toml has no slug");
    let skin = need!(installed(&m, &slug), "the mod's skin pak is not in the ledger");
    // the voice pak: the first other ledger owner of the mod with a pak installed (a scene map is not a pak)
    let owners: Vec<String> = field("owners").map(|o| o.split('"').skip(1).step_by(2).map(str::to_string).collect()).unwrap_or_default();
    let voice = need!(owners.iter().filter(|o| **o != slug).find_map(|o| installed(&m, o)), "no other pak of the mod ([mod] owners) in the ledger");
    let allow = need!(allow(&m, &slug), "no magician.allow for the mod in the CSDK");
    let name = |p: &Path| p.file_name().unwrap().to_string_lossy().into_owned();
    let both = record(&[skin.clone(), voice.clone()], &toml, Some(&allow), None);
    let label = |r: &str| r.rsplit("\"label\":\"").next().unwrap_or("").split('"').next().unwrap_or("").to_string();
    assert!(label(&both).starts_with("Echo (look, ") && label(&both).contains(", sound)"), "{both}");
    assert!(both.contains("\"identity\":{\"kind\":\"new\",\"signals\":[\"head_replaced\",\"renamed\",\"new_hero_art\"]"), "{both}");
    assert!(both.contains("\"fair_play\":{\"state\":\"pass\""), "{both}");
    assert!(both.contains("\"rights\":{\"state\":\"cleared\"") && both.contains("\"release_gate\":{\"pass\":true,\"blockers\":[]"), "{both}");
    let (lo, hi) = if name(&voice) < name(&skin) { (name(&voice), name(&skin)) } else { (name(&skin), name(&voice)) };
    let (a, b) = (both.find(&format!("{{\"name\":\"{lo}\",\"sha256\":\"")), both.find(&format!("{{\"name\":\"{hi}\",\"sha256\":\"")));
    assert!(matches!((a, b), (Some(x), Some(y)) if x < y), "both paks, lowest slot first: {both}");
    // the skin pak alone has no voice pak: still a new character
    let alone = record(std::slice::from_ref(&skin), &toml, Some(&allow), None);
    assert!(alone.contains("\"identity\":{\"kind\":\"new\"") && label(&alone).starts_with("Echo (look, "), "{alone}");
    assert!(alone.contains("Valve's voice") && alone.contains("{\"id\":\"voice\",\"group\":\"sound\",\"weight\":5,\"counted\":false"), "the voice is in the other pak, shown and not counted: {alone}");
    assert!(alone.contains(&format!("{{\"name\":\"{}\",\"sha256\":\"", name(&skin))) && !alone.contains(&format!("{{\"name\":\"{}\"", name(&voice))), "{alone}");
    // with only --mod the grade finds the same allowance file and gives the same grade
    let found = record(&[skin, voice], &toml, None, None);
    assert_eq!(label(&found), label(&both), "{found}");
    assert!(found.contains("\"allowance_files\":[\"allowances magician.allow (sha256 "), "{found}");
}

#[test]
fn maid_lash_is_a_costume_of_look_effects_and_sound_with_its_m140_allowance_listed_and_fair_play_pass() {
    let m = need!(machine(), "Deadlock not found (magician paths)");
    let pak = need!(installed(&m, "maid_lash"), "no installed maid_lash pak in the ledger");
    let toml = need!(mod_toml("maid-lash"), "no model-replacements/maid-lash/mod.toml");
    let allow = need!(allow(&m, "maid_lash"), "no maid_lash magician.allow in the CSDK");
    let r = record(std::slice::from_ref(&pak), &toml, Some(&allow), None);
    // This test is about the released pak. A rebuild installed while it is being worked on has no proof yet, so its
    // label and gate carry `unproven`: not this test's subject.
    need!((!r.contains("\"freshness\":{\"proven\":[]")).then_some(()), "the installed maid_lash pak has no proof on record (a rebuild under test)");
    assert!(r.contains("\"label\":\"Costume (look, effects, sound) · ranked-safe\""), "{r}");
    assert!(r.contains("\"broken\":false") && !r.contains("\"letter\"") && !r.contains("\"caps\""), "{r}");
    assert!(r.contains("\"identity\":{\"kind\":\"same\""), "Valve's head, still Lash: {r}");
    assert!(r.contains("\"fair_play\":{\"state\":\"pass\""), "{r}");
    assert!(r.contains("\"rights\":{\"state\":\"cleared\"") && r.contains("\"release_gate\":{\"pass\":true,\"blockers\":[]"), "{r}");
    assert!(r.contains("\"fair_play_enforced\":false,\"rights_enforced\":false"), "lenient by default: {r}");
    let allowance = &r[r.find("\"allowances\":[{\"rule\":\"M140\"").unwrap_or_else(|| panic!("no M140 allowance: {r}"))..];
    let measure = &allowance[allowance.find("\"name\":\"worst_units\"").unwrap()..];
    let (worst, limit) = (number_after(measure, "\"value\":"), number_after(measure, "\"limit\":"));
    assert!(worst > 10.0 && worst <= limit && limit == 18.0, "the bell skirt sits between the taste line and the M129 line: {worst} (limit {limit})");
    // the same pak judged twice is the same record, byte for byte
    assert_eq!(record(std::slice::from_ref(&pak), &toml, Some(&allow), None), r);
    // with only --mod the grade finds the mod's own allowance file: the same label, not an unexplained freeform
    let found = record(std::slice::from_ref(&pak), &toml, None, None);
    assert!(found.contains("\"label\":\"Costume (look, effects, sound) · ranked-safe\"") && found.contains("\"fair_play\":{\"state\":\"pass\",\"name\":\"ranked-safe\""), "{found}");

    // A policy that moves the M140 allowance limit: just under the bell's worst point it is freeform; at it, or just over,
    // the allowance holds. The record names the policy by fingerprint.
    let d = std::env::temp_dir().join(format!("magician-real-policy-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    let at_with = |limit: f64, enforce: bool| {
        let p = d.join(format!("limit-{limit}-{enforce}.toml"));
        std::fs::write(&p, format!("[fair_play]\nenforce = {enforce}\n[compliance]\nm140_max_worst_units = {limit}\n")).unwrap();
        record(std::slice::from_ref(&pak), &toml, Some(&allow), Some(&p))
    };
    let at = |limit: f64| at_with(limit, false);
    let under = at(worst - 0.01);
    assert!(under.contains("\"fair_play\":{\"state\":\"fail\",\"name\":\"freeform\"") && under.contains("\"label\":\"Costume (look, effects, sound) · freeform"), "{under}");
    assert!(under.contains("\"overrides\":[\"compliance.m140_max_worst_units\"]"), "{under}");
    // lenient, freeform is a warning at the gate; enforced, it blocks
    assert!(under.contains("\"release_gate\":{\"pass\":true,\"blockers\":[],\"warnings\":[\"fair play: freeform (M140"), "{under}");
    let strict = at_with(worst - 0.01, true);
    assert!(strict.contains("\"release_gate\":{\"pass\":false,\"blockers\":[\"fair play: freeform (M140") && strict.contains("\"label\":\"Costume (look, effects, sound) · freeform"), "{strict}");
    for limit in [worst, worst + 0.01] {
        let ok = at(limit);
        assert!(ok.contains("\"fair_play\":{\"state\":\"pass\""), "{limit}: {ok}");
    }
    let _ = std::fs::remove_dir_all(&d);
}
