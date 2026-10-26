//! A clip open in the Animation tab: its toolbar controls, settings,
//! timeline and bones, and the pose its preview shows.
//!
//! The bones its take moves are the take's (locked); the others can be
//! posed in the preview and keyed as `Bone` tracks on the clip, which play
//! alongside the take. Convert to keys bakes the take into bone keys.

use std::path::Path;
use std::sync::Arc;

use axcenity_anim::{
    Clip, ClipEvent, ClipFile, EvaluatedProperty, PropertyName, PropertyTrack, TakeLink,
    bone_from_numbers, bone_numbers, bone_paths, find_bone, take_bones,
};
use axcenity_assets::{Assets, BoneTransform, Model, Rig, Take};
use eframe::egui;

use super::posing::Folds;
use super::property_editor::PropertyDoc;
use super::timeline::Timeline;
use crate::theme::icons;

/// A linked clip as this editor shows it: part of a take, played at a
/// speed, with its events in seconds of the take from the clip's start.
/// It turns into the one clip shape ([`ClipFile`]) to save, keeping the
/// clip's own tracks as they were.
#[derive(Clone, Debug, PartialEq)]
pub struct LinkedClip {
    pub source: String,
    pub take: String,
    /// Where it starts and ends in the take, in seconds (`start <= end`).
    pub start: f64,
    pub end: f64,
    /// Negative plays the take backwards.
    pub speed: f64,
    pub looping: bool,
    pub root_motion: bool,
    pub events: Vec<ClipEvent>,
    /// The rest of the clip (its tracks), kept as it was.
    rest: ClipFile,
}

impl LinkedClip {
    /// `file` as a part of its take. A clip with no link gets an empty one.
    pub fn of(file: ClipFile) -> Self {
        let link = file.link.clone().unwrap_or(TakeLink {
            source: String::new(),
            take: String::new(),
            start: 0.0,
            end: file.duration,
            root_motion: false,
        });
        let speed = file.speed();
        let backwards = link.end < link.start;
        let (start, end) = if backwards {
            (link.end, link.start)
        } else {
            (link.start, link.end)
        };
        let length = end - start;
        let events = file
            .events
            .iter()
            .map(|event| {
                let into = event.time * speed.abs();
                ClipEvent {
                    time: if backwards { length - into } else { into }.clamp(0.0, length),
                    name: event.name.clone(),
                }
            })
            .collect();
        Self {
            source: link.source,
            take: link.take,
            start,
            end,
            speed,
            looping: file.looping,
            root_motion: link.root_motion,
            events,
            rest: file,
        }
    }

    /// Its length in seconds of the take (before `speed`), never negative.
    pub fn length(&self) -> f64 {
        (self.end - self.start).max(0.0)
    }

    /// It in the one clip shape.
    pub fn to_file(&self) -> ClipFile {
        let speed = self.speed.abs().max(1e-3);
        let backwards = self.speed < 0.0;
        let length = self.length();
        let mut file = self.rest.clone();
        file.duration = length / speed;
        file.looping = self.looping;
        file.link = Some(TakeLink {
            source: self.source.clone(),
            take: self.take.clone(),
            start: if backwards { self.end } else { self.start },
            end: if backwards { self.start } else { self.end },
            root_motion: self.root_motion,
        });
        file.events = self
            .events
            .iter()
            .map(|event| {
                let into = if backwards {
                    length - event.time
                } else {
                    event.time
                };
                ClipEvent {
                    time: into.clamp(0.0, length) / speed,
                    name: event.name.clone(),
                }
            })
            .collect();
        file
    }
}

/// A clip being looked at and changed.
pub struct ClipDoc {
    pub path: String,
    pub file: LinkedClip,
    /// As last written, to tell when it needs writing again.
    written: LinkedClip,
    /// As it was opened, for Revert.
    opened: LinkedClip,
    /// The takes of its model file, or why they couldn't be read.
    takes: Result<Arc<Vec<Take>>, String>,
    /// Seconds into the clip (0 is its start), at its own speed.
    pub playhead: f64,
    pub playing: bool,
    /// The model the preview shows it on (a `MeshId`).
    pub preview_model: Option<String>,
    /// The clip bound to the preview model, for the file as it was then.
    bound: Option<(u64, LinkedClip, Result<Clip, String>)>,
    /// A new event's name, typed before Add event.
    new_event: String,
    /// Bones turned in the preview that aren't keys (by bone path), and
    /// the playhead they were turned at (moving it drops them).
    pending: Vec<(String, Vec<f32>)>,
    pending_at: f64,
    /// The take's motion as the keys Convert to keys would make, shown
    /// read-only under the timeline.
    imported: PropertyDoc,
    /// The preview model and the clip those keys were worked out for, and
    /// the keys or why there aren't any.
    imported_for: Option<(Option<String>, LinkedClip, Result<ClipFile, String>)>,
}

