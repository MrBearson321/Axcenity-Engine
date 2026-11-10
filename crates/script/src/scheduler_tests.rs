//! The order tasks resume in, cancelling, and dropping handlers of what's
//! gone. The queue is a heap with tombstones, so these pin what it must
//! match: earliest due first, then first scheduled.

use std::time::Instant;

use axcenity_core::{ClassName, InstanceId, World};
use mlua::{Lua, MultiValue};

use super::*;
use crate::scheduler::Scheduler;

fn run(runtime: &mut ScriptRuntime, world: &mut World, source: &str) -> InstanceId {
    let script = world.create(ClassName::Script).unwrap();
    world
        .set_parent(script, Some(world.server_script_service()))
        .unwrap();
    runtime.start_script(world, script, "scripts/test.server.lua", source);
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

fn part(world: &mut World, name: &str) -> InstanceId {
    let part = world.create(ClassName::Part).unwrap();
    world.set_name(part, name).unwrap();
    world.set_parent(part, Some(world.workspace())).unwrap();
    part
}

#[test]
fn wait_delay_and_defer_due_together_resume_in_the_order_they_were_scheduled() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    run(
        &mut runtime,
        &mut world,
        r#"
        task.defer(print, "defer 1")
        task.delay(0, print, "delay 0")
        task.spawn(function()
            task.wait()
            print("wait")
        end)
        task.defer(print, "defer 2")
        task.delay(0, print, "delay 0 again")
        "#,
    );
    assert_eq!(lines(&mut runtime), Vec::<String>::new());
    runtime.step(&mut world, 0.25);
    assert_eq!(
        lines(&mut runtime),
        ["defer 1", "delay 0", "wait", "defer 2", "delay 0 again"]
    );
}

#[test]
fn earlier_due_tasks_resume_first_whenever_they_were_scheduled() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    run(
        &mut runtime,
        &mut world,
        r#"
        task.delay(0.75, print, "0.75")
        task.delay(0.25, print, "0.25, first")
        task.delay(0.5, print, "0.5")
        task.delay(0.25, print, "0.25, second")
        "#,
    );
    // One long frame makes them all due at once.
    runtime.step(&mut world, 1.0);
    assert_eq!(
        lines(&mut runtime),
        ["0.25, first", "0.25, second", "0.5", "0.75"]
    );
}

#[test]
fn a_task_scheduled_while_tasks_resume_waits_for_the_next_step() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    run(
        &mut runtime,
        &mut world,
        r#"
        task.delay(0, function()
            print("first")
            task.defer(print, "deferred from first")
            task.delay(0, print, "delayed from first")
        end)
        task.delay(0, print, "second")
        "#,
    );
    runtime.step(&mut world, 0.25);
    assert_eq!(lines(&mut runtime), ["first", "second"]);
    runtime.step(&mut world, 0.25);
    assert_eq!(
        lines(&mut runtime),
        ["deferred from first", "delayed from first"]
    );
}

#[test]
fn a_cancelled_task_never_resumes_and_the_others_keep_their_order() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    run(
        &mut runtime,
        &mut world,
        r#"
        task.delay(0.1, print, "a")
        local b = task.delay(0.1, print, "b")
        task.delay(0.1, print, "c")
        task.cancel(b)
        local waiter = task.spawn(function()
            task.wait(0.1)
            print("waiter")
        end)
        task.cancel(waiter)
        "#,
    );
    runtime.step(&mut world, 0.1);
    assert_eq!(lines(&mut runtime), ["a", "c"]);
}

#[test]
fn a_task_cancelled_by_one_due_in_the_same_step_does_not_run() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    run(
        &mut runtime,
        &mut world,
        r#"
        local later
        task.delay(0.1, function()
            print("first")
            task.cancel(later)
        end)
        later = task.delay(0.1, print, "later")
        "#,
    );
    runtime.step(&mut world, 0.1);
    assert_eq!(lines(&mut runtime), ["first"]);
}

#[test]
fn cancelling_a_thread_scheduled_twice_drops_both() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    run(
        &mut runtime,
        &mut world,
        r#"
        local waiter = task.spawn(function()
            task.wait(0.1)
            print("resumed")
        end)
        task.defer(waiter)
        task.cancel(waiter)
        "#,
    );
    step_frames(&mut runtime, &mut world, 0.1, 3);
    assert_eq!(lines(&mut runtime), Vec::<String>::new());
}

fn step_frames(runtime: &mut ScriptRuntime, world: &mut World, seconds: f64, count: usize) {
    for _ in 0..count {
        runtime.step(world, seconds);
    }
}

/// A thread for the scheduler to hold.
fn thread(lua: &Lua) -> mlua::Thread {
    let function = lua.create_function(|_, ()| Ok(())).unwrap();
    lua.create_thread(function).unwrap()
}

