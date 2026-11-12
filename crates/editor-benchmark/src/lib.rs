//! Debug › Graphics Benchmark: measures how this computer draws the
//! editor's viewport and recommends the editor's graphics settings
//! (Settings › Graphics) for it.
//!
//! It draws worlds of its own, in memory (`workloads.rs`), with a renderer
//! of its own on the editor's GPU device (`run.rs`), so the open project
//! and its game's graphics (`graphics.json`, Lighting) are never touched.
//! The recommendation (`recommend.rs`) is only advice: nothing changes
//! until Apply, which Undo takes back. The last report is saved with the
//! editor's settings and shown again when the window reopens.

mod recommend;
mod report;
mod run;
mod video_memory;
mod workloads;

#[cfg(any(test, feature = "test-support"))]
pub mod testing;
#[cfg(test)]
mod tests;

use std::time::{SystemTime, UNIX_EPOCH};

use axcenity_render::GraphicsSettings;
use eframe::egui::{self, RichText};

use axcenity_editor_kit::theme::{self, icons, space, type_scale};
use axcenity_editor_kit::widgets;

pub use recommend::FRAME_BUDGET_MS;
pub use report::{Report, Tier};
pub use run::{Plan, Progress, Run};

/// Where the last report is saved between runs, as JSON.
pub const REPORT_KEY: &str = "graphics benchmark";

/// What the window asks the app to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Start,
    Cancel,
    /// Use the recommended settings.
    Apply,
    /// Close, changing nothing.
    Keep,
    /// Put back the settings Apply replaced.
    Undo,
}

/// The benchmark's state: the window, a run in progress, and the last
/// report.
#[derive(Default)]
pub struct GraphicsBenchmark {
    pub open: bool,
    pub run: Option<Run>,
    /// The last finished run's report.
    pub report: Option<Report>,
    /// Why the last run stopped short.
    pub error: Option<String>,
    /// The settings Apply replaced, while Undo can bring them back.
    pub previous: Option<GraphicsSettings>,
    /// Scroll back to the recommendation on the next frame drawn: set on
    /// opening and when a run finishes.
    pub scroll_to_top: bool,
}

impl GraphicsBenchmark {
    /// A saved report, if `text` is one of this version.
    pub fn load(text: &str) -> Option<Report> {
        serde_json::from_str::<Report>(text)
            .ok()
            .filter(|report| report.version == report::REPORT_VERSION)
    }

    /// Opens the window at its top.
    pub fn show(&mut self) {
        self.open = true;
        self.scroll_to_top = true;
    }

    /// What Apply sets on top of `current`, once there's a report.
    pub fn applied(&self, current: &GraphicsSettings) -> Option<GraphicsSettings> {
        self.report
            .as_ref()
            .map(|report| recommend::apply_to(current, report))
    }

    pub fn saved(&self) -> Option<String> {
        self.report
            .as_ref()
            .and_then(|report| serde_json::to_string(report).ok())
    }

    /// Draws the window while it's open. `current` is the editor's settings
    /// now; `adapter` the GPU drawing the editor (to notice a report from
    /// another card); `playing` blocks starting.
    pub fn window(
        &mut self,
        ctx: &egui::Context,
        current: &GraphicsSettings,
        adapter: Option<&str>,
        playing: bool,
    ) -> Option<Action> {
        let mut open = self.open;
        let mut action = None;
        widgets::tool_window(icons::GAUGE, "Graphics Benchmark")
            .size(egui::vec2(600.0, 680.0))
            .min_width(320.0)
            .show(ctx, &mut open, |ui| {
                let footer = self.run.is_none() && self.report.is_some();
                let footer_height = match (footer, self.previous.is_some()) {
                    (false, _) => 0.0,
                    (true, false) => 52.0,
                    (true, true) => 88.0,
                };
                let mut scroll = egui::ScrollArea::vertical()
                    .auto_shrink([false, true])
                    .max_height((ui.available_height() - footer_height).max(120.0));
                if std::mem::take(&mut self.scroll_to_top) {
                    scroll = scroll.vertical_scroll_offset(0.0);
                }
                scroll.show(ui, |ui| {
                    action = self.contents(ui, current, adapter, playing);
                });
                if footer {
                    ui.separator();
                    action = self.footer(ui, current, playing).or(action);
                }
            });
        if !open {
            self.open = false;
            if self.run.is_some() {
                action = Some(Action::Cancel);
            }
        }
        action
    }

