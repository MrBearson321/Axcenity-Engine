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

#[derive(Clone)]
struct Slot<T> {
    generation: u32,
    value: Option<T>,
}

/// Indexes from here up are a replica's own values (see
/// [`Arena::make_replica`]); the original never uses them.
const LOCAL_BASE: u32 = 1 << 31;

/// `Clone` copies every value and keeps every slot's generation, so each
/// `Id` points at the same value in the copy as in the original.
#[derive(Clone)]
pub struct Arena<T> {
    slots: Vec<Slot<T>>,
    /// Indexes of empty slots, reused before the `Vec` grows.
    free: Vec<u32>,
    /// A replica's own values, at `LOCAL_BASE` and up.
    local_slots: Vec<Slot<T>>,
    local_free: Vec<u32>,
    /// A replica takes its original's values at their own IDs
    /// ([`Arena::insert_at`]), and puts its own above `LOCAL_BASE`.
    replica: bool,
    /// Index ranges this writer may fill (Team Create ID blocks), as
    /// `(next, end)`: [`insert`](Self::insert) takes from these first in a
    /// replica.
    blocks: Vec<(u32, u32)>,
    len: usize,
}

impl Id {
    /// The ID as one number, the same on every computer (a network ID).
    pub fn to_bits(self) -> u64 {
        (u64::from(self.generation) << 32) | u64::from(self.index)
    }

    pub fn from_bits(bits: u64) -> Id {
        Id {
            index: bits as u32,
            generation: (bits >> 32) as u32,
        }
    }

    /// True for a replica's own value, which its original never had.
    pub fn is_local(self) -> bool {
        self.index >= LOCAL_BASE
    }
}

impl<T> Arena<T> {
    pub fn new() -> Self {
        Self {
            slots: Vec::new(),
            free: Vec::new(),
            local_slots: Vec::new(),
            local_free: Vec::new(),
            replica: false,
            blocks: Vec::new(),
            len: 0,
        }
    }

    /// From now on, [`insert`](Self::insert) gives IDs the original arena
    /// never gives, so this arena can also hold the original's values at
    /// their own IDs ([`insert_at`](Self::insert_at)).
    pub fn make_replica(&mut self) {
        self.replica = true;
    }

    /// The first index no value has had yet: every index from here up is
    /// free and has never been used, so an ID there with generation 0 can't
    /// match a stale one. Team Create hands out blocks of IDs from here.
    pub fn next_unused_index(&self) -> u32 {
        u32::try_from(self.slots.len()).expect("slots stay below LOCAL_BASE")
    }

    /// Lets this arena fill indices `start..start + len` itself (at
    /// generation 0), after any blocks it already has.
    pub fn add_block(&mut self, start: u32, len: u32) {
        let end = start.checked_add(len).filter(|&end| end <= LOCAL_BASE);
        if let Some(end) = end
            && len > 0
        {
            self.blocks.push((start, end));
        }
    }

    /// How many indices are left in the blocks.
    pub fn ids_left(&self) -> u32 {
        self.blocks.iter().map(|(next, end)| end - next).sum()
    }

    /// The unused part of each block, as `(start, len)`.
    pub fn blocks_left(&self) -> Vec<(u32, u32)> {
        self.blocks
            .iter()
            .filter(|(next, end)| next < end)
            .map(|&(next, end)| (next, end - next))
            .collect()
    }

    /// Takes the next index from the blocks, as an ID at generation 0,
    /// without storing anything there.
    pub fn take_block_id(&mut self) -> Option<Id> {
        while let Some((next, end)) = self.blocks.first_mut() {
            if *next < *end {
                let index = *next;
                *next += 1;
                return Some(Id {
                    index,
                    generation: 0,
                });
            }
            self.blocks.remove(0);
        }
        None
    }

    /// Stores `value` and returns its new ID.
    pub fn insert(&mut self, value: T) -> Id {
        if self.replica {
            // A block's indices are new everywhere, so their slots are
            // empty; one that isn't (a bug elsewhere) is skipped.
            while let Some(id) = self.take_block_id() {
                if self.slot(id.index).is_none_or(|slot| slot.value.is_none()) {
                    let stored = self.insert_at(id, value);
                    debug_assert!(stored, "checked the slot is free");
                    return id;
                }
            }
        }
        self.insert_unshared(value)
    }

