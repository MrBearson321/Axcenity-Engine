//! Axcenity physics: simulates the Parts in the Workspace with `rapier3d`.
//!
//! Anchored parts are fixed colliders, and unanchored parts are dynamic
//! bodies. [`Physics::step`] runs at a fixed 60 steps a second whatever the
//! frame rate, and writes each moved part's `Position` and `Orientation` back
//! into the [`World`].
//!
//! MeshParts collide as the convex hull of their model, which comes from
//! the [`Assets`] given to [`Physics::with_assets`]. Terrains collide as
//! their heightfield, rebuilt only when the terrain changes. Painted
//! foliage marked to collide is extra solid colliders on that terrain.
//!
//! Physics never gets told about changes. Instead, before stepping it compares
//! every Part with what it last saw (or wrote), so a script that moves,
//! anchors, resizes or deletes a part is noticed on the next frame.
//!
//! After each step it also notes which parts started touching, for the
//! `Touched` event. The host collects them with [`Physics::take_touches`].
//!
//! The Workspace's `GlobalWind` pushes unanchored parts that have
//! `WindAffected` on: before each step, each gets a drag force toward the
//! wind's speed, larger for parts with bigger faces (see [`WIND_DRAG`]).
//!
//! Each part weighs, grips and bounces by its `CustomPhysicalProperties`,
//! or its Material's (see [`axcenity_core::Material::physical`]). The
//! Workspace's `Gravity` pulls everything down. Physics writes each moving
//! part's `AssemblyLinearVelocity` and `AssemblyAngularVelocity` back, and
//! a script that sets them throws the part; [`Mass`] tells scripts how much
//! an impulse changes them.
//!
//! Parts held together by enabled WeldConstraints are one assembly: one
//! rapier body with a collider per part (see [`assembly`]). An anchored part
//! anchors its whole assembly.
//!
//! Hinge, ball socket, prismatic, cylindrical, spring, rope and rod
//! constraints join two parts' bodies with a rapier joint (see [`joints`]).
//!
//! [`cast_ray`] and [`debug_colliders`] read the world the same way the
//! simulation does, without stepping it or writing back.

pub mod assembly;
pub mod joints;
mod mass;
mod material;
mod query;
mod shapes;

pub use mass::Mass;
pub use query::{
    ColliderOutline, Column, RayHit, Solid, cast_ray, cast_ray_excluding, debug_colliders, solids,
};

use std::collections::{BTreeMap, BTreeSet, HashMap};

use axcenity_assets::Assets;
use axcenity_core::{
    ClassName, Instance, InstanceId, Material, PhysicalProperties, Value, Vector3, World,
};
use rapier3d::glamx::{EulerRot, Quat, Vec3};
use rapier3d::math::Pose;
use rapier3d::pipeline::PhysicsWorld;
use rapier3d::prelude::{
    Collider, ColliderHandle, RigidBodyBuilder, RigidBodyHandle, RigidBodyType,
};
use shapes::Form;

/// How long one physics step is: physics always runs at 60 steps a second.
pub const STEP_SECONDS: f64 = 1.0 / 60.0;

/// Downward pull in studs per second squared (about 1 stud = 0.28 m): the
/// default of the Workspace's `Gravity`, which [`gravity`] reads.
pub const GRAVITY: f32 = 196.2;

/// The most steps one frame may run. The player already caps a frame at
/// 0.1 s (6 steps), so this only matters if a host passes a huge time: the
/// rest is dropped rather than freezing the game while physics catches up.
const MAX_STEPS_PER_FRAME: u32 = 10;

/// A frame that is this close to a whole number of steps counts as reaching
/// it, so rounding in summed frame times (1/144 + 1/144 + …) never costs a step.
const STEP_TOLERANCE: f64 = 1e-6;

/// How quickly spinning slows down by itself. rapier has no rolling
/// resistance, so without this a ball on flat ground would roll forever.
const ANGULAR_DAMPING: f32 = 0.5;

