//! The Import Model dialog, shown when a model file holds more than one
//! object, has texture maps or is animated: tick which objects to import,
//! choose how they arrive, look over the maps, and make clips from the
//! file's takes (see `anim_import.rs`).
//!
//! The ticking rules are plain methods, so they're easy to test; the app
//! turns the answer into Instances.

use std::ops::Range;
use std::path::PathBuf;

use axcenity_assets::{Channel, ModelObject};
use eframe::egui;

use crate::anim_import::AnimationImport;
use crate::surface::{MapFile, MaterialMaps, Slot};
use crate::theme::icons;

/// How the ticked objects arrive in the world.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Placement {
    /// A Model named after the file, holding a MeshPart per object where
    /// it is in the file. Groups in the file become Models inside it.
    #[default]
    Grouped,
    /// A MeshPart per object, each at the insert point.
    Separate,
    /// One MeshPart drawing all of them.
    Merged,
}

/// What the dialog was answered with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Answer {
    Import,
    Cancel,
}

pub struct ImportDialog {
    /// The model file being imported, outside the project.
    pub file: PathBuf,
    pub objects: Vec<ModelObject>,
    /// One per object. Only the ticks of objects with meshes matter; a
    /// group shows its contents' ticks.
    ticked: Vec<bool>,
    pub placement: Placement,
    /// Only objects whose names contain this are shown (ignoring case).
    filter: String,
    /// False for a file with one object: then there's nothing to choose,
    /// and the dialog only shows the materials.
    pub pick_objects: bool,
    /// The maps of each of the file's materials, as found, then as changed.
    pub materials: Vec<MaterialMaps>,
    /// The file's skeleton and takes, and the clips to make, if it's animated.
    pub animation: Option<AnimationImport>,
    /// True for a file with only animation in it (nothing to draw): only
    /// its clips are imported.
    pub animation_only: bool,
}

impl ImportDialog {
    /// The dialog for `file`'s `objects`, with all of them ticked, and
    /// its `materials`. With one object, it comes in as one MeshPart.
    pub fn new(file: PathBuf, objects: Vec<ModelObject>, materials: Vec<MaterialMaps>) -> Self {
        let meshes = objects.iter().filter(|object| object.has_mesh()).count();
        Self {
            file,
            ticked: vec![true; objects.len()],
            objects,
            placement: if meshes > 1 {
                Placement::default()
            } else {
                Placement::Merged
            },
            filter: String::new(),
            pick_objects: meshes > 1,
            materials,
            animation: None,
            animation_only: false,
        }
    }

    /// The dialog for a file with only animation in it.
    pub fn animation_only(file: PathBuf, animation: AnimationImport) -> Self {
        Self {
            animation: Some(animation),
            animation_only: true,
            ..Self::new(file, Vec::new(), Vec::new())
        }
    }

    /// Adds the file's animation. An animated file comes in as one
    /// MeshPart by default, so one Animator moves all of it.
    pub fn with_animation(mut self, animation: Option<AnimationImport>) -> Self {
        if animation.is_some() {
            self.placement = Placement::Merged;
        }
        self.animation = animation;
        self
    }

    /// True if there's anything to ask: objects to choose from, texture
    /// maps to look over, or clips to make.
    pub fn worth_asking(&self) -> bool {
        self.pick_objects
            || self.animation.is_some()
            || self
                .materials
                .iter()
                .any(|maps| maps.files.iter().any(Option::is_some))
    }

    /// The objects inside object `index`, and it: the ones after it that
    /// are deeper.
    pub fn subtree(&self, index: usize) -> Range<usize> {
        let depth = self.objects[index].depth;
        let end = self.objects[index + 1..]
            .iter()
            .position(|object| object.depth <= depth)
            .map_or(self.objects.len(), |after| index + 1 + after);
        index..end
    }

    /// The object `index` is directly inside, if any.
    pub fn parent(&self, index: usize) -> Option<usize> {
        let depth = self.objects[index].depth;
        (0..index).rev().find(|&i| self.objects[i].depth < depth)
    }

