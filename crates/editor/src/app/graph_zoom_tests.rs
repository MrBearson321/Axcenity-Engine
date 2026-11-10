//! Graph editors open their graphs at a size whose text reads without
//! zooming in (`docs/research/usability/design-system.md`, V10).

use super::tests::copy_of_example;
use super::*;

/// Draws the whole editor a few times, 1600 by 900 points.
fn draw(app: &mut EditorApp) {
    let ctx = theme::test_context();
    for _ in 0..3 {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1600.0, 900.0),
            )),
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| app.frame(ctx, None));
    }
}

/// Opens a graph in the editor.
type Open = fn(&mut EditorApp);

#[test]
fn every_graph_editor_opens_its_example_at_a_readable_zoom() {
    let cases: [(&str, &str, Open); 3] = [
        ("hello-material-graph", "zoom-material", |app| {
            app.open_material("assets/materials/MossyStone.material.json");
            app.materials.view = crate::material_editor::MaterialView::Graph;
        }),
        ("hello-behavior", "zoom-behavior", |app| {
            app.open_behavior_tree("assets/ai/Guard.behaviortree.json");
        }),
        ("hello-dialogue", "zoom-dialogue", |app| {
            app.open_dialogue("assets/dialogue/Farmer.dialogue.json");
        }),
    ];
    for (example, test, open) in cases {
        let folder = copy_of_example(example, test);
        let mut app = EditorApp::new(Some(&folder));
        open(&mut app);
        draw(&mut app);
        let zooms = match example {
            "hello-material-graph" => app.materials.view_zooms(),
            "hello-behavior" => app.behavior.view_zooms(),
            _ => app.dialogues.view_zooms(),
        };
        assert!(!zooms.is_empty(), "{example}: no graph drawn");
        for zoom in zooms {
            assert!(zoom >= 0.8, "{example} opened at {:.0}%", zoom * 100.0);
        }
        let _ = std::fs::remove_dir_all(&folder);
    }
}
