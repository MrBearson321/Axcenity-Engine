//! `style_edit`: the project's CSS stylesheets, by selector. `list` shows
//! each stylesheet's simple selectors; `get` one selector's rules (with
//! their states, such as `:hover`); `set` changes declarations in place,
//! adds new ones to the rule (or a new rule, or a new `ui.css` when the
//! project has no stylesheet, as the Style section does) and takes away
//! those set to `null`. A change goes into the stylesheet's tab, unsaved
//! like a typed one (`project_save` writes it), restyles the UI at once,
//! and is one undo step.

use axcenity_mcp::{Args, Class, Code, Kind, Outcome, Param, Profile, ToolError, ToolSpec};
use serde_json::Value as Json;

use super::Host;
use super::refs::quoted;
use crate::history::TextChange;

pub const SPEC: ToolSpec = ToolSpec {
    name: "style_edit",
    title: "Edit stylesheets",
    summary: "List CSS selectors, show a selector's rules, or set and remove its declarations.",
    profile: Profile::Full,
    class: Class::Modify,
    idempotent: true,
    params: &[
        Param::optional(
            "action",
            Kind::Enum(&["list", "get", "set"]),
            "default list",
        ),
        Param::optional("selector", Kind::String, "e.g. #play, .card, #play:hover"),
        Param::optional(
            "props",
            Kind::Object,
            "set: {\"background-color\":\"#222\"}; null removes",
        ),
    ],
};

