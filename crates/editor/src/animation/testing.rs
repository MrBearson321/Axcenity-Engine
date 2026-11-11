//! Posing in tests: what the Animation tab does when a bone is picked,
//! turned or keyed in the preview, without drawing it.

use std::collections::BTreeSet;

use axcenity_anim::{PropertyTrack, bone_paths};
use axcenity_assets::BoneTransform;
use axcenity_render::glam::{Quat, Vec3};

use super::posing::{Posed, Posing, Skeleton};
use super::property_editor::NewKeys;

use super::*;

/// The bones a drag doing `posing` sends, with their transforms.
fn changes_of(posed: &Posed, posing: Posing) -> Vec<(usize, BoneTransform)> {
    let bones: BTreeSet<usize> = posed.sent(posing);
    bones.into_iter().map(|b| (b, posed.pose[b])).collect()
}

/// Frames that drag the mouse with the left button from `from` to `to`.
pub fn drag_frames(from: egui::Pos2, to: egui::Pos2) -> Vec<Vec<egui::Event>> {
    let button = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    let mut frames = vec![
        vec![egui::Event::PointerMoved(from)],
        vec![button(from, true)],
    ];
    for step in 1..=5 {
        frames.push(vec![egui::Event::PointerMoved(
            from.lerp(to, step as f32 / 5.0),
        )]);
    }
    frames.push(vec![button(to, false)]);
    frames.push(Vec::new());
    frames
}

/// Frames that click the left button at `at`.
pub fn click_frames(at: egui::Pos2) -> Vec<Vec<egui::Event>> {
    let button = |pressed| egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    vec![
        vec![egui::Event::PointerMoved(at)],
        vec![button(true)],
        vec![button(false)],
        Vec::new(),
    ]
}

impl AnimationStudio {
    /// The open state machine.
    pub fn machine_doc(&mut self) -> &mut ControllerDoc {
        self.machine.doc.as_mut().expect("a state machine is open")
    }

    /// Picks New keys in the open clip's toolbar.
    pub fn set_new_keys(&mut self, choice: NewKeys) {
        self.clip.new_keys = choice;
        match &mut self.clip.doc {
            Some(Clip::Property(doc)) => doc.new_keys = choice,
            Some(Clip::Take(doc)) => doc.new_keys = choice,
            None => {}
        }
    }

    /// The New keys easing the open clip makes keys with.
    pub fn new_keys(&self) -> NewKeys {
        match &self.clip.doc {
            Some(Clip::Property(doc)) => doc.new_keys,
            Some(Clip::Take(doc)) => doc.new_keys,
            None => self.clip.new_keys,
        }
    }

    /// Selects every key of the open keyed clip's track on `target`, then
    /// picks Easing ▸ `choice` from their right-click menu.
    pub fn ease_keys_of(&mut self, target: &str, choice: NewKeys) {
        let Some(Clip::Property(doc)) = &mut self.clip.doc else {
            panic!("no keyed clip is open");
        };
        doc.selected = doc
            .file
            .tracks
            .iter()
            .filter(|t| t.target == target)
            .flat_map(|t| t.keys.iter().map(|k| (t.id.clone(), k.id.clone())))
            .collect();
        let (interp, easing) = choice.easing();
        doc.ease_selected(interp, easing);
    }

    /// Shows the open clip's keys as curves (or the dope sheet).
    pub fn show_curves(&mut self, on: bool) {
        match &mut self.clip.doc {
            Some(Clip::Property(doc)) => doc.show_curves = on,
            Some(Clip::Take(doc)) => doc.show_imported_curves(on),
            None => {}
        }
    }

    /// Gets the open clip's preview ready, as drawing the tab does: a
    /// property clip's copy of the followed object posed at the playhead,
    /// or a take clip's model.
    fn ready_preview(&mut self, assets: &Assets, world: &World) {
        self.follow_pins();
        match &self.clip.doc {
            Some(Clip::Property(_)) => {
                if let Some(object) = self.subject {
                    self.clip.objects.follow_within(world, object, None);
                }
                self.pose_copy(assets).expect("the clip plays");
            }
            Some(Clip::Take(doc)) => {
                show_model(&mut self.clip.preview, doc.preview_model.as_deref(), assets);
            }
            None => {}
        }
    }

