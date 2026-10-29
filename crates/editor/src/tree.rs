//! The rules for editing the Instance tree: what the Insert buttons and
//! menus make and where it goes, the default names of new Instances, and
//! which Instances can be deleted, duplicated, renamed or moved, and where to.
//!
//! Pure functions of the world, so the rules are easy to test. The app does
//! the changes, and records them for undo.

use axcenity_core::{ClassName, Color3, InstanceId, PartType, UDim2, Value, Vector3, World};

/// Something the toolbar or the Explorer's Insert menu can add.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Insertable {
    Part(PartType),
    /// An imported model. Import Model makes these (with its `MeshId`), not
    /// the Insert buttons and menus.
    MeshPart,
    Model,
    Folder,
    RemoteEvent,
    /// A RemoteFunction, BindableEvent or BindableFunction: like a
    /// RemoteEvent, kept where both sides find it.
    Messenger(ClassName),
    ScreenGui,
    Frame,
    TextLabel,
    TextButton,
    ImageLabel,
    TextBox,
    PageLayout,
    /// A Frame parented under a PageLayout, named as a page.
    Page,
    Table,
    ScrollingFrame,
    Slider,
    Toggle,
    Dropdown,
    ProgressBar,
    BillboardGui,
    /// A ready-made health bar (see [`health_bar`]): a BillboardGui over
    /// the nearest Health's character and a HUD bar, both bound to it.
    HealthBar,
    Sound,
    /// An audio bus (Phase 52): in the selected SoundGroup (a sub-bus), or
    /// in SoundService.
    SoundGroup,
    /// A sound effect, in the selected Sound or SoundGroup, or SoundService.
    SoundEffect(ClassName),
    /// A see-through part whose reverb blends onto spatial Sounds while the
    /// camera is inside it.
    ReverbZone,
    PointLight,
    SpotLight,
    /// A reflection probe, in the selected part or a new invisible box.
    ReflectionProbe,
    /// A light probe volume, in the selected part or a new invisible box.
    LightProbeVolume,
    /// Air that scatters sunlight into the sky, always in Lighting.
    Atmosphere,
    /// A layer of cloud in the sky, always in Lighting.
    Clouds,
    /// Rain or snow and wet or snowy surfaces, always in Lighting.
    Weather,
    ParticleEmitter,
    Animator,
    Animation,
    /// Plays a cutscene sequence (Phase 51), in the Workspace.
    Sequence,
    /// A ragdoll for the selected skinned MeshPart; Build in its Properties
    /// makes its bodies and joints.
    Ragdoll,
    /// Breaks the selected part into pieces; Generate in its Properties
    /// cuts them.
    Fracture,
    /// A kinematic third-person character controller.
    ThirdPersonController,
    /// The same movement, with the camera at the character's eyes.
    FirstPersonController,
    /// Walks a character along the navigation mesh.
    NavigationAgent,
    /// A place a character is put back at.
    Checkpoint,
    /// How much the selected Model can take.
    Health,
    /// A see-through part that damages every Health inside it.
    DamageZone,
    /// A part characters sit on.
    Seat,
    /// A prompt on the selected part or Attachment, held to do something.
    ProximityPrompt,
    /// Makes the selected part clickable.
    ClickDetector,
    /// Something a character carries: a Tool with a Handle part and its
    /// Grip (see [`tool`]).
    Tool,
    /// The Tools the selected character carries.
    Inventory,
    /// A Roblox-familiar front for the selected character's pieces.
    Humanoid,
    /// A pad players' characters spawn on (Phase 47).
    SpawnLocation,
    /// A side players are on, always in Teams.
    Team,
    /// A ready character: a Model of six parts with a Humanoid, a
    /// third-person controller, Health, Inventory and Animator (see
    /// [`character`]).
    Character,
    /// A ready-to-drive car: a Model with a chassis, a VehicleSeat welded
    /// on it, four VehicleWheels and a cylinder for each (see [`vehicle`]).
    Vehicle,
    /// A Seat that drives the car it's welded into.
    VehicleSeat,
    /// A wheel on the selected part.
    VehicleWheel,
    /// A named viewpoint scripts cut the camera to.
    Camera,
    /// A point and direction on the selected part, at its middle.
    Attachment,
    /// An IntValue, StringValue or other Value object.
    Value(ClassName),
}

