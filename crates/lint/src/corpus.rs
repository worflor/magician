//! Calibration on Valve's own content: the rules run over files that are known to work, because Valve shipped them
//! and the game plays them. A rule that fires there is wrong, or it states a fact without the consequence it claims.
//!
//! Each calibration pairs one rule with the measure it judges by, taken over the installed build's own heroes, and
//! says how often the rule's test fires on Valve's content (`fires` of `of`). `magician corpus` runs them all, writes
//! `<reference>/<build>/corpus.tsv` and prints what each found; on patch day it says again whether each line holds.
//! A candidate rule is measured here before it is written: if it fires on Valve's own content, it is not a rule.
//! Everything here is a pure function of Valve's bytes: the same build gives the same file.
//!
//! - **cloth in the air** (a candidate, not a rule): a free cloth node lighter than the hero's fall whose goal spring
//!   cannot hold it below its anchor once the hero is airborne (`cloth::lift` over 1), across every simulated group
//!   of every hero. On 6868 it fires on 59 of Valve's 200 groups, so it cannot tell a floating ribbon from Valve's
//!   own cloth and M141's family has no rule for it.
//! - **voice length** (M063 on `sounds/vo/`): M063's own duration test, run between two of Valve's heroes on the same
//!   line (`orion_effort_dash_02` against `fencer_effort_dash_02`): how often one Valve voice reads as a hidden cue
//!   against another.
//! - **attachments** (M062): which attachment names every Valve hero model carries, and which only some do.
//! - **folds and crossings** (M109, M110): each hero's visible fold and crossing line in units of its size, against
//!   the line the rule draws at the most any Valve hero has: none of Valve's heroes may be over it.

use crate::cloth;
use std::collections::BTreeMap;

/// One rule's line measured on Valve's content.
#[derive(Debug, Clone, PartialEq)]
pub struct Calibration {
    pub rule: &'static str,
    pub measure: &'static str,
    /// How many of Valve's items the rule's test fires on, of how many it judged.
    pub fires: usize,
    pub of: usize,
    /// The worst Valve value of the measure and where it is.
    pub worst: f64,
    pub worst_at: String,
    /// What the result means for the rule, one sentence.
    pub reading: String,
}

#[derive(Debug, Clone, Default)]
pub struct Corpus {
    pub build: String,
    pub calibrations: Vec<Calibration>,
    /// Attachment name to the Valve hero models that carry it (file names), and the number of models read.
    pub attachments: BTreeMap<String, Vec<String>>,
    pub models: Vec<String>,
    /// Every Valve cloth group in the air: (model file, group).
    pub cloth: Vec<(String, cloth::Airborne)>,
}

/// Valve's hero models judged for cloth: (model path, its bytes).
pub fn cloth_calibration(models: &[(String, Vec<u8>)]) -> Calibration {
    cloth_census(models).1
}

/// Every Valve cloth group in the air, as (model file, group) rows, and the calibration.
pub fn cloth_census(models: &[(String, Vec<u8>)]) -> (Vec<(String, cloth::Airborne)>, Calibration) {
    let (mut fires, mut of, mut worst, mut worst_at) = (0, 0, f64::MIN, String::new());
    let mut rows = Vec::new();
    for (path, data) in models {
        let Some(c) = cloth::read(data) else { continue };
        for g in cloth::airborne(&c) {
            rows.push((file_name(path).to_string(), g.clone()));
            of += 1;
            if g.lift > 1.0 {
                fires += 1;
            }
            if g.lift > worst {
                worst = g.lift;
                worst_at = format!("{} {}", file_name(path), g.label);
            }
        }
    }
    let reading = if fires == 0 {
        format!("no Valve cloth group is lighter than the hero's fall ({} u/s2) and too loose to hang: a group that is would not be Valve's design, and the line could become a rule", cloth::HERO_FALL)
    } else {
        format!("{fires} of Valve's own groups are lighter than the hero's fall and too loose to hang in the air: the measure cannot tell a floating piece from Valve's design, so it is not a rule")
    };
    (rows, Calibration { rule: "cloth", measure: "airborne lift (median node)", fires, of, worst: if of == 0 { 0.0 } else { worst }, worst_at, reading })
}

