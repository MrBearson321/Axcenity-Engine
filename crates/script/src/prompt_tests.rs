//! Tests for Phase 44: ProximityPrompts and ClickDetectors, stepped with
//! input as a host feeds it.

use axcenity_core::input::{InputEvent, InputKind};
use axcenity_core::{ClassName, InstanceId, KeyCode, UserInputType, Value, Vector3, World};

use super::*;

const FRAME: f64 = 1.0 / 60.0;

fn start(world: &mut World, runtime: &mut ScriptRuntime, source: &str) {
    let script = world.create(ClassName::Script).unwrap();
    world
        .set_parent(script, Some(world.server_script_service()))
        .unwrap();
    runtime.start_script(world, script, "scripts/test.server.lua", source);
}

fn output(runtime: &mut ScriptRuntime) -> Vec<String> {
    runtime
        .take_output()
        .into_iter()
        .map(|line| match line.kind {
            OutputKind::Error => format!("error: {}", line.text),
            _ => line.text.replace('\t', " "),
        })
        .collect()
}

fn part(world: &mut World, name: &str, position: Vector3, size: Vector3) -> InstanceId {
    let part = world.create(ClassName::Part).unwrap();
    world.set_name(part, name).unwrap();
    world.set_parent(part, Some(world.workspace())).unwrap();
    for (property, value) in [
        ("Position", Value::Vector3(position)),
        ("Size", Value::Vector3(size)),
        ("Anchored", Value::Bool(true)),
    ] {
        world.set_property(part, property, value).unwrap();
    }
    part
}

/// A character (a Model with a Body and a ThirdPersonController) whose
/// middle is at `x`, 2.5, 0.
fn character(world: &mut World, x: f32) -> InstanceId {
    let model = world.create(ClassName::Model).unwrap();
    world.set_name(model, "Character").unwrap();
    world.set_parent(model, Some(world.workspace())).unwrap();
    let body = part(
        world,
        "Body",
        Vector3::new(x, 2.5, 0.0),
        Vector3::new(2.0, 5.0, 1.0),
    );
    world.set_parent(body, Some(model)).unwrap();
    let controller = world.create(ClassName::ThirdPersonController).unwrap();
    world.set_parent(controller, Some(model)).unwrap();
    body
}

fn move_to(world: &mut World, body: InstanceId, x: f32) {
    world
        .set_property(body, "Position", Value::Vector3(Vector3::new(x, 2.5, 0.0)))
        .unwrap();
}

/// A Door at x = 0 with a prompt "Open · Door" held for `hold` seconds.
fn door(world: &mut World, hold: f64) -> InstanceId {
    let door = part(
        world,
        "Door",
        Vector3::new(0.0, 2.5, 0.0),
        Vector3::new(1.0, 5.0, 4.0),
    );
    let prompt = world.create(ClassName::ProximityPrompt).unwrap();
    world.set_parent(prompt, Some(door)).unwrap();
    for (property, value) in [
        ("ActionText", Value::String("Open".into())),
        ("ObjectText", Value::String("Door".into())),
        ("HoldDuration", Value::Number(hold)),
    ] {
        world.set_property(prompt, property, value).unwrap();
    }
    prompt
}

const LISTEN: &str = r#"
local prompt = workspace.Door.ProximityPrompt
for _, name in {"Triggered", "TriggerEnded", "PromptButtonHoldBegan", "PromptButtonHoldEnded"} do
    prompt[name]:Connect(function(player) print(name, player.Name) end)
end
prompt.PromptShown:Connect(function() print("PromptShown") end)
prompt.PromptHidden:Connect(function() print("PromptHidden") end)
"#;

fn key(runtime: &mut ScriptRuntime, world: &mut World, key: KeyCode, pressed: bool) {
    runtime.input_events(world, &[InputEvent::new(InputKind::Key { key, pressed })]);
}

/// Steps `frames` frames with no new input.
fn frames(runtime: &mut ScriptRuntime, world: &mut World, frames: usize) {
    for _ in 0..frames {
        runtime.input_events(world, &[]);
        runtime.step_prompts(world, FRAME);
    }
}

