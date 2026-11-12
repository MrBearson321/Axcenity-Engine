//! The Team Board panel (`docs/editor/team-board.md`): the project's tasks
//! as a list or as columns by status, who they're assigned to, and one
//! item's window with its fields, checklist, links, comments and activity.
//! The panel only draws: what to do comes back as [`BoardAction`]s, which
//! `app/team_board.rs` carries out (it writes the item's file, which a
//! Collaborate session and the project's repository then share).

use axcenity_teamwork::{
    Board, Date, Filter, Item, Kind, Link, Priority, Severity, Status, Whose, board,
    item::same_person, time,
};
use eframe::egui::{self, RichText, vec2};

use crate::theme::{self, icons};
use crate::widgets;

/// What the panel asked the app to do.
#[derive(Clone, Debug, PartialEq)]
pub enum BoardAction {
    /// Make a new item with this title (and open it).
    Create { kind: Kind, title: String },
    /// Write this changed item.
    Save(Box<Item>),
    /// Delete the item with this ID.
    Delete(String),
    /// Select the Instance at this path.
    SelectInstance(String),
    /// Open this file (at this line).
    OpenFile { path: String, line: Option<u32> },
    /// Make a bug report of the Console's last error (and open it).
    ReportLastError,
}

/// List or columns.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum View {
    #[default]
    List,
    Columns,
}

/// What the app tells the panel about the editor around it.
pub struct Context<'a> {
    pub board: &'a Board,
    /// This computer's person (the Collaborate name).
    pub me: &'a str,
    /// Everyone who can be assigned.
    pub people: &'a [String],
    /// Seconds since 1970.
    pub now: u64,
    /// The selected Instance's path, to link.
    pub selected: Option<String>,
    /// The script tab used last, to link.
    pub script: Option<String>,
    /// Can't change anything (a Viewer in a session, or no project).
    pub read_only: bool,
    /// The Console's last error, to report as a bug.
    pub last_error: Option<String>,
    /// This build and system, for a bug's Found in ("0.2.0-alpha.1 on
    /// windows").
    pub found_in: &'a str,
}

/// What's typed into the open item's fields, kept until it's committed.
#[derive(Default)]
struct Draft {
    id: String,
    title: String,
    description: String,
    due: String,
    labels: String,
    comment: String,
    check: String,
    person: String,
    steps: String,
    expected: String,
    actual: String,
    found_in: String,
    fixed_in: String,
    /// The Delete button was pressed once.
    deleting: bool,
}

impl Draft {
    fn of(item: &Item) -> Draft {
        Draft {
            id: item.id.clone(),
            title: item.title.clone(),
            description: item.description.clone(),
            due: item.due.map(|d| d.to_string()).unwrap_or_default(),
            labels: item.labels.join(", "),
            steps: bug_text(item, |b| &b.steps),
            expected: bug_text(item, |b| &b.expected),
            actual: bug_text(item, |b| &b.actual),
            found_in: bug_text(item, |b| &b.found_in),
            fixed_in: bug_text(item, |b| &b.fixed_in),
            ..Draft::default()
        }
    }
}

/// The panel's own state.
#[derive(Default)]
pub struct BoardPanel {
    pub filter: Filter,
    pub view: View,
    /// The title typed for a new item.
    pub new_title: String,
    /// What a new item will be.
    pub new_kind: Kind,
    /// The item whose window is open.
    pub open: Option<String>,
    draft: Draft,
}

/// The dragged card's item ID, in the columns view.
#[derive(Clone, Debug)]
struct DraggedCard(String);

/// A colour for `person` that's the same on every computer.
pub fn person_color(person: &str) -> egui::Color32 {
    const COLORS: [[u8; 3]; 8] = [
        [86, 156, 214],
        [214, 118, 86],
        [106, 176, 96],
        [190, 120, 200],
        [214, 170, 70],
        [80, 180, 180],
        [220, 100, 140],
        [140, 140, 220],
    ];
    let hash = person
        .trim()
        .to_lowercase()
        .bytes()
        .fold(7u32, |h, b| h.wrapping_mul(31).wrapping_add(u32::from(b)));
    let [r, g, b] = COLORS[(hash % COLORS.len() as u32) as usize];
    egui::Color32::from_rgb(r, g, b)
}

/// The person's initials in a small round badge; hover for the name.
fn avatar(ui: &mut egui::Ui, person: &str) -> egui::Response {
    let initials: String = person
        .split_whitespace()
        .filter_map(|w| w.chars().next())
        .take(2)
        .flat_map(char::to_uppercase)
        .collect();
    let size = 18.0;
    let (rect, response) = ui.allocate_exact_size(vec2(size, size), egui::Sense::hover());
    let painter = ui.painter();
    painter.circle_filled(rect.center(), size / 2.0, person_color(person));
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        if initials.is_empty() { "?" } else { &initials },
        egui::FontId::new(9.5, theme::medium_family()),
        egui::Color32::WHITE,
    );
    response.on_hover_text(person)
}

