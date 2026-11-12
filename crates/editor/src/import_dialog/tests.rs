use super::outcome::Made;
use super::*;
use crate::surface::{MapFile, Slot};

/// Street (a group) holding Bench and Lamp, then Dumpster (a mesh)
/// holding Lid, all of plain material `Metal`.
fn dialog() -> ImportDialog {
    let material = axcenity_assets::ModelMaterial {
        name: "Metal".into(),
        ..Default::default()
    };
    let object = |name: &str, depth, triangles| ModelObject {
        path: name.into(),
        name: name.into(),
        depth,
        triangles,
        materials: vec!["Metal".into()],
    };
    ImportDialog::new(
        PathBuf::from("props.fbx"),
        vec![
            object("Street", 0, 0),
            object("Bench", 1, 10),
            object("Lamp", 1, 20),
            object("Dumpster", 0, 30),
            object("Lid", 1, 5),
        ],
        vec![MaterialMaps::of(&material)],
    )
}

fn rows(made: &[Made]) -> Vec<(usize, Option<ClassName>, &str)> {
    made.iter()
        .map(|row| (row.depth, row.class, row.name.as_str()))
        .collect()
}

#[test]
fn everything_starts_ticked_and_groups_are_not_imported_themselves() {
    let dialog = dialog();
    assert_eq!(dialog.chosen(), [1, 2, 3, 4]);
    assert!(dialog.all_chosen());
    assert_eq!(dialog.subtree(0), 0..3);
    assert_eq!(dialog.subtree(3), 3..5);
    assert_eq!(dialog.subtree(4), 4..5);
    assert_eq!(dialog.parent(2), Some(0));
    assert_eq!(dialog.parent(3), None);
}

#[test]
fn ticking_a_group_ticks_what_is_inside_it() {
    let mut dialog = dialog();
    dialog.set(0, false);
    assert_eq!(dialog.chosen(), [3, 4]);
    assert_eq!(dialog.state(0), (false, false));
    dialog.set(2, true);
    // Partly ticked: the Lamp is, the Bench isn't.
    assert_eq!(dialog.state(0), (false, true));
    assert!(!dialog.all_chosen());
    // Unticking the Lid leaves the Dumpster itself ticked.
    dialog.set(4, false);
    assert_eq!(dialog.chosen(), [2, 3]);
    assert_eq!(dialog.state(3), (false, true));
}

#[test]
fn only_the_tabs_a_file_needs_are_shown_in_order() {
    let mut dialog = dialog();
    assert_eq!(dialog.tabs(), [Tab::Objects, Tab::Materials]);
    assert_eq!(dialog.current(), Tab::Objects);
    // One object: nothing to choose, so it opens on its maps.
    dialog.pick_objects = false;
    assert_eq!(dialog.tabs(), [Tab::Materials]);
    assert_eq!(dialog.current(), Tab::Materials);

    let take = axcenity_assets::Take {
        skeleton: Default::default(),
        name: "Walk".into(),
        duration: 1.0,
        frame_rate: 30.0,
        tracks: vec![axcenity_assets::Track::default()],
    };
    let animation = AnimationImport::new(None, &[take], "walk").unwrap();
    let only = ImportDialog::animation_only(PathBuf::from("walk.fbx"), animation);
    assert_eq!(only.tabs(), [Tab::Animation]);
    assert!(only.made().is_empty());
    assert_eq!(
        only.creates().sentence(),
        "Creates: 1 clip in `assets/animations/walk/`. Copies the animation file to \
         `assets/models/walk.fbx`."
    );
    assert_eq!(
        only.files_made(),
        [
            (
                "assets/models/walk.fbx".to_owned(),
                "the animation file".to_owned()
            ),
            ("assets/animations/walk/".to_owned(), "1 clip".to_owned()),
        ]
    );
}

