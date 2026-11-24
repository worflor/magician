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

/// `magician mesh`: verdict plus every finding. `findings` are (code, message) in the order printed.
pub fn mesh_run(model: &str, vpk: &str, verdict: &str, findings: &[(&'static str, String)]) -> String {
    let items: Vec<String> = findings
        .iter()
        .map(|(c, m)| {
            let r = lint::rule(c);
            format!("{{\"code\":{},\"name\":{},\"severity\":{},\"message\":{},\"in_game\":{}}}", s(c), s(r.name), s(severity(r.severity)), s(m), s(r.in_game))
        })
        .collect();
    format!("{{\"model\":{},\"vpk\":{},\"verdict\":{},\"findings\":[{}]}}\n", s(model), s(vpk), s(verdict), items.join(","))
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
        .build()
}

pub fn rules() -> String {
    let items: Vec<String> = RULES
        .iter()
        .map(|r| format!("{{\"code\":{},\"name\":{},\"severity\":{},\"in_game\":{}}}", s(r.code), s(r.name), s(severity(r.severity)), s(r.in_game)))
        .collect();
    format!("{{\"schema\":\"magician.rules/1\",\"rules\":[{}]}}\n", items.join(","))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules_shape() {
        let j = rules();
        assert!(j.starts_with("{\"schema\":\"magician.rules/1\",\"rules\":[{\"code\":\"M001\",\"name\":\"path-case\",\"severity\":\"error\""));
        assert!(j.trim_end().ends_with("}]}"));
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
        let j = mesh_run("m.vmdl_c", "p.vpk", "PASS", &[]);
        assert_eq!(j, "{\"model\":\"m.vmdl_c\",\"vpk\":\"p.vpk\",\"verdict\":\"PASS\",\"findings\":[]}\n");
    }
}
