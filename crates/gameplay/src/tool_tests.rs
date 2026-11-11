//! Tests for Phase 45: Tools and Inventories, stepped with input as a host
//! feeds it.

use axcenity_core::input::{InputEvent, InputKind};
use axcenity_core::tool::equipped;
use axcenity_core::{
    ClassName, InstanceId, KeyCode, Parameter, UserInputType, Value, Vector3, World,
};

use axcenity_script::{OutputKind, ScriptRuntime};

use crate::{player_inventory, script_runtime, step_tools, tool};

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

fn part(
    world: &mut World,
    parent: InstanceId,
    name: &str,
    position: Vector3,
    size: Vector3,
) -> InstanceId {
    let part = world.create(ClassName::Part).unwrap();
    world.set_name(part, name).unwrap();
    world.set_parent(part, Some(parent)).unwrap();
    for (property, value) in [
        ("Position", Value::Vector3(position)),
        ("Size", Value::Vector3(size)),
        ("Anchored", Value::Bool(true)),
    ] {
        world.set_property(part, property, value).unwrap();
    }
    part
}

/// The player character: a Model with a 2 × 5 × 1 Body whose middle is at
/// `x`, 2.5, 0, a ThirdPersonController and an Inventory of `capacity`.
/// Returns the Body and the Inventory.
fn character(world: &mut World, x: f32, capacity: f64) -> (InstanceId, InstanceId) {
    let model = world.create(ClassName::Model).unwrap();
    world.set_name(model, "Character").unwrap();
    world.set_parent(model, Some(world.workspace())).unwrap();
    let body = part(
        world,
        model,
        "Body",
        Vector3::new(x, 2.5, 0.0),
        Vector3::new(2.0, 5.0, 1.0),
    );
    let controller = world.create(ClassName::ThirdPersonController).unwrap();
    world.set_parent(controller, Some(model)).unwrap();
    let inventory = world.create(ClassName::Inventory).unwrap();
    world.set_name(inventory, "Inventory").unwrap();
    world
        .set_property(inventory, "Capacity", Value::Number(capacity))
        .unwrap();
    world.set_parent(inventory, Some(model)).unwrap();
    (body, inventory)
}

/// A Tool `name` lying in the Workspace with its Handle's middle at
/// `position`, and a Grip at the Handle's back end.
fn tool(world: &mut World, name: &str, position: Vector3) -> InstanceId {
    let tool = world.create(ClassName::Tool).unwrap();
    world.set_name(tool, name).unwrap();
    world.set_parent(tool, Some(world.workspace())).unwrap();
    let handle = part(world, tool, "Handle", position, Vector3::new(0.4, 0.4, 4.0));
    let grip = world.create(ClassName::Attachment).unwrap();
    world.set_name(grip, "Grip").unwrap();
    world
        .set_property(
            grip,
            "Position",
            Value::Vector3(Vector3::new(0.0, 0.0, 1.5)),
        )
        .unwrap();
    world.set_parent(grip, Some(handle)).unwrap();
    tool
}

fn handle_position(world: &World, tool: InstanceId) -> Vector3 {
    let handle = world.find_first_child(tool, "Handle").unwrap();
    world
        .get(handle)
        .unwrap()
        .property("Position")
        .and_then(Value::as_vector3)
        .unwrap()
}

fn parent(world: &World, id: InstanceId) -> Option<InstanceId> {
    world.get(id).unwrap().parent()
}

/// One frame of the input step with `events`, then the tools step.
fn frame(runtime: &mut ScriptRuntime, world: &mut World, events: &[InputEvent]) {
    runtime.input_events(world, events);
    runtime.step_prompts(world, 1.0 / 60.0);
    step_tools(runtime, world);
}

fn key(runtime: &mut ScriptRuntime, world: &mut World, key: KeyCode, pressed: bool) {
    frame(
        runtime,
        world,
        &[InputEvent::new(InputKind::Key { key, pressed })],
    );
}

fn click(runtime: &mut ScriptRuntime, world: &mut World, pressed: bool) {
    let event = InputEvent::new(InputKind::MouseButton {
        button: UserInputType::MouseButton1,
        pressed,
        position: [10.0, 10.0],
    });
    frame(runtime, world, &[event]);
}

