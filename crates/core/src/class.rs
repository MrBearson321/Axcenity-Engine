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
    /// The service that holds server scripts.
    ServerScriptService,
    /// A solid shape in the world.
    Part,
    /// Lua code that runs on the server. Its source lives in a
    /// `*.server.lua` file, not in a property.
    Script,
    /// Groups the parts of one thing, such as a car or a house.
    Model,
    /// Groups Instances to keep the tree tidy. Has no effect on the game.
    Folder,
}

impl ClassName {
    pub const ALL: [ClassName; 7] = [
        ClassName::DataModel,
        ClassName::Workspace,
        ClassName::ServerScriptService,
        ClassName::Part,
        ClassName::Script,
        ClassName::Model,
        ClassName::Folder,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            ClassName::DataModel => "DataModel",
            ClassName::Workspace => "Workspace",
            ClassName::ServerScriptService => "ServerScriptService",
            ClassName::Part => "Part",
            ClassName::Script => "Script",
            ClassName::Model => "Model",
            ClassName::Folder => "Folder",
        }
    }

    /// Looks up a class by the name used in world files and scripts.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|class| class.as_str() == name)
    }

    /// Services exist exactly once and are created with the [`World`](crate::World),
    /// so they can't be created again.
    pub fn is_service(self) -> bool {
        matches!(
            self,
            ClassName::DataModel | ClassName::Workspace | ClassName::ServerScriptService
        )
    }

    /// Every property this class has, with its default value. A new Instance
    /// starts with all of these, which also fixes each property's type.
    pub fn default_properties(self) -> Vec<(&'static str, Value)> {
        match self {
            ClassName::DataModel
            | ClassName::Workspace
            | ClassName::ServerScriptService
            | ClassName::Script
            | ClassName::Model
            | ClassName::Folder => Vec::new(),
            ClassName::Part => {
                let material = Material::Plastic;
                let preset = material.preset();
                vec![
                    // Anchored parts stay put; unanchored ones fall and collide.
                    ("Anchored", Value::Bool(false)),
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

    /// The names of this class's events, such as `Touched` on a Part. Scripts
    /// reach them as `part.Touched` and connect handlers with `:Connect`.
    pub fn events(self) -> &'static [&'static str] {
        match self {
            ClassName::DataModel
            | ClassName::Workspace
            | ClassName::ServerScriptService
            | ClassName::Script
            | ClassName::Model
            | ClassName::Folder => &[],
            // Fires on both parts when two parts start touching.
            ClassName::Part => &["Touched"],
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
        for class in ClassName::ALL {
            assert_eq!(ClassName::from_name(class.as_str()), Some(class));
        }
        assert_eq!(ClassName::from_name("part"), None);
    }

    #[test]
    fn event_names_never_clash_with_property_names() {
        for class in ClassName::ALL {
            for event in class.events() {
                let clashes = class
                    .default_properties()
                    .iter()
                    .any(|(name, _)| name == event);
                assert!(!clashes, "{class}.{event} is both an event and a property");
            }
        }
    }
}
