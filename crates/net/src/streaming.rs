//! World streaming for joined players (Phase 80): each player's device
//! only gets what's near them.
//!
//! With `Workspace.StreamingEnabled` on, what's in a
//! [`Unit`](axcenity_core::streaming::Unit) reaches a device only while
//! the unit is in range: within `StreamingMinRadius` it's always sent,
//! within `StreamingTargetRadius` it's sent nearest first as the budget
//! allows, and past that (with a margin) the device is told to drop it.
//! Everything else replicates as it always has. "In range" means near the
//! player's character, or near a region `RequestStreamAroundAsync` asked
//! for. A device gets a unit's Model and hears what's streamed out, so
//! scripts can ask `StreamingState`.
//!
//! [`Streams`] sits between [`Replicator`](crate::replicate::Replicator)
//! and the session: the replicator still finds what changed once for
//! everyone, and each player's share of it goes through their
//! [`PeerStream`].

use std::collections::{BTreeMap, BTreeSet};

use rustc_hash::FxHashSet;

use axcenity_core::streaming::{Partition, Settings};
use axcenity_core::{InstanceId, Vector3, World};

use crate::replicate::{Op, Update, create};

/// Seconds between going over what each player should have.
pub const TICK_SECONDS: f64 = 0.2;
/// Instances a second a player may be sent past `StreamingMinRadius`.
pub const INSTANCES_PER_SECOND: f64 = 4000.0;
/// How long a region `RequestStreamAroundAsync` asked for stays loaded.
pub const REGION_SECONDS: f64 = 10.0;
/// Regions a player can have loaded at once, the newest ones.
const MAX_REGIONS: usize = 4;
/// One update carries at most this many Instances, so a big region goes
/// as several reliable messages.
const OPS_PER_MESSAGE: usize = 256;

/// A place `RequestStreamAroundAsync` asked to have loaded.
#[derive(Clone, Copy, Debug)]
struct Region {
    center: Vector3,
    until: f64,
}

/// What one player has of the world.
#[derive(Debug, Default)]
pub struct PeerStream {
    /// The roots of the units they have.
    in_units: BTreeSet<InstanceId>,
    /// The units' members they have, and new Instances passed on.
    sent: FxHashSet<InstanceId>,
    regions: Vec<Region>,
    /// `RequestStreamAroundAsync` calls waiting for their region.
    waiting: Vec<(u64, Vector3)>,
    /// The BaseParts they have, which move by motion.
    parts: Vec<InstanceId>,
    /// Instances they may still be sent past the minimum radius.
    budget: f64,
}

impl PeerStream {
    /// Whether the player has `id`: it's always there, or they were sent it.
    fn has(&self, partition: &Partition, id: InstanceId) -> bool {
        self.sent.contains(&id) || !partition.is_member(id)
    }

    /// The BaseParts this player has, for motion.
    pub fn parts(&self) -> &[InstanceId] {
        &self.parts
    }

    fn focuses(&self, character: Option<Vector3>) -> Vec<Vector3> {
        let mut focuses: Vec<Vector3> = self.regions.iter().map(|region| region.center).collect();
        focuses.extend(character);
        if focuses.is_empty() {
            // No character yet: what's around the origin.
            focuses.push(Vector3::default());
        }
        focuses
    }

    fn update_regions(&mut self, clock: f64) {
        self.regions.retain(|region| region.until > clock);
    }
}

/// What a tick decided for one player.
#[derive(Debug, Default)]
pub struct Tick {
    /// Updates to send, in order.
    pub updates: Vec<Update>,
    /// `RequestStreamAroundAsync` calls whose region is on its way, each
    /// with the request's number.
    pub answered: Vec<u64>,
}

/// Every player's stream, while streaming is on.
#[derive(Default)]
pub struct Streams {
    partition: Partition,
    peers: BTreeMap<u32, PeerStream>,
    /// Seconds until the next tick.
    next_tick: f64,
    /// Whether a request is waiting, so the next tick is now.
    urgent: bool,
}

impl Streams {
    /// Whether any player has a stream (streaming is on, or was just
    /// turned off and they still need what they were missing).
    pub fn is_active(&self) -> bool {
        !self.peers.is_empty()
    }

    pub fn partition(&self) -> &Partition {
        &self.partition
    }

    /// Forgets `peer`.
    pub fn remove(&mut self, peer: u32) {
        self.peers.remove(&peer);
    }

