//! Moving parts (Phase 65): where the server's parts are, sent often,
//! small and on the unreliable channel, then smoothed on players' devices.
//!
//! A part's `Position`, `Orientation`, `AssemblyLinearVelocity` and
//! `AssemblyAngularVelocity` change every frame it moves, so they leave
//! the reliable updates (`replicate`) once the part exists on a device.
//! Instead, each frame, [`MotionSender`] (one per player) picks the parts
//! that moved, most-moved and nearest the player's character first, and
//! sends as many as the player's byte budget allows, quantized: a position
//! to 1/128 stud, an angle to 360/65536 degree, a velocity to 1/32 stud a
//! second. A part that has stood still for [`SETTLE_TIME`] is then sent
//! once more, exactly and reliably, so a lost packet never leaves it
//! somewhere else.
//!
//! On the player's device, [`Smoother`] keeps each part's last few states
//! and shows them [`DELAY`] behind the newest, between two of them, so
//! parts move smoothly whatever arrives when.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use axcenity_core::{InstanceId, MOTION_PROPERTIES, Value, Vector3, World};

use crate::replicate::{Op, WireChange};
use crate::wire::WireValue;

/// The first byte of a motion packet on the unreliable channel.
pub const MOTION_TAG: u8 = b'M';
/// A player's device shows parts this many seconds behind the newest
/// state it has, so there's usually a state on each side to blend.
pub const DELAY: f64 = 0.1;
/// A part that hasn't moved for this many seconds is sent exactly, once,
/// on the reliable channel.
pub const SETTLE_TIME: f64 = 0.2;
/// Bytes a second each player may get of motion. A part is
/// [`ENTRY_BYTES`], so this is about 750 part moves a second.
pub const MOTION_BUDGET: f64 = 24_000.0;

const POSITION_STEP: f32 = 1.0 / 128.0;
const ANGLE_STEP: f32 = 360.0 / 65536.0;
const VELOCITY_STEP: f32 = 1.0 / 32.0;
const HEADER_BYTES: usize = 1 + 8 + 2;
/// One part's state in a packet.
pub const ENTRY_BYTES: usize = 8 + 12 + 6 + 6;
/// Packets stay below one UDP datagram, so a lost one loses few parts.
const MAX_PACKET_BYTES: usize = 1000;
/// The budget saved up at most, in seconds of it.
const BUDGET_BURST: f64 = 0.25;
/// Samples further apart than this are a part that stood still between
/// them, not one sent seldom (a far one, or one of many).
const MAX_GAP: f64 = 1.0;

/// A part's state, quantized.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PartState {
    pub position: [i32; 3],
    pub orientation: [u16; 3],
    pub velocity: [i16; 3],
}

impl PartState {
    pub fn of(world: &World, id: InstanceId) -> Option<PartState> {
        let instance = world.get(id)?;
        let vector = |name| {
            instance
                .property(name)
                .and_then(Value::as_vector3)
                .unwrap_or_default()
        };
        let p = vector("Position");
        let o = vector("Orientation");
        let v = vector("AssemblyLinearVelocity");
        let position = |x: f32| (x / POSITION_STEP).round() as i32;
        let angle = |x: f32| (x.rem_euclid(360.0) / ANGLE_STEP).round() as u32 as u16;
        let velocity = |x: f32| {
            (x / VELOCITY_STEP)
                .round()
                .clamp(f32::from(i16::MIN), f32::from(i16::MAX)) as i16
        };
        Some(PartState {
            position: [position(p.x), position(p.y), position(p.z)],
            orientation: [angle(o.x), angle(o.y), angle(o.z)],
            velocity: [velocity(v.x), velocity(v.y), velocity(v.z)],
        })
    }

    pub fn position(&self) -> Vector3 {
        let [x, y, z] = self.position.map(|n| n as f32 * POSITION_STEP);
        Vector3::new(x, y, z)
    }

    /// Degrees, from -180 to 180.
    pub fn orientation(&self) -> Vector3 {
        let [x, y, z] = self.orientation.map(|n| {
            let degrees = f32::from(n) * ANGLE_STEP;
            if degrees > 180.0 {
                degrees - 360.0
            } else {
                degrees
            }
        });
        Vector3::new(x, y, z)
    }

    pub fn velocity(&self) -> Vector3 {
        let [x, y, z] = self.velocity.map(|n| f32::from(n) * VELOCITY_STEP);
        Vector3::new(x, y, z)
    }

