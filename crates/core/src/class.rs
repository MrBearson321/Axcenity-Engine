//! Instance classes and the properties each one has.

use std::fmt;

use crate::enums::{Material, PartType, ParticleType, TextureChannel, ToneMapping};
use crate::value::{Color3, UDim2, Value, Vector2, Vector3};

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
    /// Texture maps for a Part, or for one material of a MeshPart (named by
    /// `MaterialName`, in place of the file's own).
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
    /// A Frame whose children sit on a canvas bigger than it, scrolled
    /// with the wheel, its bars, or a drag. It always clips its children.
    ScrollingFrame,
    /// A track with a thumb dragged (or the track clicked) to pick a
    /// number between `MinValue` and `MaxValue`.
    Slider,
    /// An on/off switch or checkbox, flipped by a click.
    Toggle,
    /// A box that opens a list of `Options` to pick one from.
    Dropdown,
    /// A bar filled as far as `Value` is from `MinValue` to `MaxValue`.
    /// Display only.
    ProgressBar,
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
    /// Inside a Part, MeshPart, Model, ScreenGui or UI element: plays
    /// animation on its parent, with its state machine (if it has one) and
    /// the AnimationTracks loaded from it.
    Animator,
    /// Points at a clip (`*.anim.json`). An Animator on the same parent
    /// plays it: from `animator:LoadAnimation(animation)`, or by itself
    /// when the game starts if `AutoPlay` is ticked.
    Animation,
    /// One Animation loaded on an Animator (`animator:LoadAnimation`), to
    /// play, stop and blend. Made by the engine, never saved.
    AnimationTrack,
    /// A landscape: a heightfield (kept beside the Instance, see
    /// [`TerrainData`](crate::TerrainData)) shaded by its TerrainLayers.
    Terrain,
    /// Inside a Terrain: one material it can be painted with, and the rule
    /// that lays it down by itself (by height and slope).
    TerrainLayer,
    /// Walks and runs a character kinematically during Play, and owns the
    /// camera from behind it. Parented to the character Model, or to a
    /// Part. Not a physics body.
    ThirdPersonController,
    /// The same movement as a ThirdPersonController, with the camera at the
    /// character's eyes. A character has one controller or the other.
    FirstPersonController,
    /// Walks the Model or Part it is parented to along the navigation mesh
    /// to `Destination` during Play, or straight at `Velocity`. Not a
    /// physics body.
    NavigationAgent,
    /// A place in the world a character can be put back at. Scripts make
    /// one active with `SaveService:SetCheckpoint`, which captures the game
    /// there, and `SaveService:RestoreCheckpoint` returns to it.
    Checkpoint,
}

