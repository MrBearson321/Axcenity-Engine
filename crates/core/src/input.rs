//! Input from the keyboard, mouse, touch screen and game controllers, and
//! the input map that names what it does.
//!
//! Hosts turn their window's events into [`InputEvent`]s. An
//! [`InputState`] follows them, so it knows what is held now. The
//! [`InputMap`] (a project's `input.json`) names actions, such as `Jump`,
//! and binds each to keys, mouse buttons, controller buttons or an axis;
//! the built-in controllers, UI navigation and scripts ask it whether an
//! action is held, or where its axis points, in that state.

use std::collections::{BTreeMap, BTreeSet};

use crate::{KeyCode, UserInputState, UserInputType};

/// How far a stick has to move from the middle before it counts, as a
/// fraction of the way to its edge. Worn sticks rest a little off centre.
pub const STICK_DEAD_ZONE: f32 = 0.15;

/// The settings name a rebound action is kept under, before its name
/// (`Input/Jump`). Settings go into save slots, so a rebind does too.
pub const REBIND_PREFIX: &str = "Input/";

/// One thing an action can be bound to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Input {
    /// A keyboard key or a controller button. In an axis, `Thumbstick1` or
    /// `Thumbstick2` is that stick's position.
    Key(KeyCode),
    /// `MouseButton1` (left), `MouseButton2` (right) or `MouseButton3`.
    Mouse(UserInputType),
    /// Four keys as an axis, in the order of W, A, S and D: up, left, down
    /// and right.
    Keys([KeyCode; 4]),
    /// The mouse's movement since the last frame, in pixels, for an axis
    /// such as `Look`.
    MouseMovement,
}

impl Input {
    /// How `input.json` and the Input settings write it: `Space`,
    /// `MouseButton1`, `W,A,S,D` or `MouseMovement`.
    pub fn name(&self) -> String {
        match self {
            Input::Key(key) => key.as_str().to_owned(),
            Input::Mouse(button) => button.as_str().to_owned(),
            Input::Keys(keys) => keys.map(KeyCode::as_str).join(","),
            Input::MouseMovement => "MouseMovement".to_owned(),
        }
    }

    /// The input `name` names, as [`Self::name`] writes it.
    pub fn parse(name: &str) -> Option<Input> {
        let name = name.trim();
        if name == "MouseMovement" {
            return Some(Input::MouseMovement);
        }
        if let Some(button) = UserInputType::from_name(name)
            && button.mouse_button().is_some()
        {
            return Some(Input::Mouse(button));
        }
        if name.contains(',') {
            let keys: Vec<KeyCode> = name
                .split(',')
                .map(|key| KeyCode::from_name(key.trim()))
                .collect::<Option<_>>()?;
            return Some(Input::Keys(keys.try_into().ok()?));
        }
        KeyCode::from_name(name).map(Input::Key)
    }

    /// True if `key` is this input or one of its keys.
    pub fn uses(&self, key: KeyCode) -> bool {
        match self {
            Input::Key(own) => *own == key,
            Input::Keys(keys) => keys.contains(&key),
            Input::Mouse(_) | Input::MouseMovement => false,
        }
    }
}

/// The inputs `text` lists, separated by spaces, as [`binding_text`] writes
/// them. `None` if one isn't an input.
pub fn parse_binding(text: &str) -> Option<Vec<Input>> {
    text.split_whitespace().map(Input::parse).collect()
}

/// `inputs` as one string, separated by spaces, for a setting.
pub fn binding_text(inputs: &[Input]) -> String {
    inputs.iter().map(Input::name).collect::<Vec<_>>().join(" ")
}

/// One named action and what it's bound to.
#[derive(Clone, Debug, PartialEq)]
pub struct Action {
    pub name: String,
    /// An axis, such as `Move`, points somewhere (`InputMap::axis`); any
    /// other action is held or not.
    pub axis: bool,
    pub inputs: Vec<Input>,
}

