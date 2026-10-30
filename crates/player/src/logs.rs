//! The game's log file (Phase 71): `logs/latest.log` in the player's data
//! folder for the game, beside its saves. Each start moves the last one to
//! `previous-1.log` (and that to `previous-2.log`, up to
//! [`KEPT_PREVIOUS`]), so the last five runs are kept.
//!
//! Everything the player prints goes through [`line`]: to the terminal,
//! into the file, and to the Developer Console's Log ([`take_lines`]):
//! scripts' output and errors, the engine's messages and, through
//! [`Logger`], `log` records. If the game panics, [`install_panic_hook`]
//! writes the panic there and says where the log is before the game exits.
//!
//! The file is global (one per process), so the panic hook can reach it
//! from wherever the panic happens. The console's lines wait per thread,
//! so each test's player sees only its own.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use axcenity_ui::{CONSOLE_LOG_LINES, LogEntry, LogLevel, LogSide};

/// The log of this run.
pub const LATEST: &str = "latest.log";
/// How many earlier logs are kept besides `latest.log`.
pub const KEPT_PREVIOUS: usize = 4;

struct LogFile {
    path: PathBuf,
    file: File,
}

/// The open log, once [`start`] has made it.
static LOG: Mutex<Option<LogFile>> = Mutex::new(None);

thread_local! {
    /// Lines waiting for the console, newest last.
    static PENDING: RefCell<VecDeque<LogEntry>> = const { RefCell::new(VecDeque::new()) };
}

/// The earlier log number `n`: `previous-1.log` is the run before this one.
fn previous(folder: &Path, n: usize) -> PathBuf {
    folder.join(format!("previous-{n}.log"))
}

/// Starts this run's log in `folder` (made if missing), moving the earlier
/// ones along and dropping the oldest. Returns the log's path.
pub fn start(folder: &Path) -> std::io::Result<PathBuf> {
    fs::create_dir_all(folder)?;
    let _ = fs::remove_file(previous(folder, KEPT_PREVIOUS));
    for n in (1..KEPT_PREVIOUS).rev() {
        let from = previous(folder, n);
        if from.is_file() {
            fs::rename(&from, previous(folder, n + 1))?;
        }
    }
    let path = folder.join(LATEST);
    if path.is_file() {
        fs::rename(&path, previous(folder, 1))?;
    }
    let file = File::create(&path)?;
    *LOG.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(LogFile {
        path: path.clone(),
        file,
    });
    Ok(path)
}

/// Where this run's log is, once started.
pub fn path() -> Option<PathBuf> {
    LOG.lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .as_ref()
        .map(|log| log.path.clone())
}

/// Adds `text` (one or more lines) to the log, if there is one. Written at
/// once, so a crash right after loses nothing.
pub fn write(text: &str) {
    let mut log = LOG.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(log) = log.as_mut() {
        let _ = writeln!(log.file, "{text}");
        let _ = log.file.flush();
    }
}

/// Prints `text` (to standard output for [`LogLevel::Info`], else to
/// standard error), writes it to the log file and hands it to the console.
/// `side` is the scripts' side it came from, if any.
pub fn line(level: LogLevel, side: Option<LogSide>, text: &str) {
    if level == LogLevel::Info {
        println!("{text}");
    } else {
        eprintln!("{text}");
    }
    write(text);
    queue(level, side, text);
}

fn queue(level: LogLevel, side: Option<LogSide>, text: &str) {
    PENDING.with(|pending| {
        let mut pending = pending.borrow_mut();
        pending.push_back(LogEntry {
            level,
            side,
            text: text.to_owned(),
        });
        while pending.len() > CONSOLE_LOG_LINES {
            pending.pop_front();
        }
    });
}

/// The lines logged on this thread since the last call, oldest first.
pub fn take_lines() -> Vec<LogEntry> {
    PENDING.with(|pending| pending.borrow_mut().drain(..).collect())
}

/// The `log` crate's records: shown as `env_logger` shows them, and
/// written to the log file too.
pub struct Logger(pub env_logger::Logger);

