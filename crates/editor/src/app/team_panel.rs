//! The Collaborate panel and the title bar chip (TC4; design §11): share
//! the open project or join a session, see who's in it and how the
//! connection is, follow someone's camera, invite people and set their
//! roles, and leave. Also the Recovered Changes window and Settings > Team
//! Create (TC6).

use axcenity_collab::MemberId;
use axcenity_collab::Role as MemberRole;
use axcenity_collab::access::Invite;
use axcenity_collab::identity::Identity;

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
    /// How long new invites work.
    pub(crate) invite_lifetime: InviteLifetime,
    /// Edits sent again after a gap that the host refused, for the
    /// Recovered Changes window (TC6). It shows while there are any.
    pub(crate) recovered: Vec<RecoveredChange>,
    /// The member whose camera the viewport follows.
    pub(crate) following: Option<MemberId>,
    /// Where following last put the camera: if it's elsewhere now, the
    /// user moved it, and following stops.
    pub(crate) followed_to: Option<[f32; 3]>,
    /// A Version History checkpoint was kept before edits made offline
    /// were sent again, for the Recovered Changes window (C5).
    pub(crate) offline_checkpoint: bool,
    /// The Properties field this editor is dragging, for others to see
    /// (C6).
    pub(crate) editing_field: Option<String>,
    /// The setup wizard and Start Session dialog (C4).
    pub(crate) setup: super::collab_setup::Setup,
    /// The chat message being typed (C8).
    pub(crate) chat_draft: String,
    /// Others' chat lines since the chat was last shown, for the chip.
    pub(crate) chat_unread: usize,
    /// Why the last chat message didn't go.
    pub(crate) chat_problem: Option<String>,
    /// The name being typed for a new checkpoint (C9).
    pub(crate) checkpoint_label: String,
    /// The checkpoint the Owner pressed Restore on, asking once more.
    pub(crate) confirm_restore: Option<u64>,
    /// What the host said about the last checkpoint or restore asked for.
    pub(crate) history_problem: Option<String>,
    /// This computer's identity, for Settings > Collaborate (read when the
    /// page first shows).
    identity: Option<Result<Identity, String>>,
}

/// One edit in the Recovered Changes window.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RecoveredChange {
    /// Its undo step's name.
    pub(crate) label: String,
    /// Why the host refused it.
    pub(crate) reason: String,
}

/// How long a new invite link works (Settings > Collaborate).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum InviteLifetime {
    Day,
    #[default]
    Week,
    Month,
}

impl InviteLifetime {
    pub(crate) fn seconds(self) -> u64 {
        const DAY: u64 = 24 * 60 * 60;
        match self {
            InviteLifetime::Day => DAY,
            InviteLifetime::Week => 7 * DAY,
            InviteLifetime::Month => 30 * DAY,
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            InviteLifetime::Day => "1 day",
            InviteLifetime::Week => "7 days",
            InviteLifetime::Month => "30 days",
        }
    }
}

/// Where the invite defaults are kept between runs.
pub(crate) const TEAM_CREATE_KEY: &str = "team_create";

impl TeamPanel {
    /// The invite defaults as saved ("editor week").
    pub(crate) fn to_text(&self) -> String {
        let role = match self.invite_role {
            InviteRole::Editor => "editor",
            InviteRole::Viewer => "viewer",
        };
        let lifetime = match self.invite_lifetime {
            InviteLifetime::Day => "day",
            InviteLifetime::Week => "week",
            InviteLifetime::Month => "month",
        };
        let ready = if self.setup.done { " ready" } else { "" };
        format!("{role} {lifetime}{ready}")
    }

    /// Reads what [`TeamPanel::to_text`] wrote; unknown words keep the
    /// defaults.
    pub(crate) fn read_text(&mut self, text: &str) {
        for word in text.split_whitespace() {
            match word {
                "editor" => self.invite_role = InviteRole::Editor,
                "viewer" => self.invite_role = InviteRole::Viewer,
                "day" => self.invite_lifetime = InviteLifetime::Day,
                "week" => self.invite_lifetime = InviteLifetime::Week,
                "month" => self.invite_lifetime = InviteLifetime::Month,
                "ready" => self.setup.done = true,
                _ => {}
            }
        }
    }
}

/// The roles an invite can give.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum InviteRole {
    #[default]
    Editor,
    Viewer,
}

