//! The standard facial controls (the Axcenity Humanoid Facial Profile):
//! the names a `FaceControls` Instance has a number property for, and the
//! names blend shapes have on Axcenity's own characters. Each is 0 (at
//! rest) to 1 (all the way). `anim` turns them into blend shape weights
//! and eye and jaw turns through a face profile (`docs/rendering/animation.md`).

/// One standard control.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FaceControl {
    /// Its name, which is also its FaceControls property's.
    pub name: &'static str,
    /// The part of the face it moves, for grouping in the editor.
    pub group: FaceGroup,
    /// What it does, for the Properties panel and the API reference.
    pub doc: &'static str,
}

/// The parts of the face the controls are grouped by.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FaceGroup {
    Eyes,
    Eyebrows,
    Mouth,
    CheeksAndNose,
    JawAndTongue,
}

impl FaceGroup {
    pub fn label(self) -> &'static str {
        match self {
            FaceGroup::Eyes => "Eyes",
            FaceGroup::Eyebrows => "Eyebrows",
            FaceGroup::Mouth => "Mouth and lips",
            FaceGroup::CheeksAndNose => "Cheeks and nose",
            FaceGroup::JawAndTongue => "Jaw and tongue",
        }
    }
}

const fn control(name: &'static str, group: FaceGroup, doc: &'static str) -> FaceControl {
    FaceControl { name, group, doc }
}

use FaceGroup::{CheeksAndNose, Eyebrows, Eyes, JawAndTongue, Mouth};

