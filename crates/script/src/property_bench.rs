//! How long one `instance.Key` read or write takes from Luau, by kind of
//! member. Not pass/fail tests:
//! `cargo test -p axcenity-script --release --lib -- --ignored bench_member_access --nocapture`

use std::time::Instant;

use axcenity_core::{ClassName, World};

use super::*;

/// Runs `body` in a loop `count` times in one Script and returns the
/// nanoseconds per pass, less an empty loop's.
fn time_loop(setup: &str, body: &str, count: u32) -> f64 {
    let once = |body: &str| {
        let mut world = World::new();
        let part = world.create(ClassName::Part).unwrap();
        world.set_name(part, "P").unwrap();
        world.set_parent(part, Some(world.workspace())).unwrap();
        let sound = world.create(ClassName::Sound).unwrap();
        world.set_parent(sound, Some(world.workspace())).unwrap();
        let mut runtime = ScriptRuntime::new();
        let script = world.create(ClassName::Script).unwrap();
        world
            .set_parent(script, Some(world.server_script_service()))
            .unwrap();
        let source = format!(
            "local part = workspace.P\nlocal sound = workspace:FindFirstChildOfClass(\"Sound\")\n\
             local v = Vector3.new(1, 2, 3)\n{setup}\nfor i = 1, {count} do\n{body}\nend"
        );
        let start = Instant::now();
        runtime.start_script(&mut world, script, "scripts/bench.server.lua", &source);
        let elapsed = start.elapsed();
        let errors: Vec<_> = runtime
            .take_output()
            .into_iter()
            .filter(|line| line.kind == OutputKind::Error)
            .collect();
        assert!(errors.is_empty(), "{body}: {errors:?}");
        elapsed.as_nanos() as f64 / f64::from(count)
    };
    // Under a profiler, one run is enough (and the empty loop is noise).
    if std::env::var_os("BENCH_ONCE").is_some() {
        return once(body);
    }
    // The best of five, each less the best empty loop.
    let empty = (0..5).map(|_| once("")).fold(f64::INFINITY, f64::min);
    let full = (0..5).map(|_| once(body)).fold(f64::INFINITY, f64::min);
    full - empty
}

#[test]
#[ignore]
fn bench_member_access() {
    // `BENCH_ONLY=Position BENCH_COUNT=20000` runs fewer, for a profiler.
    let count = std::env::var("BENCH_COUNT")
        .ok()
        .and_then(|count| count.parse().ok())
        .unwrap_or(200_000);
    let only = std::env::var("BENCH_ONLY").unwrap_or_default();
    let cases = [
        ("read part.Position", "", "local _ = part.Position"),
        ("write part.Position", "", "part.Position = v"),
        ("read part.Anchored", "", "local _ = part.Anchored"),
        ("read part.Name", "", "local _ = part.Name"),
        ("read part.CFrame", "", "local _ = part.CFrame"),
        ("read part.Touched (event)", "", "local _ = part.Touched"),
        ("read sound.Play (class method)", "", "local _ = sound.Play"),
        ("read workspace.P (child)", "", "local _ = workspace.P"),
        (
            "call part:IsA(\"BasePart\")",
            "",
            "local _ = part:IsA(\"BasePart\")",
        ),
        ("call a Lua function", "local function f() end", "f()"),
    ];
    for (name, setup, body) in cases {
        if !name.contains(only.as_str()) {
            continue;
        }
        println!("{name}: {:.0} ns", time_loop(setup, body, count));
    }
}
