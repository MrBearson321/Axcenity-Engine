//! The server's prompts and resources (`docs/editor/mcp.md`).
//!
//! Two prompts give a fixed recipe instead of relying on the model to plan
//! (architecture §10): `axcenity_build` makes something the user
//! describes, `axcenity_fix` finds and fixes problems. Each names only
//! the tools the server lists, so a `lite` model isn't sent to a tool it
//! doesn't have.
//!
//! The resources mirror read tools at `axcenity://` URIs; reading one runs
//! the tool, so they say exactly what the tool would.

use axcenity_mcp::{Args, PromptArg, PromptSpec, ResourceSpec, ToolSpec};

pub(super) const PROMPTS: &[&PromptSpec] = &[&BUILD, &FIX];

pub(super) const RESOURCES: &[&ResourceSpec] = &[
    &ResourceSpec {
        uri: "axcenity://project",
        name: "project",
        title: "Project",
        description: "The project's name, levels, counts and settings (project_info).",
        tool: "project_info",
    },
    &ResourceSpec {
        uri: "axcenity://scene",
        name: "scene",
        title: "Scene",
        description: "The Workspace tree, two levels deep (scene_inspect).",
        tool: "scene_inspect",
    },
    &ResourceSpec {
        uri: "axcenity://object/{id}",
        name: "object",
        title: "Object",
        description: "One object's class, place and changed properties, by ID or path (object_get).",
        tool: "object_get",
    },
    &ResourceSpec {
        uri: "axcenity://docs/{class}",
        name: "docs",
        title: "Class docs",
        description: "A class's properties and what they mean (docs).",
        tool: "docs",
    },
    &ResourceSpec {
        uri: "axcenity://script/{path}",
        name: "script",
        title: "Script",
        description: "A script's code with line numbers, by file or object (script_read).",
        tool: "script_read",
    },
];

const BUILD: PromptSpec = PromptSpec {
    name: "axcenity_build",
    title: "Build in the scene",
    description: "Step-by-step recipe for building what you describe in the open project.",
    args: &[PromptArg {
        name: "goal",
        about: "what to build, e.g. a small wooden hut with a door",
        required: true,
    }],
    render: build,
};

const FIX: PromptSpec = PromptSpec {
    name: "axcenity_fix",
    title: "Find and fix problems",
    description: "Step-by-step recipe for finding and fixing errors in the open project.",
    args: &[PromptArg {
        name: "problem",
        about: "what's wrong, if you know",
        required: false,
    }],
    render: fix,
};

/// Numbered steps, leaving out those whose tool isn't listed.
fn steps(tools: &[&'static ToolSpec], steps: &[(&str, &str)]) -> String {
    let has = |name: &str| name.is_empty() || tools.iter().any(|tool| tool.name == name);
    steps
        .iter()
        .filter(|(tool, _)| has(tool))
        .enumerate()
        .map(|(n, (_, step))| format!("{}. {step}", n + 1))
        .collect::<Vec<_>>()
        .join("\n")
}

fn build(args: &Args, tools: &[&'static ToolSpec]) -> String {
    let goal = args.str("goal").unwrap_or_default().trim();
    let make = if tools.iter().any(|tool| tool.name == "batch") {
        "Make it: object_create for one object, or one batch for several (create a Model first and use \"$name\" to parent the rest to it)."
    } else {
        "Make it: object_create a Model first, then each part with parent set to the Model's ID."
    };
    format!(
        "Build this in the open Axcenity project: {goal}\n\n\
         Follow these steps, one tool call at a time:\n{}\n\n\
         Sizes are in studs: a door is about 4 wide and 7 tall, a character about 5 tall. \
         Y is up and a part's Position is its centre, so a part of height h sits on the ground at y = h/2. \
         Reuse the IDs results give you instead of searching again.",
        steps(
            tools,
            &[
                (
                    "scene_inspect",
                    "scene_inspect to see what's already there, and where."
                ),
                (
                    "",
                    "Plan the parts: name, size, position and color of each, and one Model to hold them."
                ),
                ("", make),
                (
                    "scene_inspect",
                    "scene_inspect with root set to your Model to check what you made."
                ),
                (
                    "object_set",
                    "Fix anything wrong with object_set (or history undo to take a step back)."
                ),
                (
                    "project_check",
                    "project_check to be sure nothing is broken."
                ),
                (
                    "project_save",
                    "project_save to keep it: it previews first, then call it again with the confirm token."
                ),
            ],
        )
    )
}

fn fix(args: &Args, tools: &[&'static ToolSpec]) -> String {
    let problem = match args.str("problem").map(str::trim) {
        Some(problem) if !problem.is_empty() => format!(" The user says: {problem}"),
        _ => String::new(),
    };
    format!(
        "Find and fix what's wrong in the open Axcenity project.{problem}\n\n\
         Follow these steps, one tool call at a time:\n{}\n\n\
         Change only what the problem needs, and say what you changed: each change is an undo step.",
        steps(
            tools,
            &[
                (
                    "project_check",
                    "project_check: it lists script syntax errors, wrong-side APIs and broken asset files or references."
                ),
                (
                    "game_run",
                    "game_run to play for a few seconds and see the errors and output while the game runs."
                ),
                (
                    "script_read",
                    "For a script error, script_read that script around the line given."
                ),
                (
                    "script_write",
                    "Fix it with script_write and an exact find/replace edit."
                ),
                (
                    "asset_find",
                    "For a missing file, asset_find a replacement and point the property at it with object_set."
                ),
                (
                    "object_get",
                    "For a wrong object, object_get it and fix it with object_set."
                ),
                (
                    "project_check",
                    "Run project_check (and game_run) again; repeat at most three times until it's clean."
                ),
                (
                    "project_save",
                    "project_save to keep the fixes to objects (scripts are saved as they're written)."
                ),
            ],
        )
    )
}
