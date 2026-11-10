//! The player's input, for LocalScripts: `UserInputService` (the keyboard,
//! mouse, touch screen and game controllers, and the input map's actions)
//! and `ContextActionService` (functions bound to inputs, by priority).
//!
//! The host feeds a frame's [`InputEvent`]s in with
//! `ScriptRuntime::input_events`, and the project's input map with
//! `set_input_map`. A rebind (`RebindAction`) is kept as a SaveService
//! setting (`Input/Jump`), so it goes into save slots and comes back with
//! them; `ScriptRuntime::input_map` is the map with the rebinds applied.
//! Everything here lives in the client interpreter's app data.

use axcenity_core::input::{
    Input, InputEvent, InputKind, InputMap, InputState, REBIND_PREFIX, binding_text,
};
use axcenity_core::{InstanceId, KeyCode, MouseBehavior, UserInputState, UserInputType, Value};
use axcenity_core::{Vector2, Vector3};
use mlua::{
    FromLua, Function, Lua, MetaMethod, UserData, UserDataFields, UserDataMethods, Variadic,
};

use crate::events::{self, LuaSignal, Owner};
use crate::types::{EnumType, LuaEnumItem, LuaVector2, LuaVector3, type_name};
use crate::{save, scheduler};

/// A key going down or coming back up, as the host reports it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyEvent {
    pub key: KeyCode,
    pub pressed: bool,
}

impl From<KeyEvent> for InputEvent {
    fn from(KeyEvent { key, pressed }: KeyEvent) -> Self {
        InputEvent::new(InputKind::Key { key, pressed })
    }
}

/// `ContextActionService`'s default priority, as in Roblox
/// (`Enum.ContextActionPriority.Default`).
const DEFAULT_PRIORITY: f64 = 2000.0;

/// What the client knows about input.
pub struct Inputs {
    /// Everything held, including what the UI took, for `IsKeyDown` and friends.
    all: InputState,
    /// What the game holds (the UI's presses left out), for actions.
    game: InputState,
    /// The project's input map, before the player's rebinds.
    pub map: InputMap,
    pub mouse_behavior: MouseBehavior,
    pub mouse_icon_enabled: bool,
    /// `ContextActionService`'s bound actions, in the order bound.
    bound: Vec<BoundAction>,
    /// Counts up, so a later bind wins a tie in priority.
    next_order: u64,
    /// True when a controller was used last, rather than the keyboard or
    /// mouse, so prompts show its buttons (Phase 44).
    gamepad: bool,
    /// The mouse buttons the game pressed (true) and let go of this frame,
    /// in order, for ClickDetectors. Clicks the UI took aren't here.
    clicks: Vec<(UserInputType, bool)>,
    /// The game's wheel notches this frame, away from the player positive,
    /// for the hotbar (Phase 45).
    wheel: f32,
}

impl Default for Inputs {
    fn default() -> Self {
        Self {
            all: InputState::default(),
            game: InputState::default(),
            map: InputMap::default(),
            mouse_behavior: MouseBehavior::Default,
            mouse_icon_enabled: true,
            bound: Vec::new(),
            next_order: 0,
            gamepad: false,
            clicks: Vec::new(),
            wheel: 0.0,
        }
    }
}

impl Inputs {
    /// Unbinds every action `script` bound, when it stops.
    pub fn forget_script(&mut self, script: InstanceId) {
        self.bound.retain(|action| action.script != Some(script));
    }
}

struct BoundAction {
    name: String,
    handler: Function,
    script: Option<InstanceId>,
    priority: f64,
    touch_button: bool,
    /// `Enum.KeyCode` and `Enum.UserInputType` items.
    inputs: Vec<LuaEnumItem>,
    order: u64,
}

impl BoundAction {
    /// True if `input` is one of this action's: its key, or its kind
    /// (`Keyboard` for any key, `Gamepad1` for anything on that controller).
    fn wants(&self, input: &LuaInputObject) -> bool {
        self.inputs.iter().any(|item| match item.enum_type {
            EnumType::KeyCode => input.key != KeyCode::Unknown && item.as_key() == Some(input.key),
            EnumType::UserInputType => UserInputType::from_name(item.name) == Some(input.kind),
            _ => false,
        })
    }
}

