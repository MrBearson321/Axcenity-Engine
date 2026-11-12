//! Hot reload with state kept (unified Phase 89), run in the real script
//! runtime with this crate's wrapper, as the editor's Play does.

use axcenity_core::{ClassName, InstanceId, World};
use axcenity_script::{OutputKind, ScriptRuntime};

fn runtime() -> ScriptRuntime {
    let mut runtime = ScriptRuntime::new();
    runtime.enable_hot_reload(crate::hot_reload::wrap);
    runtime
}

fn start(
    runtime: &mut ScriptRuntime,
    world: &mut World,
    class: ClassName,
    path: &str,
    source: &str,
) -> InstanceId {
    let script = world.create(class).unwrap();
    let name = path.rsplit('/').next().unwrap().split('.').next().unwrap();
    world.set_name(script, name).unwrap();
    let parent = match class {
        ClassName::Script => world.server_script_service(),
        _ => world.replicated_storage(),
    };
    world.set_parent(script, Some(parent)).unwrap();
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

/// Ten frames of 0.1 s, with Heartbeat.
fn second(runtime: &mut ScriptRuntime, world: &mut World) {
    for _ in 0..10 {
        runtime.step(world, 0.1);
        runtime.heartbeat(world, 0.1);
    }
}

fn parts_named(world: &World, name: &str) -> usize {
    world
        .get(world.workspace())
        .unwrap()
        .children()
        .iter()
        .filter(|&&id| world.get(id).is_some_and(|i| i.name() == name))
        .count()
}

const COUNTER: &str = r#"local count = 0
local step = 1
local made = Instance.new("Part")
made.Name = "Made"
made.Parent = workspace
game:GetService("RunService").Heartbeat:Connect(function()
    count += step
    if count % 10 == 0 then
        print("A " .. count)
    end
end)
"#;

#[test]
fn a_saved_script_keeps_its_counter_and_part_and_runs_the_new_text() {
    let mut world = World::new();
    let mut runtime = runtime();
    let path = "scripts/Counter.server.lua";
    let script = start(&mut runtime, &mut world, ClassName::Script, path, COUNTER);
    second(&mut runtime, &mut world);
    assert_eq!(lines(&mut runtime), ["A 10"]);

    let kept = runtime
        .reload_script(
            &mut world,
            script,
            path,
            &COUNTER.replace("\"A \"", "\"B \""),
        )
        .unwrap();
    assert_eq!(kept, ["count", "made", "step"]);
    second(&mut runtime, &mut world);
    // The old handler is gone, the new one counts on from 10.
    assert_eq!(lines(&mut runtime), ["B 20"]);
    assert_eq!(parts_named(&world, "Made"), 1, "no second part");
}

#[test]
fn an_edited_initializer_and_a_reset_local_take_their_new_values() {
    let mut world = World::new();
    let mut runtime = runtime();
    let path = "scripts/Counter.server.lua";
    let script = start(&mut runtime, &mut world, ClassName::Script, path, COUNTER);
    second(&mut runtime, &mut world);
    assert_eq!(lines(&mut runtime), ["A 10"]);

    // `step` changed, so it's 5 now; `count` carries on from 10.
    let new = COUNTER.replace("local step = 1", "local step = 5");
    let kept = runtime
        .reload_script(&mut world, script, path, &new)
        .unwrap();
    assert_eq!(kept, ["count", "made"]);
    second(&mut runtime, &mut world);
    assert_eq!(
        lines(&mut runtime),
        ["A 20", "A 30", "A 40", "A 50", "A 60"]
    );

    // `-- @reset` starts the count again.
    let reset = new.replace("local count = 0", "local count = 0 -- @reset");
    runtime
        .reload_script(&mut world, script, path, &reset)
        .unwrap();
    second(&mut runtime, &mut world);
    assert_eq!(
        lines(&mut runtime),
        ["A 10", "A 20", "A 30", "A 40", "A 50"]
    );
    assert_eq!(parts_named(&world, "Made"), 1);
}

const MODULE: &str = r#"local Greeter = {}
local calls = 0
function Greeter.greet()
    calls += 1
    return "hi " .. calls
end
return Greeter
"#;

const USER: &str = r#"local Greeter = require(game.ReplicatedStorage.Greeter)
local frames = 0
game:GetService("RunService").Heartbeat:Connect(function()
    frames += 1
    if frames % 10 == 0 then
        print(Greeter.greet(), frames)
    end
end)
"#;

#[test]
fn a_saved_module_keeps_its_table_and_state_and_its_user_keeps_counting() {
    let mut world = World::new();
    let mut runtime = runtime();
    let module_path = "scripts/Greeter.module.lua";
    let module = start(
        &mut runtime,
        &mut world,
        ClassName::ModuleScript,
        module_path,
        MODULE,
    );
    start(
        &mut runtime,
        &mut world,
        ClassName::Script,
        "scripts/User.server.lua",
        USER,
    );
    second(&mut runtime, &mut world);
    assert_eq!(lines(&mut runtime), ["hi 1\t10"]);

    let kept = runtime
        .reload_script(
            &mut world,
            module,
            module_path,
            &MODULE.replace("\"hi \"", "\"hello \""),
        )
        .unwrap();
    assert_eq!(kept, ["Greeter", "calls"]);
    second(&mut runtime, &mut world);
    assert_eq!(lines(&mut runtime), ["hello 2\t20"]);
}

#[test]
fn a_module_that_returns_a_new_table_fills_the_one_scripts_hold() {
    let mut world = World::new();
    let mut runtime = runtime();
    let module_path = "scripts/Greeter.module.lua";
    let literal = "return { greet = function() return 'hi' end, old = true }\n";
    let module = start(
        &mut runtime,
        &mut world,
        ClassName::ModuleScript,
        module_path,
        literal,
    );
    start(
        &mut runtime,
        &mut world,
        ClassName::Script,
        "scripts/User.server.lua",
        USER,
    );
    second(&mut runtime, &mut world);
    assert_eq!(lines(&mut runtime), ["hi\t10"]);

    let new = "return { greet = function() return 'hello' end }\n";
    runtime
        .reload_script(&mut world, module, module_path, new)
        .unwrap();
    second(&mut runtime, &mut world);
    assert_eq!(lines(&mut runtime), ["hello\t20"]);
}

#[test]
fn a_syntax_error_keeps_the_old_code_and_its_state() {
    let mut world = World::new();
    let mut runtime = runtime();
    let path = "scripts/Counter.server.lua";
    let script = start(&mut runtime, &mut world, ClassName::Script, path, COUNTER);
    second(&mut runtime, &mut world);
    assert!(
        runtime
            .reload_script(&mut world, script, path, "local count = (\n")
            .is_err()
    );
    let _ = lines(&mut runtime);
    second(&mut runtime, &mut world);
    assert_eq!(lines(&mut runtime), ["A 20"]);
}

#[test]
fn without_hot_reload_a_script_starts_from_scratch() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    let path = "scripts/Counter.server.lua";
    let script = start(&mut runtime, &mut world, ClassName::Script, path, COUNTER);
    second(&mut runtime, &mut world);
    let kept = runtime
        .reload_script(&mut world, script, path, COUNTER)
        .unwrap();
    assert!(kept.is_empty());
    second(&mut runtime, &mut world);
    assert_eq!(lines(&mut runtime), ["A 10", "A 10"]);
    assert_eq!(parts_named(&world, "Made"), 2);
}

