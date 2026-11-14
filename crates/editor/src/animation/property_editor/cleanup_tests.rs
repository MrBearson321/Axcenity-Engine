use std::sync::Arc;

use axcenity_anim::{ClipFile, PropertyKeyframe, PropertyName, PropertyTrack, bone_numbers};
use axcenity_assets::{Bone, BoneTransform, Rig};
use axcenity_render::glam::{Mat4, Quat, Vec3};

use super::*;
use crate::theme;

fn track(
    id: &str,
    target: &str,
    property: PropertyName,
    keys: Vec<(f64, Vec<f32>)>,
) -> PropertyTrack {
    PropertyTrack {
        id: id.into(),
        target: target.into(),
        property,
        keys: keys
            .into_iter()
            .enumerate()
            .map(|(n, (time, value))| PropertyKeyframe::new(format!("k{}", n + 1), time, value))
            .collect(),
    }
}

/// A door clip: a fade with a key that changes nothing, and a slide.
fn doc() -> PropertyDoc {
    let mut file = ClipFile::new(2.0);
    file.looping = true;
    file.tracks.push(track(
        "fade",
        ".",
        PropertyName::Transparency,
        vec![(0.0, vec![0.0]), (1.0, vec![0.5]), (2.0, vec![1.0])],
    ));
    file.tracks.push(track(
        "slide",
        ".",
        PropertyName::Position,
        vec![(0.0, vec![0.0, 0.0, 0.0]), (1.0, vec![0.0, 2.0, 0.0])],
    ));
    PropertyDoc::open("assets/animations/Door.anim.json".into(), file)
}

#[test]
fn the_tools_change_the_selected_keys_tracks_or_every_track() {
    let mut doc = doc();
    assert_eq!(doc.cleanup_tracks(), None);
    doc.selected.insert(("slide".into(), "k2".into()));
    assert_eq!(doc.cleanup_tracks(), Some(vec!["slide".to_owned()]));

    // Only the slide's track: the fade keeps its middle key.
    assert_eq!(doc.reduce(), "No key could go without changing the motion.");
    doc.selected.clear();
    assert_eq!(doc.reduce(), "Removed 1 key.");
    assert_eq!(doc.file.tracks[0].keys.len(), 2);

    // The slide ends 2 up from where it starts: bent back over the blend.
    assert_eq!(doc.fix_seam(), "Bent 2 tracks to end where they start.");
    let slide = &doc.file.tracks[1];
    assert_eq!(slide.keys.last().unwrap().time, 2.0);
    assert_eq!(slide.keys.last().unwrap().value, [0.0, 0.0, 0.0]);

    doc.cleanup.from = 0.0;
    doc.cleanup.to = 1.0;
    doc.cleanup.length = 2.0;
    doc.playhead = 1.5;
    assert!(doc.retime_range().unwrap().contains("now 3.00 s"));
    assert_eq!(doc.file.duration, 3.0);
    assert_eq!(doc.file.tracks[1].keys[1].time, 2.0);
    assert_eq!(
        (doc.cleanup.from, doc.cleanup.to),
        (0.0, 2.0),
        "the range follows its new end"
    );
    doc.cleanup.length = 3.0;
    assert!(doc.retime_range().is_ok());
    assert_eq!(doc.file.duration, 4.0);
    assert!(doc.unwritten(), "an undo step when written");
}

/// Hips with an arm out to each side.
fn arms() -> Arc<Rig> {
    let bone = |name: &str, parent, x: f32| Bone {
        name: name.into(),
        parent,
        rest: BoneTransform {
            translation: Vec3::new(x, if parent.is_some() { 0.5 } else { 1.0 }, 0.0),
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
        },
    };
    Arc::new(Rig {
        bones: vec![
            bone("Hips", None, 0.0),
            bone("LeftArm", Some(0), 0.2),
            bone("RightArm", Some(0), -0.2),
        ],
        joints: Vec::new(),
        fit: Mat4::IDENTITY,
    })
}

#[test]
fn mirror_needs_the_preview_skeleton_and_swaps_the_arms() {
    let rig = arms();
    let raised = |angle: f32| {
        bone_numbers(&BoneTransform {
            rotation: Quat::from_rotation_z(angle),
            ..rig.bones[1].rest
        })
    };
    let mut file = ClipFile::new(1.0);
    file.tracks.push(track(
        "bone1",
        "Hips/LeftArm",
        PropertyName::Bone,
        vec![(0.0, raised(0.0)), (1.0, raised(1.0))],
    ));
    let mut doc = PropertyDoc::open("assets/animations/Wave.anim.json".into(), file);
    let rigs = RigDefinitions::default();
    assert!(
        doc.mirror_bones(&rigs)
            .unwrap_err()
            .contains("skinned model")
    );
    doc.rig = Some(rig);
    assert_eq!(
        doc.mirror_bones(&rigs).unwrap(),
        "Mirrored: 2 bones swapped sides, 0 mirrored in place."
    );
    let right = doc
        .file
        .tracks
        .iter()
        .find(|t| t.target == "Hips/RightArm")
        .expect("the right arm is keyed");
    let end = axcenity_anim::bone_from_numbers(&right.keys.last().unwrap().value).unwrap();
    assert!(end.rotation.angle_between(Quat::from_rotation_z(-1.0)) < 1e-3);
}

/// Every piece of text drawn in `shapes`.
fn texts(shapes: &[egui::epaint::ClippedShape]) -> String {
    fn walk(shape: &egui::Shape, found: &mut String) {
        match shape {
            egui::Shape::Text(text) => {
                found.push_str(text.galley.text());
                found.push('\n');
            }
            egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| walk(s, found)),
            _ => {}
        }
    }
    let mut found = String::new();
    for clipped in shapes {
        walk(&clipped.shape, &mut found);
    }
    found
}

#[test]
fn the_section_draws_its_tools_in_a_narrow_column() {
    let mut doc = doc();
    let rigs = RigDefinitions::default();
    let ctx = theme::test_context();
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(260.0, 700.0),
        )),
        ..Default::default()
    };
    let shown = theme::run(&ctx, input, |ui| {
        egui::CentralPanel::default().show(ui, |ui| doc.cleanup_tools(ui, &rigs));
    });
    let text = texts(&shown.shapes);
    for tool in [
        "Smooth",
        "Reduce keys",
        "Fix loop seam",
        "Re-time",
        "Changes every track",
    ] {
        assert!(text.contains(tool), "{tool} in {text}");
    }
    assert!(!text.contains("Mirror"), "no bone keys: {text}");
    assert_eq!(
        (doc.cleanup.from, doc.cleanup.to),
        (0.0, 2.0),
        "the whole clip to start"
    );
    if let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS") {
        let path = std::path::PathBuf::from(folder).join("clip-cleanup.png");
        crate::snapshot::snapshot(&path, egui::vec2(260.0, 520.0), vec![Vec::new(); 2], |ui| {
            egui::CentralPanel::default().show(ui, |ui| doc.cleanup_tools(ui, &rigs));
        });
    }
}
