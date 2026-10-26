use super::*;
use crate::files::{Interp, PropertyKeyframe, PropertyName, PropertyTrack};

fn door_clip() -> PropertyClip {
    PropertyClip::from_file(ClipFile {
        format_version: 2,
        link: None,
        duration: 1.5,
        looping: false,
        tracks: vec![PropertyTrack {
            id: "pos".into(),
            target: ".".into(),
            property: PropertyName::Position,
            keys: vec![
                PropertyKeyframe {
                    id: "k0".into(),
                    time: 0.0,
                    value: vec![0.0, 0.0, 0.0],
                    interp: Interp::Linear,
                    easing: None,
                    bezier: None,
                },
                PropertyKeyframe {
                    id: "k1".into(),
                    time: 1.5,
                    value: vec![0.0, 8.0, 0.0],
                    interp: Interp::Linear,
                    easing: None,
                    bezier: None,
                },
            ],
        }],
        events: vec![ClipEvent {
            time: 1.0,
            name: "Opened".into(),
        }],
    })
    .unwrap()
}

#[test]
fn samples_before_on_between_and_at_end() {
    let clip = door_clip();
    let at = |t| {
        clip.evaluate(t)
            .into_iter()
            .find(|e| e.property == PropertyName::Position)
            .unwrap()
            .value
    };
    assert_eq!(at(-1.0), vec![0.0, 0.0, 0.0]);
    assert_eq!(at(0.0), vec![0.0, 0.0, 0.0]);
    assert_eq!(at(0.75), vec![0.0, 4.0, 0.0]);
    assert_eq!(at(1.5), vec![0.0, 8.0, 0.0]);
    assert_eq!(at(10.0), vec![0.0, 8.0, 0.0]);
}

#[test]
fn zero_duration_and_single_key() {
    let clip = PropertyClip::from_file(ClipFile {
        format_version: 2,
        link: None,
        duration: 0.0,
        looping: false,
        tracks: vec![PropertyTrack {
            id: "pos".into(),
            target: ".".into(),
            property: PropertyName::Position,
            keys: vec![PropertyKeyframe {
                id: "k0".into(),
                time: 0.0,
                value: vec![1.0, 2.0, 3.0],
                interp: Interp::Linear,
                easing: None,
                bezier: None,
            }],
        }],
        events: vec![],
    })
    .unwrap();
    assert_eq!(clip.evaluate(0.0)[0].value, vec![1.0, 2.0, 3.0]);
    assert_eq!(clip.evaluate(5.0)[0].value, vec![1.0, 2.0, 3.0]);
}

#[test]
fn looping_and_large_time_steps() {
    let mut file = door_clip().file;
    file.looping = true;
    file.duration = 1.0;
    file.tracks[0].keys[1].time = 1.0;
    file.tracks[0].keys[1].value = vec![0.0, 10.0, 0.0];
    file.events.clear();
    let clip = PropertyClip::from_file(file).unwrap();
    assert_eq!(clip.evaluate(0.5)[0].value[1], 5.0);
    assert_eq!(clip.evaluate(1.5)[0].value[1], 5.0);
    assert_eq!(clip.evaluate(2.0)[0].value[1], 10.0);
}

#[test]
fn step_holds_until_next_key() {
    let clip = PropertyClip::from_file(ClipFile {
        format_version: 2,
        link: None,
        duration: 2.0,
        looping: false,
        tracks: vec![PropertyTrack {
            id: "pos".into(),
            target: ".".into(),
            property: PropertyName::Position,
            keys: vec![
                PropertyKeyframe {
                    id: "k0".into(),
                    time: 0.0,
                    value: vec![0.0, 0.0, 0.0],
                    interp: Interp::Step,
                    easing: None,
                    bezier: None,
                },
                PropertyKeyframe {
                    id: "k1".into(),
                    time: 1.0,
                    value: vec![0.0, 10.0, 0.0],
                    interp: Interp::Linear,
                    easing: None,
                    bezier: None,
                },
            ],
        }],
        events: vec![],
    })
    .unwrap();
    assert_eq!(clip.evaluate(0.9)[0].value[1], 0.0);
    assert_eq!(clip.evaluate(1.0)[0].value[1], 10.0);
}

