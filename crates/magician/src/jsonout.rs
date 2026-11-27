//! Hand-written JSON for the machine-readable outputs (`--json` on lint, drift, patchday, grade and release;
//! `lint --json-out`, `mesh --json-out`, `rules --json`), written with the one JSON writer (`grade::json`); the
//! shapes are fixed by the tests below. Every document a command prints with
//! `--json` starts with a `schema` string (`magician.<command>/<version>`).

use lint::allow::Applied;
use lint::{Finding, Severity, RULES};

pub use grade::json::{arr, num, s, Obj};

pub fn severity(v: Severity) -> &'static str {
    match v {
        Severity::Error => "error",
        Severity::Warn => "warn",
        Severity::Info => "info",
    }
}

fn path(p: &Option<String>) -> String {
    p.as_deref().map_or("null".to_string(), s)
}

fn finding(f: &Finding, allowed: Option<&str>) -> String {
    let mut o = format!(
        "{{\"code\":{},\"name\":{},\"severity\":{},\"path\":{},\"message\":{},\"allowed\":{}",
        s(f.code), s(f.name), s(severity(f.severity)), path(&f.path), s(&f.message), allowed.is_some()
    );
    if let Some(r) = allowed {
        o.push_str(&format!(",\"reason\":{}", s(r)));
    }
    o.push('}');
    o
}

/// The whole lint run after allowances: kept and allowed findings in one list (each marked), the stale
/// allowances, and the counts the text output ends with.
pub fn lint_run(r: &Applied, build: &str, vpk: &str, deep: bool) -> String {
    let errors = r.kept.iter().filter(|x| x.severity == Severity::Error).count();
    let warns = r.kept.iter().filter(|x| x.severity == Severity::Warn).count();
    let mut all: Vec<String> = r.kept.iter().map(|f| finding(f, None)).collect();
    all.extend(r.allowed.iter().map(|(f, why)| finding(f, Some(why))));
    let stale: Vec<String> = r
        .stale
        .iter()
        .map(|a| format!("{{\"line\":{},\"code\":{},\"path\":{},\"match\":{}}}", a.line, s(&a.code), s(&a.path), s(&a.needle)))
        .collect();
    format!(
        "{{\"schema\":\"magician.lint/1\",\"vpk\":{},\"build\":{},\"depth\":{},\"counts\":{{\"errors\":{errors},\"warnings\":{warns},\"notes\":{},\"allowed\":{},\"stale\":{}}},\"findings\":[{}],\"stale_allowances\":[{}]}}\n",
        s(vpk), s(build), s(if deep { "deep" } else { "light" }),
        r.kept.len() - errors - warns, r.allowed.len(), r.stale.len(), all.join(","), stale.join(",")
    )
}