/// How hard the wind pushes: the force on a part is this times its average
/// face area times how much faster the wind moves than the part. With
/// density 1, a 50 studs/s wind blows a thin 2×0.2×2 sheet along the
/// ground, while a 4×1×2 block stays put there and only drifts in the air.
pub const WIND_DRAG: f32 = 2.0;

/// How many steps in a row two parts must be apart before touching again
/// counts as a new touch. Parts resting on each other can lose and regain
/// contact for a single step; this stops that from firing `Touched` again,
/// while a real bounce (apart for many steps) still does.
const SEPARATE_STEPS: u32 = 3;

/// Owns the rapier simulation and remembers which body belongs to which Part.
pub struct Physics {
    rapier: PhysicsWorld,
    /// A `BTreeMap` (sorted) rather than a `HashMap` so bodies are always
    /// visited in the same order, which keeps the simulation repeatable.
    parts: BTreeMap<InstanceId, Body>,
    /// Time passed that hasn't been simulated yet, always less than one step
    /// after [`step`](Self::step).
    unsimulated_seconds: f64,
    /// Which part each collider belongs to, to name the parts in a contact.
    owners: HashMap<ColliderHandle, InstanceId>,
    /// Pairs of parts that count as touching (smaller ID first), with how
    /// many steps in a row they've been apart (see [`SEPARATE_STEPS`]).
    touching: BTreeMap<(InstanceId, InstanceId), u32>,
    /// False until the first step has run. Contacts found in that step were
    /// there when the game started, so they aren't reported (decided in Phase 7).
    started: bool,
    /// Pairs that started touching since the last
    /// [`take_touches`](Self::take_touches).
    touches: Vec<(InstanceId, InstanceId)>,
    /// Where MeshParts' models come from.
    assets: Assets,
    /// The joint of each enabled constraint between two bodies.
    joints: BTreeMap<InstanceId, joints::Joint>,
}

/// One Part's collider on its assembly's rapier body, plus the part's
/// properties as physics last saw or wrote them. A property that differs
/// from these was changed by a script.
struct Body {
    /// Its assembly's body, which every part welded to it shares.
    body: RigidBodyHandle,
    collider: ColliderHandle,
    /// Where the part sits on the body: nowhere off for the root part, which
    /// the body is placed at.
    offset: Pose,
    /// True for its assembly's root part (see [`assembly`]).
    root: bool,
    /// Extra colliders for colliding foliage, parented to a Terrain.
    foliage: Vec<ColliderHandle>,
    seen: PartState,
}

/// The properties of a part that physics cares about.
#[derive(Clone, PartialEq)]
pub(crate) struct PartState {
    anchored: bool,
    /// False makes the collider a sensor: nothing bounces off it, but
    /// touches are still found.
    pub(crate) can_collide: bool,
    pub(crate) shape: Form,
    pub(crate) size: Vector3,
    pub(crate) position: Vector3,
    orientation: Vector3,
    /// True if the Workspace's wind pushes the part.
    wind_affected: bool,
    /// True if it adds no mass to the assembly it's welded into.
    massless: bool,
    /// Its `CustomPhysicalProperties`, or its material's.
    pub(crate) physical: PhysicalProperties,
    /// `AssemblyLinearVelocity` and `AssemblyAngularVelocity`.
    velocity: Vector3,
    spin: Vector3,
}

