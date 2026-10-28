//! Asset packages (`.axpkg`): a zip holding a manifest (`axpkg.json`) and
//! asset files at their project paths, to move assets from one project to
//! another.
//!
//! Exporting brings everything the chosen assets need
//! ([`package_contents`]): a state machine brings its clips, a linked clip
//! the model its take is in, a model in a folder of its own every file in
//! that folder (its textures), and a prefab every asset its Instances name.
//!
//! Importing ([`plan_import`]) leaves files the project already has, byte
//! for byte, alone. A file whose path holds a different file is a clash,
//! settled one way for all of them ([`Clashes`]). Keep both moves the
//! incoming folder (or file) to a free numbered name and rewrites the
//! package's paths to match. Nothing here writes into a project: the editor
//! writes the plan, as one undo step.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{Read, Write};
use std::path::Path;

use axcenity_anim::{CLIP_EXTENSION, ClipFile, STATE_MACHINE_EXTENSION, StateMachineFile};
use axcenity_assets::{
    Cell, ColumnType, DataTable, MODELS_FOLDER, ModelFormat, Pick, data_table_path, is_data_table,
    safe_path,
};
use axcenity_core::World;
use serde::{Deserialize, Serialize};

use crate::validate::{AssetKind, PREFAB_SUFFIX, files_under, refs, string_prop};
use crate::world_file::parse_prefab;

/// A package file's extension.
pub const PACKAGE_EXTENSION: &str = "axpkg";
/// The manifest's name inside the zip.
const MANIFEST_FILE: &str = "axpkg.json";
/// The newest package format this reads, and the one it writes.
const PACKAGE_FORMAT: u32 = 1;
/// Every file in a package is under this folder.
const ASSETS_ROOT: &str = "assets/";

/// What a package says it holds.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageManifest {
    pub format: u32,
    /// What it was exported as, such as `noodle` or `Hero`.
    pub name: String,
    /// The assets that were exported; the rest came along for them.
    pub roots: Vec<String>,
    /// Every file in it, at its project path.
    pub files: Vec<String>,
}

/// A package read into memory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Package {
    pub manifest: PackageManifest,
    /// Each file's bytes, by its project path.
    pub files: BTreeMap<String, Vec<u8>>,
}

#[derive(Debug, thiserror::Error)]
pub enum PackageError {
    #[error("could not read or write `{path}`: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("it isn't an asset package: {0}")]
    Zip(String),
    #[error("it has no manifest (`{MANIFEST_FILE}`)")]
    NoManifest,
    #[error("its manifest is damaged: {0}")]
    PackageManifest(serde_json::Error),
    #[error("it was made by a newer version of Axcenity (package format {0})")]
    TooNew(u32),
    #[error("`{0}` isn't an asset path inside a project")]
    UnsafePath(String),
    #[error("its manifest lists `{0}`, which isn't in it")]
    MissingEntry(String),
    #[error("`{0}` isn't in the project")]
    MissingRoot(String),
}

fn io_error(path: &Path) -> impl FnOnce(std::io::Error) -> PackageError + '_ {
    move |source| PackageError::Io {
        path: path.display().to_string(),
        source,
    }
}

/// What exporting some assets packs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Contents {
    /// Every file, by its project path.
    pub files: BTreeMap<String, Vec<u8>>,
    /// Files the assets name that aren't in the project, left out.
    pub missing: Vec<String>,
    /// Where a script reads a data table by a name worked out as it runs
    /// (`script.server.lua:3`), so which table it needs isn't known: for
    /// the creator to check.
    pub unknown_tables: Vec<String>,
}

/// The data tables the Lua `source` reads with
/// `DataService:GetTable("name")`: the names written out in it, and the
/// lines (from 1) of calls whose name isn't written out.
pub fn script_tables(source: &str) -> (Vec<String>, Vec<usize>) {
    let mut names = Vec::new();
    let mut lines = Vec::new();
    for (index, line) in source.lines().enumerate() {
        // A comment's call doesn't run.
        let code = line.split("--").next().unwrap_or(line);
        for (at, _) in code.match_indices("GetTable") {
            let rest = code[at + "GetTable".len()..].trim_start();
            let Some(rest) = rest.strip_prefix('(') else {
                continue;
            };
            let rest = rest.trim_start();
            let literal = rest.chars().next().filter(|c| matches!(c, '"' | '\''));
            let name = literal.and_then(|quote| {
                let inner = &rest[1..];
                let end = inner.find(quote)?;
                let name = &inner[..end];
                let after = inner[end + 1..].trim_start();
                (after.starts_with(')') && !name.contains('\\')).then_some(name)
            });
            match name {
                Some(name) => names.push(name.to_owned()),
                None => lines.push(index + 1),
            }
        }
    }
    names.sort();
    names.dedup();
    (names, lines)
}

