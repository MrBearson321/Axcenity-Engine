//! The Collaborate panel's **Project** section and its windows
//! (Collaborate 2.0, P6 of `docs/research/collaborate-persistent.md` §4.8):
//!
//! - **Project**: name, Owner, who hosts (or who hosted last, with
//!   **Rejoin**), the backup host the Owner names, and "Let others host from
//!   my computer".
//! - **Repository**: the sync chip and what it means, the branch, the last
//!   upload and **Sync now**, **History…**, **Resolve…** while files are in
//!   conflict, and **Manage repository…**.
//! - **Manage repository** window: GitHub sign-in (device code, the GitHub
//!   CLI's sign-in, or a pasted token), connect an existing repository,
//!   create a new private one, any other Git host by URL, choosing a side
//!   when the repository already had files, and disconnecting.
//! - **History** window: the project's checkpoints, newest first, and
//!   restoring one (outside a session, with everything saved).
//! - **Resolve conflicts** window: each file's two sides, a side-by-side
//!   compare for text, and a choice per file (`collab_conflicts.rs`).
//! - The title bar chip shows the sync state whenever it isn't Synced.
//!
//! Anything that may touch the network runs on a thread of its own
//! ([`EditorApp::repo_task`]); the frame only polls. The words are the
//! brief's: checkpoint, upload, history, changes from others.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant, SystemTime};

use axcenity_collab::MemberId;
use axcenity_collab::project::ProjectRecord;
use axcenity_project as project;
use axcenity_repo::checkpoint::{self, Kind};
use axcenity_repo::conflicts::{Choice, Conflict};
use axcenity_repo::github::{self, DeviceCode, User};
use axcenity_repo::manager::{Connected, SignIn};
use axcenity_repo::repository::{self, Linked};
use axcenity_repo::sync::State;
use axcenity_repo::{RepositoryManager, Secret};

use super::*;
use crate::theme::icons;

/// How often the project's record is read again for the panel.
const RECORD_EVERY: Duration = Duration::from_secs(5);
/// The most of a file the compare shows.
const COMPARE_MOST: usize = 256 * 1024;

/// How the project gets a repository in the Manage window.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ConnectMode {
    /// A GitHub repository that exists (`owner/name`).
    #[default]
    Existing,
    /// A new GitHub repository.
    New,
    /// Any other Git host, by URL.
    Url,
}

/// What a background repository task found.
pub(crate) enum Done {
    Account(Result<Option<User>, String>),
    Device(Result<DeviceCode, String>),
    SignIn(Result<SignIn, String>),
    Connected(Result<Connected, String>),
    Disconnected(Result<(), String>),
    /// "Upload my version" or "Use the repository's version" finished.
    Settled(Result<(), String>),
    History(Result<Vec<checkpoint::Entry>, String>),
    Restored(Result<String, String>),
}

/// The Project section's windows and what they're waiting for.
#[derive(Default)]
pub(crate) struct RepoUi {
    pub(crate) manage_open: bool,
    pub(crate) history_open: bool,
    pub(crate) conflicts_open: bool,
    /// Who's signed in to GitHub: `None` until asked.
    pub(crate) account: Option<Result<Option<User>, String>>,
    /// A device sign-in going on, and when to ask GitHub again.
    device: Option<(DeviceCode, Instant)>,
    pub(crate) token: String,
    pub(crate) mode: ConnectMode,
    pub(crate) reference: String,
    pub(crate) new_name: String,
    pub(crate) private: bool,
    pub(crate) url: String,
    pub(crate) branch: String,
    /// The repository already had files: these of the project's differ.
    pub(crate) adopted: Option<(Connected, Vec<String>)>,
    /// Asked to disconnect: waiting for the confirmation.
    confirm_disconnect: bool,
    pub(crate) problem: Option<String>,
    pub(crate) note: Option<String>,
    /// The task running, in words, and where its answer comes.
    busy: Option<(String, Receiver<Done>)>,
    pub(crate) history: Option<Vec<checkpoint::Entry>>,
    /// The checkpoint asked to restore, waiting for the confirmation.
    confirm_restore: Option<checkpoint::Entry>,
    pub(crate) conflicts: Option<Vec<Conflict>>,
    pub(crate) choices: BTreeMap<String, Choice>,
    /// The file compared side by side, and both texts.
    compare: Option<(String, String, String)>,
    /// This project's record on this computer, and when it was read.
    record: Option<(Option<ProjectRecord>, Instant)>,
}

impl RepoUi {
    #[cfg(test)]
    pub(crate) fn busy_for_test(&self) -> bool {
        self.busy.is_some()
    }
}

impl EditorApp {
    /// Runs `job` with the repository manager on a thread of its own; its
    /// answer is taken in by [`Self::repo_ui_frame`].
    pub(crate) fn repo_task(
        &mut self,
        what: &str,
        job: impl FnOnce(&RepositoryManager) -> Done + Send + 'static,
    ) {
        if self.repo_ui().busy.is_some() {
            return;
        }
        let manager = self.repo.manager();
        let (send, receive) = channel();
        std::thread::Builder::new()
            .name("collaborate-repository".into())
            .spawn(move || {
                let _ = send.send(job(&manager));
            })
            .ok();
        let ui = self.repo_ui();
        ui.problem = None;
        ui.busy = Some((what.to_owned(), receive));
    }

    pub(crate) fn repo_ui(&mut self) -> &mut RepoUi {
        &mut self.repo.ui
    }