/// What ProximityPrompts and ClickDetectors read of a frame's input
/// (Phase 44).
pub struct PromptInput {
    /// What the game holds, the UI's presses left out.
    pub game: InputState,
    /// The input map with the player's rebinds.
    pub map: InputMap,
    /// True when a controller was used last.
    pub gamepad: bool,
    /// The game's mouse presses and releases this frame.
    pub clicks: Vec<(UserInputType, bool)>,
    pub mouse: [f32; 2],
    /// The game's wheel notches this frame.
    pub wheel: f32,
}

/// This frame's [`PromptInput`], after [`fire_input`].
pub fn prompt_input(lua: &Lua) -> PromptInput {
    let map = input_map(lua);
    match lua.app_data_ref::<Inputs>() {
        Some(inputs) => PromptInput {
            game: inputs.game.clone(),
            map,
            gamepad: inputs.gamepad,
            clicks: inputs.clicks.clone(),
            mouse: inputs.all.mouse_position,
            wheel: inputs.wheel,
        },
        None => PromptInput {
            game: InputState::default(),
            map,
            gamepad: false,
            clicks: Vec::new(),
            mouse: [0.0, 0.0],
            wheel: 0.0,
        },
    }
}

/// The input map with the player's rebinds (the `Input/...` settings) applied.
pub fn input_map(lua: &Lua) -> InputMap {
    let Some(inputs) = lua.app_data_ref::<Inputs>() else {
        return InputMap::default();
    };
    let Ok(saves) = save::saves(lua) else {
        return inputs.map.clone();
    };
    let saves = saves.borrow();
    let rebinds = saves
        .settings
        .iter()
        .filter_map(|(name, value)| match value {
            Value::String(text) => Some((name.as_str(), text.as_str())),
            _ => None,
        });
    inputs.map.with_rebinds(rebinds)
}

/// Follows `events`, in order, and fires `UserInputService`'s events and
/// `ContextActionService`'s actions for each that changed something (a
/// key the system repeats while it's held isn't a new press). The mouse's
/// movement starts again from nothing for this frame.
pub fn fire_input(lua: &Lua, events: &[InputEvent]) {
    if let Some(mut inputs) = lua.app_data_mut::<Inputs>() {
        inputs.all.end_frame();
        inputs.game.end_frame();
        inputs.clicks.clear();
        inputs.wheel = 0.0;
    }
    for event in events {
        let Some((input, old_stick, game)) = follow(lua, event) else {
            continue;
        };
        // Actions only hear what the game does: not a press the UI took, nor its release.
        let processed = event.processed || !game;
        match event.kind {
            InputKind::PadConnected(pad) | InputKind::PadDisconnected(pad) => {
                let name = if matches!(event.kind, InputKind::PadConnected(_)) {
                    "GamepadConnected"
                } else {
                    "GamepadDisconnected"
                };
                if let Some(kind) = UserInputType::gamepad(pad) {
                    events::fire(lua, Owner::UserInputService, name, input_type(kind));
                }
                continue;
            }
            InputKind::PadStick { position, .. } => {
                let input = LuaInputObject {
                    delta: Vector3::new(
                        position[0] - old_stick[0],
                        position[1] - old_stick[1],
                        0.0,
                    ),
                    ..input
                };
                fire_one(lua, input, event.processed, processed);
                continue;
            }
            _ => {}
        }
        fire_one(lua, input, event.processed, processed);
    }
}

/// Fires `input`'s events: `InputBegan`, `InputChanged` or `InputEnded` (and
/// for a touch, `TouchStarted`, `TouchMoved` or `TouchEnded` first), with
/// `processed` as `gameProcessedEvent`, then the bound action that wants
/// it, unless it's `hidden` from actions.
fn fire_one(lua: &Lua, input: LuaInputObject, processed: bool, hidden: bool) {
    if input.kind == UserInputType::Touch {
        let touch = match input.state {
            UserInputState::Begin => "TouchStarted",
            UserInputState::Change => "TouchMoved",
            _ => "TouchEnded",
        };
        events::fire(lua, Owner::UserInputService, touch, (input, processed));
    }
    let event = match input.state {
        UserInputState::Begin => "InputBegan",
        UserInputState::Change => "InputChanged",
        _ => "InputEnded",
    };
    events::fire(lua, Owner::UserInputService, event, (input, processed));
    if !hidden {
        call_action(lua, input);
    }
}

