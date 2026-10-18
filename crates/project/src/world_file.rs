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
//!   }
//! }
//! ```
//!
//! Property values are plain JSON. Their meaning comes from the class: the
//! same `[0, 1, 0]` is a `Vector3` for `Position` and a `Color3` for `Color`.
//! Properties that are left out keep their default values.

use axcenity_core::{ClassName, InstanceId, Value, ValueKind, World};
use serde::Deserialize;
use serde_json::{Map, Value as Json};

use crate::FORMAT_VERSION;

#[derive(Debug, thiserror::Error)]
pub enum WorldError {
    /// The text isn't valid JSON, or doesn't have the expected fields.
    /// The message includes the line and column.
    #[error(transparent)]
    Json(serde_json::Error),
    #[error(
        "world format {found} is newer than this engine understands ({FORMAT_VERSION}); update Axcenity to open it"
    )]
    TooNew { found: u32 },
    #[error("{path}: there is no class named `{class}`")]
    UnknownClass { path: String, class: String },
    #[error("{path}: {class} is a service and can't be listed as a child")]
    Service { path: String, class: ClassName },
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
    if file.format_version > FORMAT_VERSION {
        return Err(WorldError::TooNew {
            found: file.format_version,
        });
    }

    let mut world = World::new();
    let workspace = world.workspace();
    apply_properties(&mut world, workspace, &file.workspace.properties)?;
    for child in &file.workspace.children {
        add_instance(&mut world, workspace, child)?;
    }
    Ok(world)
}

/// Creates the Instance described by `node` under `parent`, then its children.
fn add_instance(
    world: &mut World,
    parent: InstanceId,
    node: &InstanceNode,
) -> Result<(), WorldError> {
    let parent_path = world.full_name(parent).expect("parent exists");
    let name = node.name.as_deref().unwrap_or(&node.class);
    let path = format!("{parent_path}.{name}");

    let class = ClassName::from_name(&node.class).ok_or_else(|| WorldError::UnknownClass {
        path: path.clone(),
        class: node.class.clone(),
    })?;
    let id = world
        .create(class)
        .map_err(|_| WorldError::Service { path, class })?;
    world.set_name(id, name).expect("just created");
    world.set_parent(id, Some(parent)).expect("both exist");

    apply_properties(world, id, &node.properties)?;
    for child in &node.children {
        add_instance(world, id, child)?;
    }
    Ok(())
}

fn apply_properties(
    world: &mut World,
    id: InstanceId,
    properties: &Map<String, Json>,
) -> Result<(), WorldError> {
    for (property, json) in properties {
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
    }
}

fn three_numbers(json: &Json) -> Option<[f32; 3]> {
    match json.as_array()?.as_slice() {
        [a, b, c] => Some([a.as_f64()? as f32, b.as_f64()? as f32, c.as_f64()? as f32]),
        _ => None,
    }
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
    fn empty_world() {
        let world = parse_world(r#"{ "format_version": 1 }"#).unwrap();
        assert!(world.get(world.workspace()).unwrap().children().is_empty());
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
}
