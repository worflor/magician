//! The automation contract, held against the built binary: `--help` and `--version` do nothing else, a typo in an
//! option is a usage error, and the exit codes keep one meaning each. Needs no game: every case stops before one is
//! looked for, or points the game at a folder that is not one.

#![allow(clippy::unwrap_used, clippy::expect_used)] // a test that cannot unwrap has failed

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("magician-contract-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// The binary with a scratch reference folder, an empty config and no game, so nothing real can be touched.
fn magician(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_magician"))
        .args(args)
        .env("MAGICIAN_REFERENCE", dir.join("reference"))
        .env("MAGICIAN_CONFIG", dir.join("config.txt"))
        .env("DEADLOCK_GAME", dir.join("no-game"))
        .env("MAGICIAN_NO_CACHE", "1")
        .env_remove("MAGICIAN_POLICY")
        .output()
        .unwrap()
}

/// The Python 3 on PATH, as the front door looks for it (`python`, then `python3`), or None.
fn python() -> Option<&'static str> {
    ["python", "python3"].into_iter().find(|p| {
        Command::new(p).arg("--version").output().is_ok_and(|o| o.status.success() && String::from_utf8_lossy(&[o.stdout, o.stderr].concat()).contains("Python 3"))
    })
}

fn text(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned() + &String::from_utf8_lossy(&o.stderr)
}

