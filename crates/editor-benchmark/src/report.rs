//! What a benchmark run found: the machine, each measurement, and the
//! recommendation. Saved with the editor's settings (one report, the last
//! run), so reopening the window shows it without running again.

use axcenity_render::{GraphicsSettings, Quality};
use serde::{Deserialize, Serialize};

use super::workloads::Workload;

/// The report's shape. A saved report of another version isn't read.
pub const REPORT_VERSION: u32 = 1;

/// The four presets the benchmark weighs, named for what they're for.
/// Each is one of Settings > Graphics' quality presets.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Tier {
    Performance,
    Balanced,
    HighQuality,
    MaximumQuality,
}

impl Tier {
    pub const ALL: [Tier; 4] = [
        Tier::Performance,
        Tier::Balanced,
        Tier::HighQuality,
        Tier::MaximumQuality,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Tier::Performance => "Performance",
            Tier::Balanced => "Balanced",
            Tier::HighQuality => "High Quality",
            Tier::MaximumQuality => "Maximum Quality",
        }
    }

    /// The Settings > Graphics preset it is.
    pub fn quality(self) -> Quality {
        match self {
            Tier::Performance => Quality::Low,
            Tier::Balanced => Quality::Medium,
            Tier::HighQuality => Quality::High,
            Tier::MaximumQuality => Quality::Ultra,
        }
    }

    pub fn of(quality: Quality) -> Tier {
        match quality {
            Quality::Low => Tier::Performance,
            Quality::Medium => Tier::Balanced,
            Quality::High => Tier::HighQuality,
            Quality::Ultra => Tier::MaximumQuality,
        }
    }

    pub fn about(self) -> &'static str {
        match self {
            Tier::Performance => {
                "The smoothest viewport: no post-processing or ambient occlusion, small \
                 shadow maps, and the scene drawn at 75% and scaled up."
            }
            Tier::Balanced => {
                "Most of the look for less: FXAA, light ambient occlusion and medium shadows."
            }
            Tier::HighQuality => {
                "The look worlds are made with: MSAA 4×, full ambient occlusion, three \
                 shadow cascades and contact shadows."
            }
            Tier::MaximumQuality => {
                "Everything at its best: 4096² shadows in four cascades reaching 300 studs, \
                 volumetric fog and the finest reflections and clouds."
            }
        }
    }

    pub fn settings(self) -> GraphicsSettings {
        GraphicsSettings::preset(self.quality())
    }

    pub fn next(self) -> Option<Tier> {
        Tier::ALL.get(self as usize + 1).copied()
    }
}

/// Times in milliseconds over the measured frames.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Timing {
    pub median: f32,
    /// 95% of frames took this long or less.
    pub p95: f32,
    pub p99: f32,
    pub worst: f32,
}

impl Timing {
    /// The timing of `samples` (milliseconds), or zeros for none.
    pub fn of(samples: &[f32]) -> Timing {
        if samples.is_empty() {
            return Timing::default();
        }
        let mut sorted = samples.to_vec();
        sorted.sort_by(f32::total_cmp);
        let at = |fraction: f32| {
            let index = ((sorted.len() - 1) as f32 * fraction).round() as usize;
            sorted[index.min(sorted.len() - 1)]
        };
        Timing {
            median: at(0.5),
            p95: at(0.95),
            p99: at(0.99),
            worst: sorted[sorted.len() - 1],
        }
    }
}

/// One pass's median GPU time.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PassTime {
    pub pass: String,
    pub ms: f32,
}

