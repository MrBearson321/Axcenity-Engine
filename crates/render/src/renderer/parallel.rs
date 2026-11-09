//! Spreading a frame's planning over the CPU's threads.
//!
//! Which items a frame draws, in what order and with what data is worked
//! out from read-only data, so it can be shared out freely. Everything here
//! keeps the input's order, so a frame comes out the same however the work
//! was shared. Small jobs stay on the calling thread: waking the others
//! takes about a tenth of a millisecond each time, more than the work.

use rayon::prelude::*;

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
    items.par_chunks(PART).map(f).collect()
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
    items.par_iter().with_min_len(1).map(f).collect()
}

/// `f` of each index below `count`, in order, shared among the threads.
pub(super) fn map_indexes<R: Send>(count: usize, f: impl Fn(usize) -> R + Sync + Send) -> Vec<R> {
    if !worth_sharing(count) {
        return (0..count).map(f).collect();
    }
    (0..count)
        .into_par_iter()
        .with_min_len(PART)
        .map(f)
        .collect()
}

/// Sorts `items`, which are all different, so the result is the same
/// however the work is shared.
pub(super) fn sort_unstable<T: Ord + Send>(items: &mut [T]) {
    if !worth_sharing(items.len()) {
        items.sort_unstable();
    } else {
        items.par_sort_unstable();
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
        jobs.into_par_iter()
            .for_each(|(to, from)| to.copy_from_slice(from));
    }
}
