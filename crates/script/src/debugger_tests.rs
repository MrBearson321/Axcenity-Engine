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
            OutputKind::Warning => format!("warning: {}", line.text),
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
    assert_eq!(local(&pause, "half"), "45");
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

/// The scripts' own coroutines (scripting priority 2): `wrap`ped and
/// `coroutine.resume`d ones pause at their breakpoints, listing the frames
/// of the coroutines that resumed them.
const OWN_COROUTINE: &str = "\
local step = coroutine.wrap(function()
  for i = 1, 2 do
    print('in', i)
    coroutine.yield()
  end
end)
print('a')
step()
print('b')
step()
print('c')";

#[test]
fn a_breakpoint_in_a_scripts_own_coroutine_pauses_there() {
    let mut world = World::new();
    let mut runtime = debugging(&[3]);
    start(
        &mut runtime,
        &mut world,
        "scripts/test.server.lua",
        OWN_COROUTINE,
    );
    assert_eq!(lines(&mut runtime), ["a"]);
    let pause = runtime.pause().expect("paused inside the coroutine");
    assert_eq!(pause.line, 3);
    assert_eq!(local(&pause, "i"), "1");
    assert_eq!(pause.frames[0].coroutine, 0);
    // The script's main code resumed it, on line 8.
    let resumer = pause
        .frames
        .iter()
        .find(|frame| frame.coroutine == 1)
        .expect("the resuming coroutine's frames are listed");
    assert_eq!((resumer.function.as_deref(), resumer.line), (None, 8));
    assert!(resumer.locals.iter().any(|local| local.name == "step"));

    runtime.continue_from_pause(&mut world);
    assert_eq!(lines(&mut runtime), ["in\t1", "b"]);
    let pause = runtime.pause().expect("paused on the second pass");
    assert_eq!((pause.line, local(&pause, "i")), (3, "2"));
    runtime.continue_from_pause(&mut world);
    assert_eq!(lines(&mut runtime), ["in\t2", "c"]);
    assert_eq!(runtime.pause(), None);
}

#[test]
fn a_pause_inside_nested_coroutines_holds_them_all_and_carries_on_in_order() {
    let mut world = World::new();
    let mut runtime = debugging(&[3]);
    start(
        &mut runtime,
        &mut world,
        "scripts/test.server.lua",
        "local inner = coroutine.create(function(x)\n  local doubled = x * 2\n  print('inner', doubled)\n  return doubled\nend)\nlocal outer = coroutine.create(function()\n  local ok, got = coroutine.resume(inner, 21)\n  print('outer', ok, got)\nend)\ncoroutine.resume(outer)\nprint('main')",
    );
    let pause = runtime.pause().expect("paused in the inner coroutine");
    assert_eq!(pause.line, 3);
    assert_eq!(local(&pause, "doubled"), "42");
    let lines_by_coroutine: Vec<(usize, usize)> = pause
        .frames
        .iter()
        .map(|frame| (frame.coroutine, frame.line))
        .collect();
    assert_eq!(lines_by_coroutine, [(0, 3), (1, 7), (2, 10)]);
    // While paused, nothing after the resumes has run.
    assert!(lines(&mut runtime).is_empty());
    // A watch reads the frame of a resuming coroutine too.
    assert_eq!(runtime.evaluate(&mut world, 0, "x"), Ok("21".to_owned()));
    runtime.continue_from_pause(&mut world);
    assert_eq!(
        lines(&mut runtime),
        ["inner\t42", "outer\ttrue\t42", "main"]
    );
    assert_eq!(runtime.pause(), None);
}

#[test]
fn a_pause_lists_the_scripts_other_coroutines_and_where_they_wait() {
    let mut world = World::new();
    let mut runtime = debugging(&[6]);
    start(
        &mut runtime,
        &mut world,
        "scripts/test.server.lua",
        "local worker = coroutine.create(function()\n  local job = 'sweep'\n  coroutine.yield()\nend)\ncoroutine.resume(worker)\nprint('main')",
    );
    let pause = runtime.pause().expect("paused in the main code");
    assert_eq!(pause.line, 6);
    assert_eq!(pause.coroutines.len(), 1, "{:?}", pause.coroutines);
    let worker = &pause.coroutines[0];
    assert_eq!(worker.status, "suspended");
    assert_eq!(worker.frames[0].line, 3);
    assert!(
        worker.frames[0]
            .locals
            .iter()
            .any(|local| local.name == "job" && local.value == "\"sweep\""),
        "{worker:?}"
    );
    runtime.continue_from_pause(&mut world);
    assert_eq!(lines(&mut runtime), ["main"]);
}