impl PartState {
    /// Reads a Part or MeshPart, or `None` for other classes. A part with a
    /// NaN or infinite Position, Orientation or Size also gives `None`, so
    /// it is left out of physics instead of breaking the simulation.
    pub(crate) fn read(world: &World, id: InstanceId, assets: &Assets) -> Option<Self> {
        let instance = world.get(id)?;
        let vector = |name| instance.property(name).and_then(Value::as_vector3);
        if instance.class() == ClassName::Terrain {
            return Self::read_terrain(world, id, instance, assets);
        }
        let anchored = match instance.property("Anchored") {
            Some(Value::Bool(anchored)) => *anchored,
            _ => return None,
        };
        let shape = match instance.class() {
            ClassName::MeshPart => {
                let mesh_id = instance.property("MeshId").and_then(Value::as_str)?;
                Form::Mesh(assets.model(mesh_id))
            }
            _ => Form::Primitive(instance.property("Shape").and_then(Value::as_part_type)?),
        };
        let can_collide = instance.property("CanCollide").and_then(Value::as_bool) != Some(false);
        let wind_affected =
            instance.property("WindAffected").and_then(Value::as_bool) == Some(true);
        let massless = instance.property("Massless").and_then(Value::as_bool) == Some(true);
        // MeshParts have no Material, so they weigh what Plastic does.
        let material = instance
            .property("Material")
            .and_then(Value::as_material)
            .unwrap_or(Material::Plastic);
        let physical = instance
            .property("CustomPhysicalProperties")
            .and_then(Value::as_physical_properties)
            .flatten()
            .map_or_else(|| material.physical(), PhysicalProperties::clamped);
        let state = Self {
            anchored,
            can_collide,
            shape,
            size: vector("Size")?,
            position: vector("Position")?,
            orientation: vector("Orientation")?,
            wind_affected,
            massless,
            physical,
            velocity: vector("AssemblyLinearVelocity").unwrap_or_default(),
            spin: vector("AssemblyAngularVelocity").unwrap_or_default(),
        };
        // A NaN or infinite velocity is ignored, not simulated.
        let placed = [state.size, state.position, state.orientation]
            .iter()
            .all(|v| finite(*v));
        placed.then_some(Self {
            velocity: Some(state.velocity)
                .filter(|v| finite(*v))
                .unwrap_or_default(),
            spin: Some(state.spin).filter(|v| finite(*v)).unwrap_or_default(),
            ..state
        })
    }

    /// A Terrain: always anchored, never turned, colliding as its heightfield.
    fn read_terrain(
        world: &World,
        id: InstanceId,
        instance: &Instance,
        assets: &Assets,
    ) -> Option<Self> {
        let vector = |name| instance.property(name).and_then(Value::as_vector3);
        let data = world.terrain(id)?.clone();
        let state = Self {
            anchored: true,
            can_collide: instance.property("CanCollide").and_then(Value::as_bool) != Some(false),
            shape: Form::Terrain(shapes::TerrainShape {
                foliage_models: shapes::colliding_model_keys(&data, assets),
                data,
            }),
            size: vector("Size")?,
            position: vector("Position")?,
            orientation: Vector3::default(),
            wind_affected: false,
            massless: false,
            physical: PhysicalProperties::LEGACY,
            velocity: Vector3::default(),
            spin: Vector3::default(),
        };
        [state.size, state.position]
            .iter()
            .all(|v| finite(*v))
            .then_some(state)
    }

    pub(crate) fn pose(&self) -> Pose {
        Pose::from_parts(
            to_vec3(self.position),
            orientation_to_quat(self.orientation),
        )
    }

    fn body_type(&self) -> RigidBodyType {
        if self.anchored {
            RigidBodyType::Fixed
        } else {
            RigidBodyType::Dynamic
        }
    }
}

impl Physics {
    /// Physics without models: MeshParts collide as their boxes. Fine for
    /// worlds without MeshParts, such as in tests.
    pub fn new() -> Self {
        Self::with_assets(Assets::default())
    }

    /// Physics whose MeshParts collide as their models from `assets`.
    pub fn with_assets(assets: Assets) -> Self {
        let mut rapier = PhysicsWorld::new();
        rapier.gravity = Vec3::new(0.0, -GRAVITY, 0.0);
        rapier.integration_parameters.dt = STEP_SECONDS as f32;
        Self {
            rapier,
            parts: BTreeMap::new(),
            unsimulated_seconds: 0.0,
            owners: HashMap::new(),
            touching: BTreeMap::new(),
            started: false,
            touches: Vec::new(),
            assets,
            joints: BTreeMap::new(),
        }
    }

