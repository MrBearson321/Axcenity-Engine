//! The editor's debugger: breakpoints that pause Play on a line, the
//! paused script's locals, Continue and stepping, pausing on an error, and
//! watch expressions (roadmap G6).
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
//! **Coroutines the script makes** (`coroutine.create`, `wrap`; scripting
//! priority 2). The sandbox's `coroutine.resume` and `wrap`
//! (`watchdog::COROUTINE_FUNCTIONS`) tell the debugger when a script
//! resumes one of its own coroutines ([`enter_user`], [`leave_user`]), so it
//! is on [`Debugger::running`] like a scheduler coroutine and can pause.
//! Its yield goes back to that `resume`, not the scheduler, so the resume
//! **relays** it: it yields the coroutine that called it, and so on down to
//! the scheduler coroutine at the bottom (the stop's `carrier`), which the
//! scheduler holds as for any pause. On Continue the carrier is resumed,
//! and each relaying `resume` resumes the coroutine above it again. The
//! pause lists the resumers' frames under the paused coroutine's
//! ([`PausedFrame::coroutine`]), and every other coroutine the scripts made
//! that is waiting to be resumed, with its locals ([`Pause::coroutines`]).
//! Step Into on a line that resumes a coroutine pauses at its first line
//! run; a step in a coroutine that yields or finishes carries on in the one
//! that resumed it.
//!
//! **Where it can't pause.** Lua can't yield inside a function called from
//! C (a `table.sort` comparison), nor relay a pause through a `resume`
//! called there. There the line runs, and Output says once that the
//! breakpoint was passed.
//!
//! **Stepping.** [`resume_from_pause`] with a [`Resume`] other than
//! Continue remembers the paused coroutine, its line and call depth. The
//! hook pauses that coroutine again at the next line that step reaches: any
//! new line for Step Into (in a function it calls too), a new line at the
//! same depth or above for Step Over, and back in the caller for Step Out.
//! A step across `task.wait` pauses where the coroutine carries on; a
//! breakpoint another coroutine reaches first pauses there and ends the
//! step; a coroutine that finishes ends it. Where the coroutine can't yield
//! (inside a C function), the step goes on to the next line where it can.
//!
//! **Conditions, hit counts and logpoints.** A breakpoint can have a
//! [`BreakpointRule`] ([`set_breakpoint_rules`]). The coroutine still
//! stops at the line as for any breakpoint, so its locals can be read; then,
//! once it has yielded and before anything else runs, [`settle`] decides:
//! a condition that isn't true, or a hit count not yet reached, resumes it
//! at once; a logpoint writes its message to Output and resumes it. The
//! scheduler does that resume itself, in a loop ([`scheduler::resume`]), so
//! a logpoint inside a long loop never nests calls. A line reached by a step
//! always pauses: rules only decide for breakpoints.
//!
//! **Pausing on request.** [`request_pause`] (the editor's Pause button)
//! pauses the next scheduler coroutine that runs a script line where it can
//! yield. A game whose scripts are all waiting pauses when one wakes.
//!
//! **Errors.** With [`set_pause_on_error`], a scheduler coroutine that fails
//! pauses Play at the failed line. Luau keeps a failed coroutine's call
//! stack, so its frames and locals are read as at a breakpoint; Continue
//! carries on with everything else (the failed coroutine is over).
//!
//! **Watches.** [`evaluate`] runs an expression in a paused frame: its
//! locals and upvalues first, then its script's globals. It runs script
//! code (metamethods, calls), under the watchdog.
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

use crate::{OutputKind, push_output, scheduler, watchdog};

/// Breakpoints by script path, as the host passes them: line numbers
/// counting from 1.
pub type Breakpoints = BTreeMap<String, BTreeSet<usize>>;

/// What decides whether a breakpoint pauses, by script path and line. A
/// breakpoint without one always pauses.
pub type BreakpointRules = BTreeMap<(String, usize), BreakpointRule>;

/// A breakpoint's condition, hit count and log message. Each is optional;
/// all that are set apply, in that order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BreakpointRule {
    /// A Lua expression evaluated in the paused frame, as a watch is: the
    /// breakpoint pauses (or logs) only when it's true (not `nil` or
    /// `false`). One that fails pauses, and Output says why.
    pub condition: Option<String>,
    /// Pause (or log) only from this hit on: `3` lets the first two
    /// passes (where the condition held) run on. Counted from Play's start,
    /// or from when the rule last changed.
    pub hit_count: Option<u32>,
    /// A logpoint: never pause, write this to Output instead, with each
    /// `{expression}` replaced by its value in the paused frame.
    pub log: Option<String>,
}

