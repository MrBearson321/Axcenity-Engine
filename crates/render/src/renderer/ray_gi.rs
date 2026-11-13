//! Ray traced global illumination (RT2, `docs/rendering/ray-tracing.md`):
//! light probe volumes' probes traced with rays, a batch each frame, in
//! place of real-time GI's six drawn faces a probe (`realtime_gi.rs`).
//!
//! A compute pass (`ray_gi.wgsl`) traces each probe of the batch through
//! the ray traced scene (`ray_tracing.rs`, which keeps a color and glow for
//! each of its items while this is on), sums what the rays bring back into
//! the probe's spherical harmonics, blends them over what the probe had,
//! and writes them to a buffer of its own. That's then copied over the
//! batch's places in the light probes' buffer, which the frame's scene
//! reads, so nothing comes back to the CPU. Hits are lit by the probes as
//! last updated, so light bounces again with each update.
//!
//! Exists only while the setting is on and the GPU can ray trace. The
//! camera then brings probes of its own (`light_probes.rs`), so it needs
//! no light probe volume; any drawn are traced too.

use std::num::NonZeroU64;

use crate::graphics::{EffectiveGraphics, RayTracedGlobalIllumination};

use super::light_probes::LightProbes;

/// The most probes in one batch: High's.
const MOST_PROBES: u32 = 512;

/// About how long, in seconds, a probe's light takes to settle after a
/// change (its blend over updates, as frames come at 60 a second).
const SETTLE_SECONDS: f32 = 0.5;

/// Must match `RayGi` in ray_gi.wgsl.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct RayGiUniform {
    /// Probes in the batch, rays from each, a number changing each update,
    /// unused.
    counts: [u32; 4],
    /// How much of what a probe had is kept, how far rays reach, how far
    /// shadow rays start off a hit, unused.
    settings: [f32; 4],
}

/// This frame's batch: which places of the light probes' buffer it covers,
/// as one or two runs (the count wraps around to the first probe).
pub(super) struct Batch {
    uniform: RayGiUniform,
    runs: [(u32, u32); 2],
}

/// The pass's pipeline and buffers.
pub(super) struct RayGlobalIllumination {
    /// The frame's bindings as a compute pass sees them, and their bind
    /// group (made again with the frame's).
    pub(super) frame_layout: wgpu::BindGroupLayout,
    pub(super) frame_group: Option<wgpu::BindGroup>,
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::ComputePipeline,
    uniform: wgpu::Buffer,
    /// Where each probe of the batch stands, and its place.
    batch: wgpu::Buffer,
    /// What the pass found for each probe of the batch, before it's copied.
    traced: wgpu::Buffer,
    /// The next probe to trace, counting through every drawn volume's.
    next: u32,
    /// Batches traced so far, which turns the rays.
    updates: u32,
    /// Probes traced so far, for tests.
    pub(super) probes_traced: u64,
}