impl Insertable {
    /// The primitives, in toolbar order.
    pub const PARTS: [Insertable; 4] = [
        Insertable::Part(PartType::Block),
        Insertable::Part(PartType::Ball),
        Insertable::Part(PartType::Cylinder),
        Insertable::Part(PartType::Wedge),
    ];
    /// The classes that group other Instances.
    pub const GROUPS: [Insertable; 2] = [Insertable::Model, Insertable::Folder];
    /// UI objects, for the toolbar's UI group and the Explorer's Insert menu.
    pub const UI: [Insertable; 16] = [
        Insertable::ScreenGui,
        Insertable::Frame,
        Insertable::TextLabel,
        Insertable::TextButton,
        Insertable::ImageLabel,
        Insertable::TextBox,
        Insertable::PageLayout,
        Insertable::Page,
        Insertable::Table,
        Insertable::ScrollingFrame,
        Insertable::Slider,
        Insertable::Toggle,
        Insertable::Dropdown,
        Insertable::ProgressBar,
        Insertable::BillboardGui,
        Insertable::HealthBar,
    ];
    /// Sounds, the groups they play through, and zones that change how
    /// they sound.
    pub const AUDIO: [Insertable; 3] = [
        Insertable::Sound,
        Insertable::SoundGroup,
        Insertable::ReverbZone,
    ];
    /// Effects on a Sound or SoundGroup (Phase 52).
    pub const SOUND_EFFECTS: [Insertable; 8] = [
        Insertable::SoundEffect(ClassName::ReverbSoundEffect),
        Insertable::SoundEffect(ClassName::EqualizerSoundEffect),
        Insertable::SoundEffect(ClassName::CompressorSoundEffect),
        Insertable::SoundEffect(ClassName::EchoSoundEffect),
        Insertable::SoundEffect(ClassName::DistortionSoundEffect),
        Insertable::SoundEffect(ClassName::PitchShiftSoundEffect),
        Insertable::SoundEffect(ClassName::LowPassSoundEffect),
        Insertable::SoundEffect(ClassName::HighPassSoundEffect),
    ];
    /// Lights shine from the part they are inside.
    pub const LIGHTS: [Insertable; 2] = [Insertable::PointLight, Insertable::SpotLight];
    /// Ribbon ▸ Lighting ▸ Reflections.
    pub const PROBES: [Insertable; 2] = [Insertable::ReflectionProbe, Insertable::LightProbeVolume];
    /// Ribbon ▸ Lighting ▸ Sky.
    pub const SKY: [Insertable; 3] = [
        Insertable::Atmosphere,
        Insertable::Clouds,
        Insertable::Weather,
    ];
    /// Effects give off particles from the part they are inside.
    pub const EFFECTS: [Insertable; 1] = [Insertable::ParticleEmitter];
    /// Animators (skeletal) and Animations (property clips).
    /// Ragdolls go in a skinned MeshPart too.
    /// A Sequence plays a cutscene, from the Workspace.
    pub const ANIMATION: [Insertable; 4] = [
        Insertable::Animator,
        Insertable::Animation,
        Insertable::Ragdoll,
        Insertable::Sequence,
    ];
    /// Fractures, which shatter the part they're inside.
    pub const BREAKING: [Insertable; 1] = [Insertable::Fracture];
    /// The character controllers: a character has one or the other.
    pub const CHARACTERS: [Insertable; 2] = [
        Insertable::ThirdPersonController,
        Insertable::FirstPersonController,
    ];
    /// Navigation agents, which walk a character to a destination.
    pub const AGENTS: [Insertable; 1] = [Insertable::NavigationAgent];
    /// Checkpoints, which a game returns the player character to.
    pub const CHECKPOINTS: [Insertable; 1] = [Insertable::Checkpoint];
    /// Health, and the zones that take it away.
    pub const HEALTH: [Insertable; 2] = [Insertable::Health, Insertable::DamageZone];
    /// Seats, which a character that touches them sits on.
    pub const SEATS: [Insertable; 1] = [Insertable::Seat];
    /// What the player uses things with: a prompt to hold a key at, and a
    /// part to click.
    pub const INTERACTION: [Insertable; 2] =
        [Insertable::ProximityPrompt, Insertable::ClickDetector];
    /// A ready character, and the Humanoid that fronts one's pieces.
    pub const HUMANOIDS: [Insertable; 2] = [Insertable::Character, Insertable::Humanoid];
    /// Where players' characters spawn, and the teams they're on.
    pub const SPAWNING: [Insertable; 2] = [Insertable::SpawnLocation, Insertable::Team];
    /// Tools, and the Inventory a character carries them in.
    pub const TOOLS: [Insertable; 2] = [Insertable::Tool, Insertable::Inventory];
    /// A ready car, and the pieces to build one's own.
    pub const VEHICLES: [Insertable; 3] = [
        Insertable::Vehicle,
        Insertable::VehicleSeat,
        Insertable::VehicleWheel,
    ];
    /// Camera viewpoints, placed where the editor's camera is.
    pub const CAMERAS: [Insertable; 1] = [Insertable::Camera];
    /// Points on parts, for constraints and effects.
    pub const CONSTRAINTS: [Insertable; 1] = [Insertable::Attachment];
    /// What scripts ask and tell each other with, after the RemoteEvent.
    pub const MESSENGERS: [Insertable; 3] = [
        Insertable::Messenger(ClassName::RemoteFunction),
        Insertable::Messenger(ClassName::BindableEvent),
        Insertable::Messenger(ClassName::BindableFunction),
    ];
    /// Value objects, which hold one value for scripts.
    pub const VALUES: [Insertable; 8] = [
        Insertable::Value(ClassName::IntValue),
        Insertable::Value(ClassName::NumberValue),
        Insertable::Value(ClassName::StringValue),
        Insertable::Value(ClassName::BoolValue),
        Insertable::Value(ClassName::Vector3Value),
        Insertable::Value(ClassName::Color3Value),
        Insertable::Value(ClassName::CFrameValue),
        Insertable::Value(ClassName::ObjectValue),
    ];

