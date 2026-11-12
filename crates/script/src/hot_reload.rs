//! Hot reload that keeps a script's state, the way s&box hotloads game code
//! (unified Phase 89). Only the editor's Play turns it on
//! ([`ScriptRuntime::enable_hot_reload`](crate::ScriptRuntime::enable_hot_reload)),
//! handing over the function that wraps a script's top-level locals
//! (`axcenity_blocks::hot_reload::wrap`, which also says what the wrapped
//! code looks like). Without it, nothing here runs and scripts load as
//! they're written: the player never turns it on.
//!
//! When a script runs again after a save:
//! 1. [`snapshot`] asks the old run's locals for their values, through the
//!    `__axcenity_watch` getters its wrapped code registered.
//! 2. The old run stops as before (handlers disconnected, threads dropped).
//! 3. The new run's wrapped locals ask `__axcenity_kept` whether a value
//!    was kept for them: the same key, an initializer written the same
//!    way, and a value that isn't code. Then the local takes the old value
//!    and its initializer doesn't run.
//! 4. [`finish`] forgets the kept values and says which names were kept.
//!
//! Functions, threads and connections are never kept: they're the old
//! code, so the new initializer runs. A ModuleScript's table keeps its
//! identity: the new table's fields are copied into the old one
//! ([`merge_into`]), so scripts that hold it see the new functions.

use std::collections::{BTreeSet, HashMap};

use axcenity_core::InstanceId;
use mlua::{Function, Lua, MultiValue, Table, Value};

use crate::events::LuaConnection;

/// The names the wrapped code calls. Keep in step with
/// `axcenity_blocks::hot_reload`.
const KEPT: &str = "__axcenity_kept";
const VALUES: &str = "__axcenity_values";
const WATCH: &str = "__axcenity_watch";

/// Rewrites a script's source so its top-level locals can be kept, or
/// `None` to run it as written.
pub type Wrap = fn(&str) -> Option<String>;

/// Lua app data on each side while hot reload is on.
pub struct HotReload {
    wrap: Wrap,
    /// Each running script's getters for its top-level locals.
    watchers: HashMap<InstanceId, Vec<Watcher>>,
    /// Values taken from a script's old run, by key, with the hash of the
    /// initializer they came from, for its next run.
    kept: HashMap<InstanceId, HashMap<String, (Value, String)>>,
    /// The values `__axcenity_kept` found, for `__axcenity_values`.
    pending: Vec<Value>,
    /// The names each script's new run kept so far.
    used: HashMap<InstanceId, BTreeSet<String>>,
}

struct Watcher {
    keys: Vec<String>,
    hash: String,
    get: Function,
}

/// Turns hot reload on for this side.
pub fn install(lua: &Lua, wrap: Wrap) {
    lua.set_app_data(HotReload {
        wrap,
        watchers: HashMap::new(),
        kept: HashMap::new(),
        pending: Vec::new(),
        used: HashMap::new(),
    });
}

/// Whether hot reload is on for this side.
pub fn enabled(lua: &Lua) -> bool {
    lua.app_data_ref::<HotReload>().is_some()
}

/// Gets `script`'s code ready to load into `env`: the wrapped source, or
/// `None` to load `source` as it is (hot reload off, or nothing to wrap).
pub fn prepare(
    lua: &Lua,
    script: InstanceId,
    source: &str,
    env: &Table,
) -> mlua::Result<Option<String>> {
    let Some(wrap) = lua.app_data_ref::<HotReload>().map(|hot| hot.wrap) else {
        return Ok(None);
    };
    let Some(wrapped) = wrap(source) else {
        return Ok(None);
    };
    env.raw_set(
        KEPT,
        lua.create_function(move |lua, (keys, hash): (Vec<String>, String)| {
            Ok(take_kept(lua, script, &keys, &hash))
        })?,
    )?;
    env.raw_set(
        VALUES,
        lua.create_function(|lua, ()| {
            let values = lua
                .app_data_mut::<HotReload>()
                .map(|mut hot| std::mem::take(&mut hot.pending))
                .unwrap_or_default();
            Ok(MultiValue::from_vec(values))
        })?,
    )?;
    env.raw_set(
        WATCH,
        lua.create_function(
            move |lua, (keys, hash, get): (Vec<String>, String, Function)| {
                if let Some(mut hot) = lua.app_data_mut::<HotReload>() {
                    hot.watchers
                        .entry(script)
                        .or_default()
                        .push(Watcher { keys, hash, get });
                }
                Ok(())
            },
        )?,
    )?;
    Ok(Some(wrapped))
}

