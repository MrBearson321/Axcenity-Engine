use std::path::PathBuf;

use axcenity_anim::{ClipFile, Severity};
use axcenity_assets::{BoneTransform, RigDefinition};

use super::*;

/// A project with the hello-animation example's noodle and its Walk clip,
/// in a folder of its own.
fn project(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "axcenity-retarget-panel-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let example = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-animation");
    std::fs::create_dir_all(root.join("assets/models")).unwrap();
    std::fs::create_dir_all(root.join("assets/animations/noodle")).unwrap();
    for file in [
        "assets/models/noodle.fbx",
        "assets/animations/noodle/Walk.anim.json",
    ] {
        std::fs::copy(example.join(file), root.join(file)).unwrap();
    }
    root
}

fn open_walk(root: &Path, assets: &Assets) -> ClipDoc {
    let path = "assets/animations/noodle/Walk.anim.json";
    let text = std::fs::read_to_string(root.join(path)).unwrap();
    let file = ClipFile::from_json(&text).unwrap();
    ClipDoc::open(
        path.to_owned(),
        file,
        assets,
        &["assets/models/noodle.fbx".to_owned()],
    )
}

fn bone(name: &str, parent: Option<usize>) -> Bone {
    Bone {
        name: name.into(),
        parent,
        rest: BoneTransform::IDENTITY,
    }
}

