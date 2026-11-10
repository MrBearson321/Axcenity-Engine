//! Which Instances in the game have each class and tag, so "every Sound" or
//! `GetTagged` doesn't walk the whole tree.
//!
//! The index lists what's in the game: everything below `game`, except
//! `game` itself. An Instance with no parent, or inside one with no parent,
//! isn't listed until it's put in the game, the same as a walk from `game`
//! wouldn't find it. [`World`](crate::World) keeps it up to date as the tree
//! and tags change, and [`World::check_index`](crate::World::check_index)
//! compares it with a walk.
//!
//! Services are found with [`World::service`](crate::World::service) and
//! aren't listed by class, so a new [`World`](crate::World) costs nothing
//! extra to make. A tag put on a service is listed.

use std::collections::{BTreeMap, BTreeSet};

use crate::class::ClassName;
use crate::world::InstanceId;

/// What a lookup of a class or tag nothing has gets.
static NONE: BTreeSet<InstanceId> = BTreeSet::new();

/// `BTreeSet`s, so going through a class or tag always takes the same order
/// (by ID) on every computer and every run.
#[derive(Clone, Default)]
pub(crate) struct Index {
    /// Indexed by `ClassName as usize`. Empty until something is listed.
    by_class: Vec<BTreeSet<InstanceId>>,
    /// A tag nothing has any more is taken out. Tags can come from players
    /// or files, so this isn't a fast hash map (see `data-model.md`).
    by_tag: BTreeMap<Box<str>, BTreeSet<InstanceId>>,
}

impl Index {
    /// Lists `id` under `class`.
    pub(crate) fn add_class(&mut self, class: ClassName, id: InstanceId) {
        let slot = class as usize;
        if slot >= self.by_class.len() {
            self.by_class
                .resize_with(ClassName::ALL.len(), BTreeSet::new);
        }
        self.by_class[slot].insert(id);
    }

    pub(crate) fn remove_class(&mut self, class: ClassName, id: InstanceId) {
        if let Some(listed) = self.by_class.get_mut(class as usize) {
            listed.remove(&id);
        }
    }

    pub(crate) fn add_tag(&mut self, tag: &str, id: InstanceId) {
        match self.by_tag.get_mut(tag) {
            Some(listed) => {
                listed.insert(id);
            }
            None => {
                self.by_tag.insert(tag.into(), BTreeSet::from([id]));
            }
        }
    }

    pub(crate) fn remove_tag(&mut self, tag: &str, id: InstanceId) {
        if let Some(listed) = self.by_tag.get_mut(tag) {
            listed.remove(&id);
            if listed.is_empty() {
                self.by_tag.remove(tag);
            }
        }
    }

    #[inline]
    pub(crate) fn of_class(&self, class: ClassName) -> &BTreeSet<InstanceId> {
        self.by_class.get(class as usize).unwrap_or(&NONE)
    }

    #[inline]
    pub(crate) fn with_tag(&self, tag: &str) -> &BTreeSet<InstanceId> {
        self.by_tag.get(tag).unwrap_or(&NONE)
    }

    /// Every tag listed, sorted, with what has it.
    pub(crate) fn tags(&self) -> impl Iterator<Item = (&str, &BTreeSet<InstanceId>)> {
        self.by_tag.iter().map(|(tag, ids)| (&**tag, ids))
    }
}
