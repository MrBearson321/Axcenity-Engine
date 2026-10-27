//! Live asset reload: a texture written over its file, read again with
//! `Assets::reload`, is drawn on everything that uses it.

use std::sync::Arc;

use axcenity_assets::{Assets, Reload};
use axcenity_core::{ClassName, UDim2, Value, World};
use axcenity_render::glam::Vec3;
use axcenity_render::{Camera, DrawList, TextureSource, UiDrawItem};

use crate::build_draw_list_with_ui;

const TEXTURE: &str = "assets/textures/wood.png";

/// A `width` × 2 PNG of one colour.
fn png(width: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    image::RgbaImage::from_pixel(width, 2, image::Rgba([200, 120, 40, 255]))
        .write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Png,
        )
        .unwrap();
    bytes
}

/// A Part with a SurfaceAppearance, a TerrainLayer and an ImageLabel, all
/// showing [`TEXTURE`].
fn world() -> World {
    let mut world = World::new();
    let workspace = world.workspace();
    let part = world.create(ClassName::Part).unwrap();
    world.set_parent(part, Some(workspace)).unwrap();
    let appearance = world.create(ClassName::SurfaceAppearance).unwrap();
    world.set_parent(appearance, Some(part)).unwrap();
    world
        .set_property(appearance, "ColorMap", Value::String(TEXTURE.into()))
        .unwrap();

    let terrain = world.create(ClassName::Terrain).unwrap();
    world.set_parent(terrain, Some(workspace)).unwrap();
    let layer = world.create(ClassName::TerrainLayer).unwrap();
    world.set_parent(layer, Some(terrain)).unwrap();
    world
        .set_property(layer, "ColorMap", Value::String(TEXTURE.into()))
        .unwrap();

    let gui = world.create(ClassName::ScreenGui).unwrap();
    world.set_parent(gui, Some(workspace)).unwrap();
    let label = world.create(ClassName::ImageLabel).unwrap();
    world.set_parent(label, Some(gui)).unwrap();
    world
        .set_property(
            label,
            "Size",
            Value::UDim2(UDim2::new(0.0, 50.0, 0.0, 50.0)),
        )
        .unwrap();
    world
        .set_property(label, "Image", Value::String(TEXTURE.into()))
        .unwrap();
    world
}

fn draw(world: &World, assets: &Assets) -> DrawList {
    let camera = Camera {
        position: Vec3::new(0.0, 40.0, 40.0),
        look_at: Vec3::ZERO,
        fov_y_degrees: 70.0,
    };
    build_draw_list_with_ui(world, assets, camera, (320, 240), &[])
}

/// The texture on the part, on the terrain's first layer and on the label.
fn textures(list: &DrawList) -> [Arc<dyn TextureSource>; 3] {
    let part = list
        .items
        .iter()
        .find_map(|item| {
            (item.material.terrain.is_none())
                .then(|| item.material.base_color_texture.clone())
                .flatten()
        })
        .expect("the part's texture");
    let layer = list
        .items
        .iter()
        .find_map(|item| item.material.terrain.as_ref())
        .and_then(|terrain| terrain.layers.layers[0].base_color_texture.clone())
        .expect("the terrain layer's texture");
    let label = list
        .ui
        .iter()
        .find_map(|item| match item {
            UiDrawItem::Image(image) => image.texture.clone(),
            _ => None,
        })
        .expect("the label's texture");
    [part, layer, label]
}

#[test]
fn a_texture_written_over_its_file_is_drawn_on_appearances_terrain_layers_and_image_labels() {
    let folder = std::env::temp_dir().join(format!("axcenity-scene-reload-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(folder.join("assets/textures")).unwrap();
    std::fs::write(folder.join(TEXTURE), png(2)).unwrap();
    let assets = Assets::new(&folder);
    let world = world();
    let before = textures(&draw(&world, &assets));
    assert!(before.iter().all(|texture| texture.size() == (2, 2)));

    std::fs::write(folder.join(TEXTURE), png(8)).unwrap();
    assert_eq!(assets.reload(TEXTURE), Reload::Reloaded);
    let after = textures(&draw(&world, &assets));
    for (old, new) in before.iter().zip(&after) {
        assert_eq!(new.size(), (8, 2));
        // A new key, so the renderer uploads it again.
        assert_ne!(old.key(), new.key());
    }
    assert!(assets.take_errors().is_empty());
    let _ = std::fs::remove_dir_all(&folder);
}
