//! Phase 45: the hello-tools example's sword, torch and shield, in the
//! player's frame order with physics.

use std::path::{Path, PathBuf};

use axcenity_core::input::{InputEvent, InputKind};
use axcenity_core::tool::equipped;
use axcenity_core::{InstanceId, KeyCode, UserInputType, Value, Vector3};
use axcenity_render::UiDrawItem;
use axcenity_script::CameraState;

use super::tests::copy_folder;
use super::*;

const FRAME: f32 = 1.0 / 60.0;
const VIEW: (u32, u32) = (480, 270);

/// The example running in the player's frame order: the character, input,
/// prompts and Tools, scripts, physics, then Tools in hand to the hand.
struct Game {
    world: World,
    scripts: ScriptRuntime,
    assets: Assets,
    characters: Controllers,
    physics: Physics,
    stylesheets: Vec<axcenity_ui::Stylesheet>,
    printed: Vec<String>,
}

impl Game {
    /// hello-tools from a copy, with its save slots in a folder of the
    /// test's own.
    fn start(test: &str) -> Self {
        let example = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-tools");
        let folder = std::env::temp_dir().join(format!(
            "axcenity-player-tools-{test}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&folder);
        copy_folder(&example, &folder);
        let project = axcenity_project::load(&folder).unwrap();
        let mut world = project.world;
        let assets = Assets::new(&folder);
        let mut scripts = ScriptRuntime::new();
        scripts.set_assets(assets.clone());
        scripts.set_save_folder(Some(folder.join("saves")));
        scripts.set_viewport_size(VIEW.0 as f32, VIEW.1 as f32);
        for script in &project.scripts {
            scripts.start_script(&mut world, script.instance, &script.path, &script.source);
        }
        let stylesheets = project
            .stylesheets
            .iter()
            .map(|file| axcenity_ui::parse_stylesheet(&file.path, &file.source).0)
            .collect();
        let mut game = Self {
            world,
            scripts,
            physics: Physics::with_assets(assets.clone()),
            assets,
            characters: Controllers::default(),
            stylesheets,
            printed: Vec::new(),
        };
        game.take_printed();
        assert!(game.printed().is_empty(), "the scripts start cleanly");
        game
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
        self.scripts.step_tools(&mut self.world);
        self.scripts.step(&mut self.world, f64::from(FRAME));
        self.characters.keep_kinematic(&mut self.world);
        self.physics.step(&mut self.world, f64::from(FRAME));
        let touches = self.physics.take_touches();
        self.scripts.fire_touched(&mut self.world, &touches);
        self.scripts
            .place_tools(&mut self.world, &axcenity_anim::Poses::new());
        self.take_printed();
    }

    fn take_printed(&mut self) {
        self.printed.extend(
            self.scripts
                .take_output()
                .into_iter()
                .map(|line| line.text.replace('\t', " ")),
        );
    }

    fn frames(&mut self, count: usize) {
        for _ in 0..count {
            self.frame(MoveInput::default(), &[]);
        }
    }

    /// Presses and lets go of `key`, a frame each, then waits a frame.
    fn tap(&mut self, key: KeyCode) {
        for pressed in [true, false] {
            let event = InputEvent::new(InputKind::Key { key, pressed });
            self.frame(MoveInput::default(), &[event]);
        }
        self.frames(1);
    }

    fn click(&mut self) {
        for pressed in [true, false] {
            let event = InputEvent::new(InputKind::MouseButton {
                button: UserInputType::MouseButton1,
                pressed,
                position: [240.0, 135.0],
            });
            self.frame(MoveInput::default(), &[event]);
        }
    }

    fn find(&self, path: &[&str]) -> InstanceId {
        path.iter().fold(self.world.workspace(), |at, name| {
            self.world.find_first_child(at, name).unwrap()
        })
    }

    fn inventory(&self) -> InstanceId {
        self.find(&["Character", "Inventory"])
    }

    fn parent(&self, id: InstanceId) -> Option<InstanceId> {
        self.world.get(id).unwrap().parent()
    }

    fn position(&self, part: InstanceId) -> Vector3 {
        self.world
            .get(part)
            .unwrap()
            .property("Position")
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

    /// Walks forward (-Z) until `tool` is in the Inventory.
    fn walk_onto(&mut self, tool: InstanceId) {
        let forward = MoveInput {
            forward: true,
            ..MoveInput::default()
        };
        for _ in 0..300 {
            if self.parent(tool) == Some(self.inventory()) {
                return;
            }
            self.frame(forward, &[]);
        }
        panic!("never picked it up; printed {:?}", self.printed);
    }

    /// The texts the hotbar draws.
    fn hotbar(&self) -> Vec<String> {
        let inventory = self.scripts.player_inventory(&self.world).unwrap();
        axcenity_ui::hotbar_items(&self.world, inventory, &self.stylesheets, VIEW.0, VIEW.1)
            .into_iter()
            .filter_map(|item| match item {
                UiDrawItem::Text(text) => Some(text.text),
                _ => None,
            })
            .collect()
    }

    fn printed(&mut self) -> Vec<String> {
        std::mem::take(&mut self.printed)
    }
}

#[test]
fn picking_up_the_sword_shows_it_in_the_hotbar_and_one_and_a_click_swing_it_in_the_right_hand() {
    let mut game = Game::start("swing");
    game.frames(5);
    let sword = game.find(&["Sword"]);
    assert!(game.hotbar().is_empty(), "no Tools, no hotbar");

    game.walk_onto(sword);
    assert_eq!(game.printed(), ["Picked up the Sword (1 of 2)"]);
    assert_eq!(game.hotbar(), ["Sword", "1"]);
    // Put away, it's out of the world: still where it was picked up.
    let handle = game.world.find_first_child(sword, "Handle").unwrap();
    let lying = game.position(handle);
    game.frames(30);
    assert_eq!(game.position(handle), lying);

    game.tap(KeyCode::One);
    assert_eq!(equipped(&game.world, game.inventory()), Some(sword));
    assert_eq!(game.printed(), ["In hand: Sword"]);
    // The ToolTip shows over the bar.
    assert_eq!(game.hotbar(), ["Sword", "1", "Click to swing"]);
    // In the right hand: the body's right (+X) and front (-Z), pointing
    // forward, and it stays there as physics steps.
    let body = game.position(game.find(&["Character", "Body"]));
    let held = game.position(handle);
    assert!(held.x > body.x + 1.0, "{held:?} beside {body:?}");
    assert!(held.z < body.z - 1.0, "{held:?} in front of {body:?}");
    assert!((held.y - body.y).abs() < 0.5, "{held:?} beside {body:?}");
    game.frames(30);
    assert_eq!(game.position(handle), held);

    game.click();
    assert_eq!(game.printed(), ["Sword hits for 10"]);

    if let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS") {
        snapshot(&game, &PathBuf::from(folder).join("tools-sword.png"));
    }
}

#[test]
fn a_full_inventory_leaves_the_shield_on_the_ground() {
    let mut game = Game::start("full");
    game.frames(5);
    let (sword, torch, shield) = (
        game.find(&["Sword"]),
        game.find(&["Torch"]),
        game.find(&["Shield"]),
    );
    game.walk_onto(sword);
    game.stand_at(6.0, -8.0);
    game.frames(2);
    assert_eq!(game.parent(torch), Some(game.inventory()));
    assert_eq!(
        game.printed(),
        [
            "Picked up the Sword (1 of 2)",
            "Picked up the Torch (2 of 2)"
        ]
    );

    game.stand_at(-6.0, -8.0);
    game.frames(30);
    assert_eq!(game.parent(shield), Some(game.world.workspace()));
    assert!(game.printed().is_empty());
    assert_eq!(game.hotbar(), ["Sword", "1", "Torch", "2"]);
    // Dropping the sword makes room.
    game.tap(KeyCode::One);
    game.tap(KeyCode::Backspace);
    assert_eq!(game.parent(sword), Some(game.world.workspace()));
    game.frames(2);
    assert_eq!(game.parent(shield), Some(game.inventory()));
}

#[test]
fn saving_and_loading_a_slot_restores_the_inventory_and_the_tool_in_hand() {
    let mut game = Game::start("saves");
    game.frames(5);
    let (sword, torch) = (game.find(&["Sword"]), game.find(&["Torch"]));
    game.walk_onto(sword);
    game.stand_at(6.0, -8.0);
    game.frames(2);
    game.tap(KeyCode::One);
    game.printed();
    game.tap(KeyCode::K);
    assert_eq!(game.printed(), ["Saved to slot 1"]);

    // Drop the sword and walk away; the torch in hand.
    game.tap(KeyCode::Backspace);
    game.stand_at(20.0, 10.0);
    game.frames(2);
    game.tap(KeyCode::One);
    assert_eq!(equipped(&game.world, game.inventory()), Some(torch));
    assert_eq!(game.parent(sword), Some(game.world.workspace()));
    game.printed();

    game.tap(KeyCode::L);
    // The same Tools, so the scripts' handlers still work.
    assert_eq!(
        inventory_items(&game),
        [sword, torch],
        "printed {:?}",
        game.printed
    );
    assert_eq!(equipped(&game.world, game.inventory()), Some(sword));
    // The example's ChildAdded handler sees the sword come back.
    assert_eq!(
        game.printed(),
        [
            "Loaded slot 1",
            "Picked up the Sword (2 of 2)",
            "In hand: Sword"
        ]
    );
    game.click();
    assert_eq!(game.printed(), ["Sword hits for 10"]);
    // It's in the hand where the character was when it saved.
    let body = game.position(game.find(&["Character", "Body"]));
    let handle = game.world.find_first_child(sword, "Handle").unwrap();
    assert!(game.position(handle).x > body.x + 1.0);
    assert!((body.x - 6.0).abs() < 0.5, "{body:?}");
}

fn inventory_items(game: &Game) -> Vec<InstanceId> {
    game.world
        .get(game.inventory())
        .unwrap()
        .children()
        .to_vec()
}

fn snapshot(game: &Game, path: &Path) {
    use axcenity_render::{TextureRenderer, wgpu};
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let Ok(adapter) = pollster::block_on(instance.request_adapter(&Default::default())) else {
        return;
    };
    let Ok((device, queue)) = pollster::block_on(adapter.request_device(&Default::default()))
    else {
        return;
    };
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
    let inventory = game.scripts.player_inventory(&game.world).unwrap();
    for _ in 0..2 {
        let mut draw_list = axcenity_scene::build_draw_list_with_ui(
            &game.world,
            &game.assets,
            shown,
            VIEW,
            &game.stylesheets,
        );
        draw_list.ui.extend(axcenity_ui::hotbar_items(
            &game.world,
            inventory,
            &game.stylesheets,
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
