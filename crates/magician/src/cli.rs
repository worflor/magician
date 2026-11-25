//! The command line's contract with whoever calls it: `--help` and `--version` work anywhere and do nothing else, an
//! option the command it was given to does not take is refused before anything runs, and each exit code has one
//! meaning.
//!
//! Where a command's options are declared: its own paragraphs of the usage text (`  magician <cmd> ...` and the
//! lines indented under it) name the options only it takes; `Shared` names, once, the options several commands take
//! (the locations, the policy, the reference folder) and which commands those are; `Spec::inherits` names a command
//! that hands its arguments on to another's work (`pack` lints with lint's options). The check, `magician help
//! <cmd>` and the usage text's shared section are all read from these, so what a command accepts and what its help
//! lists cannot part.
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

/// Options every command takes, answered before any other check: help and the version (they run nothing), and
/// `--json`, whose own check (`JSON_COMMANDS`) says where it is available.
const ANYWHERE: &[&str] = &["--help", "-h", "--version", "--json"];

/// Which commands take an option several share.
#[derive(Debug, Clone, Copy)]
pub enum Takers {
    /// Every command Magician runs itself (not the front-door verbs, whose arguments are their tool's own).
    Every,
    Only(&'static [&'static str]),
}

/// An option several commands take, declared once: which commands take it, the value it wants (empty for a switch)
/// and what it does. The usage text's section on these and each command's help are written from it.
#[derive(Debug, Clone, Copy)]
pub struct Shared {
    pub name: &'static str,
    pub value: &'static str,
    pub takers: Takers,
    pub what: &'static str,
}

/// The command line's declaration (see the module comment).
pub struct Spec<'a> {
    pub usage: &'a str,
    pub shared: &'a [Shared],
    /// `(command, other)`: the command also takes `other`'s own options (but not its `--json`), because it runs
    /// `other`'s work with its own arguments.
    pub inherits: &'a [(&'a str, &'a str)],
    /// Verbs whose arguments go untouched to a tool of their own: never checked here, and they share nothing.
    pub passthrough: &'a [&'a str],
}

impl Spec<'_> {
    /// Every command the usage text documents, in its order (the front-door verbs included).
    pub fn commands(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for l in self.usage.lines() {
            if let Some(rest) = l.strip_prefix("  magician ") {
                let c: String = rest.chars().take_while(|c| c.is_ascii_lowercase() || *c == '-').collect();
                if !c.is_empty() && !out.contains(&c) {
                    out.push(c);
                }
            }
        }
        out
    }

    fn shared_by(&self, cmd: &str) -> impl Iterator<Item = &Shared> + '_ {
        let native = !self.passthrough.contains(&cmd);
        let cmd = cmd.to_string();
        self.shared.iter().filter(move |s| {
            native
                && match s.takers {
                    Takers::Every => true,
                    Takers::Only(l) => l.contains(&cmd.as_str()),
                }
        })
    }

    /// The options a command's own paragraphs name, less the shared ones (those are taken where `Shared` says).
    pub fn own(&self, cmd: &str) -> BTreeSet<String> {
        options_named(&block(self.usage, cmd)).into_iter().filter(|o| !self.shared.iter().any(|s| s.name == o)).collect()
    }

    /// What a command takes from another's options (`Spec::inherits`), less what it takes on its own.
    pub fn inherited(&self, cmd: &str) -> BTreeSet<String> {
        let own = self.own(cmd);
        self.inherits.iter().filter(|(c, _)| *c == cmd).flat_map(|(_, from)| self.own(from)).filter(|o| o != "--json" && !own.contains(o)).collect()
    }

    /// Every option `cmd` accepts: its own, what it inherits, the shared ones it is among the takers of, and help.
    pub fn takes(&self, cmd: &str) -> BTreeSet<String> {
        let mut out = self.own(cmd);
        out.extend(self.inherited(cmd));
        out.extend(self.shared_by(cmd).map(|s| s.name.to_string()));
        out.extend(ANYWHERE.iter().map(|s| s.to_string()));
        out
    }

    /// The commands that take `opt`, in the usage text's order.
    pub fn takers_of(&self, opt: &str) -> Vec<String> {
        self.commands().into_iter().filter(|c| self.takes(c).contains(opt)).collect()
    }

    /// Every option some command takes.
    pub fn all(&self) -> BTreeSet<String> {
        self.commands().iter().flat_map(|c| self.takes(c)).collect()
    }

    /// `magician help <cmd>`: its own paragraphs, what it inherits and shares, then the exit codes. The whole usage
    /// text when `cmd` is not a command it documents.
    pub fn help(&self, cmd: Option<&str>) -> String {
        let Some(cmd) = cmd.filter(|c| *c != "help" && !c.starts_with('-')) else { return self.usage.to_string() };
        let mut out = block(self.usage, cmd);
        if out.is_empty() {
            return self.usage.to_string();
        }
        let mut also: Vec<String> = Vec::new();
        for (_, from) in self.inherits.iter().filter(|(c, _)| *c == cmd) {
            let names: Vec<String> = self.inherited(cmd).into_iter().collect();
            also.push(format!("{from}'s options, for the {from} it runs (`magician help {from}`): {}", names.join(", ")));
        }
        let shared: Vec<String> = self.shared_by(cmd).map(|s| if s.value.is_empty() { s.name.to_string() } else { format!("{} {}", s.name, s.value) }).collect();
        if !shared.is_empty() {
            also.push(format!("the options it shares with other commands (`magician help` says what each does): {}", shared.join(", ")));
        }
        if !also.is_empty() {
            out.push('\n');
            out.push_str(&wrap_at("", &format!("Also {}.", also.join("; and ")), 0));
        }
        if let Some(i) = self.usage.find("Exit codes:") {
            out.push('\n');
            let tail = &self.usage[i..];
            let end = tail.find("\n\n").unwrap_or(tail.len());
            out.push_str(&tail[..end]);
            out.push('\n');
        }
        out
    }
}

