//! The Developer Console during Play (Phase 71), as built games have it:
//! F9 over the viewport opens `axcenity_ui::DevConsole`, drawn over the
//! game. Its Log mirrors the Output panel's lines since Play started; the
//! built-in commands use the editor's own switches (`wireframe` is View >
//! View Mode's Wireframe, `collisions` the collider overlay); a game's
//! `ConsoleService` commands and Lua run in the Play session.

use std::collections::VecDeque;

use axcenity_render::UiDrawItem;
use axcenity_script::ConsoleRun;
use axcenity_ui::{BuiltIn, ConsoleKey, ConsoleStats, LogLevel};
use eframe::egui;

use super::EditorApp;
use crate::output::OutputKind;
use crate::viewport::ViewMode;

/// The frames the frame rate is averaged over.
const AVERAGED_FRAMES: usize = 60;

/// The console's state besides `DevConsole`: what its commands changed.
pub(super) struct DevPlay {
    pub console: axcenity_ui::DevConsole,
    /// Game time per real second (`timescale`).
    pub time_scale: f64,
    pub show_fps: bool,
    /// `fps full`: the readout with the drawing's numbers.
    pub fps_full: bool,
    /// The Output lines already in the console's Log.
    seen: usize,
    /// The last frames' lengths, in seconds, newest last.
    frames: VecDeque<f32>,
}

impl Default for DevPlay {
    fn default() -> Self {
        Self {
            console: axcenity_ui::DevConsole::default(),
            time_scale: 1.0,
            show_fps: false,
            fps_full: false,
            seen: 0,
            frames: VecDeque::new(),
        }
    }
}

fn console_key(key: egui::Key) -> Option<ConsoleKey> {
    Some(match key {
        egui::Key::Enter => ConsoleKey::Enter,
        egui::Key::Backspace => ConsoleKey::Backspace,
        egui::Key::Tab => ConsoleKey::Tab,
        egui::Key::ArrowUp => ConsoleKey::Up,
        egui::Key::ArrowDown => ConsoleKey::Down,
        egui::Key::PageUp => ConsoleKey::PageUp,
        egui::Key::PageDown => ConsoleKey::PageDown,
        egui::Key::Escape => ConsoleKey::Escape,
        _ => return None,
    })
}

impl EditorApp {
    /// F9 during Play, and typing while the console is open. True while
    /// it's open, so the game doesn't get the keys. `editing` is true while
    /// an editor control has the keyboard. Outside Play it starts over.
    pub(super) fn type_dev_console(&mut self, events: &[egui::Event], editing: bool) -> bool {
        if self.play.is_none() {
            let seen = self.output.total();
            self.dev = DevPlay {
                seen,
                ..DevPlay::default()
            };
            return false;
        }
        for event in events {
            match event {
                egui::Event::Key {
                    key,
                    pressed: true,
                    repeat: false,
                    modifiers,
                    ..
                } if (!editing || self.dev.console.open)
                    && crate::shortcuts::Action::DevConsole
                        .keys()
                        .iter()
                        .any(|chord| {
                            chord.logical_key == *key
                                && modifiers.matches_logically(chord.modifiers)
                        }) =>
                {
                    self.dev.console.toggle()
                }
                _ if !self.dev.console.open => {}
                egui::Event::Text(text) => self.dev.console.type_text(text),
                egui::Event::Key {
                    key, pressed: true, ..
                } => {
                    if let Some(key) = console_key(*key)
                        && let Some(line) = self.dev.console.key(key)
                    {
                        self.run_dev_line(&line);
                    }
                }
                _ => {}
            }
        }
        self.feed_dev_console();
        self.dev.console.open
    }

    /// Notes a Play frame `seconds` long, for the frame rate.
    pub(super) fn note_dev_frame(&mut self, seconds: f64) {
        if seconds > 0.0 {
            self.dev.frames.push_back(seconds as f32);
            while self.dev.frames.len() > AVERAGED_FRAMES {
                self.dev.frames.pop_front();
            }
        }
    }

    /// Moves the Output's new lines into the console's Log.
    fn feed_dev_console(&mut self) {
        for line in self.output.since(self.dev.seen) {
            let level = match line.kind {
                OutputKind::Info => LogLevel::Info,
                OutputKind::Warning => LogLevel::Warning,
                OutputKind::Error => LogLevel::Error,
                // The console echoes its own lines.
                OutputKind::Command => continue,
            };
            self.dev.console.log(level, None, line.text.clone());
        }
        self.dev.seen = self.output.total();
    }

    /// Runs a line typed in the console's Commands tab.
    pub(super) fn run_dev_line(&mut self, line: &str) {
        match axcenity_ui::built_in(line) {
            Some(Ok(command)) => self.run_dev_built_in(command),
            Some(Err(why)) => self.output.warning(why),
            None => {
                let Some(session) = &mut self.play else {
                    return;
                };
                match session.run_console_command(line, &mut self.output) {
                    Ok(ConsoleRun::Unknown) => {
                        let word = line.split_whitespace().next().unwrap_or_default();
                        if axcenity_ui::looks_like_command(line) && !session.console_knows(word) {
                            self.output.warning(format!(
                                "Unknown command `{word}`. Type help for the list."
                            ));
                        } else {
                            match session.run_command(line, &mut self.output) {
                                Ok(Ok(())) => {}
                                Ok(Err(message)) => self.output.error(message),
                                Err(crash) => self.crashed(crash),
                            }
                        }
                    }
                    Ok(_) => {}
                    Err(crash) => self.crashed(crash),
                }
            }
        }
        self.feed_dev_console();
    }