    /// Clicks the joint of the bone named `name` in the preview.
    pub fn pick_bone(&mut self, assets: &Assets, world: &World, name: &str) {
        self.ready_preview(assets, world);
        match &self.clip.doc {
            Some(Clip::Property(_)) => {
                let rig = self.clip.objects.rig().expect("a skinned copy");
                let bone = rig.bone(name).expect("the bone");
                self.clip.objects.select_bone(Some(bone));
                let root = self.clip.objects.root();
                self.preview_events(vec![PreviewEvent::Select(root)]);
            }
            Some(Clip::Take(doc)) => {
                let model = doc
                    .preview_model
                    .as_deref()
                    .and_then(|id| assets.model_blocking(id))
                    .expect("a preview model");
                let bone = model.rig.as_ref().and_then(|rig| rig.bone(name));
                self.clip.bones.selected = Some(bone.expect("the bone"));
            }
            None => panic!("no clip is open"),
        }
    }

    /// Turns the picked bone by `turn` (around the world's axes) with its
    /// gizmo. A take clip says why it can't, for a locked bone.
    pub fn turn_bone(&mut self, assets: &Assets, world: &World, turn: Quat) -> Result<(), String> {
        self.pose_picked(assets, world, |skeleton, bone| {
            Posing::Set(bone, skeleton.turned(bone, turn).expect("the bone"))
        })
        .map(|_| ())
    }

    /// Moves the picked bone's joint `by` (in the world) with its Move
    /// gizmo: a root moves, and any other joint is reached by turning the
    /// bones above it. Returns the pinned joints that slipped, by name.
    pub fn move_joint(
        &mut self,
        assets: &Assets,
        world: &World,
        by: Vec3,
    ) -> Result<Vec<String>, String> {
        self.pose_picked(assets, world, |skeleton, bone| {
            Posing::Reach(bone, skeleton.joints()[bone] + by)
        })
    }

    /// Poses the picked bone as `posing` says (from the preview's skeleton)
    /// the way a gizmo drag does: holding the open clip's pins, and sending
    /// every bone it changed. Returns the pinned joints that slipped.
    fn pose_picked(
        &mut self,
        assets: &Assets,
        world: &World,
        posing: impl FnOnce(&Skeleton, usize) -> Posing,
    ) -> Result<Vec<String>, String> {
        self.ready_preview(assets, world);
        let settings = self.clip.pose;
        let (changes, slipped, rig) = match &mut self.clip.doc {
            Some(Clip::Property(_)) => {
                let mut poser = std::mem::take(&mut self.clip.objects.bones);
                let bone = poser.selected.expect("a picked bone");
                let skeleton = self.clip.objects.skeleton().expect("a skinned copy");
                let posing = posing(&skeleton, bone);
                let posed = poser.pose(&skeleton, posing, &settings);
                let rig = skeleton.rig.clone();
                self.clip.objects.bones = poser;
                (changes_of(&posed, posing), posed.slipped, rig)
            }
            Some(Clip::Take(doc)) => {
                let bone = self.clip.bones.selected.expect("a picked bone");
                let pose = doc.pose(assets)?;
                let skeleton = self
                    .clip
                    .preview
                    .skeleton(pose.rig(), &pose.pose, &pose.locked)
                    .expect("the preview shows the model");
                let posing = posing(&skeleton, bone);
                let posed = self.clip.bones.pose(&skeleton, posing, &settings);
                let changes = changes_of(&posed, posing);
                doc.pose_bones(pose.rig(), &changes, self.clip.record)?;
                (Vec::new(), posed.slipped, pose.rig().clone())
            }
            None => panic!("no clip is open"),
        };
        if !changes.is_empty() {
            let edit = self.clip.objects.bone_edits(&changes);
            self.preview_events(edit.into_iter().collect());
        }
        Ok(slipped
            .into_iter()
            .map(|bone| rig.bones[bone].name.clone())
            .collect())
    }

    /// Picks Pin (or Unpin) on the picked bone's joint.
    pub fn pin_picked(&mut self, assets: &Assets, world: &World, pin: bool) {
        self.ready_preview(assets, world);
        match &mut self.clip.doc {
            Some(Clip::Property(_)) => {
                let mut poser = std::mem::take(&mut self.clip.objects.bones);
                let bone = poser.selected.expect("a picked bone");
                let skeleton = self.clip.objects.skeleton().expect("a skinned copy");
                if pin {
                    poser.pin(&skeleton, bone);
                } else {
                    poser.unpin(bone);
                }
                self.clip.objects.bones = poser;
            }
            Some(Clip::Take(doc)) => {
                let bone = self.clip.bones.selected.expect("a picked bone");
                let pose = doc.pose(assets).expect("a pose");
                let skeleton = self
                    .clip
                    .preview
                    .skeleton(pose.rig(), &pose.pose, &pose.locked)
                    .expect("the preview shows the model");
                if pin {
                    self.clip.bones.pin(&skeleton, bone);
                } else {
                    self.clip.bones.unpin(bone);
                }
            }
            None => panic!("no clip is open"),
        }
    }

