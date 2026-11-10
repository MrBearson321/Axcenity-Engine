//! Catching up with the [`World`] before each step: bodies, colliders and
//! joints for the parts that came, went or changed since the last one.
//!
//! Only what changed is read. A part's revision says whether anything but
//! physics touched it, and the Workspace's list of moves says what came in
//! or went out, so a reparent costs what it moved, not the whole world. A
//! change to one part of an assembly takes the whole assembly along: its
//! parts share a body. Everything is read again only on the first step,
//! when the list of moves has run out, and when the collision groups change.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use axcenity_core::{ClassName, Instance, InstanceId, Value, Vector3, World};
use rapier3d::glamx::Vec3;
use rapier3d::math::Pose;
use rapier3d::prelude::{Collider, QueryFilter, RigidBodyBuilder, RigidBodyHandle, RigidBodyType};

use crate::shapes::{self, Form};
use crate::{
    ANGULAR_DAMPING, Body, PartState, Physics, assembly, fracture, joints, material, movers,
    part_collider, to_vec3, vehicles,
};

/// Extra solver iterations for a ragdoll's bodies, so a long chain of
/// light limbs holds its joints when it lands. Many more (32) made a
/// knocked ragdoll leap off the ground.
const RAGDOLL_ITERATIONS: usize = 16;

/// What the last sync was made from.
pub(crate) struct Synced {
    /// The Workspace's structure version.
    structure: u64,
    /// The held parts.
    held: BTreeSet<InstanceId>,
    /// The Instances that decide how parts are put together (welds,
    /// ragdolls, inventories), wherever they are, with their revisions.
    watched: Vec<(InstanceId, u64)>,
    /// The parts each weld holds together.
    welds: Vec<(InstanceId, InstanceId)>,
    /// The parts left to Ragdolls (see [`ragdoll_parts`]).
    ragdolled: HashSet<InstanceId>,
}

/// What the tree's changes since the last sync touched.
#[derive(Default)]
struct Touched {
    /// Parts whose assemblies have to be looked at again.
    parts: BTreeSet<InstanceId>,
    /// The welds before these changes, when they were different.
    old_welds: Vec<(InstanceId, InstanceId)>,
}

/// Bodies a script moved, threw or otherwise changed.
type Moved = HashSet<RigidBodyHandle>;

impl Physics {
    /// Adds new Parts, removes ones that left the Workspace or were
    /// destroyed, applies anything a script changed since the last step,
    /// and welds or unwelds assemblies.
    pub(crate) fn sync_from_world(&mut self, world: &World) {
        profiling::scope!("Physics::sync");
        // Before the reads: a model that arrives during them is read again.
        let generation = self.assets.generation();
        let moved = match self.touched_by_tree_changes(world) {
            Some(touched) => self.sync_changes(world, touched, generation),
            None => self.sync_everything(world),
        };
        self.assets_generation = generation;
        self.finish_sync(world, moved);
        #[cfg(any(test, debug_assertions))]
        self.verify_sync(world);
    }

    /// Reads all of the Workspace again, when the moves since the last sync
    /// aren't known.
    fn sync_everything(&mut self, world: &World) -> Moved {
        let (ragdolled, ragdoll_bodies) = ragdoll_parts(world);
        self.ragdoll_bodies = ragdoll_bodies;
        let synced = Synced {
            // Only the Workspace: moving things around in ReplicatedStorage
            // or putting a Model together with no parent yet doesn't touch a
            // part. `watched` catches what puts parts together wherever it is.
            structure: world.structure_version_of(world.workspace()),
            held: self.held.clone(),
            watched: watched(world).collect(),
            welds: assembly::welds(world),
            ragdolled,
        };
        let scope: BTreeSet<InstanceId> = world
            .descendants_iter(world.workspace())
            .chain(self.parts.keys().copied())
            .collect();
        self.stats.full_sync = true;
        let mut moved = Moved::new();
        self.sync_assemblies(world, &scope, &synced, &mut moved);
        self.synced = Some(synced);
        moved
    }