impl Action {
    fn new(name: &str, axis: bool, inputs: &[&str]) -> Self {
        Self {
            name: name.to_owned(),
            axis,
            inputs: inputs
                .iter()
                .map(|input| Input::parse(input).expect("default inputs are valid"))
                .collect(),
        }
    }

    /// True for the UI's own navigation actions (`UINavigate`, `UIConfirm`, ...).
    pub fn is_ui(&self) -> bool {
        self.name.starts_with("UI")
    }

    /// What a prompt shows for this action: its first controller button
    /// when `gamepad` (a controller was used last), else its first key or
    /// mouse button, such as `E`, `X` or `Shift`. Its first input when it
    /// has none of that device's; empty when it has none at all.
    pub fn label(&self, gamepad: bool) -> String {
        let on_pad = |input: &Input| match input {
            Input::Key(key) => key.is_gamepad(),
            Input::Keys(keys) => keys[0].is_gamepad(),
            Input::Mouse(_) | Input::MouseMovement => false,
        };
        let input = self
            .inputs
            .iter()
            .find(|input| on_pad(input) == gamepad)
            .or(self.inputs.first());
        match input {
            Some(Input::Key(key)) => key_label(*key),
            Some(Input::Mouse(UserInputType::MouseButton1)) => "Click".to_owned(),
            Some(Input::Mouse(UserInputType::MouseButton2)) => "Right Click".to_owned(),
            Some(input) => input.name(),
            None => String::new(),
        }
    }
}

/// A key's or button's short name, as a prompt shows it: `ButtonX` is
/// `X`, `ButtonL1` is `LB`, `Return` is `Enter`, `Seven` is `7`.
pub fn key_label(key: KeyCode) -> String {
    let short = match key {
        KeyCode::ButtonA => "A",
        KeyCode::ButtonB => "B",
        KeyCode::ButtonX => "X",
        KeyCode::ButtonY => "Y",
        KeyCode::ButtonL1 => "LB",
        KeyCode::ButtonR1 => "RB",
        KeyCode::ButtonL2 => "LT",
        KeyCode::ButtonR2 => "RT",
        KeyCode::ButtonL3 => "LS",
        KeyCode::ButtonR3 => "RS",
        KeyCode::ButtonStart => "Menu",
        KeyCode::ButtonSelect => "View",
        KeyCode::DPadUp => "Up",
        KeyCode::DPadDown => "Down",
        KeyCode::DPadLeft => "Left",
        KeyCode::DPadRight => "Right",
        KeyCode::Zero => "0",
        KeyCode::One => "1",
        KeyCode::Two => "2",
        KeyCode::Three => "3",
        KeyCode::Four => "4",
        KeyCode::Five => "5",
        KeyCode::Six => "6",
        KeyCode::Seven => "7",
        KeyCode::Eight => "8",
        KeyCode::Nine => "9",
        KeyCode::Return => "Enter",
        KeyCode::Escape => "Esc",
        KeyCode::LeftShift | KeyCode::RightShift => "Shift",
        KeyCode::LeftControl | KeyCode::RightControl => "Ctrl",
        KeyCode::LeftAlt | KeyCode::RightAlt => "Alt",
        other => other.as_str(),
    };
    short.to_owned()
}

/// The UI's navigation actions, which the focus system reads.
pub const UI_NAVIGATE: &str = "UINavigate";
pub const UI_CONFIRM: &str = "UIConfirm";
pub const UI_BACK: &str = "UIBack";
pub const UI_NEXT: &str = "UINext";
pub const UI_PREVIOUS: &str = "UIPrevious";

/// A project's actions, in order. [`Default`] is the actions every new
/// project starts with.
#[derive(Clone, Debug, PartialEq)]
pub struct InputMap {
    pub actions: Vec<Action>,
}

