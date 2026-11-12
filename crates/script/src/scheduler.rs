//! The task scheduler. Every script body runs as a Lua coroutine (a "thread"
//! in mlua), and the scheduler holds the ones paused by `task.wait` or
//! waiting to start from `task.delay`. Each frame, `ScriptRuntime::step`
//! moves the clock forward and resumes the ones whose time has come.
//!
//! The clock is game time: the sum of the frame times the host passes in, not
//! the computer's clock. That keeps the order of events predictable, and lets
//! tests drive the clock exactly.
//!
//! Every coroutine belongs to the script whose code started it: a script's
//! own body, its event handlers, and what they `task.spawn` or
//! `task.delay`. The scheduler knows which script is running now
//! ([`running_script`]), so what's paused and connected can be dropped for
//! one script when its file is reloaded ([`Scheduler::forget_script`]).

use std::cmp::{Ordering, Reverse};
use std::collections::BinaryHeap;
use std::collections::hash_map::Entry;

use axcenity_core::InstanceId;
use mlua::{IntoLuaMulti, Lua, MultiValue, Thread};
use rustc_hash::FxHashMap;

use crate::errors::error_message;
use crate::{OutputKind, debugger, push_error, push_output, watchdog};

/// Frame times are summed as floating-point numbers, which aren't exact:
/// sixty steps of 1/60 add up to slightly less than 1. A task this close to
/// due counts as due, so rounding never delays it by a whole frame.
pub const TOLERANCE_SECONDS: f64 = 1e-6;

/// Lives in Lua's app data, so that `task.wait` and `task.delay` can reach it.
#[derive(Default)]
pub struct Scheduler {
    /// Game time in seconds since the runtime was created.
    now: f64,
    /// Counts up for every scheduled task, so that tasks due at the same
    /// moment resume in the order they were scheduled.
    next_order: u64,
    /// When each task is due, soonest first (a `BinaryHeap` pops its largest
    /// item, and `Reverse` turns that into the smallest). A task that stopped
    /// waiting leaves a tombstone here: nothing in `waiting` has its number,
    /// so it's skipped when it comes up.
    queue: BinaryHeap<Reverse<Slot>>,
    /// The tasks still waiting, by their order number.
    waiting: FxHashMap<u64, Waiting>,
    /// Which tasks each coroutine has waiting, for `task.cancel`.
    by_thread: FxHashMap<usize, Orders>,
    /// The script whose coroutine is running now, if any (not the
    /// Console's commands).
    running: Option<InstanceId>,
}

/// One place in the queue: earliest due first, then first scheduled.
#[derive(Clone, Copy)]
struct Slot {
    due: f64,
    order: u64,
}

impl Ord for Slot {
    fn cmp(&self, other: &Self) -> Ordering {
        // Floats have no total order in Rust (because of NaN), so `total_cmp` picks one.
        self.due
            .total_cmp(&other.due)
            .then(self.order.cmp(&other.order))
    }
}

impl PartialOrd for Slot {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Slot {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Slot {}

/// The order numbers of one coroutine's waiting tasks. Nearly every
/// coroutine has one, which needs no allocation; `task.defer` on a waiting
/// coroutine makes more.
enum Orders {
    One(u64),
    Many(Vec<u64>),
}

/// A coroutine waiting for its time to come.
struct Waiting {
    thread: Thread,
    resume_with: ResumeWith,
    /// The script it belongs to.
    script: Option<InstanceId>,
}

enum ResumeWith {
    /// Paused by `task.wait` at this game time. It's resumed with the seconds
    /// it really waited, which `task.wait` returns.
    WaitedSince(f64),
    /// Created by `task.delay`, and started with these arguments.
    Arguments(MultiValue),
}

impl Scheduler {
    /// True while a coroutine is waiting (`task.wait`, `task.delay`).
    pub fn has_waiting(&self) -> bool {
        !self.waiting.is_empty()
    }

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
        let order = self.next_order;
        self.next_order += 1;
        self.queue.push(Reverse(Slot {
            due: self.now + seconds,
            order,
        }));
        add_order(&mut self.by_thread, thread_key(&thread), order);
        self.waiting.insert(
            order,
            Waiting {
                thread,
                resume_with,
                script: self.running,
            },
        );
    }

