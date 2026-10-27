//! `GuiService` (the game UI settings), the `UI/…` settings and slots that
//! keep them, `AccessibleName`, and reduced motion for UI tweens.

use std::path::PathBuf;

use axcenity_core::{ClassName, InstanceId, UiSettings, Value, World};

use super::*;

fn save_folder(test: &str) -> PathBuf {
    let folder = std::env::temp_dir().join(format!(
        "axcenity-script-ui-settings-{test}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&folder);
    folder
}

fn add(world: &mut World, class: ClassName, parent: InstanceId, name: &str) -> InstanceId {
    let id = world.create(class).unwrap();
    world.set_name(id, name).unwrap();
    world.set_parent(id, Some(parent)).unwrap();
    id
}

/// A Part `Block` and a ScreenGui `Hud` with a Frame `Menu` holding a
/// TextButton `Play`.
fn level() -> (World, InstanceId, InstanceId, InstanceId) {
    let mut world = World::new();
    let workspace = world.workspace();
    let block = add(&mut world, ClassName::Part, workspace, "Block");
    let hud = add(&mut world, ClassName::ScreenGui, workspace, "Hud");
    let menu = add(&mut world, ClassName::Frame, hud, "Menu");
    let play = add(&mut world, ClassName::TextButton, menu, "Play");
    (world, block, menu, play)
}

fn run(runtime: &mut ScriptRuntime, world: &mut World, class: ClassName, source: &str) {
    let parent = match class {
        ClassName::LocalScript => world.starter_player_scripts(),
        _ => world.server_script_service(),
    };
    let script = add(world, class, parent, "Settings");
    runtime.start_script(world, script, "scripts/Settings.lua", source);
}

fn output(runtime: &mut ScriptRuntime) -> Vec<String> {
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
fn gui_service_reads_and_sets_the_game_ui_settings_on_both_sides() {
    let (mut world, ..) = level();
    let mut runtime = ScriptRuntime::new();
    run(
        &mut runtime,
        &mut world,
        ClassName::LocalScript,
        r#"
        local GuiService = game:GetService("GuiService")
        print(GuiService.TextScale, GuiService.HighContrast, GuiService.ReducedMotion)
        GuiService.TextScale = 1.2
        GuiService.HighContrast = true
        game.GuiService.ReducedMotion = true
        print(GuiService.TextScale, GuiService.HighContrast, GuiService.ReducedMotion, GuiService)
        "#,
    );
    run(
        &mut runtime,
        &mut world,
        ClassName::Script,
        r#"print(game:GetService("GuiService").TextScale)"#,
    );
    assert_eq!(
        output(&mut runtime),
        ["1\tfalse\tfalse", "1.2\ttrue\ttrue\tGuiService", "1.2"]
    );
    assert_eq!(
        world.ui_settings(),
        UiSettings {
            text_scale: 1.2,
            high_contrast: true,
            reduced_motion: true,
        }
    );
}

#[test]
fn a_wrong_game_ui_setting_is_an_error_and_changes_nothing() {
    let (mut world, ..) = level();
    let mut runtime = ScriptRuntime::new();
    run(
        &mut runtime,
        &mut world,
        ClassName::LocalScript,
        r#"
        local GuiService = game:GetService("GuiService")
        print(pcall(function() GuiService.TextScale = 10 end))
        print(pcall(function() GuiService.TextScale = "big" end))
        print(pcall(function() GuiService.HighContrast = 1 end))
        print(pcall(function() game.SaveService:SetSetting("UI/Colours", true) end))
        print(pcall(function() game.SaveService:SetSetting("UI/TextScale", 0.1) end))
        "#,
    );
    let output = output(&mut runtime);
    assert!(
        output[0].ends_with("GuiService.TextScale must be a number from 0.5 to 3, not 10"),
        "{}",
        output[0]
    );
    assert!(output[1].ends_with("not a string"), "{}", output[1]);
    assert!(
        output[2].ends_with("GuiService.HighContrast must be true or false, not 1"),
        "{}",
        output[2]
    );
    assert!(
        output[3].ends_with(
            "\"UI/Colours\" isn't a game UI setting; the UI/ settings are UI/TextScale, \
             UI/HighContrast, UI/ReducedMotion"
        ),
        "{}",
        output[3]
    );
    assert!(
        output[4].ends_with("UI/TextScale must be a number from 0.5 to 3"),
        "{}",
        output[4]
    );
    assert_eq!(world.ui_settings(), UiSettings::default());
}

#[test]
fn the_ui_settings_are_save_settings_kept_in_a_slot_and_restored_on_load() {
    let folder = save_folder("slot");
    let (mut world, ..) = level();
    let mut runtime = ScriptRuntime::new();
    runtime.set_save_folder(Some(folder.clone()));
    run(
        &mut runtime,
        &mut world,
        ClassName::LocalScript,
        r#"
        local GuiService = game:GetService("GuiService")
        local SaveService = game:GetService("SaveService")
        GuiService.TextScale = 1.5
        SaveService:SetSetting("UI/HighContrast", true)
        SaveService:SetSetting("Volume", 0.25)
        print(SaveService:GetSetting("UI/TextScale"), GuiService.HighContrast)
        SaveService:SaveSlot(1)
        GuiService.TextScale = 1
        GuiService.HighContrast = false
        SaveService:SetSetting("UI/ReducedMotion", true)
        SaveService:SetSetting("UI/ReducedMotion", nil)
        print(GuiService.TextScale, GuiService.HighContrast, GuiService.ReducedMotion)
        SaveService:LoadSlot(1)
        print(GuiService.TextScale, GuiService.HighContrast, SaveService:GetSetting("Volume"))
        "#,
    );
    assert_eq!(
        output(&mut runtime),
        ["1.5\ttrue", "1\tfalse\tfalse", "1.5\ttrue\t0.25"]
    );
    let slot = std::fs::read_to_string(folder.join("slot-1.json")).unwrap();
    for line in [
        "\"UI/TextScale\": 1.5",
        "\"UI/HighContrast\": true",
        "\"UI/ReducedMotion\": false",
        "\"Volume\": 0.25",
        "\"version\": 2",
    ] {
        assert!(slot.contains(line), "{line} in {slot}");
    }

    // A new game loads the slot and starts with the player's settings.
    let (mut fresh, ..) = level();
    let mut runtime = ScriptRuntime::new();
    runtime.set_save_folder(Some(folder.clone()));
    run(
        &mut runtime,
        &mut fresh,
        ClassName::Script,
        r#"game.SaveService:LoadSlot(1)"#,
    );
    assert_eq!(output(&mut runtime), Vec::<String>::new());
    assert_eq!(fresh.ui_settings().text_scale, 1.5);
    assert!(fresh.ui_settings().high_contrast);
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn a_script_reads_each_controls_accessible_name_which_defaults_to_its_name() {
    let (mut world, _, menu, play) = level();
    world
        .set_property(play, "AccessibleName", Value::String("Start game".into()))
        .unwrap();
    let mut runtime = ScriptRuntime::new();
    run(
        &mut runtime,
        &mut world,
        ClassName::LocalScript,
        r#"
        local menu = workspace.Hud.Menu
        print(menu.AccessibleName, menu.Play.AccessibleName)
        menu.AccessibleName = "Main menu"
        menu.Play.AccessibleName = ""
        print(menu.AccessibleName, menu.Play.AccessibleName)
        print(pcall(function() return workspace.Block.AccessibleName end))
        "#,
    );
    let output = output(&mut runtime);
    assert_eq!(output[..2], ["Menu\tStart game", "Main menu\tPlay"]);
    assert!(output[2].starts_with("false"), "{}", output[2]);
    assert_eq!(
        world.get(menu).unwrap().property("AccessibleName"),
        Some(&Value::String("Main menu".into()))
    );
}

#[test]
fn reduced_motion_ends_a_ui_tween_at_once_and_leaves_a_part_tween_alone() {
    let (mut world, block, menu, _) = level();
    let mut settings = world.ui_settings();
    settings.reduced_motion = true;
    world.set_ui_settings(settings);
    let mut runtime = ScriptRuntime::new();
    run(
        &mut runtime,
        &mut world,
        ClassName::Script,
        r#"
        local TweenService = game:GetService("TweenService")
        local info = TweenInfo.new(1, Enum.EasingStyle.Quad, Enum.EasingDirection.Out)
        local move = TweenService:Create(workspace.Block, info, {Position = Vector3.new(10, 0, 0)})
        local fade = TweenService:Create(workspace.Hud.Menu, info, {BackgroundTransparency = 1})
        local pulse = TweenService:Create(workspace.Hud.Menu.Play, TweenInfo.new(0.5, Enum.EasingStyle.Linear, Enum.EasingDirection.Out, -1, true), {Rotation = 20})
        fade.Completed:Connect(function(state) print("faded", state) end)
        pulse.Completed:Connect(function(state) print("pulsed", state) end)
        move:Play()
        fade:Play()
        pulse:Play()
        "#,
    );
    runtime.step(&mut world, 0.1);
    let number = |world: &World, id: InstanceId, name: &str| {
        world
            .get(id)
            .unwrap()
            .property(name)
            .and_then(Value::as_number)
            .unwrap()
    };
    assert_eq!(number(&world, menu, "BackgroundTransparency"), 1.0);
    let position = world.get(block).unwrap().property("Position").cloned();
    let Some(Value::Vector3(position)) = position else {
        panic!("{position:?}");
    };
    assert!(
        position.x > 0.0 && position.x < 10.0,
        "the part still moves: {position:?}"
    );
    runtime.step(&mut world, 0.0);
    assert_eq!(
        output(&mut runtime),
        [
            "faded\tEnum.PlaybackState.Completed",
            "pulsed\tEnum.PlaybackState.Completed"
        ]
    );
    // A forever tween that goes there and back ends where it started.
    let play = world
        .find_first_child(menu, "Play")
        .expect("the button is there");
    assert_eq!(number(&world, play, "Rotation"), 0.0);
}