/// `__axcenity_kept`: whether every key has a kept value from an
/// initializer written as `hash`. If so, the values wait in `pending`.
fn take_kept(lua: &Lua, script: InstanceId, keys: &[String], hash: &str) -> bool {
    let Some(mut hot) = lua.app_data_mut::<HotReload>() else {
        return false;
    };
    let Some(kept) = hot.kept.get(&script) else {
        return false;
    };
    let mut values = Vec::with_capacity(keys.len());
    for key in keys {
        match kept.get(key) {
            Some((value, from)) if from == hash => values.push(value.clone()),
            _ => return false,
        }
    }
    hot.pending = values;
    let used = hot.used.entry(script).or_default();
    for key in keys {
        used.insert(key.split('#').next().unwrap_or(key).to_owned());
    }
    true
}

/// Reads `script`'s top-level locals before it stops, so its next run can
/// keep them. A getter that fails (it shouldn't: it only returns locals)
/// keeps nothing for its locals.
pub fn snapshot(lua: &Lua, script: InstanceId) {
    let watchers = match lua.app_data_mut::<HotReload>() {
        Some(mut hot) => hot.watchers.remove(&script).unwrap_or_default(),
        None => return,
    };
    let mut kept = HashMap::new();
    for watcher in watchers {
        let Ok(values) = watcher.get.call::<MultiValue>(()) else {
            continue;
        };
        let values = values.into_vec();
        for (index, key) in watcher.keys.into_iter().enumerate() {
            let value = values.get(index).cloned().unwrap_or(Value::Nil);
            if keepable(&value) {
                kept.insert(key, (value, watcher.hash.clone()));
            }
        }
    }
    if let Some(mut hot) = lua.app_data_mut::<HotReload>() {
        hot.kept.insert(script, kept);
    }
}

/// Forgets what was kept for `script`'s new run, which has started, and
/// returns the names it kept, in order.
pub fn finish(lua: &Lua, script: InstanceId) -> Vec<String> {
    let Some(mut hot) = lua.app_data_mut::<HotReload>() else {
        return Vec::new();
    };
    hot.kept.remove(&script);
    hot.pending.clear();
    hot.used
        .remove(&script)
        .map(|names| names.into_iter().collect())
        .unwrap_or_default()
}

/// Forgets `script`'s getters: it stopped.
pub fn forget(lua: &Lua, script: InstanceId) {
    if let Some(mut hot) = lua.app_data_mut::<HotReload>() {
        hot.watchers.remove(&script);
    }
}

/// Whether a value can carry over to new code: data, Instances and other
/// values, but not code (functions, threads, connections) or nothing.
fn keepable(value: &Value) -> bool {
    match value {
        Value::Nil | Value::Function(_) | Value::Thread(_) | Value::LightUserData(_) => false,
        Value::UserData(data) => !data.is::<LuaConnection>(),
        _ => true,
    }
}

/// Makes `old` (a ModuleScript's table that scripts already hold) hold what
/// `new` holds: the new functions and fields, and none of the old ones the
/// new code dropped. Does nothing when they're the same table (the module
/// kept its table in a top-level local).
pub fn merge_into(old: &Table, new: &Table) -> mlua::Result<()> {
    if old == new {
        return Ok(());
    }
    let readonly = old.is_readonly();
    if readonly {
        old.set_readonly(false);
    }
    let stale: Vec<Value> = old
        .pairs::<Value, Value>()
        .filter_map(|pair| pair.ok().map(|(key, _)| key))
        .collect();
    for key in stale {
        old.raw_set(key, Value::Nil)?;
    }
    for pair in new.pairs::<Value, Value>() {
        let (key, value) = pair?;
        old.raw_set(key, value)?;
    }
    old.set_metatable(new.metatable())?;
    if readonly || new.is_readonly() {
        old.set_readonly(true);
    }
    Ok(())
}
