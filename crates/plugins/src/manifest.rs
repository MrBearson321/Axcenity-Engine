//! `plugin.json` and finding plugin folders.
//!
//! A plugin is a folder with a `plugin.json` and an `init.lua`, in a
//! project's `plugins/` folder or in the editor's own `Plugins` folder
//! (the user's plugins, for every project):
//!
//! ```json
//! { "name": "Align to Grid", "version": "1.0.0", "permissions": ["files"] }
//! ```

use std::path::{Path, PathBuf};

use serde::Deserialize;

/// The folder in a project that holds its plugins. The game never sees it:
/// it isn't packed into built games and its stylesheets don't style the game.
pub const PLUGINS_FOLDER: &str = "plugins";
/// The file in a plugin's folder that describes it.
pub const MANIFEST_FILE: &str = "plugin.json";
/// The file a plugin's code starts in.
pub const ENTRY_FILE: &str = "init.lua";

/// What a plugin may do besides editing the world.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Permission {
    /// `plugin:ReadProjectFile` and `WriteProjectFile`, inside the project.
    Files,
}

/// A plugin's `plugin.json`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub permissions: Vec<Permission>,
}

impl Manifest {
    pub fn parse(text: &str) -> Result<Manifest, String> {
        let manifest: Manifest =
            serde_json::from_str(text).map_err(|err| format!("{MANIFEST_FILE}: {err}"))?;
        if manifest.name.trim().is_empty() {
            return Err(format!("{MANIFEST_FILE}: the name is empty"));
        }
        Ok(manifest)
    }

    pub fn allows(&self, permission: Permission) -> bool {
        self.permissions.contains(&permission)
    }
}

/// Where a plugin was found.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Origin {
    /// The open project's `plugins/` folder.
    Project,
    /// The editor's own `Plugins` folder.
    User,
}

/// A plugin folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
    /// Names the plugin for its settings and whether it's on:
    /// `project/<folder>` or `user/<folder>`.
    pub key: String,
    pub origin: Origin,
    pub folder: PathBuf,
    /// Its `plugin.json`, or why it couldn't be read.
    pub manifest: Result<Manifest, String>,
}

impl Found {
    /// The name to show: the manifest's, or the folder's.
    pub fn name(&self) -> String {
        match &self.manifest {
            Ok(manifest) => manifest.name.clone(),
            Err(_) => self
                .folder
                .file_name()
                .map_or_else(String::new, |name| name.to_string_lossy().into_owned()),
        }
    }
}

/// Every plugin in the project's `plugins/` folder, then every one in
/// `user` (the editor's `Plugins` folder), each sorted by folder name. A
/// folder without a `plugin.json` isn't a plugin.
pub fn find_plugins(project: Option<&Path>, user: Option<&Path>) -> Vec<Found> {
    let mut found = Vec::new();
    if let Some(project) = project {
        find_in(&project.join(PLUGINS_FOLDER), Origin::Project, &mut found);
    }
    if let Some(user) = user {
        find_in(user, Origin::User, &mut found);
    }
    found
}

fn find_in(folder: &Path, origin: Origin, found: &mut Vec<Found>) {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    let mut folders: Vec<PathBuf> = entries
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .map(|entry| entry.path())
        .filter(|path| path.join(MANIFEST_FILE).is_file())
        .collect();
    folders.sort();
    let prefix = match origin {
        Origin::Project => "project",
        Origin::User => "user",
    };
    for folder in folders {
        let name = folder
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
        let manifest = std::fs::read_to_string(folder.join(MANIFEST_FILE))
            .map_err(|err| format!("{MANIFEST_FILE}: {err}"))
            .and_then(|text| Manifest::parse(&text));
        found.push(Found {
            key: format!("{prefix}/{name}"),
            origin,
            folder,
            manifest,
        });
    }
}

/// The files of a plugin folder that reload it when they change: its
/// `plugin.json`, Lua and stylesheets, by their paths inside `folder`.
pub fn plugin_files(folder: &Path) -> Vec<String> {
    let mut files = Vec::new();
    list(folder, "", &mut files);
    files.sort();
    files
}

fn list(directory: &Path, prefix: &str, files: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let path = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}/{name}")
        };
        match entry.file_type() {
            Ok(kind) if kind.is_dir() => list(&entry.path(), &path, files),
            Ok(kind) if kind.is_file() => {
                let lower = name.to_ascii_lowercase();
                if lower == MANIFEST_FILE || lower.ends_with(".lua") || lower.ends_with(".css") {
                    files.push(path);
                }
            }
            _ => {}
        }
    }
}
