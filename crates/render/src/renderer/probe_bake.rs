//! Baking probes: the scene drawn six times from one point, a quarter turn
//! wide along each axis, through the same pipeline as every frame, and read
//! back before post-processing, in linear light. A reflection probe
//! (graphics Phase 17) keeps the six faces; a light probe (Phase 19) keeps
//! only the light they bring from each way, as spherical harmonics.

use glam::Vec3;

use crate::Camera;

use super::environment::basis;
use super::*;

/// The faces' width and height the editor bakes reflection probes at, in
/// pixels. A bake file keeps its own size, so changing this changes only
/// new bakes.
pub const PROBE_BAKE_SIZE: u32 = 128;

/// The faces' width and height each light probe is baked at, in pixels:
/// only the light's broad direction is kept, so small faces are enough.
pub const LIGHT_PROBE_BAKE_SIZE: u32 = 16;

impl TextureRenderer {
    /// What a reflection probe at `center` sees of the scene in `list` (its
    /// sky, items, lights and shadows; not its UI or its own reflection
    /// probes): six `size` × `size` faces in cubemap order (+X, -X, +Y, -Y,
    /// +Z, -Z), each as a cubemap keeps it, red, green and blue in linear
    /// light (see [`ProbeImage::rgb`](crate::ProbeImage::rgb)). Drawn on a
    /// renderer of its own, with `settings` at a render scale of 100%.
    pub fn bake_reflection_probe(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        list: &DrawList,
        center: Vec3,
        size: u32,
        settings: &GraphicsSettings,
    ) -> Result<Vec<f32>> {
        ProbeCamera::new(device, queue, list, size, settings).faces(center)
    }

    /// The light reaching a light probe at each of `centers` from the scene
    /// in `list` (its sky, and items lit by the sun, lamps and sky: one
    /// bounce; not its UI, its probes or the sun's disc, which lights
    /// surfaces directly), each as four spherical harmonics coefficients
    /// (see [`LightProbeGrid::probes`](crate::LightProbeGrid::probes)).
    /// Each probe's six `size` × `size` faces are drawn on one renderer of
    /// its own, with `settings` at a render scale of 100%.
    pub fn bake_light_probes(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        list: &DrawList,
        centers: &[Vec3],
        size: u32,
        settings: &GraphicsSettings,
    ) -> Result<Vec<[[f32; 3]; 4]>> {
        let mut camera = ProbeCamera::new(device, queue, list, size, settings);
        camera.list.light_probe_volumes.clear();
        camera.renderer.scene.sun_disc = false;
        centers
            .iter()
            .map(|&center| Ok(cube_light(camera.size, &camera.faces(center)?)))
            .collect()
    }
}

/// A renderer of its own drawing what a probe sees.
struct ProbeCamera {
    renderer: TextureRenderer,
    /// The scene, without UI or reflection probes.
    list: DrawList,
    size: u32,
}

impl ProbeCamera {
    fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        list: &DrawList,
        size: u32,
        settings: &GraphicsSettings,
    ) -> Self {
        let size = size.max(1);
        let mut renderer = TextureRenderer::new(device, queue, size, size);
        renderer.set_graphics(&GraphicsSettings {
            render_scale: Some(1.0),
            ..settings.clone()
        });
        let mut list = list.clone();
        list.ui.clear();
        list.reflection_probes.clear();
        Self {
            renderer,
            list,
            size,
        }
    }

    /// The six faces seen from `center` (see
    /// [`TextureRenderer::bake_reflection_probe`]).
    fn faces(&mut self, center: Vec3) -> Result<Vec<f32>> {
        let size = self.size;
        let mut faces = Vec::with_capacity((size * size * 6 * 3) as usize);
        for face in 0..6 {
            self.list.camera = face_camera(center, face);
            self.renderer.render(&self.list)?;
            let pixels = read_scene(&self.renderer.scene, size);
            // A camera's picture has +X to its right as it looks; a cubemap
            // face has it to the left (cubemaps look out from inside), so
            // each row is read right to left.
            for row in pixels.chunks(size as usize) {
                for pixel in row.iter().rev() {
                    faces.extend_from_slice(pixel);
                }
            }
        }
        Ok(faces)
    }
}

