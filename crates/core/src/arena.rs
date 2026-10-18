//! A generational arena: a `Vec` of slots that can be reused after removal.
//!
//! Each [`Id`] records which slot it points to *and* which "generation" of
//! that slot it was created for. When a value is removed, the slot's
//! generation goes up, so old IDs for that slot stop working instead of
//! silently pointing at whatever is stored there next.

/// A handle to a value in an [`Arena`]. Cheap to copy and compare.
///
/// An `Id` can outlive its value; lookups then return `None`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Id {
    index: u32,
    generation: u32,
}

struct Slot<T> {
    generation: u32,
    value: Option<T>,
}

pub struct Arena<T> {
    slots: Vec<Slot<T>>,
    /// Indexes of empty slots, reused before the `Vec` grows.
    free: Vec<u32>,
    len: usize,
}

impl<T> Arena<T> {
    pub fn new() -> Self {
        Self {
            slots: Vec::new(),
            free: Vec::new(),
            len: 0,
        }
    }

    /// Stores `value` and returns its new ID.
    pub fn insert(&mut self, value: T) -> Id {
        self.len += 1;
        if let Some(index) = self.free.pop() {
            let slot = &mut self.slots[index as usize];
            slot.value = Some(value);
            return Id {
                index,
                generation: slot.generation,
            };
        }

        let index = u32::try_from(self.slots.len()).expect("arena is full (over 4 billion values)");
        self.slots.push(Slot {
            generation: 0,
            value: Some(value),
        });
        Id {
            index,
            generation: 0,
        }
    }

    /// Removes and returns the value, or `None` if `id` is stale.
    pub fn remove(&mut self, id: Id) -> Option<T> {
        let slot = self.slots.get_mut(id.index as usize)?;
        if slot.generation != id.generation {
            return None;
        }
        let value = slot.value.take()?;
        // Wrapping keeps this from panicking after 4 billion reuses of one slot.
        slot.generation = slot.generation.wrapping_add(1);
        self.free.push(id.index);
        self.len -= 1;
        Some(value)
    }

    pub fn get(&self, id: Id) -> Option<&T> {
        let slot = self.slots.get(id.index as usize)?;
        if slot.generation != id.generation {
            return None;
        }
        slot.value.as_ref()
    }

    pub fn get_mut(&mut self, id: Id) -> Option<&mut T> {
        let slot = self.slots.get_mut(id.index as usize)?;
        if slot.generation != id.generation {
            return None;
        }
        slot.value.as_mut()
    }

    pub fn contains(&self, id: Id) -> bool {
        self.get(id).is_some()
    }

    /// The number of values currently stored.
    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

// Clippy asks for `Default` whenever there's a `new()` with no arguments.
impl<T> Default for Arena<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_then_get() {
        let mut arena = Arena::new();
        let a = arena.insert("a");
        let b = arena.insert("b");
        assert_eq!(arena.get(a), Some(&"a"));
        assert_eq!(arena.get(b), Some(&"b"));
        assert_eq!(arena.len(), 2);
    }

    #[test]
    fn get_mut_changes_the_value() {
        let mut arena = Arena::new();
        let id = arena.insert(1);
        *arena.get_mut(id).unwrap() += 1;
        assert_eq!(arena.get(id), Some(&2));
    }

    #[test]
    fn removed_id_is_stale() {
        let mut arena = Arena::new();
        let id = arena.insert("a");
        assert_eq!(arena.remove(id), Some("a"));
        assert_eq!(arena.get(id), None);
        assert_eq!(arena.remove(id), None);
        assert!(!arena.contains(id));
        assert!(arena.is_empty());
    }

    #[test]
    fn reused_slot_does_not_revive_old_id() {
        let mut arena = Arena::new();
        let old = arena.insert("old");
        arena.remove(old);
        let new = arena.insert("new");

        // Same slot, different generation.
        assert_eq!(old.index, new.index);
        assert_ne!(old, new);
        assert_eq!(arena.get(old), None);
        assert_eq!(arena.get(new), Some(&"new"));
    }
}
