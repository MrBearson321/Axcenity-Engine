//! Hardware ray tracing's scene (`docs/rendering/ray-tracing.md`): every
//! drawn mesh as a bottom-level acceleration structure (BLAS), and the
//! frame's items within the ray tracing distance as one top-level structure
//! (TLAS) that ray queries trace against.
//!
//! It exists only while something traces rays (a ray traced effect, or the
//! editor's Ray Hits view) on a GPU with wgpu's ray queries
//! ([`crate::Capabilities::ray_queries`]); otherwise nothing here is made.
//!
//! - A mesh's BLAS is built the first frame it's wanted, up to
//!   [`BLAS_TRIANGLES_PER_FRAME`] triangles a frame, so a big import builds
//!   over a few frames. Until it's built its items are left out. A caller's
//!   mesh whose [`MeshSource::version`](crate::MeshSource::version) changes
//!   is built again (wgpu has no refit, so every update is a full build).
//! - The TLAS keeps one slot per item in draw-list order and is rebuilt
//!   only when a slot changed (an item moved, came or went). wgpu uploads
//!   the instances from the lowest changed slot up, so a still scene costs
//!   nothing after its first frame.
//! - Posed (skinned) items are left out: their mesh is posed in the vertex
//!   shader, so a BLAS of its corners would be the rest pose.

use glam::{Mat4, Vec3};
use rustc_hash::FxHashMap;

use super::scene::gpu_mesh_in;
use super::{Draw, GpuMesh, MeshKey, ScenePipeline};
use crate::draw_list::Material;

/// Solid items that cast shadows.
pub(super) const MASK_SOLID: u8 = 1 << 0;
/// Items with cut-out textures (`alpha_cutoff`) that cast shadows.
pub(super) const MASK_CUT_OUT: u8 = 1 << 1;
/// See-through items and water.
pub(super) const MASK_SEE_THROUGH: u8 = 1 << 2;
/// Solid items with `casts_shadow` off.
pub(super) const MASK_NO_SHADOW: u8 = 1 << 3;
/// Cut-out items with `casts_shadow` off.
pub(super) const MASK_NO_SHADOW_CUT_OUT: u8 = 1 << 4;

/// The most triangles built into new BLASes in one frame. One mesh bigger
/// than this is still built whole, on a frame of its own. About 5 ms on an
/// RX 9070 XT (2.6 ms per million, `docs/research/hardware-ray-tracing.md`).
pub(super) const BLAS_TRIANGLES_PER_FRAME: u64 = 2_000_000;

/// A mesh's BLAS unused for this many frames is freed, as meshes are.
const FORGET_AFTER: u64 = 600;

/// How many items ray tracing saw in a frame, for [`crate::RenderStats`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RayTracingStats {
    /// Items in the TLAS.
    pub instances: u32,
    /// Meshes with a built BLAS.
    pub meshes: u32,
    /// Their triangles.
    pub triangles: u64,
    /// Meshes wanted but not built yet (waiting for the per-frame budget).
    pub meshes_waiting: u32,
    /// Whether the TLAS was rebuilt this frame.
    pub rebuilt: bool,
}

struct MeshBlas {
    blas: wgpu::Blas,
    size: wgpu::BlasTriangleGeometrySizeDescriptor,
    /// The mesh's version it was made for.
    version: u64,
    built: bool,
    last_used: u64,
}

/// What a ray that hits a TLAS slot's item finds, for ray traced GI
/// (`ray_gi.wgsl`'s `gi_surfaces`, two `vec4`s a slot): its color times its
/// texture's average, with 1 after it for a cut-out (a leaf card, lit from
/// either side) and 0 otherwise, then its glow, both linear light.
pub(super) type SurfaceRaw = [[f32; 4]; 2];

/// The TLAS slots' [`SurfaceRaw`]s on the GPU, kept while ray traced GI
/// asks for them.
pub(super) struct Surfaces {
    pub(super) buffer: wgpu::Buffer,
    capacity: usize,
    written: Vec<SurfaceRaw>,
    /// Each base color texture's average color (linear), by its key.
    texture_colors: FxHashMap<u64, [f32; 3]>,
}