const LISTEN: &str = r#"
for _, tool in workspace:GetChildren() do
    if tool:IsA("Tool") then
        for _, name in {"Equipped", "Unequipped", "Activated", "Deactivated"} do
            tool[name]:Connect(function() print(name, tool.Name) end)
        end
    end
end
"#;

#[test]
fn touching_a_tool_picks_it_up_one_equips_it_in_the_right_hand_and_clicking_activates_it() {
    let mut world = World::new();
    let (_, inventory) = character(&mut world, 0.0, 10.0);
    // Lying across the character's feet.
    let sword = tool(&mut world, "Sword", Vector3::new(0.5, 0.2, 0.0));
    let mut runtime = script_runtime();
    start(&mut world, &mut runtime, LISTEN);

    frame(&mut runtime, &mut world, &[]);
    assert_eq!(parent(&world, sword), Some(inventory));
    assert_eq!(equipped(&world, inventory), None);
    assert_eq!(tool::items(&world, inventory), [sword]);
    assert_eq!(player_inventory(&world), Some(inventory));
    assert!(output(&mut runtime).is_empty());

    key(&mut runtime, &mut world, KeyCode::One, true);
    assert_eq!(equipped(&world, inventory), Some(sword));
    assert_eq!(output(&mut runtime), ["Equipped Sword"]);
    // The Grip (the Handle's back end) is at the body's right side, in
    // front, so the blade points forward (-Z).
    let at = handle_position(&world, sword);
    assert!((at.x - 1.5).abs() < 1e-4, "{at:?}");
    assert!((at.y - 2.5).abs() < 1e-4, "{at:?}");
    assert!((at.z - (-1.0 - 1.5)).abs() < 1e-4, "{at:?}");
    key(&mut runtime, &mut world, KeyCode::One, false);

    click(&mut runtime, &mut world, true);
    assert_eq!(output(&mut runtime), ["Activated Sword"]);
    // Holding on fires nothing more.
    frame(&mut runtime, &mut world, &[]);
    assert!(output(&mut runtime).is_empty());
    click(&mut runtime, &mut world, false);
    assert_eq!(output(&mut runtime), ["Deactivated Sword"]);

    // 1 again puts it away.
    key(&mut runtime, &mut world, KeyCode::One, true);
    assert_eq!(equipped(&world, inventory), None);
    assert_eq!(parent(&world, sword), Some(inventory));
    assert_eq!(output(&mut runtime), ["Unequipped Sword"]);
}

#[test]
fn a_full_inventory_refuses_a_pickup_and_leaves_the_tool_on_the_ground() {
    let mut world = World::new();
    let (_, inventory) = character(&mut world, 0.0, 1.0);
    let sword = tool(&mut world, "Sword", Vector3::new(0.5, 0.2, 0.0));
    let shield = tool(&mut world, "Shield", Vector3::new(-0.5, 0.2, 0.0));
    let mut runtime = script_runtime();
    for _ in 0..5 {
        frame(&mut runtime, &mut world, &[]);
    }
    assert_eq!(tool::items(&world, inventory), [sword]);
    assert_eq!(parent(&world, shield), Some(world.workspace()));
    assert_eq!(
        handle_position(&world, shield),
        Vector3::new(-0.5, 0.2, 0.0),
        "it stays where it was"
    );
    // Scripts get false too.
    start(
        &mut world,
        &mut runtime,
        "print(workspace.Character.Inventory:Add(workspace.Shield))",
    );
    assert_eq!(output(&mut runtime), ["false"]);
    assert_eq!(parent(&world, shield), Some(world.workspace()));
}

