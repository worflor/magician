//! A line on stderr that says what a slow step is doing, for a person watching a terminal: `magician: linting deep
//! pak10_dir.vpk (8 s)`. It appears only after a second and a half (a quick step shows nothing), counts the seconds,
//! and is wiped when the step ends. Never on a pipe or a file, so a log or a script sees nothing of it, and never
//! with `NO_COLOR`-style opt-outs: `MAGICIAN_NO_PROGRESS=1` turns it off too.

use std::io::IsTerminal;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Whether stderr is a terminal a person is watching (and progress is not turned off).
pub fn on_terminal() -> bool {
    std::io::stderr().is_terminal() && std::env::var_os("MAGICIAN_NO_PROGRESS").is_none_or(|v| v.is_empty() || v == "0") && std::env::var("TERM").map_or(true, |t| t != "dumb")
}

/// A running progress line; dropping it wipes the line.
pub struct Hint {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

/// Show `what` on stderr while the returned hint lives, when stderr is a terminal.
pub fn start(what: &str) -> Hint {
    let stop = Arc::new(AtomicBool::new(false));
    if !on_terminal() {
        return Hint { stop, thread: None };
    }
    let (flag, what) = (stop.clone(), what.to_string());
    let thread = std::thread::spawn(move || {
        let t0 = Instant::now();
        let mut shown = 0usize;
        while !flag.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(100));
            let s = t0.elapsed().as_secs_f64();
            if s >= 1.5 && !flag.load(Ordering::Relaxed) {
                let line = format!("\rmagician: {what} ({:.0} s)", s.floor());
                shown = shown.max(line.len());
                crate::out::stderr(&line);
            }
        }
        if shown > 0 {
            crate::out::stderr(&format!("\r{}\r", " ".repeat(shown)));
        }
    });
    Hint { stop, thread: Some(thread) }
}

impl Drop for Hint {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}
