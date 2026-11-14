//! What a face reads from the World each frame (facial animation F4): its
//! FaceControls' properties, the expressions and overrides scripts set
//! (`World::face_mut`), and where its gaze target is. The layers it makes
//! are composed into controls later, possibly off the main thread
//! ([`crate::face_play::FaceLayers::compose`]).

use std::collections::HashMap;

use axcenity_assets::{Assets, Model};
use axcenity_core::face::{FACE_CONTROLS, FaceLayer, FaceState};
use axcenity_core::{ClassName, InstanceId, Value, World};
use glam::{Mat4, Vec3};

use crate::face_play::{
    BlinkSettings, FaceDetail, FaceLayers, FaceMotion, LayerValues, Procedural,
    advance_expressions, approach,
};
use crate::lip_sync::{CompiledVisemes, LipTiming, find_viseme_map, viseme_weights};
use crate::ragdoll::{placement_with, to_vec3};
use crate::retarget::{HumanBone, HumanoidMap};

/// What a face looks at things with: the viewer's position, and the
/// resting head of each skinned model looked at (by model key).
pub(crate) struct Lookers<'a> {
    pub viewer: Option<Vec3>,
    pub heads: &'a mut HashMap<u64, Option<Vec3>>,
}

/// The layers of the FaceControls `face` in `owner` (whose model is
/// `model`) this frame, with its expressions and own motion moved on by
/// `dt` seconds, working out only what `detail` says. What's wrong goes
/// in `problems`, one sentence each.
#[allow(clippy::too_many_arguments)]
pub(crate) fn face_layers(
    world: &mut World,
    assets: &Assets,
    face: InstanceId,
    owner: InstanceId,
    model: &Model,
    motion: &mut FaceMotion,
    lookers: Lookers,
    detail: FaceDetail,
    dt: f64,
    problems: &mut Vec<String>,
) -> Option<FaceLayers> {
    let instance = world.get(face)?;
    let name = world.full_name(face).unwrap_or_default();
    let number = |property: &str, default: f64| {
        instance
            .property(property)
            .and_then(Value::as_number)
            .unwrap_or(default)
    };
    let text = |property: &str| {
        instance
            .property(property)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
    };
    let base: Vec<f32> = FACE_CONTROLS
        .iter()
        .map(|control| number(control.name, 0.0) as f32)
        .collect();
    let order = FaceLayer::order(
        instance
            .property("LayerOrder")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    )
    .unwrap_or_else(|why| {
        problems.push(format!("The FaceControls `{name}`'s LayerOrder: {why}."));
        FaceLayer::ALL.to_vec()
    });
    let blink = BlinkSettings {
        enabled: detail.procedural
            && instance.property("BlinkEnabled").and_then(Value::as_bool) != Some(false),
        eyes: match text("BlinkEyes").as_str() {
            "left" => [true, false],
            "right" => [false, true],
            _ => [true, true],
        },
        interval: (
            number("BlinkIntervalMin", 2.0),
            number("BlinkIntervalMax", 6.0),
        ),
        duration: number("BlinkDuration", 0.15),
        double_chance: number("DoubleBlinkChance", 0.15),
    };
    let idle = if detail.idle {
        number("IdleMotion", 0.5) as f32
    } else {
        0.0
    };
    let timing = LipTiming {
        smoothing: number("LipSyncSmoothing", 0.06),
        anticipation: number("LipSyncAnticipation", 0.04),
    };
    let map_name = instance
        .property("VisemeMap")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let smoothing = number("GazeSmoothing", 0.12);
    let weight = number("GazeWeight", 1.0) as f32;
    let gaze_mode = if detail.procedural {
        text("GazeMode")
    } else {
        String::new()
    };
    let gaze_at = match gaze_mode.as_str() {
        "camera" => lookers.viewer,
        "target" => instance
            .property("GazeTarget")
            .and_then(Value::as_instance)
            .and_then(|target| look_point(world, assets, lookers.heads, target)),
        "position" => instance
            .property("GazePosition")
            .and_then(Value::as_vector3)
            .map(to_vec3),
        "animation" | "" => None,
        other => {
            problems.push(format!(
                "The FaceControls `{name}` has GazeMode `{other}`; it can be Animation, Camera, Target or Position."
            ));
            None
        }
    };
    // Into the model file's space, where the face's bones are.
    let fit = model.rig.as_ref().map_or(Mat4::IDENTITY, |rig| rig.fit);
    let gaze_at = gaze_at
        .zip(placement_with(world, owner, fit))
        .map(|(point, place)| place.inverse().transform_point3(point))
        .filter(|point| point.is_finite());

    // Whether the Sound a lip sync keeps time with plays, and how fast:
    // `None` without one, `Some(None)` once it's gone.
    let sound = world
        .face(face)
        .and_then(|state| state.lip_sync.as_ref())
        .and_then(|play| play.sound)
        .map(|sound| {
            world.get(sound).map(|sound| {
                (
                    sound.property("Playing").and_then(Value::as_bool) == Some(true),
                    sound
                        .property("PlaybackSpeed")
                        .and_then(Value::as_number)
                        .unwrap_or(1.0),
                )
            })
        });

    let expressions = assets.face_expressions();
    let state = world.face_mut(face)?;
    let mut dialogue = advance_lip_sync(
        state,
        assets,
        sound,
        (&map_name, timing),
        dt,
        &name,
        problems,
    );
    if !detail.dialogue {
        // Kept in time, but not shown.
        dialogue = LayerValues::empty();
    }
    let mut unknown = Vec::new();
    let expression = advance_expressions(state, &expressions, dt, &mut unknown);
    for missing in unknown {
        problems.push(format!(
            "The FaceControls `{name}` was given the expression `{missing}`, which isn't built in or a `*.expression.json` file in assets/faces/expressions, so it's skipped."
        ));
    }
    let settings = FaceLayer::ALL.map(|layer| state.layer(layer)).to_vec();
    let overrides = state
        .overrides
        .iter()
        .map(|(&control, &value)| (control, value as f32))
        .collect();

    let blink = motion.blink(&blink, dt);
    let (saccade, brow) = motion.idle(idle, dt);
    let gaze = motion.gaze(gaze_at, weight, smoothing, dt);
    Some(FaceLayers {
        base,
        expression,
        dialogue,
        procedural: Procedural {
            blink,
            gaze,
            saccade,
            brow,
        },
        overrides,
        order,
        settings,
    })
}