/// Follows `event` in both states. The InputObject it makes, a stick's
/// position before it moved, and whether the game's state changed; `None`
/// if it changed nothing.
fn follow(lua: &Lua, event: &InputEvent) -> Option<(LuaInputObject, [f32; 2], bool)> {
    let mut inputs = lua.app_data_mut::<Inputs>()?;
    let old_stick = match event.kind {
        InputKind::PadStick { pad, stick, .. } => inputs
            .all
            .pad(pad)
            .map(|state| state.sticks[usize::from(stick == KeyCode::Thumbstick2)])
            .unwrap_or_default(),
        _ => [0.0, 0.0],
    };
    let game = inputs.game.follow(event);
    match event.kind {
        InputKind::PadButton { .. } => inputs.gamepad = true,
        InputKind::PadStick { position, .. }
            if position[0].hypot(position[1]) > axcenity_core::input::STICK_DEAD_ZONE =>
        {
            inputs.gamepad = true;
        }
        InputKind::Key { .. } | InputKind::MouseButton { .. } | InputKind::Wheel { .. } => {
            inputs.gamepad = false;
        }
        _ => {}
    }
    if game
        && let InputKind::MouseButton {
            button, pressed, ..
        } = event.kind
    {
        inputs.clicks.push((button, pressed));
    }
    if !event.processed
        && let InputKind::Wheel { notches, .. } = event.kind
    {
        inputs.wheel += notches;
    }
    if !inputs.all.apply(&event.kind) {
        return None;
    }
    let mouse = inputs.all.mouse_position;
    let at = |[x, y]: [f32; 2]| Vector3::new(x, y, 0.0);
    let begins = |pressed: bool| {
        if pressed {
            UserInputState::Begin
        } else {
            UserInputState::End
        }
    };
    let input = LuaInputObject {
        kind: UserInputType::None,
        state: UserInputState::Change,
        key: KeyCode::Unknown,
        position: at(mouse),
        delta: Vector3::new(0.0, 0.0, 0.0),
    };
    let input = match event.kind {
        InputKind::Key { key, pressed } => LuaInputObject {
            kind: UserInputType::Keyboard,
            state: begins(pressed),
            key,
            ..input
        },
        InputKind::MouseButton {
            button, pressed, ..
        } => LuaInputObject {
            kind: button,
            state: begins(pressed),
            ..input
        },
        InputKind::MouseMove { delta, .. } => LuaInputObject {
            kind: UserInputType::MouseMovement,
            delta: at(delta),
            ..input
        },
        // Roblox puts the wheel's notches in Position.Z.
        InputKind::Wheel { position, notches } => LuaInputObject {
            kind: UserInputType::MouseWheel,
            position: Vector3::new(position[0], position[1], notches),
            ..input
        },
        InputKind::Touch {
            state, position, ..
        } => LuaInputObject {
            kind: UserInputType::Touch,
            state,
            position: at(position),
            ..input
        },
        InputKind::PadButton { pad, key, pressed } => LuaInputObject {
            kind: UserInputType::gamepad(pad).unwrap_or(UserInputType::None),
            state: begins(pressed),
            key,
            ..input
        },
        InputKind::PadStick {
            pad,
            stick,
            position,
        } => LuaInputObject {
            kind: UserInputType::gamepad(pad).unwrap_or(UserInputType::None),
            key: stick,
            position: at(position),
            ..input
        },
        InputKind::PadConnected(_) | InputKind::PadDisconnected(_) => input,
    };
    Some((input, old_stick, game))
}

/// Calls the highest-priority bound action that wants `input` (the latest
/// bound, of equal ones) with `(actionName, inputState, inputObject)`.
fn call_action(lua: &Lua, input: LuaInputObject) {
    let chosen = lua.app_data_ref::<Inputs>().and_then(|inputs| {
        inputs
            .bound
            .iter()
            .filter(|action| action.wants(&input))
            .max_by(|a, b| {
                a.priority
                    .total_cmp(&b.priority)
                    .then(a.order.cmp(&b.order))
            })
            .map(|action| (action.name.clone(), action.handler.clone(), action.script))
    });
    let Some((name, handler, script)) = chosen else {
        return;
    };
    let state = LuaEnumItem::find(EnumType::UserInputState, input.state.as_str());
    crate::handler_threads::run(lua, handler, (name, state, input), script);
}

