//! Axcenity animation: what plays animation on Parts, Models, UI and
//! skinned MeshParts.
//!
//! - [`files`]: the clip (`*.anim.json`) and state machine
//!   (`*.statemachine.json`) files in a project.
//! - [`Clip`]: a clip ready to play: its property tracks, and on a skeleton
//!   its linked take and bone tracks.
//! - [`convert_to_keys`] and the other bone helpers: how a bone track
//!   names its bone and stores its transform, and baking a take into keys.
//! - [`ik`]: turning the bones above a joint so it reaches a point
//!   (Move with IK and pins in the Animation tab), keeping bone lengths.
//! - [`ease`] and [`cubic_bezier`]: how an eased or Bezier key moves to the
//!   next one, shared with tweens.
//! - [`Compiled`] and [`Machine`]: a state machine made ready to run, and
//!   what runs it (which state plays, and blends between them), with its
//!   layers ([`CompiledLayer`]) and 2D blend spaces ([`blend_space_weights`]).
//! - [`Animators`]: runs every Animator, AnimationTrack and played Animation
//!   in a world each frame, posing skeletons ([`Poses`]) and setting keyed
//!   properties; owned by the host like physics.
//!
//! - [`sequence`]: cutscene sequences (`*.sequence.json`) and putting a
//!   world where one has it at a time, shared by the editor's preview and
//!   the Sequence Instance.
//! - `ik_control` and `feet`: an IKControl and a character's FootPlacement
//!   turning bones during the game, after the Animator and before drawing.
//! - [`ragdoll`]: building a Ragdoll's capsules and joints from a skeleton,
//!   and the pose its bodies give the bones while physics moves them.
//!
//! Poses come from `assets` ([`axcenity_assets::Model::pose`]); `scene`
//! draws them.

mod animators;
mod blend_space;
mod bones;
mod cleanup;
mod clip;
mod controller;
mod easing;
mod face;
mod face_play;
mod face_world;
mod feet;
pub mod files;
pub mod ik;
mod ik_control;
mod lip_sync;
mod property_clip;
pub mod ragdoll;
mod retarget;
pub mod sequence;
mod threads;

pub use animators::{
    AnimatorEvent, AnimatorView, Animators, FadeView, FiredView, LayerView, LoadedClip,
    MachineView, PlayingView, Poses, load_clip, read_retarget_profile, read_state_machine,
};
pub use blend_space::{blend_space_weights, triangulate};
pub use bones::{
    bone_from_numbers, bone_numbers, bone_paths, convert_to_keys, convert_to_keys_with, find_bone,
    pose_bones, take_bones,
};
pub use cleanup::{
    Mirrored, Tolerance, fix_loop_seam, mirror, partners, reduce_keys, retime, smooth,
};
pub use clip::{Clip, RootMotion};
pub use controller::{
    Advanced, Compiled, CompiledLayer, Fade, Machine, Parameters, Playing, bone_mask, exit_reached,
    holds,
};
pub use easing::{DEFAULT_BEZIER, Easing, EasingDirection, EasingStyle, cubic_bezier, ease};
pub use face::{
    BUILT_IN_FACE_PROFILES, FaceBoneRole, FaceProblem, FaceRig, GazeAim, built_in_face_profile,
    check_face, choose_face_profile, face_point, pose_face,
};
pub use face_play::{BUILT_IN_EXPRESSIONS, built_in_expression, expression_names, find_expression};
pub use feet::{GroundHit, GroundProbe};
pub use files::{
    ANIMATIONS_FOLDER, ANY_STATE, BlendChild, BlendSpace, BlendSpaceChild, BlendTree,
    CLIP_EXTENSION, ClipEvent, ClipFile, Condition, DEFAULT_BLEND, FileError, Interp, LayerDef,
    LayerMode, Motion, ParameterDef, PropertyKeyframe, PropertyName, PropertyTrack, ReturnDef,
    ReturnWhen, SHARED_FOLDER, STATE_MACHINE_EXTENSION, StateDef, StateMachineFile, TakeLink, Test,
    TransitionDef, blend_weights, with_returns,
};
pub use lip_sync::{
    CompiledVisemes, LipTiming, default_viseme_map, find_viseme_map, lip_sync_from_audio,
    lip_sync_from_phonemes, phoneme_viseme, viseme_weights,
};
pub use property_clip::{
    Applied, EvaluatedProperty, Placement, PropertyClip, apply_evaluated, blend_evaluated,
    current_numbers, model_pivot, numbers_of, resolve_target, sample_time, sample_track,
    target_path, value_from_numbers,
};
pub use retarget::{
    Digit, HipsTravel, HumanBone, HumanoidMap, RETARGET_EXTENSION, ROLE_COUNT, RetargetProfile,
    RigProblem, RigReport, RoleSetting, Segment, Severity, Side, check_pair, check_rig,
    is_retarget_profile, rest_arm_angle,
};
pub use threads::{MAX_THREADS as ANIM_MAX_THREADS, ThreadTimes};

#[cfg(test)]
mod face_tests;
#[cfg(test)]
mod ik_control_tests;
#[cfg(test)]
mod layer_tests;
#[cfg(test)]
mod state_machine_tests;
#[cfg(test)]
mod tests;
