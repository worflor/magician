//! Everything the CLI writes to stdout and stderr goes through here, so a reader that stops reading never crashes it.
//!
//! `magician lint x.vpk | head -3` closes the pipe after three lines. Rust's `println!` panics on the next write
//! (exit 101, and a panic message on stderr), which a script reads as a crash. Here a closed stream is noted once and
//! every later write to it is dropped: the command still runs to its end and exits with the code its result has, so
//! `set -o pipefail` sees a lint that failed, not a broken pipe.

use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};

static STDOUT_CLOSED: AtomicBool = AtomicBool::new(false);
static STDERR_CLOSED: AtomicBool = AtomicBool::new(false);

/// Write `text` to `w` unless the stream was found closed; a write that fails closes it for the rest of the run.
fn put(w: &mut dyn Write, closed: &AtomicBool, text: &str) {
    if closed.load(Ordering::Relaxed) {
        return;
    }
    if w.write_all(text.as_bytes()).and_then(|()| w.flush()).is_err() {
        closed.store(true, Ordering::Relaxed);
    }
}

/// `text` on stdout as it is.
pub fn stdout(text: &str) {
    put(&mut std::io::stdout().lock(), &STDOUT_CLOSED, text);
}

/// `text` on stderr as it is.
pub fn stderr(text: &str) {
    put(&mut std::io::stderr().lock(), &STDERR_CLOSED, text);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stream whose reader has gone: every write fails as a closed pipe does.
    struct Gone(usize);
    impl Write for Gone {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            self.0 += 1;
            Err(std::io::Error::from(std::io::ErrorKind::BrokenPipe))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn a_closed_stream_is_written_to_once_and_then_left_alone() {
        let closed = AtomicBool::new(false);
        let mut w = Gone(0);
        put(&mut w, &closed, "first\n");
        assert!(closed.load(Ordering::Relaxed));
        put(&mut w, &closed, "second\n");
        put(&mut w, &closed, "third\n");
        assert_eq!(w.0, 1, "nothing is tried after the pipe closed");
        let open = AtomicBool::new(false);
        let mut v: Vec<u8> = Vec::new();
        put(&mut v, &open, "a\n");
        put(&mut v, &open, "b\n");
        assert_eq!((v.as_slice(), open.load(Ordering::Relaxed)), (&b"a\nb\n"[..], false));
    }
}
