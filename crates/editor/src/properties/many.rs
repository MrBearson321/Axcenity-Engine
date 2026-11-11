//! Properties with several Instances selected (design system V5): the
//! properties they all have, each edit made to every one of them as one
//! undo step by the app (`EditorApp::apply_many`). A value that differs
//! between them shows as `—` and is never written back just by showing:
//! a checkbox shows half-ticked, and anything else shows its control (the
//! last one clicked's value) only once `—` is clicked.

use super::*;

/// What the header calls `count` Instances: "3 Parts" when they're one
/// class, otherwise "3 objects".
pub fn count_name(world: &World, ids: &[InstanceId]) -> String {
    let mut classes = ids
        .iter()
        .filter_map(|id| world.get(*id))
        .map(Instance::class);
    let first = classes.next();
    match first {
        Some(class) if classes.all(|other| other == class) => {
            format!("{} {}", ids.len(), plural(class.as_str()))
        }
        _ => format!("{} objects", ids.len()),
    }
}

fn plural(class: &str) -> String {
    if ["s", "x", "ch", "sh"]
        .iter()
        .any(|end| class.ends_with(end))
    {
        format!("{class}es")
    } else {
        format!("{class}s")
    }
}

/// How many classes there are among `instances`.
fn kinds(instances: &[(InstanceId, &Instance)]) -> usize {
    let mut classes: Vec<_> = instances.iter().map(|(_, i)| i.class()).collect();
    classes.sort_by_key(|class| class.as_str());
    classes.dedup();
    classes.len()
}

/// Whether every one of `others` has `name`, holding the same kind of
/// value as `value`, and if so whether any holds a different one.
fn shared(others: &[(InstanceId, &Instance)], name: &str, value: &Value) -> Option<bool> {
    let mut mixed = false;
    for (_, other) in others {
        let theirs = other.property(name)?;
        if std::mem::discriminant(theirs) != std::mem::discriminant(value) {
            return None;
        }
        mixed |= theirs != value;
    }
    Some(mixed)
}

/// Properties that point at one thing or are the editor's own pick aren't
/// shared: an ObjectValue's target, a seat's Occupant, Terrain's folder.
fn editable_together(name: &str, value: &Value) -> bool {
    !matches!(value, Value::Instance(_)) && name != "DataFolder"
}

