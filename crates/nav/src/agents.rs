//! Navigation agents: a [`NavigationAgent`](axcenity_core::ClassName::NavigationAgent)
//! walks the Model or Part it is parented to during Play.
//!
//! When its `Destination` changes (and when Play starts), the agent finds a
//! path on the navigation mesh and walks it at `WalkSpeed`, its feet on the
//! ground. A destination it can't reach leaves it where it is, sets
//! `Status` to `"Unreachable"` and gives one warning. A `Velocity` that
//! isn't zero moves it straight that way instead, along the ground and
//! never off it, with `Destination` following it.
//!
//! Like a character controller, an agent is kinematic: [`Agents::keep_kinematic`]
//! anchors its parts before physics. Its parts aren't ground, so the mesh
//! leaves them out.

use std::collections::BTreeMap;

use axcenity_assets::Assets;
use axcenity_core::{ClassName, InstanceId, Value, Vector3, World};

use crate::{NavMesh, NavSettings, fingerprint};

/// The least game time between two builds of the mesh, so parts falling
/// don't rebuild it every frame. The editor's overlay waits as long.
const REBUILD_SECONDS: f32 = 0.5;
/// The longest stretch moved before the ground is looked up again, in cells.
const SLICE: f32 = 0.5;
const MAX_SLICES: f32 = 64.0;

/// Where an agent is in following its destination, as `Status` says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentStatus {
    /// Stopped after `Velocity` moved it, or not started.
    Idle,
    /// Walking a path, or moving at `Velocity`.
    Moving,
    /// At its destination.
    Reached,
    /// Its destination is off the mesh or has no route to it.
    Unreachable,
}

impl AgentStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            AgentStatus::Idle => "Idle",
            AgentStatus::Moving => "Moving",
            AgentStatus::Reached => "Reached",
            AgentStatus::Unreachable => "Unreachable",
        }
    }
}

/// One agent's walk. Not saved.
#[derive(Default)]
struct Walk {
    /// The `Destination` this walk is for.
    target: Option<Vector3>,
    /// The corners still ahead, nearest first.
    corners: Vec<Vector3>,
    /// Where its feet were after its last step.
    feet: Vector3,
    /// True while `Velocity` moves it.
    driven: bool,
    /// The last warning given, so a retry that fails the same way is quiet.
    warned: Option<String>,
}

/// The mesh the agents walk on, and what it was built from.
struct Built {
    mesh: NavMesh,
    fingerprint: u64,
    /// Game seconds since it was built.
    age: f32,
}

/// Steps every navigation agent. The player and the editor each own one
/// for the game they are running.
#[derive(Default)]
pub struct Agents {
    built: Option<Built>,
    walks: BTreeMap<InstanceId, Walk>,
    warnings: Vec<String>,
}

impl Agents {
    /// Moves every agent in the Workspace that has a body for `seconds` of
    /// game time. The mesh is built the first time an agent needs it, and
    /// again after the world changes. Hosts call it after character
    /// controllers and before scripts.
    pub fn step(&mut self, world: &mut World, assets: &Assets, seconds: f32) {
        let dt = if seconds.is_finite() && seconds > 0.0 {
            seconds
        } else {
            0.0
        };
        self.walks.retain(|id, _| world.contains(*id));
        let agents = agents(world);
        if agents.is_empty() {
            return;
        }
        self.refresh_mesh(world, assets, dt);
        let Some(built) = &self.built else {
            return;
        };
        for id in agents {
            let walk = self.walks.entry(id).or_default();
            if let Some(warning) = step_agent(world, &built.mesh, id, walk, dt) {
                self.warnings.push(warning);
            }
        }
    }

    /// Builds the mesh if there's none, or again once the world it was
    /// built from has changed and it's old enough. Agents that couldn't
    /// reach their destination try again on the new mesh.
    fn refresh_mesh(&mut self, world: &World, assets: &Assets, dt: f32) {
        let now = fingerprint(world);
        if let Some(built) = &mut self.built {
            built.age += dt;
            if built.fingerprint == now || built.age < REBUILD_SECONDS {
                return;
            }
            for walk in self.walks.values_mut() {
                if walk.warned.is_some() && walk.corners.is_empty() {
                    walk.target = None;
                }
            }
        }
        self.built = Some(Built {
            mesh: NavMesh::build(world, assets, NavSettings::default()),
            fingerprint: now,
            age: 0.0,
        });
    }

