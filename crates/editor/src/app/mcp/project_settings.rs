//! `project_settings`: project-wide settings that aren't objects. `input`
//! is File ▸ Input… (`input.json`: actions and what they're bound to),
//! `collision` the Collision Groups window (`collision-groups.json`), and
//! `strings` the string table (`strings.json`, saved with the project).
//! Each change goes through the editor's own window code and is one undo
//! step; input and collision groups are written at once, as there.

use axcenity_core::input::{Input, binding_text, parse_binding};
use axcenity_mcp::{Args, Class, Code, Kind, Outcome, Param, Profile, ToolError, ToolSpec};

use super::Host;
use super::ops::checkpoint;
use super::refs::quoted;
use crate::app::collision_groups::GroupEdit;
use crate::app::input_settings::InputEdit;

pub const SPEC: ToolSpec = ToolSpec {
    name: "project_settings",
    title: "Project settings",
    summary: "List or change the input map, collision groups or string table.",
    profile: Profile::Full,
    class: Class::Modify,
    idempotent: false,
    params: &[
        Param::required(
            "setting",
            Kind::Enum(&["input", "collision", "strings"]),
            "",
        ),
        Param::optional(
            "action",
            Kind::Enum(&["list", "add", "remove", "set"]),
            "default list",
        ),
        Param::optional("name", Kind::String, "action, group or string key"),
        Param::optional(
            "input",
            Kind::String,
            "input: a binding, e.g. Space, W,A,S,D, ButtonA",
        ),
        Param::optional("axis", Kind::Bool, "input add: a new action is an axis"),
        Param::optional("other", Kind::String, "collision set: the second group"),
        Param::optional("collide", Kind::Bool, "collision set"),
        Param::optional("value", Kind::String, "strings set"),
        Param::optional("language", Kind::String, "strings; default the first"),
    ],
};

/// The most string keys `list` shows.
const MAX_KEYS: usize = 50;

pub fn call(host: &mut Host, args: &Args) -> Outcome {
    let Some(folder) = host
        .app
        .project
        .as_ref()
        .map(|project| project.folder.clone())
    else {
        return Err(ToolError::not_found("no project is open"));
    };
    let setting = args.str("setting").unwrap_or_default();
    let action = args.str("action").unwrap_or("list");
    if action == "list" {
        return Ok(list(host, setting));
    }
    if host.app.playing() {
        return Err(ToolError::new(Code::Busy, "the game is playing")
            .with_fix("stop Play in the editor, then try again"));
    }
    let Some(name) = args
        .str("name")
        .map(str::trim)
        .filter(|name| !name.is_empty())
    else {
        return Err(ToolError::bad_args(format!("{action} needs name")));
    };
    let name = name.to_owned();
    checkpoint(host, &folder);
    let revision = host.app.history.revision();
    let from = host.app.output.total();
    let done = match (setting, action) {
        ("input", "add") => {
            let exists = host
                .app
                .project
                .as_ref()
                .is_some_and(|project| project.input_map.action(&name).is_some());
            let mut edits = Vec::new();
            if !exists {
                edits.push(InputEdit::AddAction {
                    name: name.clone(),
                    axis: args.bool("axis") == Some(true),
                });
            }
            let input = args.str("input").map(one_input).transpose()?;
            if let Some(input) = &input {
                edits.push(InputEdit::Add {
                    action: name.clone(),
                    input: input.clone(),
                });
            }
            if edits.is_empty() {
                return Err(ToolError::new(
                    Code::BadValue,
                    format!("there's an action named {name} already"),
                )
                .with_fix("give input to bind it to something more"));
            }
            host.app
                .edit_input_map_all(format!("Add input to {name}"), edits);
            match input {
                Some(input) => format!("{name} is bound to {}", binding_text(&[input])),
                None => format!("added action {name}"),
            }
        }
        ("input", "remove") => match args.str("input") {
            Some(text) => {
                let input = one_input(text)?;
                let index = host
                    .app
                    .project
                    .as_ref()
                    .and_then(|project| project.input_map.action(&name))
                    .and_then(|action| action.inputs.iter().position(|known| *known == input))
                    .ok_or_else(|| ToolError::not_found(format!("{name} isn't bound to {text}")))?;
                host.app.edit_input_map(InputEdit::Remove {
                    action: name.clone(),
                    index,
                });
                format!("{name} is no longer bound to {text}")
            }
            None => {
                host.app
                    .edit_input_map(InputEdit::RemoveAction(name.clone()));
                format!("removed action {name}")
            }
        },
        ("collision", "add") => {
            host.app.edit_collision_groups(GroupEdit::Add(name.clone()));
            format!("added collision group {name}")
        }
        ("collision", "remove") => {
            host.app
                .edit_collision_groups(GroupEdit::Remove(name.clone()));
            format!("removed collision group {name}")
        }
        ("collision", "set") => {
            let (Some(other), Some(collide)) = (args.str("other"), args.bool("collide")) else {
                return Err(ToolError::bad_args("collision set needs other and collide"));
            };
            host.app.edit_collision_groups(GroupEdit::SetCollidable(
                name.clone(),
                other.to_owned(),
                collide,
            ));
            format!(
                "{name} and {other} {}",
                if collide { "collide" } else { "don't collide" }
            )
        }
        ("strings", "set" | "remove") => {
            let before = host.app.world.strings().clone();
            let mut table = (*before).clone();
            let language = args
                .str("language")
                .map_or_else(|| table.default_language().to_owned(), str::to_owned);
            if action == "remove" {
                if table.entries.remove(&name).is_none() {
                    return Err(ToolError::not_found(format!("no string {}", quoted(&name))));
                }
            } else {
                let Some(value) = args.str("value") else {
                    return Err(ToolError::bad_args("strings set needs value"));
                };
                if !table.has_language(&language) {
                    table.add_language(&language);
                }
                table.set(&name, &language, value);
            }
            let world = &mut host.app.world;
            world.set_strings(std::sync::Arc::new(table));
            host.app.history.seal();
            let label = format!("Change string {name}");
            host.app
                .history
                .record_strings(&mut host.app.world, String::new(), label, before);
            if action == "remove" {
                format!("removed string {name} (unsaved; project_save writes strings.json)")
            } else {
                format!(
                    "string {name} [{language}] set (unsaved; project_save writes strings.json)"
                )
            }
        }
        _ => {
            return Err(
                ToolError::bad_args(format!("{setting} doesn't {action}")).with_fix(
                    "input: add, remove; collision: add, remove, set; strings: set, remove",
                ),
            );
        }
    };
    if host.app.history.revision() == revision {
        let reason = host
            .app
            .output
            .since(from)
            .iter()
            .rev()
            .find(|line| line.kind == crate::output::OutputKind::Error)
            .map_or_else(|| "nothing changed".to_owned(), |line| line.text.clone());
        return Err(ToolError::new(Code::BadValue, reason));
    }
    let undo = format!(
        "AI ({}): {}",
        host.session.client,
        host.app.history.undo_label().unwrap_or_default()
    );
    host.app.history.relabel_top(undo.clone());
    host.app.history.seal();
    host.session.last_undo = Some(undo.clone());
    Ok(format!("{done} · undo {}", quoted(&undo)))
}