    /// Each frame: takes in a finished task, keeps a device sign-in going,
    /// and draws the windows that are open.
    pub(super) fn repo_ui_frame(&mut self, ctx: &egui::Context) {
        let done = match &self.repo.ui.busy {
            Some((_, receive)) => match receive.try_recv() {
                Ok(done) => Some(done),
                Err(std::sync::mpsc::TryRecvError::Empty) => None,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    Some(Done::Settled(Err("the task stopped".into())))
                }
            },
            None => None,
        };
        if let Some(done) = done {
            self.repo.ui.busy = None;
            self.repo_task_done(done);
        }
        if let Some((code, at)) = &self.repo.ui.device
            && Instant::now() >= *at
            && self.repo.ui.busy.is_none()
            && let Some(client) = github::client_id()
        {
            let code = code.clone();
            self.repo_task("Waiting for GitHub", move |manager| {
                Done::SignIn(
                    manager
                        .poll_sign_in(&client, &code)
                        .map_err(|e| e.to_string()),
                )
            });
        }
        if self.repo.ui.busy.is_some() || self.repo.ui.device.is_some() {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
        if self.repo.ui.manage_open {
            self.manage_repository_window(ctx);
        }
        if self.repo.ui.history_open {
            self.repo_history_window(ctx);
        }
        if self.repo.ui.conflicts_open {
            self.repo_conflicts_window(ctx);
        }
    }

    fn repo_task_done(&mut self, done: Done) {
        match done {
            Done::Account(account) => self.repo.ui.account = Some(account),
            Done::Device(Ok(code)) => {
                let at = Instant::now() + code.interval;
                self.repo.ui.device = Some((code, at));
            }
            Done::Device(Err(why)) | Done::Disconnected(Err(why)) | Done::Settled(Err(why)) => {
                self.repo.ui.problem = Some(why)
            }
            Done::SignIn(Ok(SignIn::Waiting(slower))) => {
                if let Some((code, at)) = &mut self.repo.ui.device {
                    *at = Instant::now() + slower.unwrap_or(code.interval);
                }
            }
            Done::SignIn(Ok(SignIn::Done { user, .. })) => {
                self.repo.ui.device = None;
                self.repo.ui.token.clear();
                self.repo.ui.note = Some(format!("Signed in to GitHub as {}.", user.login));
                self.repo.ui.account = Some(Ok(Some(user)));
            }
            Done::SignIn(Ok(SignIn::Denied)) => {
                self.repo.ui.device = None;
                self.repo.ui.problem = Some("GitHub sign-in was turned down.".into());
            }
            Done::SignIn(Ok(SignIn::Expired)) => {
                self.repo.ui.device = None;
                self.repo.ui.problem =
                    Some("The sign-in code expired: start again to get a new one.".into());
            }
            Done::SignIn(Err(why)) => {
                self.repo.ui.device = None;
                self.repo.ui.problem = Some(why);
            }
            Done::Connected(Ok(connected)) => {
                let differences = match &connected.linked {
                    Linked::Adopted { differences } => {
                        differences.iter().map(|c| c.path.clone()).collect()
                    }
                    _ => Vec::new(),
                };
                if differences.is_empty() {
                    self.record_repository_link(&connected);
                } else {
                    // Nothing is uploaded until the person chooses.
                    self.repo.ui.adopted = Some((connected, differences));
                }
            }
            Done::Connected(Err(why)) => self.repo.ui.problem = Some(why),
            Done::Disconnected(Ok(())) => {
                self.write_repository_link(None);
                self.repo.ui.note = Some(
                    "Disconnected. The project's files and its history stay on this computer."
                        .into(),
                );
            }
            Done::Settled(Ok(())) => {
                if let Some((connected, _)) = self.repo.ui.adopted.take() {
                    self.record_repository_link(&connected);
                }
                // Files may have changed on disk.
                if let Some(folder) = self.project.as_ref().map(|p| p.folder.clone())
                    && !self.unsaved()
                {
                    self.open(&folder);
                }
            }
            Done::History(Ok(entries)) => self.repo.ui.history = Some(entries),
            Done::History(Err(why)) | Done::Restored(Err(why)) => self.repo.ui.problem = Some(why),
            Done::Restored(Ok(_)) => {
                self.repo.ui.history = None;
                self.repo.ui.note = Some("Restored. Uploading it…".into());
                if let Some(folder) = self.project.as_ref().map(|p| p.folder.clone()) {
                    self.open(&folder);
                }
                self.repo_checkpoint(Kind::Manual, None);
                self.load_repo_history();
            }
        }
    }

    /// Writes a new link into `collaborate.json` and starts syncing.
    fn record_repository_link(&mut self, connected: &Connected) {
        self.write_repository_link(Some(project::RepositoryLink {
            provider: connected.provider.clone(),
            url: connected.url.clone(),
            branch: connected.branch.clone(),
        }));
        self.repo.ui.note = Some(format!(
            "Connected to {} ({}). Checkpoints upload there from now on.",
            connected.url, connected.branch
        ));
    }

    fn write_repository_link(&mut self, link: Option<project::RepositoryLink>) {
        let Some(folder) = self.project.as_ref().map(|p| p.folder.clone()) else {
            return;
        };
        let mut file = project::load_collaborate_file(&folder).unwrap_or_default();
        file.repository = link;
        if let Err(error) = project::save_collaborate_file(&folder, &file) {
            self.repo.ui.problem = Some(format!("Couldn't write collaborate.json: {error}"));
            return;
        }
        // Read again on the next frame.
        self.repo.force_reload();
    }

    /// This project's record on this computer (read every few seconds).
    fn project_record(&mut self) -> Option<ProjectRecord> {
        let folder = self.project.as_ref().map(|p| p.folder.clone())?;
        let stale = self
            .repo
            .ui
            .record
            .as_ref()
            .is_none_or(|(_, at)| at.elapsed() >= RECORD_EVERY);
        if stale {
            let record = self
                .collaborate_projects(self.team.as_ref().and_then(|t| t.data.as_deref()))
                .into_iter()
                .find(|r| same_folder(&r.folder, &folder));
            self.repo.ui.record = Some((record, Instant::now()));
        }
        self.repo.ui.record.as_ref().and_then(|(r, _)| r.clone())
    }

