//! The Team Create panel and the title bar chip (TC4; design §11): share
//! the open project or join a session, see who's in it and how the
//! connection is, invite people and set their roles, and leave.

use axcenity_collab::MemberId;
use axcenity_collab::Role as MemberRole;
use axcenity_collab::access::Invite;

use super::team::TeamStatus;
use super::*;

/// What the panel keeps between frames.
#[derive(Default)]
pub(crate) struct TeamPanel {
    /// The invite link being typed or pasted.
    pub(crate) link: String,
    /// The role new invites give.
    pub(crate) invite_role: InviteRole,
    /// Why the last Share or Join didn't start.
    pub(crate) problem: Option<String>,
}

/// The roles an invite can give.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum InviteRole {
    #[default]
    Editor,
    Viewer,
}

impl InviteRole {
    fn role(self) -> MemberRole {
        match self {
            InviteRole::Editor => MemberRole::Editor,
            InviteRole::Viewer => MemberRole::Viewer,
        }
    }
}

/// What the panel asked for this frame.
enum Action {
    Share,
    Join,
    Invite,
    Revoke,
    SetRole(MemberId, MemberRole),
    Remove(MemberId),
    Leave,
}

/// The panel buttons tests press.
#[cfg(test)]
pub(crate) enum TestAction {
    Join,
}

/// The colour a member's profile names.
pub(crate) fn member_color(color: [u8; 3]) -> egui::Color32 {
    egui::Color32::from_rgb(color[0], color[1], color[2])
}

/// A member's initial in a filled circle of their colour.
fn avatar(ui: &mut egui::Ui, name: &str, color: [u8; 3], size: f32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::click());
    let fill = member_color(color);
    ui.painter().circle_filled(rect.center(), size * 0.5, fill);
    let initial: String = name.chars().next().unwrap_or('?').to_uppercase().collect();
    // Dark text on light colours, white on dark ones.
    let light =
        0.299 * f32::from(color[0]) + 0.587 * f32::from(color[1]) + 0.114 * f32::from(color[2])
            > 150.0;
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        initial,
        egui::FontId::new(size * 0.55, theme::medium_family()),
        if light {
            egui::Color32::BLACK
        } else {
            egui::Color32::WHITE
        },
    );
    response
}

/// The colour of the sync dot for `status`.
fn status_color(status: &TeamStatus, palette: &theme::Palette) -> egui::Color32 {
    match status {
        TeamStatus::Connected { waiting: 0 } => palette.success,
        TeamStatus::Connected { .. } | TeamStatus::Joining => palette.warning,
        TeamStatus::Reconnecting { .. } | TeamStatus::Refused(_) => palette.error,
    }
}

/// `status` in a few words.
pub(crate) fn status_text(status: &TeamStatus) -> String {
    match status {
        TeamStatus::Joining => "Joining…".to_owned(),
        TeamStatus::Connected { waiting: 0 } => "Connected".to_owned(),
        TeamStatus::Connected { waiting } => format!("Syncing {waiting}"),
        TeamStatus::Reconnecting { waiting: 0, .. } => "Reconnecting…".to_owned(),
        TeamStatus::Reconnecting { waiting, .. } => {
            format!("Offline, {waiting} change(s) waiting")
        }
        TeamStatus::Refused(_) => "Not in the session".to_owned(),
    }
}