impl BreakpointRule {
    /// Whether it does nothing a plain breakpoint wouldn't.
    pub fn is_plain(&self) -> bool {
        self.condition.is_none() && self.hit_count.is_none() && self.log.is_none()
    }
}

/// The most frames and locals a pause lists, so a deep recursion or a huge
/// function can't make it slow.
const MAX_FRAMES: usize = 20;
/// The most of the scripts' other coroutines a pause lists.
const MAX_COROUTINES: usize = 50;
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
    /// Set when an error paused Play ([`set_pause_on_error`]): its message.
    /// `line` is then the line that failed, which can't run on.
    pub error: Option<String>,
    /// Whether a breakpoint on `line` paused it (not only a step).
    pub breakpoint: bool,
    /// Every other coroutine the scripts made that is waiting to be
    /// resumed (`suspended`), with where it waits.
    pub coroutines: Vec<CoroutineView>,
}

/// A coroutine a script made, as a pause shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct CoroutineView {
    /// `suspended`, or `normal` while it resumes another.
    pub status: String,
    /// Its script functions, innermost (where it waits) first.
    pub frames: Vec<PausedFrame>,
}

/// How Play carries on from a pause.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resume {
    /// Run on to the next breakpoint.
    Continue,
    /// Pause at the next line of this function (or the one it returns to).
    StepOver,
    /// Pause at the next line run, inside a function this line calls too.
    StepInto,
    /// Pause once this function returns to the one that called it.
    StepOut,
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
    /// The variables from outside it that it uses (upvalues), such as a
    /// script's own locals inside one of its functions.
    pub upvalues: Vec<Local>,
    /// Which coroutine it's on: 0 is the paused one; 1 the one that
    /// resumed it (with `coroutine.resume` or a `wrap`ped function), and so
    /// on down to the scheduler's.
    pub coroutine: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Local {
    pub name: String,
    /// The value as text: `12`, `"hello"`, `Door`, `{1, 2, open = true}`.
    pub value: String,
    /// A table's entries (keys as names, `[1]` for numbers), read without
    /// its metamethods, for the Locals view to expand: up to
    /// [`EXPANDED_ENTRIES`] each, two tables deep. Empty for anything else.
    pub fields: Vec<Local>,
}

/// The most entries an expanded table lists, and how many tables deep the
/// Locals view can expand.
pub const EXPANDED_ENTRIES: usize = 50;
const EXPANDED_DEPTH: usize = 2;

/// Lives in Lua's app data, where the hook and the scheduler can reach it.
#[derive(Default)]
struct Debugger {
    breakpoints: Breakpoints,
    /// The coroutines running, innermost last: the scheduler's, and the
    /// scripts' own that a scheduler coroutine resumed. Only the innermost
    /// can be paused: its yield goes back to the scheduler, or is relayed
    /// there by the scripts' `resume`s.
    running: Vec<Running>,
    /// The hook made a script's own coroutine yield, and the `resume`s that
    /// resumed it are relaying the yield down to the scheduler: `holding`
    /// is the coroutine that should yield next.
    relaying: bool,
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
    /// The step under way, until the coroutine pauses or ends.
    stepping: Option<Stepping>,
    /// Pause when a scheduler coroutine fails ([`set_pause_on_error`]).
    pause_on_error: bool,
    /// Conditions, hit counts and log messages ([`set_breakpoint_rules`]).
    rules: BreakpointRules,
    /// How many times each ruled breakpoint was passed with its condition
    /// true.
    hits: BTreeMap<(String, usize), u32>,
    /// Pause at the next line any scheduler coroutine runs ([`request_pause`]).
    pause_requested: bool,
}

/// A coroutine on [`Debugger::running`].
#[derive(Clone)]
struct Running {
    thread: Thread,
    /// One the scripts made (`coroutine.create`), resumed by a script's
    /// `coroutine.resume`, not by the scheduler.
    user: bool,
}

/// A step from a pause: the coroutine and where it was.
#[derive(Clone)]
struct Stepping {
    how: Resume,
    thread: Thread,
    path: String,
    line: usize,
    depth: i32,
}

impl Stepping {
    /// Whether the stepping coroutine, at this line and depth, has gone as
    /// far as the step goes.
    fn reached(&self, path: &str, line: usize, depth: i32) -> bool {
        let moved = path != self.path || line != self.line;
        match self.how {
            Resume::Continue => false,
            Resume::StepInto => depth != self.depth || moved,
            Resume::StepOver => depth < self.depth || (depth == self.depth && moved),
            Resume::StepOut => depth < self.depth,
        }
    }
}

