//! A property clip open in the Animation tab: dope-sheet tracks, keys,
//! playback on the selected Part (with restore), Record, and save/undo hooks.

use std::path::Path;

use axcenity_anim::{
    Interp, PropertyClip, PropertyClipFile, PropertyKeyframe, PropertyName, PropertyTrack,
    apply_evaluated,
};
use axcenity_core::{InstanceId, Value, Vector3, World};
use eframe::egui;
use eframe::egui::Color32;

use crate::theme::icons;

/// A property clip being edited.
pub struct PropertyDoc {
    pub path: String,
    pub file: PropertyClipFile,
    written: PropertyClipFile,
    /// As it was opened, for Revert.
    opened: PropertyClipFile,
    pub playhead: f64,
    pub playing: bool,
    /// Key id selected in the dope sheet.
    pub selected_key: Option<(String, String)>,
    /// Track id selected for Insert key.
    pub selected_track: Option<String>,
    /// Baseline Part transforms when preview started, restored on stop/close.
    baseline: Option<(InstanceId, Vec<(String, Vector3)>)>,
    /// Each track's value as the preview last set it, so Record can tell
    /// when something else moved it.
    applied: Vec<(String, [f32; 3])>,
    new_event: String,
}

impl PropertyDoc {
    pub fn open(path: String, file: PropertyClipFile) -> Self {
        let selected_track = file.tracks.first().map(|t| t.id.clone());
        Self {
            path,
            written: file.clone(),
            opened: file.clone(),
            file,
            playhead: 0.0,
            playing: false,
            selected_key: None,
            selected_track,
            baseline: None,
            applied: Vec::new(),
            new_event: "Opened".to_owned(),
        }
    }

    pub fn unwritten(&self) -> bool {
        self.file != self.written
    }

    pub fn write(&mut self, project: &Path) -> std::io::Result<Option<(String, String)>> {
        if !self.unwritten() {
            return Ok(None);
        }
        let before = self.written.to_json();
        let after = self.file.to_json();
        axcenity_anim::files::write_file(&project.join(&self.path), &after)?;
        self.written = self.file.clone();
        Ok(Some((before, after)))
    }

    /// Snapshot `root`'s animated properties so scrubbing can be undone.
    pub fn capture_baseline(&mut self, world: &World, root: InstanceId) {
        if self.baseline.is_some() {
            return;
        }
        let mut props = Vec::new();
        if let Ok(clip) = PropertyClip::from_file(self.file.clone()) {
            for track in &clip.file.tracks {
                if let Some(id) = axcenity_anim::resolve_target(world, root, &track.target)
                    && let Some(Value::Vector3(v)) = world
                        .get(id)
                        .and_then(|i| i.property(track.property.as_str()))
                {
                    props.push((format!("{}:{}", track.target, track.property.as_str()), *v));
                    let _ = (id, track);
                }
            }
        }
        // Also snapshot the root's transform properties directly.
        if let Some(instance) = world.get(root) {
            for name in ["Position", "Orientation", "Size"] {
                if let Some(Value::Vector3(v)) = instance.property(name) {
                    props.push((format!(".:{name}"), *v));
                }
            }
        }
        self.baseline = Some((root, props));
    }

    pub fn restore_baseline(&mut self, world: &mut World) {
        self.applied.clear();
        let Some((root, props)) = self.baseline.take() else {
            return;
        };
        for (key, value) in props {
            let (target, property) = key.split_once(':').unwrap_or((".", key.as_str()));
            let target = if target == "." { "." } else { target };
            if let Some(id) = axcenity_anim::resolve_target(world, root, target) {
                let _ = world.set_property(id, property, Value::Vector3(value));
            }
        }
    }