#[test]
fn a_coroutine_resumed_where_lua_cant_yield_is_passed_and_reported_once() {
    let mut world = World::new();
    let mut runtime = debugging(&[2]);
    start(
        &mut runtime,
        &mut world,
        "scripts/test.server.lua",
        "local co = coroutine.create(function()\n  print('inside')\nend)\nlocal t = { 2, 1 }\ntable.sort(t, function(a, b)\n  if coroutine.status(co) == 'suspended' then coroutine.resume(co) end\n  return a < b\nend)\nprint('done')",
    );
    assert_eq!(runtime.pause(), None);
    let output = lines(&mut runtime);
    assert_eq!(output.len(), 3, "{output:?}");
    assert!(
        output[0]
            .starts_with("error: scripts/test.server.lua:2: Play can't pause at this breakpoint"),
        "{output:?}"
    );
    assert_eq!(output[1..], ["inside", "done"]);
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

fn rule(line: usize, rule: BreakpointRule) -> BreakpointRules {
    BreakpointRules::from([(("scripts/test.server.lua".to_owned(), line), rule)])
}

#[test]
fn a_conditional_breakpoint_pauses_only_where_its_condition_is_true() {
    let mut world = World::new();
    let mut runtime = debugging(&[2]);
    runtime.set_breakpoint_rules(&rule(
        2,
        BreakpointRule {
            condition: Some("i % 3 == 0".to_owned()),
            ..Default::default()
        },
    ));
    start(
        &mut runtime,
        &mut world,
        "scripts/test.server.lua",
        "for i = 1, 7 do\n  print(i)\nend",
    );
    assert_eq!(local(&runtime.pause().unwrap(), "i"), "3");
    assert_eq!(lines(&mut runtime), ["1", "2"]);
    runtime.continue_from_pause(&mut world);
    assert_eq!(local(&runtime.pause().unwrap(), "i"), "6");
    runtime.continue_from_pause(&mut world);
    assert_eq!(runtime.pause(), None);
    assert_eq!(lines(&mut runtime), ["3", "4", "5", "6", "7"]);
}

#[test]
fn a_condition_that_fails_pauses_and_says_why() {
    let mut world = World::new();
    let mut runtime = debugging(&[2]);
    runtime.set_breakpoint_rules(&rule(
        2,
        BreakpointRule {
            condition: Some("missing.field".to_owned()),
            ..Default::default()
        },
    ));
    start(
        &mut runtime,
        &mut world,
        "scripts/test.server.lua",
        "local x = 1\nprint(x)",
    );
    assert!(runtime.pause().is_some());
    let output = lines(&mut runtime);
    assert!(
        output[0]
            .starts_with("error: scripts/test.server.lua:2: the breakpoint's condition failed"),
        "{output:?}"
    );
}

#[test]
fn a_hit_count_lets_the_first_passes_run_on() {
    let mut world = World::new();
    let mut runtime = debugging(&[2]);
    runtime.set_breakpoint_rules(&rule(
        2,
        BreakpointRule {
            hit_count: Some(4),
            ..Default::default()
        },
    ));
    start(
        &mut runtime,
        &mut world,
        "scripts/test.server.lua",
        "for i = 1, 5 do\n  print(i)\nend",
    );
    assert_eq!(local(&runtime.pause().unwrap(), "i"), "4");
    runtime.continue_from_pause(&mut world);
    assert_eq!(
        local(&runtime.pause().unwrap(), "i"),
        "5",
        "from the 4th on"
    );
    runtime.continue_from_pause(&mut world);
    assert_eq!(lines(&mut runtime), ["1", "2", "3", "4", "5"]);
}

#[test]
fn a_logpoint_writes_to_output_and_never_pauses() {
    let mut world = World::new();
    let mut runtime = debugging(&[3]);
    runtime.set_breakpoint_rules(&rule(
        3,
        BreakpointRule {
            log: Some("i is {i}, name {name}, {{braces}}, {nope.x}".to_owned()),
            ..Default::default()
        },
    ));
    start(
        &mut runtime,
        &mut world,
        "scripts/test.server.lua",
        "local name = 'door'\nfor i = 1, 2 do\n  print(i)\nend",
    );
    assert_eq!(runtime.pause(), None);
    let output = lines(&mut runtime);
    assert_eq!(output.len(), 4, "{output:?}");
    assert!(
        output[0].starts_with("scripts/test.server.lua:3: i is 1, name door, {braces}, {error: "),
        "{output:?}"
    );
    assert_eq!(output[1], "1");
    assert!(output[2].starts_with("scripts/test.server.lua:3: i is 2"));
    assert_eq!(output[3], "2");
}

#[test]
fn a_logpoint_in_a_long_loop_doesnt_nest() {
    let mut world = World::new();
    let mut runtime = debugging(&[2]);
    runtime.set_breakpoint_rules(&rule(
        2,
        BreakpointRule {
            log: Some("{i}".to_owned()),
            ..Default::default()
        },
    ));
    start(
        &mut runtime,
        &mut world,
        "scripts/test.server.lua",
        "local total = 0\nfor i = 1, 5000 do total += i end\nprint(total)",
    );
    let output = lines(&mut runtime);
    assert_eq!(output.last().map(String::as_str), Some("12502500"));
}

#[test]
fn a_step_onto_a_ruled_breakpoint_still_pauses() {
    let mut world = World::new();
    let mut runtime = debugging(&[1, 2]);
    runtime.set_breakpoint_rules(&rule(
        2,
        BreakpointRule {
            condition: Some("false".to_owned()),
            ..Default::default()
        },
    ));
    start(
        &mut runtime,
        &mut world,
        "scripts/test.server.lua",
        "local a = 1\nlocal b = 2\nprint(a + b)",
    );
    assert_eq!(runtime.pause().unwrap().line, 1);
    runtime.resume_from_pause(&mut world, Resume::StepOver);
    assert_eq!(runtime.pause().unwrap().line, 2, "the step goes there");
    runtime.continue_from_pause(&mut world);
    assert_eq!(lines(&mut runtime), ["3"]);
}

#[test]
fn pause_stops_at_the_next_line_a_script_runs() {
    let mut world = World::new();
    let mut runtime = debugging(&[]);
    start(
        &mut runtime,
        &mut world,
        "scripts/test.server.lua",
        "local n = 0\nwhile true do\n  n += 1\n  task.wait()\nend",
    );
    assert_eq!(runtime.pause(), None);
    runtime.request_pause(true);
    assert!(runtime.pause_requested());
    runtime.step(&mut world, 1.0 / 60.0);
    let pause = runtime.pause().expect("paused by the request");
    assert!(!pause.breakpoint);
    assert!(!runtime.pause_requested());
    assert_eq!(local(&pause, "n"), "1");
    runtime.continue_from_pause(&mut world);
    assert_eq!(runtime.pause(), None);
    runtime.step(&mut world, 1.0 / 60.0);
    assert_eq!(runtime.pause(), None, "asked once");
}

#[test]
fn tables_in_locals_can_be_expanded() {
    let mut world = World::new();
    let mut runtime = debugging(&[2]);
    start(
        &mut runtime,
        &mut world,
        "scripts/test.server.lua",
        "local t = {10, 20, door = {open = true}, [true] = 'yes'}\nprint(t)",
    );
    let pause = runtime.pause().unwrap();
    let t = pause.frames[0]
        .locals
        .iter()
        .find(|local| local.name == "t")
        .unwrap();
    let field = |name: &str| t.fields.iter().find(|field| field.name == name).unwrap();
    assert_eq!(field("[1]").value, "10");
    assert_eq!(field("[2]").value, "20");
    assert_eq!(field("[true]").value, "\"yes\"");
    let door = field("door");
    assert_eq!(door.fields.len(), 1);
    assert_eq!(door.fields[0].name, "open");
    assert_eq!(door.fields[0].value, "true");
}

#[test]
fn the_watchdog_still_stops_an_endless_loop_with_a_logpoint_in_it() {
    let mut world = World::new();
    let mut runtime = debugging(&[3]);
    runtime.set_watchdog_limit(Duration::from_millis(100));
    runtime.set_breakpoint_rules(&rule(
        3,
        BreakpointRule {
            condition: Some("n % 100000 == 0".to_owned()),
            log: Some("n = {n}".to_owned()),
            ..Default::default()
        },
    ));
    start(
        &mut runtime,
        &mut world,
        "scripts/test.server.lua",
        "local n = 0\nwhile true do\n  n += 1\nend",
    );
    let output = lines(&mut runtime);
    assert!(
        output
            .last()
            .is_some_and(|line| line.contains("without pausing, so it was stopped")),
        "{output:?}"
    );
}
