//! Scripts inside objects (G2) on devices following a server: a LocalScript
//! the server places at run time, and one in a model that streams in, start
//! with the code of the file their `SourceFile` names.

use axcenity_core::{ClassName, InstanceId, Value, World};
use axcenity_script::ScriptRuntime;

use super::*;
use crate::replication_tests::{Device, castle, frames, info};

const GLOW: &str = "objects/Lamp/Glow.client.lua";
const GLOW_CODE: &str =
    r#"print(game.Players.LocalPlayer.Name, "glows", script.Parent:GetFullName())"#;

/// The castle with a lamp template in ServerStorage, carrying a LocalScript
/// inside it, as every device loads it. Returns the LocalScript.
fn add_lamp(world: &mut World) -> InstanceId {
    let lamp = world.create(ClassName::Model).unwrap();
    world.set_name(lamp, "Lamp").unwrap();
    world
        .set_parent(lamp, Some(world.server_storage()))
        .unwrap();
    let glow = world.create(ClassName::LocalScript).unwrap();
    world.set_name(glow, "Glow").unwrap();
    world
        .set_property(glow, "SourceFile", Value::String(GLOW.to_owned()))
        .unwrap();
    world.set_parent(glow, Some(lamp)).unwrap();
    glow
}

/// `axcenity-server` with the lamp castle running `server`, and after
/// `before` frames a player whose device knows the lamp template's code
/// from the game it loaded, as `client_scripts` lists it.
fn game(server: &str, before: usize) -> (Device, Vec<Device>) {
    let mut castle_world = castle();
    add_lamp(&mut castle_world.world);
    let mut world = castle_world.world;
    let mut scripts = ScriptRuntime::new();
    scripts.set_dedicated();
    scripts.start_script(
        &mut world,
        castle_world.server_script,
        "scripts/main.server.lua",
        server,
    );
    let mut network = Network::new(info()).with_discovery_port(47936);
    let link = network.host_in_memory(&mut world, &mut scripts, false);
    let mut server = Device {
        world,
        scripts,
        network,
        output: Vec::new(),
    };
    frames(&mut server, &mut [], before);

    // The player's device loads the same game: the template's code is in
    // the game's LocalScripts, though the server never sends ServerStorage.
    let mut own = castle();
    let template = add_lamp(&mut own.world);
    let mut world = own.world;
    let mut scripts = ScriptRuntime::new();
    scripts.set_user_id(7.0);
    let mut network = Network::new(info());
    network.set_client_scripts(vec![ScriptCode {
        instance: template,
        path: GLOW.to_owned(),
        source: GLOW_CODE.to_owned(),
    }]);
    network.join_through(&mut world, &mut scripts, link, None);
    let player = Device {
        world,
        scripts,
        network,
        output: Vec::new(),
    };
    (server, vec![player])
}

fn glows(device: &Device) -> Vec<&str> {
    device
        .output
        .iter()
        .filter(|line| line.contains("glows"))
        .map(String::as_str)
        .collect()
}

#[test]
fn a_local_script_the_server_clones_into_the_workspace_starts_on_a_players_device() {
    let (mut server, mut players) = game(
        r#"game.Players.PlayerAdded:Connect(function()
            task.wait(0.2)
            game.ServerStorage.Lamp:Clone().Parent = workspace
        end)"#,
        0,
    );
    frames(&mut server, &mut players, 10);
    let player = &players[0];
    assert!(
        !player
            .output
            .iter()
            .any(|line| line.contains("aren't in the server's world")),
        "a template isn't missing: {:?}",
        player.output
    );
    assert!(glows(player).is_empty());
    frames(&mut server, &mut players, 30);
    assert_eq!(
        glows(&players[0]),
        ["Player glows Workspace.Lamp"],
        "{:?}",
        players[0].output
    );
}

#[test]
fn a_clone_placed_before_a_player_joined_starts_when_they_join() {
    let (mut server, mut players) =
        game(r#"game.ServerStorage.Lamp:Clone().Parent = workspace"#, 5);
    frames(&mut server, &mut players, 20);
    assert_eq!(
        glows(&players[0]),
        ["Player glows Workspace.Lamp"],
        "{:?}",
        players[0].output
    );
}
