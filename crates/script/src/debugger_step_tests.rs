//! Stepping, pausing on an error and watch expressions (roadmap G6).

use axcenity_core::{ClassName, World};

use super::*;

const PATH: &str = "scripts/test.server.lua";

/// A runtime as the editor makes it for Play, with breakpoints at `lines`
/// of [`PATH`], running `source` there.
fn paused_in(lines: &[usize], source: &str) -> (ScriptRuntime, World) {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    runtime.enable_debugger();
    runtime.set_breakpoints(&Breakpoints::from([(
        PATH.to_owned(),
        lines.iter().copied().collect(),
    )]));
    start(&mut runtime, &mut world, PATH, source);
    (runtime, world)
}

fn start(runtime: &mut ScriptRuntime, world: &mut World, path: &str, source: &str) {
    let script = world.create(ClassName::Script).unwrap();
    world
        .set_parent(script, Some(world.server_script_service()))
        .unwrap();
    runtime.start_script(world, script, path, source);
}

fn printed(runtime: &mut ScriptRuntime) -> Vec<String> {
    runtime
        .take_output()
        .into_iter()
        .map(|line| match line.kind {
            OutputKind::Print => line.text,
            _ => format!("error: {}", line.text),
        })
        .collect()
}

/// Where Play is paused: the line and the innermost function's name.
fn at(runtime: &ScriptRuntime) -> Option<(usize, Option<String>)> {
    runtime
        .pause()
        .map(|pause| (pause.line, pause.frames[0].function.clone()))
}

fn local(runtime: &ScriptRuntime, name: &str) -> String {
    let pause = runtime.pause().expect("paused");
    pause.frames[0]
        .locals
        .iter()
        .find(|local| local.name == name)
        .map(|local| local.value.clone())
        .unwrap_or_else(|| panic!("no local `{name}` in {pause:?}"))
}

fn step(runtime: &mut ScriptRuntime, world: &mut World, how: Resume) {
    runtime.resume_from_pause(world, how);
}

const ADD: &str = "local function add(a, b)\n  local sum = a + b\n  return sum\nend\nlocal total = add(1, 2)\nprint('total', total)\nprint('end')";

#[test]
fn step_over_runs_a_line_and_pauses_at_the_next() {
    let (mut runtime, mut world) = paused_in(&[5], ADD);
    assert_eq!(at(&runtime), Some((5, None)));
    step(&mut runtime, &mut world, Resume::StepOver);
    assert_eq!(at(&runtime), Some((6, None)), "over add, not into it");
    assert_eq!(local(&runtime, "total"), "3");
    assert!(printed(&mut runtime).is_empty());

    step(&mut runtime, &mut world, Resume::StepOver);
    assert_eq!(at(&runtime), Some((7, None)));
    assert_eq!(printed(&mut runtime), ["total\t3"]);
    // Off the end of the script: the game runs on.
    step(&mut runtime, &mut world, Resume::StepOver);
    assert_eq!(at(&runtime), None);
    assert_eq!(printed(&mut runtime), ["end"]);
}

#[test]
fn step_into_enters_a_call_and_step_out_returns_to_the_caller() {
    let (mut runtime, mut world) = paused_in(&[5], ADD);
    step(&mut runtime, &mut world, Resume::StepInto);
    assert_eq!(at(&runtime), Some((2, Some("add".to_owned()))));
    assert_eq!(local(&runtime, "a"), "1");
    step(&mut runtime, &mut world, Resume::StepOver);
    assert_eq!(at(&runtime), Some((3, Some("add".to_owned()))));
    assert_eq!(local(&runtime, "sum"), "3");

    // Back in the caller. Nothing of line 5 is left after the call, so
    // it pauses at line 6.
    step(&mut runtime, &mut world, Resume::StepOut);
    assert_eq!(at(&runtime), Some((6, None)));
    assert_eq!(local(&runtime, "total"), "3");
}