    /// Forgets everyone.
    pub fn clear(&mut self) {
        *self = Streams::default();
    }

    /// The player's stream, if they have one.
    pub fn of(&self, peer: u32) -> Option<&PeerStream> {
        self.peers.get(&peer)
    }

    /// Catches up with `world`, once a frame, and says whether it's time
    /// to go over what each player should have.
    pub fn frame(&mut self, world: &World, seconds: f64) -> bool {
        self.partition.refresh(world);
        self.next_tick -= seconds;
        if self.next_tick > 0.0 && !self.urgent {
            return false;
        }
        self.next_tick = TICK_SECONDS;
        self.urgent = false;
        true
    }

    /// Asks that what's around `position` reach `peer`, answered with
    /// `request` in a later [`Tick`]; the region stays loaded for
    /// [`REGION_SECONDS`].
    pub fn request_region(&mut self, peer: u32, position: Vector3, request: u64, clock: f64) {
        let stream = self.peers.entry(peer).or_default();
        if stream.regions.len() == MAX_REGIONS {
            stream.regions.remove(0);
        }
        stream.regions.push(Region {
            center: position,
            until: clock + REGION_SECONDS,
        });
        stream.waiting.push((request, position));
        self.urgent = true;
    }

    /// Starts a new player's stream and decides which units they start
    /// with, for their snapshot (see [`snapshot_has`](Self::snapshot_has)).
    pub fn start_peer(
        &mut self,
        world: &World,
        settings: &Settings,
        peer: u32,
        character: Option<Vector3>,
        owned: &BTreeSet<InstanceId>,
    ) {
        self.partition.refresh(world);
        // Keeps a region asked for before the snapshot went out.
        let mut stream = self.peers.remove(&peer).unwrap_or_default();
        stream.budget = INSTANCES_PER_SECOND * TICK_SECONDS;
        let focuses = stream.focuses(character);
        let wanted = wanted_units(&self.partition, settings, &focuses, owned, &stream.in_units);
        let units = self.partition.units();
        // Only what's inside the minimum radius: the rest follows within
        // the budget, so the first message stays small.
        for &index in &wanted.must {
            let unit = &units[index];
            stream.in_units.insert(unit.root);
            stream.sent.extend(unit.members.iter().copied());
        }
        rebuild_parts(world, &self.partition, &mut stream);
        self.peers.insert(peer, stream);
    }

    /// Whether `peer`'s first snapshot includes the Instance `id`.
    pub fn snapshot_has(&self, peer: u32, id: InstanceId) -> bool {
        self.peers
            .get(&peer)
            .is_none_or(|stream| stream.has(&self.partition, id))
    }

    /// The unit roots `peer` doesn't have, as their snapshot says.
    pub fn out_of(&self, peer: u32) -> Vec<u64> {
        let Some(stream) = self.peers.get(&peer) else {
            return Vec::new();
        };
        self.partition
            .units()
            .iter()
            .filter(|unit| !stream.in_units.contains(&unit.root))
            .map(|unit| unit.root.to_bits())
            .collect()
    }

    /// Gives `peer` their share of this frame's `update` and, on a tick,
    /// what came into range and left it. `settings` is `None` when
    /// streaming was just turned off: they get what they're missing and
    /// their stream ends.
    #[allow(clippy::too_many_arguments)]
    pub fn for_peer(
        &mut self,
        world: &World,
        settings: Option<&Settings>,
        peer: u32,
        character: Option<Vector3>,
        owned: &BTreeSet<InstanceId>,
        update: &Update,
        tick: bool,
        clock: f64,
    ) -> Tick {
        let partition = &self.partition;
        let stream = self.peers.entry(peer).or_default();
        if tick {
            // A tick's worth, saved up for at most a second.
            stream.budget =
                (stream.budget + INSTANCES_PER_SECOND * TICK_SECONDS).min(INSTANCES_PER_SECOND);
        }
        stream.update_regions(clock);
        let mut result = Tick::default();
        let forwarded = forward(world, partition, settings, stream, update, character);
        if !forwarded.is_empty() {
            result.updates.push(forwarded);
        }
        if tick || settings.is_none() {
            let (changes, answered) =
                reconcile(world, partition, settings, stream, character, owned);
            result.updates.extend(changes);
            result.answered = answered;
            rebuild_parts(world, partition, stream);
            if settings.is_none() {
                self.peers.remove(&peer);
            }
        }
        result
    }
}

