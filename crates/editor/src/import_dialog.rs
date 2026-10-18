//! The Import Model dialog, shown when a model file holds more than one
//! object: tick which objects to import, and choose how they arrive.
//!
//! The ticking rules are plain methods, so they're easy to test; the app
//! turns the answer into Instances.

use std::ops::Range;
use std::path::PathBuf;

use axcenity_assets::ModelObject;
use eframe::egui;

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
}

impl ImportDialog {
    /// The dialog for `file`'s `objects`, with all of them ticked.
    pub fn new(file: PathBuf, objects: Vec<ModelObject>) -> Self {
        Self {
            file,
            ticked: vec![true; objects.len()],
            objects,
            placement: Placement::default(),
            filter: String::new(),
        }
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
            ui.set_width(520.0);
            ui.heading(format!("Import {file_name}"));
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
                .max_height(320.0)
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
                                    .add(
                                        egui::Checkbox::new(&mut checked, name)
                                            .indeterminate(partly),
                                    )
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

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(!chosen.is_empty(), egui::Button::new("Import"))
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
