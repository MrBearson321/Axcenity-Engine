//! The MCP server's tools (`docs/editor/mcp.md`): what AI clients such as
//! Claude Code, Cursor or Codex can do in a project, through the same
//! editor code the panels use.
//!
//! `axcenity-editor --mcp <project>` relays to the open editor when it
//! takes AI connections for that project ([`live`]); otherwise it opens the
//! project without a window and serves the tools on stdin and stdout
//! ([`run_from_command_line`]). The
//! `axcenity_mcp` crate speaks the protocol on a thread of its own and hands
//! each call over as a [`Job`]; this thread owns the [`EditorApp`] and runs
//! them one at a time, so tools change the world exactly as the editor does.
//!
//! Each tool is a child module with a `SPEC` and a `call`, listed in
//! [`TOOLS`]. Tools share parameter constants ([`ID`], [`DETAIL`], …) so a
//! name means the same everywhere. Writes go through [`ops`]: each call is
//! one undo step labelled `AI (<client>): …`, a destructive one needs a
//! confirm token in `edit` mode, the session's first write keeps a version
//! of the project first, and every call is a line of `.axcenity/mcp-log.jsonl`.

use std::io::Write as _;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{SystemTime, UNIX_EPOCH};

use axcenity_mcp::{
    Allowed, Args, Class, Code, Confirms, Detail, Job, Kind, Mode, Outcome, Param, Profile, Setup,
    ToolError, ToolSpec, Waker,
};

use super::EditorApp;

mod anim_edit;
mod asset_find;
mod asset_import;
mod asset_manage;
mod batch;
mod changes;
mod check;
mod data_edit;
mod docs;
mod env_set;
mod game_run;
mod history;
mod level;
mod live;
mod material_edit;
mod object_create;
mod object_delete;
mod object_duplicate;
mod object_get;
mod object_set;
mod ops;
mod prefab;
mod project_check;
mod project_info;
mod project_save;
mod project_settings;
mod prompts;
mod refs;
mod render_stats;
mod scene_inspect;
mod script_read;
mod script_write;
mod solid_op;
mod style_edit;
mod terrain_edit;
mod values;
mod view_capture;

#[cfg(test)]
mod benchmarks;
#[cfg(test)]
mod tests;

pub(crate) use live::AiConnections;
pub(crate) use render_stats::LastDrawn;

/// The command-line flag that starts the server.
pub(crate) const FLAG: &str = "--mcp";

/// The stack the server's thread gets (`main.rs`): the editor's state is
/// built on it, which takes more than Windows' 1 MB main-thread stack in a
/// debug build. `mcp_stdio`'s tests start the binary with a 1 MB stack
/// limit to keep it honest.
pub(crate) const STACK_SIZE: usize = 64 * 1024 * 1024;

/// The toolkit's own version, separate from the engine's: tool names and
/// what their parameters mean only change with its major version.
pub(crate) const TOOLKIT_VERSION: &str = "0.1.0";

/// Sent to clients when they connect. Short: some put it in every prompt.
const INSTRUCTIONS: &str = "Axcenity Engine project tools. Name objects by ID \
(\"17\", from results) or path (\"Workspace/House/Door\"). Values: [x,y,z] \
(Y up; a part's Position is its centre), [r,g,b] 0-1, enum names as strings. \
Find with scene_inspect, read with object_get, look up classes with docs. \
Each edit is one undo step (see history). Deletes and saves preview first; \
call again with the confirm token. Object edits stay in memory until \
project_save; script_write saves its file at once.";

/// The editor and the calling session: what every tool gets. Borrowed,
/// so the open editor can lend itself for one call ([`live`]); a server
/// without a window owns them in a [`Headless`].
pub(super) struct Host<'a> {
    pub app: &'a mut EditorApp,
    pub session: &'a mut Session,
    /// What changed in the world, shared by every session of this editor
    /// (`history changes`).
    pub changes: &'a mut changes::ChangeLog,
}

/// A server without a window: the editor it opened, its one session and
/// its change log.
pub(super) struct Headless {
    pub app: EditorApp,
    pub session: Session,
    pub changes: changes::ChangeLog,
}

