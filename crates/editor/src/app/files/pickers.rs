use super::*;

impl EditorApp {
    /// How a reference to the project file at `path` (maybe with a model's
    /// `#pick`) is written into the world: by its asset ID, so it keeps
    /// working when the file is renamed or moved, giving the file one first
    /// if it takes one (a file just imported has none yet). Else the path.
    pub(in crate::app) fn reference_to(&self, path: String) -> String {
        let Some(folder) = self.project.as_ref().map(|project| &project.folder) else {
            return path;
        };
        let file = path.split('#').next().unwrap_or(&path);
        if let Err(err) = axcenity_assets::give_asset_id(folder, file) {
            log::warn!("couldn't give `{file}` an asset ID: {err}");
        }
        self.assets.asset_reference(&path)
    }

    /// Asks for a texture file with the system's file picker, for a map
    /// in Properties, and returns its path in the project: a file outside
    /// the project is copied into `assets/textures` first. `None` if
    /// cancelled or it can't be used (said in Output).
    pub(in crate::app) fn pick_texture(&mut self, into: &str) -> Option<String> {
        let project = self.project.as_ref()?;
        let file = rfd::FileDialog::new()
            .set_title("Choose a texture")
            .add_filter("Texture", &axcenity_assets::TEXTURE_EXTENSIONS)
            .set_directory(&project.folder)
            .pick_file()?;
        match axcenity_assets::import_texture(&project.folder, &file, into) {
            Ok(path) => Some(path),
            Err(err) => {
                let err = chain(&err);
                self.output
                    .error(format!("Could not use `{}`: {err}", file.display()));
                None
            }
        }
    }

    /// Asks for a ColorCorrectionEffect's lookup table (`LookupTable`) or
    /// palette image (`Palette`), and returns its path in the project,
    /// copied into `assets/grading` first if it was outside.
    pub(in crate::app) fn pick_grading_file(&mut self, name: &str) -> Option<String> {
        let project = self.project.as_ref()?;
        let dialog = rfd::FileDialog::new().set_directory(&project.folder);
        let dialog = if name == "LookupTable" {
            dialog
                .set_title("Choose a lookup table")
                .add_filter("Lookup table", &axcenity_assets::LOOKUP_EXTENSIONS)
        } else {
            dialog
                .set_title("Choose a palette")
                .add_filter("Palette image", &axcenity_assets::TEXTURE_EXTENSIONS)
        };
        let file = dialog.pick_file()?;
        match axcenity_assets::import_grading_file(&project.folder, &file) {
            Ok(path) => Some(path),
            Err(err) => {
                let err = chain(&err);
                self.output
                    .error(format!("Could not use `{}`: {err}", file.display()));
                None
            }
        }
    }

    /// Asks for a sound file, and returns its `SoundId`: its path in the
    /// project, copied into `assets/audio` first if it was outside.
    pub(in crate::app) fn pick_audio(&mut self) -> Option<String> {
        let project = self.project.as_ref()?;
        let file = rfd::FileDialog::new()
            .set_title("Choose a sound")
            .add_filter("Sound", &axcenity_assets::AUDIO_EXTENSIONS)
            .set_directory(&project.folder)
            .pick_file()?;
        match axcenity_assets::import_audio(&project.folder, &file) {
            Ok(path) => Some(path),
            Err(err) => {
                let err = chain(&err);
                self.output
                    .error(format!("Could not use `{}`: {err}", file.display()));
                None
            }
        }
    }

