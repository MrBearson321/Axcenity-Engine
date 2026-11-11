//! The Tags panel: every tag an Instance in the world has, with how many
//! have it. A row's Select button selects all of them; Rename renames the
//! tag on every one, as one undo step (the app does both, see
//! `app/tags.rs`).

use std::collections::BTreeMap;

use eframe::egui::{self, RichText};

use crate::theme::{self, icons};
use crate::widgets;

/// What the panel asked the app to do.
#[derive(Debug, Default, PartialEq, Eq)]
pub enum TagAction {
    #[default]
    None,
    /// Select every Instance with this tag.
    Select(String),
    /// Rename the tag (first) to the second, everywhere.
    Rename(String, String),
}

/// The panel's own state: the tag being renamed and the name typed so far.
#[derive(Default)]
pub struct TagsPanel {
    renaming: Option<(String, String)>,
}

impl TagsPanel {
    /// Opens the rename box on `tag`'s row, as its Rename button does.
    pub fn start_renaming(&mut self, tag: &str) {
        self.renaming = Some((tag.to_owned(), tag.to_owned()));
    }

    /// Draws the panel for `tags` (each tag and how many have it).
    pub fn show(&mut self, ui: &mut egui::Ui, tags: &BTreeMap<String, usize>) -> TagAction {
        let palette = theme::palette(ui.ctx());
        let mut action = TagAction::None;
        if tags.is_empty() {
            crate::widgets::empty_state(
                ui,
                icons::TAG,
                "No tags",
                "Add one in Properties › Tags, or from a script with instance:AddTag(\"Lava\").",
                None,
            );
            return action;
        }
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                for (tag, count) in tags {
                    let renaming = self
                        .renaming
                        .as_mut()
                        .filter(|(old, _)| old == tag)
                        .map(|(_, name)| name);
                    match renaming {
                        Some(name) => {
                            let mut finished = None;
                            ui.horizontal(|ui| {
                                let response = ui.add(
                                    egui::TextEdit::singleline(name)
                                        .desired_width(ui.available_width() - 60.0),
                                );
                                response.request_focus();
                                let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                                let escape = ui.input(|i| i.key_pressed(egui::Key::Escape));
                                let new = name.trim().to_owned();
                                if widgets::icon_button(ui, icons::CHECK, "Rename", false).clicked()
                                    || enter
                                {
                                    finished = Some(Some(new));
                                } else if widgets::icon_button(ui, icons::X, "Cancel", false)
                                    .clicked()
                                    || escape
                                {
                                    finished = Some(None);
                                }
                            });
                            match finished {
                                Some(Some(new)) => {
                                    if !new.is_empty() && new != *tag {
                                        action = TagAction::Rename(tag.clone(), new);
                                    }
                                    self.renaming = None;
                                }
                                Some(None) => self.renaming = None,
                                None => {}
                            }
                        }
                        None => {
                            // Buttons and count first, from the right, so a long
                            // tag is cut short ("…") instead of pushing them out.
                            let row = egui::Layout::right_to_left(egui::Align::Center);
                            ui.allocate_ui_with_layout(
                                egui::vec2(ui.available_width(), ui.spacing().interact_size.y),
                                row,
                                |ui| {
                                    let rename = format!("Rename {tag} everywhere");
                                    if widgets::icon_button(
                                        ui,
                                        icons::PENCIL_SIMPLE,
                                        &rename,
                                        false,
                                    )
                                    .clicked()
                                    {
                                        self.start_renaming(tag);
                                    }
                                    let select = format!("Select everything tagged {tag}");
                                    if widgets::icon_button(ui, icons::CURSOR_CLICK, &select, false)
                                        .clicked()
                                    {
                                        action = TagAction::Select(tag.clone());
                                    }
                                    ui.label(
                                        RichText::new(count.to_string()).color(palette.text_weak),
                                    );
                                    ui.with_layout(
                                        egui::Layout::left_to_right(egui::Align::Center),
                                        |ui| {
                                            ui.label(
                                                RichText::new(icons::TAG).color(palette.text_muted),
                                            );
                                            ui.add(egui::Label::new(tag.as_str()).truncate());
                                        },
                                    );
                                },
                            );
                        }
                    }
                }
            });
        action
    }
}
