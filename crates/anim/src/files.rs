//! The animation files in a project: clips (`*.anim.json`) and animator
//! controllers (`*.controller.json`), plain JSON like the world file.
//!
//! A **take clip** names a take in a model file and the part of it to play,
//! with how it plays (speed, looping, root motion) and its events. A
//! **property clip** (`"kind": "property"`) stores authored Position /
//! Orientation / Size keyframes for Parts. A controller holds an Animator's
//! parameters, states, default state and transitions (take clips only).
//! Paths in them are from the project folder, with `/` between folders.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// The version these files are written in. Newer ones are refused.
pub const FORMAT_VERSION: u32 = 1;
/// Where the editor keeps clips and controllers.
pub const ANIMATIONS_FOLDER: &str = "assets/animations";
pub const CLIP_EXTENSION: &str = ".anim.json";
pub const CONTROLLER_EXTENSION: &str = ".controller.json";
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

/// A clip file: part of one take of a model file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClipFile {
    pub format_version: u32,
    /// The model file the take is in, such as `assets/models/hero.fbx`.
    pub source: String,
    /// The take's name in that file, such as `mixamo.com` or `Take 001`.
    pub take: String,
    /// Where the clip starts and ends in the take, in seconds.
    pub start: f64,
    pub end: f64,
    /// 2 plays it twice as fast.
    #[serde(default = "one")]
    pub speed: f64,
    /// On, it plays again from the start when it ends; off, it holds its
    /// last frame.
    #[serde(rename = "loop", default = "yes")]
    pub looping: bool,
    /// On, the root bone's travel across the ground moves the MeshPart
    /// instead of the bone.
    #[serde(default)]
    pub root_motion: bool,
    /// Named moments in the clip, which fire the Animator's
    /// `AnimationEvent` as it passes them.
    #[serde(default)]
    pub events: Vec<ClipEvent>,
}

impl ClipFile {
    /// A clip of the whole of `take`, `duration` seconds long, in `source`.
    pub fn new(source: &str, take: &str, start: f64, end: f64) -> Self {
        Self {
            format_version: FORMAT_VERSION,
            source: source.to_owned(),
            take: take.to_owned(),
            start,
            end,
            speed: 1.0,
            looping: true,
            root_motion: false,
            events: Vec::new(),
        }
    }

    /// Its length in seconds of the take (before `speed`), never negative.
    pub fn length(&self) -> f64 {
        (self.end - self.start).max(0.0)
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

/// How a property keyframe reaches the next one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Interp {
    Step,
    #[default]
    Linear,
}

/// Which Part property a property track drives.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PropertyName {
    Position,
    Orientation,
    Size,
}

impl PropertyName {
    pub fn as_str(self) -> &'static str {
        match self {
            PropertyName::Position => "Position",
            PropertyName::Orientation => "Orientation",
            PropertyName::Size => "Size",
        }
    }
}

/// One key on a property track.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PropertyKeyframe {
    pub id: String,
    pub time: f64,
    /// Vector3 as `[x, y, z]` (degrees for Orientation).
    pub value: [f32; 3],
    #[serde(default)]
    pub interp: Interp,
}

/// One animated property on a relative target.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PropertyTrack {
    pub id: String,
    /// Relative to the Animation's parent: `.` is that Part/Model.
    pub target: String,
    pub property: PropertyName,
    pub keys: Vec<PropertyKeyframe>,
}

/// An authored property clip (`"kind": "property"`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PropertyClipFile {
    pub format_version: u32,
    /// Always [`Self::KIND`].
    pub kind: String,
    /// Explicit length in seconds. Keys must lie in `0..=duration`.
    pub duration: f64,
    #[serde(rename = "loop", default = "yes")]
    pub looping: bool,
    #[serde(default)]
    pub tracks: Vec<PropertyTrack>,
    #[serde(default)]
    pub events: Vec<ClipEvent>,
}

impl PropertyClipFile {
    pub const KIND: &'static str = "property";

    pub fn new(duration: f64) -> Self {
        Self {
            format_version: FORMAT_VERSION,
            kind: Self::KIND.to_owned(),
            duration,
            looping: false,
            tracks: Vec::new(),
            events: Vec::new(),
        }
    }

    pub fn from_json(text: &str) -> Result<Self, FileError> {
        let clip: Self = parse(text)?;
        check_version(clip.format_version)?;
        if clip.kind != Self::KIND {
            return Err(FileError(format!(
                "its kind is `{}`, not `{}`",
                clip.kind,
                Self::KIND
            )));
        }
        if !clip.duration.is_finite() {
            return Err(FileError("its duration must be a number".into()));
        }
        Ok(clip)
    }

    pub fn to_json(&self) -> String {
        to_json(self)
    }
}

/// Either kind of `*.anim.json` clip.
#[derive(Clone, Debug, PartialEq)]
pub enum AnimClipFile {
    Take(ClipFile),
    Property(PropertyClipFile),
}

