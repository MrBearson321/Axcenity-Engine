//! Cutscene sequences (`*.sequence.json`, Phase 51): a timeline of tracks
//! that move a world from start to end, played by a Sequence Instance and
//! previewed in the editor's Sequencer tab.
//!
//! A sequence's tracks, each finding what it moves by a **binding**: a path
//! of names from `game` (`Workspace/Door`) or `tag:` and a tag
//! (`tag:Hero`), which finds the first Instance with that tag, so placed
//! prefab copies can be bound.
//! - **Property tracks** (`tracks`): keys like a property clip's, one track
//!   per bound Instance and property, with the same easings and curves.
//!   Tracks whose target is [`CAMERA_TARGET`] are the sequence's own
//!   camera keys (`Position`, `Orientation`, `FieldOfView`).
//! - **Camera cuts**: from each cut's time to the next, the view is a bound
//!   Camera's (its `Position`, `Orientation` and `FieldOfView`), or with no
//!   camera, the sequence's own camera keys.
//! - **Animation sections**: a clip played on a bound Animator (or the
//!   Animator inside the bound Instance), blending in and out.
//! - **Audio sections**: a bound Sound plays from its start for the section.
//! - **Events**: a name, which hosts fire as `EventReached` when passed.
//! - **UI**: fades, letterbox bars and subtitles (string table keys), drawn
//!   by a ScreenGui named [`OVERLAY_NAME`] the sequence puts in the
//!   Workspace while it plays.
//! - **Level sections**: a level added beside the current one for the
//!   section (hosts do the adding, see [`Applied::levels`]).
//!
//! Playing is pure in time: [`SequenceRun::apply`] puts a world where the
//! sequence has it at a time, whatever time came before, so the preview,
//! Play and a built game show the same at the same time.

use std::collections::{BTreeSet, HashMap};

use axcenity_core::spline::SplineCurve;
use axcenity_core::{ClassName, Color3, InstanceId, UDim2, Value, Vector3, World};
use serde::{Deserialize, Serialize};

use crate::files::{ClipEvent, ClipFile, FileError, PropertyName, PropertyTrack};
use crate::property_clip::{
    Applied as PropertyApplied, EvaluatedProperty, PropertyClip, apply_evaluated, resolve_target,
    target_path,
};

pub const SEQUENCES_FOLDER: &str = "assets/sequences";
pub const SEQUENCE_EXTENSION: &str = ".sequence.json";
pub const SEQUENCE_FORMAT: u32 = 1;
/// The target of the sequence's own camera keys.
pub const CAMERA_TARGET: &str = "@Camera";
/// A binding that starts with this finds an Instance by tag.
pub const TAG_PREFIX: &str = "tag:";
/// The ScreenGui a playing sequence's UI tracks draw in.
pub const OVERLAY_NAME: &str = "Cutscene";
/// A camera's field of view when nothing says otherwise, in degrees.
const DEFAULT_FIELD_OF_VIEW: f32 = 50.0;
/// The overlay's `DisplayOrder`: over the game's own UI.
const OVERLAY_ORDER: f64 = 1000.0;

/// A `*.sequence.json` file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SequenceFile {
    pub format_version: u32,
    /// Seconds from start to end.
    pub duration: f64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub camera_cuts: Vec<CameraCut>,
    /// Property tracks, and the camera's own keys.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tracks: Vec<PropertyTrack>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub animations: Vec<AnimationSection>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub audio: Vec<AudioSection>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<ClipEvent>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fades: Vec<FadeSection>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub letterbox: Vec<LetterboxSection>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subtitles: Vec<SubtitleSection>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub levels: Vec<LevelSection>,
}

/// From `time` until the next cut, the view is `camera`'s (a binding), or
/// with `camera` empty, the sequence's own camera keys.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CameraCut {
    pub id: String,
    pub time: f64,
    #[serde(default)]
    pub camera: String,
}

