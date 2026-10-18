//! Axcenity Player: the runtime that ships with every exported game.
//!
//! Loads a project folder, starts its Scripts and then its LocalScripts, and
//! runs the frame loop: pass key presses to LocalScripts, resume scripts
//! whose `task.wait` is over (and deliver RemoteEvent messages), step
//! physics, fire `Touched` for parts that started touching, then draw the
//! Workspace, with a fly camera for looking around.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result, bail};
use axcenity_core::World;
use axcenity_physics::Physics;
use axcenity_project::ScriptFile;
use axcenity_render::Renderer;
use axcenity_render::glam::Vec3;
use axcenity_scene::{FlyCamera, MoveKeys};
use axcenity_script::{CameraState, KeyEvent, OutputKind, ScriptRuntime};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{DeviceEvent, DeviceId, ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Window, WindowId};

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
    // A damaged file only takes itself out: say which, and run the rest.
    for problem in project.problems {
        eprintln!("error: {:#}", anyhow::Error::new(problem));
    }
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
        key_events: Vec::new(),
        held_keys: BTreeSet::new(),
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

/// Starts every script in the order the project lists them (Scripts, then
/// LocalScripts, each by path), and prints their output. Each runs until it
/// finishes or first waits; the frame loop resumes it after that. An error
/// stops only the script it happens in.
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
    /// Key presses and releases since the last frame, for LocalScripts.
    key_events: Vec<KeyEvent>,
    /// The keys held down, so they can all be let go when the window loses focus.
    held_keys: BTreeSet<axcenity_core::KeyCode>,
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

/// Records a key press or release. Keys are physical positions, so the same
/// keys work on AZERTY and other layouts.
fn set_move_key(keys: &mut MoveKeys, key: KeyCode, pressed: bool) {
    let held = match key {
        KeyCode::KeyW => &mut keys.forward,
        KeyCode::KeyS => &mut keys.back,
        KeyCode::KeyA => &mut keys.left,
        KeyCode::KeyD => &mut keys.right,
        KeyCode::KeyE => &mut keys.up,
        KeyCode::KeyQ => &mut keys.down,
        KeyCode::ShiftLeft | KeyCode::ShiftRight => &mut keys.slow,
        _ => return,
    };
    *held = pressed;
}

/// Each winit key, and the key scripts know it as (`Enum.KeyCode.<name>`).
/// Keys are physical positions, like the movement keys.
const SCRIPT_KEYS: [(KeyCode, axcenity_core::KeyCode); 51] = {
    use axcenity_core::KeyCode as K;
    [
        (KeyCode::KeyA, K::A),
        (KeyCode::KeyB, K::B),
        (KeyCode::KeyC, K::C),
        (KeyCode::KeyD, K::D),
        (KeyCode::KeyE, K::E),
        (KeyCode::KeyF, K::F),
        (KeyCode::KeyG, K::G),
        (KeyCode::KeyH, K::H),
        (KeyCode::KeyI, K::I),
        (KeyCode::KeyJ, K::J),
        (KeyCode::KeyK, K::K),
        (KeyCode::KeyL, K::L),
        (KeyCode::KeyM, K::M),
        (KeyCode::KeyN, K::N),
        (KeyCode::KeyO, K::O),
        (KeyCode::KeyP, K::P),
        (KeyCode::KeyQ, K::Q),
        (KeyCode::KeyR, K::R),
        (KeyCode::KeyS, K::S),
        (KeyCode::KeyT, K::T),
        (KeyCode::KeyU, K::U),
        (KeyCode::KeyV, K::V),
        (KeyCode::KeyW, K::W),
        (KeyCode::KeyX, K::X),
        (KeyCode::KeyY, K::Y),
        (KeyCode::KeyZ, K::Z),
        (KeyCode::Digit0, K::Zero),
        (KeyCode::Digit1, K::One),
        (KeyCode::Digit2, K::Two),
        (KeyCode::Digit3, K::Three),
        (KeyCode::Digit4, K::Four),
        (KeyCode::Digit5, K::Five),
        (KeyCode::Digit6, K::Six),
        (KeyCode::Digit7, K::Seven),
        (KeyCode::Digit8, K::Eight),
        (KeyCode::Digit9, K::Nine),
        (KeyCode::Space, K::Space),
        (KeyCode::Enter, K::Return),
        (KeyCode::Escape, K::Escape),
        (KeyCode::Tab, K::Tab),
        (KeyCode::Backspace, K::Backspace),
        (KeyCode::ArrowUp, K::Up),
        (KeyCode::ArrowDown, K::Down),
        (KeyCode::ArrowLeft, K::Left),
        (KeyCode::ArrowRight, K::Right),
        (KeyCode::ShiftLeft, K::LeftShift),
        (KeyCode::ShiftRight, K::RightShift),
        (KeyCode::ControlLeft, K::LeftControl),
        (KeyCode::ControlRight, K::RightControl),
        (KeyCode::AltLeft, K::LeftAlt),
        (KeyCode::AltRight, K::RightAlt),
    ]
};

