//! Game systems from other crates (roadmap L7). A [`GameSystem`] adds
//! services to `game:GetService` (and `game.<Name>`), steps once per frame
//! on each side, and hooks into the frame where the runtime offers it,
//! without `script` knowing it: the `gameplay` crate holds them, and hosts
//! add them with [`crate::ScriptRuntime::add_system`].
//!
//! The functions here are the part of the interpreter a system may use: the
//! game clock, destroying an Instance as `Destroy` does, reporting errors,
//! naming a value's type, script environments, resuming waiting scripts,
//! and service signals. The `ScriptRuntime` methods documented as "for game
//! systems" are the part of the runtime. A system keeps per-side state in
//! the interpreter's app data (`Lua::set_app_data`, set up in
//! [`GameSystem::added`]), and state both sides share in itself.

use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;

use axcenity_core::{ClassName, InstanceId, World};
use axcenity_save::Saves;
use mlua::{AnyUserData, Function, IntoLuaMulti, Lua, Table, Thread};

pub use crate::cframe::{LuaCFrame, to_vec3};
pub use crate::health::{HealthEvent, damage, heal, set_health};

use crate::input::PromptInput;
use crate::{ScriptRuntime, Side};

/// A part of the game that lives outside `script`: its services, its
/// per-frame step and its hooks into the frame. One value serves both
/// sides. Every method has a default that does nothing, so a system names
/// only what it uses.
pub trait GameSystem: Any {
    /// The names of the services it adds, such as `"Debris"`.
    fn services(&self) -> &'static [&'static str] {
        &[]
    }

    /// The service `name` (one of [`GameSystem::services`]) as the scripts
    /// on `lua` see it.
    fn service(&self, _lua: &Lua, name: &str) -> mlua::Result<mlua::Value> {
        Err(mlua::Error::runtime(format!(
            "there is no service named \"{name}\""
        )))
    }

    /// Scripts' name for one of its userdata values, for `typeof` and
    /// error messages (`"QuestService"`); `None` when it isn't one.
    fn type_name(&self, _data: &AnyUserData) -> Option<&'static str> {
        None
    }

    /// Called once for each side's interpreter when the system is added,
    /// before any script runs: set up its app data here.
    fn added(&self, _lua: &Lua) {}

    /// Methods it adds to Instances of a class (`ability:Activate()`), as
    /// `(class, name, function)`. A name the engine already has for that
    /// class keeps the engine's.
    fn methods(&self, _lua: &Lua) -> mlua::Result<Vec<(ClassName, &'static str, Function)>> {
        Ok(Vec::new())
    }

    /// Why scripts can't set `class.key`, when it's a property the system
    /// keeps (`Ability.CooldownLeft`); `None` lets the write through.
    fn read_only(&self, _class: ClassName, _key: &str) -> Option<String> {
        None
    }

    /// Runs once per frame on each side, after scripts have resumed, with
    /// the World lent (`crate::lent_world_mut`). Systems step in the order
    /// they were added.
    fn step(&self, _lua: &Lua) {}

    /// True while it has the player's keys (a dialogue runs): prompts,
    /// ClickDetectors and Tools don't step, and the character stands still
    /// (`ScriptRuntime::movement_locked`).
    fn holds_input(&self) -> bool {
        false
    }

    /// Runs in `ScriptRuntime::step_prompts` (frame step 1) with the
    /// player's input: after ProximityPrompts and ClickDetectors have
    /// stepped and before their events fire, or instead of them while a
    /// system [holds input](GameSystem::holds_input).
    fn step_input(&self, _runtime: &mut ScriptRuntime, _world: &mut World, _input: &PromptInput) {}

    /// A ProximityPrompt `prompt` fired `Triggered`, after its handlers ran.
    fn prompt_triggered(
        &self,
        _runtime: &mut ScriptRuntime,
        _world: &mut World,
        _prompt: InstanceId,
        _input: &PromptInput,
    ) {
    }

    /// Fires the system's waiting events on both sides, from
    /// `ScriptRuntime::fire_changes`. Use `fire_change_rounds`, not
    /// `fire_changes`, after them. True when something fired, so the
    /// runtime asks again (up to its limit of rounds).
    fn fire_events(&self, _runtime: &mut ScriptRuntime, _world: &mut World) -> bool {
        false
    }

    /// True to fire its events before the engine's own (Health, Tools,
    /// Players, levels) rather than after them: Abilities do, so the
    /// Health events their Effects cause fire in the same call.
    fn fires_first(&self) -> bool {
        false
    }

    /// Runs in `ScriptRuntime::step_health` (frame step 4, on the device
    /// that runs the server) after Healths step, with the frame's length.
    fn step_server(&self, _runtime: &mut ScriptRuntime, _world: &mut World, _seconds: f64) {}

    /// A player asked to activate `target` (`MessageKind::Activate`, from
    /// this device or another), for the server to serve.
    fn activation_requested(&self, _target: InstanceId, _player: InstanceId) {}
}

/// Services the API definitions (`definitions/tables.rs`) describe that
/// game systems add rather than `script`: a runtime made with
/// `ScriptRuntime::new` lacks them, so `script`'s definition tests skip
/// them and the `gameplay` crate's tests check them instead.
#[doc(hidden)]
pub const DESCRIBED_SYSTEM_SERVICES: &[&str] = &["Debris", "DialogueService", "QuestService"];

