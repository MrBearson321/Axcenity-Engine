//! The Sequencer tab (Phase 51): a cutscene sequence (`*.sequence.json`)
//! on one timeline.
//!
//! The Animation tab's pieces do the keys: the timeline (playhead, events
//! and the end, which is the duration) and the property editor's dope
//! sheet and Curves view, over the sequence's property tracks and its own
//! camera keys (`@Camera`). Under the timeline, a row of sections for each
//! other track: camera cuts, clips on Animators, Sounds, fades, letterbox,
//! subtitles and levels; drag a section to move it, or its ends to resize
//! it, and pick one to set it in the inspector below.
//!
//! Above the timeline, the Animation tab's transport bar (start, a frame
//! back, play or pause, a frame on, end, stop, a seek bar, the time and the
//! preview's rate), with its keys while the mouse is over the tab.
//!
//! **Preview** shows the sequence in the Scene view at the playhead without
//! entering Play: a copy of the world put where the sequence has it (by the
//! same `SequenceRun` the game plays), posed by its own Animators and seen
//! from the camera cut, with its Sounds while it plays.
//!
//! Each edit is written to the file once the mouse is let go, as one undo
//! step.

use std::path::Path;

use axcenity_anim::sequence::{
    AnimationSection, AudioSection, CAMERA_TARGET, CameraCut, FadeSection, LetterboxSection,
    LevelSection, SEQUENCE_EXTENSION, SEQUENCES_FOLDER, SequenceFile, SequenceRun, Shot,
    SubtitleSection,
};
use axcenity_anim::{Animators, ClipEvent, EvaluatedProperty, Poses, PropertyName};
use axcenity_assets::Assets;
use axcenity_audio::{GameAudio, Listener};
use axcenity_core::{InstanceId, Vector3, World};
use eframe::egui;

use crate::animation::object_preview::Focus;
use crate::animation::property_editor::PropertyDoc;
use crate::animation::timeline::Timeline;
use crate::animation::transport;
use crate::theme::{self, icons};

/// How long a new sequence is, in seconds.
pub(crate) const NEW_DURATION: f64 = 10.0;
/// A row of sections' height, in points.
const ROW: f32 = 22.0;
/// How close (in points) to a section's end grabs the end.
const EDGE: f32 = 6.0;
/// The longest frame the playhead moves by while playing.
const MAX_FRAME_SECONDS: f64 = 0.1;

/// The path of the sequence called `name`.
pub fn sequence_path(name: &str) -> String {
    format!("{SEQUENCES_FOLDER}/{}{SEQUENCE_EXTENSION}", name.trim())
}

/// The name of the sequence at `path` (`Intro` for
/// `assets/sequences/Intro.sequence.json`).
pub fn sequence_name(path: &str) -> &str {
    let file = path.rsplit('/').next().unwrap_or(path);
    file.strip_suffix(SEQUENCE_EXTENSION).unwrap_or(file)
}

/// The rows of sections, top to bottom.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Row {
    Camera,
    Animation,
    Audio,
    Fade,
    Letterbox,
    Subtitle,
    Level,
}

impl Row {
    pub const ALL: [Row; 7] = [
        Row::Camera,
        Row::Animation,
        Row::Audio,
        Row::Fade,
        Row::Letterbox,
        Row::Subtitle,
        Row::Level,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Row::Camera => "Camera cuts",
            Row::Animation => "Animation",
            Row::Audio => "Audio",
            Row::Fade => "Fade",
            Row::Letterbox => "Letterbox",
            Row::Subtitle => "Subtitles",
            Row::Level => "Levels",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Row::Camera => icons::VIDEO_CAMERA,
            Row::Animation => icons::PERSON_SIMPLE_WALK,
            Row::Audio => icons::WAVEFORM,
            Row::Fade => icons::CIRCLE_HALF,
            Row::Letterbox => icons::FRAME_CORNERS,
            Row::Subtitle => icons::CHAT_TEXT,
            Row::Level => icons::STACK,
        }
    }
}

/// A section, by its row and its place in that row's list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Section {
    pub row: Row,
    pub index: usize,
}

/// What a section's drag moves.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Grab {
    Body { start: f64, end: f64 },
    Start,
    End,
}

/// What's selected in the Explorer, for binding new tracks and keying it.
pub struct Picked {
    pub id: InstanceId,
    /// Its path binding (`Workspace/Door`).
    pub path: String,
    /// Its tags, for binding by tag instead.
    pub tags: Vec<String>,
    pub name: String,
}

/// What the tab is shown with.
pub struct SequencerScene<'a> {
    /// Every sequence file in the project.
    pub files: &'a [String],
    /// Every clip (`*.anim.json`), for animation sections.
    pub clips: &'a [String],
    /// The project's levels, for level sections.
    pub levels: &'a [String],
    pub picked: Option<Picked>,
    /// The world as the preview shows it now, to key what's picked.
    pub world: &'a World,
    /// The editor's fly camera, to key the sequence's own camera.
    pub camera: (Vector3, Vector3),
    pub editable: bool,
}

