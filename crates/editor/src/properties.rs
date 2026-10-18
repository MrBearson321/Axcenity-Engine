//! The Properties panel: the selected Instance's name, class and properties,
//! in groups like Roblox Studio's, each with a control that fits its type.
//!
//! The panel never changes the world itself. [`show`] returns the [`Edit`]
//! the user made, and the app applies it, which keeps the panel easy to test
//! and gives one place to notice unsaved changes.

use axcenity_core::{Color3, Instance, InstanceId, Material, PartType, Value, World};
use eframe::egui;

/// A change made in the Properties panel.
#[derive(Clone, Debug, PartialEq)]
pub enum Edit {
    Name(String),
    Property(&'static str, Value),
}

/// One group of properties in the panel, such as "Appearance".
#[derive(Debug, PartialEq)]
pub struct Section {
    pub title: &'static str,
    pub properties: Vec<(&'static str, Value)>,
}

/// The groups, in panel order, and which properties go in each. Name and
/// ClassName come first, under "Data", and aren't in the property map.
const GROUPS: [(&str, &[&str]); 3] = [
    (
        "Appearance",
        &["Color", "Material", "Metalness", "Roughness"],
    ),
    ("Behavior", &["Anchored"]),
    ("Part", &["Shape", "Position", "Orientation", "Size"]),
];
/// Where properties that no group lists go, so a new property always shows.
const OTHER: &str = "Other";

/// The smallest a part can be on any axis, the same as Roblox. Zero would
/// leave nothing to see or click. The Scale tool stops here too.
pub const MIN_SIZE: f32 = 0.001;
/// Numbers show at most this many decimals, so a typed 0.3 doesn't show up
/// as 0.30000001 (the closest a 32-bit float gets).
const MAX_DECIMALS: usize = 3;

/// `instance`'s properties grouped into sections, in panel order. Empty
/// sections are left out.
pub fn sections(instance: &Instance) -> Vec<Section> {
    let mut sections: Vec<Section> = GROUPS
        .iter()
        .map(|&(title, names)| Section {
            title,
            properties: names
                .iter()
                .filter_map(|&name| Some((name, instance.property(name)?.clone())))
                .collect(),
        })
        .collect();
    let other = instance
        .properties()
        .filter(|(name, _)| !GROUPS.iter().any(|(_, names)| names.contains(name)))
        .map(|(name, value)| (name, value.clone()))
        .collect();
    sections.push(Section {
        title: OTHER,
        properties: other,
    });
    sections.retain(|section| !section.properties.is_empty());
    sections
}

/// Whether the Properties panel lets you rename this Instance. Services keep
/// their names, and a Script is named after its file, so renaming it here
/// would be lost on the next load.
pub fn can_rename(instance: &Instance) -> bool {
    !instance.class().is_service()
}

/// Whether the Name box in this panel can rename `instance`. Renaming a
/// script renames its file, which shouldn't happen at every key typed, so
/// Scripts are renamed in the Explorer instead, all at once.
pub fn can_rename_here(instance: &Instance) -> bool {
    can_rename(instance) && !instance.class().is_script()
}

/// Shown when hovering a greyed-out value during Play.
const PLAYING_HINT: &str = "Values show the running game. Press Stop to edit.";

/// Draws the panel for `selected` and returns the edit made this frame, if any.
/// When not `editable` (during Play), the values are shown but greyed out.
pub fn show(
    ui: &mut egui::Ui,
    world: &World,
    selected: Option<InstanceId>,
    editable: bool,
) -> Option<Edit> {
    ui.strong("Properties");
    ui.separator();
    let mut edit = None;
    // The scroll area fills the panel even when nothing is selected. Without
    // it the panel would shrink to fit one line of text and stay that small.
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            let Some((id, instance)) = selected.and_then(|id| Some((id, world.get(id)?))) else {
                ui.weak("Select something in the Explorer or the viewport to see its properties.");
                return;
            };
            // Widgets remember things such as half-typed numbers by ID. Mixing in
            // the selection gives each Instance its own, so nothing typed for one
            // part turns up on the next one selected.
            ui.push_id(id, |ui| {
                section(ui, "Data", |ui| {
                    ui.label("Name");
                    let mut name = instance.name().to_string();
                    if !editable {
                        ui.add_enabled(false, egui::TextEdit::singleline(&mut name))
                            .on_disabled_hover_text(PLAYING_HINT);
                    } else if can_rename_here(instance) {
                        if ui.text_edit_singleline(&mut name).changed() {
                            edit = Some(Edit::Name(name));
                        }
                    } else {
                        let hint = if instance.class().is_script() {
                            "Rename a script in the Explorer (F2). That renames its file too."
                        } else {
                            "Services can't be renamed."
                        };
                        ui.add_enabled(false, egui::TextEdit::singleline(&mut name))
                            .on_disabled_hover_text(hint);
                    }
                    ui.end_row();
                    ui.label("ClassName");
                    ui.label(instance.class().as_str());
                    ui.end_row();
                });

                for group in sections(instance) {
                    section(ui, group.title, |ui| {
                        for (name, value) in group.properties {
                            ui.label(name);
                            // `add_enabled_ui` greys out (and ignores) whatever is
                            // drawn inside it; `.inner` is what the closure returned.
                            let changed = ui
                                .add_enabled_ui(editable, |ui| value_control(ui, name, value))
                                .inner;
                            if let Some(value) = changed {
                                edit = Some(Edit::Property(name, value));
                            }
                            ui.end_row();
                        }
                    });
                }
            });
        });
    edit
}

