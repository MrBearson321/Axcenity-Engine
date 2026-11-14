//! The animation files in a project: clips (`*.anim.json`) and state
//! machines (`*.statemachine.json`), plain JSON like the world file.
//!
//! There is **one clip type**. A clip has a duration, whether it loops,
//! events, and tracks: each track animates a bone or a property of
//! something named by its path from the Animator's parent. A clip can also
//! be **linked** to a take in a model file: its bones then move as the take
//! does, and its own tracks animate only what the take doesn't move.
//!
//! A state machine holds an Animator's parameters, states, default state
//! and transitions. Paths in these files are from the project folder, with
//! `/` between folders. Each animated thing has its own folder,
//! `assets/animations/<name>/`, holding its clips and its state machine;
//! shared clips go in `assets/animations/Shared/`.
//!
//! Files from before format 2 (take clips, property clips with
//! `"kind": "property"`, and `*.controller.json` controllers) are still
//! read, turned into this shape; projects are upgraded once when they open.

use std::path::Path;

use axcenity_core::face::FACE_CONTROLS;
use serde::{Deserialize, Serialize};

use crate::easing::{DEFAULT_BEZIER, Easing, cubic_bezier};

/// The version these files are written in. Newer and older ones are
/// refused: the project upgrade (format 3, `project`'s
/// `migrate_animation.rs`) rewrote every format 1 file in this shape.
pub const FORMAT_VERSION: u32 = 2;
/// Where the editor keeps clips and state machines.
pub const ANIMATIONS_FOLDER: &str = "assets/animations";
/// The folder in [`ANIMATIONS_FOLDER`] for clips any model can play.
pub const SHARED_FOLDER: &str = "Shared";
pub const CLIP_EXTENSION: &str = ".anim.json";
pub const STATE_MACHINE_EXTENSION: &str = ".statemachine.json";
/// The name a transition's `from` uses for "from whatever state is playing".
pub const ANY_STATE: &str = "Any State";

fn one() -> f64 {
    1.0
}

fn yes() -> bool {
    true
}

fn is_false(value: &bool) -> bool {
    !value
}

/// A clip file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClipFile {
    pub format_version: u32,
    /// Seconds one pass lasts. Keys and events lie in `0..=duration`.
    pub duration: f64,
    /// On, it plays again from the start when it ends; off, it holds its
    /// last frame.
    #[serde(rename = "loop", default = "yes")]
    pub looping: bool,
    /// The take its bones' motion is read from, if it's linked to one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<TakeLink>,
    #[serde(default)]
    pub tracks: Vec<PropertyTrack>,
    /// Named moments, which fire events as playback passes them.
    #[serde(default)]
    pub events: Vec<ClipEvent>,
}

/// A clip's link to part of a take in a model file. Its bones move as the
/// take does from `start` to `end` over the clip's duration (so a shorter
/// duration plays it faster, and `end` before `start` plays it backwards).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TakeLink {
    /// The model file the take is in, such as `assets/models/hero.fbx`.
    pub source: String,
    /// The take's name in that file, such as `mixamo.com` or `Take 001`.
    pub take: String,
    /// Where the clip starts and ends in the take, in seconds.
    pub start: f64,
    pub end: f64,
    /// On, the root bone's travel across the ground moves the MeshPart
    /// instead of the bone.
    #[serde(default, skip_serializing_if = "is_false")]
    pub root_motion: bool,
    /// The retarget profile (`*.retarget.json`, AP2) that says how the take
    /// plays on another skeleton, if the clip has one. With one, the take
    /// is always retargeted, even onto a skeleton whose bone names match.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retarget: Option<String>,
}

impl TakeLink {
    /// Seconds of the take it covers, never negative.
    pub fn length(&self) -> f64 {
        (self.end - self.start).abs()
    }
}

impl ClipFile {
    /// An empty clip `duration` seconds long that doesn't loop, for keys.
    pub fn new(duration: f64) -> Self {
        Self {
            format_version: FORMAT_VERSION,
            duration,
            looping: false,
            link: None,
            tracks: Vec::new(),
            events: Vec::new(),
        }
    }

    /// A looping clip linked to `take` in `source` from `start` to `end`
    /// seconds, playing it at its own speed.
    pub fn linked(source: &str, take: &str, start: f64, end: f64) -> Self {
        Self {
            format_version: FORMAT_VERSION,
            duration: (end - start).abs(),
            looping: true,
            link: Some(TakeLink {
                source: source.to_owned(),
                take: take.to_owned(),
                start,
                end,
                root_motion: false,
                retarget: None,
            }),
            tracks: Vec::new(),
            events: Vec::new(),
        }
    }

    /// How fast a linked clip plays its take: 2 is twice as fast, and a
    /// negative speed plays it backwards. 1 for a clip with no link.
    pub fn speed(&self) -> f64 {
        match &self.link {
            Some(link) if self.duration > 0.0 => (link.end - link.start) / self.duration,
            _ => 1.0,
        }
    }