fn input_type(kind: UserInputType) -> Option<LuaEnumItem> {
    LuaEnumItem::find(EnumType::UserInputType, kind.as_str())
}

/// The item of `enum_type` that `value` is, or an error saying what
/// `method` takes.
fn enum_arg(
    lua: &Lua,
    value: &mlua::Value,
    enum_type: EnumType,
    method: &str,
    example: &str,
) -> mlua::Result<LuaEnumItem> {
    LuaEnumItem::from_lua(value.clone(), lua)
        .ok()
        .filter(|item| item.enum_type == enum_type)
        .ok_or_else(|| {
            mlua::Error::runtime(format!(
                "{method} takes an Enum.{}, such as {example}, not a {}",
                enum_type.name(),
                type_name(lua, value)
            ))
        })
}

/// Where the mouse is, in pixels from the view's top left.
pub(crate) fn mouse_position(lua: &Lua) -> mlua::Result<[f32; 2]> {
    Ok(inputs_ref(lua)?.all.mouse_position)
}

fn inputs_ref(lua: &Lua) -> mlua::Result<mlua::AppDataRef<'_, Inputs>> {
    lua.app_data_ref::<Inputs>()
        .ok_or_else(|| mlua::Error::runtime("input isn't available here"))
}

fn inputs_mut(lua: &Lua) -> mlua::Result<mlua::AppDataRefMut<'_, Inputs>> {
    lua.app_data_mut::<Inputs>()
        .ok_or_else(|| mlua::Error::runtime("input isn't available here"))
}

/// The name of an action the input map has, for `method`.
fn action_arg(lua: &Lua, value: &mlua::Value, method: &str) -> mlua::Result<(String, InputMap)> {
    let mlua::Value::String(name) = value else {
        return Err(mlua::Error::runtime(format!(
            "{method} takes an action's name, such as \"Jump\", not a {}",
            type_name(lua, value)
        )));
    };
    let name = name.to_string_lossy();
    let map = input_map(lua);
    if map.action(&name).is_none() {
        // `rebind` words the error, naming the actions there are.
        let err = map.clone().rebind(&name, Vec::new()).unwrap_err();
        return Err(mlua::Error::runtime(format!("{method}: {err}")));
    }
    Ok((name, map))
}

/// One input given to `RebindAction`: an `Enum.KeyCode` item, a mouse
/// button or `MouseMovement` from `Enum.UserInputType`, or an input's
/// name as `input.json` writes it (`"W,A,S,D"`).
fn rebind_input(lua: &Lua, value: &mlua::Value) -> mlua::Result<Input> {
    let found = match value {
        mlua::Value::String(name) => Input::parse(&name.to_string_lossy()),
        other => LuaEnumItem::from_lua(other.clone(), lua)
            .ok()
            .and_then(|item| match item.enum_type {
                EnumType::KeyCode => item.as_key().map(Input::Key),
                EnumType::UserInputType => match UserInputType::from_name(item.name)? {
                    UserInputType::MouseMovement => Some(Input::MouseMovement),
                    button if button.mouse_button().is_some() => Some(Input::Mouse(button)),
                    _ => None,
                },
                _ => None,
            }),
    };
    found.ok_or_else(|| {
        mlua::Error::runtime(format!(
            "RebindAction takes keys and buttons, such as {{Enum.KeyCode.E, Enum.KeyCode.ButtonA}}, \
             mouse buttons (Enum.UserInputType.MouseButton1) or an axis such as \"W,A,S,D\"; \
             {} isn't one",
            match value {
                mlua::Value::String(name) => format!("\"{}\"", name.to_string_lossy()),
                other => format!("a {}", type_name(lua, other)),
            }
        ))
    })
}

/// `game:GetService("UserInputService")`: the keyboard, mouse, touch screen
/// and game controllers, and the input map's actions.
#[derive(Clone, Copy)]
pub struct LuaUserInputService;