impl EditorApp {
    /// The title bar's chip while in a session: up to four collaborators
    /// who are here and a sync dot. Opens the Team Create panel.
    pub(super) fn team_chip(&mut self, ui: &mut egui::Ui) {
        let Some(team) = &self.team else {
            return;
        };
        let palette = theme::palette(ui.ctx());
        let status = team.status();
        let here: Vec<_> = team.roster().iter().filter(|c| c.online).cloned().collect();
        let response = ui
            .horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                let (dot, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
                ui.painter()
                    .circle_filled(dot.center(), 3.5, status_color(&status, &palette));
                for c in here.iter().take(4) {
                    avatar(ui, &c.profile.name, c.profile.color, 16.0);
                }
                if here.len() > 4 {
                    ui.label(widgets::micro(
                        &format!("+{}", here.len() - 4),
                        palette.text_weak,
                    ));
                }
            })
            .response
            .interact(egui::Sense::click());
        let names: Vec<&str> = here.iter().map(|c| c.profile.name.as_str()).collect();
        let hover = format!(
            "Team Create: {}. Here: {}.",
            status_text(&status),
            if names.is_empty() {
                "nobody yet".to_owned()
            } else {
                names.join(", ")
            }
        );
        if response.on_hover_text(hover).clicked() {
            self.layout.open(Pane::TeamCreate);
        }
    }

    /// The Team Create panel.
    pub(in crate::app) fn team_create_pane(&mut self, ui: &mut egui::Ui) {
        let mut action = None;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                action = if self.team.is_some() {
                    self.team_session_ui(ui)
                } else {
                    self.team_start_ui(ui)
                };
            });
        if let Some(action) = action {
            self.team_action(action);
        }
    }

    /// Outside a session: share this project, or join someone's.
    fn team_start_ui(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        let palette = theme::palette(ui.ctx());
        let mut action = None;
        let open = self.project.is_some();
        if widgets::empty_state(
            ui,
            icons::USERS_THREE,
            "Team Create",
            "Edit this project with others at the same time. Share it from this editor, \
             or join someone's session with their invite link.",
            None,
        ) {
            action = Some(Action::Share);
        }
        ui.add_space(theme::space::M);
        widgets::section(icons::SHARE_NETWORK, "Share this project").show(ui, |ui| {
            ui.add(
                egui::Label::new(
                    egui::RichText::new(
                        "Hosts a session from this editor on port 47810 and makes an invite \
                         link for an Editor. Others on your network (or a VPN such as \
                         Tailscale) join with it.",
                    )
                    .color(palette.text_weak),
                )
                .wrap(),
            );
            let share = widgets::button(
                widgets::Role::Primary,
                Some(icons::SHARE_NETWORK),
                "Share with Team Create",
                &palette,
            );
            if ui
                .add_enabled(open && !self.playing(), share)
                .on_disabled_hover_text("Open a project first, and stop Play.")
                .clicked()
            {
                action = Some(Action::Share);
            }
        });
        widgets::section(icons::SIGN_IN, "Join a session").show(ui, |ui| {
            ui.add(
                egui::Label::new(
                    egui::RichText::new(
                        "Paste the invite link you were sent. Open your own copy of the \
                         project first: until project files are shared, scripts, terrain \
                         and assets come from your copy.",
                    )
                    .color(palette.text_weak),
                )
                .wrap(),
            );
            ui.add(
                egui::TextEdit::singleline(&mut self.team_panel.link)
                    .hint_text("axcenity://teamcreate/join?…")
                    .desired_width(f32::INFINITY),
            );
            let join = widgets::button(
                widgets::Role::Secondary,
                Some(icons::SIGN_IN),
                "Join",
                &palette,
            );
            let ready = open && !self.playing() && !self.team_panel.link.trim().is_empty();
            if ui
                .add_enabled(ready, join)
                .on_disabled_hover_text("Open your copy of the project and paste a link.")
                .clicked()
            {
                action = Some(Action::Join);
            }
        });
        if let Some(problem) = &self.team_panel.problem {
            ui.add(egui::Label::new(egui::RichText::new(problem).color(palette.error)).wrap());
        }
        action
    }

    /// In a session: how it's going, who's here, invites, and leaving.
    fn team_session_ui(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        let palette = theme::palette(ui.ctx());
        let team = self.team.as_ref().expect("in a session");
        let mut action = None;
        let status = team.status();
        let hosting = team.hosting().is_some();
        let me = team.me();
        let my_role = team.client_role();

        // Connection.
        ui.horizontal(|ui| {
            let (dot, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
            ui.painter()
                .circle_filled(dot.center(), 4.5, status_color(&status, &palette));
            ui.label(egui::RichText::new(status_text(&status)).family(theme::medium_family()));
        });
        let place = if hosting {
            format!("Hosted by this editor on {}", team.address())
        } else {
            format!("Host: {}", team.address())
        };
        ui.label(egui::RichText::new(place).color(palette.text_weak));
        match &status {
            TeamStatus::Reconnecting { why, .. } => {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(format!(
                            "{why}. Keep working: your changes are sent once it's back."
                        ))
                        .color(palette.text_weak),
                    )
                    .wrap(),
                );
            }
            TeamStatus::Refused(why) => {
                ui.add(egui::Label::new(egui::RichText::new(why).color(palette.error)).wrap());
            }
            _ => {}
        }
        ui.add_space(theme::space::S);

        // Collaborators.
        let roster = team.roster().to_vec();
        widgets::section(icons::USERS_THREE, "Collaborators").show(ui, |ui| {
            if roster.is_empty() {
                ui.label(egui::RichText::new("Waiting for the host…").color(palette.text_weak));
            }
            for c in &roster {
                ui.horizontal(|ui| {
                    avatar(ui, &c.profile.name, c.profile.color, 18.0);
                    let mut name = c.profile.name.clone();
                    if Some(c.member) == me {
                        name.push_str(" (you)");
                    }
                    let text = egui::RichText::new(name);
                    ui.label(if c.online {
                        text
                    } else {
                        text.color(palette.text_weak)
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let can_manage = hosting
                            && my_role == Some(MemberRole::Owner)
                            && c.role != MemberRole::Owner;
                        if can_manage {
                            if widgets::icon_button(
                                ui,
                                icons::USER_MINUS,
                                "Remove from the project",
                                false,
                            )
                            .clicked()
                            {
                                action = Some(Action::Remove(c.member));
                            }
                            let mut role = c.role;
                            egui::ComboBox::from_id_salt(("team role", c.member.0))
                                .selected_text(format!("{role:?}"))
                                .width(70.0)
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut role, MemberRole::Editor, "Editor");
                                    ui.selectable_value(&mut role, MemberRole::Viewer, "Viewer");
                                });
                            if role != c.role {
                                action = Some(Action::SetRole(c.member, role));
                            }
                        } else {
                            ui.label(
                                egui::RichText::new(format!("{:?}", c.role))
                                    .color(palette.text_weak),
                            );
                        }
                        if !c.online {
                            ui.label(widgets::micro("offline", palette.text_weak));
                        }
                    });
                });
            }
        });

        // Invitations.
        if hosting {
            let invite = team.invite().map(str::to_owned);
            widgets::section(icons::LINK, "Invite").show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Role");
                    let role = &mut self.team_panel.invite_role;
                    ui.selectable_value(role, InviteRole::Editor, "Editor");
                    ui.selectable_value(role, InviteRole::Viewer, "Viewer");
                });
                if self.team_panel.invite_role == InviteRole::Viewer {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(
                                "A Viewer gets the whole project, scripts included: they can't \
                                 change it, but they can copy it.",
                            )
                            .color(palette.text_weak),
                        )
                        .wrap(),
                    );
                }
                let make = widgets::button(
                    widgets::Role::Secondary,
                    Some(icons::LINK),
                    "Create invite link",
                    &palette,
                );
                if ui.add(make).clicked() {
                    action = Some(Action::Invite);
                }
                if let Some(link) = invite {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new("Works once, for 7 days:").color(palette.text_weak),
                        )
                        .wrap(),
                    );
                    let mut shown = link.clone();
                    ui.add(
                        egui::TextEdit::singleline(&mut shown)
                            .desired_width(f32::INFINITY)
                            .font(egui::TextStyle::Monospace),
                    );
                    ui.horizontal(|ui| {
                        if ui
                            .add(widgets::button(
                                widgets::Role::Secondary,
                                Some(icons::COPY),
                                "Copy link",
                                &palette,
                            ))
                            .clicked()
                        {
                            ui.ctx().copy_text(link.clone());
                        }
                        if ui
                            .add(widgets::button(
                                widgets::Role::Ghost,
                                None,
                                "Revoke unused invites",
                                &palette,
                            ))
                            .clicked()
                        {
                            action = Some(Action::Revoke);
                        }
                    });
                }
            });
        }

        ui.add_space(theme::space::M);
        let leave = if hosting {
            widgets::button(
                widgets::Role::Danger,
                Some(icons::SIGN_OUT),
                "End session",
                &palette,
            )
        } else {
            widgets::button(
                widgets::Role::Secondary,
                Some(icons::SIGN_OUT),
                "Leave session",
                &palette,
            )
        };
        let hover = if hosting {
            "Disconnects everyone. Your World stays as it is."
        } else {
            "Your copy stays open here, no longer in step."
        };
        if ui.add(leave).on_hover_text(hover).clicked() {
            action = Some(Action::Leave);
        }
        action
    }

    fn team_action(&mut self, action: Action) {
        match action {
            Action::Share => {
                self.team_panel.problem = None;
                match self.host_network_session(None, axcenity_collab::socket::DEFAULT_PORT) {
                    Ok(invite) => self.output.info(format!(
                        "Team Create: hosting on {}. Invite: {}",
                        invite.host,
                        invite.link()
                    )),
                    Err(error) => self.team_panel.problem = Some(error),
                }
            }
            Action::Join => {
                self.team_panel.problem = None;
                let joined = Invite::parse(&self.team_panel.link)
                    .map_err(|e| e.to_string())
                    .and_then(|invite| self.join_network_session(&invite, None));
                match joined {
                    Ok(()) => self.team_panel.link.clear(),
                    Err(error) => self.team_panel.problem = Some(error),
                }
            }
            Action::Invite => {
                let role = self.team_panel.invite_role.role();
                let _ = self.make_team_invite(role);
            }
            Action::Revoke => self.revoke_team_invites(),
            Action::SetRole(member, role) => self.set_team_role(member, role),
            Action::Remove(member) => self.remove_team_member(member),
            Action::Leave => self.leave_team_session(),
        }
    }

    /// File > Share with Team Create…: opens the panel and hosts.
    pub(in crate::app) fn share_with_team_create(&mut self) {
        self.layout.open(Pane::TeamCreate);
        if self.team.is_none() {
            self.team_action(Action::Share);
        }
    }

    /// Runs a panel button's action, for tests.
    #[cfg(test)]
    pub(crate) fn team_action_for_test(&mut self, action: TestAction) {
        self.team_action(match action {
            TestAction::Join => Action::Join,
        });
    }

    /// File > Join Team Create…: opens the panel, where the link goes.
    pub(in crate::app) fn join_team_create(&mut self) {
        self.layout.open(Pane::TeamCreate);
    }
}
