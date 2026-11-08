//! A state machine file made ready to run, and the [`Machine`] that runs it.

use std::collections::BTreeMap;

use axcenity_assets::{Pose, Rig};
use axcenity_core::Parameter;

use crate::animators::LoadedClip;
use crate::blend_space::blend_space_weights;
use crate::clip::{Clip, RootMotion};
use crate::files::{
    ANY_STATE, Condition, LayerMode, Motion, ParameterDef, StateMachineFile, Test, blend_weights,
    with_returns,
};
use crate::property_clip::EvaluatedProperty;

/// An animator's parameters, by name.
pub type Parameters = BTreeMap<String, Parameter>;

/// A state machine with its clips made ready (bound to a rig, if it has one).
#[derive(Debug)]
pub struct Compiled {
    pub file: StateMachineFile,
    states: Vec<CompiledState>,
    transitions: Vec<CompiledTransition>,
    start: usize,
    layers: Vec<CompiledLayer>,
}

/// A layer over the base layer (Phase 78), made ready: its own states, as
/// a state machine of their own, and its mask by bone.
#[derive(Debug)]
pub struct CompiledLayer {
    pub name: String,
    /// Its weight when the game starts, from 0 to 1.
    pub weight: f64,
    pub mode: LayerMode,
    /// Whether it may move each bone of the rig; empty is every bone.
    pub mask: Vec<bool>,
    pub machine: Compiled,
}

impl CompiledLayer {
    /// How much its mask lets it move `bone`: 1 or 0.
    pub fn masks(&self, bone: usize) -> f32 {
        if self.mask.is_empty() || self.mask.get(bone).copied().unwrap_or(false) {
            1.0
        } else {
            0.0
        }
    }

    /// Mixes the layer's pose `over` (covering each bone by `cover`, from
    /// [`Machine::layer_pose`]) into the pose of the layers `under` it, by
    /// `weight`, on the bones its mask picks. Override moves each bone
    /// toward the layer's; Additive adds how far the layer's pose moves the
    /// bone from the rig's rest pose.
    pub fn mix(&self, under: &mut Pose, over: &Pose, cover: &[f32], weight: f32, rig: &Rig) {
        let weight = if weight.is_finite() {
            weight.clamp(0.0, 1.0)
        } else {
            0.0
        };
        for (bone, (under, over)) in under.iter_mut().zip(over).enumerate() {
            let amount = weight * cover.get(bone).copied().unwrap_or(0.0) * self.masks(bone);
            if amount <= 0.0 {
                continue;
            }
            match self.mode {
                LayerMode::Override => *under = under.lerp(over, amount),
                LayerMode::Additive => {
                    let Some(rest) = rig.bones.get(bone).map(|b| b.rest) else {
                        continue;
                    };
                    let turn = rest.rotation.inverse() * over.rotation;
                    let stretch = glam::Vec3::select(
                        rest.scale.abs().cmpgt(glam::Vec3::splat(1e-6)),
                        over.scale / rest.scale,
                        glam::Vec3::ONE,
                    );
                    under.translation += (over.translation - rest.translation) * amount;
                    under.rotation =
                        (under.rotation * glam::Quat::IDENTITY.slerp(turn, amount)).normalize();
                    under.scale *= glam::Vec3::ONE.lerp(stretch, amount);
                }
            }
        }
    }
}

#[derive(Debug)]
struct CompiledState {
    name: String,
    speed: f64,
    motion: CompiledMotion,
}

/// What a state plays, with its clips bound. A clip that's `None` has no
/// path or couldn't be loaded.
#[derive(Debug)]
enum CompiledMotion {
    /// Boxed, as a clip is much bigger than a blend tree's list.
    Clip(Option<Box<Clip>>),
    Tree {
        parameter: String,
        /// Each child's threshold and clip.
        children: Vec<(f64, Option<Clip>)>,
    },
    Space {
        parameters: [String; 2],
        /// Each child's point and clip.
        children: Vec<([f64; 2], Option<Clip>)>,
    },
}

/// A Number parameter's value, 0 if it isn't one.
fn number(parameters: &Parameters, name: &str) -> f64 {
    match parameters.get(name) {
        Some(Parameter::Number(value)) => *value,
        _ => 0.0,
    }
}

