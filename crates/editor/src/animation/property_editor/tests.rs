use super::*;

fn value(target: &str, property: PropertyName, value: &[f32]) -> EvaluatedProperty {
    EvaluatedProperty {
        target: target.into(),
        property,
        value: value.to_vec(),
    }
}

fn doc() -> PropertyDoc {
    PropertyDoc::open(
        "assets/animations/Door.anim.json".into(),
        ClipFile::new(2.0),
    )
}

#[test]
fn recording_a_move_makes_a_track_and_a_key_at_the_playhead() {
    let mut doc = doc();
    doc.playhead = 1.0;
    let turn = value("Hinge", PropertyName::Orientation, &[0.0, 90.0, 0.0]);
    doc.edit(vec![turn.clone()], true);
    assert_eq!(doc.file.tracks.len(), 1);
    let track = &doc.file.tracks[0];
    assert_eq!(
        (track.target.as_str(), track.property),
        ("Hinge", PropertyName::Orientation)
    );
    assert_eq!(track.keys[0].time, 1.0);
    assert_eq!(doc.selected.len(), 1);
    // Moving it again at the same time updates that key.
    doc.edit(
        vec![value("Hinge", PropertyName::Orientation, &[0.0, 45.0, 0.0])],
        true,
    );
    assert_eq!(doc.file.tracks[0].keys.len(), 1);
    assert_eq!(doc.file.tracks[0].keys[0].value, [0.0, 45.0, 0.0]);
}

#[test]
fn a_move_without_record_shows_until_the_playhead_moves_or_k() {
    let mut doc = doc();
    doc.edit(vec![value(".", PropertyName::Transparency, &[0.5])], false);
    assert!(doc.file.tracks.is_empty());
    let shown = doc.evaluated().unwrap();
    assert_eq!(shown[0].value, [0.5]);

    // K keys it (with where the object is).
    let focus = Focus {
        target: ".".into(),
        name: "Door".into(),
        values: vec![
            value(".", PropertyName::Position, &[0.0, 1.0, 0.0]),
            value(".", PropertyName::Orientation, &[0.0, 0.0, 0.0]),
            value(".", PropertyName::Transparency, &[0.0]),
        ],
        transform: vec![PropertyName::Position, PropertyName::Orientation],
    };
    doc.key_focus(&focus);
    let keyed: Vec<PropertyName> = doc.file.tracks.iter().map(|t| t.property).collect();
    assert_eq!(
        keyed,
        [
            PropertyName::Position,
            PropertyName::Orientation,
            PropertyName::Transparency
        ]
    );
    assert_eq!(doc.file.tracks[2].keys[0].value, [0.5]);

    doc.edit(vec![value(".", PropertyName::Transparency, &[0.9])], false);
    doc.playhead = 1.0;
    let shown = doc.evaluated().unwrap();
    let transparency = shown
        .iter()
        .find(|v| v.property == PropertyName::Transparency)
        .unwrap();
    assert_eq!(transparency.value, [0.5]);
}

#[test]
fn copy_paste_delete_and_snapped_keys() {
    let mut doc = doc();
    doc.playhead = 0.51;
    doc.key_values(&[value(".", PropertyName::Position, &[0.0, 0.0, 0.0])]);
    // 0.51 s snaps to frame 15 at 30 fps.
    assert_eq!(doc.file.tracks[0].keys[0].time, 0.5);
    doc.copy();
    doc.playhead = 1.0;
    doc.paste();
    assert_eq!(doc.file.tracks[0].keys.len(), 2);
    assert_eq!(doc.file.tracks[0].keys[1].time, 1.0);
    assert_eq!(doc.selected.len(), 1);
    doc.delete_selected();
    assert_eq!(doc.file.tracks[0].keys.len(), 1);
}

#[test]
fn a_moved_key_landing_on_another_replaces_it() {
    let mut doc = doc();
    for (time, y) in [(0.0, 0.0), (1.0, 5.0)] {
        doc.playhead = time;
        doc.key_values(&[value(".", PropertyName::Position, &[0.0, y, 0.0])]);
    }
    // The key at 1 s is selected; drag it onto the one at 0 s.
    doc.file.tracks[0].keys[1].time = 0.0;
    doc.tidy();
    let keys = &doc.file.tracks[0].keys;
    assert_eq!(keys.len(), 1);
    assert_eq!(keys[0].value, [0.0, 5.0, 0.0]);
    assert_eq!(doc.selected.len(), 1);
}

