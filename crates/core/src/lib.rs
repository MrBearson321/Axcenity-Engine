//! Axcenity core: the Instance tree that every other crate works with.
//!
//! - [`Arena`] stores values and hands out generational [`Id`]s.
//! - [`World`] is the tree of [`Instance`]s, rooted at `game`.
//! - [`TerrainData`] is a Terrain's heightfield, kept beside its Instance.
//! - [`Value`] is a typed property value, such as a [`Vector3`] or [`Color3`].

mod animation;
mod arena;
mod class;
mod enums;
mod terrain;
mod value;
mod world;

pub use animation::{
    AnimationTrackState, AnimatorState, Parameter, PlayRequest, PropertyAnimationState,
};
pub use arena::{Arena, Id};
pub use class::{ClassName, INSTANCE_EVENTS};
pub use enums::{
    KeyCode, Material, MaterialPreset, PartType, ParticleType, TextureChannel, ToneMapping,
};
pub use terrain::{
    CHUNK_CELLS, DEFAULT_TERRAIN_HEIGHT, DEFAULT_TERRAIN_SAMPLES, FoliageInstance, FoliagePlant,
    FoliageProp, FoliageSource, MAX_FOLIAGE_INSTANCES, MAX_TERRAIN_LAYERS, MAX_TERRAIN_SAMPLES,
    MIN_TERRAIN_SAMPLES, PaintWeights, SampleRect, TerrainData, TerrainError, TerrainRegion,
    valid_terrain_size,
};
pub use value::{Color3, UDim, UDim2, Value, ValueKind, Vector2, Vector3};
pub use world::{Change, Instance, InstanceId, PropertyError, RecordedChange, TreeError, World};
