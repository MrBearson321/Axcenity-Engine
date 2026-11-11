//! Turning measurements into advice. Plain functions of the numbers, so
//! tests can check each rule without a GPU.
//!
//! The rule: the best preset whose test-town frame (95th percentile) fits
//! the viewport's budget, a share of a 60 fps frame. When the next preset
//! up only just misses, the one setting that grew the most is lowered and
//! that combination measured too ([`tuning`]). Below the lowest preset, a
//! GPU-bound machine gets a lower render scale. The texture memory budget
//! follows the card's memory where the system reports it.

use axcenity_render::{GraphicsSettings, ShadowQuality};

use super::report::{Measurement, Recommendation, Report, Tier};
use super::workloads::Workload;

/// The frame rate the viewport should keep while building.
pub const TARGET_FPS: f32 = 60.0;
/// The viewport's share of a frame at [`TARGET_FPS`], in milliseconds:
/// two-thirds, leaving the rest for the editor's panels and Play's
/// simulation.
pub const FRAME_BUDGET_MS: f32 = 1000.0 / TARGET_FPS * 2.0 / 3.0;
/// A preset this many times over budget means the ones above it aren't
/// worth drawing: they'd only be slower.
pub const SKIP_FACTOR: f32 = 3.0;
/// The lowest render scale the benchmark suggests.
const MIN_RENDER_SCALE: f32 = 0.5;
/// Frames with hitches above this share of the measured frames read as
/// uneven.
const UNEVEN_SHARE: f32 = 0.05;

pub fn fits(measurement: &Measurement) -> bool {
    measurement.frame.p95 <= FRAME_BUDGET_MS
}

/// Whether the presets above `measurement`'s are worth measuring.
pub fn worth_going_on(measurement: &Measurement) -> bool {
    measurement.frame.p95 <= FRAME_BUDGET_MS * SKIP_FACTOR
}

/// The best preset that fits, if any does.
pub fn best_fitting(presets: &[Measurement]) -> Option<Tier> {
    presets
        .iter()
        .filter(|measurement| fits(measurement))
        .map(|measurement| Tier::of(measurement.settings.quality))
        .max()
}

/// When the best fitting preset's next one up was measured and missed:
/// that preset with the setting whose pass grew the most set back to the
/// lower preset's value, and words for it ("Medium shadows"). `None` when
/// there's nothing to try.
pub fn tuning(presets: &[Measurement]) -> Option<(GraphicsSettings, String)> {
    let best = best_fitting(presets)?;
    let next = best.next()?;
    let find = |tier: Tier| {
        presets
            .iter()
            .find(|measurement| Tier::of(measurement.settings.quality) == tier)
    };
    let (lower, upper) = (find(best)?, find(next)?);
    if fits(upper) {
        return None;
    }
    let low = &lower.settings;
    let mut tuned = upper.settings.clone();
    // The passes by how much they grew, most first.
    let mut growth: Vec<(&str, f32)> = ["Shadows", "Ambient occlusion", "Volumetric fog", "Scene"]
        .into_iter()
        .map(|pass| {
            let grew = upper.pass(pass) - lower.pass(pass);
            // The prepass is drawn for ambient occlusion.
            let grew = if pass == "Ambient occlusion" {
                grew + upper.pass("Depth prepass") - lower.pass("Depth prepass")
            } else {
                grew
            };
            (pass, grew)
        })
        .filter(|(_, grew)| *grew > 0.0)
        .collect();
    growth.sort_by(|a, b| b.1.total_cmp(&a.1));
    for (pass, _) in growth {
        let words = match pass {
            "Shadows" if tuned.shadow_quality() != low.shadow_quality() => {
                tuned.shadow_quality = Some(low.shadow_quality());
                format!("{} shadows", low.shadow_quality().label())
            }
            "Ambient occlusion" if tuned.ambient_occlusion() != low.ambient_occlusion() => {
                tuned.ambient_occlusion = Some(low.ambient_occlusion());
                format!("{} ambient occlusion", low.ambient_occlusion().label())
            }
            "Volumetric fog" if tuned.volumetric_fog() => {
                tuned.volumetric_fog = Some(false);
                "no volumetric fog".to_owned()
            }
            "Scene" if tuned.anti_aliasing() != low.anti_aliasing() => {
                tuned.anti_aliasing = Some(low.anti_aliasing());
                format!("{} anti-aliasing", low.anti_aliasing().label())
            }
            "Scene" if tuned.cloud_quality() != low.cloud_quality() => {
                tuned.cloud_quality = Some(low.cloud_quality());
                format!("{} clouds", low.cloud_quality().label())
            }
            _ => continue,
        };
        return Some((tuned, words));
    }
    None
}

