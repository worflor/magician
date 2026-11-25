//! One front door: `magician session ...`, `magician bundle ...` and `magician source ...` run the toolkit's Python
//! tools with the same arguments, the same stdin, stdout and stderr, and the same exit code, so a user or a script
//! names one program. The Python is not rewritten here: these verbs only find it and hand over.
//!
//! | Verb | Runs |
//! |---|---|
//! | `session` | `ingame\mod_session.py`: a mod's in-game session (`up`, `run`, `film`, `down`, `all`, `qa`) |
//! | `bundle` | `tools\bundle.py`: a player release bundle from a `bundle.json` |
//! | `source` | `tools\source.py`: a static Deadlock Mod Manager mod source from bundles |
//!
//! The toolkit folder and Python come from `paths::toolkit` and `paths::python`; when either is not there the verb says
//! what is missing and how to point at it, and exits 2. Everything after the verb is the tool's, `--help` included.

use crate::cli::Stop;
use std::path::PathBuf;
use std::process::ExitCode;

/// The pass-through verbs and the script each runs, relative to the toolkit folder.
pub const VERBS: &[(&str, &str)] = &[("session", "ingame/mod_session.py"), ("bundle", "tools/bundle.py"), ("source", "tools/source.py")];

pub fn script_of(verb: &str) -> Option<&'static str> {
    VERBS.iter().find(|(v, _)| *v == verb).map(|(_, s)| *s)
}

/// The command line a verb runs: the program, then its arguments (`-I`, the script, the caller's arguments).
pub fn command_line(verb: &str, rest: &[String]) -> Result<(PathBuf, Vec<String>), Stop> {
    let script = script_of(verb).ok_or_else(|| Stop::Usage(format!("{verb} is not a front-door verb")))?;
    let home = crate::paths::toolkit().map_err(|e| Stop::Usage(format!("`magician {verb}` runs {script}: {e}")))?;
    let path = home.path.join(crate::build::native(script));
    if !path.is_file() {
        return Err(Stop::Usage(format!("`magician {verb}` runs {script}, and {} has none ({}): point MAGICIAN_HOME at a full magician folder", home.path.display(), home.source)));
    }
    let (py, pre) = crate::paths::python().map_err(|e| Stop::Usage(format!("`magician {verb}` runs {script} with Python 3: {e}")))?;
    let mut args = pre;
    // isolated, as the toolkit's own docs run them: the user's site-packages and PYTHON* variables can't change it
    args.push("-I".into());
    args.push(path.to_string_lossy().into_owned());
    args.extend(rest.iter().cloned());
    Ok((py.path, args))
}

/// Run a verb: the tool's own stdio and exit code. A tool killed without one is exit 1.
pub fn run(verb: &str, rest: &[String]) -> Result<ExitCode, Stop> {
    let (prog, args) = command_line(verb, rest)?;
    let status = std::process::Command::new(&prog).args(&args).status().map_err(|e| Stop::Usage(format!("`magician {verb}`: {} did not start: {e}", prog.display())))?;
    Ok(ExitCode::from(status.code().map_or(1, |c| u8::try_from(c).unwrap_or(1))))
}
