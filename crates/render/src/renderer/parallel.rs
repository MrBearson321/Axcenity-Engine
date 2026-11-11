//! Spreading a frame's planning over the CPU's threads.
//!
//! Which items a frame draws, in what order and with what data is worked
//! out from read-only data, so it can be shared out freely. Everything here
//! keeps the input's order, so a frame comes out the same however the work
//! was shared. Small jobs stay on the calling thread: waking the others
//! takes about a tenth of a millisecond each time, more than the work.
//!
//! The work runs on the renderer's own pool of at most [`MAX_THREADS`]
//! threads, not rayon's global one. The global pool has a thread per
//! logical CPU (32 on a 16-core desktop; on Windows whatever the process's
//! affinity), and a frame's planning spread that thin spends more waking
//! and waiting than it saves (docs/rendering/performance.md, "The
//! renderer's own threads"). Cooking and terrain textures keep the global
//! pool, where more threads do help. `AXCENITY_RENDER_THREADS` sets the
//! number; 0 uses the global pool as before.

use std::sync::OnceLock;

use rayon::prelude::*;

/// The most threads a frame's planning is shared among unless
/// `AXCENITY_RENDER_THREADS` says.
pub const MAX_THREADS: usize = 12;

/// The renderer's pool, made the first time a job is shared. `None` runs
/// on rayon's global pool: asked for with `AXCENITY_RENDER_THREADS=0`, or
/// when the threads couldn't be started.
fn pool() -> Option<&'static rayon::ThreadPool> {
    static POOL: OnceLock<Option<rayon::ThreadPool>> = OnceLock::new();
    POOL.get_or_init(|| {
        let threads = thread_count(
            std::env::var("AXCENITY_RENDER_THREADS").ok().as_deref(),
            std::thread::available_parallelism().map_or(1, usize::from),
        )?;
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .thread_name(|index| format!("render {index}"))
            .build()
            .ok()
    })
    .as_ref()
}

/// How many threads the renderer's pool has: `asked` if it's a number (0
/// for none: the global pool), otherwise the `cpus`, at most
/// [`MAX_THREADS`].
fn thread_count(asked: Option<&str>, cpus: usize) -> Option<usize> {
    match asked.and_then(|value| value.trim().parse::<usize>().ok()) {
        Some(0) => None,
        Some(threads) => Some(threads),
        None => Some(cpus.clamp(1, MAX_THREADS)),
    }
}

/// Runs `job` on the renderer's pool (or the global one). Called from a
/// thread of some pool already (a caller's own, as the tests' one-thread
/// pool), the job stays in that pool.
fn shared<R: Send>(job: impl FnOnce() -> R + Send) -> R {
    match pool() {
        Some(pool) if rayon::current_thread_index().is_none() => pool.install(job),
        _ => job(),
    }
}

/// The most items one thread takes at a time.
const PART: usize = 512;

/// Below this many items, a job stays on the calling thread.
const WORTH_SHARING: usize = 4096;

/// Below this many bytes, a copy stays on the calling thread.
const WORTH_SHARING_BYTES: usize = 1 << 20;

/// Whether a job over `count` items is worth waking other threads for.
pub(super) fn worth_sharing(count: usize) -> bool {
    count > WORTH_SHARING
}

/// `f` of `items` cut into parts, in order, shared among the threads.
pub(super) fn map_parts<'a, T: Sync, R: Send>(
    items: &'a [T],
    f: impl Fn(&'a [T]) -> R + Sync + Send,
) -> Vec<R> {
    if !worth_sharing(items.len()) {
        return vec![f(items)];
    }
    shared(|| items.par_chunks(PART).map(f).collect())
}

/// `f` of each of `items`, in order, each its own job for the threads if
/// `share` (see [`worth_sharing`]): for items that are big jobs themselves.
pub(super) fn map_each<'a, T: Sync, R: Send>(
    items: &'a [T],
    share: bool,
    f: impl Fn(&'a T) -> R + Sync + Send,
) -> Vec<R> {
    if !share || items.len() <= 1 {
        return items.iter().map(f).collect();
    }
    shared(|| items.par_iter().with_min_len(1).map(f).collect())
}

/// `f` of each index below `count`, in order, shared among the threads.
pub(super) fn map_indexes<R: Send>(count: usize, f: impl Fn(usize) -> R + Sync + Send) -> Vec<R> {
    if !worth_sharing(count) {
        return (0..count).map(f).collect();
    }
    shared(|| {
        (0..count)
            .into_par_iter()
            .with_min_len(PART)
            .map(f)
            .collect()
    })
}

/// Sorts `items`, which are all different, so the result is the same
/// however the work is shared.
pub(super) fn sort_unstable<T: Ord + Send>(items: &mut [T]) {
    if !worth_sharing(items.len()) {
        items.sort_unstable();
    } else {
        shared(|| items.par_sort_unstable());
    }
}

/// Copies `pieces`, one after another, to the start of `buffer`, each piece
/// on a thread of its own when there's enough of them to be worth it.
pub(super) fn write_pieces<T: bytemuck::Pod + Sync>(
    queue: &wgpu::Queue,
    buffer: &wgpu::Buffer,
    pieces: &[&[T]],
) {
    let total: usize = pieces.iter().map(|piece| size_of_val(*piece)).sum();
    let Some(size) = wgpu::BufferSize::new(total as u64) else {
        return;
    };
    let Some(mut staging) = queue.write_buffer_with(buffer, 0, size) else {
        return;
    };
    let mut rest: &mut [u8] = &mut staging;
    let jobs: Vec<(&mut [u8], &[u8])> = pieces
        .iter()
        .map(|piece| {
            let bytes: &[u8] = bytemuck::cast_slice(piece);
            let (to, after) = std::mem::take(&mut rest).split_at_mut(bytes.len());
            rest = after;
            (to, bytes)
        })
        .collect();
    if jobs.len() == 1 || total < WORTH_SHARING_BYTES {
        jobs.into_iter()
            .for_each(|(to, from)| to.copy_from_slice(from));
    } else {
        shared(|| {
            jobs.into_par_iter()
                .for_each(|(to, from)| to.copy_from_slice(from));
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pool_has_at_most_twelve_threads_unless_asked() {
        assert_eq!(thread_count(None, 32), Some(MAX_THREADS));
        assert_eq!(thread_count(None, 4), Some(4));
        assert_eq!(thread_count(None, 0), Some(1));
        assert_eq!(thread_count(Some("20"), 32), Some(20));
        assert_eq!(thread_count(Some(" 0 "), 32), None, "0: the global pool");
        assert_eq!(thread_count(Some("lots"), 32), Some(MAX_THREADS));
    }
}
