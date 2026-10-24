//! What an Animator, Animation or AnimationTrack is doing while the game
//! runs.
//!
//! Animator state (parameters, current state, Play requests, its tracks),
//! an Animation's `animation:Play()` playback and an AnimationTrack's
//! playback are not saved as Instance properties: scripts read and set them
//! through methods, and the host's animation system reads and updates them
//! every frame. They live in the [`World`](crate::World), beside the
//! Instances.

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
    /// The AnimationTracks loaded on it with `LoadAnimation`, oldest first.
    pub tracks: Vec<crate::InstanceId>,
    /// Counts up each time something starts playing on it, so the most
    /// recently started wins where two animate the same thing.
    pub next_order: u64,
}

impl AnimatorState {
    /// The next start's number (see [`next_order`](Self::next_order)).
    pub fn start_order(&mut self) -> u64 {
        self.next_order += 1;
        self.next_order
    }
}

/// One AnimationTrack's live playback. Scripts change it with the track's
/// methods; the host moves it on each frame.
#[derive(Clone, Debug, PartialEq)]
pub struct AnimationTrackState {
    /// The Animator it was loaded on, and the Animation it plays.
    pub animator: crate::InstanceId,
    pub animation: crate::InstanceId,
    /// True from `Play` until `Stop`, or until a clip that doesn't loop ends.
    pub playing: bool,
    /// Seconds into the clip (`TimePosition`).
    pub time: f64,
    /// How fast it plays (`AdjustSpeed`); negative plays backwards.
    pub speed: f64,
    /// Whether it loops; `None` until the host reads the clip's own.
    pub looped: Option<bool>,
    /// The clip's length in seconds, once the host has read it.
    pub length: f64,
    /// How much it counts now, from 0 to 1, moving to `target_weight` over
    /// `fade` seconds.
    pub weight: f64,
    pub target_weight: f64,
    pub fade: f64,
    /// When it last started (see [`AnimatorState::start_order`]).
    pub order: u64,
    /// True while it fades out after stopping; `Stopped` fires when it's done.
    pub stopping: bool,
}

impl AnimationTrackState {
    pub fn new(animator: crate::InstanceId, animation: crate::InstanceId) -> Self {
        Self {
            animator,
            animation,
            playing: false,
            time: 0.0,
            speed: 1.0,
            looped: None,
            length: 0.0,
            weight: 0.0,
            target_weight: 1.0,
            fade: 0.0,
            order: 0,
            stopping: false,
        }
    }
}

/// One [`Animation`](crate::ClassName::Animation)'s `animation:Play()`
/// playback, through the Animator on its parent.
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
    /// When it last started (see [`AnimatorState::start_order`]).
    pub order: u64,
}
