//! The editor's Lua plugins (Phase 54): finding them ([`find_plugins`]),
//! and running one ([`Plugin`]) with the plugin API in a sandboxed
//! interpreter of its own, against the world being edited.
//!
//! Only the editor uses this crate; the player and built games never have
//! plugin code. Behavior: `docs/editor/plugins.md`.
//!
//! A [`Plugin`] doesn't know about undo or the editor's panels. The host
//! calls it (a button's click, a new selection, the mouse, a frame), then
//! reads what it made ([`Plugin::toolbars`], [`Plugin::widgets`],
//! [`Plugin::take_drawings`]) and asked
//! for ([`Plugin::take_requests`]), and its output.

mod api;
pub mod bundle;
mod draw;
pub mod files;
mod manifest;
mod rows;

use std::collections::BTreeMap;
use std::path::PathBuf;

use axcenity_core::{InstanceId, RecordedChange, Value, Vector3, World};
use axcenity_script::{EditRuntime, OutputLine};
use mlua::{IntoLuaMulti, MultiValue};

pub use bundle::{
    Preview, SUFFIX as PLUGIN_PACKAGE_SUFFIX, install, pack, preview, scaffold, slug,
};
pub use draw::{
    DEFAULT_COLOR, DEFAULT_HANDLE_SIZE, DEFAULT_THICKNESS, Drawing, Handle, MAX_DRAWINGS,
    MAX_HANDLES, Shape,
};
pub use manifest::{
    ENTRY_FILE, Found, MANIFEST_FILE, Manifest, Origin, PLUGINS_FOLDER, Permission, find_plugins,
    plugin_files,
};
pub use rows::{MAX_ROWS, PropertyRow, RowKind};
pub use serde_json::Value as Json;

use api::{PluginState, SignalKey};

/// The luau-lsp definitions of the plugin API, to load after the engine's
/// (`definitions/axcenity-plugin.d.luau`).
pub const PLUGIN_DEFINITIONS: &str = include_str!("../../../definitions/axcenity-plugin.d.luau");
/// Where the definitions are kept, relative to the repository root.
pub const PLUGIN_DEFINITIONS_PATH: &str = "definitions/axcenity-plugin.d.luau";

/// A plugin's settings (`plugin:GetSetting`), which the editor keeps in its
/// own data, by the plugin's [`Found::key`].
pub type Settings = BTreeMap<String, Json>;

/// A toolbar a plugin made: one ribbon group.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Toolbar {
    pub name: String,
    pub buttons: Vec<ToolbarButton>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolbarButton {
    /// The button's number, for [`Plugin::click`].
    pub id: u32,
    /// The id the plugin gave it, shown as its label.
    pub name: String,
    pub tooltip: String,
    /// A Phosphor icon name (`"grid-four"`), or empty.
    pub icon: String,
    pub enabled: bool,
    /// Drawn pressed in (`button:SetActive(true)`).
    pub active: bool,
}

/// A dock tab a plugin made with `plugin:CreateDockWidget`. Its contents
/// are UI Instances under `gui`, a ScreenGui outside the tree (so it's
/// never saved, shown in the Explorer, or undone).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DockWidget {
    pub id: String,
    pub title: String,
    pub gui: InstanceId,
}

/// Something a plugin asked the editor for during a call.
pub enum Request {
    /// `Selection:Set`: select these.
    Select(Vec<InstanceId>),
    /// `ChangeHistoryService:SetWaypoint`.
    Waypoint(Waypoint),
    Undo,
    Redo,
}

/// `ChangeHistoryService:SetWaypoint(name)`: the edits since the last
/// waypoint are one undo step named `name`.
pub struct Waypoint {
    pub name: String,
    /// The world's changes since the call started or the last waypoint,
    /// taken out of the world's change log.
    pub changes: Vec<RecordedChange>,
    /// The world as it was at the waypoint, when anything changed.
    pub world: Option<Box<World>>,
}

