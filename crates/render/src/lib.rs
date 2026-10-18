//! Axcenity renderer.
//!
//! Callers describe a frame as a [`DrawList`] and hand it to
//! [`Renderer::render`]. This crate never sees Instances, scripts or windows:
//! anything that can provide a window handle (such as a `winit` window) can be
//! drawn into.

mod draw_list;
mod mesh;
mod renderer;

pub use draw_list::{AmbientLight, Camera, DrawItem, DrawList, Material, Shape, Sun};
pub use renderer::Renderer;

/// Re-exported so callers build transforms with the same `glam` version.
pub use glam;
