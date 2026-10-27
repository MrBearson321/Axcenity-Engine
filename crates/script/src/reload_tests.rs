//! Reloading a script's file while the game runs (`ScriptRuntime::reload_script`).

use axcenity_core::{ClassName, InstanceId, Value, World};

use super::*;

/// A world with a Part named `Block` in the Workspace, at the origin.
fn world_with_block() -> (World, InstanceId) {
    let mut world = World::new();
    let block = world.create(ClassName::Part).unwrap();
    world.set_name(block, "Block").unwrap();
    world.set_parent(block, Some(world.workspace())).unwrap();
    (world, block)
}

/// Adds a script Instance of `class` like the project loader does, and
/// starts `source` as it.
fn start(
    runtime: &mut ScriptRuntime,
    world: &mut World,
    class: ClassName,
    path: &str,
    source: &str,
) -> InstanceId {
    let script = world.create(class).unwrap();
    let name = path.rsplit('/').next().unwrap().split('.').next().unwrap();
    world.set_name(script, name).unwrap();
    let parent = match class {
        ClassName::Script => world.server_script_service(),
        ClassName::LocalScript => world.starter_player_scripts(),
        _ => world.replicated_storage(),
    };
    world.set_parent(script, Some(parent)).unwrap();
    runtime.start_script(world, script, path, source);
    script
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

/// Runs the game for `seconds` at 10 frames a second, with Heartbeat.
fn run_for(runtime: &mut ScriptRuntime, world: &mut World, seconds: f64) {
    for _ in 0..(seconds * 10.0).round() as u32 {
        runtime.step(world, 0.1);
        runtime.heartbeat(world, 0.1);
    }
}

fn x_of(world: &World, part: InstanceId) -> f32 {
    match world.get(part).unwrap().property("Position") {
        Some(Value::Vector3(position)) => position.x,
        other => panic!("{other:?}"),
    }
}

const GREETER: &str = "
while true do
    print('Hello')
    task.wait(1)
end
";

const MOVER: &str = "
game:GetService('RunService').Heartbeat:Connect(function(dt)
    workspace.Block.Position = workspace.Block.Position + Vector3.new(dt, 0, 0)
end)
";

#[test]
fn a_reloaded_script_prints_its_new_text_and_a_part_another_script_moves_keeps_moving() {
    let (mut world, block) = world_with_block();
    let mut runtime = ScriptRuntime::new();
    let greeter = start(
        &mut runtime,
        &mut world,
        ClassName::Script,
        "scripts/Greeter.server.lua",
        GREETER,
    );
    start(
        &mut runtime,
        &mut world,
        ClassName::Script,
        "scripts/Mover.server.lua",
        MOVER,
    );
    run_for(&mut runtime, &mut world, 1.5);
    assert_eq!(lines(&mut runtime), ["Hello", "Hello"]);
    let moved = x_of(&world, block);
    assert!(moved > 1.0, "{moved}");

    let new = GREETER.replace("Hello", "Goodbye");
    runtime
        .reload_script(&mut world, greeter, "scripts/Greeter.server.lua", &new)
        .unwrap();
    run_for(&mut runtime, &mut world, 2.5);
    // The new code runs at once, then every second; the old loop never
    // prints again.
    assert_eq!(lines(&mut runtime), ["Goodbye", "Goodbye", "Goodbye"]);
    assert!(x_of(&world, block) > moved + 2.0, "the mover carried on");
    // The world was kept: the block wasn't put back.
    assert!(world.contains(block));
}

#[test]
fn after_a_reload_the_old_touched_connection_and_wait_loop_stop() {
    let (mut world, block) = world_with_block();
    let mut runtime = ScriptRuntime::new();
    let old = "
        workspace.Block.Touched:Connect(function(other)
            print('old touch ' .. other.Name)
        end)
        task.spawn(function()
            while true do
                task.wait(0.5)
                print('old tick')
            end
        end)
        task.delay(5, function() print('old delay') end)
    ";
    let door = start(
        &mut runtime,
        &mut world,
        ClassName::Script,
        "scripts/Door.server.lua",
        old,
    );
    run_for(&mut runtime, &mut world, 0.5);
    runtime.fire_touched(&mut world, &[(block, block)]);
    assert_eq!(
        lines(&mut runtime),
        ["old tick", "old touch Block", "old touch Block"]
    );

    let new = "
        workspace.Block.Touched:Connect(function(other)
            print('new touch ' .. other.Name)
        end)
    ";
    runtime
        .reload_script(&mut world, door, "scripts/Door.server.lua", new)
        .unwrap();
    run_for(&mut runtime, &mut world, 6.0);
    runtime.fire_touched(&mut world, &[(block, block)]);
    assert_eq!(
        lines(&mut runtime),
        ["new touch Block", "new touch Block"],
        "no old handler, wait loop or delay ran"
    );
}

#[test]
fn a_reload_leaves_other_scripts_connections_and_waits_alone() {
    let (mut world, block) = world_with_block();
    let mut runtime = ScriptRuntime::new();
    let listener = "
        workspace.Block.Touched:Connect(function() print('listener heard') end)
        while true do task.wait(1) print('listener tick') end
    ";
    start(
        &mut runtime,
        &mut world,
        ClassName::Script,
        "scripts/Listener.server.lua",
        listener,
    );
    let other = start(
        &mut runtime,
        &mut world,
        ClassName::Script,
        "scripts/Other.server.lua",
        "print('other 1')",
    );
    runtime
        .reload_script(
            &mut world,
            other,
            "scripts/Other.server.lua",
            "print('other 2')",
        )
        .unwrap();
    run_for(&mut runtime, &mut world, 1.0);
    runtime.fire_touched(&mut world, &[(block, block)]);
    assert_eq!(
        lines(&mut runtime),
        [
            "other 1",
            "other 2",
            "listener tick",
            "listener heard",
            "listener heard"
        ]
    );
}

#[test]
fn a_syntax_error_keeps_the_old_version_running_and_reports_its_line() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    let greeter = start(
        &mut runtime,
        &mut world,
        ClassName::Script,
        "scripts/Greeter.server.lua",
        GREETER,
    );
    lines(&mut runtime);
    let broken = "print('Goodbye')\nlocal = 5\n";
    let err = runtime
        .reload_script(&mut world, greeter, "scripts/Greeter.server.lua", broken)
        .unwrap_err();
    assert!(err.starts_with("scripts/Greeter.server.lua:2:"), "{err}");
    assert_eq!(lines(&mut runtime), [format!("error: {err}")]);
    run_for(&mut runtime, &mut world, 2.0);
    assert_eq!(
        lines(&mut runtime),
        ["Hello", "Hello"],
        "the old loop runs on"
    );
}