/// The preview model's skeleton at the playhead.
pub struct Posing {
    pub model: Arc<Model>,
    pub pose: Vec<BoneTransform>,
    /// One per bone: moved by the take, so it can't be keyed.
    pub locked: Vec<bool>,
}

impl Posing {
    pub fn rig(&self) -> &Rig {
        self.model.rig.as_deref().expect("a posed model has a rig")
    }
}

impl ClipDoc {
    pub fn open(path: String, file: ClipFile, assets: &Assets, models: &[String]) -> Self {
        let file = LinkedClip::of(file);
        let takes = assets
            .takes(&file.source)
            .ok_or_else(|| format!("its model file `{}` can't be read", file.source));
        let mut doc = Self {
            imported: PropertyDoc::read_only(path.clone(), ClipFile::new(file.length())),
            imported_for: None,
            path,
            written: file.clone(),
            opened: file.clone(),
            file,
            takes,
            playhead: 0.0,
            playing: true,
            preview_model: None,
            bound: None,
            new_event: "Footstep".to_owned(),
            pending: Vec::new(),
            pending_at: 0.0,
        };
        doc.preview_model = doc.default_preview_model(assets, models);
        doc
    }

    /// The clip's own model if it has one to draw with a skeleton, or the
    /// first project model its take moves.
    fn default_preview_model(&self, assets: &Assets, models: &[String]) -> Option<String> {
        std::iter::once(self.file.source.clone())
            .chain(models.iter().cloned())
            .find(|mesh_id| self.fits(assets, mesh_id))
    }

    /// Whether the clip's take moves the skeleton of the model `mesh_id`.
    fn fits(&self, assets: &Assets, mesh_id: &str) -> bool {
        let Ok(takes) = &self.takes else {
            return false;
        };
        let Some(model) = assets.model(mesh_id) else {
            return false;
        };
        let Some(rig) = &model.rig else {
            return false;
        };
        Clip::bind(self.file.to_file(), Some(takes.clone()), Some(rig)).is_ok()
    }

    /// Previews on the model `mesh_id` (the object being animated) if the
    /// clip's take fits its skeleton.
    pub fn prefer_model(&mut self, assets: &Assets, mesh_id: &str) {
        if self.preview_model.as_deref() != Some(mesh_id) && self.fits(assets, mesh_id) {
            self.preview_model = Some(mesh_id.to_owned());
        }
    }

    /// True if it has changes not yet written.
    pub fn unwritten(&self) -> bool {
        self.file != self.written
    }

    /// Writes it if it changed.
    pub fn write(&mut self, project: &Path) -> std::io::Result<bool> {
        if !self.unwritten() {
            return Ok(false);
        }
        super::store::save_clip(project, &self.path, &self.file.to_file())?;
        self.written = self.file.clone();
        Ok(true)
    }

    fn take(&self) -> Option<&Take> {
        self.takes
            .as_ref()
            .ok()?
            .iter()
            .find(|take| take.name == self.file.take)
    }

    fn frame_rate(&self) -> f32 {
        self.take().map_or(30.0, |take| take.frame_rate.max(1.0))
    }

    fn frames(&self, seconds: f64) -> u32 {
        (seconds * f64::from(self.frame_rate())).round().max(0.0) as u32
    }

    fn seconds(&self, frames: u32) -> f64 {
        f64::from(frames) / f64::from(self.frame_rate())
    }

    /// Moves the playhead on by `dt` seconds while playing.
    pub fn tick(&mut self, dt: f64) {
        let length = self.file.length() / self.file.speed.abs().max(1e-3);
        if !self.playing || length <= 0.0 {
            return;
        }
        self.playhead += dt;
        if self.playhead >= length {
            if self.file.looping {
                self.playhead = self.playhead.rem_euclid(length);
            } else {
                self.playhead = length;
                self.playing = false;
            }
        }
    }