/// The icon and colour of a status.
pub fn status_look(status: Status, palette: &theme::Palette) -> (&'static str, egui::Color32) {
    match status {
        Status::Todo => (icons::CIRCLE_DASHED, palette.text_weak),
        Status::InProgress => (icons::CIRCLE_HALF, palette.accent_text),
        Status::InReview => (icons::EYE, palette.warning),
        Status::Done => (icons::CHECK_CIRCLE, palette.success),
        Status::Closed => (icons::PROHIBIT, palette.text_muted),
    }
}

fn priority_color(priority: Priority, palette: &theme::Palette) -> egui::Color32 {
    match priority {
        Priority::Urgent => palette.error,
        Priority::High => palette.warning,
        Priority::Normal => palette.text_weak,
        Priority::Low => palette.text_muted,
    }
}

pub fn kind_icon(kind: Kind) -> &'static str {
    match kind {
        Kind::Task => icons::CHECK_SQUARE,
        Kind::Bug => icons::BUG,
    }
}

fn severity_color(severity: Severity, palette: &theme::Palette) -> egui::Color32 {
    match severity {
        Severity::Blocker => palette.error,
        Severity::Major => palette.warning,
        Severity::Minor => palette.text_weak,
        Severity::Trivial => palette.text_muted,
    }
}

/// An item's kind icon, a bug's in its severity's colour.
fn kind_mark(item: &Item, palette: &theme::Palette) -> RichText {
    let color = match item.kind {
        Kind::Bug => severity_color(item.severity(), palette),
        Kind::Task => palette.text_muted,
    };
    RichText::new(kind_icon(item.kind)).color(color)
}

/// One of a bug's texts (empty for other kinds).
fn bug_text(item: &Item, field: impl Fn(&axcenity_teamwork::BugInfo) -> &String) -> String {
    item.bug
        .as_ref()
        .map(|b| field(b).clone())
        .unwrap_or_default()
}

/// "Nov 20", in the error colour when it's past and the item isn't done.
fn due_text(item: &Item, today: Date, palette: &theme::Palette) -> Option<RichText> {
    let due = item.due?;
    let color = if item.is_overdue(today) {
        palette.error
    } else if due == today && !item.status.is_finished() {
        palette.warning
    } else {
        palette.text_weak
    };
    Some(RichText::new(due.short(today.year)).color(color))
}

impl BoardPanel {
    /// Whose names the status headings use: bugs' when only bugs show.
    fn heading_kind(&self) -> Kind {
        match self.filter.kinds.as_slice() {
            [kind] => *kind,
            _ => Kind::Task,
        }
    }

    /// Opens `id`'s window.
    pub fn open_item(&mut self, item: &Item) {
        self.open = Some(item.id.clone());
        self.draft = Draft::of(item);
    }

    /// Draws the panel. Returns what to do.
    pub fn show(&mut self, ui: &mut egui::Ui, cx: &Context<'_>) -> Vec<BoardAction> {
        let mut actions = Vec::new();
        let palette = theme::palette(ui.ctx());
        self.toolbar(ui, cx, &mut actions);
        for problem in &cx.board.problems {
            ui.label(
                RichText::new(format!("{}  {problem}", icons::WARNING)).color(palette.warning),
            );
        }
        let items = cx.board.filtered(&self.filter);
        if cx.board.is_empty() {
            widgets::empty_state(
                ui,
                icons::KANBAN,
                "No tasks yet",
                "Type a task above and press Enter. Everyone in the project sees it, in a Collaborate session and through the project's repository.",
                None,
            );
            return actions;
        }
        if items.is_empty() {
            ui.add_space(theme::space::S);
            ui.label(RichText::new("Nothing matches the filter.").color(palette.text_weak));
            return actions;
        }
        // Columns need room; a narrow panel shows the list.
        let columns = self.view == View::Columns && ui.available_width() >= 520.0;
        if columns {
            self.columns(ui, cx, &items, &mut actions);
        } else {
            self.list(ui, cx, &items, &mut actions);
        }
        actions
    }

