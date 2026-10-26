//! The Asset Check panel's scan and its fixes. A fix is worked out by
//! `project::plan_fix` and made here as one undo step: the world's
//! properties it changes and the project files it writes, copies or
//! deletes. Ignoring a finding saves it in `.axcenity/asset-check.json`
//! and isn't an undo step. Fixes are off during Play.

use std::path::PathBuf;
use std::sync::Arc;

use axcenity_core::Value;
use axcenity_project::{self as project, AssetKind, Finding, Fix, Ignored, Site};

use super::{EditorApp, shown};
use crate::asset_check::{CheckAction, FixChoice};
use crate::history::{BlobChange, Field, PropertyEdit};
use crate::layout::Pane;

impl EditorApp {
    /// The Asset Check panel: scan results, their fixes, and a button to run the scan.
    pub(in crate::app) fn asset_check_pane(&mut self, ui: &mut eframe::egui::Ui) {
        let can_fix = !self.playing();
        match self.asset_check.show(ui, self.project.is_some(), can_fix) {
            CheckAction::Run => self.run_asset_check(),
            CheckAction::Select(owner) => self.select_finding_owner(&owner),
            CheckAction::Fix(findings, choice) => self.fix_findings(&findings, choice),
            CheckAction::Ignore(findings) => self.ignore_findings(&findings),
            CheckAction::Unignore(key) => self.unignore_finding(&key),
            CheckAction::None => {}
        }
    }

    /// File > Check Assets, the Test ribbon, and the panel's button.
    pub(in crate::app) fn run_asset_check(&mut self) {
        if self.project.is_none() {
            self.output.error("Open a project first.");
            return;
        }
        self.refresh_asset_check();
        let count = self.asset_check.shown().len();
        if count == 0 {
            self.output.info(
                "No missing files, invalid references, import failures, or broken dependencies.",
            );
            self.toasts.success("No asset problems");
        } else {
            let noun = if count == 1 { "problem" } else { "problems" };
            self.output.info(format!(
                "Asset check found {count} {noun}. See the Asset Check panel."
            ));
            self.toasts.error(format!("{count} {noun}"));
        }
        self.layout.open(Pane::AssetCheck);
    }

    /// Scans again and shows the result, without a message.
    fn refresh_asset_check(&mut self) {
        let Some(project) = &self.project else {
            return;
        };
        let report = project::check_assets(&project.folder, &self.world);
        let files = project::list_project_files(&project.folder);
        let ignored = project::read_ignored(&project.folder);
        self.asset_check.set(report, &files, ignored);
    }

    pub(super) fn select_finding_owner(&mut self, owner: &str) {
        let world = shown(&self.world, &self.play);
        let found = world
            .descendants(world.root())
            .into_iter()
            .find(|&id| world.full_name(id).as_deref() == Some(owner));
        if let Some(id) = found {
            self.select_from_viewport(Some(id));
        }
    }

