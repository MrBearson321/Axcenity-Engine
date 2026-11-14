//! The script profiler: work timed by handler, script and task, and
//! sampled time by function.

use std::time::Duration;

use axcenity_core::{ClassName, World};

use super::*;

fn start(runtime: &mut ScriptRuntime, world: &mut World, path: &str, source: &str) {
    let script = world.create(ClassName::Script).unwrap();
    world
        .set_parent(script, Some(world.server_script_service()))
        .unwrap();
    runtime.start_script(world, script, path, source);
}

/// Runs a handler that spends about 6 ms in `busy` and one that does
/// almost nothing, on three Heartbeats.
const HANDLERS: &str = r#"
local RunService = game:GetService("RunService")

local function busy(seconds)
    local stop = os.clock() + seconds
    local n = 0
    while os.clock() < stop do
        n += 1
    end
    return n
end

local function onHeartbeat()
    busy(0.006)
end

local function cheap()
    local x = 1 + 1
end

RunService.Heartbeat:Connect(onHeartbeat)
RunService.Heartbeat:Connect(cheap)
"#;

#[test]
fn nothing_is_recorded_until_recording_starts() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        "scripts/game.server.lua",
        HANDLERS,
    );
    runtime.heartbeat(&mut world, 1.0 / 60.0);
    assert_eq!(runtime.script_profile(), ScriptProfile::default());
}

#[test]
fn an_expensive_handler_and_the_function_it_calls_come_first() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    runtime.set_profiling(true);
    start(
        &mut runtime,
        &mut world,
        "scripts/game.server.lua",
        HANDLERS,
    );
    for _ in 0..3 {
        runtime.heartbeat(&mut world, 1.0 / 60.0);
    }
    let profile = runtime.script_profile();

    let top = &profile.work[0];
    assert_eq!(top.kind, WorkKind::Handler);
    assert_eq!(top.what, "RunService.Heartbeat");
    assert_eq!(top.function.as_deref(), Some("onHeartbeat"));
    assert_eq!(
        (top.path.as_str(), top.line),
        ("scripts/game.server.lua", 13)
    );
    assert_eq!((top.calls, top.resumes), (3, 3));
    assert!(top.time >= Duration::from_millis(18), "{:?}", top.time);
    assert!(top.longest >= Duration::from_millis(6));
    assert!(!top.client);

    let cheap = profile
        .work
        .iter()
        .find(|row| row.function.as_deref() == Some("cheap"))
        .unwrap();
    assert_eq!(cheap.calls, 3);
    assert!(cheap.time < top.time / 10);
    let body = profile
        .work
        .iter()
        .find(|row| row.kind == WorkKind::Script)
        .unwrap();
    assert_eq!((body.what.as_str(), body.calls), ("game.server.lua", 1));
    assert_eq!(profile.total, profile.work.iter().map(|row| row.time).sum());

    // Sampled: nearly all of it is spent in `busy`'s loop, inside
    // `onHeartbeat`.
    let busy = &profile.functions[0];
    assert_eq!(busy.name.as_deref(), Some("busy"));
    assert_eq!((busy.line, busy.hottest_line), (4, 7));
    assert!(
        busy.self_time >= Duration::from_millis(12),
        "{:?}",
        busy.self_time
    );
    let handler = profile
        .functions
        .iter()
        .find(|row| row.name.as_deref() == Some("onHeartbeat"))
        .unwrap();
    assert!(handler.total_time >= busy.total_time);
    assert!(handler.self_time < busy.self_time / 4);
}

#[test]
fn a_spawned_task_is_its_own_row_and_its_time_isnt_counted_twice() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    runtime.set_profiling(true);
    let source = r#"
        local function work()
            local stop = os.clock() + 0.01
            while os.clock() < stop do end
        end
        task.spawn(work)
        task.delay(0, work)
    "#;
    start(&mut runtime, &mut world, "scripts/spawn.server.lua", source);
    runtime.step(&mut world, 1.0 / 60.0);
    let profile = runtime.script_profile();
    let spawned = profile
        .work
        .iter()
        .find(|row| row.what == "task.spawn")
        .unwrap();
    let delayed = profile
        .work
        .iter()
        .find(|row| row.what == "task.delay")
        .unwrap();
    let body = profile
        .work
        .iter()
        .find(|row| row.kind == WorkKind::Script)
        .unwrap();
    for task in [spawned, delayed] {
        assert_eq!(task.kind, WorkKind::Task);
        assert_eq!(task.function.as_deref(), Some("work"));
        assert!(task.time >= Duration::from_millis(10));
    }
    // The body ran `task.spawn(work)` inside its own resume, but that
    // time is the task's.
    assert!(body.time < Duration::from_millis(5), "{:?}", body.time);
}

#[test]
fn a_handler_that_waits_counts_its_later_resumes_and_clear_forgets_it() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    runtime.set_profiling(true);
    let source = r#"
        local function onHeartbeat()
            task.wait(0.01)
        end
        game:GetService("RunService").Heartbeat:Once(onHeartbeat)
    "#;
    start(&mut runtime, &mut world, "scripts/wait.server.lua", source);
    runtime.heartbeat(&mut world, 1.0 / 60.0);
    runtime.step(&mut world, 0.02);
    let profile = runtime.script_profile();
    let handler = profile
        .work
        .iter()
        .find(|row| row.kind == WorkKind::Handler)
        .unwrap();
    assert_eq!((handler.calls, handler.resumes), (1, 2));

    runtime.clear_profile();
    assert_eq!(runtime.script_profile(), ScriptProfile::default());
    runtime.set_profiling(false);
}
