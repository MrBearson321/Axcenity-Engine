//! The texture memory budget: textures past it lose their largest levels,
//! the least recently drawn first, and get them back when there's room.

use super::*;

const MIB: u64 = 1024 * 1024;

/// A square texture of one color, so it looks the same at every mip level.
struct Plain(u64, u32, Vec<u8>);

impl Plain {
    fn texture(key: u64, size: u32, color: [u8; 3]) -> Arc<dyn TextureSource> {
        let [red, green, blue] = color;
        let pixels = [red, green, blue, 255].repeat((size * size) as usize);
        Arc::new(Plain(key, size, pixels))
    }
}

impl TextureSource for Plain {
    fn key(&self) -> u64 {
        self.0
    }
    fn size(&self) -> (u32, u32) {
        (self.1, self.1)
    }
    fn rgba(&self) -> &[u8] {
        &self.2
    }
}

/// Eight blocks in a row, each with its own 1024² texture (keys 100 to
/// 107): about 43 MiB of texture data with their mips.
fn eight_textured_blocks() -> DrawList {
    let items = (0..8u8)
        .map(|index| {
            let color = [30 * index, 200 - 20 * index, 90];
            let mut block = textured_block(Plain::texture(100 + u64::from(index), 1024, color));
            block.transform = Mat4::from_scale_rotation_translation(
                Vec3::splat(2.5),
                glam::Quat::IDENTITY,
                Vec3::new(-10.5 + 3.0 * f32::from(index), 0.0, -20.0),
            );
            block
        })
        .collect();
    let mut list = list_of(items);
    list.ambient.intensity = 1.0;
    list
}

fn with_budget(megabytes: u32) -> GraphicsSettings {
    GraphicsSettings {
        texture_budget: Some(megabytes),
        ..GraphicsSettings::default()
    }
}

/// How many levels the budget left out of the texture with `key`.
fn dropped(renderer: &TextureRenderer, key: u64) -> u32 {
    renderer.scene.textures[&key].gpu.as_ref().unwrap().dropped
}

#[test]
fn more_texture_data_than_the_budget_stays_within_it_and_draws() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let list = eight_textured_blocks();
    let (width, height) = (96, 64);
    let mut unlimited = TextureRenderer::new(&device, &queue, width, height);
    unlimited.render(&list).unwrap();
    let full_bytes = unlimited.stats().texture_bytes;
    assert!(full_bytes > 40 * MIB, "{full_bytes} bytes");

    let mut renderer = TextureRenderer::new(&device, &queue, width, height);
    renderer.set_graphics(&with_budget(16));
    for _ in 0..3 {
        renderer.render(&list).unwrap();
        let bytes = renderer.stats().texture_bytes;
        assert!(bytes <= 16 * MIB, "{bytes} bytes over a 16 MiB budget");
    }
    let shrunk: Vec<u32> = (100..108).map(|key| dropped(&renderer, key)).collect();
    // Each loses its largest level (a quarter of it left), which leaves
    // room for one to keep its full size.
    let kept_full = shrunk.iter().filter(|&&levels| levels == 0).count();
    assert!(kept_full <= 1, "{shrunk:?}");
    assert_eq!(renderer.scene.materials.len(), 8, "one each, not remade");

    // The smaller copies hold the same colors (the GPU copied their mips).
    let (budgeted, full) = (renderer.read_pixels(), unlimited.read_pixels());
    let difference = budgeted
        .iter()
        .zip(&full)
        .map(|(a, b)| u64::from(a.abs_diff(*b)))
        .max()
        .unwrap();
    assert!(difference <= 2, "pixels differ by up to {difference}");
}

#[test]
fn textures_not_drawn_lately_shrink_first_and_come_back_when_there_is_room() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let all = eight_textured_blocks();
    let only = |keys: std::ops::Range<usize>| {
        let mut list = all.clone();
        list.items = all.items[keys].to_vec();
        list
    };
    let mut renderer = TextureRenderer::new(&device, &queue, 64, 64);
    // Room for about four full textures.
    renderer.set_graphics(&with_budget(22));
    renderer.render(&only(0..4)).unwrap();
    assert!((100..104).all(|key| dropped(&renderer, key) == 0));

    // The first four are kept but not drawn: they shrink, the new ones don't.
    renderer.render(&only(4..8)).unwrap();
    assert!(renderer.stats().texture_bytes <= 22 * MIB);
    assert!((104..108).all(|key| dropped(&renderer, key) == 0));
    assert!((100..104).all(|key| dropped(&renderer, key) > 0));

    // With room again, those drawn get their full size back.
    renderer.set_graphics(&with_budget(64));
    renderer.render(&only(0..4)).unwrap();
    assert!((100..108).all(|key| dropped(&renderer, key) == 0));
}
