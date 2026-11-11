//! Asset packages in the editor: Export Package… (for assets picked in the
//! Assets panel, or an object in the Explorer, which goes in as a prefab)
//! and importing a `.axpkg` through the Import dialog's Package tab. What
//! a package holds and how its clashes are settled is `project::package`;
//! this writes the files and makes the import one undo step.

use std::path::PathBuf;
use std::sync::Arc;

use super::*;
use crate::asset_browser::{PREFAB_SUFFIX, PREFABS_FOLDER, prefab_file_stem};
use crate::history::BlobChange;
use crate::import_dialog::PackageImport;

impl EditorApp {
    /// Export Package… for the asset files at `roots` (project paths):
    /// asks where to save it, then writes it.
    pub(in crate::app) fn export_package_with_dialog(&mut self, name: &str, roots: Vec<String>) {
        if let Some(destination) = self.ask_package_destination(name) {
            self.export_package(&destination, name, roots, Vec::new());
        }
    }

    /// Export Package… for the object `id` in the Explorer: it goes in the
    /// package as a prefab, with every asset it uses and the data tables
    /// its scripts read.
    pub(in crate::app) fn export_object_package_with_dialog(&mut self, id: InstanceId) {
        let Some((name, prefab)) = self.object_prefab(id) else {
            return;
        };
        if let Some(destination) = self.ask_package_destination(&name) {
            let scripts = self.scripts_in(id);
            self.export_package_with_scripts(
                &destination,
                &name,
                Vec::new(),
                vec![prefab],
                &scripts,
            );
        }
    }

    /// The scripts inside `id` (the object itself too): each one's file and
    /// Lua (a block script's translation).
    pub(in crate::app) fn scripts_in(&self, id: InstanceId) -> Vec<(String, String)> {
        let Some(project) = &self.project else {
            return Vec::new();
        };
        project
            .scripts
            .iter()
            .filter(|script| {
                script.instance == id || self.world.is_descendant_of(script.instance, id)
            })
            .map(|script| (script.path.clone(), script.source.clone()))
            .collect()
    }

    /// `id` as a prefab file for a package, at the path it would have in
    /// `assets/prefabs`, and the object's name. `None` (said in the
    /// Console) for something that can't be a prefab.
    pub(in crate::app) fn object_prefab(
        &mut self,
        id: InstanceId,
    ) -> Option<(String, (String, Vec<u8>))> {
        if !tree::can_be_prefab(&self.world, id) {
            self.output.error(
                "Only parts, Models and Folders in the Workspace can be exported as a package.",
            );
            return None;
        }
        let name = self.world.get(id)?.name().to_owned();
        let path = format!(
            "{PREFABS_FOLDER}/{}{PREFAB_SUFFIX}",
            prefab_file_stem(&name)
        );
        let text = project::write_prefab(&self.world, &[id]);
        Some((name, (path, text.into_bytes())))
    }

    /// Asks where to save a package named `name`. `None` if cancelled.
    fn ask_package_destination(&self, name: &str) -> Option<PathBuf> {
        let project = self.project.as_ref()?;
        rfd::FileDialog::new()
            .set_title("Export Package")
            .add_filter("Asset package", &[project::PACKAGE_EXTENSION])
            .set_directory(&project.folder)
            .set_file_name(format!("{name}.{}", project::PACKAGE_EXTENSION))
            .save_file()
    }

    /// Writes a package to `destination` holding `roots` (files in the
    /// project) and `extra` (files made for it, such as a prefab), with
    /// everything they need. Says in the Console what went in, and what
    /// they need that isn't in the project. True if it was written.
    pub(in crate::app) fn export_package(
        &mut self,
        destination: &Path,
        name: &str,
        roots: Vec<String>,
        extra: Vec<(String, Vec<u8>)>,
    ) -> bool {
        self.export_package_with_scripts(destination, name, roots, extra, &[])
    }

    /// [`export_package`](Self::export_package), with the data tables
    /// `scripts` read.
    pub(in crate::app) fn export_package_with_scripts(
        &mut self,
        destination: &Path,
        name: &str,
        roots: Vec<String>,
        extra: Vec<(String, Vec<u8>)>,
        scripts: &[(String, String)],
    ) -> bool {
        let Some(folder) = self.project.as_ref().map(|p| p.folder.clone()) else {
            return false;
        };
        let contents =
            match project::package_contents_with_scripts(&folder, &roots, &extra, scripts) {
                Ok(contents) => contents,
                Err(err) => {
                    self.output
                        .error(format!("Could not export the package: {}", chain(&err)));
                    return false;
                }
            };
        for missing in &contents.missing {
            self.output.error(format!(
                "The package needs `{missing}`, which isn't in the project, so it was left out."
            ));
        }
        for place in &contents.unknown_tables {
            self.output.warning(format!(
                "`{place}` reads a data table whose name is worked out as the game runs, so \
                 the package may not have it: check, and export that table too if it's needed."
            ));
        }
        let all_roots: Vec<String> = roots
            .into_iter()
            .chain(extra.into_iter().map(|(path, _)| path))
            .collect();
        match project::write_package(destination, name, &all_roots, &contents.files) {
            Ok(()) => {
                let count = contents.files.len();
                self.output.info(format!(
                    "Exported the package `{}` with {count} {}.",
                    destination.display(),
                    if count == 1 { "file" } else { "files" }
                ));
                self.toasts.success(format!("Exported {name}"));
                true
            }
            Err(err) => {
                self.output
                    .error(format!("Could not export the package: {}", chain(&err)));
                false
            }
        }
    }

