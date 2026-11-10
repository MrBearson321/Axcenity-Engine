//! The Material tool's library: built-in materials, Kenney's prototype
//! textures, and custom materials made from a project's own texture maps.
//!
//! A custom material is an ordinary material file (`docs/rendering/models.md`),
//! so the Material tab can edit it further and the player draws it as it
//! draws any other. Its images are copied into
//! `assets/materials/<name>/`, next to nothing outside the project.

use std::path::{Path, PathBuf};

use axcenity_assets::{
    MATERIAL_SUFFIX, MATERIALS_FOLDER, MapFile, MaterialFile, MaterialMaps, material_json,
    parse_material,
};
use axcenity_core::{InstanceId, Material, Value};
use eframe::egui;

use super::EditorApp;
use crate::animation::store::FileDiff;
use crate::asset_browser::{display_name, prefab_file_stem};
use crate::build_tools;
use crate::history::Field;
use crate::properties::Edit;
use crate::prototype;
use crate::tree;
use crate::widgets::{self, Role};

/// The image files a material can be made from.
const IMAGE_EXTENSIONS: [&str; 7] = ["png", "jpg", "jpeg", "tga", "bmp", "exr", "hdr"];

/// The New Custom Material window's form.
#[derive(Default)]
pub(super) struct CustomMaterialForm {
    name: String,
    albedo: Option<PathBuf>,
    normal: Option<PathBuf>,
    roughness: Option<PathBuf>,
    /// Why the last Create didn't work.
    problem: Option<String>,
}

/// A map to copy into the project: from where, and the file name it gets.
struct MapCopy {
    from: PathBuf,
    name: String,
}

/// `name`'s project folder for its images.
fn images_folder(stem: &str) -> String {
    format!("{MATERIALS_FOLDER}/{stem}")
}

/// The material a form makes, and the images to copy for it.
fn from_form(stem: &str, form: &CustomMaterialForm) -> (MaterialFile, Vec<MapCopy>) {
    let mut copies = Vec::new();
    let mut map = |file: &Option<PathBuf>, slot: &str| {
        let from = file.as_ref()?.clone();
        let extension = from.extension()?.to_string_lossy().to_lowercase();
        let name = format!("{slot}.{extension}");
        let path = format!("{}/{name}", images_folder(stem));
        copies.push(MapCopy { from, name });
        Some(MapFile {
            path: Some(path),
            ..Default::default()
        })
    };
    let maps = MaterialMaps {
        color: map(&form.albedo, "albedo"),
        normal: map(&form.normal, "normal"),
        roughness: map(&form.roughness, "roughness"),
        ..Default::default()
    };
    let file = MaterialFile {
        color: Some([1.0, 1.0, 1.0]),
        metalness: Some(0.0),
        // A roughness map multiplies this, so it decides alone.
        roughness: Some(if maps.roughness.is_some() { 1.0 } else { 0.5 }),
        maps,
        ..Default::default()
    };
    (file, copies)
}

/// The image `path` in a material file at `source` names, looked for from
/// the file's own folder and then from each folder above it (a project's
/// paths start at the project folder).
fn locate_map(source: &Path, path: &str) -> Option<PathBuf> {
    source
        .ancestors()
        .skip(1)
        .map(|folder| folder.join(path))
        .find(|candidate| candidate.is_file())
}

