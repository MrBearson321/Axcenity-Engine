//! The controls for `NumberSequence` and `ColorSequence` properties
//! (roadmap R17.1), such as a ParticleEmitter's `SizeOverLife`: a picture
//! of the curve or gradient, and its keypoints as rows to edit, add and
//! remove. Unset (empty), a sequence shows a Set button; Clear unsets it.

use axcenity_core::{
    Color3, ColorKeypoint, ColorSequence, MAX_KEYPOINTS, NumberKeypoint, NumberSequence,
};
use eframe::egui;

use super::MAX_DECIMALS;
use super::values::color_control;
use crate::theme::icons;

/// How big the curve's and gradient's pictures are, in points.
const PICTURE: egui::Vec2 = egui::vec2(180.0, 44.0);

/// What a sequence property starts as when Set: what its start and end
/// values would blend to by default.
pub(super) fn starting_number_sequence(name: &str) -> NumberSequence {
    if name.starts_with("Transparency") {
        NumberSequence::ramp(0.0, 1.0)
    } else {
        NumberSequence::constant(1.0)
    }
}

/// The middle of the widest gap between two keypoints' times: where an
/// added keypoint goes.
fn widest_gap(times: &[f32]) -> f32 {
    times
        .windows(2)
        .max_by(|a, b| (a[1] - a[0]).total_cmp(&(b[1] - b[0])))
        .map_or(0.5, |pair| (pair[0] + pair[1]) / 2.0)
}

/// A keypoint's time between its neighbours', the first at 0 and the last
/// at 1.
fn time_row(ui: &mut egui::Ui, times: &[f32], index: usize, time: &mut f32) -> bool {
    let last = times.len() - 1;
    if index == 0 || index == last {
        ui.weak(if index == 0 { "0" } else { "1" });
        return false;
    }
    let range = times[index - 1]..=times[index + 1];
    ui.add(
        egui::DragValue::new(time)
            .speed(0.005)
            .range(range)
            .max_decimals(MAX_DECIMALS),
    )
    .on_hover_text("When, from birth (0) to death (1)")
    .changed()
}

/// A `NumberSequence` property's control. Returns the new sequence if it
/// changed this frame.
pub(super) fn number_sequence_control(
    ui: &mut egui::Ui,
    name: &str,
    sequence: &NumberSequence,
) -> Option<NumberSequence> {
    if sequence.is_empty() {
        return unset_row(ui).then(|| starting_number_sequence(name));
    }
    let mut points: Vec<NumberKeypoint> = sequence.keypoints().to_vec();
    let mut changed = false;
    let mut cleared = false;
    ui.vertical(|ui| {
        paint_curve(ui, sequence);
        let times: Vec<f32> = points.iter().map(|point| point.time).collect();
        let mut remove = None;
        for (index, point) in points.iter_mut().enumerate() {
            ui.push_id(index, |ui| {
                ui.horizontal(|ui| {
                    changed |= time_row(ui, &times, index, &mut point.time);
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut point.value)
                                .speed(0.05)
                                .max_decimals(MAX_DECIMALS),
                        )
                        .on_hover_text("The value")
                        .changed();
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut point.envelope)
                                .speed(0.02)
                                .range(0.0..=f32::INFINITY)
                                .prefix("± ")
                                .max_decimals(MAX_DECIMALS),
                        )
                        .on_hover_text(
                            "Give or take: each particle picks once where in this range it is",
                        )
                        .changed();
                    if index != 0
                        && index != times.len() - 1
                        && ui
                            .small_button(icons::MINUS)
                            .on_hover_text("Remove this keypoint")
                            .clicked()
                    {
                        remove = Some(index);
                    }
                });
            });
        }
        if let Some(index) = remove {
            points.remove(index);
            changed = true;
        }
        ui.horizontal(|ui| {
            let room = points.len() < MAX_KEYPOINTS;
            if ui
                .add_enabled(
                    room,
                    egui::Button::new(format!("{} Keypoint", icons::PLUS)).small(),
                )
                .on_hover_text("Add a keypoint in the widest gap")
                .clicked()
            {
                let time = widest_gap(&times);
                let (value, envelope) = sequence.sample(time).unwrap_or((0.0, 0.0));
                let at = points
                    .iter()
                    .position(|point| point.time > time)
                    .unwrap_or(points.len() - 1);
                points.insert(at, NumberKeypoint::new(time, value, envelope));
                changed = true;
            }
            cleared = ui
                .small_button("Clear")
                .on_hover_text("Unset: the start and end values are used again")
                .clicked();
        });
    });
    if cleared {
        return Some(NumberSequence::default());
    }
    changed.then(|| NumberSequence::new(points).ok()).flatten()
}

