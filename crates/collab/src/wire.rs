//! Collaborate's messages as bytes (TC3): a format byte, then postcard, as
//! the game's wire does. Values use `net`'s [`WireValue`] and Instances
//! their network IDs (`InstanceId::to_bits`), so a World travels with every
//! ID it has.
//!
//! A join snapshot carries every Instance under `game` (services included,
//! server-only ones too: collaborators edit ServerScriptService) and every
//! deleted one, each with the properties that differ from its class's
//! defaults. Not yet: Terrain voxels, the string table and the other
//! world-wide settings (later). Files (TC5) travel in pieces by hash.

use axcenity_core::{ClassName, InstanceId, Value, World};
use axcenity_net::WireValue;
use serde::{Deserialize, Serialize};

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::apply::{DropReason, Graveyard, Note, Place, Refusal};
use crate::files::{FileHash, Manifest, Piece};
use crate::history::Checkpoint;
use crate::ids::IdBlock;
use crate::message::{Have, MemberId, Snapshot, ToClient, ToHost};
use crate::op::{NodeSpec, Op, Transaction};
use crate::presence::{Camera, ChatLine, Collaborator, Presence, TextCursor};
use crate::role::Role;

/// The first byte of every message. Raise it when a message changes.
pub const FORMAT: u8 = 6;

/// Why bytes aren't a message.
#[derive(Debug, thiserror::Error)]
pub enum WireError {
    #[error("a Collaborate message from a different version (format {0}, this one reads {FORMAT})")]
    Format(u8),
    #[error("an empty message")]
    Empty,
    #[error("an unreadable message: {0}")]
    Decode(#[from] postcard::Error),
    #[error("a message naming something this version doesn't have: {0}")]
    Unknown(String),
}

pub fn encode_to_host(message: &ToHost) -> Vec<u8> {
    encode(&WireToHost::from(message))
}

pub fn decode_to_host(bytes: &[u8]) -> Result<ToHost, WireError> {
    decode::<WireToHost>(bytes)?.into_message()
}

pub fn encode_to_client(message: &ToClient) -> Vec<u8> {
    encode(&WireToClient::from(message))
}

pub fn decode_to_client(bytes: &[u8]) -> Result<ToClient, WireError> {
    decode::<WireToClient>(bytes)?.into_message()
}

pub(crate) fn encode<T: Serialize>(value: &T) -> Vec<u8> {
    let mut bytes = vec![FORMAT];
    // Writing to a Vec can't fail.
    bytes.extend(postcard::to_stdvec(value).expect("postcard writes to a Vec"));
    bytes
}

pub(crate) fn decode<'a, T: Deserialize<'a>>(bytes: &'a [u8]) -> Result<T, WireError> {
    match bytes.split_first() {
        None => Err(WireError::Empty),
        Some((&FORMAT, rest)) => Ok(postcard::from_bytes(rest)?),
        Some((&other, _)) => Err(WireError::Format(other)),
    }
}

/// File bytes, written as one block (postcard's bytes) rather than one
/// number per byte, which is many times faster for big files.
struct Blob(Vec<u8>);

impl Serialize for Blob {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bytes(&self.0)
    }
}

impl<'de> Deserialize<'de> for Blob {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Blob, D::Error> {
        struct Bytes;
        impl<'de> serde::de::Visitor<'de> for Bytes {
            type Value = Blob;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("bytes")
            }
            fn visit_bytes<E>(self, bytes: &[u8]) -> Result<Blob, E> {
                Ok(Blob(bytes.to_vec()))
            }
            fn visit_byte_buf<E>(self, bytes: Vec<u8>) -> Result<Blob, E> {
                Ok(Blob(bytes))
            }
        }
        deserializer.deserialize_byte_buf(Bytes)
    }
}

fn id(bits: u64) -> InstanceId {
    InstanceId::from_bits(bits)
}

fn values(properties: &[(String, Value)]) -> Vec<(String, WireValue)> {
    properties
        .iter()
        .map(|(name, value)| (name.clone(), WireValue::from_value(value)))
        .collect()
}

fn value(value: &WireValue) -> Result<Value, WireError> {
    value
        .to_value()
        .ok_or_else(|| WireError::Unknown(format!("{value:?}")))
}