    /// How far apart two states are, as one number: studs, plus 1 for
    /// each 30 degrees turned and each 10 studs a second of velocity.
    fn distance(&self, other: &PartState) -> f64 {
        let moved = distance(self.position(), other.position());
        let mut turned = 0.0;
        for axis in 0..3 {
            let a = f64::from(self.orientation[axis]);
            let b = f64::from(other.orientation[axis]);
            let d = (a - b).abs();
            turned += d.min(65536.0 - d) * f64::from(ANGLE_STEP);
        }
        let sped = distance(self.velocity(), other.velocity());
        moved + turned / 30.0 + sped / 10.0
    }
}

pub(crate) fn distance(a: Vector3, b: Vector3) -> f64 {
    let (x, y, z) = (a.x - b.x, a.y - b.y, a.z - b.z);
    f64::from(x * x + y * y + z * z).sqrt()
}

/// States of parts at one moment of the server's clock.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Motion {
    /// The server's clock, in seconds.
    pub time: f64,
    pub parts: Vec<(u64, PartState)>,
}

impl Motion {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(HEADER_BYTES + self.parts.len() * ENTRY_BYTES);
        bytes.push(MOTION_TAG);
        bytes.extend(self.time.to_le_bytes());
        bytes.extend((self.parts.len() as u16).to_le_bytes());
        for (id, state) in &self.parts {
            bytes.extend(id.to_le_bytes());
            for n in state.position {
                bytes.extend(n.to_le_bytes());
            }
            for n in state.orientation {
                bytes.extend(n.to_le_bytes());
            }
            for n in state.velocity {
                bytes.extend(n.to_le_bytes());
            }
        }
        bytes
    }

    /// A motion packet, or `None` for anything else.
    pub fn from_bytes(bytes: &[u8]) -> Option<Motion> {
        let (&tag, rest) = bytes.split_first()?;
        if tag != MOTION_TAG || rest.len() < HEADER_BYTES - 1 {
            return None;
        }
        let time = f64::from_le_bytes(rest[..8].try_into().ok()?);
        let count = u16::from_le_bytes(rest[8..10].try_into().ok()?) as usize;
        let entries = &rest[10..];
        if entries.len() != count * ENTRY_BYTES {
            return None;
        }
        let parts = entries
            .as_chunks::<ENTRY_BYTES>()
            .0
            .iter()
            .map(|entry| {
                let u64_at = |at: usize| u64::from_le_bytes(entry[at..at + 8].try_into().unwrap());
                let i32_at = |at: usize| i32::from_le_bytes(entry[at..at + 4].try_into().unwrap());
                let u16_at = |at: usize| u16::from_le_bytes(entry[at..at + 2].try_into().unwrap());
                let i16_at = |at: usize| i16::from_le_bytes(entry[at..at + 2].try_into().unwrap());
                let state = PartState {
                    position: [i32_at(8), i32_at(12), i32_at(16)],
                    orientation: [u16_at(20), u16_at(22), u16_at(24)],
                    velocity: [i16_at(26), i16_at(28), i16_at(30)],
                };
                (u64_at(0), state)
            })
            .collect();
        Some(Motion { time, parts })
    }
}

/// A part, as one player was last sent it.
struct Tracked {
    /// What the player has.
    sent: PartState,
    /// What it was last frame.
    seen: PartState,
    /// Seconds since it last moved.
    still: f64,
    /// Whether the player has it exactly (from its creation, or settled).
    exact: bool,
    /// Grows each frame it's out of date, by how much and how near.
    priority: f64,
}

/// One player's motion: what they have of each part, and their budget.
pub struct MotionSender {
    parts: BTreeMap<InstanceId, Tracked>,
    /// Bytes they may still be sent.
    budget: f64,
    rate: f64,
}

impl MotionSender {
    /// For a player who has `world`'s parts as they are now (just sent a
    /// snapshot), with a budget of `rate` bytes a second.
    pub fn new(world: &World, parts: &[InstanceId], rate: f64) -> MotionSender {
        let mut sender = MotionSender {
            parts: BTreeMap::new(),
            budget: 0.0,
            rate,
        };
        for &id in parts {
            sender.track(world, id);
        }
        sender
    }

    fn track(&mut self, world: &World, id: InstanceId) {
        if let Some(state) = PartState::of(world, id) {
            self.parts.insert(
                id,
                Tracked {
                    sent: state,
                    seen: state,
                    still: 0.0,
                    exact: true,
                    priority: 0.0,
                },
            );
        }
    }

