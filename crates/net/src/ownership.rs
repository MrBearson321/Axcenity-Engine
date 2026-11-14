//! Network ownership and characters (Phase 66).
//!
//! A player's own character, and any assembly the server gives them
//! (`SetNetworkOwner`, Auto, or the VehicleSeat they drive), moves on
//! their device with no delay. Each frame the device sends where those
//! parts are ([`Owned`], on the unreliable channel), whether its player is
//! running, and the seat its character sits in. The server checks the
//! character against its controller's limits ([`CharacterCheck`]): how
//! far it may go at its `WalkSpeed` (or `RunSpeed`), how high at its
//! `JumpHeight`, and never more than [`TELEPORT_DISTANCE`] at once. What
//! passes is put in the server's world, so the other players see it, by
//! motion, like any part. What fails is left where the server had it, and
//! the device is told where that is ([`Message::Correction`]); so is a
//! device whose character a server Script moved.
//!
//! A character standing on a part a SplineFollower moves rides it on its
//! device (Phase 73), so the check takes off as far as that part went
//! since the last update before measuring the character's own movement.

use std::collections::{BTreeMap, BTreeSet};

use axcenity_assets::Assets;
use axcenity_core::{ClassName, InstanceId, Value, Vector3, World};

use crate::motion::{Motion, PartState, exact_motion};
use crate::movement::{
    IN_THE_AIR, PART_TOO_FAST, Strikes, THROUGH_WALL, TOO_FAST, highest, supported, through_wall,
};
use crate::replicate::{Op, Update};

/// The first byte of an owned-parts packet on the unreliable channel.
pub const OWNED_TAG: u8 = b'O';
/// A character never moves further than this in one update, in studs.
pub const TELEPORT_DISTANCE: f64 = 30.0;
/// How much faster than its speed a character may seem to go, for
/// packets that arrive bunched together.
pub(crate) const TOLERANCE: f64 = 1.15;
/// The most distance saved up, in seconds of movement, plus [`SLACK`].
const BURST: f64 = 0.25;
const SLACK: f64 = 2.0;
/// Movement smaller than this is the wire's rounding (positions go to
/// 1/128 stud), not the character's: it costs nothing, so a character
/// standing still is never moved back, even on its first update.
const ROUNDING: f64 = 1.0 / 64.0;
/// The Workspace's default gravity, at which `JumpHeight` is measured.
const GRAVITY: f64 = 196.2;
/// A device sends all of its parts this often, in seconds, besides the
/// ones that moved, so a lost packet is soon made good.
pub const RESEND_ALL: f64 = 0.5;

/// What a device sends of the parts it moves.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Owned {
    pub motion: Motion,
    /// Whether its player holds Run.
    pub running: bool,
    /// The seat its character sits in (a network ID).
    pub seat: Option<u64>,
}

impl Owned {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = self.motion.to_bytes();
        bytes[0] = OWNED_TAG;
        bytes.push(u8::from(self.running));
        bytes.extend(self.seat.unwrap_or(0).to_le_bytes());
        bytes
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Owned> {
        if bytes.first() != Some(&OWNED_TAG) || bytes.len() < 10 {
            return None;
        }
        let (body, trailer) = bytes.split_at(bytes.len() - 9);
        let mut body = body.to_vec();
        body[0] = crate::motion::MOTION_TAG;
        let motion = Motion::from_bytes(&body)?;
        let seat = u64::from_le_bytes(trailer[1..9].try_into().ok()?);
        Some(Owned {
            motion,
            running: trailer[0] != 0,
            seat: (seat != 0).then_some(seat),
        })
    }
}

/// The parts of `player`'s character: its BaseParts.
pub fn character_parts(world: &World, player: InstanceId) -> Vec<InstanceId> {
    let Some(character) = character_of(world, player) else {
        return Vec::new();
    };
    world
        .descendants_iter(character)
        .filter(|&id| world.get(id).is_some_and(|i| i.class().is_base_part()))
        .collect()
}

pub fn character_of(world: &World, player: InstanceId) -> Option<InstanceId> {
    let character = world.get(player)?.property("Character")?.as_instance()?;
    world.contains(character).then_some(character)
}

/// The part a character is checked by: its PrimaryPart, else its
/// HumanoidRootPart, else its first part.
pub(crate) fn root_of(world: &World, character: InstanceId) -> Option<InstanceId> {
    world
        .get(character)?
        .property("PrimaryPart")
        .and_then(Value::as_instance)
        .filter(|&id| world.contains(id))
        .or_else(|| world.find_first_child(character, "HumanoidRootPart"))
        .or_else(|| {
            world
                .descendants_iter(character)
                .find(|&id| world.get(id).is_some_and(|i| i.class().is_base_part()))
        })
}

