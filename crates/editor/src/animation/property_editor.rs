//! A property clip open in the Animation tab: its tracks and keys, the
//! dope sheet, and keying what's moved in the preview.
//!
//! Moving something in the preview with Record off changes it only until
//! the playhead moves ("pending"); Key selected (K) or Record turns the
//! moves into keys at the playhead, making tracks as needed. The dope sheet
//! selects keys by clicking (Shift or Ctrl adds) or dragging a box, moves
//! them by dragging, and copies, pastes and deletes them. With Snap on,
//! times land on whole frames.

use std::collections::BTreeSet;
use std::path::Path;

use axcenity_anim::{
    ClipFile, EvaluatedProperty, Interp, PropertyClip, PropertyKeyframe, PropertyName,
    PropertyTrack, target_path,
};
use axcenity_core::InstanceId;
use eframe::egui;

use super::object_preview::{Focus, ObjectPreview};
use crate::theme::{self, icons};

/// The dope sheet's row height and ruler height, in points.
const ROW: f32 = 22.0;
const RULER: f32 = 20.0;
/// How close (in points) the mouse must be to a key to grab it.
const GRAB: f32 = 7.0;

/// A key, by its track's id and its own.
type KeyRef = (String, String);

/// What a drag in the dope sheet is doing.
enum Gesture {
    /// Moving the playhead along the ruler.
    Scrub,
    /// Moving the selected keys: the time under the mouse when it went
    /// down, the grabbed key's time, and every selected key's time then.
    Keys {
        from: f64,
        anchor: f64,
        start: Vec<(KeyRef, f64)>,
    },
    /// Selecting the keys in a box from `from`; `add` keeps the selection.
    Box { from: egui::Pos2, add: bool },
}

/// A property clip being edited.
pub struct PropertyDoc {
    pub path: String,
    pub file: ClipFile,
    written: ClipFile,
    /// As it was opened, for Revert.
    opened: ClipFile,
    pub playhead: f64,
    pub playing: bool,
    /// Keys selected in the dope sheet.
    pub selected: BTreeSet<KeyRef>,
    /// The track last clicked, highlighted in the dope sheet and drawer.
    pub selected_track: Option<String>,
    /// Moves made in the preview that aren't keys, and the playhead they
    /// were made at (moving it drops them).
    pending: Vec<EvaluatedProperty>,
    pending_at: f64,
    /// Copied keys: their track, how long after the first copied key, and
    /// the key.
    clipboard: Vec<(String, f64, PropertyKeyframe)>,
    /// Times snap to whole frames at `fps` frames a second.
    pub snap: bool,
    pub fps: f64,
    gesture: Option<Gesture>,
    new_event: String,
}

