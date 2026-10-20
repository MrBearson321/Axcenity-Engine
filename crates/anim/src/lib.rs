//! Axcenity animation: what plays an animated MeshPart.
//!
//! - [`files`]: the clip (`*.anim.json`) and animator controller
//!   (`*.controller.json`) files in a project.
//! - [`BoundClip`]: a clip ready to pose one model's skeleton.
//! - [`Compiled`] and [`Machine`]: a controller made ready to run, and the
//!   state machine that runs it (which state plays, and blends between them).
//! - [`Animators`]: runs every Animator in a world each frame and poses its
//!   MeshPart's model ([`Poses`]), owned by the host like physics.
//!
//! Poses come from `assets` ([`axcenity_assets::Model::pose`]); `scene`
//! draws them.

mod animators;
mod clip;
mod controller;
pub mod files;

pub use animators::{AnimatorEvent, Animators, Poses, load_clip};
pub use clip::{BoundClip, RootMotion};
pub use controller::{Advanced, Compiled, Fade, Machine, Parameters, Playing, exit_reached, holds};
pub use files::{
    ANIMATIONS_FOLDER, ANY_STATE, BlendChild, BlendTree, CLIP_EXTENSION, CONTROLLER_EXTENSION,
    ClipEvent, ClipFile, Condition, ControllerFile, DEFAULT_BLEND, FileError, Motion, ParameterDef,
    StateDef, Test, TransitionDef, blend_weights,
};

#[cfg(test)]
mod state_machine_tests;
#[cfg(test)]
mod tests;
