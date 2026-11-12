//! Phase 81: lag compensation, with a dedicated server and a player's
//! device in this process over a slow connection.

use std::time::Duration;

use axcenity_core::{ClassName, Value};
use axcenity_script::ScriptRuntime;

use super::*;
use crate::replication_tests::{Device, castle, frames, info};

/// The server moves a 4-stud target along X at 10 studs a second. A shot
/// is a ray straight down Z through the X its sender sent.
const SERVER: &str = r#"
local target = Instance.new("Part")
target.Name = "Target"
target.Anchored = true
target.Size = Vector3.new(4, 4, 4)
target.Position = Vector3.new(0, 50, 0)
target.Parent = workspace
task.spawn(function()
    local x = 0
    while true do
        x = x + 10 / 60
        target.Position = Vector3.new(x, 50, 0)
        task.wait()
    end
end)
local function shot(origin, params)
    local result = workspace:Raycast(origin, Vector3.new(0, 0, 40), params)
    return result and result.Instance.Name or "miss"
end
game.ReplicatedStorage.Shout.OnServerEvent:Connect(function(player, origin)
    local params = RaycastParams.new()
    params.CompensateFor = player
    print("shot", shot(origin, params), shot(origin))
end)
game.ReplicatedStorage.Gun.Fire.OnServerEvent:Connect(function(player, origin)
    print("gun", shot(origin))
    task.wait()
    print("gun after waiting", shot(origin))
end)
"#;

/// The player aims at the middle of the target where they see it.
const CLIENT: &str = r#"
local target = workspace:WaitForChild("Target")
task.wait(2)
local p = target.Position
local origin = Vector3.new(p.X, p.Y, -20)
game.ReplicatedStorage.Shout:FireServer(origin)
game.ReplicatedStorage.Gun.Fire:FireServer(origin)
"#;

#[test]
fn a_shot_at_a_moving_part_hits_where_the_player_saw_it_only_when_compensated() {
    let castle = castle();
    let mut world = castle.world;
    let gun = world.create(ClassName::Tool).unwrap();
    world.set_name(gun, "Gun").unwrap();
    world
        .set_property(gun, "LagCompensated", Value::Bool(true))
        .unwrap();
    world
        .set_parent(gun, Some(world.replicated_storage()))
        .unwrap();
    let fire = world.create(ClassName::RemoteEvent).unwrap();
    world.set_name(fire, "Fire").unwrap();
    world.set_parent(fire, Some(gun)).unwrap();
    let client_world = world.clone();

    let mut scripts = ScriptRuntime::new();
    scripts.set_dedicated();
    scripts.start_script(
        &mut world,
        castle.server_script,
        "scripts/main.server.lua",
        SERVER,
    );
    let mut network = Network::new(info()).with_discovery_port(47935);
    let link = network.host_in_memory(&mut world, &mut scripts, false);
    // 150 ms each way: the player sees the target about 0.4 s late, 4
    // studs behind where it is when their shot arrives.
    network.emulate(Conditions {
        latency: Duration::from_millis(150),
        ..Conditions::default()
    });
    let mut server = Device {
        world,
        scripts,
        network,
        output: Vec::new(),
    };

    let mut world = client_world;
    let mut scripts = ScriptRuntime::new();
    scripts.set_user_id(101.0);
    let mut network = Network::new(info());
    network.set_client_scripts(vec![ScriptCode {
        instance: castle.client_script,
        path: "scripts/hud.client.lua".to_owned(),
        source: CLIENT.to_owned(),
    }]);
    network.join_through(&mut world, &mut scripts, link, None);
    let mut players = vec![Device {
        world,
        scripts,
        network,
        output: Vec::new(),
    }];

    for _ in 0..400 {
        frames(&mut server, &mut players, 1);
        if server
            .output
            .iter()
            .any(|line| line.starts_with("gun after"))
        {
            break;
        }
    }
    let output = &server.output;
    assert!(server.said("shot Target miss"), "{output:?}");
    assert!(server.said("gun Target"), "{output:?}");
    assert!(
        server.said("gun after waiting miss"),
        "after a wait the handler's shots aren't turned back: {output:?}"
    );
}