fn unwire(properties: &[(String, WireValue)]) -> Result<Vec<(String, Value)>, WireError> {
    properties
        .iter()
        .map(|(name, v)| Ok((name.clone(), value(v)?)))
        .collect()
}

fn class(name: &str) -> Result<ClassName, WireError> {
    ClassName::from_name(name).ok_or_else(|| WireError::Unknown(format!("class {name}")))
}

#[derive(Serialize, Deserialize)]
struct WireNode {
    id: u64,
    class: String,
    parent: u64,
    index: Option<u64>,
    name: String,
    properties: Vec<(String, WireValue)>,
    attributes: Vec<(String, WireValue)>,
    tags: Vec<String>,
}

#[derive(Serialize, Deserialize)]
enum WireOp {
    CreateTree(Vec<WireNode>),
    Reparent {
        id: u64,
        parent: Option<u64>,
        index: Option<u64>,
        restore: bool,
    },
    SetProps {
        id: u64,
        properties: Vec<(String, WireValue)>,
    },
    SetAttribute {
        id: u64,
        name: String,
        value: Option<WireValue>,
    },
    Tag {
        id: u64,
        tag: String,
        added: bool,
    },
    FileVersion {
        path: String,
        base: Option<FileHash>,
        new: Option<FileHash>,
    },
}

impl From<&Op> for WireOp {
    fn from(op: &Op) -> WireOp {
        match op {
            Op::CreateTree { nodes } => WireOp::CreateTree(
                nodes
                    .iter()
                    .map(|node| WireNode {
                        id: node.id.to_bits(),
                        class: node.class.as_str().to_owned(),
                        parent: node.parent.to_bits(),
                        index: node.index.map(|i| i as u64),
                        name: node.name.clone(),
                        properties: values(&node.properties),
                        attributes: values(&node.attributes),
                        tags: node.tags.clone(),
                    })
                    .collect(),
            ),
            Op::Reparent {
                id,
                parent,
                index,
                restore,
            } => WireOp::Reparent {
                id: id.to_bits(),
                parent: parent.map(InstanceId::to_bits),
                index: index.map(|i| i as u64),
                restore: *restore,
            },
            Op::SetProps { id, properties } => WireOp::SetProps {
                id: id.to_bits(),
                properties: values(properties),
            },
            Op::SetAttribute { id, name, value } => WireOp::SetAttribute {
                id: id.to_bits(),
                name: name.clone(),
                value: value.as_ref().map(WireValue::from_value),
            },
            Op::Tag { id, tag, added } => WireOp::Tag {
                id: id.to_bits(),
                tag: tag.clone(),
                added: *added,
            },
            Op::FileVersion { path, base, new } => WireOp::FileVersion {
                path: path.clone(),
                base: *base,
                new: *new,
            },
        }
    }
}

impl WireOp {
    fn into_op(self) -> Result<Op, WireError> {
        Ok(match self {
            WireOp::CreateTree(nodes) => Op::CreateTree {
                nodes: nodes
                    .into_iter()
                    .map(|node| {
                        Ok(NodeSpec {
                            id: id(node.id),
                            class: class(&node.class)?,
                            parent: id(node.parent),
                            index: node.index.map(|i| i as usize),
                            name: node.name,
                            properties: unwire(&node.properties)?,
                            attributes: unwire(&node.attributes)?,
                            tags: node.tags,
                        })
                    })
                    .collect::<Result<_, WireError>>()?,
            },
            WireOp::Reparent {
                id: bits,
                parent,
                index,
                restore,
            } => Op::Reparent {
                id: id(bits),
                parent: parent.map(id),
                index: index.map(|i| i as usize),
                restore,
            },
            WireOp::SetProps {
                id: bits,
                properties,
            } => Op::SetProps {
                id: id(bits),
                properties: unwire(&properties)?,
            },
            WireOp::SetAttribute {
                id: bits,
                name,
                value: v,
            } => Op::SetAttribute {
                id: id(bits),
                name,
                value: v.as_ref().map(value).transpose()?,
            },
            WireOp::Tag {
                id: bits,
                tag,
                added,
            } => Op::Tag {
                id: id(bits),
                tag,
                added,
            },
            WireOp::FileVersion { path, base, new } => Op::FileVersion { path, base, new },
        })
    }
}

