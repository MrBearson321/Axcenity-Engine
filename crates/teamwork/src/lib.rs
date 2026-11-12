//! Teamwork: the project's shared board of tasks and who they're assigned
//! to (`docs/research/teamwork.md`).
//!
//! Everything here is plain files inside the project (`team/`), so nothing
//! new travels between computers: a Collaborate session's mirror shares a
//! changed file like any other, and the project's repository keeps its
//! history. People are their Collaborate names, the same names
//! checkpoints list as participants.

pub mod board;
pub mod item;
pub mod time;

#[cfg(test)]
mod tests;

pub use board::{Board, Copies, Filter, Loaded, Whose};
pub use item::{
    BugInfo, Change, Check, Comment, Event, Item, Kind, Link, Priority, Severity, Status,
};
pub use time::Date;

use std::hash::{BuildHasher, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};

/// The project's teamwork files.
pub const FOLDER: &str = "team";
/// One file per item.
pub const BOARD_FOLDER: &str = "team/board";

/// The letters an ID is made of (no `i`, `l`, `o` or `u`, which read as
/// others).
const ID_LETTERS: &[u8; 32] = b"0123456789abcdefghjkmnpqrstvwxyz";
/// An ID's length: 10 letters of 5 bits.
const ID_LEN: usize = 10;

/// A new ID, unlikely ever to be made twice on any computer: the time and
/// a random number, so IDs made on two computers between syncs don't clash.
pub fn new_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    // `RandomState` is seeded randomly per process; hashing the time and a
    // counter with it gives a different number every call.
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    hasher.write_u128(nanos);
    hasher.write_u64(COUNTER.fetch_add(1, Ordering::Relaxed));
    hasher.write_u32(std::process::id());
    let mut bits = hasher.finish();
    (0..ID_LEN)
        .map(|_| {
            let letter = ID_LETTERS[(bits & 31) as usize];
            bits >>= 5;
            char::from(letter)
        })
        .collect()
}

/// True for something [`new_id`] could have made (the board reads only
/// such file names).
pub fn is_id(text: &str) -> bool {
    text.len() == ID_LEN && text.bytes().all(|b| ID_LETTERS.contains(&b))
}
