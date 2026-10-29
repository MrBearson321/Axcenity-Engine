//! Instance classes and the properties each one has.

use std::fmt;

use crate::enums::{Material, PartType, ParticleType, TextureChannel, ToneMapping};
use crate::value::{CFrame, Color3, EnumItem, UDim2, Value, Vector2, Vector3};

/// Events every Instance has, whatever its class. `Changed` fires with the
/// name of a property that got a new value, `ChildAdded` and `ChildRemoved`
/// with the child. Scripts reach them like class events: `part.Changed`.
/// `Lighting.ClockTime` and `GeographicLatitude` for worlds saved before
/// they existed: on this day, at this place, the sun is where it always
/// was (toward 0.5, 1, 0.25), so those worlds look as they did.
pub const DEFAULT_CLOCK_TIME: f64 = 10.2748873292782;
pub const DEFAULT_LATITUDE: f64 = 14.0362434679265;

pub const INSTANCE_EVENTS: [&str; 8] = [
    "Changed",
    "ChildAdded",
    "ChildRemoved",
    "DescendantAdded",
    "DescendantRemoving",
    "AncestryChanged",
    "Destroying",
    // Fires with an attribute's name when `SetAttribute` changes it.
    "AttributeChanged",
];

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
    /// The service for things only the server uses, such as Models a
    /// Script clones into the Workspace. Never sent to players' devices
    /// (Phase 64).
    ServerStorage,
    /// The service with what each player starts with. Holds
    /// StarterPlayerScripts.
    StarterPlayer,
    /// Holds the LocalScripts, which run on the player's side. Inside
    /// StarterPlayer, and can't be moved, like a service.
    StarterPlayerScripts,
    /// The service that holds the Teams (Phase 47).
    Teams,
    /// The service that holds the SoundGroups (Phase 52): the game's
    /// audio buses.
    SoundService,
    /// The default chat (Phase 67): `ChatEnabled`, `MessageReceived`,
    /// `SendMessage`, `DisplaySystemMessage` and `OnIncomingMessage`.
    TextChatService,
    /// A side players are on, in Teams: they spawn on the SpawnLocations
    /// of its `TeamColor`.
    Team,
    /// A solid shape in the world.
    Part,
    /// A solid 3D model imported from a glTF or FBX file (or some objects
    /// in one), which it names in `MeshId`. Drawn with the file's own materials.
    MeshPart,
    /// Parts combined into one solid by Union, Negate or Intersect (Phase
    /// 50): drawn and collided as the model in `MeshId` (the result,
    /// saved by the editor), with the parts it was made of inside it,
    /// hidden, for Separate.
    UnionOperation,
    /// A Part that cuts itself out of the parts it's unioned with (Phase
    /// 50). Shown pink and see-through in the editor.
    NegateOperation,
    /// A Part that characters sit on: one that touches it while it's empty
    /// and enabled sits down (`Occupant`).
    Seat,
    /// A Seat that drives the car it's welded into: while someone sits on
    /// it, the Move action sets `Throttle` and `Steer`, and its
    /// VehicleWheels push, turn and hold the car.
    VehicleSeat,
    /// A Part that hurts (Phase 43): every Health whose Model has a part
    /// inside it loses `DamagePerSecond` a second. Anchored, see-through
    /// and walked through by default.
    DamageZone,
    /// A Part the listener walks into (Phase 52): while the camera is
    /// inside it, its `ReverbType` preset blends onto the spatial Sounds
    /// over `FadeTime` seconds. Anchored, see-through and walked through.
    ReverbZone,
    /// A Part characters spawn on (Phase 47): an `Enabled` one of their
    /// team's `TeamColor`, or a `Neutral` one, chosen at random. For
    /// `Duration` seconds after, a ForceField keeps them from harm.
    SpawnLocation,
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
    /// Asks the other side and waits for its answer: a LocalScript's
    /// `InvokeServer` returns what the server's `OnServerInvoke` returns.
    RemoteFunction,
    /// An event scripts on one side fire for each other (`Fire`, `Event`).
    BindableEvent,
    /// A function scripts on one side call for each other (`Invoke`, `OnInvoke`).
    BindableFunction,
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
    /// An audio bus (Phase 52): the Sounds whose `SoundGroup` is it, and
    /// the SoundGroups inside it, mixed at its `Volume` through its effects.
    SoundGroup,
    /// Inside a Sound or SoundGroup: makes it sound like a room (Phase 52).
    ReverbSoundEffect,
    /// Inside a Sound or SoundGroup: raises or lowers its lows, mids and
    /// highs.
    EqualizerSoundEffect,
    /// Inside a Sound or SoundGroup: turns down what's louder than
    /// `Threshold`.
    CompressorSoundEffect,
    /// Inside a Sound or SoundGroup: repeats it after `Delay` seconds.
    EchoSoundEffect,
    /// Inside a Sound or SoundGroup: overdrives it.
    DistortionSoundEffect,
    /// Inside a Sound or SoundGroup: raises or lowers its pitch without
    /// changing its speed.
    PitchShiftSoundEffect,
    /// Inside a Sound or SoundGroup: lets through only what's below `Cutoff`.
    LowPassSoundEffect,
    /// Inside a Sound or SoundGroup: lets through only what's above `Cutoff`.
    HighPassSoundEffect,
    /// Light that shines in every direction from just above its parent part.
    PointLight,
    /// Light that shines in a cone from just above its parent part.
    SpotLight,
    /// Inside a Part: what's in the part's box reflects the world as the
    /// editor baked it from the part's middle (`Bake`), not the sky.
    ReflectionProbe,
    /// Inside a Part: a grid of light probes filling the part's box, which
    /// light what's in it with the light the editor baked around them
    /// (`Bake`), not the sky's.
    LightProbeVolume,
    /// Inside Lighting: the sky is drawn as light scattered by air (sunsets
    /// redden it, night darkens it) in place of its colors, and colors the
    /// sun's light. It also lights and is reflected by surfaces.
    Atmosphere,
    /// Inside Lighting: a layer of cloud high in the sky, lit by the sun,
    /// whose shadows fall on the world. The Workspace's `GlobalWind`
    /// carries it along.
    Clouds,
    /// Inside Lighting: rain or snow falling around the camera, and how wet
    /// and snow-covered every surface is. Changes ease in over
    /// `TransitionTime`.
    Weather,
    /// Inside Lighting: lines drawn along the edges of everything solid
    /// (silhouettes and creases), found on screen, with no setup per part.
    OutlineEffect,
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
    /// Plays a cutscene sequence (`*.sequence.json`, Phase 51): its camera
    /// cuts, property keys, clips, sounds, events, fades, letterbox,
    /// subtitles and levels, from `Play` until `Completed`.
    Sequence,
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
    /// A named viewpoint in the world, which scripts cut to with
    /// `workspace.CurrentCamera.CFrame = viewpoint.CFrame`. Not drawn and
    /// not a body.
    Camera,
    /// A point and a direction on the part it's parented to, which moves
    /// with the part. `Position` and `Orientation` are relative to the
    /// part; scripts also read `WorldCFrame`. Not drawn in the game.
    Attachment,
    /// A wheel of the car whose chassis part it's parented to: where it is
    /// (`Position` and `Orientation`, relative to the part, as an
    /// Attachment's), how big, how springy, and whether it steers, drives
    /// and brakes. It rolls toward its -Z, turning about its X. Not drawn;
    /// its `WheelPart` is moved to it.
    VehicleWheel,
    /// A ragdoll for the skinned MeshPart it's parented to (Phase 41): a
    /// capsule Part per main bone (named after the bone) and a
    /// BallSocketConstraint per joint, made by Build in the editor. While
    /// `Enabled`, physics moves the capsules and they pose the bones;
    /// otherwise they follow the animation and physics leaves them out.
    Ragdoll,
    /// Breaks the Part or MeshPart it's parented to into pieces (Phase 42):
    /// Generate in the editor cuts it into `Pieces` Voronoi pieces, saved
    /// as `<name>.fracture.glb` (`PiecesId`). An impact faster than
    /// `Threshold`, or `Shatter(point, force)`, replaces the part with its
    /// pieces, which fly from the point and fade after `DebrisLifetime`.
    Fracture,
    /// How much a Model can take (Phase 43): `Health` out of `MaxHealth`,
    /// lost to `TakeDamage`, damage zones and falls, won back by `Heal`
    /// and by regenerating `RegenRate` a second once `RegenDelay` seconds
    /// pass without damage. At 0 it fires `Died` once.
    Health,
    /// A prompt to do something, under a part or an Attachment (Phase 44):
    /// when the player character comes within `MaxActivationDistance`, the
    /// nearest one shows `ActionText` and `ObjectText` with the key or
    /// button bound to its input map `Action`, and holding that for
    /// `HoldDuration` seconds fires `Triggered`.
    ProximityPrompt,
    /// Makes the part it's parented to clickable (Phase 44): `MouseClick`
    /// and `RightMouseClick` fire when the mouse clicks it from within
    /// `MaxActivationDistance`, and the cursor turns to `CursorIcon` over it.
    ClickDetector,
    /// Something a character can carry (Phase 45), a Model of parts with
    /// one `Handle`: touching a dropped one puts it in the character's
    /// Inventory, and equipping holds it in the right hand by its `Grip`.
    /// Fires `Equipped`, `Unequipped`, `Activated` (the Primary action) and
    /// `Deactivated`.
    Tool,
    /// The Tools a character carries (Phase 45), put in the character's
    /// Model: up to `Capacity` of them, as its children, and the one in
    /// hand (`Equipped`). The default hotbar shows them while
    /// `HotbarEnabled` is on.
    Inventory,
    /// A Roblox-familiar front for a character (Phase 46), in its Model:
    /// it finds (or makes) the Model's controller, Health, Inventory and
    /// Animator, and its properties read and write theirs. It stores none
    /// of its own but `BreakJointsOnDeath` and `RagdollOnDeath`. Fires
    /// `MoveToFinished`, `StateChanged` and `Died`.
    Humanoid,
    /// Keeps the Health of the Model it's in from losing any (Phase 47).
    /// A SpawnLocation puts one on each character it spawns, for its
    /// `Duration`.
    ForceField,
    /// Holds `Part0` and `Part1` together where they were when it was made
    /// or enabled. Welded unanchored parts move as one body: an assembly.
    WeldConstraint,
    /// Lets `Attachment1`'s part turn around `Attachment0`'s X axis (its
    /// `Axis`), within limits, and turns it with a motor or a servo.
    HingeConstraint,
    /// Holds the two attachments together and lets them turn freely, within
    /// a cone and a twist when limited.
    BallSocketConstraint,
    /// Lets the parts slide along `Attachment0`'s X axis only, within limits,
    /// and slides them with a motor or a servo.
    PrismaticConstraint,
    /// Lets the parts slide along, and turn around, `Attachment0`'s X axis,
    /// each with its own limits, motor or servo.
    CylindricalConstraint,
    /// Pulls or pushes the attachments toward `FreeLength` apart, kept between
    /// `MinLength` and `MaxLength`.
    SpringConstraint,
    /// Keeps the attachments at most `Length` apart, and reels the rope in or
    /// out with a winch.
    RopeConstraint,
    /// Keeps the attachments exactly `Length` apart; they turn freely.
    RodConstraint,
    /// Pushes `Attachment0`'s part to move at `VectorVelocity`, with at most
    /// `MaxForce`.
    LinearVelocity,
    /// Turns `Attachment0`'s part to spin at `AngularVelocity`, with at most
    /// `MaxTorque`.
    AngularVelocity,
    /// Pushes `Attachment0`'s part with a constant `Force`.
    VectorForce,
    /// Turns `Attachment0`'s part with a constant `Torque`.
    Torque,
    /// Moves `Attachment0` toward `Attachment1`, or toward `Position`.
    AlignPosition,
    /// Turns `Attachment0` to face the way `Attachment1` does, or `CFrame`.
    AlignOrientation,
    IntValue,
    NumberValue,
    StringValue,
    BoolValue,
    Vector3Value,
    Color3Value,
    CFrameValue,
    ObjectValue,
}

