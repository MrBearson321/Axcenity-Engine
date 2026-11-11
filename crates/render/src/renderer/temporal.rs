//! Motion vectors, TAA and motion blur (`temporal.wgsl`, and `vs_motion` and
//! `fs_motion` in shader.wgsl): the passes that keep last frame's image.
//!
//! While TAA is on or motion blur is drawn, the prepass is kept alive and a
//! motion target follows it: the camera's motion, worked out from the
//! prepass's depth, with the items that moved since last frame drawn over
//! it. Each item is known from frame to frame by [`DrawItem::id`]; [`History`]
//! keeps last frame's camera and each item's transform and joints.
//!
//! TAA draws the scene a fraction of a pixel to one side each frame (the
//! jitter, [`History::jitter`]) and blends it into the frames before, which
//! it keeps in one of two history targets (each frame reads the other).
//! Motion blur streaks the scene by the motion. Each writes into a target of
//! its own, which is then copied over the scene's HDR texture, so what comes
//! after (depth of field, bloom, the composite pass) reads the scene as
//! before. Every target here exists only while it's used.

use std::collections::HashMap;
use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec2, Vec3};

use crate::draw_list::{DrawItem, DrawList, LensEffects, Skin};
use crate::graphics::{EffectiveGraphics, MotionBlurQuality};
use crate::mesh::Vertex;

use super::gpu::{shader_module, target_bytes};
use super::lighting::instance_raw;
use super::lod::Draw;
use super::post::fullscreen_pass;
use super::skinning::JointRaw;
use super::{DEPTH_FORMAT, FAR_PLANE, HDR_FORMAT, MeshKey, NEAR_PLANE};

/// How far each pixel moved across the screen since last frame, in texture
/// coordinates.
pub(super) const MOTION_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rg16Float;
/// This frame's share of TAA's blend: the rest is the frames before.
pub(super) const TAA_SHARE: f32 = 0.1;
/// How many places the jitter steps through before it repeats.
pub(super) const JITTER_PHASES: u64 = 8;
/// The longest half of a motion blur streak, as a share of the scene's
/// height (30 pixels each way at 1080).
pub(super) const MAX_MOTION_BLUR: f32 = 1.0 / 36.0;

/// What the temporal passes read. Must match `Temporal` in temporal.wgsl
/// (its first two matrices are `MotionView` in shader.wgsl).
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct TemporalUniform {
    view_proj: [[f32; 4]; 4],
    previous_view_proj: [[f32; 4]; 4],
    inv_view_proj: [[f32; 4]; 4],
    /// 1 when last frame's history can be used, this frame's share of the
    /// blend, the near plane and the far plane.
    settings: [f32; 4],
    size: [f32; 4],
    /// Motion blur's share of a frame's motion, its longest half-streak in
    /// pixels, samples per pixel and tile size in pixels.
    blur: [f32; 4],
    noise: [f32; 4],
}

/// A moved item as the motion pass reads it. Must match `MotionIn` in
/// shader.wgsl.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub(super) struct MotionInstance {
    model: [[f32; 4]; 4],
    previous: [[f32; 4]; 4],
    skin: [u32; 2],
    sway: [f32; 4],
    fade: f32,
}

/// The temporal passes' pipelines, uniform and per-frame buffers, which
/// don't depend on the scene's size.
pub(super) struct TemporalPasses {
    uniform: wgpu::Buffer,
    sampler: wgpu::Sampler,
    camera_layout: wgpu::BindGroupLayout,
    taa_layout: wgpu::BindGroupLayout,
    tile_layout: wgpu::BindGroupLayout,
    blur_layout: wgpu::BindGroupLayout,
    /// Group 3 of the moving items' pass: the uniform and last frame's joints.
    motion_layout: wgpu::BindGroupLayout,
    motion_group: wgpu::BindGroup,
    camera_motion: wgpu::RenderPipeline,
    moving: wgpu::RenderPipeline,
    taa: wgpu::RenderPipeline,
    tile_max: wgpu::RenderPipeline,
    motion_blur: wgpu::RenderPipeline,
    /// This frame's moved items, and how many fit.
    instances: wgpu::Buffer,
    instance_capacity: usize,
    /// Last frame's joints, laid out as this frame's are, and how many fit.
    previous_joints: wgpu::Buffer,
    joint_capacity: usize,
}

