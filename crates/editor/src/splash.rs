//! The editor's launch splash, laid out like Unity's loading window: a black
//! screen, a small white wordmark, a status line naming each startup step
//! with a percentage, a thin loading bar, the version in the corner, and a
//! short fade.
//!
//! The bar is timed, not a measure of remaining load: the steps name what
//! starting the editor did, shown one after another along the bar. The clock
//! only counts frames that were drawn (a slow frame counts as at most
//! [`MAX_STEP`]), so a slow start can't use the splash up before it's seen.
//! Click or a key skips to the fade-out. Reduce motion shows a static
//! wordmark for a moment instead.

use std::time::Instant;

use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, vec2};

use crate::start::Brand;
use crate::theme;

/// Fade the logo in over this many seconds.
const INTRO: f32 = 0.5;
const BAR_START: f32 = 0.3;
const BAR_END: f32 = 2.8;
const DURATION: f32 = 3.3;
const REDUCE_SECONDS: f32 = 0.25;
/// The most one frame moves the clock, in seconds.
const MAX_STEP: f32 = 0.05;

/// Owns the splash clock. The clock starts on the first frame it is shown.
#[derive(Default)]
pub struct Splash {
    /// Seconds of splash shown so far.
    elapsed: f32,
    last_frame: Option<Instant>,
    skipped_at: Option<f32>,
}

impl Splash {
    /// Draw the overlay. Returns true while it still covers the window.
    /// `project` is the project opened at launch, if any, for the status line.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        brand: &Brand,
        project: Option<&str>,
        reduce_motion: bool,
    ) -> bool {
        self.tick();
        let Some(t) = self.visual_t(reduce_motion) else {
            return false;
        };
        if self.wants_skip(ctx) {
            self.skip();
        }
        paint(ctx, brand, project, t, reduce_motion);
        ctx.request_repaint();
        true
    }

    /// Moves the clock on by the time since the last frame, capped.
    fn tick(&mut self) {
        let now = Instant::now();
        if let Some(last) = self.last_frame {
            self.elapsed += (now - last).as_secs_f32().min(MAX_STEP);
        }
        self.last_frame = Some(now);
    }

    fn skip(&mut self) {
        if self.skipped_at.is_none() && self.elapsed < BAR_END {
            self.skipped_at = Some(self.elapsed);
        }
    }

    fn visual_t(&self, reduce_motion: bool) -> Option<f32> {
        let elapsed = self.elapsed;
        if reduce_motion {
            return (elapsed < REDUCE_SECONDS).then_some(BAR_END);
        }
        let t = match self.skipped_at {
            Some(t0) => BAR_END + (elapsed - t0),
            None => elapsed,
        };
        (t < DURATION).then_some(t)
    }

    fn wants_skip(&self, ctx: &egui::Context) -> bool {
        ctx.input(|input| {
            input.events.iter().any(|event| {
                matches!(
                    event,
                    egui::Event::Key {
                        pressed: true,
                        repeat: false,
                        ..
                    } | egui::Event::PointerButton { pressed: true, .. }
                )
            })
        })
    }
}

fn ease_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

fn alpha_at(t: f32) -> f32 {
    if t < INTRO {
        (t / INTRO).clamp(0.0, 1.0)
    } else if t < BAR_END {
        1.0
    } else {
        (1.0 - (t - BAR_END) / (DURATION - BAR_END)).clamp(0.0, 1.0)
    }
}

/// What starting the editor did, in order, for the status line. It names
/// the project when the editor was started with one.
fn steps(project: Option<&str>) -> Vec<String> {
    let mut steps = vec![
        "Loading fonts and icons".to_owned(),
        "Applying theme and settings".to_owned(),
        "Restoring panel layout".to_owned(),
        "Loading recent projects".to_owned(),
    ];
    match project {
        Some(name) => {
            steps.push(format!("Opening {name}"));
            steps.push("Loading scripts and assets".to_owned());
        }
        None => steps.push("Preparing start screen".to_owned()),
    }
    steps.push("Starting renderer".to_owned());
    steps
}

/// Which of `count` steps is showing at `t`, and how full the bar is (0 to 1).
/// Each step gets an equal slice of the bar's time: the bar moves quickly
/// through the start of the slice, then waits, as a real load does.
fn progress_at(t: f32, count: usize) -> (usize, f32) {
    if t <= BAR_START {
        return (0, 0.0);
    }
    let n = count as f32;
    let s = ((t - BAR_START) / (BAR_END - BAR_START)).clamp(0.0, 1.0);
    let step = ((s * n) as usize).min(count - 1);
    let local = s * n - step as f32;
    let fill = (step as f32 + ease_out_cubic(local / 0.7)) / n;
    (step, fill)
}