/// The render scale that should bring a GPU-bound `measurement` within
/// budget: the GPU's work goes with the pixels drawn, so with the square
/// root of the time over. Rounded down to 5%, never below 50%, and `None`
/// when the preset's own scale already does.
fn render_scale_for(measurement: &Measurement) -> Option<f32> {
    let gpu = measurement.gpu?.p95;
    let current = measurement.settings.render_scale();
    let wanted = current * (FRAME_BUDGET_MS / gpu).sqrt();
    let wanted = ((wanted * 20.0).floor() / 20.0).max(MIN_RENDER_SCALE);
    (wanted < current).then_some(wanted)
}

/// The texture memory budget for a card with `video_memory` bytes: a
/// quarter of it in MiB, rounded down to a power of two, between 256 MiB
/// and 8 GiB.
pub fn texture_budget_for(video_memory: u64) -> u32 {
    let quarter = (video_memory / 4 / (1024 * 1024)).max(1);
    let power = 1u64 << (63 - quarter.leading_zeros());
    power.clamp(256, 8192) as u32
}

fn ms(value: f32) -> String {
    if value >= 10.0 {
        format!("{value:.0} ms")
    } else {
        format!("{value:.1} ms")
    }
}

fn gigabytes(bytes: u64) -> String {
    let gib = bytes as f64 / (1024.0 * 1024.0 * 1024.0);
    if gib >= 1.0 {
        format!("{gib:.1} GB")
    } else {
        format!("{} MB", bytes / (1024 * 1024))
    }
}

