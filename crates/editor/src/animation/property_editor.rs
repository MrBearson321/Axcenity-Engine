//! A property clip open in the Animation tab: its tracks and keys, the
//! dope sheet, and keying what's moved in the preview.
//!
//! Moving something in the preview with Record off changes it only until
//! the playhead moves ("pending"); Key selected (K) or Record turns the
//! moves into keys at the playhead, making tracks as needed. The dope sheet
//! selects keys by clicking (Shift or Ctrl adds) or dragging a box, moves
//! them by dragging, and copies, pastes and deletes them. With Snap on,
//! times land on whole frames. The Curves view (`curves.rs`) shows the
//! same keys as curves, with Bezier handles.
//!
//! New keys get the toolbar's **New keys** easing ([`NewKeys`], Smooth to
//! start with); keying over a key keeps its easing. Right-clicking selected
//! keys has **Easing ▸**, which sets all of theirs.
//!
//! On a skinned MeshPart the clip's `Bone` tracks pose its skeleton:
//! turning a bone in the preview is a `Bone` edit like any other, and
//! [`PropertyDoc::bone_pose`] gives the pose at the playhead.

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;

use axcenity_anim::{
    ClipFile, Easing, EasingDirection, EasingStyle, EvaluatedProperty, Interp, PropertyClip,
    PropertyKeyframe, PropertyName, PropertyTrack, bone_from_numbers, bone_numbers, bone_paths,
    find_bone, target_path,
};
use axcenity_assets::{BoneTransform, Rig};
use axcenity_core::InstanceId;
use axcenity_render::glam::{EulerRot, Quat};
use eframe::egui;

use super::object_preview::{Focus, ObjectPreview};
use super::posing::Folds;
use crate::theme::{self, icons};

/// The dope sheet's row height and ruler height, in points.
const ROW: f32 = 22.0;
const RULER: f32 = 20.0;
/// How close (in points) the mouse must be to a key to grab it.
const GRAB: f32 = 7.0;

/// A key, by its track's id and its own.
type KeyRef = (String, String);

/// The easing new keys get from Record, K, posing and Add track.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NewKeys {
    /// Ease InOut, Sine: starts and stops gently.
    #[default]
    Smooth,
    Linear,
    Constant,
}

impl NewKeys {
    pub const ALL: [NewKeys; 3] = [NewKeys::Smooth, NewKeys::Linear, NewKeys::Constant];

    pub fn label(self) -> &'static str {
        match self {
            NewKeys::Smooth => "Smooth",
            NewKeys::Linear => "Linear",
            NewKeys::Constant => "Constant",
        }
    }

    /// The interpolation and easing a key gets.
    pub fn easing(self) -> (Interp, Option<Easing>) {
        match self {
            NewKeys::Smooth => (
                Interp::Ease,
                Some(Easing::new(EasingStyle::Sine, EasingDirection::InOut)),
            ),
            NewKeys::Linear => (Interp::Linear, None),
            NewKeys::Constant => (Interp::Step, None),
        }
    }

    /// The New keys menu, for a toolbar.
    pub fn picker(&mut self, ui: &mut egui::Ui) {
        egui::ComboBox::from_id_salt("new keys easing")
            .selected_text(format!("New keys: {}", self.label()))
            .show_ui(ui, |ui| {
                for choice in Self::ALL {
                    ui.selectable_value(self, choice, choice.label());
                }
            })
            .response
            .on_hover_text(
                "How keys you make from now on move to the next: Smooth eases in and out. \
                 Keys already made keep theirs.",
            );
    }
}

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

