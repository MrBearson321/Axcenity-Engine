//! The People window (T4 of `docs/research/teamwork.md`): pick a person and
//! see every change they made to the project, by day, with links back to
//! what they touched. It only draws; `app/people_history.rs` reads the
//! history and carries out what comes back as [`PeopleAction`]s.

use axcenity_teamwork::history::{History, What};
use axcenity_teamwork::item::same_person;
use axcenity_teamwork::{Board, Date, Link, time};
use eframe::egui::{self, RichText, vec2};

use crate::team_board::{avatar, person_color};
use crate::theme::{self, icons};
use crate::widgets;

/// Lines shown at most, so a year of history stays quick to draw.
const SHOWN: usize = 500;

/// What the window asked the app to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PeopleAction {
    SelectInstance(String),
    OpenFile {
        path: String,
        line: Option<u32>,
    },
    /// Open the board item with this ID.
    OpenItem(String),
    /// Read the history files again.
    Refresh,
}

/// The window's own state.
#[derive(Default)]
pub struct PeopleWindow {
    /// The person shown (this computer's person when none is picked).
    pub person: Option<String>,
    /// Only this kind of change.
    pub what: Option<What>,
}

fn what_icon(what: What) -> &'static str {
    match what {
        What::Edit => icons::CUBE,
        What::File => icons::FILE_CODE,
        What::Save => icons::FLOPPY_DISK,
        What::Board => icons::KANBAN,
    }
}

/// "Today", "Yesterday" or "Nov 10".
fn day_name(day: Date, today: Date) -> String {
    match today.days() - day.days() {
        0 => "Today".to_owned(),
        1 => "Yesterday".to_owned(),
        _ => day.short(today.year),
    }
}