/// The recommendation for what `report` measured. `report.recommendation`
/// is ignored (it's what this makes).
pub fn recommend(report: &Report) -> Recommendation {
    let budget = ms(FRAME_BUDGET_MS);
    let mut reasons = Vec::new();
    let fitting = best_fitting(&report.presets);
    let tuned = report
        .tuned
        .as_ref()
        .filter(|tuned| fits(tuned) && fitting.is_some());

    // Draws on the CPU: Performance whatever the numbers.
    let (tier, mut settings, mut title, chosen) = if report.adapter.software {
        reasons.push(format!(
            "{} draws on the CPU, not a graphics card, so the lightest preset keeps \
             the viewport usable.",
            report.adapter.name
        ));
        let chosen = report.preset(Tier::Performance);
        (
            Tier::Performance,
            Tier::Performance.settings(),
            Tier::Performance.label().to_owned(),
            chosen,
        )
    } else if let Some(tuned) = tuned {
        let tier = Tier::of(tuned.settings.quality);
        let base = report.preset(tier);
        reasons.push(format!(
            "{} with {} drew the test town in {} (95% of frames), inside the {budget} \
             the viewport has to stay at {TARGET_FPS:.0} fps.",
            tier.label(),
            tuned.label,
            ms(tuned.frame.p95),
        ));
        if let Some(base) = base {
            reasons.push(format!(
                "{} on its own took {}: lowering that one setting was enough.",
                tier.label(),
                ms(base.frame.p95),
            ));
        }
        (
            tier,
            tuned.settings.clone(),
            format!("{}, with {}", tier.label(), tuned.label),
            Some(tuned),
        )
    } else if let Some(tier) = fitting {
        let chosen = report.preset(tier);
        if let Some(chosen) = chosen {
            reasons.push(format!(
                "{} drew the test town in {} (95% of frames), inside the {budget} the \
                 viewport has to stay at {TARGET_FPS:.0} fps.",
                tier.label(),
                ms(chosen.frame.p95),
            ));
        }
        match tier.next().and_then(|next| report.preset(next)) {
            Some(next) => reasons.push(format!(
                "{} took {}: over budget{}.",
                Tier::of(next.settings.quality).label(),
                ms(next.frame.p95),
                biggest_growth(chosen, next)
                    .map(|(pass, grew)| format!(", mostly {} (+{})", pass.to_lowercase(), ms(grew)))
                    .unwrap_or_default(),
            )),
            None if tier == Tier::MaximumQuality => {
                reasons.push("This computer draws every preset within budget.".to_owned());
            }
            None => {}
        }
        (tier, tier.settings(), tier.label().to_owned(), chosen)
    } else {
        // Not even Performance fits.
        let chosen = report.preset(Tier::Performance);
        let mut settings = Tier::Performance.settings();
        let mut title = Tier::Performance.label().to_owned();
        if let Some(low) = chosen {
            reasons.push(format!(
                "Even {} took {} a frame, over the {budget} the viewport has to stay at \
                 {TARGET_FPS:.0} fps.",
                Tier::Performance.label(),
                ms(low.frame.p95),
            ));
            if low.gpu_bound() {
                if let Some(scale) = render_scale_for(low) {
                    settings.render_scale = Some(scale);
                    title = format!("{title}, at {:.0}% render scale", scale * 100.0);
                    reasons.push(format!(
                        "The graphics card set the pace, so drawing fewer pixels helps: \
                         {:.0}% render scale draws the scene smaller and scales it up.",
                        scale * 100.0
                    ));
                }
            } else {
                reasons.push(format!(
                    "The CPU set the pace ({} a frame turning the world into a draw list \
                     and recording it), which graphics settings can't change much: \
                     big worlds will be slow to fly around on this computer.",
                    ms(low.cpu.p95)
                ));
            }
        }
        (Tier::Performance, settings, title, chosen)
    };

    // The texture budget, from the card's memory.
    if let Some(memory) = report.video_memory.filter(|_| !report.adapter.software) {
        let wanted = texture_budget_for(memory);
        let preset = GraphicsSettings::preset(settings.quality).texture_budget();
        // Lower when the preset asks too much of a small card; higher only
        // at Maximum Quality, where the card has room to spare.
        if wanted < preset || (tier == Tier::MaximumQuality && wanted > preset) {
            settings.texture_budget = Some(wanted);
            reasons.push(format!(
                "Textures may use {} of this card's {}: a quarter, leaving the rest for \
                 the frame and other programs (the preset's is {}).",
                gigabytes(u64::from(wanted) * 1024 * 1024),
                gigabytes(memory),
                gigabytes(u64::from(preset) * 1024 * 1024),
            ));
            if !title.contains(',') {
                title = format!(
                    "{title}, with a {} texture budget",
                    gigabytes(u64::from(wanted) * 1024 * 1024)
                );
            }
        }
    }

    if let Some(chosen) = chosen {
        let share = chosen.hitches as f32 / chosen.frames.max(1) as f32;
        reasons.push(if share > UNEVEN_SHARE {
            format!(
                "Frame times were uneven: {} of {} frames took over twice the usual \
                 time (worst {}). Other programs using the GPU, or the driver, can \
                 cause that; running again with them closed gives steadier numbers.",
                chosen.hitches,
                chosen.frames,
                ms(chosen.frame.worst),
            )
        } else {
            format!(
                "Frame times were steady: {} of {} frames took over twice the usual time.",
                chosen.hitches, chosen.frames,
            )
        });
        reasons.push(format!(
            "It keeps {} of graphics memory for the test town{}.",
            gigabytes(chosen.gpu_bytes),
            report
                .video_memory
                .map(|memory| format!(", of the card's {}", gigabytes(memory)))
                .unwrap_or_default(),
        ));
    }

    // What the heavier workloads showed, at these settings.
    for stress in &report.stress {
        match stress.workload {
            Workload::Geometry if !stress.gpu_bound() => reasons.push(format!(
                "With 20,000 parts the CPU took {} a frame, {} of it building the draw \
                 list: very large worlds are limited by the CPU here, not the settings.",
                ms(stress.cpu.p95),
                ms(stress.draw_list.p95),
            )),
            Workload::Lights if !fits(stress) => {
                let shadows = stress.pass("Shadows");
                reasons.push(format!(
                    "80 lamps (24 with shadows) took {} a frame{}. If a world with many \
                     shadow-casting lamps feels slow, lower Shadow quality first.",
                    ms(stress.frame.p95),
                    if shadows > 0.0 {
                        format!(", {} of it shadows", ms(shadows))
                    } else {
                        String::new()
                    }
                ));
            }
            _ => {}
        }
    }
    settings.automatic = false;
    // Overrides equal to the preset's own value say nothing: drop them.
    let preset = GraphicsSettings::preset(settings.quality);
    if settings.shadow_quality == Some(preset.shadow_quality()) {
        settings.shadow_quality = None::<ShadowQuality>;
    }
    Recommendation {
        tier,
        settings,
        title,
        reasons,
    }
}

