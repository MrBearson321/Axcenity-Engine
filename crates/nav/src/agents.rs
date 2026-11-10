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
//! While it walks, it looks along its path ahead on the latest mesh. Where
//! something that wasn't there when the path was found now stands in the
//! way, it finds a short detour round it, back onto the path past it. When
//! there's none, it stops, `Status` becomes `"Blocked"` and it warns once,
//! and it carries on when the way clears.
//!
//! Before it moves, an agent with a `BehaviorTree` runs its tree
//! (`behavior.rs`), which may set its `Destination`. After it moves, it
//! looks and listens for the player character (`perception.rs`).
//!
//! Like a character controller, an agent is kinematic: [`Agents::keep_kinematic`]
//! anchors its parts before physics. Its parts aren't ground, so the mesh
//! leaves them out.

use std::collections::BTreeMap;
use std::sync::Arc;

use axcenity_assets::Assets;
use axcenity_core::{ClassName, InstanceId, Value, Vector3, World};

use crate::behavior::{self, BehaviorTree, TreeRun};
use crate::perception::{self, Player, Sensed, Senses};
use crate::{MeshBuilder, NavMesh, NavSettings, Refresh};

/// The least game time between two looks at what changed, so anchored
/// parts that scripts keep moving don't rebuild the mesh every frame. The
/// editor's overlay waits as long.
const REBUILD_SECONDS: f32 = 0.5;
/// The longest stretch moved before the ground is looked up again, in cells.
const SLICE: f32 = 0.5;
const MAX_SLICES: f32 = 64.0;
/// How far along its path an agent looks for something in the way: this
/// many seconds of walking, and no less than [`MIN_LOOKAHEAD`] cells.
const LOOKAHEAD_SECONDS: f32 = 1.0;
const MIN_LOOKAHEAD: f32 = 4.0;
/// How far past an obstacle, in cells, a detour rejoins the path, where the
/// path stays clear that long.
const REJOIN: f32 = 2.0;
/// The most a detour may cost: this many times the stretch of path it
/// replaces, plus [`DETOUR_EXTRA`] cells. Anything longer is blocked.
const DETOUR_FACTOR: f32 = 3.0;
const DETOUR_EXTRA: f32 = 20.0;

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
    /// Stopped on its path, because something in the way has no way round.
    Blocked,
}

impl AgentStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            AgentStatus::Idle => "Idle",
            AgentStatus::Moving => "Moving",
            AgentStatus::Reached => "Reached",
            AgentStatus::Unreachable => "Unreachable",
            AgentStatus::Blocked => "Blocked",
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
    /// The build of the mesh on which the way ahead was found blocked. It
    /// looks again once the mesh is rebuilt.
    blocked: Option<u64>,
}

/// The mesh the agents walk on, and what it was built from.
struct Built {
    builder: MeshBuilder,
    /// Game seconds since what changed was last read.
    age: f32,
    /// Counts builds, so agents know when to look again.
    generation: u64,
}

/// Steps every navigation agent. The player and the editor each own one
/// for the game they are running.
#[derive(Default)]
pub struct Agents {
    built: Option<Built>,
    walks: BTreeMap<InstanceId, Walk>,
    senses: BTreeMap<InstanceId, Senses>,
    /// Each agent's behavior tree as it runs.
    runs: BTreeMap<InstanceId, TreeRun>,
    /// Every tree file read, by path; `None` when it can't run.
    trees: BTreeMap<String, Option<Arc<BehaviorTree>>>,
    warnings: Vec<String>,
    sensed: Vec<Sensed>,
    /// Each patrolling agent's place on its `PatrolSpline` (Phase 73).
    patrols: BTreeMap<InstanceId, Patrol>,
}

/// Where an agent is on its patrol. Not saved.
struct Patrol {
    spline: InstanceId,
    /// The point it's walking to.
    index: usize,
    /// On an open spline, true while it walks toward the last point.
    forward: bool,
}

