//! Tests for Scripts and LocalScripts running side by side: RemoteEvents,
//! keyboard input, the camera, and the APIs that only work on one side.

use std::sync::{Arc, Mutex};

use axcenity_core::{ClassName, InstanceId, KeyCode, Value, Vector3, World};

use super::*;

/// Makes a Script or LocalScript Instance like the project loader does, and
/// starts `source` as it. The path's ending says which: `.client.lua` is a
/// LocalScript.
fn start(runtime: &mut ScriptRuntime, world: &mut World, path: &str, source: &str) {
    let (class, service) = if path.ends_with(".client.lua") {
        (ClassName::LocalScript, world.starter_player_scripts())
    } else {
        (ClassName::Script, world.server_script_service())
    };
    let script = world.create(class).unwrap();
    world.set_parent(script, Some(service)).unwrap();
    runtime.start_script(world, script, path, source);
}

/// A world with a RemoteEvent called `Remote` in ReplicatedStorage, and a
/// Part called `Mover` in the Workspace.
fn world_with_remote() -> (World, InstanceId) {
    let mut world = World::new();
    let remote = world.create(ClassName::RemoteEvent).unwrap();
    world.set_name(remote, "Remote").unwrap();
    world
        .set_parent(remote, Some(world.replicated_storage()))
        .unwrap();
    let mover = world.create(ClassName::Part).unwrap();
    world.set_name(mover, "Mover").unwrap();
    world.set_parent(mover, Some(world.workspace())).unwrap();
    (world, mover)
}

fn lines(runtime: &mut ScriptRuntime) -> Vec<String> {
    runtime
        .take_output()
        .into_iter()
        .map(|line| match line.kind {
            OutputKind::Print => line.text,
            OutputKind::Error => format!("error: {}", line.text),
        })
        .collect()
}

fn press(key: KeyCode) -> KeyEvent {
    KeyEvent { key, pressed: true }
}

fn release(key: KeyCode) -> KeyEvent {
    KeyEvent {
        key,
        pressed: false,
    }
}

const MOVE_SERVER: &str = r#"
local remote = game.ReplicatedStorage.Remote
remote.OnServerEvent:Connect(function(player, direction)
    print(player.Name .. " moves " .. tostring(direction))
    workspace.Mover.Position = workspace.Mover.Position + direction
end)
"#;

const MOVE_CLIENT: &str = r#"
local UserInputService = game:GetService("UserInputService")
local remote = game.ReplicatedStorage.Remote
UserInputService.InputBegan:Connect(function(input)
    if input.KeyCode == Enum.KeyCode.Right then
        remote:FireServer(Vector3.new(1, 0, 0))
    end
end)
"#;

#[test]
fn a_local_script_reads_a_key_and_a_script_moves_a_part_through_a_remote_event() {
    let (mut world, mover) = world_with_remote();
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        "scripts/move.server.lua",
        MOVE_SERVER,
    );
    start(
        &mut runtime,
        &mut world,
        "scripts/keys.client.lua",
        MOVE_CLIENT,
    );

    // Frame step 1 reads the key, and step 2 delivers the message.
    runtime.input(&mut world, &[press(KeyCode::Right), press(KeyCode::W)]);
    runtime.step(&mut world, 1.0 / 60.0);

    assert_eq!(lines(&mut runtime), ["Player moves 1, 0, 0"]);
    assert_eq!(
        world.get(mover).unwrap().property("Position"),
        Some(&Value::Vector3(Vector3::new(1.0, 0.0, 0.0)))
    );
}

#[test]
fn the_player_joins_before_the_first_local_script_starts() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        "scripts/count.server.lua",
        "print(game.Players:FindFirstChild('Player') == nil)",
    );
    assert_eq!(runtime.local_player(), None, "no player before the client");
    start(
        &mut runtime,
        &mut world,
        "scripts/me.client.lua",
        "local me = game.Players.LocalPlayer print(me.Name, me.ClassName, me.Parent)",
    );
    assert_eq!(lines(&mut runtime), ["true", "Player\tPlayer\tPlayers"]);
    let player = runtime.local_player().unwrap();
    assert_eq!(world.get(player).unwrap().parent(), Some(world.players()));
}