/// A collapsible group with a two-column grid of names and controls.
fn section(ui: &mut egui::Ui, title: &str, rows: impl FnOnce(&mut egui::Ui)) {
    egui::CollapsingHeader::new(title)
        .default_open(true)
        .show(ui, |ui| {
            egui::Grid::new(title)
                .num_columns(2)
                .striped(true)
                .spacing([12.0, 4.0])
                .show(ui, rows);
        });
}

/// Draws the control for one property and returns its new value if the
/// user changed it this frame.
fn value_control(ui: &mut egui::Ui, name: &str, value: Value) -> Option<Value> {
    match value {
        Value::Bool(mut on) => ui
            .checkbox(&mut on, "")
            .changed()
            .then_some(Value::Bool(on)),
        Value::Number(mut number) => {
            let mut drag = egui::DragValue::new(&mut number).max_decimals(MAX_DECIMALS);
            drag = match name {
                // These only make sense from 0 to 1.
                "Metalness" | "Roughness" => drag.speed(0.01).range(0.0..=1.0),
                _ => drag.speed(0.1),
            };
            // `clamp_existing_to_range(false)`: only clamp what the user
            // types or drags, never a value that was already there, so just
            // selecting a part can't change it.
            let drag = drag.clamp_existing_to_range(false);
            ui.add(drag).changed().then_some(Value::Number(number))
        }
        Value::String(mut text) => ui
            .text_edit_singleline(&mut text)
            .changed()
            .then_some(Value::String(text)),
        Value::Vector3(mut vector) => {
            let (speed, min) = match name {
                // Degrees, so a bigger step per point dragged.
                "Orientation" => (1.0, f32::NEG_INFINITY),
                "Size" => (0.1, MIN_SIZE),
                _ => (0.1, f32::NEG_INFINITY),
            };
            let mut changed = false;
            ui.horizontal(|ui| {
                for (axis, number) in [
                    ("X ", &mut vector.x),
                    ("Y ", &mut vector.y),
                    ("Z ", &mut vector.z),
                ] {
                    let drag = egui::DragValue::new(number)
                        .prefix(axis)
                        .speed(speed)
                        .range(min..=f32::INFINITY)
                        .clamp_existing_to_range(false)
                        .max_decimals(MAX_DECIMALS);
                    // `|=` keeps `changed` true once any axis changes.
                    changed |= ui.add(drag).changed();
                }
            });
            changed.then_some(Value::Vector3(vector))
        }
        Value::Color3(color) => color_control(ui, color).map(Value::Color3),
        Value::PartType(shape) => {
            choice_control(ui, name, shape, &PartType::ALL, |s| s.as_str()).map(Value::PartType)
        }
        Value::Material(material) => {
            choice_control(ui, name, material, &Material::ALL, |m| m.as_str()).map(Value::Material)
        }
    }
}

