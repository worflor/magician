//! The grade record: a measured claim about separate properties of a mod, and the label that projects it.
//!
//! ```text
//! <letter> <word>[ · NSFR | · FP?][ · stale(<last proven build>) | · unproven]
//! not graded (<kind>)[ · NSFR | · FP?][ · stale(<build>) | · unproven]
//! ```
//!
//! Three axes that never touch each other: **depth** (the letter, within the archetype), **compliance** (fair play:
//! pass, fail `NSFR`, unknown `FP?`) and **freshness** (proven on the target build, or the last build it was). The
//! **release gate** is separate from the label: it lists what stops automated packaging or publishing. Whoever reads
//! it years later must be able to explain any grade from the record alone, so every criterion, signal, rule, allowance,
//! build and blocker behind the label is in it, with the policy's fingerprint and the toolchain that judged it.

use crate::compliance::{self, FairPlay, State};
use crate::footprint::Footprint;
use crate::identity::{self, Identity};
use crate::json::{arr, num, s, Obj};
use crate::lintfacts::LintFacts;
use crate::policy::Policy;
use crate::rubric::{self, Report};

pub const SCHEMA: &str = "magician.grade/2";

/// One pak of the mod, as judged.
#[derive(Debug, Clone)]
pub struct Pak {
    pub name: String,
    pub sha256: String,
    pub files: usize,
}

/// A person's verdict on the rights to what the mod ships (`mod.toml [rights] state`). Byte identity can't settle
/// licensing (processed Valve audio, modified Valve assets), so Magician never decides it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rights {
    Unreviewed,
    Cleared,
    Denied,
}

impl Rights {
    pub fn parse(state: Option<&str>) -> Result<Rights, String> {
        match state {
            None | Some("unreviewed") => Ok(Rights::Unreviewed),
            Some("cleared") => Ok(Rights::Cleared),
            Some("denied") => Ok(Rights::Denied),
            Some(other) => Err(format!("mod.toml [rights] state = \"{other}\": it is \"unreviewed\", \"cleared\" or \"denied\"")),
        }
    }
    pub fn id(self) -> &'static str {
        match self {
            Rights::Unreviewed => "unreviewed",
            Rights::Cleared => "cleared",
            Rights::Denied => "denied",
        }
    }
}

/// Is the mod proven on the build it is judged for? Asked of the bytes graded: `proven` holds the builds a recorded
/// proof holds these paks on (the CLI reads the mod's proof ledger); `unbound` the builds its `[proven] builds` claims,
/// or its ledger records, for other bytes or without a record, which count for nothing here and are said.
#[derive(Debug, Clone, PartialEq)]
pub struct Freshness {
    pub proven: Vec<u64>,
    pub last_proven: Option<u64>,
    pub target: u64,
    pub unbound: Vec<u64>,
}

fn builds(v: &[String]) -> Vec<u64> {
    let mut p: Vec<u64> = v.iter().filter_map(|b| b.parse().ok()).collect();
    p.sort();
    p.dedup();
    p
}

impl Freshness {
    pub fn new(proven: &[String], target: &str) -> Freshness {
        Freshness::bound(proven, &[], target)
    }
    /// `proven` bound to the bytes, `unbound` claimed for other bytes or never recorded.
    pub fn bound(proven: &[String], unbound: &[String], target: &str) -> Freshness {
        let p = builds(proven);
        let u: Vec<u64> = builds(unbound).into_iter().filter(|b| !p.contains(b)).collect();
        Freshness { last_proven: p.last().copied(), proven: p, target: target.parse().unwrap_or(0), unbound: u }
    }
    fn unbound_note(&self) -> String {
        if self.unbound.is_empty() {
            String::new()
        } else {
            format!("; {} claimed for other bytes or never recorded", self.unbound.iter().map(|b| b.to_string()).collect::<Vec<_>>().join(", "))
        }
    }
    pub fn current(&self) -> bool {
        self.proven.contains(&self.target)
    }
    /// The label's mark: nothing when current, `stale(<last proven>)`, or `unproven` with no proof on record.
    pub fn mark(&self) -> Option<String> {
        match (self.current(), self.last_proven) {
            (true, _) => None,
            (false, Some(b)) => Some(format!("stale({b})")),
            (false, None) => Some("unproven".into()),
        }
    }
}

