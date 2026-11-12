//! The start screen's Team Projects page: where working with others
//! starts. Join a session, share one of this computer's projects (or a new
//! one) by hosting it, open or rejoin the team projects this computer
//! already hosted or joined (its project records), and reach Collaborate's
//! setup, settings and "How Collaborate works". The setup wizard and the
//! Start Session dialog behind it are the editor's own (`app/collab_setup.rs`).

use axcenity_collab::project::ProjectRecord;

use super::*;

/// How many of this computer's projects "Share a project" lists.
const SHARED_CHOICES: usize = 6;
/// The page explaining what Collaborate does, under `documentation/`.
pub(super) const HOW_IT_WORKS: &str = "learn/how-collaborate-works.md";

/// The Team Projects page's own state.
#[derive(Default)]
pub(super) struct TeamPage {
    /// This computer's team projects, most recent first; `None` until the
    /// app reads them (each time the page is shown).
    pub(super) records: Option<Vec<ProjectRecord>>,
    /// New Team Project… was pressed: the project the New Project dialog
    /// makes is hosted once it opens.
    pub(super) host_new: bool,
}

impl StartScreen {
    /// Whether the Team Projects page waits for this computer's team
    /// projects (see [`StartScreen::set_team_projects`]).
    pub fn wants_team_projects(&self) -> bool {
        self.page == Page::Team && self.team.records.is_none()
    }

    /// This computer's team projects, for the Team Projects page.
    pub fn set_team_projects(&mut self, records: Vec<ProjectRecord>) {
        self.team.records = Some(records);
    }

    /// Forgets the team projects read, so they're read again next time
    /// the page is shown.
    pub(super) fn forget_team_projects(&mut self) {
        self.team.records = None;
    }

    /// Shows the Team Projects page.
    #[cfg(test)]
    pub fn show_team_page(&mut self) {
        self.page = Page::Team;
    }

    /// The Team Projects page. Returns what was picked.
    pub(super) fn team_page(
        &mut self,
        ui: &mut egui::Ui,
        projects: &[ProjectLink],
    ) -> Option<StartAction> {
        let palette = theme::palette(ui.ctx());
        let mut action = None;
        ui.label(
            RichText::new("Team Projects")
                .family(theme::display_family())
                .size(type_scale::DISPLAY)
                .extra_letter_spacing(theme::DISPLAY_TRACKING)
                .color(palette.text_strong),
        );
        ui.add(
            egui::Label::new(
                RichText::new(
                    "Make a game together. One computer hosts the project's session and the \
                     others join it with an invite link; everyone edits the same world live.",
                )
                .color(palette.text_secondary),
            )
            .wrap(),
        );
        ui.add_space(14.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
            let buttons = [
                (
                    Role::Primary,
                    icons::SIGN_IN,
                    "Join Session…",
                    "Paste an invite link someone sent you",
                ),
                (
                    Role::Secondary,
                    icons::PLUS,
                    "New Team Project…",
                    "Make a new project, then host a session for it",
                ),
                (
                    Role::Secondary,
                    icons::WRENCH,
                    "Set Up Collaborate…",
                    "The setup wizard: your name, Tailscale for working over the internet, \
                     and a check that others can reach this computer",
                ),
                (
                    Role::Secondary,
                    icons::GEAR_SIX,
                    "Collaborate Settings",
                    "Who gets in, where this computer listens, and the other settings",
                ),
                (
                    Role::Secondary,
                    icons::INFO,
                    "How Collaborate works",
                    "What's shared, where files live, who can get in and what goes over the \
                     internet",
                ),
            ];
            for (index, (role, icon, label, tip)) in buttons.into_iter().enumerate() {
                let button =
                    widgets::button(role, Some(icon), label, &palette).min_size(vec2(0.0, 32.0));
                if ui.add(button).on_hover_text(tip).clicked() {
                    match index {
                        0 => self.join_session(),
                        1 => {
                            self.new_project(Template::Baseplate);
                            self.team.host_new = true;
                        }
                        2 => action = Some(StartAction::SetUpCollaborate),
                        3 => action = Some(StartAction::CollaborateSettings),
                        _ => self.how_collaborate_works(),
                    }
                }
            }
        });

        ui.add_space(28.0);
        widgets::section_label(ui, "On this computer");
        ui.add_space(8.0);
        let records = self.team.records.clone().unwrap_or_default();
        if records.is_empty() {
            empty_state(
                ui,
                icons::USERS_THREE,
                "No team projects yet",
                "Projects this computer hosts or joins are listed here, to open or rejoin.",
            );
        }
        let now = SystemTime::now();
        for record in &records {
            if let Some(picked) = record_row(ui, &palette, record, now) {
                action = Some(picked);
            }
            ui.add_space(8.0);
        }

