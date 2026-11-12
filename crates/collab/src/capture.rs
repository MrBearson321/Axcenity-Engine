//! Turning edits made straight on a World (by any editor tool, a plugin or
//! an AI client) into Collaborate operations, from the World's edit journal
//! ([`World::record_edits`]).
//!
//! [`capture`] reads what changed and builds the ops from the World as it
//! is now, then puts the World back as it was before those edits, using the
//! values the journal kept. The client then applies the ops with
//! [`apply`](crate::apply::apply), so the edit is on the World exactly as
//! every other copy will apply it, and comes with an exact undo. No tool
//! needs code of its own for Collaborate.
//!
//! Left alone: Instances only this computer has (`InstanceId::is_local`),
//! and new Instances that are still outside the tree (kept in `unsent` until
//! they're placed, since a tool may still be building them). A new Instance
//! put inside a shared one that's then deleted is sent too, so it's deleted
//! with it everywhere.

use axcenity_core::{Edit, InstanceId, Value, World};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::apply::{Graveyard, Place, in_tree, place_of, put, set_tags};
use crate::op::{NodeSpec, Op};

/// What [`capture`] found.
#[derive(Debug, Default)]
pub struct Captured {
    /// The ops to apply to the World (now back as it was before the edits).
    pub ops: Vec<Op>,
    /// Shared Instances a tool destroyed for good; they're sent as deletes
    /// but can't be kept here as deleted ones are. (A tool should take them
    /// out of the tree instead, as the editor's Delete does.)
    pub destroyed: Vec<InstanceId>,
}

