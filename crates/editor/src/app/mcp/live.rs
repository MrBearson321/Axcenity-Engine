//! AI clients working in the open editor (`docs/editor/mcp.md`, "In the
//! open editor"; design: `docs/research/mcp/architecture.md` §3.2–3.3).
//!
//! With Settings > AI connections on and a project open, the editor
//! listens on a loopback port and writes it, with a fresh token, to
//! [`ENDPOINT_FILE`] in the project. `axcenity-editor --mcp <project>`
//! reads that file first: if the editor answers, it only relays stdin and
//! stdout to it ([`relay`]), so the client's calls change what's on screen
//! and land in the editor's undo history. Otherwise it serves the project
//! without a window, as before, and refuses writes while an editor that
//! takes AI connections has the project open ([`editor_listening`]), so
//! two copies never overwrite each other.
//!
//! The connection starts with one line each way: the relay sends
//! `{"token", "profile", "mode", "allow_import"}`, the editor answers
//! `ok` or `no <why>`. Then it's MCP, served by `rmcp` on the
//! connection's own thread. Calls come to the editor's main thread as
//! jobs, run between frames within [`BUDGET`] ([`Live::drain`]); a job
//! wakes an idle window. A long call (`game_run`) holds the window while
//! it runs.

use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use axcenity_mcp::{Job, Mode, Profile, Waker};
use eframe::egui;
use serde_json::{Value, json};

use super::changes::ChangeLog;
use super::{Host, Session, run, setup};
use crate::app::EditorApp;

/// Where the open editor says how to reach it, in the project's hidden
/// folder: `{"port", "token", "pid"}`.
pub(crate) const ENDPOINT_FILE: &str = ".axcenity/mcp-endpoint.json";

/// Written while a server without a window has the project open, so the
/// editor can warn that the two would overwrite each other's saves.
pub(crate) const HEADLESS_FILE: &str = ".axcenity/mcp-headless.lock";

/// The most time a frame spends running calls before it draws again.
pub(crate) const BUDGET: Duration = Duration::from_millis(4);

/// How long either side waits for the other's first line.
const HELLO_WAIT: Duration = Duration::from_secs(3);

/// The longest first line either side reads.
const HELLO_LIMIT: usize = 64 * 1024;

/// What the relay asks for: the options `--mcp` was started with.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Hello {
    pub profile: Profile,
    pub mode: Mode,
    pub import_folders: Vec<PathBuf>,
    /// Only asks whether the editor would take a client, then hangs up.
    pub probe: bool,
}

impl Hello {
    fn to_line(&self, token: &str) -> String {
        let folders: Vec<String> = self
            .import_folders
            .iter()
            .map(|folder| folder.display().to_string())
            .collect();
        json!({
            "token": token,
            "profile": self.profile.name(),
            "mode": self.mode.name(),
            "allow_import": folders,
            "probe": self.probe,
        })
        .to_string()
    }

