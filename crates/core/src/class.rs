//! Instance classes and the properties each one has.

use std::fmt;

use crate::enums::{Material, PartType};
use crate::value::{Color3, Value, Vector3};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ClassName {
    /// The root of the tree, called `game` in scripts.
    DataModel,
    /// The service that holds the 3D world.
    Workspace,
    /// A solid shape in the world.
    Part,
}

impl ClassName {
    pub fn as_str(self) -> &'static str {
        match self {
            ClassName::DataModel => "DataModel",
            ClassName::Workspace => "Workspace",
            ClassName::Part => "Part",
        }
    }

    /// Looks up a class by the name used in world files and scripts.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "DataModel" => Some(ClassName::DataModel),
            "Workspace" => Some(ClassName::Workspace),
            "Part" => Some(ClassName::Part),
            _ => None,
        }
    }

    /// Services exist exactly once and are created with the [`World`](crate::World),
    /// so they can't be created again.
    pub fn is_service(self) -> bool {
        matches!(self, ClassName::DataModel | ClassName::Workspace)
    }

    /// Every property this class has, with its default value. A new Instance
    /// starts with all of these, which also fixes each property's type.
    pub fn default_properties(self) -> Vec<(&'static str, Value)> {
        match self {
            ClassName::DataModel | ClassName::Workspace => Vec::new(),
            ClassName::Part => {
                let material = Material::Plastic;
                let preset = material.preset();
                vec![
                    ("Shape", Value::PartType(PartType::Block)),
                    ("Position", Value::Vector3(Vector3::new(0.0, 0.0, 0.0))),
                    // Rotation in degrees around X, Y and Z, applied Z first, then X, then Y.
                    ("Orientation", Value::Vector3(Vector3::new(0.0, 0.0, 0.0))),
                    ("Size", Value::Vector3(Vector3::new(4.0, 1.0, 2.0))),
                    ("Color", Value::Color3(Color3::new(0.64, 0.64, 0.64))),
                    ("Material", Value::Material(material)),
                    ("Metalness", Value::Number(preset.metalness)),
                    ("Roughness", Value::Number(preset.roughness)),
                ]
            }
        }
    }
}

impl fmt::Display for ClassName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::ClassName;

    #[test]
    fn names_round_trip() {
        for class in [ClassName::DataModel, ClassName::Workspace, ClassName::Part] {
            assert_eq!(ClassName::from_name(class.as_str()), Some(class));
        }
        assert_eq!(ClassName::from_name("part"), None);
    }
}
