//! Properties with several selected (design system V5): an edit goes to
//! each of them as one undo step, undo gives each its own value back, and
//! showing them changes nothing.

use super::tests::example;
use super::*;

/// hello-part with three new Parts in the Workspace, selected: Anchored on
/// the first only, and each a different Transparency.
fn three_parts() -> (EditorApp, [InstanceId; 3]) {
    let mut app = EditorApp::new(Some(&example("hello-part")));
    let workspace = app.world.workspace();
    let parts = [0.1, 0.2, 0.3].map(|transparency| {
        let part = app.world.create(ClassName::Part).unwrap();
        app.world.set_parent(part, Some(workspace)).unwrap();
        app.world
            .set_property(part, "Transparency", Value::Number(transparency))
            .unwrap();
        part
    });
    app.world
        .set_property(parts[0], "Anchored", Value::Bool(true))
        .unwrap();
    app.world
        .set_property(parts[1], "Anchored", Value::Bool(false))
        .unwrap();
    app.world
        .set_property(parts[2], "Anchored", Value::Bool(false))
        .unwrap();
    app.selection = parts.to_vec();
    app.selected = Some(parts[2]);
    (app, parts)
}

fn value(app: &EditorApp, id: InstanceId, name: &str) -> Value {
    let world = shown(&app.world, &app.play);
    world.get(id).unwrap().property(name).unwrap().clone()
}

fn draw_properties(app: &mut EditorApp, ctx: &egui::Context) -> egui::FullOutput {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(320.0, 2400.0),
        )),
        ..Default::default()
    };
    crate::theme::run(ctx, input, |ui| {
        egui::CentralPanel::default().show(ui, |ui| app.properties_pane(ui));
    })
}

#[test]
fn an_edit_to_three_parts_is_one_undo_step_and_undo_gives_each_its_own_back() {
    let (mut app, parts) = three_parts();
    app.apply_many(&parts, Edit::Property("Anchored", Value::Bool(true)));
    for part in parts {
        assert_eq!(value(&app, part, "Anchored"), Value::Bool(true));
    }
    assert_eq!(app.history.undo_label(), Some("Change Anchored of 3 Parts"));
    app.undo();
    let anchored = parts.map(|part| value(&app, part, "Anchored"));
    assert_eq!(anchored, [true, false, false].map(Value::Bool));
    assert_eq!(app.history.undo_label(), None, "one step");

    // Dragging a number is many edits but one step.
    for step in 1..=5 {
        let transparency = Value::Number(0.5 + step as f64 / 100.0);
        app.apply_many(&parts, Edit::Property("Transparency", transparency));
    }
    app.history.seal();
    app.undo();
    let transparency = parts.map(|part| value(&app, part, "Transparency"));
    assert_eq!(transparency, [0.1, 0.2, 0.3].map(Value::Number));
    assert_eq!(app.history.undo_label(), None);

    // ↺ puts each back to its own class's default, as one step.
    app.apply_many(&parts, Edit::Reset("Transparency"));
    for part in parts {
        assert_eq!(value(&app, part, "Transparency"), Value::Number(0.0));
    }
    app.undo();
    assert_eq!(value(&app, parts[1], "Transparency"), Value::Number(0.2));
}

#[test]
fn a_locked_one_keeps_its_values_but_takes_locked() {
    let (mut app, parts) = three_parts();
    app.world
        .set_property(parts[2], "Locked", Value::Bool(true))
        .unwrap();
    app.apply_many(&parts, Edit::Property("Transparency", Value::Number(0.9)));
    assert_eq!(value(&app, parts[0], "Transparency"), Value::Number(0.9));
    assert_eq!(value(&app, parts[2], "Transparency"), Value::Number(0.3));
    app.apply_many(&parts, Edit::Property("Locked", Value::Bool(false)));
    assert_eq!(value(&app, parts[2], "Locked"), Value::Bool(false));
}

#[test]
fn names_go_to_all_that_can_be_renamed() {
    let (mut app, parts) = three_parts();
    app.apply_many(&parts, Edit::Name("Crate".into()));
    for part in parts {
        assert_eq!(app.world.get(part).unwrap().name(), "Crate");
    }
    assert_eq!(app.history.undo_label(), Some("Rename 3 Parts"));
}