    /// The open clip's pinned joints, by name.
    pub fn pinned_bones(&mut self, assets: &Assets, world: &World) -> Vec<String> {
        self.ready_preview(assets, world);
        let (poser, rig) = match &self.clip.doc {
            Some(Clip::Property(_)) => (&self.clip.objects.bones, self.clip.objects.rig().cloned()),
            Some(Clip::Take(doc)) => (
                &self.clip.bones,
                doc.preview_model
                    .as_deref()
                    .and_then(|id| assets.model_blocking(id))
                    .and_then(|model| model.rig.as_deref().cloned()),
            ),
            None => return Vec::new(),
        };
        let rig = rig.expect("a rig");
        poser
            .pins()
            .keys()
            .map(|&bone| rig.bones[bone].name.clone())
            .collect()
    }

    /// Picks how many bones a moved joint turns.
    pub fn set_chain(&mut self, length: usize) {
        self.clip.pose.chain = length;
    }

    /// Presses K over the tab: keys the picked bone where it is.
    pub fn key_picked(&mut self, assets: &Assets, world: &World) -> Result<(), String> {
        self.ready_preview(assets, world);
        match &mut self.clip.doc {
            Some(Clip::Property(doc)) => {
                let focus = self.clip.objects.focus().expect("something picked");
                doc.key_focus(&focus);
                Ok(())
            }
            Some(Clip::Take(doc)) => {
                let bone = self.clip.bones.selected.expect("a picked bone");
                doc.key_selected(assets, bone)
            }
            None => panic!("no clip is open"),
        }
    }

    /// The open clip as the transport sees it: whether it plays, the
    /// playhead and the length, in seconds, and its frames a second.
    pub fn transport_state(&mut self) -> (bool, f64, f64, f64) {
        let doc = self.clip.playback().expect("a clip is open");
        (
            doc.playing(),
            doc.playhead(),
            doc.length(),
            doc.frame_rate(),
        )
    }

    /// The preview's playback rate.
    pub fn preview_rate(&self) -> f64 {
        self.clip.rate.0
    }

    /// Sets the preview's playback rate, as its box does.
    pub fn set_preview_rate(&mut self, rate: f64) {
        self.clip.rate.0 = rate;
    }

    /// Moves the open clip's playhead to `seconds` (paused).
    pub fn set_playhead(&mut self, seconds: f64) {
        match &mut self.clip.doc {
            Some(Clip::Property(doc)) => {
                doc.playing = false;
                doc.playhead = seconds;
            }
            Some(Clip::Take(doc)) => {
                doc.playing = false;
                doc.playhead = seconds;
            }
            None => {}
        }
    }

    /// Moves parts in an open property clip's preview, as the gizmo does:
    /// keyed at the playhead with Record on.
    pub fn move_in_preview(&mut self, edits: Vec<axcenity_anim::EvaluatedProperty>) {
        self.preview_events(vec![PreviewEvent::Edit(edits)]);
    }

    /// Turns Record on or off.
    pub fn set_record(&mut self, on: bool) {
        self.clip.record = on;
    }

    /// The bones as the preview shows them at the playhead.
    pub fn preview_bones(&mut self, assets: &Assets, world: &World) -> Vec<BoneTransform> {
        self.ready_preview(assets, world);
        match &mut self.clip.doc {
            Some(Clip::Property(_)) => self
                .clip
                .objects
                .bone_pose()
                .expect("a skinned copy")
                .to_vec(),
            Some(Clip::Take(doc)) => doc.pose(assets).expect("a pose").pose,
            None => panic!("no clip is open"),
        }
    }

    /// The open clip's tracks.
    pub fn open_tracks(&self) -> Vec<PropertyTrack> {
        match &self.clip.doc {
            Some(Clip::Property(doc)) => doc.file.tracks.clone(),
            Some(Clip::Take(doc)) => doc.tracks().to_vec(),
            None => Vec::new(),
        }
    }

    /// The open clip's badge, as its toolbar shows it.
    pub fn open_badge(&self) -> String {
        self.clip
            .doc
            .as_ref()
            .map(|clip| clip.badge().text())
            .unwrap_or_default()
    }