#[test]
fn orientation_uses_slerp() {
    let clip = PropertyClip::from_file(ClipFile {
        format_version: 2,
        link: None,
        duration: 1.0,
        looping: false,
        tracks: vec![PropertyTrack {
            id: "rot".into(),
            target: ".".into(),
            property: PropertyName::Orientation,
            keys: vec![
                PropertyKeyframe {
                    id: "k0".into(),
                    time: 0.0,
                    value: vec![0.0, 0.0, 0.0],
                    interp: Interp::Linear,
                    easing: None,
                    bezier: None,
                },
                PropertyKeyframe {
                    id: "k1".into(),
                    time: 1.0,
                    value: vec![0.0, 90.0, 0.0],
                    interp: Interp::Linear,
                    easing: None,
                    bezier: None,
                },
            ],
        }],
        events: vec![],
    })
    .unwrap();
    let mid = clip.evaluate(0.5)[0].value.clone();
    assert!((mid[1] - 45.0).abs() < 0.1, "{mid:?}");
    assert!(mid[0].abs() < 0.1 && mid[2].abs() < 0.1, "{mid:?}");
}

#[test]
fn events_fire_once_per_pass() {
    let clip = door_clip();
    // Half-open [from, to): the marker at 1.0 fires when the clock crosses it.
    assert_eq!(clip.events_between(0.0, 1.01), ["Opened"]);
    assert!(clip.events_between(1.01, 3.0).is_empty());
    assert!(clip.events_between(0.0, 1.0).is_empty());
}

#[test]
fn json_round_trip() {
    let clip = door_clip();
    let text = clip.file.to_json();
    let again = PropertyClip::from_json(&text).unwrap();
    assert_eq!(again.file, clip.file);
}

#[test]
fn rejects_key_past_duration() {
    let err = PropertyClip::from_file(ClipFile {
        format_version: 2,
        link: None,
        duration: 1.0,
        looping: false,
        tracks: vec![PropertyTrack {
            id: "pos".into(),
            target: ".".into(),
            property: PropertyName::Position,
            keys: vec![PropertyKeyframe {
                id: "k0".into(),
                time: 2.0,
                value: vec![0.0, 0.0, 0.0],
                interp: Interp::Linear,
                easing: None,
                bezier: None,
            }],
        }],
        events: vec![],
    })
    .unwrap_err();
    assert!(err.0.contains("outside"), "{err}");
}

#[test]
fn coalesce_duplicate_times() {
    let clip = PropertyClip::from_file(ClipFile {
        format_version: 2,
        link: None,
        duration: 1.0,
        looping: false,
        tracks: vec![PropertyTrack {
            id: "pos".into(),
            target: ".".into(),
            property: PropertyName::Position,
            keys: vec![
                PropertyKeyframe {
                    id: "a".into(),
                    time: 0.5,
                    value: vec![1.0, 0.0, 0.0],
                    interp: Interp::Linear,
                    easing: None,
                    bezier: None,
                },
                PropertyKeyframe {
                    id: "b".into(),
                    time: 0.5,
                    value: vec![2.0, 0.0, 0.0],
                    interp: Interp::Linear,
                    easing: None,
                    bezier: None,
                },
            ],
        }],
        events: vec![],
    })
    .unwrap();
    assert_eq!(clip.file.tracks[0].keys.len(), 1);
    assert_eq!(clip.file.tracks[0].keys[0].value[0], 2.0);
}

#[test]
fn apply_and_missing_target() {
    let mut world = World::new();
    let part = world.create(axcenity_core::ClassName::Part).unwrap();
    world.set_parent(part, Some(world.workspace())).unwrap();
    let clip = door_clip();
    let evaluated = clip.evaluate(0.75);
    let mut applied = Applied::default();
    apply_evaluated(&mut world, part, &evaluated, &mut applied);
    assert!(applied.missing.is_empty());
    assert_eq!(
        world.get(part).unwrap().property("Position"),
        Some(&Value::Vector3(Vector3::new(0.0, 4.0, 0.0)))
    );

    let bad = [EvaluatedProperty {
        target: "NoSuch".into(),
        property: PropertyName::Position,
        value: vec![0.0, 0.0, 0.0],
    }];
    apply_evaluated(&mut world, part, &bad, &mut applied);
    assert!(applied.missing.contains("NoSuch"));
}

