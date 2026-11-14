use super::*;

fn doc() -> SequenceDoc {
    SequenceDoc::open(sequence_path("Test"), SequenceFile::new(10.0))
}

#[test]
fn sections_are_added_at_the_playhead_moved_inside_the_duration_and_deleted() {
    let mut doc = doc();
    doc.keys.playhead = 9.0;
    let line = doc.add(Row::Subtitle, String::new(), "intro.line1");
    assert_eq!(
        doc.sections(Row::Subtitle),
        vec![(9.0, 10.0, "intro.line1".to_owned())]
    );
    // A move can't take it past either end.
    doc.set_span(line, -3.0, 4.0);
    assert_eq!(doc.sections(Row::Subtitle)[0].0, 0.0);
    doc.set_span(line, 8.0, 14.0);
    assert_eq!(doc.sections(Row::Subtitle)[0].1, 10.0);

    // Cuts stay in time order, and the selection follows the moved one.
    doc.keys.playhead = 2.0;
    doc.add(Row::Camera, "Workspace/A".into(), "");
    doc.keys.playhead = 6.0;
    let late = doc.add(Row::Camera, "Workspace/B".into(), "");
    doc.set_span(late, 1.0, 1.0);
    let cuts: Vec<&str> = doc
        .file
        .camera_cuts
        .iter()
        .map(|c| c.camera.as_str())
        .collect();
    assert_eq!(cuts, ["Workspace/B", "Workspace/A"]);
    assert_eq!(
        doc.selected,
        Some(Section {
            row: Row::Camera,
            index: 0
        })
    );
    // A cut lasts until the next one.
    assert_eq!(doc.sections(Row::Camera)[0].1, 2.0);

    doc.delete(line);
    assert!(doc.file.subtitles.is_empty());
    assert_eq!(doc.selected, None);
}

#[test]
fn a_shorter_duration_pulls_keys_events_and_sections_in() {
    let mut doc = doc();
    doc.keys.playhead = 8.0;
    doc.add(Row::Audio, "Workspace/Music".into(), "");
    doc.keys.file.events.push(ClipEvent {
        time: 9.0,
        name: "Late".into(),
    });
    doc.set_duration(5.0);
    assert_eq!(doc.sections(Row::Audio)[0].0, 5.0);
    assert_eq!(doc.keys.file.events[0].time, 5.0);
    assert_eq!(doc.keys.playhead, 5.0);
    let file = doc.current();
    assert_eq!(file.duration, 5.0);
    // What's written reads back as a valid sequence.
    assert_eq!(SequenceFile::from_json(&file.to_json()).unwrap(), file);
}

#[test]
fn keying_the_camera_makes_the_sequences_own_camera_keys() {
    let mut doc = doc();
    doc.keys.playhead = 3.0;
    let focus = camera_focus(Vector3::new(1.0, 2.0, 3.0), Vector3::new(-10.0, 45.0, 0.0));
    doc.keys.key_focus(&focus);
    let file = doc.current();
    let position = file
        .tracks
        .iter()
        .find(|t| t.target == CAMERA_TARGET && t.property == PropertyName::Position)
        .unwrap();
    assert_eq!(position.keys[0].time, 3.0);
    assert_eq!(position.keys[0].value, vec![1.0, 2.0, 3.0]);
    // With a cut to its own keys, the sequence looks from there.
    doc.keys.playhead = 0.0;
    doc.add(Row::Camera, String::new(), "");
    let mut run = SequenceRun::new(doc.current()).unwrap();
    let shot = run.apply(&mut World::new(), 3.0).camera.unwrap();
    assert_eq!(shot.position, Vector3::new(1.0, 2.0, 3.0));
    assert_eq!(shot.orientation, Vector3::new(-10.0, 45.0, 0.0));
}

#[test]
fn names_and_paths() {
    assert_eq!(
        sequence_path("Intro"),
        "assets/sequences/Intro.sequence.json"
    );
    assert_eq!(
        sequence_name("assets/sequences/Intro.sequence.json"),
        "Intro"
    );
}

#[test]
fn edit_points_are_section_ends_cuts_and_events() {
    let mut doc = doc();
    doc.keys.playhead = 2.0;
    let fade = doc.add(Row::Fade, String::new(), "");
    doc.set_span(fade, 2.0, 3.5);
    doc.keys.playhead = 4.0;
    doc.add(Row::Camera, "Workspace/A".into(), "");
    doc.keys.file.events.push(ClipEvent {
        time: 3.5,
        name: "Boom".into(),
    });
    doc.keys.file.events.push(ClipEvent {
        time: 7.25,
        name: "Done".into(),
    });
    assert_eq!(doc.edit_points(), [0.0, 2.0, 3.5, 4.0, 7.25, 10.0]);
}

#[test]
fn the_sequencer_plays_its_range_round_and_round() {
    let mut doc = doc();
    let mut mode = transport::PlayMode {
        looping: true,
        backwards: false,
        range: Some((2.0, 4.0)),
    };
    transport::apply_with(&mut doc.keys, &mut mode, &[], transport::Command::PlayPause);
    assert_eq!(doc.keys.playhead, 2.0);
    for _ in 0..30 {
        doc.tick(0.1, &mode);
    }
    assert!(doc.keys.playing);
    assert!(
        (doc.keys.playhead - 3.0).abs() < 1e-6,
        "{}",
        doc.keys.playhead
    );
}
