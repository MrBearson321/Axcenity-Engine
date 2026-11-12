//! The Collaborate panel's History (C9; design §5.8): the host's
//! checkpoints of the session, newest first, with who asked for each and
//! who changed things since the one before. Editors save a checkpoint with
//! a label; the Owner restores one for everyone, after saying so once.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axcenity_collab::Role as MemberRole;
use axcenity_collab::history::{Checkpoint, CheckpointKind};

use super::team::Team;
use super::team_panel::TeamPanel;
use super::*;

/// What the History section asked for this frame.
pub(super) enum HistoryAction {
    /// Save a checkpoint with the label being typed.
    Save,
    /// Ask before restoring (`Some`), or stop asking (`None`).
    Confirm(Option<u64>),
    /// Restore this checkpoint for everyone.
    Restore(u64),
}

/// What a checkpoint is, in a few words, for its second line.
pub(crate) fn describe(checkpoint: &Checkpoint, now: SystemTime) -> String {
    let when = crate::start::ago(UNIX_EPOCH + Duration::from_secs(checkpoint.at), now);
    let mut parts = vec![when];
    match (&checkpoint.by, checkpoint.kind) {
        (Some(by), CheckpointKind::BeforeRestore) => parts.push(format!("kept when {by} restored")),
        (Some(by), _) => parts.push(format!("by {by}")),
        (None, _) => parts.push("automatic".to_owned()),
    }
    if !checkpoint.edited_by.is_empty() {
        parts.push(format!("changes by {}", checkpoint.edited_by.join(", ")));
    }
    parts.join(" · ")
}

/// The History section of the Collaborate panel.
pub(super) fn history_section(
    ui: &mut egui::Ui,
    panel: &mut TeamPanel,
    team: &Team,
) -> Option<HistoryAction> {
    let palette = theme::palette(ui.ctx());
    let mut action = None;
    let role = team.client_role();
    let can_save = role.is_some_and(|role| role.can(axcenity_collab::Capability::SaveCheckpoint));
    let owner = role == Some(MemberRole::Owner);
    let checkpoints = team.client().checkpoints();
    let now = SystemTime::now();
    widgets::section(icons::CLOCK_COUNTER_CLOCKWISE, "History").show(ui, |ui| {
        if can_save {
            ui.horizontal(|ui| {
                let save = ui
                    .add(widgets::button(
                        widgets::Role::Secondary,
                        Some(icons::BOOKMARK_SIMPLE),
                        "Save",
                        &palette,
                    ))
                    .on_hover_text("Keep the session as it is now, on the host, for everyone");
                let response = ui.add(
                    egui::TextEdit::singleline(&mut panel.checkpoint_label)
                        .id_salt("collaborate checkpoint label")
                        .hint_text("Checkpoint name")
                        .desired_width(ui.available_width()),
                );
                let entered =
                    response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
                if save.clicked() || entered {
                    action = Some(HistoryAction::Save);
                }
            });
        }
        if let Some(problem) = &panel.history_problem {
            ui.add(egui::Label::new(egui::RichText::new(problem).color(palette.warning)).wrap());
        }
        if checkpoints.is_empty() {
            ui.add(
                egui::Label::new(
                    egui::RichText::new(
                        "No checkpoints yet. The host keeps one when the session starts and \
                         every hour with changes.",
                    )
                    .color(palette.text_weak),
                )
                .wrap(),
            );
        }
        egui::ScrollArea::vertical()
            .id_salt("collaborate history")
            .max_height(220.0)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                for checkpoint in checkpoints {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.set_width((ui.available_width() - 32.0).max(80.0));
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(&checkpoint.label)
                                        .family(theme::medium_family()),
                                )
                                .truncate(),
                            );
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(describe(checkpoint, now))
                                        .color(palette.text_weak)
                                        .small(),
                                )
                                .wrap(),
                            );
                        });
                        if owner
                            && widgets::icon_button(
                                ui,
                                icons::ARROW_COUNTER_CLOCKWISE,
                                "Restore this for everyone…",
                                panel.confirm_restore == Some(checkpoint.id),
                            )
                            .clicked()
                        {
                            action = Some(HistoryAction::Confirm(
                                (panel.confirm_restore != Some(checkpoint.id))
                                    .then_some(checkpoint.id),
                            ));
                        }
                    });
                    if panel.confirm_restore == Some(checkpoint.id) {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(
                                    "Everyone's session goes back to this, as one step. The \
                                     session as it is now is kept as a checkpoint first.",
                                )
                                .color(palette.warning),
                            )
                            .wrap(),
                        );
                        ui.horizontal(|ui| {
                            if ui
                                .add(widgets::button(
                                    widgets::Role::Danger,
                                    None,
                                    "Restore for everyone",
                                    &palette,
                                ))
                                .clicked()
                            {
                                action = Some(HistoryAction::Restore(checkpoint.id));
                            }
                            if ui
                                .add(widgets::button(
                                    widgets::Role::Ghost,
                                    None,
                                    "Cancel",
                                    &palette,
                                ))
                                .clicked()
                            {
                                action = Some(HistoryAction::Confirm(None));
                            }
                        });
                    }
                    ui.add_space(theme::space::XS);
                }
            });
    });
    action
}

impl EditorApp {
    pub(super) fn team_history_action(&mut self, action: HistoryAction) {
        let Some(team) = self.team.as_mut() else {
            return;
        };
        match action {
            HistoryAction::Save => {
                let label = std::mem::take(&mut self.team_panel.checkpoint_label);
                match team.save_checkpoint(&label) {
                    Ok(()) => self.team_panel.history_problem = None,
                    Err(problem) => {
                        self.team_panel.checkpoint_label = label;
                        self.team_panel.history_problem = Some(problem);
                    }
                }
            }
            HistoryAction::Confirm(id) => self.team_panel.confirm_restore = id,
            HistoryAction::Restore(id) => {
                self.team_panel.confirm_restore = None;
                match team.restore_checkpoint(id) {
                    Ok(()) => self.team_panel.history_problem = None,
                    Err(problem) => self.team_panel.history_problem = Some(problem),
                }
            }
        }
    }
}
