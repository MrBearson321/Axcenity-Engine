//! The script profiler (scripting P2 PR 4): which script functions and
//! which pieces of work (a script's body, an event handler, a `task.spawn`)
//! cost the most time, for the editor's Profiler panel. Off unless the host
//! turns it on ([`set_recording`]); the player never does.
//!
//! **Work** is timed exactly: the scheduler calls [`enter`] and [`leave`]
//! around every resume, and each resume's time goes to the work its
//! coroutine was started for ([`label`]), less the time of resumes nested
//! inside it (a `task.spawn` runs at once, inside its caller's resume), so
//! the rows add up to the scripts' whole time. A handler that waits counts
//! its later resumes too.
//!
//! **Functions** are sampled: while recording, the watchdog's interrupt
//! (which Luau runs at every call and loop iteration) calls [`sample`]
//! every [`SAMPLE_EVERY`] interrupts. Once [`SAMPLE_INTERVAL`] has passed
//! since the last sample, the time since then goes to the script functions
//! on the running coroutine's call stack: all of it to the innermost
//! script function ("self" time, and to the line it is on) and to each
//! function on the stack once ("total" time, the time inside it and what
//! it calls). Time spent in an engine function (`workspace:Raycast`) goes
//! to the script function that called it. The stack is read with mlua's
//! `inspect_stack`, which in an interrupt reads the interrupted coroutine.

use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use mlua::thread::ThreadStatus;
use mlua::{Function, Lua, Thread};

/// How many interrupts pass between looks at the clock while recording.
pub(crate) const SAMPLE_EVERY: u32 = 32;
/// The least time between two samples of the call stack.
const SAMPLE_INTERVAL: Duration = Duration::from_micros(200);
/// How deep a call stack a sample reads.
const MAX_DEPTH: usize = 64;
/// How many work rows are kept; more are counted under "Other".
const MAX_WORK: usize = 2000;

/// Which kind of work a coroutine was started for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WorkKind {
    /// A script's body, from its first line until it finishes.
    Script,
    /// An event handler (`Connect`, `Once`, `BindToRenderStep`, a
    /// ContextActionService action), each time the event fires.
    Handler,
    /// `task.spawn`, `task.delay` or `task.defer`.
    Task,
    /// Anything else: a RemoteFunction's `OnServerInvoke`, a Console
    /// command, a game system resuming a script.
    Other,
}

/// One piece of work's time while recording.
#[derive(Clone, Debug, PartialEq)]
pub struct WorkCost {
    pub kind: WorkKind,
    /// What started it: `Door.Touched`, `task.delay`, a script's name.
    pub what: String,
    /// The function it runs (`None` for a script's body or when unknown).
    pub function: Option<String>,
    /// Where that function (or script) is, for opening it.
    pub path: String,
    pub line: u32,
    /// How many times it started (a handler: how many times it was called).
    pub calls: u32,
    /// How many times a coroutine for it was resumed (more than `calls`
    /// when it waits).
    pub resumes: u32,
    /// Its own time, not counting resumes nested inside it.
    pub time: Duration,
    /// Its longest single resume.
    pub longest: Duration,
    /// Whether it ran on the client side.
    pub client: bool,
}

/// One script function's sampled time while recording.
#[derive(Clone, Debug, PartialEq)]
pub struct FunctionCost {
    /// Its name, or `None` for an anonymous function or a script's body.
    pub name: Option<String>,
    pub path: String,
    /// The line it starts on (0 for a script's body).
    pub line: u32,
    /// Time with it innermost on the stack.
    pub self_time: Duration,
    /// Time with it anywhere on the stack.
    pub total_time: Duration,
    /// The line where most of its self time was spent.
    pub hottest_line: u32,
    /// Whether it ran on the client side.
    pub client: bool,
}

/// What the profiler has recorded, the costliest first.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ScriptProfile {
    pub work: Vec<WorkCost>,
    pub functions: Vec<FunctionCost>,
    /// All the work's time, the sum of [`WorkCost::time`].
    pub total: Duration,
}

impl ScriptProfile {
    /// Adds `other`'s rows (the other side's) and sorts them again.
    pub fn merge(&mut self, other: ScriptProfile) {
        self.work.extend(other.work);
        self.functions.extend(other.functions);
        self.total += other.total;
        self.sort();
    }

    fn sort(&mut self) {
        self.work.sort_by_key(|row| std::cmp::Reverse(row.time));
        self.functions.sort_by(|a, b| {
            b.self_time
                .cmp(&a.self_time)
                .then(b.total_time.cmp(&a.total_time))
        });
    }
}

