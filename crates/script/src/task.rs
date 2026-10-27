//! The `task` library: `task.wait`, `task.spawn`, `task.delay`,
//! `task.defer` and `task.cancel`. The coroutines they pause and start are
//! kept by the `scheduler`. `task.synchronize` and `task.desynchronize`
//! do nothing: every script runs on one thread.

use mlua::{Function, Lua, MultiValue, Table, Thread, Value};

use crate::scheduler::{self, scheduler_mut};
use crate::types::type_name;

/// `task.wait` must pause the script's coroutine, and only Lua code can do
/// that (mlua's Rust functions can't yield), so it's a short Lua function.
/// `schedule` is Rust: it checks `seconds` and tells the scheduler when to
/// resume, and the scheduler then resumes it with the seconds that passed.
const WAIT: &str = r#"
local running, yield, schedule = ...

return function(seconds)
    local thread, is_main = running()
    if is_main then
        error("task.wait can only pause a script", 2)
    end
    schedule(thread, seconds)
    return yield()
end
"#;

/// The functions in every script's `task` table. They hold no state of their
/// own, so one set is made when the runtime starts.
pub struct TaskFunctions {
    wait: Function,
    spawn: Function,
    delay: Function,
    defer: Function,
    cancel: Function,
    nothing: Function,
}

impl TaskFunctions {
    pub fn new(lua: &Lua) -> mlua::Result<Self> {
        // The real coroutine functions, not a script's copy it might have changed.
        let coroutine: Table = lua.globals().raw_get("coroutine")?;
        let schedule = lua.create_function(|lua, (thread, seconds): (Thread, Value)| {
            let seconds = seconds_arg(lua, "task.wait", seconds)?;
            crate::debugger::waiting(lua, &thread);
            scheduler_mut(lua)?.wait(thread, seconds);
            Ok(())
        })?;
        let wait = lua.load(WAIT).set_name("=[engine]").call((
            coroutine.raw_get::<Function>("running")?,
            coroutine.raw_get::<Function>("yield")?,
            schedule,
        ))?;

        let spawn = lua.create_function(|lua, (callback, args): (Value, MultiValue)| {
            let thread = new_thread(lua, "task.spawn", callback)?;
            scheduler::resume(lua, &thread, args);
            Ok(thread)
        })?;

        let delay = lua.create_function(
            |lua, (seconds, callback, args): (Value, Value, MultiValue)| {
                let seconds = seconds_arg(lua, "task.delay", seconds)?;
                let thread = new_thread(lua, "task.delay", callback)?;
                // `clone` makes a second handle to the same coroutine, one
                // for the scheduler and one to return.
                scheduler_mut(lua)?.delay(thread.clone(), seconds, args);
                Ok(thread)
            },
        )?;

        // Deferred work runs at the next frame step, after the code
        // running now has finished or paused.
        let defer = lua.create_function(|lua, (callback, args): (Value, MultiValue)| {
            let thread = match callback {
                Value::Thread(thread) => thread,
                other => new_thread(lua, "task.defer", other)?,
            };
            scheduler_mut(lua)?.delay(thread.clone(), 0.0, args);
            Ok(thread)
        })?;

        let close: Function = coroutine.raw_get("close")?;
        let cancel = lua.create_function(move |lua, thread: Value| {
            let Value::Thread(thread) = thread else {
                return Err(mlua::Error::runtime(format!(
                    "task.cancel takes a thread, such as one task.spawn or task.delay \
                     returned, not a {}",
                    type_name(lua, &thread)
                )));
            };
            if thread.is_running() {
                return Err(mlua::Error::runtime(
                    "task.cancel can't cancel the thread that's running; return from it instead",
                ));
            }
            scheduler_mut(lua)?.cancel(&thread);
            // A thread that has finished, or failed, is already closed.
            if thread.is_resumable() {
                close.call::<()>(thread)?;
            }
            Ok(())
        })?;

        let nothing = lua.create_function(|_, ()| Ok(()))?;

        Ok(Self {
            wait,
            spawn,
            delay,
            defer,
            cancel,
            nothing,
        })
    }

    /// `task.wait`, for other engine Lua that needs to wait.
    pub fn wait(&self) -> &Function {
        &self.wait
    }

    /// A new `task` table for one script, so a script that replaces
    /// `task.wait` only breaks itself.
    pub fn library(&self, lua: &Lua) -> mlua::Result<Table> {
        let task = lua.create_table()?;
        task.raw_set("wait", &self.wait)?;
        task.raw_set("spawn", &self.spawn)?;
        task.raw_set("delay", &self.delay)?;
        task.raw_set("defer", &self.defer)?;
        task.raw_set("cancel", &self.cancel)?;
        task.raw_set("synchronize", &self.nothing)?;
        task.raw_set("desynchronize", &self.nothing)?;
        Ok(task)
    }
}

/// A number of seconds for `task.wait` or `task.delay`. Nil, negative numbers
/// and NaN all mean 0: wait until the next frame.
fn seconds_arg(lua: &Lua, function: &str, value: Value) -> mlua::Result<f64> {
    let seconds = match value {
        Value::Nil => 0.0,
        Value::Integer(n) => n as f64,
        Value::Number(n) => n,
        other => {
            return Err(mlua::Error::runtime(format!(
                "{function} takes a number of seconds, not a {}",
                type_name(lua, &other)
            )));
        }
    };
    // `max` also turns NaN into 0.
    Ok(seconds.max(0.0))
}

/// A new coroutine that will run `callback`.
fn new_thread(lua: &Lua, function: &str, callback: Value) -> mlua::Result<Thread> {
    match callback {
        Value::Function(callback) => lua.create_thread(callback),
        other => Err(mlua::Error::runtime(format!(
            "{function} takes a function, not a {}",
            type_name(lua, &other)
        ))),
    }
}