    /// Fixes `findings` the way `choice` says, as one undo step, then scans again.
    pub(in crate::app) fn fix_findings(&mut self, findings: &[Finding], choice: FixChoice) {
        if self.playing() || findings.is_empty() {
            return;
        }
        let Some(folder) = self.project.as_ref().map(|p| p.folder.clone()) else {
            return;
        };
        let fix = match choice {
            FixChoice::Use(path) => Fix::Use(path),
            FixChoice::CopyIn(file) => Fix::CopyIn(file),
            FixChoice::Clear => Fix::Clear,
            FixChoice::Delete => Fix::DeleteFile,
            FixChoice::Locate => match self.locate(&findings[0]) {
                Some(file) => Fix::CopyIn(file),
                None => return,
            },
        };
        let label = fix_label(&fix, findings.len());
        let targets: Vec<&Finding> = findings.iter().collect();
        let plan = match project::plan_fix(&folder, &targets, &fix) {
            Ok(plan) => plan,
            Err(err) => {
                self.output.error(format!("Could not fix that: {err}."));
                return;
            }
        };

        let mut properties = Vec::new();
        for change in &plan.properties {
            let Some(before) = self
                .world
                .get(change.id)
                .and_then(|instance| instance.property(change.property).cloned())
            else {
                continue;
            };
            let after = Value::String(change.value.clone());
            if let Err(err) = self
                .world
                .set_property(change.id, change.property, after.clone())
            {
                self.output.error(format!("Could not fix that: {err:#}"));
                continue;
            }
            properties.push(PropertyEdit {
                id: change.id,
                field: Field::Property(change.property),
                before,
                after,
            });
        }
        let blobs: Vec<BlobChange> = plan
            .files
            .iter()
            .map(|write| BlobChange {
                path: write.path.clone(),
                before: write.before.clone().map(Arc::new),
                after: write.after.clone().map(Arc::new),
            })
            .collect();
        let written = self.write_package_files(&blobs);
        let blobs: Vec<BlobChange> = blobs.into_iter().take(written).collect();

        // What changed is read again the next time it's drawn.
        for blob in &blobs {
            self.assets.forget(&blob.path);
        }
        for finding in findings {
            if let Site::ModelTexture { model, .. } = &finding.site {
                self.assets.forget(model);
            }
        }
        self.assets.forget_failures();

        if !blobs.is_empty() || !properties.is_empty() {
            self.output.info(format!("{label}."));
        }
        self.history
            .record_blobs_and_properties(&mut self.world, label, blobs, properties);
        self.refresh_asset_check();
    }

    /// Asks for the file (or folder) `finding` should name.
    fn locate(&self, finding: &Finding) -> Option<PathBuf> {
        let kind = finding.file_kind()?;
        let mut dialog = rfd::FileDialog::new().set_title("Locate the file");
        if let Some(project) = &self.project {
            dialog = dialog.set_directory(&project.folder);
        }
        let extensions: &[&str] = match kind {
            AssetKind::Folder => return dialog.set_title("Locate the folder").pick_folder(),
            AssetKind::Model => &["gltf", "glb", "fbx"],
            AssetKind::Texture => &axcenity_assets::TEXTURE_EXTENSIONS,
            AssetKind::Audio => &axcenity_assets::AUDIO_EXTENSIONS,
            AssetKind::Clip | AssetKind::StateMachine => &["json"],
        };
        dialog.add_filter(kind.expected(), extensions).pick_file()
    }

    /// Hides `findings` from the panel, in this project, from now on.
    pub(in crate::app) fn ignore_findings(&mut self, findings: &[Finding]) {
        let Some(folder) = self.project.as_ref().map(|p| p.folder.clone()) else {
            return;
        };
        let mut ignored = project::read_ignored(&folder);
        ignored.extend(findings.iter().map(Finding::ignore_key));
        if let Err(err) = project::write_ignored(&folder, &ignored) {
            self.output
                .error(format!("Could not save the ignored problems: {err}"));
            return;
        }
        self.refresh_asset_check();
    }

    pub(in crate::app) fn unignore_finding(&mut self, key: &Ignored) {
        let Some(folder) = self.project.as_ref().map(|p| p.folder.clone()) else {
            return;
        };
        let mut ignored = project::read_ignored(&folder);
        ignored.retain(|kept| kept != key);
        if let Err(err) = project::write_ignored(&folder, &ignored) {
            self.output
                .error(format!("Could not save the ignored problems: {err}"));
            return;
        }
        self.refresh_asset_check();
    }
}

/// The undo step's name, such as "Fix 3 asset problems: use `chair.glb`".
fn fix_label(fix: &Fix, count: usize) -> String {
    let what = match fix {
        Fix::Use(path) => format!("use `{}`", file_name(path)),
        Fix::CopyIn(file) => format!(
            "copy in `{}`",
            file.file_name().map_or_else(
                || file.display().to_string(),
                |name| name.to_string_lossy().into_owned()
            )
        ),
        Fix::Clear => "clear".to_owned(),
        Fix::DeleteFile => "delete the file".to_owned(),
    };
    if count == 1 {
        format!("Fix asset problem: {what}")
    } else {
        format!("Fix {count} asset problems: {what}")
    }
}

fn file_name(path: &str) -> &str {
    let file = path.split('#').next().unwrap_or(path);
    file.rsplit('/').next().unwrap_or(file)
}