/// What the interrupt reads without borrowing the app data.
pub(crate) struct Gate {
    pub(crate) on: Cell<bool>,
    pub(crate) count: Cell<u32>,
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct WorkKey {
    kind: WorkKind,
    what: String,
    path: String,
    line: u32,
}

struct Open {
    work: usize,
    started: Instant,
    nested: Duration,
}

#[derive(Default)]
struct FunctionTimes {
    name: Option<String>,
    self_time: Duration,
    total_time: Duration,
    lines: HashMap<u32, Duration>,
}

struct Profiler {
    gate: Rc<Gate>,
    client: bool,
    work: Vec<WorkCost>,
    work_index: HashMap<WorkKey, usize>,
    /// Which work each coroutine runs, by its pointer, holding the
    /// coroutine so the pointer can't be reused while it's here.
    labels: HashMap<usize, (Thread, usize)>,
    /// The resumes running now, innermost last.
    open: Vec<Open>,
    /// Sampled time is counted up to here.
    mark: Option<Instant>,
    /// Script functions by path, then the line they start on.
    functions: HashMap<String, HashMap<u32, FunctionTimes>>,
}

impl Profiler {
    fn work_row(&mut self, key: WorkKey, function: Option<String>) -> usize {
        if let Some(&index) = self.work_index.get(&key) {
            return index;
        }
        if self.work.len() >= MAX_WORK {
            return self.other();
        }
        let index = self.work.len();
        self.work.push(WorkCost {
            kind: key.kind,
            what: key.what.clone(),
            function,
            path: key.path.clone(),
            line: key.line,
            calls: 0,
            resumes: 0,
            time: Duration::ZERO,
            longest: Duration::ZERO,
            client: self.client,
        });
        self.work_index.insert(key, index);
        index
    }

