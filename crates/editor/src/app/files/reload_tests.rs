//! Live asset reload: files under `assets/` written by other programs are
//! swapped in, in edit mode and during Play.

use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axcenity_anim::files::{Motion, StateDef, StateMachineFile};
use axcenity_assets::Assets;
use axcenity_core::{PlayRequest, World};
use axcenity_render::glam::Vec3;
use axcenity_render::{Camera, Shape};

use super::super::tests::{copy_of_example, play_for, printed};
use super::*;

const TRIANGLE: &str = "assets/models/tri.gltf";

/// A one-triangle glTF `width` studs wide at [`TRIANGLE`], its buffer in a
/// `.bin` file beside it.
fn write_triangle(folder: &Path, width: f32) {
    let corners: [f32; 9] = [0.0, 0.0, 0.0, width, 0.0, 0.0, 0.0, 1.0, 0.0];
    let bytes: Vec<u8> = corners.iter().flat_map(|c| c.to_le_bytes()).collect();
    let models = folder.join("assets/models");
    fs::create_dir_all(&models).unwrap();
    fs::write(models.join("tri.bin"), bytes).unwrap();
    let gltf = format!(
        r#"{{
            "asset": {{ "version": "2.0" }},
            "buffers": [ {{ "uri": "tri.bin", "byteLength": 36 }} ],
            "bufferViews": [ {{ "buffer": 0, "byteLength": 36 }} ],
            "accessors": [ {{ "bufferView": 0, "componentType": 5126, "count": 3,
                              "type": "VEC3", "min": [0, 0, 0], "max": [{width}, 1, 0] }} ],
            "meshes": [ {{ "primitives": [ {{ "attributes": {{ "POSITION": 0 }} }} ] }} ]
        }}"#
    );
    fs::write(models.join("tri.gltf"), gltf).unwrap();
}

/// Adds an anchored MeshPart showing [`TRIANGLE`] to the edit world.
fn add_triangle_part(app: &mut EditorApp) -> InstanceId {
    let world = &mut app.world;
    let part = world.create(ClassName::MeshPart).unwrap();
    world.set_name(part, "Triangle").unwrap();
    world
        .set_property(part, "MeshId", Value::String(TRIANGLE.into()))
        .unwrap();
    world
        .set_property(part, "Anchored", Value::Bool(true))
        .unwrap();
    world.set_parent(part, Some(world.workspace())).unwrap();
    part
}

/// The key of the first mesh drawn for `world`.
fn drawn_mesh(world: &World, assets: &Assets) -> u64 {
    let camera = Camera {
        position: Vec3::new(0.0, 5.0, 10.0),
        look_at: Vec3::ZERO,
        fov_y_degrees: 70.0,
    };
    axcenity_scene::build_draw_list(world, assets, camera)
        .items
        .iter()
        .find_map(|item| match &item.shape {
            Shape::Mesh(mesh) => Some(mesh.key()),
            _ => None,
        })
        .expect("the MeshPart is drawn")
}

/// Takes in changed files (and plays a frame, during Play) until `done`,
/// for up to 3 seconds. How long it took.
fn until(app: &mut EditorApp, done: impl Fn(&EditorApp) -> bool) -> Duration {
    let start = Instant::now();
    while !done(app) && start.elapsed() < Duration::from_secs(3) {
        std::thread::sleep(Duration::from_millis(20));
        app.check_disk();
        if app.playing() {
            play_for(app, 1.0 / 60.0);
        }
    }
    start.elapsed()
}

fn width(app: &EditorApp) -> f32 {
    app.assets.model_blocking(TRIANGLE).unwrap().size[0]
}

#[test]
fn a_model_written_over_its_file_updates_its_mesh_parts_in_edit_mode_and_in_play() {
    let folder = copy_of_example("hello-touched", "reload-model");
    write_triangle(&folder, 1.0);
    let mut app = EditorApp::new(Some(&folder));
    add_triangle_part(&mut app);
    let first = drawn_mesh(&app.world, &app.assets);
    // Let the watcher take its first look.
    std::thread::sleep(Duration::from_millis(100));

    // In edit mode.
    write_triangle(&folder, 2.5);
    let took = until(&mut app, |app| width(app) == 2.5);
    assert!(took < Duration::from_secs(1), "took {took:?}");
    let second = drawn_mesh(&app.world, &app.assets);
    assert_ne!(first, second, "drawn with the new model");
    // The `.bin` beside it may be the one named: it reads the model again.
    assert!(
        printed(&app)
            .iter()
            .any(|line| line.starts_with("Reloaded `assets/models/tri.")),
        "{:?}",
        printed(&app)
    );

    // During Play: the game keeps running, and draws the new model.
    app.play();
    play_for(&mut app, 0.1);
    write_triangle(&folder, 3.75);
    let took = until(&mut app, |app| width(app) == 3.75);
    assert!(took < Duration::from_secs(1), "took {took:?}");
    assert!(app.playing());
    play_for(&mut app, 0.1);
    let session = app.play.as_ref().unwrap();
    assert_ne!(drawn_mesh(session.world(), &app.assets), second);
    assert!(
        session
            .world()
            .find_first_child(session.world().workspace(), "Triangle")
            .is_some()
    );
    drop(app);
    let _ = fs::remove_dir_all(&folder);
}

