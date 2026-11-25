//! What each command takes, held to the code and to every real caller: the toolkit's own tools, Magician's calls of
//! itself, and every command line the docs, the skills and the CLI's own messages show. A flag one of them passes
//! that its command does not take fails here, so neither a new flag nor a new caller can reopen what `install
//! --dry-run` was: an option borrowed from another command, ignored while the command did its real work.

use super::*;
use std::collections::BTreeSet;

fn words(line: &str) -> Vec<String> {
    line.split_whitespace().map(String::from).collect()
}

/// The checks `main` runs before anything else, on one command line.
fn check(line: &str) -> Result<(), String> {
    let args = cli::command_first(&words(line), SHARED);
    cli::check_options(&args, &spec()).and_then(|()| cli::check_values(&args, SWITCHES, &USAGE))
}

/// Every call the toolkit's tools make of the binary, and the ones Magician makes of itself, as written there (a
/// placeholder for each value; an optional flag the caller adds on a condition is included).
const REAL_CALLS: &[(&str, &str)] = &[
    ("ingame/mod_session.py", "install --status"),
    ("ingame/mod_session.py", "install PAK --as pak10_dir.vpk --mod OWNER --holder HOLDER"),
    ("ingame/mod_session.py", "uninstall pak10_dir.vpk --holder HOLDER"),
    ("ingame/mod_session.py", "footprint HERO"),
    ("ingame/reprove.py", "patchday --capture --json"),
    ("ingame/reprove.py", "prove MOD.toml --evidence DIR"),
    ("cinema/session.py", "install PAK --as pak90_dir.vpk --mod cinema --holder HOLDER"),
    ("cinema/session.py", "install --status"),
    ("cinema/session.py", "uninstall pak90_dir.vpk --holder HOLDER"),
    ("cinema/cinema.ps1", "build cinema_dev OUT"),
    ("cinema/cinema.ps1", "install OUT --mod cinema"),
    ("cinema/cinema.ps1", "uninstall pak90_dir.vpk"),
    ("cinema/stage.py, tools/scene_lump.py, qa/detect", "dump VPK PATH"),
    ("cinema/acting.py, tools/scene_lump.py", "pack SRC OUT"),
    ("tools/bundle.py, qa", "ls VPK"),
    ("qa/detect", "ls VPK PREFIX"),
    ("qa/qa_profile.py", "refs VPK PATH"),
    ("tools/bundle.py", "scene check PATCH"),
    ("tools/publish.py", "grade P1 P2 --mod MOD --json"),
    ("tools/make-release.ps1", "update --describe"),
    ("mcp/server.py", "rules --json"),
    ("mcp/server.py", "lint VPK --json --deep --allow FILE --policy FILE"),
    ("mcp/server.py", "mesh VPK MODEL --no-png --against valve --with W1 --with W2 --json-out FILE"),
    ("mcp/server.py", "cloth VPK MODEL --against none --shapes"),
    ("mcp/server.py", "grade P1 P2 --json --hero HERO --mod MOD --light --target BUILD --allow F1 --allow F2 --policy FILE"),
    ("mcp/server.py", "release ADDON --hero HERO --no-wait --json --with W1 --with W2 --out OUT --mod MOD --no-compile"),
    ("mcp/server.py", "drift P1 P2 --from BUILD --json --to BUILD"),
    ("mcp/server.py", "patchday --json --capture --from BUILD --mod M1 --mod M2"),
    ("mcp/server.py", "check MOD --json --light --policy FILE"),
    ("mcp/server.py", "doctor --json --mod MOD"),
    ("mcp/server.py", "policy --policy FILE"),
    ("mcp/server.py", "build ADDON OUT --no-wait --no-compile"),
    ("mcp/server.py", "ls VPK PREFIX"),
    ("mcp/server.py", "dump VPK PATH --block TAG"),
    ("mcp/server.py", "verify VPK --key KEY --summary"),
    ("release, its lint (main.rs)", "lint OUT --deep --timings --findings-out F --json-out F --allow F --sources D1 --sources D2 --game G --baseline B --policy P"),
    ("release, its grade (main.rs)", "grade OUT --hero H --scene-root D1 --scene-root D2 --addon-root D --findings F --record-out F --mod M --game G --baseline B --policy P"),
    ("a mod's folder fills lint (modctx.rs)", "lint PAK --allow F1 --allow F2"),
    ("a mod's folder fills grade (modctx.rs)", "grade P1 P2 --mod MOD"),
    ("a mod's folder fills drift (modctx.rs)", "drift P1 --from BUILD"),
    ("a mod's folder fills release (modctx.rs)", "release ADDON --hero HERO --mod MOD --with W"),
    ("a mod's folder fills install and uninstall (modctx.rs)", "install PAK --mod OWNER"),
    ("a mod's folder fills install and uninstall (modctx.rs)", "uninstall --mod OWNER"),
    ("options shared by several commands before the command", "--no-cache --reference D --game G lint PAK --json"),
];