impl UserData for LuaUserInputService {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        for event in [
            "InputBegan",
            "InputChanged",
            "InputEnded",
            "TouchStarted",
            "TouchMoved",
            "TouchEnded",
            "GamepadConnected",
            "GamepadDisconnected",
        ] {
            fields.add_field_method_get(event, move |_, _| {
                Ok(LuaSignal::named(Owner::UserInputService, event))
            });
        }
        fields.add_field_method_get("MouseBehavior", |lua, _| {
            let behavior = inputs_ref(lua)?.mouse_behavior;
            Ok(LuaEnumItem::find(
                EnumType::MouseBehavior,
                behavior.as_str(),
            ))
        });
        fields.add_field_method_set("MouseBehavior", |lua, _, value: mlua::Value| {
            let item = enum_arg(
                lua,
                &value,
                EnumType::MouseBehavior,
                "MouseBehavior",
                "Enum.MouseBehavior.LockCenter",
            )?;
            inputs_mut(lua)?.mouse_behavior =
                MouseBehavior::from_name(item.name).unwrap_or_default();
            Ok(())
        });
        fields.add_field_method_get("MouseIconEnabled", |lua, _| {
            Ok(inputs_ref(lua)?.mouse_icon_enabled)
        });
        fields.add_field_method_set("MouseIconEnabled", |lua, _, value: mlua::Value| {
            let mlua::Value::Boolean(shown) = value else {
                return Err(mlua::Error::runtime(format!(
                    "MouseIconEnabled must be true or false, not a {}",
                    type_name(lua, &value)
                )));
            };
            inputs_mut(lua)?.mouse_icon_enabled = shown;
            Ok(())
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("IsKeyDown", |lua, _, key: mlua::Value| {
            let key = enum_arg(lua, &key, EnumType::KeyCode, "IsKeyDown", "Enum.KeyCode.W")?;
            let key = key.as_key().unwrap_or(KeyCode::Unknown);
            Ok(inputs_ref(lua)?.all.is_key_down(key))
        });
        methods.add_method("IsMouseButtonPressed", |lua, _, button: mlua::Value| {
            let button = enum_arg(
                lua,
                &button,
                EnumType::UserInputType,
                "IsMouseButtonPressed",
                "Enum.UserInputType.MouseButton1",
            )?;
            let button = UserInputType::from_name(button.name).unwrap_or(UserInputType::None);
            Ok(inputs_ref(lua)?.all.is_mouse_down(button))
        });
        methods.add_method("GetMouseLocation", |lua, _, ()| {
            let [x, y] = inputs_ref(lua)?.all.mouse_position;
            Ok(LuaVector2(Vector2::new(x, y)))
        });
        methods.add_method("GetMouseDelta", |lua, _, ()| {
            let [x, y] = inputs_ref(lua)?.all.mouse_delta;
            Ok(LuaVector2(Vector2::new(x, y)))
        });
        methods.add_method("GetConnectedGamepads", |lua, _, ()| {
            let inputs = inputs_ref(lua)?;
            let pads: Vec<LuaEnumItem> = (0..4)
                .filter(|pad| inputs.all.pad(*pad).is_some())
                .filter_map(UserInputType::gamepad)
                .filter_map(input_type)
                .collect();
            Ok(pads)
        });
        methods.add_method(
            "IsGamepadButtonDown",
            |lua, _, (pad, key): (mlua::Value, mlua::Value)| {
                let pad = gamepad_arg(lua, &pad, "IsGamepadButtonDown")?;
                let key = enum_arg(
                    lua,
                    &key,
                    EnumType::KeyCode,
                    "IsGamepadButtonDown",
                    "Enum.KeyCode.ButtonA",
                )?;
                let key = key.as_key().unwrap_or(KeyCode::Unknown);
                Ok(inputs_ref(lua)?
                    .all
                    .pad(pad)
                    .is_some_and(|state| state.buttons.contains(&key)))
            },
        );
        methods.add_method("GetGamepadState", |lua, _, pad: mlua::Value| {
            let pad = gamepad_arg(lua, &pad, "GetGamepadState")?;
            let inputs = inputs_ref(lua)?;
            let Some(state) = inputs.all.pad(pad) else {
                return Ok(Vec::new());
            };
            let kind = UserInputType::gamepad(pad).unwrap_or(UserInputType::None);
            let objects = KeyCode::ALL
                .into_iter()
                .filter(|key| key.is_gamepad())
                .map(|key| {
                    let held = state.buttons.contains(&key);
                    let [x, y] = match key {
                        KeyCode::Thumbstick1 => state.sticks[0],
                        KeyCode::Thumbstick2 => state.sticks[1],
                        _ => [0.0, 0.0],
                    };
                    LuaInputObject {
                        kind,
                        state: if held || x != 0.0 || y != 0.0 {
                            UserInputState::Begin
                        } else {
                            UserInputState::End
                        },
                        key,
                        position: Vector3::new(x, y, 0.0),
                        delta: Vector3::new(0.0, 0.0, 0.0),
                    }
                })
                .collect::<Vec<_>>();
            Ok(objects)
        });
        methods.add_method("IsActionPressed", |lua, _, name: mlua::Value| {
            let (name, map) = action_arg(lua, &name, "IsActionPressed")?;
            Ok(map.is_pressed(&name, &inputs_ref(lua)?.game))
        });
        methods.add_method("GetActionAxis", |lua, _, name: mlua::Value| {
            let (name, map) = action_arg(lua, &name, "GetActionAxis")?;
            let [x, y] = map.axis(&name, &inputs_ref(lua)?.game);
            Ok(LuaVector2(Vector2::new(x, y)))
        });
        methods.add_method("GetActionInputs", |lua, _, name: mlua::Value| {
            let (name, map) = action_arg(lua, &name, "GetActionInputs")?;
            let inputs = &map.action(&name).expect("checked").inputs;
            Ok(inputs.iter().map(Input::name).collect::<Vec<_>>())
        });
        methods.add_method(
            "RebindAction",
            |lua, _, (name, inputs): (mlua::Value, mlua::Value)| {
                let (name, _) = action_arg(lua, &name, "RebindAction")?;
                let values: Vec<mlua::Value> = match &inputs {
                    mlua::Value::Table(table) => {
                        table.sequence_values().collect::<Result<_, _>>()?
                    }
                    single => vec![single.clone()],
                };
                let inputs = values
                    .iter()
                    .map(|value| rebind_input(lua, value))
                    .collect::<mlua::Result<Vec<_>>>()?;
                save::saves(lua)?.borrow_mut().settings.insert(
                    format!("{REBIND_PREFIX}{name}"),
                    Value::String(binding_text(&inputs)),
                );
                Ok(())
            },
        );
        methods.add_meta_method(MetaMethod::ToString, |_, _, ()| Ok("UserInputService"));
    }
}

/// Which controller `value` (`Enum.UserInputType.Gamepad1` to `Gamepad4`) is.
fn gamepad_arg(lua: &Lua, value: &mlua::Value, method: &str) -> mlua::Result<u8> {
    let item = enum_arg(
        lua,
        value,
        EnumType::UserInputType,
        method,
        "Enum.UserInputType.Gamepad1",
    )?;
    UserInputType::from_name(item.name)
        .and_then(UserInputType::pad)
        .ok_or_else(|| {
            mlua::Error::runtime(format!(
                "{method} takes a controller, Enum.UserInputType.Gamepad1 to Gamepad4, not Enum.UserInputType.{}",
                item.name
            ))
        })
}

/// What input handlers get: which device and key, where, and how far it moved.
#[derive(Clone, Copy, Debug)]
pub struct LuaInputObject {
    kind: UserInputType,
    state: UserInputState,
    key: KeyCode,
    position: Vector3,
    delta: Vector3,
}

impl UserData for LuaInputObject {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("KeyCode", |_, input| Ok(LuaEnumItem::key(input.key)));
        fields.add_field_method_get("UserInputType", |_, input| Ok(input_type(input.kind)));
        fields.add_field_method_get("UserInputState", |_, input| {
            Ok(LuaEnumItem::find(
                EnumType::UserInputState,
                input.state.as_str(),
            ))
        });
        fields.add_field_method_get("Position", |_, input| Ok(LuaVector3(input.position)));
        fields.add_field_method_get("Delta", |_, input| Ok(LuaVector3(input.delta)));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::ToString, |_, _, ()| Ok("InputObject"));
    }
}