impl PropertyDoc {
    pub fn open(path: String, file: ClipFile) -> Self {
        let selected_track = file.tracks.first().map(|t| t.id.clone());
        Self {
            path,
            written: file.clone(),
            opened: file.clone(),
            file,
            playhead: 0.0,
            playing: false,
            selected: BTreeSet::new(),
            selected_track,
            pending: Vec::new(),
            pending_at: 0.0,
            clipboard: Vec::new(),
            snap: true,
            fps: 30.0,
            gesture: None,
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

    /// The clip as it plays: keys sorted and inside the duration.
    pub fn clip(&self) -> Result<PropertyClip, String> {
        let mut file = self.file.clone();
        for track in &mut file.tracks {
            for key in &mut track.keys {
                key.time = key.time.clamp(0.0, file.duration.max(0.0));
            }
        }
        for event in &mut file.events {
            event.time = event.time.clamp(0.0, file.duration.max(0.0));
        }
        PropertyClip::from_file(file).map_err(|err| err.0)
    }

    /// What the preview shows at the playhead: the clip, with the pending
    /// moves on top (dropped if the playhead has moved since).
    pub fn evaluated(&mut self) -> Result<Vec<EvaluatedProperty>, String> {
        if self.playing || self.playhead != self.pending_at {
            self.pending.clear();
        }
        let mut evaluated = self.clip()?.evaluate(self.playhead);
        for edit in &self.pending {
            match evaluated
                .iter_mut()
                .find(|e| e.target == edit.target && e.property == edit.property)
            {
                Some(e) => e.value = edit.value.clone(),
                None => evaluated.push(edit.clone()),
            }
        }
        Ok(evaluated)
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

    /// `time` on a whole frame with Snap on (or to the millisecond), inside
    /// the clip.
    fn snapped(&self, time: f64) -> f64 {
        let time = if self.snap && self.fps > 0.0 {
            (time * self.fps).round() / self.fps
        } else {
            (time * 1000.0).round() / 1000.0
        };
        time.clamp(0.0, self.file.duration.max(0.0))
    }

    /// Something was moved in the preview: with `record`, it's keyed at the
    /// playhead; otherwise it shows until the playhead moves.
    pub fn edit(&mut self, edits: Vec<EvaluatedProperty>, record: bool) {
        self.playing = false;
        if record {
            self.key_values(&edits);
            return;
        }
        self.pending_at = self.playhead;
        for edit in edits {
            self.pending
                .retain(|p| !(p.target == edit.target && p.property == edit.property));
            self.pending.push(edit);
        }
    }

    /// Keys every value at the playhead, making the tracks it needs, and
    /// selects those keys.
    pub fn key_values(&mut self, values: &[EvaluatedProperty]) {
        let time = self.snapped(self.playhead);
        let mut keyed = BTreeSet::new();
        for value in values {
            let index = match self
                .file
                .tracks
                .iter()
                .position(|t| t.target == value.target && t.property == value.property)
            {
                Some(index) => index,
                None => {
                    let id = free_id(
                        &value.property.as_str().to_lowercase(),
                        self.file.tracks.iter().map(|t| t.id.as_str()),
                    );
                    self.file.tracks.push(PropertyTrack {
                        id,
                        target: value.target.clone(),
                        property: value.property,
                        keys: Vec::new(),
                    });
                    self.file.tracks.len() - 1
                }
            };
            let track = &mut self.file.tracks[index];
            if track
                .keys
                .first()
                .is_some_and(|key| key.value.len() != value.value.len())
            {
                // A UDim2 key can't go on a Vector3 track (or the reverse).
                continue;
            }
            let id = match track.keys.iter_mut().find(|k| (k.time - time).abs() < 1e-6) {
                Some(key) => {
                    key.value = value.value.clone();
                    key.id.clone()
                }
                None => {
                    let id = free_id("k", track.keys.iter().map(|k| k.id.as_str()));
                    track.keys.push(PropertyKeyframe {
                        id: id.clone(),
                        time,
                        value: value.value.clone(),
                        interp: Interp::Linear,
                    });
                    track.keys.sort_by(|a, b| a.time.total_cmp(&b.time));
                    id
                }
            };
            self.pending
                .retain(|p| !(p.target == value.target && p.property == value.property));
            self.selected_track = Some(track.id.clone());
            keyed.insert((track.id.clone(), id));
        }
        if !keyed.is_empty() {
            self.selected = keyed;
        }
    }

    /// Key selected (K): keys the selected object's tracks, its pending
    /// moves, and where it is (Position and Orientation, or a UI element's
    /// Position and Size).
    pub fn key_focus(&mut self, focus: &Focus) {
        let mut properties: Vec<PropertyName> = focus.transform.clone();
        let more = self
            .file
            .tracks
            .iter()
            .filter(|t| t.target == focus.target)
            .map(|t| t.property)
            .chain(
                self.pending
                    .iter()
                    .filter(|p| p.target == focus.target)
                    .map(|p| p.property),
            );
        for property in more {
            if !properties.contains(&property) {
                properties.push(property);
            }
        }
        let values: Vec<EvaluatedProperty> = properties
            .into_iter()
            .filter_map(|property| {
                self.pending
                    .iter()
                    .chain(&focus.values)
                    .find(|v| v.target == focus.target && v.property == property)
                    .cloned()
            })
            .collect();
        self.key_values(&values);
    }

    pub fn delete_selected(&mut self) {
        for (track_id, key_id) in std::mem::take(&mut self.selected) {
            if let Some(track) = self.file.tracks.iter_mut().find(|t| t.id == track_id) {
                track.keys.retain(|k| k.id != key_id);
            }
        }
    }

    pub fn delete_track(&mut self, id: &str) {
        self.file.tracks.retain(|t| t.id != id);
        self.selected.retain(|(track, _)| track != id);
        if self.selected_track.as_deref() == Some(id) {
            self.selected_track = None;
        }
    }

    /// The selected keys, and their times.
    fn selected_keys(&self) -> Vec<(KeyRef, f64)> {
        self.file
            .tracks
            .iter()
            .flat_map(|t| t.keys.iter().map(move |k| (t, k)))
            .filter(|(t, k)| self.selected.contains(&(t.id.clone(), k.id.clone())))
            .map(|(t, k)| ((t.id.clone(), k.id.clone()), k.time))
            .collect()
    }

    pub fn copy(&mut self) {
        let keys = self.selected_keys();
        let Some(first) = keys.iter().map(|(_, time)| *time).min_by(f64::total_cmp) else {
            return;
        };
        self.clipboard = keys
            .into_iter()
            .filter_map(|((track_id, key_id), time)| {
                let track = self.file.tracks.iter().find(|t| t.id == track_id)?;
                let key = track.keys.iter().find(|k| k.id == key_id)?;
                Some((track_id, time - first, key.clone()))
            })
            .collect();
    }

    pub fn can_paste(&self) -> bool {
        !self.clipboard.is_empty()
    }

    /// Pastes the copied keys at the playhead, on the tracks they came
    /// from, replacing keys at the same times; then selects them.
    pub fn paste(&mut self) {
        let mut pasted = BTreeSet::new();
        for (track_id, offset, key) in self.clipboard.clone() {
            let time = self.snapped(self.playhead + offset);
            let Some(track) = self.file.tracks.iter_mut().find(|t| t.id == track_id) else {
                continue;
            };
            track.keys.retain(|k| (k.time - time).abs() >= 1e-6);
            let id = free_id("k", track.keys.iter().map(|k| k.id.as_str()));
            track.keys.push(PropertyKeyframe {
                id: id.clone(),
                time,
                ..key
            });
            track.keys.sort_by(|a, b| a.time.total_cmp(&b.time));
            pasted.insert((track_id, id));
        }
        if !pasted.is_empty() {
            self.selected = pasted;
        }
    }

    /// After keys moved: sorted again, and where a moved key landed on
    /// another, the moved one stays.
    fn tidy(&mut self) {
        for track in &mut self.file.tracks {
            let moved = |key: &PropertyKeyframe| {
                self.selected.contains(&(track.id.clone(), key.id.clone()))
            };
            let mut keys = std::mem::take(&mut track.keys);
            keys.sort_by(|a, b| a.time.total_cmp(&b.time).then(moved(a).cmp(&moved(b))));
            let mut kept: Vec<PropertyKeyframe> = Vec::with_capacity(keys.len());
            for key in keys {
                if kept
                    .last()
                    .is_some_and(|last| (last.time - key.time).abs() < 1e-6)
                {
                    *kept.last_mut().expect("just checked") = key;
                } else {
                    kept.push(key);
                }
            }
            track.keys = kept;
        }
        let tracks = &self.file.tracks;
        self.selected.retain(|(track_id, key_id)| {
            tracks
                .iter()
                .any(|t| t.id == *track_id && t.keys.iter().any(|k| k.id == *key_id))
        });
    }

    /// The toolbar's part: play/pause, back to the start, Stop and Loop.
    pub fn controls(&mut self, ui: &mut egui::Ui) {
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
            .on_hover_text("Stop and go back to the start")
            .clicked()
        {
            self.playing = false;
            self.playhead = 0.0;
        }
        ui.checkbox(&mut self.file.looping, "Loop");
        ui.monospace(format!("{:.2} s", self.playhead));
    }

    /// The drawer: the copy's objects as a tree, each with its tracks.
    /// Returns the object clicked.
    pub fn objects(&mut self, ui: &mut egui::Ui, preview: &ObjectPreview) -> Option<InstanceId> {
        let Some(root) = preview.root() else {
            ui.weak("Select a Part, Model or ScreenGui in the Explorer.");
            return None;
        };
        let world = preview.world();
        let mut picked = None;
        let mut delete = None;
        let mut shown = BTreeSet::new();
        for (id, depth) in preview.tree() {
            let Some(instance) = world.get(id) else {
                continue;
            };
            let Some(target) = target_path(world, root, id) else {
                continue;
            };
            ui.horizontal(|ui| {
                ui.add_space(depth as f32 * 12.0);
                let text = format!(
                    "{} {}",
                    theme::class_icon(instance.class()),
                    instance.name()
                );
                let label = egui::Button::selectable(preview.selected == Some(id), text).truncate();
                if ui.add(label).clicked() {
                    picked = Some(id);
                }
            });
            for track in self.file.tracks.iter().filter(|t| t.target == target) {
                shown.insert(track.id.clone());
                ui.horizontal(|ui| {
                    ui.add_space(depth as f32 * 12.0 + 18.0);
                    self.track_label(ui, track, &mut delete);
                });
            }
        }
        let lost: Vec<&PropertyTrack> = self
            .file
            .tracks
            .iter()
            .filter(|t| !shown.contains(&t.id))
            .collect();
        if !lost.is_empty() {
            ui.add_space(4.0);
            ui.weak("Not in this object:");
            for track in lost {
                ui.horizontal(|ui| {
                    ui.add_space(6.0);
                    ui.weak(&track.target);
                    self.track_label(ui, track, &mut delete);
                });
            }
        }
        if let Some(id) = delete {
            self.delete_track(&id);
        }
        picked
    }

    fn track_label(&self, ui: &mut egui::Ui, track: &PropertyTrack, delete: &mut Option<String>) {
        let selected = self.selected_track.as_deref() == Some(track.id.as_str());
        let text = format!("{} {}", icons::KEY, track.property.as_str());
        ui.add(egui::Button::selectable(selected, text).truncate())
            .on_hover_text(format!("{} keys", track.keys.len()))
            .context_menu(|ui| {
                if ui
                    .button(format!("{} Delete track", icons::TRASH))
                    .clicked()
                {
                    *delete = Some(track.id.clone());
                    ui.close();
                }
            });
    }

    /// Add track, Key selected, copy, paste, delete and snap, then the
    /// dope sheet. Returns true if Key selected was pressed.
    pub fn keys(&mut self, ui: &mut egui::Ui, focus: Option<&Focus>) -> bool {
        let mut key = false;
        crate::responsive::toolbar(ui, |ui| {
            let addable: Vec<&EvaluatedProperty> = focus
                .map(|focus| {
                    focus
                        .values
                        .iter()
                        .filter(|v| {
                            !self
                                .file
                                .tracks
                                .iter()
                                .any(|t| t.target == v.target && t.property == v.property)
                        })
                        .collect()
                })
                .unwrap_or_default();
            ui.add_enabled_ui(!addable.is_empty(), |ui| {
                ui.menu_button(format!("{} Add track", icons::PLUS), |ui| {
                    for value in &addable {
                        if ui.button(value.property.as_str()).clicked() {
                            self.key_values(std::slice::from_ref(*value));
                            ui.close();
                        }
                    }
                })
                .response
                .on_disabled_hover_text("Select something in the preview to add its tracks");
            });
            if ui
                .add_enabled(
                    focus.is_some(),
                    egui::Button::new(format!("{} Key selected", icons::KEY)),
                )
                .on_hover_text("Key the selected object at the playhead (K)")
                .clicked()
            {
                key = true;
            }
            let any = !self.selected.is_empty();
            if ui
                .add_enabled(any, egui::Button::new(icons::COPY))
                .on_hover_text("Copy the selected keys (Ctrl+C)")
                .clicked()
            {
                self.copy();
            }
            if ui
                .add_enabled(self.can_paste(), egui::Button::new(icons::CLIPBOARD))
                .on_hover_text("Paste at the playhead (Ctrl+V)")
                .clicked()
            {
                self.paste();
            }
            if ui
                .add_enabled(any, egui::Button::new(icons::TRASH))
                .on_hover_text("Delete the selected keys (Delete)")
                .clicked()
            {
                self.delete_selected();
            }
            ui.checkbox(&mut self.snap, "Snap")
                .on_hover_text("Put keys and the playhead on whole frames");
            if self.snap {
                ui.add(
                    egui::DragValue::new(&mut self.fps)
                        .range(1.0..=240.0)
                        .speed(1.0)
                        .suffix(" fps"),
                );
            }
        });
        self.dope_sheet(ui);
        key
    }

    /// Delete, K, Ctrl+C, Ctrl+X and Ctrl+V, while the mouse is over the
    /// Animation tab. Returns true if K (Key selected) was pressed.
    pub fn shortcuts(&mut self, ui: &egui::Ui) -> bool {
        let (delete, key, copy, cut, paste) = ui.input(|input| {
            let command = input.modifiers.command;
            let plain = input.modifiers.is_none();
            (
                plain
                    && (input.key_pressed(egui::Key::Delete)
                        || input.key_pressed(egui::Key::Backspace)),
                plain && input.key_pressed(egui::Key::K),
                command && input.key_pressed(egui::Key::C),
                command && input.key_pressed(egui::Key::X),
                command && input.key_pressed(egui::Key::V),
            )
        });
        if copy || cut {
            self.copy();
        }
        if delete || cut {
            self.delete_selected();
        }
        if paste {
            self.paste();
        }
        key
    }

    /// The clip's settings: its name, duration, the selected key and the
    /// events. Returns true if Rename was pressed.
    pub fn inspector(&mut self, ui: &mut egui::Ui, root: Option<&str>) -> bool {
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
        if root.is_none() {
            ui.weak("Select a Part, Model or ScreenGui in the Explorer to animate a copy of it.");
        }
        ui.add_space(4.0);
        crate::responsive::form(ui, "property clip settings", |ui| {
            ui.label("Duration");
            let changed = ui
                .add(
                    egui::DragValue::new(&mut self.file.duration)
                        .speed(0.05)
                        .range(0.0..=3600.0)
                        .suffix(" s"),
                )
                .changed();
            if changed {
                // Keys past the new end move to it.
                let end = self.file.duration;
                for key in self.file.tracks.iter_mut().flat_map(|t| &mut t.keys) {
                    key.time = key.time.min(end);
                }
                for event in &mut self.file.events {
                    event.time = event.time.min(end);
                }
            }
            ui.end_row();
        });

        let one = (self.selected.len() == 1)
            .then(|| self.selected.first().cloned())
            .flatten();
        if let Some((track_id, key_id)) = one
            && let Some(track) = self.file.tracks.iter_mut().find(|t| t.id == track_id)
            && let Some(key) = track.keys.iter_mut().find(|k| k.id == key_id)
        {
            ui.separator();
            ui.strong(format!("Key · {}", track.property.as_str()));
            ui.weak(&track.target);
            let duration = self.file.duration;
            let property = track.property;
            crate::responsive::form(ui, "property key", |ui| {
                ui.label("Time");
                ui.add(egui::DragValue::new(&mut key.time).speed(0.01).suffix(" s"));
                key.time = key.time.clamp(0.0, duration);
                ui.end_row();
                for (i, label) in value_labels(property, key.value.len()).iter().enumerate() {
                    ui.label(*label);
                    let unit = matches!(
                        property,
                        PropertyName::Color
                            | PropertyName::BackgroundColor3
                            | PropertyName::TextColor3
                            | PropertyName::Transparency
                            | PropertyName::BackgroundTransparency
                            | PropertyName::ImageTransparency
                    );
                    let mut drag = egui::DragValue::new(&mut key.value[i]).speed(0.1);
                    if unit {
                        drag = drag.speed(0.01).range(0.0..=1.0);
                    }
                    ui.add(drag);
                    ui.end_row();
                }
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
        } else if self.selected.len() > 1 {
            ui.separator();
            ui.weak(format!(
                "{} keys selected. Drag one to move them all.",
                self.selected.len()
            ));
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
                self.selected.clear();
                self.playhead = 0.0;
            }
        });
        rename
    }

    fn dope_sheet(&mut self, ui: &mut egui::Ui) {
        let palette = theme::palette(ui.ctx());
        let duration = self.file.duration.max(0.001);
        let rows = self.file.tracks.len().max(1);
        let height = RULER + ROW * rows as f32 + 4.0;
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), height),
            egui::Sense::click_and_drag(),
        );
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, palette.inset);