#[test]
fn a_bone_map_is_edited_saved_as_a_rig_definition_and_then_used() {
    let root = project("bone-map");
    let assets = Assets::new(&root);
    let bones = vec![
        bone("root_pelvis", None),
        bone("spine", Some(0)),
        bone("Hand_L", Some(1)),
        bone("odd_wrist_thing", Some(1)),
    ];
    let mut map = BoneMap::open(
        Which::Model,
        "assets/models/Odd Guy.fbx",
        bones.clone(),
        &assets.rig_definitions(),
    );
    assert_eq!(map.path, "assets/rigs/Odd Guy.rig.json");
    assert_eq!(map.map.get(HumanBone::LeftHand), Some(2));
    assert!(map.problems().iter().any(|p| p.severity == Severity::Error));

    map.set(HumanBone::Hips, Some(0));
    // The wrist thing is the left hand; `Hand_L` plays no role now.
    map.set(HumanBone::LeftHand, Some(3));
    assert!(map.changed);
    // A bone given to another role leaves the one it had.
    map.set(HumanBone::Spine, Some(0));
    assert_eq!(map.map.get(HumanBone::Hips), None);
    map.set(HumanBone::Hips, Some(0));
    map.set(HumanBone::Spine, Some(1));
    let definition = map.definition();
    assert_eq!(definition.bones["LeftHand"], "odd_wrist_thing");
    assert_eq!(definition.bones["Hips"], "root_pelvis");

    map.save(&root, &assets).unwrap();
    assert!(!map.changed);
    let saved = std::fs::read_to_string(root.join("assets/rigs/Odd Guy.rig.json")).unwrap();
    assert_eq!(RigDefinition::from_json(&saved).unwrap(), definition);
    // The project's definitions are read again, and this skeleton fits.
    let (found, path) = HumanoidMap::for_skeleton(&bones, &assets.rig_definitions());
    assert_eq!(path.as_deref(), Some("assets/rigs/Odd Guy.rig.json"));
    assert_eq!(found.get(HumanBone::LeftHand), Some(3));
    // Opening it again edits the definition it fits.
    let again = BoneMap::open(Which::Model, "x.fbx", bones, &assets.rig_definitions());
    assert_eq!(again.path, "assets/rigs/Odd Guy.rig.json");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_clip_gets_a_new_profile_whose_changes_are_written_at_once() {
    let root = project("profile");
    let assets = Assets::new(&root);
    let mut doc = open_walk(&root, &assets);
    let mut panel = RetargetPanel::default();
    assert!(profile_paths(&root).is_empty());

    let path = new_profile_path(&root, &doc.path);
    assert_eq!(path, "assets/rigs/Walk.retarget.json");
    panel
        .use_profile(&mut doc, &root, Some(path.clone()))
        .unwrap();
    assert_eq!(doc.file.retarget.as_deref(), Some(path.as_str()));
    assert_eq!(profile_paths(&root), std::slice::from_ref(&path));
    // The next new one gets another name.
    assert_eq!(
        new_profile_path(&root, &doc.path),
        "assets/rigs/Walk 2.retarget.json"
    );
    assert_eq!(
        doc.profile(&assets).unwrap(),
        Some(RetargetProfile::default())
    );

    let planted = RetargetProfile {
        plant_feet: true,
        hips: HipsTravel::InPlace,
        ..RetargetProfile::default()
    };
    panel
        .change_profile(&mut doc, &root, planted.clone())
        .unwrap();
    assert_eq!(doc.profile(&assets).unwrap(), Some(planted));
    // The link keeps the profile when the clip is saved.
    assert_eq!(
        doc.file.to_file().link.unwrap().retarget.as_deref(),
        Some(path.as_str())
    );

    // A profile asks for retargeting, which the noodle (not a humanoid)
    // can't have, so it still plays by its bone names.
    assert!(doc.pose(&assets).is_ok());
    panel.use_profile(&mut doc, &root, None).unwrap();
    assert_eq!(doc.file.retarget, None);
    let _ = std::fs::remove_dir_all(&root);
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
fn the_section_says_how_the_take_plays_and_what_the_checks_found() {
    let root = project("draw");
    let assets = Assets::new(&root);
    let mut doc = open_walk(&root, &assets);
    let mut panel = RetargetPanel::default();
    let mut output = Output::default();
    let ctx = theme::test_context();
    let input = || egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(420.0, 900.0),
        )),
        ..Default::default()
    };
    let mut draw = |doc: &mut ClipDoc, panel: &mut RetargetPanel| {
        let shown = theme::run(&ctx, input(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                panel.show(ui, doc, &assets, &root, &mut output);
            });
        });
        texts(&shown.shapes)
    };
    let text = draw(&mut doc, &mut panel);
    assert!(text.contains("Plays by bone names"), "{text}");
    // Checks only matter for a take going through roles.
    assert!(!text.contains("No bone plays"), "{text}");
    assert!(text.contains("None (defaults)"), "{text}");

    panel
        .use_profile(
            &mut doc,
            &root,
            Some("assets/rigs/Walk.retarget.json".into()),
        )
        .unwrap();
    let text = draw(&mut doc, &mut panel);
    assert!(text.contains("profile isn't used"), "{text}");
    // The noodle isn't a humanoid: its checks say which roles it lacks.
    assert!(text.contains("No bone plays Hips"), "{text}");
    assert!(!text.contains("finger"), "{text}");
    assert!(text.contains("Plant feet"), "{text}");

    panel.bone_map = Some(BoneMap::open(
        Which::Model,
        "assets/models/noodle.fbx",
        assets
            .model_blocking("assets/models/noodle.fbx")
            .unwrap()
            .rig
            .as_ref()
            .unwrap()
            .bones
            .clone(),
        &assets.rig_definitions(),
    ));
    // A window is sized on its first frame and drawn from its second.
    draw(&mut doc, &mut panel);
    let text = draw(&mut doc, &mut panel);
    assert!(text.contains("Bone map: noodle.fbx"), "{text}");
    assert!(text.contains("LeftUpperArm"), "{text}");
    if let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS") {
        let path = PathBuf::from(folder).join("retarget-panel.png");
        crate::snapshot::snapshot(&path, egui::vec2(420.0, 900.0), vec![Vec::new(); 3], |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                panel.show(ui, &mut doc, &assets, &root, &mut output);
            });
        });
    }
    let _ = std::fs::remove_dir_all(&root);
}
