//! During Play, the mouse and touches over the viewport as the game's
//! input: positions in pixels from the viewport's top left, as the player
//! reports them from its window's.

use axcenity_core::input::{InputEvent, InputKind};
use axcenity_core::{UserInputState, UserInputType};
use eframe::egui;

use super::POINTS_PER_NOTCH;

/// What the viewport is, for turning a frame's egui events into the game's.
pub(super) struct GameArea {
    pub rect: egui::Rect,
    pub pixels_per_point: f32,
    /// The pointer is over the viewport (and nothing on top of it).
    pub hovered: bool,
    /// The wheel scrolled a ScrollingFrame, so the game doesn't use it.
    pub wheel_taken: bool,
}

impl GameArea {
    fn pixels(&self, pos: egui::Pos2) -> [f32; 2] {
        let at = (pos - self.rect.min) * self.pixels_per_point;
        [at.x, at.y]
    }

    /// The game's input in `events`. `ui_takes(position)` says whether a
    /// left press there is the UI's (a button, a text box, a widget); then
    /// it and its release are `processed`, which `ui_mouse_taken` keeps
    /// track of between frames.
    pub fn input(
        &self,
        events: &[egui::Event],
        ui_takes: impl Fn([f32; 2]) -> bool,
        ui_mouse_taken: &mut bool,
    ) -> Vec<InputEvent> {
        let mut input = Vec::new();
        let mut motion = egui::Vec2::ZERO;
        let mut pointer = None;
        for event in events {
            match event {
                egui::Event::PointerButton {
                    pos,
                    button,
                    pressed,
                    ..
                } => {
                    let button = match button {
                        egui::PointerButton::Primary => UserInputType::MouseButton1,
                        egui::PointerButton::Secondary => UserInputType::MouseButton2,
                        egui::PointerButton::Middle => UserInputType::MouseButton3,
                        _ => continue,
                    };
                    // Presses outside the viewport aren't the game's;
                    // releases are, so nothing stays held.
                    if *pressed && !(self.hovered && self.rect.contains(*pos)) {
                        continue;
                    }
                    let position = self.pixels(*pos);
                    let processed = if button != UserInputType::MouseButton1 {
                        false
                    } else if *pressed {
                        *ui_mouse_taken = ui_takes(position);
                        *ui_mouse_taken
                    } else {
                        std::mem::take(ui_mouse_taken)
                    };
                    input.push(InputEvent {
                        kind: InputKind::MouseButton {
                            button,
                            pressed: *pressed,
                            position,
                        },
                        processed,
                    });
                }
                egui::Event::PointerMoved(pos) => pointer = Some(*pos),
                egui::Event::MouseMoved(delta) => motion += *delta,
                egui::Event::MouseWheel { unit, delta, .. } if self.hovered => {
                    let notches = match unit {
                        egui::MouseWheelUnit::Point => delta.y / POINTS_PER_NOTCH,
                        egui::MouseWheelUnit::Line | egui::MouseWheelUnit::Page => delta.y,
                    };
                    let position = pointer.map(|pos| self.pixels(pos)).unwrap_or_default();
                    input.push(InputEvent {
                        kind: InputKind::Wheel { position, notches },
                        processed: self.wheel_taken,
                    });
                }
                egui::Event::Touch { id, phase, pos, .. } if self.rect.contains(*pos) => {
                    let state = match phase {
                        egui::TouchPhase::Start => UserInputState::Begin,
                        egui::TouchPhase::Move => UserInputState::Change,
                        egui::TouchPhase::End => UserInputState::End,
                        egui::TouchPhase::Cancel => UserInputState::Cancel,
                    };
                    input.push(InputEvent::new(InputKind::Touch {
                        id: id.0,
                        state,
                        position: self.pixels(*pos),
                    }));
                }
                _ => {}
            }
        }
        if self.hovered
            && motion != egui::Vec2::ZERO
            && let Some(pos) = pointer
        {
            input.push(InputEvent::new(InputKind::MouseMove {
                position: self.pixels(pos),
                delta: [motion.x, motion.y],
            }));
        }
        input
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area() -> GameArea {
        GameArea {
            rect: egui::Rect::from_min_size(egui::pos2(100.0, 50.0), egui::vec2(400.0, 300.0)),
            pixels_per_point: 2.0,
            hovered: true,
            wheel_taken: false,
        }
    }

    fn click(pos: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }

    #[test]
    fn a_click_on_the_game_ui_is_processed_and_positions_are_viewport_pixels() {
        let area = area();
        let mut taken = false;
        // The UI takes presses left of x = 100 pixels.
        let on_button = |[x, _]: [f32; 2]| x < 100.0;
        let input = area.input(
            &[
                click(egui::pos2(120.0, 60.0), true),
                click(egui::pos2(120.0, 60.0), false),
                click(egui::pos2(300.0, 60.0), true),
            ],
            on_button,
            &mut taken,
        );
        let at = |pressed| InputKind::MouseButton {
            button: UserInputType::MouseButton1,
            pressed,
            position: [40.0, 20.0],
        };
        assert_eq!(input[0], InputEvent::processed(at(true)));
        assert_eq!(input[1], InputEvent::processed(at(false)));
        assert_eq!(
            input[2],
            InputEvent::new(InputKind::MouseButton {
                button: UserInputType::MouseButton1,
                pressed: true,
                position: [400.0, 20.0],
            })
        );
        // Outside the viewport, a press isn't the game's.
        assert!(
            area.input(
                &[click(egui::pos2(10.0, 10.0), true)],
                on_button,
                &mut taken
            )
            .is_empty()
        );
    }
}
