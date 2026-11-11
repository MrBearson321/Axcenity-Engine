//! Plugin packages for sharing: a zip of a plugin folder with the
//! `.axcenity-plugin` suffix, plus a scaffold for a new plugin.
//!
//! Creators pack a working folder with [`pack`], send the file, and others
//! install it with [`install`] into a project's `plugins/` or the editor's
//! `Plugins` folder. The Toolbox and Plugins panel both call these.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use zip::ZipArchive;
use zip::write::SimpleFileOptions;

use crate::manifest::{ENTRY_FILE, MANIFEST_FILE, Manifest};

/// The end of a shared plugin file: `align-to-grid.axcenity-plugin`.
pub const SUFFIX: &str = ".axcenity-plugin";

/// A short description of a package, for the install confirmation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preview {
    pub name: String,
    pub version: String,
    pub folder: String,
    pub permissions: Vec<String>,
    pub files: Vec<String>,
}

/// Turns a plugin folder name into a safe directory / file stem.
pub fn slug(name: &str) -> String {
    let slug: String = name
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let slug = slug
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if slug.is_empty() {
        "plugin".to_owned()
    } else {
        slug
    }
}

/// Writes a starter plugin into `parent/<slug>/` and returns that folder.
pub fn scaffold(parent: &Path, name: &str) -> Result<PathBuf, String> {
    let folder_name = slug(name);
    let folder = parent.join(&folder_name);
    if folder.exists() {
        return Err(format!("a plugin called `{folder_name}` is already there"));
    }
    fs::create_dir_all(&folder).map_err(|err| format!("could not create {folder:?}: {err}"))?;
    let display = name.trim();
    let display = if display.is_empty() {
        "New Plugin"
    } else {
        display
    };
    let manifest = format!(
        "{{\n  \"name\": {},\n  \"version\": \"1.0.0\"\n}}\n",
        serde_json::to_string(display).unwrap_or_else(|_| "\"New Plugin\"".to_owned())
    );
    fs::write(folder.join(MANIFEST_FILE), manifest)
        .map_err(|err| format!("could not write {MANIFEST_FILE}: {err}"))?;
    fs::write(folder.join(ENTRY_FILE), STARTER)
        .map_err(|err| format!("could not write {ENTRY_FILE}: {err}"))?;
    Ok(folder)
}

const STARTER: &str = r#"-- A starter editor plugin. Rename the toolbar and wire up what you need.
-- Docs: docs/editor/plugins.md · types: definitions/axcenity-plugin.d.luau

local toolbar = plugin:CreateToolbar("Tools")
local button = toolbar:CreateButton("Hello", "Say hello in the Console", "puzzle-piece")

button.Click:Connect(function()
	print("Hello from your plugin")
	ChangeHistoryService:SetWaypoint("Plugin hello")
end)
"#;

/// Packs `folder` (a plugin with `plugin.json`) into `out` as a
/// `.axcenity-plugin` zip. Paths inside the zip are relative to the folder.
pub fn pack(folder: &Path, out: &Path) -> Result<(), String> {
    let manifest_path = folder.join(MANIFEST_FILE);
    let text = fs::read_to_string(&manifest_path)
        .map_err(|err| format!("could not read {MANIFEST_FILE}: {err}"))?;
    Manifest::parse(&text)?;
    let files = collect_files(folder)?;
    if !files.iter().any(|path| path == ENTRY_FILE) {
        return Err(format!("the plugin has no {ENTRY_FILE}"));
    }
    let file = File::create(out).map_err(|err| format!("could not create {out:?}: {err}"))?;
    let mut writer = zip::ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for relative in &files {
        let bytes = fs::read(folder.join(relative))
            .map_err(|err| format!("could not read {relative}: {err}"))?;
        writer
            .start_file(relative.as_str(), options)
            .map_err(|err| err.to_string())?;
        writer
            .write_all(&bytes)
            .map_err(|err| format!("could not write {relative}: {err}"))?;
    }
    writer.finish().map_err(|err| err.to_string())?;
    Ok(())
}

/// Reads a package's manifest and file list without installing it.
pub fn preview(archive: &Path) -> Result<Preview, String> {
    let (manifest, files, folder, _) = read_package(archive)?;
    Ok(Preview {
        name: manifest.name,
        version: manifest.version,
        folder,
        permissions: manifest
            .permissions
            .iter()
            .map(|permission| match permission {
                crate::Permission::Files => "files".to_owned(),
            })
            .collect(),
        files,
    })
}