#[test]
fn every_call_the_toolkit_and_magician_make_is_accepted() {
    for (from, line) in REAL_CALLS {
        if let Err(e) = check(line) {
            panic!("{from}: `magician {line}` is refused: {e}");
        }
    }
}

#[test]
fn an_option_borrowed_from_another_command_is_refused_naming_the_ones_that_take_it() {
    for (line, said) in [
        ("install pak10_dir.vpk --dry-run", "(`publish`, `sign` and `update` take it)"),
        ("uninstall pak10_dir.vpk --dry-run", "(`publish`, `sign` and `update` take it)"),
        ("install pak10_dir.vpk --check", "(`update` takes it)"),
        ("release addon --hero lash --dry-run", "(`publish`, `sign` and `update` take it)"),
        ("prove mod.toml --dry-run", "(`publish`, `sign` and `update` take it)"),
        ("install --plan", "(`reprove` takes it)"),
        ("install pak10_dir.vpk --describe", "(`update` takes it)"),
        ("uninstall --all --status", "(`install` takes it)"),
        ("release addon --hero lash --force", "; did you mean --force-compile?)"),
        ("release addon --hero lash --bin win64", "(`build` takes it)"),
        ("rules --game G", "take it)"),
        ("--policy P ls VPK", "take it)"),
    ] {
        let e = check(line).expect_err(line);
        assert!(e.contains(" is not an option of `magician ") && e.contains(said) && e.contains("lists the ones it takes"), "{line}: {e}");
    }
    // the wording a command without a JSON form has always had
    assert_eq!(check("install PAK --json").unwrap_err(), format!("`magician install` has no --json form; it is available on {}", cli::JSON_COMMANDS.join(", ")));
}

#[test]
fn each_commands_help_lists_exactly_the_options_it_takes() {
    let s = spec();
    for c in s.commands().iter().filter(|c| !frontdoor::VERB_NAMES.contains(&c.as_str())) {
        let help = s.help(Some(c));
        let body = &help[..help.find("\nExit codes:").expect("help ends with the exit codes")];
        let mut takes = s.takes(c);
        for o in ["--help", "-h", "--version"] {
            takes.remove(o);
        }
        if !cli::JSON_COMMANDS.contains(&c.as_str()) {
            takes.remove("--json");
        }
        // a shared option a command's paragraphs name, it must be among the takers of: Shared is its one declaration
        assert_eq!(cli::options_named(body), takes, "`magician help {c}` against what it accepts");
    }
}

#[test]
fn the_declaration_names_only_commands_and_every_command_is_in_it() {
    let s = spec();
    let documented = s.commands();
    for c in COMMANDS.iter().filter(|c| **c != "help") {
        assert!(documented.iter().any(|d| d == c), "{c} is dispatched but has no paragraph in the usage text");
    }
    for c in &documented {
        assert!(COMMANDS.contains(&c.as_str()), "{c} is documented but not in COMMANDS");
    }
    let verbs: Vec<&str> = frontdoor::VERBS.iter().map(|(v, _)| *v).collect();
    assert_eq!(frontdoor::VERB_NAMES, verbs.as_slice());
    let native = |c: &str| documented.iter().any(|d| d == c) && !frontdoor::VERB_NAMES.contains(&c);
    for sh in SHARED {
        if let Takers::Only(list) = sh.takers {
            for c in list {
                assert!(native(c), "{}: {c} is not a command Magician runs itself", sh.name);
            }
        }
    }
    for (c, from) in INHERITS {
        assert!(native(c) && native(from), "{c} inherits {from}");
    }
    for c in cli::JSON_COMMANDS {
        assert!(native(c), "{c}");
    }
    // a switch takes no value whichever command it is on
    let switches: BTreeSet<&str> = SWITCHES.iter().copied().collect();
    for sh in SHARED {
        assert_eq!(sh.value.is_empty(), sh.name == "--no-cache" || switches.contains(sh.name), "{}", sh.name);
    }
}

/// `"--word"` literals in Magician's sources that are not its options: arguments it passes to curl, git and cargo.
const NOT_OPTIONS: &[&str] = &[
    "--connect-timeout", "--fail", "--location", "--max-redirs", "--max-time", "--noproxy", "--output", "--proto", "--proto-redir", "--show-error",
    "--silent", "--speed-limit", "--speed-time", "--user-agent", "--abbrev-ref", "--count", "--ff-only", "--hard", "--left-right", "--porcelain",
    "--quiet", "--release", "--short", "--symbolic-full-name",
];