    /// The title bar's sync chip: shown when the project's repository
    /// isn't Synced. Opens the Collaborate panel.
    pub(super) fn repo_chip(&mut self, ui: &mut egui::Ui) {
        let Some(state) = self.repo.state().cloned() else {
            return;
        };
        if state == State::Synced {
            return;
        }
        let palette = theme::palette(ui.ctx());
        let (icon, color) = sync_look(&state, &palette);
        let response = ui
            .add(
                egui::Label::new(widgets::micro(&format!("{icon} {}", state.word()), color))
                    .sense(egui::Sense::click()),
            )
            .on_hover_text(format!("Project repository: {}", state.explain()));
        if response.clicked() {
            self.layout.open(Pane::TeamCreate);
        }
    }

    /// The Collaborate panel's Project section (both in and out of a
    /// session).
    pub(super) fn project_section_ui(&mut self, ui: &mut egui::Ui) {
        if self.project.is_none() {
            return;
        }
        let palette = theme::palette(ui.ctx());
        let record = self.project_record();
        let folder = self
            .project
            .as_ref()
            .map(|p| p.folder.clone())
            .unwrap_or_default();
        let file = project::load_collaborate_file(&folder).unwrap_or_default();
        let name = file
            .name
            .clone()
            .or_else(|| record.as_ref().map(|r| r.name.clone()))
            .unwrap_or_else(|| {
                folder
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default()
            });
        let mut action = None;
        widgets::section(icons::FOLDER_SIMPLE, "Project")
            .id_salt("collaborate project")
            .show(ui, |ui| {
                ui.label(egui::RichText::new(&name).family(theme::medium_family()));
                let owner = self
                    .team
                    .as_ref()
                    .and_then(|t| t.client())
                    .and_then(|c| c.members_list())
                    .or_else(|| record.as_ref().and_then(|r| r.members.as_ref()))
                    .and_then(|list| list.owner())
                    .map(|m| m.name.clone());
                if let Some(owner) = owner {
                    weak(ui, &palette, &format!("Owner: {owner}"));
                }
                // Who hosts.
                let host = match &self.team {
                    Some(team) if team.hosting().is_some() => "Hosted by this editor".to_owned(),
                    Some(team) => format!("Hosted at {}", team.address()),
                    None => match record.as_ref().and_then(|r| r.last_host.clone()) {
                        Some(who) => format!("No one is hosting here now (last: {who})"),
                        None => "No one is hosting".to_owned(),
                    },
                };
                weak(ui, &palette, &host);
                if self.team.is_none()
                    && let Some(record) = record.as_ref().filter(|r| !r.hosts.is_empty())
                {
                    let rejoin = widgets::button(
                        widgets::Role::Secondary,
                        Some(icons::SIGN_IN),
                        "Rejoin",
                        &palette,
                    );
                    if ui
                        .add_enabled(!self.playing(), rejoin)
                        .on_hover_text(
                            "Joins the project's session where it was last hosted, without an \
                             invite: the host knows this computer's key.",
                        )
                        .clicked()
                    {
                        action = Some(SectionAction::Rejoin(Box::new(record.clone())));
                    }
                }
                if let Some(team_action) = self.hosting_options_ui(ui, &palette) {
                    action = Some(team_action);
                }
            });
        widgets::section(icons::GIT_BRANCH, "Repository")
            .id_salt("collaborate repository")
            .show(ui, |ui| {
                if let Some(a) = self.repository_line_ui(ui, &palette) {
                    action = Some(a);
                }
            });
        if let Some(action) = action {
            self.section_action(action);
        }
    }

