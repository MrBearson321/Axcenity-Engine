//! ID blocks: how several writers make Instances at once without clashing.
//!
//! The host hands each writer blocks of arena indices that no Instance has
//! had yet ([`World::next_unused_index`](axcenity_core::World::next_unused_index)
//! and up), in order, so the arena stays dense. A writer makes Instances
//! only at IDs from its own blocks (generation 0), so every copy can
//! `create_at` them and an ID never has to be rewritten.

use axcenity_core::InstanceId;

/// How many IDs a block holds.
pub const BLOCK_SIZE: u32 = 256;

/// Arena indices `start..start + len`, each used at generation 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

    fn id(&self, offset: u32) -> InstanceId {
        InstanceId::from_bits(u64::from(self.start + offset))
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

    pub fn block(&mut self) -> IdBlock {
        let block = IdBlock {
            start: self.next,
            len: BLOCK_SIZE,
        };
        self.next = self
            .next
            .checked_add(BLOCK_SIZE)
            .filter(|&next| next < 1 << 31)
            .expect("Team Create ran out of IDs (over 2 billion)");
        block
    }
}

/// A writer's side: the IDs it has left.
#[derive(Clone, Debug, Default)]
pub struct IdPool {
    blocks: Vec<IdBlock>,
    /// How many of the first block are used.
    used: u32,
}

impl IdPool {
    pub fn add(&mut self, block: IdBlock) {
        self.blocks.push(block);
    }

    /// The next free ID, or `None` until the host sends another block.
    pub fn take(&mut self) -> Option<InstanceId> {
        loop {
            let block = *self.blocks.first()?;
            if self.used < block.len {
                self.used += 1;
                return Some(block.id(self.used - 1));
            }
            self.blocks.remove(0);
            self.used = 0;
        }
    }

    /// How many IDs are left.
    pub fn left(&self) -> u32 {
        self.blocks.iter().map(|block| block.len).sum::<u32>() - self.used
    }
}