    /// The hello in `line`, if its token is `token`; else why not.
    fn from_line(line: &str, token: &str) -> Result<Hello, String> {
        let value: Value = serde_json::from_str(line).map_err(|_| "not a hello".to_owned())?;
        if value["token"].as_str() != Some(token) {
            return Err("wrong token; the editor was restarted, so read the endpoint again".into());
        }
        let profile = value["profile"].as_str().unwrap_or("standard");
        let mode = value["mode"].as_str().unwrap_or("edit");
        Ok(Hello {
            profile: Profile::from_name(profile).ok_or(format!("unknown profile {profile}"))?,
            mode: Mode::from_name(mode).ok_or(format!("unknown mode {mode}"))?,
            import_folders: value["allow_import"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(PathBuf::from)
                .collect(),
            probe: value["probe"].as_bool().unwrap_or(false),
        })
    }
}

/// A connection that said hello, for the main thread to pick up.
struct Incoming {
    jobs: mpsc::Receiver<Job>,
    hello: Hello,
    /// To hang up when the editor stops serving.
    stream: TcpStream,
}

/// A connected client, as the Settings page lists it.
pub(crate) struct ClientInfo {
    pub name: String,
    pub mode: Mode,
    pub profile: Profile,
}

struct Client {
    jobs: mpsc::Receiver<Job>,
    session: Session,
    profile: Profile,
    stream: TcpStream,
    /// Its connection ended.
    gone: bool,
}

/// The open editor taking AI connections for one project.
pub(crate) struct Live {
    folder: PathBuf,
    port: u16,
    token: String,
    stop: Arc<AtomicBool>,
    listener: Option<JoinHandle<()>>,
    incoming: Arc<Mutex<Vec<Incoming>>>,
    clients: Vec<Client>,
    /// One log for every client, so each sees the others' changes.
    changes: ChangeLog,
    /// The last session number given out.
    last_me: u32,
}

impl Live {
    /// Listens for clients of the project in `folder` and writes its
    /// [`ENDPOINT_FILE`]. `waker` is called whenever a client calls.
    pub(crate) fn start(folder: &Path, waker: Waker) -> std::io::Result<Live> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        let port = listener.local_addr()?.port();
        let token = new_token();
        write_endpoint(folder, port, &token)?;
        let stop = Arc::new(AtomicBool::new(false));
        let incoming = Arc::new(Mutex::new(Vec::new()));
        let thread = {
            let (stop, incoming, token) = (stop.clone(), incoming.clone(), token.clone());
            std::thread::Builder::new()
                .name("mcp listener".into())
                .spawn(move || listen(&listener, &stop, &incoming, &token, &waker))?
        };
        Ok(Live {
            folder: folder.to_owned(),
            port,
            token,
            stop,
            listener: Some(thread),
            incoming,
            clients: Vec::new(),
            changes: ChangeLog::default(),
            last_me: 0,
        })
    }

    pub(crate) fn port(&self) -> u16 {
        self.port
    }

    pub(crate) fn clients(&self) -> Vec<ClientInfo> {
        self.clients
            .iter()
            .map(|client| ClientInfo {
                name: client.session.client.clone(),
                mode: client.session.mode,
                profile: client.profile,
            })
            .collect()
    }

    /// Takes in new clients and runs the calls waiting, one at a time, for
    /// up to [`BUDGET`]. True if calls may still be waiting (the frame
    /// should come round again soon).
    pub(crate) fn drain(&mut self, app: &mut EditorApp) -> bool {
        let arrived: Vec<Incoming> = self
            .incoming
            .lock()
            .map(|mut list| list.drain(..).collect())
            .unwrap_or_default();
        for incoming in arrived {
            if self.clients.is_empty() {
                // The first client: noting changes starts now.
                self.changes = ChangeLog::default();
            }
            self.last_me += 1;
            let mut session = Session::new(incoming.hello.mode, self.last_me);
            session.import_folders = incoming.hello.import_folders;
            self.clients.push(Client {
                jobs: incoming.jobs,
                session,
                profile: incoming.hello.profile,
                stream: incoming.stream,
                gone: false,
            });
        }
        if self.clients.is_empty() {
            return false;
        }
        // What the user did since the last frame.
        self.changes.take(&mut app.world, None);
        let started = Instant::now();
        let mut more = false;
        'rounds: loop {
            let mut ran = false;
            for client in &mut self.clients {
                if started.elapsed() >= BUDGET {
                    more = true;
                    break 'rounds;
                }
                let job = match client.jobs.try_recv() {
                    Ok(job) => job,
                    Err(TryRecvError::Empty) => continue,
                    Err(TryRecvError::Disconnected) => {
                        client.gone = true;
                        continue;
                    }
                };
                let mut host = Host {
                    app: &mut *app,
                    session: &mut client.session,
                    changes: &mut self.changes,
                };
                let outcome = run(
                    &mut host,
                    job.call.tool,
                    &job.call.args,
                    job.call.client.as_deref(),
                );
                job.answer_with(outcome, client.session.png.take());
                ran = true;
            }
            if !ran {
                break;
            }
        }
        // The last client gone: the world stops noting changes.
        self.clients.retain(|client| !client.gone);
        if self.clients.is_empty() {
            app.world.stop_recording_changes();
        }
        more
    }
}

