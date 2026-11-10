//! The editor's debugger: breakpoints that pause Play on a line, the
//! paused script's locals, and Continue.
//!
//! Only the editor turns it on ([`install`], through
//! `ScriptRuntime::enable_debugger`), for its own Play. The player never
//! does, and scripts have no way to: nothing here is reachable from Lua.
//! Without it, the scheduler's calls into this module do nothing.
//!
//! **Pausing.** Installed, every coroutine the scheduler resumes runs in
//! Luau's single-step mode: Luau calls a hook before every instruction. When
//! the instruction's line has a breakpoint and is running in a coroutine the
//! scheduler resumed, the hook **yields** that coroutine before the line
//! runs, as the watchdog does when it stops one.
//! The scheduler then holds it here instead of dropping it. While paused,
//! nothing else runs either: whatever the scheduler is asked to resume
//! (due waits, `task.delay`s, event handlers, Console commands) waits in
//! [`Debugger::held`], in order, and a coroutine that gets control back
//! (one that `task.spawn`ed the paused one) is held at its next line.
//! Continue resumes them all in that order. Luau runs the hook again for
//! the instruction a coroutine yielded at, so the debugger remembers where
//! each one stopped (a [`Spot`]) and lets it carry on until it reaches
//! another line in that function.
//!
//! **Where it can't pause.** A coroutine the script made itself
//! (`coroutine.wrap`) would hand the yield to the script, not the
//! scheduler, and Lua can't yield inside a function called from C (a
//! `table.sort` comparison). There the line runs, and Output says once that
//! the breakpoint was passed.
//!
//! **Locals.** Once the coroutine has yielded, its call stack is read
//! through Luau's C API (`lua_getlocal`), not the `debug` library, which is
//! never loaded. Local names are only in the bytecode at debug level 2, so
//! installing the debugger raises the compiler's debug level. Values are shown as `tostring` would show them, except
//! tables, which are listed without calling `__tostring` or `__pairs`, so
//! inspecting runs no script code.

use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};
use std::ffi::CStr;
use std::panic::{AssertUnwindSafe, catch_unwind};

use mlua::{Function, Lua, MultiValue, Table, Thread, Value, VmState, ffi};

use crate::{OutputKind, push_output, scheduler};

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
    held: VecDeque<Held>,
    /// Breakpoints Output has said it couldn't pause at, so it says so once.
    reported: HashSet<(String, usize)>,
    /// Where the hook asked coroutines to yield. They don't stop there again
    /// until they've moved on.
    spots: Vec<Spot>,
}

/// A coroutine held for Continue: what to resume it with, and the script
/// it belongs to (see `scheduler`).
type Held = (Thread, MultiValue, Option<axcenity_core::InstanceId>);

/// A line one coroutine was stopped at, in the function `depth` calls deep.
struct Spot {
    thread: Thread,
    path: String,
    line: usize,
    depth: i32,
}

/// Where the step hook is: the running coroutine's state, and its script
/// path, line and call depth, if it's script code.
struct Step {
    state: *mut ffi::lua_State,
    location: Option<(String, usize, i32)>,
}

/// The breakpoint Play is paused at.
struct Stop {
    path: String,
    line: usize,
    thread: Thread,
    /// Read by the hook, before the coroutine yields: afterwards Luau
    /// reports its innermost function one instruction early.
    frames: Vec<PausedFrame>,
    /// Set once the coroutine has really yielded.
    yielded: bool,
}

/// Turns the debugger on: every coroutine the scheduler resumes from here
/// on runs the step hook. Call it before any script is loaded, so scripts
/// are compiled with their local names.
pub fn install(lua: &Lua) -> mlua::Result<()> {
    lua.set_app_data(Debugger::default());
    lua.set_compiler(crate::sandbox::compiler().set_debug_level(2));
    set_step_hook(lua)
}

/// Whether [`install`] ran, so scripts compile with local names.
pub fn is_installed(lua: &Lua) -> bool {
    lua.app_data_ref::<Debugger>().is_some()
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
    let stop = debugger.paused.as_ref().filter(|stop| stop.yielded)?;
    Some(Pause {
        path: stop.path.clone(),
        line: stop.line,
        frames: stop.frames.clone(),
    })
}

fn is_paused(lua: &Lua) -> bool {
    lua.app_data_ref::<Debugger>()
        .is_some_and(|debugger| debugger.paused.is_some())
}

/// Called by the scheduler before it resumes `thread`. While paused, the
/// thread waits for Continue (`None`); otherwise the arguments come back.
pub fn hold_while_paused(lua: &Lua, thread: &Thread, args: MultiValue) -> Option<MultiValue> {
    let script = scheduler::running_script(lua);
    let Some(mut debugger) = lua.app_data_mut::<Debugger>() else {
        return Some(args);
    };
    if debugger.paused.is_none() {
        return Some(args);
    }
    debugger.held.push_back((thread.clone(), args, script));
    None
}

