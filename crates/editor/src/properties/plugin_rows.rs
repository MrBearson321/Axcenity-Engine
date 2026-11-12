//! Plugins' rows in the Properties panel (roadmap G10,
//! `plugin:AddPropertyRow` and `AddPropertyButton`): one section per
//! plugin, named after it, with its rows that apply to the selected
//! Instance. An attribute row edits the attribute like the Attributes
//! section does; a button row asks the app to call the plugin.

use axcenity_core::{Instance, Value};
use axcenity_plugins::{PropertyRow, RowKind};
use eframe::egui;

use super::{section_titled, value_edit};
use crate::theme;

/// One running plugin's rows, with its place in the editor's list and its
/// name for the section title.
pub struct PluginRows<'a> {
    pub plugin: usize,
    pub name: &'a str,
    pub rows: &'a [PropertyRow],
}

/// What was done with a plugin's row this frame.
#[derive(Clone, Debug, PartialEq)]
pub enum RowAction {
    /// The attribute row `row` was set to `value`.
    Set {
        plugin: usize,
        row: u64,
        name: String,
        value: Value,
    },
    /// The button row `row` was clicked.
    Click { plugin: usize, row: u64 },
}

/// Draws a section per plugin with rows for `instance`, and returns what
/// was done.
pub fn plugin_sections(
    ui: &mut egui::Ui,
    instance: &Instance,
    plugins: &[PluginRows],
    open: Option<bool>,
) -> Option<RowAction> {
    let mut action = None;
    for plugin in plugins {
        let rows: Vec<&PropertyRow> = plugin
            .rows
            .iter()
            .filter(|row| row.applies_to(instance.class()))
            .collect();
        if rows.is_empty() {
            continue;
        }
        let salt = format!("plugin:{}:{}", plugin.plugin, plugin.name);
        section_titled(ui, &salt, plugin.name, true, open, |ui| {
            for row in rows {
                ui.push_id(row.id, |ui| {
                    if let Some(done) = row_ui(ui, instance, plugin.plugin, row) {
                        action = Some(done);
                    }
                });
                ui.end_row();
            }
        });
    }
    action
}