/// What the tab asks the editor for.
#[derive(Clone, Debug, PartialEq)]
pub enum SequencerAction {
    Open(String),
    New(String),
    /// A Sequence Instance in the Workspace that plays the open file.
    Insert(String),
}

/// One open sequence.
pub struct SequenceDoc {
    pub path: String,
    /// Its sections. Its keys, events and duration are `keys.file`'s.
    pub file: SequenceFile,
    pub keys: PropertyDoc,
    /// The file as last written (or read).
    written: String,
    pub selected: Option<Section>,
    drag: Option<(Section, Grab, f64)>,
    /// Bind new sections by the picked Instance's first tag.
    pub by_tag: bool,
}

impl SequenceDoc {
    pub fn open(path: String, file: SequenceFile) -> Self {
        let written = file.to_json();
        let mut keys = PropertyDoc::open(path.clone(), file.keyed_clip());
        keys.new_keys = Default::default();
        keys.sequence = true;
        Self {
            path,
            file,
            keys,
            written,
            selected: None,
            drag: None,
            by_tag: false,
        }
    }

    /// The whole sequence as it is now.
    pub fn current(&self) -> SequenceFile {
        let mut file = self.file.clone();
        file.set_keyed(self.keys.file.clone());
        file
    }

    pub fn duration(&self) -> f64 {
        self.keys.file.duration
    }

    /// Writes the file if it changed: the text before and after.
    pub fn write(&mut self, project: &Path) -> std::io::Result<Option<(String, String)>> {
        let after = self.current().to_json();
        if after == self.written {
            return Ok(None);
        }
        axcenity_anim::files::write_file(&project.join(&self.path), &after)?;
        let before = std::mem::replace(&mut self.written, after.clone());
        Ok(Some((before, after)))
    }

    /// Shows `file` (the file after an undo) in place of what's shown.
    pub fn show_file(&mut self, file: SequenceFile) {
        self.written = file.to_json();
        self.keys.show_file(file.keyed_clip());
        self.file = file;
        if self
            .selected
            .is_some_and(|s| s.index >= self.sections(s.row).len())
        {
            self.selected = None;
        }
    }

    /// Each section of `row`: its start, end and label.
    pub fn sections(&self, row: Row) -> Vec<(f64, f64, String)> {
        let f = &self.file;
        let name = |path: &str| path.rsplit('/').next().unwrap_or(path).to_owned();
        match row {
            Row::Camera => {
                let mut cuts: Vec<(f64, f64, String)> = Vec::new();
                for (i, cut) in f.camera_cuts.iter().enumerate() {
                    let end = f
                        .camera_cuts
                        .get(i + 1)
                        .map_or(self.duration(), |next| next.time);
                    let label = if cut.camera.is_empty() {
                        "Own keys".to_owned()
                    } else {
                        name(&cut.camera)
                    };
                    cuts.push((cut.time, end, label));
                }
                cuts
            }
            Row::Animation => f
                .animations
                .iter()
                .map(|s| {
                    (
                        s.start,
                        s.end,
                        axcenity_anim::files::clip_name(&s.clip).to_owned(),
                    )
                })
                .collect(),
            Row::Audio => f
                .audio
                .iter()
                .map(|s| (s.start, s.end, name(&s.sound)))
                .collect(),
            Row::Fade => f
                .fades
                .iter()
                .map(|s| {
                    (
                        s.start,
                        s.end,
                        format!("{:.0}% → {:.0}%", s.from * 100.0, s.to * 100.0),
                    )
                })
                .collect(),
            Row::Letterbox => f
                .letterbox
                .iter()
                .map(|s| (s.start, s.end, format!("{:.0}%", s.size * 100.0)))
                .collect(),
            Row::Subtitle => f
                .subtitles
                .iter()
                .map(|s| {
                    let label = if s.text.is_empty() { &s.key } else { &s.text };
                    (s.start, s.end, label.clone())
                })
                .collect(),
            Row::Level => f
                .levels
                .iter()
                .map(|s| (s.start, s.end, s.level.clone()))
                .collect(),
        }
    }

    /// Moves `section` to run from `start` to `end` (a cut only moves its time).
    pub fn set_span(&mut self, section: Section, start: f64, end: f64) {
        let duration = self.duration();
        let start = start.clamp(0.0, duration);
        let end = end.clamp(start, duration);
        let f = &mut self.file;
        let i = section.index;
        let span = |s: &mut f64, e: &mut f64| {
            *s = start;
            *e = end;
        };
        match section.row {
            Row::Camera => {
                if let Some(cut) = f.camera_cuts.get_mut(i) {
                    cut.time = start;
                }
                let id = f.camera_cuts.get(i).map(|c| c.id.clone());
                f.camera_cuts.sort_by(|a, b| a.time.total_cmp(&b.time));
                if let Some(id) = id {
                    let index = f.camera_cuts.iter().position(|c| c.id == id);
                    self.selected = index.map(|index| Section {
                        row: Row::Camera,
                        index,
                    });
                }
            }
            Row::Animation => {
                if let Some(s) = f.animations.get_mut(i) {
                    span(&mut s.start, &mut s.end);
                }
            }
            Row::Audio => {
                if let Some(s) = f.audio.get_mut(i) {
                    span(&mut s.start, &mut s.end);
                }
            }
            Row::Fade => {
                if let Some(s) = f.fades.get_mut(i) {
                    span(&mut s.start, &mut s.end);
                }
            }
            Row::Letterbox => {
                if let Some(s) = f.letterbox.get_mut(i) {
                    span(&mut s.start, &mut s.end);
                }
            }
            Row::Subtitle => {
                if let Some(s) = f.subtitles.get_mut(i) {
                    span(&mut s.start, &mut s.end);
                }
            }
            Row::Level => {
                if let Some(s) = f.levels.get_mut(i) {
                    span(&mut s.start, &mut s.end);
                }
            }
        }
    }

