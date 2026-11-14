//! `axcenity-bots <game> --join <address:port> [--count <n>] ...`: headless
//! players for scale and soak tests (multiplayer readiness MP1,
//! `docs/research/multiplayer-readiness.md`).
//!
//! Each bot is a player's device as its own connection over real UDP: it
//! loads the same game (the server checks the version and class
//! defaults), joins, follows the server's world as a replica, runs the
//! game's LocalScripts unless `--no-scripts`, and walks its character
//! with made-up input, so the server checks its movement as it would a
//! person's. Bots run on `--threads` threads, a share each, at 60 frames
//! a second. Every `--report` seconds a line sums them up (round trips,
//! bandwidth, corrections, bots' own frame times); at the end comes the
//! summary, also as JSON lines in `--report-file`.

mod bot;
mod stats;

use std::ffi::OsString;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::{Result, bail};

use bot::{Bot, BotSettings};
use stats::{BotSample, Summary};

const USAGE: &str = "usage: axcenity-bots <project-folder> --join <address:port> [--count <n>] [--seconds <s>] \
                     [--threads <n>] [--stagger <ms>] [--password <text>] [--no-scripts] [--idle] \
                     [--report <s>] [--report-file <path>] [--user-id <first>]\n\
                     example: cargo run -p axcenity-server --bin axcenity-bots -- examples/hello-character --join 127.0.0.1:47800 --count 16";

/// Each frame is this long.
const FRAME: Duration = Duration::from_micros(16_667);

#[derive(Debug, PartialEq)]
struct Arguments {
    game: Option<PathBuf>,
    address: Option<String>,
    count: usize,
    /// How long to stay; 0 for until the process is stopped.
    seconds: f64,
    threads: usize,
    /// Between one bot's start and the next's.
    stagger: Duration,
    password: Option<String>,
    scripts: bool,
    idle: bool,
    report: f64,
    report_file: Option<PathBuf>,
    first_user_id: Option<u64>,
}

impl Default for Arguments {
    fn default() -> Arguments {
        Arguments {
            game: None,
            address: None,
            count: 1,
            seconds: 60.0,
            threads: std::thread::available_parallelism().map_or(2, |n| n.get()),
            stagger: Duration::from_millis(100),
            password: None,
            scripts: true,
            idle: false,
            report: 5.0,
            report_file: None,
            first_user_id: None,
        }
    }
}

impl Arguments {
    fn parse(arguments: impl IntoIterator<Item = OsString>) -> Result<Arguments> {
        let mut parsed = Arguments::default();
        let mut arguments = arguments.into_iter();
        let mut value = |name: &str, what: &str| -> Result<String> {
            match arguments.next() {
                Some(value) => Ok(value.to_string_lossy().into_owned()),
                None => bail!("{name} takes {what}\n\n{USAGE}"),
            }
        };
        fn number<T: std::str::FromStr>(text: &str, name: &str, what: &str) -> Result<T> {
            text.parse()
                .map_err(|_| anyhow::anyhow!("{name} takes {what}, not `{text}`\n\n{USAGE}"))
        }
        let mut rest = Vec::new();
        while let Ok(text) = value("", "") {
            match text.as_str() {
                "--join" => parsed.address = Some(value("--join", "an address and port")?),
                "--count" => {
                    let count: usize =
                        number(&value("--count", "a number")?, "--count", "a number")?;
                    if count == 0 {
                        bail!("--count takes at least 1\n\n{USAGE}");
                    }
                    parsed.count = count;
                }
                "--seconds" => {
                    parsed.seconds =
                        number(&value("--seconds", "seconds")?, "--seconds", "seconds")?
                }
                "--threads" => {
                    let threads: usize =
                        number(&value("--threads", "a number")?, "--threads", "a number")?;
                    parsed.threads = threads.max(1);
                }
                "--stagger" => {
                    let ms: u64 = number(
                        &value("--stagger", "milliseconds")?,
                        "--stagger",
                        "milliseconds",
                    )?;
                    parsed.stagger = Duration::from_millis(ms);
                }
                "--password" => parsed.password = Some(value("--password", "the password")?),
                "--no-scripts" => parsed.scripts = false,
                "--idle" => parsed.idle = true,
                "--report" => {
                    let seconds: f64 =
                        number(&value("--report", "seconds")?, "--report", "seconds")?;
                    parsed.report = seconds.max(0.5);
                }
                "--report-file" => {
                    parsed.report_file = Some(PathBuf::from(value("--report-file", "a path")?))
                }
                "--user-id" => {
                    parsed.first_user_id = Some(number(
                        &value("--user-id", "a number")?,
                        "--user-id",
                        "a number",
                    )?)
                }
                "--help" | "-h" => bail!("{USAGE}"),
                _ if text.starts_with("--") => bail!("unknown option `{text}`\n\n{USAGE}"),
                _ => rest.push(text),
            }
        }
        match rest.as_slice() {
            [] => {}
            [game] => parsed.game = Some(PathBuf::from(game)),
            _ => bail!("only one game at a time\n\n{USAGE}"),
        }
        Ok(parsed)
    }
}