#[test]
fn a_broken_model_saved_over_a_good_one_keeps_the_good_one_and_is_reported_once() {
    let folder = copy_of_example("hello-touched", "reload-broken");
    write_triangle(&folder, 1.0);
    let mut app = EditorApp::new(Some(&folder));
    add_triangle_part(&mut app);
    let good = app.assets.model_blocking(TRIANGLE).unwrap();
    let drawn = drawn_mesh(&app.world, &app.assets);
    std::thread::sleep(Duration::from_millis(100));

    fs::write(folder.join(TRIANGLE), "{ half a model").unwrap();
    let reported = |app: &EditorApp| {
        printed(app)
            .iter()
            .filter(|line| line.starts_with("Could not reload `assets/models/tri.gltf`"))
            .count()
    };
    until(&mut app, |app| reported(app) > 0);
    // Saved broken again, and looked at for a while longer.
    fs::write(folder.join(TRIANGLE), "{ still half a model").unwrap();
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(1200) {
        std::thread::sleep(Duration::from_millis(50));
        app.check_disk();
    }
    assert_eq!(reported(&app), 1, "{:?}", printed(&app));
    assert!(Arc::ptr_eq(
        &app.assets.model_blocking(TRIANGLE).unwrap(),
        &good
    ));
    assert_eq!(
        drawn_mesh(&app.world, &app.assets),
        drawn,
        "the good one stays on screen"
    );
    drop(app);
    let _ = fs::remove_dir_all(&folder);
}

const DOOR_CLIP: &str = "assets/animations/Door/Door.anim.json";
const DOOR_MACHINE: &str = "assets/animations/Door/Door.statemachine.json";

/// The door's height in the game.
fn door_y(app: &EditorApp) -> f32 {
    let world = app.play.as_ref().unwrap().world();
    let door = world.find_first_child(world.workspace(), "Door").unwrap();
    world
        .get(door)
        .and_then(|door| door.property("Position"))
        .and_then(Value::as_vector3)
        .unwrap()
        .y
}

#[test]
fn an_edited_clip_changes_a_playing_animators_motion_and_it_stays_in_its_state() {
    let folder = copy_of_example("hello-door", "reload-clip");
    // Shut plays nothing; Rise plays the door clip (y 2 to 8 over 1.5 s).
    let machine = StateMachineFile {
        default_state: "Shut".into(),
        states: vec![
            StateDef::new("Shut", Motion::Clip(String::new()), [0.0, 0.0]),
            StateDef::new("Rise", Motion::Clip(DOOR_CLIP.into()), [0.0, 0.0]),
        ],
        ..StateMachineFile::default()
    };
    fs::write(folder.join(DOOR_MACHINE), machine.to_json()).unwrap();
    let mut app = EditorApp::new(Some(&folder));
    let door = app
        .world
        .find_first_child(app.world.workspace(), "Door")
        .unwrap();
    let animator = app.world.find_first_child(door, "Animator").unwrap();
    app.world
        .set_property(animator, "StateMachine", Value::String(DOOR_MACHINE.into()))
        .unwrap();
    std::thread::sleep(Duration::from_millis(100));

    app.play();
    play_for(&mut app, 1.0 / 60.0);
    let session = app.play.as_mut().unwrap();
    session.world_mut().animator_mut(animator).unwrap().play = Some(PlayRequest {
        state: "Rise".into(),
        blend: None,
    });
    play_for(&mut app, 0.5);
    let before = door_y(&app);
    assert!(before > 2.5 && before < 5.0, "{before}");

    // The clip now holds the door at 20, higher than the old one ever goes.
    let clip = fs::read_to_string(folder.join(DOOR_CLIP)).unwrap();
    let held = clip.replace("2.0,", "20.0,").replace("8.0", "20.0");
    fs::write(folder.join(DOOR_CLIP), held).unwrap();
    let reloaded = format!("Reloaded `{DOOR_CLIP}`.");
    let took = until(&mut app, |app| printed(app).contains(&reloaded.as_str()));
    assert!(took < Duration::from_secs(1), "took {took:?}");
    play_for(&mut app, 1.0 / 60.0);
    let world = app.play.as_ref().unwrap().world();
    assert_eq!(world.animator(animator).unwrap().current_state, "Rise");
    assert_eq!(door_y(&app), 20.0);
    assert!(app.playing(), "the game kept running");
    drop(app);
    let _ = fs::remove_dir_all(&folder);
}