impl ClassName {
    pub const ALL: [ClassName; 110] = [
        ClassName::DataModel,
        ClassName::Workspace,
        ClassName::Players,
        ClassName::Lighting,
        ClassName::ReplicatedStorage,
        ClassName::ServerScriptService,
        ClassName::ServerStorage,
        ClassName::StarterPlayer,
        ClassName::StarterPlayerScripts,
        ClassName::Part,
        ClassName::MeshPart,
        ClassName::UnionOperation,
        ClassName::NegateOperation,
        ClassName::Seat,
        ClassName::VehicleSeat,
        ClassName::DamageZone,
        ClassName::ReverbZone,
        ClassName::SpawnLocation,
        ClassName::Script,
        ClassName::LocalScript,
        ClassName::ModuleScript,
        ClassName::RemoteEvent,
        ClassName::RemoteFunction,
        ClassName::BindableEvent,
        ClassName::BindableFunction,
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
        ClassName::SoundGroup,
        ClassName::ReverbSoundEffect,
        ClassName::EqualizerSoundEffect,
        ClassName::CompressorSoundEffect,
        ClassName::EchoSoundEffect,
        ClassName::DistortionSoundEffect,
        ClassName::PitchShiftSoundEffect,
        ClassName::LowPassSoundEffect,
        ClassName::HighPassSoundEffect,
        ClassName::PointLight,
        ClassName::SpotLight,
        ClassName::ReflectionProbe,
        ClassName::LightProbeVolume,
        ClassName::Atmosphere,
        ClassName::Clouds,
        ClassName::Weather,
        ClassName::OutlineEffect,
        ClassName::ParticleEmitter,
        ClassName::Animator,
        ClassName::Animation,
        ClassName::AnimationTrack,
        ClassName::Sequence,
        ClassName::Terrain,
        ClassName::TerrainLayer,
        ClassName::ThirdPersonController,
        ClassName::FirstPersonController,
        ClassName::NavigationAgent,
        ClassName::Checkpoint,
        ClassName::Camera,
        ClassName::Attachment,
        ClassName::VehicleWheel,
        ClassName::Ragdoll,
        ClassName::Fracture,
        ClassName::Health,
        ClassName::ProximityPrompt,
        ClassName::ClickDetector,
        ClassName::Tool,
        ClassName::Inventory,
        ClassName::Humanoid,
        ClassName::ForceField,
        ClassName::Teams,
        ClassName::Team,
        ClassName::SoundService,
        ClassName::TextChatService,
        ClassName::WeldConstraint,
        ClassName::HingeConstraint,
        ClassName::BallSocketConstraint,
        ClassName::PrismaticConstraint,
        ClassName::CylindricalConstraint,
        ClassName::SpringConstraint,
        ClassName::RopeConstraint,
        ClassName::RodConstraint,
        ClassName::LinearVelocity,
        ClassName::AngularVelocity,
        ClassName::VectorForce,
        ClassName::Torque,
        ClassName::AlignPosition,
        ClassName::AlignOrientation,
        ClassName::IntValue,
        ClassName::NumberValue,
        ClassName::StringValue,
        ClassName::BoolValue,
        ClassName::Vector3Value,
        ClassName::Color3Value,
        ClassName::CFrameValue,
        ClassName::ObjectValue,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            ClassName::DataModel => "DataModel",
            ClassName::Workspace => "Workspace",
            ClassName::Players => "Players",
            ClassName::Lighting => "Lighting",
            ClassName::ReplicatedStorage => "ReplicatedStorage",
            ClassName::ServerScriptService => "ServerScriptService",
            ClassName::ServerStorage => "ServerStorage",
            ClassName::StarterPlayer => "StarterPlayer",
            ClassName::StarterPlayerScripts => "StarterPlayerScripts",
            ClassName::Teams => "Teams",
            ClassName::SoundService => "SoundService",
            ClassName::TextChatService => "TextChatService",
            ClassName::Team => "Team",
            ClassName::Part => "Part",
            ClassName::Seat => "Seat",
            ClassName::VehicleSeat => "VehicleSeat",
            ClassName::DamageZone => "DamageZone",
            ClassName::ReverbZone => "ReverbZone",
            ClassName::SpawnLocation => "SpawnLocation",
            ClassName::MeshPart => "MeshPart",
            ClassName::UnionOperation => "UnionOperation",
            ClassName::NegateOperation => "NegateOperation",
            ClassName::Script => "Script",
            ClassName::LocalScript => "LocalScript",
            ClassName::ModuleScript => "ModuleScript",
            ClassName::RemoteEvent => "RemoteEvent",
            ClassName::RemoteFunction => "RemoteFunction",
            ClassName::BindableEvent => "BindableEvent",
            ClassName::BindableFunction => "BindableFunction",
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
            ClassName::SoundGroup => "SoundGroup",
            ClassName::ReverbSoundEffect => "ReverbSoundEffect",
            ClassName::EqualizerSoundEffect => "EqualizerSoundEffect",
            ClassName::CompressorSoundEffect => "CompressorSoundEffect",
            ClassName::EchoSoundEffect => "EchoSoundEffect",
            ClassName::DistortionSoundEffect => "DistortionSoundEffect",
            ClassName::PitchShiftSoundEffect => "PitchShiftSoundEffect",
            ClassName::LowPassSoundEffect => "LowPassSoundEffect",
            ClassName::HighPassSoundEffect => "HighPassSoundEffect",
            ClassName::PointLight => "PointLight",
            ClassName::SpotLight => "SpotLight",
            ClassName::ReflectionProbe => "ReflectionProbe",
            ClassName::LightProbeVolume => "LightProbeVolume",
            ClassName::Atmosphere => "Atmosphere",
            ClassName::Clouds => "Clouds",
            ClassName::Weather => "Weather",
            ClassName::OutlineEffect => "OutlineEffect",
            ClassName::ParticleEmitter => "ParticleEmitter",
            ClassName::Animator => "Animator",
            ClassName::Animation => "Animation",
            ClassName::AnimationTrack => "AnimationTrack",
            ClassName::Sequence => "Sequence",
            ClassName::Terrain => "Terrain",
            ClassName::TerrainLayer => "TerrainLayer",
            ClassName::ThirdPersonController => "ThirdPersonController",
            ClassName::FirstPersonController => "FirstPersonController",
            ClassName::NavigationAgent => "NavigationAgent",
            ClassName::Checkpoint => "Checkpoint",
            ClassName::Camera => "Camera",
            ClassName::Attachment => "Attachment",
            ClassName::VehicleWheel => "VehicleWheel",
            ClassName::Ragdoll => "Ragdoll",
            ClassName::Fracture => "Fracture",
            ClassName::Health => "Health",
            ClassName::ProximityPrompt => "ProximityPrompt",
            ClassName::ClickDetector => "ClickDetector",
            ClassName::Tool => "Tool",
            ClassName::Inventory => "Inventory",
            ClassName::Humanoid => "Humanoid",
            ClassName::ForceField => "ForceField",
            ClassName::WeldConstraint => "WeldConstraint",
            ClassName::HingeConstraint => "HingeConstraint",
            ClassName::BallSocketConstraint => "BallSocketConstraint",
            ClassName::PrismaticConstraint => "PrismaticConstraint",
            ClassName::CylindricalConstraint => "CylindricalConstraint",
            ClassName::SpringConstraint => "SpringConstraint",
            ClassName::RopeConstraint => "RopeConstraint",
            ClassName::RodConstraint => "RodConstraint",
            ClassName::LinearVelocity => "LinearVelocity",
            ClassName::AngularVelocity => "AngularVelocity",
            ClassName::VectorForce => "VectorForce",
            ClassName::Torque => "Torque",
            ClassName::AlignPosition => "AlignPosition",
            ClassName::AlignOrientation => "AlignOrientation",
            ClassName::IntValue => "IntValue",
            ClassName::NumberValue => "NumberValue",
            ClassName::StringValue => "StringValue",
            ClassName::BoolValue => "BoolValue",
            ClassName::Vector3Value => "Vector3Value",
            ClassName::Color3Value => "Color3Value",
            ClassName::CFrameValue => "CFrameValue",
            ClassName::ObjectValue => "ObjectValue",
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

    /// The abstract classes this one belongs to, nearest first, as in
    /// Roblox: a MeshPart is a BasePart, a LocalScript is a Script. Every
    /// class is also an `Instance`.
    pub fn superclasses(self) -> &'static [&'static str] {
        const PART: &[&str] = &["BasePart", "PVInstance"];
        const GUI: &[&str] = &["GuiObject", "GuiBase2d"];
        const LAYER: &[&str] = &["LayerCollector", "GuiBase2d"];
        match self {
            ClassName::Part | ClassName::MeshPart => PART,
            ClassName::UnionOperation | ClassName::NegateOperation => &[
                "PartOperation",
                "TriangleMeshPart",
                "BasePart",
                "PVInstance",
            ],
            ClassName::Seat
            | ClassName::DamageZone
            | ClassName::ReverbZone
            | ClassName::SpawnLocation => &["Part", "BasePart", "PVInstance"],
            ClassName::VehicleSeat => &["Seat", "Part", "BasePart", "PVInstance"],
            ClassName::Model => &["PVInstance"],
            ClassName::Tool => &["Model", "PVInstance"],
            ClassName::Script | ClassName::ModuleScript => &["LuaSourceContainer"],
            ClassName::LocalScript => &["Script", "LuaSourceContainer"],
            ClassName::TextButton => &["GuiButton", "GuiObject", "GuiBase2d"],
            ClassName::ScreenGui | ClassName::BillboardGui => LAYER,
            ClassName::PointLight | ClassName::SpotLight => &["Light"],
            class if class.is_joint() || class.is_mover() => &["Constraint"],
            class if class.is_sound_effect() => &["SoundEffect"],
            class if class.is_value_object() => &["ValueBase"],
            class if class.is_screen_element() => GUI,
            _ => &[],
        }
    }