#[test]
fn walking_up_to_a_door_shows_it_and_holding_e_for_hold_duration_triggers_once() {
    let mut world = World::new();
    let body = character(&mut world, 20.0);
    let prompt = door(&mut world, 0.5);
    let mut runtime = ScriptRuntime::new();
    start(&mut world, &mut runtime, LISTEN);
    frames(&mut runtime, &mut world, 2);
    // 20 studs away: out of its 10 studs' reach.
    assert!(runtime.shown_prompts().is_empty());

    move_to(&mut world, body, 5.0);
    frames(&mut runtime, &mut world, 1);
    let shown = runtime.shown_prompts();
    assert_eq!(shown.len(), 1);
    assert_eq!((shown[0].prompt, shown[0].key.as_str()), (prompt, "E"));
    assert_eq!(output(&mut runtime), ["PromptShown"]);

    key(&mut runtime, &mut world, KeyCode::E, true);
    runtime.step_prompts(&mut world, FRAME);
    // Most of the way, not there yet.
    frames(&mut runtime, &mut world, 24);
    let progress = runtime.shown_prompts()[0].progress;
    assert!((0.7..1.0).contains(&progress), "{progress}");
    assert_eq!(output(&mut runtime), ["PromptButtonHoldBegan Player"]);
    // Past 0.5 seconds, and held on for two more.
    frames(&mut runtime, &mut world, 6);
    frames(&mut runtime, &mut world, 120);
    assert_eq!(
        output(&mut runtime),
        ["PromptButtonHoldEnded Player", "Triggered Player"]
    );
    assert_eq!(runtime.shown_prompts()[0].progress, 1.0);

    key(&mut runtime, &mut world, KeyCode::E, false);
    runtime.step_prompts(&mut world, FRAME);
    assert_eq!(output(&mut runtime), ["TriggerEnded Player"]);
    assert_eq!(runtime.shown_prompts()[0].progress, 0.0);

    move_to(&mut world, body, 30.0);
    frames(&mut runtime, &mut world, 1);
    assert_eq!(output(&mut runtime), ["PromptHidden"]);
}

#[test]
fn letting_go_early_ends_the_hold_without_triggering() {
    let mut world = World::new();
    character(&mut world, 4.0);
    door(&mut world, 0.5);
    let mut runtime = ScriptRuntime::new();
    start(&mut world, &mut runtime, LISTEN);
    frames(&mut runtime, &mut world, 1);
    key(&mut runtime, &mut world, KeyCode::E, true);
    runtime.step_prompts(&mut world, FRAME);
    frames(&mut runtime, &mut world, 10);
    key(&mut runtime, &mut world, KeyCode::E, false);
    runtime.step_prompts(&mut world, FRAME);
    frames(&mut runtime, &mut world, 60);
    assert_eq!(
        output(&mut runtime),
        [
            "PromptShown",
            "PromptButtonHoldBegan Player",
            "PromptButtonHoldEnded Player"
        ]
    );
}

#[test]
fn a_controller_shows_and_uses_the_bound_button() {
    let mut world = World::new();
    character(&mut world, 4.0);
    door(&mut world, 0.0);
    let mut runtime = ScriptRuntime::new();
    start(&mut world, &mut runtime, LISTEN);
    frames(&mut runtime, &mut world, 1);
    assert_eq!(runtime.shown_prompts()[0].key, "E");
    let pad = |pressed| {
        InputEvent::new(InputKind::PadButton {
            pad: 0,
            key: KeyCode::ButtonX,
            pressed,
        })
    };
    runtime.input_events(&mut world, &[pad(true)]);
    runtime.step_prompts(&mut world, FRAME);
    assert_eq!(runtime.shown_prompts()[0].key, "X");
    frames(&mut runtime, &mut world, 30);
    runtime.input_events(&mut world, &[pad(false)]);
    runtime.step_prompts(&mut world, FRAME);
    assert_eq!(
        output(&mut runtime),
        ["PromptShown", "Triggered Player", "TriggerEnded Player"]
    );
    // A key press goes back to showing the key.
    key(&mut runtime, &mut world, KeyCode::Q, true);
    runtime.step_prompts(&mut world, FRAME);
    assert_eq!(runtime.shown_prompts()[0].key, "E");
}

