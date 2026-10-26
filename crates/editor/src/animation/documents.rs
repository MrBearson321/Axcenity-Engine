use super::*;

impl AnimationStudio {
    /// Reloads whatever is open after undo or redo wrote `files`. A renamed
    /// file stays open at its other path; a deleted one closes.
    pub fn sync_after_history(
        &mut self,
        project: &Path,
        assets: &Assets,
        world: &mut World,
        files: &[store::FileDiff],
        output: &mut Output,
    ) {
        self.rescan(project);
        for open in [self.clip_path(), self.controller_path()]
            .into_iter()
            .flatten()
        {
            let Some(change) = files.iter().find(|file| file.path == open) else {
                continue;
            };
            if change.after.is_none() {
                let replacement = files.iter().find(|file| {
                    file.path != open
                        && file.before.is_none()
                        && file.after.is_some()
                        && same_asset_kind(&file.path, &open)
                });
                match replacement {
                    Some(file) => {
                        let path = file.path.clone();
                        self.reopen(project, assets, &path, world, output);
                    }
                    None => self.close(&open),
                }
                continue;
            }
            if open.ends_with(axcenity_anim::STATE_MACHINE_EXTENSION)
                && let Some(doc) = &mut self.machine.doc
                && let Ok(file) = store::read_controller(project, &open)
            {
                doc.reload(file);
                continue;
            }
            self.reopen(project, assets, &open, world, output);
        }
    }

    /// Closes `path` in whichever tab has it open.
    pub(super) fn close(&mut self, path: &str) {
        if self.clip_path().as_deref() == Some(path) {
            self.clip.doc = None;
        }
        if self.controller_path().as_deref() == Some(path) {
            self.machine.doc = None;
        }
    }

    /// Opens `path` from disk in its tab, without writing what that tab
    /// has open.
    pub(super) fn reopen(
        &mut self,
        project: &Path,
        assets: &Assets,
        path: &str,
        world: &mut World,
        output: &mut Output,
    ) {
        if path.ends_with(axcenity_anim::CLIP_EXTENSION) {
            self.clip.objects.refresh(world);
            self.load_clip(project, assets, path, output);
        } else if path.ends_with(axcenity_anim::STATE_MACHINE_EXTENSION) {
            let preview = self
                .machine
                .doc
                .as_ref()
                .and_then(|doc| doc.preview_model.clone());
            self.machine.doc = None;
            match store::read_controller(project, path) {
                Ok(file) => {
                    let mut doc = ControllerDoc::open(path.to_owned(), file, preview);
                    if doc.preview_model.is_none() {
                        doc.preview_model = doc.first_clip_source(project);
                    }
                    self.machine.doc = Some(Box::new(doc));
                }
                Err(err) => {
                    output.error(format!("Could not open the state machine `{path}`: {err}"))
                }
            }
        }
    }

    /// Reads the clip at `path` into the Animation tab (nothing is open if
    /// it can't be read).
    pub(super) fn load_clip(
        &mut self,
        project: &Path,
        assets: &Assets,
        path: &str,
        output: &mut Output,
    ) {
        // A bone picked on another clip's model may not be on this one's.
        self.clip.bones.selected = None;
        // A linked clip opens on its take; a keyed one in the property editor.
        self.clip.doc = match store::read_any_clip(project, path) {
            Ok(file) if file.link.is_some() => {
                let mut doc = ClipDoc::open(path.to_owned(), file, assets, &self.files.models);
                doc.new_keys = self.clip.new_keys;
                Some(Clip::Take(Box::new(doc)))
            }
            Ok(file) => {
                let mut doc = PropertyDoc::open(path.to_owned(), file);
                doc.new_keys = self.clip.new_keys;
                Some(Clip::Property(Box::new(doc)))
            }
            Err(err) => {
                output.error(format!("Could not open the clip `{path}`: {err}"));
                None
            }
        };
    }

    /// Opens the clip at `path` (from the project folder) and shows it.
    pub fn open_clip(
        &mut self,
        project: &Path,
        assets: &Assets,
        path: &str,
        world: &mut World,
        output: &mut Output,
    ) {
        // A fresh copy, with whatever was changed in the world since.
        self.clip.objects.refresh(world);
        let _ = self.write_all(project, output);
        self.rescan(project);
        self.load_clip(project, assets, path, output);
        self.view = View::Clip;
    }

    /// Opens the clip at `path` on its own, as the Assets panel does: it
    /// follows no object, and previews on the model picked with "Preview
    /// on…" (or on the Part, Model or UI selected, for a keyed clip).
    pub fn open_clip_alone(
        &mut self,
        project: &Path,
        assets: &Assets,
        path: &str,
        world: &mut World,
        output: &mut Output,
    ) {
        self.subject = None;
        self.from_state = None;
        self.clip.objects.forget();
        self.open_clip(project, assets, path, world, output);
    }

    /// After Animate made the clip at `path` for `object`: follows `object`
    /// and opens the clip with Record on.
    pub fn animated(
        &mut self,
        project: &Path,
        assets: &Assets,
        object: InstanceId,
        path: &str,
        world: &mut World,
        output: &mut Output,
    ) {
        self.subject = Some(object);
        self.from_state = None;
        self.open_clip(project, assets, path, world, output);
        self.clip.objects.show_root(world, object);
        self.clip.record = true;
    }

