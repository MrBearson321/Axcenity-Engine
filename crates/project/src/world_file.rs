//! The world file (`world.json`): the Instances that make up the world.
//!
//! ```json
//! {
//!   "format_version": 1,
//!   "workspace": {
//!     "children": [
//!       {
//!         "class": "Part",
//!         "name": "Block",
//!         "properties": { "Position": [0, 1, 0], "Color": [0.8, 0.2, 0.2] },
//!         "children": []
//!       }
//!     ]
//!   },
//!   "replicated_storage": {
//!     "children": [ { "class": "RemoteEvent", "name": "Jump" } ]
//!   }
//! }
//! ```
//!
//! Only the Workspace, ReplicatedStorage and the Lighting settings are saved.
//! The Workspace's own properties (`GlobalWind`) are only written when they
//! aren't their defaults, so `"workspace": {}` stays as short as ever.
//! `replicated_storage` can be left out, and is only written when it has
//! something in it. `lighting` (`{ "properties": { "ShadowsEnabled": false } }`)
//! can be left out too, and is only written when a setting isn't its default.
//!
//! Property values are plain JSON. Their meaning comes from the class: the
//! same `[0, 1, 0]` is a `Vector3` for `Position` and a `Color3` for `Color`,
//! `[0, 120]` is a `Vector2`, `[0, 0, 1, 0]` is a `UDim2` (X scale, X offset, Y scale, Y offset), and
//! enum items are strings such as `"Neon"`. Properties that are left out keep
//! their default values.
//!
//! [`write_world`] saves every property, sorted by name, so saving the same
//! world always produces the same text.

use axcenity_core::{
    ClassName, InstanceId, Material, PartType, ParticleType, TextureChannel, ToneMapping, Value,
    ValueKind, World,
};
use serde::Deserialize;
use serde_json::{Map, Value as Json};

/// `world.json` and prefab files. The manifest version is separate: a project
/// upgrade can leave these files unchanged.
const WORLD_FORMAT_VERSION: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum WorldError {
    /// The text isn't valid JSON, or doesn't have the expected fields.
    /// The message includes the line and column.
    #[error(transparent)]
    Json(serde_json::Error),
    #[error(
        "world format {found} is newer than this engine understands ({WORLD_FORMAT_VERSION}); update Axcenity to open it"
    )]
    TooNew { found: u32 },
    #[error("{path}: there is no class named `{class}`")]
    UnknownClass { path: String, class: String },
    #[error("{path}: {class} is a service and can't be listed as a child")]
    Service { path: String, class: ClassName },
    #[error(
        "{path}: scripts can't be listed in the world file; save them as files in the `scripts` folder instead (`.server.lua` for a Script, `.client.lua` for a LocalScript, `.module.lua` for a ModuleScript)"
    )]
    Script { path: String },
    #[error("{path}: Players are made when someone joins the game, so they can't be saved")]
    Player { path: String },
    #[error("{path}: {class} has no property named `{property}`")]
    UnknownProperty {
        path: String,
        class: ClassName,
        property: String,
    },
    #[error("{path}.{property}: {message}")]
    InvalidProperty {
        path: String,
        property: String,
        message: String,
    },
}

// `deny_unknown_fields` turns typos such as "childern" into errors that name
// the line, instead of being silently ignored.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WorldFile {
    format_version: u32,
    #[serde(default)]
    workspace: ServiceNode,
    #[serde(default)]
    replicated_storage: ServiceNode,
    #[serde(default)]
    lighting: LightingNode,
}

