//! GPU particles: [`DrawList::particle_emitters`]' particles, kept, moved
//! and sorted on the GPU, then drawn by the sprite pass.
//!
//! Each emitter keeps its particles in a ring of `capacity` slots: birth
//! number `n` goes in slot `n % capacity`. Each frame, one compute pass runs
//! three shaders (`particles.wgsl`) per emitter: `simulate` moves the
//! particles on (or gives birth in a slot), a bitonic `sort` puts them
//! farthest first, and `write` turns them into `SpriteRaw`s in that order,
//! into a buffer the sprite pipeline draws as it is. Nothing is read back.
//!
//! Buffers exist only for emitters in the frame's list, and the whole of
//! this only while there's one.

use std::collections::HashMap;

use bytemuck::{Pod, Zeroable};
use glam::Vec3;

use super::gpu::{PARTICLE_SHADER, shader_module};
use crate::draw_list::{CURVE_ENTRIES, DrawList, ParticleEmitter};

/// The most particles one emitter keeps on the GPU.
pub const MAX_GPU_PARTICLES: u32 = 1 << 18;
/// Threads per workgroup, as in particles.wgsl.
const WORKGROUP: u32 = 256;
/// Bytes between sort steps in their uniform buffer: the largest offset
/// alignment a device may ask for.
const SORT_STEP_STRIDE: u64 = 256;

/// Must match `Emitter` in particles.wgsl.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct EmitterUniform {
    spawn: [[f32; 4]; 4],
    direction: [f32; 4],
    acceleration: [f32; 4],
    air: [f32; 4],
    eye: [f32; 4],
    counts: [u32; 4],
    life_size: [f32; 4],
    color_start: [f32; 4],
    color_end: [f32; 4],
    look: [f32; 4],
    /// The spawn shape's code (`SpawnShape::code`), 1 for only its
    /// surface, which way they head (`ShapeInOut::code`), and one unused.
    spawn_shape: [u32; 4],
    /// The velocity every new particle starts with besides its heading
    /// (`ParticleEmitter::inherit`), and one unused.
    inherit: [f32; 4],
    /// Size, transparency and color over life while `look[3]` is 1
    /// (`ParticleCurves`; colors' fourth number is the speed multiplier).
    shape: [[f32; 4]; CURVE_ENTRIES],
    colors: [[f32; 4]; CURVE_ENTRIES],
}

/// Bytes per particle: one `Particle`, one `Key` and one `SpriteOut` in
/// particles.wgsl.
const PARTICLE_BYTES: u64 = 32;
const KEY_BYTES: u64 = 8;
const SPRITE_BYTES: u64 = 48;

/// One emitter's buffers.
struct Kept {
    capacity: u32,
    /// The step last moved on, or `None` before the first.
    step: Option<u64>,
    uniform: wgpu::Buffer,
    /// Its particles as sprites, farthest first, for the sprite pipeline.
    sprites: wgpu::Buffer,
    group: wgpu::BindGroup,
    bytes: u64,
}

/// One emitter to draw this frame.
pub(super) struct EmitterDraw {
    key: u64,
    /// Its texture's key, if it has one.
    texture: Option<u64>,
    /// How far its part is from the camera, squared.
    distance: f32,
}

pub(super) struct GpuParticles {
    layout: wgpu::BindGroupLayout,
    simulate: wgpu::ComputePipeline,
    sort: wgpu::ComputePipeline,
    write: wgpu::ComputePipeline,
    steps_group: wgpu::BindGroup,
    steps_buffer: wgpu::Buffer,
    kept: HashMap<u64, Kept>,
    /// This frame's emitters, farthest first.
    order: Vec<EmitterDraw>,
}

