//! Game controllers, for moving focus through menus. The d-pad and the left
//! stick move focus (held, they repeat), South (A on an Xbox pad, Cross on
//! a PlayStation one) confirms, East (B, Circle) goes back, and the
//! shoulder buttons step through the tab order. Read with `gilrs`; hosts
//! keep one [`Gamepads`] and pass what [`Gamepads::poll`] returns to
//! [`crate::UiInteraction::navigate`], as they do with the keyboard's keys.

use std::collections::HashMap;

use gilrs::{Axis, Button, EventType, GamepadId, Gilrs};

/// A controller's button, as [`button_key`] reads it.
pub use gilrs::Button as GamepadButton;

use crate::NavKey;

/// How long a direction is held before it repeats, in seconds.
pub const REPEAT_DELAY: f32 = 0.4;
/// How often a held direction repeats after that, in seconds.
pub const REPEAT_INTERVAL: f32 = 0.12;
/// How far the left stick goes over before it counts as a direction.
const STICK_THRESHOLD: f32 = 0.5;

/// What a controller's button does to the UI, when it does anything.
pub fn button_key(button: Button) -> Option<NavKey> {
    Some(match button {
        Button::DPadUp => NavKey::Up,
        Button::DPadDown => NavKey::Down,
        Button::DPadLeft => NavKey::Left,
        Button::DPadRight => NavKey::Right,
        Button::South => NavKey::Confirm,
        Button::East => NavKey::Back,
        Button::LeftTrigger => NavKey::Previous,
        Button::RightTrigger => NavKey::Next,
        _ => return None,
    })
}

fn is_direction(key: NavKey) -> bool {
    matches!(
        key,
        NavKey::Up | NavKey::Down | NavKey::Left | NavKey::Right
    )
}

/// Turns a held direction into presses: one as soon as it's held, then one
/// every [`REPEAT_INTERVAL`] once it has been held for [`REPEAT_DELAY`].
#[derive(Clone, Copy, Debug, Default)]
pub struct NavRepeat {
    held: Option<NavKey>,
    /// Seconds until the next repeat.
    wait: f32,
}

impl NavRepeat {
    /// `held` is the direction held this frame, `seconds` after the last.
    /// At most one press a frame.
    pub fn update(&mut self, held: Option<NavKey>, seconds: f32) -> Option<NavKey> {
        if held != self.held {
            self.held = held;
            self.wait = REPEAT_DELAY;
            return held;
        }
        let key = held?;
        self.wait -= seconds.max(0.0);
        if self.wait > 0.0 {
            return None;
        }
        self.wait = (self.wait + REPEAT_INTERVAL).max(0.0);
        Some(key)
    }
}

/// One controller's d-pad and left stick.
#[derive(Clone, Copy, Debug, Default)]
struct Pad {
    /// The d-pad buttons held: up, down, left, right.
    dpad: [bool; 4],
    /// The left stick, right and up positive.
    stick: [f32; 2],
}

impl Pad {
    /// The direction it's held in: the d-pad first, then the stick's
    /// furthest axis past the threshold.
    fn direction(&self) -> Option<NavKey> {
        const DPAD: [NavKey; 4] = [NavKey::Up, NavKey::Down, NavKey::Left, NavKey::Right];
        if let Some(index) = self.dpad.iter().position(|held| *held) {
            return Some(DPAD[index]);
        }
        let [x, y] = self.stick;
        if x.abs().max(y.abs()) < STICK_THRESHOLD {
            None
        } else if x.abs() > y.abs() {
            Some(if x > 0.0 { NavKey::Right } else { NavKey::Left })
        } else {
            Some(if y > 0.0 { NavKey::Up } else { NavKey::Down })
        }
    }
}

/// The game controllers plugged in. With none, or on a system `gilrs`
/// can't read controllers on, [`Self::poll`] returns nothing.
pub struct Gamepads {
    gilrs: Option<Gilrs>,
    pads: HashMap<GamepadId, Pad>,
    repeat: NavRepeat,
}

impl Default for Gamepads {
    fn default() -> Self {
        Self::new()
    }
}