    /// Asks for a behavior tree file inside the project, and returns its
    /// path there for a NavigationAgent's `BehaviorTree`.
    pub(in crate::app) fn pick_behavior_tree(&mut self) -> Option<String> {
        use axcenity_blocks::behavior_tree::{BEHAVIOR_TREES_FOLDER, is_behavior_tree};
        let project = self.project.as_ref()?;
        let trees = project.folder.join(BEHAVIOR_TREES_FOLDER);
        let start = if trees.is_dir() {
            trees
        } else {
            project.folder.clone()
        };
        let file = rfd::FileDialog::new()
            .set_title("Choose a behavior tree")
            .add_filter("Behavior tree", &["json"])
            .set_directory(&start)
            .pick_file()?;
        let relative = file
            .strip_prefix(&project.folder)
            .ok()
            .map(|path| {
                path.components()
                    .map(|part| part.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("/")
            })
            .filter(|path| is_behavior_tree(path));
        if relative.is_none() {
            self.output.error(format!(
                "Could not use `{}`: choose a `.behaviortree.json` file inside the project folder.",
                file.display()
            ));
        }
        relative
    }

    /// Asks for the code file of the selected script inside an object (its
    /// `SourceFile`, G2) inside the project, with the ending its class needs,
    /// and returns its path there.
    pub(in crate::app) fn pick_source_file(&mut self) -> Option<String> {
        let class = self.world.get(self.selected?)?.class();
        let project = self.project.as_ref()?;
        let objects = project.folder.join(project::OBJECTS_FOLDER);
        let start = if objects.is_dir() {
            objects
        } else {
            project.folder.clone()
        };
        let file = rfd::FileDialog::new()
            .set_title("Choose the script's file")
            .add_filter("Script", &["lua", "json"])
            .set_directory(&start)
            .pick_file()?;
        let relative = file
            .strip_prefix(&project.folder)
            .ok()
            .map(|path| {
                path.components()
                    .map(|part| part.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("/")
            })
            .filter(|path| project::source_file_fits(class, path));
        if relative.is_none() {
            self.output.error(format!(
                "Could not use `{}`: a {} needs a file inside the project folder ending in {}.",
                file.display(),
                class.as_str(),
                match class {
                    ClassName::LocalScript => "`.client.lua` or `.client.blocks.json`",
                    ClassName::ModuleScript => "`.module.lua`",
                    _ => "`.server.lua` or `.server.blocks.json`",
                }
            ));
        }
        relative
    }

    /// Asks for an animator controller file inside the project, and returns
    /// its path there for an Animator's `Controller`.
    pub(in crate::app) fn pick_controller(&mut self) -> Option<String> {
        let project = self.project.as_ref()?;
        let animations = project.folder.join(axcenity_anim::ANIMATIONS_FOLDER);
        let start = if animations.is_dir() {
            animations
        } else {
            project.folder.clone()
        };
        let file = rfd::FileDialog::new()
            .set_title("Choose a state machine")
            .add_filter("State machine", &["json"])
            .set_directory(&start)
            .pick_file()?;
        let relative = file
            .strip_prefix(&project.folder)
            .ok()
            .map(|path| {
                path.components()
                    .map(|part| part.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("/")
            })
            .filter(|path| path.ends_with(axcenity_anim::STATE_MACHINE_EXTENSION));
        if relative.is_none() {
            self.output.error(format!(
                "Could not use `{}`: choose a `.statemachine.json` file inside the project folder.",
                file.display()
            ));
        }
        relative
    }

    /// Asks for a property clip (`.anim.json` with `"kind": "property"`).
    pub(in crate::app) fn pick_property_clip(&mut self) -> Option<String> {
        let project = self.project.as_ref()?;
        let animations = project.folder.join(axcenity_anim::ANIMATIONS_FOLDER);
        let start = if animations.is_dir() {
            animations
        } else {
            project.folder.clone()
        };
        let file = rfd::FileDialog::new()
            .set_title("Choose a property clip")
            .add_filter("Animation clip", &["json"])
            .set_directory(&start)
            .pick_file()?;
        let relative = file
            .strip_prefix(&project.folder)
            .ok()
            .map(|path| {
                path.components()
                    .map(|part| part.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("/")
            })
            .filter(|path| path.ends_with(axcenity_anim::CLIP_EXTENSION));
        let Some(path) = relative else {
            self.output.error(format!(
                "Could not use `{}`: choose a `.anim.json` file inside the project folder.",
                file.display()
            ));
            return None;
        };
        match store::read_any_clip(&project.folder, &path) {
            Ok(_) => Some(path),
            Err(err) => {
                self.output.error(format!("Could not read `{path}`: {err}"));
                None
            }
        }
    }

    /// Plays the Sound `id` in the editor (or stops it), with its own
    /// settings. Starting Play stops it too.
    pub(in crate::app) fn preview_sound(&mut self, id: InstanceId, play: bool) {
        if !play {
            self.sound_preview = None;
            return;
        }
        let Some(sound) = self.world.get(id) else {
            return;
        };
        let text = |name| {
            sound
                .property(name)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned()
        };
        let action = axcenity_audio::SoundAction::Play {
            instance: id,
            sound_id: text("SoundId"),
            volume: sound
                .property("Volume")
                .and_then(Value::as_number)
                .unwrap_or(1.0),
            looped: sound.property("Looped").and_then(Value::as_bool) == Some(true),
        };
        if self.sound_preview.is_none() {
            match axcenity_audio::AudioEngine::new() {
                Ok(engine) => self.sound_preview = Some(engine),
                Err(err) => {
                    self.output.error(format!("Audio is unavailable: {err}"));
                    return;
                }
            }
        }
        let (position, orientation) = self.viewport.camera_view();
        let listener = axcenity_audio::Listener::from_view(position, orientation);
        if let Some(engine) = &mut self.sound_preview {
            engine.set_listener(listener);
            if let Err(err) = engine.apply(action, &self.world, &self.assets) {
                self.output.error(format!("Audio: {err}"));
            }
        }
    }

    /// Keeps a Properties preview in the place the camera and the Sound's
    /// part are now. A spatial preview pans as the viewport camera moves.
    pub(in crate::app) fn follow_sound_preview(&mut self) {
        if self.playing() {
            return;
        }
        let Some(engine) = &mut self.sound_preview else {
            return;
        };
        let (position, orientation) = self.viewport.camera_view();
        let listener = axcenity_audio::Listener::from_view(position, orientation);
        for err in engine.follow(&self.world, &self.assets, listener) {
            self.output.error(format!("Audio: {err}"));
        }
    }
}