    /// Anchors every part an agent moves, so physics doesn't drop or push
    /// it. Hosts call it just before the physics step, as they do for
    /// character controllers.
    pub fn keep_kinematic(&self, world: &mut World) {
        for part in agent_parts(world) {
            let anchored = world
                .get(part)
                .and_then(|instance| instance.property("Anchored"))
                .and_then(Value::as_bool);
            if anchored == Some(false) {
                let _ = world.set_property(part, "Anchored", Value::Bool(true));
            }
        }
    }

    /// The path an agent is walking: where its feet are, then the corners
    /// still ahead. `None` when it isn't walking one.
    pub fn path(&self, agent: InstanceId) -> Option<Vec<Vector3>> {
        let walk = self.walks.get(&agent)?;
        if walk.corners.is_empty() {
            return None;
        }
        let mut path = vec![walk.feet];
        path.extend(&walk.corners);
        Some(path)
    }

    /// Every agent's path, as [`Self::path`] gives it, for drawing.
    pub fn paths(&self) -> Vec<(InstanceId, Vec<Vector3>)> {
        self.walks
            .keys()
            .filter_map(|&id| Some((id, self.path(id)?)))
            .collect()
    }

    /// The mesh the agents walk on, once one has been built.
    pub fn mesh(&self) -> Option<&NavMesh> {
        self.built.as_ref().map(|built| &built.mesh)
    }

    /// The warnings since the last call, such as a destination no route
    /// leads to. Hosts show them (the editor's Output, the player's log).
    pub fn take_warnings(&mut self) -> Vec<String> {
        std::mem::take(&mut self.warnings)
    }
}

/// Every NavigationAgent in the Workspace whose parent is a body.
fn agents(world: &World) -> Vec<InstanceId> {
    world
        .descendants(world.workspace())
        .into_iter()
        .filter(|&id| {
            world
                .get(id)
                .is_some_and(|instance| instance.class() == ClassName::NavigationAgent)
                && !parts_of(world, id).is_empty()
        })
        .collect()
}

/// Every part a NavigationAgent in the Workspace moves. They walk the
/// mesh, so they aren't part of it.
pub fn agent_parts(world: &World) -> Vec<InstanceId> {
    agents(world)
        .into_iter()
        .flat_map(|id| parts_of(world, id))
        .collect()
}

fn parts_of(world: &World, agent: InstanceId) -> Vec<InstanceId> {
    match world.get(agent).and_then(|instance| instance.parent()) {
        Some(parent) => axcenity_character::body_parts(world, parent),
        None => Vec::new(),
    }
}

/// Where the feet of the body `parts` make are: the middle of its box, at
/// the bottom.
pub fn feet_of(world: &World, parts: &[InstanceId]) -> Option<Vector3> {
    let mut min = Vector3::new(f32::INFINITY, f32::INFINITY, f32::INFINITY);
    let mut max = Vector3::new(f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);
    for &id in parts {
        let instance = world.get(id)?;
        let position = instance.property("Position").and_then(Value::as_vector3)?;
        let size = instance.property("Size").and_then(Value::as_vector3)?;
        if !crate::finite(position) || !crate::finite(size) {
            continue;
        }
        min.x = min.x.min(position.x - size.x.abs() * 0.5);
        min.y = min.y.min(position.y - size.y.abs() * 0.5);
        min.z = min.z.min(position.z - size.z.abs() * 0.5);
        max.x = max.x.max(position.x + size.x.abs() * 0.5);
        max.z = max.z.max(position.z + size.z.abs() * 0.5);
    }
    crate::finite(min).then(|| Vector3::new((min.x + max.x) * 0.5, min.y, (min.z + max.z) * 0.5))
}

fn vector(world: &World, id: InstanceId, name: &str) -> Option<Vector3> {
    let value = world.get(id)?.property(name)?.as_vector3()?;
    crate::finite(value).then_some(value)
}

fn set_status(world: &mut World, id: InstanceId, status: AgentStatus) {
    let _ = world.set_property(id, "Status", Value::String(status.as_str().to_owned()));
}

