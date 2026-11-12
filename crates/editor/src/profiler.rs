//! The Profiler panel: what the game costs while it plays, from the
//! measurements Play's frame loop (`Session::frame`) and the viewport
//! already take. The panel only reads them, so closing it changes nothing
//! in the game.
//!
//! Three tabs: **Game** (frame rate, memory, physics, audio, scripts and
//! the network), **Graphics** (frame and pass times, draws, culling,
//! levels of detail, GPU memory and shaders, all from [`RenderStats`]) and
//! **Memory** (roadmap G7: what the game holds by kind, from
//! `Session::memory`, the GPU's share, and the Instances by class).
//!
//! Readings start fresh at each Play, update every frame, and freeze at
//! Stop. The player has no profiler.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use axcenity_core::bench::{Bound, FramePacing};
use axcenity_core::memory::MemoryBreakdown;
use axcenity_render::{FramePass, LOD_LEVELS, RenderStats, SHADOW_CASCADE_COUNT, ShaderError};
use eframe::egui::{self, RichText};

use crate::responsive;
use crate::theme::{self, type_scale};
use crate::widgets;

/// How many frames the readings average over: about two seconds at 60 fps.
const WINDOW: usize = 120;
/// Asking the OS for the process's memory every frame would be wasted
/// work; every this many frames is plenty.
const MEMORY_EVERY: u64 = 15;
/// How many classes the Memory tab lists, the most bytes first.
const MEMORY_CLASSES: usize = 12;
/// How many shader messages the Graphics tab keeps, newest last.
const SHADER_MESSAGES: usize = 20;

/// What one Play frame's steps took on the CPU.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FrameCost {
    /// Input, resuming tasks, and the frame's events (`Touched`, `Heartbeat`).
    pub scripts: Duration,
    pub physics: Duration,
    /// Fixed 60 Hz steps run this frame (0 to N).
    pub physics_steps: u32,
    /// Applying sound commands and syncing every Sound. Mixing runs on the
    /// audio thread and isn't counted.
    pub audio: Duration,
    /// Animators and property animations.
    pub animation: Duration,
    /// How long each thread spent posing skeletons, inside `animation`
    /// (unified Phase 93).
    pub posing: PosingThreads,
    /// The whole game frame, steps 1 to 4.
    pub total: Duration,
}

/// Posing time by thread: the calling (main) thread first, then the
/// animation pool's threads in order (`anim 0`, `anim 1`, …; more than
/// the slots share the last).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PosingThreads(pub [Duration; PosingThreads::SLOTS]);

impl PosingThreads {
    pub const SLOTS: usize = 1 + axcenity_anim::ANIM_MAX_THREADS;

    /// From the names `Animators::thread_times` gives.
    pub fn of(times: &axcenity_anim::ThreadTimes) -> Self {
        let mut slots = [Duration::ZERO; Self::SLOTS];
        for (name, time) in times {
            let slot = name
                .strip_prefix("anim ")
                .and_then(|index| index.parse::<usize>().ok())
                .map_or(0, |index| (index + 1).min(Self::SLOTS - 1));
            slots[slot] += *time;
        }
        Self(slots)
    }

    /// Each slot's name, as the panel shows it.
    fn name(slot: usize) -> String {
        match slot {
            0 => "Main thread".to_owned(),
            slot => format!("Thread {slot}"),
        }
    }
}

impl FrameCost {
    /// A frame's cost from the benchmark steps it was timed in.
    pub fn from_laps(
        laps: &axcenity_core::bench::Laps,
        physics_steps: u32,
        total: Duration,
    ) -> Self {
        let mut cost = FrameCost {
            physics_steps,
            total,
            ..FrameCost::default()
        };
        for &(name, time) in laps.stages() {
            match name {
                "Input" | "Scripts" | "Heartbeat" | "RenderStep" => cost.scripts += time,
                "Physics" => cost.physics += time,
                "Audio" => cost.audio += time,
                "Animation" => cost.animation += time,
                _ => {}
            }
        }
        cost
    }
}

/// How much the game has to simulate.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GameLoad {
    /// Parts in the physics simulation, anchored ones included.
    pub bodies: usize,
    pub sounds_playing: usize,
    /// Both Lua heaps (server and client).
    pub lua_bytes: usize,
}

/// What drawing the game in the viewport took.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RenderCost {
    /// Building the draw list and recording the GPU's work, on the CPU.
    pub cpu: Duration,
    pub lights: usize,
    pub stats: RenderStats,
}

#[derive(Clone, Copy, Debug, Default)]
struct Sample {
    game: FrameCost,
    load: GameLoad,
    render: RenderCost,
    /// Wall time since the frame before, for the frame rate.
    interval: Option<Duration>,
}

/// Whether the readings are of the game playing now.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum State {
    /// Nothing has played yet.
    #[default]
    Empty,
    Live,
    /// Play stopped; the readings are its last ones.
    Frozen,
}

/// Which of the panel's tabs is shown.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Game,
    Graphics,
    Memory,
}

/// The profiler's numbers for the `fps` readout ([`Profiler::readout`]).
pub struct Readout {
    pub low_fps: f32,
    pub cpu_ms: f32,
    pub gpu_ms: Option<f32>,
    pub bound: &'static str,
    pub stats: RenderStats,
}

#[derive(Default)]
pub struct Profiler {
    pub tab: Tab,
    samples: VecDeque<Sample>,
    state: State,
    last_frame: Option<Instant>,
    frames: u64,
    /// The process's resident memory, as last asked; `None` where the OS
    /// doesn't say.
    memory: Option<usize>,
    /// The game's memory by kind, as last asked ([`Profiler::memory_due`]).
    breakdown: Option<MemoryBreakdown>,
    /// Each client's connection to the server, as last measured (Play with
    /// several clients, Phase 67).
    network: Vec<NetworkRow>,
    /// Materials' surface shaders that didn't compile, as reported (each
    /// once, in or out of Play), so they're kept across Plays.
    shader_messages: VecDeque<String>,
}

