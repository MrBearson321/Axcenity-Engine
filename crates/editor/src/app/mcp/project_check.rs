//! `project_check`: the editor's own checks in one pass, worst first: the
//! Asset Check (missing files, bad references, files that won't load) and
//! every script's syntax and wrong-side uses.

use axcenity_mcp::{Args, Class, Kind, Outcome, Param, Profile, ToolError, ToolSpec};

use super::script_write::{check, warnings};
use super::{Host, LIMIT};

pub const SPEC: ToolSpec = ToolSpec {
    name: "project_check",
    title: "Check project",
    summary: "Find problems: broken asset files and references, script syntax errors, wrong-side APIs.",
    profile: Profile::Lite,
    class: Class::Read,
    idempotent: true,
    params: &[
        Param::optional(
            "scope",
            Kind::Enum(&["all", "assets", "scripts"]),
            "default all",
        ),
        LIMIT,
    ],
};

pub fn call(host: &mut Host, args: &Args) -> Outcome {
    let app = &host.app;
    let project = app
        .project
        .as_ref()
        .ok_or_else(|| ToolError::not_found("no project is open"))?;
    let scope = args.str("scope").unwrap_or("all");
    let mut errors = Vec::new();
    let mut warned = Vec::new();
    if scope != "assets" {
        for script in &project.scripts {
            let path = &script.path;
            if axcenity_project::is_block_script(path)
                || axcenity_blocks::ui_graph::is_ui_graph(path)
            {
                continue;
            }
            let source = app
                .tabs
                .get(path)
                .map_or(script.source.as_str(), |tab| tab.text.as_str());
            if let Some(error) = check(path, source) {
                errors.push(error);
            }
            warned.extend(warnings(path, source));
        }
    }
    if scope != "scripts" {
        let report = axcenity_project::check_assets_in_level(
            &project.folder,
            &app.world,
            project.level.as_deref(),
        );
        for finding in report.findings {
            let mut line = format!(
                "error {} {}: {}",
                finding.kind.id(),
                finding.owner,
                finding.message
            );
            if !finding.path.is_empty() {
                line.push_str(&format!(" ({})", finding.path));
            }
            errors.push(line);
        }
    }
    let total = errors.len() + warned.len();
    if total == 0 {
        return Ok(format!("no problems found ({scope})"));
    }
    let limit = args.limit(40, 200);
    let mut out = format!(
        "{total} problem{} ({} error{}, {} warning{})",
        if total == 1 { "" } else { "s" },
        errors.len(),
        if errors.len() == 1 { "" } else { "s" },
        warned.len(),
        if warned.len() == 1 { "" } else { "s" },
    );
    for line in errors.iter().chain(&warned).take(limit) {
        out.push('\n');
        out.push_str(line);
    }
    if total > limit {
        out.push_str(&format!(
            "\n… {} more; raise limit or narrow scope",
            total - limit
        ));
    }
    Ok(out)
}
