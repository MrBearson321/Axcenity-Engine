//! Instance classes and the properties each one has.

use std::fmt;

use crate::enums::{Material, PartType, TextureChannel};
use crate::value::{Color3, Value, Vector3};

/// Events every Instance has, whatever its class. `Changed` fires with the
/// name of a property that got a new value, `ChildAdded` and `ChildRemoved`
/// with the child. Scripts reach them like class events: `part.Changed`.
pub const INSTANCE_EVENTS: [&str; 3] = ["Changed", "ChildAdded", "ChildRemoved"];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ClassName {
    /// The root of the tree, called `game` in scripts.
    DataModel,
    /// The service that holds the 3D world.
    Workspace,
    /// The service that holds a Player for everyone in the game.
    Players,
    /// The service for things both sides use, such as RemoteEvents.
    ReplicatedStorage,
    /// The service that holds server scripts.
    ServerScriptService,
    /// The service with what each player starts with. Holds
    /// StarterPlayerScripts.
    StarterPlayer,
    /// Holds the LocalScripts, which run on the player's side. Inside
    /// StarterPlayer, and can't be moved, like a service.
    StarterPlayerScripts,
    /// A solid shape in the world.
    Part,
    /// A solid 3D model imported from a glTF or FBX file (or some objects
    /// in one), which it names in `MeshId`. Drawn with the file's own materials.
    MeshPart,
    /// Lua code that runs on the server. Its source lives in a
    /// `*.server.lua` file, not in a property.
    Script,
    /// Lua code that runs on the player's side (input, camera). Its source
    /// lives in a `*.client.lua` file.
    LocalScript,
    /// Lua code that other scripts load with `require`, from a `*.module.lua`
    /// file. It runs once per side, the first time it's required.
    ModuleScript,
    /// Carries messages between Scripts and LocalScripts.
    RemoteEvent,
    /// Someone playing the game. Made by the engine when they join, in Players.
    Player,
    /// Groups the parts of one thing, such as a car or a house.
    Model,
    /// Groups Instances to keep the tree tidy. Has no effect on the game.
    Folder,
    /// Inside a MeshPart: the texture maps of one of its model's materials,
    /// which it names in `MaterialName`, in place of the file's own.
    SurfaceAppearance,
}