fn ops(ops: &[Op]) -> Vec<WireOp> {
    ops.iter().map(WireOp::from).collect()
}

fn unwire_ops(ops: Vec<WireOp>) -> Result<Vec<Op>, WireError> {
    ops.into_iter().map(WireOp::into_op).collect()
}

#[derive(Serialize, Deserialize)]
struct WireTransaction {
    client_seq: u64,
    label: String,
    ops: Vec<WireOp>,
}

#[derive(Serialize, Deserialize)]
enum WireRefusal {
    Permission,
    Cycle(u64),
    NotYourId(u64),
    Invalid(String),
    FileChanged(String),
    Locked(String, u32),
}

impl From<&Refusal> for WireRefusal {
    fn from(refusal: &Refusal) -> WireRefusal {
        match refusal {
            Refusal::Permission => WireRefusal::Permission,
            Refusal::Cycle { id } => WireRefusal::Cycle(id.to_bits()),
            Refusal::NotYourId { id } => WireRefusal::NotYourId(id.to_bits()),
            Refusal::Invalid(why) => WireRefusal::Invalid(why.clone()),
            Refusal::FileChanged { path } => WireRefusal::FileChanged(path.clone()),
            Refusal::Locked { path, by } => WireRefusal::Locked(path.clone(), by.0),
        }
    }
}

impl From<WireRefusal> for Refusal {
    fn from(refusal: WireRefusal) -> Refusal {
        match refusal {
            WireRefusal::Permission => Refusal::Permission,
            WireRefusal::Cycle(bits) => Refusal::Cycle { id: id(bits) },
            WireRefusal::NotYourId(bits) => Refusal::NotYourId { id: id(bits) },
            WireRefusal::Invalid(why) => Refusal::Invalid(why),
            WireRefusal::FileChanged(path) => Refusal::FileChanged { path },
            WireRefusal::Locked(path, by) => Refusal::Locked {
                path,
                by: MemberId(by),
            },
        }
    }
}

fn refusals(list: &[(u64, Refusal)]) -> Vec<(u64, WireRefusal)> {
    list.iter().map(|(seq, r)| (*seq, r.into())).collect()
}

fn unwire_refusals(list: Vec<(u64, WireRefusal)>) -> Vec<(u64, Refusal)> {
    list.into_iter().map(|(seq, r)| (seq, r.into())).collect()
}

#[derive(Serialize, Deserialize)]
enum WireNote {
    Dropped { id: u64, deleted: bool },
    Rehomed { id: u64, to: u64 },
}

/// One Instance of a snapshot. `parent` is `None` for a deleted one (its
/// place is in the graveyard list).
#[derive(Serialize, Deserialize)]
struct WireInstance {
    id: u64,
    class: String,
    parent: Option<u64>,
    name: String,
    properties: Vec<(String, WireValue)>,
    attributes: Vec<(String, WireValue)>,
    tags: Vec<String>,
}

/// A World with every ID it has, and where its deleted Instances were.
#[derive(Serialize, Deserialize)]
struct WireWorld {
    /// Parents before their children, children in order.
    instances: Vec<WireInstance>,
    /// Each deleted Instance: its ID, and its old parent and position.
    graveyard: Vec<(u64, u64, u64)>,
}

/// The properties of `id` that differ from its class's defaults (with
/// Metalness and Roughness whenever Material does, since setting Material
/// resets them).
fn changed_properties(world: &World, id: InstanceId) -> Vec<(String, WireValue)> {
    let Some(instance) = world.get(id) else {
        return Vec::new();
    };
    let defaults = instance.class().default_properties();
    let default = |name: &str| defaults.iter().find(|(n, _)| *n == name).map(|(_, v)| v);
    let material_set = instance
        .property("Material")
        .is_some_and(|v| default("Material") != Some(v));
    instance
        .properties()
        .filter(|(name, value)| {
            (material_set && matches!(*name, "Metalness" | "Roughness"))
                || default(name) != Some(*value)
        })
        .map(|(name, value)| (name.to_owned(), WireValue::from_value(value)))
        .collect()
}