    /// This frame's motion for the player: `parts` are the parts they
    /// have (a new one was just sent whole), `focus` where their character
    /// is, `skip` parts they move themselves. Returns the packets for the
    /// unreliable channel, and the exact states of parts that settled for
    /// the reliable one.
    pub fn frame(
        &mut self,
        world: &World,
        parts: &[InstanceId],
        focus: Option<Vector3>,
        skip: &BTreeSet<InstanceId>,
        seconds: f64,
        time: f64,
    ) -> (Vec<Vec<u8>>, Vec<Op>) {
        let present: BTreeSet<InstanceId> = parts.iter().copied().collect();
        self.parts.retain(|id, _| present.contains(id));
        self.budget = (self.budget + self.rate * seconds).min(self.rate * BUDGET_BURST);
        let mut due = Vec::new();
        let mut settled = Vec::new();
        for &id in parts {
            let Some(tracked) = self.parts.get_mut(&id) else {
                self.track(world, id);
                continue;
            };
            let Some(now) = PartState::of(world, id) else {
                continue;
            };
            if skip.contains(&id) {
                tracked.sent = now;
                tracked.seen = now;
                tracked.exact = true;
                tracked.priority = 0.0;
                continue;
            }
            if now == tracked.seen {
                tracked.still += seconds;
            } else {
                tracked.still = 0.0;
                tracked.exact = false;
            }
            tracked.seen = now;
            if !tracked.exact && tracked.still >= SETTLE_TIME {
                tracked.exact = true;
                tracked.sent = now;
                tracked.priority = 0.0;
                settled.push(id);
                continue;
            }
            if now != tracked.sent {
                let nearness = focus.map_or(1.0, |focus| {
                    1.0 / (1.0 + distance(focus, now.position()) / 64.0)
                });
                tracked.priority += (now.distance(&tracked.sent) + 0.05) * nearness;
                due.push(id);
            }
        }
        due.sort_by(|a, b| {
            let priority = |id| self.parts[id].priority;
            priority(b).total_cmp(&priority(a))
        });
        let mut packets = Vec::new();
        let mut motion = Motion {
            time,
            parts: Vec::new(),
        };
        let per_packet = (MAX_PACKET_BYTES - HEADER_BYTES) / ENTRY_BYTES;
        for id in due {
            let cost = ENTRY_BYTES as f64
                + if motion.parts.is_empty() {
                    HEADER_BYTES as f64
                } else {
                    0.0
                };
            if self.budget < cost {
                break;
            }
            self.budget -= cost;
            let tracked = self.parts.get_mut(&id).expect("due parts are tracked");
            tracked.sent = tracked.seen;
            tracked.priority = 0.0;
            motion.parts.push((id.to_bits(), tracked.seen));
            if motion.parts.len() == per_packet {
                packets.push(motion.to_bytes());
                motion.parts.clear();
            }
        }
        if !motion.parts.is_empty() {
            packets.push(motion.to_bytes());
        }
        let ops = settled
            .into_iter()
            .filter_map(|id| exact_motion(world, id))
            .collect();
        (packets, ops)
    }
}

/// A reliable change with a part's exact motion properties.
pub fn exact_motion(world: &World, id: InstanceId) -> Option<Op> {
    let instance = world.get(id)?;
    let properties = MOTION_PROPERTIES
        .iter()
        .filter_map(|&name| {
            let value = instance.property(name)?;
            Some((name.to_owned(), WireValue::from_value(value)))
        })
        .collect();
    Some(Op::Change(WireChange {
        id: id.to_bits(),
        properties,
        ..WireChange::default()
    }))
}

#[derive(Clone, Copy, Debug)]
struct Sample {
    time: f64,
    position: Vector3,
    orientation: Vector3,
    velocity: Vector3,
}

impl Sample {
    fn of_world(world: &World, id: InstanceId, time: f64) -> Option<Sample> {
        let instance = world.get(id)?;
        let vector = |name| {
            instance
                .property(name)
                .and_then(Value::as_vector3)
                .unwrap_or_default()
        };
        Some(Sample {
            time,
            position: vector("Position"),
            orientation: vector("Orientation"),
            velocity: vector("AssemblyLinearVelocity"),
        })
    }
}

/// A player's device's side: parts shown [`DELAY`] behind the newest
/// state, blended between the two states around that moment.
#[derive(Default)]
pub struct Smoother {
    samples: BTreeMap<InstanceId, VecDeque<Sample>>,
    /// The newest state's time for each part: an older one is stale.
    newest: BTreeMap<InstanceId, f64>,
    /// The server's newest clock heard.
    latest: Option<f64>,
    /// The moment shown, on the server's clock.
    shown: Option<f64>,
    /// Parts this device moves itself (its own character).
    owned: BTreeSet<InstanceId>,
}