impl Drop for Live {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        // Wakes the listener's `accept`, which then sees `stop`.
        let _ = TcpStream::connect_timeout(
            &SocketAddr::from(([127, 0, 0, 1], self.port)),
            Duration::from_secs(1),
        );
        if let Some(thread) = self.listener.take() {
            let _ = thread.join();
        }
        let waiting = self
            .incoming
            .lock()
            .map(|mut list| list.drain(..).collect::<Vec<_>>())
            .unwrap_or_default();
        for stream in waiting
            .iter()
            .map(|incoming| &incoming.stream)
            .chain(self.clients.iter().map(|client| &client.stream))
        {
            let _ = stream.shutdown(Shutdown::Both);
        }
        // Only ours: another editor may have the project open since.
        if read_endpoint(&self.folder).is_some_and(|(_, token)| token == self.token) {
            let _ = std::fs::remove_file(self.folder.join(ENDPOINT_FILE));
        }
    }
}

/// The listener's thread: one thread per connection, until `stop`.
fn listen(
    listener: &TcpListener,
    stop: &AtomicBool,
    incoming: &Arc<Mutex<Vec<Incoming>>>,
    token: &str,
    waker: &Waker,
) {
    for stream in listener.incoming() {
        if stop.load(Ordering::SeqCst) {
            break;
        }
        let Ok(stream) = stream else {
            continue;
        };
        let (incoming, token, waker) = (incoming.clone(), token.to_owned(), waker.clone());
        let _ = std::thread::Builder::new()
            .name("mcp client".into())
            .spawn(move || {
                let _ = greet(stream, &incoming, &token, waker);
            });
    }
}

/// One connection: its hello, then MCP until it ends.
fn greet(
    mut stream: TcpStream,
    incoming: &Mutex<Vec<Incoming>>,
    token: &str,
    waker: Waker,
) -> std::io::Result<()> {
    // Answers go out as they're written: with Nagle's algorithm on, a
    // short answer waits for the client's delayed ACK (about 40 ms).
    stream.set_nodelay(true)?;
    stream.set_read_timeout(Some(HELLO_WAIT))?;
    let line = read_line(&mut stream)?;
    let hello = match Hello::from_line(&line, token) {
        Ok(hello) => hello,
        Err(why) => {
            writeln!(stream, "no {why}")?;
            return Ok(());
        }
    };
    writeln!(stream, "ok")?;
    if hello.probe {
        return Ok(());
    }
    stream.set_read_timeout(None)?;
    let (jobs, calls) = mpsc::channel();
    let setup = setup(hello.profile, Some(waker.clone()));
    if let Ok(mut list) = incoming.lock() {
        list.push(Incoming {
            jobs: calls,
            hello,
            stream: stream.try_clone()?,
        });
    }
    (waker.0)();
    axcenity_mcp::serve_tcp(setup, jobs, stream)
}

/// One line, read a byte at a time so nothing after it is taken (the
/// protocol starts right after). Without its newline.
fn read_line(stream: &mut TcpStream) -> std::io::Result<String> {
    let mut bytes = Vec::new();
    let mut byte = [0u8];
    while bytes.len() < HELLO_LIMIT {
        if stream.read(&mut byte)? == 0 || byte[0] == b'\n' {
            break;
        }
        bytes.push(byte[0]);
    }
    Ok(String::from_utf8_lossy(&bytes).trim().to_owned())
}

/// A token nobody else can guess: 128 bits from the standard library's
/// randomly seeded hasher (the editor has no other random source), mixed
/// with the time and process.
fn new_token() -> String {
    use std::hash::{BuildHasher, Hasher};
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |time| time.as_nanos());
    (0..2u64)
        .map(|round| {
            let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
            hasher.write_u64(round);
            hasher.write_u128(nanos);
            hasher.write_u32(std::process::id());
            format!("{:016x}", hasher.finish())
        })
        .collect()
}

/// Writes the endpoint file whole (a reader never sees half of it).
fn write_endpoint(folder: &Path, port: u16, token: &str) -> std::io::Result<()> {
    let path = folder.join(ENDPOINT_FILE);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let text = json!({"port": port, "token": token, "pid": std::process::id()}).to_string();
    let partial = path.with_extension("json.partial");
    std::fs::write(&partial, text)?;
    std::fs::rename(&partial, &path)
}

/// The port and token in the project's endpoint file, if there is one.
fn read_endpoint(folder: &Path) -> Option<(u16, String)> {
    let text = std::fs::read_to_string(folder.join(ENDPOINT_FILE)).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    let port = u16::try_from(value["port"].as_u64()?).ok()?;
    Some((port, value["token"].as_str()?.to_owned()))
}