#[test]
fn different_kinds_share_only_what_they_all_have() {
    let (mut app, parts) = three_parts();
    let model = app.world.create(ClassName::Model).unwrap();
    app.world
        .set_parent(model, Some(app.world.workspace()))
        .unwrap();
    let parts_only = crate::properties::shared_names(&app.world, &parts);
    let names: Vec<_> = parts_only.iter().map(|(name, _)| *name).collect();
    assert!(names.contains(&"Anchored") && names.contains(&"Color"));
    assert!(parts_only.contains(&("Transparency", true)), "differs");
    assert!(parts_only.contains(&("CanCollide", false)), "the same");

    let mixed = crate::properties::shared_names(&app.world, &[parts[0], model]);
    let names: Vec<_> = mixed.iter().map(|(name, _)| *name).collect();
    assert!(
        !names.contains(&"Anchored"),
        "a Model has no Anchored: {names:?}"
    );
    for name in &names {
        assert!(
            app.world.get(model).unwrap().property(name).is_some()
                && app.world.get(parts[0]).unwrap().property(name).is_some()
        );
    }
    // Editing what both have changes both; what one lacks is left out.
    app.apply_many(
        &[parts[0], model],
        Edit::Property("Anchored", Value::Bool(false)),
    );
    assert_eq!(value(&app, parts[0], "Anchored"), Value::Bool(false));
}

#[test]
fn several_selected_show_a_dash_where_they_differ_and_looking_changes_nothing() {
    let (mut app, _) = three_parts();
    let ctx = theme::test_context();
    ctx.enable_accesskit();
    let mut output = draw_properties(&mut app, &ctx);
    for _ in 0..2 {
        output = draw_properties(&mut app, &ctx);
    }
    assert_eq!(app.history.undo_label(), None, "looking is never an edit");
    let update = output.platform_output.accesskit_update.unwrap();
    let labels: Vec<String> = update
        .nodes
        .iter()
        .filter_map(|(_, node)| node.label().or(node.value()).map(str::to_owned))
        .collect();
    assert!(labels.iter().any(|l| l == "3 Parts"), "header: {labels:?}");
    assert!(labels.iter().any(|l| l == "—"), "Transparency differs");
}

#[test]
fn during_play_an_edit_changes_every_selected_part_in_the_game() {
    let (mut app, parts) = three_parts();
    app.play();
    assert!(app.playing());
    app.apply_many(&parts, Edit::Property("Transparency", Value::Number(0.7)));
    for part in parts {
        assert_eq!(value(&app, part, "Transparency"), Value::Number(0.7));
    }
    app.stop();
    assert_eq!(value(&app, parts[0], "Transparency"), Value::Number(0.1));
}

/// With `AXCENITY_SNAPSHOTS`, saves Properties for three Parts that differ
/// (`properties-three-parts.png`) and for a Part and a Model together.
#[test]
fn snapshot_of_several_selected() {
    let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS") else {
        return;
    };
    let folder = std::path::PathBuf::from(folder);
    let (mut app, parts) = three_parts();
    let size = egui::vec2(300.0, 820.0);
    let draw = |app: &mut EditorApp, ui: &mut egui::Ui| {
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(theme::palette(ui.ctx()).panel)
                    .inner_margin(8),
            )
            .show(ui, |ui| app.properties_pane(ui));
    };
    crate::snapshot::snapshot(
        &folder.join("properties-three-parts.png"),
        size,
        vec![Vec::new(); 3],
        |ui| draw(&mut app, ui),
    );
    // One alone, to compare.
    app.selection = vec![parts[2]];
    crate::snapshot::snapshot(
        &folder.join("properties-one-part.png"),
        size,
        vec![Vec::new(); 3],
        |ui| draw(&mut app, ui),
    );
    let model = app.world.create(ClassName::Model).unwrap();
    app.world
        .set_parent(model, Some(app.world.workspace()))
        .unwrap();
    app.selection = vec![parts[0], model];
    app.selected = Some(model);
    crate::snapshot::snapshot(
        &folder.join("properties-part-and-model.png"),
        size,
        vec![Vec::new(); 3],
        |ui| draw(&mut app, ui),
    );
}