    /// How far through one pass the playhead is, from 0 to (just under) 1.
    fn phase(&self) -> f64 {
        let length = self.file.length() / self.file.speed.abs().max(1e-3);
        if length <= 0.0 {
            0.0
        } else {
            (self.playhead / length).clamp(0.0, 0.999_999)
        }
    }

    /// The preview model's skeleton at the playhead, with bones turned but
    /// not yet keyed on top, or why there isn't one.
    pub fn pose(&mut self, assets: &Assets) -> Result<Posing, String> {
        if self.playing || self.playhead != self.pending_at {
            self.pending.clear();
        }
        let mesh_id = self
            .preview_model
            .clone()
            .ok_or("no model in the project has this clip's skeleton")?;
        let model = assets
            .model(&mesh_id)
            .ok_or_else(|| format!("`{mesh_id}` can't be loaded"))?;
        let rig = model
            .rig
            .as_ref()
            .ok_or_else(|| format!("`{mesh_id}` has no skeleton"))?;
        let takes = self.takes.clone()?;
        let stale = self
            .bound
            .as_ref()
            .is_none_or(|(key, file, _)| *key != model.key() || *file != self.file);
        if stale {
            let bound = Clip::bind(self.file.to_file(), Some(takes), Some(rig));
            self.bound = Some((model.key(), self.file.clone(), bound));
        }
        let Some((_, _, bound)) = &self.bound else {
            return Err("the clip isn't ready".to_owned());
        };
        let bound = bound.as_ref().map_err(Clone::clone)?;
        let mut pose = rig.rest_pose();
        bound.sample(self.phase(), rig, &mut pose);
        let paths = bone_paths(rig);
        for (path, numbers) in &self.pending {
            if let Some(bone) = find_bone(rig, &paths, path)
                && let (Some(transform), Some(slot)) =
                    (bone_from_numbers(numbers), pose.get_mut(bone))
            {
                *slot = transform;
            }
        }
        let locked = self.locked(rig);
        Ok(Posing {
            model,
            pose,
            locked,
        })
    }

    /// One per bone of `rig`: whether the take moves it.
    pub fn locked(&self, rig: &Rig) -> Vec<bool> {
        self.take().map_or_else(
            || vec![false; rig.bones.len()],
            |take| take_bones(take, rig),
        )
    }

    /// The clip's own tracks (its bone keys), which play alongside the take.
    pub fn tracks(&self) -> &[PropertyTrack] {
        &self.file.rest.tracks
    }

    /// The playhead on the nearest frame of the take, in seconds of the clip.
    fn key_time(&self) -> f64 {
        let frames_a_second = f64::from(self.frame_rate()) * self.file.speed.abs().max(1e-3);
        let length = self.file.length() / self.file.speed.abs().max(1e-3);
        ((self.playhead * frames_a_second).round() / frames_a_second).clamp(0.0, length)
    }

    /// `bone` of `rig` turned or moved to `transform` in the preview: keyed
    /// at the playhead with `record`, otherwise shown until the playhead
    /// moves. A bone the take moves can't be keyed.
    pub fn pose_bone(
        &mut self,
        rig: &Rig,
        bone: usize,
        transform: &BoneTransform,
        record: bool,
    ) -> Result<(), String> {
        let path = self.unlocked_path(rig, bone)?;
        self.playing = false;
        let numbers = bone_numbers(transform);
        self.pending.retain(|(p, _)| *p != path);
        if record {
            self.key_bone(path, numbers);
        } else {
            self.pending_at = self.playhead;
            self.pending.push((path, numbers));
        }
        Ok(())
    }

    /// Key selected (K): keys `bone` where it is in the preview now.
    pub fn key_selected(&mut self, assets: &Assets, bone: usize) -> Result<(), String> {
        let posing = self.pose(assets)?;
        let path = self.unlocked_path(posing.rig(), bone)?;
        let transform = posing.pose.get(bone).ok_or("there's no such bone")?;
        self.pending.retain(|(p, _)| *p != path);
        self.key_bone(path, bone_numbers(transform));
        Ok(())
    }