/// M109 or M110 over Valve's heroes: (model path, the hero's measure in units of its size), judged at `line`.
pub fn line_calibration(rule: &'static str, measure: &'static str, rows: &[(String, f64)], line: f64) -> Calibration {
    let fires = rows.iter().filter(|(_, x)| *x > line).count();
    let (worst_at, worst) = rows.iter().max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal)).map_or((String::new(), 0.0), |(p, x)| (file_name(p).to_string(), *x));
    let reading = if fires == 0 {
        format!("no Valve hero is over the line ({line}); the most is {worst:.2}: a model past it crosses more than Valve builds any hero")
    } else {
        format!("{fires} of Valve's heroes are over the line ({line}), the most {worst:.2}: the line is below Valve's own construction and must move to the most (rules.rs)")
    };
    Calibration { rule, measure, fires, of: rows.len(), worst, worst_at, reading }
}

/// The line a voice file speaks, without the hero: `sounds/vo/orion/emote/orion_effort_dash_02.vsnd_c` is
/// (`orion`, `emote/effort_dash_02`).
pub fn voice_line(path: &str) -> Option<(String, String)> {
    let rest = path.strip_prefix("sounds/vo/")?;
    let (hero, tail) = rest.split_once('/')?;
    let (sub, file) = tail.rsplit_once('/').map_or(("", tail), |(s, f)| (s, f));
    let line = file.strip_suffix(".vsnd_c")?.strip_prefix(hero)?.strip_prefix('_')?;
    Some((hero.to_string(), if sub.is_empty() { line.to_string() } else { format!("{sub}/{line}") }))
}

/// M063's duration test between Valve's own voices: for every line two or more heroes record, each hero's take
/// against every other's. `durations` is (path, seconds) for Valve's voice files; `ratio` the fair-play limit.
pub fn voice_calibration(durations: &[(String, f64)], ratio: f64) -> Calibration {
    let mut by_line: BTreeMap<String, Vec<(String, f64)>> = BTreeMap::new();
    for (p, d) in durations {
        if let Some((hero, line)) = voice_line(p) {
            by_line.entry(line).or_default().push((hero, *d));
        }
    }
    let (mut fires, mut of, mut worst, mut worst_at) = (0, 0, f64::MAX, String::new());
    for (line, takes) in &by_line {
        for (h1, a) in takes {
            for (h2, b) in takes {
                if h1 == h2 || *b <= 0.0 {
                    continue;
                }
                of += 1;
                if *a < b * ratio {
                    fires += 1;
                }
                if a / b < worst {
                    worst = a / b;
                    worst_at = format!("{line}: {h1} {a:.2} s, {h2} {b:.2} s");
                }
            }
        }
    }
    let rate = if of == 0 { 0.0 } else { fires as f64 / of as f64 };
    let reading = if rate > 0.05 {
        format!("{:.0}% of Valve's own voices are 'shorter than Valve's' against another Valve voice on the same line: a line's length is the voice, not a cue", 100.0 * rate)
    } else {
        "Valve's voices agree on line length: a much shorter line stands out".into()
    };
    Calibration { rule: "M063", measure: "voice line length, one Valve hero against another", fires, of, worst: if of == 0 { 0.0 } else { worst }, worst_at, reading }
}

/// The attachments every model carries and those only some do, from (model path, attachment names).
pub fn attachment_census(models: &[(String, Vec<String>)]) -> (BTreeMap<String, Vec<String>>, Calibration) {
    let mut by: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (p, names) in models {
        let mut n = names.clone();
        n.sort();
        n.dedup();
        for a in n {
            by.entry(a).or_default().push(file_name(p).to_string());
        }
    }
    let all = models.len();
    let partial: Vec<(String, usize)> = by.iter().map(|(k, v)| (k.clone(), v.len())).filter(|(_, n)| *n * 2 >= all && *n < all).collect();
    let (worst_at, worst) = partial.iter().min_by_key(|(_, n)| *n).map_or((String::new(), all as f64), |(k, n)| (k.clone(), *n as f64));
    let reading = format!(
        "{} attachment name(s) most Valve heroes carry and some don't ({}): the game copes without them, so a mod missing one loses only what that hero's own data asks of it",
        partial.len(),
        partial.iter().take(6).map(|(k, n)| format!("{k} {n}/{all}")).collect::<Vec<_>>().join(", ")
    );
    (by, Calibration { rule: "M062", measure: "attachment names across Valve's hero models", fires: partial.len(), of: all, worst, worst_at, reading })
}

fn file_name(p: &str) -> &str {
    p.rsplit('/').next().unwrap_or(p)
}