/// Where the mouse points in the Scene View.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MouseState {
    /// The point under the mouse (`mouse.Hit`).
    pub hit: Vector3,
    /// The part under the mouse (`mouse.Target`).
    pub target: Option<InstanceId>,
}

/// What the mouse did over the Scene View this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MouseInput {
    pub state: MouseState,
    pub moved: bool,
    pub pressed: bool,
    pub released: bool,
}

/// What a plugin starts with besides its folder.
#[derive(Clone, Debug, Default)]
pub struct Start {
    /// The open project's folder, for `ReadProjectFile` and `WriteProjectFile`.
    pub project: Option<PathBuf>,
    pub settings: Settings,
    pub selection: Vec<InstanceId>,
    /// How its files are named in messages, such as `plugins/align`.
    pub shown: String,
}

/// One running plugin. See the crate documentation.
pub struct Plugin {
    runtime: EditRuntime,
}

impl Plugin {
    /// Starts the plugin in `found`: runs its `init.lua` against `world`.
    /// `Err` when it can't start at all: a bad `plugin.json`, no
    /// `init.lua`, or code that doesn't compile (`path:line: message`). An
    /// error while `init.lua` runs joins the output instead, and the plugin
    /// keeps what it made before it.
    ///
    /// The world should be recording changes (`World::record_changes`), so
    /// waypoints can see them.
    pub fn start(world: &mut World, found: &Found, start: Start) -> Result<Plugin, String> {
        let manifest = found.manifest.clone()?;
        let entry = found.folder.join(ENTRY_FILE);
        let source = std::fs::read_to_string(&entry)
            .map_err(|err| format!("{}/{ENTRY_FILE}: {err}", start.shown))?;
        let runtime = EditRuntime::new();
        let lua = runtime.lua();
        lua.set_app_data(PluginState {
            folder: found.folder.clone(),
            shown: start.shown.clone(),
            project: start.project,
            files: manifest.allows(Permission::Files),
            toolbars: Vec::new(),
            next_button: 1,
            widgets: Vec::new(),
            handlers: BTreeMap::new(),
            next_connection: 1,
            selection: start.selection,
            requests: Vec::new(),
            settings: start.settings,
            settings_changed: false,
            mouse: MouseState::default(),
            mouse_active: false,
            modules: BTreeMap::new(),
            env: None,
            drawings: Default::default(),
            handles: Default::default(),
            rows: Default::default(),
        });
        let message = |err: mlua::Error| err.to_string();
        let env = runtime.environment(world).map_err(message)?;
        api::install(lua, &env).map_err(message)?;
        runtime.add_service(
            "Selection",
            mlua::Value::UserData(lua.create_userdata(api::LuaSelection).map_err(message)?),
        );
        runtime.add_service(
            "ChangeHistoryService",
            mlua::Value::UserData(
                lua.create_userdata(api::LuaChangeHistory)
                    .map_err(message)?,
            ),
        );
        if let Ok(mut state) = api::state(lua) {
            state.env = Some(env.clone());
        }
        let path = format!("{}/{ENTRY_FILE}", start.shown);
        let function = runtime
            .load(&env, &path, &source)
            .map_err(|err| axcenity_script::error_text(&err))?;
        runtime.run(world, &function, ());
        Ok(Plugin { runtime })
    }

