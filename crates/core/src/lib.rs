//! Axcenity core: the Instance tree that every other crate works with.
//!
//! - [`Arena`] stores values and hands out generational [`Id`]s.
//! - [`World`] is the tree of [`Instance`]s, rooted at `game`.
//! - [`Value`] is a typed property value, such as a [`Vector3`] or [`Color3`].

mod arena;
mod class;
mod enums;
mod value;
mod world;

pub use arena::{Arena, Id};
pub use class::ClassName;
pub use enums::{Material, MaterialPreset, PartType};
pub use value::{Color3, Value, ValueKind, Vector3};
pub use world::{Instance, InstanceId, PropertyError, TreeError, World};
