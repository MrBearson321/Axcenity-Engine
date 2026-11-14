//! Character Studio's Lip sync section: a `*.lipsync.json` on a timeline
//! over its sound's waveform. Visemes are picked, dragged to a new time,
//! changed, added and deleted; the play head plays them on the face
//! (without the sound); Make from makes one from a sound or phoneme
//! timings (as the Console's `lipsync` does), and Save writes it as one
//! undo step.

use std::path::Path;

use axcenity_anim::{
    CompiledVisemes, LipTiming, default_viseme_map, find_viseme_map, viseme_weights,
};
use axcenity_assets::{Assets, LIP_SYNC_EXTENSION, LipSync, VisemeKey};
use axcenity_core::face::{FACE_CONTROLS, VISEMES};
use axcenity_core::{Instance, Value};
use eframe::egui;

use super::{StudioAction, project_files};
use crate::theme::{self, icons};
use crate::widgets;

/// The closest two visemes can be, in seconds.
const NEAREST: f64 = 0.01;

/// What a lip sync can be made from: sounds and phoneme timings.
pub const SOURCES: [&str; 7] = [".ogg", ".wav", ".mp3", ".flac", ".lab", ".tsv", ".txt"];

/// A sound's loudness across the timeline.
struct Waveform {
    sound: String,
    /// The loudest sample in each slice, 0 to 1.
    peaks: Vec<f32>,
    seconds: f64,
}

/// The Lip sync section's state.
#[derive(Default)]
pub struct LipSyncEditor {
    /// The file open, if any.
    pub path: Option<String>,
    pub lip: LipSync,
    /// Changed since it was opened or saved.
    pub dirty: bool,
    pub selected: Option<usize>,
    /// The play head, in seconds.
    pub time: f64,
    pub playing: bool,
    waveform: Option<Waveform>,
    /// The project's lip syncs and what one can be made from.
    files: Option<(Vec<String>, Vec<String>)>,
    /// A viseme being dragged to a new time.
    drag: Option<usize>,
    scrubbing: bool,
    pub error: Option<String>,
}

/// Moves key `index` to `time`, kept between its neighbours and inside
/// the line.
pub fn retime(lip: &mut LipSync, index: usize, time: f64) {
    let low = index
        .checked_sub(1)
        .and_then(|before| lip.visemes.get(before))
        .map_or(0.0, |key| key.time + NEAREST);
    let high = lip
        .visemes
        .get(index + 1)
        .map_or(lip.duration.max(low), |key| key.time - NEAREST);
    if let Some(key) = lip.visemes.get_mut(index) {
        key.time = time.clamp(low, high.max(low));
    }
}

/// Adds `viseme` at `time` in time order (replacing a key already there)
/// and returns where it is.
pub fn add_key(lip: &mut LipSync, time: f64, viseme: &str) -> usize {
    let time = time.clamp(0.0, lip.duration.max(0.0));
    if let Some(at) = lip
        .visemes
        .iter()
        .position(|key| (key.time - time).abs() < NEAREST)
    {
        lip.visemes[at].viseme = viseme.to_owned();
        return at;
    }
    let at = lip.visemes.partition_point(|key| key.time < time);
    lip.visemes.insert(
        at,
        VisemeKey {
            time,
            viseme: viseme.to_owned(),
            weight: 1.0,
        },
    );
    at
}

/// The key being said at `time`: the last one at or before it.
pub fn key_at(lip: &LipSync, time: f64) -> Option<usize> {
    lip.visemes
        .partition_point(|key| key.time <= time)
        .checked_sub(1)
}

/// The loudest sample in each of `slices` slices of `samples`, 0 to 1.
pub fn peaks(samples: &[f32], slices: usize) -> Vec<f32> {
    if samples.is_empty() || slices == 0 {
        return Vec::new();
    }
    let per = samples.len().div_ceil(slices).max(1);
    samples
        .chunks(per)
        .map(|chunk| {
            chunk
                .iter()
                .fold(0.0_f32, |most, s| most.max(s.abs()))
                .min(1.0)
        })
        .collect()
}

impl LipSyncEditor {
    /// Opens `lip` as the file at `path`.
    pub fn open(&mut self, path: &str, lip: LipSync) {
        self.path = Some(path.to_owned());
        self.lip = lip;
        self.dirty = false;
        self.selected = None;
        self.time = 0.0;
        self.playing = false;
        self.drag = None;
        self.error = None;
    }

    /// Reads the project's files again next time they're listed.
    pub fn rescan(&mut self) {
        self.files = None;
    }