    /// Opens the state machine at `path` and shows it, previewed on `model`
    /// (a `MeshId`) if given, or else on its first clip's model.
    pub fn open_controller(
        &mut self,
        project: &Path,
        assets: &Assets,
        path: &str,
        model: Option<String>,
        output: &mut Output,
    ) {
        let _ = self.write_all(project, output);
        self.rescan(project);
        match store::read_controller(project, path) {
            Ok(file) => {
                let mut doc = ControllerDoc::open(path.to_owned(), file, None);
                let animated = |mesh_id: &String| {
                    assets
                        .model(mesh_id)
                        .is_some_and(|model| model.rig.is_some())
                };
                doc.preview_model = model
                    .filter(animated)
                    .or_else(|| doc.first_clip_source(project).filter(animated))
                    .or_else(|| self.files.models.iter().find(|m| animated(m)).cloned());
                self.machine.doc = Some(Box::new(doc));
                self.view = View::StateMachine;
                self.from_state = None;
            }
            Err(err) => output.error(format!("Could not open the state machine `{path}`: {err}")),
        }
    }

    /// Follows the selection. Selecting something that stands for another
    /// object (see [`animatable_of`]) with an Animator opens what that
    /// Animator has: its state machine, or else its first Animation's clip.
    /// Selecting inside the followed object keeps it (so a Door's Hinge can
    /// be picked in its preview). Selecting another object with no Animator
    /// follows it with nothing open, ready for Animate, unless a clip was
    /// opened on its own.
    #[allow(clippy::too_many_arguments)] // Each is a different input to the studio.
    pub(super) fn follow(
        &mut self,
        project: &Path,
        assets: &Assets,
        world: &mut World,
        selected: Option<InstanceId>,
        output: &mut Output,
        actions: &mut Vec<StudioAction>,
    ) {
        if self.subject.is_some_and(|id| !world.contains(id)) {
            self.subject = None;
        }
        if selected == self.last_selected {
            return;
        }
        self.last_selected = selected;
        let Some(object) = selected.and_then(|id| animatable_of(world, id)) else {
            return;
        };
        let animator = animator_of(world, object);
        if let Some(current) = self.subject
            && (object == current
                || (animator.is_none() && world.is_descendant_of(object, current)))
        {
            // An Animation of the followed object opens its clip.
            let clip = selected
                .and_then(|id| world.get(id))
                .filter(|instance| instance.class() == ClassName::Animation)
                .and_then(|instance| instance.property("Clip").and_then(Value::as_str))
                .filter(|clip| !clip.is_empty())
                .map(str::to_owned);
            if let Some(clip) = clip {
                self.go(Go::Animation(clip), project, assets, world, output, actions);
            }
            return;
        }
        let open = self.clip.doc.is_some() || self.machine.doc.is_some();
        if animator.is_none() && self.subject.is_none() && open {
            return;
        }
        actions.extend(self.write_all(project, output));
        self.subject = Some(object);
        self.from_state = None;
        self.clip.doc = None;
        self.clip.record = false;
        self.machine.doc = None;
        let Some(subject) = Subject::of(world, object) else {
            return;
        };
        if !subject.state_machine.is_empty() {
            self.go(Go::StateMachine, project, assets, world, output, actions);
        } else if let Some(animation) = subject.animations.iter().find(|a| !a.clip.is_empty()) {
            let clip = animation.clip.clone();
            self.go(Go::Animation(clip), project, assets, world, output, actions);
        }
    }

    /// Goes where the outline or the breadcrumb asked.
    pub(super) fn go(
        &mut self,
        go: Go,
        project: &Path,
        assets: &Assets,
        world: &mut World,
        output: &mut Output,
        actions: &mut Vec<StudioAction>,
    ) {
        let subject = self.subject.and_then(|id| Subject::of(world, id));
        match go {
            Go::StateMachine => {
                let Some(subject) = subject.filter(|s| !s.state_machine.is_empty()) else {
                    return;
                };
                if self.controller_path().as_deref() == Some(subject.state_machine.as_str()) {
                    self.view = View::StateMachine;
                    self.from_state = None;
                } else {
                    let model = subject.rigged_model(assets);
                    self.open_controller(project, assets, &subject.state_machine, model, output);
                }
            }
            Go::State(state, clip) => {
                self.open_subject_clip(project, assets, &clip, subject.as_ref(), world, output);
                self.from_state = Some(state);
            }
            Go::Animation(clip) => {
                self.open_subject_clip(project, assets, &clip, subject.as_ref(), world, output);
                self.from_state = None;
            }
            Go::Animate => {
                if let Some(subject) = subject {
                    actions.push(StudioAction::Animate(subject.object));
                }
            }
            Go::Import => actions.push(StudioAction::Import),
        }
    }

    /// Opens the followed object's clip at `path` (or shows it, if it's
    /// open), on the object's model if its take fits that skeleton.
    pub(super) fn open_subject_clip(
        &mut self,
        project: &Path,
        assets: &Assets,
        path: &str,
        subject: Option<&Subject>,
        world: &mut World,
        output: &mut Output,
    ) {
        if self.clip_path().as_deref() == Some(path) {
            self.view = View::Clip;
        } else {
            self.open_clip(project, assets, path, world, output);
        }
        if let Some(Clip::Take(doc)) = &mut self.clip.doc
            && let Some(model) = subject.and_then(|s| s.rigged_model(assets))
        {
            doc.prefer_model(assets, &model);
        }
    }

    /// The states of the state machine at `path` and the clips each plays:
    /// the open one's, or as read for the outline.
    pub(super) fn states_of(&self, path: &str) -> Vec<(String, Vec<String>)> {
        if let Some(doc) = &self.machine.doc
            && doc.path == path
        {
            return doc
                .file
                .states
                .iter()
                .map(|state| (state.name.clone(), motion_clips(&state.motion)))
                .collect();
        }
        self.machines.get(path).cloned().unwrap_or_default()
    }
}
