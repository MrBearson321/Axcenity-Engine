//! `script_write`: a new script, or a change to one: its whole `text`, or
//! `edits` (exact find and replace, each matching once: the cheap way to
//! change a long file). The file is written at once, as Insert Script and a
//! script tab's Save do, and the change is one undo step that puts the
//! file back. Returns a syntax check and the script's wrong-side warnings.

use axcenity_core::ClassName;
use axcenity_mcp::{Args, Class, Code, Kind, Outcome, Param, Profile, ToolError, ToolSpec};

use super::Host;
use super::ops::checkpoint;
use super::refs::quoted;
use super::script_read::find;
use crate::history::TextChange;

pub const SPEC: ToolSpec = ToolSpec {
    name: "script_write",
    title: "Write a script",
    summary: "Make a script (name, side) or change one (path) by text or exact find/replace edits; checks syntax.",
    profile: Profile::Lite,
    class: Class::Modify,
    idempotent: false,
    params: &[
        Param::optional("path", Kind::String, "script to change"),
        Param::optional("name", Kind::String, "new script's name"),
        Param::optional(
            "side",
            Kind::Enum(&["server", "client", "module"]),
            "new script's side",
        ),
        Param::optional("text", Kind::String, "the whole code"),
        Param::optional(
            "edits",
            Kind::Pairs("find", "replace"),
            "each find matches exactly once",
        ),
    ],
};

pub fn call(host: &mut Host, args: &Args) -> Outcome {
    let Some(folder) = host
        .app
        .project
        .as_ref()
        .map(|project| project.folder.clone())
    else {
        return Err(ToolError::not_found("no project is open"));
    };
    if host.app.playing() {
        return Err(ToolError::new(Code::Busy, "the game is playing")
            .with_fix("stop Play in the editor, then try again"));
    }
    let text = args.str("text");
    let edits = args.pairs("edits", "find", "replace");
    let (path, before, after) = match (args.str("path"), args.str("name")) {
        (Some(_), Some(_)) => {
            return Err(ToolError::bad_args(
                "give path (to change a script) or name (to make one), not both",
            ));
        }
        (None, None) => {
            return Err(
                ToolError::bad_args("script_write needs path or name").with_fix(
                    "path changes a script (script_read lists them); name and side make one",
                ),
            );
        }
        (Some(target), None) => {
            let found = find(host, target).map_err(|error| {
                if error.code == Code::NotFound && text.is_some() && edits.is_none() {
                    // Small models give a new script a path under a part.
                    error.with_fix(
                        "to make a new script give name and side, not path; \
                         scripts go in their service, not under parts",
                    )
                } else {
                    error
                }
            })?;
            if axcenity_project::is_block_script(&found.path)
                || axcenity_blocks::ui_graph::is_ui_graph(&found.path)
            {
                return Err(ToolError::new(
                    Code::Unsupported,
                    format!("{} is blocks or a graph, not code", found.path),
                )
                .with_fix("switch it to Code in the editor first"));
            }
            if host.app.tabs.unsaved_paths().contains(&found.path) {
                return Err(ToolError::new(
                    Code::Locked,
                    format!("{} has unsaved changes in the editor", found.path),
                )
                .with_fix("ask the user to save or close its tab"));
            }
            let after = match (text, &edits) {
                (Some(text), None) => text.to_owned(),
                (None, Some(edits)) => apply_edits(&found.source, edits)?,
                _ => {
                    return Err(ToolError::bad_args(
                        "give text or edits, not both or neither",
                    ));
                }
            };
            (found.path, Some(found.source), after)
        }
        (None, Some(name)) => {
            if edits.is_some() {
                return Err(ToolError::bad_args("a new script takes text, not edits"));
            }
            let class = match args.str("side") {
                Some("client") => ClassName::LocalScript,
                Some("module") => ClassName::ModuleScript,
                Some("server") => ClassName::Script,
                _ => {
                    return Err(ToolError::bad_args(
                        "a new script needs side: server, client or module",
                    ));
                }
            };
            axcenity_project::check_script_name(name).map_err(|reason| {
                ToolError::new(Code::BadValue, format!("name {}: {reason}", quoted(name)))
            })?;
            let path = axcenity_project::script_path(name, class);
            if folder.join(&path).exists() {
                return Err(
                    ToolError::new(Code::BadValue, format!("{path} is already there"))
                        .with_fix("give it as path to change it, or pick another name"),
                );
            }
            (path, None, text.unwrap_or_default().to_owned())
        }
    };
    if before.as_deref() == Some(after.as_str()) {
        // Resending broken code changes nothing: say it's still broken.
        if let Some(error) = check(&path, &after) {
            return Err(ToolError::new(
                Code::BadValue,
                format!("{path} is already like that, so it still has a {error}"),
            )
            .with_fix("change the code at that line; the same text again changes nothing"));
        }
        return Ok(format!("{path} is already like that · nothing changed"));
    }
    checkpoint(host, &folder);
    axcenity_project::write_script(&folder, &path, &after)
        .map_err(|error| ToolError::internal(format!("could not write {path}: {error}")))?;
    // Takes the file in as if another program had saved it: a new script
    // gets its object, and an open tab its text.
    host.app.disk_changed(std::slice::from_ref(&path));
    let file = path.rsplit('/').next().unwrap_or(&path).to_owned();
    let verb = if before.is_none() { "Create" } else { "Edit" };
    let label = format!("AI ({}): {verb} {file}", host.session.client);
    let new = before.is_none();
    let app = &mut host.app;
    app.history.record_files(
        &mut app.world,
        label.clone(),
        vec![TextChange {
            path: path.clone(),
            before,
            after: Some(after.clone()),
        }],
    );
    app.history.seal();
    host.session.last_undo = Some(label.clone());
    let wrote = format!(
        "{} {path} ({} lines)",
        if new { "created" } else { "wrote" },
        after.lines().count(),
    );
    // A syntax error leads, so a model doesn't take the write as done.
    let mut out = match check(&path, &after) {
        None => format!("{wrote} · syntax ok · undo {}", quoted(&label)),
        Some(error) => format!(
            "{error} · the script won't run\n{wrote} · undo {}\n\
             fix: change that line (script_write with path {} and edits)",
            quoted(&label),
            quoted(&path)
        ),
    };
    for warning in warnings(&path, &after) {
        out.push('\n');
        out.push_str(&warning);
    }
    Ok(out)
}