        let label_w = (rect.width() * 0.3).clamp(70.0, 160.0);
        let timeline = egui::Rect::from_min_max(
            egui::pos2(rect.left() + label_w + 6.0, rect.top()),
            egui::pos2(rect.right() - 8.0, rect.bottom()),
        );
        let width = timeline.width().max(1.0);
        let x_of = |t: f64| timeline.left() + (t / duration) as f32 * width;
        let t_of =
            |x: f32| (((x - timeline.left()) / width) as f64 * duration).clamp(0.0, duration);
        let rows_used = self.file.tracks.len();
        let row_of = |y: f32| {
            let row = ((y - rect.top() - RULER) / ROW).floor();
            (row >= 0.0 && (row as usize) < rows_used).then_some(row as usize)
        };
        let key_at = |tracks: &[PropertyTrack], pos: egui::Pos2| -> Option<KeyRef> {
            let track = tracks.get(row_of(pos.y)?)?;
            track
                .keys
                .iter()
                .map(|k| (k, (x_of(k.time) - pos.x).abs()))
                .filter(|(_, dx)| *dx <= GRAB)
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(k, _)| (track.id.clone(), k.id.clone()))
        };

        // Ruler: a tick every "nice" step, and frames when they're far apart.
        let step = nice_step(duration, width);
        let mut t = 0.0;
        while t <= duration + 1e-9 {
            let x = x_of(t);
            painter.line_segment(
                [
                    egui::pos2(x, rect.top() + 12.0),
                    egui::pos2(x, rect.bottom()),
                ],
                egui::Stroke::new(1.0_f32, palette.border),
            );
            painter.text(
                egui::pos2(x + 2.0, rect.top() + 1.0),
                egui::Align2::LEFT_TOP,
                format!("{t:.2}")
                    .trim_end_matches('0')
                    .trim_end_matches('.')
                    .to_owned(),
                egui::FontId::proportional(10.0),
                palette.text_weak,
            );
            t += step;
        }
        if self.snap && self.fps > 0.0 && width / (duration * self.fps) as f32 > 6.0 {
            let frames = (duration * self.fps).floor() as usize;
            for frame in 0..=frames {
                let x = x_of(frame as f64 / self.fps);
                painter.line_segment(
                    [
                        egui::pos2(x, rect.top() + RULER - 4.0),
                        egui::pos2(x, rect.top() + RULER),
                    ],
                    egui::Stroke::new(1.0_f32, palette.border_strong),
                );
            }
        }

        for (row, track) in self.file.tracks.iter().enumerate() {
            let y = rect.top() + RULER + row as f32 * ROW;
            let row_rect = egui::Rect::from_min_size(
                egui::pos2(rect.left(), y),
                egui::vec2(rect.width(), ROW),
            );
            if self.selected_track.as_deref() == Some(track.id.as_str()) {
                painter.rect_filled(row_rect, 0.0, palette.accent_soft);
            } else if row % 2 == 1 {
                painter.rect_filled(row_rect, 0.0, palette.surface.gamma_multiply(0.4));
            }
            let name = match track.target.as_str() {
                "." | "" => "(root)",
                path => path.rsplit('/').next().unwrap_or(path),
            };
            let label_rect = egui::Rect::from_min_size(
                egui::pos2(rect.left() + 4.0, y),
                egui::vec2(label_w, ROW),
            );
            painter.with_clip_rect(label_rect).text(
                egui::pos2(label_rect.left(), y + ROW / 2.0),
                egui::Align2::LEFT_CENTER,
                format!("{name} · {}", track.property.as_str()),
                egui::FontId::proportional(12.0),
                palette.text,
            );
            for key in &track.keys {
                let center = egui::pos2(x_of(key.time), y + ROW / 2.0);
                let selected = self.selected.contains(&(track.id.clone(), key.id.clone()));
                let (fill, r) = if selected {
                    (palette.accent, 6.0)
                } else {
                    (palette.text_secondary, 5.0)
                };
                let diamond = vec![
                    center + egui::vec2(0.0, -r),
                    center + egui::vec2(r, 0.0),
                    center + egui::vec2(0.0, r),
                    center + egui::vec2(-r, 0.0),
                ];
                painter.add(egui::Shape::convex_polygon(
                    diamond,
                    fill,
                    egui::Stroke::new(1.0_f32, palette.inset),
                ));
            }
        }
        if self.file.tracks.is_empty() {
            painter.text(
                egui::pos2(timeline.left(), rect.top() + RULER + ROW / 2.0),
                egui::Align2::LEFT_CENTER,
                "No tracks yet: select something in the preview, move it, and press K.",
                egui::FontId::proportional(12.0),
                palette.text_weak,
            );
        }

        // Interaction.
        let (modifiers, origin) = ui.input(|i| (i.modifiers, i.pointer.press_origin()));
        let add = modifiers.shift || modifiers.command;
        if response.drag_started()
            && let Some(origin) = origin
        {
            self.gesture = if origin.y < rect.top() + RULER {
                Some(Gesture::Scrub)
            } else if let Some(hit) = key_at(&self.file.tracks, origin) {
                if !self.selected.contains(&hit) {
                    if !add {
                        self.selected.clear();
                    }
                    self.selected.insert(hit.clone());
                }
                let start = self.selected_keys();
                let anchor = start
                    .iter()
                    .find(|(key, _)| *key == hit)
                    .map_or(0.0, |(_, time)| *time);
                Some(Gesture::Keys {
                    from: t_of(origin.x),
                    anchor,
                    start,
                })
            } else if origin.x >= timeline.left() {
                Some(Gesture::Box { from: origin, add })
            } else {
                None
            };
        }
        if let Some(pointer) = response.interact_pointer_pos() {
            match &self.gesture {
                Some(Gesture::Scrub) if response.dragged() => {
                    self.playhead = self.snapped(t_of(pointer.x));
                    self.playing = false;
                }
                Some(Gesture::Keys {
                    from,
                    anchor,
                    start,
                }) if response.dragged() => {
                    let moved = self.snapped(anchor + t_of(pointer.x) - from) - anchor;
                    let end = self.file.duration;
                    for ((track_id, key_id), time) in start.clone() {
                        if let Some(key) = self
                            .file
                            .tracks
                            .iter_mut()
                            .find(|t| t.id == track_id)
                            .and_then(|t| t.keys.iter_mut().find(|k| k.id == key_id))
                        {
                            key.time = (time + moved).clamp(0.0, end);
                        }
                    }
                }
                Some(Gesture::Box { from, .. }) => {
                    let area = egui::Rect::from_two_pos(*from, pointer);
                    painter.rect(
                        area,
                        0.0,
                        palette.accent_soft,
                        egui::Stroke::new(1.0_f32, palette.accent),
                        egui::StrokeKind::Inside,
                    );
                }
                _ => {}
            }
        }
        if response.drag_stopped() {
            match self.gesture.take() {
                Some(Gesture::Keys { .. }) => self.tidy(),
                Some(Gesture::Box { from, add }) => {
                    if let Some(to) = response.interact_pointer_pos() {
                        let area = egui::Rect::from_two_pos(from, to);
                        if !add {
                            self.selected.clear();
                        }
                        for (row, track) in self.file.tracks.iter().enumerate() {
                            let y = rect.top() + RULER + row as f32 * ROW + ROW / 2.0;
                            for key in &track.keys {
                                if area.contains(egui::pos2(x_of(key.time), y)) {
                                    self.selected.insert((track.id.clone(), key.id.clone()));
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        if response.clicked()
            && let Some(pos) = response.interact_pointer_pos()
        {
            if let Some(row) = row_of(pos.y) {
                self.selected_track = Some(self.file.tracks[row].id.clone());
            }
            match key_at(&self.file.tracks, pos) {
                Some(hit) if add => {
                    if !self.selected.remove(&hit) {
                        self.selected.insert(hit);
                    }
                }
                Some(hit) => {
                    self.selected = BTreeSet::from([hit]);
                }
                None => {
                    if !add {
                        self.selected.clear();
                    }
                    if pos.x >= timeline.left() {
                        self.playhead = self.snapped(t_of(pos.x));
                        self.playing = false;
                    }
                }
            }
        }

        // Playhead.
        let px = x_of(self.playhead.clamp(0.0, duration));
        painter.line_segment(
            [egui::pos2(px, rect.top()), egui::pos2(px, rect.bottom())],
            egui::Stroke::new(2.0_f32, palette.error),
        );
    }
}

/// The first of `prefix1`, `prefix2`, … not in `taken`.
fn free_id<'a>(prefix: &str, taken: impl Iterator<Item = &'a str> + Clone) -> String {
    (1..)
        .map(|n| format!("{prefix}{n}"))
        .find(|id| !taken.clone().any(|t| t == id))
        .expect("some id is free")
}

/// A ruler step (seconds) that leaves labels at least 48 points apart.
fn nice_step(duration: f64, width: f32) -> f64 {
    let fits = (width / 48.0).max(1.0) as f64;
    [0.05, 0.1, 0.25, 0.5, 1.0, 2.0, 5.0, 10.0, 30.0, 60.0, 300.0]
        .into_iter()
        .find(|step| duration / step <= fits)
        .unwrap_or(duration)
}

/// What each number of a key is called in the inspector.
fn value_labels(property: PropertyName, width: usize) -> &'static [&'static str] {
    match (property, width) {
        (_, 4) => &["X scale", "X offset", "Y scale", "Y offset"],
        (_, 1) => &["Value"],
        (PropertyName::Color | PropertyName::BackgroundColor3 | PropertyName::TextColor3, _) => {
            &["R", "G", "B"]
        }
        _ => &["X", "Y", "Z"],
    }
}

/// Empty property clip for "New property clip".
pub fn new_property_clip(duration: f64) -> ClipFile {
    ClipFile::new(duration)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value(target: &str, property: PropertyName, value: &[f32]) -> EvaluatedProperty {
        EvaluatedProperty {
            target: target.into(),
            property,
            value: value.to_vec(),
        }
    }

    fn doc() -> PropertyDoc {
        PropertyDoc::open(
            "assets/animations/Door.anim.json".into(),
            ClipFile::new(2.0),
        )
    }

    #[test]
    fn recording_a_move_makes_a_track_and_a_key_at_the_playhead() {
        let mut doc = doc();
        doc.playhead = 1.0;
        let turn = value("Hinge", PropertyName::Orientation, &[0.0, 90.0, 0.0]);
        doc.edit(vec![turn.clone()], true);
        assert_eq!(doc.file.tracks.len(), 1);
        let track = &doc.file.tracks[0];
        assert_eq!(
            (track.target.as_str(), track.property),
            ("Hinge", PropertyName::Orientation)
        );
        assert_eq!(track.keys[0].time, 1.0);
        assert_eq!(doc.selected.len(), 1);
        // Moving it again at the same time updates that key.
        doc.edit(
            vec![value("Hinge", PropertyName::Orientation, &[0.0, 45.0, 0.0])],
            true,
        );
        assert_eq!(doc.file.tracks[0].keys.len(), 1);
        assert_eq!(doc.file.tracks[0].keys[0].value, [0.0, 45.0, 0.0]);
    }

    #[test]
    fn a_move_without_record_shows_until_the_playhead_moves_or_k() {
        let mut doc = doc();
        doc.edit(vec![value(".", PropertyName::Transparency, &[0.5])], false);
        assert!(doc.file.tracks.is_empty());
        let shown = doc.evaluated().unwrap();
        assert_eq!(shown[0].value, [0.5]);

        // K keys it (with where the object is).
        let focus = Focus {
            target: ".".into(),
            name: "Door".into(),
            values: vec![
                value(".", PropertyName::Position, &[0.0, 1.0, 0.0]),
                value(".", PropertyName::Orientation, &[0.0, 0.0, 0.0]),
                value(".", PropertyName::Transparency, &[0.0]),
            ],
            transform: vec![PropertyName::Position, PropertyName::Orientation],
        };
        doc.key_focus(&focus);
        let keyed: Vec<PropertyName> = doc.file.tracks.iter().map(|t| t.property).collect();
        assert_eq!(
            keyed,
            [
                PropertyName::Position,
                PropertyName::Orientation,
                PropertyName::Transparency
            ]
        );
        assert_eq!(doc.file.tracks[2].keys[0].value, [0.5]);

        doc.edit(vec![value(".", PropertyName::Transparency, &[0.9])], false);
        doc.playhead = 1.0;
        let shown = doc.evaluated().unwrap();
        let transparency = shown
            .iter()
            .find(|v| v.property == PropertyName::Transparency)
            .unwrap();
        assert_eq!(transparency.value, [0.5]);
    }

    #[test]
    fn copy_paste_delete_and_snapped_keys() {
        let mut doc = doc();
        doc.playhead = 0.51;
        doc.key_values(&[value(".", PropertyName::Position, &[0.0, 0.0, 0.0])]);
        // 0.51 s snaps to frame 15 at 30 fps.
        assert_eq!(doc.file.tracks[0].keys[0].time, 0.5);
        doc.copy();
        doc.playhead = 1.0;
        doc.paste();
        assert_eq!(doc.file.tracks[0].keys.len(), 2);
        assert_eq!(doc.file.tracks[0].keys[1].time, 1.0);
        assert_eq!(doc.selected.len(), 1);
        doc.delete_selected();
        assert_eq!(doc.file.tracks[0].keys.len(), 1);
    }

    #[test]
    fn a_moved_key_landing_on_another_replaces_it() {
        let mut doc = doc();
        for (time, y) in [(0.0, 0.0), (1.0, 5.0)] {
            doc.playhead = time;
            doc.key_values(&[value(".", PropertyName::Position, &[0.0, y, 0.0])]);
        }
        // The key at 1 s is selected; drag it onto the one at 0 s.
        doc.file.tracks[0].keys[1].time = 0.0;
        doc.tidy();
        let keys = &doc.file.tracks[0].keys;
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].value, [0.0, 5.0, 0.0]);
        assert_eq!(doc.selected.len(), 1);
    }

    #[test]
    fn ui_keys_stay_off_vector_tracks() {
        let mut doc = doc();
        doc.key_values(&[value(
            "Menu",
            PropertyName::Position,
            &[0.0, 10.0, 0.0, 20.0],
        )]);
        doc.key_values(&[value("Menu", PropertyName::Position, &[1.0, 2.0, 3.0])]);
        assert_eq!(doc.file.tracks[0].keys[0].value.len(), 4);
        assert!(doc.clip().is_ok());
    }
}
