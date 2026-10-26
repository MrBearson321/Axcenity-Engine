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
    ClassName, InstanceId, Material, PartType, ParticleType, TextureChannel, Value, ValueKind,
    World,
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
mod tests {
    use super::*;
    use axcenity_core::{Color3, Vector3};

    fn parse_err(text: &str) -> String {
        match parse_world(text) {
            Ok(_) => panic!("expected an error"),
            Err(err) => err.to_string(),
        }
    }

    #[test]
    fn prefabs_round_trip_and_a_bad_one_adds_nothing() {
        let mut world = World::new();
        let workspace = world.workspace();
        let model = world.create(ClassName::Model).unwrap();
        world.set_name(model, "Lamp").unwrap();
        world.set_parent(model, Some(workspace)).unwrap();
        let bulb = world.create(ClassName::Part).unwrap();
        world.set_name(bulb, "Bulb").unwrap();
        world
            .set_property(bulb, "Material", Value::Material(Material::Neon))
            .unwrap();
        world.set_parent(bulb, Some(model)).unwrap();
        let light = world.create(ClassName::PointLight).unwrap();
        world.set_parent(light, Some(bulb)).unwrap();
        let text = write_prefab(&world, &[model]);

        let mut other = World::new();
        let workspace = other.workspace();
        let added = parse_prefab(&text, &mut other, workspace).unwrap();
        assert_eq!(added.len(), 1);
        assert_eq!(write_prefab(&other, &added), text);

        let before = other.descendants(workspace).len();
        let bad = r#"{ "format_version": 1, "instances": [
            { "class": "Part" }, { "class": "Nope" } ] }"#;
        assert!(parse_prefab(bad, &mut other, workspace).is_err());
        assert_eq!(other.descendants(workspace).len(), before);
    }

    #[test]
    fn empty_world() {
        let world = parse_world(r#"{ "format_version": 1 }"#).unwrap();
        assert!(world.get(world.workspace()).unwrap().children().is_empty());
    }

    #[test]
    fn lighting_settings_round_trip_and_default_ones_are_not_written() {
        let world = parse_world(r#"{ "format_version": 1 }"#).unwrap();
        assert!(!write_world(&world).contains("lighting"));

        let world = parse_world(
            r#"{ "format_version": 1, "lighting": { "properties": { "ShadowsEnabled": false } } }"#,
        )
        .unwrap();
        let lighting = world.get(world.lighting()).unwrap();
        assert_eq!(
            lighting.property("ShadowsEnabled"),
            Some(&Value::Bool(false))
        );
        let reloaded = parse_world(&write_world(&world)).unwrap();
        let lighting = reloaded.get(reloaded.lighting()).unwrap();
        assert_eq!(
            lighting.property("ShadowsEnabled"),
            Some(&Value::Bool(false))
        );
    }

    #[test]
    fn part_with_properties_and_nested_child() {
        let world = parse_world(
            r#"{
                "format_version": 1,
                "workspace": { "children": [
                    {
                        "class": "Part",
                        "name": "Block",
                        "properties": {
                            "Position": [1, 2, 3],
                            "Color": [1, 0.5, 0],
                            "Metalness": 1
                        },
                        "children": [ { "class": "Part" } ]
                    }
                ] }
            }"#,
        )
        .unwrap();

        let block = world.find_first_child(world.workspace(), "Block").unwrap();
        let block = world.get(block).unwrap();
        assert_eq!(
            block.property("Position"),
            Some(&Value::Vector3(Vector3::new(1.0, 2.0, 3.0)))
        );
        assert_eq!(
            block.property("Color"),
            Some(&Value::Color3(Color3::new(1.0, 0.5, 0.0)))
        );
        assert_eq!(block.property("Metalness"), Some(&Value::Number(1.0)));
        // Left out, so still the default.
        assert_eq!(block.property("Roughness"), Some(&Value::Number(0.5)));

        let child = world.get(block.children()[0]).unwrap();
        assert_eq!(child.name(), "Part");
    }

    #[test]
    fn malformed_json_reports_line_and_column() {
        let message = parse_err("{\n  \"format_version\": 1,\n  \"workspace\": {\n}");
        assert!(message.contains("line"), "{message}");
        assert!(message.contains("column"), "{message}");
    }

    #[test]
    fn misspelled_field_is_an_error() {
        let message = parse_err(r#"{ "format_version": 1, "workspce": {} }"#);
        assert!(message.contains("workspce"), "{message}");
    }

    #[test]
    fn missing_format_version_is_an_error() {
        let message = parse_err(r#"{ "workspace": {} }"#);
        assert!(message.contains("format_version"), "{message}");
    }

    #[test]
    fn newer_format_is_an_error() {
        assert!(matches!(
            parse_world(r#"{ "format_version": 999 }"#),
            Err(WorldError::TooNew { found: 999 })
        ));
    }

    #[test]
    fn unknown_class_names_the_path() {
        let message = parse_err(
            r#"{ "format_version": 1, "workspace": { "children": [
                { "class": "Prat", "name": "Floor" } ] } }"#,
        );
        assert!(message.contains("Workspace.Floor"), "{message}");
        assert!(message.contains("Prat"), "{message}");
    }

    #[test]
    fn service_as_child_is_an_error() {
        assert!(matches!(
            parse_world(
                r#"{ "format_version": 1, "workspace": { "children": [
                    { "class": "Workspace" } ] } }"#
            ),
            Err(WorldError::Service { .. })
        ));
    }

    #[test]
    fn script_in_the_world_file_is_an_error() {
        let err = parse_err(
            r#"{ "format_version": 1, "workspace": { "children": [
                { "class": "Script", "name": "Spin" } ] } }"#,
        );
        assert!(err.contains("Workspace.Spin"), "{err}");
        assert!(err.contains("scripts"), "{err}");
    }

    #[test]
    fn local_scripts_and_players_in_the_world_file_are_errors() {
        let err = parse_err(
            r#"{ "format_version": 1, "replicated_storage": { "children": [
                { "class": "LocalScript", "name": "Keys" } ] } }"#,
        );
        assert!(err.contains("ReplicatedStorage.Keys"), "{err}");
        assert!(err.contains(".client.lua"), "{err}");
        let err = parse_err(
            r#"{ "format_version": 1, "workspace": { "children": [
                { "class": "Player" } ] } }"#,
        );
        assert!(err.contains("Players are made when someone joins"), "{err}");
    }

    #[test]
    fn scripts_and_players_are_left_out_when_saving() {
        let mut world = World::new();
        for (class, parent) in [
            (ClassName::Script, world.workspace()),
            (ClassName::LocalScript, world.replicated_storage()),
            (ClassName::Player, world.replicated_storage()),
        ] {
            let id = world.create(class).unwrap();
            world.set_parent(id, Some(parent)).unwrap();
        }
        assert_eq!(write_world(&world), write_world(&World::new()));
    }

    #[test]
    fn remote_events_in_replicated_storage_save_and_load() {
        let mut world = World::new();
        let folder = world.create(ClassName::Folder).unwrap();
        world.set_name(folder, "Remotes").unwrap();
        world
            .set_parent(folder, Some(world.replicated_storage()))
            .unwrap();
        let remote = world.create(ClassName::RemoteEvent).unwrap();
        world.set_name(remote, "Jump").unwrap();
        world.set_parent(remote, Some(folder)).unwrap();

        let written = write_world(&world);
        assert_eq!(
            written,
            "{\n  \"format_version\": 1,\n  \"workspace\": {},\n  \"replicated_storage\": {\n    \"children\": [\n      {\n        \"class\": \"Folder\",\n        \"name\": \"Remotes\",\n        \"children\": [\n          {\n            \"class\": \"RemoteEvent\",\n            \"name\": \"Jump\"\n          }\n        ]\n      }\n    ]\n  }\n}\n"
        );
        let loaded = parse_world(&written).unwrap();
        let jump = loaded
            .find_first_descendant(loaded.replicated_storage(), "Jump")
            .unwrap();
        assert_eq!(loaded.get(jump).unwrap().class(), ClassName::RemoteEvent);
        assert_eq!(write_world(&loaded), written);
    }

    #[test]
    fn a_scrolling_frame_and_layer_order_save_and_load_and_older_worlds_get_defaults() {
        let mut world = World::new();
        let gui = world.create(ClassName::ScreenGui).unwrap();
        world.set_parent(gui, Some(world.workspace())).unwrap();
        world
            .set_property(gui, "DisplayOrder", Value::Number(3.0))
            .unwrap();
        let list = world.create(ClassName::ScrollingFrame).unwrap();
        world.set_parent(list, Some(gui)).unwrap();
        let at = axcenity_core::Vector2::new(0.0, 120.5);
        world
            .set_property(list, "CanvasPosition", Value::Vector2(at))
            .unwrap();
        world
            .set_property(list, "ZIndex", Value::Number(4.0))
            .unwrap();

        let written = write_world(&world);
        assert!(
            written.contains("\"CanvasPosition\": [0, 120.5]"),
            "{written}"
        );
        let loaded = parse_world(&written).unwrap();
        let list = loaded
            .find_first_descendant(loaded.workspace(), "ScrollingFrame")
            .unwrap();
        let list = loaded.get(list).unwrap();
        assert_eq!(list.property("CanvasPosition"), Some(&Value::Vector2(at)));
        assert_eq!(list.property("ZIndex"), Some(&Value::Number(4.0)));
        assert_eq!(write_world(&loaded), written);

        // A world saved before layer order: a Frame without ZIndex and a
        // ScreenGui without DisplayOrder load with the defaults.
        let old = parse_world(
            r#"{ "format_version": 1, "workspace": { "children": [
                { "class": "ScreenGui", "name": "Hud", "properties": { "Enabled": true },
                  "children": [ { "class": "Frame", "name": "Panel" } ] } ] } }"#,
        )
        .unwrap();
        let panel = old.find_first_descendant(old.workspace(), "Panel").unwrap();
        let hud = old.find_first_descendant(old.workspace(), "Hud").unwrap();
        assert_eq!(
            old.get(panel).unwrap().property("ZIndex"),
            Some(&Value::Number(1.0))
        );
        assert_eq!(
            old.get(hud).unwrap().property("DisplayOrder"),
            Some(&Value::Number(0.0))
        );
    }

    #[test]
    fn ui_instances_and_udim2_properties_save_and_load() {
        let mut world = World::new();
        let gui = world.create(ClassName::ScreenGui).unwrap();
        world.set_name(gui, "Hud").unwrap();
        world.set_parent(gui, Some(world.workspace())).unwrap();
        let frame = world.create(ClassName::Frame).unwrap();
        world.set_parent(frame, Some(gui)).unwrap();
        world
            .set_property(frame, "Class", Value::String("hud".to_owned()))
            .unwrap();
        world
            .set_property(frame, "Id", Value::String("main".to_owned()))
            .unwrap();
        let position = axcenity_core::UDim2::new(0.5, 12.0, 0.25, -4.0);
        world
            .set_property(frame, "Position", Value::UDim2(position))
            .unwrap();
        let label = world.create(ClassName::TextLabel).unwrap();
        world.set_parent(label, Some(frame)).unwrap();
        world
            .set_property(label, "Text", Value::String("Ready".to_owned()))
            .unwrap();

        let written = write_world(&world);
        assert!(written.contains("\"Position\": [0.5, 12, 0.25, -4]"));
        let loaded = parse_world(&written).unwrap();
        let frame = loaded
            .find_first_descendant(loaded.workspace(), "Frame")
            .unwrap();
        assert_eq!(
            loaded.get(frame).unwrap().property("Position"),
            Some(&Value::UDim2(position))
        );
        assert_eq!(
            loaded.get(frame).unwrap().property("Class"),
            Some(&Value::String("hud".to_owned()))
        );
        assert_eq!(
            loaded.get(frame).unwrap().property("Id"),
            Some(&Value::String("main".to_owned()))
        );
        let label = loaded
            .find_first_descendant(loaded.workspace(), "TextLabel")
            .unwrap();
        assert_eq!(
            loaded.get(label).unwrap().property("Text"),
            Some(&Value::String("Ready".to_owned()))
        );
        assert_eq!(write_world(&loaded), written);
    }

    #[test]
    fn unknown_property_is_an_error() {
        let message = parse_err(
            r#"{ "format_version": 1, "workspace": { "children": [
                { "class": "Part", "properties": { "Colour": [1, 0, 0] } } ] } }"#,
        );
        assert!(message.contains("Colour"), "{message}");
    }

    #[test]
    fn wrong_property_shape_explains_what_was_expected() {
        let message = parse_err(
            r#"{ "format_version": 1, "workspace": { "children": [
                { "class": "Part", "properties": { "Position": [1, 2] } } ] } }"#,
        );
        assert_eq!(
            message,
            "Workspace.Part.Position: expected [x, y, z]: three numbers"
        );
    }

    #[test]
    fn color_out_of_range_suggests_dividing_by_255() {
        let message = parse_err(
            r#"{ "format_version": 1, "workspace": { "children": [
                { "class": "Part", "properties": { "Color": [255, 0, 0] } } ] } }"#,
        );
        assert!(message.contains("divide each by 255"), "{message}");
    }

    #[test]
    fn enum_values_are_read_by_name() {
        let world = parse_world(
            r#"{ "format_version": 1, "workspace": { "children": [
                { "class": "Part", "properties": { "Shape": "Ball", "Material": "Neon" } } ] } }"#,
        )
        .unwrap();
        let part = world.get(world.descendants(world.workspace())[0]).unwrap();
        assert_eq!(
            part.property("Shape"),
            Some(&Value::PartType(PartType::Ball))
        );
        assert_eq!(
            part.property("Material"),
            Some(&Value::Material(Material::Neon))
        );
    }

    #[test]
    fn unknown_enum_item_lists_the_choices() {
        let message = parse_err(
            r#"{ "format_version": 1, "workspace": { "children": [
                { "class": "Part", "properties": { "Material": "Wood" } } ] } }"#,
        );
        assert_eq!(
            message,
            r#"Workspace.Part.Material: expected one of: "Plastic", "Metal", "Glass", "Neon""#
        );
    }

    #[test]
    fn roughness_in_the_file_overrides_the_material_preset() {
        // "Roughness" is listed before "Material" on purpose: the order in the file doesn't matter.
        let world = parse_world(
            r#"{ "format_version": 1, "workspace": { "children": [
                { "class": "Part", "properties": { "Roughness": 0.8, "Material": "Metal" } } ] } }"#,
        )
        .unwrap();
        let part = world.get(world.descendants(world.workspace())[0]).unwrap();
        assert_eq!(part.property("Metalness"), Some(&Value::Number(1.0)));
        assert_eq!(part.property("Roughness"), Some(&Value::Number(0.8)));
    }

    #[test]
    fn written_world_has_the_hand_written_layout() {
        let world = parse_world(
            r#"{ "format_version": 1, "workspace": { "children": [
                { "class": "Part", "name": "Say \"hi\"",
                  "properties": { "Position": [1, 0.45, -2.5], "Material": "Glass" },
                  "children": [ { "class": "Part" } ] } ] } }"#,
        )
        .unwrap();
        let part = |indent: &str, name: &str, position: &str, material: &str, roughness: &str| {
            [
                format!("{indent}  \"class\": \"Part\","),
                format!("{indent}  \"name\": {name},"),
                format!("{indent}  \"properties\": {{"),
                format!("{indent}    \"Anchored\": false,"),
                format!("{indent}    \"CanCollide\": true,"),
                format!("{indent}    \"CastShadow\": true,"),
                format!("{indent}    \"Color\": [0.64, 0.64, 0.64],"),
                format!("{indent}    \"Emission\": 0,"),
                format!("{indent}    \"EmissiveColor\": [1, 1, 1],"),
                format!("{indent}    \"Material\": \"{material}\","),
                format!("{indent}    \"Metalness\": 0,"),
                format!("{indent}    \"Orientation\": [0, 0, 0],"),
                format!("{indent}    \"Position\": {position},"),
                format!("{indent}    \"Roughness\": {roughness},"),
                format!("{indent}    \"Shape\": \"Block\","),
                format!("{indent}    \"Size\": [4, 1, 2],"),
                format!("{indent}    \"TileSize\": 8,"),
                format!("{indent}    \"Transparency\": 0,"),
                format!("{indent}    \"WindAffected\": false"),
                format!("{indent}  }}"),
            ]
            .join("\n")
        };
        let expected = [
            "{".to_owned(),
            "  \"format_version\": 1,".to_owned(),
            "  \"workspace\": {".to_owned(),
            "    \"children\": [".to_owned(),
            "      {".to_owned(),
            part(
                "      ",
                r#""Say \"hi\"""#,
                "[1, 0.45, -2.5]",
                "Glass",
                "0.05",
            ) + ",",
            "        \"children\": [".to_owned(),
            "          {".to_owned(),
            part("          ", "\"Part\"", "[0, 0, 0]", "Plastic", "0.5"),
            "          }".to_owned(),
            "        ]".to_owned(),
            "      }".to_owned(),
            "    ]".to_owned(),
            "  }".to_owned(),
            "}".to_owned(),
            String::new(),
        ]
        .join("\n");
        assert_eq!(write_world(&world), expected);
    }

    #[test]
    fn models_and_folders_save_and_load_with_their_children() {
        let text = r#"{ "format_version": 1, "workspace": { "children": [
            { "class": "Folder", "name": "Props", "children": [
                { "class": "Model", "name": "Car", "children": [
                    { "class": "Part", "name": "Body" } ] } ] } ] } }"#;
        let world = parse_world(text).unwrap();
        let body = world
            .find_first_descendant(world.workspace(), "Body")
            .unwrap();
        assert_eq!(
            world.full_name(body).as_deref(),
            Some("Workspace.Props.Car.Body")
        );
        let car = world.get(body).unwrap().parent().unwrap();
        assert_eq!(world.get(car).unwrap().class(), ClassName::Model);

        // They have no properties, so the file lists only their class, name and children.
        let written = write_world(&world);
        assert!(
            written.contains(
                "\"class\": \"Folder\",\n        \"name\": \"Props\",\n        \"children\": ["
            ),
            "{written}"
        );
        assert_eq!(write_world(&parse_world(&written).unwrap()), written);
    }

    #[test]
    fn empty_world_writes_an_empty_workspace() {
        let world = World::new();
        assert_eq!(
            write_world(&world),
            "{\n  \"format_version\": 1,\n  \"workspace\": {}\n}\n"
        );
    }

    #[test]
    fn global_wind_is_saved_only_when_it_blows() {
        let mut world = World::new();
        let workspace = world.workspace();
        world
            .set_property(
                workspace,
                "GlobalWind",
                Value::Vector3(axcenity_core::Vector3::new(10.0, 0.0, -5.0)),
            )
            .unwrap();
        let text = write_world(&world);
        assert!(text.contains("\"GlobalWind\": [10, 0, -5]"), "{text}");
        let reloaded = parse_world(&text).unwrap();
        assert_eq!(
            reloaded
                .get(reloaded.workspace())
                .unwrap()
                .property("GlobalWind"),
            world.get(workspace).unwrap().property("GlobalWind")
        );
    }

    #[test]
    fn values_that_json_cannot_hold_fail_to_load_again() {
        let mut world = World::new();
        let part = world.create(ClassName::Part).unwrap();
        world.set_parent(part, Some(world.workspace())).unwrap();
        world
            .set_property(part, "Metalness", Value::Number(f64::NAN))
            .unwrap();
        assert!(matches!(
            parse_world(&write_world(&world)),
            Err(WorldError::InvalidProperty { .. })
        ));
    }

    #[test]
    fn a_third_person_controller_saves_and_loads() {
        let mut world = World::new();
        let controller = world.create(ClassName::ThirdPersonController).unwrap();
        world
            .set_parent(controller, Some(world.workspace()))
            .unwrap();
        world
            .set_property(controller, "Movement", Value::String("Run".to_owned()))
            .unwrap();
        world
            .set_property(controller, "WalkSpeed", Value::Number(12.0))
            .unwrap();
        world
            .set_property(controller, "RunSpeed", Value::Number(40.0))
            .unwrap();
        let reloaded = parse_world(&write_world(&world)).unwrap();
        let id = reloaded
            .find_first_child(reloaded.workspace(), "ThirdPersonController")
            .unwrap();
        let saved = reloaded.get(id).unwrap();
        assert_eq!(
            saved.property("Movement"),
            Some(&Value::String("Run".to_owned()))
        );
        assert_eq!(saved.property("WalkSpeed"), Some(&Value::Number(12.0)));
        assert_eq!(saved.property("RunSpeed"), Some(&Value::Number(40.0)));
        assert_eq!(saved.property("CameraDistance"), Some(&Value::Number(12.0)));
    }
}