/// `file` made ready for this project, its images under `stem`'s folder,
/// with what to copy and the names of any images that couldn't be found.
/// A base material is merged in, as the base isn't brought along.
fn imported(
    stem: &str,
    mut file: MaterialFile,
    source: &Path,
) -> (MaterialFile, Vec<MapCopy>, Vec<String>) {
    // Follow its bases, stopping at anything that can't be read or goes round.
    let mut seen = vec![source.to_path_buf()];
    while let Some(base) = file.base.take() {
        let Some(found) = locate_map(seen.last().expect("starts with one"), &base)
            .filter(|found| !seen.contains(found))
        else {
            break;
        };
        let Some(below) = std::fs::read_to_string(&found)
            .ok()
            .and_then(|text| parse_material(&text).ok())
        else {
            break;
        };
        file = file.over(&below);
        file.base = below.base;
        seen.push(found);
    }
    let (mut copies, mut missing) = (Vec::new(), Vec::new());
    let maps = &mut file.maps;
    let slots = [
        (&mut maps.color, "color"),
        (&mut maps.normal, "normal"),
        (&mut maps.metalness, "metalness"),
        (&mut maps.roughness, "roughness"),
        (&mut maps.occlusion, "occlusion"),
        (&mut maps.emissive, "emissive"),
        (&mut maps.opacity, "opacity"),
        (&mut maps.specular, "specular"),
    ];
    for (slot, role) in slots {
        let Some(map) = slot else { continue };
        let Some(path) = map.path.clone().filter(|path| !path.is_empty()) else {
            continue;
        };
        match locate_map(source, &path) {
            Some(from) => {
                let extension = from
                    .extension()
                    .map_or(String::new(), |e| e.to_string_lossy().to_lowercase());
                let name = format!("{role}.{extension}");
                map.path = Some(format!("{}/{name}", images_folder(stem)));
                copies.push(MapCopy { from, name });
            }
            None => {
                missing.push(path);
                *slot = None;
            }
        }
    }
    (file, copies, missing)
}

/// A name for a material file: `wanted`'s file stem, numbered if the
/// project already has a material or folder by that name.
fn free_stem(folder: &Path, wanted: &str) -> String {
    let stem = prefab_file_stem(wanted.trim());
    let stem = if stem.is_empty() {
        "Material".to_owned()
    } else {
        stem
    };
    tree::numbered_names(&stem)
        .find(|stem| {
            !folder
                .join(format!("{MATERIALS_FOLDER}/{stem}{MATERIAL_SUFFIX}"))
                .exists()
                && !folder.join(images_folder(stem)).exists()
        })
        .expect("numbered names never run out")
}

fn file_label(file: &Option<PathBuf>) -> String {
    file.as_ref()
        .and_then(|file| file.file_name())
        .map_or("None".to_owned(), |name| {
            name.to_string_lossy().into_owned()
        })
}

impl EditorApp {
    /// Draws `parts` in a built-in material: chooses it and takes away any
    /// material file or prototype texture, as one undo step.
    pub(super) fn use_built_in_material(&mut self, parts: &[InstanceId], material: Material) {
        let value = Value::Material(material);
        let edits: Vec<_> = parts
            .iter()
            .copied()
            .filter(|&id| {
                self.world
                    .get(id)
                    .is_some_and(|part| part.property("Material").is_some())
            })
            .map(|id| {
                let mut edit = vec![Edit::Property("Material", value.clone())];
                if self
                    .world
                    .get(id)
                    .is_some_and(|part| part.property("MaterialVariant").is_some())
                {
                    edit.push(Edit::Property(
                        "MaterialVariant",
                        Value::String(String::new()),
                    ));
                }
                (id, edit)
            })
            .collect();
        let Some(name) = edits
            .last()
            .and_then(|(id, _)| self.world.get(*id))
            .map(|part| part.name().to_owned())
        else {
            return;
        };
        let label = build_tools::step_label("Material", &value, edits.len(), &name);
        self.change_many(Field::Property("Material"), label, edits);
    }

    /// Copies a prototype texture into the project and draws `parts` in it.
    pub(super) fn use_prototype(&mut self, parts: Vec<InstanceId>, palette: usize, number: usize) {
        let Some(folder) = self.project.as_ref().map(|project| project.folder.clone()) else {
            return;
        };
        match prototype::install(&folder, palette, number) {
            Ok(path) => {
                self.assets.forget(&path);
                self.assets.forget(&prototype::image_path(palette, number));
                self.asset_browser.refresh();
                self.use_material(&path, &prototype::name(palette, number), parts);
            }
            Err(err) => self
                .output
                .error(format!("Could not add the prototype texture: {err}")),
        }
    }