/// The light reaching a point from six `size` × `size` cube faces around
/// it (`rgb` as [`TextureRenderer::bake_reflection_probe`] gives them), as
/// the first four spherical harmonics coefficients, blurred over the
/// hemisphere and divided by pi like the sky's (`environment::harmonics`):
/// what a white matte surface facing each way returns.
fn cube_light(size: u32, rgb: &[f32]) -> [[f32; 3]; 4] {
    let mut sums = [Vec3::ZERO; 4];
    let mut total = 0.0;
    for face in 0..6 {
        for y in 0..size {
            for x in 0..size {
                let (u, v) = (
                    (x as f32 + 0.5) / size as f32,
                    (y as f32 + 0.5) / size as f32,
                );
                // The share of all directions a texel covers: less toward
                // the face's corners, which are further away and slanted.
                let (s, t) = (u * 2.0 - 1.0, v * 2.0 - 1.0);
                let solid_angle = (1.0 + s * s + t * t).powf(-1.5);
                total += solid_angle;
                let at = ((face * size * size + y * size + x) * 3) as usize;
                let light = Vec3::from_slice(&rgb[at..at + 3]) * solid_angle;
                for (sum, basis) in sums.iter_mut().zip(basis(face_direction(face, u, v))) {
                    *sum += light * basis;
                }
            }
        }
    }
    // Scaled so the texels cover all directions (4 pi) exactly, then the
    // cosine lobe's share of each band, over pi: 1, then 2/3.
    let scale = 4.0 * std::f32::consts::PI / total.max(f32::MIN_POSITIVE);
    let bands = [1.0, 2.0 / 3.0, 2.0 / 3.0, 2.0 / 3.0];
    std::array::from_fn(|i| (sums[i] * scale * bands[i]).to_array())
}

/// The direction a cubemap texel at `u`, `v` (0 to 1 across, from the top
/// left) of `face` looks along: a copy of `face_direction` in
/// environment.wgsl.
fn face_direction(face: u32, u: f32, v: f32) -> Vec3 {
    let (s, t) = (u * 2.0 - 1.0, v * 2.0 - 1.0);
    match face {
        0 => Vec3::new(1.0, -t, -s),
        1 => Vec3::new(-1.0, -t, s),
        2 => Vec3::new(s, 1.0, t),
        3 => Vec3::new(s, -1.0, -t),
        4 => Vec3::new(s, -t, 1.0),
        _ => Vec3::new(-s, -t, -1.0),
    }
    .normalize()
}

/// A camera at `center` looking out through cubemap face `face`, a quarter
/// turn wide, turned so its picture mirrored left to right is the face.
/// Straight up and down look a hair toward +Z, so `look_at`'s up (+Y) is
/// never along the view and the picture's top is -Z (up) or +Z (down), as
/// the faces have it.
fn face_camera(center: Vec3, face: u32) -> Camera {
    const NUDGE: f32 = 1e-4;
    let forward = match face {
        0 => Vec3::X,
        1 => Vec3::NEG_X,
        2 => Vec3::new(0.0, 1.0, NUDGE),
        3 => Vec3::new(0.0, -1.0, NUDGE),
        4 => Vec3::Z,
        _ => Vec3::NEG_Z,
    };
    Camera {
        position: center,
        look_at: center + forward,
        fov_y_degrees: 90.0,
    }
}

/// The scene's HDR picture from the last frame `scene` drew, `size` pixels
/// square: red, green and blue for each pixel, rows from the top.
fn read_scene(scene: &ScenePipeline, size: u32) -> Vec<[f32; 3]> {
    let texture = &scene.frame_targets.hdr;
    let device = &scene.device;
    let row_bytes = (size * 8).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("probe face read back"),
        size: u64::from(row_bytes * size),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row_bytes),
                rows_per_image: None,
            },
        },
        wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
    );
    scene.queue.submit([encoder.finish()]);
    buffer.slice(..).map_async(wgpu::MapMode::Read, |result| {
        result.expect("could not read the probe face back");
    });
    device.poll(wgpu::PollType::Wait).unwrap();
    let mapped = buffer.slice(..).get_mapped_range();
    mapped
        .chunks(row_bytes as usize)
        .flat_map(|row| row[..(size * 8) as usize].chunks(8))
        .map(|texel| {
            let channel = |at: usize| float_of_half(u16::from_le_bytes([texel[at], texel[at + 1]]));
            [channel(0), channel(2), channel(4)]
        })
        .collect()
}