/// `magician mesh`: verdict, the fair-play silhouettes against Valve's model (the numbers the report prints and M124
/// judges; empty when there is no Valve model to compare with) and every finding. `findings` are (code, message) in
/// the order printed.
pub fn mesh_run(model: &str, vpk: &str, verdict: &str, silhouettes: &[lint::mesh::views::ViewCompare], findings: &[(&'static str, String)]) -> String {
    let views: Vec<String> = silhouettes
        .iter()
        .map(|c| {
            Obj::new()
                .str("view", c.view)
                .num("percent_of_valve", 100.0 * c.ratio())
                .num("iou", c.iou)
                .int("ours_px", c.ours as i64)
                .int("valve_px", c.valve as i64)
                .int("added_px", c.added as i64)
                .int("lost_px", c.lost as i64)
                .build()
        })
        .collect();
    let items: Vec<String> = findings
        .iter()
        .map(|(c, m)| {
            let r = lint::rule(c);
            format!("{{\"code\":{},\"name\":{},\"severity\":{},\"message\":{},\"in_game\":{}}}", s(c), s(r.name), s(severity(r.severity)), s(m), s(r.in_game))
        })
        .collect();
    format!("{{\"model\":{},\"vpk\":{},\"verdict\":{},\"silhouettes\":[{}],\"findings\":[{}]}}\n", s(model), s(vpk), s(verdict), views.join(","), items.join(","))
}

/// One pak's drift between two eras (`drift --json`, and each pak of `patchday --json`).
pub fn drift(vpk: &str, files: usize, d: &crate::patchday::Drift, from: &str, to: &str) -> String {
    let deps: Vec<&String> = d.deps.iter().collect();
    Obj::new()
        .str("vpk", vpk)
        .int("files", files as i64)
        .str("state", d.state())
        .str("verdict", &d.verdict(from, to))
        .strs("changed", &d.changed)
        .strs("removed", &d.removed)
        .strs("broken", &crate::patchday::group_broken(&d.broken))
        .strs("deps_changed", &deps)
        .strs("rig_deps_changed", &d.rig_deps())
        .strs("bones_lost", &d.skeleton)
        .strs("params_lost", &d.params)
        .bool("contract_checked", d.contract_checked)
        .strs("contract", &d.contract)
        .int("contract_own", d.contract_own as i64)
        .build()
}

#[cfg(test)]
pub fn rules() -> String {
    rules_with_record(|_| true, None)
}

/// Each rule's record on one build, as `rules` prints it: Valve's own content it fires on (`magician corpus`), and
/// what the game said of its findings in witnessed passes (`magician witness`).
pub struct RuleRecord<'a> {
    pub build: &'a str,
    pub corpus: &'a [(String, usize, usize, String)],
    pub witness: &'a std::collections::BTreeMap<String, [usize; 4]>,
}

/// The catalogue as `magician.rules/1`, the rules `keep` takes by code. `fair_play` says the rule is judged on the
/// fair-play axis rather than as a break (M062 for its hitboxes only, and M144, the check that could not run). With a
/// record, `build` and per rule `corpus` (each calibration: fires, of, reading) and `witness`
/// (confirmed, contradicted, not reached, a frame's or the files'), `[]` and `null` where the build has none.
pub fn rules_with_record(keep: impl Fn(&str) -> bool, record: Option<&RuleRecord>) -> String {
    let items: Vec<String> = RULES
        .iter()
        .filter(|r| keep(r.code))
        .map(|r| {
            let fair = grade::rubric::FAIR_PLAY_RULES.contains(&r.code) || r.code == grade::rubric::UNCHECKED;
            let mut o = Obj::new().str("code", r.code).str("name", r.name).str("severity", severity(r.severity)).bool("fair_play", fair).str("in_game", r.in_game);
            if let Some(rec) = record {
                let corpus = arr(rec.corpus.iter().filter(|c| c.0 == r.code).map(|(_, fires, of, reading)| Obj::new().int("fires", *fires as i64).int("of", *of as i64).str("reading", reading).build()));
                let witness = rec.witness.get(r.code).map(|[c, x, n, f]| Obj::new().int("confirmed", *c as i64).int("contradicted", *x as i64).int("not_reached", *n as i64).int("frame_or_files", *f as i64).build());
                o = o.raw("corpus", corpus).raw("witness", witness.unwrap_or_else(|| "null".into()));
            }
            o.build()
        })
        .collect();
    let build = record.map(|r| format!(",\"build\":{}", s(r.build))).unwrap_or_default();
    format!("{{\"schema\":\"magician.rules/1\"{build},\"rules\":[{}]}}\n", items.join(","))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules_shape() {
        let j = rules();
        assert!(j.starts_with("{\"schema\":\"magician.rules/1\",\"rules\":[{\"code\":\"M001\",\"name\":\"path-case\",\"severity\":\"error\""));
        assert!(j.trim_end().ends_with("}]}"));
        assert!(j.contains("\"code\":\"M124\",\"name\":\"silhouette-fair-play\",\"severity\":\"error\",\"fair_play\":true,") && j.contains("\"code\":\"M144\",\"name\":\"fair-play-unchecked\",\"severity\":\"warn\",\"fair_play\":true,"), "{j}");
        assert!(j.contains("\"code\":\"M030\",\"name\":\"dangling-ref\",\"severity\":\"error\",\"fair_play\":false,"), "{j}");
    }

    #[test]
    fn lint_shape() {
        let f = Finding { code: "M011", name: "loose-media", severity: Severity::Warn, path: Some("a/b.png".into()), message: "say \"hi\"".into() };
        let g = Finding { code: "M001", name: "path-case", severity: Severity::Error, path: None, message: "x".into() };
        let r = Applied { kept: vec![f], allowed: vec![(g, "known and accepted".into())], stale: vec![] };
        let j = lint_run(&r, "6864", "p.vpk", true);
        assert!(j.starts_with("{\"schema\":\"magician.lint/1\",\"vpk\":\"p.vpk\""), "{j}");
        assert!(j.contains("\"depth\":\"deep\""));
        assert!(j.contains("\"counts\":{\"errors\":0,\"warnings\":1,\"notes\":0,\"allowed\":1,\"stale\":0}"));
        assert!(j.contains("\"message\":\"say \\\"hi\\\"\",\"allowed\":false}"));
        assert!(j.contains("\"path\":null,\"message\":\"x\",\"allowed\":true,\"reason\":\"known and accepted\"}"));
    }

    #[test]
    fn drift_shape() {
        let mut d = crate::patchday::Drift::default();
        d.changed.push("a.vnmclip_c".into());
        let j = drift("pak10_dir.vpk", 3, &d, "6861", "6864");
        assert!(j.starts_with("{\"vpk\":\"pak10_dir.vpk\",\"files\":3,\"state\":\"re-prove\",\"verdict\":\"loads on 6864"), "{j}");
        assert!(j.contains("\"changed\":[\"a.vnmclip_c\"]") && j.contains("\"contract_checked\":false"), "{j}");
    }

    #[test]
    fn mesh_shape() {
        let j = mesh_run("m.vmdl_c", "p.vpk", "PASS", &[], &[]);
        assert_eq!(j, "{\"model\":\"m.vmdl_c\",\"vpk\":\"p.vpk\",\"verdict\":\"PASS\",\"silhouettes\":[],\"findings\":[]}\n");
        // the silhouette the report prints (and M124 judges) is in the file too
        let front = lint::mesh::views::ViewCompare { view: "front", ours: 930, valve: 1000, iou: 0.9, added: 10, lost: 80 };
        let j = mesh_run("m.vmdl_c", "p.vpk", "FAIL", &[front], &[]);
        assert!(j.contains("\"silhouettes\":[{\"view\":\"front\",\"percent_of_valve\":93,\"iou\":0.9,\"ours_px\":930,\"valve_px\":1000,\"added_px\":10,\"lost_px\":80}]"), "{j}");
    }
}