/// One TLAS slot: what's in it, compared frame to frame.
#[derive(Clone, Copy, PartialEq)]
struct Slot {
    mesh: MeshKey,
    transform: [f32; 12],
    mask: u8,
}

/// The ray traced scene: BLASes by mesh and the TLAS.
pub(super) struct RayScene {
    blases: FxHashMap<MeshKey, MeshBlas>,
    tlas: wgpu::Tlas,
    capacity: u32,
    slots: Vec<Slot>,
    /// Whether the TLAS has been built with `slots`.
    built: bool,
    /// What rays hitting each slot find, while ray traced GI is on.
    pub(super) surfaces: Option<Surfaces>,
    pub(super) stats: RayTracingStats,
}

impl RayScene {
    pub(super) fn new(device: &wgpu::Device) -> Self {
        let capacity = 256;
        Self {
            blases: FxHashMap::default(),
            tlas: create_tlas(device, capacity),
            capacity,
            slots: Vec::new(),
            built: false,
            surfaces: None,
            stats: RayTracingStats::default(),
        }
    }

    /// The TLAS, built (this frame or before) with the frame's items.
    pub(super) fn tlas(&self) -> &wgpu::Tlas {
        &self.tlas
    }

    /// Brings the structures up to date with this frame's `draws`, recording
    /// their builds into `encoder`: new meshes' BLASes within the budget,
    /// and the TLAS if any slot changed. `reach` is the ray tracing
    /// distance from `camera`; `mesh` finds an uploaded mesh. With
    /// `surfaces`, each slot's [`SurfaceRaw`] is kept up to date too.
    pub(super) fn prepare<'a>(
        &mut self,
        (device, queue): (&wgpu::Device, &wgpu::Queue),
        encoder: &mut wgpu::CommandEncoder,
        (frame, camera, reach, surfaces): (u64, Vec3, f32, bool),
        draws: &[Draw],
        mesh: impl Fn(MeshKey) -> Option<&'a GpuMesh>,
    ) {
        if !surfaces {
            self.surfaces = None;
        }
        // The items wanted, and the meshes they need.
        let mut wanted: Vec<(MeshKey, &'a GpuMesh, Mat4, u8)> = Vec::new();
        let mut wanted_surfaces: Vec<SurfaceRaw> = Vec::new();
        for draw in draws {
            let item = draw.item;
            // The copy fading out of a level-of-detail switch, and posed
            // items (see the module's notes).
            if draw.fade > 1.0 || item.skin.is_some() {
                continue;
            }
            let key = draw.mesh();
            let Some(gpu) = mesh(key) else {
                continue;
            };
            let (center, radius) = gpu.bounds.sphere(item.transform);
            // Also leaves out an item whose bounds aren't a number.
            let within = center.distance(camera) - radius <= reach;
            if !within {
                continue;
            }
            wanted.push((key, gpu, item.transform, mask_of(&item.material, draw)));
            if surfaces {
                let table = self.surfaces.get_or_insert_with(|| Surfaces::new(device));
                wanted_surfaces.push(surface_of(&item.material, &mut table.texture_colors));
            }
        }

        // New or changed meshes get a BLAS, built within the budget.
        let mut budget = BLAS_TRIANGLES_PER_FRAME;
        let mut to_build: Vec<(MeshKey, &'a GpuMesh)> = Vec::new();
        let mut waiting = 0;
        for &(key, gpu, _, _) in &wanted {
            let blas = self.blases.entry(key);
            let entry = match blas {
                std::collections::hash_map::Entry::Occupied(entry)
                    if entry.get().version == gpu.version =>
                {
                    entry.into_mut()
                }
                blas => {
                    let size = size_of_mesh(gpu);
                    let fresh = MeshBlas {
                        blas: create_blas(device, &size),
                        size,
                        version: gpu.version,
                        built: false,
                        last_used: frame,
                    };
                    match blas {
                        std::collections::hash_map::Entry::Occupied(mut entry) => {
                            entry.insert(fresh);
                            entry.into_mut()
                        }
                        std::collections::hash_map::Entry::Vacant(entry) => entry.insert(fresh),
                    }
                }
            };
            entry.last_used = frame;
            if entry.built || to_build.iter().any(|(queued, _)| *queued == key) {
                continue;
            }
            let triangles = u64::from(gpu.index_count / 3);
            if triangles > budget && budget < BLAS_TRIANGLES_PER_FRAME {
                waiting += 1;
                continue;
            }
            budget = budget.saturating_sub(triangles);
            to_build.push((key, gpu));
        }
        for (key, _) in &to_build {
            if let Some(entry) = self.blases.get_mut(key) {
                entry.built = true;
            }
        }

        // The frame's slots: items whose BLAS is (or is being) built.
        let in_slots: Vec<usize> = (0..wanted.len())
            .filter(|&index| {
                self.blases
                    .get(&wanted[index].0)
                    .is_some_and(|entry| entry.built)
            })
            .collect();
        let slots: Vec<Slot> = in_slots
            .iter()
            .map(|&index| {
                let (mesh, _, transform, mask) = wanted[index];
                Slot {
                    mesh,
                    transform: rows_3x4(transform),
                    mask,
                }
            })
            .collect();
        if let Some(table) = &mut self.surfaces {
            let raw: Vec<SurfaceRaw> = in_slots
                .iter()
                .map(|&index| wanted_surfaces[index])
                .collect();
            table.write(device, queue, raw);
        }
        let count = slots.len() as u32;
        if count > self.capacity {
            self.capacity = count.next_power_of_two();
            self.tlas = create_tlas(device, self.capacity);
            self.slots.clear();
            self.built = false;
        }
        let changed = !self.built || slots != self.slots || !to_build.is_empty();
        if changed {
            for (index, slot) in slots.iter().enumerate() {
                // A slot whose mesh was built again needs its new BLAS.
                let rebuilt = to_build.iter().any(|(key, _)| *key == slot.mesh);
                if self.built && !rebuilt && self.slots.get(index) == Some(slot) {
                    continue;
                }
                let blas = &self.blases[&slot.mesh].blas;
                self.tlas[index] = Some(wgpu::TlasInstance::new(
                    blas,
                    slot.transform,
                    index as u32,
                    slot.mask,
                ));
            }
            for index in slots.len()..self.slots.len().min(self.capacity as usize) {
                self.tlas[index] = None;
            }
        }

        // Record the builds: the BLASes, then the TLAS that uses them.
        let entries: Vec<wgpu::BlasBuildEntry> = to_build
            .iter()
            .map(|(key, gpu)| {
                let entry = &self.blases[key];
                wgpu::BlasBuildEntry {
                    blas: &entry.blas,
                    geometry: wgpu::BlasGeometries::TriangleGeometries(vec![
                        wgpu::BlasTriangleGeometry {
                            size: &entry.size,
                            vertex_buffer: &gpu.vertices,
                            first_vertex: 0,
                            vertex_stride: size_of::<crate::mesh::Vertex>() as u64,
                            index_buffer: Some(&gpu.indices),
                            first_index: Some(0),
                            transform_buffer: None,
                            transform_buffer_offset: None,
                        },
                    ]),
                }
            })
            .collect();
        if changed || !entries.is_empty() {
            let tlas = changed.then_some(&self.tlas);
            encoder.build_acceleration_structures(entries.iter(), tlas);
        }
        drop(entries);
        self.slots = slots;
        self.built = true;

        // Meshes nothing has wanted for a while are freed.
        self.blases
            .retain(|_, entry| frame.saturating_sub(entry.last_used) < FORGET_AFTER);
        let built = self.blases.values().filter(|entry| entry.built);
        self.stats = RayTracingStats {
            instances: count,
            meshes: built.clone().count() as u32,
            triangles: built
                .map(|entry| u64::from(entry.size.index_count.unwrap_or(0) / 3))
                .sum(),
            meshes_waiting: waiting,
            rebuilt: changed,
        };
    }
}

impl ScenePipeline {
    /// Whether anything this frame traces rays on a device that can: a ray
    /// traced effect drawn this frame (`effects`: sun shadows, ambient
    /// occlusion) or the editor's Ray Hits view.
    pub(super) fn traces_rays(&self, effects: bool) -> bool {
        if !self.graphics.ray_queries {
            return false;
        }
        if effects {
            return true;
        }
        #[cfg(feature = "debug-views")]
        if self
            .debug_views
            .as_ref()
            .is_some_and(|debug| debug.view == super::DebugView::RayHits)
        {
            return true;
        }
        false
    }
}

/// Makes, updates or frees `scene` (a [`ScenePipeline`]'s) for this frame's
/// `draws`, seen from `camera`: it's freed when nothing `traces`. With
/// `surfaces` (ray traced GI), each slot's [`SurfaceRaw`] is kept too. Returns
/// whether it recorded anything (for the GPU timer's Acceleration
/// structures pass). Takes the pipeline's fields, not the pipeline, so the
/// frame's other borrows of it can stay.
pub(super) fn prepare_scene(
    scene: &mut Option<Box<RayScene>>,
    pipeline: (
        (&wgpu::Device, &wgpu::Queue),
        &super::Meshes,
        &FxHashMap<u64, super::Cached<GpuMesh>>,
    ),
    encoder: &mut wgpu::CommandEncoder,
    (traces, frame, camera, reach, surfaces): (bool, u64, Vec3, f32, bool),
    draws: &[Draw],
) -> bool {
    let ((device, queue), meshes, custom_meshes) = pipeline;
    if !traces {
        *scene = None;
        return false;
    }
    let scene = scene.get_or_insert_with(|| Box::new(RayScene::new(device)));
    scene.prepare(
        (device, queue),
        encoder,
        (frame, camera, reach, surfaces),
        draws,
        |key| gpu_mesh_in(meshes, custom_meshes, key),
    );
    scene.stats.rebuilt || scene.stats.meshes > 0
}

impl Surfaces {
    fn new(device: &wgpu::Device) -> Self {
        let capacity = 256;
        Self {
            buffer: surface_buffer(device, capacity),
            capacity,
            written: Vec::new(),
            texture_colors: FxHashMap::default(),
        }
    }

    /// Writes `raw` (one a slot) to the GPU if it changed, growing the
    /// buffer to fit.
    fn write(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, raw: Vec<SurfaceRaw>) {
        if raw.len() > self.capacity {
            self.capacity = raw.len().next_power_of_two();
            self.buffer = surface_buffer(device, self.capacity);
            self.written.clear();
        }
        if raw != self.written && !raw.is_empty() {
            queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&raw));
        }
        self.written = raw;
    }
}

