//! The task scheduler. Every script body runs as a Lua coroutine (a "thread"
//! in mlua), and the scheduler holds the ones paused by `task.wait` or
//! waiting to start from `task.delay`. Each frame, `ScriptRuntime::step`
//! moves the clock forward and resumes the ones whose time has come.
//!
//! The clock is game time: the sum of the frame times the host passes in, not
//! the computer's clock. That keeps the order of events predictable, and lets
//! tests drive the clock exactly.

use mlua::{IntoLuaMulti, Lua, MultiValue, Thread};

use crate::errors::error_message;
use crate::{OutputKind, push_output, watchdog};

/// Frame times are summed as floating-point numbers, which aren't exact:
/// sixty steps of 1/60 add up to slightly less than 1. A task this close to
/// due counts as due, so rounding never delays it by a whole frame.
const TOLERANCE_SECONDS: f64 = 1e-6;

/// Lives in Lua's app data, so that `task.wait` and `task.delay` can reach it.
#[derive(Default)]
pub struct Scheduler {
    /// Game time in seconds since the runtime was created.
    now: f64,
    /// Counts up for every scheduled task, so that tasks due at the same
    /// moment resume in the order they were scheduled.
    next_order: u64,
    waiting: Vec<Waiting>,
}

/// A coroutine waiting for its time to come.
struct Waiting {
    /// The game time when it's due.
    due: f64,
    order: u64,
    thread: Thread,
    resume_with: ResumeWith,
}

enum ResumeWith {
    /// Paused by `task.wait` at this game time. It's resumed with the seconds
    /// it really waited, which `task.wait` returns.
    WaitedSince(f64),
    /// Created by `task.delay`, and started with these arguments.
    Arguments(MultiValue),
}

impl Scheduler {
    /// Pauses `thread` (already yielding) for `seconds`. A wait of 0 lasts
    /// until the next step, so a loop around `task.wait()` runs once a frame.
    pub fn wait(&mut self, thread: Thread, seconds: f64) {
        let since = self.now;
        self.add(thread, seconds, ResumeWith::WaitedSince(since));
    }

    /// Starts `thread` with `args` once `seconds` have passed.
    pub fn delay(&mut self, thread: Thread, seconds: f64, args: MultiValue) {
        self.add(thread, seconds, ResumeWith::Arguments(args));
    }

    fn add(&mut self, thread: Thread, seconds: f64, resume_with: ResumeWith) {
        self.waiting.push(Waiting {
            due: self.now + seconds,
            order: self.next_order,
            thread,
            resume_with,
        });
        self.next_order += 1;
    }

    /// Moves the clock forward and removes every task that is due, sorted
    /// into the order they resume in: earliest due first, then first scheduled.
    fn take_due(&mut self, seconds: f64) -> Vec<Waiting> {
        // `max` also turns NaN into 0, so a bad frame time can't break the clock.
        self.now += seconds.max(0.0);
        let now = self.now;
        // `partition` splits the list in two: due now, and still waiting.
        let (mut due, waiting): (Vec<Waiting>, Vec<Waiting>) = std::mem::take(&mut self.waiting)
            .into_iter()
            .partition(|task| task.due <= now + TOLERANCE_SECONDS);
        self.waiting = waiting;
        // Floats have no total order in Rust (because of NaN), so `total_cmp` picks one.
        due.sort_by(|a, b| a.due.total_cmp(&b.due).then(a.order.cmp(&b.order)));
        due
    }
}

/// The scheduler, for changing. Drop the returned guard before resuming any
/// coroutine, because that coroutine may call `task.wait` and need it too.
pub fn scheduler_mut(lua: &Lua) -> mlua::Result<mlua::AppDataRefMut<'_, Scheduler>> {
    lua.app_data_mut::<Scheduler>()
        .ok_or_else(|| mlua::Error::runtime("the task scheduler isn't running"))
}

/// Moves the clock forward by `seconds` and resumes every task that is due.
///
/// Tasks scheduled while this runs (such as a resumed task calling
/// `task.wait()` again) are due at the next step at the earliest, so this
/// always ends.
pub fn step(lua: &Lua, seconds: f64) {
    // The block ends the borrow of the scheduler before any task runs.
    let (due, now) = {
        let Ok(mut scheduler) = scheduler_mut(lua) else {
            return;
        };
        let due = scheduler.take_due(seconds);
        (due, scheduler.now)
    };
    for task in due {
        match task.resume_with {
            ResumeWith::WaitedSince(since) => resume(lua, &task.thread, now - since),
            ResumeWith::Arguments(args) => resume(lua, &task.thread, args),
        }
    }
}

/// Runs `thread` until it finishes, pauses or fails. An error is added to the
/// output and goes no further, so it never stops whoever resumed the thread.
///
/// `impl IntoLuaMulti` accepts anything mlua can turn into Lua arguments:
/// `()` for none, one number, or a list of values.
pub fn resume(lua: &Lua, thread: &Thread, args: impl IntoLuaMulti) {
    // Scripts can also resume coroutines themselves with `coroutine.resume`,
    // so a task may already have finished by the time it's due.
    if !thread.is_resumable() {
        return;
    }
    // The watchdog times the thread from here until it pauses or ends.
    watchdog::enter(lua);
    // `()` because whatever the thread returns or yields isn't needed here.
    match thread.resume::<()>(args) {
        // Paused or finished. The watchdog may have paused it for good.
        Ok(()) => {
            if let Some(message) = watchdog::paused_by_watchdog(lua, thread) {
                push_output(lua, OutputKind::Error, message);
            }
        }
        Err(err) => {
            let message = error_message(&err);
            if watchdog::should_report(lua, &message) {
                push_output(lua, OutputKind::Error, message);
            }
        }
    }
    watchdog::leave(lua);
}
