//! Material files on parts: assigning one by dropping it (or
//! double-clicking it for the selection), and Save as Material.
//!
//! Assigning sets the part's `MaterialVariant`, and a Part's `Color` to
//! white, since the material carries its color; one drop is one undo step.

use axcenity_assets::{MATERIAL_SUFFIX, MATERIALS_FOLDER, material_json};
use axcenity_core::{ClassName, Color3, InstanceId, Value};

use super::EditorApp;
use axcenity_project as project;

use super::texture_set::property_changes;
use crate::asset_browser::{AssetEntry, display_name, prefab_file_stem};
use crate::history;
use crate::tree;

impl EditorApp {
    /// Draws `hit` in the material file `entry`. With no hit (a
    /// double-click), the selected parts instead.
    pub(super) fn apply_material(&mut self, entry: &AssetEntry, hit: Option<InstanceId>) {
        let targets: Vec<InstanceId> = match hit {
            Some(id) => vec![id],
            None => self.selection.clone(),
        };
        self.use_material(&entry.path, &entry.name, targets);
    }

    /// Draws each Part and MeshPart of `targets` in the material file at
    /// `path` (called `name`), as one undo step.
    pub(super) fn use_material(&mut self, path: &str, name: &str, targets: Vec<InstanceId>) {
        let mut properties = Vec::new();
        for id in targets {
            let Some(class) = self.world.get(id).map(|instance| instance.class()) else {
                continue;
            };
            if !class.is_base_part() {
                continue;
            }
            let before = history::snapshot(&self.world, id);
            let _ = self
                .world
                .set_property(id, "MaterialVariant", Value::String(path.to_owned()));
            if matches!(
                class,
                ClassName::Part | ClassName::Seat | ClassName::VehicleSeat | ClassName::DamageZone
            ) {
                let _ =
                    self.world
                        .set_property(id, "Color", Value::Color3(Color3::new(1.0, 1.0, 1.0)));
            }
            properties.extend(property_changes(
                id,
                before,
                history::snapshot(&self.world, id),
            ));
        }
        if properties.is_empty() {
            self.output.info(
                "Drop a material onto a part or MeshPart, or select some and double-click the \
                 material.",
            );
            return;
        }
        self.history.record_tree_and_properties(
            &mut self.world,
            format!("Material {name}"),
            Vec::new(),
            properties,
        );
    }

    /// Saves the look of the Part `id` as a new material file in
    /// `assets/materials`, named after the part.
    pub(super) fn save_material(&mut self, id: InstanceId) {
        let Some(folder) = self.project.as_ref().map(|project| project.folder.clone()) else {
            return;
        };
        let Some(file) = axcenity_scene::part_material_file(&self.world, id, &self.assets) else {
            self.output
                .error("Only a Part's look can be saved as a material.");
            return;
        };
        let name = self
            .world
            .get(id)
            .map(|instance| instance.name().to_owned())
            .unwrap_or_default();
        let stem = prefab_file_stem(&name);
        let path = tree::numbered_names(&stem)
            .map(|stem| format!("{MATERIALS_FOLDER}/{stem}{MATERIAL_SUFFIX}"))
            .find(|path| !folder.join(path).exists())
            .expect("numbered names never run out");
        let text = material_json(&file);
        match project::write_script(&folder, &path, &text) {
            Ok(()) => {
                self.toasts.success(format!("Saved the material `{path}`"));
                self.output.info(format!(
                    "Saved the material `{path}`. Drag it onto parts to use it."
                ));
                self.assets.forget(&path);
                self.record_asset_files(
                    format!("Create {}", display_name(&path)),
                    vec![crate::animation::store::FileDiff {
                        path,
                        before: None,
                        after: Some(text),
                    }],
                    None,
                );
            }
            Err(err) => self
                .output
                .error(format!("Could not save the material `{path}`: {err}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use axcenity_core::Material;

    use super::*;
    use crate::app::tests::{copy_of_example, part};
    use crate::asset_browser::AssetKind;
    use crate::explorer::Action;

    fn text(app: &EditorApp, id: InstanceId, name: &str) -> String {
        app.world
            .get(id)
            .unwrap()
            .property(name)
            .and_then(Value::as_str)
            .unwrap()
            .to_owned()
    }

    #[test]
    fn a_parts_look_saves_as_a_material_that_drops_onto_another_part_as_one_undo_step() {
        let folder = copy_of_example("hello-touched", "save-material");
        let mut app = EditorApp::new(Some(&folder));
        let (pad, ball) = (part(&app, "Pad"), part(&app, "Ball"));
        app.world
            .set_property(pad, "Material", Value::Material(Material::Metal))
            .unwrap();
        app.act(Action::SaveMaterial(pad));
        let path = "assets/materials/Pad.material.json";
        let saved = axcenity_assets::read_material_file(&folder.join(path)).unwrap();
        assert_eq!(saved.metalness, Some(1.0));
        let pad_color = app.world.get(pad).unwrap().property("Color").cloned();
        let [r, g, b] = saved.color.unwrap();
        assert_eq!(pad_color, Some(Value::Color3(Color3::new(r, g, b))));
        assert_eq!(
            crate::asset_browser::kind_of(path),
            Some(AssetKind::Material)
        );

        let ball_color = app.world.get(ball).unwrap().property("Color").cloned();
        let entry = AssetEntry {
            kind: AssetKind::Material,
            path: path.to_owned(),
            name: "Pad".to_owned(),
        };
        app.drop_asset(&entry, None);
        // Nothing selected: nothing changes.
        assert_eq!(text(&app, ball, "MaterialVariant"), "");
        app.apply_material(&entry, Some(ball));
        assert_eq!(text(&app, ball, "MaterialVariant"), path);
        let white = Value::Color3(Color3::new(1.0, 1.0, 1.0));
        assert_eq!(app.world.get(ball).unwrap().property("Color"), Some(&white));
        app.undo();
        assert_eq!(text(&app, ball, "MaterialVariant"), "");
        assert_eq!(
            app.world.get(ball).unwrap().property("Color").cloned(),
            ball_color
        );
        app.redo();
        assert_eq!(text(&app, ball, "MaterialVariant"), path);

        // Saving it again doesn't overwrite the first file.
        app.act(Action::SaveMaterial(pad));
        assert!(folder.join("assets/materials/Pad2.material.json").exists());
        let _ = std::fs::remove_dir_all(folder);
    }
}
