//! The globals a plugin's code gets on top of a script's: `plugin`,
//! `Selection` and `ChangeHistoryService` (also from `game:GetService`),
//! and a `require` that loads the plugin's own Lua files.
//!
//! What the plugin makes (toolbars, dock widgets) and asks for (a new
//! selection, waypoints) is kept in [`PluginState`], in Lua's app data,
//! for the host to read after each call.

use std::collections::BTreeMap;

use axcenity_core::{ClassName, InstanceId};
use axcenity_script::{LuaInstance, lent_world, lent_world_mut};
use mlua::{
    Function, IntoLua, Lua, MetaMethod, Table, UserData, UserDataFields, UserDataMethods, Value,
};
use serde_json::Value as Json;

use crate::{DockWidget, MouseState, Request, Toolbar, ToolbarButton, Waypoint, files};

/// An event of the plugin API.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum SignalKey {
    /// A toolbar button's `Click`, by the button's number.
    Click(u32),
    /// `Selection.SelectionChanged`.
    SelectionChanged,
    /// The mouse's `Button1Down`, `Button1Up` and `Move`.
    Button1Down,
    Button1Up,
    Move,
    /// `plugin.Deactivation`: another plugin or tool took the mouse.
    Deactivation,
}

/// A handler connected to a plugin event.
pub(crate) struct Handler {
    pub(crate) id: u64,
    pub(crate) callback: Function,
    /// Disconnected after it first runs (`Once`).
    pub(crate) once: bool,
}

/// Everything a plugin made and asked for. See the module documentation.
pub(crate) struct PluginState {
    /// The plugin's folder, for `require`.
    pub(crate) folder: std::path::PathBuf,
    /// How its files are named in messages, such as `plugins/align`.
    pub(crate) shown: String,
    /// The open project, for file access.
    pub(crate) project: Option<std::path::PathBuf>,
    /// Whether `plugin.json` asks for the `files` permission.
    pub(crate) files: bool,
    pub(crate) toolbars: Vec<Toolbar>,
    pub(crate) next_button: u32,
    pub(crate) widgets: Vec<DockWidget>,
    pub(crate) handlers: BTreeMap<SignalKey, Vec<Handler>>,
    pub(crate) next_connection: u64,
    pub(crate) selection: Vec<InstanceId>,
    pub(crate) requests: Vec<Request>,
    pub(crate) settings: BTreeMap<String, Json>,
    pub(crate) settings_changed: bool,
    pub(crate) mouse: MouseState,
    /// `plugin:Activate(true)` was called: the plugin has the Scene View's
    /// left button.
    pub(crate) mouse_active: bool,
    /// Modules `require` loaded, by name.
    pub(crate) modules: BTreeMap<String, Value>,
    /// The plugin's environment, for `CFrame.new` and modules.
    pub(crate) env: Option<Table>,
}

pub(crate) fn state(lua: &Lua) -> mlua::Result<mlua::AppDataRefMut<'_, PluginState>> {
    lua.app_data_mut::<PluginState>()
        .ok_or_else(|| mlua::Error::runtime("the plugin isn't running"))
}

/// Adds the plugin globals to `env`.
pub(crate) fn install(lua: &Lua, env: &Table) -> mlua::Result<()> {
    env.raw_set("plugin", LuaPlugin)?;
    env.raw_set("Selection", LuaSelection)?;
    env.raw_set("ChangeHistoryService", LuaChangeHistory)?;
    env.raw_set("require", lua.create_function(require)?)?;
    Ok(())
}

/// `require("name")`: runs the plugin's `name.lua` once and returns what it
/// returns. Names are paths inside the plugin's folder (`"tools/snap"`).
fn require(lua: &Lua, name: String) -> mlua::Result<Value> {
    if let Some(loaded) = state(lua)?.modules.get(&name) {
        return Ok(loaded.clone());
    }
    let (folder, shown, env) = {
        let state = state(lua)?;
        (state.folder.clone(), state.shown.clone(), state.env.clone())
    };
    let env = env.ok_or_else(|| mlua::Error::runtime("the plugin isn't running"))?;
    let path = format!("{name}.lua");
    let file = files::project_file(&folder, &path)
        .map_err(|_| mlua::Error::runtime(format!("no module \"{name}\" in this plugin")))?;
    let source = std::fs::read_to_string(&file)
        .map_err(|_| mlua::Error::runtime(format!("no module \"{name}\" in this plugin")))?;
    let function = lua
        .load(source)
        .set_name(format!("@{shown}/{path}"))
        .set_mode(mlua::chunk::ChunkMode::Text)
        .set_environment(env)
        .into_function()?;
    let loaded: Value = function.call(())?;
    let loaded = if loaded.is_nil() {
        Value::Boolean(true)
    } else {
        loaded
    };
    state(lua)?.modules.insert(name, loaded.clone());
    Ok(loaded)
}