impl Smoother {
    /// Takes in a motion packet.
    pub fn receive(&mut self, world: &World, motion: &Motion) {
        self.latest = Some(self.latest.map_or(motion.time, |t| t.max(motion.time)));
        for &(id, state) in &motion.parts {
            let id = InstanceId::from_bits(id);
            if self.owned.contains(&id) || !world.contains(id) {
                continue;
            }
            if self.newest.get(&id).is_some_and(|&t| t >= motion.time) {
                continue;
            }
            self.newest.insert(id, motion.time);
            let samples = self.samples.entry(id).or_default();
            // After standing still, it starts from where it stood, one
            // frame before.
            let standing = match samples.back() {
                Some(last) if motion.time - last.time <= MAX_GAP => None,
                Some(&last) => Some(last),
                None => Sample::of_world(world, id, motion.time),
            };
            if let Some(last) = standing {
                samples.clear();
                samples.push_back(Sample {
                    time: motion.time - 1.0 / 60.0,
                    ..last
                });
            }
            samples.push_back(Sample {
                time: motion.time,
                position: state.position(),
                orientation: state.orientation(),
                velocity: state.velocity(),
            });
        }
    }

    /// A reliable update at `time` set the part `id` exactly: older states
    /// are stale.
    pub fn settle(&mut self, id: InstanceId, time: f64) {
        self.samples.remove(&id);
        let newest = self.newest.entry(id).or_insert(time);
        *newest = newest.max(time);
    }

    /// This device moves `parts` itself; the server's states of them are
    /// ignored.
    pub fn set_owned(&mut self, parts: BTreeSet<InstanceId>) {
        for id in &parts {
            self.samples.remove(id);
        }
        self.owned = parts;
    }

    /// Moves the shown moment on by `seconds` and shows each part there.
    pub fn step(&mut self, world: &mut World, seconds: f64) {
        let Some(latest) = self.latest else {
            return;
        };
        let target = latest - DELAY;
        let shown = match self.shown {
            Some(shown) if (shown + seconds - target).abs() < 0.3 => {
                // Drift toward the target, so the clocks stay together.
                let shown = shown + seconds;
                shown + (target - shown) * 0.1
            }
            _ => target,
        };
        self.shown = Some(shown);
        self.samples.retain(|&id, samples| {
            if !world.contains(id) {
                return false;
            }
            while samples.len() > 2 && samples[1].time <= shown {
                samples.pop_front();
            }
            let sample = match (samples.front(), samples.get(1)) {
                (Some(&a), Some(&b)) if shown > a.time => {
                    let t = ((shown - a.time) / (b.time - a.time).max(1e-6)).clamp(0.0, 1.0);
                    blend(a, b, t as f32)
                }
                (Some(&a), _) => a,
                _ => return false,
            };
            set_vector(world, id, "Position", sample.position);
            set_vector(world, id, "Orientation", sample.orientation);
            set_vector(world, id, "AssemblyLinearVelocity", sample.velocity);
            // A part that has arrived needs no more blending.
            samples.len() > 1 || shown < sample.time
        });
    }
}

fn set_vector(world: &mut World, id: InstanceId, name: &str, value: Vector3) {
    let same = world
        .get(id)
        .and_then(|i| i.property(name))
        .and_then(Value::as_vector3)
        == Some(value);
    if !same {
        let _ = world.set_property(id, name, Value::Vector3(value));
    }
}

fn blend(a: Sample, b: Sample, t: f32) -> Sample {
    let lerp = |a: Vector3, b: Vector3| {
        Vector3::new(
            a.x + (b.x - a.x) * t,
            a.y + (b.y - a.y) * t,
            a.z + (b.z - a.z) * t,
        )
    };
    // Each angle the short way round.
    let angle = |a: f32, b: f32| {
        let d = (b - a + 180.0).rem_euclid(360.0) - 180.0;
        let mut x = a + d * t;
        if x > 180.0 {
            x -= 360.0;
        } else if x < -180.0 {
            x += 360.0;
        }
        x
    };
    Sample {
        time: a.time + (b.time - a.time) * f64::from(t),
        position: lerp(a.position, b.position),
        orientation: Vector3::new(
            angle(a.orientation.x, b.orientation.x),
            angle(a.orientation.y, b.orientation.y),
            angle(a.orientation.z, b.orientation.z),
        ),
        velocity: lerp(a.velocity, b.velocity),
    }
}
