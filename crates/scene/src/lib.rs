//! The bridge between the Instance tree and the renderer, shared by the
//! player and the editor viewport.
//!
//! - [`build_draw_list`] turns the Workspace into a `render::DrawList`.
//! - [`FlyCamera`] is the camera both of them look through.
//! - [`CameraView`] and [`pick`] find the part under the mouse in the editor.
//! - [`rotation_from_orientation`] and back turn a Part's `Orientation` into
//!   a rotation, for drawing and for the editor's rotate tool.

mod camera;
mod draw_list;
mod orientation;
mod pick;

pub use camera::{FlyCamera, MoveKeys};
pub use draw_list::build_draw_list;
pub use orientation::{orientation_from_rotation, rotation_from_orientation};
pub use pick::{CameraView, Hit, Ray, part_box_edges, part_half_extent, pick, pick_hit};