/// Unpacks `archive` into `parent/<folder>/`. Refuses to overwrite an
/// existing folder. Returns the installed folder.
pub fn install(archive: &Path, parent: &Path) -> Result<PathBuf, String> {
    let (manifest, files, folder_name, prefix) = read_package(archive)?;
    let _ = manifest;
    let dest = parent.join(&folder_name);
    if dest.exists() {
        return Err(format!(
            "a plugin called `{folder_name}` is already installed"
        ));
    }
    fs::create_dir_all(parent).map_err(|err| format!("could not create {parent:?}: {err}"))?;
    let file = File::open(archive).map_err(|err| format!("could not open {archive:?}: {err}"))?;
    let mut zip = ZipArchive::new(file).map_err(|err| err.to_string())?;
    fs::create_dir_all(&dest).map_err(|err| format!("could not create {dest:?}: {err}"))?;
    for relative in &files {
        let zip_name = match &prefix {
            Some(top) => format!("{top}/{relative}"),
            None => relative.clone(),
        };
        let mut entry = zip
            .by_name(&zip_name)
            .map_err(|_| format!("missing `{zip_name}` in the package"))?;
        let mut bytes = Vec::new();
        entry
            .read_to_end(&mut bytes)
            .map_err(|err| format!("could not read `{relative}`: {err}"))?;
        let path = dest.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|err| format!("could not create {parent:?}: {err}"))?;
        }
        fs::write(&path, bytes).map_err(|err| format!("could not write {path:?}: {err}"))?;
    }
    Ok(dest)
}

fn read_package(archive: &Path) -> Result<(Manifest, Vec<String>, String, Option<String>), String> {
    let file = File::open(archive).map_err(|err| format!("could not open {archive:?}: {err}"))?;
    let mut zip = ZipArchive::new(file).map_err(|err| err.to_string())?;
    let mut files = Vec::new();
    let mut prefix: Option<String> = None;
    for index in 0..zip.len() {
        let entry = zip.by_index(index).map_err(|err| err.to_string())?;
        if entry.is_dir() {
            continue;
        }
        let name = entry
            .enclosed_name()
            .ok_or_else(|| format!("unsafe path in the package: {}", entry.name()))?
            .to_string_lossy()
            .replace('\\', "/");
        if name.contains("..") || Path::new(&name).is_absolute() {
            return Err(format!("unsafe path in the package: {name}"));
        }
        // Nested packages put everything under one top folder; flat ones
        // keep plugin.json at the zip root.
        let (top, rest) = match name.split_once('/') {
            Some((top, rest)) => (Some(top.to_owned()), rest.to_owned()),
            None => (None, name.clone()),
        };
        match (&prefix, top) {
            (None, Some(top)) => prefix = Some(top),
            (Some(have), Some(top)) if have == &top => {}
            (Some(_), Some(_)) => {
                return Err("the package has files in more than one folder".to_owned());
            }
            (Some(_), None) => {
                return Err("the package mixes a folder with root files".to_owned());
            }
            (None, None) => {}
        }
        let relative = if prefix.is_some() { rest } else { name };
        let lower = relative.to_ascii_lowercase();
        if !(lower == MANIFEST_FILE || lower.ends_with(".lua") || lower.ends_with(".css")) {
            return Err(format!(
                "plugins may only ship .lua, .css and {MANIFEST_FILE} (found `{relative}`)"
            ));
        }
        files.push(relative);
    }
    files.sort();
    files.dedup();
    if !files.iter().any(|path| path == MANIFEST_FILE) {
        return Err(format!("the package has no {MANIFEST_FILE}"));
    }
    if !files.iter().any(|path| path == ENTRY_FILE) {
        return Err(format!("the package has no {ENTRY_FILE}"));
    }
    let manifest_name = match &prefix {
        Some(top) => format!("{top}/{MANIFEST_FILE}"),
        None => MANIFEST_FILE.to_owned(),
    };
    let mut entry = zip
        .by_name(&manifest_name)
        .map_err(|_| format!("missing `{manifest_name}` in the package"))?;
    let mut text = String::new();
    entry
        .read_to_string(&mut text)
        .map_err(|err| format!("could not read {MANIFEST_FILE}: {err}"))?;
    let manifest = Manifest::parse(&text)?;
    let folder = prefix.clone().unwrap_or_else(|| slug(&manifest.name));
    Ok((manifest, files, folder, prefix))
}

fn collect_files(folder: &Path) -> Result<Vec<String>, String> {
    let mut files = Vec::new();
    collect(folder, "", &mut files)?;
    files.sort();
    Ok(files)
}