#[test]
fn walking_up_with_the_key_already_held_does_not_trigger() {
    let mut world = World::new();
    let body = character(&mut world, 20.0);
    door(&mut world, 0.0);
    let mut runtime = ScriptRuntime::new();
    start(&mut world, &mut runtime, LISTEN);
    key(&mut runtime, &mut world, KeyCode::E, true);
    runtime.step_prompts(&mut world, FRAME);
    move_to(&mut world, body, 4.0);
    frames(&mut runtime, &mut world, 30);
    assert_eq!(output(&mut runtime), ["PromptShown"]);
}

#[test]
fn a_prompt_behind_a_wall_does_not_show_when_it_needs_line_of_sight() {
    let mut world = World::new();
    character(&mut world, 6.0);
    let prompt = door(&mut world, 0.0);
    part(
        &mut world,
        "Wall",
        Vector3::new(3.0, 2.5, 0.0),
        Vector3::new(0.5, 8.0, 8.0),
    );
    let mut runtime = ScriptRuntime::new();
    frames(&mut runtime, &mut world, 2);
    assert!(runtime.shown_prompts().is_empty());
    world
        .set_property(prompt, "RequiresLineOfSight", Value::Bool(false))
        .unwrap();
    frames(&mut runtime, &mut world, 1);
    assert_eq!(runtime.shown_prompts().len(), 1);
}

#[test]
fn only_the_nearest_prompt_for_an_action_shows_unless_it_always_shows() {
    let mut world = World::new();
    character(&mut world, 2.0);
    let near = door(&mut world, 0.0);
    let far_part = part(
        &mut world,
        "Chest",
        Vector3::new(6.0, 2.5, 0.0),
        Vector3::new(2.0, 2.0, 2.0),
    );
    // On an Attachment 1 stud up from the chest.
    let attachment = world.create(ClassName::Attachment).unwrap();
    world.set_parent(attachment, Some(far_part)).unwrap();
    world
        .set_property(
            attachment,
            "Position",
            Value::Vector3(Vector3::new(0.0, 1.0, 0.0)),
        )
        .unwrap();
    let far = world.create(ClassName::ProximityPrompt).unwrap();
    world.set_parent(far, Some(attachment)).unwrap();
    let mut runtime = ScriptRuntime::new();
    frames(&mut runtime, &mut world, 1);
    let ids: Vec<_> = runtime.shown_prompts().iter().map(|p| p.prompt).collect();
    assert_eq!(ids, [near]);

    world
        .set_property(
            far,
            "Exclusivity",
            Value::EnumItem(axcenity_core::EnumItem {
                enum_name: "ProximityPromptExclusivity".into(),
                item: "AlwaysShow".into(),
            }),
        )
        .unwrap();
    frames(&mut runtime, &mut world, 1);
    let shown = runtime.shown_prompts();
    assert_eq!(shown.len(), 2);
    let chest = shown.iter().find(|p| p.prompt == far).unwrap();
    assert_eq!(chest.position, Vector3::new(6.0, 3.5, 0.0));
}

/// A camera at 0, 5, 20 looking along -Z at a Button part at 0, 5, 0
/// with a ClickDetector, and the mouse in the middle of an 800×600 view.
fn clickable(world: &mut World, runtime: &mut ScriptRuntime) -> InstanceId {
    let button = part(
        world,
        "Button",
        Vector3::new(0.0, 5.0, 0.0),
        Vector3::new(4.0, 4.0, 1.0),
    );
    let detector = world.create(ClassName::ClickDetector).unwrap();
    world.set_parent(detector, Some(button)).unwrap();
    runtime.set_viewport_size(800.0, 600.0);
    runtime.set_camera(CameraState {
        position: Vector3::new(0.0, 5.0, 20.0),
        orientation: Vector3::new(0.0, 0.0, 0.0),
    });
    start(
        world,
        runtime,
        r#"
local detector = workspace.Button.ClickDetector
for _, name in {"MouseClick", "RightMouseClick", "MouseHoverEnter", "MouseHoverLeave"} do
    detector[name]:Connect(function(player) print(name, player.Name) end)
end
"#,
    );
    detector
}

fn mouse(runtime: &mut ScriptRuntime, world: &mut World, events: &[InputKind]) {
    let events: Vec<_> = events.iter().copied().map(InputEvent::new).collect();
    runtime.input_events(world, &events);
    runtime.step_prompts(world, FRAME);
}

fn moved(position: [f32; 2]) -> InputKind {
    InputKind::MouseMove {
        position,
        delta: [0.0, 0.0],
    }
}