    /// The bone paths of the preview model the open take clip shows, by
    /// bone.
    pub fn preview_bone_paths(&self, assets: &Assets) -> Vec<String> {
        let Some(Clip::Take(doc)) = &self.clip.doc else {
            return self.clip.objects.bone_paths();
        };
        doc.preview_model
            .as_deref()
            .and_then(|id| assets.model_blocking(id))
            .and_then(|model| model.rig.as_deref().map(bone_paths))
            .unwrap_or_default()
    }

    /// The open linked clip's imported keys, as shown under its timeline.
    pub fn imported_view(&mut self, assets: &Assets) -> &mut PropertyDoc {
        let Some(Clip::Take(doc)) = &mut self.clip.doc else {
            panic!("no linked clip is open");
        };
        doc.imported_keys(assets).expect("its keys can be shown")
    }

    /// Sets where the open linked clip starts and ends in its take, in
    /// seconds, as dragging its ends on the timeline does.
    pub fn set_clip_range(&mut self, start: f64, end: f64) {
        let Some(Clip::Take(doc)) = &mut self.clip.doc else {
            panic!("no linked clip is open");
        };
        doc.file.start = start;
        doc.file.end = end;
    }

    /// The open keyed clip as it would be saved.
    pub fn open_keyed_file(&self) -> axcenity_anim::ClipFile {
        let Some(Clip::Property(doc)) = &self.clip.doc else {
            panic!("no keyed clip is open");
        };
        doc.file.clone()
    }

    /// The open linked clip as it would be saved.
    pub fn open_clip_file(&self) -> axcenity_anim::ClipFile {
        let Some(Clip::Take(doc)) = &self.clip.doc else {
            panic!("no linked clip is open");
        };
        doc.file.to_file()
    }

    /// The names of the bones the drawer shows for the open linked clip's
    /// preview model, as folded now.
    pub fn shown_bones(&mut self, assets: &Assets) -> Vec<String> {
        let (mesh_id, model) = self.preview_rig(assets);
        let rig = model.rig.as_deref().expect("a rig");
        let keys = vec![0; rig.bones.len()];
        self.clip
            .folds
            .rows(&mesh_id, rig, &keys)
            .into_iter()
            .map(|row| rig.bones[row.bone].name.clone())
            .collect()
    }

    /// Presses Collapse all or Expand all over the drawer's bones.
    pub fn fold_all_bones(&mut self, assets: &Assets, fold: bool) {
        let (mesh_id, model) = self.preview_rig(assets);
        let rig = model.rig.as_deref().expect("a rig");
        if fold {
            self.clip.folds.collapse_all(&mesh_id, rig);
        } else {
            self.clip.folds.expand_all(&mesh_id, rig);
        }
    }

    fn preview_rig(&self, assets: &Assets) -> (String, std::sync::Arc<axcenity_assets::Model>) {
        let Some(Clip::Take(doc)) = &self.clip.doc else {
            panic!("no linked clip is open");
        };
        let mesh_id = doc.preview_model.clone().expect("a preview model");
        let model = assets.model_blocking(&mesh_id).expect("the preview model");
        (mesh_id, model)
    }

    /// The clips and state machines Open ▾ marks **not used**, looking at
    /// the project's files again.
    pub fn unused_files(&mut self, project: &Path, world: &World) -> Vec<String> {
        self.rescan(project);
        let used = self.used_files(project, world);
        self.files
            .controllers
            .iter()
            .chain(&self.files.clips)
            .filter(|path| !used.contains(*path))
            .cloned()
            .collect()
    }

    /// Picks Convert to keys in the open clip's ⋯ menu.
    pub fn convert_open_clip(
        &mut self,
        project: &Path,
        assets: &Assets,
        world: &mut World,
        output: &mut Output,
    ) -> Vec<StudioAction> {
        let mut actions = Vec::new();
        let path = self.clip_path().expect("a clip is open");
        self.act(
            Pick::ConvertToKeys(path),
            project,
            assets,
            world,
            output,
            &mut actions,
        );
        actions
    }

    /// Opens the clip at `path` for the object `object`, as clicking its
    /// Animation in the outline does.
    pub fn open_for(
        &mut self,
        project: &Path,
        assets: &Assets,
        object: InstanceId,
        path: &str,
        world: &mut World,
        output: &mut Output,
    ) {
        self.subject = Some(object);
        let mut actions = Vec::new();
        self.go(
            Go::Animation(path.to_owned()),
            project,
            assets,
            world,
            output,
            &mut actions,
        );
    }
}