    pub fn from_json(text: &str) -> Result<Self, FileError> {
        let value: serde_json::Value = parse(text)?;
        let version = value
            .get("format_version")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0) as u32;
        check_version(version)?;
        // Read from the text, so a mistake names its line.
        let clip = parse::<Self>(text)?;
        if !clip.duration.is_finite() || clip.duration < 0.0 {
            return Err(FileError(
                "its duration must be a number of seconds, 0 or more".into(),
            ));
        }
        if let Some(link) = &clip.link
            && !(link.start.is_finite() && link.end.is_finite())
        {
            return Err(FileError("its link's start and end must be numbers".into()));
        }
        Ok(clip)
    }

    /// Readable JSON, the same for the same clip.
    pub fn to_json(&self) -> String {
        to_json(self)
    }
}

/// A named moment in a clip.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClipEvent {
    /// Seconds from the clip's start.
    pub time: f64,
    pub name: String,
}

/// How a keyframe reaches the next one. `Step` holds the key's value
/// until the next (the editor calls it Constant); `Ease` follows the key's
/// [`Easing`] and `Bezier` its handles.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Interp {
    Step,
    #[default]
    Linear,
    Ease,
    Bezier,
}

/// What a track drives. On a Model, `Position` and `Orientation` are its
/// pivot (see `property_clip.rs`); on a UI element, `Position` and `Size`
/// are UDim2s. `Bone` is a bone's transform, for a track whose target
/// names a bone. `Face` is one of a FaceControls' standard controls (by
/// its index in `core::face::FACE_CONTROLS`), written by its name
/// (`"Jaw_Open"`), facial animation F4b.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PropertyName {
    Position,
    Orientation,
    Size,
    Color,
    Transparency,
    BackgroundColor3,
    BackgroundTransparency,
    TextColor3,
    TextSize,
    ImageTransparency,
    /// A Camera's (and a sequence's own camera keys', Phase 51).
    FieldOfView,
    Bone,
    Face(u8),
}

impl Serialize for PropertyName {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for PropertyName {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let name = String::deserialize(deserializer)?;
        PropertyName::from_name(&name).ok_or_else(|| {
            serde::de::Error::custom(format!(
                "`{name}` isn't a property a track can key (Position, Orientation, Size, Color, Transparency, BackgroundColor3, BackgroundTransparency, TextColor3, TextSize, ImageTransparency, FieldOfView, Bone, or a face control such as Jaw_Open)"
            ))
        })
    }
}

impl PropertyName {
    /// Every property but the face controls ([`PropertyName::face`]).
    pub const ALL: [PropertyName; 12] = [
        PropertyName::Position,
        PropertyName::Orientation,
        PropertyName::Size,
        PropertyName::Color,
        PropertyName::Transparency,
        PropertyName::BackgroundColor3,
        PropertyName::BackgroundTransparency,
        PropertyName::TextColor3,
        PropertyName::TextSize,
        PropertyName::ImageTransparency,
        PropertyName::FieldOfView,
        PropertyName::Bone,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            PropertyName::Position => "Position",
            PropertyName::Orientation => "Orientation",
            PropertyName::Size => "Size",
            PropertyName::Color => "Color",
            PropertyName::Transparency => "Transparency",
            PropertyName::BackgroundColor3 => "BackgroundColor3",
            PropertyName::BackgroundTransparency => "BackgroundTransparency",
            PropertyName::TextColor3 => "TextColor3",
            PropertyName::TextSize => "TextSize",
            PropertyName::ImageTransparency => "ImageTransparency",
            PropertyName::FieldOfView => "FieldOfView",
            PropertyName::Bone => "Bone",
            PropertyName::Face(control) => FACE_CONTROLS
                .get(control as usize)
                .map_or("Face", |control| control.name),
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|p| p.as_str() == name)
            .or_else(|| Self::face(name))
    }

    /// The standard face control `name` (`"Jaw_Open"`), keyed on a
    /// FaceControls.
    pub fn face(name: &str) -> Option<Self> {
        let index = axcenity_core::face::face_control(name)?;
        Some(PropertyName::Face(index as u8))
    }

    /// Every face control, in the standard order.
    pub fn face_controls() -> impl Iterator<Item = PropertyName> {
        (0..FACE_CONTROLS.len()).map(|index| PropertyName::Face(index as u8))
    }

    /// Every property a track can key: [`PropertyName::ALL`], then the
    /// face controls.
    pub fn keyable() -> impl Iterator<Item = PropertyName> {
        Self::ALL.into_iter().chain(Self::face_controls())
    }

    /// Whether this is a FaceControls' control.
    pub fn is_face(self) -> bool {
        matches!(self, PropertyName::Face(_))
    }

    /// How many numbers a key of this property may hold: 3 for a Vector3 or
    /// Color3, 4 for a UDim2 (`Position` and `Size` of UI elements), 1 for
    /// a number, and 10 for a bone (translation, rotation as a quaternion
    /// `x, y, z, w`, and scale, relative to its parent bone).
    pub fn widths(self) -> &'static [usize] {
        match self {
            PropertyName::Position | PropertyName::Size => &[3, 4],
            PropertyName::Orientation
            | PropertyName::Color
            | PropertyName::BackgroundColor3
            | PropertyName::TextColor3 => &[3],
            PropertyName::Transparency
            | PropertyName::BackgroundTransparency
            | PropertyName::TextSize
            | PropertyName::ImageTransparency
            | PropertyName::FieldOfView
            | PropertyName::Face(_) => &[1],
            PropertyName::Bone => &[10],
        }
    }
}