/// One binding: a key, a button, or four keys as an axis.
fn one_input(text: &str) -> Result<Input, ToolError> {
    match parse_binding(text).as_deref() {
        Some([one]) => Ok(one.clone()),
        _ => Err(ToolError::new(Code::BadValue, format!("{} isn't one input", quoted(text)))
            .with_fix("a key (Space, E, LeftShift), a button (ButtonA, MouseButton1), W,A,S,D, Thumbstick1 or MouseMovement")),
    }
}

fn list(host: &Host, setting: &str) -> String {
    let app = &host.app;
    match setting {
        "input" => {
            let Some(project) = &app.project else {
                return String::new();
            };
            let mut out = format!("{} actions", project.input_map.actions.len());
            for action in &project.input_map.actions {
                out.push_str(&format!(
                    "\n{}{}: {}",
                    action.name,
                    if action.axis { " (axis)" } else { "" },
                    action
                        .inputs
                        .iter()
                        .map(Input::name)
                        .collect::<Vec<_>>()
                        .join(" | ")
                ));
            }
            out
        }
        "collision" => {
            let groups = app.world.collision_groups();
            let mut out = format!(
                "{} groups: {}",
                groups.names().len(),
                groups.names().join(", ")
            );
            let apart: Vec<String> = groups.apart().map(|(a, b)| format!("{a}/{b}")).collect();
            if apart.is_empty() {
                out.push_str("\nevery pair collides");
            } else {
                out.push_str(&format!("\ndon't collide: {}", apart.join(", ")));
            }
            out
        }
        _ => {
            let table = app.world.strings();
            let mut out = format!(
                "{} strings · languages {}",
                table.entries.len(),
                if table.languages.is_empty() {
                    "none".to_owned()
                } else {
                    table.languages.join(", ")
                }
            );
            for (key, values) in table.entries.iter().take(MAX_KEYS) {
                let first = values
                    .get(table.default_language())
                    .or_else(|| values.values().next())
                    .cloned()
                    .unwrap_or_default();
                out.push_str(&format!("\n{key}: {}", quoted(&first)));
            }
            if table.entries.len() > MAX_KEYS {
                out.push_str(&format!("\n… {} more", table.entries.len() - MAX_KEYS));
            }
            out
        }
    }
}
