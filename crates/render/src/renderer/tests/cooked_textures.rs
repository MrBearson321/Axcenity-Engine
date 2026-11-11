//! Cooked textures (roadmap L2): BC7 levels (and normal maps' BC5) made
//! ahead go up as they are, draw like their RGBA original, take a quarter
//! of the memory, shrink for the texture budget, and fall back to RGBA on a
//! GPU without BC compression.

use super::*;

/// One 4×4 block of BC7 (mode 6) holding sRGB (200, 60, 30), opaque, as
/// Intel's compressor encodes it.
const BLOCK: [u8; 16] = [
    8, 144, 201, 100, 178, 199, 227, 241, 60, 30, 143, 7, 172, 170, 170, 170,
];
const COLOR: [u8; 4] = [200, 60, 30, 255];

/// A square texture of one color with its BC7 levels, or without them.
struct Cooked {
    key: u64,
    size: u32,
    rgba: Vec<u8>,
    levels: Vec<Vec<u8>>,
}

impl Cooked {
    fn texture(key: u64, size: u32, cooked: bool) -> Arc<dyn TextureSource> {
        let count = 32 - size.leading_zeros();
        let levels = (0..count)
            .map(|level| {
                let side = (size >> level).max(1).div_ceil(4) as usize;
                BLOCK.repeat(side * side)
            })
            .filter(|_| cooked)
            .collect();
        Arc::new(Cooked {
            key,
            size,
            rgba: COLOR.repeat((size * size) as usize),
            levels,
        })
    }
}

impl TextureSource for Cooked {
    fn key(&self) -> u64 {
        self.key
    }
    fn size(&self) -> (u32, u32) {
        (self.size, self.size)
    }
    fn rgba(&self) -> &[u8] {
        &self.rgba
    }
    fn bc7_levels(&self) -> &[Vec<u8>] {
        &self.levels
    }
}

/// A test device that reads BC7, or `None` without one.
fn bc7_device() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = crate::adapters::instance();
    let adapter = pollster::block_on(crate::adapters::adapter(&instance, None)).ok()?;
    if !adapter
        .features()
        .contains(wgpu::Features::TEXTURE_COMPRESSION_BC)
    {
        return None;
    }
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_features: wgpu::Features::TEXTURE_COMPRESSION_BC,
        ..Default::default()
    }))
    .ok()
}

fn lit_block(texture: Arc<dyn TextureSource>) -> DrawList {
    let mut block = textured_block(texture);
    block.transform = Mat4::from_scale_rotation_translation(
        Vec3::splat(4.0),
        glam::Quat::IDENTITY,
        Vec3::new(0.0, 0.0, -8.0),
    );
    let mut list = list_of(vec![block]);
    list.ambient.intensity = 1.0;
    list
}

fn texels(renderer: &TextureRenderer, key: u64) -> Texels {
    renderer.scene.textures[&key].gpu.as_ref().unwrap().texels
}

fn largest_difference(a: &[u8], b: &[u8]) -> u8 {
    a.iter().zip(b).map(|(a, b)| a.abs_diff(*b)).max().unwrap()
}

#[test]
fn a_cooked_texture_draws_like_its_rgba_original_in_a_quarter_of_the_memory() {
    let Some((device, queue)) = bc7_device() else {
        eprintln!("skipped: no graphics adapter with BC7 on this machine");
        return;
    };
    let mut plain = TextureRenderer::new(&device, &queue, 64, 64);
    plain
        .render(&lit_block(Cooked::texture(1, 256, false)))
        .unwrap();
    let mut cooked = TextureRenderer::new(&device, &queue, 64, 64);
    cooked
        .render(&lit_block(Cooked::texture(1, 256, true)))
        .unwrap();
    assert_eq!(texels(&plain, 1), Texels::Rgba8);
    assert_eq!(texels(&cooked, 1), Texels::Bc7);

    let difference = largest_difference(&plain.read_pixels(), &cooked.read_pixels());
    assert!(difference <= 3, "pixels differ by up to {difference}");
    let (plain_bytes, cooked_bytes) = (
        plain.scene.textures[&1].gpu.as_ref().unwrap().bytes.bytes(),
        cooked.scene.textures[&1]
            .gpu
            .as_ref()
            .unwrap()
            .bytes
            .bytes(),
    );
    assert!(
        cooked_bytes * 4 <= plain_bytes + 4 * 64,
        "{cooked_bytes} vs {plain_bytes}"
    );
}

#[test]
fn a_cooked_texture_is_sized_by_the_texture_quality_and_shrunk_by_the_budget() {
    let Some((device, queue)) = bc7_device() else {
        eprintln!("skipped: no graphics adapter with BC7 on this machine");
        return;
    };
    let list = lit_block(Cooked::texture(7, 4096, true));
    let mut full = TextureRenderer::new(&device, &queue, 64, 64);
    full.render(&list).unwrap();
    let mut renderer = TextureRenderer::new(&device, &queue, 64, 64);
    // Less than its 21 MiB (the smallest budget there is): it loses
    // levels by GPU copies of whole blocks (copying a part block would fail
    // validation).
    renderer.set_graphics(&GraphicsSettings {
        texture_budget: Some(16),
        ..GraphicsSettings::default()
    });
    for _ in 0..2 {
        renderer.render(&list).unwrap();
    }
    let gpu = renderer.scene.textures[&7].gpu.as_ref().unwrap();
    assert_eq!(gpu.texels, Texels::Bc7);
    assert!(gpu.dropped > 0);
    assert!(renderer.stats().texture_bytes <= 16 * 1024 * 1024);
    let difference = largest_difference(&renderer.read_pixels(), &full.read_pixels());
    assert!(difference <= 3, "pixels differ by up to {difference}");
}

