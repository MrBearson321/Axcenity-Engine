//! Property values as results show them: the `world.json` forms (so a
//! value read here can be written back the same way), compact: numbers to
//! 3 decimals, no spaces. Object references are IDs.

use axcenity_core::{ClassName, InstanceId, Value, World};
use axcenity_mcp::text::number;
use axcenity_mcp::{Code, ToolError, closest};
use serde_json::{Map, Value as Json};

use super::refs::{id_text, quoted, resolve};

fn list(numbers: &[f64]) -> String {
    let parts: Vec<String> = numbers.iter().map(|&n| number(n)).collect();
    format!("[{}]", parts.join(","))
}

/// `value` as a result shows it.
pub fn show(value: &Value) -> String {
    match value {
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => number(*n),
        Value::String(s) => quoted(s),
        Value::Vector3(v) => list(&[v.x, v.y, v.z].map(f64::from)),
        Value::Vector2(v) => list(&[v.x, v.y].map(f64::from)),
        Value::UDim2(v) => list(&[v.x.scale, v.x.offset, v.y.scale, v.y.offset].map(f64::from)),
        Value::Color3(c) => list(&[c.r, c.g, c.b].map(f64::from)),
        Value::PartType(t) => quoted(t.as_str()),
        Value::Material(m) => quoted(m.as_str()),
        Value::TextureChannel(c) => quoted(c.as_str()),
        Value::ParticleType(t) => quoted(t.as_str()),
        Value::ToneMapping(t) => quoted(t.as_str()),
        Value::CFrame(c) => {
            let p = c.position;
            let mut numbers = vec![f64::from(p.x), f64::from(p.y), f64::from(p.z)];
            numbers.extend(c.rotation.map(f64::from));
            list(&numbers)
        }
        Value::EnumItem(item) => quoted(&format!("{}.{}", item.enum_name, item.item)),
        Value::Instance(None) => "null".to_owned(),
        Value::Instance(Some(id)) => quoted(&id_text(*id)),
        Value::PhysicalProperties(None) => "null".to_owned(),
        Value::PhysicalProperties(Some(physical)) => list(&physical.to_array().map(f64::from)),
    }
}

/// `show`, with a reference also naming its target: `"12" (Model "House")`.
pub fn show_in(world: &World, value: &Value) -> String {
    match value {
        Value::Instance(Some(id)) => match world.get(*id) {
            Some(target) => format!(
                "{} ({} {})",
                quoted(&id_text(*id)),
                target.class().as_str(),
                quoted(target.name())
            ),
            None => format!("{} (deleted)", quoted(&id_text(*id))),
        },
        other => show(other),
    }
}

/// What kind of value it is, for `docs`.
pub fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Vector3(_) => "vector3",
        Value::Vector2(_) => "vector2",
        Value::UDim2(_) => "udim2",
        Value::Color3(_) => "color",
        Value::PartType(_) => "PartType",
        Value::Material(_) => "Material",
        Value::TextureChannel(_) => "TextureChannel",
        Value::ParticleType(_) => "ParticleType",
        Value::ToneMapping(_) => "ToneMapping",
        Value::CFrame(_) => "cframe",
        Value::EnumItem(_) => "enum",
        Value::Instance(_) => "object",
        Value::PhysicalProperties(_) => "physical",
    }
}

/// A class's default value for `property`, as a new one has it.
pub fn default_of(class: ClassName, property: &str) -> Option<Value> {
    class
        .default_properties()
        .into_iter()
        .find(|(name, _)| *name == property)
        .map(|(_, value)| value)
}

/// Whether `value` differs from the class default (a property with no
/// default always counts as differing).
pub fn differs(class: ClassName, property: &str, value: &Value) -> bool {
    match default_of(class, property) {
        Some(default) => !same(&default, value),
        None => true,
    }
}

/// Equal as shown: tiny float differences don't count.
fn same(a: &Value, b: &Value) -> bool {
    a == b || show(a) == show(b)
}

/// The properties of `id` worth showing at `std` detail: those that differ
/// from the class default, sorted by name.
pub fn changed_properties(world: &World, id: InstanceId) -> Vec<(&'static str, Value)> {
    let Some(instance) = world.get(id) else {
        return Vec::new();
    };
    let class = instance.class();
    instance
        .properties()
        .filter(|(name, value)| differs(class, name, value))
        .map(|(name, value)| (name, value.clone()))
        .collect()
}