fn controller_of(world: &World, character: InstanceId) -> Option<InstanceId> {
    world
        .get(character)?
        .children()
        .iter()
        .copied()
        .find(|&id| {
            world
                .get(id)
                .is_some_and(|i| i.class().is_character_controller())
        })
}

fn number(world: &World, id: InstanceId, name: &str) -> Option<f64> {
    world.get(id)?.property(name)?.as_number()
}

fn horizontal(a: Vector3, b: Vector3) -> f64 {
    f64::from((a.x - b.x).hypot(a.z - b.z))
}

/// How fast an owned part may go across or up, in studs a second: faster
/// than any car or thrown part a game makes, slower than a teleport.
pub const ASSEMBLY_SPEED: f64 = 400.0;

/// Whose update [`CharacterCheck::receive`] takes in.
#[derive(Clone, Copy)]
pub struct Sender<'a> {
    pub player: InstanceId,
    /// The parts the server gives that player.
    pub assemblies: &'a BTreeSet<InstanceId>,
    /// Every player's character.
    pub characters: &'a [InstanceId],
}

/// What [`CharacterCheck::allows`] checks a move with.
struct Move<'a> {
    assets: &'a Assets,
    character: InstanceId,
    controller: InstanceId,
    /// Every player's character: a path through one isn't through a wall.
    characters: &'a [InstanceId],
    running: bool,
    dt: f64,
    now: f64,
}

/// The server's check of one player's character.
#[derive(Default)]
pub struct CharacterCheck {
    /// Studs it may still go across the ground.
    across: f64,
    /// Studs it may still rise.
    rise: f64,
    /// The server's clock at the last update.
    last: Option<f64>,
    /// What the server last put in its world from the device, or sent
    /// it, so a move of the server's own is told apart.
    pub(crate) applied: BTreeMap<InstanceId, PartState>,
    /// The device must be told where its character is.
    pub(crate) correct: bool,
    /// The seat the device last said its character sits in.
    seat: Option<Option<u64>>,
    /// Where each part a SplineFollower moves was at the last update.
    carriers: BTreeMap<InstanceId, Vector3>,
    /// The height and time the character last stood on something, or
    /// `None` until it next does (or the server moved it).
    ground: Option<(f64, f64)>,
    /// When each owned assembly last moved.
    assembly_at: BTreeMap<InstanceId, f64>,
    /// Refused moves.
    strikes: Strikes,
}