impl Default for InputMap {
    fn default() -> Self {
        let actions = vec![
            Action::new(
                "Move",
                true,
                &["W,A,S,D", "Up,Left,Down,Right", "Thumbstick1"],
            ),
            Action::new("Look", true, &["MouseMovement", "Thumbstick2"]),
            Action::new("Jump", false, &["Space", "ButtonA"]),
            Action::new("Run", false, &["LeftShift", "ButtonL3"]),
            Action::new("Crouch", false, &["C", "ButtonB"]),
            Action::new("Interact", false, &["E", "ButtonX"]),
            Action::new("Primary", false, &["MouseButton1", "ButtonR2"]),
            Action::new("Secondary", false, &["MouseButton2", "ButtonL2"]),
            Action::new("Menu", false, &["Escape", "ButtonStart"]),
            Action::new(
                UI_NAVIGATE,
                true,
                &[
                    "Up,Left,Down,Right",
                    "DPadUp,DPadLeft,DPadDown,DPadRight",
                    "Thumbstick1",
                ],
            ),
            Action::new(UI_CONFIRM, false, &["Return", "ButtonA"]),
            Action::new(UI_BACK, false, &["Escape", "ButtonB"]),
            Action::new(UI_NEXT, false, &["Tab", "ButtonR1"]),
            Action::new(UI_PREVIOUS, false, &["ButtonL1"]),
        ];
        Self { actions }
    }
}

impl InputMap {
    pub fn action(&self, name: &str) -> Option<&Action> {
        self.actions.iter().find(|action| action.name == name)
    }

    /// Binds `name` to `inputs` instead of what it had. An error names the
    /// actions there are, when there's no action called `name`.
    pub fn rebind(&mut self, name: &str, inputs: Vec<Input>) -> Result<(), String> {
        match self.actions.iter_mut().find(|action| action.name == name) {
            Some(action) => {
                action.inputs = inputs;
                Ok(())
            }
            None => Err(format!(
                "there's no action named \"{name}\" in the input map; choose from {}",
                self.actions
                    .iter()
                    .map(|action| action.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
        }
    }

    /// This map with the rebinds kept in `settings` (named
    /// [`REBIND_PREFIX`] and the action) applied. A setting that isn't a
    /// list of inputs, or names no action, is left out.
    pub fn with_rebinds<'a>(&self, settings: impl IntoIterator<Item = (&'a str, &'a str)>) -> Self {
        let mut map = self.clone();
        for (name, text) in settings {
            if let Some(action) = name.strip_prefix(REBIND_PREFIX)
                && let Some(inputs) = parse_binding(text)
            {
                let _ = map.rebind(action, inputs);
            }
        }
        map
    }

    /// True while something bound to `name` is held: a key, a button, a
    /// stick past its dead zone, or (for an axis) anything moving it.
    pub fn is_pressed(&self, name: &str, state: &InputState) -> bool {
        let Some(action) = self.action(name) else {
            return false;
        };
        if action.axis {
            let [x, y] = self.axis(name, state);
            return x != 0.0 || y != 0.0;
        }
        action.inputs.iter().any(|input| match input {
            Input::Key(key) => state.is_down(*key),
            Input::Mouse(button) => state.is_mouse_down(*button),
            Input::Keys(keys) => keys.iter().any(|key| state.is_down(*key)),
            Input::MouseMovement => state.mouse_delta != [0.0, 0.0],
        })
    }

    /// Where `name` points, right and up positive. Keys and sticks add up
    /// to at most 1 long (a half-pushed stick is half); the mouse's
    /// movement adds its pixels on top. Zero for an unknown action.
    pub fn axis(&self, name: &str, state: &InputState) -> [f32; 2] {
        let Some(action) = self.action(name) else {
            return [0.0, 0.0];
        };
        let (mut x, mut y) = (0.0, 0.0);
        let held = |key: KeyCode| f32::from(u8::from(state.is_down(key)));
        for input in &action.inputs {
            match input {
                Input::Keys([up, left, down, right]) => {
                    x += held(*right) - held(*left);
                    y += held(*up) - held(*down);
                }
                Input::Key(key) if key.is_stick() => {
                    let [sx, sy] = state.stick(*key);
                    x += sx;
                    y += sy;
                }
                Input::Key(key) => y += held(*key),
                Input::Mouse(button) => y += f32::from(u8::from(state.is_mouse_down(*button))),
                Input::MouseMovement => {}
            }
        }
        let length = x.hypot(y);
        if length > 1.0 {
            x /= length;
            y /= length;
        }
        if action.inputs.contains(&Input::MouseMovement) {
            x += state.mouse_delta[0];
            // The mouse reports down as positive; the axis has up positive.
            y -= state.mouse_delta[1];
        }
        [x, y]
    }

    /// True if `key` is bound to an action other than the UI's own.
    pub fn is_game_input(&self, key: KeyCode) -> bool {
        self.actions
            .iter()
            .any(|action| !action.is_ui() && action.inputs.iter().any(|input| input.uses(key)))
    }

    /// The UI action `key` triggers when pressed, if any: `UIConfirm`,
    /// `UIBack`, `UINext`, `UIPrevious`, or `UINavigate` with the direction
    /// (right and up positive) when it's one of a `Keys` axis's keys.
    pub fn ui_action(&self, key: KeyCode) -> Option<(&str, [i8; 2])> {
        for action in self.actions.iter().filter(|action| action.is_ui()) {
            for input in &action.inputs {
                match input {
                    Input::Key(own) if *own == key && !action.axis => {
                        return Some((action.name.as_str(), [0, 0]));
                    }
                    Input::Keys([up, left, down, right]) if action.axis => {
                        let direction = if key == *up {
                            [0, 1]
                        } else if key == *down {
                            [0, -1]
                        } else if key == *left {
                            [-1, 0]
                        } else if key == *right {
                            [1, 0]
                        } else {
                            continue;
                        };
                        return Some((action.name.as_str(), direction));
                    }
                    _ => {}
                }
            }
        }
        None
    }
}

/// One controller's held buttons and sticks.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PadState {
    pub buttons: BTreeSet<KeyCode>,
    /// `Thumbstick1` then `Thumbstick2`, right and up positive, -1 to 1.
    pub sticks: [[f32; 2]; 2],
}

/// What one input did. `processed` is true when the game's UI took it (a
/// click on a button, typing in a TextBox, a key that moved focus): scripts
/// still hear it, as `gameProcessedEvent`, but the built-in controllers and
/// actions don't use it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InputEvent {
    pub kind: InputKind,
    pub processed: bool,
}