    /// Takes `section` out.
    pub fn delete(&mut self, section: Section) {
        let f = &mut self.file;
        let i = section.index;
        fn take<T>(list: &mut Vec<T>, i: usize) {
            if i < list.len() {
                list.remove(i);
            }
        }
        match section.row {
            Row::Camera => take(&mut f.camera_cuts, i),
            Row::Animation => take(&mut f.animations, i),
            Row::Audio => take(&mut f.audio, i),
            Row::Fade => take(&mut f.fades, i),
            Row::Letterbox => take(&mut f.letterbox, i),
            Row::Subtitle => take(&mut f.subtitles, i),
            Row::Level => take(&mut f.levels, i),
        }
        self.selected = None;
    }

    /// An id for a new section of `row`.
    fn new_id(&self, prefix: &str) -> String {
        let f = &self.file;
        let taken: Vec<&str> = f
            .camera_cuts
            .iter()
            .map(|s| s.id.as_str())
            .chain(f.animations.iter().map(|s| s.id.as_str()))
            .chain(f.audio.iter().map(|s| s.id.as_str()))
            .chain(f.fades.iter().map(|s| s.id.as_str()))
            .chain(f.letterbox.iter().map(|s| s.id.as_str()))
            .chain(f.subtitles.iter().map(|s| s.id.as_str()))
            .chain(f.levels.iter().map(|s| s.id.as_str()))
            .collect();
        (1..)
            .map(|n| format!("{prefix}{n}"))
            .find(|id| !taken.contains(&id.as_str()))
            .expect("there's always a free number")
    }

    /// Adds a section to `row` at the playhead, `binding` (when it takes
    /// one) bound to what's picked. Returns it.
    pub fn add(&mut self, row: Row, binding: String, extra: &str) -> Section {
        let start = self.keys.playhead.clamp(0.0, self.duration());
        let end = (start + 2.0).min(self.duration());
        let index = match row {
            Row::Camera => {
                let id = self.new_id("cut");
                let f = &mut self.file;
                f.camera_cuts.push(CameraCut {
                    id: id.clone(),
                    time: start,
                    camera: binding,
                });
                f.camera_cuts.sort_by(|a, b| a.time.total_cmp(&b.time));
                f.camera_cuts.iter().position(|c| c.id == id).unwrap_or(0)
            }
            Row::Animation => {
                let id = self.new_id("clip");
                let f = &mut self.file;
                f.animations.push(AnimationSection {
                    id,
                    start,
                    end,
                    target: binding,
                    clip: extra.to_owned(),
                    blend_in: 0.25,
                    blend_out: 0.25,
                });
                f.animations.len() - 1
            }
            Row::Audio => {
                let id = self.new_id("sound");
                let f = &mut self.file;
                f.audio.push(AudioSection {
                    id,
                    start,
                    end,
                    sound: binding,
                });
                f.audio.len() - 1
            }
            Row::Fade => {
                let id = self.new_id("fade");
                let f = &mut self.file;
                f.fades.push(FadeSection {
                    id,
                    start,
                    end: (start + 1.0).min(end),
                    from: 1.0,
                    to: 0.0,
                    color: [0.0, 0.0, 0.0],
                });
                f.fades.len() - 1
            }
            Row::Letterbox => {
                let id = self.new_id("bars");
                let f = &mut self.file;
                f.letterbox.push(LetterboxSection {
                    id,
                    start,
                    end,
                    size: 0.12,
                    ramp: 0.5,
                });
                f.letterbox.len() - 1
            }
            Row::Subtitle => {
                let id = self.new_id("line");
                let f = &mut self.file;
                f.subtitles.push(SubtitleSection {
                    id,
                    start,
                    end,
                    key: extra.to_owned(),
                    text: String::new(),
                });
                f.subtitles.len() - 1
            }
            Row::Level => {
                let id = self.new_id("level");
                let f = &mut self.file;
                f.levels.push(LevelSection {
                    id,
                    start,
                    end,
                    level: extra.to_owned(),
                });
                f.levels.len() - 1
            }
        };
        let section = Section { row, index };
        self.selected = Some(section);
        section
    }

    /// Sets the duration: keys, events and sections past it move to it.
    pub fn set_duration(&mut self, duration: f64) {
        let duration = duration.max(0.1);
        self.keys.file.duration = duration;
        for key in self.keys.file.tracks.iter_mut().flat_map(|t| &mut t.keys) {
            key.time = key.time.min(duration);
        }
        for event in &mut self.keys.file.events {
            event.time = event.time.min(duration);
        }
        for row in Row::ALL {
            for (index, (start, end, _)) in self.sections(row).into_iter().enumerate() {
                self.set_span(Section { row, index }, start, end);
            }
        }
        self.keys.playhead = self.keys.playhead.min(duration);
    }

