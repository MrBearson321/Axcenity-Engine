//! The bridge between the Instance tree and the renderer, shared by the
//! player and the editor viewport.
//!
//! - [`build_draw_list`] turns the Workspace into a `render::DrawList`.
//! - [`build_draw_list_with_ui`] also lays out on-screen UI for a viewport.
//! - [`FlyCamera`] is the camera both of them look through.
//! - [`Particles`] moves ParticleEmitters' particles and draws them as sprites.
//! - [`TerrainCache`] keeps Terrains' chunk meshes and weights between frames.
//! - [`sun_direction`] is where the sun is at a time of day and latitude.
//! - [`asset_files`] lists the models, textures and material files drawing a
//!   world reads.
//! - [`part_material_file`] makes a material file from a Part's look.
//! - [`CameraView`] and [`pick`] find the part under the mouse in the editor.
//! - [`rotation_from_orientation`] and back turn a Part's `Orientation` into
//!   a rotation, for drawing and for the editor's rotate tool.

mod camera;
mod clouds;
mod color_grading;
mod constraints;
mod decals;
mod draw_list;
mod fog;
mod foliage;
mod graphics_volumes;
mod lens;
mod light_probes;
mod material;
mod mesh_cache;
mod orientation;
mod outlines;
mod particles;
mod pick;
mod preload;
#[cfg(test)]
mod reload_tests;
mod streaming;
mod surfaces;
mod terrain;
mod time_of_day;
mod weather;

pub use camera::{FlyCamera, MoveKeys};
pub use draw_list::{
    LightMarker, MarkerKind, Mount, Poses, UiFrame, attach_ui_textures, build_draw_list,
    build_draw_list_posed, build_draw_list_with_ui, build_draw_list_with_ui_interaction,
    light_markers, mount_of,
};
pub use foliage::brush_color as foliage_brush_color;
pub use light_probes::{LightProbePoints, light_bake_fingerprint, light_probe_points};
pub use material::part_material_file;
pub use orientation::{orientation_from_rotation, rotation_from_orientation};
pub use particles::{
    GPU_PARTICLE_THRESHOLD, MAX_PER_EMITTER, Particles, built_in as built_in_particle,
};
pub use pick::{
    CameraView, Hit, Occluders, Ray, part_box_edges, part_half_extent, part_hit, pick, pick_hit,
    pick_hit_skipping, visible_box_edges,
};
pub use preload::{AssetFile, asset_files};
pub use streaming::Streaming;
pub use terrain::{DEFAULT_DETAIL_DISTANCE, TerrainCache, terrain_hit};
pub use time_of_day::sun_direction;