    /// What the buttons and menus call it. A Ball is a "Sphere", as in Studio.
    pub fn label(self) -> &'static str {
        match self {
            Insertable::Part(PartType::Block) => "Block",
            Insertable::Part(PartType::Ball) => "Sphere",
            Insertable::Part(PartType::Cylinder) => "Cylinder",
            Insertable::Part(PartType::Wedge) => "Wedge",
            Insertable::Part(PartType::Capsule) => "Capsule",
            Insertable::MeshPart => "MeshPart",
            Insertable::Model => "Model",
            Insertable::Folder => "Folder",
            Insertable::RemoteEvent => "RemoteEvent",
            Insertable::Messenger(class) => class.as_str(),
            Insertable::ScreenGui => "ScreenGui",
            Insertable::Frame => "Frame",
            Insertable::TextLabel => "TextLabel",
            Insertable::TextButton => "Button",
            Insertable::ImageLabel => "ImageLabel",
            Insertable::TextBox => "TextBox",
            Insertable::PageLayout => "PageLayout",
            Insertable::Page => "Page",
            Insertable::Table => "Table",
            Insertable::ScrollingFrame => "ScrollingFrame",
            Insertable::Slider => "Slider",
            Insertable::Toggle => "Toggle",
            Insertable::Dropdown => "Dropdown",
            Insertable::ProgressBar => "ProgressBar",
            Insertable::BillboardGui => "BillboardGui",
            Insertable::HealthBar => "Health bar",
            Insertable::Sound => "Sound",
            Insertable::SoundGroup => "SoundGroup",
            Insertable::SoundEffect(ClassName::ReverbSoundEffect) => "Reverb",
            Insertable::SoundEffect(ClassName::EqualizerSoundEffect) => "Equalizer",
            Insertable::SoundEffect(ClassName::CompressorSoundEffect) => "Compressor",
            Insertable::SoundEffect(ClassName::EchoSoundEffect) => "Echo",
            Insertable::SoundEffect(ClassName::DistortionSoundEffect) => "Distortion",
            Insertable::SoundEffect(ClassName::PitchShiftSoundEffect) => "Pitch Shift",
            Insertable::SoundEffect(ClassName::LowPassSoundEffect) => "Low Pass",
            Insertable::SoundEffect(ClassName::HighPassSoundEffect) => "High Pass",
            Insertable::SoundEffect(class) => class.as_str(),
            Insertable::ReverbZone => "Reverb Zone",
            Insertable::PointLight => "PointLight",
            Insertable::SpotLight => "SpotLight",
            Insertable::ReflectionProbe => "Reflection Probe",
            Insertable::LightProbeVolume => "Light Probes",
            Insertable::Atmosphere => "Atmosphere",
            Insertable::Clouds => "Clouds",
            Insertable::Weather => "Weather",
            Insertable::ParticleEmitter => "Particles",
            Insertable::Animator => "Animator",
            Insertable::Ragdoll => "Ragdoll",
            Insertable::Fracture => "Fracture",
            Insertable::Animation => "Animation",
            Insertable::Sequence => "Sequence",
            Insertable::ThirdPersonController => "Third Person",
            Insertable::FirstPersonController => "First Person",
            Insertable::NavigationAgent => "Agent",
            Insertable::Checkpoint => "Checkpoint",
            Insertable::Health => "Health",
            Insertable::DamageZone => "Damage Zone",
            Insertable::Seat => "Seat",
            Insertable::ProximityPrompt => "Prompt",
            Insertable::ClickDetector => "Click Detector",
            Insertable::Tool => "Tool",
            Insertable::Inventory => "Inventory",
            Insertable::Humanoid => "Humanoid",
            Insertable::SpawnLocation => "Spawn",
            Insertable::Team => "Team",
            Insertable::Character => "Character",
            Insertable::Vehicle => "Vehicle",
            Insertable::VehicleSeat => "Vehicle Seat",
            Insertable::VehicleWheel => "Wheel",
            Insertable::Camera => "Camera",
            Insertable::Attachment => "Attachment",
            Insertable::Value(class) => class.as_str(),
        }
    }

    pub fn class(self) -> ClassName {
        match self {
            Insertable::Part(_) => ClassName::Part,
            Insertable::MeshPart => ClassName::MeshPart,
            Insertable::Model => ClassName::Model,
            Insertable::Folder => ClassName::Folder,
            Insertable::RemoteEvent => ClassName::RemoteEvent,
            Insertable::Messenger(class) => class,
            Insertable::ScreenGui => ClassName::ScreenGui,
            Insertable::Frame => ClassName::Frame,
            Insertable::TextLabel => ClassName::TextLabel,
            Insertable::TextButton => ClassName::TextButton,
            Insertable::ImageLabel => ClassName::ImageLabel,
            Insertable::TextBox => ClassName::TextBox,
            Insertable::PageLayout => ClassName::PageLayout,
            Insertable::Page => ClassName::Frame,
            Insertable::Table => ClassName::Table,
            Insertable::ScrollingFrame => ClassName::ScrollingFrame,
            Insertable::Slider => ClassName::Slider,
            Insertable::Toggle => ClassName::Toggle,
            Insertable::Dropdown => ClassName::Dropdown,
            Insertable::ProgressBar => ClassName::ProgressBar,
            Insertable::BillboardGui => ClassName::BillboardGui,
            Insertable::HealthBar => ClassName::ProgressBar,
            Insertable::Sound => ClassName::Sound,
            Insertable::SoundGroup => ClassName::SoundGroup,
            Insertable::SoundEffect(class) => class,
            Insertable::ReverbZone => ClassName::ReverbZone,
            Insertable::PointLight => ClassName::PointLight,
            Insertable::SpotLight => ClassName::SpotLight,
            Insertable::ReflectionProbe => ClassName::ReflectionProbe,
            Insertable::LightProbeVolume => ClassName::LightProbeVolume,
            Insertable::Atmosphere => ClassName::Atmosphere,
            Insertable::Clouds => ClassName::Clouds,
            Insertable::Weather => ClassName::Weather,
            Insertable::ParticleEmitter => ClassName::ParticleEmitter,
            Insertable::Animator => ClassName::Animator,
            Insertable::Ragdoll => ClassName::Ragdoll,
            Insertable::Fracture => ClassName::Fracture,
            Insertable::Animation => ClassName::Animation,
            Insertable::Sequence => ClassName::Sequence,
            Insertable::ThirdPersonController => ClassName::ThirdPersonController,
            Insertable::FirstPersonController => ClassName::FirstPersonController,
            Insertable::NavigationAgent => ClassName::NavigationAgent,
            Insertable::Checkpoint => ClassName::Checkpoint,
            Insertable::Health => ClassName::Health,
            Insertable::DamageZone => ClassName::DamageZone,
            Insertable::Seat => ClassName::Seat,
            Insertable::ProximityPrompt => ClassName::ProximityPrompt,
            Insertable::ClickDetector => ClassName::ClickDetector,
            Insertable::Tool => ClassName::Tool,
            Insertable::Inventory => ClassName::Inventory,
            Insertable::Humanoid => ClassName::Humanoid,
            Insertable::SpawnLocation => ClassName::SpawnLocation,
            Insertable::Team => ClassName::Team,
            Insertable::Character => ClassName::Model,
            Insertable::Vehicle => ClassName::Model,
            Insertable::VehicleSeat => ClassName::VehicleSeat,
            Insertable::VehicleWheel => ClassName::VehicleWheel,
            Insertable::Camera => ClassName::Camera,
            Insertable::Attachment => ClassName::Attachment,
            Insertable::Value(class) => class,
        }
    }

    /// UI that shows only inside a ScreenGui: frames, text, buttons and images.
    pub fn is_ui_element(self) -> bool {
        matches!(
            self,
            Insertable::Frame
                | Insertable::TextLabel
                | Insertable::TextButton
                | Insertable::ImageLabel
                | Insertable::TextBox
                | Insertable::PageLayout
                | Insertable::Page
                | Insertable::Table
                | Insertable::ScrollingFrame
                | Insertable::Slider
                | Insertable::Toggle
                | Insertable::Dropdown
                | Insertable::ProgressBar
        )
    }
}

/// True for the classes UI elements can go inside.
fn is_ui_container(world: &World, id: InstanceId) -> bool {
    matches!(
        world.get(id).map(|instance| instance.class()),
        Some(
            ClassName::ScreenGui
                | ClassName::Frame
                | ClassName::PageLayout
                | ClassName::Table
                | ClassName::ScrollingFrame
                | ClassName::BillboardGui
        )
    )
}

/// The ScreenGui a new UI element goes in when nothing UI is selected: the
/// one around `near`, or else the Workspace's first.
fn screen_gui_for(world: &World, near: Option<InstanceId>) -> Option<InstanceId> {
    let is_gui = |id: InstanceId| {
        world
            .get(id)
            .is_some_and(|instance| instance.class() == ClassName::ScreenGui)
    };
    let mut current = near;
    while let Some(id) = current {
        if is_gui(id) && is_saved(world, id) {
            return Some(id);
        }
        current = world.get(id).and_then(|instance| instance.parent());
    }
    world
        .descendants(world.workspace())
        .into_iter()
        .find(|&id| is_gui(id))
}

