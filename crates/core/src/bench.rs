//! Frame-stage timing for benchmarks (roadmap item I1, "measure first").
//!
//! A host times each frame's stages with [`Laps`], then hands them to
//! [`StageSamples`], which keeps every frame and reports the mean, median,
//! 95th percentile and worst time of each stage. Every host (player,
//! editor Play, dedicated server, test runner) reports in the same shape,
//! so their numbers can be put side by side. How to run them:
//! `docs/development/benchmarks.md`.
//!
//! This only uses `std::time`, so it costs nothing to keep in headless
//! builds. Profiler spans (the `profiling` crate) are separate: they show
//! the same stages in Tracy with the `profile-tracy` feature.

use std::fmt::Write as _;
use std::time::{Duration, Instant};

/// The time of each stage of one frame, timed one after another.
#[derive(Debug, Clone)]
pub struct Laps {
    started: Instant,
    last: Instant,
    stages: Vec<(&'static str, Duration)>,
}

impl Default for Laps {
    fn default() -> Self {
        Self::start()
    }
}

impl Laps {
    /// Starts timing a frame.
    pub fn start() -> Self {
        let now = Instant::now();
        Self {
            started: now,
            last: now,
            stages: Vec::new(),
        }
    }

    /// Ends the stage `name`: the time since the last lap (or the start).
    pub fn lap(&mut self, name: &'static str) {
        let now = Instant::now();
        self.stages.push((name, now - self.last));
        self.last = now;
    }