#[test]
fn dropping_and_picking_up_again_gives_back_the_same_tool_with_its_attributes() {
    let mut world = World::new();
    let (body, inventory) = character(&mut world, 0.0, 10.0);
    let sword = tool(&mut world, "Sword", Vector3::new(0.5, 0.2, 0.0));
    world
        .set_attribute(sword, "Sharpness", Some(Value::Number(3.0)))
        .unwrap();
    let mut runtime = script_runtime();
    start(&mut world, &mut runtime, LISTEN);
    frame(&mut runtime, &mut world, &[]);
    key(&mut runtime, &mut world, KeyCode::One, true);
    key(&mut runtime, &mut world, KeyCode::One, false);
    output(&mut runtime);

    key(&mut runtime, &mut world, KeyCode::Backspace, true);
    assert_eq!(output(&mut runtime), ["Unequipped Sword"]);
    assert_eq!(parent(&world, sword), Some(world.workspace()));
    assert_eq!(equipped(&world, inventory), None);
    assert!(tool::items(&world, inventory).is_empty());
    // In front of the character (-Z), clear of it.
    let dropped = handle_position(&world, sword);
    assert!(dropped.z < -2.5, "{dropped:?}");
    key(&mut runtime, &mut world, KeyCode::Backspace, false);
    for _ in 0..3 {
        frame(&mut runtime, &mut world, &[]);
    }
    assert_eq!(parent(&world, sword), Some(world.workspace()));

    // Walking onto it picks the same Tool up again.
    world
        .set_property(
            body,
            "Position",
            Value::Vector3(Vector3::new(dropped.x, 2.5, dropped.z)),
        )
        .unwrap();
    frame(&mut runtime, &mut world, &[]);
    assert_eq!(tool::items(&world, inventory), [sword]);
    assert_eq!(
        world.get(sword).unwrap().attribute("Sharpness"),
        Some(&Value::Number(3.0))
    );

    // tool:Drop() does the same from a script; a character standing where
    // it lands doesn't pick it straight back up.
    start(
        &mut world,
        &mut runtime,
        "print(workspace.Character.Inventory.Sword:Drop())",
    );
    assert_eq!(output(&mut runtime), ["true"]);
    assert_eq!(parent(&world, sword), Some(world.workspace()));
    let front = handle_position(&world, sword);
    world
        .set_property(
            body,
            "Position",
            Value::Vector3(Vector3::new(front.x, 2.5, front.z)),
        )
        .unwrap();
    frame(&mut runtime, &mut world, &[]);
    assert_eq!(
        parent(&world, sword),
        Some(world.workspace()),
        "still standing on it"
    );
    world
        .set_property(
            body,
            "Position",
            Value::Vector3(Vector3::new(20.0, 2.5, 0.0)),
        )
        .unwrap();
    frame(&mut runtime, &mut world, &[]);
    world
        .set_property(
            body,
            "Position",
            Value::Vector3(Vector3::new(front.x, 2.5, front.z)),
        )
        .unwrap();
    frame(&mut runtime, &mut world, &[]);
    assert_eq!(parent(&world, sword), Some(inventory), "back on it");
}

#[test]
fn scripts_add_equip_unequip_and_remove_tools() {
    let mut world = World::new();
    let (_, inventory) = character(&mut world, 0.0, 10.0);
    // Away from the character, so only scripts pick them up.
    let sword = tool(&mut world, "Sword", Vector3::new(20.0, 0.2, 0.0));
    let torch = tool(&mut world, "Torch", Vector3::new(25.0, 0.2, 0.0));
    let mut runtime = script_runtime();
    start(&mut world, &mut runtime, LISTEN);
    let mut run = |world: &mut World, source: &str| {
        let source = format!(
            "local inventory = workspace.Character.Inventory\n\
             local function find(name)\n\
                 return inventory:FindFirstChild(name) or workspace:FindFirstChild(name)\n\
             end\n\
             local sword, torch = find('Sword'), find('Torch')\n{source}"
        );
        start(world, &mut runtime, &source);
        output(&mut runtime)
    };
    assert_eq!(
        run(
            &mut world,
            "print(inventory:Add(sword), inventory:Add(torch), #inventory:GetItems())"
        ),
        ["true true 2"]
    );
    let lines = run(
        &mut world,
        "print(pcall(inventory.Equip, inventory, inventory.Parent))",
    );
    assert!(
        lines[0].starts_with("false") && lines[0].contains("needs a Tool"),
        "{lines:?}"
    );
    // The events fire once the script has run, for what changed.
    assert_eq!(
        run(
            &mut world,
            "inventory:Equip(sword) print(inventory.Equipped == sword)"
        ),
        ["true", "Equipped Sword"]
    );
    assert_eq!(
        run(&mut world, "inventory:Equip(torch)"),
        ["Unequipped Sword", "Equipped Torch"]
    );
    let lines = run(
        &mut world,
        "print(pcall(function() inventory.Equipped = sword end))",
    );
    assert!(
        lines[0].contains("Inventory.Equipped is read-only"),
        "{lines:?}"
    );
    assert_eq!(
        run(
            &mut world,
            "inventory:UnequipAll() print(inventory.Equipped)"
        ),
        ["nil", "Unequipped Torch"]
    );
    assert_eq!(
        run(&mut world, "inventory:Equip(torch)"),
        ["Equipped Torch"]
    );
    let lines = run(
        &mut world,
        "print(inventory:Remove(torch), torch.Parent, inventory.Equipped)\n\
         print(pcall(inventory.Equip, inventory, torch))",
    );
    assert_eq!(lines[0], "true nil nil");
    assert!(
        lines[1].contains("put it in with inventory:Add(tool) first"),
        "{lines:?}"
    );
    assert_eq!(lines[2..], ["Unequipped Torch"]);
    assert_eq!(parent(&world, torch), None);
    assert_eq!(tool::items(&world, inventory), [sword]);
}