/// The PageLayout a new page Frame should join: `near` itself, the one
/// that contains it, or else the Workspace's first.
pub fn page_layout_for(world: &World, near: Option<InstanceId>) -> Option<InstanceId> {
    let mut current = near;
    while let Some(id) = current {
        if world
            .get(id)
            .is_some_and(|instance| instance.class() == ClassName::PageLayout)
            && is_saved(world, id)
        {
            return Some(id);
        }
        current = world.get(id).and_then(|instance| instance.parent());
    }
    world
        .descendants(world.workspace())
        .into_iter()
        .find(|&id| {
            world
                .get(id)
                .is_some_and(|instance| instance.class() == ClassName::PageLayout)
        })
}

/// True when a new light would be parented to a part, which is the only
/// place a light is drawn.
pub fn light_has_part_parent(world: &World, near: Option<InstanceId>) -> bool {
    world
        .get(insert_parent(world, near, Insertable::PointLight))
        .is_some_and(|instance| instance.class().is_base_part())
}

/// The size a new part of each shape starts with, in studs. Blocks and
/// wedges are the default Part size; spheres are 2 across; cylinders are 4
/// long (along X) and 2 across, so they look like cylinders straight away.
fn new_part_size(shape: PartType) -> Vector3 {
    match shape {
        PartType::Block | PartType::Wedge => Vector3::new(4.0, 1.0, 2.0),
        PartType::Ball => Vector3::new(2.0, 2.0, 2.0),
        PartType::Cylinder | PartType::Capsule => Vector3::new(4.0, 2.0, 2.0),
    }
}

/// Makes a new `what`, with no parent yet. A part gets its shape and size,
/// and is centered on `position`, and so is a MeshPart.
pub fn create(world: &mut World, what: Insertable, position: Vector3) -> InstanceId {
    let id = world
        .create(what.class())
        .expect("only non-services are insertable");
    let properties = match what {
        Insertable::Part(shape) => vec![
            ("Shape", Value::PartType(shape)),
            ("Size", Value::Vector3(new_part_size(shape))),
            ("Position", Value::Vector3(position)),
        ],
        Insertable::MeshPart => vec![("Position", Value::Vector3(position))],
        Insertable::Seat | Insertable::VehicleSeat => vec![
            ("Size", Value::Vector3(Vector3::new(2.0, 1.0, 2.0))),
            ("Position", Value::Vector3(position)),
        ],
        // A room to walk into, with its bottom on the ground.
        Insertable::ReverbZone => vec![(
            "Position",
            Value::Vector3(Vector3::new(position.x, position.y + 6.0, position.z)),
        )],
        // Big enough to walk into, with its bottom on the ground.
        Insertable::DamageZone => vec![
            ("Size", Value::Vector3(Vector3::new(8.0, 4.0, 8.0))),
            (
                "Position",
                Value::Vector3(Vector3::new(position.x, position.y + 2.0, position.z)),
            ),
        ],
        // A pad with its bottom on the ground.
        Insertable::SpawnLocation => vec![(
            "Position",
            Value::Vector3(Vector3::new(position.x, position.y + 0.5, position.z)),
        )],
        // Scales with the screen, laid out over all of it, so a UI made at
        // one size looks the same at any other.
        Insertable::ScreenGui => vec![("ScaleMode", Value::String("Scale".to_owned()))],
        _ => Vec::new(),
    };
    for (name, value) in properties {
        world
            .set_property(id, name, value)
            .expect("every part has these properties");
    }
    id
}

/// The car Insert > Vehicle makes, with no parent yet: a Model named Car
/// whose 6×1×10 Chassis is two studs above `position` (where its wheels
/// stand), facing -Z, with a VehicleSeat welded on its left-hand side, so a
/// character walking into that side sits on it, and four VehicleWheels
/// (the front two steer), each moving an anchored, uncolliding cylinder.
pub fn vehicle(world: &mut World, position: Vector3) -> InstanceId {
    let at = |x: f32, y: f32, z: f32| {
        Value::Vector3(Vector3::new(position.x + x, position.y + y, position.z + z))
    };
    let car = world.create(ClassName::Model).expect("not a service");
    world.set_name(car, "Car").expect("just made");
    let mut make = |class, name: &str, parent: InstanceId, properties: Vec<(&str, Value)>| {
        let id = world.create(class).expect("not a service");
        world.set_name(id, name).expect("just made");
        for (property, value) in properties {
            world
                .set_property(id, property, value)
                .expect("the class has it");
        }
        world.set_parent(id, Some(parent)).expect("a new Model");
        id
    };
    let chassis = make(
        ClassName::Part,
        "Chassis",
        car,
        vec![
            ("Size", Value::Vector3(Vector3::new(6.0, 1.0, 10.0))),
            ("Position", at(0.0, 2.0, 0.0)),
            ("Color", Value::Color3(Color3::new(0.8, 0.2, 0.18))),
        ],
    );
    let seat = make(
        ClassName::VehicleSeat,
        "DriverSeat",
        car,
        vec![
            ("Size", Value::Vector3(Vector3::new(2.0, 1.0, 2.0))),
            ("Position", at(-2.0, 3.0, 1.0)),
            ("Color", Value::Color3(Color3::new(0.2, 0.2, 0.22))),
        ],
    );
    make(
        ClassName::WeldConstraint,
        "SeatWeld",
        chassis,
        vec![
            ("Part0", Value::Instance(Some(chassis))),
            ("Part1", Value::Instance(Some(seat))),
        ],
    );
    for (name, x, z) in [
        ("FrontLeft", -3.5, -3.5),
        ("FrontRight", 3.5, -3.5),
        ("BackLeft", -3.5, 3.5),
        ("BackRight", 3.5, 3.5),
    ] {
        // A cylinder's round faces are along its X, as a wheel's axle is.
        let shown = make(
            ClassName::Part,
            &format!("{name}Wheel"),
            car,
            vec![
                ("Shape", Value::PartType(PartType::Cylinder)),
                ("Size", Value::Vector3(Vector3::new(0.8, 2.0, 2.0))),
                ("Position", at(x, 1.0, z)),
                ("Color", Value::Color3(Color3::new(0.1, 0.1, 0.1))),
                ("Anchored", Value::Bool(true)),
                ("CanCollide", Value::Bool(false)),
            ],
        );
        make(
            ClassName::VehicleWheel,
            name,
            chassis,
            vec![
                ("Position", Value::Vector3(Vector3::new(x, -1.0, z))),
                ("Steers", Value::Bool(z < 0.0)),
                ("WheelPart", Value::Instance(Some(shown))),
            ],
        );
    }
    car
}

pub use axcenity_character::rig::{CHARACTER_STATES, character};