#[test]
fn a_gpu_without_bc7_draws_a_cooked_texture_from_its_pixels() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    // The default device asks for no features, so it reads no BC7.
    let mut renderer = TextureRenderer::new(&device, &queue, 64, 64);
    renderer
        .render(&lit_block(Cooked::texture(3, 64, true)))
        .unwrap();
    assert_eq!(texels(&renderer, 3), Texels::Rgba8);
}

#[test]
fn levels_that_dont_match_the_size_are_ignored() {
    let Some((device, queue)) = bc7_device() else {
        eprintln!("skipped: no graphics adapter with BC7 on this machine");
        return;
    };
    let mut wrong = Cooked {
        key: 4,
        size: 64,
        rgba: COLOR.repeat(64 * 64),
        levels: vec![BLOCK.to_vec(); 7],
    };
    wrong.levels[0].pop();
    let mut renderer = TextureRenderer::new(&device, &queue, 64, 64);
    renderer.render(&lit_block(Arc::new(wrong))).unwrap();
    assert_eq!(texels(&renderer, 4), Texels::Rgba8);
}

/// A normal map tipped toward +X by about 37°: (0.6, 0, 0.8).
const TIPPED: [u8; 4] = [204, 128, 230, 255];

/// A square normal map of [`TIPPED`], with its BC5 levels (red and green:
/// two BC4 blocks, each with both ends at the value), or without them.
struct CookedNormal {
    key: u64,
    size: u32,
    rgba: Vec<u8>,
    levels: Vec<Vec<u8>>,
}

impl CookedNormal {
    fn texture(key: u64, size: u32, cooked: bool) -> Arc<dyn TextureSource> {
        let block = [
            [TIPPED[0], TIPPED[0], 0, 0, 0, 0, 0, 0],
            [TIPPED[1], TIPPED[1], 0, 0, 0, 0, 0, 0],
        ]
        .concat();
        let count = 32 - size.leading_zeros();
        let levels = (0..count)
            .map(|level| {
                let side = (size >> level).max(1).div_ceil(4) as usize;
                block.repeat(side * side)
            })
            .filter(|_| cooked)
            .collect();
        Arc::new(CookedNormal {
            key,
            size,
            rgba: TIPPED.repeat((size * size) as usize),
            levels,
        })
    }
}

impl TextureSource for CookedNormal {
    fn key(&self) -> u64 {
        self.key
    }
    fn size(&self) -> (u32, u32) {
        (self.size, self.size)
    }
    fn rgba(&self) -> &[u8] {
        &self.rgba
    }
    fn bc5_levels(&self) -> &[Vec<u8>] {
        &self.levels
    }
}

fn normal_mapped(normal: Option<Arc<dyn TextureSource>>) -> DrawList {
    let mut list = lit_block(Cooked::texture(1, 64, false));
    list.ambient.intensity = 0.2;
    // From the front and the right, so tipping the face right lights it.
    list.sun.direction = Vec3::new(1.0, 0.0, 1.0).normalize();
    list.items[0].material.maps.normal = normal.map(|texture| NormalMap {
        texture,
        scale: 1.0,
        flip_green: false,
    });
    list
}

#[test]
fn a_bc5_normal_map_tips_the_light_like_its_rgba_original() {
    let Some((device, queue)) = bc7_device() else {
        eprintln!("skipped: no graphics adapter with BC compression on this machine");
        return;
    };
    let draw = |normal| {
        let mut renderer = TextureRenderer::new(&device, &queue, 64, 64);
        renderer.render(&normal_mapped(normal)).unwrap();
        (renderer.read_pixels(), renderer)
    };
    let (flat, _) = draw(None);
    let (plain, _) = draw(Some(CookedNormal::texture(2, 64, false)));
    let (cooked, renderer) = draw(Some(CookedNormal::texture(3, 64, true)));
    assert_eq!(texels(&renderer, 3), Texels::Bc5);

    // The map changes the light, and the BC5 copy (no blue: worked out
    // from red and green) changes it the same way.
    assert!(
        largest_difference(&flat, &plain) > 10,
        "the map does nothing"
    );
    let difference = largest_difference(&plain, &cooked);
    assert!(difference <= 3, "pixels differ by up to {difference}");
}

#[test]
fn a_gpu_without_bc5_draws_a_cooked_normal_map_from_its_pixels() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut renderer = TextureRenderer::new(&device, &queue, 64, 64);
    renderer
        .render(&normal_mapped(Some(CookedNormal::texture(3, 64, true))))
        .unwrap();
    assert_eq!(texels(&renderer, 3), Texels::Rgba8);
}
