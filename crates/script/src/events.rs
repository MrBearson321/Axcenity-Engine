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
//!
//! A handler belongs to the script that connected it, and runs as that
//! script, so reloading the script disconnects it ([`Connections::forget_script`]).

use std::collections::BTreeMap;

use axcenity_core::InstanceId;
use mlua::{
    AppDataRefMut, Function, IntoLuaMulti, Lua, MetaMethod, UserData, UserDataFields,
    UserDataMethods, Value,
};

use crate::instance;
use crate::types::type_name;
use crate::{scheduler, tween};

/// What an event belongs to: an Instance (`part.Touched`), or an engine
/// object that isn't one (`UserInputService.InputBegan`, `RunService.Heartbeat`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Owner {
    Instance(InstanceId),
    UserInputService,
    RunService,
    /// A Tween, by its number (see `tween`).
    Tween(u64),
    /// `CollectionService`'s tag signals.
    CollectionService,
    /// `LevelService.LevelLoaded` (Phase 48).
    LevelService,
    /// `NetworkService.ConnectionFailed` (Phase 63).
    NetworkService,
    /// `QuestService`'s quest events (Phase 77).
    QuestService,
}

/// Which event of its owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Event {
    /// An event by its name, such as `Touched` or `Changed`.
    Named(&'static str),
    /// `instance:GetPropertyChangedSignal(property)`: fires when that one
    /// property changes.
    PropertyChanged(&'static str),
    /// `track:GetMarkerReachedSignal(name)`: fires when an AnimationTrack
    /// passes the event with that name, by its number in [`EventNames`].
    Marker(u32),
    /// `instance:GetAttributeChangedSignal(name)`, by the attribute's
    /// number in [`EventNames`].
    AttributeChanged(u32),
    /// `CollectionService:GetInstanceAddedSignal(tag)`, by the tag's number.
    TagAdded(u32),
    /// `CollectionService:GetInstanceRemovedSignal(tag)`, by the tag's number.
    TagRemoved(u32),
}

/// The marker, attribute and tag names signals were asked for, so an
/// [`Event`] can stay a small `Copy` value.
#[derive(Default)]
pub struct EventNames(Vec<String>);

/// The number of `name`, giving it one the first time.
pub fn name_id(lua: &Lua, name: &str) -> u32 {
    if lua.app_data_ref::<EventNames>().is_none() {
        lua.set_app_data(EventNames::default());
    }
    let mut names = lua.app_data_mut::<EventNames>().expect("made just above");
    match names.0.iter().position(|known| known == name) {
        Some(index) => index as u32,
        None => {
            names.0.push(name.to_owned());
            (names.0.len() - 1) as u32
        }
    }
}

/// The number of `name`, if a signal was ever asked for with it.
pub fn known_name_id(lua: &Lua, name: &str) -> Option<u32> {
    lua.app_data_ref::<EventNames>()
        .and_then(|names| names.0.iter().position(|known| known == name))
        .map(|index| index as u32)
}

/// Fires `owner`'s `GetMarkerReachedSignal(name)` handlers, with the name.
pub fn fire_marker(lua: &Lua, owner: Owner, name: &str) {
    let id = known_name_id(lua, name);
    if let Some(id) = id {
        fire_handlers(lua, (owner, Event::Marker(id)), None, name.to_owned());
    }
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
    /// the order they run in. A `BTreeMap` so all of one owner's events sit
    /// together, for [`Connections::forget_owner`].
    handlers: BTreeMap<EventKey, Vec<Handler>>,
    /// Owners that were already gone when a handler was connected, so no
    /// [`forget_owner`](Connections::forget_owner) is coming for them. Looked
    /// at once, at the next frame step.
    connected_while_gone: Vec<Owner>,
}

struct Handler {
    id: u64,
    callback: Function,
    /// The World's next change number when this was connected: change
    /// events only run it for changes numbered this or higher.
    since: u64,
    /// The script that connected it (see `scheduler`).
    script: Option<InstanceId>,
}

impl Connections {
    /// How many events have a handler.
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.handlers.len()
    }

    fn connect(
        &mut self,
        key: EventKey,
        callback: Function,
        since: u64,
        script: Option<InstanceId>,
    ) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.handlers.entry(key).or_default().push(Handler {
            id,
            callback,
            since,
            script,
        });
        id
    }

    /// Disconnects every handler `script` connected.
    pub fn forget_script(&mut self, script: InstanceId) {
        self.handlers.retain(|_, handlers| {
            handlers.retain(|handler| handler.script != Some(script));
            !handlers.is_empty()
        });
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
        // Connected in order, so sorted by `id`.
        self.handlers.get(&key).is_some_and(|handlers| {
            handlers
                .binary_search_by_key(&id, |handler| handler.id)
                .is_ok()
        })
    }

    /// Drops every handler of `owner`, so their functions (and whatever
    /// those functions use) can be freed. Finds them by key, so this
    /// doesn't depend on how many handlers there are elsewhere.
    pub fn forget_owner(&mut self, owner: Owner) {
        // `Event::Named("")` sorts before every other event.
        let keys: Vec<EventKey> = self
            .handlers
            .range((owner, Event::Named(""))..)
            .map(|(key, _)| *key)
            .take_while(|(other, _)| *other == owner)
            .collect();
        for key in keys {
            self.handlers.remove(&key);
        }
    }

    /// The owners noted since the last call as gone when connected to.
    pub fn take_connected_while_gone(&mut self) -> Vec<Owner> {
        std::mem::take(&mut self.connected_while_gone)
    }

    /// Drops the handlers of every Instance that doesn't exist, for when
    /// the whole World was replaced and nothing says what's gone.
    pub fn forget_missing_instances(&mut self, exists: impl Fn(InstanceId) -> bool) {
        self.handlers.retain(|(owner, _), _| match owner {
            Owner::Instance(instance) => exists(*instance),
            _ => true,
        });
    }
}

