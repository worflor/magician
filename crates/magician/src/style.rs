//! Colour for a person at a terminal, and nowhere else: stdout piped into a file, a script or an agent gets the same
//! bytes as ever, and the words carry the meaning on their own (`ok`, `todo`, `FAIL`, `BROKEN`), so nothing is lost
//! without it. `NO_COLOR` (any value) turns it off, `MAGICIAN_COLOR=always` on.

use std::io::IsTerminal;
use std::sync::OnceLock;

#[derive(Clone, Copy)]
pub enum Tone {
    Good,
    Warn,
    Bad,
    Dim,
    Bold,
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

/// `text` in `tone` when stdout is a terminal, else `text` unchanged.
pub fn paint(text: &str, tone: Tone) -> String {
    if !enabled() {
        return text.to_string();
    }
    let code = match tone {
        Tone::Good => "32",
        Tone::Warn => "33",
        Tone::Bad => "31;1",
        Tone::Dim => "2",
        Tone::Bold => "1",
    };
    format!("\x1b[{code}m{text}\x1b[0m")
}

/// The tone of a verdict word as Magician prints them.
pub fn tone_of(word: &str) -> Tone {
    match word.trim().to_ascii_lowercase().as_str() {
        "ok" | "clean" | "clear" | "pass" => Tone::Good,
        "todo" | "warn" | "re-prove" | "stale" | "unproven" => Tone::Warn,
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