/// Each API that only works on one side, used from the other side.
#[test]
fn input_and_camera_apis_in_a_script_explain_to_use_a_local_script() {
    let wrong_side = [
        (
            "scripts/input.server.lua",
            "local uis = game:GetService('UserInputService')",
            "UserInputService only works in a LocalScript",
        ),
        (
            "scripts/input.server.lua",
            "local uis = game.UserInputService",
            "UserInputService only works in a LocalScript",
        ),
        (
            "scripts/camera.server.lua",
            "local camera = workspace.CurrentCamera",
            "CurrentCamera only works in a LocalScript",
        ),
        (
            "scripts/me.server.lua",
            "local me = game.Players.LocalPlayer",
            "LocalPlayer only works in a LocalScript",
        ),
        (
            "scripts/send.server.lua",
            "game.ReplicatedStorage.Remote:FireServer(1)",
            "FireServer only works in a LocalScript",
        ),
        (
            "scripts/listen.server.lua",
            "game.ReplicatedStorage.Remote.OnClientEvent:Connect(print)",
            "OnClientEvent only works in a LocalScript",
        ),
        (
            "scripts/send.client.lua",
            "game.ReplicatedStorage.Remote:FireAllClients(1)",
            "FireAllClients only works in a Script",
        ),
        (
            "scripts/send.client.lua",
            "game.ReplicatedStorage.Remote:FireClient(game.Players.LocalPlayer, 1)",
            "FireClient only works in a Script",
        ),
        (
            "scripts/listen.client.lua",
            "game.ReplicatedStorage.Remote.OnServerEvent:Connect(print)",
            "OnServerEvent only works in a Script",
        ),
    ];
    for (path, source, expected) in wrong_side {
        let (mut world, _) = world_with_remote();
        let mut runtime = ScriptRuntime::new();
        start(&mut runtime, &mut world, path, source);
        let output = lines(&mut runtime);
        let [line] = output.as_slice() else {
            panic!("{source}: {output:?}");
        };
        assert!(
            line.starts_with(&format!("error: {path}:1: {expected}")),
            "{source}: {line}"
        );
    }

    // The message for input says what to do instead.
    let (mut world, _) = world_with_remote();
    let mut runtime = ScriptRuntime::new();
    let source = "local UserInputService = game:GetService('UserInputService')";
    start(&mut runtime, &mut world, "scripts/input.server.lua", source);
    assert_eq!(
        lines(&mut runtime),
        [
            "error: scripts/input.server.lua:1: UserInputService only works in a LocalScript, \
             because input comes from the player's keyboard. This is a Script, which runs on \
             the server: read the input in a LocalScript, and send it to the server with a \
             RemoteEvent's FireServer."
        ]
    );
}

#[test]
fn server_and_client_contexts_do_not_share_lua_globals() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    // Inside one side, scripts already have their own globals and library
    // tables (and the string metatable is locked, see `sandbox_tests`).
    start(
        &mut runtime,
        &mut world,
        "scripts/set.server.lua",
        r#"
        score = 10
        _G.level = 2
        string.shout = string.upper
        "#,
    );
    start(
        &mut runtime,
        &mut world,
        "scripts/get.client.lua",
        r#"
        print(score, _G.level, string.shout)
        "#,
    );
    assert_eq!(lines(&mut runtime), ["nil\tnil\tnil"]);
}

/// A stand-in transport: records every message it carries, and only
/// delivers them if `deliver` is on.
struct TestTransport {
    log: Arc<Mutex<Vec<Message>>>,
    deliver: bool,
    inner: Loopback,
}