/// The motion target (two, so last frame's is still there to compare with)
/// and, while used, TAA's two history targets and motion blur's target,
/// with the bindings of each frame's passes (by [`History::parity`]).
pub(super) struct TemporalTargets {
    /// Kept for reading back in tests.
    #[cfg_attr(not(test), allow(dead_code))]
    motion_textures: [wgpu::Texture; 2],
    motion: [wgpu::TextureView; 2],
    camera_groups: [wgpu::BindGroup; 2],
    taa: Option<TaaTargets>,
    blur: Option<BlurTargets>,
    /// Bytes of every texture here.
    pub(super) bytes: u64,
    /// A clone of [`super::TargetsAlive::motion`].
    _alive: Arc<()>,
}

struct TaaTargets {
    /// Each frame draws into one and reads the other.
    history: [wgpu::Texture; 2],
    views: [wgpu::TextureView; 2],
    groups: [wgpu::BindGroup; 2],
    /// A clone of [`super::TargetsAlive::taa`].
    _alive: Arc<()>,
}

struct BlurTargets {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    /// The longest streak in each tile of the screen.
    tiles: wgpu::TextureView,
    tile_groups: [wgpu::BindGroup; 2],
    groups: [wgpu::BindGroup; 2],
    /// A clone of [`super::TargetsAlive::motion_blur`].
    _alive: Arc<()>,
}

/// What the temporal passes keep from one frame to the next.
#[derive(Default)]
pub(super) struct History {
    /// Whether TAA's history target holds last frame's picture, at this
    /// size: false after the targets are remade or a frame without TAA.
    pub(super) valid: bool,
    /// Which of each pair of targets this frame draws into.
    parity: usize,
    /// Frames drawn with the motion target, for the jitter and the noise.
    frame: u64,
    /// Last frame's view, without the jitter.
    view_proj: Option<Mat4>,
    /// Each item's transform and joints last frame, by its ID.
    models: HashMap<u64, Mat4>,
    joints: HashMap<u64, Vec<JointRaw>>,
}

/// What the temporal passes need of the scene's targets.
pub(super) struct SceneViews<'a> {
    pub(super) hdr: &'a wgpu::TextureView,
    pub(super) depth: &'a wgpu::TextureView,
}

impl TemporalPasses {
    /// `item_layouts` are the frame's, an empty one and the skin's, as the
    /// moving items' pass binds them.
    pub(super) fn new(device: &wgpu::Device, item_layouts: [&wgpu::BindGroupLayout; 3]) -> Self {
        let fragment = wgpu::ShaderStages::FRAGMENT;
        let uniform_entry = |visibility| wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let texture = |binding, sample_type| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: fragment,
            ty: wgpu::BindingType::Texture {
                sample_type,
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let depth = || texture(1, wgpu::TextureSampleType::Depth);
        let unfiltered = |binding| {
            texture(
                binding,
                wgpu::TextureSampleType::Float { filterable: false },
            )
        };
        let layout = |label, entries: &[wgpu::BindGroupLayoutEntry]| {
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some(label),
                entries,
            })
        };
        let camera_layout = layout("camera motion", &[uniform_entry(fragment), depth()]);
        let taa_layout = layout(
            "TAA",
            &[
                uniform_entry(fragment),
                depth(),
                unfiltered(2),
                texture(3, wgpu::TextureSampleType::Float { filterable: true }),
                unfiltered(4),
                unfiltered(5),
                wgpu::BindGroupLayoutEntry {
                    binding: 6,
                    visibility: fragment,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        );
        let tile_layout = layout(
            "motion blur tiles",
            &[uniform_entry(fragment), unfiltered(4)],
        );
        let blur_layout = layout(
            "motion blur",
            &[
                uniform_entry(fragment),
                depth(),
                unfiltered(2),
                unfiltered(4),
                unfiltered(7),
            ],
        );
        let motion_layout = layout(
            "moving items",
            &[
                uniform_entry(wgpu::ShaderStages::VERTEX),
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        );
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("temporal"),
            size: size_of::<TemporalUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("TAA history"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let shader = device.create_shader_module(wgpu::include_wgsl!("../temporal.wgsl"));
        let fullscreen =
            |label, layout: &wgpu::BindGroupLayout, entry, format: wgpu::TextureFormat| {
                let pipeline_layout =
                    device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                        label: Some(label),
                        bind_group_layouts: &[Some(layout)],
                        immediate_size: 0,
                    });
                device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some(label),
                    layout: Some(&pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &shader,
                        entry_point: Some("vs_main"),
                        compilation_options: Default::default(),
                        buffers: &[],
                    },
                    primitive: wgpu::PrimitiveState::default(),
                    depth_stencil: None,
                    multisample: Default::default(),
                    fragment: Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: Some(entry),
                        compilation_options: Default::default(),
                        targets: &[Some(format.into())],
                    }),
                    multiview_mask: None,
                    cache: None,
                })
            };
        let camera_motion = fullscreen(
            "camera motion",
            &camera_layout,
            "fs_camera_motion",
            MOTION_FORMAT,
        );
        let taa = fullscreen("TAA", &taa_layout, "fs_taa", HDR_FORMAT);
        let tile_max = fullscreen(
            "motion blur tiles",
            &tile_layout,
            "fs_tile_max",
            MOTION_FORMAT,
        );
        let motion_blur = fullscreen("motion blur", &blur_layout, "fs_motion_blur", HDR_FORMAT);
        let moving = moving_pipeline(device, item_layouts, &motion_layout);
        let instance_capacity = 16;
        let joint_capacity = 16;
        let instances = instance_buffer(device, instance_capacity);
        let previous_joints = joint_buffer(device, joint_capacity);
        let motion_group = motion_group(device, &motion_layout, &uniform, &previous_joints);
        Self {
            uniform,
            sampler,
            camera_layout,
            taa_layout,
            tile_layout,
            blur_layout,
            motion_layout,
            motion_group,
            camera_motion,
            moving,
            taa,
            tile_max,
            motion_blur,
            instances,
            instance_capacity,
            previous_joints,
            joint_capacity,
        }
    }
}

