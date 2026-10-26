//! What only LocalScripts can reach, because it belongs to the player's
//! device: the keyboard (`UserInputService`), the camera
//! (`workspace.CurrentCamera`), `Players.LocalPlayer` and the loading
//! screen (`ReplicatedFirst:RemoveDefaultLoadingScreen`).
//!
//! The host feeds key presses in with `ScriptRuntime::input`, and keeps
//! the camera in step with `set_camera` / `take_camera_move`. Everything
//! here lives in the client interpreter's app data.

use std::collections::BTreeSet;

use axcenity_core::{InstanceId, KeyCode, Vector3};
use mlua::{FromLua, Lua, MetaMethod, UserData, UserDataFields, UserDataMethods};

use crate::events::{self, LuaSignal, Owner};
use crate::types::{LuaEnumItem, LuaVector3, type_name};

/// A key going down or coming back up, as the host reports it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyEvent {
    pub key: KeyCode,
    pub pressed: bool,
}

/// The keys held down right now.
#[derive(Default)]
pub struct HeldKeys(BTreeSet<KeyCode>);

/// The Player of whoever plays on this device.
#[derive(Clone, Copy)]
pub struct LocalPlayer(pub InstanceId);

/// Where the camera is and which way it looks, in the same terms as a
/// Part: `orientation` is degrees around X, Y and Z (only X, looking up and
/// down, and Y, turning, change the view).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraState {
    pub position: Vector3,
    pub orientation: Vector3,
}

impl Default for CameraState {
    fn default() -> Self {
        Self {
            position: Vector3::new(0.0, 0.0, 0.0),
            orientation: Vector3::new(0.0, 0.0, 0.0),
        }
    }
}

/// The camera as scripts last saw or set it.
#[derive(Default)]
pub struct Camera {
    pub state: CameraState,
    /// True once a script has set Position or Orientation since the host
    /// last took the change.
    pub moved: bool,
}

/// Updates the held keys from `events`, then fires `InputBegan` or
/// `InputEnded` for each. A press of a key already held (the OS repeating
/// it) and a release of one that isn't held do nothing.
pub fn fire_input(lua: &Lua, events: &[KeyEvent]) {
    for &KeyEvent { key, pressed } in events {
        let changed = match lua.app_data_mut::<HeldKeys>() {
            Some(mut held) if pressed => held.0.insert(key),
            Some(mut held) => held.0.remove(&key),
            None => false,
        };
        if !changed {
            continue;
        }
        let event = if pressed { "InputBegan" } else { "InputEnded" };
        // The second argument is Roblox's `gameProcessedEvent`: true when the
        // game's own UI used the key. There's no UI yet, so always false.
        events::fire(
            lua,
            Owner::UserInputService,
            event,
            (LuaInputObject { key }, false),
        );
    }
}

/// Set when a LocalScript calls `RemoveDefaultLoadingScreen`, until the
/// host takes it with `ScriptRuntime::take_loading_screen_removed`.
#[derive(Default)]
pub struct LoadingScreenRemoved(pub bool);

/// `game:GetService("ReplicatedFirst")`. Its `RemoveDefaultLoadingScreen`
/// says the game is ready, and the host takes the loading screen down once
/// its minimum time is up.
#[derive(Clone, Copy)]
pub struct LuaReplicatedFirst;

impl UserData for LuaReplicatedFirst {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("RemoveDefaultLoadingScreen", |lua, _, ()| {
            crate::sides::check(lua, "RemoveDefaultLoadingScreen")?;
            if let Some(mut removed) = lua.app_data_mut::<LoadingScreenRemoved>() {
                removed.0 = true;
            }
            Ok(())
        });
        methods.add_meta_method(MetaMethod::ToString, |_, _, ()| Ok("ReplicatedFirst"));
    }
}

/// `game:GetService("UserInputService")`: the keyboard.
#[derive(Clone, Copy)]
pub struct LuaUserInputService;

impl UserData for LuaUserInputService {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        for event in ["InputBegan", "InputEnded"] {
            fields.add_field_method_get(event, move |_, _| {
                Ok(LuaSignal::named(Owner::UserInputService, event))
            });
        }
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("IsKeyDown", |lua, _, key: mlua::Value| {
            let key = key_arg(lua, key)?;
            Ok(lua
                .app_data_ref::<HeldKeys>()
                .is_some_and(|held| held.0.contains(&key)))
        });
        methods.add_meta_method(MetaMethod::ToString, |_, _, ()| Ok("UserInputService"));
    }
}