/// The Tool Insert > Tool makes, with no parent yet: a Tool with a Handle
/// part (a 1 × 1 × 4 stick lying along -Z, centred on `position`) and a
/// Grip Attachment at its back end, so it's held pointing forward.
pub fn tool(world: &mut World, position: Vector3) -> InstanceId {
    let tool = world.create(ClassName::Tool).expect("not a service");
    world.set_name(tool, "Tool").expect("just made");
    let handle = world.create(ClassName::Part).expect("not a service");
    world.set_name(handle, "Handle").expect("just made");
    for (property, value) in [
        ("Size", Value::Vector3(Vector3::new(1.0, 1.0, 4.0))),
        ("Position", Value::Vector3(position)),
    ] {
        world
            .set_property(handle, property, value)
            .expect("a Part has it");
    }
    world.set_parent(handle, Some(tool)).expect("a new Tool");
    let grip = world.create(ClassName::Attachment).expect("not a service");
    world.set_name(grip, "Grip").expect("just made");
    world
        .set_property(
            grip,
            "Position",
            Value::Vector3(Vector3::new(0.0, 0.0, 1.5)),
        )
        .expect("an Attachment has it");
    world.set_parent(grip, Some(handle)).expect("a new part");
    tool
}

/// The CSS class of the bars [`health_bar`] makes.
pub const HEALTH_BAR_CLASS: &str = "health-bar";

/// The stylesheet Insert > UI > Health bar adds, as `health-bar.css`, if
/// no stylesheet has a `.health-bar` rule yet.
pub const HEALTH_BAR_CSS: &str =
    "/* Insert > UI > Health bar. A ProgressBar with an ObjectValue named
   Health inside it shows that Health out of its MaxHealth. */
.health-bar {
  background-color: #1d1d22;
  border: 2px solid #0b0b0d;
  border-radius: 5px;
}
.health-bar::fill {
  background-color: #46c35a;
  border-radius: 3px;
}
";

/// The Model `near` is, or is in: where a new Health goes.
pub fn model_of(world: &World, near: Option<InstanceId>) -> Option<InstanceId> {
    let mut at = near;
    while let Some(id) = at {
        let instance = world.get(id)?;
        if instance.class() == ClassName::Model && is_saved(world, id) {
            return Some(id);
        }
        at = instance.parent();
    }
    None
}

/// The Health a new health bar follows: the one in (or around) `near`'s
/// Model, or else the Workspace's Health whose Model is nearest
/// `position`. `None` when the Workspace has none.
pub fn nearest_health(
    world: &World,
    near: Option<InstanceId>,
    position: Vector3,
) -> Option<InstanceId> {
    let is_health = |id: InstanceId| {
        world
            .get(id)
            .is_some_and(|instance| instance.class() == ClassName::Health)
    };
    let mut at = near;
    while let Some(id) = at {
        let instance = world.get(id)?;
        if is_health(id) {
            return Some(id);
        }
        if let Some(&health) = instance.children().iter().find(|&&child| is_health(child)) {
            return Some(health);
        }
        at = instance.parent();
    }
    let distance = |health: InstanceId| {
        let at = world
            .get(health)
            .and_then(|h| h.parent())
            .and_then(|holder| body_part(world, holder))
            .and_then(|part| world.get(part)?.property("Position")?.as_vector3())
            .unwrap_or_default();
        let (dx, dy, dz) = (at.x - position.x, at.y - position.y, at.z - position.z);
        dx * dx + dy * dy + dz * dz
    };
    world
        .descendants(world.workspace())
        .into_iter()
        .filter(|&id| is_health(id))
        .min_by(|&a, &b| distance(a).total_cmp(&distance(b)))
}

/// The part a billboard over `holder` floats on: `holder` itself if it's a
/// part, else its first part (a character's body).
fn body_part(world: &World, holder: InstanceId) -> Option<InstanceId> {
    let is_part = |id: InstanceId| {
        world
            .get(id)
            .is_some_and(|instance| instance.class().is_base_part())
    };
    if is_part(holder) {
        return Some(holder);
    }
    world
        .descendants(holder)
        .into_iter()
        .find(|&id| is_part(id))
}

/// What [`health_bar`] made, with no parents yet: the billboard goes in
/// `billboard.1` (`None` when there's no Health with a part to float
/// over), the HUD ScreenGui in the Workspace.
pub struct HealthBar {
    pub billboard: Option<(InstanceId, InstanceId)>,
    pub hud: InstanceId,
}

/// Insert > UI > Health bar: a BillboardGui two studs over the part of
/// `health`'s Model, holding a bar, and a ScreenGui with a bar at the top
/// left of the screen. Each bar has the class `health-bar` and an
/// ObjectValue named `Health` pointing at `health` (empty without one), so
/// it follows that Health with no script.
pub fn health_bar(world: &mut World, health: Option<InstanceId>) -> HealthBar {
    let make = |world: &mut World, class, name: &str, properties: Vec<(&str, Value)>| {
        let id = world.create(class).expect("not a service");
        world.set_name(id, name).expect("just made");
        for (property, value) in properties {
            world
                .set_property(id, property, value)
                .expect("the class has it");
        }
        id
    };
    let (shown, max) = health
        .and_then(|id| world.get(id))
        .map(|instance| {
            let number = |name| instance.property(name).and_then(Value::as_number);
            (
                number("Health").unwrap_or(100.0),
                number("MaxHealth").unwrap_or(100.0),
            )
        })
        .unwrap_or((100.0, 100.0));
    let bar = |world: &mut World, parent: InstanceId, size: UDim2, position: UDim2| {
        let bar = make(
            world,
            ClassName::ProgressBar,
            "HealthBar",
            vec![
                ("Class", Value::String(HEALTH_BAR_CLASS.to_owned())),
                ("Size", Value::UDim2(size)),
                ("Position", Value::UDim2(position)),
                ("Value", Value::Number(shown)),
                ("MaxValue", Value::Number(max)),
            ],
        );
        world
            .set_parent(bar, Some(parent))
            .expect("a new UI element");
        let binding = make(
            world,
            ClassName::ObjectValue,
            "Health",
            vec![("Value", Value::Instance(health))],
        );
        world.set_parent(binding, Some(bar)).expect("a new bar");
    };
    let holder = health
        .and_then(|id| world.get(id)?.parent())
        .and_then(|holder| body_part(world, holder));
    let billboard = holder.map(|part| {
        let billboard = make(
            world,
            ClassName::BillboardGui,
            "HealthBillboard",
            vec![
                ("Size", Value::UDim2(UDim2::new(0.0, 120.0, 0.0, 14.0))),
                ("StudsOffset", Value::Vector3(Vector3::new(0.0, 4.0, 0.0))),
                ("MaxDistance", Value::Number(100.0)),
            ],
        );
        bar(
            world,
            billboard,
            UDim2::new(1.0, 0.0, 1.0, 0.0),
            UDim2::default(),
        );
        (billboard, part)
    });
    let hud = make(
        world,
        ClassName::ScreenGui,
        "HealthHud",
        vec![("ScaleMode", Value::String("Scale".to_owned()))],
    );
    bar(
        world,
        hud,
        UDim2::new(0.0, 260.0, 0.0, 22.0),
        UDim2::new(0.0, 24.0, 0.0, 24.0),
    );
    HealthBar { billboard, hud }
}