/// `game:GetService("ContextActionService")`: functions bound to inputs.
/// When a bound input comes, only the highest-priority action bound to it
/// is called, so a menu can bind Escape above the game's own use of it
/// until it closes.
#[derive(Clone, Copy)]
pub struct LuaContextActionService;

impl LuaContextActionService {
    /// Binds `name` (replacing an action bound with that name before).
    fn bind(
        lua: &Lua,
        method: &str,
        (name, handler, touch_button): (mlua::Value, mlua::Value, mlua::Value),
        priority: f64,
        inputs: Variadic<mlua::Value>,
    ) -> mlua::Result<()> {
        let mlua::Value::String(name) = name else {
            return Err(mlua::Error::runtime(format!(
                "{method} takes the action's name first, such as \"Reload\", not a {}",
                type_name(lua, &name)
            )));
        };
        let mlua::Value::Function(handler) = handler else {
            return Err(mlua::Error::runtime(format!(
                "{method} takes a function second: function(actionName, inputState, inputObject) ... end, not a {}",
                type_name(lua, &handler)
            )));
        };
        let items = inputs
            .iter()
            .map(|value| {
                LuaEnumItem::from_lua(value.clone(), lua)
                    .ok()
                    .filter(|item| {
                        matches!(item.enum_type, EnumType::KeyCode | EnumType::UserInputType)
                    })
                    .ok_or_else(|| {
                        mlua::Error::runtime(format!(
                            "{method} binds Enum.KeyCode or Enum.UserInputType items, such as \
                             Enum.KeyCode.R, not a {}",
                            type_name(lua, value)
                        ))
                    })
            })
            .collect::<mlua::Result<Vec<_>>>()?;
        let script = scheduler::running_script(lua);
        let mut inputs = inputs_mut(lua)?;
        let name = name.to_string_lossy();
        inputs.bound.retain(|action| action.name != name);
        let order = inputs.next_order;
        inputs.next_order += 1;
        inputs.bound.push(BoundAction {
            name,
            handler,
            script,
            priority,
            touch_button: touch_button.as_boolean().unwrap_or(false),
            inputs: items,
            order,
        });
        Ok(())
    }
}