#[test]
fn has_waiting_stops_counting_a_cancelled_task_at_once() {
    let lua = Lua::new();
    let mut scheduler = Scheduler::default();
    let (a, b) = (thread(&lua), thread(&lua));
    scheduler.delay(a.clone(), 3600.0, MultiValue::new());
    scheduler.delay(b.clone(), 3600.0, MultiValue::new());
    scheduler.cancel(&a);
    assert!(scheduler.has_waiting());
    scheduler.cancel(&b);
    assert!(!scheduler.has_waiting());
}

#[test]
fn cancelling_never_lets_the_queue_grow_past_the_tasks_waiting() {
    let lua = Lua::new();
    let mut scheduler = Scheduler::default();
    for _ in 0..10_000 {
        let thread = thread(&lua);
        scheduler.delay(thread.clone(), 3600.0, MultiValue::new());
        scheduler.cancel(&thread);
    }
    assert!(!scheduler.has_waiting());
    assert!(scheduler.queue_len() <= 64);
}

/// Pins the queue against the plain definition of the order, for a mix of
/// delays with many ties, some cancelled.
#[test]
fn the_queue_resumes_in_the_order_a_full_sort_gives() {
    let lua = Lua::new();
    let mut scheduler = Scheduler::default();
    let mut random = 17u32;
    let mut next = move |below: u32| {
        random = random.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (random >> 8) % below
    };
    let mut tasks = Vec::new();
    for number in 0..5_000u64 {
        // Tenths of a second, so many land on exactly the same time.
        let seconds = f64::from(next(80)) / 10.0;
        let thread = thread(&lua);
        scheduler.delay(thread.clone(), seconds, MultiValue::new());
        tasks.push((seconds, number, thread));
    }
    let mut expected: Vec<(f64, u64)> = Vec::new();
    for (seconds, number, thread) in &tasks {
        if next(4) == 0 {
            scheduler.cancel(thread);
        } else {
            expected.push((*seconds, *number));
        }
    }
    expected.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));

    // Four long frames; each must return what's newly due, in full order.
    let mut resumed: Vec<mlua::Thread> = Vec::new();
    for _ in 0..4 {
        resumed.extend(scheduler.take_due_threads(2.0));
    }
    let expected: Vec<&mlua::Thread> = expected
        .iter()
        .map(|&(_, number)| &tasks[number as usize].2)
        .collect();
    assert_eq!(resumed.len(), expected.len());
    assert!(resumed.iter().zip(expected).all(|(a, b)| a == b));
    assert!(!scheduler.has_waiting());
}

#[test]
fn destroying_an_instance_disconnects_its_handlers_and_its_descendants() {
    let mut world = World::new();
    let parent = part(&mut world, "Holder");
    let child = part(&mut world, "Child");
    world.set_parent(child, Some(parent)).unwrap();
    let mut runtime = ScriptRuntime::new();
    run(
        &mut runtime,
        &mut world,
        r#"
        local a = workspace.Holder.Touched:Connect(function() end)
        local b = workspace.Holder.Child.Touched:Connect(function() end)
        local c = workspace.Holder.Child.Changed:Connect(function() end)
        task.delay(0.1, function()
            print(a.Connected, b.Connected, c.Connected)
        end)
        "#,
    );
    world.destroy(parent).unwrap();
    runtime.step(&mut world, 0.1);
    assert_eq!(lines(&mut runtime), ["false\tfalse\tfalse"]);
}

#[test]
fn handlers_on_something_else_stay_connected_when_an_instance_is_destroyed() {
    let mut world = World::new();
    let doomed = part(&mut world, "Doomed");
    part(&mut world, "Other");
    let mut runtime = ScriptRuntime::new();
    run(
        &mut runtime,
        &mut world,
        r#"
        local on_doomed = workspace.Doomed.Touched:Connect(function() end)
        local on_other = workspace.Other.Touched:Connect(function() end)
        task.delay(0.1, function() print(on_doomed.Connected, on_other.Connected) end)
        "#,
    );
    world.destroy(doomed).unwrap();
    runtime.step(&mut world, 0.1);
    assert_eq!(lines(&mut runtime), ["false\ttrue"]);
}

#[test]
fn a_handler_connected_to_a_destroyed_instance_is_dropped_at_the_next_step() {
    let mut world = World::new();
    let gone = part(&mut world, "Gone");
    let mut runtime = ScriptRuntime::new();
    run(
        &mut runtime,
        &mut world,
        r#"
        local signal = workspace.Gone.Touched
        workspace.Gone:Destroy()
        local connection = signal:Connect(function() end)
        print(connection.Connected)
        task.delay(0.1, function() print(connection.Connected) end)
        "#,
    );
    assert!(!world.contains(gone));
    runtime.step(&mut world, 0.1);
    assert_eq!(lines(&mut runtime), ["true", "false"]);
}