/// `clip` (an `*.anim.json` path) played on the Animator bound by
/// `target` from `start` to `end`, from the clip's start. Its weight rises
/// over `blend_in` seconds and falls over the last `blend_out`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnimationSection {
    pub id: String,
    pub start: f64,
    pub end: f64,
    pub target: String,
    pub clip: String,
    #[serde(default)]
    pub blend_in: f64,
    #[serde(default)]
    pub blend_out: f64,
}

/// The Sound bound by `sound` plays from its start at `start` and stops at
/// `end`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioSection {
    pub id: String,
    pub start: f64,
    pub end: f64,
    pub sound: String,
}

/// The screen covered in `color`, from `from` (0 clear, 1 covered) at
/// `start` to `to` at `end`. After the section it stays at `to` until the
/// next fade.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FadeSection {
    pub id: String,
    pub start: f64,
    pub end: f64,
    pub from: f32,
    pub to: f32,
    #[serde(default = "black")]
    pub color: [f32; 3],
}

fn black() -> [f32; 3] {
    [0.0, 0.0, 0.0]
}

/// Black bars at the top and bottom of the screen, each `size` of its
/// height, sliding in over `ramp` seconds and out over the last `ramp`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LetterboxSection {
    pub id: String,
    pub start: f64,
    pub end: f64,
    #[serde(default = "letterbox_size")]
    pub size: f32,
    #[serde(default)]
    pub ramp: f64,
}

fn letterbox_size() -> f32 {
    0.12
}

/// A line at the bottom of the screen: the string table's `key` in the
/// player's language, or `text` when the table hasn't got it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubtitleSection {
    pub id: String,
    pub start: f64,
    pub end: f64,
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub text: String,
}

/// The level `level` is added beside the current one at `start` and taken
/// out at `end`. A section that reaches the sequence's end leaves it added.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LevelSection {
    pub id: String,
    pub start: f64,
    pub end: f64,
    pub level: String,
}

impl SequenceFile {
    /// An empty sequence `duration` seconds long.
    pub fn new(duration: f64) -> Self {
        Self {
            format_version: SEQUENCE_FORMAT,
            duration,
            camera_cuts: Vec::new(),
            tracks: Vec::new(),
            animations: Vec::new(),
            audio: Vec::new(),
            events: Vec::new(),
            fades: Vec::new(),
            letterbox: Vec::new(),
            subtitles: Vec::new(),
            levels: Vec::new(),
        }
    }

    /// Reads and checks a sequence: its keys sorted, every section inside
    /// the duration and ending after it starts.
    pub fn from_json(text: &str) -> Result<Self, FileError> {
        let mut file: SequenceFile =
            serde_json::from_str(text).map_err(|err| FileError(err.to_string()))?;
        if file.format_version != SEQUENCE_FORMAT {
            return Err(FileError(format!(
                "it's format {}; this engine reads format {SEQUENCE_FORMAT}",
                file.format_version
            )));
        }
        let keyed = PropertyClip::from_file(file.keyed_clip())?;
        file.set_keyed(keyed.file);
        file.camera_cuts.sort_by(|a, b| a.time.total_cmp(&b.time));
        let duration = file.duration;
        let inside = |what: &str, id: &str, start: f64, end: f64| {
            if !(start.is_finite() && end.is_finite() && 0.0 <= start && start <= end)
                || end > duration
            {
                return Err(FileError(format!(
                    "{what} `{id}` runs from {start}s to {end}s; it must start before it ends, inside 0..{duration}"
                )));
            }
            Ok(())
        };
        for cut in &file.camera_cuts {
            inside("camera cut", &cut.id, cut.time, cut.time)?;
        }
        for s in &file.animations {
            inside("animation", &s.id, s.start, s.end)?;
        }
        for s in &file.audio {
            inside("audio section", &s.id, s.start, s.end)?;
        }
        for s in &file.fades {
            inside("fade", &s.id, s.start, s.end)?;
        }
        for s in &file.letterbox {
            inside("letterbox", &s.id, s.start, s.end)?;
        }
        for s in &file.subtitles {
            inside("subtitle", &s.id, s.start, s.end)?;
        }
        for s in &file.levels {
            inside("level section", &s.id, s.start, s.end)?;
        }
        Ok(file)
    }

