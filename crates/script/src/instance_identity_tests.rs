//! Tests for roadmap item I2: one Instance is one Lua value, so it works as
//! a table key (`data[player]`) and with `rawequal`, like in Roblox.

use axcenity_core::{ClassName, InstanceId, World};

use super::*;

fn start(runtime: &mut ScriptRuntime, world: &mut World, source: &str) -> InstanceId {
    let script = world.create(ClassName::Script).unwrap();
    world
        .set_parent(script, Some(world.server_script_service()))
        .unwrap();
    runtime.start_script(world, script, "scripts/main.server.lua", source);
    script
}

fn lines(runtime: &mut ScriptRuntime) -> Vec<String> {
    runtime
        .take_output()
        .into_iter()
        .map(|line| match line.kind {
            OutputKind::Print => line.text,
            OutputKind::Error => format!("error: {}", line.text),
            OutputKind::Warning => format!("warning: {}", line.text),
        })
        .collect()
}

fn baseplate(world: &mut World) {
    let part = world.create(ClassName::Part).unwrap();
    world.set_name(part, "Baseplate").unwrap();
    world.set_parent(part, Some(world.workspace())).unwrap();
}

#[test]
fn an_instance_read_twice_is_the_same_lua_value() {
    let mut world = World::new();
    baseplate(&mut world);
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        r#"
        local a = workspace.Baseplate
        local b = workspace:FindFirstChild("Baseplate")
        print(rawequal(a, b), a == b)
        print(rawequal(workspace, game.Workspace))
        print(rawequal(a.Parent, workspace))
        "#,
    );
    runtime.step(&mut world, 0.0);
    assert_eq!(lines(&mut runtime), ["true\ttrue", "true", "true"]);
}

#[test]
fn an_instance_works_as_a_table_key() {
    let mut world = World::new();
    baseplate(&mut world);
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        r#"
        local t = {}
        t[workspace.Baseplate] = 1
        print(t[workspace.Baseplate], t[workspace:FindFirstChild("Baseplate")])
        local count = 0
        t[workspace.Baseplate] = 2
        for _ in pairs(t) do count += 1 end
        print(count, t[workspace.Baseplate])
        "#,
    );
    runtime.step(&mut world, 0.0);
    assert_eq!(lines(&mut runtime), ["1\t1", "1\t2"]);
}

#[test]
fn player_data_keyed_by_player_survives_between_events() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        r#"
        local Players = game:GetService("Players")
        local data = {}
        Players.PlayerAdded:Connect(function(player)
            data[player] = { coins = 10 }
        end)
        Players.PlayerRemoving:Connect(function(player)
            local entry = data[player]
            print(player.Name, entry and entry.coins)
            data[player] = nil
        end)
        "#,
    );
    runtime.step(&mut world, 0.0);
    let bob = runtime.add_remote_player(&mut world, "Bob", "Bob", 2.0);
    runtime.step(&mut world, 0.0);
    runtime.remove_player(&mut world, bob);
    runtime.step(&mut world, 0.0);
    let out: Vec<String> = lines(&mut runtime)
        .into_iter()
        .filter(|line| line.starts_with("Bob"))
        .collect();
    assert_eq!(out, ["Bob\t10"]);
}

#[test]
fn a_destroyed_instance_keeps_its_identity_and_a_new_one_gets_its_own() {
    let mut world = World::new();
    baseplate(&mut world);
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        r#"
        local t = {}
        local old = workspace.Baseplate
        t[old] = "old"
        old:Destroy()
        -- A new Instance may reuse the old arena slot with a new generation.
        local new = Instance.new("Part")
        new.Name = "Baseplate"
        new.Parent = workspace
        print(t[old], t[workspace.Baseplate], rawequal(old, new), old == new)
        "#,
    );
    runtime.step(&mut world, 0.0);
    assert_eq!(lines(&mut runtime), ["old\tnil\tfalse\tfalse"]);
}
