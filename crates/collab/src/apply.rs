//! Applying operations to a [`Doc`] with Team Create's conflict rules, and
//! undoing them exactly.
//!
//! The host and every client run the same code: the host to decide what a
//! member's transaction does, a client to show its own edits at once and to
//! take them off and put them back around the host's (see [`crate::client`]).
//!
//! | Situation | Rule | Here |
//! |---|---|---|
//! | Two writes to one property | The later one (host order) wins | `SetProps` just sets |
//! | Edit or move of a deleted Instance | Delete wins: the op is dropped | [`Note::Dropped`] |
//! | Create or move into a deleted parent | Goes to its nearest surviving ancestor | [`Note::Rehomed`] |
//! | Move into itself or its own descendant | The whole transaction is refused | [`Refusal::Cycle`] |
//! | Bad class, property, type or ID | The whole transaction is refused | [`Refusal::Invalid`], [`Refusal::NotYourId`] |
//!
//! A transaction applies completely or not at all: on a refusal, what its
//! earlier ops did is undone before returning.

use std::collections::BTreeMap;

use axcenity_core::{InstanceId, TreeError, Value, World};
use rustc_hash::FxHashMap;

use crate::op::{NodeSpec, Op};

/// Where an Instance sat: its parent and its position among the parent's
/// children (0 is the first).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Place {
    pub parent: InstanceId,
    pub index: usize,
}

/// A World as Team Create edits it, with where each deleted Instance was,
/// so something made inside it meanwhile can go to the nearest place that
/// still exists.
#[derive(Clone)]
pub struct Doc {
    pub world: World,
    /// Deleted (detached) Instances and the place they were deleted from.
    pub graveyard: FxHashMap<InstanceId, Place>,
}

impl Doc {
    /// A document for `world`, which is switched to replica mode
    /// ([`World::make_replica`]) so local `create` calls can't take an ID
    /// another writer will use.
    pub fn new(mut world: World) -> Doc {
        world.make_replica();
        Doc {
            world,
            graveyard: FxHashMap::default(),
        }
    }

    /// True if `id` is `game` or inside it (not deleted).
    pub fn in_tree(&self, id: InstanceId) -> bool {
        let root = self.world.root();
        id == root || self.world.is_descendant_of(id, root)
    }
}

/// Why an op was skipped while the rest of its transaction applied.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DropReason {
    /// Someone deleted the Instance first.
    Deleted,
    /// The Instance doesn't exist in this session.
    Gone,
}

/// Something a transaction's author should hear about although it applied.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Note {
    /// An op was skipped.
    Dropped { id: InstanceId, reason: DropReason },
    /// `id` was made or moved into a parent that was deleted meanwhile, so
    /// it went to `to`, the nearest place still in the game.
    Rehomed { id: InstanceId, to: InstanceId },
}

/// Why a whole transaction was refused.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Refusal {
    #[error("your role can't change this project")]
    Permission,
    #[error(
        "an Instance can't go inside itself or one of its descendants (someone may have moved it)"
    )]
    Cycle { id: InstanceId },
    #[error("a new Instance must use an ID from your own block")]
    NotYourId { id: InstanceId },
    #[error("{0}")]
    Invalid(String),
}

/// What a transaction did, and how to take it off again.
#[derive(Debug, Default)]
pub struct Applied {
    /// The ops as they were applied: dropped ones left out, rehomed ones
    /// with their new parent. Applying exactly these to another copy in
    /// the same state gives the same result, with no notes.
    pub ops: Vec<Op>,
    pub notes: Vec<Note>,
    pub undo: Undo,
}

/// How to put a [`Doc`] back as it was before a transaction. Undo with
/// [`undo`]; it restores names, properties (Material first, as the
/// editor's own undo does), attributes, tag order, child order and the
/// graveyard exactly.
#[derive(Debug, Default)]
pub struct Undo(Vec<Inverse>);