impl PeopleWindow {
    /// Draws the window. `open` turns false when it's closed.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        open: &mut bool,
        history: &History,
        board: Option<&Board>,
        me: &str,
        now: u64,
    ) -> Vec<PeopleAction> {
        let mut actions = Vec::new();
        widgets::tool_window(icons::USERS, "People")
            .id(egui::Id::new("people history"))
            .size(vec2(440.0, 520.0))
            .show(ctx, open, |ui| {
                self.body(ui, history, board, me, now, &mut actions)
            });
        actions
    }

    fn body(
        &mut self,
        ui: &mut egui::Ui,
        history: &History,
        board: Option<&Board>,
        me: &str,
        now: u64,
        actions: &mut Vec<PeopleAction>,
    ) {
        let palette = theme::palette(ui.ctx());
        let today = Date::of(now);
        let mut people = history.people();
        if !people.iter().any(|p| same_person(p, me)) {
            people.push(me.to_owned());
        }
        let person = self
            .person
            .clone()
            .filter(|p| people.iter().any(|q| same_person(p, q)))
            .unwrap_or_else(|| me.to_owned());
        ui.horizontal_wrapped(|ui| {
            for name in &people {
                let on = same_person(name, &person);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    avatar(ui, name);
                    if widgets::chip(ui, name, on).clicked() {
                        self.person = Some(name.clone());
                    }
                });
            }
            if widgets::icon_button(ui, icons::ARROW_CLOCKWISE, "Read the history again", false)
                .clicked()
            {
                actions.push(PeopleAction::Refresh);
            }
        });
        ui.add_space(theme::space::XS);
        let week = history.changes_since(&person, now.saturating_sub(7 * 86_400));
        let last = history.of(&person, None).first().map(|e| e.at);
        let mut summary = format!(
            "{week} {} in the last 7 days",
            if week == 1 { "change" } else { "changes" }
        );
        if let Some(last) = last {
            summary.push_str(&format!(" · last {}", time::ago(last, now)));
        }
        if let Some(board) = board {
            let (open, late) = board.open_for(&person, today);
            if open > 0 {
                summary.push_str(&format!(" · {open} open on the board"));
                if late > 0 {
                    summary.push_str(&format!(", {late} overdue"));
                }
            }
        }
        ui.label(
            RichText::new(&person)
                .color(person_color(&person))
                .family(theme::medium_family())
                .size(theme::type_scale::BODY + 2.0),
        );
        ui.label(RichText::new(summary).color(palette.text_weak));
        crate::responsive::toolbar(ui, |ui| {
            if widgets::chip(ui, "All", self.what.is_none()).clicked() {
                self.what = None;
            }
            for what in What::ALL {
                let on = self.what == Some(what);
                if widgets::chip(ui, &theme::with_icon(what_icon(what), what.name()), on).clicked()
                {
                    self.what = if on { None } else { Some(what) };
                }
            }
        });
        for problem in &history.problems {
            ui.label(
                RichText::new(format!("{}  {problem}", icons::WARNING)).color(palette.warning),
            );
        }
        let days = history.days(&person, self.what);
        if days.is_empty() {
            widgets::empty_state(
                ui,
                icons::CLOCK_COUNTER_CLOCKWISE,
                "Nothing yet",
                &format!(
                    "What {person} changes shows here: saves, board changes, and in a Collaborate session every edit, kept in the project's team/history folder."
                ),
                None,
            );
            return;
        }
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                let mut shown = 0;
                for (day, entries) in days {
                    if shown >= SHOWN {
                        break;
                    }
                    ui.add_space(theme::space::XS);
                    let count: u32 = entries.iter().map(|e| e.count).sum();
                    ui.label(
                        RichText::new(format!("{}  ·  {count}", day_name(day, today)))
                            .color(palette.text_secondary)
                            .family(theme::medium_family()),
                    );
                    for entry in entries {
                        shown += 1;
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(what_icon(entry.what)).color(palette.text_muted),
                            );
                            let text = RichText::new(&entry.text).color(palette.text);
                            let label = match &entry.item {
                                Some(_) => egui::Label::new(text.color(palette.accent_text))
                                    .truncate()
                                    .sense(egui::Sense::click()),
                                None => egui::Label::new(text).truncate(),
                            };
                            let response = ui.add(label);
                            if let Some(id) = &entry.item
                                && response
                                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                                    .clicked()
                            {
                                actions.push(PeopleAction::OpenItem(id.clone()));
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.label(widgets::micro(
                                        &time::ago(entry.at, now),
                                        palette.text_muted,
                                    ));
                                    if entry.count > 1 {
                                        ui.label(widgets::micro(
                                            &format!("×{}", entry.count),
                                            palette.text_weak,
                                        ));
                                    }
                                },
                            );
                        });
                        if !entry.links.is_empty() {
                            ui.indent(("people links", entry.at, &entry.text), |ui| {
                                ui.horizontal_wrapped(|ui| {
                                    for link in entry.links.iter().take(4) {
                                        link_button(ui, link, actions);
                                    }
                                    if entry.links.len() > 4 {
                                        ui.label(widgets::micro(
                                            &format!("and {} more", entry.links.len() - 4),
                                            palette.text_muted,
                                        ));
                                    }
                                });
                            });
                        }
                        if !entry.details.is_empty() {
                            ui.indent(("people details", entry.at, &entry.text), |ui| {
                                egui::CollapsingHeader::new(
                                    RichText::new(format!("{} changes", entry.details.len()))
                                        .color(palette.text_weak),
                                )
                                .id_salt(("people details", entry.at, &entry.who))
                                .show(ui, |ui| {
                                    for detail in &entry.details {
                                        ui.add(
                                            egui::Label::new(
                                                RichText::new(detail).color(palette.text_secondary),
                                            )
                                            .wrap(),
                                        );
                                    }
                                });
                            });
                        }
                    }
                }
            });
    }
}

/// A link to an Instance or a file, which selects or opens it.
fn link_button(ui: &mut egui::Ui, link: &Link, actions: &mut Vec<PeopleAction>) {
    let palette = theme::palette(ui.ctx());
    let icon = match link {
        Link::Instance(_) => icons::CUBE,
        Link::File { .. } => icons::FILE_CODE,
    };
    let text = RichText::new(theme::with_icon(icon, &link.label()))
        .color(palette.accent_text)
        .size(theme::type_scale::SMALL);
    if ui
        .add(egui::Label::new(text).sense(egui::Sense::click()))
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
    {
        actions.push(match link {
            Link::Instance(path) => PeopleAction::SelectInstance(path.clone()),
            Link::File { path, line } => PeopleAction::OpenFile {
                path: path.clone(),
                line: *line,
            },
        });
    }
}