    /// Opens the Import dialog for the package at `file`, or says in the
    /// Console why it can't be read. Waits its turn behind another import.
    pub(in crate::app) fn import_package(&mut self, file: &Path) {
        let Some(folder) = self.project.as_ref().map(|p| p.folder.clone()) else {
            return;
        };
        if self.playing() {
            return;
        }
        if self.importing.is_some() {
            self.import_queue.push_back(file.to_owned());
            return;
        }
        match project::read_package(file) {
            Ok(package) => {
                let package = PackageImport::new(&folder, package);
                self.importing = Some(ImportDialog::package(file.to_owned(), package));
            }
            Err(err) => self.output.error(format!(
                "Could not import `{}`: {}",
                file.display(),
                chain(&err)
            )),
        }
    }

    /// Writes the files the dialog's package plan says, as one undo step.
    pub(in crate::app) fn import_package_chosen(&mut self, dialog: &ImportDialog) {
        let Some(folder) = self.project.as_ref().map(|p| p.folder.clone()) else {
            return;
        };
        let Some(package) = &dialog.package else {
            return;
        };
        let mut blobs = Vec::new();
        for (path, bytes) in &package.plan.writes {
            let before = fs::read(folder.join(path)).ok().map(Arc::new);
            blobs.push(BlobChange {
                path: path.clone(),
                before,
                after: Some(Arc::new(bytes.clone())),
            });
        }
        let written = self.write_package_files(&blobs);
        let name = dialog
            .file
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.output.info(format!(
            "Imported `{name}`: {} {}.",
            written,
            if written == 1 { "file" } else { "files" }
        ));
        for (old, new) in &package.plan.renamed {
            self.output
                .info(format!("Kept both: the package's `{old}` is `{new}`."));
        }
        let blobs = blobs.into_iter().take(written).collect();
        let label = format!("Import {}", package.package.manifest.name);
        self.history.record_blobs(&mut self.world, label, blobs);
    }

    /// Puts each file of `blobs` at its `after` (removing it for `None`),
    /// for importing a package, deleting assets, and their undo and redo. Stops at the first
    /// that can't be written, saying so in the Console. Returns how many
    /// were written.
    pub(in crate::app) fn write_package_files(&mut self, blobs: &[BlobChange]) -> usize {
        let Some(folder) = self.project.as_ref().map(|p| p.folder.clone()) else {
            return 0;
        };
        if blobs.is_empty() {
            return 0;
        }
        let mut written = 0;
        for blob in blobs {
            let Some(relative) = axcenity_assets::safe_path(&blob.path) else {
                continue;
            };
            let disk = folder.join(&relative);
            let result = match &blob.after {
                Some(bytes) => disk
                    .parent()
                    .map_or(Ok(()), fs::create_dir_all)
                    .and_then(|()| fs::write(&disk, bytes.as_slice())),
                None => match fs::remove_file(&disk) {
                    Err(err) if err.kind() == ErrorKind::NotFound => Ok(()),
                    other => other.map(|()| remove_empty_folders(&folder, &relative)),
                },
            };
            if let Err(err) = result {
                self.output
                    .error(format!("Could not write `{}`: {err}", blob.path));
                break;
            }
            written += 1;
        }
        // A sidecar moved with its file (Rename, its undo, a package): IDs
        // name the new places.
        if blobs[..written]
            .iter()
            .any(|blob| axcenity_assets::is_import_file(&blob.path))
        {
            self.assets.forget_asset_ids();
        }
        self.asset_browser.refresh();
        self.animation.refresh();
        // A table a fix (or its undo) rewrote: scripts and its tab read it again.
        for blob in blobs.iter().take(written) {
            if axcenity_assets::is_data_table(&blob.path) {
                self.assets.forget(&blob.path);
                self.data_tables.file_changed(&folder, &blob.path);
            }
            // A model a Model Studio save (or its undo) rewrote: MeshParts
            // using it draw that version now.
            if axcenity_assets::ModelFormat::of(std::path::Path::new(&blob.path)).is_some() {
                let _ = self.assets.reload(&blob.path);
            }
        }
        // A script Replace All (or its undo) rewrote: its tab and blocks take it.
        for blob in blobs.iter().take(written) {
            if crate::app::find_in_scripts::is_script_file(&blob.path)
                && let Some(bytes) = &blob.after
            {
                let text = String::from_utf8_lossy(bytes).into_owned();
                self.take_script_text(&blob.path, &text);
            }
        }
        self.hand_over_tables();
        written
    }
}

/// Removes the folders around the removed file `relative` (in `project`)
/// that are empty now, up to but not including the top two levels
/// (`assets/models`), so undoing an import leaves no empty folders behind.
fn remove_empty_folders(project: &Path, relative: &Path) {
    let mut folder = relative.parent();
    while let Some(at) = folder.filter(|at| at.components().count() > 2) {
        // Fails, and stops, on a folder that still has something in it.
        if fs::remove_dir(project.join(at)).is_err() {
            break;
        }
        folder = at.parent();
    }
}
