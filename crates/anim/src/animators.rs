//! Running every Animator in a world, owned by the host (the player, or
//! the editor's Play session) like its physics.
//!
//! Each animated thing (the parent of an Animator, or of Animations) has
//! one runner. Each frame it plays, in this order, so later ones win where
//! two animate the same thing:
//! 1. the Animator's state machine, if it has one;
//! 2. what plays on top, oldest start first: AnimationTracks from
//!    `animator:LoadAnimation`, by their weight (fading in and out), and
//!    Animations played with `animation:Play()` (or `AutoPlay`).
//!
//! On a MeshPart with a skeleton they pose its bones; on anything they set
//! the properties their tracks key. Properties keep the value last given
//! when nothing plays them any more; bones go back to what plays under them.
//!
//! A clip, state machine or model file that changed on disk
//! ([`Animators::reload`]) is bound again at the next update, and each
//! Animator using it stays in the state it was in, matched by name.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use axcenity_assets::{Assets, Model, ModelFormat, Posed, Rig, Take};
use axcenity_core::{ClassName, InstanceId, Value, Vector3, World};
use glam::{EulerRot, Quat, Vec3};

use crate::clip::{Clip, RootMotion};
use crate::controller::{Compiled, Machine, Playing};
use crate::files::{
    CLIP_EXTENSION, ClipFile, DEFAULT_BLEND, STATE_MACHINE_EXTENSION, StateMachineFile,
};
use crate::property_clip::{Applied, apply_evaluated, blend_evaluated};

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
    /// An Animation's `AnimationEvent`, from `animation:Play()`.
    AnimationMarker { animation: InstanceId, name: String },
    /// An Animation's `Completed`: `animation:Play()` reached the end of a
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
}

/// An Animator's state machine and where it is in it.
struct RunningMachine {
    /// The `StateMachine` it was made for; a change starts it again.
    path: String,
    compiled: Arc<Compiled>,
    machine: Machine,
}

/// Something playing over the state machine this frame.
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
}