/// Every standard control, grouped, in the order the editor lists them.
/// Left and right are the character's own.
pub const FACE_CONTROLS: &[FaceControl] = &[
    control(
        "Blink_Left",
        Eyes,
        "Closes the left eye in a blink, 0 to 1.",
    ),
    control(
        "Blink_Right",
        Eyes,
        "Closes the right eye in a blink, 0 to 1.",
    ),
    control(
        "Eye_Closed_Left",
        Eyes,
        "Holds the left eye closed, 0 to 1, as in sleep or a wink, apart from blinking.",
    ),
    control(
        "Eye_Closed_Right",
        Eyes,
        "Holds the right eye closed, 0 to 1, as in sleep or a wink, apart from blinking.",
    ),
    control("Eye_Squint_Left", Eyes, "Narrows the left eye, 0 to 1."),
    control("Eye_Squint_Right", Eyes, "Narrows the right eye, 0 to 1."),
    control("Eye_Wide_Left", Eyes, "Opens the left eye wide, 0 to 1."),
    control("Eye_Wide_Right", Eyes, "Opens the right eye wide, 0 to 1."),
    control(
        "Eye_Look_Up",
        Eyes,
        "Turns both eyes up, 0 to 1 (1 is the face profile's up limit, 25° by default). Turns the eye bones when the model has them, otherwise its look shapes.",
    ),
    control(
        "Eye_Look_Down",
        Eyes,
        "Turns both eyes down, 0 to 1 (1 is the face profile's down limit, 20° by default).",
    ),
    control(
        "Eye_Look_Left",
        Eyes,
        "Turns both eyes to the character's left, 0 to 1 (1 is the face profile's sideways limit, 30° by default).",
    ),
    control(
        "Eye_Look_Right",
        Eyes,
        "Turns both eyes to the character's right, 0 to 1 (1 is the face profile's sideways limit, 30° by default).",
    ),
    control(
        "Brow_Raise_Left",
        Eyebrows,
        "Raises the left eyebrow, 0 to 1.",
    ),
    control(
        "Brow_Raise_Right",
        Eyebrows,
        "Raises the right eyebrow, 0 to 1.",
    ),
    control(
        "Brow_Lower_Left",
        Eyebrows,
        "Lowers the left eyebrow, 0 to 1.",
    ),
    control(
        "Brow_Lower_Right",
        Eyebrows,
        "Lowers the right eyebrow, 0 to 1.",
    ),
    control(
        "Brow_Inward_Left",
        Eyebrows,
        "Draws the left eyebrow's inner end in and up, 0 to 1, as in worry.",
    ),
    control(
        "Brow_Inward_Right",
        Eyebrows,
        "Draws the right eyebrow's inner end in and up, 0 to 1, as in worry.",
    ),
    control(
        "Mouth_Smile_Left",
        Mouth,
        "Pulls the left corner of the mouth up into a smile, 0 to 1.",
    ),
    control(
        "Mouth_Smile_Right",
        Mouth,
        "Pulls the right corner of the mouth up into a smile, 0 to 1.",
    ),
    control(
        "Mouth_Frown_Left",
        Mouth,
        "Pulls the left corner of the mouth down, 0 to 1.",
    ),
    control(
        "Mouth_Frown_Right",
        Mouth,
        "Pulls the right corner of the mouth down, 0 to 1.",
    ),
    control(
        "Mouth_Open",
        Mouth,
        "Parts the lips, 0 to 1, without the jaw (Jaw_Open drops the jaw).",
    ),
    control("Mouth_Wide", Mouth, "Stretches the mouth wide, 0 to 1."),
    control(
        "Mouth_Narrow",
        Mouth,
        "Narrows the mouth into a round opening, 0 to 1, as for \"oo\".",
    ),
    control(
        "Mouth_Pucker",
        Mouth,
        "Pushes the lips forward together, 0 to 1, as for a kiss.",
    ),
    control(
        "Mouth_Left",
        Mouth,
        "Moves the whole mouth to the character's left, 0 to 1.",
    ),
    control(
        "Mouth_Right",
        Mouth,
        "Moves the whole mouth to the character's right, 0 to 1.",
    ),
    control("Upper_Lip_Raise", Mouth, "Raises the upper lip, 0 to 1."),
    control("Lower_Lip_Lower", Mouth, "Lowers the lower lip, 0 to 1."),
    control(
        "Upper_Lip_Raise_Left",
        Mouth,
        "Raises the left side of the upper lip, 0 to 1.",
    ),
    control(
        "Upper_Lip_Raise_Right",
        Mouth,
        "Raises the right side of the upper lip, 0 to 1.",
    ),
    control(
        "Lower_Lip_Lower_Left",
        Mouth,
        "Lowers the left side of the lower lip, 0 to 1.",
    ),
    control(
        "Lower_Lip_Lower_Right",
        Mouth,
        "Lowers the right side of the lower lip, 0 to 1.",
    ),
    control(
        "Lip_Corner_Pull_Left",
        Mouth,
        "Pulls the left corner of the lips back, 0 to 1.",
    ),
    control(
        "Lip_Corner_Pull_Right",
        Mouth,
        "Pulls the right corner of the lips back, 0 to 1.",
    ),
    control(
        "Cheek_Raise_Left",
        CheeksAndNose,
        "Raises the left cheek, 0 to 1, as in a broad smile.",
    ),
    control(
        "Cheek_Raise_Right",
        CheeksAndNose,
        "Raises the right cheek, 0 to 1, as in a broad smile.",
    ),
    control(
        "Cheek_Puff",
        CheeksAndNose,
        "Puffs both cheeks out, 0 to 1.",
    ),
    control(
        "Nose_Sneer_Left",
        CheeksAndNose,
        "Wrinkles the left side of the nose up, 0 to 1.",
    ),
    control(
        "Nose_Sneer_Right",
        CheeksAndNose,
        "Wrinkles the right side of the nose up, 0 to 1.",
    ),
    control(
        "Jaw_Open",
        JawAndTongue,
        "Drops the jaw, 0 to 1 (1 is the face profile's jaw limit, 25° by default). Turns the jaw bone when the model has one, and drives a Jaw_Open shape too when it has one.",
    ),
    control(
        "Jaw_Forward",
        JawAndTongue,
        "Pushes the jaw forward, 0 to 1.",
    ),
    control(
        "Jaw_Left",
        JawAndTongue,
        "Moves the jaw to the character's left, 0 to 1. Turns the jaw bone when the model has one.",
    ),
    control(
        "Jaw_Right",
        JawAndTongue,
        "Moves the jaw to the character's right, 0 to 1. Turns the jaw bone when the model has one.",
    ),
    control("Tongue_Out", JawAndTongue, "Sticks the tongue out, 0 to 1."),
    control(
        "Tongue_Up",
        JawAndTongue,
        "Curls the tongue's tip up, 0 to 1.",
    ),
    control(
        "Tongue_Down",
        JawAndTongue,
        "Curls the tongue's tip down, 0 to 1.",
    ),
];

/// The index of the control named `name` in [`FACE_CONTROLS`].
pub fn face_control(name: &str) -> Option<usize> {
    FACE_CONTROLS
        .iter()
        .position(|control| control.name == name)
}

/// The layers a face's controls are blended through, lowest first by
/// default: the FaceControls' own properties, expressions, dialogue (lip
/// sync), procedural motion (blinks, gaze, idle), and gameplay overrides.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FaceLayer {
    Base,
    Expression,
    Dialogue,
    Procedural,
    Override,
}

