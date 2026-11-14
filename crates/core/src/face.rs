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
}
