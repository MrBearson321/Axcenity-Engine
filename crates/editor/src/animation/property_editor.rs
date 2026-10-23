//! A property clip open in the Animation panel: dope-sheet tracks, keys,
//! playback on the selected Part (with restore), and save/undo hooks.

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
    #[allow(dead_code)] // kept for Revert
    opened: PropertyClipFile,
    pub playhead: f64,
    pub playing: bool,
    /// Key id selected in the dope sheet.
    pub selected_key: Option<(String, String)>,
    /// Track id selected for Insert key.
    pub selected_track: Option<String>,
    /// Baseline Part transforms when preview started, restored on stop/close.
    baseline: Option<(InstanceId, Vec<(String, Vector3)>)>,
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

    #[allow(dead_code)]
    pub fn revert(&mut self) {
        self.file = self.opened.clone();
        self.written = self.opened.clone();
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
        let id = format!("k{}", track.keys.len() + 1);
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

    /// Draws the inspector and dope sheet. Returns true if the file changed.
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        world: &mut World,
        preview_root: Option<InstanceId>,
    ) -> bool {
        let before = self.file.clone();
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    !self.playing,
                    egui::Button::new(format!("{} Play", icons::PLAY)),
                )
                .clicked()
            {
                self.playing = true;
            }
            if ui.button(format!("{} Pause", icons::PAUSE)).clicked() {
                self.playing = false;
            }
            if ui
                .button(format!("{} Stop", icons::ARROW_COUNTER_CLOCKWISE))
                .on_hover_text("Stop and restore the Part's saved pose")
                .clicked()
            {
                self.playing = false;
                self.playhead = 0.0;
                if preview_root.is_some() {
                    self.restore_baseline(world);
                }
            }
            ui.checkbox(&mut self.file.looping, "Loop");
            ui.label(format!("{:.2} s", self.playhead));
            ui.add(
                egui::DragValue::new(&mut self.file.duration)
                    .speed(0.05)
                    .range(0.0..=3600.0)
                    .prefix("Duration "),
            );
        });

        ui.separator();
        ui.horizontal_wrapped(|ui| {
            ui.menu_button("Add track", |ui| {
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
            if let Some(track_id) = self.selected_track.clone()
                && let Some(root) = preview_root
                && ui
                    .button("Insert key")
                    .on_hover_text("Key the track from the selected Part at the playhead")
                    .clicked()
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
        });

        self.dope_sheet(ui);

        if let Some((track_id, key_id)) = self.selected_key.clone()
            && let Some(track) = self.file.tracks.iter_mut().find(|t| t.id == track_id)
            && let Some(key) = track.keys.iter_mut().find(|k| k.id == key_id)
        {
            ui.separator();
            ui.heading("Keyframe");
            ui.add(
                egui::DragValue::new(&mut key.time)
                    .speed(0.01)
                    .prefix("Time "),
            );
            key.time = key.time.clamp(0.0, self.file.duration);
            ui.horizontal_wrapped(|ui| {
                ui.label("Value");
                for (i, label) in ["X", "Y", "Z"].iter().enumerate() {
                    ui.add(
                        egui::DragValue::new(&mut key.value[i])
                            .speed(0.1)
                            .prefix(*label),
                    );
                }
            });
            egui::ComboBox::from_label("Interpolation")
                .selected_text(match key.interp {
                    Interp::Step => "Step",
                    Interp::Linear => "Linear",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut key.interp, Interp::Step, "Step");
                    ui.selectable_value(&mut key.interp, Interp::Linear, "Linear");
                });
        }

        ui.separator();
        ui.heading("Events");
        let mut remove = None;
        for (i, event) in self.file.events.iter_mut().enumerate() {
            ui.horizontal_wrapped(|ui| {
                ui.add(egui::DragValue::new(&mut event.time).speed(0.01));
                ui.text_edit_singleline(&mut event.name);
                if ui.small_button(icons::TRASH).clicked() {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove {
            self.file.events.remove(i);
        }
        ui.horizontal_wrapped(|ui| {
            ui.text_edit_singleline(&mut self.new_event);
            if ui.button("Add at playhead").clicked() && !self.new_event.trim().is_empty() {
                self.file.events.push(axcenity_anim::ClipEvent {
                    time: self.playhead.clamp(0.0, self.file.duration),
                    name: self.new_event.trim().to_owned(),
                });
            }
        });

        if let Some(root) = preview_root {
            self.apply_preview(world, root);
        }

        self.file != before
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
