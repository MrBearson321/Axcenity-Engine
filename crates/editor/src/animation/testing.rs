//! Posing in tests: what the Animation tab does when a bone is picked,
//! turned or keyed in the preview, without drawing it.

use axcenity_anim::{PropertyTrack, bone_paths};
use axcenity_assets::BoneTransform;
use axcenity_render::glam::Quat;

use super::*;

impl AnimationStudio {
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
                    .and_then(|id| assets.model(id))
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
        self.ready_preview(assets, world);
        match &mut self.clip.doc {
            Some(Clip::Property(_)) => {
                let bone = self.clip.objects.bones.selected.expect("a picked bone");
                let skeleton = self.clip.objects.skeleton().expect("a skinned copy");
                let turned = skeleton.turned(bone, turn).expect("the bone");
                let edit = self.clip.objects.bone_edit(bone, &turned);
                self.preview_events(edit.into_iter().collect());
                Ok(())
            }
            Some(Clip::Take(doc)) => {
                let bone = self.clip.bones.selected.expect("a picked bone");
                let posing = doc.pose(assets)?;
                let skeleton = self
                    .clip
                    .preview
                    .skeleton(posing.rig(), &posing.pose, &posing.locked)
                    .expect("the preview shows the model");
                let turned = skeleton.turned(bone, turn).expect("the bone");
                doc.pose_bone(posing.rig(), bone, &turned, self.clip.record)
            }
            None => panic!("no clip is open"),
        }
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
            .and_then(|id| assets.model(id))
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
        let model = assets.model(&mesh_id).expect("the preview model");
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