    /// Stores `value` without taking an index from the blocks: in a
    /// replica, at an index only this copy uses.
    pub fn insert_unshared(&mut self, value: T) -> Id {
        self.len += 1;
        let (slots, free, base) = if self.replica {
            (&mut self.local_slots, &mut self.local_free, LOCAL_BASE)
        } else {
            (&mut self.slots, &mut self.free, 0)
        };
        if let Some(index) = free.pop() {
            let slot = &mut slots[(index - base) as usize];
            slot.value = Some(value);
            return Id {
                index,
                generation: slot.generation,
            };
        }

        let index = u32::try_from(slots.len())
            .ok()
            .filter(|&index| index < LOCAL_BASE)
            .expect("arena is full (over 2 billion values)")
            + base;
        slots.push(Slot {
            generation: 0,
            value: Some(value),
        });
        Id {
            index,
            generation: 0,
        }
    }

    /// Stores `value` at exactly `id`, as the original arena has it (a
    /// replica following its original). False if that slot holds a value,
    /// or `id` is a replica's own.
    pub fn insert_at(&mut self, id: Id, value: T) -> bool {
        if id.is_local() {
            return false;
        }
        let index = id.index as usize;
        if index >= self.slots.len() {
            self.slots.resize_with(index + 1, || Slot {
                generation: 0,
                value: None,
            });
        }
        let slot = &mut self.slots[index];
        if slot.value.is_some() {
            return false;
        }
        slot.generation = id.generation;
        slot.value = Some(value);
        self.free.retain(|&free| free != id.index);
        self.len += 1;
        true
    }

    fn slot(&self, index: u32) -> Option<&Slot<T>> {
        match index.checked_sub(LOCAL_BASE) {
            Some(local) => self.local_slots.get(local as usize),
            None => self.slots.get(index as usize),
        }
    }

    fn slot_mut(&mut self, index: u32) -> Option<&mut Slot<T>> {
        match index.checked_sub(LOCAL_BASE) {
            Some(local) => self.local_slots.get_mut(local as usize),
            None => self.slots.get_mut(index as usize),
        }
    }

    /// Removes and returns the value, or `None` if `id` is stale.
    pub fn remove(&mut self, id: Id) -> Option<T> {
        let slot = self.slot_mut(id.index)?;
        if slot.generation != id.generation {
            return None;
        }
        let value = slot.value.take()?;
        // Wrapping keeps this from panicking after 4 billion reuses of one slot.
        slot.generation = slot.generation.wrapping_add(1);
        if id.is_local() {
            self.local_free.push(id.index);
        } else if !self.replica {
            // A replica's original decides what goes in its slots.
            self.free.push(id.index);
        }
        self.len -= 1;
        Some(value)
    }

    pub fn get(&self, id: Id) -> Option<&T> {
        let slot = self.slot(id.index)?;
        if slot.generation != id.generation {
            return None;
        }
        slot.value.as_ref()
    }

    pub fn get_mut(&mut self, id: Id) -> Option<&mut T> {
        let slot = self.slot_mut(id.index)?;
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
    fn next_unused_index_is_past_every_slot_ever_used() {
        let mut arena = Arena::new();
        assert_eq!(arena.next_unused_index(), 0);
        let a = arena.insert("a");
        arena.insert("b");
        arena.remove(a);
        // A freed slot is still "used": its generation went up.
        assert_eq!(arena.next_unused_index(), 2);
        arena.make_replica();
        arena.insert("local");
        assert_eq!(
            arena.next_unused_index(),
            2,
            "a replica's own values are elsewhere"
        );
        assert!(arena.insert_at(Id::from_bits(5), "theirs"));
        assert_eq!(arena.next_unused_index(), 6);
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

    #[test]
    fn a_replica_keeps_its_originals_ids_and_puts_its_own_elsewhere() {
        let mut original = Arena::new();
        let a = original.insert("a");
        let gone = original.insert("gone");
        original.remove(gone);
        let b = original.insert("b");
        let mut replica = Arena::new();
        replica.make_replica();
        let own = replica.insert("own");
        assert!(replica.insert_at(b, "b"));
        assert!(replica.insert_at(a, "a"));
        assert!(!replica.insert_at(a, "again"), "the slot is taken");
        assert_eq!(replica.get(a), Some(&"a"));
        assert_eq!(replica.get(b), Some(&"b"));
        assert_eq!(replica.get(gone), None, "an older generation");
        assert!(own.is_local() && !a.is_local());
        assert_eq!(replica.get(own), Some(&"own"));
        assert_eq!(replica.len(), 3);
        // The original's next value may land where the replica's own can't.
        let c = original.insert("c");
        assert!(replica.insert_at(c, "c"));
        assert_eq!(replica.get(own), Some(&"own"));
        assert_eq!(Id::from_bits(c.to_bits()), c);
    }
}
