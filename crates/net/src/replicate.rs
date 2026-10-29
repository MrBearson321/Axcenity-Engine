//! Replication, the plain version (Phase 64): players' devices follow the
//! server's world.
//!
//! A joining player gets a snapshot of everything outside the server-only
//! services (ServerScriptService and ServerStorage): each service's
//! properties, and every Instance inside, parents first. After that, each
//! frame the server sends what changed since the last frame: Instances
//! made, renamed, moved, destroyed (or taken out of the game, or into a
//! server-only service), and their changed properties, attributes and tags.
//!
//! [`Replicator`] finds those changes by comparing the world with what it
//! last sent, so nothing the server does is missed, however it changed
//! (a change record would be missed by a host that took them for its
//! scripts first). These go on the reliable channel. A part's movement
//! doesn't: once a part is on a device it moves by `motion`, quantized and
//! within each player's budget (Phase 65).
//!
//! A replica keeps the server's Instances at their own IDs (see
//! `World::empty_replica`), so an ID means the same Instance on every
//! computer. A player's own changes to their replica stay theirs: the
//! server only overwrites what it changes itself.

use std::collections::{BTreeMap, BTreeSet};

use axcenity_core::{ClassName, InstanceId, Value, World};
use serde::{Deserialize, Serialize};

use crate::motion::MOTION_PROPERTIES;
use crate::wire::WireValue;

/// An Instance as the server has it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WireInstance {
    pub id: u64,
    pub class: String,
    pub name: String,
    pub parent: u64,
    pub properties: Vec<(String, WireValue)>,
    pub attributes: Vec<(String, WireValue)>,
    pub tags: Vec<String>,
}

/// What changed about an Instance a player already has.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WireChange {
    pub id: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Its new parent, if it moved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub properties: Vec<(String, WireValue)>,
    /// Each changed attribute's new value, or none where it was removed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attributes: Vec<(String, Option<WireValue>)>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags_added: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags_removed: Vec<String>,
}

impl WireChange {
    fn is_empty(&self) -> bool {
        self.name.is_none()
            && self.parent.is_none()
            && self.properties.is_empty()
            && self.attributes.is_empty()
            && self.tags_added.is_empty()
            && self.tags_removed.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op")]
pub enum Op {
    Create(WireInstance),
    Change(WireChange),
    Destroy { id: u64 },
}

/// A snapshot, or one frame's changes, in the order to apply them:
/// parents before their children, and what went away last.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Update {
    pub ops: Vec<Op>,
    /// The server's clock when it was made, in seconds: a part's motion
    /// sent before it is stale.
    #[serde(default)]
    pub time: f64,
    /// `LevelService`'s levels, when they changed (and in a snapshot).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub levels: Option<LevelState>,
}

/// The level in the Workspace and those added beside it, each with its
/// Model's network ID.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LevelState {
    pub current: String,
    pub added: Vec<(String, u64)>,
}

impl Update {
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty() && self.levels.is_none()
    }
}

