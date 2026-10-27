use super::*;

pub(super) fn object_menu(
    ui: &mut egui::Ui,
    path: &mut String,
    filter: Filter,
    objects: &[ObjectInfo],
) {
    let mut any = false;
    egui::ScrollArea::vertical()
        .max_height(280.0)
        .show(ui, |ui| {
            if filter == Filter::Places {
                any = true;
                let text = theme::with_icon(theme::class_icon(ClassName::Workspace), "Workspace");
                if ui.selectable_label(path.is_empty(), text).clicked() {
                    path.clear();
                    ui.close();
                }
            }
            for object in objects.iter().filter(|object| filter.matches(object.class)) {
                any = true;
                let text = theme::with_icon(theme::class_icon(object.class), &object.path);
                if ui.selectable_label(*path == object.path, text).clicked() {
                    *path = object.path.clone();
                    ui.close();
                }
            }
        });
    if !any {
        ui.weak("No objects of this kind in the Workspace yet.");
    }
    ui.separator();
    ui.weak("Or type its path (Model.Part):");
    ui.text_edit_singleline(path);
}

pub(super) fn property_menu(ui: &mut egui::Ui, name: &mut String, class: Option<ClassName>) {
    let names: Vec<&'static str> = match class {
        Some(class) => class
            .default_properties()
            .into_iter()
            .map(|(name, _)| name)
            .collect(),
        None => vec![
            "Position",
            "Size",
            "Orientation",
            "Color",
            "Anchored",
            "Visible",
        ],
    };
    egui::ScrollArea::vertical()
        .max_height(280.0)
        .show(ui, |ui| {
            for property in names {
                if ui.selectable_label(name == property, property).clicked() {
                    *name = property.to_owned();
                    ui.close();
                }
            }
        });
    ui.separator();
    ui.text_edit_singleline(name);
}

pub(super) fn variable_menu(ui: &mut egui::Ui, name: &mut String, variables: &[String]) {
    for variable in variables {
        if ui.selectable_label(name == variable, variable).clicked() {
            *name = variable.clone();
            ui.close();
        }
    }
    ui.separator();
    ui.weak("Rename:");
    ui.text_edit_singleline(name);
    if !valid_identifier(name) {
        ui.colored_label(
            ui.visuals().error_fg_color,
            "Letters, digits and _ only, not starting with a digit",
        );
    }
}

/// The names the Animator's controller has of this kind, and a box to type
/// another.
pub(super) fn name_menu(
    ui: &mut egui::Ui,
    name: &mut String,
    kind: NameKind,
    suggestions: &[String],
) {
    for suggestion in suggestions {
        if ui
            .selectable_label(name == suggestion, suggestion)
            .clicked()
        {
            *name = suggestion.clone();
            ui.close();
        }
    }
    if suggestions.is_empty() && kind == NameKind::Child {
        ui.weak("Nothing is in there yet in the editor.");
    } else if suggestions.is_empty() {
        ui.weak(format!(
            "Its state machine has no {} yet (or pick the Animator first).",
            kind.what()
        ));
    }
    ui.separator();
    ui.weak(format!("Or type the {}'s name:", kind.what()));
    ui.text_edit_singleline(name);
}

pub(super) fn key_menu(ui: &mut egui::Ui, key: &mut String) {
    egui::ScrollArea::vertical()
        .max_height(280.0)
        .show(ui, |ui| {
            for code in KeyCode::ALL {
                let name = code.as_str();
                if ui.selectable_label(key == name, name).clicked() {
                    *key = name.to_owned();
                    ui.close();
                }
            }
        });
}