/// What lasts between calls of one server run.
pub(super) struct Session {
    /// The client's own name for itself (`clientInfo.name`), for undo
    /// labels and the log. Self-reported, so never trusted.
    pub client: String,
    pub mode: Mode,
    pub confirms: Confirms,
    /// Whether the version before this session's first write was kept.
    pub checkpointed: bool,
    /// The undo label of the last write, for the log.
    pub last_undo: Option<String>,
    /// Tells this run's lines in the log apart.
    pub id: String,
    /// Asset files this session wrote, read again after an undo or redo.
    pub files: Vec<String>,
    /// The folders `asset_import` may copy from: only what the user named
    /// with `--allow-import`.
    pub import_folders: Vec<PathBuf>,
    /// This session's number in the change log, to tell its changes from
    /// other sessions'.
    pub me: u32,
    /// The mark `history changes` last answered: its default `since`.
    pub asked: u64,
    /// The picture the last call made (`view_capture`), sent with its
    /// answer.
    pub png: Option<Vec<u8>>,
}

impl Session {
    pub fn new(mode: Mode, me: u32) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |time| time.subsec_nanos());
        Session {
            client: "AI".to_owned(),
            mode,
            confirms: Confirms::default(),
            checkpointed: false,
            last_undo: None,
            files: Vec::new(),
            import_folders: Vec::new(),
            me,
            asked: 0,
            png: None,
            id: format!(
                "{:x}",
                u64::from(std::process::id()) << 32 | u64::from(nanos)
            ),
        }
    }
}

impl Headless {
    pub fn new(mut app: EditorApp, mode: Mode) -> Self {
        let mut changes = changes::ChangeLog::default();
        // From here on the world's changes are noted, for `history changes`.
        changes.take(&mut app.world, None);
        Headless {
            app,
            session: Session::new(mode, 1),
            changes,
        }
    }

    pub fn host(&mut self) -> Host<'_> {
        Host {
            app: &mut self.app,
            session: &mut self.session,
            changes: &mut self.changes,
        }
    }
}

/// A tool: what clients see, and what runs it.
pub(super) struct Tool {
    pub spec: &'static ToolSpec,
    pub call: fn(&mut Host, &Args) -> Outcome,
}

/// Every tool, in the order clients list them.
pub(super) const TOOLS: &[Tool] = &[
    Tool {
        spec: &project_info::SPEC,
        call: project_info::call,
    },
    Tool {
        spec: &scene_inspect::SPEC,
        call: scene_inspect::call,
    },
    Tool {
        spec: &object_get::SPEC,
        call: object_get::call,
    },
    Tool {
        spec: &docs::SPEC,
        call: docs::call,
    },
    Tool {
        spec: &asset_find::SPEC,
        call: asset_find::call,
    },
    Tool {
        spec: &asset_import::SPEC,
        call: asset_import::call,
    },
    Tool {
        spec: &object_create::SPEC,
        call: object_create::call,
    },
    Tool {
        spec: &object_set::SPEC,
        call: object_set::call,
    },
    Tool {
        spec: &object_delete::SPEC,
        call: object_delete::call,
    },
    Tool {
        spec: &object_duplicate::SPEC,
        call: object_duplicate::call,
    },
    Tool {
        spec: &batch::SPEC,
        call: batch::call,
    },
    Tool {
        spec: &material_edit::SPEC,
        call: material_edit::call,
    },
    Tool {
        spec: &env_set::SPEC,
        call: env_set::call,
    },
    Tool {
        spec: &script_read::SPEC,
        call: script_read::call,
    },
    Tool {
        spec: &script_write::SPEC,
        call: script_write::call,
    },
    Tool {
        spec: &project_check::SPEC,
        call: project_check::call,
    },
    Tool {
        spec: &game_run::SPEC,
        call: game_run::call,
    },
    Tool {
        spec: &history::SPEC,
        call: history::call,
    },
    Tool {
        spec: &project_save::SPEC,
        call: project_save::call,
    },
    Tool {
        spec: &terrain_edit::SPEC,
        call: terrain_edit::call,
    },
    Tool {
        spec: &anim_edit::SPEC,
        call: anim_edit::call,
    },
    Tool {
        spec: &level::SPEC,
        call: level::call,
    },
    Tool {
        spec: &prefab::SPEC,
        call: prefab::call,
    },
    Tool {
        spec: &solid_op::SPEC,
        call: solid_op::call,
    },
    Tool {
        spec: &style_edit::SPEC,
        call: style_edit::call,
    },
    Tool {
        spec: &project_settings::SPEC,
        call: project_settings::call,
    },
    Tool {
        spec: &asset_manage::SPEC,
        call: asset_manage::call,
    },
    Tool {
        spec: &render_stats::SPEC,
        call: render_stats::call,
    },
    Tool {
        spec: &view_capture::SPEC,
        call: view_capture::call,
    },
    Tool {
        spec: &data_edit::SPEC,
        call: data_edit::call,
    },
];

