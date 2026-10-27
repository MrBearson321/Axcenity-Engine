//! Game controllers, and how keys and controllers move focus through menus.
//!
//! [`Gamepads`] reads controllers with `gilrs` and reports their buttons
//! and sticks as [`InputKind`]s, for scripts and the input map, as the
//! keyboard's keys are. What moves focus is the input map's UI actions
//! ([`nav_key`]): by default the arrows, the d-pad and the left stick move
//! it (a held direction repeats), Enter and A (South) confirm, Escape and B
//! (East) go back, and Tab and the shoulder buttons step through the tab
//! order. Hosts keep one [`PadNavigation`] to turn a frame's controller
//! input into [`NavKey`]s for [`crate::UiInteraction::navigate`].

use std::collections::HashMap;

use axcenity_core::KeyCode;
use axcenity_core::input::{
    Input, InputEvent, InputKind, InputMap, InputState, UI_BACK, UI_CONFIRM, UI_NAVIGATE, UI_NEXT,
    UI_PREVIOUS,
};
use gilrs::{Axis, Button, EventType, GamepadId, Gilrs};

/// A controller's button, as [`pad_key`] reads it.
pub use gilrs::Button as GamepadButton;

use crate::{NavKey, UiInteraction};
use axcenity_core::World;

/// How long a direction is held before it repeats, in seconds.
pub const REPEAT_DELAY: f32 = 0.4;
/// How often a held direction repeats after that, in seconds.
pub const REPEAT_INTERVAL: f32 = 0.12;
/// How far a stick goes over before it counts as a direction for focus.
const STICK_THRESHOLD: f32 = 0.5;
/// Controllers after the fourth aren't reported.
const MAX_PADS: u8 = 4;

/// The `Enum.KeyCode` a controller's button is, if it's one scripts know.
pub fn pad_key(button: Button) -> Option<KeyCode> {
    Some(match button {
        Button::South => KeyCode::ButtonA,
        Button::East => KeyCode::ButtonB,
        Button::West => KeyCode::ButtonX,
        Button::North => KeyCode::ButtonY,
        Button::LeftTrigger => KeyCode::ButtonL1,
        Button::RightTrigger => KeyCode::ButtonR1,
        Button::LeftTrigger2 => KeyCode::ButtonL2,
        Button::RightTrigger2 => KeyCode::ButtonR2,
        Button::LeftThumb => KeyCode::ButtonL3,
        Button::RightThumb => KeyCode::ButtonR3,
        Button::Start => KeyCode::ButtonStart,
        Button::Select => KeyCode::ButtonSelect,
        Button::DPadUp => KeyCode::DPadUp,
        Button::DPadDown => KeyCode::DPadDown,
        Button::DPadLeft => KeyCode::DPadLeft,
        Button::DPadRight => KeyCode::DPadRight,
        _ => return None,
    })
}

/// What pressing `key` (a keyboard key or a controller button) does to
/// the UI's focus, by the input map's UI actions. With `shift` held, the
/// next element (Tab) is the one before instead.
pub fn nav_key(map: &InputMap, key: KeyCode, shift: bool) -> Option<NavKey> {
    let (action, [x, y]) = map.ui_action(key)?;
    Some(match action {
        UI_NAVIGATE => direction_key(x, y)?,
        UI_CONFIRM => NavKey::Confirm,
        UI_BACK => NavKey::Back,
        UI_NEXT if shift => NavKey::Previous,
        UI_NEXT => NavKey::Next,
        UI_PREVIOUS => NavKey::Previous,
        _ => return None,
    })
}

