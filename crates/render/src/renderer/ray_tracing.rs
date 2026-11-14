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
//! - Posed (skinned) items are posed again in a compute pass each frame
//!   into a copy of their own, built into a BLAS of their own
//!   (`ray_sway.rs`, `ray_skin.wgsl`), within a budget, nearest first; one
//!   past the budget is left out (its mesh's rest pose would be wrong).
//! - Swaying plants in a wind get a bent copy of their mesh each frame, a
//!   BLAS of their own (`ray_sway.rs`), within a budget, nearest first.

use glam::{Mat4, Vec3};
use rustc_hash::FxHashMap;

use super::ray_cut_outs::{CutOutRaw, CutOuts, has_holes};
use super::ray_sway::{BENT_TRIANGLES, BentKey, BentMeshes, POSED_TRIANGLES, bending, bent_raw};
use super::scene::gpu_mesh_in;
use super::{Draw, GpuMesh, MeshKey, ScenePipeline};
use crate::draw_list::{DrawItem, Material};

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
    /// Paths a pixel the editor's Path Traced view has averaged so far (0
    /// while it isn't shown).
    pub path_traced_paths: u32,
    /// Swaying items bent by the wind into BLASes of their own this frame.
    pub bent: u32,
    /// Posed (skinned) characters moved into BLASes of their own this
    /// frame.
    pub posed: u32,
    /// Posed characters in reach left out for the per-frame budget.
    pub posed_left_out: u32,
    /// How far ray traced sun shadows reach this frame, in studs (0 while
    /// they're off): the ray tracing distance or the world's
    /// `ShadowDistance`, whichever is nearer.
    pub shadow_reach: u32,
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

/// A mesh's BLAS: the mesh, and whether it's built for a cut-out (without
/// the opaque flag, so rays can pass through its holes). A mesh drawn both
/// ways has one of each.
type BlasKey = (MeshKey, bool);

/// An item wanted in the ray traced scene: its BLAS, mesh, transform and
/// instance mask.
type Wanted<'a> = (BlasKey, &'a GpuMesh, Mat4, u8);

/// One TLAS slot: what's in it, compared frame to frame.
#[derive(Clone, Copy, PartialEq)]
struct Slot {
    mesh: BlasKey,
    transform: [f32; 12],
    mask: u8,
    /// Its bent copy, for a swaying item in the wind.
    bent: Option<BentKey>,
}

