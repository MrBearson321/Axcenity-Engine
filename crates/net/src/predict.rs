//! Prediction and reconciliation for server-moved characters (multiplayer
//! readiness MP6a, plan D3).
//!
//! With `Players.CharacterAuthority` set to "Server", a player's device no
//! longer says where its character is. Each step it takes of its own
//! character is a numbered [`Move`]: the keys, the camera's turn and how
//! long the step was. The device takes the step at once (its prediction)
//! and sends its moves the server hasn't confirmed, several times over on
//! the unreliable channel. The server takes the same steps with the same
//! character controller, within the time that has passed on its own clock,
//! and answers with where that left the character ([`Moved`]). The device
//! compares that with what it predicted for the same move; when they
//! differ, it puts its character where the server has it and takes its
//! moves since again (the rollback). A small difference is eased away
//! over a few frames; a large one (a respawn, a teleport) snaps.
//!
//! Floats differ between machines and so do other players' positions, so
//! the answer and the prediction are close, not equal: the comparison
//! ignores what no one would see ([`CharacterState::matches`]).

use std::collections::{BTreeMap, VecDeque};
use std::time::Instant;

use axcenity_assets::Assets;
use axcenity_character::{CharacterState, Controllers, Drive, MoveInput};
use axcenity_core::{InstanceId, Vector3, World};
use serde::{Deserialize, Serialize};

/// The first byte of a device's moves on the unreliable channel.
pub const MOVES_TAG: u8 = b'P';
/// The first byte of the server's answer.
pub const MOVED_TAG: u8 = b'Q';

/// The most moves in one packet: the newest the server hasn't answered.
const MOST_SENT: usize = 32;
/// Moves a device sends at most this often (each packet carries all the
/// unanswered ones, so a fast frame rate doesn't send more packets).
const SEND_EVERY: f64 = 1.0 / 60.0;
/// The most moves the server keeps waiting for one player.
const MOST_WAITING: usize = 256;
/// The most moves the server takes for one player in a frame.
const MOST_PER_FRAME: usize = 64;
/// A move is at most this long, in seconds (the player's frames are
/// shorter than this, except after a hitch).
const LONGEST: f32 = 0.25;
/// And counts as at least this long against the player's time, so a
/// flood of tiny moves can't cost the server more than real ones.
const SHORTEST: f64 = 0.002;
/// How far a player's moves may run ahead of the server's clock, in
/// seconds: time saved up while no moves came (a late packet, a pause) is
/// spent when they arrive, never more than this.
pub const AHEAD: f64 = 0.5;
/// A correction further than this snaps; a nearer one is eased.
pub const SNAP: f32 = 4.0;
/// How fast an eased correction goes: half of what's left in this long.
const EASE_HALF_LIFE: f32 = 0.05;

const FORWARD: u8 = 1;
const BACK: u8 = 2;
const LEFT: u8 = 4;
const RIGHT: u8 = 8;
const JUMP: u8 = 16;
const CROUCH: u8 = 32;
const RUN: u8 = 64;

/// One step a device took of its own character.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Move {
    pub seq: u32,
    pub seconds: f32,
    pub yaw: f32,
    pub keys: u8,
    pub stick: Option<[f32; 2]>,
}

impl Move {
    fn of(seq: u32, drive: &Drive) -> Move {
        let input = drive.input;
        let keys = [
            (input.forward, FORWARD),
            (input.back, BACK),
            (input.left, LEFT),
            (input.right, RIGHT),
            (input.jump, JUMP),
            (input.crouch, CROUCH),
            (input.run, RUN),
        ]
        .into_iter()
        .filter(|&(held, _)| held)
        .fold(0, |keys, (_, bit)| keys | bit);
        Move {
            seq,
            seconds: drive.seconds,
            yaw: drive.yaw,
            keys,
            stick: (input.stick != [0.0, 0.0]).then_some(input.stick),
        }
    }

    pub fn drive(&self) -> Drive {
        let held = |bit: u8| self.keys & bit != 0;
        Drive {
            input: MoveInput {
                forward: held(FORWARD),
                back: held(BACK),
                left: held(LEFT),
                right: held(RIGHT),
                jump: held(JUMP),
                crouch: held(CROUCH),
                run: held(RUN),
                stick: self.stick.unwrap_or_default(),
                ..MoveInput::default()
            },
            yaw: self.yaw,
            seconds: if self.seconds.is_finite() {
                self.seconds.clamp(0.0, LONGEST)
            } else {
                0.0
            },
        }
    }
}

#[derive(Serialize, Deserialize)]
struct Moves {
    moves: Vec<Move>,
}

/// A packet of `moves`.
pub fn moves_to_bytes(moves: &[Move]) -> Vec<u8> {
    let mut bytes = vec![MOVES_TAG];
    bytes.extend(
        postcard::to_allocvec(&Moves {
            moves: moves.to_vec(),
        })
        .unwrap_or_default(),
    );
    bytes
}

pub fn moves_from_bytes(bytes: &[u8]) -> Option<Vec<Move>> {
    let (&tag, rest) = bytes.split_first()?;
    if tag != MOVES_TAG {
        return None;
    }
    let moves: Moves = postcard::from_bytes(rest).ok()?;
    (moves.moves.len() <= MOST_SENT).then_some(moves.moves)
}

