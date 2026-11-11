//! What the Toolbox tab's actions do in the editor: a texture set is
//! applied where it was asked for, a sky asks to be used, a model goes
//! through the Import dialog (and its install is recorded once it's
//! imported), and Make material writes a material file and opens it.

use axcenity_assets::{MATERIAL_SUFFIX, MATERIALS_FOLDER, material_json, read_texture_set};

use super::*;
use crate::asset_browser::{AssetEntry, AssetKind, display_name, prefab_file_stem};
use crate::terrain_studio::TerrainAction;
use crate::toolbox_panel::{Target, ToolboxAction, install};

impl EditorApp {
    /// The Toolbox tab.
    pub(in crate::app) fn toolbox_pane(&mut self, ui: &mut egui::Ui) {
        let folder = self.project.as_ref().map(|project| project.folder.clone());
        let actions = self.toolbox.show(ui, folder.as_deref(), !self.playing());
        for action in actions {
            self.toolbox_action(action);
        }
    }

    /// Shows the Toolbox tab; with a target, the next texture got fills it.
    pub(in crate::app) fn open_toolbox(&mut self, target: Option<Target>) {
        self.layout.open(Pane::Toolbox);
        self.toolbox.opened();
        if let Some(target) = target {
            self.toolbox.aim(target);
        }
    }

    pub(in crate::app) fn toolbox_action(&mut self, action: ToolboxAction) {
        // Whatever the Toolbox did may have downloaded or installed something.
        self.inventory.refresh();
        match action {
            ToolboxAction::Notice { error: true, text } => self.output.error(text),
            ToolboxAction::Notice { error: false, text } => self.output.info(text),
            ToolboxAction::TextureSet { path, name, target } => {
                self.asset_browser.refresh();
                if let Some(Target::TerrainLayer(layer)) = target {
                    let entry = AssetEntry {
                        kind: AssetKind::TextureSet,
                        path,
                        name,
                    };
                    self.terrain_action(TerrainAction::UseTextureSet(layer, entry));
                }
            }
            ToolboxAction::AskSky(path) => {
                self.asset_browser.refresh();
                self.ask_sky(path);
            }
            ToolboxAction::Sound(_) => self.asset_browser.refresh(),
            ToolboxAction::ImportModel { file, key, variant } => {
                self.toolbox_imports.insert(file.clone(), (key, variant));
                self.import_model(&file);
            }
            ToolboxAction::MakeMaterial { set, name } => self.make_material(&set, &name),
            ToolboxAction::OpenInventory => self.open_inventory(),
            ToolboxAction::ImportPlugin(file) => {
                let origin = if self.project.is_some() {
                    axcenity_plugins::Origin::Project
                } else {
                    axcenity_plugins::Origin::User
                };
                self.install_plugin_package(&file, origin);
                if let Some(notice) = self.plugins.notice.take() {
                    if notice.starts_with("Could not") {
                        self.output.error(notice);
                    } else {
                        self.output.info(notice);
                    }
                }
                self.layout.open(Pane::Plugins);
            }
        }
    }

    /// Records a model the Toolbox got once the Import dialog has copied
    /// it into the project as `copy`.
    pub(in crate::app) fn toolbox_imported(&mut self, file: &Path, copy: &str) {
        let Some((key, variant)) = self.toolbox_imports.remove(file) else {
            return;
        };
        if let Some(project) = &self.project {
            install::record_model_install(&project.folder, copy, &key, &variant);
        }
    }

    /// Writes `assets/materials/<name>.material.json` from the texture set
    /// at `set` (one undo step, as creating any asset file is) and opens it
    /// in the Material tab.
    pub(in crate::app) fn make_material(&mut self, set: &str, name: &str) {
        let Some(folder) = self.project.as_ref().map(|project| project.folder.clone()) else {
            return;
        };
        if self.playing() {
            return;
        }
        let Some(relative) = axcenity_assets::safe_path(set) else {
            return;
        };
        let texture_set = match read_texture_set(&folder.join(relative)) {
            Ok(texture_set) => texture_set,
            Err(err) => {
                self.output.error(format!("Could not read `{set}`: {err}."));
                return;
            }
        };
        let material = install::material_from_set(&texture_set);
        let stem = prefab_file_stem(name);
        let path = tree::numbered_names(&stem)
            .map(|stem| format!("{MATERIALS_FOLDER}/{stem}{MATERIAL_SUFFIX}"))
            .find(|path| !folder.join(path).exists())
            .expect("numbered names never run out");
        let text = material_json(&material);
        if let Err(err) = axcenity_project::write_script(&folder, &path, &text) {
            self.output
                .error(format!("Could not save the material `{path}`: {err}"));
            return;
        }
        self.assets.forget(&path);
        self.record_asset_files(
            format!("Create {}", display_name(&path)),
            vec![crate::animation::store::FileDiff {
                path: path.clone(),
                before: None,
                after: Some(text),
            }],
            None,
        );
        self.output.info(format!(
            "Made the material `{path}` from {name}. Drag it onto parts to use it."
        ));
        self.asset_browser.refresh();
        self.open_material(&path);
    }
}