fn connections_mut(lua: &Lua) -> mlua::Result<AppDataRefMut<'_, Connections>> {
    lua.app_data_mut::<Connections>()
        .ok_or_else(|| mlua::Error::runtime("events aren't available"))
}

/// Runs every handler connected to `event` of `owner`, each in a coroutine
/// of its own (see `handler_threads`), in the order they were connected. A handler that waits pauses
/// only itself, and an error in one is reported without stopping the others.
pub fn fire(lua: &Lua, owner: Owner, event: &'static str, args: impl IntoLuaMulti) {
    fire_handlers(lua, (owner, Event::Named(event)), None, args);
}

/// Like [`fire`], for any kind of event.
pub fn fire_event(lua: &Lua, owner: Owner, event: Event, args: impl IntoLuaMulti) {
    fire_handlers(lua, (owner, event), None, args);
}

/// Like [`fire`], for change number `change` of the World: only handlers
/// connected before that change happened run.
pub fn fire_change(lua: &Lua, owner: Owner, event: Event, change: u64, args: impl IntoLuaMulti) {
    fire_handlers(lua, (owner, event), Some(change), args);
}

fn fire_handlers(lua: &Lua, key: EventKey, change: Option<u64>, args: impl IntoLuaMulti) {
    // Take a list first and let go of `Connections`, because a handler may
    // connect or disconnect handlers while it runs.
    let handlers: Vec<(u64, Function, Option<InstanceId>)> = match lua.app_data_ref::<Connections>()
    {
        Some(connections) => match connections.handlers.get(&key) {
            Some(handlers) => handlers
                .iter()
                .filter(|handler| change.is_none_or(|change| handler.since <= change))
                .map(|handler| (handler.id, handler.callback.clone(), handler.script))
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
    for (id, callback, script) in handlers {
        // An earlier handler may have disconnected this one.
        let connected = lua
            .app_data_ref::<Connections>()
            .is_some_and(|connections| connections.is_connected(key, id));
        if !connected {
            continue;
        }
        crate::handler_threads::run(lua, callback, args.clone(), script);
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
            let (since, gone) = {
                let world = instance::world(lua)?;
                let gone = match this.owner {
                    Owner::Instance(id) => !world.contains(id),
                    Owner::Tween(id) => !tween::is_alive(lua, id),
                    _ => false,
                };
                (world.next_change_number(), gone)
            };
            let script = scheduler::running_script(lua);
            let mut connections = connections_mut(lua)?;
            let id = connections.connect(key, callback, since, script);
            if gone {
                connections.connected_while_gone.push(this.owner);
            }
            Ok(LuaConnection { key, id })
        });
        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(match this.event {
                Event::Named(name) => format!("Signal {name}"),
                Event::PropertyChanged(property) => format!("Signal {property} changed"),
                Event::Marker(_) => "Signal MarkerReached".to_owned(),
                Event::AttributeChanged(_) => "Signal AttributeChanged".to_owned(),
                Event::TagAdded(_) => "Signal InstanceAdded".to_owned(),
                Event::TagRemoved(_) => "Signal InstanceRemoved".to_owned(),
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
