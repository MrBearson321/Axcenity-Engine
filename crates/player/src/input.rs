//! The player's input: winit's keys, mouse, touches and the game
//! controllers as [`InputEvent`]s for LocalScripts, the game's held input
//! for the built-in controllers (through the input map's actions), and the
//! cursor `UserInputService.MouseBehavior` asks for.

use axcenity_character::MoveInput;
use axcenity_core::input::{InputEvent, InputKind, InputMap, InputState};
use axcenity_core::{MouseBehavior, UserInputState, UserInputType, World};
use axcenity_ui::{Gamepads, NavKey, PadNavigation, UiInteraction};
use winit::event::{MouseButton, Touch, TouchPhase};
use winit::keyboard::KeyCode;

/// A frame's input, gathered from window events until the frame takes it.
pub struct PlayerInput {
    /// What happened since the last frame, in order, for LocalScripts.
    events: Vec<InputEvent>,
    /// What the game holds: everything but the presses the UI took.
    pub game: InputState,
    /// The project's input map with the player's rebinds, as of the last
    /// frame (`ScriptRuntime::input_map`).
    pub map: InputMap,
    /// Raw mouse movement since the last frame, in pixels.
    motion: [f32; 2],
    gamepads: Gamepads,
    pads: PadNavigation,
}

impl PlayerInput {
    pub fn new(map: InputMap) -> Self {
        Self {
            events: Vec::new(),
            game: InputState::default(),
            map,
            motion: [0.0, 0.0],
            gamepads: Gamepads::new(),
            pads: PadNavigation::default(),
        }
    }

    pub fn push(&mut self, event: InputEvent) {
        self.game.follow(&event);
        self.events.push(event);
    }

    /// The mouse moved this far, right and down positive.
    pub fn add_motion(&mut self, dx: f32, dy: f32) {
        self.motion[0] += dx;
        self.motion[1] += dy;
    }

    /// The controllers' input since the last frame, `seconds` ago: what the
    /// UI takes moves focus (the keys returned), and the rest is the game's.
    pub fn poll_pads(&mut self, ui: &UiInteraction, world: &World, seconds: f32) -> Vec<NavKey> {
        let mut events = self.gamepads.poll();
        let keys = self.pads.update(ui, world, &self.map, &mut events, seconds);
        for event in events {
            self.push(event);
        }
        keys
    }

    /// The built-in controllers' input from the actions, with `look` (the
    /// mouse look, in pixels) and `zoom` (wheel notches), `seconds` after
    /// the last frame.
    pub fn motion_input(&self, look: [f32; 2], zoom: f32, seconds: f32) -> MoveInput {
        MoveInput::from_actions(&self.map, &self.game, look, zoom, seconds)
    }

    /// Takes the frame's events, with the mouse's movement since the last
    /// frame (at `pointer`) at the end, and starts the next frame.
    pub fn take_events(&mut self, pointer: Option<[f32; 2]>) -> Vec<InputEvent> {
        let delta = std::mem::take(&mut self.motion);
        if delta != [0.0, 0.0] {
            self.push(InputEvent::new(InputKind::MouseMove {
                position: pointer.unwrap_or(self.game.mouse_position),
                delta,
            }));
        }
        self.game.end_frame();
        std::mem::take(&mut self.events)
    }
}

/// The `Enum.UserInputType` of a mouse button scripts know.
pub fn mouse_button(button: MouseButton) -> Option<UserInputType> {
    Some(match button {
        MouseButton::Left => UserInputType::MouseButton1,
        MouseButton::Right => UserInputType::MouseButton2,
        MouseButton::Middle => UserInputType::MouseButton3,
        _ => return None,
    })
}

/// A finger on a touch screen, as scripts see it.
pub fn touch(touch: &Touch) -> InputKind {
    let state = match touch.phase {
        TouchPhase::Started => UserInputState::Begin,
        TouchPhase::Moved => UserInputState::Change,
        TouchPhase::Ended => UserInputState::End,
        TouchPhase::Cancelled => UserInputState::Cancel,
    };
    InputKind::Touch {
        id: touch.id,
        state,
        position: [touch.location.x as f32, touch.location.y as f32],
    }
}

/// Where the cursor may go.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorLock {
    Free,
    /// Held in the middle of the window.
    Centre,
    /// Held where it is.
    InPlace,
}

/// How the cursor should be: `MouseBehavior`, then right-button mouse look
/// (`looking`), which holds it where it is. It's hidden while held, and
/// when `MouseIconEnabled` is false.
pub fn cursor_mode(behavior: MouseBehavior, icon: bool, looking: bool) -> (CursorLock, bool) {
    let lock = match behavior {
        MouseBehavior::LockCenter => CursorLock::Centre,
        MouseBehavior::LockCurrentPosition => CursorLock::InPlace,
        MouseBehavior::Default if looking => CursorLock::InPlace,
        MouseBehavior::Default => CursorLock::Free,
    };
    (lock, icon && lock == CursorLock::Free)
}

