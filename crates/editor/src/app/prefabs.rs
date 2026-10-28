//! Placed prefab copies (Phase 49): Apply overrides, Revert, Unpack and New
//! variant. Each is one undo step that keeps the whole world and the
//! prefab file it wrote (`History::record_world_and_files`). After a
//! prefab file is written, every copy in the world is brought up to date
//! from the files ([`EditorApp::refresh_prefab_copies`]).

use std::path::PathBuf;

use axcenity_core::prefab::{self, Overrides, PrefabSource};
use axcenity_core::{InstanceId, World};
use axcenity_project::{self as project, Prefabs};

use super::EditorApp;
use crate::asset_browser::{PREFAB_SUFFIX, PREFABS_FOLDER, display_name, prefab_file_stem};
use crate::history::TextChange;
use crate::prefab_menu::{PrefabAction, prefab_name};
use crate::tree;

impl EditorApp {
    pub(in crate::app) fn prefab_action(&mut self, action: PrefabAction) {
        if self.playing() {
            return;
        }
        match action {
            PrefabAction::ApplyAll(root) => self.apply_overrides(root, None),
            PrefabAction::ApplyProperty { id, property } => {
                if let Some(place) = prefab::place(&self.world, id) {
                    self.apply_overrides(place.root, Some((id, property)));
                }
            }
            PrefabAction::RevertAll(root) => {
                let keep = prefab::overrides(&self.world, root).placement_only();
                self.revert_overrides(root, keep, "Revert all".to_owned());
            }
            PrefabAction::RevertProperty { id, property } => {
                let Some(place) = prefab::place(&self.world, id) else {
                    return;
                };
                let keep = place.overrides.without_property(&place.path, &property);
                self.revert_overrides(place.root, keep, format!("Revert {property}"));
            }
            PrefabAction::Unpack(root) => {
                let before = self.world.clone();
                if self.world.set_prefab(root, None).is_ok() {
                    let name = self.instance_name(root);
                    self.history
                        .record_world(&mut self.world, format!("Unpack {name}"), before);
                    self.output
                        .info(format!("Unpacked {name}: it no longer follows its prefab."));
                }
            }
        }
    }

    fn project_folder(&self) -> Option<PathBuf> {
        self.project.as_ref().map(|p| p.folder.clone())
    }

    fn instance_name(&self, id: InstanceId) -> String {
        self.world
            .get(id)
            .map(|i| i.name().to_owned())
            .unwrap_or_default()
    }

    /// What the copy at `root` was made from, read from its file if it has
    /// never been synced.
    fn copy_source(&mut self, root: InstanceId) -> Option<(std::sync::Arc<PrefabSource>, usize)> {
        let link = self.world.get(root)?.prefab()?.clone();
        if let Some(source) = link.source {
            return Some((source, link.index));
        }
        let folder = self.project_folder()?;
        match Prefabs::in_folder(&folder).source(&link.path) {
            Ok(source) => Some((source, link.index)),
            Err(err) => {
                self.output.error(err.to_string());
                None
            }
        }
    }

    /// Writes the copy at `root` into its prefab (only the property `only`
    /// of one of its Instances, when given) and brings every copy up to date.
    fn apply_overrides(&mut self, root: InstanceId, only: Option<(InstanceId, String)>) {
        let Some(folder) = self.project_folder() else {
            return;
        };
        let Some(path) = self
            .world
            .get(root)
            .and_then(|i| i.prefab())
            .map(|link| link.path.clone())
        else {
            return;
        };
        let mut prefabs = Prefabs::in_folder(&folder);
        let text = match &only {
            None => project::applied_text(&mut prefabs, &self.world, root),
            Some((id, property)) => {
                // The copy as its prefab with just that one change.
                let Some(place) = prefab::place(&self.world, *id) else {
                    return;
                };
                let Some((source, index)) = self.copy_source(root) else {
                    return;
                };
                let mut scratch = World::new();
                let workspace = scratch.workspace();
                let Ok(copy) = scratch.copy_from(&self.world, root) else {
                    return;
                };
                let _ = scratch.set_parent(copy, Some(workspace));
                let one = Overrides::only_property(&place.path, property);
                prefab::sync(&mut scratch, copy, &source, index, &one);
                project::applied_text(&mut prefabs, &scratch, copy)
            }
        };
        let text = match text {
            Ok(text) => text,
            Err(err) => {
                self.output
                    .error(format!("Could not apply to `{path}`: {err}"));
                return;
            }
        };
        let old = std::fs::read_to_string(folder.join(&path)).ok();
        if let Err(err) = project::write_script(&folder, &path, &text) {
            self.output
                .error(format!("Could not apply to `{path}`: {err}"));
            return;
        }
        let before = self.world.clone();
        self.refresh_prefab_copies();
        let what = match &only {
            Some((_, property)) => format!("Apply {property} to {}", prefab_name(&path)),
            None => format!("Apply overrides to {}", prefab_name(&path)),
        };
        self.history.record_world_and_files(
            &mut self.world,
            what.clone(),
            before,
            vec![TextChange {
                path: path.clone(),
                before: old,
                after: Some(text),
            }],
        );
        self.asset_browser.refresh();
        self.output
            .info(format!("{what}: every copy of `{path}` follows it."));
    }

