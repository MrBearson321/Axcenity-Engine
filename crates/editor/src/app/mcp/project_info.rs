//! `project_info`: the open project in a few lines, the first call an
//! agent makes.

use axcenity_mcp::{Args, Class, Outcome, Profile, ToolError, ToolSpec};

use super::super::EditorApp;
use super::TOOLKIT_VERSION;
use super::refs::quoted;

pub const SPEC: ToolSpec = ToolSpec {
    name: "project_info",
    title: "Project info",
    summary: "The open project: name, levels, what's in it, engine and toolkit versions.",
    profile: Profile::Lite,
    class: Class::Read,
    idempotent: true,
    params: &[],
};

pub fn call(app: &mut EditorApp, _args: &Args) -> Outcome {
    let Some(project) = &app.project else {
        return Err(ToolError::not_found("no project is open"));
    };
    let world = &app.world;
    let root = world.root();
    let objects = world.descendants_iter(root).count();
    let in_workspace = world.descendants_iter(world.workspace()).count();
    let start = &project.start_level;
    let level = project.level.as_deref().unwrap_or(start);
    let mut levels = vec![start.clone()];
    levels.extend(axcenity_project::level_names(&project.folder));
    let mut text = format!(
        "project {} · engine {} · toolkit {TOOLKIT_VERSION}\n",
        quoted(&project.name),
        env!("CARGO_PKG_VERSION"),
    );
    text.push_str(&format!(
        "level {} (start {}) · levels: {}\n",
        quoted(level),
        quoted(start),
        levels.join(", ")
    ));
    text.push_str(&format!(
        "objects {objects} ({in_workspace} in Workspace) · scripts {}\n",
        project.scripts.len()
    ));
    text.push_str("mode read: this server only reads (editing tools come later)");
    Ok(text)
}