impl GpuParticles {
    pub(super) fn new(device: &wgpu::Device) -> Self {
        let storage = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: false },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let uniform = |dynamic| wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: dynamic,
                min_binding_size: None,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("GPU particles"),
            entries: &[uniform(false), storage(1), storage(2), storage(3)],
        });
        let steps_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("GPU particle sort steps"),
            entries: &[uniform(true)],
        });
        // Every step of a bitonic sort of `MAX_GPU_PARTICLES`. A smaller
        // sort's steps are the first ones, as `k` only grows.
        let mut steps = Vec::new();
        let mut k = 2;
        while k <= MAX_GPU_PARTICLES {
            let mut j = k / 2;
            while j > 0 {
                let mut step = [0u32; (SORT_STEP_STRIDE / 4) as usize];
                step[0] = j;
                step[1] = k;
                steps.extend_from_slice(&step);
                j /= 2;
            }
            k *= 2;
        }
        let steps_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("GPU particle sort steps"),
            size: (steps.len() * 4) as u64,
            usage: wgpu::BufferUsages::UNIFORM,
            mapped_at_creation: true,
        });
        steps_buffer
            .slice(..)
            .get_mapped_range_mut()
            .expect("the buffer is mapped")
            .copy_from_slice(bytemuck::cast_slice(&steps));
        steps_buffer.unmap();
        let steps_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("GPU particle sort steps"),
            layout: &steps_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &steps_buffer,
                    offset: 0,
                    size: wgpu::BufferSize::new(16),
                }),
            }],
        });

        let shader = shader_module(device, "GPU particles", PARTICLE_SHADER);
        let pipeline = |label, layouts: &[&wgpu::BindGroupLayout], entry_point| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(label),
                layout: Some(
                    &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                        label: Some(label),
                        bind_group_layouts: &super::gpu::some(layouts),
                        immediate_size: 0,
                    }),
                ),
                module: &shader,
                entry_point: Some(entry_point),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        Self {
            simulate: pipeline("simulate particles", &[&layout], "simulate"),
            sort: pipeline("sort particles", &[&layout, &steps_layout], "sort"),
            write: pipeline("write particles", &[&layout], "write"),
            layout,
            steps_group,
            steps_buffer,
            kept: HashMap::new(),
            order: Vec::new(),
        }
    }

    /// Makes buffers for `list`'s new emitters (and those whose capacity
    /// changed), frees those of emitters it no longer has, and writes each
    /// one's step. `textures` are the emitters' texture keys as the sprite
    /// pass will draw them, in the list's order.
    pub(super) fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        list: &DrawList,
        textures: &[Option<u64>],
    ) {
        let emitters = &list.particle_emitters;
        self.kept
            .retain(|key, _| emitters.iter().any(|emitter| emitter.key == *key));
        let eye = list.camera.position;
        self.order.clear();
        for (emitter, &texture) in emitters.iter().zip(textures) {
            let capacity = emitter
                .capacity
                .clamp(1, MAX_GPU_PARTICLES)
                .next_power_of_two();
            if self
                .kept
                .get(&emitter.key)
                .is_none_or(|kept| kept.capacity != capacity)
            {
                let kept = self.create(device, capacity);
                self.kept.insert(emitter.key, kept);
            }
            let kept = self.kept.get_mut(&emitter.key).expect("made above");
            // A step drawn again moves nothing on.
            let (seconds, births) = if kept.step == Some(emitter.step) {
                (0.0, 0)
            } else {
                kept.step = Some(emitter.step);
                (
                    if emitter.seconds.is_finite() {
                        emitter.seconds.max(0.0)
                    } else {
                        0.0
                    },
                    emitter.births.min(capacity),
                )
            };
            queue.write_buffer(
                &kept.uniform,
                0,
                bytemuck::bytes_of(&uniform_of(emitter, (eye, seconds), (capacity, births))),
            );
            self.order.push(EmitterDraw {
                key: emitter.key,
                texture,
                distance: emitter.spawn.w_axis.truncate().distance_squared(eye),
            });
        }
        // Farthest first, by where they're born.
        self.order.sort_by(|a, b| b.distance.total_cmp(&a.distance));
    }

    fn create(&self, device: &wgpu::Device, capacity: u32) -> Kept {
        let buffer = |label, bytes: u64, usage| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: bytes * u64::from(capacity),
                usage: wgpu::BufferUsages::STORAGE | usage,
                mapped_at_creation: false,
            })
        };
        // New buffers are zeroed: every particle is dead (age 0, life 0).
        let particles = buffer("GPU particles", PARTICLE_BYTES, wgpu::BufferUsages::empty());
        let keys = buffer("GPU particle keys", KEY_BYTES, wgpu::BufferUsages::empty());
        let sprites = buffer(
            "GPU particle sprites",
            SPRITE_BYTES,
            wgpu::BufferUsages::VERTEX,
        );
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("GPU particle emitter"),
            size: size_of::<EmitterUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("GPU particles"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: particles.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: keys.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: sprites.as_entire_binding(),
                },
            ],
        });
        Kept {
            capacity,
            step: None,
            bytes: particles.size() + keys.size() + sprites.size() + uniform.size(),
            uniform,
            sprites,
            group,
        }
    }

    /// Moves, sorts and writes every emitter's particles.
    pub(super) fn run(&self, encoder: &mut wgpu::CommandEncoder) {
        if self.order.is_empty() {
            return;
        }
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("GPU particles"),
            timestamp_writes: None,
        });
        for draw in &self.order {
            let kept = &self.kept[&draw.key];
            let groups = kept.capacity.div_ceil(WORKGROUP);
            pass.set_bind_group(0, &kept.group, &[]);
            pass.set_pipeline(&self.simulate);
            pass.dispatch_workgroups(groups, 1, 1);
            pass.set_pipeline(&self.sort);
            let levels = kept.capacity.trailing_zeros();
            let steps = levels * (levels + 1) / 2;
            for step in 0..steps {
                let offset = (u64::from(step) * SORT_STEP_STRIDE) as u32;
                pass.set_bind_group(1, &self.steps_group, &[offset]);
                pass.dispatch_workgroups(groups, 1, 1);
            }
            pass.set_pipeline(&self.write);
            pass.dispatch_workgroups(groups, 1, 1);
        }
    }

    /// This frame's emitters to draw, farthest first: each one's sprites,
    /// how many, and its texture's key.
    pub(super) fn draws(&self) -> Vec<(&wgpu::Buffer, u32, Option<u64>)> {
        self.order
            .iter()
            .map(|draw| {
                let kept = &self.kept[&draw.key];
                (&kept.sprites, kept.capacity, draw.texture)
            })
            .collect()
    }

    /// Particle slots kept, summed over every emitter.
    pub(super) fn count(&self) -> u32 {
        self.kept.values().map(|kept| kept.capacity).sum()
    }

    /// Emitters with buffers.
    #[cfg(test)]
    pub(super) fn emitters(&self) -> usize {
        self.kept.len()
    }

    /// Bytes of buffers: every emitter's and the sort steps.
    pub(super) fn bytes(&self) -> u64 {
        self.steps_buffer.size() + self.kept.values().map(|kept| kept.bytes).sum::<u64>()
    }
}