/// `id` as a snapshot sends it. A service (in `game`) is there already on
/// the other side, maybe not with its class's defaults, so all of its
/// properties go.
fn wire_instance(
    world: &World,
    id: InstanceId,
    parent: Option<InstanceId>,
) -> Option<WireInstance> {
    let instance = world.get(id)?;
    let properties = if parent == Some(world.root()) {
        instance
            .properties()
            .map(|(name, value)| (name.to_owned(), WireValue::from_value(value)))
            .collect()
    } else {
        changed_properties(world, id)
    };
    Some(WireInstance {
        id: id.to_bits(),
        class: instance.class().as_str().to_owned(),
        parent: parent.map(InstanceId::to_bits),
        name: instance.name().to_owned(),
        properties,
        attributes: instance
            .attributes()
            .iter()
            .map(|(name, value)| (name.clone(), WireValue::from_value(value)))
            .collect(),
        tags: instance.tags().to_vec(),
    })
}

impl WireWorld {
    fn of(world: &World, graveyard: &Graveyard) -> WireWorld {
        let mut instances = Vec::new();
        let mut add_tree = |top: InstanceId, parent: Option<InstanceId>| {
            instances.extend(wire_instance(world, top, parent));
            for inside in world.descendants(top) {
                let parent = world.get(inside).and_then(|i| i.parent());
                instances.extend(wire_instance(world, inside, parent));
            }
        };
        for &service in world.get(world.root()).map(|r| r.children()).unwrap_or(&[]) {
            add_tree(service, Some(world.root()));
        }
        let mut graves: Vec<_> = graveyard.iter().collect();
        graves.sort_by_key(|(id, _)| id.to_bits());
        for (dead, _) in &graves {
            add_tree(**dead, None);
        }
        WireWorld {
            instances,
            graveyard: graves
                .iter()
                .map(|(id, place)| (id.to_bits(), place.parent.to_bits(), place.index as u64))
                .collect(),
        }
    }

    /// The World, in replica mode, and its graveyard.
    fn into_world(self) -> Result<(World, Graveyard), WireError> {
        let mut world = World::new();
        world.make_replica();
        // Instance references are set last, once what they name exists.
        let mut references = Vec::new();
        for wire in &self.instances {
            let id = id(wire.id);
            if !world.contains(id) {
                world
                    .create_at(id, class(&wire.class)?)
                    .map_err(|e| WireError::Unknown(format!("Instance {id:?}: {e}")))?;
            }
            let _ = world.set_name(id, wire.name.clone());
            for (name, v) in &wire.properties {
                match value(v)? {
                    reference @ Value::Instance(_) => references.push((id, name, reference)),
                    v => {
                        let _ = world.set_property(id, name, v);
                    }
                }
            }
            for (name, v) in &wire.attributes {
                let _ = world.set_attribute(id, name, Some(value(v)?));
            }
            for tag in &wire.tags {
                let _ = world.add_tag(id, tag);
            }
            let parent = wire.parent.map(self::id);
            let current = world.get(id).and_then(|i| i.parent());
            if parent.is_some() && current != parent {
                world
                    .set_parent(id, parent)
                    .map_err(|e| WireError::Unknown(format!("Instance {id:?}: {e}")))?;
            }
        }
        for (id, name, reference) in references {
            let _ = world.set_property(id, name, reference);
        }
        // What a new World starts with that the host's doesn't have.
        let sent: rustc_hash::FxHashSet<u64> = self.instances.iter().map(|i| i.id).collect();
        for extra in world.descendants(world.root()) {
            if !sent.contains(&extra.to_bits()) && world.contains(extra) {
                let _ = world.destroy(extra);
            }
        }
        let graveyard = self
            .graveyard
            .into_iter()
            .map(|(dead, parent, index)| {
                (
                    id(dead),
                    Place {
                        parent: id(parent),
                        index: index as usize,
                    },
                )
            })
            .collect();
        Ok((world, graveyard))
    }
}

#[derive(Serialize, Deserialize)]
enum WireToHost {
    Hello {
        /// Epoch and sequence number.
        have: Option<(u64, u64)>,
    },
    Submit(WireTransaction),
    NeedIds,
    Presence(WirePresence),
    Upload {
        hash: FileHash,
        at: u64,
        total: u64,
        bytes: Blob,
    },
    Fetch(Vec<FileHash>),
    Lock(String),
    Unlock(String),
    DocOpen(String, Blob),
    DocUpdate(String, Blob),
    Chat(String),
    Checkpoint(String),
    Restore(u64),
}