    /// Moves the playhead on while it plays.
    pub fn tick(&mut self, dt: f64) {
        self.keys.tick(dt);
    }
}

/// Where the preview is.
pub struct Preview {
    /// What it was made from: the edit world's revision and the sequence.
    made_from: ((u64, bool), String),
    pub world: World,
    run: SequenceRun,
    animators: Animators,
    shown_at: Option<f64>,
    pub camera: Option<Shot>,
}

impl Preview {
    pub fn poses(&self) -> &Poses {
        self.animators.poses()
    }
}

/// The Sequencer tab.
#[derive(Default)]
pub struct Sequencer {
    pub doc: Option<SequenceDoc>,
    /// Show the open sequence in the Scene view.
    pub preview_on: bool,
    new_name: String,
    preview: Option<Preview>,
    /// How fast the preview plays (the transport's rate).
    pub rate: transport::Rate,
    audio: GameAudio,
    /// Errors to show once.
    pub errors: Vec<String>,
}

impl Sequencer {
    /// Opens `file` from `path`.
    pub fn open(&mut self, path: String, file: SequenceFile) {
        self.doc = Some(SequenceDoc::open(path, file));
        self.preview = None;
        self.preview_on = true;
    }

    /// Whether the open sequence is at `path`.
    pub fn is_open(&self, path: &str) -> bool {
        self.doc.as_ref().is_some_and(|doc| doc.path == path)
    }

    /// The preview to draw in the Scene view, made for the world as it is
    /// (`revision` is the edit history's) at the playhead. `None` while
    /// it's off or nothing is open.
    pub fn preview(
        &mut self,
        world: &World,
        assets: &Assets,
        revision: (u64, bool),
    ) -> Option<&Preview> {
        let Some((file, time, playing)) = self
            .doc
            .as_ref()
            .filter(|_| self.preview_on)
            .map(|doc| (doc.current(), doc.keys.playhead, doc.keys.playing))
        else {
            self.stop_preview();
            return None;
        };
        let made_from = (revision, file.to_json());
        if self
            .preview
            .as_ref()
            .is_none_or(|p| p.made_from != made_from)
        {
            self.stop_preview();
            match SequenceRun::new(file) {
                Ok(run) => {
                    self.preview = Some(Preview {
                        made_from,
                        world: world.clone(),
                        run,
                        animators: Animators::new(),
                        shown_at: None,
                        camera: None,
                    });
                }
                Err(why) => {
                    self.errors
                        .push(format!("The sequence can't play: {}", why.0));
                    self.preview_on = false;
                    return None;
                }
            }
        }
        let preview = self.preview.as_mut().expect("made above");
        if preview.shown_at != Some(time) {
            preview.shown_at = Some(time);
            let applied = show_at(preview, assets, time);
            // Sounds only while it plays, not while scrubbing.
            let cues = if playing {
                applied.sounds
            } else {
                preview.run.stop_sounds()
            };
            let actions = axcenity_script::sequence_sound_actions(&mut preview.world, cues);
            let listener = match applied.camera {
                Some(shot) => Listener::from_view(shot.position, shot.orientation),
                None => Listener::default(),
            };
            let update = self.audio.update(actions, &preview.world, assets, listener);
            self.errors
                .extend(update.errors.into_iter().map(|e| format!("Audio: {e}")));
        }
        self.preview.as_ref()
    }

    /// The preview as last made, while it's on.
    pub fn shown(&self) -> Option<&Preview> {
        self.preview
            .as_ref()
            .filter(|_| self.preview_on && self.doc.is_some())
    }

    /// Stops the preview's Sounds and forgets it.
    fn stop_preview(&mut self) {
        if let Some(mut preview) = self.preview.take() {
            let cues = preview.run.finish(&mut preview.world);
            let actions = axcenity_script::sequence_sound_actions(&mut preview.world, cues);
            if !actions.is_empty() {
                let assets = Assets::default();
                let _ = self
                    .audio
                    .update(actions, &preview.world, &assets, Listener::default());
            }
        }
    }