/// An Instance as it was last sent.
struct Sent {
    name: String,
    parent: Option<InstanceId>,
    properties: Vec<(&'static str, Value)>,
    attributes: BTreeMap<String, Value>,
    tags: Vec<String>,
}

impl Sent {
    /// What updates carry of `id`: a part's motion goes on its own.
    fn of(world: &World, id: InstanceId) -> Option<Sent> {
        let instance = world.get(id)?;
        let part = instance.class().is_base_part();
        Some(Sent {
            name: instance.name().to_owned(),
            parent: instance.parent(),
            properties: instance
                .properties()
                .filter(|(name, _)| !(part && MOTION_PROPERTIES.contains(name)))
                .map(|(name, value)| (name, value.clone()))
                .collect(),
            attributes: instance.attributes().clone(),
            tags: instance.tags().to_vec(),
        })
    }
}

/// The services players get, in the order they're sent: every service of
/// `game` but the server-only ones, each with what's inside (StarterPlayer
/// holds StarterPlayerScripts).
fn replicated_services(world: &World) -> Vec<InstanceId> {
    let server_only = world.server_only_services();
    let mut services = vec![world.root()];
    if let Some(game) = world.get(world.root()) {
        services.extend(
            game.children()
                .iter()
                .copied()
                .filter(|id| !server_only.contains(id)),
        );
    }
    services.push(world.starter_player_scripts());
    services
}

/// Every Instance players get, parents first, with the services.
fn replicated(world: &World) -> (Vec<InstanceId>, Vec<InstanceId>) {
    let services = replicated_services(world);
    let mut inside = Vec::new();
    for &service in &services {
        if service == world.root() {
            continue;
        }
        let children = world.get(service).map(|s| s.children().to_vec());
        for child in children.unwrap_or_default() {
            let is_service = world
                .get(child)
                .is_some_and(|instance| instance.class().is_service());
            if is_service {
                continue;
            }
            inside.push(child);
            inside.extend(world.descendants(child));
        }
    }
    (services, inside)
}

/// The BaseParts players get, which move by `motion`.
pub fn replicated_parts(world: &World) -> Vec<InstanceId> {
    let (_, inside) = replicated(world);
    inside
        .into_iter()
        .filter(|&id| world.get(id).is_some_and(|i| i.class().is_base_part()))
        .collect()
}

fn properties(instance: &axcenity_core::Instance) -> Vec<(String, WireValue)> {
    instance
        .properties()
        .map(|(name, value)| (name.to_owned(), WireValue::from_value(value)))
        .collect()
}

fn create(world: &World, id: InstanceId) -> Option<Op> {
    let instance = world.get(id)?;
    Some(Op::Create(WireInstance {
        id: id.to_bits(),
        class: instance.class().as_str().to_owned(),
        name: instance.name().to_owned(),
        parent: instance.parent()?.to_bits(),
        properties: properties(instance),
        attributes: instance
            .attributes()
            .iter()
            .map(|(name, value)| (name.clone(), WireValue::from_value(value)))
            .collect(),
        tags: instance.tags().to_vec(),
    }))
}

/// The server's side of replication: what each Instance was when it was
/// last sent.
#[derive(Default)]
pub struct Replicator {
    sent: BTreeMap<InstanceId, Sent>,
}

impl Replicator {
    /// Everything players get, for one who just joined. Call it after
    /// [`frame`](Self::frame), so it matches what the others have.
    pub fn snapshot(&self, world: &World) -> Update {
        let (services, inside) = replicated(world);
        let mut ops = Vec::new();
        for service in services {
            if let Some(instance) = world.get(service) {
                ops.push(Op::Change(WireChange {
                    id: service.to_bits(),
                    properties: properties(instance),
                    ..WireChange::default()
                }));
            }
        }
        ops.extend(inside.into_iter().filter_map(|id| create(world, id)));
        Update {
            ops,
            ..Update::default()
        }
    }

    /// What changed since the last call, for the players who have the
    /// world as it was then.
    pub fn frame(&mut self, world: &World) -> Update {
        let (services, inside) = replicated(world);
        let mut ops = Vec::new();
        let mut seen = BTreeSet::new();
        for id in services.into_iter().chain(inside) {
            seen.insert(id);
            let Some(now) = Sent::of(world, id) else {
                continue;
            };
            match self.sent.get(&id) {
                None => {
                    // A service is always there: only its properties.
                    let is_service = world.get(id).is_some_and(|i| i.class().is_service());
                    if !is_service && let Some(op) = create(world, id) {
                        ops.push(op);
                    }
                }
                Some(before) => {
                    let change = diff(id, before, &now);
                    if !change.is_empty() {
                        ops.push(Op::Change(change));
                    }
                }
            }
            self.sent.insert(id, now);
        }
        let gone: Vec<InstanceId> = self
            .sent
            .keys()
            .copied()
            .filter(|id| !seen.contains(id))
            .collect();
        for id in gone {
            self.sent.remove(&id);
            ops.push(Op::Destroy { id: id.to_bits() });
        }
        Update {
            ops,
            ..Update::default()
        }
    }
}

fn diff(id: InstanceId, before: &Sent, now: &Sent) -> WireChange {
    let mut change = WireChange {
        id: id.to_bits(),
        ..WireChange::default()
    };
    if before.name != now.name {
        change.name = Some(now.name.clone());
    }
    if before.parent != now.parent {
        change.parent = now.parent.map(InstanceId::to_bits);
    }
    for (name, value) in &now.properties {
        let was = before
            .properties
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v);
        if was != Some(value) {
            change
                .properties
                .push(((*name).to_owned(), WireValue::from_value(value)));
        }
    }
    for (name, value) in &now.attributes {
        if before.attributes.get(name) != Some(value) {
            change
                .attributes
                .push((name.clone(), Some(WireValue::from_value(value))));
        }
    }
    for name in before.attributes.keys() {
        if !now.attributes.contains_key(name) {
            change.attributes.push((name.clone(), None));
        }
    }
    change.tags_added = now
        .tags
        .iter()
        .filter(|tag| !before.tags.contains(tag))
        .cloned()
        .collect();
    change.tags_removed = before
        .tags
        .iter()
        .filter(|tag| !now.tags.contains(tag))
        .cloned()
        .collect();
    change
}