/// The key `IsKeyDown` was asked about, which must be an `Enum.KeyCode` item.
fn key_arg(lua: &Lua, value: mlua::Value) -> mlua::Result<KeyCode> {
    LuaEnumItem::from_lua(value.clone(), lua)
        .ok()
        .and_then(LuaEnumItem::as_key)
        .ok_or_else(|| {
            mlua::Error::runtime(format!(
                "IsKeyDown takes an Enum.KeyCode, such as Enum.KeyCode.W, not a {}",
                type_name(lua, &value)
            ))
        })
}

/// What `InputBegan` and `InputEnded` handlers get: which key it was.
#[derive(Clone, Copy)]
pub struct LuaInputObject {
    key: KeyCode,
}

impl UserData for LuaInputObject {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("KeyCode", |_, input| Ok(LuaEnumItem::key(input.key)));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::ToString, |_, _, ()| Ok("InputObject"));
    }
}

/// `workspace.CurrentCamera`. Holds nothing itself: the state is the
/// `Camera` in app data, so every read sees the latest values.
#[derive(Clone, Copy)]
pub struct LuaCamera;

impl LuaCamera {
    fn state(lua: &Lua) -> CameraState {
        lua.app_data_ref::<Camera>()
            .map(|camera| camera.state)
            .unwrap_or_default()
    }

    /// Sets one of the camera's Vector3 fields, and marks the camera moved.
    fn set(
        lua: &Lua,
        field: &str,
        value: mlua::Value,
        apply: impl FnOnce(&mut CameraState, Vector3),
    ) -> mlua::Result<()> {
        let vector = LuaVector3::from_lua(value.clone(), lua).map_err(|_| {
            mlua::Error::runtime(format!(
                "Camera.{field} must be a Vector3, not a {}",
                type_name(lua, &value)
            ))
        })?;
        if let Some(mut camera) = lua.app_data_mut::<Camera>() {
            apply(&mut camera.state, vector.0);
            camera.moved = true;
        }
        Ok(())
    }
}

/// The direction a camera with this orientation looks in, one stud long.
/// Turning around Y by `yaw` and then tilting up by `pitch` takes the
/// starting direction (straight ahead, -Z) here; rolling around Z doesn't
/// change where it points.
pub fn look_vector(orientation: Vector3) -> Vector3 {
    let pitch = orientation.x.to_radians();
    let yaw = orientation.y.to_radians();
    Vector3::new(
        -yaw.sin() * pitch.cos(),
        pitch.sin(),
        -yaw.cos() * pitch.cos(),
    )
}

impl UserData for LuaCamera {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("Position", |lua, _| {
            Ok(LuaVector3(Self::state(lua).position))
        });
        fields.add_field_method_set("Position", |lua, _, value: mlua::Value| {
            Self::set(lua, "Position", value, |state, v| state.position = v)
        });
        fields.add_field_method_get("Orientation", |lua, _| {
            Ok(LuaVector3(Self::state(lua).orientation))
        });
        fields.add_field_method_set("Orientation", |lua, _, value: mlua::Value| {
            Self::set(lua, "Orientation", value, |state, v| state.orientation = v)
        });
        fields.add_field_method_get("LookVector", |lua, _| {
            Ok(LuaVector3(look_vector(Self::state(lua).orientation)))
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::ToString, |_, _, ()| Ok("Camera"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Vector3, b: Vector3) -> bool {
        (a.x - b.x).abs() < 1e-5 && (a.y - b.y).abs() < 1e-5 && (a.z - b.z).abs() < 1e-5
    }

    #[test]
    fn look_vector_turns_with_yaw_and_tilts_with_pitch() {
        let ahead = look_vector(Vector3::new(0.0, 0.0, 0.0));
        assert!(close(ahead, Vector3::new(0.0, 0.0, -1.0)), "{ahead:?}");
        // Turning left (positive Y) by 90 degrees looks along -X.
        let left = look_vector(Vector3::new(0.0, 90.0, 0.0));
        assert!(close(left, Vector3::new(-1.0, 0.0, 0.0)), "{left:?}");
        let up = look_vector(Vector3::new(90.0, 0.0, 0.0));
        assert!(close(up, Vector3::new(0.0, 1.0, 0.0)), "{up:?}");
        // Roll doesn't change where it looks.
        let rolled = look_vector(Vector3::new(0.0, 0.0, 45.0));
        assert!(close(rolled, ahead), "{rolled:?}");
    }
}
