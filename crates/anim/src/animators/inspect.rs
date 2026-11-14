//! What an animated thing is doing right now, for the editor's Animator
//! Inspector (AP6): its state machine's state and fade, its layers, what
//! plays over it, its parameters, what it fired lately and its root motion.
//! Read only: looking changes nothing.

use std::collections::BTreeMap;

use axcenity_core::{InstanceId, Parameter, World};
use glam::Vec3;

use super::{AnimatorEvent, Animators, Running};

/// How many fired events each thing remembers.
pub(super) const RECENT: usize = 8;

/// Something a thing fired, and when (by the Animators' clock).
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Fired {
    at: f64,
    what: String,
}

/// A snapshot of one animated thing ([`Animators::inspect`]).
#[derive(Clone, Debug, PartialEq)]
pub struct AnimatorView {
    /// The thing being animated (usually a MeshPart).
    pub owner: InstanceId,
    pub animator: Option<InstanceId>,
    /// Its Animator's state machine, while one is running.
    pub machine: Option<MachineView>,
    /// AnimationTracks and Animations playing over the state machine, in
    /// the order they're applied (the last wins).
    pub playing: Vec<PlayingView>,
    /// The Animator's parameters as scripts and transitions left them.
    pub parameters: BTreeMap<String, Parameter>,
    /// What it fired lately, newest first.
    pub fired: Vec<FiredView>,
    /// Whether its skeleton was posed at the last update.
    pub posed: bool,
}

/// Where a state machine is.
#[derive(Clone, Debug, PartialEq)]
pub struct MachineView {
    /// The `*.statemachine.json` file.
    pub path: String,
    pub state: String,
    /// How far through the state's clip, from 0 to 1 (a loop's later passes
    /// start again at 0).
    pub progress: f64,
    /// The state's length in seconds, with the parameters as they are.
    pub duration: f64,
    pub fade: Option<FadeView>,
    pub layers: Vec<LayerView>,
    /// How far root motion moved it at the last update, in the skeleton's
    /// space, and how far it turned (radians around up).
    pub root_travel: Vec3,
    pub root_yaw: f32,
}

/// A blend from the state it left.
#[derive(Clone, Debug, PartialEq)]
pub struct FadeView {
    pub from: String,
    /// How far into the new state, from 0 to 1.
    pub weight: f64,
    /// The whole blend, in seconds.
    pub seconds: f64,
}

/// One of a state machine's layers.
#[derive(Clone, Debug, PartialEq)]
pub struct LayerView {
    pub name: String,
    pub state: String,
    pub weight: f64,
}

/// A clip playing over the state machine.
#[derive(Clone, Debug, PartialEq)]
pub struct PlayingView {
    /// The AnimationTrack or Animation playing it.
    pub source: InstanceId,
    pub clip: String,
    pub progress: f64,
    pub weight: f32,
}

/// Something fired lately.
#[derive(Clone, Debug, PartialEq)]
pub struct FiredView {
    pub seconds_ago: f64,
    /// Such as "Entered Walk (from Idle)" or "Event Footstep".
    pub what: String,
}

/// Remembers what `running` just fired (`events`, all its own) at `clock`.
pub(super) fn remember(world: &World, running: &mut Running, events: &[AnimatorEvent], at: f64) {
    for event in events {
        running.recent.push_back(Fired {
            at,
            what: describe(world, event),
        });
    }
    while running.recent.len() > RECENT {
        running.recent.pop_front();
    }
}