/// One agent's step. Returns a warning when it can't reach a new destination.
fn step_agent(
    world: &mut World,
    mesh: &NavMesh,
    id: InstanceId,
    walk: &mut Walk,
    dt: f32,
) -> Option<String> {
    let parts = parts_of(world, id);
    let mut feet = feet_of(world, &parts)?;
    let velocity = vector(world, id, "Velocity").unwrap_or(Vector3::new(0.0, 0.0, 0.0));
    let destination = vector(world, id, "Destination")?;

    if velocity.x.hypot(velocity.z) > 1e-4 {
        walk.corners.clear();
        walk.driven = true;
        let to = Vector3::new(feet.x + velocity.x * dt, feet.y, feet.z + velocity.z * dt);
        move_feet(world, mesh, &parts, &mut feet, to, true);
        // Destination follows, so the agent stays where Velocity leaves it.
        walk.target = Some(feet);
        walk.feet = feet;
        let _ = world.set_property(id, "Destination", Value::Vector3(feet));
        set_status(world, id, AgentStatus::Moving);
        return None;
    }
    if walk.driven {
        walk.driven = false;
        if walk.target == Some(destination) {
            set_status(world, id, AgentStatus::Idle);
        }
    }

    let mut warning = None;
    if walk.target != Some(destination) {
        walk.target = Some(destination);
        match mesh.find_path(feet, destination) {
            Ok(path) => {
                walk.corners = path.into_iter().skip(1).collect();
                walk.warned = None;
                set_status(world, id, AgentStatus::Moving);
            }
            Err(why) => {
                walk.corners.clear();
                set_status(world, id, AgentStatus::Unreachable);
                let name = world
                    .full_name(id)
                    .unwrap_or_else(|| "NavigationAgent".to_owned());
                let message = format!("{name} can't reach {}: {why}.", point(destination));
                if walk.warned.as_ref() != Some(&message) {
                    warning = Some(message.clone());
                }
                walk.warned = Some(message);
            }
        }
    }

    if !walk.corners.is_empty() {
        let speed = world
            .get(id)
            .and_then(|instance| instance.property("WalkSpeed"))
            .and_then(Value::as_number)
            .filter(|speed| speed.is_finite())
            .unwrap_or(16.0)
            .max(0.0) as f32;
        let mut left = speed * dt;
        while left > 0.0
            && let Some(&corner) = walk.corners.first()
        {
            let away = (corner.x - feet.x).hypot(corner.z - feet.z);
            if away <= left {
                move_feet(world, mesh, &parts, &mut feet, corner, false);
                walk.corners.remove(0);
                left -= away;
            } else {
                let share = left / away;
                let to = Vector3::new(
                    feet.x + (corner.x - feet.x) * share,
                    feet.y + (corner.y - feet.y) * share,
                    feet.z + (corner.z - feet.z) * share,
                );
                move_feet(world, mesh, &parts, &mut feet, to, false);
                left = 0.0;
            }
        }
        if walk.corners.is_empty() {
            set_status(world, id, AgentStatus::Reached);
        }
    }
    walk.feet = feet;
    warning
}

/// Moves the body's feet across to `to`'s X and Z in short slices, each on
/// the ground of the mesh. Off the mesh, a path keeps to the heights of its
/// corners, and `Velocity` (`stop_at_edges`) stops instead.
fn move_feet(
    world: &mut World,
    mesh: &NavMesh,
    parts: &[InstanceId],
    feet: &mut Vector3,
    to: Vector3,
    stop_at_edges: bool,
) {
    let (dx, dz) = (to.x - feet.x, to.z - feet.z);
    let slices = (dx.hypot(dz) / (mesh.settings().cell_size * SLICE))
        .ceil()
        .clamp(1.0, MAX_SLICES);
    let from = *feet;
    for slice in 1..=slices as usize {
        let share = slice as f32 / slices;
        let x = from.x + dx * share;
        let z = from.z + dz * share;
        let y = match mesh.ground_at(Vector3::new(x, feet.y, z)) {
            Some(y) => y,
            None if stop_at_edges => return,
            None => from.y + (to.y - from.y) * share,
        };
        let next = Vector3::new(x, y, z);
        translate(
            world,
            parts,
            next.x - feet.x,
            next.y - feet.y,
            next.z - feet.z,
        );
        *feet = next;
    }
}

fn translate(world: &mut World, parts: &[InstanceId], dx: f32, dy: f32, dz: f32) {
    if dx == 0.0 && dy == 0.0 && dz == 0.0 {
        return;
    }
    for &id in parts {
        let Some(position) = world
            .get(id)
            .and_then(|instance| instance.property("Position"))
            .and_then(Value::as_vector3)
        else {
            continue;
        };
        let next = Vector3::new(position.x + dx, position.y + dy, position.z + dz);
        let _ = world.set_property(id, "Position", Value::Vector3(next));
    }
}

/// A point as `(20, 0, 5.5)`, without trailing zeros.
fn point(p: Vector3) -> String {
    let short = |n: f32| {
        let text = format!("{n:.2}");
        let text = text.trim_end_matches('0').trim_end_matches('.');
        if text == "-0" {
            "0".to_owned()
        } else {
            text.to_owned()
        }
    };
    format!("({}, {}, {})", short(p.x), short(p.y), short(p.z))
}