/// One client's connection, from the server's end.
#[derive(Clone, Debug, PartialEq)]
pub struct NetworkRow {
    pub player: String,
    /// The round trip, in seconds.
    pub ping: f64,
    /// Bytes a second the server sends them, and gets from them.
    pub sent: f64,
    pub received: f64,
    /// The share of packets lost, 0 to 1.
    pub loss: f64,
    /// Instances in their world.
    pub instances: usize,
}

impl Profiler {
    /// Play started: the readings start again.
    pub fn start(&mut self) {
        *self = Profiler {
            state: State::Live,
            tab: self.tab,
            shader_messages: std::mem::take(&mut self.shader_messages),
            ..Profiler::default()
        };
    }

    /// Play stopped: the readings stay as they were, marked as stopped.
    pub fn stop(&mut self) {
        if self.state == State::Live {
            self.state = State::Frozen;
            self.last_frame = None;
        }
    }

    pub fn is_live(&self) -> bool {
        self.state == State::Live
    }

    /// One game frame ran (not while paused at a breakpoint).
    pub fn record_game(&mut self, game: FrameCost, load: GameLoad) {
        if !self.is_live() {
            return;
        }
        let now = Instant::now();
        let interval = self.last_frame.map(|last| now - last);
        self.last_frame = Some(now);
        if self.frames.is_multiple_of(MEMORY_EVERY) {
            self.memory = memory_stats::memory_stats().map(|stats| stats.physical_mem);
        }
        self.frames += 1;
        // The viewport draws after the game frame; until then, the last
        // frame's drawing stands in.
        let render = self.samples.back().map(|last| last.render);
        if self.samples.len() == WINDOW {
            self.samples.pop_front();
        }
        self.samples.push_back(Sample {
            game,
            load,
            render: render.unwrap_or_default(),
            interval,
        });
    }

    /// True on the frames the game's memory breakdown should be asked for
    /// and handed to [`record_memory`](Self::record_memory): it walks every
    /// Instance and asset, so every few frames while live.
    pub fn memory_due(&self) -> bool {
        self.is_live() && self.frames.is_multiple_of(MEMORY_EVERY)
    }

    /// The game's memory by kind, for the Memory tab.
    pub fn record_memory(&mut self, breakdown: MemoryBreakdown) {
        if self.is_live() {
            self.breakdown = Some(breakdown);
        }
    }

    /// The memory breakdown last recorded, for the Developer Console.
    pub fn breakdown(&self) -> Option<&MemoryBreakdown> {
        self.breakdown.as_ref()
    }

    /// Each client's connection this frame.
    pub fn record_network(&mut self, rows: Vec<NetworkRow>) {
        if self.is_live() {
            self.network = rows;
        }
    }

    /// Surface shaders the renderer just found don't compile.
    pub fn record_shader_errors(&mut self, errors: &[ShaderError]) {
        for error in errors {
            if self.shader_messages.len() == SHADER_MESSAGES {
                self.shader_messages.pop_front();
            }
            self.shader_messages.push_back(error.to_string());
        }
    }

    /// What the `fps` readout shows during Play: the 1% low, the CPU's
    /// and GPU's times, what limits the frame and the newest drawing
    /// numbers. `None` before the first frame.
    pub fn readout(&self) -> Option<Readout> {
        let summary = self.summary()?;
        Some(Readout {
            low_fps: summary.low_fps.unwrap_or(0.0) as f32,
            cpu_ms: summary.cpu.as_secs_f32() * 1000.0,
            gpu_ms: summary.gpu.map(|gpu| gpu.as_secs_f32() * 1000.0),
            bound: summary.bound.label(),
            stats: summary.stats,
        })
    }

    /// The viewport drew the game frame just recorded.
    pub fn record_render(&mut self, render: RenderCost) {
        if self.is_live()
            && let Some(last) = self.samples.back_mut()
        {
            last.render = render;
        }
    }

    /// The readings: averages over the last frames, with the newest counts.
    fn summary(&self) -> Option<Summary> {
        let newest = self.samples.back()?;
        let count = self.samples.len() as u32;
        let average =
            |time: fn(&Sample) -> Duration| self.samples.iter().map(time).sum::<Duration>() / count;
        let intervals: Vec<Duration> = self.samples.iter().filter_map(|s| s.interval).collect();
        let fps = (!intervals.is_empty()).then(|| {
            let mean = intervals.iter().sum::<Duration>() / intervals.len() as u32;
            1.0 / mean.as_secs_f64().max(1e-6)
        });
        let gpu: Vec<Duration> = self
            .samples
            .iter()
            .filter_map(|s| s.render.stats.gpu_time)
            .collect();
        let low_fps = FramePacing::of(&intervals).map(|pacing| pacing.low_1pct_fps);
        let cpu = average(|s| s.game.total + s.render.cpu);
        let bound = match (intervals.is_empty(), mean(&gpu)) {
            (false, gpu) => Bound::of(
                (intervals.iter().sum::<Duration>() / intervals.len() as u32).as_secs_f64()
                    * 1000.0,
                cpu.as_secs_f64() * 1000.0,
                gpu.map(|gpu| gpu.as_secs_f64() * 1000.0),
            ),
            (true, _) => Bound::Unknown,
        };
        Some(Summary {
            fps,
            low_fps,
            bound,
            cpu: average(|s| s.game.total + s.render.cpu),
            cpu_peak: self
                .samples
                .iter()
                .map(|s| s.game.total + s.render.cpu)
                .max()
                .unwrap_or_default(),
            gpu: mean(&gpu),
            gpu_timing: newest.render.stats.gpu_timing,
            passes: FramePass::ALL.map(|pass| {
                let times: Vec<Duration> = self
                    .samples
                    .iter()
                    .filter_map(|s| s.render.stats.pass_time(pass))
                    .collect();
                mean(&times)
            }),
            cascades: std::array::from_fn(|cascade| {
                let times: Vec<Duration> = self
                    .samples
                    .iter()
                    .filter_map(|s| s.render.stats.cascade_times[cascade])
                    .collect();
                mean(&times)
            }),
            memory: self.memory,
            lua_bytes: newest.load.lua_bytes,
            scripts: average(|s| s.game.scripts),
            animation: average(|s| s.game.animation),
            posing: PosingThreads(std::array::from_fn(|slot| {
                self.samples
                    .iter()
                    .map(|s| s.game.posing.0[slot])
                    .sum::<Duration>()
                    / count
            })),
            physics: average(|s| s.game.physics),
            physics_steps: self
                .samples
                .iter()
                .map(|s| f64::from(s.game.physics_steps))
                .sum::<f64>()
                / f64::from(count),
            bodies: newest.load.bodies,
            render_cpu: average(|s| s.render.cpu),
            stats: newest.render.stats,
            lights: newest.render.lights,
            audio: average(|s| s.game.audio),
            sounds_playing: newest.load.sounds_playing,
            history: self
                .samples
                .iter()
                .map(|s| (s.game.total + s.render.cpu).as_secs_f32() * 1000.0)
                .collect(),
        })
    }