    /// What changed in the tree since the last sync, and notes it as seen.
    /// `None` if that can't be told and everything has to be read again.
    fn touched_by_tree_changes(&mut self, world: &World) -> Option<Touched> {
        let mut synced = self.synced.take()?;
        let moves = world.workspace_moves_since(synced.structure)?;
        let same_watched = synced.watched.iter().copied().eq(watched(world));
        if moves.is_empty() && same_watched && synced.held == self.held {
            self.synced = Some(synced);
            return Some(Touched::default());
        }

        let mut touched = Touched::default();
        for id in moves {
            touched.parts.insert(id);
            touched.parts.extend(world.descendants_iter(id));
        }
        touched
            .parts
            .extend(synced.held.symmetric_difference(&self.held).copied());
        if !same_watched {
            let before: HashMap<InstanceId, u64> = synced.watched.iter().copied().collect();
            for (id, revision) in watched(world) {
                // Whether its Tools are put away or in hand.
                let inventory = world.get(id).map(Instance::class) == Some(ClassName::Inventory);
                if inventory && before.get(&id) != Some(&revision) {
                    touched.parts.extend(world.descendants_iter(id));
                }
            }
        }

        let welds = assembly::welds(world);
        let (old, new): (HashSet<_>, HashSet<_>) = (
            synced.welds.iter().copied().collect(),
            welds.iter().copied().collect(),
        );
        for &(a, b) in old.symmetric_difference(&new) {
            touched.parts.extend([a, b]);
        }
        if old != new {
            touched.old_welds = std::mem::replace(&mut synced.welds, welds);
        }

        let (ragdolled, ragdoll_bodies) = ragdoll_parts(world);
        touched
            .parts
            .extend(ragdolled.symmetric_difference(&synced.ragdolled).copied());
        touched.parts.extend(
            ragdoll_bodies
                .symmetric_difference(&self.ragdoll_bodies)
                .copied(),
        );
        self.ragdoll_bodies = ragdoll_bodies;
        synced.ragdolled = ragdolled;

        synced.structure = world.structure_version_of(world.workspace());
        synced.held.clone_from(&self.held);
        if !same_watched {
            synced.watched = watched(world).collect();
        }
        self.synced = Some(synced);
        Some(touched)
    }

    /// Catches up with `touched` and with every part whose Instance changed
    /// since the last sync.
    fn sync_changes(&mut self, world: &World, mut touched: Touched, generation: u64) -> Moved {
        let assets_changed = generation != self.assets_generation;
        // Parts that keep their body. Welded ones are rebuilt with their
        // assemblies instead, so the welds hold them where they are now.
        let mut changed = Vec::new();
        for (&id, part) in &mut self.parts {
            let Some(instance) = world.get(id) else {
                touched.parts.insert(id);
                continue;
            };
            let revision = instance.revision();
            if part.revision == revision && !part.seen.shape.outdated(world, id, assets_changed) {
                continue;
            }
            // Read below, with the rest of what the tree's changes touched.
            if touched.parts.contains(&id) {
                continue;
            }
            let Some(mut state) = PartState::read(world, id, &self.assets) else {
                touched.parts.insert(id);
                continue;
            };
            state.anchored |= self.held.contains(&id);
            self.stats.parts_read += 1;
            if state == part.seen {
                part.revision = revision;
            } else if part.welded {
                touched.parts.insert(id);
            } else {
                changed.push((id, state, revision));
            }
        }

        let mut moved = Moved::new();
        if !touched.parts.is_empty() {
            let synced = self.synced.take().expect("a sync without one started over");
            let scope = whole_assemblies(touched.parts, &synced.welds, &touched.old_welds);
            changed.retain(|(id, ..)| !scope.contains(id));
            self.sync_assemblies(world, &scope, &synced, &mut moved);
            self.synced = Some(synced);
        }
        for (id, state, revision) in changed {
            self.apply_part_change(id, state, false, &mut moved);
            if let Some(part) = self.parts.get_mut(&id) {
                part.revision = revision;
            }
        }
        moved
    }

