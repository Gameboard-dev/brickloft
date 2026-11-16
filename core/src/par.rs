//! Work shared out over the processor's cores, with the results in the
//! order one core would have made them.
//!
//! A build is many pieces of work that do not depend on one another: the
//! layers of cells a measurement looks at, the rows of an outline's samples,
//! the stones of a path. [`Threads::map_ranges`] hands such pieces to several
//! threads and gives their results back in the order of the pieces, so what
//! is built from them is the same, bit for bit, on one thread or sixteen.
//! Nothing here lets the order threads happen to finish in reach a result.
//!
//! The rules for whoever calls it:
//!
//! - the work for one item must not depend on the work for another;
//! - whole numbers may be added up across pieces in any grouping, but sums
//!   of `f64` are not the same in every grouping, so those are added up by
//!   the caller, item by item, in order;
//! - work that does depend on what came before (a stone that must keep clear
//!   of the stones already laid) stays on one thread, after the part of it
//!   that does not has been done here.
//!
//! The threads are the standard library's scoped threads, started for one
//! call and joined before it returns:
//! <https://doc.rust-lang.org/std/thread/fn.scope.html>
//! A pool of threads kept between builds would save their starting, which
//! was measured at 50 to 100 microseconds a thread on a Windows desktop
//! with 16 logical cores (see `spawn_cost` in the tests below), three
//! quarters of a millisecond for all sixteen. The builds worth sharing out
//! take tens to hundreds of milliseconds, so a pool would save a few
//! hundredths of them, and would be state kept alive between builds, with
//! its own shutting down and its own behaviour when a piece of work panics.
//! What the starting costs is kept small the other way: work is shared out
//! only where each thread's part of it is several times longer than the
//! thread takes to start (see `least` in [`Threads::map_ranges`]). The `rayon` crate does all of
//! this well and was not taken either: the fitting crate depends on one
//! vector-maths crate and compiles in seconds, and what is needed of rayon
//! here is the hundred lines below.
//!
//! The browser build has no threads (`wasm32-unknown-unknown` cannot start
//! one), so there every call does its work in place on the one thread.

use std::ops::Range;

/// How many threads a build may use, decided once where the build starts
/// and handed down to whatever shares work out.
///
/// It is a value passed down, not a setting read where the work is done,
/// for two reasons. The number of cores is asked of the system once a build
/// and not once a call (on Linux the question reads files). And a test can
/// build the same form with one thread and with several and compare the two.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Threads {
    count: usize,
    /// share out however little work there is (see [`Threads::forced`])
    always: bool,
}

impl Threads {
    /// One thread: everything is done in place, in order.
    pub const ONE: Threads = Threads { count: 1, always: false };

    /// As many threads as the machine has cores to run them on; one where
    /// that cannot be found out, and in the browser.
    /// <https://doc.rust-lang.org/std/thread/fn.available_parallelism.html>
    pub fn available() -> Threads {
        #[cfg(not(target_arch = "wasm32"))]
        let count = std::thread::available_parallelism().map_or(1, |n| n.get());
        #[cfg(target_arch = "wasm32")]
        let count = 1;
        Threads { count, always: false }
    }

    /// At most `count` threads, used only where there is enough work for
    /// them to pay for their starting.
    pub fn at_most(count: usize) -> Threads {
        Threads { count: count.max(1), always: false }
    }

    /// `count` threads used for any work at all, however small: for tests,
    /// which compare what several threads build with what one does, on
    /// forms too small to be worth sharing out otherwise.
    pub fn forced(count: usize) -> Threads {
        Threads { count: count.max(1), always: true }
    }

    /// How many threads `len` items are worth, if `least` of them are the
    /// fewest worth a thread of their own.
    fn worth(self, len: usize, least: usize) -> usize {
        if cfg!(target_arch = "wasm32") {
            return 1;
        }
        let most = if self.always { len } else { len / least.max(1) };
        self.count.min(most).max(1)
    }

    /// `work` done on each of several ranges that together are `0..len`,
    /// with the results in the order of the ranges.
    ///
    /// `least` is the fewest items worth giving a thread of their own: the
    /// number whose work takes about half a millisecond, five to ten times
    /// the 50 to 100 microseconds starting a thread was measured to cost.
    /// Each place that calls this says how long one of its items was
    /// measured to take. With fewer than twice `least`, `work` is called
    /// once, on this thread, for the whole range; with more, there is a
    /// thread for every `least` items, up to the number of cores.
    ///
    /// Where the ranges fall depends on the number of threads, so `work`
    /// must give results that do not depend on that once they are put end
    /// to end (see the rules at the top of this file).
    ///
    /// The items are cut into more ranges than there are threads, four to
    /// a thread, and each thread takes the next range not yet taken. Cut
    /// into one range a thread, a form with most of its surface at one end
    /// of the range of items kept one thread busy while the rest had
    /// finished. Which thread does which range is then left to chance, and
    /// does not matter: each result is put back in its range's place.
    pub fn map_ranges<R: Send>(self, len: usize, least: usize, work: impl Fn(Range<usize>) -> R + Sync) -> Vec<R> {
        let threads = self.worth(len, least);
        if threads <= 1 {
            return vec![work(0..len)];
        }
        self.share(len, threads, &work)
    }

