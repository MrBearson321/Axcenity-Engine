//! Collaborate's first-run setup and Start Session (C4 of
//! `docs/research/collaborate.md`; ADR-0021): a wizard that explains what
//! Collaborate is, gets Tailscale installed, running and signed in, checks
//! this computer can host, and says when it's ready; then a Start Session
//! dialog (name, role and lifetime of the invite) that hosts and hands over
//! the invite with what the other person has to do.
//!
//! Nothing here needs networking knowledge from the user: every step says
//! what to click, in plain words. Tailscale's state comes from a
//! background thread ([`Watch`]) every two seconds while the wizard is
//! open, so the editor never waits on the `tailscale` tool. The time each
//! setup takes is measured on this computer and said in Output (it's never
//! sent anywhere).

use std::net::{IpAddr, TcpListener, ToSocketAddrs};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant};

use axcenity_collab::socket::DEFAULT_PORT;
use axcenity_collab::tailscale::{self, Cli, Runner, State, Tailnet};

use super::team_panel::{InviteLifetime, InviteRole};
use super::*;
use crate::dialog;

/// Tailscale's page of machines, where a host shares their computer with
/// someone on another tailnet (Machines › … › Share).
pub(crate) const TAILSCALE_MACHINES: &str = "https://login.tailscale.com/admin/machines";

/// How often the wizard asks Tailscale how it is.
const WATCH_EVERY: Duration = Duration::from_secs(2);

/// Why the wizard was opened: what comes after it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Purpose {
    /// Set Up Collaborate… from the menu: nothing after.
    Setup,
    /// Start Session: the Start Session dialog after.
    Host,
    /// Join: the Collaborate panel after, to paste the link.
    Join,
}

/// The wizard's pages.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Page {
    /// What Collaborate is and why Tailscale.
    Welcome,
    /// Install, open, sign in, turn on: whatever Tailscale needs next.
    Tailscale,
    /// Tailscale is connected: checks that this computer can host.
    Check,
    /// Done.
    Ready,
}

/// What Tailscale needs from the user next, from its state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Step {
    Install,
    Open,
    SignIn { auth_url: Option<String> },
    Approve,
    TurnOn,
    Wait,
    Connected(Tailnet),
}

impl Step {
    pub(crate) fn of(state: &State) -> Step {
        match state {
            State::Missing => Step::Install,
            State::NotRunning { .. } => Step::Open,
            State::NeedsLogin { auth_url } => Step::SignIn {
                auth_url: auth_url.clone(),
            },
            State::NeedsApproval => Step::Approve,
            State::Stopped => Step::TurnOn,
            State::Starting | State::Other(_) => Step::Wait,
            State::Running(net) => Step::Connected(net.clone()),
        }
    }