/// A color swatch that opens a color picker, with the 0–255 values beside it.
fn color_control(ui: &mut egui::Ui, color: Color3) -> Option<Color3> {
    // `Color3` is sRGB, but egui's float color picker works in linear light,
    // so convert there and back.
    use egui::ecolor::{gamma_from_linear, linear_from_gamma};
    let mut linear = [color.r, color.g, color.b].map(linear_from_gamma);
    let mut picked = None;
    ui.horizontal(|ui| {
        if ui.color_edit_button_rgb(&mut linear).changed() {
            let [r, g, b] = linear.map(gamma_from_linear);
            picked = Some(Color3::new(r, g, b));
        }
        let shown = picked.unwrap_or(color);
        let byte = |channel: f32| (channel * 255.0).round() as u8;
        ui.weak(format!(
            "{}, {}, {}",
            byte(shown.r),
            byte(shown.g),
            byte(shown.b)
        ));
    });
    picked
}

/// A dropdown to pick one item of an enum such as `Material`.
fn choice_control<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    name: &str,
    current: T,
    items: &[T],
    label: fn(T) -> &'static str,
) -> Option<T> {
    let mut chosen = current;
    egui::ComboBox::from_id_salt(name)
        .selected_text(label(current))
        .show_ui(ui, |ui| {
            for &item in items {
                ui.selectable_value(&mut chosen, item, label(item));
            }
        });
    (chosen != current).then_some(chosen)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axcenity_core::ClassName;

    fn titles(sections: &[Section]) -> Vec<&'static str> {
        sections.iter().map(|section| section.title).collect()
    }

    #[test]
    fn part_properties_are_grouped_like_studio_and_none_are_left_out() {
        let mut world = World::new();
        let part = world.create(ClassName::Part).unwrap();
        let instance = world.get(part).unwrap();

        let sections = sections(instance);
        assert_eq!(titles(&sections), ["Appearance", "Behavior", "Part"]);
        let names: Vec<&str> = sections
            .iter()
            .flat_map(|section| section.properties.iter().map(|(name, _)| *name))
            .collect();
        assert_eq!(
            names,
            [
                "Color",
                "Material",
                "Metalness",
                "Roughness",
                "Anchored",
                "Shape",
                "Position",
                "Orientation",
                "Size"
            ]
        );
        // Every property shows exactly once, with its current value.
        assert_eq!(names.len(), instance.properties().count());
        for section in &sections {
            for (name, value) in &section.properties {
                assert_eq!(instance.property(name), Some(value));
            }
        }
    }

    #[test]
    fn services_have_no_property_sections() {
        let world = World::new();
        let workspace = world.get(world.workspace()).unwrap();
        assert!(sections(workspace).is_empty());
    }

    #[test]
    fn parts_can_be_renamed_here_scripts_only_in_the_explorer_and_services_never() {
        let mut world = World::new();
        let part = world.create(ClassName::Part).unwrap();
        let script = world.create(ClassName::Script).unwrap();
        assert!(can_rename_here(world.get(part).unwrap()));
        assert!(can_rename(world.get(script).unwrap()));
        assert!(!can_rename_here(world.get(script).unwrap()));
        assert!(!can_rename(world.get(world.workspace()).unwrap()));
        assert!(!can_rename(world.get(world.root()).unwrap()));
    }
}
