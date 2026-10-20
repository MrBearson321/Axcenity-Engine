//! The clip timeline: the whole take as a bar, the clip's part of it
//! (with handles to drag its start and end), the playhead (click or drag
//! to scrub) and the clip's events (drag to move, right-click to remove).
//! Times are seconds of the take, kept on whole frames.

use eframe::egui;

/// What changed on the timeline this frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TimelineChange {
    /// The playhead was dragged here.
    pub playhead: Option<f64>,
    pub start: Option<f64>,
    pub end: Option<f64>,
    /// An event (by index) was dragged to this time.
    pub moved_event: Option<(usize, f64)>,
    pub removed_event: Option<usize>,
}

/// What the timeline shows.
pub struct Timeline<'a> {
    /// The whole take, in seconds.
    pub take_seconds: f64,
    pub frame_rate: f32,
    pub start: f64,
    pub end: f64,
    pub playhead: f64,
    /// Each event's time in the take and name.
    pub events: &'a [(f64, &'a str)],
}

const HEIGHT: f32 = 58.0;

impl Timeline<'_> {
    fn snap(&self, seconds: f64) -> f64 {
        let fps = f64::from(self.frame_rate.max(1.0));
        ((seconds * fps).round() / fps).clamp(0.0, self.take_seconds)
    }

    pub fn show(&self, ui: &mut egui::Ui) -> TimelineChange {
        let mut change = TimelineChange::default();
        let width = ui.available_width().max(120.0);
        let (rect, bar) =
            ui.allocate_exact_size(egui::vec2(width, HEIGHT), egui::Sense::click_and_drag());
        let visuals = ui.visuals().clone();
        let painter = ui.painter_at(rect.expand(2.0));
        let track = egui::Rect::from_min_max(
            rect.left_top() + egui::vec2(6.0, 18.0),
            rect.right_top() + egui::vec2(-6.0, 42.0),
        );
        let length = self.take_seconds.max(1e-6);
        let x_of = |seconds: f64| track.left() + (seconds / length) as f32 * track.width();
        let seconds_at = |x: f32| f64::from((x - track.left()) / track.width()) * length;

        // Frame ticks, every 1, 5, 10… frames, whatever fits.
        let frames = (self.take_seconds * f64::from(self.frame_rate))
            .round()
            .max(1.0) as u32;
        let step = [1, 5, 10, 15, 30, 60, 120, 300, 600]
            .into_iter()
            .find(|&step| track.width() / (frames as f32 / step as f32) >= 36.0)
            .unwrap_or(1200);
        painter.rect_filled(track, 3.0, visuals.extreme_bg_color);
        let mut frame = 0;
        while frame <= frames {
            let x = x_of(f64::from(frame) / f64::from(self.frame_rate.max(1.0)));
            painter.line_segment(
                [egui::pos2(x, track.top() - 4.0), egui::pos2(x, track.top())],
                egui::Stroke::new(1.0_f32, visuals.weak_text_color()),
            );
            painter.text(
                egui::pos2(x, track.top() - 5.0),
                egui::Align2::CENTER_BOTTOM,
                frame.to_string(),
                egui::FontId::proportional(10.0),
                visuals.weak_text_color(),
            );
            frame += step;
        }

        // The clip's part of the take.
        let accent = visuals.selection.bg_fill;
        let clip = egui::Rect::from_x_y_ranges(x_of(self.start)..=x_of(self.end), track.y_range());
        painter.rect_filled(clip, 3.0, accent.gamma_multiply(0.45));

        // Scrubbing anywhere on the bar.
        if (bar.dragged() || bar.clicked())
            && let Some(pointer) = bar.interact_pointer_pos()
        {
            let at = self.snap(seconds_at(pointer.x)).clamp(self.start, self.end);
            change.playhead = Some(at);
        }

        // The start and end handles.
        for (index, seconds) in [(0, self.start), (1, self.end)] {
            let handle = egui::Rect::from_center_size(
                egui::pos2(x_of(seconds), track.center().y),
                egui::vec2(10.0, track.height() + 6.0),
            );
            let response = ui
                .interact(handle, bar.id.with(("range", index)), egui::Sense::drag())
                .on_hover_cursor(egui::CursorIcon::ResizeHorizontal)
                .on_hover_text(if index == 0 {
                    "Drag the clip's start"
                } else {
                    "Drag the clip's end"
                });
            let held = response.hovered() || response.dragged();
            painter.rect_filled(
                handle.shrink2(egui::vec2(2.0, 0.0)),
                2.0,
                if held {
                    visuals.strong_text_color()
                } else {
                    accent
                },
            );
            if response.dragged()
                && let Some(pointer) = response.interact_pointer_pos()
            {
                let at = self.snap(seconds_at(pointer.x));
                let one_frame = 1.0 / f64::from(self.frame_rate.max(1.0));
                if index == 0 {
                    change.start = Some(at.min(self.end - one_frame).max(0.0));
                } else {
                    change.end = Some(at.max(self.start + one_frame).min(self.take_seconds));
                }
            }
        }

        // The playhead.
        let x = x_of(self.playhead);
        painter.line_segment(
            [
                egui::pos2(x, track.top() - 2.0),
                egui::pos2(x, track.bottom() + 2.0),
            ],
            egui::Stroke::new(2.0_f32, egui::Color32::from_rgb(255, 120, 80)),
        );

        // Events, as diamonds under the bar.
        for (index, (seconds, name)) in self.events.iter().enumerate() {
            let center = egui::pos2(x_of(*seconds), track.bottom() + 9.0);
            let marker = egui::Rect::from_center_size(center, egui::vec2(12.0, 12.0));
            let response = ui
                .interact(
                    marker,
                    bar.id.with(("event", index)),
                    egui::Sense::click_and_drag(),
                )
                .on_hover_text(format!("{name}: drag to move, right-click to remove"));
            let color = if response.hovered() || response.dragged() {
                visuals.strong_text_color()
            } else {
                egui::Color32::from_rgb(255, 200, 60)
            };
            let points = vec![
                center + egui::vec2(0.0, -5.0),
                center + egui::vec2(5.0, 0.0),
                center + egui::vec2(0.0, 5.0),
                center + egui::vec2(-5.0, 0.0),
            ];
            painter.add(egui::Shape::convex_polygon(
                points,
                color,
                egui::Stroke::NONE,
            ));
            if response.dragged()
                && let Some(pointer) = response.interact_pointer_pos()
            {
                let at = self.snap(seconds_at(pointer.x)).clamp(self.start, self.end);
                change.moved_event = Some((index, at));
            }
            if response.secondary_clicked() {
                change.removed_event = Some(index);
            }
        }
        change
    }
}
