//! Host that plays every [`ClassName::Animation`](axcenity_core::ClassName::Animation)
//! in the Workspace: advances time, applies property clips, and reports events.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use axcenity_assets::Assets;
use axcenity_core::{ClassName, InstanceId, Value, World};

use crate::files::{AnimClipFile, PropertyClipFile};
use crate::property_clip::{PropertyClip, apply_evaluated};

/// Something scripts hear about from a property [`Animation`](ClassName::Animation).
#[derive(Clone, Debug, PartialEq)]
pub enum PropertyAnimationEvent {
    /// The clip finished a non-looping play.
    Completed { animation: InstanceId },
    /// A named marker on the clip timeline.
    Marker { animation: InstanceId, name: String },
}

impl PropertyAnimationEvent {
    pub fn script_event(&self) -> (InstanceId, &'static str, Vec<String>) {
        match self {
            PropertyAnimationEvent::Completed { animation } => (*animation, "Completed", vec![]),
            PropertyAnimationEvent::Marker { animation, name } => {
                (*animation, "AnimationEvent", vec![name.clone()])
            }
        }
    }
}

/// Every property Animation's loaded clip and last diagnostics.
#[derive(Default)]
pub struct PropertyPlayers {
    loaded: HashMap<InstanceId, Loaded>,
    errors: Vec<String>,
    reported: HashSet<String>,
}

struct Loaded {
    path: String,
    clip: Arc<PropertyClip>,
    /// Target paths already reported missing for this Animation.
    missing: HashMap<String, ()>,
}

impl PropertyPlayers {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn take_errors(&mut self) -> Vec<String> {
        std::mem::take(&mut self.errors)
    }

    pub fn clear(&mut self) {
        self.loaded.clear();
        self.reported.clear();
    }

    /// Advances every Animation under the Workspace by `dt` seconds of game
    /// time (0 evaluates the current time only). Returns script events.
    pub fn update(&mut self, world: &mut World, assets: &Assets, dt: f64) -> Vec<PropertyAnimationEvent> {
        let mut events = Vec::new();
        let animations: Vec<InstanceId> = world
            .descendants(world.workspace())
            .into_iter()
            .filter(|&id| {
                world
                    .get(id)
                    .is_some_and(|i| i.class() == ClassName::Animation)
            })
            .collect();
        let alive: HashSet<InstanceId> = animations.iter().copied().collect();
        self.loaded.retain(|id, _| alive.contains(id));

        for id in animations {
            if let Some(batch) = self.update_one(world, assets, id, dt) {
                events.extend(batch);
            }
        }
        events
    }