    /// Frame step 3: catches up with script changes, then runs as many fixed
    /// steps as fit in the time passed (0 to N), writing positions back to
    /// `world` after each one. Returns how many steps ran.
    pub fn step(&mut self, world: &mut World, seconds: f64) -> u32 {
        self.sync_from_world(world);
        let wind = global_wind(world);
        self.rapier.gravity = Vec3::new(0.0, -gravity(world), 0.0);

        // `max` also turns NaN into 0, so a bad time can't stop physics forever.
        self.unsimulated_seconds += seconds.max(0.0);
        let mut steps = 0;
        while self.unsimulated_seconds + STEP_TOLERANCE >= STEP_SECONDS {
            if steps == MAX_STEPS_PER_FRAME {
                self.unsimulated_seconds = 0.0;
                break;
            }
            self.blow(wind);
            joints::drive(&mut self.rapier, &mut self.joints);
            // The hook only runs for colliders with friction or
            // elasticity weights (see `material`).
            self.rapier.step_with_events(&material::Weighted, &());
            self.write_to_world(world);
            self.find_touches();
            self.unsimulated_seconds = (self.unsimulated_seconds - STEP_SECONDS).max(0.0);
            steps += 1;
        }
        steps
    }

    /// True if `id` currently has a body in the simulation.
    pub fn has_body(&self, id: InstanceId) -> bool {
        self.parts.contains_key(&id)
    }

    /// Removes and returns the pairs of parts that started touching since the
    /// last call, in a fixed order. Each pair is listed once; frame step 4
    /// fires `Touched` on both parts.
    pub fn take_touches(&mut self) -> Vec<(InstanceId, InstanceId)> {
        std::mem::take(&mut self.touches)
    }

    /// How many parts are being simulated (anchored ones included).
    pub fn body_count(&self) -> usize {
        self.parts.len()
    }

