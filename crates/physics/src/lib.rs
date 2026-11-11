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
//! Physics never gets told about changes. Instead, before stepping it checks
//! every Part's revision against the one physics last saw or wrote. Only a
//! part something else changed is read and compared, so a script that moves,
//! anchors, resizes or deletes a part is noticed on the next frame, and
//! bodies physics moves itself aren't read again.
//!
//! After each step it also notes which parts started touching, for the
//! `Touched` event, and which stopped, for `TouchEnded`. The host collects
//! them with [`Physics::take_touches`] and [`Physics::take_touch_ends`].
//! Parts with `CanTouch` off touch nothing.
//!
//! Each part is in a collision group (its `CollisionGroup`, see
//! [`axcenity_core::CollisionGroups`]); parts in two groups set not to
//! collide pass through each other and never touch.
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
//! constraints join two parts' bodies with a rapier joint (see [`joints`]),
//! as do welds that can break. One held past its `BreakForce` or
//! `BreakTorque` turns off; the host collects it with
//! [`Physics::take_broken`] and fires `Broken`.
//! LinearVelocity, AngularVelocity, VectorForce, Torque, AlignPosition and
//! AlignOrientation push or turn a part's body every step (see `movers`).
//! A VehicleSeat drives the body it's welded into on its VehicleWheels
//! (see `vehicles`).
//!
//! [`cast_ray`], [`raycast`], [`shapecast`], the overlap queries and
//! [`debug_colliders`] read the world the same way the simulation does,
//! without stepping it or writing back.

pub mod assembly;
mod blend;
mod fracture;
pub mod joints;
mod mass;
mod material;
mod movers;
mod owners;
mod query;
mod shapes;
mod sync;
#[cfg(feature = "parallel")]
pub mod threads;
mod vehicles;

pub use blend::{Blend, interpolates};
pub use mass::Mass;
pub use query::{
    CastShape, ColliderOutline, Column, QueryParams, RayHit, Solid, cast_ray, cast_ray_excluding,
    debug_colliders, part_solids, parts_in_box, parts_in_part, parts_in_radius, raycast, shapecast,
    solids,
};
pub use shapes::prepare_precise;

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use axcenity_assets::Assets;
use axcenity_core::tool::Carried;
use axcenity_core::{
    ClassName, Instance, InstanceId, Material, PhysicalProperties, Value, Vector3, World,
};
use rapier3d::glamx::{EulerRot, Quat, Vec3};
use rapier3d::math::Pose;
use rapier3d::pipeline::PhysicsWorld;
use rapier3d::prelude::{
    Collider, ColliderHandle, Group, InteractionGroups, InteractionTestMode, RigidBody,
    RigidBodyHandle, RigidBodyType,
};
use shapes::{Fidelity, Form};

/// How long one physics step is: physics always runs at 60 steps a second.
pub const STEP_SECONDS: f64 = 1.0 / 60.0;

/// Downward pull in studs per second squared (about 1 stud = 0.28 m): the
/// default of the Workspace's `Gravity`, which [`gravity`] reads.
pub const GRAVITY: f32 = 196.2;