#[test]
fn a_step_across_task_wait_pauses_where_the_script_carries_on() {
    let (mut runtime, mut world) = paused_in(&[2], "print('a')\ntask.wait(1)\nprint('b')");
    assert_eq!(printed(&mut runtime), ["a"]);
    step(&mut runtime, &mut world, Resume::StepOver);
    assert_eq!(at(&runtime), None, "waiting, not paused");
    runtime.step(&mut world, 0.5);
    assert_eq!(at(&runtime), None);
    runtime.step(&mut world, 0.6);
    assert_eq!(at(&runtime), Some((3, None)));
    assert!(printed(&mut runtime).is_empty());
    runtime.continue_from_pause(&mut world);
    assert_eq!(printed(&mut runtime), ["b"]);
}

#[test]
fn another_coroutine_runs_through_a_step_without_pausing() {
    let (mut runtime, mut world) = paused_in(
        &[1],
        "task.spawn(function()\n  print('spawned')\nend)\nprint('main')",
    );
    step(&mut runtime, &mut world, Resume::StepOver);
    assert_eq!(at(&runtime), Some((4, None)));
    assert_eq!(printed(&mut runtime), ["spawned"]);
}

#[test]
fn a_breakpoint_reached_first_ends_the_step() {
    let (mut runtime, mut world) = paused_in(
        &[2, 4],
        "local function f()\n  print('in f')\nend\nf()\nprint('after')",
    );
    assert_eq!(at(&runtime), Some((4, None)));
    step(&mut runtime, &mut world, Resume::StepOver);
    assert_eq!(
        at(&runtime),
        Some((2, Some("f".to_owned()))),
        "the breakpoint in f"
    );
    runtime.continue_from_pause(&mut world);
    assert_eq!(at(&runtime), None, "the step is over");
    assert_eq!(printed(&mut runtime), ["in f", "after"]);
}

#[test]
fn stepping_into_and_over_a_pcall() {
    let source = "local ok = pcall(function()\n  error('nope')\nend)\nprint(ok)";
    let (mut runtime, mut world) = paused_in(&[1], source);
    step(&mut runtime, &mut world, Resume::StepOver);
    assert_eq!(at(&runtime), Some((4, None)));
    assert_eq!(local(&runtime, "ok"), "false");

    let (mut runtime, mut world) = paused_in(&[1], source);
    step(&mut runtime, &mut world, Resume::StepInto);
    assert_eq!(
        at(&runtime).map(|(line, _)| line),
        Some(2),
        "inside the pcall"
    );
    step(&mut runtime, &mut world, Resume::StepOver);
    assert_eq!(at(&runtime), Some((4, None)), "the error is caught");
    runtime.continue_from_pause(&mut world);
    assert_eq!(printed(&mut runtime), ["false"]);
}

#[test]
fn step_into_a_module_function_and_out_again() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    runtime.enable_debugger();
    runtime.set_breakpoints(&Breakpoints::from([(PATH.to_owned(), [2].into())]));
    let module = world.create(ClassName::ModuleScript).unwrap();
    world.set_name(module, "Maths").unwrap();
    world
        .set_parent(module, Some(world.replicated_storage()))
        .unwrap();
    runtime.start_script(
        &mut world,
        module,
        "scripts/Maths.module.lua",
        "local M = {}\nfunction M.double(n)\n  return n * 2\nend\nreturn M",
    );
    start(
        &mut runtime,
        &mut world,
        PATH,
        "local Maths = require(game.ReplicatedStorage.Maths)\nlocal x = Maths.double(4)\nprint(x)",
    );
    assert_eq!(at(&runtime).map(|(line, _)| line), Some(2));
    step(&mut runtime, &mut world, Resume::StepInto);
    let pause = runtime.pause().unwrap();
    assert_eq!(
        (pause.path.as_str(), pause.line),
        ("scripts/Maths.module.lua", 3)
    );
    assert_eq!(local(&runtime, "n"), "4");
    step(&mut runtime, &mut world, Resume::StepOut);
    assert_eq!(runtime.pause().unwrap().path, PATH);
    assert_eq!(at(&runtime), Some((3, None)));
    assert_eq!(local(&runtime, "x"), "8");
}

const BOOM: &str = "local count = 2\nlocal function boom(n)\n  local half = n / 2\n  error('boom ' .. half)\nend\nboom(count)\nprint('never')";

