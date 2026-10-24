//! The Profiler panel: what the game costs while it plays, from the
//! measurements Play's frame loop (`Session::frame`) and the viewport
//! already take. The panel only reads them, so closing it changes nothing
//! in the game.
//!
//! Readings start fresh at each Play, update every frame, and freeze at
//! Stop. The player has no profiler.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use axcenity_render::RenderStats;
use eframe::egui::{self, RichText};

use crate::responsive;
use crate::theme::{self, type_scale};
use crate::widgets;

/// How many frames the readings average over: about two seconds at 60 fps.
const WINDOW: usize = 120;
/// Asking the OS for the process's memory every frame would be wasted
/// work; every this many frames is plenty.
const MEMORY_EVERY: u64 = 15;

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
    /// The whole game frame, steps 1 to 4.
    pub total: Duration,
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
    /// Parts, meshes and terrain chunks in the draw list.
    pub items: usize,
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

#[derive(Default)]
pub struct Profiler {
    samples: VecDeque<Sample>,
    state: State,
    last_frame: Option<Instant>,
    frames: u64,
    /// The process's resident memory, as last asked; `None` where the OS
    /// doesn't say.
    memory: Option<usize>,
}

impl Profiler {
    /// Play started: the readings start again.
    pub fn start(&mut self) {
        *self = Profiler {
            state: State::Live,
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
        Some(Summary {
            fps,
            cpu: average(|s| s.game.total + s.render.cpu),
            cpu_peak: self
                .samples
                .iter()
                .map(|s| s.game.total + s.render.cpu)
                .max()
                .unwrap_or_default(),
            gpu: (!gpu.is_empty()).then(|| gpu.iter().sum::<Duration>() / gpu.len() as u32),
            gpu_timing: newest.render.stats.gpu_timing,
            memory: self.memory,
            lua_bytes: newest.load.lua_bytes,
            scripts: average(|s| s.game.scripts),
            animation: average(|s| s.game.animation),
            physics: average(|s| s.game.physics),
            physics_steps: self
                .samples
                .iter()
                .map(|s| f64::from(s.game.physics_steps))
                .sum::<f64>()
                / f64::from(count),
            bodies: newest.load.bodies,
            render_cpu: average(|s| s.render.cpu),
            draw_calls: newest.render.stats.draw_calls,
            triangles: newest.render.stats.triangles,
            items: newest.render.items,
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
    pub fn show(&self, ui: &mut egui::Ui) {
        let palette = theme::palette(ui.ctx());
        let (status, color) = match self.state {
            State::Empty => ("Not playing", palette.text_weak),
            State::Live => ("Live", palette.success),
            State::Frozen => ("Stopped: last Play's readings", palette.text_weak),
        };
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(theme::with_icon(theme::icons::CIRCLE, status)).color(color));
        });
        let Some(summary) = self.summary() else {
            ui.add_space(6.0);
            ui.label(
                RichText::new(
                    "Press Play to profile the game. Readings average the last two seconds.",
                )
                .color(palette.text_secondary),
            );
            return;
        };
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add_space(4.0);
                history(ui, &summary.history);
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
                section(ui, "Rendering", |ui| {
                    row(ui, "CPU", millis(summary.render_cpu));
                    row(ui, "GPU", gpu_text(summary.gpu, summary.gpu_timing));
                    row(ui, "Draw calls", summary.draw_calls.to_string());
                    row(ui, "Triangles", thousands(summary.triangles));
                    row(ui, "Items", summary.items.to_string());
                    row(ui, "Lights", summary.lights.to_string());
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
            });
    }
}

/// The readings the panel shows.
struct Summary {
    fps: Option<f64>,
    cpu: Duration,
    cpu_peak: Duration,
    gpu: Option<Duration>,
    gpu_timing: bool,
    memory: Option<usize>,
    lua_bytes: usize,
    scripts: Duration,
    animation: Duration,
    physics: Duration,
    physics_steps: f64,
    bodies: usize,
    render_cpu: Duration,
    draw_calls: u32,
    triangles: u64,
    items: usize,
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
                items: 2,
                ..RenderCost::default()
            });
        }
        let summary = profiler.summary().unwrap();
        assert_eq!(summary.physics, Duration::from_millis(1));
        assert_eq!((summary.bodies, summary.items), (2, 2));

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

    #[test]
    fn numbers_read_as_people_write_them() {
        assert_eq!(thousands(1_234_567), "1,234,567");
        assert_eq!(thousands(999), "999");
        assert_eq!(bytes(512 * 1024), "512 KB");
        assert_eq!(bytes(3 * 1024 * 1024 / 2), "1.5 MB");
        assert_eq!(millis(Duration::from_micros(1500)), "1.50 ms");
    }
}
