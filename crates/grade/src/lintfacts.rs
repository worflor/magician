//! What a lint run tells the depth grade. One function, used by `magician grade` whether it lints itself or reads the
//! run `magician release` made, so the cap "lint errors -> max C" sees every rule family the run checked (the
//! mesh rules M100-M140 included when the run was deep).
//!
//! Fair-play findings (and M144, a fair-play check that could not run) are left out here: they set compliance
//! (`compliance.rs`), never the letter. Every other error counts even when an allowance accepts it: an allowance
//! records why a finding is accepted, it can never argue away something that won't load (docs/PDD.md, Taste).
//! Allowed warnings and allowed repoints (M061) are not craft defects.

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
    use crate::footprint::{Cat, Footprint, Item};
    use crate::policy::Policy;
    use crate::rubric::{grade, Archetype, ModFacts};
    use lint::report;

    fn fp() -> Footprint {
        let mut f = Footprint { hero_key: "hero_lash".into(), display: "Lash".into(), tokens: vec!["lash".into()], ..Default::default() };
        let mut add = |cat, g: &str, p: String| f.items.push(Item { cat, group: g.into(), path: p });
        add(Cat::Model, "core", "models/heroes_wip/lash/lash.vmdl_c".into());
        for i in 0..8 {
            add(Cat::AbilityVfx, "strike", format!("particles/abilities/lash/strike_{i}.vpcf_c"));
            add(Cat::Sound, "strike", format!("sounds/abilities/lash/strike_{i}.vsnd_c"));
            add(Cat::Voice, "vo:kill", format!("sounds/vo/lash/lash_kill_{i}.vsnd_c"));
        }
        for i in 0..4 {
            add(Cat::Animation, "clips:abilities", format!("models/heroes_staging/lash_v2/clips/ability_{i}.vnmclip_c"));
        }
        add(Cat::Animation, "clips:ui", "models/heroes_staging/lash_v2/clips/ui_hero_select.vnmclip_c".into());
        add(Cat::UiArt, "ui", "panorama/images/heroes/lash_card_psd.vtex_c".into());
        f
    }

    fn facts(fp: &Footprint, l: &LintFacts) -> ModFacts {
        let mut paths: std::collections::BTreeSet<String> = fp.items.iter().map(|i| i.path.clone()).collect();
        paths.extend((0..3).map(|i| format!("models/heroes_wip/lash/materials/m{i}.vmat_c")));
        ModFacts { paths, lint_errors: l.errors, lint_warnings: l.warnings, dropped_deps: l.dropped_deps, orphans: l.orphans, model_anim_changed: true, ..Default::default() }
    }

    fn finding(c: &str, message: &str) -> Finding {
        let r = lint::rule(c);
        Finding { code: r.code, name: r.name, severity: r.severity, path: Some("models/heroes_wip/lash/lash.vmdl_c".into()), message: message.into() }
    }

    fn g(fp: &Footprint, l: &LintFacts) -> crate::rubric::Report {
        grade(fp, &facts(fp, l), Archetype::Character, &Policy::default())
    }

    #[test]
    fn a_release_whose_only_errors_are_deep_mesh_errors_grades_at_most_c() {
        // Real case: Maid Lash's release lint (--deep) reported M106, M129 and M137 errors while its grade, which
        // linted again without --deep, printed S 93.2.
        let fp = fp();
        let clean = g(&fp, &lint_facts(vec![], &[], Depth::Deep));
        assert!(matches!(clean.letter, "S" | "A"), "the fixture must grade high without errors: {} {}", clean.letter, clean.score);

        // The release's own run, written and read back the way `magician release` hands it to the grade.
        let run: Vec<Finding> = ["M106", "M137"].iter().map(|c| finding(c, "on the release model")).collect();
        let file = report::write(&run, Depth::Deep, "6864", "pak10_dir.vpk");
        let (depth, back) = report::read(&file).unwrap();
        // Even accepted with a written reason, an error still caps.
        let allow = lint::allow::parse("M137\t*\t*\tdesigner's call: the pale apron reads as cloth in game\n").unwrap();
        let l = lint_facts(back, &allow, depth);
        assert_eq!(l.depth, Depth::Deep);
        assert_eq!(l.errors, 2, "allowed errors keep counting");
        assert_eq!(l.allowed_by_code.get("M137"), Some(&1));
        let r = g(&fp, &l);
        assert!(matches!(r.letter, "C" | "D" | "F"), "deep mesh errors must cap the grade at C, got {} {}", r.letter, r.score);
        assert!(r.caps.iter().any(|c| c.why.contains("lint errors")));
    }

    #[test]
    fn fair_play_findings_never_touch_the_letter() {
        let fp = fp();
        let clean = g(&fp, &lint_facts(vec![], &[], Depth::Deep));
        // M129 (the egregious envelope, an error), M140 kept without a reason, M130, M144: none is a depth matter.
        let run = vec![finding("M129", "x"), finding("M140", "y"), finding("M130", "z"), finding("M144", "w"), finding("M062", "hitbox 'default/head' changed")];
        let l = lint_facts(run, &[], Depth::Deep);
        assert_eq!((l.errors, l.warnings), (0, 0), "{l:?}");
        let r = g(&fp, &l);
        assert_eq!((r.letter, r.score), (clean.letter, clean.score));
        assert!(r.caps.is_empty());
        // a lost bone is the mod not working: that M062 caps like any error
        let l = lint_facts(vec![finding("M062", "bone 'pelvis' missing")], &[], Depth::Deep);
        assert_eq!(l.errors, 1);
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