/// Where a new `what` goes: into `near` if it's ReplicatedStorage or
/// ServerStorage, or a
/// Model or Folder in a place that's saved (see [`is_saved`]), or for an
/// Animation, a part or a ScreenGui or frame it can animate. Otherwise a
/// RemoteEvent (or RemoteFunction, BindableEvent or BindableFunction) goes
/// into ReplicatedStorage (where both sides find it, as in Roblox), and
/// anything else into the Workspace. An Atmosphere, Clouds or Weather
/// always goes into Lighting, and a Team into Teams. A SoundGroup goes into the
/// selected SoundGroup, and a sound effect into the selected Sound or
/// SoundGroup; otherwise both go into SoundService.
pub fn insert_parent(world: &World, near: Option<InstanceId>, what: Insertable) -> InstanceId {
    if matches!(
        what,
        Insertable::Atmosphere | Insertable::Clouds | Insertable::Weather
    ) {
        return world.lighting();
    }
    if what == Insertable::Team {
        return world.teams();
    }
    if matches!(what, Insertable::SoundGroup | Insertable::SoundEffect(_)) {
        let holds = |id: &InstanceId| {
            world.get(*id).is_some_and(|instance| {
                instance.class() == ClassName::SoundGroup
                    || (matches!(what, Insertable::SoundEffect(_))
                        && instance.class() == ClassName::Sound)
            }) && is_saved(world, *id)
        };
        return near.filter(holds).unwrap_or(world.sound_service());
    }
    let default = match what {
        Insertable::RemoteEvent | Insertable::Messenger(_) => world.replicated_storage(),
        what if what.is_ui_element() => screen_gui_for(world, near).unwrap_or(world.workspace()),
        _ => world.workspace(),
    };
    near.filter(|&id| {
        let is_group = world.get(id).is_some_and(|instance| {
            matches!(instance.class(), ClassName::Model | ClassName::Folder)
        });
        let is_ui_container = is_ui_container(world, id);
        let is_sound_container = world.get(id).is_some_and(|instance| {
            instance.class().is_base_part() || instance.class() == ClassName::Model
        });
        // An Attachment's (or wheel's) Position is on its part, so only a
        // part holds one.
        let is_part = world
            .get(id)
            .is_some_and(|instance| instance.class().is_base_part());
        id == world.replicated_storage()
            || id == world.server_storage()
            || (is_group && is_saved(world, id))
            // A Value object goes inside whatever is selected.
            || (matches!(what, Insertable::Value(_)) && is_saved(world, id))
            || (matches!(
                what,
                Insertable::Attachment | Insertable::VehicleWheel | Insertable::ClickDetector
            ) && is_part
                && is_saved(world, id))
            // A prompt shows over its part or Attachment.
            || (what == Insertable::ProximityPrompt
                && (is_part
                    || world
                        .get(id)
                        .is_some_and(|instance| instance.class() == ClassName::Attachment))
                && is_saved(world, id))
            || (is_ui_container
                && (what.is_ui_element() || what == Insertable::Animation)
                && is_saved(world, id))
            || (is_sound_container
                && matches!(
                    what,
                    Insertable::Sound
                        | Insertable::PointLight
                        | Insertable::SpotLight
                        | Insertable::ReflectionProbe
                        | Insertable::LightProbeVolume
                        | Insertable::ParticleEmitter
                        | Insertable::BillboardGui
                        | Insertable::Animator
                        | Insertable::Ragdoll
                        | Insertable::Fracture
                        | Insertable::Animation
                        | Insertable::ThirdPersonController
                        | Insertable::FirstPersonController
                        | Insertable::NavigationAgent
                )
                && is_saved(world, id))
    })
    .unwrap_or(default)
}

/// The middle of the top of the Part or MeshPart `id`, where a new
/// Checkpoint goes. `None` for anything else.
pub fn top_of(world: &World, id: Option<InstanceId>) -> Option<Vector3> {
    let instance = world.get(id?)?;
    if !instance.class().is_base_part() {
        return None;
    }
    let position = instance.property("Position")?.as_vector3()?;
    let size = instance.property("Size")?.as_vector3()?;
    Some(Vector3::new(
        position.x,
        position.y + size.y.abs() / 2.0,
        position.z,
    ))
}

/// True when a character controller already moves `id`: one is inside it,
/// or inside a Model or part that holds it.
pub fn has_controller(world: &World, id: InstanceId) -> bool {
    let is_controller = |id: InstanceId| {
        world
            .get(id)
            .is_some_and(|instance| instance.class().is_character_controller())
    };
    if world.descendants(id).into_iter().any(is_controller) {
        return true;
    }
    let mut holder = world.get(id).and_then(|instance| instance.parent());
    while let Some(parent) = holder {
        let Some(instance) = world.get(parent) else {
            break;
        };
        if instance
            .children()
            .iter()
            .any(|&child| is_controller(child))
        {
            return true;
        }
        holder = instance.parent();
    }
    false
}

/// True if a new `what` needs a ScreenGui made first: it's UI, and there's
/// no ScreenGui or Frame for it to go in.
pub fn needs_screen_gui(world: &World, near: Option<InstanceId>, what: Insertable) -> bool {
    what.is_ui_element() && !is_ui_container(world, insert_parent(world, near, what))
}

/// True if `id` is in the Workspace, ReplicatedStorage, ServerStorage, Teams
/// or SoundService: the places the world file saves what's inside.
pub fn is_saved(world: &World, id: InstanceId) -> bool {
    world.is_descendant_of(id, world.workspace())
        || world.is_descendant_of(id, world.replicated_storage())
        || world.is_descendant_of(id, world.server_storage())
        || world.is_descendant_of(id, world.teams())
        || world.is_descendant_of(id, world.sound_service())
}

/// A name that none of `parent`'s children has: `wanted` itself if it's
/// free, otherwise with the first free number from 2 on the end ("Part",
/// "Part2", "Part3"). A number already on the end is replaced, so a copy of
/// "Part2" is "Part3", not "Part22".
pub fn unique_name(world: &World, parent: InstanceId, wanted: &str) -> String {
    numbered_names(wanted)
        .find(|name| world.find_first_child(parent, name).is_none())
        .expect("some number is always free")
}

