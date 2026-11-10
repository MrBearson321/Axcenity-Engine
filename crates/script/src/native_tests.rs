//! Native code for `--!native` scripts (roadmap L3): which scripts ask for
//! it, and that a script behaves the same compiled to machine code as in
//! the interpreter.

use std::time::{Duration, Instant};

use axcenity_core::{ClassName, InstanceId, World};

use super::*;
use crate::chunks::wants_native;

fn start(runtime: &mut ScriptRuntime, world: &mut World, path: &str, source: &str) -> InstanceId {
    let script = world.create(ClassName::Script).unwrap();
    world
        .set_parent(script, Some(world.server_script_service()))
        .unwrap();
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
            OutputKind::Warning => format!("warning: {}", line.text),
        })
        .collect()
}

/// What `body` prints over `frames` frames of 0.1 s, run with the header
/// `header` (one line, so line numbers match between runs).
fn run(header: &str, body: &str, frames: usize) -> Vec<String> {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    let source = format!("{header}\n{body}");
    start(
        &mut runtime,
        &mut world,
        "scripts/native.server.lua",
        &source,
    );
    for _ in 0..frames {
        runtime.step(&mut world, 0.1);
    }
    lines(&mut runtime)
}

#[test]
fn a_native_hot_comment_in_the_header_or_a_native_function_asks_for_native_code() {
    assert!(wants_native("--!native\nprint(1)"));
    assert!(wants_native("--!strict\n--!native\nlocal x = 1"));
    assert!(wants_native(
        "\u{feff}  \n-- my script\n--!native   \nprint(1)"
    ));
    assert!(wants_native("--[[ about\nthis script ]]\n--!native\n"));
    assert!(wants_native("--[==[ ]] still a comment ]==]--!native"));
    assert!(wants_native("--!native"));
    assert!(
        wants_native("@native\nlocal function f() end"),
        "a native function"
    );

    assert!(!wants_native("print(1)\n--!native"), "after code");
    assert!(!wants_native("-- --!native\nprint(1)"), "inside a comment");
    assert!(!wants_native("--!nativeish\nprint(1)"));
    assert!(!wants_native("--! native\nprint(1)"));
    assert!(!wants_native("--!native please\nprint(1)"));
    assert!(!wants_native("--!strict\nprint(1)"));
    assert!(!wants_native("--[[ unclosed --!native"));
    assert!(!wants_native(""));
}

/// Numbers, strings, tables, closures, the engine's value types and
/// Instances, `task.wait` and a caught error: native code has to give the
/// interpreter's results for all of them.
const MIXED: &str = r#"
local function fib(n) if n < 2 then return n end return fib(n - 1) + fib(n - 2) end
local total = 0
for i = 1, 1000 do total += math.sqrt(i) * (i % 7) end
local words = {}
for word in string.gmatch("one two three", "%a+") do table.insert(words, word:upper()) end
local counter = 0
local function bump(by) counter += by return counter end
bump(2) bump(3)
local v = Vector3.new(1, 2, 3) + Vector3.new(0.5, 0.5, 0.5) * 2
local part = Instance.new("Part")
part.Name = "Native"
part.Position = v
part.Parent = workspace
print(fib(20), string.format("%.4f", total), table.concat(words, ","), counter)
print(typeof(v), v.X, v.Y, v.Z, part.Position.Z, workspace.Native.Name)
local ok, message = pcall(function() local t = nil return t.field end)
print(ok, message)
for step = 1, 3 do
    task.wait(0.1)
    print("after wait", step)
end
error("stopped on purpose")
"#;

#[test]
fn a_native_script_prints_what_the_interpreter_prints() {
    let interpreted = run("-- not native", MIXED, 5);
    let native = run("--!native", MIXED, 5);
    assert!(
        interpreted
            .last()
            .is_some_and(|line| line.contains("scripts/native.server.lua:24: stopped on purpose")),
        "{interpreted:?}"
    );
    assert!(
        interpreted.contains(&"after wait\t3".to_string()),
        "{interpreted:?}"
    );
    assert_eq!(native, interpreted);
}

#[test]
fn the_watchdog_stops_an_endless_loop_in_native_code() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    runtime.set_watchdog_limit(Duration::from_millis(100));
    start(
        &mut runtime,
        &mut world,
        "scripts/native.server.lua",
        "--!native\nlocal n = 0\nwhile true do n += 1 end",
    );
    let output = lines(&mut runtime);
    assert!(
        output
            .first()
            .is_some_and(|line| line.contains("without pausing, so it was stopped")),
        "{output:?}"
    );
}

#[test]
fn a_native_module_works_from_a_plain_script() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    let module = world.create(ClassName::ModuleScript).unwrap();
    world.set_name(module, "Maths").unwrap();
    world
        .set_parent(module, Some(world.replicated_storage()))
        .unwrap();
    runtime.start_script(
        &mut world,
        module,
        "scripts/Maths.module.lua",
        "--!native\nlocal M = {}\nfunction M.sum(n) local s = 0 for i = 1, n do s += i end return s end\nreturn M",
    );
    start(
        &mut runtime,
        &mut world,
        "scripts/uses.server.lua",
        "print(require(game.ReplicatedStorage.Maths).sum(100))",
    );
    assert_eq!(lines(&mut runtime), vec!["5050"]);
}

/// Numeric work with and without `--!native`. Not a pass/fail test:
/// `cargo test -p axcenity-script --release --lib -- --ignored bench_native --nocapture`
#[test]
#[ignore]
fn bench_native() {
    let cases = [
        (
            "numbers",
            "local s = 0 for i = 1, 3000000 do s += math.sin(i) * math.sqrt(i) end",
        ),
        (
            "fib",
            "local function fib(n) if n < 2 then return n end return fib(n - 1) + fib(n - 2) end fib(27)",
        ),
        (
            "tables",
            "local t = {} for i = 1, 300000 do t[i] = i * 2 end local s = 0 for _, v in ipairs(t) do s += v end",
        ),
        (
            "Vector3",
            "local v = Vector3.new(0, 0, 0) for i = 1, 200000 do v = v + Vector3.new(1, 0, 0) * 0.5 end",
        ),
        (
            "Instance reads",
            "local p = Instance.new('Part') local s = 0 for i = 1, 100000 do s += p.Position.X end",
        ),
    ];
    for (name, body) in cases {
        let mut times = Vec::new();
        for header in ["-- interpreted", "--!native"] {
            let mut best = f64::MAX;
            for _ in 0..5 {
                let mut world = World::new();
                let mut runtime = ScriptRuntime::new();
                let started = Instant::now();
                start(
                    &mut runtime,
                    &mut world,
                    "scripts/bench.server.lua",
                    &format!("{header}\n{body}"),
                );
                best = best.min(started.elapsed().as_secs_f64() * 1e3);
                let output = lines(&mut runtime);
                assert!(output.is_empty(), "{output:?}");
            }
            times.push(best);
        }
        println!(
            "{name:>15}: interpreted {:7.2} ms, native {:7.2} ms ({:.2}x)",
            times[0],
            times[1],
            times[0] / times[1]
        );
    }
}
