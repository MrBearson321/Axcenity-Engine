//! Axcenity core: the Instance tree that every other crate works with.
//!
//! - [`Arena`] stores values and hands out generational [`Id`]s.
//! - [`World`] is the tree of [`Instance`]s, rooted at `game`.
//! - [`TerrainData`] is a Terrain's heightfield, kept beside its Instance.
//! - [`StringTable`] is the project's UI text in each language, kept by the world.
//! - [`Value`] is a typed property value, such as a [`Vector3`] or [`Color3`].

mod animation;
mod arena;
mod class;
mod enums;
pub mod input;
mod strings;
mod terrain;
mod value;
mod world;

pub use animation::{
    AnimationTrackState, AnimatorState, Parameter, PlayRequest, PropertyAnimationState,
};
pub use arena::{Arena, Id};
pub use class::{ClassName, INSTANCE_EVENTS};
pub use enums::{
    KeyCode, Material, MaterialPreset, MouseBehavior, PartType, ParticleType, TextureChannel,
    ToneMapping, UserInputState, UserInputType,
};
pub use strings::StringTable;
pub use terrain::{
    CHUNK_CELLS, DEFAULT_TERRAIN_HEIGHT, DEFAULT_TERRAIN_SAMPLES, FoliageInstance, FoliagePlant,
    FoliageProp, FoliageSource, MAX_FOLIAGE_INSTANCES, MAX_TERRAIN_LAYERS, MAX_TERRAIN_SAMPLES,
    MIN_TERRAIN_SAMPLES, PaintWeights, SampleRect, TerrainData, TerrainError, TerrainRegion,
    valid_terrain_size,
};
pub use value::{CFrame, Color3, EnumItem, UDim, UDim2, Value, ValueKind, Vector2, Vector3};
pub use world::{
    AttributeError, Change, Instance, InstanceId, MAX_ATTRIBUTE_NAME, PropertyError,
    RecordedChange, TreeError, World, is_attribute_name,
};
