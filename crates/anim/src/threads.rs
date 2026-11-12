//! The threads animators' poses are worked out on (unified Phase 93).
//!
//! A pose is worked out from the clips and the state machine alone, so
//! each animated thing's can be on its own thread; what reads or writes the
//! World (state machines, properties, ragdolls, foot placement, IK) stays
//! on the calling thread. Results keep the input's order, so a frame comes
//! out the same however the work was shared.
//!
//! Like the renderer's and physics' pools, posing has its own of at most
//! [`MAX_THREADS`] threads rather than rayon's global one (a thread per
//! logical CPU), which wakes more threads than a frame's posing can use.
//! `AXCENITY_ANIM_THREADS` sets the number; 1 keeps posing on the calling
//! thread. Small batches stay there anyway: waking the pool costs about a
//! tenth of a millisecond, more than a few poses.

use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use rayon::prelude::*;

/// The most threads posing uses unless `AXCENITY_ANIM_THREADS` says.
pub const MAX_THREADS: usize = 8;

/// Fewer things than this are posed on the calling thread.
const SHARED_FROM: usize = 8;

/// Where an [`Animators`](crate::Animators) works out its poses.
#[derive(Clone, Default)]
pub(crate) enum Threads {
    /// The pool every Animators shares, made the first time it's needed.
    #[default]
    Shared,
    /// Always the calling thread.
    One,
    /// A pool of its own (tests and benchmarks).
    Own(Arc<rayon::ThreadPool>),
}

/// How long each thread spent on the last shared batch, by its name
/// (`anim 0`, `anim 1`, …; `main` for the calling thread).
pub type ThreadTimes = Vec<(String, Duration)>;

impl Threads {
    /// A pool of `threads` threads, or the calling thread for 1 (or 0).
    pub(crate) fn of(threads: usize) -> Threads {
        if threads <= 1 {
            return Threads::One;
        }
        match build(threads) {
            Some(pool) => Threads::Own(Arc::new(pool)),
            None => Threads::One,
        }
    }

    fn pool(&self) -> Option<&rayon::ThreadPool> {
        match self {
            Threads::Shared => shared(),
            Threads::One => None,
            Threads::Own(pool) => Some(pool),
        }
    }

    /// `job` on each item, in their order, and how long each thread
    /// worked on them.
    pub(crate) fn map<T: Sync, R: Send>(
        &self,
        items: &[T],
        job: impl Fn(&T) -> R + Sync + Send,
    ) -> (Vec<R>, ThreadTimes) {
        let pool = self.pool().filter(|_| items.len() >= SHARED_FROM);
        let Some(pool) = pool else {
            let started = Instant::now();
            let results = items.iter().map(job).collect();
            let times = if items.is_empty() {
                Vec::new()
            } else {
                vec![("main".to_owned(), started.elapsed())]
            };
            return (results, times);
        };
        let threads = pool.current_num_threads();
        let timed: Vec<(R, usize, Duration)> = pool.install(|| {
            items
                .par_iter()
                .map(|item| {
                    let started = Instant::now();
                    let result = job(item);
                    let thread = rayon::current_thread_index().unwrap_or(threads);
                    (result, thread, started.elapsed())
                })
                .collect()
        });
        let mut busy = vec![Duration::ZERO; threads + 1];
        let results = timed
            .into_iter()
            .map(|(result, thread, time)| {
                busy[thread.min(threads)] += time;
                result
            })
            .collect();
        let times = busy
            .into_iter()
            .enumerate()
            .filter(|(_, time)| !time.is_zero())
            .map(|(thread, time)| {
                let name = if thread == threads {
                    "main".to_owned()
                } else {
                    format!("anim {thread}")
                };
                (name, time)
            })
            .collect();
        (results, times)
    }
}

fn shared() -> Option<&'static rayon::ThreadPool> {
    static POOL: OnceLock<Option<rayon::ThreadPool>> = OnceLock::new();
    POOL.get_or_init(|| {
        let threads = thread_count(
            std::env::var("AXCENITY_ANIM_THREADS").ok().as_deref(),
            std::thread::available_parallelism().map_or(1, usize::from),
        );
        (threads > 1).then(|| build(threads)).flatten()
    })
    .as_ref()
}

fn build(threads: usize) -> Option<rayon::ThreadPool> {
    rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .thread_name(|index| format!("anim {index}"))
        .build()
        .ok()
}

/// `asked` if it's a number of at least 1, otherwise the `cpus`, at most
/// [`MAX_THREADS`].
fn thread_count(asked: Option<&str>, cpus: usize) -> usize {
    asked
        .and_then(|value| value.trim().parse::<usize>().ok())
        .filter(|&threads| threads >= 1)
        .unwrap_or_else(|| cpus.clamp(1, MAX_THREADS))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_thread_count_follows_the_cpu_up_to_eight_unless_asked() {
        assert_eq!(thread_count(None, 4), 4);
        assert_eq!(thread_count(None, 32), MAX_THREADS);
        assert_eq!(thread_count(Some("1"), 32), 1);
        assert_eq!(thread_count(Some(" 16 "), 4), 16);
        assert_eq!(thread_count(Some("0"), 4), 4);
        assert_eq!(thread_count(Some("lots"), 2), 2);
    }

    #[test]
    fn a_shared_batch_keeps_its_order_and_says_which_threads_did_it() {
        let numbers: Vec<u64> = (0..100).collect();
        let (one, times) = Threads::One.map(&numbers, |n| n * n);
        assert_eq!(times.len(), 1);
        assert_eq!(times[0].0, "main");
        let (eight, times) = Threads::of(8).map(&numbers, |n| n * n);
        assert_eq!(one, eight);
        assert!(!times.is_empty());
        assert!(
            times
                .iter()
                .all(|(name, _)| name == "main" || name.starts_with("anim ")),
            "{times:?}"
        );
        // A few stay on the calling thread.
        let (_, times) = Threads::of(8).map(&numbers[..3], |n| n + 1);
        assert_eq!(times.len(), 1);
    }
}
