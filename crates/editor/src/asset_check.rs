//! The Asset Check panel: one scan of the open project that lists missing
//! files, invalid references, import failures, and broken dependencies.
//! Running it does not write the project. The player has no such panel.

use axcenity_project::{Finding, FindingKind, Report};
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
}

/// The last scan, if one has been run for this open project.
#[derive(Default)]
pub struct AssetCheck {
    report: Option<Report>,
}

impl AssetCheck {
    pub fn clear(&mut self) {
        self.report = None;
    }

    pub fn set(&mut self, report: Report) {
        self.report = Some(report);
    }

    /// The panel. `can_run` is false with no project open.
    pub fn show(&self, ui: &mut egui::Ui, can_run: bool) -> CheckAction {
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
        match &self.report {
            None => {
                ui.label(
                    RichText::new(
                        "Check Assets scans the open project and lists missing files, \
                         invalid references, import failures, and broken dependencies \
                         in one place.",
                    )
                    .color(palette.text_secondary),
                );
            }
            Some(report) if report.is_ok() => {
                ui.label(
                    RichText::new(theme::with_icon(
                        icons::CHECK_CIRCLE,
                        "No missing files, invalid references, import failures, or \
                         broken dependencies.",
                    ))
                    .color(palette.success),
                );
            }
            Some(report) => {
                let count = report.findings.len();
                let label = if count == 1 {
                    "1 problem".to_owned()
                } else {
                    format!("{count} problems")
                };
                ui.label(
                    RichText::new(theme::with_icon(icons::WARNING, &label)).color(palette.warning),
                );
                ui.add_space(6.0);
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for finding in &report.findings {
                            if let CheckAction::Select(owner) = finding_row(ui, finding)
                                && matches!(action, CheckAction::None)
                            {
                                action = CheckAction::Select(owner);
                            }
                        }
                    });
            }
        }
        action
    }
}

fn finding_row(ui: &mut egui::Ui, finding: &Finding) -> CheckAction {
    let palette = theme::palette(ui.ctx());
    let (kind_icon, kind_color) = match finding.kind {
        FindingKind::MissingFile => (icons::FILE, palette.warning),
        FindingKind::InvalidReference => (icons::WARNING, palette.warning),
        FindingKind::ImportFailure => (icons::WARNING_CIRCLE, palette.error),
        FindingKind::BrokenDependency => (icons::LINK, palette.warning),
    };
    let mut action = CheckAction::None;
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
    ui.add_space(8.0);
    action
}