/// The `plugin` global.
struct LuaPlugin;

impl UserData for LuaPlugin {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("Deactivation", |_, _| {
            Ok(LuaSignal(SignalKey::Deactivation))
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("CreateToolbar", |lua, _, name: String| {
            let mut state = state(lua)?;
            let index = match state.toolbars.iter().position(|bar| bar.name == name) {
                Some(index) => index,
                None => {
                    state.toolbars.push(Toolbar {
                        name,
                        buttons: Vec::new(),
                    });
                    state.toolbars.len() - 1
                }
            };
            Ok(LuaToolbar(index))
        });
        methods.add_method(
            "CreateDockWidget",
            |lua, _, (id, info): (String, Option<Table>)| {
                let title = match &info {
                    Some(info) => info.get::<Option<String>>("Title")?,
                    None => None,
                }
                .unwrap_or_else(|| id.clone());
                if let Some(widget) = state(lua)?.widgets.iter().find(|w| w.id == id) {
                    return Ok(LuaInstance(widget.gui));
                }
                let gui = {
                    let mut world = lent_world_mut(lua)?;
                    let gui = world
                        .create(ClassName::ScreenGui)
                        .map_err(|err| mlua::Error::runtime(err.to_string()))?;
                    let _ = world.set_name(gui, id.clone());
                    gui
                };
                state(lua)?.widgets.push(DockWidget { id, title, gui });
                Ok(LuaInstance(gui))
            },
        );
        methods.add_method("GetSetting", |lua, _, key: String| {
            let value = state(lua)?.settings.get(&key).cloned();
            match value {
                Some(json) => json_to_lua(lua, &json),
                None => Ok(Value::Nil),
            }
        });
        methods.add_method("SetSetting", |lua, _, (key, value): (String, Value)| {
            let json = lua_to_json(&value, 0)?;
            let mut state = state(lua)?;
            match json {
                Json::Null => state.settings.remove(&key),
                json => state.settings.insert(key, json),
            };
            state.settings_changed = true;
            Ok(())
        });
        methods.add_method("ReadProjectFile", |lua, _, path: String| {
            let project = file_access(lua)?;
            files::read(&project, &path).map_err(mlua::Error::runtime)
        });
        methods.add_method(
            "WriteProjectFile",
            |lua, _, (path, text): (String, String)| {
                let project = file_access(lua)?;
                files::write(&project, &path, &text).map_err(mlua::Error::runtime)
            },
        );
        methods.add_method("GetMouse", |_, _, ()| Ok(LuaMouse));
        methods.add_method("Activate", |lua, _, exclusive: Option<bool>| {
            state(lua)?.mouse_active = exclusive.unwrap_or(false);
            Ok(())
        });
        methods.add_method("Deactivate", |lua, _, ()| {
            state(lua)?.mouse_active = false;
            Ok(())
        });
        methods.add_method("IsActivated", |lua, _, ()| Ok(state(lua)?.mouse_active));
        methods.add_meta_method(MetaMethod::ToString, |_, _, ()| Ok("Plugin"));
    }
}

/// The project folder, if the plugin may use its files.
fn file_access(lua: &Lua) -> mlua::Result<std::path::PathBuf> {
    let state = state(lua)?;
    if !state.files {
        return Err(mlua::Error::runtime(
            "this plugin can't use files: add \"files\" to the permissions in its plugin.json",
        ));
    }
    state
        .project
        .clone()
        .ok_or_else(|| mlua::Error::runtime("no project is open"))
}

/// A toolbar: a ribbon group of buttons, by its index.
struct LuaToolbar(usize);

impl UserData for LuaToolbar {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method(
            "CreateButton",
            |lua, this, (id, tooltip, icon): (String, Option<String>, Option<String>)| {
                let mut state = state(lua)?;
                let number = state.next_button;
                let toolbar = state
                    .toolbars
                    .get_mut(this.0)
                    .ok_or_else(|| mlua::Error::runtime("this toolbar is gone"))?;
                if let Some(button) = toolbar.buttons.iter().find(|b| b.name == id) {
                    return Ok(LuaButton(button.id));
                }
                toolbar.buttons.push(ToolbarButton {
                    id: number,
                    name: id,
                    tooltip: tooltip.unwrap_or_default(),
                    icon: icon.unwrap_or_default(),
                    enabled: true,
                    active: false,
                });
                state.next_button += 1;
                Ok(LuaButton(number))
            },
        );
        methods.add_meta_method(MetaMethod::ToString, |_, _, ()| Ok("PluginToolbar"));
    }
}