    /// Adds new Parts, removes ones that left the Workspace or were destroyed,
    /// applies anything a script changed since the last step, and welds or
    /// unwelds assemblies.
    fn sync_from_world(&mut self, world: &World) {
        let current: BTreeMap<InstanceId, PartState> = world
            .descendants(world.workspace())
            .into_iter()
            .filter_map(|id| Some((id, PartState::read(world, id, &self.assets)?)))
            .collect();
        let ids: Vec<InstanceId> = current.keys().copied().collect();
        let groups: Vec<(Vec<InstanceId>, InstanceId)> =
            assembly::group(&ids, &assembly::welds(world))
                .into_iter()
                .map(|members| {
                    let root = assembly::root(world, &members);
                    (members, root)
                })
                .collect();
        let mut anything_changed = false;

        // An assembly is built again when a part joins or leaves it, its
        // root changes, or a script moves or (un)anchors a welded part, so
        // the welds hold the parts where they are now. A part alone keeps
        // its body (see `apply_change`).
        let mut sharing: HashMap<RigidBodyHandle, usize> = HashMap::new();
        for part in self.parts.values() {
            *sharing.entry(part.body).or_default() += 1;
        }
        let rebuild: Vec<bool> = groups
            .iter()
            .map(|(members, root)| !self.is_kept(members, *root, &current, &sharing))
            .collect();

        // Take out the parts that are gone or rebuilt, with their bodies.
        // Their last state tells the new body how fast to start.
        let leaving = self
            .parts
            .keys()
            .filter(|id| !current.contains_key(id))
            .copied();
        let rebuilt = groups
            .iter()
            .zip(&rebuild)
            .filter(|(_, rebuild)| **rebuild)
            .flat_map(|((members, _), _)| members.iter().copied());
        let doomed: Vec<InstanceId> = leaving.chain(rebuilt).collect();
        let mut old: HashMap<InstanceId, PartState> = HashMap::new();
        for id in doomed {
            let Some(part) = self.parts.remove(&id) else {
                continue;
            };
            // `true` also removes the body's colliders. Welded parts share a
            // body, so it may be gone already.
            self.rapier.remove_body_with_colliders(part.body, true);
            self.owners.remove(&part.collider);
            for handle in part.foliage {
                self.owners.remove(&handle);
            }
            old.insert(id, part.seen);
            anything_changed = true;
        }
        // A part that comes back (moved into the Workspace again) touches anew.
        self.touching
            .retain(|(a, b), _| current.contains_key(a) && current.contains_key(b));

        for ((members, root), rebuild) in groups.iter().zip(rebuild) {
            if rebuild {
                self.build_assembly(members, *root, &current, &old);
                continue;
            }
            let welded = members.len() > 1;
            for id in members {
                let state = &current[id];
                let part = &self.parts[id];
                if part.seen == *state {
                    continue;
                }
                let mut part = self.parts.remove(id).expect("just found");
                let weightless = welded && !part.root && state.massless;
                self.owners.remove(&part.collider);
                for handle in &part.foliage {
                    self.owners.remove(handle);
                }
                self.apply_change(&mut part, state.clone(), weightless);
                self.owners.insert(part.collider, *id);
                self.attach_foliage(*id, &mut part);
                self.parts.insert(*id, part);
                anything_changed = true;
            }
        }

        // Joints go between the bodies just made, so they come after them.
        anything_changed |= joints::sync(world, &mut self.rapier, &self.parts, &mut self.joints);

        // Parts at rest "sleep" and stop being simulated. If a script moved
        // or removed something, a sleeping part may have lost what it was
        // resting on, so wake everything and let it settle again. Simple, and
        // cheap enough for v1 scene sizes.
        if anything_changed {
            self.rapier.wake_up_all(true);
        }
    }

    /// True if the parts `members`, with this `root`, already make up one
    /// body by themselves that a change to them doesn't rebuild.
    fn is_kept(
        &self,
        members: &[InstanceId],
        root: InstanceId,
        current: &BTreeMap<InstanceId, PartState>,
        sharing: &HashMap<RigidBodyHandle, usize>,
    ) -> bool {
        let Some(body) = self.parts.get(&members[0]).map(|part| part.body) else {
            return false;
        };
        if sharing.get(&body) != Some(&members.len()) {
            return false;
        }
        members.iter().all(|id| {
            let Some(part) = self.parts.get(id) else {
                return false;
            };
            let (seen, now) = (&part.seen, &current[id]);
            part.body == body
                && (members.len() == 1
                    || (part.root == (*id == root)
                        && seen.position == now.position
                        && seen.orientation == now.orientation
                        && seen.anchored == now.anchored))
        })
    }