/// `corpus.tsv`: one `calibration` row per rule, then `attachment` rows (name, models carrying it).
pub fn to_tsv(c: &Corpus) -> String {
    let mut t = format!("# magician corpus build={} models={}\n", c.build, c.models.len());
    for k in &c.calibrations {
        t.push_str(&format!("calibration\t{}\t{}\t{}\t{}\t{:.4}\t{}\t{}\n", k.rule, k.measure, k.fires, k.of, k.worst, k.worst_at, k.reading));
    }
    for (a, m) in &c.attachments {
        t.push_str(&format!("attachment\t{a}\t{}\t{}\n", m.len(), m.join(",")));
    }
    for (m, g) in &c.cloth {
        t.push_str(&format!("cloth\t{m}\t{}\t{}\t{:.4}\t{:.4}\t{:.1}\t{:.5}\n", g.label, g.names.len(), g.lift, g.worst, g.gravity, g.goal));
    }
    t
}

/// The calibrations of a `corpus.tsv` as (rule, fires, of, reading), for `magician rules`.
pub fn read_calibrations(text: &str) -> Vec<(String, usize, usize, String)> {
    text.lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            (f.len() >= 8 && f[0] == "calibration").then(|| (f[1].to_string(), f[3].parse().unwrap_or(0), f[4].parse().unwrap_or(0), f[7].to_string()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_voice_line_is_named_without_its_hero() {
        assert_eq!(voice_line("sounds/vo/orion/emote/orion_effort_dash_02.vsnd_c"), Some(("orion".into(), "emote/effort_dash_02".into())));
        assert_eq!(voice_line("sounds/vo/lash/lash_kill_01.vsnd_c"), Some(("lash".into(), "kill_01".into())));
        assert_eq!(voice_line("sounds/vo/lash/emote/other_hero_01.vsnd_c"), None, "a file that is not the hero's own line");
    }

    #[test]
    fn voices_that_differ_in_length_fire_the_duration_test_on_each_other() {
        let d = |p: &str, s: f64| (p.to_string(), s);
        let v = vec![
            d("sounds/vo/aa/emote/aa_pain_01.vsnd_c", 0.4),
            d("sounds/vo/bb/emote/bb_pain_01.vsnd_c", 0.8),
            d("sounds/vo/cc/emote/cc_pain_01.vsnd_c", 0.82),
            d("sounds/vo/cc/emote/cc_only_01.vsnd_c", 9.0),
        ];
        let c = voice_calibration(&v, 0.8);
        assert_eq!((c.fires, c.of), (2, 6), "aa against bb and cc; nothing else is under 0.8x");
        assert!(c.reading.contains("the voice, not a cue"), "{}", c.reading);
        assert!((c.worst - 0.4 / 0.82).abs() < 1e-9 && c.worst_at.starts_with("emote/pain_01: aa"), "{c:?}");
    }

    #[test]
    fn a_line_below_a_valve_hero_says_it_must_move() {
        let rows = vec![("models/a/a.vmdl_c".to_string(), 3.0), ("models/b/b.vmdl_c".to_string(), 4.8)];
        let c = line_calibration("M109", "fold line", &rows, 4.81);
        assert_eq!((c.fires, c.of, c.worst_at.as_str()), (0, 2, "b.vmdl_c"));
        let c = line_calibration("M109", "fold line", &rows, 4.0);
        assert!(c.fires == 1 && c.reading.contains("must move"), "{c:?}");
    }

    #[test]
    fn attachments_some_heroes_lack_are_named() {
        let m = |p: &str, a: &[&str]| (p.to_string(), a.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        let (by, c) = attachment_census(&[m("models/a/a.vmdl_c", &["aim", "healthbar"]), m("models/b/b.vmdl_c", &["aim", "healthbar"]), m("models/c/c.vmdl_c", &["aim"])]);
        assert_eq!(by["aim"].len(), 3);
        assert_eq!((c.fires, c.of, c.worst_at.as_str()), (1, 3, "healthbar"));
        let t = to_tsv(&Corpus { build: "1".into(), calibrations: vec![c], attachments: by, models: vec!["a".into(), "b".into(), "c".into()], cloth: Vec::new() });
        assert_eq!(read_calibrations(&t), vec![("M062".into(), 1, 3, read_calibrations(&t)[0].3.clone())]);
        assert!(t.contains("attachment\thealthbar\t2\ta.vmdl_c,b.vmdl_c\n"), "{t}");
    }
}