impl UserData for LuaContextActionService {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method(
            "BindAction",
            |lua,
             _,
             (name, handler, touch, inputs): (
                mlua::Value,
                mlua::Value,
                mlua::Value,
                Variadic<mlua::Value>,
            )| {
                Self::bind(
                    lua,
                    "BindAction",
                    (name, handler, touch),
                    DEFAULT_PRIORITY,
                    inputs,
                )
            },
        );
        methods.add_method(
            "BindActionAtPriority",
            |lua,
             _,
             (name, handler, touch, priority, inputs): (
                mlua::Value,
                mlua::Value,
                mlua::Value,
                mlua::Value,
                Variadic<mlua::Value>,
            )| {
                let priority = match priority {
                    mlua::Value::Integer(n) => n as f64,
                    mlua::Value::Number(n) => n,
                    other => {
                        return Err(mlua::Error::runtime(format!(
                            "BindActionAtPriority takes the priority fourth, a number such as 3000, not a {}",
                            type_name(lua, &other)
                        )));
                    }
                };
                Self::bind(lua, "BindActionAtPriority", (name, handler, touch), priority, inputs)
            },
        );
        methods.add_method("UnbindAction", |lua, _, name: String| {
            inputs_mut(lua)?.bound.retain(|action| action.name != name);
            Ok(())
        });
        methods.add_method("UnbindAllActions", |lua, _, ()| {
            inputs_mut(lua)?.bound.clear();
            Ok(())
        });
        methods.add_method("GetBoundActionInfo", |lua, _, name: String| {
            let table = lua.create_table()?;
            let inputs = inputs_ref(lua)?;
            let Some(action) = inputs.bound.iter().find(|action| action.name == name) else {
                return Ok(table);
            };
            // Its place among the bound actions, latest highest.
            let stack_order = inputs
                .bound
                .iter()
                .filter(|other| other.order < action.order)
                .count();
            table.set("priorityLevel", action.priority)?;
            table.set("createTouchButton", action.touch_button)?;
            table.set("stackOrder", stack_order)?;
            table.set("inputTypes", action.inputs.clone())?;
            Ok(table)
        });
        methods.add_meta_method(MetaMethod::ToString, |_, _, ()| Ok("ContextActionService"));
    }
}
