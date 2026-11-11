//! How long the GPU spends on a frame, and on each part of it, for the
//! editor's profiler and the benchmarks.
//!
//! Only devices opened with `Features::TIMESTAMP_QUERY` can be timed; the
//! player never asks for it, so it never times anything. An empty compute
//! pass writes a timestamp before each [`FramePass`] and one after the
//! last, so each part's time runs from its stamp to the next. (A part such
//! as the shadows or bloom is many render passes, so stamping between parts
//! times them whole.) The results are copied into a buffer that's read once
//! the GPU is done with it, a frame or two later, so measuring never makes
//! the CPU wait for the GPU.

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Duration;

/// The parts of a frame the GPU is timed on, in the order they're drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FramePass {
    /// The sun's cascades and the spot lights' shadow maps.
    Shadows,
    /// Solid items' depth and normals, while ambient occlusion is on.
    Prepass,
    /// Ambient occlusion and its blur.
    AmbientOcclusion,
    /// The volumetric fog's grid, while it's on (its light is added to the
    /// scene as part of `Scene`).
    VolumetricFog,
    /// GPU particles moved, sorted and written as sprites.
    Particles,
    /// The sky, items, terrain and sprites, with MSAA resolved.
    Scene,
    /// Bloom, tone mapping and FXAA.
    PostProcessing,
    /// Turned text and the UI overlay.
    Ui,
}

impl FramePass {
    pub const COUNT: usize = 8;
    pub const ALL: [FramePass; Self::COUNT] = [
        FramePass::Shadows,
        FramePass::Prepass,
        FramePass::AmbientOcclusion,
        FramePass::VolumetricFog,
        FramePass::Particles,
        FramePass::Scene,
        FramePass::PostProcessing,
        FramePass::Ui,
    ];

    pub fn label(self) -> &'static str {
        match self {
            FramePass::Shadows => "Shadows",
            FramePass::Prepass => "Depth prepass",
            FramePass::AmbientOcclusion => "Ambient occlusion",
            FramePass::VolumetricFog => "Volumetric fog",
            FramePass::Particles => "GPU particles",
            FramePass::Scene => "Scene",
            FramePass::PostProcessing => "Post-processing",
            FramePass::Ui => "UI",
        }
    }
}

/// The sun's cascades, timed within the Shadows pass.
pub(super) const CASCADES: usize = super::types::SHADOW_CASCADES;