/// The server's answer: where its character was after move `seq`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Moved {
    pub seq: u32,
    pub feet: [f32; 3],
    pub facing: f32,
    pub rising: f32,
    /// Grounded 1, airborne 2, crouched 4.
    pub flags: u8,
}

impl Moved {
    fn of(seq: u32, state: &CharacterState) -> Moved {
        Moved {
            seq,
            feet: [state.feet.x, state.feet.y, state.feet.z],
            facing: state.facing,
            rising: state.rising,
            flags: u8::from(state.grounded)
                | (u8::from(state.airborne) << 1)
                | (u8::from(state.crouched) << 2),
        }
    }

    pub fn state(&self) -> CharacterState {
        let [x, y, z] = self.feet;
        CharacterState {
            feet: Vector3::new(x, y, z),
            facing: self.facing,
            rising: self.rising,
            grounded: self.flags & 1 != 0,
            airborne: self.flags & 2 != 0,
            crouched: self.flags & 4 != 0,
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = vec![MOVED_TAG];
        bytes.extend(postcard::to_allocvec(self).unwrap_or_default());
        bytes
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Moved> {
        let (&tag, rest) = bytes.split_first()?;
        if tag != MOVED_TAG {
            return None;
        }
        postcard::from_bytes(rest).ok()
    }
}

/// How prediction went on a device, since it joined.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PredictionStats {
    /// Moves taken, and answers that differed from the prediction.
    pub moves: u64,
    pub corrections: u64,
    /// Moves taken again after corrections.
    pub replayed: u64,
    /// The time those took, in seconds, and the longest one frame's did.
    pub replay_seconds: f64,
    pub longest_replay: f64,
    /// The largest correction, and how far all of them went (studs), so
    /// a mean is `total / corrections`.
    pub largest: f32,
    pub total: f64,
    /// Corrections that snapped rather than eased.
    pub snaps: u64,
    /// Moves dropped unanswered because too many waited (a server that
    /// answers nothing: an older one, or a lost link).
    pub dropped: u64,
}

/// A device's side: its moves, what it predicted after each, and how far
/// its character is shown from where it predicts it.
#[derive(Default)]
pub struct Predictor {
    /// The controller the moves are for (a respawn starts again).
    controller: Option<InstanceId>,
    next: u32,
    /// Moves the server hasn't answered, each with the state predicted
    /// after it (without the eased offset).
    history: VecDeque<(Move, CharacterState)>,
    /// The newest answer not yet compared.
    answer: Option<Moved>,
    /// Where the character is shown, less where it's predicted.
    offset: Vector3,
    since_sent: f64,
    stats: PredictionStats,
}

impl Predictor {
    pub fn stats(&self) -> PredictionStats {
        self.stats
    }

    /// Starts the counts again (a measurement's warm-up).
    #[cfg(test)]
    pub fn reset_stats(&mut self) {
        self.stats = PredictionStats::default();
    }

    /// The server's answer arrived (the newest one counts).
    pub fn heard(&mut self, moved: Moved) {
        if self.answer.is_none_or(|known| moved.seq > known.seq) {
            self.answer = Some(moved);
        }
    }

    /// After the device's step: the move it took, for controller `id`.
    pub fn record(
        &mut self,
        world: &World,
        controllers: &Controllers,
        id: InstanceId,
        drive: Drive,
    ) {
        if self.controller != Some(id) {
            // A new character: nothing before it applies.
            self.controller = Some(id);
            self.history.clear();
            self.offset = Vector3::default();
        }
        let Some(mut state) = controllers.state(world, id) else {
            return;
        };
        state.feet = sub(state.feet, self.offset);
        self.next = self.next.wrapping_add(1);
        self.history.push_back((Move::of(self.next, &drive), state));
        self.stats.moves += 1;
        // A server that answers nothing (an older one, or a lost link)
        // leaves the history to grow; only the newest matter.
        while self.history.len() > MOST_WAITING {
            self.history.pop_front();
            self.stats.dropped += 1;
        }
    }

    /// The packet of moves to send this frame, if one is due.
    pub fn packet(&mut self, seconds: f64) -> Option<Vec<u8>> {
        self.since_sent += seconds;
        if self.since_sent < SEND_EVERY || self.history.is_empty() {
            return None;
        }
        self.since_sent = 0.0;
        let skip = self.history.len().saturating_sub(MOST_SENT);
        let moves: Vec<Move> = self.history.iter().skip(skip).map(|(m, _)| *m).collect();
        Some(moves_to_bytes(&moves))
    }

    /// Before the frame's step: compares the server's newest answer with
    /// the prediction for that move, and when they differ puts the
    /// character where the server had it and takes the moves since again;
    /// then eases the shown character toward the predicted one.
    pub fn reconcile(
        &mut self,
        world: &mut World,
        assets: &Assets,
        controllers: &mut Controllers,
        seconds: f32,
    ) {
        let Some(id) = self.controller else {
            return;
        };
        if let Some(answer) = self.answer.take() {
            self.compare(world, assets, controllers, id, answer);
        }
        self.ease(world, controllers, id, seconds);
    }

