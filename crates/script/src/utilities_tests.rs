//! Tests for unified Phase 32: BindableEvents and BindableFunctions,
//! RemoteFunctions, RunService's frame hooks, HttpService's JSON and the
//! rest of the `task` library.

use axcenity_core::{ClassName, InstanceId, World};

use super::*;

/// Makes a Script or LocalScript (by the path's ending) and starts it.
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

/// The output, with the tabs `print` puts between values as spaces.
fn lines(runtime: &mut ScriptRuntime) -> Vec<String> {
    runtime
        .take_output()
        .into_iter()
        .map(|line| {
            let text = line.text.replace('\t', " ");
            match line.kind {
                OutputKind::Print => text,
                OutputKind::Error => format!("error: {text}"),
                OutputKind::Warning => format!("warning: {text}"),
            }
        })
        .collect()
}

const FRAME: f64 = 1.0 / 60.0;

const SHOP_SERVER: &str = r#"
local buy = game.ReplicatedStorage.Buy
buy.OnServerInvoke = function(player, item, count)
    if item == "sword" then
        error("no swords left")
    end
    task.wait(0.1)
    return player.Name .. " bought " .. count .. " " .. item, { left = 10 - count }
end
"#;

#[test]
fn invoke_server_returns_the_servers_values_and_waits_while_the_server_waits() {
    let (mut world, _) = world_with(ClassName::RemoteFunction, "Buy");
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        "scripts/shop.server.lua",
        SHOP_SERVER,
    );
    start(
        &mut runtime,
        &mut world,
        "scripts/buy.client.lua",
        r#"
local message, stock = game.ReplicatedStorage.Buy:InvokeServer("apple", 3)
print(message, stock.left)
"#,
    );
    // The question goes at the next step; the server waits a tenth of a second.
    runtime.step(&mut world, FRAME);
    assert!(lines(&mut runtime).is_empty());
    for _ in 0..7 {
        runtime.step(&mut world, FRAME);
    }
    assert_eq!(lines(&mut runtime), ["Player bought 3 apple 7"]);
}

#[test]
fn an_error_in_on_server_invoke_is_raised_in_the_caller_with_the_servers_line() {
    let (mut world, _) = world_with(ClassName::RemoteFunction, "Buy");
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        "scripts/shop.server.lua",
        SHOP_SERVER,
    );
    start(
        &mut runtime,
        &mut world,
        "scripts/caught.client.lua",
        r#"
local ok, err = pcall(function()
    return game.ReplicatedStorage.Buy:InvokeServer("sword", 1)
end)
print(ok, err)
"#,
    );
    start(
        &mut runtime,
        &mut world,
        "scripts/uncaught.client.lua",
        r#"game.ReplicatedStorage.Buy:InvokeServer("sword", 1)
print("never")
"#,
    );
    runtime.step(&mut world, FRAME);
    assert_eq!(
        lines(&mut runtime),
        [
            "false scripts/shop.server.lua:5: no swords left",
            "error: scripts/shop.server.lua:5: no swords left",
        ]
    );
}

#[test]
fn a_question_before_on_server_invoke_is_set_waits_for_it() {
    let (mut world, _) = world_with(ClassName::RemoteFunction, "Ask");
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        "scripts/late.server.lua",
        r#"
task.wait(0.5)
game.ReplicatedStorage.Ask.OnServerInvoke = function(player, n)
    return n * 2
end
"#,
    );
    start(
        &mut runtime,
        &mut world,
        "scripts/ask.client.lua",
        "print(game.ReplicatedStorage.Ask:InvokeServer(21))",
    );
    for _ in 0..20 {
        runtime.step(&mut world, FRAME);
    }
    assert!(lines(&mut runtime).is_empty());
    for _ in 0..15 {
        runtime.step(&mut world, FRAME);
    }
    assert_eq!(lines(&mut runtime), ["42"]);
}

#[test]
fn remote_function_members_only_work_on_their_side() {
    let (mut world, _) = world_with(ClassName::RemoteFunction, "Ask");
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        "scripts/wrong.server.lua",
        "game.ReplicatedStorage.Ask:InvokeServer()",
    );
    start(
        &mut runtime,
        &mut world,
        "scripts/wrong.client.lua",
        "game.ReplicatedStorage.Ask.OnServerInvoke = function() end",
    );
    let output = lines(&mut runtime);
    assert!(
        output[0].starts_with(
            "error: scripts/wrong.server.lua:1: InvokeServer only works in a LocalScript"
        ),
        "{output:?}"
    );
    assert!(
        output[1].starts_with(
            "error: scripts/wrong.client.lua:1: OnServerInvoke only works in a Script"
        ),
        "{output:?}"
    );
}