/// A stamp before each pass and one after the last, then one before each
/// cascade and one after the last cascade.
const STAMPS: u32 = FramePass::COUNT as u32 + 1 + CASCADES as u32 + 1;
/// The stamp after the last pass.
const FRAME_END: u32 = FramePass::COUNT as u32;
/// The first cascade's stamp.
const CASCADE_STAMPS: u32 = FramePass::COUNT as u32 + 1;
const RESULT_BYTES: u64 = STAMPS as u64 * size_of::<u64>() as u64;

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
    /// While waiting for `readback` to be read: which passes that frame
    /// drew. No new frame is timed meanwhile, since the buffer is in use.
    waiting: Option<([bool; FramePass::COUNT], [bool; CASCADES])>,
    /// [`PENDING`], [`MAPPED`] or [`FAILED`]: set by the map callback, on
    /// whichever thread polls the device.
    mapped: Arc<AtomicU8>,
    /// The last frame measured, whole.
    last: Option<Duration>,
    /// Its passes, `None` for any it didn't draw.
    last_passes: [Option<Duration>; FramePass::COUNT],
    /// Its sun cascades, `None` for any it didn't draw (off, or kept from
    /// an earlier frame while the far cascades take turns).
    last_cascades: [Option<Duration>; CASCADES],
    /// How many measurements have been read, so a reader can tell a new
    /// one from the last one again.
    readings: u64,
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
            count: STAMPS,
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
            waiting: None,
            mapped: Arc::new(AtomicU8::new(PENDING)),
            last: None,
            last_passes: [None; FramePass::COUNT],
            last_cascades: [None; CASCADES],
            readings: 0,
        })
    }

    /// The GPU time of the last frame measured.
    pub(super) fn last(&self) -> Option<Duration> {
        self.last
    }

    /// The GPU time of each pass of the last frame measured, in
    /// [`FramePass::ALL`]'s order: `None` for a pass it didn't draw.
    pub(super) fn last_passes(&self) -> [Option<Duration>; FramePass::COUNT] {
        self.last_passes
    }

    /// The GPU time of each sun cascade of the last frame measured.
    pub(super) fn last_cascades(&self) -> [Option<Duration>; CASCADES] {
        self.last_cascades
    }

    /// How many measurements have been read so far.
    pub(super) fn readings(&self) -> u64 {
        self.readings
    }

    /// Reads a finished measurement, if one is ready. Returns true if this
    /// frame can be timed (call [`Self::mark`] before each pass, then
    /// [`Self::end`]).
    pub(super) fn collect(&mut self, device: &wgpu::Device) -> bool {
        let Some((drawn, cascades)) = self.waiting else {
            return true;
        };
        // Runs the map callback if the GPU has finished, without waiting.
        let _ = device.poll(wgpu::PollType::Poll);
        match self.mapped.swap(PENDING, Ordering::AcqRel) {
            PENDING => return false,
            MAPPED => {
                let bytes = self
                    .readback
                    .slice(..)
                    .get_mapped_range()
                    .expect("the buffer is mapped");
                let stamps: [u64; STAMPS as usize] = bytemuck::pod_read_unaligned(&bytes);
                drop(bytes);
                self.readback.unmap();
                let between = |start: u64, end: u64| {
                    // A clock that went backwards (some drivers, across
                    // power states) gives no reading rather than a wrong one.
                    (end >= start)
                        .then(|| Duration::from_nanos(((end - start) as f64 * self.period) as u64))
                };
                if let Some(whole) = between(stamps[0], stamps[FRAME_END as usize]) {
                    self.last = Some(whole);
                    self.readings += 1;
                    for (index, time) in self.last_passes.iter_mut().enumerate() {
                        *time = drawn[index]
                            .then(|| between(stamps[index], stamps[index + 1]))
                            .flatten();
                    }
                    // Each drawn cascade runs to the next drawn one's stamp,
                    // or to the stamp after the last.
                    let stamp = |cascade: usize| stamps[CASCADE_STAMPS as usize + cascade];
                    for index in 0..CASCADES {
                        let end = (index + 1..CASCADES)
                            .find(|&next| cascades[next])
                            .unwrap_or(CASCADES);
                        self.last_cascades[index] = cascades[index]
                            .then(|| between(stamp(index), stamp(end)))
                            .flatten();
                    }
                }
            }
            // The device was lost; the next frame tries again.
            _ => {}
        }
        self.waiting = None;
        true
    }

    /// Writes the timestamp before `pass`.
    pub(super) fn mark(&self, encoder: &mut wgpu::CommandEncoder, pass: FramePass) {
        self.stamp(encoder, pass as u32);
    }

    /// Writes the timestamp before sun cascade `cascade`, or after the last
    /// cascade for `CASCADES`.
    pub(super) fn mark_cascade(&self, encoder: &mut wgpu::CommandEncoder, cascade: usize) {
        self.stamp(encoder, CASCADE_STAMPS + cascade as u32);
    }

    /// Writes the timestamp after the last pass, and copies them all where
    /// the CPU can read them.
    pub(super) fn end(&self, encoder: &mut wgpu::CommandEncoder) {
        self.stamp(encoder, FRAME_END);
        encoder.resolve_query_set(&self.queries, 0..STAMPS, &self.resolve, 0);
        encoder.copy_buffer_to_buffer(&self.resolve, 0, &self.readback, 0, RESULT_BYTES);
    }

    /// Asks to read the timestamps once the frame just submitted is done.
    /// `drawn` says which of its passes drew anything.
    pub(super) fn submitted(
        &mut self,
        drawn: [bool; FramePass::COUNT],
        cascades: [bool; CASCADES],
    ) {
        let mapped = self.mapped.clone();
        self.readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let state = if result.is_ok() { MAPPED } else { FAILED };
                mapped.store(state, Ordering::Release);
            });
        self.waiting = Some((drawn, cascades));
    }

    /// An empty compute pass that only writes timestamp `index`.
    fn stamp(&self, encoder: &mut wgpu::CommandEncoder, index: u32) {
        encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("frame timestamp"),
            timestamp_writes: Some(wgpu::ComputePassTimestampWrites {
                query_set: &self.queries,
                beginning_of_pass_write_index: Some(index),
                end_of_pass_write_index: None,
            }),
        });
    }
}
