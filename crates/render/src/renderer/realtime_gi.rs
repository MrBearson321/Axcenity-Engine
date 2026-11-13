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
/// changes, instead of tracing every probe from the first frame, and then
/// trace a face at a time, with frames between ([`HEAVY_FACE_EVERY`]).
const HEAVY_SCENE_ITEMS: usize = 4000;

/// A heavy scene draws one of a probe's six faces every this many frames
/// while its light settles. A face is a whole draw of the scene, longer
/// than a frame there, so the frames between are what keep the window
/// moving; tracing all of it would take minutes at a couple of frames a
/// second.
const HEAVY_FACE_EVERY: u32 = 4;

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
    /// What the probes were last traced under (`None` before the first
    /// frame), and how many have been traced since it changed: once that's
    /// all of them, there's nothing new to see until the light changes.
    light: Option<LightState>,
    settled: u32,
    /// A heavy scene's next face of probe `next`, and the frames since it
    /// last drew one.
    face: u32,
    since_face: u32,
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
                    bind_group_layouts: &[Some(&layout)],
                    immediate_size: 0,
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
            light: None,
            settled: 0,
            face: 0,
            since_face: 0,
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
        let items = list.items.len() + list.chunks.iter().map(|c| c.items.len()).sum::<usize>();
        let heavy = items > HEAVY_SCENE_ITEMS;
        let light = LightState::of(list, total);
        if self.light.as_ref() != Some(&light) {
            // A heavy scene starts from its bake: tracing every probe of
            // it first would be many slow frames for nothing new. A sky
            // picture is read on another thread, so it arrives some frames
            // after the first, and that isn't the light changing either.
            let keeps_bake = self
                .light
                .as_ref()
                .is_none_or(|old| self.traced == 0 && light.is_with_sky_picture_of(old));
            self.settled = if heavy && keeps_bake { total } else { 0 };
            self.light = Some(light);
        }
        if self.settled >= total {
            return Ok(());
        }
        self.camera.set_graphics(&camera_settings(settings));
        if heavy {
            self.since_face += 1;
            if self.since_face < HEAVY_FACE_EVERY {
                return Ok(());
            }
            self.since_face = 0;
            self.camera.show(list);
            let index = self.next % total;
            let center = probes.point(index).unwrap_or_default();
            self.trace_face(center, 0, self.face)?;
            self.face += 1;
            if self.face < 6 {
                return Ok(());
            }
            self.face = 0;
            self.next = (index + 1) % total;
            let mut targets = [[0u32; 4]; PROBES_PER_FRAME as usize];
            targets[0][0] = index;
            self.sum_probes(targets, 1, probes);
            return Ok(());
        }
        self.camera.show(list);
        let mut targets = [[0u32; 4]; PROBES_PER_FRAME as usize];
        let count = PROBES_PER_FRAME.min(total);
        for slot in 0..count {
            let index = self.next % total;
            self.next = (index + 1) % total;
            targets[slot as usize][0] = index;
            let center = probes.point(index).unwrap_or_default();
            for face in 0..6 {
                self.trace_face(center, slot, face)?;
            }
        }
        self.sum_probes(targets, count, probes);
        Ok(())
    }

    /// Draws face `face` of a probe at `center` and copies it into `slot`'s
    /// layers. The camera draws every face into the same target, so each is
    /// copied out before the next is drawn.
    fn trace_face(&mut self, center: glam::Vec3, slot: u32, face: u32) -> Result<()> {
        self.camera.draw_face(center, face)?;
        let device = self.camera.renderer.scene.device.clone();
        let queue = self.camera.renderer.scene.queue.clone();
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
        Ok(())
    }

    /// Sums the first `count` slots' faces into the probes `targets` names,
    /// which the frame drawn next uses.
    fn sum_probes(
        &mut self,
        targets: [[u32; 4]; PROBES_PER_FRAME as usize],
        count: u32,
        probes: &LightProbes,
    ) {
        let device = self.camera.renderer.scene.device.clone();
        let queue = self.camera.renderer.scene.queue.clone();
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
    }

    /// Bytes of the faces and the probe camera's own targets.
    pub(super) fn bytes(&self) -> u64 {
        let size = u64::from(self.camera.size);
        // Four 16-bit floats a texel.
        size * size * 8 * u64::from(6 * PROBES_PER_FRAME)
            + self.camera.renderer.stats().target_bytes
    }
}

/// The light falling on a scene, as numbers: while it's the same, tracing
/// its probes again would change nothing.
#[derive(PartialEq)]
struct LightState {
    /// Everything but the sky's picture, and how many probes there are.
    /// The clouds' drift isn't in it (drifting clouds would never let it
    /// rest).
    light: u64,
    sky_picture: Option<u64>,
}

impl LightState {
    fn of(list: &DrawList, probes: u32) -> Self {
        use std::hash::{DefaultHasher, Hash, Hasher};
        let clouds = (list.clouds.cover, list.clouds.density, list.clouds.color);
        let sky = crate::Sky {
            image: None,
            ..list.sky.clone()
        };
        let state = format!(
            "{:?} {:?} {:?} {:?} {:?} {:?}",
            list.sun, list.ambient, sky, clouds, list.weather, list.lights
        );
        let mut hasher = DefaultHasher::new();
        state.hash(&mut hasher);
        probes.hash(&mut hasher);
        Self {
            light: hasher.finish(),
            sky_picture: list.sky.image.as_ref().map(|image| image.key),
        }
    }

    /// Whether this is `old` with only its sky picture newly loaded.
    fn is_with_sky_picture_of(&self, old: &Self) -> bool {
        self.light == old.light && old.sky_picture.is_none() && self.sky_picture.is_some()
    }
}

/// What the probe camera draws with: the frame's settings, with the sun's
/// shadow maps at most Medium's (spot and point light shadows included), as
/// small faces need no more and they're drawn for every face.
fn camera_settings(settings: &GraphicsSettings) -> GraphicsSettings {
    GraphicsSettings {
        shadow_quality: Some(settings.shadow_quality().min(crate::ShadowQuality::Medium)),
        // A probe's faces are blurred into its light: rays would be wasted.
        ray_traced_shadows: Some(crate::RayTracedShadows::Off),
        ..settings.clone()
    }
}
