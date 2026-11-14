//! Faces' live state on the wire (facial animation F7b, decision D8 in
//! `docs/research/facial-animation.md`): what scripts set on a FaceControls
//! through its methods (expressions, overrides, layer settings, the lip
//! sync playing) isn't Instance properties, so it goes beside the
//! Instances like Animators' does.
//!
//! Faces are presentation: a device gets where each thing is heading (an
//! expression's intensity, a lip sync's weight) and blends there itself,
//! never every control every frame. Each FaceControls says which parts go
//! (`ReplicateExpressions`, `ReplicateOverrides`, `ReplicateLipSync`) and
//! how often at most (`ReplicationRate`). A lip sync's place goes when it
//! starts and once a second while it plays; a device that has drifted
//! further than [`LIP_SYNC_DRIFT`] from it jumps there. Gaze settings are
//! properties, so they replicate as any property does.

use std::collections::BTreeMap;

use axcenity_core::face::{
    ExpressionPlay, FaceBlend, FaceLayer, FaceLayerSetting, FaceState, LipSyncPlay,
};
use axcenity_core::{ClassName, InstanceId, Value, World};
use serde::{Deserialize, Serialize};

/// Seconds a device's lip sync may be from the server's before it jumps.
pub const LIP_SYNC_DRIFT: f64 = 0.1;

/// What changed of a FaceControls' live state. A part left out (`None`)
/// didn't change, or isn't replicated; one sent is the whole of that part.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WireFace {
    pub id: u64,
    pub expressions: Option<Vec<WireExpression>>,
    /// Gameplay overrides: control index (in `FACE_CONTROLS`) and value.
    pub overrides: Option<Vec<(u8, f32)>>,
    /// Layer settings changed from the defaults.
    pub layers: Option<Vec<WireFaceLayer>>,
    /// The lip sync playing; `Some(None)`: none is.
    pub lip_sync: Option<Option<WireLipSync>>,
}

/// An expression set on a face, as where it's heading.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WireExpression {
    pub name: String,
    pub intensity: f32,
    /// 1 while it's set, 0 once it's cleared (fading out).
    pub presence: f32,
    pub transition: Option<f32>,
    pub weight: f32,
    pub priority: i32,
    pub asymmetry: f32,
    pub order: u64,
}

/// A layer's settings: the layer's index in `FaceLayer::ALL`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WireFaceLayer {
    pub layer: u8,
    pub weight: f32,
    pub additive: bool,
    pub mask: Option<Vec<u8>>,
}

/// The lip sync playing on a face.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WireLipSync {
    pub path: String,
    /// Seconds into it when this was sent.
    pub time: f64,
    pub speed: f32,
    /// The Sound it keeps time with (left out when players don't get it).
    pub sound: Option<u64>,
    /// The weight it's heading to, and the seconds that takes.
    pub weight: f32,
    pub fade: f32,
}

impl WireFace {
    pub fn is_empty(&self) -> bool {
        self.expressions.is_none()
            && self.overrides.is_none()
            && self.layers.is_none()
            && self.lip_sync.is_none()
    }
}

/// Which parts of a face go, and how often.
struct Choice {
    expressions: bool,
    overrides: bool,
    lip_sync: bool,
    /// Seconds between sends at least (0: every frame).
    every: f64,
}

impl Choice {
    fn of(world: &World, id: InstanceId) -> Option<Self> {
        let face = world.get(id)?;
        let flag = |name| face.property(name).and_then(Value::as_bool) != Some(false);
        let rate = face
            .property("ReplicationRate")
            .and_then(Value::as_number)
            .unwrap_or(10.0);
        Some(Self {
            expressions: flag("ReplicateExpressions"),
            overrides: flag("ReplicateOverrides"),
            lip_sync: flag("ReplicateLipSync"),
            every: if rate > 0.0 { 1.0 / rate } else { 0.0 },
        })
    }
}

/// A face's state as it was last sent.
#[derive(Clone, Default, PartialEq)]
pub(crate) struct SentFace {
    expressions: Option<Vec<WireExpression>>,
    overrides: Option<Vec<(u8, f32)>>,
    layers: Option<Vec<WireFaceLayer>>,
    /// The lip sync without its time, and the whole second it was at.
    lip_sync: Option<(Option<WireLipSync>, i64)>,
    /// The server's clock when this face may next be sent.
    due: f64,
}

impl SentFace {
    /// The FaceControls `id`'s state, the parts it replicates. `None`
    /// when no script has set anything on it.
    fn of(world: &World, id: InstanceId, choice: &Choice) -> Option<Self> {
        let state = world.face(id)?;
        Some(Self {
            expressions: choice.expressions.then(|| expressions(state)),
            overrides: choice.overrides.then(|| overrides(state)),
            layers: choice.overrides.then(|| layers(state)),
            lip_sync: choice.lip_sync.then(|| {
                let play = state.lip_sync.as_ref().map(|play| lip_sync(world, play));
                let second = state
                    .lip_sync
                    .as_ref()
                    .map_or(-1, |play| play.time.floor() as i64);
                (play, second)
            }),
            due: 0.0,
        })
    }