impl FaceLayer {
    pub const ALL: [FaceLayer; 5] = [
        FaceLayer::Base,
        FaceLayer::Expression,
        FaceLayer::Dialogue,
        FaceLayer::Procedural,
        FaceLayer::Override,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            FaceLayer::Base => "Base",
            FaceLayer::Expression => "Expression",
            FaceLayer::Dialogue => "Dialogue",
            FaceLayer::Procedural => "Procedural",
            FaceLayer::Override => "Override",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|layer| layer.as_str().eq_ignore_ascii_case(name.trim()))
    }

    /// A FaceControls' `LayerOrder` read: each layer named once, in its
    /// order; layers left out keep their default place after the named
    /// ones. `None` names an unknown layer.
    pub fn order(text: &str) -> Result<Vec<FaceLayer>, String> {
        let mut order = Vec::new();
        for name in text
            .split(',')
            .map(str::trim)
            .filter(|name| !name.is_empty())
        {
            let Some(layer) = FaceLayer::from_name(name) else {
                return Err(format!(
                    "`{name}` isn't a face layer (Base, Expression, Dialogue, Procedural or Override)"
                ));
            };
            if !order.contains(&layer) {
                order.push(layer);
            }
        }
        for layer in FaceLayer::ALL {
            if !order.contains(&layer) {
                order.push(layer);
            }
        }
        Ok(order)
    }
}

/// How a layer goes over the ones under it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FaceBlend {
    /// Its values replace those under it, by its weight.
    #[default]
    Override,
    /// Its values are added to those under it, by its weight.
    Additive,
}

/// A layer's settings, which `FaceControls:SetLayer` changes.
#[derive(Clone, Debug, PartialEq)]
pub struct FaceLayerSetting {
    /// How much it counts, 0 to 1.
    pub weight: f64,
    pub blend: FaceBlend,
    /// The controls it may move (by index in [`FACE_CONTROLS`]), or `None`
    /// for all.
    pub mask: Option<Vec<usize>>,
}

impl Default for FaceLayerSetting {
    fn default() -> Self {
        Self {
            weight: 1.0,
            blend: FaceBlend::Override,
            mask: None,
        }
    }
}

/// An expression a script set with `FaceControls:SetExpression`.
#[derive(Clone, Debug, PartialEq)]
pub struct ExpressionPlay {
    /// The expression's name (`Happy`) or file path.
    pub name: String,
    /// How strongly it's shown now, and where that's heading.
    pub intensity: f64,
    pub target_intensity: f64,
    /// How far it has faded in, 0 to 1, and where that's heading (0 once
    /// it's cleared, when it's then dropped).
    pub presence: f64,
    pub target_presence: f64,
    /// Seconds a change takes; `None`: the expression file's own.
    pub transition: Option<f64>,
    /// How much it counts against those under it, 0 to 1.
    pub weight: f64,
    /// Higher goes over lower; equal ones go in the order they were set.
    pub priority: i32,
    /// -1 (only the left side) to 1 (only the right side); 0 is both.
    pub asymmetry: f64,
    /// When it was set, among this face's expressions.
    pub order: u64,
}

/// A FaceControls' live state while the game runs: what scripts set
/// through its methods, which the host's animation reads every frame. Not
/// saved, like an Animator's.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FaceState {
    /// Expressions playing or fading out, in the order they were set.
    pub expressions: Vec<ExpressionPlay>,
    /// Gameplay overrides, by control index: the value it's held at.
    pub overrides: std::collections::BTreeMap<usize, f64>,
    /// Layer settings changed from the defaults.
    pub layers: std::collections::BTreeMap<FaceLayer, FaceLayerSetting>,
    pub next_order: u64,
    /// The lip sync `PlayLipSync` started, while it plays or fades out.
    pub lip_sync: Option<LipSyncPlay>,
}

/// A lip sync a script played on a face (facial animation F5).
#[derive(Clone, Debug, PartialEq)]
pub struct LipSyncPlay {
    /// The `*.lipsync.json` file.
    pub path: String,
    /// Seconds into it.
    pub time: f64,
    /// How fast it goes; with a Sound, times the Sound's PlaybackSpeed.
    pub speed: f64,
    /// The Sound it keeps time with: it moves on only while the Sound
    /// plays, and stops when the Sound has played and stopped.
    pub sound: Option<crate::InstanceId>,
    /// The Sound has been seen playing.
    pub heard: bool,
    /// How much it counts now (0 to 1) and where that's heading (0 once
    /// stopped, when it's then dropped), and the seconds that takes.
    pub weight: f64,
    pub target_weight: f64,
    pub fade: f64,
}

