//! A controller file made ready to run, and the state machine that runs it.

use std::sync::Arc;

use axcenity_assets::{Pose, Rig, Take};

use crate::clip::BoundClip;
use crate::files::{ClipFile, ControllerFile, Motion};

/// A controller with its clips bound to one rig.
#[derive(Debug)]
pub struct Compiled {
    pub file: ControllerFile,
    states: Vec<CompiledState>,
    start: usize,
}

#[derive(Debug)]
struct CompiledState {
    name: String,
    speed: f64,
    /// `None` when its clip couldn't be loaded: it holds the rest pose.
    clip: Option<BoundClip>,
}

impl Compiled {
    /// Binds every state's clip to `rig`. `load_clip` reads a clip file and
    /// the takes of the model file it names. A clip that can't be loaded
    /// leaves its state still, and its reason is in the returned list.
    pub fn new(
        file: ControllerFile,
        rig: &Rig,
        mut load_clip: impl FnMut(&str) -> Result<(ClipFile, Arc<Vec<Take>>), String>,
    ) -> (Self, Vec<String>) {
        let mut problems = Vec::new();
        let states = file
            .states
            .iter()
            .map(|state| {
                let clip = match &state.motion {
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
        let start = file
            .starting_state()
            .and_then(|start| file.states.iter().position(|s| s.name == start.name))
            .unwrap_or(0);
        (
            Self {
                file,
                states,
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

    /// Poses `pose` (already at rest) as `state` is at `phase`.
    pub fn sample(&self, state: usize, phase: f64, rig: &Rig, pose: &mut Pose) {
        if let Some(clip) = self.states.get(state).and_then(|s| s.clip.as_ref()) {
            clip.sample(phase, rig, pose);
        }
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
    /// `blend` seconds. Blending from a blend starts from whichever of its
    /// two states counts more.
    pub fn play(&mut self, state: usize, blend: f64) {
        let from = match self.fade {
            Some(fade) if fade.weight() < 0.5 => fade.from,
            _ => self.current,
        };
        self.current = Playing { state, phase: 0.0 };
        self.fade = (blend > 0.0).then_some(Fade {
            from,
            elapsed: 0.0,
            duration: blend,
        });
    }

    /// Moves on by `dt` seconds.
    pub fn advance(&mut self, controller: &Compiled, dt: f64) {
        if !(dt.is_finite() && dt > 0.0) {
            return;
        }
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
        let mut pose = rig.rest_pose();
        controller.sample(self.current.state, self.current.phase, rig, &mut pose);
        if let Some(fade) = &self.fade {
            let mut old = rig.rest_pose();
            controller.sample(fade.from.state, fade.from.phase, rig, &mut old);
            let weight = fade.weight() as f32;
            for (new, old) in pose.iter_mut().zip(&old) {
                *new = old.lerp(new, weight);
            }
        }
        pose
    }
}