/// The units a player should have, by index in the partition.
struct Wanted {
    /// Within the minimum radius (or a part they move): sent at once.
    must: Vec<usize>,
    /// Between the minimum and the target radius, nearest first, sent as
    /// the budget allows, and those already in the unload radius.
    may: Vec<usize>,
}

fn wanted_units(
    partition: &Partition,
    settings: &Settings,
    focuses: &[Vector3],
    owned: &BTreeSet<InstanceId>,
    in_units: &BTreeSet<InstanceId>,
) -> Wanted {
    let owned_units: BTreeSet<usize> = owned
        .iter()
        .filter_map(|&id| partition.unit_of(id))
        .collect();
    let mut must = Vec::new();
    let mut may: Vec<(f32, usize)> = Vec::new();
    let mut kept = Vec::new();
    for (index, unit) in partition.units().iter().enumerate() {
        let distance = focuses
            .iter()
            .map(|&focus| unit.distance(focus))
            .fold(f32::MAX, f32::min);
        if distance <= settings.min_radius || owned_units.contains(&index) {
            must.push(index);
        } else if distance <= settings.target_radius {
            may.push((distance, index));
        } else if in_units.contains(&unit.root) && distance <= settings.unload_radius() {
            kept.push(index);
        }
    }
    may.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut near: Vec<usize> = may.into_iter().map(|(_, index)| index).collect();
    near.extend(kept);
    Wanted { must, may: near }
}

/// Passes on the part of this frame's `update` the player can use.
fn forward(
    world: &World,
    partition: &Partition,
    settings: Option<&Settings>,
    stream: &mut PeerStream,
    update: &Update,
    character: Option<Vector3>,
) -> Update {
    let mut ops = Vec::new();
    for op in &update.ops {
        match op {
            Op::Create(wire) => {
                let id = InstanceId::from_bits(wire.id);
                let parent = InstanceId::from_bits(wire.parent);
                if wants_new(world, partition, settings, stream, id, parent, character) {
                    stream.sent.insert(id);
                    if world.get(id).is_some_and(|i| i.class().is_base_part()) {
                        stream.parts.push(id);
                    }
                    ops.push(op.clone());
                }
            }
            Op::Change(change) => {
                if stream.has(partition, InstanceId::from_bits(change.id)) {
                    ops.push(op.clone());
                }
            }
            Op::Destroy { id } => {
                let id = InstanceId::from_bits(*id);
                if stream.has(partition, id) {
                    stream.sent.remove(&id);
                    ops.push(op.clone());
                }
            }
        }
    }
    Update {
        ops,
        time: update.time,
        levels: update.levels.clone(),
        animators: update
            .animators
            .iter()
            .filter(|wire| stream.has(partition, InstanceId::from_bits(wire.id)))
            .cloned()
            .collect(),
        faces: update
            .faces
            .iter()
            .filter(|wire| stream.has(partition, InstanceId::from_bits(wire.id)))
            .cloned()
            .collect(),
        ..Update::default()
    }
}

/// Whether a player should get an Instance made since the last split.
fn wants_new(
    world: &World,
    partition: &Partition,
    settings: Option<&Settings>,
    stream: &PeerStream,
    id: InstanceId,
    parent: InstanceId,
    character: Option<Vector3>,
) -> bool {
    let Some(settings) = settings else {
        return true;
    };
    let units = partition.units();
    if let Some(unit) = partition.unit_of(id).or_else(|| partition.unit_of(parent)) {
        return stream.in_units.contains(&units[unit].root);
    }
    // Inside a Model that loads and unloads: only while it's loaded.
    if let Some(unit) = partition.unit_rooted_at(parent) {
        return stream.in_units.contains(&units[unit].root);
    }
    // A new loose part goes if it's near, as the next tick would decide.
    let Some(instance) = world.get(id) else {
        return false;
    };
    if instance.class().is_base_part() {
        let position = instance
            .property("Position")
            .and_then(|value| value.as_vector3())
            .unwrap_or_default();
        let near = stream
            .focuses(character)
            .iter()
            .any(|&focus| distance_between(focus, position) <= settings.target_radius);
        return near;
    }
    true
}

fn distance_between(a: Vector3, b: Vector3) -> f32 {
    let (x, y, z) = (a.x - b.x, a.y - b.y, a.z - b.z);
    (x * x + y * y + z * z).sqrt()
}