pub fn call(host: &mut Host, args: &Args) -> Outcome {
    let Some(project) = host.app.project.as_ref() else {
        return Err(ToolError::not_found("no project is open"));
    };
    let action = args.str("action").unwrap_or("list");
    let sheets = project.stylesheets.clone();
    if action == "list" {
        if sheets.is_empty() {
            return Ok("no stylesheets · set makes ui.css".to_owned());
        }
        let mut out = String::new();
        for sheet in &sheets {
            let sets = axcenity_ui::rule_sets(sheet);
            out.push_str(&format!(
                "{} · {} selector{}",
                sheet.path(),
                sets.len(),
                if sets.len() == 1 { "" } else { "s" }
            ));
            for set in sets.iter().take(60) {
                let states: Vec<&str> = set
                    .rules
                    .iter()
                    .map(|(suffix, _)| suffix.as_str())
                    .filter(|suffix| !suffix.is_empty())
                    .collect();
                out.push_str(&format!("\n{}", set.selector));
                if !states.is_empty() {
                    out.push_str(&format!(" ({})", states.join(" ")));
                }
            }
            out.push('\n');
        }
        return Ok(out.trim_end().to_owned());
    }
    let Some(selector) = args.str("selector").map(str::trim) else {
        return Err(ToolError::bad_args(format!("{action} needs selector")));
    };
    if action == "get" {
        let base = &selector[..selector.find(':').unwrap_or(selector.len())];
        let mut out = String::new();
        for sheet in &sheets {
            for set in axcenity_ui::rule_sets(sheet)
                .into_iter()
                .filter(|set| set.selector == base)
            {
                for (suffix, declarations) in &set.rules {
                    out.push_str(&format!("{}{suffix} {{ ", set.selector));
                    for (name, value) in declarations {
                        out.push_str(&format!("{name}: {value}; "));
                    }
                    out.push_str(&format!("}} in {}\n", sheet.path()));
                }
            }
        }
        if out.is_empty() {
            return Err(
                ToolError::not_found(format!("no rule for {}", quoted(selector)))
                    .with_fix("action list shows the selectors; set makes a rule"),
            );
        }
        return Ok(out.trim_end().to_owned());
    }
    if host.app.playing() {
        return Err(ToolError::new(Code::Busy, "the game is playing")
            .with_fix("stop Play in the editor, then try again"));
    }
    let Some(Json::Object(props)) = args.get("props") else {
        return Err(ToolError::bad_args("set needs props"));
    };
    if props.is_empty() {
        return Err(ToolError::bad_args("props is empty"));
    }
    if selector.is_empty() || selector.contains(['{', '}', ';']) {
        return Err(ToolError::new(
            Code::BadValue,
            format!("{} isn't a selector", quoted(selector)),
        ));
    }
    let mut changes: Vec<TextChange> = Vec::new();
    let mut done = Vec::new();
    for (property, value) in props {
        if property.is_empty()
            || !property
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-')
        {
            return Err(ToolError::new(
                Code::BadValue,
                format!("{} isn't a CSS property", quoted(property)),
            ));
        }
        let value = match value {
            Json::Null => None,
            Json::String(text) => Some(text.trim().to_owned()),
            Json::Number(number) => Some(number.to_string()),
            _ => {
                return Err(ToolError::new(
                    Code::BadValue,
                    format!("{property}'s value is CSS text, such as \"12px\""),
                ));
            }
        };
        if value
            .as_deref()
            .is_some_and(|text| text.is_empty() || text.contains(['{', '}', ';']))
        {
            return Err(ToolError::new(
                Code::BadValue,
                format!(
                    "{} isn't a CSS value",
                    quoted(value.as_deref().unwrap_or_default())
                ),
            ));
        }
        let stylesheets = host
            .app
            .project
            .as_ref()
            .map(|p| p.stylesheets.clone())
            .unwrap_or_default();
        let written = axcenity_ui::rule_declaration(&stylesheets, selector, property);
        let change = match (written, value) {
            (Some(declaration), value) => {
                let Some(source) = host.app.stylesheet_text(&declaration.path) else {
                    continue;
                };
                let updated = match &value {
                    Some(value) => format!(
                        "{}{value}{}",
                        &source[..declaration.span.start],
                        &source[declaration.span.end..]
                    ),
                    None => without_declaration(&source, property, declaration.span.clone()),
                };
                host.app.set_stylesheet_text(&declaration.path, &updated);
                done.push(match value {
                    Some(value) => format!("{property}: {value}"),
                    None => format!("{property} removed"),
                });
                TextChange {
                    path: declaration.path,
                    before: Some(source),
                    after: Some(updated),
                }
            }
            (None, Some(value)) => {
                let Some(change) = host.app.add_style_text(selector, property, &value, false)
                else {
                    return Err(ToolError::internal("could not add it to a stylesheet"));
                };
                done.push(format!("{property}: {value}"));
                change
            }
            (None, None) => continue,
        };
        // One change per file: the first text before, the last after.
        match changes.iter_mut().find(|known| known.path == change.path) {
            Some(known) => known.after = change.after,
            None => changes.push(change),
        }
    }
    if changes.is_empty() {
        return Ok("nothing changed".to_owned());
    }
    let paths: Vec<String> = changes.iter().map(|change| change.path.clone()).collect();
    let undo = format!("AI ({}): Style {selector}", host.session.client);
    let app = &mut host.app;
    app.history
        .record_asset(&mut app.world, undo.clone(), changes, Vec::new());
    app.history.seal();
    host.session.last_undo = Some(undo.clone());
    Ok(format!(
        "{selector} {{ {} }} in {} (unsaved; project_save writes it) · undo {}",
        done.join("; "),
        paths.join(", "),
        quoted(&undo)
    ))
}

/// `source` without the declaration of `property` whose value is at `span`.
fn without_declaration(source: &str, property: &str, span: std::ops::Range<usize>) -> String {
    let start = source[..span.start].rfind(property).unwrap_or(span.start);
    // Back to the line's start, if only spaces are before it.
    let line_start = source[..start].rfind('\n').map_or(0, |at| at + 1);
    let start = if source[line_start..start].trim().is_empty() {
        line_start
    } else {
        start
    };
    let rest = &source[span.end..];
    let mut end = span.end + rest.find([';', '}']).unwrap_or(0);
    if source[end..].starts_with(';') {
        end += 1;
    }
    if source[end..].starts_with('\n') && start == line_start {
        end += 1;
    }
    format!("{}{}", &source[..start], &source[end..])
}
