//! `object_set`: the same change to one or many objects (properties, name,
//! parent, attributes, tags), as one undo step. Returns what changed, not
//! the objects. Locked objects are refused unless the change unlocks them;
//! more than [`ASK_OVER`] objects at once asks first.

use axcenity_core::{InstanceId, World};
use axcenity_mcp::{Args, Class, Code, Kind, Outcome, Param, Profile, ToolError, ToolSpec};
use serde_json::{Map, Value as Json};

use super::Host;
use super::ops::{confirm, write};
use super::refs::{label, quoted, resolve};
use super::values::set_props;
use super::{CONFIRM, ID, IDS};
use crate::tree;

pub const SPEC: ToolSpec = ToolSpec {
    name: "object_set",
    title: "Change objects",
    summary: "Change properties, name, parent, attributes or tags of objects, e.g. {\"id\":\"17\",\"props\":{\"Color\":[1,0,0]}}.",
    profile: Profile::Lite,
    class: Class::Modify,
    idempotent: true,
    params: &[
        ID,
        IDS,
        Param::optional("props", Kind::Object, "property name to world.json value"),
        Param::optional("name", Kind::String, "new name"),
        Param::optional("parent", Kind::String, "move into this object (ID or path)"),
        Param::optional(
            "attrs",
            Kind::Object,
            "attribute name to value; null removes",
        ),
        Param::optional("tags_add", Kind::Strings, "tags to add"),
        Param::optional("tags_remove", Kind::Strings, "tags to remove"),
        CONFIRM,
    ],
};

/// The most objects one call changes without asking first.
const ASK_OVER: usize = 50;
/// The most objects one call changes.
const MAX_OBJECTS: usize = 2000;

pub fn call(host: &mut Host, args: &Args) -> Outcome {
    let world = &host.app.world;
    let ids = targets(world, args)?;
    let props = match args.get("props") {
        Some(Json::Object(props)) => props.clone(),
        _ => Map::new(),
    };
    let attrs = match args.get("attrs") {
        Some(Json::Object(attrs)) => attrs.clone(),
        _ => Map::new(),
    };
    let name = args.str("name").map(str::to_owned);
    let parent = args
        .str("parent")
        .map(|text| resolve(world, text))
        .transpose()?;
    let tags_add: Vec<String> = args
        .strings("tags_add")
        .unwrap_or_default()
        .into_iter()
        .map(str::to_owned)
        .collect();
    let tags_remove: Vec<String> = args
        .strings("tags_remove")
        .unwrap_or_default()
        .into_iter()
        .map(str::to_owned)
        .collect();
    if props.is_empty()
        && attrs.is_empty()
        && name.is_none()
        && parent.is_none()
        && tags_add.is_empty()
        && tags_remove.is_empty()
    {
        return Err(ToolError::bad_args("nothing to change")
            .with_fix("give props, name, parent, attrs, tags_add or tags_remove"));
    }
    let unlocking = props.get("Locked") == Some(&Json::Bool(false));
    for &id in &ids {
        check(world, id, unlocking, name.is_some(), parent)?;
    }
    if ids.len() > ASK_OVER {
        let preview = format!(
            "changes {} objects, starting with {}",
            ids.len(),
            label(world, ids[0])
        );
        confirm(host, &SPEC, args, &preview)?;
    }
    let what = match ids.as_slice() {
        [one] => host
            .app
            .world
            .get(*one)
            .map_or_else(String::new, |i| i.name().to_owned()),
        many => format!("{} objects", many.len()),
    };
    let written = write(host, &format!("Change {what}"), |world| {
        let mut changed: Vec<String> = Vec::new();
        let mut note = |names: Vec<String>| {
            for name in names {
                if !changed.contains(&name) {
                    changed.push(name);
                }
            }
        };
        for &id in &ids {
            note(set_props(world, id, &props)?);
            if let Some(name) = &name {
                world
                    .set_name(id, name)
                    .map_err(|error| ToolError::new(Code::BadValue, format!("name: {error}")))?;
                note(vec!["Name".to_owned()]);
            }
            if let Some(parent) = parent {
                tree::check_move(world, id, parent).map_err(|reason| {
                    ToolError::new(
                        Code::NotAllowed,
                        format!("can't move {}: {reason}", label(world, id)),
                    )
                })?;
                world.set_parent(id, Some(parent)).map_err(|error| {
                    ToolError::new(Code::NotAllowed, format!("parent: {error}"))
                })?;
                note(vec!["Parent".to_owned()]);
            }
            for (attribute, json) in &attrs {
                let value = match json {
                    Json::Null => None,
                    json => Some(
                        axcenity_project::json_to_attribute(json).map_err(|message| {
                            ToolError::new(
                                Code::BadValue,
                                format!("attribute {}: {message}", quoted(attribute)),
                            )
                        })?,
                    ),
                };
                world.set_attribute(id, attribute, value).map_err(|error| {
                    ToolError::new(
                        Code::BadValue,
                        format!("attribute {}: {error}", quoted(attribute)),
                    )
                })?;
                note(vec![format!("attribute {}", quoted(attribute))]);
            }
            for tag in &tags_add {
                world.add_tag(id, tag).map_err(|error| {
                    ToolError::new(Code::BadValue, format!("tag {}: {error}", quoted(tag)))
                })?;
                note(vec!["tags".to_owned()]);
            }
            for tag in &tags_remove {
                world.remove_tag(id, tag).map_err(|error| {
                    ToolError::new(Code::BadValue, format!("tag {}: {error}", quoted(tag)))
                })?;
                note(vec!["tags".to_owned()]);
            }
        }
        Ok(changed)
    })?;
    let objects = match ids.as_slice() {
        [one] => label(&host.app.world, *one),
        many => format!("{} objects", many.len()),
    };
    Ok(format!(
        "changed {objects}: {}{}",
        written.value.join(", "),
        written.undo_note()
    ))
}

