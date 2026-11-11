//! Which part each collider belongs to, to name the parts in a contact,
//! and whether that part can touch (`CanTouch`).
//!
//! Looked up for every contact after every step, so it's a table indexed
//! by the collider handle's slot rather than a map: rapier hands out
//! handles from an arena, so the slots stay dense, and a handle's
//! generation tells a removed collider from the one that reused its slot.
//! `CanTouch` is kept here too, set whenever a collider gets its owner,
//! which is whenever its part's `seen` state changes (`sync`), so a
//! contact doesn't need its parts looked up.

use axcenity_core::InstanceId;
use rapier3d::prelude::ColliderHandle;

#[derive(Default)]
pub(crate) struct Owners {
    /// By the handle's index.
    slots: Vec<Option<Owner>>,
}

#[derive(Clone, Copy)]
struct Owner {
    generation: u32,
    part: InstanceId,
    can_touch: bool,
}

impl Owners {
    pub(crate) fn get(&self, handle: &ColliderHandle) -> Option<&InstanceId> {
        self.owner(handle).map(|owner| &owner.part)
    }

    /// The part and whether it can touch.
    pub(crate) fn touchable(&self, handle: &ColliderHandle) -> Option<(InstanceId, bool)> {
        self.owner(handle)
            .map(|owner| (owner.part, owner.can_touch))
    }

    fn owner(&self, handle: &ColliderHandle) -> Option<&Owner> {
        let (index, generation) = handle.into_raw_parts();
        self.slots
            .get(index as usize)?
            .as_ref()
            .filter(|owner| owner.generation == generation)
    }

    pub(crate) fn insert(&mut self, handle: ColliderHandle, part: InstanceId, can_touch: bool) {
        let (index, generation) = handle.into_raw_parts();
        let index = index as usize;
        if index >= self.slots.len() {
            self.slots.resize(index + 1, None);
        }
        self.slots[index] = Some(Owner {
            generation,
            part,
            can_touch,
        });
    }

    pub(crate) fn remove(&mut self, handle: &ColliderHandle) {
        let (index, generation) = handle.into_raw_parts();
        if let Some(slot) = self.slots.get_mut(index as usize)
            && slot.is_some_and(|owner| owner.generation == generation)
        {
            *slot = None;
        }
    }
}