    /// Brings the assemblies of `scope` up to date, which has every part in
    /// each assembly it touches: bodies come, go or are built again,
    /// and parts that keep theirs get what changed.
    fn sync_assemblies(
        &mut self,
        world: &World,
        scope: &BTreeSet<InstanceId>,
        synced: &Synced,
        moved: &mut Moved,
    ) {
        let workspace = world.workspace();
        let current: BTreeMap<InstanceId, PartState> = scope
            .iter()
            .copied()
            .filter(|&id| world.is_descendant_of(id, workspace) && !synced.ragdolled.contains(&id))
            .filter_map(|id| {
                let mut state = PartState::read(world, id, &self.assets)?;
                state.anchored |= self.held.contains(&id);
                Some((id, state))
            })
            .collect();
        self.stats.parts_read += current.len();
        let ids: Vec<InstanceId> = current.keys().copied().collect();
        let groups: Vec<(Vec<InstanceId>, InstanceId)> = assembly::group(&ids, &synced.welds)
            .into_iter()
            .map(|members| {
                let root = assembly::root(world, &members);
                (members, root)
            })
            .collect();

        // An assembly is built again when a part joins or leaves it, its
        // root changes, or a script moves or (un)anchors a welded part, so
        // the welds hold the parts where they are now. A part alone keeps
        // its body (see `apply_change`).
        let mut sharing: HashMap<RigidBodyHandle, usize> = HashMap::new();
        for id in scope {
            if let Some(part) = self.parts.get(id) {
                *sharing.entry(part.body).or_default() += 1;
            }
        }
        let rebuild: Vec<bool> = groups
            .iter()
            .map(|(members, root)| !self.is_kept(members, *root, &current, &sharing))
            .collect();

        // Take out the parts that are gone or rebuilt, with their bodies.
        // Their last state tells the new body how fast to start.
        let leaving = scope
            .iter()
            .copied()
            .filter(|id| !current.contains_key(id) && self.parts.contains_key(id));
        let gone: HashSet<InstanceId> = leaving.clone().collect();
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
            // What it was holding up has to notice. Welded parts share a
            // body, so it may be gone already.
            self.wake_touching(part.body);
            // `true` also removes the body's colliders.
            self.rapier.remove_body_with_colliders(part.body, true);
            self.owners.remove(&part.collider);
            for handle in part.foliage {
                self.owners.remove(&handle);
            }
            old.insert(id, part.seen);
        }
        // A part that comes back (moved into the Workspace again) touches anew.
        if !gone.is_empty() {
            self.touching
                .retain(|(a, b), _| !gone.contains(a) && !gone.contains(b));
        }

        for ((members, root), rebuild) in groups.iter().zip(rebuild) {
            if rebuild {
                self.build_assembly(members, *root, &current, &old);
                self.stats.bodies_built += 1;
                continue;
            }
            let welded = members.len() > 1;
            for id in members {
                let state = &current[id];
                let part = &self.parts[id];
                if part.seen == *state {
                    continue;
                }
                let weightless = welded && !part.root && state.massless;
                self.apply_part_change(*id, state.clone(), weightless, moved);
            }
        }