    /// Shows the tab. Returns what it asks the editor for, and whether the
    /// open file was changed (to write once the mouse is let go).
    pub fn show(&mut self, ui: &mut egui::Ui, scene: &SequencerScene) -> Vec<SequencerAction> {
        let mut actions = Vec::new();
        // The transport's keys first, before any widget of the tab's can
        // take Space or an arrow.
        if let Some(doc) = &mut self.doc
            && ui.rect_contains_pointer(ui.max_rect())
            && transport::keys_free(ui.ctx())
        {
            for command in ui.input_mut(transport::keys) {
                transport::apply(&mut doc.keys, command);
            }
        }
        let dt =
            f64::from(ui.input(|input| input.unstable_dt)).min(MAX_FRAME_SECONDS) * self.rate.0;
        if let Some(doc) = &mut self.doc {
            doc.tick(dt);
            if doc.keys.playing {
                ui.ctx().request_repaint();
            }
        }
        crate::responsive::toolbar(ui, |ui| {
            let shown = self
                .doc
                .as_ref()
                .map_or("Open a sequence…".to_owned(), |doc| {
                    sequence_name(&doc.path).to_owned()
                });
            egui::ComboBox::from_id_salt("sequencer file")
                .selected_text(theme::with_icon(icons::FILM_SLATE, &shown))
                .show_ui(ui, |ui| {
                    if scene.files.is_empty() {
                        ui.weak("No sequences yet");
                    }
                    for path in scene.files {
                        if ui
                            .selectable_label(self.is_open(path), sequence_name(path))
                            .clicked()
                        {
                            actions.push(SequencerAction::Open(path.clone()));
                        }
                    }
                });
            ui.add(
                egui::TextEdit::singleline(&mut self.new_name)
                    .hint_text("New name")
                    .desired_width(90.0),
            );
            let name = self.new_name.trim().to_owned();
            if ui
                .add_enabled(
                    scene.editable && !name.is_empty(),
                    egui::Button::new(icons::PLUS),
                )
                .on_hover_text("Make a new sequence with this name")
                .clicked()
            {
                actions.push(SequencerAction::New(name));
                self.new_name.clear();
            }
        });
        let Some(doc) = &mut self.doc else {
            ui.weak("Open a sequence, or name a new one, to lay out a cutscene.");
            return actions;
        };
        // What's picked is keyed as the preview shows it.
        let shown = self.preview.as_ref().map_or(scene.world, |p| &p.world);
        crate::responsive::toolbar(ui, |ui| {
            let mut duration = doc.duration();
            ui.label("Length");
            if ui
                .add(
                    egui::DragValue::new(&mut duration)
                        .speed(0.1)
                        .range(0.1..=3600.0)
                        .suffix(" s"),
                )
                .changed()
            {
                doc.set_duration(duration);
            }
            ui.toggle_value(&mut self.preview_on, format!("{} Preview", icons::EYE))
                .on_hover_text("Show the sequence at the playhead in the Scene view, without Play");
            if ui
                .add_enabled(
                    scene.editable,
                    egui::Button::new(format!("{} Insert", icons::PLUS)),
                )
                .on_hover_text("Put a Sequence that plays this file in the Workspace")
                .clicked()
            {
                actions.push(SequencerAction::Insert(doc.path.clone()));
            }
        });

        if let Some(command) = transport::bar(ui, &doc.keys, &mut self.rate) {
            transport::apply(&mut doc.keys, command);
        }

        // The timeline: playhead, events, and the end as the duration.
        let events: Vec<(f64, String)> = doc
            .keys
            .file
            .events
            .iter()
            .map(|e| (e.time, e.name.clone()))
            .collect();
        let event_refs: Vec<(f64, &str)> = events.iter().map(|(t, n)| (*t, n.as_str())).collect();
        let change = Timeline {
            take_seconds: doc.duration(),
            frame_rate: doc.keys.fps as f32,
            start: 0.0,
            end: doc.duration(),
            playhead: doc.keys.playhead,
            events: &event_refs,
        }
        .show(ui);
        if let Some(at) = change.playhead {
            doc.keys.playhead = at;
            doc.keys.playing = false;
        }
        if let Some((index, at)) = change.moved_event
            && let Some(event) = doc.keys.file.events.get_mut(index)
        {
            event.time = at;
        }
        if let Some(index) = change.removed_event
            && index < doc.keys.file.events.len()
        {
            doc.keys.file.events.remove(index);
        }
        let strip = ui.min_rect();
        let left = strip.left() + 6.0;
        let width = (ui.available_width().max(120.0) - 12.0).max(1.0);

        sections_view(ui, doc, left, width);
        add_menu(ui, doc, scene);
        ui.separator();
        section_inspector(ui, doc, scene);
        ui.separator();

        // The keys: property tracks and the sequence's own camera.
        crate::responsive::toolbar(ui, |ui| {
            if ui
                .button(format!("{} Key camera", icons::VIDEO_CAMERA))
                .on_hover_text(
                    "Key the sequence's own camera at the playhead where the Scene view's camera is",
                )
                .clicked()
            {
                let (position, orientation) = scene.camera;
                doc.keys.key_focus(&camera_focus(position, orientation));
            }
            ui.add_space(6.0);
            let event = format!("Event{}", doc.keys.file.events.len() + 1);
            if ui
                .button(format!("{} Event", icons::FLAG))
                .on_hover_text("Add an event at the playhead; scripts hear it as EventReached")
                .clicked()
            {
                let time = doc.keys.playhead;
                doc.keys.file.events.push(ClipEvent { time, name: event });
                doc.keys
                    .file
                    .events
                    .sort_by(|a, b| a.time.total_cmp(&b.time));
            }
        });
        // The preview's Scene view has no tools: say how to change a pose.
        if self.preview_on && scene.editable {
            ui.weak(theme::with_icon(
                icons::EYE,
                "Previewing: the Scene view shows the sequence and can't be edited. Turn \
                 Preview off to move things, then key them (K).",
            ));
        }
        let focus = scene
            .picked
            .as_ref()
            .and_then(|picked| picked_focus(shown, picked, doc.by_tag));
        let key = doc.keys.keys(ui, focus.as_ref());
        let hovered = ui.ui_contains_pointer();
        if (key || (hovered && doc.keys.shortcuts(ui)))
            && let Some(focus) = &focus
        {
            doc.keys.key_focus(focus);
        }
        actions
    }

