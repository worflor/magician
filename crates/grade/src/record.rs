//! The grade record: a measured claim about separate properties of a mod, and the label that projects it.
//!
//! ```text
//! <Kind> (<groups>)[ · broken][ · NSFR | · FP?][ · stale(<last proven build>) | · unproven]
//! not graded (<kind>)[ · broken][ · NSFR | · FP?][ · stale(<build>) | · unproven]
//! ```
//!
//! The description says what the mod is and what it makes its own: the kind (Echo, Costume, Variant, Kit) and the
//! groups (look, effects, sound; `some <group>` when begun, left out while Valve's). The flags never touch it:
//! **broken** (lint errors that are not fair play), **compliance** (fair play: pass, fail `NSFR`, unknown `FP?`) and
//! **freshness** (proven on the target build, or the last build it was). The **release gate** is separate from the
//! label: it lists what stops automated packaging or publishing. Whoever reads it years later must be able to explain
//! any grade from the record alone, so every criterion, signal, rule, allowance, build and blocker behind the label is
//! in it, with the policy's fingerprint and the toolchain that judged it.

use crate::compliance::{self, FairPlay, State};
use crate::footprint::Footprint;
use crate::identity::{self, Identity};
use crate::json::{arr, num, s, Obj};
use crate::lintfacts::LintFacts;
use crate::policy::Policy;
use crate::rubric::{self, Report};

pub const SCHEMA: &str = "magician.grade/3";
/// How many violations and advisories of one rule the text report shows before it counts the rest.
const VIOLATIONS_SHOWN: usize = 5;
/// How long a finding's message runs in the text report before it is cut, at a word, with "...".
const MESSAGE_SHOWN: usize = 180;

/// A finding's message for the text report: whole, or cut at the last word that fits (the record keeps it whole).
fn brief(m: &str) -> String {
    if m.chars().count() <= MESSAGE_SHOWN {
        return m.to_string();
    }
    let cut: String = m.chars().take(MESSAGE_SHOWN).collect();
    format!("{}...", cut.rsplit_once(' ').map(|(a, _)| a).unwrap_or(&cut).trim_end_matches([',', ';', ':']))
}
use std::collections::BTreeMap;

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
    /// What the mod makes its own; `None` for a mod that is not graded.
    pub description: Option<Report>,
    pub label: String,
    pub blockers: Vec<String>,
    pub warnings: Vec<String>,
}

impl<'a> Record<'a> {
    pub fn gate_passes(&self) -> bool {
        self.blockers.is_empty()
    }
    /// Lint errors that are not fair play: the `broken` flag, as `magician check` reads it too.
    pub fn broken(&self) -> bool {
        self.i.lint.broken()
    }
}

/// The marks after the description, in label order: `broken`, fair play's, freshness's.
fn marks(i: &Inputs) -> Vec<String> {
    let mut m = Vec::new();
    if i.lint.broken() {
        m.push("broken".to_string());
    }
    m.extend(i.fair_play.state.mark().map(str::to_string));
    m.extend(i.freshness.mark());
    m
}

fn codes(by: &std::collections::BTreeMap<&'static str, usize>) -> String {
    by.iter().map(|(c, n)| format!("{c} x{n}")).collect::<Vec<_>>().join(", ")
}

fn first<T: AsRef<str>>(v: &[T]) -> String {
    v.first().map(|x| x.as_ref().to_string()).unwrap_or_default()
}

