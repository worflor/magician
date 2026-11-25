//! The text files the CLI reads, on hostile input: a scene patch (downloaded with a mod), the install ledger (other
//! tools rename and rewrite the addons folder), `gameinfo.gi`, the game lock's JSON, the build lock, `mod.toml`'s
//! proven line as `prove` rewrites it, a rig sidecar, `magician.exclude`, the config file, Steam's library list, a
//! process's command line (the seat check on Linux), the game's hero table and a hero's name as a person types it,
//! a `mod.toml` as discovery reads it and as `init` writes it, and a mod site's zip as `init` takes its paks out of
//! it. Each must read or refuse, never panic, and where
//! the CLI writes the file back, what it writes reads back the same. See `crates/fuzz`.

use crate::{install, lock, patchday, paths, scene};

const PATCH: &str = "# Scene patch for maps/ui/hero_prefabs/lash.vpk.\nmagician-scene-patch\t1\nmap\tmaps/ui/hero_prefabs/lash.vpk\nmod\tlash_showcase\nbuilt_for\t6864\nresult\td287ca64\nkeep\tmaps/ui/hero_prefabs/lash.vmap_c\t0a1b2c3d\t120\nreplace\tmaps/ui/hero_prefabs/lash/entities/default_ents.vents_c\t11111111\t22222222\t3000\nadd\tmaps/ui/hero_prefabs/lash/models/x.vmdl_c\t33333333\t42\n";

#[test]
fn scene_patches() {
    fuzz::run_text("scene-patch", fuzz::DEFAULT_CASES * 2, &[PATCH, &PATCH.replace('\n', "\r\n")], |t| {
        if let Ok(p) = scene::parse(t) {
            // what render writes, parse reads back as the same patch
            let mut again = scene::parse(&p.render()).unwrap_or_else(|e| panic!("{e}\n{}", p.render()));
            let mut p = p;
            // render lists the files sorted by path
            again.items.sort_by(|a, b| a.path.cmp(&b.path));
            p.items.sort_by(|a, b| a.path.cmp(&b.path));
            assert_eq!(again, p);
            for i in &p.items {
                assert!(!i.path.split('/').any(|s| s == ".." || s.is_empty()) && !i.path.contains('\\') && !i.path.contains(':'), "{}", i.path);
                assert!(i.path.starts_with(&format!("maps/ui/hero_prefabs/{}", p.hero)), "{}", i.path);
            }
        }
    });
}

const GAMEINFO: &str = "\"GameInfo\"\n{\n\tFileSystem\n\t{\n\t\tSearchPaths\n\t\t{\n\t\t\tMod\t\t\t\tcitadel\n\t\t\tWrite\t\t\t\tcitadel\n\t\t\tGame\t\t\t\tcitadel\n\t\t\tGame\t\t\t\tcore\n\t\t}\n\t}\n}\n";

#[test]
fn gameinfo_edits() {
    let crlf = GAMEINFO.replace('\n', "\r\n");
    let bare = GAMEINFO.replace("\t\t\tMod\t\t\t\tcitadel\n\t\t\tWrite\t\t\t\tcitadel\n", "");
    fuzz::run_text("gameinfo", fuzz::DEFAULT_CASES * 2, &[GAMEINFO, &crlf, &bare], |t| {
        for repair in [false, true] {
            if let Ok(Some(new)) = install::configure_addons_search_path(t, repair) {
                // the edit only adds lines, and once made it is made
                assert!(new.len() > t.len());
                assert_eq!(install::add_addons_search_path(&new), Ok(None), "the edit is idempotent:\n{new}");
            }
        }
    });
}

#[test]
fn ledgers_locks_manifests_and_lists() {
    let ledger = "pak10_dir.vpk\t3a6a5aff\tmaid_lash\nmaps/ui/hero_prefabs/lash.vpk\td287ca64\tlash_showcase\npak06_dir.vpk\t10e684c4\n";
    let game_lock = r#"{"holder": "qa-lash \"night\"", "since": 1763800000.5, "pid": 4242, "busy_until": 0}"#;
    let build_lock = "pid 4242\nstarted 1763800000\nholder release maid_lash\n";
    let toml = "[mod]\nslug = \"maid-lash\"\n\n[proven]\nbuilds = [6861, \"6864\"]  # filled by the harness, never by hand\n\n[rights]\nstate = \"cleared\"\n";
    let rig = "# magician rig build=6864 skeletons=1 graphs=1\nskel\tmodels/heroes_wip/lash/lash.vnmskel_c\troot,pelvis,spine_0\ngraph\tmodels/heroes_wip/lash/lash.vnmgraph_c\tSpeed,IsCrouching\n";
    let exclude = "# sound ships in pak06\nSounds\\VO/  # voices\n\nsoundevents/\n";
    let config = "# where things are\ngame = C:\\old\n reference=\"D:\\ref\"\nGAME = C:\\new\n";
    let vdf = "\"libraryfolders\"\n{\n\t\"0\"\n\t{\n\t\t\"path\"\t\t\"C:\\\\Program Files (x86)\\\\Steam\"\n\t}\n}\n";
    fuzz::run_text("cli-text", fuzz::DEFAULT_CASES * 4, &[ledger, game_lock, build_lock, toml, rig, exclude, config, vdf], |t| {
        let rows = install::parse_installed(t);
        let mut path = std::path::PathBuf::new();
        for r in &rows {
            assert!(!r.name.is_empty() && !r.name.contains('\t') && !r.crc.contains('\t'));
            path.push(&r.name);
        }
        let _ = install::lock_holder(t);
        let _ = install::seat_check(true, Some(t), Some("qa"));
        let _ = install::is_game_process(t, t);
        if let Some(h) = lock::Holder::parse(t) {
            let _ = h.describe();
        }
        if let Ok(new) = patchday::add_proven(t, "6865") {
            assert!(patchday::proven_builds(&new).contains(&"6865".to_string()), "{new}");
            assert_eq!(patchday::add_proven(&new, "6865").as_deref(), Ok(new.as_str()), "proving twice changes nothing");
        }
        if let Ok(r) = patchday::Rig::parse(t) {
            let mut out = Vec::new();
            r.write(&mut out).unwrap();
            let back = patchday::Rig::parse(&String::from_utf8(out).unwrap()).unwrap();
            assert_eq!((back.skeletons.len(), back.graphs.len()), (r.skeletons.len(), r.graphs.len()));
        }
        let _ = crate::exclude_lines(t);
        let _ = paths::config_lookup(t, "game");
        let _ = paths::library_paths(t);
        let _ = grade::manifest::proven(t);
    });
}