    /// Drops `thread` from the waiting coroutines (`task.cancel`).
    pub fn cancel(&mut self, thread: &Thread) {
        match self.by_thread.remove(&thread_key(thread)) {
            Some(Orders::One(order)) => {
                self.waiting.remove(&order);
            }
            Some(Orders::Many(orders)) => {
                for order in orders {
                    self.waiting.remove(&order);
                }
            }
            None => return,
        }
        self.sweep_tombstones();
    }

    /// Drops every waiting coroutine of `script`, so they never resume.
    pub fn forget_script(&mut self, script: InstanceId) {
        let by_thread = &mut self.by_thread;
        self.waiting.retain(|&order, task| {
            let keep = task.script != Some(script);
            if !keep {
                remove_order(by_thread, thread_key(&task.thread), order);
            }
            keep
        });
        self.sweep_tombstones();
    }

    /// How many places the queue holds, tombstones included.
    #[cfg(test)]
    pub(crate) fn queue_len(&self) -> usize {
        self.queue.len()
    }

    /// The coroutines [`step`] would resume after `seconds`, in that order.
    #[cfg(test)]
    pub(crate) fn take_due_threads(&mut self, seconds: f64) -> Vec<Thread> {
        self.take_due(seconds)
            .into_iter()
            .map(|task| task.thread)
            .collect()
    }

    /// Clears the tombstones out of the queue once they outnumber the
    /// tasks, so a script that keeps cancelling long delays can't grow it.
    fn sweep_tombstones(&mut self) {
        if self.queue.len() > 2 * self.waiting.len() + 64 {
            let waiting = &self.waiting;
            self.queue
                .retain(|Reverse(slot)| waiting.contains_key(&slot.order));
        }
    }

    /// Moves the clock forward and removes every task that is due, in the
    /// order they resume in: earliest due first, then first scheduled.
    fn take_due(&mut self, seconds: f64) -> Vec<Waiting> {
        // `max` also turns NaN into 0, so a bad frame time can't break the clock.
        self.now += seconds.max(0.0);
        let deadline = self.now + TOLERANCE_SECONDS;
        let mut due = Vec::new();
        while let Some(Reverse(slot)) = self.queue.peek().copied()
            && slot.due <= deadline
        {
            self.queue.pop();
            // `None` is a tombstone: the task stopped waiting.
            if let Some(task) = self.waiting.remove(&slot.order) {
                remove_order(&mut self.by_thread, thread_key(&task.thread), slot.order);
                due.push(task);
            }
        }
        due
    }
}

/// Whether `thread` has a task waiting (`task.wait`, `task.delay`, ...).
pub fn is_waiting(lua: &Lua, thread: &Thread) -> bool {
    lua.app_data_ref::<Scheduler>()
        .is_some_and(|scheduler| scheduler.by_thread.contains_key(&thread_key(thread)))
}

/// Identifies a coroutine while it's held somewhere, as a `Thread` in
/// [`Waiting`] is.
fn thread_key(thread: &Thread) -> usize {
    thread.to_pointer() as usize
}

fn add_order(by_thread: &mut FxHashMap<usize, Orders>, thread: usize, order: u64) {
    match by_thread.entry(thread) {
        Entry::Vacant(entry) => {
            entry.insert(Orders::One(order));
        }
        Entry::Occupied(mut entry) => match entry.get_mut() {
            Orders::One(first) => {
                let first = *first;
                entry.insert(Orders::Many(vec![first, order]));
            }
            Orders::Many(orders) => orders.push(order),
        },
    }
}

fn remove_order(by_thread: &mut FxHashMap<usize, Orders>, thread: usize, order: u64) {
    if let Entry::Occupied(mut entry) = by_thread.entry(thread) {
        let none_left = match entry.get_mut() {
            Orders::One(_) => true,
            Orders::Many(orders) => {
                orders.retain(|&other| other != order);
                orders.is_empty()
            }
        };
        if none_left {
            entry.remove();
        }
    }
}

/// The scheduler, for changing. Drop the returned guard before resuming any
/// coroutine, because that coroutine may call `task.wait` and need it too.
pub fn scheduler_mut(lua: &Lua) -> mlua::Result<mlua::AppDataRefMut<'_, Scheduler>> {
    lua.app_data_mut::<Scheduler>()
        .ok_or_else(|| mlua::Error::runtime("the task scheduler isn't running"))
}