// Parameters shared by several tools.
pub(super) const ID: Param = Param::optional("id", Kind::String, "ID or path");
pub(super) const IDS: Param = Param::optional("ids", Kind::Strings, "several IDs or paths");
pub(super) const DETAIL: Param =
    Param::optional("detail", Kind::Enum(Detail::NAMES), "how much to return");
pub(super) const LIMIT: Param = Param::optional("limit", Kind::Integer, "max rows");
pub(super) const CONFIRM: Param = Param::optional("confirm", Kind::String, "token from a preview");
pub(super) const CURSOR: Param = Param::optional("cursor", Kind::String, "from a cut result");

/// The tools a server with `profile` lists.
pub(super) fn tools_for(profile: Profile) -> Vec<&'static ToolSpec> {
    TOOLS
        .iter()
        .filter(|tool| profile.shows(tool.spec.profile))
        .map(|tool| tool.spec)
        .collect()
}

/// What a server with `profile` says about itself and lists.
pub(super) fn setup(profile: Profile, waker: Option<Waker>) -> Setup {
    Setup {
        version: TOOLKIT_VERSION.to_owned(),
        instructions: INSTRUCTIONS.to_owned(),
        tools: tools_for(profile),
        prompts: prompts::PROMPTS.to_vec(),
        resources: prompts::RESOURCES.to_vec(),
        waker,
    }
}

/// Runs one call on the editor, as `client`: refused if the session's mode
/// doesn't allow its tool, logged either way. An engine panic becomes an
/// `internal` error rather than taking the server down.
pub(super) fn run(host: &mut Host, tool: &ToolSpec, args: &Args, client: Option<&str>) -> Outcome {
    if let Some(client) = client.filter(|name| !name.trim().is_empty()) {
        host.session.client = client.trim().chars().take(40).collect();
    }
    host.session.last_undo = None;
    host.session.png = None;
    // Whatever changed between calls wasn't this client.
    host.changes.take(&mut host.app.world, None);
    let outcome = call(host, tool, args);
    host.changes
        .take(&mut host.app.world, Some(host.session.me));
    log(host, tool, args, &outcome);
    outcome
}

fn call(host: &mut Host, tool: &ToolSpec, args: &Args) -> Outcome {
    let Some(entry) = TOOLS.iter().find(|entry| entry.spec.name == tool.name) else {
        return Err(ToolError::not_found(format!("no tool {}", tool.name)));
    };
    if host.session.mode.allows(tool.class) == Allowed::No {
        return Err(host.session.mode.refusal(tool.name));
    }
    // The open editor's Play shows another world; edits wait for Stop.
    if tool.class != Class::Read && host.app.playing() {
        return Err(
            ToolError::new(Code::Busy, "the game is playing in the editor")
                .with_fix("stop Play in the editor, then try again"),
        );
    }
    let call = entry.call;
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| call(host, args))).unwrap_or_else(
        |panic| {
            let message = panic
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| panic.downcast_ref::<&str>().map(|s| (*s).to_owned()))
                .unwrap_or_else(|| "unknown".to_owned());
            Err(ToolError::internal(format!(
                "the engine failed running {}: {message}",
                tool.name
            )))
        },
    )
}

/// Where the log of calls goes, in the project's own hidden folder (never
/// exported).
pub(super) const LOG_FILE: &str = ".axcenity/mcp-log.jsonl";

/// The longest arguments the log keeps, in characters.
const LOGGED_ARGS: usize = 300;

/// Adds a line for this call to the project's [`LOG_FILE`]. A log that
/// can't be written is said on stderr; the call still counts.
fn log(host: &Host, tool: &ToolSpec, args: &Args, outcome: &Outcome) {
    let Some(project) = &host.app.project else {
        return;
    };
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |time| time.as_secs());
    let mut arguments = serde_json::Value::Object(args.0.clone()).to_string();
    if arguments.chars().count() > LOGGED_ARGS {
        arguments = arguments.chars().take(LOGGED_ARGS).collect::<String>() + "…";
    }
    let result = match outcome {
        Ok(_) => "ok",
        Err(error) => error.code.as_str(),
    };
    let line = serde_json::json!({
        "time": time,
        "session": host.session.id,
        "client": host.session.client,
        "tool": tool.name,
        "class": tool.class.name(),
        "args": arguments,
        "result": result,
        "undo": host.session.last_undo,
    });
    let path = project.folder.join(LOG_FILE);
    let written = path
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
        })
        .and_then(|mut file| writeln!(file, "{line}"));
    if let Err(error) = written {
        eprintln!(
            "axcenity-editor --mcp: could not write {}: {error}",
            path.display()
        );
    }
}

