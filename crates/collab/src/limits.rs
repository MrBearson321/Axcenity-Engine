//! How much of each kind of message the host takes from one member (C10;
//! design §6.1). The connection as a whole may send
//! [`MAX_MESSAGES_PER_SECOND`](crate::session::MAX_MESSAGES_PER_SECOND);
//! inside that, kinds that make the host send a lot back, write to disk or
//! use up something shared each have a budget of their own.
//!
//! A budget is a *token bucket*: it holds up to `burst` tokens, refills at
//! `per_second`, and each message (or, for fetched files, each byte) takes
//! some. A full bucket lets a burst through at once, such as reopening
//! every script tab after a reconnect; only a client that keeps going
//! faster than any editor does runs it dry. Each budget is set well above
//! what an editor sends (see [`budget`]), so normal editing never meets
//! one.
//!
//! Edits themselves (`Submit`, `DocUpdate`) have no budget beyond the
//! connection's: dropping one would leave a copy out of step, a member
//! re-sends every unconfirmed edit at once after a reconnect, and each is
//! checked in full anyway.

use std::time::Instant;

/// The kinds of message with a budget of their own.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize)]
pub enum Kind {
    /// `Hello`: each answer can be the whole World.
    Hello,
    /// `NeedIds`: each answer uses up a block of the session's IDs.
    Ids,
    /// `Presence`: each is sent on to everyone.
    Presence,
    /// `Lock` and `Unlock`: each change is sent to everyone.
    Lock,
    /// `DocOpen`: each answer can be a whole document.
    DocOpen,
    /// `Fetch`, counted in bytes of the files sent back.
    FetchBytes,
    /// `Chat`: each line goes to everyone.
    Chat,
    /// `Checkpoint`: each writes a copy of the World to disk.
    Checkpoint,
    /// `Restore`: each changes everyone's session.
    Restore,
    /// `HostCard`: each change sends the successor list to everyone
    /// (Collaborate 2.0, P4).
    HostCard,
    /// `Tail`: up to 16 MiB to decode; one answers each `SendTail`.
    Tail,
}

impl Kind {
    pub const ALL: [Kind; 11] = [
        Kind::Hello,
        Kind::Ids,
        Kind::Presence,
        Kind::Lock,
        Kind::DocOpen,
        Kind::FetchBytes,
        Kind::Chat,
        Kind::Checkpoint,
        Kind::Restore,
        Kind::HostCard,
        Kind::Tail,
    ];

    /// How the audit log and the design doc name it.
    pub fn name(self) -> &'static str {
        match self {
            Kind::Hello => "joins and resyncs",
            Kind::Ids => "ID blocks",
            Kind::Presence => "presence",
            Kind::Lock => "document locks",
            Kind::DocOpen => "opening live documents",
            Kind::FetchBytes => "file downloads",
            Kind::Chat => "chat",
            Kind::Checkpoint => "checkpoints",
            Kind::Restore => "restores",
            Kind::HostCard => "host cards",
            Kind::Tail => "transactions for a new host",
        }
    }
}

/// How much of one kind a member may send.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Budget {
    /// Tokens added each second.
    pub per_second: f64,
    /// The most tokens held, and so the biggest burst.
    pub burst: f64,
}

const MIB: f64 = (1 << 20) as f64;

/// Each kind's budget, with what an editor sends for comparison.
pub fn budget(kind: Kind) -> Budget {
    let (per_second, burst) = match kind {
        // One per connection (reconnects back off from 2 s) and one per
        // resync after a gap. Over it, the connection closes.
        Kind::Hello => (1.0, 20.0),
        // One per 1,024 new Instances, asked for one at a time.
        Kind::Ids => (20.0, 50.0),
        // At most 10 a second (the client coalesces to the latest).
        Kind::Presence => (30.0, 60.0),
        // One per document tab started or saved.
        Kind::Lock => (20.0, 100.0),
        // One per live tab opened; every open tab again after a reconnect.
        Kind::DocOpen => (20.0, 500.0),
        // A joiner fetches the project's files once; any file fits a full
        // bucket ([`MAX_FILE`](crate::files::MAX_FILE) is 1 GiB).
        Kind::FetchBytes => (128.0 * MIB, 2048.0 * MIB),
        // Typed by a person.
        Kind::Chat => {
            let most = crate::presence::CHAT_PER_SECOND as f64;
            (most, most)
        }
        // Asked for by a person.
        Kind::Checkpoint => (0.1, 3.0),
        Kind::Restore => (0.2, 2.0),
        // One per join, and when the person's address changes.
        Kind::HostCard => (0.2, 5.0),
        // One per recovering host that asks.
        Kind::Tail => (0.1, 3.0),
    };
    Budget { per_second, burst }
}

