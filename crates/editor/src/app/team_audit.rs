//! The Collaborate panel's Audit log (C10; design §6.1), for the Owner:
//! who joined or was turned away, invites, roles, removals, restores, and
//! members refused for their role or for sending too much. The host keeps
//! it; this asks for it when the section first shows and on Refresh.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axcenity_collab::Role as MemberRole;
use axcenity_collab::audit::{AuditEntry, AuditKind};

use super::team::Team;
use super::*;

/// Entries the section lists, newest first.
const SHOWN: usize = 200;

/// A word for what kind of entry it is.
fn kind_word(kind: AuditKind) -> &'static str {
    match kind {
        AuditKind::Joined => "Joined",
        AuditKind::Refused => "Turned away",
        AuditKind::Invite => "Invite",
        AuditKind::Role => "Role",
        AuditKind::Removed => "Removed",
        AuditKind::Restored => "Restored",
        AuditKind::Denied => "Not allowed",
        AuditKind::Limited => "Too much",
    }
}

/// The second line of an entry: when, and what kind.
pub(crate) fn describe(entry: &AuditEntry, now: SystemTime) -> String {
    let when = crate::start::ago(UNIX_EPOCH + Duration::from_secs(entry.at), now);
    format!("{when} · {}", kind_word(entry.kind))
}

/// The Audit log section; true when it should ask the host for the log.
pub(super) fn audit_section(ui: &mut egui::Ui, team: &Team) -> bool {
    let Some(client) = team.client() else {
        return false;
    };
    if team.client_role() != Some(MemberRole::Owner) {
        return false;
    }
    let palette = theme::palette(ui.ctx());
    let mut ask = !team.audit_asked();
    let now = SystemTime::now();
    widgets::section(icons::SHIELD_CHECK, "Audit log")
        .default_open(false)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new("Kept by the host; only you can see it.")
                            .color(palette.text_weak)
                            .small(),
                    )
                    .wrap(),
                );
                if widgets::icon_button(ui, icons::ARROW_CLOCKWISE, "Refresh", false).clicked() {
                    ask = true;
                }
            });
            let log = client.audit_log();
            if log.is_empty() {
                ui.label(egui::RichText::new("Nothing yet.").color(palette.text_weak));
            }
            egui::ScrollArea::vertical()
                .id_salt("collaborate audit")
                .max_height(200.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    for entry in log.iter().rev().take(SHOWN) {
                        let color = match entry.kind {
                            AuditKind::Refused | AuditKind::Denied | AuditKind::Limited => {
                                palette.warning
                            }
                            _ => palette.text,
                        };
                        ui.add(
                            egui::Label::new(egui::RichText::new(&entry.text).color(color)).wrap(),
                        );
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(describe(entry, now))
                                    .color(palette.text_weak)
                                    .small(),
                            )
                            .wrap(),
                        );
                        ui.add_space(theme::space::XS);
                    }
                });
        });
    ask
}

impl EditorApp {
    /// Asks the host for its audit log.
    pub(super) fn ask_for_team_audit(&mut self) {
        if let Some(team) = self.team.as_mut() {
            team.ask_for_audit_log();
        }
    }
}
