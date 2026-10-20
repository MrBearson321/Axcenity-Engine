//! Axcenity core: the Instance tree that every other crate works with.
//!
//! - [`Arena`] stores values and hands out generational [`Id`]s.
//! - [`World`] is the tree of [`Instance`]s, rooted at `game`.
//! - [`Value`] is a typed property value, such as a [`Vector3`] or [`Color3`].

mod animation;
mod arena;
mod class;
mod enums;
mod value;
mod world;

pub use animation::{AnimatorState, Parameter, PlayRequest, PropertyAnimationState};
pub use arena::{Arena, Id};
pub use class::{ClassName, INSTANCE_EVENTS};
pub use enums::{KeyCode, Material, MaterialPreset, PartType, ParticleType, TextureChannel};
pub use value::{Color3, UDim, UDim2, Value, ValueKind, Vector3};
pub use world::{Change, Instance, InstanceId, PropertyError, RecordedChange, TreeError, World};