    /// Makes one body for the parts `members`, placed at `root`, with each
    /// part's collider where the part is now. It's anchored if any part is.
    /// `old` has the last state of parts that had a body before.
    fn build_assembly(
        &mut self,
        members: &[InstanceId],
        root: InstanceId,
        current: &BTreeMap<InstanceId, PartState>,
        old: &HashMap<InstanceId, PartState>,
    ) {
        let root_state = &current[&root];
        let anchored = members.iter().any(|id| current[id].anchored);
        // It keeps going at the root's speed, which physics wrote to every
        // welded part. A part that a script moved or (un)anchored starts
        // the assembly from rest (decided in Phase 6), and a velocity a
        // script set throws it.
        let (mut velocity, mut spin) = (root_state.velocity, root_state.spin);
        let restarted = members.iter().any(|id| {
            let now = &current[id];
            old.get(id).is_some_and(|seen| {
                seen.position != now.position
                    || seen.orientation != now.orientation
                    || seen.anchored != now.anchored
            })
        });
        if restarted {
            (velocity, spin) = (Vector3::default(), Vector3::default());
        }
        for id in members {
            let now = &current[id];
            if let Some(seen) = old.get(id) {
                if seen.velocity != now.velocity {
                    velocity = now.velocity;
                }
                if seen.spin != now.spin {
                    spin = now.spin;
                }
            }
        }
        let body_type = if anchored {
            RigidBodyType::Fixed
        } else {
            RigidBodyType::Dynamic
        };
        let root_pose = root_state.pose();
        let body = RigidBodyBuilder::new(body_type)
            .pose(root_pose)
            // Continuous collision detection: fast parts can't skip through
            // thin ones between two steps.
            .ccd_enabled(true)
            .angular_damping(ANGULAR_DAMPING)
            .linvel(to_vec3(velocity))
            .angvel(to_vec3(spin))
            .build();
        let body = self.rapier.bodies.insert(body);
        let welded = members.len() > 1;
        for &id in members {
            let state = current[&id].clone();
            let is_root = id == root;
            let offset = if is_root {
                Pose::IDENTITY
            } else {
                root_pose.inverse() * state.pose()
            };
            let collider = part_collider(&state, offset, welded && !is_root && state.massless);
            let rapier = &mut self.rapier;
            let collider = rapier
                .colliders
                .insert_with_parent(collider, body, &mut rapier.bodies);
            self.owners.insert(collider, id);
            let mut part = Body {
                body,
                collider,
                offset,
                root: is_root,
                foliage: Vec::new(),
                seen: state,
            };
            self.attach_foliage(id, &mut part);
            self.parts.insert(id, part);
        }
    }

    /// Parents colliding foliage colliders to a terrain body, replacing any
    /// that were there. Other classes have none.
    fn attach_foliage(&mut self, id: InstanceId, part: &mut Body) {
        for handle in part.foliage.drain(..) {
            let rapier = &mut self.rapier;
            rapier.colliders.remove(
                handle,
                &mut rapier.islands,
                &mut rapier.bodies,
                &mut rapier.soft_bodies,
                true,
            );
            self.owners.remove(&handle);
        }
        let Form::Terrain(terrain) = &part.seen.shape else {
            return;
        };
        let colliders =
            shapes::foliage_colliders(&terrain.data, to_vec3(part.seen.size), &self.assets);
        for collider in colliders {
            let handle = self.rapier.colliders.insert_with_parent(
                collider,
                part.body,
                &mut self.rapier.bodies,
            );
            self.owners.insert(handle, id);
            part.foliage.push(handle);
        }
    }

    /// Applies what a script changed on a part that already has a body. A
    /// welded part (in an assembly that's kept) can only change its
    /// collider and the assembly's speed; moving or anchoring one rebuilds
    /// the assembly instead. `weightless` is true for a Massless part that
    /// isn't its assembly's root.
    fn apply_change(&mut self, part: &mut Body, state: PartState, weightless: bool) {
        let old = std::mem::replace(&mut part.seen, state);
        let state = &part.seen;

        if old.shape != state.shape || old.size != state.size {
            let rapier = &mut self.rapier;
            rapier.colliders.remove(
                part.collider,
                &mut rapier.islands,
                &mut rapier.bodies,
                &mut rapier.soft_bodies,
                true,
            );
            let collider = part_collider(state, part.offset, weightless);
            part.collider =
                rapier
                    .colliders
                    .insert_with_parent(collider, part.body, &mut rapier.bodies);
        } else if let Some(collider) = self.rapier.colliders.get_mut(part.collider) {
            if old.can_collide != state.can_collide {
                collider.set_sensor(!state.can_collide);
            }
            if old.physical != state.physical || old.massless != state.massless {
                material::apply(collider, state.physical);
                if weightless {
                    collider.set_density(0.0);
                }
            }
        }

        let Some(body) = self.rapier.bodies.get_mut(part.body) else {
            return;
        };
        if old.anchored != state.anchored {
            body.set_body_type(state.body_type(), true);
        }
        let moved = old.position != state.position || old.orientation != state.orientation;
        if moved {
            body.set_position(state.pose(), true);
        }
        // A teleported part starts again from rest (decided in Phase 6), and
        // so does a part that was just released.
        if moved || old.anchored != state.anchored {
            body.set_linvel(Vec3::ZERO, true);
            body.set_angvel(Vec3::ZERO, true);
        }
        // A velocity set at the same time still throws it.
        if old.velocity != state.velocity {
            body.set_linvel(to_vec3(state.velocity), true);
        }
        if old.spin != state.spin {
            body.set_angvel(to_vec3(state.spin), true);
        }
    }

