//! How long the GPU spends on a frame, for the editor's profiler.
//!
//! Only devices opened with `Features::TIMESTAMP_QUERY` can be timed; the
//! player never asks for it, so it never times anything. Two empty compute
//! passes write a timestamp before and after the frame's work. The result
//! is copied into a buffer that's read once the GPU is done with it, a frame
//! or two later, so measuring never makes the CPU wait for the GPU.

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Duration;

/// Two `u64` timestamps: before and after the frame.
const RESULT_BYTES: u64 = 2 * size_of::<u64>() as u64;

/// What became of the last request to read the results.
const PENDING: u8 = 0;
const MAPPED: u8 = 1;
const FAILED: u8 = 2;

pub(super) struct GpuTimer {
    queries: wgpu::QuerySet,
    /// Where the queries are resolved on the GPU.
    resolve: wgpu::Buffer,
    /// A copy of `resolve` the CPU can read.
    readback: wgpu::Buffer,
    /// Nanoseconds per timestamp tick.
    period: f64,
    /// True from asking to read `readback` until it's been read and unmapped.
    /// No new frame is timed meanwhile, since the buffer is in use.
    waiting: bool,
    /// [`PENDING`], [`MAPPED`] or [`FAILED`]: set by the map callback, on
    /// whichever thread polls the device.
    mapped: Arc<AtomicU8>,
    /// The last frame measured.
    last: Option<Duration>,
}

impl GpuTimer {
    /// A timer, or `None` if `device` can't take timestamps.
    pub(super) fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Option<Self> {
        if !device.features().contains(wgpu::Features::TIMESTAMP_QUERY) {
            return None;
        }
        let queries = device.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some("frame timestamps"),
            ty: wgpu::QueryType::Timestamp,
            count: 2,
        });
        let buffer = |label, usage| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: RESULT_BYTES,
                usage,
                mapped_at_creation: false,
            })
        };
        Some(Self {
            queries,
            resolve: buffer(
                "frame timestamps resolve",
                wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            ),
            readback: buffer(
                "frame timestamps readback",
                wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            ),
            period: f64::from(queue.get_timestamp_period()),
            waiting: false,
            mapped: Arc::new(AtomicU8::new(PENDING)),
            last: None,
        })
    }

    /// The GPU time of the last frame measured.
    pub(super) fn last(&self) -> Option<Duration> {
        self.last
    }

    /// Reads a finished measurement, if one is ready. Returns true if this
    /// frame can be timed (call [`Self::begin`] and [`Self::end`]).
    pub(super) fn collect(&mut self, device: &wgpu::Device) -> bool {
        if !self.waiting {
            return true;
        }
        // Runs the map callback if the GPU has finished, without waiting.
        let _ = device.poll(wgpu::PollType::Poll);
        match self.mapped.swap(PENDING, Ordering::AcqRel) {
            PENDING => return false,
            MAPPED => {
                let bytes = self.readback.slice(..).get_mapped_range();
                let [start, end]: [u64; 2] = bytemuck::pod_read_unaligned(&bytes);
                drop(bytes);
                self.readback.unmap();
                // A clock that went backwards (some drivers, across power
                // states) gives no reading rather than a wrong one.
                if end >= start {
                    let nanos = (end - start) as f64 * self.period;
                    self.last = Some(Duration::from_nanos(nanos as u64));
                }
            }
            // The device was lost; the next frame tries again.
            _ => {}
        }
        self.waiting = false;
        true
    }

    /// Writes the timestamp before the frame's work.
    pub(super) fn begin(&self, encoder: &mut wgpu::CommandEncoder) {
        self.stamp(encoder, Some(0), None);
    }

    /// Writes the timestamp after the frame's work, and copies both where
    /// the CPU can read them.
    pub(super) fn end(&self, encoder: &mut wgpu::CommandEncoder) {
        self.stamp(encoder, None, Some(1));
        encoder.resolve_query_set(&self.queries, 0..2, &self.resolve, 0);
        encoder.copy_buffer_to_buffer(&self.resolve, 0, &self.readback, 0, RESULT_BYTES);
    }

    /// Asks to read the timestamps once the frame just submitted is done.
    pub(super) fn submitted(&mut self) {
        let mapped = self.mapped.clone();
        self.readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let state = if result.is_ok() { MAPPED } else { FAILED };
                mapped.store(state, Ordering::Release);
            });
        self.waiting = true;
    }

    /// An empty compute pass that only writes timestamps.
    fn stamp(&self, encoder: &mut wgpu::CommandEncoder, start: Option<u32>, end: Option<u32>) {
        encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("frame timestamp"),
            timestamp_writes: Some(wgpu::ComputePassTimestampWrites {
                query_set: &self.queries,
                beginning_of_pass_write_index: start,
                end_of_pass_write_index: end,
            }),
        });
    }
}