    /// Writes the open sequence if it changed and the mouse is up: the
    /// path and the text before and after.
    pub fn flush(
        &mut self,
        ctx: &egui::Context,
        project: &Path,
    ) -> Option<std::io::Result<(String, String, String)>> {
        if ctx.input(|input| input.pointer.any_down()) {
            return None;
        }
        let doc = self.doc.as_mut()?;
        match doc.write(project) {
            Ok(Some((before, after))) => Some(Ok((doc.path.clone(), before, after))),
            Ok(None) => None,
            Err(err) => Some(Err(err)),
        }
    }
}

/// Puts the preview's world where the sequence has it at `time`, posed.
pub fn show_at(
    preview: &mut Preview,
    assets: &Assets,
    time: f64,
) -> axcenity_anim::sequence::Applied {
    let applied = preview.run.apply(&mut preview.world, time);
    preview.animators.update(&mut preview.world, assets, 0.0);
    preview.camera = applied.camera;
    applied
}

/// What keying the sequence's own camera keys: the Scene view's camera.
fn camera_focus(position: Vector3, orientation: Vector3) -> Focus {
    let value = |property, value: Vec<f32>| EvaluatedProperty {
        target: CAMERA_TARGET.to_owned(),
        property,
        value,
    };
    Focus {
        target: CAMERA_TARGET.to_owned(),
        name: "Camera".to_owned(),
        values: vec![
            value(
                PropertyName::Position,
                vec![position.x, position.y, position.z],
            ),
            value(
                PropertyName::Orientation,
                vec![orientation.x, orientation.y, 0.0],
            ),
            value(PropertyName::FieldOfView, vec![50.0]),
        ],
        transform: vec![PropertyName::Position, PropertyName::Orientation],
    }
}

/// The binding a new track or section of `picked` gets.
fn binding_of(picked: &Picked, by_tag: bool) -> String {
    match picked.tags.first() {
        Some(tag) if by_tag => format!("{}{tag}", axcenity_anim::sequence::TAG_PREFIX),
        _ => picked.path.clone(),
    }
}

/// What can be keyed on `picked`, as it is in `world` now.
fn picked_focus(world: &World, picked: &Picked, by_tag: bool) -> Option<Focus> {
    let target = binding_of(picked, by_tag);
    let applied = axcenity_anim::Applied::default();
    let root = world.root();
    let path = axcenity_anim::target_path(world, root, picked.id)?;
    let values: Vec<EvaluatedProperty> = PropertyName::ALL
        .into_iter()
        .filter(|p| *p != PropertyName::Bone)
        .filter_map(|property| {
            let value = axcenity_anim::current_numbers(world, root, &path, property, &applied)?;
            property
                .widths()
                .contains(&value.len())
                .then(|| EvaluatedProperty {
                    target: target.clone(),
                    property,
                    value,
                })
        })
        .collect();
    if values.is_empty() {
        return None;
    }
    let ui = world.get(picked.id).is_some_and(|i| i.class().is_gui());
    let transform = if ui {
        vec![PropertyName::Position, PropertyName::Size]
    } else {
        vec![PropertyName::Position, PropertyName::Orientation]
    };
    Some(Focus {
        target,
        name: picked.name.clone(),
        values,
        transform,
    })
}