    /// `bone`'s path, if it can be keyed, or why not.
    fn unlocked_path(&self, rig: &Rig, bone: usize) -> Result<String, String> {
        let name = &rig.bones.get(bone).ok_or("there's no such bone")?.name;
        if self.locked(rig).get(bone).copied().unwrap_or(false) {
            return Err(format!(
                "{name} is moved by the take `{}`, so it can't be keyed. Convert to keys (⋯) to edit it.",
                self.file.take
            ));
        }
        bone_paths(rig)
            .get(bone)
            .filter(|path| !path.is_empty())
            .cloned()
            .ok_or_else(|| "that bone has no name to key it by".to_owned())
    }

    fn key_bone(&mut self, path: String, numbers: Vec<f32>) {
        let value = EvaluatedProperty {
            target: path,
            property: PropertyName::Bone,
            value: numbers,
        };
        let time = self.key_time();
        super::property_editor::set_key(&mut self.file.rest.tracks, &value, time);
    }

    /// How many keys each bone of `rig` has on the clip's own tracks.
    pub fn bone_keys(&self, rig: &Rig) -> Vec<usize> {
        let paths = bone_paths(rig);
        let mut keys = vec![0; rig.bones.len()];
        for track in self
            .tracks()
            .iter()
            .filter(|t| t.property == PropertyName::Bone)
        {
            if let Some(bone) = find_bone(rig, &paths, &track.target) {
                keys[bone] += track.keys.len();
            }
        }
        keys
    }

    /// Removes `bone`'s keys from the clip.
    fn remove_bone_keys(&mut self, rig: &Rig, bone: usize) {
        let paths = bone_paths(rig);
        self.file.rest.tracks.retain(|track| {
            track.property != PropertyName::Bone
                || find_bone(rig, &paths, &track.target) != Some(bone)
        });
    }

    /// The preview model, if it can be loaded and has a skeleton.
    fn rigged_preview(&self, assets: &Assets) -> Result<Arc<Model>, String> {
        let mesh_id = self
            .preview_model
            .as_deref()
            .ok_or("no model in the project has this clip's skeleton")?;
        let model = assets
            .model(mesh_id)
            .ok_or_else(|| format!("`{mesh_id}` can't be loaded"))?;
        if model.rig.is_none() {
            return Err(format!("`{mesh_id}` has no skeleton"));
        }
        Ok(model)
    }

    /// Convert to keys: the clip with its take baked into bone keys on the
    /// preview model's skeleton, and no link (see
    /// [`axcenity_anim::convert_to_keys`]).
    pub fn converted(&self, assets: &Assets) -> Result<ClipFile, String> {
        let model = self.rigged_preview(assets)?;
        let rig = model.rig.as_deref().expect("a rigged preview");
        axcenity_anim::convert_to_keys(&self.file.to_file(), self.takes.clone()?, rig)
    }

    /// The keys Convert to keys would make for the bones the take moves,
    /// without the clip's own tracks: the imported motion as keys.
    pub fn take_keys(&self, assets: &Assets) -> Result<ClipFile, String> {
        let model = self.rigged_preview(assets)?;
        let rig = model.rig.as_deref().expect("a rigged preview");
        let mut file =
            axcenity_anim::convert_to_keys(&self.file.to_file(), self.takes.clone()?, rig)?;
        let locked = self.locked(rig);
        let paths = bone_paths(rig);
        file.tracks.retain(|track| {
            track.property == PropertyName::Bone
                && find_bone(rig, &paths, &track.target).is_some_and(|bone| locked[bone])
        });
        Ok(file)
    }

    /// The read-only view of the take's keys, worked out again when the
    /// clip or the preview model changed. Whatever was done to its keys is
    /// put back.
    fn imported_view(&mut self, assets: &Assets) -> Result<&mut PropertyDoc, String> {
        let stale = self
            .imported_for
            .as_ref()
            .is_none_or(|(model, file, _)| *model != self.preview_model || *file != self.file);
        if stale {
            let keys = self.take_keys(assets);
            if let Ok(file) = &keys {
                self.imported.show_file(file.clone());
            }
            self.imported_for = Some((self.preview_model.clone(), self.file.clone(), keys));
        }
        let (_, _, keys) = self.imported_for.as_ref().expect("just worked out");
        let file = keys.as_ref().map_err(Clone::clone)?;
        if self.imported.file != *file {
            self.imported.file = file.clone();
        }
        Ok(&mut self.imported)
    }

