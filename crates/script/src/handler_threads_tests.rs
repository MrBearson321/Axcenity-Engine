//! Reusing finished handler coroutines (`handler_threads`) must not change
//! what scripts see: waiting, errors, `coroutine.running`, nested events
//! and the watchdog all behave as with a new coroutine per call.

use std::time::{Duration, Instant};

use axcenity_core::{ClassName, World};

use super::*;

fn start(runtime: &mut ScriptRuntime, world: &mut World, source: &str) {
    let script = world.create(ClassName::Script).unwrap();
    world
        .set_parent(script, Some(world.server_script_service()))
        .unwrap();
    runtime.start_script(world, script, "scripts/test.server.lua", source);
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

fn free(runtime: &ScriptRuntime) -> usize {
    handler_threads::free_count(&runtime.server.lua)
}

#[test]
fn a_handler_that_finishes_leaves_its_coroutine_for_the_next_call() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        "local n = 0
        game:GetService('RunService').Heartbeat:Connect(function(dt)
            n += 1
            local mine = n
            print('beat', mine, dt)
        end)",
    );
    runtime.heartbeat(&mut world, 0.5);
    assert_eq!(free(&runtime), 1);
    runtime.heartbeat(&mut world, 0.25);
    assert_eq!(free(&runtime), 1, "the same coroutine went back");
    assert_eq!(lines(&mut runtime), ["beat\t1\t0.5", "beat\t2\t0.25"]);
}

#[test]
fn a_handler_that_waits_keeps_its_coroutine_and_resumes() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        "local n = 0
        game:GetService('RunService').Heartbeat:Connect(function()
            n += 1
            local mine = n
            if mine <= 2 then
                task.wait(0.1)
                print('woke', mine)
            else
                print('quick', mine)
            end
        end)",
    );
    runtime.heartbeat(&mut world, 0.0);
    runtime.heartbeat(&mut world, 0.0);
    assert_eq!(free(&runtime), 0, "both are waiting");
    runtime.heartbeat(&mut world, 0.0);
    runtime.step(&mut world, 0.2);
    assert_eq!(lines(&mut runtime), ["quick\t3", "woke\t1", "woke\t2"]);
}

#[test]
fn a_coroutine_a_script_asked_for_is_never_given_to_another_handler() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        "local first
        game:GetService('RunService').Heartbeat:Connect(function()
            if first == nil then
                first = coroutine.running()
            else
                print(coroutine.running() == first, coroutine.status(first))
            end
        end)",
    );
    runtime.heartbeat(&mut world, 0.0);
    assert_eq!(free(&runtime), 0, "handed out, so not kept");
    runtime.heartbeat(&mut world, 0.0);
    assert_eq!(lines(&mut runtime), ["false\tdead"]);
}

#[test]
fn a_handler_that_fails_is_reported_and_the_next_call_runs() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        "local n = 0
        game:GetService('RunService').Heartbeat:Connect(function()
            n += 1
            if n == 1 then error('first call fails') end
            print('call', n)
        end)",
    );
    runtime.heartbeat(&mut world, 0.0);
    runtime.heartbeat(&mut world, 0.0);
    runtime.heartbeat(&mut world, 0.0);
    let output = lines(&mut runtime);
    assert_eq!(output.len(), 3, "{output:?}");
    assert!(output[0].contains("first call fails"), "{output:?}");
    assert_eq!(output[1..], ["call\t2", "call\t3"]);
}

#[test]
fn a_handler_that_fires_another_event_runs_both_in_order() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        "local inner = Instance.new('BindableEvent')
        inner.Event:Connect(function(x) print('inner', x) end)
        game:GetService('RunService').Heartbeat:Connect(function()
            print('outer start')
            inner:Fire(1)
            inner:Fire(2)
            print('outer end')
        end)",
    );
    runtime.heartbeat(&mut world, 0.0);
    runtime.heartbeat(&mut world, 0.0);
    assert_eq!(
        lines(&mut runtime),
        [
            "outer start",
            "inner\t1",
            "inner\t2",
            "outer end",
            "outer start",
            "inner\t1",
            "inner\t2",
            "outer end"
        ]
    );
}

#[test]
fn a_reused_coroutine_starts_clean() {
    // Varargs and the stack from the last call are gone.
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        "game:GetService('RunService').Heartbeat:Connect(function(...)
            print(select('#', ...), coroutine.isyieldable())
        end)",
    );
    runtime.heartbeat(&mut world, 0.0);
    runtime.heartbeat(&mut world, 0.0);
    assert_eq!(lines(&mut runtime), ["1\ttrue", "1\ttrue"]);
}

#[test]
fn the_watchdog_still_stops_a_handler_that_never_pauses() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    runtime.set_watchdog_limit(Duration::from_millis(100));
    start(
        &mut runtime,
        &mut world,
        "local n = 0
        game:GetService('RunService').Heartbeat:Connect(function()
            n += 1
            if n == 2 then while true do end end
            print('call', n)
        end)",
    );
    runtime.heartbeat(&mut world, 0.0);
    runtime.heartbeat(&mut world, 0.0);
    runtime.heartbeat(&mut world, 0.0);
    let output = lines(&mut runtime);
    assert_eq!(output.len(), 3, "{output:?}");
    assert_eq!(output[0], "call\t1");
    assert!(output[1].contains("without pausing"), "{output:?}");
    assert_eq!(output[2], "call\t3");
}

/// Firing `Heartbeat` with 1,000 handlers that don't wait. Not a pass/fail
/// test:
/// `cargo test -p axcenity-script --release --lib -- --ignored bench_handler_fire --nocapture`
#[test]
#[ignore]
fn bench_handler_fire() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        "local total = 0
        for i = 1, 1000 do
            game:GetService('RunService').Heartbeat:Connect(function(dt)
                total += dt
            end)
        end",
    );
    let frames = 1_000;
    let start_time = Instant::now();
    for _ in 0..frames {
        runtime.heartbeat(&mut world, 1.0 / 60.0);
    }
    let elapsed = start_time.elapsed();
    println!(
        "Heartbeat with 1,000 handlers: {:.0} ns per handler call ({frames} frames in {elapsed:?})",
        elapsed.as_nanos() as f64 / f64::from(frames * 1000)
    );
}
