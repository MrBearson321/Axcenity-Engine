//! Tests for Phase 75's `workspace:SpawnDecal`: where the decal goes, its
//! fade and end, the Workspace's cap, and its errors.

use axcenity_core::{ClassName, Value, Vector3, World};

use super::*;

const FRAME: f64 = 1.0 / 60.0;

fn start(runtime: &mut ScriptRuntime, world: &mut World, source: &str) {
    let script = world.create(ClassName::Script).unwrap();
    world
        .set_parent(script, Some(world.server_script_service()))
        .unwrap();
    runtime.start_script(world, script, "scripts/decals.server.lua", source);
}

fn spawned(world: &World) -> Vec<axcenity_core::InstanceId> {
    world
        .get(world.workspace())
        .unwrap()
        .children()
        .iter()
        .copied()
        .filter(|&id| world.get(id).unwrap().class() == ClassName::ProjectedDecal)
        .collect()
}

fn number(world: &World, id: axcenity_core::InstanceId, name: &str) -> f64 {
    world
        .get(id)
        .unwrap()
        .property(name)
        .unwrap()
        .as_number()
        .unwrap()
}

#[test]
fn a_spawned_decal_faces_its_cframe_then_fades_out_and_is_destroyed() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        r#"
local hit = Vector3.new(0, 3, -5)
local normal = Vector3.new(0, 0, 1)
local decal = workspace:SpawnDecal("assets/textures/hole.png",
    CFrame.lookAt(hit, hit + normal), 0.5, 4)
print(decal.ClassName, decal.Name, decal.Parent == workspace, decal.Texture)
local look = decal.CFrame.LookVector
print(decal.Size, math.round(look.X) + 0, math.round(look.Y) + 0, math.round(look.Z) + 0)
"#,
    );
    assert_eq!(
        runtime
            .take_output()
            .into_iter()
            .map(|line| line.text.replace('\t', " "))
            .collect::<Vec<_>>(),
        [
            "ProjectedDecal SpawnedDecal true assets/textures/hole.png",
            "0.5, 0.5, 0.5 0 0 1",
        ]
    );
    let [decal] = spawned(&world)[..] else {
        panic!("one decal");
    };
    // 3 seconds in: not fading yet.
    for _ in 0..180 {
        runtime.step(&mut world, FRAME);
    }
    assert_eq!(number(&world, decal, "Transparency"), 0.0);
    // Halfway through its last second, half faded.
    for _ in 0..30 {
        runtime.step(&mut world, FRAME);
    }
    let halfway = number(&world, decal, "Transparency");
    assert!((halfway - 0.5).abs() < 0.03, "{halfway}");
    for _ in 0..29 {
        runtime.step(&mut world, FRAME);
    }
    assert!(world.contains(decal), "there just before 4 seconds");
    runtime.step(&mut world, FRAME);
    assert!(!world.contains(decal), "gone at 4 seconds");
}

#[test]
fn past_max_spawned_decals_the_oldest_go_first() {
    let mut world = World::new();
    let workspace = world.workspace();
    world
        .set_property(workspace, "MaxSpawnedDecals", Value::Number(5.0))
        .unwrap();
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        r#"
for i = 1, 8 do
    workspace:SpawnDecal("", CFrame.new(i, 0, 0), Vector3.new(1, 1, 1), 60)
end
"#,
    );
    let kept: Vec<f32> = spawned(&world)
        .into_iter()
        .map(|id| match world.get(id).unwrap().property("Position") {
            Some(Value::Vector3(v)) => v.x,
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(kept, [4.0, 5.0, 6.0, 7.0, 8.0]);
    // A decal destroyed by a script no longer counts toward the cap.
    start(
        &mut runtime,
        &mut world,
        r#"
workspace:FindFirstChild("SpawnedDecal"):Destroy()
workspace:SpawnDecal("", CFrame.new(9, 0, 0), 1, 60)
"#,
    );
    assert_eq!(spawned(&world).len(), 5);
    assert_eq!(
        world.get(spawned(&world)[0]).unwrap().property("Position"),
        Some(&Value::Vector3(Vector3::new(5.0, 0.0, 0.0)))
    );
}

#[test]
fn spawn_decal_explains_wrong_arguments() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        r#"
print(pcall(function() workspace:SpawnDecal(5, CFrame.new(), 1) end))
print(pcall(function() workspace:SpawnDecal("a.png", Vector3.new(), 1) end))
print(pcall(function() workspace:SpawnDecal("a.png", CFrame.new(), "big") end))
"#,
    );
    let output: Vec<String> = runtime.take_output().into_iter().map(|l| l.text).collect();
    assert_eq!(output.len(), 3, "{output:?}");
    assert!(output[0].contains("picture's path first"), "{output:?}");
    assert!(output[1].contains("a CFrame second"), "{output:?}");
    assert!(output[2].contains("size third"), "{output:?}");
    assert!(spawned(&world).is_empty());
}