    /// A short name, for the time each step took.
    fn name(&self) -> &'static str {
        match self {
            Step::Install => "installing Tailscale",
            Step::Open => "opening Tailscale",
            Step::SignIn { .. } => "signing in",
            Step::Approve => "waiting for approval",
            Step::TurnOn => "turning Tailscale on",
            Step::Wait => "connecting",
            Step::Connected(_) => "connected",
        }
    }

    /// The step's title and what to do, in plain words, and a line of
    /// help for when it doesn't work.
    pub(crate) fn guidance(&self) -> (&'static str, String, &'static str) {
        match self {
            Step::Install => (
                "Install Tailscale",
                "Tailscale is a free app that connects your computers privately, wherever they \
                 are, without changing your router. Download it, run the installer (it may ask \
                 for your computer's password), then come back here: this page moves on by \
                 itself."
                    .to_owned(),
                "On a work computer you may need your IT team to install it.",
            ),
            Step::Open => (
                "Open Tailscale",
                "Tailscale is installed but isn't running. Open the Tailscale app (on Windows \
                 from the Start menu, on a Mac from Applications); its icon appears next to the \
                 clock or in the menu bar."
                    .to_owned(),
                "On Linux, start it with `sudo systemctl start tailscaled`.",
            ),
            Step::SignIn { .. } => (
                "Sign in to Tailscale",
                "Sign in with a Google, Microsoft, GitHub or Apple account. Tailscale has no \
                 password of its own. Your browser opens; when it says you're connected, come \
                 back here. Everyone in a session can use the same account, or their own."
                    .to_owned(),
                "Nothing happens? Click the Tailscale icon next to the clock and choose Log in.",
            ),
            Step::Approve => (
                "Waiting for approval",
                "You're signed in, but the Tailscale network you joined asks its admin to \
                 approve new computers. Ask them to approve this one in Tailscale's admin \
                 console; this page moves on when they have."
                    .to_owned(),
                "Using your own account instead avoids this step.",
            ),
            Step::TurnOn => (
                "Turn Tailscale on",
                "Tailscale is switched off. Click its icon next to the clock (or in the menu \
                 bar) and switch it on."
                    .to_owned(),
                "",
            ),
            Step::Wait => (
                "Connecting…",
                "Tailscale is connecting. This takes a few seconds.".to_owned(),
                "",
            ),
            Step::Connected(net) => (
                "Tailscale is connected",
                format!(
                    "This computer is {} on Tailscale{}.",
                    net.dns_name.as_deref().unwrap_or(&net.machine),
                    net.login
                        .as_deref()
                        .map(|l| format!(", signed in as {l}"))
                        .unwrap_or_default()
                ),
                "",
            ),
        }
    }
}

/// One of the checks on the Check page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CheckResult {
    pub(crate) what: String,
    /// `Err` says what's wrong and what to do; `warning` marks a problem
    /// that doesn't stop setup.
    pub(crate) outcome: Result<String, String>,
    pub(crate) warning: bool,
}

/// Checks that this computer can host over Tailscale: it has a name
/// others can use, that name finds it, and the port is free. Blocks (name
/// lookups), so it runs off the UI thread.
pub(crate) fn host_checks(net: &Tailnet, port: u16) -> Vec<CheckResult> {
    let mut checks = Vec::new();
    let address = net.invite_address(port);
    checks.push(CheckResult {
        what: "A name others can reach".to_owned(),
        outcome: address.clone().ok_or_else(|| {
            "Tailscale gave this computer no address yet. Wait a moment and check again.".to_owned()
        }),
        warning: false,
    });
    if let Some(name) = &net.dns_name {
        let found: Vec<IpAddr> = (name.as_str(), port)
            .to_socket_addrs()
            .map(|found| found.map(|a| a.ip()).collect())
            .unwrap_or_default();
        let ours = found.iter().any(|ip| net.addresses.contains(ip));
        checks.push(CheckResult {
            what: "The name finds this computer".to_owned(),
            outcome: if ours {
                Ok(name.clone())
            } else {
                Err(format!(
                    "`{name}` doesn't lead here yet. Tailscale's names (MagicDNS) may still be \
                     starting; check again in a minute. Invites will use this computer's \
                     Tailscale address meanwhile."
                ))
            },
            warning: true,
        });
    }
    checks.push(CheckResult {
        what: format!("Port {port} is free for hosting"),
        outcome: match TcpListener::bind(("0.0.0.0", port)) {
            Ok(_) => Ok("free".to_owned()),
            Err(error) => Err(format!(
                "something on this computer already uses it ({error}). Another Axcenity may \
                 be hosting; that's fine if it's yours."
            )),
        },
        warning: true,
    });
    checks
}

/// Asks Tailscale how it is every [`WATCH_EVERY`] on its own thread, while
/// it's kept.
pub(crate) struct Watch {
    states: Receiver<State>,
    stop: Arc<AtomicBool>,
}

