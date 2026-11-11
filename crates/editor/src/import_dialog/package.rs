//! The Import dialog's Package tab, for an asset package (`.axpkg`): what's
//! inside it, what importing adds, and, when some of its files are already
//! in the project with other contents, one choice for all of them (see
//! `project::plan_import`).

use std::path::{Path, PathBuf};

use axcenity_project::{self as project, Clashes, ImportPlan, Package};
use eframe::egui;

use super::plural;
use crate::theme::{self, icons};
use crate::widgets;

/// A package being imported, and what importing it would do.
pub struct PackageImport {
    pub package: Package,
    /// The project it goes into.
    folder: PathBuf,
    pub clashes: Clashes,
    /// Its scripts inside objects (G2): code from elsewhere that would run
    /// in this project's game.
    pub scripts: Vec<String>,
    /// Whether to bring `scripts` in. Off until ticked: without them, the
    /// package's prefabs come without those scripts.
    pub keep_scripts: bool,
    /// What importing does with `clashes` and `keep_scripts` as they are,
    /// worked out again when they change.
    pub plan: ImportPlan,
}

impl PackageImport {
    /// `package`, going into the project in `folder`, with clashes kept
    /// both ways to begin with.
    pub fn new(folder: &Path, package: Package) -> Self {
        let mut import = Self {
            scripts: project::package_scripts(&package),
            package,
            folder: folder.to_owned(),
            clashes: Clashes::default(),
            keep_scripts: false,
            plan: ImportPlan::default(),
        };
        import.replan();
        import
    }

    /// Works out what importing does now.
    fn replan(&mut self) {
        self.plan = if self.keep_scripts || self.scripts.is_empty() {
            project::plan_import(&self.folder, &self.package, self.clashes)
        } else {
            let plain = project::without_scripts(&self.package);
            project::plan_import(&self.folder, &plain, self.clashes)
        };
    }

    /// Settles clashes as `clashes` says from now on.
    pub fn set_clashes(&mut self, clashes: Clashes) {
        if clashes != self.clashes {
            self.clashes = clashes;
            self.replan();
        }
    }

    /// Brings the package's scripts in, or leaves them out.
    pub fn set_keep_scripts(&mut self, keep: bool) {
        if keep != self.keep_scripts {
            self.keep_scripts = keep;
            self.replan();
        }
    }

    /// What's in it, in a few words, for under the title.
    pub fn holds(&self) -> String {
        let mut parts = vec![plural(self.package.files.len(), "file")];
        if let [root] = self.package.manifest.roots.as_slice() {
            parts.push(format!("exported from {}", short_name(root)));
        }
        parts.join(" · ")
    }

    /// Why Import is off, if it is.
    pub fn problem(&self) -> Option<String> {
        self.plan.writes.is_empty().then(|| {
            if self.plan.clashes.is_empty() {
                "The project already has everything in this package.".to_owned()
            } else {
                "Nothing new to add: every file is already in the project.".to_owned()
            }
        })
    }

    /// What Import adds, and the clashes and what's done with them, such
    /// as "Adds 6 files: 1 state machine, 5 clips. 1 file already exists:
    /// Walk. Keep both puts the package's in `assets/animations/noodle2/`."
    pub fn sentence(&self) -> String {
        let mut text = if self.plan.writes.is_empty() {
            "Adds nothing.".to_owned()
        } else {
            let paths: Vec<&str> = self.plan.writes.iter().map(|(p, _)| p.as_str()).collect();
            format!(
                "Adds {}: {}.",
                plural(paths.len(), "file"),
                kinds(&paths).join(", ")
            )
        };
        let left = self
            .plan
            .identical
            .iter()
            .filter(|path| self.plan.writes.iter().all(|(written, _)| written != *path))
            .count();
        if left > 0 && self.plan.clashes.is_empty() {
            text.push_str(&format!(
                " {} already in the project, left as {}.",
                plural_be(left, "is", "are"),
                if left == 1 { "it is" } else { "they are" }
            ));
        }
        if !self.plan.clashes.is_empty() {
            let names: Vec<String> = self.plan.clashes.iter().map(|p| short_name(p)).collect();
            text.push_str(&format!(
                " {} already {}: {}.",
                plural(names.len(), "file"),
                if names.len() == 1 { "exists" } else { "exist" },
                names.join(", ")
            ));
            text.push_str(&match self.clashes {
                Clashes::KeepBoth => {
                    let places: Vec<String> = self
                        .plan
                        .renamed
                        .iter()
                        .map(|(_, new)| format!("`{}`", shown_place(new)))
                        .collect();
                    format!(" Keep both puts the package's in {}.", places.join(", "))
                }
                Clashes::Replace => " Replace writes the package's over them.".to_owned(),
                Clashes::SkipExisting => " Skip existing leaves them as they are.".to_owned(),
            });
        }
        text
    }