    /// Readable JSON, the same for the same sequence.
    pub fn to_json(&self) -> String {
        let mut text = serde_json::to_string_pretty(self).expect("sequence files always serialize");
        text.push('\n');
        text
    }

    /// The keys (property tracks and camera keys) and events as a clip,
    /// which the Sequencer's dope sheet and curves edit.
    pub fn keyed_clip(&self) -> ClipFile {
        let mut clip = ClipFile::new(self.duration);
        clip.tracks = self.tracks.clone();
        clip.events = self.events.clone();
        clip
    }

    /// Takes the keys, events and duration from `clip` (see
    /// [`keyed_clip`](Self::keyed_clip)).
    pub fn set_keyed(&mut self, clip: ClipFile) {
        self.duration = clip.duration;
        self.tracks = clip.tracks;
        self.events = clip.events;
    }

    /// The events passed going from `from` to `to` seconds, in time order:
    /// those at `from` or later and before `to`, and at the very end those
    /// at the end too.
    pub fn events_between(&self, from: f64, to: f64) -> Vec<String> {
        let end = to >= self.duration;
        let mut passed: Vec<&ClipEvent> = self
            .events
            .iter()
            .filter(|e| e.time >= from && (e.time < to || (end && e.time <= to)))
            .collect();
        passed.sort_by(|a, b| a.time.total_cmp(&b.time));
        passed.into_iter().map(|e| e.name.clone()).collect()
    }

    /// The levels added at `time`.
    pub fn levels_at(&self, time: f64) -> Vec<String> {
        self.levels
            .iter()
            .filter(|s| s.start <= time && (time < s.end || s.end >= self.duration))
            .map(|s| s.level.clone())
            .collect()
    }

    /// How much of the screen the fade covers at `time` (0 to 1), and its color.
    pub fn fade_at(&self, time: f64) -> (f32, [f32; 3]) {
        let mut shown = (0.0, black());
        let mut latest = f64::NEG_INFINITY;
        for fade in &self.fades {
            if fade.start <= time && time <= fade.end {
                let span = (fade.end - fade.start).max(1e-9);
                let t = ((time - fade.start) / span).clamp(0.0, 1.0) as f32;
                return (fade.from + (fade.to - fade.from) * t, fade.color);
            }
            if fade.end < time && fade.end > latest {
                latest = fade.end;
                shown = (fade.to, fade.color);
            }
        }
        shown
    }

    /// How tall each letterbox bar is at `time`, as a share of the screen.
    pub fn letterbox_at(&self, time: f64) -> f32 {
        self.letterbox
            .iter()
            .filter(|s| s.start <= time && time <= s.end)
            .map(|s| {
                let ramp = |gone: f64| {
                    if s.ramp > 0.0 {
                        (gone / s.ramp).clamp(0.0, 1.0) as f32
                    } else {
                        1.0
                    }
                };
                s.size * ramp(time - s.start).min(ramp(s.end - time))
            })
            .fold(0.0, f32::max)
    }

    /// The subtitle showing at `time`.
    pub fn subtitle_at(&self, time: f64) -> Option<&SubtitleSection> {
        self.subtitles
            .iter()
            .rfind(|s| s.start <= time && time < s.end)
    }

    /// The camera cut in charge at `time`.
    pub fn cut_at(&self, time: f64) -> Option<&CameraCut> {
        self.camera_cuts.iter().rfind(|cut| cut.time <= time)
    }

    /// When the cut in charge at `time` gives way: the next cut's time, or
    /// the end.
    pub fn cut_end(&self, time: f64) -> f64 {
        self.camera_cuts
            .iter()
            .find(|cut| cut.time > time)
            .map_or(self.duration, |cut| cut.time)
    }