/// A toolbar button, by its number.
struct LuaButton(u32);

fn with_button<R>(lua: &Lua, id: u32, f: impl FnOnce(&mut ToolbarButton) -> R) -> mlua::Result<R> {
    let mut state = state(lua)?;
    state
        .toolbars
        .iter_mut()
        .flat_map(|toolbar| toolbar.buttons.iter_mut())
        .find(|button| button.id == id)
        .map(f)
        .ok_or_else(|| mlua::Error::runtime("this button is gone"))
}

impl UserData for LuaButton {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("Click", |_, this| Ok(LuaSignal(SignalKey::Click(this.0))));
        fields.add_field_method_get("Enabled", |lua, this| {
            with_button(lua, this.0, |button| button.enabled)
        });
        fields.add_field_method_set("Enabled", |lua, this, enabled: bool| {
            with_button(lua, this.0, |button| button.enabled = enabled)
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("SetActive", |lua, this, active: bool| {
            with_button(lua, this.0, |button| button.active = active)
        });
        methods.add_meta_method(MetaMethod::ToString, |_, _, ()| Ok("PluginToolbarButton"));
    }
}

/// The `Selection` service: what's selected in the editor.
pub(crate) struct LuaSelection;

impl UserData for LuaSelection {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("SelectionChanged", |_, _| {
            Ok(LuaSignal(SignalKey::SelectionChanged))
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("Get", |lua, _, ()| {
            let world = lent_world(lua)?;
            let selection = state(lua)?.selection.clone();
            Ok(selection
                .into_iter()
                .filter(|&id| world.contains(id))
                .map(LuaInstance)
                .collect::<Vec<_>>())
        });
        methods.add_method("Set", |lua, _, instances: Vec<LuaInstance>| {
            let ids: Vec<InstanceId> = {
                let world = lent_world(lua)?;
                let mut ids = Vec::new();
                for LuaInstance(id) in instances {
                    // Only what's in the game can be selected.
                    if world.is_descendant_of(id, world.root()) && !ids.contains(&id) {
                        ids.push(id);
                    }
                }
                ids
            };
            let mut state = state(lua)?;
            state.selection = ids.clone();
            state.requests.push(Request::Select(ids));
            Ok(())
        });
        methods.add_meta_method(MetaMethod::ToString, |_, _, ()| Ok("Selection"));
    }
}

/// The `ChangeHistoryService`: undo steps.
pub(crate) struct LuaChangeHistory;

impl UserData for LuaChangeHistory {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("SetWaypoint", |lua, _, name: String| {
            let waypoint = {
                let mut world = lent_world_mut(lua)?;
                let changes = world.take_changes();
                let after = (!changes.is_empty()).then(|| Box::new(world.clone()));
                Waypoint {
                    name,
                    changes,
                    world: after,
                }
            };
            state(lua)?.requests.push(Request::Waypoint(waypoint));
            Ok(())
        });
        methods.add_method("Undo", |lua, _, ()| {
            state(lua)?.requests.push(Request::Undo);
            Ok(())
        });
        methods.add_method("Redo", |lua, _, ()| {
            state(lua)?.requests.push(Request::Redo);
            Ok(())
        });
        methods.add_meta_method(MetaMethod::ToString, |_, _, ()| Ok("ChangeHistoryService"));
    }
}

/// `plugin:GetMouse()`: the mouse over the Scene View.
struct LuaMouse;

impl UserData for LuaMouse {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("Hit", |lua, _| {
            let (hit, env) = {
                let state = state(lua)?;
                (state.mouse.hit, state.env.clone())
            };
            let env = env.ok_or_else(|| mlua::Error::runtime("the plugin isn't running"))?;
            let cframe: Table = env.get("CFrame")?;
            let new: Function = cframe.get("new")?;
            new.call::<Value>((hit.x, hit.y, hit.z))
        });
        fields.add_field_method_get("Target", |lua, _| {
            let target = state(lua)?.mouse.target;
            let world = lent_world(lua)?;
            match target.filter(|&id| world.contains(id)) {
                Some(id) => LuaInstance(id).into_lua(lua),
                None => Ok(Value::Nil),
            }
        });
        fields.add_field_method_get("Button1Down", |_, _| Ok(LuaSignal(SignalKey::Button1Down)));
        fields.add_field_method_get("Button1Up", |_, _| Ok(LuaSignal(SignalKey::Button1Up)));
        fields.add_field_method_get("Move", |_, _| Ok(LuaSignal(SignalKey::Move)));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::ToString, |_, _, ()| Ok("PluginMouse"));
    }
}