impl ClassName {
    pub const ALL: [ClassName; 17] = [
        ClassName::DataModel,
        ClassName::Workspace,
        ClassName::Players,
        ClassName::ReplicatedStorage,
        ClassName::ServerScriptService,
        ClassName::StarterPlayer,
        ClassName::StarterPlayerScripts,
        ClassName::Part,
        ClassName::MeshPart,
        ClassName::Script,
        ClassName::LocalScript,
        ClassName::ModuleScript,
        ClassName::RemoteEvent,
        ClassName::Player,
        ClassName::Model,
        ClassName::Folder,
        ClassName::SurfaceAppearance,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            ClassName::DataModel => "DataModel",
            ClassName::Workspace => "Workspace",
            ClassName::Players => "Players",
            ClassName::ReplicatedStorage => "ReplicatedStorage",
            ClassName::ServerScriptService => "ServerScriptService",
            ClassName::StarterPlayer => "StarterPlayer",
            ClassName::StarterPlayerScripts => "StarterPlayerScripts",
            ClassName::Part => "Part",
            ClassName::MeshPart => "MeshPart",
            ClassName::Script => "Script",
            ClassName::LocalScript => "LocalScript",
            ClassName::ModuleScript => "ModuleScript",
            ClassName::RemoteEvent => "RemoteEvent",
            ClassName::Player => "Player",
            ClassName::Model => "Model",
            ClassName::Folder => "Folder",
            ClassName::SurfaceAppearance => "SurfaceAppearance",
        }
    }

    /// Looks up a class by the name used in world files and scripts.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|class| class.as_str() == name)
    }

    /// Services exist exactly once and are created with the [`World`](crate::World),
    /// so they can't be created again. StarterPlayerScripts counts as one:
    /// it's always inside StarterPlayer.
    pub fn is_service(self) -> bool {
        matches!(
            self,
            ClassName::DataModel
                | ClassName::Workspace
                | ClassName::Players
                | ClassName::ReplicatedStorage
                | ClassName::ServerScriptService
                | ClassName::StarterPlayer
                | ClassName::StarterPlayerScripts
        )
    }

    /// Script, LocalScript or ModuleScript: Lua code kept in a file of its own.
    pub fn is_script(self) -> bool {
        matches!(
            self,
            ClassName::Script | ClassName::LocalScript | ClassName::ModuleScript
        )
    }

    /// Every property this class has, with its default value. A new Instance
    /// starts with all of these, which also fixes each property's type.
    pub fn default_properties(self) -> Vec<(&'static str, Value)> {
        match self {
            ClassName::DataModel
            | ClassName::Workspace
            | ClassName::Players
            | ClassName::ReplicatedStorage
            | ClassName::ServerScriptService
            | ClassName::StarterPlayer
            | ClassName::StarterPlayerScripts
            | ClassName::Script
            | ClassName::LocalScript
            | ClassName::ModuleScript
            | ClassName::RemoteEvent
            | ClassName::Player
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
            ClassName::MeshPart => vec![
                ("Anchored", Value::Bool(false)),
                // The model's file, relative to the project folder, such as
                // "assets/models/chair.glb". Empty means no model yet.
                ("MeshId", Value::String(String::new())),
                ("Position", Value::Vector3(Vector3::new(0.0, 0.0, 0.0))),
                ("Orientation", Value::Vector3(Vector3::new(0.0, 0.0, 0.0))),
                // The model is stretched so its bounding box is this size.
                ("Size", Value::Vector3(Vector3::new(2.0, 2.0, 2.0))),
            ],
            ClassName::SurfaceAppearance => {
                let map = |name| (name, Value::String(String::new()));
                let channel = |name, channel| (name, Value::TextureChannel(channel));
                vec![
                    // The material of the parent MeshPart's model it's for.
                    ("MaterialName", Value::String(String::new())),
                    // Multiplies the material's color.
                    ("Color", Value::Color3(Color3::new(1.0, 1.0, 1.0))),
                    // Texture files, relative to the project folder, such as
                    // "assets/textures/wood.png". Empty means none.
                    map("ColorMap"),
                    map("NormalMap"),
                    map("MetalnessMap"),
                    map("RoughnessMap"),
                    map("EmissiveMap"),
                    map("AmbientOcclusionMap"),
                    map("OpacityMap"),
                    map("SpecularMap"),
                    // How the maps are read.
                    ("NormalStrength", Value::Number(1.0)),
                    ("NormalMapDirectX", Value::Bool(false)),
                    channel("MetalnessChannel", TextureChannel::Red),
                    channel("RoughnessChannel", TextureChannel::Red),
                    ("InvertRoughness", Value::Bool(false)),
                    channel("AmbientOcclusionChannel", TextureChannel::Red),
                    ("AmbientOcclusionStrength", Value::Number(1.0)),
                    channel("OpacityChannel", TextureChannel::Alpha),
                    ("InvertOpacity", Value::Bool(false)),
                    ("EmissiveStrength", Value::Number(1.0)),
                ]
            }
        }
    }

    /// The names of this class's own events, such as `Touched` on a Part.
    /// Scripts reach them as `part.Touched` and connect handlers with
    /// `:Connect`. Every class also has the [`INSTANCE_EVENTS`].
    pub fn events(self) -> &'static [&'static str] {
        match self {
            ClassName::DataModel
            | ClassName::Workspace
            | ClassName::Players
            | ClassName::ReplicatedStorage
            | ClassName::ServerScriptService
            | ClassName::StarterPlayer
            | ClassName::StarterPlayerScripts
            | ClassName::Script
            | ClassName::LocalScript
            | ClassName::ModuleScript
            | ClassName::Player
            | ClassName::Model
            | ClassName::Folder
            | ClassName::SurfaceAppearance => &[],
            // Fires on both parts when two parts start touching.
            ClassName::Part | ClassName::MeshPart => &["Touched"],
            // `OnServerEvent` fires in Scripts for each `FireServer`, and
            // `OnClientEvent` in LocalScripts for each `FireClient`.
            ClassName::RemoteEvent => &["OnServerEvent", "OnClientEvent"],
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
    use super::{ClassName, INSTANCE_EVENTS};

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
            for event in class.events().iter().chain(&INSTANCE_EVENTS) {
                let clashes = class
                    .default_properties()
                    .iter()
                    .any(|(name, _)| name == event);
                assert!(!clashes, "{class}.{event} is both an event and a property");
                assert!(
                    !INSTANCE_EVENTS.contains(event) || !class.events().contains(event),
                    "{class} lists {event}, which every Instance already has"
                );
            }
        }
    }
}