fn rgba(r: u8, g: u8, b: u8, a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(r, g, b, (a.clamp(0.0, 1.0) * 255.0).round() as u8)
}

fn paint(ctx: &egui::Context, brand: &Brand, project: Option<&str>, t: f32, reduce_motion: bool) {
    let a = if reduce_motion { 1.0 } else { alpha_at(t) };
    // The backdrop is solid from the first frame; only the logo fades in.
    let backdrop = if reduce_motion {
        1.0
    } else {
        alpha_at(t.max(INTRO))
    };
    if backdrop <= 0.0 {
        return;
    }
    let screen = ctx.screen_rect();
    egui::Area::new(egui::Id::new("boot splash"))
        .order(egui::Order::Foreground)
        .constrain(false)
        .anchor(Align2::LEFT_TOP, [0.0, 0.0])
        .movable(false)
        .interactable(true)
        .show(ctx, |ui| {
            // Covers the window, so nothing under it can be clicked.
            ui.allocate_rect(screen, Sense::click());
            let painter = ui.painter_at(screen);
            painter.rect_filled(screen, 0, rgba(7, 7, 8, backdrop));

            let cx = screen.center().x;
            let cy = screen.center().y - screen.height() * 0.06;
            let texture = brand.wordmark();
            let size = texture.size_vec2();
            let logo_w = (screen.width() * 0.16).clamp(150.0, 230.0);
            let logo_h = logo_w * size.y / size.x;
            let intro = if reduce_motion {
                1.0
            } else {
                ease_out_cubic((t / INTRO).min(1.0))
            };
            let rise = (1.0 - intro) * 10.0;
            let logo = Rect::from_center_size(Pos2::new(cx, cy + rise), vec2(logo_w, logo_h));
            painter.image(
                texture.id(),
                logo,
                Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                Color32::from_white_alpha((a * 255.0).round() as u8),
            );

            // Under the logo: what's loading on the left, how far along on
            // the right, and a thin bar beneath both.
            let steps = steps(project);
            let (step, fill) = if reduce_motion {
                (steps.len() - 1, 1.0)
            } else {
                progress_at(t, steps.len())
            };
            let status = if fill >= 1.0 {
                "Ready".to_owned()
            } else {
                format!("{}…", steps[step])
            };
            let block_w = (logo_w * 1.5).clamp(260.0, 340.0);
            let left = cx - block_w * 0.5;
            let top = logo.bottom() + 44.0;
            painter.text(
                Pos2::new(left, top),
                Align2::LEFT_TOP,
                status,
                FontId::new(12.0, theme::medium_family()),
                rgba(184, 184, 188, a),
            );
            painter.text(
                Pos2::new(left + block_w, top),
                Align2::RIGHT_TOP,
                format!("{}%", (fill * 100.0).round() as u32),
                FontId::monospace(11.0),
                rgba(118, 118, 124, a),
            );
            let bar_h = 3.0;
            let bar = Rect::from_min_size(Pos2::new(left, top + 22.0), vec2(block_w, bar_h));
            painter.rect_filled(bar, bar_h * 0.5, rgba(38, 38, 42, a));
            if fill > 0.0 {
                let filled = Rect::from_min_size(bar.min, vec2(block_w * fill, bar_h));
                painter.rect_filled(filled, bar_h * 0.5, rgba(236, 236, 240, a));
            }

            painter.text(
                Pos2::new(screen.left() + 24.0, screen.bottom() - 20.0),
                Align2::LEFT_BOTTOM,
                format!("Axcenity Engine {}", env!("CARGO_PKG_VERSION")),
                FontId::new(11.0, theme::medium_family()),
                rgba(104, 104, 110, a),
            );
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_status_line_names_the_project_being_opened() {
        assert!(
            steps(Some("Obby"))
                .iter()
                .any(|step| step == "Opening Obby")
        );
        assert!(!steps(None).iter().any(|step| step.starts_with("Opening")));
    }

    #[test]
    fn the_bar_only_moves_forward_through_every_step_to_full() {
        let count = steps(None).len();
        let mut last = (0, 0.0);
        let mut seen = vec![false; count];
        let mut t = 0.0;
        while t <= BAR_END {
            let now = progress_at(t, count);
            assert!(now.0 >= last.0 && now.1 >= last.1, "went back at {t}");
            seen[now.0] = true;
            last = now;
            t += 0.01;
        }
        assert!(seen.iter().all(|&shown| shown), "every step shows");
        assert_eq!(progress_at(BAR_END, count), (count - 1, 1.0));
    }
}
