//! Light probe volumes (graphics Phase 19).
//!
//! Each volume's grid of baked probes is uploaded once into one storage
//! buffer, four `vec4`s a probe (its spherical harmonics coefficients, rgb
//! each). Surfaces inside a volume's box blend the eight probes around them
//! for their diffuse environment light, in place of the sky's
//! (`surrounding_diffuse` in lighting.wgsl). The buffer is rewritten only
//! when the volumes drawn change, and shrinks back to one blank probe while
//! there are none. With real-time GI on (Phase 20), `realtime_gi.rs`
//! writes freshly traced probes over the baked ones in place.
//!
//! With ray traced GI on (`ray_gi.rs`), the camera brings probes of its own
//! too: a fine grid around it, first in the buffer and first to be read,
//! with no bake (each starts unusable until traced). As the camera moves a
//! spacing, the grid moves with it and its probes are copied along on the
//! GPU, so only the row it moves into starts again.

use bytemuck::{Pod, Zeroable};
use glam::{IVec3, Quat, Vec3};
use wgpu::util::DeviceExt;

use crate::draw_list::LightProbeVolume;

use super::probes::{box_distance, to_box_rows};

/// The most volumes drawn at once: the nearest to the camera. Must match
/// `MAX_VOLUMES` in lighting.wgsl.
pub(super) const MAX_VOLUMES: usize = 8;

/// The most probes one volume may have, so a hand-made draw list can't ask
/// for a buffer larger than the GPU allows.
const MAX_PROBES_PER_VOLUME: usize = 64 * 64 * 64;

/// One probe as the shaders read it: four coefficients, rgb each. The w
/// of the first is 1 for a probe that's not to be used (ray_gi.wgsl).
type ProbeTexels = [[f32; 4]; 4];

/// The camera's probes (with ray traced GI): so many along X, Y and Z.
pub(super) const CAMERA_PROBES: [u32; 3] = [16, 8, 16];
/// How far apart the camera's probes stand, in studs: fine enough for a
/// room to get its own light.
pub(super) const CAMERA_SPACING: f32 = 3.0;
/// How far inside the camera's box, in studs, its light fades in.
const CAMERA_BLEND: f32 = 6.0;
/// Stands for the camera's probes among the bakes' keys.
const CAMERA_KEY: u64 = u64::MAX;
/// A probe with nothing in it yet: unusable until it's traced.
const UNTRACED: ProbeTexels = [[0.0, 0.0, 0.0, 1.0], [0.0; 4], [0.0; 4], [0.0; 4]];

/// One volume as the shaders read it. Must match `Volume` in lighting.wgsl.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
pub(super) struct VolumeRaw {
    /// Turns world space to the box's own: the rows of the matrix (see
    /// `probes::to_box_rows`).
    to_box: [[f32; 4]; 3],
    /// xyz: half the box's size, w: its blend distance.
    extent: [f32; 4],
    /// How many probes stand along the box's own X, Y and Z, then where its
    /// first probe is in the buffer.
    grid: [u32; 4],
}

/// Where a drawn volume's probes stand, to trace them again.
struct ProbeGrid {
    center: Vec3,
    rotation: Quat,
    half_size: Vec3,
    counts: [u32; 3],
}

/// Where the camera's probes stand, and what moving them needs.
struct CameraProbes {
    /// The middle of their box, in spacings.
    cell: IVec3,
    /// How many spacings the box has moved since its probes were last
    /// copied along (by `follow_camera`).
    moved: IVec3,
    /// Untraced probes, copied over the places nothing moves into.
    untraced: wgpu::Buffer,
    /// The probes as they were before a move, copied from.
    before: wgpu::Buffer,
}

impl CameraProbes {
    fn new(device: &wgpu::Device, cell: IVec3) -> Self {
        let count = camera_probe_count() as usize;
        let untraced = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("camera light probes (untraced)"),
            contents: bytemuck::cast_slice(&vec![UNTRACED; count]),
            usage: wgpu::BufferUsages::COPY_SRC,
        });
        let before = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("camera light probes (before a move)"),
            size: buffer_size(count),
            usage: wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            cell,
            moved: IVec3::ZERO,
            untraced,
            before,
        }
    }

    fn volume(&self) -> (Vec3, Vec3) {
        let counts = Vec3::from_array(CAMERA_PROBES.map(|count| count as f32));
        (
            self.cell.as_vec3() * CAMERA_SPACING,
            counts * CAMERA_SPACING * 0.5,
        )
    }
}

/// How many probes the camera brings.
fn camera_probe_count() -> u32 {
    CAMERA_PROBES.iter().product()
}

