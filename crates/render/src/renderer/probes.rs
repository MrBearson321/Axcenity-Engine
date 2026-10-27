//! Reflection probes (graphics Phase 17).
//!
//! Each probe's bake (six faces of the world seen from the middle of its
//! box) is uploaded once, as a cubemap with every mip level, and blurred into
//! its own layer of one cubemap array the way the sky's reflections are
//! (`Environment::filter`). Surfaces inside a probe's box read it where
//! their reflection leaves the box (`reflected` in lighting.wgsl). The
//! array exists only while probes are drawn, at the reflection quality's
//! size; at Low there are none.

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};

use crate::draw_list::ReflectionProbe;
use crate::graphics::ReflectionQuality;

use super::environment::{ENVIRONMENT_FORMAT, Environment, ROUGHEST_LEVEL_SIZE, half};

/// The most probes drawn at once: the nearest to the camera. Must match
/// `MAX_PROBES` in lighting.wgsl.
pub(super) const MAX_PROBES: usize = 8;

/// One probe as the shaders read it. Must match `Probe` in lighting.wgsl.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
pub(super) struct ProbeRaw {
    /// Turns world space to the box's own (its middle at 0, its sides
    /// along the axes): the rows of the matrix. The first three numbers of
    /// each are also a box axis in world space.
    to_box: [[f32; 4]; 3],
    /// xyz: half the box's size, w: its blend distance.
    extent: [f32; 4],
    /// x: its layer of the probe array.
    layer: [u32; 4],
}

/// The probes' cubemap array and which bake each layer holds.
pub(super) struct Probes {
    maps: Option<ProbeMaps>,
    /// Bound in place of the array while there are no probes.
    blank: wgpu::TextureView,
    /// The probes drawn this frame, smallest box first.
    drawn: Vec<ProbeRaw>,
    /// How many bakes have been filtered into a layer, for tests.
    pub(super) filters: u32,
}

struct ProbeMaps {
    /// Each face's size.
    size: u32,
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    /// The bake (its key) in each layer.
    layers: Vec<Option<u64>>,
    bytes: u64,
}

impl Probes {
    pub(super) fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("no reflection probes"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 6,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: ENVIRONMENT_FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            texture.as_image_copy(),
            &[0; 8 * 6],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(8),
                rows_per_image: Some(1),
            },
            texture.size(),
        );
        Self {
            maps: None,
            blank: texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::CubeArray),
                ..Default::default()
            }),
            drawn: Vec::new(),
            filters: 0,
        }
    }

    /// Picks the probes to draw (the nearest `MAX_PROBES` to `camera` with
    /// a usable bake) and makes sure each is filtered into a layer, filtering
    /// only bakes not already there. The array is made, resized or freed to
    /// fit them at `quality`. True when it was, so the frame's bindings must
    /// be made again.
    pub(super) fn prepare(
        &mut self,
        (device, queue): (&wgpu::Device, &wgpu::Queue),
        environment: &Environment,
        probes: &[ReflectionProbe],
        camera: Vec3,
        (quality, max_texture_size): (ReflectionQuality, u32),
    ) -> bool {
        let mut chosen: Vec<&ReflectionProbe> = probes
            .iter()
            .filter(|probe| usable(probe, max_texture_size))
            .collect();
        chosen.sort_by(|a, b| distance_to(a, camera).total_cmp(&distance_to(b, camera)));
        chosen.truncate(MAX_PROBES);
        // The smallest first, so a small box inside a big one wins there.
        chosen.sort_by(|a, b| volume(a).total_cmp(&volume(b)));
        let map = quality.map().filter(|_| !chosen.is_empty());
        let wanted = map.map(|map| (map.size, chosen.len().next_power_of_two()));
        let made = self
            .maps
            .as_ref()
            .map(|maps| (maps.size, maps.layers.len()));
        let rebind = made != wanted;
        if rebind {
            self.maps = wanted.map(|(size, layers)| ProbeMaps::new(device, size, layers));
        }
        self.drawn.clear();
        let (Some(map), Some(maps)) = (map, &mut self.maps) else {
            return rebind;
        };
        let keys: Vec<u64> = chosen.iter().map(|probe| probe.image.key).collect();
        for probe in chosen {
            let key = probe.image.key;
            let layer = match maps.layers.iter().position(|held| *held == Some(key)) {
                Some(layer) => layer,
                None => {
                    // A layer holding a bake not drawn now (there is one,
                    // since there are as many layers as probes or more).
                    let layer = maps
                        .layers
                        .iter()
                        .position(|held| held.is_none_or(|held| !keys.contains(&held)))
                        .expect("a free layer");
                    let source = upload_source(device, queue, probe);
                    environment.filter(
                        device,
                        queue,
                        &source,
                        (&maps.texture, layer as u32 * 6),
                        map.samples,
                    );
                    maps.layers[layer] = Some(key);
                    self.filters += 1;
                    layer
                }
            };
            self.drawn.push(raw(probe, layer as u32));
        }
        rebind
    }

    /// The frame uniform's probes, and how many there are.
    pub(super) fn uniform(&self) -> ([ProbeRaw; MAX_PROBES], u32) {
        let mut probes = [ProbeRaw::default(); MAX_PROBES];
        probes[..self.drawn.len()].copy_from_slice(&self.drawn);
        (probes, self.drawn.len() as u32)
    }

    /// The array to bind, or a blank one.
    pub(super) fn view(&self) -> &wgpu::TextureView {
        self.maps.as_ref().map_or(&self.blank, |maps| &maps.view)
    }

    /// Bytes of the array on the GPU.
    pub(super) fn bytes(&self) -> u64 {
        self.maps.as_ref().map_or(0, |maps| maps.bytes)
    }

    /// Whether the array exists, for tests.
    #[cfg(test)]
    pub(super) fn allocated(&self) -> bool {
        self.maps.is_some()
    }
}