    /// Whether anything is drawn in the overlay.
    fn has_ui(&self) -> bool {
        !(self.fades.is_empty() && self.letterbox.is_empty() && self.subtitles.is_empty())
    }
}

/// Reads the sequence at `path` through `assets`, or says why it can't.
pub fn load_sequence(assets: &axcenity_assets::Assets, path: &str) -> Result<SequenceFile, String> {
    let bytes = assets.read_file(path)?;
    let text = String::from_utf8(bytes).map_err(|_| "it isn't text".to_owned())?;
    SequenceFile::from_json(&text).map_err(|err| err.0)
}

/// The weight of `section` at `time`: 0 outside it, rising over its blend
/// in and falling over its blend out.
fn section_weight(section: &AnimationSection, time: f64) -> f64 {
    if time < section.start || time > section.end {
        return 0.0;
    }
    let ramp = |gone: f64, blend: f64| {
        if blend > 0.0 {
            (gone / blend).clamp(0.0, 1.0)
        } else {
            1.0
        }
    };
    ramp(time - section.start, section.blend_in).min(ramp(section.end - time, section.blend_out))
}

/// The Instance `binding` names: `tag:` and a tag finds the first Instance
/// with that tag, anything else is a path of names from `game`.
pub fn resolve_binding(world: &World, binding: &str) -> Option<InstanceId> {
    if let Some(tag) = binding.strip_prefix(TAG_PREFIX) {
        return world.tagged(tag.trim()).first().copied();
    }
    let binding = binding.trim();
    if binding.is_empty() || binding.starts_with('@') {
        return None;
    }
    resolve_target(world, world.root(), binding)
}

/// The path binding of `id` (`Workspace/Door`).
pub fn binding_path(world: &World, id: InstanceId) -> Option<String> {
    target_path(world, world.root(), id).filter(|path| path != ".")
}

/// Where a camera cut looks from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shot {
    pub position: Vector3,
    /// Degrees, as a Part's.
    pub orientation: Vector3,
    /// Vertical, in degrees.
    pub field_of_view: f32,
}

/// A Sound to start from its beginning or stop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoundCue {
    Start(InstanceId),
    Stop(InstanceId),
}

/// What a sequence leaves to its host at one time.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Applied {
    /// The view, while a camera cut is in charge.
    pub camera: Option<Shot>,
    /// Sounds to start or stop since the last time applied.
    pub sounds: Vec<SoundCue>,
    /// The levels that should be added now.
    pub levels: Vec<String>,
}

/// One sequence playing in one world: what it made there (clips loaded on
/// Animators, the overlay) and what it set going (Sounds).
#[derive(Debug)]
pub struct SequenceRun {
    pub file: SequenceFile,
    keys: PropertyClip,
    properties: PropertyApplied,
    /// Each animation section's Animator, Animation and AnimationTrack.
    clips: HashMap<String, (InstanceId, InstanceId, InstanceId)>,
    overlay: Option<InstanceId>,
    sounding: BTreeSet<InstanceId>,
    /// Bindings (and `binding:Property`) that found nothing, so a host
    /// reports each once.
    pub missing: BTreeSet<String>,
}

impl SequenceRun {
    pub fn new(file: SequenceFile) -> Result<Self, FileError> {
        let keys = PropertyClip::from_file(file.keyed_clip())?;
        Ok(Self {
            file,
            keys,
            properties: PropertyApplied::default(),
            clips: HashMap::new(),
            overlay: None,
            sounding: BTreeSet::new(),
            missing: BTreeSet::new(),
        })
    }

    pub fn duration(&self) -> f64 {
        self.file.duration
    }