/// The spacing nearest `camera`: the middle of the camera's probes' box.
fn camera_cell(camera: Vec3) -> IVec3 {
    let cell = (camera / CAMERA_SPACING).round();
    if cell.is_finite() {
        cell.as_ivec3()
    } else {
        IVec3::ZERO
    }
}

/// The copies, as (from, to, probes), that move the camera's probes along
/// when their box moves `moved` spacings: each row of probes along X that
/// stays inside the box goes from where it stood in `before` to where it
/// stands now. The rest are left untraced.
fn moved_rows(moved: IVec3) -> Vec<(u32, u32, u32)> {
    let [nx, ny, nz] = CAMERA_PROBES.map(|count| count as i32);
    let mut rows = Vec::new();
    let x0 = (-moved.x).max(0);
    let x1 = (nx - moved.x).min(nx);
    if x1 <= x0 {
        return rows;
    }
    for z in 0..nz {
        for y in 0..ny {
            let (from_y, from_z) = (y + moved.y, z + moved.z);
            if !(0..ny).contains(&from_y) || !(0..nz).contains(&from_z) {
                continue;
            }
            let from = x0 + moved.x + nx * (from_y + ny * from_z);
            let to = x0 + nx * (y + ny * z);
            rows.push((from as u32, to as u32, (x1 - x0) as u32));
        }
    }
    rows
}

/// The probes' buffer and which volumes it holds.
pub(super) struct LightProbes {
    buffer: wgpu::Buffer,
    /// The bakes (their keys) in the buffer, in the order they're there:
    /// `CAMERA_KEY` first for the camera's probes.
    held: Vec<u64>,
    /// The camera's probes, while ray traced GI asks for them.
    camera: Option<CameraProbes>,
    /// The volumes drawn this frame, smallest box first.
    drawn: Vec<VolumeRaw>,
    /// Where their probes stand, in the same order.
    grids: Vec<ProbeGrid>,
    /// How many times the buffer has been written, for tests.
    pub(super) uploads: u32,
    /// How many times the camera's probes have been copied along, for
    /// tests.
    pub(super) moves: u32,
}

impl LightProbes {
    pub(super) fn new(device: &wgpu::Device) -> Self {
        Self {
            buffer: probe_buffer(device, 1),
            held: Vec::new(),
            camera: None,
            drawn: Vec::new(),
            grids: Vec::new(),
            uploads: 0,
            moves: 0,
        }
    }