/// What a thread sends the main thread.
enum Report {
    Sample(BotSample),
    /// A bot couldn't start at all.
    Failed(usize, String),
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    let arguments = Arguments::parse(std::env::args_os().skip(1))?;
    let Some(game) = arguments.game.clone() else {
        bail!("no game to join\n\n{USAGE}");
    };
    let Some(address) = arguments.address.clone() else {
        bail!("--join <address:port> is needed\n\n{USAGE}");
    };
    // UserIds a real install never picks (those go up to 2^53 - 1 at
    // random) would be just as good; a fixed range per process is easier
    // to read in the server's log.
    let first_user_id = arguments
        .first_user_id
        .unwrap_or_else(|| 1_000_000 + u64::from(std::process::id()) * 1000);
    let settings = BotSettings {
        game,
        address,
        password: arguments.password.clone(),
        scripts: arguments.scripts,
        idle: arguments.idle,
        first_user_id,
    };
    let stop = Arc::new(AtomicBool::new(false));
    let (sender, receiver) = mpsc::channel();
    let threads = arguments.threads.min(arguments.count);
    let started = Instant::now();
    let handles: Vec<_> = (0..threads)
        .map(|thread| {
            let mine: Vec<usize> = (thread..arguments.count).step_by(threads).collect();
            let settings = settings.clone();
            let stop = stop.clone();
            let sender = sender.clone();
            let stagger = arguments.stagger;
            let report = Duration::from_secs_f64(arguments.report);
            std::thread::Builder::new()
                .name(format!("bots-{thread}"))
                // Luau and the editor's tests want a deep stack in debug.
                .stack_size(16 * 1024 * 1024)
                .spawn(move || run_thread(mine, &settings, stagger, report, &stop, &sender))
        })
        .collect::<std::io::Result<_>>()?;
    drop(sender);
    eprintln!(
        "Starting {} bots on {threads} threads, joining {}...",
        arguments.count, settings.address
    );

