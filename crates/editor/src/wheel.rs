//! The mouse wheel without egui's smoothing.
//!
//! egui spreads a wheel notch over a few frames (`smooth_scroll_delta`),
//! which suits scrolling a list but not stepping a value once per notch.
//! egui 0.36 dropped its unsmoothed `raw_scroll_delta`, so it is worked out
//! here from this frame's wheel events, the way egui did.

use eframe::egui::{self, Event, InputState, MouseWheelUnit, TouchPhase, Vec2};

/// How far the wheel turned this frame, in points: a line is egui's
/// default line height for scrolling, a page the window's height, and with
/// Shift held all of it is sideways (as egui scrolls).
pub(crate) fn raw(input: &InputState) -> Vec2 {
    let line = egui::InputOptions::default().line_scroll_speed;
    input
        .events
        .iter()
        .filter_map(|event| match event {
            Event::MouseWheel {
                unit,
                delta,
                phase: TouchPhase::Move,
                modifiers,
            } => {
                let delta = match unit {
                    MouseWheelUnit::Point => *delta,
                    MouseWheelUnit::Line => line * *delta,
                    MouseWheelUnit::Page => input.content_rect().height() * *delta,
                };
                Some(if modifiers.shift {
                    Vec2::new(delta.x + delta.y, 0.0)
                } else {
                    delta
                })
            }
            _ => None,
        })
        .fold(Vec2::ZERO, |sum, delta| sum + delta)
}

/// Takes this frame's wheel, smoothed or not, so nothing drawn after
/// (a scroll area, the camera) also acts on it.
pub(crate) fn take(input: &mut InputState) {
    input
        .events
        .retain(|event| !matches!(event, Event::MouseWheel { .. }));
    input.smooth_scroll_delta = Vec2::ZERO;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wheel(unit: MouseWheelUnit, delta: Vec2, shift: bool) -> Event {
        Event::MouseWheel {
            unit,
            delta,
            phase: TouchPhase::Move,
            modifiers: egui::Modifiers {
                shift,
                ..Default::default()
            },
        }
    }

    fn input_with(events: Vec<Event>) -> InputState {
        let ctx = egui::Context::default();
        let mut state = None;
        let raw = egui::RawInput {
            events,
            ..Default::default()
        };
        let _ = crate::theme::run(&ctx, raw, |ui| {
            state = Some(ui.input(|input| input.clone()))
        });
        state.expect("one frame ran")
    }

    #[test]
    fn a_notch_counts_in_full_on_the_frame_it_turns() {
        let line = egui::InputOptions::default().line_scroll_speed;
        let input = input_with(vec![wheel(
            MouseWheelUnit::Line,
            Vec2::new(0.0, 1.0),
            false,
        )]);
        assert_eq!(raw(&input), Vec2::new(0.0, line));
        let input = input_with(vec![
            wheel(MouseWheelUnit::Point, Vec2::new(0.0, -3.0), false),
            wheel(MouseWheelUnit::Point, Vec2::new(0.0, -4.0), false),
        ]);
        assert_eq!(raw(&input), Vec2::new(0.0, -7.0));
    }

    #[test]
    fn shift_turns_the_wheel_sideways() {
        let input = input_with(vec![wheel(
            MouseWheelUnit::Point,
            Vec2::new(0.0, 5.0),
            true,
        )]);
        assert_eq!(raw(&input), Vec2::new(5.0, 0.0));
    }

    #[test]
    fn taking_the_wheel_leaves_none_for_later() {
        let mut input = input_with(vec![wheel(
            MouseWheelUnit::Point,
            Vec2::new(0.0, 20.0),
            false,
        )]);
        take(&mut input);
        assert_eq!(raw(&input), Vec2::ZERO);
        assert_eq!(input.smooth_scroll_delta, Vec2::ZERO);
    }
}
