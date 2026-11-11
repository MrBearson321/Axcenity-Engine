//! `object_create`: one new object, as the editor's Insert makes it (its
//! defaults, its usual parent, a name no sibling has), as one undo step.
//!
//! `kind` is an Insert name (`Block`, `Sphere`, `PointLight`, …) or its
//! class. The few kinds whose Insert builds several objects or files (a
//! Character, a health bar, splines) aren't offered yet.

use axcenity_core::{InstanceId, Value, Vector3, World};
use axcenity_mcp::{
    Args, Class, Code, Kind, Outcome, Param, Profile, ToolError, ToolSpec, closest,
};
use serde_json::{Map, Value as Json};

use super::Host;
use super::ops::write;
use super::refs::{label, path, quoted, resolve};
use super::values::set_props;
use crate::tree::{self, Insertable};

pub const SPEC: ToolSpec = ToolSpec {
    name: "object_create",
    title: "Create object",
    summary: "Create one object as Insert does, e.g. {\"kind\":\"Block\",\"size\":[20,10,1]}.",
    profile: Profile::Lite,
    class: Class::Add,
    idempotent: false,
    params: &[
        Param::required(
            "kind",
            Kind::String,
            "Block, Sphere, Model, PointLight... (see docs)",
        ),
        Param::optional("name", Kind::String, "default: the kind"),
        Param::optional("parent", Kind::String, "default: where Insert puts it"),
        Param::optional("position", Kind::Vector3, "centre; default on the ground"),
        Param::optional("size", Kind::Vector3, "studs"),
        Param::optional("color", Kind::Vector3, "[r,g,b] 0-1"),
        Param::optional("material", Kind::String, "e.g. Brick"),
        Param::optional("props", Kind::Object, "other properties"),
    ],
};

/// Every kind Insert offers.
pub(super) fn kinds() -> Vec<Insertable> {
    let lists: [&[Insertable]; 27] = [
        &Insertable::PARTS,
        &Insertable::GROUPS,
        &Insertable::UI,
        &Insertable::AUDIO,
        &Insertable::SOUND_EFFECTS,
        &Insertable::LIGHTS,
        &Insertable::PROBES,
        &Insertable::SKY,
        &Insertable::LENS,
        &Insertable::VOLUMES,
        &Insertable::EFFECTS,
        &Insertable::ANIMATION,
        &Insertable::BREAKING,
        &Insertable::CHARACTERS,
        &Insertable::AGENTS,
        &Insertable::CHECKPOINTS,
        &Insertable::HEALTH,
        &Insertable::SEATS,
        &Insertable::INTERACTION,
        &Insertable::HUMANOIDS,
        &Insertable::SPAWNING,
        &Insertable::TOOLS,
        &Insertable::VEHICLES,
        &Insertable::SPLINES,
        &Insertable::CAMERAS,
        &Insertable::CONSTRAINTS,
        &Insertable::MESSENGERS,
    ];
    let mut kinds: Vec<Insertable> = Vec::new();
    for kind in lists.into_iter().flatten().chain(&Insertable::VALUES) {
        if !kinds.contains(kind) {
            kinds.push(*kind);
        }
    }
    kinds
}

/// Kinds whose Insert does more than make one object: not offered yet.
fn compound(kind: Insertable) -> bool {
    matches!(
        kind,
        Insertable::Character
            | Insertable::HealthBar
            | Insertable::Health
            | Insertable::Inventory
            | Insertable::NavigationAgent
            | Insertable::Spline
            | Insertable::SplineMesh
            | Insertable::SplineScatter
            | Insertable::SplineFollower
            | Insertable::Page
            | Insertable::MeshPart
            | Insertable::ReflectionProbe
            | Insertable::LightProbeVolume
            | Insertable::Fracture
    ) || kind.class().is_character_controller()
}

/// The name a client gives for `kind`: its Insert label, without spaces.
pub(super) fn kind_name(kind: Insertable) -> String {
    kind.label().replace(' ', "")
}

/// The kinds `object_create` makes, for `docs`.
pub(super) fn offered() -> Vec<String> {
    kinds()
        .into_iter()
        .filter(|kind| !compound(*kind))
        .map(kind_name)
        .collect()
}