/// Connects to the editor that has `folder` open and says `hello`. `Ok(None)`
/// when no editor is listening; `Err` when one answered but said no.
pub(super) fn connect(folder: &Path, hello: &Hello) -> Result<Option<TcpStream>, String> {
    let Some((port, token)) = read_endpoint(folder) else {
        return Ok(None);
    };
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let Ok(mut stream) = TcpStream::connect_timeout(&address, Duration::from_secs(1)) else {
        return Ok(None);
    };
    let said = (|| {
        stream.set_nodelay(true)?;
        stream.set_read_timeout(Some(HELLO_WAIT))?;
        writeln!(stream, "{}", hello.to_line(&token))?;
        read_line(&mut stream)
    })();
    match said {
        Ok(answer) if answer == "ok" => {
            let _ = stream.set_read_timeout(None);
            Ok(Some(stream))
        }
        Ok(answer) if answer.starts_with("no ") => Err(answer[3..].to_owned()),
        // Something else has the port now: the file is stale.
        _ => Ok(None),
    }
}

/// Whether an editor that takes AI connections has `folder` open.
pub(super) fn editor_listening(folder: &Path) -> bool {
    let probe = Hello {
        profile: Profile::Standard,
        mode: Mode::Read,
        import_folders: Vec::new(),
        probe: true,
    };
    matches!(connect(folder, &probe), Ok(Some(_)))
}

/// What [`relay`] did.
pub(super) enum Relayed {
    /// No editor is listening for this project: serve it without a window.
    NoEditor,
    /// An editor answered but refused, and why.
    Refused(String),
    /// Relayed until one side closed; whether that went cleanly.
    Done(bool),
}

/// If the open editor takes AI connections for `folder`, relays this
/// process's stdin and stdout to it until either side closes.
pub(super) fn relay(folder: &Path, hello: &Hello) -> Relayed {
    let stream = match connect(folder, hello) {
        Ok(Some(stream)) => stream,
        Ok(None) => return Relayed::NoEditor,
        Err(why) => return Relayed::Refused(why),
    };
    eprintln!("axcenity-editor --mcp: working in the open editor");
    let Ok(mut to_editor) = stream.try_clone() else {
        return Relayed::Done(false);
    };
    // Stdin's thread is left blocked when the editor closes first; the
    // process ends anyway.
    std::thread::spawn(move || {
        let _ = std::io::copy(&mut std::io::stdin().lock(), &mut to_editor);
        let _ = to_editor.shutdown(Shutdown::Write);
    });
    let mut from_editor = stream;
    let mut stdout = std::io::stdout().lock();
    let copied = std::io::copy(&mut from_editor, &mut stdout).and_then(|_| stdout.flush());
    Relayed::Done(copied.is_ok())
}

/// [`HEADLESS_FILE`] while this process serves the project without a
/// window; removed when dropped.
pub(super) struct HeadlessMark(PathBuf);

impl HeadlessMark {
    pub(super) fn new(folder: &Path) -> HeadlessMark {
        let path = folder.join(HEADLESS_FILE);
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(&path, json!({"pid": std::process::id()}).to_string());
        HeadlessMark(path)
    }
}

impl Drop for HeadlessMark {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// The editor's side: Settings > AI connections, and the [`Live`] it runs.
#[derive(Default)]
pub(crate) struct AiConnections {
    /// Settings > AI connections (saved between runs; off at first).
    pub allowed: bool,
    pub live: Option<Live>,
    /// The project folder last started for (or failed to start for), so a
    /// failure isn't retried every frame.
    tried: Option<PathBuf>,
    /// Why listening failed, for the Settings page.
    pub error: Option<String>,
    /// The project folder last looked at for a server without a window.
    warned: Option<PathBuf>,
}

impl EditorApp {
    /// Takes what the edit world recorded, as code that turns recording on
    /// for itself does (plugins), and hands a copy to the AI clients'
    /// change log, which would otherwise see a gap and start again.
    pub(crate) fn take_world_changes(&mut self) -> Vec<axcenity_core::RecordedChange> {
        let changes = self.world.take_changes();
        if let Some(live) = &mut self.ai.live
            && !live.clients.is_empty()
        {
            live.changes.add(None, changes.clone());
        }
        changes
    }

