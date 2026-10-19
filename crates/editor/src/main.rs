//! Axcenity Editor: open a project folder, look around it in a 3D viewport,
//! and see its Instance tree in the Explorer.
//!
//! Phase 12: insert, select, move, edit, rename, delete, duplicate and
//! regroup parts, with undo, and save. Play runs the game in the viewport,
//! and Stop puts the world back as it was.
//! Built on eframe (egui's app framework), which owns the window and the GPU
//! device; the viewport draws with the same renderer as the player.

mod app;
mod asset_browser;
mod autocomplete;
mod block_editor;
mod css_syntax;
mod dialog;
mod explorer;
mod gizmo;
mod history;
mod import_dialog;
mod lua_syntax;
mod output;
mod placing;
mod play;
mod properties;
mod props;
mod ribbon;
mod screen;
mod scripts;
mod side_warnings;
#[cfg(test)]
mod snapshot;
mod start;
mod style_panel;
mod surface;
mod theme;
mod thumbnail;
mod tree;
mod ui_handles;
mod viewport;
mod watcher;

use std::path::PathBuf;

use eframe::egui;

use crate::app::EditorApp;

/// The logo at 256×256, from `assets/branding` (see its README), built into
/// the program. Windows and X11 show it; macOS and Wayland ignore window icons.
const ICON_PNG: &[u8] = include_bytes!("../../../assets/branding/icon-256.png");

/// The logo as a window icon. If it can't be decoded, eframe's default icon
/// is used and the reason goes to the log.
fn window_icon() -> egui::IconData {
    eframe::icon_data::from_png_bytes(ICON_PNG).unwrap_or_else(|err| {
        log::warn!("no window icon: {err}");
        egui::IconData::default()
    })
}

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
            .with_inner_size([1280.0, 800.0])
            .with_icon(window_icon()),
        ..Default::default()
    };
    eframe::run_native(
        "Axcenity Editor",
        options,
        // `move` hands `folder` over to the closure, which eframe calls once
        // the window exists to create the app.
        Box::new(move |creation_context| {
            let ctx = &creation_context.egui_ctx;
            theme::install(ctx);
            let mut app = EditorApp::new(folder.as_deref());
            app.load_settings(ctx, creation_context.storage);
            Ok(Box::new(app))
        }),
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_logo_decodes_into_a_window_icon() {
        let icon = super::window_icon();
        assert_eq!((icon.width, icon.height), (256, 256));
        assert_eq!(icon.rgba.len(), 256 * 256 * 4);
    }
}
