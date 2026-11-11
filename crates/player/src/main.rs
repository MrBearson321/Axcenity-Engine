//! Axcenity Player: the runtime that ships with every exported game.
//!
//! Loads a project folder, starts its Scripts and then its LocalScripts, and
//! runs the frame loop: move a third-person character when the world has
//! one, pass key presses to LocalScripts, resume scripts whose `task.wait`
//! is over (and deliver RemoteEvent messages), step physics, fire `Touched`
//! for parts that started touching, then draw the Workspace. The character's
//! camera follows it; otherwise a fly camera looks around. The keyboard and
//! game controllers can move focus through menus (`axcenity_ui::NavKey`).

mod dev_console;
mod graphics;
mod icon;
mod input;
mod logs;
mod preload;
mod splash;
mod watch;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::ops;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use axcenity_anim::Animators;
use axcenity_assets::Assets;
use axcenity_audio::{GameAudio, SoundAction};
use axcenity_character::{Controllers, MoveInput};
use axcenity_core::bench::BenchRun;
use axcenity_core::input::{InputEvent, InputKind};
use axcenity_core::{ClassName, InstanceId, MouseBehavior, UserInputType, World};
use axcenity_nav::Agents;
use axcenity_net::{GameInfo, Network, ScriptCode};
use axcenity_physics::Physics;
use axcenity_project::ScriptFile;
use axcenity_render::glam::Vec3;
use axcenity_render::{Capabilities, GraphicsSettings, Renderer};
use axcenity_scene::{FlyCamera, MoveKeys, UiFrame};
use axcenity_script::{CameraState, CameraView, GameCamera, OutputKind, ScriptRuntime};
use axcenity_sim::{FrameCtx, HostIo, Hosts, Schedule, Sim};
use axcenity_ui::{
    LoadingGui, LogLevel, LogSide, NavKey, UiCursor, UiEvent, UiInput, UiInteraction,
};
use input::{CursorLock, PlayerInput, script_key};
use watch::NetworkStart;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{
    DeviceEvent, DeviceId, ElementState, MouseButton, MouseScrollDelta, WindowEvent,
};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, CursorIcon, Window, WindowId};

const DEFAULT_WINDOW_SIZE: LogicalSize<f64> = LogicalSize::new(1280.0, 720.0);

/// `AXCENITY_WINDOW_SIZE`'s `<width>x<height>`, if it's two whole numbers
/// above zero.
fn window_size_override(value: Option<&str>) -> Option<(u32, u32)> {
    let (width, height) = value?.trim().split_once(['x', 'X'])?;
    let (width, height) = (width.trim().parse().ok()?, height.trim().parse().ok()?);
    (width > 0 && height > 0).then_some((width, height))
}

#[cfg(test)]
mod window_size_tests {
    use super::window_size_override;

    #[test]
    fn reads_width_by_height() {
        assert_eq!(window_size_override(Some("1920x1080")), Some((1920, 1080)));
        assert_eq!(window_size_override(Some(" 800 X 600 ")), Some((800, 600)));
        assert_eq!(window_size_override(Some("0x600")), None);
        assert_eq!(window_size_override(Some("big")), None);
        assert_eq!(window_size_override(None), None);
    }
}
const START_CAMERA_POSITION: Vec3 = Vec3::new(12.0, 8.0, 16.0);
/// After a long pause (such as dragging the window), don't jump the camera
/// by the whole pause; treat the frame as at most this long.
const MAX_FRAME_SECONDS: f32 = 0.1;
/// Longest a `RequestStreamAroundAsync` waits for its region to be on the GPU.
const REGION_WAIT_SECONDS: f64 = 5.0;
/// Touchpads scroll in pixels; this many is one mouse wheel notch.
const PIXELS_PER_NOTCH: f32 = 40.0;
const GAME_FILE: &str = "game.axgame";
const USAGE: &str = "usage: axcenity-player [--watch] [--dev] [--host [port] | --join address:port] \
                     [--password text] [--bench frames] [--size WxH] [--quality preset] \
                     [project-folder | game.axgame]  (or --version [--json])\n\
                     example: cargo run -p axcenity-player -- examples/hello-part\n\
                     --watch reloads a project folder's changed assets while it plays\n\
                     --dev turns on the F9 Developer Console in a built game\n\
                     --host hosts the game (port 47800 unless given); --join joins one\n\
                     --bench measures that many frames' steps, prints them and quits\n\
                     --size sets the window's size in pixels, such as 2560x1440\n\
                     --quality draws with that preset (Low, Medium, High, Ultra) for this run only";

