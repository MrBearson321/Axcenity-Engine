//! `ConsoleService` (Phase 71): commands a game registers for the
//! Developer Console, on each side, and who may run the server's.

use axcenity_core::{ClassName, World};

use super::*;

fn start(runtime: &mut ScriptRuntime, world: &mut World, path: &str, source: &str) {
    let (class, service) = if path.ends_with(".client.lua") {
        (ClassName::LocalScript, world.starter_player_scripts())
    } else {
        (ClassName::Script, world.server_script_service())
    };
    let script = world.create(class).unwrap();
    world.set_parent(script, Some(service)).unwrap();
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

#[test]
fn a_registered_command_runs_with_the_player_and_its_words_and_can_wait() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        "scripts/heal.server.lua",
        r#"
local ConsoleService = game:GetService("ConsoleService")
ConsoleService:RegisterCommand("heal", "Heals by an amount", function(player, amount, extra)
    print("heal", player, amount, extra)
    task.wait(1)
    print("healed")
end)
"#,
    );
    start(
        &mut runtime,
        &mut world,
        "scripts/wave.client.lua",
        r#"
game:GetService("ConsoleService"):RegisterCommand("wave", "Waves", function()
    print("waving here")
end)
"#,
    );
    assert_eq!(lines(&mut runtime), Vec::<String>::new());
    let names: Vec<(String, String)> = runtime
        .console_commands()
        .into_iter()
        .map(|command| (command.name, command.help))
        .collect();
    assert_eq!(
        names,
        [
            ("wave".to_owned(), "Waves".to_owned()),
            ("heal".to_owned(), "Heals by an amount".to_owned()),
        ]
    );

    assert_eq!(
        runtime.run_console_command(&mut world, "heal 25  now"),
        ConsoleRun::Ran
    );
    assert_eq!(lines(&mut runtime), ["heal	Player	25	now"]);
    runtime.step(&mut world, 1.0);
    assert_eq!(lines(&mut runtime), ["healed"], "it waited in its own task");

    assert_eq!(
        runtime.run_console_command(&mut world, "wave"),
        ConsoleRun::Ran
    );
    assert_eq!(lines(&mut runtime), ["waving here"]);
    assert_eq!(
        runtime.run_console_command(&mut world, "dance"),
        ConsoleRun::Unknown
    );
    assert_eq!(
        runtime.run_console_command(&mut world, "   "),
        ConsoleRun::Unknown
    );
}

#[test]
fn only_allowed_players_on_other_devices_run_the_servers_commands() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        "scripts/admin.server.lua",
        r#"
local ConsoleService = game:GetService("ConsoleService")
ConsoleService.AllowedUserIds = {7}
ConsoleService:RegisterCommand("kickall", "Kicks everyone", function(player)
    print("kickall by", player.Name)
end)
print(#ConsoleService.AllowedUserIds, ConsoleService.AllowedUserIds[1])
"#,
    );
    assert_eq!(lines(&mut runtime), ["1	7"]);
    let ann = runtime.add_remote_player(&mut world, "Ann", "Ann", 7.0);
    let bob = runtime.add_remote_player(&mut world, "Bob", "Bob", 8.0);
    assert_eq!(
        runtime.run_remote_console_command(&mut world, bob, "kickall"),
        ConsoleRun::NotAllowed
    );
    assert_eq!(lines(&mut runtime), Vec::<String>::new());
    assert_eq!(
        runtime.run_remote_console_command(&mut world, ann, "kickall"),
        ConsoleRun::Ran
    );
    assert_eq!(lines(&mut runtime), ["kickall by	Ann"]);
    // This device (the host, or offline) always may.
    assert_eq!(
        runtime.run_console_command(&mut world, "kickall"),
        ConsoleRun::Ran
    );
}

#[test]
fn bad_registrations_say_what_is_wrong_and_a_stopped_script_takes_its_commands() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        "scripts/bad.server.lua",
        r#"
local ConsoleService = game:GetService("ConsoleService")
print(pcall(function() ConsoleService:RegisterCommand("two words", "", function() end) end))
print(pcall(function() ConsoleService:RegisterCommand("x", "", 5) end))
print(pcall(function() ConsoleService.AllowedUserIds = {"me"} end))
"#,
    );
    let output = lines(&mut runtime);
    assert_eq!(output.len(), 3, "{output:?}");
    assert!(output[0].contains("is one word"), "{output:?}");
    assert!(output[1].contains("takes a function"), "{output:?}");
    assert!(output[2].contains("holds UserIds"), "{output:?}");

    let script = world.create(ClassName::Script).unwrap();
    world
        .set_parent(script, Some(world.server_script_service()))
        .unwrap();
    runtime.start_script(
        &mut world,
        script,
        "scripts/temp.server.lua",
        r#"game:GetService("ConsoleService"):RegisterCommand("temp", "", function() end)"#,
    );
    assert_eq!(runtime.console_commands().len(), 1);
    world.destroy(script).unwrap();
    runtime.step(&mut world, 0.0);
    assert!(runtime.console_commands().is_empty());
}

#[test]
fn console_lua_knows_globals_and_names_set_by_earlier_commands() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    assert!(runtime.console_knows(&world, "print"));
    assert!(!runtime.console_knows(&world, "heal"));
    runtime.run_console_lua(&mut world, "heal = 5").unwrap();
    assert!(runtime.console_knows(&world, "heal"));
}