#[test]
fn what_you_get_follows_the_placement_and_the_ticks() {
    let mut dialog = dialog();
    dialog.set(4, false);
    // A Model, with a Model for the Street group; the Lid isn't ticked.
    assert_eq!(
        rows(&dialog.made()),
        [
            (0, Some(ClassName::Model), "props"),
            (1, Some(ClassName::Model), "Street"),
            (2, Some(ClassName::MeshPart), "Bench"),
            (2, Some(ClassName::MeshPart), "Lamp"),
            (1, Some(ClassName::MeshPart), "Dumpster"),
        ]
    );
    assert_eq!(dialog.made()[2].note, "1 material");

    dialog.placement = Placement::Separate;
    assert_eq!(
        rows(&dialog.made()),
        [
            (0, Some(ClassName::MeshPart), "Bench"),
            (0, Some(ClassName::MeshPart), "Lamp"),
            (0, Some(ClassName::MeshPart), "Dumpster"),
        ]
    );

    // One MeshPart named after the file, with a SurfaceAppearance per material.
    dialog.placement = Placement::Merged;
    assert_eq!(
        rows(&dialog.made()),
        [
            (0, Some(ClassName::MeshPart), "props"),
            (1, Some(ClassName::SurfaceAppearance), "Metal"),
        ]
    );
    assert_eq!(dialog.made()[1].note, "colors only");

    // Nothing ticked can't be imported, and the tab says so.
    for i in 0..dialog.objects.len() {
        dialog.set(i, false);
    }
    assert!(dialog.problem().is_some());
    assert_eq!(dialog.problem_tab(), Some(Tab::Objects));
}

#[test]
fn the_summary_says_what_import_creates_and_where() {
    let mut dialog = dialog();
    assert_eq!(
        dialog.creates().sentence(),
        "Creates: 1 Model with 4 MeshParts and 1 Model for its groups. Copies the model to \
         `assets/models/props.fbx`."
    );
    // Unticking the Street group drops its Model too.
    dialog.set(0, false);
    assert_eq!(
        dialog.creates().sentence(),
        "Creates: 1 Model with 2 MeshParts. Copies the model to `assets/models/props.fbx`."
    );
    dialog.placement = Placement::Separate;
    assert!(
        dialog
            .creates()
            .sentence()
            .starts_with("Creates: 2 MeshParts. ")
    );
    dialog.placement = Placement::Merged;
    assert!(
        dialog
            .creates()
            .sentence()
            .starts_with("Creates: 1 MeshPart. ")
    );

    let take = axcenity_assets::Take {
        skeleton: Default::default(),
        name: "Walk".into(),
        duration: 1.0,
        frame_rate: 30.0,
        tracks: vec![axcenity_assets::Track::default()],
    };
    let mut animation = AnimationImport::new(None, &[take], "props").unwrap();
    // A skeleton to move, so the MeshPart gets an Animator.
    animation.bones = vec![crate::anim_import::BoneRow {
        name: "Root".into(),
        depth: 0,
    }];
    let dialog = dialog.with_animation(Some(animation));
    assert_eq!(
        dialog.creates().sentence(),
        "Creates: 1 MeshPart · 1 clip in `assets/animations/props/` · 1 state machine · an \
         Animator. Copies the model to `assets/models/props.fbx`."
    );
}

#[test]
fn a_long_list_of_parts_ends_in_how_many_more() {
    let objects = (0..20)
        .map(|i| ModelObject {
            path: format!("Rock{i}"),
            name: format!("Rock{i}"),
            depth: 0,
            triangles: 12,
            materials: Vec::new(),
        })
        .collect();
    let mut dialog = ImportDialog::new(PathBuf::from("rocks.glb"), objects, Vec::new());
    dialog.placement = Placement::Separate;
    let made = dialog.made();
    assert_eq!(made.len(), outcome::MADE_ROWS);
    assert_eq!(made.last().unwrap().name, "… and 9 more");
    assert_eq!(made.last().unwrap().class, None);
}

#[test]
fn the_files_say_where_the_model_and_its_textures_go() {
    let mut dialog = dialog();
    // No maps: a .fbx on its own.
    assert_eq!(
        dialog.files_made(),
        [("assets/models/props.fbx".to_owned(), "the model".to_owned())]
    );
    // Two maps from the file and one picked: a folder of its own.
    let maps = &mut dialog.materials[0];
    maps.set_file(Slot::Color, Some(MapFile::Model("textures/a.png".into())));
    maps.set_file(Slot::Normal, Some(MapFile::Model("textures/b.png".into())));
    maps.set_file(Slot::Roughness, Some(MapFile::Picked("C:/r.png".into())));
    assert_eq!(
        dialog.files_made(),
        [(
            "assets/models/props/".to_owned(),
            "the model and 2 textures, plus 1 texture you picked".to_owned()
        )]
    );
}