fn main() -> Result<()> {
    // `--version` (and `--version --json`): this build's version, channel,
    // commit and toolchain, then exit.
    let all: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    if let Some(text) = axcenity_release::version_flag("axcenity-player", &all) {
        println!("{text}");
        return Ok(());
    }
    // `--bench` reports its launch times (first frame, assets loaded) from here.
    let launched = Instant::now();
    // Show warnings and errors by default. `wgpu_hal` relays harmless graphics
    // driver chatter as warnings, so only its errors are shown. Set RUST_LOG to
    // override (for example RUST_LOG=info).
    // They also go in the game's log file (Phase 71), which a panic is
    // written to before the game exits.
    logs::install_logger(
        env_logger::Builder::from_env(
            env_logger::Env::default().default_filter_or("warn,wgpu_hal=error"),
        )
        .build(),
    );
    logs::install_panic_hook(logs::tell_on_stderr);

    // `args_os` instead of `args` so folder names that aren't valid Unicode still work.
    let arguments = watch::Arguments::parse(std::env::args_os().skip(1))?;
    let watcher = arguments.watched_folder().map(watch::start);
    // A project folder is a development run; a `.axgame` file (or one
    // bundled next to the player) is a built game.
    let game_path = arguments.game.clone();
    let folder = match arguments.game {
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
            let beside = player
                .parent()
                .context("the player program has no parent folder")?;
            // Beside the player (Windows, Linux), or in a macOS app's
            // `Contents/Resources` (Build Game keeps `Contents/MacOS` for code).
            let bundled = [
                beside.join(GAME_FILE),
                beside.join("../Resources").join(GAME_FILE),
            ]
            .into_iter()
            .find(|path| path.is_file())
            .unwrap_or_else(|| beside.join(GAME_FILE));
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
    // Saves and logs go in the player's data for this game, never the
    // game's folder.
    let data_folder = axcenity_save::game_folder(&project.manifest.name);
    if let Some(data) = &data_folder {
        match logs::start(&data.join("logs")) {
            Ok(path) => log::info!("logging to `{}`", path.display()),
            Err(err) => log::warn!("could not start the game's log: {err}"),
        }
    }
    // The F9 Developer Console: in a development run, else only when the
    // project turns it on or the player is started with `--dev`.
    let console_allowed = dev_console::console_allowed(
        game_path.as_deref(),
        arguments.dev,
        project.manifest.developer_console,
    );
    if let Some(from) = project.migrated_from {
        let kept = project
            .migration_backup
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| axcenity_project::PREVIOUS_FORMAT_DIR.to_owned());
        logs::line(
            LogLevel::Warning,
            None,
            &format!(
                "Upgraded \"{}\" from project format {from} to {}. The previous files are kept in `{kept}`.",
                project.manifest.name,
                axcenity_project::FORMAT_VERSION,
            ),
        );
    }
    log::info!("loaded project \"{}\"", project.manifest.name);
    let show_splash = project.manifest.show_splash;
    let loading = project.manifest.loading_screen.clone();
    // Every level, read before anything changes the start level.
    let mut problems = project.problems;
    let start_level = project.manifest.start_level.clone();
    let levels = game_levels(&project.folder, &start_level, &project.world, &mut problems);
    // A damaged file only takes itself out: say which, and run the rest.
    for problem in problems {
        logs::line(
            LogLevel::Error,
            None,
            &format!("error: {:#}", anyhow::Error::new(problem)),
        );
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
        logs::line(
            LogLevel::Warning,
            None,
            &format!(
                "warning: the loading screen \"{}\" is not a ScreenGui in the Workspace",
                loading.screen_gui
            ),
        );
    }
    let show_splash = show_splash && loading_gui.is_none();
    let mut splash = splash::Splash::new();
    if loading.wait_for_script {
        splash.hold();
    }
    let assets = Assets::new(&project.folder);
    // The Workspace's models and textures load on other threads while the
    // scripts start and the window opens; the splash or loading screen
    // stays up until they're in (roadmap I11).
    let preload = preload::Preload::start(&world, &assets);
    let mut scripts = axcenity_sim::script_runtime();
    scripts.set_assets(assets.clone());
    scripts.set_save_folder(data_folder.clone());
    // Who plays, the same each time on this install (Phase 64).
    scripts.set_user_id(axcenity_save::user_id() as f64);
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
    let mut animators = Animators::with_ground(axcenity_character::foot_ground);
    animators.update(&mut world, &assets, 0.0);
    // The character each player spawns as, before scripts look for it.
    if let Err(err) = axcenity_project::load_starter_character(&project.folder, &mut world) {
        logs::line(LogLevel::Warning, None, &format!("warning: {err}"));
    }
    // The project's actions, before any LocalScript asks about them.
    scripts.set_input_map(project.input_map.clone());
    // Scripts inside objects in levels LevelService loads later, and in
    // the StarterCharacter each player spawns as (G2).
    let worlds = levels.values().chain([&world]);
    for (path, source) in axcenity_project::level_code(&project.folder, worlds) {
        scripts.know_code(&path, &source);
    }
    scripts.set_levels(&start_level, &start_level, levels);
    start_scripts(&mut scripts, &mut world, &project.scripts);
    // Hosting or joining from the command line, once the scripts listen
    // for players (Phase 63).
    let mut network = Network::new(GameInfo {
        name: project.manifest.name.clone(),
        version: project.manifest.version.clone(),
    });
    // Joined, the LocalScripts start again on the server's world.
    network.set_client_scripts(client_scripts(&world, &project.scripts));
    match arguments.network {
        Some(NetworkStart::Host(port)) => {
            let password = arguments.password.clone();
            if let Err(reason) = network.host(&mut world, &mut scripts, port, password, true) {
                bail!("{reason}");
            }
        }
        Some(NetworkStart::Join(address)) => {
            let password = arguments.password.clone();
            if let Err(reason) = network.join(&mut world, &mut scripts, &address, password) {
                bail!("{reason}");
            }
        }
        None => {}
    }
    print_network_log(&mut network);
    print_output(&mut scripts);
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

    let mut app = App::new(world, scripts, stylesheets, assets, animators, camera);
    app.title = format!("{} - Axcenity Player", project.manifest.name);
    app.show_splash = show_splash;
    app.splash = splash;
    app.loading_gui = loading_gui;
    app.loading_screen = loading;
    app.audio = audio;
    app.data_folder = data_folder;
    app.watcher = watcher;
    app.network = network;
    app.console_allowed = console_allowed;
    app.bench = arguments.bench.map(|frames| {
        let mut run = BenchRun::new(BenchRun::DEFAULT_WARM_UP, frames);
        run.wait_stage(dev_console::PRESENT_WAIT);
        run
    });
    if app.bench.is_some() && std::env::var_os("AXCENITY_BENCH_TIMELINE").is_some() {
        app.bench_counts = Some(
            "camera x,camera y,camera z,look x,look y,look z,items drawn,triangles,draw calls,shadow casters\n"
                .to_owned(),
        );
    }
    app.window_size = arguments.size;
    app.quality = arguments.quality;
    app.bench_scene = axcenity_core::bench::scene_name(&project.folder);
    app.launched = launched;
    app.preload = Some(preload);
    event_loop.run_app(&mut app)?;
    // Tell the other players this one is gone.
    app.network
        .disconnect(&mut app.sim.world, &mut app.sim.scripts);

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

/// The game's levels for `LevelService`, by name: the start level (as
/// `start_world` loaded) and every one in `levels/`. One that can't be
/// read is left out and added to `problems`.
fn game_levels(
    folder: &std::path::Path,
    start: &str,
    start_world: &World,
    problems: &mut Vec<axcenity_project::LoadError>,
) -> BTreeMap<String, World> {
    let mut levels: BTreeMap<String, World> = axcenity_project::load_levels(folder, problems)
        .into_iter()
        .collect();
    levels.insert(start.to_owned(), start_world.clone());
    levels
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

/// Prints who joined and left, and why a connection failed.
/// The game's ModuleScripts and LocalScripts, in the order they start, for
/// a device that joins a server elsewhere (Phase 64).
fn client_scripts(world: &World, scripts: &[ScriptFile]) -> Vec<ScriptCode> {
    scripts
        .iter()
        .filter(|script| {
            world.get(script.instance).is_some_and(|instance| {
                matches!(
                    instance.class(),
                    ClassName::ModuleScript | ClassName::LocalScript
                )
            })
        })
        .map(|script| ScriptCode {
            instance: script.instance,
            path: script.path.clone(),
            source: script.source.clone(),
        })
        .collect()
}

fn print_network_log(network: &mut Network) {
    for line in network.take_log() {
        let level = if line.starts_with("warning:") {
            LogLevel::Warning
        } else {
            LogLevel::Info
        };
        logs::line(level, None, &line);
    }
}

/// Prints what scripts printed to stdout, and their errors to stderr, each
/// error followed by its stack trace, one indented call per line.
fn print_output(runtime: &mut ScriptRuntime) {
    for line in runtime.take_output() {
        let side = line.side.map(|side| match side {
            axcenity_script::Side::Server => LogSide::Server,
            axcenity_script::Side::Client => LogSide::Client,
        });
        match line.kind {
            OutputKind::Print => logs::line(LogLevel::Info, side, &line.text),
            OutputKind::Warning => {
                logs::line(LogLevel::Warning, side, &format!("warning: {}", line.text));
            }
            OutputKind::Error => {
                logs::line(LogLevel::Error, side, &format!("error: {}", line.text));
                for frame in &line.trace {
                    logs::line(LogLevel::Error, side, &format!("    {frame}"));
                }
            }
        }
    }
}

/// The moving parts to draw between physics steps this frame (roadmap G3).
fn blends_to_draw(sim: &axcenity_sim::Sim) -> axcenity_scene::Blends {
    let moving = sim.blends();
    let mut blends = axcenity_scene::Blends::new(sim.physics.blend_alpha());
    blends.reserve(moving.len());
    for blend in moving {
        blends.add(
            blend.part,
            (blend.from_position, blend.from_orientation),
            (blend.to_position, blend.to_orientation),
        );
    }
    blends
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

fn listener_of_view(view: CameraView) -> axcenity_audio::Listener {
    axcenity_audio::Listener::from_view(view.position, view.orientation)
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

/// Fires `Seated` on each controller whose character just sat down or
/// stood up.
#[cfg(test)]
fn fire_seated(scripts: &mut ScriptRuntime, world: &mut World, characters: &mut Controllers) {
    let changes: Vec<_> = characters
        .take_seat_changes()
        .into_iter()
        .map(|change| (change.controller, change.seated, change.seat))
        .collect();
    if !changes.is_empty() {
        scripts.fire_seated(world, &changes);
    }
}

/// The ProximityPrompts `shown` (Phase 44), for the UI to draw.
fn prompt_views(shown: &[axcenity_script::ShownPrompt]) -> Vec<axcenity_ui::PromptView> {
    shown
        .iter()
        .map(|shown| axcenity_ui::PromptView {
            prompt: shown.prompt,
            position: [shown.position.x, shown.position.y, shown.position.z],
            progress: shown.progress,
            key: shown.key.clone(),
        })
        .collect()
}

/// True while the player character has a Tool in hand, so the wheel goes
/// through the hotbar instead of zooming (Phase 45).
fn tool_in_hand(world: &World) -> bool {
    axcenity_gameplay::player_inventory(world)
        .and_then(|inventory| axcenity_core::tool::equipped(world, inventory))
        .is_some()
}

/// Fires `Landed` on each controller whose character just landed, after
/// fall damage (Phase 43).
#[cfg(test)]
fn fire_landed(scripts: &mut ScriptRuntime, world: &mut World, characters: &mut Controllers) {
    let landings = characters.take_landings();
    if !landings.is_empty() {
        axcenity_gameplay::fire_landed(scripts, world, &landings);
    }
}

/// Fires `PlayerSensed` on each agent that has just started seeing or
/// hearing the player character, with the sense's name.
#[cfg(test)]
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
    /// How the loading screen goes, for each level loaded (Phase 48).
    loading_screen: axcenity_project::LoadingScreen,
    /// The world, scripts, physics, characters, agents, animators and assets.
    sim: Sim,
    stylesheets: Vec<axcenity_ui::Stylesheet>,
    /// Audio output and handles, created only when a script first plays a sound.
    audio: GameAudio,
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
    /// `RequestStreamAroundAsync` calls waiting for their region: each
    /// request, and how many seconds it may still wait (Phase 80).
    region_waits: Vec<(u64, f64)>,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    /// The player's data for this game (saves and `graphics.json`), if
    /// there's a place for it.
    data_folder: Option<PathBuf>,
    error: Option<anyhow::Error>,
    camera: FlyCamera,
    /// What scripts did to the camera last frame (`ScriptRuntime::step_camera`).
    game_camera: GameCamera,
    keys: MoveKeys,
    /// Input since the last frame for LocalScripts, the game's held input
    /// and the game controllers.
    input: PlayerInput,
    /// The keyboard keys held down, so they can all be let go when the
    /// window loses focus.
    held_keys: BTreeSet<axcenity_core::KeyCode>,
    /// `UserInputService.MouseBehavior` and `MouseIconEnabled` as last
    /// applied to the cursor.
    mouse: (MouseBehavior, bool),
    /// False while another window has the keyboard.
    focused: bool,
    /// True while the right mouse button is held, when moving the mouse turns the camera.
    looking: bool,
    /// The mouse position in physical pixels, for on-screen UI.
    pointer: Option<[f32; 2]>,
    /// A TextBox the player is typing in. Keys go to it instead of the game.
    text_focus: Option<InstanceId>,
    /// The chat line being typed, while the chat is open (`/`, Phase 67).
    chat_typing: Option<String>,
    /// UI layout warnings already printed, so a PageLayout problem is not
    /// repeated every frame.
    ui_warnings: Vec<String>,
    /// Left-button transitions since the last frame.
    ui_mouse_events: Vec<bool>,
    ui_mouse_down: bool,
    /// True while the held left button is the UI's (it pressed a button,
    /// a text box or a widget), so its release is too.
    ui_mouse_taken: bool,
    ui_pressed_button: Option<InstanceId>,
    /// Hover and press states, CSS transitions and tooltips over time.
    ui: UiInteraction,
    /// Keys for the UI's focus since the last frame (Tab, the arrows,
    /// Enter, Escape), taken from the game: see [`UiInteraction::takes`].
    nav_keys: Vec<NavKey>,
    /// Buttons' hover and click sounds, played with the frame's audio.
    ui_sounds: Vec<SoundAction>,
    /// The cursor last shown, so it's only set when it changes.
    cursor: CursorIcon,
    /// The player's frame.
    schedule: Schedule,
    /// The window's size in this frame.
    frame_size: winit::dpi::PhysicalSize<u32>,
    /// True while this frame's Workspace assets are still loading.
    frame_loading_assets: bool,
    /// The list and UI layout the draw steps are building, for rendering.
    frame_draw: Option<axcenity_render::DrawList>,
    frame_ui_layout: Option<axcenity_ui::UiLayout>,
    /// When the previous frame was drawn, to measure how much time passed.
    last_frame: Option<Instant>,
    /// Seconds of frames run, which moving surface shaders read.
    shader_time: f32,
    /// Watches the project's assets with `--watch`; `None` otherwise, and
    /// always for a built game.
    watcher: Option<axcenity_project::Watcher>,
    /// Hosting or joining (`--host`, `--join`, `NetworkService`), or
    /// offline (Phase 63).
    network: Network,
    /// This device joined a server elsewhere and follows its world
    /// (Phase 64): the server moves the parts, characters and agents, and
    /// only LocalScripts run here.
    following: bool,
    /// The F9 Developer Console (Phase 71), and whether this run has it
    /// (`dev_console::console_allowed`).
    console: axcenity_ui::DevConsole,
    console_allowed: bool,
    /// What the console's commands changed.
    dev: dev_console::DevSettings,
    /// The frame rate and steps' times, for the console's Stats tab.
    frame_stats: dev_console::FrameStats,
    /// `--bench`: the frames measured so far, and the scene's name.
    bench: Option<BenchRun>,
    bench_scene: String,
    /// `AXCENITY_BENCH_TIMELINE`: each frame's camera and drawing counts,
    /// written beside the timeline when the run ends.
    bench_counts: Option<String>,
    /// `--size`: the window's size in physical pixels.
    window_size: Option<(u32, u32)>,
    /// `--quality`: the preset drawn with for this run, whatever
    /// `graphics.json` or a LocalScript says.
    quality: Option<axcenity_render::Quality>,
    /// When the player started, for `--bench`'s launch times.
    launched: Instant,
    /// The Workspace's assets being read ahead; `None` when nothing was
    /// (tests), which counts as loaded.
    preload: Option<preload::Preload>,
}

impl App {
    /// A player for `world`, whose scripts have started, before its window
    /// opens: no splash, loading screen, sound yet, save folder or watcher.
    fn new(
        world: World,
        scripts: ScriptRuntime,
        stylesheets: Vec<axcenity_ui::Stylesheet>,
        assets: Assets,
        animators: Animators,
        camera: FlyCamera,
    ) -> Self {
        let map = scripts.input_map();
        App {
            title: String::new(),
            show_splash: false,
            splash: splash::Splash::new(),
            loading_gui: None,
            loading_screen: axcenity_project::LoadingScreen::default(),
            sim: Sim {
                animators,
                ..Sim::new(world, scripts, assets)
            },
            stylesheets,
            audio: GameAudio::default(),
            particles: axcenity_scene::Particles::new(),
            terrain: axcenity_scene::TerrainCache::new(),
            region_waits: Vec::new(),
            window: None,
            renderer: None,
            data_folder: None,
            error: None,
            camera,
            game_camera: GameCamera::default(),
            keys: MoveKeys::default(),
            input: PlayerInput::new(map),
            held_keys: BTreeSet::new(),
            mouse: (MouseBehavior::Default, true),
            focused: true,
            looking: false,
            look_dx: 0.0,
            look_dy: 0.0,
            zoom: 0.0,
            pointer: None,
            text_focus: None,
            chat_typing: None,
            ui_warnings: Vec::new(),
            ui_mouse_events: Vec::new(),
            ui_mouse_down: false,
            ui_mouse_taken: false,
            ui_pressed_button: None,
            ui: UiInteraction::default(),
            nav_keys: Vec::new(),
            ui_sounds: Vec::new(),
            cursor: CursorIcon::Default,
            schedule: Schedule::for_host(Hosts::PLAYER),
            frame_size: winit::dpi::PhysicalSize::new(0, 0),
            frame_loading_assets: false,
            frame_draw: None,
            frame_ui_layout: None,
            last_frame: None,
            shader_time: 0.0,
            watcher: None,
            network: Network::new(GameInfo {
                name: String::new(),
                version: String::new(),
            }),
            following: false,
            console: axcenity_ui::DevConsole::default(),
            console_allowed: false,
            dev: dev_console::DevSettings::default(),
            frame_stats: dev_console::FrameStats::default(),
            bench: None,
            bench_scene: String::new(),
            bench_counts: None,
            window_size: None,
            quality: None,
            launched: Instant::now(),
            preload: None,
        }
    }

    /// The world became a replica of a server's (Phase 64): what was kept
    /// about the old one starts again, and from now on the server
    /// simulates.
    fn follow_server(&mut self) {
        self.following = true;
        self.sim.physics = Physics::with_assets(self.sim.assets.clone());
        self.sim.characters = Controllers::default();
        self.sim.agents = Agents::default();
        self.particles = axcenity_scene::Particles::new();
        self.terrain = axcenity_scene::TerrainCache::new();
        self.sim.animators = Animators::with_ground(axcenity_character::foot_ground);
        self.sim
            .animators
            .update(&mut self.sim.world, &self.sim.assets, 0.0);
        self.text_focus = None;
        self.ui_pressed_button = None;
    }

    /// Shows the game's loading screen again for its minimum time when a
    /// script loaded a level (`LevelService:LoadLevel`, or a slot saved in
    /// another level). Its ScreenGui stays through the load.
    /// Frame step 1's input for the character controllers: the input
    /// map's actions with the player's rebinds, and the mouse look and
    /// wheel since the last frame. None while a TextBox is typed in or a
    /// Sequence locks input (Phase 51).
    fn controller_input(&mut self, seconds: f32) -> MoveInput {
        self.input.map = self.sim.scripts.input_map();
        let look = [self.look_dx, self.look_dy];
        let zoom = self.zoom;
        self.look_dx = 0.0;
        self.look_dy = 0.0;
        self.zoom = 0.0;
        if self.text_focus.is_some()
            || self.chat_typing.is_some()
            || self.sim.scripts.movement_locked(&self.sim.world)
        {
            return MoveInput::default();
        }
        self.input.motion_input(look, zoom, seconds)
    }

    /// Checks on the asset preload: true while the Workspace's assets are
    /// still loading.
    fn preload_loading(&mut self) -> bool {
        self.preload
            .as_mut()
            .is_some_and(|preload| !preload.update())
    }

    fn follow_level_loads(&mut self) {
        if self.sim.scripts.take_level_loads().is_empty() {
            return;
        }
        let shown = LoadingGui::start(
            &mut self.sim.world,
            &self.loading_screen.screen_gui,
            self.loading_screen.min_seconds,
            false,
        );
        if shown.is_some() {
            self.loading_gui = shown;
        }
    }

    fn fail(&mut self, event_loop: &ActiveEventLoop, err: anyhow::Error) {
        log::error!("{err:#}");
        self.error = Some(err);
        event_loop.exit();
    }

    /// The camera to draw and to hand to scripts: the follow camera when a
    /// third-person controller has a body, otherwise the fly camera.
    fn script_camera(&self) -> CameraState {
        match self.sim.characters.view(&self.sim.world) {
            Some(view) => CameraState {
                position: view.position,
                orientation: view.orientation,
            },
            None => camera_state(&self.camera),
        }
    }

    /// The camera drawn: the host's (`script_camera`), or where scripts
    /// hold it, with their field of view and shake.
    fn shown_view(&self) -> CameraView {
        self.game_camera.view(self.script_camera())
    }

    fn shown_camera(&self) -> axcenity_render::Camera {
        self.shown_view().render_camera()
    }

    /// Far from the camera and every other player, physics sleeps while
    /// streaming is on (Phase 80).
    fn set_far_physics_sleep(&mut self) {
        let far = axcenity_core::streaming::Settings::of(&self.sim.world).map(|settings| {
            let at = self.shown_camera().position;
            let mut focuses = vec![axcenity_core::Vector3::new(at.x, at.y, at.z)];
            focuses.extend(self.network.player_focuses(&self.sim.world));
            (focuses, settings.unload_radius())
        });
        self.sim.physics.set_far_sleep(far);
    }

    /// World streaming on this screen (Phase 80): what's in range of the
    /// camera, regions scripts asked for, and answering those once what's
    /// around them is on the GPU.
    fn step_streaming(&mut self, focus: Vec3, seconds: f64) {
        self.terrain.streaming.step(&self.sim.world, focus, seconds);
        let on = self.terrain.streaming.settings().is_some();
        if let Some(renderer) = &mut self.renderer {
            renderer.set_background_uploads(on);
        }
        for (request, position) in self.network.take_viewer_requests() {
            let at = Vec3::new(position.x, position.y, position.z);
            self.terrain.streaming.hold_region(at);
            self.region_waits.push((request, REGION_WAIT_SECONDS));
        }
        let uploads = self.renderer.as_ref().map_or(0, |r| r.pending_uploads());
        let loaded = self.terrain.pending_chunks() == 0 && uploads == 0;
        let mut waits = std::mem::take(&mut self.region_waits);
        waits.retain_mut(|(request, left)| {
            // A frame to ask for what it needs, then until it's all there.
            *left -= seconds;
            let ready = *left < REGION_WAIT_SECONDS - 2.0 * seconds && loaded;
            if ready || *left <= 0.0 {
                self.sim
                    .scripts
                    .finish_stream_around(&mut self.sim.world, *request);
                false
            } else {
                true
            }
        });
        self.region_waits = waits;
    }

    /// Frame step 1's camera, after the character controllers: the fly
    /// camera flies (unless a controller or a script holds the view), and
    /// LocalScripts see the camera and the window's size.
    fn begin_camera(&mut self, seconds: f32, size: (u32, u32)) {
        if self.sim.characters.view(&self.sim.world).is_none()
            && !self.sim.scripts.camera_scripted()
        {
            self.camera.update(&self.keys, seconds);
        }
        let seen = self.script_camera();
        self.sim
            .scripts
            .set_viewport_size(size.0 as f32, size.1 as f32);
        self.sim.scripts.set_camera(seen);
    }

    /// Just before step 6 draws: LocalScripts' functions bound with
    /// `BindToRenderStep` run in priority order, then `RenderStepped`
    /// fires, so what they move (the camera too) is drawn this frame.
    #[cfg(test)]
    fn render_step(&mut self, seconds: f32) {
        self.sim
            .scripts
            .render_step(&mut self.sim.world, f64::from(seconds));
    }

    /// Before drawing: a LocalScript's move of a Custom camera goes to the
    /// fly camera (a character controller owns the view otherwise), then a
    /// Follow or Track camera follows its subject, which physics and
    /// animation have moved, and shakes move on.
    #[cfg(test)]
    fn end_camera(&mut self, seconds: f32) {
        let moved = self.sim.scripts.take_camera_move();
        self.apply_camera_move(moved);
        self.game_camera = self
            .sim
            .scripts
            .step_camera(&self.sim.world, f64::from(seconds));
    }

    /// See [`HostIo::camera_moved`].
    fn apply_camera_move(&mut self, moved: Option<CameraState>) {
        if self.sim.characters.view(&self.sim.world).is_none()
            && let Some(view) = moved
        {
            self.camera.set_view(view.position, view.orientation);
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
                &self.sim.world,
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
            let click = |pressed| InputKind::MouseButton {
                button: UserInputType::MouseButton1,
                pressed,
                position: self.pointer.unwrap_or_default(),
            };
            if pressed {
                self.ui_mouse_down = true;
                // A press on a ScrollingFrame's bar drags the bar instead,
                // and one on a widget (or an open Dropdown's list) changes it.
                if let Some(pointer) = in_window
                    && self
                        .ui
                        .press(&self.sim.world, pointer)
                        .apply(&mut self.sim.world)
                {
                    self.ui_pressed_button = None;
                    self.ui_mouse_taken = true;
                    self.input.push(InputEvent::processed(click(true)));
                    continue;
                }
                // Scripts hear every click; one on a button or text box
                // has `gameProcessedEvent` true, and the game doesn't use it.
                self.ui_mouse_taken = hit.is_some() || text_box.is_some();
                self.input.push(InputEvent {
                    kind: click(true),
                    processed: self.ui_mouse_taken,
                });
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
                    && self.ui.activate(&self.sim.world, button)
                {
                    self.sim
                        .scripts
                        .activate_ui_button(&mut self.sim.world, button);
                    print_output(&mut self.sim.scripts);
                }
                self.ui_pressed_button = None;
                self.ui_mouse_down = false;
                self.input.push(InputEvent {
                    kind: click(false),
                    processed: std::mem::take(&mut self.ui_mouse_taken),
                });
                // Let go over a Toggle flips it, over a Dropdown opens its
                // list, and over one of its options picks it.
                let _ = self
                    .ui
                    .release(&self.sim.world, in_window)
                    .apply(&mut self.sim.world);
            }
        }
        // A held press scrolls a ScrollingFrame by its bar, or by its content
        // once it has moved far enough; then the button it began on won't fire.
        if self.ui_mouse_down
            && let Some(pointer) = self.pointer
            && self
                .ui
                .drag(&self.sim.world, pointer)
                .apply(&mut self.sim.world)
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
        keys.extend(self.input.poll_pads(&self.ui, &self.sim.world, seconds));
        for key in keys {
            if let Some(id) = self.text_focus {
                if matches!(key, NavKey::Confirm | NavKey::Back) {
                    self.blur_text(id);
                }
                continue;
            }
            let typing = navigate_ui(
                &mut self.ui,
                &mut self.sim.world,
                &mut self.sim.scripts,
                key,
            );
            print_output(&mut self.sim.scripts);
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
            self.sim
                .scripts
                .release_text_box(&mut self.sim.world, previous);
            print_output(&mut self.sim.scripts);
        }
        self.text_focus = Some(id);
        self.keys = MoveKeys::default();
        self.release_keys();
    }

    /// Lets go of every key held, for scripts and the game.
    /// Lets go of every key and button the game sees as held, so nothing
    /// keeps moving (the window lost focus, or the console opened).
    fn release_game_keys(&mut self) {
        self.keys = MoveKeys::default();
        self.release_keys();
        for event in self.input.game.release_all() {
            self.input.push(event);
        }
        self.looking = false;
    }

    fn release_keys(&mut self) {
        for key in std::mem::take(&mut self.held_keys) {
            self.input.push(InputEvent::new(InputKind::Key {
                key,
                pressed: false,
            }));
        }
    }

    /// Records a key for scripts (the UI took it when `processed`), and as
    /// held, until it comes up.
    fn push_key(&mut self, key: axcenity_core::KeyCode, pressed: bool, processed: bool) {
        if pressed {
            self.held_keys.insert(key);
        } else {
            self.held_keys.remove(&key);
        }
        self.input.push(InputEvent {
            kind: InputKind::Key { key, pressed },
            processed,
        });
    }

    /// Holds or frees the cursor as `MouseBehavior`, `MouseIconEnabled` and
    /// right-button look ask, and keeps it in the middle while it's locked
    /// there. Returns where scripts see the pointer.
    fn apply_cursor(&mut self, size: (u32, u32)) -> Option<[f32; 2]> {
        let Some(window) = &self.window else {
            return self.pointer;
        };
        let (behavior, icon) = self.sim.scripts.mouse();
        let (lock, visible) = input::cursor_mode(behavior, icon, self.looking);
        if (behavior, icon) != self.mouse {
            self.mouse = (behavior, icon);
            capture_mouse(window, lock != CursorLock::Free && self.focused);
            window.set_cursor_visible(visible);
        }
        if lock == CursorLock::Centre && self.focused {
            let [x, y] = input::centre(size);
            // `Confined` (Windows, X11) lets the cursor drift; put it back.
            let _ = window.set_cursor_position(winit::dpi::PhysicalPosition::new(x, y));
            return Some([x, y]);
        }
        self.pointer
    }

    fn blur_text(&mut self, id: InstanceId) {
        self.text_focus = None;
        self.sim.scripts.release_text_box(&mut self.sim.world, id);
        print_output(&mut self.sim.scripts);
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
        if axcenity_ui::edit_text_box(&mut self.sim.world, id, command)
            == axcenity_ui::TextOutcome::Finished
        {
            self.blur_text(id);
        }
    }

    /// Typing in the open chat: Enter sends the line, Esc closes it.
    fn type_chat(&mut self, event: &winit::event::KeyEvent) {
        if event.state != ElementState::Pressed {
            return;
        }
        let Some(line) = &mut self.chat_typing else {
            return;
        };
        match event.physical_key {
            PhysicalKey::Code(KeyCode::Escape) => self.chat_typing = None,
            PhysicalKey::Code(KeyCode::Enter | KeyCode::NumpadEnter) => {
                let line = self.chat_typing.take().unwrap_or_default();
                self.sim.scripts.send_chat(&line);
            }
            PhysicalKey::Code(KeyCode::Backspace) => {
                line.pop();
            }
            _ => {
                if let Some(text) = &event.text {
                    line.extend(text.chars().filter(|c| !c.is_control()));
                }
            }
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

impl App {
    /// One frame, in the order from docs/architecture/frame-loop.md.
    fn frame(&mut self, event_loop: &ActiveEventLoop) {
        profiling::scope!("frame");
        // `timescale` in the console speeds the game up or slows it.
        let real_seconds = frame_seconds(&mut self.last_frame);
        // `--bench` steps a fixed 1/60 s a frame, as the other
        // hosts' benchmarks do, so every frame has the same work.
        let seconds = if self.bench.is_some() {
            1.0 / 60.0
        } else {
            self.game_seconds(real_seconds)
        };
        let mut watch = dev_console::Stopwatch::start();
        let size = self
            .window
            .as_ref()
            .expect("checked at the start of the event")
            .inner_size();
        self.frame_size = size;
        self.process_ui_mouse_events((size.width, size.height));
        self.process_ui_navigation(seconds);
        // What the UI did last frame and this click: MouseEnter,
        // MouseLeave and buttons' sounds.
        let events = self.ui.take_events();
        let sounds = fire_ui_events(&mut self.sim.scripts, &mut self.sim.world, events);
        self.ui_sounds.extend(sounds);

        // Assets changed on disk (`--watch`) are swapped in first,
        // so this frame's steps use them.
        if let Some(watcher) = &self.watcher {
            let paths = watcher.changes();
            let lines = watch::reload(
                &paths,
                &self.sim.assets,
                &mut self.sim.animators,
                &mut self.audio,
            );
            for line in lines {
                logs::line(LogLevel::Info, None, &line);
            }
        }

        // The steps, up to network send. `Player:Kick` on this device's
        // player ends the frame and the game.
        let schedule = std::mem::take(&mut self.schedule);
        let ended = {
            let mut ctx = FrameCtx::new(f64::from(seconds), Hosts::PLAYER, self);
            schedule.run(&mut ctx, &mut watch);
            ctx.ended()
        };
        self.schedule = schedule;
        if ended {
            event_loop.exit();
            return;
        }

        // `|| ...` is a closure: a small function the renderer calls at the right moment.
        let window = self
            .window
            .as_ref()
            .expect("checked at the start of the event")
            .clone();
        let draw_list = self.frame_draw.take().expect("the draw steps ran");
        let rendered = self
            .renderer
            .as_mut()
            .expect("checked")
            .render(&draw_list, || window.pre_present_notify());
        if let Err(err) = rendered {
            self.fail(event_loop, err);
        }
        // Materials whose shaders don't compile (drawn magenta).
        if let Some(renderer) = &mut self.renderer {
            for error in renderer.take_shader_errors() {
                logs::line(LogLevel::Error, None, &format!("error: {error}"));
            }
        }
        let renderer = self.renderer.as_mut().expect("checked");
        watch.lap_with_wait("Render", dev_console::PRESENT_WAIT, renderer.present_wait());
        // The GPU is timed while someone is looking: `--bench`, the `fps`
        // readout or the console's Stats tab.
        let looking = self.bench.is_some() || self.dev.show_fps || self.console.showing_stats();
        renderer.set_gpu_timing(looking);
        let render_stats = looking.then(|| renderer.stats());
        if let (Some(bench), Some(stats)) = (&mut self.bench, &render_stats)
            && let Some(gpu) = stats.gpu_time
            && stats.gpu_readings != self.frame_stats.gpu_readings()
        {
            bench.record_gpu(gpu, &axcenity_ui::pass_times(stats));
        }
        if let (Some(counts), Some(stats)) = (&mut self.bench_counts, &render_stats) {
            let camera = draw_list.camera;
            let _ = writeln!(
                counts,
                "{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{},{},{},{}",
                camera.position.x,
                camera.position.y,
                camera.position.z,
                camera.look_at.x,
                camera.look_at.y,
                camera.look_at.z,
                stats.items_drawn,
                stats.triangles,
                stats.draw_calls,
                stats.shadow_casters_drawn
            );
        }
        self.frame_stats
            .finish(real_seconds, &watch, render_stats.as_ref());
        profiling::finish_frame!();
        let loading_assets = self.frame_loading_assets;
        if let Some(bench) = &mut self.bench {
            bench.note("launch to first frame", self.launched.elapsed());
            if !loading_assets {
                bench.note("launch to assets loaded", self.launched.elapsed());
            }
        }
        if let Some(bench) = &mut self.bench
            && bench.record(&watch)
        {
            print!("{}", bench.finish_text("player", &self.bench_scene));
            // Every frame's steps (and the GPU's passes) for a spreadsheet.
            if let Some(path) = std::env::var_os("AXCENITY_BENCH_TIMELINE") {
                let path = std::path::PathBuf::from(path);
                let _ = std::fs::write(&path, bench.samples.csv());
                let _ = std::fs::write(path.with_extension("gpu.csv"), bench.gpu.csv());
                if let Some(counts) = &self.bench_counts {
                    let _ = std::fs::write(path.with_extension("counts.csv"), counts);
                }
            }
            event_loop.exit();
        }
    }
}

impl HostIo for App {
    fn sim(&mut self) -> &mut Sim {
        &mut self.sim
    }

    fn following(&self) -> bool {
        self.following
    }

    /// What scripts asked of NetworkService last frame, then who joined or
    /// left (Phase 63).
    fn net_receive(&mut self, seconds: f64) -> ops::ControlFlow<()> {
        self.network
            .receive(&mut self.sim.world, &mut self.sim.scripts, seconds);
        print_network_log(&mut self.network);
        print_output(&mut self.sim.scripts);
        // `Player:Kick` on this device's player ends the game.
        if let Some(reason) = self.network.take_kicked() {
            logs::line(
                LogLevel::Warning,
                None,
                &format!("You were kicked from the game: {reason}"),
            );
            return ops::ControlFlow::Break(());
        }
        if !axcenity_ui::chat_enabled(&self.sim.world) {
            self.chat_typing = None;
        }
        if self.sim.scripts.is_client_only() && !self.following {
            self.follow_server();
        }
        ops::ControlFlow::Continue(())
    }

    /// A character controller moves first and owns the camera, by the
    /// input map's actions (with the player's rebinds). In a hosted or
    /// joined game, only this device's player's character takes input, and
    /// other devices move their own (Phase 66).
    fn character_motion(&mut self, seconds: f32) -> MoveInput {
        let motion = self.controller_input(seconds);
        let others = self
            .network
            .other_characters(&self.sim.world, &self.sim.scripts);
        self.sim
            .characters
            .set_players(self.sim.scripts.local_player(), others);
        self.network.set_running(motion.run);
        motion
    }

    fn flush_output(&mut self) {
        print_output(&mut self.sim.scripts);
    }

    fn agent_warning(&mut self, warning: String) {
        logs::line(LogLevel::Warning, None, &format!("warning: {warning}"));
    }

    /// Without a controller the fly camera moves, and LocalScripts see that.
    fn frame_input(&mut self, seconds: f64) -> Vec<InputEvent> {
        let size = (self.frame_size.width, self.frame_size.height);
        self.begin_camera(seconds as f32, size);
        let pointer = self.apply_cursor(size);
        self.input.take_events(pointer)
    }

    /// Parts another device moves are held still here, and physics sleeps
    /// far from the camera and every other player.
    fn prepare_physics(&mut self) {
        let held = self.network.held_parts(&self.sim.world, &self.sim.scripts);
        self.sim.physics.set_held(held);
        self.set_far_physics_sleep();
    }

    fn owns_physics(&self) -> bool {
        self.network.simulates_physics()
    }

    fn audio(&mut self) {
        let heard = listener_of_view(self.shown_view());
        apply_audio(
            &mut self.sim.scripts,
            &mut self.sim.world,
            &self.sim.assets,
            &mut self.audio,
            heard,
            std::mem::take(&mut self.ui_sounds),
        );
    }

    /// Big emitters move on the GPU, where it runs compute shaders.
    fn particles(&mut self, seconds: f64) {
        self.particles.set_gpu(
            self.renderer
                .as_ref()
                .is_some_and(|renderer| renderer.capabilities().compute_shaders),
        );
        for (emitter, count) in self.sim.scripts.take_particle_emits() {
            self.particles.emit(&self.sim.world, emitter, count);
        }
        self.particles.update(&self.sim.world, seconds as f32);
    }

    fn animator_error(&mut self, error: String) {
        logs::line(LogLevel::Error, None, &format!("error: {error}"));
    }

    /// A level loaded this frame shows the loading screen again.
    fn level_loads(&mut self) {
        self.follow_level_loads();
    }

    /// The loading screen goes once its time is up and, when the game
    /// waits for one, a LocalScript has said it's ready. The splash or
    /// loading screen also waits for the preload.
    fn loading_screen(&mut self, seconds: f64) {
        let ready = self.sim.scripts.take_loading_screen_removed();
        if ready {
            self.splash.release();
        }
        let loading_assets = self.preload_loading();
        self.frame_loading_assets = loading_assets;
        self.splash.set_loading(loading_assets);
        if let Some(loading) = &mut self.loading_gui {
            loading.set_assets_loading(loading_assets);
            loading.advance(&mut self.sim.world, seconds as f32, ready);
        }
    }

    /// A LocalScript's move of a Custom camera goes to the fly camera; a
    /// character controller owns the view otherwise.
    fn camera_moved(&mut self, moved: Option<CameraState>) {
        self.apply_camera_move(moved);
    }

    fn game_camera(&mut self, camera: GameCamera) {
        self.game_camera = camera;
    }

    /// A LocalScript's graphics change (`UserGameSettings`) is saved and
    /// drawn from this frame.
    fn graphics_changed(&mut self) {
        if let Some(change) = self.sim.scripts.take_graphics_change()
            && self.quality.is_none()
            && let Some(renderer) = &mut self.renderer
        {
            let capabilities = *renderer.capabilities();
            let settings = graphics::changed(self.data_folder.as_deref(), change, &capabilities);
            set_graphics(renderer, &settings);
        }
    }

    fn streaming(&mut self, seconds: f64) {
        let render_camera = self.shown_camera();
        let heard = listener_of_view(self.shown_view());
        self.audio.set_listener(heard);
        self.step_streaming(render_camera.position, seconds);
    }

    /// Builds the draw list at this window's current pixel size, from the
    /// camera scripts left.
    fn draw_scene(&mut self, seconds: f64) {
        let size = self.frame_size;
        let on_top = self.loading_gui.as_ref().and_then(LoadingGui::on_top);
        let camera = self.shown_camera();
        self.terrain.set_blends(blends_to_draw(&self.sim));
        let (draw_list, ui_layout) = axcenity_scene::build_draw_list_with_ui_interaction(
            &self.sim.world,
            &self.sim.assets,
            camera,
            (size.width, size.height),
            &self.stylesheets,
            UiFrame {
                interaction: &mut self.ui,
                input: UiInput {
                    pointer: self.pointer,
                    pressed: self.ui_pressed_button,
                    focused: self.text_focus,
                    on_top,
                },
                seconds: seconds as f32,
            },
            self.sim.animators.poses(),
            &mut self.terrain,
        );
        self.frame_draw = Some(draw_list);
        self.frame_ui_layout = Some(ui_layout);
    }

    /// The game's overlays, the particles, the console and the splash or
    /// loading screen, over the draw list.
    fn draw_ui(&mut self, seconds: f64) {
        let size = self.frame_size;
        let render_camera = self.shown_camera();
        let mut draw_list = self.frame_draw.take().expect("the scene was drawn first");
        let ui_layout = self
            .frame_ui_layout
            .take()
            .expect("the scene was drawn first");
        self.note_ui_warnings(&ui_layout.warnings);
        // The UI's cursor, else a ClickDetector's under the mouse.
        let click_cursor = self.sim.scripts.click_cursor(&self.sim.world);
        let cursor = match (ui_layout.cursor, click_cursor) {
            (UiCursor::Default, Some(name)) => name.parse().unwrap_or(CursorIcon::Pointer),
            (cursor, _) => cursor_icon(cursor),
        };
        if cursor != self.cursor
            && let Some(window) = &self.window
        {
            self.cursor = cursor;
            window.set_cursor(cursor);
        }
        // The hotbar, the ability bar, the quest tracker and dialogue box,
        // the leaderboard, the chat window, then ProximityPrompts that show.
        let inventory = axcenity_gameplay::player_inventory(&self.sim.world);
        let abilities = axcenity_gameplay::ability_bar(&self.sim.scripts, &self.sim.world);
        let quests = axcenity_gameplay::quest_tracker(&self.sim.scripts, &self.sim.world);
        let dialogue = axcenity_gameplay::dialogue_view(&self.sim.scripts, &self.sim.world);
        let lines: Vec<(Option<String>, String)> = self
            .sim
            .scripts
            .chat_lines()
            .into_iter()
            .map(|line| (line.from, line.text))
            .collect();
        let prompts = prompt_views(self.sim.scripts.shown_prompts());
        draw_list.ui.extend(axcenity_ui::overlay_items(
            &self.sim.world,
            &axcenity_ui::OverlayView {
                inventory,
                abilities: &abilities,
                quests: &quests,
                dialogue: dialogue.as_ref(),
                leaderboard: self.sim.scripts.leaderboard_shown(),
                chat: Some((&lines, self.chat_typing.as_deref())),
                prompts: &prompts,
            },
            &self.stylesheets,
            &render_camera,
            size.width,
            size.height,
        ));
        self.particles
            .draw(&mut draw_list, &self.sim.world, &self.sim.assets);
        self.particles.add_weather(&mut draw_list);
        self.shader_time += seconds as f32;
        draw_list.time = self.shader_time;
        // The console's outlines and readout, then the console, over the
        // game's UI (Phase 71).
        self.take_log();
        draw_list
            .ui
            .extend(self.console_items(render_camera, (size.width, size.height)));
        let loading_assets = self.frame_loading_assets;
        if self.show_splash {
            // The first drawn frame starts the splash clock.
            self.splash.tick();
            // Once its bar is full, a splash still waiting for assets says
            // how many are in.
            let caption = match &self.preload {
                Some(preload) if self.splash.waiting_for_assets() => preload.progress().caption(),
                _ => "Made with Axcenity".to_owned(),
            };
            draw_list.ui.extend(
                self.splash
                    .items(size.width as f32, size.height as f32, &caption),
            );
        } else if loading_assets
            && self.loading_gui.is_none()
            && let Some(preload) = &self.preload
        {
            // No splash and no loading screen of the game's own: a plain
            // one, so textures don't pop in.
            draw_list.ui.extend(preload::screen_items(
                size.width as f32,
                size.height as f32,
                preload.progress(),
            ));
        }
        // Models that couldn't be loaded, by physics or for drawing.
        for error in self.sim.assets.take_errors() {
            logs::line(LogLevel::Error, None, &format!("error: {error}"));
        }
        self.frame_draw = Some(draw_list);
    }

    /// After the frame's scripts, physics and UI layout, before rendering.
    fn net_send(&mut self) {
        self.network.send(&self.sim.world, &mut self.sim.scripts);
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

        let mut attributes = Window::default_attributes()
            .with_title(&self.title)
            .with_inner_size(DEFAULT_WINDOW_SIZE)
            .with_window_icon(icon::window_icon());
        // `--size`, else `AXCENITY_WINDOW_SIZE=1920x1080` (physical pixels),
        // for benchmarks that need the same window every run, in every engine.
        let size = self.window_size.or_else(|| {
            window_size_override(std::env::var("AXCENITY_WINDOW_SIZE").ok().as_deref())
        });
        if let Some((width, height)) = size {
            attributes = attributes.with_inner_size(winit::dpi::PhysicalSize::new(width, height));
        }
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
                let settings = match self.sim.scripts.take_graphics_change() {
                    Some(change) => graphics::changed(folder, change, &capabilities),
                    None => graphics::start(folder, &capabilities),
                };
                // `--quality`: the preset alone, not saved.
                let settings = match self.quality {
                    Some(quality) => GraphicsSettings::preset(quality),
                    None => settings,
                };
                if self.bench.is_some() {
                    // Frames as fast as they're drawn, each timed on the GPU.
                    renderer.set_vsync(false);
                    renderer.set_gpu_timing(true);
                }
                self.sim.scripts.set_graphics(&settings, &capabilities);
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
                let pressed = event.state == ElementState::Pressed;
                let key = match event.physical_key {
                    PhysicalKey::Code(code) => Some(code),
                    PhysicalKey::Unidentified(_) => None,
                };
                // F9, and every key while the Developer Console is open
                // (Phase 71).
                if self.console_key(key, pressed, event.repeat, event.text.as_deref()) {
                    return;
                }
                if self.chat_typing.is_some() {
                    self.type_chat(&event);
                    if let Some(key) = key.and_then(script_key) {
                        self.push_key(key, pressed, true);
                    }
                } else if pressed
                    && key == Some(KeyCode::Slash)
                    && axcenity_ui::chat_enabled(&self.sim.world)
                {
                    // `/` opens the chat (Phase 67).
                    self.chat_typing = Some(String::new());
                } else if self.text_focus.is_some() {
                    self.type_text(&event);
                    // Scripts hear typed keys, as the UI's.
                    if let Some(key) = key.and_then(script_key) {
                        self.push_key(key, pressed, true);
                    }
                } else if let Some(code) = key {
                    // A press that moves focus is the UI's, not the game's.
                    let shift = self.held_keys.contains(&axcenity_core::KeyCode::LeftShift)
                        || self.held_keys.contains(&axcenity_core::KeyCode::RightShift);
                    let script = script_key(code);
                    if pressed
                        && let Some(key) = script
                        && let Some(nav) = axcenity_ui::nav_key(&self.input.map, key, shift)
                        && self.ui.takes(&self.sim.world, nav)
                    {
                        self.nav_keys.push(nav);
                        self.push_key(key, true, true);
                        return;
                    }
                    set_move_key(&mut self.keys, code, pressed);
                    if let Some(key) = script {
                        self.push_key(key, pressed, false);
                    }
                }
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => self.ui_mouse_events.push(state == ElementState::Pressed),
            WindowEvent::MouseInput { state, button, .. } => {
                let pressed = state == ElementState::Pressed;
                // The right button looks around, unless a script holds the cursor.
                if button == MouseButton::Right {
                    self.looking = pressed;
                    if self.mouse.0 == MouseBehavior::Default
                        && let Some(window) = &self.window
                    {
                        capture_mouse(window, self.looking);
                        window.set_cursor_visible(!self.looking && self.mouse.1);
                    }
                }
                if let Some(button) = input::mouse_button(button) {
                    self.input.push(InputEvent::new(InputKind::MouseButton {
                        button,
                        pressed,
                        position: self.pointer.unwrap_or_default(),
                    }));
                }
            }
            // Over a ScrollingFrame the wheel scrolls it; with a Tool in
            // hand it goes through the hotbar; elsewhere it zooms.
            WindowEvent::MouseWheel { delta, .. } => {
                let scrolled = self.pointer.is_some_and(|pointer| {
                    self.ui
                        .wheel(&self.sim.world, pointer, wheel_pixels(delta))
                        .apply(&mut self.sim.world)
                });
                if !scrolled && !tool_in_hand(&self.sim.world) {
                    self.zoom += wheel_notches(delta);
                }
                self.input.push(InputEvent {
                    kind: InputKind::Wheel {
                        position: self.pointer.unwrap_or_default(),
                        notches: wheel_notches(delta),
                    },
                    processed: scrolled,
                });
            }
            WindowEvent::Touch(touch) => self.input.push(InputEvent::new(input::touch(&touch))),
            WindowEvent::Focused(true) => {
                self.focused = true;
                // Locked again, if a script asked for it.
                self.mouse = (MouseBehavior::Default, true);
            }
            // Key releases aren't reported while another window has focus,
            // so let go of everything to avoid the camera drifting on its own.
            WindowEvent::Focused(false) => {
                self.focused = false;
                self.release_game_keys();
                if let Some(window) = &self.window {
                    capture_mouse(window, false);
                }
                self.ui_mouse_events.push(false);
            }
            // One frame: the host's input first, then the shared schedule
            // (`axcenity_sim`, docs/architecture/frame-loop.md), then render.
            WindowEvent::RedrawRequested => self.frame(event_loop),
            _ => {}
        }
    }

    /// Raw mouse movement, reported even when the cursor is held still.
    /// Scripts hear it (`GetMouseDelta`) while the window has focus; it
    /// turns the camera while the right button is held.
    fn device_event(&mut self, _event_loop: &ActiveEventLoop, _id: DeviceId, event: DeviceEvent) {
        let DeviceEvent::MouseMotion { delta: (dx, dy) } = event else {
            return;
        };
        if !self.focused {
            return;
        }
        let (dx, dy) = (dx as f32, dy as f32);
        self.input.add_motion(dx, dy);
        if self.looking && !self.sim.scripts.camera_scripted() {
            if self.sim.characters.view(&self.sim.world).is_some() {
                self.look_dx += dx;
                self.look_dy += dy;
            } else {
                self.camera.look(dx, dy);
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
mod input_tests;

#[cfg(test)]
mod camera_tests;

#[cfg(test)]
mod graphics_tests;

#[cfg(test)]
mod time_of_day_tests;

#[cfg(test)]
mod clouds_tests;
#[cfg(test)]
mod console_tests;

#[cfg(test)]
mod volume_tests;
#[cfg(test)]
mod weather_tests;

#[cfg(test)]
mod bench_tests;
#[cfg(test)]
mod profile_tests;

#[cfg(test)]
mod gpu_particles_tests;

#[cfg(test)]
mod skinning_tests;

#[cfg(test)]
mod shader_tests;

#[cfg(test)]
mod material_graph_tests;

#[cfg(test)]
mod accessibility_tests;

#[cfg(test)]
mod csg_tests;
#[cfg(test)]
mod cutscene_tests;
#[cfg(test)]
mod destruction_tests;
#[cfg(test)]
mod physics_tests;

#[cfg(test)]
mod decal_tests;
#[cfg(test)]
mod health_tests;
#[cfg(test)]
mod humanoid_tests;
#[cfg(test)]
mod level_tests;
#[cfg(test)]
mod prompt_tests;
#[cfg(test)]
mod ragdoll_tests;
#[cfg(test)]
mod road_tests;
#[cfg(test)]
mod seat_tests;
#[cfg(test)]
mod spawn_tests;
#[cfg(test)]
mod spline_tests;
#[cfg(test)]
mod template_tests;
#[cfg(test)]
mod tool_tests;
#[cfg(test)]
mod vehicle_tests;
