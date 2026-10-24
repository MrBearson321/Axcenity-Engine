//! Breakpoints: Play pauses before the line runs, shows the script's
//! locals, holds everything else, and continues in order.

use std::time::Duration;

use axcenity_core::{ClassName, World};

use super::*;

/// A runtime as the editor makes it for Play, with breakpoints at `lines`
/// of `scripts/test.server.lua`.
fn debugging(lines: &[usize]) -> ScriptRuntime {
    let mut runtime = ScriptRuntime::new();
    runtime.enable_debugger();
    runtime.set_breakpoints(&breakpoints("scripts/test.server.lua", lines));
    runtime
}

fn breakpoints(path: &str, lines: &[usize]) -> Breakpoints {
    Breakpoints::from([(path.to_owned(), lines.iter().copied().collect())])
}

fn start(runtime: &mut ScriptRuntime, world: &mut World, path: &str, source: &str) {
    let script = world.create(ClassName::Script).unwrap();
    world
        .set_parent(script, Some(world.server_script_service()))
        .unwrap();
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

fn local<'a>(pause: &'a Pause, name: &str) -> &'a str {
    pause.frames[0]
        .locals
        .iter()
        .find(|local| local.name == name)
        .map(|local| local.value.as_str())
        .unwrap_or_else(|| panic!("no local `{name}` in {pause:?}"))
}

#[test]
fn a_breakpoint_pauses_before_its_line_and_continue_runs_on() {
    let mut world = World::new();
    let mut runtime = debugging(&[4]);
    start(
        &mut runtime,
        &mut world,
        "scripts/test.server.lua",
        "local count = 3\nlocal name = 'door'\nprint('before')\nprint('at', count)\nprint('after')",
    );
    assert_eq!(lines(&mut runtime), ["before"]);
    let pause = runtime.pause().expect("paused at line 4");
    assert_eq!(
        (pause.path.as_str(), pause.line),
        ("scripts/test.server.lua", 4)
    );
    assert_eq!(pause.frames[0].function, None, "the script's main code");
    assert_eq!(local(&pause, "count"), "3");
    assert_eq!(local(&pause, "name"), "\"door\"");

    runtime.continue_from_pause(&mut world);
    assert_eq!(lines(&mut runtime), ["at\t3", "after"]);
    assert_eq!(runtime.pause(), None);
}

#[test]
fn each_pass_over_a_breakpoint_pauses_again() {
    let mut world = World::new();
    let mut runtime = debugging(&[2]);
    start(
        &mut runtime,
        &mut world,
        "scripts/test.server.lua",
        "for i = 1, 3 do\n  print(i)\n  task.wait()\nend",
    );
    assert_eq!(local(&runtime.pause().unwrap(), "i"), "1");
    runtime.continue_from_pause(&mut world);
    assert_eq!(lines(&mut runtime), ["1"]);
    assert_eq!(runtime.pause(), None, "waiting, not paused");

    runtime.step(&mut world, 1.0 / 60.0);
    assert_eq!(local(&runtime.pause().unwrap(), "i"), "2");
    assert!(lines(&mut runtime).is_empty());
    // With the breakpoint gone, Continue runs to the end of Play's work.
    runtime.set_breakpoints(&Breakpoints::new());
    runtime.continue_from_pause(&mut world);
    runtime.step(&mut world, 1.0 / 60.0);
    assert_eq!(lines(&mut runtime), ["2", "3"]);
}

#[test]
fn locals_of_every_script_function_on_the_stack_are_shown() {
    let mut world = World::new();
    let mut runtime = debugging(&[3]);
    start(
        &mut runtime,
        &mut world,
        "scripts/test.server.lua",
        "local function open(door, angle)\n  local half = angle / 2\n  return half\nend\nlocal \
         hinge = 'Hinge'\nopen(hinge, 90)",
    );
    let pause = runtime.pause().unwrap();
    let names: Vec<_> = pause
        .frames
        .iter()
        .map(|frame| (frame.function.as_deref(), frame.line))
        .collect();
    assert_eq!(names, [(Some("open"), 3), (None, 6)]);
    assert_eq!(local(&pause, "door"), "\"Hinge\"");
    assert_eq!(local(&pause, "angle"), "90");
    assert_eq!(local(&pause, "half"), "45.0");
    assert!(
        pause.frames[1]
            .locals
            .iter()
            .any(|local| local.name == "hinge")
    );
}