/// The key scripts know for a winit key, if it's one they can see.
fn script_key(key: KeyCode) -> Option<axcenity_core::KeyCode> {
    SCRIPT_KEYS
        .into_iter()
        .find(|(winit, _)| *winit == key)
        .map(|(_, script)| script)
}

/// The camera as `workspace.CurrentCamera` shows it to LocalScripts.
fn camera_state(camera: &FlyCamera) -> CameraState {
    let (position, orientation) = camera.view();
    CameraState {
        position,
        orientation,
    }
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
                    let pressed = event.state == ElementState::Pressed;
                    set_move_key(&mut self.keys, key, pressed);
                    if let Some(key) = script_key(key) {
                        self.key_events.push(KeyEvent { key, pressed });
                        if pressed {
                            self.held_keys.insert(key);
                        } else {
                            self.held_keys.remove(&key);
                        }
                    }
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
                for key in std::mem::take(&mut self.held_keys) {
                    self.key_events.push(KeyEvent {
                        key,
                        pressed: false,
                    });
                }
                self.looking = false;
                capture_mouse(window, false);
            }
            // One frame, in the fixed order from docs/architecture.md.
            WindowEvent::RedrawRequested => {
                let seconds = frame_seconds(&mut self.last_frame);

                // 1. Input: the events above have already updated the keys
                //    and mouse look, so move the camera. Then LocalScripts
                //    see where it is, and the keys pressed since last frame.
                self.camera.update(&self.keys, seconds);
                self.scripts.set_camera(camera_state(&self.camera));
                let keys = std::mem::take(&mut self.key_events);
                self.scripts.input(&mut self.world, &keys);
                print_output(&mut self.scripts);

                // 2. Resume scripts whose wait is over (and due `task.delay`s).
                //    Game time is the sum of the (capped) frame times, so a
                //    long pause doesn't make every waiting script fire at once.
                self.scripts.step(&mut self.world, f64::from(seconds));
                print_output(&mut self.scripts);

                // 3. Step physics at a fixed 60 Hz (0 to N steps this frame),
                //    after scripts so it sees what they changed this frame.
                self.physics.step(&mut self.world, f64::from(seconds));

                // 4. Fire `Touched` for parts that started touching during
                //    those steps. Handlers that wait carry on in step 2.
                let touches = self.physics.take_touches();
                self.scripts.fire_touched(&mut self.world, &touches);
                print_output(&mut self.scripts);

                // 5 (UI layout) comes in a later phase.

                // 6. Build the draw list and render, from wherever a
                //    LocalScript put the camera (if one moved it).
                if let Some(view) = self.scripts.take_camera_move() {
                    self.camera.set_view(view.position, view.orientation);
                }
                let draw_list =
                    axcenity_scene::build_draw_list(&self.world, self.camera.to_render_camera());
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
    fn arrow_and_letter_keys_reach_scripts_by_their_roblox_names() {
        use axcenity_core::KeyCode as K;
        assert_eq!(script_key(KeyCode::ArrowUp), Some(K::Up));
        assert_eq!(script_key(KeyCode::KeyW), Some(K::W));
        assert_eq!(script_key(KeyCode::Digit0), Some(K::Zero));
        assert_eq!(script_key(KeyCode::Enter), Some(K::Return));
        assert_eq!(script_key(KeyCode::F13), None);
        // Every key scripts know can be pressed in the player.
        for key in K::ALL {
            assert!(
                SCRIPT_KEYS.iter().any(|(_, script)| *script == key),
                "{key:?}"
            );
        }
    }

    #[test]
    fn hello_remote_example_moves_the_block_while_an_arrow_key_is_held() {
        let (mut world, mut scripts) = start_example("hello-remote");
        let mover = world.find_first_child(world.workspace(), "Mover").unwrap();
        let start = position_of(&world, "Mover");
        let mut physics = Physics::new();
        let mut frame = |world: &mut World, keys: &[KeyEvent]| {
            scripts.input(world, keys);
            scripts.step(world, 1.0 / 60.0);
            physics.step(world, 1.0 / 60.0);
            let touches = physics.take_touches();
            scripts.fire_touched(world, &touches);
        };
        // Hold Right for one second: the LocalScript sends the direction
        // every frame, and the Script moves the block 12 studs a second.
        let right = axcenity_core::KeyCode::Right;
        frame(
            &mut world,
            &[KeyEvent {
                key: right,
                pressed: true,
            }],
        );
        for _ in 0..59 {
            frame(&mut world, &[]);
        }
        frame(
            &mut world,
            &[KeyEvent {
                key: right,
                pressed: false,
            }],
        );
        let moved = position_of(&world, "Mover");
        assert!(
            (moved.x - start.x - 12.0).abs() < 0.5,
            "{start:?} -> {moved:?}"
        );
        assert_eq!((moved.y, moved.z), (start.y, start.z));
        // Let go: it stays put.
        for _ in 0..30 {
            frame(&mut world, &[]);
        }
        assert_eq!(position_of(&world, "Mover"), moved);
        assert!(world.contains(mover));
        let errors: Vec<String> = scripts
            .take_output()
            .into_iter()
            .filter(|line| line.kind == OutputKind::Error)
            .map(|line| line.text)
            .collect();
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn keys_are_recorded_and_released() {
        let mut keys = MoveKeys::default();
        set_move_key(&mut keys, KeyCode::KeyW, true);
        set_move_key(&mut keys, KeyCode::ShiftRight, true);
        set_move_key(&mut keys, KeyCode::KeyZ, true);
        assert!(keys.forward && keys.slow);
        set_move_key(&mut keys, KeyCode::KeyW, false);
        assert!(!keys.forward);
    }

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
        let draw_list = axcenity_scene::build_draw_list(&world, camera);
        assert_eq!(draw_list.items.len(), parts_before + 5);
    }

    #[test]
    fn hello_modules_example_builds_stairs_through_two_modules() {
        let (world, _) = start_example("hello-modules");
        let workspace = world.workspace();
        let colors: Vec<Option<&Value>> = (1..=8)
            .map(|n| {
                let step = world
                    .find_first_child(workspace, &format!("Step{n}"))
                    .unwrap();
                world.get(step).unwrap().property("Color")
            })
            .collect();
        // The Palette module, required by the Builder module, goes round
        // four colors.
        assert_eq!(colors[0], colors[4]);
        assert_ne!(colors[0], colors[1]);
        // The modules are in ReplicatedStorage, where both sides find them.
        let storage = world.replicated_storage();
        assert!(world.find_first_child(storage, "Palette").is_some());
        assert!(world.find_first_child(storage, "Builder").is_some());
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
    /// with scripts, physics and then `Touched` each frame, as the frame loop
    /// does.
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
            let touches = physics.take_touches();
            scripts.fire_touched(world, &touches);
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

    /// Like `run_until`, but also collects what scripts printed.
    fn run_and_collect(
        world: &mut World,
        scripts: &mut ScriptRuntime,
        physics: &mut Physics,
        clock: &mut u32,
        until: f64,
    ) -> Vec<String> {
        let mut printed = Vec::new();
        while f64::from(*clock) / 60.0 < until {
            run_until(world, scripts, physics, clock, f64::from(*clock + 1) / 60.0);
            printed.extend(
                scripts
                    .take_output()
                    .into_iter()
                    .map(|line| match line.kind {
                        OutputKind::Print => line.text,
                        OutputKind::Error => format!("error: {}", line.text),
                    }),
            );
        }
        printed
    }

    #[test]
    fn hello_touched_demo_recolors_the_pad_and_prints_what_landed() {
        let (mut world, mut scripts) = start_example("hello-touched");
        let mut physics = Physics::new();
        let mut clock = 0;
        let pad = world.find_first_child(world.workspace(), "Pad").unwrap();
        let color = |world: &World| world.get(pad).unwrap().property("Color").cloned();
        let start_color = color(&world);

        // The red ball falls about 9 studs and lands within the first second.
        let first = run_and_collect(&mut world, &mut scripts, &mut physics, &mut clock, 2.9);
        assert!(!first.is_empty(), "the ball should have landed");
        assert!(
            first.iter().all(|line| line == "Ball touched the pad"),
            "{first:?}"
        );
        assert_ne!(color(&world), start_color, "the pad changed color");

        // At 3 s a blue ball drops; at 6 s the script disconnects.
        let second = run_and_collect(&mut world, &mut scripts, &mut physics, &mut clock, 6.0);
        let (last, landings) = second.split_last().unwrap();
        assert_eq!(last, "stopped listening for touches");
        assert!(!landings.is_empty(), "the blue ball should have landed");
        assert!(
            landings
                .iter()
                .all(|line| line == "BlueBall touched the pad"),
            "{second:?}"
        );

        // The last ball lands after the disconnect, so nothing more happens.
        let after = run_and_collect(&mut world, &mut scripts, &mut physics, &mut clock, 9.0);
        assert_eq!(after, Vec::<String>::new());
        let ignored = position_of(&world, "IgnoredBall");
        assert!(ignored.y < 2.0, "the last ball landed: {ignored:?}");
    }

    #[test]
    fn settled_hello_physics_parts_stop_reporting_touches() {
        let (mut world, mut scripts) = start_example("hello-physics");
        let mut physics = Physics::new();
        let mut clock = 0;
        run_until(&mut world, &mut scripts, &mut physics, &mut clock, 9.0);
        // Everything is at rest now, so 3 more seconds report no new touches.
        let mut touches = Vec::new();
        for _ in 0..180 {
            physics.step(&mut world, 1.0 / 60.0);
            touches.extend(physics.take_touches());
        }
        assert_eq!(touches, []);
    }
}