/// Instance methods the API definitions describe that game systems add
/// ([`GameSystem::methods`]), skipped and checked as
/// [`DESCRIBED_SYSTEM_SERVICES`] are.
#[doc(hidden)]
pub const DESCRIBED_SYSTEM_METHODS: &[(ClassName, &str)] = &[
    (ClassName::Ability, "CanActivate"),
    (ClassName::Ability, "Activate"),
    (ClassName::Ability, "Cancel"),
    (ClassName::Model, "ApplyEffect"),
    (ClassName::Inventory, "GetItems"),
    (ClassName::Inventory, "Add"),
    (ClassName::Inventory, "Remove"),
    (ClassName::Inventory, "Equip"),
    (ClassName::Inventory, "UnequipAll"),
    (ClassName::Tool, "Drop"),
    (ClassName::Humanoid, "EquipTool"),
    (ClassName::Humanoid, "UnequipTools"),
];

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

/// Why `class.key` is read-only, if a system on `lua` keeps it.
pub(crate) fn read_only(lua: &Lua, class: ClassName, key: &str) -> Option<String> {
    all(lua)
        .iter()
        .find_map(|system| system.read_only(class, key))
}

/// Scripts' name for `data` if a system on `lua` made it.
pub(crate) fn type_name_of(lua: &Lua, data: &AnyUserData) -> Option<&'static str> {
    all(lua).iter().find_map(|system| system.type_name(data))
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

/// Adds a line to the output as an error, without a stack trace.
pub fn report(lua: &Lua, text: impl Into<String>) {
    crate::push_output(lua, crate::OutputKind::Error, text.into());
}

/// An error's message as the output shows it, with the script's path and
/// line.
pub fn error_message(err: &mlua::Error) -> String {
    crate::errors::error_message(err)
}

/// Which side `lua` is.
pub fn side(lua: &Lua) -> Side {
    lua.app_data_ref::<Side>()
        .map_or(Side::Server, |side| *side)
}

/// A fresh script environment (the sandbox's globals) for engine code,
/// with `script` set to the DataModel. The World must be lent.
pub fn engine_environment(lua: &Lua) -> mlua::Result<Table> {
    let ids = {
        let world = crate::instance::world(lua)?;
        crate::sandbox::ScriptIds {
            game: world.root(),
            workspace: world.workspace(),
            script: world.root(),
        }
    };
    crate::sandbox::environment(lua, ids)
}

/// The script whose code is running now, if any.
pub fn running_script(lua: &Lua) -> Option<InstanceId> {
    crate::scheduler::running_script(lua)
}

/// Tells the debugger that `thread` now waits on the engine (call it
/// before yielding a script from engine code), so a step over a waiting
/// line isn't held.
pub fn waiting(lua: &Lua, thread: &Thread) {
    crate::debugger::waiting(lua, thread);
}

/// Resumes `thread` with `args`, as owned by `script` (its handlers and
/// errors belong to that script). The World must be lent.
pub fn resume_as(lua: &Lua, thread: &Thread, args: impl IntoLuaMulti, script: Option<InstanceId>) {
    crate::scheduler::resume_as(lua, thread, args, script);
}

/// The signal `service.event` (`QuestService.QuestStarted`), for a
/// service's field.
pub fn signal(service: &'static str, event: &'static str) -> impl mlua::UserData + 'static {
    crate::events::LuaSignal::named(crate::events::Owner::Service(service), event)
}

/// Fires `service.event` on `lua`'s side with `args`. The World must be
/// lent.
pub fn fire(lua: &Lua, service: &'static str, event: &'static str, args: impl IntoLuaMulti) {
    crate::events::fire(lua, crate::events::Owner::Service(service), event, args);
}

/// SaveService's player data, settings and checkpoint, which both sides
/// share.
pub fn saves(lua: &Lua) -> mlua::Result<Rc<RefCell<Saves>>> {
    crate::save::saves(lua)
}

/// Where the player character's middle is, if there is one: what quests'
/// Reach objectives and ProximityPrompts measure from.
pub fn player_middle(world: &World) -> Option<glam::Vec3> {
    crate::prompt::Viewer::find(world, None).map(|viewer| viewer.position)
}

/// The error for using a destroyed Instance.
pub fn destroyed() -> mlua::Error {
    crate::instance::destroyed()
}

/// The local player on `lua`'s side: a LocalScript's `Players.LocalPlayer`.
pub fn local_player(lua: &Lua) -> Option<InstanceId> {
    lua.app_data_ref::<crate::client::LocalPlayer>()
        .map(|player| player.0)
}

/// The Inventory of the character whose Humanoid is `humanoid`, made (with
/// the character's other pieces) if it has none. An error names `call`
/// when the Humanoid is destroyed or isn't in a character's Model.
pub fn humanoid_inventory(
    lua: &Lua,
    humanoid: InstanceId,
    call: &str,
) -> mlua::Result<Option<InstanceId>> {
    crate::humanoid::inventory(lua, humanoid, call)
}

/// The Player whose `Character` is `character`, if any.
pub fn player_of(world: &World, character: InstanceId) -> Option<InstanceId> {
    crate::players::player_of(world, character)
}

/// From a LocalScript: asks the server to activate `target` for
/// `player` (`MessageKind::Activate`); the server's system gets
/// [`GameSystem::activation_requested`].
pub fn send_activation(lua: &Lua, target: InstanceId, player: InstanceId) {
    if let Some(mut outbox) = lua.app_data_mut::<crate::remote::Outbox>() {
        outbox.0.push(crate::Message {
            to: Side::Server,
            remote: target,
            player,
            kind: crate::MessageKind::Activate,
            args: Vec::new(),
        });
    }
}

/// Fires the event `event` of the Instance `id` (`ability.Activated`) on
/// `lua`'s side with `args`. The World must be lent.
pub fn fire_instance_event(
    lua: &Lua,
    id: InstanceId,
    event: &'static str,
    args: impl IntoLuaMulti,
) {
    crate::events::fire(lua, crate::events::Owner::Instance(id), event, args);
}
