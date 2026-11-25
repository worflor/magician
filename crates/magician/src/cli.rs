//! The command line's contract with whoever calls it: `--help` and `--version` work anywhere and do nothing else, an
//! option no command knows is refused before anything runs, and each exit code has one meaning.
//!
//! | Code | Meaning |
//! |---|---|
//! | 0 | clean: nothing to do |
//! | 1 | broken: a contract or rule failed, a person or an agent must act |
//! | 2 | usage error: bad arguments, a missing file, something that could not run |
//! | 3 | refused on purpose: lint errors at install, a held game seat or build lock, a slot another mod owns |
//! | 4 | re-prove: it loads, but must be proven in game on the new build |
//! | 5 | an update is available (`magician update --check`, and nothing else) |

use std::collections::BTreeSet;

pub const CLEAN: u8 = 0;
pub const BROKEN: u8 = 1;
pub const USAGE: u8 = 2;
pub const REFUSED: u8 = 3;
pub const REPROVE: u8 = 4;
pub const UPDATE_AVAILABLE: u8 = 5;

/// Why a command stopped before its normal end. The message goes to stderr after `magician:`.
#[derive(Debug, Clone, PartialEq)]
pub enum Stop {
    /// Bad arguments, a missing file, a tool that is not there (exit 2).
    Usage(String),
    /// A check found something a person or an agent must fix (exit 1).
    Broken(String),
    /// Refused on purpose, so nothing was changed (exit 3).
    Refused(String),
}

impl Stop {
    pub fn code(&self) -> u8 {
        match self {
            Stop::Usage(_) => USAGE,
            Stop::Broken(_) => BROKEN,
            Stop::Refused(_) => REFUSED,
        }
    }
    pub fn text(&self) -> &str {
        match self {
            Stop::Usage(t) | Stop::Broken(t) | Stop::Refused(t) => t,
        }
    }
}

impl From<String> for Stop {
    fn from(s: String) -> Stop {
        Stop::Usage(s)
    }
}

impl From<&str> for Stop {
    fn from(s: &str) -> Stop {
        Stop::Usage(s.to_string())
    }
}

/// A stop reads as its message, so `e.contains(...)` works on it as on the old `String` errors.
impl std::ops::Deref for Stop {
    type Target = str;
    fn deref(&self) -> &str {
        self.text()
    }
}

impl std::fmt::Display for Stop {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.text())
    }
}

/// `magician <version> (git <commit>[-dirty])`, fixed when the binary was built (build.rs).
pub fn version() -> String {
    format!("magician {} (git {})", env!("CARGO_PKG_VERSION"), env!("MAGICIAN_GIT"))
}

/// The commands that print one JSON document on stdout with `--json` (and nothing else).
pub const JSON_COMMANDS: &[&str] = &["lint", "grade", "drift", "patchday", "release", "rules", "check", "doctor", "prove", "update"];

/// Options every command accepts or that are passed between Magician's own processes, beyond the ones the usage text
/// lists for a command.
const ALWAYS: &[&str] = &["--help", "-h", "--version", "--json", "--no-cache", "--reference", "--policy", "--game", "--addon-root", "--users"];

/// Every option the usage text names (`--word`), plus `ALWAYS`.
pub fn known_options(usage: &str) -> BTreeSet<String> {
    let mut out: BTreeSet<String> = ALWAYS.iter().map(|s| s.to_string()).collect();
    let b = usage.as_bytes();
    let mut i = 0;
    while i + 2 < b.len() {
        if b[i] == b'-' && b[i + 1] == b'-' && (i == 0 || !b[i - 1].is_ascii_alphanumeric() && b[i - 1] != b'-') && b[i + 2].is_ascii_lowercase() {
            let mut j = i + 2;
            while j < b.len() && (b[j].is_ascii_lowercase() || b[j].is_ascii_digit() || b[j] == b'-') {
                j += 1;
            }
            out.insert(usage[i..j].trim_end_matches('-').to_string());
            i = j;
        } else {
            i += 1;
        }
    }
    out
}

/// The edit distance between two words (insertions, deletions, substitutions, and a swap of two neighbours).
pub fn distance(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut d = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i;
    }
    for j in 0..=b.len() {
        d[0][j] = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            d[i][j] = (d[i - 1][j] + 1).min(d[i][j - 1] + 1).min(d[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                d[i][j] = d[i][j].min(d[i - 2][j - 2] + 1);
            }
        }
    }
    d[a.len()][b.len()]
}

