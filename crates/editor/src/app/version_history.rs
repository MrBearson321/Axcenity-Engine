//! File ▸ Version History (unified Phase 72): the timeline of saved
//! versions with their pins, what changed in the selected one, Save
//! Checkpoint, restoring a file or the whole project, and Back Up Project.
//! The versions themselves are `axcenity_project::versions`.

use axcenity_project::{BackupSettings, Change, ChangedFile, Changes, Line, Version};

use super::*;

/// Lines of context a script diff shows around each change.
const DIFF_CONTEXT: usize = 2;
/// The most diff lines drawn for one file.
const MAX_DIFF_LINES: usize = 400;
/// The timeline's share of the window when there's room beside the changes.
const TIMELINE_SHARE: f32 = 0.38;
/// The most Instance sentences, or assets, listed before "and N more".
const MAX_LISTED: usize = 30;

/// The Version History window's state.
#[derive(Default)]
pub(super) struct VersionHistory {
    pub(super) open: bool,
    /// What Save Checkpoint calls the next checkpoint.
    pub(super) message: String,
    /// Put the cursor in the message box on the next frame.
    focus_message: bool,
    /// The version whose changes are shown.
    selected: Option<String>,
    /// The selected version's changes, worked out once (or why they couldn't be).
    changes: Option<(String, Result<Changes, String>)>,
    /// A restore waiting for Restore or Cancel.
    pub(super) confirm: Option<Restore>,
}

/// A restore the creator asked for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Restore {
    /// Restore project to here.
    Project { id: String },
    /// Restore this file.
    File { id: String, path: String },
}

/// What the window asked for this frame, done once it's drawn.
enum Ask {
    Checkpoint,
    BackUp,
    ChooseBackupFolder,
    BackUpOnCheckpoint(bool),
    Select(String),
    Pin(String, bool),
    Restore(Restore),
}

impl VersionHistory {
    /// Opens the window with the cursor in the checkpoint message box.
    pub(super) fn open_for_checkpoint(&mut self) {
        self.open = true;
        self.focus_message = true;
    }

    /// The selected version's changes, worked out when the selection changed.
    fn changes_of(&mut self, folder: &Path, id: &str) -> &Result<Changes, String> {
        if self.changes.as_ref().is_none_or(|(shown, _)| shown != id) {
            let changes =
                axcenity_project::version_changes(folder, id).map_err(|err| err.to_string());
            self.changes = Some((id.to_owned(), changes));
        }
        &self.changes.as_ref().expect("just set").1
    }

    /// Forget worked-out changes, after versions were added or removed.
    fn refresh(&mut self) {
        self.changes = None;
    }
}

