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
        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                doc.keys(ui, None);
            });
        });
    }
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
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
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
