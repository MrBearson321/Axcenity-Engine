//! Phase 44: the hello-prompt example's door, chest and button, in the
//! player's frame order.

use std::path::{Path, PathBuf};

use axcenity_core::input::{InputEvent, InputKind};
use axcenity_core::{InstanceId, KeyCode, UserInputType, Value, Vector3};
use axcenity_script::CameraState;

use super::tests::start_example;
use super::*;

const FRAME: f32 = 1.0 / 60.0;
const VIEW: (u32, u32) = (480, 270);

/// The example running in the player's frame order: the character, input,
/// prompts and click detectors, then scripts.
struct Game {
    world: World,
    scripts: ScriptRuntime,
    assets: Assets,
    characters: Controllers,
    printed: Vec<String>,
}

impl Game {
    fn start() -> Self {
        let (world, mut scripts) = start_example("hello-prompt");
        scripts.set_viewport_size(VIEW.0 as f32, VIEW.1 as f32);
        Self {
            world,
            scripts,
            assets: Assets::default(),
            characters: Controllers::default(),
            printed: Vec::new(),
        }
    }

    fn frame(&mut self, input: MoveInput, events: &[InputEvent]) {
        self.characters
            .step(&mut self.world, &self.assets, input, FRAME);
        if let Some(camera) = self.characters.view(&self.world) {
            self.scripts.set_camera(CameraState {
                position: camera.position,
                orientation: camera.orientation,
            });
        }
        self.scripts.input_events(&mut self.world, events);
        self.scripts.step_prompts(&mut self.world, f64::from(FRAME));
        self.scripts.step(&mut self.world, f64::from(FRAME));
        self.printed
            .extend(self.scripts.take_output().into_iter().map(|line| line.text));
    }

    fn frames(&mut self, count: usize) {
        for _ in 0..count {
            self.frame(MoveInput::default(), &[]);
        }
    }

    fn key(&mut self, key: KeyCode, pressed: bool) {
        let event = InputEvent::new(InputKind::Key { key, pressed });
        self.frame(MoveInput::default(), &[event]);
    }

    fn find(&self, path: &[&str]) -> InstanceId {
        path.iter().fold(self.world.workspace(), |at, name| {
            self.world.find_first_child(at, name).unwrap()
        })
    }

    fn body_z(&self) -> f32 {
        self.vector(&["Character", "Body"], "Position").z
    }

    fn vector(&self, path: &[&str], name: &str) -> Vector3 {
        self.world
            .get(self.find(path))
            .unwrap()
            .property(name)
            .and_then(Value::as_vector3)
            .unwrap()
    }

    /// Puts the character's body at `x`, 2.5, `z`.
    fn stand_at(&mut self, x: f32, z: f32) {
        let body = self.find(&["Character", "Body"]);
        self.world
            .set_property(body, "Position", Value::Vector3(Vector3::new(x, 2.5, z)))
            .unwrap();
    }

    fn shown(&self) -> Vec<(String, String)> {
        self.scripts
            .shown_prompts()
            .iter()
            .map(|shown| {
                (
                    axcenity_ui::prompt_text(&self.world, shown.prompt),
                    shown.key.clone(),
                )
            })
            .collect()
    }

    fn printed(&mut self) -> Vec<String> {
        std::mem::take(&mut self.printed)
    }
}

#[test]
fn walking_up_to_the_door_shows_open_door_and_holding_e_opens_it_once() {
    let mut game = Game::start();
    game.frames(5);
    // The door is 12 studs ahead: out of reach.
    assert!(game.shown().is_empty());

    // Forward (away from the camera) until the prompt shows.
    let forward = MoveInput {
        forward: true,
        ..MoveInput::default()
    };
    for _ in 0..240 {
        if !game.shown().is_empty() {
            break;
        }
        game.frame(forward, &[]);
    }
    let z = game.body_z();
    assert!((5.0..9.0).contains(&z), "shown at z = {z}");
    assert_eq!(game.shown(), [("Open · Door".to_owned(), "E".to_owned())]);
    if let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS") {
        // Half way through a hold, for the ring.
        game.key(KeyCode::E, true);
        game.frames(14);
        snapshot(&game, &PathBuf::from(folder).join("prompt-door.png"));
        game.key(KeyCode::E, false);
        game.printed();
    }

    // Held for HoldDuration (0.5 s), and on for two seconds more: once.
    game.key(KeyCode::E, true);
    game.frames(20);
    assert!(game.printed().is_empty());
    game.frames(130);
    assert_eq!(game.printed(), ["Player\topened the door"]);
    let door = game.find(&["Door"]);
    let open = game.world.get(door).unwrap();
    assert_eq!(open.property("CanCollide"), Some(&Value::Bool(false)));
    game.key(KeyCode::E, false);
    game.frames(2);
    assert_eq!(game.shown(), [("Close · Door".to_owned(), "E".to_owned())]);

    // A controller's button shows instead once it's used, and closes it.
    let pad = |pressed| {
        InputEvent::new(InputKind::PadButton {
            pad: 0,
            key: KeyCode::ButtonX,
            pressed,
        })
    };
    game.frame(MoveInput::default(), &[pad(true)]);
    assert_eq!(game.shown()[0].1, "X");
    game.frames(40);
    game.frame(MoveInput::default(), &[pad(false)]);
    assert_eq!(game.printed(), ["Player\tclosed the door"]);
}

