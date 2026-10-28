//! ProximityPrompts (Phase 44): which prompts show for the player
//! character, holding their action, and the events that fire.
//!
//! Each frame, after the input, [`Prompts::step`] finds the prompts in
//! reach (and in sight, with `RequiresLineOfSight`), picks the ones that
//! show by `Exclusivity`, follows the input map's actions on them, and
//! returns the events for the runtime to fire on both sides. Hosts draw
//! [`Prompts::shown`]. A press counts only on the frame its action goes
//! down while the prompt shows, so walking up with the key held does
//! nothing, and holding on after `Triggered` doesn't trigger again.

use std::collections::{BTreeMap, BTreeSet};

use axcenity_assets::Assets;
use axcenity_core::input::InputMap;
use axcenity_core::{ClassName, InstanceId, Value, Vector3, World};
use glam::Vec3;

use crate::cframe::{from_vec3, to_vec3};
use crate::input::PromptInput;

/// A prompt showing this frame, for the host to draw.
#[derive(Clone, Debug, PartialEq)]
pub struct ShownPrompt {
    pub prompt: InstanceId,
    /// Where it is in the world: its part's middle, or its Attachment.
    pub position: Vector3,
    /// How far the hold is, 0 to 1. 1 once it's triggered, until let go.
    pub progress: f32,
    /// The key or button that uses it, such as `E` or `X`.
    pub key: String,
}

/// An event for the runtime to fire: on `instance`, with the Player when
/// `with_player`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fired {
    pub instance: InstanceId,
    pub event: &'static str,
    pub with_player: bool,
}

impl Fired {
    pub(crate) fn new(instance: InstanceId, event: &'static str) -> Self {
        Self {
            instance,
            event,
            with_player: true,
        }
    }

    fn bare(instance: InstanceId, event: &'static str) -> Self {
        Self {
            instance,
            event,
            with_player: false,
        }
    }
}

/// One showing prompt's hold.
#[derive(Clone, Copy, Debug, Default)]
struct Hold {
    /// Seconds its action has been held, while a hold is under way.
    held: Option<f64>,
    /// True from `Triggered` until the action is let go.
    triggered: bool,
}

/// The prompts that show and what's held, kept between frames.
#[derive(Default)]
pub struct Prompts {
    holds: BTreeMap<InstanceId, Hold>,
    /// The actions held last frame, so a press is the frame one goes down.
    held: BTreeSet<String>,
    shown: Vec<ShownPrompt>,
}

/// Where the player character's middle is and what to leave out of
/// sight lines (the character), or the camera with no character.
pub(crate) struct Viewer {
    pub(crate) position: Vec3,
    pub(crate) skip: Vec<InstanceId>,
}

impl Viewer {
    pub(crate) fn find(world: &World, camera: Option<Vec3>) -> Option<Self> {
        if let Some(holder) = axcenity_character::player_character(world)
            && let Some(middle) = body_middle(world, &axcenity_character::body_parts(world, holder))
        {
            return Some(Self {
                position: middle,
                skip: vec![holder],
            });
        }
        camera.map(|position| Self {
            position,
            skip: Vec::new(),
        })
    }
}

/// The middle of the box round `parts`, ignoring rotation.
fn body_middle(world: &World, parts: &[InstanceId]) -> Option<Vec3> {
    let mut min = Vec3::INFINITY;
    let mut max = Vec3::NEG_INFINITY;
    for &id in parts {
        let instance = world.get(id)?;
        let (Some(position), Some(size)) = (
            instance.property("Position").and_then(Value::as_vector3),
            instance.property("Size").and_then(Value::as_vector3),
        ) else {
            continue;
        };
        let (position, half) = (to_vec3(position), to_vec3(size).abs() * 0.5);
        if position.is_finite() && half.is_finite() {
            min = min.min(position - half);
            max = max.max(position + half);
        }
    }
    min.is_finite().then(|| (min + max) * 0.5)
}