    /// Ticks or unticks object `index` and everything inside it.
    pub fn set(&mut self, index: usize, on: bool) {
        for i in self.subtree(index) {
            self.ticked[i] = on;
        }
    }

    /// Whether object `index`'s box shows ticked, and whether it's only
    /// partly (some of what's inside it is ticked, some isn't).
    pub fn state(&self, index: usize) -> (bool, bool) {
        let meshes: Vec<bool> = self
            .subtree(index)
            .filter(|&i| self.objects[i].has_mesh())
            .map(|i| self.ticked[i])
            .collect();
        let all = meshes.iter().all(|&ticked| ticked);
        let any = meshes.iter().any(|&ticked| ticked);
        (all, any && !all)
    }

    /// The ticked objects with meshes, in file order: each becomes a MeshPart.
    pub fn chosen(&self) -> Vec<usize> {
        (0..self.objects.len())
            .filter(|&i| self.ticked[i] && self.objects[i].has_mesh())
            .collect()
    }

    /// True if every object with a mesh is ticked.
    pub fn all_chosen(&self) -> bool {
        self.objects
            .iter()
            .zip(&self.ticked)
            .all(|(object, &ticked)| ticked || !object.has_mesh())
    }

    /// The rows the filter lets through.
    fn shown(&self) -> Vec<usize> {
        let filter = self.filter.trim().to_lowercase();
        (0..self.objects.len())
            .filter(|&i| filter.is_empty() || self.objects[i].name.to_lowercase().contains(&filter))
            .collect()
    }