    /// Moves the play head on by `seconds` while playing. Returns whether
    /// it's still playing.
    pub fn advance(&mut self, seconds: f64) -> bool {
        if !self.playing {
            return false;
        }
        self.time += seconds;
        if self.time >= self.lip.duration {
            self.time = self.lip.duration;
            self.playing = false;
        }
        self.playing
    }

    /// The face's controls at the play head, through the FaceControls'
    /// viseme map and timing (or the defaults).
    pub fn controls(&self, face: Option<&Instance>, assets: &Assets) -> Vec<f32> {
        let mut controls = vec![0.0; FACE_CONTROLS.len()];
        if self.lip.visemes.is_empty() {
            return controls;
        }
        let number = |name: &str| {
            face.and_then(|face| face.property(name))
                .and_then(Value::as_number)
        };
        let map_name = face
            .and_then(|face| face.property("VisemeMap"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        let maps = assets.viseme_maps();
        let map = if map_name.is_empty() {
            default_viseme_map()
        } else {
            find_viseme_map(&maps, map_name).unwrap_or_else(|| default_viseme_map())
        };
        let (compiled, _) = CompiledVisemes::new(map);
        let defaults = LipTiming::default();
        let timing = LipTiming {
            smoothing: number("LipSyncSmoothing").unwrap_or(defaults.smoothing),
            anticipation: number("LipSyncAnticipation").unwrap_or(defaults.anticipation),
        };
        let weights = viseme_weights(&self.lip, self.time, timing);
        compiled.apply(&weights, &mut controls);
        controls
    }

    fn files(&mut self, folder: Option<&Path>) -> &(Vec<String>, Vec<String>) {
        self.files.get_or_insert_with(|| match folder {
            Some(folder) => (
                project_files(folder, &[LIP_SYNC_EXTENSION]),
                project_files(folder, &SOURCES),
            ),
            None => (Vec::new(), Vec::new()),
        })
    }

    /// Reads the waveform of the lip sync's sound, once per sound.
    fn load_waveform(&mut self, folder: Option<&Path>) {
        let sound = self.lip.sound.clone();
        if self.waveform.as_ref().map(|w| w.sound.as_str()) == Some(sound.as_str()) {
            return;
        }
        let read = || -> Option<Waveform> {
            let file = folder?.join(axcenity_assets::safe_path(&sound)?);
            let bytes = std::fs::read(file).ok()?;
            let (samples, rate) = axcenity_audio::decode_mono(&sound, bytes).ok()?;
            Some(Waveform {
                sound: sound.clone(),
                peaks: peaks(&samples, 1_200),
                seconds: samples.len() as f64 / f64::from(rate.max(1)),
            })
        };
        self.waveform = Some(read().unwrap_or(Waveform {
            sound,
            peaks: Vec::new(),
            seconds: 0.0,
        }));
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, folder: Option<&Path>) -> Option<StudioAction> {
        let palette = theme::palette(ui.ctx());
        let mut action = None;
        let (lip_syncs, sources) = self.files(folder).clone();
        let mut open = None;
        crate::responsive::toolbar(ui, |ui| {
            let shown = self.path.as_deref().unwrap_or("Open a lip sync");
            egui::ComboBox::from_id_salt("lip-sync-file")
                .selected_text(shown)
                .width(200.0)
                .show_ui(ui, |ui| {
                    if lip_syncs.is_empty() {
                        ui.label("No .lipsync.json files in the project yet.");
                    }
                    for path in &lip_syncs {
                        if ui
                            .selectable_label(self.path.as_ref() == Some(path), path)
                            .clicked()
                        {
                            open = Some(path.clone());
                        }
                    }
                });
            widgets::menu_button(ui, Some(icons::MAGIC_WAND), "Make from", |ui| {
                if sources.is_empty() {
                    ui.label("No sounds or phoneme timings in the project.");
                }
                for source in &sources {
                    if ui.button(source).clicked() {
                        action = Some(StudioAction::MakeLipSync {
                            source: source.clone(),
                        });
                        ui.close();
                    }
                }
            })
            .response
            .on_hover_text(
                "Make a lip sync beside a sound (from how it sounds) or a phoneme timings file \
                 (start end phoneme per line)",
            );
            if widgets::icon_button(
                ui,
                icons::ARROWS_CLOCKWISE,
                "List the project's files again",
                false,
            )
            .clicked()
            {
                self.files = None;
            }
            let save = egui::Button::new(theme::with_icon(icons::FLOPPY_DISK, "Save"));
            if ui
                .add_enabled(self.dirty && self.path.is_some(), save)
                .on_hover_text("Save the lip sync (one undo step)")
                .clicked()
                && let Some(path) = &self.path
            {
                action = Some(StudioAction::SaveLipSync {
                    path: path.clone(),
                    lip_sync: self.lip.clone(),
                });
            }
        });
        if let Some(path) = open {
            match folder.map(|folder| axcenity_assets::read_lip_sync(folder, &path)) {
                Some(Ok(lip)) => self.open(&path, lip),
                Some(Err(why)) => self.error = Some(why),
                None => {}
            }
        }
        if let Some(error) = &self.error {
            ui.colored_label(palette.error, format!("{} {error}", icons::WARNING));
        }
        if self.path.is_none() {
            ui.label(
                egui::RichText::new(
                    "Open a lip sync, or make one from a line's sound. It plays on the face \
                     here; in a game, face:PlayLipSync(path, sound) or a voiced Dialogue line \
                     plays it.",
                )
                .color(palette.text_weak),
            );
            return action;
        }
        self.load_waveform(folder);

        crate::responsive::toolbar(ui, |ui| {
            let (icon, tip) = if self.playing {
                (icons::PAUSE, "Pause")
            } else {
                (
                    icons::PLAY,
                    "Play on the face (the sound doesn't play here)",
                )
            };
            if widgets::icon_button(ui, icon, tip, false).clicked() {
                if !self.playing && self.time >= self.lip.duration {
                    self.time = 0.0;
                }
                self.playing = !self.playing;
            }
            if widgets::icon_button(ui, icons::SKIP_BACK, "Back to the start", false).clicked() {
                self.time = 0.0;
            }
            ui.label(format!("{:.2} / {:.2} s", self.time, self.lip.duration));
            ui.label("Length");
            let mut duration = self.lip.duration;
            if ui
                .add(
                    egui::DragValue::new(&mut duration)
                        .speed(0.01)
                        .range(0.0..=600.0)
                        .suffix(" s"),
                )
                .changed()
            {
                self.lip.duration = duration;
                self.dirty = true;
            }
            if self.dirty {
                ui.label(egui::RichText::new("Not saved").color(palette.warning));
            }
        });
        if !self.lip.sound.is_empty() {
            ui.label(
                egui::RichText::new(format!("{} {}", icons::SPEAKER_HIGH, self.lip.sound))
                    .small()
                    .color(palette.text_weak),
            );
        }
        self.timeline(ui);
        self.key_detail(ui);
        action
    }

    /// The waveform and the visemes as blocks, with the play head.
    fn timeline(&mut self, ui: &mut egui::Ui) {
        let palette = theme::palette(ui.ctx());
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(ui.available_width().max(1.0), 120.0),
            egui::Sense::click_and_drag(),
        );
        let duration = self.lip.duration.max(0.01);
        let x_of = |time: f64| rect.left() + (time / duration) as f32 * rect.width();
        let time_of = |x: f32| ((x - rect.left()) / rect.width()) as f64 * duration;
        let wave = egui::Rect::from_min_max(rect.min, egui::pos2(rect.right(), rect.top() + 70.0));
        let strip =
            egui::Rect::from_min_max(egui::pos2(rect.left(), wave.bottom() + 4.0), rect.max);

        // Picking, dragging and scrubbing.
        let pointer = response.interact_pointer_pos();
        if response.drag_started()
            && let Some(at) = pointer
        {
            if strip.contains(at) {
                let index = key_at(&self.lip, time_of(at.x));
                self.selected = index;
                self.drag = index.filter(|&i| i > 0 || self.lip.visemes.len() == 1);
            } else {
                self.scrubbing = true;
            }
        }
        if response.dragged()
            && let Some(at) = pointer
        {
            if let Some(index) = self.drag {
                retime(&mut self.lip, index, time_of(at.x));
                self.dirty = true;
            } else if self.scrubbing {
                self.time = time_of(at.x).clamp(0.0, self.lip.duration);
            }
        }
        if response.drag_stopped() {
            self.drag = None;
            self.scrubbing = false;
        }
        if response.clicked()
            && let Some(at) = pointer
        {
            if strip.contains(at) {
                self.selected = key_at(&self.lip, time_of(at.x));
            } else {
                self.time = time_of(at.x).clamp(0.0, self.lip.duration);
            }
        }
        if response.double_clicked()
            && let Some(at) = pointer
            && strip.contains(at)
        {
            let viseme = self
                .selected
                .and_then(|i| self.lip.visemes.get(i))
                .map_or("AI".to_owned(), |key| key.viseme.clone());
            self.selected = Some(add_key(&mut self.lip, time_of(at.x), &viseme));
            self.dirty = true;
        }

        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, theme::radius::CONTROL, palette.inset);
        // The waveform, scaled to the line's length.
        if let Some(waveform) = self.waveform.as_ref().filter(|w| !w.peaks.is_empty()) {
            let middle = wave.center().y;
            let slices = waveform.peaks.len();
            let mut x = wave.left();
            while x < wave.right() {
                let time = time_of(x);
                let slice = (time / waveform.seconds.max(1e-6) * slices as f64) as usize;
                if let Some(peak) = waveform.peaks.get(slice) {
                    let half = peak * wave.height() * 0.48;
                    painter.line_segment(
                        [egui::pos2(x, middle - half), egui::pos2(x, middle + half)],
                        egui::Stroke::new(1.0, palette.text_muted),
                    );
                }
                x += 1.0;
            }
        } else {
            painter.text(
                wave.center(),
                egui::Align2::CENTER_CENTER,
                if self.lip.sound.is_empty() {
                    "No sound named"
                } else {
                    "The sound can't be read"
                },
                egui::FontId::proportional(12.0),
                palette.text_weak,
            );
        }
        // The visemes, each until the next.
        for (index, key) in self.lip.visemes.iter().enumerate() {
            let end = self
                .lip
                .visemes
                .get(index + 1)
                .map_or(self.lip.duration, |next| next.time);
            let block = egui::Rect::from_min_max(
                egui::pos2(x_of(key.time), strip.top()),
                egui::pos2(x_of(end).max(x_of(key.time) + 1.0), strip.bottom()),
            );
            let selected = self.selected == Some(index);
            let fill = if selected {
                palette.accent_solid
            } else if key.viseme.eq_ignore_ascii_case("Silence") {
                palette.control
            } else {
                palette.accent_soft
            };
            painter.rect_filled(block.shrink2(egui::vec2(0.5, 0.0)), 2.0, fill);
            if block.width() > 16.0 {
                let color = if selected {
                    palette.on_accent
                } else {
                    palette.text
                };
                painter.text(
                    block.left_center() + egui::vec2(3.0, 0.0),
                    egui::Align2::LEFT_CENTER,
                    &key.viseme,
                    egui::FontId::proportional(11.0),
                    color,
                );
            }
        }
        let head = x_of(self.time);
        painter.line_segment(
            [
                egui::pos2(head, rect.top()),
                egui::pos2(head, rect.bottom()),
            ],
            egui::Stroke::new(1.5, palette.warning),
        );
        if response.hovered() {
            let edge = pointer
                .or_else(|| ui.input(|input| input.pointer.hover_pos()))
                .filter(|at| strip.contains(*at));
            if edge.is_some() || self.drag.is_some() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
            }
        }
        response.on_hover_text(
            "Click the waveform to move the play head · click a viseme to pick it, drag it to \
             retime it · double-click to add one",
        );
    }

    /// The picked viseme's row: which, how wide, when; add and delete.
    fn key_detail(&mut self, ui: &mut egui::Ui) {
        crate::responsive::toolbar(ui, |ui| {
            if ui
                .button(theme::with_icon(icons::PLUS, "Add at play head"))
                .clicked()
            {
                let viseme = self
                    .selected
                    .and_then(|i| self.lip.visemes.get(i))
                    .map_or("AI".to_owned(), |key| key.viseme.clone());
                self.selected = Some(add_key(&mut self.lip, self.time, &viseme));
                self.dirty = true;
            }
            let Some(index) = self.selected.filter(|&i| i < self.lip.visemes.len()) else {
                return;
            };
            let key = &mut self.lip.visemes[index];
            let mut viseme = key.viseme.clone();
            egui::ComboBox::from_id_salt("lip-sync-viseme")
                .selected_text(&viseme)
                .show_ui(ui, |ui| {
                    for known in VISEMES {
                        ui.selectable_value(&mut viseme, known.name.to_owned(), known.name)
                            .on_hover_text(known.doc);
                    }
                });
            let mut weight = key.weight;
            ui.add(egui::Slider::new(&mut weight, 0.0..=1.0).text("Width"))
                .on_hover_text("How wide it's said");
            let mut time = key.time;
            ui.add(
                egui::DragValue::new(&mut time)
                    .speed(0.005)
                    .suffix(" s")
                    .max_decimals(3),
            );
            let changed = viseme != key.viseme || weight != key.weight;
            key.viseme = viseme;
            key.weight = weight;
            if changed {
                self.dirty = true;
            }
            if time != self.lip.visemes[index].time {
                retime(&mut self.lip, index, time);
                self.dirty = true;
            }
            if widgets::icon_button(ui, icons::TRASH, "Delete this viseme", false).clicked() {
                self.lip.visemes.remove(index);
                self.selected = None;
                self.dirty = true;
            }
        });
    }
}