    /// Whether this class is `name` or inherits from it (`IsA` in scripts).
    pub fn is_a(self, name: &str) -> bool {
        name == "Instance" || self.as_str() == name || self.superclasses().contains(&name)
    }

    /// A Part, MeshPart, UnionOperation, NegateOperation, Seat, VehicleSeat,
    /// DamageZone or SpawnLocation: a solid in the world (`BasePart`).
    pub fn is_base_part(self) -> bool {
        self.has_model() || self.is_part()
    }

    /// A MeshPart or UnionOperation: drawn and collided as the model its
    /// `MeshId` names, stretched to its `Size`, instead of by a `Shape`.
    pub fn has_model(self) -> bool {
        matches!(self, ClassName::MeshPart | ClassName::UnionOperation)
    }

    /// A Part or one of the classes that are Parts: NegateOperation, Seat,
    /// VehicleSeat, DamageZone, ReverbZone and SpawnLocation. They have a
    /// `Shape`, a `Material` and the rest.
    pub fn is_part(self) -> bool {
        matches!(
            self,
            ClassName::Part
                | ClassName::NegateOperation
                | ClassName::Seat
                | ClassName::VehicleSeat
                | ClassName::DamageZone
                | ClassName::ReverbZone
                | ClassName::SpawnLocation
        )
    }

    /// A sound effect (Phase 52), applied to the Sound or SoundGroup it's
    /// inside, in `Priority` order.
    pub fn is_sound_effect(self) -> bool {
        matches!(
            self,
            ClassName::ReverbSoundEffect
                | ClassName::EqualizerSoundEffect
                | ClassName::CompressorSoundEffect
                | ClassName::EchoSoundEffect
                | ClassName::DistortionSoundEffect
                | ClassName::PitchShiftSoundEffect
                | ClassName::LowPassSoundEffect
                | ClassName::HighPassSoundEffect
        )
    }

    /// A Seat or a VehicleSeat: a character that touches it sits on it.
    pub fn is_seat(self) -> bool {
        matches!(self, ClassName::Seat | ClassName::VehicleSeat)
    }

    /// A third- or first-person character controller.
    pub fn is_character_controller(self) -> bool {
        matches!(
            self,
            ClassName::ThirdPersonController | ClassName::FirstPersonController
        )
    }

    /// A constraint between two Attachments that physics turns into a joint
    /// (Phase 36): a hinge, ball socket, prismatic, cylindrical, spring,
    /// rope or rod.
    pub fn is_joint(self) -> bool {
        matches!(
            self,
            ClassName::HingeConstraint
                | ClassName::BallSocketConstraint
                | ClassName::PrismaticConstraint
                | ClassName::CylindricalConstraint
                | ClassName::SpringConstraint
                | ClassName::RopeConstraint
                | ClassName::RodConstraint
        )
    }