/// One of the plugin API's events.
struct LuaSignal(SignalKey);

impl UserData for LuaSignal {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("Connect", |lua, this, callback: Function| {
            connect(lua, this.0, callback, false)
        });
        methods.add_method("Once", |lua, this, callback: Function| {
            connect(lua, this.0, callback, true)
        });
        methods.add_meta_method(MetaMethod::ToString, |_, _, ()| Ok("Signal"));
    }
}

fn connect(
    lua: &Lua,
    key: SignalKey,
    callback: Function,
    once: bool,
) -> mlua::Result<LuaConnection> {
    let mut state = state(lua)?;
    let id = state.next_connection;
    state.next_connection += 1;
    state
        .handlers
        .entry(key)
        .or_default()
        .push(Handler { id, callback, once });
    Ok(LuaConnection(key, id))
}

struct LuaConnection(SignalKey, u64);

impl UserData for LuaConnection {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("Connected", |lua, this| {
            Ok(state(lua)?
                .handlers
                .get(&this.0)
                .is_some_and(|handlers| handlers.iter().any(|h| h.id == this.1)))
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("Disconnect", |lua, this, ()| {
            if let Some(handlers) = state(lua)?.handlers.get_mut(&this.0) {
                handlers.retain(|handler| handler.id != this.1);
            }
            Ok(())
        });
    }
}

/// How deep a setting's tables may nest.
const MAX_SETTING_DEPTH: usize = 16;

/// A setting's value as JSON: nil, booleans, numbers, strings and tables
/// of them. A table with only the keys 1 to n is an array.
fn lua_to_json(value: &Value, depth: usize) -> mlua::Result<Json> {
    if depth > MAX_SETTING_DEPTH {
        return Err(mlua::Error::runtime("a setting's tables nest too deep"));
    }
    Ok(match value {
        Value::Nil => Json::Null,
        Value::Boolean(b) => Json::Bool(*b),
        Value::Integer(n) => Json::from(*n),
        Value::Number(n) => serde_json::Number::from_f64(*n).map_or(Json::Null, Json::Number),
        Value::String(s) => Json::String(s.to_str()?.to_owned()),
        Value::Table(table) => {
            let length = table.raw_len();
            let pairs: Vec<(Value, Value)> =
                table.pairs::<Value, Value>().collect::<Result<_, _>>()?;
            if length > 0 && pairs.len() == length {
                let mut items = Vec::with_capacity(length);
                for index in 1..=length {
                    items.push(lua_to_json(&table.raw_get::<Value>(index)?, depth + 1)?);
                }
                Json::Array(items)
            } else {
                let mut object = serde_json::Map::new();
                for (key, item) in pairs {
                    let key = match key {
                        Value::String(s) => s.to_str()?.to_owned(),
                        _ => {
                            return Err(mlua::Error::runtime(
                                "a setting's table keys must be strings, or 1 to n for a list",
                            ));
                        }
                    };
                    object.insert(key, lua_to_json(&item, depth + 1)?);
                }
                Json::Object(object)
            }
        }
        _ => {
            return Err(mlua::Error::runtime(
                "a setting can only hold nil, true or false, numbers, strings and tables of them",
            ));
        }
    })
}

fn json_to_lua(lua: &Lua, json: &Json) -> mlua::Result<Value> {
    Ok(match json {
        Json::Null => Value::Nil,
        Json::Bool(b) => Value::Boolean(*b),
        Json::Number(n) => Value::Number(n.as_f64().unwrap_or(0.0)),
        Json::String(s) => Value::String(lua.create_string(s)?),
        Json::Array(items) => {
            let table = lua.create_table()?;
            for (index, item) in items.iter().enumerate() {
                table.raw_set(index + 1, json_to_lua(lua, item)?)?;
            }
            Value::Table(table)
        }
        Json::Object(object) => {
            let table = lua.create_table()?;
            for (key, item) in object {
                table.raw_set(key.as_str(), json_to_lua(lua, item)?)?;
            }
            Value::Table(table)
        }
    })
}
