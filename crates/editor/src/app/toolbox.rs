//! What the Toolbox tab's actions do in the editor: a texture set is
//! applied where it was asked for, a sky asks to be used, a model goes
//! through the Import dialog (and its install is recorded once it's
//! imported), and Make material writes a material file and opens it.

use axcenity_assets::{MATERIAL_SUFFIX, MATERIALS_FOLDER, material_json, read_texture_set};

use super::*;
use crate::asset_browser::{AssetEntry, AssetKind, display_name, prefab_file_stem};
use crate::terrain_studio::TerrainAction;
use crate::toolbox_panel::{CardDrag, Spot, Target, ToolboxAction, install};
use crate::viewport::DropTarget;
use axcenity_render::glam::Vec3;

/// Where a dropped Toolbox card lands, as the viewport gives a dropped tile.
fn drop_target(spot: Spot) -> DropTarget {
    DropTarget {
        point: Vec3::new(spot.point.x, spot.point.y, spot.point.z),
        normal: Vec3::new(spot.normal.x, spot.normal.y, spot.normal.z),
        part: spot.part,
        ui: spot.ui,
        released: true,
    }
}

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

    /// A Toolbox card was dragged onto the viewport and let go at `target`:
    /// the Toolbox gets it, and its install is placed there (texture sets on
    /// the part, sounds at the point; skies ask and models go through the
    /// Import dialog, as Get does).
    pub(in crate::app) fn drop_toolbox_card(
        &mut self,
        ctx: &egui::Context,
        card: &CardDrag,
        target: DropTarget,
    ) {
        if self.playing() {
            return;
        }
        let Some(folder) = self.project.as_ref().map(|project| project.folder.clone()) else {
            return;
        };
        let spot = Spot {
            point: Vector3::new(target.point.x, target.point.y, target.point.z),
            normal: Vector3::new(target.normal.x, target.normal.y, target.normal.z),
            part: target.part,
            ui: target.ui,
        };
        if !self
            .toolbox
            .get_dropped(ctx, &folder, card, Target::Drop(spot))
        {
            self.output
                .error(format!("Could not get {} from the Toolbox now.", card.name));
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
                let entry = AssetEntry {
                    kind: AssetKind::TextureSet,
                    path,
                    name,
                };
                match target {
                    Some(Target::TerrainLayer(layer)) => {
                        self.terrain_action(TerrainAction::UseTextureSet(layer, entry));
                    }
                    Some(Target::Drop(spot)) => self.drop_asset(&entry, Some(drop_target(spot))),
                    None => {}
                }
            }
            ToolboxAction::AskSky(path) => {
                self.asset_browser.refresh();
                self.ask_sky(path);
            }
            ToolboxAction::Sound { path, target } => {
                self.asset_browser.refresh();
                if let Some(Target::Drop(spot)) = target {
                    let entry = AssetEntry {
                        kind: AssetKind::Audio,
                        name: display_name(&path),
                        path,
                    };
                    self.drop_asset(&entry, Some(drop_target(spot)));
                }
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sound_dragged_from_the_toolbox_onto_a_part_lands_inside_it() {
        let temp =
            std::env::temp_dir().join(format!("axcenity-toolbox-drop-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp);
        std::fs::create_dir_all(&temp).unwrap();
        let folder = axcenity_project::create_project(&temp, "Toolbox Drop").unwrap();
        std::fs::create_dir_all(folder.join("assets/audio")).unwrap();
        std::fs::write(folder.join("assets/audio/rain-1a2b.ogg"), b"sound").unwrap();
        let mut app = EditorApp::new(Some(&folder));
        let part = app.world.create(ClassName::Part).unwrap();
        app.world
            .set_parent(part, Some(app.world.workspace()))
            .unwrap();
        let spot = Spot {
            point: Vector3::new(0.0, 1.0, 0.0),
            normal: Vector3::new(0.0, 1.0, 0.0),
            part: Some(part),
            ui: None,
        };

        app.toolbox_action(ToolboxAction::Sound {
            path: "assets/audio/rain-1a2b.ogg".to_owned(),
            target: Some(Target::Drop(spot)),
        });
        let sound = app.selected.expect("the placed sound is selected");
        let instance = app.world.get(sound).unwrap();
        assert_eq!(instance.class(), ClassName::Sound);
        assert_eq!(instance.parent(), Some(part));
        // Got without a drop (the Get button), nothing is placed.
        let children = app.world.get(part).unwrap().children().len();
        app.toolbox_action(ToolboxAction::Sound {
            path: "assets/audio/rain-1a2b.ogg".to_owned(),
            target: None,
        });
        assert_eq!(app.world.get(part).unwrap().children().len(), children);
        let _ = std::fs::remove_dir_all(&temp);
    }
}