/// The files exporting `roots` (project paths, read from `folder`) packs:
/// them and everything they need. `extra` are files that aren't in the
/// project but go in the package as roots too, such as a prefab made from
/// an object in the Explorer. A root that isn't there is an error; a
/// missing dependency is left out and listed in [`Contents::missing`].
pub fn package_contents(
    folder: &Path,
    roots: &[String],
    extra: &[(String, Vec<u8>)],
) -> Result<Contents, PackageError> {
    package_contents_with_scripts(folder, roots, extra, &[])
}

/// [`package_contents`], also bringing the data tables `scripts` read: the
/// scripts (project path and Lua) of an object exported as a prefab. A
/// table named in the code is brought like any dependency; a call whose
/// name isn't written out is listed in [`Contents::unknown_tables`].
pub fn package_contents_with_scripts(
    folder: &Path,
    roots: &[String],
    extra: &[(String, Vec<u8>)],
    scripts: &[(String, String)],
) -> Result<Contents, PackageError> {
    let mut contents = Contents::default();
    let mut wanted: Vec<String> = Vec::new();
    for (path, source) in scripts {
        let (names, lines) = script_tables(source);
        wanted.extend(names.iter().map(|name| data_table_path(name)));
        contents
            .unknown_tables
            .extend(lines.into_iter().map(|line| format!("{path}:{line}")));
    }
    for (path, bytes) in extra {
        check_path(path)?;
        contents.files.insert(path.clone(), bytes.clone());
        wanted.extend(needs(folder, path, bytes));
    }
    for root in roots {
        check_path(root)?;
        let bytes = read_project_file(folder, root)
            .ok_or_else(|| PackageError::MissingRoot(root.clone()))?;
        wanted.extend(needs(folder, root, &bytes));
        contents.files.insert(root.clone(), bytes);
    }
    while let Some(path) = wanted.pop() {
        if contents.files.contains_key(&path) || contents.missing.contains(&path) {
            continue;
        }
        let Some(bytes) = (check_path(&path).is_ok())
            .then(|| read_project_file(folder, &path))
            .flatten()
        else {
            contents.missing.push(path);
            continue;
        };
        wanted.extend(needs(folder, &path, &bytes));
        contents.files.insert(path, bytes);
    }
    contents.missing.sort();
    Ok(contents)
}

/// The project files `path` (holding `bytes`) needs to work.
fn needs(folder: &Path, path: &str, bytes: &[u8]) -> Vec<String> {
    let lower = path.to_ascii_lowercase();
    let text = || std::str::from_utf8(bytes).ok();
    let mut needed = Vec::new();
    if lower.ends_with(STATE_MACHINE_EXTENSION) {
        if let Some(machine) = text().and_then(|text| StateMachineFile::from_json(text).ok()) {
            for state in &machine.states {
                needed.extend(state.motion.clips().into_iter().cloned());
            }
        }
    } else if lower.ends_with(CLIP_EXTENSION) {
        if let Some(link) = text()
            .and_then(|text| ClipFile::from_json(text).ok())
            .and_then(|clip| clip.link)
        {
            needed.push(Pick::of_mesh_id(&link.source).0.to_owned());
        }
    } else if lower.ends_with(PREFAB_SUFFIX) {
        let mut world = World::new();
        let workspace = world.workspace();
        if let Some(text) = text()
            && parse_prefab(text, &mut world, workspace).is_ok()
        {
            needed.extend(world_needs(folder, &world));
        }
    } else if is_data_table(path) {
        // The tables its references name, and the assets in its cells.
        if let Some(table) = text().and_then(|text| DataTable::from_json(text).ok()) {
            for column in &table.columns {
                match &column.kind {
                    ColumnType::Reference { table } => needed.push(data_table_path(table)),
                    ColumnType::Asset { .. } => {
                        needed.extend(table.rows.iter().filter_map(|row| match row.cell(column) {
                            Cell::Text(path) if !path.is_empty() => Some(path),
                            _ => None,
                        }))
                    }
                    _ => {}
                }
            }
        }
    } else if ModelFormat::of(Path::new(path)).is_some()
        && let Some(own) = own_model_folder(path)
    {
        // A model in a folder of its own: its buffers and textures.
        needed.extend(files_under(folder, &own));
    }
    needed
}

/// The asset files `world`'s Instances name.
pub(crate) fn world_needs(folder: &Path, world: &World) -> Vec<String> {
    let mut needed = Vec::new();
    for id in world.descendants(world.root()) {
        let Some(instance) = world.get(id) else {
            continue;
        };
        // A variant's base and the prefabs nested in it (Phase 49).
        if let Some(link) = instance.prefab() {
            needed.push(link.path.clone());
        }
        for (property, kind) in refs(instance.class()) {
            let Some(path) = string_prop(instance.property(property)).filter(|p| !p.is_empty())
            else {
                continue;
            };
            match kind {
                AssetKind::Model => needed.push(Pick::of_mesh_id(path).0.to_owned()),
                AssetKind::Folder => needed.extend(files_under(folder, path)),
                _ => needed.push(path.to_owned()),
            }
        }
    }
    needed
}