/// The pass that grew most from `lower` to `upper`, and by how much.
fn biggest_growth<'a>(
    lower: Option<&Measurement>,
    upper: &'a Measurement,
) -> Option<(&'a str, f32)> {
    let lower = lower?;
    upper
        .passes
        .iter()
        .map(|pass| (pass.pass.as_str(), pass.ms - lower.pass(&pass.pass)))
        .filter(|(_, grew)| *grew > 0.05)
        .max_by(|a, b| a.1.total_cmp(&b.1))
}

#[cfg(test)]
mod tests {
    use super::super::report::{Adapter, PassTime, REPORT_VERSION, Timing};
    use super::*;
    use axcenity_render::{AmbientOcclusion, Quality};

    fn timing(ms: f32) -> Timing {
        Timing {
            median: ms * 0.9,
            p95: ms,
            p99: ms * 1.1,
            worst: ms * 1.2,
        }
    }

    fn measured(tier: Tier, frame: f32, passes: &[(&str, f32)]) -> Measurement {
        Measurement {
            workload: Workload::Town,
            label: tier.label().to_owned(),
            settings: tier.settings(),
            frames: 90,
            cpu: timing(2.0),
            draw_list: timing(1.0),
            gpu: Some(timing(frame)),
            frame: timing(frame),
            first_frame_ms: 30.0,
            hitches: 1,
            passes: passes
                .iter()
                .map(|(pass, ms)| PassTime {
                    pass: (*pass).to_owned(),
                    ms: *ms,
                })
                .collect(),
            gpu_bytes: 300 << 20,
            triangles: 1_000_000,
            draw_calls: 400,
        }
    }

    fn report(presets: Vec<Measurement>) -> Report {
        Report {
            version: REPORT_VERSION,
            finished_at: 0,
            adapter: Adapter {
                name: "Test GPU".to_owned(),
                ..Adapter::default()
            },
            video_memory: None,
            cpu_threads: 8,
            size: (1280, 720),
            setup_ms: 100.0,
            presets,
            tuned: None,
            stress: Vec::new(),
            recommendation: Recommendation {
                tier: Tier::Performance,
                settings: GraphicsSettings::default(),
                title: String::new(),
                reasons: Vec::new(),
            },
        }
    }

    #[test]
    fn the_best_preset_within_budget_is_recommended() {
        let presets = vec![
            measured(Tier::Performance, 3.0, &[]),
            measured(Tier::Balanced, 5.0, &[("Shadows", 1.0)]),
            measured(Tier::HighQuality, 9.0, &[("Shadows", 2.0)]),
            measured(Tier::MaximumQuality, 16.0, &[("Shadows", 8.0)]),
        ];
        let advice = recommend(&report(presets));
        assert_eq!(advice.tier, Tier::HighQuality);
        assert_eq!(advice.settings, GraphicsSettings::preset(Quality::High));
        assert!(!advice.settings.automatic);
        assert!(advice.reasons[0].contains("High Quality drew the test town in 9.0 ms"));
        assert!(
            advice.reasons[1].contains("Maximum Quality took 16 ms")
                && advice.reasons[1].contains("mostly shadows (+6.0 ms)"),
            "{:?}",
            advice.reasons
        );
    }

    #[test]
    fn every_preset_fitting_recommends_maximum_quality() {
        let presets = Tier::ALL
            .into_iter()
            .map(|tier| measured(tier, 2.0 + tier as usize as f32, &[]))
            .collect();
        let advice = recommend(&report(presets));
        assert_eq!(advice.tier, Tier::MaximumQuality);
        assert!(advice.reasons.iter().any(|r| r.contains("every preset")));
    }