    fn contents(
        &self,
        ui: &mut egui::Ui,
        current: &GraphicsSettings,
        adapter: Option<&str>,
        playing: bool,
    ) -> Option<Action> {
        let palette = theme::palette(ui.ctx());
        let mut action = None;
        ui.label(
            RichText::new(
                "Measures how this computer draws the viewport, then suggests the editor's \
                 graphics settings. Only Settings › Graphics changes, and only if you apply \
                 it: your project and your game's graphics settings are never touched.",
            )
            .color(palette.text_secondary),
        );
        ui.add_space(space::M);

        if let Some(run) = &self.run {
            ui.label(RichText::new(run.stage()).strong());
            ui.add_space(space::XS);
            ui.add(
                egui::ProgressBar::new(run.fraction())
                    .show_percentage()
                    .animate(true),
            );
            ui.add_space(space::XS);
            ui.label(
                RichText::new(
                    "The viewport may stutter until it's done. Each preset draws a test town \
                     on its own, then heavier tests at the best one.",
                )
                .size(type_scale::SMALL)
                .color(palette.text_weak),
            );
            ui.add_space(space::S);
            if ui
                .add(widgets::button(
                    widgets::Role::Secondary,
                    Some(icons::STOP),
                    "Cancel",
                    &palette,
                ))
                .clicked()
            {
                action = Some(Action::Cancel);
            }
            return action;
        }

        if let Some(error) = &self.error {
            ui.colored_label(palette.error, error);
            ui.add_space(space::S);
        }

        let Some(report) = &self.report else {
            for tier in Tier::ALL {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(tier.label()).strong());
                    ui.label(RichText::new(tier.about()).color(palette.text_secondary));
                });
                ui.add_space(space::XXS);
            }
            ui.add_space(space::S);
            ui.label(
                RichText::new(format!(
                    "Takes up to a minute. A preset is recommended when the viewport \
                     stays within {FRAME_BUDGET_MS:.0} ms a frame (two-thirds of 60 fps, the \
                     rest for the editor and Play)."
                ))
                .size(type_scale::SMALL)
                .color(palette.text_weak),
            );
            ui.add_space(space::M);
            if start_button(ui, playing, "Run Benchmark") {
                action = Some(Action::Start);
            }
            return action;
        };

        let advice = &report.recommendation;
        ui.label(
            RichText::new("Recommended")
                .size(type_scale::SMALL)
                .color(palette.text_weak),
        );
        ui.label(
            RichText::new(&advice.title)
                .size(type_scale::HEADING)
                .strong()
                .color(palette.text_strong),
        );
        ui.label(RichText::new(advice.tier.about()).color(palette.text_secondary));
        ui.add_space(space::S);
        for reason in &advice.reasons {
            ui.horizontal_top(|ui| {
                ui.label(RichText::new("•").color(palette.accent_text));
                ui.add(egui::Label::new(reason).wrap());
            });
        }
        if let Some(adapter) = adapter
            && !adapter.starts_with(&report.adapter.name)
        {
            ui.add_space(space::S);
            ui.colored_label(
                palette.warning,
                format!(
                    "Measured on {}, not the graphics card the editor uses now: run it \
                     again for this one.",
                    report.adapter.name
                ),
            );
        }
        ui.add_space(space::M);

        let applied = recommend::apply_to(current, report);
        let changes = changes(current, &applied);
        widgets::section_label(ui, "What Apply changes");
        if changes.is_empty() {
            ui.label(
                RichText::new("Your graphics settings already match the recommendation.")
                    .color(palette.success),
            );
        } else {
            egui::Grid::new("benchmark changes")
                .num_columns(3)
                .spacing([space::L, space::XS])
                .striped(true)
                .show(ui, |ui| {
                    ui.label(RichText::new("Setting").color(palette.text_weak));
                    ui.label(RichText::new("Now").color(palette.text_weak));
                    ui.label(RichText::new("Recommended").color(palette.text_weak));
                    ui.end_row();
                    for change in &changes {
                        ui.label(change.setting);
                        ui.label(RichText::new(&change.now).color(palette.text_secondary));
                        ui.label(RichText::new(&change.recommended).strong());
                        ui.end_row();
                    }
                });
        }
        ui.add_space(space::XS);
        ui.label(
            RichText::new(
                "Kept as you have them: real-time GI and the lens effects (the test worlds \
                 don't use them), and a texture memory budget your card can hold.",
            )
            .size(type_scale::SMALL)
            .color(palette.text_weak),
        );
        ui.add_space(space::M);
        results(ui, report);
        action
    }

    /// Apply, Keep, Run Again (and Undo after Apply), under the scrolling
    /// part so they're always in reach.
    fn footer(
        &self,
        ui: &mut egui::Ui,
        current: &GraphicsSettings,
        playing: bool,
    ) -> Option<Action> {
        let palette = theme::palette(ui.ctx());
        let mut action = None;
        let changes = self
            .applied(current)
            .map(|applied| changes(current, &applied))
            .unwrap_or_default();
        ui.horizontal_wrapped(|ui| {
            let apply = widgets::button(
                widgets::Role::Primary,
                Some(icons::CHECK),
                "Apply Recommended",
                &palette,
            );
            if ui
                .add_enabled(!changes.is_empty(), apply)
                .on_hover_text("Sets Settings › Graphics to the recommendation. Undo puts it back.")
                .clicked()
            {
                action = Some(Action::Apply);
            }
            if ui
                .add(widgets::button(
                    widgets::Role::Secondary,
                    None,
                    "Keep Current Settings",
                    &palette,
                ))
                .on_hover_text("Closes the benchmark and changes nothing.")
                .clicked()
            {
                action = Some(Action::Keep);
            }
            if start_button(ui, playing, "Run Again") {
                action = Some(Action::Start);
            }
        });
        if self.previous.is_some() {
            ui.add_space(space::XS);
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Applied to Settings › Graphics.").color(palette.success));
                if ui
                    .add(widgets::button(
                        widgets::Role::Ghost,
                        Some(icons::ARROW_COUNTER_CLOCKWISE),
                        "Undo",
                        &palette,
                    ))
                    .on_hover_text("Put back the graphics settings from before.")
                    .clicked()
                {
                    action = Some(Action::Undo);
                }
            });
        }
        action
    }
}