    /// Applies the pose at the playhead to `root` (after capturing baseline).
    pub fn apply_preview(&mut self, world: &mut World, root: InstanceId) {
        self.capture_baseline(world, root);
        let Ok(clip) = PropertyClip::from_file(self.file.clone()) else {
            return;
        };
        let evaluated = clip.evaluate(self.playhead);
        let mut missing = std::collections::HashMap::new();
        apply_evaluated(world, root, &evaluated, &mut missing);
        self.applied = self
            .file
            .tracks
            .iter()
            .filter_map(|track| Some((track.id.clone(), track_value(world, root, track)?)))
            .collect();
    }

    pub fn tick(&mut self, dt: f64) {
        if !self.playing {
            return;
        }
        self.playhead += dt;
        if self.file.looping {
            let d = self.file.duration.max(1e-9);
            self.playhead = self.playhead.rem_euclid(d);
        } else if self.playhead >= self.file.duration {
            self.playhead = self.file.duration;
            self.playing = false;
        }
    }

    /// Inserts or updates a key on `track_id` at the playhead from `world`.
    pub fn insert_key_from_world(&mut self, world: &World, root: InstanceId, track_id: &str) {
        let Some(track) = self.file.tracks.iter_mut().find(|t| t.id == track_id) else {
            return;
        };
        let Some(id) = axcenity_anim::resolve_target(world, root, &track.target) else {
            return;
        };
        let Some(Value::Vector3(v)) = world
            .get(id)
            .and_then(|i| i.property(track.property.as_str()).cloned())
        else {
            return;
        };
        let time = (self.playhead * 1000.0).round() / 1000.0;
        if let Some(key) = track.keys.iter_mut().find(|k| (k.time - time).abs() < 1e-6) {
            key.value = [v.x, v.y, v.z];
            self.selected_key = Some((track.id.clone(), key.id.clone()));
            return;
        }
        // The first free id, as deleting keys leaves gaps.
        let id = (1..)
            .map(|n| format!("k{n}"))
            .find(|id| !track.keys.iter().any(|key| key.id == *id))
            .expect("some id is free");
        track.keys.push(PropertyKeyframe {
            id: id.clone(),
            time,
            value: [v.x, v.y, v.z],
            interp: Interp::Linear,
        });
        track.keys.sort_by(|a, b| a.time.total_cmp(&b.time));
        self.selected_key = Some((track.id.clone(), id));
    }

    pub fn add_track(&mut self, property: PropertyName) {
        let id = format!(
            "{}{}",
            property.as_str().to_lowercase(),
            self.file.tracks.len() + 1
        );
        self.file.tracks.push(PropertyTrack {
            id: id.clone(),
            target: ".".into(),
            property,
            keys: Vec::new(),
        });
        self.selected_track = Some(id);
    }

    pub fn delete_selected_key(&mut self) {
        let Some((track_id, key_id)) = self.selected_key.take() else {
            return;
        };
        if let Some(track) = self.file.tracks.iter_mut().find(|t| t.id == track_id) {
            track.keys.retain(|k| k.id != key_id);
        }
    }

    /// Keys, at the playhead, every track whose property something else
    /// (a drag in the Scene View) changed since the preview last set it.
    /// Recording pauses playback.
    pub fn record_changes(&mut self, world: &World, root: InstanceId) {
        let moved: Vec<String> = self
            .file
            .tracks
            .iter()
            .filter(|track| {
                let applied = self.applied.iter().find(|(id, _)| *id == track.id);
                let now = track_value(world, root, track);
                matches!((applied, now), (Some((_, was)), Some(now))
                    if was.iter().zip(now).any(|(a, b)| (a - b).abs() > 1e-4))
            })
            .map(|track| track.id.clone())
            .collect();
        if !moved.is_empty() {
            self.playing = false;
        }
        for track in moved {
            self.insert_key_from_world(world, root, &track);
        }
    }

