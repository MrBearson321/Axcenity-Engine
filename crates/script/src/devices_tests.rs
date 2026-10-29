//! Tests for unified Phase 64: `InvokeClient`, messages for other devices,
//! a replica's client-only runtime, and `UserId` and `DisplayName`.

use axcenity_core::{ClassName, InstanceId, Value, World};

use super::*;

/// Makes a Script or LocalScript (by the path's ending) and starts it.
fn start(runtime: &mut ScriptRuntime, world: &mut World, path: &str, source: &str) -> InstanceId {
    let (class, service) = if path.ends_with(".client.lua") {
        (ClassName::LocalScript, world.starter_player_scripts())
    } else {
        (ClassName::Script, world.server_script_service())
    };
    let script = world.create(class).unwrap();
    world.set_parent(script, Some(service)).unwrap();
    runtime.start_script(world, script, path, source);
    script
}

/// A world with one Instance of `class` called `name` in ReplicatedStorage.
fn world_with(class: ClassName, name: &str) -> (World, InstanceId) {
    let mut world = World::new();
    let id = world.create(class).unwrap();
    world.set_name(id, name).unwrap();
    world
        .set_parent(id, Some(world.replicated_storage()))
        .unwrap();
    (world, id)
}

fn lines(runtime: &mut ScriptRuntime) -> Vec<String> {
    runtime
        .take_output()
        .into_iter()
        .map(|line| match line.kind {
            OutputKind::Print => line.text.replace('\t', " "),
            _ => format!("error: {}", line.text),
        })
        .collect()
}

#[test]
fn a_script_asks_the_players_device_with_invoke_client() {
    let (mut world, _) = world_with(ClassName::RemoteFunction, "Ask");
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        "scripts/answer.client.lua",
        r#"
game.ReplicatedStorage.Ask.OnClientInvoke = function(question)
    return question .. "?", 42
end
"#,
    );
    start(
        &mut runtime,
        &mut world,
        "scripts/ask.server.lua",
        r#"
local player = game.Players:WaitForChild("Player")
print("asked", game.ReplicatedStorage.Ask:InvokeClient(player, "ready"))
print(pcall(game.ReplicatedStorage.Ask.InvokeClient, game.ReplicatedStorage.Ask, 5))
"#,
    );
    runtime.step(&mut world, 0.0);
    runtime.step(&mut world, 0.0);
    let output = lines(&mut runtime);
    assert_eq!(output[0], "asked ready? 42", "{output:?}");
    assert!(
        output[1].starts_with("false") && output[1].contains("InvokeClient needs the Player"),
        "{output:?}"
    );
}

#[test]
fn an_error_in_on_client_invoke_is_raised_in_the_script_and_the_members_keep_to_their_sides() {
    let (mut world, _) = world_with(ClassName::RemoteFunction, "Ask");
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        "scripts/answer.client.lua",
        r#"
game.ReplicatedStorage.Ask.OnClientInvoke = function()
    error("not now")
end
print(pcall(game.ReplicatedStorage.Ask.InvokeClient, game.ReplicatedStorage.Ask))
"#,
    );
    start(
        &mut runtime,
        &mut world,
        "scripts/ask.server.lua",
        r#"
game.ReplicatedStorage.Ask.OnClientInvoke = function() end
"#,
    );
    start(
        &mut runtime,
        &mut world,
        "scripts/ask2.server.lua",
        r#"
print(pcall(game.ReplicatedStorage.Ask.InvokeClient, game.ReplicatedStorage.Ask, game.Players.Player))
"#,
    );
    runtime.step(&mut world, 0.0);
    runtime.step(&mut world, 0.0);
    let output = lines(&mut runtime);
    assert!(
        output[0].contains("InvokeClient only works in a Script"),
        "{output:?}"
    );
    assert!(
        output[1].contains("OnClientInvoke only works in a LocalScript"),
        "{output:?}"
    );
    assert!(
        output[2].starts_with("false scripts/answer.client.lua:3: not now"),
        "{output:?}"
    );
}