#[test]
fn an_error_pauses_at_the_failed_line_when_asked() {
    let (mut runtime, mut world) = {
        let mut world = World::new();
        let mut runtime = ScriptRuntime::new();
        runtime.enable_debugger();
        runtime.set_pause_on_error(true);
        start(&mut runtime, &mut world, PATH, BOOM);
        (runtime, world)
    };
    let pause = runtime.pause().expect("paused on the error");
    assert_eq!(
        (pause.line, pause.frames[0].function.as_deref()),
        (4, Some("boom"))
    );
    assert!(
        pause
            .error
            .as_deref()
            .is_some_and(|error| error.contains("boom 1")),
        "{pause:?}"
    );
    assert_eq!(local(&runtime, "half"), "1");
    assert_eq!(pause.frames[1].line, 6, "where boom was called");
    let output = printed(&mut runtime);
    assert!(
        output[0].starts_with("error: "),
        "the error is still shown: {output:?}"
    );

    // The failed script can't step on: a step is a Continue.
    step(&mut runtime, &mut world, Resume::StepOver);
    assert_eq!(runtime.pause(), None);
    assert!(printed(&mut runtime).is_empty(), "never");
}

#[test]
fn errors_dont_pause_unless_asked_nor_when_caught() {
    let (runtime, _) = paused_in(&[], BOOM);
    assert_eq!(runtime.pause(), None);

    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    runtime.enable_debugger();
    runtime.set_pause_on_error(true);
    start(
        &mut runtime,
        &mut world,
        PATH,
        "print(pcall(error, 'caught'))",
    );
    assert_eq!(runtime.pause(), None);
    assert_eq!(printed(&mut runtime), ["false\tcaught"]);
}

#[test]
fn watches_see_the_paused_functions_locals_upvalues_and_globals() {
    let (mut runtime, mut world) = paused_in(
        &[3],
        "local base = 10\nlocal function scale(n)\n  return n * base\nend\nprint(scale(4))",
    );
    let pause = runtime.pause().unwrap();
    assert!(
        pause.frames[0]
            .upvalues
            .iter()
            .any(|upvalue| upvalue.name == "base" && upvalue.value == "10"),
        "{pause:?}"
    );
    let mut watch = |frame, expression: &str| runtime.evaluate(&mut world, frame, expression);
    assert_eq!(watch(0, "n * 2"), Ok("8".to_owned()));
    assert_eq!(watch(0, "base + n"), Ok("14".to_owned()), "an upvalue");
    assert_eq!(
        watch(1, "base"),
        Ok("10".to_owned()),
        "the script's own frame"
    );
    assert_eq!(watch(0, "workspace.Name"), Ok("\"Workspace\"".to_owned()));
    assert_eq!(watch(0, "{ n, 'x' }"), Ok("{4, \"x\"}".to_owned()));
    assert!(watch(0, "n +").is_err(), "not an expression");
    assert!(watch(0, "error('no')").unwrap_err().contains("no"));
    assert!(watch(5, "n").is_err(), "no such frame");

    runtime.continue_from_pause(&mut world);
    assert_eq!(printed(&mut runtime), ["40"]);
    assert_eq!(
        runtime.evaluate(&mut world, 0, "1"),
        Err("Play isn't paused.".to_owned())
    );
}

#[test]
fn step_out_pauses_on_the_callers_line_while_it_has_more_to_run() {
    let (mut runtime, mut world) = paused_in(
        &[2],
        "local function two()\n  return 2\nend\nlocal four = two() + two()\nprint(four)",
    );
    step(&mut runtime, &mut world, Resume::StepOut);
    assert_eq!(
        at(&runtime),
        Some((4, None)),
        "the second call is still to come"
    );
    // Into the second call, which has no breakpoint of its own.
    step(&mut runtime, &mut world, Resume::StepInto);
    assert_eq!(at(&runtime), Some((2, Some("two".to_owned()))));
    runtime.set_breakpoints(&Breakpoints::new());
    runtime.continue_from_pause(&mut world);
    assert_eq!(printed(&mut runtime), ["4"]);
}