    /// Sets the wind's push on every unanchored part for the next step. A
    /// body's force stays until it's reset, so every body's is reset first,
    /// in case its WindAffected was just turned off or the wind stopped.
    /// Each welded part pushes its assembly by its own faces.
    fn blow(&mut self, wind: Vec3) {
        for part in self.parts.values() {
            if let Some(body) = self.rapier.bodies.get_mut(part.body)
                && body.is_dynamic()
            {
                body.reset_forces(false);
            }
        }
        if wind == Vec3::ZERO {
            return;
        }
        for part in self.parts.values() {
            let Some(body) = self.rapier.bodies.get_mut(part.body) else {
                continue;
            };
            if !body.is_dynamic() || !part.seen.wind_affected {
                continue;
            }
            let size = part.seen.size;
            let area = (size.x * size.y + size.y * size.z + size.z * size.x) / 3.0;
            // Waking the part means a resting part feels the wind too.
            body.add_force((wind - body.linvel()) * (WIND_DRAG * area), true);
        }
    }

    /// Compares the parts touching after this step with those touching
    /// before, and records the pairs that are new.
    fn find_touches(&mut self) {
        // Only contacts the solver acts on count, not pairs whose bounding
        // boxes merely overlap. A set, so the order is the same every run.
        // Parts with CanCollide off are sensors: their overlaps count too.
        let pair_of = |one: &ColliderHandle, two: &ColliderHandle| {
            let a = *self.owners.get(one)?;
            let b = *self.owners.get(two)?;
            Some(if a < b { (a, b) } else { (b, a) })
        };
        let contacts = self
            .rapier
            .contact_pairs()
            .filter(|pair| pair.has_any_active_contact())
            .filter_map(|pair| pair_of(&pair.collider1, &pair.collider2));
        let overlaps = self
            .rapier
            .intersection_pairs()
            .filter(|(_, _, _, _, intersecting)| *intersecting)
            .filter_map(|(one, _, two, _, _)| pair_of(&one, &two));
        let now: BTreeSet<(InstanceId, InstanceId)> = contacts.chain(overlaps).collect();

        for &(a, b) in &now {
            // `insert` returns the old value: `None` means the pair is new.
            let was_touching = self.touching.insert((a, b), 0).is_some();
            if !was_touching && self.started {
                self.touches.push((a, b));
            }
        }
        // Pairs that are apart this step stop counting once they've been
        // apart for long enough.
        self.touching.retain(|pair, steps_apart| {
            if !now.contains(pair) {
                *steps_apart += 1;
            }
            *steps_apart < SEPARATE_STEPS
        });
        self.started = true;
    }