fn direction_key(x: i8, y: i8) -> Option<NavKey> {
    Some(match (x, y) {
        (0, 1) => NavKey::Up,
        (0, -1) => NavKey::Down,
        (-1, 0) => NavKey::Left,
        (1, 0) => NavKey::Right,
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

/// The direction a controller holds for `UINavigate`, and the inputs it
/// comes from: a d-pad (four controller buttons) first, then a stick's
/// furthest axis past the threshold. The keyboard's arrows aren't here:
/// the system repeats a held key itself.
fn held_direction(map: &InputMap, state: &InputState) -> Option<(NavKey, Input)> {
    let action = map.action(UI_NAVIGATE)?;
    for input in &action.inputs {
        if let Input::Keys(keys) = input
            && keys.iter().all(|key| key.is_gamepad())
            && let Some(index) = keys.iter().position(|key| state.is_down(*key))
        {
            const WASD: [NavKey; 4] = [NavKey::Up, NavKey::Left, NavKey::Down, NavKey::Right];
            return Some((WASD[index], *input));
        }
    }
    for input in &action.inputs {
        if let Input::Key(stick) = input
            && stick.is_stick()
            && let Some(key) = stick_direction(state.stick(*stick))
        {
            return Some((key, *input));
        }
    }
    None
}

/// The direction a stick at `[x, y]` (right and up positive) points, once
/// past [`STICK_THRESHOLD`] on its furthest axis.
fn stick_direction([x, y]: [f32; 2]) -> Option<NavKey> {
    if x.abs().max(y.abs()) < STICK_THRESHOLD {
        None
    } else if x.abs() > y.abs() {
        Some(if x > 0.0 { NavKey::Right } else { NavKey::Left })
    } else {
        Some(if y > 0.0 { NavKey::Up } else { NavKey::Down })
    }
}

/// True if an input the game also uses (bound to a non-UI action) should go
/// to the UI for `key`: only while the UI takes it. Inputs only the UI uses
/// always go to it, so the d-pad focuses a menu with nothing focused yet.
fn ui_gets(ui: &UiInteraction, world: &World, map: &InputMap, input: &Input, key: NavKey) -> bool {
    let game = match input {
        Input::Key(key) => map.is_game_input(*key),
        Input::Keys(keys) => keys.iter().any(|key| map.is_game_input(*key)),
        Input::Mouse(_) | Input::MouseMovement => true,
    };
    !game || ui.takes(world, key)
}

/// Turns a frame's controller input into focus moves.
#[derive(Clone, Debug, Default)]
pub struct PadNavigation {
    /// Every controller input, taken by the UI or not, for the held direction.
    state: InputState,
    repeat: NavRepeat,
}

impl PadNavigation {
    /// The keys `events` (this frame's controller events, `seconds` after
    /// the last) give the UI: presses of buttons bound to the UI's actions,
    /// and the held direction's repeats. An input the game also uses (A for
    /// Jump, the left stick for Move) moves focus only while the UI
    /// [takes](UiInteraction::takes) it; the presses it takes are marked
    /// `processed`, so the game doesn't use them too.
    pub fn update(
        &mut self,
        ui: &UiInteraction,
        world: &World,
        map: &InputMap,
        events: &mut [InputEvent],
        seconds: f32,
    ) -> Vec<NavKey> {
        let mut keys = Vec::new();
        for event in events.iter_mut() {
            self.state.apply(&event.kind);
            if let InputKind::PadButton {
                key, pressed: true, ..
            } = event.kind
                && let Some(nav) = nav_key(map, key, false)
                && !is_direction(nav)
                && ui_gets(ui, world, map, &Input::Key(key), nav)
            {
                keys.push(nav);
                event.processed = true;
            }
        }
        let held = held_direction(map, &self.state)
            .filter(|(key, input)| ui_gets(ui, world, map, input, *key))
            .map(|(key, _)| key);
        keys.extend(self.repeat.update(held, seconds));
        keys
    }
}

/// The game controllers plugged in. With none, or on a system `gilrs`
/// can't read controllers on, [`Self::poll`] returns nothing.
pub struct Gamepads {
    gilrs: Option<Gilrs>,
    /// Each controller's number (0 is `Gamepad1`), the lowest free one
    /// when it was first seen.
    pads: HashMap<GamepadId, u8>,
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
        }
    }

    /// What the controllers did since the last call, in order. The first
    /// event from a controller connects it.
    pub fn poll(&mut self) -> Vec<InputEvent> {
        let mut events = Vec::new();
        let Some(gilrs) = &mut self.gilrs else {
            return events;
        };
        while let Some(event) = gilrs.next_event() {
            if event.event == EventType::Disconnected {
                if let Some(pad) = self.pads.remove(&event.id) {
                    events.push(InputKind::PadDisconnected(pad).into());
                }
                continue;
            }
            let pad = match self.pads.get(&event.id) {
                Some(pad) => *pad,
                None => {
                    let Some(free) = (0..MAX_PADS).find(|n| !self.pads.values().any(|p| p == n))
                    else {
                        continue;
                    };
                    self.pads.insert(event.id, free);
                    events.push(InputKind::PadConnected(free).into());
                    free
                }
            };
            let gamepad = gilrs.gamepad(event.id);
            let stick = |x: Axis, y: Axis| [gamepad.value(x), gamepad.value(y)];
            let kind = match event.event {
                EventType::ButtonPressed(button, _) | EventType::ButtonReleased(button, _) => {
                    let Some(key) = pad_key(button) else {
                        continue;
                    };
                    InputKind::PadButton {
                        pad,
                        key,
                        pressed: matches!(event.event, EventType::ButtonPressed(..)),
                    }
                }
                EventType::AxisChanged(Axis::LeftStickX | Axis::LeftStickY, _, _) => {
                    InputKind::PadStick {
                        pad,
                        stick: KeyCode::Thumbstick1,
                        position: stick(Axis::LeftStickX, Axis::LeftStickY),
                    }
                }
                EventType::AxisChanged(Axis::RightStickX | Axis::RightStickY, _, _) => {
                    InputKind::PadStick {
                        pad,
                        stick: KeyCode::Thumbstick2,
                        position: stick(Axis::RightStickX, Axis::RightStickY),
                    }
                }
                _ => continue,
            };
            events.push(kind.into());
        }
        events
    }
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
        let map = InputMap::default();
        let mut state = InputState::default();
        let stick = |state: &mut InputState, position| {
            state.apply(&InputKind::PadStick {
                pad: 0,
                stick: KeyCode::Thumbstick1,
                position,
            });
        };
        stick(&mut state, [0.3, -0.4]);
        assert_eq!(held_direction(&map, &state), None);
        stick(&mut state, [0.3, -0.8]);
        assert_eq!(held_direction(&map, &state).unwrap().0, NavKey::Down);
        stick(&mut state, [-0.9, 0.6]);
        assert_eq!(held_direction(&map, &state).unwrap().0, NavKey::Left);
        state.apply(&InputKind::PadButton {
            pad: 0,
            key: KeyCode::DPadUp,
            pressed: true,
        });
        assert_eq!(held_direction(&map, &state).unwrap().0, NavKey::Up);
        // The keyboard's arrows repeat by themselves, so they aren't held here.
        let mut keys = InputState::default();
        keys.apply(&InputKind::Key {
            key: KeyCode::Down,
            pressed: true,
        });
        assert_eq!(held_direction(&map, &keys), None);
    }

    #[test]
    fn the_face_and_shoulder_buttons_confirm_go_back_and_tab() {
        let map = InputMap::default();
        let key = |button| nav_key(&map, pad_key(button).unwrap(), false);
        assert_eq!(key(Button::South), Some(NavKey::Confirm));
        assert_eq!(key(Button::East), Some(NavKey::Back));
        assert_eq!(key(Button::RightTrigger), Some(NavKey::Next));
        assert_eq!(key(Button::LeftTrigger), Some(NavKey::Previous));
        assert_eq!(key(Button::DPadRight), Some(NavKey::Right));
        assert_eq!(key(Button::Start), None);
        assert_eq!(nav_key(&map, KeyCode::Tab, true), Some(NavKey::Previous));
        assert_eq!(nav_key(&map, KeyCode::Return, false), Some(NavKey::Confirm));
    }

    #[test]
    fn rebinding_a_ui_action_moves_focus_with_the_new_key() {
        let mut map = InputMap::default();
        map.rebind(UI_CONFIRM, vec![Input::Key(KeyCode::F)])
            .unwrap();
        assert_eq!(nav_key(&map, KeyCode::F, false), Some(NavKey::Confirm));
        assert_eq!(nav_key(&map, KeyCode::Return, false), None);
    }
}