impl log::Log for Logger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        self.0.enabled(metadata)
    }

    fn log(&self, record: &log::Record) {
        if self.0.matches(record) {
            self.0.log(record);
            let text = format!(
                "{}: {}: {}",
                record.level().as_str().to_lowercase(),
                record.target(),
                record.args()
            );
            write(&text);
            let level = match record.level() {
                log::Level::Error => LogLevel::Error,
                log::Level::Warn => LogLevel::Warning,
                _ => LogLevel::Info,
            };
            queue(level, None, &text);
        }
    }

    fn flush(&self) {
        self.0.flush();
    }
}

/// Uses [`Logger`] for the `log` crate's records.
pub fn install_logger(inner: env_logger::Logger) {
    let level = inner.filter();
    // `leak` gives the logger the `'static` life `set_logger` needs; it
    // lives as long as the program anyway.
    if log::set_logger(Box::leak(Box::new(Logger(inner)))).is_ok() {
        log::set_max_level(level);
    }
}

/// What a crash says: the panic, and where the log is.
pub fn crash_message(panic: &str, log: Option<&Path>) -> String {
    match log {
        Some(log) => format!("The game crashed: {panic}\nThe log is at {}", log.display()),
        None => format!("The game crashed: {panic}"),
    }
}

/// On a panic, writes it (with where in the engine it happened) to the
/// log, then hands [`crash_message`] to `tell` before the game exits.
pub fn install_panic_hook(tell: fn(&str)) {
    std::panic::set_hook(Box::new(move |info| {
        let payload = info.payload();
        let message = payload
            .downcast_ref::<&str>()
            .map(|text| (*text).to_owned())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "an unknown error".to_owned());
        let panic = match info.location() {
            Some(at) => format!("{message} (at {}:{})", at.file(), at.line()),
            None => message,
        };
        write(&format!("panic: {panic}"));
        tell(&crash_message(&panic, path().as_deref()));
    }));
}

/// How a crash is told: on the terminal's standard error.
pub fn tell_on_stderr(message: &str) {
    eprintln!("{message}");
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    static TOLD: Mutex<Vec<String>> = Mutex::new(Vec::new());

    fn remember(message: &str) {
        TOLD.lock().unwrap().push(message.to_owned());
    }

    #[test]
    fn a_forced_panic_leaves_a_log_with_the_panic_and_says_where_the_log_is() {
        let folder = std::env::temp_dir().join(format!(
            "axcenity-player-logs-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&folder);
        // Six runs: the last five logs are kept, newest as latest.log.
        for run in 1..=6 {
            let path = start(&folder).unwrap();
            assert_eq!(path, folder.join(LATEST));
            line(LogLevel::Info, None, &format!("run {run}"));
        }
        let kept: Vec<String> = (1..=KEPT_PREVIOUS)
            .map(|n| fs::read_to_string(previous(&folder, n)).unwrap())
            .collect();
        assert_eq!(kept, ["run 5\n", "run 4\n", "run 3\n", "run 2\n"]);
        assert!(!previous(&folder, KEPT_PREVIOUS + 1).exists());
        assert_eq!(fs::read_dir(&folder).unwrap().count(), 5);

        let before = std::panic::take_hook();
        install_panic_hook(remember);
        let crashed = std::panic::catch_unwind(|| panic!("forced for the test"));
        std::panic::set_hook(before);
        assert!(crashed.is_err());

        let queued: Vec<String> = take_lines().into_iter().map(|line| line.text).collect();
        assert_eq!(queued.len(), 6, "the console gets them too: {queued:?}");
        let log = fs::read_to_string(folder.join(LATEST)).unwrap();
        assert!(log.starts_with("run 6\n"), "{log}");
        assert!(log.contains("panic: forced for the test (at "), "{log}");
        assert!(log.contains("logs.rs"), "where in the engine: {log}");
        let told = TOLD.lock().unwrap().clone();
        assert_eq!(told.len(), 1, "{told:?}");
        assert!(told[0].starts_with("The game crashed: forced for the test"));
        assert!(
            told[0].ends_with(&format!("The log is at {}", folder.join(LATEST).display())),
            "{told:?}"
        );
        *LOG.lock().unwrap() = None;
        fs::remove_dir_all(&folder).unwrap();
    }
}