    /// The panel.
    pub fn show(&mut self, ui: &mut egui::Ui) {
        let palette = theme::palette(ui.ctx());
        let (status, color) = match self.state {
            State::Empty => ("Not playing", palette.text_weak),
            State::Live => ("Live", palette.success),
            State::Frozen => ("Stopped: last Play's readings", palette.text_weak),
        };
        ui.horizontal_wrapped(|ui| {
            widgets::segmented(
                ui,
                &mut self.tab,
                &[
                    (Tab::Game, None, "Game"),
                    (Tab::Graphics, None, "Graphics"),
                    (Tab::Memory, None, "Memory"),
                ],
            );
            ui.label(RichText::new(theme::with_icon(theme::icons::CIRCLE, status)).color(color));
        });
        let summary = self.summary();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let Some(summary) = &summary else {
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(
                            "Press Play to profile the game. Readings average the last two \
                             seconds.",
                        )
                        .color(palette.text_secondary),
                    );
                    if self.tab == Tab::Graphics {
                        self.shaders(ui, None);
                    }
                    return;
                };
                ui.add_space(4.0);
                history(ui, &summary.history);
                match self.tab {
                    Tab::Game => self.game(ui, summary),
                    Tab::Graphics => self.graphics(ui, summary),
                    Tab::Memory => self.memory(ui, summary),
                }
            });
    }

    /// The Game tab.
    fn game(&self, ui: &mut egui::Ui, summary: &Summary) {
        section(ui, "Frame", |ui| {
            row(
                ui,
                "Frame rate",
                summary
                    .fps
                    .map_or("-".into(), |fps| format!("{fps:.0} fps")),
            );
            row(
                ui,
                "1% low",
                summary
                    .low_fps
                    .map_or("-".into(), |fps| format!("{fps:.0} fps")),
            );
            row(
                ui,
                "CPU time",
                format!(
                    "{} (peak {})",
                    millis(summary.cpu),
                    millis(summary.cpu_peak)
                ),
            );
            row(ui, "GPU time", gpu_text(summary.gpu, summary.gpu_timing));
        });
        section(ui, "Memory", |ui| {
            row(
                ui,
                "Editor process",
                summary.memory.map_or("-".into(), bytes),
            );
            row(ui, "Lua", bytes(summary.lua_bytes));
        });
        section(ui, "Physics", |ui| {
            row(ui, "CPU", millis(summary.physics));
            row(
                ui,
                "Steps per frame",
                format!("{:.1}", summary.physics_steps),
            );
            row(ui, "Bodies", summary.bodies.to_string());
        });
        section(ui, "Audio", |ui| {
            row(ui, "CPU", millis(summary.audio));
            row(ui, "Sounds playing", summary.sounds_playing.to_string());
        });
        section(ui, "Scripts and animation", |ui| {
            row(ui, "Scripts", millis(summary.scripts));
            row(ui, "Animation", millis(summary.animation));
        });
        // Where skeletons were posed: the main thread for a few, the
        // animation pool's threads for a crowd (unified Phase 93).
        let posing: Vec<(usize, Duration)> = summary
            .posing
            .0
            .iter()
            .copied()
            .enumerate()
            .filter(|(_, time)| !time.is_zero())
            .collect();
        if !posing.is_empty() {
            section(ui, "Posing by thread", |ui| {
                for (slot, time) in posing {
                    row(ui, &PosingThreads::name(slot), millis(time));
                }
            });
        }
        section(ui, "Network", |ui| {
            if self.network.is_empty() {
                row(ui, "Clients", "1 (no connections)".into());
            }
            for client in &self.network {
                row(
                    ui,
                    &client.player,
                    format!("{:.0} ms ping", client.ping * 1000.0),
                );
                row(ui, "  Sent", format!("{}/s", bytes(client.sent as usize)));
                row(
                    ui,
                    "  Received",
                    format!("{}/s", bytes(client.received as usize)),
                );
                row(ui, "  Packet loss", format!("{:.1}%", client.loss * 100.0));
                row(ui, "  Instances", client.instances.to_string());
            }
        });
    }

    /// The Graphics tab: the viewport's newest [`RenderStats`], with times
    /// averaged like the Game tab's.
    fn graphics(&self, ui: &mut egui::Ui, summary: &Summary) {
        let stats = &summary.stats;
        section(ui, "Frame", |ui| {
            row(
                ui,
                "CPU frame",
                format!(
                    "{} (peak {})",
                    millis(summary.cpu),
                    millis(summary.cpu_peak)
                ),
            );
            row(ui, "Rendering CPU", millis(summary.render_cpu));
            row(ui, "GPU frame", gpu_text(summary.gpu, summary.gpu_timing));
            row(ui, "Limited by", summary.bound.label().to_owned());
        });
        section(ui, "GPU passes", |ui| {
            if summary.gpu_timing {
                for (pass, time) in FramePass::ALL.iter().zip(summary.passes) {
                    row(ui, pass.label(), time.map_or("off".into(), millis));
                    if *pass == FramePass::Shadows {
                        // Within Shadows, each of the sun's cascades it drew.
                        for (cascade, time) in summary.cascades.iter().enumerate() {
                            if let Some(time) = time {
                                row(ui, &format!("  Sun cascade {}", cascade + 1), millis(*time));
                            }
                        }
                    }
                }
            } else {
                row(ui, "Per pass", "not available on this GPU".into());
            }
        });
        section(ui, "Draws", |ui| {
            row(ui, "Draw calls", stats.draw_calls.to_string());
            row(ui, "Batches", stats.batches.to_string());
            row(ui, "Triangles", thousands(stats.triangles));
            row(ui, "Vertices", thousands(stats.vertices));
        });
        section(ui, "Culling", |ui| {
            row(
                ui,
                "Items drawn",
                format!("{} of {}", stats.items_drawn, stats.items_submitted),
            );
            row(ui, "Items culled", stats.items_culled.to_string());
            row(
                ui,
                "Shadow casters",
                format!(
                    "{} drawn, {} culled",
                    stats.shadow_casters_drawn, stats.shadow_casters_culled
                ),
            );
            row(ui, "Sprites culled", stats.sprites_culled.to_string());
            row(ui, "GPU particles", thousands(stats.gpu_particles.into()));
            row(ui, "Lights", summary.lights.to_string());
        });
        section(ui, "Levels of detail", |ui| {
            for (name, count) in LOD_NAMES.iter().zip(stats.lod_items) {
                row(ui, name, count.to_string());
            }
        });
        section(ui, "GPU memory", |ui| {
            let kinds = [
                ("Textures", stats.texture_bytes),
                ("Meshes", stats.mesh_bytes),
                ("Render targets", stats.target_bytes),
                ("Buffers", stats.buffer_bytes),
            ];
            for (kind, count) in kinds {
                row(ui, kind, bytes(count as usize));
            }
            let total = kinds.iter().map(|(_, count)| count).sum::<u64>();
            row(ui, "Total", bytes(total as usize));
        });
        self.shaders(ui, Some(stats));
    }

    /// The Memory tab: the process, what the game holds by kind (CPU),
    /// the GPU's share, and the classes with the most bytes.
    fn memory(&self, ui: &mut egui::Ui, summary: &Summary) {
        let stats = &summary.stats;
        let gpu = stats.texture_bytes + stats.mesh_bytes + stats.target_bytes + stats.buffer_bytes;
        section(ui, "Process", |ui| {
            row(
                ui,
                "Editor process",
                summary.memory.map_or("-".into(), bytes),
            );
            if let Some(breakdown) = &self.breakdown {
                row(ui, "Game (counted)", bytes(breakdown.total()));
            }
            row(ui, "GPU", bytes(gpu as usize));
        });
        let Some(breakdown) = &self.breakdown else {
            return;
        };
        section(ui, "Game, by kind", |ui| {
            for (name, count, size) in breakdown.rows() {
                let value = match count {
                    Some(count) => format!("{} ({})", bytes(size), thousands(count as u64)),
                    None => bytes(size),
                };
                row(ui, name, value);
            }
        });
        section(ui, "Instances by class", |ui| {
            for (class, used) in breakdown.instances.classes.iter().take(MEMORY_CLASSES) {
                row(
                    ui,
                    class.as_str(),
                    format!("{} ({})", bytes(used.bytes), thousands(used.count as u64)),
                );
            }
            let more = breakdown
                .instances
                .classes
                .len()
                .saturating_sub(MEMORY_CLASSES);
            if more > 0 {
                row(ui, "Other classes", more.to_string());
            }
        });
        let palette = theme::palette(ui.ctx());
        ui.add_space(6.0);
        ui.label(
            RichText::new(
                "Estimates of what each part holds; the process also has the editor, \
                 fonts and the allocator's spare room.",
            )
            .color(palette.text_weak)
            .size(type_scale::CAPTION),
        );
    }

    /// The surface shaders compiled, once a frame was drawn, and the
    /// messages of those that didn't compile.
    fn shaders(&self, ui: &mut egui::Ui, stats: Option<&RenderStats>) {
        section(ui, "Shaders", |ui| {
            if let Some(stats) = stats {
                row(ui, "Surface shaders", stats.surface_shaders.to_string());
                row(ui, "Not compiling", stats.broken_shaders.to_string());
            }
            if self.shader_messages.is_empty() {
                row(ui, "Messages", "none".into());
            }
        });
        let palette = theme::palette(ui.ctx());
        for message in &self.shader_messages {
            ui.label(RichText::new(message).color(palette.error));
        }
    }
}