/// Moves the lip sync `state` plays on by `dt` (while its Sound plays, if
/// it has one) and returns the Dialogue layer, dropping it once it has
/// faded out. `sound` is whether its Sound plays and how fast (`None`
/// without one, `Some(None)` once the Sound is gone).
fn advance_lip_sync(
    state: &mut FaceState,
    assets: &Assets,
    sound: Option<Option<(bool, f64)>>,
    (map_name, timing): (&str, LipTiming),
    dt: f64,
    name: &str,
    problems: &mut Vec<String>,
) -> LayerValues {
    let Some(play) = state.lip_sync.as_mut() else {
        return LayerValues::empty();
    };
    let clip = match assets.lip_sync(&play.path) {
        Ok(clip) => clip,
        Err(why) => {
            problems.push(format!(
                "The FaceControls `{name}` can't play the lip sync `{}`: {why}.",
                play.path
            ));
            state.lip_sync = None;
            return LayerValues::empty();
        }
    };
    let rate = match sound {
        None => Some(1.0),
        Some(None) => None,
        Some(Some((true, speed))) => {
            play.heard = true;
            Some(speed)
        }
        Some(Some((false, _))) => None,
    };
    let sound_ended = matches!(sound, Some(None)) || (play.heard && rate.is_none());
    if let Some(rate) = rate {
        play.time += dt * play.speed * rate;
    }
    let over = play.time > clip.duration + timing.smoothing.max(0.0) + timing.anticipation.abs();
    if sound_ended || over {
        play.target_weight = 0.0;
    }
    let step = if play.fade > 0.0 { dt / play.fade } else { 1.0 };
    play.weight = approach(play.weight, play.target_weight, step);
    if play.weight <= 0.0 && play.target_weight <= 0.0 {
        state.lip_sync = None;
        return LayerValues::empty();
    }
    let maps = assets.viseme_maps();
    let map = find_viseme_map(&maps, map_name).unwrap_or_else(|| {
        problems.push(format!(
            "The FaceControls `{name}` has VisemeMap `{map_name}`, which isn't a `*.visemes.json` file in assets/faces/visemes, so the built-in one is used."
        ));
        crate::lip_sync::default_viseme_map()
    });
    let (compiled, map_problems) = CompiledVisemes::new(map);
    for problem in map_problems {
        problems.push(format!("The viseme map `{map_name}`: {problem}."));
    }
    let weights = viseme_weights(&clip, play.time, timing);
    compiled.layer(&weights, play.weight as f32)
}

/// Where in the world to look at `target`: a part's or Attachment's
/// position, a skinned MeshPart's resting head, or a Model's Head (else
/// its PrimaryPart, else its first part).
fn look_point(
    world: &World,
    assets: &Assets,
    heads: &mut HashMap<u64, Option<Vec3>>,
    target: InstanceId,
) -> Option<Vec3> {
    let instance = world.get(target)?;
    match instance.class() {
        ClassName::Model => {
            let head = world
                .find_first_child(target, "Head")
                .filter(|&head| {
                    world
                        .get(head)
                        .is_some_and(|head| head.class().is_base_part())
                })
                .or_else(|| {
                    instance
                        .property("PrimaryPart")
                        .and_then(Value::as_instance)
                })
                .or_else(|| {
                    world.descendants_iter(target).find(|&part| {
                        world
                            .get(part)
                            .is_some_and(|part| part.class().is_base_part())
                    })
                })?;
            look_point(world, assets, heads, head)
        }
        ClassName::MeshPart => {
            let skinned = crate::animators::mesh_model(world, assets, target)
                .filter(|model| model.rig.is_some());
            if let Some(model) = skinned {
                let rig = model.rig.as_ref().expect("filtered");
                let head = *heads.entry(model.key()).or_insert_with(|| {
                    let head = HumanoidMap::detect(&rig.bones).get(HumanBone::Head)?;
                    Some(rig.world(&rig.rest_pose())[head].w_axis.truncate())
                });
                if let Some(head) = head {
                    return placement_with(world, target, rig.fit)
                        .map(|place| place.transform_point3(head));
                }
            }
            crate::ik_control::frame_of(world, target).map(|frame| frame.position)
        }
        _ => crate::ik_control::frame_of(world, target).map(|frame| frame.position),
    }
}