/// What `--mcp` was given.
#[derive(Debug, PartialEq)]
struct Options {
    folder: PathBuf,
    profile: Profile,
    mode: Mode,
    /// Folders `asset_import` may read from (`--allow-import`).
    import_folders: Vec<PathBuf>,
    /// `--check`: report whether serving would work, and how to add it to
    /// clients, instead of serving.
    check: bool,
}

const USAGE: &str = "usage: axcenity-editor --mcp [--check] <project folder> [--profile lite|standard|full] [--mode read|edit|full] [--allow-import <folder>]...";

fn parse_options(args: impl Iterator<Item = std::ffi::OsString>) -> Result<Options, String> {
    let mut folder = None;
    let mut profile = Profile::Standard;
    let mut mode = Mode::Edit;
    let mut import_folders = Vec::new();
    let mut check = false;
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        if arg == "--profile" {
            let name = args
                .next()
                .ok_or("--profile needs lite, standard or full")?;
            profile = name
                .to_str()
                .and_then(Profile::from_name)
                .ok_or_else(|| format!("unknown profile {}", name.to_string_lossy()))?;
        } else if arg == "--mode" {
            let name = args.next().ok_or("--mode needs read, edit or full")?;
            mode = name
                .to_str()
                .and_then(Mode::from_name)
                .ok_or_else(|| format!("unknown mode {}", name.to_string_lossy()))?;
        } else if arg == "--allow-import" {
            import_folders.push(PathBuf::from(
                args.next().ok_or("--allow-import needs a folder")?,
            ));
        } else if arg == "--check" {
            check = true;
        } else if arg == "--project" {
            folder = Some(PathBuf::from(
                args.next().ok_or("--project needs a folder")?,
            ));
        } else if folder.is_none() && !arg.to_string_lossy().starts_with("--") {
            folder = Some(PathBuf::from(arg));
        } else {
            return Err(format!("unknown option {}", arg.to_string_lossy()));
        }
    }
    let folder = folder.ok_or("no project folder")?;
    Ok(Options {
        folder,
        profile,
        mode,
        import_folders,
        check,
    })
}

/// `axcenity-editor --mcp <project> [--profile …]`: opens the project
/// without a window and serves the tools on stdin and stdout until the
/// client closes stdin. Returns whether it ran cleanly. Messages go to
/// stderr only: stdout belongs to the protocol.
pub(crate) fn run_from_command_line(args: impl Iterator<Item = std::ffi::OsString>) -> bool {
    let options = match parse_options(args) {
        Ok(options) => options,
        Err(error) => {
            eprintln!("{error}\n{USAGE}");
            return false;
        }
    };
    if options.check {
        let (report, ok) = check::report(&options, &check::this_binary());
        println!("{report}");
        return ok;
    }
    let hello = live::Hello {
        profile: options.profile,
        mode: options.mode,
        import_folders: options
            .import_folders
            .iter()
            .map(|folder| std::fs::canonicalize(folder).unwrap_or_else(|_| folder.clone()))
            .collect(),
        probe: false,
    };
    match live::relay(&options.folder, &hello) {
        live::Relayed::Done(ok) => return ok,
        live::Relayed::NoEditor => {}
        live::Relayed::Refused(why) => eprintln!(
            "axcenity-editor --mcp: the open editor refused the connection ({why}); \
             serving the project without it"
        ),
    }
    let app = EditorApp::new(Some(&options.folder));
    if app.project.is_none() {
        eprintln!(
            "axcenity-editor --mcp: could not open the project in {}",
            options.folder.display()
        );
        for line in app.output.lines() {
            eprintln!("{}", line.text);
        }
        return false;
    }
    let folder = options.folder.clone();
    let _mark = live::HeadlessMark::new(&folder);
    let mut headless = Headless::new(app, options.mode);
    headless.session.import_folders = options.import_folders;
    let (jobs, calls) = mpsc::channel::<Job>();
    let server = axcenity_mcp::serve_stdio(setup(options.profile, None), jobs);
    for job in calls {
        // An editor that opened the project since takes AI connections:
        // two copies writing would overwrite each other.
        let outcome = if job.call.tool.class != Class::Read && live::editor_listening(&folder) {
            Err(ToolError::new(
                Code::Busy,
                "the editor has this project open and takes AI connections",
            )
            .with_fix("restart this MCP server so it works in the editor window"))
        } else {
            run(
                &mut headless.host(),
                job.call.tool,
                &job.call.args,
                job.call.client.as_deref(),
            )
        };
        job.answer_with(outcome, headless.session.png.take());
    }
    match server.join() {
        Ok(Ok(())) => true,
        Ok(Err(error)) => {
            eprintln!("axcenity-editor --mcp: {error}");
            false
        }
        Err(_) => false,
    }
}