/// The Graphics tab's names for [`RenderStats::lod_items`]' levels.
const LOD_NAMES: [&str; LOD_LEVELS] = ["Full detail", "Level 1", "Level 2"];

/// The readings the panel shows.
struct Summary {
    fps: Option<f64>,
    /// The average frame rate of the slowest 1% of the kept frames.
    low_fps: Option<f64>,
    /// What limits the frame: the GPU, the CPU (game and drawing), or
    /// neither (the editor's own UI, or the display).
    bound: Bound,
    cpu: Duration,
    cpu_peak: Duration,
    gpu: Option<Duration>,
    gpu_timing: bool,
    /// Each pass's average GPU time, in [`FramePass::ALL`]'s order: `None`
    /// for one no recent frame drew.
    passes: [Option<Duration>; FramePass::COUNT],
    /// Each sun cascade's average GPU time, nearest first: `None` for one
    /// no recent frame drew.
    cascades: [Option<Duration>; SHADOW_CASCADE_COUNT],
    memory: Option<usize>,
    lua_bytes: usize,
    scripts: Duration,
    animation: Duration,
    /// Average posing time per thread.
    posing: PosingThreads,
    physics: Duration,
    physics_steps: f64,
    bodies: usize,
    render_cpu: Duration,
    /// The newest frame's counts.
    stats: RenderStats,
    lights: usize,
    audio: Duration,
    sounds_playing: usize,
    /// Each frame's CPU time in milliseconds, oldest first.
    history: Vec<f32>,
}