/// Judge: what the mod makes its own, by the archetype identity chose, the label, and the release gate.
pub fn build(i: Inputs) -> Record {
    let description = i.identity.kind.archetype().map(|a| rubric::grade(i.footprint, &i.facts, a, i.policy));
    let mut label = match (&description, &i.identity.not_graded) {
        (Some(r), _) => format!("{} ({})", identity::word(i.identity.kind), r.description()),
        (None, kind) => format!("not graded ({})", kind.as_deref().unwrap_or("not a hero mod")),
    };
    for m in marks(&i) {
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
    Record { i, description, label, blockers, warnings }
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

/// The ten most useful missing items of a criterion, and how many there are.
const MISSING_SHOWN: usize = 10;

/// How a report paints a mark (`broken`, `NSFR`, `FP?`, `stale(N)`, `unproven`, `pass`): the CLI colours them on a
/// terminal, everything else keeps the words as they are.
pub type Paint<'p> = &'p dyn Fn(&str) -> String;

/// The label with its marks painted.
pub fn painted(label: &str, paint: Paint) -> String {
    let mut parts = label.split(" · ");
    let mut out = parts.next().unwrap_or("").to_string();
    for m in parts {
        out.push_str(" · ");
        out.push_str(&paint(m));
    }
    out
}

impl<'a> Record<'a> {
    /// What is still to make its own: the criteria that count and are not covered, the largest part of their group
    /// first, with their most useful missing items.
    pub fn next_steps(&self) -> Vec<(&'static str, &'static str, &'static str, Vec<String>)> {
        let Some(r) = &self.description else { return Vec::new() };
        let share = |l: &rubric::Line| {
            let total: f64 = r.lines.iter().filter(|x| x.counted && x.group == l.group).map(|x| x.weight).sum();
            if total > 0.0 { l.weight * (1.0 - l.coverage) / total } else { 0.0 }
        };
        let mut gaps: Vec<(f64, &rubric::Line)> = r.lines.iter().filter(|l| l.counted && l.coverage < 1.0 - 1e-9).map(|l| (share(l), l)).filter(|(g, _)| *g > 0.0).collect();
        gaps.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal).then(a.1.id.cmp(b.1.id)));
        gaps.truncate(5);
        gaps.into_iter().map(|(_, l)| (l.id, l.group.id(), l.name, l.missing.iter().take(3).cloned().collect())).collect()
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
        let description = match &self.description {
            None => "null".to_string(),
            Some(r) => Obj::new()
                .str("kind", identity::word(i.identity.kind))
                .str("rubric", r.archetype.id())
                .str("says", &r.description())
                .raw(
                    "groups",
                    arr(r.groups.iter().map(|g| {
                        Obj::new()
                            .str("id", g.group.id())
                            .str("state", g.state.id())
                            .num("coverage", g.coverage)
                            .strs("criteria", &r.lines.iter().filter(|l| l.group == g.group && l.counted).map(|l| l.id).collect::<Vec<_>>())
                            .str("counted", &g.counted)
                            .build()
                    })),
                )
                .raw(
                    "criteria",
                    arr(r.lines.iter().map(|l| {
                        Obj::new()
                            .str("id", l.id)
                            .str("group", l.group.id())
                            .num("weight", l.weight)
                            .bool("counted", l.counted)
                            .num("coverage", l.coverage)
                            .str("detail", &l.detail)
                            .strs("missing", &l.missing.iter().take(MISSING_SHOWN).collect::<Vec<_>>())
                            .int("missing_count", l.missing.len() as i64)
                            .build()
                    })),
                )
                .strs("notes", &r.notes)
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
        let next = arr(self.next_steps().into_iter().map(|(id, group, _, items)| Obj::new().str("criterion", id).str("group", group).strs("items", &items).build()));
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
            .bool("broken", self.broken())
            .raw("description", description)
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

    /// The human report; `paint` colours the marks (the CLI's terminal colours, or the words unchanged).
    pub fn text(&self, paint: Paint) -> String {
        let i = &self.i;
        let mut o = String::new();
        let mut line = |t: String| {
            o.push_str(&t);
            o.push('\n');
        };
        let paks: Vec<&str> = i.paks.iter().map(|p| p.name.as_str()).collect();
        line(format!("{} grade for {}  (build {}, footprint {} files, policy {})", i.footprint.display, paks.join(" + "), i.build, i.footprint.items.len(), i.policy.file.as_deref().unwrap_or("built-in")));
        line(format!("  label: {}", painted(&self.label, paint)));
        line(String::new());
        match &self.description {
            Some(r) => {
                line(format!("{}: {} ({} rubric)", identity::word(i.identity.kind), identity::means(i.identity.kind), r.archetype.id()));
                for g in &r.groups {
                    line(format!("  {:<8} {}", g.group.id(), g.counted));
                }
                for n in &r.notes {
                    line(format!("  note: {n}"));
                }
            }
            None => line(format!("not graded: {}; there is no rubric for it yet", i.identity.not_graded.as_deref().unwrap_or("not a hero mod"))),
        }
        if self.broken() {
            line(format!("{}: {} lint error(s) that are not fair play ({}): something won't load in game", paint("broken"), i.lint.errors, codes(&i.lint.errors_by_code)));
        }
        line(String::new());
        line(format!("identity: {} ({} signal(s), {} for a new character: {})", i.identity.kind.id(), num(i.identity.score), num(i.identity.required), if i.identity.signals.is_empty() { "none".to_string() } else { i.identity.signals.join(", ") }));
        for (k, v) in &i.identity.evidence {
            line(format!("  {k}: {v}"));
        }
        let fp = &i.fair_play;
        let state = match fp.state.mark() {
            Some(m) => format!("{} {}", fp.state.id(), paint(m)),
            None => paint(fp.state.id()),
        };
        line(format!("fair play: {state}  (checks run: {})", if fp.checks_run.is_empty() { "none".to_string() } else { fp.checks_run.join(", ") }));
        for u in &fp.unknown {
            line(format!("  not run: {} {}: {}", u.rules.join(", "), u.path.as_deref().unwrap_or(""), u.why));
        }
        // Each violation with what was measured; past the first few of one rule, a count (the record has every one).
        let mut shown: BTreeMap<&str, usize> = BTreeMap::new();
        for v in &fp.violations {
            let n = shown.entry(v.rule).or_default();
            *n += 1;
            if *n <= VIOLATIONS_SHOWN {
                line(format!("  violation {} {}: {} ({})", v.rule, v.path.as_deref().unwrap_or("(vpk)"), brief(&v.message), v.why));
            }
        }
        for (rule, n) in &shown {
            if *n > VIOLATIONS_SHOWN {
                line(format!("  and {} more {rule} violation(s): `magician lint` lists every one, the --json record too", n - VIOLATIONS_SHOWN));
            }
        }
        for a in &fp.allowances {
            let m: Vec<String> = a.measures.iter().map(|m| format!("{} {} (limit {})", m.name, num(m.value), num(m.limit))).collect();
            line(format!("  allowed {} {}: {} -- {}", a.rule, a.path.as_deref().unwrap_or("(vpk)"), if m.is_empty() { "by reason".to_string() } else { m.join(", ") }, a.reason.as_deref().unwrap_or("")));
        }
        let mut shown: BTreeMap<&str, usize> = BTreeMap::new();
        for a in &fp.advisories {
            let n = shown.entry(a.rule).or_default();
            *n += 1;
            if *n <= VIOLATIONS_SHOWN {
                line(format!("  advisory {} {}: {}", a.rule, a.path.as_deref().unwrap_or("(vpk)"), brief(&a.message)));
            }
        }
        for (rule, n) in &shown {
            if *n > VIOLATIONS_SHOWN {
                line(format!("  and {} more {rule} advisory(ies): `magician lint` lists every one, the --json record too", n - VIOLATIONS_SHOWN));
            }
        }
        let f = &i.freshness;
        line(format!(
            "freshness: these bytes proven on {}; target {}{}{}",
            if f.proven.is_empty() { "no build".to_string() } else { f.proven.iter().map(|b| b.to_string()).collect::<Vec<_>>().join(", ") },
            f.target,
            match f.mark() {
                None => " (current)".to_string(),
                Some(m) => format!(" ({})", paint(&m)),
            },
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
            line("Not yet its own (the largest part of its group first):".into());
            for (_, group, name, items) in steps {
                line(format!("  {name} ({group})"));
                for m in items {
                    line(format!("    - {m}"));
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
            add(Cat::Animation, "clips:abilities", format!("models/heroes_staging/lash_v2/clips/ability_{i}.vnmclip_c"));
        }
        for i in 0..40 {
            add(Cat::Voice, "vo:kill", format!("sounds/vo/lash/lash_kill_{i}.vsnd_c"));
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
            let facts = rubric::ModFacts { paths: self.paths.into_iter().collect(), model_anim_changed: true, dropped_deps: lint.dropped_deps, ..Default::default() };
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
    fn a_complete_cleared_current_costume_says_so_and_passes_the_gate() {
        Case::complete().run(|r| {
            assert_eq!(r.label, "Costume (look, effects, sound)");
            assert!(r.gate_passes(), "{:?}", r.blockers);
            let j = r.json();
            assert!(j.starts_with("{\"schema\":\"magician.grade/3\",\"rules_version\":\"3.1.0\",\"toolchain\":\"magician test\",\"deadlock_build\":6864,"), "{j}");
            assert!(j.contains("\"label\":\"Costume (look, effects, sound)\"}") && j.contains("\"release_gate\":{\"pass\":true,\"blockers\":[],\"warnings\":[]}"), "{j}");
            assert!(j.contains("\"broken\":false,\"description\":{\"kind\":\"Costume\",\"rubric\":\"character\",\"says\":\"look, effects, sound\",\"groups\":[{\"id\":\"look\",\"state\":\"covered\",\"coverage\":1,\"criteria\":[\"model\",\"materials\",\"ui\",\"scenes\"],\"counted\":\"model, materials, UI art, scenes\"}"), "{j}");
            assert!(!j.contains("\"letter\"") && !j.contains("\"score\":9") && !j.contains("\"caps\""), "no letter, no overall score: {j}");
            assert_eq!(j, r.json(), "the record is deterministic");
            let t = r.text(&|m| m.to_string());
            assert!(t.contains("Costume: the same character, a new model (character rubric)\n  look     model, materials, UI art, scenes\n  effects  ability VFX, animation\n  sound    kit sound, voice\n"), "{t}");
        });
    }

    #[test]
    fn a_new_character_is_an_echo_whatever_it_has_made_its_own() {
        let mut c = Case::complete();
        c.signals = renamed();
        c.run(|r| assert_eq!(r.label, "Echo (look, effects, sound)"));
        let mut c = Case::complete();
        c.signals = renamed();
        c.paths.retain(|p| !p.contains("strike") && !p.contains("/vo/"));
        c.run(|r| {
            assert_eq!(r.label, "Echo (look, some effects)");
            let t = r.text(&|m| m.to_string());
            assert!(t.contains("  effects  Valve's ability VFX, animation\n  sound    Valve's\n"), "{t}");
            assert!(r.json().contains("\"identity\":{\"kind\":\"new\",\"signals\":[\"renamed\",\"new_hero_art\"],\"score\":2,\"required\":2"), "{}", r.json());
        });
    }

    #[test]
    fn lint_errors_flag_broken_and_leave_the_description_alone() {
        let mut c = Case::complete();
        c.findings = vec![finding("M030", "needs 'x'")];
        c.run(|r| {
            assert_eq!(r.label, "Costume (look, effects, sound) · broken");
            assert!(r.broken() && r.json().contains("\"broken\":true"), "{}", r.json());
            let t = r.text(&|m| format!("<{m}>"));
            assert!(t.contains("  label: Costume (look, effects, sound) · <broken>\n"), "{t}");
            assert!(t.contains("<broken>: 1 lint error(s) that are not fair play (M030 x1): something won't load in game"), "{t}");
        });
        // a dropped dependency is a warning: a plain note, not a flag
        let mut c = Case::complete();
        c.findings = vec![finding("M061", "drops 'x'")];
        c.run(|r| {
            assert_eq!(r.label, "Costume (look, effects, sound)");
            assert!(r.text(&|m| m.to_string()).contains("  note: 1 dropped dependency (M061): effects or sounds of Valve's originals that no longer play\n"));
            assert!(r.json().contains("\"notes\":[\"1 dropped dependency (M061)"), "{}", r.json());
        });
    }

    #[test]
    fn fair_play_marks_the_label_without_touching_the_description() {
        // NSFR: the egregious envelope (fair play enforced: it blocks the gate)
        let mut c = Case::complete();
        c.findings = vec![finding("M129", "x")];
        c.policy = enforced();
        c.run(|r| {
            assert_eq!(r.label, "Costume (look, effects, sound) · NSFR");
            assert!(r.blockers.iter().any(|b| b.starts_with("fair play: NSFR (M129")), "{:?}", r.blockers);
            assert!(!r.broken(), "fair play is never broken");
            assert!(r.text(&|m| format!("<{m}>")).contains("fair play: fail <NSFR>"));
        });
        // FP?: a check that could not run
        let mut c = Case::complete();
        c.findings = vec![finding("M144", "the model's meshes could not be read, so its silhouette and hitbox envelope were not judged (M124, M129, M140)")];
        c.policy = enforced();
        c.run(|r| {
            assert_eq!(r.label, "Costume (look, effects, sound) · FP?");
            assert!(r.blockers.iter().any(|b| b.starts_with("fair play: unknown (M124, M129, M140 not run")), "{:?}", r.blockers);
            assert!(r.json().contains("\"fair_play_enforced\":true"), "{}", r.json());
        });
        // lenient (the default): NSFR and FP? are warnings at the gate; the label and the record say the same
        for (code, mark, warning) in [("M129", "NSFR", "fair play: NSFR (M129"), ("M144", "FP?", "fair play: unknown (M124")] {
            let mut c = Case::complete();
            c.findings = vec![finding(code, "the model's meshes could not be read (M124)")];
            c.run(|r| {
                assert_eq!(r.label, format!("Costume (look, effects, sound) · {mark}"), "the label still says it");
                assert!(r.gate_passes(), "{:?}", r.blockers);
                assert!(r.warnings[0].starts_with(warning) && r.warnings[0].ends_with("not blocking: fair play is not enforced (fair_play.enforce = false)"), "{:?}", r.warnings);
                assert!(r.json().contains("\"fair_play_enforced\":false"), "{}", r.json());
                assert!(r.json().contains(&format!("\"fair_play\":{{\"state\":\"{}\"", if mark == "NSFR" { "fail" } else { "unknown" })), "{}", r.json());
            });
        }
    }

    #[test]
    fn violations_say_what_was_measured_and_a_long_run_of_one_rule_is_counted() {
        let mut c = Case::complete();
        c.findings = (0..8).map(|i| finding("M124", &format!("front view: our in-game silhouette is 9{i}.0% of Valve's"))).collect();
        c.run(|r| {
            let t = r.text(&|m| m.to_string());
            assert!(t.contains("  violation M124 models/heroes_wip/lash/lash.vmdl_c: front view: our in-game silhouette is 90.0% of Valve's (never allowable)\n"), "{t}");
            assert_eq!(t.matches("  violation M124").count(), VIOLATIONS_SHOWN, "{t}");
            assert!(t.contains("  and 3 more M124 violation(s): `magician lint` lists every one, the --json record too\n"), "{t}");
            assert_eq!(r.json().matches("\"rule\":\"M124\"").count(), 8, "the record keeps every one: {}", r.json());
        });
    }

    #[test]
    fn a_long_message_is_cut_at_a_word_never_inside_one() {
        let m124 = "front view: our in-game silhouette is 75.7% of Valve's (28231 vs 37272 px at 6 mm, 10569 px lost): the target is harder to see than the original";
        assert_eq!(brief(m124), m124, "a fair-play message is shown whole");
        let long = "word ".repeat(60);
        let b = brief(&long);
        assert!(b.ends_with("word...") && b.chars().count() <= MESSAGE_SHOWN + 3, "{b}");
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
            assert_eq!(r.label, "Costume (look, effects, sound) · stale(6864)");
            assert!(r.blockers.contains(&"freshness: not proven on 6865 (last proven on 6864)".to_string()), "{:?}", r.blockers);
            assert!(r.json().contains("\"freshness\":{\"proven\":[6864],\"last_proven\":6864,\"target\":6865,\"current\":false,\"unbound\":[]}"), "{}", r.json());
        });
        let mut c = Case::complete();
        c.proven = vec![];
        c.run(|r| {
            assert_eq!(r.label, "Costume (look, effects, sound) · unproven");
            assert!(r.text(&|m| format!("<{m}>")).contains("target 6864 (<unproven>)"));
            assert!(r.blockers.iter().any(|b| b.contains("no proven build on record")));
        });
        // broken, then fair play, then freshness
        let mut c = Case::complete();
        c.findings = vec![finding("M144", "could not be read (M124)"), finding("M030", "needs 'x'")];
        c.target = "6865";
        c.run(|r| assert_eq!(r.label, "Costume (look, effects, sound) · broken · FP? · stale(6864)"));
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
            assert_eq!(r.label, "Costume (look, effects, sound) · FP?");
            assert!(r.text(&|m| m.to_string()).contains("not run: M124, M129, M130, M140"), "{}", r.text(&|m| m.to_string()));
        });
    }

    #[test]
    fn a_mod_that_touches_nothing_of_the_hero_is_not_graded() {
        let mut c = Case::complete();
        c.paths = vec!["panorama/layout/hud.vxml_c".into()];
        c.signals = Signals { other_kind: "HUD mod".into(), ..Default::default() };
        c.run(|r| {
            assert_eq!(r.label, "not graded (HUD mod)");
            assert!(r.description.is_none() && r.json().contains("\"description\":null"), "{}", r.json());
            assert!(r.text(&|m| m.to_string()).contains("there is no rubric for it yet"));
        });
    }
}