fn surface_buffer(device: &wgpu::Device, slots: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("ray traced surfaces"),
        size: (slots * size_of::<SurfaceRaw>()) as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

/// What a ray hitting an item of `material` finds: its color (times its
/// base color texture's average, worked out once a texture into
/// `texture_colors`) and its glow, in linear light.
fn surface_of(material: &Material, texture_colors: &mut FxHashMap<u64, [f32; 3]>) -> SurfaceRaw {
    let mut color = super::lighting::linear_color(material.base_color);
    if let Some(texture) = &material.base_color_texture {
        let average = *texture_colors
            .entry(texture.key())
            .or_insert_with(|| average_color(texture.rgba()));
        color *= Vec3::from(average);
    }
    let glow = super::lighting::linear_color(material.emissive);
    let cut_out = if material.alpha_cutoff > 0.0 {
        1.0
    } else {
        0.0
    };
    [
        color.extend(cut_out).to_array(),
        glow.extend(0.0).to_array(),
    ]
}

/// The average of sRGB `rgba` pixels in linear light, from at most about
/// 4,096 of them spread over the picture. Mid grey if there are none.
fn average_color(rgba: &[u8]) -> [f32; 3] {
    let pixels = rgba.len() / 4;
    if pixels == 0 {
        return [0.5; 3];
    }
    let step = (pixels / 4096).max(1);
    let mut sum = Vec3::ZERO;
    let mut count = 0.0;
    for pixel in rgba.chunks_exact(4).step_by(step) {
        let srgb = [pixel[0], pixel[1], pixel[2]].map(|c| f32::from(c) / 255.0);
        sum += super::lighting::linear_color(srgb);
        count += 1.0;
    }
    (sum / count).to_array()
}

/// Which kinds of ray see an item (the instance mask): see the `MASK_*`s.
fn mask_of(material: &Material, draw: &Draw) -> u8 {
    if material.opacity < 1.0 || material.water.is_some() {
        return MASK_SEE_THROUGH;
    }
    match (draw.casts_shadow, material.alpha_cutoff > 0.0) {
        (true, true) => MASK_CUT_OUT,
        (true, false) => MASK_SOLID,
        (false, true) => MASK_NO_SHADOW_CUT_OUT,
        (false, false) => MASK_NO_SHADOW,
    }
}

/// `transform`'s top three rows, as wgpu's TLAS instances take it.
fn rows_3x4(transform: Mat4) -> [f32; 12] {
    let rows = transform.transpose().to_cols_array();
    rows[..12].try_into().unwrap()
}

fn size_of_mesh(gpu: &GpuMesh) -> wgpu::BlasTriangleGeometrySizeDescriptor {
    wgpu::BlasTriangleGeometrySizeDescriptor {
        // `Vertex::position` comes first in each corner.
        vertex_format: wgpu::VertexFormat::Float32x3,
        vertex_count: gpu.vertex_count,
        index_format: Some(gpu.index_format),
        index_count: Some(gpu.index_count),
        flags: wgpu::AccelerationStructureGeometryFlags::OPAQUE,
    }
}

fn create_blas(
    device: &wgpu::Device,
    size: &wgpu::BlasTriangleGeometrySizeDescriptor,
) -> wgpu::Blas {
    device.create_blas(
        &wgpu::CreateBlasDescriptor {
            label: Some("mesh BLAS"),
            flags: wgpu::AccelerationStructureFlags::PREFER_FAST_TRACE,
            update_mode: wgpu::AccelerationStructureUpdateMode::Build,
        },
        wgpu::BlasGeometrySizeDescriptors::Triangles {
            descriptors: vec![size.clone()],
        },
    )
}

fn create_tlas(device: &wgpu::Device, capacity: u32) -> wgpu::Tlas {
    device.create_tlas(&wgpu::CreateTlasDescriptor {
        label: Some("ray traced scene"),
        max_instances: capacity,
        flags: wgpu::AccelerationStructureFlags::PREFER_FAST_TRACE,
        update_mode: wgpu::AccelerationStructureUpdateMode::Build,
    })
}

/// Usage a mesh's buffers need to be built into a BLAS on `device`.
pub(super) fn blas_input(device: &wgpu::Device) -> wgpu::BufferUsages {
    if device
        .features()
        .contains(wgpu::Features::EXPERIMENTAL_RAY_QUERY)
    {
        wgpu::BufferUsages::BLAS_INPUT
    } else {
        wgpu::BufferUsages::empty()
    }
}

/// The editor's Ray Hits view: what the camera's rays hit in the ray traced
/// scene, each item a color of its own, its triangles alternately lighter
/// and darker, dimmer the farther away (`ray_hits.wgsl`).
#[cfg(feature = "debug-views")]
pub(super) struct RayHitsPass {
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
    uniform: wgpu::Buffer,
}

/// Must match `RayView` in ray_hits.wgsl.
#[cfg(feature = "debug-views")]
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct RayViewUniform {
    inv_view_proj: [[f32; 4]; 4],
    /// The output's width and height, the ray tracing distance, unused.
    size: [f32; 4],
}

#[cfg(feature = "debug-views")]
impl RayHitsPass {
    pub(super) fn new(device: &wgpu::Device, color_format: wgpu::TextureFormat) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ray hits"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::AccelerationStructure {
                        vertex_return: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ray hits"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let shader =
            super::gpu::shader_module(device, "ray hits", include_str!("../ray_hits.wgsl"));
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ray hits"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_ray_hits"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_ray_hits"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ray hits"),
            size: size_of::<RayViewUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            layout,
            pipeline,
            uniform,
        }
    }

    /// Draws the view over `target`, from `view_proj`.
    pub(super) fn draw(
        &self,
        (device, queue): (&wgpu::Device, &wgpu::Queue),
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        (scene, view_proj, size, reach): (&RayScene, Mat4, (u32, u32), f32),
    ) {
        let uniform = RayViewUniform {
            inv_view_proj: view_proj.inverse().to_cols_array_2d(),
            size: [size.0 as f32, size.1 as f32, reach, 0.0],
        };
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&uniform));
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ray hits"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: scene.tlas().as_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.uniform.as_entire_binding(),
                },
            ],
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("ray hits"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                depth_slice: None,
                view: target,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.draw(0..3, 0..1);
    }
}
