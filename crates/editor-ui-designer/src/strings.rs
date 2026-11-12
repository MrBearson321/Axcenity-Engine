//! The string table in the UI Designer: the **Strings** tab, where the
//! project's languages and keys are added and each key's text typed per
//! language; the toolbar's **Language**, which the artboard (and Play)
//! show; and the inspector's **Key**, which binds a label or button's text
//! to an entry. The table is the world's (`World::strings`) and is saved
//! as `strings.json`.

use axcenity_core::{ClassName, InstanceId, StringTable, Value};
use eframe::egui;

use super::{DesignerAction, DesignerScene, UiDesigner};
use crate::edit::Edit;
use crate::theme::{self, icons};
use crate::{responsive, widgets};

/// What a language or key typed in the Strings tab becomes: trimmed, and
/// `None` when nothing's left.
fn trimmed(text: &str) -> Option<String> {
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

/// The class names whose `Text` can be bound to the string table.
pub fn has_text_key(class: ClassName) -> bool {
    matches!(class, ClassName::TextLabel | ClassName::TextButton)
}

impl UiDesigner {
    /// The Strings tab: the languages, then each key with its text in each
    /// language. Each change is one undo step; typing in one box is one.
    pub(super) fn strings_tab(
        &mut self,
        ui: &mut egui::Ui,
        scene: &DesignerScene,
        actions: &mut Vec<DesignerAction>,
    ) {
        let table = scene.world.strings();
        let mut changed: Option<(String, String, StringTable)> = None;
        egui::ScrollArea::vertical()
            .id_salt("ui designer strings")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add_enabled_ui(scene.editing, |ui| {
                    ui.weak(
                        "Text in each language. A label or button whose Key names an entry \
                         shows it in the language picked above (the first when the game \
                         starts), or its own Text when there's none. Saved as strings.json.",
                    );
                    ui.add_space(4.0);
                    widgets::section_label(ui, "Languages");
                    responsive::toolbar(ui, |ui| {
                        for language in &table.languages {
                            responsive::room_for(ui, 120.0);
                            ui.label(language);
                            if ui
                                .small_button(icons::X)
                                .on_hover_text(format!("Remove {language} and all its text"))
                                .clicked()
                            {
                                let mut edited = (**table).clone();
                                edited.remove_language(language);
                                changed = Some((
                                    String::new(),
                                    format!("Remove language {language}"),
                                    edited,
                                ));
                            }
                        }
                    });
                    responsive::toolbar(ui, |ui| {
                        let typed = ui.add(
                            egui::TextEdit::singleline(&mut self.new_language)
                                .hint_text("Language, e.g. English")
                                .desired_width(160.0),
                        );
                        let entered = typed.lost_focus()
                            && ui.input(|input| input.key_pressed(egui::Key::Enter));
                        let language = trimmed(&self.new_language)
                            .filter(|language| !table.has_language(language));
                        let add = ui.add_enabled(
                            language.is_some(),
                            egui::Button::new(theme::with_icon(icons::PLUS, "Add language")),
                        );
                        if let Some(language) = language
                            && (add.clicked() || entered)
                        {
                            let mut edited = (**table).clone();
                            edited.add_language(&language);
                            changed =
                                Some((String::new(), format!("Add language {language}"), edited));
                            self.new_language.clear();
                        }
                    });
                    ui.add_space(6.0);
                    widgets::section_label(ui, "Keys");
                    responsive::toolbar(ui, |ui| {
                        let typed = ui.add(
                            egui::TextEdit::singleline(&mut self.new_key)
                                .hint_text("Key, such as menu.play")
                                .desired_width(160.0),
                        );
                        let entered = typed.lost_focus()
                            && ui.input(|input| input.key_pressed(egui::Key::Enter));
                        let key =
                            trimmed(&self.new_key).filter(|key| !table.entries.contains_key(key));
                        let add = ui.add_enabled(
                            key.is_some(),
                            egui::Button::new(theme::with_icon(icons::PLUS, "Add key")),
                        );
                        if let Some(key) = key
                            && (add.clicked() || entered)
                        {
                            let mut edited = (**table).clone();
                            edited.entries.entry(key.clone()).or_default();
                            changed = Some((String::new(), format!("Add key {key}"), edited));
                            self.new_key.clear();
                        }
                    });
                    if table.languages.is_empty() && !table.entries.is_empty() {
                        ui.weak("Add a language to type each key's text in it.");
                    }
                    for (key, values) in &table.entries {
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            ui.strong(key);
                            if ui
                                .small_button(icons::TRASH)
                                .on_hover_text(
                                    "Delete this key. Elements bound to it show their own Text.",
                                )
                                .clicked()
                            {
                                let mut edited = (**table).clone();
                                edited.entries.remove(key);
                                changed =
                                    Some((String::new(), format!("Delete key {key}"), edited));
                            }
                        });
                        responsive::form(ui, ("ui designer string", key), |ui| {
                            for language in &table.languages {
                                ui.label(language);
                                let mut text = values.get(language).cloned().unwrap_or_default();
                                if ui
                                    .add(
                                        egui::TextEdit::singleline(&mut text)
                                            .desired_width(f32::INFINITY),
                                    )
                                    .changed()
                                {
                                    let mut edited = (**table).clone();
                                    edited.set(key, language, &text);
                                    changed = Some((
                                        format!("{key}\n{language}"),
                                        format!("Edit {key} ({language})"),
                                        edited,
                                    ));
                                }
                                ui.end_row();
                            }
                        });
                    }
                });
            });
        if let Some((key, label, table)) = changed {
            actions.push(DesignerAction::Strings { key, label, table });
        }
    }
}

