//! Tests for `ContentProvider:PreloadAsync` (roadmap M5). The load gate
//! holds a file open, so "the script is still paused" is checked while the
//! load can't possibly have finished, without relying on timing.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::mpsc::{self, Sender};
use std::time::{Duration, Instant};

use axcenity_assets::{Assets, LoadStatus};
use axcenity_core::{ClassName, Value, World};

use crate::{OutputKind, ScriptRuntime};

const LIMIT: Duration = Duration::from_secs(30);
const MODEL: &str = "assets/models/tri.gltf";

fn project(name: &str) -> PathBuf {
    let folder =
        std::env::temp_dir().join(format!("axcenity-preload-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(folder.join("assets/models")).unwrap();
    write_triangle(&folder, MODEL);
    folder
}

/// A one-triangle glTF, its buffer beside it.
fn write_triangle(folder: &Path, path: &str) {
    let corners: [f32; 9] = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
    let bytes: Vec<u8> = corners.iter().flat_map(|c| c.to_le_bytes()).collect();
    let buffer = format!("{path}.bin");
    let uri = buffer.rsplit('/').next().unwrap();
    let gltf = format!(
        r#"{{
            "asset": {{ "version": "2.0" }},
            "buffers": [ {{ "uri": "{uri}", "byteLength": 36 }} ],
            "bufferViews": [ {{ "buffer": 0, "byteLength": 36 }} ],
            "accessors": [ {{ "bufferView": 0, "componentType": 5126, "count": 3,
                              "type": "VEC3", "min": [0, 0, 0], "max": [1, 1, 0] }} ],
            "meshes": [ {{ "primitives": [ {{ "attributes": {{ "POSITION": 0 }} }} ] }} ]
        }}"#
    );
    std::fs::write(folder.join(&buffer), &bytes).unwrap();
    std::fs::write(folder.join(path), gltf).unwrap();
}

/// Holds `held` until the returned sender is sent to, and says on the
/// receiver when its load has reached that point.
fn gate_hold(assets: &Assets, held: &'static str) -> (Sender<()>, mpsc::Receiver<()>) {
    let (release, released) = mpsc::channel::<()>();
    let released = Mutex::new(released);
    let (reached, reaches) = mpsc::channel();
    let reached = Mutex::new(reached);
    assets.set_load_gate(move |path| {
        if path == held {
            let _ = reached.lock().unwrap().send(());
            let _ = released.lock().unwrap().recv_timeout(LIMIT);
        }
    });
    (release, reaches)
}

fn start(assets: Assets, source: &str) -> (ScriptRuntime, World) {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    runtime.set_assets(assets);
    let script = world.create(ClassName::Script).unwrap();
    world
        .set_parent(script, Some(world.server_script_service()))
        .unwrap();
    runtime.start_script(&mut world, script, "scripts/preload.server.lua", source);
    (runtime, world)
}

fn lines(runtime: &mut ScriptRuntime) -> Vec<String> {
    runtime
        .take_output()
        .into_iter()
        .map(|line| match line.kind {
            OutputKind::Error => format!("error: {}", line.text),
            _ => line.text.replace('\t', " "),
        })
        .collect()
}

/// Steps the game until a line has been printed that `done` likes, and
/// returns every line so far.
fn step_until(
    runtime: &mut ScriptRuntime,
    world: &mut World,
    done: impl Fn(&str) -> bool,
) -> Vec<String> {
    let started = Instant::now();
    let mut output = Vec::new();
    while !output.iter().any(|line: &String| done(line)) {
        assert!(started.elapsed() < LIMIT, "never came: {output:?}");
        runtime.step(world, 0.01);
        output.extend(lines(runtime));
    }
    output
}

#[test]
fn preload_async_pauses_the_script_until_the_file_has_loaded() {
    let folder = project("pause");
    let assets = Assets::new(&folder);
    let (release, reached) = gate_hold(&assets, MODEL);
    let (mut runtime, mut world) = start(
        assets.clone(),
        &format!(
            r#"
            print("before")
            game:GetService("ContentProvider"):PreloadAsync({{ "{MODEL}" }})
            print("after")
            "#
        ),
    );
    assert_eq!(lines(&mut runtime), ["before"]);
    reached.recv_timeout(LIMIT).expect("the load started");

    // Held at the gate: however many steps go by, the script stays paused.
    for _ in 0..5 {
        runtime.step(&mut world, 0.01);
    }
    assert!(lines(&mut runtime).is_empty());
    assert!(assets.model_ready(MODEL).is_none());

    release.send(()).unwrap();
    let output = step_until(&mut runtime, &mut world, |line| line == "after");
    assert_eq!(output, ["after"]);
    assert!(
        assets.model_ready(MODEL).is_some(),
        "loaded before it resumed"
    );
}

#[test]
fn other_scripts_run_while_one_is_paused_in_preload_async() {
    let folder = project("others");
    let assets = Assets::new(&folder);
    let (release, reached) = gate_hold(&assets, MODEL);
    let (mut runtime, mut world) = start(
        assets,
        &format!(
            r#"
            task.spawn(function()
                game:GetService("ContentProvider"):PreloadAsync({{ "{MODEL}" }})
                print("loaded")
            end)
            task.wait()
            print("still going")
            "#
        ),
    );
    reached.recv_timeout(LIMIT).expect("the load started");
    let output = step_until(&mut runtime, &mut world, |line| line == "still going");
    assert_eq!(output, ["still going"]);
    release.send(()).unwrap();
    step_until(&mut runtime, &mut world, |line| line == "loaded");
}

