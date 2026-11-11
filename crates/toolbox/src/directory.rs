//! The central Axcenity asset directory: one per user, shared by the
//! editor, the CLI and the Blender integration, holding every downloaded
//! original and the metadata database.
//!
//! ```text
//! <asset directory>/
//!   metadata.db                                  the metadata database (`db`)
//!   textures/<provider>/<id>/<variant>/          files as downloaded
//!   hdri/<provider>/<id>/<variant>/
//!   models/<provider>/<id>/<variant>/
//!   audio/<provider>/<id>/<variant>/
//!   cache/<provider>/                            archives while they unpack
//! ```
//!
//! The folders follow the Blender pipeline's `asset_registry.py` layout.
//! Downloaded files are never changed after they're in place.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use crate::record::Kind;
use crate::{SourceError, guard};

/// Overrides where the directory is.
pub const ENV: &str = "AXCENITY_ASSET_DIR";

/// The database's file name inside the directory.
pub const DATABASE: &str = "metadata.db";

/// Where the directory is: [`ENV`] when set, else the platform's local
/// data folder (`%LOCALAPPDATA%\Axcenity\Assets` on Windows,
/// `$XDG_DATA_HOME/axcenity/assets` or `~/.local/share/axcenity/assets` on
/// Linux, `~/Library/Application Support/Axcenity/Assets` on macOS).
/// `None` when the environment names none of them.
pub fn locate() -> Option<PathBuf> {
    locate_with(|name| std::env::var_os(name))
}

/// [`locate`] reading the environment through `var`, for tests.
pub fn locate_with(var: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    let set = |name: &str| {
        var(name)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    };
    if let Some(dir) = set(ENV) {
        return Some(dir);
    }
    if cfg!(windows) {
        set("LOCALAPPDATA").map(|dir| dir.join("Axcenity").join("Assets"))
    } else if cfg!(target_os = "macos") {
        set("HOME").map(|home| {
            home.join("Library")
                .join("Application Support")
                .join("Axcenity")
                .join("Assets")
        })
    } else {
        set("XDG_DATA_HOME")
            .filter(|dir| dir.is_absolute())
            .map(|dir| dir.join("axcenity").join("assets"))
            .or_else(|| {
                set("HOME").map(|home| {
                    home.join(".local")
                        .join("share")
                        .join("axcenity")
                        .join("assets")
                })
            })
    }
}

/// The folder each kind's downloads go in.
pub fn kind_folder(kind: Kind) -> &'static str {
    match kind {
        Kind::Texture => "textures",
        Kind::Hdri => "hdri",
        Kind::Model => "models",
        Kind::Sound => "audio",
    }
}

/// An open asset directory.
#[derive(Clone, Debug)]
pub struct AssetDirectory {
    root: PathBuf,
}

impl AssetDirectory {
    /// The directory at `root`, created if it isn't there.
    pub fn open(root: &Path) -> Result<Self, SourceError> {
        fs::create_dir_all(root)?;
        Ok(Self {
            root: root.to_owned(),
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The metadata database's path.
    pub fn database_path(&self) -> PathBuf {
        self.root.join(DATABASE)
    }

    /// Where one variant of one asset goes, relative to the directory with
    /// `/` separators: `textures/ambientcg/Bricks090/2K-JPG`. Each part
    /// must be one plain folder name.
    pub fn variant_folder(
        &self,
        provider: &str,
        kind: Kind,
        id: &str,
        variant: &str,
    ) -> Result<String, SourceError> {
        let variant = folder_name(variant);
        for part in [provider, id, variant.as_str()] {
            if part.contains('/') || guard::safe_relative_path(part).is_none() {
                return Err(SourceError::Refused("the asset's name can't be a folder"));
            }
        }
        Ok(format!("{}/{provider}/{id}/{variant}", kind_folder(kind)))
    }

    /// A path inside the directory from a relative one (`/` separators).
    pub fn path(&self, relative: &str) -> PathBuf {
        relative
            .split('/')
            .fold(self.root.clone(), |path, part| path.join(part))
    }

    /// Where a source's archives wait while they unpack.
    pub fn cache_folder(&self, provider: &str) -> PathBuf {
        self.root.join("cache").join(provider)
    }
}

/// A variant's label as a folder name: `2K JPG` is `2K-JPG`.
pub fn folder_name(label: &str) -> String {
    let name: String = label
        .trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let name = name.trim_matches('-').to_owned();
    if name.is_empty() {
        "default".to_owned()
    } else {
        name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_environment_override_wins() {
        let found = locate_with(|name| (name == ENV).then(|| OsString::from("D:/Assets")));
        assert_eq!(found, Some(PathBuf::from("D:/Assets")));
    }

    #[test]
    fn the_platform_default_is_local_data() {
        let found = locate_with(|name| match name {
            "LOCALAPPDATA" => Some("C:/Users/a/AppData/Local".into()),
            "HOME" => Some("/home/a".into()),
            _ => None,
        })
        .unwrap();
        let expected = if cfg!(windows) {
            PathBuf::from("C:/Users/a/AppData/Local")
                .join("Axcenity")
                .join("Assets")
        } else if cfg!(target_os = "macos") {
            PathBuf::from("/home/a/Library/Application Support/Axcenity/Assets")
        } else {
            PathBuf::from("/home/a/.local/share/axcenity/assets")
        };
        assert_eq!(found, expected);
        assert_eq!(locate_with(|_| None), None);
    }

    #[test]
    fn variant_folders_follow_the_layout_and_refuse_odd_names() {
        let dir = AssetDirectory {
            root: PathBuf::from("root"),
        };
        assert_eq!(
            dir.variant_folder("ambientcg", Kind::Texture, "Bricks090", "2K JPG")
                .unwrap(),
            "textures/ambientcg/Bricks090/2K-JPG"
        );
        assert_eq!(
            dir.variant_folder("openverse", Kind::Sound, "a-b", "MP3")
                .unwrap(),
            "audio/openverse/a-b/MP3"
        );
        assert!(
            dir.variant_folder("ambientcg", Kind::Texture, "../x", "2K")
                .is_err()
        );
        assert!(dir.variant_folder("a/b", Kind::Texture, "x", "2K").is_err());
        assert_eq!(
            dir.path("textures/a/b"),
            PathBuf::from("root").join("textures").join("a").join("b")
        );
    }

    #[test]
    fn labels_become_plain_folder_names() {
        assert_eq!(folder_name("2K JPG"), "2K-JPG");
        assert_eq!(folder_name("4K glTF"), "4K-glTF");
        assert_eq!(folder_name("  "), "default");
    }
}