impl InputEvent {
    pub fn new(kind: InputKind) -> Self {
        Self {
            kind,
            processed: false,
        }
    }

    /// The same input, taken by the UI.
    pub fn processed(kind: InputKind) -> Self {
        Self {
            kind,
            processed: true,
        }
    }
}

impl From<InputKind> for InputEvent {
    fn from(kind: InputKind) -> Self {
        Self::new(kind)
    }
}

/// The kinds of input. Positions are in physical pixels from the window's
/// top left.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum InputKind {
    Key {
        key: KeyCode,
        pressed: bool,
    },
    /// `button` is `MouseButton1`, `MouseButton2` or `MouseButton3`.
    MouseButton {
        button: UserInputType,
        pressed: bool,
        position: [f32; 2],
    },
    /// The mouse moved `delta` pixels (right and down positive) and is now
    /// at `position`. While the cursor is locked, `position` stays put.
    MouseMove {
        position: [f32; 2],
        delta: [f32; 2],
    },
    /// Wheel notches, away from the player positive.
    Wheel {
        position: [f32; 2],
        notches: f32,
    },
    /// A finger, by its number for as long as it touches.
    Touch {
        id: u64,
        state: UserInputState,
        position: [f32; 2],
    },
    /// Controller `pad` (from 0) pressed or let go of a button.
    PadButton {
        pad: u8,
        key: KeyCode,
        pressed: bool,
    },
    /// A stick (`Thumbstick1` or `Thumbstick2`) of controller `pad` moved.
    PadStick {
        pad: u8,
        stick: KeyCode,
        position: [f32; 2],
    },
    PadConnected(u8),
    PadDisconnected(u8),
}

