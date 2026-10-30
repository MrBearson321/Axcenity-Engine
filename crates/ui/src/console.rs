//! The Developer Console (Phase 71): F9 in the player's development runs
//! and in games built with it turned on. Drawn over everything as draw
//! items, like the chat window, with three tabs:
//!
//! - **Log**: what scripts and the engine said, filtered by level (`L`
//!   cycles all, warnings and errors, errors only) and side (`S` cycles
//!   both, server, client).
//! - **Stats**: the frame rate, each frame step's time, memory, the
//!   Instance count and, in multiplayer, ping and bandwidth
//!   ([`ConsoleStats`], which the host measures).
//! - **Commands**: a line to type in, over the log. The built-in commands
//!   ([`BUILT_INS`], read by [`built_in`]) are the host's; anything else
//!   is a game's `ConsoleService` command or Lua.
//!
//! `Tab` switches tabs, `Up` and `Down` go through the lines typed before,
//! `PageUp` and `PageDown` scroll the log, and `Escape` or F9 close it.
//! [`DevConsole`] keeps the state between frames; [`console_items`] draws it.

use std::collections::VecDeque;

use axcenity_core::Vector3;
use axcenity_render::{TextAlign, UiDrawItem, UiRectangle, UiText, VerticalAlign};

/// The log keeps this many lines; older ones go.
pub const CONSOLE_LOG_LINES: usize = 500;
/// The lines typed before that `Up` goes back through.
const HISTORY: usize = 50;
const FONT: &str = "JetBrains Mono";
const FONT_SIZE: f32 = 14.0;
const LINE_HEIGHT: f32 = 19.0;
const PADDING: f32 = 10.0;
/// How much of the window's height the console covers.
const HEIGHT_SHARE: f32 = 0.55;

/// Opaque: blended in linear light, even a little of a bright game
/// showing through tints the panel and makes its text harder to read.
const BACKGROUND: [f32; 4] = [0.06, 0.06, 0.08, 1.0];
/// The `fps` readout's box, which the game shows through.
const READOUT: [f32; 4] = [0.06, 0.06, 0.08, 0.85];
const BAR: [f32; 4] = [0.12, 0.12, 0.16, 1.0];
const ACTIVE_TAB: [f32; 4] = [0.22, 0.42, 0.85, 1.0];
const TEXT: [f32; 4] = [0.92, 0.92, 0.94, 1.0];
const DIM: [f32; 4] = [0.6, 0.62, 0.68, 1.0];
const WARNING: [f32; 4] = [1.0, 0.78, 0.3, 1.0];
const ERROR: [f32; 4] = [1.0, 0.42, 0.4, 1.0];
const COMMAND: [f32; 4] = [0.55, 0.8, 1.0, 1.0];

/// The console's tabs, in order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ConsoleTab {
    Log,
    Stats,
    #[default]
    Commands,
}

impl ConsoleTab {
    pub const ALL: [ConsoleTab; 3] = [ConsoleTab::Log, ConsoleTab::Stats, ConsoleTab::Commands];

    pub fn label(self) -> &'static str {
        match self {
            ConsoleTab::Log => "Log",
            ConsoleTab::Stats => "Stats",
            ConsoleTab::Commands => "Commands",
        }
    }
}

/// How serious a log line is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Info,
    Warning,
    Error,
    /// A line typed in the Commands tab, echoed.
    Command,
}

/// Where a log line came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogSide {
    Server,
    Client,
}

/// One line of the log. `side` is `None` for the engine's own messages,
/// which every side filter shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogEntry {
    pub level: LogLevel,
    pub side: Option<LogSide>,
    pub text: String,
}

/// Which levels the Log tab shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LevelFilter {
    #[default]
    All,
    WarningsAndErrors,
    Errors,
}

/// Which side the Log tab shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SideFilter {
    #[default]
    Both,
    Server,
    Client,
}

impl LevelFilter {
    fn next(self) -> Self {
        match self {
            LevelFilter::All => LevelFilter::WarningsAndErrors,
            LevelFilter::WarningsAndErrors => LevelFilter::Errors,
            LevelFilter::Errors => LevelFilter::All,
        }
    }

