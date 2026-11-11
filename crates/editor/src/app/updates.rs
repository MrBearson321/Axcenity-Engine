//! Update checks (Settings > Updates, Help > Check for Updates…): which
//! channel to follow, how often to look, and what the last look found. The
//! check itself is `axcenity_release::updates::check` on a thread, with the
//! Toolbox's HTTP client: one request for GitHub's releases list, then the
//! newest release's signed manifest. Only an official build with the release
//! key looks at all; a local build says so and never touches the network.
//! When an update is found, a quiet chip by the Settings gear says so (never
//! during Play), and a short note shows once. Installing it is Phase 6; for
//! now the release page has the downloads.

use std::sync::mpsc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use axcenity_release::updates::{self, CheckInterval, Update, UpdateError, UpdatePrefs};
use axcenity_release::{BuildInfo, Channel, InstallKind};

use super::*;
use crate::theme::space;
use crate::widgets;

/// Where the update settings are saved, as JSON.
pub(super) const UPDATES_KEY: &str = "updates";

/// The most a check reads from one answer: the releases list is a few
/// hundred kilobytes at most, a manifest a few kilobytes.
const ANSWER_LIMIT: u64 = 4 * 1024 * 1024;

/// How long after start an automatic check waits, so it never competes
/// with opening the editor.
const STARTUP_DELAY: Duration = Duration::from_secs(5);

/// What the last check this session found.
#[derive(Clone, Debug)]
pub(super) enum Outcome {
    UpToDate,
    Available(Box<Update>),
    Failed(String),
}

/// The update settings and the check going on, if one is.
#[derive(Default)]
pub(super) struct Updates {
    pub prefs: UpdatePrefs,
    running: Option<mpsc::Receiver<Result<Option<Update>, UpdateError>>>,
    /// The first frame, so an automatic check waits until the editor has
    /// started ([`STARTUP_DELAY`]).
    first_frame: Option<Instant>,
    /// Whether the running check was asked for (Check now), so its result
    /// is said even when it's "up to date".
    by_hand: bool,
    pub outcome: Option<Outcome>,
}

impl Updates {
    pub fn load(text: &str) -> UpdatePrefs {
        serde_json::from_str(text).unwrap_or_default()
    }

    pub fn to_text(&self) -> String {
        serde_json::to_string(&self.prefs).expect("update settings always serialize")
    }

    pub fn checking(&self) -> bool {
        self.running.is_some()
    }

    /// The update to point at: found, and not dismissed.
    pub fn available(&self) -> Option<&Update> {
        match &self.outcome {
            Some(Outcome::Available(update)) if self.prefs.notifies(&update.candidate.version) => {
                Some(update)
            }
            _ => None,
        }
    }

    /// Starts a check on a thread with `fetch`, unless one is running.
    fn start(
        &mut self,
        by_hand: bool,
        installed: axcenity_release::Version,
        key: &'static str,
        mut fetch: impl FnMut(&str) -> Result<Vec<u8>, String> + Send + 'static,
    ) {
        if self.running.is_some() {
            return;
        }
        let follow = self.prefs.channel(&installed);
        let (send, receive) = mpsc::channel();
        std::thread::Builder::new()
            .name("update check".to_owned())
            .spawn(move || {
                let _ = send.send(updates::check(&mut fetch, &installed, follow, key));
            })
            .expect("a thread can be started");
        self.running = Some(receive);
        self.by_hand = by_hand;
    }

