//! What the Import dialog says importing will make: the Instances,
//! as the Explorer will show them, and the files written into the
//! project. Worked out from the dialog's choices as they are, so the list
//! beside the tabs, and the summary above the buttons, change as they do.

use std::collections::HashSet;

use axcenity_core::ClassName;
use eframe::egui;

use super::{ImportDialog, Placement, object_name, plural};
use crate::surface::MapFile;
use crate::theme::{self, icons};
use crate::widgets;
/// At most this many rows in "What you'll get" (the last says how many more).
pub(super) const MADE_ROWS: usize = 12;

/// One Instance importing makes, as the Explorer will show it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Made {
    pub depth: usize,
    /// `None` for the row that says how many more there are.
    pub class: Option<ClassName>,
    pub name: String,
    /// A few words after the name, such as "2 materials".
    pub note: String,
}

/// What Import creates, counted, and where its files go: the summary
/// above the buttons says it in words ([`Creates::sentence`]).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Creates {
    /// The Model named after the file and one for each group in it.
    pub models: usize,
    pub mesh_parts: usize,
    pub animators: usize,
    pub clips: usize,
    pub state_machines: usize,
    /// The folder the clips and state machine go in, such as
    /// `assets/animations/noodle/`.
    pub animations_folder: Option<String>,
    /// Where the file's copy goes, and what's in it: the first of
    /// [`ImportDialog::files_made`].
    pub copy: (String, String),
}

impl Creates {
    /// "Creates: 1 Model with 3 MeshParts · 4 clips in
    /// `assets/animations/noodle/` · 1 state machine · an Animator. Copies
    /// the model to `assets/models/noodle.fbx`." Paths are between backticks.
    pub fn sentence(&self) -> String {
        let mut parts = Vec::new();
        let groups = self.models.saturating_sub(1);
        match self.models {
            0 if self.mesh_parts > 0 => parts.push(plural(self.mesh_parts, "MeshPart")),
            0 => {}
            _ if groups == 0 => parts.push(format!(
                "1 Model with {}",
                plural(self.mesh_parts, "MeshPart")
            )),
            _ => parts.push(format!(
                "1 Model with {} and {} for its groups",
                plural(self.mesh_parts, "MeshPart"),
                plural(groups, "Model")
            )),
        }
        if self.clips > 0 {
            let clips = plural(self.clips, "clip");
            parts.push(match &self.animations_folder {
                Some(folder) => format!("{clips} in `{folder}`"),
                None => clips,
            });
        }
        if self.state_machines > 0 {
            parts.push(plural(self.state_machines, "state machine"));
        }
        match self.animators {
            0 => {}
            1 => parts.push("an Animator".to_owned()),
            n => parts.push(format!("{n} Animators")),
        }
        let (path, what) = &self.copy;
        let copies = format!("Copies {what} to `{path}`.");
        if parts.is_empty() {
            copies
        } else {
            format!("Creates: {}. {copies}", parts.join(" · "))
        }
    }
}

impl ImportDialog {
    /// What importing creates with the choices as they are.
    pub fn creates(&self) -> Creates {
        let (models, mesh_parts) = if self.animation_only {
            (0, 0)
        } else {
            match self.placement {
                Placement::Merged => (0, 1),
                Placement::Separate => (0, self.chosen().len()),
                // A group gets a Model when a ticked object is inside it.
                Placement::Grouped => {
                    let groups = (0..self.objects.len())
                        .filter(|&i| {
                            !self.objects[i].has_mesh()
                                && self
                                    .subtree(i)
                                    .any(|j| self.ticked[j] && self.objects[j].has_mesh())
                        })
                        .count();
                    (1 + groups, self.chosen().len())
                }
            }
        };
        let clips = self
            .animation
            .as_ref()
            .map_or(0, |animation| animation.clips.len());
        let state_machine = self
            .animation
            .as_ref()
            .is_some_and(|animation| animation.add_animator && !self.animation_only);
        Creates {
            models,
            mesh_parts,
            // Every new MeshPart gets one, as "What you'll get" shows.
            animators: if self.animator_clips().is_some() {
                mesh_parts
            } else {
                0
            },
            clips,
            state_machines: usize::from(state_machine && clips > 0),
            animations_folder: (clips > 0).then(|| self.animations_folder()),
            copy: self.files_made().swap_remove(0),
        }
    }

