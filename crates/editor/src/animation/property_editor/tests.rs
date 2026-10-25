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