#[test]
fn a_bindable_event_reaches_scripts_on_its_own_side_only() {
    let (mut world, _) = world_with(ClassName::BindableEvent, "Scored");
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        "scripts/listen.server.lua",
        r#"
game.ReplicatedStorage.Scored.Event:Connect(function(points, info)
    print("scored", points, info.by)
    info.seen = true
end)
"#,
    );
    start(
        &mut runtime,
        &mut world,
        "scripts/listen.client.lua",
        r#"game.ReplicatedStorage.Scored.Event:Connect(function() print("client heard") end)"#,
    );
    start(
        &mut runtime,
        &mut world,
        "scripts/fire.server.lua",
        r#"
local info = { by = "Ann" }
game.ReplicatedStorage.Scored:Fire(5, info)
print("same table", info.seen)
game.ReplicatedStorage.Scored.Fire(5)
"#,
    );
    let output = lines(&mut runtime);
    assert_eq!(output[..2], ["scored 5 Ann", "same table true"]);
    assert!(
        output[2].contains("call Fire with a colon: event:Fire(...)"),
        "{output:?}"
    );
}

#[test]
fn a_bindable_function_returns_what_on_invoke_returns_even_after_waiting() {
    let (mut world, _) = world_with(ClassName::BindableFunction, "Double");
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        "scripts/early.server.lua",
        r#"
local ok, err = pcall(function() return game.ReplicatedStorage.Double:Invoke(1) end)
print(err)
print(pcall(function() return game.ReplicatedStorage.Double.OnInvoke end))
"#,
    );
    start(
        &mut runtime,
        &mut world,
        "scripts/double.server.lua",
        r#"
game.ReplicatedStorage.Double.OnInvoke = function(n)
    task.wait(0.05)
    return n * 2, "done"
end
"#,
    );
    start(
        &mut runtime,
        &mut world,
        "scripts/ask.server.lua",
        "print(game.ReplicatedStorage.Double:Invoke(4))",
    );
    let output = lines(&mut runtime);
    assert!(
        output[0].contains("BindableFunction \"Double\" has no OnInvoke on this side"),
        "{output:?}"
    );
    assert!(
        output[1].contains("OnInvoke is a callback member of BindableFunction"),
        "{output:?}"
    );
    for _ in 0..4 {
        runtime.step(&mut world, FRAME);
    }
    assert_eq!(lines(&mut runtime), ["8 done"]);
}

#[test]
fn bind_to_render_step_runs_in_priority_order_every_frame_then_render_stepped() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        "scripts/render.client.lua",
        r#"
local RunService = game:GetService("RunService")
RunService:BindToRenderStep("late", 300, function(dt) print("late") end)
RunService.RenderStepped:Connect(function(dt) print("rendered", dt) end)
RunService:BindToRenderStep("early", Enum.RenderPriority.First.Value, function(dt) print("early", dt) end)
RunService:BindToRenderStep("camera", Enum.RenderPriority.Camera, function() print("camera") end)
RunService:BindToRenderStep("tie", 300, function() print("tie") end)
"#,
    );
    runtime.render_step(&mut world, 0.5);
    let frame = ["early 0.5", "camera", "late", "tie", "rendered 0.5"];
    assert_eq!(lines(&mut runtime), frame);
    runtime.step(&mut world, 0.5);
    runtime.render_step(&mut world, 0.5);
    assert_eq!(lines(&mut runtime), frame);

    runtime.run_command(&mut world, "print('server')").unwrap();
    start(
        &mut runtime,
        &mut world,
        "scripts/unbind.client.lua",
        r#"game:GetService("RunService"):UnbindFromRenderStep("camera")"#,
    );
    runtime.render_step(&mut world, 0.25);
    assert_eq!(
        lines(&mut runtime),
        ["server", "early 0.25", "late", "tie", "rendered 0.25"]
    );
}

#[test]
fn stepped_fires_before_physics_with_the_game_time_and_render_step_is_client_only() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        "scripts/stepped.server.lua",
        r#"