/// The Start or Run Again button, held while Play runs.
fn start_button(ui: &mut egui::Ui, playing: bool, label: &str) -> bool {
    let palette = theme::palette(ui.ctx());
    let role = if label == "Run Again" {
        widgets::Role::Secondary
    } else {
        widgets::Role::Primary
    };
    ui.add_enabled(
        !playing,
        widgets::button(role, Some(icons::PLAY), label, &palette),
    )
    .on_disabled_hover_text("Stop Play first: the benchmark needs the graphics card to itself.")
    .clicked()
}

/// The measurements, in tables.
fn results(ui: &mut egui::Ui, report: &Report) {
    let palette = theme::palette(ui.ctx());
    let ms = |value: f32| format!("{value:.1} ms");
    let gpu = |measurement: &report::Measurement| {
        measurement
            .gpu
            .map_or("–".to_owned(), |timing| ms(timing.p95))
    };
    widgets::section(icons::CHART_BAR, "Presets")
        .id_salt("benchmark presets")
        .default_open(true)
        .show(ui, |ui| {
            ui.label(
                RichText::new(format!(
                    "The test town at {} × {}, 95% of frames at or under each time. Budget: \
                     {FRAME_BUDGET_MS:.1} ms.",
                    report.size.0, report.size.1
                ))
                .size(type_scale::SMALL)
                .color(palette.text_weak),
            );
            egui::Grid::new("benchmark presets grid")
                .num_columns(6)
                .spacing([space::M, space::XS])
                .striped(true)
                .show(ui, |ui| {
                    for heading in ["Preset", "Frame", "FPS", "GPU", "CPU", "Memory"] {
                        ui.label(RichText::new(heading).color(palette.text_weak));
                    }
                    ui.end_row();
                    for tier in Tier::ALL {
                        let Some(measurement) = report.preset(tier) else {
                            ui.label(tier.label());
                            ui.label(RichText::new("skipped").color(palette.text_weak))
                                .on_hover_text("A lower preset was already far over budget.");
                            ui.end_row();
                            continue;
                        };
                        ui.label(tier.label());
                        let color = if recommend::fits(measurement) {
                            palette.success
                        } else {
                            palette.warning
                        };
                        ui.label(RichText::new(ms(measurement.frame.p95)).color(color))
                            .on_hover_text(format!(
                                "Median {}, worst {}; {} of {} frames over twice the median. \
                                 First frame (shaders and targets): {}.",
                                ms(measurement.frame.median),
                                ms(measurement.frame.worst),
                                measurement.hitches,
                                measurement.frames,
                                ms(measurement.first_frame_ms),
                            ));
                        ui.label(format!("{:.0}", measurement.fps()));
                        ui.label(gpu(measurement));
                        ui.label(ms(measurement.cpu.p95));
                        ui.label(bytes(measurement.gpu_bytes));
                        ui.end_row();
                    }
                    if let Some(tuned) = &report.tuned {
                        ui.label(format!(
                            "{}, {}",
                            Tier::of(tuned.settings.quality).label(),
                            tuned.label
                        ));
                        ui.label(ms(tuned.frame.p95));
                        ui.label(format!("{:.0}", tuned.fps()));
                        ui.label(gpu(tuned));
                        ui.label(ms(tuned.cpu.p95));
                        ui.label(bytes(tuned.gpu_bytes));
                        ui.end_row();
                    }
                });
        });
    widgets::section(icons::STACK, "Heavier tests")
        .id_salt("benchmark stress")
        .default_open(false)
        .show(ui, |ui| {
            ui.label(
                RichText::new("At the recommended settings.")
                    .size(type_scale::SMALL)
                    .color(palette.text_weak),
            );
            egui::Grid::new("benchmark stress grid")
                .num_columns(5)
                .spacing([space::M, space::XS])
                .striped(true)
                .show(ui, |ui| {
                    for heading in ["Test", "Frame", "GPU", "CPU", "Draw list"] {
                        ui.label(RichText::new(heading).color(palette.text_weak));
                    }
                    ui.end_row();
                    for stress in &report.stress {
                        ui.label(stress.workload.label())
                            .on_hover_text(stress.workload.about());
                        let color = if recommend::fits(stress) {
                            palette.success
                        } else {
                            palette.warning
                        };
                        ui.label(RichText::new(ms(stress.frame.p95)).color(color));
                        ui.label(gpu(stress));
                        ui.label(ms(stress.cpu.p95));
                        ui.label(ms(stress.draw_list.p95));
                        ui.end_row();
                    }
                });
        });
    let chosen = report
        .tuned
        .as_ref()
        .filter(|tuned| tuned.settings == report.recommendation.settings)
        .or_else(|| report.preset(report.recommendation.tier));
    if let Some(chosen) = chosen.filter(|chosen| !chosen.passes.is_empty()) {
        let title = format!(
            "Where the GPU's time goes: {}",
            if chosen.workload == workloads::Workload::Town
                && Tier::of(chosen.settings.quality).label() == chosen.label
            {
                chosen.label.clone()
            } else {
                format!(
                    "{}, {}",
                    Tier::of(chosen.settings.quality).label(),
                    chosen.label
                )
            }
        );
        widgets::section(icons::GAUGE, &title)
            .id_salt("benchmark passes")
            .default_open(false)
            .show(ui, |ui| {
                let total: f32 = chosen.passes.iter().map(|pass| pass.ms).sum();
                egui::Grid::new("benchmark passes grid")
                    .num_columns(3)
                    .spacing([space::M, space::XS])
                    .show(ui, |ui| {
                        for pass in &chosen.passes {
                            ui.label(&pass.pass);
                            ui.label(ms(pass.ms));
                            ui.add(
                                egui::ProgressBar::new(pass.ms / total.max(0.001))
                                    .desired_width(160.0),
                            );
                            ui.end_row();
                        }
                    });
            });
    }
    widgets::section(icons::CPU, "This computer")
        .id_salt("benchmark machine")
        .default_open(false)
        .show(ui, |ui| {
            let adapter = &report.adapter;
            egui::Grid::new("benchmark machine grid")
                .num_columns(2)
                .spacing([space::M, space::XS])
                .show(ui, |ui| {
                    let mut row = |name: &str, value: String| {
                        ui.label(RichText::new(name).color(palette.text_weak));
                        ui.label(value);
                        ui.end_row();
                    };
                    row("Graphics card", adapter.name.clone());
                    row(
                        "API and kind",
                        format!("{} · {}", adapter.backend, adapter.kind),
                    );
                    row("Driver", adapter.driver.clone());
                    row(
                        "Card memory",
                        report.video_memory.map_or("Not reported".to_owned(), bytes),
                    );
                    row(
                        "GPU timing",
                        if adapter.timestamps {
                            "Timestamps".to_owned()
                        } else {
                            "Not available: frames are timed as a whole".to_owned()
                        },
                    );
                    row("CPU threads", report.cpu_threads.to_string());
                    row(
                        "Renderer setup",
                        format!("{} (pipelines and shaders)", ms(report.setup_ms)),
                    );
                    row("Measured", ago(report.finished_at));
                });
        });
}

