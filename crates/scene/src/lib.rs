//! The bridge between the Instance tree and the renderer, shared by the
//! player and the editor viewport.
//!
//! - [`build_draw_list`] turns the Workspace into a `render::DrawList`.
//! - [`FlyCamera`] is the camera both of them look through.

mod camera;
mod draw_list;

pub use camera::{FlyCamera, MoveKeys};
pub use draw_list::build_draw_list;