#[test]
fn help_prints_usage_and_does_nothing_else() {
    let d = scratch("help");
    for args in [&["baseline", "--help"][..], &["patchday", "--capture", "-h"], &["install", "pak10_dir.vpk", "--help"], &["help"], &["--help"]] {
        let o = magician(&d, args);
        assert_eq!(o.status.code(), Some(0), "{args:?}: {}", text(&o));
        assert!(String::from_utf8_lossy(&o.stdout).contains("magician "), "{args:?}: usage on stdout");
    }
    let o = magician(&d, &["baseline", "--help"]);
    let out = String::from_utf8_lossy(&o.stdout);
    assert!(out.contains("magician baseline") && !out.contains("magician scene make") && out.contains("Exit codes"), "{out}");
    assert!(!d.join("reference").exists(), "--help must not write a baseline or anything else");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn version_names_the_toolchain() {
    let d = scratch("version");
    let o = magician(&d, &["--version"]);
    assert_eq!(o.status.code(), Some(0));
    let v = String::from_utf8_lossy(&o.stdout);
    assert!(v.starts_with(&format!("magician {} (git ", env!("CARGO_PKG_VERSION"))), "{v}");
    let o = magician(&d, &["patchday", "--version"]);
    assert_eq!((o.status.code(), String::from_utf8_lossy(&o.stdout).into_owned()), (Some(0), v.into_owned()), "--version anywhere");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn rules_print_their_catalogue_as_json() {
    let d = scratch("rules");
    let o = magician(&d, &["rules", "--json"]);
    let out = String::from_utf8_lossy(&o.stdout).into_owned();
    assert_eq!(o.status.code(), Some(0));
    assert!(out.starts_with("{\"schema\":\"magician.rules/1\",\"rules\":[{\"code\":\"M001\"") && out.contains("\"code\":\"M144\""), "{out}");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn usage_errors_exit_2() {
    let d = scratch("usage");
    for args in [&["lint", "x.vpk", "--deeep"][..], &["ls", "x.vpk", "--json"], &["no-such-command"], &["ls", "missing.vpk"], &[], &["lint", "x.vpk", "--game"], &["install", "x.vpk", "--as"], &["lint", "x.vpk", "--game", "a", "--game", "b"]] {
        let o = magician(&d, args);
        assert_eq!(o.status.code(), Some(2), "{args:?}: {}", text(&o));
    }
    assert!(text(&magician(&d, &["lint", "x.vpk", "--deeep"])).contains("unknown option --deeep"));
    let _ = std::fs::remove_dir_all(&d);
}

/// A folder that passes for Deadlock's `game` folder: a build number and a stock hero-page map, nothing else.
fn fake_game(root: &Path, files: &[(String, Vec<u8>)]) -> PathBuf {
    let game = root.join("game");
    std::fs::create_dir_all(game.join("citadel").join("maps").join("ui").join("hero_prefabs")).unwrap();
    std::fs::write(game.join("citadel").join("steam.inf"), "ClientVersion=6864\n").unwrap();
    vpk::write(game.join("citadel").join("maps").join("ui").join("hero_prefabs").join("lash.vpk"), files).unwrap();
    game
}

fn scene(files: &[(&str, &str)]) -> Vec<(String, Vec<u8>)> {
    files.iter().map(|(p, b)| (format!("maps/ui/hero_prefabs/lash{p}"), b.as_bytes().to_vec())).collect()
}

#[test]
fn a_scene_patch_that_no_longer_fits_is_broken_for_check_and_refused_for_compose() {
    let d = scratch("scene");
    let valve = scene(&[(".vmap_c", "map"), ("/entities/default_ents.vents_c", "valve lump"), ("/cubemaps/cube.vtex_c", "cube")]);
    let game = fake_game(&d, &valve);
    let ours = d.join("lash.vpk");
    vpk::write(&ours, &scene(&[(".vmap_c", "map"), ("/entities/default_ents.vents_c", "OUR lump"), ("/cubemaps/cube.vtex_c", "cube")])).unwrap();
    let g = game.to_string_lossy().into_owned();
    let patch = d.join("patch");
    let run = |args: &[&str]| {
        let mut a: Vec<&str> = args.to_vec();
        a.extend(["--game", &g]);
        magician(&d, &a)
    };
    let o = run(&["scene", "make", &ours.to_string_lossy(), "--out", &patch.to_string_lossy()]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    assert_eq!(run(&["scene", "check", &patch.to_string_lossy()]).status.code(), Some(0));
    // a patch whose `result` is not the scene these files make: compose refuses (3) and writes nothing; --force writes
    let manifest = patch.join("scene-patch.txt");
    let good = std::fs::read_to_string(&manifest).unwrap();
    let line = good.lines().find(|l| l.starts_with("result	")).unwrap().to_string();
    std::fs::write(&manifest, good.replace(&line, "result	00000000")).unwrap();
    let wrong = d.join("wrong.vpk");
    let o = run(&["scene", "compose", &patch.to_string_lossy(), "--out", &wrong.to_string_lossy()]);
    assert_eq!(o.status.code(), Some(3), "{}", text(&o));
    assert!(!wrong.exists() && text(&o).contains("not the 00000000 its author proved"), "{}", text(&o));
    let o = run(&["scene", "compose", &patch.to_string_lossy(), "--out", &wrong.to_string_lossy(), "--force"]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    assert!(wrong.is_file() && text(&o).contains("NOT the 00000000 the author proved"), "{}", text(&o));
    std::fs::write(&manifest, good).unwrap();
    // the game updates the map under the patch
    fake_game(&d, &scene(&[(".vmap_c", "map"), ("/entities/default_ents.vents_c", "valve lump"), ("/cubemaps/cube.vtex_c", "a new cube")]));
    let o = run(&["scene", "check", &patch.to_string_lossy()]);
    assert_eq!(o.status.code(), Some(1), "check found something: {}", text(&o));
    let out = d.join("composed.vpk");
    let o = run(&["scene", "compose", &patch.to_string_lossy(), "--out", &out.to_string_lossy()]);
    assert_eq!(o.status.code(), Some(3), "compose refuses: {}", text(&o));
    assert!(!out.exists(), "a refusal writes nothing");
    let _ = std::fs::remove_dir_all(&d);
}

const CLIP: &str = "models/heroes/x/clips/run.vnmclip_c";

/// A whole fake install on build `build`: Valve's two archives (one clip, one model), `gameinfo.gi` already mounting
/// the addons folder, and one installed pak of mod `mod_a` that replaces the clip, recorded in the ledger.
fn fake_install(root: &Path, build: &str, valve_clip: &[u8]) -> PathBuf {
    let game = root.join("game");
    let citadel = game.join("citadel");
    std::fs::create_dir_all(citadel.join("addons")).unwrap();
    std::fs::create_dir_all(game.join("core")).unwrap();
    std::fs::write(citadel.join("steam.inf"), format!("ClientVersion={build}\n")).unwrap();
    std::fs::write(citadel.join("gameinfo.gi"), "\"GameInfo\"\n{\n\tFileSystem\n\t{\n\t\tSearchPaths\n\t\t{\n\t\t\tGame\tcitadel/addons\n\t\t\tGame\tcitadel\n\t\t\tGame\tcore\n\t\t}\n\t}\n}\n").unwrap();
    vpk::write(citadel.join("pak01_dir.vpk"), &[(CLIP.to_string(), valve_clip.to_vec()), ("models/heroes/x/x.vmdl_c".to_string(), b"valve model".to_vec())]).unwrap();
    vpk::write(game.join("core").join("pak01_dir.vpk"), &[("shaders/x.vcs".to_string(), b"core".to_vec())]).unwrap();
    let pak = citadel.join("addons").join("pak10_dir.vpk");
    vpk::write(&pak, &[(CLIP.to_string(), b"our clip".to_vec())]).unwrap();
    let crc = vpk::crc32(&std::fs::read(&pak).unwrap());
    std::fs::write(citadel.join("addons").join(".magician-installed.tsv"), format!("pak10_dir.vpk\t{crc:08x}\tmod_a\n")).unwrap();
    game
}

#[test]
fn patchday_compares_each_mod_from_its_last_proven_build() {
    let d = scratch("patchday");
    let g = |build: &str, clip: &[u8]| fake_install(&d, build, clip).to_string_lossy().into_owned();
    // build 6864 on record, then Valve changes the clip in 6865
    let game = g("6864", b"valve clip v1");
    assert_eq!(magician(&d, &["baseline", "--game", &game]).status.code(), Some(0));
    let game = g("6865", b"valve clip v2");
    assert_eq!(magician(&d, &["baseline", "--game", &game]).status.code(), Some(0));
    let toml = d.join("mod.toml");
    let ts = toml.to_string_lossy().into_owned();
    std::fs::write(&toml, "[mod]\nslug = \"mod-a\"\n[proven]\nbuilds = []\n").unwrap();
    let patchday = || magician(&d, &["patchday", "--game", &game, "--mod", &ts, "--json"]);
    // the installed pak proven on 6864 (it was installed then too): the clip changed under it since, so it loads and
    // must be re-proven (exit 4)
    let o = magician(&d, &["prove", &ts, "--game", &game, "--build", "6864", "--date", "2026-11-20"]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    assert!(text(&o).contains("proof ") && text(&o).contains("of build 6864 (2026-11-20) recorded in"), "{}", text(&o));
    let o = patchday();
    let out = String::from_utf8_lossy(&o.stdout).into_owned();
    assert_eq!(o.status.code(), Some(4), "{}", text(&o));
    assert!(out.starts_with("{\"schema\":\"magician.patchday/1\"") && out.contains("\"state\":\"re-prove\""), "{out}");
    assert!(out.contains("\"owner\":\"mod_a\",\"state\":\"re-prove\",\"from\":\"6864\""), "{out}");
    assert_eq!(out.trim_end().lines().count(), 1, "one JSON document and nothing else on stdout");
    // proven on the installed build: current, so clear (exit 0), even though the clip changed since 6864
    assert_eq!(magician(&d, &["prove", &ts, "--game", &game]).status.code(), Some(0));
    let o = patchday();
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    assert!(String::from_utf8_lossy(&o.stdout).contains("\"verdict\":{\"state\":\"clear\""));
    assert!(std::fs::read_to_string(&toml).unwrap().contains("builds = [6864, 6865]"), "the summary follows the proofs");
    // the human report says which build each mod was compared from
    let o = magician(&d, &["patchday", "--game", &game, "--mod", &ts]);
    assert!(String::from_utf8_lossy(&o.stdout).contains("mod_a: ok (proven on 6865)"), "{}", text(&o));

    // A rebuilt pak inherits nothing: the summary still lists 6864 and 6865, the new bytes are proven on neither.
    let pak = PathBuf::from(&game).join("citadel").join("addons").join("pak10_dir.vpk");
    vpk::write(&pak, &[(CLIP.to_string(), b"our clip, rebuilt".to_vec())]).unwrap();
    let crc = vpk::crc32(&std::fs::read(&pak).unwrap());
    std::fs::write(pak.with_file_name(".magician-installed.tsv"), format!("pak10_dir.vpk\t{crc:08x}\tmod_a\n")).unwrap();
    let o = magician(&d, &["patchday", "--game", &game, "--mod", &ts]);
    let out = String::from_utf8_lossy(&o.stdout).into_owned();
    assert_eq!(o.status.code(), Some(4), "{}", text(&o));
    assert!(out.contains("mod-a: its installed bytes proven on 0 of") && out.contains("6864, 6865 (claimed for other bytes or never recorded); build 6865: NOT proven"), "{out}");
    assert!(out.contains(&format!("pak10_dir.vpk (mod_a): crc {crc:08x} is not the bytes proven on 6865")), "{out}");
    assert!(out.contains("mod_a: RE-PROVE (compared from 6864): no proven build on record for these bytes"), "{out}");

    // A legacy list with no ledger at all reads the same way: the claim counts for nothing bound.
    std::fs::remove_file(d.join("proofs.tsv")).unwrap();
    let o = patchday();
    let out = String::from_utf8_lossy(&o.stdout).into_owned();
    assert_eq!(o.status.code(), Some(4), "{}", text(&o));
    assert!(out.contains("\"proven\":[],\"claimed\":[\"6864\",\"6865\"],\"unbound\":[\"6864\",\"6865\"]"), "{out}");
    assert!(std::fs::read_to_string(&toml).unwrap().contains("builds = [6864, 6865]"), "nothing dropped from the mod.toml");

    // prove refuses what is not the ledger's bytes, and a mod with nothing installed, writing nothing
    std::fs::write(&pak, b"not what was installed").unwrap();
    let o = magician(&d, &["prove", &ts, "--game", &game]);
    assert_eq!(o.status.code(), Some(3), "{}", text(&o));
    assert!(text(&o).contains("not readable as the install ledger has it") && !d.join("proofs.tsv").exists(), "{}", text(&o));
    std::fs::write(&toml, "[mod]\nslug = \"mod-b\"\n[proven]\nbuilds = []\n").unwrap();
    let o = magician(&d, &["prove", &ts, "--game", &game]);
    assert_eq!(o.status.code(), Some(3), "{}", text(&o));
    assert!(text(&o).contains("holds no file for mod_b") && std::fs::read_to_string(&toml).unwrap().contains("builds = []"), "{}", text(&o));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn patchday_says_per_era_whether_the_engine_changed_too() {
    let d = scratch("patchday-engine");
    let game = fake_install(&d, "6864", b"valve clip");
    let g = game.to_string_lossy().into_owned();
    let bin = game.join("bin").join("win64");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::write(bin.join("engine2.dll"), b"engine v1").unwrap();
    std::fs::write(game.join("citadel").join("shaders_pc_dir.vpk"), b"shaders v1").unwrap();
    assert_eq!(magician(&d, &["baseline", "--game", &g]).status.code(), Some(0));
    let engine = d.join("reference").join("6864").join("engine.tsv");
    let sidecar = std::fs::read_to_string(&engine).unwrap();
    assert!(sidecar.contains("bin/win64/engine2.dll\t9\t") && sidecar.contains("citadel/shaders_pc_dir.vpk\t10\t"), "{sidecar}");
    // gameinfo.gi already mounts the addons folder and there is no stock backup: not known as shipped, said so
    assert!(sidecar.contains("citadel/gameinfo.gi\t-\t-"), "{sidecar}");
    let toml = d.join("mod.toml");
    std::fs::write(&toml, "[mod]\nslug = \"mod-a\"\n[proven]\nbuilds = []\n").unwrap();
    let ts = toml.to_string_lossy().into_owned();
    assert_eq!(magician(&d, &["prove", &ts, "--game", &g]).status.code(), Some(0));
    // a data-only patch: the clip changes, the engine does not
    std::fs::write(game.join("citadel").join("steam.inf"), "ClientVersion=6865\n").unwrap();
    vpk::write(game.join("citadel").join("pak01_dir.vpk"), &[(CLIP.to_string(), b"valve clip v2".to_vec()), ("models/heroes/x/x.vmdl_c".to_string(), b"valve model".to_vec())]).unwrap();
    let o = magician(&d, &["patchday", "--game", &g, "--mod", &ts, "--capture"]);
    let out = String::from_utf8_lossy(&o.stdout).into_owned();
    assert_eq!(o.status.code(), Some(4), "{}", text(&o));
    assert!(out.contains("engine 6864 -> 6865: data only: the same 2 executables, libraries and shader archives (1 not known as shipped on one side: citadel/gameinfo.gi)"), "{out}");
    // the engine changes too: said per file, and the verdict is the same
    std::fs::write(game.join("citadel").join("steam.inf"), "ClientVersion=6866\n").unwrap();
    std::fs::write(bin.join("engine2.dll"), b"engine v2").unwrap();
    let o = magician(&d, &["patchday", "--game", &g, "--mod", &ts, "--capture", "--json"]);
    let out = String::from_utf8_lossy(&o.stdout).into_owned();
    assert_eq!(o.status.code(), Some(4), "the engine never decides the verdict: {}", text(&o));
    assert!(out.contains("\"engine\":[{\"from\":\"6864\",\"to\":\"6866\",\"changed\":[\"bin/win64/engine2.dll\"],\"added\":[],\"removed\":[],\"unknown\":[\"citadel/gameinfo.gi\"],\"state\":\"engine-too\"}"), "{out}");
    let o = magician(&d, &["patchday", "--game", &g, "--mod", &ts]);
    assert!(String::from_utf8_lossy(&o.stdout).contains("engine 6864 -> 6866: engine too: 1 file(s) changed, added or removed: bin/win64/engine2.dll (1 not known"),"{}", text(&o));
    // an era captured before the sidecar existed: no record, never a guess
    std::fs::remove_file(&engine).unwrap();
    let o = magician(&d, &["patchday", "--game", &g, "--mod", &ts]);
    assert!(String::from_utf8_lossy(&o.stdout).contains("engine 6864 -> 6866: no engine record for build 6864"), "{}", text(&o));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn prove_records_the_installed_bytes_and_the_evidence_and_release_copies_with_pak() {
    let d = scratch("prove");
    let game = fake_install(&d, "6865", b"valve clip").to_string_lossy().into_owned();
    let toml = d.join("mod.toml");
    let ts = toml.to_string_lossy().into_owned();
    std::fs::write(&toml, "[mod]\nslug = \"mod-a\"\n[proven]\nbuilds = [6861]  # filled by the harness\n").unwrap();
    let pass = d.join("pass");
    std::fs::create_dir_all(&pass).unwrap();
    std::fs::write(pass.join("index.md"), "# pass\n").unwrap();
    let o = magician(&d, &["prove", &ts, "--game", &game, "--evidence", &pass.to_string_lossy(), "--json"]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    let out = String::from_utf8_lossy(&o.stdout).into_owned();
    let index_sha = sha256_hex(b"# pass\n");
    assert!(out.starts_with("{\"schema\":\"magician.prove/1\",\"mod\":\"mod-a\",\"build\":\"6865\"") && out.contains(&format!("\"evidence_sha256\":\"{index_sha}\"")), "{out}");
    assert!(out.contains("\"proven\":[\"6865\"],\"unbound\":[\"6861\"]"), "{out}");
    let ledger = std::fs::read_to_string(d.join("proofs.tsv")).unwrap();
    let pak = PathBuf::from(&game).join("citadel").join("addons").join("pak10_dir.vpk");
    let bytes = std::fs::read(&pak).unwrap();
    let row = ledger.lines().find(|l| l.starts_with("6865\t")).unwrap_or_else(|| panic!("{ledger}"));
    let c: Vec<&str> = row.split('\t').collect();
    assert_eq!((c[3], c[4], c[5], c[6], c[8]), ("mod_a", "pak10_dir.vpk", format!("{:08x}", vpk::crc32(&bytes)).as_str(), sha256_hex(&bytes).as_str(), index_sha.as_str()), "{row}");
    assert!(c[7].ends_with("pass"), "{row}");
    assert!(std::fs::read_to_string(&toml).unwrap().contains("builds = [6861, 6865]  # filled by the harness"));
    // proving the same bytes again records nothing new
    let o = magician(&d, &["prove", &ts, "--game", &game, "--evidence", &pass.to_string_lossy(), "--date", "2026-11-24"]);
    assert!(text(&o).contains("of build 6865 (2026-11-24) recorded in"), "a new date is a new row of the same proof: {}", text(&o));
    let again = magician(&d, &["prove", &ts, "--game", &game, "--evidence", &pass.to_string_lossy(), "--date", "2026-11-24"]);
    assert!(text(&again).contains("was already in"), "{}", text(&again));
    // a release copy proven earlier, by its bytes: the installed pak is not that one
    let old = d.join("release").join("pak10_dir.vpk");
    std::fs::create_dir_all(old.parent().unwrap()).unwrap();
    vpk::write(&old, &[(CLIP.to_string(), b"the earlier clip".to_vec())]).unwrap();
    let o = magician(&d, &["prove", &ts, "--game", &game, "--build", "6861", "--date", "2026-11-18", "--pak", &old.to_string_lossy()]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    assert!(text(&o).contains("note: build 6861 is not the installed build 6865") && text(&o).contains("these bytes are proven on 6861"), "{}", text(&o));
    // the installed bytes are not proven on 6861 by it
    assert_eq!(magician(&d, &["baseline", "--game", &game]).status.code(), Some(0));
    let o = magician(&d, &["patchday", "--game", &game, "--mod", &ts]);
    assert!(String::from_utf8_lossy(&o.stdout).contains("its installed bytes proven on 1 of 1 builds on record (6865); 6861 (claimed for other bytes or never recorded)"), "{}", text(&o));
    // an evidence folder that is not there is a usage error, nothing written
    let before = std::fs::read_to_string(d.join("proofs.tsv")).unwrap();
    let o = magician(&d, &["prove", &ts, "--game", &game, "--evidence", &d.join("nope").to_string_lossy()]);
    assert_eq!(o.status.code(), Some(2), "{}", text(&o));
    assert_eq!(std::fs::read_to_string(d.join("proofs.tsv")).unwrap(), before);
    let _ = std::fs::remove_dir_all(&d);
}

fn sha256_hex(b: &[u8]) -> String {
    grade::sha256::hex(b)
}

#[test]
fn lint_and_drift_print_one_json_document() {
    let d = scratch("json");
    let game = fake_install(&d, "6864", b"valve clip").to_string_lossy().into_owned();
    let pak = PathBuf::from(&game).join("citadel").join("addons").join("pak10_dir.vpk");
    let o = magician(&d, &["lint", &pak.to_string_lossy(), "--game", &game, "--json"]);
    let out = String::from_utf8_lossy(&o.stdout).into_owned();
    assert!(out.starts_with("{\"schema\":\"magician.lint/1\",\"vpk\":\"pak10_dir.vpk\",\"build\":\"6864\""), "{}", text(&o));
    assert_eq!(out.trim_end().lines().count(), 1, "{out}");
    assert_eq!(o.status.code(), Some(if out.contains("\"errors\":0,") { 0 } else { 1 }), "the exit code follows the error count");
    assert_eq!(magician(&d, &["baseline", "--game", &game]).status.code(), Some(0));
    let o = magician(&d, &["drift", &pak.to_string_lossy(), "--from", "6864", "--game", &game, "--json"]);
    assert_eq!(o.status.code(), Some(0), "nothing changed: clean: {}", text(&o));
    assert!(String::from_utf8_lossy(&o.stdout).starts_with("{\"schema\":\"magician.drift/1\",\"from\":\"6864\",\"to\":\"6864\""));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_policy_comes_from_the_flag_the_environment_or_the_config_and_a_bad_one_is_a_usage_error() {
    let d = scratch("policy");
    let good = d.join("house.toml");
    std::fs::write(&good, "# a stricter house line\n[fair_play]\nhitbox_egregious_units = 15\n").unwrap();
    let bad = d.join("bad.toml");
    std::fs::write(&bad, "[fair_play]\nhitbox_units = 3\n").unwrap();
    let run = |args: &[&str], env: Option<&PathBuf>, config: Option<&PathBuf>| {
        let mut c = Command::new(env!("CARGO_BIN_EXE_magician"));
        c.args(args).env("MAGICIAN_REFERENCE", d.join("reference")).env("MAGICIAN_CONFIG", d.join("config.txt")).env_remove("MAGICIAN_POLICY");
        std::fs::write(d.join("config.txt"), config.map(|p| format!("policy = {}\n", p.display())).unwrap_or_default()).unwrap();
        if let Some(p) = env {
            c.env("MAGICIAN_POLICY", p);
        }
        c.output().unwrap()
    };
    // built-in: every key a default
    let o = run(&["policy"], None, None);
    let out = String::from_utf8_lossy(&o.stdout).into_owned();
    assert!(out.starts_with("policy\t-\tbuilt-in") && out.contains("fair_play.hitbox_egregious_units\t18\tdefault"), "{out}");
    // the flag, the environment and the config file each select it, in that order
    let g = good.to_string_lossy().into_owned();
    for (args, env, config, source) in [
        (vec!["policy", "--policy", g.as_str()], None, None, "--policy"),
        (vec!["policy"], Some(&good), None, "MAGICIAN_POLICY"),
        (vec!["policy"], None, Some(&good), "config (policy)"),
        (vec!["policy", "--policy", g.as_str()], Some(&bad), Some(&bad), "--policy"),
    ] {
        let o = run(&args, env, config);
        let out = String::from_utf8_lossy(&o.stdout).into_owned();
        assert_eq!(o.status.code(), Some(0), "{source}: {}", String::from_utf8_lossy(&o.stderr));
        assert!(out.contains(&format!("\t{source}\n")) && out.contains("fair_play.hitbox_egregious_units\t15\thouse.toml line 3"), "{source}: {out}");
    }
    // an unknown key is refused with its line and name, through any route, by every command that reads a policy
    for (args, env) in [(vec!["policy", "--policy", bad.to_str().unwrap()], None), (vec!["rubric"], Some(&bad)), (vec!["lint", "x.vpk"], Some(&bad))] {
        let o = run(&args, env, None);
        let err = String::from_utf8_lossy(&o.stderr).into_owned();
        assert_eq!(o.status.code(), Some(2), "{args:?}: {err}");
        assert!(err.contains("bad.toml line 2: unknown key fair_play.hitbox_units"), "{args:?}: {err}");
    }
    // a policy that names a file that is not there is refused, not ignored
    let o = run(&["policy"], Some(&d.join("missing.toml")), None);
    assert_eq!(o.status.code(), Some(2));
    let _ = std::fs::remove_dir_all(&d);
}

const EVTS: &str = "soundevents/hero/lash.vsndevts_c";

/// A compiled sound event file holding one event at `volume`.
fn events(volume: f64) -> Vec<u8> {
    let v = resource::kv3::parse_text(&format!("{{ Lash.A1.GroundStrike.Explosion = {{ base = \"Base.Ability\" volume = {volume} vsnd_files = [ ] vsnd_duration = 2.0 }} }}")).unwrap();
    resource::synth_with(&[], &[(*b"DATA", resource::kv3::encode(&v))])
}

#[test]
fn install_stops_on_fair_play_only_when_enforced_and_on_any_other_error_always() {
    let d = scratch("enforce");
    let game = fake_install(&d, "6864", b"valve clip");
    let citadel = game.join("citadel");
    // Valve's sound event, and a fair-play-only mod that plays it 7 dB quieter (M063: a weakened gameplay cue)
    vpk::write(citadel.join("pak01_dir.vpk"), &[(CLIP.to_string(), b"valve clip".to_vec()), (EVTS.to_string(), events(3.0))]).unwrap();
    let quiet = d.join("pak20_dir.vpk");
    vpk::write(&quiet, &[(EVTS.to_string(), events(-4.0))]).unwrap();
    // and a mod whose only error is not fair play: a material naming a texture nobody ships
    let broken = d.join("pak21_dir.vpk");
    vpk::write(&broken, &[("materials/x/broken.vmat_c".to_string(), resource::synth(&["materials/x/nowhere_color.vtex"]))]).unwrap();
    let strict = d.join("strict.toml");
    std::fs::write(&strict, "[fair_play]\nenforce = true\n").unwrap();
    let g = game.to_string_lossy().into_owned();
    let run = |args: &[&str], policy: Option<&PathBuf>| {
        let mut a: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        a.extend(["--game".into(), g.clone(), "--ignore-game".into()]);
        if let Some(p) = policy {
            a.extend(["--policy".into(), p.to_string_lossy().into_owned()]);
        }
        let a: Vec<&str> = a.iter().map(String::as_str).collect();
        magician(&d, &a)
    };
    let installed = |name: &str| citadel.join("addons").join(name).is_file();
    let q = quiet.to_string_lossy().into_owned();

    // lenient (the default): the M063 is a warning; lint is clean and install goes ahead
    let o = run(&["lint", &q], None);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    assert!(text(&o).contains("warn  M063"), "{}", text(&o));
    let o = run(&["install", &q, "--mod", "quiet"], None);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    assert!(installed("pak20_dir.vpk"));
    assert_eq!(run(&["uninstall", "pak20_dir.vpk"], None).status.code(), Some(0));
    assert!(!installed("pak20_dir.vpk"));

    // enforced: the same finding is an error; lint exits 1 and install refuses with nothing changed
    let o = run(&["lint", &q], Some(&strict));
    assert_eq!(o.status.code(), Some(1), "{}", text(&o));
    assert!(text(&o).contains("error M063"), "{}", text(&o));
    let o = run(&["install", &q, "--mod", "quiet"], Some(&strict));
    assert_eq!(o.status.code(), Some(3), "{}", text(&o));
    assert!(!installed("pak20_dir.vpk"), "a refusal installs nothing");

    // an error that is not fair play is refused under either policy
    let b = broken.to_string_lossy().into_owned();
    for policy in [None, Some(&strict)] {
        let o = run(&["install", &b, "--mod", "broken"], policy);
        assert_eq!(o.status.code(), Some(3), "{policy:?}: {}", text(&o));
        assert!(!installed("pak21_dir.vpk"));
    }
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn an_addon_is_one_folder_name_and_its_compiler_marker_a_bin_of_the_csdk() {
    let d = scratch("addon-names");
    let csdk = d.join("csdk");
    std::fs::create_dir_all(csdk.join("content").join("citadel_addons").join("a")).unwrap();
    std::fs::create_dir_all(csdk.join("game").join("citadel_addons")).unwrap();
    std::fs::create_dir_all(csdk.join("game").join("bin").join("win64")).unwrap();
    std::fs::create_dir_all(d.join("elsewhere")).unwrap();
    let c = csdk.to_string_lossy().into_owned();
    let out = d.join("o").join("pak10_dir.vpk").to_string_lossy().into_owned();
    let elsewhere = d.join("elsewhere").to_string_lossy().into_owned();
    // an unset variable, a path out of the CSDK, a walk up it: refused before a lock or log is written anywhere
    for addon in ["", "..", elsewhere.as_str(), "a/../..", r"a\..\..", "no/such"] {
        let o = magician(&d, &["build", addon, &out, "--csdk", &c, "--no-compile"]);
        assert_eq!(o.status.code(), Some(2), "build {addon:?}: {}", text(&o));
        assert!(text(&o).contains("an addon is one folder name"), "build {addon:?}: {}", text(&o));
        // sign takes a folder by path on purpose; a name that is not one is still one folder of the CSDK
        if !std::path::Path::new(addon).is_dir() {
            let o = magician(&d, &["sign", addon, "--csdk", &c, "--dry-run"]);
            assert_eq!(o.status.code(), Some(2), "sign {addon:?}: {}", text(&o));
            assert!(text(&o).contains("an addon is one folder name"), "sign {addon:?}: {}", text(&o));
        }
    }
    assert!(!csdk.join("game").join("citadel_addons").join(".magician-build.lock").exists() && !d.join("elsewhere").join(".magician-build.lock").exists());
    // the addon's own .magician-compiler can name a bin folder of the CSDK, never a path to an executable elsewhere
    std::fs::write(csdk.join("content").join("citadel_addons").join("a").join(".magician-compiler"), &elsewhere).unwrap();
    let o = magician(&d, &["build", "a", &out, "--csdk", &c, "--no-compile"]);
    assert_eq!(o.status.code(), Some(2), "{}", text(&o));
    assert!(text(&o).contains("must be the name of a folder in the CSDK"), "{}", text(&o));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_release_whose_grade_cannot_run_says_so_and_never_reports_an_old_one() {
    let d = scratch("release-sub");
    let game = fake_install(&d, "6864", b"valve clip").to_string_lossy().into_owned();
    let csdk = d.join("csdk");
    let src = csdk.join("content").join("citadel_addons").join("a").join("particles");
    let built = csdk.join("game").join("citadel_addons").join("a").join("particles");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::create_dir_all(&built).unwrap();
    std::fs::create_dir_all(csdk.join("game").join("bin").join("win64")).unwrap();
    std::fs::write(src.join("x.vpcf"), "<!-- kv3 encoding:text:version{e21c7f3c-8a33-41c5-9977-a76d3a32aa0d} -->\n{ }\n").unwrap();
    std::fs::write(built.join("x.vpcf_c"), resource::synth(&[])).unwrap();
    let out = d.join("out");
    std::fs::create_dir_all(&out).unwrap();
    // a previous release's record beside the pak
    std::fs::write(out.join("grade.json"), "{\"schema\":\"magician.grade/3\",\"label\":\"an old grade\"}\n").unwrap();
    let pak = out.join("pak10_dir.vpk").to_string_lossy().into_owned();
    let o = magician(&d, &["release", "a", "--hero", "hero_lash", "--csdk", &csdk.to_string_lossy(), "--no-compile", "--out", &pak, "--game", &game, "--json"]);
    // there is no Source 2 Viewer here, so grade cannot read the hero: the release says that, exit 2
    assert_eq!(o.status.code(), Some(2), "{}", text(&o));
    assert!(text(&o).contains("`magician grade` did not run (exit 2)"), "{}", text(&o));
    assert!(!String::from_utf8_lossy(&o.stdout).contains("an old grade") && !out.join("grade.json").exists(), "the old record is gone, not passed off as this run's");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn scene_make_with_an_empty_out_touches_nothing_in_the_working_folder() {
    let d = scratch("scene-empty-out");
    let valve = scene(&[(".vmap_c", "map"), ("/entities/default_ents.vents_c", "valve lump")]);
    let game = fake_game(&d, &valve);
    let ours = d.join("lash.vpk");
    vpk::write(&ours, &scene(&[(".vmap_c", "map"), ("/entities/default_ents.vents_c", "OUR lump")])).unwrap();
    let work = d.join("work");
    std::fs::create_dir_all(work.join("files")).unwrap();
    std::fs::write(work.join("files").join("keep.txt"), "someone's").unwrap();
    let o = Command::new(env!("CARGO_BIN_EXE_magician"))
        .args(["scene", "make", &ours.to_string_lossy(), "--out", "", "--game", &game.to_string_lossy()])
        .current_dir(&work)
        .env("MAGICIAN_REFERENCE", d.join("reference"))
        .env("MAGICIAN_CONFIG", d.join("config.txt"))
        .env_remove("MAGICIAN_POLICY")
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(2), "{}", text(&o));
    assert!(text(&o).contains("--out names no folder"), "{}", text(&o));
    assert!(work.join("files").join("keep.txt").exists() && !work.join("scene-patch.txt").exists());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn json_is_one_document_even_when_the_command_stops() {
    let d = scratch("json-errors");
    for (args, code) in [(&["drift", "x.vpk", "--from", "1", "--json"][..], 2), (&["lint", "x.vpk", "--deeep", "--json"], 2), (&["lint", "x.vpk", "--game", "--json"], 2), (&["grade", "x.vpk", "--json"], 2)] {
        let o = magician(&d, args);
        let out = String::from_utf8_lossy(&o.stdout).into_owned();
        assert_eq!(o.status.code(), Some(code), "{args:?}: {}", text(&o));
        assert_eq!(out.trim_end().lines().count(), 1, "{args:?}: one document: {out}");
        assert!(out.starts_with("{\"schema\":\"magician.error/1\",\"command\":") && out.contains(&format!("\"exit\":{code},\"kind\":\"usage\",\"error\":\"")), "{args:?}: {out}");
        let err = String::from_utf8_lossy(&o.stderr);
        assert!(err.starts_with("magician: "), "the reason is on stderr as before: {err}");
    }
    // without --json, stdout stays empty on a stop
    assert!(magician(&d, &["drift", "x.vpk", "--from", "1"]).stdout.is_empty());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn patchday_follows_a_pak_a_mod_manager_renamed() {
    let d = scratch("patchday-renamed");
    let game = fake_install(&d, "6864", b"valve clip");
    assert_eq!(magician(&d, &["baseline", "--game", &game.to_string_lossy()]).status.code(), Some(0));
    let toml = d.join("mod.toml");
    std::fs::write(&toml, "[mod]\nslug = \"mod-a\"\n[proven]\nbuilds = []\n").unwrap();
    assert_eq!(magician(&d, &["prove", &toml.to_string_lossy(), "--game", &game.to_string_lossy()]).status.code(), Some(0));
    // Deadlock Mod Manager renumbers the addons folder: our pak10 is pak03 now, same bytes, so the same proof
    let addons = game.join("citadel").join("addons");
    std::fs::rename(addons.join("pak10_dir.vpk"), addons.join("pak03_dir.vpk")).unwrap();
    let o = magician(&d, &["patchday", "--game", &game.to_string_lossy(), "--mod", &toml.to_string_lossy(), "--json"]);
    let out = String::from_utf8_lossy(&o.stdout).into_owned();
    assert_eq!(o.status.code(), Some(0), "the pak is there, under another name: {}", text(&o));
    assert!(out.contains("{\"name\":\"pak03_dir.vpk\",") && out.contains("\"state\":\"installed\",\"moved_from\":\"pak10_dir.vpk\"}"), "{out}");
    // read only: the ledger keeps its name until the next install or uninstall
    assert!(std::fs::read_to_string(addons.join(".magician-installed.tsv")).unwrap().starts_with("pak10_dir.vpk\t"));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn patchday_reports_a_pak_that_does_not_read_as_broken_and_finishes() {
    let d = scratch("patchday-damaged");
    let game = fake_install(&d, "6864", b"valve clip");
    assert_eq!(magician(&d, &["baseline", "--game", &game.to_string_lossy()]).status.code(), Some(0));
    std::fs::write(game.join("citadel").join("addons").join("pak10_dir.vpk"), b"not a pak any more").unwrap();
    let o = magician(&d, &["patchday", "--game", &game.to_string_lossy(), "--json"]);
    let out = String::from_utf8_lossy(&o.stdout).into_owned();
    assert_eq!(o.status.code(), Some(1), "broken, not a usage error: {}", text(&o));
    assert!(out.starts_with("{\"schema\":\"magician.patchday/1\"") && out.contains("\"verdict\":{\"state\":\"broken\""), "{out}");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn patchday_with_no_baseline_says_so_in_one_document_and_writes_nothing_without_capture() {
    let d = scratch("patchday-nobase");
    let game = fake_install(&d, "6864", b"valve clip");
    let o = magician(&d, &["patchday", "--game", &game.to_string_lossy(), "--json"]);
    let out = String::from_utf8_lossy(&o.stdout).into_owned();
    assert_eq!(o.status.code(), Some(1), "{}", text(&o));
    assert_eq!(out.trim_end().lines().count(), 1, "{out}");
    assert!(out.contains("\"state\":\"missing\"") && out.contains("\"verdict\":{\"state\":\"broken\""), "{out}");
    assert!(!d.join("reference").join("6864").join("baseline.tsv").exists(), "a check without --capture writes no baseline");
    let o = magician(&d, &["patchday", "--game", &game.to_string_lossy(), "--capture", "--json"]);
    assert!(d.join("reference").join("6864").join("baseline.tsv").is_file(), "{}", text(&o));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn global_options_may_come_before_the_command() {
    let d = scratch("global-first");
    let o = magician(&d, &["--no-cache", "rules", "--json"]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    assert!(String::from_utf8_lossy(&o.stdout).starts_with("{\"schema\":\"magician.rules/1\""));
    let r = d.join("elsewhere").to_string_lossy().into_owned();
    let o = magician(&d, &["--reference", &r, "paths"]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    assert!(String::from_utf8_lossy(&o.stdout).contains(&format!("reference\t{r}\t")), "{}", text(&o));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_reader_that_stops_reading_does_not_crash_the_command() {
    use std::io::BufRead;
    let d = scratch("pipe");
    // far more listing than a pipe buffers, so the command is still writing when the reader leaves
    let files: Vec<(String, Vec<u8>)> = (0..20000).map(|i| (format!("materials/many/file_{i:05}.vmat_c"), vec![b'x'])).collect();
    let pak = d.join("pak10_dir.vpk");
    vpk::write(&pak, &files).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_magician"))
        .args(["ls", &pak.to_string_lossy()])
        .env("MAGICIAN_REFERENCE", d.join("reference"))
        .env("MAGICIAN_CONFIG", d.join("config.txt"))
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut first = String::new();
    std::io::BufReader::new(child.stdout.take().unwrap()).read_line(&mut first).unwrap();
    assert!(first.starts_with("materials/many/file_00000.vmat_c"), "{first}");
    // the reader is dropped here, as `| head -1` closes the pipe
    let o = child.wait_with_output().unwrap();
    let err = String::from_utf8_lossy(&o.stderr).into_owned();
    assert_eq!(o.status.code(), Some(0), "the command's own exit code, not a panic's: {err}");
    assert!(!err.contains("panicked") && !err.contains("failed printing"), "{err}");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn an_output_file_that_cannot_be_written_fails_the_command() {
    let d = scratch("jsonout");
    let game = fake_install(&d, "6864", b"valve clip").to_string_lossy().into_owned();
    let pak = PathBuf::from(&game).join("citadel").join("addons").join("pak10_dir.vpk").to_string_lossy().into_owned();
    let nowhere = d.join("no-such-folder").join("lint.json").to_string_lossy().into_owned();
    for flag in ["--json-out", "--findings-out"] {
        let o = magician(&d, &["lint", &pak, "--game", &game, flag, &nowhere]);
        assert_eq!(o.status.code(), Some(2), "{flag}: {}", text(&o));
        assert!(text(&o).contains(&format!("magician: {flag} {nowhere}: could not write it")), "{flag}: {}", text(&o));
    }
    let ok = d.join("lint.json");
    let o = magician(&d, &["lint", &pak, "--game", &game, "--json-out", &ok.to_string_lossy()]);
    assert!(o.status.code() != Some(2) && std::fs::read_to_string(&ok).unwrap().starts_with("{\"schema\":\"magician.lint/1\""), "{}", text(&o));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn the_first_run_on_a_build_captures_its_baseline_only_for_this_machines_own_game() {
    let d = scratch("autobase");
    let game = fake_install(&d, "6870", b"valve clip");
    let pak = game.join("citadel").join("addons").join("pak10_dir.vpk").to_string_lossy().into_owned();
    let baseline = d.join("reference").join("6870").join("baseline.tsv");
    // a game named by the environment (a drill's copy, often): read live, nothing written
    let o = magician(&d, &["lint", &pak, "--game", &game.to_string_lossy()]);
    assert!(o.status.code() != Some(2) && !baseline.exists(), "{}", text(&o));
    // the game the config file names is this machine's own: its first run on the build captures the baseline
    std::fs::write(d.join("config.txt"), format!("game = {}\n", game.display())).unwrap();
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_magician"))
            .args(["lint", &pak])
            .env("MAGICIAN_REFERENCE", d.join("reference"))
            .env("MAGICIAN_CONFIG", d.join("config.txt"))
            .env_remove("DEADLOCK_GAME")
            .env("MAGICIAN_NO_CACHE", "1")
            .output()
            .unwrap()
    };
    let o = run();
    assert!(o.status.code() != Some(2), "{}", text(&o));
    assert!(baseline.is_file() && baseline.with_file_name("rig.tsv").is_file(), "{}", text(&o));
    assert!(String::from_utf8_lossy(&o.stderr).contains("first run on build 6870: captured its baseline"), "{}", text(&o));
    // and only the first
    let o = run();
    assert!(!String::from_utf8_lossy(&o.stderr).contains("captured"), "{}", text(&o));
    let _ = std::fs::remove_dir_all(&d);
}

/// The binary run from `cwd`, with the scratch reference, config and game of `d`.
fn magician_in(d: &Path, cwd: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_magician"))
        .args(args)
        .current_dir(cwd)
        .env("MAGICIAN_REFERENCE", d.join("reference"))
        .env("MAGICIAN_CONFIG", d.join("config.txt"))
        .env("DEADLOCK_GAME", d.join("game"))
        .env("MAGICIAN_NO_CACHE", "1")
        .env_remove("MAGICIAN_POLICY")
        .output()
        .unwrap()
}

const HEROES_VDATA: &str = "{ hero_base = { } hero_x = { m_bDisabled = false m_strModelName = \"models/heroes/x/x.vmdl\" } hero_lash = { m_strModelName = \"models/heroes_wip/lash/lash.vmdl\" } }";

/// `fake_install` with a heroes.vdata in Valve's archive, so heroes can be named and inferred.
fn fake_install_with_heroes(root: &Path, build: &str) -> PathBuf {
    let game = fake_install(root, build, b"valve clip");
    let heroes = resource::synth_with(&[], &[(*b"DATA", resource::kv3::encode(&resource::kv3::parse_text(HEROES_VDATA).unwrap()))]);
    vpk::write(game.join("citadel").join("pak01_dir.vpk"), &[(CLIP.to_string(), b"valve clip".to_vec()), ("models/heroes/x/x.vmdl_c".to_string(), b"valve model".to_vec()), ("scripts/heroes.vdata_c".to_string(), heroes)]).unwrap();
    game
}

#[test]
fn new_scaffolds_a_mod_from_the_games_own_data_and_never_over_a_folder() {
    let d = scratch("new");
    let game = fake_install_with_heroes(&d, "6868");
    // the hero's compiled model names its skeleton, as Valve's do
    let heroes = resource::synth_with(&[], &[(*b"DATA", resource::kv3::encode(&resource::kv3::parse_text(HEROES_VDATA).unwrap()))]);
    let model = resource::synth_with(&[], &[(*b"DATA", resource::kv3::encode(&resource::kv3::parse_text("{ m_vecNmSkeletonRefs = [ \"models/heroes/x/x.vnmskel\" ] }").unwrap()))]);
    vpk::write(game.join("citadel").join("pak01_dir.vpk"), &[(CLIP.to_string(), b"valve clip".to_vec()), ("models/heroes/x/x.vmdl_c".to_string(), model), ("scripts/heroes.vdata_c".to_string(), heroes)]).unwrap();
    let mods = d.join("mods");
    std::fs::create_dir_all(&mods).unwrap();
    let ms = mods.to_string_lossy().into_owned();
    let o = magician_in(&d, &d, &["new", "x", "--dir", &ms, "--author", "someone"]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    let m = mods.join("x-skin");
    let out = String::from_utf8_lossy(&o.stdout).into_owned();
    assert!(out.contains("skeleton models/heroes/x/x.vnmskel (from the compiled model)") && out.contains("magician release --out pak10_dir.vpk"), "{out}");
    let toml = std::fs::read_to_string(m.join("mod.toml")).unwrap();
    assert!(toml.contains("model    = \"models/heroes/x/x.vmdl\"") && toml.contains("skeleton = \"models/heroes/x/x.vnmskel\"") && toml.contains("hero    = \"hero_x\""), "{toml}");
    for f in ["README.md", ".gitignore", "addon/.gitignore", "addon/magician.allow", "addon/models/heroes/x/.gitignore", "addon/particles/abilities/.gitignore", "addon/sounds/.gitignore", "addon/panorama/images/.gitignore", "package/bundle.json", "package/README.template.md", "package/CHANGELOG.md", "source/ingame/scenarios/enter_x.txt", "source/ingame/scenarios/x_angles.txt"] {
        assert!(m.join(f).is_file(), "no {f}");
    }
    // nothing of Valve's: every file it wrote is text it made
    let mut all = Vec::new();
    fn walk(p: &Path, out: &mut Vec<PathBuf>) {
        for e in std::fs::read_dir(p).unwrap().flatten() {
            if e.path().is_dir() { walk(&e.path(), out) } else { out.push(e.path()) }
        }
    }
    walk(&m, &mut all);
    assert!(all.iter().all(|p| std::str::from_utf8(&std::fs::read(p).unwrap()).is_ok() && !p.to_string_lossy().ends_with("_c")), "{all:?}");
    // never over a folder that is there, and never inside a mod
    let o = magician_in(&d, &d, &["new", "x", "--dir", &ms]);
    assert_eq!(o.status.code(), Some(3), "{}", text(&o));
    assert_eq!(magician_in(&d, &m, &["new", "x", "--slug", "other"]).status.code(), Some(3), "not inside a mod");
    assert_eq!(magician_in(&d, &d, &["new", "nobody", "--dir", &ms]).status.code(), Some(2), "a hero the game does not have");
    // `magician` and `check` in it say plainly there is no pak yet
    let o = magician_in(&d, &m, &[]);
    assert!(text(&o).contains("no pak yet") && text(&o).contains("magician release --out pak10_dir.vpk"), "{}", text(&o));
    let o = magician_in(&d, &m, &["check", "--light"]);
    assert_eq!(o.status.code(), Some(2), "{}", text(&o));
    assert!(text(&o).contains("x-skin has no pak yet, so there is nothing to check"), "{}", text(&o));
    // once there is a pak, check reads it, and tools/bundle.py accepts the package as written
    vpk::write(m.join("pak10_dir.vpk"), &[("models/heroes/x/materials/x_body.vmat_c".to_string(), resource::synth(&[]))]).unwrap();
    let o = magician_in(&d, &m, &["check", "--light"]);
    assert!(matches!(o.status.code(), Some(0 | 1 | 4)) && String::from_utf8_lossy(&o.stdout).starts_with("x-skin: "), "{}", text(&o));
    let bundle = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("tools").join("bundle.py");
    match Command::new(python().unwrap_or("python")).arg("-I").arg(&bundle).arg(m.join("package").join("bundle.json")).args(["--no-valve-check", "--magician", env!("CARGO_BIN_EXE_magician")]).env("DEADLOCK_GAME", &game).output() {
        Ok(o) => {
            assert_eq!(o.status.code(), Some(0), "bundle.py: {}", text(&o));
            assert!(m.join("dist").join("x-skin").join("mod-interchange.json").is_file());
        }
        Err(e) => eprintln!("skipped the bundle check: no python ({e})"),
    }
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn session_bundle_and_source_hand_everything_to_the_toolkits_python() {
    let d = scratch("frontdoor");
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
    let run = |args: &[&str], envs: &[(&str, &Path)]| {
        let mut c = Command::new(env!("CARGO_BIN_EXE_magician"));
        c.args(args).current_dir(&d).env_remove("MAGICIAN_HOME").env_remove("MAGICIAN_PYTHON").env("MAGICIAN_CONFIG", d.join("config.txt"));
        for (k, v) in envs {
            c.env(k, v);
        }
        c.output().unwrap()
    };
    // the toolkit and Python not there: exit 2, saying what is missing and how to point at it
    let o = run(&["bundle", "x.json"], &[("MAGICIAN_HOME", &d)]);
    assert_eq!(o.status.code(), Some(2), "{}", text(&o));
    assert!(text(&o).contains("holds no ingame/mod_session.py: point MAGICIAN_HOME"), "{}", text(&o));
    let o = run(&["session", "qa"], &[("MAGICIAN_HOME", &repo), ("MAGICIAN_PYTHON", &d.join("no-python.exe"))]);
    assert_eq!(o.status.code(), Some(2), "{}", text(&o));
    assert!(text(&o).contains("does not run as Python 3: point MAGICIAN_PYTHON"), "{}", text(&o));
    if python().is_none() {
        eprintln!("skipped the pass-through runs: no python");
        let _ = std::fs::remove_dir_all(&d);
        return;
    }
    // Found from the executable's own checkout when cargo built it inside one; a build elsewhere (CARGO_TARGET_DIR)
    // finds nothing above itself, and says so, and MAGICIAN_HOME points it at the toolkit. The tool's --help is the
    // tool's, with its exit code.
    let exe = std::fs::canonicalize(env!("CARGO_BIN_EXE_magician")).unwrap();
    let inside = exe.starts_with(std::fs::canonicalize(&repo).unwrap());
    if !inside {
        let o = run(&["bundle", "--help"], &[]);
        assert_eq!(o.status.code(), Some(2), "{}", text(&o));
        assert!(text(&o).contains("holds ingame/mod_session.py") && text(&o).contains("set MAGICIAN_HOME"), "{}", text(&o));
    }
    let home: &[(&str, &Path)] = if inside { &[] } else { &[("MAGICIAN_HOME", &repo)] };
    let run = |args: &[&str], envs: &[(&str, &Path)]| run(args, &[home, envs].concat());
    let o = run(&["bundle", "--help"], &[]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    assert!(String::from_utf8_lossy(&o.stdout).starts_with("Assemble a release bundle"), "{}", text(&o));
    let o = run(&["source", "-h"], &[("MAGICIAN_HOME", &repo)]);
    assert!(o.status.code() == Some(0) && String::from_utf8_lossy(&o.stdout).contains("Deadlock Mod Manager mod source"), "{}", text(&o));
    // the arguments arrive as given (a path with a space), and a failing tool's exit code is the verb's
    let o = run(&["bundle", "a spec.json", "--zip"], &[]);
    assert_eq!(o.status.code(), Some(1), "{}", text(&o));
    assert!(String::from_utf8_lossy(&o.stderr).contains("a spec.json"), "{}", text(&o));
    // --json after the verb is the tool's, not magician's
    let o = run(&["source", "--json"], &[]);
    assert!(!String::from_utf8_lossy(&o.stdout).contains("magician.error/1"), "{}", text(&o));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_mod_folder_needs_nothing_after_the_verb() {
    let d = scratch("modfolder");
    let game = fake_install_with_heroes(&d, "6864");
    // a modder's folder with one pak in it, and nothing else
    let m = d.join("x-skin");
    std::fs::create_dir_all(&m).unwrap();
    vpk::write(m.join("pak33_dir.vpk"), &[("models/heroes/x/materials/x_body.vmat_c".to_string(), resource::synth(&[]))]).unwrap();
    // without a mod.toml, a bare verb says what it needs and how to get it
    let o = magician_in(&d, &m, &["lint"]);
    assert_eq!(o.status.code(), Some(2), "{}", text(&o));
    assert!(text(&o).contains("no mod.toml in this folder or above it") && text(&o).contains("magician init"), "{}", text(&o));
    // init writes one: the hero from the files the pak replaces, the slug from the folder
    let o = magician_in(&d, &m, &["init"]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    let toml = std::fs::read_to_string(m.join("mod.toml")).unwrap();
    assert!(toml.contains("slug = \"x-skin\"") && toml.contains("hero = \"hero_x\"") && toml.contains("paks = [\"pak33_dir.vpk\"]"), "{toml}");
    assert!(text(&o).contains("hero_x (x): 1 of 1 files are its own"), "{}", text(&o));
    assert_eq!(magician_in(&d, &m, &["init"]).status.code(), Some(3), "never over an existing mod.toml");
    // from a folder below it, lint finds the mod and says what it used
    std::fs::create_dir_all(m.join("notes")).unwrap();
    let o = magician_in(&d, &m.join("notes"), &["lint"]);
    assert_ne!(o.status.code(), Some(2), "{}", text(&o));
    let err = String::from_utf8_lossy(&o.stderr).into_owned();
    assert!(err.contains("magician: x-skin: ") && err.contains("pak33_dir.vpk ([mod] paks in mod.toml)"), "{err}");
    assert!(String::from_utf8_lossy(&o.stdout).contains("pak33_dir.vpk"), "{}", text(&o));
    // an explicit pak wins
    let other = d.join("pak40_dir.vpk");
    vpk::write(&other, &[("materials/x/y.vmat_c".to_string(), b"m".to_vec())]).unwrap();
    let o = magician_in(&d, &m, &["lint", &other.to_string_lossy()]);
    assert!(String::from_utf8_lossy(&o.stdout).contains("pak40_dir.vpk") && !String::from_utf8_lossy(&o.stderr).contains("pak33"), "{}", text(&o));
    // a build the summary claims, with no proof of these bytes: check says so, and drift has no older era to use
    std::fs::write(m.join("mod.toml"), toml.replace("builds = []", "builds = [6864]")).unwrap();
    assert_eq!(magician(&d, &["baseline", "--game", &game.to_string_lossy()]).status.code(), Some(0));
    let o = magician_in(&d, &m, &["check", "--light"]);
    assert_eq!(o.status.code(), Some(4), "{}", text(&o));
    assert!(String::from_utf8_lossy(&o.stdout).contains("these bytes proven on none; 6864 (claimed for other bytes or never recorded)"), "{}", text(&o));
    // drift compares from the last build a proof holds these paks on
    let o = magician_in(&d, &m, &["prove", "--pak", &m.join("pak33_dir.vpk").to_string_lossy()]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    let o = magician_in(&d, &m, &["drift", "--json"]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    assert!(String::from_utf8_lossy(&o.stdout).starts_with("{\"schema\":\"magician.drift/1\",\"from\":\"6864\""), "{}", text(&o));
    // check: one verdict; proven on the installed build and clean is 0
    let o = magician_in(&d, &m, &["check", "--light"]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    let out = String::from_utf8_lossy(&o.stdout).into_owned();
    assert!(out.starts_with("x-skin: ok") && out.contains("proven on 6864, the installed build"), "{out}");
    let o = magician_in(&d, &d, &["check", &m.to_string_lossy(), "--light", "--json"]);
    let out = String::from_utf8_lossy(&o.stdout).into_owned();
    assert!(out.starts_with("{\"schema\":\"magician.check/1\"") && out.contains("\"verdict\":\"ok\"") && out.trim_end().lines().count() == 1, "{}", text(&o));
    // install puts the mod's own pak in under its slug, through every check install makes
    let o = magician_in(&d, &m, &["install", "--ignore-game"]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    let ledger = std::fs::read_to_string(game.join("citadel").join("addons").join(".magician-installed.tsv")).unwrap();
    assert!(ledger.contains("pak33_dir.vpk\t") && ledger.contains("\tx_skin"), "{ledger}");
    // and uninstall takes out what the ledger holds for it, and only that
    let o = magician_in(&d, &m, &["uninstall", "--ignore-game"]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    let ledger = std::fs::read_to_string(game.join("citadel").join("addons").join(".magician-installed.tsv")).unwrap();
    assert!(!ledger.contains("x_skin") && ledger.contains("mod_a"), "{ledger}");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn magician_alone_says_where_things_stand_and_what_to_run() {
    let d = scratch("orient");
    let o = magician_in(&d, &d, &[]);
    assert_eq!(o.status.code(), Some(2), "a command that came out empty is not a command that ran");
    let err = String::from_utf8_lossy(&o.stderr).into_owned();
    assert!(o.stdout.is_empty() && err.contains("game  not found") && err.contains("magician doctor") && err.contains("magician init <pak>") && err.contains("magician help"), "{err}");
    assert!(err.lines().count() < 20, "a screen, not the reference: {err}");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn doctor_says_what_is_missing_and_how_to_fix_it() {
    let d = scratch("doctor");
    // no game: a failure, with the fix
    let o = magician_in(&d, &d, &["doctor"]);
    assert_eq!(o.status.code(), Some(1), "{}", text(&o));
    let out = String::from_utf8_lossy(&o.stdout).into_owned();
    assert!(out.starts_with("magician doctor: 1 failing") && out.contains("FAIL  game") && out.contains("-> pass --game"), "{out}");
    // a game with no baseline yet: ready to work, with the baseline to do
    let game = fake_install_with_heroes(&d, "6864");
    let o = magician_in(&d, &d, &["doctor", "--json"]);
    let out = String::from_utf8_lossy(&o.stdout).into_owned();
    assert_eq!(o.status.code(), Some(0), "{out}");
    assert!(out.starts_with("{\"schema\":\"magician.doctor/1\"") && out.contains("{\"name\":\"baseline\",\"state\":\"todo\"") && out.contains("\"fix\":\"magician baseline"), "{out}");
    assert!(!d.join("reference").join("6864").exists(), "doctor reads only");
    assert_eq!(magician(&d, &["baseline", "--game", &game.to_string_lossy()]).status.code(), Some(0));
    let out = String::from_utf8_lossy(&magician_in(&d, &d, &["doctor"]).stdout).into_owned();
    assert!(out.contains("ok    baseline"), "{out}");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_hero_is_named_any_way_and_a_misspelt_one_is_answered() {
    let d = scratch("heroname");
    fake_install_with_heroes(&d, "6864");
    let o = magician_in(&d, &d, &["footprint", "lahs"]);
    assert_eq!(o.status.code(), Some(2), "{}", text(&o));
    assert!(text(&o).contains("no hero \"lahs\" in this build (did you mean hero_lash (lash)?)"), "{}", text(&o));
    let _ = std::fs::remove_dir_all(&d);
}

/// The maker's mark is signed under the mod's own author, never a built-in name: `--key`, else `[mark] key`, else
/// `[mod] author`, else the key the addon was signed with before; with none, sign and verify refuse and say how.
#[test]
fn the_mark_key_comes_from_the_mod_and_there_is_no_default() {
    let d = scratch("markkey");
    let m = d.join("example-skin");
    let addon = m.join("addon");
    std::fs::create_dir_all(addon.join("materials")).unwrap();
    let a = addon.to_string_lossy().into_owned();
    let key_line = |o: &Output| String::from_utf8_lossy(&o.stdout).lines().next().unwrap_or("").to_string();

    std::fs::write(m.join("mod.toml"), "[mod]\nslug = \"example-skin\"\n").unwrap();
    let o = magician(&d, &["sign", &a, "--dry-run"]);
    assert_eq!(o.status.code(), Some(2), "{}", text(&o));
    assert!(text(&o).contains("no maker's mark key") && text(&o).contains("[mark]") && text(&o).contains("author"), "{}", text(&o));

    std::fs::write(m.join("mod.toml"), "[mod]\nslug = \"example-skin\"\nauthor = \"Jane Example\"\n").unwrap();
    let o = magician(&d, &["sign", &a, "--dry-run"]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    assert!(key_line(&o).contains("key Jane Example,"), "{}", text(&o));
    assert!(String::from_utf8_lossy(&o.stderr).contains("[mod] author"), "where the key came from is said: {}", text(&o));

    std::fs::write(m.join("mod.toml"), "[mod]\nslug = \"example-skin\"\nauthor = \"Jane Example\"\n[mark]\nkey = \"jx\"\n").unwrap();
    let o = magician(&d, &["sign", &a, "--dry-run"]);
    assert!(key_line(&o).contains("key jx,"), "[mark] key wins over the author: {}", text(&o));
    let o = magician(&d, &["sign", &a, "--dry-run", "--key", "given"]);
    assert!(key_line(&o).contains("key given,"), "--key wins: {}", text(&o));

    // a pak with the mod.toml beside it, or none
    let pak = m.join("pak33_dir.vpk");
    vpk::write(&pak, &[("materials/x.txt".to_string(), b"x".to_vec())]).unwrap();
    let o = magician(&d, &["verify", &pak.to_string_lossy()]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    assert!(key_line(&o).contains("key jx,"), "{}", text(&o));
    let lone = d.join("lone");
    std::fs::create_dir_all(&lone).unwrap();
    std::fs::copy(&pak, lone.join("pak33_dir.vpk")).unwrap();
    let o = Command::new(env!("CARGO_BIN_EXE_magician")).args(["verify", &lone.join("pak33_dir.vpk").to_string_lossy()]).current_dir(&lone)
        .env("MAGICIAN_REFERENCE", d.join("reference")).env("MAGICIAN_CONFIG", d.join("config.txt")).env("DEADLOCK_GAME", d.join("no-game")).output().unwrap();
    assert_eq!(o.status.code(), Some(2), "{}", text(&o));
    assert!(text(&o).contains("no maker's mark key"), "{}", text(&o));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn update_check_exits_5_for_an_available_update_and_nothing_else_does() {
    let d = scratch("update-exit");
    // the exit codes are documented, 5 included, wherever a script looks for them
    for args in [&["--help"][..], &["help", "update"]] {
        let o = magician(&d, args);
        assert!(String::from_utf8_lossy(&o.stdout).contains("5 an update is available"), "{args:?}: {}", text(&o));
    }
    let me = String::from_utf8(magician(&d, &["update", "--describe"]).stdout).unwrap();
    let release = |name: &str, text: &str| {
        let dir = d.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("VERSION"), text).unwrap();
        dir.to_string_lossy().into_owned()
    };
    let check = |src: &str| magician(&d, &["update", "--check", "--source", src]).status.code();
    // a build that could not read its own commit (git missing, or a worktree whose git folder this system can't
    // open) can't be told from the release, which counts as an update (update::compare)
    let unknown = me.lines().any(|l| l.trim() == "commit = unknown");
    assert_eq!(check(&release("current", &me)), Some(if unknown { 5 } else { 0 }));
    assert_eq!(check(&release("newer", &me.replace(&format!("version = {}", env!("CARGO_PKG_VERSION")), "version = 99.0.0"))), Some(5));
    assert_eq!(check(&release("older", &me.replace(&format!("version = {}", env!("CARGO_PKG_VERSION")), "version = 0.0.1"))), Some(0), "a build newer than the latest release is ahead, not behind");
    assert_eq!(check(&d.join("no-release").to_string_lossy()), Some(3), "could not check");
    // no other command answers 5, and a mistyped update is a usage error
    assert_eq!(magician(&d, &["update", "--chek"]).status.code(), Some(2));
    let _ = std::fs::remove_dir_all(&d);
}
