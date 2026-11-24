//! Reading a mod's `mod.toml`, line by line (the file is flat enough that a TOML parser isn't worth a dependency).
//! The keys Magician reads:
//!
//! ```text
//! [mod]
//! slug   = "example-skin"            # the install-ledger owner is the slug with '-' as '_'
//! name   = "Example Skin"
//! hero   = "hero_magician"              # `grade --mod` grades against this hero
//! owners = ["example_voice"]           # other ledger owners the mod installs under (patch day)
//! [target]
//! model = "models/heroes_staging/magician_v2/magician.vmdl"
//! [proven]
//! builds   = [6864]                     # written by `magician prove`, never by hand
//! evidence = ["proof/kit-hud.png"]      # what the proof was (listed in the grade record)
//! [identity]
//! name  = "Example Skin"             # what a replaced name logo reads (default: [mod] name)
//! voice = "new"                         # a new voice identity, declared (bytes can't tell it from a treatment)
//! [rights]
//! state = "unreviewed"                  # unreviewed | cleared | denied: a person's call, never Magician's
//! notes = "original modelling only"
//! ```

/// A quoted string value of `key` in `[section]`.
pub fn string(toml: &str, section: &str, key: &str) -> Option<String> {
    value(toml, section, key).and_then(|v| v.trim().strip_prefix('"').and_then(|x| x.split('"').next()).map(String::from))
}

/// A one-line array of quoted strings (`key = ["a", "b"]  # note`) in `[section]`.
pub fn list(toml: &str, section: &str, key: &str) -> Vec<String> {
    value(toml, section, key).map(|v| v.split('#').next().unwrap_or("").split('"').skip(1).step_by(2).map(String::from).collect()).unwrap_or_default()
}

/// The builds a mod was proven on (`[proven] builds = [6861, "6864"]`), ascending, each once.
pub fn proven(toml: &str) -> Vec<String> {
    let Some(v) = value(toml, "proven", "builds") else { return Vec::new() };
    let body = v.split('#').next().unwrap_or("");
    let mut out: Vec<String> = body.split(|c: char| !c.is_ascii_digit()).filter(|s| !s.is_empty()).map(String::from).collect();
    out.sort_by_key(|n| n.parse::<u64>().unwrap_or(0));
    out.dedup();
    out
}

/// The section a line opens: `[name]` (spaces inside the brackets allowed, a comment after them), or `None` for a
/// line that is not a header. `[[name]]` is not `[name]`. Every reader and writer of `mod.toml` (grade, patchday,
/// `prove`) decides sections with this, so a build `prove` writes is the build they read.
pub fn section_of(line: &str) -> Option<&str> {
    let t = line.trim();
    t.strip_prefix('[')?.split_once(']').map(|(name, _)| name.trim())
}

/// The raw text after `key =` in `[section]`.
fn value<'a>(toml: &'a str, section: &str, key: &str) -> Option<&'a str> {
    let mut inside = false;
    for line in toml.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            inside = section_of(t) == Some(section);
        } else if inside {
            if let Some((k, v)) = t.split_once('=') {
                if k.trim() == key {
                    return Some(v);
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_keys_magician_uses() {
        let t = "# a mod\n[mod]\nslug = \"example-skin\"  # note\nhero = \"hero_magician\"\nowners = [\"example_voice\", \"example_showcase\"]\n[proven]\nbuilds = [6864, \"6861\", 6864]  # by the harness\nevidence = [\"proof/a.png\"]\n[rights]\nstate = \"cleared\"\n";
        assert_eq!(string(t, "mod", "slug").as_deref(), Some("example-skin"));
        assert_eq!(string(t, "mod", "hero").as_deref(), Some("hero_magician"));
        assert_eq!(list(t, "mod", "owners"), vec!["example_voice", "example_showcase"]);
        assert_eq!(proven(t), vec!["6861", "6864"]);
        assert_eq!(list(t, "proven", "evidence"), vec!["proof/a.png"]);
        assert_eq!(string(t, "rights", "state").as_deref(), Some("cleared"));
        assert_eq!(string(t, "rights", "notes"), None);
        assert_eq!(string(t, "mod", "name"), None);
        assert!(proven("[mod]\nslug = \"x\"\n").is_empty());
    }
}