/// What's held right now, followed from [`InputEvent`]s.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct InputState {
    keys: BTreeSet<KeyCode>,
    mouse: [bool; 3],
    pub mouse_position: [f32; 2],
    /// How far the mouse moved this frame, in pixels; hosts clear it with
    /// [`Self::end_frame`].
    pub mouse_delta: [f32; 2],
    pads: BTreeMap<u8, PadState>,
    touches: BTreeMap<u64, [f32; 2]>,
}

impl InputState {
    /// Follows `kind`. False when it changes nothing, such as a key the
    /// system repeats while it's held, or the release of one that isn't.
    pub fn apply(&mut self, kind: &InputKind) -> bool {
        match *kind {
            InputKind::Key { key, pressed } => set(&mut self.keys, key, pressed),
            InputKind::MouseButton {
                button,
                pressed,
                position,
            } => {
                self.mouse_position = position;
                match button.mouse_button() {
                    Some(index) if self.mouse[index] != pressed => {
                        self.mouse[index] = pressed;
                        true
                    }
                    _ => false,
                }
            }
            InputKind::MouseMove { position, delta } => {
                self.mouse_position = position;
                self.mouse_delta[0] += delta[0];
                self.mouse_delta[1] += delta[1];
                true
            }
            InputKind::Wheel { position, .. } => {
                self.mouse_position = position;
                true
            }
            InputKind::Touch {
                id,
                state,
                position,
            } => {
                match state {
                    UserInputState::Begin | UserInputState::Change => {
                        self.touches.insert(id, position);
                    }
                    _ => {
                        self.touches.remove(&id);
                    }
                }
                true
            }
            InputKind::PadButton { pad, key, pressed } => {
                set(&mut self.pads.entry(pad).or_default().buttons, key, pressed)
            }
            InputKind::PadStick {
                pad,
                stick,
                position,
            } => {
                let index = usize::from(stick == KeyCode::Thumbstick2);
                let state = self.pads.entry(pad).or_default();
                let changed = state.sticks[index] != position;
                state.sticks[index] = position;
                changed
            }
            InputKind::PadConnected(pad) => {
                let new = !self.pads.contains_key(&pad);
                self.pads.entry(pad).or_default();
                new
            }
            InputKind::PadDisconnected(pad) => self.pads.remove(&pad).is_some(),
        }
    }

    /// Follows `event` as the game sees it: a press the UI took is left
    /// out, though its release still lets go.
    pub fn follow(&mut self, event: &InputEvent) -> bool {
        let press = match event.kind {
            InputKind::Key { pressed, .. }
            | InputKind::MouseButton { pressed, .. }
            | InputKind::PadButton { pressed, .. } => pressed,
            InputKind::Touch { state, .. } => state == UserInputState::Begin,
            _ => false,
        };
        if event.processed && press {
            return false;
        }
        self.apply(&event.kind)
    }

    /// Starts the next frame: the mouse hasn't moved in it yet.
    pub fn end_frame(&mut self) {
        self.mouse_delta = [0.0, 0.0];
    }

    /// True while keyboard key `key` is held.
    pub fn is_key_down(&self, key: KeyCode) -> bool {
        self.keys.contains(&key)
    }

    /// True while `key` is held on the keyboard or on any controller, or,
    /// for a stick, while it's pushed past its dead zone.
    pub fn is_down(&self, key: KeyCode) -> bool {
        if key.is_stick() {
            let [x, y] = self.stick(key);
            return x != 0.0 || y != 0.0;
        }
        self.keys.contains(&key) || self.pads.values().any(|pad| pad.buttons.contains(&key))
    }

    pub fn is_mouse_down(&self, button: UserInputType) -> bool {
        button.mouse_button().is_some_and(|index| self.mouse[index])
    }