    fn compare(
        &mut self,
        world: &mut World,
        assets: &Assets,
        controllers: &mut Controllers,
        id: InstanceId,
        answer: Moved,
    ) {
        while self
            .history
            .front()
            .is_some_and(|(m, _)| seq_before(m.seq, answer.seq))
        {
            // Taken in the same server frame as the answered one.
            self.history.pop_front();
        }
        if self.history.front().map(|(m, _)| m.seq) != Some(answer.seq) {
            return;
        }
        let Some((_, predicted)) = self.history.pop_front() else {
            return;
        };
        let server = answer.state();
        if predicted.matches(&server) {
            return;
        }
        let Some(shown) = controllers.state(world, id) else {
            return;
        };
        let started = Instant::now();
        controllers.set_state(world, assets, id, &server);
        for (m, state) in self.history.iter_mut() {
            controllers.drive_again(world, assets, id, m.drive());
            if let Some(now) = controllers.state(world, id) {
                *state = now;
            }
        }
        let took = started.elapsed().as_secs_f64();
        self.stats.replayed += self.history.len() as u64;
        self.stats.replay_seconds += took;
        self.stats.longest_replay = self.stats.longest_replay.max(took);
        let Some(now) = controllers.state(world, id) else {
            return;
        };
        // How far the prediction moved, and where that leaves the shown
        // character from the new prediction.
        let jump = length(sub(now.feet, sub(shown.feet, self.offset)));
        self.stats.corrections += 1;
        self.stats.total += f64::from(jump);
        self.stats.largest = self.stats.largest.max(jump);
        let offset = sub(shown.feet, now.feet);
        if length(offset) > SNAP {
            self.offset = Vector3::default();
            self.stats.snaps += 1;
        } else {
            self.offset = offset;
            controllers.nudge(world, id, offset);
        }
    }

    fn ease(
        &mut self,
        world: &mut World,
        controllers: &mut Controllers,
        id: InstanceId,
        seconds: f32,
    ) {
        if self.offset == Vector3::default() {
            return;
        }
        let keep = 0.5f32.powf(seconds.max(0.0) / EASE_HALF_LIFE);
        let mut next = scale(self.offset, keep);
        if length(next) < 0.001 {
            next = Vector3::default();
        }
        controllers.nudge(world, id, sub(next, self.offset));
        self.offset = next;
    }
}

/// `a` comes before `b`, counting round when the numbers wrap.
fn seq_before(a: u32, b: u32) -> bool {
    a != b && b.wrapping_sub(a) < u32::MAX / 2
}

fn sub(a: Vector3, b: Vector3) -> Vector3 {
    Vector3::new(a.x - b.x, a.y - b.y, a.z - b.z)
}

fn scale(a: Vector3, by: f32) -> Vector3 {
    Vector3::new(a.x * by, a.y * by, a.z * by)
}

fn length(a: Vector3) -> f32 {
    (a.x * a.x + a.y * a.y + a.z * a.z).sqrt()
}

/// The server's side for one player: their moves waiting, the last one
/// taken, and how much time they have to spend.
#[derive(Default)]
pub(crate) struct Driven {
    waiting: BTreeMap<u32, Move>,
    last: Option<u32>,
    allowance: Option<f64>,
    /// The answer to send this frame.
    answer: Option<Moved>,
}

impl Driven {
    /// Moves that arrived; old ones (taken already) are dropped.
    pub fn receive(&mut self, moves: Vec<Move>) {
        for m in moves {
            if self.last.is_some_and(|last| !seq_before(last, m.seq)) {
                continue;
            }
            self.waiting.insert(m.seq, m);
        }
        while self.waiting.len() > MOST_WAITING {
            self.waiting.pop_first();
        }
    }

    /// The server's frame of `seconds`: takes the waiting moves in order
    /// with controller `id`, as far as the player's time allows.
    pub fn step(
        &mut self,
        world: &mut World,
        assets: &Assets,
        controllers: &mut Controllers,
        id: InstanceId,
        seconds: f64,
    ) {
        let allowance = self
            .allowance
            .map_or(AHEAD, |allowance| (allowance + seconds.max(0.0)).min(AHEAD));
        let mut allowance = allowance;
        let mut taken = 0;
        while taken < MOST_PER_FRAME {
            let Some((&seq, m)) = self.waiting.first_key_value() else {
                break;
            };
            let drive = m.drive();
            let cost = f64::from(drive.seconds).max(SHORTEST);
            if cost > allowance {
                break;
            }
            allowance -= cost;
            self.waiting.pop_first();
            controllers.drive(world, assets, id, drive);
            self.last = Some(seq);
            taken += 1;
        }
        self.allowance = Some(allowance);
        if taken > 0
            && let (Some(seq), Some(state)) = (self.last, controllers.state(world, id))
        {
            self.answer = Some(Moved::of(seq, &state));
        }
    }

    pub fn take_answer(&mut self) -> Option<Moved> {
        self.answer.take()
    }
}