    /// Each stage's time, in the order they ran.
    pub fn stages(&self) -> &[(&'static str, Duration)] {
        &self.stages
    }

    /// The time from the start to the last lap.
    pub fn total(&self) -> Duration {
        self.last - self.started
    }
}

/// One stage's times over a run, summarized.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StageSummary {
    pub name: &'static str,
    pub mean: Duration,
    pub median: Duration,
    pub p95: Duration,
    pub max: Duration,
}

/// Every frame's stage times over a run.
#[derive(Debug, Default, Clone)]
pub struct StageSamples {
    frames: Vec<Duration>,
    /// Stage names in the order first seen, with every frame's time.
    stages: Vec<(&'static str, Vec<Duration>)>,
}

impl StageSamples {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a frame `total` long whose stages took `stages`. A stage a
    /// frame skipped counts as zero for it, so means stay per frame.
    pub fn record(&mut self, total: Duration, stages: &[(&'static str, Duration)]) {
        let frame = self.frames.len();
        self.frames.push(total);
        for &(name, time) in stages {
            let index = match self.stages.iter().position(|(seen, _)| *seen == name) {
                Some(index) => index,
                None => {
                    self.stages.push((name, vec![Duration::ZERO; frame]));
                    self.stages.len() - 1
                }
            };
            let times = &mut self.stages[index].1;
            // The same stage twice in a frame adds up.
            if times.len() > frame {
                times[frame] += time;
            } else {
                times.push(time);
            }
        }
        for (_, times) in &mut self.stages {
            times.resize(frame + 1, Duration::ZERO);
        }
    }

    /// Adds a frame timed with `laps`.
    pub fn record_laps(&mut self, laps: &Laps) {
        self.record(laps.total(), laps.stages());
    }

    /// How many frames were recorded.
    pub fn frames(&self) -> usize {
        self.frames.len()
    }

    /// The whole frame's times, summarized (named `frame`).
    pub fn frame_summary(&self) -> StageSummary {
        summarize("frame", &self.frames)
    }

    /// Each stage's times, summarized, in the order the stages ran.
    pub fn summaries(&self) -> Vec<StageSummary> {
        self.stages
            .iter()
            .map(|(name, times)| summarize(name, times))
            .collect()
    }

    /// A table for people: one row per stage, then the whole frame.
    pub fn report(&self, title: &str) -> String {
        let mut text = format!("{title} ({} frames)\n", self.frames());
        let _ = writeln!(
            text,
            "  {:<24} {:>10} {:>10} {:>10} {:>10}",
            "stage", "mean ms", "median ms", "p95 ms", "max ms"
        );
        for summary in self
            .summaries()
            .into_iter()
            .chain(std::iter::once(self.frame_summary()))
        {
            let _ = writeln!(
                text,
                "  {:<24} {:>10.3} {:>10.3} {:>10.3} {:>10.3}",
                summary.name,
                millis(summary.mean),
                millis(summary.median),
                millis(summary.p95),
                millis(summary.max)
            );
        }
        text
    }

    /// One line of JSON for tools and nightly trends:
    /// `{"host":…,"scene":…,"frames":…,"stages":[{"name":…,"mean_ms":…},…]}`.
    pub fn json(&self, host: &str, scene: &str) -> String {
        let mut text = format!(
            "{{\"host\":{},\"scene\":{},\"frames\":{},\"stages\":[",
            json_string(host),
            json_string(scene),
            self.frames()
        );
        let all = self
            .summaries()
            .into_iter()
            .chain(std::iter::once(self.frame_summary()));
        for (index, summary) in all.enumerate() {
            if index > 0 {
                text.push(',');
            }
            let _ = write!(
                text,
                "{{\"name\":{},\"mean_ms\":{:.4},\"median_ms\":{:.4},\"p95_ms\":{:.4},\"max_ms\":{:.4}}}",
                json_string(summary.name),
                millis(summary.mean),
                millis(summary.median),
                millis(summary.p95),
                millis(summary.max)
            );
        }
        text.push_str("]}");
        text
    }
}

/// A host's `--bench <frames>` run: frames to skip while caches fill and
/// physics settles, then frames to keep.
#[derive(Debug, Clone)]
pub struct BenchRun {
    warm_up: usize,
    frames: usize,
    seen: usize,
    pub samples: StageSamples,
    /// The very first frame, which reads whatever wasn't loaded ahead.
    first_frame: Option<Duration>,
    /// The slowest frame of the warm-up, where first-sight loads land.
    warm_up_worst: Duration,
    /// One-off times a host notes, such as launch to first frame.
    startup: Vec<(&'static str, Duration)>,
}

impl BenchRun {
    /// Frames skipped before measuring unless a host says otherwise.
    pub const DEFAULT_WARM_UP: usize = 60;

    /// Keeps `frames` frames after `warm_up` ones.
    pub fn new(warm_up: usize, frames: usize) -> Self {
        Self {
            warm_up,
            frames,
            seen: 0,
            samples: StageSamples::new(),
            first_frame: None,
            warm_up_worst: Duration::ZERO,
            startup: Vec::new(),
        }
    }

    /// Notes one frame. True once every frame to keep has been kept.
    pub fn record(&mut self, laps: &Laps) -> bool {
        self.seen += 1;
        let total = laps.total();
        self.first_frame.get_or_insert(total);
        if self.seen > self.warm_up {
            self.samples.record_laps(laps);
        } else {
            self.warm_up_worst = self.warm_up_worst.max(total);
        }
        self.done()
    }

    /// Notes a one-off time, such as launch to first frame, once: a second
    /// note of the same name is ignored.
    pub fn note(&mut self, name: &'static str, time: Duration) {
        if !self.startup.iter().any(|(seen, _)| *seen == name) {
            self.startup.push((name, time));
        }
    }

    /// Whether a one-off time called `name` was noted.
    pub fn noted(&self, name: &str) -> bool {
        self.startup.iter().any(|(seen, _)| *seen == name)
    }

    /// The first frame, the warm-up's worst frame, then each noted time,
    /// in milliseconds, in the order they're reported.
    pub fn startup(&self) -> Vec<(&'static str, Duration)> {
        let mut times = Vec::new();
        if let Some(first) = self.first_frame {
            times.push(("first frame", first));
            times.push(("worst warm-up frame", self.warm_up_worst));
        }
        times.extend(self.startup.iter().copied());
        times
    }

    /// True once every frame to keep has been kept.
    pub fn done(&self) -> bool {
        self.samples.frames() >= self.frames
    }

    /// The table, then the JSON line, as hosts print them when done.
    pub fn finish_text(&self, host: &str, scene: &str) -> String {
        let mut text = self.samples.report(&format!("{host}: {scene}"));
        let startup = self.startup();
        for (name, time) in &startup {
            let _ = writeln!(text, "  {:<24} {:>10.3}", name, millis(*time));
        }
        let mut json = self.samples.json(host, scene);
        // `"startup":{…}` goes in before the closing brace.
        json.pop();
        json.push_str(",\"startup\":{");
        for (index, (name, time)) in startup.iter().enumerate() {
            if index > 0 {
                json.push(',');
            }
            let _ = write!(json, "{}:{:.4}", json_string(name), millis(*time));
        }
        json.push_str("}}");
        format!("{text}{json}\n")
    }
}

/// The scene a benchmark ran: a project folder's last name.
pub fn scene_name(folder: &std::path::Path) -> String {
    folder
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| folder.display().to_string())
}

fn summarize(name: &'static str, times: &[Duration]) -> StageSummary {
    if times.is_empty() {
        return StageSummary {
            name,
            mean: Duration::ZERO,
            median: Duration::ZERO,
            p95: Duration::ZERO,
            max: Duration::ZERO,
        };
    }
    let mut sorted = times.to_vec();
    sorted.sort();
    let at = |fraction: f64| {
        let index = ((sorted.len() - 1) as f64 * fraction).round() as usize;
        sorted[index]
    };
    let sum: Duration = sorted.iter().sum();
    StageSummary {
        name,
        mean: sum / sorted.len() as u32,
        median: at(0.5),
        p95: at(0.95),
        max: *sorted.last().expect("not empty"),
    }
}

fn millis(time: Duration) -> f64 {
    time.as_secs_f64() * 1000.0
}

fn json_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn stages_are_summarized_per_frame_in_the_order_they_ran() {
        let mut samples = StageSamples::new();
        for n in 1..=100 {
            samples.record(ms(n + 1), &[("Scripts", ms(n)), ("Physics", ms(1))]);
        }
        let summaries = samples.summaries();
        assert_eq!(summaries[0].name, "Scripts");
        assert_eq!(summaries[1].name, "Physics");
        assert_eq!(summaries[0].max, ms(100));
        assert_eq!(summaries[0].median, ms(51));
        assert_eq!(summaries[0].p95, ms(95));
        assert_eq!(summaries[1].mean, ms(1));
        assert_eq!(samples.frame_summary().max, ms(101));
    }

    #[test]
    fn a_stage_a_frame_skips_counts_as_zero_and_repeats_add_up() {
        let mut samples = StageSamples::new();
        samples.record(ms(4), &[("Physics", ms(4))]);
        samples.record(ms(6), &[("Physics", ms(2)), ("Nav", ms(3)), ("Nav", ms(1))]);
        let summaries = samples.summaries();
        let nav = summaries.iter().find(|s| s.name == "Nav").unwrap();
        assert_eq!(nav.max, ms(4));
        assert_eq!(nav.mean, ms(2));
    }

    #[test]
    fn the_json_line_names_the_host_scene_and_every_stage() {
        let mut samples = StageSamples::new();
        samples.record(ms(2), &[("Scripts", ms(2))]);
        let json = samples.json("server", "S10k \"v1\"");
        assert!(json.starts_with("{\"host\":\"server\",\"scene\":\"S10k \\\"v1\\\"\""));
        assert!(json.contains("\"name\":\"Scripts\""));
        assert!(json.contains("\"name\":\"frame\""));
        assert!(json.ends_with("]}"));
    }

    #[test]
    fn a_bench_run_skips_the_warm_up_then_keeps_its_frames() {
        let mut run = BenchRun::new(2, 3);
        let laps = Laps::start();
        assert!(!run.record(&laps));
        assert!(!run.record(&laps));
        assert_eq!(run.samples.frames(), 0);
        assert!(!run.record(&laps));
        assert!(!run.record(&laps));
        assert!(run.record(&laps));
        assert_eq!(run.samples.frames(), 3);
        assert!(
            run.finish_text("server", "S10k")
                .contains("\"host\":\"server\"")
        );
    }

    #[test]
    fn a_bench_run_reports_its_first_frame_worst_warm_up_and_notes() {
        let mut run = BenchRun::new(2, 1);
        let mut slow = Laps::start();
        std::thread::sleep(ms(5));
        slow.lap("Draw list");
        run.record(&Laps::start());
        run.record(&slow);
        run.record(&Laps::start());
        run.note("launch to first frame", ms(40));
        run.note("launch to first frame", ms(99));
        let startup = run.startup();
        let names: Vec<_> = startup.iter().map(|(name, _)| *name).collect();
        assert_eq!(
            names,
            [
                "first frame",
                "worst warm-up frame",
                "launch to first frame"
            ]
        );
        assert!(startup[0].1 < ms(5), "the first frame was the fast one");
        assert!(startup[1].1 >= ms(5), "the slow frame was in the warm-up");
        assert_eq!(startup[2].1, ms(40), "a second note is ignored");
        let text = run.finish_text("player", "S10k");
        let json = text.lines().last().unwrap();
        assert!(
            json.ends_with("\"launch to first frame\":40.0000}}"),
            "{json}"
        );
        assert!(json.contains(",\"startup\":{\"first frame\":"), "{json}");
    }

    #[test]
    fn laps_time_one_stage_after_another() {
        let mut laps = Laps::start();
        laps.lap("A");
        laps.lap("B");
        let names: Vec<_> = laps.stages().iter().map(|(name, _)| *name).collect();
        assert_eq!(names, ["A", "B"]);
        let sum: Duration = laps.stages().iter().map(|(_, t)| *t).sum();
        assert_eq!(sum, laps.total());
    }
}
