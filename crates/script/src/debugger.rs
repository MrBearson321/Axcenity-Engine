//! The editor's debugger: breakpoints that pause Play on a line, the
//! paused script's locals, and Continue.
//!
//! Only the editor turns it on ([`install`], through
//! `ScriptRuntime::enable_debugger`), for its own Play. The player never
//! does, and scripts have no way to: nothing here is reachable from Lua.
//! Without it, the scheduler's calls into this module do nothing.
//!
//! **Pausing.** Installed, Lua calls a hook before every new line, beside
//! the watchdog's instruction count. When the line has a breakpoint and is
//! running in a coroutine the scheduler resumed, the hook **yields** that
//! coroutine before the line runs, as the watchdog does when it stops one.
//! The scheduler then holds it here instead of dropping it. While paused,
//! nothing else runs either: whatever the scheduler is asked to resume
//! (due waits, `task.delay`s, event handlers, Console commands) waits in
//! [`Debugger::held`], in order, and a coroutine that gets control back
//! (one that `task.spawn`ed the paused one) is held at its next line.
//! Continue resumes them all in that order. A coroutine resumed after a
//! hook yield carries on exactly where it stopped, without the hook firing
//! again for that line.
//!
//! **Where it can't pause.** A coroutine the script made itself
//! (`coroutine.wrap`) would hand the yield to the script, not the
//! scheduler, and Lua can't yield inside a function called from C (a
//! `table.sort` comparison). There the line runs, and Output says once that
//! the breakpoint was passed.
//!
//! **Locals.** Once the coroutine has yielded, its call stack is read
//! through Lua's C API (`lua_getlocal`), not the `debug` library, which is
//! never loaded. Values are shown as `tostring` would show them, except
//! tables, which are listed without calling `__tostring` or `__pairs`, so
//! inspecting runs no script code.

use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};
use std::ffi::CStr;

use mlua::debug::{Debug, DebugEvent};
use mlua::{Function, HookTriggers, Lua, MultiValue, Table, Thread, Value, VmState, ffi};

use crate::{OutputKind, push_output, scheduler, watchdog};

/// Breakpoints by script path, as the host passes them: line numbers
/// counting from 1.
pub type Breakpoints = BTreeMap<String, BTreeSet<usize>>;

/// The most frames and locals a pause lists, so a deep recursion or a huge
/// function can't make it slow.
const MAX_FRAMES: usize = 20;
const MAX_LOCALS: usize = 60;
/// How many entries of a table a local's value lists, and how long a value's
/// text gets before it's cut short.
const TABLE_ENTRIES: usize = 8;
const VALUE_CHARACTERS: usize = 120;

/// Where Play is paused, for the host to show.
#[derive(Clone, Debug, PartialEq)]
pub struct Pause {
    /// The script and line (counting from 1) of the breakpoint, which hasn't
    /// run yet.
    pub path: String,
    pub line: usize,
    /// The script functions on the paused coroutine's call stack, innermost
    /// (the breakpoint's) first.
    pub frames: Vec<PausedFrame>,
}

