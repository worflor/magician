//! A lint run written down, so a second step (the grade inside `magician release`) judges exactly the run
//! the first step performed, at the same depth, instead of re-linting at its own.
//!
//! ```text
//! # magician findings depth=deep build=6864 vpk=pak10_dir.vpk
//! M106<TAB>models/heroes_wip/lash/lash.vmdl_c<TAB>cut-body opening ...<TAB>warn
//! ```
//! Each finding carries the severity the run gave it, since one rule's findings need not share one: M062's cosmetic
//! moves and M071's copies of Valve's own pixels are warnings of rules whose default is an error, and the policy makes
//! fair play lenient or strict. Findings are written before allowances are applied; whoever reads them applies the
//! allow file itself.

use crate::{Finding, Severity, RULES};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Depth {
    /// Every rule, with the mesh family limited to its cheap checks.
    Light,
    /// Every rule including the mesh geometry rules (`--deep`, what `magician release` runs).
    Deep,
}

impl Depth {
    pub fn from_flag(deep: bool) -> Depth {
        if deep { Depth::Deep } else { Depth::Light }
    }
    pub fn label(self) -> &'static str {
        match self {
            Depth::Light => "light",
            Depth::Deep => "deep",
        }
    }
}

fn word(s: Severity) -> &'static str {
    match s {
        Severity::Error => "error",
        Severity::Warn => "warn",
        Severity::Info => "info",
    }
}

fn clean(s: &str) -> String {
    s.replace(['\t', '\n', '\r'], " ")
}

pub fn write(findings: &[Finding], depth: Depth, build: &str, vpk: &str) -> String {
    let mut t = format!("# magician findings depth={} build={} vpk={}\n", depth.label(), clean(build), clean(vpk));
    for f in findings {
        t.push_str(&format!("{}\t{}\t{}\t{}\n", f.code, f.path.as_deref().map(clean).unwrap_or_default(), clean(&f.message), word(f.severity)));
    }
    t
}

/// Read a findings file back, each finding at the severity it was written with. A file from before findings carried
/// one (three columns) reads each at its rule's severity, as that rule judges its message. The name comes from the
/// rule catalogue; an unknown code or severity is refused (the file is from a newer or different Magician).
pub fn read(text: &str) -> Result<(Depth, Vec<Finding>), String> {
    let mut depth = None;
    let mut out = Vec::new();
    for (n, line) in text.lines().enumerate() {
        if let Some(h) = line.strip_prefix("# magician findings") {
            depth = h.split_whitespace().find_map(|kv| kv.strip_prefix("depth=")).map(|d| if d == "deep" { Depth::Deep } else { Depth::Light });
            continue;
        }
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let mut cols = line.splitn(4, '\t');
        let code = cols.next().unwrap_or("");
        let path = cols.next().unwrap_or("");
        let message = cols.next().unwrap_or("").to_string();
        let r = RULES.iter().find(|r| r.code == code).ok_or_else(|| format!("findings line {}: unknown rule {code}", n + 1))?;
        let severity = match cols.next() {
            Some("error") => Severity::Error,
            Some("warn") => Severity::Warn,
            Some("info") => Severity::Info,
            Some(other) => return Err(format!("findings line {}: unknown severity {other:?}", n + 1)),
            // an older file: M062 judges its own messages (`contract_severity`), every other rule has its default
            None if r.code == "M062" => crate::deep::contract_severity(&message),
            None => r.severity,
        };
        out.push(Finding { code: r.code, name: r.name, severity, path: if path.is_empty() { None } else { Some(path.to_string()) }, message });
    }
    let depth = depth.ok_or("not a magician findings file (no '# magician findings depth=...' header)")?;
    Ok((depth, out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule;

    fn f(code: &'static str, path: Option<&str>, msg: &str) -> Finding {
        let r = rule(code);
        Finding { code: r.code, name: r.name, severity: r.severity, path: path.map(String::from), message: msg.into() }
    }

    fn at(severity: Severity, x: Finding) -> Finding {
        Finding { severity, ..x }
    }

    #[test]
    fn round_trip_keeps_depth_and_severity() {
        let all = vec![
            f("M137", Some("models/heroes_wip/lash/lash.vmdl_c"), "absolute white\twith a tab"),
            f("M050", None, "'x.vpk' should be pakNN_dir.vpk"),
        ];
        let (d, back) = read(&write(&all, Depth::Deep, "6864", "pak10_dir.vpk")).unwrap();
        assert_eq!(d, Depth::Deep);
        assert_eq!(back.len(), 2);
        assert_eq!(back[0].severity, Severity::Error);
        assert_eq!(back[0].message, "absolute white with a tab");
        assert_eq!(back[1].path, None);
    }

    #[test]
    fn each_finding_reads_back_at_the_severity_it_was_written_with() {
        // M071 is an error rule whose copy of Valve's own pixels is a warning, and a lenient policy makes a fair-play
        // error a warning: a release's findings file gives the grade that run, not the rules' defaults
        let all = vec![
            at(Severity::Warn, f("M071", Some("materials/default/default_normal_tga_1a2b.vtex_c"), "compiler byproduct with Valve's own picture")),
            f("M071", Some("materials/default/default_color_tga_3c4d.vtex_c"), "compiler byproduct whose pixels differ from Valve's default"),
            at(Severity::Warn, f("M124", Some("models/heroes_wip/lash/lash.vmdl_c"), "silhouette 4% smaller")),
            at(Severity::Warn, f("M062", Some("models/heroes_wip/lash/lash.vmdl_c"), &format!("bone 'ball_end_L' missing: Valve's model hangs nothing on it; {}", crate::carry::NOTHING_CHANGES))),
            f("M062", Some("models/heroes_wip/lash/lash.vmdl_c"), "bone 'pelvis' missing: Valve's model hangs 3139 vertices on it"),
        ];
        let text = write(&all, Depth::Deep, "6875", "pak10_dir.vpk");
        assert!(text.lines().nth(1).unwrap().ends_with("\twarn") && text.lines().nth(2).unwrap().ends_with("\terror"), "{text}");
        let (_, back) = read(&text).unwrap();
        assert_eq!(back, all);
        // a file from before findings carried their severity: each rule's own, and M062 as it judges its message
        let moved = format!("attachment 'idol' moved 4.0 units, 4.0 off the body: on spine at -14.00,0.00,20.00 rot 0.00,0.00,0.00,1.00 (Valve's on spine at -10.00,0.00,20.00 rot 0.00,0.00,0.00,1.00); {}, within 7.0 units of the body", crate::deep::COSMETIC_MOVE);
        let old = format!("# magician findings depth=deep build=6873 vpk=pak10_dir.vpk\nM071\tmaterials/default/x.vtex_c\tcompiler byproduct\nM062\tm.vmdl_c\t{moved}\nM062\tm.vmdl_c\tbone 'pelvis' missing\n");
        let (_, back) = read(&old).unwrap();
        assert_eq!(back.iter().map(|x| x.severity).collect::<Vec<_>>(), vec![Severity::Error, Severity::Warn, Severity::Error]);
        assert!(read("# magician findings depth=deep\nM071\tx\ty\tloud\n").is_err(), "an unknown severity is refused");
    }

    #[test]
    fn refuses_unknown_codes_and_headerless_files() {
        assert!(read("# magician findings depth=deep\nM999\tx\ty\n").is_err());
        assert!(read("M106\tx\ty\n").is_err());
    }
}