    /// Takes a finished check's result. Returns the outcome and whether the
    /// check was asked for, once.
    fn finish(&mut self, now: u64) -> Option<(Outcome, bool)> {
        let result = self.running.as_ref()?.try_recv();
        let outcome = match result {
            Err(mpsc::TryRecvError::Empty) => return None,
            Err(mpsc::TryRecvError::Disconnected) => {
                Outcome::Failed("the check stopped unexpectedly".to_owned())
            }
            Ok(Ok(None)) => Outcome::UpToDate,
            Ok(Ok(Some(update))) => Outcome::Available(Box::new(update)),
            Ok(Err(error)) => Outcome::Failed(error.to_string()),
        };
        self.running = None;
        // A failed check is tried again next time the editor starts, not
        // every frame: it still counts as a check.
        self.prefs.last_check = Some(now);
        self.outcome = Some(outcome.clone());
        Some((outcome, self.by_hand))
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

/// GETs `url` with the Toolbox's client (HTTPS only, size-limited).
fn fetch(client: &axcenity_toolbox::http::Client, url: &str) -> Result<Vec<u8>, String> {
    axcenity_toolbox::http::get_bytes(client, url, ANSWER_LIMIT).map_err(|error| error.to_string())
}

impl EditorApp {
    /// Each frame: takes a finished check's result, and starts an automatic
    /// check when one is due (not during Play, nor in the first seconds).
    pub(super) fn updates_frame(&mut self) {
        let now = unix_now();
        if let Some((outcome, by_hand)) = self.updates.finish(now) {
            match outcome {
                Outcome::Available(update) => {
                    let version = &update.candidate.version;
                    if (by_hand || self.updates.prefs.notifies(version)) && !self.playing() {
                        self.toasts
                            .success(format!("Axcenity {version} is available"));
                    }
                }
                Outcome::UpToDate if by_hand => self.toasts.success("Axcenity is up to date"),
                Outcome::Failed(error) if by_hand => {
                    self.output
                        .warning(format!("Couldn't check for updates: {error}"));
                }
                Outcome::Failed(error) => log::info!("update check failed: {error}"),
                Outcome::UpToDate => {}
            }
        }
        let build = BuildInfo::current();
        if updates::checks_disabled(&build).is_none()
            && !self.playing()
            && self
                .updates
                .first_frame
                .get_or_insert_with(Instant::now)
                .elapsed()
                >= STARTUP_DELAY
            && self.updates.prefs.due(now)
        {
            self.check_for_updates(false);
        }
    }

    /// Starts a check now, if this build checks at all.
    pub(super) fn check_for_updates(&mut self, by_hand: bool) {
        let build = BuildInfo::current();
        if updates::checks_disabled(&build).is_some() {
            return;
        }
        let Some(key) = updates::release_public_key() else {
            return;
        };
        let client = axcenity_toolbox::http::Client::new(Duration::from_secs(20));
        self.updates
            .start(by_hand, build.version, key, move |url| fetch(&client, url));
    }

    /// Help > Check for Updates…: Settings on the Updates page, checking.
    pub(super) fn open_updates(&mut self) {
        self.settings.open = true;
        self.settings.page = settings::SettingsPage::Updates;
        self.check_for_updates(true);
    }

    /// The menu bar's quiet "update available" chip, beside the Settings
    /// gear; it opens Settings > Updates. Not during Play.
    pub(super) fn update_chip(&mut self, ui: &mut egui::Ui) {
        if self.playing() {
            return;
        }
        let Some(update) = self.updates.available() else {
            return;
        };
        let version = update.candidate.version.to_string();
        let palette = theme::palette(ui.ctx());
        let text = egui::RichText::new(theme::with_icon(icons::ARROW_CIRCLE_UP, &version))
            .color(palette.accent_text)
            .size(theme::type_scale::SMALL);
        if ui
            .add(egui::Button::new(text).frame(false))
            .on_hover_text(format!(
                "Axcenity {version} is available. Settings › Updates says what's new."
            ))
            .clicked()
        {
            self.settings.open = true;
            self.settings.page = settings::SettingsPage::Updates;
        }
    }

    /// Settings > Updates.
    pub(super) fn updates_page(&mut self, ui: &mut egui::Ui) {
        settings::page_title(
            ui,
            "Updates",
            "Which releases this editor tells you about, and how often it looks.",
        );
        let palette = theme::palette(ui.ctx());
        let build = BuildInfo::current();
        let install = InstallKind::current();
        settings::section(ui, "This editor");
        ui.label(format!(
            "Axcenity {} · {}",
            build.version,
            build.version.channel()
        ));
        ui.label(
            egui::RichText::new(format!("Installed as: {}", install.label()))
                .color(palette.text_weak),
        );
        let disabled = updates::checks_disabled(&build);
        if let Some(reason) = disabled {
            ui.add(egui::Label::new(egui::RichText::new(reason).color(palette.text_weak)).wrap());
        }
        ui.add_space(space::L);

        settings::section(ui, "Update channel");
        let own = build.version.channel();
        let picked = self.updates.prefs.channel;
        let label = |channel: Option<Channel>| match channel {
            None => format!("This build's channel ({own})"),
            Some(channel) => channel.to_string(),
        };
        ui.add_enabled_ui(disabled.is_none(), |ui| {
            egui::ComboBox::from_id_salt("update channel")
                .selected_text(label(picked))
                .width(ui.available_width().min(260.0))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.updates.prefs.channel, None, label(None));
                    for channel in Channel::ALL {
                        ui.selectable_value(
                            &mut self.updates.prefs.channel,
                            Some(channel),
                            label(Some(channel)),
                        )
                        .on_hover_text(channel_about(channel));
                    }
                });
        });
        let follow = self.updates.prefs.channel(&build.version);
        ui.add(
            egui::Label::new(egui::RichText::new(channel_about(follow)).color(palette.text_weak))
                .wrap(),
        );
        if self.updates.prefs.channel != picked {
            // Another channel offers other releases: look again.
            self.updates.outcome = None;
        }
        ui.add_space(space::L);

