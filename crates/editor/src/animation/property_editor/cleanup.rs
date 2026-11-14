//! A keyed clip's Cleanup section (AP4): Mirror, Smooth, Reduce keys,
//! Fix loop seam and Re-time, through `axcenity_anim`'s cleanup functions.
//! Each changes the clip's keys like any edit, so it's one undo step when
//! the clip is written.

use axcenity_anim::{Tolerance, fix_loop_seam, mirror, reduce_keys, retime, smooth};
use axcenity_assets::RigDefinitions;
use eframe::egui;

use super::PropertyDoc;
use crate::theme::icons;

/// The Cleanup section's settings, kept while the clip is open.
#[derive(Clone, Debug, PartialEq)]
pub struct Cleanup {
    pub passes: u32,
    pub tolerance: Tolerance,
    /// Seconds Fix loop seam bends over.
    pub blend: f64,
    /// The range Re-time changes, and how long it becomes.
    pub from: f64,
    pub to: f64,
    pub length: f64,
    /// What the last tool did, or why it couldn't.
    pub note: Option<String>,
}

impl Default for Cleanup {
    fn default() -> Self {
        Self {
            passes: 1,
            tolerance: Tolerance::default(),
            blend: 0.25,
            from: 0.0,
            to: 0.0,
            length: 0.0,
            note: None,
        }
    }
}

impl PropertyDoc {
    /// The tracks the tools change: those with a selected key, or else all.
    pub fn cleanup_tracks(&self) -> Option<Vec<String>> {
        let mut ids: Vec<String> = self.selected.iter().map(|(t, _)| t.clone()).collect();
        ids.dedup();
        (!ids.is_empty()).then_some(ids)
    }

    /// Mirrors the clip's bone keys left to right on the preview's skeleton.
    pub fn mirror_bones(&mut self, rigs: &RigDefinitions) -> Result<String, String> {
        let rig = self
            .rig
            .clone()
            .ok_or("preview it on a skinned model first: mirroring needs its skeleton")?;
        let done = mirror(&mut self.file, &rig, rigs)?;
        self.after_cleanup();
        Ok(format!(
            "Mirrored: {} bones swapped sides, {} mirrored in place.",
            done.swapped, done.in_place
        ))
    }

    /// Drops the keys that change nothing, to within the tolerance.
    pub fn reduce(&mut self) -> String {
        let tracks = self.cleanup_tracks();
        let dropped = reduce_keys(&mut self.file, tracks.as_deref(), self.cleanup.tolerance);
        self.after_cleanup();
        match dropped {
            0 => "No key could go without changing the motion.".to_owned(),
            1 => "Removed 1 key.".to_owned(),
            n => format!("Removed {n} keys."),
        }
    }

    /// Smooths the keys.
    pub fn smooth_keys(&mut self) -> String {
        let tracks = self.cleanup_tracks();
        let moved = smooth(&mut self.file, tracks.as_deref(), self.cleanup.passes);
        self.after_cleanup();
        match moved {
            0 => "Nothing to smooth: a track needs 3 keys or more.".to_owned(),
            n => format!("Smoothed {n} keys."),
        }
    }

    /// Bends each track's end onto its start.
    pub fn fix_seam(&mut self) -> String {
        let tracks = self.cleanup_tracks();
        let changed = fix_loop_seam(&mut self.file, tracks.as_deref(), self.cleanup.blend);
        self.after_cleanup();
        match changed {
            0 => "Every track already ends where it starts.".to_owned(),
            1 => "Bent 1 track to end where it starts.".to_owned(),
            n => format!("Bent {n} tracks to end where they start."),
        }
    }

    /// Re-times the range to its new length.
    pub fn retime_range(&mut self) -> Result<String, String> {
        let Cleanup {
            from, to, length, ..
        } = self.cleanup;
        retime(&mut self.file, from, to, length)?;
        self.cleanup.to = from + length;
        self.after_cleanup();
        Ok(format!(
            "Re-timed {from:.2}–{to:.2} s to {length:.2} s; the clip is now {:.2} s.",
            self.file.duration
        ))
    }

    /// After a tool: keys that went are no longer selected, and the
    /// playhead stays inside the clip.
    fn after_cleanup(&mut self) {
        self.tidy();
        self.playhead = self.playhead.min(self.file.duration);
    }

    /// The Cleanup section, under the clip's settings.
    pub fn cleanup_section(&mut self, ui: &mut egui::Ui, rigs: &RigDefinitions) {
        if self.read_only || self.file.tracks.is_empty() {
            return;
        }
        ui.separator();
        egui::CollapsingHeader::new(format!("{} Cleanup", icons::SPARKLE))
            .id_salt("property clip cleanup")
            .show(ui, |ui| self.cleanup_tools(ui, rigs));
    }

