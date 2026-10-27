//! `workspace.CurrentCamera` in the editor's Play (unified Phase 31), with
//! the `hello-camera` example. The player runs it in
//! `player/src/camera_tests.rs` and checks the same numbers, so the
//! flyover plays the same in both.

use axcenity_core::input::{InputEvent, InputKind};
use axcenity_core::{ClassName, UserInputType, Value, Vector3};
use axcenity_render::glam::Vec3;
use axcenity_scene::CameraView as ScreenView;
use axcenity_script::{CameraState, CameraView};

use super::EditorApp;
use super::tests::{copy_of_example, inserted, printed};
use crate::tree::Insertable;

const FRAME: f64 = 1.0 / 60.0;
const SIZE: (u32, u32) = (960, 540);

/// The flyover's points, as `camera.client.lua` makes them.
const CORNER: Vector3 = Vector3::new(30.0, 20.0, 30.0);
const OVERLOOK: Vector3 = Vector3::new(0.0, 30.0, 10.0);
const FAR_SIDE: Vector3 = Vector3::new(-30.0, 12.0, 10.0);

fn lerp(a: Vector3, b: Vector3, t: f32) -> Vector3 {
    Vector3::new(
        a.x + (b.x - a.x) * t,
        a.y + (b.y - a.y) * t,
        a.z + (b.z - a.z) * t,
    )
}

fn near(a: Vector3, b: Vector3, within: f32) -> bool {
    (a.x - b.x).abs() < within && (a.y - b.y).abs() < within && (a.z - b.z).abs() < within
}

/// The camera the viewport draws during Play, as `chrome/panes.rs` asks.
fn shown(app: &EditorApp) -> CameraView {
    let (position, orientation) = app.viewport.camera_view();
    app.play.as_ref().unwrap().shown_camera(CameraState {
        position,
        orientation,
    })
}

fn frame(app: &mut EditorApp, input: &[InputEvent]) {
    app.play_frame(FRAME, input, None, None);
}

fn playing(test: &str) -> EditorApp {
    let folder = copy_of_example("hello-camera", test);
    let mut app = EditorApp::new(Some(&folder));
    app.viewport.set_pixel_size(SIZE);
    app.play();
    app
}

fn done(app: &EditorApp) -> bool {
    printed(app)
        .iter()
        .any(|line| line.starts_with("Flyover done"))
}

/// Where `point` is drawn by the scene's own projection (the renderer's),
/// in pixels from the viewport's top left.
fn drawn_at(app: &EditorApp, point: Vector3) -> [f32; 2] {
    let view = ScreenView::new(shown(app).render_camera(), SIZE.0 as f32 / SIZE.1 as f32);
    let screen = view
        .project_point(Vec3::new(point.x, point.y, point.z))
        .expect("in front of the camera");
    [
        (screen.x + 1.0) / 2.0 * SIZE.0 as f32,
        (1.0 - screen.y) / 2.0 * SIZE.1 as f32,
    ]
}

fn position_of(app: &EditorApp, name: &str) -> Vector3 {
    let world = app.play.as_ref().unwrap().world();
    let id = world.find_first_child(world.workspace(), name).unwrap();
    world
        .get(id)
        .unwrap()
        .property("Position")
        .and_then(Value::as_vector3)
        .unwrap()
}

#[test]
fn a_scriptable_flyover_plays_as_in_the_player_and_stop_brings_the_editor_camera_back() {
    let mut app = playing("camera-flyover");
    let fly = app.viewport.camera_view();
    for step in 1..=60 {
        frame(&mut app, &[]);
        let view = shown(&app);
        let want = lerp(CORNER, OVERLOOK, step as f32 / 60.0);
        assert!(
            near(view.position, want, 1e-3),
            "frame {step}: {view:?}, want {want:?}"
        );
        assert!(app.play.as_ref().unwrap().holds_camera());
    }
    assert!(near(
        shown(&app).orientation,
        Vector3::new(-50.0, 0.0, 0.0),
        1e-2
    ));
    let mut views = Vec::new();
    while !done(&app) {
        assert!(views.len() < 300, "the flyover never ended");
        frame(&mut app, &[]);
        views.push(shown(&app));
    }
    for (step, view) in views[..views.len() - 1].iter().enumerate() {
        let want = lerp(OVERLOOK, FAR_SIDE, (step + 1) as f32 / 60.0);
        assert!(
            near(view.position, want, 1e-3),
            "frame {step}: {view:?}, want {want:?}"
        );
    }
    assert_eq!(views.len(), 60);

    // Custom again: the character's controller has the camera.
    let character = app.play.as_ref().unwrap().character_camera().unwrap();
    assert!(near(shown(&app).position, character.position, 1e-4));
    // The fly camera never moved, and comes back on Stop.
    assert_eq!(app.viewport.camera_view(), fly);
    app.stop();
    assert_eq!(app.viewport.camera_view(), fly);
}