    /// Puts `world` where the sequence has it at `time` seconds (kept
    /// inside the duration): keyed properties, clips on Animators and the
    /// overlay. Hosts do the rest: the camera, Sounds and levels.
    pub fn apply(&mut self, world: &mut World, time: f64) -> Applied {
        let time = if time.is_finite() {
            time.clamp(0.0, self.file.duration.max(0.0))
        } else {
            0.0
        };
        let evaluated = self.keys.evaluate(time);
        let mut own_camera = Vec::new();
        let mut targets: Vec<(&str, Vec<EvaluatedProperty>)> = Vec::new();
        for item in &evaluated {
            if item.target == CAMERA_TARGET {
                own_camera.push(item);
                continue;
            }
            let rooted = EvaluatedProperty {
                target: ".".to_owned(),
                ..item.clone()
            };
            match targets.iter_mut().find(|(t, _)| *t == item.target) {
                Some((_, items)) => items.push(rooted),
                None => targets.push((&item.target, vec![rooted])),
            }
        }
        for (binding, items) in targets {
            let Some(id) = resolve_binding(world, binding) else {
                self.missing.insert(binding.to_owned());
                continue;
            };
            apply_evaluated(world, id, &items, &mut self.properties);
            for missed in std::mem::take(&mut self.properties.missing) {
                let property = missed.rsplit(':').next().unwrap_or_default();
                self.missing.insert(format!("{binding}:{property}"));
            }
        }
        self.apply_clips(world, time);
        self.apply_overlay(world, time);
        Applied {
            camera: self.shot(world, time, &own_camera),
            sounds: self.cue_sounds(world, time),
            levels: self.file.levels_at(time),
        }
    }

    /// Lets go of what the sequence set going: its clips stop, the overlay
    /// goes and its Sounds stop. Keyed properties stay where they are.
    pub fn finish(&mut self, world: &mut World) -> Vec<SoundCue> {
        for &(_, _, track) in self.clips.values() {
            if let Some(state) = world.animation_track_mut(track) {
                state.playing = false;
                state.stopping = false;
                state.weight = 0.0;
            }
        }
        if let Some(overlay) = self.overlay.take() {
            let _ = world.destroy(overlay);
        }
        std::mem::take(&mut self.sounding)
            .into_iter()
            .map(SoundCue::Stop)
            .collect()
    }

    /// Stops the Sounds it set going, leaving the rest (the editor's
    /// preview, scrubbing). The next apply starts those it wants again.
    pub fn stop_sounds(&mut self) -> Vec<SoundCue> {
        std::mem::take(&mut self.sounding)
            .into_iter()
            .map(SoundCue::Stop)
            .collect()
    }

    fn apply_clips(&mut self, world: &mut World, time: f64) {
        for section in &self.file.animations {
            let weight = section_weight(section, time);
            let animator = resolve_binding(world, &section.target).and_then(|id| {
                let instance = world.get(id)?;
                if instance.class() == ClassName::Animator {
                    return Some(id);
                }
                instance.children().iter().copied().find(|&c| {
                    world
                        .get(c)
                        .is_some_and(|c| c.class() == ClassName::Animator)
                })
            });
            let Some(animator) = animator else {
                self.missing.insert(section.target.clone());
                continue;
            };
            let loaded = self
                .clips
                .get(&section.id)
                .filter(|(on, _, track)| *on == animator && world.contains(*track))
                .copied();
            let track = match loaded {
                Some((_, _, track)) => track,
                None if weight > 0.0 => {
                    let Some(track) = load_clip(world, animator, &section.clip) else {
                        continue;
                    };
                    self.clips.insert(section.id.clone(), track);
                    track.2
                }
                None => continue,
            };
            let Some(state) = world.animation_track_mut(track) else {
                continue;
            };
            if weight > 0.0 {
                // Held at the sequence's time: the Animators sample it there
                // and don't move it on.
                state.playing = true;
                state.stopping = false;
                state.speed = 0.0;
                state.fade = 0.0;
                state.time = time - section.start;
                state.weight = weight;
                state.target_weight = weight;
            } else if state.playing {
                state.playing = false;
                state.weight = 0.0;
            }
        }
    }

