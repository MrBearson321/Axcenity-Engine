//! Axcenity animation: what plays animation on Parts, Models, UI and
//! skinned MeshParts.
//!
//! - [`files`]: the clip (`*.anim.json`) and state machine
//!   (`*.statemachine.json`) files in a project.
//! - [`Clip`]: a clip ready to play: its property tracks, and on a skeleton
//!   its linked take and bone tracks.
//! - [`Compiled`] and [`Machine`]: a state machine made ready to run, and
//!   what runs it (which state plays, and blends between them).
//! - [`Animators`]: runs every Animator, AnimationTrack and played Animation
//!   in a world each frame, posing skeletons ([`Poses`]) and setting keyed
//!   properties; owned by the host like physics.
//!
//! Poses come from `assets` ([`axcenity_assets::Model::pose`]); `scene`
//! draws them.

mod animators;
mod clip;
mod controller;
pub mod files;
mod property_clip;

pub use animators::{AnimatorEvent, Animators, Poses, load_clip};
pub use clip::{Clip, RootMotion};
pub use controller::{Advanced, Compiled, Fade, Machine, Parameters, Playing, exit_reached, holds};
pub use files::{
    ANIMATIONS_FOLDER, ANY_STATE, BlendChild, BlendTree, CLIP_EXTENSION, ClipEvent, ClipFile,
    Condition, DEFAULT_BLEND, FileError, Interp, LEGACY_CONTROLLER_EXTENSION, Motion, ParameterDef,
    PropertyKeyframe, PropertyName, PropertyTrack, ReturnDef, ReturnWhen, SHARED_FOLDER,
    STATE_MACHINE_EXTENSION, StateDef, StateMachineFile, TakeLink, Test, TransitionDef,
    blend_weights, with_returns,
};
pub use property_clip::{
    Applied, EvaluatedProperty, Placement, PropertyClip, apply_evaluated, blend_evaluated,
    current_numbers, model_pivot, numbers_of, resolve_target, sample_time, target_path,
    value_from_numbers,
};

#[cfg(test)]
mod state_machine_tests;
#[cfg(test)]
mod tests;