    /// Shows the dialog. Returns the answer once there is one.
    pub fn show(&mut self, ctx: &egui::Context) -> Option<Answer> {
        let mut answer = None;
        let file_name = self
            .file
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let modal = egui::Modal::new(egui::Id::new("import model")).show(ctx, |ui| {
            ui.set_width(640.0);
            ui.heading(format!("Import {file_name}"));
            if self.animation_only {
                ui.label("This file has animation but nothing to draw: its takes become clips, for a model with the same skeleton.");
            } else if self.pick_objects {
                self.objects_section(ui);
            } else if self.animation.is_some() {
                self.model_summary(ui);
            }
            if !self.materials.is_empty() {
                ui.separator();
                self.materials_section(ui);
            }
            let animation_only = self.animation_only;
            if let Some(animation) = &mut self.animation {
                ui.separator();
                animation.show(ui, !animation_only);
            }

            ui.add_space(8.0);
            let can_import = (self.animation_only
                || !self.pick_objects
                || !self.chosen().is_empty())
                && self
                    .animation
                    .as_ref()
                    .is_none_or(|animation| animation.problem().is_none());
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(can_import, egui::Button::new("Import"))
                    .clicked()
                {
                    answer = Some(Answer::Import);
                }
                if ui.button("Cancel").clicked() {
                    answer = Some(Answer::Cancel);
                }
            });
        });
        // Escape or a click outside the box means Cancel.
        if answer.is_none() && modal.should_close() {
            answer = Some(Answer::Cancel);
        }
        answer
    }

    /// What the file's model holds, for a file with one object.
    fn model_summary(&self, ui: &mut egui::Ui) {
        let meshes = self.objects.iter().filter(|o| o.has_mesh()).count();
        let triangles: usize = self.objects.iter().map(|o| o.triangles).sum();
        let mut materials: Vec<&str> = self
            .objects
            .iter()
            .flat_map(|o| o.materials.iter().map(String::as_str))
            .collect();
        materials.sort_unstable();
        materials.dedup();
        ui.strong(format!("{} Model", icons::CUBE_TRANSPARENT));
        ui.weak(format!(
            "{meshes} {} · {} triangles · materials: {}",
            if meshes == 1 { "object" } else { "objects" },
            thousands(triangles),
            if materials.is_empty() {
                "none".to_owned()
            } else {
                materials.join(", ")
            }
        ));
    }

    /// The object tree, with its search box, and how the objects arrive.
    fn objects_section(&mut self, ui: &mut egui::Ui) {
        ui.label("Choose the objects to import.");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.filter)
                    .hint_text("Search objects")
                    .desired_width(220.0),
            );
            let shown = self.shown();
            // With a search, these tick only what's shown.
            if ui.button("Select all").clicked() {
                for &i in &shown {
                    self.ticked[i] = true;
                }
            }
            if ui.button("Select none").clicked() {
                for &i in &shown {
                    self.ticked[i] = false;
                }
            }
        });
        ui.add_space(4.0);

        egui::ScrollArea::vertical()
            .id_salt("objects")
            .max_height(260.0)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                let searching = !self.filter.trim().is_empty();
                for i in self.shown() {
                    let object = &self.objects[i];
                    let (mut checked, partly) = self.state(i);
                    let details = if object.has_mesh() {
                        format!(
                            "{} triangles · {}",
                            thousands(object.triangles),
                            object.materials.join(", ")
                        )
                    } else {
                        "group".to_owned()
                    };
                    let name = object.name.clone();
                    // A search shows matches without their place in the tree.
                    let indent = if searching { 0 } else { object.depth };
                    let changed = ui
                        .horizontal(|ui| {
                            ui.add_space(indent as f32 * 18.0);
                            let changed = ui
                                .add(egui::Checkbox::new(&mut checked, name).indeterminate(partly))
                                .changed();
                            ui.weak(details);
                            changed
                        })
                        .inner;
                    if changed {
                        self.set(i, checked);
                    }
                }
            });

        let chosen = self.chosen();
        let triangles: usize = chosen.iter().map(|&i| self.objects[i].triangles).sum();
        let meshes = self.objects.iter().filter(|o| o.has_mesh()).count();
        ui.add_space(4.0);
        ui.weak(format!(
            "{} of {meshes} objects, {} triangles",
            chosen.len(),
            thousands(triangles)
        ));

        ui.separator();
        ui.label("Bring them in as:");
        ui.radio_value(
            &mut self.placement,
            Placement::Grouped,
            "A Model, laid out as in the file",
        );
        ui.radio_value(
            &mut self.placement,
            Placement::Separate,
            "Separate MeshParts, each at the insert point",
        );
        ui.radio_value(
            &mut self.placement,
            Placement::Merged,
            "One MeshPart with all of them",
        );
    }

    /// Each material's maps, as found in the file, to change before importing.
    fn materials_section(&mut self, ui: &mut egui::Ui) {
        ui.label("The texture maps found for each material. Change any before importing:");
        ui.weak("Each material gets a SurfaceAppearance in its MeshParts, to change later in Properties.");
        ui.add_space(4.0);
        egui::ScrollArea::vertical()
            .id_salt("materials")
            .max_height(260.0)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                let open = self.materials.len() <= 3;
                for (index, maps) in self.materials.iter_mut().enumerate() {
                    let found = maps.files.iter().flatten().count();
                    let title = match maps.material.as_str() {
                        "" => "(unnamed material)".to_owned(),
                        name => name.to_owned(),
                    };
                    egui::CollapsingHeader::new(format!("{title}  ·  {found} maps"))
                        .id_salt(("material", index))
                        .default_open(open)
                        .show(ui, |ui| material_maps(ui, index, maps));
                }
            });
    }
}

