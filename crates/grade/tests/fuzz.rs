//! The text files a grade reads, on hostile input: a policy (read strictly: every bad line is an error naming it,
//! never a panic, and what a policy prints reads back to the same fingerprint) and a `mod.toml`. See `crates/fuzz`.

#![allow(clippy::unwrap_used, clippy::expect_used)] // a test that cannot unwrap has failed

#[global_allocator]
static CAP: fuzz::Capped = fuzz::Capped;

/// Every key at its default, as `magician policy` prints it: the policy file that spells everything out.
fn spelled(p: &grade::Policy) -> String {
    let mut text = String::new();
    let mut last = String::new();
    for (k, v, _) in p.entries() {
        let (sec, key) = k.rsplit_once('.').unwrap();
        if sec != last {
            text.push_str(&format!("[{sec}]\n"));
            last = sec.to_string();
        }
        text.push_str(&format!("{key} = {v}\n"));
    }
    text
}

#[test]
fn policy_files() {
    let full = spelled(&grade::Policy::default());
    let seeds = [
        "# a stricter house line\n[fair_play]\nenforce = true\nhitbox_egregious_units = 15  # stricter\n[compliance]\nm140_max_worst_units = 12\nadvisory = [\"M081\", \"M130\"]\n[rights]\nenforce = false\n",
        "[groups]\ncovered = 0.9\ntouched = 0.1\n[weights.kit]\nvfx = 30\nsound = 20\n",
        full.as_str(),
    ];
    fuzz::run_text("policy", fuzz::DEFAULT_CASES * 2, &seeds, |t| {
        if let Ok(p) = grade::policy::parse(t, "fuzz.toml") {
            // what a policy says it is, spelled out, is the same policy
            let back = grade::policy::parse(&spelled(&p), "again.toml").unwrap_or_else(|e| panic!("{e}\n{}", spelled(&p)));
            assert_eq!(back.fingerprint(), p.fingerprint());
        } else if let Err(e) = grade::policy::parse(t, "fuzz.toml") {
            assert!(e.starts_with("fuzz.toml"), "an error names the file: {e}");
        }
    });
}

#[test]
fn mod_manifests() {
    let seeds = [
        "[mod]\nslug    = \"maid-lash\"\nname    = \"Maid Lash\"\nhero    = \"hero_lash\"   # heroes.vdata key\nowners = [\"lash_showcase\", \"lash_voice\"]\n\n[proven]\nbuilds = [6861, \"6864\"]  # filled by the harness\n\n[rights]\nstate = \"cleared\"\nnotes = \"original\"\n\n[identity]\nvoice = \"new\"\n",
        "[target]\nmodel = \"models/heroes_wip/lash/lash.vmdl\"\n[proven]\nevidence = [\"a\", \"b\"]\n",
    ];
    fuzz::run_text("manifest", fuzz::DEFAULT_CASES * 2, &seeds, |t| {
        for (sec, key) in [("mod", "slug"), ("mod", "hero"), ("rights", "state"), ("identity", "name"), ("mod", "owners"), ("proven", "evidence")] {
            let _ = grade::manifest::string(t, sec, key);
            let _ = grade::manifest::list(t, sec, key);
        }
        let _ = grade::record::Rights::parse(grade::manifest::string(t, "rights", "state").as_deref());
        for b in grade::manifest::proven(t) {
            assert!(b.bytes().all(|c| c.is_ascii_digit()), "{b}");
        }
    });
}
