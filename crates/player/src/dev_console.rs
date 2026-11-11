//! The player's side of the Developer Console (Phase 71): F9 and the keys
//! while it's open, running what's typed, the built-in commands' effects,
//! the Stats tab's measurements, and the console, `fps` readout and
//! `collisions` outlines over the frame. The console itself (its tabs,
//! log and drawing) is `axcenity_ui::DevConsole`.
//!
//! A line typed in the Commands tab is a built-in command
//! ([`axcenity_ui::built_in`]), else a game's `ConsoleService` command,
//! else Lua. Joined to a server elsewhere, a line that reads as a command
//! no script here knows goes to the server, which runs it when the player
//! is allowed.

use std::collections::VecDeque;
use std::path::Path;

use axcenity_render::UiDrawItem;
use axcenity_render::glam::Vec3;
use axcenity_scene::CameraView;
use axcenity_script::ConsoleRun;
use axcenity_ui::{BuiltIn, ConsoleKey, ConsoleStats, LogLevel, NetworkStats};
use winit::keyboard::KeyCode;

use crate::{App, logs, print_output};

/// The frames the frame rate is averaged over.
const AVERAGED_FRAMES: usize = 60;
/// The frames the 1% low is taken over: the slowest 10 of them.
const PACED_FRAMES: usize = 1000;
/// The frame step that's time blocked on the window (vertical sync, or the
/// GPU frames behind), split off `Render`.
pub const PRESENT_WAIT: &str = "Present wait";
/// Asking the OS for memory every frame would be wasted work.
const MEMORY_EVERY: u64 = 30;
/// Collider outlines: solid (green) and sensors (amber).
const SOLID_COLOR: [f32; 4] = [0.31, 0.86, 0.47, 1.0];
const SENSOR_COLOR: [f32; 4] = [1.0, 0.7, 0.24, 1.0];

/// Whether this run has the console: a development run (a project
/// folder), `--dev`, or a built game whose project turns it on.
pub fn console_allowed(game: Option<&Path>, dev_flag: bool, developer_console: bool) -> bool {
    let development = game.is_some_and(Path::is_dir);
    development || dev_flag || developer_console
}

/// What the console's commands have changed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DevSettings {
    /// Game time per real second (`timescale`).
    pub time_scale: f32,
    pub show_fps: bool,
    /// `fps full`: the readout with the drawing's numbers.
    pub fps_full: bool,
    pub wireframe: bool,
    pub collisions: bool,
}

impl Default for DevSettings {
    fn default() -> Self {
        Self {
            time_scale: 1.0,
            show_fps: false,
            fps_full: false,
            wireframe: false,
            collisions: false,
        }
    }
}

/// Times one frame's steps, each from the end of the one before. The same
/// timer every host's benchmark uses (`--bench`, roadmap I1).
pub use axcenity_core::bench::Laps as Stopwatch;

/// The measurements the Stats tab and `fps` show, kept between frames.
#[derive(Default)]
pub struct FrameStats {
    /// The last frames' real lengths, in seconds, newest last.
    frames: VecDeque<f32>,
    /// More of them, in milliseconds, for the 1% low.
    paced: VecDeque<f32>,
    stages: Vec<(&'static str, f32)>,
    memory: Option<u64>,
    frame: u64,
    /// The CPU's work in recent frames (each frame less its present
    /// wait), in milliseconds, averaged like `frames`.
    cpu: VecDeque<f32>,
    /// The GPU's recent readings, in milliseconds.
    gpu: VecDeque<f32>,
    /// The newest drawing numbers, while someone is looking.
    render: Option<axcenity_render::RenderStats>,
    gpu_readings: u64,
}

impl FrameStats {
    /// The GPU measurements read so far, so a new one can be told apart.
    pub fn gpu_readings(&self) -> u64 {
        self.gpu_readings
    }

