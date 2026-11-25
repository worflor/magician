//! A policy: every number that encodes a judgement, in one value, so a person can move a line without rebuilding
//! Magician. With no policy file every value is the built-in default from its one home (`rubric.rs` for grading, the
//! constants block in `lint/src/mesh/rules.rs` for the fair-play limits), so behaviour is unchanged.
//!
//! The file is plain text in the shape `mod.toml` and `config.txt` already use: `[section]` headers, one
//! `key = value` per line, `#` comments. A value is a number, a quoted string, or a one-line list of quoted strings.
//! A key left out keeps its default. Anything the reader does not know (a section, a key, the wrong kind of value, a
//! number outside its range, a key set twice) is refused with the line and the key, never ignored.
//!
//! ```text
//! # a stricter house line for a ranked-only server
//! [fair_play]
//! hitbox_egregious_units = 15
//! enforce = true
//! [compliance]
//! m140_max_worst_units = 12
//! ```
//!
//! `magician policy` prints every effective value and where it came from. A grade record carries the policy's
//! fingerprint (SHA-256 of every effective value), so two runs with the same fingerprint judged by the same lines.

use crate::rubric::{self, Archetype};
use lint::policy::FairPlay;
use std::collections::BTreeMap;

/// How the grade reads fair-play findings.
#[derive(Debug, Clone, PartialEq)]
pub struct Compliance {
    /// Rules whose findings fail fair play, allowed or not.
    pub never: Vec<String>,
    /// Rules whose findings are reported and never fail fair play.
    pub advisory: Vec<String>,
    /// The farthest an M140 allowance may accept: the worst visible point (units) and the visible share beyond the
    /// taste line. `None` follows the M129 line (`fair_play.hitbox_egregious_*`), so every M140 is allowable.
    pub m140_max_worst_units: Option<f64>,
    pub m140_max_visible_share: Option<f64>,
    /// The widest M130 share an allowance may accept (used only if M130 is moved out of `advisory`).
    pub m130_max_share: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Coverage {
    pub group_share: f64,
    pub materials_full: f64,
    pub voice_full: f64,
    pub new_sounds_full: f64,
    pub anim_ui_full: f64,
    pub anim_abilities_full: f64,
    pub anim_other_full: f64,
    pub ui_hero_art_share: f64,
}

/// The two lines that describe a group (look, effects, sound) by its weighted coverage.
#[derive(Debug, Clone, PartialEq)]
pub struct Groups {
    /// Named in the label at or above this.
    pub covered: f64,
    /// `some <group>` at or above this (and above nothing); left out under it.
    pub touched: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Identity {
    pub new_character_signals: f64,
    pub voice_identity_weight: f64,
    pub head_kept_share: f64,
    pub portrait_share: f64,
    pub valve_mesh_share: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Policy {
    pub fair_play: FairPlay,
    pub compliance: Compliance,
    /// Whether fair play is enforced (`fair_play.enforce`): its findings errors, NSFR and FP? blockers at the gate.
    /// Off by default: the same findings are warnings and the gate lists NSFR and FP? as warnings.
    pub enforce: bool,
    /// Whether rights must be cleared to pass the release gate (`rights.enforce`). Off by default: `unreviewed` is a
    /// warning; `denied` blocks either way.
    pub rights_enforce: bool,
    pub groups: Groups,
    pub weights_character: Vec<(String, f64)>,
    pub weights_variant: Vec<(String, f64)>,
    pub weights_kit: Vec<(String, f64)>,
    pub identity: Identity,
    pub coverage: Coverage,
    /// Where each key that a file set came from (`<file> line N`); keys absent here are defaults.
    pub set_by: BTreeMap<String, String>,
    /// The file the policy was read from, if any.
    pub file: Option<String>,
}

fn owned(w: &[(&str, f64)]) -> Vec<(String, f64)> {
    w.iter().map(|(k, v)| (k.to_string(), *v)).collect()
}

fn strings(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

impl Default for Policy {
    fn default() -> Self {
        Policy {
            fair_play: lint::policy::DEFAULT,
            compliance: Compliance { never: strings(rubric::NEVER_ALLOWABLE), advisory: strings(rubric::ADVISORY), m140_max_worst_units: None, m140_max_visible_share: None, m130_max_share: 1.0 },
            enforce: rubric::FAIR_PLAY_ENFORCED,
            rights_enforce: rubric::RIGHTS_ENFORCED,
            groups: Groups { covered: rubric::COVERED, touched: rubric::TOUCHED },
            weights_character: owned(rubric::WEIGHTS_CHARACTER),
            weights_variant: owned(rubric::WEIGHTS_VARIANT),
            weights_kit: owned(rubric::WEIGHTS_KIT),
            identity: Identity {
                new_character_signals: rubric::NEW_CHARACTER_SIGNALS,
                voice_identity_weight: rubric::VOICE_IDENTITY_WEIGHT,
                head_kept_share: rubric::HEAD_KEPT_SHARE,
                portrait_share: rubric::PORTRAIT_SHARE,
                valve_mesh_share: rubric::VALVE_MESH_SHARE,
            },
            coverage: Coverage {
                group_share: rubric::GROUP_SHARE,
                materials_full: rubric::MATERIALS_FULL,
                voice_full: rubric::VOICE_FULL,
                new_sounds_full: rubric::NEW_SOUNDS_FULL,
                anim_ui_full: rubric::ANIM_UI_FULL,
                anim_abilities_full: rubric::ANIM_ABILITIES_FULL,
                anim_other_full: rubric::ANIM_OTHER_FULL,
                ui_hero_art_share: rubric::UI_HERO_ART_SHARE,
            },
            set_by: BTreeMap::new(),
            file: None,
        }
    }
}

/// A value as a policy file spells it.
#[derive(Debug, Clone, PartialEq)]
enum Value {
    Num(f64),
    Str(String),
    List(Vec<String>),
    Bool(bool),
}

/// The kind of value a key takes, and its range for numbers.
#[derive(Clone, Copy)]
enum Kind {
    Num(f64, f64),
    Rules,
    Bool,
}

/// One key of the file: its full name (`section.key`), kind, and what it means.
pub struct Key {
    pub name: String,
    kind: Kind,
    pub what: String,
}

fn num(name: &str, min: f64, max: f64, what: &str) -> Key {
    Key { name: name.to_string(), kind: Kind::Num(min, max), what: what.to_string() }
}

/// Every key a policy file may set, in the order `magician policy` prints them.
pub fn keys() -> Vec<Key> {
    let mut k = Vec::new();
    k.push(Key {
        name: "fair_play.enforce".into(),
        kind: Kind::Bool,
        what: "true: fair-play findings are errors (lint exits 1, install refuses) and NSFR or FP? blocks the release gate; false: the same are warnings".into(),
    });
    for f in lint::policy::KEYS {
        k.push(num(&format!("fair_play.{}", f.name), f.min, f.max, &format!("{} ({})", f.what, f.rules)));
    }
    k.push(Key { name: "compliance.never_allowable".into(), kind: Kind::Rules, what: "fair-play rules whose findings fail fair play, allowed or not".into() });
    k.push(Key { name: "compliance.advisory".into(), kind: Kind::Rules, what: "fair-play rules whose findings are reported and never fail it".into() });
    k.push(num("compliance.m140_max_worst_units", 0.0, 200.0, "the farthest worst visible point an M140 allowance accepts (default: the M129 line)"));
    k.push(num("compliance.m140_max_visible_share", 0.0, 1.0, "the largest visible share beyond the taste line an M140 allowance accepts (default: the M129 line)"));
    k.push(num("compliance.m130_max_share", 0.0, 1.0, "the widest M130 share an allowance accepts, if M130 is not advisory"));
    k.push(Key { name: "rights.enforce".into(), kind: Kind::Bool, what: "true: rights must be cleared to pass the release gate; false: unreviewed is a warning (denied blocks either way)".into() });
    k.push(num("groups.covered", 0.01, 1.0, "a group (look, effects, sound) is named in the label once its weighted coverage reaches this"));
    k.push(num("groups.touched", 0.0, 1.0, "a group reads \"some <group>\" once its weighted coverage reaches this (and is above nothing); under it, it is left out"));
    for a in Archetype::all() {
        let (req, app) = a.surfaces();
        for id in req.iter().chain(app) {
            let c = rubric::criterion(id);
            k.push(num(&format!("weights.{}.{id}", a.id()), 0.0, 100.0, &format!("how much {} counts within {} in the {} rubric", c.name, c.group.id(), a.id())));
        }
    }
    k.push(num("identity.new_character_signals", 0.5, 4.0, "identity signals a new character needs"));
    k.push(num("identity.voice_identity_weight", 0.0, 1.0, "how much of a signal a declared new voice identity counts for"));
    k.push(num("identity.head_kept_share", 0.0, 1.0, "the head is Valve's when at least this share of our head vertices sit on Valve's"));
    k.push(num("identity.portrait_share", 0.0, 1.0, "share of the hero's portrait and card art replaced for new_hero_art"));
    k.push(num("identity.valve_mesh_share", 0.0, 1.0, "a replaced model keeps Valve's mesh (a variant) when this share of its vertices sit on Valve's"));
    k.push(num("coverage.group_share", 0.01, 1.0, "a group (one ability, one moment) is fully covered once this share of its files is replaced"));
    k.push(num("coverage.materials_full", 1.0, 100.0, "hero materials for full materials credit"));
    k.push(num("coverage.voice_full", 1.0, 10000.0, "treated voice files for full voice credit"));
    k.push(num("coverage.new_sounds_full", 1.0, 10000.0, "new sound files for the custom kit-sound credit"));
    k.push(num("coverage.anim_ui_full", 1.0, 100.0, "hero-select and showcase clips for full credit in that band"));
    k.push(num("coverage.anim_abilities_full", 1.0, 100.0, "ability clips for full credit in that band"));
    k.push(num("coverage.anim_other_full", 1.0, 1000.0, "other clips for full credit in that band"));
    k.push(num("coverage.ui_hero_art_share", 0.01, 1.0, "share of the hero art replaced for full hero-art credit"));
    k
}

impl Policy {
    pub fn weights(&self, a: Archetype) -> &[(String, f64)] {
        match a {
            Archetype::Character => &self.weights_character,
            Archetype::Variant => &self.weights_variant,
            Archetype::Kit => &self.weights_kit,
        }
    }

    /// The effective M140 allowance limits: the policy's own, else the M129 line.
    pub fn m140_limits(&self) -> (f64, f64) {
        (
            self.compliance.m140_max_worst_units.unwrap_or(self.fair_play.hitbox_egregious_units),
            self.compliance.m140_max_visible_share.unwrap_or(self.fair_play.hitbox_egregious_share),
        )
    }

    fn get(&self, key: &str) -> String {
        let (section, rest) = key.split_once('.').unwrap_or((key, ""));
        let n = |x: f64| fmt(x);
        match section {
            "fair_play" if rest == "enforce" => self.enforce.to_string(),
            "rights" => self.rights_enforce.to_string(),
            "fair_play" => n(self.fair_play.get(rest).unwrap_or(f64::NAN)),
            "compliance" => match rest {
                "never_allowable" => list(&sorted(&self.compliance.never)),
                "advisory" => list(&sorted(&self.compliance.advisory)),
                "m140_max_worst_units" => n(self.m140_limits().0),
                "m140_max_visible_share" => n(self.m140_limits().1),
                _ => n(self.compliance.m130_max_share),
            },
            "groups" => n(if rest == "covered" { self.groups.covered } else { self.groups.touched }),
            "weights" => {
                let (a, id) = rest.split_once('.').unwrap_or(("", ""));
                let a = Archetype::all().into_iter().find(|x| x.id() == a).unwrap_or(Archetype::Character);
                n(self.weights(a).iter().find(|(k, _)| k == id).map_or(0.0, |(_, w)| *w))
            }
            "identity" => n(match rest {
                "new_character_signals" => self.identity.new_character_signals,
                "voice_identity_weight" => self.identity.voice_identity_weight,
                "head_kept_share" => self.identity.head_kept_share,
                "portrait_share" => self.identity.portrait_share,
                _ => self.identity.valve_mesh_share,
            }),
            _ => n(match rest {
                "group_share" => self.coverage.group_share,
                "materials_full" => self.coverage.materials_full,
                "voice_full" => self.coverage.voice_full,
                "new_sounds_full" => self.coverage.new_sounds_full,
                "anim_ui_full" => self.coverage.anim_ui_full,
                "anim_abilities_full" => self.coverage.anim_abilities_full,
                "anim_other_full" => self.coverage.anim_other_full,
                _ => self.coverage.ui_hero_art_share,
            }),
        }
    }

    fn set(&mut self, key: &str, v: Value) -> Result<(), String> {
        let (section, rest) = key.split_once('.').unwrap_or((key, ""));
        let Value::Num(x) = v.clone() else {
            return match (key, v) {
                ("fair_play.enforce", Value::Bool(b)) => {
                    self.enforce = b;
                    Ok(())
                }
                ("rights.enforce", Value::Bool(b)) => {
                    self.rights_enforce = b;
                    Ok(())
                }
                ("compliance.never_allowable", Value::List(l)) => {
                    self.compliance.never = l;
                    Ok(())
                }
                ("compliance.advisory", Value::List(l)) => {
                    self.compliance.advisory = l;
                    Ok(())
                }
                _ => Err(format!("{key}: wrong kind of value")),
            };
        };
        match section {
            "fair_play" => self.fair_play.set(rest, x)?,
            "compliance" => match rest {
                "m140_max_worst_units" => self.compliance.m140_max_worst_units = Some(x),
                "m140_max_visible_share" => self.compliance.m140_max_visible_share = Some(x),
                _ => self.compliance.m130_max_share = x,
            },
            "groups" if rest == "covered" => self.groups.covered = x,
            "groups" => self.groups.touched = x,
            "weights" => {
                let (a, id) = rest.split_once('.').unwrap_or(("", ""));
                let list = match a {
                    "character" => &mut self.weights_character,
                    "variant" => &mut self.weights_variant,
                    _ => &mut self.weights_kit,
                };
                match list.iter_mut().find(|(k, _)| k == id) {
                    Some(slot) => slot.1 = x,
                    None => list.push((id.to_string(), x)),
                }
            }
            "identity" => {
                let slot = match rest {
                    "new_character_signals" => &mut self.identity.new_character_signals,
                    "voice_identity_weight" => &mut self.identity.voice_identity_weight,
                    "head_kept_share" => &mut self.identity.head_kept_share,
                    "portrait_share" => &mut self.identity.portrait_share,
                    _ => &mut self.identity.valve_mesh_share,
                };
                *slot = x;
            }
            _ => {
                let c = &mut self.coverage;
                let slot = match rest {
                    "group_share" => &mut c.group_share,
                    "materials_full" => &mut c.materials_full,
                    "voice_full" => &mut c.voice_full,
                    "new_sounds_full" => &mut c.new_sounds_full,
                    "anim_ui_full" => &mut c.anim_ui_full,
                    "anim_abilities_full" => &mut c.anim_abilities_full,
                    "anim_other_full" => &mut c.anim_other_full,
                    _ => &mut c.ui_hero_art_share,
                };
                *slot = x;
            }
        }
        Ok(())
    }

    /// Checks across keys: the envelope tiers in order, the touched line under the covered one, and the rule lists
    /// naming fair-play rules, none in both.
    fn check(&self) -> Result<(), String> {
        self.fair_play.check()?;
        if self.groups.touched >= self.groups.covered {
            return Err(format!("groups.touched ({}) must be under groups.covered ({})", fmt(self.groups.touched), fmt(self.groups.covered)));
        }
        for (name, l) in [("compliance.never_allowable", &self.compliance.never), ("compliance.advisory", &self.compliance.advisory)] {
            if let Some(bad) = l.iter().find(|c| !rubric::FAIR_PLAY_RULES.contains(&c.as_str())) {
                return Err(format!("{name}: {bad} is not a fair-play rule (they are {})", rubric::FAIR_PLAY_RULES.join(", ")));
            }
        }
        if let Some(both) = self.compliance.never.iter().find(|c| self.compliance.advisory.contains(c)) {
            return Err(format!("{both} is in both compliance.never_allowable and compliance.advisory"));
        }
        Ok(())
    }

    /// Every key with its effective value and where it came from (`default`, or `<file> line N`).
    pub fn entries(&self) -> Vec<(String, String, String)> {
        keys()
            .into_iter()
            .map(|k| {
                let src = match self.set_by.get(&k.name) {
                    Some(s) => s.clone(),
                    None if k.name.starts_with("compliance.m140_max") => format!("default (the M129 line, fair_play.{})", if k.name.ends_with("units") { "hitbox_egregious_units" } else { "hitbox_egregious_share" }),
                    None => "default".to_string(),
                };
                let v = self.get(&k.name);
                (k.name, v, src)
            })
            .collect()
    }

    /// SHA-256 of every effective value, one `key = value` line each in a fixed order. A policy that spells the
    /// defaults out has the default fingerprint.
    pub fn fingerprint(&self) -> String {
        let text: String = keys().iter().map(|k| format!("{} = {}\n", k.name, self.get(&k.name))).collect();
        crate::sha256::hex(text.as_bytes())
    }

    /// The keys a file set to something other than the default, in key order.
    pub fn overrides(&self) -> Vec<String> {
        let d = Policy::default();
        keys().into_iter().filter(|k| self.get(&k.name) != d.get(&k.name)).map(|k| k.name).collect()
    }
}

fn fmt(x: f64) -> String {
    if x.is_finite() && x == x.trunc() && x.abs() < 1e15 {
        format!("{}", x as i64)
    } else {
        format!("{x}")
    }
}

fn sorted(v: &[String]) -> Vec<String> {
    let mut v = v.to_vec();
    v.sort();
    v.dedup();
    v
}

fn list(v: &[String]) -> String {
    format!("[{}]", v.iter().map(|s| format!("\"{s}\"")).collect::<Vec<_>>().join(", "))
}

/// The part of a line before a `#` that is not inside quotes.
fn strip_comment(line: &str) -> &str {
    let mut quoted = false;
    for (i, c) in line.char_indices() {
        match c {
            '"' => quoted = !quoted,
            '#' if !quoted => return &line[..i],
            _ => {}
        }
    }
    line
}

fn parse_value(raw: &str) -> Option<Value> {
    let t = raw.trim();
    if let Some(body) = t.strip_prefix('[').and_then(|b| b.strip_suffix(']')) {
        let items: Vec<&str> = body.split(',').map(str::trim).filter(|s| !s.is_empty()).collect();
        let mut out = Vec::new();
        for i in items {
            out.push(i.strip_prefix('"')?.strip_suffix('"')?.to_string());
        }
        return Some(Value::List(out));
    }
    if let Some(s) = t.strip_prefix('"').and_then(|b| b.strip_suffix('"')) {
        return Some(Value::Str(s.to_string()));
    }
    match t {
        "true" => return Some(Value::Bool(true)),
        "false" => return Some(Value::Bool(false)),
        _ => {}
    }
    t.parse::<f64>().ok().filter(|x| x.is_finite()).map(Value::Num)
}

/// Read a policy file's text. `name` is how errors and sources name the file. Every error names the line and key.
pub fn parse(text: &str, name: &str) -> Result<Policy, String> {
    let all = keys();
    let mut p = Policy { file: Some(name.to_string()), ..Policy::default() };
    let mut section: Option<String> = None;
    let sections: Vec<String> = {
        let mut s: Vec<String> = all.iter().map(|k| k.name.rsplit_once('.').map(|(a, _)| a.to_string()).unwrap_or_default()).collect();
        s.dedup();
        s
    };
    for (n, raw) in text.lines().enumerate() {
        let at = format!("{name} line {}", n + 1);
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        if let Some(h) = line.strip_prefix('[') {
            let h = h.strip_suffix(']').ok_or_else(|| format!("{at}: a section header is [name]"))?.trim();
            if h == "letters" {
                return Err(format!("{at}: [letters] is gone: a grade describes what a mod makes its own and has no letter; [groups] covered and touched are the lines that name a group"));
            }
            if h == "gate" {
                return Err(format!("{at}: [gate] nsfr is gone: fair play has one switch, [fair_play] enforce (nsfr = \"block\" is enforce = true, nsfr = \"warn\" is enforce = false)"));
            }
            if !sections.iter().any(|s| s == h) {
                return Err(format!("{at}: unknown section [{h}] (known: {})", sections.iter().map(|s| format!("[{s}]")).collect::<Vec<_>>().join(", ")));
            }
            section = Some(h.to_string());
            continue;
        }
        let (k, v) = line.split_once('=').ok_or_else(|| format!("{at}: expected key = value"))?;
        let k = k.trim();
        let sec = section.as_deref().ok_or_else(|| format!("{at}: {k} is outside any [section]"))?;
        let full = format!("{sec}.{k}");
        let key = all.iter().find(|x| x.name == full).ok_or_else(|| format!("{at}: unknown key {full}"))?;
        if let Some(before) = p.set_by.get(&full) {
            return Err(format!("{at}: {full} is set twice (first at {before})"));
        }
        let value = parse_value(v).ok_or_else(|| format!("{at}: {full}: cannot read the value {:?}", v.trim()))?;
        let value = match (key.kind, value) {
            (Kind::Num(min, max), Value::Num(x)) => {
                if x < min || x > max {
                    return Err(format!("{at}: {full} = {} is out of range ({} to {})", fmt(x), fmt(min), fmt(max)));
                }
                Value::Num(x)
            }
            (Kind::Num(..), _) => return Err(format!("{at}: {full} takes a number")),
            (Kind::Bool, Value::Bool(b)) => Value::Bool(b),
            (Kind::Bool, _) => return Err(format!("{at}: {full} takes true or false")),
            (Kind::Rules, Value::List(l)) => Value::List(l),
            (Kind::Rules, _) => return Err(format!("{at}: {full} takes a list of rule codes, e.g. [\"M129\", \"M124\"]")),
        };
        p.set(&full, value).map_err(|e| format!("{at}: {e}"))?;
        p.set_by.insert(full, at);
    }
    p.check().map_err(|e| format!("{name}: {e}"))?;
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_file_is_the_built_in_defaults() {
        let d = Policy::default();
        assert_eq!(d.fair_play, lint::policy::DEFAULT);
        assert!(d.overrides().is_empty() && !d.enforce && !d.rights_enforce, "fair play and rights are lenient unless enforced");
        assert!(d.entries().iter().all(|(_, _, s)| s.starts_with("default")));
        d.check().unwrap();
        // the M140 allowance follows the M129 line until a file says otherwise
        assert_eq!(d.m140_limits(), (18.0, 0.035));
    }

    #[test]
    fn a_file_sets_keys_and_says_where() {
        let p = parse("# house line\n[fair_play]\nhitbox_egregious_units = 15  # stricter\nenforce = true\n[compliance]\nm140_max_worst_units = 12\nadvisory = [\"M081\"]\n", "house.toml").unwrap();
        assert_eq!(p.fair_play.hitbox_egregious_units, 15.0);
        assert_eq!(p.m140_limits(), (12.0, 0.035));
        assert!(p.enforce && !p.rights_enforce);
        assert_eq!(p.compliance.advisory, vec!["M081".to_string()]);
        let e: BTreeMap<String, (String, String)> = p.entries().into_iter().map(|(k, v, s)| (k, (v, s))).collect();
        assert_eq!(e["fair_play.hitbox_egregious_units"], ("15".to_string(), "house.toml line 3".to_string()));
        assert_eq!(e["fair_play.enforce"], ("true".to_string(), "house.toml line 4".to_string()));
        assert_eq!(e["rights.enforce"], ("false".to_string(), "default".to_string()));
        assert_eq!(e["groups.covered"], ("0.8".to_string(), "default".to_string()));
        assert_eq!(p.overrides(), vec!["fair_play.enforce", "fair_play.hitbox_egregious_units", "compliance.advisory", "compliance.m140_max_worst_units"]);
    }

    #[test]
    fn the_fingerprint_follows_the_values_not_the_spelling() {
        let d = Policy::default().fingerprint();
        assert_eq!(d.len(), 64);
        let spelled = parse("[groups]\ncovered = 0.8\n[fair_play]\nsilhouette_min = 0.97\n", "x").unwrap();
        assert_eq!(spelled.fingerprint(), d, "a file that spells the defaults out is the default policy");
        assert!(spelled.overrides().is_empty());
        let moved = parse("[groups]\ncovered = 0.9\n", "x").unwrap();
        assert_ne!(moved.fingerprint(), d);
        let other = parse("[compliance]\nadvisory = [\"M130\", \"M081\"]\n", "x").unwrap();
        assert_eq!(other.fingerprint(), d, "a rule list is a set: its order is not a value");
    }

    #[test]
    fn anything_unknown_or_out_of_range_is_refused_with_its_key() {
        let bad = |t: &str| parse(t, "p.toml").unwrap_err();
        let e = bad("[fair_play]\nhitbox_units = 3\n");
        assert!(e.contains("p.toml line 2") && e.contains("unknown key fair_play.hitbox_units"), "{e}");
        assert!(bad("[fairplay]\nx = 1\n").contains("unknown section [fairplay]"));
        assert!(bad("silhouette_min = 0.9\n").contains("outside any [section]"));
        let e = bad("[fair_play]\nsilhouette_min = 1.5\n");
        assert!(e.contains("fair_play.silhouette_min = 1.5 is out of range (0.5 to 1)"), "{e}");
        assert!(bad("[fair_play]\nsilhouette_min = \"high\"\n").contains("fair_play.silhouette_min takes a number"));
        assert!(bad("[fair_play]\nenforce = \"yes\"\n").contains("fair_play.enforce takes true or false"));
        assert!(bad("[rights]\nenforce = 1\n").contains("rights.enforce takes true or false"));
        assert!(bad("[fair_play]\nenforce = True\n").contains("cannot read the value"));
        assert!(bad("[fair_play]\nsilhouette_min = true\n").contains("fair_play.silhouette_min takes a number"));
        // the old switch is refused with the new one's name, never read as the opposite
        let e = bad("[gate]\nnsfr = \"warn\"\n");
        assert!(e.contains("p.toml line 1") && e.contains("[fair_play] enforce") && e.contains("\"warn\" is enforce = false"), "{e}");
        assert!(bad("[groups]\ncovered = 0.9\ncovered = 0.7\n").contains("groups.covered is set twice (first at p.toml line 2)"));
        assert!(bad("[groups]\ntouched = 0.8\n").contains("groups.touched (0.8) must be under groups.covered (0.8)"));
        let e = bad("[letters]\nS = 90\n");
        assert!(e.contains("p.toml line 1") && e.contains("[letters] is gone") && e.contains("[groups]"), "{e}");
        assert!(bad("[weights.kit]\nmodel = 10\n").contains("unknown key weights.kit.model"), "a kit has no model criterion");
        assert!(bad("[weights.character]\ncraft = 15\n").contains("unknown key weights.character.craft"), "craft is lint's now");
        assert!(bad("[compliance]\nadvisory = [\"M061\"]\n").contains("M061 is not a fair-play rule"));
        assert!(bad("[compliance]\nadvisory = [\"M129\"]\n").contains("M129 is in both"));
        assert!(bad("[fair_play]\nhitbox_taste_units = 20\n").contains("hitbox_egregious_units"));
        assert!(bad("[compliance]\nadvisory = M081\n").contains("cannot read the value"));
    }

    #[test]
    fn every_key_parses_its_own_default_back() {
        let d = Policy::default();
        let mut text = String::new();
        let mut last = String::new();
        for (k, v, _) in d.entries() {
            let (sec, key) = k.rsplit_once('.').unwrap();
            if sec != last {
                text.push_str(&format!("[{sec}]\n"));
                last = sec.to_string();
            }
            text.push_str(&format!("{key} = {v}\n"));
        }
        let back = parse(&text, "all.toml").unwrap();
        assert_eq!(back.fingerprint(), d.fingerprint(), "{text}");
        assert!(back.overrides().is_empty());
    }
}