impl EditorApp {
    /// File ▸ Version History: the timeline, the selected version's changes,
    /// checkpoints, restoring and backups.
    pub(super) fn version_window(&mut self, ctx: &egui::Context) {
        let Some(folder) = self.project.as_ref().map(|project| project.folder.clone()) else {
            self.version_history.open = false;
            return;
        };
        let palette = theme::palette(ctx);
        let versions = axcenity_project::list_versions(&folder);
        let selected = self
            .version_history
            .selected
            .clone()
            .filter(|id| versions.iter().any(|version| &version.id == id))
            .or_else(|| versions.first().map(|version| version.id.clone()));
        let unsaved = self.unsaved();
        let backup = axcenity_project::backup_settings(&folder);
        let mut asks = Vec::new();
        let mut open = self.version_history.open;
        widgets::tool_window(icons::CLOCK_COUNTER_CLOCKWISE, "Version History")
            .size(egui::vec2(760.0, 520.0))
            .show(ctx, &mut open, |ui| {
                ui.weak(
                    "Each save keeps the world, levels, scripts and styles. A checkpoint is \
                     a save with a message that's never dropped. Assets and the project \
                     settings aren't put back by a restore.",
                );
                ui.add_space(6.0);
                checkpoint_row(ui, &mut self.version_history, &palette, &mut asks);
                backup_row(ui, &backup, &palette, &mut asks);
                ui.separator();
                if versions.is_empty() {
                    ui.weak("No versions yet. Save the project to keep the first one.");
                    return;
                }
                let history = &mut self.version_history;
                crate::responsive::split(ui, 560.0, TIMELINE_SHARE, |ui, pane| {
                    if pane == 0 {
                        timeline(ui, &versions, selected.as_deref(), &palette, &mut asks);
                    } else if let Some(version) = selected
                        .as_deref()
                        .and_then(|id| versions.iter().find(|version| version.id == id))
                    {
                        let changes = history.changes_of(&folder, &version.id);
                        change_view(ui, version, changes, unsaved, &palette, &mut asks);
                    }
                });
            });
        self.version_history.open = open;
        // Closed, it opens on the newest version next time.
        self.version_history.selected = selected.filter(|_| open);

        for ask in asks {
            match ask {
                Ask::Checkpoint => self.save_checkpoint(),
                Ask::BackUp => self.back_up_project(false),
                Ask::ChooseBackupFolder => self.back_up_project(true),
                Ask::BackUpOnCheckpoint(on) => {
                    let settings = BackupSettings {
                        on_checkpoint: on,
                        ..backup.clone()
                    };
                    if let Err(err) = axcenity_project::save_backup_settings(&folder, &settings) {
                        self.output
                            .warning(format!("Could not keep the backup setting: {err}"));
                    }
                }
                Ask::Select(id) => self.version_history.selected = Some(id),
                Ask::Pin(id, pinned) => {
                    if let Err(err) = axcenity_project::set_pinned(&folder, &id, pinned) {
                        self.output
                            .warning(format!("Could not change that version's pin: {err}"));
                    }
                    self.version_history.refresh();
                }
                Ask::Restore(restore) => self.version_history.confirm = Some(restore),
            }
        }
        self.confirm_restore(ctx, &versions);
    }

    /// Asks before a restore, then does it.
    fn confirm_restore(&mut self, ctx: &egui::Context, versions: &[Version]) {
        let Some(restore) = self.version_history.confirm.clone() else {
            return;
        };
        let id = match &restore {
            Restore::Project { id } | Restore::File { id, .. } => id,
        };
        let when = versions
            .iter()
            .find(|version| &version.id == id)
            .map_or_else(String::new, |version| {
                format!("\"{}\" ({})", version.label, version.when())
            });
        let title = match &restore {
            Restore::Project { .. } => "Restore the project?".to_owned(),
            Restore::File { path, .. } => format!("Restore {path}?"),
        };
        let message = match &restore {
            Restore::Project { .. } => format!(
                "The world, levels, scripts and styles go back to {when}. The current \
                 files are kept first as a pinned version named \"Before restore\", so \
                 you can undo this."
            ),
            Restore::File { .. } => format!(
                "This file goes back to {when}. The current files are kept first as a \
                 pinned version named \"Before restore\", so you can undo this."
            ),
        };
        let outcome = dialog::ask(
            ctx,
            &dialog::Question {
                id: "restore version",
                icon: icons::CLOCK_COUNTER_CLOCKWISE,
                tone: dialog::Tone::Question,
                title: &title,
                message: &message,
                items: Vec::new(),
            },
            &[
                dialog::Button::new(icons::X, "Cancel", dialog::Kind::Plain),
                dialog::Button::new(
                    icons::CLOCK_COUNTER_CLOCKWISE,
                    "Restore",
                    dialog::Kind::Main,
                ),
            ],
        );
        match outcome {
            Some(dialog::Outcome::Picked(1)) => {
                self.version_history.confirm = None;
                self.restore(&restore);
            }
            Some(_) => self.version_history.confirm = None,
            None => {}
        }
    }

    /// Restores a version or one of its files. Refuses while anything is
    /// unsaved, so nothing is lost that no version has.
    pub(super) fn restore(&mut self, restore: &Restore) {
        let Some(folder) = self.project.as_ref().map(|project| project.folder.clone()) else {
            return;
        };
        if self.unsaved() {
            self.output.warning(
                "Save or undo your changes before restoring, so nothing is lost that no \
                 version has.",
            );
            self.toasts.error("Save your changes before restoring.");
            return;
        }
        let result = match restore {
            Restore::Project { id } => axcenity_project::restore_version(&folder, id),
            Restore::File { id, path } => axcenity_project::restore_file(&folder, id, path),
        };
        match result {
            Ok(()) => {
                self.open(&folder);
                let what = match restore {
                    Restore::Project { .. } => "that version".to_owned(),
                    Restore::File { path, .. } => path.clone(),
                };
                self.output.info(format!(
                    "Restored {what}. The files it replaced were kept as a pinned version \
                     named \"Before restore\"."
                ));
                self.toasts.success(format!("Restored {what}"));
                self.version_history.open = true;
                self.version_history.selected = None;
                self.version_history.refresh();
            }
            Err(err) => self.output.error(format!("Could not restore: {err}")),
        }
    }