/// One key on a track.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PropertyKeyframe {
    pub id: String,
    pub time: f64,
    /// The value as numbers: `[x, y, z]` for a Vector3 (degrees for
    /// Orientation), `[r, g, b]` (0 to 1) for a Color3,
    /// `[x_scale, x_offset, y_scale, y_offset]` for a UDim2, `[n]` for a
    /// number, and 10 numbers for a bone (see [`PropertyName::widths`]).
    pub value: Vec<f32>,
    #[serde(default)]
    pub interp: Interp,
    /// An `Ease` key's style and direction (Quad InOut if left out).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub easing: Option<Easing>,
    /// A `Bezier` key's handles to the next key, `[x1, y1, x2, y2]` as in
    /// CSS's `cubic-bezier`: `x` is time and `y` how far along, both `0`
    /// to `1` across the gap ([`DEFAULT_BEZIER`] if left out).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bezier: Option<[f32; 4]>,
}

impl PropertyKeyframe {
    /// A linear key.
    pub fn new(id: impl Into<String>, time: f64, value: Vec<f32>) -> Self {
        Self {
            id: id.into(),
            time,
            value,
            interp: Interp::Linear,
            easing: None,
            bezier: None,
        }
    }

    /// How far along the motion to the next key is at `t`, `0` to `1` of
    /// the way there in time (`0` until the next key for a Step key).
    pub fn progress(&self, t: f32) -> f32 {
        match self.interp {
            Interp::Step => 0.0,
            Interp::Linear => t.clamp(0.0, 1.0),
            Interp::Ease => self.easing.unwrap_or_default().apply(t),
            Interp::Bezier => cubic_bezier(self.bezier.unwrap_or(DEFAULT_BEZIER), t),
        }
    }
}

/// One animated bone or property on a relative target.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PropertyTrack {
    pub id: String,
    /// From the Animator's parent: `.` is that Part/Model, `Hinge/Panel` is
    /// the child `Panel` of its child `Hinge`, and a bone is named the same
    /// way, by the bones above it (`Hips/Spine`).
    pub target: String,
    pub property: PropertyName,
    pub keys: Vec<PropertyKeyframe>,
}

/// A state machine file: an Animator's parameters, states and transitions.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateMachineFile {
    pub format_version: u32,
    #[serde(default)]
    pub parameters: Vec<ParameterDef>,
    /// The state it starts in (Idle, usually). Empty means the first.
    #[serde(default)]
    pub default_state: String,
    #[serde(default)]
    pub states: Vec<StateDef>,
    #[serde(default)]
    pub transitions: Vec<TransitionDef>,
    /// Where the editor's graph draws the Any State node, once it's moved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub any_state_position: Option<[f32; 2]>,
    /// Where the editor's graph draws the Entry node, once it's moved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry_position: Option<[f32; 2]>,
    /// Layers over the states above (the base layer), each with its own
    /// states (Phase 78). Left out when there are none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub layers: Vec<LayerDef>,
}

impl Default for StateMachineFile {
    fn default() -> Self {
        Self {
            format_version: FORMAT_VERSION,
            parameters: Vec::new(),
            default_state: String::new(),
            states: Vec::new(),
            transitions: Vec::new(),
            any_state_position: None,
            entry_position: None,
            layers: Vec::new(),
        }
    }
}

impl StateMachineFile {
    /// The state named `name`.
    pub fn state(&self, name: &str) -> Option<&StateDef> {
        self.states.iter().find(|state| state.name == name)
    }

    /// The state it starts in: `default_state`, or the first if that's empty
    /// or isn't one of its states.
    pub fn starting_state(&self) -> Option<&StateDef> {
        self.state(&self.default_state).or(self.states.first())
    }

    /// Reads a state machine, or an animator controller saved before
    /// format 2 (the same content).
    pub fn from_json(text: &str) -> Result<Self, FileError> {
        let mut machine: Self = parse(text)?;
        check_version(machine.format_version)?;
        machine.format_version = FORMAT_VERSION;
        Ok(machine)
    }

    /// Readable JSON, the same for the same state machine.
    pub fn to_json(&self) -> String {
        to_json(self)
    }

    /// The layer named `name`.
    pub fn layer(&self, name: &str) -> Option<&LayerDef> {
        self.layers.iter().find(|layer| layer.name == name)
    }

    /// Every state, the base layer's first, each with its layer's name
    /// (empty for the base layer).
    pub fn all_states(&self) -> impl Iterator<Item = (&str, &StateDef)> {
        let layers = self.layers.iter().flat_map(|layer| {
            layer
                .states
                .iter()
                .map(move |state| (layer.name.as_str(), state))
        });
        self.states.iter().map(|state| ("", state)).chain(layers)
    }