fn number(world: &World, id: InstanceId, name: &str, default: f64) -> f64 {
    world
        .get(id)
        .and_then(|instance| instance.property(name))
        .and_then(Value::as_number)
        .filter(|n| n.is_finite())
        .unwrap_or(default)
}

fn text<'a>(world: &'a World, id: InstanceId, name: &str) -> &'a str {
    world
        .get(id)
        .and_then(|instance| instance.property(name))
        .and_then(Value::as_str)
        .unwrap_or("")
}

fn flag(world: &World, id: InstanceId, name: &str, default: bool) -> bool {
    world
        .get(id)
        .and_then(|instance| instance.property(name))
        .and_then(Value::as_bool)
        .unwrap_or(default)
}

/// The prompt's place and the part it's on: its parent part's middle, or
/// its parent Attachment's place in the world. `None` under anything else.
fn place(world: &World, prompt: InstanceId) -> Option<(Vec3, InstanceId)> {
    let parent = world.get(prompt)?.parent()?;
    let instance = world.get(parent)?;
    if instance.class().is_base_part() {
        let position = to_vec3(instance.property("Position")?.as_vector3()?);
        return Some((position, parent));
    }
    if instance.class() == ClassName::Attachment {
        let part = instance.parent()?;
        return Some((crate::attachment::world_position(world, parent)?, part));
    }
    None
}

/// True when nothing a query hits is between `from` and `to`, leaving
/// out `skip` (and what's inside them).
pub(crate) fn in_sight(
    world: &World,
    assets: &Assets,
    from: Vec3,
    to: Vec3,
    skip: &[InstanceId],
) -> bool {
    let toward = to - from;
    let length = toward.length();
    if length < 1e-3 {
        return true;
    }
    let hit = axcenity_physics::cast_ray_excluding(
        world,
        assets,
        from_vec3(from),
        from_vec3(toward),
        skip,
    );
    hit.is_none_or(|hit| hit.distance >= length - 1e-2)
}

/// A prompt in reach this frame.
struct Candidate {
    id: InstanceId,
    position: Vec3,
    distance: f32,
    action: String,
    exclusivity: String,
}

/// `Exclusivity`'s item: OnePerButton, OneGlobally or AlwaysShow.
fn exclusivity(world: &World, id: InstanceId) -> String {
    match world
        .get(id)
        .and_then(|instance| instance.property("Exclusivity"))
    {
        Some(Value::EnumItem(item)) => item.item.clone(),
        _ => "OnePerButton".to_owned(),
    }
}

/// The enabled prompts in the Workspace within reach (and in sight, when
/// they need it) of `viewer`, nearest first.
fn candidates(world: &World, assets: &Assets, viewer: &Viewer) -> Vec<Candidate> {
    let mut found: Vec<Candidate> = world
        .descendants(world.workspace())
        .into_iter()
        .filter(|&id| {
            world
                .get(id)
                .is_some_and(|instance| instance.class() == ClassName::ProximityPrompt)
                && flag(world, id, "Enabled", true)
        })
        .filter_map(|id| {
            let (position, part) = place(world, id)?;
            let distance = position.distance(viewer.position);
            let reach = number(world, id, "MaxActivationDistance", 10.0) as f32;
            if !distance.is_finite() || distance > reach {
                return None;
            }
            if flag(world, id, "RequiresLineOfSight", true) {
                let mut skip = viewer.skip.clone();
                skip.push(part);
                if !in_sight(world, assets, viewer.position, position, &skip) {
                    return None;
                }
            }
            Some(Candidate {
                id,
                position,
                distance,
                action: text(world, id, "Action").to_owned(),
                exclusivity: exclusivity(world, id),
            })
        })
        .collect();
    found.sort_by(|a, b| a.distance.total_cmp(&b.distance));
    found
}

/// The candidates that show: every AlwaysShow one, the nearest of the rest
/// if it's OneGlobally, and the nearest OnePerButton one for each action.
fn showing(candidates: Vec<Candidate>) -> Vec<Candidate> {
    let nearest = candidates
        .iter()
        .find(|c| c.exclusivity != "AlwaysShow")
        .map(|c| c.id);
    let mut actions = BTreeSet::new();
    candidates
        .into_iter()
        .filter(|c| match c.exclusivity.as_str() {
            "AlwaysShow" => true,
            "OneGlobally" => Some(c.id) == nearest,
            _ => actions.insert(c.action.clone()),
        })
        .collect()
}