    fn toolbar(&mut self, ui: &mut egui::Ui, cx: &Context<'_>, actions: &mut Vec<BoardAction>) {
        let palette = theme::palette(ui.ctx());
        ui.horizontal(|ui| {
            ui.add_enabled_ui(!cx.read_only, |ui| {
                ui.menu_button(
                    RichText::new(kind_icon(self.new_kind)).color(palette.text_secondary),
                    |ui| {
                        for kind in Kind::ALL {
                            let text = theme::with_icon(kind_icon(kind), kind.name());
                            if ui.selectable_label(self.new_kind == kind, text).clicked() {
                                self.new_kind = kind;
                                ui.close();
                            }
                        }
                    },
                )
                .response
                .on_hover_text("What to add: a task or a bug");
            });
            let width = (ui.available_width() - 34.0).max(60.0);
            let hint = format!("New {}…", self.new_kind.name().to_lowercase());
            let response = ui.add_enabled(
                !cx.read_only,
                egui::TextEdit::singleline(&mut self.new_title)
                    .hint_text(RichText::new(hint).color(palette.text_muted))
                    .desired_width(width),
            );
            let enter = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let add = ui
                .add_enabled_ui(!cx.read_only && !self.new_title.trim().is_empty(), |ui| {
                    widgets::icon_button(ui, icons::PLUS, "Add the task", false)
                })
                .inner
                .clicked();
            if (enter || add) && !self.new_title.trim().is_empty() && !cx.read_only {
                actions.push(BoardAction::Create {
                    kind: self.new_kind,
                    title: std::mem::take(&mut self.new_title),
                });
                if enter {
                    response.request_focus();
                }
            }
        });
        if self.new_kind == Kind::Bug
            && !cx.read_only
            && let Some(error) = &cx.last_error
        {
            let first = error.lines().next().unwrap_or_default();
            if ui
                .small_button(theme::with_icon(
                    icons::WARNING_CIRCLE,
                    "Report the last error",
                ))
                .on_hover_text(first)
                .clicked()
            {
                actions.push(BoardAction::ReportLastError);
            }
        }
        crate::responsive::toolbar(ui, |ui| {
            widgets::segmented(
                ui,
                &mut self.view,
                &[
                    (View::List, Some(icons::ROWS), "List"),
                    (View::Columns, Some(icons::KANBAN), "Board"),
                ],
            );
            crate::responsive::room_for(ui, 150.0);
            widgets::search_field(ui, &mut self.filter.text, "Filter", 110.0);
        });
        crate::responsive::toolbar(ui, |ui| {
            let mine = Whose::Person(cx.me.to_owned());
            let choices = [
                (Whose::Everyone, "Everyone".to_owned()),
                (mine, "Mine".to_owned()),
                (Whose::Unassigned, "Unassigned".to_owned()),
            ];
            for (whose, label) in choices {
                if widgets::chip(ui, &label, self.filter.whose == whose).clicked() {
                    self.filter.whose = whose;
                }
            }
            let someone = match &self.filter.whose {
                Whose::Person(p) if !same_person(p, cx.me) => Some(p.clone()),
                _ => None,
            };
            let label = someone.as_deref().unwrap_or("Person");
            ui.menu_button(
                RichText::new(format!("{label} {}", icons::CARET_DOWN)).color(palette.text_weak),
                |ui| {
                    for person in cx.people {
                        if ui.button(person).clicked() {
                            self.filter.whose = Whose::Person(person.clone());
                            ui.close();
                        }
                    }
                },
            );
            let labels = cx.board.labels();
            if !labels.is_empty() || self.filter.label.is_some() {
                let current = self.filter.label.clone().unwrap_or_else(|| "Label".into());
                ui.menu_button(
                    RichText::new(format!("{current} {}", icons::CARET_DOWN))
                        .color(palette.text_weak),
                    |ui| {
                        if ui.button("Any label").clicked() {
                            self.filter.label = None;
                            ui.close();
                        }
                        for label in labels {
                            if ui.button(&label).clicked() {
                                self.filter.label = Some(label);
                                ui.close();
                            }
                        }
                    },
                );
            }
            for kind in Kind::ALL {
                let plural = kind.plural();
                let label = format!("{}{}", plural[..1].to_uppercase(), &plural[1..]);
                let on = self.filter.kinds == [kind];
                if widgets::chip(ui, &label, on).clicked() {
                    self.filter.kinds = if on { Vec::new() } else { vec![kind] };
                }
            }
            ui.checkbox(&mut self.filter.finished, "Done");
        });
        let today = Date::of(cx.now);
        let (open, late) = cx.board.open_for(cx.me, today);
        if open > 0 {
            let text = if late > 0 {
                format!("{open} assigned to you, {late} overdue")
            } else {
                format!("{open} assigned to you")
            };
            let color = if late > 0 {
                palette.error
            } else {
                palette.text_weak
            };
            ui.label(widgets::micro(&text, color));
        }
        let (bugs, blockers) = cx.board.open_bugs();
        if bugs > 0 {
            let text = if blockers > 0 {
                format!("{bugs} open bugs, {blockers} blocking")
            } else {
                format!("{bugs} open bugs")
            };
            let color = if blockers > 0 {
                palette.error
            } else {
                palette.text_weak
            };
            ui.label(widgets::micro(&text, color));
        }
        ui.add_space(theme::space::XS);
    }