/// `assets/models/<name>` for a model inside a folder of its own there.
fn own_model_folder(path: &str) -> Option<String> {
    let rest = path.strip_prefix(MODELS_FOLDER)?.strip_prefix('/')?;
    let (name, _) = rest.split_once('/')?;
    Some(format!("{MODELS_FOLDER}/{name}"))
}

/// A path a package may hold: inside `assets`, with no way out of it.
fn check_path(path: &str) -> Result<(), PackageError> {
    let unsafe_path = || PackageError::UnsafePath(path.to_owned());
    let normal = safe_path(path).ok_or_else(unsafe_path)?;
    let normal = normal.to_string_lossy().replace('\\', "/");
    if normal != path || !path.starts_with(ASSETS_ROOT) {
        return Err(unsafe_path());
    }
    Ok(())
}

fn read_project_file(folder: &Path, path: &str) -> Option<Vec<u8>> {
    fs::read(folder.join(safe_path(path)?)).ok()
}

/// Writes a package of `files` to `path`, named `name`, saying `roots`
/// were what was exported.
pub fn write_package(
    path: &Path,
    name: &str,
    roots: &[String],
    files: &BTreeMap<String, Vec<u8>>,
) -> Result<(), PackageError> {
    let manifest = PackageManifest {
        format: PACKAGE_FORMAT,
        name: name.to_owned(),
        roots: roots.to_vec(),
        files: files.keys().cloned().collect(),
    };
    let zip_error = |err: zip::result::ZipError| PackageError::Zip(err.to_string());
    let file = fs::File::create(path).map_err(io_error(path))?;
    let mut writer = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    writer
        .start_file(MANIFEST_FILE, options)
        .map_err(zip_error)?;
    let text = serde_json::to_string_pretty(&manifest).map_err(PackageError::PackageManifest)?;
    writer.write_all(text.as_bytes()).map_err(io_error(path))?;
    for (name, bytes) in files {
        writer
            .start_file(name.as_str(), options)
            .map_err(zip_error)?;
        writer.write_all(bytes).map_err(io_error(path))?;
    }
    writer.finish().map_err(zip_error)?;
    Ok(())
}

/// Reads the package at `path`. Every file its manifest lists must be in
/// it, at an asset path inside a project.
pub fn read_package(path: &Path) -> Result<Package, PackageError> {
    let zip_error = |err: zip::result::ZipError| PackageError::Zip(err.to_string());
    let file = fs::File::open(path).map_err(io_error(path))?;
    let mut archive = zip::ZipArchive::new(file).map_err(zip_error)?;
    let manifest: PackageManifest = {
        let mut entry = archive
            .by_name(MANIFEST_FILE)
            .map_err(|_| PackageError::NoManifest)?;
        let mut text = String::new();
        entry.read_to_string(&mut text).map_err(io_error(path))?;
        serde_json::from_str(&text).map_err(PackageError::PackageManifest)?
    };
    if manifest.format > PACKAGE_FORMAT {
        return Err(PackageError::TooNew(manifest.format));
    }
    let mut files = BTreeMap::new();
    for name in &manifest.files {
        check_path(name)?;
        let mut entry = archive
            .by_name(name)
            .map_err(|_| PackageError::MissingEntry(name.clone()))?;
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).map_err(io_error(path))?;
        files.insert(name.clone(), bytes);
    }
    Ok(Package { manifest, files })
}

/// What to do with package files whose path in the project already holds
/// a different file.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Clashes {
    /// Put the incoming folder (or file) beside the project's, numbered,
    /// with the package's paths rewritten to match.
    #[default]
    KeepBoth,
    /// Write the incoming files over the project's.
    Replace,
    /// Leave the project's files, and bring in only new ones.
    SkipExisting,
}

/// What importing a package into a project does.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImportPlan {
    /// The files to write, at the paths they'll have, with their bytes.
    pub writes: Vec<(String, Vec<u8>)>,
    /// Package files (by the package's paths) whose path in the project
    /// holds a different file.
    pub clashes: Vec<String>,
    /// Package files the project already has, byte for byte: left alone.
    pub identical: Vec<String>,
    /// For Keep both, each folder (or file) moved to a numbered name.
    pub renamed: Vec<(String, String)>,
}