/// The moving items' pipeline: each moved item's motion, drawn where the
/// prepass's depth shows it (equal or nearer, never written).
fn moving_pipeline(
    device: &wgpu::Device,
    [frame_layout, empty_layout, skin_layout]: [&wgpu::BindGroupLayout; 3],
    motion_layout: &wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
    let shader = shader_module(device, "moving items", super::gpu::SCENE_SHADER);
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("moving items"),
        bind_group_layouts: &[
            Some(frame_layout),
            Some(empty_layout),
            Some(skin_layout),
            Some(motion_layout),
        ],
        immediate_size: 0,
    });
    let vertex_layout = wgpu::VertexBufferLayout {
        array_stride: size_of::<Vertex>() as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &wgpu::vertex_attr_array![0 => Float32x3],
    };
    let instance_layout = wgpu::VertexBufferLayout {
        array_stride: size_of::<MotionInstance>() as u64,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &wgpu::vertex_attr_array![
            2 => Float32x4, 3 => Float32x4, 4 => Float32x4, 5 => Float32x4,
            6 => Float32x4, 7 => Float32x4, 8 => Float32x4, 9 => Float32x4,
            10 => Uint32x2,
            11 => Float32x4,
            12 => Float32,
        ],
    };
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("moving items"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_motion"),
            compilation_options: Default::default(),
            buffers: &[Some(vertex_layout), Some(instance_layout)],
        },
        // Back faces are behind the front ones, so the depth test leaves
        // them out; double-sided items need theirs.
        primitive: wgpu::PrimitiveState {
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::LessEqual),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_motion"),
            compilation_options: Default::default(),
            targets: &[Some(MOTION_FORMAT.into())],
        }),
        multiview_mask: None,
        cache: None,
    })
}

fn instance_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("moved items"),
        size: (capacity * size_of::<MotionInstance>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn joint_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("last frame's joints"),
        size: (capacity * size_of::<JointRaw>()) as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn motion_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    uniform: &wgpu::Buffer,
    joints: &wgpu::Buffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("moving items"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: joints.as_entire_binding(),
            },
        ],
    })
}

/// Which temporal targets a frame needs: TAA's history and motion blur's.
/// Either needs the motion target and the prepass.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct TemporalUse {
    pub(super) taa: bool,
    pub(super) motion_blur: bool,
}

