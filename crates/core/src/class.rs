//! Instance classes and the properties each one has.

use std::fmt;

use crate::enums::{Material, PartType, ParticleType, TextureChannel};
use crate::value::{Color3, UDim2, Value, Vector3};

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
    /// The service for the world's lighting, such as shadows.
    Lighting,
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
    /// Root of a group of in-game UI elements.
    ScreenGui,
    /// A colored rectangular UI element.
    Frame,
    /// A UI element that displays text.
    TextLabel,
    /// A UI element that can be activated by clicking or tapping.
    TextButton,
    /// A UI element that displays an image from the project.
    ImageLabel,
    /// A box the player can type in.
    TextBox,
    /// Shows one direct child Frame at a time. Those Frames are its pages.
    PageLayout,
    /// Lays its children out in a grid, left to right, then top to bottom.
    Table,
    /// UI that floats in the world, on the part it is parented to.
    BillboardGui,
    /// Plays an audio file from the project.
    Sound,
    /// Light that shines in every direction from just above its parent part.
    PointLight,
    /// Light that shines in a cone from just above its parent part.
    SpotLight,
    /// Gives off particles (sparks, smoke, fire...) from inside its parent part.
    ParticleEmitter,
}

impl ClassName {
    pub const ALL: [ClassName; 31] = [
        ClassName::DataModel,
        ClassName::Workspace,
        ClassName::Players,
        ClassName::Lighting,
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
        ClassName::ScreenGui,
        ClassName::Frame,
        ClassName::TextLabel,
        ClassName::TextButton,
        ClassName::ImageLabel,
        ClassName::TextBox,
        ClassName::PageLayout,
        ClassName::Table,
        ClassName::BillboardGui,
        ClassName::Sound,
        ClassName::PointLight,
        ClassName::SpotLight,
        ClassName::ParticleEmitter,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            ClassName::DataModel => "DataModel",
            ClassName::Workspace => "Workspace",
            ClassName::Players => "Players",
            ClassName::Lighting => "Lighting",
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
            ClassName::ScreenGui => "ScreenGui",
            ClassName::Frame => "Frame",
            ClassName::TextLabel => "TextLabel",
            ClassName::TextButton => "TextButton",
            ClassName::ImageLabel => "ImageLabel",
            ClassName::TextBox => "TextBox",
            ClassName::PageLayout => "PageLayout",
            ClassName::Table => "Table",
            ClassName::BillboardGui => "BillboardGui",
            ClassName::Sound => "Sound",
            ClassName::PointLight => "PointLight",
            ClassName::SpotLight => "SpotLight",
            ClassName::ParticleEmitter => "ParticleEmitter",
        }
    }

    /// Looks up a class by the name used in world files and scripts.
    pub fn from_name(name: &str) -> Option<Self> {
        // "Button" is the toolbar name of a TextButton: one object, with
        // text, an image, or both. Saved projects keep the class name
        // TextButton.
        if name == "Button" {
            return Some(ClassName::TextButton);
        }
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
                | ClassName::Lighting
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
            ClassName::Workspace => vec![
                // The wind everywhere, in studs a second. It pushes particles
                // from emitters with WindAffectsDrag, and parts with
                // WindAffected.
                ("GlobalWind", Value::Vector3(Vector3::new(0.0, 0.0, 0.0))),
            ],
            ClassName::Lighting => vec![
                ("ShadowsEnabled", Value::Bool(true)),
                ("ShadowDistance", Value::Number(150.0)),
                // 0 is crisp; higher blurs edges more the further they are
                // from what casts them.
                ("ShadowSoftness", Value::Number(0.2)),
                // The procedural sky, which also lights and is reflected by parts.
                ("SkyTopColor", Value::Color3(Color3::new(0.25, 0.5, 0.9))),
                (
                    "SkyHorizonColor",
                    Value::Color3(Color3::new(0.63, 0.8, 0.95)),
                ),
                ("GroundColor", Value::Color3(Color3::new(0.35, 0.33, 0.3))),
                ("EnvironmentIntensity", Value::Number(0.6)),
                // Tone mapping and bloom.
                ("PostProcessingEnabled", Value::Bool(true)),
                ("Exposure", Value::Number(1.0)),
                ("BloomEnabled", Value::Bool(true)),
                ("BloomIntensity", Value::Number(0.35)),
                ("BloomThreshold", Value::Number(1.0)),
            ],
            ClassName::PointLight => vec![
                ("Enabled", Value::Bool(true)),
                ("Color", Value::Color3(Color3::new(1.0, 1.0, 1.0))),
                ("Brightness", Value::Number(1.0)),
                // How far the light reaches, in studs.
                ("Range", Value::Number(16.0)),
            ],
            ClassName::SpotLight => vec![
                ("Enabled", Value::Bool(true)),
                ("Color", Value::Color3(Color3::new(1.0, 1.0, 1.0))),
                ("Brightness", Value::Number(1.0)),
                ("Range", Value::Number(32.0)),
                // The width of the cone in degrees.
                ("Angle", Value::Number(60.0)),
                // Where the cone points, relative to the parent part.
                ("Direction", Value::Vector3(Vector3::new(0.0, -1.0, 0.0))),
            ],
            ClassName::ParticleEmitter => vec![
                ("Enabled", Value::Bool(true)),
                // The picture each particle shows, unless Texture names an
                // image file in the project ("assets/particles/spark.png").
                ("ParticleType", Value::ParticleType(ParticleType::Spark)),
                ("Texture", Value::String(String::new())),
                // Particles a second while Enabled.
                ("Rate", Value::Number(20.0)),
                // Seconds each particle lives (each a little more or less).
                ("Lifetime", Value::Number(2.0)),
                // Studs a second, out along Direction (relative to the
                // part), spread in a cone SpreadAngle degrees wide.
                ("Speed", Value::Number(5.0)),
                ("Direction", Value::Vector3(Vector3::new(0.0, 1.0, 0.0))),
                ("SpreadAngle", Value::Number(30.0)),
                // Added to the speed each second, in world studs (gravity is
                // (0, -196.2, 0)), and how quickly particles slow down.
                ("Acceleration", Value::Vector3(Vector3::new(0.0, 0.0, 0.0))),
                ("Drag", Value::Number(0.0)),
                // On, Drag slows particles toward the Workspace's
                // GlobalWind instead of to a stop, so the wind carries them.
                ("WindAffectsDrag", Value::Bool(false)),
                // Studs across, color and transparency at birth, each
                // blending to its End value by the end of the particle's life.
                ("Size", Value::Number(1.0)),
                ("SizeEnd", Value::Number(1.0)),
                ("Color", Value::Color3(Color3::new(1.0, 1.0, 1.0))),
                ("ColorEnd", Value::Color3(Color3::new(1.0, 1.0, 1.0))),
                ("Transparency", Value::Number(0.0)),
                ("TransparencyEnd", Value::Number(1.0)),
                // Starting turn, in degrees, and degrees a second.
                ("Rotation", Value::Number(0.0)),
                ("RotSpeed", Value::Number(0.0)),
                // 0 paints over what's behind; 1 adds light (fire, sparks).
                ("LightEmission", Value::Number(0.0)),
                // Above 1 glows (and blooms).
                ("Brightness", Value::Number(1.0)),
            ],
            ClassName::ScreenGui => vec![
                ("Class", Value::String(String::new())),
                ("Id", Value::String(String::new())),
                ("Enabled", Value::Bool(true)),
                // "Fixed" uses the viewport as it is. "Fit" lays the GUI out
                // at DesignWidth by DesignHeight, then scales it uniformly
                // so it stays proportional and inside any screen.
                ("ScaleMode", Value::String("Fixed".to_owned())),
                ("DesignWidth", Value::Number(1920.0)),
                ("DesignHeight", Value::Number(1080.0)),
            ],
            ClassName::Frame => vec![
                ("Class", Value::String(String::new())),
                ("Id", Value::String(String::new())),
                ("Position", Value::UDim2(UDim2::default())),
                ("Size", Value::UDim2(UDim2::new(0.0, 200.0, 0.0, 100.0))),
                (
                    "BackgroundColor3",
                    Value::Color3(Color3::new(0.2, 0.2, 0.2)),
                ),
                ("BackgroundTransparency", Value::Number(0.0)),
                ("Visible", Value::Bool(true)),
            ],
            ClassName::TextLabel => vec![
                ("Class", Value::String(String::new())),
                ("Id", Value::String(String::new())),
                ("Position", Value::UDim2(UDim2::default())),
                ("Size", Value::UDim2(UDim2::new(1.0, 0.0, 1.0, 0.0))),
                (
                    "BackgroundColor3",
                    Value::Color3(Color3::new(0.2, 0.2, 0.2)),
                ),
                ("BackgroundTransparency", Value::Number(1.0)),
                ("Text", Value::String("Label".to_owned())),
                ("TextColor3", Value::Color3(Color3::new(1.0, 1.0, 1.0))),
                ("TextSize", Value::Number(16.0)),
                ("Visible", Value::Bool(true)),
            ],
            ClassName::TextButton => vec![
                ("Class", Value::String(String::new())),
                ("Id", Value::String(String::new())),
                ("Position", Value::UDim2(UDim2::default())),
                ("Size", Value::UDim2(UDim2::new(0.0, 160.0, 0.0, 48.0))),
                (
                    "BackgroundColor3",
                    Value::Color3(Color3::new(0.2, 0.2, 0.2)),
                ),
                ("BackgroundTransparency", Value::Number(0.0)),
                ("Text", Value::String("Button".to_owned())),
                ("TextColor3", Value::Color3(Color3::new(1.0, 1.0, 1.0))),
                ("TextSize", Value::Number(16.0)),
                // Empty shows no picture. A path is relative to the project.
                ("Image", Value::String(String::new())),
                ("ImageTransparency", Value::Number(0.0)),
                ("Visible", Value::Bool(true)),
            ],
            ClassName::TextBox => vec![
                ("Class", Value::String(String::new())),
                ("Id", Value::String(String::new())),
                ("Position", Value::UDim2(UDim2::default())),
                ("Size", Value::UDim2(UDim2::new(0.0, 200.0, 0.0, 36.0))),
                (
                    "BackgroundColor3",
                    Value::Color3(Color3::new(0.96, 0.96, 0.97)),
                ),
                ("BackgroundTransparency", Value::Number(0.0)),
                ("Text", Value::String(String::new())),
                ("Placeholder", Value::String("Type here".to_owned())),
                ("TextColor3", Value::Color3(Color3::new(0.1, 0.1, 0.12))),
                ("TextSize", Value::Number(16.0)),
                // Off: one line, and Enter finishes typing. On: Enter starts
                // a new line, and clicking away finishes typing.
                ("MultiLine", Value::Bool(false)),
                ("Visible", Value::Bool(true)),
            ],
            ClassName::PageLayout => vec![
                ("Class", Value::String(String::new())),
                ("Id", Value::String(String::new())),
                ("Position", Value::UDim2(UDim2::default())),
                ("Size", Value::UDim2(UDim2::new(1.0, 0.0, 1.0, 0.0))),
                (
                    "BackgroundColor3",
                    Value::Color3(Color3::new(0.2, 0.2, 0.2)),
                ),
                ("BackgroundTransparency", Value::Number(1.0)),
                ("Visible", Value::Bool(true)),
                // Empty shows the first child Frame. Any other value is that
                // Frame's name.
                ("CurrentPage", Value::String(String::new())),
            ],
            ClassName::Table => vec![
                ("Class", Value::String(String::new())),
                ("Id", Value::String(String::new())),
                ("Position", Value::UDim2(UDim2::default())),
                ("Size", Value::UDim2(UDim2::new(0.0, 400.0, 0.0, 160.0))),
                (
                    "BackgroundColor3",
                    Value::Color3(Color3::new(0.2, 0.2, 0.2)),
                ),
                ("BackgroundTransparency", Value::Number(0.0)),
                ("Visible", Value::Bool(true)),
                // How many cells sit in one row.
                ("Columns", Value::Number(2.0)),
            ],
            ClassName::BillboardGui => vec![
                ("Class", Value::String(String::new())),
                ("Id", Value::String(String::new())),
                ("Enabled", Value::Bool(true)),
                ("Size", Value::UDim2(UDim2::new(0.0, 200.0, 0.0, 80.0))),
                // Added to the parent part's position, in world studs.
                ("StudsOffset", Value::Vector3(Vector3::new(0.0, 2.0, 0.0))),
                // 0 shows it at any distance. Otherwise it hides past this
                // many studs from the camera.
                ("MaxDistance", Value::Number(0.0)),
            ],
            ClassName::ImageLabel => vec![
                ("Class", Value::String(String::new())),
                ("Id", Value::String(String::new())),
                ("Position", Value::UDim2(UDim2::default())),
                ("Size", Value::UDim2(UDim2::new(0.0, 100.0, 0.0, 100.0))),
                (
                    "BackgroundColor3",
                    Value::Color3(Color3::new(0.2, 0.2, 0.2)),
                ),
                ("BackgroundTransparency", Value::Number(1.0)),
                ("Image", Value::String(String::new())),
                ("ImageTransparency", Value::Number(0.0)),
                ("Visible", Value::Bool(true)),
            ],
            ClassName::Sound => vec![
                ("SoundId", Value::String(String::new())),
                ("Volume", Value::Number(1.0)),
                ("Looped", Value::Bool(false)),
                // 1 is the file's own speed; 2 plays twice as fast and higher.
                ("PlaybackSpeed", Value::Number(1.0)),
                // True while it plays. Saved true, it plays when the game
                // starts; set false to pause and true to carry on.
                ("Playing", Value::Bool(false)),
            ],
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
                    // Off, the part casts no sun shadow (it's still shaded).
                    ("CastShadow", Value::Bool(true)),
                    // 0 is as solid as the material, 1 invisible.
                    ("Transparency", Value::Number(0.0)),
                    // Off, other parts pass through it (it still fires Touched).
                    ("CanCollide", Value::Bool(true)),
                    // On, the Workspace's GlobalWind pushes the part (if
                    // it isn't anchored).
                    ("WindAffected", Value::Bool(false)),
                    // The part glows in EmissiveColor, Emission times as
                    // brightly as a plain white surface. 0 is no glow.
                    ("EmissiveColor", Value::Color3(Color3::new(1.0, 1.0, 1.0))),
                    ("Emission", Value::Number(0.0)),
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
                ("CastShadow", Value::Bool(true)),
                ("Transparency", Value::Number(0.0)),
                ("CanCollide", Value::Bool(true)),
                ("WindAffected", Value::Bool(false)),
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
            | ClassName::Lighting
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
            | ClassName::SurfaceAppearance
            | ClassName::ScreenGui
            | ClassName::Frame
            | ClassName::TextLabel
            | ClassName::ImageLabel
            | ClassName::PageLayout
            | ClassName::Table
            | ClassName::BillboardGui
            | ClassName::PointLight
            | ClassName::SpotLight
            | ClassName::ParticleEmitter => &[],
            // Fires when a sound that isn't looped plays to its end.
            ClassName::Sound => &["Ended"],
            // Fires on both parts when two parts start touching.
            ClassName::Part | ClassName::MeshPart => &["Touched"],
            // `OnServerEvent` fires in Scripts for each `FireServer`, and
            // `OnClientEvent` in LocalScripts for each `FireClient`.
            ClassName::RemoteEvent => &["OnServerEvent", "OnClientEvent"],
            ClassName::TextButton => &["Activated"],
            // Fires in LocalScripts when the player finishes typing: Enter
            // on one line, or a click away (and Enter, on one line only).
            ClassName::TextBox => &["FocusLost"],
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
        assert_eq!(ClassName::from_name("Button"), Some(ClassName::TextButton));
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