    /// Where Keep both puts the package's file `path`, if it moves it.
    fn moved(&self, path: &str) -> Option<String> {
        self.plan.renamed.iter().find_map(|(old, new)| {
            let rest = path.strip_prefix(old.as_str())?;
            (rest.is_empty() || rest.starts_with('/')).then(|| format!("{new}{rest}"))
        })
    }

    /// The Package tab: the choice for clashes, then every file in the
    /// package and what happens to it.
    pub fn show(&mut self, ui: &mut egui::Ui) {
        let palette = theme::palette(ui.ctx());
        let quiet = |text: &str| {
            egui::RichText::new(text.to_owned())
                .size(theme::type_scale::SMALL)
                .color(palette.text_weak)
        };
        ui.label(quiet(
            "An asset package holds assets from another project, at the paths they had there, \
             with everything they need. Files the project already has, exactly, are left alone.",
        ));
        if !self.plan.clashes.is_empty() {
            ui.add_space(8.0);
            widgets::section_label(ui, "Files that already exist");
            ui.label(format!(
                "{} the package has {} in the project already, with other contents.",
                plural(self.plan.clashes.len(), "file"),
                if self.plan.clashes.len() == 1 {
                    "is"
                } else {
                    "are"
                }
            ));
            let mut clashes = self.clashes;
            widgets::segmented(
                ui,
                &mut clashes,
                &[
                    (Clashes::KeepBoth, None, "Keep both"),
                    (Clashes::Replace, None, "Replace"),
                    (Clashes::SkipExisting, None, "Skip existing"),
                ],
            );
            self.set_clashes(clashes);
            ui.label(quiet(match self.clashes {
                Clashes::KeepBoth => {
                    "The package's go in a numbered folder beside the project's, and its own \
                     files name them there."
                }
                Clashes::Replace => "The package's files take the place of the project's.",
                Clashes::SkipExisting => {
                    "The project's files stay as they are; only new files are added."
                }
            }));
        }
        if !self.scripts.is_empty() {
            ui.add_space(8.0);
            widgets::section_label(ui, "Scripts");
            ui.label(format!(
                "{} inside the package's objects {} code from another project, which runs in \
                 your game.",
                plural(self.scripts.len(), "script"),
                if self.scripts.len() == 1 { "is" } else { "are" }
            ));
            for path in &self.scripts {
                ui.label(
                    egui::RichText::new(path)
                        .monospace()
                        .size(theme::type_scale::SMALL),
                );
            }
            let mut keep = self.keep_scripts;
            ui.checkbox(&mut keep, "Keep its scripts")
                .on_hover_text("Only if you trust where the package came from.");
            self.set_keep_scripts(keep);
            ui.label(quiet(if self.keep_scripts {
                "The scripts come in with their objects. Read them before you press Play."
            } else {
                "The objects come in without these scripts."
            }));
        }
        ui.add_space(8.0);
        widgets::section_label(ui, "In the package");
        let written: Vec<&str> = self.plan.writes.iter().map(|(p, _)| p.as_str()).collect();
        for path in self.package.files.keys() {
            if !self.keep_scripts && self.scripts.contains(path) {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 5.0;
                    ui.label(egui::RichText::new(icons::MINUS).color(palette.text_secondary));
                    ui.label(
                        egui::RichText::new(path)
                            .monospace()
                            .size(theme::type_scale::SMALL),
                    );
                    ui.label(quiet("left out: a script"));
                });
                continue;
            }
            let moved = self.moved(path).map(|to| format!("as {to}"));
            let (icon, note) = if self.plan.clashes.contains(path) {
                let note = match self.clashes {
                    Clashes::KeepBoth => moved.unwrap_or_default(),
                    Clashes::Replace => "replaces the project's".to_owned(),
                    Clashes::SkipExisting => "skipped: the project's stays".to_owned(),
                };
                (icons::WARNING, note)
            } else if written.contains(&path.as_str()) || moved.is_some() {
                (icons::PLUS, moved.unwrap_or_else(|| "new".to_owned()))
            } else if self.plan.identical.contains(path) {
                (icons::CHECK, "already in the project".to_owned())
            } else {
                // Skip existing leaves out only clashes, so this is rare.
                (icons::MINUS, "not added".to_owned())
            };
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 5.0;
                ui.label(egui::RichText::new(icon).color(palette.text_secondary));
                ui.label(
                    egui::RichText::new(path)
                        .monospace()
                        .size(theme::type_scale::SMALL),
                );
                ui.label(quiet(&note));
            });
        }
    }
}

