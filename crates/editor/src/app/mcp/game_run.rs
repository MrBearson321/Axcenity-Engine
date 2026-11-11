//! `game_run`: try the game. `play` runs it for a few seconds of game
//! time on a copy of the world, as Play does (without saving first), then
//! stops; it returns the scripts' errors and the end of their output, and
//! how long frames took. Whatever the game changed is gone after, as after
//! Stop. `tests` runs the project's game tests (`tests/*.spec.lua`,
//! `axcenity_testing::Runner`, as the Tests panel does), which read the
//! saved files.

use std::time::{Duration, Instant};

use axcenity_mcp::{Args, Class, Code, Kind, Outcome, Param, Profile, ToolError, ToolSpec};
use axcenity_testing::{Runner, Selection};

use super::refs::quoted;
use super::{Host, LIMIT};
use crate::output::{OutputKind, OutputLine};

pub const SPEC: ToolSpec = ToolSpec {
    name: "game_run",
    title: "Run the game",
    summary: "Play the game for some seconds and report script errors and output, or run its game tests.",
    profile: Profile::Lite,
    class: Class::Modify,
    idempotent: false,
    params: &[
        Param::optional("mode", Kind::Enum(&["play", "tests"]), "default play"),
        Param::optional("seconds", Kind::Number, "default 3, max 60"),
        Param::optional("file", Kind::String, "one spec file (tests)"),
        LIMIT,
    ],
};

/// The longest a play run lasts, in game seconds.
const MAX_SECONDS: f64 = 60.0;
/// Frames a game second, as the editor's tests step it.
const FRAME_RATE: f64 = 60.0;

pub fn call(host: &mut Host, args: &Args) -> Outcome {
    if host.app.project.is_none() {
        return Err(ToolError::not_found("no project is open"));
    }
    if host.app.playing() {
        return Err(ToolError::new(Code::Busy, "the game is already playing")
            .with_fix("stop Play in the editor, then try again"));
    }
    let limit = args.limit(30, 200);
    match args.str("mode").unwrap_or("play") {
        "tests" => tests(host, args.str("file"), limit),
        _ => {
            let seconds = args.f64("seconds").unwrap_or(3.0);
            if !(seconds > 0.0 && seconds <= MAX_SECONDS) {
                return Err(ToolError::new(
                    Code::BadValue,
                    format!("seconds {seconds}; give more than 0, up to {MAX_SECONDS}"),
                ));
            }
            play(host, seconds, limit)
        }
    }
}

fn play(host: &mut Host, seconds: f64, limit: usize) -> Outcome {
    let app = &mut host.app;
    let from = app.output.total();
    app.start_session(None);
    if !app.playing() {
        let lines = own_lines(app.output.since(from));
        return Err(ToolError::new(
            Code::Internal,
            format!("the game didn't start{}", report(&lines, limit)),
        ));
    }
    let frames = (seconds * FRAME_RATE).round().max(1.0) as u32;
    let mut ran = 0;
    let mut total = Duration::ZERO;
    let mut worst = Duration::ZERO;
    for _ in 0..frames {
        let started = Instant::now();
        app.play_frame(1.0 / FRAME_RATE, &[], None, None);
        let took = started.elapsed();
        total += took;
        worst = worst.max(took);
        ran += 1;
        // A game can end itself.
        if !app.playing() {
            break;
        }
    }
    app.stop();
    let lines = own_lines(app.output.since(from));
    let errors = lines
        .iter()
        .filter(|line| line.kind == OutputKind::Error)
        .count();
    let ms = |time: Duration| time.as_secs_f64() * 1000.0;
    Ok(format!(
        "played {:.1} s ({ran} frames; avg {:.1} ms, worst {:.1} ms) · {errors} error{} · stopped, the world is as before{}",
        f64::from(ran) / FRAME_RATE,
        ms(total) / f64::from(ran.max(1)),
        ms(worst),
        if errors == 1 { "" } else { "s" },
        report(&lines, limit)
    ))
}

/// The game's own lines: without the editor's "Playing…" and "Stopped…".
fn own_lines(lines: &[OutputLine]) -> Vec<&OutputLine> {
    lines
        .iter()
        .filter(|line| {
            !(line.kind == OutputKind::Info
                && (line.text.starts_with("Playing \"") || line.text.starts_with("Stopped.")))
        })
        .collect()
}

/// Every error, then the newest other lines, `limit` in all, each on a
/// line of its own.
fn report(lines: &[&OutputLine], limit: usize) -> String {
    let (errors, others): (Vec<&OutputLine>, Vec<&OutputLine>) = lines
        .iter()
        .partition(|line| line.kind == OutputKind::Error);
    let mut out = String::new();
    for line in errors.iter().take(limit) {
        out.push_str(&format!("\nerror {}", line.text));
    }
    let room = limit.saturating_sub(errors.len());
    let skipped = others.len().saturating_sub(room);
    if skipped > 0 && room > 0 {
        out.push_str(&format!("\n… {skipped} earlier lines"));
    }
    for line in others.iter().skip(skipped) {
        let kind = match line.kind {
            OutputKind::Warning => "warning ",
            _ => "",
        };
        out.push_str(&format!("\n{kind}{}", line.text));
    }
    out
}

fn tests(host: &mut Host, file: Option<&str>, limit: usize) -> Outcome {
    let Some(folder) = host.app.project.as_ref().map(|p| p.folder.clone()) else {
        return Err(ToolError::not_found("no project is open"));
    };
    let mut runner = Runner::open(&folder)
        .map_err(|problem| ToolError::internal(format!("could not read the project: {problem}")))?;
    let files = runner.spec_files();
    if files.is_empty() {
        return Ok("no game tests: a test is an `it` in tests/<name>.spec.lua".to_owned());
    }
    let selection = match file {
        Some(file) if files.iter().any(|known| known == file) => Selection::File(file.to_owned()),
        Some(file) => {
            return Err(
                ToolError::not_found(format!("no spec file {}", quoted(file)))
                    .with_fix(format!("spec files: {}", files.join(", "))),
            );
        }
        None => Selection::All,
    };
    let results = runner.run(&selection, &mut |_| {});
    let failed: Vec<_> = results.iter().filter(|result| !result.passed()).collect();
    let mut out = format!(
        "{} passed, {} failed",
        results.len() - failed.len(),
        failed.len()
    );
    if host.app.unsaved() {
        out.push_str(" · unsaved changes aren't in this run (tests read the saved files; project_save first)");
    }
    for result in failed.iter().take(limit) {
        let message = result
            .failure
            .as_ref()
            .map_or("", |failure| failure.message.as_str());
        out.push_str(&format!(
            "\nFAIL {}:{} {}: {message}",
            result.file,
            result.line,
            result.title()
        ));
        // What the game said just before, which often explains it.
        for line in result.output.iter().rev().take(3).rev() {
            out.push_str(&format!("\n  {line}"));
        }
    }
    if failed.len() > limit {
        out.push_str(&format!("\n… {} more failures", failed.len() - limit));
    }
    Ok(out)
}
