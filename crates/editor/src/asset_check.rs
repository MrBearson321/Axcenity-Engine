//! The Asset Check panel: one scan of the open project that lists missing
//! files, invalid references, import failures, and broken dependencies,
//! with the fixes that suit each (Use, Copy in, Locate…, Replace…, Clear,
//! Delete file, Ignore). Running it does not write the project; a fix is
//! applied by the app as one undo step. The player has no such panel.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use axcenity_project::{AssetKind, Finding, FindingKind, Ignored, Report};
use eframe::egui::{self, RichText, Sense};

use crate::responsive;
use crate::theme::{self, icons, type_scale};
use crate::widgets;

/// What the panel asked the app to do.
#[derive(Debug, Default)]
pub enum CheckAction {
    #[default]
    None,
    /// Run the scan.
    Run,
    /// Select the Instance named by this finding's owner, if it's in the world.
    Select(String),
    /// Fix these findings (the one clicked, and the ones like it when
    /// "Apply to all like this" is ticked) this way.
    Fix(Vec<Finding>, FixChoice),
    /// Hide these findings from now on.
    Ignore(Vec<Finding>),
    /// Show this ignored finding again.
    Unignore(Ignored),
}

/// The fix a row's button picked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FixChoice {
    /// The suggested file, or one picked from Replace….
    Use(String),
    /// A file on this computer: the reference's own full path (Copy in).
    CopyIn(PathBuf),
    /// Ask for a file with a file dialog, then copy it in if it's outside.
    Locate,
    Clear,
    Delete,
    /// Point everything at the kept file of a duplicate and delete the copy.
    Merge,
}

/// The last scan, if one has been run for this open project.
#[derive(Default)]
pub struct AssetCheck {
    report: Option<Report>,
    /// The suggested file for each finding, in the report's order.
    suggestions: Vec<Option<String>>,
    /// The project's files of each kind, for Replace….
    choices: HashMap<AssetKind, Vec<String>>,
    /// Findings the developer chose to hide, as saved in the project.
    ignored: Vec<Ignored>,
    show_ignored: bool,
    /// Findings whose "Apply to all like this" box is ticked.
    apply_all: Vec<Finding>,
}

impl AssetCheck {
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// A new scan of the project whose files are `files`, with the
    /// findings in `ignored` hidden. A file `moved` outside the editor
    /// (found by its asset ID) is suggested before one found by name.
    pub fn set(
        &mut self,
        report: Report,
        files: &[String],
        moved: &BTreeMap<String, String>,
        ignored: Vec<Ignored>,
    ) {
        self.suggestions = report
            .findings
            .iter()
            .map(|finding| {
                axcenity_project::moved_suggestion(finding, moved)
                    .or_else(|| axcenity_project::suggestion(finding, files))
            })
            .collect();
        self.choices = [
            AssetKind::Model,
            AssetKind::Texture,
            AssetKind::Audio,
            AssetKind::Clip,
            AssetKind::StateMachine,
            AssetKind::Material,
            AssetKind::Prefab,
            AssetKind::ServerScript,
            AssetKind::ClientScript,
            AssetKind::ModuleScript,
        ]
        .into_iter()
        .map(|kind| (kind, axcenity_project::files_of_kind(files, kind)))
        .collect();
        self.ignored = ignored;
        self.apply_all
            .retain(|ticked| report.findings.iter().any(|finding| finding == ticked));
        self.report = Some(report);
    }

    /// The findings that aren't ignored.
    pub fn shown(&self) -> Vec<&Finding> {
        self.report
            .iter()
            .flat_map(|report| &report.findings)
            .filter(|finding| !self.is_ignored(finding))
            .collect()
    }

    fn is_ignored(&self, finding: &Finding) -> bool {
        self.ignored.contains(&finding.ignore_key())
    }