#[derive(Serialize, Deserialize)]
struct WirePresence {
    selection: Vec<u64>,
    camera: Option<([f32; 3], [f32; 3])>,
    editing: Option<String>,
    /// Path, anchor, head.
    script: Option<(String, Blob, Blob)>,
}

impl From<&Presence> for WirePresence {
    fn from(p: &Presence) -> WirePresence {
        WirePresence {
            selection: p.selection.iter().map(|i| i.to_bits()).collect(),
            camera: p.camera.map(|c| (c.position, c.look)),
            editing: p.editing.clone(),
            script: p
                .script
                .as_ref()
                .map(|c| (c.path.clone(), Blob(c.anchor.clone()), Blob(c.head.clone()))),
        }
    }
}

impl From<WirePresence> for Presence {
    fn from(p: WirePresence) -> Presence {
        Presence {
            selection: p.selection.into_iter().map(id).collect(),
            camera: p.camera.map(|(position, look)| Camera { position, look }),
            editing: p.editing,
            script: p.script.map(|(path, anchor, head)| TextCursor {
                path,
                anchor: anchor.0,
                head: head.0,
            }),
        }
    }
}

impl From<&ToHost> for WireToHost {
    fn from(message: &ToHost) -> WireToHost {
        match message {
            ToHost::Hello { have } => WireToHost::Hello {
                have: have.map(|have| (have.epoch, have.seq)),
            },
            ToHost::Submit(transaction) => WireToHost::Submit(WireTransaction {
                client_seq: transaction.client_seq,
                label: transaction.label.clone(),
                ops: ops(&transaction.ops),
            }),
            ToHost::NeedIds => WireToHost::NeedIds,
            ToHost::Presence(p) => WireToHost::Presence(p.into()),
            ToHost::Upload {
                hash,
                at,
                total,
                bytes,
            } => WireToHost::Upload {
                hash: *hash,
                at: *at,
                total: *total,
                bytes: Blob(bytes.0.to_vec()),
            },
            ToHost::Fetch(hashes) => WireToHost::Fetch(hashes.clone()),
            ToHost::Lock { path } => WireToHost::Lock(path.clone()),
            ToHost::Unlock { path } => WireToHost::Unlock(path.clone()),
            ToHost::DocOpen { path, state_vector } => {
                WireToHost::DocOpen(path.clone(), Blob(state_vector.clone()))
            }
            ToHost::DocUpdate { path, update } => {
                WireToHost::DocUpdate(path.clone(), Blob(update.clone()))
            }
            ToHost::Chat(text) => WireToHost::Chat(text.clone()),
            ToHost::Checkpoint(label) => WireToHost::Checkpoint(label.clone()),
            ToHost::Restore(id) => WireToHost::Restore(*id),
        }
    }
}

impl WireToHost {
    fn into_message(self) -> Result<ToHost, WireError> {
        Ok(match self {
            WireToHost::Hello { have } => ToHost::Hello {
                have: have.map(|(epoch, seq)| Have { epoch, seq }),
            },
            WireToHost::Submit(t) => ToHost::Submit(Transaction {
                client_seq: t.client_seq,
                label: t.label,
                ops: unwire_ops(t.ops)?,
            }),
            WireToHost::NeedIds => ToHost::NeedIds,
            WireToHost::Presence(p) => ToHost::Presence(p.into()),
            WireToHost::Upload {
                hash,
                at,
                total,
                bytes,
            } => ToHost::Upload {
                hash,
                at,
                total,
                bytes: Piece(Arc::from(bytes.0)),
            },
            WireToHost::Fetch(hashes) => ToHost::Fetch(hashes),
            WireToHost::Lock(path) => ToHost::Lock { path },
            WireToHost::Unlock(path) => ToHost::Unlock { path },
            WireToHost::DocOpen(path, state_vector) => ToHost::DocOpen {
                path,
                state_vector: state_vector.0,
            },
            WireToHost::DocUpdate(path, update) => ToHost::DocUpdate {
                path,
                update: update.0,
            },
            WireToHost::Chat(text) => ToHost::Chat(text),
            WireToHost::Checkpoint(label) => ToHost::Checkpoint(label),
            WireToHost::Restore(id) => ToHost::Restore(id),
        })
    }
}