/// Called by the scheduler just before it resumes `thread`.
pub fn enter(lua: &Lua, thread: &Thread) {
    if let Some(mut debugger) = lua.app_data_mut::<Debugger>() {
        single_step(thread);
        debugger.running.push(thread.clone());
    }
}

/// Called by the scheduler once `thread` has paused, finished or failed. If
/// the hook made it yield, it's held for Continue; if it's the one at the
/// breakpoint, Play is now paused.
pub fn leave(lua: &Lua, thread: &Thread) {
    let script = scheduler::running_script(lua);
    let Some(mut debugger) = lua.app_data_mut::<Debugger>() else {
        return;
    };
    debugger.running.pop();
    if !thread.is_resumable() {
        debugger.spots.retain(|spot| spot.thread != *thread);
    }
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
    match &mut debugger.paused {
        Some(stop) if stop.thread == *thread => {
            stop.yielded = true;
            debugger
                .held
                .push_front((thread.clone(), MultiValue::new(), script));
        }
        _ => debugger
            .held
            .push_back((thread.clone(), MultiValue::new(), script)),
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

fn run_held(lua: &Lua, mut queue: VecDeque<Held>) {
    while let Some((thread, args, script)) = queue.pop_front() {
        scheduler::resume_as(lua, &thread, args, script);
        if is_paused(lua) {
            // Paused again: the new pause's coroutines go first, then these.
            if let Some(mut debugger) = lua.app_data_mut::<Debugger>() {
                debugger.held.extend(queue);
            }
            return;
        }
    }
}

/// The step hook, before an instruction: pauses at a breakpoint, and holds
/// coroutines that get control while paused.
fn on_step(lua: &Lua, step: Step) -> VmState {
    let Some(mut debugger) = lua.app_data_mut::<Debugger>() else {
        return VmState::Continue;
    };
    if let Some(holding) = debugger.holding.clone()
        && holding.state() == step.state
    {
        // Still running after the hook asked it to yield: it couldn't.
        drop(debugger);
        hold_failed(lua, &holding);
        return VmState::Continue;
    }
    let Some((path, line, depth)) = step.location else {
        return VmState::Continue;
    };
    if debugger.stopped_here(step.state, &path, line, depth) {
        return VmState::Continue;
    }
    // Only the innermost coroutine the scheduler is running can pause: its
    // yield goes back to the scheduler.
    let Some(current) = debugger
        .running
        .last()
        .filter(|thread| thread.state() == step.state)
        .cloned()
    else {
        let missed = debugger.paused.is_none() && debugger.on_breakpoint(&path, line);
        if missed && debugger.reported.insert((path.clone(), line)) {
            drop(debugger);
            report_missed(lua, &path, line);
        }
        return VmState::Continue;
    };
    if debugger.paused.is_none() {
        if !debugger.on_breakpoint(&path, line) {
            return VmState::Continue;
        }
        debugger.paused = Some(Stop {
            path: path.clone(),
            line,
            thread: current.clone(),
            frames: Vec::new(),
            yielded: false,
        });
    }
    let at_breakpoint = debugger
        .paused
        .as_ref()
        .is_some_and(|stop| stop.thread == current);
    debugger.holding = Some(current.clone());
    debugger.spots.push(Spot {
        thread: current.clone(),
        path,
        line,
        depth,
    });
    drop(debugger);
    if at_breakpoint {
        let frames = read_frames(lua, &current);
        if let Some(mut debugger) = lua.app_data_mut::<Debugger>()
            && let Some(stop) = &mut debugger.paused
        {
            stop.frames = frames;
        }
    }
    VmState::Yield
}

impl Debugger {
    /// Whether the step hook has nothing to do: the common case.
    fn is_idle(&self) -> bool {
        self.breakpoints.is_empty()
            && self.paused.is_none()
            && self.holding.is_none()
            && self.spots.is_empty()
    }

    fn on_breakpoint(&self, path: &str, line: usize) -> bool {
        self.breakpoints
            .get(path)
            .is_some_and(|lines| lines.contains(&line))
    }

    /// Whether the coroutine running as `state` was stopped at this line of
    /// this function and hasn't moved on yet. Forgets the spots it has moved
    /// on from: in a function that returned, or at another line of this one.
    /// Spots in the functions that called this one stay.
    fn stopped_here(
        &mut self,
        state: *mut ffi::lua_State,
        path: &str,
        line: usize,
        depth: i32,
    ) -> bool {
        self.spots.retain(|spot| {
            spot.thread.state() != state
                || spot.depth < depth
                || (spot.depth == depth && spot.path == path && spot.line == line)
        });
        self.spots
            .iter()
            .any(|spot| spot.thread.state() == state && spot.depth == depth)
    }
}

/// Sets Luau's step hook, which runs for coroutines in single-step mode.
///
/// The hook and [`single_step`] are, with [`read_frames`], the workspace's
/// only `unsafe` code: mlua has no safe API for Luau's single-step mode.
#[allow(unsafe_code)]
fn set_step_hook(lua: &Lua) -> mlua::Result<()> {
    // SAFETY: `lua_callbacks` points at the interpreter's callbacks, shared
    // by all its coroutines, which live as long as `lua`. Only the
    // `debugstep` slot is set; mlua doesn't use it.
    unsafe {
        lua.exec_raw::<()>((), |state| {
            (*ffi::lua_callbacks(state)).debugstep = Some(step_hook);
        })
    }
}

/// Turns on single-step mode for `thread`, so the step hook runs for it and
/// for the coroutines it creates.
#[allow(unsafe_code)]
fn single_step(thread: &Thread) {
    // SAFETY: `thread` is alive (we hold it) and isn't running: the
    // scheduler is about to resume it.
    unsafe { ffi::lua_singlestep(thread.state(), 1) }
}

/// Luau calls this before each instruction of a single-stepping coroutine.
#[allow(unsafe_code)]
unsafe extern "C-unwind" fn step_hook(state: *mut ffi::lua_State, _: *mut ffi::lua_Debug) {
    // A panic must not unwind into Luau's C++ frames: a failed hook only
    // means this instruction isn't checked.
    let action = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: `state` is the running coroutine of a `Lua` that's alive,
        // since it's running Lua code.
        let lua = unsafe { Lua::get_or_init_from_ptr(state) };
        let idle = lua
            .app_data_ref::<Debugger>()
            .is_none_or(|debugger| debugger.is_idle());
        if idle {
            return VmState::Continue;
        }
        // SAFETY: inside the hook, the running function is at level 0.
        let location = unsafe { step_location(state) };
        on_step(lua, Step { state, location })
    }))
    .unwrap_or(VmState::Continue);
    // SAFETY: a step hook may yield its coroutine (Luau checks for that once
    // the hook returns), but only where the coroutine can yield.
    unsafe {
        if matches!(action, VmState::Yield) && ffi::lua_isyieldable(state) != 0 {
            ffi::lua_yield(state, 0);
        }
    }
}