#[test]
fn ui_keys_stay_off_vector_tracks() {
    let mut doc = doc();
    doc.key_values(&[value(
        "Menu",
        PropertyName::Position,
        &[0.0, 10.0, 0.0, 20.0],
    )]);
    doc.key_values(&[value("Menu", PropertyName::Position, &[1.0, 2.0, 3.0])]);
    assert_eq!(doc.file.tracks[0].keys[0].value.len(), 4);
    assert!(doc.clip().is_ok());
}

/// A doc of a clip raising the root from y 0 to y 10 over a second.
fn rising() -> PropertyDoc {
    let mut doc = doc();
    doc.file.duration = 1.0;
    for (time, y) in [(0.0, 0.0), (1.0, 10.0)] {
        doc.playhead = time;
        doc.key_values(&[value(".", PropertyName::Position, &[0.0, y, 0.0])]);
    }
    doc
}

fn y_at(doc: &mut PropertyDoc, time: f64) -> f32 {
    doc.playhead = time;
    doc.evaluated().unwrap()[0].value[1]
}

#[test]
fn a_keys_easing_changes_the_preview_and_plays_the_same_in_the_game() {
    let mut doc = rising();
    assert!((y_at(&mut doc, 0.5) - 5.0).abs() < 1e-4);
    let key = &mut doc.file.tracks[0].keys[0];
    key.interp = Interp::Ease;
    key.easing = Some(Easing::new(EasingStyle::Back, EasingDirection::In));
    let shown = y_at(&mut doc, 0.25);
    assert!(shown < 0.0, "Back In pulls back first: {shown}");

    // The game plays the saved clip the same.
    let folder = std::env::temp_dir().join(format!(
        "axcenity-eased-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(folder.join("assets/animations")).unwrap();
    doc.write(&folder).unwrap();
    let assets = axcenity_assets::Assets::new(&folder);
    let mut world = axcenity_core::World::new();
    let part = world.create(axcenity_core::ClassName::Part).unwrap();
    world.set_parent(part, Some(world.workspace())).unwrap();
    let animation = world.create(axcenity_core::ClassName::Animation).unwrap();
    world.set_parent(animation, Some(part)).unwrap();
    world
        .set_property(
            animation,
            "Clip",
            axcenity_core::Value::String(doc.path.clone()),
        )
        .unwrap();
    world.property_animation_mut(animation).unwrap().play = true;
    let mut players = axcenity_anim::Animators::new();
    players.update(&mut world, &assets, 0.0);
    players.update(&mut world, &assets, 0.25);
    assert!(players.take_errors().is_empty());
    let played = world
        .get(part)
        .and_then(|p| p.property("Position"))
        .and_then(axcenity_core::Value::as_vector3)
        .unwrap();
    assert!((played.y - shown).abs() < 1e-4, "{} vs {shown}", played.y);
    std::fs::remove_dir_all(folder).unwrap();
}

/// Dragging with the left button from `from` to `to`, then letting go.
fn drag(from: egui::Pos2, to: egui::Pos2) -> Vec<Vec<egui::Event>> {
    let button = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    let mut frames = vec![
        vec![egui::Event::PointerMoved(from)],
        vec![button(from, true)],
    ];
    for step in 1..=4 {
        frames.push(vec![egui::Event::PointerMoved(
            from.lerp(to, step as f32 / 4.0),
        )]);
    }
    frames.push(vec![button(to, false)]);
    frames.push(Vec::new());
    frames
}

/// Runs the dope sheet's area (toolbar and Curves view) for `frames`.
fn run_curves(doc: &mut PropertyDoc, frames: Vec<Vec<egui::Event>>) {
    let ctx = theme::test_context();
    for events in frames {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(900.0, 360.0),
            )),
            events,
            ..Default::default()
        };
        let _ = crate::theme::run(&ctx, input, |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                doc.keys(ui, None);
            });
        });
    }
}