/// One script function on a paused call stack.
#[derive(Clone, Debug, PartialEq)]
pub struct PausedFrame {
    /// The function's name, if Lua knows it; `None` for a script's main code.
    pub function: Option<String>,
    pub path: String,
    pub line: usize,
    /// Its local variables visible at that line, in the order they were
    /// declared.
    pub locals: Vec<Local>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Local {
    pub name: String,
    /// The value as text: `12`, `"hello"`, `Door`, `{1, 2, open = true}`.
    pub value: String,
}

/// Lives in Lua's app data, where the hook and the scheduler can reach it.
#[derive(Default)]
struct Debugger {
    breakpoints: Breakpoints,
    /// The coroutines the scheduler is running, innermost last. Only the
    /// innermost can be paused: its yield goes back to the scheduler.
    running: Vec<Thread>,
    /// A coroutine the hook asked to yield, until the scheduler sees it did.
    holding: Option<Thread>,
    paused: Option<Stop>,
    /// What runs on Continue, in order: the paused coroutine first (with
    /// no arguments; Lua ignores them after a hook's yield), then the rest.
    held: VecDeque<(Thread, MultiValue)>,
    /// Breakpoints Output has said it couldn't pause at, so it says so once.
    reported: HashSet<(String, usize)>,
}

/// The breakpoint Play is paused at.
struct Stop {
    path: String,
    line: usize,
    thread: Thread,
    /// Set once the coroutine has really yielded and its frames were read.
    frames: Option<Vec<PausedFrame>>,
}

/// Turns the debugger on: every coroutine made from here on runs the line
/// hook as well as the watchdog's. Call it before any script starts.
pub fn install(lua: &Lua) -> mlua::Result<()> {
    lua.set_app_data(Debugger::default());
    let triggers = HookTriggers::new()
        .every_line()
        .every_nth_instruction(watchdog::CHECK_EVERY);
    lua.set_global_hook(triggers, |lua, debug| match debug.event() {
        DebugEvent::Line => on_line(lua, debug),
        _ => watchdog::check(lua, debug),
    })
}

/// Replaces every breakpoint. Does nothing without the debugger.
pub fn set_breakpoints(lua: &Lua, breakpoints: &Breakpoints) {
    if let Some(mut debugger) = lua.app_data_mut::<Debugger>() {
        debugger.breakpoints = breakpoints.clone();
    }
}

/// Where this side is paused, once the paused coroutine has stopped.
pub fn pause(lua: &Lua) -> Option<Pause> {
    let debugger = lua.app_data_ref::<Debugger>()?;
    let stop = debugger.paused.as_ref()?;
    Some(Pause {
        path: stop.path.clone(),
        line: stop.line,
        frames: stop.frames.clone()?,
    })
}

fn is_paused(lua: &Lua) -> bool {
    lua.app_data_ref::<Debugger>()
        .is_some_and(|debugger| debugger.paused.is_some())
}

/// Called by the scheduler before it resumes `thread`. While paused, the
/// thread waits for Continue (`None`); otherwise the arguments come back.
pub fn hold_while_paused(lua: &Lua, thread: &Thread, args: MultiValue) -> Option<MultiValue> {
    let Some(mut debugger) = lua.app_data_mut::<Debugger>() else {
        return Some(args);
    };
    if debugger.paused.is_none() {
        return Some(args);
    }
    debugger.held.push_back((thread.clone(), args));
    None
}

/// Called by the scheduler just before it resumes `thread`.
pub fn enter(lua: &Lua, thread: &Thread) {
    if let Some(mut debugger) = lua.app_data_mut::<Debugger>() {
        debugger.running.push(thread.clone());
    }
}

/// Called by the scheduler once `thread` has paused, finished or failed. If
/// the hook made it yield, it's held for Continue; if it's the one at the
/// breakpoint, its frames are read now.
pub fn leave(lua: &Lua, thread: &Thread) {
    let at_breakpoint = {
        let Some(mut debugger) = lua.app_data_mut::<Debugger>() else {
            return;
        };
        debugger.running.pop();
        if debugger.holding.as_ref() != Some(thread) {
            return;
        }
        if !thread.is_resumable() {
            // It finished or failed without yielding: the yield couldn't
            // happen where the hook asked.
            drop(debugger);
            hold_failed(lua, thread);
            return;
        }
        debugger.holding = None;
        let at_breakpoint = debugger
            .paused
            .as_ref()
            .is_some_and(|stop| stop.thread == *thread);
        if at_breakpoint {
            debugger.held.push_front((thread.clone(), MultiValue::new()));
        } else {
            debugger.held.push_back((thread.clone(), MultiValue::new()));
        }
        at_breakpoint
    };
    if at_breakpoint {
        let frames = read_frames(lua, thread);
        if let Some(mut debugger) = lua.app_data_mut::<Debugger>()
            && let Some(stop) = &mut debugger.paused
        {
            stop.frames = Some(frames);
        }
    }
}

/// Called by `task.wait` as `thread` is about to yield to wait. If the hook
/// had asked it to yield on this line, that yield never happened (the
/// line's code ran inside a C function), so it isn't held.
pub fn waiting(lua: &Lua, thread: &Thread) {
    let holding = lua
        .app_data_ref::<Debugger>()
        .is_some_and(|debugger| debugger.holding.as_ref() == Some(thread));
    if holding {
        hold_failed(lua, thread);
    }
}

/// `thread` didn't yield where the hook asked. If that was the breakpoint,
/// Play isn't paused after all, and Output says why once.
fn hold_failed(lua: &Lua, thread: &Thread) {
    let missed = {
        let Some(mut debugger) = lua.app_data_mut::<Debugger>() else {
            return;
        };
        debugger.holding = None;
        if !debugger
            .paused
            .as_ref()
            .is_some_and(|stop| stop.thread == *thread)
        {
            return;
        }
        let stop = debugger.paused.take().expect("checked above");
        debugger
            .reported
            .insert((stop.path.clone(), stop.line))
            .then_some((stop.path, stop.line))
    };
    if let Some((path, line)) = missed {
        report_missed(lua, &path, line);
    }
}

fn report_missed(lua: &Lua, path: &str, line: usize) {
    push_output(
        lua,
        OutputKind::Error,
        format!(
            "{path}:{line}: Play can't pause at this breakpoint, so the game carried on. The \
             line ran inside a coroutine the script made itself (coroutine.wrap or \
             coroutine.resume), or inside a function the engine calls that can't pause \
             (such as a table.sort comparison). Put the breakpoint where that code is \
             called, or start it with task.spawn."
        ),
    );
}

/// Resumes what the pause held, in order, unless one of them pauses again;
/// then the rest wait for the next Continue.
pub fn continue_from_pause(lua: &Lua) {
    let queue = {
        let Some(mut debugger) = lua.app_data_mut::<Debugger>() else {
            return;
        };
        debugger.paused = None;
        debugger.holding = None;
        std::mem::take(&mut debugger.held)
    };
    run_held(lua, queue);
}

/// Resumes anything left held while not paused: a breakpoint that couldn't
/// pause after all may have held others first. Called at each step.
pub fn release(lua: &Lua) {
    let queue = {
        let Some(mut debugger) = lua.app_data_mut::<Debugger>() else {
            return;
        };
        if debugger.paused.is_some() || debugger.held.is_empty() {
            return;
        }
        std::mem::take(&mut debugger.held)
    };
    run_held(lua, queue);
}

fn run_held(lua: &Lua, mut queue: VecDeque<(Thread, MultiValue)>) {
    while let Some((thread, args)) = queue.pop_front() {
        scheduler::resume(lua, &thread, args);
        if is_paused(lua) {
            // Paused again: the new pause's coroutines go first, then these.
            if let Some(mut debugger) = lua.app_data_mut::<Debugger>() {
                debugger.held.extend(queue);
            }
            return;
        }
    }
}

/// The line hook: pauses at a breakpoint, and holds coroutines that get
/// control while paused.
fn on_line(lua: &Lua, debug: &Debug) -> mlua::Result<VmState> {
    let Some(mut debugger) = lua.app_data_mut::<Debugger>() else {
        return Ok(VmState::Continue);
    };
    // The common case, checked first: nothing to stop for.
    if debugger.breakpoints.is_empty() && debugger.paused.is_none() && debugger.holding.is_none()
    {
        return Ok(VmState::Continue);
    }
    let current = lua.current_thread();
    if debugger.holding.as_ref() == Some(&current) {
        // Still running after the hook asked it to yield: it couldn't.
        drop(debugger);
        hold_failed(lua, &current);
        return Ok(VmState::Continue);
    }
    let pausable = debugger.running.last() == Some(&current);
    if debugger.paused.is_some() {
        if !pausable {
            return Ok(VmState::Continue);
        }
        debugger.holding = Some(current);
        return Ok(VmState::Yield);
    }
    let Some((path, line)) = script_line(debug) else {
        return Ok(VmState::Continue);
    };
    let on_breakpoint = debugger
        .breakpoints
        .get(path.as_str())
        .is_some_and(|lines| lines.contains(&line));
    if !on_breakpoint {
        return Ok(VmState::Continue);
    }
    if !pausable {
        if debugger.reported.insert((path.clone(), line)) {
            drop(debugger);
            report_missed(lua, &path, line);
        }
        return Ok(VmState::Continue);
    }
    debugger.paused = Some(Stop {
        path,
        line,
        thread: current.clone(),
        frames: None,
    });
    debugger.holding = Some(current);
    Ok(VmState::Yield)
}

/// The script path and line about to run, if it's script code. Scripts are
/// loaded with the name `@path`; the engine's Lua isn't.
fn script_line(debug: &Debug) -> Option<(String, usize)> {
    let source = debug.source();
    let path = source.source?.strip_prefix('@')?.to_owned();
    Some((path, debug.current_line()?))
}

/// The script functions on the call stack of `thread`, which has yielded.
///
/// The workspace's only `unsafe` code (decided in v2 Phase 4): mlua has no
/// safe way to read a coroutine's locals, and loading the `debug` library
/// would be unsafe too, with more reach. The player never gets here.
#[allow(unsafe_code)]
fn read_frames(lua: &Lua, thread: &Thread) -> Vec<PausedFrame> {
    let mut frames: Vec<(Option<String>, String, usize, Vec<String>)> = Vec::new();
    let from = thread.state();
    // SAFETY: `from` is the state of `thread`, which is alive (we hold it)
    // and suspended, so its stack can be read; `exec_raw` runs the closure
    // in a protected call on the interpreter's own state (`to`), and every
    // value `lua_getlocal` pushes on `from` is moved to `to` or popped, so
    // the paused coroutine's stack is left as it was. The values left on
    // `to` come back as `values`, one per name in `frames`, in order.
    let values = unsafe {
        lua.exec_raw::<MultiValue>((), |to| {
            let mut level = 0;
            while frames.len() < MAX_FRAMES {
                let mut record: ffi::lua_Debug = std::mem::zeroed();
                if ffi::lua_getstack(from, level, &mut record) == 0 {
                    break;
                }
                level += 1;
                if ffi::lua_getinfo(from, c"Sln".as_ptr(), &mut record) == 0
                    || record.source.is_null()
                {
                    continue;
                }
                let source = CStr::from_ptr(record.source).to_string_lossy();
                let Some(path) = source.strip_prefix('@') else {
                    continue;
                };
                let function = (!record.name.is_null())
                    .then(|| CStr::from_ptr(record.name).to_string_lossy().into_owned());
                let mut names = Vec::new();
                let mut n = 1;
                while names.len() < MAX_LOCALS {
                    if ffi::lua_checkstack(from, 1) == 0 || ffi::lua_checkstack(to, 1) == 0 {
                        break;
                    }
                    let name = ffi::lua_getlocal(from, &record, n);
                    if name.is_null() {
                        break;
                    }
                    n += 1;
                    let name = CStr::from_ptr(name).to_string_lossy().into_owned();
                    // `(for state)`, `(temporary)` and the like are Lua's own.
                    if name.starts_with('(') {
                        ffi::lua_pop(from, 1);
                        continue;
                    }
                    ffi::lua_xmove(from, to, 1);
                    names.push(name);
                }
                let line = usize::try_from(record.currentline).unwrap_or(0);
                frames.push((function, path.to_owned(), line, names));
            }
        })
    };
    let mut values = values.unwrap_or_default().into_iter();
    let tostring: Option<Function> = lua.globals().raw_get("tostring").ok();
    frames
        .into_iter()
        .map(|(function, path, line, names)| PausedFrame {
            function,
            path,
            line,
            locals: names
                .into_iter()
                .map(|name| Local {
                    name,
                    value: values
                        .next()
                        .map(|value| describe(&value, tostring.as_ref(), true))
                        .unwrap_or_default(),
                })
                .collect(),
        })
        .collect()
}

/// A value as text. Tables are listed (one level deep at the top, `{…}`
/// below) without their metamethods; everything else goes through Lua's
/// own `tostring`, which for engine values is the engine's Rust code.
fn describe(value: &Value, tostring: Option<&Function>, top: bool) -> String {
    let text = match value {
        Value::String(text) => format!("{:?}", text.to_string_lossy()),
        Value::Table(table) if top => describe_table(table, tostring),
        Value::Table(_) => "{…}".to_owned(),
        _ => tostring
            .and_then(|tostring| tostring.call::<String>(value.clone()).ok())
            .unwrap_or_else(|| value.type_name().to_owned()),
    };
    shorten(text)
}

fn describe_table(table: &Table, tostring: Option<&Function>) -> String {
    let mut parts = Vec::new();
    let mut count = 0;
    // The array part comes first from `next`, so `{1, 2, 3}` reads as written.
    let mut next_index = 1;
    for pair in table.pairs::<Value, Value>() {
        let Ok((key, value)) = pair else {
            break;
        };
        count += 1;
        if parts.len() >= TABLE_ENTRIES {
            continue;
        }
        let value = describe(&value, tostring, false);
        match key {
            Value::Integer(index) if index == next_index => {
                next_index += 1;
                parts.push(value);
            }
            Value::String(name)
                if name
                    .to_str()
                    .is_ok_and(|name| is_identifier(&name)) =>
            {
                parts.push(format!("{} = {value}", name.to_string_lossy()));
            }
            key => parts.push(format!("[{}] = {value}", describe(&key, tostring, false))),
        }
    }
    if count > parts.len() {
        parts.push(format!("… {} more", count - parts.len()));
    }
    format!("{{{}}}", parts.join(", "))
}

fn is_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn shorten(text: String) -> String {
    if text.chars().count() <= VALUE_CHARACTERS {
        return text;
    }
    let mut short: String = text.chars().take(VALUE_CHARACTERS - 1).collect();
    short.push('…');
    short
}