/// The clips with a share above 0, their shares made to add up to 1 again
/// (a clip that didn't load gives its share to the others).
fn shared<'a>(children: impl Iterator<Item = (&'a Option<Clip>, f64)>) -> Vec<(&'a Clip, f64)> {
    let playing: Vec<(&Clip, f64)> = children
        .filter(|(_, weight)| *weight > 0.0)
        .filter_map(|(clip, weight)| Some((clip.as_ref()?, weight)))
        .collect();
    let total: f64 = playing.iter().map(|(_, weight)| weight).sum();
    playing
        .into_iter()
        .map(|(clip, weight)| (clip, weight / total.max(1e-12)))
        .collect()
}

impl CompiledState {
    /// The clips playing now, each with its share (adding up to 1), by
    /// `parameters` for a blend tree. Empty for a state with nothing to
    /// play: it holds the rest pose.
    fn playing(&self, parameters: &Parameters) -> Vec<(&Clip, f64)> {
        match &self.motion {
            CompiledMotion::Clip(Some(clip)) => vec![(&**clip, 1.0)],
            CompiledMotion::Clip(None) => Vec::new(),
            CompiledMotion::Tree { children, .. } => {
                let weights = self.weights(parameters).unwrap_or_default();
                shared(children.iter().map(|(_, clip)| clip).zip(weights))
            }
            CompiledMotion::Space { children, .. } => {
                let weights = self.weights(parameters).unwrap_or_default();
                shared(children.iter().map(|(_, clip)| clip).zip(weights))
            }
        }
    }

    /// Each child's share by `parameters`, in the file's order, before
    /// shares of clips that didn't load move to the others. `None` for a
    /// single clip.
    fn weights(&self, parameters: &Parameters) -> Option<Vec<f64>> {
        match &self.motion {
            CompiledMotion::Clip(_) => None,
            CompiledMotion::Tree {
                parameter,
                children,
            } => {
                let thresholds: Vec<f64> = children.iter().map(|(t, _)| *t).collect();
                Some(blend_weights(&thresholds, number(parameters, parameter)))
            }
            CompiledMotion::Space {
                parameters: [x, y],
                children,
            } => {
                let points: Vec<[f64; 2]> = children.iter().map(|(p, _)| *p).collect();
                let value = [number(parameters, x), number(parameters, y)];
                Some(blend_space_weights(&points, value))
            }
        }
    }
}

/// A transition with its states found.
#[derive(Clone, Debug)]
struct CompiledTransition {
    /// Its place in the file's list, which breaks ties of priority.
    index: usize,
    /// `None` for Any State.
    from: Option<usize>,
    to: usize,
    conditions: Vec<Condition>,
    blend: f64,
    exit_time: Option<f64>,
    interruptible: bool,
    priority: i32,
}

impl Compiled {
    /// Makes every state's clips ready, bound to `rig` if there is one.
    /// `load_clip` reads a clip file and the takes of the model file it's
    /// linked to (`None` if it isn't). A clip that can't be loaded
    /// leaves its state (or its part of a blend tree) still, and its reason
    /// (and any transition that can't work) is in the returned list.
    pub fn new(
        file: StateMachineFile,
        rig: Option<&Rig>,
        mut load_clip: impl FnMut(&str) -> Result<LoadedClip, String>,
    ) -> (Self, Vec<String>) {
        Self::compile(file, rig, &mut load_clip)
    }

