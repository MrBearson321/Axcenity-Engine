//! Naming objects in results and finding them from what a model sends.
//!
//! An ID is the arena slot, with the generation after a dot when it isn't
//! 0: `"17"`, `"17.2"`. It's stable while the editor runs, also through
//! delete and undo (deleting only detaches). A path is the names from a
//! service down, joined by `/`: `"Workspace/House/Door"`. Paths survive
//! reopening the project; IDs don't (`world.json` has none).

use axcenity_core::{InstanceId, World};
use axcenity_mcp::{Code, ToolError, closest};

/// `"17"`, or `"17.2"` for generation 2.
pub fn id_text(id: InstanceId) -> String {
    let bits = id.to_bits();
    let (index, generation) = (bits & 0xFFFF_FFFF, bits >> 32);
    if generation == 0 {
        index.to_string()
    } else {
        format!("{index}.{generation}")
    }
}

fn parse_id(text: &str) -> Option<InstanceId> {
    let (index, generation) = match text.split_once('.') {
        Some((index, generation)) => (index, generation),
        None => (text, "0"),
    };
    let index: u32 = index.parse().ok()?;
    let generation: u32 = generation.parse().ok()?;
    Some(InstanceId::from_bits(
        (u64::from(generation) << 32) | u64::from(index),
    ))
}

/// Whether `id` is in the game's tree (not deleted, not detached).
pub fn in_tree(world: &World, id: InstanceId) -> bool {
    id == world.root() || world.is_descendant_of(id, world.root())
}

/// The path of `id` from its service: `Workspace/House/Door`. `game` for
/// the root itself.
pub fn path(world: &World, id: InstanceId) -> String {
    let root = world.root();
    let mut names = Vec::new();
    let mut at = Some(id);
    while let Some(current) = at {
        if current == root {
            break;
        }
        let Some(instance) = world.get(current) else {
            break;
        };
        names.push(instance.name().to_owned());
        at = instance.parent();
    }
    if names.is_empty() {
        return "game".to_owned();
    }
    names.reverse();
    names.join("/")
}

/// `17 Part "Door"`: how every result names an object.
pub fn label(world: &World, id: InstanceId) -> String {
    match world.get(id) {
        Some(instance) => format!(
            "{} {} {}",
            id_text(id),
            instance.class().as_str(),
            quoted(instance.name())
        ),
        None => id_text(id),
    }
}

pub fn quoted(text: &str) -> String {
    serde_json::to_string(text).unwrap_or_else(|_| format!("\"{text}\""))
}

/// The object `text` names: an ID or a path (`game` is the root). Errors
/// say what was found instead.
pub fn resolve(world: &World, text: &str) -> Result<InstanceId, ToolError> {
    let text = text.trim();
    if let Some(id) = parse_id(text) {
        if world.get(id).is_some() && in_tree(world, id) {
            return Ok(id);
        }
        if world.get(id).is_some() {
            return Err(ToolError::new(
                Code::StaleId,
                format!("object {} was deleted", label(world, id)),
            )
            .with_fix("undo brings it back; scene_inspect lists what's there"));
        }
        return Err(
            ToolError::new(Code::StaleId, format!("no object has ID {text}"))
                .with_fix("IDs come from earlier results; scene_inspect lists objects"),
        );
    }
    let root = world.root();
    if text.ends_with(".lua") || text.ends_with(".luau") {
        return Err(script_file(world, text));
    }
    let trimmed = text.trim_matches('/');
    let path = trimmed
        .strip_prefix("game/")
        .or_else(|| (trimmed == "game").then_some(""))
        .unwrap_or(trimmed);
    if path.is_empty() {
        return Ok(root);
    }
    let mut at = root;
    for (depth, name) in path.split('/').enumerate() {
        let Some(instance) = world.get(at) else {
            break;
        };
        let matches: Vec<InstanceId> = instance
            .children()
            .iter()
            .copied()
            .filter(|&child| world.get(child).is_some_and(|c| c.name() == name))
            .collect();
        match matches.as_slice() {
            [one] => at = *one,
            [] => {
                let here = if depth == 0 {
                    "game".to_owned()
                } else {
                    self::path(world, at)
                };
                let names: Vec<&str> = instance
                    .children()
                    .iter()
                    .filter_map(|&child| world.get(child).map(|c| c.name()))
                    .collect();
                let near = closest(name, names.iter().copied(), 3);
                // A bare name (`"Block"`) usually means an object deeper
                // down: name where the ones called exactly that are.
                let elsewhere: Vec<String> = world
                    .descendants_iter(root)
                    .filter(|&id| world.get(id).is_some_and(|c| c.name() == name))
                    .take(3)
                    .map(|id| format!("{} (ID {})", quoted(&self::path(world, id)), id_text(id)))
                    .collect();
                let fix = if !elsewhere.is_empty() {
                    format!("did you mean {}?", elsewhere.join(" or "))
                } else if near.is_empty() {
                    format!("scene_inspect with root \"{here}\" lists what's in it")
                } else {
                    let options: Vec<String> = near
                        .iter()
                        .map(|n| match depth {
                            0 => quoted(n),
                            _ => quoted(&format!("{here}/{n}")),
                        })
                        .collect();
                    format!("did you mean {}?", options.join(" or "))
                };
                return Err(ToolError::not_found(format!(
                    "{here} has nothing called {}",
                    quoted(name)
                ))
                .with_fix(fix));
            }
            several => {
                let options: Vec<String> = several.iter().map(|&id| label(world, id)).collect();
                return Err(ToolError::new(
                    Code::Ambiguous,
                    format!("{} objects are at {}", several.len(), quoted(text)),
                )
                .with_fix(format!("use an ID: {}", options.join(", "))));
            }
        }
    }
    Ok(at)
}

