//! Running every Animator in a world, owned by the host (the player, or
//! the editor's Play session) like its physics.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use axcenity_assets::{Assets, Model, Posed, Rig, Take};
use axcenity_core::{ClassName, InstanceId, Value, Vector3, World};
use glam::{EulerRot, Quat, Vec3};

use crate::clip::RootMotion;

use crate::controller::{Compiled, Machine};
use crate::files::{ClipFile, ControllerFile, DEFAULT_BLEND};

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
    /// The Animator's `AnimationEvent`: a playing clip passed this event.
    Marker { animator: InstanceId, name: String },
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
        }
    }
}

/// Every Animator's place in its controller, and the poses they make.
#[derive(Default)]
pub struct Animators {
    running: HashMap<InstanceId, Running>,
    poses: Poses,
    errors: Vec<String>,
    /// Errors already given, so each is reported once.
    reported: HashSet<String>,
}

/// One Animator being played.
struct Running {
    /// The `Controller` and model it was made for; a change starts it again.
    controller: String,
    model_key: u64,
    part: InstanceId,
    compiled: Arc<Compiled>,
    machine: Machine,
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

    /// Forgets every animator, so the next update starts each from its
    /// default state with its files read again.
    pub fn clear(&mut self) {
        self.running.clear();
        self.poses.clear();
        self.reported.clear();
    }

    /// Moves every Animator in the Workspace on by `dt` seconds of game
    /// time (0 just poses them) and poses its MeshPart. Fills in the
    /// controller's parameters an animator doesn't have yet, starts the
    /// states scripts asked for (`animator:Play`) and keeps `CurrentState`
    /// up to date. Returns what scripts should hear about, in order.
    pub fn update(&mut self, world: &mut World, assets: &Assets, dt: f64) -> Vec<AnimatorEvent> {
        let mut events = Vec::new();
        let animators: Vec<InstanceId> = world
            .descendants(world.workspace())
            .into_iter()
            .filter(|&id| {
                world
                    .get(id)
                    .is_some_and(|i| i.class() == ClassName::Animator)
            })
            .collect();
        let alive: HashSet<InstanceId> = animators.iter().copied().collect();
        self.running.retain(|id, _| alive.contains(id));
        let mut posed_parts = HashSet::new();

        for id in animators {
            let Some(found) = self.find(world, assets, id) else {
                self.running.remove(&id);
                continue;
            };
            let Found {
                part,
                model,
                controller,
                enabled,
                speed,
            } = found;
            let Some(rig) = model.rig.clone() else {
                let name = world.full_name(id).unwrap_or_default();
                self.report(format!(
                    "The Animator `{name}` is in a MeshPart whose model has no skeleton or animation, so it can't animate it."
                ));
                self.running.remove(&id);
                continue;
            };
            let stale = self.running.get(&id).is_none_or(|running| {
                running.controller != controller
                    || running.model_key != model.key()
                    || running.part != part
            });
            if stale {
                match self.start(world, assets, id, &controller, &rig) {
                    Some(compiled) => {
                        let machine = Machine::new(&compiled);
                        self.running.insert(
                            id,
                            Running {
                                controller: controller.clone(),
                                model_key: model.key(),
                                part,
                                compiled,
                                machine,
                            },
                        );
                    }
                    None => {
                        self.running.remove(&id);
                        continue;
                    }
                }
            }
            let Some(running) = self.running.get_mut(&id) else {
                continue;
            };
            let compiled = running.compiled.clone();

            // Parameters the animator doesn't have yet start at their defaults.
            let request = world.animator_mut(id).and_then(|state| {
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
                        let name = world.full_name(id).unwrap_or_default();
                        let message = format!(
                            "The Animator `{name}` was asked to play `{}`, but its controller has no state with that name.",
                            request.state
                        );
                        report(&mut self.errors, &mut self.reported, message);
                    }
                }
            }
            let mut markers = Vec::new();
            // Blend trees mix by the parameters as they are now.
            let parameters = world
                .animator(id)
                .map(|live| live.parameters.clone())
                .unwrap_or_default();
            if enabled {
                let advanced = running
                    .machine
                    .advance(&compiled, &rig, &parameters, dt * speed);
                markers = advanced.events;
                move_by_root_motion(world, part, &rig, advanced.motion);
                // Transitions whose time has come, now the clock has moved on.
                if let Some(live) = world.animator_mut(id) {
                    running.machine.evaluate(&compiled, &mut live.parameters);
                }
            }
            // Parameters a script set that the controller doesn't have
            // can't change anything: say so, once each.
            if let Some(live) = world.animator(id) {
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
                    let name = world.full_name(id).unwrap_or_default();
                    for parameter in unknown {
                        let message = format!(
                            "The Animator `{name}` was given the parameter `{parameter}`, which its controller `{controller}` doesn't have, so it changes nothing.",
                            controller = running.controller
                        );
                        report(&mut self.errors, &mut self.reported, message);
                    }
                }
            }