    /// Every state of every layer, to change.
    pub fn all_states_mut(&mut self) -> impl Iterator<Item = &mut StateDef> {
        let layers = self
            .layers
            .iter_mut()
            .flat_map(|layer| layer.states.iter_mut());
        self.states.iter_mut().chain(layers)
    }

    /// The states of the layer named `layer` (the base layer's for "").
    pub fn layer_states_mut(&mut self, layer: &str) -> Option<&mut Vec<StateDef>> {
        if layer.is_empty() {
            return Some(&mut self.states);
        }
        self.layers
            .iter_mut()
            .find(|found| found.name == layer)
            .map(|found| &mut found.states)
    }
}

/// A layer of a state machine (Phase 78): its own states and transitions,
/// sharing the file's parameters, mixed over the layers under it by its
/// weight, on the bones its mask picks. An upper-body layer can reload
/// while the base layer runs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LayerDef {
    pub name: String,
    /// How much it counts when the game starts, from 0 to 1. Scripts change
    /// it with `animator:SetLayerWeight`.
    #[serde(default = "one")]
    pub weight: f64,
    #[serde(default)]
    pub mode: LayerMode,
    /// The bones it moves, each with every bone under it (`Hips/Spine` is
    /// the spine and up). Empty is the whole skeleton.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mask: Vec<String>,
    /// The state it starts in. Empty means the first.
    #[serde(default)]
    pub default_state: String,
    #[serde(default)]
    pub states: Vec<StateDef>,
    #[serde(default)]
    pub transitions: Vec<TransitionDef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub any_state_position: Option<[f32; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry_position: Option<[f32; 2]>,
}

impl LayerDef {
    /// An empty Override layer at full weight over the whole skeleton.
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            weight: 1.0,
            mode: LayerMode::Override,
            mask: Vec::new(),
            default_state: String::new(),
            states: Vec::new(),
            transitions: Vec::new(),
            any_state_position: None,
            entry_position: None,
        }
    }

    /// Its states as a state machine of their own, with `parameters` (the
    /// file's), so it runs and edits like the base layer.
    pub fn machine(&self, parameters: &[ParameterDef]) -> StateMachineFile {
        StateMachineFile {
            parameters: parameters.to_vec(),
            default_state: self.default_state.clone(),
            states: self.states.clone(),
            transitions: self.transitions.clone(),
            any_state_position: self.any_state_position,
            entry_position: self.entry_position,
            ..StateMachineFile::default()
        }
    }

    /// Takes the states, transitions and node places of `machine` (made by
    /// [`machine`](Self::machine) and edited).
    pub fn set_machine(&mut self, machine: &StateMachineFile) {
        self.default_state.clone_from(&machine.default_state);
        self.states.clone_from(&machine.states);
        self.transitions.clone_from(&machine.transitions);
        self.any_state_position = machine.any_state_position;
        self.entry_position = machine.entry_position;
    }
}

/// How a layer mixes with the layers under it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LayerMode {
    /// Its pose replaces theirs, by its weight, on the bones its clips move.
    #[default]
    Override,
    /// How far its pose turns and moves each bone from the rest pose is
    /// added to theirs, by its weight: an aim offset leaning the spine.
    Additive,
}

/// One parameter, with its value when the game starts.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum ParameterDef {
    /// Persistent: stays true or false until something changes it.
    Bool {
        name: String,
        #[serde(default)]
        default: bool,
    },
    /// A continuous value, such as Speed, for blend trees and conditions.
    Number {
        name: String,
        #[serde(default)]
        default: f64,
    },
    /// A one-time request, used up by the transition it starts.
    Trigger { name: String },
}

impl ParameterDef {
    pub fn name(&self) -> &str {
        match self {
            ParameterDef::Bool { name, .. }
            | ParameterDef::Number { name, .. }
            | ParameterDef::Trigger { name } => name,
        }
    }

    /// Its value when an animator starts.
    pub fn initial(&self) -> axcenity_core::Parameter {
        match self {
            ParameterDef::Bool { default, .. } => axcenity_core::Parameter::Bool(*default),
            ParameterDef::Number { default, .. } => axcenity_core::Parameter::Number(*default),
            ParameterDef::Trigger { .. } => axcenity_core::Parameter::Trigger(false),
        }
    }
}

/// One state of the state machine.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateDef {
    pub name: String,
    pub motion: Motion,
    /// Plays its motion this many times as fast.
    #[serde(default = "one")]
    pub speed: f64,
    /// Where it's drawn in the editor's graph.
    #[serde(default)]
    pub position: [f32; 2],
    /// Goes back to another state when this happens, by a transition made
    /// for it (see [`with_returns`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub returns: Option<ReturnDef>,
}

impl StateDef {
    pub fn new(name: &str, motion: Motion, position: [f32; 2]) -> Self {
        Self {
            name: name.to_owned(),
            motion,
            speed: 1.0,
            position,
            returns: None,
        }
    }
}

/// A state's Return setting: which state to go back to, when, and how
/// smoothly.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReturnDef {
    pub to: String,
    pub when: ReturnWhen,
    #[serde(default = "default_blend")]
    pub blend: f64,
}

