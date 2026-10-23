//! A clip open in the Animation tab: its toolbar controls, settings,
//! timeline and bones, and the pose its preview shows.

use std::path::Path;
use std::sync::Arc;

use axcenity_anim::{BoundClip, ClipEvent, ClipFile};
use axcenity_assets::{Assets, Posed, Take};
use eframe::egui;

use super::timeline::Timeline;
use crate::theme::icons;

/// A clip being looked at and changed.
pub struct ClipDoc {
    pub path: String,
    pub file: ClipFile,
    /// As last written, to tell when it needs writing again.
    written: ClipFile,
    /// As it was opened, for Revert.
    opened: ClipFile,
    /// The takes of its model file, or why they couldn't be read.
    takes: Result<Arc<Vec<Take>>, String>,
    /// Seconds into the clip (0 is its start), at its own speed.
    pub playhead: f64,
    pub playing: bool,
    /// The model the preview shows it on (a `MeshId`).
    pub preview_model: Option<String>,
    /// The clip bound to the preview model, for the file as it was then.
    bound: Option<(u64, ClipFile, Result<BoundClip, String>)>,
    /// A new event's name, typed before Add event.
    new_event: String,
}

impl ClipDoc {
    pub fn open(path: String, file: ClipFile, assets: &Assets, models: &[String]) -> Self {
        let takes = assets
            .takes(&file.source)
            .ok_or_else(|| format!("its model file `{}` can't be read", file.source));
        let mut doc = Self {
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
        };
        doc.preview_model = doc.default_preview_model(assets, models);
        doc
    }

    /// The clip's own model if it has one to draw with a skeleton, or the
    /// first project model its take moves.
    fn default_preview_model(&self, assets: &Assets, models: &[String]) -> Option<String> {
        let takes = self.takes.as_ref().ok()?;
        let fits = |mesh_id: &str| {
            let Some(model) = assets.model(mesh_id) else {
                return false;
            };
            let Some(rig) = &model.rig else {
                return false;
            };
            BoundClip::bind(self.file.clone(), takes.clone(), rig).is_ok()
        };
        std::iter::once(self.file.source.clone())
            .chain(models.iter().cloned())
            .find(|mesh_id| fits(mesh_id))
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
        super::store::save_clip(project, &self.path, &self.file)?;
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

    /// The preview model's pose at the playhead, or why there isn't one.
    pub fn pose(&mut self, assets: &Assets) -> Result<Arc<Posed>, String> {
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
            let bound = BoundClip::bind(self.file.clone(), takes, rig);
            self.bound = Some((model.key(), self.file.clone(), bound));
        }
        let Some((_, _, bound)) = &self.bound else {
            return Err("the clip isn't ready".to_owned());
        };
        let bound = bound.as_ref().map_err(Clone::clone)?;
        let mut pose = rig.rest_pose();
        bound.sample(self.phase(), rig, &mut pose);
        model
            .pose(&pose)
            .map(Arc::new)
            .ok_or_else(|| "the model can't be posed".to_owned())
    }

    /// Whether root motion found a bone to move the model by.
    fn root_motion_found(&self) -> Option<bool> {
        let (_, _, bound) = self.bound.as_ref()?;
        bound.as_ref().ok().map(BoundClip::has_root_motion)
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
            .selected_text(format!("{} {shown}", icons::CUBE))
            .width(130.0)
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

    /// The bones of the preview model's skeleton as a tree, those the take
    /// moves in the normal text colour and the rest dimmed.
    pub fn bones(&self, ui: &mut egui::Ui, assets: &Assets) {
        let Some(model) = self
            .preview_model
            .as_deref()
            .and_then(|id| assets.model(id))
        else {
            ui.weak("No model to show the bones of.");
            return;
        };
        let Some(rig) = &model.rig else {
            ui.weak("This model has no skeleton.");
            return;
        };
        let moved: Vec<&str> = self
            .take()
            .map(|take| {
                take.tracks
                    .iter()
                    .map(|track| track.bone.as_str())
                    .collect()
            })
            .unwrap_or_default();
        // Parents come before their children, so each depth is known in time.
        let mut depths = Vec::with_capacity(rig.bones.len());
        for bone in &rig.bones {
            let depth = bone.parent.map_or(0, |parent| depths[parent] + 1);
            depths.push(depth);
        }
        ui.weak(format!("{} bones, {} moved", rig.bones.len(), moved.len()));
        for (bone, depth) in rig.bones.iter().zip(depths) {
            ui.horizontal(|ui| {
                ui.add_space((depth as f32 * 8.0).min(64.0));
                let text = egui::RichText::new(format!("{} {}", icons::BONE, bone.name));
                let text = if moved.contains(&bone.name.as_str()) {
                    text
                } else {
                    text.weak()
                };
                ui.add(egui::Label::new(text).truncate())
                    .on_hover_text(&bone.name);
            });
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
