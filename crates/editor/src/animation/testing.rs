//! Posing in tests: what the Animation tab does when a bone is picked,
//! turned or keyed in the preview, without drawing it.

use axcenity_anim::{PropertyTrack, bone_paths};
use axcenity_assets::BoneTransform;
use axcenity_render::glam::Quat;

use super::*;

impl AnimationStudio {
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