/// The error for a script's file path given as an object: names the
/// script's object, which stays in its service.
fn script_file(world: &World, file: &str) -> ToolError {
    let class = axcenity_project::script_class(file);
    let service = axcenity_project::script_service(world, class);
    let name = axcenity_project::script_name(file);
    let object = world.get(service).and_then(|instance| {
        instance
            .children()
            .iter()
            .copied()
            .find(|&child| world.get(child).is_some_and(|c| c.name() == name))
    });
    let error = ToolError::not_found(format!("{} is a script file, not an object", quoted(file)));
    match object {
        Some(id) => error.with_fix(format!(
            "its object is {} (ID {}); scripts stay in their service, so reach a part from the code (workspace.Block)",
            quoted(&path(world, id)),
            id_text(id)
        )),
        None => error.with_fix("script_read and script_write take script files"),
    }
}

#[cfg(test)]
mod tests {
    use axcenity_core::{ClassName, World};

    use super::*;

    fn add(world: &mut World, class: ClassName, name: &str, parent: InstanceId) -> InstanceId {
        let id = world.create(class).unwrap();
        world.set_name(id, name).unwrap();
        world.set_parent(id, Some(parent)).unwrap();
        id
    }

    pub(crate) fn world_with_a_house() -> (World, InstanceId, InstanceId) {
        let mut world = World::new();
        let workspace = world.workspace();
        let house = add(&mut world, ClassName::Model, "House", workspace);
        let door = add(&mut world, ClassName::Part, "Door", house);
        add(&mut world, ClassName::Part, "Wall", house);
        add(&mut world, ClassName::Part, "Wall", house);
        (world, house, door)
    }

    #[test]
    fn ids_and_paths_name_the_same_object() {
        let (world, house, door) = world_with_a_house();
        assert_eq!(path(&world, door), "Workspace/House/Door");
        assert_eq!(path(&world, world.root()), "game");
        assert_eq!(resolve(&world, &id_text(door)), Ok(door));
        assert_eq!(resolve(&world, "Workspace/House/Door"), Ok(door));
        assert_eq!(resolve(&world, "/game/Workspace/House/"), Ok(house));
        assert_eq!(resolve(&world, "game"), Ok(world.root()));
        assert_eq!(
            label(&world, door),
            format!("{} Part \"Door\"", id_text(door))
        );
    }

    #[test]
    fn a_wrong_path_says_what_is_there() {
        let (world, _, _) = world_with_a_house();
        assert_eq!(
            resolve(&world, "Workspace/Hous/Door")
                .unwrap_err()
                .to_string(),
            "error not_found: Workspace has nothing called \"Hous\"\nfix: did you mean \"Workspace/House\"?"
        );
        let error = resolve(&world, "Workspace/House/Wall").unwrap_err();
        assert_eq!(error.code, Code::Ambiguous);
        assert!(
            error
                .to_string()
                .contains("2 objects are at \"Workspace/House/Wall\""),
            "{error}"
        );
        assert!(error.to_string().contains("Part \"Wall\""), "{error}");
        assert_eq!(
            resolve(&world, "Lightning").unwrap_err().fix.as_deref(),
            Some("did you mean \"Lighting\"?")
        );
    }

    #[test]
    fn a_bare_name_points_to_the_object_called_that() {
        let (world, _, door) = world_with_a_house();
        let error = resolve(&world, "Door").unwrap_err();
        assert_eq!(error.code, Code::NotFound);
        assert_eq!(
            error.fix,
            Some(format!(
                "did you mean \"Workspace/House/Door\" (ID {})?",
                id_text(door)
            ))
        );
    }

    #[test]
    fn a_deleted_or_unknown_id_is_stale() {
        let (mut world, house, _) = world_with_a_house();
        world.set_parent(house, None).unwrap();
        let error = resolve(&world, &id_text(house)).unwrap_err();
        assert_eq!(error.code, Code::StaleId);
        assert!(error.message.contains("was deleted"), "{error}");
        assert_eq!(resolve(&world, "99999").unwrap_err().code, Code::StaleId);
        assert_eq!(id_text(InstanceId::from_bits((2 << 32) | 17)), "17.2");
        assert_eq!(
            parse_id("17.2"),
            Some(InstanceId::from_bits((2 << 32) | 17))
        );
    }
}