fn collect(directory: &Path, prefix: &str, files: &mut Vec<String>) -> Result<(), String> {
    let entries =
        fs::read_dir(directory).map_err(|err| format!("could not read {directory:?}: {err}"))?;
    for entry in entries {
        let entry = entry.map_err(|err| err.to_string())?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let relative = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}/{name}")
        };
        let path = entry.path();
        let kind = entry.file_type().map_err(|err| err.to_string())?;
        if kind.is_dir() {
            collect(&path, &relative, files)?;
        } else if kind.is_file() {
            let lower = name.to_ascii_lowercase();
            if lower == MANIFEST_FILE || lower.ends_with(".lua") || lower.ends_with(".css") {
                files.push(relative);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Permission;

    fn temp(test: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!(
            "axcenity-plugin-bundle-{test}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(&folder).unwrap();
        folder
    }

    #[test]
    fn a_scaffold_packs_and_installs_round_trip() {
        let root = temp("round-trip");
        let made = scaffold(&root, "Align Tools").unwrap();
        assert!(made.join(MANIFEST_FILE).is_file());
        assert!(made.join(ENTRY_FILE).is_file());
        assert_eq!(made.file_name().unwrap(), "align-tools");

        let archive = root.join("align-tools.axcenity-plugin");
        pack(&made, &archive).unwrap();
        let preview = preview(&archive).unwrap();
        assert_eq!(preview.name, "Align Tools");
        assert_eq!(preview.version, "1.0.0");
        assert!(preview.files.contains(&ENTRY_FILE.to_owned()));

        let into = root.join("installed");
        let dest = install(&archive, &into).unwrap();
        assert!(dest.join(MANIFEST_FILE).is_file());
        assert!(dest.join(ENTRY_FILE).is_file());
        assert_eq!(
            fs::read_to_string(made.join(ENTRY_FILE)).unwrap(),
            fs::read_to_string(dest.join(ENTRY_FILE)).unwrap()
        );
        assert!(install(&archive, &into).is_err());
    }

    #[test]
    fn a_nested_folder_package_installs_under_its_slug() {
        let root = temp("nested");
        let plugin = root.join("part-tools");
        fs::create_dir_all(&plugin).unwrap();
        fs::write(
            plugin.join(MANIFEST_FILE),
            r#"{ "name": "Part Tools", "version": "2.0.0", "permissions": ["files"] }"#,
        )
        .unwrap();
        fs::write(plugin.join(ENTRY_FILE), "print('ok')\n").unwrap();
        let archive = root.join("part-tools.axcenity-plugin");
        pack(&plugin, &archive).unwrap();

        // Re-pack with a top-level folder inside the zip.
        let nested = root.join("nested.axcenity-plugin");
        {
            let file = File::create(&nested).unwrap();
            let mut writer = zip::ZipWriter::new(file);
            let options =
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
            for name in [MANIFEST_FILE, ENTRY_FILE] {
                writer
                    .start_file(format!("part-tools/{name}"), options)
                    .unwrap();
                writer
                    .write_all(&fs::read(plugin.join(name)).unwrap())
                    .unwrap();
            }
            writer.finish().unwrap();
        }
        let preview = preview(&nested).unwrap();
        assert_eq!(preview.folder, "part-tools");
        assert_eq!(preview.permissions, ["files"]);
        assert!(
            Manifest::parse(&fs::read_to_string(plugin.join(MANIFEST_FILE)).unwrap())
                .unwrap()
                .allows(Permission::Files)
        );
        let dest = install(&nested, &root.join("out")).unwrap();
        assert_eq!(dest.file_name().unwrap(), "part-tools");
    }

    #[test]
    fn unsafe_and_foreign_files_are_refused() {
        let root = temp("unsafe");
        let archive = root.join("bad.axcenity-plugin");
        {
            let file = File::create(&archive).unwrap();
            let mut writer = zip::ZipWriter::new(file);
            let options =
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
            writer.start_file(MANIFEST_FILE, options).unwrap();
            writer
                .write_all(br#"{ "name": "Bad", "version": "1.0.0" }"#)
                .unwrap();
            writer.start_file(ENTRY_FILE, options).unwrap();
            writer.write_all(b"print(1)\n").unwrap();
            writer.start_file("secret.exe", options).unwrap();
            writer.write_all(b"nope").unwrap();
            writer.finish().unwrap();
        }
        assert!(preview(&archive).unwrap_err().contains(".lua"));
    }
}