impl ProbeMaps {
    /// An array of `layers` cubes of `size`, with one level per roughness.
    fn new(device: &wgpu::Device, size: u32, layers: usize) -> Self {
        let levels = (size / ROUGHEST_LEVEL_SIZE).max(1).ilog2() + 1;
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("reflection probes"),
            size: wgpu::Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: 6 * layers as u32,
            },
            mip_level_count: levels,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: ENVIRONMENT_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let bytes = (0..levels)
            .map(|level| u64::from((size >> level).max(1)).pow(2) * 6 * 8)
            .sum::<u64>()
            * layers as u64;
        Self {
            size,
            view: texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::CubeArray),
                ..Default::default()
            }),
            texture,
            layers: vec![None; layers],
            bytes,
        }
    }
}

/// Whether `probe`'s bake can be drawn: it has pixels, as many as it says,
/// in a size the GPU holds, and its box is a real box.
fn usable(probe: &ReflectionProbe, max_texture_size: u32) -> bool {
    let size = probe.image.size;
    size > 0
        && size <= max_texture_size
        && probe.image.rgb.len() == (size as usize).pow(2) * 6 * 3
        && probe.half_size.cmpgt(Vec3::ZERO).all()
        && probe.half_size.is_finite()
        && probe.center.is_finite()
        && probe.rotation.is_finite()
}

/// How far `point` is from `probe`'s box, 0 inside it.
fn distance_to(probe: &ReflectionProbe, point: Vec3) -> f32 {
    let local = probe.rotation.inverse() * (point - probe.center);
    (local.abs() - probe.half_size).max(Vec3::ZERO).length()
}

fn volume(probe: &ReflectionProbe) -> f32 {
    probe.half_size.x * probe.half_size.y * probe.half_size.z
}

fn raw(probe: &ReflectionProbe, layer: u32) -> ProbeRaw {
    let to_box = (Mat4::from_quat(probe.rotation.inverse())
        * Mat4::from_translation(-probe.center))
    .transpose();
    ProbeRaw {
        to_box: [
            to_box.x_axis.to_array(),
            to_box.y_axis.to_array(),
            to_box.z_axis.to_array(),
        ],
        extent: probe.half_size.extend(probe.blend_distance).to_array(),
        layer: [layer, 0, 0, 0],
    }
}

/// `probe`'s bake as a cubemap on the GPU, its largest level filled in and
/// the levels below left for `Environment::filter` to draw.
fn upload_source(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    probe: &ReflectionProbe,
) -> wgpu::Texture {
    let size = probe.image.size;
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("reflection probe bake"),
        size: wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 6,
        },
        mip_level_count: size.ilog2() + 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: ENVIRONMENT_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let texels: Vec<u16> = probe
        .image
        .rgb
        .as_chunks::<3>()
        .0
        .iter()
        .flat_map(|&[red, green, blue]| [half(red), half(green), half(blue), half(1.0)])
        .collect();
    queue.write_texture(
        texture.as_image_copy(),
        bytemuck::cast_slice(&texels),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(size * 8),
            rows_per_image: Some(size),
        },
        wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 6,
        },
    );
    texture
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use glam::Quat;

    use super::*;
    use crate::draw_list::ProbeImage;

    fn probe(center: Vec3, half_size: Vec3) -> ReflectionProbe {
        ReflectionProbe {
            center,
            rotation: Quat::from_rotation_y(0.5),
            half_size,
            blend_distance: 1.0,
            image: ProbeImage {
                key: 1,
                size: 2,
                rgb: Arc::new(vec![0.0; 2 * 2 * 6 * 3]),
            },
        }
    }

    #[test]
    fn a_point_inside_a_box_is_no_distance_from_it() {
        let probe = probe(Vec3::new(10.0, 0.0, 0.0), Vec3::splat(2.0));
        assert_eq!(distance_to(&probe, Vec3::new(10.0, 1.0, 0.0)), 0.0);
        let far = distance_to(&probe, Vec3::new(10.0, 7.0, 0.0));
        assert!((far - 5.0).abs() < 1e-4, "{far}");
    }

    #[test]
    fn the_box_matrix_puts_its_middle_at_zero_and_keeps_its_axes() {
        let probe = probe(Vec3::new(3.0, 4.0, 5.0), Vec3::splat(1.0));
        let raw = raw(&probe, 0);
        let row = |i: usize| glam::Vec4::from_array(raw.to_box[i]);
        let point = |p: Vec3| {
            Vec3::new(
                row(0).dot(p.extend(1.0)),
                row(1).dot(p.extend(1.0)),
                row(2).dot(p.extend(1.0)),
            )
        };
        assert!(point(probe.center).length() < 1e-5);
        let along_x = probe.center + probe.rotation * Vec3::X;
        assert!(point(along_x).abs_diff_eq(Vec3::X, 1e-5));
        // Each row's first three numbers are the box's own axis in the world.
        assert!(
            row(0)
                .truncate()
                .abs_diff_eq(probe.rotation * Vec3::X, 1e-5)
        );
    }

    #[test]
    fn a_broken_bake_or_a_flat_box_isnt_drawn() {
        let good = probe(Vec3::ZERO, Vec3::ONE);
        assert!(usable(&good, 2048));
        assert!(!usable(&good, 1), "too big for the GPU");
        let mut short = good.clone();
        short.image.rgb = Arc::new(vec![0.0; 5]);
        assert!(!usable(&short, 2048));
        let flat = probe(Vec3::ZERO, Vec3::new(1.0, 0.0, 1.0));
        assert!(!usable(&flat, 2048));
    }
}