    #[cfg(target_arch = "wasm32")]
    fn share<R: Send>(self, len: usize, _threads: usize, work: &(impl Fn(Range<usize>) -> R + Sync)) -> Vec<R> {
        vec![work(0..len)]
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn share<R: Send>(self, len: usize, threads: usize, work: &(impl Fn(Range<usize>) -> R + Sync)) -> Vec<R> {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let pieces = (threads * 4).min(len);
        // piece `k` of `pieces`: as near equal as whole numbers allow
        let range = |k: usize| k * len / pieces..(k + 1) * len / pieces;
        // The next piece nobody has taken: one number, counted up. A queue
        // of pieces behind a lock would do the same with more to go wrong.
        // https://doc.rust-lang.org/std/sync/atomic/struct.AtomicUsize.html#method.fetch_add
        let next = AtomicUsize::new(0);
        let take = || {
            let mut done: Vec<(usize, R)> = Vec::new();
            loop {
                // (`Relaxed`: only the count itself is shared, and each number is handed out once whatever the ordering)
                let k = next.fetch_add(1, Ordering::Relaxed);
                if k >= pieces {
                    return done;
                }
                done.push((k, work(range(k))));
            }
        };
        let mut results: Vec<Option<R>> = (0..pieces).map(|_| None).collect();
        std::thread::scope(|scope| {
            // this thread works too, so one fewer is started
            let others: Vec<_> = (1..threads).map(|_| scope.spawn(take)).collect();
            let mine = take();
            let theirs = others.into_iter().flat_map(|thread| match thread.join() {
                Ok(done) => done,
                // a panic in the work is the caller's panic, as it would have been on one thread
                Err(panic) => std::panic::resume_unwind(panic),
            });
            for (k, result) in mine.into_iter().chain(theirs) {
                results[k] = Some(result);
            }
        });
        results.into_iter().map(|r| r.expect("every piece is taken once")).collect()
    }

    /// `work` done for each of `0..len`, with the results in that order.
    /// `least` is as for [`Threads::map_ranges`].
    pub fn map<T: Send>(self, len: usize, least: usize, work: impl Fn(usize) -> T + Sync) -> Vec<T> {
        let mut parts = self.map_ranges(len, least, |range| range.map(&work).collect::<Vec<T>>());
        if parts.len() == 1 {
            return parts.pop().unwrap_or_default();
        }
        let mut all = Vec::with_capacity(len);
        for part in parts {
            all.extend(part);
        }
        all
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn results_are_in_order_whatever_the_number_of_threads() {
        let serial: Vec<u64> = (0..1000u64).map(|i| i * i % 37).collect();
        for count in [1, 2, 3, 7, 16, 64] {
            assert_eq!(Threads::forced(count).map(1000, 1, |i| (i * i % 37) as u64), serial, "{count} threads");
            let ranges = Threads::forced(count).map_ranges(1000, 1, |r| r);
            assert_eq!(ranges.first().map(|r| r.start), Some(0));
            assert_eq!(ranges.last().map(|r| r.end), Some(1000));
            assert!(ranges.windows(2).all(|pair| pair[0].end == pair[1].start), "{count} threads: the ranges do not follow on: {ranges:?}");
        }
    }

    #[test]
    fn results_are_in_order_when_no_thread_does_two_items_running() {
        // The test above passes with the results left in the order the
        // threads hand them back, if one thread happens to do all the work
        // before another has started, as it does for work this small. Here
        // an item is not finished until the item after it has been started,
        // which only another thread can do: so no thread does two items
        // running, and the order the threads hand results back in is never
        // the order of the items.
        use std::sync::atomic::{AtomicUsize, Ordering};
        for count in [2, 3, 8] {
            // (few enough items that each is a range of its own)
            let len = count * 3;
            let started = AtomicUsize::new(0);
            let found = Threads::forced(count).map(len, 1, |i| {
                started.fetch_max(i, Ordering::SeqCst);
                while i + 1 < len && started.load(Ordering::SeqCst) <= i {
                    std::hint::spin_loop();
                }
                i
            });
            assert_eq!(found, (0..len).collect::<Vec<_>>(), "{count} threads");
        }
    }

    #[test]
    fn little_work_is_not_shared_out() {
        // under twice the least worth a thread: one call, on this thread
        let here = std::thread::current().id();
        let ranges = Threads::at_most(8).map_ranges(199, 100, |r| (r, std::thread::current().id()));
        assert_eq!(ranges, vec![(0..199, here)]);
        assert!(Threads::at_most(8).map_ranges(800, 100, |r| r).len() > 1);
        assert_eq!(Threads::ONE.map_ranges(800, 1, |r| r), vec![0..800]);
        // nothing at all is still one (empty) range, and no items
        assert_eq!(Threads::forced(4).map_ranges(0, 1, |r| r), vec![0..0]);
        assert_eq!(Threads::forced(4).map(0, 1, |i| i), Vec::<usize>::new());
    }

    /// What starting threads costs, for the thresholds: `cargo test --release
    /// -p brickloft-core spawn_cost -- --ignored --nocapture`.
    #[test]
    #[ignore = "a measurement, not a check"]
    fn spawn_cost() {
        for count in [2, 4, 8, 16] {
            let least = (0..200)
                .map(|_| {
                    let start = std::time::Instant::now();
                    std::hint::black_box(Threads::forced(count).map_ranges(count * 4, 1, |r| r.len()));
                    start.elapsed().as_secs_f64() * 1e6
                })
                .fold(f64::INFINITY, f64::min);
            println!("{count} threads: {least:.0} microseconds to start and join, {:.0} a thread", least / (count - 1) as f64);
        }
    }
}
