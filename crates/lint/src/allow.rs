//! Per-site allowances: the only way to accept a finding. Rules are never weakened to make a mod pass.
//!
//! An allowance file (`magician.allow`, at the addon root or passed with `--allow`) holds one line per
//! accepted finding, tab-separated:
//!
//! ```text
//! # code   path                                              match            reason
//! M061     particles/abilities/lash/lash_flog_whip.vpcf_c   whip_sparks.vpcf_c    plumes replace sparks (MOTIFS: feather duster)
//! ```
//!
//! - `code` is a rule code; `path` is the exact file the finding is on (`*` = the VPK itself);
//!   `match` is a substring the finding's message must contain (`*` = any).
//! - **The reason is mandatory.** An allowance without one is rejected.
//! - Allowed findings are still reported, with their reason, so evidence shows what was accepted and why.
//! - An allowance that matches nothing is **stale** and reported, so the file can't rot silently. One for a
//!   rule that didn't run (a deep-only mesh rule in a light lint) can't be judged, so it isn't called stale.

use crate::Finding;

#[derive(Debug, Clone, PartialEq)]
pub struct Allowance {
    pub code: String,
    pub path: String,
    pub needle: String,
    pub reason: String,
    pub line: usize,
}

pub fn parse(text: &str) -> Result<Vec<Allowance>, String> {
    let mut out = Vec::new();
    for (n, raw) in text.lines().enumerate() {
        let line = raw.trim_end();
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        let cols: Vec<&str> = line.split('\t').map(str::trim).filter(|c| !c.is_empty()).collect();
        if cols.len() < 4 {
            return Err(format!("allowance line {}: need code, path, match and a reason (tab-separated)", n + 1));
        }
        let reason = cols[3..].join(" ");
        if reason.len() < 8 {
            return Err(format!("allowance line {}: the reason must actually explain (got '{reason}')", n + 1));
        }
        out.push(Allowance { code: cols[0].into(), path: cols[1].into(), needle: cols[2].into(), reason, line: n + 1 });
    }
    Ok(out)
}

pub struct Applied {
    pub kept: Vec<Finding>,
    pub allowed: Vec<(Finding, String)>,
    pub stale: Vec<Allowance>,
}

fn matches(a: &Allowance, f: &Finding) -> bool {
    a.code == f.code
        && (a.path == "*" || f.path.as_deref() == Some(a.path.as_str()))
        && (a.needle == "*" || f.message.contains(&a.needle))
}

/// `deep`: whether the lint ran the deep mesh rules; without them their allowances are neither used nor stale.
pub fn apply(findings: Vec<Finding>, allowances: &[Allowance], deep: bool) -> Applied {
    let mut used = vec![false; allowances.len()];
    let mut kept = Vec::new();
    let mut allowed = Vec::new();
    for f in findings {
        match allowances.iter().position(|a| matches(a, &f)) {
            Some(i) => {
                used[i] = true;
                allowed.push((f, allowances[i].reason.clone()));
            }
            None => kept.push(f),
        }
    }
    let stale = allowances
        .iter()
        .zip(used)
        .filter(|(a, u)| !u && (deep || !crate::mesh::deep_only(&a.code)))
        .map(|(a, _)| a.clone())
        .collect();
    Applied { kept, allowed, stale }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{rule, Severity};

    fn f(code: &'static str, path: &str, msg: &str) -> Finding {
        let r = rule(code);
        Finding { code: r.code, name: r.name, severity: r.severity, path: Some(path.into()), message: msg.into() }
    }

    #[test]
    fn allowance_moves_a_finding_and_keeps_its_reason() {
        let a = parse("M061\tparticles/x.vpcf_c\tsparks.vmat_c\tplumes replace sparks (feather duster)\n").unwrap();
        let r = apply(vec![f("M061", "particles/x.vpcf_c", "original referenced 'materials/particle/sparks.vmat_c'")], &a, true);
        assert!(r.kept.is_empty());
        assert_eq!(r.allowed.len(), 1);
        assert!(r.allowed[0].1.contains("feather duster"));
        assert!(r.stale.is_empty());
    }

    #[test]
    fn allowance_is_specific_to_code_path_and_match() {
        let a = parse("M061\tparticles/x.vpcf_c\tsparks\tplumes replace sparks (feather duster)\n").unwrap();
        let r = apply(
            vec![
                f("M061", "particles/y.vpcf_c", "original referenced 'sparks'"), // other path
                f("M061", "particles/x.vpcf_c", "original referenced 'smoke'"),  // other target
                f("M030", "particles/x.vpcf_c", "needs 'sparks'"),               // other rule
            ],
            &a,
            false,
        );
        assert_eq!(r.kept.len(), 3);
        assert_eq!(r.stale.len(), 1, "an allowance that matched nothing is stale");
    }

    #[test]
    fn a_reason_is_mandatory() {
        assert!(parse("M061\tparticles/x.vpcf_c\t*\n").is_err());
        assert!(parse("M061\tparticles/x.vpcf_c\t*\tok\n").is_err(), "a one-word reason isn't a reason");
    }

    #[test]
    fn errors_can_be_allowed_but_stay_visible() {
        let a = parse("M030\t*\t*\tknown engine-internal path verified in-game on 6861\n").unwrap();
        let r = apply(vec![f("M030", "a.vmat_c", "needs 'x'")], &a, true);
        assert_eq!(r.allowed[0].0.severity, Severity::Error, "an allowed error is still recorded as an error");
    }

    #[test]
    fn a_deep_rule_allowance_is_not_stale_in_a_light_lint() {
        let a = parse("M140	m.vmdl_c	*	the bell skirt's flare is the silhouette
M123	m.vmdl_c	*	budget accepted for the lace hem
").unwrap();
        let light = apply(Vec::new(), &a, false);
        assert_eq!(light.stale.iter().map(|x| x.code.as_str()).collect::<Vec<_>>(), ["M123"], "M140 didn't run; M123 did");
        assert_eq!(apply(Vec::new(), &a, true).stale.len(), 2, "a deep lint judges both");
    }
}
