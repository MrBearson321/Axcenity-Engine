//! `workspace.CurrentCamera` in the player (unified Phase 31), with the
//! `hello-camera` example: a Scriptable flyover tweened along a CFrame
//! path, the part under the mouse, and a tag kept over a part with
//! `WorldToScreenPoint`. The editor's Play runs the same example in
//! `editor/src/app/camera_tests.rs` and checks the same numbers.

use axcenity_core::input::{InputEvent, InputKind};
use axcenity_core::{KeyCode, UserInputType, Value, Vector3};
use axcenity_scene::CameraView as ScreenView;

use super::input_tests::open;
use super::*;

const FRAME: f32 = 1.0 / 60.0;

/// The flyover's points, as `camera.client.lua` makes them: a corner, the
/// Overlook viewpoint, the other side.
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

impl App {
    /// One frame in the frame loop's order, in a window `size` pixels big.
    fn camera_frame(&mut self, size: (u32, u32)) {
        self.input.map = self.scripts.input_map();
        let motion = self.input.motion_input([0.0, 0.0], 0.0, FRAME);
        self.characters
            .step(&mut self.world, &self.assets, motion, FRAME);
        self.begin_camera(FRAME, size);
        let events = self.input.take_events(self.pointer);
        self.scripts.input_events(&mut self.world, &events);
        self.scripts.step(&mut self.world, f64::from(FRAME));
        self.characters.keep_kinematic(&mut self.world);
        self.physics.step(&mut self.world, f64::from(FRAME));
        self.scripts.heartbeat(&mut self.world, f64::from(FRAME));
        self.end_camera(FRAME);
    }

    fn said(&mut self) -> Vec<String> {
        self.scripts
            .take_output()
            .into_iter()
            .map(|line| line.text)
            .collect()
    }

    /// Frames until the flyover ends, with the drawn camera of each.
    fn fly_over(&mut self, size: (u32, u32)) -> Vec<axcenity_script::CameraView> {
        let mut views = Vec::new();
        for _ in 0..300 {
            self.camera_frame(size);
            views.push(self.shown_view());
            if self
                .said()
                .iter()
                .any(|line| line.starts_with("Flyover done"))
            {
                return views;
            }
        }
        panic!("the flyover never ended");
    }

    fn part_position(&self, name: &str) -> Vector3 {
        let id = self
            .world
            .find_first_child(self.world.workspace(), name)
            .unwrap();
        self.world
            .get(id)
            .unwrap()
            .property("Position")
            .and_then(Value::as_vector3)
            .unwrap()
    }

    /// Where `point` is drawn in a window `size` big, by the scene's own
    /// projection (the renderer's), in pixels from the top left.
    fn drawn_at(&self, point: Vector3, size: (u32, u32)) -> [f32; 2] {
        let view = ScreenView::new(self.shown_camera(), size.0 as f32 / size.1 as f32);
        let screen = view
            .project_point(Vec3::new(point.x, point.y, point.z))
            .expect("in front of the camera");
        [
            (screen.x + 1.0) / 2.0 * size.0 as f32,
            (1.0 - screen.y) / 2.0 * size.1 as f32,
        ]
    }
}