    /// The imported keys as shown under the timeline, for tests.
    #[cfg(test)]
    pub fn imported_keys(&mut self, assets: &Assets) -> Result<&mut PropertyDoc, String> {
        self.imported_view(assets)
    }

    /// Shows the imported keys as curves (or the dope sheet), for tests.
    #[cfg(test)]
    pub fn show_imported_curves(&mut self, on: bool) {
        self.imported.show_curves = on;
    }

    /// Under the timeline: the take's motion as the keys Convert to keys
    /// would make, one row per bone it moves, in the dope sheet or Curves.
    /// They can be selected but not changed. Returns true if Convert to
    /// keys was pressed.
    pub fn imported(&mut self, ui: &mut egui::Ui, assets: &Assets) -> bool {
        let mut convert = false;
        ui.horizontal_wrapped(|ui| {
            ui.weak(format!(
                "{} Imported motion: Convert to keys to edit it",
                icons::LOCK
            ));
            if ui
                .button(format!("{} Convert to keys", icons::KEY))
                .on_hover_text(
                    "Bake the take into bone keys you can edit, and unlink the clip (one undo step)",
                )
                .clicked()
            {
                convert = true;
            }
        });
        let fps = f64::from(self.frame_rate()) * self.file.speed.abs().max(1e-3);
        let playhead = self.playhead;
        let moved = match self.imported_view(assets) {
            Ok(view) => {
                view.fps = fps;
                view.playing = false;
                view.playhead = playhead;
                view.keys(ui, None);
                (view.playhead != playhead).then_some(view.playhead)
            }
            Err(problem) => {
                ui.weak(format!("Its keys can't be shown: {problem}"));
                None
            }
        };
        if let Some(at) = moved {
            self.playhead = at;
            self.playing = false;
        }
        convert
    }

    /// Whether root motion found a bone to move the model by.
    fn root_motion_found(&self) -> Option<bool> {
        let (_, _, bound) = self.bound.as_ref()?;
        bound.as_ref().ok().map(Clip::has_root_motion)
    }

    /// The toolbar's part: play/pause, back to the start, Loop, Speed and
    /// the model the preview shows it on (from `models`).
    pub fn controls(&mut self, ui: &mut egui::Ui, models: &[String]) {
        let speed = self.file.speed.abs().max(1e-3);
        let (icon, hover) = if self.playing {
            (icons::PAUSE, "Pause")
        } else {
            (icons::PLAY, "Play")
        };
        if ui.button(icon).on_hover_text(hover).clicked() {
            if !self.playing && self.playhead * speed >= self.file.length() {
                self.playhead = 0.0;
            }
            self.playing = !self.playing;
        }
        if ui
            .button(icons::SKIP_BACK)
            .on_hover_text("Back to the start")
            .clicked()
        {
            self.playhead = 0.0;
        }
        ui.checkbox(&mut self.file.looping, "Loop")
            .on_hover_text("On, it plays again from the start; off, it holds its last frame");
        ui.add(
            egui::DragValue::new(&mut self.file.speed)
                .range(-10.0..=10.0)
                .speed(0.01)
                .prefix("Speed ")
                .suffix("×"),
        )
        .on_hover_text("2 plays it twice as fast; below 0 plays it backwards");
        let shown = self
            .preview_model
            .as_deref()
            .map_or("(no model)", |path| path.rsplit('/').next().unwrap_or(path));
        let mut picked = self.preview_model.clone();
        crate::responsive::room_for(ui, super::MODEL_PICKER_ROOM);
        egui::ComboBox::from_id_salt("clip preview model")
            .selected_text(format!("{} Preview on {shown}", icons::CUBE))
            .width(160.0)
            .show_ui(ui, |ui| {
                for model in models {
                    let name = model.rsplit('/').next().unwrap_or(model);
                    ui.selectable_value(&mut picked, Some(model.clone()), name);
                }
            })
            .response
            .on_hover_text("The model the preview shows it on");
        if picked != self.preview_model {
            self.preview_model = picked;
        }
    }