/// The candidate a typo most likely meant: the only one it is a prefix of, else the closest within a third of its
/// length (at least one edit), ties to the first. `None` when nothing is close: a guess that far off would mislead.
pub fn nearest<'a>(word: &str, candidates: impl IntoIterator<Item = &'a str>) -> Option<&'a str> {
    let w = word.to_ascii_lowercase();
    let all: Vec<&str> = candidates.into_iter().collect();
    let prefixed: Vec<&str> = all.iter().copied().filter(|c| w.len() >= 3 && c.to_ascii_lowercase().starts_with(&w)).collect();
    if prefixed.len() == 1 {
        return Some(prefixed[0]);
    }
    let limit = (w.chars().count() / 3).max(1);
    let mut best: Option<(usize, &str)> = None;
    for c in all {
        let d = distance(&w, &c.to_ascii_lowercase());
        if d <= limit && best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, c));
        }
    }
    best.map(|(_, c)| c)
}

/// ` (did you mean X?)`, or nothing.
pub fn did_you_mean(word: &str, candidates: &[&str], show: impl Fn(&str) -> String) -> String {
    nearest(word, candidates.iter().copied()).filter(|c| *c != word).map(|c| format!(" (did you mean {}?)", show(c))).unwrap_or_default()
}

/// Refuse an option no command takes (a typo such as `--deeep` would otherwise be ignored, or swallow the argument
/// after it), and `--json` on a command that has no JSON form.
pub fn check_options(args: &[String], usage: &str) -> Result<(), String> {
    let known = known_options(usage);
    // a typo is most likely one of the options of the command it was given to
    let own: Vec<String> = args.first().map(|c| known_options(&help_for(usage, Some(c))).into_iter().collect()).unwrap_or_default();
    for a in args {
        if a.starts_with("--") && a.len() > 2 && !known.contains(a.as_str()) {
            let own_refs: Vec<&str> = own.iter().map(String::as_str).filter(|o| o.starts_with("--")).collect();
            let all: Vec<&str> = known.iter().map(String::as_str).filter(|o| o.starts_with("--")).collect();
            let mut hint = did_you_mean(a, &own_refs, |c| c.to_string());
            if hint.is_empty() {
                hint = did_you_mean(a, &all, |c| c.to_string());
            }
            let cmd = args.first().filter(|c| !c.starts_with('-')).map(|c| format!("magician {c} --help")).unwrap_or_else(|| "magician --help".into());
            return Err(format!("unknown option {a}{hint}; `{cmd}` lists the options"));
        }
    }
    if let Some(cmd) = args.first() {
        if args.iter().any(|a| a == "--json") && !JSON_COMMANDS.contains(&cmd.as_str()) {
            return Err(format!("`magician {cmd}` has no --json form; it is available on {}", JSON_COMMANDS.join(", ")));
        }
    }
    Ok(())
}

/// Options that take no value whichever command they are on.
const ALWAYS_SWITCHES: &[&str] = &["--help", "-h", "--version", "--json", "--no-cache"];

/// The arguments with the command first: options every command takes may come before it (`magician --no-cache lint
/// x.vpk --json`, `magician --reference D:\ref patchday`), and they move behind it, so the command is what dispatch
/// and the --json check read.
pub fn command_first(args: &[String]) -> Vec<String> {
    let mut lead = Vec::new();
    let mut i = 0;
    while i < args.len() && args[i].starts_with("--") && ALWAYS.contains(&args[i].as_str()) {
        let takes_value = !ALWAYS_SWITCHES.contains(&args[i].as_str());
        let n = if takes_value { 2 } else { 1 };
        lead.extend(args[i..(i + n).min(args.len())].iter().cloned());
        i += n;
    }
    let mut out: Vec<String> = args[i.min(args.len())..].to_vec();
    if !out.is_empty() {
        out.splice(1..1, lead);
    } else {
        out = lead;
    }
    out
}

/// Options the usage text marks repeatable (`[--with <addon>]...`), plus the expression bounds.
fn repeatable(usage: &str) -> BTreeSet<String> {
    let mut out: BTreeSet<String> = ["--attr", "--cap"].iter().map(|s| s.to_string()).collect();
    for (i, _) in usage.match_indices("]...") {
        if let Some(open) = usage[..i].rfind('[') {
            if let Some(name) = usage[open + 1..i].split_whitespace().next().filter(|n| n.starts_with("--")) {
                out.insert(name.to_string());
            }
        }
    }
    out
}