    /// Keeps the listener in step with the setting and the open project,
    /// then runs waiting calls. Called each frame once a project is loaded.
    pub(crate) fn ai_frame(&mut self, ctx: &egui::Context) {
        let folder = self.project.as_ref().map(|project| project.folder.clone());
        if folder != self.ai.warned {
            self.ai.warned.clone_from(&folder);
            if let Some(folder) = &folder
                && folder.join(HEADLESS_FILE).exists()
            {
                self.output.warning(
                    "An AI client may be working on this project without the window \
                     (axcenity-editor --mcp). Its saves and yours would overwrite each other: \
                     turn on Settings > AI connections and restart the client so it works here.",
                );
            }
        }
        let wanted = folder.filter(|_| self.ai.allowed);
        if wanted != self.ai.tried {
            self.ai.live = None;
            self.ai.error = None;
            self.ai.tried.clone_from(&wanted);
            if let Some(folder) = wanted {
                let ctx = ctx.clone();
                let waker = Waker(Arc::new(move || ctx.request_repaint()));
                match Live::start(&folder, waker) {
                    Ok(live) => self.ai.live = Some(live),
                    Err(error) => {
                        let why = format!("AI connections: could not listen: {error}");
                        self.output.warning(why.clone());
                        self.ai.error = Some(why);
                    }
                }
            }
        }
        // Lent out for the calls, which need the whole editor.
        if let Some(mut live) = self.ai.live.take() {
            let more = live.drain(self);
            self.ai.live = Some(live);
            if more {
                ctx.request_repaint();
            }
        }
    }
}

impl EditorApp {
    /// Settings > AI connections.
    pub(crate) fn ai_page(&mut self, ui: &mut egui::Ui) {
        use crate::app::settings;
        use crate::theme::{self, icons};
        use crate::widgets;
        settings::page_title(
            ui,
            "AI connections",
            "Let AI clients such as Claude Code, Cursor or Codex work in this window (MCP).",
        );
        let palette = theme::palette(ui.ctx());
        let weak = |ui: &mut egui::Ui, text: &str| {
            ui.add(egui::Label::new(egui::RichText::new(text).color(palette.text_weak)).wrap());
        };
        settings::section(ui, "Allow AI connections");
        ui.checkbox(
            &mut self.ai.allowed,
            theme::with_icon(icons::ROBOT, "Allow AI connections"),
        )
        .on_hover_text(
            "Clients started with axcenity-editor --mcp work in this window: you see what they \
             change, and each call is a step in Undo. Only programs on this computer that can \
             read the project's files can connect.",
        );
        ui.add_space(4.0);
        match (&self.ai.live, &self.ai.error, &self.project) {
            _ if !self.ai.allowed => weak(
                ui,
                "Off: a client opens its own copy of the project without a window. Don't edit \
                 the project here at the same time; your saves would overwrite each other.",
            ),
            (_, Some(error), _) => weak(ui, error),
            (_, _, None) => weak(ui, "Open a project to take connections."),
            (None, _, Some(_)) => weak(ui, "Starting…"),
            (Some(live), _, Some(project)) => {
                ui.label(format!(
                    "Taking connections for {} (port {})",
                    project.name,
                    live.port()
                ));
                let clients = live.clients();
                if clients.is_empty() {
                    weak(ui, "No client is connected.");
                }
                for client in clients {
                    ui.label(format!(
                        "{} · {} mode · {} tools",
                        client.name,
                        client.mode.name(),
                        client.profile.name()
                    ));
                }
            }
        }
        if let Some(project) = &self.project {
            ui.add_space(theme::space::L);
            settings::section(ui, "Add to a client");
            weak(ui, "Claude Code, from a terminal:");
            let command = super::check::claude_code_command(&project.folder);
            ui.add(
                egui::Label::new(egui::RichText::new(&command).monospace())
                    .wrap()
                    .selectable(true),
            );
            if ui
                .add(widgets::button(
                    widgets::Role::Secondary,
                    Some(icons::COPY),
                    "Copy",
                    &palette,
                ))
                .clicked()
            {
                ui.ctx().copy_text(command);
            }
            weak(
                ui,
                "Other clients: axcenity-editor --mcp --check <project> prints their settings \
                 (docs/mcp/README.md). A client that was already running connects here once \
                 it restarts its server.",
            );
        }
    }
}
