//! Running every Animator in a world, owned by the host (the player, or
//! the editor's Play session) like its physics.
//!
//! Each animated thing (the parent of an Animator, or of Animations) has
//! one runner. Each frame it plays, in this order, so later ones win where
//! two animate the same thing:
//! 1. the Animator's state machine, if it has one, and over its base layer
//!    each of its layers, by the layer's weight and mask;
//! 2. what plays on top, oldest start first: AnimationTracks from
//!    `animator:LoadAnimation`, by their weight (fading in and out), and
//!    Animations played with `AutoPlay`.
//!
//! On a MeshPart with a skeleton they pose its bones; on anything they set
//! the properties their tracks key. A Ragdoll in the MeshPart overrides the
//! pose while it's on, and blends back to it over `BlendTime` once it's
//! off (see [`crate::ragdoll`]). Properties keep the value last given
//! when nothing plays them any more; bones go back to what plays under them.
//!
//! Over a skeleton's pose (not while its Ragdoll is on) come its character
//! controller's foot placement ([`crate::feet`]) and then its IKControls
//! ([`crate::ik_control`]), all before drawing. Last, a FaceControls in
//! the MeshPart turns its eye and jaw bones and sets its blend shapes
//! ([`crate::face`]); a model with blend shapes and no skeleton is posed
//! for its face alone.
//!
//! A clip, state machine or model file that changed on disk
//! ([`Animators::reload`]) is bound again at the next update, and each
//! Animator using it stays in the state it was in, matched by name.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use axcenity_assets::{
    Assets, FaceProfiles, Model, ModelFormat, Posed, Rig, RigDefinitions, Slot, Take,
};
use axcenity_core::{
    ClassName, InstanceId, LayerWeight, Parameter, PlayRequest, Value, Vector3, World,
};
use glam::{EulerRot, Quat, Vec3};

use crate::clip::{Clip, RootMotion};
use crate::controller::{Compiled, Machine, Playing};
use crate::face::{FaceRig, choose_face_profile};
use crate::face_play::{FaceDetail, FaceLayers, FaceLod, FaceMotion};
use crate::face_world::{Lookers, face_layers};
use crate::feet::{self, GroundProbe, Legs};
use crate::files::{
    CLIP_EXTENSION, ClipFile, DEFAULT_BLEND, STATE_MACHINE_EXTENSION, StateMachineFile,
};
use crate::ik_control;
use crate::property_clip::{Applied, apply_evaluated, blend_evaluated};
use crate::ragdoll;
use crate::retarget::RetargetProfile;
use crate::threads::{ThreadTimes, Threads};

/// Each animated MeshPart's posed model, by the MeshPart. The draw list
/// uses these in place of the model's still triangles.
pub type Poses = HashMap<InstanceId, Arc<Posed>>;

/// Something scripts hear about, which the host fires after updating.
#[derive(Clone, Debug, PartialEq)]
pub enum AnimatorEvent {
    /// The Animator's `StateChanged`: the new state's name and the old one's.
    StateChanged {
        animator: InstanceId,
        state: String,
        previous: String,
    },
    /// The Animator's `AnimationEvent`: its state machine's clip passed this
    /// event.
    Marker { animator: InstanceId, name: String },
    /// An Animation's `AnimationEvent`, from AutoPlay.
    AnimationMarker { animation: InstanceId, name: String },
    /// An Animation's `Completed`: AutoPlay reached the end of a
    /// clip it doesn't loop.
    AnimationCompleted { animation: InstanceId },
    /// An AnimationTrack's `KeyframeReached` (and its
    /// `GetMarkerReachedSignal(name)`): it passed this event.
    TrackMarker { track: InstanceId, name: String },
    /// An AnimationTrack's `Stopped`: it stopped and has faded out.
    TrackStopped { track: InstanceId },
}

impl AnimatorEvent {
    /// The Instance, event name and handler arguments to fire it with
    /// (`ScriptRuntime::fire_instance_event`).
    pub fn script_event(&self) -> (InstanceId, &'static str, Vec<String>) {
        match self {
            AnimatorEvent::StateChanged {
                animator,
                state,
                previous,
            } => (
                *animator,
                "StateChanged",
                vec![state.clone(), previous.clone()],
            ),
            AnimatorEvent::Marker { animator, name } => {
                (*animator, "AnimationEvent", vec![name.clone()])
            }
            AnimatorEvent::AnimationMarker { animation, name } => {
                (*animation, "AnimationEvent", vec![name.clone()])
            }
            AnimatorEvent::AnimationCompleted { animation } => {
                (*animation, "Completed", Vec::new())
            }
            AnimatorEvent::TrackMarker { track, name } => {
                (*track, "KeyframeReached", vec![name.clone()])
            }
            AnimatorEvent::TrackStopped { track } => (*track, "Stopped", Vec::new()),
        }
    }
}

/// How a thing's clips move this frame. A UI element's are `Reduced`
/// while the game UI setting reduced motion is on: a clip that doesn't
/// loop jumps to its end (firing its events, then Completed or Stopped)
/// and one that loops holds where it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Motion {
    Full,
    Reduced,
}

/// Every animated thing's runner, and the poses they make.
#[derive(Default)]
pub struct Animators {
    running: HashMap<InstanceId, Running>,
    poses: Poses,
    errors: Vec<String>,
    /// Errors already given, so each is reported once.
    reported: HashSet<String>,
    /// Files changed on disk since the last update ([`Animators::reload`]).
    changed: HashSet<String>,
    /// How foot placement finds the ground. Without one, feet stay where
    /// the animation has them.
    ground: Option<GroundProbe>,
    /// Where poses are worked out (unified Phase 93).
    threads: Threads,
    /// How long each thread spent posing in the last update.
    thread_times: ThreadTimes,
    /// Where the camera is, which faces with GazeMode Camera look at and
    /// facial LOD measures from, and which way it looks (a unit vector).
    viewer: Option<Vec3>,
    look: Option<Vec3>,
    /// What faces cost in the last update (F7).
    face_stats: FaceStats,
    /// The resting head of each skinned model a face has looked at, in
    /// its file's space, by model key.
    heads: HashMap<u64, Option<Vec3>>,
}

/// A pose to work out off the calling thread: everything it needs from
/// the World, read beforehand. Its state machine is the owner's in
/// `Animators::running`, which nothing changes while poses are made.
struct PoseJob {
    owner: InstanceId,
    model: Arc<Model>,
    rig: Arc<Rig>,
    parameters: BTreeMap<String, Parameter>,
    /// Each state machine layer's weight, in the file's order.
    layer_weights: Vec<f32>,
    /// What plays over the state machine and moves bones, in order.
    bone_layers: Vec<Layer>,
    /// Its face, over the body's pose.
    face: Option<FaceFrame>,
}

/// A face's layers this frame (shared with the frames that reuse them
/// at a lower LOD), and the profile matched to its model.
struct FaceFrame {
    rig: Arc<FaceRig>,
    layers: Arc<FaceLayers>,
}

/// Where a face's LOD has got to: its level, the seconds since it was
/// last worked out, and the layers it made then.
#[derive(Default)]
struct FaceLodRun {
    level: FaceLod,
    waited: f64,
    last: Option<Arc<FaceLayers>>,
}