    /// Copies where each moving part is now, and how fast it moves, into
    /// its `Position`, `Orientation` and velocities, and remembers the
    /// values written. Welded parts share their assembly's velocities.
    fn write_to_world(&mut self, world: &mut World) {
        for (&id, part) in &mut self.parts {
            let Some(body) = self.rapier.bodies.get(part.body) else {
                continue;
            };
            // An anchored part doesn't move, whatever was set; a part that
            // falls asleep stops. Either way its velocities become zero.
            let (velocity, spin) = if body.is_dynamic() {
                (to_vector3(body.linvel()), to_vector3(body.angvel()))
            } else {
                (Vector3::default(), Vector3::default())
            };
            if velocity != part.seen.velocity {
                part.seen.velocity = velocity;
                let _ = world.set_property(id, "AssemblyLinearVelocity", Value::Vector3(velocity));
            }
            if spin != part.seen.spin {
                part.seen.spin = spin;
                let _ = world.set_property(id, "AssemblyAngularVelocity", Value::Vector3(spin));
            }
            if !body.is_dynamic() || body.is_sleeping() {
                continue;
            }
            let pose = if part.root {
                *body.position()
            } else {
                body.position() * part.offset
            };
            let position = to_vector3(pose.translation);
            let orientation = quat_to_orientation(pose.rotation);
            if position != part.seen.position {
                part.seen.position = position;
                // The part was checked to exist this frame, and scripts don't
                // run between physics steps, so this can't fail.
                let _ = world.set_property(id, "Position", Value::Vector3(position));
            }
            if orientation != part.seen.orientation {
                part.seen.orientation = orientation;
                let _ = world.set_property(id, "Orientation", Value::Vector3(orientation));
            }
        }
    }
}

/// The collider for a part, `offset` from its assembly's body. A
/// `weightless` one (Massless, and not the root) adds no mass.
fn part_collider(state: &PartState, offset: Pose, weightless: bool) -> Collider {
    let mut collider = shapes::collider(&state.shape, to_vec3(state.size));
    // A turned shape (a cylinder) keeps its turn within the part.
    collider.set_position(offset * *collider.position());
    collider.set_sensor(!state.can_collide);
    material::apply(&mut collider, state.physical);
    if weightless {
        collider.set_density(0.0);
    }
    collider
}

impl Default for Physics {
    fn default() -> Self {
        Self::new()
    }
}

/// The Workspace's `GlobalWind`, or none if it isn't a finite vector.
fn global_wind(world: &World) -> Vec3 {
    world
        .get(world.workspace())
        .and_then(|workspace| workspace.property("GlobalWind"))
        .and_then(Value::as_vector3)
        .map(to_vec3)
        .filter(|wind| wind.is_finite())
        .unwrap_or(Vec3::ZERO)
}

/// The Workspace's `Gravity` in studs a second squared, or [`GRAVITY`] if
/// it isn't a finite number. The character controllers fall by it too.
pub fn gravity(world: &World) -> f32 {
    world
        .get(world.workspace())
        .and_then(|workspace| workspace.property("Gravity"))
        .and_then(Value::as_number)
        .map(|gravity| gravity as f32)
        .filter(|gravity| gravity.is_finite())
        .unwrap_or(GRAVITY)
}

fn finite(v: Vector3) -> bool {
    v.x.is_finite() && v.y.is_finite() && v.z.is_finite()
}

pub(crate) fn to_vec3(v: Vector3) -> Vec3 {
    Vec3::new(v.x, v.y, v.z)
}

pub(crate) fn to_vector3(v: Vec3) -> Vector3 {
    Vector3::new(v.x, v.y, v.z)
}

/// `Orientation` is degrees around X, Y and Z, applied Z first, then X, then
/// Y (the same rule as the renderer in `player/src/scene.rs`).
fn orientation_to_quat(orientation: Vector3) -> Quat {
    let [x, y, z] = [orientation.x, orientation.y, orientation.z].map(f32::to_radians);
    Quat::from_euler(EulerRot::YXZ, y, x, z)
}

fn quat_to_orientation(rotation: Quat) -> Vector3 {
    let (y, x, z) = rotation.to_euler(EulerRot::YXZ);
    Vector3::new(x.to_degrees(), y.to_degrees(), z.to_degrees())
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod assembly_tests;

#[cfg(test)]
mod joint_tests;