#[test]
fn every_option_the_code_reads_is_one_some_command_takes() {
    let all = spec().all();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut stray = BTreeSet::new();
    for e in std::fs::read_dir(&dir).unwrap() {
        let p = e.unwrap().path();
        let name = p.file_name().unwrap().to_string_lossy().into_owned();
        if !name.ends_with(".rs") || name == "fuzz_tests.rs" || name == "option_tests.rs" {
            continue;
        }
        let text = std::fs::read_to_string(&p).unwrap();
        // the code, not its tests (each file's test module comes last)
        let code = text.split("\n#[cfg(test)]").next().unwrap_or("");
        for (i, _) in code.match_indices("\"--") {
            let word: String = code[i + 1..].chars().take_while(|c| *c == '-' || c.is_ascii_lowercase() || c.is_ascii_digit()).collect();
            if code[i + 1 + word.len()..].starts_with('"') && word.len() > 2 && !all.contains(&word) && !NOT_OPTIONS.contains(&word.as_str()) {
                stray.insert(format!("{word} ({name})"));
            }
        }
    }
    assert!(stray.is_empty(), "options the code reads that no command takes (declare each in its command's paragraphs of the usage text, or in SHARED): {stray:?}");
}

/// The text after each `magician ` shown as a command (in a code span or a fenced block; for a source file, a
/// code span in a string or a comment), up to where that command line ends.
fn shown_calls(text: &str, fences: bool) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut fenced = false;
    for (n, line) in text.lines().enumerate() {
        if fences && line.trim_start().starts_with("```") {
            fenced = !fenced;
            continue;
        }
        let line = line.replace("magician.exe ", "magician ");
        let pieces: Vec<&str> = if fenced { vec![line.as_str()] } else { line.split('`').skip(1).step_by(2).collect() };
        for piece in pieces {
            let mut at = 0;
            while let Some(i) = piece[at..].find("magician ") {
                let start = at + i;
                let before = piece[..start].chars().last();
                at = start + "magician ".len();
                if before.is_some_and(|c| c.is_alphanumeric() || "_-/\\.".contains(c)) {
                    continue;
                }
                out.push((n + 1, cut_call(&piece[at..])));
            }
        }
    }
    out
}

/// A command line up to its end: a pipe or `||` outside brackets, `;`, `&&`, a comment.
fn cut_call(s: &str) -> String {
    let mut depth = 0i32;
    let b: Vec<char> = s.chars().collect();
    let mut end = b.len();
    for (i, c) in b.iter().enumerate() {
        match c {
            '[' => depth += 1,
            ']' => depth -= 1,
            '|' if depth <= 0 => {
                end = i;
                break;
            }
            ';' => {
                end = i;
                break;
            }
            '&' if b.get(i + 1) == Some(&'&') => {
                end = i;
                break;
            }
            '#' if i > 0 && b[i - 1] == ' ' => {
                end = i;
                break;
            }
            _ => {}
        }
    }
    b[..end].iter().collect()
}

#[test]
fn every_command_line_the_docs_show_passes_only_options_its_command_takes() {
    let s = spec();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
    let mut docs: Vec<PathBuf> = ["README.md", "CONTRIBUTING.md", "ingame/README.md", "mcp/README.md", "cinema/README.md"].iter().map(|f| root.join(f)).collect();
    fn walk(d: &Path, ext: &str, out: &mut Vec<PathBuf>) {
        for e in std::fs::read_dir(d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, ext, out);
            } else if p.extension().is_some_and(|x| x == ext) {
                out.push(p);
            }
        }
    }
    walk(&root.join("docs"), "md", &mut docs);
    walk(&root.join("skills"), "md", &mut docs);
    let mut sources = Vec::new();
    walk(&root.join("crates").join("magician").join("src"), "rs", &mut sources);
    // the usage text's every line is a command line or prose about one, as a fenced block's are
    let mut texts: Vec<(String, String, bool)> = vec![("the usage text".into(), format!("```\n{}\n```", *USAGE), true)];
    for p in docs {
        texts.push((p.strip_prefix(&root).unwrap_or(&p).display().to_string(), std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display())), true));
    }
    for p in sources.iter().filter(|p| !p.ends_with("option_tests.rs")) {
        texts.push((p.strip_prefix(&root).unwrap_or(p).display().to_string(), std::fs::read_to_string(p).unwrap(), false));
    }
    let commands = s.commands();
    let (mut wrong, mut seen) = (Vec::new(), 0);
    for (name, text, fences) in &texts {
        for (line, call) in shown_calls(text, *fences) {
            let cmd: String = call.chars().take_while(|c| c.is_ascii_lowercase() || *c == '-').collect();
            if !commands.contains(&cmd) || frontdoor::VERB_NAMES.contains(&cmd.as_str()) {
                continue;
            }
            seen += 1;
            let takes = s.takes(&cmd);
            for o in cli::options_named(&call) {
                let ok = if o == "--json" { cli::JSON_COMMANDS.contains(&cmd.as_str()) } else { takes.contains(&o) };
                if !ok {
                    wrong.push(format!("{name}:{line}: `magician {}` shows {o}, which {cmd} does not take", call.trim()));
                }
            }
        }
    }
    assert!(seen > 100, "only {seen} command lines found in the docs: is the walk still reading them?");
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