    /// Backup host (the Owner, hosting) and "Let others host from my
    /// computer" (a member who may host).
    fn hosting_options_ui(
        &mut self,
        ui: &mut egui::Ui,
        palette: &theme::Palette,
    ) -> Option<SectionAction> {
        let team = self.team.as_ref()?;
        let mut action = None;
        let me = team.me();
        let role = team.client_role();
        let next = team
            .client()
            .and_then(|c| c.successors().first().cloned())
            .and_then(|card| card.member)
            .and_then(|m| team.roster().iter().find(|c| c.member == m))
            .map(|c| c.profile.name.clone());
        if let Some(hosting) = team.hosting() {
            weak(
                ui,
                palette,
                &match &next {
                    Some(name) => format!("If this editor leaves, {name} hosts next."),
                    None => "If this editor leaves, the session ends: nobody else can host \
                             right now."
                        .to_owned(),
                },
            );
            if role == Some(axcenity_collab::Role::Owner) {
                let backup = hosting.lock().host().backup();
                let candidates: Vec<(MemberId, String)> = team
                    .roster()
                    .iter()
                    .filter(|c| Some(c.member) != me && c.role.may_host())
                    .map(|c| (c.member, c.profile.name.clone()))
                    .collect();
                let shown = backup
                    .and_then(|b| candidates.iter().find(|(m, _)| *m == b))
                    .map_or("Usual order".to_owned(), |(_, n)| n.clone());
                let mut chosen = backup;
                ui.horizontal(|ui| {
                    ui.label("Backup host");
                    egui::ComboBox::from_id_salt("collaborate backup host")
                        .selected_text(shown)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut chosen, None, "Usual order");
                            for (member, name) in &candidates {
                                ui.selectable_value(&mut chosen, Some(*member), name);
                            }
                        })
                        .response
                        .on_hover_text(
                            "Who takes the session on first if this editor leaves or is lost. \
                             The usual order is whoever has been here longest.",
                        );
                });
                if chosen != backup {
                    action = Some(SectionAction::Backup(chosen));
                }
            }
        } else if role.is_some_and(|r| r.may_host()) {
            let mut offer = !self.team_panel.no_host_offer;
            if ui
                .checkbox(&mut offer, "Let others host from my computer")
                .on_hover_text(
                    "If the host leaves or is lost, the session can go on from this computer. \
                     Turn it off to never be picked.",
                )
                .changed()
            {
                action = Some(SectionAction::Offer(offer));
            }
            if let Some(next) = &next {
                weak(ui, palette, &format!("Next host if the host goes: {next}"));
            }
        }
        action
    }

    /// The sync chip, what it means, and the buttons.
    fn repository_line_ui(
        &mut self,
        ui: &mut egui::Ui,
        palette: &theme::Palette,
    ) -> Option<SectionAction> {
        let mut action = None;
        let Some(link) = self.repo.link.clone() else {
            weak(
                ui,
                palette,
                "No repository: the project lives on this computer (and in sessions). Connect \
                 one to keep checkpoints online and pick the project up anywhere.",
            );
            let connect = widgets::button(
                widgets::Role::Secondary,
                Some(icons::GIT_BRANCH),
                "Connect a repository…",
                palette,
            );
            if ui.add(connect).clicked() {
                action = Some(SectionAction::Manage);
            }
            return action;
        };
        let state = self.repo.state().cloned().unwrap_or(State::Pending);
        let (icon, color) = sync_look(&state, palette);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(format!("{icon} {}", state.word())).color(color));
            ui.label(egui::RichText::new(&link.branch).color(palette.text_weak))
                .on_hover_text(&link.url);
        });
        ui.add(
            egui::Label::new(egui::RichText::new(state.explain()).color(palette.text_weak)).wrap(),
        );
        if let Some(at) = self.repo.schedule.last_synced {
            weak(ui, palette, &format!("Last uploaded {}", ago(at)));
        }
        let here = self.repo_commits_here();
        if !here {
            weak(
                ui,
                palette,
                "In a session the host makes the checkpoints and uploads them.",
            );
        }
        ui.horizontal_wrapped(|ui| {
            let sync = widgets::button(
                widgets::Role::Secondary,
                Some(icons::CLOUD_ARROW_UP),
                "Sync now",
                palette,
            );
            if ui
                .add_enabled(here && !self.repo.schedule.is_running(), sync)
                .on_hover_text("Makes a checkpoint of what changed and uploads it now.")
                .clicked()
            {
                action = Some(SectionAction::SyncNow);
            }
            if ui
                .add(widgets::button(
                    widgets::Role::Secondary,
                    Some(icons::CLOCK_COUNTER_CLOCKWISE),
                    "History…",
                    palette,
                ))
                .clicked()
            {
                action = Some(SectionAction::History);
            }
            if matches!(state, State::Conflict(_)) {
                let resolve = widgets::button(
                    widgets::Role::Primary,
                    Some(icons::GIT_MERGE),
                    "Resolve…",
                    palette,
                );
                if ui.add_enabled(here, resolve).clicked() {
                    action = Some(SectionAction::Conflicts);
                }
            }
            if ui
                .add(widgets::button(
                    widgets::Role::Secondary,
                    Some(icons::GEAR_SIX),
                    "Manage…",
                    palette,
                ))
                .on_hover_text("Sign in, connect another repository, or disconnect.")
                .clicked()
            {
                action = Some(SectionAction::Manage);
            }
        });
        action
    }

    fn section_action(&mut self, action: SectionAction) {
        match action {
            SectionAction::Rejoin(record) => {
                if let Err(error) = self.rejoin_project(&record, None) {
                    self.team_panel.problem = Some(error);
                }
            }
            SectionAction::Backup(member) => {
                if let Some(hosting) = self.team.as_ref().and_then(|t| t.hosting()) {
                    hosting.lock().set_backup(member);
                }
            }
            SectionAction::Offer(offer) => self.set_host_offer(offer),
            SectionAction::SyncNow => self.repo_checkpoint(Kind::Manual, None),
            SectionAction::History => {
                self.repo.ui.history_open = true;
                self.load_repo_history();
            }
            SectionAction::Conflicts => self.open_conflicts_window(),
            SectionAction::Manage => self.open_manage_repository(),
        }
    }

    /// "Let others host from my computer": kept between runs; in a session
    /// the host hears at once.
    pub(crate) fn set_host_offer(&mut self, offer: bool) {
        // Kept between runs with the panel's other settings.
        self.team_panel.no_host_offer = !offer;
        let Some(team) = self.team.as_mut() else {
            return;
        };
        if offer {
            team.lets_others_host = true;
            team.card_sent = false;
        } else if let Some(client) = team.client_mut() {
            client.withdraw_card();
        }
    }

    pub(crate) fn load_repo_history(&mut self) {
        let Some(folder) = self.project.as_ref().map(|p| p.folder.clone()) else {
            return;
        };
        self.repo_task("Reading the history", move |manager| {
            Done::History(
                checkpoint::history(&manager.git, &folder, 100).map_err(|e| e.to_string()),
            )
        });
    }

    pub(crate) fn open_manage_repository(&mut self) {
        let ui = &mut self.repo.ui;
        ui.manage_open = true;
        ui.problem = None;
        ui.note = None;
        if ui.new_name.is_empty() {
            ui.new_name = self
                .project
                .as_ref()
                .and_then(|p| p.folder.file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            ui.private = true;
        }
        if self.repo.ui.account.is_none() {
            self.repo_task("Checking your GitHub sign-in", |manager| {
                Done::Account(manager.signed_in().map_err(|e| e.to_string()))
            });
        }
    }

    fn manage_repository_window(&mut self, ctx: &egui::Context) {
        let mut open = self.repo.ui.manage_open;
        let mut pick = None;
        widgets::tool_window(icons::GIT_BRANCH, "Project Repository")
            .fit(440.0)
            .show(ctx, &mut open, |ui| pick = self.manage_repository_ui(ui));
        self.repo.ui.manage_open = open;
        if let Some(pick) = pick {
            self.manage_pick(pick);
        }
    }

    fn manage_repository_ui(&mut self, ui: &mut egui::Ui) -> Option<ManagePick> {
        let palette = theme::palette(ui.ctx());
        let mut pick = None;
        let busy = self.repo.ui.busy.as_ref().map(|(what, _)| what.clone());
        let folder = self.project.as_ref().map(|p| p.folder.clone());
        // GitHub account.
        widgets::section_label(ui, "GitHub");
        match &self.repo.ui.account {
            None => weak(ui, &palette, "Checking…"),
            Some(Err(why)) => weak(ui, &palette, why),
            Some(Ok(Some(user))) => {
                ui.horizontal(|ui| {
                    ui.label(format!(
                        "{} Signed in as {}",
                        icons::GITHUB_LOGO,
                        user.login
                    ));
                    if ui.small_button("Sign out").clicked() {
                        pick = Some(ManagePick::SignOut);
                    }
                });
            }
            Some(Ok(None)) => {
                if let Some((code, _)) = &self.repo.ui.device {
                    weak(ui, &palette, "On github.com/login/device, type this code:");
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(&code.user_code)
                                .size(22.0)
                                .family(theme::medium_family()),
                        )
                        .selectable(true),
                    );
                    ui.hyperlink_to("Open github.com/login/device", &code.verification_uri);
                    weak(ui, &palette, "Axcenity is waiting for GitHub…");
                } else {
                    weak(
                        ui,
                        &palette,
                        "Not signed in. GitHub is needed only to connect or create a GitHub \
                         repository; the token goes to Git's own credential store, never into \
                         the project.",
                    );
                    ui.horizontal_wrapped(|ui| {
                        if github::client_id().is_some()
                            && ui
                                .add(widgets::button(
                                    widgets::Role::Primary,
                                    Some(icons::GITHUB_LOGO),
                                    "Sign in with GitHub",
                                    &palette,
                                ))
                                .clicked()
                        {
                            pick = Some(ManagePick::SignIn);
                        }
                        if ui
                            .button("Use the GitHub CLI's sign-in")
                            .on_hover_text(
                                "Takes the token `gh auth login` keeps, if gh is installed.",
                            )
                            .clicked()
                        {
                            pick = Some(ManagePick::GhToken);
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.repo.ui.token)
                                .password(true)
                                .hint_text("or paste a token")
                                .desired_width(220.0),
                        );
                        if ui
                            .add_enabled(
                                !self.repo.ui.token.trim().is_empty(),
                                egui::Button::new("Use"),
                            )
                            .clicked()
                        {
                            pick = Some(ManagePick::PastedToken);
                        }
                    });
                }
            }
        }
        ui.add_space(theme::space::M);
        // The project's link.
        widgets::section_label(ui, "This project");
        if let Some((connected, differences)) = &self.repo.ui.adopted {
            weak(
                ui,
                &palette,
                &format!(
                    "{} already has files, and {} of this project's differ from them. Nothing \
                     is uploaded until you choose:",
                    connected.url,
                    differences.len()
                ),
            );
            for path in differences.iter().take(8) {
                weak(ui, &palette, &format!("  {path}"));
            }
            if differences.len() > 8 {
                weak(
                    ui,
                    &palette,
                    &format!("  and {} more", differences.len() - 8),
                );
            }
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add(widgets::button(
                        widgets::Role::Primary,
                        Some(icons::CLOUD_ARROW_UP),
                        "Upload my version",
                        &palette,
                    ))
                    .on_hover_text("A checkpoint of this project on top of the repository's.")
                    .clicked()
                {
                    pick = Some(ManagePick::UploadMine);
                }
                if ui
                    .add(widgets::button(
                        widgets::Role::Secondary,
                        Some(icons::CLOUD_ARROW_DOWN),
                        "Use the repository's version",
                        &palette,
                    ))
                    .on_hover_text(
                        "This project's files become the repository's. Yours are kept in Git's \
                         stash first, so nothing is lost.",
                    )
                    .clicked()
                {
                    pick = Some(ManagePick::TakeTheirs);
                }
            });
        } else if let Some(link) = self.repo.link.clone() {
            ui.label(format!("{} ({})", link.url, link.branch));
            if self.repo.ui.confirm_disconnect {
                weak(
                    ui,
                    &palette,
                    "Disconnect? Checkpoints stop uploading. The files, the history on this \
                     computer and the repository online all stay as they are.",
                );
                ui.horizontal(|ui| {
                    if ui
                        .add(widgets::button(
                            widgets::Role::Danger,
                            None,
                            "Disconnect",
                            &palette,
                        ))
                        .clicked()
                    {
                        pick = Some(ManagePick::Disconnect);
                    }
                    if ui.button("Cancel").clicked() {
                        self.repo.ui.confirm_disconnect = false;
                    }
                });
            } else if ui
                .add_enabled(self.repo_commits_here(), egui::Button::new("Disconnect…"))
                .clicked()
            {
                self.repo.ui.confirm_disconnect = true;
            }
        } else {
            let signed_in = matches!(self.repo.ui.account, Some(Ok(Some(_))));
            let mode = &mut self.repo.ui.mode;
            ui.horizontal_wrapped(|ui| {
                ui.selectable_value(mode, ConnectMode::Existing, "Existing on GitHub");
                ui.selectable_value(mode, ConnectMode::New, "New on GitHub");
                ui.selectable_value(mode, ConnectMode::Url, "Other Git host");
            });
            let ready = folder.is_some() && busy.is_none() && self.repo_commits_here();
            match self.repo.ui.mode {
                ConnectMode::Existing => {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.repo.ui.reference)
                            .hint_text("owner/name or its github.com address")
                            .desired_width(f32::INFINITY),
                    );
                    let go = !self.repo.ui.reference.trim().is_empty() && signed_in && ready;
                    if ui
                        .add_enabled(
                            go,
                            widgets::button(widgets::Role::Primary, None, "Connect", &palette),
                        )
                        .on_disabled_hover_text("Sign in to GitHub and name the repository.")
                        .clicked()
                    {
                        pick = Some(ManagePick::ConnectExisting);
                    }
                }
                ConnectMode::New => {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.repo.ui.new_name)
                            .hint_text("Repository name")
                            .desired_width(f32::INFINITY),
                    );
                    ui.checkbox(
                        &mut self.repo.ui.private,
                        "Private (only people you add see it)",
                    );
                    let go = !self.repo.ui.new_name.trim().is_empty() && signed_in && ready;
                    if ui
                        .add_enabled(
                            go,
                            widgets::button(
                                widgets::Role::Primary,
                                None,
                                "Create and connect",
                                &palette,
                            ),
                        )
                        .on_disabled_hover_text("Sign in to GitHub and name the repository.")
                        .clicked()
                    {
                        pick = Some(ManagePick::Create);
                    }
                }
                ConnectMode::Url => {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.repo.ui.url)
                            .hint_text("https://… or git@…")
                            .desired_width(f32::INFINITY),
                    );
                    ui.add(
                        egui::TextEdit::singleline(&mut self.repo.ui.branch)
                            .hint_text("Branch (empty: the repository's default)")
                            .desired_width(f32::INFINITY),
                    );
                    weak(
                        ui,
                        &palette,
                        "Sign-in for other hosts comes from your own Git setup.",
                    );
                    let go = !self.repo.ui.url.trim().is_empty() && ready;
                    if ui
                        .add_enabled(
                            go,
                            widgets::button(widgets::Role::Primary, None, "Connect", &palette),
                        )
                        .clicked()
                    {
                        pick = Some(ManagePick::ConnectUrl);
                    }
                }
            }
        }
        if let Some(what) = busy {
            ui.horizontal(|ui| {
                ui.spinner();
                weak(ui, &palette, &format!("{what}…"));
            });
        }
        if let Some(note) = &self.repo.ui.note {
            ui.add(egui::Label::new(egui::RichText::new(note).color(palette.success)).wrap());
        }
        if let Some(problem) = &self.repo.ui.problem {
            ui.add(egui::Label::new(egui::RichText::new(problem).color(palette.error)).wrap());
        }
        pick
    }

    fn manage_pick(&mut self, pick: ManagePick) {
        let Some(folder) = self.project.as_ref().map(|p| p.folder.clone()) else {
            return;
        };
        let author = self.repo_author(&folder);
        self.repo.ui.note = None;
        match pick {
            ManagePick::SignIn => {
                let Some(client) = github::client_id() else {
                    return;
                };
                self.repo_task("Asking GitHub for a code", move |manager| {
                    Done::Device(manager.start_sign_in(&client).map_err(|e| e.to_string()))
                });
            }
            ManagePick::GhToken => self.repo_task("Checking the GitHub CLI's sign-in", |manager| {
                match axcenity_repo::credentials::github_cli_token() {
                    Some(token) => {
                        Done::SignIn(manager.keep_token(token).map_err(|e| e.to_string()))
                    }
                    None => Done::SignIn(Err(
                        "The GitHub CLI isn't signed in here (run `gh auth login`), or isn't \
                         installed."
                            .into(),
                    )),
                }
            }),
            ManagePick::PastedToken => {
                let token = Secret::new(self.repo.ui.token.trim());
                self.repo.ui.token.clear();
                self.repo_task("Checking the token", move |manager| {
                    Done::SignIn(manager.keep_token(token).map_err(|e| e.to_string()))
                });
            }
            ManagePick::SignOut => {
                self.repo.manager().sign_out();
                self.repo.ui.account = Some(Ok(None));
            }
            ManagePick::ConnectExisting => {
                let reference = self.repo.ui.reference.trim().to_owned();
                self.repo_task("Connecting", move |manager| {
                    Done::Connected(
                        manager
                            .connect_github(&folder, &reference, &author, false)
                            .map_err(|e| e.to_string()),
                    )
                });
            }
            ManagePick::Create => {
                let name = self.repo.ui.new_name.trim().to_owned();
                let private = self.repo.ui.private;
                self.repo_task("Creating the repository", move |manager| {
                    Done::Connected(
                        manager
                            .create_github(&folder, &name, private, &author)
                            .map_err(|e| e.to_string()),
                    )
                });
            }
            ManagePick::ConnectUrl => {
                let url = self.repo.ui.url.trim().to_owned();
                let branch = Some(self.repo.ui.branch.trim().to_owned()).filter(|b| !b.is_empty());
                self.repo_task("Connecting", move |manager| {
                    Done::Connected(
                        manager
                            .connect_url(&folder, &url, branch.as_deref(), &author, false)
                            .map_err(|e| e.to_string()),
                    )
                });
            }
            ManagePick::UploadMine | ManagePick::TakeTheirs => {
                let Some((connected, _)) = &self.repo.ui.adopted else {
                    return;
                };
                let branch = connected.branch.clone();
                let mine = matches!(pick, ManagePick::UploadMine);
                self.repo_task("Settling the two versions", move |manager| {
                    let git = manager.git_with_token();
                    let result = if mine {
                        repository::upload_my_version(&git, &folder, &branch, &author).map(|_| ())
                    } else {
                        repository::take_remote_version(&git, &folder, &branch)
                    };
                    Done::Settled(result.map_err(|e| e.to_string()))
                });
            }
            ManagePick::Disconnect => {
                self.repo.ui.confirm_disconnect = false;
                self.repo_task("Disconnecting", move |manager| {
                    Done::Disconnected(manager.disconnect(&folder).map_err(|e| e.to_string()))
                });
            }
        }
    }

    fn repo_history_window(&mut self, ctx: &egui::Context) {
        let mut open = self.repo.ui.history_open;
        let mut restore = None;
        widgets::tool_window(icons::CLOCK_COUNTER_CLOCKWISE, "Project History")
            .size(egui::vec2(460.0, 420.0))
            .show(ctx, &mut open, |ui| restore = self.repo_history_ui(ui));
        self.repo.ui.history_open = open;
        if let Some(entry) = restore {
            self.restore_checkpoint(entry);
        }
    }

    fn repo_history_ui(&mut self, ui: &mut egui::Ui) -> Option<checkpoint::Entry> {
        let palette = theme::palette(ui.ctx());
        let mut restore = None;
        let may_restore = self.team.is_none() && !self.unsaved() && self.repo.link.is_some();
        weak(
            ui,
            &palette,
            if self.team.is_some() {
                "In a session, use the session's own History (in this panel) to go back."
            } else if self.unsaved() {
                "Save first to restore a checkpoint."
            } else {
                "Restoring puts the project back as it was and keeps that as a new checkpoint, \
                 so a restore can be undone the same way."
            },
        );
        if let Some(entry) = self.repo.ui.confirm_restore.clone() {
            ui.horizontal_wrapped(|ui| {
                ui.label(format!("Restore “{}”?", entry.title()));
                if ui
                    .add(widgets::button(
                        widgets::Role::Primary,
                        None,
                        "Restore",
                        &palette,
                    ))
                    .clicked()
                {
                    restore = Some(entry.clone());
                }
                if ui.button("Cancel").clicked() {
                    self.repo.ui.confirm_restore = None;
                }
            });
        }
        let Some(entries) = self.repo.ui.history.clone() else {
            ui.spinner();
            return restore;
        };
        if entries.is_empty() {
            weak(ui, &palette, "No checkpoints yet.");
        }
        egui::ScrollArea::vertical().show(ui, |ui| {
            for entry in &entries {
                ui.horizontal(|ui| {
                    let icon = if entry.kind.is_some() {
                        icons::GIT_COMMIT
                    } else {
                        icons::USER
                    };
                    ui.vertical(|ui| {
                        ui.label(format!("{icon} {}", entry.title()));
                        let when = SystemTime::UNIX_EPOCH
                            + Duration::from_secs(entry.commit.time);
                        weak(
                            ui,
                            &palette,
                            &format!("{} · {}", entry.commit.author, ago(when)),
                        );
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add_enabled(may_restore, egui::Button::new("Restore…"))
                            .clicked()
                        {
                            self.repo.ui.confirm_restore = Some(entry.clone());
                        }
                    });
                });
                ui.separator();
            }
        });
        if let Some(problem) = &self.repo.ui.problem {
            ui.add(egui::Label::new(egui::RichText::new(problem).color(palette.error)).wrap());
        }
        restore
    }

    fn restore_checkpoint(&mut self, entry: checkpoint::Entry) {
        self.repo.ui.confirm_restore = None;
        let Some(folder) = self.project.as_ref().map(|p| p.folder.clone()) else {
            return;
        };
        if self.team.is_some() || self.unsaved() || self.repo.schedule.is_running() {
            self.repo.ui.problem = Some(
                "Restore outside a session, with everything saved and no sync running.".into(),
            );
            return;
        }
        let author = self.repo_author(&folder);
        let commit = entry.commit.sha.clone();
        self.repo_task("Restoring", move |manager| {
            Done::Restored(
                checkpoint::restore(&manager.git, &folder, &commit, &author)
                    .map_err(|e| e.to_string()),
            )
        });
    }

    pub(crate) fn open_conflicts_window(&mut self) {
        self.repo.ui.conflicts_open = true;
        self.repo.ui.compare = None;
        match self.repo_conflicts() {
            Ok(found) => {
                // Choices made before for files still in conflict stay.
                self.repo
                    .ui
                    .choices
                    .retain(|path, _| found.iter().any(|c| c.path == *path));
                self.repo.ui.conflicts = Some(found);
                self.repo.ui.problem = None;
            }
            Err(why) => self.repo.ui.problem = Some(why),
        }
    }

    fn repo_conflicts_window(&mut self, ctx: &egui::Context) {
        let mut open = self.repo.ui.conflicts_open;
        let mut resolve = false;
        widgets::tool_window(icons::GIT_MERGE, "Resolve Conflicts")
            .size(egui::vec2(620.0, 480.0))
            .show(ctx, &mut open, |ui| resolve = self.repo_conflicts_ui(ui));
        self.repo.ui.conflicts_open = open;
        if resolve {
            self.resolve_chosen();
        }
    }

    fn repo_conflicts_ui(&mut self, ui: &mut egui::Ui) -> bool {
        let palette = theme::palette(ui.ctx());
        let mut resolve = false;
        let Some(found) = self.repo.ui.conflicts.clone() else {
            if let Some(problem) = &self.repo.ui.problem {
                ui.add(egui::Label::new(egui::RichText::new(problem).color(palette.error)).wrap());
            }
            return false;
        };
        if found.is_empty() {
            weak(
                ui,
                &palette,
                "No conflicts: the project and its repository agree.",
            );
            return false;
        }
        weak(
            ui,
            &palette,
            "These files changed both here and in the repository. Nothing was overwritten: the \
             repository's version is beside each one. Choose what to keep of each; yours stays \
             in the history whatever you choose.",
        );
        let folder = self
            .project
            .as_ref()
            .map(|p| p.folder.clone())
            .unwrap_or_default();
        egui::ScrollArea::vertical()
            .max_height(220.0)
            .id_salt("conflict files")
            .show(ui, |ui| {
                for conflict in &found {
                    ui.label(egui::RichText::new(&conflict.path).family(theme::medium_family()));
                    weak(
                        ui,
                        &palette,
                        &format!(
                            "Here: {} · In the repository: {}",
                            side_words(&conflict.mine),
                            side_words(&conflict.theirs)
                        ),
                    );
                    let mut choice = self.repo.ui.choices.get(&conflict.path).copied();
                    ui.horizontal_wrapped(|ui| {
                        ui.selectable_value(&mut choice, Some(Choice::Mine), "Keep mine");
                        ui.selectable_value(&mut choice, Some(Choice::Theirs), "Keep theirs");
                        ui.selectable_value(&mut choice, Some(Choice::Both), "Keep both")
                            .on_hover_text(
                                "Mine stays at this path; theirs is kept beside it as the copy.",
                            );
                        if conflict.text
                            && let Some(copy) = &conflict.copy
                            && ui.small_button("Compare").clicked()
                        {
                            let mine = read_some(&folder.join(&conflict.path));
                            let theirs = read_some(&folder.join(copy));
                            self.repo.ui.compare = Some((conflict.path.clone(), mine, theirs));
                        }
                    });
                    if let Some(choice) = choice {
                        self.repo.ui.choices.insert(conflict.path.clone(), choice);
                    }
                    ui.separator();
                }
            });
        if let Some((path, mine, theirs)) = &self.repo.ui.compare {
            ui.label(
                egui::RichText::new(format!("{path}: here | in the repository"))
                    .color(palette.text_weak),
            );
            egui::ScrollArea::both()
                .max_height(180.0)
                .id_salt("conflict compare")
                .show(ui, |ui| {
                    ui.columns(2, |columns| {
                        columns[0].label(egui::RichText::new(mine).monospace());
                        columns[1].label(egui::RichText::new(theirs).monospace());
                    });
                });
        }
        let chosen = found
            .iter()
            .filter(|c| self.repo.ui.choices.contains_key(&c.path))
            .count();
        ui.horizontal(|ui| {
            let button = widgets::button(
                widgets::Role::Primary,
                Some(icons::GIT_MERGE),
                "Resolve and upload",
                &palette,
            );
            if ui
                .add_enabled(
                    chosen == found.len() && !self.repo.schedule.is_running(),
                    button,
                )
                .on_disabled_hover_text("Choose for every file first.")
                .clicked()
            {
                resolve = true;
            }
            weak(ui, &palette, &format!("{chosen} of {} chosen", found.len()));
        });
        if let Some(problem) = &self.repo.ui.problem {
            ui.add(egui::Label::new(egui::RichText::new(problem).color(palette.error)).wrap());
        }
        resolve
    }

    /// Makes the merge with the window's choices.
    pub(crate) fn resolve_chosen(&mut self) {
        let choices: Vec<(String, Choice)> = self
            .repo
            .ui
            .choices
            .iter()
            .map(|(p, c)| (p.clone(), *c))
            .collect();
        let mut result = Err("nothing chosen".to_owned());
        for (path, choice) in choices {
            result = self.choose_resolution(choice, Some(&path));
            if result.is_err() {
                break;
            }
        }
        match result {
            Ok(message) => {
                self.output.info(format!("Collaborate: {message}"));
                self.repo.ui.choices.clear();
                self.repo.ui.conflicts = None;
                self.repo.ui.compare = None;
                self.repo.ui.conflicts_open = false;
                // What the repository brought in is read again.
                if let Some(folder) = self.project.as_ref().map(|p| p.folder.clone())
                    && !self.unsaved()
                {
                    self.open(&folder);
                }
            }
            Err(why) => self.repo.ui.problem = Some(why),
        }
    }
}