#[test]
fn a_destroyed_bindable_function_loses_its_callback() {
    let mut world = World::new();
    let function = world.create(ClassName::BindableFunction).unwrap();
    world.set_name(function, "Call").unwrap();
    world.set_parent(function, Some(world.workspace())).unwrap();
    let mut runtime = ScriptRuntime::new();
    run(
        &mut runtime,
        &mut world,
        "workspace.Call.OnInvoke = function() return 1 end",
    );
    assert_eq!(lines(&mut runtime), Vec::<String>::new());
    world.destroy(function).unwrap();
    runtime.step(&mut world, 0.1);
    let callbacks = runtime.server.lua.app_data_ref::<bindable::Callbacks>();
    assert!(callbacks.unwrap().is_empty());
}

#[test]
fn handlers_on_a_streamed_out_instance_survive_until_it_comes_back() {
    let mut world = World::new();
    let away = part(&mut world, "Away");
    let mut runtime = ScriptRuntime::new();
    let script = world.create(ClassName::LocalScript).unwrap();
    world
        .set_parent(script, Some(world.starter_player_scripts()))
        .unwrap();
    runtime.start_script(
        &mut world,
        script,
        "scripts/test.client.lua",
        r#"
        local away = workspace.Away
        connection = away.StreamedIn:Connect(function() print("came back") end)
        task.delay(0.2, function() print(connection.Connected) end)
        "#,
    );
    // As when streaming takes it off this device.
    world.destroy(away).unwrap();
    runtime.note_streaming(&mut world, &[away], &[]);
    step_frames(&mut runtime, &mut world, 0.1, 2);
    assert_eq!(lines(&mut runtime), ["true"]);
}

fn count_connected(lua: &Lua) -> usize {
    lua.app_data_ref::<events::Connections>()
        .map_or(0, |connections| connections.len())
}

#[test]
fn a_tween_nothing_holds_loses_its_handlers() {
    let mut world = World::new();
    part(&mut world, "Moved");
    let mut runtime = ScriptRuntime::new();
    run(
        &mut runtime,
        &mut world,
        r#"
        do
            local info = TweenInfo.new(1)
            local tween = game:GetService("TweenService"):Create(
                workspace.Moved, info, { Transparency = 1 })
            tween.Completed:Connect(function() end)
        end
        "#,
    );
    assert_eq!(count_connected(&runtime.server.lua), 1);
    runtime.server.lua.gc_collect().unwrap();
    runtime.server.lua.gc_collect().unwrap();
    runtime.step(&mut world, 0.1);
    assert_eq!(count_connected(&runtime.server.lua), 0);
}

/// One `scheduler::step` with 10,000 coroutines waiting, none due, then with
/// about a tenth of them due every step. Not a pass/fail test:
/// `cargo test -p axcenity-script --release --lib -- --ignored bench_scheduler --nocapture`
#[test]
#[ignore]
fn bench_scheduler() {
    let frame = 1.0 / 60.0;

    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    run(
        &mut runtime,
        &mut world,
        "for i = 1, 10000 do task.spawn(function() task.wait(100000 + i) end) end",
    );
    let lua = &runtime.server.lua;
    let steps = 20_000;
    let start = Instant::now();
    for _ in 0..steps {
        scheduler::step(lua, frame);
    }
    let elapsed = start.elapsed();
    println!(
        "scheduler idle, 10k waiting: {:.0} ns per step ({steps} steps in {elapsed:?})",
        elapsed.as_nanos() as f64 / f64::from(steps)
    );

    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    run(
        &mut runtime,
        &mut world,
        "for i = 1, 10000 do
            task.spawn(function()
                while true do task.wait(0.1 + (i % 100) * 0.001) end
            end)
        end",
    );
    let lua = &runtime.server.lua;
    let steps = 2_000;
    let start = Instant::now();
    for _ in 0..steps {
        scheduler::step(lua, frame);
    }
    let elapsed = start.elapsed();
    println!(
        "scheduler churn, 10k looping on task.wait: {:.0} ns per step ({steps} steps in {elapsed:?})",
        elapsed.as_nanos() as f64 / f64::from(steps)
    );
}

/// `ScriptRuntime::step` with 10,000 handlers on 10,000 Parts and nothing
/// destroyed, which used to look at every handler each frame:
/// `cargo test -p axcenity-script --release --lib -- --ignored bench_connections --nocapture`
#[test]
#[ignore]
fn bench_connections() {
    let mut world = World::new();
    for _ in 0..10_000 {
        part(&mut world, "P");
    }
    let mut runtime = ScriptRuntime::new();
    run(
        &mut runtime,
        &mut world,
        "for _, part in workspace:GetChildren() do
            part.Touched:Connect(function() end)
        end",
    );
    let steps = 2_000;
    let start = Instant::now();
    for _ in 0..steps {
        runtime.step(&mut world, 1.0 / 60.0);
    }
    let elapsed = start.elapsed();
    println!(
        "ScriptRuntime::step with 10k handlers, none destroyed: {:.0} ns per step ({steps} steps in {elapsed:?})",
        elapsed.as_nanos() as f64 / f64::from(steps)
    );
}