    /// A constraint that pushes or turns `Attachment0`'s part (Phase 37):
    /// a linear or angular velocity, a force, a torque, or an align.
    pub fn is_mover(self) -> bool {
        matches!(
            self,
            ClassName::LinearVelocity
                | ClassName::AngularVelocity
                | ClassName::VectorForce
                | ClassName::Torque
                | ClassName::AlignPosition
                | ClassName::AlignOrientation
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
                | ClassName::ServerStorage
                | ClassName::StarterPlayer
                | ClassName::StarterPlayerScripts
                | ClassName::Teams
                | ClassName::SoundService
                | ClassName::TextChatService
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

    /// IntValue, NumberValue, StringValue, BoolValue, Vector3Value,
    /// Color3Value, CFrameValue or ObjectValue: one `Value`, whose
    /// `Changed` fires with the new value (not the property's name).
    pub fn is_value_object(self) -> bool {
        matches!(
            self,
            ClassName::IntValue
                | ClassName::NumberValue
                | ClassName::StringValue
                | ClassName::BoolValue
                | ClassName::Vector3Value
                | ClassName::Color3Value
                | ClassName::CFrameValue
                | ClassName::ObjectValue
        )
    }

    /// Script, LocalScript or ModuleScript: Lua code kept in a file of its own.
    pub fn is_script(self) -> bool {
        matches!(
            self,
            ClassName::Script | ClassName::LocalScript | ClassName::ModuleScript
        )
    }

    /// A game UI class: ScreenGui, BillboardGui or a screen element. Game
    /// UI settings (reduced motion) apply to these.
    pub fn is_gui(self) -> bool {
        matches!(self, ClassName::ScreenGui | ClassName::BillboardGui) || self.is_screen_element()
    }

    /// A screen element that keyboard and controller navigation can move
    /// through: every GUI element but ScreenGui and BillboardGui.
    pub fn is_screen_element(self) -> bool {
        matches!(
            self,
            ClassName::Frame
                | ClassName::TextLabel
                | ClassName::TextButton
                | ClassName::ImageLabel
                | ClassName::TextBox
                | ClassName::PageLayout
                | ClassName::Table
                | ClassName::ScrollingFrame
        ) || self.is_value_widget()
    }

    /// Whether navigation focuses this class unless its `Selectable` is
    /// turned off: buttons, text boxes and the widgets that can be changed.
    pub fn selectable_by_default(self) -> bool {
        matches!(
            self,
            ClassName::TextButton
                | ClassName::TextBox
                | ClassName::Slider
                | ClassName::Toggle
                | ClassName::Dropdown
        )
    }

    /// Every property this class has, with its default value. A new Instance
    /// starts with all of these, which also fixes each property's type.
    pub fn default_properties(self) -> Vec<(&'static str, Value)> {
        let mut properties = self.own_properties();
        if self.is_screen_element() {
            properties.extend([
                // Whether keyboard and controller navigation can focus it.
                ("Selectable", Value::Bool(self.selectable_by_default())),
                // The `Id` of the element focus moves to on that arrow, or
                // empty for the nearest selectable element that way.
                ("NextSelectionUp", Value::String(String::new())),
                ("NextSelectionDown", Value::String(String::new())),
                ("NextSelectionLeft", Value::String(String::new())),
                ("NextSelectionRight", Value::String(String::new())),
                // What navigation and scripts call it; empty is its `Name`.
                ("AccessibleName", Value::String(String::new())),
            ]);
        }
        properties
    }

    fn own_properties(self) -> Vec<(&'static str, Value)> {
        match self {
            ClassName::DataModel
            | ClassName::ReplicatedStorage
            | ClassName::ServerScriptService
            | ClassName::ServerStorage
            | ClassName::StarterPlayerScripts
            | ClassName::Teams
            | ClassName::SoundService
            | ClassName::ForceField
            | ClassName::Script
            | ClassName::LocalScript
            | ClassName::ModuleScript
            | ClassName::RemoteEvent
            | ClassName::RemoteFunction
            | ClassName::BindableEvent
            | ClassName::BindableFunction
            | ClassName::Model
            | ClassName::Folder
            | ClassName::AnimationTrack => Vec::new(),
            // An IntValue's number is always whole (`World::set_property`
            // rounds it).
            ClassName::TextChatService => vec![
                // Off, the default chat window isn't drawn and `/` doesn't
                // open it; scripts can still send and hear messages.
                ("ChatEnabled", Value::Bool(true)),
            ],
            ClassName::Players => vec![
                // On, each player gets a character when they join and
                // again `RespawnTime` seconds after it dies.
                ("CharacterAutoLoads", Value::Bool(true)),
                ("RespawnTime", Value::Number(5.0)),
                // Off, the default leaderboard (each Player's `leaderstats`)
                // isn't drawn, for a game's own.
                ("LeaderboardEnabled", Value::Bool(true)),
                // How many players a hosted game takes, the host's own
                // player among them (Phase 63).
                ("MaxPlayers", Value::Number(8.0)),
            ],
            ClassName::StarterPlayer => vec![
                // The prefab each player's character is a copy of, such as
                // "assets/prefabs/Hero.prefab.json"; empty for the
                // character Insert > Character makes.
                ("StarterCharacter", Value::String(String::new())),
            ],
            ClassName::Player => vec![
                // The character Model it plays as, or none.
                ("Character", Value::Instance(None)),
                // The Team it's on; on none while `Neutral`.
                ("Team", Value::Instance(None)),
                ("Neutral", Value::Bool(true)),
                // Who plays, the same each time on one install of the
                // engine (Phase 64); 0 until they're in a game.
                ("UserId", Value::Number(0.0)),
                // The name they joined with; `Name` is that, made unique.
                ("DisplayName", Value::String(String::new())),
            ],
            ClassName::Team => vec![
                ("TeamColor", Value::Color3(Color3::new(1.0, 1.0, 1.0))),
                // On, players joining are put on it (on the one with the
                // fewest players, when several are).
                ("AutoAssignable", Value::Bool(true)),
            ],
            ClassName::IntValue | ClassName::NumberValue => vec![("Value", Value::Number(0.0))],
            ClassName::StringValue => vec![("Value", Value::String(String::new()))],
            ClassName::BoolValue => vec![("Value", Value::Bool(false))],
            ClassName::Vector3Value => vec![("Value", Value::Vector3(Vector3::default()))],
            ClassName::Color3Value => vec![("Value", Value::Color3(Color3::default()))],
            ClassName::CFrameValue => vec![("Value", Value::CFrame(CFrame::IDENTITY))],
            ClassName::ObjectValue => vec![("Value", Value::Instance(None))],
            ClassName::Workspace => vec![
                // The wind everywhere, in studs a second. It pushes particles
                // from emitters with WindAffectsDrag, and parts with
                // WindAffected.
                ("GlobalWind", Value::Vector3(Vector3::new(0.0, 0.0, 0.0))),
                // Downward pull on unanchored parts and the characters, in
                // studs a second squared (Roblox's default; physics'
                // `GRAVITY`).
                ("Gravity", Value::Number(196.2)),
            ],
            ClassName::Lighting => vec![
                ("ShadowsEnabled", Value::Bool(true)),
                ("ShadowDistance", Value::Number(150.0)),
                // 0 is crisp; higher blurs edges more the further they are
                // from what casts them.
                ("ShadowSoftness", Value::Number(0.2)),
                // How strongly screen-space ambient occlusion darkens the
                // ambient and sky light in corners, from 0 to 2.
                ("AmbientOcclusionIntensity", Value::Number(1.0)),
                // The procedural sky, which also lights and is reflected by parts.
                ("SkyTopColor", Value::Color3(Color3::new(0.25, 0.5, 0.9))),
                (
                    "SkyHorizonColor",
                    Value::Color3(Color3::new(0.63, 0.8, 0.95)),
                ),
                ("GroundColor", Value::Color3(Color3::new(0.35, 0.33, 0.3))),
                ("EnvironmentIntensity", Value::Number(0.6)),
                // A project path to an HDR picture of the whole sky, drawn
                // and reflected in place of the colors above. Empty for none.
                ("SkyImage", Value::String(String::new())),
                // Distance fog: none nearer than FogStart studs, all FogColor
                // from FogEnd on. The default end is further than anything is
                // seen, so there's none. An Atmosphere's fog replaces it.
                (
                    "FogColor",
                    Value::Color3(Color3::new(192.0 / 255.0, 192.0 / 255.0, 192.0 / 255.0)),
                ),
                ("FogStart", Value::Number(0.0)),
                ("FogEnd", Value::Number(100_000.0)),
                // Tone mapping and bloom.
                ("PostProcessingEnabled", Value::Bool(true)),
                ("Exposure", Value::Number(1.0)),
                // Classic for worlds saved without it; the editor's templates
                // set Neutral.
                ("ToneMapping", Value::ToneMapping(ToneMapping::Classic)),
                ("BloomEnabled", Value::Bool(true)),
                ("BloomIntensity", Value::Number(0.35)),
                ("BloomThreshold", Value::Number(1.0)),
                // The time of day in hours, from 0 to 24 (it wraps), and how
                // far north of the equator the world is in degrees. They set
                // where the sun and moon are. The defaults put the sun where
                // it was before the time of day could be set.
                ("ClockTime", Value::Number(DEFAULT_CLOCK_TIME)),
                ("GeographicLatitude", Value::Number(DEFAULT_LATITUDE)),
            ],
            ClassName::PointLight => vec![
                ("Enabled", Value::Bool(true)),
                ("Color", Value::Color3(Color3::new(1.0, 1.0, 1.0))),
                ("Brightness", Value::Number(1.0)),
                // How far the light reaches, in studs.
                ("Range", Value::Number(16.0)),
                // Whether parts around it cast shadows. Off, so worlds
                // saved without it look as they did.
                ("Shadows", Value::Bool(false)),
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
                // Whether parts in the beam cast shadows. Off, so worlds
                // saved without it look as they did.
                ("Shadows", Value::Bool(false)),
            ],
            ClassName::ReflectionProbe => vec![
                // The bake the editor saved (Bake Reflection Probes), such as
                // "assets/probes/Hall.probe.hdr". Empty until it's baked,
                // when the sky is reflected.
                ("Bake", Value::String(String::new())),
                // Studs inside the box's faces over which its reflection
                // fades in from what's around it.
                ("BlendDistance", Value::Number(2.0)),
            ],
            ClassName::LightProbeVolume => vec![
                // The bake the editor saved (Bake Probes), such as
                // "assets/probes/Hall.lightprobes.json". Empty until it's
                // baked, when the sky lights what's inside.
                ("Bake", Value::String(String::new())),
                // Studs between probes along each side of the box.
                ("Spacing", Value::Number(4.0)),
                // Studs inside the box's faces over which its light fades in
                // from what's around it.
                ("BlendDistance", Value::Number(2.0)),
            ],
            ClassName::Atmosphere => vec![
                // How thick the air is, from 0 (none: a black sky and a white
                // sun) to 1. 0.395 is Earth's.
                ("Density", Value::Number(0.395)),
                // How high above the ground the sky is seen from, from 0 to 1
                // (one height of the air, where the sky above darkens).
                ("Offset", Value::Number(0.0)),
                // Tints the whole sky; this grey leaves it as it is.
                (
                    "Color",
                    Value::Color3(Color3::new(199.0 / 255.0, 199.0 / 255.0, 199.0 / 255.0)),
                ),
                // Tints the sky away from the sun; this blue-grey leaves it
                // as it is.
                (
                    "Decay",
                    Value::Color3(Color3::new(106.0 / 255.0, 112.0 / 255.0, 125.0 / 255.0)),
                ),
                // Brightens the glow around the sun, from 0 to 10.
                ("Glare", Value::Number(0.0)),
                // Adds dust and haze to the air, from 0 to 10: a paler
                // horizon, a wider glow and a dimmer, redder sun.
                ("Haze", Value::Number(0.0)),
            ],
            ClassName::Clouds => vec![
                // How much of the sky is cloud, from 0 (none) to 1 (overcast).
                ("Cover", Value::Number(0.5)),
                // How thick the clouds are, from 0 (none) to 1: thicker ones
                // are whiter on top, darker below, and cast deeper shadows.
                ("Density", Value::Number(0.7)),
                // The clouds' own color, which the sun and sky light.
                ("Color", Value::Color3(Color3::new(1.0, 1.0, 1.0))),
                // How fast the clouds drift on their own, in studs a second,
                // and which way, in degrees turned from +X (east) toward +Z
                // (south). The Workspace's GlobalWind adds to it.
                ("Speed", Value::Number(20.0)),
                ("Direction", Value::Number(0.0)),
            ],
            ClassName::Weather => vec![
                // What falls around the camera (`Enum.Precipitation`): None,
                // Rain or Snow. Rain wets surfaces as it falls, and snow
                // covers them.
                (
                    "Precipitation",
                    Value::EnumItem(EnumItem {
                        enum_name: "Precipitation".to_owned(),
                        item: "Rain".to_owned(),
                    }),
                ),
                // How hard it falls, from 0 to 1.
                ("Intensity", Value::Number(0.5)),
                // How wet surfaces are, and how much snow covers the ones
                // facing up, from 0 to 1, whatever is falling (puddles
                // after the rain, old snow).
                ("Wetness", Value::Number(0.0)),
                ("SnowCover", Value::Number(0.0)),
                // Seconds a change takes to ease in: rain wetting surfaces,
                // drying, snow settling or melting.
                ("TransitionTime", Value::Number(10.0)),
            ],
            ClassName::OutlineEffect => vec![
                ("Enabled", Value::Bool(true)),
                // The lines' color.
                ("Color", Value::Color3(Color3::new(0.0, 0.0, 0.0))),
                // How thick they are, in pixels of a view 1080 pixels tall
                // (scaled with the view's height, never under one pixel).
                ("Thickness", Value::Number(1.0)),
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
                // A key in the string table: shows its value in the active
                // language instead of Text. Empty, or a missing entry, shows Text.
                ("TextKey", Value::String(String::new())),
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
                // A key in the string table: shows its value in the active
                // language instead of Text. Empty, or a missing entry, shows Text.
                ("TextKey", Value::String(String::new())),
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
            ClassName::Sequence => vec![
                // The sequence file, such as
                // "assets/sequences/Intro.sequence.json".
                ("SequenceId", Value::String(String::new())),
                // On, it plays when the game starts.
                ("AutoPlay", Value::Bool(false)),
                // On, the Menu action jumps it to its end.
                ("Skippable", Value::Bool(true)),
                // On, the player can't move the character or use the game's
                // input while it plays.
                ("LockPlayerInput", Value::Bool(true)),
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
                // The SoundGroup it plays through, or none for straight
                // out.
                ("SoundGroup", Value::Instance(None)),
            ],
            ClassName::SoundGroup => vec![
                // A multiplier, as Sound.Volume: 1 leaves what goes
                // through it as it is, 0 silences it.
                ("Volume", Value::Number(1.0)),
                // On, the default audio settings UI has a slider for it.
                ("UserAdjustable", Value::Bool(false)),
            ],
            ClassName::ReverbSoundEffect => sound_effect_properties([
                // Seconds the tail takes to die away.
                ("DecayTime", Value::Number(1.5)),
                // 0 is a few echoes, 1 a smooth wash; also how bright it stays.
                ("Density", Value::Number(1.0)),
                // Decibels of the sound as it is, and of the reverb.
                ("DryLevel", Value::Number(-6.0)),
                ("WetLevel", Value::Number(0.0)),
            ]),
            ClassName::EqualizerSoundEffect => sound_effect_properties([
                // Decibels for below MidRange's X, between, and above its Y.
                ("LowGain", Value::Number(0.0)),
                ("MidGain", Value::Number(0.0)),
                ("HighGain", Value::Number(0.0)),
                // The mids, in hertz: from X to Y.
                ("MidRange", Value::Vector2(Vector2::new(400.0, 4000.0))),
            ]),
            ClassName::CompressorSoundEffect => sound_effect_properties([
                // Decibels above which it turns the sound down,
                ("Threshold", Value::Number(-40.0)),
                // by this much: 4 lets through a quarter of what's above.
                ("Ratio", Value::Number(40.0)),
                // Seconds it takes to turn down, and back up.
                ("Attack", Value::Number(0.1)),
                ("Release", Value::Number(0.1)),
                // Decibels it turns everything back up by after.
                ("GainMakeup", Value::Number(0.0)),
            ]),
            ClassName::EchoSoundEffect => sound_effect_properties([
                // Seconds between repeats.
                ("Delay", Value::Number(1.0)),
                // Decibels each repeat is quieter by (as a negative number).
                ("Feedback", Value::Number(-3.0)),
                ("DryLevel", Value::Number(0.0)),
                ("WetLevel", Value::Number(0.0)),
            ]),
            // From 0 (clean) to 1 (fuzz).
            ClassName::DistortionSoundEffect => {
                sound_effect_properties([("Level", Value::Number(0.75))])
            }
            // How much higher: 2 is an octave up, 0.5 one down.
            ClassName::PitchShiftSoundEffect => {
                sound_effect_properties([("Octave", Value::Number(1.25))])
            }
            ClassName::LowPassSoundEffect => sound_effect_properties([
                // Hertz: what's above it fades away.
                ("Cutoff", Value::Number(1000.0)),
                // From 0 to 1: how much it rings at Cutoff.
                ("Resonance", Value::Number(0.0)),
            ]),
            ClassName::HighPassSoundEffect => sound_effect_properties([
                // Hertz: what's below it fades away.
                ("Cutoff", Value::Number(500.0)),
                ("Resonance", Value::Number(0.0)),
            ]),
            ClassName::Part
            | ClassName::NegateOperation
            | ClassName::Seat
            | ClassName::VehicleSeat
            | ClassName::DamageZone
            | ClassName::ReverbZone
            | ClassName::SpawnLocation => {
                let material = Material::Plastic;
                let preset = material.preset();
                let mut properties = vec![
                    // Anchored parts stay put; unanchored ones fall and collide.
                    ("Anchored", Value::Bool(false)),
                    ("Shape", Value::PartType(PartType::Block)),
                    ("Position", Value::Vector3(Vector3::new(0.0, 0.0, 0.0))),
                    // Rotation in degrees around X, Y and Z, applied Z first, then X, then Y.
                    ("Orientation", Value::Vector3(Vector3::new(0.0, 0.0, 0.0))),
                    ("Size", Value::Vector3(Vector3::new(4.0, 1.0, 2.0))),
                    ("Color", Value::Color3(Color3::new(0.64, 0.64, 0.64))),
                    ("Material", Value::Material(material)),
                    // A material file in the project, such as
                    // "assets/materials/brick.material.json", drawn in place
                    // of Material (Color still tints it). Empty for none.
                    ("MaterialVariant", Value::String(String::new())),
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
                ];
                properties.extend(collision_properties());
                properties.extend(body_properties());
                if self.is_seat() {
                    properties.extend([
                        // The character controller sitting on it, or none.
                        // Read-only for scripts: `seat:Sit(controller)` sits one.
                        ("Occupant", Value::Instance(None)),
                        // On, touching it doesn't sit anyone; `Sit` still does.
                        ("Disabled", Value::Bool(false)),
                        // An Attachment where the character's feet go, or
                        // none for the middle of the top face.
                        ("SeatOffset", Value::Instance(None)),
                    ]);
                }
                if self == ClassName::DamageZone {
                    // Something to walk into, not onto, and see into.
                    for (name, value) in [
                        ("Anchored", Value::Bool(true)),
                        ("CanCollide", Value::Bool(false)),
                        ("Transparency", Value::Number(0.5)),
                        ("Color", Value::Color3(Color3::new(0.85, 0.2, 0.15))),
                        ("CastShadow", Value::Bool(false)),
                    ] {
                        if let Some(slot) = properties.iter_mut().find(|(n, _)| *n == name) {
                            slot.1 = value;
                        }
                    }
                    // Health a second taken from each Health inside it.
                    properties.push(("DamagePerSecond", Value::Number(10.0)));
                }
                if self == ClassName::ReverbZone {
                    // Something to walk into, not onto, and see into.
                    for (name, value) in [
                        ("Anchored", Value::Bool(true)),
                        ("CanCollide", Value::Bool(false)),
                        ("Transparency", Value::Number(0.8)),
                        ("Color", Value::Color3(Color3::new(0.35, 0.55, 0.95))),
                        ("CastShadow", Value::Bool(false)),
                        ("Size", Value::Vector3(Vector3::new(20.0, 12.0, 20.0))),
                    ] {
                        if let Some(slot) = properties.iter_mut().find(|(n, _)| *n == name) {
                            slot.1 = value;
                        }
                    }
                    properties.extend([
                        // The room it sounds like (`Enum.ReverbType`).
                        (
                            "ReverbType",
                            Value::EnumItem(EnumItem {
                                enum_name: "ReverbType".to_owned(),
                                item: "Room".to_owned(),
                            }),
                        ),
                        // Seconds the reverb takes to blend in on entering,
                        // and out on leaving.
                        ("FadeTime", Value::Number(1.0)),
                    ]);
                }
                if self == ClassName::SpawnLocation {
                    // A pad to stand on.
                    for (name, value) in [
                        ("Anchored", Value::Bool(true)),
                        ("Size", Value::Vector3(Vector3::new(12.0, 1.0, 12.0))),
                    ] {
                        if let Some(slot) = properties.iter_mut().find(|(n, _)| *n == name) {
                            slot.1 = value;
                        }
                    }
                    properties.extend([
                        // Off, nobody spawns here.
                        ("Enabled", Value::Bool(true)),
                        // On, players on any team (or none) spawn here, when
                        // their team has no SpawnLocation of its own.
                        ("Neutral", Value::Bool(true)),
                        // The Team whose players spawn here, by its
                        // TeamColor, while `Neutral` is off.
                        ("TeamColor", Value::Color3(Color3::new(1.0, 1.0, 1.0))),
                        // Seconds a ForceField keeps a character spawned
                        // here from harm; 0 for none.
                        ("Duration", Value::Number(10.0)),
                    ]);
                }
                if self == ClassName::VehicleSeat {
                    properties.extend([
                        // Studs a second the wheels stop pushing at.
                        ("MaxSpeed", Value::Number(60.0)),
                        // How hard the driven wheels push at full throttle:
                        // studs a second squared on flat ground, whatever
                        // the car weighs.
                        ("Torque", Value::Number(80.0)),
                        // Degrees a second the steering wheels turn.
                        ("TurnSpeed", Value::Number(120.0)),
                        // Degrees the steering wheels turn at full lock.
                        ("SteerAngle", Value::Number(30.0)),
                        // -1 (reverse) to 1 (forward). The Move action
                        // sets it while someone sits here; scripts may too.
                        ("Throttle", Value::Number(0.0)),
                        // -1 (left) to 1 (right), set like `Throttle`.
                        ("Steer", Value::Number(0.0)),
                        // On, the braking wheels hold the car still.
                        ("HandBrake", Value::Bool(false)),
                    ]);
                }
                properties
            }
            ClassName::MeshPart => {
                let mut properties = vec![
                    ("Anchored", Value::Bool(false)),
                    // The model's file, relative to the project folder, such as
                    // "assets/models/chair.glb". Empty means no model yet.
                    ("MeshId", Value::String(String::new())),
                    // A material file drawn in place of the model's materials.
                    // Empty for none.
                    ("MaterialVariant", Value::String(String::new())),
                    ("Position", Value::Vector3(Vector3::new(0.0, 0.0, 0.0))),
                    ("Orientation", Value::Vector3(Vector3::new(0.0, 0.0, 0.0))),
                    // The model is stretched so its bounding box is this size.
                    ("Size", Value::Vector3(Vector3::new(2.0, 2.0, 2.0))),
                    ("CastShadow", Value::Bool(true)),
                    ("Transparency", Value::Number(0.0)),
                    ("CanCollide", Value::Bool(true)),
                    ("WindAffected", Value::Bool(false)),
                ];
                properties.extend(collision_properties());
                properties.extend(body_properties());
                properties
            }
            ClassName::UnionOperation => {
                let mut properties = vec![
                    ("Anchored", Value::Bool(false)),
                    // The combined solid's model, saved by the editor in
                    // assets/models (a `.union.glb`).
                    ("MeshId", Value::String(String::new())),
                    ("Position", Value::Vector3(Vector3::new(0.0, 0.0, 0.0))),
                    ("Orientation", Value::Vector3(Vector3::new(0.0, 0.0, 0.0))),
                    // The model is stretched so its bounding box is this size.
                    ("Size", Value::Vector3(Vector3::new(2.0, 2.0, 2.0))),
                    // On, the whole solid is drawn in Color; off, each face
                    // keeps the color and material of the part it came from.
                    ("UsePartColor", Value::Bool(false)),
                    ("Color", Value::Color3(Color3::new(0.64, 0.64, 0.64))),
                    // What it collides as: its box, the convex hull of its
                    // corners, or convex pieces that follow its hollows
                    // (doorways, craters).
                    ("CollisionFidelity", collision_fidelity("Hull")),
                    ("CastShadow", Value::Bool(true)),
                    ("Transparency", Value::Number(0.0)),
                    ("CanCollide", Value::Bool(true)),
                    ("WindAffected", Value::Bool(false)),
                ];
                properties.extend(collision_properties());
                properties.extend(body_properties());
                properties
            }
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
            ClassName::Attachment => vec![
                // Where it sits and how it's turned, relative to the part
                // it's parented to (degrees, like a Part's Orientation).
                ("Position", Value::Vector3(Vector3::new(0.0, 0.0, 0.0))),
                ("Orientation", Value::Vector3(Vector3::new(0.0, 0.0, 0.0))),
            ],
            ClassName::VehicleWheel => vec![
                // The wheel's middle with its spring at rest length, and
                // how it's turned, relative to its part (as an Attachment).
                ("Position", Value::Vector3(Vector3::new(0.0, 0.0, 0.0))),
                ("Orientation", Value::Vector3(Vector3::new(0.0, 0.0, 0.0))),
                ("Radius", Value::Number(1.0)),
                // Studs of spring above the wheel's middle.
                ("SuspensionRestLength", Value::Number(1.5)),
                // How hard the spring pushes back per stud it's squashed,
                // and how much it holds back bouncing, whatever the car
                // weighs. The defaults squash it half a stud at rest.
                ("SuspensionStiffness", Value::Number(100.0)),
                ("SuspensionDamping", Value::Number(8.0)),
                // How much grip the tyre has before it slides.
                ("Friction", Value::Number(2.0)),
                // Turns with the seat's Steer.
                ("Steers", Value::Bool(false)),
                // Pushes with the seat's Throttle.
                ("Driven", Value::Bool(true)),
                // Holds with the HandBrake, and when nobody drives.
                ("Brakes", Value::Bool(true)),
                // A part moved to the wheel every frame, spinning as it
                // rolls and turning as it steers, or none.
                ("WheelPart", Value::Instance(None)),
            ],
            ClassName::Ragdoll => vec![
                // On, physics moves the capsules and they pose the bones;
                // off, the Animator does.
                ("Enabled", Value::Bool(false)),
                // Seconds to blend from the ragdoll back to the Animator's
                // pose once it's turned off.
                ("BlendTime", Value::Number(0.5)),
                // The Animator state to play when it's turned off, or "".
                ("GetUpState", Value::String(String::new())),
            ],
            ClassName::Fracture => vec![
                // How many pieces Generate cuts the part into.
                ("Pieces", Value::Number(12.0)),
                // The same Seed always makes the same pieces.
                ("Seed", Value::Number(0.0)),
                // Uniform: pieces of a size; Impact: smaller toward the middle.
                ("Pattern", fracture_pattern("Uniform")),
                // What the faces inside the part, which the cuts make, look like.
                ("InteriorMaterial", Value::Material(Material::Plastic)),
                // Studs a second something must hit the part at to shatter it.
                ("Threshold", Value::Number(60.0)),
                // Seconds the pieces last before they fade out and are
                // destroyed. 0 keeps them.
                ("DebrisLifetime", Value::Number(5.0)),
                // The pieces' file, set by Generate, such as
                // "assets/models/Pane.fracture.glb". Empty: not generated yet.
                ("PiecesId", Value::String(String::new())),
            ],
            ClassName::Health => vec![
                ("MaxHealth", Value::Number(100.0)),
                // 0 to MaxHealth. At 0 the Model is dead: it fires Died once,
                // and only setting Health brings it back.
                ("Health", Value::Number(100.0)),
                // Health a second won back once RegenDelay seconds pass
                // without damage. 0 never regenerates.
                ("RegenRate", Value::Number(1.0)),
                ("RegenDelay", Value::Number(5.0)),
                // On, nothing damages it; Heal still heals.
                ("Invulnerable", Value::Bool(false)),
                // On, a character landing faster than FallDamageThreshold
                // studs a second loses FallDamageScale health for each stud
                // a second over it.
                ("FallDamage", Value::Bool(false)),
                ("FallDamageThreshold", Value::Number(60.0)),
                ("FallDamageScale", Value::Number(1.0)),
                // The name a game shows over the character (Phase 46).
                // Empty is the Model's Name.
                ("DisplayName", Value::String(String::new())),
                // How far away, in studs, a game shows the character's
                // health over it. 0 never.
                ("HealthDisplayDistance", Value::Number(100.0)),
            ],
            ClassName::ProximityPrompt => vec![
                // What it does, such as "Open", and to what, such as "Door".
                ("ActionText", Value::String("Interact".to_owned())),
                ("ObjectText", Value::String(String::new())),
                // The input map action that uses it.
                ("Action", Value::String("Interact".to_owned())),
                // Seconds the action is held before it triggers. 0 triggers
                // on the press.
                ("HoldDuration", Value::Number(0.0)),
                // Studs from the player character it shows within.
                ("MaxActivationDistance", Value::Number(10.0)),
                // On, it doesn't show while something solid is between it
                // and the character.
                ("RequiresLineOfSight", Value::Bool(true)),
                ("Enabled", Value::Bool(true)),
                // Which prompts in reach show: the nearest for each action
                // (OnePerButton), the nearest of all (OneGlobally), or this
                // one whenever it's in reach (AlwaysShow).
                (
                    "Exclusivity",
                    Value::EnumItem(EnumItem {
                        enum_name: "ProximityPromptExclusivity".to_owned(),
                        item: "OnePerButton".to_owned(),
                    }),
                ),
            ],
            ClassName::ClickDetector => vec![
                // Studs from the player character (or the camera, with no
                // character) a click reaches.
                ("MaxActivationDistance", Value::Number(32.0)),
                // The cursor over its part, as a CSS cursor name such as
                // "pointer" or "crosshair". Empty is the pointing hand.
                ("CursorIcon", Value::String(String::new())),
            ],
            ClassName::Tool => vec![
                // The part that's held and touched to pick it up. Empty is
                // its child part named "Handle".
                ("Handle", Value::Instance(None)),
                // The Attachment in the Handle that goes in the hand. Empty
                // is the Handle's Attachment named "Grip", else its middle.
                ("Grip", Value::Instance(None)),
                // Off, a Tool with no Handle can still be carried and
                // equipped; it's never picked up by touch.
                ("RequiresHandle", Value::Bool(true)),
                ("CanBeDropped", Value::Bool(true)),
                // What the hotbar shows for it: ToolTip on hover, the
                // TextureId image in its slot (its Name with none).
                ("ToolTip", Value::String(String::new())),
                ("TextureId", Value::String(String::new())),
                // The data-table row it is, as "table:key" (such as
                // "weapons:sword"). Empty for none.
                ("Item", Value::String(String::new())),
            ],
            ClassName::Inventory => vec![
                // How many Tools it holds; touching more leaves them.
                ("Capacity", Value::Number(10.0)),
                // The Tool in hand, set by Equip and UnequipAll.
                ("Equipped", Value::Instance(None)),
                // Off, the default hotbar doesn't show and its keys do
                // nothing, for a game's own.
                ("HotbarEnabled", Value::Bool(true)),
            ],
            ClassName::Humanoid => vec![
                // On, dying breaks the Model's welds and joints (not its
                // Ragdolls') and lets its parts fall.
                ("BreakJointsOnDeath", Value::Bool(true)),
                // On, dying turns on the Model's Ragdolls.
                ("RagdollOnDeath", Value::Bool(false)),
            ],
            ClassName::WeldConstraint => {
                let mut properties = vec![
                    ("Part0", Value::Instance(None)),
                    ("Part1", Value::Instance(None)),
                    // Off, the two parts move apart; on again, they're welded
                    // where they are then.
                    ("Enabled", Value::Bool(true)),
                ];
                properties.extend(break_properties());
                properties
            }
            ClassName::HingeConstraint => {
                let mut properties = joint_properties();
                properties.extend([
                    // Degrees `Attachment1` may turn from `Attachment0`
                    // around the axis, while LimitsEnabled is on.
                    ("LimitsEnabled", Value::Bool(false)),
                    ("LowerAngle", Value::Number(-45.0)),
                    ("UpperAngle", Value::Number(45.0)),
                ]);
                properties.extend(turn_actuator("ActuatorType"));
                properties
            }
            ClassName::BallSocketConstraint => {
                let mut properties = joint_properties();
                properties.extend([
                    // How far, in degrees, Attachment1's axis may lean from
                    // Attachment0's, while LimitsEnabled is on.
                    ("LimitsEnabled", Value::Bool(false)),
                    ("UpperAngle", Value::Number(45.0)),
                    // How far, in degrees, it may twist around its axis,
                    // while TwistLimitsEnabled is on.
                    ("TwistLimitsEnabled", Value::Bool(false)),
                    ("TwistLowerAngle", Value::Number(-45.0)),
                    ("TwistUpperAngle", Value::Number(45.0)),
                ]);
                properties
            }
            ClassName::PrismaticConstraint => {
                let mut properties = joint_properties();
                properties.extend(slide_actuator());
                properties
            }
            ClassName::CylindricalConstraint => {
                let mut properties = joint_properties();
                properties.extend(slide_actuator());
                properties.extend([
                    ("AngularLimitsEnabled", Value::Bool(false)),
                    ("LowerAngle", Value::Number(-45.0)),
                    ("UpperAngle", Value::Number(45.0)),
                ]);
                properties.extend(turn_actuator("AngularActuatorType"));
                properties
            }
            ClassName::SpringConstraint => {
                let mut properties = joint_properties();
                properties.extend([
                    // Force per stud it's stretched or squashed from
                    // FreeLength, and force per stud a second of stretching.
                    ("Stiffness", Value::Number(1000.0)),
                    ("Damping", Value::Number(20.0)),
                    // The length it pulls or pushes toward, in studs.
                    ("FreeLength", Value::Number(5.0)),
                    // It never gets shorter or longer than these.
                    ("MinLength", Value::Number(0.0)),
                    ("MaxLength", Value::Number(1000.0)),
                    // How the coil is drawn while Visible.
                    ("Coils", Value::Number(8.0)),
                    ("Radius", Value::Number(0.4)),
                    ("Thickness", Value::Number(0.1)),
                ]);
                properties
            }
            ClassName::RopeConstraint => {
                let mut properties = joint_properties();
                properties.extend([
                    // The farthest the attachments can be apart, in studs.
                    ("Length", Value::Number(5.0)),
                    // On, a winch reels the rope toward WinchTarget at
                    // WinchSpeed studs a second, while lifting needs no
                    // more than WinchForce.
                    ("WinchEnabled", Value::Bool(false)),
                    ("WinchTarget", Value::Number(5.0)),
                    ("WinchSpeed", Value::Number(2.0)),
                    ("WinchForce", Value::Number(100_000.0)),
                    ("Thickness", Value::Number(0.1)),
                ]);
                properties
            }
            ClassName::RodConstraint => {
                let mut properties = joint_properties();
                properties.extend([
                    // How far apart the attachments are held, in studs.
                    ("Length", Value::Number(5.0)),
                    ("Thickness", Value::Number(0.1)),
                ]);
                properties
            }
            ClassName::LinearVelocity => {
                let mut properties = mover_properties();
                properties.extend([
                    // Studs a second, in the RelativeTo frame's axes.
                    (
                        "VectorVelocity",
                        Value::Vector3(Vector3::new(0.0, 0.0, 0.0)),
                    ),
                    // The most force it pushes with to keep that speed.
                    ("MaxForce", Value::Number(100_000.0)),
                    ("RelativeTo", relative_to("World")),
                ]);
                properties
            }
            ClassName::AngularVelocity => {
                let mut properties = mover_properties();
                properties.extend([
                    // Radians a second around each axis of the RelativeTo
                    // frame.
                    (
                        "AngularVelocity",
                        Value::Vector3(Vector3::new(0.0, 0.0, 0.0)),
                    ),
                    ("MaxTorque", Value::Number(100_000.0)),
                    ("RelativeTo", relative_to("World")),
                ]);
                properties
            }
            ClassName::VectorForce => {
                let mut properties = mover_properties();
                properties.extend([
                    ("Force", Value::Vector3(Vector3::new(0.0, 0.0, 0.0))),
                    ("RelativeTo", relative_to("Attachment0")),
                    // Off, it pushes at Attachment0, so a force off the
                    // center of mass turns the part too.
                    ("ApplyAtCenterOfMass", Value::Bool(false)),
                ]);
                properties
            }
            ClassName::Torque => {
                let mut properties = mover_properties();
                properties.extend([
                    ("Torque", Value::Vector3(Vector3::new(0.0, 0.0, 0.0))),
                    ("RelativeTo", relative_to("Attachment0")),
                ]);
                properties
            }
            ClassName::AlignPosition => {
                let mut properties = mover_properties();
                properties.extend([
                    // TwoAttachment follows Attachment1; OneAttachment goes
                    // to `Position`, in the world.
                    ("Mode", alignment_mode("PositionAlignmentMode")),
                    ("Position", Value::Vector3(Vector3::new(0.0, 0.0, 0.0))),
                    ("MaxForce", Value::Number(100_000.0)),
                    // Studs a second.
                    ("MaxVelocity", Value::Number(100_000.0)),
                    // How quickly it gets there: about 1/Responsiveness
                    // seconds to close most of the gap.
                    ("Responsiveness", Value::Number(10.0)),
                    // On, it gets there as fast as MaxForce and MaxVelocity
                    // allow, whatever Responsiveness says.
                    ("RigidityEnabled", Value::Bool(false)),
                ]);
                properties
            }
            ClassName::AlignOrientation => {
                let mut properties = mover_properties();
                properties.extend([
                    // TwoAttachment turns to Attachment1; OneAttachment to
                    // `CFrame`'s rotation, in the world.
                    ("Mode", alignment_mode("OrientationAlignmentMode")),
                    ("CFrame", Value::CFrame(CFrame::IDENTITY)),
                    ("MaxTorque", Value::Number(100_000.0)),
                    // Radians a second.
                    ("MaxAngularVelocity", Value::Number(100_000.0)),
                    ("Responsiveness", Value::Number(10.0)),
                    ("RigidityEnabled", Value::Bool(false)),
                ]);
                properties
            }
            ClassName::Camera => vec![
                // Where the viewpoint is and which way it looks, in degrees
                // like a Part's (Z, rolling, isn't drawn).
                ("Position", Value::Vector3(Vector3::new(0.0, 0.0, 0.0))),
                ("Orientation", Value::Vector3(Vector3::new(0.0, 0.0, 0.0))),
                // The vertical field of view, in degrees, from 1 to 120.
                ("FieldOfView", Value::Number(50.0)),
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
            | ClassName::Lighting
            | ClassName::ReplicatedStorage
            | ClassName::ServerScriptService
            | ClassName::ServerStorage
            | ClassName::StarterPlayer
            | ClassName::StarterPlayerScripts
            | ClassName::Teams
            | ClassName::Team
            | ClassName::SoundService
            | ClassName::SoundGroup
            | ClassName::ReverbSoundEffect
            | ClassName::EqualizerSoundEffect
            | ClassName::CompressorSoundEffect
            | ClassName::EchoSoundEffect
            | ClassName::DistortionSoundEffect
            | ClassName::PitchShiftSoundEffect
            | ClassName::LowPassSoundEffect
            | ClassName::HighPassSoundEffect
            | ClassName::ForceField
            | ClassName::Script
            | ClassName::LocalScript
            | ClassName::ModuleScript
            | ClassName::RemoteFunction
            | ClassName::BindableFunction
            | ClassName::Model
            | ClassName::Inventory
            | ClassName::Folder
            | ClassName::SurfaceAppearance
            | ClassName::ScreenGui
            | ClassName::BillboardGui
            | ClassName::PointLight
            | ClassName::SpotLight
            | ClassName::ReflectionProbe
            | ClassName::LightProbeVolume
            | ClassName::Atmosphere
            | ClassName::Clouds
            | ClassName::Weather
            | ClassName::OutlineEffect
            | ClassName::ParticleEmitter
            | ClassName::Terrain
            | ClassName::TerrainLayer
            | ClassName::Checkpoint
            | ClassName::Camera
            | ClassName::Attachment
            | ClassName::VehicleWheel
            | ClassName::Ragdoll
            | ClassName::LinearVelocity
            | ClassName::AngularVelocity
            | ClassName::VectorForce
            | ClassName::Torque
            | ClassName::AlignPosition
            | ClassName::AlignOrientation
            | ClassName::IntValue
            | ClassName::NumberValue
            | ClassName::StringValue
            | ClassName::BoolValue
            | ClassName::Vector3Value
            | ClassName::Color3Value
            | ClassName::CFrameValue
            | ClassName::ObjectValue => &[],
            // Fires with true and the Seat when the character sits, and
            // with false and the Seat it left when it stands.
            // `Landed` fires with how fast it was falling, in studs a
            // second, as it lands.
            ClassName::ThirdPersonController | ClassName::FirstPersonController => {
                &["Seated", "Landed"]
            }
            // `HealthChanged` with the new health and the old, `Damaged`
            // with how much and what did it (or nil), `Died` once at 0.
            ClassName::Health => &["HealthChanged", "Damaged", "Died"],
            // `MoveToFinished(reached)` when a `MoveTo` ends, `StateChanged`
            // with the old and new `Enum.HumanoidStateType`, and `Died`
            // once when its Health reaches 0.
            ClassName::Humanoid => &["MoveToFinished", "StateChanged", "Died"],
            // Each with the Player, as they join (Phase 47) and as they
            // leave a hosted game (Phase 63).
            ClassName::Players => &["PlayerAdded", "PlayerRemoving"],
            // With the message: `Text`, and `Player` (nil for a system
            // message), on each device once it arrives (Phase 67).
            ClassName::TextChatService => &["MessageReceived"],
            // Each with the character Model: `CharacterAdded` once it's in
            // the Workspace, `CharacterRemoving` as it goes.
            ClassName::Player => &["CharacterAdded", "CharacterRemoving"],
            // Each with the Player, but `PromptShown` and `PromptHidden`.
            ClassName::ProximityPrompt => &[
                "Triggered",
                "TriggerEnded",
                "PromptShown",
                "PromptHidden",
                "PromptButtonHoldBegan",
                "PromptButtonHoldEnded",
            ],
            // Each with the Player who clicked or pointed.
            ClassName::ClickDetector => &[
                "MouseClick",
                "RightMouseClick",
                "MouseHoverEnter",
                "MouseHoverLeave",
            ],
            // In and out of the hand, and the Primary action pressed and
            // let go while it's in the hand.
            ClassName::Tool => &["Equipped", "Unequipped", "Activated", "Deactivated"],
            // Fires with "Sight" or "Hearing" when it starts seeing or
            // hearing the player character.
            ClassName::NavigationAgent => &["PlayerSensed"],
            // Fires once when it breaks past its BreakForce or BreakTorque.
            ClassName::WeldConstraint
            | ClassName::HingeConstraint
            | ClassName::BallSocketConstraint
            | ClassName::PrismaticConstraint
            | ClassName::CylindricalConstraint
            | ClassName::SpringConstraint
            | ClassName::RopeConstraint
            | ClassName::RodConstraint => &["Broken"],
            // Fires when its part is replaced by its pieces.
            ClassName::Fracture => &["Shattered"],
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
            // `Completed` once when it reaches its end (or is skipped there);
            // `EventReached` with an event track's name as it passes.
            ClassName::Sequence => &["Completed", "EventReached"],
            // Fire on both parts when two parts start and stop touching.
            ClassName::Part
            | ClassName::MeshPart
            | ClassName::UnionOperation
            | ClassName::NegateOperation
            | ClassName::Seat
            | ClassName::VehicleSeat
            | ClassName::DamageZone
            | ClassName::ReverbZone
            | ClassName::SpawnLocation => &["Touched", "TouchEnded"],
            // `OnServerEvent` fires in Scripts for each `FireServer`, and
            // `OnClientEvent` in LocalScripts for each `FireClient`.
            ClassName::RemoteEvent => &["OnServerEvent", "OnClientEvent"],
            // Fires on the same side, with what `Fire` was given.
            ClassName::BindableEvent => &["Event"],
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

/// What every sound effect has (Phase 52), then its own `settings`.
/// Effects with a higher `Priority` come first; equal ones keep their
/// order among the children.
fn sound_effect_properties<const N: usize>(
    settings: [(&'static str, Value); N],
) -> Vec<(&'static str, Value)> {
    let mut properties = vec![
        // Off, it's passed by as if it weren't there.
        ("Enabled", Value::Bool(true)),
        ("Priority", Value::Number(0.0)),
    ];
    properties.extend(settings);
    properties
}

/// What every joint constraint has (Phase 36): the Attachments it joins,
/// whether it acts, and whether the game draws it.
fn joint_properties() -> Vec<(&'static str, Value)> {
    let mut properties = vec![
        ("Attachment0", Value::Instance(None)),
        ("Attachment1", Value::Instance(None)),
        // Off, it doesn't hold the parts at all.
        ("Enabled", Value::Bool(true)),
        // On, the game draws it (a coil, rope, rod or axle) in `Color`.
        ("Visible", Value::Bool(false)),
        ("Color", Value::Color3(Color3::new(0.62, 0.64, 0.68))),
    ];
    properties.extend(break_properties());
    properties
}

/// What makes a joint or weld break (Phase 42): past `BreakForce` (mass
/// times studs a second squared) or `BreakTorque` it turns itself off and
/// fires `Broken`. 0 never breaks.
fn break_properties() -> [(&'static str, Value); 2] {
    [
        ("BreakForce", Value::Number(0.0)),
        ("BreakTorque", Value::Number(0.0)),
    ]
}

/// A Fracture's `Pattern` item: Uniform or Impact.
fn fracture_pattern(item: &str) -> Value {
    Value::EnumItem(EnumItem {
        enum_name: "FracturePattern".to_owned(),
        item: item.to_owned(),
    })
}

/// What turns a hinge, or a cylindrical constraint around its axis:
/// `actuator` is None, Motor (spins at `AngularVelocity` radians a second,
/// with at most `MotorMaxTorque`) or Servo (turns to `TargetAngle` degrees
/// at up to `AngularSpeed` radians a second, with at most `ServoMaxTorque`).
fn turn_actuator(actuator: &'static str) -> [(&'static str, Value); 6] {
    [
        (actuator, actuator_type("None")),
        ("AngularVelocity", Value::Number(0.0)),
        ("MotorMaxTorque", Value::Number(100_000.0)),
        ("TargetAngle", Value::Number(0.0)),
        ("AngularSpeed", Value::Number(2.0)),
        ("ServoMaxTorque", Value::Number(100_000.0)),
    ]
}

/// What slides a prismatic or cylindrical constraint along its axis: limits
/// in studs along `Attachment0`'s X axis, and a motor (`Velocity` studs a
/// second, with at most `MotorMaxForce`) or a servo (to `TargetPosition` at
/// up to `Speed`, with at most `ServoMaxForce`).
fn slide_actuator() -> [(&'static str, Value); 9] {
    [
        ("LimitsEnabled", Value::Bool(false)),
        ("LowerLimit", Value::Number(-5.0)),
        ("UpperLimit", Value::Number(5.0)),
        ("ActuatorType", actuator_type("None")),
        ("Velocity", Value::Number(0.0)),
        ("MotorMaxForce", Value::Number(100_000.0)),
        ("TargetPosition", Value::Number(0.0)),
        ("Speed", Value::Number(5.0)),
        ("ServoMaxForce", Value::Number(100_000.0)),
    ]
}

/// An `ActuatorType` item: None, Motor or Servo.
fn actuator_type(item: &str) -> Value {
    Value::EnumItem(EnumItem {
        enum_name: "ActuatorType".to_owned(),
        item: item.to_owned(),
    })
}

/// What every mover has (Phase 37): the Attachment it acts on, the one it
/// may follow or measure from, and whether it acts.
fn mover_properties() -> Vec<(&'static str, Value)> {
    vec![
        ("Attachment0", Value::Instance(None)),
        ("Attachment1", Value::Instance(None)),
        ("Enabled", Value::Bool(true)),
    ]
}

/// An `ActuatorRelativeTo` item: the frame a mover's vector is in
/// (World, Attachment0 or Attachment1).
fn relative_to(item: &str) -> Value {
    Value::EnumItem(EnumItem {
        enum_name: "ActuatorRelativeTo".to_owned(),
        item: item.to_owned(),
    })
}

/// An align's `Mode` (`enum_name` is PositionAlignmentMode or
/// OrientationAlignmentMode), TwoAttachment to start with.
fn alignment_mode(enum_name: &str) -> Value {
    Value::EnumItem(EnumItem {
        enum_name: enum_name.to_owned(),
        item: "TwoAttachment".to_owned(),
    })
}

/// A UnionOperation's `CollisionFidelity` item: Box, Hull or
/// PreciseConvexDecomposition.
fn collision_fidelity(item: &str) -> Value {
    Value::EnumItem(EnumItem {
        enum_name: "CollisionFidelity".to_owned(),
        item: item.to_owned(),
    })
}

/// What Parts and MeshParts share about what they collide with, touch and
/// are found by (Phase 38).
fn collision_properties() -> [(&'static str, Value); 3] {
    [
        // The collision group it's in (`CollisionGroups`). A name that
        // isn't registered counts as Default.
        (
            "CollisionGroup",
            Value::String(crate::DEFAULT_GROUP.to_owned()),
        ),
        // Off, scripts' raycasts, shape casts and overlap queries pass it by.
        ("CanQuery", Value::Bool(true)),
        // Off, it fires no Touched or TouchEnded, and nothing touching it
        // does for it.
        ("CanTouch", Value::Bool(true)),
    ]
}

/// What Parts and MeshParts share about how they move (Phase 34). Parts
/// welded together move as one assembly (Phase 35).
fn body_properties() -> [(&'static str, Value); 4] {
    [
        // On, the part adds no mass to an assembly it shares with other
        // parts. A part alone keeps its own mass.
        ("Massless", Value::Bool(false)),
        // Density, friction, bounce and their weights; nil uses the
        // material's (`Material::physical`). MeshParts have no Material and
        // use Plastic's.
        ("CustomPhysicalProperties", Value::PhysicalProperties(None)),
        // How fast the part moves, in studs a second, and spins, in radians
        // a second around each axis. Physics writes them as it moves the
        // part; a script or the editor can set them to throw it.
        (
            "AssemblyLinearVelocity",
            Value::Vector3(Vector3::new(0.0, 0.0, 0.0)),
        ),
        (
            "AssemblyAngularVelocity",
            Value::Vector3(Vector3::new(0.0, 0.0, 0.0)),
        ),
    ]
}

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
        // True while it sits on a Seat. Set false to stand it up.
        ("Sit", Value::Bool(false)),
        // Phase 46. Where a controller that doesn't receive input walks:
        // a direction across the ground (its length up to 1 is how much of
        // its speed). The player's controller walks it while no move key
        // is held. Humanoid:Move and MoveTo set it.
        ("MoveDirection", Value::Vector3(Vector3::new(0.0, 0.0, 0.0))),
        // On, the controller neither moves nor jumps the character.
        ("PlatformStand", Value::Bool(false)),
        // On, the character turns to face the way it walks (a first-person
        // one, the way the camera looks).
        ("AutoRotate", Value::Bool(true)),
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
    fn is_a_follows_the_class_hierarchy() {
        assert!(ClassName::Part.is_a("BasePart"));
        assert!(ClassName::MeshPart.is_a("BasePart"));
        assert!(!ClassName::Frame.is_a("BasePart"));
        assert!(ClassName::Frame.is_a("GuiObject"));
        assert!(ClassName::LocalScript.is_a("Script"));
        assert!(!ClassName::Script.is_a("LocalScript"));
        for class in ClassName::ALL {
            assert!(class.is_a("Instance") && class.is_a(class.as_str()));
        }
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