/// What importing `package` into the project in `folder` would do, with
/// clashes settled as `clashes` says. Reads the project; writes nothing.
pub fn plan_import(folder: &Path, package: &Package, clashes: Clashes) -> ImportPlan {
    let existing = |path: &str| read_project_file(folder, path);
    let mut plan = ImportPlan::default();
    for (path, bytes) in &package.files {
        match existing(path) {
            Some(theirs) if theirs == *bytes => plan.identical.push(path.clone()),
            Some(_) => plan.clashes.push(path.clone()),
            None => {}
        }
    }
    match clashes {
        Clashes::Replace => {
            plan.writes = package
                .files
                .iter()
                .filter(|(path, _)| !plan.identical.contains(path))
                .map(|(path, bytes)| (path.clone(), bytes.clone()))
                .collect();
        }
        Clashes::SkipExisting => {
            plan.writes = package
                .files
                .iter()
                .filter(|(path, _)| existing(path).is_none())
                .map(|(path, bytes)| (path.clone(), bytes.clone()))
                .collect();
        }
        Clashes::KeepBoth => keep_both(folder, package, &mut plan),
    }
    plan
}

/// Keep both: every group (a folder under an asset folder, or a file right
/// in one) with a clash moves to a free numbered name, and the package's
/// text files are rewritten to name the new paths. A rewritten file can
/// clash in turn, so this goes on until nothing new clashes.
fn keep_both(folder: &Path, package: &Package, plan: &mut ImportPlan) {
    let mut files: BTreeMap<String, Vec<u8>> = package.files.clone();
    let mut renamed: BTreeMap<String, String> = BTreeMap::new();
    loop {
        let clashing: BTreeSet<String> = files
            .iter()
            .filter(|(path, bytes)| {
                !renamed.contains_key(&group(path))
                    && read_project_file(folder, path).is_some_and(|theirs| theirs != **bytes)
            })
            .map(|(path, _)| group(path))
            .collect();
        if clashing.is_empty() {
            break;
        }
        for old in clashing {
            let taken = |candidate: &str| {
                folder.join(candidate).exists()
                    || renamed.values().any(|new| new == candidate)
                    || files.keys().any(|path| group(path) == candidate)
            };
            let new = numbered(&old)
                .find(|candidate| !taken(candidate))
                .expect("numbered names never run out");
            files = files
                .into_iter()
                .map(|(path, bytes)| {
                    let path = match path.strip_prefix(&old) {
                        Some(rest) if rest.is_empty() || rest.starts_with('/') => {
                            format!("{new}{rest}")
                        }
                        _ => path,
                    };
                    let bytes = rewrite(&path, bytes, &old, &new);
                    (path, bytes)
                })
                .collect();
            renamed.insert(old, new);
        }
    }
    plan.writes = files
        .into_iter()
        .filter(|(path, bytes)| {
            read_project_file(folder, path).is_none_or(|theirs| theirs != *bytes)
        })
        .collect();
    plan.renamed = renamed.into_iter().collect();
}

/// What moves together under Keep both: `assets/<kind>/<name>` for a file
/// inside a folder there, or the file itself.
fn group(path: &str) -> String {
    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() > 3 {
        parts[..3].join("/")
    } else {
        path.to_owned()
    }
}

/// `Hero2`, `Hero3`… for a folder; `Walk2.anim.json`… for a file, numbered
/// before its first dot.
fn numbered(path: &str) -> impl Iterator<Item = String> + '_ {
    let (parent, name) = path.rsplit_once('/').unwrap_or(("", path));
    let (stem, rest) = name
        .split_once('.')
        .map_or((name, String::new()), |(stem, rest)| {
            (stem, format!(".{rest}"))
        });
    (2..).map(move |n| format!("{parent}/{stem}{n}{rest}"))
}

/// A JSON file's text with paths into `old` (a folder or a file) pointing
/// into `new`. Other files are left as they are: a model's own references
/// are relative, so they move with it.
fn rewrite(path: &str, bytes: Vec<u8>, old: &str, new: &str) -> Vec<u8> {
    if !path.to_ascii_lowercase().ends_with(".json") {
        return bytes;
    }
    let text = match String::from_utf8(bytes) {
        Ok(text) => text,
        // Not text after all: nothing in it to rewrite.
        Err(err) => return err.into_bytes(),
    };
    // Paths are JSON strings: the quote before them keeps `Hero` from
    // matching inside `MyHero`, and what follows the old path (a `/` for a
    // folder; the closing quote or a `#` pick for a file) keeps it from
    // matching `Hero2`.
    let mut text = text.replace(&format!("\"{old}/"), &format!("\"{new}/"));
    for after in ['"', '#'] {
        text = text.replace(&format!("\"{old}{after}"), &format!("\"{new}{after}"));
    }
    text.into_bytes()
}

#[cfg(test)]
mod tests;