/// The objects `id` and `ids` name, each once.
pub(super) fn targets(world: &World, args: &Args) -> Result<Vec<InstanceId>, ToolError> {
    let mut names: Vec<&str> = args.strings("ids").unwrap_or_default();
    if let Some(id) = args.str("id") {
        names.insert(0, id);
    }
    if names.is_empty() {
        return Err(ToolError::bad_args("give id or ids")
            .with_fix("an ID (\"17\") or path (\"Workspace/Door\")"));
    }
    if names.len() > MAX_OBJECTS {
        return Err(ToolError::new(
            Code::TooLarge,
            format!("{} objects; at most {MAX_OBJECTS} at a time", names.len()),
        ));
    }
    let mut ids = Vec::new();
    for name in names {
        let id = resolve(world, name)?;
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    Ok(ids)
}

/// Whether `id` may change: not locked (unless this unlocks it), and not a
/// service or a Player when it's renamed or moved.
fn check(
    world: &World,
    id: InstanceId,
    unlocking: bool,
    renames: bool,
    parent: Option<InstanceId>,
) -> Result<(), ToolError> {
    let instance = world
        .get(id)
        .ok_or_else(|| ToolError::internal("object vanished"))?;
    if world.is_locked(id) && !unlocking {
        return Err(
            ToolError::new(Code::Locked, format!("{} is locked", label(world, id)))
                .with_fix("set props {\"Locked\":false} on it (or the locked Model it's in) first"),
        );
    }
    if instance.class() == axcenity_core::ClassName::Player {
        return Err(ToolError::new(
            Code::NotAllowed,
            "Players only exist while the game runs",
        ));
    }
    if (renames || parent.is_some()) && instance.class().is_service() {
        return Err(ToolError::new(
            Code::NotAllowed,
            format!(
                "{} is a service: it can't be renamed or moved",
                label(world, id)
            ),
        ));
    }
    Ok(())
}