    /// The toolbar's part: play/pause, back to the start, Stop (which puts
    /// the Part back) and Loop.
    pub fn controls(
        &mut self,
        ui: &mut egui::Ui,
        world: &mut World,
        preview_root: Option<InstanceId>,
    ) {
        let (icon, hover) = if self.playing {
            (icons::PAUSE, "Pause")
        } else {
            (icons::PLAY, "Play")
        };
        if ui.button(icon).on_hover_text(hover).clicked() {
            if !self.playing && self.playhead >= self.file.duration {
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
        if ui
            .button(icons::STOP)
            .on_hover_text("Stop and put the Part back as it was")
            .clicked()
        {
            self.playing = false;
            self.playhead = 0.0;
            if preview_root.is_some() {
                self.restore_baseline(world);
            }
        }
        ui.checkbox(&mut self.file.looping, "Loop");
        ui.monospace(format!("{:.2} s", self.playhead));
    }

    /// The objects it animates (each track's target) and their tracks;
    /// clicking a track selects it. `root` names what `.` is.
    pub fn objects(&mut self, ui: &mut egui::Ui, root: Option<&str>) {
        if self.file.tracks.is_empty() {
            ui.weak("No tracks yet: Add track, under the dope sheet.");
            return;
        }
        let mut targets: Vec<&str> = Vec::new();
        for track in &self.file.tracks {
            if !targets.contains(&track.target.as_str()) {
                targets.push(&track.target);
            }
        }
        let mut picked = None;
        for target in targets {
            let name = match target {
                "." | "" => root.unwrap_or("(the Part)"),
                path => path,
            };
            ui.add(egui::Label::new(format!("{} {name}", icons::CUBE)).truncate());
            for track in self.file.tracks.iter().filter(|t| t.target == target) {
                ui.horizontal(|ui| {
                    ui.add_space(12.0);
                    let selected = self.selected_track.as_deref() == Some(track.id.as_str());
                    if ui
                        .selectable_label(selected, track.property.as_str())
                        .on_hover_text(format!("{} keys", track.keys.len()))
                        .clicked()
                    {
                        picked = Some(track.id.clone());
                    }
                });
            }
        }
        if picked.is_some() {
            self.selected_track = picked;
        }
    }

    /// Add track, Insert key and Delete key, then the dope sheet.
    pub fn keys(&mut self, ui: &mut egui::Ui, world: &mut World, preview_root: Option<InstanceId>) {
        crate::responsive::toolbar(ui, |ui| {
            ui.menu_button(format!("{} Add track", icons::PLUS), |ui| {
                for property in [
                    PropertyName::Position,
                    PropertyName::Orientation,
                    PropertyName::Size,
                ] {
                    if ui.button(property.as_str()).clicked() {
                        self.add_track(property);
                        ui.close();
                    }
                }
            });
            let can_key = self.selected_track.is_some() && preview_root.is_some();
            if ui
                .add_enabled(
                    can_key,
                    egui::Button::new(format!("{} Insert key", icons::KEY)),
                )
                .on_hover_text("Key the selected track from the selected Part at the playhead")
                .clicked()
                && let (Some(track_id), Some(root)) = (self.selected_track.clone(), preview_root)
            {
                self.capture_baseline(world, root);
                self.insert_key_from_world(world, root, &track_id);
            }
            if ui
                .add_enabled(self.selected_key.is_some(), egui::Button::new("Delete key"))
                .clicked()
            {
                self.delete_selected_key();
            }
            ui.weak("Shift+drag moves the selected key");
        });
        self.dope_sheet(ui);
    }

    /// The clip's settings: its name, duration, the selected key and the
    /// events. Returns true if Rename was pressed.
    pub fn inspector(&mut self, ui: &mut egui::Ui, preview_root: Option<InstanceId>) -> bool {
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
        if preview_root.is_none() {
            ui.weak("Select a Part, MeshPart or Model in the Explorer to preview and key it.");
        }
        ui.add_space(4.0);
        crate::responsive::form(ui, "property clip settings", |ui| {
            ui.label("Duration");
            ui.add(
                egui::DragValue::new(&mut self.file.duration)
                    .speed(0.05)
                    .range(0.0..=3600.0)
                    .suffix(" s"),
            );
            ui.end_row();
        });

        if let Some((track_id, key_id)) = self.selected_key.clone()
            && let Some(track) = self.file.tracks.iter_mut().find(|t| t.id == track_id)
            && let Some(key) = track.keys.iter_mut().find(|k| k.id == key_id)
        {
            ui.separator();
            ui.strong("Keyframe");
            crate::responsive::form(ui, "property key", |ui| {
                ui.label("Time");
                ui.add(egui::DragValue::new(&mut key.time).speed(0.01).suffix(" s"));
                key.time = key.time.clamp(0.0, self.file.duration);
                ui.end_row();
                ui.label("Value");
                ui.horizontal_wrapped(|ui| {
                    for (i, label) in ["X ", "Y ", "Z "].iter().enumerate() {
                        ui.add(
                            egui::DragValue::new(&mut key.value[i])
                                .speed(0.1)
                                .prefix(*label),
                        );
                    }
                });
                ui.end_row();
                ui.label("Interpolation");
                egui::ComboBox::from_id_salt("property key interpolation")
                    .selected_text(match key.interp {
                        Interp::Step => "Step",
                        Interp::Linear => "Linear",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut key.interp, Interp::Step, "Step");
                        ui.selectable_value(&mut key.interp, Interp::Linear, "Linear");
                    });
                ui.end_row();
            });
        }

        ui.separator();
        ui.strong("Events");
        let mut remove = None;
        for (i, event) in self.file.events.iter_mut().enumerate() {
            ui.horizontal_wrapped(|ui| {
                ui.add(
                    egui::DragValue::new(&mut event.time)
                        .speed(0.01)
                        .suffix(" s"),
                );
                ui.add(egui::TextEdit::singleline(&mut event.name).desired_width(110.0));
                if ui
                    .small_button(icons::TRASH)
                    .on_hover_text("Remove")
                    .clicked()
                {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove {
            self.file.events.remove(i);
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
                self.file.events.push(axcenity_anim::ClipEvent {
                    time: self.playhead.clamp(0.0, self.file.duration),
                    name: self.new_event.trim().to_owned(),
                });
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
                self.selected_key = None;
                self.playhead = 0.0;
            }
        });
        rename
    }

    fn dope_sheet(&mut self, ui: &mut egui::Ui) {
        let duration = self.file.duration.max(0.001);
        let height = 24.0 * (self.file.tracks.len().max(1) as f32) + 28.0;
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), height),
            egui::Sense::click_and_drag(),
        );
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, ui.visuals().extreme_bg_color);