    fn state(&self) -> Option<mlua::AppDataRef<'_, PluginState>> {
        self.runtime.lua().app_data_ref::<PluginState>()
    }

    /// The toolbars it made, in order.
    pub fn toolbars(&self) -> Vec<Toolbar> {
        self.state()
            .map(|state| state.toolbars.clone())
            .unwrap_or_default()
    }

    /// The dock widgets it made, in order.
    pub fn widgets(&self) -> Vec<DockWidget> {
        self.state()
            .map(|state| state.widgets.clone())
            .unwrap_or_default()
    }

    /// What it draws in the Scene View, when that changed since the last
    /// call (drawings stay until the plugin removes them).
    pub fn take_drawings(&self) -> Option<Vec<Drawing>> {
        let mut state = api::state(self.runtime.lua()).ok()?;
        std::mem::take(&mut state.drawings.changed)
            .then(|| state.drawings.shapes.values().cloned().collect())
    }

    /// Its handles in the Scene View, when they changed since the last call.
    pub fn take_handles(&self) -> Option<Vec<Handle>> {
        let mut state = api::state(self.runtime.lua()).ok()?;
        std::mem::take(&mut state.handles.changed)
            .then(|| state.handles.handles.values().cloned().collect())
    }

    /// Its Properties rows, when they changed since the last call.
    pub fn take_rows(&self) -> Option<Vec<PropertyRow>> {
        let mut state = api::state(self.runtime.lua()).ok()?;
        std::mem::take(&mut state.rows.changed).then(|| state.rows.rows.values().cloned().collect())
    }

    /// The Properties button row numbered `row` was clicked for
    /// `instance`: fires its `Activated(instance)`.
    pub fn activate_row(&self, world: &mut World, row: u64, instance: InstanceId) {
        if self.has_row(row) && world.contains(instance) {
            let instance = axcenity_script::LuaInstance(instance);
            self.fire(world, SignalKey::RowActivated(row), instance);
        }
    }

    /// The attribute row numbered `row` was edited on `instance` (the
    /// editor already set the attribute): fires its `Changed(instance,
    /// value)`.
    pub fn row_changed(&self, world: &mut World, row: u64, instance: InstanceId, value: &Value) {
        if !self.has_row(row) || !world.contains(instance) {
            return;
        }
        let lua = self.runtime.lua();
        let Ok(value) = axcenity_script::system::value_to_lua(lua, value) else {
            return;
        };
        let instance = axcenity_script::LuaInstance(instance);
        self.fire(world, SignalKey::RowChanged(row), (instance, value));
    }

    fn has_row(&self, row: u64) -> bool {
        self.state()
            .is_some_and(|state| state.rows.rows.contains_key(&row))
    }

    /// The user dragged the handle numbered `id` to `position`: moves it
    /// and fires its `Dragged(position)`, or its `DragEnded(position)` when
    /// the button was let go (`ended`).
    pub fn drag_handle(&self, world: &mut World, id: u64, position: Vector3, ended: bool) {
        let moved = match api::state(self.runtime.lua()) {
            Ok(mut state) => {
                let handles = &mut state.handles;
                match handles.handles.get_mut(&id) {
                    Some(handle) => {
                        handle.position = position;
                        handles.changed = true;
                        true
                    }
                    None => false,
                }
            }
            Err(_) => false,
        };
        if !moved {
            return;
        }
        let at = axcenity_script::system::LuaVector3(position);
        let key = if ended {
            SignalKey::HandleDragEnded(id)
        } else {
            SignalKey::HandleDragged(id)
        };
        self.fire(world, key, at);
    }

    /// Whether it took the Scene View's left button (`plugin:Activate(true)`).
    pub fn mouse_active(&self) -> bool {
        self.state().is_some_and(|state| state.mouse_active)
    }

    /// Takes the mouse back (another plugin or a tool took it), firing
    /// `plugin.Deactivation`.
    pub fn deactivate(&self, world: &mut World) {
        let was = match api::state(self.runtime.lua()) {
            Ok(mut state) => std::mem::take(&mut state.mouse_active),
            Err(_) => false,
        };
        if was {
            self.fire(world, SignalKey::Deactivation, ());
        }
    }

    /// Whether `task.wait` or `task.delay` is waiting for [`step`](Self::step).
    pub fn has_waiting(&self) -> bool {
        self.runtime.has_waiting()
    }

    /// The toolbar button numbered `button` was clicked.
    pub fn click(&self, world: &mut World, button: u32) {
        let enabled = self
            .toolbars()
            .iter()
            .any(|toolbar| toolbar.buttons.iter().any(|b| b.id == button && b.enabled));
        if enabled {
            self.fire(world, SignalKey::Click(button), ());
        }
    }

    /// The editor's selection is now `selection`: fires `SelectionChanged`
    /// if that's not what the plugin last knew.
    pub fn set_selection(&self, world: &mut World, selection: &[InstanceId]) {
        let changed = match api::state(self.runtime.lua()) {
            Ok(mut state) if state.selection != selection => {
                state.selection = selection.to_vec();
                true
            }
            _ => false,
        };
        if changed {
            self.fire(world, SignalKey::SelectionChanged, ());
        }
    }

    /// The mouse over the Scene View: where it points, and its events.
    pub fn mouse(&self, world: &mut World, input: MouseInput) {
        if let Ok(mut state) = api::state(self.runtime.lua()) {
            state.mouse = input.state;
        }
        if input.moved {
            self.fire(world, SignalKey::Move, ());
        }
        if input.pressed {
            self.fire(world, SignalKey::Button1Down, ());
        }
        if input.released {
            self.fire(world, SignalKey::Button1Up, ());
        }
    }

    /// A TextButton in one of its dock widgets was clicked: fires its
    /// `Activated`.
    pub fn activate_button(&self, world: &mut World, button: InstanceId) {
        self.runtime.fire_instance_event(world, button, "Activated");
    }

    /// Fires `MouseEnter` or `MouseLeave` on an element of its dock widgets.
    pub fn fire_ui_event(&self, world: &mut World, element: InstanceId, event: &'static str) {
        self.runtime.fire_instance_event(world, element, event);
    }

    /// Moves its clock on and resumes what's due.
    pub fn step(&self, world: &mut World, seconds: f64) {
        self.runtime.step(world, seconds);
    }

    /// Fires its handlers of `Changed`, `ValueChanged` (a Slider dragged in
    /// a dock widget) and the other change events for `changes`.
    pub fn fire_changes(&self, world: &mut World, changes: &[RecordedChange]) {
        self.runtime.fire_changes(world, changes);
    }

    /// What it asked for since the last call, in order.
    pub fn take_requests(&self) -> Vec<Request> {
        match api::state(self.runtime.lua()) {
            Ok(mut state) => std::mem::take(&mut state.requests),
            Err(_) => Vec::new(),
        }
    }

    /// Its settings, when `SetSetting` changed them since the last call.
    pub fn take_settings(&self) -> Option<Settings> {
        let mut state = api::state(self.runtime.lua()).ok()?;
        std::mem::take(&mut state.settings_changed).then(|| state.settings.clone())
    }

    /// What it printed and its errors since the last call. Errors start
    /// with the file and line: `plugins/align/init.lua:12: ...`.
    pub fn take_output(&self) -> Vec<OutputLine> {
        self.runtime.take_output()
    }

    /// Stops the plugin: its dock widgets' UI is destroyed. Dropping the
    /// plugin stops its code.
    pub fn stop(self, world: &mut World) {
        for widget in self.widgets() {
            let _ = world.destroy(widget.gui);
        }
    }

    fn fire(&self, world: &mut World, key: SignalKey, args: impl IntoLuaMulti) {
        let lua = self.runtime.lua();
        let Ok(args) = args.into_lua_multi(lua) else {
            return;
        };
        let callbacks: Vec<mlua::Function> = match api::state(lua) {
            Ok(mut state) => match state.handlers.get_mut(&key) {
                Some(handlers) => {
                    let callbacks = handlers.iter().map(|h| h.callback.clone()).collect();
                    handlers.retain(|handler| !handler.once);
                    callbacks
                }
                None => Vec::new(),
            },
            Err(_) => Vec::new(),
        };
        for callback in callbacks {
            self.runtime.run(
                world,
                &callback,
                MultiValue::from_iter(args.iter().cloned()),
            );
        }
    }
}

#[cfg(test)]
mod tests;
