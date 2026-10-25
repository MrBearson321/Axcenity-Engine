//! The watchdog: stops a script that runs too long without pausing, such
//! as `while true do end`, so a mistake can't freeze the game or the editor.
//!
//! Luau calls an interrupt at every function call and every loop iteration,
//! in every coroutine. Every [`CHECK_EVERY`] interrupts, the interrupt
//! compares the time since the scheduler last resumed a coroutine with the
//! limit. Pausing (`task.wait`) hands control back to the scheduler, so the
//! next resume starts a fresh time budget: long work that pauses now and
//! then is never stopped.
//!
//! Once the budget is used up, the interrupt **pauses** the running
//! coroutine for good: it yields, and nothing resumes it again. The
//! scheduler then reports an error naming the script and line. A yield can't
//! be caught by `pcall` and runs no error handlers, so a script can't keep
//! itself running. The scripts' `coroutine` functions (see
//! [`coroutine_functions`]) treat a stopped coroutine as dead, so no script
//! can resume or close it either.
//!
//! Some code can't yield: Lua called from inside a C function, such as a
//! `table.sort` comparator. There the next interrupt raises the error
//! instead, and the engine's `pcall` and `xpcall` pass it on rather than
//! catch it (see `errors`). Raising is the second choice because an error
//! runs the script's error handlers on its way out.
//!
//! The budget stays used up until the scheduler gets control back. When the
//! watchdog stops a coroutine the script made itself, the task running it
//! (the coroutine the scheduler resumed) may run [`CHECK_EVERY`] more
//! interrupts of its own, enough to finish what it was doing, and is then
//! stopped too (`while true do coroutine.resume(...) end` would otherwise
//! loop forever, each check landing in a new coroutine). Only a coroutine
//! that `task.spawn`ed it gets a fresh budget: the scheduler started that
//! one, so it's a separate task.

use std::time::{Duration, Instant};

use mlua::debug::Debug;
use mlua::{Function, Lua, Table, Thread, VmState};

/// How long a coroutine may run without pausing (decided in Phase 15).
pub const DEFAULT_LIMIT: Duration = Duration::from_secs(3);

/// How many interrupts pass between reads of the clock, and how many a task
/// may still run after a coroutine it resumed was stopped.
const CHECK_EVERY: u32 = 1000;

/// Lives in Lua's app data, where the interrupt and the scheduler can reach it.
pub struct Watchdog {
    limit: Duration,
    /// When the time budget started: at the outermost resume by the
    /// scheduler, or when a `task.spawn`ed coroutine was stopped.
    started: Option<Instant>,
    /// The coroutines the scheduler is running, innermost last. `task.spawn`
    /// resumes a new coroutine while the one that called it is still
    /// running, and both count against the same budget.
    tasks: Vec<Thread>,
    /// The coroutines the watchdog stopped in this run.
    stopped: Vec<Stopped>,
    /// Interrupts since the clock was last read.
    interrupts: u32,
    /// A task whose budget is used up, and how many more interrupts it may
    /// run before it's stopped.
    finishing: Option<(Thread, u32)>,
    /// Every coroutine the watchdog ever stopped, with its error, for the
    /// scripts' `coroutine` functions. Its keys are weak, so Lua still
    /// frees the coroutines.
    ever_stopped: Table,
}

struct Stopped {
    thread: Thread,
    message: String,
    /// Whether the error has been added to the output, so it's reported
    /// once even if it passes through several coroutines.
    reported: bool,
}

/// Starts watching every coroutine that `lua` runs.
pub fn install(lua: &Lua) -> mlua::Result<()> {
    let ever_stopped = lua.create_table()?;
    let weak_keys = lua.create_table()?;
    weak_keys.raw_set("__mode", "k")?;
    ever_stopped.set_metatable(Some(weak_keys))?;
    lua.set_app_data(Watchdog {
        limit: DEFAULT_LIMIT,
        started: None,
        tasks: Vec::new(),
        stopped: Vec::new(),
        interrupts: 0,
        finishing: None,
        ever_stopped,
    });
    lua.set_interrupt(check);
    Ok(())
}

pub fn set_limit(lua: &Lua, limit: Duration) {
    if let Some(mut watchdog) = lua.app_data_mut::<Watchdog>() {
        watchdog.limit = limit;
    }
}