    fn label(self) -> &'static str {
        match self {
            LevelFilter::All => "all levels",
            LevelFilter::WarningsAndErrors => "warnings and errors",
            LevelFilter::Errors => "errors",
        }
    }

    fn shows(self, level: LogLevel) -> bool {
        match self {
            LevelFilter::All => true,
            LevelFilter::WarningsAndErrors => matches!(level, LogLevel::Warning | LogLevel::Error),
            LevelFilter::Errors => level == LogLevel::Error,
        }
    }
}

impl SideFilter {
    fn next(self) -> Self {
        match self {
            SideFilter::Both => SideFilter::Server,
            SideFilter::Server => SideFilter::Client,
            SideFilter::Client => SideFilter::Both,
        }
    }

    fn label(self) -> &'static str {
        match self {
            SideFilter::Both => "server and client",
            SideFilter::Server => "server",
            SideFilter::Client => "client",
        }
    }

    fn shows(self, side: Option<LogSide>) -> bool {
        match (self, side) {
            (SideFilter::Both, _) | (_, None) => true,
            (SideFilter::Server, Some(side)) => side == LogSide::Server,
            (SideFilter::Client, Some(side)) => side == LogSide::Client,
        }
    }
}

/// What the Stats tab shows, measured by the host each frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ConsoleStats {
    /// Frames a second, averaged over the last second or so.
    pub fps: f32,
    /// The last frame, in milliseconds.
    pub frame_ms: f32,
    /// Each frame step and its milliseconds, in frame order.
    pub stages: Vec<(&'static str, f32)>,
    /// The process's memory, when the OS says.
    pub memory_bytes: Option<u64>,
    /// Both Lua heaps.
    pub lua_bytes: usize,
    pub instances: usize,
    /// While hosting or joined.
    pub network: Option<NetworkStats>,
}

/// The connection's numbers: to the server when joined, the slowest
/// player's ping and every player's traffic summed while hosting.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NetworkStats {
    pub players: usize,
    pub ping_ms: f32,
    pub sent_per_second: f32,
    pub received_per_second: f32,
}

/// A keyboard key the console uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConsoleKey {
    Enter,
    Backspace,
    Tab,
    Up,
    Down,
    PageUp,
    PageDown,
    Escape,
}

/// The console's state, kept between frames.
#[derive(Clone, Debug, Default)]
pub struct DevConsole {
    pub open: bool,
    pub tab: ConsoleTab,
    /// The line being typed in the Commands tab.
    pub input: String,
    pub level: LevelFilter,
    pub side: SideFilter,
    log: VecDeque<LogEntry>,
    history: Vec<String>,
    /// Where `Up` has gone back to in `history`.
    history_at: Option<usize>,
    /// Lines scrolled up from the newest.
    scroll: usize,
}

impl DevConsole {
    /// Opens a closed console and closes an open one (F9).
    pub fn toggle(&mut self) {
        self.open = !self.open;
    }

    /// Adds a line to the log, dropping the oldest past
    /// [`CONSOLE_LOG_LINES`].
    pub fn log(&mut self, level: LogLevel, side: Option<LogSide>, text: impl Into<String>) {
        self.log.push_back(LogEntry {
            level,
            side,
            text: text.into(),
        });
        while self.log.len() > CONSOLE_LOG_LINES {
            self.log.pop_front();
        }
    }

    /// Every line, oldest first.
    pub fn lines(&self) -> impl Iterator<Item = &LogEntry> {
        self.log.iter()
    }

    /// The lines the Log tab's filters show, oldest first.
    pub fn shown_lines(&self) -> impl Iterator<Item = &LogEntry> {
        self.log
            .iter()
            .filter(|line| self.level.shows(line.level) && self.side.shows(line.side))
    }

    pub fn clear(&mut self) {
        self.log.clear();
        self.scroll = 0;
    }

    /// Text typed while the console is open: into the command line in the
    /// Commands tab; in the Log tab, `l` and `s` change the filters.
    pub fn type_text(&mut self, text: &str) {
        match self.tab {
            ConsoleTab::Commands => {
                self.input.extend(text.chars().filter(|c| !c.is_control()));
                self.history_at = None;
            }
            ConsoleTab::Log => {
                for c in text.chars() {
                    match c.to_ascii_lowercase() {
                        'l' => self.level = self.level.next(),
                        's' => self.side = self.side.next(),
                        _ => {}
                    }
                }
            }
            ConsoleTab::Stats => {}
        }
    }