#[derive(Serialize, Deserialize)]
enum WireToClient {
    Welcome {
        world: WireWorld,
        seq: u64,
        last_client_seq: u64,
        role: Role,
        ids: Option<IdBlock>,
        rejected: Vec<(u64, WireRefusal)>,
        files: Manifest,
        session: String,
        epoch: u64,
    },
    Resumed {
        last_client_seq: u64,
        role: Role,
        rejected: Vec<(u64, WireRefusal)>,
    },
    Accepted {
        seq: u64,
        author: u32,
        client_seq: u64,
        label: String,
        ops: Vec<WireOp>,
        notes: Vec<WireNote>,
    },
    Rejected {
        client_seq: u64,
        reason: WireRefusal,
    },
    Ids(IdBlock),
    RoleChanged(Role),
    Roster(Vec<Collaborator>),
    Presence {
        member: u32,
        presence: WirePresence,
    },
    File {
        hash: FileHash,
        at: u64,
        total: u64,
        bytes: Blob,
    },
    Missing(FileHash),
    Locks(BTreeMap<String, u32>),
    Ended,
    DocState(String, Blob, Blob, u64),
    DocUpdate(String, Blob, u32),
    DocRefused(String, String),
    Chat(ChatLine),
    ChatHistory(Vec<ChatLine>),
    Checkpoints(Vec<Checkpoint>),
    HistoryNotice(String),
}

impl From<&ToClient> for WireToClient {
    fn from(message: &ToClient) -> WireToClient {
        match message {
            ToClient::Welcome {
                world,
                graveyard,
                seq,
                last_client_seq,
                role,
                ids,
                rejected,
                files,
                session,
                epoch,
            } => WireToClient::Welcome {
                world: WireWorld::of(&world.0, graveyard),
                seq: *seq,
                last_client_seq: *last_client_seq,
                role: *role,
                ids: *ids,
                rejected: refusals(rejected),
                files: files.clone(),
                session: session.clone(),
                epoch: *epoch,
            },
            ToClient::Resumed {
                last_client_seq,
                role,
                rejected,
            } => WireToClient::Resumed {
                last_client_seq: *last_client_seq,
                role: *role,
                rejected: refusals(rejected),
            },
            ToClient::Accepted {
                seq,
                author,
                client_seq,
                label,
                ops: list,
                notes,
            } => WireToClient::Accepted {
                seq: *seq,
                author: author.0,
                client_seq: *client_seq,
                label: label.clone(),
                ops: ops(list),
                notes: notes
                    .iter()
                    .map(|note| match note {
                        Note::Dropped { id, reason } => WireNote::Dropped {
                            id: id.to_bits(),
                            deleted: *reason == DropReason::Deleted,
                        },
                        Note::Rehomed { id, to } => WireNote::Rehomed {
                            id: id.to_bits(),
                            to: to.to_bits(),
                        },
                    })
                    .collect(),
            },
            ToClient::Rejected { client_seq, reason } => WireToClient::Rejected {
                client_seq: *client_seq,
                reason: reason.into(),
            },
            ToClient::Ids(block) => WireToClient::Ids(*block),
            ToClient::RoleChanged(role) => WireToClient::RoleChanged(*role),
            ToClient::Roster(roster) => WireToClient::Roster(roster.clone()),
            ToClient::Presence { member, presence } => WireToClient::Presence {
                member: member.0,
                presence: presence.into(),
            },
            ToClient::File {
                hash,
                at,
                total,
                bytes,
            } => WireToClient::File {
                hash: *hash,
                at: *at,
                total: *total,
                bytes: Blob(bytes.0.to_vec()),
            },
            ToClient::Missing(hash) => WireToClient::Missing(*hash),
            ToClient::Locks(locks) => WireToClient::Locks(
                locks
                    .iter()
                    .map(|(path, member)| (path.clone(), member.0))
                    .collect(),
            ),
            ToClient::Ended => WireToClient::Ended,
            ToClient::DocState {
                path,
                update,
                state_vector,
                origin,
            } => WireToClient::DocState(
                path.clone(),
                Blob(update.clone()),
                Blob(state_vector.clone()),
                *origin,
            ),
            ToClient::DocUpdate {
                path,
                update,
                author,
            } => WireToClient::DocUpdate(path.clone(), Blob(update.clone()), author.0),
            ToClient::DocRefused { path, reason } => {
                WireToClient::DocRefused(path.clone(), reason.clone())
            }
            ToClient::Chat(line) => WireToClient::Chat(line.clone()),
            ToClient::ChatHistory(lines) => WireToClient::ChatHistory(lines.clone()),
            ToClient::Checkpoints(list) => WireToClient::Checkpoints(list.clone()),
            ToClient::HistoryNotice(reason) => WireToClient::HistoryNotice(reason.clone()),
        }
    }
}