/// What a paused frame's watches can see: its variables, then its globals.
struct Scope {
    /// Upvalues first, then locals in the order declared, so a later name
    /// shadows an earlier one as in the code.
    variables: Vec<(String, Value)>,
    globals: Option<Table>,
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
    /// Whether the coroutine can yield here (not inside a C function).
    yieldable: bool,
}

/// The breakpoint, step or error Play is paused at.
struct Stop {
    path: String,
    line: usize,
    thread: Thread,
    /// The scheduler coroutine the pause is held on: `thread`, or for a
    /// coroutine a script made, the scheduler coroutine that (through any
    /// others) resumed it.
    carrier: Thread,
    /// Its call depth there.
    depth: i32,
    /// Read by the hook, before the coroutine yields: afterwards Luau
    /// reports its innermost function one instruction early.
    frames: Vec<PausedFrame>,
    /// One per frame, for watches.
    scopes: Vec<Scope>,
    /// Set once the coroutine has really yielded.
    yielded: bool,
    /// The step that paused here, if one did: it goes on if the coroutine
    /// can't yield after all.
    stepped: Option<Stepping>,
    /// An error's message, when an error paused Play.
    error: Option<String>,
    /// Paused by a breakpoint on this line.
    breakpoint: bool,
    /// The scripts' other coroutines, read when it paused.
    coroutines: Vec<CoroutineView>,
    /// The breakpoint, not a step or a pause request, paused it, so its
    /// rule decides ([`settle`]).
    ruled: bool,
}

/// Turns the debugger on: every coroutine the scheduler resumes from here
/// on runs the step hook. Call it before any script is loaded, so scripts
/// are compiled with their local names.
pub fn install(lua: &Lua) -> mlua::Result<()> {
    lua.set_app_data(Debugger::default());
    coroutine_state(lua)?.raw_set("on", true)?;
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
        error: stop.error.clone(),
        breakpoint: stop.breakpoint,
        coroutines: stop.coroutines.clone(),
    })
}

/// Replaces every breakpoint's rule. A breakpoint whose rule changed counts
/// its hits from zero again. Does nothing without the debugger.
pub fn set_breakpoint_rules(lua: &Lua, rules: &BreakpointRules) {
    if let Some(mut debugger) = lua.app_data_mut::<Debugger>() {
        let rules: BreakpointRules = rules
            .iter()
            .filter(|(_, rule)| !rule.is_plain())
            .map(|(at, rule)| (at.clone(), rule.clone()))
            .collect();
        let old = std::mem::replace(&mut debugger.rules, rules);
        let kept: Vec<_> = debugger
            .hits
            .keys()
            .filter(|at| old.get(*at) == debugger.rules.get(*at))
            .cloned()
            .collect();
        debugger.hits.retain(|at, _| kept.contains(at));
    }
}

/// Pauses at the next script line a scheduler coroutine runs, where it can
/// pause (the editor's Pause button). Does nothing without the debugger or
/// while paused.
pub fn request_pause(lua: &Lua, on: bool) {
    if let Some(mut debugger) = lua.app_data_mut::<Debugger>() {
        debugger.pause_requested = on && debugger.paused.is_none();
    }
}

/// Whether a pause was asked for and hasn't happened yet.
pub fn pause_requested(lua: &Lua) -> bool {
    lua.app_data_ref::<Debugger>()
        .is_some_and(|debugger| debugger.pause_requested)
}

/// Whether a scheduler coroutine that fails pauses Play (off by default).
/// Does nothing without the debugger.
pub fn set_pause_on_error(lua: &Lua, on: bool) {
    if let Some(mut debugger) = lua.app_data_mut::<Debugger>() {
        debugger.pause_on_error = on;
    }
}

