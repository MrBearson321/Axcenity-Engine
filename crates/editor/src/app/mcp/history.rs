//! `history`: the undo steps, newest first, and undoing or redoing this
//! client's own. A step someone else made (the user, in the live editor,
//! or another client) stays: the user can undo anything in the editor.

use axcenity_mcp::{Args, Class, Code, Kind, Outcome, Param, Profile, ToolError, ToolSpec};

use super::Host;
use super::refs::quoted;

pub const SPEC: ToolSpec = ToolSpec {
    name: "history",
    title: "Undo history",
    summary: "List the undo steps, or undo or redo this client's latest ones.",
    profile: Profile::Lite,
    class: Class::Modify,
    idempotent: false,
    params: &[
        Param::optional(
            "action",
            Kind::Enum(&["list", "undo", "redo"]),
            "default list",
        ),
        Param::optional(
            "steps",
            Kind::Integer,
            "how many to undo or redo, default 1",
        ),
    ],
};

/// The most steps one call lists, or undoes.
const MAX_STEPS: usize = 20;

pub fn call(host: &mut Host, args: &Args) -> Outcome {
    let steps = args.u64("steps").unwrap_or(1).clamp(1, MAX_STEPS as u64) as usize;
    match args.str("action").unwrap_or("list") {
        "undo" => step(host, steps, true),
        "redo" => step(host, steps, false),
        _ => Ok(list(host)),
    }
}

fn list(host: &Host) -> String {
    let history = &host.app.history;
    let undo = history.undo_labels();
    let redo = history.redo_labels();
    let mut text = format!(
        "{} step{} to undo{}",
        undo.len(),
        if undo.len() == 1 { "" } else { "s" },
        if history.is_saved() {
            " · saved"
        } else {
            " · unsaved changes"
        }
    );
    for label in undo.iter().take(MAX_STEPS) {
        text.push_str(&format!("\n  {}", quoted(label)));
    }
    if undo.len() > MAX_STEPS {
        text.push_str(&format!("\n  … {} older", undo.len() - MAX_STEPS));
    }
    if !redo.is_empty() {
        let next: Vec<String> = redo.iter().take(5).map(|label| quoted(label)).collect();
        text.push_str(&format!("\nredo: {}", next.join(", ")));
    }
    text
}

/// Undoes (or redoes) `count` steps, each only if it's this client's.
fn step(host: &mut Host, count: usize, undo: bool) -> Outcome {
    let mine = format!("AI ({}): ", host.session.client);
    let mut done = Vec::new();
    for _ in 0..count {
        let history = &host.app.history;
        let top = if undo {
            history.undo_label()
        } else {
            history.redo_label()
        };
        let Some(label) = top.map(str::to_owned) else {
            break;
        };
        if !label.starts_with(&mine) {
            if done.is_empty() {
                return Err(ToolError::new(
                    Code::NotAllowed,
                    format!(
                        "the step to {} is {}, which this client didn't make",
                        if undo { "undo" } else { "redo" },
                        quoted(&label)
                    ),
                )
                .with_fix("only the user can undo it, in the editor"));
            }
            break;
        }
        if undo {
            host.app.undo();
        } else {
            host.app.redo();
        }
        done.push(quoted(&label));
    }
    if done.is_empty() {
        return Err(ToolError::new(
            Code::NotFound,
            format!("nothing to {}", if undo { "undo" } else { "redo" }),
        ));
    }
    Ok(format!(
        "{} {}",
        if undo { "undid" } else { "redid" },
        done.join(", ")
    ))
}
