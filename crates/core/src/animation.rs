//! What an Animator or property Animation is doing while the game runs.
//!
//! Animator state (parameters, current state, Play requests) and property
//! Animation playback (time, playing / paused, Play/Stop requests) are not
//! saved as Instance properties: scripts read and set them through methods,
//! and the host's animation system reads and updates them every frame. They
//! live in the [`World`](crate::World), beside the Instances.

use std::collections::BTreeMap;

/// One animator parameter's value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Parameter {
    /// Something that stays true or false until it's changed, such as
    /// `IsGrounded`.
    Bool(bool),
    /// A number that changes gradually, such as `Speed`.
    Number(f64),
    /// A one-time request, such as `Jump`: set, it stays set until a
    /// transition uses it (or a script resets it).
    Trigger(bool),
}

impl Parameter {
    /// "Bool", "Number" or "Trigger", for messages.
    pub fn kind(&self) -> &'static str {
        match self {
            Parameter::Bool(_) => "Bool",
            Parameter::Number(_) => "Number",
            Parameter::Trigger(_) => "Trigger",
        }
    }
}

/// A state a script asked an Animator to play, with `animator:Play`.
#[derive(Clone, Debug, PartialEq)]
pub struct PlayRequest {
    pub state: String,
    /// Seconds to blend into it, or `None` for the animator's default.
    pub blend: Option<f64>,
}

/// One Animator's live state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AnimatorState {
    /// Every parameter by name. The host adds the controller's (at their
    /// defaults) the first time it runs the animator; scripts may set them
    /// sooner.
    pub parameters: BTreeMap<String, Parameter>,
    /// The name of the state playing, as the host last set it. Empty until
    /// the animator first runs.
    pub current_state: String,
    /// A state to switch to, which the host takes at its next update.
    pub play: Option<PlayRequest>,
}

/// One property [`Animation`](crate::ClassName::Animation)'s live playback.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PropertyAnimationState {
    /// Seconds into the clip (may grow past duration when looping).
    pub time: f64,
    /// Advancing when true.
    pub playing: bool,
    /// True after Pause until Resume or Play/Stop.
    pub paused: bool,
    /// Script asked to Play from the start.
    pub play: bool,
    /// Script asked to Stop (time back to 0, not playing).
    pub stop: bool,
    /// Script asked to Pause.
    pub pause: bool,
    /// Script asked to Resume from a pause.
    pub resume: bool,
    /// Seek to this time on the next host update.
    pub seek: Option<f64>,
}
