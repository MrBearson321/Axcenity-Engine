//! The Collaborate panel's Chat (C8; design §8): the session's last 200
//! lines with names in each member's colour, Instance paths such as
//! `Workspace.Door` as links that select the Instance, an `@` button that
//! puts the selected Instance's path in the message, and the unread count
//! on the title bar chip. Chat text is plain: nothing in it is read as
//! markup.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axcenity_core::{InstanceId, World};

use super::team::Team;
use super::team_panel::{TeamPanel, member_color};
use super::*;

/// What the Chat section asked for this frame.
pub(super) enum ChatAction {
    /// Send the message being typed.
    Send,
    /// Select an Instance a message names.
    Select(InstanceId),
}

/// A piece of a chat line: plain text, or an Instance path that names an
/// Instance in the World now.
#[derive(Debug, PartialEq)]
pub(crate) enum Piece<'a> {
    Text(&'a str),
    Instance(&'a str, InstanceId),
}

/// Cuts `text` into plain text and Instance paths. A path is a word with a
/// dot in it (punctuation around it left out) that
/// [`resolve_path`](crate::console::resolve_path) finds, so "Door." or
/// "(Workspace.Door)" still link and "e.g." doesn't.
pub(crate) fn pieces<'a>(text: &'a str, world: &World) -> Vec<Piece<'a>> {
    let mut pieces = Vec::new();
    let mut plain = 0;
    let mut word_start = None;
    // A space after the text ends the last word too.
    for (i, c) in text.char_indices().chain([(text.len(), ' ')]) {
        match (c.is_whitespace(), word_start) {
            (false, None) => word_start = Some(i),
            (true, Some(start)) => {
                word_start = None;
                let word = &text[start..i];
                let core = word
                    .trim_start_matches(['(', '"', '\'', '@'])
                    .trim_end_matches(['.', ',', ';', ':', '!', '?', ')', '"', '\'']);
                if !core.contains('.') {
                    continue;
                }
                let Some(id) = crate::console::resolve_path(world, core) else {
                    continue;
                };
                let at = start + word.find(core).unwrap_or(0);
                if plain < at {
                    pieces.push(Piece::Text(&text[plain..at]));
                }
                pieces.push(Piece::Instance(core, id));
                plain = at + core.len();
            }
            _ => {}
        }
    }
    if plain < text.len() {
        pieces.push(Piece::Text(&text[plain..]));
    }
    pieces
}

/// The Chat section of the Collaborate panel. Drawing it marks the chat
/// read.
pub(super) fn chat_section(
    ui: &mut egui::Ui,
    panel: &mut TeamPanel,
    team: &Team,
    world: &World,
    selected: Option<InstanceId>,
) -> Option<ChatAction> {
    // A hosting editor shows its panel a few frames before it's let in.
    let lines = team.client()?.chat();
    let palette = theme::palette(ui.ctx());
    let mut action = None;
    panel.chat_unread = 0;
    let roster = team.roster();
    let now = SystemTime::now();
    widgets::section(icons::CHAT_CIRCLE_TEXT, "Chat").show(ui, |ui| {
        egui::ScrollArea::vertical()
            .id_salt("collaborate chat")
            .max_height(200.0)
            .auto_shrink([false, true])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                if lines.is_empty() {
                    ui.label(
                        egui::RichText::new("No messages yet. Everyone in the session sees them.")
                            .color(palette.text_weak),
                    );
                }
                for line in lines {
                    let who = roster.iter().find(|c| c.member == line.member);
                    let (name, color) = who.map_or(("Someone", palette.text_weak), |c| {
                        (c.profile.name.as_str(), member_color(c.profile.color))
                    });
                    let when = UNIX_EPOCH + Duration::from_secs(line.at);
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        ui.label(
                            egui::RichText::new(format!("{name}  "))
                                .color(color)
                                .family(theme::medium_family()),
                        )
                        .on_hover_text(crate::start::ago(when, now));
                        for piece in pieces(&line.text, world) {
                            match piece {
                                Piece::Text(text) => {
                                    ui.label(text);
                                }
                                Piece::Instance(path, id) => {
                                    if ui.link(path).on_hover_text("Select it").clicked() {
                                        action = Some(ChatAction::Select(id));
                                    }
                                }
                            }
                        }
                    });
                }
            });
        let mention = selected.and_then(|id| world.full_name(id));
        ui.horizontal(|ui| {
            let at = ui
                .add_enabled_ui(mention.is_some(), |ui| {
                    widgets::icon_button(ui, icons::AT, "Mention the selected Instance", false)
                })
                .inner;
            if let Some(path) = mention.as_ref().filter(|_| at.clicked()) {
                if !panel.chat_draft.is_empty() && !panel.chat_draft.ends_with(' ') {
                    panel.chat_draft.push(' ');
                }
                panel.chat_draft.push_str(path);
                panel.chat_draft.push(' ');
            }
            let send = widgets::icon_button(ui, icons::PAPER_PLANE_RIGHT, "Send", false);
            let response = ui.add(
                egui::TextEdit::singleline(&mut panel.chat_draft)
                    .id_salt("collaborate chat draft")
                    .hint_text("Message")
                    .desired_width(ui.available_width()),
            );
            let entered =
                response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
            if entered || send.clicked() {
                action = Some(ChatAction::Send);
            }
            if entered {
                response.request_focus();
            }
        });
        if let Some(problem) = &panel.chat_problem {
            ui.label(egui::RichText::new(problem).color(palette.error));
        }
    });
    action
}

impl EditorApp {
    /// Sends the message being typed, or says why it can't go.
    pub(super) fn send_team_chat(&mut self) {
        let Some(team) = self.team.as_mut() else {
            return;
        };
        match team.send_chat(&self.team_panel.chat_draft) {
            Ok(()) => {
                self.team_panel.chat_draft.clear();
                self.team_panel.chat_problem = None;
            }
            Err(problem) => self.team_panel.chat_problem = Some(problem),
        }
    }

    /// Others' chat lines that arrived since last frame, for the unread
    /// count on the title bar chip while the chat isn't shown.
    pub(super) fn count_unread_chat(&mut self) {
        if let Some(new) = self.team.as_mut().map(Team::take_new_chat) {
            self.team_panel.chat_unread += new;
        }
    }
}