/// Refuse an option that takes a value and has none (`install x.vpk --as`, `lint x.vpk --game`: without this the
/// flag is dropped and the default used, which for `--game` is the real game), one whose "value" is the next option
/// (`--reference --deep`), and a single-valued option given twice (which one wins would be a guess). `switches` are
/// the options that take no value.
pub fn check_values(args: &[String], switches: &[&str], usage: &str) -> Result<(), String> {
    let many = repeatable(usage);
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        if a.starts_with("--") && a.len() > 2 && !switches.contains(&a) && !ALWAYS_SWITCHES.contains(&a) {
            match args.get(i + 1) {
                None => return Err(format!("{a} needs a value")),
                Some(v) if v.starts_with("--") && v.len() > 2 => return Err(format!("{a} needs a value, and {v} is an option")),
                Some(_) => {}
            }
            if !seen.insert(a) && !many.contains(a) {
                return Err(format!("{a} is given twice; it takes one value"));
            }
            i += 2;
            continue;
        }
        i += 1;
    }
    Ok(())
}

/// Whether the arguments ask for help (`--help`, `-h`, or `help` as the command).
pub fn wants_help(args: &[String]) -> bool {
    args.first().is_some_and(|a| a == "help") || args.iter().any(|a| a == "--help" || a == "-h")
}