/// The most file hashes one `Fetch` waits with (a project with more files
/// than this asks again for the rest).
pub const FETCH_QUEUED_MOST: usize = 100_000;

/// The most files one member uploads at once (the client sends them one
/// after another, so it's 1 in practice).
pub const UPLOADS_AT_ONCE: usize = 4;

/// How many connections the host keeps open at once; more are closed as
/// they open, before any TLS work.
pub const CONNECTIONS_MOST: usize = 256;

/// How many connections may be partway through the handshake at once;
/// more are closed as they open.
pub const KNOCKING_MOST: usize = 64;

/// How long a connection has to finish the handshake.
pub const KNOCK_TIME: std::time::Duration = std::time::Duration::from_secs(10);

/// How long a joiner waits for the person hosting to let them in (when
/// the host asks, cohesion pass CW7.2) before they're turned away.
pub const ASK_TIME: std::time::Duration = std::time::Duration::from_secs(120);

/// Handshakes one address may fail in [`FAILURES_WINDOW`] before its new
/// connections are closed until the window has passed.
pub const FAILURES_MOST: u32 = 10;

/// How long failed handshakes count against an address.
pub const FAILURES_WINDOW: std::time::Duration = std::time::Duration::from_secs(60);

#[derive(Clone, Copy, Debug)]
struct Bucket {
    tokens: f64,
    at: Option<Instant>,
}

/// Each kind's budget: [`budget`], or another table (tests).
pub type Budgets = fn(Kind) -> Budget;

/// One member's buckets.
#[derive(Clone, Debug)]
pub struct Limits {
    budgets: Budgets,
    buckets: [Bucket; Kind::ALL.len()],
}

impl Default for Limits {
    fn default() -> Limits {
        Limits::new(budget)
    }
}

impl Limits {
    /// Full buckets for `budgets`.
    pub fn new(budgets: Budgets) -> Limits {
        let mut buckets = [Bucket {
            tokens: 0.0,
            at: None,
        }; Kind::ALL.len()];
        for kind in Kind::ALL {
            buckets[kind as usize].tokens = budgets(kind).burst;
        }
        Limits { budgets, buckets }
    }

    /// Takes `cost` tokens of `kind` at `now`; false (and nothing taken)
    /// when there aren't enough.
    pub fn take(&mut self, kind: Kind, cost: f64, now: Instant) -> bool {
        if !self.has(kind, cost, now) {
            return false;
        }
        self.buckets[kind as usize].tokens -= cost;
        true
    }

    /// Whether `cost` tokens of `kind` are there at `now`.
    pub fn has(&mut self, kind: Kind, cost: f64, now: Instant) -> bool {
        let Budget { per_second, burst } = (self.budgets)(kind);
        let bucket = &mut self.buckets[kind as usize];
        if let Some(at) = bucket.at {
            let since = now.saturating_duration_since(at).as_secs_f64();
            bucket.tokens = (bucket.tokens + since * per_second).min(burst);
        }
        bucket.at = Some(now);
        bucket.tokens >= cost
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn a_burst_goes_through_then_the_rate_holds() {
        let start = Instant::now();
        let mut limits = Limits::default();
        let burst = budget(Kind::Lock).burst as usize;
        for _ in 0..burst {
            assert!(limits.take(Kind::Lock, 1.0, start));
        }
        assert!(!limits.take(Kind::Lock, 1.0, start));
        // A second later, `per_second` more.
        let later = start + Duration::from_secs(1);
        for _ in 0..budget(Kind::Lock).per_second as usize {
            assert!(limits.take(Kind::Lock, 1.0, later));
        }
        assert!(!limits.take(Kind::Lock, 1.0, later));
        // Other kinds have their own.
        assert!(limits.take(Kind::Presence, 1.0, later));
        // Never more than the burst, however long it waits.
        let much_later = later + Duration::from_secs(3600);
        assert!(limits.has(Kind::Lock, budget(Kind::Lock).burst, much_later));
        assert!(!limits.has(Kind::Lock, budget(Kind::Lock).burst + 1.0, much_later));
    }

    #[test]
    fn any_file_fits_a_full_download_budget() {
        assert!(budget(Kind::FetchBytes).burst >= crate::files::MAX_FILE as f64);
    }
}
