//! Running the benchmark: a renderer of its own on the editor's GPU device,
//! drawing the workloads a batch of frames at a time so the editor's window
//! keeps drawing (and shows the progress) in between.
//!
//! Each frame is timed as the viewport's would be: the world turned into a
//! draw list and recorded on the CPU, then the GPU's own timestamps (where
//! the device takes them). The CPU waits for the GPU after each frame, so
//! frames don't pile up and every frame's numbers are its own; the frame's
//! length is the longer of the two, as they overlap in the editor.

use std::collections::HashMap;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use axcenity_assets::Assets;
use axcenity_core::World;
use axcenity_render::{FramePass, GraphicsSettings, RenderStats, TextureRenderer, wgpu};

use super::recommend::{self, worth_going_on};
use super::report::{
    Adapter, Measurement, PassTime, REPORT_VERSION, Recommendation, Report, Tier, Timing,
};
use super::video_memory;
use super::workloads::Workload;

/// How many frames each measurement takes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plan {
    /// Frames drawn and thrown away after the first, while caches fill.
    pub warmup: u32,
    /// Frames measured for each preset (and the tuned one).
    pub preset_frames: u32,
    /// Frames measured for each heavier workload.
    pub stress_frames: u32,
    /// How long one batch of frames may take before the editor's window
    /// gets to draw again.
    pub batch: Duration,
}

impl Plan {
    /// About 15–30 seconds on a graphics card.
    pub const FULL: Plan = Plan {
        warmup: 20,
        preset_frames: 180,
        stress_frames: 90,
        batch: Duration::from_millis(50),
    };

    /// A handful of frames each: for tests on a software adapter.
    #[cfg(test)]
    pub const QUICK: Plan = Plan {
        warmup: 1,
        preset_frames: 4,
        stress_frames: 2,
        batch: Duration::from_secs(600),
    };

    /// Frames the whole run draws, the tuned preset included.
    fn total_frames(&self) -> u32 {
        let preset = 1 + self.warmup + self.preset_frames;
        let stress = 1 + self.warmup + self.stress_frames;
        preset * (Tier::ALL.len() as u32 + 1) + stress * Workload::STRESS.len() as u32
    }
}

/// What's being measured now.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Phase {
    /// The town at `Tier::ALL[index]`.
    Preset(usize),
    Tune,
    /// `Workload::STRESS[index]` at the settings that look best so far.
    Stress(usize),
    Done,
}

/// One measurement in progress.
struct Active {
    workload: Workload,
    label: String,
    settings: GraphicsSettings,
    frames: u32,
    /// Frames drawn so far, the first and warm-up ones included.
    drawn: u32,
    first_frame_ms: f32,
    cpu: Vec<f32>,
    draw_list: Vec<f32>,
    gpu: Vec<f32>,
    frame: Vec<f32>,
    passes: [Vec<f32>; FramePass::COUNT],
    /// `RenderStats::gpu_readings` when the last reading was taken.
    readings: u64,
    last: RenderStats,
}

/// How far a run has got.
pub enum Progress {
    Running,
    Finished(Box<Report>),
    Failed(String),
}

pub struct Run {
    plan: Plan,
    device: wgpu::Device,
    renderer: TextureRenderer,
    assets: Assets,
    worlds: HashMap<Workload, World>,
    adapter: Adapter,
    video_memory: Option<JoinHandle<Option<u64>>>,
    size: (u32, u32),
    setup_ms: f32,
    phase: Phase,
    active: Option<Active>,
    presets: Vec<Measurement>,
    tuned: Option<Measurement>,
    stress: Vec<Measurement>,
    /// Frames drawn so far, for the progress bar.
    drawn: u32,
}

/// Milliseconds in `duration`.
fn ms(duration: Duration) -> f32 {
    duration.as_secs_f32() * 1000.0
}

impl Run {
    /// Starts a run on the editor's device, drawing at `size` (the
    /// viewport's, in pixels). Making the renderer (its pipelines and
    /// shaders) is the first thing timed.
    pub fn start(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        info: &wgpu::AdapterInfo,
        size: (u32, u32),
        plan: Plan,
    ) -> Run {
        let size = (size.0.clamp(320, 3840), size.1.clamp(180, 2160));
        let started = Instant::now();
        let renderer = TextureRenderer::new(device, queue, size.0, size.1);
        let setup_ms = ms(started.elapsed());
        let capabilities = renderer.capabilities();
        let adapter = Adapter {
            name: info.name.clone(),
            backend: format!("{:?}", info.backend),
            kind: format!("{:?}", info.device_type),
            driver: format!("{} {}", info.driver, info.driver_info)
                .trim()
                .to_owned(),
            // `TextureRenderer` reads its capabilities without the adapter's
            // kind, so the kind is checked here too.
            software: capabilities.software || info.device_type == wgpu::DeviceType::Cpu,
            timestamps: capabilities.timestamp_queries,
        };
        let (vendor, id) = (info.vendor, info.device);
        let video_memory = std::thread::Builder::new()
            .name("video memory".to_owned())
            .spawn(move || video_memory::read(vendor, id))
            .ok();
        Run {
            plan,
            device: device.clone(),
            renderer,
            // Parts need no files.
            assets: Assets::default(),
            worlds: HashMap::new(),
            adapter,
            video_memory,
            size,
            setup_ms,
            phase: Phase::Preset(0),
            active: None,
            presets: Vec::new(),
            tuned: None,
            stress: Vec::new(),
            drawn: 0,
        }
    }

