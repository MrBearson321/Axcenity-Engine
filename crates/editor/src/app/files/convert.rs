//! Model formats the engine doesn't read itself (USD), converted to a
//! `.glb` before the Import dialog opens (ADR-0027, `axcenity-convert`).
//! From there importing is as for any `.glb`; the dialog's title, Output
//! and the copy's sidecar (`"converted_from"`) name the file picked.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;

/// Each conversion gets a folder of its own, so two files with the same
/// name don't meet.
static CONVERSIONS: AtomicUsize = AtomicUsize::new(0);

/// A new folder for one converted file, in the system's temporary folder.
fn conversion_folder() -> std::io::Result<PathBuf> {
    let folder = std::env::temp_dir().join(format!(
        "axcenity-converted-{}/{}",
        std::process::id(),
        CONVERSIONS.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&folder);
    fs::create_dir_all(&folder)?;
    Ok(folder)
}

impl EditorApp {
    /// Converts `file` (a `.usd`, `.usda`, `.usdc` or `.usdz`) to a `.glb`
    /// of the same name and imports that through the Import dialog. What
    /// the conversion left out or couldn't find is reported in Output; a
    /// file that can't be converted is reported there and not imported.
    pub(in crate::app) fn import_converted(&mut self, file: &Path) {
        let name = file
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let converted = match axcenity_convert::convert(file) {
            Ok(converted) => converted,
            Err(err) => {
                self.output.error(format!(
                    "Could not import `{}`: {}",
                    file.display(),
                    chain(&err)
                ));
                self.toasts
                    .error(format!("Could not import {name}. Output says why."));
                return;
            }
        };
        for warning in &converted.warnings {
            self.output
                .error(format!("Converting `{name}`: {warning}."));
        }
        let stem = file
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Model".to_owned());
        let glb = conversion_folder().and_then(|folder| {
            let glb = folder.join(format!("{stem}.glb"));
            fs::write(&glb, &converted.glb)?;
            Ok(glb)
        });
        match glb {
            Ok(glb) => {
                self.converted.insert(glb.clone(), file.to_owned());
                self.import_model(&glb);
            }
            Err(err) => self.output.error(format!(
                "Could not import `{}`: its converted copy couldn't be written: {err}",
                file.display()
            )),
        }
    }

    /// Notes in the sidecar of `copy` (the project path of an imported
    /// `.glb`) the file it was converted from, if it was. It's written
    /// before the import's undo step is recorded, so it's part of it.
    pub(super) fn note_converted(&mut self, dialog: &ImportDialog, copy: &str) {
        let (Some(source), Some(project)) = (&dialog.source, &self.project) else {
            return;
        };
        let name = source
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        if let Err(err) = axcenity_assets::note_converted_from(&project.folder, copy, &name) {
            self.output.error(format!(
                "Could not note in `{copy}`'s sidecar that it came from `{name}`: {err}"
            ));
        }
    }
}