/// When a state returns.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ReturnWhen {
    /// Its clip has played once through.
    Finished,
    /// A Bool parameter becomes `value`.
    BoolIs { parameter: String, value: bool },
    /// A Trigger is set (and is used up).
    Triggered { parameter: String },
    /// This many seconds after it starts.
    After { seconds: f64 },
}

/// `file` with its generated transitions made again from its states' Return
/// settings: one per state that returns to a state it has, at the end of
/// the list, in state order. `seconds_per_pass` gives a state's pass length
/// (for After), if known; otherwise a pass counts as a second.
pub fn with_returns(
    file: &StateMachineFile,
    mut seconds_per_pass: impl FnMut(&StateDef) -> Option<f64>,
) -> StateMachineFile {
    let mut out = file.clone();
    out.transitions.retain(|transition| !transition.generated);
    for state in &file.states {
        let Some(returns) = &state.returns else {
            continue;
        };
        if file.state(&returns.to).is_none() {
            continue;
        }
        let mut transition = TransitionDef::new(&state.name, &returns.to);
        transition.blend = returns.blend;
        transition.generated = true;
        match &returns.when {
            ReturnWhen::Finished => transition.exit_time = Some(1.0),
            ReturnWhen::BoolIs { parameter, value } => transition.conditions.push(Condition {
                parameter: parameter.clone(),
                test: if *value { Test::IsTrue } else { Test::IsFalse },
                value: 0.0,
            }),
            ReturnWhen::Triggered { parameter } => transition.conditions.push(Condition {
                parameter: parameter.clone(),
                test: Test::Triggered,
                value: 0.0,
            }),
            ReturnWhen::After { seconds } => {
                let pass = seconds_per_pass(state)
                    .filter(|pass| *pass > 0.0)
                    .unwrap_or(1.0);
                transition.exit_time = Some((seconds.max(0.0) / pass).max(0.0));
            }
        }
        out.transitions.push(transition);
    }
    out
}

/// What a state plays.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Motion {
    /// A clip file's path, such as `assets/animations/hero/Walk.anim.json`.
    Clip(String),
    /// Related clips mixed by a Number parameter, such as Idle, Walk and
    /// Run by Speed.
    BlendTree(BlendTree),
    /// Clips placed at points of two Number parameters, such as eight
    /// strafes by sideways and forward speed (Phase 78).
    BlendSpace(BlendSpace),
}

impl Motion {
    /// Every clip path it plays.
    pub fn clips(&self) -> Vec<&String> {
        match self {
            Motion::Clip(path) => vec![path],
            Motion::BlendTree(tree) => tree.children.iter().map(|child| &child.clip).collect(),
            Motion::BlendSpace(space) => space.children.iter().map(|child| &child.clip).collect(),
        }
    }

    /// Every clip path it plays, to change.
    pub fn clips_mut(&mut self) -> Vec<&mut String> {
        match self {
            Motion::Clip(path) => vec![path],
            Motion::BlendTree(tree) => tree
                .children
                .iter_mut()
                .map(|child| &mut child.clip)
                .collect(),
            Motion::BlendSpace(space) => space
                .children
                .iter_mut()
                .map(|child| &mut child.clip)
                .collect(),
        }
    }
}

/// A 2D blend space: clips at points of two Number parameters, mixed across
/// the triangles between them (`crate::blend_space_weights`). An aim
/// offset is one on an Additive layer, by pitch and yaw.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlendSpace {
    /// The Number parameter along X, such as `MoveX` (sideways speed).
    pub parameter_x: String,
    /// The Number parameter along Y, such as `MoveZ` (forward speed).
    pub parameter_y: String,
    pub children: Vec<BlendSpaceChild>,
}

/// One clip of a blend space, at its point.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlendSpaceChild {
    pub clip: String,
    /// Its place: `[x, y]`, in the two parameters' values.
    pub position: [f64; 2],
}

/// A 1D blend tree: clips at thresholds of a Number parameter. The two
/// either side of the parameter's value are mixed by how far it is
/// between their thresholds; below the first or above the last, that clip
/// plays alone.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlendTree {
    /// A Number parameter's name.
    pub parameter: String,
    pub children: Vec<BlendChild>,
}

/// One clip of a blend tree, at its threshold.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlendChild {
    pub clip: String,
    pub threshold: f64,
}

/// Each child's weight in a 1D blend at `value`, by their `thresholds` (in
/// any order): the two around the value share it by how close it is to
/// each; outside the range, the end one counts alone. The weights add up
/// to 1 (none for no thresholds).
pub fn blend_weights(thresholds: &[f64], value: f64) -> Vec<f64> {
    let mut weights = vec![0.0; thresholds.len()];
    if thresholds.is_empty() {
        return weights;
    }
    let mut order: Vec<usize> = (0..thresholds.len()).collect();
    order.sort_by(|&a, &b| thresholds[a].total_cmp(&thresholds[b]));
    let value = if value.is_finite() { value } else { 0.0 };
    let (first, last) = (order[0], order[order.len() - 1]);
    if value <= thresholds[first] {
        weights[first] = 1.0;
        return weights;
    }
    if value >= thresholds[last] {
        weights[last] = 1.0;
        return weights;
    }
    for pair in order.windows(2) {
        let (low, high) = (pair[0], pair[1]);
        let (a, b) = (thresholds[low], thresholds[high]);
        if value >= a && value <= b {
            let t = if b > a { (value - a) / (b - a) } else { 1.0 };
            weights[low] = 1.0 - t;
            weights[high] = t;
            break;
        }
    }
    weights
}

