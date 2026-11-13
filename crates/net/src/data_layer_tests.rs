//! Roadmap R16.4: data layers in a game with a joined player. The server
//! switches a layer; the player's device sees its parts leave and come
//! back, and scripts on both sides hear `LayerChanged`.

use axcenity_core::{ClassName, InstanceId, Value, World, data_layers};
use axcenity_script::ScriptRuntime;

use super::*;
use crate::replication_tests::{Device, frames, info};

const SERVER: &str = r#"
local layers = game:GetService("DataLayerService")
print("server starts", layers:IsLayerActive("Night"), game.Workspace:FindFirstChild("Moon") ~= nil)
layers.LayerChanged:Connect(function(name, active)
    print("server", name, active)
end)
"#;

const CLIENT: &str = r#"
local layers = game:GetService("DataLayerService")
print("client starts", layers:IsLayerActive("Night"), game.Workspace:FindFirstChild("Moon") ~= nil)
layers.LayerChanged:Connect(function(name, active)
    print("client", name, active, game.Workspace:FindFirstChild("Moon") ~= nil)
end)
local ok = pcall(function()
    layers:SetLayerActive("Night", true)
end)
print("client can switch", ok)
"#;

struct Night {
    world: World,
    server_script: InstanceId,
    client_script: InstanceId,
    moon: InstanceId,
}

/// A game with a Night layer that starts off, holding the Moon.
fn night() -> Night {
    let mut world = World::new();
    let add = |world: &mut World, class, name: &str, parent| {
        let id = world.create(class).unwrap();
        world.set_name(id, name).unwrap();
        world.set_parent(id, Some(parent)).unwrap();
        id
    };
    let service = world.data_layer_service();
    let layer = add(&mut world, ClassName::DataLayer, "Night", service);
    world
        .set_property(layer, "ActiveOnStart", Value::Bool(false))
        .unwrap();
    let workspace = world.workspace();
    add(&mut world, ClassName::Part, "Ground", workspace);
    let moon = add(&mut world, ClassName::Part, "Moon", workspace);
    world.add_tag(moon, "Night").unwrap();
    let service = world.server_script_service();
    let server_script = add(&mut world, ClassName::Script, "Main", service);
    let starter = world.starter_player_scripts();
    let client_script = add(&mut world, ClassName::LocalScript, "Sky", starter);
    Night {
        world,
        server_script,
        client_script,
        moon,
    }
}

#[test]
fn a_layer_switched_on_the_server_comes_and_goes_on_a_joined_device() {
    let Night {
        mut world,
        server_script,
        moon,
        ..
    } = night();
    data_layers::start(&mut world);
    let mut scripts = ScriptRuntime::new();
    scripts.set_dedicated();
    scripts.start_script(&mut world, server_script, "scripts/main.server.lua", SERVER);
    let mut network = Network::new(info()).with_discovery_port(47937);
    let client = network.host_in_memory(&mut world, &mut scripts, false);
    let mut server = Device {
        world,
        scripts,
        network,
        output: Vec::new(),
    };

    let Night {
        mut world,
        client_script,
        ..
    } = night();
    let mut scripts = ScriptRuntime::new();
    scripts.set_user_id(101.0);
    let mut network = Network::new(info());
    network.set_client_scripts(vec![ScriptCode {
        instance: client_script,
        path: "scripts/sky.client.lua".to_owned(),
        source: CLIENT.to_owned(),
    }]);
    network.join_through(&mut world, &mut scripts, client, None);
    let mut players = vec![Device {
        world,
        scripts,
        network,
        output: Vec::new(),
    }];
    frames(&mut server, &mut players, 10);

    assert!(
        server.said("server starts false false"),
        "{:?}",
        server.output
    );
    let player = &players[0];
    assert!(
        player.said("client starts false false"),
        "{:?}",
        player.output
    );
    assert!(
        player.said("client can switch false"),
        "{:?}",
        player.output
    );
    assert!(
        !player.world.contains(moon),
        "a parked part never reaches a player"
    );

    // The server switches Night on: the Moon arrives, both sides hear it.
    server
        .scripts
        .run_command(
            &mut server.world,
            r#"game:GetService("DataLayerService"):SetLayerActive("Night", true)"#,
        )
        .unwrap();
    frames(&mut server, &mut players, 5);
    assert!(server.said("server Night true"), "{:?}", server.output);
    let player = &players[0];
    assert!(player.said("client Night true true"), "{:?}", player.output);
    let workspace = player.world.workspace();
    assert_eq!(player.world.get(moon).unwrap().parent(), Some(workspace));

    // And off again: it leaves the player's device.
    server
        .scripts
        .run_command(
            &mut server.world,
            r#"game:GetService("DataLayerService"):SetLayerActive("Night", false)"#,
        )
        .unwrap();
    frames(&mut server, &mut players, 5);
    assert!(server.said("server Night false"), "{:?}", server.output);
    let player = &players[0];
    assert!(
        player.said("client Night false false"),
        "{:?}",
        player.output
    );
    assert!(!player.world.contains(moon));
    assert!(
        server.world.contains(moon),
        "the server keeps it, set aside"
    );
}