    /// Picks the volumes to draw (the nearest `MAX_VOLUMES` to `camera`
    /// with a usable bake, and the camera's own probes first with
    /// `camera_probes`) and makes sure the buffer holds their probes,
    /// writing it only when they've changed. True when the buffer was made
    /// again, so the frame's bindings must be too.
    pub(super) fn prepare(
        &mut self,
        (device, queue): (&wgpu::Device, &wgpu::Queue),
        volumes: &[LightProbeVolume],
        camera: Vec3,
        camera_probes: bool,
    ) -> bool {
        let mut chosen: Vec<&LightProbeVolume> =
            volumes.iter().filter(|volume| usable(volume)).collect();
        let distance =
            |v: &LightProbeVolume| box_distance(v.center, v.rotation, v.half_size, camera);
        chosen.sort_by(|a, b| distance(a).total_cmp(&distance(b)));
        chosen.truncate(MAX_VOLUMES - usize::from(camera_probes));
        // The smallest first, so a small box inside a big one wins there.
        let size = |v: &LightProbeVolume| v.half_size.x * v.half_size.y * v.half_size.z;
        chosen.sort_by(|a, b| size(a).total_cmp(&size(b)));

        let keys: Vec<u64> = camera_probes
            .then_some(CAMERA_KEY)
            .into_iter()
            .chain(chosen.iter().map(|volume| volume.probes.key))
            .collect();
        let cell = camera_cell(camera);
        if !camera_probes {
            self.camera = None;
        } else if let Some(probes) = &mut self.camera
            && keys == self.held
        {
            probes.moved += cell - probes.cell;
            probes.cell = cell;
        }
        let mut rebind = false;
        if keys != self.held {
            if camera_probes {
                let probes = self
                    .camera
                    .get_or_insert_with(|| CameraProbes::new(device, cell));
                probes.cell = cell;
                probes.moved = IVec3::ZERO;
            }
            let untraced = (0..camera_probe_count())
                .filter(|_| camera_probes)
                .map(|_| UNTRACED);
            let texels: Vec<ProbeTexels> = untraced
                .chain(
                    chosen
                        .iter()
                        .flat_map(|volume| volume.probes.probes.iter())
                        .map(|probe| probe.map(|[r, g, b]| [r, g, b, 0.0])),
                )
                .collect();
            let count = texels.len().max(1);
            if self.buffer.size() != buffer_size(count) {
                self.buffer = probe_buffer(device, count);
                rebind = true;
            }
            if !texels.is_empty() {
                queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&texels));
                self.uploads += 1;
            }
            self.held = keys;
        }
        // The boxes may have moved although their bakes haven't.
        self.drawn.clear();
        self.grids.clear();
        let mut first = 0;
        if let Some(probes) = &self.camera {
            let (center, half_size) = probes.volume();
            let [x, y, z] = CAMERA_PROBES;
            self.grids.push(ProbeGrid {
                center,
                rotation: Quat::IDENTITY,
                half_size,
                counts: CAMERA_PROBES,
            });
            self.drawn.push(VolumeRaw {
                to_box: to_box_rows(center, Quat::IDENTITY),
                extent: half_size.extend(CAMERA_BLEND).to_array(),
                grid: [x, y, z, 0],
            });
            first = x * y * z;
        }
        for volume in chosen {
            let [x, y, z] = volume.probes.counts;
            self.grids.push(ProbeGrid {
                center: volume.center,
                rotation: volume.rotation,
                half_size: volume.half_size,
                counts: volume.probes.counts,
            });
            self.drawn.push(VolumeRaw {
                to_box: to_box_rows(volume.center, volume.rotation),
                extent: volume.half_size.extend(volume.blend_distance).to_array(),
                grid: [x, y, z, first],
            });
            first += x * y * z;
        }
        rebind
    }

    /// Records copying the camera's probes along into `encoder` if their
    /// box has moved since: each to its new place, and the places nothing
    /// moved into back to untraced. Before anything reads them this frame.
    pub(super) fn follow_camera(&mut self, encoder: &mut wgpu::CommandEncoder) {
        let Some(probes) = &mut self.camera else {
            return;
        };
        if probes.moved == IVec3::ZERO {
            return;
        }
        let rows = moved_rows(probes.moved);
        probes.moved = IVec3::ZERO;
        let size = buffer_size(camera_probe_count() as usize);
        if !rows.is_empty() {
            encoder.copy_buffer_to_buffer(&self.buffer, 0, &probes.before, 0, size);
        }
        encoder.copy_buffer_to_buffer(&probes.untraced, 0, &self.buffer, 0, size);
        let texels = size_of::<ProbeTexels>() as u64;
        for (from, to, count) in rows {
            encoder.copy_buffer_to_buffer(
                &probes.before,
                u64::from(from) * texels,
                &self.buffer,
                u64::from(to) * texels,
                u64::from(count) * texels,
            );
        }
        self.moves += 1;
    }

    /// The frame uniform's volumes, and how many there are.
    pub(super) fn uniform(&self) -> ([VolumeRaw; MAX_VOLUMES], u32) {
        let mut volumes = [VolumeRaw::default(); MAX_VOLUMES];
        volumes[..self.drawn.len()].copy_from_slice(&self.drawn);
        (volumes, self.drawn.len() as u32)
    }

    pub(super) fn buffer(&self) -> &wgpu::Buffer {
        &self.buffer
    }

    /// How many probes the volumes drawn this frame have.
    pub(super) fn count(&self) -> u32 {
        self.grids
            .iter()
            .map(|grid| grid.counts.iter().product::<u32>())
            .sum()
    }

    /// Where probe `index` of the buffer stands in the world, if there's
    /// one: the middle of its cell of its volume's box.
    pub(super) fn point(&self, mut index: u32) -> Option<Vec3> {
        for grid in &self.grids {
            let [x, y, z] = grid.counts;
            if index < x * y * z {
                let at = Vec3::new(
                    (index % x) as f32,
                    (index / x % y) as f32,
                    (index / (x * y)) as f32,
                );
                let cell = (at + 0.5) / Vec3::new(x as f32, y as f32, z as f32) - 0.5;
                return Some(grid.center + grid.rotation * (cell * 2.0 * grid.half_size));
            }
            index -= x * y * z;
        }
        None
    }

    /// Writes the bakes into the buffer again on the next `prepare`, over
    /// any probes traced since.
    pub(super) fn restore_bakes(&mut self) {
        self.held.clear();
    }
}

fn buffer_size(probes: usize) -> u64 {
    (probes * size_of::<ProbeTexels>()) as u64
}

/// A zeroed buffer for `probes` probes.
fn probe_buffer(device: &wgpu::Device, probes: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("light probes"),
        size: buffer_size(probes),
        // Copied from only to check it in tests.
        usage: wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::COPY_DST
            | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    })
}