impl InviteRole {
    pub(crate) fn role(self) -> MemberRole {
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
    /// Follows a member's camera, or stops with `None`.
    Follow(Option<MemberId>),
    /// Something in the Chat section (C8).
    Chat(super::team_chat::ChatAction),
    /// Something in the History section (C9).
    History(super::team_history::HistoryAction),
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
        TeamStatus::Reconnecting { .. } | TeamStatus::Refused(_) | TeamStatus::HostLeft { .. } => {
            palette.error
        }
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
        TeamStatus::HostLeft { waiting: 0 } => "The host left (read-only)".to_owned(),
        TeamStatus::HostLeft { waiting } => {
            format!("The host left (read-only), {waiting} change(s) kept")
        }
    }
}

impl EditorApp {
    /// The title bar's chip while in a session: up to four collaborators
    /// who are here and a sync dot. Opens the Collaborate panel.
    pub(super) fn team_chip(&mut self, ui: &mut egui::Ui) {
        let Some(team) = &self.team else {
            return;
        };
        let palette = theme::palette(ui.ctx());
        let status = team.status();
        let unread = self.team_panel.chat_unread;
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
                if unread > 0 {
                    ui.label(widgets::micro(
                        &format!("{} {unread}", icons::CHAT_CIRCLE_TEXT),
                        palette.accent,
                    ));
                }
            })
            .response
            .interact(egui::Sense::click());
        let names: Vec<&str> = here.iter().map(|c| c.profile.name.as_str()).collect();
        let mut hover = format!(
            "Collaborate: {}. Here: {}.",
            status_text(&status),
            if names.is_empty() {
                "nobody yet".to_owned()
            } else {
                names.join(", ")
            }
        );
        match unread {
            0 => {}
            1 => hover.push_str(" 1 new chat message."),
            n => hover.push_str(&format!(" {n} new chat messages.")),
        }
        if response.on_hover_text(hover).clicked() {
            self.layout.open(Pane::TeamCreate);
        }
    }

    /// The Collaborate panel.
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
            "Collaborate",
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
                        "Hosts a session from this editor and makes an invite link. With \
                         Tailscale set up, people anywhere can join with it; without, people \
                         on your network.",
                    )
                    .color(palette.text_weak),
                )
                .wrap(),
            );
            let share = widgets::button(
                widgets::Role::Primary,
                Some(icons::SHARE_NETWORK),
                "Start Collaborate Session",
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
                        "Paste the invite link you were sent. The session's project opens \
                         here once its files arrive. If the host uses Tailscale, you need \
                         Tailscale running too (File > Set Up Collaborate…).",
                    )
                    .color(palette.text_weak),
                )
                .wrap(),
            );
            ui.add(
                egui::TextEdit::singleline(&mut self.team_panel.link)
                    .hint_text("axcenity://collaborate/join?…")
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
        // How others reach the host (C3): over Tailscale from anywhere, or
        // only on this network.
        let reach = match (&team.tailscale, hosting) {
            (Some(note), _) => Some(format!("Tailscale: {note}.")),
            (None, true) => {
                Some("Tailscale isn't connected: invites work on this network only.".to_owned())
            }
            (None, false) if axcenity_collab::tailscale::is_tailscale_host(team.address()) => {
                Some("Connected over Tailscale.".to_owned())
            }
            (None, false) => None,
        };
        if let Some(reach) = reach {
            ui.add(egui::Label::new(egui::RichText::new(reach).color(palette.text_weak)).wrap());
        }
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
            TeamStatus::HostLeft { waiting } => {
                let kept = if *waiting > 0 {
                    " Changes the host didn't get are kept and sent the next time you join."
                } else {
                    ""
                };
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(format!(
                            "The host ended the session. This copy stays open, read-only.{kept} \
                             Leave the session to work on it alone."
                        ))
                        .color(palette.text_weak),
                    )
                    .wrap(),
                );
            }
            _ => {}
        }
        ui.add_space(theme::space::S);

        // Collaborators.
        let roster = team.roster().to_vec();
        let with_camera: Vec<MemberId> = team
            .presences()
            .filter(|(_, presence)| presence.camera.is_some())
            .map(|(member, _)| member)
            .collect();
        let following = self.team_panel.following;
        if let Some(name) = following
            .and_then(|member| roster.iter().find(|c| c.member == member))
            .map(|c| c.profile.name.clone())
        {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("{} Following {name}", icons::EYE))
                        .color(palette.accent),
                );
                if ui
                    .add(widgets::button(
                        widgets::Role::Ghost,
                        None,
                        "Stop",
                        &palette,
                    ))
                    .on_hover_text("Or move the camera yourself.")
                    .clicked()
                {
                    action = Some(Action::Follow(None));
                }
            });
        }
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
                        if Some(c.member) != me && with_camera.contains(&c.member) {
                            let on = following == Some(c.member);
                            let hover = if on {
                                "Stop following"
                            } else {
                                "Follow their camera"
                            };
                            if widgets::icon_button(ui, icons::EYE, hover, on).clicked() {
                                action = Some(Action::Follow((!on).then_some(c.member)));
                            }
                        }
                    });
                });
            }
        });

        // Chat (C8).
        if let Some(chat) = super::team_chat::chat_section(
            ui,
            &mut self.team_panel,
            team,
            &self.world,
            self.selected,
        ) {
            action = Some(Action::Chat(chat));
        }

        // History (C9).
        if let Some(history) = super::team_history::history_section(ui, &mut self.team_panel, team)
        {
            action = Some(Action::History(history));
        }

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
                    let works = format!(
                        "Works once, for {}:",
                        self.team_panel.invite_lifetime.label()
                    );
                    ui.add(
                        egui::Label::new(egui::RichText::new(works).color(palette.text_weak))
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
                self.start_collaborate_session();
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
            Action::Follow(member) => {
                self.team_panel.following = member;
                self.team_panel.followed_to = None;
            }
            Action::Chat(super::team_chat::ChatAction::Send) => self.send_team_chat(),
            Action::Chat(super::team_chat::ChatAction::Select(id)) => {
                self.select_from_viewport(Some(id));
            }
            Action::History(history) => self.team_history_action(history),
        }
    }

    /// Keeps the viewport's camera on the followed member's, until they
    /// leave or the user moves the camera. Runs each frame in a session.
    pub(super) fn follow_member(&mut self) {
        let Some(member) = self.team_panel.following else {
            return;
        };
        let camera = self.team.as_ref().and_then(|team| {
            team.presences()
                .find(|(m, _)| *m == member)
                .and_then(|(_, presence)| presence.camera)
        });
        let here = self.viewport.render_camera().position.to_array();
        let moved_by_user = self.team_panel.followed_to.is_some_and(|to| {
            axcenity_render::glam::Vec3::from(to).distance(axcenity_render::glam::Vec3::from(here))
                > 1e-3
        });
        let Some(camera) = camera.filter(|_| !moved_by_user) else {
            self.team_panel.following = None;
            self.team_panel.followed_to = None;
            return;
        };
        let position = axcenity_render::glam::Vec3::from(camera.position);
        let look = axcenity_render::glam::Vec3::from(camera.look)
            .normalize_or(axcenity_render::glam::Vec3::NEG_Z);
        self.viewport.look_at(position, position + look);
        self.team_panel.followed_to = Some(self.viewport.render_camera().position.to_array());
    }

    /// The Recovered Changes window (design §9): edits made while offline,
    /// or before the editor or the host stopped, that the host refused when
    /// they were sent again. Nothing is dropped without saying so.
    pub(super) fn recovered_changes_window(&mut self, ctx: &egui::Context) {
        if self.team_panel.recovered.is_empty() {
            return;
        }
        let mut open = true;
        let mut close = false;
        let mut open_history = false;
        widgets::tool_window(icons::CLOCK_COUNTER_CLOCKWISE, "Recovered Changes")
            .id("recovered changes")
            .show(ctx, &mut open, |ui| {
                let palette = theme::palette(ui.ctx());
                ui.set_max_width(360.0);
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(
                            "These changes were made while you were offline (or before the \
                             editor or the host stopped) and couldn't be applied when they were \
                             sent again: someone changed or deleted what they edited. Make them \
                             again if you still want them.",
                        )
                        .color(palette.text_weak),
                    )
                    .wrap(),
                );
                if self.team_panel.offline_checkpoint {
                    ui.add_space(theme::space::XS);
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(format!(
                                "Your copy from before they were sent is in Version History as \
                                 “{}”: compare it with the session there.",
                                super::team::OFFLINE_CHECKPOINT
                            ))
                            .color(palette.text_weak),
                        )
                        .wrap(),
                    );
                }
                ui.add_space(theme::space::S);
                egui::ScrollArea::vertical()
                    .max_height(220.0)
                    .show(ui, |ui| {
                        for change in &self.team_panel.recovered {
                            ui.label(
                                egui::RichText::new(&change.label).family(theme::medium_family()),
                            );
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(&change.reason).color(palette.text_weak),
                                )
                                .wrap(),
                            );
                            ui.add_space(theme::space::XS);
                        }
                    });
                ui.add_space(theme::space::S);
                ui.horizontal(|ui| {
                    if ui
                        .add(widgets::button(
                            widgets::Role::Secondary,
                            Some(icons::COPY),
                            "Copy list",
                            &palette,
                        ))
                        .clicked()
                    {
                        let text: Vec<String> = self
                            .team_panel
                            .recovered
                            .iter()
                            .map(|c| format!("{}: {}", c.label, c.reason))
                            .collect();
                        ui.ctx().copy_text(text.join("\n"));
                    }
                    if self.team_panel.offline_checkpoint
                        && ui
                            .add(widgets::button(
                                widgets::Role::Secondary,
                                Some(icons::CLOCK_COUNTER_CLOCKWISE),
                                "Version History…",
                                &palette,
                            ))
                            .clicked()
                    {
                        open_history = true;
                    }
                    if ui
                        .add(widgets::button(
                            widgets::Role::Primary,
                            None,
                            "Done",
                            &palette,
                        ))
                        .clicked()
                    {
                        close = true;
                    }
                });
            });
        if open_history {
            self.version_history.open = true;
        }
        if !open || close {
            self.team_panel.recovered.clear();
        }
    }

    /// Settings > Collaborate: who this computer is to others, and what
    /// new invites give.
    pub(super) fn team_settings_page(&mut self, ui: &mut egui::Ui) {
        use super::settings;
        settings::page_title(
            ui,
            "Collaborate",
            "How others see you in a session, and the invites you make.",
        );
        let palette = theme::palette(ui.ctx());
        let weak = |ui: &mut egui::Ui, text: &str| {
            ui.add(egui::Label::new(egui::RichText::new(text).color(palette.text_weak)).wrap());
        };
        settings::section(ui, "Your name and colour");
        let identity = self
            .team_panel
            .identity
            .get_or_insert_with(super::team::load_own_identity);
        match identity {
            Err(error) => weak(ui, error),
            Ok(identity) => {
                let mut name = identity.name.clone();
                let mut color = identity.color;
                ui.horizontal(|ui| {
                    super::color_palette::edit_srgb(ui, &mut color)
                        .on_hover_text("Your colour: your selections and avatar show in it.");
                    ui.add(
                        egui::TextEdit::singleline(&mut name)
                            .hint_text("Your name")
                            .char_limit(axcenity_collab::presence::TEXT_MOST)
                            .desired_width(220.0),
                    );
                });
                let name = name.trim();
                if (name != identity.name && !name.is_empty()) || color != identity.color {
                    if !name.is_empty() {
                        identity.name = name.to_owned();
                    }
                    identity.color = color;
                    if let Err(error) = super::team::save_own_identity(identity) {
                        self.toasts
                            .warning(format!("Couldn't save your Collaborate name: {error}"));
                    }
                }
                weak(
                    ui,
                    "Others see this from the next time you join or share a session.",
                );
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(format!(
                        "Device key {}",
                        identity.public_key().fingerprint().short()
                    ))
                    .monospace()
                    .color(palette.text_weak),
                )
                .on_hover_text(
                    "This computer's key: the host knows you by it. It's kept in the editor's \
                     data folder, never in a project.",
                );
            }
        }
        ui.add_space(theme::space::M);
        settings::section(ui, "New invites");
        ui.horizontal(|ui| {
            ui.label("Role");
            let role = &mut self.team_panel.invite_role;
            ui.selectable_value(role, InviteRole::Editor, "Editor");
            ui.selectable_value(role, InviteRole::Viewer, "Viewer");
        });
        ui.horizontal(|ui| {
            ui.label("Works for");
            let lifetime = &mut self.team_panel.invite_lifetime;
            for choice in [
                InviteLifetime::Day,
                InviteLifetime::Week,
                InviteLifetime::Month,
            ] {
                ui.selectable_value(lifetime, choice, choice.label());
            }
        });
        weak(
            ui,
            "Each link works once. Revoke unused ones from the Collaborate panel.",
        );
    }

    /// File > Share with Collaborate…: opens the panel and hosts.
    /// File > Start Collaborate Session…: setup the first time, then the
    /// Start Session dialog (C4).
    pub(in crate::app) fn share_with_team_create(&mut self) {
        if self.team.is_none() {
            self.start_collaborate_session();
        } else {
            self.layout.open(Pane::TeamCreate);
        }
    }

    /// Runs a panel button's action, for tests.
    #[cfg(test)]
    pub(crate) fn team_action_for_test(&mut self, action: TestAction) {
        self.team_action(match action {
            TestAction::Join => Action::Join,
        });
    }

    /// File > Join Collaborate…: setup the first time, then the panel,
    /// where the link goes.
    pub(in crate::app) fn join_team_create(&mut self) {
        if self.team_panel.setup.done || self.team.is_some() {
            self.layout.open(Pane::TeamCreate);
        } else {
            self.team_panel
                .setup
                .open(super::collab_setup::Purpose::Join);
        }
    }
}