impl TemporalUse {
    /// What `graphics` and a draw list that wants motion blur (or doesn't)
    /// use.
    pub(super) fn of(graphics: &EffectiveGraphics, wants_motion_blur: bool) -> Self {
        Self {
            taa: graphics.anti_aliasing == crate::AntiAliasing::Taa,
            motion_blur: wants_motion_blur && graphics.motion_blur != MotionBlurQuality::Off,
        }
    }

    pub(super) fn any(self) -> bool {
        self.taa || self.motion_blur
    }
}

/// Whether `lens` has motion blur that streaks anything.
pub(super) fn wants_motion_blur(lens: &LensEffects) -> bool {
    lens.motion_blur
        .is_some_and(|share| share.is_finite() && share > 0.0)
}

/// Motion blur's tile size for a scene `height` pixels tall: as long as its
/// longest half-streak, so the tiles around a pixel hold every streak that
/// reaches it.
fn blur_tile(height: u32) -> u32 {
    ((height as f32 * MAX_MOTION_BLUR).ceil() as u32).clamp(4, 64)
}

impl TemporalTargets {
    /// Makes the targets `used` needs for a `width` × `height` scene,
    /// reading `scene`'s HDR color and the prepass's depth. Each set holds a
    /// clone of its counter in `alive`.
    pub(super) fn new(
        device: &wgpu::Device,
        passes: &TemporalPasses,
        (width, height): (u32, u32),
        scene: SceneViews<'_>,
        used: TemporalUse,
        alive: &super::TargetsAlive,
    ) -> Self {
        let mut bytes = 0;
        let mut texture = |label, (width, height): (u32, u32), format| {
            let (width, height) = (width.max(1), height.max(1));
            bytes += target_bytes((width, height, 1), format, 1);
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            })
        };
        let view = |texture: &wgpu::Texture| texture.create_view(&Default::default());
        let size = (width, height);
        let motion_textures = [0, 1].map(|_| texture("motion vectors", size, MOTION_FORMAT));
        let motion = [view(&motion_textures[0]), view(&motion_textures[1])];
        let history = used
            .taa
            .then(|| [0, 1].map(|_| texture("TAA history", size, HDR_FORMAT)));
        let blur = used.motion_blur.then(|| {
            let tile = blur_tile(height);
            let tiles = (width.div_ceil(tile), height.div_ceil(tile));
            (
                texture("motion blur", size, HDR_FORMAT),
                view(&texture("motion blur tiles", tiles, MOTION_FORMAT)),
            )
        });

        // The uniform, then each texture at its binding (and for TAA the
        // history's sampler).
        let group =
            |label, layout: &wgpu::BindGroupLayout, entries: &[(u32, &wgpu::TextureView)]| {
                let mut all = vec![wgpu::BindGroupEntry {
                    binding: 0,
                    resource: passes.uniform.as_entire_binding(),
                }];
                all.extend(entries.iter().map(|&(binding, view)| wgpu::BindGroupEntry {
                    binding,
                    resource: wgpu::BindingResource::TextureView(view),
                }));
                if entries.iter().any(|&(binding, _)| binding == 3) {
                    all.push(wgpu::BindGroupEntry {
                        binding: 6,
                        resource: wgpu::BindingResource::Sampler(&passes.sampler),
                    });
                }
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some(label),
                    layout,
                    entries: &all,
                })
            };
        let camera_groups =
            [0, 1].map(|_| group("camera motion", &passes.camera_layout, &[(1, scene.depth)]));
        let taa = history.map(|history| {
            let views = [view(&history[0]), view(&history[1])];
            // Frame `parity` draws into its own history and reads the
            // other's, with its own motion and the other's.
            let groups = [0, 1].map(|parity| {
                group(
                    "TAA",
                    &passes.taa_layout,
                    &[
                        (1, scene.depth),
                        (2, scene.hdr),
                        (3, &views[1 - parity]),
                        (4, &motion[parity]),
                        (5, &motion[1 - parity]),
                    ],
                )
            });
            TaaTargets {
                history,
                views,
                groups,
                _alive: alive.taa.clone(),
            }
        });
        let blur = blur.map(|(texture, tiles)| {
            let tile_groups = [0, 1].map(|parity| {
                group(
                    "motion blur tiles",
                    &passes.tile_layout,
                    &[(4, &motion[parity])],
                )
            });
            let groups = [0, 1].map(|parity| {
                group(
                    "motion blur",
                    &passes.blur_layout,
                    &[
                        (1, scene.depth),
                        (2, scene.hdr),
                        (4, &motion[parity]),
                        (7, &tiles),
                    ],
                )
            });
            BlurTargets {
                view: view(&texture),
                texture,
                tiles,
                tile_groups,
                groups,
                _alive: alive.motion_blur.clone(),
            }
        });
        Self {
            motion_textures,
            motion,
            camera_groups,
            taa,
            blur,
            bytes,
            _alive: alive.motion.clone(),
        }
    }

    pub(super) fn has_taa(&self) -> bool {
        self.taa.is_some()
    }

    /// The motion target the last frame drew (before `history` moved on).
    #[cfg(test)]
    pub(super) fn last_motion(&self, history: &History) -> &wgpu::Texture {
        &self.motion_textures[1 - history.parity]
    }

    /// What these targets were made for.
    pub(super) fn used(&self) -> TemporalUse {
        TemporalUse {
            taa: self.taa.is_some(),
            motion_blur: self.blur.is_some(),
        }
    }
}

