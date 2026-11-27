//! Compliance: is the mod fair to play with? Three states, read from the lint run and the allowances.
//!
//! - **pass** (`ranked-safe`): every fair-play check the mod's files call for ran, and nothing is unresolved;
//! - **fail** (`freeform`): at least one measurement past a fair-play limit; the skin took liberties with how the hero
//!   reads, which informs and never rates it lower;
//! - **unknown** (`fair play not checked`): a check that should have run could not (a light lint over a model, or M144: a file or
//!   Valve's original that did not decode). Unknown never becomes pass.
//!
//! Each fair-play rule is one of three kinds (`rubric.rs` defaults, moved by a policy): never allowable (a finding
//! fails fair play whatever an allowance says), advisory (reported, never a violation), or bounded (needs an
//! allowance with a reason, and the allowance holds only within the rule's limit, which the record lists beside the
//! measured value). A violation never changes the description: what a mod makes its own and whether it is fair are
//! separate.

use crate::policy::Policy;
use crate::rubric::{is_compliance, is_fair_play, UNCHECKED};
use lint::allow::{apply, Allowance};
use lint::report::Depth;
use lint::{Finding, Severity};

/// The severity a finding is reported at under the policy. Fair play is lenient unless enforced: by default every
/// fair-play finding and every M144 is at most a warning, so lint, build and install never stop on one alone (the
/// grade still reads them, and its label still says freeform or fair play not checked). With `fair_play.enforce = true` every one that can
/// fail fair play or leave it unknown (all but the advisory rules) is an error. Anything else keeps its rule's own
/// severity: a broken reference is an error whatever the policy says.
pub fn severity(code: &str, message: &str, own: Severity, p: &Policy) -> Severity {
    if !is_compliance(code, message) {
        own
    } else if !p.enforce {
        own.min(Severity::Warn)
    } else if p.compliance.advisory.iter().any(|a| a == code) {
        own
    } else {
        Severity::Error
    }
}

/// `severity` over a lint run, in place: what `lint`, `build`, `release`, `install` and `grade` act on.
pub fn set_severities(findings: &mut [Finding], p: &Policy) {
    for f in findings.iter_mut() {
        f.severity = severity(f.code, &f.message, f.severity, p);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Pass,
    Fail,
    Unknown,
}

impl State {
    pub fn id(self) -> &'static str {
        match self {
            State::Pass => "pass",
            State::Fail => "fail",
            State::Unknown => "unknown",
        }
    }
    /// The words a reader sees: `ranked-safe`, `freeform` or `fair play not checked`. `id` stays the machine value.
    pub fn name(self) -> &'static str {
        match self {
            State::Pass => "ranked-safe",
            State::Fail => "freeform",
            State::Unknown => "fair play not checked",
        }
    }
    /// The label's mark: `freeform`, `fair play not checked` or nothing (a ranked-safe skin adds no mark).
    pub fn mark(self) -> Option<&'static str> {
        match self {
            State::Pass => None,
            State::Fail => Some("freeform"),
            State::Unknown => Some("fair play not checked"),
        }
    }
}

/// What the mod ships that fair play has rules for: replaced models, particles and sounds.
#[derive(Debug, Clone, Copy, Default)]
pub struct Applicable {
    pub models: bool,
    pub particles: bool,
    pub sounds: bool,
}