    let mut latest: Vec<BotSample> = (0..arguments.count)
        .map(|bot| BotSample {
            bot,
            ..BotSample::default()
        })
        .collect();
    let mut file = match &arguments.report_file {
        Some(path) => Some(std::fs::File::create(path)?),
        None => None,
    };
    let mut next_report = arguments.report;
    let mut last = Summary::default();
    loop {
        let elapsed = started.elapsed().as_secs_f64();
        if arguments.seconds > 0.0 && elapsed >= arguments.seconds {
            stop.store(true, Ordering::Relaxed);
        }
        match receiver.recv_timeout(Duration::from_millis(100)) {
            Ok(Report::Sample(sample)) => {
                let bot = sample.bot;
                latest[bot] = sample;
            }
            Ok(Report::Failed(bot, reason)) => {
                latest[bot].ended = Some(reason);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        if elapsed >= next_report {
            next_report += arguments.report;
            last = Summary::of(&latest);
            println!("[{elapsed:6.1} s] {}", last.line());
            if let Some(file) = &mut file {
                writeln!(file, "{}", last.json(elapsed))?;
            }
        }
    }
    for handle in handles {
        let _ = handle.join();
    }
    let elapsed = started.elapsed().as_secs_f64();
    let end = Summary::of(&latest);
    // The link numbers of the last report, while the bots were still in.
    let summary = Summary {
        connected: last.connected,
        rtt_p50: last.rtt_p50,
        rtt_p95: last.rtt_p95,
        rtt_max: last.rtt_max,
        loss_mean: last.loss_mean,
        sent_mean: last.sent_mean,
        received_mean: last.received_mean,
        frame_ms_mean: last.frame_ms_mean,
        frame_ms_max: last.frame_ms_max,
        ..end
    };
    println!("Summary after {elapsed:.1} s: {}", summary.line());
    if let Some(file) = &mut file {
        let mut json = summary.json(elapsed);
        json["summary"] = serde_json::Value::Bool(true);
        writeln!(file, "{json}")?;
    }
    if summary.joined < summary.bots {
        bail!(
            "{} of {} bots never joined",
            summary.bots - summary.joined,
            summary.bots
        );
    }
    Ok(())
}

/// One thread's bots: started one by one, `stagger` apart, then stepped
/// each frame until `stop`; a sample of each goes out every `report`.
fn run_thread(
    indices: Vec<usize>,
    settings: &BotSettings,
    stagger: Duration,
    report: Duration,
    stop: &AtomicBool,
    sender: &mpsc::Sender<Report>,
) {
    let started = Instant::now();
    let mut bots: Vec<Bot> = Vec::new();
    let mut waiting = indices.into_iter().peekable();
    let mut last_report = Instant::now();
    let mut last = Instant::now();
    while !stop.load(Ordering::Relaxed) {
        let frame_started = Instant::now();
        // Bot n starts n staggers after the run did, whichever thread has it.
        while let Some(&index) = waiting.peek() {
            if started.elapsed() < stagger * index as u32 {
                break;
            }
            waiting.next();
            match Bot::start(index, settings) {
                Ok(bot) => bots.push(bot),
                Err(err) => {
                    let _ = sender.send(Report::Failed(index, format!("{err:#}")));
                }
            }
        }
        let seconds = frame_started.duration_since(last).as_secs_f64().min(0.1);
        last = frame_started;
        for bot in &mut bots {
            bot.frame(seconds);
        }
        if last_report.elapsed() >= report {
            last_report = Instant::now();
            for bot in &mut bots {
                let _ = sender.send(Report::Sample(bot.sample()));
            }
        }
        if waiting.peek().is_none() && !bots.is_empty() && bots.iter().all(Bot::is_over) {
            break;
        }
        if let Some(rest) = FRAME.checked_sub(frame_started.elapsed()) {
            std::thread::sleep(rest);
        }
    }
    for bot in &mut bots {
        let _ = sender.send(Report::Sample(bot.sample()));
        bot.leave();
    }
    // Let the leaving packets go out.
    for bot in &mut bots {
        bot.frame(FRAME.as_secs_f64());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(arguments: &[&str]) -> Result<Arguments> {
        Arguments::parse(arguments.iter().map(OsString::from))
    }

    #[test]
    fn options_come_in_any_order() {
        let parsed = parse(&[
            "--count",
            "32",
            "games/castle",
            "--join",
            "192.168.1.20:47800",
            "--no-scripts",
            "--seconds",
            "120",
        ])
        .unwrap();
        assert_eq!(parsed.game, Some(PathBuf::from("games/castle")));
        assert_eq!(parsed.address.as_deref(), Some("192.168.1.20:47800"));
        assert_eq!(parsed.count, 32);
        assert_eq!(parsed.seconds, 120.0);
        assert!(!parsed.scripts);
    }

    #[test]
    fn a_bad_count_says_what_it_takes() {
        let err = parse(&["castle", "--count", "lots"]).unwrap_err();
        assert!(err.to_string().contains("--count takes a number"), "{err}");
        let err = parse(&["castle", "--count", "0"]).unwrap_err();
        assert!(err.to_string().contains("at least 1"), "{err}");
    }
}