fn button(button: UserInputType, pressed: bool) -> InputKind {
    InputKind::MouseButton {
        button,
        pressed,
        position: [400.0, 300.0],
    }
}

#[test]
fn a_click_detector_added_during_the_game_is_pointed_at() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    let button = part(
        &mut world,
        "Button",
        Vector3::new(0.0, 5.0, 0.0),
        Vector3::new(4.0, 4.0, 1.0),
    );
    runtime.set_viewport_size(800.0, 600.0);
    runtime.set_camera(CameraState {
        position: Vector3::new(0.0, 5.0, 20.0),
        orientation: Vector3::new(0.0, 0.0, 0.0),
    });
    start(&mut world, &mut runtime, "");
    mouse(&mut runtime, &mut world, &[moved([400.0, 300.0])]);
    assert_eq!(runtime.click_cursor(&world), None);
    let detector = world.create(ClassName::ClickDetector).unwrap();
    world.set_parent(detector, Some(button)).unwrap();
    mouse(&mut runtime, &mut world, &[]);
    assert_eq!(runtime.click_cursor(&world).as_deref(), Some("pointer"));
}

#[test]
fn clicking_a_part_with_a_click_detector_fires_mouse_click_and_hover_changes_the_cursor() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    let detector = clickable(&mut world, &mut runtime);
    mouse(&mut runtime, &mut world, &[moved([20.0, 20.0])]);
    assert_eq!(runtime.click_cursor(&world), None);
    mouse(&mut runtime, &mut world, &[moved([400.0, 300.0])]);
    assert_eq!(runtime.click_cursor(&world).as_deref(), Some("pointer"));
    world
        .set_property(detector, "CursorIcon", Value::String("crosshair".into()))
        .unwrap();
    assert_eq!(runtime.click_cursor(&world).as_deref(), Some("crosshair"));

    let left = UserInputType::MouseButton1;
    let right = UserInputType::MouseButton2;
    mouse(&mut runtime, &mut world, &[button(left, true)]);
    mouse(&mut runtime, &mut world, &[button(left, false)]);
    mouse(
        &mut runtime,
        &mut world,
        &[button(right, true), button(right, false)],
    );
    // A click the UI took isn't the part's.
    runtime.input_events(
        &mut world,
        &[
            InputEvent::processed(button(left, true)),
            InputEvent::new(button(left, false)),
        ],
    );
    runtime.step_prompts(&mut world, FRAME);
    // Pressed on the part and let go off it: no click.
    mouse(&mut runtime, &mut world, &[button(left, true)]);
    mouse(
        &mut runtime,
        &mut world,
        &[
            moved([20.0, 20.0]),
            InputKind::MouseButton {
                button: left,
                pressed: false,
                position: [20.0, 20.0],
            },
        ],
    );
    assert_eq!(runtime.click_cursor(&world), None);
    assert_eq!(
        output(&mut runtime),
        [
            "MouseHoverEnter Player",
            "MouseClick Player",
            "RightMouseClick Player",
            "MouseHoverLeave Player"
        ]
    );
}

#[test]
fn a_click_outside_max_activation_distance_does_nothing() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    let detector = clickable(&mut world, &mut runtime);
    // The camera is 19.5 studs from the part's face.
    world
        .set_property(detector, "MaxActivationDistance", Value::Number(10.0))
        .unwrap();
    let left = UserInputType::MouseButton1;
    mouse(&mut runtime, &mut world, &[moved([400.0, 300.0])]);
    mouse(&mut runtime, &mut world, &[button(left, true)]);
    mouse(&mut runtime, &mut world, &[button(left, false)]);
    assert_eq!(runtime.click_cursor(&world), None);
    assert!(output(&mut runtime).is_empty());

    // In reach of a character standing near it, the same click counts;
    // the character's own body doesn't block the ray.
    character(&mut world, 0.0);
    let body = world
        .find_first_descendant(world.workspace(), "Body")
        .unwrap();
    world
        .set_property(
            body,
            "Position",
            Value::Vector3(Vector3::new(0.0, 5.0, 8.0)),
        )
        .unwrap();
    mouse(&mut runtime, &mut world, &[button(left, true)]);
    mouse(&mut runtime, &mut world, &[button(left, false)]);
    assert_eq!(
        output(&mut runtime),
        ["MouseHoverEnter Player", "MouseClick Player"]
    );
}
