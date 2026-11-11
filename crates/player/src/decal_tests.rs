//! Phase 75: hello-decals' Decals and ProjectedDecals save, reload and
//! export, and draw the same frame each way.

use std::fs;
use std::path::Path;

use axcenity_core::{ClassName, Value};
use axcenity_render::glam::Vec3;
use axcenity_render::{Camera, DecalRole, TextureRenderer};

use super::tests::{TempCopy, test_gpu};
use super::*;

/// A Decal's or ProjectedDecal's class, path and properties.
type SavedDecal = (ClassName, String, Vec<(String, Value)>);

/// Every Decal and ProjectedDecal, in Explorer order.
fn decals_in(world: &World) -> Vec<SavedDecal> {
    world
        .descendants(world.workspace())
        .into_iter()
        .filter_map(|id| {
            let instance = world.get(id)?;
            matches!(
                instance.class(),
                ClassName::Decal | ClassName::ProjectedDecal
            )
            .then(|| {
                let mut properties: Vec<(String, Value)> = instance
                    .properties()
                    .map(|(name, value)| (name.to_owned(), value.clone()))
                    .collect();
                properties.sort_by(|a, b| a.0.cmp(&b.0));
                (
                    instance.class(),
                    world.full_name(id).unwrap_or_default(),
                    properties,
                )
            })
        })
        .collect()
}

#[test]
fn decals_save_reload_and_export_and_draw_the_same() {
    let Some((device, queue)) = test_gpu() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let example = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-decals");
    let folder = TempCopy::of(
        &example,
        &format!("axcenity-decals-save-{}", std::process::id()),
    );

    let camera = Camera {
        position: Vec3::new(12.0, 8.0, 16.0),
        look_at: Vec3::ZERO,
        fov_y_degrees: 70.0,
    };
    let mut renderer = TextureRenderer::new(&device, &queue, 240, 135);
    let mut drawn = |folder: &Path| {
        let project = axcenity_project::load(folder).unwrap();
        assert!(project.problems.is_empty(), "{:?}", project.problems);
        let assets = Assets::new(folder);
        let list = axcenity_scene::build_draw_list(&project.world, &assets, camera);
        // Models and textures load the first time they're drawn.
        for _ in 0..2 {
            renderer.render(&list).unwrap();
        }
        assert!(assets.take_errors().is_empty());
        let face_decals = list
            .items
            .iter()
            .filter(|item| item.decal_role == DecalRole::Decal)
            .count();
        assert_eq!((list.decals.len(), face_decals), (2, 2), "two of each");
        assert!(
            list.decals.iter().all(|decal| decal.texture.is_some()),
            "their pictures load"
        );
        (decals_in(&project.world), renderer.read_pixels())
    };
    let (decals, pixels) = drawn(&folder);
    assert_eq!(decals.len(), 4);

    // Saved and reloaded.
    let world = axcenity_project::load(&folder).unwrap().world;
    axcenity_project::save_world(&folder, &world).unwrap();
    let (reloaded, reloaded_pixels) = drawn(&folder);
    assert_eq!(reloaded, decals);
    assert!(
        reloaded_pixels == pixels,
        "the reloaded project draws the same"
    );

    // Exported.
    let exported = folder.with_extension("exported");
    let _ = fs::remove_dir_all(&exported);
    let packed = axcenity_project::pack_game(&folder).unwrap();
    axcenity_project::unpack_game(&packed, &exported).unwrap();
    let (in_game, game_pixels) = drawn(&exported);
    assert_eq!(in_game, decals);
    assert!(game_pixels == pixels, "the exported game draws the same");
    let _ = fs::remove_dir_all(&folder);
    let _ = fs::remove_dir_all(&exported);
}