/// The Lighting service only has properties; nothing is saved inside it.
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct LightingNode {
    #[serde(default)]
    properties: Map<String, Json>,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct ServiceNode {
    #[serde(default)]
    properties: Map<String, Json>,
    #[serde(default)]
    children: Vec<InstanceNode>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InstanceNode {
    class: String,
    /// Defaults to the class name.
    name: Option<String>,
    #[serde(default)]
    properties: Map<String, Json>,
    #[serde(default)]
    children: Vec<InstanceNode>,
}

/// Builds a [`World`] from the text of a world file.
pub fn parse_world(text: &str) -> Result<World, WorldError> {
    let file: WorldFile = serde_json::from_str(text).map_err(WorldError::Json)?;
    if file.format_version > WORLD_FORMAT_VERSION {
        return Err(WorldError::TooNew {
            found: file.format_version,
        });
    }

    let mut world = World::new();
    let lighting = world.lighting();
    apply_properties(&mut world, lighting, &file.lighting.properties)?;
    let services = [
        (world.workspace(), &file.workspace),
        (world.replicated_storage(), &file.replicated_storage),
    ];
    for (service, node) in services {
        apply_properties(&mut world, service, &node.properties)?;
        for child in &node.children {
            add_instance(&mut world, service, child)?;
        }
    }
    Ok(world)
}

/// A prefab file (`assets/prefabs/*.prefab.json`): Instances saved to be
/// placed again, written like the world file's children.
///
/// ```json
/// { "format_version": 1, "instances": [ { "class": "Model", "name": "Lamp", "children": [] } ] }
/// ```
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PrefabFile {
    format_version: u32,
    #[serde(default)]
    instances: Vec<InstanceNode>,
}

/// Adds the Instances of a prefab file's text to `world`, inside `parent`,
/// and returns the top-level ones in file order. Nothing is added if the
/// text has any error.
pub fn parse_prefab(
    text: &str,
    world: &mut World,
    parent: InstanceId,
) -> Result<Vec<InstanceId>, WorldError> {
    let file: PrefabFile = serde_json::from_str(text).map_err(WorldError::Json)?;
    if file.format_version > WORLD_FORMAT_VERSION {
        return Err(WorldError::TooNew {
            found: file.format_version,
        });
    }
    // Checked in a world of its own first, so a bad file leaves `world` alone.
    let mut check = World::new();
    let workspace = check.workspace();
    for node in &file.instances {
        add_instance(&mut check, workspace, node)?;
    }
    let mut added = Vec::new();
    for node in &file.instances {
        added.push(add_instance(world, parent, node)?);
    }
    Ok(added)
}

/// The text of a prefab file holding `ids` and everything inside them
/// (scripts are left out, as in the world file).
pub fn write_prefab(world: &World, ids: &[InstanceId]) -> String {
    let mut out =
        format!("{{\n  \"format_version\": {WORLD_FORMAT_VERSION},\n  \"instances\": [\n");
    let ids: Vec<InstanceId> = ids
        .iter()
        .copied()
        .filter(|&id| {
            world
                .get(id)
                .is_some_and(|i| !i.class().is_script() && !i.class().is_service())
        })
        .collect();
    for (i, &id) in ids.iter().enumerate() {
        out.push_str("    ");
        write_instance(world, id, 2, &mut out);
        if i + 1 < ids.len() {
            out.push(',');
        }
        out.push('\n');
    }
    out.push_str("  ]\n}\n");
    out
}

/// Creates the Instance described by `node` under `parent`, then its children.
fn add_instance(
    world: &mut World,
    parent: InstanceId,
    node: &InstanceNode,
) -> Result<InstanceId, WorldError> {
    let parent_path = world.full_name(parent).expect("parent exists");
    let name = node.name.as_deref().unwrap_or(&node.class);
    let path = format!("{parent_path}.{name}");

    let class = ClassName::from_name(&node.class).ok_or_else(|| WorldError::UnknownClass {
        path: path.clone(),
        class: node.class.clone(),
    })?;
    if class.is_script() {
        return Err(WorldError::Script { path });
    }
    if class == ClassName::Player {
        return Err(WorldError::Player { path });
    }
    let id = world
        .create(class)
        .map_err(|_| WorldError::Service { path, class })?;
    world.set_name(id, name).expect("just created");
    world.set_parent(id, Some(parent)).expect("both exist");

    apply_properties(world, id, &node.properties)?;
    for child in &node.children {
        add_instance(world, id, child)?;
    }
    Ok(id)
}

fn apply_properties(
    world: &mut World,
    id: InstanceId,
    properties: &Map<String, Json>,
) -> Result<(), WorldError> {
    // `Material` goes first: setting it resets `Metalness` and `Roughness` to
    // its preset, and any values the file gives for those must win.
    let (material, others): (Vec<_>, Vec<_>) = properties
        .iter()
        .partition(|(property, _)| property.as_str() == "Material");

    for (property, json) in material.into_iter().chain(others) {
        let instance = world.get(id).expect("caller passes a live Instance");
        let path = world.full_name(id).expect("caller passes a live Instance");
        let Some(current) = instance.property(property) else {
            return Err(WorldError::UnknownProperty {
                path,
                class: instance.class(),
                property: property.clone(),
            });
        };

        let value =
            json_to_value(json, current.kind()).map_err(|message| WorldError::InvalidProperty {
                path,
                property: property.clone(),
                message,
            })?;
        world
            .set_property(id, property, value)
            .expect("value was converted to the property's own type");
    }
    Ok(())
}

/// Reads `json` as a value of `kind`, or explains what was expected.
fn json_to_value(json: &Json, kind: ValueKind) -> Result<Value, String> {
    match kind {
        ValueKind::Bool => json
            .as_bool()
            .map(Value::Bool)
            .ok_or_else(|| "expected true or false".to_owned()),
        ValueKind::Number => json
            .as_f64()
            .map(Value::Number)
            .ok_or_else(|| "expected a number".to_owned()),
        ValueKind::String => json
            .as_str()
            .map(|s| Value::String(s.to_owned()))
            .ok_or_else(|| "expected a string in quotes".to_owned()),
        ValueKind::Vector3 => {
            let [x, y, z] = three_numbers(json).ok_or("expected [x, y, z]: three numbers")?;
            Ok(Value::Vector3(axcenity_core::Vector3::new(x, y, z)))
        }
        ValueKind::Vector2 => match json.as_array().map(Vec::as_slice) {
            Some([x, y]) => match (x.as_f64(), y.as_f64()) {
                (Some(x), Some(y)) => Ok(Value::Vector2(axcenity_core::Vector2::new(
                    x as f32, y as f32,
                ))),
                _ => Err("expected [x, y]: two numbers".to_owned()),
            },
            _ => Err("expected [x, y]: two numbers".to_owned()),
        },
        ValueKind::UDim2 => {
            let [x_scale, x_offset, y_scale, y_offset] =
                four_numbers(json).ok_or("expected [x_scale, x_offset, y_scale, y_offset]")?;
            Ok(Value::UDim2(axcenity_core::UDim2::new(
                x_scale, x_offset, y_scale, y_offset,
            )))
        }
        ValueKind::Color3 => {
            const EXPECTED: &str = "expected [r, g, b]: three numbers from 0 to 1";
            let [r, g, b] = three_numbers(json).ok_or(EXPECTED)?;
            if [r, g, b].iter().any(|c| *c > 1.0) {
                return Err(format!("{EXPECTED} (for 0-255 values, divide each by 255)"));
            }
            if [r, g, b].iter().any(|c| *c < 0.0) {
                return Err(EXPECTED.to_owned());
            }
            Ok(Value::Color3(axcenity_core::Color3::new(r, g, b)))
        }
        ValueKind::PartType => {
            let names = PartType::ALL.map(PartType::as_str);
            let name = enum_name(json, &names)?;
            Ok(Value::PartType(PartType::from_name(name).expect("checked")))
        }
        ValueKind::Material => {
            let names = Material::ALL.map(Material::as_str);
            let name = enum_name(json, &names)?;
            Ok(Value::Material(Material::from_name(name).expect("checked")))
        }
        ValueKind::ParticleType => {
            let names = ParticleType::ALL.map(ParticleType::as_str);
            let name = enum_name(json, &names)?;
            Ok(Value::ParticleType(
                ParticleType::from_name(name).expect("checked"),
            ))
        }
        ValueKind::TextureChannel => {
            let names = TextureChannel::ALL.map(TextureChannel::as_str);
            let name = enum_name(json, &names)?;
            Ok(Value::TextureChannel(
                TextureChannel::from_name(name).expect("checked"),
            ))
        }
        ValueKind::ToneMapping => {
            let names = ToneMapping::ALL.map(ToneMapping::as_str);
            let name = enum_name(json, &names)?;
            Ok(Value::ToneMapping(
                ToneMapping::from_name(name).expect("checked"),
            ))
        }
    }
}

/// Reads `json` as one of `names`, or explains which names are allowed.
fn enum_name<'a>(json: &'a Json, names: &[&str]) -> Result<&'a str, String> {
    match json.as_str() {
        Some(name) if names.contains(&name) => Ok(name),
        _ => Err(format!("expected one of: \"{}\"", names.join("\", \""))),
    }
}