fn row_ui(
    ui: &mut egui::Ui,
    instance: &Instance,
    plugin: usize,
    row: &PropertyRow,
) -> Option<RowAction> {
    let palette = theme::palette(ui.ctx());
    match &row.kind {
        RowKind::Attribute {
            name,
            default,
            min,
            max,
        } => {
            let current = instance.attribute(name).cloned();
            let set = current.is_some();
            // An unset attribute's name is weaker, and it shows the default.
            let color = if set {
                palette.text_secondary
            } else {
                palette.text_weak
            };
            let label =
                ui.add(egui::Label::new(egui::RichText::new(&row.label).color(color)).truncate());
            let hint = match (row.tooltip.is_empty(), set) {
                (true, true) => format!("The attribute {name}"),
                (true, false) => format!("The attribute {name} (not set yet)"),
                (false, true) => row.tooltip.clone(),
                (false, false) => format!("{} (not set yet)", row.tooltip),
            };
            label.on_hover_text(hint);
            let shown = current.unwrap_or_else(|| default.clone());
            let value = match (shown, min, max) {
                (Value::Number(mut number), Some(min), Some(max)) => ui
                    .add(
                        egui::Slider::new(&mut number, *min..=*max)
                            .clamping(egui::SliderClamping::Always),
                    )
                    .changed()
                    .then_some(Value::Number(number)),
                (shown, ..) => value_edit(ui, name, shown).map(|value| match value {
                    Value::Number(number) => {
                        let number = min.map_or(number, |min| number.max(min));
                        Value::Number(max.map_or(number, |max| number.min(max)))
                    }
                    other => other,
                }),
            };
            value.map(|value| RowAction::Set {
                plugin,
                row: row.id,
                name: name.clone(),
                value,
            })
        }
        RowKind::Button => {
            ui.label("");
            let button = ui.button(&row.label);
            let button = if row.tooltip.is_empty() {
                button
            } else {
                button.on_hover_text(&row.tooltip)
            };
            button.clicked().then_some(RowAction::Click {
                plugin,
                row: row.id,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use axcenity_assets::Assets;
    use axcenity_core::{ClassName, InstanceId, World};

    use super::*;

    fn rows() -> Vec<PropertyRow> {
        vec![
            PropertyRow {
                id: 1,
                class: "BasePart".into(),
                label: "Speed".into(),
                tooltip: "How fast it spins".into(),
                kind: RowKind::Attribute {
                    name: "Speed".into(),
                    default: Value::Number(16.0),
                    min: Some(0.0),
                    max: Some(50.0),
                },
            },
            PropertyRow {
                id: 2,
                class: "Part".into(),
                label: "Spin Once".into(),
                tooltip: String::new(),
                kind: RowKind::Button,
            },
        ]
    }

    /// Draws the Properties panel for `id` in a 260-point column with the
    /// plugin "Spinner"'s rows, then (with `click`) clicks the text `click`.
    /// Returns the texts drawn, the row action, and whether it fit.
    fn panel(
        world: &World,
        id: InstanceId,
        click: Option<&str>,
    ) -> (Vec<String>, Option<RowAction>, bool) {
        fn walk(shape: &egui::Shape, found: &mut Vec<(String, egui::Pos2)>) {
            match shape {
                egui::Shape::Text(text) => found.push((
                    text.galley.text().to_owned(),
                    text.visual_bounding_rect().center(),
                )),
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| walk(s, found)),
                _ => {}
            }
        }
        let rows = rows();
        let plugins = [PluginRows {
            plugin: 3,
            name: "Spinner",
            rows: &rows,
        }];
        let ctx = crate::theme::test_context();
        let size = egui::vec2(260.0, 1400.0);
        let mut action = None;
        let mut fits = true;
        let mut frame = |events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                events,
                ..Default::default()
            };
            let output = crate::theme::run(&ctx, input, |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    let room = ui.max_rect();
                    let _ = super::super::show(
                        ui,
                        world,
                        &Assets::default(),
                        Some(id),
                        false,
                        &[],
                        |ui| {
                            let instance = world.get(id).unwrap();
                            if let Some(done) = plugin_sections(ui, instance, &plugins, None) {
                                action = Some(done);
                            }
                        },
                    );
                    fits &= ui.min_rect().width() <= room.width() + 0.5;
                });
            });
            let mut found = Vec::new();
            for clipped in &output.shapes {
                walk(&clipped.shape, &mut found);
            }
            found
        };
        frame(Vec::new());
        let found = frame(Vec::new());
        if let Some(text) = click
            && let Some((_, at)) = found.iter().find(|(t, _)| t == text)
        {
            let at = *at;
            let press = |pressed| egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            frame(vec![egui::Event::PointerMoved(at)]);
            frame(vec![press(true)]);
            frame(vec![press(false)]);
        }
        (found.into_iter().map(|(t, _)| t).collect(), action, fits)
    }

    #[test]
    fn a_plugins_rows_show_under_its_name_for_their_class_and_a_button_click_is_reported() {
        let mut world = World::new();
        let part = world.create(ClassName::Part).unwrap();
        world.set_parent(part, Some(world.workspace())).unwrap();
        let model = world.create(ClassName::Model).unwrap();
        world.set_parent(model, Some(world.workspace())).unwrap();

        let (texts, action, fits) = panel(&world, part, Some("Spin Once"));
        for wanted in ["Spinner", "Speed", "Spin Once"] {
            assert!(
                texts.iter().any(|t| t.ends_with(wanted)),
                "{wanted} not in {texts:?}"
            );
        }
        assert!(fits, "the rows fit a 260-point column");
        assert_eq!(action, Some(RowAction::Click { plugin: 3, row: 2 }));

        // A Model is neither a BasePart nor a Part: no section.
        let (texts, action, _) = panel(&world, model, None);
        assert!(!texts.iter().any(|t| t.ends_with("Spinner")), "{texts:?}");
        assert_eq!(action, None);
    }
}
