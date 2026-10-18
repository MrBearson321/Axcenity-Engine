//! The bridge between the Instance tree and the renderer, shared by the
//! player and the editor viewport.
//!
//! - [`build_draw_list`] turns the Workspace into a `render::DrawList`.
//! - [`FlyCamera`] is the camera both of them look through.
//! - [`CameraView`] and [`pick`] find the part under the mouse in the editor.

mod camera;
mod draw_list;
mod pick;

pub use camera::{FlyCamera, MoveKeys};
pub use draw_list::build_draw_list;
pub use pick::{CameraView, Ray, part_box_edges, pick};