/// An animated, textured FBX with every tab, in an 800×600 window: the
/// tabs are far taller than that, yet Import is in the window on every
/// tab (clicked in the tab bar), and a click on Import imports. With
/// `AXCENITY_SNAPSHOTS` set, each tab is saved as
/// `import-dialog-800x600-<tab>.png`.
#[test]
fn the_whole_dialog_fits_an_800_by_600_window_with_import_clickable() {
    let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/hello-animation/assets/models/noodle.fbx");
    let objects = axcenity_assets::list_model_objects(&file).unwrap();
    let model = axcenity_assets::load_model_file(&file).unwrap();
    let takes = axcenity_assets::load_takes_file(&file).unwrap();
    let animation = AnimationImport::new(model.rig.as_deref(), &takes, "noodle");
    assert!(animation.is_some(), "noodle.fbx is animated");
    let mut materials =
        crate::surface::materials_maps(model.primitives.iter().map(|p| &p.material));
    // Three materials with every map, so each opens with all its rows.
    materials.truncate(1);
    let mut maps = materials.pop().unwrap();
    for (slot, file) in maps.files.iter_mut().enumerate() {
        *file = Some(MapFile::Model(format!("textures/map{slot}.png")));
    }
    let materials = ["Body", "Eyes", "Hat"].map(|name| MaterialMaps {
        material: name.to_owned(),
        ..maps.clone()
    });
    let mut dialog = ImportDialog::new(file, objects, materials.to_vec())
        .with_animation(animation)
        .with_preview(model, takes);
    // The object list too, as for a file with several objects.
    dialog.pick_objects = true;
    dialog.placement = Placement::Merged;
    assert_eq!(
        dialog.tabs(),
        [Tab::Objects, Tab::Materials, Tab::Animation]
    );

    let size = egui::vec2(800.0, 600.0);
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
    let ctx = crate::theme::test_context();
    let mut frame = 0;
    let mut run = |dialog: &mut ImportDialog, events: Vec<egui::Event>| {
        let mut answer = None;
        frame += 1;
        let input = egui::RawInput {
            screen_rect: Some(screen),
            events,
            // A tenth of a second a frame, so the dialog has faded in.
            time: Some(f64::from(frame) * 0.1),
            ..Default::default()
        };
        let _ = crate::theme::run(&ctx, input, |ui| {
            let ctx = &ui.ctx().clone();
            answer = dialog.show(ctx, None)
        });
        answer
    };
    let click = |at: egui::Pos2| {
        let button = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        [
            vec![egui::Event::PointerMoved(at)],
            vec![button(true)],
            vec![button(false)],
        ]
    };
    // The scroll areas take a few frames to reach their full height.
    for _ in 0..10 {
        assert_eq!(run(&mut dialog, Vec::new()), None);
    }
    let buttons = dialog.buttons.expect("the buttons were drawn");
    assert!(screen.contains_rect(buttons), "{buttons:?} in {screen:?}");
    let import = dialog.import_button.expect("Import was drawn");
    assert!(screen.contains_rect(import), "{import:?} in {screen:?}");

    // Clicking each tab in the tab bar shows it, and the buttons stay in
    // the window on each.
    let mut seen = vec![dialog.current()];
    for &tab in &dialog.tabs()[1..] {
        let at = dialog.tab_rects[&tab].center();
        for events in click(at) {
            assert_eq!(run(&mut dialog, events), None);
        }
        for _ in 0..3 {
            run(&mut dialog, Vec::new());
        }
        seen.push(dialog.current());
        let buttons = dialog.buttons.unwrap();
        assert!(screen.contains_rect(buttons), "{buttons:?} on {seen:?}");
        let import = dialog.import_button.unwrap();
        assert!(screen.contains_rect(import), "{import:?} on {seen:?}");
    }
    assert_eq!(seen, dialog.tabs());

    let import = dialog.import_button.unwrap();
    let mut answers = Vec::new();
    for events in click(import.center()) {
        answers.push(run(&mut dialog, events));
    }
    assert_eq!(answers.last(), Some(&Some(Answer::Import)));

    if let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS") {
        for tab in dialog.tabs() {
            dialog.tab = tab;
            let name = tab.title().to_lowercase();
            crate::snapshot::snapshot(
                &PathBuf::from(&folder).join(format!("import-dialog-800x600-{name}.png")),
                size,
                vec![Vec::new(); 10],
                |ctx| {
                    dialog.show(ctx, None);
                },
            );
        }
    }
}

#[test]
fn big_numbers_get_commas() {
    assert_eq!(thousands(7), "7");
    assert_eq!(thousands(1234), "1,234");
    assert_eq!(thousands(1234567), "1,234,567");
}
