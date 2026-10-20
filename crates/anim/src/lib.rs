//! Axcenity animation: what plays an animated MeshPart or property clip.
//!
//! - [`files`]: the clip (`*.anim.json`) and animator controller
//!   (`*.controller.json`) files in a project (take clips and property clips).
//! - [`BoundClip`]: a take clip ready to pose one model's skeleton.
//! - [`PropertyClip`]: an authored Position/Orientation/Size clip.
//! - [`Compiled`] and [`Machine`]: a controller made ready to run, and the
//!   state machine that runs it (which state plays, and blends between them).
//! - [`Animators`]: runs every Animator in a world each frame and poses its
//!   MeshPart's model ([`Poses`]), owned by the host like physics.
//! - [`PropertyPlayers`]: runs every property Animation and applies tracks.
//!
//! Poses come from `assets` ([`axcenity_assets::Model::pose`]); `scene`
//! draws them.

mod animators;
mod clip;
mod controller;
pub mod files;
mod property_clip;
mod property_players;

pub use animators::{AnimatorEvent, Animators, Poses, load_clip};
pub use clip::{BoundClip, RootMotion};
pub use controller::{Advanced, Compiled, Fade, Machine, Parameters, Playing, exit_reached, holds};
pub use files::{
    ANIMATIONS_FOLDER, ANY_STATE, AnimClipFile, BlendChild, BlendTree, CLIP_EXTENSION,
    CONTROLLER_EXTENSION, ClipEvent, ClipFile, Condition, ControllerFile, DEFAULT_BLEND, FileError,
    Interp, Motion, ParameterDef, PropertyClipFile, PropertyKeyframe, PropertyName, PropertyTrack,
    ReturnDef, ReturnWhen, StateDef, Test, TransitionDef, blend_weights, with_returns,
};
pub use property_clip::{
    EvaluatedProperty, PropertyClip, apply_evaluated, resolve_target, sample_time,
};
pub use property_players::{
    PropertyAnimationEvent, PropertyPlayers, load_property_clip, parse_property_clip,
};

#[cfg(test)]
mod state_machine_tests;
#[cfg(test)]
mod tests;