impl Animators {
    pub fn new() -> Self {
        Self::default()
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
            || is_model_file(file)
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
            if !is_model_file(path) && !self.plays(path) {
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
        for id in world.descendants(world.workspace()) {
            let Some(instance) = world.get(id) else {
                continue;
            };
            let class = instance.class();
            if class != ClassName::Animator && class != ClassName::Animation {
                continue;
            }
            let Some(mut owner) = instance.parent() else {
                continue;
            };
            // An Animation under an Animator animates the Animator's parent.
            if class == ClassName::Animation
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
            if class == ClassName::Animator {
                parts.animator.get_or_insert(id);
            } else {
                parts.animations.push(id);
            }
        }
        let alive: HashSet<InstanceId> = owners.iter().map(|(owner, _)| *owner).collect();
        self.running.retain(|owner, _| alive.contains(owner));
        let changed = self.readable_changes(assets);

        let mut events = Vec::new();
        let mut posed = HashSet::new();
        for (owner, parts) in owners {
            if self.update_one(world, assets, owner, &parts, &changed, dt, &mut events) {
                posed.insert(owner);
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
    ) -> bool {
        let model = skeleton_model(world, assets, owner);
        let rig = model.as_ref().and_then(|model| model.rig.clone());
        let model_key = model.as_ref().map(|model| model.key());
        let (enabled, speed) = parts.animator.map_or((true, 1.0), |id| settings(world, id));
        let dt = if enabled { dt * speed } else { 0.0 };

        let mut running = self.running.remove(&owner).unwrap_or_default();
        // A machine made again (for a new model or changed files) goes on
        // from the state it was in.
        let mut was_in = None;
        if running.model_key != model_key {
            running.model_key = model_key;
            running.clips.clear();
            was_in = running.machine.take().map(RunningMachine::where_it_is);
        }
        if !changed.is_empty() {
            // A changed model file has new takes for its linked clips.
            let models = changed.iter().any(|path| is_model_file(path));
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
                dt,
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
                    dt,
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

        // Bones: the state machine's pose, then each layer over it.
        let mut posed = false;
        if let (Some(model), Some(rig)) = (&model, &rig) {
            let bone_layers: Vec<&Layer> = layers
                .iter()
                .filter(|layer| layer.clip.animated_bones().iter().any(|&moved| moved))
                .collect();
            if running.machine.is_some() || !bone_layers.is_empty() {
                let mut pose = match (&running.machine, parts.animator) {
                    (Some(machine), Some(animator)) => {
                        let parameters = world
                            .animator(animator)
                            .map(|live| live.parameters.clone())
                            .unwrap_or_default();
                        machine.machine.pose(&machine.compiled, rig, &parameters)
                    }
                    _ => rig.rest_pose(),
                };
                for layer in bone_layers {
                    let mut over = rig.rest_pose();
                    layer.clip.sample(layer.phase, rig, &mut over);
                    for (bone, moved) in layer.clip.animated_bones().iter().enumerate() {
                        if *moved
                            && let (Some(under), Some(over)) = (pose.get_mut(bone), over.get(bone))
                        {
                            *under = under.lerp(over, layer.weight);
                        }
                    }
                }
                if let Some(pose) = model.pose(&pose) {
                    self.poses.insert(owner, Arc::new(pose));
                    posed = true;
                }
            }
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
            // Transitions whose time has come, now the clock has moved on.
            if let Some(live) = world.animator_mut(animator) {
                running.machine.evaluate(&compiled, &mut live.parameters);
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

    /// Takes `animation:Play()`, `Stop`, `Pause` and `Resume` and moves its
    /// playback on. A layer once it has been played (it keeps its last
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
        dt: f64,
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
        let requested = world.property_animation(animation).is_some_and(|state| {
            state.play || state.stop || state.pause || state.resume || state.order > 0
        });
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
            state.paused = false;
            state.time = 0.0;
        }
        if std::mem::take(&mut state.pause) && state.playing {
            state.playing = false;
            state.paused = true;
        }
        if std::mem::take(&mut state.resume) && state.paused {
            state.playing = true;
            state.paused = false;
        }
        if std::mem::take(&mut state.play) {
            state.playing = true;
            state.paused = false;
            state.time = 0.0;
            state.order = order.unwrap_or(state.order.max(1));
            running.seen.remove(&animation);
        }
        let from = running.seen.get(&animation).copied().unwrap_or(state.time);
        let duration = clip.duration();
        let mut completed = false;
        if enabled && state.playing {
            let mut next = state.time + dt * speed;
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
        dt: f64,
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
        if dt > 0.0 {
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
            .and_then(|(file, takes)| Clip::bind(file, takes, rig))
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

    fn report(&mut self, message: String) {
        if self.reported.insert(message.clone()) {
            self.errors.push(message);
        }
    }
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

/// The model of `owner` if it's a MeshPart whose model has a skeleton.
fn skeleton_model(world: &World, assets: &Assets, owner: InstanceId) -> Option<Arc<Model>> {
    let part = world.get(owner)?;
    if part.class() != ClassName::MeshPart {
        return None;
    }
    let mesh_id = part.property("MeshId").and_then(Value::as_str)?;
    let model = assets.model(mesh_id)?;
    model.rig.is_some().then_some(model)
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
        is(&self.path)
            || self
                .compiled
                .file
                .states
                .iter()
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

/// Reads the state machine file at `path`.
fn read_state_machine(assets: &Assets, path: &str) -> Result<StateMachineFile, String> {
    let bytes = assets.read_file(path)?;
    let text = String::from_utf8(bytes).map_err(|_| "it isn't text".to_owned())?;
    StateMachineFile::from_json(&text).map_err(|err| err.0)
}

/// A clip file and the takes of the model file it's linked to (`None` if it
/// isn't linked, or that file can't be read).
pub type LoadedClip = (ClipFile, Option<Arc<Vec<Take>>>);

/// Reads the clip file at `path`, and the takes of the model file it's
/// linked to.
pub fn load_clip(assets: &Assets, path: &str) -> Result<LoadedClip, String> {
    let bytes = assets.read_file(path)?;
    let text = String::from_utf8(bytes).map_err(|_| "it isn't text".to_owned())?;
    let clip = ClipFile::from_json(&text).map_err(|err| err.0)?;
    let takes = clip
        .link
        .as_ref()
        .and_then(|link| assets.takes(&link.source));
    Ok((clip, takes))
}