impl ClassName {
    pub const ALL: [ClassName; 45] = [
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
        ClassName::ScrollingFrame,
        ClassName::Slider,
        ClassName::Toggle,
        ClassName::Dropdown,
        ClassName::ProgressBar,
        ClassName::BillboardGui,
        ClassName::Sound,
        ClassName::PointLight,
        ClassName::SpotLight,
        ClassName::ParticleEmitter,
        ClassName::Animator,
        ClassName::Animation,
        ClassName::AnimationTrack,
        ClassName::Terrain,
        ClassName::TerrainLayer,
        ClassName::ThirdPersonController,
        ClassName::FirstPersonController,
        ClassName::NavigationAgent,
        ClassName::Checkpoint,
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
            ClassName::ScrollingFrame => "ScrollingFrame",
            ClassName::Slider => "Slider",
            ClassName::Toggle => "Toggle",
            ClassName::Dropdown => "Dropdown",
            ClassName::ProgressBar => "ProgressBar",
            ClassName::BillboardGui => "BillboardGui",
            ClassName::Sound => "Sound",
            ClassName::PointLight => "PointLight",
            ClassName::SpotLight => "SpotLight",
            ClassName::ParticleEmitter => "ParticleEmitter",
            ClassName::Animator => "Animator",
            ClassName::Animation => "Animation",
            ClassName::AnimationTrack => "AnimationTrack",
            ClassName::Terrain => "Terrain",
            ClassName::TerrainLayer => "TerrainLayer",
            ClassName::ThirdPersonController => "ThirdPersonController",
            ClassName::FirstPersonController => "FirstPersonController",
            ClassName::NavigationAgent => "NavigationAgent",
            ClassName::Checkpoint => "Checkpoint",
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

    /// A third- or first-person character controller.
    pub fn is_character_controller(self) -> bool {
        matches!(
            self,
            ClassName::ThirdPersonController | ClassName::FirstPersonController
        )
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

    /// Slider, Toggle, Dropdown or ProgressBar: a ready-made widget whose
    /// parts are drawn (not child Instances) and whose `Value` fires
    /// `ValueChanged`.
    pub fn is_value_widget(self) -> bool {
        matches!(
            self,
            ClassName::Slider | ClassName::Toggle | ClassName::Dropdown | ClassName::ProgressBar
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
            | ClassName::Folder
            | ClassName::AnimationTrack => Vec::new(),
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
                // Classic for worlds saved without it; the editor's templates
                // set Neutral.
                ("ToneMapping", Value::ToneMapping(ToneMapping::Classic)),
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
                // Enabled ScreenGuis are drawn lowest first; ties keep tree order.
                ("DisplayOrder", Value::Number(0.0)),
                // "None", or "DeviceSafeInsets" to keep inside a phone's notch
                // and home bar (a desktop screen has none).
                ("ScreenInsets", Value::String("None".to_owned())),
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
                ("Interactable", Value::Bool(true)),
                ("Tooltip", Value::String(String::new())),
                // Siblings are drawn lowest first (ties keep tree order), and
                // the one on top takes the pointer. CSS `z-index` wins.
                ("ZIndex", Value::Number(1.0)),
                // Degrees clockwise: it and its children are drawn and hit
                // turned around its middle. Layout doesn't change.
                ("Rotation", Value::Number(0.0)),
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
                // 0 is solid text, 1 invisible. Only the text fades.
                ("TextTransparency", Value::Number(0.0)),
                // On: drawn at the largest size up to TextSize that fits.
                ("TextScaled", Value::Bool(false)),
                // On: tags in Text such as <b> and <font color="#f00"> style it.
                ("RichText", Value::Bool(false)),
                ("Visible", Value::Bool(true)),
                ("Interactable", Value::Bool(true)),
                ("Tooltip", Value::String(String::new())),
                // Siblings are drawn lowest first (ties keep tree order), and
                // the one on top takes the pointer. CSS `z-index` wins.
                ("ZIndex", Value::Number(1.0)),
                // Degrees clockwise: it and its children are drawn and hit
                // turned around its middle. Layout doesn't change.
                ("Rotation", Value::Number(0.0)),
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
                // 0 is solid text, 1 invisible. Only the text fades.
                ("TextTransparency", Value::Number(0.0)),
                // On: drawn at the largest size up to TextSize that fits.
                ("TextScaled", Value::Bool(false)),
                // On: tags in Text such as <b> and <font color="#f00"> style it.
                ("RichText", Value::Bool(false)),
                // Empty shows no picture. A path is relative to the project.
                ("Image", Value::String(String::new())),
                ("ImageTransparency", Value::Number(0.0)),
                ("Visible", Value::Bool(true)),
                // Off: greyed out by `:disabled`, takes no hover and doesn't
                // fire `Activated`.
                ("Interactable", Value::Bool(true)),
                // Shown in a small box near the pointer after a short hover.
                ("Tooltip", Value::String(String::new())),
                // Siblings are drawn lowest first (ties keep tree order), and
                // the one on top takes the pointer. CSS `z-index` wins.
                ("ZIndex", Value::Number(1.0)),
                // Degrees clockwise: it and its children are drawn and hit
                // turned around its middle. Layout doesn't change.
                ("Rotation", Value::Number(0.0)),
                // Sound files (paths in the project) played when the pointer
                // comes onto it and when it's clicked. Empty plays nothing.
                ("HoverSound", Value::String(String::new())),
                ("ClickSound", Value::String(String::new())),
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
                // 0 is solid text, 1 invisible. Only the text fades.
                ("TextTransparency", Value::Number(0.0)),
                // On: drawn at the largest size up to TextSize that fits.
                ("TextScaled", Value::Bool(false)),
                // On: tags in Text such as <b> and <font color="#f00"> style it.
                ("RichText", Value::Bool(false)),
                // Off: one line, and Enter finishes typing. On: Enter starts
                // a new line, and clicking away finishes typing.
                ("MultiLine", Value::Bool(false)),
                ("Visible", Value::Bool(true)),
                ("Interactable", Value::Bool(true)),
                ("Tooltip", Value::String(String::new())),
                // Siblings are drawn lowest first (ties keep tree order), and
                // the one on top takes the pointer. CSS `z-index` wins.
                ("ZIndex", Value::Number(1.0)),
                // Degrees clockwise: it and its children are drawn and hit
                // turned around its middle. Layout doesn't change.
                ("Rotation", Value::Number(0.0)),
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
                ("Interactable", Value::Bool(true)),
                ("Tooltip", Value::String(String::new())),
                // Siblings are drawn lowest first (ties keep tree order), and
                // the one on top takes the pointer. CSS `z-index` wins.
                ("ZIndex", Value::Number(1.0)),
                // Degrees clockwise: it and its children are drawn and hit
                // turned around its middle. Layout doesn't change.
                ("Rotation", Value::Number(0.0)),
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
                ("Interactable", Value::Bool(true)),
                ("Tooltip", Value::String(String::new())),
                // Siblings are drawn lowest first (ties keep tree order), and
                // the one on top takes the pointer. CSS `z-index` wins.
                ("ZIndex", Value::Number(1.0)),
                // Degrees clockwise: it and its children are drawn and hit
                // turned around its middle. Layout doesn't change.
                ("Rotation", Value::Number(0.0)),
                // How many cells sit in one row.
                ("Columns", Value::Number(2.0)),
            ],
            ClassName::ScrollingFrame => vec![
                ("Class", Value::String(String::new())),
                ("Id", Value::String(String::new())),
                ("Position", Value::UDim2(UDim2::default())),
                ("Size", Value::UDim2(UDim2::new(0.0, 300.0, 0.0, 200.0))),
                (
                    "BackgroundColor3",
                    Value::Color3(Color3::new(0.2, 0.2, 0.2)),
                ),
                ("BackgroundTransparency", Value::Number(0.0)),
                ("Visible", Value::Bool(true)),
                ("Interactable", Value::Bool(true)),
                ("Tooltip", Value::String(String::new())),
                ("ZIndex", Value::Number(1.0)),
                // Degrees clockwise: it and its children are drawn and hit
                // turned around its middle. Layout doesn't change.
                ("Rotation", Value::Number(0.0)),
                // The area its children are laid out on: scale of the frame's
                // own size plus pixels. Never smaller than the frame.
                ("CanvasSize", Value::UDim2(UDim2::new(0.0, 0.0, 2.0, 0.0))),
                // How far the canvas is scrolled, in pixels from its top left.
                ("CanvasPosition", Value::Vector2(Vector2::new(0.0, 0.0))),
                // "X", "Y" or "XY": which ways it scrolls.
                ("ScrollingDirection", Value::String("XY".to_owned())),
                // How thick its scroll bars are, in pixels. 0 hides them.
                ("ScrollBarThickness", Value::Number(12.0)),
                // "None", "X", "Y" or "XY": the canvas grows that way to fit
                // the children.
                ("AutomaticCanvasSize", Value::String("None".to_owned())),
            ],
            ClassName::Slider => {
                let clear = Color3::new(0.2, 0.2, 0.2);
                let mut properties =
                    widget_properties(UDim2::new(0.0, 240.0, 0.0, 24.0), clear, 1.0);
                properties.extend([
                    // The number picked, kept between MinValue and MaxValue.
                    ("Value", Value::Number(50.0)),
                    ("MinValue", Value::Number(0.0)),
                    ("MaxValue", Value::Number(100.0)),
                    // Dragging moves it in steps this big from MinValue. 0
                    // picks any number.
                    ("Step", Value::Number(1.0)),
                    // "Horizontal" (MinValue at the left) or "Vertical"
                    // (MinValue at the bottom).
                    ("Orientation", Value::String("Horizontal".to_owned())),
                ]);
                properties
            }
            ClassName::Toggle => {
                let clear = Color3::new(0.2, 0.2, 0.2);
                let mut properties =
                    widget_properties(UDim2::new(0.0, 52.0, 0.0, 28.0), clear, 1.0);
                properties.extend([
                    // On or off. A click flips it.
                    ("Value", Value::Bool(false)),
                    // "Switch" (a sliding knob) or "Checkbox" (a box ticked when on).
                    ("Style", Value::String("Switch".to_owned())),
                ]);
                properties
            }
            ClassName::Dropdown => {
                let light = Color3::new(0.96, 0.96, 0.97);
                let mut properties =
                    widget_properties(UDim2::new(0.0, 200.0, 0.0, 36.0), light, 0.0);
                properties.extend([
                    // The choices, one per line.
                    ("Options", Value::String("Low\nMedium\nHigh".to_owned())),
                    // The chosen option's text. Empty shows the Placeholder.
                    ("Value", Value::String(String::new())),
                    ("Placeholder", Value::String("Choose…".to_owned())),
                    ("TextColor3", Value::Color3(Color3::new(0.1, 0.1, 0.12))),
                    ("TextSize", Value::Number(16.0)),
                ]);
                properties
            }
            ClassName::ProgressBar => {
                let dark = Color3::new(0.2, 0.2, 0.2);
                let mut properties =
                    widget_properties(UDim2::new(0.0, 240.0, 0.0, 16.0), dark, 0.0);
                properties.extend([
                    // How far along it is, from MinValue (empty) to MaxValue (full).
                    ("Value", Value::Number(50.0)),
                    ("MinValue", Value::Number(0.0)),
                    ("MaxValue", Value::Number(100.0)),
                    // "Horizontal" (fills from the left) or "Vertical"
                    // (fills from the bottom).
                    ("Orientation", Value::String("Horizontal".to_owned())),
                ]);
                properties
            }
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
                ("Interactable", Value::Bool(true)),
                ("Tooltip", Value::String(String::new())),
                // Siblings are drawn lowest first (ties keep tree order), and
                // the one on top takes the pointer. CSS `z-index` wins.
                ("ZIndex", Value::Number(1.0)),
                // Degrees clockwise: it and its children are drawn and hit
                // turned around its middle. Layout doesn't change.
                ("Rotation", Value::Number(0.0)),
            ],
            ClassName::Animator => vec![
                // The state machine it plays, relative to the project folder,
                // such as "assets/animations/hero/hero.statemachine.json".
                // Empty plays only its AnimationTracks.
                ("StateMachine", Value::String(String::new())),
                // Off, everything it animates stays where it last was.
                ("Enabled", Value::Bool(true)),
                // Plays everything this many times as fast.
                ("Speed", Value::Number(1.0)),
            ],
            ClassName::Animation => vec![
                // The clip file, such as "assets/animations/Door/Open.anim.json".
                ("Clip", Value::String(String::new())),
                // On, the Animator on the same parent plays it when the game
                // starts.
                ("AutoPlay", Value::Bool(false)),
                ("Enabled", Value::Bool(true)),
                ("Speed", Value::Number(1.0)),
                // Whether `animation:Play()` loops it (the editor copies the
                // clip's loop when the clip is set). A track loaded with
                // LoadAnimation uses the clip's own.
                ("Looped", Value::Bool(false)),
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
                // Off, the sound is the same in both ears, wherever it sits.
                ("Spatial", Value::Bool(false)),
                // Used when no ancestor Part, MeshPart or Terrain has a Position.
                ("Position", Value::Vector3(Vector3::new(0.0, 0.0, 0.0))),
                // Full volume this close to the listener, in studs. Silent at
                // RollOffMaxDistance. Ignored while Spatial is off.
                ("RollOffMinDistance", Value::Number(10.0)),
                ("RollOffMaxDistance", Value::Number(150.0)),
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
                    // Studs one copy of a SurfaceAppearance's textures covers.
                    // 0 leaves the shape's own texture coordinates unscaled.
                    ("TileSize", Value::Number(8.0)),
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
            ClassName::Terrain => vec![
                // The middle of the terrain's footprint, at the bottom of its
                // height range.
                ("Position", Value::Vector3(Vector3::new(0.0, -32.0, 0.0))),
                // Studs along X, the full height range, and studs along Z.
                ("Size", Value::Vector3(Vector3::new(512.0, 128.0, 512.0))),
                ("CastShadow", Value::Bool(true)),
                ("CanCollide", Value::Bool(true)),
                // The folder its heights and paint are saved in, relative to
                // the project, such as "assets/terrain/Terrain". The editor
                // picks one.
                ("DataFolder", Value::String(String::new())),
            ],
            ClassName::TerrainLayer => vec![
                // The material: a tint, the texture maps (files relative to
                // the project, empty for none) and how they're read, as on a
                // SurfaceAppearance.
                ("Color", Value::Color3(Color3::new(0.45, 0.6, 0.3))),
                ("ColorMap", Value::String(String::new())),
                ("NormalMap", Value::String(String::new())),
                ("RoughnessMap", Value::String(String::new())),
                ("MetalnessMap", Value::String(String::new())),
                ("AmbientOcclusionMap", Value::String(String::new())),
                // Light means high: where layers meet, the higher one's
                // bumps show through (stones poking out of sand).
                ("HeightMap", Value::String(String::new())),
                ("Metalness", Value::Number(0.0)),
                ("Roughness", Value::Number(0.9)),
                ("NormalStrength", Value::Number(1.0)),
                ("NormalMapDirectX", Value::Bool(false)),
                // Which channel of each map holds its value, so one
                // channel-packed texture (such as an ORM map) can fill several.
                (
                    "RoughnessChannel",
                    Value::TextureChannel(TextureChannel::Red),
                ),
                ("InvertRoughness", Value::Bool(false)),
                (
                    "MetalnessChannel",
                    Value::TextureChannel(TextureChannel::Red),
                ),
                (
                    "AmbientOcclusionChannel",
                    Value::TextureChannel(TextureChannel::Red),
                ),
                // Studs one copy of the textures covers, and its turn in degrees.
                ("TileSize", Value::Number(8.0)),
                ("TextureRotation", Value::Number(0.0)),
                // Projects the textures from the side too, so cliffs don't stretch them.
                ("Triplanar", Value::Bool(false)),
                // 0 to 1: mixes in a larger, turned copy so repeats show less.
                ("AntiTiling", Value::Number(0.3)),
                // 0 to 1: how sharply layers meet where it has a HeightMap.
                ("HeightBlend", Value::Number(0.5)),
                // The automatic rule: where this layer covers the layers
                // before it. Heights are studs above the terrain's bottom,
                // slopes degrees from flat. The fades soften each edge.
                ("AutoEnabled", Value::Bool(false)),
                ("MinHeight", Value::Number(0.0)),
                ("MaxHeight", Value::Number(10000.0)),
                ("HeightFade", Value::Number(4.0)),
                ("MinSlope", Value::Number(0.0)),
                ("MaxSlope", Value::Number(90.0)),
                ("SlopeFade", Value::Number(5.0)),
                // 0 to 1: how fully it covers where the rule holds.
                ("RuleStrength", Value::Number(1.0)),
                // 0 to 1: breaks the rule's edges up with noise NoiseScale studs across.
                ("RuleNoise", Value::Number(0.3)),
                ("NoiseScale", Value::Number(24.0)),
            ],
            ClassName::ThirdPersonController => {
                let mut properties = movement_properties();
                properties.extend([
                    // How far behind the character the camera sits, in
                    // studs. The mouse wheel zooms it during Play.
                    ("CameraDistance", Value::Number(12.0)),
                    // How close and how far the wheel zooms, in studs. The
                    // same number in both locks the distance.
                    ("CameraMinZoomDistance", Value::Number(0.5)),
                    ("CameraMaxZoomDistance", Value::Number(128.0)),
                    // How far above the character's feet the camera looks, in studs.
                    ("CameraHeight", Value::Number(4.0)),
                ]);
                properties
            }
            ClassName::FirstPersonController => {
                let mut properties = movement_properties();
                // How far above the character's feet its eyes are, in studs.
                properties.push(("EyeHeight", Value::Number(4.5)));
                properties
            }
            ClassName::NavigationAgent => vec![
                // Where it walks to along the navigation mesh. A new value
                // finds a new path. The editor starts it where the agent stands.
                ("Destination", Value::Vector3(Vector3::new(0.0, 0.0, 0.0))),
                // Studs a second along its path.
                ("WalkSpeed", Value::Number(16.0)),
                // Studs a second. Not zero, it moves straight that way along
                // the ground, with no path, and Destination follows it, so
                // it stays where it stops.
                ("Velocity", Value::Vector3(Vector3::new(0.0, 0.0, 0.0))),
                // "Idle", "Moving", "Reached", "Unreachable" or "Blocked". Set by the
                // game during Play, for scripts to read.
                ("Status", Value::String("Idle".to_owned())),
                // How far it sees, in studs, and how wide, in degrees across
                // (centred on the way it faces). Colliders block sight. Zero
                // sees nothing.
                ("SightRange", Value::Number(60.0)),
                ("SightAngle", Value::Number(120.0)),
                // How far it hears a Sound playing in the player character,
                // in studs, through walls. Zero hears nothing.
                ("HearingRange", Value::Number(30.0)),
                // True while it sees, or hears, the player character. Set by
                // the game during Play, for scripts to read.
                ("SeesPlayer", Value::Bool(false)),
                ("HearsPlayer", Value::Bool(false)),
                // A behavior tree file (`.behaviortree.json`) the agent runs
                // during Play, deciding where it walks. Empty runs none, and
                // scripts set Destination themselves.
                ("BehaviorTree", Value::String(String::new())),
            ],
            ClassName::Checkpoint => vec![
                // Where a restored character stands: the middle of its feet.
                ("Position", Value::Vector3(Vector3::new(0.0, 0.0, 0.0))),
            ],
            ClassName::SurfaceAppearance => {
                let map = |name| (name, Value::String(String::new()));
                let channel = |name, channel| (name, Value::TextureChannel(channel));
                vec![
                    // On a MeshPart, the model's material this replaces.
                    // Unused on a Part.
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
                    // Where the maps sit: `Tiling` repeats them that many
                    // times as often, then `Offset` shifts them, in copies
                    // of the texture.
                    ("Tiling", Value::Vector2(Vector2::new(1.0, 1.0))),
                    ("Offset", Value::Vector2(Vector2::new(0.0, 0.0))),
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
            | ClassName::BillboardGui
            | ClassName::PointLight
            | ClassName::SpotLight
            | ClassName::ParticleEmitter
            | ClassName::Terrain
            | ClassName::TerrainLayer
            | ClassName::ThirdPersonController
            | ClassName::FirstPersonController
            | ClassName::Checkpoint => &[],
            // Fires with "Sight" or "Hearing" when it starts seeing or
            // hearing the player character.
            ClassName::NavigationAgent => &["PlayerSensed"],
            // Fires when a sound that isn't looped plays to its end.
            ClassName::Sound => &["Ended"],
            // `StateChanged` fires with the new state's name and the old
            // one's; `AnimationEvent` with an event's name, as a clip passes it.
            ClassName::Animator => &["StateChanged", "AnimationEvent"],
            // `Completed` when `animation:Play()` finishes a clip that
            // doesn't loop; `AnimationEvent` with an event's name as it passes.
            ClassName::Animation => &["Completed", "AnimationEvent"],
            // `Stopped` when it stops (after its fade); `KeyframeReached`
            // with an event's name as it passes, which
            // `GetMarkerReachedSignal(name)` hears one name of.
            ClassName::AnimationTrack => &["Stopped", "KeyframeReached"],
            // Fires on both parts when two parts start touching.
            ClassName::Part | ClassName::MeshPart => &["Touched"],
            // `OnServerEvent` fires in Scripts for each `FireServer`, and
            // `OnClientEvent` in LocalScripts for each `FireClient`.
            ClassName::RemoteEvent => &["OnServerEvent", "OnClientEvent"],
            // `MouseEnter` and `MouseLeave` fire in LocalScripts when the
            // pointer comes onto or goes off the element, as `:hover` does.
            ClassName::Frame
            | ClassName::TextLabel
            | ClassName::ImageLabel
            | ClassName::PageLayout
            | ClassName::Table
            | ClassName::ScrollingFrame => &["MouseEnter", "MouseLeave"],
            // `ValueChanged` fires on both sides, with the new value, whenever
            // `Value` changes: dragged, clicked, picked or set by a script.
            ClassName::Slider
            | ClassName::Toggle
            | ClassName::Dropdown
            | ClassName::ProgressBar => &["ValueChanged", "MouseEnter", "MouseLeave"],
            ClassName::TextButton => &["Activated", "MouseEnter", "MouseLeave"],
            // `FocusLost` fires in LocalScripts when the player finishes
            // typing: Enter on one line, or a click away (and Enter, on one
            // line only).
            ClassName::TextBox => &["FocusLost", "MouseEnter", "MouseLeave"],
        }
    }
}

/// What every widget (Slider, Toggle, Dropdown, ProgressBar) has, as other
/// GUI elements do, starting at `size` with `background` at `transparency`.
fn widget_properties(
    size: UDim2,
    background: Color3,
    transparency: f64,
) -> Vec<(&'static str, Value)> {
    vec![
        ("Class", Value::String(String::new())),
        ("Id", Value::String(String::new())),
        ("Position", Value::UDim2(UDim2::default())),
        ("Size", Value::UDim2(size)),
        ("BackgroundColor3", Value::Color3(background)),
        ("BackgroundTransparency", Value::Number(transparency)),
        ("Visible", Value::Bool(true)),
        // Off: greyed out by `:disabled`, and it can't be changed by the pointer.
        ("Interactable", Value::Bool(true)),
        ("Tooltip", Value::String(String::new())),
        // Siblings are drawn lowest first (ties keep tree order), and
        // the one on top takes the pointer. CSS `z-index` wins.
        ("ZIndex", Value::Number(1.0)),
        // Degrees clockwise: it's drawn and hit turned around its middle.
        // Layout doesn't change.
        ("Rotation", Value::Number(0.0)),
    ]
}

/// What both character controllers have: how the character moves, jumps
/// and crouches.
fn movement_properties() -> Vec<(&'static str, Value)> {
    vec![
        // "Walk", "Run" or "Crouch". Any other name is a custom mode that
        // scripts move; "" clears it back to the built-in mode it replaced.
        ("Movement", Value::String("Walk".to_owned())),
        // Studs a second across flat ground.
        ("WalkSpeed", Value::Number(16.0)),
        ("RunSpeed", Value::Number(32.0)),
        // How high a jump rises, in studs.
        ("JumpHeight", Value::Number(7.2)),
        // Set true to jump once from the ground. The controller sets
        // it back to false on its next step, in the air or not.
        ("Jump", Value::Bool(false)),
        // The steepest ground it walks up, in degrees from flat.
        ("MaxSlopeAngle", Value::Number(45.0)),
        // The tallest ledge it steps onto without jumping, in studs.
        ("MaxStepHeight", Value::Number(1.0)),
        // Set true to crouch, false to stand. Standing waits until
        // nothing overhead is in the way.
        ("Crouch", Value::Bool(false)),
        // How tall the character is while crouched, in studs.
        ("CrouchHeight", Value::Number(3.0)),
        // 0 to 1: how much of its walk or run speed it keeps crouched.
        ("CrouchSpeedScale", Value::Number(0.5)),
    ]
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