/// The script path, line and call depth of the instruction about to run in
/// `state`, if it's script code. Scripts are loaded with the name `@path`;
/// the engine's Lua isn't.
#[allow(unsafe_code)]
unsafe fn step_location(state: *mut ffi::lua_State) -> Option<(String, usize, i32)> {
    // SAFETY: the caller passes the running coroutine; `lua_getinfo` only
    // fills `record`, whose strings Luau keeps alive while we copy them.
    unsafe {
        let mut record: ffi::lua_Debug = std::mem::zeroed();
        if ffi::lua_getinfo(state, 0, c"sl".as_ptr(), &mut record) == 0 || record.source.is_null() {
            return None;
        }
        let source = CStr::from_ptr(record.source).to_string_lossy();
        let path = source.strip_prefix('@')?.to_owned();
        let line = usize::try_from(record.currentline).ok()?;
        Some((path, line, ffi::lua_stackdepth(state)))
    }
}

/// The script functions on the call stack of `thread`, which the step hook
/// is about to yield.
///
/// Part of the workspace's only `unsafe` code (decided in v2 Phase 4): mlua
/// has no safe way to read a coroutine's locals, and loading the `debug`
/// library would be unsafe too, with more reach. The player never gets here.
#[allow(unsafe_code)]
fn read_frames(lua: &Lua, thread: &Thread) -> Vec<PausedFrame> {
    let mut frames: Vec<(Option<String>, String, usize, Vec<String>)> = Vec::new();
    let from = thread.state();
    // SAFETY: `from` is the state of `thread`, which is alive (we hold it)
    // and stopped in the step hook, which Luau gives room on the stack, so
    // its stack can be read; `exec_raw` runs the closure in a protected call
    // on another state (`to`), and every value `lua_getlocal` pushes on
    // `from` is moved to `to` or popped, so the paused coroutine's stack is
    // left as it was. The values left on
    // `to` come back as `values`, one per name in `frames`, in order.
    let values = unsafe {
        lua.exec_raw::<MultiValue>((), |to| {
            let mut next_level = 0;
            while frames.len() < MAX_FRAMES {
                let level = next_level;
                next_level += 1;
                let mut record: ffi::lua_Debug = std::mem::zeroed();
                // Luau's `lua_getinfo` returns 0 past the bottom of the stack.
                if ffi::lua_getinfo(from, level, c"sln".as_ptr(), &mut record) == 0 {
                    break;
                }
                if record.source.is_null() {
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
                    let name = ffi::lua_getlocal(from, level, n);
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
            Value::String(name) if name.to_str().is_ok_and(|name| is_identifier(&name)) => {
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
