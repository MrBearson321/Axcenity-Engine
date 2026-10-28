use axcenity_core::{EnumItem, Vector2};

use super::*;

fn add(world: &mut World, class: ClassName, name: &str, parent: InstanceId) -> InstanceId {
    let id = world.create(class).unwrap();
    world.set_name(id, name).unwrap();
    world.set_parent(id, Some(parent)).unwrap();
    id
}

/// Master, with Music and SFX inside it, in SoundService.
fn master_music_sfx(world: &mut World) -> [InstanceId; 3] {
    let service = world.sound_service();
    let master = add(world, ClassName::SoundGroup, "Master", service);
    let music = add(world, ClassName::SoundGroup, "Music", master);
    let sfx = add(world, ClassName::SoundGroup, "SFX", master);
    [master, music, sfx]
}

#[test]
fn groups_inside_groups_are_sub_buses_listed_parents_first() {
    let mut world = World::new();
    let [master, music, sfx] = master_music_sfx(&mut world);
    world
        .set_property(music, "Volume", Value::Number(0.25))
        .unwrap();
    world
        .set_property(sfx, "Volume", Value::Number(-2.0))
        .unwrap();
    let groups = groups(&world);
    let ids: Vec<_> = groups.iter().map(|g| g.id).collect();
    assert_eq!(ids, [master, music, sfx]);
    assert_eq!(groups[0].parent, None);
    assert_eq!(groups[1].parent, Some(master));
    assert_eq!(groups[1].volume, 0.25);
    assert_eq!(groups[2].volume, 0.0, "never below silent");
}

#[test]
fn effects_go_in_priority_order_then_child_order() {
    let mut world = World::new();
    let [_, music, _] = master_music_sfx(&mut world);
    let echo = add(&mut world, ClassName::EchoSoundEffect, "Echo", music);
    let low = add(&mut world, ClassName::LowPassSoundEffect, "Low", music);
    let reverb = add(&mut world, ClassName::ReverbSoundEffect, "Reverb", music);
    world
        .set_property(reverb, "Priority", Value::Number(5.0))
        .unwrap();
    add(&mut world, ClassName::Folder, "NotAnEffect", music);
    let order: Vec<_> = effects(&world, music).iter().map(|e| e.id).collect();
    assert_eq!(order, [reverb, echo, low]);

    world
        .set_property(low, "Cutoff", Value::Number(1e9))
        .unwrap();
    world
        .set_property(low, "Enabled", Value::Bool(false))
        .unwrap();
    let low = Effect::of(&world, low).unwrap();
    assert!(!low.enabled);
    assert_eq!(
        low.settings,
        EffectSettings::LowPass {
            cutoff: 20_000.0,
            resonance: 0.0
        }
    );
}

#[test]
fn an_effect_keeps_its_shape_while_its_settings_change() {
    let mut world = World::new();
    let [_, music, _] = master_music_sfx(&mut world);
    let low = add(&mut world, ClassName::LowPassSoundEffect, "Low", music);
    let echo = add(&mut world, ClassName::EchoSoundEffect, "Echo", music);
    let before = |world: &World| {
        effects(world, music)
            .iter()
            .map(Effect::shape)
            .collect::<Vec<_>>()
    };
    let shape = before(&world);
    world
        .set_property(low, "Cutoff", Value::Number(300.0))
        .unwrap();
    world
        .set_property(low, "Enabled", Value::Bool(false))
        .unwrap();
    assert_eq!(before(&world), shape, "settings move the built effect");
    world
        .set_property(echo, "Delay", Value::Number(0.25))
        .unwrap();
    assert_ne!(before(&world), shape, "Kira fixes an echo's delay");
}

#[test]
fn an_equalizer_keeps_its_mids_in_order() {
    let mut world = World::new();
    let [_, music, _] = master_music_sfx(&mut world);
    let eq = add(&mut world, ClassName::EqualizerSoundEffect, "Eq", music);
    world
        .set_property(eq, "MidRange", Value::Vector2(Vector2::new(5000.0, 300.0)))
        .unwrap();
    let EffectSettings::Equalizer {
        mid_low, mid_high, ..
    } = Effect::of(&world, eq).unwrap().settings
    else {
        panic!("an equalizer");
    };
    assert_eq!((mid_low, mid_high), (300.0, 5000.0));
}