impl Applicable {
    /// The fair-play rules these files call for, and which of them only a deep lint runs.
    fn rules(self) -> Vec<(&'static str, bool)> {
        let mut out = Vec::new();
        if self.models {
            out.extend([("M062", false), ("M124", true), ("M129", true), ("M130", true), ("M131", false), ("M140", true), ("M152", false)]);
        }
        if self.particles {
            out.extend([("M081", false), ("M082", false)]);
        }
        if self.sounds {
            out.push(("M063", false));
        }
        out.sort();
        out
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Measure {
    pub name: &'static str,
    pub value: f64,
    pub limit: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub rule: &'static str,
    pub path: Option<String>,
    pub message: String,
    /// The allowance's reason, when one accepted the finding.
    pub reason: Option<String>,
    pub measures: Vec<Measure>,
    /// Why a violation is one (no allowance, never allowable, over the limit).
    pub why: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Unknown {
    pub rules: Vec<String>,
    pub path: Option<String>,
    pub why: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FairPlay {
    pub state: State,
    pub checks_run: Vec<&'static str>,
    pub unknown: Vec<Unknown>,
    pub violations: Vec<Item>,
    pub allowances: Vec<Item>,
    pub advisories: Vec<Item>,
}

/// The rule codes an M144 message says did not run: the list in its last parentheses.
fn unchecked_rules(message: &str) -> Vec<String> {
    let inner = message.rsplit('(').next().and_then(|t| t.split(')').next()).unwrap_or("");
    inner.split(',').map(str::trim).filter(|c| c.len() == 4 && c.starts_with('M') && c[1..].bytes().all(|b| b.is_ascii_digit())).map(String::from).collect()
}

/// What a bounded rule's finding measured, against the policy's limits. `None`: the measure could not be read.
fn measures(code: &str, message: &str, p: &Policy) -> Option<Vec<Measure>> {
    match code {
        "M140" => {
            let (worst, share) = lint::mesh::rules::envelope_measures(message)?;
            let (lw, ls) = p.m140_limits();
            Some(vec![Measure { name: "worst_units", value: worst, limit: lw }, Measure { name: "visible_share", value: share, limit: ls }])
        }
        "M130" => Some(vec![Measure { name: "wide_share", value: lint::mesh::rules::wide_measure(message)?, limit: p.compliance.m130_max_share }]),
        _ => Some(vec![]),
    }
}

/// Fair play for a lint run: every finding before allowances, the allowances, the depth the run had, and what the mod
/// ships. Allowances on rules that are not fair play are applied by `lintfacts`, not here.
pub fn judge(findings: &[Finding], allowances: &[Allowance], depth: Depth, applicable: Applicable, p: &Policy) -> FairPlay {
    let mut unknown: Vec<Unknown> = Vec::new();
    let deep = matches!(depth, Depth::Deep);
    let wanted = applicable.rules();
    let deep_only: Vec<String> = wanted.iter().filter(|(_, d)| *d).map(|(c, _)| c.to_string()).collect();
    if !deep && !deep_only.is_empty() {
        unknown.push(Unknown { rules: deep_only, path: None, why: "the lint ran light, so the mesh family's fair-play rules did not run (grade without --light)".into() });
    }
    for f in findings.iter().filter(|f| f.code == UNCHECKED) {
        unknown.push(Unknown { rules: unchecked_rules(&f.message), path: f.path.clone(), why: f.message.clone() });
    }
    let not_run: Vec<&String> = unknown.iter().flat_map(|u| u.rules.iter()).collect();
    let checks_run: Vec<&'static str> = wanted.iter().map(|(c, _)| *c).filter(|c| !not_run.iter().any(|n| n.as_str() == *c)).collect();

    let fair: Vec<Finding> = findings.iter().filter(|f| is_fair_play(f.code, &f.message)).cloned().collect();
    let applied = apply(fair, allowances, deep, None);
    let never = |c: &str| p.compliance.never.iter().any(|x| x == c);
    let advisory = |c: &str| p.compliance.advisory.iter().any(|x| x == c);
    let item = |f: &Finding, reason: Option<String>, measures: Vec<Measure>, why: &str| Item {
        rule: f.code,
        path: f.path.clone(),
        message: f.message.clone(),
        reason,
        measures,
        why: why.to_string(),
    };
    let (mut violations, mut allowed, mut advisories) = (Vec::new(), Vec::new(), Vec::new());
    for f in &applied.kept {
        if advisory(f.code) {
            advisories.push(item(f, None, vec![], "advisory"));
        } else if never(f.code) {
            violations.push(item(f, None, vec![], "never allowable"));
        } else {
            violations.push(item(f, None, measures(f.code, &f.message, p).unwrap_or_default(), "needs an allowance with a written reason"));
        }
    }
    for (f, reason) in &applied.allowed {
        if advisory(f.code) {
            advisories.push(item(f, Some(reason.clone()), vec![], "advisory"));
        } else if never(f.code) {
            violations.push(item(f, Some(reason.clone()), vec![], "never allowable: an allowance does not waive it"));
        } else {
            match measures(f.code, &f.message, p) {
                Some(m) if m.iter().all(|x| x.value <= x.limit + 1e-12) => allowed.push(item(f, Some(reason.clone()), m, "within the allowance limit")),
                Some(m) => {
                    let over: Vec<String> = m.iter().filter(|x| x.value > x.limit + 1e-12).map(|x| format!("{} {} over the limit {}", x.name, crate::json::num(x.value), crate::json::num(x.limit))).collect();
                    violations.push(item(f, Some(reason.clone()), m, &format!("allowed, but {}", over.join(" and "))));
                }
                None => violations.push(item(f, Some(reason.clone()), vec![], "allowed, but its measure could not be read, so the limit cannot be checked")),
            }
        }
    }
    // One order whatever order the lint listed them in: by rule, then path, then message.
    for list in [&mut violations, &mut allowed, &mut advisories] {
        list.sort_by(|a: &Item, b: &Item| (a.rule, &a.path, &a.message).cmp(&(b.rule, &b.path, &b.message)));
    }
    let state = if !violations.is_empty() {
        State::Fail
    } else if !unknown.is_empty() {
        State::Unknown
    } else {
        State::Pass
    };
    FairPlay { state, checks_run, unknown, violations, allowances: allowed, advisories }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MODEL: &str = "models/heroes_wip/lash/lash.vmdl_c";

    fn f(code: &str, path: &str, message: &str) -> Finding {
        let r = lint::rule(code);
        Finding { code: r.code, name: r.name, severity: r.severity, path: Some(path.into()), message: message.into() }
    }

    #[test]
    fn fair_play_is_a_warning_unless_enforced_and_nothing_else_moves() {
        let lenient = Policy::default();
        let strict = crate::policy::parse("[fair_play]\nenforce = true\n", "strict.toml").unwrap();
        let sev = |code: &str, message: &str, p: &Policy| {
            let mut run = vec![f(code, MODEL, message)];
            set_severities(&mut run, p);
            run[0].severity
        };
        use Severity::{Error, Warn};
        for (code, message, lenient_is, strict_is) in [
            ("M129", "a bit much", Warn, Error),
            ("M124", "front view", Warn, Error),
            ("M063", "volume -4", Warn, Error),
            ("M062", "hitbox head_0 is gone", Warn, Error),
            ("M140", "past the taste line", Warn, Error),
            ("M144", "did not decode (M063)", Warn, Error),
            ("M081", "brighter", Warn, Warn),     // advisory: never a violation, so never an error
            ("M062", "bone 'head' is gone", Error, Error), // a lost bone is the mod not working, not fair play
            ("M030", "needs 'x'", Error, Error),
        ] {
            assert_eq!((sev(code, message, &lenient), sev(code, message, &strict)), (lenient_is, strict_is), "{code} {message}");
        }
        // a policy that moves a rule out of the advisory list makes it an error when enforced
        let m081 = crate::policy::parse("[fair_play]\nenforce = true\n[compliance]\nadvisory = [\"M130\"]\n", "p").unwrap();
        assert_eq!(sev("M081", "brighter", &m081), Error);
    }

    /// An M140 message as the lint writes it, for a worst point and visible share.
    fn m140(worst: f64, share: f64) -> Finding {
        let pct = format!("{:.2}", 100.0 * share);
        let msg = format!("106.0 u2 of visible surface ({}% of it) lies more than 10 units beyond every hitbox capsule; worst point {worst:.2} u beyond leg_upper_capsule_R at (-22, -1, 43), under the 18 u / 3.5% line: tasteful overflow needs a written reason", pct.trim_end_matches('0'));
        f("M140", MODEL, &msg)
    }

    fn allow(text: &str) -> Vec<Allowance> {
        lint::allow::parse(text).unwrap()
    }

    const MODELS: Applicable = Applicable { models: true, particles: false, sounds: false };

    #[test]
    fn nothing_found_on_a_deep_run_is_a_pass_naming_the_checks_that_ran() {
        let r = judge(&[], &[], Depth::Deep, Applicable { models: true, particles: true, sounds: true }, &Policy::default());
        assert_eq!(r.state, State::Pass);
        assert_eq!(r.checks_run, vec!["M062", "M063", "M081", "M082", "M124", "M129", "M130", "M131", "M140", "M152"]);
        assert_eq!(r.state.mark(), None);
    }

    #[test]
    fn an_allowed_m140_within_its_limit_passes_and_is_listed_with_value_and_limit() {
        // Maid Lash's bell skirt: worst point 11.27 u, 0.39% beyond 10 u, accepted with a reason
        let a = allow("M140\tmodels/heroes_wip/lash/lash.vmdl_c\t*\tthe bell skirt's flare is the signature silhouette\n");
        let r = judge(&[m140(11.27, 0.0039)], &a, Depth::Deep, MODELS, &Policy::default());
        assert_eq!(r.state, State::Pass, "{r:?}");
        assert_eq!(r.allowances.len(), 1);
        let x = &r.allowances[0];
        assert_eq!((x.rule, x.reason.as_deref()), ("M140", Some("the bell skirt's flare is the signature silhouette")));
        assert_eq!(x.measures, vec![Measure { name: "worst_units", value: 11.27, limit: 18.0 }, Measure { name: "visible_share", value: 0.0039, limit: 0.035 }]);
    }

    #[test]
    fn an_m140_without_a_reason_is_a_violation() {
        let r = judge(&[m140(11.27, 0.0039)], &[], Depth::Deep, MODELS, &Policy::default());
        assert_eq!((r.state, r.state.mark()), (State::Fail, Some("freeform")));
        assert!(r.violations[0].why.contains("written reason"));
    }

    #[test]
    fn an_allowance_over_the_policy_limit_is_freeform_just_under_at_and_just_over() {
        let a = allow("M140\t*\t*\tthe train is the whole point of the costume\n");
        let at = |limit: f64| {
            let p = crate::policy::parse(&format!("[compliance]\nm140_max_worst_units = {limit}\n"), "p").unwrap();
            judge(&[m140(12.5, 0.005)], &a, Depth::Deep, MODELS, &p)
        };
        assert_eq!(at(12.49).state, State::Fail);
        assert!(at(12.49).violations[0].why.contains("worst_units 12.5 over the limit 12.49"), "{:?}", at(12.49).violations);
        assert_eq!(at(12.5).state, State::Pass, "at the limit is within it");
        assert_eq!(at(12.51).state, State::Pass);
    }

    #[test]
    fn a_never_allowable_rule_fails_even_with_an_allowance() {
        let a = allow("M129\t*\t*\tthe train is the point of the costume\nM124\t*\t*\tslimmer on purpose for the silhouette\n");
        for code in ["M129", "M124"] {
            let r = judge(&[f(code, MODEL, "x")], &a, Depth::Deep, MODELS, &Policy::default());
            assert_eq!(r.state, State::Fail, "{code}");
            assert!(r.violations[0].why.contains("does not waive"), "{code}: {:?}", r.violations);
        }
        // a hidden sound cue the same way
        let r = judge(&[f("M063", "soundevents/hero/lash.vsndevts_c", "Lash.Flog: volume -4 dB, Valve's 3")], &[], Depth::Deep, Applicable { sounds: true, ..Default::default() }, &Policy::default());
        assert_eq!(r.state, State::Fail);
    }

    #[test]
    fn advisories_are_listed_and_never_fail() {
        let found = [f("M130", MODEL, "14.5% of the surface lies more than 5 units beyond every hitbox capsule"), f("M081", "particles/x.vpcf_c", "colour x alpha x overbright 0.887, Valve's 0.621 (1.4x)")];
        let r = judge(&found, &[], Depth::Deep, Applicable { models: true, particles: true, sounds: false }, &Policy::default());
        assert_eq!(r.state, State::Pass);
        assert_eq!(r.advisories.iter().map(|x| x.rule).collect::<Vec<_>>(), vec!["M081", "M130"]);
        // a policy that makes M130 bounded: unallowed it is a violation, allowed within its share limit it holds
        let p = crate::policy::parse("[compliance]\nadvisory = [\"M081\"]\nm130_max_share = 0.2\n", "p").unwrap();
        assert_eq!(judge(&found, &[], Depth::Deep, MODELS, &p).state, State::Fail);
        let a = allow("M130\t*\t*\tthe skirt is wide on purpose and reads as cloth\n");
        let r = judge(&found, &a, Depth::Deep, MODELS, &p);
        assert_eq!(r.state, State::Pass, "{r:?}");
        assert_eq!(r.allowances[0].measures, vec![Measure { name: "wide_share", value: 0.145, limit: 0.2 }]);
    }

    #[test]
    fn a_check_that_could_not_run_is_unknown_never_pass() {
        // a replaced particle whose data did not decode (lint M144)
        let u = f("M144", "particles/abilities/lash/lash_flog.vpcf_c", "the particle's data did not decode, so its energy and lights were not compared with Valve's (M081, M082)");
        let r = judge(std::slice::from_ref(&u), &[], Depth::Deep, Applicable { particles: true, ..Default::default() }, &Policy::default());
        assert_eq!((r.state, r.state.mark()), (State::Unknown, Some("fair play not checked")));
        assert_eq!(r.unknown[0].rules, vec!["M081", "M082"]);
        assert!(r.checks_run.is_empty());
        // an allowance cannot make an unknown known
        let a = allow("M144\t*\t*\twe looked at it in game and it is fine\n");
        assert_eq!(judge(std::slice::from_ref(&u), &a, Depth::Deep, Applicable { particles: true, ..Default::default() }, &Policy::default()).state, State::Unknown);
        // a light lint over a model: the mesh rules did not run
        let r = judge(&[], &[], Depth::Light, MODELS, &Policy::default());
        assert_eq!(r.state, State::Unknown);
        assert_eq!(r.unknown[0].rules, vec!["M124", "M129", "M130", "M140"]);
        assert_eq!(r.checks_run, vec!["M062", "M131", "M152"]);
        // a light lint of a mod with no model is complete: nothing deep was called for
        assert_eq!(judge(&[], &[], Depth::Light, Applicable { sounds: true, ..Default::default() }, &Policy::default()).state, State::Pass);
        // a violation outranks an unknown
        let r = judge(&[u, f("M129", MODEL, "x")], &[], Depth::Deep, Applicable { models: true, particles: true, sounds: false }, &Policy::default());
        assert_eq!(r.state, State::Fail);
    }

    #[test]
    fn a_lost_bone_is_not_fair_play_but_a_changed_hitbox_is() {
        let r = judge(&[f("M062", MODEL, "bone 'pelvis' missing")], &[], Depth::Deep, MODELS, &Policy::default());
        assert_eq!(r.state, State::Pass, "a lint error: the mod is broken, not unfair");
        let r = judge(&[f("M062", MODEL, "hitbox 'default/head_capsule' changed: 7.5 (Valve's 6.7)")], &[], Depth::Deep, MODELS, &Policy::default());
        assert_eq!(r.state, State::Fail);
    }
}