        settings::section(ui, "Automatic checks");
        ui.add_enabled_ui(disabled.is_none(), |ui| {
            egui::ComboBox::from_id_salt("update interval")
                .selected_text(self.updates.prefs.interval.label())
                .width(ui.available_width().min(260.0))
                .show_ui(ui, |ui| {
                    for interval in CheckInterval::ALL {
                        ui.selectable_value(
                            &mut self.updates.prefs.interval,
                            interval,
                            interval.label(),
                        );
                    }
                });
        });
        ui.add(
            egui::Label::new(
                egui::RichText::new(
                    "A check asks GitHub for its list of Axcenity releases and nothing else: \
                     no account, no project details.",
                )
                .color(palette.text_weak),
            )
            .wrap(),
        );
        ui.add_space(space::S);
        ui.horizontal(|ui| {
            let check = widgets::button(
                widgets::Role::Secondary,
                Some(icons::ARROWS_CLOCKWISE),
                "Check now",
                &palette,
            );
            if ui
                .add_enabled(disabled.is_none() && !self.updates.checking(), check)
                .clicked()
            {
                self.check_for_updates(true);
            }
            if self.updates.checking() {
                ui.spinner();
                ui.label(egui::RichText::new("Checking…").color(palette.text_weak));
            }
        });
        if let Some(last) = self.updates.prefs.last_check {
            let then = UNIX_EPOCH + Duration::from_secs(last);
            ui.label(
                egui::RichText::new(format!(
                    "Last checked {}",
                    crate::start::ago(then, SystemTime::now())
                ))
                .color(palette.text_weak),
            );
        }
        ui.add_space(space::L);
        self.update_result(ui);
    }

    /// The last check's result on the Updates page.
    fn update_result(&mut self, ui: &mut egui::Ui) {
        let palette = theme::palette(ui.ctx());
        match self.updates.outcome.clone() {
            None => {}
            Some(Outcome::UpToDate) => {
                ui.label(theme::with_icon(
                    icons::CHECK_CIRCLE,
                    "Axcenity is up to date.",
                ));
            }
            Some(Outcome::Failed(error)) => {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(format!("Couldn't check: {error}"))
                            .color(palette.warning),
                    )
                    .wrap(),
                );
            }
            Some(Outcome::Available(update)) => {
                let manifest = &update.manifest;
                settings::section(ui, "Available");
                ui.label(
                    egui::RichText::new(format!(
                        "Axcenity {}: {}",
                        manifest.version, manifest.title
                    ))
                    .color(palette.text_strong),
                );
                if !manifest.summary.is_empty() {
                    ui.add(egui::Label::new(manifest.summary.trim()).wrap());
                }
                if !manifest.known_issues.is_empty() {
                    ui.label(
                        egui::RichText::new(format!(
                            "Known issues: {}",
                            manifest.known_issues.join(", ")
                        ))
                        .color(palette.text_weak),
                    );
                }
                ui.horizontal_wrapped(|ui| {
                    ui.hyperlink_to(
                        theme::with_icon(icons::ARROW_SQUARE_OUT, "Release page and downloads"),
                        &update.candidate.page,
                    );
                    let version = update.candidate.version.clone();
                    if self.updates.prefs.notifies(&version)
                        && ui
                            .button("Don't remind me")
                            .on_hover_text("Hides the chip until a newer version comes out.")
                            .clicked()
                    {
                        self.updates.prefs.dismissed = Some(version);
                    }
                });
            }
        }
    }
}