/// One setting Apply would change.
#[derive(Clone, Debug, PartialEq)]
pub struct Change {
    pub setting: &'static str,
    pub now: String,
    pub recommended: String,
}

/// A setting's name, and its value as it would draw.
type Row = (&'static str, fn(&GraphicsSettings) -> String);

/// The settings that differ between `now` and `recommended`, as they'd be
/// drawn (each the override or its preset's value).
pub fn changes(now: &GraphicsSettings, recommended: &GraphicsSettings) -> Vec<Change> {
    let rows: [Row; 22] = [
        ("Preset", |s| {
            let preset = Tier::of(s.quality).label();
            if s.automatic {
                format!("{preset} (Automatic)")
            } else {
                preset.to_owned()
            }
        }),
        ("Shadow quality", |s| s.shadow_quality().label().to_owned()),
        ("Shadow distance", |s| {
            format!("{:.0} studs", s.shadow_distance())
        }),
        ("Anti-aliasing", |s| s.anti_aliasing().label().to_owned()),
        ("Ambient occlusion", |s| {
            s.ambient_occlusion().label().to_owned()
        }),
        ("Reflections", |s| s.reflection_quality().label().to_owned()),
        ("Render scale", |s| {
            format!("{:.0}%", s.render_scale() * 100.0)
        }),
        ("Texture quality", |s| {
            s.texture_quality().label().to_owned()
        }),
        ("Texture memory", |s| format!("{} MB", s.texture_budget())),
        ("Post-processing", |s| on_off(s.post_processing())),
        ("Contact shadows", |s| on_off(s.contact_shadows())),
        ("Volumetric fog", |s| on_off(s.volumetric_fog())),
        ("Real-time GI", |s| on_off(s.realtime_gi())),
        ("Clouds", |s| s.cloud_quality().label().to_owned()),
        ("Depth of field", |s| s.depth_of_field().label().to_owned()),
        ("Motion blur", |s| s.motion_blur().label().to_owned()),
        ("Vignette", |s| on_off(s.vignette())),
        ("Film grain", |s| on_off(s.film_grain())),
        ("Chromatic aberration", |s| on_off(s.chromatic_aberration())),
        ("Sharpen", |s| on_off(s.sharpen())),
        ("Ray tracing distance", |s| {
            format!("{} studs", s.ray_tracing_distance())
        }),
        ("Ray traced shadows", |s| {
            s.ray_traced_shadows().label().to_owned()
        }),
    ];
    rows.into_iter()
        .filter_map(|(setting, value)| {
            let (now, recommended) = (value(now), value(recommended));
            (now != recommended).then_some(Change {
                setting,
                now,
                recommended,
            })
        })
        .collect()
}

fn on_off(on: bool) -> String {
    if on { "On" } else { "Off" }.to_owned()
}

fn bytes(bytes: u64) -> String {
    let mib = bytes as f64 / (1024.0 * 1024.0);
    if mib >= 1024.0 {
        format!("{:.1} GB", mib / 1024.0)
    } else {
        format!("{mib:.0} MB")
    }
}

/// "3 minutes ago" for a time in seconds since 1970.
fn ago(then: u64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    let seconds = now.saturating_sub(then);
    let (count, unit) = match seconds {
        0..60 => return "Just now".to_owned(),
        60..3_600 => (seconds / 60, "minute"),
        3_600..86_400 => (seconds / 3_600, "hour"),
        _ => (seconds / 86_400, "day"),
    };
    format!("{count} {unit}{} ago", if count == 1 { "" } else { "s" })
}