/// What one Instance's edits since the last capture changed, with the
/// values from before the first of them.
#[derive(Default)]
struct Touched {
    /// Where it was before its first move (`Some(None)`: it had no parent).
    parent: Option<Option<(InstanceId, usize)>>,
    /// Field names in the order first changed, with their first value.
    fields: Vec<(&'static str, Value)>,
    attributes: Vec<(String, Option<Value>)>,
    tags: Option<Vec<String>>,
}

/// Reads and clears `world`'s edit journal, puts the World back as it was
/// before those edits, and returns the ops that redo them. `unsent` holds
/// new Instances not yet sent (a `CreateTree` sends them).
pub fn capture(
    world: &mut World,
    graveyard: &Graveyard,
    unsent: &mut FxHashSet<InstanceId>,
) -> Captured {
    let edits = world.take_edits();
    let mut captured = Captured::default();
    if edits.is_empty() {
        return captured;
    }
    let mut touched: FxHashMap<InstanceId, Touched> = FxHashMap::default();
    let mut order: Vec<InstanceId> = Vec::new();
    for edit in &edits {
        match edit {
            Edit::Created(id) if !id.is_local() => {
                unsent.insert(*id);
            }
            Edit::Destroyed(id) if !id.is_local() => {
                if !unsent.remove(id) {
                    captured.destroyed.push(*id);
                }
            }
            Edit::Parent { id, before } if !id.is_local() => {
                let t = touch(&mut touched, &mut order, *id);
                t.parent.get_or_insert(*before);
            }
            Edit::Name { id, before } if !id.is_local() => {
                let t = touch(&mut touched, &mut order, *id);
                if !t.fields.iter().any(|(name, _)| *name == "Name") {
                    t.fields.push(("Name", Value::String(before.clone())));
                }
            }
            Edit::Property { id, name, before } if !id.is_local() => {
                let t = touch(&mut touched, &mut order, *id);
                if !t.fields.iter().any(|(known, _)| known == name) {
                    t.fields.push((name, before.clone()));
                }
            }
            Edit::Attribute { id, name, before } if !id.is_local() => {
                let t = touch(&mut touched, &mut order, *id);
                if !t.attributes.iter().any(|(known, _)| known == name) {
                    t.attributes.push((name.clone(), before.clone()));
                }
            }
            Edit::Tags { id, before } if !id.is_local() => {
                let t = touch(&mut touched, &mut order, *id);
                t.tags.get_or_insert_with(|| before.clone());
            }
            _ => {}
        }
    }

    // New Instances placed in the game since the last capture, or inside a
    // shared Instance taken out of it (they go with it, as a delete does).
    // The rest are still being built.
    let root = world.root();
    let placed: Vec<InstanceId> = unsent
        .iter()
        .copied()
        .filter(|&id| {
            let top = top_of(world, id);
            top == root || (top != id && !top.is_local() && !unsent.contains(&top))
        })
        .collect();
    let new: FxHashSet<InstanceId> = placed.iter().copied().collect();
    for id in &placed {
        unsent.remove(id);
    }

    // Shared Instances moved into the tree (including restores), or out of
    // it from a place other copies have (deletes).
    let shared = |id: &InstanceId| !new.contains(id) && !unsent.contains(id) && world.contains(*id);
    let mut attaches: Vec<InstanceId> = Vec::new();
    let mut deletes: Vec<InstanceId> = Vec::new();
    for id in order.iter().filter(|id| shared(id)) {
        let Some(before) = touched[id].parent else {
            continue;
        };
        let now = place_of(world, *id).map(|p| (p.parent, p.index));
        if now == before {
            continue;
        }
        if in_tree(world, *id) {
            attaches.push(*id);
        } else if before
            .is_some_and(|(parent, _)| in_tree(world, parent) || graveyard.contains_key(&parent))
        {
            deletes.push(*id);
        }
    }

    // Creates and moves into the tree in tree order, so a parent is there
    // before its children whichever kind each is (a new Model holding
    // existing Parts, a new Part inside a restored Folder); then new
    // Instances inside deleted ones, parents first; then field, attribute
    // and tag changes; then deletes. So each op finds what it edits.
    let mut arrivals = world.in_tree_order(attaches.iter().chain(&placed).copied().collect());
    let mut hidden: Vec<InstanceId> = placed
        .iter()
        .copied()
        .filter(|&id| !in_tree(world, id))
        .collect();
    hidden.sort_by_key(|&id| (top_of(world, id).to_bits(), depth(world, id), id.to_bits()));
    arrivals.extend(hidden);
    let mut ops = Vec::new();
    let mut nodes: Vec<NodeSpec> = Vec::new();
    for id in arrivals {
        if new.contains(&id) {
            nodes.extend(node(world, id));
            continue;
        }
        if !nodes.is_empty() {
            ops.push(Op::CreateTree {
                nodes: std::mem::take(&mut nodes),
            });
        }
        let place = place_of(world, id).expect("in the tree");
        ops.push(Op::Reparent {
            id,
            parent: Some(place.parent),
            index: Some(place.index),
            restore: touched[&id].parent == Some(None) || graveyard.contains_key(&id),
        });
    }
    if !nodes.is_empty() {
        ops.push(Op::CreateTree { nodes });
    }
    for id in order.iter().filter(|id| shared(id)) {
        let t = &touched[id];
        let Some(instance) = world.get(*id) else {
            continue;
        };
        let mut properties: Vec<(String, Value)> = Vec::new();
        for (name, before) in &t.fields {
            let now = if *name == "Name" {
                Value::String(instance.name().to_owned())
            } else {
                match instance.property(name) {
                    Some(value) => value.clone(),
                    None => continue,
                }
            };
            if now != *before {
                properties.push(((*name).to_owned(), now));
            }
        }
        // Material first: setting it resets Metalness and Roughness.
        properties.sort_by_key(|(name, _)| name != "Material");
        if !properties.is_empty() {
            ops.push(Op::SetProps {
                id: *id,
                properties,
            });
        }
        for (name, before) in &t.attributes {
            let now = instance.attribute(name).cloned();
            if now != *before {
                ops.push(Op::SetAttribute {
                    id: *id,
                    name: name.clone(),
                    value: now,
                });
            }
        }
        if let Some(before) = &t.tags
            && instance.tags() != before.as_slice()
        {
            // Off and on again, so the order comes out the same everywhere.
            for tag in before {
                ops.push(Op::Tag {
                    id: *id,
                    tag: tag.clone(),
                    added: false,
                });
            }
            for tag in instance.tags() {
                ops.push(Op::Tag {
                    id: *id,
                    tag: tag.clone(),
                    added: true,
                });
            }
        }
    }
    for id in deletes
        .into_iter()
        .chain(captured.destroyed.iter().copied())
    {
        ops.push(Op::Reparent {
            id,
            parent: None,
            index: None,
            restore: false,
        });
    }
    captured.ops = ops;

    // Put the World back as it was before the edits (what's still being
    // built outside the tree stays), newest edit first.
    for edit in edits.iter().rev() {
        undo_edit(world, edit, unsent);
    }
    // New Instances made in an earlier batch and placed now are still
    // there (outside the tree); the CreateTree makes them again at the
    // same IDs.
    for id in &placed {
        if world.contains(*id) {
            let _ = world.destroy(*id);
        }
    }
    world.take_edits();
    captured
}

/// The record for `id`, made (and its place in `order` noted) the first
/// time.
fn touch<'m>(
    touched: &'m mut FxHashMap<InstanceId, Touched>,
    order: &mut Vec<InstanceId>,
    id: InstanceId,
) -> &'m mut Touched {
    touched.entry(id).or_insert_with(|| {
        order.push(id);
        Touched::default()
    })
}