#[test]
fn the_chest_behind_the_wall_shows_only_once_it_can_be_seen() {
    let mut game = Game::start();
    game.frames(2);
    let chest = ("Search · Chest".to_owned(), "E".to_owned());
    // 7 studs from the chest, with the wall between: only the door shows.
    game.stand_at(8.0, 0.0);
    game.frames(2);
    assert_eq!(game.shown(), [("Open · Door".to_owned(), "E".to_owned())]);
    // Round the end of the wall, as near.
    game.stand_at(14.0, 7.0);
    game.frames(2);
    assert_eq!(game.shown(), [chest]);
    game.key(KeyCode::E, true);
    game.key(KeyCode::E, false);
    game.frames(2);
    assert_eq!(game.printed(), ["found 10 coins"]);
    assert!(
        game.shown().is_empty(),
        "the chest's prompt turns itself off"
    );
}

#[test]
fn clicking_the_button_turns_it_green_from_its_block_script() {
    let mut game = Game::start();
    game.frames(2);
    let mouse = |kind| InputEvent::new(kind);
    let middle = [VIEW.0 as f32 / 2.0, VIEW.1 as f32 / 2.0];
    let click = |pressed| {
        mouse(InputKind::MouseButton {
            button: UserInputType::MouseButton1,
            pressed,
            position: middle,
        })
    };
    // Looking straight at the button from 8 studs in front of it, with the
    // mouse in the middle of the view; one frame with `events` too.
    let look = |game: &mut Game, events: &[InputEvent]| {
        game.scripts.set_camera(CameraState {
            position: Vector3::new(-8.0, 3.25, 6.0),
            orientation: Vector3::new(0.0, 0.0, 0.0),
        });
        let mut frame = vec![mouse(InputKind::MouseMove {
            position: middle,
            delta: [0.0, 0.0],
        })];
        frame.extend_from_slice(events);
        game.scripts.input_events(&mut game.world, &frame);
        game.scripts.step_prompts(&mut game.world, f64::from(FRAME));
    };
    look(&mut game, &[]);
    assert_eq!(
        game.scripts.click_cursor(&game.world).as_deref(),
        Some("pointer")
    );
    for pressed in [true, false] {
        look(&mut game, &[click(pressed)]);
    }
    game.printed
        .extend(game.scripts.take_output().into_iter().map(|line| line.text));
    assert_eq!(game.printed(), ["the button was clicked"]);
    let color = game
        .world
        .get(game.find(&["Button"]))
        .unwrap()
        .property("Color")
        .cloned();
    assert_eq!(
        color,
        Some(Value::Color3(axcenity_core::Color3::new(0.2, 0.8, 0.3)))
    );

    // From across the room (the character 40 studs off) it's out of reach.
    game.stand_at(30.0, 25.0);
    look(&mut game, &[]);
    assert_eq!(game.scripts.click_cursor(&game.world), None);
}

/// Draws the game as the player would (from the character's camera, with
/// the prompts over it) into `path`, when there's a GPU.
fn snapshot(game: &Game, path: &Path) {
    use axcenity_render::TextureRenderer;
    let instance = axcenity_render::adapters::instance();
    let Ok(adapter) = pollster::block_on(axcenity_render::adapters::adapter(&instance, None))
    else {
        return;
    };
    let Ok((device, queue)) = pollster::block_on(adapter.request_device(&Default::default()))
    else {
        return;
    };
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-prompt");
    let project = axcenity_project::load(&folder).unwrap();
    let stylesheets: Vec<_> = project
        .stylesheets
        .iter()
        .map(|file| axcenity_ui::parse_stylesheet(&file.path, &file.source).0)
        .collect();
    let camera = game
        .characters
        .view(&game.world)
        .expect("the character owns the camera");
    let shown = axcenity_script::CameraView {
        position: camera.position,
        orientation: camera.orientation,
        field_of_view: axcenity_script::DEFAULT_FIELD_OF_VIEW,
    }
    .render_camera();
    let mut renderer = TextureRenderer::new(&device, &queue, VIEW.0, VIEW.1);
    for _ in 0..2 {
        let mut draw_list = axcenity_scene::build_draw_list_with_ui(
            &game.world,
            &game.assets,
            shown,
            VIEW,
            &stylesheets,
        );
        draw_list.ui.extend(axcenity_ui::prompt_items(
            &game.world,
            &prompt_views(game.scripts.shown_prompts()),
            &stylesheets,
            &shown,
            VIEW.0,
            VIEW.1,
        ));
        renderer.render(&draw_list).unwrap();
    }
    let pixels = renderer.read_pixels();
    let file = std::fs::File::create(path).unwrap();
    let mut encoder = png::Encoder::new(file, VIEW.0, VIEW.1);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(&pixels)
        .unwrap();
}
