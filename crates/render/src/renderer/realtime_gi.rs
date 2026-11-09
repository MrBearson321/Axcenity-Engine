//! Real-time global illumination (graphics Phase 20): light probe volumes
//! traced again a few probes each frame, so their light follows the sun,
//! the lamps and the sky as they change.
//!
//! Each probe's six faces are drawn as a bake draws them (a
//! [`ProbeCamera`] seeing one bounce) and copied into a texture array. A
//! compute shader (`light_probe_sum.wgsl`) then sums them into spherical
//! harmonics and writes them over the probe in the light probes' buffer,
//! so nothing is read back to the CPU and the frame drawn next uses them.

use std::num::NonZeroU64;

use super::light_probes::LightProbes;
use super::probe_bake::ProbeCamera;
use super::*;

/// How many probes are traced each frame. Must match `SLOTS` in
/// light_probe_sum.wgsl.
pub(super) const PROBES_PER_FRAME: u32 = 2;

/// Scenes with more items than this keep their bake until the light
/// changes, instead of tracing every probe from the first frame.
const HEAVY_SCENE_ITEMS: usize = 4000;

/// The probes' faces and the pass that sums them.
pub(super) struct RealtimeGi {
    camera: ProbeCamera,
    /// Six layers (faces) for each probe traced in a frame.
    faces: wgpu::Texture,
    faces_view: wgpu::TextureView,
    /// Which probe each one traced this frame is written to.
    targets: wgpu::Buffer,
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::ComputePipeline,
    /// The probe to trace next, counting through every drawn volume's.
    next: u32,
    /// How many probes have been traced, for tests.
    pub(super) traced: u32,
    /// What the probes were last traced under, and how many have been
    /// traced since it changed: once that's all of them, there's nothing
    /// new to see until the light changes.
    light: u64,
    settled: u32,
}

impl RealtimeGi {
    pub(super) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        list: &DrawList,
        settings: &GraphicsSettings,
    ) -> Self {
        let mut camera = ProbeCamera::new(
            device,
            queue,
            list,
            LIGHT_PROBE_BAKE_SIZE,
            &camera_settings(settings),
        );
        camera.light_only();
        let faces = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("real-time GI faces"),
            size: wgpu::Extent3d {
                width: camera.size,
                height: camera.size,
                depth_or_array_layers: 6 * PROBES_PER_FRAME,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: camera.hdr().format(),
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let faces_view = faces.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        let targets = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("real-time GI targets"),
            size: u64::from(16 * PROBES_PER_FRAME),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let entry = |binding, ty| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty,
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("real-time GI"),
            entries: &[
                entry(
                    0,
                    wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                ),
                entry(
                    1,
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: NonZeroU64::new(u64::from(16 * PROBES_PER_FRAME)),
                    },
                ),
                entry(
                    2,
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                ),
            ],
        });
        let shader = shader_module(
            device,
            "light probe sum",
            include_str!("../light_probe_sum.wgsl"),
        );
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("light probe sum"),
            layout: Some(
                &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("light probe sum"),
                    bind_group_layouts: &[&layout],
                    push_constant_ranges: &[],
                }),
            ),
            module: &shader,
            entry_point: Some("sum_probe"),
            compilation_options: Default::default(),
            cache: None,
        });
        Self {
            camera,
            faces,
            faces_view,
            targets,
            layout,
            pipeline,
            next: 0,
            traced: 0,
            light: 0,
            settled: 0,
        }
    }

    /// Traces the next few of `probes`' probes in the scene of `list`, seen
    /// with `settings`, and writes them into its buffer. The GPU runs it
    /// before anything submitted afterward, such as the frame's scene.
    pub(super) fn update(
        &mut self,
        list: &DrawList,
        probes: &LightProbes,
        settings: &GraphicsSettings,
    ) -> Result<()> {
        let total = probes.count();
        if total == 0 {
            return Ok(());
        }
        // Every probe is six small draws of the whole scene, so a scene
        // whose light is still pays for one pass over them and no more.
        let light = light_state(list, total);
        if light != self.light {
            // A heavy scene starts from its bake: tracing every probe of
            // it first would be many slow frames for nothing new.
            let starting = self.light == 0;
            let items = list.items.len() + list.chunks.iter().map(|c| c.items.len()).sum::<usize>();
            self.settled = if starting && items > HEAVY_SCENE_ITEMS {
                total
            } else {
                0
            };
            self.light = light;
        }
        if self.settled >= total {
            return Ok(());
        }
        self.camera.set_graphics(&camera_settings(settings));
        self.camera.show(list);
        let device = self.camera.renderer.scene.device.clone();
        let queue = self.camera.renderer.scene.queue.clone();
        let mut targets = [[0u32; 4]; PROBES_PER_FRAME as usize];
        let count = PROBES_PER_FRAME.min(total);
        for slot in 0..count {
            let index = self.next % total;
            self.next = (index + 1) % total;
            targets[slot as usize][0] = index;
            let center = probes.point(index).unwrap_or_default();
            for face in 0..6 {
                self.camera.draw_face(center, face)?;
                // The camera draws every face into the same target, so each
                // is copied out before the next is drawn.
                let mut encoder = device.create_command_encoder(&Default::default());
                encoder.copy_texture_to_texture(
                    self.camera.hdr().as_image_copy(),
                    wgpu::TexelCopyTextureInfo {
                        texture: &self.faces,
                        mip_level: 0,
                        origin: wgpu::Origin3d {
                            x: 0,
                            y: 0,
                            z: slot * 6 + face,
                        },
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::Extent3d {
                        width: self.camera.size,
                        height: self.camera.size,
                        depth_or_array_layers: 1,
                    },
                );
                queue.submit([encoder.finish()]);
            }
        }
        queue.write_buffer(&self.targets, 0, bytemuck::cast_slice(&targets));
        // The light probes' buffer is made again when the volumes change,
        // so it's bound afresh each time.
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("real-time GI"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&self.faces_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.targets.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: probes.buffer().as_entire_binding(),
                },
            ],
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("light probe sum"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(count, 1, 1);
        }
        queue.submit([encoder.finish()]);
        self.traced += count;
        self.settled += count;
        Ok(())
    }

    /// Bytes of the faces and the probe camera's own targets.
    pub(super) fn bytes(&self) -> u64 {
        let size = u64::from(self.camera.size);
        // Four 16-bit floats a texel.
        size * size * 8 * u64::from(6 * PROBES_PER_FRAME)
            + self.camera.renderer.stats().target_bytes
    }
}

/// A number for the light falling on `list`'s scene and how many probes
/// there are: while it's the same, tracing them again would change nothing.
/// The clouds' drift isn't in it (drifting clouds would never let it rest).
fn light_state(list: &DrawList, probes: u32) -> u64 {
    use std::hash::{DefaultHasher, Hash, Hasher};
    let clouds = (list.clouds.cover, list.clouds.density, list.clouds.color);
    let state = format!(
        "{:?} {:?} {:?} {:?} {:?} {:?}",
        list.sun, list.ambient, list.sky, clouds, list.weather, list.lights
    );
    let mut hasher = DefaultHasher::new();
    state.hash(&mut hasher);
    probes.hash(&mut hasher);
    hasher.finish()
}

/// What the probe camera draws with: the frame's settings, with the sun's
/// shadow maps at most Medium's (spot and point light shadows included), as
/// small faces need no more and they're drawn for every face.
fn camera_settings(settings: &GraphicsSettings) -> GraphicsSettings {
    GraphicsSettings {
        shadow_quality: Some(settings.shadow_quality().min(crate::ShadowQuality::Medium)),
        ..settings.clone()
    }
}
