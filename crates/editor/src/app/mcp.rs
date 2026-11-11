//! The MCP server's tools (`docs/editor/mcp.md`): what AI clients such as
//! Claude Code, Cursor or Codex can do in a project, through the same
//! editor code the panels use.
//!
//! `axcenity-editor --mcp <project>` opens the project without a window and
//! serves the tools on stdin and stdout ([`run_from_command_line`]). The
//! `axcenity_mcp` crate speaks the protocol on a thread of its own and hands
//! each call over as a [`Job`]; this thread owns the [`EditorApp`] and runs
//! them one at a time, so tools change the world exactly as the editor does.
//!
//! Each tool is a child module with a `SPEC` and a `call`, listed in
//! [`TOOLS`]. Tools share parameter constants ([`ID`], [`DETAIL`], …) so a
//! name means the same everywhere.

use std::path::PathBuf;
use std::sync::mpsc;

use axcenity_mcp::{Args, Detail, Job, Kind, Outcome, Param, Profile, Setup, ToolError, ToolSpec};

use super::EditorApp;

mod docs;
mod object_get;
mod project_info;
mod refs;
mod scene_inspect;
mod values;

#[cfg(test)]
mod tests;

/// The command-line flag that starts the server.
pub(crate) const FLAG: &str = "--mcp";

/// The toolkit's own version, separate from the engine's: tool names and
/// what their parameters mean only change with its major version.
pub(crate) const TOOLKIT_VERSION: &str = "0.1.0";

/// Sent to clients when they connect. Short: some put it in every prompt.
const INSTRUCTIONS: &str = "Axcenity Engine project tools. Objects are named by \
ID (\"17\", from any result) or path (\"Workspace/House/Door\"). Values use \
world.json form: [x,y,z] vectors, [r,g,b] colors 0-1, enum names as strings. \
Use scene_inspect to find objects, object_get for their properties, docs for \
classes and properties.";

/// A tool: what clients see, and what runs it.
pub(super) struct Tool {
    pub spec: &'static ToolSpec,
    pub call: fn(&mut EditorApp, &Args) -> Outcome,
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
];

// Parameters shared by several tools.
pub(super) const ID: Param = Param::optional(
    "id",
    Kind::String,
    "object ID (\"17\") or path (\"Workspace/Door\")",
);
pub(super) const IDS: Param =
    Param::optional("ids", Kind::Strings, "several objects, same forms as id");
pub(super) const DETAIL: Param =
    Param::optional("detail", Kind::Enum(Detail::NAMES), "how much to return");
pub(super) const LIMIT: Param = Param::optional("limit", Kind::Integer, "most rows to return");
pub(super) const CURSOR: Param = Param::optional(
    "cursor",
    Kind::String,
    "from a result that was cut, to get the next part",
);

/// The tools a server with `profile` lists.
pub(super) fn tools_for(profile: Profile) -> Vec<&'static ToolSpec> {
    TOOLS
        .iter()
        .filter(|tool| profile.shows(tool.spec.profile))
        .map(|tool| tool.spec)
        .collect()
}

/// Runs one call on the editor. An engine panic becomes an `internal` error
/// rather than taking the server down.
pub(super) fn run(app: &mut EditorApp, tool: &ToolSpec, args: &Args) -> Outcome {
    let Some(entry) = TOOLS.iter().find(|entry| entry.spec.name == tool.name) else {
        return Err(ToolError::not_found(format!("no tool {}", tool.name)));
    };
    let call = entry.call;
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| call(app, args))).unwrap_or_else(
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

/// What `--mcp` was given.
#[derive(Debug, PartialEq)]
struct Options {
    folder: PathBuf,
    profile: Profile,
}

const USAGE: &str = "usage: axcenity-editor --mcp <project folder> [--profile lite|standard|full]";

fn parse_options(args: impl Iterator<Item = std::ffi::OsString>) -> Result<Options, String> {
    let mut folder = None;
    let mut profile = Profile::Standard;
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
    Ok(Options { folder, profile })
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
    let mut app = EditorApp::new(Some(&options.folder));
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
    let setup = Setup {
        version: TOOLKIT_VERSION.to_owned(),
        instructions: INSTRUCTIONS.to_owned(),
        tools: tools_for(options.profile),
    };
    let (jobs, calls) = mpsc::channel::<Job>();
    let server = axcenity_mcp::serve_stdio(setup, jobs);
    for job in calls {
        let outcome = run(&mut app, job.call.tool, &job.call.args);
        job.answer(outcome);
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
