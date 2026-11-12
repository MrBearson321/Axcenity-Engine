//! ID blocks: how several writers make Instances at once without clashing.
//!
//! The host hands each writer blocks of arena indices that no Instance has
//! had yet ([`World::next_unused_index`](axcenity_core::World::next_unused_index)
//! and up), in order, so the arena stays dense. A writer makes Instances
//! only at IDs from its own blocks (generation 0), so every copy can
//! `create_at` them and an ID never has to be rewritten.

use axcenity_core::InstanceId;

/// How many IDs a block holds.
pub const BLOCK_SIZE: u32 = 1024;

/// Arena indices `start..start + len`, each used at generation 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct IdBlock {
    pub start: u32,
    pub len: u32,
}

impl IdBlock {
    pub fn contains(&self, id: InstanceId) -> bool {
        let bits = id.to_bits();
        let (index, generation) = (bits as u32, (bits >> 32) as u32);
        generation == 0 && index >= self.start && index - self.start < self.len
    }
}

/// The host's side: the next index to hand out.
#[derive(Clone, Debug)]
pub struct IdAllocator {
    next: u32,
}

impl IdAllocator {
    /// Starts at `first`, the world's first never-used index.
    pub fn new(first: u32) -> Self {
        Self { next: first }
    }

    /// The first index the next block would start at.
    pub fn next_index(&self) -> u32 {
        self.next
    }

    /// Never hands out an index below `next` (a successor host starts after
    /// the old host's blocks).
    pub fn skip_to(&mut self, next: u32) {
        self.next = self.next.max(next);
    }

    /// The next block; `None` once the indices a World can hold (2^31)
    /// are used up, which takes two million blocks.
    pub fn block(&mut self) -> Option<IdBlock> {
        let next = self
            .next
            .checked_add(BLOCK_SIZE)
            .filter(|&n| n <= 1 << 31)?;
        let block = IdBlock {
            start: self.next,
            len: BLOCK_SIZE,
        };
        self.next = next;
        Some(block)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn running_out_of_ids_is_none_not_a_crash() {
        let mut ids = IdAllocator::new((1 << 31) - 2 * BLOCK_SIZE);
        assert!(ids.block().is_some());
        let last = ids.block().unwrap();
        assert_eq!(last.start + last.len, 1 << 31);
        assert_eq!(ids.block(), None);
        assert_eq!(ids.block(), None);
    }
}
