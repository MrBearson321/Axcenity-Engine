//! `game:GetService("RunService")`, with `Heartbeat`: an event that fires
//! once every frame, after physics. The host fires it with
//! [`ScriptRuntime::heartbeat`](crate::ScriptRuntime::heartbeat).

use mlua::{MetaMethod, UserData, UserDataFields, UserDataMethods};

use crate::events::{LuaSignal, Owner};

/// The RunService, as scripts see it. Both sides have one.
#[derive(Clone, Copy)]
pub struct LuaRunService;

impl UserData for LuaRunService {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("Heartbeat", |_, _| {
            Ok(LuaSignal::named(Owner::RunService, "Heartbeat"))
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::ToString, |_, _, ()| Ok("RunService"));
    }
}