    /// The share of the run done, 0 to 1.
    pub fn fraction(&self) -> f32 {
        (self.drawn as f32 / self.plan.total_frames() as f32).min(0.99)
    }

    /// What's being measured, for the progress bar.
    pub fn stage(&self) -> String {
        match (&self.phase, &self.active) {
            (Phase::Preset(index), _) => format!(
                "Drawing the test town at {} ({} of {})",
                Tier::ALL[*index].label(),
                index + 1,
                Tier::ALL.len()
            ),
            (Phase::Tune, Some(active)) => format!("Trying {}", active.label),
            (Phase::Tune, None) => "Choosing what to try next".to_owned(),
            (Phase::Stress(index), _) => format!(
                "Heavier test: {} ({} of {})",
                Workload::STRESS[*index].label(),
                index + 1,
                Workload::STRESS.len()
            ),
            (Phase::Done, _) => "Working out the recommendation".to_owned(),
        }
    }

    /// Draws frames for up to the plan's batch time, and says how far the
    /// run is. Call once per editor frame while it's `Running`.
    pub fn step(&mut self) -> Progress {
        // Let whatever the editor drew finish first, so it isn't counted.
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
        let started = Instant::now();
        while started.elapsed() < self.plan.batch {
            if self.active.is_none() {
                match self.next_measurement() {
                    Some(active) => {
                        self.renderer.set_graphics(&active.settings);
                        self.active = Some(active);
                    }
                    None => return Progress::Finished(Box::new(self.finish())),
                }
            }
            if let Err(error) = self.frame() {
                return Progress::Failed(format!("Drawing failed: {error}"));
            }
            let active = self.active.as_ref().expect("a measurement is running");
            if active.drawn >= 1 + self.plan.warmup + active.frames {
                let active = self.active.take().expect("a measurement is running");
                self.measured(active);
            }
        }
        Progress::Running
    }

    /// The next measurement, moving through the phases; `None` when done.
    fn next_measurement(&mut self) -> Option<Active> {
        loop {
            let (workload, label, settings, frames) = match self.phase {
                Phase::Preset(index) => {
                    let tier = Tier::ALL[index];
                    (
                        Workload::Town,
                        tier.label().to_owned(),
                        tier.settings(),
                        self.plan.preset_frames,
                    )
                }
                Phase::Tune => match recommend::tuning(&self.presets) {
                    Some((settings, words)) => {
                        (Workload::Town, words, settings, self.plan.preset_frames)
                    }
                    None => {
                        self.drawn += 1 + self.plan.warmup + self.plan.preset_frames;
                        self.phase = Phase::Stress(0);
                        continue;
                    }
                },
                Phase::Stress(index) => {
                    let workload = Workload::STRESS[index];
                    let settings = recommend::recommend(&self.report()).settings;
                    (
                        workload,
                        workload.label().to_owned(),
                        settings,
                        self.plan.stress_frames,
                    )
                }
                Phase::Done => return None,
            };
            return Some(Active {
                workload,
                label,
                settings,
                frames,
                drawn: 0,
                first_frame_ms: 0.0,
                cpu: Vec::new(),
                draw_list: Vec::new(),
                gpu: Vec::new(),
                frame: Vec::new(),
                passes: Default::default(),
                readings: self.renderer.stats().gpu_readings,
                last: RenderStats::default(),
            });
        }
    }