/// Where the dope sheet drew the key at `time` on its first row.
fn sheet_key(doc: &PropertyDoc, time: f64) -> egui::Pos2 {
    let rect = doc.sheet;
    let label_w = (rect.width() * 0.3).clamp(70.0, 160.0);
    let (left, right) = (rect.left() + label_w + 6.0, rect.right() - 8.0);
    let x = left + (time / doc.file.duration) as f32 * (right - left);
    egui::pos2(x, rect.top() + RULER + ROW / 2.0)
}

/// A key press with no modifiers, then letting go.
fn press(key: egui::Key) -> Vec<Vec<egui::Event>> {
    let event = |pressed| egui::Event::Key {
        key,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers: Default::default(),
    };
    vec![vec![event(true)], vec![event(false)]]
}

#[test]
fn read_only_keys_can_be_selected_but_never_moved_deleted_or_changed() {
    let mut doc = rising();
    doc.read_only = true;
    doc.selected.clear();
    let track = doc.file.tracks[0].id.clone();
    let last = (track.clone(), doc.file.tracks[0].keys[1].id.clone());
    doc.file.tracks[0].keys[0].interp = Interp::Bezier;
    let shown_bezier = doc.file.clone();

    // In the dope sheet, dragging the last key selects it and leaves it be.
    run_curves(&mut doc, vec![Vec::new(), Vec::new()]);
    assert!(doc.sheet.width() > 100.0, "the dope sheet is drawn");
    let at = sheet_key(&doc, 1.0);
    run_curves(&mut doc, drag(at, at - egui::vec2(200.0, 0.0)));
    assert_eq!(doc.selected, BTreeSet::from([last.clone()]));
    assert_eq!(doc.file, shown_bezier, "nothing moved in the dope sheet");

    // In Curves, dragging the key or its Bezier handles moves nothing.
    doc.show_curves = true;
    run_curves(&mut doc, vec![Vec::new(), Vec::new()]);
    let key = doc.curve_view.screen(1.0, 10.0);
    run_curves(&mut doc, drag(key, key - egui::vec2(120.0, -60.0)));
    assert_eq!(doc.file, shown_bezier, "nothing moved in Curves");
    assert!(doc.selected.contains(&last));
    let handle = doc.curve_view.screen(0.58, 10.0);
    run_curves(&mut doc, drag(handle, handle - egui::vec2(80.0, 40.0)));
    assert_eq!(doc.file, shown_bezier, "the handles stay");

    // Delete, cut, paste, keying and preview moves change nothing either.
    doc.show_curves = false;
    run_curves(&mut doc, press(egui::Key::Delete));
    let ctx = theme::test_context();
    let _ = crate::theme::run(&ctx, Default::default(), |ui| {
        egui::CentralPanel::default().show(ui, |ui| {
            assert!(!doc.shortcuts(ui));
        });
    });
    doc.copy();
    doc.playhead = 0.5;
    doc.paste();
    doc.delete_selected();
    doc.delete_track(&track);
    doc.key_values(&[value(".", PropertyName::Position, &[1.0, 2.0, 3.0])]);
    doc.edit(
        vec![value(".", PropertyName::Position, &[4.0, 5.0, 6.0])],
        true,
    );
    assert_eq!(doc.file, shown_bezier);
    assert!(doc.selected.contains(&last), "still selected");
}