/// How far, in studs, two touching colliders may move against each other
/// before rapier works out their contact points again instead of keeping
/// last step's (rapier's default is 0.05). Kept points carry their impulses
/// over, so stacks settle: at 0.05 a 2x2x2 block on a stack of five fell in
/// 4 of 6 slightly different builds, at 0.1 in none, at no measured cost
/// (docs/benchmarks.md, "Physics step, part 3"). At 0.2 parts landing on
/// edited terrain missed the edit.
const CONTACT_RECYCLE_DISTANCE: f32 = 0.1;

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
    /// The threads rapier's step runs on (see [`threads`]).
    #[cfg(feature = "parallel")]
    pool: Option<threads::Pool>,
    /// A `BTreeMap` (sorted) rather than a `HashMap` so bodies are always
    /// visited in the same order, which keeps the simulation repeatable.
    parts: BTreeMap<InstanceId, Body>,
    /// Time passed that hasn't been simulated yet, always less than one step
    /// after [`step`](Self::step).
    unsimulated_seconds: f64,
    /// Which part each collider belongs to, to name the parts in a contact.
    owners: owners::Owners,
    /// Pairs of parts that count as touching (smaller ID first), with how
    /// many steps in a row they've been apart (see [`SEPARATE_STEPS`]).
    /// Sorted by pair, so `find_touches` merges it with a step's contacts.
    touching: Vec<((InstanceId, InstanceId), u32)>,
    /// False until the first step has run. Contacts found in that step were
    /// there when the game started, so they aren't reported (decided in Phase 7).
    started: bool,
    /// Pairs that started touching since the last
    /// [`take_touches`](Self::take_touches).
    touches: Vec<(InstanceId, InstanceId)>,
    /// The bodies given a force or torque for the last step (by the wind
    /// or movers), which the next step resets.
    pushed: Vec<RigidBodyHandle>,
    /// Pairs that stopped touching since the last
    /// [`take_touch_ends`](Self::take_touch_ends).
    touch_ends: Vec<(InstanceId, InstanceId)>,
    /// Where MeshParts' models come from.
    assets: Assets,
    /// The joint of each enabled constraint between two bodies.
    joints: BTreeMap<InstanceId, joints::Joint>,
    /// The enabled movers, read again every frame.
    movers: Vec<movers::Mover>,
    /// What each velocity and align mover learned about contacts and joints.
    learned: BTreeMap<InstanceId, movers::Learned>,
    /// The cars, read again every frame, and how far each wheel has rolled.
    vehicles: Vec<vehicles::Vehicle>,
    rolling: BTreeMap<InstanceId, vehicles::Rolling>,
    /// The bodies of the Ragdolls that are on, at the last sync.
    ragdoll_bodies: HashSet<InstanceId>,
    /// Constraints that broke since the last
    /// [`take_broken`](Self::take_broken).
    broken: Vec<InstanceId>,
    /// Parts with a generated Fracture, at the last sync.
    breakables: BTreeMap<InstanceId, fracture::Breakable>,
    /// The pieces of each shatter, until their `DebrisLifetime` is up.
    debris: Vec<fracture::Debris>,
    /// Fractures that shattered since the last
    /// [`take_shattered`](Self::take_shattered).
    shattered: Vec<InstanceId>,
    /// Parts held still as if anchored, whatever their `Anchored` says:
    /// another device moves them (Phase 66, network ownership).
    held: BTreeSet<InstanceId>,
    /// While streaming (Phase 80): the points parts stay awake near, and
    /// how near.
    far_sleep: Option<(Vec<Vector3>, f32)>,
    /// Bodies put to sleep for being far, to wake when they come near.
    far_asleep: HashSet<RigidBodyHandle>,
    /// Bodies the write-back looks at besides the awake ones: those the
    /// last sync built or changed, and those awake at the last write-back
    /// (which may have fallen asleep since, or stopped being dynamic).
    unwritten: HashSet<RigidBodyHandle>,
    /// What the last sync was made from; `None` before the first.
    synced: Option<sync::Synced>,
    /// `assets`' generation at the last sync: a model that changes is the
    /// only way a MeshPart changes without its Instance doing so.
    assets_generation: u64,
    stats: SyncStats,
    /// Where moving parts are drawn between steps (see [`blend`]).
    blending: blend::Blending,
}

/// How much the last [`Physics::step`] read from the world to catch up, for
/// tests and profiling.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SyncStats {
    /// How many parts' properties were read.
    pub parts_read: usize,
    /// How many bodies were built, one per assembly.
    pub bodies_built: usize,
    /// True if all of the Workspace was read again.
    pub full_sync: bool,
    /// How many parts the write-back looked at.
    pub parts_written: usize,
}

/// How a body moved before a step: its velocities and center of mass.
#[derive(Clone, Copy)]
struct Motion {
    velocity: Vec3,
    spin: Vec3,
    center: Vec3,
}

impl Motion {
    /// How fast the body's point `at` moves.
    fn at(self, at: Vec3) -> Vec3 {
        self.velocity + self.spin.cross(at - self.center)
    }
}

/// The fastest impact on a breakable part in a step.
struct Impact {
    point: Vec3,
    speed: f32,
    /// The momentum it brought: its speed times the reduced mass of the two.
    force: f32,
    /// The body that hit it and how it moved before, so it carries on.
    hitter: Option<(RigidBodyHandle, Vec3)>,
    /// How the part moved before.
    velocity: Vec3,
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
    /// True if other parts share its body: a change to it rebuilds them.
    welded: bool,
    /// Its Instance's revision when `seen` was last compared with it.
    revision: u64,
    /// Its Instance's [`revision_without_motion`] when `seen` was last
    /// read whole: while it's the same, only the part's motion can have
    /// changed, so a part a script moves every frame isn't read whole.
    ///
    /// [`revision_without_motion`]: Instance::revision_without_motion
    static_revision: u64,
    /// True once its pose was written while its body slept: a body can
    /// fall asleep in any of a frame's steps, so the first write-back after
    /// that still copies where it came to rest.
    settled: bool,
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
    /// Its collision group's bit, and the groups it collides with.
    pub(crate) groups: Groups,
    /// False: it touches nothing (`CanTouch`).
    can_touch: bool,
    /// False: scripts' queries pass it by (`CanQuery`).
    pub(crate) can_query: bool,
}