    fn update_one(
        &mut self,
        world: &mut World,
        assets: &Assets,
        id: InstanceId,
        dt: f64,
    ) -> Option<Vec<PropertyAnimationEvent>> {
        let root = world.get(id)?.parent()?;
        let (clip_path, enabled, speed, looping) = {
            let anim = world.get(id)?;
            let clip = anim
                .property("Clip")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let enabled = anim.property("Enabled").and_then(Value::as_bool) != Some(false);
            let speed = anim
                .property("Speed")
                .and_then(Value::as_number)
                .filter(|s| s.is_finite())
                .unwrap_or(1.0);
            // Instance Looped is the playback flag (editor copies the clip's
            // loop when assigning Clip).
            let looped = anim.property("Looped").and_then(Value::as_bool) == Some(true);
            (clip, enabled, speed, looped)
        };
        if clip_path.is_empty() {
            self.loaded.remove(&id);
            return None;
        }

        if self.loaded.get(&id).is_none_or(|loaded| loaded.path != clip_path) {
            match load_property_clip(assets, &clip_path) {
                Ok(clip) => {
                    self.loaded.insert(
                        id,
                        Loaded {
                            path: clip_path.clone(),
                            clip,
                            missing: HashMap::new(),
                        },
                    );
                }
                Err(message) => {
                    let name = world.full_name(id).unwrap_or_default();
                    self.report(format!(
                        "The Animation `{name}` could not load `{clip_path}`: {message}"
                    ));
                    self.loaded.remove(&id);
                    return None;
                }
            }
        }
        let Some(loaded) = self.loaded.get_mut(&id) else {
            return None;
        };
        let clip = loaded.clip.clone();

        let mut out = Vec::new();
        let Some(state) = world.property_animation_mut(id) else {
            return None;
        };

        if let Some(time) = state.seek.take() {
            state.time = time.max(0.0);
        }
        if state.stop {
            state.stop = false;
            state.playing = false;
            state.paused = false;
            state.time = 0.0;
        }
        if state.pause {
            state.pause = false;
            if state.playing {
                state.playing = false;
                state.paused = true;
            }
        }
        if state.resume {
            state.resume = false;
            if state.paused {
                state.playing = true;
                state.paused = false;
            }
        }
        if state.play {
            state.play = false;
            state.playing = true;
            state.paused = false;
            state.time = 0.0;
        }

        let from = state.time;
        let mut completed = false;
        if enabled && state.playing {
            let delta = dt * speed;
            let mut next = from + delta;
            if looping {
                // Keep time growing for event windows; sample_time wraps.
                if next.is_finite() {
                    state.time = next;
                }
                let markers = clip.events_between(from, state.time);
                out.extend(markers.into_iter().map(|name| PropertyAnimationEvent::Marker {
                    animation: id,
                    name,
                }));
            } else {
                let duration = clip.duration();
                if next >= duration {
                    next = duration;
                    state.playing = false;
                    completed = duration > 0.0 || from < duration;
                }
                if next.is_finite() {
                    state.time = next;
                }
                let markers = clip.events_between(from, state.time);
                out.extend(markers.into_iter().map(|name| PropertyAnimationEvent::Marker {
                    animation: id,
                    name,
                }));
            }
        }

        let time = state.time;
        let evaluated = clip.evaluate(time);
        // Re-borrow loaded for missing map after clip Arc clone.
        let Some(loaded) = self.loaded.get_mut(&id) else {
            return Some(out);
        };
        let before = loaded.missing.len();
        apply_evaluated(world, root, &evaluated, &mut loaded.missing);
        if loaded.missing.len() > before {
            let name = world.full_name(id).unwrap_or_default();
            for target in loaded.missing.keys() {
                let message = format!(
                    "The Animation `{name}` has no target `{target}` under its parent, so that track is skipped."
                );
                report(&mut self.errors, &mut self.reported, message);
            }
        }

        if completed {
            out.push(PropertyAnimationEvent::Completed { animation: id });
        }
        Some(out)
    }

    fn report(&mut self, message: String) {
        report(&mut self.errors, &mut self.reported, message);
    }
}

fn report(errors: &mut Vec<String>, reported: &mut HashSet<String>, message: String) {
    if reported.insert(message.clone()) {
        errors.push(message);
    }
}

/// Reads a property clip from the project. Take clips are refused.
pub fn load_property_clip(assets: &Assets, path: &str) -> Result<Arc<PropertyClip>, String> {
    let bytes = assets.read_file(path)?;
    let text = String::from_utf8(bytes).map_err(|_| "it isn't text".to_owned())?;
    match AnimClipFile::from_json(&text).map_err(|err| err.0)? {
        AnimClipFile::Property(file) => Ok(Arc::new(
            PropertyClip::from_file(file).map_err(|err| err.0)?,
        )),
        AnimClipFile::Take(_) => Err(
            "it is a take clip for a skeletal Animator, not a property clip (`\"kind\": \"property\"`)"
                .into(),
        ),
    }
}