    /// A key pressed while the console is open. Returns the line to run
    /// when `Enter` submits one in the Commands tab, echoed into the log.
    pub fn key(&mut self, key: ConsoleKey) -> Option<String> {
        match key {
            ConsoleKey::Escape => self.open = false,
            ConsoleKey::Tab => {
                let at = ConsoleTab::ALL
                    .iter()
                    .position(|&tab| tab == self.tab)
                    .unwrap_or(0);
                self.tab = ConsoleTab::ALL[(at + 1) % ConsoleTab::ALL.len()];
            }
            ConsoleKey::PageUp => self.scroll = (self.scroll + 10).min(self.log.len()),
            ConsoleKey::PageDown => self.scroll = self.scroll.saturating_sub(10),
            _ if self.tab != ConsoleTab::Commands => {}
            ConsoleKey::Backspace => {
                self.input.pop();
            }
            ConsoleKey::Up if !self.history.is_empty() => {
                let at = match self.history_at {
                    Some(at) => at.saturating_sub(1),
                    None => self.history.len() - 1,
                };
                self.history_at = Some(at);
                self.input = self.history[at].clone();
            }
            ConsoleKey::Down => match self.history_at {
                Some(at) if at + 1 < self.history.len() => {
                    self.history_at = Some(at + 1);
                    self.input = self.history[at + 1].clone();
                }
                Some(_) => {
                    self.history_at = None;
                    self.input.clear();
                }
                None => {}
            },
            ConsoleKey::Up => {}
            ConsoleKey::Enter => {
                let line = std::mem::take(&mut self.input).trim().to_owned();
                self.history_at = None;
                self.scroll = 0;
                if line.is_empty() {
                    return None;
                }
                self.log(LogLevel::Command, None, format!("> {line}"));
                if self.history.last() != Some(&line) {
                    self.history.push(line.clone());
                }
                if self.history.len() > HISTORY {
                    self.history.remove(0);
                }
                return Some(line);
            }
        }
        None
    }
}

/// The console's built-in commands and what each does, for `help`.
pub const BUILT_INS: &[(&str, &str)] = &[
    ("help", "lists the commands"),
    ("clear", "empties the log"),
    ("fps", "shows or hides the frame rate in the corner"),
    ("wireframe", "draws every triangle's edges"),
    ("collisions", "draws the colliders' outlines"),
    (
        "noclip",
        "the character flies through walls (Jump up, Crouch down)",
    ),
    (
        "timescale <scale>",
        "runs the game at this speed: 0.5 is half",
    ),
    (
        "teleport <x> <y> <z>",
        "moves the character (or camera) there",
    ),
    ("crash", "stops the game with a panic, to try the crash log"),
];

/// A built-in command, as typed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BuiltIn {
    Help,
    Clear,
    Fps,
    Wireframe,
    Collisions,
    Noclip,
    Timescale(f32),
    Teleport(Vector3),
    Crash,
}

/// The built-in command `line` is, or why its words are wrong for it.
/// `None` when its first word isn't a built-in.
pub fn built_in(line: &str) -> Option<Result<BuiltIn, String>> {
    let mut words = line.split_whitespace();
    let name = words.next()?.to_ascii_lowercase();
    let args: Vec<&str> = words.collect();
    let numbers = || -> Result<Vec<f32>, String> {
        args.iter()
            .map(|arg| {
                arg.parse::<f32>()
                    .ok()
                    .filter(|n| n.is_finite())
                    .ok_or_else(|| format!("`{arg}` isn't a number"))
            })
            .collect()
    };
    let plain = |command: BuiltIn| {
        if args.is_empty() {
            Ok(command)
        } else {
            Err(format!("`{name}` takes nothing after it"))
        }
    };
    Some(match name.as_str() {
        "help" => plain(BuiltIn::Help),
        "clear" => plain(BuiltIn::Clear),
        "fps" => plain(BuiltIn::Fps),
        "wireframe" => plain(BuiltIn::Wireframe),
        "collisions" => plain(BuiltIn::Collisions),
        "noclip" => plain(BuiltIn::Noclip),
        "crash" => plain(BuiltIn::Crash),
        "timescale" => numbers().and_then(|n| match n[..] {
            [scale] if (0.0..=100.0).contains(&scale) => Ok(BuiltIn::Timescale(scale)),
            [_] => Err("the time scale goes from 0 to 100".to_owned()),
            _ => Err("usage: timescale <scale>, such as timescale 0.5".to_owned()),
        }),
        "teleport" => numbers().and_then(|n| match n[..] {
            [x, y, z] => Ok(BuiltIn::Teleport(Vector3::new(x, y, z))),
            _ => Err("usage: teleport <x> <y> <z>".to_owned()),
        }),
        _ => return None,
    })
}

