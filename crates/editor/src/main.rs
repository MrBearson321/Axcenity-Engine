//! Axcenity Editor: open a project folder, look around it in a 3D viewport,
//! and see its Instance tree in the Explorer.
//!
//! Phase 9: select parts, edit their properties and save. Undo, transform
//! tools and Play come in later phases.
//! Built on eframe (egui's app framework), which owns the window and the GPU
//! device; the viewport draws with the same renderer as the player.

mod app;
mod explorer;
mod output;
mod properties;
mod viewport;

use std::path::PathBuf;

use eframe::egui;

use crate::app::EditorApp;

fn main() -> eframe::Result {
    // Same logging defaults as the player: warnings and errors, minus
    // harmless graphics driver chatter. Set RUST_LOG to override.
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("warn,wgpu_hal=error"),
    )
    .init();

    // An optional project folder to open, such as `examples/hello-touched`.
    // `args_os` instead of `args` so folder names that aren't valid Unicode still work.
    let folder = std::env::args_os().nth(1).map(PathBuf::from);

    let options = eframe::NativeOptions {
        // The viewport draws with wgpu, so egui must too (they share the GPU device).
        renderer: eframe::Renderer::Wgpu,
        viewport: egui::ViewportBuilder::default()
            .with_title("Axcenity Editor")
            .with_inner_size([1280.0, 800.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Axcenity Editor",
        options,
        // `move` hands `folder` over to the closure, which eframe calls once
        // the window exists to create the app.
        Box::new(move |_creation_context| Ok(Box::new(EditorApp::new(folder.as_deref())))),
    )
}