/// What the Project section asked for.
enum SectionAction {
    Rejoin(Box<ProjectRecord>),
    Backup(Option<MemberId>),
    Offer(bool),
    SyncNow,
    History,
    Conflicts,
    Manage,
}

/// What the Manage window asked for.
enum ManagePick {
    SignIn,
    GhToken,
    PastedToken,
    SignOut,
    ConnectExisting,
    Create,
    ConnectUrl,
    UploadMine,
    TakeTheirs,
    Disconnect,
}

/// The chip's icon and colour for `state`.
pub(crate) fn sync_look(state: &State, palette: &theme::Palette) -> (&'static str, egui::Color32) {
    match state {
        State::Synced => (icons::CLOUD_CHECK, palette.success),
        State::Syncing => (icons::ARROWS_CLOCKWISE, palette.accent),
        State::Pending => (icons::CLOUD_ARROW_UP, palette.text_weak),
        State::Offline => (icons::CLOUD_SLASH, palette.warning),
        State::Conflict(_) => (icons::GIT_MERGE, palette.warning),
        State::Error(_) => (icons::CLOUD_WARNING, palette.error),
    }
}

fn weak(ui: &mut egui::Ui, palette: &theme::Palette, text: &str) {
    ui.add(egui::Label::new(egui::RichText::new(text).color(palette.text_weak)).wrap());
}