impl History {
    /// Forgets everything kept: the next frame drawn with motion starts
    /// afresh, with nothing moving but the camera.
    pub(super) fn reset(&mut self) {
        *self = Self::default();
    }

    /// Where TAA moves this frame's scene, in its pixels: one of
    /// [`JITTER_PHASES`] places of a Halton sequence, each within half a
    /// pixel of the middle.
    pub(super) fn jitter(&self) -> Vec2 {
        let index = self.frame % JITTER_PHASES + 1;
        Vec2::new(halton(index, 2), halton(index, 3)) - 0.5
    }

    /// The camera's view as drawn this frame: `view_proj` moved by the
    /// jitter, for a scene of `size` pixels.
    pub(super) fn jittered(&self, view_proj: Mat4, (width, height): (u32, u32)) -> Mat4 {
        let jitter = self.jitter();
        let offset = Vec3::new(
            2.0 * jitter.x / width.max(1) as f32,
            -2.0 * jitter.y / height.max(1) as f32,
            0.0,
        );
        Mat4::from_translation(offset) * view_proj
    }
}

/// The `index`th number of the Halton sequence in `base`: spread evenly
/// over 0 to 1, each new one between those before.
fn halton(mut index: u64, base: u64) -> f32 {
    let mut fraction = 1.0;
    let mut result = 0.0;
    while index > 0 {
        fraction /= base as f32;
        result += fraction * (index % base) as f32;
        index /= base;
    }
    result
}

/// The moved items of a frame, for the motion pass, and last frame's joints
/// laid out as this frame's.
pub(super) struct Moved {
    /// By mesh, so each mesh's run is drawn at once.
    pub(super) instances: Vec<(MeshKey, MotionInstance)>,
    pub(super) previous_joints: Vec<JointRaw>,
}