    /// What of it to send to a device that had `before` (everything, for
    /// none).
    fn changes(&self, id: InstanceId, world: &World, before: Option<&Self>) -> WireFace {
        fn changed<T: PartialEq + Clone>(now: &Option<T>, before: Option<&Option<T>>) -> Option<T> {
            if before == Some(now) {
                None
            } else {
                now.clone()
            }
        }
        let lip_sync = match (&self.lip_sync, before.map(|b| &b.lip_sync)) {
            (Some(now), Some(Some(then))) if now == then => None,
            (Some((play, _)), _) => Some(play.clone().map(|mut play| {
                // The time as it is now, not when it was compared.
                play.time = world
                    .face(id)
                    .and_then(|state| state.lip_sync.as_ref())
                    .map_or(play.time, |live| live.time);
                play
            })),
            (None, _) => None,
        };
        WireFace {
            id: id.to_bits(),
            expressions: changed(&self.expressions, before.map(|b| &b.expressions)),
            overrides: changed(&self.overrides, before.map(|b| &b.overrides)),
            layers: changed(&self.layers, before.map(|b| &b.layers)),
            lip_sync,
        }
    }
}

fn expressions(state: &FaceState) -> Vec<WireExpression> {
    state
        .expressions
        .iter()
        // Faded out on the server: it's gone from the list, so a device
        // fades it out too.
        .filter(|play| play.target_presence > 0.0 || play.presence > 0.0)
        .map(|play| WireExpression {
            name: play.name.clone(),
            intensity: play.target_intensity as f32,
            presence: play.target_presence as f32,
            transition: play.transition.map(|t| t as f32),
            weight: play.weight as f32,
            priority: play.priority,
            asymmetry: play.asymmetry as f32,
            order: play.order,
        })
        .collect()
}

fn overrides(state: &FaceState) -> Vec<(u8, f32)> {
    state
        .overrides
        .iter()
        .map(|(&index, &value)| (index as u8, value as f32))
        .collect()
}

fn layers(state: &FaceState) -> Vec<WireFaceLayer> {
    state
        .layers
        .iter()
        .map(|(layer, setting)| WireFaceLayer {
            layer: FaceLayer::ALL
                .iter()
                .position(|l| l == layer)
                .unwrap_or_default() as u8,
            weight: setting.weight as f32,
            additive: setting.blend == FaceBlend::Additive,
            mask: setting
                .mask
                .as_ref()
                .map(|mask| mask.iter().map(|&i| i as u8).collect()),
        })
        .collect()
}

/// A lip sync as it's compared: its time left at 0 (it's sent by the
/// second instead).
fn lip_sync(world: &World, play: &LipSyncPlay) -> WireLipSync {
    WireLipSync {
        path: play.path.clone(),
        time: 0.0,
        speed: play.speed as f32,
        sound: play
            .sound
            .filter(|&sound| world.contains(sound) && !world.is_server_only(sound))
            .map(InstanceId::to_bits),
        weight: play.target_weight as f32,
        fade: play.fade as f32,
    }
}

/// The server's side: each face as it was last sent.
#[derive(Default)]
pub(crate) struct SentFaces {
    faces: BTreeMap<InstanceId, SentFace>,
}

/// The FaceControls among `inside`.
pub(crate) fn face_ids(world: &World, inside: &[InstanceId]) -> Vec<InstanceId> {
    inside
        .iter()
        .copied()
        .filter(|&id| {
            world
                .get(id)
                .is_some_and(|i| i.class() == ClassName::FaceControls)
        })
        .collect()
}

/// Everything about the faces in `ids` that have live state, for a player
/// who just joined (the lip sync's place exactly).
pub(crate) fn snapshot(world: &World, ids: &[InstanceId]) -> Vec<WireFace> {
    ids.iter()
        .filter_map(|&id| {
            let choice = Choice::of(world, id)?;
            let now = SentFace::of(world, id, &choice)?;
            Some(now.changes(id, world, None))
        })
        .filter(|wire| !wire.is_empty())
        .collect()
}

