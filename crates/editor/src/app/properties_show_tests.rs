//! Looking is never an edit: showing an Instance's properties doesn't
//! change it. A widget that rewrites its value as it draws (a slider that
//! rounds it, say) made "Change GeographicLatitude of Lighting" an undo
//! step, and the project unsaved, just by selecting Lighting. The same
//! holds with several selected, whose differing values show as `—`.

use super::tests::copy_of_example;
use super::*;

#[test]
fn showing_the_properties_of_anything_in_the_examples_changes_nothing() {
    let examples = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let mut names: Vec<String> = std::fs::read_dir(&examples)
        .unwrap()
        .filter_map(|entry| {
            let entry = entry.ok()?;
            entry
                .path()
                .join("world.json")
                .exists()
                .then(|| entry.file_name().to_string_lossy().into_owned())
        })
        .collect();
    names.sort();
    assert!(names.len() > 20, "the examples: {names:?}");
    let ctx = theme::test_context();
    let mut seen = std::collections::HashSet::new();
    let mut changed = Vec::new();
    for name in names {
        let folder = copy_of_example(&name, &format!("properties-show-{name}"));
        let mut app = EditorApp::new(Some(&folder));
        // One of each class: the services, and what the example puts in.
        let ids: Vec<InstanceId> = app
            .world
            .descendants(app.world.root())
            .into_iter()
            .filter(|&id| {
                app.world
                    .get(id)
                    .is_some_and(|instance| seen.insert(instance.class()))
            })
            .collect();
        for id in ids {
            app.selected = Some(id);
            app.selection = vec![id];
            for _ in 0..2 {
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(320.0, 2400.0),
                    )),
                    ..Default::default()
                };
                let _ = crate::theme::run(&ctx, input, |ui| {
                    egui::CentralPanel::default().show(ui, |ui| app.properties_pane(ui));
                });
            }
            if let Some(label) = app.history.undo_label() {
                let class = app.world.get(id).map(|i| i.class());
                changed.push(format!("{name}: {class:?}: {label}"));
                app.undo();
            }
        }
        // Several at once (design system V5): each class with two or
        // more, and everything in the Workspace together.
        let mut by_class: std::collections::BTreeMap<&str, Vec<InstanceId>> = Default::default();
        for id in app.world.descendants(app.world.root()) {
            if let Some(instance) = app.world.get(id) {
                by_class
                    .entry(instance.class().as_str())
                    .or_default()
                    .push(id);
            }
        }
        let mut selections: Vec<Vec<InstanceId>> =
            by_class.into_values().filter(|ids| ids.len() > 1).collect();
        selections.push(app.world.descendants(app.world.workspace()));
        for ids in selections.into_iter().filter(|ids| ids.len() > 1) {
            app.selected = ids.last().copied();
            app.selection = ids;
            for _ in 0..2 {
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(320.0, 2400.0),
                    )),
                    ..Default::default()
                };
                let _ = crate::theme::run(&ctx, input, |ui| {
                    egui::CentralPanel::default().show(ui, |ui| app.properties_pane(ui));
                });
            }
            if let Some(label) = app.history.undo_label() {
                changed.push(format!("{name}: {} selected: {label}", app.selection.len()));
                app.undo();
            }
        }
        let _ = std::fs::remove_dir_all(&folder);
    }
    assert!(seen.len() > 40, "only {} classes shown", seen.len());
    assert!(changed.is_empty(), "showing them changed: {changed:#?}");
}