    /// The panel. `can_run` is false with no project open; `can_fix` is
    /// false during Play.
    pub fn show(&mut self, ui: &mut egui::Ui, can_run: bool, can_fix: bool) -> CheckAction {
        let palette = theme::palette(ui.ctx());
        let mut action = CheckAction::None;
        responsive::toolbar(ui, |ui| {
            let button = widgets::button(
                widgets::Role::Secondary,
                Some(icons::MAGNIFYING_GLASS),
                "Check Assets",
                &palette,
            );
            if ui
                .add_enabled(can_run, button)
                .on_hover_text(
                    "Scan the open project for missing files, invalid references, \
                     import failures, and broken dependencies. Files are not changed.",
                )
                .on_disabled_hover_text("Open a project first.")
                .clicked()
            {
                action = CheckAction::Run;
            }
        });
        ui.add_space(6.0);
        let Some(report) = &self.report else {
            ui.label(
                RichText::new(
                    "Check Assets scans the open project and lists missing files, \
                     invalid references, import failures, and broken dependencies \
                     in one place, with ways to fix each.",
                )
                .color(palette.text_secondary),
            );
            return action;
        };
        // Fields, not `self`, so `show_ignored` and `apply_all` can change below.
        let ignored = &self.ignored;
        let is_ignored = |finding: &Finding| ignored.contains(&finding.ignore_key());
        let (hidden, shown): (Vec<&Finding>, Vec<&Finding>) = report
            .findings
            .iter()
            .partition(|finding| is_ignored(finding));
        if shown.is_empty() {
            ui.label(
                RichText::new(theme::with_icon(
                    icons::CHECK_CIRCLE,
                    "No missing files, invalid references, import failures, or \
                     broken dependencies.",
                ))
                .color(palette.success),
            );
        } else {
            let count = shown.len();
            let label = if count == 1 {
                "1 problem".to_owned()
            } else {
                format!("{count} problems")
            };
            ui.label(
                RichText::new(theme::with_icon(icons::WARNING, &label)).color(palette.warning),
            );
        }
        if !hidden.is_empty() {
            let label = if self.show_ignored {
                format!("Hide ignored ({})", hidden.len())
            } else {
                format!("Show ignored ({})", hidden.len())
            };
            let icon = if self.show_ignored {
                icons::EYE_SLASH
            } else {
                icons::EYE
            };
            if ui
                .add(widgets::button(
                    widgets::Role::Ghost,
                    Some(icon),
                    &label,
                    &palette,
                ))
                .clicked()
            {
                self.show_ignored = !self.show_ignored;
            }
        }
        if !can_fix && !shown.is_empty() {
            ui.label(
                RichText::new("Stop the game to fix these.")
                    .size(type_scale::SMALL)
                    .color(palette.text_secondary),
            );
        }
        ui.add_space(6.0);
        let mut ticks = Vec::new();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (index, finding) in report.findings.iter().enumerate() {
                    let ignored = is_ignored(finding);
                    if ignored && !self.show_ignored {
                        continue;
                    }
                    let row = Row {
                        finding,
                        suggestion: self.suggestions.get(index).cloned().flatten(),
                        choices: finding
                            .file_kind()
                            .and_then(|kind| self.choices.get(&kind))
                            .map_or(&[][..], Vec::as_slice),
                        like: shown
                            .iter()
                            .filter(|other| !std::ptr::eq(**other, finding))
                            .filter(|other| finding.is_like(other))
                            .map(|other| (*other).clone())
                            .collect(),
                        ticked: self.apply_all.contains(finding),
                        ignored,
                        can_fix,
                    };
                    let (picked, tick) = row.show(ui);
                    if let Some(tick) = tick {
                        ticks.push((finding.clone(), tick));
                    }
                    if !matches!(picked, CheckAction::None) && matches!(action, CheckAction::None) {
                        action = picked;
                    }
                }
            });
        for (finding, tick) in ticks {
            self.apply_all.retain(|ticked| ticked != &finding);
            if tick {
                self.apply_all.push(finding);
            }
        }
        action
    }
}

/// One finding's row, with what it needs to offer its fixes.
struct Row<'a> {
    finding: &'a Finding,
    suggestion: Option<String>,
    choices: &'a [String],
    /// The other shown findings a fix here can also apply to.
    like: Vec<Finding>,
    ticked: bool,
    ignored: bool,
    can_fix: bool,
}