/// The error for `name`, which `class` doesn't have, with the closest of
/// its `known` properties.
pub fn unknown_property<'a>(
    class: ClassName,
    name: &str,
    known: impl IntoIterator<Item = &'a str>,
) -> ToolError {
    let near = closest(name, known, 3);
    let error = ToolError::new(
        Code::UnknownProperty,
        format!("{} has no property {}", class.as_str(), quoted(name)),
    );
    error.with_fix(if near.is_empty() {
        format!(
            "docs with class \"{}\" lists its properties",
            class.as_str()
        )
    } else {
        format!("did you mean {}?", near.join(" or "))
    })
}

/// Reads `json` as a value for `property` of `id`, in the `world.json`
/// form (`project::json_to_value`); an object reference is an ID, a path or
/// `null`.
pub fn read_value(
    world: &World,
    id: InstanceId,
    property: &str,
    json: &Json,
) -> Result<Value, ToolError> {
    let instance = world
        .get(id)
        .ok_or_else(|| ToolError::internal("object vanished"))?;
    let class = instance.class();
    let Some(current) = instance.property(property) else {
        return Err(unknown_property(
            class,
            property,
            instance.properties().map(|(name, _)| name),
        ));
    };
    let bad = |message: String| {
        ToolError::new(Code::BadValue, format!("{property}: {message}")).with_fix(format!(
            "docs with {{\"class\":\"{}\",\"property\":\"{property}\"}} shows its type",
            class.as_str()
        ))
    };
    if let Value::Instance(_) = current {
        return match json {
            Json::Null => Ok(Value::Instance(None)),
            Json::String(text) => Ok(Value::Instance(Some(resolve(world, text)?))),
            _ => Err(bad("expected an object's ID or path, or null".to_owned())),
        };
    }
    axcenity_project::json_to_value(json, current.kind()).map_err(bad)
}

/// Sets each of `props` (property name to `world.json` value) on `id`.
/// `Name` renames it. Returns the names set.
pub fn set_props(
    world: &mut World,
    id: InstanceId,
    props: &Map<String, Json>,
) -> Result<Vec<String>, ToolError> {
    let mut names = Vec::new();
    for (name, json) in props {
        match name.as_str() {
            "Name" => {
                let text = json
                    .as_str()
                    .ok_or_else(|| ToolError::new(Code::BadValue, "Name: expected a string"))?;
                world
                    .set_name(id, text)
                    .map_err(|error| ToolError::new(Code::BadValue, format!("Name: {error}")))?;
            }
            "Parent" | "ClassName" => {
                return Err(
                    ToolError::bad_args(format!("{name} isn't set through props"))
                        .with_fix("object_set's parent moves an object; a class can't change"),
                );
            }
            _ => {
                let value = read_value(world, id, name, json)?;
                world
                    .set_property(id, name, value)
                    .map_err(|error| ToolError::new(Code::BadValue, format!("{name}: {error}")))?;
            }
        }
        names.push(name.clone());
    }
    Ok(names)
}

#[cfg(test)]
mod tests {
    use axcenity_core::{Color3, Vector3};

    use super::*;

    #[test]
    fn values_are_compact_world_json() {
        assert_eq!(
            show(&Value::Vector3(Vector3::new(4.0, 3.5, -0.0))),
            "[4,3.5,0]"
        );
        assert_eq!(
            show(&Value::Color3(Color3::new(0.45, 0.3, 0.2))),
            "[0.45,0.3,0.2]"
        );
        assert_eq!(show(&Value::Number(0.123_456)), "0.123");
        assert_eq!(show(&Value::String("Door \"A\"".into())), r#""Door \"A\"""#);
        assert_eq!(show(&Value::Instance(None)), "null");
    }

    #[test]
    fn only_changed_properties_differ_from_defaults() {
        let class = ClassName::Part;
        let default_size = default_of(class, "Size").unwrap();
        assert!(!differs(class, "Size", &default_size));
        assert!(differs(
            class,
            "Size",
            &Value::Vector3(Vector3::new(9.0, 9.0, 9.0))
        ));
    }
}
