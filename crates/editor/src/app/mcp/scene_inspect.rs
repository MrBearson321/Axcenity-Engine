//! `scene_inspect`: browse the tree, or search it. One line per object.
//!
//! Without filters it shows the tree under `root` to `depth`, folding a run
//! of siblings with the same class and name into one line (`Fence ×40`).
//! With any filter (`class`, `name`, `tag`, `near`) it lists the matches
//! with their paths. Long answers are cut at `limit` lines with a cursor
//! for the next part.

use axcenity_core::{ClassName, InstanceId, Value, World};
use axcenity_mcp::{
    Args, Class, Code, Kind, Outcome, Param, Profile, ToolError, ToolSpec, closest,
};

use super::Host;
use super::refs::{label, path, quoted, resolve};
use super::{CURSOR, LIMIT};

pub const SPEC: ToolSpec = ToolSpec {
    name: "scene_inspect",
    title: "Inspect scene",
    summary: "Browse the object tree, or search it by class, name, tag or distance.",
    profile: Profile::Lite,
    class: Class::Read,
    idempotent: true,
    params: &[
        Param::optional(
            "root",
            Kind::String,
            "ID or path; default Workspace; game for all",
        ),
        Param::optional("depth", Kind::Integer, "tree levels, default 2"),
        Param::optional("class", Kind::String, "class or base (BasePart...)"),
        Param::optional("name", Kind::String, "name contains"),
        Param::optional("tag", Kind::String, "has tag"),
        Param::optional("near", Kind::Vector3, "within radius of [x,y,z]"),
        Param::optional("radius", Kind::Number, "default 50"),
        LIMIT,
        CURSOR,
    ],
};

const DEFAULT_DEPTH: u64 = 2;
const MAX_DEPTH: u64 = 12;
const DEFAULT_LIMIT: usize = 60;
const MAX_LIMIT: usize = 500;

pub fn call(host: &mut Host, args: &Args) -> Outcome {
    let world = &host.app.world;
    let root = match args.str("root") {
        Some(text) => resolve(world, text)?,
        None => world.workspace(),
    };
    let limit = args.limit(DEFAULT_LIMIT, MAX_LIMIT);
    let offset = match args.str("cursor") {
        Some(text) => text
            .parse::<usize>()
            .map_err(|_| ToolError::bad_args("cursor should be the text a cut result gave"))?,
        None => 0,
    };
    let searching = ["class", "name", "tag"]
        .iter()
        .any(|name| filter(args, name).is_some())
        || args.get("near").is_some();
    let (header, lines) = if searching {
        search(world, root, args)?
    } else {
        let depth = args
            .u64("depth")
            .unwrap_or(DEFAULT_DEPTH)
            .clamp(1, MAX_DEPTH);
        tree(world, root, depth as usize)
    };
    Ok(page(header, &lines, offset, limit))
}

/// The lines from `offset`, at most `limit` of them, after the header, with
/// how to get the rest.
fn page(header: String, lines: &[String], offset: usize, limit: usize) -> String {
    let mut text = header;
    let end = (offset + limit).min(lines.len());
    for line in lines.get(offset..end).unwrap_or(&[]) {
        text.push('\n');
        text.push_str(line);
    }
    if end < lines.len() {
        text.push_str(&format!(
            "\n… {} more lines: call again with cursor \"{end}\"",
            lines.len() - end
        ));
    }
    text
}

fn tree(world: &World, root: InstanceId, depth: usize) -> (String, Vec<String>) {
    let children = world
        .get(root)
        .map_or(0, |instance| instance.children().len());
    let header = format!(
        "{} · {} · {children} children",
        label(world, root),
        path(world, root)
    );
    let mut lines = Vec::new();
    add_children(world, root, 0, depth, &mut lines);
    (header, lines)
}