impl TransitionDef {
    /// A transition from `from` to `to` with no conditions yet, blending
    /// smoothly.
    pub fn new(from: &str, to: &str) -> Self {
        Self {
            from: from.to_owned(),
            to: to.to_owned(),
            conditions: Vec::new(),
            blend: DEFAULT_BLEND,
            exit_time: None,
            interruptible: false,
            priority: 0,
            generated: false,
        }
    }
}

/// A way from one state to another.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransitionDef {
    /// A state's name, or [`ANY_STATE`].
    pub from: String,
    pub to: String,
    /// All must hold for it to happen.
    #[serde(default)]
    pub conditions: Vec<Condition>,
    /// Seconds to blend from the old pose into the new.
    #[serde(default = "default_blend")]
    pub blend: f64,
    /// If set, it can only happen once the state has played this much of
    /// its motion (1 is once through); with no conditions, it happens then.
    #[serde(default)]
    pub exit_time: Option<f64>,
    /// On, another transition can take over while this one blends.
    #[serde(default)]
    pub interruptible: bool,
    /// When several could happen at once, the highest goes.
    #[serde(default)]
    pub priority: i32,
    /// Made by a state's Return setting rather than by hand.
    #[serde(default, skip_serializing_if = "is_false")]
    pub generated: bool,
}

/// The blend a new transition starts with: smooth, a quarter second.
pub const DEFAULT_BLEND: f64 = 0.25;

fn default_blend() -> f64 {
    DEFAULT_BLEND
}

/// Something that must hold for a transition.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Condition {
    pub parameter: String,
    pub test: Test,
    /// What a Number is compared with; unused by the other tests.
    #[serde(default)]
    pub value: f64,
}

/// How a condition tests its parameter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Test {
    /// A Bool is true.
    IsTrue,
    /// A Bool is false.
    IsFalse,
    /// A Trigger is set (and is used up by the transition).
    Triggered,
    /// A Number is more than the value.
    Greater,
    /// A Number is less than the value.
    Less,
}

/// Why an animation file couldn't be read. The messages finish "Could not
/// load `…`: ".
#[derive(Clone, Debug, PartialEq)]
pub struct FileError(pub String);

impl std::fmt::Display for FileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for FileError {}

/// Reads a file's JSON, naming the line for a mistake.
fn parse<T: for<'de> Deserialize<'de>>(text: &str) -> Result<T, FileError> {
    let value: T = serde_json::from_str(text).map_err(|err| {
        FileError(format!(
            "line {}, column {}: {err}",
            err.line(),
            err.column()
        ))
    })?;
    Ok(value)
}

fn check_version(version: u32) -> Result<(), FileError> {
    if version > FORMAT_VERSION {
        return Err(FileError(format!(
            "it was saved by a newer version of Axcenity (format {version}); please update"
        )));
    }
    if version < FORMAT_VERSION {
        return Err(FileError(format!(
            "it is in an older animation format ({version}); open its project in the editor to upgrade it"
        )));
    }
    Ok(())
}

fn to_json<T: Serialize>(value: &T) -> String {
    let mut text = serde_json::to_string_pretty(value).expect("animation files always serialize");
    text.push('\n');
    text
}

/// Writes `text` to `path` through a temporary file beside it, which then
/// replaces it in one step, so a crash never leaves half a file.
pub fn write_file(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut temporary = path.as_os_str().to_owned();
    temporary.push(".saving");
    std::fs::write(&temporary, text)?;
    std::fs::rename(&temporary, path)
}

/// A clip's or state machine's name: its file name without its extension
/// (`Walk` for
/// `assets/animations/hero/Walk.anim.json`).
pub fn clip_name(path: &str) -> &str {
    let file = path.rsplit('/').next().unwrap_or(path);
    [CLIP_EXTENSION, STATE_MACHINE_EXTENSION]
        .into_iter()
        .find_map(|extension| file.strip_suffix(extension))
        .unwrap_or(file)
}