impl CharacterCheck {
    /// Takes in `owned` from `from`'s device at `now` on the server's
    /// clock: what it may move goes in `world`. Returns why the player is
    /// to be flagged, when it keeps making moves that are refused.
    pub fn receive(
        &mut self,
        world: &mut World,
        assets: &Assets,
        from: &Sender,
        owned: &Owned,
        now: f64,
    ) -> Option<&'static str> {
        let Sender {
            player,
            assemblies,
            characters,
        } = *from;
        let mut refused = None;
        // The first update has a full burst to spend, as if the character
        // had stood still before it.
        let dt = self.last.map_or(BURST, |last| (now - last).max(0.0));
        self.last = Some(now);
        let character = character_of(world, player);
        let parts: BTreeSet<InstanceId> = character_parts(world, player).into_iter().collect();
        let states: BTreeMap<InstanceId, PartState> = owned
            .motion
            .parts
            .iter()
            .map(|&(id, state)| (InstanceId::from_bits(id), state))
            .collect();
        if let Some(character) = character
            && let Some(controller) = controller_of(world, character)
        {
            if self.seat != Some(owned.seat) {
                self.seat = Some(owned.seat);
                self.sit(world, controller, owned.seat);
            }
            let check = Move {
                assets,
                character,
                controller,
                characters,
                running: owned.running,
                dt,
                now,
            };
            let ok =
                match root_of(world, character).and_then(|root| Some((root, states.get(&root)?))) {
                    Some((root, state)) => self.allows(world, &check, root, state),
                    // Only some parts came: they're checked with the root's next.
                    None => Ok(()),
                };
            if let Err(reason) = ok {
                refused = Some(reason);
                self.correct = true;
            } else {
                for (&id, state) in &states {
                    if parts.contains(&id) {
                        place(world, id, state);
                        self.applied.insert(id, *state);
                    }
                }
            }
        }
        // Owned assemblies: they go where the device says, unless that's a
        // jump no simulation makes, or faster than parts go.
        for (&id, state) in &states {
            if parts.contains(&id) || !assemblies.contains(&id) {
                continue;
            }
            let Some(was) = PartState::of(world, id) else {
                continue;
            };
            let (from, to) = (was.position(), state.position());
            let jump = crate::motion::distance(from, to);
            let seconds = self.assembly_at.get(&id).map_or(BURST, |&at| now - at);
            let across = horizontal(from, to).hypot(f64::from((to.y - from.y).max(0.0)));
            let reach = ASSEMBLY_SPEED * TOLERANCE * seconds.clamp(0.0, 1.0) + SLACK;
            if jump <= TELEPORT_DISTANCE * 2.0 && across <= reach {
                place(world, id, state);
                self.assembly_at.insert(id, now);
            } else {
                refused = Some(PART_TOO_FAST);
            }
        }
        self.assembly_at.retain(|id, _| assemblies.contains(id));
        let reason = refused?;
        self.strikes.strike(now).then_some(reason)
    }

    /// Whether the character's root may go to `state`, spending what it
    /// may move.
    fn allows(
        &mut self,
        world: &World,
        check: &Move,
        root: InstanceId,
        state: &PartState,
    ) -> Result<(), &'static str> {
        let Move {
            controller,
            running,
            dt,
            now,
            ..
        } = *check;
        let Some(was) = world
            .get(root)
            .and_then(|i| i.property("Position"))
            .and_then(Value::as_vector3)
        else {
            return Err(TOO_FAST);
        };
        // The server moved it (or it's new): it stands where it was put.
        let moved = self
            .applied
            .get(&root)
            .is_some_and(|applied| applied.position() != was);
        if moved || self.ground.is_none() {
            self.ground = Some((f64::from(was.y), now));
        }
        let to = state.position();
        // Falling is never a teleport: a character falls faster the longer
        // it falls (off the edge of a map, as far as it likes), so only
        // going across or up counts.
        let jump = horizontal(was, to).hypot(f64::from((to.y - was.y).max(0.0)));
        if jump > TELEPORT_DISTANCE {
            return Err(TOO_FAST);
        }
        let movement = world
            .get(controller)
            .and_then(|c| c.property("Movement"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        let speed = match movement.as_str() {
            "Run" => number(world, controller, "RunSpeed"),
            "Crouch" => number(world, controller, "WalkSpeed"),
            "" | "Walk" if running => number(world, controller, "RunSpeed"),
            "" | "Walk" => number(world, controller, "WalkSpeed"),
            // Seated, or a custom mode scripts move: only teleports are
            // checked.
            _ => None,
        };
        let Some(speed) = speed else {
            return Ok(());
        };
        let jump_height = number(world, controller, "JumpHeight").unwrap_or(7.2);
        let launch = (2.0 * GRAVITY * jump_height).sqrt();
        self.across = (self.across + speed * TOLERANCE * dt).min(speed * TOLERANCE * BURST + SLACK);
        self.rise = (self.rise + launch * TOLERANCE * dt).min(jump_height * 1.5 + SLACK);
        // What it stands on carried it this far; the rest is its own.
        let (carried_across, carried_up) = self.carried(world, was);
        let across = (horizontal(was, to) - carried_across * TOLERANCE - ROUNDING).max(0.0);
        let up = (f64::from((to.y - was.y).max(0.0)) - carried_up * TOLERANCE - ROUNDING).max(0.0);
        if across > self.across || up > self.rise {
            return Err(TOO_FAST);
        }
        let group = world
            .get(root)
            .and_then(|i| i.property("CollisionGroup"))
            .and_then(Value::as_str)
            .map(str::to_owned);
        if through_wall(
            world,
            check.assets,
            group,
            check.characters.to_vec(),
            was,
            to,
        ) {
            return Err(THROUGH_WALL);
        }
        if supported(world, check.assets, check.character, to) {
            self.ground = Some((f64::from(to.y), now));
        } else if let Some((height, since)) = self.ground
            && highest(world, height, jump_height, now - since)
                .is_some_and(|highest| f64::from(to.y) > highest)
        {
            return Err(IN_THE_AIR);
        }
        self.across -= across;
        self.rise -= up;
        Ok(())
    }

    /// How far across and up the SplineFollower-moved part under `at` (a
    /// character's root, where the server had it) went since the last
    /// update, and notes where every such part is now.
    fn carried(&mut self, world: &World, at: Vector3) -> (f64, f64) {
        let mut carried = (0.0, 0.0);
        let mut now = BTreeMap::new();
        for part in carriers(world) {
            let Some(instance) = world.get(part) else {
                continue;
            };
            let (Some(position), Some(size)) = (
                instance.property("Position").and_then(Value::as_vector3),
                instance.property("Size").and_then(Value::as_vector3),
            ) else {
                continue;
            };
            now.insert(part, position);
            let Some(&before) = self.carriers.get(&part) else {
                continue;
            };
            // Over it, where it was, with room for a turned part and a
            // character's height above its top.
            let reach = 0.5 * size.x.max(size.z) + 2.0;
            let above = at.y - (before.y + 0.5 * size.y);
            if (at.x - before.x).abs() <= reach
                && (at.z - before.z).abs() <= reach
                && (-1.0..=10.0).contains(&above)
            {
                let across = horizontal(before, position);
                if across >= carried.0 {
                    carried = (across, f64::from((position.y - before.y).max(0.0)));
                }
            }
        }
        self.carriers = now;
        carried
    }

    /// The character sits in `seat` on the device: so it does on the
    /// server, where scripts see `Occupant`.
    fn sit(&self, world: &mut World, controller: InstanceId, seat: Option<u64>) {
        let seats: Vec<InstanceId> = world.all_below(world.workspace(), ClassName::is_seat);
        let wanted = seat
            .map(InstanceId::from_bits)
            .filter(|id| seats.contains(id));
        for seat in seats {
            let occupant = world
                .get(seat)
                .and_then(|s| s.property("Occupant"))
                .and_then(Value::as_instance);
            if Some(seat) == wanted && occupant.is_none() {
                let _ = world.set_property(seat, "Occupant", Value::Instance(Some(controller)));
            } else if Some(seat) != wanted && occupant == Some(controller) {
                let _ = world.set_property(seat, "Occupant", Value::Instance(None));
            }
        }
    }

    /// The correction for `player`'s device, when one is due: where the
    /// server has its character. Also due when the server moved it.
    pub fn correction(&mut self, world: &World, player: InstanceId, time: f64) -> Option<Update> {
        let parts = character_parts(world, player);
        let moved = parts.iter().any(|&id| {
            let now = PartState::of(world, id);
            match self.applied.get(&id) {
                Some(applied) => Some(*applied) != now,
                // First seen: the device got it with the character.
                None => false,
            }
        });
        for &id in &parts {
            if let Some(state) = PartState::of(world, id) {
                self.applied.insert(id, state);
            }
        }
        self.applied.retain(|id, _| parts.contains(id));
        if !(self.correct || moved) {
            return None;
        }
        if moved {
            self.ground = None;
        }
        self.correct = false;
        let ops: Vec<Op> = parts
            .into_iter()
            .filter_map(|id| exact_motion(world, id))
            .collect();
        Some(Update {
            ops,
            time,
            ..Update::default()
        })
    }
}