impl Row<'_> {
    /// What was clicked, and the "Apply to all like this" box if it changed.
    fn show(self, ui: &mut egui::Ui) -> (CheckAction, Option<bool>) {
        let palette = theme::palette(ui.ctx());
        let finding = self.finding;
        let (kind_icon, kind_color) = match finding.kind {
            FindingKind::MissingFile => (icons::FILE, palette.warning),
            FindingKind::InvalidReference => (icons::WARNING, palette.warning),
            FindingKind::ImportFailure => (icons::WARNING_CIRCLE, palette.error),
            FindingKind::BrokenDependency => (icons::LINK, palette.warning),
            FindingKind::UnusedFile => (icons::FILE_CODE, palette.text_weak),
            FindingKind::DuplicateFile => (icons::COPY, palette.text_weak),
        };
        let mut action = CheckAction::None;
        let mut tick = None;
        let kind_color = if self.ignored {
            palette.text_secondary
        } else {
            kind_color
        };
        ui.label(
            RichText::new(theme::with_icon(kind_icon, finding.kind.label()))
                .size(type_scale::CAPTION)
                .color(kind_color),
        );
        let owner = ui.add(
            egui::Label::new(
                RichText::new(&finding.owner)
                    .family(theme::medium_family())
                    .color(palette.text_strong),
            )
            .wrap()
            .sense(Sense::click()),
        );
        if owner.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if owner.clicked() {
            action = CheckAction::Select(finding.owner.clone());
        }
        owner.on_hover_text("Select this in the Explorer, if it's an Instance.");
        ui.label(
            RichText::new(&finding.message)
                .size(type_scale::SMALL)
                .color(palette.text_secondary),
        );
        if self.ignored {
            if ui
                .add(widgets::button(
                    widgets::Role::Ghost,
                    Some(icons::EYE),
                    "Unignore",
                    &palette,
                ))
                .clicked()
            {
                action = CheckAction::Unignore(finding.ignore_key());
            }
            ui.add_space(8.0);
            return (action, tick);
        }
        let targets = || {
            let mut all = vec![finding.clone()];
            if self.ticked {
                all.extend(self.like.iter().cloned());
            }
            all
        };
        let mut choice = None;
        let can_fix = self.can_fix;
        // A fix button, off during Play. True when clicked.
        let fix = |ui: &mut egui::Ui, role, icon, label: &str, tip: &str| {
            let button = widgets::button(role, Some(icon), label, &palette);
            ui.add_enabled(can_fix, button)
                .on_hover_text(tip)
                .on_disabled_hover_text("Stop the game to fix this.")
                .clicked()
        };
        ui.add_space(2.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
            if let Some(path) = &self.suggestion {
                let name = path.rsplit('/').next().unwrap_or(path);
                let tip = format!("Point it at `{path}`, which has the same name.");
                let label = format!("Use {name}");
                if fix(ui, widgets::Role::Primary, icons::MAGIC_WAND, &label, &tip) {
                    choice = Some(FixChoice::Use(path.clone()));
                }
            }
            if let Some(file) = finding.outside_file() {
                let tip = format!(
                    "Copy `{}` into the project and point it at the copy.",
                    file.display()
                );
                if fix(
                    ui,
                    widgets::Role::Secondary,
                    icons::DOWNLOAD_SIMPLE,
                    "Copy in",
                    &tip,
                ) {
                    choice = Some(FixChoice::CopyIn(file));
                }
            }
            if finding.can_point_elsewhere() {
                let tip = "Pick the file. One outside the project is copied in first.";
                if fix(
                    ui,
                    widgets::Role::Secondary,
                    icons::FOLDER_OPEN,
                    "Locate…",
                    tip,
                ) {
                    choice = Some(FixChoice::Locate);
                }
                let label = theme::with_icon(icons::ARROWS_LEFT_RIGHT, "Replace…");
                if self.choices.is_empty() {
                    // Nothing in the project to replace it with.
                } else if can_fix {
                    ui.menu_button(label, |ui| {
                        egui::ScrollArea::vertical()
                            .max_height(240.0)
                            .show(ui, |ui| {
                                for path in self.choices {
                                    if ui.button(path).clicked() {
                                        choice = Some(FixChoice::Use(path.clone()));
                                        ui.close();
                                    }
                                }
                            });
                    })
                    .response
                    .on_hover_text("Point it at another file in the project.");
                } else {
                    ui.add_enabled(false, egui::Button::new(label))
                        .on_disabled_hover_text("Stop the game to fix this.");
                }
            }
            if finding.can_clear() {
                let tip = "Empty the reference, so it names no file.";
                if fix(ui, widgets::Role::Ghost, icons::ERASER, "Clear", tip) {
                    choice = Some(FixChoice::Clear);
                }
            }
            if finding.kind == FindingKind::DuplicateFile {
                let keep = finding.path.rsplit('/').next().unwrap_or(&finding.path);
                let label = format!("Merge into {keep}");
                let tip = format!(
                    "Point everything that uses this file at `{}` and delete this one. Undo puts it back.",
                    finding.path
                );
                if fix(ui, widgets::Role::Primary, icons::GIT_MERGE, &label, &tip) {
                    choice = Some(FixChoice::Merge);
                }
            }
            if finding.can_delete() {
                let tip = "Delete the file. Undo puts it back.";
                if fix(ui, widgets::Role::Danger, icons::TRASH, "Delete file", tip) {
                    choice = Some(FixChoice::Delete);
                }
            }
            let button = widgets::button(
                widgets::Role::Ghost,
                Some(icons::EYE_SLASH),
                "Ignore",
                &palette,
            );
            if ui
                .add(button)
                .on_hover_text("Hide this problem. Show ignored lists it again.")
                .clicked()
            {
                action = CheckAction::Ignore(targets());
            }
        });
        if !self.like.is_empty() {
            let mut ticked = self.ticked;
            let more = self.like.len();
            let label = if more == 1 {
                "Apply to all like this (1 more)".to_owned()
            } else {
                format!("Apply to all like this ({more} more)")
            };
            if ui
                .checkbox(&mut ticked, RichText::new(label).size(type_scale::SMALL))
                .changed()
            {
                tick = Some(ticked);
            }
        }
        if let Some(choice) = choice {
            action = CheckAction::Fix(targets(), choice);
        }
        ui.add_space(8.0);
        (action, tick)
    }
}
