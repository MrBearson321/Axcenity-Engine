//! `project_save`: the editor's Save (the world file and every unsaved
//! script, graph and table), which also keeps a version in Version
//! History. Destructive (it overwrites the files), so it asks first in
//! `edit` mode.

use axcenity_mcp::{Args, Class, Code, Outcome, Profile, ToolError, ToolSpec};

use super::ops::confirm;
use super::refs::quoted;
use super::{CONFIRM, Host};

pub const SPEC: ToolSpec = ToolSpec {
    name: "project_save",
    title: "Save project",
    summary: "Save the project's files (Version History keeps a version); previews first.",
    profile: Profile::Lite,
    class: Class::Destructive,
    idempotent: true,
    params: &[CONFIRM],
};

pub fn call(host: &mut Host, args: &Args) -> Outcome {
    let Some(project) = &host.app.project else {
        return Err(ToolError::not_found("no project is open"));
    };
    let name = project.name.clone();
    if host.app.playing() {
        return Err(ToolError::new(Code::Busy, "the game is playing")
            .with_fix("stop Play in the editor, then save"));
    }
    // Nothing to write: say so, with no token to come back with.
    if !host.app.unsaved() {
        return Ok(format!(
            "nothing to save: {} has no changes since it was saved",
            quoted(&name)
        ));
    }
    let steps = host.app.history.undo_labels().len();
    let preview = if host.app.history.is_saved() {
        format!(
            "writes {} again; nothing has changed since it was saved",
            quoted(&name)
        )
    } else {
        format!(
            "overwrites {}'s files with the open project ({steps} undo step{}){}",
            quoted(&name),
            if steps == 1 { "" } else { "s" },
            if host.session.checkpointed {
                "; Version History keeps the files from before this AI session"
            } else {
                ""
            }
        )
    };
    confirm(host, &SPEC, args, &preview)?;
    let errors_before = host.app.output.lines().len();
    if !host.app.save() {
        let reasons: Vec<String> = host
            .app
            .output
            .lines()
            .iter()
            .skip(errors_before)
            .map(|line| line.text.clone())
            .collect();
        return Err(ToolError::new(
            Code::Internal,
            format!("could not save: {}", reasons.join(" ")),
        ));
    }
    Ok(format!(
        "saved {} · a version was kept in Version History",
        quoted(&name)
    ))
}