impl Transport for TestTransport {
    fn send(&mut self, message: Message) {
        self.log.lock().unwrap().push(message.clone());
        if self.deliver {
            self.inner.send(message);
        }
    }

    fn receive(&mut self, side: Side) -> Vec<Message> {
        self.inner.receive(side)
    }
}

#[test]
fn the_loopback_transport_can_be_swapped_for_a_test_double() {
    for deliver in [true, false] {
        let (mut world, mover) = world_with_remote();
        // `Arc<Mutex<..>>` lets the test read the log while the runtime owns
        // the transport. (Only tests share it; nothing in the engine does.)
        let log = Arc::new(Mutex::new(Vec::new()));
        let transport = TestTransport {
            log: log.clone(),
            deliver,
            inner: Loopback::default(),
        };
        let mut runtime = ScriptRuntime::with_transport(Box::new(transport));
        start(
            &mut runtime,
            &mut world,
            "scripts/move.server.lua",
            MOVE_SERVER,
        );
        start(
            &mut runtime,
            &mut world,
            "scripts/keys.client.lua",
            MOVE_CLIENT,
        );
        runtime.input(&mut world, &[press(KeyCode::Right)]);
        runtime.step(&mut world, 1.0 / 60.0);

        let remote = world
            .find_first_child(world.replicated_storage(), "Remote")
            .unwrap();
        assert_eq!(
            *log.lock().unwrap(),
            [Message {
                to: Side::Server,
                remote,
                player: runtime.local_player().unwrap(),
                args: vec![RemoteValue::Vector3(Vector3::new(1.0, 0.0, 0.0))],
            }]
        );
        let moved = world.get(mover).unwrap().property("Position")
            == Some(&Value::Vector3(Vector3::new(1.0, 0.0, 0.0)));
        assert_eq!(moved, deliver, "the part moves only if the message arrives");
    }
}

#[test]
fn remote_events_carry_values_to_the_client_and_back() {
    let (mut world, _) = world_with_remote();
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        "scripts/server.server.lua",
        r#"
        local remote = game.ReplicatedStorage.Remote
        remote.OnServerEvent:Connect(function(player, text)
            print("server got " .. text .. " from " .. player.Name)
            remote:FireClient(player, "just you")
        end)
        task.wait()
        remote:FireAllClients(1, 2.5, "hi", true, nil, Vector3.new(1, 2, 3),
            Color3.new(1, 0, 0), Enum.KeyCode.Space, workspace.Mover,
            { list = { "a", "b" }, [10] = false })
        "#,
    );
    start(
        &mut runtime,
        &mut world,
        "scripts/client.client.lua",
        r#"
        local remote = game.ReplicatedStorage.Remote
        remote.OnClientEvent:Connect(function(...)
            local values = table.pack(...)
            if values.n == 1 then
                print("client got " .. values[1])
                return
            end
            print(values.n, values[1], values[2], values[3], values[4], values[5])
            print(values[6], values[7], values[8], values[9].Name)
            local t = values[10]
            print(t.list[1], t.list[2], t[10])
            remote:FireServer("thanks")
        end)
        "#,
    );
    for _ in 0..3 {
        runtime.step(&mut world, 1.0 / 60.0);
    }
    assert_eq!(
        lines(&mut runtime),
        [
            // Whole numbers stay whole, and nil keeps its place.
            "10\t1\t2.5\thi\ttrue\tnil",
            "1, 2, 3\t1, 0, 0\tEnum.KeyCode.Space\tMover",
            "a\tb\tfalse",
            "server got thanks from Player",
            "client got just you",
        ]
    );
}