    /// The bones of the preview model's skeleton as a tree that folds
    /// (`folds`): those the take moves locked (🔒), the rest selectable to
    /// pose, with their key counts (right-click one to remove its keys).
    pub fn bones(
        &mut self,
        ui: &mut egui::Ui,
        assets: &Assets,
        selected: &mut Option<usize>,
        folds: &mut Folds,
    ) {
        let Some((mesh_id, model)) = self
            .preview_model
            .clone()
            .and_then(|id| Some((id.clone(), assets.model(&id)?)))
        else {
            ui.weak("No model to show the bones of.");
            return;
        };
        let Some(rig) = &model.rig else {
            ui.weak("This model has no skeleton.");
            return;
        };
        let locked = self.locked(rig);
        let keys = self.bone_keys(rig);
        let named = super::posing::bone_rows(rig);
        let moved = named.iter().filter(|(bone, _)| locked[*bone]).count();
        ui.horizontal_wrapped(|ui| {
            ui.weak(format!(
                "{} bones, {moved} moved by the take {}",
                named.len(),
                icons::LOCK
            ));
            folds.buttons(ui, &mesh_id, rig);
        });
        let mut remove = None;
        let mut toggle = None;
        for tree_row in folds.rows(&mesh_id, rig, &keys) {
            let bone = tree_row.bone;
            let (row, toggled) = super::posing::bone_row(
                ui,
                &rig.bones[bone].name,
                &tree_row,
                *selected == Some(bone),
                locked[bone],
                keys[bone],
            );
            if toggled {
                toggle = Some(bone);
            }
            if row.clicked() {
                *selected = Some(bone);
            }
            if keys[bone] > 0 {
                row.context_menu(|ui| {
                    if ui
                        .button(format!("{} Remove its keys", icons::TRASH))
                        .clicked()
                    {
                        remove = Some(bone);
                        ui.close();
                    }
                });
            }
        }
        if let Some(bone) = toggle {
            folds.toggle(&mesh_id, rig, bone);
        }
        if let Some(bone) = remove {
            self.remove_bone_keys(rig, bone);
        }
    }

    /// The frame counter and the timeline.
    pub fn timeline(&mut self, ui: &mut egui::Ui) {
        let take_seconds = self
            .take()
            .map_or(self.file.end, |take| f64::from(take.duration));
        let speed = self.file.speed.abs().max(1e-3);
        ui.horizontal_wrapped(|ui| {
            let frame = self.frames(self.playhead * speed);
            let frames = self.frames(self.file.length());
            ui.monospace(format!("Frame {frame} / {frames}"));
            ui.weak(format!(
                "{:.2} s of {:.2} s",
                self.playhead,
                self.file.length() / speed
            ));
        });
        let events: Vec<(f64, &str)> = self
            .file
            .events
            .iter()
            .map(|event| (self.file.start + event.time, event.name.as_str()))
            .collect();
        let change = Timeline {
            take_seconds: take_seconds.max(self.file.end),
            frame_rate: self.frame_rate(),
            start: self.file.start,
            end: self.file.end,
            playhead: self.file.start + self.playhead * speed,
            events: &events,
        }
        .show(ui);
        if let Some(at) = change.playhead {
            self.playing = false;
            self.playhead = (at - self.file.start) / speed;
        }
        if let Some(start) = change.start {
            self.file.start = start;
        }
        if let Some(end) = change.end {
            self.file.end = end;
        }
        if let Some((index, at)) = change.moved_event
            && let Some(event) = self.file.events.get_mut(index)
        {
            event.time = (at - self.file.start).max(0.0);
        }
        if let Some(index) = change.removed_event {
            self.file.events.remove(index);
        }
        self.playhead = self.playhead.clamp(0.0, self.file.length() / speed);
    }

