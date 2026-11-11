//! `level`: the Levels panel's actions (`app/levels.rs`). `list` names
//! every level, the start one and the open one; `create`, `duplicate`,
//! `rename` and `delete` change level files as one undo step each;
//! `set_start` makes a level the start level; `open` opens a level in
//! place of the one being edited. Opening and Set as Start want the
//! project saved first (the panel saves or asks); here they say so
//! instead, so `project_save` stays the one tool that saves.

use axcenity_mcp::{Args, Class, Code, Kind, Outcome, Param, Profile, ToolError, ToolSpec};
use axcenity_project as project;

use super::ops::{checkpoint, confirm};
use super::refs::quoted;
use super::{CONFIRM, Host};

pub const SPEC: ToolSpec = ToolSpec {
    name: "level",
    title: "Levels",
    summary: "List, create, duplicate, rename, delete or open levels, or set the start level.",
    profile: Profile::Full,
    class: Class::Modify,
    idempotent: false,
    params: &[
        Param::optional(
            "action",
            Kind::Enum(&[
                "list",
                "create",
                "duplicate",
                "rename",
                "delete",
                "open",
                "set_start",
            ]),
            "default list",
        ),
        Param::optional("name", Kind::String, "the level"),
        Param::optional("to", Kind::String, "new name (rename, duplicate)"),
        CONFIRM,
    ],
};