    fn cue_sounds(&mut self, world: &World, time: f64) -> Vec<SoundCue> {
        let mut wanted = BTreeSet::new();
        for section in &self.file.audio {
            if !(section.start <= time && time < section.end) {
                continue;
            }
            match resolve_binding(world, &section.sound)
                .filter(|&id| world.get(id).is_some_and(|s| s.class() == ClassName::Sound))
            {
                Some(sound) => {
                    wanted.insert(sound);
                }
                None => {
                    self.missing.insert(section.sound.clone());
                }
            }
        }
        let mut cues: Vec<SoundCue> = self
            .sounding
            .difference(&wanted)
            .map(|&id| SoundCue::Stop(id))
            .collect();
        cues.extend(
            wanted
                .difference(&self.sounding)
                .map(|&id| SoundCue::Start(id)),
        );
        self.sounding = wanted;
        cues
    }

    fn shot(&mut self, world: &World, time: f64, own: &[&EvaluatedProperty]) -> Option<Shot> {
        let cut = self.file.cut_at(time)?;
        if cut.camera.is_empty() {
            let numbers = |property| {
                own.iter()
                    .find(|item| item.property == property)
                    .map(|item| item.value.as_slice())
            };
            let vector = |property| match numbers(property) {
                Some([x, y, z]) => Some(Vector3::new(*x, *y, *z)),
                _ => None,
            };
            return Some(Shot {
                position: vector(PropertyName::Position)?,
                orientation: vector(PropertyName::Orientation).unwrap_or_default(),
                field_of_view: match numbers(PropertyName::FieldOfView) {
                    Some([fov]) => *fov,
                    _ => DEFAULT_FIELD_OF_VIEW,
                },
            });
        }
        let bound = resolve_binding(world, &cut.camera);
        let Some(camera) = bound.and_then(|id| world.get(id)) else {
            self.missing.insert(cut.camera.clone());
            return None;
        };
        // A Spline (Phase 73): the camera rides it from its start at the
        // cut to its end when the cut gives way, facing along it.
        if camera.class() == ClassName::Spline {
            let curve = SplineCurve::of(world, bound?)?;
            let span = self.file.cut_end(time) - cut.time;
            let share = if span > 0.0 {
                ((time - cut.time) / span).clamp(0.0, 1.0)
            } else {
                1.0
            };
            let distance = share as f32 * curve.length();
            return Some(Shot {
                position: curve.position_at_distance(distance),
                orientation: curve.orientation_at_distance(distance),
                field_of_view: DEFAULT_FIELD_OF_VIEW,
            });
        }
        let vector = |name| camera.property(name).and_then(Value::as_vector3);
        Some(Shot {
            position: vector("Position")?,
            orientation: vector("Orientation").unwrap_or_default(),
            field_of_view: camera
                .property("FieldOfView")
                .and_then(Value::as_number)
                .map_or(DEFAULT_FIELD_OF_VIEW, |n| n as f32),
        })
    }

