//! Sequence Instances (Phase 51): cutscenes from `*.sequence.json` files.
//!
//! - `sequence:Play()` starts it (from the start once it has ended or
//!   stopped; a paused one carries on), `Pause()` holds it where it is and
//!   `Stop()` lets go of it and goes back to the start. `TimePosition` is
//!   where it is (setting it jumps there), `IsPlaying` and `Length` read.
//! - `EventReached(name)` fires as it passes each event, and `Completed`
//!   once when it reaches its end.
//! - `AutoPlay` plays it when the game starts. With `Skippable`, the Menu
//!   action jumps a playing sequence to its end: everything it keys is left
//!   at its last values and `Completed` fires once (the events skipped
//!   don't). With `LockPlayerInput`, the character gets no movement and
//!   scripts and prompts no presses from the player while it plays or is
//!   paused.
//!
//! Both sides share the playbacks; [`ScriptRuntime::step`] moves them on
//! after the scripts, so the Animators sample this frame's time. What the
//! sequence puts in the world is `anim::sequence::SequenceRun`'s, the same
//! as the editor's preview; here are the camera (the client's
//! `workspace.CurrentCamera`, held Scriptable while a cut is in charge),
//! the Sounds and the levels.
//!
//! [`ScriptRuntime::step`]: crate::ScriptRuntime::step

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use axcenity_anim::sequence::{SequenceRun, SoundCue, load_sequence};
use axcenity_assets::Assets;
use axcenity_core::{ClassName, InstanceId, Value, World};
use mlua::{Function, IntoLua, Lua};

use crate::camera::{Camera, CameraState, CameraType};
use crate::instance::{LuaInstance, destroyed, world};

/// Where a played Sequence is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Playing,
    Paused,
    /// Not started, stopped or ended.
    Stopped,
}

struct Playback {
    run: SequenceRun,
    time: f64,
    state: State,
    /// Applied at the next step even if it isn't playing (a jump).
    dirty: bool,
    /// Let go of at the next step (Stop).
    finish: bool,
    /// The camera before a cut took it, given back when it ends.
    camera: Option<(CameraType, f32)>,
    /// The levels it added.
    levels: Vec<String>,
}

/// Every Sequence played, shared by both sides' interpreters.
#[derive(Default)]
pub struct Sequences {
    playing: BTreeMap<InstanceId, Playback>,
    /// Whether AutoPlay ones have been started.
    started: bool,
    /// Bindings already warned about.
    warned: BTreeSet<String>,
}

#[derive(Clone, Default)]
pub(crate) struct SharedSequences(pub Rc<RefCell<Sequences>>);

fn shared(lua: &Lua) -> mlua::Result<Rc<RefCell<Sequences>>> {
    lua.app_data_ref::<SharedSequences>()
        .map(|shared| shared.0.clone())
        .ok_or_else(|| mlua::Error::runtime("sequences aren't ready"))
}

/// Reads the Sequence `id`'s file.
fn load(world: &World, assets: &Assets, id: InstanceId) -> Result<SequenceRun, String> {
    let instance = world.get(id).ok_or("it was destroyed")?;
    let path = instance
        .property("SequenceId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if path.is_empty() {
        return Err(format!(
            "{}'s SequenceId is empty; set it to a file such as assets/sequences/Intro.sequence.json",
            instance.name()
        ));
    }
    let file = load_sequence(assets, path).map_err(|why| format!("can't read {path}: {why}"))?;
    SequenceRun::new(file).map_err(|why| format!("can't play {path}: {}", why.0))
}

