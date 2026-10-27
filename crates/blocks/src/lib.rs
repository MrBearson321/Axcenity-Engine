//! Scratch-style block scripts for the editor. The data and its translation
//! to Lua are in `axcenity-blocks-runtime` (re-exported here), which games
//! use too. This crate adds [`from_lua`], which goes the other way, for
//! switching a script between Code and Blocks (or Graph). The player never
//! depends on it.

pub use axcenity_blocks_runtime::*;

mod from_lua;
mod lua;

pub use from_lua::{FromLuaError, from_lua};
pub use lua::SyntaxError;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod tween_tests;