#[test]
fn messages_for_players_on_other_devices_leave_for_the_network() {
    let (mut world, remote) = world_with(ClassName::RemoteEvent, "Ping");
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        "scripts/hear.client.lua",
        r#"game.ReplicatedStorage.Ping.OnClientEvent:Connect(function(n) print("here", n) end)"#,
    );
    let ann = runtime.add_remote_player(&mut world, "Ann", "Ann", 7.0);
    start(
        &mut runtime,
        &mut world,
        "scripts/ping.server.lua",
        r#"game.ReplicatedStorage.Ping:FireAllClients(1)"#,
    );
    runtime.step(&mut world, 0.0);
    assert_eq!(lines(&mut runtime), ["here 1"], "this device's own player");
    let away = runtime.take_remote_messages();
    assert_eq!(away.len(), 1, "{away:?}");
    assert_eq!(away[0].to, Side::Client);
    assert_eq!(away[0].player, ann);
    assert_eq!(away[0].remote, remote);
    assert_eq!(away[0].args, [RemoteValue::Integer(1)]);
    let player = world.get(ann).unwrap();
    assert_eq!(player.property("UserId"), Some(&Value::Number(7.0)));
    assert_eq!(
        player.property("DisplayName"),
        Some(&Value::String("Ann".to_owned()))
    );

    // Ann's answer comes back from the network with her Player.
    start(
        &mut runtime,
        &mut world,
        "scripts/hear.server.lua",
        r#"game.ReplicatedStorage.Ping.OnServerEvent:Connect(function(p, n) print("from", p.Name, n) end)"#,
    );
    runtime.receive_remote_messages(vec![Message {
        to: Side::Server,
        remote,
        player: ann,
        kind: MessageKind::Event,
        args: vec![RemoteValue::Integer(2)],
    }]);
    runtime.step(&mut world, 0.0);
    assert_eq!(lines(&mut runtime), ["from Ann 2"]);
}

#[test]
fn a_dedicated_server_has_no_player_of_its_own() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    runtime.set_dedicated();
    start(
        &mut runtime,
        &mut world,
        "scripts/count.server.lua",
        "print(#game.Players:GetPlayers())",
    );
    runtime.step(&mut world, 0.0);
    runtime.step_players(&mut world, 0.0);
    assert_eq!(lines(&mut runtime), ["0"]);
    assert_eq!(runtime.local_player(), None);
}

#[test]
fn a_replicas_runtime_runs_only_local_scripts_and_sends_to_the_server() {
    let (mut server, remote) = world_with(ClassName::RemoteEvent, "Ping");
    let player = server.create(ClassName::Player).unwrap();
    server.set_name(player, "Ann").unwrap();
    server.set_parent(player, Some(server.players())).unwrap();
    // The replica: the same Instances at the same IDs.
    let mut replica = server.empty_replica();
    for id in [remote, player] {
        let class = server.get(id).unwrap().class();
        replica.create_at(id, class).unwrap();
        replica
            .set_name(id, server.get(id).unwrap().name())
            .unwrap();
        let parent = server.get(id).unwrap().parent();
        replica.set_parent(id, parent).unwrap();
    }

    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        "scripts/old.server.lua",
        "print('before')",
    );
    assert_eq!(lines(&mut runtime), ["before"]);
    runtime.become_client(&mut world, replica, player);
    assert!(runtime.is_client_only());
    start(
        &mut runtime,
        &mut world,
        "scripts/s.server.lua",
        "print('server code')",
    );
    start(
        &mut runtime,
        &mut world,
        "scripts/c.client.lua",
        r#"
print("me", game.Players.LocalPlayer.Name, #game.Players:GetPlayers())
game.Players.PlayerAdded:Connect(function(p) print("joined", p.Name) end)
game.ReplicatedStorage.Ping:FireServer("hi")
"#,
    );
    runtime.step(&mut world, 0.0);
    assert_eq!(
        lines(&mut runtime),
        ["me Ann 1"],
        "no Script ran, and no Player was made"
    );
    let away = runtime.take_remote_messages();
    assert_eq!(away.len(), 1);
    assert_eq!(away[0].to, Side::Server);
    assert_eq!(away[0].player, player);

    // Bob arrives through replication.
    let bob = world.create(ClassName::Player).unwrap();
    world.set_name(bob, "Bob").unwrap();
    world.set_parent(bob, Some(world.players())).unwrap();
    runtime.add_replicated_player(&mut world, bob);
    assert_eq!(lines(&mut runtime), ["joined Bob"]);
}

#[test]
fn user_id_and_display_name_are_read_only() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    runtime.set_user_id(123.0);
    start(
        &mut runtime,
        &mut world,
        "scripts/me.client.lua",
        r#"
local me = game.Players.LocalPlayer
print(me.UserId, me.DisplayName)
print(pcall(function() me.UserId = 5 end))
"#,
    );
    let output = lines(&mut runtime);
    assert_eq!(output[0], "123 Player");
    assert!(
        output[1].contains("Player.UserId is read-only"),
        "{output:?}"
    );
}