/// The rows of sections, each laid over the timeline's seconds.
fn sections_view(ui: &mut egui::Ui, doc: &mut SequenceDoc, left: f32, width: f32) {
    let palette = theme::palette(ui.ctx());
    let duration = doc.duration().max(1e-6);
    let x_of = |seconds: f64| left + (seconds / duration) as f32 * width;
    let seconds_at = |x: f32| f64::from((x - left) / width) * duration;
    let snap = |seconds: f64, fps: f64| ((seconds * fps).round() / fps).clamp(0.0, duration);
    let fps = doc.keys.fps.max(1.0);
    for row in Row::ALL {
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(ui.available_width().max(120.0), ROW),
            egui::Sense::click_and_drag(),
        );
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect.shrink2(egui::vec2(0.0, 1.0)), 3.0, palette.inset);
        let sections = doc.sections(row);
        let mut hit = None;
        for (index, (start, end, label)) in sections.iter().enumerate() {
            let section = Section { row, index };
            let x0 = x_of(*start);
            let x1 = x_of(*end).max(x0 + 4.0);
            let bar = egui::Rect::from_min_max(
                egui::pos2(x0, rect.top() + 3.0),
                egui::pos2(x1, rect.bottom() - 3.0),
            );
            let chosen = doc.selected == Some(section);
            let fill = if chosen {
                palette.accent_solid
            } else {
                palette.control
            };
            painter.rect_filled(bar, 3.0, fill);
            painter.text(
                bar.left_center() + egui::vec2(4.0, 0.0),
                egui::Align2::LEFT_CENTER,
                label,
                egui::FontId::proportional(11.0),
                if chosen {
                    palette.text_strong
                } else {
                    palette.text
                },
            );
            if let Some(pos) = response.interact_pointer_pos()
                && bar.expand2(egui::vec2(EDGE / 2.0, 0.0)).contains(pos)
            {
                hit = Some((section, bar, pos));
            }
        }
        // The row's name, on a chip at its right end.
        let name = painter.layout_no_wrap(
            theme::with_icon(row.icon(), row.label()),
            egui::FontId::proportional(10.0),
            palette.text_weak,
        );
        let chip = egui::Rect::from_min_size(
            egui::pos2(
                rect.right() - name.size().x - 10.0,
                rect.center().y - name.size().y / 2.0 - 1.0,
            ),
            name.size() + egui::vec2(8.0, 2.0),
        );
        painter.rect_filled(chip, 3.0, palette.inset);
        painter.galley(chip.min + egui::vec2(4.0, 1.0), name, palette.text_weak);
        // The playhead runs through the rows too.
        let x = x_of(doc.keys.playhead);
        painter.line_segment(
            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
            egui::Stroke::new(1.0_f32, palette.accent),
        );
        if response.drag_started()
            && let Some((section, bar, pos)) = hit
        {
            let grab = if row != Row::Camera && (pos.x - bar.left()).abs() < EDGE {
                Grab::Start
            } else if row != Row::Camera && (pos.x - bar.right()).abs() < EDGE {
                Grab::End
            } else {
                let (start, end, _) = &sections[section.index];
                Grab::Body {
                    start: *start,
                    end: *end,
                }
            };
            doc.selected = Some(section);
            doc.drag = Some((section, grab, seconds_at(pos.x)));
        }
        if response.dragged()
            && let Some((section, grab, from)) = doc.drag
            && section.row == row
            && let Some(pos) = response.interact_pointer_pos()
        {
            let (start, end, _) = doc.sections(row)[section.index].clone();
            let at = snap(seconds_at(pos.x), fps);
            match grab {
                Grab::Body { start: s0, end: e0 } => {
                    let shift = (at - snap(from, fps)).clamp(-s0, duration - e0);
                    doc.set_span(section, s0 + shift, e0 + shift);
                }
                Grab::Start => doc.set_span(section, at.min(end), end),
                Grab::End => doc.set_span(section, start, at.max(start)),
            }
        }
        if response.drag_stopped() {
            doc.drag = None;
        }
        if response.clicked() {
            doc.selected = hit.map(|(section, _, _)| section);
        }
    }
}

/// Add ▸ a section of each row at the playhead, for what's picked.
fn add_menu(ui: &mut egui::Ui, doc: &mut SequenceDoc, scene: &SequencerScene) {
    crate::responsive::toolbar(ui, |ui| {
        let binding = scene
            .picked
            .as_ref()
            .map(|picked| binding_of(picked, doc.by_tag));
        ui.menu_button(format!("{} Add section", icons::PLUS), |ui| {
            let picked = binding.clone().unwrap_or_default();
            let hint = "Select what it binds in the Explorer first";
            if ui
                .button("Camera cut (the selected Camera)")
                .on_hover_text("From the playhead, the view is the selected Camera's")
                .clicked()
            {
                doc.add(Row::Camera, picked.clone(), "");
                ui.close();
            }
            if ui
                .button("Camera cut (own keys)")
                .on_hover_text("From the playhead, the view follows Key camera's keys")
                .clicked()
            {
                doc.add(Row::Camera, String::new(), "");
                ui.close();
            }
            ui.add_enabled_ui(binding.is_some(), |ui| {
                ui.menu_button("Animation", |ui| {
                    if scene.clips.is_empty() {
                        ui.weak("No clips in the project");
                    }
                    for clip in scene.clips {
                        if ui.button(axcenity_anim::files::clip_name(clip)).clicked() {
                            doc.add(Row::Animation, picked.clone(), clip);
                            ui.close();
                        }
                    }
                })
                .response
                .on_disabled_hover_text(hint);
                if ui
                    .button("Audio (the selected Sound)")
                    .on_disabled_hover_text(hint)
                    .clicked()
                {
                    doc.add(Row::Audio, picked.clone(), "");
                    ui.close();
                }
            });
            if ui.button("Fade").clicked() {
                doc.add(Row::Fade, String::new(), "");
                ui.close();
            }
            if ui.button("Letterbox").clicked() {
                doc.add(Row::Letterbox, String::new(), "");
                ui.close();
            }
            if ui.button("Subtitle").clicked() {
                doc.add(Row::Subtitle, String::new(), "");
                ui.close();
            }
            ui.menu_button("Level", |ui| {
                if scene.levels.is_empty() {
                    ui.weak("No other levels in the project");
                }
                for level in scene.levels {
                    if ui.button(level).clicked() {
                        doc.add(Row::Level, String::new(), level);
                        ui.close();
                    }
                }
            });
        });
        let can_tag = scene.picked.as_ref().is_some_and(|p| !p.tags.is_empty());
        ui.add_enabled(can_tag, egui::Checkbox::new(&mut doc.by_tag, "Bind by tag"))
            .on_hover_text("New tracks find the selected Instance by its first tag, so prefab copies can be bound")
            .on_disabled_hover_text("The selected Instance has no tags");
        match &binding {
            Some(binding) => ui.weak(format!("Binds {binding}")),
            None => ui.weak("Select something in the Explorer to bind it"),
        };
    });
}