/// A name a command could have: letters, digits and `_`, not starting with
/// a digit. Lua such as `print(1)` or `x = 5` isn't one.
pub fn is_command_word(word: &str) -> bool {
    let mut chars = word.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Whether `line` reads as a command and its words (`give 5`, `heal Bob
/// 2.5`) rather than Lua (`x = 5`, `print("hi")`): its first word is a
/// [command word](is_command_word), and the rest is only letters, digits,
/// spaces and `_ - . + : , / @`.
pub fn looks_like_command(line: &str) -> bool {
    let line = line.trim();
    line.split_whitespace().next().is_some_and(is_command_word)
        && line
            .chars()
            .all(|c| c.is_alphanumeric() || c.is_whitespace() || "_-.+:,/@".contains(c))
}

fn rectangle(x: f32, y: f32, width: f32, height: f32, color: [f32; 4]) -> UiDrawItem {
    UiDrawItem::Rectangle(UiRectangle {
        position: [x, y],
        size: [width.max(0.0), height.max(0.0)],
        color,
        color_end: color,
        ..UiRectangle::default()
    })
}

fn text(text: String, x: f32, y: f32, width: f32, color: [f32; 4]) -> UiDrawItem {
    UiDrawItem::Text(UiText {
        position: [x, y],
        size: [width.max(0.0), LINE_HEIGHT],
        text,
        font_size: FONT_SIZE,
        font_family: FONT.to_owned(),
        color,
        align: TextAlign::Left,
        vertical_align: VerticalAlign::Middle,
        weight: 400,
        wrap: false,
        ..UiText::default()
    })
}

fn line_color(line: &LogEntry) -> [f32; 4] {
    match line.level {
        LogLevel::Info => TEXT,
        LogLevel::Warning => WARNING,
        LogLevel::Error => ERROR,
        LogLevel::Command => COMMAND,
    }
}

fn line_text(line: &LogEntry) -> String {
    match line.side {
        Some(LogSide::Server) => format!("[server] {}", line.text),
        Some(LogSide::Client) => format!("[client] {}", line.text),
        None => line.text.clone(),
    }
}

fn megabytes(bytes: f64) -> String {
    format!("{:.1} MB", bytes / (1024.0 * 1024.0))
}

fn kilobytes(bytes: f32) -> String {
    format!("{:.1} KB/s", bytes / 1024.0)
}

/// The Stats tab's lines.
pub fn stats_lines(stats: &ConsoleStats) -> Vec<String> {
    let mut lines = vec![format!(
        "FPS {:.0}    frame {:.2} ms",
        stats.fps, stats.frame_ms
    )];
    for (stage, ms) in &stats.stages {
        lines.push(format!("  {stage:<12} {ms:>7.2} ms"));
    }
    lines.push(match stats.memory_bytes {
        Some(bytes) => format!("Memory {}", megabytes(bytes as f64)),
        None => "Memory unknown".to_owned(),
    });
    lines.push(format!("Lua {}", megabytes(stats.lua_bytes as f64)));
    lines.push(format!("Instances {}", stats.instances));
    match stats.network {
        Some(network) => {
            lines.push(format!(
                "Network: {} player{}, ping {:.0} ms",
                network.players,
                if network.players == 1 { "" } else { "s" },
                network.ping_ms
            ));
            lines.push(format!(
                "  sent {}    received {}",
                kilobytes(network.sent_per_second),
                kilobytes(network.received_per_second)
            ));
        }
        None => lines.push("Network: offline".to_owned()),
    }
    lines
}

/// The draw items for `console` in a `width` × `height` view, over
/// everything else: nothing while it's closed.
pub fn console_items(
    console: &DevConsole,
    stats: &ConsoleStats,
    width: u32,
    height: u32,
) -> Vec<UiDrawItem> {
    let mut out = Vec::new();
    if !console.open || width == 0 || height == 0 {
        return out;
    }
    let (width, height) = (width as f32, height as f32);
    let panel = panel_height(height);
    out.push(rectangle(0.0, 0.0, width, panel, BACKGROUND));
    // The tab bar.
    out.push(rectangle(0.0, 0.0, width, LINE_HEIGHT + 8.0, BAR));
    let mut x = PADDING;
    for tab in ConsoleTab::ALL {
        let label = tab.label();
        let tab_width = label.len() as f32 * FONT_SIZE * 0.6 + 20.0;
        if tab == console.tab {
            out.push(rectangle(x, 2.0, tab_width, LINE_HEIGHT + 4.0, ACTIVE_TAB));
        }
        out.push(text(label.to_owned(), x + 10.0, 4.0, tab_width, TEXT));
        x += tab_width + 4.0;
    }
    let hint = match console.tab {
        ConsoleTab::Log => format!(
            "showing {}, {} (L, S)   Tab: next tab   F9: close",
            console.level.label(),
            console.side.label()
        ),
        _ => "Tab: next tab   PageUp/PageDown: scroll   F9: close".to_owned(),
    };
    out.push(text(hint, x + 12.0, 4.0, width - x - 12.0, DIM));

    let top = LINE_HEIGHT + 8.0 + PADDING / 2.0;
    let inner = width - PADDING * 2.0;
    match console.tab {
        ConsoleTab::Stats => {
            let mut y = top;
            for line in stats_lines(stats) {
                if y + LINE_HEIGHT > panel {
                    break;
                }
                out.push(text(line, PADDING, y, inner, TEXT));
                y += LINE_HEIGHT;
            }
        }
        ConsoleTab::Log | ConsoleTab::Commands => {
            let typing = console.tab == ConsoleTab::Commands;
            let bottom = panel - PADDING / 2.0 - if typing { LINE_HEIGHT + 6.0 } else { 0.0 };
            let rows = ((bottom - top) / LINE_HEIGHT).floor().max(0.0) as usize;
            let lines: Vec<&LogEntry> = if typing {
                console.lines().collect()
            } else {
                console.shown_lines().collect()
            };
            let end = lines.len().saturating_sub(console.scroll);
            let start = end.saturating_sub(rows);
            let mut y = bottom - (end - start) as f32 * LINE_HEIGHT;
            for line in &lines[start..end] {
                out.push(text(line_text(line), PADDING, y, inner, line_color(line)));
                y += LINE_HEIGHT;
            }
            if typing {
                let y = panel - LINE_HEIGHT - PADDING / 2.0 - 2.0;
                out.push(rectangle(0.0, y - 3.0, width, LINE_HEIGHT + 6.0, BAR));
                out.push(text(
                    format!("> {}_", console.input),
                    PADDING,
                    y,
                    inner,
                    TEXT,
                ));
            }
        }
    }
    out
}

/// How tall the open console is in a view `height` pixels tall.
fn panel_height(height: f32) -> f32 {
    (height * HEIGHT_SHARE).max(LINE_HEIGHT * 6.0).min(height)
}

/// The `fps` command's readout, at the top right of a `width` × `height`
/// view: under the console while it's open.
pub fn fps_items(
    stats: &ConsoleStats,
    console: &DevConsole,
    width: u32,
    height: u32,
) -> Vec<UiDrawItem> {
    let label = format!("{:.0} FPS  {:.1} ms", stats.fps, stats.frame_ms);
    let box_width = label.len() as f32 * FONT_SIZE * 0.6 + PADDING * 2.0;
    let x = (width as f32 - box_width - PADDING).max(0.0);
    let y = if console.open {
        panel_height(height as f32) + PADDING
    } else {
        PADDING
    };
    vec![
        rectangle(x, y, box_width, LINE_HEIGHT + 6.0, READOUT),
        text(label, x + PADDING, y + 3.0, box_width, TEXT),
    ]
}

/// Screen-space line segments, each `(from, to)` in pixels, drawn as thin
/// turned rectangles in `color` (the `collisions` command's outlines).
pub fn line_items(segments: &[([f32; 2], [f32; 2])], color: [f32; 4]) -> Vec<UiDrawItem> {
    const THICKNESS: f32 = 1.5;
    segments
        .iter()
        .filter_map(|&([ax, ay], [bx, by])| {
            let length = (bx - ax).hypot(by - ay);
            if !length.is_finite() || length < 0.5 {
                return None;
            }
            let (cx, cy) = ((ax + bx) * 0.5, (ay + by) * 0.5);
            Some(UiDrawItem::Rectangle(UiRectangle {
                position: [cx - length * 0.5, cy - THICKNESS * 0.5],
                size: [length, THICKNESS],
                color,
                color_end: color,
                rotation_degrees: (by - ay).atan2(bx - ax).to_degrees(),
                ..UiRectangle::default()
            }))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(items: &[UiDrawItem]) -> Vec<String> {
        items
            .iter()
            .filter_map(|item| match item {
                UiDrawItem::Text(text) => Some(text.text.clone()),
                _ => None,
            })
            .collect()
    }

    fn typed(console: &mut DevConsole, line: &str) -> Option<String> {
        console.type_text(line);
        console.key(ConsoleKey::Enter)
    }

    #[test]
    fn a_typed_line_is_submitted_echoed_and_kept_for_up() {
        let mut console = DevConsole::default();
        console.toggle();
        assert!(console.open);
        assert_eq!(console.tab, ConsoleTab::Commands);
        assert_eq!(
            typed(&mut console, "timescale 0.5"),
            Some("timescale 0.5".to_owned())
        );
        assert_eq!(console.input, "");
        assert_eq!(typed(&mut console, "fpz"), Some("fpz".to_owned()));
        console.key(ConsoleKey::Backspace);
        console.key(ConsoleKey::Up);
        assert_eq!(console.input, "fpz");
        console.key(ConsoleKey::Up);
        assert_eq!(console.input, "timescale 0.5");
        console.key(ConsoleKey::Down);
        console.key(ConsoleKey::Down);
        assert_eq!(console.input, "");
        assert_eq!(
            console.key(ConsoleKey::Enter),
            None,
            "a blank line runs nothing"
        );
        let echoed: Vec<&str> = console.lines().map(|line| line.text.as_str()).collect();
        assert_eq!(echoed, ["> timescale 0.5", "> fpz"]);
        console.key(ConsoleKey::Escape);
        assert!(!console.open);
    }

    #[test]
    fn the_log_tab_filters_by_level_and_side() {
        let mut console = DevConsole::default();
        console.log(LogLevel::Info, Some(LogSide::Server), "server hello");
        console.log(LogLevel::Warning, Some(LogSide::Client), "client careful");
        console.log(LogLevel::Error, Some(LogSide::Server), "server broke");
        console.log(LogLevel::Error, None, "engine broke");
        console.key(ConsoleKey::Tab);
        assert_eq!(console.tab, ConsoleTab::Log);
        let shown = |console: &DevConsole| -> Vec<String> {
            console
                .shown_lines()
                .map(|line| line.text.clone())
                .collect()
        };
        assert_eq!(shown(&console).len(), 4);
        console.type_text("l");
        assert_eq!(
            shown(&console),
            ["client careful", "server broke", "engine broke"]
        );
        console.type_text("l");
        assert_eq!(shown(&console), ["server broke", "engine broke"]);
        console.type_text("ls");
        assert_eq!(
            shown(&console),
            ["server hello", "server broke", "engine broke"]
        );
        console.type_text("s");
        assert_eq!(shown(&console), ["client careful", "engine broke"]);
        assert_eq!(console.input, "", "letters don't go to the command line");
        for n in 0..CONSOLE_LOG_LINES {
            console.log(LogLevel::Info, None, format!("{n}"));
        }
        assert_eq!(console.lines().count(), CONSOLE_LOG_LINES);
    }

    #[test]
    fn built_in_commands_read_their_numbers_and_say_what_is_wrong() {
        assert_eq!(built_in("timescale 0.5"), Some(Ok(BuiltIn::Timescale(0.5))));
        assert_eq!(built_in("FPS"), Some(Ok(BuiltIn::Fps)));
        assert_eq!(
            built_in("teleport 1 2.5 -3"),
            Some(Ok(BuiltIn::Teleport(Vector3::new(1.0, 2.5, -3.0))))
        );
        assert!(matches!(built_in("timescale fast"), Some(Err(why)) if why.contains("fast")));
        assert!(matches!(built_in("timescale -1"), Some(Err(_))));
        assert!(matches!(built_in("teleport 1 2"), Some(Err(why)) if why.contains("usage")));
        assert!(matches!(built_in("noclip now"), Some(Err(_))));
        assert_eq!(built_in("heal 5"), None);
        assert_eq!(built_in(""), None);
        assert!(is_command_word("heal") && is_command_word("_x1"));
        assert!(!is_command_word("print(1)") && !is_command_word("1x") && !is_command_word(""));
        assert!(looks_like_command("give -5") && looks_like_command("heal Bob 2.5"));
        assert!(!looks_like_command("x = 5") && !looks_like_command("print('hi')"));
    }

    #[test]
    fn each_tab_draws_its_own_lines_and_a_closed_console_draws_nothing() {
        let mut console = DevConsole::default();
        let stats = ConsoleStats {
            fps: 60.0,
            frame_ms: 16.7,
            stages: vec![("Physics", 2.5)],
            memory_bytes: Some(64 * 1024 * 1024),
            lua_bytes: 1024 * 1024,
            instances: 42,
            network: Some(NetworkStats {
                players: 2,
                ping_ms: 35.0,
                sent_per_second: 2048.0,
                received_per_second: 1024.0,
            }),
        };
        assert!(console_items(&console, &stats, 800, 600).is_empty());
        console.toggle();
        console.log(LogLevel::Info, Some(LogSide::Client), "hi");
        console.type_text("hea");
        let shown = texts(&console_items(&console, &stats, 800, 600));
        assert!(shown.contains(&"[client] hi".to_owned()), "{shown:?}");
        assert_eq!(shown.last().map(String::as_str), Some("> hea_"));
        console.key(ConsoleKey::Tab);
        console.key(ConsoleKey::Tab);
        assert_eq!(console.tab, ConsoleTab::Stats);
        let shown = texts(&console_items(&console, &stats, 800, 600));
        assert!(
            shown.contains(&"FPS 60    frame 16.70 ms".to_owned()),
            "{shown:?}"
        );
        assert!(
            shown
                .iter()
                .any(|line| line.contains("Physics") && line.contains("2.50 ms"))
        );
        assert!(shown.contains(&"Memory 64.0 MB".to_owned()));
        assert!(shown.contains(&"Instances 42".to_owned()));
        assert!(
            shown.contains(&"Network: 2 players, ping 35 ms".to_owned()),
            "{shown:?}"
        );
        assert!(shown.contains(&"  sent 2.0 KB/s    received 1.0 KB/s".to_owned()));
        let readout = fps_items(&stats, &console, 800, 600);
        assert_eq!(texts(&readout), ["60 FPS  16.7 ms"]);
        let UiDrawItem::Rectangle(under) = &readout[0] else {
            panic!("{readout:?}");
        };
        assert!(
            under.position[1] > 600.0 * HEIGHT_SHARE,
            "below the open console"
        );
    }

    #[test]
    fn a_line_segment_is_a_turned_rectangle_between_its_ends() {
        let items = line_items(
            &[([0.0, 0.0], [10.0, 10.0]), ([5.0, 5.0], [5.0, 5.0])],
            TEXT,
        );
        assert_eq!(items.len(), 1, "a dot isn't drawn");
        let UiDrawItem::Rectangle(rectangle) = &items[0] else {
            panic!("{items:?}");
        };
        assert!((rectangle.rotation_degrees - 45.0).abs() < 1e-3);
        assert!((rectangle.size[0] - 200f32.sqrt()).abs() < 1e-3);
        assert!((rectangle.position[0] + rectangle.size[0] * 0.5 - 5.0).abs() < 1e-3);
    }
}
