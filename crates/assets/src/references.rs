//! Asset references by ID (roadmap L2): anywhere a project file's path is
//! taken (a MeshPart's `MeshId`, a map, a `SoundId`, a sky's faces), its
//! asset ID (`ax://…`, see [`AssetId`]) is taken too, with a model's
//! `#pick` after it as after a path. An ID keeps naming the file when it's
//! renamed or moved, with its sidecar, inside or outside the editor.
//!
//! [`Assets`] turns an ID into the file's path before anything else, so an
//! ID and the path it names share one load. In the editor, the IDs come
//! from the sidecars ([`asset_ids`]); a built game has no sidecars, so
//! Build Game writes them all into [`ASSET_INDEX_FILE`] instead. Paths are
//! still taken everywhere: older projects and scripts don't change.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::ops::Deref;
use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::{AssetId, Assets, asset_ids, read_asset_id};

/// Where a built game keeps each asset ID's path, at the top of the game's
/// folder (see [`encode_asset_index`]).
pub const ASSET_INDEX_FILE: &str = "asset-index.json";

const INDEX_VERSION: u32 = 1;

/// [`ASSET_INDEX_FILE`]'s contents.
#[derive(Serialize, Deserialize)]
struct IndexFile {
    format_version: u32,
    /// Each ID's project path.
    assets: BTreeMap<AssetId, String>,
}

/// Whether `text` is an asset ID reference (`ax://…`, maybe with a
/// `#pick`) rather than a path.
pub fn is_asset_reference(text: &str) -> bool {
    text.starts_with("ax://")
}

/// The ID in a reference and what follows it (a model's `#pick`, with its
/// `#`, or nothing), or `None` if it isn't one.
pub fn split_reference(text: &str) -> Option<(AssetId, &str)> {
    if !is_asset_reference(text) {
        return None;
    }
    let end = text.find('#').unwrap_or(text.len());
    Some((text[..end].parse().ok()?, &text[end..]))
}

/// [`ASSET_INDEX_FILE`] for `ids` (each file's ID, as [`asset_ids`] gives
/// them).
pub fn encode_asset_index(ids: &BTreeMap<String, AssetId>) -> Vec<u8> {
    let file = IndexFile {
        format_version: INDEX_VERSION,
        assets: ids.iter().map(|(path, &id)| (id, path.clone())).collect(),
    };
    let mut bytes = serde_json::to_vec_pretty(&file).expect("plain data");
    bytes.push(b'\n');
    bytes
}

/// Each asset ID in the project at `folder`, with its file's path: from
/// [`ASSET_INDEX_FILE`] if it has one (a built game), else from the
/// sidecars.
pub fn read_asset_index(folder: &Path) -> HashMap<AssetId, String> {
    if let Ok(bytes) = fs::read(folder.join(ASSET_INDEX_FILE))
        && let Ok(file) = serde_json::from_slice::<IndexFile>(&bytes)
    {
        return file.assets.into_iter().collect();
    }
    asset_ids(folder)
        .into_iter()
        .map(|(path, id)| (id, path))
        .collect()
}

/// What [`Assets::resolve`] gives: the path it was handed, or the path an
/// ID names. Reads as a `&str`.
pub enum AssetPath<'a> {
    /// Handed a path (or an ID no file has): used as it is.
    Given(&'a str),
    /// The path an ID names, with its `#pick`.
    Named(Arc<str>),
}

impl Deref for AssetPath<'_> {
    type Target = str;

    fn deref(&self) -> &str {
        match self {
            AssetPath::Given(path) => path,
            AssetPath::Named(path) => path,
        }
    }
}

/// The IDs [`Assets`] knows, kept until [`Assets::forget_asset_ids`].
#[derive(Default)]
pub(crate) struct IdCache {
    /// Each ID's path, read the first time an ID is asked for.
    index: Option<Arc<HashMap<AssetId, String>>>,
    /// Each reference already turned into a path, so drawing a part every
    /// frame doesn't make the string again.
    resolved: HashMap<Box<str>, Arc<str>>,
}

