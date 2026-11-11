//! `script_read`: the project's scripts, or one script's code with line
//! numbers (what the editor shows: a tab's text if one is open).

use axcenity_core::{ClassName, InstanceId};
use axcenity_mcp::{
    Args, Class, Code, Kind, Outcome, Param, Profile, ToolError, ToolSpec, closest,
};

use super::Host;
use super::refs::{id_text, quoted, resolve};

pub const SPEC: ToolSpec = ToolSpec {
    name: "script_read",
    title: "Read scripts",
    summary: "List the scripts, or show one's code with line numbers.",
    profile: Profile::Lite,
    class: Class::Read,
    idempotent: true,
    params: &[
        Param::optional(
            "path",
            Kind::String,
            "script file or object; none lists all",
        ),
        Param::optional("from", Kind::Integer, "first line, default 1"),
        Param::optional("to", Kind::Integer, "last line"),
    ],
};

/// The most lines one call shows.
const MAX_LINES: usize = 300;

/// A script of the open project.
pub(super) struct Found {
    pub path: String,
    pub instance: InstanceId,
    pub class: ClassName,
    /// What the editor has for it now: an open tab's text, or the file's.
    pub source: String,
}

/// The script `text` names: its file path, or its Script object by ID or
/// tree path.
pub(super) fn find(host: &Host, text: &str) -> Result<Found, ToolError> {
    let app = &host.app;
    let project = app
        .project
        .as_ref()
        .ok_or_else(|| ToolError::not_found("no project is open"))?;
    let by_path = project.scripts.iter().find(|script| script.path == text);
    let script = match by_path {
        Some(script) => script,
        None => {
            let looks_like_file =
                text.ends_with(".lua") || text.ends_with(".json") || text.starts_with("scripts/");
            let by_object = if looks_like_file {
                None
            } else {
                let id = resolve(&app.world, text)?;
                project.scripts.iter().find(|script| script.instance == id)
            };
            match by_object {
                Some(script) => script,
                None => {
                    let near = closest(
                        text,
                        project.scripts.iter().map(|script| script.path.as_str()),
                        3,
                    );
                    let error =
                        ToolError::new(Code::NotFound, format!("no script {}", quoted(text)));
                    return Err(if near.is_empty() {
                        error.with_fix("script_read with no path lists them")
                    } else {
                        error.with_fix(format!(
                            "did you mean {}?",
                            near.iter()
                                .map(|p| quoted(p))
                                .collect::<Vec<_>>()
                                .join(" or ")
                        ))
                    });
                }
            }
        }
    };
    let source = app
        .tabs
        .get(&script.path)
        .map_or_else(|| script.source.clone(), |tab| tab.text.clone());
    Ok(Found {
        path: script.path.clone(),
        instance: script.instance,
        class: axcenity_project::script_class(&script.path),
        source,
    })
}

pub fn call(host: &mut Host, args: &Args) -> Outcome {
    let Some(text) = args.str("path") else {
        return list(host);
    };
    let found = find(host, text)?;
    let lines: Vec<&str> = found.source.lines().collect();
    let total = lines.len();
    let from = args.u64("from").unwrap_or(1).max(1) as usize;
    let to = args.u64("to").map_or(total, |to| to as usize).min(total);
    let end = to.min(from + MAX_LINES - 1);
    let mut out = format!(
        "{} · {} {} · {total} lines",
        found.path,
        id_text(found.instance),
        found.class.as_str()
    );
    if from > total {
        out.push_str(&format!("\n(from {from} is past the end)"));
        return Ok(out);
    }
    for (index, line) in lines.iter().enumerate().take(end).skip(from - 1) {
        out.push_str(&format!("\n{}| {line}", index + 1));
    }
    if end < to {
        out.push_str(&format!(
            "\n… lines {}-{to}: call again with from {}",
            end + 1,
            end + 1
        ));
    }
    Ok(out)
}

fn list(host: &Host) -> Outcome {
    let project = host
        .app
        .project
        .as_ref()
        .ok_or_else(|| ToolError::not_found("no project is open"))?;
    let mut out = format!("{} scripts", project.scripts.len());
    for script in &project.scripts {
        out.push_str(&format!(
            "\n{} · {} {} · {} lines",
            script.path,
            id_text(script.instance),
            axcenity_project::script_class(&script.path).as_str(),
            script.source.lines().count()
        ));
    }
    Ok(out)
}