impl AnimClipFile {
    /// Reads a clip file, branching on `"kind": "property"`.
    pub fn from_json(text: &str) -> Result<Self, FileError> {
        let value: serde_json::Value = serde_json::from_str(text).map_err(|err| {
            FileError(format!(
                "line {}, column {}: {err}",
                err.line(),
                err.column()
            ))
        })?;
        match value.get("kind").and_then(|kind| kind.as_str()) {
            Some(PropertyClipFile::KIND) => {
                let clip: PropertyClipFile = serde_json::from_value(value).map_err(|err| {
                    FileError(format!("it isn't a valid property clip: {err}"))
                })?;
                check_version(clip.format_version)?;
                if clip.kind != PropertyClipFile::KIND {
                    return Err(FileError(format!(
                        "its kind is `{}`, not `{}`",
                        clip.kind,
                        PropertyClipFile::KIND
                    )));
                }
                Ok(Self::Property(clip))
            }
            Some(other) => Err(FileError(format!(
                "unknown clip kind `{other}` (expected `{kind}` or a take clip)",
                kind = PropertyClipFile::KIND
            ))),
            None => {
                let clip: ClipFile = serde_json::from_value(value)
                    .map_err(|err| FileError(format!("it isn't a valid take clip: {err}")))?;
                check_version(clip.format_version)?;
                if !(clip.start.is_finite() && clip.end.is_finite() && clip.speed.is_finite()) {
                    return Err(FileError("its start, end and speed must be numbers".into()));
                }
                Ok(Self::Take(clip))
            }
        }
    }
}

/// An animator controller file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControllerFile {
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
}

impl Default for ControllerFile {
    fn default() -> Self {
        Self {
            format_version: FORMAT_VERSION,
            parameters: Vec::new(),
            default_state: String::new(),
            states: Vec::new(),
            transitions: Vec::new(),
            any_state_position: None,
        }
    }
}

impl ControllerFile {
    /// The state named `name`.
    pub fn state(&self, name: &str) -> Option<&StateDef> {
        self.states.iter().find(|state| state.name == name)
    }

    /// The state it starts in: `default_state`, or the first if that's empty
    /// or isn't one of its states.
    pub fn starting_state(&self) -> Option<&StateDef> {
        self.state(&self.default_state).or(self.states.first())
    }
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
    file: &ControllerFile,
    mut seconds_per_pass: impl FnMut(&StateDef) -> Option<f64>,
) -> ControllerFile {
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
}

impl Motion {
    /// Every clip path it plays.
    pub fn clips(&self) -> Vec<&String> {
        match self {
            Motion::Clip(path) => vec![path],
            Motion::BlendTree(tree) => tree.children.iter().map(|child| &child.clip).collect(),
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
        }
    }
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
    Ok(())
}

impl ClipFile {
    pub fn from_json(text: &str) -> Result<Self, FileError> {
        let clip: Self = parse(text)?;
        check_version(clip.format_version)?;
        if !(clip.start.is_finite() && clip.end.is_finite() && clip.speed.is_finite()) {
            return Err(FileError("its start, end and speed must be numbers".into()));
        }
        Ok(clip)
    }

    /// Readable JSON, the same for the same clip.
    pub fn to_json(&self) -> String {
        to_json(self)
    }
}

impl ControllerFile {
    pub fn from_json(text: &str) -> Result<Self, FileError> {
        let controller: Self = parse(text)?;
        check_version(controller.format_version)?;
        Ok(controller)
    }

    /// Readable JSON, the same for the same controller.
    pub fn to_json(&self) -> String {
        to_json(self)
    }
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

/// A clip's name: its file name without `.anim.json` (`Walk` for
/// `assets/animations/hero/Walk.anim.json`).
pub fn clip_name(path: &str) -> &str {
    let file = path.rsplit('/').next().unwrap_or(path);
    file.strip_suffix(CLIP_EXTENSION).unwrap_or(file)
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

    fn sample_controller() -> ControllerFile {
        ControllerFile {
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
        }
    }

    #[test]
    fn controllers_round_trip() {
        let controller = sample_controller();
        let text = controller.to_json();
        let read = ControllerFile::from_json(&text).unwrap();
        assert_eq!(read, controller);
        assert_eq!(read.to_json(), text);
    }

    #[test]
    fn clips_round_trip() {
        let mut clip = ClipFile::new("assets/models/noodle.fbx", "Moves", 0.0, 1.0);
        clip.events.push(ClipEvent {
            time: 0.5,
            name: "Wave".into(),
        });
        clip.root_motion = true;
        let text = clip.to_json();
        assert!(text.contains("\"loop\": true"), "{text}");
        let read = ClipFile::from_json(&text).unwrap();
        assert_eq!(read, clip);
        assert_eq!(read.to_json(), text);
    }

    #[test]
    fn left_out_fields_take_their_defaults() {
        let clip = ClipFile::from_json(
            r#"{ "format_version": 1, "source": "a.fbx", "take": "T", "start": 0, "end": 2 }"#,
        )
        .unwrap();
        assert_eq!(clip.speed, 1.0);
        assert!(clip.looping);
        assert!(!clip.root_motion);
        let controller = ControllerFile::from_json(r#"{ "format_version": 1 }"#).unwrap();
        assert!(controller.states.is_empty());
    }

    #[test]
    fn mistakes_name_their_line_and_newer_files_are_refused() {
        let err = ClipFile::from_json("{\n \"format_version\": 1,\n \"sauce\": 1 }").unwrap_err();
        assert!(err.0.starts_with("line 3"), "{err}");
        let err = ControllerFile::from_json(r#"{ "format_version": 99 }"#).unwrap_err();
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
        assert_eq!(ControllerFile::from_json(&text).unwrap(), controller);
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
        assert_eq!(ControllerFile::from_json(&text).unwrap(), timed);
    }

    #[test]
    fn clip_names_come_from_their_files() {
        assert_eq!(clip_name("assets/animations/hero/Walk.anim.json"), "Walk");
        assert_eq!(file_safe("Run: fast!"), "Run fast");
        assert_eq!(file_safe("??"), "Clip");
    }
}