            let pose = running.machine.pose(&compiled, &rig, &parameters);
            if let Some(posed) = model.pose(&pose) {
                self.poses.insert(part, Arc::new(posed));
                posed_parts.insert(part);
            }

            let state = compiled
                .state_name(running.machine.current.state)
                .to_owned();
            if let Some(live) = world.animator_mut(id)
                && live.current_state != state
            {
                let previous = std::mem::replace(&mut live.current_state, state.clone());
                events.push(AnimatorEvent::StateChanged {
                    animator: id,
                    state,
                    previous,
                });
            }
            events.extend(
                markers
                    .into_iter()
                    .map(|name| AnimatorEvent::Marker { animator: id, name }),
            );
        }
        // A MeshPart whose animator is gone (or broke) goes back to its
        // still model.
        self.poses.retain(|part, _| posed_parts.contains(part));
        events
    }

    /// The animator's MeshPart, its model and its settings, or `None` if it
    /// has nothing to play (no Controller, not in a MeshPart with a model).
    fn find(&mut self, world: &World, assets: &Assets, id: InstanceId) -> Option<Found> {
        let animator = world.get(id)?;
        let controller = animator
            .property("Controller")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        if controller.is_empty() {
            return None;
        }
        let enabled = animator.property("Enabled").and_then(Value::as_bool) != Some(false);
        let speed = animator
            .property("Speed")
            .and_then(Value::as_number)
            .filter(|speed| speed.is_finite())
            .unwrap_or(1.0);
        let part_id = animator.parent()?;
        let part = world.get(part_id)?;
        if part.class() != ClassName::MeshPart {
            let name = world.full_name(id).unwrap_or_default();
            self.report(format!(
                "The Animator `{name}` must be inside a MeshPart to animate it."
            ));
            return None;
        }
        let mesh_id = part.property("MeshId").and_then(Value::as_str)?;
        let model = assets.model(mesh_id)?;
        Some(Found {
            part: part_id,
            model,
            controller,
            enabled,
            speed,
        })
    }

    /// Reads and binds the animator's controller and clips, reporting what
    /// goes wrong. `None` if the controller can't be used at all.
    fn start(
        &mut self,
        world: &World,
        assets: &Assets,
        id: InstanceId,
        controller: &str,
        rig: &axcenity_assets::Rig,
    ) -> Option<Arc<Compiled>> {
        let name = world.full_name(id).unwrap_or_default();
        let file = assets
            .read_file(controller)
            .and_then(|bytes| String::from_utf8(bytes).map_err(|_| "it isn't text".to_owned()))
            .and_then(|text| ControllerFile::from_json(&text).map_err(|err| err.0));
        let file = match file {
            Ok(file) => file,
            Err(problem) => {
                self.report(format!(
                    "Could not load the animator controller `{controller}`: {problem}"
                ));
                return None;
            }
        };
        if file.states.is_empty() {
            self.report(format!(
                "The Animator `{name}` plays `{controller}`, which has no states yet."
            ));
            return None;
        }
        let (compiled, problems) = Compiled::new(file, rig, |path| load_clip(assets, path));
        for problem in problems {
            self.report(format!(
                "The animator controller `{controller}`: {problem}."
            ));
        }
        Some(Arc::new(compiled))
    }

    fn report(&mut self, message: String) {
        report(&mut self.errors, &mut self.reported, message);
    }
}

fn report(errors: &mut Vec<String>, reported: &mut HashSet<String>, message: String) {
    if reported.insert(message.clone()) {
        errors.push(message);
    }
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

/// What [`Animators::find`] finds.
struct Found {
    part: InstanceId,
    model: Arc<Model>,
    controller: String,
    enabled: bool,
    speed: f64,
}

/// Reads the clip file at `path` and the takes of the model file it names.
pub fn load_clip(assets: &Assets, path: &str) -> Result<(ClipFile, Arc<Vec<Take>>), String> {
    let bytes = assets.read_file(path)?;
    let text = String::from_utf8(bytes).map_err(|_| "it isn't text".to_owned())?;
    let clip = ClipFile::from_json(&text).map_err(|err| err.0)?;
    let takes = assets
        .takes(&clip.source)
        .ok_or_else(|| format!("its model `{}` can't be read", clip.source))?;
    Ok((clip, takes))
}