/// Called by the scheduler just before it resumes `thread`.
pub fn enter(lua: &Lua, thread: &Thread) {
    if let Some(mut watchdog) = lua.app_data_mut::<Watchdog>() {
        if watchdog.tasks.is_empty() {
            watchdog.started = Some(Instant::now());
            watchdog.stopped.clear();
        }
        watchdog.tasks.push(thread.clone());
    }
}

/// Called by the scheduler after the coroutine it resumed has paused,
/// finished or failed.
pub fn leave(lua: &Lua) {
    if let Some(mut watchdog) = lua.app_data_mut::<Watchdog>() {
        let task = watchdog.tasks.pop();
        if watchdog
            .finishing
            .as_ref()
            .is_some_and(|(thread, _)| Some(thread) == task.as_ref())
        {
            watchdog.finishing = None;
        }
        if watchdog.tasks.is_empty() {
            watchdog.started = None;
            watchdog.stopped.clear();
        } else if !watchdog.stopped.is_empty() {
            // A `task.spawn`ed coroutine was stopped; the one that spawned
            // it carries on with a fresh budget.
            watchdog.started = Some(Instant::now());
        }
    }
}

/// The watchdog's error, if it has stopped the coroutine running now.
pub fn stopped(lua: &Lua) -> Option<String> {
    let watchdog = lua.app_data_ref::<Watchdog>()?;
    if watchdog.stopped.is_empty() {
        return None;
    }
    let current = lua.current_thread();
    watchdog
        .stopped
        .iter()
        .find(|stopped| stopped.thread == current)
        .map(|stopped| stopped.message.clone())
}

/// The error to report for `thread`, which the scheduler resumed and which
/// has just paused: `Some` if the watchdog paused it (once per thread).
pub fn paused_by_watchdog(lua: &Lua, thread: &Thread) -> Option<String> {
    let mut watchdog = lua.app_data_mut::<Watchdog>()?;
    let stopped = watchdog
        .stopped
        .iter_mut()
        .find(|stopped| stopped.thread == *thread)?;
    // `replace` stores `true` and returns what was there before.
    (!std::mem::replace(&mut stopped.reported, true)).then(|| stopped.message.clone())
}

/// Whether an error with `message` should be added to the output: always,
/// unless it's the watchdog's error and that was reported already.
pub fn should_report(lua: &Lua, message: &str) -> bool {
    let Some(mut watchdog) = lua.app_data_mut::<Watchdog>() else {
        return true;
    };
    // `ends_with`, in case Lua put a location in front.
    match watchdog
        .stopped
        .iter_mut()
        .find(|stopped| message.ends_with(&stopped.message))
    {
        Some(stopped) => !std::mem::replace(&mut stopped.reported, true),
        None => true,
    }
}

/// The interrupt Luau calls at every call and loop iteration. `Yield`
/// pauses the running coroutine; an error is raised in the running code.
fn check(lua: &Lua) -> mlua::Result<VmState> {
    // Still running after the interrupt asked it to pause, so it couldn't
    // (it's inside a C function): raise the error instead.
    if let Some(message) = stopped(lua) {
        return Err(mlua::Error::runtime(message));
    }
    // The block ends the borrow of the watchdog before the stack is read.
    let seconds = {
        let Some(mut watchdog) = lua.app_data_mut::<Watchdog>() else {
            return Ok(VmState::Continue);
        };
        let limit = watchdog.limit.as_secs_f64();
        let mut finished = false;
        if let Some((task, left)) = &mut watchdog.finishing
            && *task == lua.current_thread()
        {
            finished = *left == 0;
            *left = left.saturating_sub(1);
        }
        if finished {
            watchdog.finishing = None;
            limit
        } else {
            watchdog.interrupts += 1;
            if watchdog.interrupts < CHECK_EVERY {
                return Ok(VmState::Continue);
            }
            watchdog.interrupts = 0;
            // Lua code can also run outside a resume (while the engine sets
            // up, say); that gets a budget from its first check on.
            let started = *watchdog.started.get_or_insert_with(Instant::now);
            if started.elapsed() < watchdog.limit {
                return Ok(VmState::Continue);
            }
            // A coroutine the script made: its task finishes up, then stops.
            let current = lua.current_thread();
            if let Some(task) = watchdog.tasks.last().cloned()
                && task != current
                && watchdog.finishing.is_none()
            {
                watchdog.finishing = Some((task, CHECK_EVERY));
            }
            limit
        }
    };
    let text = format!(
        "this script ran for {seconds} seconds without pausing, so it was stopped. A loop \
         that never pauses freezes the game: put task.wait() inside long loops"
    );
    let message = match script_location(lua) {
        Some(location) => format!("{location}: {text}"),
        None => text,
    };
    if let Some(mut watchdog) = lua.app_data_mut::<Watchdog>() {
        let thread = lua.current_thread();
        watchdog.ever_stopped.raw_set(&thread, message.as_str())?;
        watchdog.stopped.push(Stopped {
            thread,
            message,
            reported: false,
        });
    }
    Ok(VmState::Yield)
}