#[test]
fn bezier_handles_dragged_in_the_curves_view_are_saved_and_reloaded() {
    let mut doc = rising();
    let first = doc.file.tracks[0].keys[0].id.clone();
    let track = doc.file.tracks[0].id.clone();
    doc.file.tracks[0].keys[0].interp = Interp::Bezier;
    doc.selected = BTreeSet::from([(track, first)]);
    doc.show_curves = true;
    run_curves(&mut doc, vec![Vec::new(), Vec::new()]);
    assert!(doc.curve_view.rect.width() > 100.0, "the curves are drawn");

    // Drag the handle by the last key (Y's, the one number that moves).
    let from = doc.curve_view.screen(0.58, 10.0);
    let to = doc.curve_view.screen(0.3, 12.0);
    run_curves(&mut doc, drag(from, to));
    let handles = doc.file.tracks[0].keys[0].bezier.expect("handles are set");
    assert!((handles[0] - 0.42).abs() < 1e-3, "{handles:?}");
    assert!((handles[1] - 0.0).abs() < 1e-3, "{handles:?}");
    assert!((handles[2] - 0.3).abs() < 0.02, "{handles:?}");
    assert!((handles[3] - 1.2).abs() < 0.02, "{handles:?}");
    // It overshoots the last key on the way.
    assert!(y_at(&mut doc, 0.8) > 10.0);
    if let Some(snapshots) = std::env::var_os("AXCENITY_SNAPSHOTS") {
        let mut shown = PropertyDoc::open(doc.path.clone(), doc.file.clone());
        shown.selected = doc.selected.clone();
        shown.show_curves = true;
        crate::snapshot::snapshot(
            &std::path::PathBuf::from(snapshots).join("curves-bezier.png"),
            egui::vec2(900.0, 360.0),
            vec![Vec::new(); 3],
            |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    shown.keys(ui, None);
                });
            },
        );
    }

    let folder = std::env::temp_dir().join(format!(
        "axcenity-bezier-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(folder.join("assets/animations")).unwrap();
    doc.write(&folder).unwrap();
    let text = std::fs::read_to_string(folder.join(&doc.path)).unwrap();
    let read = ClipFile::from_json(&text).unwrap();
    assert_eq!(read.tracks[0].keys[0].bezier, Some(handles));
    assert_eq!(read.tracks[0].keys[0].interp, Interp::Bezier);
    let mut reopened = PropertyDoc::open(doc.path.clone(), read);
    assert_eq!(y_at(&mut reopened, 0.8), y_at(&mut doc, 0.8));
    std::fs::remove_dir_all(folder).unwrap();
}

/// The interpolation and easing of each key of the track on `target`.
fn easings(doc: &PropertyDoc, target: &str) -> Vec<(Interp, Option<Easing>)> {
    doc.file
        .tracks
        .iter()
        .find(|t| t.target == target)
        .map(|t| t.keys.iter().map(|k| (k.interp, k.easing)).collect())
        .unwrap_or_default()
}

#[test]
fn record_k_and_add_track_make_keys_with_the_new_keys_easing_and_only_new_keys_change() {
    let mut doc = doc();
    assert_eq!(doc.new_keys, NewKeys::Smooth, "Smooth to start with");
    let smooth = NewKeys::Smooth.easing();
    assert_eq!(
        smooth,
        (
            Interp::Ease,
            Some(Easing::new(EasingStyle::Sine, EasingDirection::InOut))
        )
    );
    // Record: a move in the preview.
    doc.edit(
        vec![value("Hinge", PropertyName::Orientation, &[0.0, 0.0, 0.0])],
        true,
    );
    // K: Key selected.
    doc.playhead = 0.5;
    let focus = Focus {
        target: "Hinge".into(),
        name: "Hinge".into(),
        values: vec![
            value("Hinge", PropertyName::Orientation, &[0.0, 45.0, 0.0]),
            value("Hinge", PropertyName::Transparency, &[0.0]),
        ],
        transform: vec![PropertyName::Orientation],
    };
    doc.key_focus(&focus);
    // Add track: what its menu does with the property picked.
    doc.key_values(&[value("Hinge", PropertyName::Transparency, &[0.2])]);
    assert_eq!(easings(&doc, "Hinge").len(), 2);
    for track in &doc.file.tracks {
        for key in &track.keys {
            assert_eq!((key.interp, key.easing), smooth, "{}", track.id);
        }
    }

    // Constant from now on: new keys are Constant; the Smooth ones stay,
    // even when a key at the same time gets a new value.
    doc.new_keys = NewKeys::Constant;
    doc.playhead = 0.5;
    doc.edit(
        vec![value("Hinge", PropertyName::Orientation, &[0.0, 60.0, 0.0])],
        true,
    );
    doc.playhead = 1.0;
    doc.edit(
        vec![value("Hinge", PropertyName::Orientation, &[0.0, 90.0, 0.0])],
        true,
    );
    let orientation = doc
        .file
        .tracks
        .iter()
        .find(|t| t.property == PropertyName::Orientation)
        .unwrap();
    assert_eq!(orientation.keys[1].value, [0.0, 60.0, 0.0], "re-keyed");
    let kinds: Vec<Interp> = orientation.keys.iter().map(|k| k.interp).collect();
    assert_eq!(kinds, [Interp::Ease, Interp::Ease, Interp::Step]);
    doc.new_keys = NewKeys::Linear;
    doc.playhead = 1.5;
    doc.key_values(&[value("Hinge", PropertyName::Orientation, &[0.0, 0.0, 0.0])]);
    let orientation = doc
        .file
        .tracks
        .iter()
        .find(|t| t.property == PropertyName::Orientation);
    let last = orientation.unwrap().keys.last().unwrap();
    assert_eq!((last.interp, last.easing), (Interp::Linear, None));
}

#[test]
fn easing_sets_every_selected_key_and_leaves_the_rest_and_saved_clips_be() {
    let mut doc = rising();
    doc.key_values(&[value("Lid", PropertyName::Transparency, &[1.0])]);
    doc.playhead = 0.0;
    doc.key_values(&[value("Lid", PropertyName::Transparency, &[0.0])]);
    let before = doc.file.clone();
    let rise = doc.file.tracks[0].id.clone();
    let lid = doc.file.tracks[1].id.clone();
    // Both of the rise's keys and the lid's first.
    doc.selected = BTreeSet::from([
        (rise.clone(), doc.file.tracks[0].keys[0].id.clone()),
        (rise.clone(), doc.file.tracks[0].keys[1].id.clone()),
        (lid.clone(), doc.file.tracks[1].keys[0].id.clone()),
    ]);
    doc.ease_selected(Interp::Linear, None);
    assert_eq!(easings(&doc, ".").len(), 2);
    assert!(
        easings(&doc, ".")
            .iter()
            .all(|e| *e == (Interp::Linear, None))
    );
    let lid_keys = easings(&doc, "Lid");
    assert_eq!(lid_keys[0], (Interp::Linear, None));
    assert_eq!(lid_keys[1], NewKeys::Smooth.easing(), "not selected: kept");
    // Times and values stay.
    for (track, was) in doc.file.tracks.iter().zip(&before.tracks) {
        for (key, was) in track.keys.iter().zip(&was.keys) {
            assert_eq!((key.time, &key.value), (was.time, &was.value));
        }
    }
    let back = Easing::new(EasingStyle::Back, EasingDirection::Out);
    doc.ease_selected(Interp::Ease, Some(back));
    assert!(
        easings(&doc, ".")
            .iter()
            .all(|e| *e == (Interp::Ease, Some(back)))
    );

    // Right-clicking a key that isn't selected selects it alone, for the
    // menu; one that is keeps the selection.
    run_curves(&mut doc, vec![Vec::new(), Vec::new()]);
    let right_click = |at: egui::Pos2| {
        let button = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Secondary,
            pressed,
            modifiers: Default::default(),
        };
        vec![
            vec![egui::Event::PointerMoved(at)],
            vec![button(true)],
            vec![button(false)],
            Vec::new(),
        ]
    };
    let three = doc.selected.clone();
    let selected_key = sheet_key(&doc, 1.0);
    run_curves(&mut doc, right_click(selected_key));
    assert_eq!(doc.selected, three, "a selected key keeps the selection");
    let lid_last = (lid.clone(), doc.file.tracks[1].keys[1].id.clone());
    let at = sheet_key(&doc, 1.0) + egui::vec2(0.0, ROW);
    run_curves(&mut doc, right_click(at));
    assert_eq!(doc.selected, BTreeSet::from([lid_last]));

    // A clip saved before opens with its keys as they were, and isn't
    // written again.
    let example = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/hello-door/assets/animations/Door/Door.anim.json");
    let text = std::fs::read_to_string(example).unwrap();
    let saved = ClipFile::from_json(&text).unwrap();
    let mut door = PropertyDoc::open("assets/animations/Door/Door.anim.json".into(), saved);
    assert!(!door.unwritten());
    assert!(
        door.file.tracks[0]
            .keys
            .iter()
            .all(|k| (k.interp, k.easing) == (Interp::Linear, None))
    );
    let mid = {
        door.playhead = 0.75;
        door.evaluated().unwrap()[0].value[1]
    };
    assert!((mid - 5.0).abs() < 1e-4, "still linear: {mid}");
    assert_eq!(door.file, ClipFile::from_json(&text).unwrap());
    assert!(!door.unwritten(), "nothing to write");
}

