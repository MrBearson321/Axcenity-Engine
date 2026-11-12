//! The graph's variables, edited beside it.

use super::*;

/// The graph's variables, beside it: each one's name and starting value,
/// with + to add one and a bin to remove one. Returns whether they changed.
pub fn variables_panel(ui: &mut egui::Ui, graph: &mut NodeGraph, editable: bool) -> bool {
    let before = graph.variables.clone();
    ui.horizontal(|ui| {
        ui.strong("Variables");
        if ui
            .add_enabled(editable, egui::Button::new(icons::PLUS).small())
            .on_hover_text("Add a variable the whole graph shares")
            .clicked()
        {
            let name = (1..)
                .map(|n| format!("value{n}"))
                .find(|name| {
                    !graph
                        .variables
                        .iter()
                        .any(|variable| variable.name == *name)
                })
                .expect("some name is free");
            graph.variables.push(axcenity_blocks::node_graph::Variable {
                name,
                value: "0".to_owned(),
            });
        }
    });
    if graph.variables.is_empty() {
        ui.weak("None yet. A variable keeps a value between events, such as a score.");
    }
    let mut remove = None;
    ui.add_enabled_ui(editable, |ui| {
        for (index, variable) in graph.variables.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut variable.name).desired_width(80.0))
                    .on_hover_text("Its name: letters, digits and underscores");
                ui.label("=");
                ui.add(egui::TextEdit::singleline(&mut variable.value).desired_width(60.0))
                    .on_hover_text("What it starts as: a number, true or false, or text");
                if ui
                    .small_button(icons::TRASH)
                    .on_hover_text("Remove this variable")
                    .clicked()
                {
                    remove = Some(index);
                }
            });
        }
    });
    if let Some(index) = remove {
        graph.variables.remove(index);
    }
    graph.variables != before
}