/// `coroutine.resume`, `wrap`, `status` and `close` for scripts. A
/// coroutine the watchdog stopped is paused, not dead, so these make it
/// look dead: it can't be resumed or closed, and `resume` and `wrap` report
/// the watchdog's error when it's stopped while they run it. Errors come
/// back as plain strings (`clean`), as with `pcall`, and `wrap` doesn't add
/// a second location to messages that already have one.
const COROUTINE_FUNCTIONS: &str = r#"
local create, resume, status, close, error, clean, ever_stopped = ...

local function finish_resume(co, ok, ...)
    if not ok then
        return false, clean((...))
    end
    local message = ever_stopped[co]
    if message then
        return false, message
    end
    return true, ...
end

local function safe_resume(co, ...)
    local message = ever_stopped[co]
    if message then
        return false, "cannot resume dead coroutine"
    end
    return finish_resume(co, resume(co, ...))
end

local function finish_wrap(co, ok, ...)
    if not ok then
        error((...), 0)
    end
    local message = ever_stopped[co]
    if message then
        error(message, 0)
    end
    return ...
end

local function safe_wrap(f)
    local co = create(f)
    return function(...)
        if ever_stopped[co] or status(co) == "dead" then
            error("cannot resume dead coroutine", 2)
        end
        return finish_wrap(co, resume(co, ...))
    end
end

local function safe_status(co)
    if ever_stopped[co] then
        return "dead"
    end
    return status(co)
end

local function safe_close(co)
    local message = ever_stopped[co]
    if message then
        return false, message
    end
    return close(co)
end

return { resume = safe_resume, wrap = safe_wrap, status = safe_status, close = safe_close }
"#;

/// Creates the `coroutine` functions in [`COROUTINE_FUNCTIONS`], as a table
/// to copy into every script's `coroutine`. `clean` turns engine error
/// objects into plain strings (see `errors`).
pub fn coroutine_functions(lua: &Lua, clean: &Function) -> mlua::Result<Table> {
    let coroutine: Table = lua.globals().raw_get("coroutine")?;
    let ever_stopped = lua
        .app_data_ref::<Watchdog>()
        .map(|watchdog| watchdog.ever_stopped.clone())
        .ok_or_else(|| mlua::Error::runtime("the watchdog isn't running"))?;
    lua.load(COROUTINE_FUNCTIONS).set_name("=[engine]").call((
        coroutine.raw_get::<Function>("create")?,
        coroutine.raw_get::<Function>("resume")?,
        coroutine.raw_get::<Function>("status")?,
        coroutine.raw_get::<Function>("close")?,
        lua.globals().raw_get::<Function>("error")?,
        clean.clone(),
        ever_stopped,
    ))
}

/// `path:line` of the script code that's running: the function the
/// interrupt stopped, or if that's the engine's own Lua, the nearest script
/// code that called it.
fn script_location(lua: &Lua) -> Option<String> {
    // `map_while` stops at the bottom of the call stack.
    (0..)
        .map_while(|level| lua.inspect_stack(level, location))
        .flatten()
        .next()
}

/// `path:line` of one function on the call stack, if it's script code.
/// Scripts are loaded with the name `@path`; the engine's Lua isn't.
fn location(debug: &Debug) -> Option<String> {
    let source = debug.source();
    let path = source.source?.strip_prefix('@')?.to_owned();
    let line = debug.current_line()?;
    Some(format!("{path}:{line}"))
}