local RunService = game:GetService("RunService")
RunService.Stepped:Connect(function(time, dt) print("stepped", time, dt) end)
print(RunService:IsServer(), RunService:IsClient(), RunService:IsStudio(), RunService:IsRunning())
print(pcall(function() return RunService.RenderStepped end))
"#,
    );
    start(
        &mut runtime,
        &mut world,
        "scripts/sides.client.lua",
        r#"
local RunService = game:GetService("RunService")
print(RunService:IsServer(), RunService:IsClient())
"#,
    );
    let output = lines(&mut runtime);
    assert_eq!(output[0], "true false false true");
    assert!(
        output[1].starts_with(
            "false scripts/stepped.server.lua:5: RenderStepped only works in a LocalScript"
        ),
        "{output:?}"
    );
    assert_eq!(output[2], "false true");
    runtime.step(&mut world, 0.5);
    runtime.stepped(&mut world, 0.5);
    runtime.step(&mut world, 0.25);
    runtime.stepped(&mut world, 0.25);
    assert_eq!(
        lines(&mut runtime),
        ["stepped 0.5 0.5", "stepped 0.75 0.25"]
    );

    runtime.set_studio(true);
    runtime
        .run_command(
            &mut world,
            "print(game:GetService('RunService'):IsStudio())",
        )
        .unwrap();
    assert_eq!(lines(&mut runtime), ["true"]);
    let edit = run_edit_command(
        &mut world,
        "local r = game:GetService('RunService') print(r:IsStudio(), r:IsRunning())",
    )
    .unwrap();
    assert_eq!(edit.output[0].text, "true\tfalse");
}

#[test]
fn json_round_trips_nested_tables_and_bad_json_errors_where_the_mistake_is() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        "scripts/json.server.lua",
        r#"
local Http = game:GetService("HttpService")
local data = {
    name = "Ann",
    level = 3,
    ratio = 0.5,
    alive = true,
    items = { "sword", "shield", { kind = "potion", uses = { 1, 2 } } },
    empty = {},
}
local text = Http:JSONEncode(data)
print(text)
local back = Http:JSONDecode(text)
print(back.name, back.level, back.ratio, back.alive, #back.items, back.items[3].kind, back.items[3].uses[2], #back.empty)
print(Http:JSONEncode(Http:JSONDecode(text)) == text)
print(pcall(Http.JSONDecode, Http, '{"a": 1,, "b": 2}'))
print(pcall(Http.JSONDecode, Http, '[1, 2,\n 3'))
local loop = {}
loop.self = loop
print(pcall(Http.JSONEncode, Http, loop))
print(pcall(Http.JSONEncode, Http, { 1, 2, x = 3 }))
print(pcall(Http.JSONEncode, Http, { print }))
print(#Http:GenerateGUID(), #Http:GenerateGUID(false), Http:GenerateGUID() ~= Http:GenerateGUID())
"#,
    );
    let output = lines(&mut runtime);
    assert_eq!(
        output[0],
        r#"{"alive":true,"empty":[],"items":["sword","shield",{"kind":"potion","uses":[1,2]}],"level":3,"name":"Ann","ratio":0.5}"#
    );
    assert_eq!(output[1], "Ann 3 0.5 true 3 potion 2 0");
    assert_eq!(output[2], "true");
    assert_eq!(
        output[3],
        "false scripts/json.server.lua:16: JSONDecode can't read this JSON: key must be a string at line 1 column 9"
    );
    assert_eq!(
        output[4],
        "false scripts/json.server.lua:17: JSONDecode can't read this JSON: EOF while parsing a list at line 2 column 2"
    );
    assert!(
        output[5].ends_with("JSONEncode can't write a table that contains itself"),
        "{output:?}"
    );
    assert!(
        output[6].contains("JSONEncode can't write a table with a"),
        "{output:?}"
    );
    assert!(
        output[7].contains("JSONEncode can't write a function"),
        "{output:?}"
    );
    assert_eq!(output[8], "38 36 true");
}

#[test]
fn task_defer_runs_at_the_next_step_and_task_cancel_stops_a_thread() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        "scripts/tasks.server.lua",
        r#"
task.defer(function(word) print("deferred", word) end, "hi")
local delayed = task.delay(0.1, function() print("never") end)
task.cancel(delayed)
local waiting = task.spawn(function()
    task.wait(0.1)
    print("never either")
end)
task.cancel(waiting)
task.synchronize()
task.desynchronize()
print(coroutine.status(delayed), coroutine.status(waiting))
print(pcall(task.cancel, 5))
print("started")
"#,
    );
    let output = lines(&mut runtime);
    assert_eq!(output[0], "dead dead");
    assert!(
        output[1].contains("task.cancel takes a thread"),
        "{output:?}"
    );
    assert_eq!(output[2], "started");
    for _ in 0..12 {
        runtime.step(&mut world, FRAME);
    }
    assert_eq!(lines(&mut runtime), ["deferred hi"]);
}
