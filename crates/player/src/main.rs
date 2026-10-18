//! Axcenity Player: the runtime that ships with every exported game.
//!
//! Phase 6: loads a project folder, starts its server scripts, and runs the
//! frame loop: resume scripts whose `task.wait` is over, step physics, then
//! draw the Workspace, with a fly camera for looking around.

mod camera;
mod scene;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result, bail};
use axcenity_core::World;
use axcenity_physics::Physics;
use axcenity_project::ScriptFile;
use axcenity_render::Renderer;
use axcenity_render::glam::Vec3;
use axcenity_script::{OutputKind, ScriptRuntime};
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
    let mut world = project.world;
    let mut scripts = ScriptRuntime::new();
    start_scripts(&mut scripts, &mut world, &project.scripts);

    let event_loop = EventLoop::new()?;
    // Games redraw continuously instead of waiting for input.
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = App {
        title: format!("{} - Axcenity Player", project.manifest.name),
        world,
        scripts,
        physics: Physics::new(),
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

/// Starts every server script, in path order, and prints its output. Each
/// runs until it finishes or first waits; the frame loop resumes it after
/// that. An error stops only the script it happens in.
fn start_scripts(runtime: &mut ScriptRuntime, world: &mut World, scripts: &[ScriptFile]) {
    for script in scripts {
        log::info!("starting {}", script.path);
        runtime.start_script(world, script.instance, &script.path, &script.source);
        print_output(runtime);
    }
}

/// Prints what scripts printed to stdout, and their errors to stderr.
fn print_output(runtime: &mut ScriptRuntime) {
    for line in runtime.take_output() {
        match line.kind {
            OutputKind::Print => println!("{}", line.text),
            OutputKind::Error => eprintln!("error: {}", line.text),
        }
    }
}

/// Everything the player owns while running. `window` and `renderer` start as
/// `None` because winit only allows creating windows once the event loop has started.
struct App {
    title: String,
    world: World,
    /// Runs the scripts, and keeps the ones paused by `task.wait`.
    scripts: ScriptRuntime,
    /// Simulates the unanchored parts.
    physics: Physics,
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
            // One frame, in the fixed order from docs/architecture.md.
            WindowEvent::RedrawRequested => {
                let seconds = frame_seconds(&mut self.last_frame);

                // 1. Input: the events above have already updated the keys
                //    and mouse look, so move the camera.
                self.camera.update(&self.keys, seconds);

                // 2. Resume scripts whose wait is over (and due `task.delay`s).
                //    Game time is the sum of the (capped) frame times, so a
                //    long pause doesn't make every waiting script fire at once.
                self.scripts.step(&mut self.world, f64::from(seconds));
                print_output(&mut self.scripts);

                // 3. Step physics at a fixed 60 Hz (0 to N steps this frame),
                //    after scripts so it sees what they changed this frame.
                self.physics.step(&mut self.world, f64::from(seconds));

                // 4 and 5 (Touched events, UI layout) come in later phases.

                // 6. Build the draw list and render.
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

#[cfg(test)]
mod tests {
    use std::path::Path;

    use axcenity_core::{Value, Vector3};

    use super::*;

    #[test]
    fn hello_script_example_creates_a_part_and_moves_an_existing_one() {
        let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-script");
        let project = axcenity_project::load(&folder).unwrap();
        let mut world = project.world;
        let parts_before = world.descendants(world.workspace()).len();

        start_scripts(&mut ScriptRuntime::new(), &mut world, &project.scripts);

        let workspace = world.workspace();
        let block = world.find_first_child(workspace, "Block").unwrap();
        assert_eq!(
            world.get(block).unwrap().property("Position"),
            Some(&Value::Vector3(Vector3::new(-8.0, 3.0, 0.0)))
        );
        for i in 1..=5 {
            assert!(
                world
                    .find_first_child(workspace, &format!("Ball{i}"))
                    .is_some()
            );
        }
        // The new parts are drawn along with the ones from the world file.
        let camera = FlyCamera::looking_at(START_CAMERA_POSITION, Vec3::ZERO).to_render_camera();
        let draw_list = scene::build_draw_list(&world, camera);
        assert_eq!(draw_list.items.len(), parts_before + 5);
    }

    /// The example project in `examples/<name>`, with its scripts started.
    fn start_example(name: &str) -> (World, ScriptRuntime) {
        let folder = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples")
            .join(name);
        let project = axcenity_project::load(&folder).unwrap();
        let mut world = project.world;
        let mut scripts = ScriptRuntime::new();
        start_scripts(&mut scripts, &mut world, &project.scripts);
        (world, scripts)
    }

    /// Runs frames at 60 fps until the game clock reaches `until` seconds,
    /// with scripts and then physics each frame, as the frame loop does.
    fn run_until(
        world: &mut World,
        scripts: &mut ScriptRuntime,
        physics: &mut Physics,
        clock: &mut u32,
        until: f64,
    ) {
        while f64::from(*clock) / 60.0 < until {
            scripts.step(world, 1.0 / 60.0);
            physics.step(world, 1.0 / 60.0);
            *clock += 1;
        }
    }

    fn position_of(world: &World, name: &str) -> Vector3 {
        let id = world.find_first_child(world.workspace(), name).unwrap();
        world
            .get(id)
            .unwrap()
            .property("Position")
            .and_then(Value::as_vector3)
            .unwrap()
    }

    #[test]
    fn hello_physics_parts_fall_collide_and_come_to_rest_on_the_floor() {
        let (mut world, mut scripts) = start_example("hello-physics");
        let mut physics = Physics::new();
        let mut clock = 0;
        let moving = [
            "Tower1",
            "Tower2",
            "Tower3",
            "Dropper",
            "RampBall",
            "GlassCylinder",
        ];
        let start: Vec<Vector3> = moving.iter().map(|n| position_of(&world, n)).collect();

        run_until(&mut world, &mut scripts, &mut physics, &mut clock, 9.0);
        let settled: Vec<Vector3> = moving.iter().map(|n| position_of(&world, n)).collect();
        run_until(&mut world, &mut scripts, &mut physics, &mut clock, 9.9);

        for (i, name) in moving.iter().enumerate() {
            let now = position_of(&world, name);
            // On the floor or on another part, not fallen through it.
            assert!(now.y > 0.4 && now.y < 8.0, "{name} is at {now:?}");
            let still = (now.x - settled[i].x).abs() < 0.01
                && (now.y - settled[i].y).abs() < 0.01
                && (now.z - settled[i].z).abs() < 0.01;
            assert!(still, "{name} still moving: {:?} -> {now:?}", settled[i]);
        }
        // Everything except the bottom of the tower (already on the floor) fell,
        // if only by the small gaps between the tower blocks.
        for (i, name) in moving.iter().enumerate().skip(1) {
            assert!(settled[i].y < start[i].y - 0.02, "{name} didn't fall");
        }
        // The ball rolled down the ramp, away from where it landed.
        assert!(
            position_of(&world, "RampBall").z < -4.0,
            "{:?}",
            position_of(&world, "RampBall")
        );
        assert_eq!(position_of(&world, "Floor"), Vector3::new(0.0, -0.5, 0.0));
    }

    #[test]
    fn hello_physics_script_freezes_and_releases_the_dropper() {
        let (mut world, mut scripts) = start_example("hello-physics");
        let mut physics = Physics::new();
        let mut clock = 0;
        let mut at = |until| {
            run_until(&mut world, &mut scripts, &mut physics, &mut clock, until);
            position_of(&world, "Dropper").y
        };

        assert_eq!(at(1.9), 14.0, "anchored at the start");
        let frozen = at(2.5);
        assert!(
            frozen < 13.0 && frozen > 8.0,
            "released, then frozen: {frozen}"
        );
        assert_eq!(at(3.5), frozen, "frozen in mid-air");
        assert!(at(5.0) < 8.0, "released again, it fell onto the tower");
    }

    #[test]
    fn hello_physics_script_teleports_the_bouncy_ball_back_to_its_start() {
        let (mut world, mut scripts) = start_example("hello-physics");
        let mut physics = Physics::new();
        let mut clock = 0;
        run_until(&mut world, &mut scripts, &mut physics, &mut clock, 4.9);
        assert!(position_of(&world, "BouncyBall").y < 2.0, "it fell");
        // The script runs at 5 s; one physics step of falling from rest follows.
        run_until(
            &mut world,
            &mut scripts,
            &mut physics,
            &mut clock,
            5.0 + 1.5 / 60.0,
        );
        let y = position_of(&world, "BouncyBall").y;
        assert!(y > 11.5 && y < 12.0, "{y}");
    }

    #[test]
    fn hello_tasks_example_blinks_spins_and_adds_a_ball_over_time() {
        let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-tasks");
        let project = axcenity_project::load(&folder).unwrap();
        let mut world = project.world;
        let mut scripts = ScriptRuntime::new();
        start_scripts(&mut scripts, &mut world, &project.scripts);

        let workspace = world.workspace();
        let lamp = world.find_first_child(workspace, "Lamp").unwrap();
        let spinner = world.find_first_child(workspace, "Spinner").unwrap();
        let property = |world: &World, id, name| world.get(id).unwrap().property(name).cloned();

        // 3.5 seconds at 60 frames per second, the way the frame loop steps it.
        let mut color = property(&world, lamp, "Color");
        let mut color_changes = 0;
        let mut printed = Vec::new();
        for _ in 0..210 {
            scripts.step(&mut world, 1.0 / 60.0);
            printed.extend(scripts.take_output().into_iter().map(|line| line.text));
            if property(&world, lamp, "Color") != color {
                color = property(&world, lamp, "Color");
                color_changes += 1;
            }
        }

        assert_eq!(color_changes, 3);
        assert_eq!(
            printed,
            [
                "blink 1",
                "blink 2",
                "blink 3",
                "a ball appeared after 3 seconds"
            ]
        );
        assert!(world.find_first_child(workspace, "Ball").is_some());
        // 90 degrees a second for 3.5 seconds is 315 degrees.
        let Some(Value::Vector3(orientation)) = property(&world, spinner, "Orientation") else {
            panic!("the spinner has no Orientation");
        };
        assert!((orientation.y - 315.0).abs() < 0.5, "{orientation:?}");
    }
}