    fn run_dev_built_in(&mut self, command: BuiltIn) {
        let on_off = |on: bool| if on { "on" } else { "off" };
        match command {
            BuiltIn::Help => {
                self.output.info("Commands:");
                let mut lines: Vec<String> = axcenity_ui::BUILT_INS
                    .iter()
                    .map(|(name, help)| format!("  {name:<22} {help}"))
                    .collect();
                if let Some(session) = &self.play {
                    for command in session.console_commands() {
                        lines.push(format!("  {:<22} {}", command.name, command.help));
                    }
                }
                for line in lines {
                    self.output.info(line);
                }
                self.output.info("Anything else runs as Lua.");
            }
            BuiltIn::Clear => self.dev.console.clear(),
            BuiltIn::Fps { full } => {
                // The same command again hides it; the other mode switches.
                self.dev.show_fps = !(self.dev.show_fps && self.dev.fps_full == full);
                self.dev.fps_full = full;
                let mode = if full { "fps full" } else { "fps" };
                self.output
                    .info(format!("{mode} {}", on_off(self.dev.show_fps)));
            }
            BuiltIn::Wireframe => {
                let wireframe = ViewMode::Debug(axcenity_render::DebugView::Wireframe);
                self.view_mode = if self.view_mode == wireframe {
                    ViewMode::Lit
                } else {
                    wireframe
                };
                let on = self.view_mode == wireframe;
                self.output.info(format!("wireframe {}", on_off(on)));
            }
            BuiltIn::Collisions => {
                self.overlays.colliders = !self.overlays.colliders;
                self.output
                    .info(format!("collisions {}", on_off(self.overlays.colliders)));
            }
            BuiltIn::Noclip => {
                if let Some(session) = &mut self.play {
                    let on = session.toggle_noclip();
                    self.output.info(format!("noclip {}", on_off(on)));
                }
            }
            BuiltIn::Timescale(scale) => {
                self.dev.time_scale = f64::from(scale);
                self.output.info(format!("timescale {scale}"));
            }
            BuiltIn::Teleport(to) => {
                let moved = self
                    .play
                    .as_mut()
                    .is_some_and(|session| session.teleport(to));
                if !moved {
                    let (_, orientation) = self.viewport.camera_view();
                    self.viewport.set_camera_view(to, orientation);
                }
                self.output
                    .info(format!("teleported to {}, {}, {}", to.x, to.y, to.z));
            }
            // The editor catches it and stops Play, as a game crash.
            BuiltIn::Crash => {
                if let Some(session) = &mut self.play {
                    session.panic_next_frame();
                }
            }
        }
    }

    /// The console's Stats tab now.
    fn dev_stats(&self) -> ConsoleStats {
        let frames = &self.dev.frames;
        let average = if frames.is_empty() {
            0.0
        } else {
            frames.iter().sum::<f32>() / frames.len() as f32
        };
        let mut stats = ConsoleStats {
            fps: if average > 0.0 { 1.0 / average } else { 0.0 },
            frame_ms: frames.back().copied().unwrap_or(0.0) * 1000.0,
            memory_bytes: memory_stats::memory_stats().map(|stats| stats.physical_mem as u64),
            ..ConsoleStats::default()
        };
        if let Some(session) = &self.play {
            if let Some(cost) = session.cost() {
                let ms = |time: std::time::Duration| time.as_secs_f32() * 1000.0;
                stats.stages = vec![
                    ("Scripts", ms(cost.scripts)),
                    ("Physics", ms(cost.physics)),
                    ("Audio", ms(cost.audio)),
                    ("Animation", ms(cost.animation)),
                ];
            }
            stats.lua_bytes = session.load().lua_bytes;
            stats.instances = session.world().len();
            stats.breakdown = self.profiler.breakdown().cloned();
        }
        if let Some(readout) = self.profiler.readout() {
            stats.low_fps = readout.low_fps;
            stats.cpu_ms = readout.cpu_ms;
            stats.gpu_ms = readout.gpu_ms;
            stats.bound = readout.bound;
            stats.detail = axcenity_ui::render_lines(&readout.stats);
        }
        stats
    }

    /// What the console draws over the game in the viewport: the `fps`
    /// readout and the console itself, while playing.
    pub(super) fn dev_console_items(&self) -> Vec<UiDrawItem> {
        if self.play.is_none() {
            return Vec::new();
        }
        let (width, height) = self.viewport.pixel_size();
        let stats = self.dev_stats();
        let mut items = axcenity_ui::console_items(&self.dev.console, &stats, width, height);
        if self.dev.show_fps {
            items.extend(axcenity_ui::fps_items(
                &stats,
                &self.dev.console,
                width,
                height,
                self.dev.fps_full,
            ));
        }
        items
    }
}