impl Watch {
    fn start(ctx: egui::Context) -> Option<Watch> {
        let (send, states) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        std::thread::Builder::new()
            .name("tailscale watch".into())
            .spawn(move || {
                while !stopped.load(Ordering::Relaxed) {
                    let cli = Cli::find();
                    let state = tailscale::state(cli.as_ref().map(|c| c as &dyn Runner));
                    if send.send(state).is_err() {
                        break;
                    }
                    ctx.request_repaint();
                    std::thread::sleep(WATCH_EVERY);
                }
            })
            .ok()?;
        Some(Watch { states, stop })
    }

    /// The newest state since last asked.
    fn latest(&self) -> Option<State> {
        let mut latest = None;
        loop {
            match self.states.try_recv() {
                Ok(state) => latest = Some(state),
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => return latest,
            }
        }
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// How long setup took, step by step, measured on this computer only.
#[derive(Clone, Debug)]
pub(crate) struct Timer {
    started: Instant,
    /// The step being timed and when it started.
    current: Option<(&'static str, Instant)>,
    /// Steps that took longer than a few seconds, in order.
    steps: Vec<(&'static str, Duration)>,
}

impl Timer {
    pub(crate) fn start(now: Instant) -> Timer {
        Timer {
            started: now,
            current: None,
            steps: Vec::new(),
        }
    }

    /// Notes that the user is on `step` at `now`.
    pub(crate) fn at(&mut self, step: &'static str, now: Instant) {
        if self.current.is_some_and(|(name, _)| name == step) {
            return;
        }
        if let Some((name, since)) = self.current.take() {
            let took = now.saturating_duration_since(since);
            if took >= Duration::from_secs(3) {
                self.steps.push((name, took));
            }
        }
        self.current = Some((step, now));
    }

    /// "Setup took 3 min 05 s (installing Tailscale 1 min 40 s, signing
    /// in 52 s)."
    pub(crate) fn summary(&mut self, now: Instant) -> String {
        self.at("done", now);
        let total = now.saturating_duration_since(self.started);
        let parts: Vec<String> = self
            .steps
            .iter()
            .map(|(name, took)| format!("{name} {}", duration_text(*took)))
            .collect();
        let goal = if total <= Duration::from_secs(300) {
            ""
        } else {
            " (over the five-minute goal: tell us what slowed you down)"
        };
        if parts.is_empty() {
            format!("Setup took {}{goal}.", duration_text(total))
        } else {
            format!(
                "Setup took {} ({}){goal}.",
                duration_text(total),
                parts.join(", ")
            )
        }
    }
}

/// "52 s", "3 min 05 s".
pub(crate) fn duration_text(duration: Duration) -> String {
    let seconds = duration.as_secs();
    if seconds < 60 {
        format!("{seconds} s")
    } else {
        format!("{} min {:02} s", seconds / 60, seconds % 60)
    }
}

/// The wizard and the Start Session dialog, kept between frames.
#[derive(Default)]
pub(crate) struct Setup {
    /// Showing, and why.
    pub(crate) open: Option<Purpose>,
    pub(crate) page: Option<Page>,
    /// Setup was finished once (saved between runs): Start and Join go
    /// straight on after that.
    pub(crate) done: bool,
    /// Tailscale's newest state, while the wizard is open.
    pub(crate) state: Option<State>,
    watch: Option<Watch>,
    /// `tailscale up` asked for a sign-in page.
    login: Option<Receiver<Result<Option<String>, String>>>,
    login_note: Option<String>,
    /// The Check page's results, and the thread making them.
    pub(crate) checks: Option<Vec<CheckResult>>,
    checking: Option<Receiver<Vec<CheckResult>>>,
    timer: Option<Timer>,
    /// The Start Session dialog, when open.
    pub(crate) start: Option<StartSession>,
}

/// The Start Session dialog.
pub(crate) struct StartSession {
    pub(crate) name: String,
    pub(crate) role: InviteRole,
    pub(crate) lifetime: InviteLifetime,
    /// The port to host on (tests pick a free one with 0).
    pub(crate) port: u16,
    /// Where Collaborate's files go: the editor's data folder when `None`
    /// (tests give their own).
    pub(crate) folder: Option<std::path::PathBuf>,
    /// After Start: the invite link to share, and how others reach us.
    pub(crate) hosted: Option<(String, bool)>,
    pub(crate) problem: Option<String>,
}

impl Setup {
    /// Opens the wizard for `purpose`, timing it from now.
    pub(crate) fn open(&mut self, purpose: Purpose) {
        self.open = Some(purpose);
        self.page = Some(Page::Welcome);
        self.checks = None;
        self.checking = None;
        self.login_note = None;
        self.timer = Some(Timer::start(Instant::now()));
    }

    /// Closes the wizard and stops asking Tailscale.
    pub(crate) fn close(&mut self) {
        self.open = None;
        self.page = None;
        self.watch = None;
        self.login = None;
    }

    /// Takes in what the background threads found.
    fn poll(&mut self, ctx: &egui::Context) {
        // Tests set `state` themselves: the computer running them may or
        // may not have Tailscale.
        if !cfg!(test)
            && self.open.is_some()
            && self.watch.is_none()
            && self.page != Some(Page::Welcome)
        {
            self.watch = Watch::start(ctx.clone());
        }
        if let Some(state) = self.watch.as_ref().and_then(Watch::latest) {
            self.state = Some(state);
        }
        if let Some(receive) = &self.login {
            match receive.try_recv() {
                Ok(Ok(Some(url))) => {
                    ctx.open_url(egui::OpenUrl::new_tab(&url));
                    self.login_note = Some(format!(
                        "Your browser should have opened Tailscale's sign-in page. If it \
                         didn't, open {url}"
                    ));
                    self.login = None;
                }
                Ok(Ok(None)) => {
                    self.login_note = Some(
                        "Tailscale didn't give a sign-in page: use Log in in the Tailscale app."
                            .to_owned(),
                    );
                    self.login = None;
                }
                Ok(Err(error)) => {
                    self.login_note =
                        Some(format!("{error}. Use Log in in the Tailscale app instead."));
                    self.login = None;
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => self.login = None,
            }
        }
        if let Some(receive) = &self.checking
            && let Ok(checks) = receive.try_recv()
        {
            self.checks = Some(checks);
            self.checking = None;
        }
        // Follow Tailscale: a page that's no longer right moves on (or back).
        if let (Some(page), Some(state)) = (self.page, &self.state) {
            match (page, state.is_running()) {
                (Page::Tailscale, true) => self.page = Some(Page::Check),
                (Page::Check, false) => {
                    self.page = Some(Page::Tailscale);
                    self.checks = None;
                }
                _ => {}
            }
            let step = Step::of(state);
            if let Some(timer) = &mut self.timer {
                timer.at(step.name(), Instant::now());
            }
        }
        if self.page == Some(Page::Check)
            && self.checks.is_none()
            && self.checking.is_none()
            && let Some(State::Running(net)) = &self.state
        {
            let (send, receive) = mpsc::channel();
            let net = net.clone();
            let ctx = ctx.clone();
            let _ = std::thread::Builder::new()
                .name("collaborate checks".into())
                .spawn(move || {
                    let _ = send.send(host_checks(&net, DEFAULT_PORT));
                    ctx.request_repaint();
                });
            self.checking = Some(receive);
        }
    }

    fn start_login(&mut self) {
        let Some(cli) = Cli::find() else {
            self.login_note = Some("Tailscale isn't installed yet.".to_owned());
            return;
        };
        let (send, receive) = mpsc::channel();
        let _ = std::thread::Builder::new()
            .name("tailscale sign-in".into())
            .spawn(move || {
                let _ = send.send(cli.start_login());
            });
        self.login = Some(receive);
        self.login_note = Some("Asking Tailscale for its sign-in page…".to_owned());
    }
}

/// The download page for this computer's system.
fn download_page() -> String {
    let system = if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "mac"
    } else {
        "linux"
    };
    format!("{}/{system}", tailscale::DOWNLOAD_PAGE)
}

/// A paragraph of weak text that wraps.
fn note(ui: &mut egui::Ui, text: &str) {
    let palette = theme::palette(ui.ctx());
    ui.add(egui::Label::new(egui::RichText::new(text).color(palette.text_secondary)).wrap());
}

/// What the wizard's buttons asked for.
enum Pick {
    Next,
    SkipTailscale,
    Download,
    SignIn,
    CheckAgain,
    Finish,
    Close,
}

impl EditorApp {
    /// File › Start Collaborate Session…: setup first if it was never
    /// done, else the Start Session dialog.
    pub(in crate::app) fn start_collaborate_session(&mut self) {
        if self.team_panel.setup.done {
            self.open_start_session();
        } else {
            self.team_panel.setup.open(Purpose::Host);
        }
    }

    /// File › Set Up Collaborate…: the wizard, whether or not it was done.
    pub(in crate::app) fn set_up_collaborate(&mut self) {
        self.team_panel.setup.open(Purpose::Setup);
    }

    pub(in crate::app) fn open_start_session(&mut self) {
        let name = self
            .project
            .as_ref()
            .and_then(|p| p.folder.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.team_panel.setup.start = Some(StartSession {
            name,
            role: self.team_panel.invite_role,
            lifetime: self.team_panel.invite_lifetime,
            port: DEFAULT_PORT,
            folder: None,
            hosted: None,
            problem: None,
        });
    }

    /// The setup wizard and the Start Session dialog, when open.
    pub(super) fn collab_setup_windows(&mut self, ctx: &egui::Context) {
        self.team_panel.setup.poll(ctx);
        if self.team_panel.setup.open.is_some() {
            self.collab_setup_wizard(ctx);
        }
        if self.team_panel.setup.start.is_some() {
            self.start_session_dialog(ctx);
        }
    }

    fn collab_setup_wizard(&mut self, ctx: &egui::Context) {
        let setup = &mut self.team_panel.setup;
        let page = setup.page.unwrap_or(Page::Welcome);
        let step = setup.state.as_ref().map(Step::of);
        let (pick, dismissed) = dialog::modal(ctx, "collaborate setup", 460.0, |ui| {
            let palette = theme::palette(ui.ctx());
            match page {
                Page::Welcome => {
                    dialog::header(
                        ui,
                        icons::USERS_THREE,
                        dialog::Tone::Question,
                        "Set up Collaborate",
                        Some(
                            "Collaborate lets several people edit one project at the same \
                             time, each in their own editor. One person hosts; others join \
                             with an invite link.",
                        ),
                    );
                    note(
                        ui,
                        "To reach each other from different homes or networks, Collaborate \
                         uses Tailscale, a free app that connects your computers privately. \
                         There's nothing to set up on your router. Setup takes about five \
                         minutes the first time, and you won't see it again.",
                    );
                    ui.add_space(theme::space::S);
                    note(
                        ui,
                        "Tailscale only connects the computers. Who may join and edit your \
                         project is still decided by your invite links and roles.",
                    );
                    dialog::buttons(
                        ui,
                        &[
                            dialog::Button::new(
                                icons::WIFI_HIGH,
                                "Same network only",
                                dialog::Kind::Plain,
                            )
                            .tooltip("Skip Tailscale: invites work for people on your network."),
                            dialog::Button::new(
                                icons::ARROW_RIGHT,
                                "Set up Tailscale",
                                dialog::Kind::Main,
                            ),
                        ],
                        dialog::Enter::Main,
                    )
                    .map(|i| {
                        if i == 0 {
                            Pick::SkipTailscale
                        } else {
                            Pick::Next
                        }
                    })
                }
                Page::Tailscale => {
                    let step = step.clone().unwrap_or(Step::Wait);
                    let (title, text, help) = step.guidance();
                    dialog::header(
                        ui,
                        icons::PLUGS_CONNECTED,
                        dialog::Tone::Question,
                        title,
                        None,
                    );
                    note(ui, &text);
                    if let Some(login) = &setup.login_note {
                        ui.add_space(theme::space::S);
                        note(ui, login);
                    }
                    if !help.is_empty() {
                        ui.add_space(theme::space::S);
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(help).small().color(palette.text_weak),
                            )
                            .wrap(),
                        );
                    }
                    if matches!(step, Step::Wait) || setup.state.is_none() {
                        ui.add_space(theme::space::S);
                        ui.spinner();
                    }
                    let main = match step {
                        Step::Install => {
                            Some((icons::DOWNLOAD_SIMPLE, "Download Tailscale", Pick::Download))
                        }
                        Step::SignIn { .. } => Some((icons::SIGN_IN, "Sign in", Pick::SignIn)),
                        _ => None,
                    };
                    let mut buttons = vec![dialog::Button::new(
                        icons::WIFI_HIGH,
                        "Same network only",
                        dialog::Kind::Plain,
                    )];
                    if let Some((icon, label, _)) = &main {
                        buttons.push(dialog::Button::new(icon, label, dialog::Kind::Main));
                    }
                    dialog::buttons(ui, &buttons, dialog::Enter::Main).map(|i| match (i, main) {
                        (0, _) => Pick::SkipTailscale,
                        (_, Some((_, _, pick))) => pick,
                        _ => Pick::Close,
                    })
                }
                Page::Check => {
                    let title = "Checking this computer";
                    dialog::header(ui, icons::SHIELD_CHECK, dialog::Tone::Question, title, None);
                    if let Some(step) = &step {
                        note(ui, &step.guidance().1);
                    }
                    ui.add_space(theme::space::S);
                    match &setup.checks {
                        None => {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label("Checking…");
                            });
                        }
                        Some(checks) => {
                            for check in checks {
                                let (icon, color, text) = match &check.outcome {
                                    Ok(text) => (icons::CHECK_CIRCLE, palette.success, text),
                                    Err(text) if check.warning => {
                                        (icons::WARNING, palette.warning, text)
                                    }
                                    Err(text) => (icons::X_CIRCLE, palette.error, text),
                                };
                                ui.horizontal_top(|ui| {
                                    ui.label(egui::RichText::new(icon).color(color));
                                    ui.vertical(|ui| {
                                        ui.label(
                                            egui::RichText::new(&check.what)
                                                .family(theme::medium_family()),
                                        );
                                        note(ui, text);
                                    });
                                });
                            }
                        }
                    }
                    let blocked = setup.checks.as_ref().is_none_or(|checks| {
                        checks.iter().any(|c| c.outcome.is_err() && !c.warning)
                    });
                    dialog::buttons(
                        ui,
                        &[
                            dialog::Button::new(
                                icons::ARROW_CLOCKWISE,
                                "Check again",
                                dialog::Kind::Plain,
                            ),
                            dialog::Button::new(icons::CHECK, "Continue", dialog::Kind::Main)
                                .enabled(!blocked)
                                .tooltip("Waiting for the checks."),
                        ],
                        dialog::Enter::Main,
                    )
                    .map(|i| if i == 0 { Pick::CheckAgain } else { Pick::Next })
                }
                Page::Ready => {
                    let text = match &setup.state {
                        Some(State::Running(_)) => {
                            "Collaborate is ready. Invites you make now work for anyone who can \
                             reach this computer over Tailscale: people on your Tailscale \
                             account, or people you share this computer with."
                        }
                        _ => {
                            "Collaborate is ready for people on your network. Set up Tailscale \
                             any time from File › Set Up Collaborate… to work with people \
                             elsewhere."
                        }
                    };
                    dialog::header(
                        ui,
                        icons::CHECK_CIRCLE,
                        dialog::Tone::Question,
                        "Ready",
                        Some(text),
                    );
                    let next = match setup.open {
                        Some(Purpose::Host) => "Start Session",
                        Some(Purpose::Join) => "Join a Session",
                        _ => "Done",
                    };
                    dialog::buttons(
                        ui,
                        &[dialog::Button::new(icons::CHECK, next, dialog::Kind::Main)],
                        dialog::Enter::Main,
                    )
                    .map(|_| Pick::Finish)
                }
            }
        });
        let pick = if dismissed { Some(Pick::Close) } else { pick };
        let Some(pick) = pick else {
            return;
        };
        let setup = &mut self.team_panel.setup;
        match pick {
            Pick::Next => {
                setup.page = Some(match page {
                    Page::Welcome => Page::Tailscale,
                    Page::Tailscale | Page::Check => Page::Ready,
                    Page::Ready => Page::Ready,
                });
                if setup.page == Some(Page::Tailscale) {
                    setup.state = None;
                }
            }
            Pick::SkipTailscale => setup.page = Some(Page::Ready),
            Pick::Download => ctx.open_url(egui::OpenUrl::new_tab(download_page())),
            Pick::SignIn => {
                if let Some(State::NeedsLogin {
                    auth_url: Some(url),
                }) = &setup.state
                {
                    ctx.open_url(egui::OpenUrl::new_tab(url));
                } else {
                    setup.start_login();
                }
            }
            Pick::CheckAgain => {
                setup.checks = None;
                setup.checking = None;
            }
            Pick::Finish => {
                let purpose = setup.open;
                let summary = setup.timer.as_mut().map(|t| t.summary(Instant::now()));
                setup.done = true;
                setup.close();
                if let Some(summary) = summary {
                    self.output.info(format!("Collaborate: {summary}"));
                }
                match purpose {
                    Some(Purpose::Host) => self.open_start_session(),
                    Some(Purpose::Join) => self.layout.open(Pane::TeamCreate),
                    _ => {}
                }
            }
            Pick::Close => setup.close(),
        }
    }

    fn start_session_dialog(&mut self, ctx: &egui::Context) {
        let open = self.project.is_some() && !self.playing();
        let Some(start) = &mut self.team_panel.setup.start else {
            return;
        };
        enum Go {
            Start,
            Close,
            Copy,
            Share,
        }
        let (go, dismissed) = dialog::modal(ctx, "start collaborate session", 440.0, |ui| {
            match &start.hosted {
                None => {
                    dialog::header(
                        ui,
                        icons::SHARE_NETWORK,
                        dialog::Tone::Question,
                        "Start Collaborate Session",
                        Some(
                            "Hosts this project from this editor. Keep it open while others work.",
                        ),
                    );
                    ui.label("Session name");
                    ui.add(dialog::text_field(&mut start.name).hint_text("Castle Game"));
                    ui.add_space(theme::space::S);
                    ui.label("The invite lets someone");
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut start.role, InviteRole::Editor, "Edit");
                        ui.selectable_value(&mut start.role, InviteRole::Viewer, "Only look");
                    });
                    ui.add_space(theme::space::XS);
                    ui.label("and works once, within");
                    ui.horizontal(|ui| {
                        for lifetime in [
                            InviteLifetime::Day,
                            InviteLifetime::Week,
                            InviteLifetime::Month,
                        ] {
                            ui.selectable_value(&mut start.lifetime, lifetime, lifetime.label());
                        }
                    });
                    if start.role == InviteRole::Viewer {
                        note(
                            ui,
                            "A viewer can't change anything, but still receives the whole \
                             project, scripts included.",
                        );
                    }
                    if let Some(problem) = &start.problem {
                        let palette = theme::palette(ui.ctx());
                        ui.add(
                            egui::Label::new(egui::RichText::new(problem).color(palette.error))
                                .wrap(),
                        );
                    }
                    dialog::buttons(
                        ui,
                        &[
                            dialog::Button::cancel(),
                            dialog::Button::new(icons::PLAY, "Start Session", dialog::Kind::Main)
                                .enabled(open)
                                .tooltip("Open a project first, and stop Play."),
                        ],
                        dialog::Enter::Main,
                    )
                    .map(|i| if i == 0 { Go::Close } else { Go::Start })
                }
                Some((link, over_tailscale)) => {
                    dialog::header(
                        ui,
                        icons::CHECK_CIRCLE,
                        dialog::Tone::Question,
                        "Your session is running",
                        Some(
                            "Send this invite link to the person joining (by chat or email). It works once.",
                        ),
                    );
                    let mut shown = link.clone();
                    ui.add(
                        egui::TextEdit::multiline(&mut shown)
                            .desired_rows(3)
                            .desired_width(f32::INFINITY),
                    );
                    ui.add_space(theme::space::S);
                    if *over_tailscale {
                        note(
                            ui,
                            "They need Tailscale too. If they sign in to Tailscale with your \
                             account, that's all. If they use their own account, share this \
                             computer with them once: Tailscale's Machines page › … next to \
                             this computer › Share, and send them that link as well.",
                        );
                    } else {
                        note(
                            ui,
                            "This invite works for people on your network. To work with people \
                             elsewhere, set up Tailscale (File › Set Up Collaborate…).",
                        );
                    }
                    let mut buttons = vec![dialog::Button::new(
                        icons::COPY,
                        "Copy link",
                        dialog::Kind::Plain,
                    )];
                    if *over_tailscale {
                        buttons.push(dialog::Button::new(
                            icons::ARROW_SQUARE_OUT,
                            "Share in Tailscale",
                            dialog::Kind::Plain,
                        ));
                    }
                    buttons.push(dialog::Button::new(
                        icons::CHECK,
                        "Done",
                        dialog::Kind::Main,
                    ));
                    let last = buttons.len() - 1;
                    dialog::buttons(ui, &buttons, dialog::Enter::Main).map(|i| match i {
                        0 => Go::Copy,
                        i if i == last => Go::Close,
                        _ => Go::Share,
                    })
                }
            }
        });
        let go = if dismissed { Some(Go::Close) } else { go };
        match go {
            None => {}
            Some(Go::Close) => self.team_panel.setup.start = None,
            Some(Go::Copy) => {
                if let Some((link, _)) = &start.hosted {
                    ctx.copy_text(link.clone());
                    self.toasts.success("Invite link copied");
                }
            }
            Some(Go::Share) => ctx.open_url(egui::OpenUrl::new_tab(TAILSCALE_MACHINES)),
            Some(Go::Start) => self.start_session_from_dialog(),
        }
    }

    /// Start Session's Start: hosts with the dialog's choices, and shows
    /// the invite, or why it didn't start.
    pub(crate) fn start_session_from_dialog(&mut self) {
        let Some(start) = &self.team_panel.setup.start else {
            return;
        };
        let name = start.name.trim().to_owned();
        let (role, lifetime, port) = (start.role, start.lifetime, start.port);
        let folder = start.folder.clone();
        self.team_panel.invite_role = role;
        self.team_panel.invite_lifetime = lifetime;
        self.team_panel.problem = None;
        let name = (!name.is_empty()).then_some(name);
        let hosted = self.host_named_session(folder.as_deref(), port, name);
        let Some(start) = &mut self.team_panel.setup.start else {
            return;
        };
        match hosted {
            Ok(invite) => {
                let over = tailscale::is_tailscale_host(&invite.host);
                start.hosted = Some((invite.link(), over));
                start.problem = None;
                self.layout.open(Pane::TeamCreate);
            }
            Err(error) => start.problem = Some(error),
        }
    }
}
