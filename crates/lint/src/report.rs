//! A lint run written down, so a second step (the grade inside `magician release`) judges exactly the run
//! the first step performed, at the same depth, instead of re-linting at its own.
//!
//! ```text
//! # magician findings depth=deep build=6864 vpk=pak10_dir.vpk
//! M106<TAB>models/heroes_wip/lash/lash.vmdl_c<TAB>cut-body opening ...
//! ```
//! Findings are written before allowances are applied; whoever reads them applies the allow file itself.

use crate::{Finding, RULES};

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

fn clean(s: &str) -> String {
    s.replace(['\t', '\n', '\r'], " ")
}

pub fn write(findings: &[Finding], depth: Depth, build: &str, vpk: &str) -> String {
    let mut t = format!("# magician findings depth={} build={} vpk={}\n", depth.label(), clean(build), clean(vpk));
    for f in findings {
        t.push_str(&format!("{}\t{}\t{}\n", f.code, f.path.as_deref().map(clean).unwrap_or_default(), clean(&f.message)));
    }
    t
}

/// Read a findings file back. Severity and name come from the rule catalogue, so a file can't claim an error
/// is a warning; an unknown code is refused (the file is from a newer or different Magician).
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
        let mut cols = line.splitn(3, '\t');
        let code = cols.next().unwrap_or("");
        let path = cols.next().unwrap_or("");
        let message = cols.next().unwrap_or("").to_string();
        let r = RULES.iter().find(|r| r.code == code).ok_or_else(|| format!("findings line {}: unknown rule {code}", n + 1))?;
        out.push(Finding {
            code: r.code,
            name: r.name,
            severity: r.severity,
            path: if path.is_empty() { None } else { Some(path.to_string()) },
            message,
        });
    }
    let depth = depth.ok_or("not a magician findings file (no '# magician findings depth=...' header)")?;
    Ok((depth, out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{rule, Severity};

    fn f(code: &'static str, path: Option<&str>, msg: &str) -> Finding {
        let r = rule(code);
        Finding { code: r.code, name: r.name, severity: r.severity, path: path.map(String::from), message: msg.into() }
    }

    #[test]
    fn round_trip_keeps_depth_and_severity() {
        let all = vec![
            f("M106", Some("models/heroes_wip/lash/lash.vmdl_c"), "cut-body opening\twith a tab"),
            f("M050", None, "'x.vpk' should be pakNN_dir.vpk"),
        ];
        let (d, back) = read(&write(&all, Depth::Deep, "6864", "pak10_dir.vpk")).unwrap();
        assert_eq!(d, Depth::Deep);
        assert_eq!(back.len(), 2);
        assert_eq!(back[0].severity, Severity::Error);
        assert_eq!(back[0].message, "cut-body opening with a tab");
        assert_eq!(back[1].path, None);
    }

    #[test]
    fn refuses_unknown_codes_and_headerless_files() {
        assert!(read("# magician findings depth=deep\nM999\tx\ty\n").is_err());
        assert!(read("M106\tx\ty\n").is_err());
    }
}