    /// [`new`](Self::new) through a `dyn` loader, so each layer compiles
    /// with the same one (a generic loader would recurse in its type).
    fn compile(
        file: StateMachineFile,
        rig: Option<&Rig>,
        load_clip: &mut dyn FnMut(&str) -> Result<LoadedClip, String>,
    ) -> (Self, Vec<String>) {
        let mut problems = Vec::new();
        let mut bind = |state: &str, path: &str, problems: &mut Vec<String>| {
            if path.is_empty() {
                return None;
            }
            match load_clip(path).and_then(|(clip, takes)| Clip::bind(clip, takes, rig)) {
                Ok(clip) => Some(clip),
                Err(problem) => {
                    problems.push(format!(
                        "the state `{state}` can't play `{path}`: {problem}"
                    ));
                    None
                }
            }
        };
        let states: Vec<CompiledState> = file
            .states
            .iter()
            .map(|state| {
                let motion = match &state.motion {
                    Motion::Clip(path) => {
                        CompiledMotion::Clip(bind(&state.name, path, &mut problems).map(Box::new))
                    }
                    Motion::BlendTree(tree) => {
                        if !is_number(&file.parameters, &tree.parameter) {
                            problems.push(format!(
                                "the blend tree of `{}` needs a Number parameter, and `{}` isn't one",
                                state.name, tree.parameter
                            ));
                        }
                        CompiledMotion::Tree {
                            parameter: tree.parameter.clone(),
                            children: tree
                                .children
                                .iter()
                                .map(|child| {
                                    let threshold =
                                        if child.threshold.is_finite() { child.threshold } else { 0.0 };
                                    (threshold, bind(&state.name, &child.clip, &mut problems))
                                })
                                .collect(),
                        }
                    }
                    Motion::BlendSpace(space) => {
                        for parameter in [&space.parameter_x, &space.parameter_y] {
                            if !is_number(&file.parameters, parameter) {
                                problems.push(format!(
                                    "the blend space of `{}` needs Number parameters, and `{parameter}` isn't one",
                                    state.name
                                ));
                            }
                        }
                        CompiledMotion::Space {
                            parameters: [space.parameter_x.clone(), space.parameter_y.clone()],
                            children: space
                                .children
                                .iter()
                                .map(|child| {
                                    let point =
                                        child.position.map(|v| if v.is_finite() { v } else { 0.0 });
                                    (point, bind(&state.name, &child.clip, &mut problems))
                                })
                                .collect(),
                        }
                    }
                };
                CompiledState {
                    name: state.name.clone(),
                    speed: if state.speed.is_finite() {
                        state.speed
                    } else {
                        1.0
                    },
                    motion,
                }
            })
            .collect();
        // Return settings make their transitions here too, so a file edited
        // by hand works the same as one saved by the editor.
        let no_parameters = Parameters::new();
        let file = with_returns(&file, |def| {
            let state = states.iter().find(|state| state.name == def.name)?;
            let playing = state.playing(&no_parameters);
            (!playing.is_empty()).then(|| {
                let length: f64 = playing.iter().map(|(clip, w)| clip.duration() * w).sum();
                length / state.speed.abs().max(1e-3)
            })
        });
        let index_of = |name: &str| states.iter().position(|state| state.name == name);
        let mut transitions = Vec::new();
        for (index, transition) in file.transitions.iter().enumerate() {
            let from = if transition.from == ANY_STATE {
                Some(None)
            } else {
                index_of(&transition.from).map(Some)
            };
            let (Some(from), Some(to)) = (from, index_of(&transition.to)) else {
                problems.push(format!(
                    "the transition from `{}` to `{}` names a state it doesn't have",
                    transition.from, transition.to
                ));
                continue;
            };
            if transition.conditions.is_empty() && transition.exit_time.is_none() {
                problems.push(format!(
                    "the transition from `{}` to `{}` never happens: give it a condition or an exit time",
                    transition.from, transition.to
                ));
                continue;
            }
            transitions.push(CompiledTransition {
                index,
                from,
                to,
                conditions: transition.conditions.clone(),
                blend: if transition.blend.is_finite() {
                    transition.blend.max(0.0)
                } else {
                    0.0
                },
                exit_time: transition.exit_time.filter(|time| time.is_finite()),
                interruptible: transition.interruptible,
                priority: transition.priority,
            });
        }
        let start = file
            .starting_state()
            .and_then(|start| file.states.iter().position(|s| s.name == start.name))
            .unwrap_or(0);
        let mut layers = Vec::new();
        for layer in &file.layers {
            let (machine, layer_problems) =
                Self::compile(layer.machine(&file.parameters), rig, load_clip);
            problems.extend(
                layer_problems
                    .into_iter()
                    .map(|problem| format!("the layer `{}`: {problem}", layer.name)),
            );
            let mask = match rig {
                Some(rig) if !layer.mask.is_empty() => {
                    let (mask, missing) = bone_mask(rig, &layer.mask);
                    problems.extend(missing.into_iter().map(|path| {
                        format!(
                            "the layer `{}`'s mask names the bone `{path}`, which the skeleton doesn't have",
                            layer.name
                        )
                    }));
                    mask
                }
                _ => Vec::new(),
            };
            layers.push(CompiledLayer {
                name: layer.name.clone(),
                weight: if layer.weight.is_finite() {
                    layer.weight.clamp(0.0, 1.0)
                } else {
                    1.0
                },
                mode: layer.mode,
                mask,
                machine,
            });
        }
        (
            Self {
                file,
                states,
                transitions,
                start,
                layers,
            },
            problems,
        )
    }