fn uniform_of(
    emitter: &ParticleEmitter,
    (eye, seconds): (Vec3, f32),
    (capacity, births): (u32, u32),
) -> EmitterUniform {
    let look = &emitter.look;
    let finite = |v: Vec3| if v.is_finite() { v } else { Vec3::ZERO };
    let direction = emitter.direction.normalize_or(Vec3::Y);
    let spread = emitter.spread.clamp(0.0, std::f32::consts::PI);
    let [start, end] = look.color;
    EmitterUniform {
        spawn: emitter.spawn.to_cols_array_2d(),
        direction: direction.extend(spread.cos()).to_array(),
        acceleration: finite(emitter.acceleration)
            .extend(emitter.drag.max(0.0))
            .to_array(),
        air: finite(emitter.air).extend(emitter.speed).to_array(),
        eye: eye.extend(seconds).to_array(),
        // Each emitter's own random numbers.
        counts: [
            capacity,
            emitter.born,
            births,
            (emitter.key ^ (emitter.key >> 32)) as u32,
        ],
        life_size: [
            emitter.lifetime.max(0.0),
            look.size[0],
            look.size[1],
            look.rotation_degrees.to_radians(),
        ],
        color_start: [start[0], start[1], start[2], look.transparency[0]],
        color_end: [end[0], end[1], end[2], look.transparency[1]],
        look: [
            look.spin_degrees.to_radians(),
            look.brightness,
            look.light_emission,
            if look.curves.is_some() { 1.0 } else { 0.0 },
        ],
        spawn_shape: [
            emitter.shape.code(),
            u32::from(emitter.shape.surface),
            emitter.in_out.code(),
            0,
        ],
        inherit: finite(emitter.inherit).extend(0.0).to_array(),
        shape: look
            .curves
            .as_ref()
            .map_or([[0.0; 4]; CURVE_ENTRIES], |curves| curves.shape),
        colors: look
            .curves
            .as_ref()
            .map_or([[0.0, 0.0, 0.0, 1.0]; CURVE_ENTRIES], |curves| {
                std::array::from_fn(|entry| {
                    let [r, g, b] = curves.color[entry];
                    [r, g, b, curves.speed[entry]]
                })
            }),
    }
}