/// The ray traced scene: BLASes by mesh and the TLAS.
pub(super) struct RayScene {
    blases: FxHashMap<BlasKey, MeshBlas>,
    tlas: wgpu::Tlas,
    capacity: u32,
    slots: Vec<Slot>,
    /// Whether the TLAS has been built with `slots`.
    built: bool,
    /// What rays hitting each slot find, while ray traced GI is on.
    pub(super) surfaces: Option<Surfaces>,
    /// Where each slot's holes are, for rays that pass through them.
    pub(super) cut_outs: CutOuts,
    /// Swaying items' bent copies, made the first time the wind blows.
    bent: Option<BentMeshes>,
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
            cut_outs: CutOuts::new(device),
            bent: None,
            stats: RayTracingStats::default(),
        }
    }

    /// The TLAS, built (this frame or before) with the frame's items.
    pub(super) fn tlas(&self) -> &wgpu::Tlas {
        &self.tlas
    }

    /// Each TLAS slot's mesh, in slot order.
    #[cfg(feature = "debug-views")]
    pub(super) fn slot_meshes(&self) -> impl Iterator<Item = MeshKey> + '_ {
        self.slots.iter().map(|slot| slot.mesh.0)
    }

    /// How many TLAS slots are filled.
    #[cfg(feature = "debug-views")]
    pub(super) fn slot_count(&self) -> usize {
        self.slots.len()
    }

    /// Brings the structures up to date with this frame's `draws`, recording
    /// their builds into `encoder`: new meshes' BLASes within the budget,
    /// and the TLAS if any slot changed. `reach` is the ray tracing
    /// distance from `camera`; `mesh` finds an uploaded mesh. With
    /// `surfaces`, each slot's [`SurfaceRaw`] is kept up to date too.
    /// `clock` is the frame's time and wind (`Frame::clock`), which bends
    /// swaying items.
    pub(super) fn prepare<'a>(
        &mut self,
        (device, queue): (&wgpu::Device, &wgpu::Queue),
        encoder: &mut wgpu::CommandEncoder,
        (frame, camera, reach, surfaces, clock): (u64, Vec3, f32, bool, [f32; 4]),
        draws: &[Draw],
        mesh: impl Fn(MeshKey) -> Option<&'a GpuMesh>,
    ) {
        if !surfaces {
            self.surfaces = None;
        }
        // The items wanted, and the meshes they need.
        let mut wanted: Vec<Wanted<'a>> = Vec::new();
        let mut wanted_items = Vec::new();
        // Each wanted item's distance, for bending the nearest first.
        let mut wanted_distances = Vec::new();
        let mut wanted_surfaces: Vec<SurfaceRaw> = Vec::new();
        for draw in draws {
            let item = draw.item;
            // The copy fading out of a level-of-detail switch.
            if draw.fade > 1.0 {
                continue;
            }
            let key = draw.mesh();
            let Some(gpu) = mesh(key) else {
                continue;
            };
            // A posed item is bounded by where its joints move it.
            let bounds = match (&item.skin, &gpu.skin) {
                (Some(skin), Some(gpu_skin)) => gpu_skin.bounds(skin),
                _ => gpu.bounds,
            };
            let (center, radius) = bounds.sphere(item.transform);
            // Also leaves out an item whose bounds aren't a number.
            let within = center.distance(camera) - radius <= reach;
            if !within {
                continue;
            }
            let cut_out = has_holes(&item.material);
            wanted.push((
                (key, cut_out),
                gpu,
                item.transform,
                mask_of(&item.material, draw),
            ));
            wanted_items.push(item);
            wanted_distances.push(center.distance(camera) - radius);
            if surfaces {
                let table = self.surfaces.get_or_insert_with(|| Surfaces::new(device));
                wanted_surfaces.push(surface_of(&item.material, &mut table.texture_colors));
            }
        }

        // New or changed meshes get a BLAS, built within the budget.
        let mut budget = BLAS_TRIANGLES_PER_FRAME;
        let mut to_build: Vec<(BlasKey, &'a GpuMesh)> = Vec::new();
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
                    let size = size_of_mesh(gpu, key.1);
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
        for (key, gpu) in &to_build {
            if let Some(entry) = self.blases.get_mut(key) {
                entry.built = true;
            }
            // A cut-out mesh's texture coordinates, for its holes.
            if key.1 {
                self.cut_outs.add_mesh(device, encoder, key.0, gpu);
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
        let (in_slots, bent, posed_left_out) = self.bend(
            (device, queue),
            in_slots,
            (&wanted, &wanted_items, &wanted_distances),
            clock,
            frame,
        );
        let slots: Vec<Slot> = in_slots
            .iter()
            .zip(&bent)
            .map(|(&index, &bent)| {
                let (mesh, _, transform, mask) = wanted[index];
                Slot {
                    mesh,
                    transform: rows_3x4(transform),
                    mask,
                    bent,
                }
            })
            .collect();
        let cut_outs: Vec<CutOutRaw> = in_slots
            .iter()
            .map(|&index| {
                let ((mesh, cut_out), gpu, _, _) = wanted[index];
                if cut_out {
                    self.cut_outs
                        .slot((device, queue), wanted_items[index], mesh, gpu.version)
                } else {
                    CutOutRaw::default()
                }
            })
            .collect();
        self.cut_outs.write(device, queue, cut_outs);
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
        // A bent copy is built afresh each frame, so the TLAS is too.
        let any_bent = bent.iter().any(Option::is_some);
        let changed = !self.built || slots != self.slots || !to_build.is_empty() || any_bent;
        if changed {
            for (index, slot) in slots.iter().enumerate() {
                // A slot whose mesh was built again needs its new BLAS.
                let rebuilt = to_build.iter().any(|(key, _)| *key == slot.mesh);
                // A bent slot is set again too: its copy may be new.
                if self.built
                    && !rebuilt
                    && slot.bent.is_none()
                    && self.slots.get(index) == Some(slot)
                {
                    continue;
                }
                let blas = match (&slot.bent, &self.bent) {
                    (Some(key), Some(bent)) => bent.blas(key),
                    _ => &self.blases[&slot.mesh].blas,
                };
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

        // Record the builds: the swaying items' bending, the BLASes, then
        // the TLAS that uses them.
        if let Some(bent_meshes) = &self.bent {
            bent_meshes.record(encoder);
        }
        let bent_entries = in_slots.iter().zip(&bent).filter_map(|(&index, key)| {
            let key = key.as_ref()?;
            Some(self.bent.as_ref()?.build_entry(key, wanted[index].1))
        });
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
            .chain(bent_entries)
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
        if let Some(bent) = &mut self.bent {
            bent.forget_unused(frame);
        }
        let built = self.blases.values().filter(|entry| entry.built);
        self.stats = RayTracingStats {
            instances: count,
            meshes: built.clone().count() as u32,
            triangles: built
                .map(|entry| u64::from(entry.size.index_count.unwrap_or(0) / 3))
                .sum(),
            meshes_waiting: waiting,
            rebuilt: changed,
            path_traced_paths: 0,
            bent: self.bent.as_ref().map_or(0, BentMeshes::bent_count),
            posed: self.bent.as_ref().map_or(0, BentMeshes::posed_count),
            posed_left_out,
            shadow_reach: 0,
        };
    }

    /// Poses this frame's skinned items and bends its swaying ones in the
    /// wind of `clock`, nearest first within [`POSED_TRIANGLES`] and
    /// [`BENT_TRIANGLES`]. Returns `in_slots` (indices into the wanted
    /// items) without the posed items past the budget, each one's bent or
    /// posed copy (`None` to keep its mesh's rest pose), and how many posed
    /// items were left out.
    fn bend(
        &mut self,
        (device, queue): (&wgpu::Device, &wgpu::Queue),
        in_slots: Vec<usize>,
        (wanted, items, distances): (&[Wanted<'_>], &[&DrawItem], &[f32]),
        clock: [f32; 4],
        frame: u64,
    ) -> (Vec<usize>, Vec<Option<BentKey>>, u32) {
        let mut bent = vec![None; in_slots.len()];
        // Posed items first (a skinned mesh never also sways here), then
        // swaying ones, each nearest first.
        let posed_mesh = |index: usize| {
            let gpu = wanted[index].1;
            items[index].skin.as_ref().zip(gpu.skin.as_ref())
        };
        let mut posing: Vec<usize> = (0..in_slots.len())
            .filter(|&slot| posed_mesh(in_slots[slot]).is_some())
            .collect();
        let mut swaying: Vec<(usize, [f32; 4])> = in_slots
            .iter()
            .enumerate()
            .filter(|&(_, &index)| posed_mesh(index).is_none())
            .filter_map(|(slot, &index)| Some((slot, bending(items[index], clock)?)))
            .collect();
        if swaying.is_empty() && posing.is_empty() {
            if let Some(meshes) = &mut self.bent {
                meshes.begin();
            }
            return (in_slots, bent, 0);
        }
        let nearest =
            |a: &usize, b: &usize| distances[in_slots[*a]].total_cmp(&distances[in_slots[*b]]);
        posing.sort_by(nearest);
        swaying.sort_by(|a, b| nearest(&a.0, &b.0));
        let meshes = self.bent.get_or_insert_with(|| BentMeshes::new(device));
        meshes.begin();
        let mut copies: FxHashMap<BlasKey, u32> = FxHashMap::default();
        let mut copy_of = |blas_key: BlasKey| {
            let copy = copies.entry(blas_key).or_default();
            let key = (blas_key.0, blas_key.1, *copy);
            *copy += 1;
            key
        };

        let mut budget = POSED_TRIANGLES;
        let mut left_out = vec![false; in_slots.len()];
        let mut posed_left_out = 0;
        for slot in posing {
            let index = in_slots[slot];
            let (blas_key, gpu, _, _) = wanted[index];
            let triangles = u64::from(gpu.index_count / 3);
            let Some((skin, gpu_skin)) = posed_mesh(index) else {
                continue;
            };
            if triangles > budget {
                left_out[slot] = true;
                posed_left_out += 1;
                continue;
            }
            budget -= triangles;
            let key = copy_of(blas_key);
            let joints: Vec<[[f32; 4]; 4]> =
                skin.joints().iter().map(Mat4::to_cols_array_2d).collect();
            meshes.pose(
                (device, queue),
                key,
                gpu,
                &gpu_skin.buffer,
                size_of_mesh(gpu, blas_key.1),
                &joints,
                frame,
            );
            bent[slot] = Some(key);
        }

        let mut budget = BENT_TRIANGLES;
        for (slot, sway) in swaying {
            let index = in_slots[slot];
            let (blas_key, gpu, transform, _) = wanted[index];
            let triangles = u64::from(gpu.index_count / 3);
            if triangles > budget {
                continue;
            }
            budget -= triangles;
            let key = copy_of(blas_key);
            let raw = bent_raw(transform, sway, clock, gpu);
            meshes.bend(
                (device, queue),
                key,
                gpu,
                size_of_mesh(gpu, blas_key.1),
                &raw,
                frame,
            );
            bent[slot] = Some(key);
        }

        if posed_left_out == 0 {
            return (in_slots, bent, 0);
        }
        let (kept_slots, kept_bent) = in_slots
            .into_iter()
            .zip(bent)
            .zip(left_out)
            .filter(|(_, out)| !out)
            .map(|(kept, _)| kept)
            .unzip();
        (kept_slots, kept_bent, posed_left_out)
    }
}

impl ScenePipeline {
    /// Whether anything this frame traces rays on a device that can: a ray
    /// traced effect drawn this frame (`effects`: sun shadows, ambient
    /// occlusion) or the editor's Ray Hits or Path Traced view.
    pub(super) fn traces_rays(&self, effects: bool) -> bool {
        if !self.graphics.ray_queries {
            return false;
        }
        effects || self.ray_traced_view(false)
    }

    /// Whether the editor's Path Traced view is shown, which needs each
    /// slot's [`SurfaceRaw`].
    pub(super) fn path_traced(&self) -> bool {
        self.graphics.ray_queries && self.ray_traced_view(true)
    }

    /// Whether the editor shows the Path Traced view, or (unless
    /// `path_traced_only`) the Ray Hits view.
    #[cfg(feature = "debug-views")]
    fn ray_traced_view(&self, path_traced_only: bool) -> bool {
        use super::DebugView;
        self.debug_views.as_ref().is_some_and(|debug| {
            debug.view == DebugView::PathTraced
                || (!path_traced_only && debug.view == DebugView::RayHits)
        })
    }

    #[cfg(not(feature = "debug-views"))]
    fn ray_traced_view(&self, _path_traced_only: bool) -> bool {
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
    (traces, frame, camera, reach, surfaces, clock): (bool, u64, Vec3, f32, bool, [f32; 4]),
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
        (frame, camera, reach, surfaces, clock),
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

/// A BLAS's size for `gpu`: opaque, unless it's for a `cut_out`, whose
/// triangles rays that don't force opacity see as candidates to test
/// against the texture's holes.
fn size_of_mesh(gpu: &GpuMesh, cut_out: bool) -> wgpu::BlasTriangleGeometrySizeDescriptor {
    wgpu::BlasTriangleGeometrySizeDescriptor {
        // `Vertex::position` comes first in each corner.
        vertex_format: wgpu::VertexFormat::Float32x3,
        vertex_count: gpu.vertex_count,
        index_format: Some(gpu.index_format),
        index_count: Some(gpu.index_count),
        flags: if cut_out {
            wgpu::AccelerationStructureGeometryFlags::empty()
        } else {
            wgpu::AccelerationStructureGeometryFlags::OPAQUE
        },
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

/// Usage a mesh's buffers need to be built into a BLAS on `device`, and
/// read (as storage) for a cut-out's texture coordinates
/// (`ray_cut_outs.rs`).
pub(super) fn blas_input(device: &wgpu::Device) -> wgpu::BufferUsages {
    if device
        .features()
        .contains(wgpu::Features::EXPERIMENTAL_RAY_QUERY)
    {
        wgpu::BufferUsages::BLAS_INPUT | wgpu::BufferUsages::STORAGE
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