    #[test]
    fn the_setting_that_grew_most_is_the_one_tuned() {
        let presets = vec![
            measured(Tier::Performance, 4.0, &[]),
            measured(
                Tier::Balanced,
                9.0,
                &[("Shadows", 2.0), ("Ambient occlusion", 1.0), ("Scene", 4.0)],
            ),
            measured(
                Tier::HighQuality,
                13.0,
                &[("Shadows", 3.0), ("Ambient occlusion", 3.5), ("Scene", 5.0)],
            ),
        ];
        let (settings, words) = tuning(&presets).unwrap();
        assert_eq!(settings.quality, Quality::High);
        assert_eq!(settings.ambient_occlusion, Some(AmbientOcclusion::Low));
        assert_eq!(words, "Low ambient occlusion");
        // Nothing to tune when the next one up already fits, or wasn't run.
        assert_eq!(tuning(&presets[..2]), None);
    }

    #[test]
    fn a_tuned_preset_that_fits_wins() {
        let presets = vec![
            measured(Tier::Performance, 4.0, &[]),
            measured(Tier::Balanced, 9.0, &[("Shadows", 2.0)]),
            measured(Tier::HighQuality, 13.0, &[("Shadows", 5.0)]),
        ];
        let (settings, words) = tuning(&presets).unwrap();
        assert_eq!(settings.shadow_quality, Some(ShadowQuality::Medium));
        let mut tuned = measured(Tier::HighQuality, 10.0, &[("Shadows", 2.0)]);
        tuned.settings = settings.clone();
        tuned.label = words;
        let mut report = report(presets);
        report.tuned = Some(tuned);
        let advice = recommend(&report);
        assert_eq!(advice.tier, Tier::HighQuality);
        assert_eq!(advice.settings.shadow_quality, Some(ShadowQuality::Medium));
        assert_eq!(advice.title, "High Quality, with Medium shadows");
    }

    #[test]
    fn a_slow_graphics_card_gets_a_lower_render_scale() {
        let presets = vec![measured(Tier::Performance, 30.0, &[])];
        let advice = recommend(&report(presets));
        assert_eq!(advice.tier, Tier::Performance);
        // 0.75 × √(11.1 / 30) ≈ 0.456, held at 50%.
        assert_eq!(advice.settings.render_scale, Some(0.5));
        assert!(advice.title.contains("50% render scale"));
    }

    #[test]
    fn a_slow_cpu_says_settings_wont_help() {
        let mut low = measured(Tier::Performance, 20.0, &[]);
        low.cpu = timing(20.0);
        low.gpu = Some(timing(3.0));
        let advice = recommend(&report(vec![low]));
        assert_eq!(advice.settings.render_scale, None);
        assert!(
            advice
                .reasons
                .iter()
                .any(|r| r.contains("The CPU set the pace"))
        );
    }

    #[test]
    fn a_software_adapter_gets_performance() {
        let mut report = report(vec![measured(Tier::MaximumQuality, 2.0, &[])]);
        report.adapter.software = true;
        assert_eq!(recommend(&report).tier, Tier::Performance);
    }

    #[test]
    fn the_texture_budget_follows_the_cards_memory() {
        assert_eq!(texture_budget_for(16 << 30), 4096);
        assert_eq!(texture_budget_for(12 << 30), 2048);
        assert_eq!(texture_budget_for(2 << 30), 512);
        assert_eq!(texture_budget_for(512 << 20), 256);
        assert_eq!(texture_budget_for(256 << 30), 8192);

        // A big card at Maximum Quality gets more; a small one at High less.
        let presets = Tier::ALL
            .into_iter()
            .map(|tier| measured(tier, 3.0, &[]))
            .collect();
        let mut big = report(presets);
        big.video_memory = Some(16 << 30);
        assert_eq!(recommend(&big).settings.texture_budget, Some(4096));
        let mut small = report(vec![
            measured(Tier::Performance, 3.0, &[]),
            measured(Tier::Balanced, 4.0, &[]),
            measured(Tier::HighQuality, 5.0, &[]),
            measured(Tier::MaximumQuality, 40.0, &[]),
        ]);
        small.video_memory = Some(2 << 30);
        let advice = recommend(&small);
        assert_eq!(advice.tier, Tier::HighQuality);
        assert_eq!(advice.settings.texture_budget, Some(512));
        // Unknown memory leaves the preset's.
        small.video_memory = None;
        assert_eq!(recommend(&small).settings.texture_budget, None);
    }

    #[test]
    fn going_on_stops_far_over_budget() {
        assert!(worth_going_on(&measured(Tier::Performance, 30.0, &[])));
        assert!(!worth_going_on(&measured(Tier::Performance, 40.0, &[])));
    }
}
