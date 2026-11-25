//! What a lint run tells the grade. One function, used by `magician grade` whether it lints itself or reads the run
//! `magician release` made, so `broken` sees every rule family the run checked (the mesh rules M100-M140 included
//! when the run was deep).
//!
//! Fair-play findings (and M144, a fair-play check that could not run) are left out here: they set compliance
//! (`compliance.rs`), never `broken`. Every other error counts even when an allowance accepts it: an allowance
//! records why a finding is accepted, it can never argue away something that won't load (docs/PDD.md, Taste).
//! Allowed warnings and allowed repoints (M061) are not defects.

use crate::rubric::is_compliance;
use lint::allow::{apply, Allowance};
use lint::report::Depth;
use lint::{Finding, Severity};
use std::collections::BTreeMap;

#[derive(Debug)]
pub struct LintFacts {
    pub depth: Depth,
    /// Every error in the run that is not fair play, allowed or not.
    pub errors: usize,
    /// Those errors by rule code, for the grade's explanation.
    pub errors_by_code: BTreeMap<&'static str, usize>,
    /// Warnings that are not fair play and carry no allowance.
    pub warnings: usize,
    pub dropped_deps: usize,
    pub orphans: usize,
    /// Every allowance used, by rule code (fair-play ones included: compliance reads those itself).
    pub allowed_by_code: BTreeMap<&'static str, usize>,
}

impl LintFacts {
    /// Whether the mod is broken: a lint error that is not fair play, allowed or not, means something won't load in
    /// game. The one definition: the grade's `broken` flag and `magician check`'s verdict both read it.
    pub fn broken(&self) -> bool {
        self.errors > 0
    }
}

pub fn lint_facts(findings: Vec<Finding>, allowances: &[Allowance], depth: Depth) -> LintFacts {
    let mut errors_by_code = BTreeMap::new();
    for f in findings.iter().filter(|f| f.severity == Severity::Error && !is_compliance(f.code, &f.message)) {
        *errors_by_code.entry(f.code).or_default() += 1;
    }
    let applied = apply(findings, allowances, matches!(depth, Depth::Deep));
    let count = |c: &str| applied.kept.iter().filter(|f| f.code == c).count();
    let mut allowed_by_code = BTreeMap::new();
    for (f, _) in &applied.allowed {
        *allowed_by_code.entry(f.code).or_default() += 1;
    }
    LintFacts {
        depth,
        errors: errors_by_code.values().sum(),
        errors_by_code,
        warnings: applied.kept.iter().filter(|f| f.severity == Severity::Warn && !is_compliance(f.code, &f.message)).count(),
        dropped_deps: count("M061"),
        orphans: count("M031"),
        allowed_by_code,
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use lint::report;

    fn finding(c: &str, message: &str) -> Finding {
        let r = lint::rule(c);
        Finding { code: r.code, name: r.name, severity: r.severity, path: Some("models/heroes_wip/lash/lash.vmdl_c".into()), message: message.into() }
    }

    #[test]
    fn a_release_whose_only_errors_are_deep_mesh_errors_is_broken() {
        // Real case: Maid Lash's release lint (--deep) reported M106, M129 and M137 errors while its grade, which
        // linted again without --deep, said nothing of them.
        assert!(!lint_facts(vec![], &[], Depth::Deep).broken());
        // The release's own run, written and read back the way `magician release` hands it to the grade.
        let run: Vec<Finding> = ["M106", "M137"].iter().map(|c| finding(c, "on the release model")).collect();
        let file = report::write(&run, Depth::Deep, "6864", "pak10_dir.vpk");
        let (depth, back) = report::read(&file).unwrap();
        // Even accepted with a written reason, an error still counts.
        let allow = lint::allow::parse("M137\t*\t*\tdesigner's call: the pale apron reads as cloth in game\n").unwrap();
        let l = lint_facts(back, &allow, depth);
        assert_eq!(l.depth, Depth::Deep);
        assert_eq!(l.errors, 2, "allowed errors keep counting");
        assert_eq!(l.allowed_by_code.get("M137"), Some(&1));
        assert!(l.broken());
    }

    #[test]
    fn fair_play_findings_never_make_a_mod_broken() {
        // M129 (the egregious envelope, an error), M140 kept without a reason, M130, M144: none is the mod not working.
        let run = vec![finding("M129", "x"), finding("M140", "y"), finding("M130", "z"), finding("M144", "w"), finding("M062", "hitbox 'default/head' changed")];
        let l = lint_facts(run, &[], Depth::Deep);
        assert_eq!((l.errors, l.warnings, l.broken()), (0, 0, false), "{l:?}");
        // a lost bone is the mod not working: that M062 is an error like any other
        let l = lint_facts(vec![finding("M062", "bone 'pelvis' missing")], &[], Depth::Deep);
        assert!(l.errors == 1 && l.broken());
    }

    #[test]
    fn allowed_warnings_are_not_defects() {
        let r = lint::rule("M064");
        let w = Finding { code: r.code, name: r.name, severity: r.severity, path: Some("sounds/vo/lash/a.vsnd_c".into()), message: "4.37 s long, Valve's 2.54 s".into() };
        let allow = lint::allow::parse("M064\t*\tlong, Valve's\tthe kill lines carry the bell ending on purpose\n").unwrap();
        let l = lint_facts(vec![w], &allow, Depth::Light);
        assert_eq!((l.errors, l.warnings), (0, 0));
    }
}
