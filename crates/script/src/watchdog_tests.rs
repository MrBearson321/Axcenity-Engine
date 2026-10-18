//! The watchdog: scripts that run too long without pausing are stopped, and
//! everything else carries on.

use std::time::{Duration, Instant};

use axcenity_core::{ClassName, World};

use super::*;

/// Short, so the tests are quick. The real limit is [`WATCHDOG_LIMIT`].
const LIMIT: Duration = Duration::from_millis(100);

fn runtime() -> ScriptRuntime {
    let mut runtime = ScriptRuntime::new();
    runtime.set_watchdog_limit(LIMIT);
    runtime
}

/// Starts `source` as a Script, a LocalScript (`.client.lua`) or a
/// ModuleScript (`.module.lua`) named after its file.
fn start(runtime: &mut ScriptRuntime, world: &mut World, path: &str, source: &str) {
    let (class, service) = if path.ends_with(".client.lua") {
        (ClassName::LocalScript, world.starter_player_scripts())
    } else if path.ends_with(".module.lua") {
        (ClassName::ModuleScript, world.replicated_storage())
    } else {
        (ClassName::Script, world.server_script_service())
    };
    let script = world.create(class).unwrap();
    let file = path.rsplit('/').next().unwrap();
    world
        .set_name(script, file.split('.').next().unwrap())
        .unwrap();
    world.set_parent(script, Some(service)).unwrap();
    runtime.start_script(world, script, path, source);
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

/// The start of the watchdog's message for the test limit.
const STOPPED: &str = "this script ran for 0.1 seconds without pausing, so it was stopped";

#[test]
fn the_limit_is_three_seconds() {
    assert_eq!(WATCHDOG_LIMIT, Duration::from_secs(3));
}

#[test]
fn an_endless_loop_is_stopped_with_the_script_and_line() {
    let mut world = World::new();
    let mut runtime = runtime();
    let started = Instant::now();
    start(
        &mut runtime,
        &mut world,
        "scripts/loop.server.lua",
        "local x = 0\nwhile true do\n  x = x + 1\nend",
    );
    let took = started.elapsed();
    // Started after the stopped one, and runs as normal.
    start(
        &mut runtime,
        &mut world,
        "scripts/next.server.lua",
        "print('next')",
    );

    let output = lines(&mut runtime);
    assert_eq!(output.len(), 2, "{output:?}");
    // Line 2 or 3: the check can land on the `while` or on its body.
    assert!(
        output[0].starts_with("error: scripts/loop.server.lua:2: ")
            || output[0].starts_with("error: scripts/loop.server.lua:3: "),
        "{output:?}"
    );
    assert!(output[0].contains(STOPPED), "{output:?}");
    assert!(output[0].ends_with("put task.wait() inside long loops"));
    assert_eq!(output[1], "next");
    assert!(
        took >= LIMIT && took < LIMIT + Duration::from_secs(2),
        "{took:?}"
    );

    // The game goes on.
    runtime.step(&mut world, 1.0 / 60.0);
    assert!(lines(&mut runtime).is_empty());
}

#[test]
fn an_endless_loop_cannot_be_caught_with_pcall_or_xpcall() {
    for source in [
        "while true do pcall(function() while true do end end) end",
        "while true do xpcall(function() while true do end end, function(e) return e end) end",
        // The handler loops too.
        "xpcall(function() while true do end end, function() while true do end end)",
        "while true do pcall(pcall, function() while true do end end) end",
    ] {
        let mut world = World::new();
        let mut runtime = runtime();
        start(
            &mut runtime,
            &mut world,
            "scripts/sneaky.server.lua",
            source,
        );
        let output = lines(&mut runtime);
        assert_eq!(output.len(), 1, "{source}: {output:?}");
        assert!(
            output[0].starts_with("error: scripts/sneaky.server.lua:1: ")
                && output[0].contains(STOPPED),
            "{source}: {output:?}"
        );
    }
}

#[test]
fn an_endless_loop_in_a_coroutine_the_script_made_is_stopped() {
    let mut world = World::new();
    let mut runtime = runtime();
    start(
        &mut runtime,
        &mut world,
        "scripts/co.server.lua",
        r#"
        local co = coroutine.create(function()
            while true do end
        end)
        local ok, err = coroutine.resume(co)
        print(ok, coroutine.status(co), err)
        "#,
    );
    let output = lines(&mut runtime);
    assert_eq!(output.len(), 1, "{output:?}");
    assert!(
        output[0].starts_with("false\tdead\tscripts/co.server.lua:3: ")
            && output[0].contains(STOPPED),
        "{output:?}"
    );
}

#[test]
fn endless_close_handlers_are_stopped_too() {
    // `<close>` variables run their `__close` when the code around them
    // ends, even when it's stopped.
    let mut world = World::new();
    let mut runtime = runtime();
    start(
        &mut runtime,
        &mut world,
        "scripts/close.server.lua",
        r#"
        local stuck = { __close = function() while true do end end }
        local co = coroutine.create(function()
            local guard <close> = setmetatable({}, stuck)
            while true do end
        end)
        print((coroutine.resume(co)))
        print((coroutine.close(co)))
        print((pcall(function()
            local guard <close> = setmetatable({}, stuck)
            while true do end
        end)))
        "#,
    );
    let output = lines(&mut runtime);
    assert_eq!(output[0], "false", "{output:?}");
    assert!(
        output
            .last()
            .unwrap()
            .starts_with("error: scripts/close.server.lua:"),
        "{output:?}"
    );
}

#[test]
fn endless_loops_where_lua_cannot_pause_are_stopped_with_an_error() {
    // Lua called from inside a C function can't yield, so the watchdog
    // raises its error there instead, and `pcall` doesn't catch it.
    for source in [
        "table.sort({ 3, 2, 1 }, function() while true do end end)",
        "while true do pcall(table.sort, { 3, 2, 1 }, function() while true do end end) end",
        "string.gsub('abc', '%w', function() while true do end end)",
        "print(setmetatable({}, { __tostring = function() while true do end end }))",
        "while true do pcall(tostring, setmetatable({}, { __tostring = function() while true do end end })) end",
    ] {
        let mut world = World::new();
        let mut runtime = runtime();
        start(&mut runtime, &mut world, "scripts/c.server.lua", source);
        let output = lines(&mut runtime);
        assert_eq!(output.len(), 1, "{source}: {output:?}");
        assert!(
            output[0].starts_with("error: scripts/c.server.lua:1: ") && output[0].contains(STOPPED),
            "{source}: {output:?}"
        );
    }
}

#[test]
fn a_stopped_coroutine_looks_dead_and_cannot_be_resumed_or_closed() {
    let mut world = World::new();
    let mut runtime = runtime();
    start(
        &mut runtime,
        &mut world,
        "scripts/co.server.lua",
        r#"
        local co = coroutine.create(function() while true do end end)
        coroutine.resume(co)
        print(coroutine.status(co), coroutine.resume(co))
        print((coroutine.close(co)))
        local wrapped = coroutine.wrap(function() while true do end end)
        print(pcall(wrapped))
        print(pcall(wrapped))
        "#,
    );
    let output = lines(&mut runtime);
    assert_eq!(output.len(), 4, "{output:?}");
    assert_eq!(output[0], "dead\tfalse\tcannot resume dead coroutine");
    assert_eq!(output[1], "false");
    assert!(
        output[2].starts_with("false\tscripts/co.server.lua:6: ") && output[2].contains(STOPPED),
        "{output:?}"
    );
    // No location: `pcall` called it, and that isn't script code.
    assert_eq!(output[3], "false\tcannot resume dead coroutine");
}

#[test]
fn a_stopped_task_cannot_be_resumed_by_the_script() {
    // `task.spawn` returns the coroutine it started, which the scheduler
    // stopped, so the script holds a handle to it.
    let mut world = World::new();
    let mut runtime = runtime();
    start(
        &mut runtime,
        &mut world,
        "scripts/b.server.lua",
        r#"
        local co = task.spawn(function() while true do end end)
        print(coroutine.status(co), coroutine.resume(co))
        "#,
    );
    let output = lines(&mut runtime);
    assert_eq!(output.len(), 2, "{output:?}");
    assert!(
        output[0].starts_with("error: scripts/b.server.lua:2: "),
        "{output:?}"
    );
    assert_eq!(output[1], "dead\tfalse\tcannot resume dead coroutine");
}

#[test]
fn resuming_stopped_coroutines_in_a_loop_is_stopped_too() {
    let mut world = World::new();
    let mut runtime = runtime();
    let started = Instant::now();
    start(
        &mut runtime,
        &mut world,
        "scripts/co.server.lua",
        "while true do\n  coroutine.resume(coroutine.create(function() while true do end end))\nend",
    );
    let took = started.elapsed();
    let output = lines(&mut runtime);
    assert_eq!(output.len(), 1, "{output:?}");
    assert!(
        output[0].starts_with("error: scripts/co.server.lua:"),
        "{output:?}"
    );
    // Stopped once the budget ran out, not after a new budget per coroutine
    // (which would never end).
    assert!(took < Duration::from_secs(3), "{took:?}");
}

#[test]
fn endless_loops_in_handlers_and_tasks_are_stopped_and_reported_once() {
    let mut world = World::new();
    let pad = world.create(ClassName::Part).unwrap();
    world.set_name(pad, "Pad").unwrap();
    world.set_parent(pad, Some(world.workspace())).unwrap();
    let ball = world.create(ClassName::Part).unwrap();
    world.set_parent(ball, Some(world.workspace())).unwrap();

    let mut runtime = runtime();
    start(
        &mut runtime,
        &mut world,
        "scripts/tasks.server.lua",
        r#"
        task.spawn(function() while true do end end)
        print("after spawn")
        task.delay(0.5, function() while true do end end)
        workspace.Pad.Touched:Connect(function() while true do end end)
        workspace.Pad.Touched:Connect(function() print("other handler") end)
        "#,
    );
    let output = lines(&mut runtime);
    assert_eq!(output.len(), 2, "{output:?}");
    assert!(
        output[0].starts_with("error: scripts/tasks.server.lua:2: ") && output[0].contains(STOPPED),
        "{output:?}"
    );
    assert_eq!(output[1], "after spawn");

    runtime.fire_touched(&mut world, &[(pad, ball)]);
    let output = lines(&mut runtime);
    assert_eq!(output.len(), 2, "{output:?}");
    assert!(output[0].contains(STOPPED), "{output:?}");
    assert_eq!(output[1], "other handler");

    for _ in 0..40 {
        runtime.step(&mut world, 1.0 / 60.0);
    }
    let output = lines(&mut runtime);
    assert_eq!(output.len(), 1, "{output:?}");
    assert!(
        output[0].starts_with("error: scripts/tasks.server.lua:4: "),
        "{output:?}"
    );
}

#[test]
fn a_stopped_script_does_not_stop_others_in_the_same_frame() {
    let mut world = World::new();
    let mut runtime = runtime();
    start(
        &mut runtime,
        &mut world,
        "scripts/a.server.lua",
        "task.wait()\nwhile true do end",
    );
    start(
        &mut runtime,
        &mut world,
        "scripts/b.server.lua",
        "task.wait()\nprint('b')",
    );
    start(
        &mut runtime,
        &mut world,
        "scripts/c.client.lua",
        "task.wait()\nwhile true do end",
    );
    runtime.step(&mut world, 1.0 / 60.0);
    let output = lines(&mut runtime);
    assert_eq!(output.len(), 3, "{output:?}");
    assert!(output[0].starts_with("error: scripts/a.server.lua:2: "));
    assert_eq!(output[1], "b");
    // LocalScripts are watched too.
    assert!(output[2].starts_with("error: scripts/c.client.lua:2: "));
}

#[test]
fn an_endless_loop_in_a_module_is_stopped() {
    let mut world = World::new();
    let mut runtime = runtime();
    start(
        &mut runtime,
        &mut world,
        "scripts/stuck.module.lua",
        "while true do end\nreturn 1",
    );
    start(
        &mut runtime,
        &mut world,
        "scripts/main.server.lua",
        "print(pcall(require, game.ReplicatedStorage.stuck))",
    );
    let output = lines(&mut runtime);
    assert_eq!(output.len(), 1, "{output:?}");
    assert!(
        output[0].starts_with("error: scripts/stuck.module.lua:1: ") && output[0].contains(STOPPED),
        "{output:?}"
    );
}

#[test]
fn heavy_work_that_pauses_is_not_stopped() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    // Tighter than the other tests, so that the work takes longer than the
    // limit in all even on a fast machine.
    let limit = Duration::from_millis(20);
    runtime.set_watchdog_limit(limit);
    let started = Instant::now();
    // Five million iterations, pausing every 50 thousand: far longer than
    // the limit in all, but never long between pauses.
    start(
        &mut runtime,
        &mut world,
        "scripts/heavy.server.lua",
        r#"
        local total = 0
        for i = 1, 5000000 do
            total = total + i % 7
            if i % 50000 == 0 then
                task.wait()
            end
        end
        print("done", total)
        "#,
    );
    let mut frames = 0;
    let mut output = lines(&mut runtime);
    while output.is_empty() && frames < 200 {
        runtime.step(&mut world, 1.0 / 60.0);
        frames += 1;
        output = lines(&mut runtime);
    }
    let took = started.elapsed();
    assert_eq!(output.len(), 1, "{output:?}");
    assert!(output[0].starts_with("done\t"), "{output:?}");
    assert_eq!(frames, 100, "it should finish after 100 pauses");
    assert!(
        took > limit,
        "the work should take longer than the limit in all ({took:?}), to show pausing resets it"
    );
}

#[test]
fn heavy_work_without_pausing_is_fine_under_the_real_limit() {
    let mut world = World::new();
    // The real limit: three million iterations in one go is ordinary work.
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        "scripts/heavy.server.lua",
        r#"
        local parts = {}
        for i = 1, 3000000 do
            parts[i % 1000 + 1] = i * 2
        end
        print("done")
        "#,
    );
    assert_eq!(lines(&mut runtime), ["done"]);
}