    /// Its layers over the base layer, in the file's order (the last on top).
    pub fn layers(&self) -> &[CompiledLayer] {
        &self.layers
    }

    /// Whether any clip `state` plays now moves each of the rig's `bones`.
    pub fn animated(&self, state: usize, parameters: &Parameters, bones: usize) -> Vec<bool> {
        let mut animated = vec![false; bones];
        for (clip, _) in self.playing(state, parameters) {
            for (bone, moved) in clip.animated_bones().iter().enumerate() {
                if *moved && let Some(slot) = animated.get_mut(bone) {
                    *slot = true;
                }
            }
        }
        animated
    }

    /// The state named `name`'s index.
    pub fn state_index(&self, name: &str) -> Option<usize> {
        self.states.iter().position(|state| state.name == name)
    }

    pub fn state_name(&self, state: usize) -> &str {
        self.states.get(state).map_or("", |s| s.name.as_str())
    }

    /// The state it starts in.
    pub fn start(&self) -> usize {
        self.start
    }

    /// The clips `state` plays now, with their shares.
    fn playing(&self, state: usize, parameters: &Parameters) -> Vec<(&Clip, f64)> {
        self.states
            .get(state)
            .map_or_else(Vec::new, |s| s.playing(parameters))
    }

    /// The clip counting most in `state` now.
    fn main_clip(&self, state: usize, parameters: &Parameters) -> Option<&Clip> {
        self.playing(state, parameters)
            .into_iter()
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(clip, _)| clip)
    }

    /// Seconds one pass of `state`'s motion lasts at its speed (and the
    /// state's): a blend tree's is its clips' lengths, by their shares, so
    /// their cycles stay in step. A state with nothing to play lasts a second.
    pub fn duration(&self, state: usize, parameters: &Parameters) -> f64 {
        let Some(compiled) = self.states.get(state) else {
            return 1.0;
        };
        let speed = compiled.speed.abs().max(1e-3);
        let playing = compiled.playing(parameters);
        let length = if playing.is_empty() {
            1.0
        } else {
            playing
                .iter()
                .map(|(clip, weight)| clip.duration() * weight)
                .sum()
        };
        length.max(1.0 / 60.0) / speed
    }

    /// Whether `state`'s motion loops (its main clip's; a state with nothing
    /// to play counts as looping).
    pub fn looping(&self, state: usize, parameters: &Parameters) -> bool {
        self.main_clip(state, parameters).is_none_or(Clip::looping)
    }

    /// Poses `pose` (already at rest) as `state` is at `phase`: a blend
    /// tree samples each of its clips at the same phase and mixes them by
    /// their shares.
    pub fn sample(
        &self,
        state: usize,
        phase: f64,
        rig: &Rig,
        parameters: &Parameters,
        pose: &mut Pose,
    ) {
        let playing = self.playing(state, parameters);
        match playing.as_slice() {
            [] => {}
            [(clip, _)] => clip.sample(phase, rig, pose),
            [(first, first_weight), rest @ ..] => {
                first.sample(phase, rig, pose);
                let mut counted = *first_weight;
                for (clip, weight) in rest {
                    let mut other = rig.rest_pose();
                    clip.sample(phase, rig, &mut other);
                    counted += weight;
                    let share = (weight / counted.max(1e-12)) as f32;
                    for (bone, other) in pose.iter_mut().zip(&other) {
                        *bone = bone.lerp(other, share);
                    }
                }
            }
        }
    }

    /// `state`'s clip, if it plays one clip that loaded (not a blend tree
    /// or space).
    pub fn clip(&self, state: usize) -> Option<&Clip> {
        match &self.states.get(state)?.motion {
            CompiledMotion::Clip(clip) => clip.as_deref(),
            CompiledMotion::Tree { .. } | CompiledMotion::Space { .. } => None,
        }
    }

    /// How far `state`'s root motion moves the model from phase `from` to
    /// `to`: its clips' travel, by their shares.
    pub fn root_motion(
        &self,
        state: usize,
        rig: &Rig,
        parameters: &Parameters,
        from: f64,
        to: f64,
    ) -> RootMotion {
        self.playing(state, parameters)
            .into_iter()
            .fold(RootMotion::default(), |motion, (clip, weight)| {
                motion + clip.root_motion(rig, from, to).scaled(weight as f32)
            })
    }

    /// The property values `state`'s main clip has at `phase`.
    pub fn properties(
        &self,
        state: usize,
        phase: f64,
        parameters: &Parameters,
    ) -> Vec<EvaluatedProperty> {
        self.main_clip(state, parameters)
            .map_or_else(Vec::new, |clip| clip.properties(phase))
    }

    /// The events `state`'s main clip passes from phase `from` to `to`.
    pub fn events(&self, state: usize, parameters: &Parameters, from: f64, to: f64) -> Vec<String> {
        self.main_clip(state, parameters)
            .map_or_else(Vec::new, |clip| clip.events_between(from, to))
    }

    /// Each child's share in `state`'s blend tree or blend space now, in
    /// the file's order, for the editor. `None` for a single clip.
    pub fn tree_weights(&self, state: usize, parameters: &Parameters) -> Option<Vec<f64>> {
        self.states.get(state)?.weights(parameters)
    }
}