    fn other(&mut self) -> usize {
        let key = WorkKey {
            kind: WorkKind::Other,
            what: "Other script work".to_owned(),
            path: String::new(),
            line: 0,
        };
        if let Some(&index) = self.work_index.get(&key) {
            return index;
        }
        let index = self.work.len();
        self.work.push(WorkCost {
            kind: WorkKind::Other,
            what: key.what.clone(),
            function: None,
            path: String::new(),
            line: 0,
            calls: 0,
            resumes: 0,
            time: Duration::ZERO,
            longest: Duration::ZERO,
            client: self.client,
        });
        self.work_index.insert(key, index);
        index
    }
}

/// Sets up the profiler for `lua`; the watchdog's interrupt checks the
/// returned gate.
pub(crate) fn install(lua: &Lua, client: bool) -> Rc<Gate> {
    let gate = Rc::new(Gate {
        on: Cell::new(false),
        count: Cell::new(0),
    });
    lua.set_app_data(Profiler {
        gate: Rc::clone(&gate),
        client,
        work: Vec::new(),
        work_index: HashMap::new(),
        labels: HashMap::new(),
        open: Vec::new(),
        mark: None,
        functions: HashMap::new(),
    });
    gate
}

/// Marks this side as the client's, for the rows it records.
pub(crate) fn set_client(lua: &Lua, client: bool) {
    if let Some(mut profiler) = lua.app_data_mut::<Profiler>() {
        profiler.client = client;
    }
}

/// Starts or stops recording. What was recorded is kept until [`clear`].
pub fn set_recording(lua: &Lua, on: bool) {
    if let Some(mut profiler) = lua.app_data_mut::<Profiler>() {
        profiler.gate.on.set(on);
        if !on {
            profiler.labels.clear();
            profiler.open.clear();
            profiler.mark = None;
        }
    }
}

pub fn is_recording(lua: &Lua) -> bool {
    lua.app_data_ref::<Profiler>()
        .is_some_and(|profiler| profiler.gate.on.get())
}

/// Forgets everything recorded.
pub fn clear(lua: &Lua) {
    if let Some(mut profiler) = lua.app_data_mut::<Profiler>() {
        profiler.work.clear();
        profiler.work_index.clear();
        profiler.functions.clear();
        // Coroutines running now keep their rows' places, so their labels go.
        profiler.labels.clear();
        profiler.open.clear();
        profiler.mark = None;
    }
}

/// What was recorded, the costliest first.
pub fn profile(lua: &Lua) -> ScriptProfile {
    let Some(profiler) = lua.app_data_ref::<Profiler>() else {
        return ScriptProfile::default();
    };
    let work: Vec<WorkCost> = profiler
        .work
        .iter()
        .filter(|row| row.resumes > 0)
        .cloned()
        .collect();
    let total = work.iter().map(|row| row.time).sum();
    let mut functions = Vec::new();
    for (path, lines) in &profiler.functions {
        for (&line, times) in lines {
            let hottest_line = times
                .lines
                .iter()
                .max_by(|a, b| a.1.cmp(b.1).then(b.0.cmp(a.0)))
                .map_or(line, |(&line, _)| line);
            functions.push(FunctionCost {
                name: times.name.clone(),
                path: path.clone(),
                line,
                self_time: times.self_time,
                total_time: times.total_time,
                hottest_line,
                client: profiler.client,
            });
        }
    }
    let mut profile = ScriptProfile {
        work,
        functions,
        total,
    };
    profile.sort();
    profile
}

/// A script path from a chunk name (`@scripts/door.server.lua`), or
/// `None` for the engine's own Lua.
fn script_path(source: &str) -> Option<&str> {
    source.strip_prefix('@')
}

/// Where `function` is, as (name, path, line), or `None` when it isn't a
/// script's.
fn where_is(function: &Function) -> Option<(Option<String>, String, u32)> {
    let info = function.info();
    let path = script_path(info.source.as_deref()?)?.to_owned();
    let line = info.line_defined.unwrap_or(0) as u32;
    Some((info.name, path, line))
}

/// Notes that `thread` starts a new run of a piece of work: `kind`, started
/// by `what` (built only while recording), running `function` (or the
/// script at `script_path`). Call before resuming it.
pub(crate) fn label(
    lua: &Lua,
    thread: &Thread,
    kind: WorkKind,
    what: impl FnOnce() -> String,
    function: Option<&Function>,
    script_path: Option<&str>,
) {
    if !is_recording(lua) {
        return;
    }
    let found = function.and_then(where_is);
    let (name, path, line) = match (found, script_path) {
        (Some(found), _) => found,
        (None, Some(path)) => (None, path.to_owned(), 0),
        (None, None) => (None, String::new(), 0),
    };
    let key = WorkKey {
        kind,
        what: what(),
        path,
        line,
    };
    let Some(mut profiler) = lua.app_data_mut::<Profiler>() else {
        return;
    };
    let index = profiler.work_row(key, name);
    profiler.work[index].calls += 1;
    profiler
        .labels
        .insert(thread.to_pointer() as usize, (thread.clone(), index));
}

/// When a resume started, for [`leave`]; `None` while not recording.
pub(crate) struct Timing(Option<Instant>);

/// The scheduler is about to resume `thread`.
pub(crate) fn enter(lua: &Lua, thread: &Thread) -> Timing {
    let Some(mut profiler) = lua.app_data_mut::<Profiler>() else {
        return Timing(None);
    };
    if !profiler.gate.on.get() {
        return Timing(None);
    }
    let now = Instant::now();
    let work = match profiler.labels.get(&(thread.to_pointer() as usize)) {
        Some(&(_, index)) => index,
        None => profiler.other(),
    };
    profiler.open.push(Open {
        work,
        started: now,
        nested: Duration::ZERO,
    });
    if profiler.mark.is_none() {
        profiler.mark = Some(now);
    }
    Timing(Some(now))
}

/// The resume [`enter`] timed has paused, finished or failed.
pub(crate) fn leave(lua: &Lua, thread: &Thread, timing: Timing) {
    let Some(started) = timing.0 else {
        return;
    };
    let Some(mut profiler) = lua.app_data_mut::<Profiler>() else {
        return;
    };
    // Recording stopped (or was cleared) inside the resume.
    if profiler
        .open
        .last()
        .is_none_or(|open| open.started != started)
    {
        return;
    }
    let Some(open) = profiler.open.pop() else {
        return;
    };
    let took = started.elapsed();
    let own = took.saturating_sub(open.nested);
    let row = &mut profiler.work[open.work];
    row.resumes += 1;
    row.time += own;
    row.longest = row.longest.max(own);
    match profiler.open.last_mut() {
        Some(outer) => outer.nested += took,
        None => profiler.mark = None,
    }
    if thread.status() != ThreadStatus::Resumable {
        profiler.labels.remove(&(thread.to_pointer() as usize));
    }
}

/// Called from the watchdog's interrupt every [`SAMPLE_EVERY`] interrupts
/// while recording.
pub(crate) fn sample(lua: &Lua) {
    let now = Instant::now();
    let elapsed = {
        let Some(mut profiler) = lua.app_data_mut::<Profiler>() else {
            return;
        };
        let Some(mark) = profiler.mark else {
            return;
        };
        let elapsed = now.saturating_duration_since(mark);
        if elapsed < SAMPLE_INTERVAL {
            return;
        }
        profiler.mark = Some(now);
        elapsed
    };
    // The script functions on the stack, innermost first, as (name, path,
    // line defined, current line).
    let mut frames: Vec<(Option<String>, String, u32, u32)> = Vec::new();
    for level in 0..MAX_DEPTH {
        let frame = lua.inspect_stack(level, |debug| {
            let source = debug.source();
            let path = source.source.as_deref().and_then(script_path)?.to_owned();
            let defined = source.line_defined.unwrap_or(0) as u32;
            let line = debug.current_line().unwrap_or(0) as u32;
            let name = debug.names().name.map(|name| name.into_owned());
            Some((name, path, defined, line))
        });
        match frame {
            None => break,
            Some(Some(frame)) => frames.push(frame),
            Some(None) => {}
        }
    }
    let Some(mut profiler) = lua.app_data_mut::<Profiler>() else {
        return;
    };
    let mut counted: Vec<(&str, u32)> = Vec::with_capacity(frames.len());
    for (index, (name, path, defined, line)) in frames.iter().enumerate() {
        if counted.contains(&(path.as_str(), *defined)) {
            continue;
        }
        counted.push((path.as_str(), *defined));
        let lines = match profiler.functions.get_mut(path.as_str()) {
            Some(lines) => lines,
            None => profiler.functions.entry(path.clone()).or_default(),
        };
        let times = lines.entry(*defined).or_default();
        if times.name.is_none() {
            times.name.clone_from(name);
        }
        times.total_time += elapsed;
        if index == 0 {
            times.self_time += elapsed;
            *times.lines.entry(*line).or_default() += elapsed;
        }
    }
}
