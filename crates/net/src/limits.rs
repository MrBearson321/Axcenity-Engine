//! How much a player's device may send the host (multiplayer readiness
//! MP3, decision D4 of `docs/research/multiplayer-readiness.md`), as
//! Collaborate's `limits.rs` does for editors.
//!
//! A budget is a *token bucket*: it holds up to `burst` tokens, refills at
//! `per_second`, and each message (or byte) takes some. A full bucket lets
//! a burst through, such as a script firing a RemoteEvent for each of 100
//! parts in one frame; only a device that keeps going faster than a game
//! client does runs it dry. Then the message is dropped and the player is
//! flagged (`SessionEvent::Flagged`): the host counts flags, fires
//! `Players.PlayerFlagged` and kicks a player flagged too often.

use std::time::Duration;

/// What a player's device sends, each with a budget.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    /// Every byte, on both channels.
    Bytes,
    /// RemoteEvent and RemoteFunction messages, and Ability activations.
    Remote,
    /// Chat messages.
    Chat,
    /// Developer Console command lines.
    Console,
    /// `StreamAround` requests.
    Stream,
    /// Packets on the unreliable channel: motion of its character and the
    /// parts it owns.
    Datagram,
}

impl Kind {
    pub const ALL: [Kind; 6] = [
        Kind::Bytes,
        Kind::Remote,
        Kind::Chat,
        Kind::Console,
        Kind::Stream,
        Kind::Datagram,
    ];

    /// Why the player is flagged when it's over: what a script's
    /// `PlayerFlagged` handler gets.
    pub fn reason(self) -> &'static str {
        match self {
            Kind::Bytes => "sent more data than a game client does",
            Kind::Remote => "fired remote events faster than a game client does",
            Kind::Chat => "sent chat messages too fast",
            Kind::Console => "sent console commands too fast",
            Kind::Stream => "asked for parts of the world too fast",
            Kind::Datagram => "sent motion faster than a game client does",
        }
    }
}

/// How much of one kind a player may send.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Budget {
    /// Tokens added each second.
    pub per_second: f64,
    /// The most tokens held, and so the biggest burst.
    pub burst: f64,
}

/// Each kind's budget, with what a game client sends for comparison.
pub fn budget(kind: Kind) -> Budget {
    let (per_second, burst) = match kind {
        // A walking player sends about 13 KB/s (MP1's bots); remote calls
        // with tables of data can add a lot more for a moment.
        Kind::Bytes => (256.0 * 1024.0, 1024.0 * 1024.0),
        // A game that fires on every frame and every input stays well under.
        Kind::Remote => (240.0, 600.0),
        // Typed by a person.
        Kind::Chat => (2.0, 8.0),
        Kind::Console => (5.0, 20.0),
        // One per teleport or camera jump in a streaming game.
        Kind::Stream => (10.0, 30.0),
        // One own-character packet and one owned-parts packet a frame.
        Kind::Datagram => (180.0, 360.0),
    };
    Budget { per_second, burst }
}

#[derive(Clone, Copy, Debug)]
struct Bucket {
    tokens: f64,
    /// When it was last refilled, by the host's clock.
    at: Duration,
}

/// One player's buckets.
#[derive(Clone, Debug)]
pub struct Limits {
    buckets: [Bucket; Kind::ALL.len()],
}

impl Limits {
    /// Full buckets, at `now`.
    pub fn new(now: Duration) -> Limits {
        Limits {
            buckets: Kind::ALL.map(|kind| Bucket {
                tokens: budget(kind).burst,
                at: now,
            }),
        }
    }

    /// Takes `amount` of `kind` at `now`: false, and nothing taken, when
    /// the bucket doesn't hold that much.
    pub fn take(&mut self, kind: Kind, amount: f64, now: Duration) -> bool {
        let budget = budget(kind);
        let bucket = &mut self.buckets[kind as usize];
        let seconds = now.saturating_sub(bucket.at).as_secs_f64();
        bucket.tokens = (bucket.tokens + seconds * budget.per_second).min(budget.burst);
        bucket.at = now;
        if bucket.tokens < amount {
            return false;
        }
        bucket.tokens -= amount;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_full_bucket_lets_a_burst_through_then_refills_at_its_rate() {
        let mut limits = Limits::new(Duration::ZERO);
        let chat = budget(Kind::Chat);
        let mut sent = 0;
        while limits.take(Kind::Chat, 1.0, Duration::ZERO) {
            sent += 1;
        }
        assert_eq!(sent, chat.burst as usize);
        // Half a second later, one more message's worth.
        assert!(limits.take(Kind::Chat, 1.0, Duration::from_millis(500)));
        assert!(!limits.take(Kind::Chat, 1.0, Duration::from_millis(500)));
        // Other kinds have buckets of their own.
        assert!(limits.take(Kind::Console, 1.0, Duration::from_millis(500)));
    }

    #[test]
    fn every_kind_has_a_reason_and_room_for_what_a_game_client_sends() {
        for kind in Kind::ALL {
            assert!(!kind.reason().is_empty());
            let budget = budget(kind);
            assert!(budget.burst >= budget.per_second, "{kind:?}");
        }
        // 60 frames a second of motion for a character and its parts.
        assert!(budget(Kind::Datagram).per_second >= 120.0);
        assert!(budget(Kind::Bytes).per_second >= 64.0 * 1024.0);
    }
}