#[test]
fn part_under_the_mouse_and_the_beacon_tag_line_up_with_what_the_viewport_draws() {
    let mut app = playing("camera-mouse");
    let mut moved = 0;
    let mut last = None;
    while !done(&app) {
        frame(&mut app, &[]);
        let camera = shown(&app);
        // The tag is centred over the beacon's top, as drawn.
        let session = app.play.as_ref().unwrap();
        let world = session.world();
        let tag = world
            .find_first_child(world.workspace(), "Hud")
            .and_then(|hud| world.find_first_child(hud, "Tag"))
            .unwrap();
        let layout = axcenity_ui::layout_ui(world, SIZE.0, SIZE.1, &[]);
        if let Some(element) = layout.elements.iter().find(|element| element.id == tag) {
            let beacon = position_of(&app, "Beacon");
            let want = drawn_at(&app, Vector3::new(beacon.x, beacon.y + 5.0, beacon.z));
            let middle = [
                element.rect.x + element.rect.width / 2.0,
                element.rect.y + element.rect.height / 2.0,
            ];
            assert!(
                (middle[0] - want[0]).abs() < 0.5 && (middle[1] - want[1]).abs() < 0.5,
                "the tag's middle is {middle:?}, the beacon's top is drawn at {want:?}"
            );
        }
        if last.is_some_and(|last: CameraView| last.position != camera.position) {
            moved += 1;
        }
        last = Some(camera);
    }
    assert!(moved > 100, "{moved}");

    // A click names the part drawn under it.
    for name in ["Beacon", "Crate"] {
        let [x, y] = drawn_at(&app, position_of(&app, name));
        let click = |pressed| {
            InputEvent::new(InputKind::MouseButton {
                button: UserInputType::MouseButton1,
                pressed,
                position: [x, y],
            })
        };
        frame(&mut app, &[click(true), click(false)]);
        let said = format!("Under the mouse: {name}");
        assert!(
            printed(&app).contains(&said.as_str()),
            "{:?}",
            printed(&app)
        );
    }
}

#[test]
fn an_inserted_camera_is_a_viewpoint_where_the_editor_looks_from_and_saves_as_a_camera() {
    let folder = copy_of_example("hello-camera", "camera-insert");
    let mut app = EditorApp::new(Some(&folder));
    app.viewport
        .look_at(Vec3::new(5.0, 9.0, 12.0), Vec3::new(5.0, 0.0, 0.0));
    let (position, orientation) = app.viewport.camera_view();
    let camera = inserted(&mut app, Insertable::Camera, None);
    let instance = app.world.get(camera).unwrap();
    assert_eq!(instance.class(), ClassName::Camera);
    assert_eq!(instance.parent(), Some(app.world.workspace()));
    assert_eq!(
        instance.property("Position"),
        Some(&Value::Vector3(position))
    );
    assert_eq!(
        instance.property("Orientation"),
        Some(&Value::Vector3(orientation))
    );

    assert!(app.save());
    let reopened = EditorApp::new(Some(&folder));
    let world = &reopened.world;
    let cameras: Vec<_> = world
        .get(world.workspace())
        .unwrap()
        .children()
        .iter()
        .filter(|&&id| world.get(id).unwrap().class() == ClassName::Camera)
        .collect();
    assert_eq!(cameras.len(), 2, "the example's Overlook and the new one");
    let _ = std::fs::remove_dir_all(&folder);
}