    /// The folder the clips and state machine are written to.
    fn animations_folder(&self) -> String {
        format!(
            "{}/{}/",
            axcenity_anim::ANIMATIONS_FOLDER,
            axcenity_anim::files::file_safe(&file_stem(&self.file))
        )
    }

    /// The Instances importing makes, as the Explorer will show them:
    /// at most [`MADE_ROWS`] rows, the last saying how many more if there
    /// are. Empty for an animation-only file.
    pub fn made(&self) -> Vec<Made> {
        if self.animation_only {
            return Vec::new();
        }
        let row = |depth, class, name: &str, note: String| Made {
            depth,
            class: Some(class),
            name: name.to_owned(),
            note,
        };
        let animator = self.animator_clips();
        let part_note = |i: usize| {
            let materials = self.objects[i].materials.len();
            let mut note = match materials {
                0 => String::new(),
                1 => "1 material".to_owned(),
                n => format!("{n} materials"),
            };
            if animator.is_some() {
                if !note.is_empty() {
                    note.push_str(" · ");
                }
                note.push_str("Animator");
            }
            note
        };
        let name = self.model_name();
        let mut rows = Vec::new();
        match self.placement {
            Placement::Merged => {
                rows.push(row(0, ClassName::MeshPart, &name, String::new()));
                for maps in &self.materials {
                    let material = match maps.material.as_str() {
                        "" => ClassName::SurfaceAppearance.as_str(),
                        material => material,
                    };
                    let count = maps.files.iter().flatten().count();
                    let note = match count {
                        0 => "colors only".to_owned(),
                        1 => "1 map".to_owned(),
                        n => format!("{n} maps"),
                    };
                    rows.push(row(1, ClassName::SurfaceAppearance, material, note));
                }
                if let Some(clips) = animator {
                    rows.push(row(
                        1,
                        ClassName::Animator,
                        "Animator",
                        plural(clips, "Animation"),
                    ));
                }
            }
            Placement::Separate => {
                for i in self.chosen() {
                    rows.push(row(
                        0,
                        ClassName::MeshPart,
                        &object_name(&self.objects[i].name),
                        part_note(i),
                    ));
                }
            }
            Placement::Grouped => {
                rows.push(row(0, ClassName::Model, &name, String::new()));
                // Each object's depth counts only the groups it's in: an
                // object inside one with a mesh goes next to it.
                let groups = |i: usize| {
                    let mut count = 0;
                    let mut parent = self.parent(i);
                    while let Some(p) = parent {
                        if !self.objects[p].has_mesh() {
                            count += 1;
                        }
                        parent = self.parent(p);
                    }
                    count
                };
                for i in 0..self.objects.len() {
                    let object = &self.objects[i];
                    if object.has_mesh() {
                        if self.ticked[i] {
                            rows.push(row(
                                1 + groups(i),
                                ClassName::MeshPart,
                                &object_name(&object.name),
                                part_note(i),
                            ));
                        }
                    } else if self
                        .subtree(i)
                        .any(|j| self.ticked[j] && self.objects[j].has_mesh())
                    {
                        rows.push(row(
                            1 + groups(i),
                            ClassName::Model,
                            &object_name(&object.name),
                            String::new(),
                        ));
                    }
                }
            }
        }
        if rows.len() > MADE_ROWS {
            let more = rows.len() - (MADE_ROWS - 1);
            rows.truncate(MADE_ROWS - 1);
            rows.push(Made {
                depth: 0,
                class: None,
                name: format!("… and {more} more"),
                note: String::new(),
            });
        }
        rows
    }

