//! Scoped-thread fan-out for the rule families: the same results as a sequential loop, in the same order.
//!
//! One budget of threads for the whole process: `map` and `join` take workers from it and run on the caller's own
//! thread when it is spent, so nested fan-outs (models side by side, each model's census groups, each group's
//! own maps) never add up to more than the cap. `MAGICIAN_THREADS=N` sets the cap (1 is fully sequential), for a
//! lint that must share the machine with a running game.

use std::sync::atomic::{AtomicUsize, Ordering};

/// The cap from `MAGICIAN_THREADS`, when it names one.
fn asked(value: Option<&str>) -> Option<usize> {
    value.and_then(|v| v.trim().parse::<usize>().ok()).filter(|n| *n >= 1)
}

/// Threads to use: `MAGICIAN_THREADS`, else the machine's, capped (the mesh phases are memory-bound past about 8).
pub fn threads() -> usize {
    static CAP: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *CAP.get_or_init(|| {
        let machine = std::thread::available_parallelism().map_or(1, |n| n.get()).min(12);
        asked(std::env::var("MAGICIAN_THREADS").ok().as_deref()).map_or(machine, |n| n.min(64))
    })
}

/// Threads at work beyond the one that entered: a worker is taken from the budget before it is spawned.
static EXTRA: AtomicUsize = AtomicUsize::new(0);

struct Worker;

impl Worker {
    fn take() -> Option<Worker> {
        let cap = threads().saturating_sub(1);
        EXTRA.fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| (n < cap).then_some(n + 1)).ok().map(|_| Worker)
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        EXTRA.fetch_sub(1, Ordering::AcqRel);
    }
}

/// `items.iter().map(f).collect()`, with the items spread over threads. Order is preserved.
pub fn map<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    if items.len() <= 1 {
        return items.iter().map(f).collect();
    }
    let next = AtomicUsize::new(0);
    let work = || {
        let mut mine = Vec::new();
        loop {
            let i = next.fetch_add(1, Ordering::Relaxed);
            if i >= items.len() {
                break;
            }
            mine.push((i, f(&items[i])));
        }
        mine
    };
    let mut parts: Vec<Vec<(usize, R)>> = Vec::new();
    std::thread::scope(|s| {
        let mut handles = Vec::new();
        while handles.len() + 1 < items.len() {
            let Some(w) = Worker::take() else { break };
            let work = &work;
            handles.push(s.spawn(move || {
                let _held = w;
                work()
            }));
        }
        parts.push(work());
        for h in handles {
            match h.join() {
                Ok(p) => parts.push(p),
                Err(e) => std::panic::resume_unwind(e),
            }
        }
    });
    let mut all: Vec<(usize, R)> = parts.into_iter().flatten().collect();
    all.sort_by_key(|(i, _)| *i);
    all.into_iter().map(|(_, r)| r).collect()
}

/// Run two closures at once (one on a new thread, when the budget has one) and return both results.
pub fn join<A: Send, B: Send>(a: impl FnOnce() -> A + Send, b: impl FnOnce() -> B + Send) -> (A, B) {
    let Some(w) = Worker::take() else {
        let x = a();
        return (x, b());
    };
    std::thread::scope(|s| {
        let hb = s.spawn(move || {
            let _held = w;
            b()
        });
        let x = a();
        match hb.join() {
            Ok(y) => (x, y),
            Err(e) => std::panic::resume_unwind(e),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_keeps_order_and_covers_every_item() {
        let items: Vec<u32> = (0..1000).collect();
        // uneven work, so threads finish out of order
        let out = map(&items, |&i| (0..(i % 7) * 50).fold(i as u64, |a, b| a.wrapping_add(b as u64)));
        let want: Vec<u64> = items.iter().map(|&i| (0..(i % 7) * 50).fold(i as u64, |a, b| a.wrapping_add(b as u64))).collect();
        assert_eq!(out, want);
        assert!(map::<u32, u32>(&[], |&i| i).is_empty());
        assert_eq!(map(&[5u32], |&i| i + 1), vec![6]);
    }

    #[test]
    fn the_thread_setting_is_a_cap_and_nonsense_is_ignored() {
        assert_eq!(asked(Some("2")), Some(2));
        assert_eq!(asked(Some(" 1 ")), Some(1));
        assert_eq!((asked(Some("0")), asked(Some("many")), asked(Some("-3")), asked(None)), (None, None, None, None));
        assert!(threads() >= 1);
    }

    #[test]
    fn nested_fan_outs_stay_inside_the_budget() {
        // maps inside maps inside joins: never more threads at work than the cap, and every worker handed back
        let live = AtomicUsize::new(0);
        let peak = AtomicUsize::new(0);
        let busy = || {
            let n = live.fetch_add(1, Ordering::SeqCst) + 1;
            peak.fetch_max(n, Ordering::SeqCst);
            std::thread::sleep(std::time::Duration::from_millis(2));
            live.fetch_sub(1, Ordering::SeqCst);
        };
        let outer: Vec<u32> = (0..6).collect();
        let (a, _) = join(
            || map(&outer, |&i| map(&outer, |&j| { busy(); i * 10 + j }).iter().sum::<u32>()),
            || map(&outer, |_| busy()),
        );
        assert_eq!(a, vec![15, 75, 135, 195, 255, 315]);
        // other tests of this process share the budget, so the bound is the cap itself
        assert!(peak.load(Ordering::SeqCst) <= threads(), "{} at work, cap {}", peak.load(Ordering::SeqCst), threads());
    }

    #[test]
    fn join_returns_both_results() {
        let (a, b) = join(|| (1..=10u32).sum::<u32>(), || "second".to_string());
        assert_eq!((a, b.as_str()), (55, "second"));
    }

    #[test]
    #[should_panic(expected = "boom")]
    fn a_panic_on_a_worker_reaches_the_caller() {
        let items = [1, 2, 3, 4];
        let _ = map(&items, |&i| if i == 3 { panic!("boom") } else { i });
    }
}
