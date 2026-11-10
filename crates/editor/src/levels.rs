//! The Levels panel (Phase 48): the start level (`world.json`) and each
//! level in `levels/`, with the open one marked. A row opens its level
//! (asking about unsaved changes first), duplicates, renames, deletes it or
//! makes it the start level; New adds an empty level, and Play from Start
//! Level makes Play start there whichever level is open. The app does the
//! work, see `app/levels.rs`.

use eframe::egui::{self, RichText};

use crate::theme::{self, icons};
use crate::{responsive, widgets};

/// What the panel asked the app to do.
#[derive(Debug, Default, PartialEq, Eq)]
pub enum LevelAction {
    #[default]
    None,
    /// Open this level (`None`: the start level).
    Open(Option<String>),
    /// Add an empty level.
    New,
    /// Copy this level to a new one.
    Duplicate(String),
    /// Rename the level (first) to the second.
    Rename(String, String),
    Delete(String),
    /// Make this level the start level.
    SetStart(String),
}

/// What the panel shows: the start level's name, the others, and the open
/// one (`None`: the start level).
pub struct LevelList<'a> {
    pub start: &'a str,
    pub others: &'a [String],
    pub open: Option<&'a str>,
}

/// The panel's own state: the level being renamed and the name typed so far.
#[derive(Default)]
pub struct LevelsPanel {
    renaming: Option<(String, String)>,
}

impl LevelsPanel {
    /// Opens the rename box on `level`'s row, as its Rename button does.
    pub fn start_renaming(&mut self, level: &str) {
        self.renaming = Some((level.to_owned(), level.to_owned()));
    }

    /// Draws the panel. `play_from_start` is the Play from Start Level box.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        levels: &LevelList,
        play_from_start: &mut bool,
    ) -> LevelAction {
        let palette = theme::palette(ui.ctx());
        let mut action = LevelAction::None;
        responsive::toolbar(ui, |ui| {
            if ui
                .add(widgets::button(
                    widgets::Role::Secondary,
                    Some(icons::PLUS),
                    "New Level",
                    &palette,
                ))
                .on_hover_text("Add an empty level in levels/")
                .clicked()
            {
                action = LevelAction::New;
            }
            ui.checkbox(play_from_start, "Play from start level")
                .on_hover_text(format!(
                    "Play starts in {} even while another level is open",
                    levels.start
                ));
        });
        ui.add_space(4.0);
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                let rows =
                    std::iter::once(levels.start).chain(levels.others.iter().map(String::as_str));
                for name in rows {
                    let start = name == levels.start;
                    let open = match levels.open {
                        Some(open) => open == name,
                        None => start,
                    };
                    if let Some(row) = self.row(ui, name, start, open, &palette) {
                        action = row;
                    }
                }
            });
        action
    }

    /// One level's row, or its rename box.
    fn row(
        &mut self,
        ui: &mut egui::Ui,
        name: &str,
        start: bool,
        open: bool,
        palette: &theme::Palette,
    ) -> Option<LevelAction> {
        if let Some((_, typed)) = self.renaming.as_mut().filter(|(old, _)| old == name) {
            let mut finished = None;
            ui.horizontal(|ui| {
                let response = ui.add(
                    egui::TextEdit::singleline(typed).desired_width(ui.available_width() - 60.0),
                );
                response.request_focus();
                let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                let escape = ui.input(|i| i.key_pressed(egui::Key::Escape));
                if widgets::icon_button(ui, icons::CHECK, "Rename", false).clicked() || enter {
                    finished = Some(Some(typed.trim().to_owned()));
                } else if widgets::icon_button(ui, icons::X, "Cancel", false).clicked() || escape {
                    finished = Some(None);
                }
            });
            return match finished {
                Some(new) => {
                    self.renaming = None;
                    new.filter(|new| !new.is_empty() && new != name)
                        .map(|new| LevelAction::Rename(name.to_owned(), new))
                }
                None => None,
            };
        }
        let mut action = None;
        // Buttons first, from the right, so a long name is cut short ("…")
        // instead of pushing them out.
        let row = egui::Layout::right_to_left(egui::Align::Center);
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), ui.spacing().interact_size.y),
            row,
            |ui| {
                // The open level and the start level stay where they are.
                if !start
                    && !open
                    && widgets::icon_button(ui, icons::TRASH, "Delete", false).clicked()
                {
                    action = Some(LevelAction::Delete(name.to_owned()));
                }
                if !start
                    && widgets::icon_button(ui, icons::FLAG, "Set as start level", false).clicked()
                {
                    action = Some(LevelAction::SetStart(name.to_owned()));
                }
                if (start || !open)
                    && widgets::icon_button(ui, icons::PENCIL_SIMPLE, "Rename", false).clicked()
                {
                    self.start_renaming(name);
                }
                if widgets::icon_button(ui, icons::COPY, "Duplicate", false).clicked() {
                    action = Some(LevelAction::Duplicate(name.to_owned()));
                }
                if !open && widgets::icon_button(ui, icons::FOLDER_OPEN, "Open", false).clicked() {
                    action = Some(LevelAction::Open((!start).then(|| name.to_owned())));
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    let icon = if start { icons::FLAG } else { icons::STACK };
                    ui.label(RichText::new(icon).color(palette.text_weak));
                    let text = if open {
                        RichText::new(name).strong().color(palette.accent_text)
                    } else {
                        RichText::new(name)
                    };
                    let label = ui.add(
                        egui::Label::new(text)
                            .truncate()
                            .sense(egui::Sense::click()),
                    );
                    let file = if start {
                        axcenity_project::WORLD_FILE.to_owned()
                    } else {
                        axcenity_project::level_path(name)
                    };
                    let hint = match (start, open) {
                        (true, true) => format!("The start level, open ({file})"),
                        (true, false) => {
                            format!("The start level ({file}). Double-click to open it")
                        }
                        (false, true) => format!("Open ({file})"),
                        (false, false) => format!("{file}. Double-click to open it"),
                    };
                    let label = label.on_hover_text(hint);
                    if label.double_clicked() && !open {
                        action = Some(LevelAction::Open((!start).then(|| name.to_owned())));
                    }
                });
            },
        );
        action
    }
}