/// Everything the record is made from. The CLI reads files into this; the rest is pure.
pub struct Inputs<'a> {
    pub toolchain: String,
    pub build: String,
    pub baseline_fingerprint: String,
    pub policy: &'a Policy,
    pub slug: Option<String>,
    pub name: Option<String>,
    pub source_commit: Option<String>,
    pub paks: Vec<Pak>,
    pub footprint: &'a Footprint,
    pub facts: rubric::ModFacts,
    pub lint: &'a LintFacts,
    pub lint_source: String,
    pub fair_play: FairPlay,
    pub identity: Identity,
    /// Files byte-identical to Valve's at the same path: they change nothing, and shipping them hands out Valve's bytes.
    pub valve_files: Vec<String>,
    /// Files two paks both ship: the lower pak number wins in game, the others are dead weight.
    pub shadowed: Vec<String>,
    pub freshness: Freshness,
    pub rights: Rights,
    pub rights_notes: String,
    /// The allowance files the grade read, one line each (name, SHA-256, where it was found), or one line saying
    /// there were none: an unexplained NSFR is often a missing allowance file.
    pub allowance_files: Vec<String>,
    pub evidence: Vec<String>,
}

pub struct Record<'a> {
    pub i: Inputs<'a>,
    /// `None` for a mod that is not graded.
    pub depth: Option<Report>,
    pub label: String,
    pub blockers: Vec<String>,
    pub warnings: Vec<String>,
}

impl<'a> Record<'a> {
    pub fn gate_passes(&self) -> bool {
        self.blockers.is_empty()
    }
}

fn codes(by: &std::collections::BTreeMap<&'static str, usize>) -> String {
    by.iter().map(|(c, n)| format!("{c} x{n}")).collect::<Vec<_>>().join(", ")
}

fn first<T: AsRef<str>>(v: &[T]) -> String {
    v.first().map(|x| x.as_ref().to_string()).unwrap_or_default()
}

/// Judge: depth by the archetype identity chose, the label, and the release gate.
pub fn build(i: Inputs) -> Record {
    let depth = i.identity.kind.archetype().map(|a| rubric::grade(i.footprint, &i.facts, a, i.policy));
    let mut label = match (&depth, &i.identity.not_graded) {
        (Some(r), _) => format!("{} {}", r.letter, identity::word(i.identity.kind, r.letter)),
        (None, kind) => format!("not graded ({})", kind.as_deref().unwrap_or("not a hero mod")),
    };
    if let Some(m) = i.fair_play.state.mark() {
        label.push_str(&format!(" · {m}"));
    }
    if let Some(m) = i.freshness.mark() {
        label.push_str(&format!(" · {m}"));
    }
    let mut blockers = Vec::new();
    let mut warnings = Vec::new();
    if i.lint.errors > 0 {
        blockers.push(format!("lint: {} error(s) that are not fair play ({})", i.lint.errors, codes(&i.lint.errors_by_code)));
    }
    if !i.valve_files.is_empty() {
        blockers.push(format!("valve files: {} file(s) byte-identical to Valve's at the same path (e.g. {})", i.valve_files.len(), first(&i.valve_files)));
    }
    match i.fair_play.state {
        State::Unknown => {
            let rules: Vec<String> = {
                let mut r: Vec<String> = i.fair_play.unknown.iter().flat_map(|u| u.rules.clone()).collect();
                r.sort();
                r.dedup();
                r
            };
            let text = format!("fair play: unknown ({} not run: {})", rules.join(", "), i.fair_play.unknown.first().map(|u| u.why.as_str()).unwrap_or(""));
            if i.policy.enforce {
                blockers.push(text);
            } else {
                warnings.push(format!("{text}; not blocking: fair play is not enforced (fair_play.enforce = false)"));
            }
        }
        State::Fail => {
            let v = &i.fair_play.violations[0];
            let text = format!("fair play: NSFR ({} on {}: {}{})", v.rule, v.path.as_deref().unwrap_or("the pak"), v.why, if i.fair_play.violations.len() > 1 { format!("; {} more", i.fair_play.violations.len() - 1) } else { String::new() });
            if i.policy.enforce {
                blockers.push(text);
            } else {
                warnings.push(format!("{text}; not blocking: fair play is not enforced (fair_play.enforce = false)"));
            }
        }
        State::Pass => {}
    }
    if !i.freshness.current() {
        blockers.push(match i.freshness.last_proven {
            Some(b) => format!("freshness: not proven on {} (last proven on {b}{})", i.freshness.target, i.freshness.unbound_note()),
            None => format!("freshness: not proven on {} (no proven build on record for these bytes{})", i.freshness.target, i.freshness.unbound_note()),
        });
    }
    match i.rights {
        Rights::Cleared => {}
        Rights::Unreviewed if !i.policy.rights_enforce => warnings.push("rights: unreviewed; not blocking: rights are not enforced (rights.enforce = false)".to_string()),
        r => blockers.push(format!("rights: {}", r.id())),
    }
    if !i.shadowed.is_empty() {
        warnings.push(format!("{} file(s) are in more than one pak; the lowest pak number wins (e.g. {})", i.shadowed.len(), first(&i.shadowed)));
    }
    Record { i, depth, label, blockers, warnings }
}