    /// Notes a frame `real_seconds` long, whose steps `watch` timed, and
    /// what drawing it cost if it was measured (`render`).
    pub fn finish(
        &mut self,
        real_seconds: f32,
        watch: &Stopwatch,
        render: Option<&axcenity_render::RenderStats>,
    ) {
        let keep = |queue: &mut VecDeque<f32>, value: f32, most: usize| {
            queue.push_back(value);
            while queue.len() > most {
                queue.pop_front();
            }
        };
        if real_seconds > 0.0 {
            keep(&mut self.frames, real_seconds, AVERAGED_FRAMES);
            keep(&mut self.paced, real_seconds * 1000.0, PACED_FRAMES);
        }
        let waited: f32 = watch
            .stages()
            .iter()
            .filter(|(name, _)| *name == PRESENT_WAIT)
            .map(|(_, time)| time.as_secs_f32() * 1000.0)
            .sum();
        let work = watch.total().as_secs_f32() * 1000.0 - waited;
        keep(&mut self.cpu, work.max(0.0), AVERAGED_FRAMES);
        match render {
            Some(stats) => {
                if let Some(gpu) = stats.gpu_time
                    && stats.gpu_readings != self.gpu_readings
                {
                    keep(&mut self.gpu, gpu.as_secs_f32() * 1000.0, AVERAGED_FRAMES);
                }
                self.gpu_readings = stats.gpu_readings;
                self.render = Some(*stats);
            }
            None => {
                self.gpu.clear();
                self.render = None;
            }
        }
        self.stages = watch
            .stages()
            .iter()
            .map(|&(name, time)| (name, time.as_secs_f32() * 1000.0))
            .collect();
        if self.frame.is_multiple_of(MEMORY_EVERY) {
            self.memory = memory_stats::memory_stats().map(|stats| stats.physical_mem as u64);
        }
        self.frame += 1;
    }
}

impl App {
    /// F9, and every key while the console is open. True when the console
    /// took the key, so the game doesn't see it. Opening it lets go of the
    /// game's held keys.
    pub(crate) fn console_key(
        &mut self,
        key: Option<KeyCode>,
        pressed: bool,
        repeat: bool,
        text: Option<&str>,
    ) -> bool {
        if !self.console_allowed {
            return false;
        }
        if key == Some(KeyCode::F9) {
            if pressed && !repeat {
                if !self.console.open {
                    self.release_game_keys();
                }
                self.console.toggle();
            }
            return true;
        }
        if !self.console.open {
            return false;
        }
        if pressed {
            let console_key = match key {
                Some(KeyCode::Enter | KeyCode::NumpadEnter) => Some(ConsoleKey::Enter),
                Some(KeyCode::Backspace) => Some(ConsoleKey::Backspace),
                Some(KeyCode::Tab) => Some(ConsoleKey::Tab),
                Some(KeyCode::ArrowUp) => Some(ConsoleKey::Up),
                Some(KeyCode::ArrowDown) => Some(ConsoleKey::Down),
                Some(KeyCode::PageUp) => Some(ConsoleKey::PageUp),
                Some(KeyCode::PageDown) => Some(ConsoleKey::PageDown),
                Some(KeyCode::Escape) => Some(ConsoleKey::Escape),
                _ => None,
            };
            match (console_key, text) {
                (Some(console_key), _) => {
                    if let Some(line) = self.console.key(console_key) {
                        self.run_console_line(&line);
                    }
                }
                (None, Some(text)) => self.console.type_text(text),
                (None, None) => {}
            }
        }
        true
    }

    /// Runs a line typed in the Commands tab.
    pub(crate) fn run_console_line(&mut self, line: &str) {
        match axcenity_ui::built_in(line) {
            Some(Ok(command)) => self.run_built_in(command),
            Some(Err(why)) => logs::line(LogLevel::Warning, None, &why),
            None => {
                if self
                    .sim
                    .scripts
                    .run_console_command(&mut self.sim.world, line)
                    == ConsoleRun::Unknown
                {
                    self.run_unknown(line);
                }
                print_output(&mut self.sim.scripts);
            }
        }
        self.take_log();
    }