    /// Draws one frame of the measurement running, and times it.
    fn frame(&mut self) -> anyhow::Result<()> {
        let active = self.active.as_mut().expect("a measurement is running");
        let world = self
            .worlds
            .entry(active.workload)
            .or_insert_with(|| active.workload.build());
        let camera = active.workload.camera(active.drawn);
        let started = Instant::now();
        let list = axcenity_scene::build_draw_list(world, &self.assets, camera);
        let built = started.elapsed();
        self.renderer.render(&list)?;
        let recorded = started.elapsed();
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
        let wall = started.elapsed();
        drop(list);
        self.drawn += 1;
        active.drawn += 1;
        let stats = self.renderer.stats();
        if active.drawn == 1 {
            // The first frame with these settings: their pipelines, shaders
            // and frame targets are made now.
            active.first_frame_ms = ms(wall);
            active.readings = stats.gpu_readings;
            return Ok(());
        }
        let warming = active.drawn <= 1 + self.plan.warmup;
        // A new GPU reading (for a frame or two back) is kept after the
        // warm-up; its frame was measured too.
        let reading = (stats.gpu_readings > active.readings)
            .then_some(stats.gpu_time)
            .flatten();
        active.readings = stats.gpu_readings;
        if warming {
            return Ok(());
        }
        let cpu = ms(recorded);
        active.cpu.push(cpu);
        active.draw_list.push(ms(built));
        let frame = match reading {
            Some(gpu) => {
                active.gpu.push(ms(gpu));
                for (times, pass) in active.passes.iter_mut().zip(FramePass::ALL) {
                    if let Some(time) = stats.pass_time(pass) {
                        times.push(ms(time));
                    }
                }
                cpu.max(ms(gpu))
            }
            // No timestamps: the whole wait, CPU and GPU in a row.
            None if !self.adapter.timestamps => ms(wall),
            // A reading still on its way: the last one stands in.
            None => cpu.max(active.gpu.last().copied().unwrap_or(ms(wall) - cpu)),
        };
        active.frame.push(frame);
        active.last = stats;
        Ok(())
    }

    /// Files a finished measurement and moves the phase on.
    fn measured(&mut self, active: Active) {
        let frame = Timing::of(&active.frame);
        let measurement = Measurement {
            workload: active.workload,
            label: active.label,
            settings: active.settings,
            frames: active.frame.len() as u32,
            cpu: Timing::of(&active.cpu),
            draw_list: Timing::of(&active.draw_list),
            gpu: (!active.gpu.is_empty()).then(|| Timing::of(&active.gpu)),
            frame,
            first_frame_ms: active.first_frame_ms,
            hitches: active
                .frame
                .iter()
                .filter(|&&ms| ms > frame.median * 2.0)
                .count() as u32,
            passes: FramePass::ALL
                .into_iter()
                .zip(&active.passes)
                .filter(|(_, times)| !times.is_empty())
                .map(|(pass, times)| PassTime {
                    pass: pass.label().to_owned(),
                    ms: Timing::of(times).median,
                })
                .collect(),
            gpu_bytes: active.last.texture_bytes
                + active.last.mesh_bytes
                + active.last.target_bytes
                + active.last.buffer_bytes,
            triangles: active.last.triangles,
            draw_calls: active.last.draw_calls,
        };
        self.phase = match self.phase {
            Phase::Preset(index) => {
                let go_on = worth_going_on(&measurement);
                self.presets.push(measurement);
                if go_on && index + 1 < Tier::ALL.len() {
                    Phase::Preset(index + 1)
                } else {
                    // The presets above are left out: the frames they'd
                    // have drawn count as done.
                    let skipped = Tier::ALL.len() - 1 - index;
                    self.drawn += skipped as u32 * (1 + self.plan.warmup + self.plan.preset_frames);
                    Phase::Tune
                }
            }
            Phase::Tune => {
                self.tuned = Some(measurement);
                Phase::Stress(0)
            }
            Phase::Stress(index) => {
                self.stress.push(measurement);
                if index + 1 < Workload::STRESS.len() {
                    Phase::Stress(index + 1)
                } else {
                    Phase::Done
                }
            }
            Phase::Done => Phase::Done,
        };
    }

    /// What's been measured so far, as a report (its recommendation not
    /// yet worked out).
    fn report(&self) -> Report {
        Report {
            version: REPORT_VERSION,
            finished_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |since| since.as_secs()),
            adapter: self.adapter.clone(),
            video_memory: None,
            cpu_threads: std::thread::available_parallelism().map_or(1, |n| n.get()),
            size: self.size,
            setup_ms: self.setup_ms,
            presets: self.presets.clone(),
            tuned: self.tuned.clone(),
            stress: self.stress.clone(),
            recommendation: Recommendation {
                tier: Tier::HighQuality,
                settings: GraphicsSettings::default(),
                title: String::new(),
                reasons: Vec::new(),
            },
        }
    }

    fn finish(&mut self) -> Report {
        let mut report = self.report();
        report.video_memory = self
            .video_memory
            .take()
            .and_then(|thread| thread.join().ok())
            .flatten();
        report.recommendation = recommend::recommend(&report);
        report
    }
}