    /// The clip's settings (Loop, Speed and the preview model are in the
    /// toolbar). Returns true if Rename was pressed.
    pub fn inspector(&mut self, ui: &mut egui::Ui) -> bool {
        let mut rename = false;
        ui.horizontal_wrapped(|ui| {
            ui.heading(axcenity_anim::files::clip_name(&self.path));
            if ui
                .small_button(icons::PENCIL_SIMPLE)
                .on_hover_text("Rename this clip (its file)")
                .clicked()
            {
                rename = true;
            }
        });
        ui.weak(&self.path);
        if let Err(problem) = &self.takes {
            ui.colored_label(ui.visuals().error_fg_color, problem);
        }
        ui.add_space(4.0);
        let takes: Vec<(String, f32)> = self.takes.as_ref().map_or_else(
            |_| Vec::new(),
            |takes| takes.iter().map(|t| (t.name.clone(), t.duration)).collect(),
        );
        let frames_total = self.frames(
            self.take()
                .map_or(self.file.end, |take| f64::from(take.duration)),
        );
        crate::responsive::form(ui, "clip settings", |ui| {
            ui.label("Model");
            ui.label(self.file.source.rsplit('/').next().unwrap_or(""))
                .on_hover_text(&self.file.source);
            ui.end_row();

            ui.label("Take");
            let mut take = self.file.take.clone();
            egui::ComboBox::from_id_salt("clip take")
                .selected_text(&take)
                .show_ui(ui, |ui| {
                    for (name, _) in &takes {
                        ui.selectable_value(&mut take, name.clone(), name);
                    }
                });
            if take != self.file.take
                && let Some((_, duration)) = takes.iter().find(|(name, _)| *name == take)
            {
                self.file.take = take;
                self.file.start = 0.0;
                self.file.end = f64::from(*duration);
                self.playhead = 0.0;
            }
            ui.end_row();

            ui.label("Start");
            let mut start = self.frames(self.file.start);
            let mut end = self.frames(self.file.end);
            if ui
                .add(
                    egui::DragValue::new(&mut start)
                        .range(0..=end.saturating_sub(1))
                        .suffix(" f"),
                )
                .changed()
            {
                self.file.start = self.seconds(start);
            }
            ui.end_row();
            ui.label("End");
            if ui
                .add(
                    egui::DragValue::new(&mut end)
                        .range(start + 1..=frames_total.max(start + 1))
                        .suffix(" f"),
                )
                .changed()
            {
                self.file.end = self.seconds(end);
            }
            ui.end_row();

            ui.label("Root motion");
            ui.horizontal_wrapped(|ui| {
                    ui.checkbox(&mut self.file.root_motion, "").on_hover_text(
                        "On, the root bone's travel across the ground moves the MeshPart instead (the preview stays in place)",
                    );
                    if self.file.root_motion && self.root_motion_found() == Some(false) {
                        ui.weak("no bone moves across the ground");
                    }
                });
            ui.end_row();
        });

        ui.separator();
        ui.strong("Events");
        ui.weak("Fire the Animator's AnimationEvent as the clip passes them.");
        let mut remove = None;
        let fps = self.frame_rate();
        let length_frames = self.frames(self.file.length());
        egui::Grid::new("clip events")
            .num_columns(3)
            .spacing([8.0, 4.0])
            .show(ui, |ui| {
                for (index, event) in self.file.events.iter_mut().enumerate() {
                    ui.add(egui::TextEdit::singleline(&mut event.name).desired_width(110.0));
                    let mut frame = (event.time * f64::from(fps)).round().max(0.0) as u32;
                    if ui
                        .add(
                            egui::DragValue::new(&mut frame)
                                .range(0..=length_frames)
                                .prefix("frame "),
                        )
                        .changed()
                    {
                        event.time = f64::from(frame) / f64::from(fps);
                    }
                    if ui.small_button(icons::X).on_hover_text("Remove").clicked() {
                        remove = Some(index);
                    }
                    ui.end_row();
                }
            });
        if let Some(index) = remove {
            self.file.events.remove(index);
        }
        ui.horizontal_wrapped(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.new_event).desired_width(110.0));
            if ui
                .add_enabled(
                    !self.new_event.trim().is_empty(),
                    egui::Button::new(format!("{} Add at playhead", icons::PLUS)),
                )
                .clicked()
            {
                let speed = self.file.speed.abs().max(1e-3);
                let time = (self.playhead * speed).clamp(0.0, self.file.length());
                self.file.events.push(ClipEvent {
                    time: (time * f64::from(fps)).round() / f64::from(fps),
                    name: self.new_event.trim().to_owned(),
                });
                self.file.events.sort_by(|a, b| a.time.total_cmp(&b.time));
            }
        });

        ui.separator();
        ui.horizontal_wrapped(|ui| {
            if self.unwritten() {
                ui.weak("Saving…");
            } else {
                ui.weak(format!("{} Saved", icons::CHECK));
            }
            if ui
                .add_enabled(self.file != self.opened, egui::Button::new("Revert"))
                .on_hover_text("Put the clip back as it was when it was opened")
                .clicked()
            {
                self.file = self.opened.clone();
                self.playhead = 0.0;
            }
        });
        rename
    }
}