/// The selected section's settings.
fn section_inspector(ui: &mut egui::Ui, doc: &mut SequenceDoc, scene: &SequencerScene) {
    let Some(section) = doc.selected else {
        ui.weak("Pick a section to set it, or drag it to move it.");
        return;
    };
    let picked = scene.picked.as_ref().map(|p| binding_of(p, doc.by_tag));
    let mut delete = false;
    let binding = |ui: &mut egui::Ui, label: &str, value: &mut String| {
        ui.label(label);
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(value).desired_width(150.0));
            if let Some(picked) = &picked
                && ui
                    .small_button(icons::CURSOR_CLICK)
                    .on_hover_text(format!("Bind {picked}"))
                    .clicked()
            {
                *value = picked.clone();
            }
        });
        ui.end_row();
    };
    let seconds = |ui: &mut egui::Ui, label: &str, value: &mut f64| {
        ui.label(label);
        ui.add(
            egui::DragValue::new(value)
                .speed(0.05)
                .range(0.0..=3600.0)
                .suffix(" s"),
        );
        ui.end_row();
    };
    let share = |ui: &mut egui::Ui, label: &str, value: &mut f32| {
        ui.label(label);
        ui.add(egui::Slider::new(value, 0.0..=1.0));
        ui.end_row();
    };
    let span = doc.sections(section.row).get(section.index).cloned();
    let Some((mut start, mut end, _)) = span else {
        doc.selected = None;
        return;
    };
    ui.strong(section.row.label());
    crate::responsive::form(ui, "sequence section", |ui| {
        seconds(
            ui,
            if section.row == Row::Camera {
                "Time"
            } else {
                "Start"
            },
            &mut start,
        );
        if section.row != Row::Camera {
            seconds(ui, "End", &mut end);
        }
        let f = &mut doc.file;
        let i = section.index;
        match section.row {
            Row::Camera => {
                if let Some(cut) = f.camera_cuts.get_mut(i) {
                    binding(ui, "Camera", &mut cut.camera);
                    ui.label("");
                    ui.weak("Empty: the sequence's own camera keys");
                    ui.end_row();
                }
            }
            Row::Animation => {
                if let Some(s) = f.animations.get_mut(i) {
                    binding(ui, "Animator", &mut s.target);
                    ui.label("Clip");
                    egui::ComboBox::from_id_salt("sequence clip")
                        .selected_text(axcenity_anim::files::clip_name(&s.clip).to_owned())
                        .show_ui(ui, |ui| {
                            for clip in scene.clips {
                                ui.selectable_value(
                                    &mut s.clip,
                                    clip.clone(),
                                    axcenity_anim::files::clip_name(clip),
                                );
                            }
                        });
                    ui.end_row();
                    seconds(ui, "Blend in", &mut s.blend_in);
                    seconds(ui, "Blend out", &mut s.blend_out);
                }
            }
            Row::Audio => {
                if let Some(s) = f.audio.get_mut(i) {
                    binding(ui, "Sound", &mut s.sound);
                }
            }
            Row::Fade => {
                if let Some(s) = f.fades.get_mut(i) {
                    share(ui, "From", &mut s.from);
                    share(ui, "To", &mut s.to);
                    ui.label("Color");
                    ui.color_edit_button_rgb(&mut s.color);
                    ui.end_row();
                }
            }
            Row::Letterbox => {
                if let Some(s) = f.letterbox.get_mut(i) {
                    share(ui, "Bar height", &mut s.size);
                    seconds(ui, "Slide", &mut s.ramp);
                }
            }
            Row::Subtitle => {
                if let Some(s) = f.subtitles.get_mut(i) {
                    ui.label("String key");
                    ui.add(egui::TextEdit::singleline(&mut s.key).desired_width(150.0))
                        .on_hover_text("A key in the string table (File > Strings): shown in the player's language");
                    ui.end_row();
                    ui.label("Text");
                    ui.add(egui::TextEdit::singleline(&mut s.text).desired_width(150.0))
                        .on_hover_text("Shown when the string table hasn't got the key");
                    ui.end_row();
                }
            }
            Row::Level => {
                if let Some(s) = f.levels.get_mut(i) {
                    ui.label("Level");
                    egui::ComboBox::from_id_salt("sequence level")
                        .selected_text(s.level.clone())
                        .show_ui(ui, |ui| {
                            for level in scene.levels {
                                ui.selectable_value(&mut s.level, level.clone(), level);
                            }
                        });
                    ui.end_row();
                }
            }
        }
    });
    if ui
        .button(format!("{} Delete section", icons::TRASH))
        .clicked()
    {
        delete = true;
    }
    if delete {
        doc.delete(section);
    } else {
        doc.set_span(section, start, end);
    }
}

#[cfg(test)]
mod tests;
