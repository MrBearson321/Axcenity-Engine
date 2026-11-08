//! Phase 80: joined players' streaming, with a dedicated server and
//! players' devices in this process.

use axcenity_core::streaming::streaming_mode;
use axcenity_core::{ClassName, InstanceId, Value, Vector3, World};

use crate::replication_tests::{Device, frames, player, server};

/// Min and target radius of these tests' worlds.
const MIN: f64 = 64.0;
const TARGET: f64 = 200.0;

fn streaming_server() -> (Device, crate::Client) {
    let (mut server, client) = server();
    let workspace = server.world.workspace();
    for (name, value) in [
        ("StreamingEnabled", Value::Bool(true)),
        ("StreamingMinRadius", Value::Number(MIN)),
        ("StreamingTargetRadius", Value::Number(TARGET)),
    ] {
        server.world.set_property(workspace, name, value).unwrap();
    }
    (server, client)
}

fn part(world: &mut World, parent: InstanceId, name: &str, x: f32) -> InstanceId {
    let id = world.create(ClassName::Part).unwrap();
    world.set_name(id, name).unwrap();
    world
        .set_property(id, "Anchored", Value::Bool(true))
        .unwrap();
    world
        .set_property(id, "Position", Value::Vector3(Vector3::new(x, 0.0, 0.0)))
        .unwrap();
    world.set_parent(id, Some(parent)).unwrap();
    id
}

fn model(world: &mut World, parent: InstanceId, name: &str, mode: &str) -> InstanceId {
    let id = world.create(ClassName::Model).unwrap();
    world.set_name(id, name).unwrap();
    world
        .set_property(id, "ModelStreamingMode", streaming_mode(mode))
        .unwrap();
    world.set_parent(id, Some(parent)).unwrap();
    id
}

fn has(device: &Device, name: &str) -> bool {
    device
        .world
        .find_first_child(device.world.workspace(), name)
        .is_some()
}

/// The server's only Player.
fn the_player(world: &World) -> InstanceId {
    world.get(world.players()).unwrap().children()[0]
}

/// Gives the server's Player a character standing at `x`, and returns its
/// root part.
fn character_at(world: &mut World, x: f32) -> InstanceId {
    let workspace = world.workspace();
    let body = model(world, workspace, "Hero", "Default");
    let root = part(world, body, "HumanoidRootPart", x);
    let player = the_player(world);
    world
        .set_property(player, "Character", Value::Instance(Some(body)))
        .unwrap();
    root
}

fn run_local_script(device: &mut Device, source: &str) {
    let script = device.world.create(ClassName::LocalScript).unwrap();
    let parent = device.world.starter_player_scripts();
    device.world.set_parent(script, Some(parent)).unwrap();
    device.scripts.start_script(
        &mut device.world,
        script,
        "scripts/check.client.lua",
        source,
    );
}

#[test]
fn a_player_only_has_what_is_near_them_and_it_follows_them() {
    let (mut server, client) = streaming_server();
    let workspace = server.world.workspace();
    part(&mut server.world, workspace, "Near", 0.0);
    part(&mut server.world, workspace, "Mid", 150.0);
    part(&mut server.world, workspace, "Far", 1000.0);
    let house = model(&mut server.world, workspace, "FarHouse", "Default");
    part(&mut server.world, house, "FarWall", 1000.0);

    let mut players = vec![player(client, 1.0)];
    frames(&mut server, &mut players, 3);
    let root = character_at(&mut server.world, 0.0);
    frames(&mut server, &mut players, 60);

    let device = &players[0];
    assert!(has(device, "Near"), "inside the minimum radius");
    assert!(has(device, "Mid"), "inside the target radius");
    assert!(!has(device, "Far"), "far outside the target radius");
    assert!(
        has(device, "FarHouse"),
        "a Model always stays on the device"
    );
    assert!(!has_child(device, "FarHouse", "FarWall"));
    assert!(has(device, "Hero"), "a character is always there");

    // The character walks over there (a teleport, here): what's near the
    // new place arrives and what's left behind goes.
    server
        .world
        .set_property(
            root,
            "Position",
            Value::Vector3(Vector3::new(1000.0, 0.0, 0.0)),
        )
        .unwrap();
    frames(&mut server, &mut players, 60);

    let device = &players[0];
    assert!(has(device, "Far"));
    assert!(
        has_child(device, "FarHouse", "FarWall"),
        "a model at a time"
    );
    assert!(!has(device, "Near"), "removed once out of range");
    assert!(!has(device, "Mid"));
}