    fn cleanup_tools(&mut self, ui: &mut egui::Ui, rigs: &RigDefinitions) {
        ui.weak(match self.cleanup_tracks() {
            Some(tracks) if tracks.len() == 1 => {
                "Changes the selected key's track (Mirror changes every bone).".to_owned()
            }
            Some(tracks) => format!(
                "Changes the {} tracks with selected keys (Mirror changes every bone).",
                tracks.len()
            ),
            None => "Changes every track. Select keys to change only their tracks.".to_owned(),
        });
        // A range for Re-time: the selected keys' span, or the whole clip.
        if self.cleanup.to <= self.cleanup.from {
            let times: Vec<f64> = self.selected_keys().into_iter().map(|(_, t)| t).collect();
            let (from, to) = match (
                times.iter().copied().reduce(f64::min),
                times.iter().copied().reduce(f64::max),
            ) {
                (Some(from), Some(to)) if to > from => (from, to),
                _ => (0.0, self.file.duration),
            };
            self.cleanup.from = from;
            self.cleanup.to = to;
            self.cleanup.length = to - from;
        }

        let mut note = None;
        let bones = self
            .file
            .tracks
            .iter()
            .any(|t| t.property == axcenity_anim::PropertyName::Bone);
        if bones {
            let button = ui
                .add_enabled(
                    self.rig.is_some(),
                    egui::Button::new(format!("{} Mirror left ↔ right", icons::FLIP_HORIZONTAL)),
                )
                .on_hover_text(
                    "Swap the motion of the left and right bones as in a mirror; bones in the middle mirror in place",
                )
                .on_disabled_hover_text("Preview the clip on a skinned model to mirror its bones");
            if button.clicked() {
                note = Some(
                    self.mirror_bones(rigs)
                        .unwrap_or_else(|err| format!("Can't mirror: {err}.")),
                );
            }
        }

        crate::responsive::form(ui, "clip cleanup", |ui| {
            ui.label("Passes");
            ui.add(egui::DragValue::new(&mut self.cleanup.passes).range(1..=20));
            ui.end_row();
            ui.label("");
            if ui
                .button(format!("{} Smooth", icons::WAVE_SINE))
                .on_hover_text("Move each key toward its neighbours, softening jitter")
                .clicked()
            {
                note = Some(self.smooth_keys());
            }
            ui.end_row();

            ui.label("Tolerance");
            ui.horizontal(|ui| {
                ui.add(
                    egui::DragValue::new(&mut self.cleanup.tolerance.value)
                        .speed(0.0005)
                        .range(0.0..=10.0)
                        .max_decimals(4),
                )
                .on_hover_text("How far a value may move (studs, colour, scale)");
                ui.add(
                    egui::DragValue::new(&mut self.cleanup.tolerance.angle)
                        .speed(0.01)
                        .range(0.0..=45.0)
                        .suffix("°"),
                )
                .on_hover_text("How far a turn may move");
            });
            ui.end_row();
            ui.label("");
            if ui
                .button(format!("{} Reduce keys", icons::FUNNEL))
                .on_hover_text("Remove the keys the keys around them already make")
                .clicked()
            {
                note = Some(self.reduce());
            }
            ui.end_row();

            ui.label("Blend");
            ui.add(
                egui::DragValue::new(&mut self.cleanup.blend)
                    .speed(0.01)
                    .range(0.01..=self.file.duration.max(0.01))
                    .suffix(" s"),
            );
            ui.end_row();
            ui.label("");
            if ui
                .button(format!("{} Fix loop seam", icons::REPEAT))
                .on_hover_text(
                    "Bend the end of each track onto its start, so the loop doesn't jump",
                )
                .clicked()
            {
                note = Some(self.fix_seam());
            }
            ui.end_row();

            let duration = self.file.duration;
            ui.label("Range");
            ui.horizontal(|ui| {
                ui.add(
                    egui::DragValue::new(&mut self.cleanup.from)
                        .speed(0.01)
                        .range(0.0..=duration)
                        .suffix(" s"),
                );
                ui.label("to");
                ui.add(
                    egui::DragValue::new(&mut self.cleanup.to)
                        .speed(0.01)
                        .range(0.0..=duration)
                        .suffix(" s"),
                );
            });
            ui.end_row();
            ui.label("New length");
            ui.add(
                egui::DragValue::new(&mut self.cleanup.length)
                    .speed(0.01)
                    .range(0.01..=3600.0)
                    .suffix(" s"),
            );
            ui.end_row();
            ui.label("");
            if ui
                .button(format!("{} Re-time", icons::TIMER))
                .on_hover_text(
                    "Stretch or squeeze the keys and events in the range; later ones move with its end",
                )
                .clicked()
            {
                note = Some(self.retime_range().unwrap_or_else(|err| format!("Can't re-time: {err}.")));
            }
            ui.end_row();
        });
        if let Some(note) = note {
            self.cleanup.note = Some(note);
        }
        if let Some(note) = &self.cleanup.note {
            ui.weak(note);
        }
    }
}

#[cfg(test)]
#[path = "cleanup_tests.rs"]
mod tests;