/// What faces cost in an update, for the Profiler (facial animation F7).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FaceStats {
    /// Faces at each level, in [`FaceLod::ALL`]'s order.
    pub faces: [usize; 4],
    /// Faces worked out this update; the rest reused their last layers.
    pub evaluated: usize,
    /// Controls that move something, over the faces shown.
    pub controls: usize,
    /// Time spent reading faces and moving them on, on the calling
    /// thread (composing them is part of posing).
    pub time: std::time::Duration,
}

impl FaceFrame {
    /// Composes the face over the body's `pose` (of `skeleton`), turns the
    /// eye and jaw bones in it, poses `model` and sets its blend shapes.
    fn pose(
        &self,
        model: &Model,
        skeleton: &Rig,
        mut pose: axcenity_assets::Pose,
    ) -> Option<Posed> {
        let controls = self.layers.compose(&self.rig, Some(skeleton), Some(&pose));
        self.rig.turn(&mut pose, &controls);
        let mut posed = model.pose(&pose)?;
        self.set_shapes(model, &controls, &mut posed);
        Some(posed)
    }

    /// A model with blend shapes and no skeleton, posed for its face.
    fn shapes_only(&self, model: &Model) -> Posed {
        let controls = self.layers.compose(&self.rig, None, None);
        let mut posed = model.rest_posed();
        self.set_shapes(model, &controls, &mut posed);
        posed
    }

    fn set_shapes(&self, model: &Model, controls: &[f32], posed: &mut Posed) {
        let mut weights = Vec::with_capacity(model.morph_targets.len());
        self.rig.weights(model, controls, &mut weights);
        posed.set_morph_weights(&weights);
    }
}

/// The profile a FaceControls was matched to its model with.
struct FaceRun {
    model_key: u64,
    /// Its `Profile` when it was matched.
    wanted: String,
    /// The project's profiles then; new ones (a file changed) match again.
    profiles: Arc<FaceProfiles>,
    rig: Option<Arc<FaceRig>>,
}

/// One animated thing being played.
#[derive(Default)]
struct Running {
    /// The skeleton's model it was made for; a new one binds clips again.
    model_key: Option<u64>,
    machine: Option<RunningMachine>,
    /// Clips by path, bound for this thing (`None`: it couldn't be read).
    clips: HashMap<String, Option<Arc<Clip>>>,
    applied: Applied,
    /// Its AutoPlay Animations have been started.
    started: bool,
    /// Where each track and Animation was after the last frame, in seconds,
    /// so a script's seek doesn't fire the events it jumps over.
    seen: HashMap<InstanceId, f64>,
    ragdoll: RagdollRun,
    /// Each IKControl's progress.
    ik: HashMap<InstanceId, ik_control::Run>,
    /// The skeleton's legs (found again for a new model), if it has them.
    legs: Option<Legs>,
    feet: feet::Run,
    face: Option<FaceRun>,
    /// Its face's blinks, darts and gaze, carried from frame to frame.
    face_motion: Option<FaceMotion>,
    face_lod: FaceLodRun,
}

/// Where a thing's Ragdoll has got to.
#[derive(Default)]
struct RagdollRun {
    /// It was on at the last update.
    on: bool,
    /// The pose physics gave it last, blended from as it lets go.
    held: Option<axcenity_assets::Pose>,
    /// Seconds since it was turned off.
    since: f64,
    /// Where its first body was at the last update, while it's on: the
    /// MeshPart follows it across the ground.
    root_at: Option<Vec3>,
}

/// An Animator's state machine and where it is in it.
struct RunningMachine {
    /// The `StateMachine` it was made for; a change starts it again.
    path: String,
    compiled: Arc<Compiled>,
    machine: Machine,
    /// Where each of its layers is, in the file's order.
    layers: Vec<Machine>,
}

/// Something playing over the state machine this frame.
#[derive(Clone)]
struct Layer {
    clip: Arc<Clip>,
    /// Phase in the clip (0 to 1, or more for a loop's later passes).
    phase: f64,
    weight: f32,
    order: u64,
}

/// What an animated thing has: its Animator (the first, if several), and
/// the Animations under it or under that Animator.
#[derive(Default)]
struct Parts {
    animator: Option<InstanceId>,
    animations: Vec<InstanceId>,
    ragdoll: Option<InstanceId>,
    ik: Vec<InstanceId>,
    face: Option<InstanceId>,
}

impl Animators {
    pub fn new() -> Self {
        Self::default()
    }

    /// Animators that work out poses on `threads` threads of their own (1:
    /// the calling thread), for tests and benchmarks; [`Animators::new`]
    /// shares one pool (see `threads.rs`).
    pub fn with_threads(threads: usize) -> Self {
        Self {
            threads: Threads::of(threads),
            ..Self::default()
        }
    }

    /// How long each thread spent working out poses in the last
    /// [`Animators::update`], by the thread's name (`main` is the calling
    /// thread), for the Profiler.
    pub fn thread_times(&self) -> &ThreadTimes {
        &self.thread_times
    }

    /// One whose characters can plant their feet (`FootPlacement`), finding
    /// the ground with `probe`.
    pub fn with_ground(probe: GroundProbe) -> Self {
        Self {
            ground: Some(probe),
            ..Self::default()
        }
    }

    /// Where the camera is in the world (the viewer) and which way it
    /// looks: faces with GazeMode Camera look at it, and facial LOD is
    /// measured from it (out of view or far off, a face costs less). Hosts
    /// set it before each update; `None` (a dedicated server) leaves those
    /// faces looking ahead and whole.
    pub fn set_viewer(
        &mut self,
        at: Option<axcenity_core::Vector3>,
        look: Option<axcenity_core::Vector3>,
    ) {
        self.viewer = at.map(crate::ragdoll::to_vec3);
        self.look = look
            .map(crate::ragdoll::to_vec3)
            .map(Vec3::normalize_or_zero)
            .filter(|look| *look != Vec3::ZERO);
    }

    /// What faces cost in the last [`update`](Self::update), for the
    /// Profiler.
    pub fn face_stats(&self) -> FaceStats {
        self.face_stats
    }

    /// The posed models, by MeshPart, after the last [`update`](Self::update).
    pub fn poses(&self) -> &Poses {
        &self.poses
    }

    /// Problems since the last call, one line each ("The Animator
    /// `Workspace.Hero.Animator` …"), each given once.
    pub fn take_errors(&mut self) -> Vec<String> {
        std::mem::take(&mut self.errors)
    }

    /// Forgets everything, so the next update starts each Animator from its
    /// default state with its files read again.
    pub fn clear(&mut self) {
        self.running.clear();
        self.poses.clear();
        self.reported.clear();
        self.changed.clear();
    }

    /// Plays the new version of the clip, state machine or model file at
    /// `path` from the next update, after it changed on disk (other files
    /// are ignored). Each Animator using it stays in the state it was in,
    /// matched by name, at the same point in it. A clip or state machine
    /// that can't be read any more keeps its last good version, and that's
    /// reported once. True if a clip or state machine playing now uses it.
    pub fn reload(&mut self, path: &str) -> bool {
        let file = path.split('#').next().unwrap_or(path);
        if path.ends_with(CLIP_EXTENSION)
            || path.ends_with(STATE_MACHINE_EXTENSION)
            || rebinds_everything(file)
        {
            self.changed.insert(file.to_owned());
        }
        self.plays(file)
    }

    /// True if a clip or state machine playing now is the file at `path`.
    fn plays(&self, path: &str) -> bool {
        self.running.values().any(|running| {
            running.clips.contains_key(path)
                || running
                    .machine
                    .as_ref()
                    .is_some_and(|machine| machine.uses(|used| used == path))
        })
    }