/// Game time in seconds since the runtime was created.
pub fn now(lua: &Lua) -> f64 {
    lua.app_data_ref::<Scheduler>()
        .map_or(0.0, |scheduler| scheduler.now)
}

/// The script whose code is running now: the owner of the coroutine being
/// resumed.
pub fn running_script(lua: &Lua) -> Option<InstanceId> {
    lua.app_data_ref::<Scheduler>()
        .and_then(|scheduler| scheduler.running)
}

/// Like [`resume`], with `script` as the running script until `thread`
/// pauses or ends, so what it connects and schedules belongs to `script`.
pub fn resume_as(lua: &Lua, thread: &Thread, args: impl IntoLuaMulti, script: Option<InstanceId>) {
    let before = set_running(lua, script);
    resume(lua, thread, args);
    set_running(lua, before);
}

/// Sets the running script and returns the one before.
fn set_running(lua: &Lua, script: Option<InstanceId>) -> Option<InstanceId> {
    match lua.app_data_mut::<Scheduler>() {
        Some(mut scheduler) => std::mem::replace(&mut scheduler.running, script),
        None => None,
    }
}

/// Moves the clock forward by `seconds` and resumes every task that is due.
///
/// Tasks scheduled while this runs (such as a resumed task calling
/// `task.wait()` again) are due at the next step at the earliest, so this
/// always ends.
pub fn step(lua: &Lua, seconds: f64) {
    debugger::release(lua);
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
            ResumeWith::WaitedSince(since) => {
                resume_as(lua, &task.thread, now - since, task.script);
            }
            ResumeWith::Arguments(args) => resume_as(lua, &task.thread, args, task.script),
        }
    }
}

/// Runs `thread` until it finishes, pauses or fails. An error is added to the
/// output, with its stack trace, and goes no further, so it never stops
/// whoever resumed the thread. While Play is paused at a breakpoint, the
/// thread waits for Continue instead (see `debugger`).
///
/// `impl IntoLuaMulti` accepts anything mlua can turn into Lua arguments:
/// `()` for none, one number, or a list of values.
pub fn resume(lua: &Lua, thread: &Thread, args: impl IntoLuaMulti) {
    // Scripts can also resume coroutines themselves with `coroutine.resume`,
    // so a task may already have finished by the time it's due.
    if !thread.is_resumable() {
        return;
    }
    let Ok(args) = args.into_lua_multi(lua) else {
        return;
    };
    let Some(mut args) = debugger::hold_while_paused(lua, thread, args) else {
        return;
    };
    // The watchdog times the thread from here until it pauses or ends. A
    // breakpoint that doesn't pause (see below) doesn't restart the clock, so
    // a logpoint can't keep an endless loop from being stopped.
    watchdog::enter(lua, thread);
    loop {
        debugger::enter(lua, thread);
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
                    push_error(lua, &err);
                }
                debugger::failed(lua, thread, &message);
            }
        }
        debugger::leave(lua, thread);
        // A breakpoint whose condition isn't true, or a logpoint, runs on
        // here rather than from inside `settle`, so they never nest.
        if !debugger::settle(lua, thread) {
            break;
        }
        args = MultiValue::new();
    }
    watchdog::leave(lua);
}