    /// The stick (`Thumbstick1` or `Thumbstick2`) pushed furthest across the
    /// controllers, right and up positive, with the dead zone taken out.
    pub fn stick(&self, stick: KeyCode) -> [f32; 2] {
        let index = usize::from(stick == KeyCode::Thumbstick2);
        self.pads
            .values()
            .map(|pad| dead_zone(pad.sticks[index]))
            .max_by(|a, b| a[0].hypot(a[1]).total_cmp(&b[0].hypot(b[1])))
            .unwrap_or([0.0, 0.0])
    }

    /// Controller `pad`, if it's connected.
    pub fn pad(&self, pad: u8) -> Option<&PadState> {
        self.pads.get(&pad)
    }

    /// Where each finger touching is, by its number.
    pub fn touches(&self) -> &BTreeMap<u64, [f32; 2]> {
        &self.touches
    }

    /// The events that let go of everything held, for when the window
    /// loses focus (it won't hear the releases).
    pub fn release_all(&self) -> Vec<InputEvent> {
        let mut events: Vec<InputEvent> = self
            .keys
            .iter()
            .map(|&key| InputKind::Key {
                key,
                pressed: false,
            })
            .map(InputEvent::new)
            .collect();
        for (index, held) in self.mouse.iter().enumerate() {
            if *held {
                let button = [
                    UserInputType::MouseButton1,
                    UserInputType::MouseButton2,
                    UserInputType::MouseButton3,
                ][index];
                events.push(InputEvent::new(InputKind::MouseButton {
                    button,
                    pressed: false,
                    position: self.mouse_position,
                }));
            }
        }
        for (&pad, state) in &self.pads {
            for &key in &state.buttons {
                events.push(InputEvent::new(InputKind::PadButton {
                    pad,
                    key,
                    pressed: false,
                }));
            }
        }
        events
    }
}

fn set(held: &mut BTreeSet<KeyCode>, key: KeyCode, pressed: bool) -> bool {
    if pressed {
        held.insert(key)
    } else {
        held.remove(&key)
    }
}

