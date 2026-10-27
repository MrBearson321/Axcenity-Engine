//! Axcenity Player: the runtime that ships with every exported game.
//!
//! Loads a project folder, starts its Scripts and then its LocalScripts, and
//! runs the frame loop: move a third-person character when the world has
//! one, pass key presses to LocalScripts, resume scripts whose `task.wait`
//! is over (and deliver RemoteEvent messages), step physics, fire `Touched`
//! for parts that started touching, then draw the Workspace. The character's
//! camera follows it; otherwise a fly camera looks around. The keyboard and
//! game controllers can move focus through menus (`axcenity_ui::NavKey`).

mod graphics;
mod icon;
mod splash;

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use axcenity_anim::Animators;
use axcenity_assets::Assets;
use axcenity_audio::{GameAudio, SoundAction};
use axcenity_character::{Controllers, MoveInput};
use axcenity_core::{InstanceId, World};
use axcenity_nav::Agents;
use axcenity_physics::Physics;
use axcenity_project::ScriptFile;
use axcenity_render::glam::Vec3;
use axcenity_render::{Capabilities, GraphicsSettings, Renderer};
use axcenity_scene::{FlyCamera, MoveKeys, UiFrame};
use axcenity_script::{CameraState, KeyEvent, OutputKind, ScriptRuntime};
use axcenity_ui::{Gamepads, LoadingGui, NavKey, UiCursor, UiEvent, UiInput, UiInteraction};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{
    DeviceEvent, DeviceId, ElementState, MouseButton, MouseScrollDelta, WindowEvent,
};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, CursorIcon, Window, WindowId};

