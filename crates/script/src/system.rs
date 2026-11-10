//! Game systems from other crates (roadmap L7). A [`GameSystem`] adds
//! services to `game:GetService` (and `game.<Name>`) and steps once per
//! frame on each side, without `script` knowing it: the `gameplay` crate
//! holds them, and hosts add them with [`crate::ScriptRuntime::add_system`].
//!
//! The functions here are the part of the runtime a system may use: the
//! game clock, destroying an Instance as `Destroy` does, reporting an error
//! to the output, and naming a value's type in a message. A system keeps
//! its own state in the interpreter's app data (`Lua::set_app_data`), one
//! copy per side.

use std::rc::Rc;

use axcenity_core::InstanceId;
use mlua::Lua;

/// A part of the game that lives outside `script`: its services and its
/// per-frame step. One value serves both sides; per-side state goes in each
/// interpreter's app data.
pub trait GameSystem: 'static {
    /// The names of the services it adds, such as `"Debris"`.
    fn services(&self) -> &'static [&'static str];

    /// The service `name` (one of [`GameSystem::services`]) as the scripts
    /// on `lua` see it.
    fn service(&self, lua: &Lua, name: &str) -> mlua::Result<mlua::Value>;

    /// Runs once per frame on each side, after scripts have resumed, with
    /// the World lent (`crate::lent_world_mut`). Systems step in the order
    /// they were added.
    fn step(&self, _lua: &Lua) {}
}

/// Services the API definitions (`definitions/tables.rs`) describe that
/// game systems add rather than `script`: a runtime made with
/// `ScriptRuntime::new` lacks them, so `script`'s definition tests skip
/// them and the `gameplay` crate's tests check them instead.
#[doc(hidden)]
pub const DESCRIBED_SYSTEM_SERVICES: &[&str] = &["Debris"];

/// The systems added to one interpreter, in the order they were added.
#[derive(Default, Clone)]
pub(crate) struct Systems(Vec<Rc<dyn GameSystem>>);

/// Adds `system` to the interpreter `lua`.
pub(crate) fn add(lua: &Lua, system: Rc<dyn GameSystem>) {
    if lua.app_data_ref::<Systems>().is_none() {
        lua.set_app_data(Systems::default());
    }
    if let Some(mut systems) = lua.app_data_mut::<Systems>() {
        systems.0.push(system);
    }
}

/// The systems added to `lua`, cloned out so none of the app data is
/// borrowed while they run (they may add app data of their own).
fn all(lua: &Lua) -> Vec<Rc<dyn GameSystem>> {
    lua.app_data_ref::<Systems>()
        .map(|systems| systems.0.clone())
        .unwrap_or_default()
}

/// The service `name` from the first system that has it, if any does.
pub(crate) fn service(lua: &Lua, name: &str) -> Option<mlua::Result<mlua::Value>> {
    all(lua)
        .into_iter()
        .find(|system| system.services().contains(&name))
        .map(|system| system.service(lua, name))
}

/// Every service the systems on `lua` add, for error messages.
pub(crate) fn service_names(lua: &Lua) -> Vec<&'static str> {
    all(lua)
        .iter()
        .flat_map(|system| system.services().iter().copied())
        .collect()
}

/// Steps every system on `lua`, in the order they were added.
pub(crate) fn step(lua: &Lua) {
    for system in all(lua) {
        system.step(lua);
    }
}

/// Game time in seconds since the runtime was created: the clock
/// `task.wait` and `task.delay` count on.
pub fn now(lua: &Lua) -> f64 {
    crate::scheduler::now(lua)
}

/// How close to due a time counts as due: frame times summed as floats
/// fall slightly short, and this keeps that from delaying anything a frame.
pub const TOLERANCE_SECONDS: f64 = crate::scheduler::TOLERANCE_SECONDS;

/// Destroys `id` as `instance:Destroy()` does: `Destroying` and the other
/// events fire, and destroying twice does nothing. The World must be
/// lent (it is during [`GameSystem::step`]).
pub fn destroy(lua: &Lua, id: InstanceId) -> mlua::Result<()> {
    crate::tree::destroy(lua, id)
}

/// Adds `err` to the output as a script error, with its stack trace.
pub fn report_error(lua: &Lua, err: &mlua::Error) {
    crate::push_error(lua, err);
}

/// The type of `value` as scripts would name it (`"Vector3"`, `"number"`,
/// ...), for error messages.
pub fn type_name(lua: &Lua, value: &mlua::Value) -> String {
    crate::types::type_name(lua, value)
}
