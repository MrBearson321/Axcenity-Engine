//! How tools change the project: [`write`] runs a change as one undo step,
//! [`confirm`] asks first for a destructive one.
//!
//! A write works like a Lua plugin's call (`app/plugins.rs`): the world is
//! copied, the change runs with the world's change log on, and the objects
//! the log names become one undo step (`History::record_edits`). If the
//! change fails part way, the copy is put back, so a failed call changes
//! nothing. The session's first write keeps a version of the project files
//! first (`Before AI session (<client>)` in Version History).

use axcenity_core::World;
use axcenity_mcp::{Allowed, Args, Class, Code, ToolError, ToolSpec, call_key};

use super::Host;
use crate::history::changed_ids;

/// What a write did: its result line, and the undo step's label (`None`
/// when nothing changed).
pub(super) struct Written<T> {
    pub value: T,
    pub undo: Option<String>,
}

impl<T> Written<T> {
    /// ` · undo "AI (…): …"`, or ` · nothing changed`, for result lines.
    pub fn undo_note(&self) -> String {
        match &self.undo {
            Some(label) => format!(" · undo {}", super::refs::quoted(label)),
            None => " · nothing changed".to_owned(),
        }
    }
}

/// Runs `change` on the edit world as one undo step labelled
/// `AI (<client>): <label>`. On an error, the world is put back as it was.
pub(super) fn write<T>(
    host: &mut Host,
    label: &str,
    change: impl FnOnce(&mut World) -> Result<T, ToolError>,
) -> Result<Written<T>, ToolError> {
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
    checkpoint(host, &folder);
    let app = &mut host.app;
    let was_recording = app.world.is_recording_changes();
    app.world.record_changes();
    // Changes from before the call aren't this one's.
    drop(app.world.take_changes());
    let before = app.world.clone();
    let result = change(&mut app.world);
    let changes = app.world.take_changes();
    if !was_recording {
        app.world.stop_recording_changes();
    }
    let value = match result {
        Ok(value) => value,
        Err(error) => {
            app.world = before;
            return Err(error);
        }
    };
    let label = format!("AI ({}): {label}", host.session.client);
    let ids = changed_ids(&changes);
    let recorded =
        app.history
            .record_edits(&mut app.world, label.clone(), (&before, None), &ids, None);
    // The next call is a step of its own.
    app.history.seal();
    let undo = recorded.map(|_| label);
    host.session.last_undo.clone_from(&undo);
    Ok(Written { value, undo })
}

/// Before the session's first write, keeps the project's files in `folder`
/// as a pinned version, `Before AI session (<client>)`.
pub(super) fn checkpoint(host: &mut Host, folder: &std::path::Path) {
    if host.session.checkpointed {
        return;
    }
    let message = format!("Before AI session ({})", host.session.client);
    // Kept even if it fails: the edits are still undoable one by one.
    if let Err(error) = axcenity_project::record_checkpoint(folder, &message) {
        eprintln!("axcenity-editor --mcp: could not keep a version first: {error}");
    }
    host.session.checkpointed = true;
}

/// Lets a destructive call go ahead: always in `full` mode; in `edit`
/// mode only with the confirm token an earlier identical call was given.
/// Without one, nothing happens and the error carries `preview` and a new
/// token. Tokens are bound to the arguments and the history's revision,
/// so one can't be used after the project changed.
pub(super) fn confirm(
    host: &mut Host,
    spec: &ToolSpec,
    args: &Args,
    preview: &str,
) -> Result<(), ToolError> {
    match host.session.mode.allows(Class::Destructive) {
        Allowed::Yes => return Ok(()),
        Allowed::No => return Err(host.session.mode.refusal(spec.name)),
        Allowed::Confirm => {}
    }
    let key = call_key(spec.name, args, host.app.history.revision());
    match args.str("confirm") {
        Some(token) => host.session.confirms.redeem(token, key),
        None => {
            let token = host.session.confirms.issue(key);
            Err(ToolError::new(Code::NeedsConfirm, format!("preview: {preview}"))
                .with_fix(format!("to go ahead, call again with the same arguments and \"confirm\":\"{token}\"")))
        }
    }
}