/// Whether `parameters` has a Number named `name`.
fn is_number(parameters: &[ParameterDef], name: &str) -> bool {
    parameters
        .iter()
        .any(|def| def.name() == name && matches!(def, ParameterDef::Number { .. }))
}

/// Which of `rig`'s bones a mask of `paths` picks (each with every bone under
/// it), and the paths it doesn't have.
pub fn bone_mask(rig: &Rig, paths: &[String]) -> (Vec<bool>, Vec<String>) {
    let all = crate::bone_paths(rig);
    let mut listed = vec![false; rig.bones.len()];
    let mut missing = Vec::new();
    for path in paths {
        match crate::find_bone(rig, &all, path) {
            Some(bone) => listed[bone] = true,
            None => missing.push(path.clone()),
        }
    }
    // Bones come parents first, so a bone's parent is decided before it.
    let mut mask = vec![false; rig.bones.len()];
    for (bone, def) in rig.bones.iter().enumerate() {
        mask[bone] = listed[bone] || def.parent.is_some_and(|parent| mask[parent]);
    }
    (mask, missing)
}

/// Whether `condition` holds for `parameters`. A parameter that isn't there,
/// or is of another type than its test is for, doesn't hold.
pub fn holds(condition: &Condition, parameters: &Parameters) -> bool {
    match (condition.test, parameters.get(&condition.parameter)) {
        (Test::IsTrue, Some(Parameter::Bool(value))) => *value,
        (Test::IsFalse, Some(Parameter::Bool(value))) => !*value,
        (Test::Triggered, Some(Parameter::Trigger(set))) => *set,
        (Test::Greater, Some(Parameter::Number(value))) => *value > condition.value,
        (Test::Less, Some(Parameter::Number(value))) => *value < condition.value,
        _ => false,
    }
}

/// A state playing: which one, and how far through its motion it is, in
/// passes (0.5 is halfway through the first, 2 the end of the second).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Playing {
    pub state: usize,
    pub phase: f64,
}

/// A blend from one state to the one now playing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fade {
    pub from: Playing,
    pub elapsed: f64,
    pub duration: f64,
    /// Whether another transition may take over before it ends.
    pub interruptible: bool,
    /// The transition (its index in the file) doing it, or `None` for a
    /// script's `Play`.
    pub transition: Option<usize>,
}