/// A part's collision group as bits: its own, and every group it collides
/// with (see [`axcenity_core::CollisionGroups::mask`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Groups {
    member: u32,
    filter: u32,
}

impl Groups {
    /// The bits of the group `name`, which is Default if it isn't registered.
    pub(crate) fn of(world: &World, name: &str) -> Self {
        let groups = world.collision_groups();
        Self {
            member: 1 << groups.index(name),
            filter: groups.mask(name),
        }
    }

    /// True if parts in these two groups collide.
    pub(crate) fn collide(self, other: Self) -> bool {
        self.member & other.filter != 0 && other.member & self.filter != 0
    }

    pub(crate) fn interaction(self) -> InteractionGroups {
        InteractionGroups::new(
            Group::from_bits_retain(self.member),
            Group::from_bits_retain(self.filter),
            InteractionTestMode::And,
        )
    }
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
        // A Tool put away in an Inventory is out of the world; one in hand
        // follows the hand, held still and walked through (Phase 45).
        let carried = axcenity_core::tool::carried(world, id);
        if carried == Some(Carried::Stored) {
            return None;
        }
        // The parts kept inside a UnionOperation are out of the world too
        // (Phase 50).
        if axcenity_core::csg::in_union(world, id) {
            return None;
        }
        let held = carried == Some(Carried::Held);
        let anchored = anchored || held;
        let shape = match instance.class() {
            class if class.has_model() => {
                let mesh_id = instance.property("MeshId").and_then(Value::as_str)?;
                let fidelity = match instance.property("CollisionFidelity") {
                    Some(Value::EnumItem(item)) => Fidelity::named(&item.item),
                    _ => Fidelity::Hull,
                };
                Form::Mesh(assets.model_blocking(mesh_id), fidelity)
            }
            _ => Form::Primitive(instance.property("Shape").and_then(Value::as_part_type)?),
        };
        let can_collide =
            !held && instance.property("CanCollide").and_then(Value::as_bool) != Some(false);
        let wind_affected =
            instance.property("WindAffected").and_then(Value::as_bool) == Some(true);
        let massless = instance.property("Massless").and_then(Value::as_bool) == Some(true);
        let flag = |name| instance.property(name).and_then(Value::as_bool) != Some(false);
        let group = instance
            .property("CollisionGroup")
            .and_then(Value::as_str)
            .unwrap_or(axcenity_core::DEFAULT_GROUP);
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
            groups: Groups::of(world, group),
            can_touch: flag("CanTouch"),
            can_query: flag("CanQuery"),
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
        let mut state = Self::read_terrain_ground(world, id, instance)?;
        if let Form::Terrain(terrain) = &mut state.shape {
            terrain.foliage_models = shapes::colliding_model_keys(&terrain.data, assets);
        }
        Some(state)
    }

    /// A Terrain's state without its colliding models' keys, which take a
    /// walk over all its foliage and only matter for noticing a change.
    pub(crate) fn read_terrain_ground(
        world: &World,
        id: InstanceId,
        instance: &Instance,
    ) -> Option<Self> {
        let vector = |name| instance.property(name).and_then(Value::as_vector3);
        let data = world.terrain(id)?.clone();
        let state = Self {
            anchored: true,
            can_collide: instance.property("CanCollide").and_then(Value::as_bool) != Some(false),
            shape: Form::Terrain(shapes::TerrainShape {
                foliage_models: Vec::new(),
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
            groups: Groups::of(world, axcenity_core::DEFAULT_GROUP),
            can_touch: true,
            can_query: true,
        };
        [state.size, state.position]
            .iter()
            .all(|v| finite(*v))
            .then_some(state)
    }

    /// `self`, with the part's motion ([`MOTION_PROPERTIES`]) read again
    /// from `instance`, for a part none of whose other properties changed
    /// since `self` was [`read`](Self::read). The same as reading it whole.
    ///
    /// [`MOTION_PROPERTIES`]: axcenity_core::MOTION_PROPERTIES
    pub(crate) fn moved(&self, instance: &Instance) -> Option<Self> {
        let vector = |name| instance.property(name).and_then(Value::as_vector3);
        let position = vector("Position")?;
        let orientation = vector("Orientation")?;
        if !finite(position) || !finite(orientation) {
            return None;
        }
        // A NaN or infinite velocity is ignored, as in `read`.
        let velocity = |name| vector(name).filter(|v| finite(*v)).unwrap_or_default();
        Some(Self {
            position,
            orientation,
            velocity: velocity("AssemblyLinearVelocity"),
            spin: velocity("AssemblyAngularVelocity"),
            ..self.clone()
        })
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
        rapier
            .integration_parameters
            .normalized_contact_recycle_distance = CONTACT_RECYCLE_DISTANCE;
        Self {
            rapier,
            #[cfg(feature = "parallel")]
            pool: threads::shared(),
            parts: BTreeMap::new(),
            unsimulated_seconds: 0.0,
            owners: owners::Owners::default(),
            touching: Vec::new(),
            started: false,
            touches: Vec::new(),
            pushed: Vec::new(),
            touch_ends: Vec::new(),
            assets,
            joints: BTreeMap::new(),
            movers: Vec::new(),
            learned: BTreeMap::new(),
            vehicles: Vec::new(),
            rolling: BTreeMap::new(),
            ragdoll_bodies: HashSet::new(),
            broken: Vec::new(),
            breakables: BTreeMap::new(),
            debris: Vec::new(),
            shattered: Vec::new(),
            held: BTreeSet::new(),
            far_sleep: None,
            far_asleep: HashSet::new(),
            unwritten: HashSet::new(),
            synced: None,
            assets_generation: 0,
            stats: SyncStats::default(),
            blending: blend::Blending::default(),
        }
    }

    /// World streaming (Phase 80): from the next step on, unanchored parts
    /// further than `radius` studs from every point of `focuses` sleep, and
    /// wake when one comes in range. Empty `focuses`: nothing is put to
    /// sleep (a server nobody is on still steps). `None` turns it off.
    pub fn set_far_sleep(&mut self, far: Option<(Vec<Vector3>, f32)>) {
        self.far_sleep = far;
        if self.far_sleep.is_none() {
            for handle in std::mem::take(&mut self.far_asleep) {
                if let Some(body) = self.rapier.bodies.get_mut(handle) {
                    body.wake_up(true);
                }
            }
        }
    }

    /// Puts far bodies to sleep and wakes those that came back in range.
    fn sleep_far_bodies(&mut self) {
        let Some((focuses, radius)) = &self.far_sleep else {
            return;
        };
        if focuses.is_empty() {
            return;
        }
        let radius_squared = radius * radius;
        // Read first: `get_mut` tells rapier the body changed.
        let mut wake = Vec::new();
        let mut far = Vec::new();
        for (handle, body) in self.rapier.bodies.iter() {
            if !body.is_dynamic() {
                continue;
            }
            let at = body.translation();
            let near = focuses.iter().any(|focus| {
                let (x, y, z) = (focus.x - at.x, focus.y - at.y, focus.z - at.z);
                x * x + y * y + z * z <= radius_squared
            });
            if near {
                if self.far_asleep.contains(&handle) {
                    wake.push(handle);
                }
            } else if !body.is_sleeping() {
                far.push(handle);
            }
        }
        for handle in wake {
            self.far_asleep.remove(&handle);
            self.rapier.wake_up(handle, true);
        }
        for handle in far {
            if let Some(body) = self.rapier.bodies.get_mut(handle) {
                body.sleep();
                self.far_asleep.insert(handle);
            }
        }
    }

    /// Holds `parts` still from the next step on, as if anchored, without
    /// changing their `Anchored`: a multiplayer device leaves to physics
    /// only the assemblies it simulates, and moves the rest as it's told.
    pub fn set_held(&mut self, parts: BTreeSet<InstanceId>) {
        self.held = parts;
    }

    /// Frame step 3: catches up with script changes, then runs as many fixed
    /// steps as fit in the time passed (0 to N), and writes positions back to
    /// `world` once after the last one. Returns how many steps ran.
    pub fn step(&mut self, world: &mut World, seconds: f64) -> u32 {
        profiling::scope!("Physics::step");
        self.stats = SyncStats::default();
        // Before syncing scripts' changes, which could make an unread list
        // of blends wrong.
        self.blending.start_frame();
        self.sync_from_world(world);
        self.sleep_far_bodies();
        let wind = global_wind(world);
        let gravity = Vec3::new(0.0, -gravity(world), 0.0);
        if gravity != self.rapier.gravity {
            self.rapier.gravity = gravity;
            self.rapier.wake_up_all(true);
        }

        // `max` also turns NaN into 0, so a bad time can't stop physics forever.
        self.unsimulated_seconds += seconds.max(0.0);
        let interpolating = blend::interpolates(world);
        let mut steps = 0;
        let mut wheels = BTreeMap::new();
        while self.unsimulated_seconds + STEP_TOLERANCE >= STEP_SECONDS {
            if steps == MAX_STEPS_PER_FRAME {
                self.unsimulated_seconds = 0.0;
                break;
            }
            // The frame's last step: the loop won't come round again.
            let last = steps + 1 == MAX_STEPS_PER_FRAME
                || self.unsimulated_seconds - STEP_SECONDS + STEP_TOLERANCE < STEP_SECONDS;
            if interpolating && last {
                self.remember_before_last_step(&wheels);
            }
            self.blow(wind);
            joints::drive(&mut self.rapier, &mut self.joints);
            // After the wind, which `blow` adds once the forces are reset.
            movers::apply(
                &mut self.rapier.bodies,
                &self.movers,
                self.rapier.gravity,
                &mut self.learned,
            );
            self.pushed.extend(movers::bodies(&self.movers));
            vehicles::drive(&mut self.rapier, &mut self.vehicles, &mut self.rolling);
            let before = self.motions_before_step();
            self.substep();
            self.broken.extend(joints::break_overloaded(
                world,
                &mut self.rapier,
                &mut self.joints,
            ));
            vehicles::place_wheels(&mut self.rapier.bodies, &self.vehicles, &mut wheels);
            self.find_touches();
            self.shatter_hit_parts(world, &before);
            fracture::age(world, &mut self.debris, STEP_SECONDS);
            self.unsimulated_seconds = (self.unsimulated_seconds - STEP_SECONDS).max(0.0);
            steps += 1;
        }
        // Once per frame, not per step: scripts and the renderer only see
        // where things are after the last step, so the steps before it
        // needn't convert and write every moving part.
        if steps > 0 {
            self.write_to_world(world);
            if interpolating {
                self.remember_wheels_before_writing();
            }
            vehicles::write_wheels(world, &mut self.parts, &wheels);
        }
        if !interpolating {
            self.blending.forget();
        }
        steps
    }

    /// rapier's step, on physics' own threads (see [`threads`]). The hook
    /// only runs for colliders with friction or elasticity weights (see
    /// `material`).
    fn substep(&mut self) {
        profiling::scope!("Physics::substep");
        let rapier = &mut self.rapier;
        let mut step = || rapier.step_with_events(&material::Weighted, &());
        #[cfg(feature = "parallel")]
        if let Some(pool) = &self.pool {
            pool.run(step);
            return;
        }
        step();
    }

    /// Runs rapier's step on a pool of its own of `threads` threads, for
    /// tests that compare thread counts.
    #[cfg(all(test, feature = "parallel"))]
    pub(crate) fn use_threads(&mut self, threads: usize) {
        self.pool = threads::make(threads);
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

    /// Removes and returns the pairs of parts that stopped touching since
    /// the last call, in a fixed order: a pair counts once it has been
    /// apart for a few steps (see [`SEPARATE_STEPS`]), or at once when one
    /// of its parts can't touch the other any more (`CanTouch` off, or set
    /// not to collide). Frame step 4 fires `TouchEnded` on both parts.
    pub fn take_touch_ends(&mut self) -> Vec<(InstanceId, InstanceId)> {
        std::mem::take(&mut self.touch_ends)
    }

    /// Removes and returns the joints and welds that broke past their
    /// `BreakForce` or `BreakTorque` since the last call, each once, in the
    /// order they broke. Physics has already turned them off; the host
    /// fires `Broken` on each.
    pub fn take_broken(&mut self) -> Vec<InstanceId> {
        std::mem::take(&mut self.broken)
    }

    /// Replaces the part the Fracture `fracture` is in with its pieces now
    /// (`fracture:Shatter(point, force)`), flying from `point` (the part's
    /// middle if `None`) with up to `force` ÷ the part's mass studs a
    /// second, on top of the part's own velocity. The pieces join the
    /// simulation at the next step. False, changing nothing, if the
    /// Fracture isn't in a part or its pieces aren't generated or can't be
    /// loaded.
    pub fn shatter(
        &mut self,
        world: &mut World,
        fracture: InstanceId,
        point: Option<Vector3>,
        force: f32,
    ) -> bool {
        let part = world
            .get(fracture)
            .and_then(Instance::parent)
            .and_then(|part| world.get(part));
        let vector = |name| part?.property(name).and_then(Value::as_vector3);
        let Some(point) = point.or_else(|| vector("Position")) else {
            return false;
        };
        let velocity = vector("AssemblyLinearVelocity").unwrap_or_default();
        let shattered = fracture::shatter(world, &self.assets, fracture, point, force, velocity);
        let Some(debris) = shattered else {
            return false;
        };
        self.shattered.push(fracture);
        self.debris.push(debris);
        true
    }

    /// Removes and returns the Fractures whose parts shattered since the
    /// last call, in order. The host fires `Shattered` on each.
    pub fn take_shattered(&mut self) -> Vec<InstanceId> {
        std::mem::take(&mut self.shattered)
    }

    /// The pieces of each shatter that hasn't reached its `DebrisLifetime`
    /// yet, with its Fracture.
    pub fn debris(&self) -> Vec<(InstanceId, Vec<InstanceId>)> {
        self.debris
            .iter()
            .map(|shatter| (shatter.fracture, shatter.pieces().collect()))
            .collect()
    }

    /// How every dynamic body moves before a step, when a breakable part
    /// might be hit in it.
    fn motions_before_step(&self) -> HashMap<RigidBodyHandle, Motion> {
        if self.breakables.is_empty() {
            return HashMap::new();
        }
        self.rapier
            .bodies
            .iter()
            .filter(|(_, body)| body.is_dynamic())
            .map(|(handle, body)| {
                let motion = Motion {
                    velocity: body.linvel(),
                    spin: body.angvel(),
                    center: body.center_of_mass(),
                };
                (handle, motion)
            })
            .collect()
    }

    /// After a step: shatters each breakable part that something hit
    /// faster than its Fracture's `Threshold`, where it was hit hardest,
    /// and lets what hit it carry on as it was going. The speed is how
    /// fast the two points in contact were closing before the step.
    fn shatter_hit_parts(&mut self, world: &mut World, before: &HashMap<RigidBodyHandle, Motion>) {
        if self.breakables.is_empty() {
            return;
        }
        let mut impacts: BTreeMap<InstanceId, Impact> = BTreeMap::new();
        for pair in self.rapier.contact_pairs() {
            if !pair.has_any_active_contact() {
                continue;
            }
            let (Some(&one), Some(&two)) = (
                self.owners.get(&pair.collider1),
                self.owners.get(&pair.collider2),
            ) else {
                continue;
            };
            if !self.breakables.contains_key(&one) && !self.breakables.contains_key(&two) {
                continue;
            }
            let (Some((manifold, contact)), Some(collider)) = (
                pair.find_deepest_contact(),
                self.rapier.colliders.get(pair.collider1),
            ) else {
                continue;
            };
            let point = *collider.position() * contact.local_p1;
            let motion = |part: InstanceId| {
                let body = self.parts.get(&part)?.body;
                let motion = before.get(&body)?;
                let mass = self.rapier.bodies.get(body)?.mass();
                Some((body, *motion, mass))
            };
            let (a, b) = (motion(one), motion(two));
            let speed_of = |m: Option<(RigidBodyHandle, Motion, f32)>| {
                m.map_or(Vec3::ZERO, |(_, motion, _)| motion.at(point))
            };
            let speed = (speed_of(a) - speed_of(b)).dot(manifold.data.normal).abs();
            // Momentum passed between them: the reduced mass, or the moving
            // one's own mass against something anchored.
            let reduced = match (a.map(|m| m.2), b.map(|m| m.2)) {
                (Some(m1), Some(m2)) if m1 + m2 > 0.0 => m1 * m2 / (m1 + m2),
                (Some(m), None) | (None, Some(m)) => m,
                _ => 0.0,
            };
            for (part, own, other) in [(one, a, b), (two, b, a)] {
                let Some(breakable) = self.breakables.get(&part) else {
                    continue;
                };
                let faster = impacts.get(&part).is_none_or(|hit| speed > hit.speed);
                if speed <= breakable.threshold || !faster {
                    continue;
                }
                let impact = Impact {
                    point,
                    speed,
                    force: speed * reduced,
                    hitter: other.map(|(body, motion, _)| (body, motion.velocity)),
                    velocity: own.map_or(Vec3::ZERO, |(_, motion, _)| motion.velocity),
                };
                impacts.insert(part, impact);
            }
        }
        if impacts.is_empty() {
            return;
        }
        // Shattering reads where the part is and how fast it moves, which
        // the world only has after a write-back.
        self.write_to_world(world);
        for (part, impact) in impacts {
            let fracture = self.breakables[&part].fracture;
            let Some(debris) = fracture::shatter(
                world,
                &self.assets,
                fracture,
                to_vector3(impact.point),
                impact.force,
                to_vector3(impact.velocity),
            ) else {
                continue;
            };
            self.shattered.push(fracture);
            self.debris.push(debris);
            // It goes through, as if the part had never been there.
            if let Some((body, velocity)) = impact.hitter
                && let Some(body) = self.rapier.bodies.get_mut(body)
            {
                body.set_linvel(velocity, true);
            }
        }
        // The part goes and its pieces come before the next step.
        self.sync_from_world(world);
    }

    /// How many parts are being simulated (anchored ones included).
    pub fn body_count(&self) -> usize {
        self.parts.len()
    }

    /// What the last [`step`](Self::step) read from the world to catch up.
    pub fn sync_stats(&self) -> SyncStats {
        self.stats
    }

    /// Sets the wind's push on every unanchored part for the next step. A
    /// body's force stays until it's reset, so the last step's pushes are
    /// reset first, in case its WindAffected was just turned off, the wind
    /// stopped or a mover was disabled.
    /// Each welded part pushes its assembly by its own faces.
    fn blow(&mut self, wind: Vec3) {
        // Only the bodies pushed in the last step can have a force, so
        // rather than looking at every body, those are reset. Torques too,
        // or a Torque mover's, added every step, would pile up.
        for handle in self.pushed.drain(..) {
            if let Some(body) = self.rapier.bodies.get_mut(handle) {
                body.reset_forces(false);
                body.reset_torques(false);
            }
        }
        if wind == Vec3::ZERO {
            return;
        }
        for part in self.parts.values() {
            if !part.seen.wind_affected {
                continue;
            }
            let Some(body) = self.rapier.bodies.get_mut(part.body) else {
                continue;
            };
            if !body.is_dynamic() {
                continue;
            }
            let size = part.seen.size;
            let area = (size.x * size.y + size.y * size.z + size.z * size.x) / 3.0;
            // Waking the part means a resting part feels the wind too.
            body.add_force((wind - body.linvel()) * (WIND_DRAG * area), true);
            self.pushed.push(part.body);
        }
    }

    /// Compares the parts touching after this step with those touching
    /// before, and records the pairs that are new and those that ended.
    fn find_touches(&mut self) {
        // Only contacts the solver acts on count, not pairs whose bounding
        // boxes merely overlap. Sorted, so the order is the same every run.
        // Parts with CanCollide off are sensors: their overlaps count too.
        // Parts with CanTouch off touch nothing.
        let pair_of = |one: &ColliderHandle, two: &ColliderHandle| {
            let (a, a_touches) = self.owners.touchable(one)?;
            let (b, b_touches) = self.owners.touchable(two)?;
            if !a_touches || !b_touches {
                return None;
            }
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
        let mut now: Vec<(InstanceId, InstanceId)> = contacts.chain(overlaps).collect();
        now.sort_unstable();
        now.dedup();

        // Both lists are sorted by pair: walk them together. A pair in
        // `now` touches (and is new if it wasn't in `touching`); a pair only
        // in `touching` is apart this step, and stops counting once it's
        // been apart for long enough. A pair that can't touch any more
        // (CanTouch off, or set not to collide) stops at once.
        let parts = &self.parts;
        let can_touch = |(a, b): (InstanceId, InstanceId)| match (parts.get(&a), parts.get(&b)) {
            (Some(a), Some(b)) => {
                a.seen.can_touch && b.seen.can_touch && a.seen.groups.collide(b.seen.groups)
            }
            _ => false,
        };
        let old = std::mem::take(&mut self.touching);
        let mut touching = Vec::with_capacity(old.len().max(now.len()));
        let (mut i, mut j) = (0, 0);
        while i < old.len() || j < now.len() {
            match (old.get(i), now.get(j)) {
                (Some(&(pair, _)), Some(&touch)) if pair == touch => {
                    touching.push((pair, 0));
                    i += 1;
                    j += 1;
                }
                (before, Some(&touch)) if before.is_none_or(|&(pair, _)| touch < pair) => {
                    if self.started {
                        self.touches.push(touch);
                    }
                    touching.push((touch, 0));
                    j += 1;
                }
                (Some(&(pair, steps_apart)), _) => {
                    let steps_apart = if can_touch(pair) {
                        steps_apart + 1
                    } else {
                        SEPARATE_STEPS
                    };
                    if steps_apart >= SEPARATE_STEPS {
                        self.touch_ends.push(pair);
                    } else {
                        touching.push((pair, steps_apart));
                    }
                    i += 1;
                }
                (None, _) => unreachable!("the loop runs while either list has more"),
            }
        }
        self.touching = touching;
        self.started = true;
    }

    /// Copies where each moving part is now, and how fast it moves, into
    /// its `Position`, `Orientation` and velocities, and remembers the
    /// values written. Welded parts share their assembly's velocities.
    ///
    /// Only the parts of awake bodies, and of bodies in `unwritten`, can
    /// have anything new to write: the rest of the world (anchored parts,
    /// bodies asleep since their last write) is skipped without a look.
    /// Parts are written in ID order, as a walk of every part would.
    fn write_to_world(&mut self, world: &mut World) {
        profiling::scope!("Physics::write_back");
        let awake: Vec<RigidBodyHandle> = self.rapier.islands.active_bodies().collect();
        let mut ids = Vec::with_capacity(awake.len() + self.unwritten.len());
        for handle in awake.iter().chain(self.unwritten.iter()) {
            let Some(body) = self.rapier.bodies.get(*handle) else {
                continue;
            };
            ids.extend(
                body.colliders()
                    .iter()
                    .filter_map(|collider| self.owners.get(collider).copied()),
            );
        }
        ids.sort_unstable();
        ids.dedup();
        self.stats.parts_written += ids.len();
        for id in ids {
            let Some(part) = self.parts.get_mut(&id) else {
                continue;
            };
            let Some(body) = self.rapier.bodies.get(part.body) else {
                continue;
            };
            let mut revision = part.revision;
            own_write(world, id, &mut revision, |world| {
                write_part(world, id, part, body);
            });
            part.revision = revision;
        }
        self.unwritten.clear();
        self.unwritten.extend(awake);
    }
}

/// Runs `write`, which sets properties of the part `id`, so that the next
/// sync doesn't take physics' own writes for someone else's: `revision`
/// (the part's) follows them, unless something else changed the part first
/// and the sync still has to look at that.
pub(crate) fn own_write(
    world: &mut World,
    id: InstanceId,
    revision: &mut u64,
    write: impl FnOnce(&mut World),
) {
    let in_step = world.get(id).map(Instance::revision) == Some(*revision);
    write(world);
    if in_step {
        *revision = world.get(id).map_or(0, Instance::revision);
    }
}

/// Copies one part's place and velocities from `body` into `world`.
fn write_part(world: &mut World, id: InstanceId, part: &mut Body, body: &RigidBody) {
    // An anchored part doesn't move, whatever was set; a part that
    // falls asleep stops. Either way its velocities become zero.
    let (velocity, spin) = if body.is_dynamic() {
        (to_vector3(body.linvel()), to_vector3(body.angvel()))
    } else {
        (Vector3::default(), Vector3::default())
    };
    if velocity != part.seen.velocity {
        part.seen.velocity = velocity;
        let _ = world.set_known_property(id, "AssemblyLinearVelocity", Value::Vector3(velocity));
    }
    if spin != part.seen.spin {
        part.seen.spin = spin;
        let _ = world.set_known_property(id, "AssemblyAngularVelocity", Value::Vector3(spin));
    }
    if !body.is_dynamic() {
        return;
    }
    if body.is_sleeping() {
        if part.settled {
            return;
        }
        part.settled = true;
    } else {
        part.settled = false;
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
        let _ = world.set_known_property(id, "Position", Value::Vector3(position));
    }
    if orientation != part.seen.orientation {
        part.seen.orientation = orientation;
        let _ = world.set_known_property(id, "Orientation", Value::Vector3(orientation));
    }
}

/// The collider for a part, `offset` from its assembly's body. A
/// `weightless` one (Massless, and not the root) adds no mass.
fn part_collider(state: &PartState, offset: Pose, weightless: bool) -> Collider {
    let mut collider = shapes::collider(&state.shape, to_vec3(state.size));
    // A turned shape (a cylinder) keeps its turn within the part.
    collider.set_position(offset * *collider.position());
    collider.set_sensor(!state.can_collide);
    collider.set_collision_groups(state.groups.interaction());
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

pub(crate) fn quat_to_orientation(rotation: Quat) -> Vector3 {
    let (y, x, z) = rotation.to_euler(EulerRot::YXZ);
    Vector3::new(x.to_degrees(), y.to_degrees(), z.to_degrees())
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod sync_tests;

#[cfg(test)]
mod assembly_tests;

#[cfg(test)]
mod joint_tests;

#[cfg(test)]
mod mover_tests;

#[cfg(test)]
mod query_tests;

#[cfg(test)]
mod vehicle_tests;

#[cfg(test)]
mod break_tests;

#[cfg(test)]
mod fracture_tests;

#[cfg(test)]
mod blend_tests;
