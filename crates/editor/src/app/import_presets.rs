//! Picking a texture's import preset in the Assets panel (roadmap R12,
//! `axcenity_assets::ImportPresets`). The pick is written to the file's
//! sidecar as one undo step; writing it (or undoing it) cooks the texture
//! again as the preset wants (`write_package_files`).

use std::sync::Arc;

use super::EditorApp;
use crate::history::BlobChange;

/// Whether `path` is a texture file (PNG, JPEG, TGA, BMP), the files import
/// presets apply to.
pub(crate) fn is_texture_file(path: &str) -> bool {
    path.rsplit_once('.').is_some_and(|(_, extension)| {
        axcenity_assets::TEXTURE_EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str())
    })
}

impl EditorApp {
    /// Gives the texture file at `path` the import preset `preset` (`None`:
    /// its folder's), as one undo step, and cooks it again. Not during Play.
    pub(super) fn set_import_preset(&mut self, path: &str, preset: Option<&str>) {
        if self.playing() {
            self.output.error("Press Stop to change import presets.");
            return;
        }
        let Some(folder) = self.project.as_ref().map(|project| project.folder.clone()) else {
            return;
        };
        // A file without a sidecar yet gets one (its ID) first, so undo
        // takes away only the pick.
        if let Err(err) = axcenity_assets::give_asset_id(&folder, path) {
            self.output
                .error(format!("Could not give `{path}` an asset ID: {err}."));
            return;
        }
        let sidecar = axcenity_assets::import_path(path);
        let Ok(before) = std::fs::read(folder.join(&sidecar)) else {
            return;
        };
        let Some(after) = axcenity_assets::sidecar_with_preset(&folder, path, preset) else {
            return;
        };
        if before == after {
            return;
        }
        let name = path.rsplit('/').next().unwrap_or(path);
        let presets = axcenity_assets::ImportPresets::read_or_default(&folder);
        let shown = preset.map_or_else(
            || {
                presets
                    .folder_preset(path)
                    .unwrap_or_else(|| axcenity_assets::DEFAULT_PRESET.to_owned())
            },
            str::to_owned,
        );
        let label = format!("Import `{name}` as {shown}");
        let blobs = vec![BlobChange {
            path: sidecar,
            before: Some(Arc::new(before)),
            after: Some(Arc::new(after)),
        }];
        let written = self.write_package_files(&blobs);
        if written == 0 {
            return;
        }
        self.output.info(format!("{label}."));
        self.history
            .record_blobs_and_properties(&mut self.world, label, blobs, Vec::new());
    }
}