/// The shared options, one entry each in the usage text's layout, written from their declaration.
pub fn shared_section(shared: &[Shared]) -> String {
    let mut out = String::from("Options several commands take (`magician help <command>` lists every option a command takes):\n");
    for s in shared {
        let head = if s.value.is_empty() { format!("  {}", s.name) } else { format!("  {} {}", s.name, s.value) };
        let who = match s.takers {
            Takers::Every => "every command".to_string(),
            Takers::Only(l) => and_list(l),
        };
        out.push_str(&wrap(&head, &format!("{who}: {}", s.what)));
    }
    out
}

/// `a`, `a and b`, `a, b and c`.
fn and_list<S: AsRef<str>>(items: &[S]) -> String {
    let v: Vec<&str> = items.iter().map(AsRef::as_ref).collect();
    match v.split_last() {
        None => String::new(),
        Some((last, [])) => last.to_string(),
        Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
    }
}

/// The column the usage text's descriptions start at, and the width it keeps to.
const DESC_COL: usize = 48;
const WIDTH: usize = 120;

/// One entry in the usage text's layout: `head`, then `text` from the description column, wrapped to the width.
fn wrap(head: &str, text: &str) -> String {
    wrap_at(head, text, DESC_COL)
}

/// `wrap`, with the text from column `col`.
fn wrap_at(head: &str, text: &str, col: usize) -> String {
    let mut out = String::new();
    let mut line = if head.is_empty() || head.len() < col { format!("{head:<col$}") } else { format!("{head}\n{:col$}", "") };
    let mut fresh = true;
    for w in text.split(' ') {
        let width = line.rsplit('\n').next().unwrap_or("").len();
        if !fresh && width + 1 + w.len() > WIDTH {
            out.push_str(line.trim_end());
            out.push('\n');
            line = format!("{:col$}", "");
            fresh = true;
        }
        if !fresh {
            line.push(' ');
        }
        line.push_str(w);
        fresh = false;
    }
    out.push_str(line.trim_end());
    out.push('\n');
    out
}