    /// Writes a new material `stem` with `file` and its images, undoably
    /// (the images stay, like any import), and draws `parts` in it.
    fn add_material(
        &mut self,
        stem: &str,
        file: MaterialFile,
        copies: Vec<MapCopy>,
        parts: Vec<InstanceId>,
    ) -> Result<String, String> {
        let folder = self
            .project
            .as_ref()
            .map(|project| project.folder.clone())
            .ok_or("No project is open.")?;
        let images = folder.join(images_folder(stem));
        if !copies.is_empty() {
            std::fs::create_dir_all(&images).map_err(|err| err.to_string())?;
        }
        for copy in copies {
            std::fs::copy(&copy.from, images.join(&copy.name))
                .map_err(|err| format!("{}: {err}", copy.from.display()))?;
            self.assets
                .forget(&format!("{}/{}", images_folder(stem), copy.name));
        }
        let path = format!("{MATERIALS_FOLDER}/{stem}{MATERIAL_SUFFIX}");
        let text = material_json(&file);
        axcenity_project::write_script(&folder, &path, &text).map_err(|err| err.to_string())?;
        self.assets.forget(&path);
        self.asset_browser.refresh();
        self.record_asset_files(
            format!("Create {}", display_name(&path)),
            vec![FileDiff {
                path: path.clone(),
                before: None,
                after: Some(text),
            }],
            None,
        );
        if !parts.is_empty() {
            self.use_material(&path, stem, parts);
        }
        Ok(path)
    }

    /// Asks for a material file anywhere on the computer and brings it,
    /// with its images, into the project.
    pub(super) fn load_material_dialog(&mut self, parts: Vec<InstanceId>) {
        let file = rfd::FileDialog::new()
            .set_title("Load a material")
            .add_filter("Material", &["json"])
            .pick_file();
        if let Some(file) = file {
            self.load_material(&file, parts);
        }
    }

    pub(super) fn load_material(&mut self, source: &Path, parts: Vec<InstanceId>) {
        let Some(folder) = self.project.as_ref().map(|project| project.folder.clone()) else {
            return;
        };
        let parsed = std::fs::read_to_string(source)
            .map_err(|err| err.to_string())
            .and_then(|text| parse_material(&text).map_err(|err| err.to_string()));
        let file = match parsed {
            Ok(file) => file,
            Err(err) => {
                self.output
                    .error(format!("Could not read `{}`: {err}", source.display()));
                return;
            }
        };
        let wanted = source
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let wanted = wanted
            .strip_suffix(MATERIAL_SUFFIX)
            .or_else(|| wanted.strip_suffix(".json"))
            .unwrap_or(&wanted)
            .to_owned();
        let stem = free_stem(&folder, &wanted);
        let (file, copies, missing) = imported(&stem, file, source);
        for path in &missing {
            self.output
                .warning(format!("`{wanted}` names `{path}`, which wasn't found."));
        }
        match self.add_material(&stem, file, copies, parts) {
            Ok(path) => self.toasts.success(format!("Loaded the material `{path}`")),
            Err(err) => self
                .output
                .error(format!("Could not load the material: {err}")),
        }
    }

    /// Makes the material the form describes and draws `parts` in it.
    /// Keeps the form open, with the reason, if that fails.
    fn create_from_form(&mut self, parts: Vec<InstanceId>) {
        let Some(mut form) = self.custom_material.take() else {
            return;
        };
        let Some(folder) = self.project.as_ref().map(|project| project.folder.clone()) else {
            return;
        };
        let stem = free_stem(&folder, &form.name);
        let (file, copies) = from_form(&stem, &form);
        match self.add_material(&stem, file, copies, parts) {
            Ok(path) => self.toasts.success(format!("Made the material `{path}`")),
            Err(err) => {
                form.problem = Some(err);
                self.custom_material = Some(form);
            }
        }
    }