#[test]
fn a_changed_module_runs_again_the_scripts_that_required_it() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    let words = start(
        &mut runtime,
        &mut world,
        ClassName::ModuleScript,
        "scripts/Words.module.lua",
        "return { greeting = 'Hello' }",
    );
    // Phrases requires Words while it loads, so it loads again too.
    start(
        &mut runtime,
        &mut world,
        ClassName::ModuleScript,
        "scripts/Phrases.module.lua",
        "local Words = require(game.ReplicatedStorage.Words)\n\
         return { line = Words.greeting .. ', world' }",
    );
    start(
        &mut runtime,
        &mut world,
        ClassName::Script,
        "scripts/Speaker.server.lua",
        "local Phrases = require(game.ReplicatedStorage.Phrases)\n\
         while true do print(Phrases.line) task.wait(1) end",
    );
    start(
        &mut runtime,
        &mut world,
        ClassName::LocalScript,
        "scripts/Hud.client.lua",
        "print('client ' .. require(game.ReplicatedStorage.Words).greeting)",
    );
    start(
        &mut runtime,
        &mut world,
        ClassName::Script,
        "scripts/Bystander.server.lua",
        "while true do task.wait(1) print('bystander') end",
    );
    assert_eq!(lines(&mut runtime), ["Hello, world", "client Hello"]);

    runtime
        .reload_script(
            &mut world,
            words,
            "scripts/Words.module.lua",
            "return { greeting = 'Howdy' }",
        )
        .unwrap();
    assert_eq!(lines(&mut runtime), ["Howdy, world", "client Howdy"]);
    run_for(&mut runtime, &mut world, 1.0);
    // One Speaker loop (the new one), and the bystander wasn't restarted.
    let mut tick = lines(&mut runtime);
    tick.sort();
    assert_eq!(tick, ["Howdy, world", "bystander"]);
}

#[test]
fn a_local_script_reloads_on_the_client() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    let hud = start(
        &mut runtime,
        &mut world,
        ClassName::LocalScript,
        "scripts/Hud.client.lua",
        "game:GetService('RunService').Heartbeat:Connect(function() print('old frame') end)",
    );
    runtime
        .reload_script(
            &mut world,
            hud,
            "scripts/Hud.client.lua",
            "game:GetService('RunService').Heartbeat:Connect(function() print('new frame') end)",
        )
        .unwrap();
    run_for(&mut runtime, &mut world, 0.2);
    assert_eq!(lines(&mut runtime), ["new frame", "new frame"]);
    // Still the same player: joining happens once.
    let players = world.get(world.players()).unwrap().children().len();
    assert_eq!(players, 1);
}

#[test]
fn a_script_no_longer_in_the_game_is_not_reloaded() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    let gone = start(
        &mut runtime,
        &mut world,
        ClassName::Script,
        "scripts/Gone.server.lua",
        "print('first')",
    );
    world.destroy(gone).unwrap();
    assert!(
        runtime
            .reload_script(
                &mut world,
                gone,
                "scripts/Gone.server.lua",
                "print('again')"
            )
            .is_err()
    );
    assert_eq!(lines(&mut runtime), ["first"]);
}