/// One workload drawn with one set of settings.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Measurement {
    pub workload: Workload,
    /// The preset (for the town) or what the settings were called.
    pub label: String,
    pub settings: GraphicsSettings,
    pub frames: u32,
    /// The CPU's part of a frame: the world turned into a draw list, then
    /// the frame recorded and submitted.
    pub cpu: Timing,
    /// The CPU's draw list alone.
    pub draw_list: Timing,
    /// The GPU's time from its own timestamps, where it can take them.
    pub gpu: Option<Timing>,
    /// A frame: the longer of the CPU's and GPU's times (they overlap in
    /// the editor), or the whole wait where the GPU can't be timed.
    pub frame: Timing,
    /// The first frame after switching to these settings: pipelines,
    /// shaders and frame targets made for them.
    pub first_frame_ms: f32,
    /// Frames that took more than twice the median.
    pub hitches: u32,
    /// The GPU's median time for each pass that drew.
    pub passes: Vec<PassTime>,
    /// Bytes the renderer holds on the GPU: frame targets, shadow maps,
    /// textures, meshes and buffers.
    pub gpu_bytes: u64,
    pub triangles: u64,
    pub draw_calls: u32,
}

impl Measurement {
    /// Frames per second its 95th-percentile frame allows.
    pub fn fps(&self) -> f32 {
        1000.0 / self.frame.p95.max(0.01)
    }

    /// Whether the GPU (not the CPU) set the frame's length.
    pub fn gpu_bound(&self) -> bool {
        self.gpu.is_some_and(|gpu| gpu.p95 >= self.cpu.p95)
    }

    pub fn pass(&self, name: &str) -> f32 {
        self.passes
            .iter()
            .find(|pass| pass.pass == name)
            .map_or(0.0, |pass| pass.ms)
    }
}

/// The graphics adapter, as wgpu reports it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Adapter {
    pub name: String,
    /// "Dx12", "Vulkan", "Metal"…
    pub backend: String,
    /// "DiscreteGpu", "IntegratedGpu", "Cpu"…
    pub kind: String,
    pub driver: String,
    /// Draws on the CPU, not a graphics card.
    pub software: bool,
    pub timestamps: bool,
}

/// The benchmark's advice, and why.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Recommendation {
    pub tier: Tier,
    /// What Apply sets: the tier's preset and any overrides.
    pub settings: GraphicsSettings,
    /// One line, such as "High Quality, with Medium shadows".
    pub title: String,
    /// Why, one finding a line, most important first.
    pub reasons: Vec<String>,
}

/// A whole run.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Report {
    pub version: u32,
    /// Seconds since 1970 (UTC) when it finished.
    pub finished_at: u64,
    pub adapter: Adapter,
    /// The graphics card's own memory in bytes, where the system says.
    pub video_memory: Option<u64>,
    pub cpu_threads: usize,
    /// The size drawn at: the viewport's, in pixels.
    pub size: (u32, u32),
    /// Making the renderer: its pipelines and shaders.
    pub setup_ms: f32,
    /// The town at each preset, Performance first. A preset left out was
    /// skipped because a lower one was already far too slow.
    pub presets: Vec<Measurement>,
    /// The next preset up, with the setting that made it too slow lowered.
    pub tuned: Option<Measurement>,
    /// Geometry, Materials, and Lights at the recommended settings.
    pub stress: Vec<Measurement>,
    pub recommendation: Recommendation,
}

impl Report {
    pub fn preset(&self, tier: Tier) -> Option<&Measurement> {
        self.presets
            .iter()
            .find(|measurement| Tier::of(measurement.settings.quality) == tier)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles_come_from_the_sorted_samples() {
        let samples: Vec<f32> = (1..=100).rev().map(|n| n as f32).collect();
        let timing = Timing::of(&samples);
        assert_eq!(timing.median, 51.0);
        assert_eq!(timing.p95, 95.0);
        assert_eq!(timing.p99, 99.0);
        assert_eq!(timing.worst, 100.0);
        assert_eq!(Timing::of(&[]), Timing::default());
        assert_eq!(Timing::of(&[4.0]).p99, 4.0);
    }

    #[test]
    fn each_tier_is_one_preset() {
        for tier in Tier::ALL {
            assert_eq!(Tier::of(tier.quality()), tier);
            assert_eq!(tier.settings(), GraphicsSettings::preset(tier.quality()));
        }
        assert_eq!(Tier::Performance.next(), Some(Tier::Balanced));
        assert_eq!(Tier::MaximumQuality.next(), None);
    }
}