    /// The New Custom Material window, while it's open.
    pub(super) fn custom_material_window(&mut self, ctx: &egui::Context) {
        let Some(mut form) = self.custom_material.take() else {
            return;
        };
        let mut open = true;
        let mut create = false;
        widgets::tool_window(crate::theme::icons::PALETTE, "New Custom Material")
            .id("new custom material")
            .fit(340.0)
            .show(ctx, &mut open, |ui| {
                egui::Grid::new("custom material maps")
                    .num_columns(3)
                    .spacing([8.0, 6.0])
                    .show(ui, |ui| {
                        ui.label("Name");
                        ui.add(
                            egui::TextEdit::singleline(&mut form.name)
                                .hint_text("Material")
                                .desired_width(180.0),
                        );
                        ui.end_row();
                        for (label, slot) in [
                            ("Albedo", &mut form.albedo),
                            ("Normal", &mut form.normal),
                            ("Roughness", &mut form.roughness),
                        ] {
                            ui.label(label);
                            ui.add(
                                egui::Label::new(file_label(slot))
                                    .truncate()
                                    .selectable(false),
                            );
                            ui.horizontal(|ui| {
                                if ui.button("Browse…").clicked()
                                    && let Some(file) = rfd::FileDialog::new()
                                        .set_title(format!("Choose the {label} map"))
                                        .add_filter("Image", &IMAGE_EXTENSIONS)
                                        .pick_file()
                                {
                                    *slot = Some(file);
                                }
                                if slot.is_some() && ui.small_button("×").clicked() {
                                    *slot = None;
                                }
                            });
                            ui.end_row();
                        }
                    });
                ui.weak(
                    "Normal maps are read green-up (OpenGL). The Material tab can change that.",
                );
                if let Some(problem) = &form.problem {
                    ui.colored_label(crate::theme::palette(ui.ctx()).error, problem);
                }
                ui.add_space(6.0);
                let palette = crate::theme::palette(ui.ctx());
                if ui
                    .add(widgets::button(
                        Role::Primary,
                        None,
                        "Create and apply",
                        &palette,
                    ))
                    .clicked()
                {
                    create = true;
                }
            });
        self.custom_material = open.then_some(form);
        if create {
            let parts = self.tool_parts();
            self.create_from_form(parts);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::{copy_of_example, part};
    use axcenity_render::{Camera, glam::Vec3};

    fn app_with_part(test: &str) -> (PathBuf, EditorApp, InstanceId) {
        let folder = copy_of_example("hello-touched", test);
        let mut app = EditorApp::new(Some(&folder));
        let pad = part(&app, "Pad");
        app.selection = vec![pad];
        (folder, app, pad)
    }

    fn variant(app: &EditorApp, id: InstanceId) -> String {
        app.world
            .get(id)
            .unwrap()
            .property("MaterialVariant")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    }

    fn png(path: &Path) {
        image::RgbaImage::from_pixel(4, 4, image::Rgba([200, 100, 50, 255]))
            .save(path)
            .unwrap();
    }

    fn camera() -> Camera {
        Camera {
            position: Vec3::new(0.0, 40.0, 60.0),
            look_at: Vec3::ZERO,
            fov_y_degrees: 70.0,
        }
    }

    fn outside_folder(test: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!(
            "axcenity-custom-material-{test}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        folder
    }

    #[test]
    fn a_prototype_texture_is_added_to_the_project_and_drawn_with_one_undo() {
        let (folder, mut app, pad) = app_with_part("prototype");
        app.use_prototype(vec![pad], 3, 2);
        let path = variant(&app, pad);
        assert_eq!(path, "assets/materials/prototype/Orange 02.material.json");
        assert!(folder.join(&path).is_file());
        assert!(
            folder
                .join("assets/textures/prototype/Orange/texture_02.png")
                .is_file()
        );
        assert!(app.assets.material(&path).unwrap().maps.color.is_some());
        app.undo();
        assert_eq!(variant(&app, pad), "");
        let _ = std::fs::remove_dir_all(folder);
    }

    #[test]
    fn choosing_a_built_in_material_takes_away_a_material_file() {
        let (folder, mut app, pad) = app_with_part("built-in");
        app.use_prototype(vec![pad], 0, 1);
        assert_ne!(variant(&app, pad), "");
        app.history.seal();
        app.use_built_in_material(&[pad], Material::Marble);
        assert_eq!(variant(&app, pad), "");
        assert_eq!(
            app.world.get(pad).unwrap().property("Material"),
            Some(&Value::Material(Material::Marble))
        );
        // One step undoes both.
        app.undo();
        assert_ne!(variant(&app, pad), "");
        let _ = std::fs::remove_dir_all(folder);
    }

    #[test]
    fn a_custom_material_copies_its_maps_and_is_drawn_on_the_selected_part() {
        let (folder, mut app, pad) = app_with_part("custom");
        let outside = outside_folder("maps");
        let (albedo, normal) = (
            outside.join("bricks_diffuse.png"),
            outside.join("bricks_n.png"),
        );
        png(&albedo);
        png(&normal);
        app.custom_material = Some(CustomMaterialForm {
            name: "My Bricks".into(),
            albedo: Some(albedo),
            normal: Some(normal),
            ..Default::default()
        });
        app.create_from_form(vec![pad]);

        assert!(app.custom_material.is_none(), "the form closes");
        let path = variant(&app, pad);
        let file = axcenity_assets::read_material_file(&folder.join(&path)).expect("written");
        let color = file.maps.color.unwrap().path.unwrap();
        assert!(color.starts_with("assets/materials/"));
        assert!(folder.join(&color).is_file());
        assert!(
            folder
                .join(file.maps.normal.unwrap().path.unwrap())
                .is_file()
        );
        assert!(file.maps.roughness.is_none());

        let list = axcenity_scene::build_draw_list(&app.world, &app.assets, camera());
        assert!(
            list.items
                .iter()
                .any(|item| item.material.base_color_texture.is_some()),
            "the part draws with the albedo map"
        );
        let _ = std::fs::remove_dir_all(folder);
        let _ = std::fs::remove_dir_all(outside);
    }

    #[test]
    fn two_custom_materials_with_one_name_stay_separate() {
        let (folder, mut app, pad) = app_with_part("same-name");
        for _ in 0..2 {
            app.custom_material = Some(CustomMaterialForm {
                name: "Same".into(),
                ..Default::default()
            });
            app.create_from_form(vec![pad]);
        }
        assert!(variant(&app, pad).contains("Same2"));
        let _ = std::fs::remove_dir_all(folder);
    }

    #[test]
    fn a_material_loaded_from_elsewhere_brings_its_images_and_merges_its_base() {
        let (folder, mut app, pad) = app_with_part("load");
        let root = outside_folder("load");
        std::fs::create_dir_all(root.join("assets/materials")).unwrap();
        std::fs::create_dir_all(root.join("assets/textures")).unwrap();
        png(&root.join("assets/textures/a.png"));
        std::fs::write(
            root.join("assets/materials/base.material.json"),
            r#"{ "roughness": 0.25, "maps": { "color": { "path": "assets/textures/a.png" } } }"#,
        )
        .unwrap();
        let source = root.join("assets/materials/top.material.json");
        std::fs::write(
            &source,
            r#"{ "base": "assets/materials/base.material.json", "metalness": 0.75,
                 "maps": { "normal": { "path": "assets/textures/gone.png" } } }"#,
        )
        .unwrap();

        app.load_material(&source, vec![pad]);

        let path = variant(&app, pad);
        let file = axcenity_assets::read_material_file(&folder.join(&path)).unwrap();
        assert_eq!(file.base, None);
        assert_eq!((file.metalness, file.roughness), (Some(0.75), Some(0.25)));
        let color = file.maps.color.unwrap().path.unwrap();
        assert!(folder.join(&color).is_file(), "the base's image came too");
        assert!(file.maps.normal.is_none(), "a missing image is left out");
        let _ = std::fs::remove_dir_all(folder);
        let _ = std::fs::remove_dir_all(root);
    }
}