impl SentFaces {
    /// What changed of the faces in `ids` at the server's clock `clock`
    /// (`None`: send every change now), each face no more often than its
    /// ReplicationRate.
    pub(crate) fn frame(
        &mut self,
        world: &World,
        ids: &[InstanceId],
        clock: Option<f64>,
    ) -> Vec<WireFace> {
        let mut wires = Vec::new();
        let mut kept = BTreeMap::new();
        for &id in ids {
            let Some(choice) = Choice::of(world, id) else {
                continue;
            };
            let before = self.faces.remove(&id);
            let Some(mut now) = SentFace::of(world, id, &choice) else {
                continue;
            };
            let waiting = match (clock, &before) {
                (Some(clock), Some(before)) => clock < before.due,
                _ => false,
            };
            if waiting {
                kept.extend(before.map(|before| (id, before)));
                continue;
            }
            let wire = now.changes(id, world, before.as_ref());
            if wire.is_empty() {
                now.due = before.map_or(0.0, |before| before.due);
            } else {
                now.due = clock.unwrap_or(0.0) + choice.every;
                wires.push(wire);
            }
            kept.insert(id, now);
        }
        self.faces = kept;
        wires
    }
}

/// Applies a face's changes on a device: expressions head where the
/// server's are (those it no longer has fade out), overrides and layers
/// are the server's, and the lip sync starts, stops or keeps time with it.
pub(crate) fn apply(world: &mut World, wire: &WireFace) {
    let id = InstanceId::from_bits(wire.id);
    let sound_ok = |world: &World, sound: Option<u64>| {
        sound
            .map(InstanceId::from_bits)
            .filter(|&sound| world.contains(sound))
    };
    let sound = wire
        .lip_sync
        .as_ref()
        .and_then(|play| play.as_ref())
        .and_then(|play| sound_ok(world, play.sound));
    let Some(state) = world.face_mut(id) else {
        return;
    };
    if let Some(expressions) = &wire.expressions {
        apply_expressions(state, expressions);
    }
    if let Some(overrides) = &wire.overrides {
        state.overrides = overrides
            .iter()
            .map(|&(index, value)| (usize::from(index), f64::from(value)))
            .collect();
    }
    if let Some(layers) = &wire.layers {
        state.layers = layers
            .iter()
            .filter_map(|layer| {
                let which = *FaceLayer::ALL.get(usize::from(layer.layer))?;
                Some((
                    which,
                    FaceLayerSetting {
                        weight: f64::from(layer.weight),
                        blend: if layer.additive {
                            FaceBlend::Additive
                        } else {
                            FaceBlend::Override
                        },
                        mask: layer
                            .mask
                            .as_ref()
                            .map(|mask| mask.iter().map(|&i| usize::from(i)).collect()),
                    },
                ))
            })
            .collect();
    }
    match &wire.lip_sync {
        None => {}
        Some(None) => {
            if let Some(play) = &mut state.lip_sync {
                play.target_weight = 0.0;
            }
        }
        Some(Some(wire)) => match &mut state.lip_sync {
            Some(play) if play.path == wire.path => {
                if (play.time - wire.time).abs() > LIP_SYNC_DRIFT {
                    play.time = wire.time;
                }
                play.speed = f64::from(wire.speed);
                play.sound = sound;
                play.target_weight = f64::from(wire.weight);
                play.fade = f64::from(wire.fade);
            }
            _ => {
                let from = state.lip_sync.as_ref().map_or(0.0, |play| play.weight);
                state.lip_sync = Some(LipSyncPlay {
                    path: wire.path.clone(),
                    time: wire.time,
                    speed: f64::from(wire.speed),
                    sound,
                    heard: false,
                    weight: from,
                    target_weight: f64::from(wire.weight),
                    fade: f64::from(wire.fade),
                });
            }
        },
    }
}

fn apply_expressions(state: &mut FaceState, wires: &[WireExpression]) {
    for wire in wires {
        let transition = wire.transition.map(f64::from);
        match state
            .expressions
            .iter_mut()
            .find(|play| play.name.eq_ignore_ascii_case(&wire.name))
        {
            Some(play) => {
                play.target_intensity = f64::from(wire.intensity);
                play.target_presence = f64::from(wire.presence);
                play.transition = transition;
                play.weight = f64::from(wire.weight);
                play.priority = wire.priority;
                play.asymmetry = f64::from(wire.asymmetry);
                play.order = wire.order;
            }
            None if wire.presence > 0.0 => state.expressions.push(ExpressionPlay {
                name: wire.name.clone(),
                intensity: f64::from(wire.intensity),
                target_intensity: f64::from(wire.intensity),
                presence: 0.0,
                target_presence: f64::from(wire.presence),
                transition,
                weight: f64::from(wire.weight),
                priority: wire.priority,
                asymmetry: f64::from(wire.asymmetry),
                order: wire.order,
            }),
            None => {}
        }
    }
    for play in &mut state.expressions {
        if !wires
            .iter()
            .any(|wire| wire.name.eq_ignore_ascii_case(&play.name))
        {
            play.target_presence = 0.0;
        }
    }
    state.expressions.sort_by_key(|play| play.order);
    state.next_order = state
        .expressions
        .iter()
        .map(|play| play.order + 1)
        .max()
        .unwrap_or(0)
        .max(state.next_order);
}