fn has_child(device: &Device, model: &str, child: &str) -> bool {
    let workspace = device.world.workspace();
    device
        .world
        .find_first_child(workspace, model)
        .is_some_and(|model| device.world.find_first_child(model, child).is_some())
}

#[test]
fn whats_within_the_minimum_radius_comes_with_the_snapshot_and_the_rest_within_the_budget() {
    let (mut server, client) = streaming_server();
    let workspace = server.world.workspace();
    part(&mut server.world, workspace, "Inside", 10.0);
    // More than a tick's budget, all between the two radii.
    for i in 0..4000 {
        part(
            &mut server.world,
            workspace,
            "Crate",
            100.0 + (i % 80) as f32,
        );
    }
    let mut players = vec![player(client, 1.0)];
    let count = |device: &Device| {
        device
            .world
            .get(device.world.workspace())
            .unwrap()
            .children()
            .len()
    };
    // The frames until the snapshot arrives.
    for _ in 0..30 {
        server.frame();
        players[0].frame();
        if has(&players[0], "Inside") {
            break;
        }
    }
    assert!(has(&players[0], "Inside"));
    let at_first = count(&players[0]);
    assert!(
        at_first < 4000,
        "the first frames bring only a share: {at_first}"
    );
    frames(&mut server, &mut players, 180);
    assert!(
        count(&players[0]) >= 4001,
        "all of it arrives: {}",
        count(&players[0])
    );
}