impl Fade {
    /// How much of the pose is the new state's, from 0 to 1.
    pub fn weight(&self) -> f64 {
        if self.duration <= 0.0 {
            1.0
        } else {
            (self.elapsed / self.duration).clamp(0.0, 1.0)
        }
    }
}

/// What happened as a [`Machine`] moved on.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Advanced {
    /// The current state's clip's events passed, in order.
    pub events: Vec<String>,
    pub motion: RootMotion,
}

/// Where an animator is in its controller: the state playing, and the
/// one it's blending from, if any.
#[derive(Clone, Debug, PartialEq)]
pub struct Machine {
    pub current: Playing,
    pub fade: Option<Fade>,
}

impl Machine {
    /// Starts in the controller's starting state.
    pub fn new(controller: &Compiled) -> Self {
        Self {
            current: Playing {
                state: controller.start(),
                phase: 0.0,
            },
            fade: None,
        }
    }

    /// Switches to `state` from its start, blending from the pose now over
    /// `blend` seconds, as a script's `Play` does. A script can always
    /// interrupt, and what it starts can be interrupted.
    pub fn play(&mut self, state: usize, blend: f64) {
        self.start(state, blend, true, None);
    }

    /// Blending from a blend starts from whichever of its two states counts
    /// more.
    fn start(&mut self, state: usize, blend: f64, interruptible: bool, transition: Option<usize>) {
        let from = match self.fade {
            Some(fade) if fade.weight() < 0.5 => fade.from,
            _ => self.current,
        };
        self.current = Playing { state, phase: 0.0 };
        self.fade = (blend > 0.0).then_some(Fade {
            from,
            elapsed: 0.0,
            duration: blend,
            interruptible,
            transition,
        });
    }

    /// Takes the transition whose time has come, if any: from Any State
    /// (never to the state already playing) or from the current state, with
    /// every condition holding and its exit time reached; the highest
    /// priority wins, then the first in the file. A fade that can't be
    /// interrupted is let finish first. The triggers its conditions test are
    /// used up. Returns the transition taken (its index in the file).
    pub fn evaluate(
        &mut self,
        controller: &Compiled,
        parameters: &mut Parameters,
    ) -> Option<usize> {
        if self.fade.is_some_and(|fade| !fade.interruptible) {
            return None;
        }
        let current = self.current;
        let looping = controller.looping(current.state, parameters);
        let chosen = controller
            .transitions
            .iter()
            .filter(|t| match t.from {
                None => t.to != current.state,
                Some(from) => from == current.state,
            })
            .filter(|t| {
                t.exit_time
                    .is_none_or(|exit| exit_reached(exit, current.phase, looping))
            })
            .filter(|t| t.conditions.iter().all(|c| holds(c, parameters)))
            // `max_by_key` keeps the last of equals, so ties go to the
            // earlier transition by comparing indices the other way.
            .max_by_key(|t| (t.priority, std::cmp::Reverse(t.index)))?
            .clone();
        for condition in &chosen.conditions {
            if condition.test == Test::Triggered
                && let Some(Parameter::Trigger(set)) = parameters.get_mut(&condition.parameter)
            {
                *set = false;
            }
        }
        self.start(
            chosen.to,
            chosen.blend,
            chosen.interruptible,
            Some(chosen.index),
        );
        Some(chosen.index)
    }

    /// Moves on by `dt` seconds, returning the events the current state's
    /// clip passed and how far root motion moved the model (both states'
    /// travel, by their share of the blend; none without a `rig`).
    /// `parameters` set blend trees' mixes.
    pub fn advance(
        &mut self,
        controller: &Compiled,
        rig: Option<&Rig>,
        parameters: &Parameters,
        dt: f64,
    ) -> Advanced {
        if !(dt.is_finite() && dt > 0.0) {
            return Advanced::default();
        }
        let before = (self.current, self.fade);
        self.step(controller, parameters, dt);
        let events = controller.events(
            before.0.state,
            parameters,
            before.0.phase,
            self.current.phase,
        );
        let Some(rig) = rig else {
            return Advanced {
                events,
                motion: RootMotion::default(),
            };
        };
        let weight = self.fade.map_or(1.0, |fade| fade.weight()) as f32;
        let mut motion = controller
            .root_motion(
                before.0.state,
                rig,
                parameters,
                before.0.phase,
                self.current.phase,
            )
            .scaled(weight);
        if let Some(old) = before.1 {
            let to = old.from.phase + dt / controller.duration(old.from.state, parameters);
            let from = controller
                .root_motion(old.from.state, rig, parameters, old.from.phase, to)
                .scaled(1.0 - weight);
            motion = motion + from;
        }
        Advanced { events, motion }
    }