        ui.add_space(24.0);
        widgets::section_label(ui, "Share a project");
        ui.add(
            egui::Label::new(
                RichText::new(
                    "Opens the project and starts a session for it, with an invite link to \
                     send. The project stays on this computer; others get a copy while they're \
                     in the session.",
                )
                .color(palette.text_weak),
            )
            .wrap(),
        );
        ui.add_space(8.0);
        let shareable: Vec<&ProjectLink> = projects
            .iter()
            .filter(|link| {
                !records
                    .iter()
                    .any(|record| same_folder(&record.folder, &link.folder))
            })
            .take(SHARED_CHOICES)
            .collect();
        if shareable.is_empty() {
            empty_state(
                ui,
                icons::CUBE,
                "Nothing to share yet",
                "Make a project on the Projects page, or press New Team Project.",
            );
        }
        for link in shareable {
            if share_row(ui, &palette, link) {
                action = Some(StartAction::Host(link.folder.clone()));
            }
            ui.add_space(8.0);
        }
        action
    }

    /// Shows "How Collaborate works" in Learn or Documentation.
    fn how_collaborate_works(&mut self) {
        if let Some(index) = docs::library().index_of(HOW_IT_WORKS) {
            self.follow_doc(Some(DocAction::Open(index, None)));
        }
    }

    /// What opening the folder the New Project dialog made asks for: to
    /// host it, after New Team Project….
    pub(super) fn opened_new(&mut self, folder: PathBuf) -> StartAction {
        if std::mem::take(&mut self.team.host_new) {
            StartAction::Host(folder)
        } else {
            StartAction::Open(folder)
        }
    }
}

/// One of this computer's team projects: its name, who hosted it last, its
/// state, and Open and Rejoin.
fn record_row(
    ui: &mut egui::Ui,
    palette: &Palette,
    record: &ProjectRecord,
    now: SystemTime,
) -> Option<StartAction> {
    let mut action = None;
    row_frame(palette).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(&record.name)
                        .family(theme::medium_family())
                        .color(palette.text_strong),
                );
                ui.label(RichText::new(record_line(record, now)).color(palette.text_weak));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                let rejoin =
                    widgets::button(Role::Primary, Some(icons::SIGN_IN), "Rejoin", palette);
                let can_rejoin = !record.hosts.is_empty();
                let tip = match record.hosts.first() {
                    Some(host) => format!("Join the project's session at {}", host.address),
                    None => "This computer doesn't know where the project is hosted".to_owned(),
                };
                if ui
                    .add_enabled(can_rejoin, rejoin)
                    .on_hover_text(&tip)
                    .on_disabled_hover_text(&tip)
                    .clicked()
                {
                    action = Some(StartAction::Rejoin(Box::new(record.clone())));
                }
                let open =
                    widgets::button(Role::Secondary, Some(icons::FOLDER_OPEN), "Open", palette);
                let here = record
                    .folder
                    .join(axcenity_project::MANIFEST_FILE)
                    .is_file();
                let tip = format!(
                    "Open this computer's copy, on its own ({})",
                    record.folder.display()
                );
                if ui
                    .add_enabled(here, open)
                    .on_hover_text(&tip)
                    .on_disabled_hover_text("This computer has no copy of the project yet")
                    .clicked()
                {
                    action = Some(StartAction::Open(record.folder.clone()));
                }
            });
        });
    });
    action
}

/// "Hosted by Ana · Offline · 3 days ago", under a team project's name.
pub(super) fn record_line(record: &ProjectRecord, now: SystemTime) -> String {
    let mut parts = Vec::new();
    if let Some(host) = &record.last_host {
        parts.push(format!("Hosted by {host}"));
    }
    parts.push(record.state.word().to_owned());
    if record.repository.is_some() {
        parts.push("Backed up to a repository".to_owned());
    }
    if record.updated > 0 {
        let then = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(record.updated);
        parts.push(ago(then, now));
    }
    parts.join(" · ")
}

/// One of this computer's projects, with Host Session. True when pressed.
fn share_row(ui: &mut egui::Ui, palette: &Palette, link: &ProjectLink) -> bool {
    let mut host = false;
    row_frame(palette).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(&link.name)
                        .family(theme::medium_family())
                        .color(palette.text_strong),
                );
                ui.label(RichText::new(link.folder.display().to_string()).color(palette.text_weak));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let button = widgets::button(
                    Role::Secondary,
                    Some(icons::SHARE_NETWORK),
                    "Host Session…",
                    palette,
                );
                host = ui
                    .add(button)
                    .on_hover_text("Open it and start a session others can join")
                    .clicked();
            });
        });
    });
    host
}

fn row_frame(palette: &Palette) -> egui::Frame {
    egui::Frame::new()
        .fill(palette.surface)
        .stroke(Stroke::new(1.0_f32, palette.border))
        .corner_radius(CARD_RADIUS)
        .inner_margin(egui::Margin::symmetric(14, 10))
}