    /// Makes the copy at `root` its prefab with only `keep` as overrides.
    fn revert_overrides(&mut self, root: InstanceId, keep: Overrides, label: String) {
        let Some((source, index)) = self.copy_source(root) else {
            return;
        };
        let before = self.world.clone();
        prefab::sync(&mut self.world, root, &source, index, &keep);
        let name = self.instance_name(root);
        self.history
            .record_world(&mut self.world, format!("{label} on {name}"), before);
        self.forget_selection_outside_tree();
    }

    /// Brings every copy in the world up to date with the prefab files, as
    /// they are on disk now. Not an undo step of its own.
    pub(in crate::app) fn refresh_prefab_copies(&mut self) {
        let Some(folder) = self.project_folder() else {
            return;
        };
        let mut prefabs = Prefabs::in_folder(&folder);
        let root = self.world.root();
        prefabs.refresh(&mut self.world, root);
        for problem in prefabs.take_problems() {
            self.output
                .warning(format!("{problem}, so its copies stay as they are."));
        }
        self.forget_selection_outside_tree();
    }

    /// Adds a linked copy of the prefab at `path` inside `parent`.
    pub(in crate::app) fn instantiate_prefab(
        &mut self,
        path: &str,
        parent: InstanceId,
    ) -> Result<Vec<InstanceId>, String> {
        let folder = self.project_folder().ok_or("no project is open")?;
        let mut prefabs = Prefabs::in_folder(&folder);
        let added = prefabs
            .instantiate(path, &mut self.world, parent)
            .map_err(|err| err.to_string())?;
        for problem in prefabs.take_problems() {
            self.output.warning(problem.to_string());
        }
        Ok(added)
    }

    /// Links `roots` (just saved as the prefab at `path`) to it, as part of
    /// the undo step that keeps `before` and the new file.
    pub(in crate::app) fn link_saved_prefab(
        &mut self,
        roots: &[InstanceId],
        path: &str,
        text: String,
        before: World,
    ) {
        let Some(folder) = self.project_folder() else {
            return;
        };
        if let Ok(source) = Prefabs::in_folder(&folder).source(path) {
            for (index, &root) in roots.iter().enumerate() {
                let mut link = prefab::PrefabLink::new(path, index);
                link.source = Some(source.clone());
                let _ = self.world.set_prefab(root, Some(link));
            }
        }
        self.history.record_world_and_files(
            &mut self.world,
            format!("Create {}", display_name(path)),
            before,
            vec![TextChange {
                path: path.to_owned(),
                before: None,
                after: Some(text),
            }],
        );
    }

    /// Makes `<name> Variant.prefab.json` beside nothing but the prefabs:
    /// a prefab whose base is the prefab at `base`, with no overrides yet.
    pub(in crate::app) fn new_variant(&mut self, base: &str) {
        let Some(folder) = self.project_folder() else {
            return;
        };
        if self.playing() {
            return;
        }
        let text = match project::variant_text(&mut Prefabs::in_folder(&folder), base) {
            Ok(text) => text,
            Err(err) => {
                self.output
                    .error(format!("Could not make a variant of `{base}`: {err}"));
                return;
            }
        };
        let stem = prefab_file_stem(&format!("{} Variant", display_name(base)));
        let path = tree::numbered_names(&stem)
            .map(|stem| format!("{PREFABS_FOLDER}/{stem}{PREFAB_SUFFIX}"))
            .find(|path| !folder.join(path).exists())
            .expect("numbered names never run out");
        if let Err(err) = project::write_script(&folder, &path, &text) {
            self.output
                .error(format!("Could not save the variant `{path}`: {err}"));
            return;
        }
        self.toasts.success(format!("Made the variant `{path}`"));
        self.output.info(format!(
            "Made `{path}`, a variant of `{base}`. Place it, change it, and apply its \
             overrides; changes to `{base}` reach it except where it has its own."
        ));
        self.record_asset_files(
            format!("New variant of {}", display_name(base)),
            vec![crate::animation::store::FileDiff {
                path,
                before: None,
                after: Some(text),
            }],
            None,
        );
        self.asset_browser.refresh();
    }
}