/// The toolbar's language picker: which language the artboard shows bound
/// text in. Not saved and not an undo step; Play starts in it. Nothing
/// when the table has no languages.
pub(super) fn language_picker(
    ui: &mut egui::Ui,
    scene: &DesignerScene,
    actions: &mut Vec<DesignerAction>,
) {
    let languages = &scene.world.strings().languages;
    if languages.is_empty() {
        return;
    }
    let active = scene.world.language();
    responsive::room_for(ui, 170.0);
    ui.label(icons::TRANSLATE)
        .on_hover_text("The language bound text is shown in");
    egui::ComboBox::from_id_salt("ui designer language")
        .width(120.0)
        .selected_text(active)
        .show_ui(ui, |ui| {
            for language in languages {
                if ui.selectable_label(language == active, language).clicked() {
                    actions.push(DesignerAction::Language(language.clone()));
                }
            }
        });
}

/// The inspector's Key row for a label or button: none, or one of the
/// string table's keys. The form row's name is added here too.
pub(super) fn text_key_row(
    ui: &mut egui::Ui,
    scene: &DesignerScene,
    id: InstanceId,
    actions: &mut Vec<DesignerAction>,
) {
    let Some(instance) = scene.world.get(id) else {
        return;
    };
    if !has_text_key(instance.class()) {
        return;
    }
    let current = instance
        .property("TextKey")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let table = scene.world.strings();
    ui.label("Key")
        .on_hover_text("A key from the Strings tab: its text in the shown language replaces Text");
    ui.vertical(|ui| {
        let mut picked = None;
        egui::ComboBox::from_id_salt(("ui designer text key", id))
            .width(ui.available_width().min(200.0))
            .selected_text(if current.is_empty() { "None" } else { &current })
            .show_ui(ui, |ui| {
                if ui.selectable_label(current.is_empty(), "None").clicked() {
                    picked = Some(String::new());
                }
                for key in table.entries.keys() {
                    if ui.selectable_label(*key == current, key).clicked() {
                        picked = Some(key.clone());
                    }
                }
            });
        if let Some(key) = picked.filter(|key| *key != current) {
            actions.push(DesignerAction::Edit(
                id,
                Edit::Property("TextKey", Value::String(key)),
            ));
        }
        if !current.is_empty() {
            let language = scene.world.language();
            let note = match scene.world.translate(&current) {
                Some(_) => format!("Shows its {language} text"),
                None if !table.entries.contains_key(&current) => {
                    "Not in the string table: shows Text".to_owned()
                }
                None => format!("No {language} text yet: shows Text"),
            };
            ui.weak(note);
        } else if table.entries.is_empty() {
            ui.weak("Add keys in the Strings tab");
        }
    });
    ui.end_row();
}
