//! What an Animator is doing while the game runs: its parameters, the
//! state it's in, and a state a script asked it to play.
//!
//! This isn't saved and isn't made of properties: scripts read and set it
//! through the Animator's methods (`animator:SetBool`, `CurrentState`...),
//! and the host's animation system (the `anim` crate) reads and updates it
//! every frame. It lives in the [`World`](crate::World), beside the
//! Instances, so both sides see the same values at once.

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