impl Assets {
    /// The path `reference` names: itself if it's a path, or the path of
    /// the file with its ID (and its `#pick`). An ID no file has is handed
    /// back as it is, so reading it fails as a missing file does.
    pub fn resolve<'a>(&self, reference: &'a str) -> AssetPath<'a> {
        if !is_asset_reference(reference) {
            return AssetPath::Given(reference);
        }
        if let Some(path) = self.lock().ids.resolved.get(reference) {
            return AssetPath::Named(path.clone());
        }
        match self.asset_path(reference) {
            Some(path) => {
                let path: Arc<str> = path.into();
                self.lock()
                    .ids
                    .resolved
                    .insert(reference.into(), path.clone());
                AssetPath::Named(path)
            }
            None => AssetPath::Given(reference),
        }
    }

    /// The path of the file `reference` names (an ID, with any `#pick`
    /// after the path), or `None` if no file has that ID. A path is handed
    /// back as it is.
    pub fn asset_path(&self, reference: &str) -> Option<String> {
        let Some((id, pick)) = split_reference(reference) else {
            return Some(reference.to_owned());
        };
        let index = self.id_index()?;
        Some(format!("{}{pick}", index.get(&id)?))
    }

    /// How a reference to the file at `path` is written: its ID if it has
    /// one (its sidecar is there), so moving the file keeps the reference,
    /// else the path. A `#pick` after the path stays after the ID. The ID
    /// resolves from now on, even if its sidecar is newer than the others.
    pub fn asset_reference(&self, path: &str) -> String {
        if is_asset_reference(path) {
            return path.to_owned();
        }
        let (file, pick) = path.split_at(path.find('#').unwrap_or(path.len()));
        let id = self
            .folder()
            .and_then(|folder| read_asset_id(&folder, file));
        let Some(id) = id else {
            return path.to_owned();
        };
        // A sidecar written since the IDs were read (a file just imported)
        // is known from now on, without reading them all again.
        if let Some(index) = &mut self.lock().ids.index
            && index.get(&id).map(String::as_str) != Some(file)
        {
            Arc::make_mut(index).insert(id, file.to_owned());
        }
        format!("{id}{pick}")
    }

    /// Forgets which file each ID names, so the next ID asked for reads
    /// them again: for when sidecars were written, moved or deleted.
    pub fn forget_asset_ids(&self) {
        self.lock().ids = IdCache::default();
    }

    /// Each ID's path, read (outside the lock) the first time it's needed.
    fn id_index(&self) -> Option<Arc<HashMap<AssetId, String>>> {
        let folder = {
            let shared = self.lock();
            if let Some(index) = &shared.ids.index {
                return Some(index.clone());
            }
            shared.folder.clone()?
        };
        let index = Arc::new(read_asset_index(&folder));
        let mut shared = self.lock();
        // Another thread may have read them meanwhile; either will do.
        Some(shared.ids.index.get_or_insert(index).clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{encode_import_file, import_path};

    fn project(name: &str) -> std::path::PathBuf {
        let folder =
            std::env::temp_dir().join(format!("axcenity-refs-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(folder.join("assets/textures")).unwrap();
        folder
    }

    #[test]
    fn a_reference_splits_into_its_id_and_its_pick() {
        let id = AssetId::new();
        let text = format!("{id}#Body/Wheel");
        assert_eq!(split_reference(&text), Some((id, "#Body/Wheel")));
        assert_eq!(split_reference(&id.to_string()), Some((id, "")));
        assert_eq!(split_reference("assets/models/ship.glb"), None);
        assert_eq!(split_reference("ax://not-an-id"), None);
    }

    #[test]
    fn an_id_resolves_to_its_files_path_and_follows_a_move() {
        let folder = project("resolve");
        let id = AssetId::new();
        let old = "assets/textures/wood.png";
        fs::write(folder.join(old), b"png").unwrap();
        fs::write(folder.join(import_path(old)), encode_import_file(id)).unwrap();
        let assets = Assets::new(&folder);

        assert_eq!(assets.asset_reference(old), id.to_string());
        assert_eq!(
            assets.asset_reference(&format!("{old}#A")),
            format!("{id}#A")
        );
        assert_eq!(&*assets.resolve(&id.to_string()), old);
        assert_eq!(&*assets.resolve(&format!("{id}#A/B")), format!("{old}#A/B"));
        assert_eq!(&*assets.resolve(old), old, "a path is used as it is");
        let unknown = AssetId::new().to_string();
        assert_eq!(&*assets.resolve(&unknown), unknown);
        assert_eq!(assets.asset_path(&unknown), None);
        // A file with no sidecar is referred to by its path.
        assert_eq!(
            assets.asset_reference("assets/textures/none.png"),
            "assets/textures/none.png"
        );

        // A sidecar written after the IDs were read resolves once its
        // reference is asked for.
        let late = "assets/textures/late.png";
        fs::write(folder.join(late), b"png").unwrap();
        let late_id = crate::give_asset_id(&folder, late).unwrap().unwrap();
        assert_eq!(assets.asset_reference(late), late_id.to_string());
        assert_eq!(&*assets.resolve(&late_id.to_string()), late);
        assert_eq!(crate::give_asset_id(&folder, late).unwrap(), Some(late_id));
        assert_eq!(
            crate::give_asset_id(&folder, "assets/textures/gone.png").unwrap(),
            None
        );

        // Moved, sidecar and all: once told, the ID names the new place.
        let new = "assets/textures/oak.png";
        fs::rename(folder.join(old), folder.join(new)).unwrap();
        fs::rename(folder.join(import_path(old)), folder.join(import_path(new))).unwrap();
        assets.forget_asset_ids();
        assert_eq!(&*assets.resolve(&id.to_string()), new);
        let _ = fs::remove_dir_all(folder);
    }

    #[test]
    fn a_built_games_index_names_each_ids_file() {
        let folder = project("index");
        let id = AssetId::new();
        let ids = BTreeMap::from([("assets/textures/wood.png".to_owned(), id)]);
        fs::write(folder.join(ASSET_INDEX_FILE), encode_asset_index(&ids)).unwrap();
        let index = read_asset_index(&folder);
        assert_eq!(
            index.get(&id).map(String::as_str),
            Some("assets/textures/wood.png")
        );
        let _ = fs::remove_dir_all(folder);
    }
}
