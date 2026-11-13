//! Axcenity Editor: open a project folder, look around it in a 3D viewport,
//! and see its Instance tree in the Explorer.
//!
//! Phase 12: insert, select, move, edit, rename, delete, duplicate and
//! regroup parts, with undo, and save. Play runs the game in the viewport,
//! and Stop puts the world back as it was.
//! Built on eframe (egui's app framework), which owns the window and the GPU
//! device; the viewport draws with the same renderer as the player.

// The shared UI base lives in the editor kit crate; these keep its modules
// at `crate::theme`, `crate::widgets` and so on (roadmap G11).
#[cfg(test)]
use axcenity_editor_kit::snapshot;
use axcenity_editor_kit::{
    color_palette, css_syntax, lua_syntax, responsive, screen, wheel, widgets,
};
// Panel families in crates of their own, under their old module names.
use axcenity_editor_benchmark as graphics_benchmark;
use axcenity_editor_model_studio as model_studio;
use axcenity_editor_node_graph as node_graph;
use axcenity_editor_toolbox as toolbox_panel;
use axcenity_editor_ui_designer::{self as ui_designer, style_panel, ui_handles};
// The viewport tools, shared with the studios, under their old module names.
use axcenity_editor_tools::{gizmo, props, shortcuts, snapping, tree};

mod angular;
mod anim_import;
mod animation;
mod app;
mod asset_browser;
mod asset_check;
mod asset_inventory;
mod autocomplete;
mod behavior_tree;
mod blender;
mod block_editor;
mod build_game;
mod build_tools;
mod command_palette;
mod console;
mod constraints;
mod cooking;
mod data_table;
mod dialog;
mod dialogue_editor;
mod effect_editor;
mod explorer;
mod find_in_scripts;
mod game_tests;
mod global_assets;
#[cfg(test)]
mod graph_keys_tests;
mod history;
mod import_dialog;
mod layout;
mod levels;
mod loading;
mod lod_inspector;
mod luau_check;
mod material_editor;
mod material_graph;
mod mixer;
mod output;
mod people_history;
mod placing;
mod play;
mod plugins;
mod prefab_menu;
mod profiler;
mod properties;
mod prototype;
mod rebuild;
mod relaunch;
mod ribbon;
mod ribbon_tabs;
mod script_completion;
mod scripts;
mod sequencer;
mod side_warnings;
mod spawning;
mod splines;
mod start;
mod surface;
mod tags;
mod team_board;
mod terrain_studio;
mod theme;
mod thumbnail;
mod toast;
#[cfg(test)]
mod ui_cost_tests;
mod version_explorer;
mod viewport;
mod watcher;
mod watches;
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
/// Profiler can show the viewport's GPU time (the player never asks for
/// those), and BC7 for cooked textures.
fn wgpu_options() -> eframe::egui_wgpu::WgpuConfiguration {
    use axcenity_render::wgpu;
    use eframe::egui_wgpu::{WgpuSetup, WgpuSetupCreateNew};

    let mut setup = WgpuSetupCreateNew::without_display_handle();
    // The same card and API as the player: DirectX 12 on Windows
    // (`axcenity_render::adapters`).
    setup.native_adapter_selector = Some(std::sync::Arc::new(|adapters, surface| {
        axcenity_render::adapters::pick(adapters, surface)
            .ok_or_else(|| "no graphics card can draw to this window".to_owned())
    }));
    let base = setup.device_descriptor.clone();
    setup.device_descriptor = std::sync::Arc::new(move |adapter| {
        let mut descriptor = base(adapter);
        // Cooked textures go up as BC7 where the GPU reads it.
        descriptor.required_features |= adapter.features()
            & (wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::TEXTURE_COMPRESSION_BC);
        // Hardware ray tracing, if Settings > Graphics turned it on and the
        // GPU has it (`axcenity_render::adapters::with_ray_queries`).
        axcenity_render::adapters::want_ray_tracing(app::settings::saved_ray_tracing());
        axcenity_render::adapters::with_ray_queries(adapter, &mut descriptor);
        descriptor
    });
    eframe::egui_wgpu::WgpuConfiguration {
        wgpu_setup: WgpuSetup::CreateNew(setup),
        ..Default::default()
    }
}