    /// Save Checkpoint: saves, then keeps the files as a pinned version with
    /// the window's message. Backs up too when that's turned on.
    pub(super) fn save_checkpoint(&mut self) {
        let Some(folder) = self.project.as_ref().map(|project| project.folder.clone()) else {
            return;
        };
        if self.unsaved() && !self.save() {
            return;
        }
        let message = std::mem::take(&mut self.version_history.message);
        match axcenity_project::record_checkpoint(&folder, &message) {
            Ok(id) => {
                let label = axcenity_project::list_versions(&folder)
                    .into_iter()
                    .find(|version| version.id == id)
                    .map_or(message, |version| version.label);
                self.output.info(format!(
                    "Saved the checkpoint \"{label}\". It's kept until unpinned."
                ));
                self.toasts.success(format!("Checkpoint \"{label}\""));
                if self.version_history.open {
                    self.version_history.selected = Some(id);
                }
                self.version_history.refresh();
                let backup = axcenity_project::backup_settings(&folder);
                if backup.on_checkpoint && backup.folder.is_some() {
                    self.back_up_project(false);
                }
            }
            Err(err) => self
                .output
                .warning(format!("Could not save the checkpoint: {err}")),
        }
    }

    /// Back Up Project: a `.zip` of the project in its backup folder. Asks
    /// for the folder when there's none yet, or when `choose`.
    pub(super) fn back_up_project(&mut self, choose: bool) {
        let Some(folder) = self.project.as_ref().map(|project| project.folder.clone()) else {
            return;
        };
        let mut settings = axcenity_project::backup_settings(&folder);
        let destination = match settings.folder.clone() {
            Some(destination) if !choose => destination,
            _ => {
                let Some(destination) = rfd::FileDialog::new()
                    .set_title("Back up the project to")
                    .pick_folder()
                else {
                    return;
                };
                settings.folder = Some(destination.clone());
                if let Err(err) = axcenity_project::save_backup_settings(&folder, &settings) {
                    self.output
                        .warning(format!("Could not keep the backup folder: {err}"));
                }
                destination
            }
        };
        match axcenity_project::back_up_project(&folder, &destination) {
            Ok(zip) => {
                self.output
                    .info(format!("Backed up the project to {}.", zip.display()));
                self.toasts.success("Backed up the project");
            }
            Err(err) => {
                self.output.error(format!(
                    "Could not back up to {}: {err}",
                    destination.display()
                ));
                self.toasts.error("Could not back up. Output says why.");
            }
        }
    }
}

/// The checkpoint message box and Save Checkpoint.
fn checkpoint_row(
    ui: &mut egui::Ui,
    history: &mut VersionHistory,
    palette: &theme::Palette,
    asks: &mut Vec<Ask>,
) {
    crate::responsive::toolbar(ui, |ui| {
        let field = ui.add(
            egui::TextEdit::singleline(&mut history.message)
                .hint_text("What's this checkpoint? (optional)")
                .desired_width(260.0),
        );
        if std::mem::take(&mut history.focus_message) {
            field.request_focus();
        }
        let enter = field.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
        let button = ui
            .add(widgets::button(
                widgets::Role::Primary,
                Some(icons::PUSH_PIN),
                "Save Checkpoint",
                palette,
            ))
            .on_hover_text(
                "Save, and keep this version however many saves follow. Ordinary saves \
                 keep only the newest 40.",
            );
        if button.clicked() || enter {
            asks.push(Ask::Checkpoint);
        }
    });
}