fn three_numbers(json: &Json) -> Option<[f32; 3]> {
    match json.as_array()?.as_slice() {
        [a, b, c] => Some([a.as_f64()? as f32, b.as_f64()? as f32, c.as_f64()? as f32]),
        _ => None,
    }
}

fn four_numbers(json: &Json) -> Option<[f32; 4]> {
    match json.as_array()?.as_slice() {
        [a, b, c, d] => Some([
            a.as_f64()? as f32,
            b.as_f64()? as f32,
            c.as_f64()? as f32,
            d.as_f64()? as f32,
        ]),
        _ => None,
    }
}

/// The text of a world file for `world`, in the same layout people write by
/// hand: two-space indents, and `[x, y, z]` values on one line.
pub fn write_world(world: &World) -> String {
    let mut out = format!("{{\n  \"format_version\": {WORLD_FORMAT_VERSION},\n  \"workspace\": ");
    write_instance(world, world.workspace(), 1, &mut out);
    // Left out when empty, so worlds without one look as they always have.
    let replicated = world.replicated_storage();
    if !saved_children(world, replicated).is_empty() {
        out.push_str(",\n  \"replicated_storage\": ");
        write_instance(world, replicated, 1, &mut out);
    }
    // Left out while every setting is its default, for the same reason.
    let lighting = world.lighting();
    let changed = world.get(lighting).is_some_and(|instance| {
        let defaults = ClassName::Lighting.default_properties();
        instance
            .properties()
            .any(|(name, value)| defaults.iter().any(|(n, v)| *n == name && v != value))
    });
    if changed {
        out.push_str(",\n  \"lighting\": ");
        write_instance(world, lighting, 1, &mut out);
    }
    out.push_str("\n}\n");
    out
}