impl Sequences {
    /// Starts (or carries on) `id`.
    fn play(&mut self, world: &World, assets: &Assets, id: InstanceId) -> Result<(), String> {
        if let Some(playback) = self.playing.get_mut(&id) {
            match playback.state {
                State::Playing => {}
                State::Paused => playback.state = State::Playing,
                State::Stopped => {
                    playback.time = 0.0;
                    playback.state = State::Playing;
                    playback.finish = false;
                }
            }
            return Ok(());
        }
        let run = load(world, assets, id)?;
        self.playing.insert(
            id,
            Playback {
                run,
                time: 0.0,
                state: State::Playing,
                dirty: true,
                finish: false,
                camera: None,
                levels: Vec::new(),
            },
        );
        Ok(())
    }

    fn time(&self, id: InstanceId) -> f64 {
        self.playing.get(&id).map_or(0.0, |p| p.time)
    }

    /// Whether the player's input is locked: a Sequence with
    /// `LockPlayerInput` is playing or paused.
    pub fn input_locked(&self, world: &World) -> bool {
        self.playing.iter().any(|(&id, playback)| {
            playback.state != State::Stopped && flag(world, id, "LockPlayerInput")
        })
    }
}

fn flag(world: &World, id: InstanceId, name: &str) -> bool {
    world
        .get(id)
        .and_then(|i| i.property(name))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

/// The methods of Sequence Instances.
pub(crate) fn methods(lua: &Lua) -> mlua::Result<Vec<(ClassName, &'static str, Function)>> {
    let play = lua.create_function(|lua, this: LuaInstance| {
        let assets = lua
            .app_data_ref::<Assets>()
            .map(|assets| assets.clone())
            .unwrap_or_default();
        let shared = shared(lua)?;
        let world = world(lua)?;
        if !world.contains(this.0) {
            return Err(destroyed());
        }
        shared
            .borrow_mut()
            .play(&world, &assets, this.0)
            .map_err(mlua::Error::runtime)
    })?;
    let pause = lua.create_function(|lua, this: LuaInstance| {
        if let Some(playback) = shared(lua)?.borrow_mut().playing.get_mut(&this.0)
            && playback.state == State::Playing
        {
            playback.state = State::Paused;
        }
        Ok(())
    })?;
    let stop = lua.create_function(|lua, this: LuaInstance| {
        if let Some(playback) = shared(lua)?.borrow_mut().playing.get_mut(&this.0) {
            playback.state = State::Stopped;
            playback.time = 0.0;
            playback.finish = true;
        }
        Ok(())
    })?;
    Ok(vec![
        (ClassName::Sequence, "Play", play),
        (ClassName::Sequence, "Pause", pause),
        (ClassName::Sequence, "Stop", stop),
    ])
}

/// `TimePosition`, `IsPlaying` and `Length`, or `None` for other keys.
pub(crate) fn get(lua: &Lua, id: InstanceId, key: &str) -> mlua::Result<Option<mlua::Value>> {
    let shared = shared(lua)?;
    let sequences = shared.borrow();
    Ok(Some(match key {
        "TimePosition" => sequences.time(id).into_lua(lua)?,
        "IsPlaying" => sequences
            .playing
            .get(&id)
            .is_some_and(|p| p.state == State::Playing)
            .into_lua(lua)?,
        "Length" => {
            let loaded = sequences.playing.get(&id).map(|p| p.run.duration());
            drop(sequences);
            let length = match loaded {
                Some(length) => length,
                None => {
                    let assets = lua
                        .app_data_ref::<Assets>()
                        .map(|assets| assets.clone())
                        .unwrap_or_default();
                    let world = world(lua)?;
                    load(&world, &assets, id).map_or(0.0, |run| run.duration())
                }
            };
            length.into_lua(lua)?
        }
        _ => return Ok(None),
    }))
}

/// Whether `key` is one of the live fields [`get`] reads.
pub(crate) fn is_field(key: &str) -> bool {
    matches!(key, "TimePosition" | "IsPlaying" | "Length")
}

/// Sets `TimePosition`: the sequence jumps there at the next step, without
/// firing the events between.
pub(crate) fn set(lua: &Lua, id: InstanceId, key: &str, value: mlua::Value) -> mlua::Result<()> {
    if key != "TimePosition" {
        return Err(mlua::Error::runtime(format!(
            "Sequence.{key} is read-only; use Play, Pause and Stop"
        )));
    }
    let Some(seconds) = value.as_f64().filter(|s| s.is_finite()) else {
        return Err(mlua::Error::runtime(
            "Sequence.TimePosition must be a number of seconds",
        ));
    };
    let assets = lua
        .app_data_ref::<Assets>()
        .map(|assets| assets.clone())
        .unwrap_or_default();
    let shared = shared(lua)?;
    let mut sequences = shared.borrow_mut();
    if !sequences.playing.contains_key(&id) {
        // Jumping before playing loads it, paused.
        let world = world(lua)?;
        sequences
            .play(&world, &assets, id)
            .map_err(mlua::Error::runtime)?;
        if let Some(playback) = sequences.playing.get_mut(&id) {
            playback.state = State::Paused;
        }
    }
    let playback = sequences.playing.get_mut(&id).expect("loaded above");
    playback.time = seconds.clamp(0.0, playback.run.duration());
    playback.dirty = true;
    if playback.state == State::Stopped {
        playback.state = State::Paused;
    }
    Ok(())
}

/// What a step leaves for the runtime to do once the sequences are let go
/// of: events to fire, Sounds to start and stop, levels to add and remove,
/// and messages.
#[derive(Default)]
pub(crate) struct Stepped {
    pub events: Vec<(InstanceId, &'static str, Vec<String>)>,
    pub sounds: Vec<SoundCue>,
    pub add_levels: Vec<String>,
    pub remove_levels: Vec<String>,
    pub warnings: Vec<String>,
}

impl Sequences {
    /// Moves every playing sequence on by `seconds` and puts the world
    /// where they have it.
    pub(crate) fn step(
        &mut self,
        world: &mut World,
        assets: &Assets,
        camera: &mut Camera,
        seconds: f64,
    ) -> Stepped {
        let mut out = Stepped::default();
        if !self.started {
            self.started = true;
            let auto: Vec<InstanceId> = world
                .descendants_iter(world.root())
                .filter(|&id| {
                    world
                        .get(id)
                        .is_some_and(|i| i.class() == ClassName::Sequence)
                        && flag(world, id, "AutoPlay")
                })
                .collect();
            for id in auto {
                if let Err(why) = self.play(world, assets, id) {
                    out.warnings.push(why);
                }
            }
        }
        let ids: Vec<InstanceId> = self.playing.keys().copied().collect();
        for id in ids {
            let gone = !world.contains(id);
            let playback = self.playing.get_mut(&id).expect("listed above");
            if gone {
                playback.finish = true;
            }
            match playback.state {
                State::Playing => {
                    let from = playback.time;
                    let to = (from + seconds.max(0.0)).min(playback.run.duration());
                    playback.time = to;
                    playback.dirty = false;
                    apply(playback, world, camera, &mut out);
                    for name in playback.run.file.events_between(from, to) {
                        out.events.push((id, "EventReached", vec![name]));
                    }
                    if to >= playback.run.duration() {
                        playback.state = State::Stopped;
                        playback.finish = true;
                        out.events.push((id, "Completed", Vec::new()));
                    }
                }
                State::Paused | State::Stopped if playback.dirty => {
                    playback.dirty = false;
                    apply(playback, world, camera, &mut out);
                }
                _ => {}
            }
            if playback.finish {
                playback.finish = false;
                let finished = finish(playback, world, camera);
                out.sounds.extend(finished.sounds);
                out.remove_levels.extend(finished.remove_levels);
            }
            if gone {
                self.playing.remove(&id);
            }
        }
        for missing in self.playing.values().flat_map(|p| p.run.missing.iter()) {
            if self.warned.insert(missing.clone()) {
                out.warnings.push(format!(
                    "a sequence's track binds {missing}, which isn't in the game"
                ));
            }
        }
        out
    }

    /// Jumps every playing sequence that's `Skippable` to its end.
    pub(crate) fn skip(&mut self, world: &mut World, camera: &mut Camera) -> Stepped {
        let mut out = Stepped::default();
        for (&id, playback) in &mut self.playing {
            if playback.state != State::Playing || !flag(world, id, "Skippable") {
                continue;
            }
            playback.time = playback.run.duration();
            apply(playback, world, camera, &mut out);
            playback.state = State::Stopped;
            let finished = finish(playback, world, camera);
            out.sounds.extend(finished.sounds);
            out.remove_levels.extend(finished.remove_levels);
            out.events.push((id, "Completed", Vec::new()));
        }
        out
    }

    /// Whether any sequence is playing that the Menu action would skip.
    pub(crate) fn skippable(&self, world: &World) -> bool {
        self.playing
            .iter()
            .any(|(&id, p)| p.state == State::Playing && flag(world, id, "Skippable"))
    }
}

/// Puts the world (and the camera) where `playback` has it now.
fn apply(playback: &mut Playback, world: &mut World, camera: &mut Camera, out: &mut Stepped) {
    let applied = playback.run.apply(world, playback.time);
    match applied.camera {
        Some(shot) => {
            if playback.camera.is_none() {
                playback.camera = Some((camera.camera_type, camera.field_of_view));
            }
            camera.camera_type = CameraType::Scriptable;
            camera.state = CameraState {
                position: shot.position,
                orientation: shot.orientation,
            };
            camera.field_of_view = shot.field_of_view;
        }
        None => give_back_camera(playback, camera),
    }
    out.sounds.extend(applied.sounds);
    for level in &applied.levels {
        if !playback.levels.contains(level) {
            playback.levels.push(level.clone());
            out.add_levels.push(level.clone());
        }
    }
    playback.levels.retain(|level| {
        let keep = applied.levels.contains(level);
        if !keep {
            out.remove_levels.push(level.clone());
        }
        keep
    });
}

fn give_back_camera(playback: &mut Playback, camera: &mut Camera) {
    if let Some((camera_type, field_of_view)) = playback.camera.take() {
        camera.camera_type = camera_type;
        camera.field_of_view = field_of_view;
    }
}

/// What the Sounds in `cues` need from the host's audio: each started one
/// plays from its start (its `Playing` set), each stopped one stops. The
/// editor's preview uses this too.
pub fn sound_actions(
    world: &mut World,
    cues: Vec<SoundCue>,
) -> Vec<axcenity_core::sound::SoundAction> {
    use axcenity_core::sound::SoundAction;
    let mut actions = Vec::new();
    for cue in cues {
        match cue {
            SoundCue::Start(sound) => {
                let Some(instance) = world.get(sound) else {
                    continue;
                };
                let property = |name| instance.property(name);
                actions.push(SoundAction::Play {
                    instance: sound,
                    sound_id: property("SoundId")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned(),
                    volume: property("Volume").and_then(Value::as_number).unwrap_or(1.0),
                    looped: property("Looped").and_then(Value::as_bool).unwrap_or(false),
                });
                let _ = world.set_property(sound, "Playing", Value::Bool(true));
            }
            SoundCue::Stop(sound) => {
                let _ = world.set_property(sound, "Playing", Value::Bool(false));
                actions.push(SoundAction::Stop(sound));
            }
        }
    }
    actions
}

/// Lets go of `playback`: its clips and overlay, its Sounds and the camera.
/// Levels it added that reach its end stay; a Stop takes them all out.
fn finish(playback: &mut Playback, world: &mut World, camera: &mut Camera) -> Stepped {
    let mut out = Stepped {
        sounds: playback.run.finish(world),
        ..Stepped::default()
    };
    give_back_camera(playback, camera);
    if playback.time < playback.run.duration() {
        out.remove_levels = std::mem::take(&mut playback.levels);
    }
    out
}