/// Where backups go, Back Up Project, and backing up on every checkpoint.
fn backup_row(
    ui: &mut egui::Ui,
    backup: &BackupSettings,
    palette: &theme::Palette,
    asks: &mut Vec<Ask>,
) {
    crate::responsive::toolbar(ui, |ui| {
        let back_up = ui
            .add(widgets::button(
                widgets::Role::Secondary,
                Some(icons::ARCHIVE),
                "Back Up Project",
                palette,
            ))
            .on_hover_text(
                "Write a .zip of the project (without the .axcenity cache) to the backup \
                 folder. Unzipped on any computer, it opens as the project.",
            );
        if back_up.clicked() {
            asks.push(Ask::BackUp);
        }
        match &backup.folder {
            Some(folder) => {
                ui.label(
                    egui::RichText::new(folder.display().to_string()).color(palette.text_weak),
                );
            }
            None => {
                ui.label(egui::RichText::new("No backup folder yet").color(palette.text_weak));
            }
        }
        if ui
            .button(theme::with_icon(icons::FOLDER_OPEN, "Choose Folder…"))
            .on_hover_text("Pick where backups go, such as a cloud-synced folder.")
            .clicked()
        {
            asks.push(Ask::ChooseBackupFolder);
        }
        let mut on = backup.on_checkpoint;
        if ui
            .add_enabled(
                backup.folder.is_some(),
                egui::Checkbox::new(&mut on, "Back up on every checkpoint"),
            )
            .on_disabled_hover_text("Choose a backup folder first.")
            .changed()
        {
            asks.push(Ask::BackUpOnCheckpoint(on));
        }
    });
}

/// The versions, newest first: a pin, the message (or "Save") and the date.
fn timeline(
    ui: &mut egui::Ui,
    versions: &[Version],
    selected: Option<&str>,
    palette: &theme::Palette,
    asks: &mut Vec<Ask>,
) {
    egui::ScrollArea::vertical()
        .id_salt("version timeline")
        .auto_shrink([false, true])
        .max_height(ui.available_height().max(240.0))
        .show(ui, |ui| {
            for version in versions {
                ui.horizontal(|ui| {
                    let (icon, tip) = if version.pinned {
                        (
                            filled_icons::PUSH_PIN,
                            "Pinned: kept however many saves follow. Click to unpin.",
                        )
                    } else {
                        (
                            icons::PUSH_PIN,
                            "Pin, so it's never dropped as an old save.",
                        )
                    };
                    if widgets::icon_button(ui, icon, tip, version.pinned).clicked() {
                        asks.push(Ask::Pin(version.id.clone(), !version.pinned));
                    }
                    let mut text = egui::text::LayoutJob::default();
                    text.append(
                        &version.label,
                        0.0,
                        egui::TextFormat {
                            font_id: egui::FontId::new(
                                theme::type_scale::BODY,
                                theme::medium_family(),
                            ),
                            color: palette.text,
                            ..Default::default()
                        },
                    );
                    text.append(
                        &format!("\n{}", version.when()),
                        0.0,
                        egui::TextFormat {
                            font_id: egui::FontId::proportional(theme::type_scale::SMALL),
                            color: palette.text_weak,
                            ..Default::default()
                        },
                    );
                    let row = ui.add(egui::Button::selectable(
                        selected == Some(version.id.as_str()),
                        text,
                    ));
                    if row.clicked() {
                        asks.push(Ask::Select(version.id.clone()));
                    }
                });
            }
        });
}