/// What applying an update did that scripts hear about.
#[derive(Debug, Default, PartialEq)]
pub struct Applied {
    /// Players that arrived, for `PlayerAdded`.
    pub players_added: Vec<InstanceId>,
    /// Players the server removed, still in the replica, so
    /// `PlayerRemoving` can fire before they go.
    pub players_leaving: Vec<InstanceId>,
    /// Parts given their exact place (made, or settled), so motion sent
    /// before it is stale.
    pub placed: Vec<InstanceId>,
}

/// Applies `update` to a replica. Instance properties are set last, so
/// one may name an Instance that came later in the update.
pub fn apply(world: &mut World, update: &Update) -> Applied {
    let mut applied = Applied::default();
    let mut references = Vec::new();
    let mut set = |world: &mut World, id: InstanceId, name: &str, value: &WireValue| {
        let Some(value) = value.to_value() else {
            return;
        };
        if matches!(value, Value::Instance(_)) {
            references.push((id, name.to_owned(), value));
        } else {
            let _ = world.set_property(id, name, value);
        }
    };
    for op in &update.ops {
        match op {
            Op::Create(instance) => {
                let id = InstanceId::from_bits(instance.id);
                let Some(class) = ClassName::from_name(&instance.class) else {
                    continue;
                };
                if world.contains(id) || world.create_at(id, class).is_err() {
                    continue;
                }
                let _ = world.set_name(id, instance.name.clone());
                for (name, value) in &instance.properties {
                    set(world, id, name, value);
                }
                for (name, value) in &instance.attributes {
                    if let Some(value) = value.to_value() {
                        let _ = world.set_attribute(id, name, Some(value));
                    }
                }
                for tag in &instance.tags {
                    let _ = world.add_tag(id, tag);
                }
                let parent = InstanceId::from_bits(instance.parent);
                let _ = world.set_parent(id, Some(parent));
                if class.is_base_part() {
                    applied.placed.push(id);
                }
                if class == ClassName::Player && parent == world.players() {
                    applied.players_added.push(id);
                }
            }
            Op::Change(change) => {
                let id = InstanceId::from_bits(change.id);
                if !world.contains(id) {
                    continue;
                }
                if let Some(name) = &change.name {
                    let _ = world.set_name(id, name.clone());
                }
                for (name, value) in &change.properties {
                    set(world, id, name, value);
                }
                if change.properties.iter().any(|(name, _)| name == "Position") {
                    applied.placed.push(id);
                }
                for (name, value) in &change.attributes {
                    let value = value.as_ref().and_then(WireValue::to_value);
                    let _ = world.set_attribute(id, name, value);
                }
                for tag in &change.tags_added {
                    let _ = world.add_tag(id, tag);
                }
                for tag in &change.tags_removed {
                    let _ = world.remove_tag(id, tag);
                }
                if let Some(parent) = change.parent {
                    let _ = world.set_parent(id, Some(InstanceId::from_bits(parent)));
                }
            }
            Op::Destroy { id } => {
                let id = InstanceId::from_bits(*id);
                let is_player = world
                    .get(id)
                    .is_some_and(|instance| instance.class() == ClassName::Player);
                if is_player {
                    applied.players_leaving.push(id);
                } else if world.contains(id) {
                    let _ = world.destroy(id);
                }
            }
        }
    }
    for (id, name, value) in references {
        let _ = world.set_property(id, &name, value);
    }
    applied
}
