//! Rulers along the top and left of the artboard, in design pixels (what
//! `Position` and `Size` offsets and CSS lengths are written in), with
//! the selected element's extent marked. Dragging out of one makes a guide.

use eframe::egui;

use super::Guide;
use super::artboard::{GUIDE_COLOR, View};
use crate::theme;

/// How thick a ruler is, in points.
pub const RULER: f32 = 18.0;
/// The closest labelled ticks get, in points.
const LABEL_SPACING: f32 = 56.0;

/// The design pixels between labelled ticks, so labels are at least
/// [`LABEL_SPACING`] points apart at `per_unit` points per design pixel.
pub fn tick_step(per_unit: f32) -> f32 {
    const STEPS: [f32; 14] = [
        1.0, 2.0, 5.0, 10.0, 20.0, 25.0, 50.0, 100.0, 200.0, 250.0, 500.0, 1000.0, 2000.0, 5000.0,
    ];
    STEPS
        .into_iter()
        .find(|step| step * per_unit >= LABEL_SPACING)
        .unwrap_or(10_000.0)
}

/// Draws the rulers in the top and left strips of `rect`, and returns their
/// responses (top, left) for dragging guides out of them. `selected` is the
/// selected element, in points.
pub fn draw(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    view: &View,
    selected: Option<egui::Rect>,
    guides: &[Guide],
) -> (egui::Response, egui::Response) {
    let palette = theme::palette(ui.ctx());
    let top = egui::Rect::from_min_max(
        egui::pos2(view.canvas.left(), rect.top()),
        egui::pos2(rect.right(), view.canvas.top()),
    );
    let left = egui::Rect::from_min_max(
        egui::pos2(rect.left(), view.canvas.top()),
        egui::pos2(view.canvas.left(), rect.bottom()),
    );
    let corner = egui::Rect::from_min_max(rect.min, view.canvas.min);
    let painter = ui.painter_at(rect);
    for strip in [top, left, corner] {
        painter.rect_filled(strip, 0.0, palette.panel);
    }
    painter.line_segment(
        [top.left_bottom(), top.right_bottom()],
        egui::Stroke::new(1.0_f32, palette.border),
    );
    painter.line_segment(
        [left.right_top(), left.right_bottom()],
        egui::Stroke::new(1.0_f32, palette.border),
    );

    // The selected element's extent, as a band on each ruler.
    if let Some(selected) = selected {
        let band = palette.accent.gamma_multiply(0.3);
        let x = egui::Rect::from_x_y_ranges(selected.x_range(), top.y_range()).intersect(top);
        let y = egui::Rect::from_x_y_ranges(left.x_range(), selected.y_range()).intersect(left);
        for strip in [x, y] {
            if strip.is_positive() {
                painter.rect_filled(strip, 0.0, band);
            }
        }
    }

    let step = tick_step(view.per_unit());
    // Minor ticks between the labelled ones: fifths, or halves of 1, 2 and 25.
    let per_label: i64 = if step >= 5.0 && (step / 5.0).fract() == 0.0 {
        5
    } else {
        2
    };
    let minor = step / per_label as f32;
    let stroke = egui::Stroke::new(1.0_f32, palette.text_muted);
    let font = egui::FontId::monospace(9.0);
    // Along the top: x. Along the left: y.
    for horizontal in [true, false] {
        let strip = if horizontal { top } else { left };
        let (from, to) = if horizontal {
            (
                view.design(strip.left_top()).x,
                view.design(strip.right_top()).x,
            )
        } else {
            (
                view.design(strip.left_top()).y,
                view.design(strip.left_bottom()).y,
            )
        };
        // Counted in whole minor ticks, so adding never drifts off a label.
        for n in (from / minor).floor() as i64..=(to / minor).ceil() as i64 {
            let tick = n as f32 * minor;
            let major = n.rem_euclid(per_label) == 0;
            let length = if major { RULER * 0.6 } else { RULER * 0.25 };
            if horizontal {
                let x = view.point(egui::vec2(tick, 0.0)).x;
                painter.line_segment(
                    [
                        egui::pos2(x, strip.bottom()),
                        egui::pos2(x, strip.bottom() - length),
                    ],
                    stroke,
                );
                if major {
                    painter.text(
                        egui::pos2(x + 2.0, strip.top() + 1.0),
                        egui::Align2::LEFT_TOP,
                        format!("{tick:.0}"),
                        font.clone(),
                        palette.text_weak,
                    );
                }
            } else {
                let y = view.point(egui::vec2(0.0, tick)).y;
                painter.line_segment(
                    [
                        egui::pos2(strip.right(), y),
                        egui::pos2(strip.right() - length, y),
                    ],
                    stroke,
                );
                if major {
                    // Written down the ruler, so a long number fits.
                    let label = format!("{tick:.0}");
                    for (index, digit) in label.chars().enumerate() {
                        painter.text(
                            egui::pos2(strip.left() + 2.0, y + 2.0 + index as f32 * 8.0),
                            egui::Align2::LEFT_TOP,
                            digit,
                            font.clone(),
                            palette.text_weak,
                        );
                    }
                }
            }
        }
    }

    // Each guide, as a small mark on its ruler.
    for &guide in guides {
        let at = view.guide_at(guide);
        let (a, b) = if guide.vertical {
            (egui::pos2(at, top.top()), egui::pos2(at, top.bottom()))
        } else {
            (egui::pos2(left.left(), at), egui::pos2(left.right(), at))
        };
        painter.line_segment([a, b], egui::Stroke::new(2.0_f32, GUIDE_COLOR));
    }

    let top = ui
        .interact(
            top,
            ui.id().with("ui designer top ruler"),
            egui::Sense::drag(),
        )
        .on_hover_text("Drag down onto the artboard for a guide that elements snap to");
    let left = ui
        .interact(
            left,
            ui.id().with("ui designer left ruler"),
            egui::Sense::drag(),
        )
        .on_hover_text("Drag right onto the artboard for a guide that elements snap to");
    if top.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
    }
    if left.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
    }
    (top, left)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ruler_labels_stay_apart_at_any_zoom() {
        assert_eq!(tick_step(1.0), 100.0);
        assert_eq!(tick_step(0.25), 250.0);
        assert_eq!(tick_step(0.1), 1000.0);
        assert_eq!(tick_step(8.0), 10.0);
        for zoom in [0.03, 0.2, 0.5, 1.0, 3.0, 8.0] {
            assert!(tick_step(zoom) * zoom >= LABEL_SPACING, "{zoom}");
        }
    }
}