    /// A line that's no built-in or registered command here: sent to the
    /// server when it reads as a command and this device is joined, else
    /// run as Lua (or said to be unknown).
    fn run_unknown(&mut self, line: &str) {
        let word = line.split_whitespace().next().unwrap_or_default();
        if axcenity_ui::looks_like_command(line)
            && !self.sim.scripts.console_knows(&self.sim.world, word)
        {
            if self.network.send_console_command(line) {
                logs::line(LogLevel::Info, None, "Sent to the server.");
            } else {
                logs::line(
                    LogLevel::Warning,
                    None,
                    &format!("Unknown command `{word}`. Type help for the list."),
                );
            }
        } else if let Err(err) = self.sim.scripts.run_console_lua(&mut self.sim.world, line) {
            logs::line(LogLevel::Error, None, &format!("error: {err}"));
        }
    }

    fn run_built_in(&mut self, command: BuiltIn) {
        let on_off = |on: bool| if on { "on" } else { "off" };
        match command {
            BuiltIn::Help => {
                let mut lines = vec!["Commands:".to_owned()];
                for (name, help) in axcenity_ui::BUILT_INS {
                    lines.push(format!("  {name:<22} {help}"));
                }
                for command in self.sim.scripts.console_commands() {
                    lines.push(format!("  {:<22} {}", command.name, command.help));
                }
                lines.push("Anything else runs as Lua.".to_owned());
                for line in lines {
                    logs::line(LogLevel::Info, None, &line);
                }
            }
            BuiltIn::Clear => self.console.clear(),
            BuiltIn::Fps { full } => {
                // The same command again hides it; the other mode switches.
                self.dev.show_fps = !(self.dev.show_fps && self.dev.fps_full == full);
                self.dev.fps_full = full;
                let mode = if full { "fps full" } else { "fps" };
                logs::line(
                    LogLevel::Info,
                    None,
                    &format!("{mode} {}", on_off(self.dev.show_fps)),
                );
            }
            BuiltIn::Wireframe => {
                self.dev.wireframe = !self.dev.wireframe;
                if let Some(renderer) = &mut self.renderer {
                    renderer.set_debug_view(
                        self.dev
                            .wireframe
                            .then_some(axcenity_render::DebugView::Wireframe),
                    );
                }
                let text = format!("wireframe {}", on_off(self.dev.wireframe));
                logs::line(LogLevel::Info, None, &text);
            }
            BuiltIn::Collisions => {
                self.dev.collisions = !self.dev.collisions;
                let text = format!("collisions {}", on_off(self.dev.collisions));
                logs::line(LogLevel::Info, None, &text);
            }
            BuiltIn::Noclip => {
                let on = !self.sim.characters.noclip();
                self.sim.characters.set_noclip(on);
                logs::line(LogLevel::Info, None, &format!("noclip {}", on_off(on)));
            }
            BuiltIn::Timescale(scale) => {
                self.dev.time_scale = scale;
                logs::line(LogLevel::Info, None, &format!("timescale {scale}"));
            }
            BuiltIn::Teleport(to) => {
                if !self.sim.characters.teleport(&mut self.sim.world, to) {
                    let (_, orientation) = self.camera.view();
                    self.camera.set_view(to, orientation);
                }
                let text = format!("teleported to {}, {}, {}", to.x, to.y, to.z);
                logs::line(LogLevel::Info, None, &text);
            }
            BuiltIn::Crash => panic!("forced from the Developer Console (`crash`)"),
        }
    }

    /// Game time for a frame `real_seconds` long: `timescale` times it.
    pub(crate) fn game_seconds(&self, real_seconds: f32) -> f32 {
        real_seconds * self.dev.time_scale
    }

    /// Moves what was logged since the last call into the console's Log.
    pub(crate) fn take_log(&mut self) {
        for line in logs::take_lines() {
            self.console.log(line.level, line.side, line.text);
        }
    }