/// The 12 visemes (mouth shapes for speech) a lip sync is made of, in
/// order: what each sounds like is in its doc.
pub const VISEMES: [Viseme; 12] = [
    Viseme::new("Silence", "The mouth at rest between words."),
    Viseme::new("AI", "Open vowels: the a in father, the i in bite."),
    Viseme::new("E", "The e in bed and the a in cat: open and wide."),
    Viseme::new("O", "The o in go: open and round."),
    Viseme::new("U", "The oo in boot: small and round."),
    Viseme::new("FV", "F and V: the lower lip under the top teeth."),
    Viseme::new(
        "L",
        "L: the tongue up behind the teeth, mouth a little open.",
    ),
    Viseme::new("MBP", "M, B and P: the lips pressed together."),
    Viseme::new("WQ", "W and Q: pursed lips."),
    Viseme::new("TH", "Th: the tongue between the teeth."),
    Viseme::new("TDN", "T, D and N (and S, Z): the teeth almost closed."),
    Viseme::new("KG", "K and G (and the ng in sing): open, the tongue back."),
];

/// One viseme.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Viseme {
    pub name: &'static str,
    pub doc: &'static str,
}

impl Viseme {
    const fn new(name: &'static str, doc: &'static str) -> Self {
        Self { name, doc }
    }
}

/// The index in [`VISEMES`] of the viseme `name`, any case, ignoring `/`,
/// `_` and spaces (so `"A/I"` and `"m_b_p"` work).
pub fn viseme(name: &str) -> Option<usize> {
    let plain: String = name
        .chars()
        .filter(|c| !matches!(c, '/' | '_' | ' ' | '-'))
        .collect();
    VISEMES
        .iter()
        .position(|viseme| viseme.name.eq_ignore_ascii_case(&plain))
}

impl FaceState {
    /// A layer's settings (the defaults when unchanged).
    pub fn layer(&self, layer: FaceLayer) -> FaceLayerSetting {
        self.layers.get(&layer).cloned().unwrap_or_default()
    }
}

/// The controls a mask name stands for: a group's ("Eyes", "Eyebrows",
/// "Mouth", "CheeksAndNose", "JawAndTongue") or one control's.
pub fn face_mask_controls(name: &str) -> Option<Vec<usize>> {
    let group = [
        FaceGroup::Eyes,
        FaceGroup::Eyebrows,
        FaceGroup::Mouth,
        FaceGroup::CheeksAndNose,
        FaceGroup::JawAndTongue,
    ]
    .into_iter()
    .find(|group| format!("{group:?}").eq_ignore_ascii_case(name.trim()));
    match group {
        Some(group) => Some(
            FACE_CONTROLS
                .iter()
                .enumerate()
                .filter(|(_, control)| control.group == group)
                .map(|(index, _)| index)
                .collect(),
        ),
        None => face_control(name.trim()).map(|index| vec![index]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_control_has_one_name_and_a_doc() {
        let mut names = std::collections::HashSet::new();
        for control in FACE_CONTROLS {
            assert!(names.insert(control.name), "{} twice", control.name);
            assert!(!control.doc.is_empty());
            assert_eq!(
                face_control(control.name).map(|i| FACE_CONTROLS[i]),
                Some(*control)
            );
        }
        assert_eq!(FACE_CONTROLS.len(), 48);
        assert_eq!(face_control("Blink"), None);
    }

    #[test]
    fn visemes_are_found_by_their_written_names() {
        assert_eq!(viseme("A/I"), Some(1));
        assert_eq!(viseme("m_b_p"), Some(7));
        assert_eq!(viseme("silence"), Some(0));
        assert_eq!(viseme("Q"), None);
    }

    #[test]
    fn layer_orders_and_masks_read_names() {
        assert_eq!(
            FaceLayer::order("override, Base").unwrap(),
            [
                FaceLayer::Override,
                FaceLayer::Base,
                FaceLayer::Expression,
                FaceLayer::Dialogue,
                FaceLayer::Procedural
            ]
        );
        assert_eq!(FaceLayer::order("").unwrap(), FaceLayer::ALL);
        assert!(FaceLayer::order("Base, Nose").is_err());
        assert_eq!(face_mask_controls("eyebrows").unwrap().len(), 6);
        assert_eq!(
            face_mask_controls("Jaw_Open"),
            face_control("Jaw_Open").map(|i| vec![i])
        );
        assert_eq!(face_mask_controls("Nose"), None);
    }
}
