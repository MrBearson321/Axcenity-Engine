//! The inspector's **Widget** section for a Slider, Toggle, Dropdown or
//! ProgressBar: its value and what it can be, so a widget is set up in the
//! designer alone. Its parts are styled in CSS (`::track`, `::fill`,
//! `::thumb`, `::check`, `::list`, `::option`).

use axcenity_core::{ClassName, InstanceId, Value};
use eframe::egui;

use super::{DesignerAction, DesignerScene};
use crate::properties::Edit;
use crate::responsive;

/// The section, for a widget; nothing for anything else. Each change is one
/// undo step, and a dragged number is one.
pub(super) fn widget_section(
    ui: &mut egui::Ui,
    scene: &DesignerScene,
    id: InstanceId,
    actions: &mut Vec<DesignerAction>,
) {
    let Some(instance) = scene.world.get(id) else {
        return;
    };
    let class = instance.class();
    if !class.is_value_widget() {
        return;
    }
    let number = |name| instance.property(name).and_then(Value::as_number);
    let text = |name| {
        instance
            .property(name)
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned()
    };
    let mut set = |name: &'static str, value: Value| {
        actions.push(DesignerAction::Edit(id, Edit::Property(name, value)));
    };
    responsive::form(ui, "ui designer widget", |ui| match class {
        ClassName::Slider | ClassName::ProgressBar => {
            let names: &[(&'static str, &str)] = if class == ClassName::Slider {
                &[
                    ("Value", "Value"),
                    ("MinValue", "Min"),
                    ("MaxValue", "Max"),
                    ("Step", "Step"),
                ]
            } else {
                &[("Value", "Value"), ("MinValue", "Min"), ("MaxValue", "Max")]
            };
            for &(name, label) in names {
                let Some(mut value) = number(name) else {
                    continue;
                };
                ui.label(label);
                let mut drag = egui::DragValue::new(&mut value).speed(1.0);
                if name == "Step" {
                    drag = drag.range(0.0..=f64::MAX);
                }
                if ui.add(drag).changed() {
                    set(name, Value::Number(value));
                }
                ui.end_row();
            }
            ui.label("Orientation");
            let vertical = text("Orientation").eq_ignore_ascii_case("Vertical");
            ui.horizontal(|ui| {
                for (way, is_vertical) in [("Horizontal", false), ("Vertical", true)] {
                    if ui.selectable_label(vertical == is_vertical, way).clicked()
                        && vertical != is_vertical
                    {
                        set("Orientation", Value::String(way.to_owned()));
                    }
                }
            });
            ui.end_row();
        }
        ClassName::Toggle => {
            ui.label("On");
            let mut on = instance.property("Value").and_then(Value::as_bool) == Some(true);
            if ui.checkbox(&mut on, "").changed() {
                set("Value", Value::Bool(on));
            }
            ui.end_row();
            ui.label("Style");
            let checkbox = text("Style").eq_ignore_ascii_case("Checkbox");
            ui.horizontal(|ui| {
                for (style, is_checkbox) in [("Switch", false), ("Checkbox", true)] {
                    if ui
                        .selectable_label(checkbox == is_checkbox, style)
                        .clicked()
                        && checkbox != is_checkbox
                    {
                        set("Style", Value::String(style.to_owned()));
                    }
                }
            });
            ui.end_row();
        }
        ClassName::Dropdown => {
            ui.label("Options").on_hover_text("One per line");
            let mut options = text("Options");
            if ui
                .add(
                    egui::TextEdit::multiline(&mut options)
                        .desired_rows(3)
                        .desired_width(f32::INFINITY),
                )
                .changed()
            {
                set("Options", Value::String(options.clone()));
            }
            ui.end_row();
            ui.label("Chosen");
            let chosen = text("Value");
            egui::ComboBox::from_id_salt(("ui designer dropdown value", id))
                .selected_text(if chosen.is_empty() { "(none)" } else { &chosen })
                .show_ui(ui, |ui| {
                    let mut pick = |label: &str, value: &str| {
                        if ui.selectable_label(chosen == value, label).clicked() && chosen != value
                        {
                            set("Value", Value::String(value.to_owned()));
                        }
                    };
                    pick("(none)", "");
                    for option in options.lines().map(str::trim).filter(|o| !o.is_empty()) {
                        pick(option, option);
                    }
                });
            ui.end_row();
            ui.label("Placeholder");
            let mut placeholder = text("Placeholder");
            if ui
                .add(egui::TextEdit::singleline(&mut placeholder).desired_width(f32::INFINITY))
                .changed()
            {
                set("Placeholder", Value::String(placeholder));
            }
            ui.end_row();
        }
        _ => {}
    });
}