#[derive(Debug)]
enum Inverse {
    /// Destroy something the transaction made.
    Uncreate(InstanceId),
    /// Put an Instance back where it was (or out of the tree), and its
    /// graveyard entry back as it was.
    Put {
        id: InstanceId,
        place: Option<Place>,
        grave: Option<Place>,
    },
    /// Earlier values of the fields that changed, Material first.
    Fields {
        id: InstanceId,
        name: Option<String>,
        properties: Vec<(&'static str, Value)>,
    },
    Attribute {
        id: InstanceId,
        name: String,
        value: Option<Value>,
    },
    /// The whole tag list as it was, in order.
    Tags { id: InstanceId, tags: Vec<String> },
}

/// Applies `ops` in order. `owns` says whether a new Instance may take an
/// ID (the host checks the author's blocks; a client trusts the host).
pub fn apply(
    doc: &mut Doc,
    ops: &[Op],
    owns: &dyn Fn(InstanceId) -> bool,
) -> Result<Applied, Refusal> {
    let mut applied = Applied::default();
    for op in ops {
        match apply_op(doc, op, owns, &mut applied) {
            Ok(()) => {}
            Err(refusal) => {
                undo(doc, std::mem::take(&mut applied.undo));
                return Err(refusal);
            }
        }
    }
    Ok(applied)
}

/// Takes a transaction off again, newest op first.
pub fn undo(doc: &mut Doc, undo: Undo) {
    for inverse in undo.0.into_iter().rev() {
        match inverse {
            Inverse::Uncreate(id) => {
                let _ = doc.world.destroy(id);
            }
            Inverse::Put { id, place, grave } => {
                // Putting back where it was can't make a cycle: everything
                // after this op has been undone already.
                let _ = put(&mut doc.world, id, place);
                match grave {
                    Some(grave) => doc.graveyard.insert(id, grave),
                    None => doc.graveyard.remove(&id),
                };
            }
            Inverse::Fields {
                id,
                name,
                properties,
            } => {
                if let Some(name) = name {
                    let _ = doc.world.set_name(id, name);
                }
                for (key, value) in properties {
                    let _ = doc.world.set_known_property(id, key, value);
                }
            }
            Inverse::Attribute { id, name, value } => {
                let _ = doc.world.set_attribute(id, &name, value);
            }
            Inverse::Tags { id, tags } => set_tags(&mut doc.world, id, &tags),
        }
    }
}

fn apply_op(
    doc: &mut Doc,
    op: &Op,
    owns: &dyn Fn(InstanceId) -> bool,
    applied: &mut Applied,
) -> Result<(), Refusal> {
    match op {
        Op::CreateTree { nodes } => create_tree(doc, nodes, owns, applied),
        Op::Reparent {
            id,
            parent,
            index,
            restore,
        } => reparent(doc, *id, *parent, *index, *restore, applied),
        Op::SetProps { id, properties } => {
            if let Some(reason) = cannot_edit(doc, *id) {
                applied.notes.push(Note::Dropped { id: *id, reason });
                return Ok(());
            }
            let before = fields_of(&doc.world, *id).expect("checked it exists");
            set_props(doc, *id, properties)?;
            let (name, properties) = changed_fields(&doc.world, *id, &before);
            applied.undo.0.push(Inverse::Fields {
                id: *id,
                name,
                properties,
            });
            applied.ops.push(op.clone());
            Ok(())
        }
        Op::SetAttribute { id, name, value } => {
            if let Some(reason) = cannot_edit(doc, *id) {
                applied.notes.push(Note::Dropped { id: *id, reason });
                return Ok(());
            }
            let before = doc.world.get(*id).and_then(|i| i.attribute(name)).cloned();
            doc.world
                .set_attribute(*id, name, value.clone())
                .map_err(|error| Refusal::Invalid(error.to_string()))?;
            applied.undo.0.push(Inverse::Attribute {
                id: *id,
                name: name.clone(),
                value: before,
            });
            applied.ops.push(op.clone());
            Ok(())
        }
        Op::Tag { id, tag, added } => {
            if let Some(reason) = cannot_edit(doc, *id) {
                applied.notes.push(Note::Dropped { id: *id, reason });
                return Ok(());
            }
            let before = doc
                .world
                .get(*id)
                .map(|i| i.tags().to_vec())
                .unwrap_or_default();
            let result = if *added {
                doc.world.add_tag(*id, tag)
            } else {
                doc.world.remove_tag(*id, tag)
            };
            result.map_err(|error| Refusal::Invalid(error.to_string()))?;
            applied.undo.0.push(Inverse::Tags {
                id: *id,
                tags: before,
            });
            applied.ops.push(op.clone());
            Ok(())
        }
    }
}

/// Why an edit of `id` is skipped, if it is: delete wins.
fn cannot_edit(doc: &Doc, id: InstanceId) -> Option<DropReason> {
    if !doc.world.contains(id) {
        Some(DropReason::Gone)
    } else if !doc.in_tree(id) {
        Some(DropReason::Deleted)
    } else {
        None
    }
}

fn create_tree(
    doc: &mut Doc,
    nodes: &[NodeSpec],
    owns: &dyn Fn(InstanceId) -> bool,
    applied: &mut Applied,
) -> Result<(), Refusal> {
    if nodes.is_empty() {
        return Ok(());
    }
    let mut made: Vec<InstanceId> = Vec::with_capacity(nodes.len());
    let mut final_nodes = Vec::with_capacity(nodes.len());
    let mut notes = Vec::new();
    let result = (|| {
        for node in nodes {
            if node.id.is_local() || doc.world.contains(node.id) || made.contains(&node.id) {
                return Err(Refusal::Invalid(format!(
                    "{:?} can't be made at an ID that is taken",
                    node.class
                )));
            }
            if !owns(node.id) {
                return Err(Refusal::NotYourId { id: node.id });
            }
            let mut node = node.clone();
            if !made.contains(&node.parent) {
                let to = surviving(doc, node.parent);
                if to != node.parent {
                    notes.push(Note::Rehomed { id: node.id, to });
                    node.parent = to;
                    node.index = None;
                }
            }
            doc.world
                .create_at(node.id, node.class)
                .map_err(|error| Refusal::Invalid(error.to_string()))?;
            made.push(node.id);
            doc.world
                .set_name(node.id, node.name.clone())
                .map_err(|error| Refusal::Invalid(error.to_string()))?;
            set_props(doc, node.id, &node.properties)?;
            for (name, value) in &node.attributes {
                doc.world
                    .set_attribute(node.id, name, Some(value.clone()))
                    .map_err(|error| Refusal::Invalid(error.to_string()))?;
            }
            for tag in &node.tags {
                doc.world
                    .add_tag(node.id, tag)
                    .map_err(|error| Refusal::Invalid(error.to_string()))?;
            }
            put(
                &mut doc.world,
                node.id,
                Some(Place {
                    parent: node.parent,
                    index: node.index.unwrap_or(usize::MAX),
                }),
            )
            .map_err(|error| Refusal::Invalid(error.to_string()))?;
            final_nodes.push(node);
        }
        Ok(())
    })();
    if let Err(refusal) = result {
        for id in made.into_iter().rev() {
            let _ = doc.world.destroy(id);
        }
        return Err(refusal);
    }
    // Destroying the tops destroys what's inside them, so only they need
    // undoing.
    for node in &final_nodes {
        if !final_nodes.iter().any(|other| other.id == node.parent) {
            applied.undo.0.push(Inverse::Uncreate(node.id));
        }
    }
    applied.notes.extend(notes);
    applied.ops.push(Op::CreateTree { nodes: final_nodes });
    Ok(())
}

fn reparent(
    doc: &mut Doc,
    id: InstanceId,
    parent: Option<InstanceId>,
    index: Option<usize>,
    restore: bool,
    applied: &mut Applied,
) -> Result<(), Refusal> {
    if !doc.world.contains(id) {
        applied.notes.push(Note::Dropped {
            id,
            reason: DropReason::Gone,
        });
        return Ok(());
    }
    let attached = doc.in_tree(id);
    if !attached && !restore {
        // Deleted by someone else first (or a delete of something already
        // deleted): delete wins.
        applied.notes.push(Note::Dropped {
            id,
            reason: DropReason::Deleted,
        });
        return Ok(());
    }
    if parent.is_none() && !attached {
        // Deleting something that's already out of the tree.
        return Ok(());
    }
    let before = place_of(&doc.world, id);
    let grave_before = doc.graveyard.get(&id).copied();
    let mut final_parent = parent;
    let mut final_index = index;
    if let Some(wanted) = parent {
        let to = surviving(doc, wanted);
        if to != wanted {
            applied.notes.push(Note::Rehomed { id, to });
            final_parent = Some(to);
            final_index = None;
        }
    }
    let place = final_parent.map(|parent| Place {
        parent,
        index: final_index.unwrap_or(usize::MAX),
    });
    put(&mut doc.world, id, place).map_err(|error| match error {
        TreeError::Cycle => Refusal::Cycle { id },
        other => Refusal::Invalid(other.to_string()),
    })?;
    match (final_parent, before) {
        (None, Some(before)) => {
            doc.graveyard.insert(id, before);
        }
        _ => {
            doc.graveyard.remove(&id);
        }
    }
    applied.undo.0.push(Inverse::Put {
        id,
        place: before,
        grave: grave_before,
    });
    applied.ops.push(Op::Reparent {
        id,
        parent: final_parent,
        index: final_index,
        restore,
    });
    Ok(())
}

/// `parent` if it's in the game; otherwise the nearest place up from where
/// it was deleted that still is, or the Workspace if there's none.
fn surviving(doc: &Doc, parent: InstanceId) -> InstanceId {
    let mut current = parent;
    // A chain of deleted parents is short; the bound only guards against a
    // graveyard that loops.
    for _ in 0..10_000 {
        if doc.in_tree(current) {
            return current;
        }
        let up = doc
            .world
            .get(current)
            .and_then(|instance| instance.parent())
            .or_else(|| doc.graveyard.get(&current).map(|place| place.parent));
        match up {
            Some(up) => current = up,
            None => break,
        }
    }
    doc.world.workspace()
}

/// Sets `properties` on `id` in order; on an error, puts back what this
/// call already changed.
fn set_props(doc: &mut Doc, id: InstanceId, properties: &[(String, Value)]) -> Result<(), Refusal> {
    let before = fields_of(&doc.world, id);
    for (name, value) in properties {
        let result = match name.as_str() {
            "Name" => match value {
                Value::String(text) => doc
                    .world
                    .set_name(id, text.clone())
                    .map_err(|e| e.to_string()),
                _ => Err("Name must be a string".to_owned()),
            },
            "Parent" => Err("Parent is changed by moving, not by setting it".to_owned()),
            _ => doc
                .world
                .set_property(id, name, value.clone())
                .map_err(|e| e.to_string()),
        };
        if let Err(message) = result {
            if let Some((name, properties)) = before {
                restore_fields(&mut doc.world, id, name, properties);
            }
            return Err(Refusal::Invalid(message));
        }
    }
    Ok(())
}

type Fields = (String, BTreeMap<&'static str, Value>);

/// What setting `Material` also sets (`World::set_property`).
const MATERIAL_RESETS: [&str; 2] = ["Metalness", "Roughness"];

fn fields_of(world: &World, id: InstanceId) -> Option<Fields> {
    let instance = world.get(id)?;
    Some((
        instance.name().to_owned(),
        instance
            .properties()
            .map(|(key, value)| (key, value.clone()))
            .collect(),
    ))
}

/// The fields of `id` that differ from `before`, with their `before`
/// values, Material first so restoring it doesn't undo Metalness and
/// Roughness put back after it.
fn changed_fields(
    world: &World,
    id: InstanceId,
    before: &Fields,
) -> (Option<String>, Vec<(&'static str, Value)>) {
    let Some(now) = fields_of(world, id) else {
        return (None, Vec::new());
    };
    let name = (now.0 != before.0).then(|| before.0.clone());
    let material_changed = before.1.get("Material") != now.1.get("Material");
    let mut properties: Vec<(&'static str, Value)> = before
        .1
        .iter()
        .filter(|(key, value)| {
            now.1.get(*key) != Some(*value)
                // Putting Material back resets these to its preset, so they
                // must be put back after it even if they ended where they
                // started.
                || (material_changed && MATERIAL_RESETS.contains(*key))
        })
        .map(|(key, value)| (*key, value.clone()))
        .collect();
    properties.sort_by_key(|(key, _)| *key != "Material");
    (name, properties)
}

fn restore_fields(
    world: &mut World,
    id: InstanceId,
    name: String,
    properties: BTreeMap<&'static str, Value>,
) {
    let before = (name, properties);
    let (name, properties) = changed_fields(world, id, &before);
    if let Some(name) = name {
        let _ = world.set_name(id, name);
    }
    for (key, value) in properties {
        let _ = world.set_known_property(id, key, value);
    }
}

/// Where `id` is in the tree now, or `None` if it has no parent.
pub fn place_of(world: &World, id: InstanceId) -> Option<Place> {
    let parent = world.get(id)?.parent()?;
    let index = world
        .get(parent)?
        .children()
        .iter()
        .position(|&child| child == id)?;
    Some(Place { parent, index })
}

fn put(world: &mut World, id: InstanceId, place: Option<Place>) -> Result<(), TreeError> {
    match place {
        Some(place) => world.set_parent_at(id, place.parent, place.index),
        None => world.set_parent(id, None),
    }
}

fn set_tags(world: &mut World, id: InstanceId, tags: &[String]) {
    let now = world.get(id).map(|i| i.tags().to_vec()).unwrap_or_default();
    for tag in now {
        let _ = world.remove_tag(id, &tag);
    }
    for tag in tags {
        let _ = world.add_tag(id, tag);
    }
}