/// Every option a text names (`--word`), as written.
pub fn options_named(usage: &str) -> BTreeSet<String> {
    let mut out: BTreeSet<String> = BTreeSet::new();
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

/// Refuse an option the command it was given to does not take, before anything runs: a typo such as `--deeep` would
/// otherwise be ignored or swallow the argument after it, and an option borrowed from another command would be
/// ignored while the command did its real work (`install --dry-run` installed). Also `--json` on a command that has
/// no JSON form. With no documented command first, an option no command takes is refused (the command's own error
/// follows for the rest).
pub fn check_options(args: &[String], spec: &Spec) -> Result<(), String> {
    let all = spec.all();
    let cmd = args.first().filter(|c| spec.commands().contains(c));
    let takes = cmd.map(|c| spec.takes(c)).unwrap_or_else(|| all.clone());
    // the options a typo is matched against: help and the version are never what an option was meant to be
    let long = |s: &BTreeSet<String>| -> Vec<String> { s.iter().filter(|o| o.starts_with("--") && !ANYWHERE.contains(&o.as_str())).cloned().collect() };
    let (takes_l, all_l) = (long(&takes), long(&all));
    fn refs(v: &[String]) -> Vec<&str> {
        v.iter().map(String::as_str).collect()
    }
    for a in args {
        if !a.starts_with("--") || a.len() <= 2 || takes.contains(a.as_str()) {
            continue;
        }
        // a typo is most likely one of the options of the command it was given to
        let mut hint = did_you_mean(a, &refs(&takes_l), |c| c.to_string());
        if !all.contains(a.as_str()) {
            if hint.is_empty() {
                hint = did_you_mean(a, &refs(&all_l), |c| c.to_string());
            }
            let help = cmd.map(|c| format!("magician {c} --help")).unwrap_or_else(|| "magician --help".into());
            return Err(format!("unknown option {a}{hint}; `{help}` lists the options"));
        }
        let Some(c) = cmd else { continue };
        let who: Vec<String> = spec.takers_of(a).iter().map(|t| format!("`{t}`")).collect();
        let verb = if who.len() == 1 { "takes" } else { "take" };
        let near = hint.trim().trim_start_matches('(').trim_end_matches(')').to_string();
        let near = if near.is_empty() { String::new() } else { format!("; {near}") };
        return Err(format!("{a} is not an option of `magician {c}` ({} {verb} it{near}); `magician help {c}` lists the ones it takes", and_list(&who)));
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

/// The arguments with the command first: the shared options may come before it (`magician --no-cache lint x.vpk
/// --json`, `magician --reference D:\ref patchday`), and they move behind it, so the command is what dispatch and
/// the option checks read (and a shared option the command does not take is refused there).
pub fn command_first(args: &[String], shared: &[Shared]) -> Vec<String> {
    let lead_value = |a: &str| -> Option<bool> {
        if ANYWHERE.contains(&a) {
            return Some(false);
        }
        shared.iter().find(|s| s.name == a).map(|s| !s.value.is_empty())
    };
    let mut lead = Vec::new();
    let mut i = 0;
    while i < args.len() && args[i].starts_with("--") && lead_value(&args[i]).is_some() {
        let takes_value = lead_value(&args[i]) == Some(true);
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

/// The paragraphs of the usage text that document `magician <cmd>` (a line `  magician <cmd> ...` and its indented
/// continuation lines); empty when it documents no such command.
pub fn block(usage: &str, cmd: &str) -> String {
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
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const U: &str = "Tagline.\n\n  magician lint <mod.vpk> [--deep] [--allow FILE] [--json]\n                [--sources DIR]...   addon trees\n  magician ls <vpk>     list\n  magician pack <src> <out>   then lint it\n  magician install <pak> [--as NAME] [--force]\n  magician build <addon> [--force-compile]\n  magician sign <dir> [--dry-run]\n  magician update [--dry-run] [--check]\n  magician scene make <x> --out DIR\n  magician scene check|compose <patch> [--game DIR]\n  magician session [--plan] [--out DIR]\n\nExit codes: 0 clean; 1 broken.\nMore: 2 usage.\n\nLocations: --game DIR.";
    const SHARED: &[Shared] = &[
        Shared { name: "--no-cache", value: "", takers: Takers::Every, what: "no cache" },
        Shared { name: "--game", value: "DIR", takers: Takers::Only(&["lint", "pack", "install", "scene"]), what: "the game folder" },
    ];
    const SPEC: Spec = Spec { usage: U, shared: SHARED, inherits: &[("pack", "lint"), ("install", "lint")], passthrough: &["session"] };

    fn a(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_commands_options_are_its_own_paragraphs_what_it_inherits_and_what_it_shares() {
        assert_eq!(SPEC.commands(), a(&["lint", "ls", "pack", "install", "build", "sign", "update", "scene", "session"]));
        let set = |v: &[&str]| -> BTreeSet<String> { v.iter().map(|s| s.to_string()).collect() };
        assert_eq!(SPEC.own("lint"), set(&["--deep", "--allow", "--json", "--sources"]));
        assert_eq!(SPEC.own("scene"), set(&["--out"]), "a shared option named in a block is taken where Shared says");
        assert_eq!(SPEC.inherited("install"), set(&["--deep", "--allow", "--sources"]), "never another's --json");
        let t = SPEC.takes("install");
        for o in ["--as", "--deep", "--allow", "--sources", "--game", "--no-cache", "--help", "--version"] {
            assert!(t.contains(o), "install takes {o}: {t:?}");
        }
        assert!(!t.contains("--dry-run") && !t.contains("--out"), "{t:?}");
        assert!(!SPEC.takes("ls").contains("--game") && SPEC.takes("ls").contains("--no-cache"));
        assert!(!SPEC.takes("session").contains("--no-cache"), "a front-door verb shares nothing");
        assert_eq!(SPEC.takers_of("--dry-run"), a(&["sign", "update"]));
        assert!(!options_named(U).contains("--deeep"));
    }

    #[test]
    fn an_option_its_command_does_not_take_is_refused_naming_the_ones_that_do() {
        assert!(check_options(&a(&["lint", "x.vpk", "--deep"]), &SPEC).is_ok());
        assert!(check_options(&a(&["install", "x.vpk", "--deep", "--game", "g", "--as", "pak10_dir.vpk"]), &SPEC).is_ok());
        let e = check_options(&a(&["install", "x.vpk", "--dry-run"]), &SPEC).unwrap_err();
        assert_eq!(e, "--dry-run is not an option of `magician install` (`sign` and `update` take it); `magician help install` lists the ones it takes");
        let e = check_options(&a(&["install", "x.vpk", "--check"]), &SPEC).unwrap_err();
        assert!(e.starts_with("--check is not an option of `magician install` (`update` takes it)"), "{e}");
        let e = check_options(&a(&["sign", "x", "--deep"]), &SPEC).unwrap_err();
        assert!(e.contains("(`lint`, `pack` and `install` take it)"), "{e}");
        // the nearest option the command does take, when there is one
        let e = check_options(&a(&["build", "x", "--force"]), &SPEC).unwrap_err();
        assert!(e.contains("(`install` takes it; did you mean --force-compile?)"), "{e}");
        // a front-door verb's options count as its own
        let e = check_options(&a(&["install", "x", "--out", "y"]), &SPEC).unwrap_err();
        assert!(e.contains("(`scene` and `session` take it)"), "{e}");
        assert!(check_options(&a(&["ls", "x", "--game", "g"]), &SPEC).unwrap_err().contains("(`lint`, `pack`, `install` and `scene` take it)"));
        // an undocumented command is answered by dispatch; an option no command takes is refused anyway
        assert!(check_options(&a(&["nonsense", "--dry-run"]), &SPEC).is_ok());
        assert!(check_options(&a(&["nonsense", "--deeep"]), &SPEC).is_err());
        let e = check_options(&a(&["lint", "x.vpk", "--deeep"]), &SPEC).unwrap_err();
        assert!(e.contains("unknown option --deeep"), "{e}");
        // `-` alone and negative numbers are values, not options
        assert!(check_options(&a(&["expr", "-1"]), &SPEC).is_ok());
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
    fn shared_options_may_come_before_the_command() {
        let first = |v: &[&str]| command_first(&a(v), SHARED);
        assert_eq!(first(&["--no-cache", "lint", "x.vpk", "--json"]), a(&["lint", "--no-cache", "x.vpk", "--json"]));
        assert_eq!(first(&["--game", "D:\\g", "--json", "lint"]), a(&["lint", "--game", "D:\\g", "--json"]));
        assert_eq!(first(&["lint", "x.vpk"]), a(&["lint", "x.vpk"]));
        assert_eq!(first(&["--deep", "lint"]), a(&["--deep", "lint"]), "a command's own option stays where it is");
        assert_eq!(first(&["--game"]), a(&["--game"]));
        assert!(check_options(&first(&["--no-cache", "lint", "x.vpk", "--json"]), &SPEC).is_ok());
        // moved behind the command, it is checked as that command's
        assert!(check_options(&first(&["--game", "g", "ls", "x.vpk"]), &SPEC).is_err());
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
        let e = check_options(&a(&["lint", "x.vpk", "--deeep"]), &SPEC).unwrap_err();
        assert!(e.contains("did you mean --deep?") && e.contains("`magician lint --help`"), "{e}");
        let e = check_options(&a(&["lint", "x.vpk", "--zzzzzzz"]), &SPEC).unwrap_err();
        assert!(!e.contains("did you mean"), "{e}");
    }

    #[test]
    fn json_only_where_a_command_has_one() {
        assert!(check_options(&a(&["lint", "x.vpk", "--json"]), &SPEC).is_ok());
        assert_eq!(check_options(&a(&["ls", "x.vpk", "--json"]), &SPEC).unwrap_err(), format!("`magician ls` has no --json form; it is available on {}", JSON_COMMANDS.join(", ")));
        assert!(check_options(&a(&["pack", "x", "y", "--json"]), &SPEC).unwrap_err().contains("no --json"), "inherited options leave --json out");
    }

    #[test]
    fn help_is_asked_for_anywhere_and_is_specific_to_the_command() {
        assert!(wants_help(&a(&["baseline", "--help"])) && wants_help(&a(&["-h"])) && wants_help(&a(&["help", "lint"])));
        assert!(!wants_help(&a(&["grade", "x.vpk", "--hero", "lash"])));
        let h = SPEC.help(Some("lint"));
        assert!(h.starts_with("  magician lint <mod.vpk>") && h.contains("--sources DIR") && !h.contains("magician ls"), "{h}");
        assert!(h.contains("Exit codes: 0 clean; 1 broken.\nMore: 2 usage."), "{h}");
        assert!(h.contains("

Also the options it shares with other commands (`magician help` says what each does): --no-cache, --game DIR.

Exit"), "{h}");
        assert!(!h.contains("Locations"), "{h}");
        let p = SPEC.help(Some("pack"));
        assert!(p.contains("
Also lint's options, for the lint it runs (`magician help lint`): --allow, --deep, --sources; and the options"), "{p}");
        let s = SPEC.help(Some("scene"));
        assert!(s.contains("scene make") && s.contains("scene check|compose"), "{s}");
        assert!(!SPEC.help(Some("session")).contains("shares"), "a front-door verb's options are its tool's");
        assert_eq!(SPEC.help(Some("nonsense")), U);
        assert_eq!(SPEC.help(None), U);
    }

    #[test]
    fn the_shared_section_is_written_from_the_declaration_in_the_usage_texts_layout() {
        let s = shared_section(SHARED);
        assert!(s.starts_with("Options several commands take"), "{s}");
        assert!(s.contains(&format!("\n  --no-cache{}every command: no cache\n", " ".repeat(DESC_COL - 12))), "{s}");
        assert!(s.contains("  --game DIR") && s.contains("lint, pack, install and scene: the game folder"), "{s}");
        let long = wrap("  --x", &"word ".repeat(60));
        assert!(long.lines().all(|l| l.len() <= WIDTH) && long.lines().count() > 2, "{long}");
        assert!(long.lines().skip(1).all(|l| l.starts_with(&" ".repeat(DESC_COL)) && l.len() > DESC_COL), "{long}");
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