/// The children of `id` that go in the world file. Scripts are saved as
/// their own `.lua` files, and Players only exist while the game runs.
fn saved_children(world: &World, id: InstanceId) -> Vec<InstanceId> {
    world.get(id).map_or_else(Vec::new, |instance| {
        instance
            .children()
            .iter()
            .copied()
            .filter(|&child| {
                world
                    .get(child)
                    .is_some_and(|c| !c.class().is_script() && c.class() != ClassName::Player)
            })
            .collect()
    })
}

/// Appends `id` as a JSON object whose closing brace is indented `depth` levels.
fn write_instance(world: &World, id: InstanceId, depth: usize, out: &mut String) {
    let instance = world.get(id).expect("only live Instances are in the tree");
    let indent = "  ".repeat(depth + 1);
    let inner_indent = "  ".repeat(depth + 2);
    let mut fields = Vec::new();

    // A service's class and name are fixed, so the file doesn't repeat them.
    if !instance.class().is_service() {
        fields.push(format!(
            "{indent}\"class\": {}",
            json_string(instance.class().as_str())
        ));
        fields.push(format!(
            "{indent}\"name\": {}",
            json_string(instance.name())
        ));
    }

    // The Workspace leaves out its defaults (see the module docs).
    let defaults = if instance.class() == ClassName::Workspace {
        instance.class().default_properties()
    } else {
        Vec::new()
    };
    let properties: Vec<String> = instance
        .properties()
        .filter(|(name, value)| !defaults.iter().any(|(n, v)| n == name && v == *value))
        .map(|(name, value)| format!("{inner_indent}{}: {}", json_string(name), json_value(value)))
        .collect();
    if !properties.is_empty() {
        fields.push(format!(
            "{indent}\"properties\": {{\n{}\n{indent}}}",
            properties.join(",\n")
        ));
    }

    let children = saved_children(world, id);
    if !children.is_empty() {
        let mut text = format!("{indent}\"children\": [\n");
        for (i, &child) in children.iter().enumerate() {
            text.push_str(&inner_indent);
            write_instance(world, child, depth + 2, &mut text);
            if i + 1 < children.len() {
                text.push(',');
            }
            text.push('\n');
        }
        text.push_str(&format!("{indent}]"));
        fields.push(text);
    }

    if fields.is_empty() {
        out.push_str("{}");
    } else {
        out.push_str(&format!(
            "{{\n{}\n{}}}",
            fields.join(",\n"),
            "  ".repeat(depth)
        ));
    }
}