pub fn call(host: &mut Host, args: &Args) -> Outcome {
    let Some(project) = host.app.project.as_ref() else {
        return Err(ToolError::not_found("no project is open"));
    };
    let folder = project.folder.clone();
    let start = project.start_level.clone();
    let open = project.level.clone().unwrap_or_else(|| start.clone());
    let others = project::level_names(&folder);
    let action = args.str("action").unwrap_or("list");
    if action == "list" {
        let mut out = format!(
            "{} level{}",
            others.len() + 1,
            if others.is_empty() { "" } else { "s" }
        );
        for name in std::iter::once(&start).chain(&others) {
            out.push_str(&format!("\n{name}"));
            if *name == start {
                out.push_str(" · start");
            }
            if *name == open {
                out.push_str(" · open");
            }
        }
        return Ok(out);
    }
    if host.app.playing() {
        return Err(ToolError::new(Code::Busy, "the game is playing")
            .with_fix("stop Play in the editor, then try again"));
    }
    let names = || {
        std::iter::once(start.as_str())
            .chain(others.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join(", ")
    };
    // The level `name` names, as it's spelled.
    let level = |name: Option<&str>| -> Result<String, ToolError> {
        let Some(name) = name else {
            return Err(ToolError::bad_args(format!("{action} needs name"))
                .with_fix(format!("levels: {}", names())));
        };
        std::iter::once(&start)
            .chain(&others)
            .find(|level| level.eq_ignore_ascii_case(name))
            .cloned()
            .ok_or_else(|| {
                ToolError::not_found(format!("no level {}", quoted(name)))
                    .with_fix(format!("levels: {}", names()))
            })
    };
    // A name a new level may have.
    let fresh = |name: &str| -> Result<(), ToolError> {
        project::check_level_name(name, &start, &others)
            .map_err(|why| ToolError::new(Code::BadValue, why))
    };
    let unsaved = |what: &str| {
        ToolError::new(
            Code::Locked,
            format!("the project has unsaved changes; {what}"),
        )
        .with_fix("call project_save first")
    };
    let client = host.session.client.clone();
    let undo = |what: String| format!("AI ({client}): {what}");
    let revision = host.app.history.revision();
    let label;
    let done;
    match action {
        "create" => {
            let name = args
                .str("name")
                .map_or_else(|| host.app.free_level_name("Level"), str::to_owned);
            fresh(&name)?;
            label = undo(format!("New level {name}"));
            checkpoint(host, &folder);
            host.app.change_level_files(
                label.clone(),
                vec![(project::level_path(&name), Some(project::new_level_text()))],
            );
            done = format!("made level {name} (a baseplate); open it to build in it");
        }
        "duplicate" => {
            let source = level(args.str("name"))?;
            let name = args.str("to").map_or_else(
                || host.app.free_level_name(&format!("{source} copy")),
                str::to_owned,
            );
            fresh(&name)?;
            let text = host
                .app
                .level_text(&source)
                .ok_or_else(|| ToolError::internal(format!("could not read the level {source}")))?;
            label = undo(format!("Duplicate level {source}"));
            checkpoint(host, &folder);
            host.app.change_level_files(
                label.clone(),
                vec![(project::level_path(&name), Some(text))],
            );
            done = format!("copied {source} as saved to {name}");
        }
        "rename" => {
            let old = level(args.str("name"))?;
            let Some(new) = args.str("to") else {
                return Err(ToolError::bad_args("rename needs to: the new name"));
            };
            if old == open && old != start {
                return Err(ToolError::new(Code::Locked, format!("{old} is open"))
                    .with_fix("open another level first"));
            }
            let rest: Vec<String> = others.iter().filter(|o| **o != old).cloned().collect();
            let start_now = if old == start { "" } else { start.as_str() };
            project::check_level_name(new, start_now, &rest)
                .map_err(|why| ToolError::new(Code::BadValue, why))?;
            label = undo(format!("Rename level {old} to {new}"));
            checkpoint(host, &folder);
            host.app.rename_level(&old, new);
            done = format!("renamed {old} to {new}");
        }
        "delete" => {
            let name = level(args.str("name"))?;
            if name == start || name == open {
                return Err(ToolError::new(
                    Code::NotAllowed,
                    format!(
                        "{name} is the {} level",
                        if name == start { "start" } else { "open" }
                    ),
                ));
            }
            confirm(
                host,
                &SPEC,
                args,
                &format!(
                    "deletes the level {name} and everything in it (undo brings the file back)"
                ),
            )?;
            label = undo(format!("Delete level {name}"));
            checkpoint(host, &folder);
            host.app
                .change_level_files(label.clone(), vec![(project::level_path(&name), None)]);
            done = format!("deleted level {name}");
        }
        "set_start" => {
            let name = level(args.str("name"))?;
            if name == start {
                return Ok(format!("{name} is already the start level"));
            }
            if host.app.unsaved() {
                return Err(unsaved("Set as Start saves the project first"));
            }
            label = undo(format!("Set {name} as the start level"));
            checkpoint(host, &folder);
            host.app.set_start_level(&name);
            done = format!("{name} is the start level (in world.json); {start} moved to levels/");
        }
        "open" => {
            let name = level(args.str("name"))?;
            if name == open {
                return Ok(format!("{name} is already open"));
            }
            if host.app.unsaved() {
                return Err(unsaved("opening another level would lose them"));
            }
            let level = (name != start).then(|| name.clone());
            host.app.open_in_level(&folder, level);
            let opened = host.app.project.as_ref().map(|project| {
                project
                    .level
                    .clone()
                    .unwrap_or_else(|| project.start_level.clone())
            });
            if opened.as_deref() != Some(name.as_str()) {
                return Err(ToolError::internal(format!("{name} didn't open")));
            }
            // A new world: earlier IDs and undo steps don't carry over.
            host.session.last_undo = None;
            return Ok(format!(
                "opened {name}; the undo history starts again and earlier IDs no longer hold"
            ));
        }
        _ => return Err(ToolError::bad_args(format!("unknown action {action}"))),
    }
    if host.app.history.revision() == revision {
        return Err(ToolError::internal(format!("{action} changed nothing")));
    }
    host.app.history.relabel_top(label.clone());
    host.app.history.seal();
    host.session.last_undo = Some(label.clone());
    Ok(format!("{done} · undo {}", quoted(&label)))
}