#[test]
fn sending_something_that_cannot_travel_is_an_error() {
    let (mut world, _) = world_with_remote();
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        "scripts/bad.server.lua",
        r#"
        local remote = game.ReplicatedStorage.Remote
        print(pcall(remote.FireAllClients, remote, function() end))
        local loop = {}
        loop.again = loop
        print(pcall(remote.FireAllClients, remote, loop))
        print(pcall(remote.FireAllClients, 5))
        print(pcall(remote.FireClient, remote, workspace, 1))
        "#,
    );
    let output = lines(&mut runtime);
    assert_eq!(output.len(), 4, "{output:?}");
    assert!(
        output[0].starts_with("false\t") && output[0].contains("can't send a function"),
        "{}",
        output[0]
    );
    assert!(output[1].contains("can't send a table that contains itself"));
    assert!(output[2].contains("call FireAllClients with a colon: remote:FireAllClients(...)"));
    assert!(output[3].contains("FireClient needs the Player to send to first, not a Workspace"));
}

#[test]
fn is_key_down_follows_presses_and_releases_and_repeats_fire_once() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        "scripts/keys.client.lua",
        r#"
        local uis = game:GetService("UserInputService")
        uis.InputBegan:Connect(function(input, processed)
            print("began", input.KeyCode.Name, processed, uis:IsKeyDown(input.KeyCode))
        end)
        uis.InputEnded:Connect(function(input)
            print("ended", input.KeyCode.Name, uis:IsKeyDown(Enum.KeyCode.Space))
        end)
        print(pcall(uis.IsKeyDown, uis, "Space"))
        "#,
    );
    // The OS repeats a held key; only the first press counts.
    runtime.input(
        &mut world,
        &[
            press(KeyCode::Space),
            press(KeyCode::Space),
            release(KeyCode::Space),
            release(KeyCode::Space),
        ],
    );
    let output = lines(&mut runtime);
    assert!(
        output[0].starts_with("false\t") && output[0].contains("IsKeyDown takes an Enum.KeyCode"),
        "{}",
        output[0]
    );
    assert_eq!(
        output[1..],
        ["began\tSpace\tfalse\ttrue", "ended\tSpace\tfalse"]
    );
}

#[test]
fn a_local_script_reads_and_moves_the_camera() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    runtime.set_camera(CameraState {
        position: Vector3::new(1.0, 2.0, 3.0),
        orientation: Vector3::new(0.0, 90.0, 0.0),
    });
    assert_eq!(runtime.take_camera_move(), None);
    start(
        &mut runtime,
        &mut world,
        "scripts/camera.client.lua",
        r#"
        local camera = workspace.CurrentCamera
        print(camera.Position, camera.LookVector.X)
        camera.Position = Vector3.new(0, 10, 20)
        camera.Orientation = Vector3.new(-30, 0, 0)
        print(camera.Position)
        print(pcall(function() camera.Position = 5 end))
        "#,
    );
    let output = lines(&mut runtime);
    assert_eq!(output[..2], ["1, 2, 3\t-1.0", "0, 10, 20"]);
    assert!(
        output[2].contains("Camera.Position must be a Vector3, not a number"),
        "{}",
        output[2]
    );
    assert_eq!(
        runtime.take_camera_move(),
        Some(CameraState {
            position: Vector3::new(0.0, 10.0, 20.0),
            orientation: Vector3::new(-30.0, 0.0, 0.0),
        })
    );
    assert_eq!(runtime.take_camera_move(), None, "taken once");
}

#[test]
fn get_service_finds_services_and_lists_them_for_a_wrong_name() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        "scripts/services.server.lua",
        r#"
        print(game:GetService("Players") == game.Players, game:GetService("Workspace") == workspace)
        print(pcall(game.GetService, game, "Lighting"))
        print(pcall(Instance.new, "Player"))
        "#,
    );
    let output = lines(&mut runtime);
    assert_eq!(output[0], "true\ttrue");
    assert!(
        output[1].ends_with(
            "there is no service named \"Lighting\"; choose from Workspace, Players, \
             ReplicatedStorage, ServerScriptService, StarterPlayer, UserInputService"
        ),
        "{}",
        output[1]
    );
    assert!(
        output[2].contains("a Player is made by the engine"),
        "{}",
        output[2]
    );
}
