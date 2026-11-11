//! Checks across every game system: what they add matches what the API
//! definitions describe.

use axcenity_core::{ClassName, World};
use axcenity_script::{OutputKind, ScriptRuntime};

use crate::script_runtime;

const FRAME: f64 = 1.0 / 60.0;

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

/// Every Instance method a system adds is described, and exists on both
/// sides.
#[test]
fn described_system_methods_are_the_installed_ones_and_exist() {
    use axcenity_script::GameSystem;
    use axcenity_script::system::DESCRIBED_SYSTEM_METHODS;

    let lua = mlua::Lua::new();
    let abilities = crate::AbilitySystem::default();
    let tools = crate::ToolSystem::default();
    let story = crate::Story::default();
    let installed: Vec<(ClassName, &str)> = [
        &crate::Debris as &dyn GameSystem,
        &abilities,
        &tools,
        &story,
    ]
    .iter()
    .flat_map(|system| system.methods(&lua).unwrap())
    .map(|(class, name, _)| (class, name))
    .collect();
    assert_eq!(installed, DESCRIBED_SYSTEM_METHODS);

    let checks: Vec<String> = DESCRIBED_SYSTEM_METHODS
        .iter()
        .map(|(class, name)| {
            format!("assert(type(Instance.new('{class}').{name}) == 'function', '{class}:{name}')")
        })
        .collect();
    let source = format!("{}\nprint('checked')", checks.join("\n"));
    assert_eq!(run_both_sides(&source), ["checked", "checked"]);
}

/// Runs `source` as a Script and as a LocalScript for one frame, and
/// returns the output.
fn run_both_sides(source: &str) -> Vec<String> {
    let mut world = World::new();
    let mut runtime = script_runtime();
    let script = world.create(ClassName::Script).unwrap();
    world
        .set_parent(script, Some(world.server_script_service()))
        .unwrap();
    runtime.start_script(&mut world, script, "scripts/check.server.lua", source);
    let local = world.create(ClassName::LocalScript).unwrap();
    world
        .set_parent(local, Some(world.starter_player_scripts()))
        .unwrap();
    runtime.start_script(&mut world, local, "scripts/check.client.lua", source);
    runtime.step(&mut world, FRAME);
    lines(&mut runtime)
}

/// What `script`'s definition tests skip, because a bare runtime lacks it,
/// is checked here: every service a system adds is described, and every
/// described member of it exists on both sides.
#[test]
fn described_system_services_are_the_installed_ones_and_their_members_exist() {
    use axcenity_script::system::DESCRIBED_SYSTEM_SERVICES;
    use axcenity_script::{ApiKind, GameSystem, api_reference};

    let abilities = crate::AbilitySystem::default();
    let tools = crate::ToolSystem::default();
    let story = crate::Story::default();
    let installed: Vec<&str> = [
        &crate::Debris as &dyn GameSystem,
        &abilities,
        &tools,
        &story,
    ]
    .iter()
    .flat_map(|system| system.services().iter().copied())
    .collect();
    assert_eq!(installed, DESCRIBED_SYSTEM_SERVICES);

    let reference = api_reference();
    let mut checks = Vec::new();
    for service in DESCRIBED_SYSTEM_SERVICES {
        // Every system service is also `game.<Name>`; the definitions
        // describe that for some (`game.Debris`).
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
    assert_eq!(run_both_sides(&source), ["checked", "checked"]);
}
