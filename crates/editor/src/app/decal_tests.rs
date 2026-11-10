//! Phase 75 in the editor: hello-decals' decals in Play are the ones the
//! exported game draws, the placed ones and those its script spawns.

use axcenity_render::glam::Vec3;
use axcenity_render::{Camera, DecalRole, DrawList};

use super::tests::{copy_of_example, play_for};
use super::*;

/// The decals `list` draws: each ProjectedDecal's box, color, opacity and
/// whether it has a picture, then each face Decal's square.
fn decals_of(list: &DrawList) -> (Vec<String>, Vec<String>) {
    let projected = list
        .decals
        .iter()
        .map(|decal| {
            format!(
                "{:.3?} {:?} {:.3} {} {} {}",
                decal.transform.to_cols_array(),
                decal.color,
                decal.opacity,
                decal.fade_angle,
                decal.ignore_characters,
                decal.texture.is_some()
            )
        })
        .collect();
    let on_faces = list
        .items
        .iter()
        .filter(|item| item.decal_role == DecalRole::Decal)
        .map(|item| {
            format!(
                "{:.3?} {:?} {}",
                item.transform.to_cols_array(),
                item.material.base_color,
                item.material.base_color_texture.is_some()
            )
        })
        .collect();
    (projected, on_faces)
}

#[test]
fn decals_in_play_are_the_ones_the_exported_game_draws() {
    let folder = copy_of_example("hello-decals", "decals-play");
    let camera = Camera {
        position: Vec3::new(12.0, 8.0, 16.0),
        look_at: Vec3::ZERO,
        fov_y_degrees: 70.0,
    };
    let mut app = EditorApp::new(Some(&folder));
    app.play();
    play_for(&mut app, 1.0);
    let assets = Assets::new(&folder);
    let in_play = {
        let game = app.play.as_ref().unwrap().world();
        decals_of(&axcenity_scene::build_draw_list(game, &assets, camera))
    };
    // The road marking, the puddle and the script's six bullet holes; the
    // sign's and the crate's Decals.
    assert_eq!((in_play.0.len(), in_play.1.len()), (8, 2), "{in_play:#?}");

    // The exported game, run from its own files.
    let exported = folder.with_extension("exported");
    let _ = std::fs::remove_dir_all(&exported);
    let packed = axcenity_project::pack_game(&folder).unwrap();
    axcenity_project::unpack_game(&packed, &exported).unwrap();
    let project = axcenity_project::load(&exported).unwrap();
    assert!(project.problems.is_empty(), "{:?}", project.problems);
    let mut world = project.world;
    let game_assets = Assets::new(&exported);
    let mut scripts = axcenity_sim::script_runtime();
    scripts.set_assets(game_assets.clone());
    for script in &project.scripts {
        scripts.start_script(&mut world, script.instance, &script.path, &script.source);
    }
    for _ in 0..60 {
        scripts.step(&mut world, 1.0 / 60.0);
    }
    let in_game = decals_of(&axcenity_scene::build_draw_list(
        &world,
        &game_assets,
        camera,
    ));
    assert_eq!(in_game, in_play);
    drop(app);
    let _ = std::fs::remove_dir_all(&folder);
    let _ = std::fs::remove_dir_all(&exported);
}
