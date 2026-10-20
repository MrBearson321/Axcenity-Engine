//! Slow terrain work (generating, eroding) on a thread of its own, so the
//! editor keeps drawing. The work reports how far it got and stops early
//! when asked.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::thread::JoinHandle;

/// What the work sees: whether it should stop, and where to say how far it got.
#[derive(Clone, Debug, Default)]
pub struct JobControl {
    cancelled: Arc<AtomicBool>,
    /// From 0 to 1, as the bits of an `f32`.
    progress: Arc<AtomicU32>,
}

impl JobControl {
    /// A control nobody cancels, for running work right here (tests, small terrains).
    pub fn new() -> Self {
        Self::default()
    }

    /// True once the job was cancelled: the work should stop and return `None`.
    pub fn cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    /// Says how far the work is, from 0 to 1.
    pub fn set_progress(&self, done: f32) {
        self.progress
            .store(done.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    pub fn progress(&self) -> f32 {
        f32::from_bits(self.progress.load(Ordering::Relaxed))
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

/// Work running on another thread, which gives a `T` when it finishes
/// (`None` if it was cancelled).
pub struct Job<T> {
    control: JobControl,
    handle: Option<JoinHandle<Option<T>>>,
}

impl<T: Send + 'static> Job<T> {
    /// Starts `work` on a new thread.
    pub fn spawn(work: impl FnOnce(&JobControl) -> Option<T> + Send + 'static) -> Self {
        let control = JobControl::new();
        let theirs = control.clone();
        let handle = std::thread::Builder::new()
            .name("terrain job".to_owned())
            .spawn(move || work(&theirs))
            .ok();
        Self { control, handle }
    }

    /// From 0 to 1.
    pub fn progress(&self) -> f32 {
        self.control.progress()
    }

    /// Asks the work to stop. It gives `None` when it does.
    pub fn cancel(&self) {
        self.control.cancel();
    }

    pub fn is_finished(&self) -> bool {
        self.handle.as_ref().is_none_or(JoinHandle::is_finished)
    }

    /// The result once the work is finished: `Some(None)` if it was
    /// cancelled, failed to start or panicked. `None` while it still runs.
    pub fn try_take(&mut self) -> Option<Option<T>> {
        if !self.is_finished() {
            return None;
        }
        let result = self.handle.take()?.join().ok().flatten();
        Some(result)
    }
}

/// Runs `rows` rows of work on every core, each row by `row`, and returns
/// them in order, or `None` if cancelled. Progress goes from `from` to `to`.
pub fn parallel_rows<R: Send>(
    rows: usize,
    control: &JobControl,
    (from, to): (f32, f32),
    row: impl Fn(usize) -> R + Sync,
) -> Option<Vec<R>> {
    let threads = std::thread::available_parallelism()
        .map_or(1, usize::from)
        .clamp(1, 16)
        .min(rows.max(1));
    let done = AtomicU32::new(0);
    let mut parts: Vec<Option<Vec<(usize, R)>>> = Vec::new();
    std::thread::scope(|scope| {
        let handles: Vec<_> = (0..threads)
            .map(|thread| {
                let (row, done) = (&row, &done);
                scope.spawn(move || {
                    let mut out = Vec::new();
                    // Every `threads`th row, so each thread gets a fair share
                    // of slow and fast rows.
                    for index in (thread..rows).step_by(threads) {
                        if control.cancelled() {
                            return None;
                        }
                        out.push((index, row(index)));
                        let finished = done.fetch_add(1, Ordering::Relaxed) + 1;
                        if thread == 0 {
                            control
                                .set_progress(from + (to - from) * finished as f32 / rows as f32);
                        }
                    }
                    Some(out)
                })
            })
            .collect();
        parts = handles
            .into_iter()
            .map(|handle| handle.join().ok().flatten())
            .collect();
    });
    let mut all: Vec<(usize, R)> = Vec::with_capacity(rows);
    for part in parts {
        all.extend(part?);
    }
    if all.len() != rows {
        return None;
    }
    all.sort_by_key(|(index, _)| *index);
    Some(all.into_iter().map(|(_, value)| value).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_job_gives_its_result_and_a_cancelled_one_gives_none() {
        let mut job = Job::spawn(|control| {
            control.set_progress(1.0);
            Some(42)
        });
        let result = loop {
            if let Some(result) = job.try_take() {
                break result;
            }
            std::thread::yield_now();
        };
        assert_eq!(result, Some(42));

        let mut cancelled = Job::spawn(|control: &JobControl| {
            while !control.cancelled() {
                std::thread::yield_now();
            }
            None::<u32>
        });
        cancelled.cancel();
        let result = loop {
            if let Some(result) = cancelled.try_take() {
                break result;
            }
            std::thread::yield_now();
        };
        assert_eq!(result, None);
    }

    #[test]
    fn rows_come_back_in_order() {
        let control = JobControl::new();
        let rows = parallel_rows(100, &control, (0.0, 1.0), |row| row * 2).unwrap();
        assert_eq!(rows, (0..100).map(|row| row * 2).collect::<Vec<_>>());
        control.cancel();
        assert!(parallel_rows(100, &control, (0.0, 1.0), |row| row).is_none());
    }
}