#[test]
fn a_sound_plays_through_its_group_only_while_that_is_a_group_in_the_game() {
    let mut world = World::new();
    let [_, music, _] = master_music_sfx(&mut world);
    let workspace = world.workspace();
    let sound = add(&mut world, ClassName::Sound, "Theme", workspace);
    world
        .set_property(sound, "SoundGroup", Value::Instance(Some(music)))
        .unwrap();
    assert_eq!(sound_group(&world, world.get(sound).unwrap()), Some(music));
    world.set_parent(music, None).unwrap();
    assert_eq!(sound_group(&world, world.get(sound).unwrap()), None);
}

#[test]
fn mute_silences_a_group_and_solo_keeps_only_its_line() {
    let mut world = World::new();
    let [master, music, sfx] = master_music_sfx(&mut world);
    let groups = groups(&world);
    let mut monitor = Monitor::default();
    for id in [master, music, sfx] {
        assert_eq!(monitor.group_gain(&groups, id), 1.0);
    }

    monitor.muted.insert(sfx);
    assert_eq!(monitor.group_gain(&groups, sfx), 0.0);
    assert_eq!(monitor.group_gain(&groups, music), 1.0);

    monitor.muted.clear();
    monitor.soloed.insert(music);
    assert_eq!(monitor.group_gain(&groups, music), 1.0);
    assert_eq!(
        monitor.group_gain(&groups, master),
        1.0,
        "Music goes out through it"
    );
    assert_eq!(monitor.group_gain(&groups, sfx), 0.0);
    assert_eq!(monitor.sound_gain(&groups, Some(music)), 1.0);
    assert_eq!(monitor.sound_gain(&groups, Some(master)), 0.0);
    assert_eq!(monitor.sound_gain(&groups, None), 0.0);
}

#[test]
fn the_listener_is_in_the_smallest_zone_around_it() {
    let mut world = World::new();
    let workspace = world.workspace();
    let cave = add(&mut world, ClassName::ReverbZone, "Cave", workspace);
    world
        .set_property(cave, "Size", Value::Vector3(Vector3::new(40.0, 10.0, 10.0)))
        .unwrap();
    world
        .set_property(
            cave,
            "ReverbType",
            Value::EnumItem(EnumItem {
                enum_name: "ReverbType".to_owned(),
                item: "Cave".to_owned(),
            }),
        )
        .unwrap();
    world
        .set_property(cave, "FadeTime", Value::Number(2.0))
        .unwrap();
    let inside = zone_at(&world, Vector3::new(15.0, 0.0, 0.0)).unwrap();
    assert_eq!(inside.id, cave);
    assert_eq!(inside.reverb, Reverb::preset("Cave"));
    assert_eq!(inside.fade_time, 2.0);
    assert!(zone_at(&world, Vector3::new(0.0, 0.0, 15.0)).is_none());

    // Turned a quarter about Y, its length runs along Z instead.
    world
        .set_property(
            cave,
            "Orientation",
            Value::Vector3(Vector3::new(0.0, 90.0, 0.0)),
        )
        .unwrap();
    assert!(zone_at(&world, Vector3::new(0.0, 0.0, 15.0)).is_some());
    assert!(zone_at(&world, Vector3::new(15.0, 0.0, 0.0)).is_none());

    let room = add(&mut world, ClassName::ReverbZone, "Room", workspace);
    world
        .set_property(room, "Size", Value::Vector3(Vector3::new(4.0, 4.0, 4.0)))
        .unwrap();
    assert_eq!(
        zone_at(&world, Vector3::new(0.0, 0.0, 1.0)).unwrap().id,
        room
    );
    assert_eq!(
        zone_at(&world, Vector3::new(0.0, 0.0, 9.0)).unwrap().id,
        cave
    );
}

#[test]
fn every_reverb_type_is_a_preset_of_its_own() {
    let presets: Vec<_> = REVERB_TYPES
        .iter()
        .map(|name| Reverb::preset(name))
        .collect();
    for (i, a) in presets.iter().enumerate() {
        for b in &presets[i + 1..] {
            assert_ne!(a, b);
        }
    }
}