#[test]
fn hero_tables_and_names() {
    use crate::heroes::{tests, Heroes};
    let table = tests::table();
    fuzz::run_text("heroes", fuzz::DEFAULT_CASES, &[tests::VDATA, tests::NAMES, "models/heroes_wip/lash/lash.vmdl_c\nsounds/vo/lash/x.vsnd_c\n"], move |t| {
        // a heroes.vdata as KV3 text, and the localisation, from the same mutated text
        if let Ok(v) = resource::kv3::parse_text(t) {
            let h = Heroes::from_kv3(&v, t);
            for x in &h.list {
                let _ = (x.tokens(), x.label());
            }
            let _ = h.resolve(t);
        }
        // a name typed by a person, and a pak's paths
        let _ = table.resolve(t);
        if let Some(i) = table.infer(t.lines()) {
            assert!(i.files <= i.of && i.hero.playable);
            let _ = i.clear();
        }
    });
}

#[test]
fn mod_manifests_as_discovery_and_init_read_and_write_them() {
    use crate::initcmd::{clean, name_of, slug_of, Draft};
    use crate::modctx::ModCtx;
    let toml = "[mod]\nslug = \"maid-lash\"\nhero = \"hero_lash\"\npaks = [\"pak10_dir.vpk\", \"../build/pak06_dir.vpk\"]\nowners = [\"lash_voice\"]\naddon = \"maid_lash\"\nscenes = [\"package/scene/lash\"]\n[proven]\nbuilds = [6864]\n";
    let nowhere = std::env::temp_dir().join("magician-fuzz-no-such-mod");
    fuzz::run_text("mod-context", fuzz::DEFAULT_CASES, &[toml, "# a draft\n[mod]\nname = \"Velvet\"\n"], move |t| {
        // what discovery reads from a mod.toml someone wrote
        let ctx = ModCtx { toml: nowhere.join("mod.toml"), dir: nowhere.clone(), text: t.to_string(), found: "fuzz".into() };
        let _ = (ctx.label(), ctx.hero(), ctx.owners(), ctx.proven(), ctx.with_addons(), ctx.addon(None), ctx.unclaimed_kin(None));
        for f in ctx.own_paks().into_iter().chain(ctx.scenes()).chain(ctx.paks(None)) {
            assert!(!f.source.is_empty());
        }
        // and what init writes reads back as it was drafted, whatever the folder and names were
        let d = Draft { slug: slug_of(t), name: clean(t), hero: Some(clean(t)).filter(|h| !h.is_empty()), model: None, paks: t.lines().map(clean).filter(|p| !p.is_empty()).take(3).collect(), addon: None };
        let text = d.render();
        assert_eq!(grade::manifest::string(&text, "mod", "slug").as_deref(), Some(d.slug.as_str()), "{text}");
        assert!(d.slug.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'));
        if !d.name.is_empty() {
            assert_eq!(grade::manifest::string(&text, "mod", "name").as_deref(), Some(d.name.as_str()), "{text}");
        }
        assert_eq!(grade::manifest::list(&text, "mod", "paks"), d.paks, "{text}");
        assert!(grade::manifest::proven(&text).is_empty());
        let _ = name_of(t);
    });
}

#[test]
fn downloaded_zips() {
    // a mod site's zip, the one archive init reads itself: it reads or refuses, never panics
    use crate::archive::{tests::zip, zip_members, zip_read};
    let pak: Vec<u8> = (0..600u32).map(|i| (i % 13) as u8).collect();
    let seeds = vec![zip(&[("Mod/pak01_dir.vpk", &pak, true), ("Mod/readme.txt", b"hi", false)]), zip(&[("a.vpk", b"stored", false)])];
    fuzz::run("zip", fuzz::DEFAULT_CASES * 2, seeds, |d| {
        if let Ok(members) = zip_members(d) {
            for m in &members {
                let _ = zip_read(d, m);
            }
        }
    });
}