    /// The changed files something plays that can be read now. Those that
    /// can't are reported and left out, so what uses them keeps its last
    /// good version.
    fn readable_changes(&mut self, assets: &Assets) -> HashSet<String> {
        let mut changed = std::mem::take(&mut self.changed);
        changed.retain(|path| {
            if !rebinds_everything(path) && !self.plays(path) {
                return false;
            }
            let problem = if path.ends_with(CLIP_EXTENSION) {
                load_clip(assets, path).err()
            } else if path.ends_with(STATE_MACHINE_EXTENSION) {
                read_state_machine(assets, path).err()
            } else {
                None
            };
            if let Some(problem) = &problem {
                self.errors.push(format!(
                    "Could not reload `{path}`: {problem}. The last good version stays."
                ));
            }
            problem.is_none()
        });
        changed
    }

    /// Moves everything animated in the Workspace on by `dt` seconds of
    /// game time (0 just applies it), posing skeletons and setting keyed
    /// properties. Fills in the state machine's parameters an Animator
    /// doesn't have yet, starts the states scripts asked for
    /// (`animator:Play`), the AutoPlay Animations the first time, and keeps
    /// `CurrentState` and each track's `TimePosition` up to date. Returns
    /// what scripts should hear about, in order.
    pub fn update(&mut self, world: &mut World, assets: &Assets, dt: f64) -> Vec<AnimatorEvent> {
        let mut owners: Vec<(InstanceId, Parts)> = Vec::new();
        let animated = world.all_below(world.workspace(), |class| {
            matches!(
                class,
                ClassName::Animator
                    | ClassName::Animation
                    | ClassName::Ragdoll
                    | ClassName::IKControl
                    | ClassName::FaceControls
            )
        });
        for id in animated {
            let Some(instance) = world.get(id) else {
                continue;
            };
            let class = instance.class();
            let Some(mut owner) = instance.parent() else {
                continue;
            };
            // An Animation or IKControl under an Animator animates the
            // Animator's parent.
            if matches!(class, ClassName::Animation | ClassName::IKControl)
                && world
                    .get(owner)
                    .is_some_and(|parent| parent.class() == ClassName::Animator)
            {
                match world.get(owner).and_then(|parent| parent.parent()) {
                    Some(above) => owner = above,
                    None => continue,
                }
            }
            let index = match owners.iter().position(|(o, _)| *o == owner) {
                Some(index) => index,
                None => {
                    owners.push((owner, Parts::default()));
                    owners.len() - 1
                }
            };
            let parts = &mut owners[index].1;
            match class {
                ClassName::Animator => {
                    parts.animator.get_or_insert(id);
                }
                ClassName::Ragdoll => {
                    parts.ragdoll.get_or_insert(id);
                }
                ClassName::IKControl => parts.ik.push(id),
                ClassName::FaceControls => {
                    parts.face.get_or_insert(id);
                }
                _ => parts.animations.push(id),
            }
        }
        self.face_stats = FaceStats::default();
        let alive: HashSet<InstanceId> = owners.iter().map(|(owner, _)| *owner).collect();
        self.running.retain(|owner, _| alive.contains(owner));
        let changed = self.readable_changes(assets);

        let mut events = Vec::new();
        let mut posed = HashSet::new();
        let mut jobs = Vec::new();
        for (owner, parts) in owners {
            if self.update_one(
                world,
                assets,
                owner,
                &parts,
                &changed,
                dt,
                &mut events,
                &mut jobs,
            ) {
                posed.insert(owner);
            }
        }
        // The poses that need nothing more from the World, across threads.
        let running = &self.running;
        let (made, times) = self.threads.map(&jobs, |job| {
            let machine = running
                .get(&job.owner)
                .and_then(|running| running.machine.as_ref());
            let pose = pose_of(
                machine,
                &job.rig,
                &job.parameters,
                &job.layer_weights,
                &job.bone_layers,
            );
            match &job.face {
                Some(face) => face.pose(&job.model, &job.rig, pose),
                None => job.model.pose(&pose),
            }
        });
        self.thread_times = times;
        for (job, pose) in jobs.iter().zip(made) {
            match pose {
                Some(pose) => {
                    self.poses.insert(job.owner, Arc::new(pose));
                }
                None => {
                    posed.remove(&job.owner);
                }
            }
        }
        // A MeshPart nothing poses any more goes back to its still model.
        self.poses.retain(|part, _| posed.contains(part));
        events
    }