        // What the next frame compares with.
        for id in scope {
            if let Some(part) = self.parts.get_mut(id) {
                part.revision = world.get(*id).map_or(0, Instance::revision);
            }
        }
    }

    /// Applies what changed on a part that keeps its body.
    fn apply_part_change(
        &mut self,
        id: InstanceId,
        state: PartState,
        weightless: bool,
        moved: &mut Moved,
    ) {
        let mut part = self.parts.remove(&id).expect("just found");
        self.owners.remove(&part.collider);
        for handle in &part.foliage {
            self.owners.remove(handle);
        }
        // What it was touching where it was, then what it touches now.
        self.wake_touching(part.body);
        self.apply_change(&mut part, state, weightless);
        self.wake_overlapping(part.body);
        moved.insert(part.body);
        self.owners.insert(part.collider, id);
        self.attach_foliage(id, &mut part);
        self.parts.insert(id, part);
    }

    /// Wakes `body` and everything in contact with or jointed to it, which
    /// may have lost what it was resting on. A sleeping part's contacts
    /// stay in place, so a sleeping stack is found this way too: waking one
    /// of its bodies wakes all of them.
    fn wake_touching(&mut self, handle: RigidBodyHandle) {
        let Some(body) = self.rapier.bodies.get(handle) else {
            return;
        };
        let mut wake = vec![handle];
        for &collider in body.colliders() {
            let contacts = self
                .rapier
                .contact_pairs_with(collider)
                .filter(|pair| pair.has_any_active_contact())
                .map(|pair| (pair.collider1, pair.collider2));
            let overlaps = self
                .rapier
                .intersection_pairs_with(collider)
                .filter(|&(_, _, _, _, intersecting)| intersecting)
                .map(|(one, _, two, _, _)| (one, two));
            for (one, two) in contacts.chain(overlaps) {
                let other = if one == collider { two } else { one };
                wake.extend(self.rapier.colliders.get(other).and_then(Collider::parent));
            }
        }
        for (one, two, ..) in self.rapier.impulse_joints.attached_joints(handle) {
            wake.push(if one == handle { two } else { one });
        }
        for body in wake {
            self.rapier.wake_up(body, true);
        }
    }

    /// Wakes whatever `body` overlaps where it is now. rapier does that
    /// for moving bodies, but not for an anchored one a script put there.
    fn wake_overlapping(&mut self, handle: RigidBodyHandle) {
        let Some(body) = self.rapier.bodies.get(handle) else {
            return;
        };
        let pose = *body.position();
        let mut wake = Vec::new();
        for &collider in body.colliders() {
            let Some(collider) = self.rapier.colliders.get(collider) else {
                continue;
            };
            // The body may have just been moved: where the collider will be
            // at the next step, which rapier hasn't worked out yet.
            let at = pose
                * collider
                    .position_wrt_parent()
                    .copied()
                    .unwrap_or(Pose::IDENTITY);
            let filter = QueryFilter::default().exclude_rigid_body(handle);
            wake.extend(
                self.rapier
                    .intersect_shape(at, collider.shape(), filter)
                    .filter_map(|(_, other)| other.parent()),
            );
        }
        for body in wake {
            self.rapier.wake_up(body, true);
        }
    }

    /// The end of every sync: joints, movers, vehicles and waking.
    fn finish_sync(&mut self, world: &World, moved: Moved) {
        // Joints go between the bodies just made, so they come after them.
        joints::sync(world, &mut self.rapier, &self.parts, &mut self.joints);
        // Movers act on those bodies too, read again every frame since
        // they keep no state.
        self.movers = movers::read(world, &self.parts);
        movers::forget(&mut self.learned, &moved);
        self.vehicles = vehicles::read(world, &self.parts, &self.rapier.bodies, &self.rolling);
        self.rolling.retain(|id, _| world.contains(*id));
        self.breakables = fracture::breakables(world, &self.parts);
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
        let ragdolled = members.iter().any(|id| self.ragdoll_bodies.contains(id));
        let extra_iterations = if ragdolled { RAGDOLL_ITERATIONS } else { 0 };
        let body = RigidBodyBuilder::new(body_type)
            .pose(root_pose)
            // Continuous collision detection: fast parts can't skip through
            // thin ones between two steps.
            .ccd_enabled(true)
            .angular_damping(ANGULAR_DAMPING)
            .additional_solver_iterations(extra_iterations)
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
                welded,
                revision: 0,
            };
            self.attach_foliage(id, &mut part);
            self.parts.insert(id, part);
        }
        self.wake_overlapping(body);
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
        for mut collider in colliders {
            collider.set_collision_groups(part.seen.groups.interaction());
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
            if old.groups != state.groups {
                collider.set_collision_groups(state.groups.interaction());
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

    /// Tests, and debug builds run with `AXCENITY_PHYSICS_VERIFY` set: the
    /// bodies the incremental sync kept are the ones reading the whole
    /// Workspace would have.
    #[cfg(any(test, debug_assertions))]
    fn verify_sync(&self, world: &World) {
        if !cfg!(test) && std::env::var_os("AXCENITY_PHYSICS_VERIFY").is_none() {
            return;
        }
        let (ragdolled, _) = ragdoll_parts(world);
        let expected: BTreeSet<InstanceId> = world
            .descendants_iter(world.workspace())
            .filter(|id| !ragdolled.contains(id))
            .filter(|&id| PartState::read(world, id, &self.assets).is_some())
            .collect();
        let have: BTreeSet<InstanceId> = self.parts.keys().copied().collect();
        assert_eq!(expected, have, "the parts with bodies");
        let ids: Vec<InstanceId> = expected.into_iter().collect();
        let mut bodies = HashSet::new();
        for members in assembly::group(&ids, &assembly::welds(world)) {
            let body = self.parts[&members[0]].body;
            assert!(
                members.iter().all(|id| self.parts[id].body == body),
                "an assembly shares a body"
            );
            assert!(bodies.insert(body), "a body is one assembly's");
        }
    }
}

/// `touched`, and every part welded to any of them directly or not, by the
/// welds now or before them.
fn whole_assemblies(
    touched: BTreeSet<InstanceId>,
    welds: &[(InstanceId, InstanceId)],
    old_welds: &[(InstanceId, InstanceId)],
) -> BTreeSet<InstanceId> {
    let mut linked: HashMap<InstanceId, Vec<InstanceId>> = HashMap::new();
    for &(a, b) in welds.iter().chain(old_welds) {
        linked.entry(a).or_default().push(b);
        linked.entry(b).or_default().push(a);
    }
    let mut stack: Vec<InstanceId> = touched.iter().copied().collect();
    let mut scope = touched;
    while let Some(id) = stack.pop() {
        for &other in linked.get(&id).into_iter().flatten() {
            if scope.insert(other) {
                stack.push(other);
            }
        }
    }
    scope
}

/// The Instances that decide how parts are put together, anywhere in the
/// game, with their revisions.
fn watched(world: &World) -> impl Iterator<Item = (InstanceId, u64)> + '_ {
    world
        .all_where(|class| {
            matches!(
                class,
                ClassName::WeldConstraint | ClassName::Ragdoll | ClassName::Inventory
            )
        })
        .filter_map(|id| Some((id, world.get(id)?.revision())))
}

/// The parts physics leaves to Ragdolls (Phase 41): the bodies of a
/// Ragdoll that's off follow the animation, and the MeshPart of one that's
/// on follows its bodies, so it doesn't push them apart. Then the bodies of
/// the Ragdolls that are on.
fn ragdoll_parts(world: &World) -> (HashSet<InstanceId>, HashSet<InstanceId>) {
    let (mut left, mut bodies) = (HashSet::new(), HashSet::new());
    for id in world.all_below(world.workspace(), |class| class == ClassName::Ragdoll) {
        let Some(ragdoll) = world.get(id) else {
            continue;
        };
        if ragdoll.property("Enabled").and_then(Value::as_bool) == Some(true) {
            left.extend(ragdoll.parent());
            bodies.extend(world.descendants_iter(id));
        } else {
            left.extend(world.descendants_iter(id));
        }
    }
    (left, bodies)
}