impl WireToClient {
    fn into_message(self) -> Result<ToClient, WireError> {
        Ok(match self {
            WireToClient::Welcome {
                world,
                seq,
                last_client_seq,
                role,
                ids,
                rejected,
                files,
                session,
                epoch,
            } => {
                let (world, graveyard) = world.into_world()?;
                ToClient::Welcome {
                    world: Snapshot(Box::new(world)),
                    graveyard,
                    seq,
                    last_client_seq,
                    role,
                    ids,
                    rejected: unwire_refusals(rejected),
                    files,
                    session,
                    epoch,
                }
            }
            WireToClient::Resumed {
                last_client_seq,
                role,
                rejected,
            } => ToClient::Resumed {
                last_client_seq,
                role,
                rejected: unwire_refusals(rejected),
            },
            WireToClient::Accepted {
                seq,
                author,
                client_seq,
                label,
                ops,
                notes,
            } => ToClient::Accepted {
                seq,
                author: MemberId(author),
                client_seq,
                label,
                ops: unwire_ops(ops)?,
                notes: notes
                    .into_iter()
                    .map(|note| match note {
                        WireNote::Dropped { id: bits, deleted } => Note::Dropped {
                            id: id(bits),
                            reason: if deleted {
                                DropReason::Deleted
                            } else {
                                DropReason::Gone
                            },
                        },
                        WireNote::Rehomed { id: bits, to } => Note::Rehomed {
                            id: id(bits),
                            to: id(to),
                        },
                    })
                    .collect(),
            },
            WireToClient::Rejected { client_seq, reason } => ToClient::Rejected {
                client_seq,
                reason: reason.into(),
            },
            WireToClient::Ids(block) => ToClient::Ids(block),
            WireToClient::RoleChanged(role) => ToClient::RoleChanged(role),
            WireToClient::Roster(roster) => ToClient::Roster(roster),
            WireToClient::Presence { member, presence } => ToClient::Presence {
                member: MemberId(member),
                presence: presence.into(),
            },
            WireToClient::File {
                hash,
                at,
                total,
                bytes,
            } => ToClient::File {
                hash,
                at,
                total,
                bytes: Piece(Arc::from(bytes.0)),
            },
            WireToClient::Missing(hash) => ToClient::Missing(hash),
            WireToClient::Locks(locks) => ToClient::Locks(
                locks
                    .into_iter()
                    .map(|(path, member)| (path, MemberId(member)))
                    .collect(),
            ),
            WireToClient::Ended => ToClient::Ended,
            WireToClient::DocState(path, update, state_vector, origin) => ToClient::DocState {
                path,
                update: update.0,
                state_vector: state_vector.0,
                origin,
            },
            WireToClient::DocUpdate(path, update, author) => ToClient::DocUpdate {
                path,
                update: update.0,
                author: MemberId(author),
            },
            WireToClient::DocRefused(path, reason) => ToClient::DocRefused { path, reason },
            WireToClient::Chat(line) => ToClient::Chat(line),
            WireToClient::ChatHistory(lines) => ToClient::ChatHistory(lines),
            WireToClient::Checkpoints(list) => ToClient::Checkpoints(list),
            WireToClient::HistoryNotice(reason) => ToClient::HistoryNotice(reason),
        })
    }
}