fn section(ui: &mut egui::Ui, title: &str, rows: impl FnOnce(&mut egui::Ui)) {
    ui.add_space(8.0);
    widgets::section_label(ui, title);
    ui.add_space(2.0);
    responsive::form(ui, ("profiler", title), rows);
}

fn row(ui: &mut egui::Ui, name: &str, value: String) {
    let palette = theme::palette(ui.ctx());
    ui.label(RichText::new(name).color(palette.text_secondary));
    ui.label(
        RichText::new(value)
            .family(egui::FontFamily::Monospace)
            .size(type_scale::BODY - 1.0)
            .color(palette.text),
    );
    ui.end_row();
}

/// Each recent frame's CPU time as a bar, scaled to the slowest, with a
/// line at 16.7 ms (60 fps) when it's in range.
fn history(ui: &mut egui::Ui, frames: &[f32]) {
    let palette = theme::palette(ui.ctx());
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 40.0), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 3.0, palette.inset);
    let budget = 1000.0 / 60.0;
    let top = frames.iter().copied().fold(budget, f32::max);
    let bar = rect.width() / WINDOW as f32;
    // The newest frame is at the right.
    let start = rect.right() - bar * frames.len() as f32;
    for (index, &ms) in frames.iter().enumerate() {
        let height = rect.height() * (ms / top).clamp(0.0, 1.0);
        let left = start + bar * index as f32;
        let color = if ms > budget {
            palette.warning
        } else {
            palette.accent
        };
        painter.rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(left, rect.bottom() - height),
                egui::pos2(left + bar.max(1.0), rect.bottom()),
            ),
            0.0,
            color,
        );
    }
    let y = rect.bottom() - rect.height() * budget / top;
    painter.hline(
        rect.x_range(),
        y,
        egui::Stroke::new(1.0_f32, palette.border_strong),
    );
}

/// The average of `times`, or `None` if there are none.
fn mean(times: &[Duration]) -> Option<Duration> {
    (!times.is_empty()).then(|| times.iter().sum::<Duration>() / times.len() as u32)
}

fn gpu_text(gpu: Option<Duration>, timing: bool) -> String {
    match gpu {
        Some(gpu) => millis(gpu),
        None if timing => "measuring...".into(),
        None => "not available on this GPU".into(),
    }
}

fn millis(time: Duration) -> String {
    format!("{:.2} ms", time.as_secs_f64() * 1000.0)
}

fn bytes(count: usize) -> String {
    let mb = count as f64 / (1024.0 * 1024.0);
    if mb >= 1024.0 {
        format!("{:.2} GB", mb / 1024.0)
    } else if mb >= 1.0 {
        format!("{mb:.1} MB")
    } else {
        format!("{:.0} KB", count as f64 / 1024.0)
    }
}