impl Prompts {
    /// The prompts showing, as of the last step.
    pub fn shown(&self) -> &[ShownPrompt] {
        &self.shown
    }

    /// One frame of `seconds`: which prompts show, and what the input did
    /// to them. `camera` is where the view is, for a game with no
    /// character. Returns the events to fire, in order.
    pub fn step(
        &mut self,
        world: &World,
        assets: &Assets,
        input: &PromptInput,
        camera: Option<Vec3>,
        seconds: f64,
    ) -> Vec<Fired> {
        let map = &input.map;
        let showing = match Viewer::find(world, camera) {
            Some(viewer) => showing(candidates(world, assets, &viewer)),
            None => Vec::new(),
        };
        let mut fired = Vec::new();

        // Prompts that stopped showing end their hold and hide.
        let ids: BTreeSet<InstanceId> = showing.iter().map(|c| c.id).collect();
        let gone: Vec<InstanceId> = self
            .holds
            .keys()
            .copied()
            .filter(|id| !ids.contains(id))
            .collect();
        for id in gone {
            let hold = self.holds.remove(&id).unwrap_or_default();
            if world.contains(id) {
                end_hold(id, hold, &mut fired);
                fired.push(Fired::bare(id, "PromptHidden"));
            }
        }

        let down: BTreeSet<String> = map
            .actions
            .iter()
            .filter(|action| map.is_pressed(&action.name, &input.game))
            .map(|action| action.name.clone())
            .collect();
        self.shown.clear();
        for candidate in showing {
            let id = candidate.id;
            let hold = self.holds.entry(id).or_insert_with(|| {
                fired.push(Fired::bare(id, "PromptShown"));
                Hold::default()
            });
            let duration = number(world, id, "HoldDuration", 0.0).max(0.0);
            let is_down = down.contains(&candidate.action);
            let pressed = is_down && !self.held.contains(&candidate.action);
            if is_down {
                if let Some(held) = &mut hold.held {
                    *held += seconds;
                } else if pressed && !hold.triggered {
                    if duration > 0.0 {
                        fired.push(Fired::new(id, "PromptButtonHoldBegan"));
                        hold.held = Some(0.0);
                    } else {
                        fired.push(Fired::new(id, "Triggered"));
                        hold.triggered = true;
                    }
                }
                if hold.held.is_some_and(|held| held >= duration) {
                    hold.held = None;
                    hold.triggered = true;
                    fired.push(Fired::new(id, "PromptButtonHoldEnded"));
                    fired.push(Fired::new(id, "Triggered"));
                }
            } else {
                end_hold(id, std::mem::take(hold), &mut fired);
            }
            let progress = match hold.held {
                _ if hold.triggered => 1.0,
                Some(held) if duration > 0.0 => (held / duration).clamp(0.0, 1.0) as f32,
                _ => 0.0,
            };
            self.shown.push(ShownPrompt {
                prompt: id,
                position: from_vec3(candidate.position),
                progress,
                key: label(map, &candidate.action, input.gamepad),
            });
        }
        self.held = down;
        fired
    }
}

/// The events for a hold or a trigger that ends: let go, or hidden.
fn end_hold(id: InstanceId, hold: Hold, fired: &mut Vec<Fired>) {
    if hold.held.is_some() {
        fired.push(Fired::new(id, "PromptButtonHoldEnded"));
    }
    if hold.triggered {
        fired.push(Fired::new(id, "TriggerEnded"));
    }
}

/// The key or button `action` shows, or the action's name if the map
/// hasn't got it.
fn label(map: &InputMap, action: &str, gamepad: bool) -> String {
    match map.action(action) {
        Some(action) => action.label(gamepad),
        None => action.to_owned(),
    }
}
