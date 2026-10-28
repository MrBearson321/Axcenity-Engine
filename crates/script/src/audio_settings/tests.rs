use std::path::PathBuf;

use axcenity_core::{ClassName, InstanceId, Value, World};

use super::*;
use crate::{OutputKind, ScriptRuntime};

fn add(world: &mut World, class: ClassName, parent: InstanceId, name: &str) -> InstanceId {
    let id = world.create(class).unwrap();
    world.set_name(id, name).unwrap();
    world.set_parent(id, Some(parent)).unwrap();
    id
}

/// Master in SoundService, with Music and SFX inside it, both adjustable
/// by the player.
fn level() -> (World, [InstanceId; 3]) {
    let mut world = World::new();
    let service = world.sound_service();
    let master = add(&mut world, ClassName::SoundGroup, service, "Master");
    let music = add(&mut world, ClassName::SoundGroup, master, "Music");
    let sfx = add(&mut world, ClassName::SoundGroup, master, "SFX");
    for group in [music, sfx] {
        world
            .set_property(group, "UserAdjustable", Value::Bool(true))
            .unwrap();
    }
    (world, [master, music, sfx])
}

fn run(runtime: &mut ScriptRuntime, world: &mut World, source: &str) {
    let parent = world.starter_player_scripts();
    let script = add(world, ClassName::LocalScript, parent, "Audio");
    runtime.start_script(world, script, "scripts/Audio.client.lua", source);
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

fn volume(world: &World, id: InstanceId) -> f64 {
    world
        .get(id)
        .and_then(|group| group.property("Volume"))
        .and_then(Value::as_number)
        .unwrap()
}

fn slider(world: &World, name: &str) -> InstanceId {
    let gui = world
        .find_first_child(world.workspace(), GUI_NAME)
        .expect("the default audio settings UI");
    let panel = world.find_first_child(gui, "Panel").unwrap();
    world.find_first_child(panel, name).unwrap()
}

fn save_folder(test: &str) -> PathBuf {
    let folder = std::env::temp_dir().join(format!(
        "axcenity-script-audio-{test}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&folder);
    folder
}

#[test]
fn music_and_sfx_are_set_separately_from_lua_and_the_settings_ui_and_persist_in_a_slot() {
    let (mut world, [_, music, sfx]) = level();
    let folder = save_folder("groups");
    let mut runtime = ScriptRuntime::new();
    runtime.set_save_folder(Some(folder.clone()));
    run(
        &mut runtime,
        &mut world,
        r#"
        local SoundService = game:GetService("SoundService")
        SoundService.Master.Music.Volume = 0.3
        game:GetService("UserGameSettings").MasterVolume = 0.7
        "#,
    );
    runtime.step(&mut world, 1.0 / 60.0);
    assert_eq!(output(&mut runtime), Vec::<String>::new());
    assert_eq!(volume(&world, music), 0.3);
    assert_eq!(volume(&world, sfx), 1.0, "set separately");
    assert_eq!(world.master_volume(), 0.7);

    // The settings UI was made at the start, hidden, and shows them.
    let gui = world.find_first_child(world.workspace(), GUI_NAME).unwrap();
    assert_eq!(
        world.get(gui).unwrap().property("Enabled"),
        Some(&Value::Bool(false))
    );
    let value = |world: &World, slider| {
        world
            .get(slider)
            .and_then(|s| s.property("Value"))
            .and_then(Value::as_number)
            .unwrap()
    };
    let (music_slider, sfx_slider) = (slider(&world, "Music"), slider(&world, "SFX"));
    let master_slider = slider(&world, "Master volume");
    assert!((value(&world, music_slider) - 0.3).abs() < 1e-9);
    assert_eq!(value(&world, sfx_slider), 1.0);
    assert!((value(&world, master_slider) - 0.7).abs() < 1e-9);
    assert!(
        world
            .find_first_child(world.get(gui).unwrap().children()[0], "Master")
            .is_none(),
        "Master isn't UserAdjustable"
    );

    // The player drags SFX down; Music stays.
    world
        .set_property(sfx_slider, "Value", Value::Number(0.4))
        .unwrap();
    runtime.step(&mut world, 1.0 / 60.0);
    assert!((volume(&world, sfx) - 0.4).abs() < 1e-9);
    assert_eq!(volume(&world, music), 0.3);
    world
        .set_property(master_slider, "Value", Value::Number(0.5))
        .unwrap();
    runtime.step(&mut world, 1.0 / 60.0);
    assert!((world.master_volume() - 0.5).abs() < 1e-9);

    // Saved, changed, and loaded back.
    run(
        &mut runtime,
        &mut world,
        r#"
        local SoundService = game:GetService("SoundService")
        local SaveService = game:GetService("SaveService")
        local Settings = game:GetService("UserGameSettings")
        SaveService:SaveSlot(1)
        SoundService.Master.Music.Volume = 1
        SoundService.Master.SFX.Volume = 1
        Settings.MasterVolume = 1
        SaveService:LoadSlot(1)
        print(SoundService.Master.Music.Volume, SoundService.Master.SFX.Volume, Settings.MasterVolume)
        print(SaveService:GetSetting("Audio/MasterVolume"))
        "#,
    );
    assert_eq!(output(&mut runtime), ["0.3\t0.4\t0.5", "0.5"]);
    runtime.step(&mut world, 1.0 / 60.0);
    assert!(
        (value(&world, sfx_slider) - 0.4).abs() < 1e-9,
        "the slider follows the load"
    );
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn no_adjustable_group_means_no_settings_ui_and_a_hand_made_slider_still_binds() {
    let mut world = World::new();
    let service = world.sound_service();
    let music = add(&mut world, ClassName::SoundGroup, service, "Music");
    let workspace = world.workspace();
    let menu = add(&mut world, ClassName::ScreenGui, workspace, "Pause");
    let slider = add(&mut world, ClassName::Slider, menu, "MusicVolume");
    world
        .set_property(slider, "Step", Value::Number(0.05))
        .unwrap();
    let binding = add(&mut world, ClassName::ObjectValue, slider, GROUP_BINDING);
    world
        .set_property(binding, "Value", Value::Instance(Some(music)))
        .unwrap();

    let mut sliders = VolumeSliders::default();
    sliders.step(&mut world);
    assert!(world.find_first_child(workspace, GUI_NAME).is_none());
    let read = |world: &World, name| {
        world
            .get(slider)
            .and_then(|s| s.property(name))
            .and_then(Value::as_number)
            .unwrap()
    };
    assert_eq!(
        (read(&world, "MinValue"), read(&world, "MaxValue")),
        (0.0, 1.0)
    );
    assert_eq!(read(&world, "Value"), 1.0);

    world
        .set_property(slider, "Value", Value::Number(0.25))
        .unwrap();
    sliders.step(&mut world);
    assert_eq!(volume(&world, music), 0.25);
    world
        .set_property(music, "Volume", Value::Number(0.6))
        .unwrap();
    sliders.step(&mut world);
    assert!((read(&world, "Value") - 0.6).abs() < 1e-9);
    assert_eq!(volume(&world, music), 0.6, "a script's change isn't undone");
}

/// The pause menu's script turns the Music group's low-pass on while the
/// menu is open and off when it closes (Phase 52); `audio`'s render tests
/// hear that dull the music.
#[test]
fn a_script_turns_the_music_low_pass_on_while_the_pause_menu_is_open() {
    let (mut world, [_, music, _]) = level();
    let workspace = world.workspace();
    let pause = add(&mut world, ClassName::ScreenGui, workspace, "PauseMenu");
    world
        .set_property(pause, "Enabled", Value::Bool(false))
        .unwrap();
    let mut runtime = ScriptRuntime::new();
    run(
        &mut runtime,
        &mut world,
        r#"
        local music = game:GetService("SoundService").Master.Music
        local muffle = Instance.new("LowPassSoundEffect")
        muffle.Name = "PauseMuffle"
        muffle.Cutoff = 600
        muffle.Enabled = false
        muffle.Parent = music
        local pause = workspace.PauseMenu
        pause.Changed:Connect(function(property)
            if property == "Enabled" then
                muffle.Enabled = pause.Enabled
            end
        end)
        print(muffle:IsA("SoundEffect"), muffle.Priority)
        "#,
    );
    assert_eq!(output(&mut runtime), ["true\t0"]);
    let muffle = world.find_first_child(music, "PauseMuffle").unwrap();
    let enabled = |world: &World| world.get(muffle).unwrap().property("Enabled").cloned();
    assert_eq!(enabled(&world), Some(Value::Bool(false)));

    world
        .set_property(pause, "Enabled", Value::Bool(true))
        .unwrap();
    runtime.step(&mut world, 1.0 / 60.0);
    assert_eq!(enabled(&world), Some(Value::Bool(true)));
    let effect = axcenity_audio::mix::effects(&world, music);
    assert_eq!(effect.len(), 1);
    assert!(effect[0].enabled);

    world
        .set_property(pause, "Enabled", Value::Bool(false))
        .unwrap();
    runtime.step(&mut world, 1.0 / 60.0);
    assert_eq!(enabled(&world), Some(Value::Bool(false)));
}