/// A name with only letters, digits, spaces, `-` and `_`, for a file name
/// that works on every OS. Empty becomes `Clip`.
pub fn file_safe(name: &str) -> String {
    let kept: String = name
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_'))
        .collect();
    let kept = kept.trim();
    if kept.is_empty() {
        "Clip".to_owned()
    } else {
        kept.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_controller() -> StateMachineFile {
        StateMachineFile {
            format_version: FORMAT_VERSION,
            parameters: vec![
                ParameterDef::Bool {
                    name: "IsGrounded".into(),
                    default: true,
                },
                ParameterDef::Number {
                    name: "Speed".into(),
                    default: 0.0,
                },
                ParameterDef::Trigger {
                    name: "Jump".into(),
                },
            ],
            default_state: "Idle".into(),
            states: vec![
                StateDef::new(
                    "Idle",
                    Motion::Clip("assets/animations/noodle/Idle.anim.json".into()),
                    [0.0, 0.0],
                ),
                StateDef::new(
                    "Jump",
                    Motion::Clip("assets/animations/noodle/Hop.anim.json".into()),
                    [200.0, 0.0],
                ),
            ],
            transitions: vec![TransitionDef {
                from: ANY_STATE.into(),
                to: "Jump".into(),
                conditions: vec![
                    Condition {
                        parameter: "Jump".into(),
                        test: Test::Triggered,
                        value: 0.0,
                    },
                    Condition {
                        parameter: "Speed".into(),
                        test: Test::Greater,
                        value: 0.5,
                    },
                ],
                blend: 0.1,
                exit_time: Some(0.9),
                interruptible: true,
                priority: 2,
                generated: false,
            }],
            any_state_position: Some([-200.0, 40.0]),
            entry_position: Some([-260.0, -40.0]),
            layers: Vec::new(),
        }
    }

    #[test]
    fn layers_and_blend_spaces_round_trip_and_files_without_them_read_the_same() {
        let plain = sample_controller();
        let text = plain.to_json();
        assert!(!text.contains("layers"), "{text}");
        assert_eq!(StateMachineFile::from_json(&text).unwrap(), plain);

        let mut layered = sample_controller();
        layered.states.push(StateDef::new(
            "Strafe",
            Motion::BlendSpace(BlendSpace {
                parameter_x: "MoveX".into(),
                parameter_y: "MoveZ".into(),
                children: vec![
                    BlendSpaceChild {
                        clip: "assets/animations/hero/Idle.anim.json".into(),
                        position: [0.0, 0.0],
                    },
                    BlendSpaceChild {
                        clip: "assets/animations/hero/Left.anim.json".into(),
                        position: [-1.0, 0.0],
                    },
                ],
            }),
            [0.0, 200.0],
        ));
        let mut upper = LayerDef::new("UpperBody");
        upper.mask = vec!["Hips/Spine".into()];
        upper.weight = 0.5;
        upper.states = vec![StateDef::new(
            "Reload",
            Motion::Clip("assets/animations/hero/Reload.anim.json".into()),
            [0.0, 0.0],
        )];
        let mut aim = LayerDef::new("Aim");
        aim.mode = LayerMode::Additive;
        layered.layers = vec![upper, aim];
        let text = layered.to_json();
        assert!(text.contains("\"blend_space\""), "{text}");
        assert!(text.contains("\"mode\": \"Additive\""), "{text}");
        assert_eq!(StateMachineFile::from_json(&text).unwrap(), layered);
        assert_eq!(StateMachineFile::from_json(&text).unwrap().to_json(), text);
        // A layer's states edit as a state machine of their own.
        let mut machine = layered.layers[0].machine(&layered.parameters);
        assert_eq!(machine.parameters, layered.parameters);
        machine.default_state = "Reload".into();
        layered.layers[0].set_machine(&machine);
        assert_eq!(layered.layer("UpperBody").unwrap().default_state, "Reload");
    }

    #[test]
    fn state_machines_round_trip() {
        let controller = sample_controller();
        let text = controller.to_json();
        let read = StateMachineFile::from_json(&text).unwrap();
        assert_eq!(read, controller);
        assert_eq!(read.to_json(), text);
    }

    #[test]
    fn a_state_machine_without_an_entry_position_reads_and_writes_the_same() {
        let mut older = sample_controller();
        older.entry_position = None;
        let text = older.to_json();
        assert!(!text.contains("entry_position"), "{text}");
        let read = StateMachineFile::from_json(&text).unwrap();
        assert_eq!(read, older);
        assert_eq!(read.to_json(), text);
    }

    #[test]
    fn clips_round_trip() {
        let mut clip = ClipFile::linked("assets/models/noodle.fbx", "Moves", 0.0, 1.0);
        clip.events.push(ClipEvent {
            time: 0.5,
            name: "Wave".into(),
        });
        clip.link.as_mut().unwrap().root_motion = true;
        clip.tracks.push(PropertyTrack {
            id: "sword".into(),
            target: "Sword".into(),
            property: PropertyName::Transparency,
            keys: vec![PropertyKeyframe {
                id: "k".into(),
                time: 0.0,
                value: vec![1.0],
                interp: Interp::Step,
                easing: None,
                bezier: None,
            }],
        });
        let text = clip.to_json();
        assert!(text.contains("\"loop\": true"), "{text}");
        let read = ClipFile::from_json(&text).unwrap();
        assert_eq!(read, clip);
        assert_eq!(read.to_json(), text);
        // A clip with no link leaves it out.
        assert!(!ClipFile::new(1.0).to_json().contains("link"));
    }

    #[test]
    fn format_1_clips_and_state_machines_are_refused_and_say_to_upgrade() {
        // The project upgrade to format 3 rewrote them; nothing reads them
        // here any more (Phase 92).
        let clip = ClipFile::from_json(
            r#"{ "format_version": 1, "source": "a.fbx", "take": "T", "start": 0, "end": 2 }"#,
        )
        .unwrap_err();
        assert!(clip.0.contains("older animation format (1)"), "{clip}");
        assert!(clip.0.contains("upgrade"), "{clip}");
        let machine = StateMachineFile::from_json(r#"{ "format_version": 1 }"#).unwrap_err();
        assert!(machine.0.contains("older animation format"), "{machine}");
    }

    #[test]
    fn mistakes_name_their_line_and_newer_files_are_refused() {
        let err = ClipFile::from_json("{\n \"format_version\": 2,\n \"sauce\": 1 }").unwrap_err();
        assert!(err.0.starts_with("line 3"), "{err}");
        let err = StateMachineFile::from_json(r#"{ "format_version": 99 }"#).unwrap_err();
        assert!(err.0.contains("newer"), "{err}");
        let err = ClipFile::from_json(r#"{ "format_version": 9, "duration": 1 }"#).unwrap_err();
        assert!(err.0.contains("newer"), "{err}");
    }

    #[test]
    fn blend_weights_are_exact_at_and_between_thresholds() {
        let thresholds = [0.0, 2.0, 6.0];
        assert_eq!(blend_weights(&thresholds, 0.0), [1.0, 0.0, 0.0]);
        assert_eq!(blend_weights(&thresholds, 1.0), [0.5, 0.5, 0.0]);
        assert_eq!(blend_weights(&thresholds, 2.0), [0.0, 1.0, 0.0]);
        assert_eq!(blend_weights(&thresholds, 5.0), [0.0, 0.25, 0.75]);
        assert_eq!(blend_weights(&thresholds, 6.0), [0.0, 0.0, 1.0]);
        // Out of range holds the end clip.
        assert_eq!(blend_weights(&thresholds, -3.0), [1.0, 0.0, 0.0]);
        assert_eq!(blend_weights(&thresholds, 9.0), [0.0, 0.0, 1.0]);
        // Any order, and two at the same threshold.
        assert_eq!(blend_weights(&[6.0, 0.0], 3.0), [0.5, 0.5]);
        let same = blend_weights(&[1.0, 1.0, 3.0], 2.0);
        assert!((same.iter().sum::<f64>() - 1.0).abs() < 1e-12);
        assert!(blend_weights(&[], 1.0).is_empty());
        assert_eq!(blend_weights(&[0.0, 2.0], f64::NAN), [1.0, 0.0]);
    }

    #[test]
    fn blend_trees_round_trip() {
        let mut controller = sample_controller();
        controller.states[0].motion = Motion::BlendTree(BlendTree {
            parameter: "Speed".into(),
            children: vec![
                BlendChild {
                    clip: "assets/animations/hero/Idle.anim.json".into(),
                    threshold: 0.0,
                },
                BlendChild {
                    clip: "assets/animations/hero/Run.anim.json".into(),
                    threshold: 6.0,
                },
            ],
        });
        let text = controller.to_json();
        assert!(text.contains("\"blend_tree\""), "{text}");
        assert_eq!(StateMachineFile::from_json(&text).unwrap(), controller);
        assert_eq!(controller.states[0].motion.clips().len(), 2);
    }

    #[test]
    fn return_settings_make_their_transitions() {
        let mut controller = sample_controller();
        controller.states[1].returns = Some(ReturnDef {
            to: "Idle".into(),
            when: ReturnWhen::Finished,
            blend: 0.1,
        });
        let made = with_returns(&controller, |_| None);
        let back = made.transitions.last().unwrap();
        assert!(back.generated);
        assert_eq!((back.from.as_str(), back.to.as_str()), ("Jump", "Idle"));
        assert_eq!(back.exit_time, Some(1.0));
        // Making them again replaces, rather than adds to, the old ones.
        let again = with_returns(&made, |_| None);
        assert_eq!(again, made);
        // A Bool, a Trigger and seconds.
        controller.states[1].returns.as_mut().unwrap().when = ReturnWhen::BoolIs {
            parameter: "IsGrounded".into(),
            value: true,
        };
        let bool_made = with_returns(&controller, |_| None);
        assert_eq!(
            bool_made.transitions.last().unwrap().conditions[0].test,
            Test::IsTrue
        );
        controller.states[1].returns.as_mut().unwrap().when = ReturnWhen::After { seconds: 3.0 };
        let timed = with_returns(&controller, |_| Some(2.0));
        assert_eq!(timed.transitions.last().unwrap().exit_time, Some(1.5));
        // A return to a state that isn't there makes nothing.
        controller.states[1].returns.as_mut().unwrap().to = "Nowhere".into();
        assert_eq!(with_returns(&controller, |_| None).transitions.len(), 1);
        // And it all round-trips.
        let text = timed.to_json();
        assert_eq!(StateMachineFile::from_json(&text).unwrap(), timed);
    }

    #[test]
    fn clip_names_come_from_their_files() {
        assert_eq!(clip_name("assets/animations/hero/Walk.anim.json"), "Walk");
        assert_eq!(file_safe("Run: fast!"), "Run fast");
        assert_eq!(file_safe("??"), "Clip");
    }
}