    /// The files importing writes into the project, each with what's in
    /// it, such as `("assets/models/Walking/", "the model and 25 textures")`.
    pub fn files_made(&self) -> Vec<(String, String)> {
        let stem = file_stem(&self.file);
        let extension = self
            .file
            .extension()
            .map(|extension| extension.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        let mut files = Vec::new();
        if self.animation_only {
            files.push((
                format!("{}/{stem}.{extension}", axcenity_assets::MODELS_FOLDER),
                "the animation file".to_owned(),
            ));
        } else {
            let mut textures = HashSet::new();
            let mut picked = HashSet::new();
            for file in self
                .materials
                .iter()
                .flat_map(|maps| maps.files.iter().flatten())
            {
                match file {
                    MapFile::Model(path) => textures.insert(path.as_str()),
                    MapFile::Picked(path) => picked.insert(path.as_path()),
                };
            }
            let mut what = "the model".to_owned();
            if !textures.is_empty() {
                what.push_str(&format!(" and {}", plural(textures.len(), "texture")));
            }
            if !picked.is_empty() {
                what.push_str(&format!(
                    ", plus {} you picked",
                    plural(picked.len(), "texture")
                ));
            }
            let folder = extension == "gltf" || !textures.is_empty() || !picked.is_empty();
            let path = if folder {
                format!("{}/{stem}/", axcenity_assets::MODELS_FOLDER)
            } else {
                format!("{}/{stem}.{extension}", axcenity_assets::MODELS_FOLDER)
            };
            files.push((path, what));
        }
        if let Some(animation) = &self.animation
            && !animation.clips.is_empty()
        {
            let mut what = plural(animation.clips.len(), "clip");
            if animation.add_animator && !self.animation_only {
                what.push_str(" and a state machine");
            }
            files.push((self.animations_folder(), what));
        }
        files
    }

    /// What importing makes: the Instances, as the Explorer will show
    /// them, and the files.
    pub(super) fn outcome(&self, ui: &mut egui::Ui) {
        let palette = theme::palette(ui.ctx());
        widgets::section_label(ui, "What you'll get");
        ui.add_space(2.0);
        let made = self.made();
        if made.is_empty() {
            ui.label(
                egui::RichText::new("Nothing in the world: only clips, for another model.")
                    .size(theme::type_scale::SMALL)
                    .color(palette.text_weak),
            );
        }
        for row in made {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 5.0;
                ui.add_space(row.depth as f32 * 14.0);
                match row.class {
                    Some(class) => {
                        ui.label(
                            egui::RichText::new(theme::class_icon(class))
                                .color(theme::class_color(class, ui.visuals())),
                        )
                        .on_hover_text(class.as_str());
                        ui.label(egui::RichText::new(&row.name).color(palette.text));
                        if !row.note.is_empty() {
                            ui.label(
                                egui::RichText::new(&row.note)
                                    .size(theme::type_scale::SMALL)
                                    .color(palette.text_weak),
                            );
                        }
                    }
                    None => {
                        ui.label(egui::RichText::new(&row.name).color(palette.text_weak));
                    }
                }
            });
        }
        if !self.animation_only {
            let at = match self.placement {
                Placement::Separate => "Each at the camera's focus point.",
                _ => "At the camera's focus point, inside the selected Model or Folder.",
            };
            ui.label(
                egui::RichText::new(at)
                    .size(theme::type_scale::SMALL)
                    .color(palette.text_weak),
            );
        }
        ui.add_space(8.0);
        widgets::section_label(ui, "Files in the project");
        ui.add_space(2.0);
        for (path, what) in self.files_made() {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 5.0;
                let icon = if path.ends_with('/') {
                    icons::FOLDER_SIMPLE
                } else {
                    icons::FILE
                };
                ui.label(egui::RichText::new(icon).color(palette.text_secondary));
                ui.label(
                    egui::RichText::new(path)
                        .monospace()
                        .size(theme::type_scale::SMALL),
                );
                ui.label(
                    egui::RichText::new(what)
                        .size(theme::type_scale::SMALL)
                        .color(palette.text_weak),
                );
            });
        }
        ui.label(
            egui::RichText::new(
                "A number is added to a name that's taken. In the Assets panel, they're one folder.",
            )
            .size(theme::type_scale::SMALL)
            .color(palette.text_muted),
        );
    }
}

/// The name the file's copy gets in `assets/models` (before a number is
/// added if it's taken): letters, digits, spaces, `-` and `_` kept, the
/// rest `_`, as `axcenity_assets::import_model` does.
fn file_stem(file: &std::path::Path) -> String {
    let stem = file
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    let name: String = stem
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    match name.trim() {
        "" => "Model".to_owned(),
        name => name.to_owned(),
    }
}