/// A `ColorSequence` property's control. Returns the new sequence if it
/// changed this frame.
pub(super) fn color_sequence_control(
    ui: &mut egui::Ui,
    sequence: &ColorSequence,
) -> Option<ColorSequence> {
    if sequence.is_empty() {
        let white = Color3::new(1.0, 1.0, 1.0);
        return unset_row(ui).then(|| ColorSequence::ramp(white, white));
    }
    let mut points: Vec<ColorKeypoint> = sequence.keypoints().to_vec();
    let mut changed = false;
    let mut cleared = false;
    ui.vertical(|ui| {
        paint_gradient(ui, sequence);
        let times: Vec<f32> = points.iter().map(|point| point.time).collect();
        let mut remove = None;
        for (index, point) in points.iter_mut().enumerate() {
            ui.push_id(index, |ui| {
                ui.horizontal(|ui| {
                    changed |= time_row(ui, &times, index, &mut point.time);
                    if let Some(color) = color_control(ui, point.color) {
                        point.color = color;
                        changed = true;
                    }
                    if index != 0
                        && index != times.len() - 1
                        && ui
                            .small_button(icons::MINUS)
                            .on_hover_text("Remove this keypoint")
                            .clicked()
                    {
                        remove = Some(index);
                    }
                });
            });
        }
        if let Some(index) = remove {
            points.remove(index);
            changed = true;
        }
        ui.horizontal(|ui| {
            let room = points.len() < MAX_KEYPOINTS;
            if ui
                .add_enabled(
                    room,
                    egui::Button::new(format!("{} Keypoint", icons::PLUS)).small(),
                )
                .on_hover_text("Add a keypoint in the widest gap")
                .clicked()
            {
                let time = widest_gap(&times);
                let color = sequence.sample(time).unwrap_or(Color3::new(1.0, 1.0, 1.0));
                let at = points
                    .iter()
                    .position(|point| point.time > time)
                    .unwrap_or(points.len() - 1);
                points.insert(at, ColorKeypoint::new(time, color));
                changed = true;
            }
            cleared = ui
                .small_button("Clear")
                .on_hover_text("Unset: the start and end colors are used again")
                .clicked();
        });
    });
    if cleared {
        return Some(ColorSequence::default());
    }
    changed.then(|| ColorSequence::new(points).ok()).flatten()
}

/// "Not set" and a Set button; true when Set was clicked.
fn unset_row(ui: &mut egui::Ui) -> bool {
    ui.horizontal(|ui| {
        ui.weak("Not set (start to end)");
        ui.small_button("Set")
            .on_hover_text("Shape it over the particle's life with keypoints")
            .clicked()
    })
    .inner
}

/// The curve, its envelope as a band, from time 0 at the left to 1 at the
/// right, scaled to fit.
fn paint_curve(ui: &mut egui::Ui, sequence: &NumberSequence) {
    let (rect, _) = ui.allocate_exact_size(PICTURE, egui::Sense::hover());
    let painter = ui.painter_at(rect);
    let visuals = ui.visuals();
    painter.rect_filled(rect, 3.0, visuals.extreme_bg_color);
    let points = sequence.keypoints();
    let low = points
        .iter()
        .map(|p| p.value - p.envelope)
        .fold(0.0_f32, f32::min);
    let high = points
        .iter()
        .map(|p| p.value + p.envelope)
        .fold(low + 1e-3, f32::max);
    let at = |time: f32, value: f32| {
        egui::pos2(
            rect.left() + time * rect.width(),
            rect.bottom() - (value - low) / (high - low) * rect.height(),
        )
    };
    let accent = visuals.selection.bg_fill;
    for pair in points.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let band = vec![
            at(a.time, a.value + a.envelope),
            at(b.time, b.value + b.envelope),
            at(b.time, b.value - b.envelope),
            at(a.time, a.value - a.envelope),
        ];
        painter.add(egui::Shape::convex_polygon(
            band,
            accent.gamma_multiply(0.25),
            egui::Stroke::NONE,
        ));
        painter.line_segment(
            [at(a.time, a.value), at(b.time, b.value)],
            egui::Stroke::new(1.5, accent),
        );
    }
    for point in points {
        painter.circle_filled(at(point.time, point.value), 2.5, accent);
    }
}

/// The gradient, from time 0 at the left to 1 at the right.
fn paint_gradient(ui: &mut egui::Ui, sequence: &ColorSequence) {
    let size = egui::vec2(PICTURE.x, 16.0);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter_at(rect);
    const STEPS: usize = 48;
    for step in 0..STEPS {
        let time = (step as f32 + 0.5) / STEPS as f32;
        let color = sequence.sample(time).unwrap_or(Color3::new(1.0, 1.0, 1.0));
        let byte = |channel: f32| (channel.clamp(0.0, 1.0) * 255.0).round() as u8;
        let left = rect.left() + rect.width() * step as f32 / STEPS as f32;
        let right = rect.left() + rect.width() * (step + 1) as f32 / STEPS as f32;
        painter.rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(left, rect.top()),
                egui::pos2(right + 0.5, rect.bottom()),
            ),
            0.0,
            egui::Color32::from_rgb(byte(color.r), byte(color.g), byte(color.b)),
        );
    }
    painter.rect_stroke(
        rect,
        2.0,
        ui.visuals().widgets.noninteractive.bg_stroke,
        egui::StrokeKind::Inside,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_keypoint_goes_in_the_widest_gap() {
        assert_eq!(widest_gap(&[0.0, 1.0]), 0.5);
        assert_eq!(widest_gap(&[0.0, 0.2, 1.0]), 0.6);
        assert_eq!(
            starting_number_sequence("TransparencyOverLife"),
            NumberSequence::ramp(0.0, 1.0)
        );
        assert_eq!(
            starting_number_sequence("SizeOverLife"),
            NumberSequence::constant(1.0)
        );
    }
}