/// Draws the panel for `ids` (two or more; the last is the one clicked
/// last) and returns the edit made this frame: a [`Edit::Name`],
/// [`Edit::Property`] or [`Edit::Reset`] for all of them, or a button's
/// edit (a file picker, Open) for the app to carry out.
pub fn show_many(
    ui: &mut egui::Ui,
    world: &World,
    assets: &Assets,
    ids: &[InstanceId],
    playing: bool,
) -> Option<Edit> {
    let instances: Vec<(InstanceId, &Instance)> = ids
        .iter()
        .filter_map(|&id| Some((id, world.get(id)?)))
        .collect();
    let (&(_, primary), others) = instances.split_last()?;
    let palette = theme::palette(ui.ctx());
    if playing {
        notice(
            ui,
            icons::INFO,
            "Changes made during Play are undone at Stop, unless you keep them: right-click \
             the objects in the Explorer and pick Keep changes.",
        );
    }
    let count = instances.len();
    let kinds = kinds(&instances);
    let locked = if playing {
        0
    } else {
        instances
            .iter()
            .filter(|(id, _)| world.is_locked(*id))
            .count()
    };
    let mut edit = None;
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let (icon, color) = if kinds == 1 {
                    (
                        theme::class_icon(primary.class()),
                        theme::class_color(primary.class(), ui.visuals()),
                    )
                } else {
                    (icons::STACK, palette.text_weak)
                };
                ui.label(egui::RichText::new(icon).size(16.0).color(color));
                let title = if kinds == 1 {
                    count_name(world, ids)
                } else {
                    format!("{count} selected · {kinds} kinds")
                };
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(title)
                            .family(theme::heading_family())
                            .color(palette.text_strong),
                    )
                    .truncate(),
                );
            });
            let mut about = String::from("Changes go to all of them; — marks values that differ.");
            if kinds > 1 {
                about.push_str(" Only what they all have is shown.");
            }
            if locked > 0 {
                about.push_str(&format!(
                    " {locked} of them {} locked and only Locked changes on {}.",
                    if locked == 1 { "is" } else { "are" },
                    if locked == 1 { "it" } else { "them" },
                ));
            }
            about.push_str(" Attributes, tags and Style show with one selected.");
            notice(ui, icons::STACK, &about);

            // The same filter as with one selected, kept between selections.
            let filter_id = egui::Id::new("properties filter");
            let mut filter: String = ui.data_mut(|d| d.get_temp(filter_id)).unwrap_or_default();
            let width = (ui.available_width() - crate::widgets::SEARCH_FIELD_MARGINS).max(60.0);
            crate::widgets::search_field(ui, &mut filter, "Filter properties", width)
                .on_hover_text("Show only the properties whose names have this in them");
            ui.data_mut(|d| d.insert_temp(filter_id, filter.clone()));
            let filter = filter.trim();
            let filtering = !filter.is_empty();
            let open = filtering.then_some(true);
            let shows = |title: &str, name: &str| {
                !filtering || matches_filter(name, filter) || matches_filter(title, filter)
            };
            let all_locked = locked == count;
            let class = primary.class();

            // Half-typed values and which `—` were opened belong to this
            // selection only.
            ui.push_id(("properties of", ids), |ui| {
                let mut shown_any = false;
                // Name, when every one of them can be renamed here.
                let renamable = instances.iter().all(|(_, i)| can_rename_here(i));
                if renamable && shows("Data", "Name") {
                    shown_any = true;
                    section(ui, "Data", open, true, |ui| {
                        described_name(ui, class, "Name");
                        let mixed = others.iter().any(|(_, i)| i.name() != primary.name());
                        let opened_id = ui.id().with("show mixed Name");
                        let opened = ui.data(|d| d.get_temp::<bool>(opened_id)).unwrap_or(false);
                        ui.add_enabled_ui(!all_locked, |ui| {
                            if mixed && !opened {
                                let dash = egui::RichText::new("—").color(palette.text_weak);
                                if ui
                                    .add(
                                        egui::Button::new(dash).min_size(egui::vec2(
                                            ui.available_width().min(120.0),
                                            0.0,
                                        )),
                                    )
                                    .on_hover_text(
                                        "Their names differ. Click to give them all one name.",
                                    )
                                    .clicked()
                                {
                                    ui.data_mut(|d| d.insert_temp(opened_id, true));
                                }
                            } else {
                                let mut name = primary.name().to_string();
                                if ui.text_edit_singleline(&mut name).changed() {
                                    edit = Some(Edit::Name(name));
                                }
                            }
                        });
                        ui.end_row();
                    });
                }

                let mut row =
                    |ui: &mut egui::Ui, name: &'static str, value: Value, mixed: bool| {
                        // ↺ when any of them differs from its class's default;
                        // it resets each to its own.
                        let differs_from_default = |(id, instance): &(InstanceId, &Instance)| {
                            if playing || (world.is_locked(*id) && name != "Locked") {
                                return None;
                            }
                            let default = instance
                                .class()
                                .default_properties()
                                .into_iter()
                                .find(|(property, _)| *property == name)?
                                .1;
                            (resettable(&default) && instance.property(name) != Some(&default))
                                .then_some(default)
                        };
                        match instances.iter().rev().find_map(differs_from_default) {
                            Some(default) => {
                                if field_name_with_reset(ui, class, name, &default) {
                                    edit = Some(Edit::Reset(name));
                                }
                            }
                            None => described_name(ui, class, name),
                        }
                        let opened_id = ui.id().with(("show mixed", name));
                        let opened = ui.data(|d| d.get_temp::<bool>(opened_id)).unwrap_or(false);
                        let enabled = !all_locked || name == "Locked";
                        let made = ui
                            .add_enabled_ui(enabled, |ui| match value {
                                Value::Bool(_) if mixed => {
                                    let mut on = false;
                                    ui.add(egui::Checkbox::new(&mut on, "").indeterminate(true))
                                        .on_hover_text(
                                            "They differ. Click to turn it on for all of them.",
                                        )
                                        .clicked()
                                        .then_some(Edit::Property(name, Value::Bool(true)))
                                }
                                _ if mixed && !opened => {
                                    let dash = egui::RichText::new("—").color(palette.text_weak);
                                    if ui
                                        .add(egui::Button::new(dash).min_size(egui::vec2(
                                            ui.available_width().min(120.0),
                                            0.0,
                                        )))
                                        .on_hover_text(
                                            "They differ. Click to set one value for all of them.",
                                        )
                                        .clicked()
                                    {
                                        ui.data_mut(|d| d.insert_temp(opened_id, true));
                                    }
                                    None
                                }
                                Value::PhysicalProperties(custom) => {
                                    physical_control(ui, primary, custom).map(|custom| {
                                        Edit::Property(name, Value::PhysicalProperties(custom))
                                    })
                                }
                                value => value_control(ui, name, value, assets),
                            })
                            .inner;
                        if made.is_some() {
                            edit = made;
                        }
                        ui.end_row();
                    };

                let mut advanced = Vec::new();
                for group in sections(primary) {
                    let mut common = Vec::new();
                    for (name, value) in group.properties {
                        if !shows(group.title, name) || !editable_together(name, &value) {
                            continue;
                        }
                        let Some(mixed) = shared(others, name, &value) else {
                            continue;
                        };
                        if is_advanced(class, name) {
                            advanced.push((name, value, mixed));
                        } else {
                            common.push((name, value, mixed));
                        }
                    }
                    if common.is_empty() {
                        continue;
                    }
                    shown_any = true;
                    section(ui, group.title, open, true, |ui| {
                        for (name, value, mixed) in common {
                            row(ui, name, value, mixed);
                        }
                    });
                }
                if !advanced.is_empty() {
                    shown_any = true;
                    section_titled(ui, ADVANCED, ADVANCED, false, open, |ui| {
                        for (name, value, mixed) in advanced {
                            row(ui, name, value, mixed);
                        }
                    });
                }
                if !shown_any {
                    ui.add_space(8.0);
                    if filtering {
                        ui.weak(format!("No properties they all have match \"{filter}\"."));
                    } else {
                        ui.weak("They have no properties in common.");
                    }
                }
            });
        });
    edit
}

/// The properties [`show_many`] shows for `ids`, unfiltered, with whether
/// each one's value differs between them.
#[cfg(test)]
pub fn shared_names(world: &World, ids: &[InstanceId]) -> Vec<(&'static str, bool)> {
    let instances: Vec<(InstanceId, &Instance)> = ids
        .iter()
        .filter_map(|&id| Some((id, world.get(id)?)))
        .collect();
    let Some((&(_, primary), others)) = instances.split_last() else {
        return Vec::new();
    };
    sections(primary)
        .into_iter()
        .flat_map(|section| section.properties)
        .filter(|(name, value)| editable_together(name, value))
        .filter_map(|(name, value)| Some((name, shared(others, name, &value)?)))
        .collect()
}
