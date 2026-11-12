//! The watchdog limit and the debugger's host side: breakpoints, pausing,
//! continuing and evaluating in a paused frame (`debugger.rs`).

use crate::*;

impl ScriptRuntime {
    /// How long a script may run without pausing before the watchdog stops
    /// it with an error. [`WATCHDOG_LIMIT`] (3 seconds) unless changed;
    /// tests use a shorter one.
    pub fn set_watchdog_limit(&mut self, limit: Duration) {
        self.server.set_watchdog_limit(limit);
        self.client.set_watchdog_limit(limit);
    }

    /// Turns on breakpoints, for the editor's Play. Call it before starting
    /// any script: only coroutines made after it can stop at a breakpoint.
    /// It makes every script line call a hook, so the player never calls it.
    pub fn enable_debugger(&mut self) {
        self.server.enable_debugger();
        self.client.enable_debugger();
    }

    /// Replaces every breakpoint: lines (from 1) by script path, as passed
    /// to [`Self::start_script`]. Does nothing unless
    /// [`Self::enable_debugger`] was called. Removing the breakpoint that's
    /// paused doesn't continue; the host does that.
    pub fn set_breakpoints(&mut self, breakpoints: &Breakpoints) {
        self.server.set_breakpoints(breakpoints);
        self.client.set_breakpoints(breakpoints);
    }

    /// Replaces every breakpoint's condition, hit count and log message
    /// ([`BreakpointRule`]), by script path and line. A breakpoint with no
    /// rule always pauses.
    pub fn set_breakpoint_rules(&mut self, rules: &BreakpointRules) {
        self.server.set_breakpoint_rules(rules);
        self.client.set_breakpoint_rules(rules);
    }

    /// Pauses at the next script line either side runs where a script can
    /// pause (the editor's Pause button), or stops asking with `false`.
    /// Continuing from any pause stops asking too.
    pub fn request_pause(&mut self, on: bool) {
        self.server.request_pause(on);
        self.client.request_pause(on);
    }

    /// Whether a pause was asked for and hasn't happened yet.
    pub fn pause_requested(&self) -> bool {
        self.server.pause_requested() || self.client.pause_requested()
    }

    /// Where a script is paused at a breakpoint, if one is. While paused,
    /// the host should stop running frames: nothing in the game should move.
    /// Anything it fires anyway (a Console command, an event) waits, and
    /// runs on [`Self::continue_from_pause`].
    pub fn pause(&self) -> Option<Pause> {
        self.server.pause().or_else(|| self.client.pause())
    }

    /// Continues from the pause: the paused script runs on from its line,
    /// then everything the pause held, in order. A breakpoint hit again
    /// pauses again, holding the rest.
    pub fn continue_from_pause(&mut self, world: &mut World) {
        self.resume_from_pause(world, Resume::Continue);
    }

    /// Continues from the pause, or steps the paused script (roadmap G6):
    /// it pauses again at the next line the step reaches, unless a
    /// breakpoint comes first or the script finishes. Everything the pause
    /// held runs on as for Continue.
    pub fn resume_from_pause(&mut self, world: &mut World, how: Resume) {
        let Some(side) = self.paused_side() else {
            return;
        };
        self.fire_changes(world);
        self.request_pause(false);
        self.context(side).resume_from_pause(world, how);
        self.collect(side);
        self.fire_changes(world);
    }

    /// Whether a script error pauses Play at the failed line (off by
    /// default). Does nothing unless [`Self::enable_debugger`] was called.
    pub fn set_pause_on_error(&mut self, on: bool) {
        self.server.set_pause_on_error(on);
        self.client.set_pause_on_error(on);
    }

    /// A watch expression's value as text in the paused call stack's frame
    /// `frame` (0 is the innermost), or why it has none. It can call
    /// functions and change things, as the line it's paused at could.
    pub fn evaluate(
        &mut self,
        world: &mut World,
        frame: usize,
        expression: &str,
    ) -> Result<String, String> {
        let side = self.paused_side().ok_or("Play isn't paused.")?;
        let value = self.context(side).evaluate(world, frame, expression);
        self.collect(side);
        value
    }

    pub(crate) fn paused_side(&self) -> Option<Side> {
        [Side::Server, Side::Client]
            .into_iter()
            .find(|&side| self.context(side).pause().is_some())
    }
}