    /// Moves the phases and the blend on by `dt` seconds.
    fn step(&mut self, controller: &Compiled, parameters: &Parameters, dt: f64) {
        self.current.phase += dt / controller.duration(self.current.state, parameters);
        if let Some(fade) = &mut self.fade {
            fade.from.phase += dt / controller.duration(fade.from.state, parameters);
            fade.elapsed += dt;
            if fade.elapsed >= fade.duration {
                self.fade = None;
            }
        }
    }

    /// The pose now: the current state's, blended with the old one's while
    /// fading.
    pub fn pose(&self, controller: &Compiled, rig: &Rig, parameters: &Parameters) -> Pose {
        let weight = self.fade.map_or(1.0, |fade| fade.weight());
        self.pose_at(controller, rig, parameters, weight)
    }

    /// A layer's pose now, with how much it covers each bone (0 to 1): the
    /// bones its clips move. Fading between a state that moves a bone and
    /// one that doesn't (such as an empty state, which plays nothing) fades
    /// how much it covers that bone, so the layer under it shows through.
    pub fn layer_pose(
        &self,
        controller: &Compiled,
        rig: &Rig,
        parameters: &Parameters,
    ) -> (Pose, Vec<f32>) {
        let bones = rig.bones.len();
        let sample = |playing: Playing| {
            let mut pose = rig.rest_pose();
            controller.sample(playing.state, playing.phase, rig, parameters, &mut pose);
            let animated = controller.animated(playing.state, parameters, bones);
            (pose, animated)
        };
        let (mut pose, animated) = sample(self.current);
        let Some(fade) = &self.fade else {
            let cover = animated
                .iter()
                .map(|&a| if a { 1.0 } else { 0.0 })
                .collect();
            return (pose, cover);
        };
        let weight = fade.weight() as f32;
        let (old, old_animated) = sample(fade.from);
        let mut cover = vec![0.0; bones];
        for bone in 0..bones {
            match (animated[bone], old_animated[bone]) {
                (true, true) => {
                    pose[bone] = old[bone].lerp(&pose[bone], weight);
                    cover[bone] = 1.0;
                }
                (true, false) => cover[bone] = weight,
                (false, true) => {
                    pose[bone] = old[bone];
                    cover[bone] = 1.0 - weight;
                }
                (false, false) => {}
            }
        }
        (pose, cover)
    }

    /// The pose with the fade's new state counting `weight` (0 is all the
    /// old state, 1 all the new), for scrubbing a blend in the editor.
    pub fn pose_at(
        &self,
        controller: &Compiled,
        rig: &Rig,
        parameters: &Parameters,
        weight: f64,
    ) -> Pose {
        let mut pose = rig.rest_pose();
        controller.sample(
            self.current.state,
            self.current.phase,
            rig,
            parameters,
            &mut pose,
        );
        if let Some(fade) = &self.fade {
            let mut old = rig.rest_pose();
            controller.sample(fade.from.state, fade.from.phase, rig, parameters, &mut old);
            let weight = weight.clamp(0.0, 1.0) as f32;
            for (new, old) in pose.iter_mut().zip(&old) {
                *new = old.lerp(new, weight);
            }
        }
        pose
    }
}

/// Whether a state at `phase` has reached exit time `exit`: in each pass
/// from `exit` to its end for a looping state with an exit time below 1,
/// otherwise once it has played that much in all.
pub fn exit_reached(exit: f64, phase: f64, looping: bool) -> bool {
    if looping && exit < 1.0 {
        phase.rem_euclid(1.0) >= exit.max(0.0)
    } else {
        phase >= exit
    }
}
