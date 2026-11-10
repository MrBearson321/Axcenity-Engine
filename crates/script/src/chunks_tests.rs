//! Compiled-chunk cache (`chunks`): the same source compiles once, and
//! what scripts see (output, errors, line numbers) doesn't change.

use std::time::Instant;

use axcenity_core::{ClassName, InstanceId, World};
use mlua::Lua;
use mlua::chunk::ChunkMode;

use super::*;

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

#[test]
fn the_same_source_compiles_once_and_runs_each_time() {
    // A source no other test uses, as the cache is per thread.
    let source = "print('cached chunk test', script.Name)";
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    let (hits, misses) = chunks::hits_and_misses();
    for _ in 0..3 {
        start(&mut runtime, &mut world, "scripts/a.server.lua", source);
    }
    let (hits_after, misses_after) = chunks::hits_and_misses();
    assert_eq!(misses_after - misses, 1);
    assert_eq!(hits_after - hits, 2);
    assert_eq!(
        lines(&mut runtime),
        vec!["cached chunk test\tScript"; 3],
        "each copy runs"
    );
}

#[test]
fn each_copy_gets_its_own_globals_and_script() {
    let source = "counter = (counter or 0) + 1 print('copy', counter)";
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    start(&mut runtime, &mut world, "scripts/a.server.lua", source);
    start(&mut runtime, &mut world, "scripts/b.server.lua", source);
    assert_eq!(lines(&mut runtime), vec!["copy\t1", "copy\t1"]);
}

#[test]
fn a_cached_chunk_reports_its_own_path_and_line() {
    let source = "local x = 1\nerror('boom from a cached chunk')";
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    start(&mut runtime, &mut world, "scripts/first.server.lua", source);
    start(
        &mut runtime,
        &mut world,
        "scripts/second.server.lua",
        source,
    );
    let output = lines(&mut runtime);
    assert_eq!(output.len(), 2, "{output:?}");
    assert!(
        output[0].contains("scripts/first.server.lua:2: boom from a cached chunk"),
        "{output:?}"
    );
    assert!(
        output[1].contains("scripts/second.server.lua:2: boom from a cached chunk"),
        "{output:?}"
    );
}

#[test]
fn a_syntax_error_reads_as_it_did_without_the_cache() {
    let source = "local x = = 1";
    let lua = Lua::new();
    let env = lua.create_table().unwrap();
    let cached = chunks::load(&lua, "scripts/bad.server.lua", source, env.clone())
        .unwrap_err()
        .to_string();
    let direct = lua
        .load(source)
        .set_name("@scripts/bad.server.lua")
        .set_mode(ChunkMode::Text)
        .set_environment(env)
        .into_function()
        .unwrap_err()
        .to_string();
    assert_eq!(cached, direct);
}

#[test]
fn bytecode_given_as_source_is_still_refused() {
    let bytecode = mlua::chunk::Compiler::new().compile("print('hi')").unwrap();
    // Only text that happens to be valid UTF-8 can reach a script's source.
    let Ok(text) = String::from_utf8(bytecode) else {
        return;
    };
    let lua = Lua::new();
    let env = lua.create_table().unwrap();
    assert!(chunks::load(&lua, "scripts/bytecode.server.lua", &text, env).is_err());
}

#[test]
fn the_debugger_gets_bytecode_with_local_names() {
    // Same source with and without the debugger: two entries, so a script
    // compiled without local names never reaches the debugger.
    let source = "local hidden_in_level_one = 5 return hidden_in_level_one";
    let plain = Lua::new();
    let debugged = Lua::new();
    crate::debugger::install(&debugged).unwrap();
    let (_, misses) = chunks::hits_and_misses();
    for lua in [&plain, &debugged] {
        let env = lua.create_table().unwrap();
        let function = chunks::load(lua, "scripts/d.server.lua", source, env).unwrap();
        assert_eq!(function.call::<i32>(()).unwrap(), 5);
    }
    let (_, misses_after) = chunks::hits_and_misses();
    assert_eq!(misses_after - misses, 2);
}

/// Starting 200 copies of a 300-line script, as cloning a model with
/// scripts in it does. Not a pass/fail test:
/// `cargo test -p axcenity-script --release --lib -- --ignored bench_script_start --nocapture`
#[test]
#[ignore]
fn bench_script_start() {
    // Fields of a table, not locals: a Luau function has at most 200 locals.
    let mut source = String::from("local total, M = 0, {}\n");
    for i in 0..300 {
        source.push_str(&format!(
            "function M.f{i}(a, b) if a > b then return a - b else return b + {i} end end\n\
             total = total + M.f{i}({i}, 3)\n"
        ));
    }
    let copies = 200;
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    let start_time = Instant::now();
    for i in 0..copies {
        start(
            &mut runtime,
            &mut world,
            &format!("scripts/copy{i}.server.lua"),
            &source,
        );
    }
    let elapsed = start_time.elapsed();
    let errors: Vec<_> = runtime
        .take_output()
        .into_iter()
        .filter(|line| line.kind == OutputKind::Error)
        .collect();
    assert!(errors.is_empty(), "{errors:?}");
    println!(
        "start_script, {copies} copies of a {} KB script: {:.0} us per copy ({elapsed:?} in all)",
        source.len() / 1024,
        elapsed.as_secs_f64() * 1e6 / f64::from(copies)
    );
}