/// A stick's position with the dead zone in the middle taken out, so it
/// still reaches 1 at the edge.
fn dead_zone([x, y]: [f32; 2]) -> [f32; 2] {
    let length = x.hypot(y);
    if length <= STICK_DEAD_ZONE {
        return [0.0, 0.0];
    }
    let scaled = ((length - STICK_DEAD_ZONE) / (1.0 - STICK_DEAD_ZONE)).min(1.0);
    [x / length * scaled, y / length * scaled]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(state: &mut InputState, key: KeyCode) {
        state.apply(&InputKind::Key { key, pressed: true });
    }

    #[test]
    fn inputs_round_trip_through_their_names() {
        let map = InputMap::default();
        for action in &map.actions {
            for input in &action.inputs {
                assert_eq!(Input::parse(&input.name()), Some(*input), "{input:?}");
            }
            let text = binding_text(&action.inputs);
            assert_eq!(parse_binding(&text).as_deref(), Some(&action.inputs[..]));
        }
        assert_eq!(Input::parse("Jump"), None);
        assert_eq!(Input::parse("W,A,S"), None);
        assert_eq!(parse_binding("E Nope"), None);
    }

    #[test]
    fn interact_is_labelled_by_the_device_used_last() {
        let map = InputMap::default();
        let interact = map.action("Interact").expect("a default action");
        assert_eq!(interact.label(false), "E");
        assert_eq!(interact.label(true), "X");
        // An action with only keys shows its key on a controller too.
        let keys_only = Action::new("Open", false, &["LeftShift"]);
        assert_eq!(keys_only.label(true), "Shift");
        assert_eq!(
            Action::new("Use", false, &["MouseButton1"]).label(false),
            "Click"
        );
        assert_eq!(Action::new("None", false, &[]).label(false), "");
    }

    #[test]
    fn wasd_and_the_left_stick_are_one_move_axis() {
        let map = InputMap::default();
        let mut state = InputState::default();
        assert_eq!(map.axis("Move", &state), [0.0, 0.0]);
        press(&mut state, KeyCode::W);
        press(&mut state, KeyCode::D);
        let [x, y] = map.axis("Move", &state);
        assert!((x - y).abs() < 1e-6 && (x.hypot(y) - 1.0).abs() < 1e-5);

        let mut pad = InputState::default();
        pad.apply(&InputKind::PadStick {
            pad: 0,
            stick: KeyCode::Thumbstick1,
            position: [0.0, 0.1],
        });
        assert_eq!(map.axis("Move", &pad), [0.0, 0.0], "inside the dead zone");
        pad.apply(&InputKind::PadStick {
            pad: 0,
            stick: KeyCode::Thumbstick1,
            position: [0.0, 1.0],
        });
        assert_eq!(map.axis("Move", &pad), [0.0, 1.0]);
        assert!(map.is_pressed("Move", &pad));
    }

    #[test]
    fn a_rebind_replaces_the_inputs_and_survives_as_a_setting() {
        let mut map = InputMap::default();
        let mut state = InputState::default();
        press(&mut state, KeyCode::E);
        assert!(!map.is_pressed("Jump", &state));
        map.rebind("Jump", vec![Input::Key(KeyCode::E)]).unwrap();
        assert!(map.is_pressed("Jump", &state));
        let err = map.rebind("Fly", Vec::new()).unwrap_err();
        assert!(err.contains("no action named \"Fly\""), "{err}");

        let rebound =
            InputMap::default().with_rebinds([("Input/Jump", "E ButtonY"), ("Volume", "3")]);
        assert_eq!(
            rebound.action("Jump").unwrap().inputs,
            [Input::Key(KeyCode::E), Input::Key(KeyCode::ButtonY)]
        );
    }

    #[test]
    fn controller_buttons_count_on_any_pad_and_release_all_lets_go() {
        let map = InputMap::default();
        let mut state = InputState::default();
        state.apply(&InputKind::PadButton {
            pad: 1,
            key: KeyCode::ButtonA,
            pressed: true,
        });
        press(&mut state, KeyCode::W);
        assert!(map.is_pressed("Jump", &state));
        assert!(
            !state.is_key_down(KeyCode::ButtonA),
            "IsKeyDown is the keyboard"
        );
        for event in state.release_all() {
            state.apply(&event.kind);
        }
        assert!(!map.is_pressed("Jump", &state));
        assert!(!state.is_down(KeyCode::W));
    }

    #[test]
    fn the_game_leaves_out_presses_the_ui_took_but_not_their_releases() {
        let mut game = InputState::default();
        let space = |pressed| InputKind::Key {
            key: KeyCode::Space,
            pressed,
        };
        assert!(!game.follow(&InputEvent::processed(space(true))));
        assert!(!game.is_down(KeyCode::Space));
        game.follow(&InputEvent::new(space(true)));
        assert!(game.follow(&InputEvent::processed(space(false))));
        assert!(!game.is_down(KeyCode::Space));
    }

    #[test]
    fn the_ui_actions_say_which_way_a_key_moves_focus() {
        let map = InputMap::default();
        assert_eq!(map.ui_action(KeyCode::Up), Some((UI_NAVIGATE, [0, 1])));
        assert_eq!(
            map.ui_action(KeyCode::DPadLeft),
            Some((UI_NAVIGATE, [-1, 0]))
        );
        assert_eq!(map.ui_action(KeyCode::Return), Some((UI_CONFIRM, [0, 0])));
        assert_eq!(map.ui_action(KeyCode::ButtonB), Some((UI_BACK, [0, 0])));
        assert_eq!(map.ui_action(KeyCode::W), None);
        assert!(map.is_game_input(KeyCode::ButtonA));
        assert!(!map.is_game_input(KeyCode::DPadUp));
    }
}
