//! Axcenity core: the Instance tree that every other crate works with.
//!
//! - [`Arena`] stores values and hands out generational [`Id`]s.
//! - [`World`] is the tree of [`Instance`]s, rooted at `game`.
//! - [`TerrainData`] is a Terrain's heightfield, kept beside its Instance.
//! - [`StringTable`] is the project's UI text in each language, kept by the world.
//! - [`CollisionGroups`] is the project's collision groups, kept by the world.
//! - [`UiSettings`] is the player's game UI settings (text scale, contrast, motion), kept by the world.
//! - [`prefab`] links placed copies to their prefab files and works out their overrides.
//! - [`Value`] is a typed property value, such as a [`Vector3`] or [`Color3`].

pub mod ability;
mod animation;
mod arena;
pub mod bench;
mod class;
mod collision_groups;
pub mod csg;
pub mod dialogue;
mod enums;
mod index;
pub mod input;
pub mod prefab;
pub mod sound;
pub mod spline;
pub mod streaming;
mod strings;
mod terrain;
pub mod tool;
mod ui_settings;
mod value;
mod world;

pub use animation::{
    AnimationTrackState, AnimatorState, LayerWeight, Parameter, PlayRequest, PropertyAnimationState,
};
pub use arena::{Arena, Id};
pub use class::{ClassName, DEFAULT_CLOCK_TIME, DEFAULT_LATITUDE, INSTANCE_EVENTS};
pub use collision_groups::{CollisionGroups, DEFAULT_GROUP, MAX_COLLISION_GROUPS};
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
pub use ui_settings::{TEXT_SCALE_RANGE, UI_SETTING_NAMES, UI_SETTING_PREFIX, UiSettings};
pub use value::{
    CFrame, Color3, EnumItem, PhysicalProperties, UDim, UDim2, Value, ValueKind, Vector2, Vector3,
};
pub use world::{
    AttributeError, Change, Descendants, Instance, InstanceId, MASTER_VOLUME_SETTING,
    MAX_ATTRIBUTE_NAME, PropertyError, RecordedChange, STARTER_CHARACTER, TreeError, World,
    is_attribute_name,
};