/// What changed in `version` since the one before it, with its restore buttons.
fn change_view(
    ui: &mut egui::Ui,
    version: &Version,
    changes: &Result<Changes, String>,
    unsaved: bool,
    palette: &theme::Palette,
    asks: &mut Vec<Ask>,
) {
    let refuse = "Save or undo your changes first, so nothing is lost that no version has.";
    ui.label(
        egui::RichText::new(&version.label)
            .family(theme::medium_family())
            .color(palette.text_strong),
    );
    ui.label(
        egui::RichText::new(version.when())
            .size(theme::type_scale::SMALL)
            .color(palette.text_weak),
    );
    crate::responsive::toolbar(ui, |ui| {
        if ui
            .add_enabled(
                !unsaved,
                widgets::button(
                    widgets::Role::Secondary,
                    Some(icons::CLOCK_COUNTER_CLOCKWISE),
                    "Restore project to here",
                    palette,
                ),
            )
            .on_disabled_hover_text(refuse)
            .clicked()
        {
            asks.push(Ask::Restore(Restore::Project {
                id: version.id.clone(),
            }));
        }
    });
    if unsaved {
        ui.label(egui::RichText::new(refuse).color(palette.warning));
    }
    ui.add_space(4.0);
    let changes = match changes {
        Ok(changes) => changes,
        Err(err) => {
            ui.label(
                egui::RichText::new(format!("Could not read this version: {err}"))
                    .color(palette.error),
            );
            return;
        }
    };
    if changes.is_empty() {
        ui.weak("Nothing changed since the version before.");
        return;
    }
    egui::ScrollArea::vertical()
        .id_salt("version changes")
        .auto_shrink([false, true])
        .max_height(ui.available_height().max(240.0))
        .show(ui, |ui| {
            for file in &changes.files {
                changed_file(ui, &version.id, file, unsaved, palette, asks);
                ui.add_space(6.0);
            }
            if !changes.assets.is_empty() {
                ui.label(egui::RichText::new("Assets").family(theme::medium_family()));
                for asset in changes.assets.iter().take(MAX_LISTED) {
                    let verb = match asset.change {
                        Change::Added => "Added",
                        Change::Removed => "Removed",
                        Change::Changed => "Changed",
                    };
                    ui.label(format!("{verb} {}", asset.path));
                }
                if changes.assets.len() > MAX_LISTED {
                    ui.weak(format!("and {} more", changes.assets.len() - MAX_LISTED));
                }
                ui.weak("Assets aren't kept in versions, so a restore leaves them as they are.");
            }
        });
}

/// One changed file: its title and Restore this file, then its Instance
/// changes in words or its line diff.
fn changed_file(
    ui: &mut egui::Ui,
    id: &str,
    file: &ChangedFile,
    unsaved: bool,
    palette: &theme::Palette,
    asks: &mut Vec<Ask>,
) {
    let (verb, icon) = match file.change {
        Change::Added => ("added", icons::PLUS_CIRCLE),
        Change::Removed => ("removed", icons::MINUS_CIRCLE),
        Change::Changed => ("changed", icons::PENCIL_SIMPLE),
    };
    crate::responsive::toolbar(ui, |ui| {
        ui.label(
            egui::RichText::new(theme::with_icon(
                icon,
                &format!("{} ({verb})", file.title()),
            ))
            .family(theme::medium_family()),
        );
        let hover = if file.change == Change::Removed {
            "Remove this file again, as in this version."
        } else {
            "Put this file back as it was in this version."
        };
        if ui
            .add_enabled(!unsaved, egui::Button::new("Restore this file").small())
            .on_hover_text(hover)
            .on_disabled_hover_text("Save or undo your changes first.")
            .clicked()
        {
            asks.push(Ask::Restore(Restore::File {
                id: id.to_owned(),
                path: file.path.clone(),
            }));
        }
    });
    for instance in file.instances.iter().take(MAX_LISTED) {
        ui.label(format!("• {}", instance.describe()));
    }
    if file.instances.len() > MAX_LISTED {
        ui.weak(format!("and {} more", file.instances.len() - MAX_LISTED));
    }
    if file.lines.is_empty() {
        return;
    }
    egui::Frame::new()
        .fill(palette.inset)
        .stroke(egui::Stroke::new(1.0_f32, palette.border))
        .corner_radius(6)
        .inner_margin(egui::Margin::symmetric(8, 4))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let shown = axcenity_project::around_changes(&file.lines, DIFF_CONTEXT);
            for line in shown.iter().take(MAX_DIFF_LINES) {
                let (text, color) = match line {
                    None => ("…".to_owned(), palette.text_muted),
                    Some(Line::Same(text)) => (format!("  {text}"), palette.text_weak),
                    Some(Line::Added(text)) => (format!("+ {text}"), palette.success),
                    Some(Line::Removed(text)) => (format!("- {text}"), palette.error),
                };
                ui.label(egui::RichText::new(text).monospace().color(color));
            }
            if shown.len() > MAX_DIFF_LINES {
                ui.weak(format!("and {} more lines", shown.len() - MAX_DIFF_LINES));
            }
        });
}