const DEFAULT_WINDOW_SIZE: LogicalSize<f64> = LogicalSize::new(1280.0, 720.0);
const START_CAMERA_POSITION: Vec3 = Vec3::new(12.0, 8.0, 16.0);
/// After a long pause (such as dragging the window), don't jump the camera
/// by the whole pause; treat the frame as at most this long.
const MAX_FRAME_SECONDS: f32 = 0.1;
/// Touchpads scroll in pixels; this many is one mouse wheel notch.
const PIXELS_PER_NOTCH: f32 = 40.0;
const GAME_FILE: &str = "game.axgame";
const USAGE: &str = "usage: axcenity-player [project-folder | game.axgame]\n\
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
    let argument = std::env::args_os().nth(1).map(PathBuf::from);
    let folder = match argument {
        Some(path) if path.is_file() => {
            let temporary = PackedProjectFolder::new()?;
            let bytes = fs::read(&path)
                .with_context(|| format!("could not read game file `{}`", path.display()))?;
            axcenity_project::unpack_game(&bytes, &temporary.path)
                .with_context(|| format!("could not unpack game file `{}`", path.display()))?;
            temporary
        }
        Some(path) => PackedProjectFolder::from_folder(path),
        None => {
            let player = std::env::current_exe().context("could not locate the player program")?;
            let bundled = player
                .parent()
                .context("the player program has no parent folder")?
                .join(GAME_FILE);
            if !bundled.is_file() {
                bail!("no project folder or `{GAME_FILE}` found next to the player\n\n{USAGE}");
            }
            let temporary = PackedProjectFolder::new()?;
            let bytes = fs::read(&bundled)
                .with_context(|| format!("could not read game file `{}`", bundled.display()))?;
            axcenity_project::unpack_game(&bytes, &temporary.path)
                .with_context(|| format!("could not unpack game file `{}`", bundled.display()))?;
            temporary
        }
    };
    let project = axcenity_project::load(&folder.path)
        .with_context(|| format!("could not open the project in `{}`", folder.path.display()))?;
    if let Some(from) = project.migrated_from {
        let kept = project
            .migration_backup
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| axcenity_project::PREVIOUS_FORMAT_DIR.to_owned());
        eprintln!(
            "Upgraded \"{}\" from project format {from} to {}. The previous files are kept in `{kept}`.",
            project.manifest.name,
            axcenity_project::FORMAT_VERSION,
        );
    }
    log::info!("loaded project \"{}\"", project.manifest.name);
    let show_splash = project.manifest.show_splash;
    let loading = project.manifest.loading_screen.clone();
    // A damaged file only takes itself out: say which, and run the rest.
    for problem in project.problems {
        eprintln!("error: {:#}", anyhow::Error::new(problem));
    }
    let mut stylesheets = Vec::new();
    for file in &project.stylesheets {
        let (stylesheet, warnings) = axcenity_ui::parse_stylesheet(&file.path, &file.source);
        stylesheets.push(stylesheet);
        for warning in warnings {
            log::warn!("{}: {}", warning.path, warning.message);
        }
    }
    let mut world = project.world;
    // The game's own loading screen is up before any script runs, so
    // scripts see it enabled; it takes the splash's place.
    let loading_gui = LoadingGui::start(
        &mut world,
        &loading.screen_gui,
        loading.min_seconds,
        loading.wait_for_script,
    );
    if !loading.screen_gui.is_empty() && loading_gui.is_none() {
        eprintln!(
            "warning: the loading screen \"{}\" is not a ScreenGui in the Workspace",
            loading.screen_gui
        );
    }
    let show_splash = show_splash && loading_gui.is_none();
    let mut splash = splash::Splash::new();
    if loading.wait_for_script {
        splash.hold();
    }
    let assets = Assets::new(&project.folder);
    let mut scripts = ScriptRuntime::new();
    scripts.set_assets(assets.clone());
    // Saves go in the player's data for this game, never the game's folder.
    let data_folder = axcenity_save::game_folder(&project.manifest.name);
    scripts.set_save_folder(data_folder.clone());
    // LocalScripts see the saved graphics settings from the start. The
    // GPU's own capabilities replace these guesses once the window opens.
    let unknown_gpu = Capabilities::default();
    let saved_graphics = data_folder
        .as_deref()
        .and_then(graphics::load)
        .unwrap_or_else(|| GraphicsSettings::default_for(&unknown_gpu));
    scripts.set_graphics(&saved_graphics, &unknown_gpu);
    // Animators start in their default states (with their parameters at
    // their defaults) before any script runs, so scripts can read them.
    let mut animators = Animators::new();
    animators.update(&mut world, &assets, 0.0);
    start_scripts(&mut scripts, &mut world, &project.scripts);
    let mut audio = GameAudio::default();
    let camera = FlyCamera::looking_at(START_CAMERA_POSITION, Vec3::ZERO);
    apply_audio(
        &mut scripts,
        &mut world,
        &assets,
        &mut audio,
        listener_of(&camera),
        Vec::new(),
    );
    // Models load the first time a MeshPart needs one; physics and drawing
    // share them.
    let event_loop = EventLoop::new()?;
    // Games redraw continuously instead of waiting for input.
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = App {
        title: format!("{} - Axcenity Player", project.manifest.name),
        show_splash,
        splash,
        loading_gui,
        world,
        stylesheets,
        scripts,
        audio,
        physics: Physics::with_assets(assets.clone()),
        characters: Controllers::default(),
        agents: Agents::default(),
        particles: axcenity_scene::Particles::new(),
        terrain: axcenity_scene::TerrainCache::new(),
        animators,
        assets,
        window: None,
        renderer: None,
        data_folder,
        error: None,
        camera,
        keys: MoveKeys::default(),
        key_events: Vec::new(),
        held_keys: BTreeSet::new(),
        looking: false,
        look_dx: 0.0,
        look_dy: 0.0,
        zoom: 0.0,
        pointer: None,
        text_focus: None,
        ui_warnings: Vec::new(),
        ui_mouse_events: Vec::new(),
        ui_mouse_down: false,
        ui_pressed_button: None,
        ui: UiInteraction::default(),
        nav_keys: Vec::new(),
        gamepads: Gamepads::new(),
        ui_sounds: Vec::new(),
        cursor: UiCursor::Default,
        last_frame: None,
    };
    event_loop.run_app(&mut app)?;

    // Errors inside the event loop can't use `?`, so they're stored and reported here.
    match app.error {
        Some(err) => Err(err),
        None => Ok(()),
    }
}

struct PackedProjectFolder {
    path: PathBuf,
    remove_on_drop: bool,
}

impl PackedProjectFolder {
    fn new() -> Result<Self> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("system clock is before the Unix epoch")?
            .as_nanos();
        let unique = format!("axcenity-game-{}-{}", std::process::id(), timestamp);
        Ok(Self {
            path: std::env::temp_dir().join(unique),
            remove_on_drop: true,
        })
    }

    fn from_folder(path: PathBuf) -> Self {
        Self {
            path,
            remove_on_drop: false,
        }
    }
}

