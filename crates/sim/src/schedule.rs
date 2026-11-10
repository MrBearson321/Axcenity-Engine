//! Stages, steps and the schedule that runs them.

use axcenity_core::bench::Laps;

use crate::host::FrameCtx;
use crate::steps::{DRIFT, LAPS, STANDARD};

/// The parts of a frame, in the order they run, named after Roblox's
/// `RunService` events so creators recognize them.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Stage {
    NetReceive,
    Input,
    PreSimulation,
    Simulation,
    PostSimulation,
    Heartbeat,
    PreAnimation,
    Animation,
    PreRender,
    NetSend,
}

/// Which hosts run a step. A small bit set: each host is one bit.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Hosts(u8);

impl Hosts {
    pub const PLAYER: Hosts = Hosts(1);
    pub const EDITOR_PLAY: Hosts = Hosts(2);
    pub const SERVER: Hosts = Hosts(4);
    pub const TEST: Hosts = Hosts(8);
    pub const ALL: Hosts = Hosts(15);
    /// The hosts with a player of their own: everything but the server.
    pub const GAME: Hosts = Hosts(1 | 2 | 8);
    /// The hosts on a network: everything but the test runner.
    pub const NETWORKED: Hosts = Hosts(1 | 2 | 4);

    /// True if every host in `other` is in this set.
    pub const fn contains(self, other: Hosts) -> bool {
        self.0 & other.0 == other.0
    }

    /// Both sets together.
    pub const fn with(self, other: Hosts) -> Hosts {
        Hosts(self.0 | other.0)
    }
}

/// One step of a frame.
#[derive(Copy, Clone)]
pub struct Step {
    pub name: &'static str,
    pub stage: Stage,
    pub hosts: Hosts,
    pub run: fn(&mut FrameCtx),
    /// The benchmark step this one ends (`core::bench`, `docs/benchmarks.md`),
    /// if it ends one. Set per host by [`Schedule::for_host`].
    pub lap: Option<&'static str>,
}

/// An ordered list of steps.
#[derive(Clone, Default)]
pub struct Schedule {
    steps: Vec<Step>,
}

impl Schedule {
    /// The one order, every host's steps together, transcribed from the
    /// player's frame.
    pub fn standard() -> Self {
        Self {
            steps: STANDARD.to_vec(),
        }
    }

    /// The steps `host` runs, in the order it runs them: the standard
    /// order, except where the host is still listed in `DRIFT`.
    pub fn for_host(host: Hosts) -> Self {
        let mut schedule = Self {
            steps: STANDARD
                .iter()
                .filter(|step| step.hosts.contains(host))
                .copied()
                .collect(),
        };
        for drift in DRIFT.iter().filter(|drift| drift.host == host) {
            let from = schedule.position(drift.step);
            let step = schedule.steps.remove(from);
            let to = schedule.position(drift.after) + 1;
            schedule.steps.insert(to, step);
        }
        let laps = LAPS
            .iter()
            .find(|(hosts, _)| *hosts == host)
            .map_or(&[][..], |(_, laps)| *laps);
        for &(after, lap) in laps {
            let at = schedule.position(after);
            schedule.steps[at].lap = Some(lap);
        }
        schedule
    }

    /// Puts `step` right after the step named `anchor`.
    ///
    /// # Panics
    ///
    /// If there's no step named `anchor`.
    pub fn insert_after(&mut self, anchor: &str, step: Step) {
        let at = self.position(anchor);
        self.steps.insert(at + 1, step);
    }

    /// The steps, in order.
    pub fn steps(&self) -> &[Step] {
        &self.steps
    }

    /// The steps' names, in order.
    pub fn names(&self) -> Vec<&'static str> {
        self.steps.iter().map(|step| step.name).collect()
    }

    /// Runs one frame on `ctx`'s host, in a profiler span per step, timing
    /// the benchmark steps into `laps`. A step that ends the frame (see
    /// [`FrameCtx::end_frame`]) stops it there.
    pub fn run(&self, ctx: &mut FrameCtx, laps: &mut Laps) {
        let host = ctx.host;
        for step in self.steps.iter().filter(|s| s.hosts.contains(host)) {
            profiling::scope!(step.name);
            (step.run)(ctx);
            if let Some(lap) = step.lap {
                laps.lap(lap);
            }
            if ctx.ended() {
                break;
            }
        }
    }

    fn position(&self, name: &str) -> usize {
        self.steps
            .iter()
            .position(|step| step.name == name)
            .unwrap_or_else(|| panic!("no step named {name}"))
    }
}