#[test]
fn tables_are_listed_without_running_their_metamethods() {
    let mut world = World::new();
    let mut runtime = debugging(&[3]);
    start(
        &mut runtime,
        &mut world,
        "scripts/test.server.lua",
        "local meta = { __tostring = function() error('ran') end, __pairs = function() error('ran') end }\nlocal t = setmetatable({ 1, 2, open = true, inner = {} }, meta)\nprint(workspace)",
    );
    let pause = runtime.pause().unwrap();
    let value = local(&pause, "t");
    assert!(value.starts_with("{1, 2, "), "{value}");
    assert!(value.contains("open = true"), "{value}");
    assert!(value.contains("inner = {…}"), "{value}");
    assert!(lines(&mut runtime).is_empty(), "no metamethod ran");
}

#[test]
fn while_paused_other_scripts_and_events_wait_then_run_in_order() {
    let mut world = World::new();
    let mut runtime = debugging(&[1]);
    start(
        &mut runtime,
        &mut world,
        "scripts/test.server.lua",
        "print('a')",
    );
    start(
        &mut runtime,
        &mut world,
        "scripts/b.server.lua",
        "print('b')",
    );
    runtime.run_command(&mut world, "print('command')").unwrap();
    assert!(lines(&mut runtime).is_empty(), "nothing runs while paused");
    runtime.continue_from_pause(&mut world);
    assert_eq!(lines(&mut runtime), ["a", "b", "command"]);
}

#[test]
fn a_script_that_spawned_the_paused_one_waits_for_it() {
    let mut world = World::new();
    let mut runtime = debugging(&[2]);
    start(
        &mut runtime,
        &mut world,
        "scripts/test.server.lua",
        "task.spawn(function()\n  print('child')\nend)\nprint('parent')",
    );
    assert_eq!(runtime.pause().map(|pause| pause.line), Some(2));
    assert!(lines(&mut runtime).is_empty(), "the parent is held too");
    runtime.continue_from_pause(&mut world);
    assert_eq!(lines(&mut runtime), ["child", "parent"]);
}

#[test]
fn a_breakpoint_in_a_scripts_own_coroutine_is_passed_and_reported_once() {
    let mut world = World::new();
    let mut runtime = debugging(&[2]);
    start(
        &mut runtime,
        &mut world,
        "scripts/test.server.lua",
        "local step = coroutine.wrap(function()\n  for i = 1, 2 do print(i) coroutine.yield() end\nend)\nstep()\nstep()",
    );
    assert_eq!(runtime.pause(), None);
    let output = lines(&mut runtime);
    assert_eq!(output.len(), 3, "{output:?}");
    assert!(
        output[0]
            .starts_with("error: scripts/test.server.lua:2: Play can't pause at this breakpoint"),
        "{output:?}"
    );
    assert_eq!(output[1..], ["1", "2"]);
}

#[test]
fn a_breakpoint_where_lua_cant_yield_is_passed() {
    let mut world = World::new();
    let mut runtime = debugging(&[2]);
    start(
        &mut runtime,
        &mut world,
        "scripts/test.server.lua",
        "local t = { 3, 1, 2 }\ntable.sort(t, function(a, b) return a < b end) print(t[1])\nprint('done')",
    );
    // The line starts in the script's coroutine, so it pauses there first.
    assert_eq!(runtime.pause().map(|pause| pause.line), Some(2));
    runtime.continue_from_pause(&mut world);
    let output = lines(&mut runtime);
    assert_eq!(
        output.last().map(String::as_str),
        Some("done"),
        "{output:?}"
    );
    assert!(output.contains(&"1".to_owned()), "{output:?}");
    assert_eq!(runtime.pause(), None);
}

#[test]
fn without_the_debugger_breakpoints_do_nothing() {
    // The player's runtime: breakpoints set, debugger never turned on.
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    runtime.set_breakpoints(&breakpoints("scripts/test.server.lua", &[1]));
    start(
        &mut runtime,
        &mut world,
        "scripts/test.server.lua",
        "print('ran')",
    );
    assert_eq!(runtime.pause(), None);
    assert_eq!(lines(&mut runtime), ["ran"]);
}

#[test]
fn scripts_still_cant_reach_the_debug_library() {
    let mut world = World::new();
    let mut runtime = debugging(&[]);
    start(
        &mut runtime,
        &mut world,
        "scripts/test.server.lua",
        "print(type(debug), type(getfenv), type(load))",
    );
    assert_eq!(lines(&mut runtime), ["nil\tnil\tnil"]);
}

#[test]
fn the_watchdog_still_stops_endless_loops_with_the_debugger_on() {
    let mut world = World::new();
    let mut runtime = debugging(&[]);
    runtime.set_watchdog_limit(Duration::from_millis(100));
    start(
        &mut runtime,
        &mut world,
        "scripts/test.server.lua",
        "while true do end",
    );
    let output = lines(&mut runtime);
    assert!(
        output[0].contains("without pausing, so it was stopped"),
        "{output:?}"
    );
}
