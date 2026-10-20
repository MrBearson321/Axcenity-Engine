//! A controller file made ready to run, and the state machine that runs it.

use std::collections::BTreeMap;
use std::sync::Arc;

use axcenity_assets::{Pose, Rig, Take};
use axcenity_core::Parameter;

use crate::clip::{BoundClip, RootMotion};
use crate::files::{ANY_STATE, ClipFile, Condition, ControllerFile, Motion, Test};

/// A controller with its clips bound to one rig.
#[derive(Debug)]
pub struct Compiled {
    pub file: ControllerFile,
    states: Vec<CompiledState>,
    transitions: Vec<CompiledTransition>,
    start: usize,
}

#[derive(Debug)]
struct CompiledState {
    name: String,
    speed: f64,
    /// `None` when it has no clip, or its clip couldn't be loaded: it holds
    /// the rest pose.
    clip: Option<BoundClip>,
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
    /// Binds every state's clip to `rig`. `load_clip` reads a clip file and
    /// the takes of the model file it names. A clip that can't be loaded
    /// leaves its state still, and its reason (and any transition that can't
    /// work) is in the returned list.
    pub fn new(
        file: ControllerFile,
        rig: &Rig,
        mut load_clip: impl FnMut(&str) -> Result<(ClipFile, Arc<Vec<Take>>), String>,
    ) -> (Self, Vec<String>) {
        let mut problems = Vec::new();
        let states: Vec<CompiledState> = file
            .states
            .iter()
            .map(|state| {
                let clip = match &state.motion {
                    Motion::Clip(path) if path.is_empty() => None,
                    Motion::Clip(path) => match load_clip(path)
                        .and_then(|(clip, takes)| BoundClip::bind(clip, takes, rig))
                    {
                        Ok(clip) => Some(clip),
                        Err(problem) => {
                            problems.push(format!(
                                "the state `{}` can't play `{path}`: {problem}",
                                state.name
                            ));
                            None
                        }
                    },
                };
                CompiledState {
                    name: state.name.clone(),
                    speed: if state.speed.is_finite() {
                        state.speed
                    } else {
                        1.0
                    },
                    clip,
                }
            })
            .collect();
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
        (
            Self {
                file,
                states,
                transitions,
                start,
            },
            problems,
        )
    }

    pub fn state_count(&self) -> usize {
        self.states.len()
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

    /// Seconds one pass of `state`'s motion lasts at its speed (and the
    /// state's).
    pub fn duration(&self, state: usize) -> f64 {
        let Some(compiled) = self.states.get(state) else {
            return 1.0;
        };
        let speed = compiled.speed.abs().max(1e-3);
        compiled.clip.as_ref().map_or(1.0, BoundClip::duration) / speed
    }

    /// Whether `state`'s motion loops (a state with no clip counts as looping).
    pub fn looping(&self, state: usize) -> bool {
        self.clip(state).is_none_or(|clip| clip.file.looping)
    }

    /// Poses `pose` (already at rest) as `state` is at `phase`.
    pub fn sample(&self, state: usize, phase: f64, rig: &Rig, pose: &mut Pose) {
        if let Some(clip) = self.clip(state) {
            clip.sample(phase, rig, pose);
        }
    }

    /// `state`'s clip, if it has one that loaded.
    pub fn clip(&self, state: usize) -> Option<&BoundClip> {
        self.states.get(state).and_then(|s| s.clip.as_ref())
    }

    /// How far `state`'s root motion moves the model from phase `from` to `to`.
    pub fn root_motion(&self, state: usize, rig: &Rig, from: f64, to: f64) -> RootMotion {
        self.clip(state)
            .map_or_else(RootMotion::default, |clip| clip.root_motion(rig, from, to))
    }

    /// The events `state`'s clip passes from phase `from` to `to`.
    pub fn events(&self, state: usize, from: f64, to: f64) -> Vec<String> {
        self.clip(state)
            .map_or_else(Vec::new, |clip| clip.events_between(from, to))
    }
}

/// Whether `condition` holds for `parameters`. A parameter that isn't there,
/// or is of another type than its test is for, doesn't hold.
pub fn holds(condition: &Condition, parameters: &BTreeMap<String, Parameter>) -> bool {
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
        parameters: &mut BTreeMap<String, Parameter>,
    ) -> Option<usize> {
        if self.fade.is_some_and(|fade| !fade.interruptible) {
            return None;
        }
        let current = self.current;
        let chosen = controller
            .transitions
            .iter()
            .filter(|t| match t.from {
                None => t.to != current.state,
                Some(from) => from == current.state,
            })
            .filter(|t| {
                t.exit_time.is_none_or(|exit| {
                    exit_reached(exit, current.phase, controller.looping(current.state))
                })
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
    /// travel, by their share of the blend).
    pub fn advance(&mut self, controller: &Compiled, rig: &Rig, dt: f64) -> Advanced {
        if !(dt.is_finite() && dt > 0.0) {
            return Advanced::default();
        }
        let before = (self.current, self.fade);
        self.step(controller, dt);
        let events = controller.events(before.0.state, before.0.phase, self.current.phase);
        let weight = self.fade.map_or(1.0, |fade| fade.weight()) as f32;
        let mut motion = controller
            .root_motion(before.0.state, rig, before.0.phase, self.current.phase)
            .scaled(weight);
        if let Some(old) = before.1 {
            let to = old.from.phase + dt / controller.duration(old.from.state);
            let from = controller
                .root_motion(old.from.state, rig, old.from.phase, to)
                .scaled(1.0 - weight);
            motion = motion + from;
        }
        Advanced { events, motion }
    }

    /// Moves the phases and the blend on by `dt` seconds.
    fn step(&mut self, controller: &Compiled, dt: f64) {
        self.current.phase += dt / controller.duration(self.current.state);
        if let Some(fade) = &mut self.fade {
            fade.from.phase += dt / controller.duration(fade.from.state);
            fade.elapsed += dt;
            if fade.elapsed >= fade.duration {
                self.fade = None;
            }
        }
    }

    /// The pose now: the current state's, blended with the old one's while
    /// fading.
    pub fn pose(&self, controller: &Compiled, rig: &Rig) -> Pose {
        let weight = self.fade.map_or(1.0, |fade| fade.weight());
        self.pose_at(controller, rig, weight)
    }

    /// The pose with the fade's new state counting `weight` (0 is all the
    /// old state, 1 all the new), for scrubbing a blend in the editor.
    pub fn pose_at(&self, controller: &Compiled, rig: &Rig, weight: f64) -> Pose {
        let mut pose = rig.rest_pose();
        controller.sample(self.current.state, self.current.phase, rig, &mut pose);
        if let Some(fade) = &self.fade {
            let mut old = rig.rest_pose();
            controller.sample(fade.from.state, fade.from.phase, rig, &mut old);
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