/// What following `channel` means, for the Updates page.
fn channel_about(channel: Channel) -> &'static str {
    match channel {
        Channel::Stable => "Stable: fully tested releases only.",
        Channel::Beta => "Beta: stable releases and betas of what's coming next.",
        Channel::Development => {
            "Development: every alpha too, the newest work, sometimes rough. Back up projects \
             before opening them in a newer alpha."
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_finished_check_is_taken_once_and_counts_as_the_last_check() {
        let mut updates = Updates::default();
        let installed = axcenity_release::Version::parse("0.2.0-alpha.1").unwrap();
        // GitHub has no releases yet: up to date, without asking for more.
        updates.start(true, installed, "unused", |url| {
            assert_eq!(url, updates::releases_url());
            Ok(b"[]".to_vec())
        });
        assert!(updates.checking());
        let taken = loop {
            if let Some(taken) = updates.finish(1_000) {
                break taken;
            }
            std::thread::yield_now();
        };
        assert!(matches!(taken, (Outcome::UpToDate, true)));
        assert!(!updates.checking());
        assert_eq!(updates.prefs.last_check, Some(1_000));
        assert!(updates.finish(2_000).is_none());
        assert!(updates.available().is_none());
    }

    #[test]
    fn a_failed_check_is_kept_as_a_message() {
        let mut updates = Updates::default();
        let installed = axcenity_release::Version::parse("0.2.0-alpha.1").unwrap();
        updates.start(false, installed, "unused", |_| Err("offline".to_owned()));
        let (outcome, by_hand) = loop {
            if let Some(taken) = updates.finish(5) {
                break taken;
            }
            std::thread::yield_now();
        };
        assert!(!by_hand);
        let Outcome::Failed(message) = outcome else {
            panic!("{outcome:?}")
        };
        assert!(message.contains("offline"), "{message}");
    }

    #[test]
    fn the_settings_survive_saving_and_bad_text_gives_the_defaults() {
        let mut updates = Updates::default();
        updates.prefs.channel = Some(Channel::Beta);
        updates.prefs.interval = CheckInterval::Weekly;
        let text = updates.to_text();
        assert_eq!(Updates::load(&text), updates.prefs);
        assert_eq!(Updates::load("not json"), UpdatePrefs::default());
    }

    #[test]
    fn a_found_update_shows_on_the_page_and_as_a_chip_until_dismissed() {
        let manifest: axcenity_release::Manifest = serde_json::from_slice(include_bytes!(
            "../../../release/src/testdata/manifest.json"
        ))
        .unwrap();
        let mut app = EditorApp::new(None);
        app.updates.outcome = Some(Outcome::Available(Box::new(Update {
            candidate: updates::Candidate {
                version: manifest.version.clone(),
                tag: manifest.tag.clone(),
                page: "https://example.invalid/release".to_owned(),
            },
            manifest,
        })));
        assert!(app.updates.available().is_some());
        app.settings.open = true;
        app.settings.page = settings::SettingsPage::Updates;
        let mut draw = |ctx: &egui::Context| {
            egui::CentralPanel::default().show(ctx, |_| {});
            app.settings_window(ctx);
        };
        let ctx = theme::test_context();
        for _ in 0..2 {
            let _ = ctx.run(Default::default(), &mut draw);
        }
        if let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS") {
            crate::snapshot::snapshot(
                &std::path::Path::new(&folder).join("settings-updates-available.png"),
                egui::vec2(900.0, 640.0),
                vec![Vec::new(); 3],
                &mut draw,
            );
        }
        let version = app.updates.available().unwrap().candidate.version.clone();
        app.updates.prefs.dismissed = Some(version);
        assert!(app.updates.available().is_none());
    }

    #[test]
    fn a_local_build_never_checks() {
        // Tests run a local build: nothing starts, even by hand.
        let mut app = EditorApp::new(None);
        app.check_for_updates(true);
        app.updates_frame();
        assert!(!app.updates.checking());
    }
}
