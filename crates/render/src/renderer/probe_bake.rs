//! Baking a reflection probe (graphics Phase 17): the scene drawn six times
//! from one point, a quarter turn wide along each axis, through the same
//! pipeline as every frame, and read back before post-processing, in
//! linear light.

use glam::Vec3;

use crate::Camera;

use super::*;

/// The faces' width and height the editor bakes at, in pixels. A bake file
/// keeps its own size, so changing this changes only new bakes.
pub const PROBE_BAKE_SIZE: u32 = 128;

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
        let size = size.max(1);
        let mut renderer = TextureRenderer::new(device, queue, size, size);
        renderer.set_graphics(&GraphicsSettings {
            render_scale: Some(1.0),
            ..settings.clone()
        });
        let mut face_list = list.clone();
        face_list.ui.clear();
        face_list.reflection_probes.clear();
        let mut faces = Vec::with_capacity((size * size * 6 * 3) as usize);
        for face in 0..6 {
            face_list.camera = face_camera(center, face);
            renderer.render(&face_list)?;
            let pixels = read_scene(&renderer.scene, size);
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

    /// The direction a cubemap texel looks along: a copy of
    /// `face_direction` in environment.wgsl.
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
