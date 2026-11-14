//! `game:GetService("RunService")`: the frame loop as scripts see it.
//!
//! - `Stepped` fires every frame before physics, with the game time and
//!   the frame's length; `Heartbeat` after physics, with the frame's length.
//! - `RenderStepped` (LocalScripts only) fires every frame just before it's
//!   drawn, after the functions bound with `BindToRenderStep`, which run
//!   in priority order, lowest first.
//! - `IsServer`, `IsClient`, `IsStudio` and `IsRunning` say where the code
//!   runs.
//!
//! The host fires them with
//! [`ScriptRuntime::stepped`](crate::ScriptRuntime::stepped),
//! [`ScriptRuntime::heartbeat`](crate::ScriptRuntime::heartbeat) and
//! [`ScriptRuntime::render_step`](crate::ScriptRuntime::render_step).

use axcenity_core::InstanceId;
use mlua::{Function, Lua, MetaMethod, UserData, UserDataFields, UserDataMethods, Value};

use crate::events::{self, LuaSignal, Owner};
use crate::scheduler;
use crate::sides::{self, Side};
use crate::types::{EnumType, LuaEnumItem, type_name};

/// The RunService, as scripts see it. Both sides have one.
#[derive(Clone, Copy)]
pub struct LuaRunService;

/// Where the game runs, for `IsStudio` and `IsRunning`. Lives in Lua's app data.
#[derive(Clone, Copy)]
pub struct RunFlags {
    /// The editor's Play, rather than the player.
    pub studio: bool,
    /// False only for the editor's Console outside Play.
    pub running: bool,
}

impl Default for RunFlags {
    fn default() -> Self {
        Self {
            studio: false,
            running: true,
        }
    }
}

/// The functions bound with `BindToRenderStep`, on the client.
#[derive(Default)]
pub struct RenderSteps {
    /// Counts up, so functions with the same priority run in the order bound.
    next_order: u64,
    bound: Vec<Bound>,
}

struct Bound {
    name: String,
    priority: f64,
    order: u64,
    callback: Function,
    /// The script that bound it (see `scheduler`).
    script: Option<InstanceId>,
}

impl RenderSteps {
    /// Unbinds what `script` bound.
    pub fn forget_script(&mut self, script: InstanceId) {
        self.bound.retain(|bound| bound.script != Some(script));
    }
}

impl UserData for LuaRunService {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("Heartbeat", |_, _| {
            Ok(LuaSignal::named(Owner::RunService, "Heartbeat"))
        });
        fields.add_field_method_get("Stepped", |_, _| {
            Ok(LuaSignal::named(Owner::RunService, "Stepped"))
        });
        fields.add_field_method_get("RenderStepped", |lua, _| {
            sides::check(lua, "RenderStepped")?;
            Ok(LuaSignal::named(Owner::RunService, "RenderStepped"))
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method(
            "BindToRenderStep",
            |lua, _, (name, priority, callback): (String, Value, Value)| {
                sides::check(lua, "BindToRenderStep")?;
                let priority = priority_arg(lua, priority)?;
                let Value::Function(callback) = callback else {
                    return Err(mlua::Error::runtime(format!(
                        "BindToRenderStep takes a function to run, not a {}",
                        type_name(lua, &callback)
                    )));
                };
                let script = scheduler::running_script(lua);
                let mut steps = render_steps_mut(lua);
                // Binding a name again replaces what was bound to it.
                steps.bound.retain(|bound| bound.name != name);
                let order = steps.next_order;
                steps.next_order += 1;
                steps.bound.push(Bound {
                    name,
                    priority,
                    order,
                    callback,
                    script,
                });
                Ok(())
            },
        );
        methods.add_method("UnbindFromRenderStep", |lua, _, name: String| {
            sides::check(lua, "UnbindFromRenderStep")?;
            render_steps_mut(lua)
                .bound
                .retain(|bound| bound.name != name);
            Ok(())
        });
        methods.add_method("IsServer", |lua, _, ()| Ok(side(lua) == Some(Side::Server)));
        methods.add_method("IsClient", |lua, _, ()| Ok(side(lua) == Some(Side::Client)));
        methods.add_method("IsStudio", |lua, _, ()| Ok(flags(lua).studio));
        methods.add_method("IsRunning", |lua, _, ()| Ok(flags(lua).running));
        methods.add_meta_method(MetaMethod::ToString, |_, _, ()| Ok("RunService"));
    }
}

fn side(lua: &Lua) -> Option<Side> {
    lua.app_data_ref::<Side>().map(|side| *side)
}

fn flags(lua: &Lua) -> RunFlags {
    lua.app_data_ref::<RunFlags>()
        .map(|flags| *flags)
        .unwrap_or_default()
}

fn render_steps_mut(lua: &Lua) -> mlua::AppDataRefMut<'_, RenderSteps> {
    if lua.app_data_ref::<RenderSteps>().is_none() {
        lua.set_app_data(RenderSteps::default());
    }
    lua.app_data_mut::<RenderSteps>().expect("made just above")
}

/// A priority: a number, or an `Enum.RenderPriority` item (its `Value`).
fn priority_arg(lua: &Lua, value: Value) -> mlua::Result<f64> {
    match &value {
        Value::Integer(n) => return Ok(*n as f64),
        Value::Number(n) if n.is_finite() => return Ok(*n),
        Value::UserData(data) => {
            if let Ok(item) = data.borrow::<LuaEnumItem>()
                && item.enum_type == EnumType::RenderPriority
            {
                return Ok(f64::from(item.value()));
            }
        }
        _ => {}
    }
    Err(mlua::Error::runtime(format!(
        "BindToRenderStep's priority must be a number, such as \
         Enum.RenderPriority.Camera.Value + 1, not a {}",
        type_name(lua, &value)
    )))
}

/// Fires `RunService.Stepped` with the game time and the frame's length.
pub fn fire_stepped(lua: &Lua, seconds: f64) {
    let time = scheduler::now(lua);
    events::fire(lua, Owner::RunService, "Stepped", (time, seconds));
}

/// Runs the bound functions in priority order (lowest first; the same
/// priority in the order bound), each as its own coroutine, then fires
/// `RenderStepped`. All get the frame's length in seconds.
pub fn render_step(lua: &Lua, seconds: f64) {
    let mut bound: Vec<(f64, u64, Function, Option<InstanceId>, String)> = lua
        .app_data_ref::<RenderSteps>()
        .map(|steps| {
            steps
                .bound
                .iter()
                .map(|b| {
                    let name = b.name.clone();
                    (b.priority, b.order, b.callback.clone(), b.script, name)
                })
                .collect()
        })
        .unwrap_or_default();
    bound.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    for (_, order, callback, script, name) in bound {
        // An earlier function may have unbound this one.
        let still_bound = lua
            .app_data_ref::<RenderSteps>()
            .is_some_and(|steps| steps.bound.iter().any(|b| b.order == order));
        if !still_bound {
            continue;
        }
        let event = || format!("BindToRenderStep \"{name}\"");
        crate::handler_threads::run(lua, callback, seconds, script, event);
    }
    events::fire(lua, Owner::RunService, "RenderStepped", seconds);
}