impl History {
    /// What moved among `draws` since last frame: each solid item (drawn in
    /// the prepass) with an ID whose transform changed, or that's posed.
    /// `joints` and `skins` are this frame's (see `DrawPlan`).
    pub(super) fn moved(
        &self,
        draws: &[Draw<'_>],
        joints: &[JointRaw],
        skins: &HashMap<*const Skin, [u32; 2]>,
    ) -> Moved {
        let mut previous_joints = joints.to_vec();
        let mut instances = Vec::new();
        for draw in draws {
            let item = draw.item;
            if item.id == 0 || !in_prepass(item) {
                continue;
            }
            let Some(previous) = self.models.get(&item.id) else {
                continue;
            };
            let skin = item
                .skin
                .as_deref()
                .and_then(|skin| skins.get(&std::ptr::from_ref(skin)).copied());
            if *previous == item.transform && skin.is_none() {
                continue;
            }
            let Some(raw) = instance_raw(item) else {
                continue;
            };
            let skin = skin.unwrap_or([0, 0]);
            if skin[1] > 0
                && let Some(before) = self.joints.get(&item.id)
                && before.len() == skin[1] as usize
            {
                let start = skin[0] as usize;
                previous_joints[start..start + before.len()].copy_from_slice(before);
            }
            instances.push((
                draw.mesh(),
                MotionInstance {
                    model: raw.model,
                    previous: previous.to_cols_array_2d(),
                    skin,
                    sway: raw.sway,
                    fade: draw.fade,
                },
            ));
        }
        instances.sort_by_key(|(mesh, _)| *mesh);
        Moved {
            instances,
            previous_joints,
        }
    }

    /// Keeps this frame's camera (`view_proj`, without the jitter) and
    /// `list`'s items for the next, and moves on to the other targets.
    /// `taa` says whether TAA's history now holds this frame.
    pub(super) fn finish(&mut self, list: &DrawList, view_proj: Mat4, taa: bool) {
        self.view_proj = Some(view_proj);
        self.models.clear();
        self.joints.clear();
        for item in list.items.iter().filter(|item| item.id != 0) {
            self.models.insert(item.id, item.transform);
            if let Some(skin) = &item.skin {
                self.joints.insert(item.id, JointRaw::of(skin).collect());
            }
        }
        self.valid = taa;
        self.parity = 1 - self.parity;
        self.frame += 1;
    }
}

/// Whether the prepass draws `item`: solid, not water, not terrain.
fn in_prepass(item: &DrawItem) -> bool {
    let material = &item.material;
    material.opacity >= 1.0 && material.water.is_none() && material.terrain.is_none()
}

impl TemporalPasses {
    /// Writes this frame's uniform and moved items: the camera's view
    /// `view_proj` (without the jitter) and as drawn (`drawn`), in a scene
    /// of `size`, with motion blur's `share` of a frame's motion (0 for
    /// none) at `quality`.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn prepare(
        &mut self,
        (device, queue): (&wgpu::Device, &wgpu::Queue),
        history: &History,
        (view_proj, drawn): (Mat4, Mat4),
        size: (u32, u32),
        (share, quality): (f32, MotionBlurQuality),
        moved: &Moved,
    ) {
        let (width, height) = (size.0.max(1) as f32, size.1.max(1) as f32);
        let longest = height * MAX_MOTION_BLUR;
        let uniform = TemporalUniform {
            view_proj: view_proj.to_cols_array_2d(),
            previous_view_proj: history.view_proj.unwrap_or(view_proj).to_cols_array_2d(),
            inv_view_proj: drawn.inverse().to_cols_array_2d(),
            settings: [
                f32::from(u8::from(history.valid)),
                TAA_SHARE,
                NEAR_PLANE,
                FAR_PLANE,
            ],
            size: [width, height, 1.0 / width, 1.0 / height],
            blur: [
                share,
                longest,
                quality.samples() as f32,
                blur_tile(size.1) as f32,
            ],
            noise: [(history.frame % 64) as f32, 0.0, 0.0, 0.0],
        };
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&uniform));
        if moved.instances.len() > self.instance_capacity {
            self.instance_capacity = moved.instances.len().next_power_of_two();
            self.instances = instance_buffer(device, self.instance_capacity);
        }
        let instances: Vec<MotionInstance> = moved
            .instances
            .iter()
            .map(|(_, instance)| *instance)
            .collect();
        if !instances.is_empty() {
            queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(&instances));
        }
        if moved.previous_joints.len() > self.joint_capacity {
            self.joint_capacity = moved.previous_joints.len().next_power_of_two();
            self.previous_joints = joint_buffer(device, self.joint_capacity);
            self.motion_group = motion_group(
                device,
                &self.motion_layout,
                &self.uniform,
                &self.previous_joints,
            );
        }
        if !moved.previous_joints.is_empty() {
            queue.write_buffer(
                &self.previous_joints,
                0,
                bytemuck::cast_slice(&moved.previous_joints),
            );
        }
    }

    /// Draws the motion target: the camera's motion everywhere, then each
    /// moved item's where the prepass's `depth` shows it. `bind` sets a
    /// mesh's vertex, index and skin bindings and gives its index count, or
    /// `None` to skip it. Returns the draws made.
    pub(super) fn draw_motion(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        (targets, history): (&TemporalTargets, &History),
        (depth, frame_group, empty): (&wgpu::TextureView, &wgpu::BindGroup, &wgpu::BindGroup),
        moved: &Moved,
        mut bind: impl FnMut(&mut wgpu::RenderPass<'_>, MeshKey) -> Option<u32>,
    ) -> u32 {
        let target = &targets.motion[history.parity];
        fullscreen_pass(
            encoder,
            "camera motion",
            target,
            wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
            &self.camera_motion,
            &[&targets.camera_groups[history.parity]],
        );
        if moved.instances.is_empty() {
            return 0;
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("moving items"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                depth_slice: None,
                view: target,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            // Read only: the prepass's depth says what's in front.
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth,
                depth_ops: None,
                stencil_ops: None,
            }),
            ..Default::default()
        });
        pass.set_pipeline(&self.moving);
        pass.set_bind_group(0, frame_group, &[]);
        pass.set_bind_group(1, empty, &[]);
        pass.set_bind_group(3, &self.motion_group, &[]);
        pass.set_vertex_buffer(1, self.instances.slice(..));
        let mut draws = 0;
        let mut start = 0;
        while start < moved.instances.len() {
            let mesh = moved.instances[start].0;
            let end = start
                + moved.instances[start..]
                    .iter()
                    .take_while(|(other, _)| *other == mesh)
                    .count();
            if let Some(index_count) = bind(&mut pass, mesh) {
                pass.draw_indexed(0..index_count, 0, start as u32..end as u32);
                draws += 1;
            }
            start = end;
        }
        draws
    }

    /// TAA's and motion blur's passes, each over the scene's HDR texture
    /// `hdr` (copied back into it): TAA if `targets` has its history, motion
    /// blur if `targets` has its target.
    pub(super) fn draw_post(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        (targets, history): (&TemporalTargets, &History),
        hdr: &wgpu::Texture,
    ) {
        let parity = history.parity;
        if let Some(taa) = &targets.taa {
            fullscreen_pass(
                encoder,
                "TAA",
                &taa.views[parity],
                wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                &self.taa,
                &[&taa.groups[parity]],
            );
            encoder.copy_texture_to_texture(
                taa.history[parity].as_image_copy(),
                hdr.as_image_copy(),
                hdr.size(),
            );
        }
        if let Some(blur) = &targets.blur {
            fullscreen_pass(
                encoder,
                "motion blur tiles",
                &blur.tiles,
                wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                &self.tile_max,
                &[&blur.tile_groups[parity]],
            );
            fullscreen_pass(
                encoder,
                "motion blur",
                &blur.view,
                wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                &self.motion_blur,
                &[&blur.groups[parity]],
            );
            encoder.copy_texture_to_texture(
                blur.texture.as_image_copy(),
                hdr.as_image_copy(),
                hdr.size(),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_jitter_steps_through_places_within_half_a_pixel() {
        let mut history = History::default();
        let mut places = Vec::new();
        for _ in 0..JITTER_PHASES {
            let jitter = history.jitter();
            assert!(jitter.abs().max_element() < 0.5, "{jitter}");
            places.push(jitter);
            history.frame += 1;
        }
        // Every place is different, and then they repeat.
        for (index, place) in places.iter().enumerate() {
            assert!(
                places[..index]
                    .iter()
                    .all(|other| other.distance(*place) > 0.01)
            );
        }
        assert_eq!(history.jitter(), places[0]);
        // On average they're about the middle of the pixel.
        let mean = places.iter().copied().sum::<Vec2>() / places.len() as f32;
        assert!(mean.abs().max_element() < 0.1, "{mean}");
    }

    #[test]
    fn the_jitter_moves_the_view_by_that_share_of_a_pixel() {
        let history = History::default();
        let camera = crate::Camera {
            position: Vec3::ZERO,
            look_at: Vec3::NEG_Z,
            fov_y_degrees: 60.0,
        };
        let view_proj = super::super::lighting::camera_view_projection(&camera, 2.0);
        let point = Vec3::new(1.0, 2.0, -10.0).extend(1.0);
        let (width, height) = (200, 100);
        let pixel = |matrix: Mat4| {
            let clip = matrix * point;
            let ndc = clip.truncate().truncate() / clip.w;
            Vec2::new(
                (ndc.x + 1.0) * 0.5 * width as f32,
                (1.0 - ndc.y) * 0.5 * height as f32,
            )
        };
        let moved = pixel(history.jittered(view_proj, (width, height))) - pixel(view_proj);
        assert!(
            moved.distance(history.jitter()) < 1e-3,
            "{moved} vs {}",
            history.jitter()
        );
    }
}