fn part_in(world: &mut World, parent: InstanceId, name: &str, position: Vec3) -> InstanceId {
    let part = world.create(ClassName::Part).unwrap();
    world.set_name(part, name).unwrap();
    let p = Vector3::new(position.x, position.y, position.z);
    world
        .set_property(part, "Position", Value::Vector3(p))
        .unwrap();
    world
        .set_property(part, "Size", Value::Vector3(Vector3::new(1.0, 1.0, 1.0)))
        .unwrap();
    world.set_parent(part, Some(parent)).unwrap();
    part
}

fn position(world: &World, id: InstanceId) -> Vec3 {
    let p = world
        .get(id)
        .unwrap()
        .property("Position")
        .and_then(Value::as_vector3)
        .unwrap();
    Vec3::new(p.x, p.y, p.z)
}

fn one(target: &str, property: PropertyName, value: &[f32]) -> EvaluatedProperty {
    EvaluatedProperty {
        target: target.into(),
        property,
        value: value.to_vec(),
    }
}

#[test]
fn a_turning_hinge_carries_the_panel_inside_it() {
    let mut world = World::new();
    let door = world.create(ClassName::Model).unwrap();
    world.set_parent(door, Some(world.workspace())).unwrap();
    let hinge = part_in(&mut world, door, "Hinge", Vec3::ZERO);
    let panel = part_in(&mut world, hinge, "Panel", Vec3::new(2.0, 0.0, 0.0));
    let mut applied = Applied::default();
    let turn = [one("Hinge", PropertyName::Orientation, &[0.0, 90.0, 0.0])];
    apply_evaluated(&mut world, door, &turn, &mut applied);
    // Turning 90° around Y swings +X to -Z.
    assert!(position(&world, panel).abs_diff_eq(Vec3::new(0.0, 0.0, -2.0), 1e-4));
    let o = world.get(panel).unwrap().property("Orientation").cloned();
    assert_eq!(o, Some(Value::Vector3(Vector3::new(0.0, 90.0, 0.0))));
    // Back again, from where the panel first was, so nothing drifts.
    let back = [one("Hinge", PropertyName::Orientation, &[0.0, 0.0, 0.0])];
    apply_evaluated(&mut world, door, &back, &mut applied);
    assert!(position(&world, panel).abs_diff_eq(Vec3::new(2.0, 0.0, 0.0), 1e-4));
}

#[test]
fn a_model_pivot_moves_every_part_inside_it() {
    let mut world = World::new();
    let model = world.create(ClassName::Model).unwrap();
    world.set_parent(model, Some(world.workspace())).unwrap();
    let a = part_in(&mut world, model, "A", Vec3::new(-1.0, 0.0, 0.0));
    let b = part_in(&mut world, model, "B", Vec3::new(1.0, 0.0, 0.0));
    let pivot = model_pivot(&world, model).unwrap();
    assert!(pivot.position.abs_diff_eq(Vec3::ZERO, 1e-5));

    let mut applied = Applied::default();
    let moved = [
        one(".", PropertyName::Position, &[0.0, 5.0, 0.0]),
        one(".", PropertyName::Orientation, &[0.0, 180.0, 0.0]),
    ];
    apply_evaluated(&mut world, model, &moved, &mut applied);
    assert!(applied.missing.is_empty(), "{:?}", applied.missing);
    assert!(position(&world, a).abs_diff_eq(Vec3::new(1.0, 5.0, 0.0), 1e-4));
    assert!(position(&world, b).abs_diff_eq(Vec3::new(-1.0, 5.0, 0.0), 1e-4));
    let now = applied.pivot(&world, model).unwrap();
    assert!(now.position.abs_diff_eq(Vec3::new(0.0, 5.0, 0.0), 1e-5));

    // A Model has no Color of its own.
    apply_evaluated(
        &mut world,
        model,
        &[one(".", PropertyName::Color, &[1.0, 0.0, 0.0])],
        &mut applied,
    );
    assert!(applied.missing.contains(".:Color"));
}