fn main() -> eframe::Result {
    // `--version` (and `--version --json`): this build's version, channel,
    // commit and toolchain, then exit.
    let all: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    if let Some(text) = axcenity_release::version_flag("axcenity-editor", &all) {
        println!("{text}");
        return Ok(());
    }
    // Same logging defaults as the player: warnings and errors, minus
    // harmless graphics driver chatter. Set RUST_LOG to override.
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("warn,wgpu_hal=error"),
    )
    .init();

    // An optional project folder to open, such as `examples/hello-touched`.
    // `args_os` instead of `args` so folder names that aren't valid Unicode still work.
    // An editor relaunched into, after a rebuild or by Restart now (a
    // setting that needs a restart, such as Hardware ray tracing), gets
    // `--resume <handoff>` instead (see `relaunch.rs`).
    let mut args = std::env::args_os().skip(1);
    let first = args.next();
    if first.as_deref() == Some(cooking::COOK_FLAG.as_ref()) {
        let Some(folder) = args.next() else {
            eprintln!(
                "usage: axcenity-editor {} <project folder>",
                cooking::COOK_FLAG
            );
            std::process::exit(2);
        };
        let cooked = cooking::cook_from_command_line(&PathBuf::from(folder));
        std::process::exit(if cooked { 0 } else { 1 });
    }
    if first.as_deref() == Some(app::mcp::FLAG.as_ref()) {
        // An MCP server for AI clients, without a window (`app/mcp.rs`).
        // It builds the whole editor state, which a debug build keeps on
        // the stack for a while: more than the 1 MB Windows gives the main
        // thread. So it runs on a thread of its own with room to spare.
        let args: Vec<std::ffi::OsString> = args.collect();
        let served = std::thread::Builder::new()
            .name("mcp main".into())
            .stack_size(app::mcp::STACK_SIZE)
            .spawn(move || app::mcp::run_from_command_line(args.into_iter()))
            .map(|thread| thread.join().unwrap_or(false))
            .unwrap_or_else(|error| {
                eprintln!("axcenity-editor --mcp: could not start: {error}");
                false
            });
        std::process::exit(if served { 0 } else { 1 });
    }
    if first.as_deref() == Some(build_game::BUILD_FLAG.as_ref()) {
        let built = build_game::build_from_command_line(args);
        std::process::exit(if built { 0 } else { 1 });
    }
    // `--collab-host` or `--collab-join <link>` after the project folder
    // (the old `--team-*` flags too): a Collaborate session to start once it
    // opens (`app/team.rs`). Without a folder, `--collab-join <link>` joins
    // from the start screen, and a bare invite link (as the OS passes one)
    // is offered there, to join with a click.
    let mut rest: Vec<String> = Vec::new();
    let mut offered: Option<String> = None;
    let (folder, handoff) = match first {
        Some(flag) if flag == relaunch::RESUME_FLAG => (None, args.next().map(PathBuf::from)),
        Some(flag) if flag == app::team::JOIN_FLAG || flag == app::team::OLD_JOIN_FLAG => {
            rest.push(app::team::JOIN_FLAG.to_owned());
            (None, None)
        }
        // A bare link is how the system hands over one that was clicked:
        // the start screen offers to join, and waits for the user (C4).
        Some(link) if axcenity_collab::access::is_link(&link.to_string_lossy()) => {
            offered = Some(link.to_string_lossy().into_owned());
            (None, None)
        }
        other => (other.map(PathBuf::from), None),
    };
    rest.extend(args.map(|a| a.to_string_lossy().into_owned()));
    let team_start = match app::team::TeamStart::from_args(&rest) {
        Ok(start) => start,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    };
    if team_start == Some(app::team::TeamStart::Host) && folder.is_none() {
        eprintln!(
            "usage: axcenity-editor <project folder> {} | [<project folder>] {} <invite link>",
            app::team::HOST_FLAG,
            app::team::JOIN_FLAG
        );
        std::process::exit(2);
    }

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
            app.set_window_icon(window_icon());
            app.load_settings(ctx, creation_context.storage);
            app.set_global_assets(global_assets::default_folder());
            app.set_templates_folder(EditorApp::default_templates_folder());
            app.set_plugin_data(plugins::default_folder());
            app.cook_textures_in_background();
            // Read in the background, so the window shows at once.
            if let Some(handoff) = &handoff {
                app.resume(ctx, handoff);
            } else if let Some(folder) = &folder {
                if let Some(start) = team_start.clone() {
                    app.start_team_after_open(start);
                }
                app.begin_open(ctx, folder);
            } else if let Some(app::team::TeamStart::Join(invite)) = &team_start {
                app.join_from_start(invite, None);
            } else if let Some(link) = offered {
                app.offer_join(link);
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