fn json_value(value: &Value) -> String {
    match value {
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => json_number(*n),
        Value::String(s) => json_string(s),
        Value::Vector3(v) => json_triple([v.x, v.y, v.z]),
        Value::Vector2(v) => {
            let [x, y] = [v.x, v.y].map(|n| {
                if n.is_finite() {
                    n.to_string()
                } else {
                    "null".to_owned()
                }
            });
            format!("[{x}, {y}]")
        }
        Value::UDim2(v) => json_quad([v.x.scale, v.x.offset, v.y.scale, v.y.offset]),
        Value::Color3(c) => json_triple([c.r, c.g, c.b]),
        Value::PartType(t) => json_string(t.as_str()),
        Value::Material(m) => json_string(m.as_str()),
        Value::TextureChannel(c) => json_string(c.as_str()),
        Value::ParticleType(t) => json_string(t.as_str()),
        Value::ToneMapping(t) => json_string(t.as_str()),
    }
}

/// Rust prints floats in the shortest form that reads back as the same
/// number, without a trailing `.0` for whole numbers, which is valid JSON.
/// JSON has no NaN or infinity, so those are written as `null`; loading the
/// file then reports the bad value instead of silently changing it.
fn json_number(n: f64) -> String {
    if n.is_finite() {
        n.to_string()
    } else {
        "null".to_owned()
    }
}

/// Like [`json_number`], for each of three `f32`s. They're printed as `f32`
/// on purpose: widening `0.45_f32` to `f64` first would print 0.44999998807907715.
fn json_triple(numbers: [f32; 3]) -> String {
    let [a, b, c] = numbers.map(|n| {
        if n.is_finite() {
            n.to_string()
        } else {
            "null".to_owned()
        }
    });
    format!("[{a}, {b}, {c}]")
}

fn json_quad(numbers: [f32; 4]) -> String {
    let [a, b, c, d] = numbers.map(|n| {
        if n.is_finite() {
            n.to_string()
        } else {
            "null".to_owned()
        }
    });
    format!("[{a}, {b}, {c}, {d}]")
}

/// A JSON string literal, with quotes and any needed escapes.
fn json_string(s: &str) -> String {
    serde_json::to_string(s).expect("a string always converts to JSON")
}

#[cfg(test)]
mod tests;