    /// One item's row: status, title, assignees, due date.
    fn row(
        &mut self,
        ui: &mut egui::Ui,
        cx: &Context<'_>,
        item: &Item,
        actions: &mut Vec<BoardAction>,
    ) -> egui::Response {
        let palette = theme::palette(ui.ctx());
        let today = Date::of(cx.now);
        let row = egui::Layout::right_to_left(egui::Align::Center);
        let inner = ui.allocate_ui_with_layout(
            vec2(ui.available_width(), ui.spacing().interact_size.y + 2.0),
            row,
            |ui| {
                if let Some(due) = due_text(item, today, &palette) {
                    ui.label(due);
                }
                for person in item.assignees.iter().rev().take(3) {
                    avatar(ui, person);
                }
                if item.priority <= Priority::High {
                    ui.label(
                        RichText::new(icons::FLAG).color(priority_color(item.priority, &palette)),
                    )
                    .on_hover_text(item.priority.name());
                }
                let (done, total) = item.checked();
                if total > 0 {
                    ui.label(widgets::micro(
                        &format!("{done}/{total}"),
                        palette.text_weak,
                    ));
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    self.status_button(ui, cx, item, actions);
                    if item.kind == Kind::Bug {
                        ui.label(kind_mark(item, &palette))
                            .on_hover_text(format!("Bug, {}", item.severity().name()));
                    }
                    let title = RichText::new(&item.title).color(if item.status.is_finished() {
                        palette.text_weak
                    } else {
                        palette.text
                    });
                    ui.add(
                        egui::Label::new(title)
                            .truncate()
                            .sense(egui::Sense::click()),
                    )
                })
                .inner
            },
        );
        let response = inner.inner.union(inner.response);
        if response.clicked() {
            self.open_item(item);
        }
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    }

    /// The status icon, which opens a menu of statuses.
    fn status_button(
        &self,
        ui: &mut egui::Ui,
        cx: &Context<'_>,
        item: &Item,
        actions: &mut Vec<BoardAction>,
    ) {
        let palette = theme::palette(ui.ctx());
        let (icon, color) = status_look(item.status, &palette);
        let button = egui::Button::new(RichText::new(icon).color(color)).frame(false);
        let response = ui
            .add_enabled(!cx.read_only, button)
            .on_hover_text(item.status.name(item.kind));
        egui::Popup::menu(&response).show(|ui| {
            for status in Status::ALL {
                let (icon, color) = status_look(status, &palette);
                let text =
                    RichText::new(theme::with_icon(icon, status.name(item.kind))).color(color);
                if ui.selectable_label(item.status == status, text).clicked() {
                    let mut changed = item.clone();
                    changed.set_status(status, cx.me, cx.now);
                    actions.push(BoardAction::Save(Box::new(changed)));
                    ui.close();
                }
            }
        });
    }

    fn list(
        &mut self,
        ui: &mut egui::Ui,
        cx: &Context<'_>,
        items: &[&Item],
        actions: &mut Vec<BoardAction>,
    ) {
        let palette = theme::palette(ui.ctx());
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                let heading = self.heading_kind();
                for status in Status::ALL {
                    let group: Vec<&&Item> = items.iter().filter(|i| i.status == status).collect();
                    if group.is_empty() {
                        continue;
                    }
                    let (icon, color) = status_look(status, &palette);
                    ui.add_space(theme::space::XS);
                    ui.label(
                        RichText::new(format!(
                            "{icon}  {}  ·  {}",
                            status.name(heading),
                            group.len()
                        ))
                        .color(color)
                        .family(theme::medium_family()),
                    );
                    for item in group {
                        self.row(ui, cx, item, actions);
                    }
                }
            });
    }

    fn columns(
        &mut self,
        ui: &mut egui::Ui,
        cx: &Context<'_>,
        items: &[&Item],
        actions: &mut Vec<BoardAction>,
    ) {
        let palette = theme::palette(ui.ctx());
        let statuses = board::columns();
        let heading = self.heading_kind();
        let gap = theme::space::S;
        let width = ((ui.available_width() - gap * (statuses.len() as f32 - 1.0) - 4.0)
            / statuses.len() as f32)
            .max(150.0);
        // Room for the horizontal scroll bar under the columns.
        let height = ui.available_height() - 12.0;
        egui::ScrollArea::horizontal()
            .auto_shrink(false)
            .show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    for status in statuses {
                        let in_column: Vec<&&Item> = items
                            .iter()
                            .filter(|i| {
                                i.status == status
                                    || (status == Status::Done && i.status == Status::Closed)
                            })
                            .collect();
                        let frame = egui::Frame::new()
                            .fill(palette.inset)
                            .corner_radius(theme::radius::CONTROL)
                            .inner_margin(theme::space::margin(theme::space::S, theme::space::S));
                        let inner = width - 2.0 * theme::space::S;
                        let (_, dropped) = ui.dnd_drop_zone::<DraggedCard, ()>(frame, |ui| {
                            ui.set_width(inner);
                            ui.set_min_height(height - 2.0 * theme::space::S);
                            ui.vertical(|ui| {
                                let (icon, color) = status_look(status, &palette);
                                ui.label(
                                    RichText::new(format!(
                                        "{icon}  {}  ·  {}",
                                        status.name(heading),
                                        in_column.len()
                                    ))
                                    .color(color)
                                    .family(theme::medium_family()),
                                );
                                egui::ScrollArea::vertical()
                                    .id_salt(("board column", status))
                                    .auto_shrink([false, true])
                                    .show(ui, |ui| {
                                        for item in in_column {
                                            self.card(ui, cx, item, inner, actions);
                                        }
                                    });
                            });
                        });
                        if let Some(card) = dropped
                            && !cx.read_only
                            && let Some(item) = cx.board.get(&card.0)
                            && item.status != status
                        {
                            let mut changed = item.clone();
                            changed.set_status(status, cx.me, cx.now);
                            actions.push(BoardAction::Save(Box::new(changed)));
                        }
                    }
                });
            });
    }

    /// One item as a card in a column: drag it to another column.
    fn card(
        &mut self,
        ui: &mut egui::Ui,
        cx: &Context<'_>,
        item: &Item,
        width: f32,
        actions: &mut Vec<BoardAction>,
    ) {
        let palette = theme::palette(ui.ctx());
        let today = Date::of(cx.now);
        let id = egui::Id::new(("board card", &item.id));
        // The frame's own margins and line come on top of what's inside.
        let width = width - 2.0 * theme::space::S - 2.0;
        let frame = egui::Frame::new()
            .fill(palette.surface)
            .stroke(egui::Stroke::new(1.0, palette.border))
            .corner_radius(theme::radius::CONTROL)
            .inner_margin(theme::space::margin(theme::space::S, theme::space::XS));
        let body = |ui: &mut egui::Ui| {
            frame.show(ui, |ui| {
                ui.set_width(width);
                ui.add(egui::Label::new(RichText::new(&item.title).color(palette.text)).wrap());
                ui.horizontal(|ui| {
                    if item.kind == Kind::Bug {
                        ui.label(kind_mark(item, &palette));
                    }
                    if item.priority <= Priority::High {
                        ui.label(
                            RichText::new(icons::FLAG)
                                .color(priority_color(item.priority, &palette)),
                        );
                    }
                    if let Some(due) = due_text(item, today, &palette) {
                        ui.label(due);
                    }
                    let (done, total) = item.checked();
                    if total > 0 {
                        ui.label(widgets::micro(
                            &format!("{done}/{total}"),
                            palette.text_weak,
                        ));
                    }
                    if !item.comments.is_empty() {
                        ui.label(widgets::micro(
                            &format!("{} {}", icons::CHAT_CIRCLE, item.comments.len()),
                            palette.text_weak,
                        ));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        for person in item.assignees.iter().rev().take(3) {
                            avatar(ui, person);
                        }
                    });
                });
            });
        };
        let response = if cx.read_only {
            ui.scope(body).response
        } else {
            ui.dnd_drag_source(id, DraggedCard(item.id.clone()), body)
                .response
        };
        if response.clicked() {
            self.open_item(item);
        }
        let _ = actions;
        ui.add_space(theme::space::XS);
    }

    /// Draws the open item's window, if one is open.
    pub fn window(&mut self, ctx: &egui::Context, cx: &Context<'_>) -> Vec<BoardAction> {
        let mut actions = Vec::new();
        let Some(id) = self.open.clone() else {
            return actions;
        };
        let Some(item) = cx.board.get(&id) else {
            // Deleted (here or by someone else).
            self.open = None;
            return actions;
        };
        if self.draft.id != id {
            self.draft = Draft::of(item);
        }
        let mut open = true;
        let title = format!("{} {}", item.kind.name(), item.short_id());
        widgets::tool_window(kind_icon(item.kind), &title)
            .id(("board item", &id))
            .size(vec2(460.0, 560.0))
            .show(ctx, &mut open, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, true])
                    .show(ui, |ui| self.item_body(ui, cx, item, &mut actions));
            });
        if !open {
            self.open = None;
        }
        actions
    }

    fn item_body(
        &mut self,
        ui: &mut egui::Ui,
        cx: &Context<'_>,
        item: &Item,
        actions: &mut Vec<BoardAction>,
    ) {
        let palette = theme::palette(ui.ctx());
        let today = Date::of(cx.now);
        let me = cx.me;
        let now = cx.now;
        let mut changed = item.clone();
        ui.add_enabled_ui(!cx.read_only, |ui| {
            // Title.
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.draft.title)
                    .font(egui::FontId::new(
                        theme::type_scale::BODY + 3.0,
                        theme::medium_family(),
                    ))
                    .desired_width(f32::INFINITY),
            );
            if response.lost_focus() {
                changed.rename(&self.draft.title, me, now);
                self.draft.title = changed.title.clone();
            }
            ui.add_space(theme::space::XS);
            crate::responsive::form(ui, "board item fields", |ui| {
                ui.label("Kind");
                egui::ComboBox::from_id_salt("board kind")
                    .selected_text(theme::with_icon(kind_icon(item.kind), item.kind.name()))
                    .show_ui(ui, |ui| {
                        for kind in Kind::ALL {
                            let text = theme::with_icon(kind_icon(kind), kind.name());
                            if ui.selectable_label(item.kind == kind, text).clicked()
                                && kind != item.kind
                            {
                                changed.kind = kind;
                                if kind == Kind::Bug {
                                    changed.bug_mut();
                                }
                                changed.touch(me, now);
                            }
                        }
                    });
                ui.end_row();

                if item.kind == Kind::Bug {
                    ui.label("Severity");
                    egui::ComboBox::from_id_salt("board severity")
                        .selected_text(
                            RichText::new(item.severity().name())
                                .color(severity_color(item.severity(), &palette)),
                        )
                        .show_ui(ui, |ui| {
                            for severity in Severity::ALL {
                                if ui
                                    .selectable_label(item.severity() == severity, severity.name())
                                    .clicked()
                                {
                                    changed.set_severity(severity, me, now);
                                }
                            }
                        });
                    ui.end_row();
                }

                ui.label("Status");
                egui::ComboBox::from_id_salt("board status")
                    .selected_text(item.status.name(item.kind))
                    .show_ui(ui, |ui| {
                        for status in Status::ALL {
                            if ui
                                .selectable_label(item.status == status, status.name(item.kind))
                                .clicked()
                            {
                                changed.set_status(status, me, now);
                            }
                        }
                    });
                ui.end_row();

                ui.label("Priority");
                egui::ComboBox::from_id_salt("board priority")
                    .selected_text(
                        RichText::new(item.priority.name())
                            .color(priority_color(item.priority, &palette)),
                    )
                    .show_ui(ui, |ui| {
                        for priority in Priority::ALL {
                            if ui
                                .selectable_label(item.priority == priority, priority.name())
                                .clicked()
                            {
                                changed.set_priority(priority, me, now);
                            }
                        }
                    });
                ui.end_row();

                ui.label("Assigned");
                ui.horizontal_wrapped(|ui| {
                    for person in &item.assignees {
                        avatar(ui, person);
                        ui.label(person);
                        if widgets::icon_button(ui, icons::X, &format!("Unassign {person}"), false)
                            .clicked()
                        {
                            changed.unassign(person, me, now);
                        }
                    }
                    ui.menu_button(
                        RichText::new(theme::with_icon(icons::USER_PLUS, "Assign"))
                            .color(palette.accent_text),
                        |ui| {
                            if !item.is_assigned(me) && ui.button(format!("Me ({me})")).clicked() {
                                changed.assign(me, me, now);
                                ui.close();
                            }
                            for person in cx.people {
                                if !item.is_assigned(person)
                                    && !same_person(person, me)
                                    && ui.button(person).clicked()
                                {
                                    changed.assign(person, me, now);
                                    ui.close();
                                }
                            }
                            ui.separator();
                            ui.horizontal(|ui| {
                                let response = ui.add(
                                    egui::TextEdit::singleline(&mut self.draft.person)
                                        .hint_text("Someone else")
                                        .desired_width(120.0),
                                );
                                let enter = response.lost_focus()
                                    && ui.input(|i| i.key_pressed(egui::Key::Enter));
                                if (ui.button("Add").clicked() || enter)
                                    && !self.draft.person.trim().is_empty()
                                {
                                    changed.assign(
                                        &std::mem::take(&mut self.draft.person),
                                        me,
                                        now,
                                    );
                                    ui.close();
                                }
                            });
                        },
                    );
                });
                ui.end_row();

                ui.label("Due");
                ui.horizontal(|ui| {
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut self.draft.due)
                            .hint_text("YYYY-MM-DD")
                            .desired_width(92.0),
                    );
                    if response.lost_focus() {
                        let text = self.draft.due.trim();
                        if text.is_empty() {
                            changed.set_due(None, me, now);
                        } else if let Some(date) = Date::parse(text) {
                            changed.set_due(Some(date), me, now);
                            self.draft.due = date.to_string();
                        }
                    }
                    if !self.draft.due.trim().is_empty() && Date::parse(&self.draft.due).is_none() {
                        ui.label(RichText::new(icons::WARNING).color(palette.warning))
                            .on_hover_text("Type a date as YYYY-MM-DD");
                    }
                    for (label, days) in [("Today", 0), ("+1 week", 7)] {
                        if ui.small_button(label).clicked() {
                            let date = today.add_days(days);
                            changed.set_due(Some(date), me, now);
                            self.draft.due = date.to_string();
                        }
                    }
                });
                ui.end_row();

                ui.label("Labels");
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.draft.labels)
                        .hint_text("art, level 2")
                        .desired_width(f32::INFINITY),
                );
                if response.lost_focus() {
                    let mut labels: Vec<String> = self
                        .draft
                        .labels
                        .split(',')
                        .map(|l| axcenity_teamwork::item::tidy(l, 40))
                        .filter(|l| !l.is_empty())
                        .collect();
                    labels.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
                    if labels != item.labels {
                        changed.labels = labels;
                        changed.touch(me, now);
                    }
                }
                ui.end_row();

                if item.kind == Kind::Bug {
                    ui.label("Found in");
                    ui.horizontal(|ui| {
                        let response = ui.add(
                            egui::TextEdit::singleline(&mut self.draft.found_in)
                                .hint_text("Version and system")
                                .desired_width((ui.available_width() - 80.0).max(80.0)),
                        );
                        let this = ui.small_button("This build").on_hover_text(cx.found_in);
                        if this.clicked() {
                            self.draft.found_in = cx.found_in.to_owned();
                        }
                        if response.lost_focus() || this.clicked() {
                            set_bug_text(&mut changed, me, now, &self.draft.found_in, |b| {
                                &mut b.found_in
                            });
                        }
                    });
                    ui.end_row();

                    ui.label("Fixed in");
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut self.draft.fixed_in)
                            .hint_text("Checkpoint, commit or version")
                            .desired_width(f32::INFINITY),
                    );
                    if response.lost_focus() {
                        set_bug_text(&mut changed, me, now, &self.draft.fixed_in, |b| {
                            &mut b.fixed_in
                        });
                    }
                    ui.end_row();
                }
            });
            if item.kind == Kind::Bug {
                for (label, hint, field) in [
                    (
                        "Steps to reproduce",
                        "1. Press Play\n2. Walk into the door",
                        0,
                    ),
                    ("Expected", "What should happen", 1),
                    (
                        "What happens",
                        "What happens instead (an error message, say)",
                        2,
                    ),
                ] {
                    ui.add_space(theme::space::S);
                    widgets::section_label(ui, label);
                    let text = match field {
                        0 => &mut self.draft.steps,
                        1 => &mut self.draft.expected,
                        _ => &mut self.draft.actual,
                    };
                    let response = ui.add(
                        egui::TextEdit::multiline(text)
                            .hint_text(hint)
                            .desired_rows(2)
                            .desired_width(f32::INFINITY),
                    );
                    if response.lost_focus() {
                        set_bug_text(&mut changed, me, now, text, |b| match field {
                            0 => &mut b.steps,
                            1 => &mut b.expected,
                            _ => &mut b.actual,
                        });
                    }
                }
            }

            ui.add_space(theme::space::S);
            widgets::section_label(ui, "Description");
            let response = ui.add(
                egui::TextEdit::multiline(&mut self.draft.description)
                    .hint_text("What needs doing, and how to tell it's done")
                    .desired_rows(3)
                    .desired_width(f32::INFINITY),
            );
            if response.lost_focus() {
                let text = axcenity_teamwork::item::tidy(
                    &self.draft.description,
                    axcenity_teamwork::item::MAX_TEXT,
                );
                if text != item.description {
                    changed.description = text;
                    changed.touch(me, now);
                }
            }

            ui.add_space(theme::space::S);
            let (done, total) = item.checked();
            widgets::section_label(
                ui,
                &if total > 0 {
                    format!("Checklist  {done}/{total}")
                } else {
                    "Checklist".to_owned()
                },
            );
            let mut remove = None;
            for (index, check) in item.checklist.iter().enumerate() {
                ui.horizontal(|ui| {
                    let mut ticked = check.done;
                    if ui.checkbox(&mut ticked, &check.text).changed() {
                        changed.checklist[index].done = ticked;
                        changed.touch(me, now);
                    }
                    if widgets::icon_button(ui, icons::X, "Remove", false).clicked() {
                        remove = Some(index);
                    }
                });
            }
            if let Some(index) = remove {
                changed.checklist.remove(index);
                changed.touch(me, now);
            }
            ui.horizontal(|ui| {
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.draft.check)
                        .hint_text("Add a step")
                        .desired_width(ui.available_width() - 30.0),
                );
                let enter = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if (widgets::icon_button(ui, icons::PLUS, "Add the step", false).clicked() || enter)
                    && !self.draft.check.trim().is_empty()
                {
                    changed.add_check(&std::mem::take(&mut self.draft.check), me, now);
                    if enter {
                        response.request_focus();
                    }
                }
            });

            ui.add_space(theme::space::S);
            widgets::section_label(ui, "Links");
            let mut unlink = None;
            for (index, link) in item.links.iter().enumerate() {
                ui.horizontal(|ui| {
                    let icon = match link {
                        Link::Instance(_) => icons::CUBE,
                        Link::File { .. } => icons::FILE_CODE,
                    };
                    let text = RichText::new(theme::with_icon(icon, &link.label()))
                        .color(palette.accent_text);
                    if ui
                        .add(
                            egui::Label::new(text)
                                .truncate()
                                .sense(egui::Sense::click()),
                        )
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .clicked()
                    {
                        actions.push(match link {
                            Link::Instance(path) => BoardAction::SelectInstance(path.clone()),
                            Link::File { path, line } => BoardAction::OpenFile {
                                path: path.clone(),
                                line: *line,
                            },
                        });
                    }
                    if widgets::icon_button(ui, icons::X, "Remove the link", false).clicked() {
                        unlink = Some(index);
                    }
                });
            }
            if let Some(index) = unlink {
                changed.links.remove(index);
                changed.touch(me, now);
            }
            ui.horizontal_wrapped(|ui| {
                if let Some(path) = &cx.selected
                    && ui
                        .small_button(theme::with_icon(icons::CUBE, &format!("Link {path}")))
                        .clicked()
                {
                    changed.add_link(Link::Instance(path.clone()), me, now);
                }
                if let Some(path) = &cx.script
                    && ui
                        .small_button(theme::with_icon(icons::FILE_CODE, &format!("Link {path}")))
                        .clicked()
                {
                    changed.add_link(
                        Link::File {
                            path: path.clone(),
                            line: None,
                        },
                        me,
                        now,
                    );
                }
                if cx.selected.is_none() && cx.script.is_none() {
                    ui.label(widgets::micro(
                        "Select an object or open a script to link it here.",
                        palette.text_muted,
                    ));
                }
            });
        });

        ui.add_space(theme::space::S);
        widgets::section_label(ui, &format!("Comments  {}", item.comments.len()));
        for comment in &item.comments {
            ui.horizontal(|ui| {
                avatar(ui, &comment.author);
                ui.label(
                    RichText::new(&comment.author)
                        .color(person_color(&comment.author))
                        .family(theme::medium_family()),
                );
                ui.label(widgets::micro(
                    &time::ago(comment.at, now),
                    palette.text_muted,
                ));
            });
            ui.add(egui::Label::new(&comment.text).wrap());
            ui.add_space(theme::space::XS);
        }
        ui.add_enabled_ui(!cx.read_only, |ui| {
            ui.add(
                egui::TextEdit::multiline(&mut self.draft.comment)
                    .hint_text("Write a comment")
                    .desired_rows(2)
                    .desired_width(f32::INFINITY),
            );
            if ui
                .add_enabled(
                    !self.draft.comment.trim().is_empty(),
                    widgets::button(
                        widgets::Role::Secondary,
                        Some(icons::CHAT_CIRCLE),
                        "Comment",
                        &palette,
                    ),
                )
                .clicked()
                && changed.comment(&self.draft.comment, me, now)
            {
                self.draft.comment.clear();
            }
        });

        ui.add_space(theme::space::S);
        widgets::section(icons::CLOCK_COUNTER_CLOCKWISE, "Activity")
            .id_salt("board activity")
            .default_open(false)
            .show(ui, |ui| {
                for event in item.events.iter().rev() {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(widgets::micro(
                            &time::ago(event.at, now),
                            palette.text_muted,
                        ));
                        ui.label(event.describe(item.kind));
                    });
                }
            });
        ui.add_space(theme::space::S);
        ui.horizontal(|ui| {
            ui.label(widgets::micro(
                &format!(
                    "Created by {} {} · changed by {} {}",
                    item.created_by,
                    time::ago(item.created_at, now),
                    item.updated_by,
                    time::ago(item.updated_at, now)
                ),
                palette.text_muted,
            ));
        });
        if !cx.read_only {
            ui.horizontal(|ui| {
                if self.draft.deleting {
                    ui.label("Delete it for everyone?");
                    if ui
                        .add(widgets::button(
                            widgets::Role::Danger,
                            Some(icons::TRASH),
                            "Delete",
                            &palette,
                        ))
                        .clicked()
                    {
                        actions.push(BoardAction::Delete(item.id.clone()));
                        self.draft.deleting = false;
                    }
                    if ui.button("Keep it").clicked() {
                        self.draft.deleting = false;
                    }
                } else if ui
                    .add(widgets::button(
                        widgets::Role::Ghost,
                        Some(icons::TRASH),
                        "Delete…",
                        &palette,
                    ))
                    .clicked()
                {
                    self.draft.deleting = true;
                }
            });
        }
        if changed != *item {
            actions.push(BoardAction::Save(Box::new(changed)));
        }
    }
}

/// Sets the bug field `field` picks to `text` (tidied), if it changed.
fn set_bug_text(
    item: &mut Item,
    who: &str,
    now: u64,
    text: &str,
    field: impl Fn(&mut axcenity_teamwork::BugInfo) -> &mut String,
) {
    let text = axcenity_teamwork::item::tidy(text, axcenity_teamwork::item::MAX_TEXT);
    let slot = field(item.bug_mut());
    if *slot != text {
        *slot = text;
        item.touch(who, now);
    }
}
