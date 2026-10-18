//! Axcenity Player: the runtime that ships with every exported game.
//!
//! Phase 3: loads a project folder and draws its Workspace, with a fly camera
//! for looking around.

mod camera;
mod scene;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result, bail};
use axcenity_core::World;
use axcenity_render::Renderer;
use axcenity_render::glam::Vec3;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{DeviceEvent, DeviceId, ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::PhysicalKey;
use winit::window::{CursorGrabMode, Window, WindowId};

use crate::camera::{FlyCamera, MoveKeys};

const DEFAULT_WINDOW_SIZE: LogicalSize<f64> = LogicalSize::new(1280.0, 720.0);
const START_CAMERA_POSITION: Vec3 = Vec3::new(12.0, 8.0, 16.0);
/// After a long pause (such as dragging the window), don't jump the camera
/// by the whole pause; treat the frame as at most this long.
const MAX_FRAME_SECONDS: f32 = 0.1;
const USAGE: &str = "usage: axcenity-player <project-folder>\n\
                     example: cargo run -p axcenity-player -- examples/hello-part";

fn main() -> Result<()> {
    // Show warnings and errors by default. `wgpu_hal` relays harmless graphics
    // driver chatter as warnings, so only its errors are shown. Set RUST_LOG to
    // override (for example RUST_LOG=info).
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("warn,wgpu_hal=error"),
    )
    .init();

    // `args_os` instead of `args` so folder names that aren't valid Unicode still work.
    let Some(folder) = std::env::args_os().nth(1).map(PathBuf::from) else {
        bail!("no project folder given\n\n{USAGE}");
    };
    let project = axcenity_project::load(&folder)
        .with_context(|| format!("could not open the project in `{}`", folder.display()))?;
    log::info!("loaded project \"{}\"", project.manifest.name);

    let event_loop = EventLoop::new()?;
    // Games redraw continuously instead of waiting for input.
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = App {
        title: format!("{} - Axcenity Player", project.manifest.name),
        world: project.world,
        window: None,
        renderer: None,
        error: None,
        camera: FlyCamera::looking_at(START_CAMERA_POSITION, Vec3::ZERO),
        keys: MoveKeys::default(),
        looking: false,
        last_frame: None,
    };
    event_loop.run_app(&mut app)?;

    // Errors inside the event loop can't use `?`, so they're stored and reported here.
    match app.error {
        Some(err) => Err(err),
        None => Ok(()),
    }
}

/// Everything the player owns while running. `window` and `renderer` start as
/// `None` because winit only allows creating windows once the event loop has started.
struct App {
    title: String,
    world: World,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    error: Option<anyhow::Error>,
    camera: FlyCamera,
    keys: MoveKeys,
    /// True while the right mouse button is held, when moving the mouse turns the camera.
    looking: bool,
    /// When the previous frame was drawn, to measure how much time passed.
    last_frame: Option<Instant>,
}

impl App {
    fn fail(&mut self, event_loop: &ActiveEventLoop, err: anyhow::Error) {
        log::error!("{err:#}");
        self.error = Some(err);
        event_loop.exit();
    }
}

/// Seconds since `last_frame` (0 for the first frame), and records now as the latest frame.
///
/// This takes only the `last_frame` field rather than all of `App`, so it can
/// be called while other fields of `App` are borrowed.
fn frame_seconds(last_frame: &mut Option<Instant>) -> f32 {
    let now = Instant::now();
    let seconds = last_frame.map_or(0.0, |last| (now - last).as_secs_f32());
    *last_frame = Some(now);
    seconds.min(MAX_FRAME_SECONDS)
}

/// While looking around, hides the cursor and keeps it from leaving the window.
fn capture_mouse(window: &Window, captured: bool) {
    let result = if captured {
        // `Locked` keeps the cursor still (macOS, Wayland). Windows and X11
        // only offer `Confined`, which keeps it inside the window.
        window
            .set_cursor_grab(CursorGrabMode::Locked)
            .or_else(|_| window.set_cursor_grab(CursorGrabMode::Confined))
    } else {
        window.set_cursor_grab(CursorGrabMode::None)
    };
    // Looking around still works without the grab, so this isn't an error.
    if let Err(err) = result {
        log::debug!("could not grab the mouse cursor: {err}");
    }
    window.set_cursor_visible(!captured);
}

impl ApplicationHandler for App {
    /// Called when the app is ready to create windows (at startup on desktop).
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let attributes = Window::default_attributes()
            .with_title(&self.title)
            .with_inner_size(DEFAULT_WINDOW_SIZE);
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(err) => return self.fail(event_loop, err.into()),
        };

        // GPU setup is `async`; `pollster::block_on` simply waits for it to finish.
        let size = window.inner_size();
        match pollster::block_on(Renderer::new(window.clone(), size.width, size.height)) {
            Ok(renderer) => self.renderer = Some(renderer),
            Err(err) => return self.fail(event_loop, err),
        }

        window.request_redraw();
        self.window = Some(window);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let (Some(window), Some(renderer)) = (&self.window, &mut self.renderer) else {
            return;
        };

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => renderer.resize(size.width, size.height),
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(key) = event.physical_key {
                    self.keys.set(key, event.state == ElementState::Pressed);
                }
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Right,
                ..
            } => {
                self.looking = state == ElementState::Pressed;
                capture_mouse(window, self.looking);
            }
            // Key releases aren't reported while another window has focus,
            // so let go of everything to avoid the camera drifting on its own.
            WindowEvent::Focused(false) => {
                self.keys = MoveKeys::default();
                self.looking = false;
                capture_mouse(window, false);
            }
            WindowEvent::RedrawRequested => {
                let seconds = frame_seconds(&mut self.last_frame);
                self.camera.update(&self.keys, seconds);

                let draw_list = scene::build_draw_list(&self.world, self.camera.to_render_camera());
                // `|| ...` is a closure: a small function the renderer calls at the right moment.
                if let Err(err) = renderer.render(&draw_list, || window.pre_present_notify()) {
                    self.fail(event_loop, err);
                }
            }
            _ => {}
        }
    }

    /// Raw mouse movement, reported even when the cursor is held still.
    fn device_event(&mut self, _event_loop: &ActiveEventLoop, _id: DeviceId, event: DeviceEvent) {
        if let DeviceEvent::MouseMotion { delta: (dx, dy) } = event
            && self.looking
        {
            self.camera.look(dx as f32, dy as f32);
        }
    }

    /// Called once all pending events are handled: ask for the next frame.
    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}
