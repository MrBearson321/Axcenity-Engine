//! Debris, moved from `script` (roadmap L7): the same behaviour, now
//! through the `GameSystem` it registers.

use axcenity_core::{ClassName, World};
use axcenity_script::{OutputKind, ScriptRuntime};

use crate::script_runtime;

const FRAME: f64 = 1.0 / 60.0;

/// Starts `source` as a Script in ServerScriptService.
fn start(runtime: &mut ScriptRuntime, world: &mut World, path: &str, source: &str) {
    let script = world.create(ClassName::Script).unwrap();
    world
        .set_parent(script, Some(world.server_script_service()))
        .unwrap();
    runtime.start_script(world, script, path, source);
}

/// The output's prints, and errors marked `error: `.
fn lines(runtime: &mut ScriptRuntime) -> Vec<String> {
    runtime
        .take_output()
        .into_iter()
        .map(|line| match line.kind {
            OutputKind::Print => line.text.replace('\t', " "),
            _ => format!("error: {}", line.text),
        })
        .collect()
}

/// A Part named `name` in the Workspace.
fn part(world: &mut World, name: &str) -> axcenity_core::InstanceId {
    let part = world.create(ClassName::Part).unwrap();
    world.set_name(part, name).unwrap();
    world.set_parent(part, Some(world.workspace())).unwrap();
    part
}

#[test]
fn debris_destroys_a_part_about_two_seconds_later_and_ignores_one_already_gone() {
    let mut world = World::new();
    let spark = part(&mut world, "Spark");
    let gone = part(&mut world, "Gone");
    let mut runtime = script_runtime();
    start(
        &mut runtime,
        &mut world,
        "scripts/debris.server.lua",
        r#"
local Debris = game:GetService("Debris")
workspace.Spark.Destroying:Connect(function() print("spark gone") end)
Debris:AddItem(workspace.Spark, 2)
local gone = workspace.Gone
Debris:AddItem(gone, 1)
gone:Destroy()
Debris:AddItem(gone, 0)
"#,
    );
    // The script that added them stopping doesn't keep them.
    for _ in 0..119 {
        runtime.step(&mut world, FRAME);
    }
    assert!(world.contains(spark), "still there just before two seconds");
    assert!(!world.contains(gone));
    runtime.step(&mut world, FRAME);
    assert!(!world.contains(spark), "gone at two seconds");
    assert_eq!(lines(&mut runtime), ["spark gone"]);
}

#[test]
fn game_dot_debris_is_the_service_and_bad_arguments_name_their_type() {
    let mut world = World::new();
    part(&mut world, "Spark");
    let mut runtime = script_runtime();
    start(
        &mut runtime,
        &mut world,
        "scripts/args.server.lua",
        r#"
print(tostring(game.Debris), tostring(game:GetService("Debris")))
print(pcall(function() game.Debris:AddItem(5) end))
print(pcall(function() game.Debris:AddItem(workspace.Spark, "soon") end))
game.Debris:AddItem(workspace.Spark)
"#,
    );
    let printed = lines(&mut runtime);
    assert_eq!(printed[0], "Debris Debris");
    assert!(
        printed[1].ends_with("Debris:AddItem takes an Instance to destroy, not a number"),
        "{printed:?}"
    );
    assert!(
        printed[2].ends_with("Debris:AddItem takes a number of seconds, not a string"),
        "{printed:?}"
    );
    // Without a time it lasts ten seconds, as in Roblox.
    for _ in 0..599 {
        runtime.step(&mut world, FRAME);
    }
    assert!(world.find_first_child(world.workspace(), "Spark").is_some());
    runtime.step(&mut world, FRAME);
    assert!(world.find_first_child(world.workspace(), "Spark").is_none());
}

#[test]
fn both_sides_have_debris_and_each_destroys_its_own_items() {
    let mut world = World::new();
    let spark = part(&mut world, "Spark");
    let mut runtime = script_runtime();
    let local = world.create(ClassName::LocalScript).unwrap();
    world
        .set_parent(local, Some(world.starter_player_scripts()))
        .unwrap();
    runtime.start_script(
        &mut world,
        local,
        "scripts/debris.client.lua",
        r#"game:GetService("Debris"):AddItem(workspace.Spark, 0.5)"#,
    );
    for _ in 0..31 {
        runtime.step(&mut world, FRAME);
    }
    assert!(!world.contains(spark));
    assert!(lines(&mut runtime).is_empty());
}

#[test]
fn a_runtime_without_the_systems_has_no_debris_and_says_which_services_exist() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        "scripts/none.server.lua",
        r#"print(pcall(function() return game:GetService("Debris") end))"#,
    );
    let printed = lines(&mut runtime);
    assert!(printed[0].starts_with("false"), "{printed:?}");
    assert!(
        printed[0].contains("no service named \"Debris\""),
        "{printed:?}"
    );

    // With them, the list of services names it.
    let mut runtime = script_runtime();
    start(
        &mut runtime,
        &mut world,
        "scripts/list.server.lua",
        r#"print(pcall(function() return game:GetService("Nope") end))"#,
    );
    let printed = lines(&mut runtime);
    assert!(printed[0].contains("Debris"), "{printed:?}");
}

/// What `script`'s definition tests skip, because a bare runtime lacks it,
/// is checked here: every service a system adds is described, and every
/// described member of it exists on both sides.
#[test]
fn described_system_services_are_the_installed_ones_and_their_members_exist() {
    use axcenity_script::system::DESCRIBED_SYSTEM_SERVICES;
    use axcenity_script::{ApiKind, GameSystem, api_reference};

    let installed: Vec<&str> = [&crate::Debris as &dyn GameSystem]
        .iter()
        .flat_map(|system| system.services().iter().copied())
        .collect();
    assert_eq!(installed, DESCRIBED_SYSTEM_SERVICES);

    let reference = api_reference();
    let mut checks = Vec::new();
    for service in DESCRIBED_SYSTEM_SERVICES {
        assert!(
            reference
                .iter()
                .any(|entry| entry.owner == "DataModel" && entry.name == *service),
            "game.{service} is described"
        );
        checks.push(format!("assert(game.{service} ~= nil, 'game.{service}')"));
        let members: Vec<_> = reference
            .iter()
            .filter(|entry| entry.owner == *service)
            .collect();
        assert!(!members.is_empty(), "{service} has described members");
        for entry in members {
            let name = &entry.name;
            checks.push(match entry.kind {
                ApiKind::Function => format!(
                    "assert(type(game:GetService('{service}').{name}) == 'function', '{service}:{name}')"
                ),
                _ => format!("assert(game:GetService('{service}').{name} ~= nil, '{service}.{name}')"),
            });
        }
    }
    let source = format!("{}\nprint('checked')", checks.join("\n"));
    let mut world = World::new();
    let mut runtime = script_runtime();
    start(
        &mut runtime,
        &mut world,
        "scripts/check.server.lua",
        &source,
    );
    let local = world.create(ClassName::LocalScript).unwrap();
    world
        .set_parent(local, Some(world.starter_player_scripts()))
        .unwrap();
    runtime.start_script(&mut world, local, "scripts/check.client.lua", &source);
    runtime.step(&mut world, FRAME);
    assert_eq!(lines(&mut runtime), ["checked", "checked"]);
}