/// The middle of a window `size` pixels across.
pub fn centre(size: (u32, u32)) -> [f32; 2] {
    [size.0 as f32 / 2.0, size.1 as f32 / 2.0]
}

/// Each winit key, and the key scripts know it as (`Enum.KeyCode.<name>`).
/// Keys are physical positions, like the movement keys.
pub const SCRIPT_KEYS: [(KeyCode, axcenity_core::KeyCode); 52] = {
    use axcenity_core::KeyCode as K;
    [
        (KeyCode::KeyA, K::A),
        (KeyCode::KeyB, K::B),
        (KeyCode::KeyC, K::C),
        (KeyCode::KeyD, K::D),
        (KeyCode::KeyE, K::E),
        (KeyCode::KeyF, K::F),
        (KeyCode::KeyG, K::G),
        (KeyCode::KeyH, K::H),
        (KeyCode::KeyI, K::I),
        (KeyCode::KeyJ, K::J),
        (KeyCode::KeyK, K::K),
        (KeyCode::KeyL, K::L),
        (KeyCode::KeyM, K::M),
        (KeyCode::KeyN, K::N),
        (KeyCode::KeyO, K::O),
        (KeyCode::KeyP, K::P),
        (KeyCode::KeyQ, K::Q),
        (KeyCode::KeyR, K::R),
        (KeyCode::KeyS, K::S),
        (KeyCode::KeyT, K::T),
        (KeyCode::KeyU, K::U),
        (KeyCode::KeyV, K::V),
        (KeyCode::KeyW, K::W),
        (KeyCode::KeyX, K::X),
        (KeyCode::KeyY, K::Y),
        (KeyCode::KeyZ, K::Z),
        (KeyCode::Digit0, K::Zero),
        (KeyCode::Digit1, K::One),
        (KeyCode::Digit2, K::Two),
        (KeyCode::Digit3, K::Three),
        (KeyCode::Digit4, K::Four),
        (KeyCode::Digit5, K::Five),
        (KeyCode::Digit6, K::Six),
        (KeyCode::Digit7, K::Seven),
        (KeyCode::Digit8, K::Eight),
        (KeyCode::Digit9, K::Nine),
        (KeyCode::Space, K::Space),
        (KeyCode::Enter, K::Return),
        // The keypad's Enter is the same to scripts.
        (KeyCode::NumpadEnter, K::Return),
        (KeyCode::Escape, K::Escape),
        (KeyCode::Tab, K::Tab),
        (KeyCode::Backspace, K::Backspace),
        (KeyCode::ArrowUp, K::Up),
        (KeyCode::ArrowDown, K::Down),
        (KeyCode::ArrowLeft, K::Left),
        (KeyCode::ArrowRight, K::Right),
        (KeyCode::ShiftLeft, K::LeftShift),
        (KeyCode::ShiftRight, K::RightShift),
        (KeyCode::ControlLeft, K::LeftControl),
        (KeyCode::ControlRight, K::RightControl),
        (KeyCode::AltLeft, K::LeftAlt),
        (KeyCode::AltRight, K::RightAlt),
    ]
};

/// The key scripts know for a winit key, if it's one they can see.
pub fn script_key(key: KeyCode) -> Option<axcenity_core::KeyCode> {
    SCRIPT_KEYS
        .into_iter()
        .find(|(winit, _)| *winit == key)
        .map(|(_, script)| script)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locking_the_mouse_in_the_centre_hides_the_cursor_there() {
        assert_eq!(
            cursor_mode(MouseBehavior::LockCenter, true, false),
            (CursorLock::Centre, false)
        );
        assert_eq!(
            cursor_mode(MouseBehavior::LockCurrentPosition, true, false),
            (CursorLock::InPlace, false)
        );
        assert_eq!(
            cursor_mode(MouseBehavior::Default, true, false),
            (CursorLock::Free, true)
        );
        // Right-button look holds the cursor still, hidden.
        assert_eq!(
            cursor_mode(MouseBehavior::Default, true, true),
            (CursorLock::InPlace, false)
        );
        assert_eq!(
            cursor_mode(MouseBehavior::Default, false, false),
            (CursorLock::Free, false)
        );
        assert_eq!(centre((1280, 720)), [640.0, 360.0]);
    }

    #[test]
    fn the_frames_mouse_movement_is_one_event_at_the_end() {
        let mut input = PlayerInput::new(InputMap::default());
        input.add_motion(3.0, 4.0);
        input.add_motion(1.0, -1.0);
        let events = input.take_events(Some([640.0, 360.0]));
        assert_eq!(
            events,
            [InputEvent::new(InputKind::MouseMove {
                position: [640.0, 360.0],
                delta: [4.0, 3.0],
            })]
        );
        assert!(input.take_events(None).is_empty());
        assert_eq!(input.game.mouse_delta, [0.0, 0.0]);
    }
}