    /// Plays one animated thing, binding again what uses the `changed`
    /// files. Returns true if it posed a skeleton.
    #[allow(clippy::too_many_arguments)]
    fn update_one(
        &mut self,
        world: &mut World,
        assets: &Assets,
        owner: InstanceId,
        parts: &Parts,
        changed: &HashSet<String>,
        dt: f64,
        events: &mut Vec<AnimatorEvent>,
        jobs: &mut Vec<PoseJob>,
    ) -> bool {
        let mesh = mesh_model(world, assets, owner);
        let model = mesh.clone().filter(|model| model.rig.is_some());
        let rig = model.as_ref().and_then(|model| model.rig.clone());
        let model_key = model.as_ref().map(|model| model.key());
        let (enabled, speed) = parts.animator.map_or((true, 1.0), |id| settings(world, id));
        // The game's time, for the ragdoll, which the Animator's speed
        // doesn't change.
        let game_dt = dt;
        let dt = if enabled { dt * speed } else { 0.0 };
        // Reduced motion: a UI element's clips don't move (see `Motion`).
        let motion = if world.ui_settings().reduced_motion
            && world.get(owner).is_some_and(|thing| thing.class().is_gui())
        {
            Motion::Reduced
        } else {
            Motion::Full
        };

        let mut running = self.running.remove(&owner).unwrap_or_default();
        // A machine made again (for a new model or changed files) goes on
        // from the state it was in.
        let mut was_in = None;
        if running.model_key != model_key {
            running.model_key = model_key;
            running.clips.clear();
            running.legs = rig.as_deref().and_then(Legs::find);
            running.feet = feet::Run::default();
            was_in = running.machine.take().map(RunningMachine::where_it_is);
        }
        if !changed.is_empty() {
            // A changed model file has new takes for its linked clips; a
            // changed rig definition can change how any take retargets.
            let models = changed.iter().any(|path| rebinds_everything(path));
            if models {
                running.clips.clear();
            } else {
                running.clips.retain(|path, _| !changed.contains(path));
            }
            let stale = running
                .machine
                .as_ref()
                .is_some_and(|machine| models || machine.uses(|used| changed.contains(used)));
            if stale {
                was_in = running.machine.take().map(RunningMachine::where_it_is);
            }
        }

        // A ragdoll just turned off gets up with its `GetUpState`.
        if let (Some(ragdoll), Some(animator)) = (parts.ragdoll, parts.animator)
            && running.ragdoll.on
            && !is_on(world, ragdoll)
        {
            let state = world
                .get(ragdoll)
                .and_then(|r| r.property("GetUpState"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            if !state.is_empty()
                && let Some(live) = world.animator_mut(animator)
            {
                live.play = Some(PlayRequest {
                    state,
                    blend: Some(0.0),
                });
            }
        }

        // 1. The state machine.
        let machine_path = parts
            .animator
            .and_then(|id| world.get(id))
            .and_then(|animator| animator.property("StateMachine"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        if machine_path.is_empty() {
            running.machine = None;
        } else if running
            .machine
            .as_ref()
            .is_none_or(|machine| machine.path != machine_path)
        {
            let animator = parts
                .animator
                .expect("a state machine comes from an Animator");
            running.machine = self
                .compile(world, assets, animator, &machine_path, rig.as_deref())
                .map(|compiled| {
                    let mut machine = Machine::new(&compiled);
                    if let Some((path, state, phase)) = was_in
                        && path == machine_path
                        && let Some(state) = compiled.state_index(&state)
                    {
                        machine.current = Playing { state, phase };
                    }
                    RunningMachine {
                        path: machine_path.clone(),
                        machine,
                        layers: compiled
                            .layers()
                            .iter()
                            .map(|layer| Machine::new(&layer.machine))
                            .collect(),
                        compiled,
                    }
                });
        }
        if let (Some(animator), Some(machine)) = (parts.animator, &mut running.machine) {
            self.run_machine(
                world,
                owner,
                animator,
                machine,
                rig.as_deref(),
                dt,
                enabled,
                events,
            );
        }

        // 2. What plays on top.
        if !running.started {
            running.started = true;
            for &animation in &parts.animations {
                let auto = world
                    .get(animation)
                    .and_then(|a| a.property("AutoPlay"))
                    .and_then(Value::as_bool)
                    == Some(true);
                if auto && let Some(state) = world.property_animation_mut(animation) {
                    state.play = true;
                }
            }
        }
        let mut layers = Vec::new();
        for &animation in &parts.animations {
            if let Some(layer) = self.run_animation(
                world,
                assets,
                &mut running,
                parts.animator,
                animation,
                rig.as_deref(),
                (dt, motion),
                events,
            ) {
                layers.push(layer);
            }
        }
        if let Some(animator) = parts.animator {
            let tracks = world
                .animator(animator)
                .map(|state| state.tracks.clone())
                .unwrap_or_default();
            let tracks: Vec<InstanceId> = tracks
                .into_iter()
                .filter(|&track| world.animation_track(track).is_some())
                .collect();
            if let Some(state) = world.animator_mut(animator) {
                state.tracks.clone_from(&tracks);
            }
            for track in tracks {
                if let Some(layer) = self.run_track(
                    world,
                    assets,
                    &mut running,
                    track,
                    rig.as_deref(),
                    (dt, motion),
                    events,
                ) {
                    layers.push(layer);
                }
            }
        }
        layers.sort_by_key(|layer| layer.order);

        // Properties: the state machine's, then each layer over them.
        if let Some(machine) = &running.machine {
            let current = machine.machine.current;
            let parameters = parts
                .animator
                .and_then(|id| world.animator(id))
                .map(|live| live.parameters.clone())
                .unwrap_or_default();
            let values = machine
                .compiled
                .properties(current.state, current.phase, &parameters);
            apply_evaluated(world, owner, &values, &mut running.applied);
        }
        for layer in &layers {
            let values = blend_evaluated(
                world,
                owner,
                layer.clip.properties(layer.phase),
                layer.weight,
                &running.applied,
            );
            apply_evaluated(world, owner, &values, &mut running.applied);
        }
        let missing: Vec<String> = running.applied.missing.iter().cloned().collect();
        if !missing.is_empty() {
            let name = world.full_name(owner).unwrap_or_default();
            for target in missing {
                self.report(format!(
                    "`{name}` has no `{target}` for its animation to move, so that track is skipped."
                ));
            }
        }

        // The face's controls, read now so the pose can be made anywhere.
        let face = parts.face.and_then(|face| {
            self.face_frame(
                world,
                assets,
                face,
                owner,
                mesh.as_deref(),
                &mut running,
                game_dt,
            )
        });

        // Bones: the state machine's pose, then each layer over it.
        let mut posed = false;
        if let (Some(model), Some(rig)) = (&model, &rig) {
            let bone_layers: Vec<Layer> = layers
                .iter()
                .filter(|layer| layer.clip.animated_bones().iter().any(|&moved| moved))
                .cloned()
                .collect();
            // A controller's foot placement needs a pose under it, and one
            // just turned off still has its feet to bring back.
            let controller = parts.animator.and_then(|_| feet::wanted(world, owner));
            let wants_feet = controller.is_some_and(|(on, _)| on);
            if running.machine.is_some()
                || !bone_layers.is_empty()
                || parts.ragdoll.is_some()
                || !parts.ik.is_empty()
                || wants_feet
                || running.feet.busy()
                || face.is_some()
            {
                let live = parts.animator.and_then(|animator| world.animator(animator));
                let parameters = live.map(|live| live.parameters.clone()).unwrap_or_default();
                let layer_weights = running
                    .machine
                    .as_ref()
                    .map(|machine| {
                        machine
                            .compiled
                            .layers()
                            .iter()
                            .map(|layer| {
                                live.and_then(|live| live.layer_weights.get(&layer.name))
                                    .map_or(layer.weight, |w| w.weight)
                                    as f32
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                // Nothing after the clips reads the World: the pose can be
                // made on another thread, with the others (`update`).
                let only_clips = parts.ragdoll.is_none()
                    && parts.ik.is_empty()
                    && !wants_feet
                    && !running.feet.busy();
                if only_clips {
                    running.ik.clear();
                    jobs.push(PoseJob {
                        owner,
                        model: Arc::clone(model),
                        rig: Arc::clone(rig),
                        parameters,
                        layer_weights,
                        bone_layers,
                        face,
                    });
                    posed = true;
                } else {
                    let mut pose = pose_of(
                        running.machine.as_ref(),
                        rig,
                        &parameters,
                        &layer_weights,
                        &bone_layers,
                    );
                    if let Some(ragdoll) = parts.ragdoll {
                        pose = run_ragdoll(
                            world,
                            owner,
                            ragdoll,
                            rig,
                            pose,
                            &mut running.ragdoll,
                            game_dt,
                        );
                    }
                    // A ragdoll that's on is physics' to pose: IK waits.
                    if parts.ragdoll.is_some_and(|ragdoll| is_on(world, ragdoll)) {
                        running.feet = feet::Run::default();
                        running.ik.clear();
                    } else {
                        if let (Some(probe), Some((on, holder))) = (self.ground, controller) {
                            match &running.legs {
                                Some(legs) => feet::plant(
                                    world,
                                    assets,
                                    probe,
                                    owner,
                                    holder,
                                    rig,
                                    legs,
                                    &mut pose,
                                    &mut running.feet,
                                    on,
                                    game_dt,
                                ),
                                None if on => {
                                    let name = world.full_name(owner).unwrap_or_default();
                                    self.report(format!(
                                        "`{name}` has FootPlacement on, but its skeleton has no legs (bones named such as `LeftFoot` and `RightFoot`), so its feet stay where the animation has them."
                                    ));
                                }
                                None => {}
                            }
                        }
                        let problems = ik_control::apply(
                            world,
                            owner,
                            rig,
                            &mut pose,
                            &parts.ik,
                            &mut running.ik,
                            game_dt,
                        );
                        for problem in problems {
                            self.report(problem);
                        }
                    }
                    let made = match &face {
                        Some(face) => face.pose(model, rig, pose),
                        None => model.pose(&pose),
                    };
                    if let Some(pose) = made {
                        self.poses.insert(owner, Arc::new(pose));
                        posed = true;
                    }
                }
            }
        } else if let (Some(face), Some(mesh)) = (&face, &mesh) {
            // Blend shapes and no skeleton: posed for the face alone.
            self.poses.insert(owner, Arc::new(face.shapes_only(mesh)));
            posed = true;
        } else if let Some(&control) = parts.ik.first() {
            let name = world.full_name(control).unwrap_or_default();
            self.report(format!(
                "The IKControl `{name}` only works in a MeshPart with a skeleton."
            ));
        }
        self.running.insert(owner, running);
        posed
    }

    /// Moves `animator`'s state machine on by `dt`, takes its requests and
    /// transitions, and moves its MeshPart by root motion.
    #[allow(clippy::too_many_arguments)]
    fn run_machine(
        &mut self,
        world: &mut World,
        owner: InstanceId,
        animator: InstanceId,
        running: &mut RunningMachine,
        rig: Option<&Rig>,
        dt: f64,
        enabled: bool,
        events: &mut Vec<AnimatorEvent>,
    ) {
        let compiled = running.compiled.clone();
        // Parameters the animator doesn't have yet start at their defaults.
        let request = world.animator_mut(animator).and_then(|state| {
            for def in &compiled.file.parameters {
                state
                    .parameters
                    .entry(def.name().to_owned())
                    .or_insert_with(|| def.initial());
            }
            for layer in compiled.layers() {
                let weight = state
                    .layer_weights
                    .entry(layer.name.clone())
                    .or_insert_with(|| LayerWeight::new(layer.weight));
                weight.step(dt);
            }
            state.play.take()
        });
        if let Some(request) = request {
            match compiled.state_index(&request.state) {
                Some(state) => {
                    let blend = request.blend.unwrap_or(DEFAULT_BLEND).max(0.0);
                    running.machine.play(state, blend);
                }
                None => {
                    let name = world.full_name(animator).unwrap_or_default();
                    self.report(format!(
                        "The Animator `{name}` was asked to play `{}`, but its state machine has no state with that name.",
                        request.state
                    ));
                }
            }
        }
        // Blend trees mix by the parameters as they are now.
        let parameters = world
            .animator(animator)
            .map(|live| live.parameters.clone())
            .unwrap_or_default();
        let mut markers = Vec::new();
        if enabled {
            let advanced = running.machine.advance(&compiled, rig, &parameters, dt);
            markers = advanced.events;
            if let Some(rig) = rig {
                move_by_root_motion(world, owner, rig, advanced.motion);
            }
            // Layers move on by the same clock, without root motion.
            for (layer, machine) in compiled.layers().iter().zip(&mut running.layers) {
                let advanced = machine.advance(&layer.machine, None, &parameters, dt);
                markers.extend(advanced.events);
            }
            // Transitions whose time has come, now the clock has moved on.
            if let Some(live) = world.animator_mut(animator) {
                running.machine.evaluate(&compiled, &mut live.parameters);
                for (layer, machine) in compiled.layers().iter().zip(&mut running.layers) {
                    machine.evaluate(&layer.machine, &mut live.parameters);
                }
            }
        }
        // Parameters a script set that the state machine doesn't have can't
        // change anything: say so, once each.
        if let Some(live) = world.animator(animator) {
            let unknown: Vec<String> = live
                .parameters
                .keys()
                .filter(|name| {
                    !compiled
                        .file
                        .parameters
                        .iter()
                        .any(|def| def.name() == name.as_str())
                })
                .cloned()
                .collect();
            if !unknown.is_empty() {
                let name = world.full_name(animator).unwrap_or_default();
                for parameter in unknown {
                    self.report(format!(
                        "The Animator `{name}` was given the parameter `{parameter}`, which its state machine `{path}` doesn't have, so it changes nothing.",
                        path = running.path
                    ));
                }
            }
        }
        // The same for layer weights set for a layer it doesn't have.
        if let Some(live) = world.animator(animator) {
            let unknown: Vec<String> = live
                .layer_weights
                .keys()
                .filter(|name| !compiled.layers().iter().any(|l| &l.name == *name))
                .cloned()
                .collect();
            if !unknown.is_empty() {
                let name = world.full_name(animator).unwrap_or_default();
                for layer in unknown {
                    self.report(format!(
                        "The Animator `{name}` was given a weight for the layer `{layer}`, which its state machine `{path}` doesn't have, so it changes nothing.",
                        path = running.path
                    ));
                }
            }
        }
        let state = compiled
            .state_name(running.machine.current.state)
            .to_owned();
        if let Some(live) = world.animator_mut(animator)
            && live.current_state != state
        {
            let previous = std::mem::replace(&mut live.current_state, state.clone());
            events.push(AnimatorEvent::StateChanged {
                animator,
                state,
                previous,
            });
        }
        events.extend(
            markers
                .into_iter()
                .map(|name| AnimatorEvent::Marker { animator, name }),
        );
    }

    /// Takes a play (AutoPlay) or stop request and moves its playback on. A layer once it has been played (it keeps its last
    /// frame after it ends or stops).
    #[allow(clippy::too_many_arguments)]
    fn run_animation(
        &mut self,
        world: &mut World,
        assets: &Assets,
        running: &mut Running,
        animator: Option<InstanceId>,
        animation: InstanceId,
        rig: Option<&Rig>,
        (dt, motion): (f64, Motion),
        events: &mut Vec<AnimatorEvent>,
    ) -> Option<Layer> {
        let (path, enabled, speed, looping) = {
            let instance = world.get(animation)?;
            let path = instance
                .property("Clip")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let (enabled, speed) = settings(world, animation);
            let looping = instance.property("Looped").and_then(Value::as_bool) == Some(true);
            (path, enabled, speed, looping)
        };
        let requested = world
            .property_animation(animation)
            .is_some_and(|state| state.play || state.stop || state.order > 0);
        if path.is_empty() || !requested {
            return None;
        }
        let clip = self.clip(world, assets, running, animation, &path, rig)?;
        let order = match animator {
            Some(animator) if world.property_animation(animation).is_some_and(|s| s.play) => world
                .animator_mut(animator)
                .map(|state| state.start_order()),
            _ => None,
        };
        let state = world.property_animation_mut(animation)?;
        if let Some(time) = state.seek.take() {
            state.time = time.max(0.0);
        }
        if std::mem::take(&mut state.stop) {
            state.playing = false;
            state.time = 0.0;
        }
        if std::mem::take(&mut state.play) {
            state.playing = true;
            state.time = 0.0;
            state.order = order.unwrap_or(state.order.max(1));
            running.seen.remove(&animation);
        }
        let from = running.seen.get(&animation).copied().unwrap_or(state.time);
        let duration = clip.duration();
        let mut completed = false;
        if enabled && state.playing {
            let mut next = match motion {
                Motion::Full => state.time + dt * speed,
                Motion::Reduced if looping => state.time,
                Motion::Reduced => duration,
            };
            if !looping && next >= duration {
                next = duration;
                state.playing = false;
                completed = true;
            }
            if next.is_finite() {
                state.time = next;
            }
        }
        let time = state.time;
        let order = state.order;
        running.seen.insert(animation, time);
        events.extend(
            events_between(&clip, from, time, looping)
                .into_iter()
                .map(|name| AnimatorEvent::AnimationMarker { animation, name }),
        );
        if completed {
            events.push(AnimatorEvent::AnimationCompleted { animation });
        }
        Some(Layer {
            phase: phase_at(time, duration, looping),
            clip,
            weight: 1.0,
            order,
        })
    }

    /// Moves an AnimationTrack on: its time, its fade, and its end. A layer
    /// while it counts for anything.
    #[allow(clippy::too_many_arguments)]
    fn run_track(
        &mut self,
        world: &mut World,
        assets: &Assets,
        running: &mut Running,
        track: InstanceId,
        rig: Option<&Rig>,
        (dt, motion): (f64, Motion),
        events: &mut Vec<AnimatorEvent>,
    ) -> Option<Layer> {
        let animation = world.animation_track(track)?.animation;
        let path = world
            .get(animation)
            .and_then(|a| a.property("Clip"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let clip = if path.is_empty() {
            None
        } else {
            self.clip(world, assets, running, animation, &path, rig)
        };
        let state = world.animation_track_mut(track)?;
        let Some(clip) = clip else {
            // Nothing to play: it stops at once.
            if state.playing || state.stopping {
                state.playing = false;
                state.stopping = false;
                state.weight = 0.0;
                events.push(AnimatorEvent::TrackStopped { track });
            }
            return None;
        };
        let duration = clip.duration();
        state.length = clip.file().duration;
        let looping = *state.looped.get_or_insert(clip.looping());
        if !(state.playing || state.stopping) {
            running.seen.remove(&track);
            return None;
        }
        let from = running.seen.get(&track).copied().unwrap_or(state.time);
        let mut ended = false;
        if motion == Motion::Reduced {
            // It ends at once at the end it's going toward, or (looping)
            // holds where it is; fades are instant.
            if !looping && state.playing && state.speed != 0.0 {
                state.time = if state.speed < 0.0 { 0.0 } else { duration };
                ended = true;
            }
            state.weight = state.target_weight;
        } else if dt > 0.0 {
            let mut next = state.time + dt * state.speed;
            if !looping && state.playing && (next >= duration || next < 0.0) {
                next = next.clamp(0.0, duration);
                ended = true;
            }
            if next.is_finite() {
                state.time = next;
            }
            // The weight moves to its target over the fade.
            let step = if state.fade > 0.0 {
                dt / state.fade
            } else {
                1.0
            };
            let gap = state.target_weight - state.weight;
            state.weight += gap.clamp(-step, step);
        } else if state.fade <= 0.0 {
            state.weight = state.target_weight;
        }
        let time = state.time;
        let weight = state.weight.clamp(0.0, 1.0) as f32;
        let order = state.order;
        if ended {
            // A clip that doesn't loop stops at its end: its last frame is
            // applied this once, then it lets go.
            state.playing = false;
            state.stopping = false;
            state.weight = 0.0;
        } else if state.stopping && state.weight <= 0.0 {
            state.stopping = false;
            ended = true;
        }
        running.seen.insert(track, time);
        events.extend(
            events_between(&clip, from, time, looping)
                .into_iter()
                .map(|name| AnimatorEvent::TrackMarker { track, name }),
        );
        if ended {
            events.push(AnimatorEvent::TrackStopped { track });
        }
        (weight > 0.0).then(|| Layer {
            phase: phase_at(time, duration, looping),
            clip,
            weight,
            order,
        })
    }

    /// The clip at `path`, bound for this thing (read once, reported once if
    /// it can't be). `user` names who asked, for the message.
    fn clip(
        &mut self,
        world: &World,
        assets: &Assets,
        running: &mut Running,
        user: InstanceId,
        path: &str,
        rig: Option<&Rig>,
    ) -> Option<Arc<Clip>> {
        if let Some(clip) = running.clips.get(path) {
            return clip.clone();
        }
        let clip = load_clip(assets, path)
            .and_then(|loaded| loaded.bind(rig))
            .map(Arc::new);
        let clip = match clip {
            Ok(clip) => Some(clip),
            Err(problem) => {
                let name = world.full_name(user).unwrap_or_default();
                self.report(format!(
                    "The Animation `{name}` could not load `{path}`: {problem}"
                ));
                None
            }
        };
        running.clips.insert(path.to_owned(), clip.clone());
        clip
    }

    /// Reads and makes ready `animator`'s state machine and its clips,
    /// reporting what goes wrong. `None` if it can't be used at all.
    fn compile(
        &mut self,
        world: &World,
        assets: &Assets,
        animator: InstanceId,
        path: &str,
        rig: Option<&Rig>,
    ) -> Option<Arc<Compiled>> {
        let name = world.full_name(animator).unwrap_or_default();
        let file = match read_state_machine(assets, path) {
            Ok(file) => file,
            Err(problem) => {
                self.report(format!(
                    "Could not load the state machine `{path}`: {problem}"
                ));
                return None;
            }
        };
        if file.states.is_empty() {
            self.report(format!(
                "The Animator `{name}` plays `{path}`, which has no states yet."
            ));
            return None;
        }
        let (compiled, problems) = Compiled::new(file, rig, |clip| load_clip(assets, clip));
        for problem in problems {
            self.report(format!("The state machine `{path}`: {problem}."));
        }
        Some(Arc::new(compiled))
    }

    /// The FaceControls `face`'s controls this frame and the profile matched
    /// to `model`, or `None` when it's off or has nothing to move (said
    /// once).
    #[allow(clippy::too_many_arguments)]
    fn face_frame(
        &mut self,
        world: &mut World,
        assets: &Assets,
        face: InstanceId,
        owner: InstanceId,
        model: Option<&Model>,
        running: &mut Running,
        dt: f64,
    ) -> Option<FaceFrame> {
        let instance = world.get(face)?;
        if instance.property("Enabled").and_then(Value::as_bool) == Some(false) {
            return None;
        }
        let name = || world.full_name(face).unwrap_or_default();
        let in_mesh_part = instance
            .parent()
            .and_then(|parent| world.get(parent))
            .is_some_and(|parent| parent.class() == ClassName::MeshPart);
        // A model still loading (or that can't be, which Assets reports)
        // just has no face yet.
        if in_mesh_part && model.is_none() {
            return None;
        }
        let Some(model) =
            model.filter(|model| !model.morph_targets.is_empty() || model.rig.is_some())
        else {
            self.report(format!(
                "The FaceControls `{}` moves nothing: it works in a MeshPart whose model has blend shapes or eye and jaw bones.",
                name()
            ));
            return None;
        };
        let wanted = instance
            .property("Profile")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let profiles = assets.face_profiles();
        let stale = running.face.as_ref().is_none_or(|run| {
            run.model_key != model.key()
                || run.wanted != wanted
                || !Arc::ptr_eq(&run.profiles, &profiles)
        });
        if stale {
            let rig = match choose_face_profile(model, &profiles, &wanted) {
                Ok((profile_name, profile)) => {
                    let (rig, problems) = FaceRig::bind(model, &profile_name, &profile);
                    for problem in problems {
                        if problem.severity() == crate::retarget::Severity::Error {
                            self.report(format!(
                                "The FaceControls `{}` (face profile `{profile_name}`): {problem}",
                                name()
                            ));
                        }
                    }
                    Some(Arc::new(rig))
                }
                Err(why) => {
                    self.report(format!("The FaceControls `{}`: {why}.", name()));
                    None
                }
            };
            running.face = Some(FaceRun {
                model_key: model.key(),
                wanted,
                profiles,
                rig,
            });
        }
        let rig = running.face.as_ref()?.rig.clone()?;
        let started = std::time::Instant::now();
        let lod = self.face_lod(world, face, owner);
        let lod_run = &mut running.face_lod;
        lod_run.waited += dt;
        let due = lod_run.level != lod
            || lod_run.waited >= lod.interval()
            || (lod != FaceLod::Off && lod_run.last.is_none());
        self.face_stats.faces[lod as usize] += 1;
        if lod != FaceLod::Off {
            self.face_stats.controls += rig.controls_used();
        }
        if !due {
            let layers = lod_run.last.clone();
            self.face_stats.time += started.elapsed();
            return Some(FaceFrame {
                rig,
                layers: layers?,
            });
        }
        let step = std::mem::take(&mut lod_run.waited);
        lod_run.level = lod;
        let procedural = world
            .get(face)
            .and_then(|face| face.property("ProceduralEnabled"))
            .and_then(Value::as_bool)
            != Some(false);
        let detail = FaceDetail::at(lod, procedural);
        let motion = running
            .face_motion
            .get_or_insert_with(|| FaceMotion::new(face.to_bits()));
        let mut problems = Vec::new();
        let lookers = Lookers {
            viewer: self.viewer,
            heads: &mut self.heads,
        };
        let layers = face_layers(
            world,
            assets,
            face,
            owner,
            model,
            motion,
            lookers,
            detail,
            step,
            &mut problems,
        )
        .map(Arc::new);
        for problem in problems {
            self.report(problem);
        }
        self.face_stats.evaluated += 1;
        self.face_stats.time += started.elapsed();
        // Off, it only kept time: the face rests.
        running.face_lod.last = layers.clone().filter(|_| lod != FaceLod::Off);
        Some(FaceFrame {
            rig,
            layers: layers.filter(|_| lod != FaceLod::Off)?,
        })
    }

    /// The LOD of the FaceControls `face` in `owner`: by its LodDistance,
    /// how far the owner is from the viewer and whether it's in view (in
    /// front of the camera, within 75 degrees of where it looks).
    fn face_lod(&self, world: &World, face: InstanceId, owner: InstanceId) -> FaceLod {
        let lod_distance = world
            .get(face)
            .and_then(|face| face.property("LodDistance"))
            .and_then(Value::as_number)
            .unwrap_or(30.0) as f32;
        let (Some(viewer), Some(at)) = (self.viewer, ik_control::frame_of(world, owner)) else {
            return FaceLod::Full;
        };
        let to = at.position - viewer;
        let distance = to.length();
        let in_view = self.look.is_none_or(|look| {
            distance < 2.0 || look.dot(to / distance) > 75f32.to_radians().cos()
        });
        FaceLod::of(Some(distance), lod_distance, in_view)
    }

    fn report(&mut self, message: String) {
        if self.reported.insert(message.clone()) {
            self.errors.push(message);
        }
    }
}

/// Whether the Ragdoll `ragdoll` is on.
fn is_on(world: &World, ragdoll: InstanceId) -> bool {
    world
        .get(ragdoll)
        .and_then(|r| r.property("Enabled"))
        .and_then(Value::as_bool)
        == Some(true)
}

/// The pose of the MeshPart `owner` with its Ragdoll `ragdoll`, over the
/// Animator's `base` pose. While it's on, physics moves the bodies and they
/// pose their bones, and the MeshPart follows the first body (the hips)
/// across the ground. Once it's off, the pose blends from the ragdoll's last
/// back to `base` over `BlendTime` seconds, and the bodies follow the pose,
/// so they're where the animation is when it's turned on again.
fn run_ragdoll(
    world: &mut World,
    owner: InstanceId,
    ragdoll: InstanceId,
    rig: &Rig,
    base: axcenity_assets::Pose,
    run: &mut RagdollRun,
    dt: f64,
) -> axcenity_assets::Pose {
    let bodies = ragdoll::bodies(world, ragdoll, rig);
    if is_on(world, ragdoll) {
        let root = bodies.first().and_then(|body| {
            let at = world.get(body.part)?.property("Position")?.as_vector3()?;
            Some(Vec3::new(at.x, at.y, at.z))
        });
        if let (Some(now), Some(before), true) = (root, run.root_at, run.on) {
            let travel = now - before;
            if let Some(at) = world
                .get(owner)
                .and_then(|p| p.property("Position"))
                .and_then(Value::as_vector3)
                && travel.is_finite()
            {
                let moved = Vector3::new(at.x + travel.x, at.y, at.z + travel.z);
                let _ = world.set_property(owner, "Position", Value::Vector3(moved));
            }
        }
        run.root_at = root;
        run.on = true;
        run.since = 0.0;
        let Some(place) = ragdoll::placement(world, owner, rig) else {
            return base;
        };
        let pose = ragdoll::pose_from_bodies(world, rig, place, &base, &bodies);
        run.held = Some(pose.clone());
        return pose;
    }
    if run.on {
        // Physics takes the MeshPart back from rest.
        run.on = false;
        run.root_at = None;
        let still = Value::Vector3(Vector3::new(0.0, 0.0, 0.0));
        let _ = world.set_property(owner, "AssemblyLinearVelocity", still.clone());
        let _ = world.set_property(owner, "AssemblyAngularVelocity", still);
    }
    let blend_time = world
        .get(ragdoll)
        .and_then(|r| r.property("BlendTime"))
        .and_then(Value::as_number)
        .filter(|t| t.is_finite())
        .unwrap_or(0.0)
        .max(0.0);
    let mut pose = base;
    if let Some(held) = &run.held {
        run.since += dt;
        let weight = if blend_time > 0.0 {
            1.0 - run.since / blend_time
        } else {
            0.0
        };
        if weight > 0.0 {
            for (bone, from) in pose.iter_mut().zip(held) {
                *bone = bone.lerp(from, weight as f32);
            }
        } else {
            run.held = None;
        }
    }
    if let Some(place) = ragdoll::placement(world, owner, rig) {
        for (part, position, rotation) in ragdoll::body_places(rig, place, &pose, &bodies) {
            ragdoll::move_body(world, part, position, rotation, dt);
        }
    }
    pose
}

/// `Enabled` and `Speed` of an Animator or Animation.
fn settings(world: &World, id: InstanceId) -> (bool, f64) {
    let Some(instance) = world.get(id) else {
        return (true, 1.0);
    };
    let enabled = instance.property("Enabled").and_then(Value::as_bool) != Some(false);
    let speed = instance
        .property("Speed")
        .and_then(Value::as_number)
        .filter(|speed| speed.is_finite())
        .unwrap_or(1.0);
    (enabled, speed)
}

/// The pose the clips make: the state machine's (each of its layers over
/// the ones under it, at `layer_weights`), or the rest pose without one,
/// then each of `bone_layers` over that. Reads nothing but its arguments,
/// so it runs on any thread.
fn pose_of(
    machine: Option<&RunningMachine>,
    rig: &Rig,
    parameters: &BTreeMap<String, Parameter>,
    layer_weights: &[f32],
    bone_layers: &[Layer],
) -> axcenity_assets::Pose {
    let mut pose = match machine {
        Some(machine) => {
            let mut pose = machine.machine.pose(&machine.compiled, rig, parameters);
            // Each layer over the ones under it, the last on top.
            for ((layer, run), &weight) in machine
                .compiled
                .layers()
                .iter()
                .zip(&machine.layers)
                .zip(layer_weights)
            {
                if weight <= 0.0 {
                    continue;
                }
                let (over, cover) = run.layer_pose(&layer.machine, rig, parameters);
                layer.mix(&mut pose, &over, &cover, weight, rig);
            }
            pose
        }
        None => rig.rest_pose(),
    };
    for layer in bone_layers {
        let mut over = rig.rest_pose();
        layer.clip.sample(layer.phase, rig, &mut over);
        for (bone, moved) in layer.clip.animated_bones().iter().enumerate() {
            if *moved && let (Some(under), Some(over)) = (pose.get_mut(bone), over.get(bone)) {
                *under = under.lerp(over, layer.weight);
            }
        }
    }
    pose
}

/// The model of `owner` if it's a MeshPart whose model is loaded.
pub(crate) fn mesh_model(world: &World, assets: &Assets, owner: InstanceId) -> Option<Arc<Model>> {
    let part = world.get(owner)?;
    if part.class() != ClassName::MeshPart {
        return None;
    }
    let mesh_id = part.property("MeshId").and_then(Value::as_str)?;
    // Like drawing, this never waits: a model still loading has no pose yet.
    let Slot::Ready(model) = assets.model_slot(mesh_id) else {
        return None;
    };
    Some(model)
}

/// The phase of a clip `duration` seconds long at `time` seconds: a loop
/// goes round (the very end of a pass holds its last frame), otherwise it
/// holds at either end.
fn phase_at(time: f64, duration: f64, looping: bool) -> f64 {
    let phase = time / duration.max(1e-9);
    if !phase.is_finite() {
        return 0.0;
    }
    if looping {
        let through = phase.rem_euclid(1.0);
        if through == 0.0 && phase > 0.0 {
            1.0
        } else {
            through
        }
    } else {
        phase.clamp(0.0, 1.0)
    }
}

/// The events `clip` passes from `from` to `to` seconds of playback.
fn events_between(clip: &Clip, from: f64, to: f64, looping: bool) -> Vec<String> {
    let duration = clip.duration();
    let (mut a, mut b) = (from / duration, to / duration);
    if !looping {
        a = a.clamp(0.0, 1.0);
        b = b.clamp(0.0, 1.0);
        // The very end counts as passed when a clip that doesn't loop stops there.
        if b >= 1.0 {
            b = 1.0 + 1e-9;
        }
    }
    clip.events_between(a, b)
}

/// Moves and turns the MeshPart `part` by root `motion` (in its model's
/// file space): fitted into the model's box, stretched by the part's `Size`,
/// then turned with the part. The turn is around the world's up.
fn move_by_root_motion(world: &mut World, part: InstanceId, rig: &Rig, motion: RootMotion) {
    if motion == RootMotion::default() {
        return;
    }
    let Some(instance) = world.get(part) else {
        return;
    };
    let vector = |name| instance.property(name).and_then(Value::as_vector3);
    let (Some(position), Some(orientation), Some(size)) =
        (vector("Position"), vector("Orientation"), vector("Size"))
    else {
        return;
    };
    // Orientation is degrees, applied around Z, then X, then Y.
    let rotation = Quat::from_euler(
        EulerRot::YXZ,
        orientation.y.to_radians(),
        orientation.x.to_radians(),
        orientation.z.to_radians(),
    );
    let fitted = rig.fit.transform_vector3(motion.travel);
    let moved = rotation * (fitted * Vec3::new(size.x, size.y, size.z));
    if !moved.is_finite() || !motion.yaw.is_finite() {
        return;
    }
    let position = Vector3::new(
        position.x + moved.x,
        position.y + moved.y,
        position.z + moved.z,
    );
    let turned = Vector3::new(
        orientation.x,
        orientation.y + motion.yaw.to_degrees(),
        orientation.z,
    );
    let _ = world.set_property(part, "Position", Value::Vector3(position));
    if motion.yaw != 0.0 {
        let _ = world.set_property(part, "Orientation", Value::Vector3(turned));
    }
}

impl RunningMachine {
    /// Whether its file, or a clip one of its states plays, is one that
    /// `is` picks.
    fn uses(&self, is: impl Fn(&str) -> bool) -> bool {
        let file = &self.compiled.file;
        is(&self.path)
            || file
                .states
                .iter()
                .chain(file.layers.iter().flat_map(|layer| &layer.states))
                .any(|state| state.motion.clips().into_iter().any(|clip| is(clip)))
    }

    /// Its path, and the name of the state playing and the point in it.
    fn where_it_is(self) -> (String, String, f64) {
        let current = self.machine.current;
        let name = self.compiled.state_name(current.state).to_owned();
        (self.path, name, current.phase)
    }
}

/// True for a `.gltf`, `.glb` or `.fbx` path.
fn is_model_file(path: &str) -> bool {
    ModelFormat::of(std::path::Path::new(path)).is_some()
}

/// Whether a change to the file at `path` can change how any clip binds:
/// a model file (its takes and skeleton), a rig definition (its bone map,
/// for retargeting) or a retarget profile.
fn rebinds_everything(path: &str) -> bool {
    is_model_file(path)
        || axcenity_assets::is_rig_definition(path)
        || crate::retarget::is_retarget_profile(path)
}

/// Reads the state machine file at `path`.
pub fn read_state_machine(assets: &Assets, path: &str) -> Result<StateMachineFile, String> {
    let bytes = assets.read_file(path)?;
    let text = String::from_utf8(bytes).map_err(|_| "it isn't text".to_owned())?;
    StateMachineFile::from_json(&text).map_err(|err| err.0)
}

/// A clip file read with what binding it needs: the takes of the model
/// file it's linked to (`None` if it isn't linked, or that file can't be
/// read), the project's rig definitions, and its link's retarget profile.
#[derive(Clone, Debug)]
pub struct LoadedClip {
    pub file: ClipFile,
    pub takes: Option<Arc<Vec<Take>>>,
    pub rigs: Arc<RigDefinitions>,
    pub profile: Option<Arc<RetargetProfile>>,
}

impl LoadedClip {
    /// A clip read from somewhere else (a test, an editor's open document),
    /// with no rig definitions or profile.
    pub fn new(file: ClipFile, takes: Option<Arc<Vec<Take>>>) -> Self {
        Self {
            file,
            takes,
            rigs: Arc::default(),
            profile: None,
        }
    }

    /// Binds it to `rig` ([`Clip::bind_with`]).
    pub fn bind(self, rig: Option<&Rig>) -> Result<Clip, String> {
        Clip::bind_with(
            self.file,
            self.takes,
            rig,
            &self.rigs,
            self.profile.as_deref(),
        )
    }
}

/// Reads the clip file at `path`, the takes of the model file it's
/// linked to, the project's rig definitions, and its retarget profile.
pub fn load_clip(assets: &Assets, path: &str) -> Result<LoadedClip, String> {
    let bytes = assets.read_file(path)?;
    let text = String::from_utf8(bytes).map_err(|_| "it isn't text".to_owned())?;
    let file = ClipFile::from_json(&text).map_err(|err| err.0)?;
    let takes = file
        .link
        .as_ref()
        .and_then(|link| assets.takes(&link.source));
    let profile = match file.link.as_ref().and_then(|link| link.retarget.as_deref()) {
        Some(path) => Some(Arc::new(read_retarget_profile(assets, path).map_err(
            |why| format!("its retarget profile `{path}` can't be read: {why}"),
        )?)),
        None => None,
    };
    Ok(LoadedClip {
        file,
        takes,
        rigs: assets.rig_definitions(),
        profile,
    })
}

/// Reads the retarget profile file at `path`.
pub fn read_retarget_profile(assets: &Assets, path: &str) -> Result<RetargetProfile, String> {
    let bytes = assets.read_file(path)?;
    let text = String::from_utf8(bytes).map_err(|_| "it isn't text".to_owned())?;
    RetargetProfile::from_json(&text)
}