#[test]
fn equipping_sets_the_characters_animator_equipped_parameter() {
    let mut world = World::new();
    let (_, inventory) = character(&mut world, 0.0, 10.0);
    let model = parent(&world, inventory).unwrap();
    let animator = world.create(ClassName::Animator).unwrap();
    world.set_parent(animator, Some(model)).unwrap();
    // As the host primes it from the state machine's parameters.
    world
        .animator_mut(animator)
        .unwrap()
        .parameters
        .insert("Equipped".to_owned(), Parameter::Bool(false));
    let sword = tool(&mut world, "Sword", Vector3::new(0.5, 0.2, 0.0));
    let mut runtime = script_runtime();
    frame(&mut runtime, &mut world, &[]);
    assert_eq!(parent(&world, sword), Some(inventory));
    let parameter = |world: &World| world.animator(animator).unwrap().parameters["Equipped"];

    key(&mut runtime, &mut world, KeyCode::One, true);
    assert_eq!(parameter(&world), Parameter::Bool(true));
    key(&mut runtime, &mut world, KeyCode::One, false);
    key(&mut runtime, &mut world, KeyCode::One, true);
    assert_eq!(parameter(&world), Parameter::Bool(false));
}

#[test]
fn bumpers_go_through_the_slots_and_a_turned_off_hotbar_ignores_its_keys() {
    let mut world = World::new();
    let (_, inventory) = character(&mut world, 0.0, 10.0);
    let sword = tool(&mut world, "Sword", Vector3::new(0.5, 0.2, 0.0));
    let torch = tool(&mut world, "Torch", Vector3::new(-0.5, 0.2, 0.0));
    let mut runtime = script_runtime();
    frame(&mut runtime, &mut world, &[]);
    assert_eq!(tool::items(&world, inventory), [sword, torch]);
    let pad = |key, pressed| {
        InputEvent::new(InputKind::PadButton {
            pad: 0,
            key,
            pressed,
        })
    };

    frame(&mut runtime, &mut world, &[pad(KeyCode::ButtonR1, true)]);
    assert_eq!(equipped(&world, inventory), Some(sword));
    frame(&mut runtime, &mut world, &[pad(KeyCode::ButtonR1, false)]);
    frame(&mut runtime, &mut world, &[pad(KeyCode::ButtonR1, true)]);
    assert_eq!(equipped(&world, inventory), Some(torch));
    frame(&mut runtime, &mut world, &[pad(KeyCode::ButtonR1, false)]);
    frame(&mut runtime, &mut world, &[pad(KeyCode::ButtonL1, true)]);
    assert_eq!(equipped(&world, inventory), Some(sword));
    frame(&mut runtime, &mut world, &[pad(KeyCode::ButtonL1, false)]);
    // The wheel, toward the player, goes on to the next one.
    let wheel = InputEvent::new(InputKind::Wheel {
        position: [0.0, 0.0],
        notches: -1.0,
    });
    frame(&mut runtime, &mut world, &[wheel]);
    assert_eq!(equipped(&world, inventory), Some(torch));

    world
        .set_property(inventory, "HotbarEnabled", Value::Bool(false))
        .unwrap();
    key(&mut runtime, &mut world, KeyCode::One, true);
    assert_eq!(equipped(&world, inventory), Some(torch));
}