/// Whether `volume` can be drawn: it has as many probes as its grid says
/// (and not too many), and its box is a real box.
fn usable(volume: &LightProbeVolume) -> bool {
    let [x, y, z] = volume.probes.counts.map(|count| count as usize);
    let total = x * y * z;
    total > 0
        && total <= MAX_PROBES_PER_VOLUME
        && volume.probes.probes.len() == total
        && volume.half_size.cmpgt(Vec3::ZERO).all()
        && volume.half_size.is_finite()
        && volume.center.is_finite()
        && volume.rotation.is_finite()
        && volume.blend_distance.is_finite()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use glam::Quat;

    use super::*;
    use crate::draw_list::LightProbeGrid;

    fn volume(counts: [u32; 3], probes: usize) -> LightProbeVolume {
        LightProbeVolume {
            center: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            half_size: Vec3::ONE,
            blend_distance: 1.0,
            probes: LightProbeGrid {
                key: 1,
                counts,
                probes: Arc::new(vec![[[0.0; 3]; 4]; probes]),
            },
        }
    }

    #[test]
    fn each_probe_stands_in_the_middle_of_its_cell_of_its_volumes_turned_box() {
        let Some((device, queue)) = crate::renderer::tests::test_device() else {
            eprintln!("skipped: no graphics adapter on this machine");
            return;
        };
        let mut small = volume([2, 1, 3], 6);
        small.center = Vec3::new(10.0, 0.0, 0.0);
        small.rotation = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
        small.half_size = Vec3::new(2.0, 1.0, 3.0);
        let mut big = volume([1, 1, 1], 1);
        big.half_size = Vec3::splat(20.0);
        big.probes.key = 2;
        let mut probes = LightProbes::new(&device);
        probes.prepare((&device, &queue), &[big, small], Vec3::ZERO, false);
        assert_eq!(probes.count(), 7);
        // The small box comes first. Its first probe is in its corner cell:
        // a quarter of the way along its own X and a sixth along its Z, so
        // 1 and 2 studs from the middle toward their - ends, then turned.
        let first = probes.point(0).unwrap();
        let turned =
            Quat::from_rotation_y(std::f32::consts::FRAC_PI_2) * Vec3::new(-1.0, 0.0, -2.0);
        assert!(
            first.abs_diff_eq(Vec3::new(10.0, 0.0, 0.0) + turned, 1e-4),
            "{first}"
        );
        assert!(
            probes.point(6).unwrap().abs_diff_eq(Vec3::ZERO, 1e-4),
            "the big box's one probe"
        );
        assert_eq!(probes.point(7), None);
    }

    #[test]
    fn moving_the_camera_probes_copies_each_row_that_stays_to_its_new_place() {
        let [nx, ny, nz] = CAMERA_PROBES;
        // One along +X: each row of 15 comes from one further along.
        let rows = moved_rows(IVec3::X);
        assert_eq!(rows.len() as u32, ny * nz);
        assert!(
            rows.iter()
                .all(|&(from, to, count)| from == to + 1 && count == nx - 1)
        );
        // One down: the bottom layer has nothing below it to come from,
        // and the rest come from the row under where they are now.
        let rows = moved_rows(-IVec3::Y);
        assert_eq!(rows.len() as u32, (ny - 1) * nz);
        assert!(
            rows.iter()
                .all(|&(from, to, count)| to == from + nx && count == nx)
        );
        // Two along -Z and one along -X together.
        let rows = moved_rows(IVec3::new(-1, 0, -2));
        assert_eq!(rows.len() as u32, ny * (nz - 2));
        assert!(
            rows.iter()
                .all(|&(from, to, count)| to == from + 1 + 2 * nx * ny && count == nx - 1)
        );
        // Further than the box: nothing stays.
        assert!(moved_rows(IVec3::new(nx as i32, 0, 0)).is_empty());
        assert!(moved_rows(IVec3::new(0, 0, -(nz as i32))).is_empty());
    }

    #[test]
    fn the_camera_probes_box_is_centred_on_the_spacing_nearest_the_camera() {
        assert_eq!(camera_cell(Vec3::new(-8.0, 3.0, 0.0)), IVec3::new(-3, 1, 0));
        assert_eq!(camera_cell(Vec3::new(1.4, -1.6, 4.6)), IVec3::new(0, -1, 2));
        assert_eq!(camera_cell(Vec3::NAN), IVec3::ZERO);
    }

    #[test]
    fn a_volume_whose_probes_dont_fill_its_grid_isnt_drawn() {
        assert!(usable(&volume([2, 1, 3], 6)));
        assert!(!usable(&volume([2, 1, 3], 5)));
        assert!(!usable(&volume([0, 1, 1], 0)));
        assert!(!usable(&volume([65, 64, 64], 65 * 64 * 64)), "too many");
        let mut flat = volume([1, 1, 1], 1);
        flat.half_size.y = 0.0;
        assert!(!usable(&flat));
    }
}