fn item_json(x: &compliance::Item) -> String {
    Obj::new()
        .str("rule", x.rule)
        .opt_str("path", x.path.as_deref())
        .str("message", &x.message)
        .opt_str("reason", x.reason.as_deref())
        .raw("measures", arr(x.measures.iter().map(|m| Obj::new().str("name", m.name).num("value", m.value).num("limit", m.limit).build())))
        .str("why", &x.why)
        .build()
}

/// A score to one decimal, rounded exactly as the text report rounds it (93.25 is 93.2 in both).
fn tenth(x: f64) -> String {
    num(format!("{x:.1}").parse::<f64>().unwrap_or(x))
}

/// The ten most useful missing items of a criterion, and how many there are.
const MISSING_SHOWN: usize = 10;

impl<'a> Record<'a> {
    /// Next steps: the largest weighted gains first, among the criteria that count.
    pub fn next_steps(&self) -> Vec<(f64, &'static str, &'static str, Vec<String>)> {
        let Some(r) = &self.depth else { return Vec::new() };
        let total: f64 = r.lines.iter().filter(|l| l.counted).map(|l| l.weight).sum();
        let mut gaps: Vec<(f64, &'static str, &'static str, Vec<String>)> = r
            .lines
            .iter()
            .filter(|l| l.counted && l.coverage < 1.0 - 1e-9 && total > 0.0)
            .map(|l| (100.0 * l.weight * (1.0 - l.coverage) / total, l.id, l.name, l.missing.iter().take(3).cloned().collect()))
            .collect();
        gaps.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal).then(a.1.cmp(b.1)));
        gaps.truncate(5);
        gaps
    }

    pub fn json(&self) -> String {
        let i = &self.i;
        let mod_json = Obj::new()
            .opt_str("slug", i.slug.as_deref())
            .opt_str("name", i.name.as_deref())
            .opt_str("source_commit", i.source_commit.as_deref())
            .raw("paks", arr(i.paks.iter().map(|p| Obj::new().str("name", &p.name).str("sha256", &p.sha256).int("files", p.files as i64).build())))
            .build();
        let policy = Obj::new()
            .str("fingerprint", &i.policy.fingerprint())
            .opt_str("file", i.policy.file.as_deref())
            .strs("overrides", &i.policy.overrides())
            .bool("fair_play_enforced", i.policy.enforce)
            .bool("rights_enforced", i.policy.rights_enforce)
            .build();
        let lint = Obj::new()
            .str("depth", i.lint.depth.label())
            .str("source", &i.lint_source)
            .int("errors", i.lint.errors as i64)
            .raw("errors_by_code", Obj(i.lint.errors_by_code.iter().map(|(c, n)| (c.to_string(), n.to_string())).collect()).build())
            .int("warnings", i.lint.warnings as i64)
            .int("dropped_deps", i.lint.dropped_deps as i64)
            .int("orphans", i.lint.orphans as i64)
            .raw("allowed_by_code", Obj(i.lint.allowed_by_code.iter().map(|(c, n)| (c.to_string(), n.to_string())).collect()).build())
            .build();
        let depth = match &self.depth {
            None => "null".to_string(),
            Some(r) => Obj::new()
                .str("letter", r.letter)
                .raw("score", tenth(r.score))
                .str("uncapped_letter", r.uncapped)
                .str("archetype_rubric", r.archetype.id())
                .raw(
                    "criteria",
                    arr(r.lines.iter().map(|l| {
                        Obj::new()
                            .str("id", l.id)
                            .num("weight", l.weight)
                            .bool("counted", l.counted)
                            .num("coverage", l.coverage)
                            .num("points", l.weight * l.coverage)
                            .str("detail", &l.detail)
                            .strs("missing", &l.missing.iter().take(MISSING_SHOWN).collect::<Vec<_>>())
                            .int("missing_count", l.missing.len() as i64)
                            .build()
                    })),
                )
                .raw("caps", arr(r.caps.iter().map(|c| Obj::new().str("max", c.max).str("why", &c.why).build())))
                .build(),
        };
        let identity = Obj::new()
            .str("kind", i.identity.kind.id())
            .strs("signals", &i.identity.signals)
            .num("score", i.identity.score)
            .num("required", i.identity.required)
            .raw("evidence", Obj(i.identity.evidence.iter().map(|(k, v)| (k.to_string(), s(v))).collect()).build())
            .opt_str("not_graded", i.identity.not_graded.as_deref())
            .build();
        let fp = &i.fair_play;
        let fair_play = Obj::new()
            .str("state", fp.state.id())
            .strs("checks_run", &fp.checks_run)
            .raw("checks_unknown", arr(fp.unknown.iter().map(|u| Obj::new().strs("rules", &u.rules).opt_str("path", u.path.as_deref()).str("why", &u.why).build())))
            .raw("violations", arr(fp.violations.iter().map(item_json)))
            .raw("allowances", arr(fp.allowances.iter().map(item_json)))
            .raw("advisories", arr(fp.advisories.iter().map(item_json)))
            .build();
        let f = &i.freshness;
        let freshness = Obj::new()
            .raw("proven", arr(f.proven.iter().map(|b| b.to_string())))
            .raw("last_proven", f.last_proven.map_or("null".into(), |b| b.to_string()))
            .int("target", f.target as i64)
            .bool("current", f.current())
            .raw("unbound", arr(f.unbound.iter().map(|b| b.to_string())))
            .build();
        let next = arr(self.next_steps().into_iter().map(|(g, id, _, items)| Obj::new().raw("gain", tenth(g)).str("criterion", id).strs("items", &items).build()));
        Obj::new()
            .str("schema", SCHEMA)
            .str("rules_version", rubric::RULES_VERSION)
            .str("toolchain", &i.toolchain)
            .raw("deadlock_build", i.build.parse::<u64>().map_or_else(|_| s(&i.build), |b| b.to_string()))
            .str("baseline_fingerprint", &i.baseline_fingerprint)
            .raw("policy", policy)
            .raw("mod", mod_json)
            .raw("hero", Obj::new().str("key", &i.footprint.hero_key).str("name", &i.footprint.display).int("footprint_files", i.footprint.items.len() as i64).build())
            .raw("lint", lint)
            .raw("depth", depth)
            .raw("identity", identity)
            .raw("fair_play", fair_play)
            .raw("freshness", freshness)
            .raw("rights", Obj::new().str("state", i.rights.id()).str("notes", &i.rights_notes).build())
            .raw("release_gate", Obj::new().bool("pass", self.gate_passes()).strs("blockers", &self.blockers).strs("warnings", &self.warnings).build())
            .strs("valve_files", &i.valve_files)
            .strs("shadowed", &i.shadowed)
            .strs("allowance_files", &i.allowance_files)
            .strs("evidence", &i.evidence)
            .raw("next_steps", next)
            .str("label", &self.label)
            .build()
    }

    /// The human report.
    pub fn text(&self) -> String {
        let i = &self.i;
        let mut o = String::new();
        let mut line = |t: String| {
            o.push_str(&t);
            o.push('\n');
        };
        let paks: Vec<&str> = i.paks.iter().map(|p| p.name.as_str()).collect();
        line(format!("{} grade for {}  (build {}, footprint {} files, policy {})", i.footprint.display, paks.join(" + "), i.build, i.footprint.items.len(), i.policy.file.as_deref().unwrap_or("built-in")));
        line(format!("  label: {}", self.label));
        line(String::new());
        match &self.depth {
            Some(r) => {
                line(format!("depth: {} {:.1} ({} rubric{})", r.letter, r.score, r.archetype.id(), if r.letter != r.uncapped { format!("; {} without caps", r.uncapped) } else { String::new() }));
                for l in &r.lines {
                    let bar = |x: f64| {
                        let n = (x * 20.0).round() as usize;
                        format!("{}{}", "#".repeat(n.min(20)), ".".repeat(20 - n.min(20)))
                    };
                    let w = if l.counted { format!("{:>5.1}/{:<3.0}", l.weight * l.coverage, l.weight) } else { "  not counted".to_string() };
                    line(format!("  {:<22} [{}] {w} {}", l.name, bar(l.coverage), l.detail));
                }
                for c in &r.caps {
                    line(format!("  max {}: {}", c.max, c.why));
                }
            }
            None => line(format!("depth: not graded: {}; there is no rubric for it yet", i.identity.not_graded.as_deref().unwrap_or("not a hero mod"))),
        }
        line(String::new());
        line(format!("identity: {} ({} signal(s), {} for a new character: {})", i.identity.kind.id(), num(i.identity.score), num(i.identity.required), if i.identity.signals.is_empty() { "none".to_string() } else { i.identity.signals.join(", ") }));
        for (k, v) in &i.identity.evidence {
            line(format!("  {k}: {v}"));
        }
        let fp = &i.fair_play;
        line(format!("fair play: {}  (checks run: {})", fp.state.id(), if fp.checks_run.is_empty() { "none".to_string() } else { fp.checks_run.join(", ") }));
        for u in &fp.unknown {
            line(format!("  not run: {} {}: {}", u.rules.join(", "), u.path.as_deref().unwrap_or(""), u.why));
        }
        for v in &fp.violations {
            line(format!("  violation {} {}: {}", v.rule, v.path.as_deref().unwrap_or("(vpk)"), v.why));
        }
        for a in &fp.allowances {
            let m: Vec<String> = a.measures.iter().map(|m| format!("{} {} (limit {})", m.name, num(m.value), num(m.limit))).collect();
            line(format!("  allowed {} {}: {} -- {}", a.rule, a.path.as_deref().unwrap_or("(vpk)"), if m.is_empty() { "by reason".to_string() } else { m.join(", ") }, a.reason.as_deref().unwrap_or("")));
        }
        for a in &fp.advisories {
            line(format!("  advisory {} {}: {}", a.rule, a.path.as_deref().unwrap_or("(vpk)"), a.message.chars().take(140).collect::<String>()));
        }
        let f = &i.freshness;
        line(format!(
            "freshness: these bytes proven on {}; target {}{}{}",
            if f.proven.is_empty() { "no build".to_string() } else { f.proven.iter().map(|b| b.to_string()).collect::<Vec<_>>().join(", ") },
            f.target,
            if f.current() { " (current)" } else { "" },
            f.unbound_note()
        ));
        line(format!("rights: {}{}", i.rights.id(), if i.rights_notes.is_empty() { String::new() } else { format!(" ({})", i.rights_notes) }));
        line(format!("lint: {} ({}), {} error(s) not fair play, {} warning(s), {} allowed", i.lint.depth.label(), i.lint_source, i.lint.errors, i.lint.warnings, i.lint.allowed_by_code.values().sum::<usize>()));
        for a in &i.allowance_files {
            line(format!("  {a}"));
        }
        line(format!("release gate: {}", if self.gate_passes() { "pass".to_string() } else { format!("blocked ({})", self.blockers.len()) }));
        for b in &self.blockers {
            line(format!("  blocker: {b}"));
        }
        for w in &self.warnings {
            line(format!("  warning: {w}"));
        }
        let steps = self.next_steps();
        if !steps.is_empty() {
            line(String::new());
            line("Next steps (largest gains first):".into());
            for (g, _, name, items) in steps {
                line(format!("  +{g:.1}  {name}"));
                for m in items {
                    line(format!("         - {m}"));
                }
            }
        }
        o
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compliance::{judge, Applicable};
    use crate::footprint::{Cat, Item};
    use crate::identity::{decide, Signals};
    use crate::lintfacts::lint_facts;
    use lint::report::Depth;
    use lint::Finding;

    const MODEL: &str = "models/heroes_wip/lash/lash.vmdl_c";

    fn fp() -> Footprint {
        let mut f = Footprint { hero_key: "hero_lash".into(), display: "Lash".into(), tokens: vec!["lash".into()], ..Default::default() };
        f.abilities = vec![("Signature_1".into(), "strike".into())];
        let mut add = |cat, g: &str, p: String| f.items.push(Item { cat, group: g.into(), path: p });
        add(Cat::Model, "core", MODEL.into());
        for i in 0..4 {
            add(Cat::AbilityVfx, "strike", format!("particles/abilities/lash/strike_{i}.vpcf_c"));
            add(Cat::Sound, "strike", format!("sounds/abilities/lash/strike_{i}.vsnd_c"));
            add(Cat::Voice, "vo:kill", format!("sounds/vo/lash/lash_kill_{i}.vsnd_c"));
            add(Cat::Animation, "clips:abilities", format!("models/heroes_staging/lash_v2/clips/ability_{i}.vnmclip_c"));
        }
        add(Cat::Animation, "clips:ui", "models/heroes_staging/lash_v2/clips/ui_a.vnmclip_c".into());
        add(Cat::UiArt, "ui", "panorama/images/heroes/lash_card_psd.vtex_c".into());
        f
    }

    fn finding(code: &str, message: &str) -> Finding {
        let r = lint::rule(code);
        Finding { code: r.code, name: r.name, severity: r.severity, path: Some(MODEL.into()), message: message.into() }
    }

    /// A case to judge: which paths the mod ships, its lint run, its signals and its manifest's facts.
    struct Case {
        paths: Vec<String>,
        findings: Vec<Finding>,
        depth: Depth,
        signals: Signals,
        proven: Vec<String>,
        unbound: Vec<String>,
        target: &'static str,
        rights: Rights,
        valve_files: Vec<String>,
        policy: Policy,
    }

    impl Case {
        fn complete() -> Case {
            let mut paths: Vec<String> = fp().items.iter().map(|i| i.path.clone()).collect();
            paths.extend((0..3).map(|i| format!("models/heroes_wip/lash/materials/m{i}.vmat_c")));
            Case {
                paths,
                findings: vec![],
                depth: Depth::Deep,
                signals: Signals { model_replaced: true, portrait: (1, 1), ..Default::default() },
                proven: vec!["6864".into()],
                unbound: vec![],
                target: "6864",
                rights: Rights::Cleared,
                valve_files: vec![],
                policy: Policy::default(),
            }
        }
        fn run<T>(self, then: impl FnOnce(&Record) -> T) -> T {
            let f = fp();
            let lint = lint_facts(self.findings.clone(), &[], self.depth);
            let mut facts = rubric::ModFacts { paths: self.paths.into_iter().collect(), model_anim_changed: true, ..Default::default() };
            facts.lint_errors = lint.errors;
            facts.lint_warnings = lint.warnings;
            let fair = judge(&self.findings, &[], self.depth, Applicable { models: facts.paths.contains(MODEL), particles: false, sounds: false }, &self.policy);
            let identity = decide(&self.signals, &self.policy);
            let r = build(Inputs {
                toolchain: "magician test".into(),
                build: self.target.into(),
                baseline_fingerprint: "0".repeat(64),
                policy: &self.policy,
                slug: Some("maid-lash".into()),
                name: None,
                source_commit: None,
                paks: vec![Pak { name: "pak10_dir.vpk".into(), sha256: "a".repeat(64), files: facts.paths.len() }],
                footprint: &f,
                facts,
                lint: &lint,
                lint_source: "linted here".into(),
                fair_play: fair,
                identity,
                valve_files: self.valve_files,
                shadowed: vec![],
                freshness: Freshness::bound(&self.proven, &self.unbound, self.target),
                rights: self.rights,
                rights_notes: String::new(),
                allowance_files: vec![],
                evidence: vec![],
            });
            then(&r)
        }
    }

    fn renamed() -> Signals {
        Signals { model_replaced: true, renamed: vec!["replaces the name logo x".into()], portrait: (1, 1), ..Default::default() }
    }

    #[test]
    fn a_complete_cleared_current_costume_is_s_costume_and_passes_the_gate() {
        Case::complete().run(|r| {
            assert_eq!(r.label, "S Costume");
            assert!(r.gate_passes(), "{:?}", r.blockers);
            let j = r.json();
            assert!(j.starts_with("{\"schema\":\"magician.grade/2\",\"rules_version\":\"2.0.0\",\"toolchain\":\"magician test\",\"deadlock_build\":6864,"), "{j}");
            assert!(j.contains("\"label\":\"S Costume\"}") && j.contains("\"release_gate\":{\"pass\":true,\"blockers\":[],\"warnings\":[]}"), "{j}");
            assert_eq!(j, r.json(), "the record is deterministic");
        });
    }

    #[test]
    fn a_new_character_below_s_is_an_echolite_and_becomes_an_echo_at_s() {
        let mut c = Case::complete();
        c.signals = renamed();
        c.run(|r| assert_eq!(r.label, "S Echo"));
        let mut c = Case::complete();
        c.signals = renamed();
        c.paths.retain(|p| !p.contains("strike") && !p.contains("/vo/"));
        c.run(|r| {
            assert!(r.label.ends_with(" Echolite") && !r.label.starts_with('S'), "{}", r.label);
            assert!(r.json().contains("\"identity\":{\"kind\":\"new\",\"signals\":[\"renamed\",\"new_hero_art\"],\"score\":2,\"required\":2"), "{}", r.json());
        });
    }

    #[test]
    fn fair_play_marks_the_label_without_touching_the_letter() {
        // NSFR: the egregious envelope (fair play enforced: it blocks the gate)
        let mut c = Case::complete();
        c.findings = vec![finding("M129", "x")];
        c.policy = enforced();
        c.run(|r| {
            assert_eq!(r.label, "S Costume · NSFR");
            assert!(r.blockers.iter().any(|b| b.starts_with("fair play: NSFR (M129")), "{:?}", r.blockers);
            assert!(r.depth.as_ref().unwrap().caps.is_empty(), "no depth cap from fair play");
        });
        // FP?: a check that could not run
        let mut c = Case::complete();
        c.findings = vec![finding("M144", "the model's meshes could not be read, so its silhouette and hitbox envelope were not judged (M124, M129, M140)")];
        c.policy = enforced();
        c.run(|r| {
            assert_eq!(r.label, "S Costume · FP?");
            assert!(r.blockers.iter().any(|b| b.starts_with("fair play: unknown (M124, M129, M140 not run")), "{:?}", r.blockers);
            assert!(r.json().contains("\"fair_play_enforced\":true"), "{}", r.json());
        });
        // lenient (the default): NSFR and FP? are warnings at the gate; the label and the record say the same
        for (code, mark, warning) in [("M129", "NSFR", "fair play: NSFR (M129"), ("M144", "FP?", "fair play: unknown (M124")] {
            let mut c = Case::complete();
            c.findings = vec![finding(code, "the model's meshes could not be read (M124)")];
            c.run(|r| {
                assert_eq!(r.label, format!("S Costume · {mark}"), "the label still says it");
                assert!(r.gate_passes(), "{:?}", r.blockers);
                assert!(r.warnings[0].starts_with(warning) && r.warnings[0].ends_with("not blocking: fair play is not enforced (fair_play.enforce = false)"), "{:?}", r.warnings);
                assert!(r.json().contains("\"fair_play_enforced\":false"), "{}", r.json());
                assert!(r.json().contains(&format!("\"fair_play\":{{\"state\":\"{}\"", if mark == "NSFR" { "fail" } else { "unknown" })), "{}", r.json());
            });
        }
    }

    /// Fair play and rights both enforced.
    fn enforced() -> Policy {
        crate::policy::parse("[fair_play]\nenforce = true\n[rights]\nenforce = true\n", "strict.toml").unwrap()
    }

    #[test]
    fn stale_shows_the_last_proven_build_when_the_target_is_ahead() {
        let mut c = Case::complete();
        c.target = "6865";
        c.run(|r| {
            assert_eq!(r.label, "S Costume · stale(6864)");
            assert!(r.blockers.contains(&"freshness: not proven on 6865 (last proven on 6864)".to_string()), "{:?}", r.blockers);
            assert!(r.json().contains("\"freshness\":{\"proven\":[6864],\"last_proven\":6864,\"target\":6865,\"current\":false,\"unbound\":[]}"), "{}", r.json());
        });
        let mut c = Case::complete();
        c.proven = vec![];
        c.run(|r| {
            assert_eq!(r.label, "S Costume · unproven");
            assert!(r.blockers.iter().any(|b| b.contains("no proven build on record")));
        });
        // fair play comes before freshness in the label
        let mut c = Case::complete();
        c.findings = vec![finding("M144", "could not be read (M124)")];
        c.target = "6865";
        c.run(|r| assert_eq!(r.label, "S Costume · FP? · stale(6864)"));
    }

    #[test]
    fn every_release_gate_blocker() {
        let blocked = |mut c: Case, starts: &str| {
            if c.policy == Policy::default() {
                c.policy = enforced();
            }
            c.run(|r| {
                assert!(!r.gate_passes());
                assert!(r.blockers.iter().any(|b| b.starts_with(starts)), "{starts}: {:?}", r.blockers);
            })
        };
        // lint errors that are not fair play
        let mut c = Case::complete();
        c.findings = vec![finding("M030", "needs 'x'")];
        blocked(c, "lint: 1 error(s) that are not fair play (M030 x1)");
        // Valve's own files redistributed
        let mut c = Case::complete();
        c.valve_files = vec!["materials/x.vtex_c".into()];
        blocked(c, "valve files: 1 file(s) byte-identical to Valve's at the same path (e.g. materials/x.vtex_c)");
        // fair play unknown, and failed
        let mut c = Case::complete();
        c.depth = Depth::Light;
        blocked(c, "fair play: unknown");
        let mut c = Case::complete();
        c.findings = vec![finding("M124", "front view: our in-game silhouette is 90% of Valve's")];
        blocked(c, "fair play: NSFR (M124");
        // not proven on the target build
        let mut c = Case::complete();
        c.target = "6865";
        blocked(c, "freshness: not proven on 6865");
        // rights not cleared
        for (rights, text) in [(Rights::Unreviewed, "rights: unreviewed"), (Rights::Denied, "rights: denied")] {
            let mut c = Case::complete();
            c.rights = rights;
            blocked(c, text);
        }
        // lenient (the default): unreviewed rights warn; denied blocks whatever the policy says
        let mut c = Case::complete();
        c.rights = Rights::Unreviewed;
        c.run(|r| {
            assert!(r.gate_passes(), "{:?}", r.blockers);
            assert_eq!(r.warnings, vec!["rights: unreviewed; not blocking: rights are not enforced (rights.enforce = false)".to_string()]);
        });
        let mut c = Case::complete();
        c.rights = Rights::Denied;
        c.run(|r| assert!(r.blockers.contains(&"rights: denied".to_string()), "{:?}", r.blockers));
        // what blocks in every policy: lint errors that are not fair play, Valve's files, freshness, denied rights
        let mut c = Case::complete();
        c.findings = vec![finding("M030", "needs 'x'")];
        c.run(|r| assert!(!r.gate_passes()));
        let mut c = Case::complete();
        c.target = "6865";
        c.run(|r| assert!(!r.gate_passes()));
        assert_eq!(Rights::parse(None), Ok(Rights::Unreviewed));
        assert!(Rights::parse(Some("maybe")).unwrap_err().contains("\"unreviewed\", \"cleared\" or \"denied\""));
    }

    #[test]
    fn a_light_lint_over_a_model_is_fp_and_says_which_rules_did_not_run() {
        let mut c = Case::complete();
        c.depth = Depth::Light;
        c.run(|r| {
            assert_eq!(r.label, "S Costume · FP?");
            assert!(r.text().contains("not run: M124, M129, M130, M140"), "{}", r.text());
        });
    }

    #[test]
    fn the_record_rounds_a_score_as_the_text_does() {
        assert_eq!((tenth(93.25), format!("{:.1}", 93.25)), ("93.2".to_string(), "93.2".to_string()));
        assert_eq!(tenth(94.58), "94.6");
        assert_eq!(tenth(90.0), "90");
    }

    #[test]
    fn a_mod_that_touches_nothing_of_the_hero_is_not_graded() {
        let mut c = Case::complete();
        c.paths = vec!["panorama/layout/hud.vxml_c".into()];
        c.signals = Signals { other_kind: "HUD mod".into(), ..Default::default() };
        c.run(|r| {
            assert_eq!(r.label, "not graded (HUD mod)");
            assert!(r.depth.is_none() && r.json().contains("\"depth\":null"), "{}", r.json());
            assert!(r.text().contains("there is no rubric for it yet"));
        });
    }
}