#[test]
fn every_script_in_the_repo_still_compiles_once_wrapped_with_its_lines_in_place() {
    fn lua_files(folder: &std::path::Path, files: &mut Vec<std::path::PathBuf>) {
        let Ok(entries) = std::fs::read_dir(folder) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                lua_files(&path, files);
            } else if path.extension().is_some_and(|ending| ending == "lua") {
                files.push(path);
            }
        }
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = Vec::new();
    for folder in ["templates", "examples", "plugins"] {
        lua_files(&root.join(folder), &mut files);
    }
    assert!(files.len() > 30, "only {} scripts", files.len());
    let mut wrapped_count = 0;
    for file in &files {
        let source = std::fs::read_to_string(file).unwrap().replace("\r\n", "\n");
        let Some(wrapped) = crate::hot_reload::wrap(&source) else {
            continue;
        };
        wrapped_count += 1;
        let name = file.display().to_string();
        assert_eq!(
            wrapped.lines().count(),
            source.lines().count(),
            "{name} moved lines"
        );
        if let Some(error) = axcenity_script::syntax_error(&name, &wrapped) {
            panic!("{name} doesn't compile once wrapped: {error}\n{wrapped}");
        }
    }
    assert!(
        wrapped_count > 20,
        "only {wrapped_count} scripts had locals"
    );
}
