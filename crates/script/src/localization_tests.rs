use std::sync::Arc;

use axcenity_core::{ClassName, StringTable, World};

use super::*;

fn world_with_strings() -> World {
    let mut table = StringTable::default();
    table.add_language("English");
    table.add_language("Français");
    table.set("menu.play", "English", "Play");
    table.set("menu.play", "Français", "Jouer");
    table.set("menu.quit", "English", "Quit");
    let mut world = World::new();
    world.set_strings(Arc::new(table));
    world
}

fn run(world: &mut World, class: ClassName, source: &str) -> Vec<String> {
    let mut runtime = ScriptRuntime::new();
    let script = world.create(class).unwrap();
    world.set_name(script, "Language").unwrap();
    let parent = match class {
        ClassName::LocalScript => world.starter_player_scripts(),
        _ => world.server_script_service(),
    };
    world.set_parent(script, Some(parent)).unwrap();
    runtime.start_script(world, script, "scripts/Language.lua", source);
    runtime.step(world, 1.0 / 60.0);
    runtime
        .take_output()
        .into_iter()
        .map(|line| match line.kind {
            OutputKind::Print => line.text,
            OutputKind::Error => format!("error: {}", line.text),
        })
        .collect()
}

#[test]
fn a_script_reads_and_sets_the_language_and_looks_up_strings() {
    let mut world = world_with_strings();
    let output = run(
        &mut world,
        ClassName::LocalScript,
        r#"
        local localization = game:GetService("LocalizationService")
        print(localization.Language, table.concat(localization.Languages, ","))
        print(localization:GetString("menu.play"))
        localization.Language = "Français"
        print(game.LocalizationService.Language, localization:GetString("menu.play"))
        print(localization:GetString("menu.quit"), localization:GetString("nope"))
        "#,
    );
    assert_eq!(
        output,
        [
            "English\tEnglish,Français",
            "Play",
            "Français\tJouer",
            "nil\tnil",
        ]
    );
    assert_eq!(world.language(), "Français");
}

#[test]
fn an_unknown_language_is_an_error_and_keeps_the_active_one() {
    let mut world = world_with_strings();
    let output = run(
        &mut world,
        ClassName::Script,
        r#"
        local localization = game:GetService("LocalizationService")
        print(pcall(function() localization.Language = "Klingon" end))
        print(pcall(function() localization.Language = 3 end))
        print(localization.Language)
        "#,
    );
    assert!(
        output[0].ends_with(
            "the string table has no language \"Klingon\"; choose from English, Français"
        ),
        "{}",
        output[0]
    );
    assert!(output[1].contains("not a number"), "{}", output[1]);
    assert_eq!(output[2], "English");
    assert_eq!(world.language(), "English");
}