/// The rows for one material's maps: each map's file with Browse and
/// Clear, its channel for grey maps, and the settings for how they're read.
fn material_maps(ui: &mut egui::Ui, index: usize, maps: &mut MaterialMaps) {
    egui::Grid::new(("maps", index))
        .num_columns(3)
        .spacing([10.0, 4.0])
        .show(ui, |ui| {
            for slot in Slot::ALL {
                ui.label(slot.label());
                ui.horizontal(|ui| {
                    if ui
                        .small_button(icons::FOLDER_OPEN)
                        .on_hover_text("Choose a texture file")
                        .clicked()
                        && let Some(file) = rfd::FileDialog::new()
                            .set_title(format!("Choose a {} map", slot.label().to_lowercase()))
                            .add_filter("Texture", &axcenity_assets::TEXTURE_EXTENSIONS)
                            .pick_file()
                    {
                        maps.set_file(slot, Some(MapFile::Picked(file)));
                    }
                    match maps.file(slot) {
                        Some(file) => {
                            let name = file.file_name();
                            if ui
                                .small_button(icons::X)
                                .on_hover_text("Use no map")
                                .clicked()
                            {
                                maps.set_file(slot, None);
                            }
                            ui.label(name);
                        }
                        None => {
                            ui.weak("None");
                        }
                    }
                });
                match maps.channel_mut(slot) {
                    Some(channel) => {
                        egui::ComboBox::from_id_salt(("channel", index, slot as usize))
                            .width(70.0)
                            .selected_text(channel_name(*channel))
                            .show_ui(ui, |ui| {
                                for item in
                                    [Channel::Red, Channel::Green, Channel::Blue, Channel::Alpha]
                                {
                                    ui.selectable_value(channel, item, channel_name(item));
                                }
                            })
                            .response
                            .on_hover_text("The channel of the texture this map is in");
                    }
                    None => {
                        ui.label("");
                    }
                }
                ui.end_row();
            }
        });
    ui.horizontal_wrapped(|ui| {
        ui.checkbox(&mut maps.normal_directx, "Normal map is DirectX-style");
        ui.checkbox(&mut maps.invert_roughness, "Roughness map is glossiness");
        ui.checkbox(&mut maps.invert_opacity, "Opacity map is inverted");
    });
}

fn channel_name(channel: Channel) -> &'static str {
    match channel {
        Channel::Red => "Red",
        Channel::Green => "Green",
        Channel::Blue => "Blue",
        Channel::Alpha => "Alpha",
    }
}

/// `n` with commas between thousands: 12,345.
fn thousands(n: usize) -> String {
    let digits = n.to_string();
    let mut text = String::new();
    for (i, digit) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            text.push(',');
        }
        text.push(digit);
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Street (a group) holding Bench and Lamp, then Dumpster (a mesh)
    /// holding Lid.
    fn dialog() -> ImportDialog {
        let object = |name: &str, depth, triangles| ModelObject {
            path: name.into(),
            name: name.into(),
            depth,
            triangles,
            materials: Vec::new(),
        };
        ImportDialog::new(
            PathBuf::from("props.fbx"),
            vec![
                object("Street", 0, 0),
                object("Bench", 1, 10),
                object("Lamp", 1, 20),
                object("Dumpster", 0, 30),
                object("Lid", 1, 5),
            ],
            Vec::new(),
        )
    }

    #[test]
    fn everything_starts_ticked_and_groups_are_not_imported_themselves() {
        let dialog = dialog();
        assert_eq!(dialog.chosen(), [1, 2, 3, 4]);
        assert!(dialog.all_chosen());
        assert_eq!(dialog.subtree(0), 0..3);
        assert_eq!(dialog.subtree(3), 3..5);
        assert_eq!(dialog.subtree(4), 4..5);
        assert_eq!(dialog.parent(2), Some(0));
        assert_eq!(dialog.parent(3), None);
    }

    #[test]
    fn ticking_a_group_ticks_what_is_inside_it() {
        let mut dialog = dialog();
        dialog.set(0, false);
        assert_eq!(dialog.chosen(), [3, 4]);
        assert_eq!(dialog.state(0), (false, false));
        dialog.set(2, true);
        // Partly ticked: the Lamp is, the Bench isn't.
        assert_eq!(dialog.state(0), (false, true));
        assert!(!dialog.all_chosen());
        // Unticking the Lid leaves the Dumpster itself ticked.
        dialog.set(4, false);
        assert_eq!(dialog.chosen(), [2, 3]);
        assert_eq!(dialog.state(3), (false, true));
    }

    #[test]
    fn big_numbers_get_commas() {
        assert_eq!(thousands(7), "7");
        assert_eq!(thousands(1234), "1,234");
        assert_eq!(thousands(1234567), "1,234,567");
    }
}
