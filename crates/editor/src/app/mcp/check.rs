//! `axcenity-editor --mcp --check <project>`: says whether the server would
//! work for this project, without serving it, and prints the lines to add
//! it to the common clients, with this binary's and the project's full
//! paths filled in (`docs/mcp/README.md`).
//!
//! Each finding is a line starting `ok`, `warn` or `FAIL`; any `FAIL`
//! makes the command exit with 1. It writes nothing to the project but a
//! probe file it deletes again (to know the call log can be written).

use std::path::{Path, PathBuf};

use axcenity_mcp::{Args, Profile, text::estimate_tokens, tools_list_json};

use super::{Host, INSTRUCTIONS, LOG_FILE, Options, TOOLKIT_VERSION, project_info, tools_for};
use crate::app::EditorApp;

/// The report for `options`, and whether nothing failed.
pub(super) fn report(options: &Options, exe: &Path) -> (String, bool) {
    let mut lines = vec!["Axcenity MCP check".to_owned()];
    let mut failed = false;
    let mut note = |level: &str, text: String| {
        failed |= level == "FAIL";
        lines.push(format!("{level:<4} {text}"));
    };
    let folder = std::fs::canonicalize(&options.folder).unwrap_or_else(|_| options.folder.clone());
    let app = EditorApp::new(Some(&folder));
    let opened = app.project.is_some();
    if opened {
        let mut host = Host::new(app, options.mode);
        match project_info::call(&mut host, &Args::default()) {
            Ok(info) => {
                for line in info.lines() {
                    note("ok", line.to_owned());
                }
            }
            Err(error) => note("FAIL", error.to_string()),
        }
        match log_problem(&folder) {
            Some(problem) => note("FAIL", problem),
            None => note(
                "ok",
                format!("calls are logged to {}", folder.join(LOG_FILE).display()),
            ),
        }
    } else {
        let why = app
            .output
            .lines()
            .last()
            .map_or_else(String::new, |line| format!(": {}", line.text));
        note(
            "ok",
            format!(
                "engine {} · toolkit {TOOLKIT_VERSION}",
                env!("CARGO_PKG_VERSION")
            ),
        );
        note(
            "FAIL",
            format!("could not open a project in {}{why}", folder.display()),
        );
    }
    let tools = tools_for(options.profile);
    let tokens = estimate_tokens(&tools_list_json(&tools)) + estimate_tokens(INSTRUCTIONS);
    note(
        "ok",
        format!(
            "profile {}: {} tools, about {tokens} tokens of definitions and instructions",
            options.profile.name(),
            tools.len()
        ),
    );
    for import in &options.import_folders {
        if import.is_dir() {
            note(
                "ok",
                format!("asset_import may copy from {}", import.display()),
            );
        } else {
            note(
                "FAIL",
                format!("--allow-import {}: not a folder", import.display()),
            );
        }
    }
    lines.push(String::new());
    lines.push(client_lines(exe, &folder, options));
    (lines.join("\n"), !failed)
}

/// Why the call log couldn't be written, if it can't: a probe file is made
/// next to it and removed.
fn log_problem(folder: &Path) -> Option<String> {
    let log = folder.join(LOG_FILE);
    let dir = log.parent()?;
    let probe = dir.join(".mcp-check");
    let result = std::fs::create_dir_all(dir).and_then(|()| std::fs::write(&probe, b"check"));
    let _ = std::fs::remove_file(&probe);
    result
        .err()
        .map(|error| format!("can't write the call log in {}: {error}", dir.display()))
}

/// The arguments a client starts the server with: everything the check
/// was given but `--check`, with full paths.
fn server_args(folder: &Path, options: &Options) -> Vec<String> {
    let mut args = vec!["--mcp".to_owned(), folder.display().to_string()];
    if options.profile != Profile::Standard {
        args.extend(["--profile".to_owned(), options.profile.name().to_owned()]);
    }
    if options.mode != axcenity_mcp::Mode::Edit {
        args.extend(["--mode".to_owned(), options.mode.name().to_owned()]);
    }
    for import in &options.import_folders {
        let full = std::fs::canonicalize(import).unwrap_or_else(|_| import.clone());
        args.extend(["--allow-import".to_owned(), full.display().to_string()]);
    }
    args
}

/// A JSON string: quotes and backslashes escaped (TOML basic strings take
/// the same escapes).
fn quote(text: &str) -> String {
    serde_json::to_string(text).unwrap_or_else(|_| format!("\"{text}\""))
}

/// For a shell: quoted only when it needs to be.
fn shell(text: &str) -> String {
    if text
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "-_./:\\".contains(c))
    {
        text.to_owned()
    } else {
        format!("\"{}\"", text.replace('"', "\\\""))
    }
}

/// How to add the server to Claude Code, JSON-config clients, VS Code and
/// Codex, with `exe` and the arguments filled in.
fn client_lines(exe: &Path, folder: &Path, options: &Options) -> String {
    let exe = exe.display().to_string();
    let args = server_args(folder, options);
    let json_args = args
        .iter()
        .map(|arg| quote(arg))
        .collect::<Vec<_>>()
        .join(", ");
    let shell_args = args
        .iter()
        .map(|arg| shell(arg))
        .collect::<Vec<_>>()
        .join(" ");
    let command = quote(&exe);
    [
        "Add it to a client (details: docs/mcp/README.md):".to_owned(),
        String::new(),
        "Claude Code:".to_owned(),
        format!("  claude mcp add axcenity -- {} {shell_args}", shell(&exe)),
        String::new(),
        "Claude Desktop, Cursor, Windsurf, Cline, LM Studio, Gemini CLI (\"mcpServers\" JSON):"
            .to_owned(),
        format!(
            "  {{\"mcpServers\": {{\"axcenity\": {{\"command\": {command}, \"args\": [{json_args}]}}}}}}"
        ),
        String::new(),
        "VS Code (.vscode/mcp.json):".to_owned(),
        format!(
            "  {{\"servers\": {{\"axcenity\": {{\"type\": \"stdio\", \"command\": {command}, \"args\": [{json_args}]}}}}}}"
        ),
        String::new(),
        "Codex (~/.codex/config.toml):".to_owned(),
        "  [mcp_servers.axcenity]".to_owned(),
        format!("  command = {command}"),
        format!("  args = [{json_args}]"),
    ]
    .join("\n")
}

/// The binary clients should start: this one.
pub(super) fn this_binary() -> PathBuf {
    std::env::current_exe().unwrap_or_else(|_| PathBuf::from("axcenity-editor"))
}