/// "just now", "5 minutes ago", "3 hours ago", "2 days ago".
fn ago(at: SystemTime) -> String {
    let secs = SystemTime::now()
        .duration_since(at)
        .unwrap_or_default()
        .as_secs();
    match secs {
        0..60 => "just now".to_owned(),
        60..120 => "a minute ago".to_owned(),
        120..3600 => format!("{} minutes ago", secs / 60),
        3600..7200 => "an hour ago".to_owned(),
        7200..86400 => format!("{} hours ago", secs / 3600),
        86400..172800 => "yesterday".to_owned(),
        _ => format!("{} days ago", secs / 86400),
    }
}

fn side_words(version: &axcenity_repo::conflicts::Version) -> String {
    let who = version.author.as_deref().unwrap_or("someone");
    if version.deleted {
        return format!("deleted by {who}");
    }
    let when = version
        .time
        .map(|t| ago(SystemTime::UNIX_EPOCH + Duration::from_secs(t.max(0) as u64)))
        .unwrap_or_default();
    match version.size {
        Some(size) => format!("{who}, {when}, {size} bytes"),
        None => format!("{who}, {when}"),
    }
}

fn read_some(path: &std::path::Path) -> String {
    let bytes = std::fs::read(path).unwrap_or_default();
    let shown = &bytes[..bytes.len().min(COMPARE_MOST)];
    let mut text = String::from_utf8_lossy(shown).into_owned();
    if bytes.len() > COMPARE_MOST {
        text.push_str("\n…");
    }
    text
}

fn same_folder(a: &std::path::Path, b: &std::path::Path) -> bool {
    let canon = |p: &std::path::Path| std::fs::canonicalize(p).unwrap_or_else(|_| PathBuf::from(p));
    canon(a) == canon(b)
}