    fn apply_overlay(&mut self, world: &mut World, time: f64) {
        if !self.file.has_ui() {
            return;
        }
        let overlay = match self.overlay.filter(|&id| world.contains(id)) {
            Some(overlay) => overlay,
            None => {
                let overlay = make_overlay(world);
                self.overlay = Some(overlay);
                overlay
            }
        };
        let child = |world: &World, name| world.find_first_child(overlay, name);
        let (cover, [r, g, b]) = self.file.fade_at(time);
        if let Some(fade) = child(world, "Fade") {
            set(
                world,
                fade,
                "BackgroundColor3",
                Value::Color3(Color3::new(r, g, b)),
            );
            let shown = 1.0 - f64::from(cover.clamp(0.0, 1.0));
            set(world, fade, "BackgroundTransparency", Value::Number(shown));
            set(world, fade, "Visible", Value::Bool(cover > 0.0));
        }
        let bar = self.file.letterbox_at(time);
        for (name, top) in [("TopBar", 0.0), ("BottomBar", 1.0 - bar)] {
            if let Some(id) = child(world, name) {
                set(
                    world,
                    id,
                    "Position",
                    Value::UDim2(UDim2::new(0.0, 0.0, top, 0.0)),
                );
                set(
                    world,
                    id,
                    "Size",
                    Value::UDim2(UDim2::new(1.0, 0.0, bar, 0.0)),
                );
                set(world, id, "Visible", Value::Bool(bar > 0.0));
            }
        }
        let subtitle = self.file.subtitle_at(time).cloned();
        if let Some(label) = child(world, "Subtitle") {
            let (key, text) = subtitle
                .as_ref()
                .map(|s| (s.key.clone(), s.text.clone()))
                .unwrap_or_default();
            set(world, label, "Visible", Value::Bool(subtitle.is_some()));
            set(world, label, "TextKey", Value::String(key));
            set(world, label, "Text", Value::String(text));
        }
    }
}

/// Sets `name` of `id` if it differs, so an unchanged frame records no change.
fn set(world: &mut World, id: InstanceId, name: &str, value: Value) {
    if world.get(id).and_then(|i| i.property(name)) != Some(&value) {
        let _ = world.set_property(id, name, value);
    }
}

/// Loads `clip` on `animator` through an Animation of its own, outside the
/// tree (so it's never saved or shown): the Animator, Animation and track.
fn load_clip(
    world: &mut World,
    animator: InstanceId,
    clip: &str,
) -> Option<(InstanceId, InstanceId, InstanceId)> {
    let animation = world.create(ClassName::Animation).ok()?;
    world
        .set_property(animation, "Clip", Value::String(clip.to_owned()))
        .ok()?;
    let track = world.load_animation(animator, animation)?;
    let order = world.animator_mut(animator)?.start_order();
    world.animation_track_mut(track)?.order = order;
    Some((animator, animation, track))
}

/// The overlay ScreenGui in the Workspace: a fade over everything, the two
/// letterbox bars and the subtitle line, all hidden.
fn make_overlay(world: &mut World) -> InstanceId {
    let gui = world.create(ClassName::ScreenGui).expect("not a service");
    let _ = world.set_name(gui, OVERLAY_NAME);
    set(world, gui, "DisplayOrder", Value::Number(OVERLAY_ORDER));
    let parts = [
        ("TopBar", ClassName::Frame, 1.0),
        ("BottomBar", ClassName::Frame, 1.0),
        ("Subtitle", ClassName::TextLabel, 2.0),
        ("Fade", ClassName::Frame, 3.0),
    ];
    for (name, class, z) in parts {
        let id = world.create(class).expect("not a service");
        let _ = world.set_name(id, name);
        set(world, id, "Visible", Value::Bool(false));
        set(world, id, "Interactable", Value::Bool(false));
        set(world, id, "ZIndex", Value::Number(z));
        set(
            world,
            id,
            "BackgroundColor3",
            Value::Color3(Color3::new(0.0, 0.0, 0.0)),
        );
        if class == ClassName::TextLabel {
            set(
                world,
                id,
                "Position",
                Value::UDim2(UDim2::new(0.1, 0.0, 0.78, 0.0)),
            );
            set(
                world,
                id,
                "Size",
                Value::UDim2(UDim2::new(0.8, 0.0, 0.08, 0.0)),
            );
            set(world, id, "TextSize", Value::Number(28.0));
            set(world, id, "Text", Value::String(String::new()));
        } else {
            set(
                world,
                id,
                "Size",
                Value::UDim2(UDim2::new(1.0, 0.0, 1.0, 0.0)),
            );
        }
        let _ = world.set_parent(id, Some(gui));
    }
    let workspace = world.workspace();
    let _ = world.set_parent(gui, Some(workspace));
    gui
}

#[cfg(test)]
mod tests;