/// The top of `id`'s branch: `game` if it's in the tree.
fn top_of(world: &World, id: InstanceId) -> InstanceId {
    let mut at = id;
    while let Some(parent) = world.get(at).and_then(|instance| instance.parent()) {
        at = parent;
    }
    at
}

/// How many ancestors `id` has.
fn depth(world: &World, id: InstanceId) -> usize {
    let mut count = 0;
    let mut at = id;
    while let Some(parent) = world.get(at).and_then(|instance| instance.parent()) {
        count += 1;
        at = parent;
    }
    count
}

/// Takes one edit back, from the value it kept.
fn undo_edit(world: &mut World, edit: &Edit, unsent: &FxHashSet<InstanceId>) {
    let keep = |id: &InstanceId| !id.is_local() && !unsent.contains(id);
    match edit {
        Edit::Created(id) if keep(id) => {
            let _ = world.destroy(*id);
        }
        Edit::Name { id, before } if keep(id) => {
            let _ = world.set_name(*id, before.clone());
        }
        Edit::Property { id, name, before } if keep(id) => {
            let _ = world.set_known_property(*id, name, before.clone());
        }
        Edit::Parent { id, before } if keep(id) => {
            let place = before.map(|(parent, index)| Place { parent, index });
            let _ = put(world, *id, place);
        }
        Edit::Attribute { id, name, before } if keep(id) => {
            let _ = world.set_attribute(*id, name, before.clone());
        }
        Edit::Tags { id, before } if keep(id) => set_tags(world, *id, before),
        _ => {}
    }
}

/// `id` as a node of a CreateTree: where it is now, its name, every
/// property (in name order, which sets Material before Metalness and
/// Roughness), attributes and tags.
fn node(world: &World, id: InstanceId) -> Option<NodeSpec> {
    let instance = world.get(id)?;
    let place = place_of(world, id)?;
    Some(NodeSpec {
        id,
        class: instance.class(),
        parent: place.parent,
        index: Some(place.index),
        name: instance.name().to_owned(),
        properties: instance
            .properties()
            .map(|(name, value)| (name.to_owned(), value.clone()))
            .collect(),
        attributes: instance
            .attributes()
            .iter()
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect(),
        tags: instance.tags().to_vec(),
    })
}