fn add_children(
    world: &World,
    parent: InstanceId,
    level: usize,
    depth: usize,
    lines: &mut Vec<String>,
) {
    let Some(instance) = world.get(parent) else {
        return;
    };
    let indent = "  ".repeat(level);
    let children = instance.children();
    let mut i = 0;
    while i < children.len() {
        let id = children[i];
        let Some(child) = world.get(id) else {
            i += 1;
            continue;
        };
        let inside = child.children().len();
        let opens = inside > 0 && level + 1 < depth;
        // A run of same-class, same-name siblings that won't be opened
        // folds into one line.
        let mut run = 1;
        if !opens {
            while let Some(&next) = children.get(i + run) {
                let same = world.get(next).is_some_and(|other| {
                    other.class() == child.class()
                        && other.name() == child.name()
                        && (other.children().is_empty() == (inside == 0))
                });
                if !same {
                    break;
                }
                run += 1;
            }
        }
        let mut line = format!("{indent}{}", label(world, id));
        if run > 1 {
            line.push_str(&format!(" ×{run}"));
        }
        if inside > 0 && !opens {
            line.push_str(&format!(" (+{inside} inside)"));
        }
        lines.push(line);
        if opens {
            add_children(world, id, level + 1, depth, lines);
        }
        i += run;
    }
}

fn search(
    world: &World,
    root: InstanceId,
    args: &Args,
) -> Result<(String, Vec<String>), ToolError> {
    let class = filter(args, "class");
    if let Some(name) = class
        && !ClassName::ALL.iter().any(|class| class.is_a(name))
    {
        let mut error = ToolError::new(Code::NotFound, format!("no class {}", quoted(name)));
        if let Some(made) = super::object_create::class_of_kind(name) {
            // `Block` is a kind object_create makes, not a class.
            error = error.with_fix(format!(
                "{name} is an object_create kind; its class is {} (or search by name)",
                made.as_str()
            ));
        } else {
            let names: Vec<&str> = ClassName::ALL.iter().map(|class| class.as_str()).collect();
            let near = closest(name, names, 3);
            if !near.is_empty() {
                error = error.with_fix(format!("did you mean {}?", near.join(" or ")));
            }
        }
        return Err(error);
    }
    let name = filter(args, "name").map(str::to_lowercase);
    let tag = filter(args, "tag");
    let near = args.vector3("near");
    let radius = args.f64("radius").unwrap_or(50.0);
    let candidates: Vec<InstanceId> = match tag {
        Some(tag) => world.in_tree_order(world.tagged_iter(tag).collect()),
        None => world.descendants_iter(root).collect(),
    };
    let matches: Vec<InstanceId> = candidates
        .into_iter()
        .filter(|&id| id == root || world.is_descendant_of(id, root))
        .filter(|&id| {
            let Some(instance) = world.get(id) else {
                return false;
            };
            class.is_none_or(|class| instance.class().is_a(class))
                && name
                    .as_ref()
                    .is_none_or(|name| instance.name().to_lowercase().contains(name))
                && near.is_none_or(|[x, y, z]| match instance.property("Position") {
                    Some(Value::Vector3(p)) => {
                        let (dx, dy, dz) =
                            (f64::from(p.x) - x, f64::from(p.y) - y, f64::from(p.z) - z);
                        (dx * dx + dy * dy + dz * dz).sqrt() <= radius
                    }
                    _ => false,
                })
        })
        .collect();
    let mut header = format!(
        "{} {} under {}",
        matches.len(),
        if matches.len() == 1 {
            "match"
        } else {
            "matches"
        },
        path(world, root)
    );
    // Small models search a name as a tag: say when that's what happened.
    if let Some(tag) = tag
        && matches.is_empty()
        && world.tagged_iter(tag).next().is_none()
    {
        let named = world
            .descendants_iter(root)
            .filter(|&id| world.get(id).is_some_and(|i| i.name() == tag))
            .count();
        header.push_str(&format!("\nno object has the tag {}", quoted(tag)));
        if named > 0 {
            header.push_str(&format!(
                "; {named} {} named that: search with name instead",
                if named == 1 { "is" } else { "are" }
            ));
        }
    }
    let lines = matches
        .iter()
        .map(|&id| format!("{}  {}", label(world, id), path(world, id)))
        .collect();
    Ok((header, lines))
}

/// A search filter's text: an empty one counts as not given (small models
/// send `"tag": ""` for filters they don't want).
fn filter<'a>(args: &'a Args, name: &str) -> Option<&'a str> {
    args.str(name)
        .map(str::trim)
        .filter(|text| !text.is_empty())
}