/// The usage for `magician <cmd>`: the paragraphs of the usage text that document it (a line `  magician <cmd> ...` and
/// its indented continuation lines), then the exit codes. The whole text when `cmd` is not a command it documents.
pub fn help_for(usage: &str, cmd: Option<&str>) -> String {
    let Some(cmd) = cmd.filter(|c| *c != "help" && !c.starts_with('-')) else { return usage.to_string() };
    let head = format!("  magician {cmd}");
    let mut out = String::new();
    let mut inside = false;
    for line in usage.lines() {
        if line.starts_with("  magician ") {
            inside = line == head || line.starts_with(&format!("{head} ")) || line.starts_with(&format!("{head}|"));
        } else if !line.starts_with("    ") {
            inside = false;
        }
        if inside {
            out.push_str(line);
            out.push('\n');
        }
    }
    if out.is_empty() {
        return usage.to_string();
    }
    if let Some(i) = usage.find("Exit codes:") {
        out.push('\n');
        let tail = &usage[i..];
        let end = tail.find("\n\n").unwrap_or(tail.len());
        out.push_str(&tail[..end]);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const U: &str = "Tagline.\n\n  magician lint <mod.vpk> [--deep] [--allow FILE]\n                [--sources DIR]...   addon trees\n  magician ls <vpk>     list\n  magician scene make <x> --out DIR\n  magician scene check|compose <patch>\n\nExit codes: 0 clean; 1 broken.\nMore: 2 usage.\n\nLocations: --game DIR.";

    fn a(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn options_come_from_the_usage_text() {
        let k = known_options(U);
        for o in ["--deep", "--allow", "--sources", "--out", "--game", "--help", "--policy"] {
            assert!(k.contains(o), "{o} missing from {k:?}");
        }
        assert!(!k.contains("--deeep"));
    }

    #[test]
    fn an_unknown_option_is_a_usage_error_naming_it() {
        assert!(check_options(&a(&["lint", "x.vpk", "--deep"]), U).is_ok());
        let e = check_options(&a(&["lint", "x.vpk", "--deeep"]), U).unwrap_err();
        assert!(e.contains("--deeep"), "{e}");
        // `-` alone and negative numbers are values, not options
        assert!(check_options(&a(&["expr", "-1"]), U).is_ok());
    }

    #[test]
    fn an_option_that_takes_a_value_needs_one_and_takes_it_once() {
        const W: &str = "  magician lint <mod.vpk> [--deep] [--allow FILE]... [--game DIR]\n  magician install <pak> [--as pakNN_dir.vpk]\n";
        let sw = &["--deep"];
        assert!(check_values(&a(&["lint", "x.vpk", "--deep", "--game", "D:\\g", "--allow", "a", "--allow", "b"]), sw, W).is_ok());
        // last on the line: the flag would be dropped and the default (the real game) used
        assert_eq!(check_values(&a(&["install", "x.vpk", "--as"]), sw, W), Err("--as needs a value".to_string()));
        assert_eq!(check_values(&a(&["lint", "x.vpk", "--game"]), sw, W), Err("--game needs a value".to_string()));
        // the next option is not its value
        assert!(check_values(&a(&["lint", "--reference", "--deep", "x.vpk"]), sw, W).unwrap_err().contains("--deep is an option"));
        // one value, given twice: which wins would be a guess; a repeatable one is fine
        assert!(check_values(&a(&["lint", "x", "--game", "a", "--game", "b"]), sw, W).unwrap_err().contains("--game is given twice"));
        // values that look like numbers or dashes are values
        assert!(check_values(&a(&["expr", "--cap", "x=-1", "--cap", "y=-2"]), sw, W).is_ok());
        assert!(check_values(&a(&["lint", "x", "--json", "--no-cache"]), sw, W).is_ok(), "switches every command takes");
    }

    #[test]
    fn options_every_command_takes_may_come_before_it() {
        assert_eq!(command_first(&a(&["--no-cache", "lint", "x.vpk", "--json"])), a(&["lint", "--no-cache", "x.vpk", "--json"]));
        assert_eq!(command_first(&a(&["--reference", "D:\\ref", "--json", "patchday"])), a(&["patchday", "--reference", "D:\\ref", "--json"]));
        assert_eq!(command_first(&a(&["lint", "x.vpk"])), a(&["lint", "x.vpk"]));
        assert_eq!(command_first(&a(&["--deep", "lint"])), a(&["--deep", "lint"]), "a command's own option stays where it is");
        assert_eq!(command_first(&a(&["--reference"])), a(&["--reference"]));
        assert!(check_options(&command_first(&a(&["--no-cache", "lint", "x.vpk", "--json"])), U).is_ok());
    }

    #[test]
    fn a_typo_is_answered_with_what_it_most_likely_meant() {
        assert_eq!(distance("lint", "lnit"), 1);
        assert_eq!(distance("patchday", "patchdya"), 1);
        assert_eq!(nearest("lnit", ["ls", "lint", "index"]), Some("lint"));
        assert_eq!(nearest("pat", ["paths", "patchday", "pack"]), None, "a prefix of two is not a guess");
        assert_eq!(nearest("patc", ["paths", "patchday", "pack"]), Some("patchday"));
        assert_eq!(nearest("xyz", ["ls", "lint"]), None, "nothing close is nothing");
        assert_eq!(nearest("Lahs", ["lash", "haze"]), Some("lash"));
        // an option is matched among the command's own first
        let e = check_options(&a(&["lint", "x.vpk", "--deeep"]), U).unwrap_err();
        assert!(e.contains("did you mean --deep?") && e.contains("`magician lint --help`"), "{e}");
        let e = check_options(&a(&["lint", "x.vpk", "--zzzzzzz"]), U).unwrap_err();
        assert!(!e.contains("did you mean"), "{e}");
    }

    #[test]
    fn json_only_where_a_command_has_one() {
        assert!(check_options(&a(&["lint", "x.vpk", "--json"]), U).is_ok());
        assert!(check_options(&a(&["ls", "x.vpk", "--json"]), U).unwrap_err().contains("no --json"));
    }

    #[test]
    fn help_is_asked_for_anywhere_and_is_specific_to_the_command() {
        assert!(wants_help(&a(&["baseline", "--help"])) && wants_help(&a(&["-h"])) && wants_help(&a(&["help", "lint"])));
        assert!(!wants_help(&a(&["grade", "x.vpk", "--hero", "lash"])));
        let h = help_for(U, Some("lint"));
        assert!(h.starts_with("  magician lint <mod.vpk>") && h.contains("--sources DIR") && !h.contains("magician ls"), "{h}");
        assert!(h.contains("Exit codes: 0 clean; 1 broken.\nMore: 2 usage."), "{h}");
        assert!(!h.contains("Locations"), "{h}");
        let s = help_for(U, Some("scene"));
        assert!(s.contains("scene make") && s.contains("scene check|compose"), "{s}");
        assert_eq!(help_for(U, Some("nonsense")), U);
        assert_eq!(help_for(U, None), U);
    }

    #[test]
    fn each_stop_has_its_own_exit_code() {
        assert_eq!(Stop::from("bad").code(), USAGE);
        assert_eq!(Stop::Broken("x".into()).code(), BROKEN);
        assert_eq!(Stop::Refused("seat held".into()).code(), REFUSED);
        assert!(Stop::Refused("seat held by lash-fix".into()).contains("lash-fix"));
        assert_ne!(REPROVE, CLEAN);
    }

    #[test]
    fn the_version_names_the_commit() {
        let v = version();
        assert!(v.starts_with(&format!("magician {} (git ", env!("CARGO_PKG_VERSION"))) && v.ends_with(')'), "{v}");
    }
}