/// Called by the scheduler when `thread` failed with `message`, before
/// [`leave`]. Pauses Play at the failed line when pausing on errors.
pub fn failed(lua: &Lua, thread: &Thread, message: &str) {
    let wanted = lua.app_data_ref::<Debugger>().is_some_and(|debugger| {
        debugger.pause_on_error
            && debugger.paused.is_none()
            && debugger
                .running
                .last()
                .is_some_and(|last| last.thread == *thread)
    });
    if !wanted {
        return;
    }
    let (frames, scopes) = read_frames(lua, thread);
    let Some(top) = frames.first() else {
        // Nothing of a script's on the stack to show.
        return;
    };
    let (path, line) = (top.path.clone(), top.line);
    if let Some(mut debugger) = lua.app_data_mut::<Debugger>() {
        debugger.stepping = None;
        debugger.paused = Some(Stop {
            path,
            line,
            thread: thread.clone(),
            carrier: thread.clone(),
            depth: 0,
            frames,
            scopes,
            yielded: true,
            stepped: None,
            error: Some(message.to_owned()),
            breakpoint: false,
            ruled: false,
            coroutines: Vec::new(),
        });
    }
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
        debugger.running.push(Running {
            thread: thread.clone(),
            user: false,
        });
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
    if debugger.holding.as_ref() == Some(thread) {
        debugger.relaying = false;
    }
    if !thread.is_resumable() {
        debugger.spots.retain(|spot| spot.thread != *thread);
        if debugger
            .stepping
            .as_ref()
            .is_some_and(|step| step.thread == *thread)
        {
            // Stepped off the end: the game runs on.
            debugger.stepping = None;
        }
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
        Some(stop) if stop.carrier == *thread => {
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

/// The name the coroutine functions' shared state is kept under in Lua's
/// registry: `{ on = <the debugger is installed>, made = <weak set of the
/// coroutines scripts made> }`.
const COROUTINE_STATE: &str = "axcenity.debugger.coroutines";

/// The table `watchdog::COROUTINE_FUNCTIONS` reads to know whether to tell
/// the debugger about resumes, and where it notes the coroutines scripts
/// make, made once per Lua state.
pub(crate) fn coroutine_state(lua: &Lua) -> mlua::Result<Table> {
    if let Ok(state) = lua.named_registry_value::<Table>(COROUTINE_STATE) {
        return Ok(state);
    }
    let state = lua.create_table()?;
    state.raw_set("on", is_installed(lua))?;
    let made = lua.create_table()?;
    let weak = lua.create_table()?;
    weak.raw_set("__mode", "k")?;
    made.set_metatable(Some(weak))?;
    state.raw_set("made", made)?;
    state.raw_set(
        "enter",
        lua.create_function(|lua, thread: Thread| {
            enter_user(lua, &thread);
            Ok(())
        })?,
    )?;
    state.raw_set(
        "leave",
        lua.create_function(|lua, thread: Thread| Ok(leave_user(lua, &thread)))?,
    )?;
    state.raw_set(
        "relay_failed",
        lua.create_function(|lua, ()| {
            relay_failed(lua);
            Ok(())
        })?,
    )?;
    lua.set_named_registry_value(COROUTINE_STATE, &state)?;
    Ok(state)
}

/// Called by the scripts' `coroutine.resume` (and `wrap`ped functions)
/// before resuming `thread`, a coroutine a script made: it runs the step
/// hook and can pause. A Step Into from the line that resumes it pauses at
/// the first line it runs.
fn enter_user(lua: &Lua, thread: &Thread) {
    let Some(mut debugger) = lua.app_data_mut::<Debugger>() else {
        return;
    };
    if !thread.is_resumable() {
        return;
    }
    let resumer = debugger
        .running
        .last()
        .map(|running| running.thread.clone());
    if let Some(step) = &mut debugger.stepping
        && step.how == Resume::StepInto
        && resumer.as_ref() == Some(&step.thread)
    {
        *step = Stepping {
            how: Resume::StepInto,
            thread: thread.clone(),
            path: String::new(),
            line: 0,
            depth: -1,
        };
    }
    single_step(thread);
    debugger.running.push(Running {
        thread: thread.clone(),
        user: true,
    });
}

/// Called by the scripts' `resume` once `thread` has yielded, finished or
/// failed. True when the hook made it yield to pause: the `resume` then
/// relays the pause by yielding its own coroutine (or calls
/// [`relay_failed`] if it can't), and resumes `thread` again once that
/// coroutine is resumed. A step in `thread` that it yields or finishes
/// before reaching carries on in the coroutine that resumed it.
fn leave_user(lua: &Lua, thread: &Thread) -> bool {
    let Some(mut debugger) = lua.app_data_mut::<Debugger>() else {
        return false;
    };
    if !debugger
        .running
        .last()
        .is_some_and(|running| running.user && running.thread == *thread)
    {
        return false;
    }
    debugger.running.pop();
    let resumer = debugger
        .running
        .last()
        .map(|running| running.thread.clone());
    let finished = !thread.is_resumable();
    if finished {
        debugger.spots.retain(|spot| spot.thread != *thread);
    }
    if debugger.holding.as_ref() == Some(thread) {
        if finished {
            drop(debugger);
            hold_failed(lua, thread);
            return false;
        }
        let Some(resumer) = resumer else {
            return false;
        };
        debugger.holding = Some(resumer);
        debugger.relaying = true;
        return true;
    }
    if let Some(resumer) = resumer
        && debugger
            .stepping
            .as_ref()
            .is_some_and(|step| step.thread == *thread)
    {
        // Pause at the next line of the coroutine that resumed it.
        debugger.stepping = Some(Stepping {
            how: Resume::StepOver,
            thread: resumer,
            path: String::new(),
            line: 0,
            depth: i32::MAX,
        });
    }
    false
}

/// The `resume` relaying a pause can't yield (it was called from a function
/// the engine calls, such as a `table.sort` comparison): the pause doesn't
/// happen, and the coroutine runs on.
fn relay_failed(lua: &Lua) {
    let thread = lua
        .app_data_ref::<Debugger>()
        .and_then(|debugger| debugger.paused.as_ref().map(|stop| stop.thread.clone()));
    match thread {
        Some(thread) => hold_failed(lua, &thread),
        None => {
            if let Some(mut debugger) = lua.app_data_mut::<Debugger>() {
                debugger.holding = None;
                debugger.relaying = false;
            }
        }
    }
}

/// The coroutines scripts made that are alive and not in `chain` (the
/// paused one and those resuming it), with their frames.
fn user_coroutines(lua: &Lua, chain: &[Thread]) -> Vec<CoroutineView> {
    let Ok(state) = coroutine_state(lua) else {
        return Vec::new();
    };
    let Ok(made) = state.raw_get::<Table>("made") else {
        return Vec::new();
    };
    let mut views = Vec::new();
    for pair in made.pairs::<Thread, Value>() {
        let Ok((thread, _)) = pair else {
            continue;
        };
        if chain.contains(&thread) || !thread.is_resumable() {
            continue;
        }
        if views.len() >= MAX_COROUTINES {
            break;
        }
        let (frames, _) = read_frames(lua, &thread);
        if frames.is_empty() {
            // Not started yet, or waiting in the engine's code only.
            continue;
        }
        views.push(CoroutineView {
            status: "suspended".to_owned(),
            frames,
        });
    }
    views.sort_by(|a, b| {
        (&a.frames[0].path, a.frames[0].line).cmp(&(&b.frames[0].path, b.frames[0].line))
    });
    views
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
        debugger.relaying = false;
        if !debugger
            .paused
            .as_ref()
            .is_some_and(|stop| stop.thread == *thread || stop.carrier == *thread)
        {
            return;
        }
        let stop = debugger.paused.take().expect("checked above");
        // A step goes on to the next line where it can pause.
        let breakpoint = debugger.on_breakpoint(&stop.path, stop.line);
        debugger.stepping = stop.stepped;
        if !breakpoint {
            return;
        }
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
             line ran inside a function the engine calls that can't pause (such as a \
             table.sort comparison), or in a coroutine resumed from one. Put the \
             breakpoint where that code is called."
        ),
    );
}

/// Carries on from the pause as `how` says: Continue, or a step of the
/// paused coroutine. Resumes what the pause held, in order, unless one of
/// them pauses again; then the rest wait. A step from an error's pause is a
/// Continue: that coroutine is over.
pub fn resume_from_pause(lua: &Lua, how: Resume) {
    let queue = {
        let Some(mut debugger) = lua.app_data_mut::<Debugger>() else {
            return;
        };
        let stop = debugger.paused.take();
        debugger.stepping = stop
            .filter(|stop| how != Resume::Continue && stop.error.is_none())
            .map(|stop| Stepping {
                how,
                thread: stop.thread,
                path: stop.path,
                line: stop.line,
                depth: stop.depth,
            });
        debugger.holding = None;
        debugger.relaying = false;
        debugger.pause_requested = false;
        std::mem::take(&mut debugger.held)
    };
    run_held(lua, queue);
}

/// Called by the scheduler once `thread` has yielded and [`leave`] ran.
/// If it stopped at a breakpoint with a rule, the rule decides: true means
/// the breakpoint didn't pause after all (a condition that isn't true, a
/// hit count not reached, or a logpoint, whose message is now in Output),
/// so the scheduler resumes `thread` again at once, with no arguments.
pub fn settle(lua: &Lua, thread: &Thread) -> bool {
    let (at, rule) = {
        let Some(debugger) = lua.app_data_ref::<Debugger>() else {
            return false;
        };
        let Some(stop) = debugger
            .paused
            .as_ref()
            .filter(|stop| stop.carrier == *thread && stop.yielded && stop.ruled)
        else {
            return false;
        };
        let at = (stop.path.clone(), stop.line);
        let Some(rule) = debugger.rules.get(&at).cloned() else {
            return false;
        };
        (at, rule)
    };
    let (path, line) = (&at.0, at.1);
    if let Some(condition) = &rule.condition {
        match evaluate_value(lua, 0, condition) {
            Ok(Value::Nil | Value::Boolean(false)) => return carry_on(lua, thread),
            Ok(_) => {}
            Err(error) => {
                push_output(
                    lua,
                    OutputKind::Error,
                    format!(
                        "{path}:{line}: the breakpoint's condition failed, so Play paused: {error}"
                    ),
                );
                return false;
            }
        }
    }
    let hits = {
        let Some(mut debugger) = lua.app_data_mut::<Debugger>() else {
            return false;
        };
        let hits = debugger.hits.entry(at.clone()).or_default();
        *hits = hits.saturating_add(1);
        *hits
    };
    if rule.hit_count.is_some_and(|wanted| hits < wanted) {
        return carry_on(lua, thread);
    }
    if let Some(message) = &rule.log {
        let text = interpolate(lua, message);
        push_output(lua, OutputKind::Print, format!("{path}:{line}: {text}"));
        return carry_on(lua, thread);
    }
    false
}

/// Ends the pause at `thread`'s breakpoint without anyone seeing it: the
/// scheduler resumes it next, and a step under way goes on.
fn carry_on(lua: &Lua, thread: &Thread) -> bool {
    let Some(mut debugger) = lua.app_data_mut::<Debugger>() else {
        return false;
    };
    let Some(stop) = debugger.paused.take() else {
        return false;
    };
    debugger.stepping = stop.stepped;
    if let Some(at) = debugger.held.iter().position(|(held, _, _)| held == thread) {
        debugger.held.remove(at);
    }
    true
}

/// A logpoint's message with each `{expression}` replaced by its value in
/// the paused frame: strings as they are, anything else as the Locals view
/// shows it, and `{error: …}` for one that fails. `{{` and `}}` are braces.
fn interpolate(lua: &Lua, message: &str) -> String {
    let tostring: Option<Function> = lua.globals().raw_get("tostring").ok();
    let mut text = String::new();
    let mut rest = message;
    while let Some(open) = rest.find(['{', '}']) {
        text.push_str(&rest[..open]);
        let brace = &rest[open..open + 1];
        let after = &rest[open + 1..];
        if after.starts_with(brace) {
            text.push_str(brace);
            rest = &after[1..];
            continue;
        }
        let Some(close) = after.find('}').filter(|_| brace == "{") else {
            text.push_str(brace);
            rest = after;
            continue;
        };
        let expression = &after[..close];
        match evaluate_value(lua, 0, expression) {
            Ok(Value::String(value)) => text.push_str(&value.to_string_lossy()),
            Ok(value) => text.push_str(&describe(&value, tostring.as_ref(), true)),
            Err(error) => text.push_str(&format!("{{error: {error}}}")),
        }
        rest = &after[close + 1..];
    }
    text.push_str(rest);
    text
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
        && !debugger.relaying
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
        .filter(|running| running.thread.state() == step.state)
        .map(|running| running.thread.clone())
    else {
        let missed = debugger.paused.is_none() && debugger.on_breakpoint(&path, line);
        if missed && debugger.reported.insert((path.clone(), line)) {
            drop(debugger);
            report_missed(lua, &path, line);
        }
        return VmState::Continue;
    };
    if debugger.paused.is_none() {
        let breakpoint = debugger.on_breakpoint(&path, line);
        let stepped = debugger
            .stepping
            .as_ref()
            .is_some_and(|step| step.thread == current && step.reached(&path, line, depth));
        // A step and a pause request wait for a line where the coroutine
        // can yield.
        let stepped = stepped && step.yieldable;
        let requested = debugger.pause_requested && step.yieldable;
        if !breakpoint && !stepped && !requested {
            return VmState::Continue;
        }
        debugger.pause_requested = false;
        let ruled = breakpoint && !stepped && !requested;
        let stepping = debugger.stepping.take();
        let carrier = debugger
            .running
            .iter()
            .rev()
            .find(|running| !running.user)
            .map_or_else(|| current.clone(), |running| running.thread.clone());
        debugger.paused = Some(Stop {
            path: path.clone(),
            line,
            thread: current.clone(),
            carrier,
            depth,
            frames: Vec::new(),
            scopes: Vec::new(),
            yielded: false,
            stepped: stepping,
            error: None,
            breakpoint,
            ruled,
            coroutines: Vec::new(),
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
    // The coroutines that resumed this one, down to the scheduler's: their
    // frames are listed after its own.
    let current_is_user = debugger.running.last().is_some_and(|running| running.user);
    let resumers: Vec<Thread> = debugger
        .running
        .iter()
        .rev()
        .skip(1)
        .scan(current_is_user, |more, running| {
            let take = *more;
            *more = running.user;
            take.then(|| running.thread.clone())
        })
        .collect();
    drop(debugger);
    if at_breakpoint {
        let (mut frames, mut scopes) = read_frames(lua, &current);
        for (index, resumer) in resumers.iter().enumerate() {
            let (more, more_scopes) = read_frames(lua, resumer);
            frames.extend(more.into_iter().map(|frame| PausedFrame {
                coroutine: index + 1,
                ..frame
            }));
            scopes.extend(more_scopes);
        }
        let mut chain = resumers;
        chain.push(current.clone());
        let coroutines = user_coroutines(lua, &chain);
        if let Some(mut debugger) = lua.app_data_mut::<Debugger>()
            && let Some(stop) = &mut debugger.paused
        {
            stop.frames = frames;
            stop.scopes = scopes;
            stop.coroutines = coroutines;
        }
    }
    VmState::Yield
}

impl Debugger {
    /// Whether the step hook has nothing to do: the common case.
    fn is_idle(&self) -> bool {
        !self.pause_requested
            && self.breakpoints.is_empty()
            && self.stepping.is_none()
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
        // SAFETY: inside the hook, the running function is at level 0, and
        // `state` is the running coroutine.
        let (location, yieldable) =
            unsafe { (step_location(state), ffi::lua_isyieldable(state) != 0) };
        on_step(
            lua,
            Step {
                state,
                location,
                yieldable,
            },
        )
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
/// is about to yield (or which failed), and what each one's watches see.
///
/// Part of the workspace's only `unsafe` code (decided in v2 Phase 4): mlua
/// has no safe way to read a coroutine's locals, and loading the `debug`
/// library would be unsafe too, with more reach. The player never gets here.
#[allow(unsafe_code)]
fn read_frames(lua: &Lua, thread: &Thread) -> (Vec<PausedFrame>, Vec<Scope>) {
    /// One frame as read: its function's name, path, line, and the names of
    /// its locals and upvalues, whose values follow on the stack, then its
    /// globals table.
    type Read = (Option<String>, String, usize, Vec<String>, Vec<String>);
    let mut frames: Vec<Read> = Vec::new();
    let from = thread.state();
    // SAFETY: `from` is the state of `thread`, which is alive (we hold it)
    // and either stopped in the step hook, which Luau gives room on the
    // stack, or failed, which leaves its stack in place; so its stack can be
    // read. `exec_raw` runs the closure in a protected call on another state
    // (`to`), and every value pushed on `from` (locals, the function, its
    // upvalues and environment) is moved to `to` or popped, so the paused
    // coroutine's stack is left as it was. The values left on `to` come back
    // as `values`: for each frame in `frames`, one per local name, one per
    // upvalue name, then its environment.
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
                // Room for the function, one value at a time on both
                // states, and the environment at the end.
                if ffi::lua_checkstack(from, 2) == 0 || ffi::lua_checkstack(to, 2) == 0 {
                    break;
                }
                let function = (!record.name.is_null())
                    .then(|| CStr::from_ptr(record.name).to_string_lossy().into_owned());
                let mut names = Vec::new();
                let mut n = 1;
                while names.len() < MAX_LOCALS {
                    if ffi::lua_checkstack(from, 1) == 0 || ffi::lua_checkstack(to, 2) == 0 {
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
                // The function itself, for its upvalues and environment.
                let mut upvalues = Vec::new();
                let mut function_record: ffi::lua_Debug = std::mem::zeroed();
                if ffi::lua_getinfo(from, level, c"f".as_ptr(), &mut function_record) != 0 {
                    let mut n = 1;
                    while upvalues.len() < MAX_LOCALS
                        && ffi::lua_checkstack(from, 1) != 0
                        && ffi::lua_checkstack(to, 2) != 0
                    {
                        let name = ffi::lua_getupvalue(from, -1, n);
                        if name.is_null() {
                            break;
                        }
                        n += 1;
                        let name = CStr::from_ptr(name).to_string_lossy().into_owned();
                        if name.is_empty() || name.starts_with('(') {
                            ffi::lua_pop(from, 1);
                            continue;
                        }
                        ffi::lua_xmove(from, to, 1);
                        upvalues.push(name);
                    }
                    ffi::lua_getfenv(from, -1);
                    ffi::lua_xmove(from, to, 1);
                    ffi::lua_pop(from, 1);
                } else {
                    ffi::lua_pushnil(to);
                }
                let line = usize::try_from(record.currentline).unwrap_or(0);
                frames.push((function, path.to_owned(), line, names, upvalues));
            }
        })
    };
    let mut values = values.unwrap_or_default().into_iter();
    let tostring: Option<Function> = lua.globals().raw_get("tostring").ok();
    let mut scopes = Vec::with_capacity(frames.len());
    let frames = frames
        .into_iter()
        .map(|(function, path, line, names, upvalue_names)| {
            let mut take = |names: Vec<String>| -> Vec<(String, Value)> {
                names
                    .into_iter()
                    .map(|name| (name, values.next().unwrap_or(Value::Nil)))
                    .collect()
            };
            let locals = take(names);
            let upvalues = take(upvalue_names);
            let globals = match values.next() {
                Some(Value::Table(table)) => Some(table),
                _ => None,
            };
            let shown = |variables: &[(String, Value)]| -> Vec<Local> {
                variables
                    .iter()
                    .map(|(name, value)| shown_local(name.clone(), value, tostring.as_ref(), 0))
                    .collect()
            };
            let frame = PausedFrame {
                function,
                path,
                line,
                locals: shown(&locals),
                upvalues: shown(&upvalues),
                coroutine: 0,
            };
            scopes.push(Scope {
                variables: upvalues.into_iter().chain(locals).collect(),
                globals,
            });
            frame
        })
        .collect();
    (frames, scopes)
}

/// Evaluates the watch `expression` in the paused frame `frame` (0 is the
/// innermost) and returns its value as text, as the locals are shown, or
/// what went wrong. It sees that frame's locals and upvalues, then its
/// script's globals. Calls and metamethods run, under the watchdog; a watch
/// can't wait.
pub fn evaluate(lua: &Lua, frame: usize, expression: &str) -> Result<String, String> {
    let value = evaluate_value(lua, frame, expression)?;
    let tostring: Option<Function> = lua.globals().raw_get("tostring").ok();
    Ok(describe(&value, tostring.as_ref(), true))
}

/// [`evaluate`]'s value itself.
fn evaluate_value(lua: &Lua, frame: usize, expression: &str) -> Result<Value, String> {
    let (variables, globals) = {
        let debugger = lua
            .app_data_ref::<Debugger>()
            .ok_or("The debugger isn't on.")?;
        let stop = debugger
            .paused
            .as_ref()
            .filter(|stop| stop.yielded)
            .ok_or("Play isn't paused.")?;
        let scope = stop.scopes.get(frame).ok_or("There's no such frame.")?;
        (scope.variables.clone(), scope.globals.clone())
    };
    let text = |err: mlua::Error| crate::errors::error_message(&err);
    let scope = lua.create_table().map_err(text)?;
    for (name, value) in variables {
        scope.raw_set(name, value).map_err(text)?;
    }
    if let Some(globals) = globals {
        let meta = lua.create_table().map_err(text)?;
        meta.raw_set("__index", globals).map_err(text)?;
        scope.set_metatable(Some(meta)).map_err(text)?;
    }
    let function = lua
        .load(format!("return ({expression}\n)"))
        .set_name("=watch")
        .set_environment(scope)
        .into_function()
        .map_err(text)?;
    let thread = lua.create_thread(function).map_err(text)?;
    watchdog::enter(lua, &thread);
    let result = thread.resume::<Value>(());
    let stopped = watchdog::paused_by_watchdog(lua, &thread);
    watchdog::leave(lua);
    if let Some(message) = stopped {
        return Err(message);
    }
    let value = result.map_err(text)?;
    if thread.is_resumable() {
        return Err("A watch can't wait.".to_owned());
    }
    Ok(value)
}

/// A variable or table entry as the Locals view shows it, with a table's
/// entries `depth` tables down.
fn shown_local(name: String, value: &Value, tostring: Option<&Function>, depth: usize) -> Local {
    let fields = match value {
        Value::Table(table) if depth < EXPANDED_DEPTH => {
            let mut fields = Vec::new();
            for pair in table.pairs::<Value, Value>() {
                let Ok((key, value)) = pair else {
                    break;
                };
                if fields.len() >= EXPANDED_ENTRIES {
                    fields.push(Local {
                        name: "…".to_owned(),
                        value: "more entries".to_owned(),
                        fields: Vec::new(),
                    });
                    break;
                }
                let name = match &key {
                    Value::String(name) if name.to_str().is_ok_and(|name| is_identifier(&name)) => {
                        name.to_string_lossy()
                    }
                    key => format!("[{}]", describe(key, tostring, false)),
                };
                fields.push(shown_local(name, &value, tostring, depth + 1));
            }
            fields
        }
        _ => Vec::new(),
    };
    Local {
        name,
        value: describe(value, tostring, true),
        fields,
    }
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
