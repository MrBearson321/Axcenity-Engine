//! Looking is never an edit: showing an Instance's properties doesn't
//! change it. A widget that rewrites its value as it draws (a slider that
//! rounds it, say) made "Change GeographicLatitude of Lighting" an undo
//! step, and the project unsaved, just by selecting Lighting.

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
                let _ = ctx.run(input, |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| app.properties_pane(ui));
                });
            }
            if let Some(label) = app.history.undo_label() {
                let class = app.world.get(id).map(|i| i.class());
                changed.push(format!("{name}: {class:?}: {label}"));
                app.undo();
            }
        }
        let _ = std::fs::remove_dir_all(&folder);
    }
    assert!(seen.len() > 40, "only {} classes shown", seen.len());
    assert!(changed.is_empty(), "showing them changed: {changed:#?}");
}