/// Rebuilds the list of parts the player has, for motion.
fn rebuild_parts(world: &World, partition: &Partition, stream: &mut PeerStream) {
    stream.parts.clear();
    let members = stream
        .sent
        .iter()
        .copied()
        .filter(|&id| world.get(id).is_some_and(|i| i.class().is_base_part()));
    stream.parts.extend(members);
    // And what's always there.
    stream.parts.extend(
        partition
            .always_there()
            .iter()
            .copied()
            .filter(|id| !stream.sent.contains(id)),
    );
    stream.parts.sort_unstable();
}

/// Goes over what the player has against what they should have, and
/// answers the region requests that came through.
fn reconcile(
    world: &World,
    partition: &Partition,
    settings: Option<&Settings>,
    stream: &mut PeerStream,
    character: Option<Vector3>,
    owned: &BTreeSet<InstanceId>,
) -> (Vec<Update>, Vec<u64>) {
    let everything = settings.is_none();
    let settings = settings.copied().unwrap_or(Settings {
        min_radius: f32::INFINITY,
        target_radius: f32::INFINITY,
    });
    let focuses = stream.focuses(character);
    let wanted = wanted_units(partition, &settings, &focuses, owned, &stream.in_units);
    let units = partition.units();

    let mut budget = stream.budget;
    let mut admitted = BTreeSet::new();
    for &index in &wanted.must {
        admitted.insert(index);
    }
    for &index in &wanted.may {
        let unit = &units[index];
        if stream.in_units.contains(&unit.root) {
            admitted.insert(index);
            continue;
        }
        if budget >= unit.members.len() as f64 || everything {
            budget -= unit.members.len() as f64;
            admitted.insert(index);
        }
    }
    stream.budget = budget.max(0.0);

    // What leaves: roots no longer admitted (the device removes what's in
    // them), and members stranded in units they don't have.
    let admitted_roots: BTreeSet<InstanceId> =
        admitted.iter().map(|&index| units[index].root).collect();
    let mut streamed_out = Vec::new();
    let left: Vec<InstanceId> = stream
        .in_units
        .iter()
        .copied()
        .filter(|root| !admitted_roots.contains(root))
        .collect();
    let mut leaving: FxHashSet<InstanceId> = FxHashSet::default();
    for root in left {
        stream.in_units.remove(&root);
        streamed_out.push(root.to_bits());
        if let Some(unit) = partition.unit_rooted_at(root) {
            leaving.extend(units[unit].members.iter().copied());
        }
    }
    let mut destroys = Vec::new();
    stream.sent.retain(|&id| {
        let Some(unit) = partition.unit_of(id) else {
            return true;
        };
        if stream.in_units.contains(&units[unit].root) || leaving.contains(&id) {
            return !leaving.contains(&id);
        }
        destroys.push(Op::Destroy { id: id.to_bits() });
        false
    });

    // What arrives, parents first, nearest first.
    let mut creates = Vec::new();
    let mut streamed_in = Vec::new();
    for &index in wanted.must.iter().chain(&wanted.may) {
        if !admitted.contains(&index) {
            continue;
        }
        let unit = &units[index];
        for &member in &unit.members {
            if stream.sent.insert(member)
                && let Some(op) = create(world, member)
            {
                creates.push(op);
            }
        }
        if stream.in_units.insert(unit.root) {
            streamed_in.push(unit.root.to_bits());
        }
    }

    // Everything waiting is answered now: `must` units are sent at once.
    let answered: Vec<u64> = stream
        .waiting
        .drain(..)
        .map(|(request, _)| request)
        .collect();

    let time = 0.0;
    let mut updates = Vec::new();
    if !destroys.is_empty() || !streamed_out.is_empty() {
        updates.push(Update {
            ops: destroys,
            time,
            streamed_out,
            ..Update::default()
        });
    }
    // Destroys and arrivals never share a message: a lost-looking Update
    // is impossible on the reliable channel, and this keeps creates in
    // parents-first order however they're split.
    let mut chunks = creates.chunks(OPS_PER_MESSAGE).peekable();
    if chunks.peek().is_none() && !streamed_in.is_empty() {
        updates.push(Update {
            time,
            streamed_in: streamed_in.clone(),
            ..Update::default()
        });
    }
    while let Some(chunk) = chunks.next() {
        updates.push(Update {
            ops: chunk.to_vec(),
            time,
            streamed_in: if chunks.peek().is_none() {
                std::mem::take(&mut streamed_in)
            } else {
                Vec::new()
            },
            ..Update::default()
        });
    }
    (updates, answered)
}