#[test]
fn a_persistent_model_is_always_on_every_device_and_scripts_can_ask_what_isnt_there() {
    let (mut server, client) = streaming_server();
    let workspace = server.world.workspace();
    let keep = model(&mut server.world, workspace, "Keep", "Persistent");
    part(&mut server.world, keep, "KeepWall", 9000.0);
    let house = model(&mut server.world, workspace, "FarHouse", "Default");
    let wall = part(&mut server.world, house, "FarWall", 1000.0);
    let pointer = server.world.create(ClassName::ObjectValue).unwrap();
    server.world.set_name(pointer, "Pointer").unwrap();
    server
        .world
        .set_property(pointer, "Value", Value::Instance(Some(wall)))
        .unwrap();
    server.world.set_parent(pointer, Some(keep)).unwrap();

    let mut players = vec![player(client, 1.0)];
    frames(&mut server, &mut players, 3);
    let root = character_at(&mut server.world, 0.0);
    frames(&mut server, &mut players, 20);
    assert!(has_child(&players[0], "Keep", "KeepWall"), "persistent");
    assert!(!has_child(&players[0], "FarHouse", "FarWall"));

    run_local_script(
        &mut players[0],
        r#"
local keep = workspace.Keep
print("keep", keep.StreamingState == Enum.StreamingState.StreamedIn)
local house = workspace.FarHouse
print("house", house.StreamingState == Enum.StreamingState.StreamedOut)
print("pointer", keep.Pointer.Value == nil)
house.StreamedIn:Connect(function()
    print("house arrived", #house:GetChildren(), house.StreamingState == Enum.StreamingState.StreamedIn)
end)
"#,
    );
    frames(&mut server, &mut players, 2);
    assert!(players[0].said("keep true"), "{:?}", players[0].output);
    assert!(players[0].said("house true"), "{:?}", players[0].output);
    assert!(players[0].said("pointer true"), "{:?}", players[0].output);

    server
        .world
        .set_property(
            root,
            "Position",
            Value::Vector3(Vector3::new(1000.0, 0.0, 0.0)),
        )
        .unwrap();
    frames(&mut server, &mut players, 60);
    assert!(
        players[0].said("house arrived 1 true"),
        "{:?}",
        players[0].output
    );
    assert!(has_child(&players[0], "Keep", "KeepWall"), "still there");
    assert!(
        !players[0].output.iter().any(|line| line.contains("error")),
        "{:?}",
        players[0].output
    );
}

#[test]
fn a_reference_to_a_part_streamed_out_reports_it_and_doesnt_error() {
    let (mut server, client) = streaming_server();
    let workspace = server.world.workspace();
    part(&mut server.world, workspace, "Near", 0.0);
    let mut players = vec![player(client, 1.0)];
    frames(&mut server, &mut players, 3);
    let root = character_at(&mut server.world, 0.0);
    frames(&mut server, &mut players, 20);
    run_local_script(
        &mut players[0],
        r#"
local near = workspace.Near
near.StreamedIn:Connect(function()
    print("back", near.StreamingState == Enum.StreamingState.StreamedIn, near.Name)
end)
task.delay(0.5, function()
    print("away", near.StreamingState == Enum.StreamingState.StreamedOut)
end)
"#,
    );
    frames(&mut server, &mut players, 2);
    server
        .world
        .set_property(
            root,
            "Position",
            Value::Vector3(Vector3::new(1000.0, 0.0, 0.0)),
        )
        .unwrap();
    frames(&mut server, &mut players, 40);
    assert!(players[0].said("away true"), "{:?}", players[0].output);
    server
        .world
        .set_property(root, "Position", Value::Vector3(Vector3::default()))
        .unwrap();
    frames(&mut server, &mut players, 40);
    assert!(
        players[0].said("back true Near"),
        "the same Instance comes back: {:?}",
        players[0].output
    );
    assert!(
        !players[0].output.iter().any(|line| line.contains("error")),
        "{:?}",
        players[0].output
    );
}

#[test]
fn teleporting_after_request_stream_around_lands_on_loaded_ground() {
    let (mut server, client) = streaming_server();
    let workspace = server.world.workspace();
    part(&mut server.world, workspace, "Ground", 3000.0);
    let mut players = vec![player(client, 1.0)];
    frames(&mut server, &mut players, 3);
    let root = character_at(&mut server.world, 0.0);
    frames(&mut server, &mut players, 20);
    assert!(!has(&players[0], "Ground"));

    server
        .scripts
        .run_command(
            &mut server.world,
            r#"
local player = game.Players:GetPlayers()[1]
player:RequestStreamAroundAsync(Vector3.new(3000, 0, 0))
print("region ready")
player.Character:PivotTo(CFrame.new(3000, 6, 0))
"#,
        )
        .unwrap();

    let mut landed = false;
    for _ in 0..120 {
        server.frame();
        players[0].frame();
        let device = &players[0];
        let where_it_is = device
            .world
            .get(root)
            .and_then(|part| part.property("Position").and_then(Value::as_vector3));
        if where_it_is.is_some_and(|p| p.x > 2000.0) {
            landed = true;
            assert!(
                has(device, "Ground"),
                "the ground arrived before the character did"
            );
        }
    }
    assert!(server.said("region ready"), "{:?}", server.output);
    assert!(landed, "the character was teleported");
}

#[test]
fn a_device_can_ask_for_a_region_itself() {
    let (mut server, client) = streaming_server();
    let workspace = server.world.workspace();
    part(&mut server.world, workspace, "Ground", 3000.0);
    let mut players = vec![player(client, 1.0)];
    frames(&mut server, &mut players, 3);
    character_at(&mut server.world, 0.0);
    frames(&mut server, &mut players, 20);
    run_local_script(
        &mut players[0],
        r#"
game.Players.LocalPlayer:RequestStreamAroundAsync(Vector3.new(3000, 0, 0))
print("loaded", workspace:FindFirstChild("Ground") ~= nil)
"#,
    );
    frames(&mut server, &mut players, 30);
    assert!(players[0].said("loaded true"), "{:?}", players[0].output);
}

#[test]
fn with_streaming_off_a_player_has_everything_and_asking_for_a_region_carries_on() {
    let (mut server, client) = server();
    let workspace = server.world.workspace();
    part(&mut server.world, workspace, "Far", 100_000.0);
    let mut players = vec![player(client, 1.0)];
    frames(&mut server, &mut players, 20);
    assert!(has(&players[0], "Far"));
    run_local_script(
        &mut players[0],
        r#"
game.Players.LocalPlayer:RequestStreamAroundAsync(Vector3.new(1, 2, 3))
print("carried on", workspace.Far.StreamingState == Enum.StreamingState.StreamedIn)
"#,
    );
    frames(&mut server, &mut players, 5);
    assert!(
        players[0].said("carried on true"),
        "{:?}",
        players[0].output
    );
}

#[test]
fn a_new_part_a_server_script_makes_reaches_those_it_is_near() {
    let (mut server, client) = streaming_server();
    let mut players = vec![player(client, 1.0)];
    frames(&mut server, &mut players, 3);
    character_at(&mut server.world, 0.0);
    frames(&mut server, &mut players, 20);
    server
        .scripts
        .run_command(
            &mut server.world,
            r#"
for _, spot in { {"NewNear", 5}, {"NewFar", 5000} } do
    local part = Instance.new("Part")
    part.Name = spot[1]
    part.Anchored = true
    part.Position = Vector3.new(spot[2], 0, 0)
    part.Parent = workspace
end
"#,
        )
        .unwrap();
    frames(&mut server, &mut players, 30);
    assert!(has(&players[0], "NewNear"));
    assert!(!has(&players[0], "NewFar"));
}