/// One line for `event`.
fn describe(world: &World, event: &AnimatorEvent) -> String {
    let name = |id: InstanceId| {
        world
            .get(id)
            .map(|instance| instance.name().to_owned())
            .unwrap_or_default()
    };
    match event {
        AnimatorEvent::StateChanged {
            state, previous, ..
        } if previous.is_empty() => format!("Started in {state}"),
        AnimatorEvent::StateChanged {
            state, previous, ..
        } => format!("Entered {state} (from {previous})"),
        AnimatorEvent::Marker { name: marker, .. } => format!("Event {marker}"),
        AnimatorEvent::AnimationMarker {
            animation,
            name: marker,
        } => format!("Event {marker} ({})", name(*animation)),
        AnimatorEvent::AnimationCompleted { animation } => {
            format!("{} completed", name(*animation))
        }
        AnimatorEvent::TrackMarker {
            track,
            name: marker,
        } => {
            format!("Event {marker} ({})", name(*track))
        }
        AnimatorEvent::TrackStopped { track } => format!("{} stopped", name(*track)),
    }
}

/// How far through one pass: a loop's later passes start again at 0.
fn progress(phase: f64) -> f64 {
    if phase > 1.0 {
        phase.fract()
    } else {
        phase.max(0.0)
    }
}

impl Animators {
    /// What the animated thing `id` belongs to is doing: `id` itself, the
    /// thing above it that's animated (for an Animator, Animation or bone
    /// selected), or else the first animated thing inside it. `None` when
    /// nothing there has played since the last [`clear`](Self::clear).
    pub fn inspect(&self, world: &World, id: InstanceId) -> Option<AnimatorView> {
        let owner = self.owner_of(world, id)?;
        let running = self.running.get(&owner)?;
        let live = running
            .animator
            .and_then(|animator| world.animator(animator));
        let parameters = live.map(|live| live.parameters.clone()).unwrap_or_default();
        let machine = running.machine.as_ref().map(|machine| {
            let compiled = &machine.compiled;
            let current = machine.machine.current;
            MachineView {
                path: machine.path.clone(),
                state: compiled.state_name(current.state).to_owned(),
                progress: progress(current.phase),
                duration: compiled.duration(current.state, &parameters),
                fade: machine.machine.fade.map(|fade| FadeView {
                    from: compiled.state_name(fade.from.state).to_owned(),
                    weight: fade.weight(),
                    seconds: fade.duration,
                }),
                layers: compiled
                    .layers()
                    .iter()
                    .zip(&machine.layers)
                    .map(|(layer, at)| LayerView {
                        name: layer.name.clone(),
                        state: layer.machine.state_name(at.current.state).to_owned(),
                        weight: live
                            .and_then(|live| live.layer_weights.get(&layer.name))
                            .map_or(layer.weight, |weight| weight.weight),
                    })
                    .collect(),
                root_travel: machine.motion.travel,
                root_yaw: machine.motion.yaw,
            }
        });
        Some(AnimatorView {
            owner,
            animator: running.animator,
            machine,
            playing: running
                .layers
                .iter()
                .map(|layer| PlayingView {
                    source: layer.source,
                    clip: layer.path.clone(),
                    progress: progress(layer.phase),
                    weight: layer.weight,
                })
                .collect(),
            parameters,
            fired: running
                .recent
                .iter()
                .rev()
                .map(|fired| FiredView {
                    seconds_ago: (self.clock - fired.at).max(0.0),
                    what: fired.what.clone(),
                })
                .collect(),
            posed: self.poses.contains_key(&owner),
        })
    }

    /// Every animated thing, in a steady order, for the inspector to offer
    /// when nothing animated is selected.
    pub fn animated(&self) -> Vec<InstanceId> {
        let mut owners: Vec<InstanceId> = self.running.keys().copied().collect();
        owners.sort();
        owners
    }

    /// The animated thing `id` is, is in, or holds.
    fn owner_of(&self, world: &World, id: InstanceId) -> Option<InstanceId> {
        let mut at = Some(id);
        while let Some(here) = at {
            if self.running.contains_key(&here) {
                return Some(here);
            }
            at = world.get(here).and_then(|instance| instance.parent());
        }
        self.running
            .keys()
            .copied()
            .filter(|&owner| world.is_descendant_of(owner, id))
            .min()
    }
}

#[cfg(test)]
#[path = "inspect_tests.rs"]
mod tests;
