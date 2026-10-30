//! `game:GetService("ConsoleService")` (Phase 71): a game's own commands
//! for the Developer Console (F9).
//!
//! - `RegisterCommand(name, help, callback)` adds the command `name`, and
//!   `help` says what it does in the console's `help` list. Typed as
//!   `name arg1 arg2`, it calls `callback(player, "arg1", "arg2")` in a task
//!   of its own, so it can `task.wait`; `player` is who typed it. A
//!   LocalScript's command runs on that device, a Script's on the server.
//!   Registering a name again replaces it.
//! - `AllowedUserIds` (a list of numbers, on the server): the `UserId`s of
//!   the players who may run the server's commands from another device.
//!   The device that hosts, or plays offline, always may.
//!
//! Hosts run what's typed with
//! [`ScriptRuntime::run_console_command`](crate::ScriptRuntime::run_console_command),
//! and a joined player's command, on the server, with
//! [`run_remote_console_command`](crate::ScriptRuntime::run_remote_console_command).

use std::collections::BTreeMap;

use axcenity_core::{InstanceId, Value, World};
use mlua::{Function, Lua, MetaMethod, UserData, UserDataFields, UserDataMethods};

use crate::instance::LuaInstance;
use crate::scheduler;
use crate::types::type_name;

/// One registered command.
struct Command {
    help: String,
    callback: Function,
    /// The script that registered it, which forgets it when it stops.
    script: Option<InstanceId>,
}

/// One side's commands and who may run them.
#[derive(Default)]
pub(crate) struct ConsoleCommands {
    commands: BTreeMap<String, Command>,
    allowed: Vec<f64>,
}

impl ConsoleCommands {
    pub fn forget_script(&mut self, script: InstanceId) {
        self.commands
            .retain(|_, command| command.script != Some(script));
    }
}

/// What came of a line typed in the console.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConsoleRun {
    /// A registered command started.
    Ran,
    /// The command is the server's, and the player who typed it isn't in
    /// `AllowedUserIds`.
    NotAllowed,
    /// No command has that name here.
    Unknown,
}

/// A registered command, for the console's `help`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConsoleCommandInfo {
    pub name: String,
    pub help: String,
}

/// The command's name and its arguments: the words of `line`, split at
/// spaces. `None` for a blank line.
pub fn split_command(line: &str) -> Option<(&str, Vec<String>)> {
    let mut words = line.split_whitespace();
    let name = words.next()?;
    Some((name, words.map(str::to_owned).collect()))
}

fn with_commands<T>(lua: &Lua, f: impl FnOnce(&mut ConsoleCommands) -> T) -> T {
    if lua.app_data_ref::<ConsoleCommands>().is_none() {
        lua.set_app_data(ConsoleCommands::default());
    }
    let mut commands = lua
        .app_data_mut::<ConsoleCommands>()
        .expect("set just above");
    f(&mut commands)
}

/// The commands registered on this side, by name.
pub(crate) fn commands(lua: &Lua) -> Vec<ConsoleCommandInfo> {
    lua.app_data_ref::<ConsoleCommands>()
        .map(|commands| {
            commands
                .commands
                .iter()
                .map(|(name, command)| ConsoleCommandInfo {
                    name: name.clone(),
                    help: command.help.clone(),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Whether `player` (`None`: this device) may run this side's commands.
pub(crate) fn allowed(lua: &Lua, world: &World, player: Option<InstanceId>) -> bool {
    let Some(player) = player else {
        return true;
    };
    let Some(Value::Number(user_id)) = world.get(player).and_then(|p| p.property("UserId")) else {
        return false;
    };
    lua.app_data_ref::<ConsoleCommands>()
        .is_some_and(|commands| commands.allowed.contains(user_id))
}

/// The command `name`'s function and the script that registered it.
pub(crate) fn find(lua: &Lua, name: &str) -> Option<(Function, Option<InstanceId>)> {
    lua.app_data_ref::<ConsoleCommands>().and_then(|commands| {
        commands
            .commands
            .get(name)
            .map(|command| (command.callback.clone(), command.script))
    })
}

/// Starts `callback` as `script` with `player` and the words `args`. The
/// world must be lent to Lua.
pub(crate) fn start(
    lua: &Lua,
    (callback, script): (Function, Option<InstanceId>),
    player: Option<InstanceId>,
    args: &[String],
) {
    let mut values = vec![mlua::Value::Nil];
    if let Some(player) = player
        && let Ok(player) = mlua::IntoLua::into_lua(LuaInstance(player), lua)
    {
        values[0] = player;
    }
    for arg in args {
        match lua.create_string(arg) {
            Ok(text) => values.push(mlua::Value::String(text)),
            Err(err) => crate::push_error(lua, &err),
        }
    }
    match lua.create_thread(callback) {
        Ok(thread) => {
            scheduler::resume_as(lua, &thread, mlua::MultiValue::from_vec(values), script);
        }
        Err(err) => crate::push_error(lua, &err),
    }
}

/// `ConsoleService`, as scripts see it. Both sides have one, each with
/// commands of its own.
#[derive(Clone, Copy)]
pub struct LuaConsoleService;

impl UserData for LuaConsoleService {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("AllowedUserIds", |lua, _| {
            let allowed = with_commands(lua, |commands| commands.allowed.clone());
            lua.create_sequence_from(allowed)
        });
        fields.add_field_method_set("AllowedUserIds", |lua, _, list: mlua::Value| {
            let mlua::Value::Table(list) = list else {
                return Err(mlua::Error::runtime(format!(
                    "AllowedUserIds is a list of UserIds, such as {{123}}, not a {}",
                    type_name(lua, &list)
                )));
            };
            let mut ids = Vec::new();
            for value in list.sequence_values::<mlua::Value>() {
                match value? {
                    mlua::Value::Integer(id) => ids.push(id as f64),
                    mlua::Value::Number(id) => ids.push(id),
                    other => {
                        return Err(mlua::Error::runtime(format!(
                            "AllowedUserIds holds UserIds (numbers), not a {}",
                            type_name(lua, &other)
                        )));
                    }
                }
            }
            with_commands(lua, |commands| commands.allowed = ids);
            Ok(())
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method(
            "RegisterCommand",
            |lua, _, (name, help, callback): (String, mlua::Value, mlua::Value)| {
                let name = name.trim().to_owned();
                if name.is_empty() || name.contains(char::is_whitespace) {
                    return Err(mlua::Error::runtime(format!(
                        "RegisterCommand's name is one word, such as \"heal\", not \"{name}\""
                    )));
                }
                let help = match help {
                    mlua::Value::Nil => String::new(),
                    mlua::Value::String(text) => text.to_string_lossy(),
                    other => {
                        return Err(mlua::Error::runtime(format!(
                            "RegisterCommand's help is text, not a {}",
                            type_name(lua, &other)
                        )));
                    }
                };
                let mlua::Value::Function(callback) = callback else {
                    return Err(mlua::Error::runtime(format!(
                        "RegisterCommand takes a function to run, not a {}",
                        type_name(lua, &callback)
                    )));
                };
                let script = scheduler::running_script(lua);
                with_commands(lua, |commands| {
                    commands.commands.insert(
                        name,
                        Command {
                            help,
                            callback,
                            script,
                        },
                    );
                });
                Ok(())
            },
        );
        methods.add_meta_method(MetaMethod::ToString, |_, _, ()| Ok("ConsoleService"));
    }
}