#[test]
fn the_callback_hears_how_each_file_went() {
    let folder = project("callback");
    let assets = Assets::new(&folder);
    let (mut runtime, mut world) = start(
        assets.clone(),
        &format!(
            r#"
            local ContentProvider = game:GetService("ContentProvider")
            local statuses = {{}}
            ContentProvider:PreloadAsync({{ "{MODEL}", "assets/models/gone.glb" }},
                function(id, status)
                    statuses[id] = status
                end)
            print(statuses["{MODEL}"] == Enum.AssetFetchStatus.Success)
            print(statuses["assets/models/gone.glb"] == Enum.AssetFetchStatus.Failure)
            print("done")
            "#
        ),
    );
    let output = step_until(&mut runtime, &mut world, |line| line == "done");
    assert_eq!(output, ["true", "true", "done"]);
    let errors = assets.take_errors();
    assert_eq!(errors.len(), 1, "a failed file is reported: {errors:?}");
    assert!(errors[0].contains("gone.glb"), "{errors:?}");
}

#[test]
fn a_callback_that_fails_doesnt_stop_the_rest() {
    let folder = project("callback-fails");
    let (mut runtime, mut world) = start(
        Assets::new(&folder),
        &format!(
            r#"
            game:GetService("ContentProvider"):PreloadAsync({{ "{MODEL}" }}, function()
                error("oops")
            end)
            print("done")
            "#
        ),
    );
    let output = step_until(&mut runtime, &mut world, |line| line == "done");
    assert!(output[0].contains("oops"), "{output:?}");
    assert_eq!(output[1], "done");
}

#[test]
fn an_instance_stands_for_the_files_it_and_its_descendants_use() {
    let folder = project("instance");
    write_triangle(&folder, "assets/models/child.gltf");
    let assets = Assets::new(&folder);
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    runtime.set_assets(assets.clone());
    let parent = world.create(ClassName::MeshPart).unwrap();
    world
        .set_property(parent, "MeshId", Value::String(MODEL.into()))
        .unwrap();
    world.set_name(parent, "Target").unwrap();
    world.set_parent(parent, Some(world.workspace())).unwrap();
    let child = world.create(ClassName::MeshPart).unwrap();
    world
        .set_property(
            child,
            "MeshId",
            Value::String("assets/models/child.gltf".into()),
        )
        .unwrap();
    world.set_parent(child, Some(parent)).unwrap();
    let script = world.create(ClassName::Script).unwrap();
    world
        .set_parent(script, Some(world.server_script_service()))
        .unwrap();
    runtime.start_script(
        &mut world,
        script,
        "scripts/preload.server.lua",
        r#"
        local count = 0
        game.ContentProvider:PreloadAsync({ workspace.Target }, function(_, status)
            if status == Enum.AssetFetchStatus.Success then
                count += 1
            end
        end)
        print(count)
        print("done")
        "#,
    );
    let output = step_until(&mut runtime, &mut world, |line| line == "done");
    assert_eq!(output, ["2", "done"]);
    assert!(assets.model_ready(MODEL).is_some());
    assert!(assets.model_ready("assets/models/child.gltf").is_some());
}

#[test]
fn an_empty_list_resumes_on_the_next_step() {
    let folder = project("empty");
    let (mut runtime, mut world) = start(
        Assets::new(&folder),
        r#"
        game:GetService("ContentProvider"):PreloadAsync({})
        print("done")
        "#,
    );
    assert_eq!(
        step_until(&mut runtime, &mut world, |line| line == "done"),
        ["done"]
    );
}

#[test]
fn wrong_arguments_are_errors_that_say_what_was_wrong() {
    let folder = project("wrong");
    let (mut runtime, mut world) = start(
        Assets::new(&folder),
        r#"
        local ContentProvider = game:GetService("ContentProvider")
        print(pcall(function() ContentProvider:PreloadAsync("x") end))
        print(pcall(function() ContentProvider:PreloadAsync({ 5 }) end))
        print(pcall(function() ContentProvider:PreloadAsync({}, 5) end))
        print(tostring(ContentProvider))
        print(ContentProvider.RequestQueueSize)
        print("done")
        "#,
    );
    let output = step_until(&mut runtime, &mut world, |line| line == "done");
    assert!(
        output[0].contains("contentIds must be a list"),
        "{output:?}"
    );
    assert!(
        output[1].contains("contentIds must be a list"),
        "{output:?}"
    );
    assert!(
        output[2].contains("callback must be a function"),
        "{output:?}"
    );
    assert_eq!(output[3], "ContentProvider");
    assert_eq!(output[4], "0");
}

#[test]
fn a_stopped_script_is_not_resumed() {
    let folder = project("stopped");
    let assets = Assets::new(&folder);
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    runtime.set_assets(assets.clone());
    let script = world.create(ClassName::Script).unwrap();
    world
        .set_parent(script, Some(world.server_script_service()))
        .unwrap();
    runtime.start_script(
        &mut world,
        script,
        "scripts/preload.server.lua",
        &format!(
            r#"
            game:GetService("ContentProvider"):PreloadAsync({{ "{MODEL}" }})
            print("resumed")
            "#
        ),
    );
    runtime.server.stop_script(script);
    assert_eq!(assets.request_model(MODEL).wait(), LoadStatus::Ready);
    for _ in 0..5 {
        runtime.step(&mut world, 0.01);
    }
    assert!(lines(&mut runtime).is_empty());
}