/// `source` with each of `edits` made, or which one didn't match once.
fn apply_edits(source: &str, edits: &[(&str, &str)]) -> Result<String, ToolError> {
    if edits.is_empty() {
        return Err(ToolError::bad_args("edits is empty"));
    }
    let mut text = source.to_owned();
    for (number, (find, replace)) in edits.iter().enumerate() {
        let count = if find.is_empty() {
            0
        } else {
            text.matches(find).count()
        };
        if count != 1 {
            return Err(ToolError::new(
                Code::BadValue,
                format!("edit {}: find matches {count} times, not once", number + 1),
            )
            .with_fix(if count == 0 {
                "copy the text exactly from script_read (without the line numbers); nothing was changed"
            } else {
                "include more of the lines around it so it matches once; nothing was changed"
            }));
        }
        text = text.replacen(find, replace, 1);
    }
    Ok(text)
}

/// `syntax error path:line: …`, or `None` when the code compiles.
pub(super) fn check(path: &str, source: &str) -> Option<String> {
    axcenity_script::syntax_error(path, source).map(|error| format!("syntax error {error}"))
}

/// The wrong-side uses in a Script's or LocalScript's code, a line each.
pub(super) fn warnings(path: &str, source: &str) -> Vec<String> {
    let class = axcenity_project::script_class(path);
    if class == ClassName::ModuleScript {
        return Vec::new();
    }
    let mut lines: Vec<String> = crate::side_warnings::side_warnings(source, class)
        .into_iter()
        .map(|warning| format!("warning {path}:{}: {}", warning.line, warning.message))
        .collect();
    // Scripts sit in their service, not under a part: `script.Parent` is
    // the service, a mistake that compiles and then never fires.
    if let Some(line) = source
        .lines()
        .position(|line| line.contains("script.Parent"))
    {
        let service = match class {
            ClassName::LocalScript => "StarterPlayerScripts",
            _ => "ServerScriptService",
        };
        lines.push(format!(
            "warning {path}:{}: script.Parent is {service} (scripts stay in their service); \
             reach a part by its path, like workspace.Block",
            line + 1
        ));
    }
    lines
}

/// After an undo or redo that may have put a file back: takes in every
/// script file, and every asset file this session wrote, as it is on disk
/// now (new, changed or gone).
pub(super) fn resync_files(host: &mut Host) {
    let Some(project) = &host.app.project else {
        return;
    };
    let mut problems = Vec::new();
    let mut paths = axcenity_project::find_scripts(&project.folder, &mut problems);
    for script in &project.scripts {
        if !paths.contains(&script.path) {
            paths.push(script.path.clone());
        }
    }
    paths.extend(host.session.files.iter().cloned());
    host.app.disk_changed(&paths);
}