/// Parses property-clip JSON without Assets (tests and editor).
pub fn parse_property_clip(text: &str) -> Result<PropertyClip, String> {
    let file = PropertyClipFile::from_json(text).map_err(|err| err.0)?;
    PropertyClip::from_file(file).map_err(|err| err.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::{Interp, PropertyKeyframe, PropertyName, PropertyTrack};
    use axcenity_core::Vector3;
    use std::path::PathBuf;

    fn write_door_project() -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "axcenity-prop-anim-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let anim = root.join("assets/animations");
        std::fs::create_dir_all(&anim).unwrap();
        let clip = PropertyClipFile {
            format_version: 1,
            kind: PropertyClipFile::KIND.to_owned(),
            duration: 1.0,
            looping: false,
            tracks: vec![PropertyTrack {
                id: "pos".into(),
                target: ".".into(),
                property: PropertyName::Position,
                keys: vec![
                    PropertyKeyframe {
                        id: "a".into(),
                        time: 0.0,
                        value: [0.0, 0.0, 0.0],
                        interp: Interp::Linear,
                    },
                    PropertyKeyframe {
                        id: "b".into(),
                        time: 1.0,
                        value: [0.0, 10.0, 0.0],
                        interp: Interp::Linear,
                    },
                ],
            }],
            events: vec![crate::files::ClipEvent {
                time: 0.5,
                name: "Halfway".into(),
            }],
        };
        std::fs::write(anim.join("Door.anim.json"), clip.to_json()).unwrap();
        root
    }

    #[test]
    fn plays_on_a_part_with_events_and_completion() {
        let root = write_door_project();
        let assets = Assets::new(&root);
        let mut world = World::new();
        let part = world.create(ClassName::Part).unwrap();
        world.set_parent(part, Some(world.workspace())).unwrap();
        let anim = world.create(ClassName::Animation).unwrap();
        world.set_parent(anim, Some(part)).unwrap();
        world
            .set_property(
                anim,
                "Clip",
                Value::String("assets/animations/Door.anim.json".into()),
            )
            .unwrap();

        let mut players = PropertyPlayers::new();
        {
            let state = world.property_animation_mut(anim).unwrap();
            state.play = true;
        }
        let events = players.update(&mut world, &assets, 0.0);
        assert!(events.is_empty());
        assert_eq!(
            world.get(part).unwrap().property("Position"),
            Some(&Value::Vector3(Vector3::new(0.0, 0.0, 0.0)))
        );

        let events = players.update(&mut world, &assets, 0.51);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, PropertyAnimationEvent::Marker { name, .. } if name == "Halfway")),
            "{events:?}"
        );
        let y = world
            .get(part)
            .unwrap()
            .property("Position")
            .and_then(Value::as_vector3)
            .unwrap()
            .y;
        assert!((y - 5.1).abs() < 1e-3, "{y}");

        let events = players.update(&mut world, &assets, 0.6);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, PropertyAnimationEvent::Completed { .. }))
        );
        assert_eq!(
            world.get(part).unwrap().property("Position"),
            Some(&Value::Vector3(Vector3::new(0.0, 10.0, 0.0)))
        );
        assert!(!world.property_animation(anim).unwrap().playing);
    }

    #[test]
    fn independent_players() {
        let root = write_door_project();
        let assets = Assets::new(&root);
        let mut world = World::new();
        let a = world.create(ClassName::Part).unwrap();
        let b = world.create(ClassName::Part).unwrap();
        world.set_parent(a, Some(world.workspace())).unwrap();
        world.set_parent(b, Some(world.workspace())).unwrap();
        for part in [a, b] {
            let anim = world.create(ClassName::Animation).unwrap();
            world.set_parent(anim, Some(part)).unwrap();
            world
                .set_property(
                    anim,
                    "Clip",
                    Value::String("assets/animations/Door.anim.json".into()),
                )
                .unwrap();
            world.property_animation_mut(anim).unwrap().play = true;
        }
        let mut players = PropertyPlayers::new();
        players.update(&mut world, &assets, 0.25);
        // Only advance the first Animation further.
        let anim_a = world.get(a).unwrap().children()[0];
        let anim_b = world.get(b).unwrap().children()[0];
        world.property_animation_mut(anim_b).unwrap().playing = false;
        players.update(&mut world, &assets, 0.25);
        let ya = world
            .get(a)
            .unwrap()
            .property("Position")
            .and_then(Value::as_vector3)
            .unwrap()
            .y;
        let yb = world
            .get(b)
            .unwrap()
            .property("Position")
            .and_then(Value::as_vector3)
            .unwrap()
            .y;
        assert!((ya - 5.0).abs() < 1e-3, "{ya}");
        assert!((yb - 2.5).abs() < 1e-3, "{yb}");
        assert_ne!(
            world.property_animation(anim_a).unwrap().time,
            world.property_animation(anim_b).unwrap().time
        );
    }
}
