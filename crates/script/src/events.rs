//! Events (signals) such as `part.Touched`, and the handlers connected to them.
//!
//! `part.Touched` gives a [`LuaSignal`]: just its [`Owner`] (here the part)
//! and the event name.
//! `signal:Connect(handler)` stores the handler in [`Connections`] (in Lua's
//! app data) and returns a [`LuaConnection`], which `:Disconnect()` removes.
//! When the engine fires an event, every handler runs as its own coroutine,
//! so one that calls `task.wait` pauses only itself.
//!
//! Change events (`Changed`, `GetPropertyChangedSignal`, `ChildAdded`,
//! `ChildRemoved`) come from the World's change log (see `changes`), after
//! the change happened. Each handler remembers the World's next change
//! number when it was connected, so it never hears about a change from
//! before it was connected.

use std::collections::BTreeMap;

use axcenity_core::InstanceId;
use mlua::{
    AppDataRefMut, Function, IntoLuaMulti, Lua, MetaMethod, UserData, UserDataFields,
    UserDataMethods, Value,
};

use crate::errors::error_message;
use crate::instance;
use crate::types::type_name;
use crate::{OutputKind, push_output, scheduler};

/// What an event belongs to: an Instance (`part.Touched`), or an engine
/// object that isn't one (`UserInputService.InputBegan`, `RunService.Heartbeat`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Owner {
    Instance(InstanceId),
    UserInputService,
    RunService,
}

/// Which event of its owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Event {
    /// An event by its name, such as `Touched` or `Changed`.
    Named(&'static str),
    /// `instance:GetPropertyChangedSignal(property)`: fires when that one
    /// property changes.
    PropertyChanged(&'static str),
}

/// One event of one owner, such as `Touched` on a Part.
type EventKey = (Owner, Event);

/// Every connected handler. Lives in Lua's app data, so that `:Connect` and
/// `:Disconnect` can reach it.
#[derive(Default)]
pub struct Connections {
    /// Counts up, so every connection has its own number.
    next_id: u64,
    /// The handlers of each event, in the order they were connected, which is
    /// the order they run in. A `BTreeMap` so pruning visits them in order too.
    handlers: BTreeMap<EventKey, Vec<Handler>>,
}

struct Handler {
    id: u64,
    callback: Function,
    /// The World's next change number when this was connected: change
    /// events only run it for changes numbered this or higher.
    since: u64,
}

impl Connections {
    fn connect(&mut self, key: EventKey, callback: Function, since: u64) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.handlers.entry(key).or_default().push(Handler {
            id,
            callback,
            since,
        });
        id
    }

    fn disconnect(&mut self, key: EventKey, id: u64) {
        if let Some(handlers) = self.handlers.get_mut(&key) {
            handlers.retain(|handler| handler.id != id);
            if handlers.is_empty() {
                self.handlers.remove(&key);
            }
        }
    }

    fn is_connected(&self, key: EventKey, id: u64) -> bool {
        self.handlers
            .get(&key)
            .is_some_and(|handlers| handlers.iter().any(|handler| handler.id == id))
    }

    /// Drops the handlers of Instances that no longer exist, so their
    /// functions (and whatever those functions use) can be freed.
    pub fn forget(&mut self, exists: impl Fn(InstanceId) -> bool) {
        self.handlers.retain(|(owner, _), _| match owner {
            Owner::Instance(instance) => exists(*instance),
            Owner::UserInputService | Owner::RunService => true,
        });
    }
}

fn connections_mut(lua: &Lua) -> mlua::Result<AppDataRefMut<'_, Connections>> {
    lua.app_data_mut::<Connections>()
        .ok_or_else(|| mlua::Error::runtime("events aren't available"))
}

/// Runs every handler connected to `event` of `owner`, each in a new
/// coroutine, in the order they were connected. A handler that waits pauses
/// only itself, and an error in one is reported without stopping the others.
pub fn fire(lua: &Lua, owner: Owner, event: &'static str, args: impl IntoLuaMulti) {
    fire_handlers(lua, (owner, Event::Named(event)), None, args);
}

/// Like [`fire`], for change number `change` of the World: only handlers
/// connected before that change happened run.
pub fn fire_change(lua: &Lua, owner: Owner, event: Event, change: u64, args: impl IntoLuaMulti) {
    fire_handlers(lua, (owner, event), Some(change), args);
}

fn fire_handlers(lua: &Lua, key: EventKey, change: Option<u64>, args: impl IntoLuaMulti) {
    // Take a list first and let go of `Connections`, because a handler may
    // connect or disconnect handlers while it runs.
    let handlers: Vec<(u64, Function)> = match lua.app_data_ref::<Connections>() {
        Some(connections) => match connections.handlers.get(&key) {
            Some(handlers) => handlers
                .iter()
                .filter(|handler| change.is_none_or(|change| handler.since <= change))
                .map(|handler| (handler.id, handler.callback.clone()))
                .collect(),
            None => return,
        },
        None => return,
    };
    if handlers.is_empty() {
        return;
    }
    let Ok(args) = args.into_lua_multi(lua) else {
        return;
    };
    for (id, callback) in handlers {
        // An earlier handler may have disconnected this one.
        let connected = lua
            .app_data_ref::<Connections>()
            .is_some_and(|connections| connections.is_connected(key, id));
        if !connected {
            continue;
        }
        match lua.create_thread(callback) {
            Ok(thread) => scheduler::resume(lua, &thread, args.clone()),
            Err(err) => push_output(lua, OutputKind::Error, error_message(&err)),
        }
    }
}

/// An event, as scripts see it: `part.Touched`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LuaSignal {
    pub owner: Owner,
    pub event: Event,
}

impl LuaSignal {
    /// The event called `event`, such as `Touched`.
    pub fn named(owner: Owner, event: &'static str) -> Self {
        Self {
            owner,
            event: Event::Named(event),
        }
    }
}

impl UserData for LuaSignal {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("Connect", |lua, this, callback: Value| {
            let Value::Function(callback) = callback else {
                return Err(mlua::Error::runtime(format!(
                    "Connect takes a function, not a {}",
                    type_name(lua, &callback)
                )));
            };
            let key = (this.owner, this.event);
            // Scripts only run while the World is lent to Lua, so it's
            // always there.
            let since = instance::world(lua)?.next_change_number();
            let id = connections_mut(lua)?.connect(key, callback, since);
            Ok(LuaConnection { key, id })
        });
        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(match this.event {
                Event::Named(name) => format!("Signal {name}"),
                Event::PropertyChanged(property) => format!("Signal {property} changed"),
            })
        });
        // `part.Touched == part.Touched` is true, although each read makes a new value.
        methods.add_meta_method(MetaMethod::Eq, |_, this, other: Value| {
            Ok(match other {
                Value::UserData(data) => data.borrow::<LuaSignal>().is_ok_and(|o| *o == *this),
                _ => false,
            })
        });
    }
}

/// What `:Connect` returns, for disconnecting the handler later.
pub struct LuaConnection {
    key: EventKey,
    id: u64,
}

impl UserData for LuaConnection {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("Connected", |lua, this| {
            Ok(lua
                .app_data_ref::<Connections>()
                .is_some_and(|connections| connections.is_connected(this.key, this.id)))
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // Disconnecting twice is harmless, as in Roblox.
        methods.add_method("Disconnect", |lua, this, ()| {
            connections_mut(lua)?.disconnect(this.key, this.id);
            Ok(())
        });
        methods.add_meta_method(MetaMethod::ToString, |_, _, ()| Ok("Connection"));
    }
}