/// What was clicked in the drawer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Drawn {
    Object(InstanceId),
    /// A bone of the copy's skinned root, by its index in the rig.
    Bone(usize),
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
    /// Whether the Curves view shows in place of the dope sheet.
    pub show_curves: bool,
    pub curve_view: CurveView,
    /// Tracks picked in the Curves view's list.
    pub curve_picked: BTreeSet<String>,
    curve_gesture: Option<curves::CurveGesture>,
    /// Keys can be selected and looked at, but not moved, added, pasted
    /// or deleted (an imported clip's motion).
    pub read_only: bool,
    /// The preview copy's skeleton, if it's a skinned MeshPart: a bone's
    /// first key after 0 gets a key at 0 where it rests.
    pub rig: Option<Arc<Rig>>,
    /// Where the dope sheet last drew.
    pub sheet: egui::Rect,
    /// The easing keys made from now on get.
    pub new_keys: NewKeys,
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
            show_curves: false,
            curve_view: CurveView::default(),
            curve_picked: BTreeSet::new(),
            curve_gesture: None,
            read_only: false,
            rig: None,
            sheet: egui::Rect::NOTHING,
            new_keys: NewKeys::default(),
        }
    }

    /// `file`'s keys to look at and select, but not change.
    pub fn read_only(path: String, file: ClipFile) -> Self {
        Self {
            read_only: true,
            ..Self::open(path, file)
        }
    }

    /// Shows `file` in place of the keys shown now, keeping the selection
    /// of keys it still has.
    pub fn show_file(&mut self, file: ClipFile) {
        let has = |(track, key): &KeyRef| {
            file.tracks
                .iter()
                .any(|t| &t.id == track && t.keys.iter().any(|k| &k.id == key))
        };
        self.selected.retain(has);
        if self
            .selected_track
            .as_ref()
            .is_none_or(|id| !file.tracks.iter().any(|t| &t.id == id))
        {
            self.selected_track = file.tracks.first().map(|t| t.id.clone());
        }
        self.written = file.clone();
        self.opened = file.clone();
        self.file = file;
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
        // Bones are posed by `bone_pose`.
        for edit in self
            .pending
            .iter()
            .filter(|p| p.property != PropertyName::Bone)
        {
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

    /// `rig`'s pose at the playhead: the clip's bone tracks, with bone
    /// moves not yet keyed on top. Call [`evaluated`](Self::evaluated)
    /// first, which drops moves made at another time.
    pub fn bone_pose(&self, rig: &Rig) -> Vec<BoneTransform> {
        let mut pose = rig.rest_pose();
        let seconds = self.playhead.clamp(0.0, self.file.duration.max(0.0));
        axcenity_anim::pose_bones(&self.file, rig, seconds, &mut pose);
        let paths = axcenity_anim::bone_paths(rig);
        for edit in self
            .pending
            .iter()
            .filter(|p| p.property == PropertyName::Bone)
        {
            if let Some(bone) = find_bone(rig, &paths, &edit.target)
                && let (Some(transform), Some(slot)) =
                    (bone_from_numbers(&edit.value), pose.get_mut(bone))
            {
                *slot = transform;
            }
        }
        pose
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
    #[cfg(test)]
    pub fn edit(&mut self, edits: Vec<EvaluatedProperty>, record: bool) {
        self.edit_from(edits, record, |_| None);
    }

    /// [`Self::edit`], with what each property was before the clip moved it
    /// (see [`Self::key_values_from`]).
    pub fn edit_from(
        &mut self,
        edits: Vec<EvaluatedProperty>,
        record: bool,
        rest: impl Fn(&EvaluatedProperty) -> Option<Vec<f32>>,
    ) {
        if self.read_only {
            return;
        }
        self.playing = false;
        if record {
            self.key_values_from(&edits, rest);
            return;
        }
        self.pending_at = self.playhead;
        for edit in edits {
            self.pending
                .retain(|p| !(p.target == edit.target && p.property == edit.property));
            self.pending.push(edit);
        }
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

/// What each of a key's [`shown_numbers`] is called in the inspector.
fn value_labels(property: PropertyName, width: usize) -> &'static [&'static str] {
    match (property, width) {
        (PropertyName::Bone, _) => &["Turn X", "Turn Y", "Turn Z", "Move X", "Move Y", "Move Z"],
        (_, 4) => &["X scale", "X offset", "Y scale", "Y offset"],
        (_, 1) => &["Value"],
        (PropertyName::Color | PropertyName::BackgroundColor3 | PropertyName::TextColor3, _) => {
            &["R", "G", "B"]
        }
        _ => &["X", "Y", "Z"],
    }
}

/// The numbers a key of `property` is shown and edited as. A bone's 10
/// (where it is, its turn as a quaternion, and its scale) show as its turn
/// in degrees (X, Y and Z, in Orientation's order), then where it is; its
/// quaternion and scale can't be edited on their own. Anything else's show
/// as they are.
fn shown_numbers(property: PropertyName, value: &[f32]) -> Vec<f32> {
    match (property, value) {
        (PropertyName::Bone, [tx, ty, tz, qx, qy, qz, qw, ..]) => {
            let turn = Quat::from_xyzw(*qx, *qy, *qz, *qw).normalize();
            let (y, x, z) = turn.to_euler(EulerRot::YXZ);
            vec![
                x.to_degrees(),
                y.to_degrees(),
                z.to_degrees(),
                *tx,
                *ty,
                *tz,
            ]
        }
        _ => value.to_vec(),
    }
}

/// Sets the `number`th of `value`'s [`shown_numbers`] to `shown`.
fn set_shown_number(property: PropertyName, value: &mut [f32], number: usize, shown: f32) {
    if property != PropertyName::Bone || value.len() != 10 {
        if let Some(n) = value.get_mut(number) {
            *n = shown;
        }
        return;
    }
    if number >= 3 {
        if let Some(n) = value.get_mut(number - 3) {
            *n = shown;
        }
        return;
    }
    let mut angles = shown_numbers(property, value);
    angles[number] = shown;
    let [x, y, z] = [angles[0], angles[1], angles[2]].map(f32::to_radians);
    let turn = Quat::from_euler(EulerRot::YXZ, y, x, z);
    value[3..7].copy_from_slice(&[turn.x, turn.y, turn.z, turn.w]);
}

/// What the inspector calls an interpolation.
fn interp_name(interp: Interp) -> &'static str {
    match interp {
        Interp::Step => "Constant",
        Interp::Linear => "Linear",
        Interp::Ease => "Ease",
        Interp::Bezier => "Bezier",
    }
}

/// Empty property clip for "New property clip".
pub fn new_property_clip(duration: f64) -> ClipFile {
    ClipFile::new(duration)
}

mod curves;
mod dope_sheet;
mod editing;
pub use curves::CurveView;
pub(super) use editing::{rest_numbers, set_key};

#[cfg(test)]
mod tests;