impl Drop for PackedProjectFolder {
    fn drop(&mut self) {
        if self.remove_on_drop
            && self.path.exists()
            && let Err(err) = fs::remove_dir_all(&self.path)
        {
            log::warn!(
                "could not remove temporary game folder `{}`: {err}",
                self.path.display()
            );
        }
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

/// Prints what scripts printed to stdout, and their errors to stderr, each
/// error followed by its stack trace, one indented call per line.
fn print_output(runtime: &mut ScriptRuntime) {
    for line in runtime.take_output() {
        match line.kind {
            OutputKind::Print => println!("{}", line.text),
            OutputKind::Error => {
                eprintln!("error: {}", line.text);
                for frame in &line.trace {
                    eprintln!("    {frame}");
                }
            }
        }
    }
}

fn listener_of(camera: &FlyCamera) -> axcenity_audio::Listener {
    let (position, orientation) = camera.view();
    listener_of_state(CameraState {
        position,
        orientation,
    })
}

fn listener_of_state(camera: CameraState) -> axcenity_audio::Listener {
    axcenity_audio::Listener::from_view(camera.position, camera.orientation)
}

/// Plays what scripts asked for, then `extra` (buttons' sounds), and fires
/// `Ended` for sounds that finished.
fn apply_audio(
    scripts: &mut ScriptRuntime,
    world: &mut World,
    assets: &Assets,
    audio: &mut GameAudio,
    listener: axcenity_audio::Listener,
    extra: Vec<SoundAction>,
) {
    let mut actions = scripts.take_sound_actions();
    actions.extend(extra);
    let update = audio.update(actions, world, assets, listener);
    for err in update.errors {
        log::error!("audio: {err}");
    }
    if !update.ended.is_empty() {
        scripts.fire_sounds_ended(world, &update.ended);
        print_output(scripts);
    }
}

/// Fires `PlayerSensed` on each agent that has just started seeing or
/// hearing the player character, with the sense's name.
fn fire_sensed(scripts: &mut ScriptRuntime, world: &mut World, agents: &mut Agents) {
    for sensed in agents.take_sensed() {
        let sense = sensed.sense.as_str().to_owned();
        scripts.fire_instance_event(world, sensed.agent, "PlayerSensed", &[sense]);
    }
}

/// Passes on what the UI did: `MouseEnter` and `MouseLeave` to
/// LocalScripts, and returns the buttons' hover and click sounds to play.
/// The frame prints what the handlers printed with the input step's output.
fn fire_ui_events(
    scripts: &mut ScriptRuntime,
    world: &mut World,
    events: Vec<UiEvent>,
) -> Vec<SoundAction> {
    let mut sounds = Vec::new();
    for event in events {
        match event {
            UiEvent::MouseEnter(id) => scripts.fire_ui_event(world, id, "MouseEnter"),
            UiEvent::MouseLeave(id) => scripts.fire_ui_event(world, id, "MouseLeave"),
            UiEvent::PlaySound(sound_id) => sounds.push(SoundAction::PlayOnce { sound_id }),
        }
    }
    sounds
}

/// Uses one key for the UI's focus (see [`UiInteraction::navigate`]): sets
/// what it changed in `world`, and fires `Activated` for a button it
/// confirmed, as a click does. Returns the TextBox to start typing in.
fn navigate_ui(
    ui: &mut UiInteraction,
    world: &mut World,
    scripts: &mut ScriptRuntime,
    key: NavKey,
) -> Option<InstanceId> {
    let done = ui.navigate(world, key);
    done.apply(world);
    if let Some(button) = done.activated {
        scripts.activate_ui_button(world, button);
    }
    done.text_box
}

/// The OS cursor for one the UI asks for (CSS `cursor`).
fn cursor_icon(cursor: UiCursor) -> CursorIcon {
    match cursor {
        UiCursor::Default => CursorIcon::Default,
        UiCursor::Pointer => CursorIcon::Pointer,
        UiCursor::Text => CursorIcon::Text,
        UiCursor::NotAllowed => CursorIcon::NotAllowed,
    }
}

/// Everything the player owns while running. `window` and `renderer` start as
/// `None` because winit only allows creating windows once the event loop has started.
struct App {
    title: String,
    show_splash: bool,
    splash: splash::Splash,
    /// The game's own loading screen, when the project names one.
    loading_gui: Option<LoadingGui>,
    world: World,
    stylesheets: Vec<axcenity_ui::Stylesheet>,
    /// Runs the scripts, and keeps the ones paused by `task.wait`.
    scripts: ScriptRuntime,
    /// Audio output and handles, created only when a script first plays a sound.
    audio: GameAudio,
    /// Simulates the unanchored parts.
    physics: Physics,
    /// Walks a character and owns the camera when one is present.
    characters: Controllers,
    /// Walks NavigationAgents along their paths.
    agents: Agents,
    /// Mouse look, in pixels, for the character camera. Right and down are positive.
    look_dx: f32,
    look_dy: f32,
    /// Mouse wheel notches since the last frame, toward the screen
    /// positive, for a third-person camera's zoom.
    zoom: f32,
    /// What the ParticleEmitters have given off.
    particles: axcenity_scene::Particles,
    /// Terrains' chunk meshes and weights, kept between frames.
    terrain: axcenity_scene::TerrainCache,
    /// Plays the Animators (state machines, AnimationTracks and played
    /// Animations), posing skeletons and setting keyed properties.
    animators: Animators,
    /// The project's models, for MeshParts.
    assets: Assets,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    /// The player's data for this game (saves and `graphics.json`), if
    /// there's a place for it.
    data_folder: Option<PathBuf>,
    error: Option<anyhow::Error>,
    camera: FlyCamera,
    keys: MoveKeys,
    /// Key presses and releases since the last frame, for LocalScripts.
    key_events: Vec<KeyEvent>,
    /// The keys held down, so they can all be let go when the window loses focus.
    held_keys: BTreeSet<axcenity_core::KeyCode>,
    /// True while the right mouse button is held, when moving the mouse turns the camera.
    looking: bool,
    /// The mouse position in physical pixels, for on-screen UI.
    pointer: Option<[f32; 2]>,
    /// A TextBox the player is typing in. Keys go to it instead of the game.
    text_focus: Option<InstanceId>,
    /// UI layout warnings already printed, so a PageLayout problem is not
    /// repeated every frame.
    ui_warnings: Vec<String>,
    /// Left-button transitions since the last frame.
    ui_mouse_events: Vec<bool>,
    ui_mouse_down: bool,
    ui_pressed_button: Option<InstanceId>,
    /// Hover and press states, CSS transitions and tooltips over time.
    ui: UiInteraction,
    /// Keys for the UI's focus since the last frame (Tab, the arrows,
    /// Enter, Escape), taken from the game: see [`UiInteraction::takes`].
    nav_keys: Vec<NavKey>,
    /// Game controllers, which move focus too.
    gamepads: Gamepads,
    /// Buttons' hover and click sounds, played with the frame's audio.
    ui_sounds: Vec<SoundAction>,
    /// The cursor last shown, so it's only set when it changes.
    cursor: UiCursor,
    /// When the previous frame was drawn, to measure how much time passed.
    last_frame: Option<Instant>,
}

impl App {
    fn fail(&mut self, event_loop: &ActiveEventLoop, err: anyhow::Error) {
        log::error!("{err:#}");
        self.error = Some(err);
        event_loop.exit();
    }

    /// The camera to draw and to hand to scripts: the follow camera when a
    /// third-person controller has a body, otherwise the fly camera.
    fn script_camera(&self) -> CameraState {
        match self.characters.view(&self.world) {
            Some(view) => CameraState {
                position: view.position,
                orientation: view.orientation,
            },
            None => camera_state(&self.camera),
        }
    }

    fn shown_camera(&self) -> axcenity_render::Camera {
        let state = self.script_camera();
        if self.characters.view(&self.world).is_some() {
            FlyCamera::from_view(state.position, state.orientation).to_render_camera()
        } else {
            self.camera.to_render_camera()
        }
    }

    fn process_ui_mouse_events(&mut self, size: (u32, u32)) {
        let camera = self.shown_camera();
        let events = std::mem::take(&mut self.ui_mouse_events);
        for pressed in events {
            if pressed == self.ui_mouse_down {
                continue;
            }
            let layout = axcenity_ui::layout_ui_interactive(
                &self.world,
                size.0,
                size.1,
                &self.stylesheets,
                UiInput {
                    pointer: self.pointer,
                    pressed: self.ui_pressed_button,
                    focused: self.text_focus,
                    on_top: self.loading_gui.as_ref().and_then(LoadingGui::on_top),
                },
                Some(&camera),
            );
            self.note_ui_warnings(&layout.warnings);
            let in_window = self
                .pointer
                .filter(|&[x, y]| x >= 0.0 && y >= 0.0 && x < size.0 as f32 && y < size.1 as f32);
            let hit = in_window.and_then(|[x, y]| layout.hit_button(x, y));
            let text_box = in_window.and_then(|[x, y]| layout.hit_text_box(x, y));
            if pressed {
                self.ui_mouse_down = true;
                // A press on a ScrollingFrame's bar drags the bar instead,
                // and one on a widget (or an open Dropdown's list) changes it.
                if let Some(pointer) = in_window
                    && self.ui.press(&self.world, pointer).apply(&mut self.world)
                {
                    self.ui_pressed_button = None;
                    continue;
                }
                self.ui_pressed_button = hit;
                match text_box {
                    Some(id) => self.focus_text(id),
                    None => {
                        if let Some(id) = self.text_focus {
                            self.blur_text(id);
                        }
                    }
                }
            } else {
                if let Some(button) = self.ui_pressed_button
                    && self.ui_pressed_button == hit
                    && self.ui.activate(&self.world, button)
                {
                    self.scripts.activate_ui_button(&mut self.world, button);
                    print_output(&mut self.scripts);
                }
                self.ui_pressed_button = None;
                self.ui_mouse_down = false;
                // Let go over a Toggle flips it, over a Dropdown opens its
                // list, and over one of its options picks it.
                let _ = self
                    .ui
                    .release(&self.world, in_window)
                    .apply(&mut self.world);
            }
        }
        // A held press scrolls a ScrollingFrame by its bar, or by its content
        // once it has moved far enough; then the button it began on won't fire.
        if self.ui_mouse_down
            && let Some(pointer) = self.pointer
            && self.ui.drag(&self.world, pointer).apply(&mut self.world)
        {
            self.ui_pressed_button = None;
        }
    }

    /// Moves focus by this frame's keys and controllers, before the
    /// game's input. Confirm on a button fires `Activated` as a click
    /// does, and on a text box starts typing in it. While typing, a
    /// controller's Confirm or Back finishes.
    fn process_ui_navigation(&mut self, seconds: f32) {
        let mut keys = std::mem::take(&mut self.nav_keys);
        keys.extend(self.gamepads.poll(seconds));
        for key in keys {
            if let Some(id) = self.text_focus {
                if matches!(key, NavKey::Confirm | NavKey::Back) {
                    self.blur_text(id);
                }
                continue;
            }
            let typing = navigate_ui(&mut self.ui, &mut self.world, &mut self.scripts, key);
            print_output(&mut self.scripts);
            if let Some(id) = typing {
                self.focus_text(id);
            }
        }
    }

    fn focus_text(&mut self, id: InstanceId) {
        if self.text_focus == Some(id) {
            return;
        }
        if let Some(previous) = self.text_focus.take() {
            self.scripts.release_text_box(&mut self.world, previous);
            print_output(&mut self.scripts);
        }
        self.text_focus = Some(id);
        self.keys = MoveKeys::default();
        for key in std::mem::take(&mut self.held_keys) {
            self.key_events.push(KeyEvent {
                key,
                pressed: false,
            });
        }
    }

    fn blur_text(&mut self, id: InstanceId) {
        self.text_focus = None;
        self.scripts.release_text_box(&mut self.world, id);
        print_output(&mut self.scripts);
    }

    fn type_text(&mut self, event: &winit::event::KeyEvent) {
        if event.state != ElementState::Pressed {
            return;
        }
        let Some(id) = self.text_focus else {
            return;
        };
        let PhysicalKey::Code(code) = event.physical_key else {
            return;
        };
        if code == KeyCode::Escape {
            self.blur_text(id);
            return;
        }
        let command = match code {
            KeyCode::Enter | KeyCode::NumpadEnter => Some(axcenity_ui::TextCommand::Submit),
            KeyCode::Backspace => Some(axcenity_ui::TextCommand::Backspace),
            _ => event
                .text
                .as_ref()
                .map(|text| axcenity_ui::TextCommand::Insert(text.to_string())),
        };
        let Some(command) = command else {
            return;
        };
        if axcenity_ui::edit_text_box(&mut self.world, id, command)
            == axcenity_ui::TextOutcome::Finished
        {
            self.blur_text(id);
        }
    }

    fn note_ui_warnings(&mut self, warnings: &[String]) {
        for warning in warnings {
            if !self.ui_warnings.iter().any(|seen| seen == warning) {
                log::warn!("{warning}");
            }
        }
        self.ui_warnings = warnings.to_vec();
    }
}

/// Seconds since `last_frame` (0 for the first frame), and records now as the latest frame.
///
/// This takes only the `last_frame` field rather than all of `App`, so it can
/// be called while other fields of `App` are borrowed.
/// Draws with `settings` from the next frame, logging anything the GPU
/// can't do.
fn set_graphics(renderer: &mut Renderer, settings: &GraphicsSettings) {
    renderer.set_graphics(settings);
    for note in &renderer.graphics().notes {
        log::info!("graphics: {note}");
    }
}

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

/// What a key does to the UI's focus: Tab and Shift+Tab step through it,
/// the arrows move it, Enter uses the focused element and Escape lets go.
fn nav_key(key: KeyCode, shift: bool) -> Option<NavKey> {
    Some(match key {
        KeyCode::Tab if shift => NavKey::Previous,
        KeyCode::Tab => NavKey::Next,
        KeyCode::ArrowUp => NavKey::Up,
        KeyCode::ArrowDown => NavKey::Down,
        KeyCode::ArrowLeft => NavKey::Left,
        KeyCode::ArrowRight => NavKey::Right,
        KeyCode::Enter | KeyCode::NumpadEnter => NavKey::Confirm,
        KeyCode::Escape => NavKey::Back,
        _ => return None,
    })
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

/// Mouse wheel notches in a scroll, away from the player (toward the
/// screen) positive. Touchpads report pixels instead of notches.
/// How far the wheel scrolls a ScrollingFrame, in pixels across and down
/// (down moves its content up).
fn wheel_pixels(delta: MouseScrollDelta) -> [f32; 2] {
    match delta {
        MouseScrollDelta::LineDelta(x, y) => [-x * PIXELS_PER_NOTCH, -y * PIXELS_PER_NOTCH],
        MouseScrollDelta::PixelDelta(position) => [-position.x as f32, -position.y as f32],
    }
}

fn wheel_notches(delta: MouseScrollDelta) -> f32 {
    match delta {
        MouseScrollDelta::LineDelta(_, y) => y,
        MouseScrollDelta::PixelDelta(position) => position.y as f32 / PIXELS_PER_NOTCH,
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
            .with_inner_size(DEFAULT_WINDOW_SIZE)
            .with_window_icon(icon::window_icon());
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(err) => return self.fail(event_loop, err.into()),
        };

        // GPU setup is `async`; `pollster::block_on` simply waits for it to finish.
        let size = window.inner_size();
        match pollster::block_on(Renderer::new(window.clone(), size.width, size.height)) {
            Ok(mut renderer) => {
                let folder = self.data_folder.as_deref();
                let capabilities = *renderer.capabilities();
                // A LocalScript may have changed them as it started.
                let settings = match self.scripts.take_graphics_change() {
                    Some(change) => graphics::changed(folder, change, &capabilities),
                    None => graphics::start(folder, &capabilities),
                };
                self.scripts.set_graphics(&settings, &capabilities);
                set_graphics(&mut renderer, &settings);
                self.renderer = Some(renderer);
            }
            Err(err) => return self.fail(event_loop, err),
        }

        window.request_redraw();
        self.window = Some(window);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if self.window.is_none() || self.renderer.is_none() {
            return;
        }

        if self.show_splash && self.splash.covers() {
            match &event {
                WindowEvent::KeyboardInput { event: key, .. }
                    if key.state == ElementState::Pressed && !key.repeat =>
                {
                    self.splash.skip();
                }
                WindowEvent::MouseInput {
                    state: ElementState::Pressed,
                    ..
                } => self.splash.skip(),
                _ => {}
            }
            match &event {
                WindowEvent::CloseRequested
                | WindowEvent::Resized(_)
                | WindowEvent::RedrawRequested
                | WindowEvent::Focused(_) => {}
                _ => return,
            }
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(renderer) = &mut self.renderer {
                    renderer.resize(size.width, size.height);
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.pointer = Some([position.x as f32, position.y as f32]);
            }
            WindowEvent::CursorLeft { .. } => self.pointer = None,
            WindowEvent::KeyboardInput { event, .. } => {
                if self.text_focus.is_some() {
                    self.type_text(&event);
                } else if let PhysicalKey::Code(key) = event.physical_key {
                    let pressed = event.state == ElementState::Pressed;
                    // A press that moves focus is the UI's, not the game's.
                    let shift = self.held_keys.contains(&axcenity_core::KeyCode::LeftShift)
                        || self.held_keys.contains(&axcenity_core::KeyCode::RightShift);
                    if pressed
                        && let Some(nav) = nav_key(key, shift)
                        && self.ui.takes(&self.world, nav)
                    {
                        self.nav_keys.push(nav);
                        return;
                    }
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
                button: MouseButton::Left,
                ..
            } => self.ui_mouse_events.push(state == ElementState::Pressed),
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Right,
                ..
            } => {
                self.looking = state == ElementState::Pressed;
                if let Some(window) = &self.window {
                    capture_mouse(window, self.looking);
                }
            }
            // Over a ScrollingFrame the wheel scrolls it; elsewhere it zooms.
            WindowEvent::MouseWheel { delta, .. } => {
                let scrolled = self.pointer.is_some_and(|pointer| {
                    self.ui
                        .wheel(&self.world, pointer, wheel_pixels(delta))
                        .apply(&mut self.world)
                });
                if !scrolled {
                    self.zoom += wheel_notches(delta);
                }
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
                if let Some(window) = &self.window {
                    capture_mouse(window, false);
                }
                self.ui_mouse_events.push(false);
            }
            // One frame, in the fixed order from docs/architecture/frame-loop.md.
            WindowEvent::RedrawRequested => {
                let seconds = frame_seconds(&mut self.last_frame);
                let size = self
                    .window
                    .as_ref()
                    .expect("checked at the start of the event")
                    .inner_size();
                self.process_ui_mouse_events((size.width, size.height));
                self.process_ui_navigation(seconds);
                // What the UI did last frame and this click: MouseEnter,
                // MouseLeave and buttons' sounds.
                let events = self.ui.take_events();
                let sounds = fire_ui_events(&mut self.scripts, &mut self.world, events);
                self.ui_sounds.extend(sounds);

                // 1. Input. A character controller moves first and owns
                //    the camera. Otherwise the fly camera moves, and
                //    LocalScripts see that.
                let motion = if self.text_focus.is_some() {
                    MoveInput::default()
                } else {
                    MoveInput {
                        forward: self.keys.forward,
                        back: self.keys.back,
                        left: self.keys.left,
                        right: self.keys.right,
                        jump: self.held_keys.contains(&axcenity_core::KeyCode::Space),
                        crouch: self.held_keys.contains(&axcenity_core::KeyCode::C),
                        look_dx: self.look_dx,
                        look_dy: self.look_dy,
                        zoom: self.zoom,
                    }
                };
                self.look_dx = 0.0;
                self.look_dy = 0.0;
                self.zoom = 0.0;
                self.characters
                    .step(&mut self.world, &self.assets, motion, seconds);
                // Then navigation agents, toward their destinations.
                self.agents.step(&mut self.world, &self.assets, seconds);
                for warning in self.agents.take_warnings() {
                    eprintln!("warning: {warning}");
                }
                fire_sensed(&mut self.scripts, &mut self.world, &mut self.agents);
                print_output(&mut self.scripts);
                if self.characters.view(&self.world).is_none() {
                    self.camera.update(&self.keys, seconds);
                }
                let seen = self.script_camera();
                self.scripts.set_camera(seen);
                let keys = std::mem::take(&mut self.key_events);
                self.scripts.input(&mut self.world, &keys);
                print_output(&mut self.scripts);

                // 2. Resume scripts whose wait is over (and due `task.delay`s).
                //    Game time is the sum of the (capped) frame times, so a
                //    long pause doesn't make every waiting script fire at once.
                self.scripts.step(&mut self.world, f64::from(seconds));
                print_output(&mut self.scripts);

                // 3. Hold characters and agents kinematic, then step physics at a
                //    fixed 60 Hz (0 to N steps this frame). Anchoring is
                //    after scripts, so a script cannot drop the character
                //    for this step. Other parts keep their gravity.
                self.characters.keep_kinematic(&mut self.world);
                self.agents.keep_kinematic(&mut self.world);
                self.physics.step(&mut self.world, f64::from(seconds));

                // 4. Fire `Touched` for parts that started touching during
                //    those steps (and `Changed` for the parts that moved),
                //    then `RunService.Heartbeat`. Handlers that wait carry
                //    on in step 2.
                let touches = self.physics.take_touches();
                self.scripts.fire_touched(&mut self.world, &touches);
                self.scripts.heartbeat(&mut self.world, f64::from(seconds));
                print_output(&mut self.scripts);
                let heard = listener_of_state(self.script_camera());
                apply_audio(
                    &mut self.scripts,
                    &mut self.world,
                    &self.assets,
                    &mut self.audio,
                    heard,
                    std::mem::take(&mut self.ui_sounds),
                );
                // Particles move on, after scripts and physics moved their parts.
                for (emitter, count) in self.scripts.take_particle_emits() {
                    self.particles.emit(&self.world, emitter, count);
                }
                self.particles.update(&self.world, seconds);
                // Animators move on and pose their models, after scripts
                // asked for states and physics moved the parts.
                let events =
                    self.animators
                        .update(&mut self.world, &self.assets, f64::from(seconds));
                for event in events {
                    let (animator, name, args) = event.script_event();
                    self.scripts
                        .fire_instance_event(&mut self.world, animator, name, &args);
                }
                print_output(&mut self.scripts);
                for error in self.animators.take_errors() {
                    eprintln!("error: {error}");
                }

                // The loading screen goes once its time is up and, when the
                // game waits for one, a LocalScript has said it's ready.
                let ready = self.scripts.take_loading_screen_removed();
                if ready {
                    self.splash.release();
                }
                if let Some(loading) = &mut self.loading_gui {
                    loading.advance(&mut self.world, seconds, ready);
                }

                // 5. Lay out ScreenGuis at this window's current pixel size.
                let ui_size = (size.width, size.height);

                // 6. Build and render. A character controller owns the camera;
                //    otherwise a LocalScript's move is applied to the fly camera.
                if self.characters.view(&self.world).is_none()
                    && let Some(view) = self.scripts.take_camera_move()
                {
                    self.camera.set_view(view.position, view.orientation);
                } else {
                    let _ = self.scripts.take_camera_move();
                }
                // A LocalScript's graphics change (`UserGameSettings`) is
                // saved and drawn from this frame.
                if let Some(change) = self.scripts.take_graphics_change()
                    && let Some(renderer) = &mut self.renderer
                {
                    let capabilities = *renderer.capabilities();
                    let settings =
                        graphics::changed(self.data_folder.as_deref(), change, &capabilities);
                    set_graphics(renderer, &settings);
                }
                let render_camera = self.shown_camera();
                let heard = listener_of_state(self.script_camera());
                self.audio.set_listener(heard);

                let (mut draw_list, ui_layout) =
                    axcenity_scene::build_draw_list_with_ui_interaction(
                        &self.world,
                        &self.assets,
                        render_camera,
                        ui_size,
                        &self.stylesheets,
                        UiFrame {
                            interaction: &mut self.ui,
                            input: UiInput {
                                pointer: self.pointer,
                                pressed: self.ui_pressed_button,
                                focused: self.text_focus,
                                on_top: self.loading_gui.as_ref().and_then(LoadingGui::on_top),
                            },
                            seconds,
                        },
                        self.animators.poses(),
                        &mut self.terrain,
                    );
                self.note_ui_warnings(&ui_layout.warnings);
                if ui_layout.cursor != self.cursor
                    && let Some(window) = &self.window
                {
                    self.cursor = ui_layout.cursor;
                    window.set_cursor(cursor_icon(self.cursor));
                }
                draw_list.sprites = self.particles.sprites(&self.world, &self.assets);
                if self.show_splash {
                    // The first drawn frame starts the splash clock.
                    self.splash.tick();
                    draw_list.ui.extend(self.splash.items(
                        size.width as f32,
                        size.height as f32,
                        "Made with Axcenity",
                    ));
                }
                // Models that couldn't be loaded, by physics or for drawing.
                for error in self.assets.take_errors() {
                    eprintln!("error: {error}");
                }
                // `|| ...` is a closure: a small function the renderer calls at the right moment.
                let window = self
                    .window
                    .as_ref()
                    .expect("checked at the start of the event")
                    .clone();
                let rendered = self
                    .renderer
                    .as_mut()
                    .expect("checked")
                    .render(&draw_list, || window.pre_present_notify());
                if let Err(err) = rendered {
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
            if self.characters.view(&self.world).is_some() {
                self.look_dx += dx as f32;
                self.look_dy += dy as f32;
            } else {
                self.camera.look(dx as f32, dy as f32);
            }
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
mod tests;

#[cfg(test)]
mod graphics_tests;