#[test]
fn colors_numbers_and_ui_udims_apply_by_their_kind() {
    let mut world = World::new();
    let workspace = world.workspace();
    let part = part_in(&mut world, workspace, "P", Vec3::ZERO);
    let gui = world.create(ClassName::ScreenGui).unwrap();
    world.set_parent(gui, Some(world.workspace())).unwrap();
    let frame = world.create(ClassName::Frame).unwrap();
    world.set_name(frame, "Menu").unwrap();
    world.set_parent(frame, Some(gui)).unwrap();

    let mut applied = Applied::default();
    let part_values = [
        one(".", PropertyName::Color, &[1.0, 0.5, 0.0]),
        one(".", PropertyName::Transparency, &[0.25]),
    ];
    apply_evaluated(&mut world, part, &part_values, &mut applied);
    let ui_values = [
        one("Menu", PropertyName::Position, &[0.5, -100.0, 0.0, 20.0]),
        one("Menu", PropertyName::BackgroundTransparency, &[0.5]),
    ];
    apply_evaluated(&mut world, gui, &ui_values, &mut applied);
    assert!(applied.missing.is_empty(), "{:?}", applied.missing);
    let part = world.get(part).unwrap();
    assert_eq!(
        part.property("Color"),
        Some(&Value::Color3(Color3::new(1.0, 0.5, 0.0)))
    );
    assert_eq!(part.property("Transparency"), Some(&Value::Number(0.25)));
    let frame = world.get(frame).unwrap();
    assert_eq!(
        frame.property("Position"),
        Some(&Value::UDim2(UDim2::new(0.5, -100.0, 0.0, 20.0)))
    );

    // Three numbers don't fit a UDim2.
    let wrong = [one("Menu", PropertyName::Size, &[1.0, 2.0, 3.0])];
    apply_evaluated(&mut world, gui, &wrong, &mut applied);
    assert!(applied.missing.contains("Menu:Size"));
}

#[test]
fn keys_must_fit_their_property() {
    let mut file = door_clip().file;
    file.tracks[0].property = PropertyName::Transparency;
    let err = PropertyClip::from_file(file).unwrap_err();
    assert!(err.0.contains("takes 1"), "{err}");

    let mut file = door_clip().file;
    file.tracks[0].keys[1].value = vec![0.0, 0.5, 0.0, 10.0];
    let err = PropertyClip::from_file(file).unwrap_err();
    assert!(err.0.contains("mixes"), "{err}");
}

#[test]
fn older_position_only_clips_still_read_the_same() {
    let text = r#"{
  "format_version": 1,
  "kind": "property",
  "duration": 1.5,
  "loop": false,
  "tracks": [
    { "id": "position", "target": ".", "property": "Position",
      "keys": [ { "id": "k1", "time": 0.0, "value": [0, 2, 0], "interp": "Linear" },
                { "id": "k2", "time": 1.5, "value": [0, 8, 0], "interp": "Linear" } ] }
  ],
  "events": [ { "time": 1.4, "name": "Opened" } ]
}"#;
    let clip = PropertyClip::from_json(text).unwrap();
    assert_eq!(clip.evaluate(0.75)[0].value, vec![0.0, 5.0, 0.0]);
}

#[test]
fn target_paths_go_from_the_root() {
    let mut world = World::new();
    let door = world.create(ClassName::Model).unwrap();
    world.set_parent(door, Some(world.workspace())).unwrap();
    let hinge = part_in(&mut world, door, "Hinge", Vec3::ZERO);
    let panel = part_in(&mut world, hinge, "Panel", Vec3::ZERO);
    assert_eq!(target_path(&world, door, door).as_deref(), Some("."));
    let path = target_path(&world, door, panel).unwrap();
    assert_eq!(path, "Hinge/Panel");
    assert_eq!(resolve_target(&world, door, &path), Some(panel));
    assert_eq!(target_path(&world, hinge, door), None);
}