#[test]
fn a_bone_key_shows_and_edits_as_its_turn_in_degrees_and_where_it_is() {
    let turn = Quat::from_euler(EulerRot::YXZ, 0.3, 0.2, 0.1);
    let mut numbers = vec![
        1.0, 55.0, 2.0, turn.x, turn.y, turn.z, turn.w, 1.0, 1.0, 1.0,
    ];
    let shown = shown_numbers(PropertyName::Bone, &numbers);
    let expected = [0.2_f32, 0.3, 0.1].map(f32::to_degrees);
    for (got, want) in shown.iter().zip(expected) {
        assert!((got - want).abs() < 1e-3, "{shown:?}");
    }
    assert_eq!(shown[3..], [1.0, 55.0, 2.0]);

    // Setting Turn Y turns only about Y, and keeps the quaternion whole,
    // the scale and where it is.
    set_shown_number(PropertyName::Bone, &mut numbers, 1, 40.0);
    let shown = shown_numbers(PropertyName::Bone, &numbers);
    assert!((shown[1] - 40.0).abs() < 1e-3, "{shown:?}");
    assert!((shown[0] - expected[0]).abs() < 1e-3, "{shown:?}");
    assert!((shown[2] - expected[2]).abs() < 1e-3, "{shown:?}");
    let length = Quat::from_slice(&numbers[3..7]).length();
    assert!((length - 1.0).abs() < 1e-5, "{length}");
    assert_eq!(numbers[7..], [1.0, 1.0, 1.0]);
    set_shown_number(PropertyName::Bone, &mut numbers, 4, 50.0);
    assert_eq!(numbers[..3], [1.0, 50.0, 2.0]);
}

