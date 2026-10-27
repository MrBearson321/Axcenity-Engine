//! What only LocalScripts can reach, because it belongs to the player's
//! device: `Players.LocalPlayer` (and its `GetMouse`) and the loading
//! screen (`ReplicatedFirst:RemoveDefaultLoadingScreen`). The camera
//! (`workspace.CurrentCamera`) is `camera.rs`; input (`UserInputService`,
//! `ContextActionService`) is `input.rs`. Everything here lives in the
//! client interpreter's app data.

use axcenity_core::{ClassName, InstanceId};
use mlua::{Function, Lua, MetaMethod, UserData, UserDataMethods};

use crate::camera::LuaMouse;
use crate::instance::LuaInstance;

/// The Player of whoever plays on this device.
#[derive(Clone, Copy)]
pub struct LocalPlayer(pub InstanceId);

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

/// `player:GetMouse()`, on the player's side: the mouse in the world.
pub fn player_methods(lua: &Lua) -> mlua::Result<Vec<(ClassName, &'static str, Function)>> {
    let get_mouse = lua.create_function(|lua, _: LuaInstance| {
        crate::sides::check(lua, "GetMouse")?;
        Ok(LuaMouse)
    })?;
    Ok(vec![(ClassName::Player, "GetMouse", get_mouse)])
}