#[test]
fn a_scriptable_flyover_follows_its_cframe_path_and_custom_gives_the_character_the_camera_back() {
    let (mut app, folder) = open("hello-camera", None);
    let size = (960, 540);
    // The first leg: a second of Linear tween, a 60th of the way per frame.
    for frame in 1..=60 {
        app.camera_frame(size);
        assert!(
            app.game_camera.scripted.is_some(),
            "Scriptable holds the view"
        );
        let shown = app.shown_view();
        let want = lerp(CORNER, OVERLOOK, frame as f32 / 60.0);
        assert!(
            near(shown.position, want, 1e-3),
            "frame {frame}: {shown:?}, want {want:?}"
        );
    }
    // At the Overlook viewpoint, it looks the way the viewpoint does.
    let at_overlook = app.shown_view();
    assert!(
        near(at_overlook.orientation, Vector3::new(-50.0, 0.0, 0.0), 1e-2),
        "{at_overlook:?}"
    );
    // The second leg starts the next frame and passes through its middle.
    // On its last frame, which reaches the far side, the script sets the
    // camera back to Custom, so that frame already shows the character.
    let views = app.fly_over(size);
    for (frame, view) in views[..views.len() - 1].iter().enumerate() {
        let want = lerp(OVERLOOK, FAR_SIDE, (frame + 1) as f32 / 60.0);
        assert!(
            near(view.position, want, 1e-3),
            "frame {frame}: {view:?}, want {want:?}"
        );
    }
    assert_eq!(views.len(), 60);

    // Custom again: the character's controller has the camera.
    assert_eq!(app.game_camera.scripted, None);
    let character = app.characters.view(&app.world).unwrap();
    assert!(near(app.shown_view().position, character.position, 1e-4));
    // And it follows the character as it walks.
    let before = app.shown_view().position;
    app.push_key(KeyCode::W, true, false);
    for _ in 0..30 {
        app.camera_frame(size);
    }
    let walked = app.shown_view().position;
    let character = app.characters.view(&app.world).unwrap();
    assert!(near(walked, character.position, 1e-4));
    assert!(!near(walked, before, 1.0), "{before:?} -> {walked:?}");
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn part_under_the_mouse_is_the_part_the_cursor_is_over_at_any_window_size() {
    let (mut app, folder) = open("hello-camera", None);
    app.fly_over((960, 540));
    for size in [(960, 540), (1920, 1080), (640, 900), (300, 200)] {
        app.camera_frame(size);
        for name in ["Beacon", "Crate"] {
            let [x, y] = app.drawn_at(app.part_position(name), size);
            for pressed in [true, false] {
                app.input.push(InputEvent::new(InputKind::MouseButton {
                    button: UserInputType::MouseButton1,
                    pressed,
                    position: [x, y],
                }));
            }
            app.camera_frame(size);
            assert_eq!(
                app.said(),
                [format!("Under the mouse: {name}")],
                "at {x}, {y} in {size:?}"
            );
        }
    }
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn world_to_screen_point_keeps_the_tag_over_the_beacon_as_the_camera_moves() {
    let (mut app, folder) = open("hello-camera", None);
    let stylesheets = app.stylesheets.clone();
    let tag = app
        .world
        .find_first_child(app.world.workspace(), "Hud")
        .and_then(|hud| app.world.find_first_child(hud, "Tag"))
        .unwrap();
    // Above the 8-stud beacon's top, as the script puts it.
    let beacon = app.part_position("Beacon");
    let above = Vector3::new(beacon.x, beacon.y + 4.0 + 1.0, beacon.z);
    let size = (960, 540);
    let mut checked = 0;
    let mut last = None;
    for _ in 0..200 {
        app.camera_frame(size);
        let camera = app.shown_view();
        let layout = axcenity_ui::layout_ui(&app.world, size.0, size.1, &stylesheets);
        let element = layout.elements.iter().find(|element| element.id == tag);
        let Some(element) = element else {
            continue; // Off screen: hidden.
        };
        let middle = [
            element.rect.x + element.rect.width / 2.0,
            element.rect.y + element.rect.height / 2.0,
        ];
        let want = app.drawn_at(above, size);
        assert!(
            (middle[0] - want[0]).abs() < 0.5 && (middle[1] - want[1]).abs() < 0.5,
            "the tag's middle is {middle:?}, the beacon's top is drawn at {want:?} ({camera:?})"
        );
        if last.is_some_and(|last: axcenity_script::CameraView| last.position != camera.position) {
            checked += 1;
        }
        last = Some(camera);
    }
    assert!(
        checked > 100,
        "the camera should move for most of it: {checked}"
    );
    let _ = std::fs::remove_dir_all(&folder);
}