/// The 16-bit float whose bits are `bits`, as an `f32`. Light is never
/// below 0 or missing, so those become 0.
fn float_of_half(bits: u16) -> f32 {
    let exponent = u32::from((bits >> 10) & 0x1f);
    let mantissa = u32::from(bits & 0x3ff);
    let value = match exponent {
        // Too small for the usual form: mantissa × 2⁻²⁴.
        0 => mantissa as f32 / (1 << 24) as f32,
        0x1f => return if mantissa == 0 { 65504.0 } else { 0.0 },
        _ => f32::from_bits(((exponent + 127 - 15) << 23) | (mantissa << 13)),
    };
    if bits & 0x8000 != 0 { 0.0 } else { value }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What a white matte surface facing `n` returns, as lighting.wgsl's
    /// `surrounding_diffuse` works it out from one probe.
    fn irradiance(light: &[[f32; 3]; 4], n: Vec3) -> Vec3 {
        let [c0, c1, c2, c3] = light.map(Vec3::from_array);
        (c0 * 0.282095 + (c1 * n.y + c2 * n.z + c3 * n.x) * 0.488603).max(Vec3::ZERO)
    }

    /// Six faces of `size`, each texel the light `light` gives its
    /// direction.
    fn cube(size: u32, light: impl Fn(Vec3) -> Vec3) -> Vec<f32> {
        let mut rgb = Vec::new();
        for face in 0..6 {
            for y in 0..size {
                for x in 0..size {
                    let u = (x as f32 + 0.5) / size as f32;
                    let v = (y as f32 + 0.5) / size as f32;
                    rgb.extend_from_slice(&light(face_direction(face, u, v)).to_array());
                }
            }
        }
        rgb
    }

    #[test]
    fn even_light_all_round_lights_every_surface_with_its_own_color() {
        let color = Vec3::new(0.2, 0.5, 1.5);
        let size = LIGHT_PROBE_BAKE_SIZE;
        let light = cube_light(size, &cube(size, |_| color));
        for n in [
            Vec3::Y,
            Vec3::NEG_Y,
            Vec3::X,
            Vec3::NEG_Z,
            Vec3::ONE.normalize(),
        ] {
            let got = irradiance(&light, n);
            assert!((got - color).abs().max_element() < 0.01, "{n}: {got}");
        }
    }

    #[test]
    fn light_from_one_side_lights_the_surfaces_facing_it() {
        // Only the half of the world toward +X is lit: a surface facing +X
        // sees all of it, one facing -X none (as far as four coefficients
        // can tell), and one facing up half.
        let lit = |d: Vec3| Vec3::splat(f32::from(u8::from(d.x > 0.0)));
        let light = cube_light(16, &cube(16, lit));
        let facing = irradiance(&light, Vec3::X).x;
        let away = irradiance(&light, Vec3::NEG_X).x;
        let up = irradiance(&light, Vec3::Y).x;
        assert!((facing - 1.0).abs() < 0.15, "{facing}");
        assert!(away < 0.15, "{away}");
        assert!((up - 0.5).abs() < 0.02, "{up}");
    }

    #[test]
    fn each_face_camera_mirrored_sees_its_cubemap_face() {
        let center = Vec3::new(3.0, -2.0, 7.0);
        for face in 0..6 {
            let view_proj = camera_view_projection(&face_camera(center, face), 1.0);
            for (u, v) in [(0.5, 0.5), (0.2, 0.3), (0.9, 0.1), (0.4, 0.85)] {
                let clip = view_proj * (center + face_direction(face, u, v)).extend(1.0);
                let ndc = clip.truncate() / clip.w;
                // Where the camera puts it: x left to right, y top to bottom.
                let (x, y) = ((ndc.x + 1.0) / 2.0, (1.0 - ndc.y) / 2.0);
                assert!(
                    (x - (1.0 - u)).abs() < 1e-3 && (y - v).abs() < 1e-3,
                    "face {face} at ({u}, {v}) drew at ({x}, {y})"
                );
            }
        }
    }

    #[test]
    fn halves_read_back_as_floats() {
        let half = super::super::environment::half;
        for value in [0.0, 1.0, 0.5, 2.0, 0.1, 37.25, 65504.0] {
            let back = float_of_half(half(value));
            assert!(
                (back - value).abs() <= value * 1e-3,
                "{value} came back {back}"
            );
        }
        assert_eq!(float_of_half(0xbc00), 0.0, "below 0");
        assert_eq!(float_of_half(0x7e00), 0.0, "not a number");
        assert_eq!(float_of_half(0x0001), 1.0 / (1 << 24) as f32);
    }
}