        let label_w = 120.0;
        let timeline = egui::Rect::from_min_max(
            egui::pos2(rect.left() + label_w, rect.top() + 20.0),
            rect.right_bottom(),
        );
        let x_of = |t: f64| timeline.left() + (t / duration) as f32 * timeline.width();
        let t_of = |x: f32| {
            (((x - timeline.left()) / timeline.width()) as f64 * duration).clamp(0.0, duration)
        };

        // Ruler.
        painter.text(
            egui::pos2(rect.left() + 4.0, rect.top() + 4.0),
            egui::Align2::LEFT_TOP,
            "Tracks",
            egui::FontId::proportional(12.0),
            ui.visuals().text_color(),
        );
        for i in 0..=4 {
            let t = duration * i as f64 / 4.0;
            let x = x_of(t);
            painter.line_segment(
                [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                egui::Stroke::new(1.0_f32, Color32::from_gray(60)),
            );
            painter.text(
                egui::pos2(x, rect.top() + 2.0),
                egui::Align2::CENTER_TOP,
                format!("{t:.1}"),
                egui::FontId::proportional(10.0),
                ui.visuals().weak_text_color(),
            );
        }

        for (row, track) in self.file.tracks.iter().enumerate() {
            let y = timeline.top() + row as f32 * 24.0;
            let row_rect = egui::Rect::from_min_size(
                egui::pos2(rect.left(), y),
                egui::vec2(rect.width(), 24.0),
            );
            let selected = self.selected_track.as_deref() == Some(track.id.as_str());
            if selected {
                painter.rect_filled(
                    row_rect,
                    0.0,
                    Color32::from_rgba_unmultiplied(40, 80, 120, 60),
                );
            }
            painter.text(
                egui::pos2(rect.left() + 4.0, y + 4.0),
                egui::Align2::LEFT_TOP,
                format!("{} · {}", track.property.as_str(), track.target),
                egui::FontId::proportional(12.0),
                ui.visuals().text_color(),
            );
            for key in &track.keys {
                let x = x_of(key.time);
                let center = egui::pos2(x, y + 12.0);
                let key_selected =
                    self.selected_key.as_ref() == Some(&(track.id.clone(), key.id.clone()));
                let color = if key_selected {
                    Color32::from_rgb(255, 200, 80)
                } else {
                    Color32::from_rgb(120, 200, 255)
                };
                painter.circle_filled(center, 5.0, color);
            }
        }

        // Playhead.
        let px = x_of(self.playhead);
        painter.line_segment(
            [egui::pos2(px, rect.top()), egui::pos2(px, rect.bottom())],
            egui::Stroke::new(2.0_f32, Color32::from_rgb(255, 120, 80)),
        );

        if response.clicked()
            && let Some(pos) = response.interact_pointer_pos()
        {
            if pos.x >= timeline.left() {
                self.playhead = t_of(pos.x);
                self.playing = false;
            }
            let row = ((pos.y - timeline.top()) / 24.0).floor() as isize;
            if row >= 0 && (row as usize) < self.file.tracks.len() {
                let track = &self.file.tracks[row as usize];
                self.selected_track = Some(track.id.clone());
                let mut best = None;
                for key in &track.keys {
                    let dx = (x_of(key.time) - pos.x).abs();
                    if dx < 8.0 {
                        best = Some((dx, key.id.clone()));
                    }
                }
                if let Some((_, id)) = best {
                    self.selected_key = Some((track.id.clone(), id));
                }
            }
        }
        if response.dragged()
            && let Some(pos) = response.interact_pointer_pos()
            && pos.x >= timeline.left()
        {
            self.playhead = t_of(pos.x);
            self.playing = false;
            if let Some((track_id, key_id)) = self.selected_key.clone()
                && let Some(track) = self.file.tracks.iter_mut().find(|t| t.id == track_id)
                && let Some(key) = track.keys.iter_mut().find(|k| k.id == key_id)
                && response.dragged_by(egui::PointerButton::Primary)
                && ui.input(|i| i.modifiers.shift)
            {
                key.time = t_of(pos.x);
            }
        }
    }
}

/// The value of `track`'s property on its target under `root`, if it's
/// there and a vector.
fn track_value(world: &World, root: InstanceId, track: &PropertyTrack) -> Option<[f32; 3]> {
    let id = axcenity_anim::resolve_target(world, root, &track.target)?;
    match world.get(id)?.property(track.property.as_str())? {
        Value::Vector3(v) => Some([v.x, v.y, v.z]),
        _ => None,
    }
}

/// Empty property clip for "New property clip".
pub fn new_property_clip(duration: f64) -> PropertyClipFile {
    let mut file = PropertyClipFile::new(duration);
    file.tracks.push(PropertyTrack {
        id: "position".into(),
        target: ".".into(),
        property: PropertyName::Position,
        keys: vec![
            PropertyKeyframe {
                id: "k1".into(),
                time: 0.0,
                value: [0.0, 0.0, 0.0],
                interp: Interp::Linear,
            },
            PropertyKeyframe {
                id: "k2".into(),
                time: duration,
                value: [0.0, 8.0, 0.0],
                interp: Interp::Linear,
            },
        ],
    });
    file
}