/// `1234567` as `1,234,567`.
fn thousands(count: u64) -> String {
    let digits = count.to_string();
    let mut out = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(ms: u64) -> FrameCost {
        FrameCost {
            physics: Duration::from_millis(ms),
            total: Duration::from_millis(ms),
            ..FrameCost::default()
        }
    }

    #[test]
    fn readings_start_at_play_follow_the_load_and_freeze_at_stop() {
        let mut profiler = Profiler::default();
        // Nothing is recorded outside Play.
        profiler.record_game(frame(1), GameLoad::default());
        assert!(profiler.summary().is_none());

        profiler.start();
        let light = GameLoad {
            bodies: 2,
            ..GameLoad::default()
        };
        for _ in 0..WINDOW {
            profiler.record_game(frame(1), light);
            profiler.record_render(RenderCost {
                lights: 2,
                ..RenderCost::default()
            });
        }
        let summary = profiler.summary().unwrap();
        assert_eq!(summary.physics, Duration::from_millis(1));
        assert_eq!((summary.bodies, summary.lights), (2, 2));

        // A heavier scene raises the numbers, and they drop back after.
        let heavy = GameLoad {
            bodies: 500,
            ..GameLoad::default()
        };
        for _ in 0..WINDOW {
            profiler.record_game(frame(9), heavy);
        }
        let summary = profiler.summary().unwrap();
        assert_eq!(summary.physics, Duration::from_millis(9));
        assert_eq!(summary.bodies, 500);
        for _ in 0..WINDOW {
            profiler.record_game(frame(1), light);
        }
        assert_eq!(
            profiler.summary().unwrap().physics,
            Duration::from_millis(1)
        );

        // Stop freezes the readings; frames after it don't change them.
        profiler.stop();
        assert!(!profiler.is_live());
        profiler.record_game(frame(50), heavy);
        assert_eq!(profiler.summary().unwrap().bodies, 2);
        // The next Play starts again from nothing.
        profiler.start();
        assert!(profiler.summary().is_none());
    }

    /// Each piece of text the panel shows, in the order it's drawn: a
    /// row's name is just before its value.
    fn shown_texts(profiler: &mut Profiler) -> Vec<String> {
        let ctx = theme::test_context();
        // Tall enough that nothing is scrolled out of view.
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(400.0, 2000.0),
            )),
            ..egui::RawInput::default()
        };
        let output = crate::theme::run(&ctx, input, |ui| {
            egui::CentralPanel::default().show(ui, |ui| profiler.show(ui));
        });
        output
            .shapes
            .into_iter()
            .filter_map(|clipped| match clipped.shape {
                egui::Shape::Text(shape) => Some(shape.galley.text().to_owned()),
                _ => None,
            })
            .collect()
    }

    /// Every piece of text the panel shows.
    fn shown_text(profiler: &mut Profiler) -> String {
        shown_texts(profiler).join("\n")
    }

    fn timed_render(scene_ms: u64) -> RenderCost {
        let mut stats = RenderStats {
            gpu_timing: true,
            gpu_time: Some(Duration::from_millis(scene_ms + 1)),
            ..RenderStats::default()
        };
        stats.pass_times[FramePass::Scene as usize] = Some(Duration::from_millis(scene_ms));
        stats.pass_times[FramePass::PostProcessing as usize] = Some(Duration::from_millis(1));
        RenderCost {
            stats,
            ..RenderCost::default()
        }
    }

    #[test]
    fn the_panel_lists_each_passs_gpu_time_or_says_it_cant() {
        let mut profiler = Profiler::default();
        profiler.start();
        for scene_ms in [2, 4] {
            profiler.record_game(frame(1), GameLoad::default());
            profiler.record_render(timed_render(scene_ms));
        }
        // Averaged over the frames that drew each pass.
        let summary = profiler.summary().unwrap();
        assert_eq!(
            summary.passes[FramePass::Scene as usize],
            Some(Duration::from_millis(3))
        );
        assert_eq!(summary.passes[FramePass::Shadows as usize], None);
        profiler.tab = Tab::Graphics;
        let text = shown_text(&mut profiler);
        for pass in FramePass::ALL {
            assert!(text.contains(pass.label()), "{pass:?} in {text}");
        }
        assert!(text.contains("3.00 ms"), "{text}");
        assert!(!text.contains("not available"), "{text}");

        // A GPU without timestamps: the panel says so, once, for every pass.
        profiler.start();
        profiler.record_game(frame(1), GameLoad::default());
        profiler.record_render(RenderCost::default());
        let text = shown_text(&mut profiler);
        assert!(text.contains("Per pass"), "{text}");
        assert!(!text.contains(FramePass::Scene.label()), "{text}");
    }

    /// The Graphics tab's rows for `stats`, as they should read.
    fn graphics_rows(stats: &RenderStats) -> Vec<(&'static str, String)> {
        let total =
            stats.texture_bytes + stats.mesh_bytes + stats.target_bytes + stats.buffer_bytes;
        vec![
            ("Draw calls", stats.draw_calls.to_string()),
            ("Batches", stats.batches.to_string()),
            ("Triangles", thousands(stats.triangles)),
            ("Vertices", thousands(stats.vertices)),
            (
                "Items drawn",
                format!("{} of {}", stats.items_drawn, stats.items_submitted),
            ),
            ("Items culled", stats.items_culled.to_string()),
            (
                "Shadow casters",
                format!(
                    "{} drawn, {} culled",
                    stats.shadow_casters_drawn, stats.shadow_casters_culled
                ),
            ),
            ("Sprites culled", stats.sprites_culled.to_string()),
            ("GPU particles", thousands(stats.gpu_particles.into())),
            ("Full detail", stats.lod_items[0].to_string()),
            ("Level 1", stats.lod_items[1].to_string()),
            ("Level 2", stats.lod_items[2].to_string()),
            ("Textures", bytes(stats.texture_bytes as usize)),
            ("Meshes", bytes(stats.mesh_bytes as usize)),
            ("Render targets", bytes(stats.target_bytes as usize)),
            ("Buffers", bytes(stats.buffer_bytes as usize)),
            ("Total", bytes(total as usize)),
            ("Surface shaders", stats.surface_shaders.to_string()),
            ("Not compiling", stats.broken_shaders.to_string()),
        ]
    }

    /// Asserts the Graphics tab shows each of `stats`' numbers on its row.
    fn assert_graphics_tab_shows(profiler: &mut Profiler, stats: &RenderStats) {
        profiler.tab = Tab::Graphics;
        let texts = shown_texts(profiler);
        for (name, value) in graphics_rows(stats) {
            let at = texts.iter().position(|text| text == name);
            let shown = at.and_then(|at| texts.get(at + 1));
            assert_eq!(shown, Some(&value), "{name} in {texts:?}");
        }
    }

    #[test]
    fn the_graphics_tab_shows_every_render_stat_on_its_own_row() {
        // Every number different, so a row showing the wrong one fails.
        let stats = RenderStats {
            draw_calls: 11,
            triangles: 1_234_567,
            vertices: 2_345_678,
            items_submitted: 40,
            items_drawn: 31,
            batches: 7,
            items_culled: 9,
            shadow_casters_drawn: 21,
            shadow_casters_culled: 13,
            sprites_culled: 5,
            gpu_particles: 100_000,
            lod_items: [3, 4, 6],
            surface_shaders: 2,
            broken_shaders: 1,
            texture_bytes: 3 << 20,
            mesh_bytes: 5 << 20,
            target_bytes: 7 << 20,
            buffer_bytes: 512 << 10,
            ..RenderStats::default()
        };
        let mut profiler = Profiler::default();
        profiler.start();
        profiler.record_game(frame(1), GameLoad::default());
        profiler.record_render(RenderCost {
            stats,
            lights: 8,
            ..RenderCost::default()
        });
        assert_graphics_tab_shows(&mut profiler, &stats);
        let text = shown_text(&mut profiler);
        assert!(text.contains("Lights\n8"), "{text}");
        assert!(!text.contains("Bodies"), "the Game tab's rows stay there");

        // A newer frame's counts replace them.
        let busier = RenderStats {
            draw_calls: 90,
            triangles: 9_000,
            lod_items: [0, 0, 12],
            ..stats
        };
        profiler.record_game(frame(1), GameLoad::default());
        profiler.record_render(RenderCost {
            stats: busier,
            ..RenderCost::default()
        });
        assert_graphics_tab_shows(&mut profiler, &busier);

        profiler.tab = Tab::Game;
        let text = shown_text(&mut profiler);
        assert!(
            text.contains("Bodies") && !text.contains("Draw calls"),
            "{text}"
        );
    }

    #[test]
    fn shader_messages_are_listed_and_kept_across_plays() {
        let error = ShaderError {
            material: "assets/materials/broken.material.json".to_owned(),
            line: Some(4),
            column: Some(3),
            message: "expected `;`".to_owned(),
        };
        let mut profiler = Profiler {
            tab: Tab::Graphics,
            ..Profiler::default()
        };
        assert!(shown_text(&mut profiler).contains("none"));
        // Reported in edit mode, before Play.
        profiler.record_shader_errors(std::slice::from_ref(&error));
        let text = shown_text(&mut profiler);
        assert!(text.contains(&error.to_string()), "{text}");
        profiler.start();
        profiler.record_game(frame(1), GameLoad::default());
        let text = shown_text(&mut profiler);
        assert!(text.contains(&error.to_string()), "{text}");
        assert_eq!(profiler.tab, Tab::Graphics, "Play keeps the tab");
        // Only the newest are kept.
        for _ in 0..SHADER_MESSAGES * 2 {
            profiler.record_shader_errors(std::slice::from_ref(&error));
        }
        assert_eq!(profiler.shader_messages.len(), SHADER_MESSAGES);
    }

    #[test]
    fn posing_time_is_listed_by_thread_only_when_there_is_some() {
        let mut profiler = Profiler::default();
        profiler.start();
        profiler.record_game(frame(1), GameLoad::default());
        assert!(
            !shown_text(&mut profiler)
                .to_lowercase()
                .contains("posing by thread")
        );

        let times = vec![
            ("main".to_owned(), Duration::from_micros(300)),
            ("anim 0".to_owned(), Duration::from_millis(2)),
            ("anim 3".to_owned(), Duration::from_millis(1)),
        ];
        let posing = PosingThreads::of(&times);
        assert_eq!(posing.0[0], Duration::from_micros(300));
        assert_eq!(posing.0[1], Duration::from_millis(2));
        assert_eq!(posing.0[4], Duration::from_millis(1));
        // A thread past the slots counts in the last.
        let many = PosingThreads::of(&vec![("anim 40".to_owned(), Duration::from_millis(1))]);
        assert_eq!(many.0[PosingThreads::SLOTS - 1], Duration::from_millis(1));

        let mut profiler = Profiler::default();
        profiler.start();
        profiler.record_game(FrameCost { posing, ..frame(4) }, GameLoad::default());
        let text = shown_text(&mut profiler);
        assert!(text.to_lowercase().contains("posing by thread"), "{text}");
        assert!(text.contains("Main thread"), "{text}");
        assert!(text.contains("Thread 1"), "{text}");
        assert!(text.contains("Thread 4"), "{text}");
        assert!(!text.contains("Thread 2"), "{text}");
    }

    #[test]
    fn every_tab_fits_a_narrow_column() {
        let mut profiler = Profiler::default();
        profiler.start();
        profiler.record_memory(breakdown());
        profiler.record_game(frame(1), GameLoad::default());
        profiler.record_render(timed_render(2));
        profiler.record_shader_errors(&[ShaderError {
            material: "assets/materials/AVeryLongMaterialName.material.json".to_owned(),
            line: Some(12),
            column: None,
            message: "no definition in scope for identifier: `scrolled_uv_coordinates`".to_owned(),
        }]);
        let snapshots = std::env::var_os("AXCENITY_SNAPSHOTS").map(std::path::PathBuf::from);
        let size = egui::vec2(260.0, 1400.0);
        for (tab, name) in [
            (Tab::Game, "game"),
            (Tab::Graphics, "graphics"),
            (Tab::Memory, "memory"),
        ] {
            profiler.tab = tab;
            let mut fit = None;
            let mut draw = |ui: &mut egui::Ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    let room = ui.max_rect();
                    profiler.show(ui);
                    fit = Some((ui.min_rect(), room));
                });
            };
            let ctx = theme::test_context();
            for _ in 0..3 {
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    ..Default::default()
                };
                let _ = crate::theme::run(&ctx, input, |ui| draw(ui));
            }
            if let Some(folder) = &snapshots {
                let path = folder.join(format!("profiler-{name}.png"));
                crate::snapshot::snapshot(&path, size, vec![Vec::new(); 3], &mut draw);
            }
            let (used, room) = fit.expect("the panel was drawn");
            assert!(
                used.width() <= room.width() + 0.5,
                "{tab:?} drew {} points wide in {}",
                used.width(),
                room.width()
            );
        }
    }

    /// A breakdown with every row different, and more classes than are
    /// listed.
    fn breakdown() -> MemoryBreakdown {
        use axcenity_core::ClassName;
        use axcenity_core::memory::{InstanceMemory, MemoryUse};
        let used = |count, megabytes: usize| MemoryUse {
            count,
            bytes: megabytes << 20,
        };
        let classes: Vec<_> = ClassName::ALL
            .iter()
            .filter(|class| !class.is_service())
            .take(MEMORY_CLASSES + 3)
            .enumerate()
            .map(|(index, &class)| (class, used(100 - index, 40 - index)))
            .collect();
        MemoryBreakdown {
            lua: 3 << 20,
            instances: InstanceMemory {
                total: used(10_234, 21),
                classes,
            },
            models: used(4, 50),
            textures: used(12, 300),
            lighting: used(2, 9),
            sounds: used(6, 17),
            physics: used(10_001, 8),
        }
    }

    #[test]
    fn the_memory_tab_lists_each_kind_and_the_biggest_classes() {
        use axcenity_core::ClassName;
        let mut profiler = Profiler::default();
        assert!(!profiler.memory_due(), "not playing");
        profiler.start();
        assert!(profiler.memory_due(), "the first frame");
        profiler.record_memory(breakdown());
        profiler.record_game(frame(1), GameLoad::default());
        assert!(!profiler.memory_due(), "every few frames");
        profiler.record_render(RenderCost {
            stats: RenderStats {
                texture_bytes: 64 << 20,
                mesh_bytes: 32 << 20,
                ..RenderStats::default()
            },
            ..RenderCost::default()
        });
        profiler.tab = Tab::Memory;
        let text = shown_text(&mut profiler);
        for line in [
            "GPU\n96.0 MB",
            "Game (counted)\n408.0 MB",
            "Lua\n3.0 MB",
            "Instances\n21.0 MB (10,234)",
            "Models\n50.0 MB (4)",
            "Textures\n300.0 MB (12)",
            "Sky and probes\n9.0 MB (2)",
            "Sounds\n17.0 MB (6)",
            "Physics\n8.0 MB (10,001)",
            "Other classes\n3",
        ] {
            assert!(text.contains(line), "{line:?} in {text}");
        }
        let first = ClassName::ALL.iter().find(|c| !c.is_service()).unwrap();
        assert!(
            text.contains(&format!("{}\n40.0 MB (100)", first.as_str())),
            "{text}"
        );

        // Stopped, the last breakdown stays and no new one is taken.
        profiler.stop();
        assert!(!profiler.memory_due());
        profiler.record_memory(MemoryBreakdown::default());
        assert_eq!(profiler.breakdown(), Some(&breakdown()));
    }

    /// A world of `parts` parts in a row in front of the camera.
    fn world_of(parts: usize) -> axcenity_core::World {
        use axcenity_core::{ClassName, Value, Vector3};
        let mut world = axcenity_core::World::new();
        for index in 0..parts {
            let part = world.create(ClassName::Part).unwrap();
            let x = index as f32 * 3.0 - 10.0;
            world
                .set_property(part, "Position", Value::Vector3(Vector3::new(x, 0.0, 0.0)))
                .unwrap();
            world.set_parent(part, Some(world.workspace())).unwrap();
        }
        world
    }

    #[test]
    fn the_graphics_tab_follows_the_renderers_stats_as_the_scene_changes() {
        let instance = axcenity_render::adapters::instance();
        let Some((device, queue)) =
            pollster::block_on(axcenity_render::adapters::adapter(&instance, None))
                .ok()
                .and_then(|adapter| {
                    pollster::block_on(adapter.request_device(&Default::default())).ok()
                })
        else {
            eprintln!("skipped: no graphics adapter on this machine");
            return;
        };
        let mut renderer = axcenity_render::TextureRenderer::new(&device, &queue, 160, 90);
        // Parts need no files: an empty project folder.
        let assets = axcenity_assets::Assets::new(std::path::Path::new("no project"));
        let camera = axcenity_render::Camera {
            position: axcenity_render::glam::Vec3::new(0.0, 6.0, 25.0),
            look_at: axcenity_render::glam::Vec3::ZERO,
            fov_y_degrees: 70.0,
        };
        let mut profiler = Profiler::default();
        profiler.start();
        let mut shown = Vec::new();
        for parts in [1, 6] {
            let list = axcenity_scene::build_draw_list(&world_of(parts), &assets, camera);
            renderer.render(&list).unwrap();
            let stats = renderer.stats();
            profiler.record_game(frame(1), GameLoad::default());
            profiler.record_render(RenderCost {
                lights: list.lights.len(),
                stats,
                ..RenderCost::default()
            });
            assert_graphics_tab_shows(&mut profiler, &stats);
            shown.push(stats);
        }
        let [one, six] = [shown[0], shown[1]];
        assert_eq!((one.items_drawn, six.items_drawn), (1, 6));
        assert!(six.triangles > one.triangles && six.vertices > one.vertices);
    }

    #[test]
    fn numbers_read_as_people_write_them() {
        assert_eq!(thousands(1_234_567), "1,234,567");
        assert_eq!(thousands(999), "999");
        assert_eq!(bytes(512 * 1024), "512 KB");
        assert_eq!(bytes(3 * 1024 * 1024 / 2), "1.5 MB");
        assert_eq!(millis(Duration::from_micros(1500)), "1.50 ms");
    }

    #[test]
    fn the_network_section_shows_each_clients_ping_bandwidth_and_loss() {
        let mut profiler = Profiler::default();
        profiler.start();
        profiler.record_game(FrameCost::default(), GameLoad::default());
        profiler.record_network(vec![NetworkRow {
            player: "Player2".to_owned(),
            ping: 0.302,
            sent: 21_000.0,
            received: 4_000.0,
            loss: 0.098,
            instances: 412,
        }]);
        let ctx = crate::theme::test_context();
        let mut texts = Vec::new();
        for _ in 0..2 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(260.0, 2000.0),
                )),
                ..Default::default()
            };
            let output = crate::theme::run(&ctx, input, |ui| {
                egui::CentralPanel::default().show(ui, |ui| profiler.show(ui));
            });
            texts = output
                .shapes
                .iter()
                .filter_map(|clipped| match &clipped.shape {
                    egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
                    _ => None,
                })
                .collect();
        }
        for expected in ["NETWORK", "Player2", "302 ms ping", "9.8%", "412"] {
            assert!(
                texts.iter().any(|t| t == expected),
                "{expected} in {texts:?}"
            );
        }
    }
}