    /// The Stats tab's numbers now.
    pub(crate) fn console_stats(&self) -> ConsoleStats {
        let mut console = self.base_console_stats();
        let stats = &self.frame_stats;
        let mean = |queue: &VecDeque<f32>| {
            (!queue.is_empty()).then(|| queue.iter().sum::<f32>() / queue.len() as f32)
        };
        console.low_fps = axcenity_core::bench::FramePacing::of_millis(
            stats.paced.iter().map(|&ms| f64::from(ms)),
        )
        .map_or(0.0, |pacing| pacing.low_1pct_fps as f32);
        console.cpu_ms = mean(&stats.cpu).unwrap_or(0.0);
        console.gpu_ms = mean(&stats.gpu);
        if let (Some(frame), Some(gpu)) = (mean(&stats.frames), console.gpu_ms) {
            let bound = axcenity_core::bench::Bound::of(
                f64::from(frame * 1000.0),
                f64::from(console.cpu_ms),
                Some(f64::from(gpu)),
            );
            console.bound = bound.label();
        }
        if let Some(render) = &stats.render {
            console.detail = axcenity_ui::render_lines(render);
        }
        console
    }

    fn base_console_stats(&self) -> ConsoleStats {
        let stats = &self.frame_stats;
        let average = if stats.frames.is_empty() {
            0.0
        } else {
            stats.frames.iter().sum::<f32>() / stats.frames.len() as f32
        };
        let peers = self.network.stats();
        let network = (self.network.is_hosting() || self.network.is_joined()).then(|| {
            let players = self
                .sim
                .world
                .get(self.sim.world.players())
                .map_or(0, |players| players.children().len());
            NetworkStats {
                players,
                ping_ms: peers
                    .iter()
                    .map(|peer| peer.link.rtt as f32 * 1000.0)
                    .fold(0.0, f32::max),
                sent_per_second: peers
                    .iter()
                    .map(|peer| peer.link.bytes_sent_per_second as f32)
                    .sum(),
                received_per_second: peers
                    .iter()
                    .map(|peer| peer.link.bytes_received_per_second as f32)
                    .sum(),
            }
        });
        ConsoleStats {
            fps: if average > 0.0 { 1.0 / average } else { 0.0 },
            frame_ms: stats.frames.back().copied().unwrap_or(0.0) * 1000.0,
            stages: stats.stages.clone(),
            memory_bytes: stats.memory,
            lua_bytes: self.sim.scripts.used_memory(),
            instances: self.sim.world.len(),
            network,
            ..ConsoleStats::default()
        }
    }

    /// What the console's commands draw over the frame, in a view `size`
    /// big: collider outlines, the `fps` readout, then the console.
    pub(crate) fn console_items(
        &self,
        camera: axcenity_render::Camera,
        size: (u32, u32),
    ) -> Vec<UiDrawItem> {
        let mut items = Vec::new();
        if !self.console_allowed {
            return items;
        }
        if self.dev.collisions && size.0 > 0 && size.1 > 0 {
            let (width, height) = (size.0 as f32, size.1 as f32);
            let view = CameraView::new(camera, width / height);
            let pixels = |point: axcenity_render::glam::Vec2| {
                [
                    (point.x + 1.0) * 0.5 * width,
                    (1.0 - point.y) * 0.5 * height,
                ]
            };
            let (mut solid, mut sensor) = (Vec::new(), Vec::new());
            for outline in axcenity_physics::debug_colliders(&self.sim.world, &self.sim.assets) {
                let lines = if outline.can_collide {
                    &mut solid
                } else {
                    &mut sensor
                };
                for (a, b) in &outline.edges {
                    let a = Vec3::new(a.x, a.y, a.z);
                    let b = Vec3::new(b.x, b.y, b.z);
                    if let Some((a, b)) = view.project_line(a, b) {
                        lines.push((pixels(a), pixels(b)));
                    }
                }
            }
            items.extend(axcenity_ui::line_items(&solid, SOLID_COLOR));
            items.extend(axcenity_ui::line_items(&sensor, SENSOR_COLOR));
        }
        let stats = self.console_stats();
        items.extend(axcenity_ui::console_items(
            &self.console,
            &stats,
            size.0,
            size.1,
        ));
        if self.dev.show_fps {
            items.extend(axcenity_ui::fps_items(
                &stats,
                &self.console,
                size.0,
                size.1,
                self.dev.fps_full,
            ));
        }
        items
    }
}