impl Agents {
    /// Moves every agent in the Workspace that has a body for `seconds` of
    /// game time, then has it look and listen for the player character.
    /// The mesh is built the first time an agent needs it, and again after
    /// the world changes. Hosts call it after character controllers and
    /// before scripts.
    pub fn step(&mut self, world: &mut World, assets: &Assets, seconds: f32) {
        let dt = if seconds.is_finite() && seconds > 0.0 {
            seconds
        } else {
            0.0
        };
        self.walks.retain(|id, _| world.contains(*id));
        self.senses.retain(|id, _| world.contains(*id));
        self.runs.retain(|id, _| world.contains(*id));
        self.patrols.retain(|id, _| world.contains(*id));
        let agents = agents(world);
        if agents.is_empty() {
            return;
        }
        self.refresh_mesh(world, assets, dt);
        if self.built.is_none() {
            return;
        }
        // Patrols pick the next point, then trees decide, from what the
        // agents sensed last frame.
        for &id in &agents {
            self.patrol(world, id);
        }
        let player_feet = axcenity_character::player_character(world)
            .and_then(|holder| feet_of(world, &axcenity_character::body_parts(world, holder)));
        for &id in &agents {
            if let Some(feet) = feet_of(world, &parts_of(world, id)) {
                let agent = behavior::Agent {
                    id,
                    feet,
                    player: player_feet,
                };
                self.think(world, assets, agent, dt);
            }
        }
        let Some(built) = &self.built else {
            return;
        };
        let player = Player::find(world);
        for id in agents {
            let parts = parts_of(world, id);
            let before = feet_of(world, &parts);
            let walk = self.walks.entry(id).or_default();
            if let Some(warning) = step_agent(world, built, id, walk, dt) {
                self.warnings.push(warning);
            }
            let moved = match (before, feet_of(world, &parts)) {
                (Some(a), Some(b)) => (b.x - a.x, b.z - a.z),
                _ => (0.0, 0.0),
            };
            let senses = self.senses.entry(id).or_default();
            for sense in
                perception::perceive(world, assets, id, &parts, senses, player.as_ref(), moved)
            {
                self.sensed.push(Sensed { agent: id, sense });
            }
        }
    }