fn place(world: &mut World, id: InstanceId, state: &PartState) {
    for (name, value) in [
        ("Position", state.position()),
        ("Orientation", state.orientation()),
        ("AssemblyLinearVelocity", state.velocity()),
    ] {
        let same = world
            .get(id)
            .and_then(|i| i.property(name))
            .and_then(Value::as_vector3)
            == Some(value);
        if !same {
            let _ = world.set_property(id, name, Value::Vector3(value));
        }
    }
}

/// A device's side: what it last sent of the parts it moves.
#[derive(Default)]
pub struct OwnedSender {
    sent: BTreeMap<InstanceId, PartState>,
    since_all: f64,
}

impl OwnedSender {
    /// The packet for this frame, of `parts`' changes (all of them every
    /// [`RESEND_ALL`]), or `None` when nothing is due.
    pub fn frame(
        &mut self,
        world: &World,
        parts: &[InstanceId],
        running: bool,
        seat: Option<InstanceId>,
        seconds: f64,
        time: f64,
    ) -> Option<Owned> {
        self.since_all += seconds;
        let all = self.since_all >= RESEND_ALL;
        if all {
            self.since_all = 0.0;
        }
        let mut motion = Motion {
            time,
            parts: Vec::new(),
        };
        for &id in parts {
            let Some(state) = PartState::of(world, id) else {
                continue;
            };
            if all || self.sent.get(&id) != Some(&state) {
                motion.parts.push((id.to_bits(), state));
                self.sent.insert(id, state);
            }
        }
        self.sent.retain(|id, _| parts.contains(id));
        (!motion.parts.is_empty() || all).then(|| Owned {
            motion,
            running,
            seat: seat.map(InstanceId::to_bits),
        })
    }
}

/// The parts SplineFollowers move: a follower's part and the parts welded
/// to it, or the parts in its Model.
fn carriers(world: &World) -> Vec<InstanceId> {
    let mut parts = Vec::new();
    for id in world.all_below(world.workspace(), |class| {
        class == ClassName::SplineFollower
    }) {
        let Some(follower) = world.get(id) else {
            continue;
        };
        let Some(target) = follower.parent() else {
            continue;
        };
        parts.extend(
            std::iter::once(target)
                .chain(world.descendants_iter(target))
                .filter(|&part| world.get(part).is_some_and(|i| i.class().is_base_part())),
        );
    }
    parts
}