impl RayGlobalIllumination {
    /// `frame_entries` are the frame's bindings, made visible to the
    /// compute pass here.
    pub(super) fn new(device: &wgpu::Device, frame_entries: &[wgpu::BindGroupLayoutEntry]) -> Self {
        let compute_entries: Vec<_> = frame_entries
            .iter()
            .map(|entry| wgpu::BindGroupLayoutEntry {
                visibility: entry.visibility | wgpu::ShaderStages::COMPUTE,
                ..*entry
            })
            .collect();
        let frame_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("frame (ray traced GI)"),
            entries: &compute_entries,
        });
        let entry = |binding, ty| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty,
            count: None,
        };
        let storage = |read_only| wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: None,
        };
        let cut_outs = super::ray_cut_outs::layout_entries(5, wgpu::ShaderStages::COMPUTE);
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ray traced GI"),
            entries: &[
                entry(
                    0,
                    wgpu::BindingType::AccelerationStructure {
                        vertex_return: false,
                    },
                ),
                entry(
                    1,
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: NonZeroU64::new(size_of::<RayGiUniform>() as u64),
                    },
                ),
                // Uniform, not storage: with the frame's four storage
                // buffers and the cut-outs' two, a ninth would pass the
                // default limit of eight a stage.
                entry(
                    2,
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: NonZeroU64::new(16 * u64::from(MOST_PROBES)),
                    },
                ),
                entry(3, storage(true)),
                entry(4, storage(false)),
                // Cut-outs' holes (LE7).
                cut_outs[0],
                cut_outs[1],
                cut_outs[2],
                cut_outs[3],
            ],
        });
        let shader = super::gpu::shader_module(device, "ray traced GI", super::gpu::RAY_GI_SHADER);
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("ray traced GI"),
            layout: Some(
                &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("ray traced GI"),
                    bind_group_layouts: &[Some(&frame_layout), Some(&layout)],
                    immediate_size: 0,
                }),
            ),
            module: &shader,
            entry_point: Some("cs_trace_probes"),
            compilation_options: Default::default(),
            cache: None,
        });
        let buffer = |label, size, usage| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage,
                mapped_at_creation: false,
            })
        };
        Self {
            frame_layout,
            frame_group: None,
            layout,
            pipeline,
            uniform: buffer(
                "ray traced GI",
                size_of::<RayGiUniform>() as u64,
                wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            ),
            batch: buffer(
                "ray traced GI batch",
                16 * u64::from(MOST_PROBES),
                wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            ),
            traced: buffer(
                "ray traced GI probes",
                64 * u64::from(MOST_PROBES),
                wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            ),
            next: 0,
            updates: 0,
            probes_traced: 0,
        }
    }

    /// Picks this frame's batch of `probes` for `graphics`' setting and
    /// writes where they stand: `None` when there are none.
    pub(super) fn plan(
        &mut self,
        queue: &wgpu::Queue,
        probes: &LightProbes,
        graphics: &EffectiveGraphics,
    ) -> Option<Batch> {
        let total = probes.count();
        let setting = graphics.ray_traced_global_illumination;
        let count = setting.probes_per_frame().min(total).min(MOST_PROBES);
        if count == 0 {
            return None;
        }
        let first = self.next % total;
        let mut standing = Vec::with_capacity(count as usize);
        for offset in 0..count {
            let index = (first + offset) % total;
            let point = probes.point(index).unwrap_or_default();
            standing.push([point.x, point.y, point.z, f32::from_bits(index)]);
        }
        self.next = (first + count) % total;
        queue.write_buffer(&self.batch, 0, bytemuck::cast_slice(&standing));
        let until_end = count.min(total - first);
        let runs = [(first, until_end), (0, count - until_end)];
        self.updates = self.updates.wrapping_add(1);
        Some(Batch {
            uniform: RayGiUniform {
                counts: [count, setting.rays(), self.updates, 0],
                settings: [keep(count, total), graphics.ray_tracing_distance, 0.05, 0.0],
            },
            runs,
        })
    }

    /// Records tracing `batch` through `tlas` (whose items' colors are in
    /// `surfaces`) and copying the result over its places in `probes`'
    /// buffer, into `encoder`. Does nothing until the frame's bindings are
    /// made (`frame_group`).
    pub(super) fn trace(
        &mut self,
        (device, queue): (&wgpu::Device, &wgpu::Queue),
        encoder: &mut wgpu::CommandEncoder,
        (tlas, surfaces, cut_outs): (&wgpu::Tlas, &wgpu::Buffer, &super::ray_cut_outs::CutOuts),
        batch: &Batch,
        probes: &LightProbes,
    ) {
        let Some(frame_group) = &self.frame_group else {
            return;
        };
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&batch.uniform));
        let [a, b, c, d] = cut_outs.entries(5);
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ray traced GI"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: tlas.as_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.batch.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: surfaces.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: self.traced.as_entire_binding(),
                },
                a,
                b,
                c,
                d,
            ],
        });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("ray traced GI"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, frame_group, &[]);
            pass.set_bind_group(1, &group, &[]);
            pass.dispatch_workgroups(batch.uniform.counts[0], 1, 1);
        }
        // Four vec4s (64 bytes) a probe, in the batch's order.
        let mut from = 0;
        for (first, count) in batch.runs {
            if count == 0 {
                continue;
            }
            encoder.copy_buffer_to_buffer(
                &self.traced,
                from * 64,
                probes.buffer(),
                u64::from(first) * 64,
                u64::from(count) * 64,
            );
            from += u64::from(count);
        }
        self.probes_traced += u64::from(batch.uniform.counts[0]);
    }

    /// Bytes of its buffers.
    pub(super) fn bytes(&self) -> u64 {
        self.batch.size() + self.traced.size() + self.uniform.size()
    }
}

/// Whether `graphics` asks for ray traced GI (and the GPU can trace rays).
pub(super) fn wanted(graphics: &EffectiveGraphics) -> bool {
    graphics.ray_queries
        && graphics.ray_traced_global_illumination != RayTracedGlobalIllumination::Off
}

/// Whether ray traced GI is drawn with `graphics` for `probes` light probes.
pub(super) fn traced(graphics: &EffectiveGraphics, probes: u32) -> bool {
    wanted(graphics) && probes > 0
}

/// How much of what a probe had is kept when `count` of `total` probes are
/// traced each frame: each is traced every `total / count` frames, so it
/// keeps less the rarer that is, settling in about [`SETTLE_SECONDS`]
/// either way (and never keeping more than 92%, so it always moves).
fn keep(count: u32, total: u32) -> f32 {
    let frames_between = total as f32 / count.max(1) as f32;
    let updates_to_settle = (SETTLE_SECONDS * 60.0 / frames_between).max(1e-3);
    (-1.0 / updates_to_settle).exp().min(0.92)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_probe_traced_every_frame_keeps_most_of_its_light_and_a_rare_one_little() {
        let every_frame = keep(64, 64);
        assert!((0.9..=0.92).contains(&every_frame), "{every_frame}");
        // Traced once every 4 seconds: long settled by its next update.
        let rare = keep(32, 32 * 240);
        assert!(rare < 0.01, "{rare}");
        assert!(keep(32, 100) > keep(32, 1000));
    }
}