fn find_kind(name: &str) -> Result<Insertable, ToolError> {
    let all = kinds();
    let found = all.iter().copied().find(|kind| {
        kind_name(*kind).eq_ignore_ascii_case(name)
            || kind.class().as_str().eq_ignore_ascii_case(name)
    });
    match found {
        Some(kind) if compound(kind) => Err(ToolError::new(
            Code::Unsupported,
            format!("{name} is made of several objects, which AI tools can't insert yet"),
        )
        .with_fix("insert it in the editor, or build it from Model, Block and other kinds")),
        Some(kind) => Ok(kind),
        None => {
            let names = offered();
            let near = closest(name, names.iter().map(String::as_str), 3);
            let error = ToolError::new(Code::NotFound, format!("no kind {}", quoted(name)));
            Err(if near.is_empty() {
                error.with_fix("docs with no arguments lists every kind")
            } else {
                error.with_fix(format!("did you mean {}?", near.join(" or ")))
            })
        }
    }
}

pub fn call(host: &mut Host, args: &Args) -> Outcome {
    let what = what(args)?;
    let written = write(host, &format!("Create {what}"), |world| step(world, args))?;
    let id = written.value;
    let world = &host.app.world;
    Ok(format!(
        "created {} in {}{}",
        label(world, id),
        parent_path(world, id),
        written.undo_note()
    ))
}

/// The name the new object is asked to have: `name`, else its class.
pub(super) fn what(args: &Args) -> Result<String, ToolError> {
    let kind = find_kind(args.str("kind").unwrap_or_default())?;
    Ok(args.str("name").unwrap_or(kind.class().as_str()).to_owned())
}

/// The path of what `id` is in, for result lines.
pub(super) fn parent_path(world: &World, id: InstanceId) -> String {
    world
        .get(id)
        .and_then(|instance| instance.parent())
        .map_or_else(String::new, |parent| path(world, parent))
}

/// Makes the object `args` describe in `world` (`write` and `batch` call
/// this inside their undo step).
pub(super) fn step(world: &mut World, args: &Args) -> Result<InstanceId, ToolError> {
    let kind = find_kind(args.str("kind").unwrap_or_default())?;
    let parent = match args.str("parent") {
        Some(text) => resolve(world, text)?,
        None => tree::insert_parent(world, None, kind),
    };
    let wanted = args.str("name").unwrap_or(kind.class().as_str());
    let mut props = Map::new();
    for (param, property) in [
        ("size", "Size"),
        ("color", "Color"),
        ("material", "Material"),
    ] {
        if let Some(value) = args.get(param) {
            props.insert(property.to_owned(), value.clone());
        }
    }
    if let Some(Json::Object(more)) = args.get("props") {
        for (name, value) in more {
            props.insert(name.clone(), value.clone());
        }
    }
    create(
        world,
        kind,
        parent,
        wanted,
        args.vector3("position"),
        &props,
    )
}

fn create(
    world: &mut World,
    kind: Insertable,
    parent: InstanceId,
    wanted: &str,
    position: Option<[f64; 3]>,
    props: &Map<String, Json>,
) -> Result<InstanceId, ToolError> {
    let id = tree::create(world, kind, Vector3::new(0.0, 0.0, 0.0));
    let name = tree::unique_name(world, parent, wanted);
    world
        .set_name(id, &name)
        .map_err(|error| ToolError::internal(error.to_string()))?;
    set_props(world, id, props)?;
    let has_position = world
        .get(id)
        .is_some_and(|instance| instance.property("Position").is_some());
    match (position, has_position) {
        (Some([x, y, z]), true) => {
            let at = Vector3::new(x as f32, y as f32, z as f32);
            world
                .set_property(id, "Position", Value::Vector3(at))
                .map_err(|error| ToolError::internal(error.to_string()))?;
        }
        (Some(_), false) => {
            return Err(ToolError::bad_args(format!(
                "a {} has no position",
                kind.class().as_str()
            ))
            .with_fix("leave position out, or put it in a part"));
        }
        // A part rests on the ground at the origin.
        (None, true) => {
            if let Some(Value::Vector3(size)) =
                world.get(id).and_then(|i| i.property("Size")).cloned()
            {
                let at = Vector3::new(0.0, size.y / 2.0, 0.0);
                let _ = world.set_property(id, "Position", Value::Vector3(at));
            }
        }
        (None, false) => {}
    }
    world
        .set_parent(id, Some(parent))
        .map_err(|error| ToolError::new(Code::BadValue, format!("parent: {error}")))?;
    tree::check_editable(world, id).map_err(|reason| {
        ToolError::new(Code::NotAllowed, format!("can't create it there: {reason}"))
            .with_fix("give parent as Workspace or a Model or Folder in it")
    })?;
    Ok(id)
}