impl Gamepads {
    pub fn new() -> Self {
        let gilrs = match Gilrs::new() {
            Ok(gilrs) => Some(gilrs),
            Err(err) => {
                log::warn!("game controllers aren't available: {err}");
                None
            }
        };
        Gamepads {
            gilrs,
            pads: HashMap::new(),
            repeat: NavRepeat::default(),
        }
    }

    /// What the controllers did since the last call, `seconds` ago, in
    /// order: button presses, and the held direction's repeats.
    pub fn poll(&mut self, seconds: f32) -> Vec<NavKey> {
        let mut keys = Vec::new();
        let Some(gilrs) = &mut self.gilrs else {
            return keys;
        };
        while let Some(event) = gilrs.next_event() {
            let pad = self.pads.entry(event.id).or_default();
            match event.event {
                EventType::ButtonPressed(button, _) => match button_key(button) {
                    Some(key) if is_direction(key) => set_dpad(pad, key, true),
                    Some(key) => keys.push(key),
                    None => {}
                },
                EventType::ButtonReleased(button, _) => {
                    if let Some(key) = button_key(button).filter(|key| is_direction(*key)) {
                        set_dpad(pad, key, false);
                    }
                }
                EventType::AxisChanged(Axis::LeftStickX, value, _) => pad.stick[0] = value,
                EventType::AxisChanged(Axis::LeftStickY, value, _) => pad.stick[1] = value,
                EventType::Disconnected => {
                    self.pads.remove(&event.id);
                }
                _ => {}
            }
        }
        let held = self.pads.values().find_map(Pad::direction);
        keys.extend(self.repeat.update(held, seconds));
        keys
    }
}

fn set_dpad(pad: &mut Pad, key: NavKey, held: bool) {
    let index = match key {
        NavKey::Up => 0,
        NavKey::Down => 1,
        NavKey::Left => 2,
        _ => 3,
    };
    pad.dpad[index] = held;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_held_direction_presses_once_then_repeats_after_a_delay() {
        let mut repeat = NavRepeat::default();
        let frame = 0.05;
        let mut presses = Vec::new();
        // Held for one second, then let go.
        for _ in 0..20 {
            presses.extend(repeat.update(Some(NavKey::Down), frame));
        }
        assert_eq!(repeat.update(None, frame), None);
        // Once at once, then at 0.4 s and every 0.12 s after: 0.4, 0.52,
        // 0.64, 0.76, 0.88 (rounding may land the last a frame either way).
        assert!((5..=7).contains(&presses.len()), "{presses:?}");
        assert!(presses.iter().all(|key| *key == NavKey::Down));

        // A new direction presses straight away.
        assert_eq!(repeat.update(Some(NavKey::Left), frame), Some(NavKey::Left));
        assert_eq!(repeat.update(Some(NavKey::Left), frame), None);
    }

    #[test]
    fn the_stick_counts_past_halfway_on_its_furthest_axis_and_the_dpad_wins() {
        let mut pad = Pad {
            stick: [0.3, -0.4],
            ..Pad::default()
        };
        assert_eq!(pad.direction(), None);
        pad.stick = [0.3, -0.8];
        assert_eq!(pad.direction(), Some(NavKey::Down));
        pad.stick = [-0.9, 0.6];
        assert_eq!(pad.direction(), Some(NavKey::Left));
        set_dpad(&mut pad, NavKey::Up, true);
        assert_eq!(pad.direction(), Some(NavKey::Up));
    }

    #[test]
    fn the_face_and_shoulder_buttons_confirm_go_back_and_tab() {
        assert_eq!(button_key(Button::South), Some(NavKey::Confirm));
        assert_eq!(button_key(Button::East), Some(NavKey::Back));
        assert_eq!(button_key(Button::RightTrigger), Some(NavKey::Next));
        assert_eq!(button_key(Button::LeftTrigger), Some(NavKey::Previous));
        assert_eq!(button_key(Button::DPadRight), Some(NavKey::Right));
        assert_eq!(button_key(Button::Start), None);
    }
}