/// `wanted`, then `wanted` with 2, 3, 4… on the end, without end. A number
/// already on the end is replaced.
pub fn numbered_names(wanted: &str) -> impl Iterator<Item = String> {
    let stem = wanted.trim_end_matches(|c: char| c.is_ascii_digit());
    // A name that is all digits, such as "7", keeps them: "72", "73".
    let stem = if stem.is_empty() { wanted } else { stem }.to_string();
    std::iter::once(wanted.to_string()).chain((2u64..).map(move |number| format!("{stem}{number}")))
}

/// Whether `id` can be deleted, duplicated or moved, or why not, in words
/// for a tooltip.
pub fn check_editable(world: &World, id: InstanceId) -> Result<(), &'static str> {
    let instance = world.get(id).ok_or("It no longer exists.")?;
    if instance.class().is_service() {
        return Err("Services are always there, so they can't be deleted, copied or moved.");
    }
    if instance.class().is_script() {
        // A script is its `.lua` file, and its Instance always sits in its
        // service; copying it would mean copying the file.
        return Err("Scripts can't be copied or moved yet. Insert a new one instead.");
    }
    if instance.class() == ClassName::Player {
        return Err("Players only exist while the game runs.");
    }
    if !is_saved(world, id) {
        return Err(
            "Only things in the Workspace and ReplicatedStorage are saved, so only those can be changed.",
        );
    }
    Ok(())
}

/// Whether `id` can be deleted, or why not. Scripts can, unlike in
/// [`check_editable`]: deleting one deletes its file.
pub fn check_delete(world: &World, id: InstanceId) -> Result<(), &'static str> {
    match world.get(id) {
        Some(instance) if instance.class().is_script() => Ok(()),
        _ => check_editable(world, id),
    }
}

/// Whether `id` can be moved into `parent`, or why not.
pub fn check_move(world: &World, id: InstanceId, parent: InstanceId) -> Result<(), &'static str> {
    check_editable(world, id)?;
    if parent == id || world.is_descendant_of(parent, id) {
        return Err("Something can't go inside itself.");
    }
    if world
        .get(parent)
        .is_some_and(|parent| parent.class().is_script())
    {
        // A script is a file; what's inside it couldn't be saved.
        return Err("Scripts can't hold other things. Use a Folder or a Model.");
    }
    let saved_service = parent == world.workspace() || parent == world.replicated_storage();
    if !saved_service && !is_saved(world, parent) {
        // The world file only holds those two, so it would be lost on saving.
        return Err(
            "Only things in the Workspace and ReplicatedStorage are saved, so it has to stay in one of them.",
        );
    }
    Ok(())
}

