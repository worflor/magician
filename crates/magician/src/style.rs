//! Colour for a person at a terminal, and nowhere else: stdout piped into a file, a script or an agent gets the same
//! bytes as ever, and the words carry the meaning on their own (`ok`, `todo`, `FAIL`, `BROKEN`, `NSFR`), so nothing is
//! lost without it. `NO_COLOR` (any value) turns it off, `MAGICIAN_COLOR=always` on.
//!
//! In a grade, red means broken and nothing else: `NSFR` is pink (a property of the mod, not an alarm), `FP?`,
//! `stale(N)` and `unproven` are orange (something to do, nothing to fear), a fair-play pass is green. Pink and orange
//! need a 256-colour terminal; on a 16-colour one they fall back to magenta and yellow.

use std::io::IsTerminal;
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Good,
    Warn,
    Bad,
    Dim,
    Bold,
    /// A property, not an alarm (`NSFR`).
    Pink,
    /// Something to do, not to fear (`FP?`, `stale(N)`, `unproven`).
    Orange,
}

fn enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        if std::env::var("MAGICIAN_COLOR").is_ok_and(|v| v == "always") {
            return true;
        }
        std::env::var_os("NO_COLOR").is_none() && std::env::var("TERM").map_or(true, |t| t != "dumb") && std::io::stdout().is_terminal()
    })
}

/// Whether the terminal says it has 256 colours (or more): `COLORTERM`, a `TERM` that names them, Windows Terminal,
/// or a terminal program that announces itself. Anything else gets the 16-colour fallback.
fn wide(colorterm: Option<&str>, term: Option<&str>, wt_session: bool, term_program: bool) -> bool {
    colorterm.is_some_and(|c| c == "truecolor" || c == "24bit") || term.is_some_and(|t| t.contains("256color") || t.contains("direct")) || wt_session || term_program
}

fn wide_here() -> bool {
    static WIDE: OnceLock<bool> = OnceLock::new();
    *WIDE.get_or_init(|| {
        let var = |k: &str| std::env::var(k).ok();
        wide(var("COLORTERM").as_deref(), var("TERM").as_deref(), std::env::var_os("WT_SESSION").is_some(), std::env::var_os("TERM_PROGRAM").is_some())
    })
}

/// The SGR code for a tone, on a 256-colour terminal or a 16-colour one.
fn code(tone: Tone, wide: bool) -> &'static str {
    match (tone, wide) {
        (Tone::Good, _) => "32",
        (Tone::Warn, _) => "33",
        (Tone::Bad, _) => "31;1",
        (Tone::Dim, _) => "2",
        (Tone::Bold, _) => "1",
        (Tone::Pink, true) => "38;5;211",
        (Tone::Pink, false) => "35",
        (Tone::Orange, true) => "38;5;208",
        (Tone::Orange, false) => "33",
    }
}

/// `text` in `tone` when stdout is a terminal, else `text` unchanged.
pub fn paint(text: &str, tone: Tone) -> String {
    if !enabled() {
        return text.to_string();
    }
    format!("\x1b[{}m{text}\x1b[0m", code(tone, wide_here()))
}

/// The tone of a verdict word as Magician prints them.
pub fn tone_of(word: &str) -> Tone {
    let w = word.trim().to_ascii_lowercase();
    if w.starts_with("stale(") {
        return Tone::Orange;
    }
    match w.as_str() {
        "ok" | "clean" | "clear" | "pass" => Tone::Good,
        "todo" | "warn" | "re-prove" => Tone::Warn,
        "stale" | "unproven" | "fp?" => Tone::Orange,
        "nsfr" => Tone::Pink,
        "fail" | "broken" | "error" | "refused" | "blocked" => Tone::Bad,
        "skip" | "--" | "note" | "info" => Tone::Dim,
        _ => Tone::Bold,
    }
}

/// `word` padded to `width`, then painted by its own tone (the padding outside the colour, so columns line up).
pub fn word(word: &str, width: usize) -> String {
    let pad = width.saturating_sub(word.chars().count());
    format!("{}{}", paint(word, tone_of(word)), " ".repeat(pad))
}

/// A grade mark (`broken`, `NSFR`, `FP?`, `stale(N)`, `unproven`, `pass`) in its tone.
pub fn mark(m: &str) -> String {
    paint(m, tone_of(m))
}

/// A grade label with its marks in their tones; the description before them stays plain.
pub fn label(l: &str) -> String {
    grade::record::painted(l, &mark)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn red_is_only_broken_in_a_grade_and_every_mark_has_a_16_colour_fallback() {
        assert_eq!(tone_of("broken"), Tone::Bad);
        assert_eq!(tone_of("NSFR"), Tone::Pink);
        for m in ["FP?", "unproven", "stale(6864)"] {
            assert_eq!(tone_of(m), Tone::Orange, "{m}");
        }
        assert_eq!(tone_of("pass"), Tone::Good);
        for t in [Tone::Pink, Tone::Orange] {
            assert!(!code(t, false).contains(';'), "16 colours: a plain SGR code");
            assert!(code(t, true).starts_with("38;5;"));
            assert_ne!(code(t, false), code(Tone::Bad, false), "never red");
        }
    }

    #[test]
    fn which_terminals_have_256_colours() {
        assert!(wide(Some("truecolor"), None, false, false));
        assert!(wide(None, Some("xterm-256color"), false, false));
        assert!(wide(None, None, true, false), "Windows Terminal");
        assert!(!wide(None, Some("xterm"), false, false));
        assert!(!wide(None, None, false, false), "an old console: the fallback");
    }
}
