//! Compiled scripts, kept by their source (roadmap M6). Cloning a model
//! starts its scripts again, both sides (server and client) load the same
//! ModuleScripts, and each Play run loads every script anew; all of these
//! compile the same source over and over. Here each source is compiled to
//! Luau bytecode once and the bytecode is loaded after that, which skips
//! parsing and compiling.
//!
//! Scripts still can't load bytecode of their own: only bytecode this
//! module compiled from source text is ever loaded as binary, and a chunk
//! that doesn't compile goes through mlua's text path, so its syntax error
//! reads exactly as before.
//!
//! The cache is per thread (each host runs its scripts on one thread) and
//! holds at most [`MAX_BYTES`] of source and bytecode, dropping the oldest
//! entries first.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use mlua::chunk::{ChunkMode, Compiler};
use mlua::{Function, Lua, Table};
use rustc_hash::{FxHashMap, FxHasher};

/// How much source and bytecode the cache keeps, together.
const MAX_BYTES: usize = 32 * 1024 * 1024;

/// What a chunk was compiled from: its source and the debug level, since
/// the debugger compiles with local names (level 2) and that changes the
/// bytecode.
#[derive(PartialEq, Eq, Hash, Clone)]
struct Key {
    source_hash: u64,
    debug_level: u8,
}

struct Entry {
    /// Kept to rule out a hash collision: two sources with the same hash
    /// never share bytecode.
    source: Rc<str>,
    bytecode: Rc<[u8]>,
}

#[derive(Default)]
struct Cache {
    entries: FxHashMap<Key, Entry>,
    /// Oldest first, for dropping when over [`MAX_BYTES`].
    order: VecDeque<Key>,
    bytes: usize,
    hits: u64,
    misses: u64,
}

thread_local! {
    static CACHE: RefCell<Cache> = RefCell::new(Cache::default());
}

/// The debug level scripts on `lua` compile with: 2 with the debugger
/// installed (see `debugger::install`), else Luau's default, 1.
fn debug_level(lua: &Lua) -> u8 {
    if crate::debugger::is_installed(lua) {
        2
    } else {
        1
    }
}

fn source_hash(source: &str) -> u64 {
    let mut hasher = FxHasher::default();
    source.hash(&mut hasher);
    hasher.finish()
}

/// `source`'s bytecode, compiled now or earlier. `None` if it doesn't
/// compile, so the caller can load the text and get mlua's usual error.
fn bytecode(source: &str, debug_level: u8) -> Option<Rc<[u8]>> {
    let key = Key {
        source_hash: source_hash(source),
        debug_level,
    };
    let found = CACHE.with_borrow_mut(|cache| {
        let found = cache
            .entries
            .get(&key)
            .filter(|entry| &*entry.source == source)
            .map(|entry| Rc::clone(&entry.bytecode));
        if found.is_some() {
            cache.hits += 1;
        } else {
            cache.misses += 1;
        }
        found
    });
    if found.is_some() {
        return found;
    }
    let bytecode: Rc<[u8]> = Compiler::new()
        .set_debug_level(debug_level)
        .compile(source)
        .ok()?
        .into();
    CACHE.with_borrow_mut(|cache| {
        let size = source.len() + bytecode.len();
        if size > MAX_BYTES {
            return;
        }
        while cache.bytes + size > MAX_BYTES {
            let Some(oldest) = cache.order.pop_front() else {
                break;
            };
            if let Some(entry) = cache.entries.remove(&oldest) {
                cache.bytes -= entry.source.len() + entry.bytecode.len();
            }
        }
        let entry = Entry {
            source: source.into(),
            bytecode: Rc::clone(&bytecode),
        };
        // A collision (same hash, other source) replaces the older entry.
        if let Some(old) = cache.entries.insert(key.clone(), entry) {
            cache.bytes -= old.source.len() + old.bytecode.len();
            cache.order.retain(|other| *other != key);
        }
        cache.order.push_back(key);
        cache.bytes += size;
    });
    Some(bytecode)
}

/// Compiles `source` (the file `path`) into a function that runs in `env`,
/// reusing the bytecode of an earlier load of the same source. Its errors
/// read `path:line: message`.
pub fn load(lua: &Lua, path: &str, source: &str, env: Table) -> mlua::Result<Function> {
    // The `@` tells Lua this is a file path, so messages read `path:line:`.
    let name = format!("@{path}");
    match bytecode(source, debug_level(lua)) {
        Some(bytecode) => lua
            .load(&bytecode[..])
            .set_name(name)
            // Compiled above from source text, never a script's own bytecode.
            .set_mode(ChunkMode::Binary)
            .set_environment(env)
            .into_function(),
        None => lua
            .load(source)
            .set_name(name)
            // Precompiled bytecode can crash the interpreter, so only accept
            // source text.
            .set_mode(ChunkMode::Text)
            .set_environment(env)
            .into_function(),
    }
}

/// How many loads on this thread found their bytecode cached, and how many
/// compiled it.
#[cfg(test)]
pub(crate) fn hits_and_misses() -> (u64, u64) {
    CACHE.with_borrow(|cache| (cache.hits, cache.misses))
}
