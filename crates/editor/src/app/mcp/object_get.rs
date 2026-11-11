//! `object_get`: what objects are and their properties.
//!
//! `min` is the header line (ID, class, name, parent, path); `std`, the
//! default, adds the properties that differ from the class's defaults, one
//! a line; `full` shows every property, attributes, tags and children. A
//! `props` list narrows any level to those properties.

use axcenity_core::{InstanceId, World};
use axcenity_mcp::{Args, Class, Code, Detail, Kind, Outcome, Param, Profile, ToolError, ToolSpec};

use super::Host;
use super::refs::{id_text, label, path, quoted, resolve};
use super::values::{changed_properties, show_in, unknown_property};
use super::{DETAIL, ID, IDS};

pub const SPEC: ToolSpec = ToolSpec {
    name: "object_get",
    title: "Get objects",
    summary: "Objects' class, place and properties (by default only the non-default ones).",
    profile: Profile::Lite,
    class: Class::Read,
    idempotent: true,
    params: &[
        ID,
        IDS,
        Param::optional("props", Kind::Strings, "only these properties"),
        DETAIL,
    ],
};

/// The most objects one call reads.
const MAX_OBJECTS: usize = 50;

pub fn call(host: &mut Host, args: &Args) -> Outcome {
    let world = &host.app.world;
    let mut names: Vec<&str> = args.strings("ids").unwrap_or_default();
    if let Some(id) = args.str("id") {
        names.insert(0, id);
    }
    if names.is_empty() {
        return Err(ToolError::bad_args("object_get needs id or ids")
            .with_fix("give an ID (\"17\") or path (\"Workspace/Door\")"));
    }
    if names.len() > MAX_OBJECTS {
        return Err(ToolError::new(
            Code::TooLarge,
            format!(
                "{} objects asked for; at most {MAX_OBJECTS} at a time",
                names.len()
            ),
        ));
    }
    let ids = names
        .iter()
        .map(|name| resolve(world, name))
        .collect::<Result<Vec<_>, _>>()?;
    let detail = args.detail(Detail::Std);
    let wanted = args.strings("props");
    let mut parts = Vec::new();
    for id in ids {
        parts.push(describe(world, id, detail, wanted.as_deref())?);
    }
    Ok(parts.join("\n"))
}

fn describe(
    world: &World,
    id: InstanceId,
    detail: Detail,
    wanted: Option<&[&str]>,
) -> Result<String, ToolError> {
    let instance = world
        .get(id)
        .ok_or_else(|| ToolError::internal("object vanished"))?;
    let mut text = label(world, id);
    if let Some(parent) = instance.parent() {
        text.push_str(&format!(" in {}", id_text(parent)));
    }
    text.push_str(&format!(" · {}", path(world, id)));
    let class = instance.class();
    let properties: Vec<(&str, axcenity_core::Value)> = match wanted {
        Some(names) => {
            let mut list = Vec::new();
            for &name in names {
                match instance.property(name) {
                    Some(value) => list.push((name, value.clone())),
                    // Name and ClassName aren't stored properties but are
                    // asked for as if they were.
                    None if name == "Name" || name == "ClassName" || name == "Parent" => {}
                    None => {
                        return Err(unknown_property(
                            class,
                            name,
                            instance.properties().map(|(n, _)| n),
                        ));
                    }
                }
            }
            list
        }
        None => match detail {
            Detail::Min => Vec::new(),
            Detail::Std => changed_properties(world, id),
            Detail::Full => instance.properties().map(|(n, v)| (n, v.clone())).collect(),
        },
    };
    if wanted.is_none() && detail == Detail::Std && properties.is_empty() {
        text.push_str("\n  (all properties are defaults)");
    }
    for (name, value) in &properties {
        text.push_str(&format!("\n  {name} {}", show_in(world, value)));
    }
    if detail == Detail::Full && wanted.is_none() {
        for (name, value) in instance.attributes() {
            text.push_str(&format!(
                "\n  attribute {} {}",
                quoted(name),
                show_in(world, value)
            ));
        }
        if !instance.tags().is_empty() {
            let tags: Vec<String> = instance.tags().iter().map(|t| quoted(t)).collect();
            text.push_str(&format!("\n  tags {}", tags.join(",")));
        }
        let children = instance.children();
        if !children.is_empty() {
            let shown: Vec<String> = children.iter().take(20).map(|&c| label(world, c)).collect();
            text.push_str(&format!(
                "\n  children {}: {}",
                children.len(),
                shown.join(", ")
            ));
            if children.len() > 20 {
                text.push_str(", …");
            }
        }
    } else if detail != Detail::Min && wanted.is_none() {
        let count = instance.children().len();
        if count > 0 {
            text.push_str(&format!("\n  ({count} children)"));
        }
    }
    Ok(text)
}
