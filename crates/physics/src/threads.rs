//! The threads rapier's step runs on, with the `parallel` feature.
//!
//! rapier spreads its step over rayon's threads. rayon's global pool has a
//! thread per logical CPU (on Windows, whatever the process's affinity),
//! which is too many for one physics step: on a 16-core, 32-thread desktop,
//! S10k's Physics step took 2.6–2.8 ms on the global pool and 1.9–2.0 ms on
//! 8 or 12 threads (docs/benchmarks.md, "Physics step, part 2"). So physics
//! has its own pool, of at most [`MAX_THREADS`], and the rest of the engine
//! keeps the global one.
//! `AXCENITY_PHYSICS_THREADS` sets the number instead. Results are the same
//! with any number of threads (`a_pile_lands_the_same_way_on_any_number_of_threads`).

use std::sync::{Arc, OnceLock};

/// The most threads physics uses unless `AXCENITY_PHYSICS_THREADS` says.
pub const MAX_THREADS: usize = 12;

/// A pool for rapier's step.
pub(crate) type Pool = Arc<rayon::ThreadPool>;

/// The pool every [`Physics`](crate::Physics) shares, made the first time
/// one asks. `None` if its threads couldn't be started: rapier then runs
/// on the global pool.
pub(crate) fn shared() -> Option<Pool> {
    static POOL: OnceLock<Option<Pool>> = OnceLock::new();
    POOL.get_or_init(|| make(thread_count())).clone()
}

/// A pool of `threads` threads.
pub(crate) fn make(threads: usize) -> Option<Pool> {
    rayon::ThreadPoolBuilder::new()
        .num_threads(threads.max(1))
        .thread_name(|index| format!("physics {index}"))
        .build()
        .ok()
        .map(Arc::new)
}

/// `AXCENITY_PHYSICS_THREADS` if it's a number of at least 1, otherwise
/// the logical CPUs, at most [`MAX_THREADS`].
fn thread_count() -> usize {
    let asked = std::env::var("AXCENITY_PHYSICS_THREADS")
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
        .filter(|&threads| threads >= 1);
    asked.unwrap_or_else(|| {
        std::thread::available_parallelism()
            .map_or(1, usize::from)
            .min(MAX_THREADS)
    })
}