#[test]
fn dragging_a_bone_curve_only_turns_the_bone() {
    let mut doc = doc();
    doc.file.duration = 1.0;
    let still = |turn: Quat| {
        vec![
            0.0, 55.0, 0.0, turn.x, turn.y, turn.z, turn.w, 1.0, 1.0, 1.0,
        ]
    };
    for (time, turn) in [
        (0.0, Quat::IDENTITY),
        (1.0, Quat::from_rotation_z(40_f32.to_radians())),
    ] {
        doc.playhead = time;
        doc.key_values(&[value("Root/Arm", PropertyName::Bone, &still(turn))]);
    }
    doc.selected.clear();
    doc.show_curves = true;
    run_curves(&mut doc, vec![Vec::new(), Vec::new()]);
    assert!(doc.curve_view.rect.width() > 100.0, "the curves are drawn");
    let (low, high) = doc.curve_view.value;
    assert!(
        low < 0.0 && high > 40.0 && high < 60.0,
        "framed in degrees: {low}..{high}"
    );

    // Drag the last key's Turn Z from 40° down to 20°.
    let from = doc.curve_view.screen(1.0, 40.0);
    let to = doc.curve_view.screen(1.0, 20.0);
    run_curves(&mut doc, drag(from, to));
    let key = &doc.file.tracks[0].keys[1];
    assert_eq!(key.time, 1.0);
    let shown = shown_numbers(PropertyName::Bone, &key.value);
    assert!((shown[2] - 20.0).abs() < 1.0, "{shown:?}");
    assert!(shown[0].abs() < 1e-3 && shown[1].abs() < 1e-3, "{shown:?}");
    let length = Quat::from_slice(&key.value[3..7]).length();
    assert!((length - 1.0).abs() < 1e-5, "{length}");
    assert_eq!(key.value[..3], [0.0, 55.0, 0.0], "it stays where it was");
    assert_eq!(key.value[7..], [1.0, 1.0, 1.0], "and its size");
}