    /// Runs agent's behavior tree, if it has one, reading the file the
    /// first time it's named. One that can't run warns once and leaves the
    /// agent to its scripts.
    fn think(&mut self, world: &mut World, assets: &Assets, agent: behavior::Agent, dt: f32) {
        let path = world
            .get(agent.id)
            .and_then(|instance| instance.property("BehaviorTree"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_owned();
        if path.is_empty() {
            self.runs.remove(&agent.id);
            return;
        }
        if self.runs.get(&agent.id).is_none_or(|run| run.path != path) {
            let tree = match self.trees.get(&path) {
                Some(tree) => tree.clone(),
                None => {
                    let tree = BehaviorTree::load(assets, &path)
                        .map_err(|why| {
                            self.warnings.push(format!(
                                "{} doesn't run the behavior tree `{path}`: {why}.",
                                name(world, agent.id)
                            ))
                        })
                        .ok()
                        .map(Arc::new);
                    self.trees.insert(path.clone(), tree.clone());
                    tree
                }
            };
            self.runs.insert(agent.id, TreeRun::new(path, tree));
        }
        if let Some(run) = self.runs.get_mut(&agent.id) {
            run.tick(world, agent, dt);
        }
    }

    /// Points a patrolling agent's `Destination` at its PatrolSpline's
    /// next point once it reached (or can't reach) the one it walked to.
    fn patrol(&mut self, world: &mut World, id: InstanceId) {
        let spline = world
            .get(id)
            .and_then(|agent| agent.property("PatrolSpline"))
            .and_then(Value::as_instance)
            .filter(|&spline| {
                world
                    .get(spline)
                    .is_some_and(|i| i.class() == ClassName::Spline)
            });
        let Some(spline) = spline else {
            self.patrols.remove(&id);
            return;
        };
        let points = patrol_points(world, spline);
        if points.is_empty() {
            return;
        }
        let closed = world
            .get(spline)
            .and_then(|i| i.property("Closed"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let n = points.len();
        let next = match self.patrols.get_mut(&id) {
            Some(patrol) if patrol.spline == spline => {
                patrol.index = patrol.index.min(n - 1);
                let goal = points[patrol.index];
                let status = world
                    .get(id)
                    .and_then(|agent| agent.property("Status"))
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let arrived = matches!(status, "Reached" | "Unreachable")
                    && vector(world, id, "Destination") == Some(goal);
                if !arrived {
                    if vector(world, id, "Destination") != Some(goal) && status != "Moving" {
                        // Something else sent it elsewhere and it got
                        // there: it carries on with its patrol.
                        Some(goal)
                    } else {
                        None
                    }
                } else if n == 1 {
                    None
                } else if closed {
                    patrol.index = (patrol.index + 1) % n;
                    Some(points[patrol.index])
                } else {
                    if patrol.index + 1 >= n {
                        patrol.forward = false;
                    } else if patrol.index == 0 {
                        patrol.forward = true;
                    }
                    patrol.index = if patrol.forward {
                        patrol.index + 1
                    } else {
                        patrol.index - 1
                    };
                    Some(points[patrol.index])
                }
            }
            _ => {
                // It starts at the point nearest it.
                let feet = feet_of(world, &parts_of(world, id)).unwrap_or_default();
                let index = (0..n)
                    .min_by(|&a, &b| {
                        horizontal(points[a], feet).total_cmp(&horizontal(points[b], feet))
                    })
                    .unwrap_or(0);
                self.patrols.insert(
                    id,
                    Patrol {
                        spline,
                        index,
                        forward: true,
                    },
                );
                Some(points[index])
            }
        };
        if let Some(point) = next {
            let _ = world.set_property(id, "Destination", Value::Vector3(point));
        }
    }

    /// The agents that started seeing or hearing the player character since
    /// the last call. Hosts fire each one's `PlayerSensed` with the sense.
    pub fn take_sensed(&mut self) -> Vec<Sensed> {
        std::mem::take(&mut self.sensed)
    }

    /// Builds the mesh if there's none. Once the last look is old enough,
    /// reads again the ground parts that changed, and rebuilds the mesh
    /// when they change it. Agents that couldn't reach their destination
    /// try again on the new mesh.
    fn refresh_mesh(&mut self, world: &World, assets: &Assets, dt: f32) {
        let Some(built) = &mut self.built else {
            let mut builder = MeshBuilder::new(NavSettings::default());
            builder.update(world, assets);
            self.built = Some(Built {
                builder,
                age: 0.0,
                generation: 0,
            });
            return;
        };
        built.age += dt;
        if built.age < REBUILD_SECONDS {
            return;
        }
        match built.builder.update(world, assets) {
            Refresh::Unchanged => return,
            Refresh::Same => {
                built.age = 0.0;
                return;
            }
            Refresh::Rebuilt => {
                built.age = 0.0;
                built.generation += 1;
            }
        }
        for walk in self.walks.values_mut() {
            if walk.warned.is_some() && walk.corners.is_empty() {
                walk.target = None;
            }
        }
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
        self.built.as_ref().map(|built| built.builder.mesh())
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
        .all_below(world.workspace(), |class| {
            class == ClassName::NavigationAgent
        })
        .into_iter()
        .filter(|&id| !parts_of(world, id).is_empty())
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
    let (min, max) = body_box(world, parts)?;
    Some(Vector3::new(
        (min.x + max.x) * 0.5,
        min.y,
        (min.z + max.z) * 0.5,
    ))
}

/// The lowest and highest corners of the box round the body `parts` make,
/// ignoring rotation. `None` when no part has a finite place and size.
pub(crate) fn body_box(world: &World, parts: &[InstanceId]) -> Option<(Vector3, Vector3)> {
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
        max.y = max.y.max(position.y + size.y.abs() * 0.5);
        max.z = max.z.max(position.z + size.z.abs() * 0.5);
    }
    crate::finite(min).then_some((min, max))
}

fn vector(world: &World, id: InstanceId, name: &str) -> Option<Vector3> {
    let value = world.get(id)?.property(name)?.as_vector3()?;
    crate::finite(value).then_some(value)
}

/// The SplinePoints of `spline`, in order.
fn patrol_points(world: &World, spline: InstanceId) -> Vec<Vector3> {
    world
        .get(spline)
        .map(|spline| spline.children().to_vec())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|id| {
            let point = world.get(id)?;
            (point.class() == ClassName::SplinePoint)
                .then(|| point.property("Position").and_then(Value::as_vector3))
                .flatten()
        })
        .collect()
}

fn set_status(world: &mut World, id: InstanceId, status: AgentStatus) {
    let _ = world.set_property(id, "Status", Value::String(status.as_str().to_owned()));
}

/// One agent's step. Returns a warning when it can't reach a new
/// destination, or when it's newly blocked.
fn step_agent(
    world: &mut World,
    built: &Built,
    id: InstanceId,
    walk: &mut Walk,
    dt: f32,
) -> Option<String> {
    let mesh = built.builder.mesh();
    let parts = parts_of(world, id);
    let mut feet = feet_of(world, &parts)?;
    let velocity = vector(world, id, "Velocity").unwrap_or(Vector3::new(0.0, 0.0, 0.0));
    let destination = vector(world, id, "Destination")?;

    if velocity.x.hypot(velocity.z) > 1e-4 {
        walk.corners.clear();
        walk.blocked = None;
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
        walk.blocked = None;
        match mesh.find_path(feet, destination) {
            Ok(path) => {
                walk.corners = path.into_iter().skip(1).collect();
                walk.warned = None;
                set_status(world, id, AgentStatus::Moving);
            }
            Err(why) => {
                walk.corners.clear();
                set_status(world, id, AgentStatus::Unreachable);
                let message = format!(
                    "{} can't reach {}: {why}.",
                    name(world, id),
                    point(destination)
                );
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
        let was_blocked = walk.blocked.is_some();
        match steer(mesh, built.generation, walk, feet, speed) {
            Ahead::Clear if was_blocked => set_status(world, id, AgentStatus::Moving),
            Ahead::Clear => {}
            Ahead::Blocked => {
                set_status(world, id, AgentStatus::Blocked);
                if !was_blocked {
                    warning = Some(format!(
                        "{} is blocked on its way to {}: nothing walkable goes round what's in \
                         the way.",
                        name(world, id),
                        point(destination)
                    ));
                }
                walk.feet = feet;
                return warning;
            }
        }
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

/// What an agent found on its path ahead.
enum Ahead {
    /// Clear, or it now walks a detour round what's in the way.
    Clear,
    /// Nothing walkable goes round what's in the way.
    Blocked,
}

/// Looks along the agent's path ahead, from its feet, on the current mesh.
/// When a stretch can't be walked any more, its corners become a detour
/// round it that rejoins the path past it, then the rest of the path. With
/// no detour it's blocked, until the mesh is rebuilt.
fn steer(mesh: &NavMesh, generation: u64, walk: &mut Walk, feet: Vector3, speed: f32) -> Ahead {
    if walk.blocked == Some(generation) {
        return Ahead::Blocked;
    }
    // Off the mesh (something landed where it stands), there's nothing to
    // steer on, so it keeps to its path.
    let Some(region) = mesh.region_at(feet) else {
        walk.blocked = None;
        return Ahead::Clear;
    };
    let lookahead = (speed * LOOKAHEAD_SECONDS).max(mesh.settings().cell_size * MIN_LOOKAHEAD);
    let mut points = vec![feet];
    points.extend(&walk.corners);
    let mut along = 0.0;
    let mut first = None;
    for (i, pair) in points.windows(2).enumerate() {
        if along > lookahead {
            break;
        }
        if !mesh.walkable_line(pair[0], pair[1]) {
            first = Some(i);
            break;
        }
        along += horizontal(pair[0], pair[1]);
    }
    let Some(first) = first else {
        walk.blocked = None;
        return Ahead::Clear;
    };
    match detour(mesh, &points, first, region) {
        Some(corners) => {
            walk.corners = corners;
            walk.blocked = None;
            Ahead::Clear
        }
        None => {
            walk.blocked = Some(generation);
            Ahead::Blocked
        }
    }
}

/// A way from `points[0]` (the feet) round what blocks the stretch starting
/// at `points[first]`, rejoining the path past it, followed by the rest of
/// the path. `None` when the path never comes clear again in the feet's
/// region, or the way round is too long to be a detour.
fn detour(mesh: &NavMesh, points: &[Vector3], first: usize, region: u32) -> Option<Vec<Vector3>> {
    let cell = mesh.settings().cell_size;
    let clear = |p: Vector3| mesh.region_at(p) == Some(region);
    // Walk the path from the blocked stretch in half-cell samples: past the
    // samples in the way, the rejoin point is REJOIN cells into the clear.
    let mut along = horizontal_length(&points[..=first]);
    let mut in_the_way = false;
    // The stretch the rejoin point is on, the point, and how far into the clear.
    let mut rejoin: Option<(usize, Vector3, f32)> = None;
    'path: for k in first..points.len() - 1 {
        let (a, b) = (points[k], points[k + 1]);
        let length = horizontal(a, b);
        let samples = (length / (cell * SLICE)).ceil().max(1.0) as usize;
        for sample in 0..=samples {
            let share = sample as f32 / samples as f32;
            let p = Vector3::new(
                a.x + (b.x - a.x) * share,
                a.y + (b.y - a.y) * share,
                a.z + (b.z - a.z) * share,
            );
            let at = along + length * share;
            if !clear(p) {
                in_the_way = true;
                rejoin = None;
            } else if in_the_way {
                let start = rejoin.map_or(at, |(_, _, start)| start);
                rejoin = Some((k, p, start));
                if at - start >= REJOIN * cell {
                    break 'path;
                }
            }
        }
        along += length;
    }
    // Every sample was clear but the stretch can't be walked (a link is
    // gone): rejoin at the stretch's end.
    if !in_the_way && clear(points[first + 1]) {
        rejoin = Some((first, points[first + 1], 0.0));
    }
    let (k, point, _) = rejoin?;
    let replaced = horizontal_length(&points[..=k]) + horizontal(points[k], point);
    let budget = replaced / cell * DETOUR_FACTOR + DETOUR_EXTRA;
    let way = mesh.find_path_within(points[0], point, budget).ok()?;
    let mut corners: Vec<Vector3> = way.into_iter().skip(1).collect();
    for &p in &points[k + 1..] {
        if corners
            .last()
            .is_none_or(|&last| horizontal(last, p) > 1e-4)
        {
            corners.push(p);
        }
    }
    Some(corners)
}

fn horizontal(a: Vector3, b: Vector3) -> f32 {
    (a.x - b.x).hypot(a.z - b.z)
}

/// The length across of a line through `points`.
fn horizontal_length(points: &[Vector3]) -> f32 {
    points.windows(2).map(|w| horizontal(w[0], w[1])).sum()
}

fn name(world: &World, id: InstanceId) -> String {
    world
        .full_name(id)
        .unwrap_or_else(|| "NavigationAgent".to_owned())
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