/// "1 file is", "3 files are".
fn plural_be(n: usize, one: &str, many: &str) -> String {
    format!("{} {}", plural(n, "file"), if n == 1 { one } else { many })
}

/// What kinds of file `paths` are, counted, such as "1 state machine",
/// "5 clips", "1 model".
fn kinds(paths: &[&str]) -> Vec<String> {
    let kinds = [
        ("state machine", ".statemachine.json"),
        ("clip", ".anim.json"),
        ("prefab", ".prefab.json"),
    ];
    let mut counts: Vec<(&str, usize)> = Vec::new();
    for path in paths {
        let lower = path.to_lowercase();
        let kind = kinds
            .iter()
            .find(|(_, suffix)| lower.ends_with(suffix))
            .map(|(kind, _)| *kind)
            .unwrap_or_else(|| {
                let extension = lower.rsplit('.').next().unwrap_or_default();
                if ["gltf", "glb", "fbx"].contains(&extension) {
                    "model"
                } else if axcenity_assets::TEXTURE_EXTENSIONS.contains(&extension) {
                    "texture"
                } else if axcenity_assets::AUDIO_EXTENSIONS.contains(&extension) {
                    "sound"
                } else {
                    "other file"
                }
            });
        match counts.iter_mut().find(|(known, _)| *known == kind) {
            Some((_, n)) => *n += 1,
            None => counts.push((kind, 1)),
        }
    }
    counts
        .into_iter()
        .map(|(kind, n)| plural(n, kind))
        .collect()
}

/// A file's name without `.json` and `.anim`, as clashes list it: `Walk`,
/// `Hero.statemachine`, `wood.png`.
fn short_name(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    let name = name.strip_suffix(".json").unwrap_or(name);
    name.strip_suffix(".anim").unwrap_or(name).to_owned()
}

/// A renamed folder shown with its slash, a renamed file as it is.
fn shown_place(path: &str) -> String {
    if path
        .rsplit('/')
        .next()
        .is_some_and(|name| name.contains('.'))
    {
        path.to_owned()
    } else {
        format!("{path}/")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clashes_are_named_like_the_assets_panel_names_them() {
        assert_eq!(short_name("assets/animations/Hero/Walk.anim.json"), "Walk");
        assert_eq!(
            short_name("assets/animations/Hero/Hero.statemachine.json"),
            "Hero.statemachine"
        );
        assert_eq!(short_name("assets/textures/wood.png"), "wood.png");
        assert_eq!(
            kinds(&[
                "a/x.statemachine.json",
                "a/W.anim.json",
                "a/R.anim.json",
                "m/n.fbx"
            ]),
            ["1 state machine", "2 clips", "1 model"]
        );
    }
}
