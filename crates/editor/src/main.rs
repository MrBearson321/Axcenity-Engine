//! Axcenity Editor: open a project folder, look around it in a 3D viewport,
//! and see its Instance tree in the Explorer.
//!
//! Phase 12: insert, select, move, edit, rename, delete, duplicate and
//! regroup parts, with undo, and save. Play runs the game in the viewport,
//! and Stop puts the world back as it was.
//! Built on eframe (egui's app framework), which owns the window and the GPU
//! device; the viewport draws with the same renderer as the player.

mod ambient_cg;
mod angular;
mod anim_import;
mod animation;
mod app;
mod asset_browser;
mod asset_check;
mod autocomplete;
mod behavior_tree;
mod block_editor;
mod build_tools;
mod command_palette;
mod console;
mod constraints;
mod css_syntax;
mod data_table;
mod dialog;
mod explorer;
mod find_in_scripts;
mod game_tests;
mod gizmo;
mod global_assets;
mod history;
mod hot_patch;
mod import_dialog;
mod layout;
mod levels;
mod loading;
mod lua_syntax;
mod material_editor;
mod material_graph;
mod mixer;
mod model_studio;
mod node_graph;
mod output;
mod placing;
mod play;
mod plugins;
mod prefab_menu;
mod profiler;
mod properties;
mod props;
mod rebuild;
mod relaunch;
mod responsive;
mod ribbon;
mod ribbon_tabs;
mod screen;
mod scripts;
mod sequencer;
mod side_warnings;
mod snapping;
#[cfg(test)]
mod snapshot;
mod start;
mod style_panel;
mod surface;
mod tags;
mod terrain_studio;
mod theme;
mod thumbnail;
mod toast;
mod tree;
mod ui_designer;
mod ui_handles;
mod viewport;
mod watcher;
mod widgets;
mod window_frame;

use std::path::PathBuf;

use eframe::egui;

use crate::app::EditorApp;

/// The editor's name to eframe, which keeps its settings in a folder of
/// this name (`eframe::storage_dir`); the global assets are there too.
pub const APP_ID: &str = "Axcenity Editor";

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

/// eframe's GPU setup, plus timestamps where the GPU can take them, so the
/// Profiler can show the viewport's GPU time. The player never asks for them.
fn wgpu_options() -> eframe::egui_wgpu::WgpuConfiguration {
    use axcenity_render::wgpu;
    use eframe::egui_wgpu::{WgpuSetup, WgpuSetupCreateNew};

    let mut setup = WgpuSetupCreateNew::default();
    let base = setup.device_descriptor.clone();
    setup.device_descriptor = std::sync::Arc::new(move |adapter| {
        let mut descriptor = base(adapter);
        descriptor.required_features |= adapter.features() & wgpu::Features::TIMESTAMP_QUERY;
        descriptor
    });
    eframe::egui_wgpu::WgpuConfiguration {
        wgpu_setup: WgpuSetup::CreateNew(setup),
        ..Default::default()
    }
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
    // A debug editor relaunched into after a rebuild gets `--resume <handoff>`
    // instead (see `relaunch.rs`); release builds never take it.
    let mut args = std::env::args_os().skip(1);
    let first = args.next();
    let (folder, handoff) = match first {
        Some(flag) if cfg!(debug_assertions) && flag == relaunch::RESUME_FLAG => {
            (None, args.next().map(PathBuf::from))
        }
        other => (other.map(PathBuf::from), None),
    };

    let options = eframe::NativeOptions {
        // The viewport draws with wgpu, so egui must too (they share the GPU device).
        renderer: eframe::Renderer::Wgpu,
        // The editor draws its own title bar (`window_frame.rs`). The title
        // still names the window in the taskbar and window switcher.
        viewport: window_frame::viewport(
            egui::ViewportBuilder::default()
                .with_title("Axcenity Editor")
                .with_inner_size([1280.0, 800.0])
                .with_min_inner_size([800.0, 500.0])
                .with_icon(window_icon()),
        ),
        // Always open at the size above, in the middle of the screen. A
        // remembered size can outgrow a monitor (or span two) and stick.
        persist_window: false,
        centered: true,
        wgpu_options: wgpu_options(),
        ..Default::default()
    };
    eframe::run_native(
        APP_ID,
        options,
        // `move` hands `folder` over to the closure, which eframe calls once
        // the window exists to create the app.
        Box::new(move |creation_context| {
            let ctx = &creation_context.egui_ctx;
            theme::install(ctx);
            let mut app = EditorApp::new(None);
            app.load_settings(ctx, creation_context.storage);
            app.set_global_assets(global_assets::default_folder());
            app.set_plugin_data(plugins::default_folder());
            // Read in the background, so the window shows at once.
            if let Some(handoff) = &handoff {
                app.resume(ctx, handoff);
            } else if let Some(folder) = &folder {
                app.begin_open(ctx, folder);
            }
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
        // The tile's rounded corners are see-through; its middle is solid.
        assert_eq!(icon.rgba[3], 0);
        let middle = (128 * 256 + 128) * 4;
        assert_eq!(icon.rgba[middle + 3], 255);
    }
}