/// True if `id` can go in a prefab: something in the Workspace that isn't
/// a script, a service or UI.
pub fn can_be_prefab(world: &World, id: InstanceId) -> bool {
    let Some(instance) = world.get(id) else {
        return false;
    };
    let class = instance.class();
    !class.is_service()
        && !class.is_script()
        && class != ClassName::ScreenGui
        && world.is_descendant_of(id, world.workspace())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_character_makes_a_welded_five_stud_rig_with_every_piece() {
        let mut world = World::new();
        let at = Vector3::new(4.0, 0.0, -2.0);
        let path = "assets/animations/Character/Character.statemachine.json";
        let model = character(&mut world, at, path);
        let names: Vec<(ClassName, String)> = world
            .get(model)
            .unwrap()
            .children()
            .iter()
            .map(|&id| {
                let child = world.get(id).unwrap();
                (child.class(), child.name().to_owned())
            })
            .collect();
        for class in [
            ClassName::Humanoid,
            ClassName::ThirdPersonController,
            ClassName::Health,
            ClassName::Inventory,
            ClassName::Animator,
        ] {
            assert_eq!(
                names.iter().filter(|(c, _)| *c == class).count(),
                1,
                "{class}"
            );
        }
        let parts: Vec<InstanceId> = world
            .descendants(model)
            .into_iter()
            .filter(|&id| world.get(id).unwrap().class() == ClassName::Part)
            .collect();
        assert_eq!(parts.len(), 6);
        // A part named Right Arm, where Tools are held.
        assert!(world.find_first_child(model, "Right Arm").is_some());
        let (mut low, mut high) = (f32::INFINITY, f32::NEG_INFINITY);
        for &part in &parts {
            let instance = world.get(part).unwrap();
            let position = instance.property("Position").unwrap().as_vector3().unwrap();
            let size = instance.property("Size").unwrap().as_vector3().unwrap();
            low = low.min(position.y - size.y / 2.0);
            high = high.max(position.y + size.y / 2.0);
        }
        assert_eq!((low, high), (0.0, 5.0));
        let welds = world
            .descendants(model)
            .into_iter()
            .filter(|&id| world.get(id).unwrap().class() == ClassName::WeldConstraint)
            .count();
        assert_eq!(welds, 5);
        let animator = world.find_first_child(model, "Animator").unwrap();
        assert_eq!(
            world.get(animator).unwrap().property("StateMachine"),
            Some(&Value::String(path.to_owned()))
        );
    }

    fn child(world: &mut World, parent: InstanceId, class: ClassName, name: &str) -> InstanceId {
        let id = world.create(class).unwrap();
        world.set_name(id, name).unwrap();
        world.set_parent(id, Some(parent)).unwrap();
        id
    }

    #[test]
    fn new_names_are_unique_among_siblings() {
        let mut world = World::new();
        let workspace = world.workspace();
        assert_eq!(unique_name(&world, workspace, "Part"), "Part");
        child(&mut world, workspace, ClassName::Part, "Part");
        assert_eq!(unique_name(&world, workspace, "Part"), "Part2");
        child(&mut world, workspace, ClassName::Part, "Part2");
        assert_eq!(unique_name(&world, workspace, "Part"), "Part3");
        // The number on the end is replaced, not added to.
        assert_eq!(unique_name(&world, workspace, "Part2"), "Part3");
        // Only siblings count: an empty Model has room for "Part".
        let model = child(&mut world, workspace, ClassName::Model, "Model");
        assert_eq!(unique_name(&world, model, "Part"), "Part");
        child(&mut world, workspace, ClassName::Part, "7");
        assert_eq!(unique_name(&world, workspace, "7"), "72");
    }

    #[test]
    fn each_primitive_gets_its_shape_size_and_position() {
        let mut world = World::new();
        let at = Vector3::new(1.0, 2.0, 3.0);
        for what in Insertable::PARTS {
            let id = create(&mut world, what, at);
            let part = world.get(id).unwrap();
            let Insertable::Part(shape) = what else {
                unreachable!()
            };
            assert_eq!(part.property("Shape"), Some(&Value::PartType(shape)));
            assert_eq!(part.property("Position"), Some(&Value::Vector3(at)));
            assert_eq!(
                part.property("Size"),
                Some(&Value::Vector3(new_part_size(shape)))
            );
            assert_eq!(part.parent(), None);
        }
        let model = create(&mut world, Insertable::Model, at);
        assert_eq!(world.get(model).unwrap().class(), ClassName::Model);
    }

    #[test]
    fn prompts_go_on_a_part_or_attachment_and_click_detectors_on_a_part() {
        let mut world = World::new();
        let workspace = world.workspace();
        let door = child(&mut world, workspace, ClassName::Part, "Door");
        let handle = child(&mut world, door, ClassName::Attachment, "Handle");
        let sound = child(&mut world, door, ClassName::Sound, "Creak");
        let prompt = Insertable::ProximityPrompt;
        let detector = Insertable::ClickDetector;
        assert_eq!(insert_parent(&world, Some(door), prompt), door);
        assert_eq!(insert_parent(&world, Some(handle), prompt), handle);
        assert_eq!(insert_parent(&world, Some(door), detector), door);
        assert_eq!(insert_parent(&world, Some(handle), detector), workspace);
        assert_eq!(insert_parent(&world, Some(sound), prompt), workspace);
    }

    #[test]
    fn new_things_go_into_a_selected_model_or_folder_otherwise_the_workspace() {
        let mut world = World::new();
        let workspace = world.workspace();
        let folder = child(&mut world, workspace, ClassName::Folder, "Folder");
        let model = child(&mut world, folder, ClassName::Model, "Model");
        let part = child(&mut world, model, ClassName::Part, "Part");
        let block = Insertable::Part(PartType::Block);
        assert_eq!(insert_parent(&world, Some(folder), block), folder);
        assert_eq!(insert_parent(&world, Some(model), block), model);
        assert_eq!(insert_parent(&world, Some(part), block), workspace);
        assert_eq!(insert_parent(&world, None, block), workspace);
        assert_eq!(insert_parent(&world, Some(world.root()), block), workspace);
        // A Folder outside the saved places wouldn't be saved.
        let loose = world.create(ClassName::Folder).unwrap();
        assert_eq!(insert_parent(&world, Some(loose), block), workspace);
    }

    #[test]
    fn remote_events_go_into_replicated_storage_unless_a_group_is_selected() {
        let mut world = World::new();
        let storage = world.replicated_storage();
        let remote = Insertable::RemoteEvent;
        assert_eq!(insert_parent(&world, None, remote), storage);
        assert_eq!(
            insert_parent(&world, Some(world.workspace()), remote),
            storage
        );
        let remotes = child(&mut world, storage, ClassName::Folder, "Remotes");
        assert_eq!(insert_parent(&world, Some(remotes), remote), remotes);
        // Right-clicking ReplicatedStorage inserts anything there.
        let folder = Insertable::Folder;
        assert_eq!(insert_parent(&world, Some(storage), folder), storage);
        let id = create(&mut world, remote, Vector3::new(0.0, 0.0, 0.0));
        assert_eq!(world.get(id).unwrap().class(), ClassName::RemoteEvent);
    }

    #[test]
    fn ui_insertions_nest_under_screen_guis_and_frames() {
        let mut world = World::new();
        let workspace = world.workspace();
        let gui = child(&mut world, workspace, ClassName::ScreenGui, "Hud");
        let frame = child(&mut world, gui, ClassName::Frame, "Frame");

        assert_eq!(insert_parent(&world, Some(gui), Insertable::Frame), gui);
        assert_eq!(
            insert_parent(&world, Some(frame), Insertable::TextLabel),
            frame
        );
        assert_eq!(
            insert_parent(&world, Some(frame), Insertable::ScreenGui),
            workspace
        );
        // With a part selected, or nothing, UI still goes in the ScreenGui.
        let part = child(&mut world, workspace, ClassName::Part, "Part");
        assert_eq!(
            insert_parent(&world, Some(part), Insertable::TextButton),
            gui
        );
        assert_eq!(insert_parent(&world, None, Insertable::ImageLabel), gui);
        assert!(!needs_screen_gui(&world, None, Insertable::Frame));
        assert!(needs_screen_gui(&World::new(), None, Insertable::Frame));
        assert!(!needs_screen_gui(
            &World::new(),
            None,
            Insertable::ScreenGui
        ));
    }

    #[test]
    fn moves_into_your_own_descendant_or_out_of_the_workspace_are_rejected() {
        let mut world = World::new();
        let workspace = world.workspace();
        let model = child(&mut world, workspace, ClassName::Model, "Car");
        let wheel = child(&mut world, model, ClassName::Part, "Wheel");
        let hub = child(&mut world, wheel, ClassName::Part, "Hub");
        let other = child(&mut world, workspace, ClassName::Folder, "Props");

        assert_eq!(check_move(&world, wheel, other), Ok(()));
        assert_eq!(check_move(&world, hub, workspace), Ok(()));
        assert!(check_move(&world, model, model).is_err());
        assert!(check_move(&world, model, wheel).is_err());
        assert!(check_move(&world, model, hub).is_err());
        assert!(check_move(&world, model, world.server_script_service()).is_err());
        assert!(check_move(&world, model, world.root()).is_err());
        assert!(check_move(&world, workspace, other).is_err());
        // ReplicatedStorage is saved too.
        let storage = world.replicated_storage();
        assert_eq!(check_move(&world, model, storage), Ok(()));
        let remotes = child(&mut world, storage, ClassName::Folder, "Remotes");
        assert_eq!(check_move(&world, wheel, remotes), Ok(()));
        assert_eq!(check_editable(&world, remotes), Ok(()));
    }

    #[test]
    fn services_cannot_be_deleted_copied_or_moved_and_scripts_can_only_be_deleted() {
        let mut world = World::new();
        let script_service = world.server_script_service();
        let script = child(&mut world, script_service, ClassName::Script, "door");
        let client = world.starter_player_scripts();
        let local_script = child(&mut world, client, ClassName::LocalScript, "keys");
        let services = [
            world.root(),
            world.workspace(),
            world.players(),
            world.replicated_storage(),
            script_service,
            client,
        ];
        for id in services.into_iter().chain([script, local_script]) {
            assert!(check_editable(&world, id).is_err());
        }
        for id in services {
            assert!(check_delete(&world, id).is_err());
        }
        assert_eq!(check_delete(&world, script), Ok(()));
        assert_eq!(check_delete(&world, local_script), Ok(()));
        let workspace = world.workspace();
        let part = child(&mut world, workspace, ClassName::Part, "Part");
        assert_eq!(check_editable(&world, part), Ok(()));
    }
}
