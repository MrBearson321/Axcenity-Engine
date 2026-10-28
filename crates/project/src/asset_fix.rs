//! Fixes for what [`check_assets`](crate::check_assets) found. Working a
//! fix out reads the project but writes nothing: [`plan_fix`] returns the
//! property changes and file writes, and the editor makes them as one undo
//! step. The ignored list is the one thing here that writes a file, in the
//! project's `.axcenity` folder.

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

use axcenity_anim::{ANIMATIONS_FOLDER, ClipFile, StateMachineFile};
use axcenity_assets::{
    Cell, DataTable, MATERIALS_FOLDER, MODELS_FOLDER, PROJECT_AUDIO_FOLDER,
    PROJECT_TEXTURES_FOLDER, Pick, TEXTURES_FOLDER, safe_file_name, safe_path, unique_file_name,
};
use axcenity_core::{InstanceId, Value, World};
use serde::{Deserialize, Serialize};

use crate::validate::{AssetKind, Finding, FindingKind, PREFABS_FOLDER, Site, project_files};
use crate::world_file::{parse_prefab, parse_world, write_prefab, write_world};

const IMAGES_FOLDER: &str = "assets/images";
const PARTICLES_FOLDER: &str = "assets/particles";

/// The ignored findings of a project, by its folder.
pub const IGNORED_FILE: &str = ".axcenity/asset-check.json";

/// What to do about one or more findings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fix {
    /// Point the reference at this project path (Use, Replace…, or a file
    /// Locate… found inside the project). For a model's missing texture,
    /// the file is copied next to the model instead.
    Use(String),
    /// Copy this file into the project and point the reference at the copy
    /// (Locate…). A file that's already inside the project is used where it is.
    CopyIn(PathBuf),
    /// Empty the reference, or unlink a linked clip.
    Clear,
    /// Delete the file that can't be loaded.
    DeleteFile,
}

/// A property to set on an Instance of the open world.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PropertyFix {
    pub id: InstanceId,
    pub property: &'static str,
    pub value: String,
}

/// A project file's bytes before and after a fix. `None` means no file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileWrite {
    pub path: String,
    pub before: Option<Vec<u8>>,
    pub after: Option<Vec<u8>>,
}

/// Everything a fix changes, for the editor to make as one undo step.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FixPlan {
    pub properties: Vec<PropertyFix>,
    pub files: Vec<FileWrite>,
}

/// One ignored finding, as kept in [`IGNORED_FILE`].
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct Ignored {
    pub kind: String,
    pub owner: String,
    pub path: String,
    pub message: String,
}

#[derive(Default, Serialize, Deserialize)]
struct IgnoredFile {
    #[serde(default)]
    ignored: Vec<Ignored>,
}

impl Finding {
    /// Use, Locate… and Replace… can point it at another file.
    pub fn can_point_elsewhere(&self) -> bool {
        match self.site {
            Site::File { .. } | Site::PrefabLink => false,
            Site::ModelTexture { .. } => true,
            _ => self.wants.is_some(),
        }
    }

    /// Clear can empty its reference.
    pub fn can_clear(&self) -> bool {
        matches!(
            self.site,
            Site::Instance { .. }
                | Site::Prefab { .. }
                | Site::Level { .. }
                | Site::ClipSource { .. }
                | Site::StateClip { .. }
                | Site::TableCell { .. }
        )
    }

    /// Delete file can remove it: a file that fails on its own.
    pub fn can_delete(&self) -> bool {
        matches!(self.site, Site::File { .. }) && safe_path(&self.path).is_some()
    }

    /// The kind of file Locate… and Replace… offer.
    pub fn file_kind(&self) -> Option<AssetKind> {
        match self.site {
            Site::ModelTexture { .. } => Some(AssetKind::Texture),
            Site::File { .. } => None,
            _ => self.wants,
        }
    }

    /// Whether "Apply to all like this" on `self` also fixes `other`: the
    /// same problem with the same file. A file that isn't there is one
    /// problem whether the world or another asset names it.
    pub fn is_like(&self, other: &Finding) -> bool {
        let gone = |kind| {
            matches!(
                kind,
                FindingKind::MissingFile | FindingKind::BrokenDependency
            )
        };
        let same_kind = self.kind == other.kind || (gone(self.kind) && gone(other.kind));
        if !same_kind || self.path != other.path || self.wants != other.wants {
            return false;
        }
        match (&self.site, &other.site) {
            (
                Site::ModelTexture { model, texture },
                Site::ModelTexture {
                    model: other_model,
                    texture: other_texture,
                },
            ) => model == other_model && texture == other_texture,
            (Site::ModelTexture { .. }, _) | (_, Site::ModelTexture { .. }) => false,
            // A missing row is only like another missing from the same table.
            (Site::TableCell { .. }, Site::TableCell { .. }) if self.wants.is_none() => {
                self.message == other.message
            }
            (Site::File { .. }, other) | (other, Site::File { .. }) => {
                matches!(other, Site::File { .. })
            }
            _ => true,
        }
    }

    /// A file on this computer that the reference names by its full path,
    /// outside the project, which Copy in can bring into the project.
    pub fn outside_file(&self) -> Option<PathBuf> {
        if self.kind != FindingKind::InvalidReference || !self.can_point_elsewhere() {
            return None;
        }
        let path = Path::new(self.target());
        let kind = self.file_kind()?;
        (path.is_absolute() && path.is_file() && kind.matches(self.target()))
            .then(|| path.to_path_buf())
    }

    /// Its key in the ignored list.
    pub fn ignore_key(&self) -> Ignored {
        Ignored {
            kind: self.kind.id().to_owned(),
            owner: self.owner.clone(),
            path: self.path.clone(),
            message: self.message.clone(),
        }
    }

    /// The file the reference names, as written: the name a model gives its
    /// texture, or the path without a model's `#` pick.
    fn target(&self) -> &str {
        match &self.site {
            Site::ModelTexture { texture, .. } => texture,
            _ if self.wants == Some(AssetKind::Model) => Pick::of_mesh_id(&self.path).0,
            _ => &self.path,
        }
    }
}

/// Every file in the project, as project paths, for [`suggestion`] and
/// [`files_of_kind`]; one listing serves a whole report.
pub fn list_project_files(folder: &Path) -> Vec<String> {
    project_files(folder)
}

/// The files in `files` that a reference of `kind` can name, for Replace….
pub fn files_of_kind(files: &[String], kind: AssetKind) -> Vec<String> {
    if kind == AssetKind::Folder {
        return Vec::new();
    }
    files
        .iter()
        .filter(|file| kind.matches(file))
        .cloned()
        .collect()
}

/// A project file with the name `finding`'s reference wants, somewhere
/// else in the project, for Use: where a moved file went. With several,
/// the one in the most of the same folders wins, then the shortest path.
pub fn suggestion(finding: &Finding, files: &[String]) -> Option<String> {
    if !finding.can_point_elsewhere() {
        return None;
    }
    let kind = finding.file_kind()?;
    if kind == AssetKind::Folder {
        return None;
    }
    let target = finding.target();
    let name = file_name(target);
    if name.is_empty() {
        return None;
    }
    // A model looks for its texture in these places, so a file there failed to read.
    let looked = match &finding.site {
        Site::ModelTexture { model, .. } => {
            let folder = parent_folder(model);
            vec![
                join(&folder, name),
                join(&join(&folder, TEXTURES_FOLDER), name),
            ]
        }
        _ => Vec::new(),
    };
    let wanted: Vec<&str> = target.split(['/', '\\']).collect();
    let shared = |file: &str| {
        file.split('/')
            .zip(&wanted)
            .take_while(|(a, b)| a.eq_ignore_ascii_case(b))
            .count()
    };
    let found = files
        .iter()
        .filter(|file| kind.matches(file))
        .filter(|file| file_name(file).eq_ignore_ascii_case(name))
        .filter(|file| file.as_str() != target)
        .filter(|file| !looked.iter().any(|spot| spot.eq_ignore_ascii_case(file)))
        .min_by(|a, b| {
            shared(b)
                .cmp(&shared(a))
                .then(a.len().cmp(&b.len()))
                .then(a.cmp(b))
        })?;
    Some(match kind {
        AssetKind::Model => with_pick(&finding.path, found),
        _ => found.clone(),
    })
}

/// What `fix` changes for every finding in `findings`, without changing
/// anything. A file several findings share is read and written once.
pub fn plan_fix(folder: &Path, findings: &[&Finding], fix: &Fix) -> Result<FixPlan, String> {
    let mut planner = Planner {
        folder,
        plan: FixPlan::default(),
        docs: BTreeMap::new(),
        copies: BTreeMap::new(),
    };
    for finding in findings {
        planner.add(finding, fix)?;
    }
    planner.finish()
}

/// The project's ignored findings. A missing or damaged file is none.
pub fn read_ignored(folder: &Path) -> Vec<Ignored> {
    fs::read_to_string(folder.join(IGNORED_FILE))
        .ok()
        .and_then(|text| serde_json::from_str::<IgnoredFile>(&text).ok())
        .map(|file| file.ignored)
        .unwrap_or_default()
}

/// Saves the project's ignored findings, sorted. None removes the file.
pub fn write_ignored(folder: &Path, ignored: &[Ignored]) -> io::Result<()> {
    let path = folder.join(IGNORED_FILE);
    if ignored.is_empty() {
        return match fs::remove_file(&path) {
            Err(err) if err.kind() == ErrorKind::NotFound => Ok(()),
            other => other,
        };
    }
    let mut ignored = ignored.to_vec();
    ignored.sort();
    ignored.dedup();
    let mut text = serde_json::to_string_pretty(&IgnoredFile { ignored })
        .map_err(|err| io::Error::new(ErrorKind::InvalidData, err))?;
    text.push('\n');
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, text)
}

/// A prefab, clip or state machine file being rewritten, with its bytes
/// as they were.
enum Doc {
    Prefab {
        // Boxed: a world is much bigger than the other two.
        world: Box<World>,
        roots: Vec<InstanceId>,
    },
    /// A level file (Phase 48), written back as it was read: `world.json`
    /// whole, a `levels/` file as its Workspace.
    Level(Box<World>),
    Clip(ClipFile),
    Machine(StateMachineFile),
    Table(DataTable),
}

struct Planner<'a> {
    folder: &'a Path,
    plan: FixPlan,
    /// Files rewritten, by path, with their bytes before.
    docs: BTreeMap<String, (Vec<u8>, Doc)>,
    /// Files copied in, by their project path: bytes before and after.
    copies: BTreeMap<String, (Option<Vec<u8>>, Vec<u8>)>,
}

impl Planner<'_> {
    fn add(&mut self, finding: &Finding, fix: &Fix) -> Result<(), String> {
        if let Site::ModelTexture { model, texture } = &finding.site {
            let source = match fix {
                Fix::Use(path) => self.disk(path)?,
                Fix::CopyIn(file) => file.clone(),
                Fix::Clear | Fix::DeleteFile => {
                    return Err("a model's own texture can only be located".to_owned());
                }
            };
            return self.copy_texture(model, texture, &source);
        }
        match fix {
            Fix::DeleteFile => self.delete(finding),
            Fix::Clear => self.set(finding, None),
            Fix::Use(path) => {
                let kind = finding
                    .file_kind()
                    .ok_or("this problem isn't a reference")?;
                let (file, _) = Pick::of_mesh_id(path);
                let file = if kind == AssetKind::Model { file } else { path };
                if safe_path(file).is_none() {
                    return Err(format!("`{path}` isn't a path inside the project"));
                }
                if !kind.matches(file) {
                    return Err(format!("`{file}` isn't {}", kind.expected()));
                }
                let value = match kind {
                    AssetKind::Model => with_pick(&finding.path, path),
                    _ => path.clone(),
                };
                self.set(finding, Some(value))
            }
            Fix::CopyIn(file) => {
                let path = self.copy_in(finding, file)?;
                self.add(finding, &Fix::Use(path))
            }
        }
    }

    /// The project path for `file`: its own inside the project, otherwise
    /// a copy's, in the folder Import uses for its kind.
    fn copy_in(&mut self, finding: &Finding, file: &Path) -> Result<String, String> {
        if let Some(inside) = inside_project(self.folder, file) {
            return Ok(inside);
        }
        let kind = finding
            .file_kind()
            .ok_or("this problem isn't a reference")?;
        let shown = file.to_string_lossy();
        if !kind.matches(&shown) {
            return Err(format!("`{shown}` isn't {}", kind.expected()));
        }
        let into = match kind {
            AssetKind::Model => MODELS_FOLDER,
            AssetKind::Audio => PROJECT_AUDIO_FOLDER,
            AssetKind::Clip | AssetKind::StateMachine => ANIMATIONS_FOLDER,
            AssetKind::Material => MATERIALS_FOLDER,
            AssetKind::Prefab => PREFABS_FOLDER,
            AssetKind::Folder => return Err("pick a folder inside the project".to_owned()),
            AssetKind::Texture => match finding.site {
                Site::Instance { property, .. }
                | Site::Prefab { property, .. }
                | Site::Level { property, .. } => match property {
                    "Image" => IMAGES_FOLDER,
                    "Texture" => PARTICLES_FOLDER,
                    _ => PROJECT_TEXTURES_FOLDER,
                },
                _ => PROJECT_TEXTURES_FOLDER,
            },
        };
        if kind == AssetKind::Model && shown.to_ascii_lowercase().ends_with(".gltf") {
            return Err(
                "a .gltf file comes with other files: import it with File > Import…, \
                 then use Replace…"
                    .to_owned(),
            );
        }
        let bytes = fs::read(file).map_err(|err| format!("could not read `{shown}`: {err}"))?;
        let name = safe_file_name(&shown).ok_or_else(|| format!("`{shown}` has no file name"))?;
        // The same bytes already there, or already being copied, are used as they are.
        let same = |path: &str, copies: &BTreeMap<String, (Option<Vec<u8>>, Vec<u8>)>| {
            copies
                .get(path)
                .map(|(_, after)| after == &bytes)
                .unwrap_or_else(|| {
                    fs::read(self.folder.join(path)).is_ok_and(|there| there == bytes)
                })
        };
        let taken = |path: &str| self.copies.contains_key(path) || self.folder.join(path).exists();
        let name = unique_file_name(&name, |name| {
            let path = join(into, name);
            taken(&path) && !same(&path, &self.copies)
        });
        let path = join(into, &name);
        if !same(&path, &self.copies) {
            self.copies.insert(path.clone(), (None, bytes));
        }
        Ok(path)
    }

    /// Copies `source` to where the model at `model` looks for `texture`:
    /// `textures/<name>` in the model's folder.
    fn copy_texture(&mut self, model: &str, texture: &str, source: &Path) -> Result<(), String> {
        let name =
            safe_file_name(texture).ok_or_else(|| format!("`{texture}` has no file name"))?;
        let wanted = extension(&name);
        let shown = source.to_string_lossy();
        if extension(&shown) != wanted {
            return Err(format!(
                "the model asks for `{name}`, so pick a .{wanted} file"
            ));
        }
        let bytes = fs::read(source).map_err(|err| format!("could not read `{shown}`: {err}"))?;
        let path = join(&join(&parent_folder(model), TEXTURES_FOLDER), &name);
        if safe_path(&path).is_none() {
            return Err(format!("`{model}` isn't a path inside the project"));
        }
        let before = fs::read(self.folder.join(&path)).ok();
        if before.as_ref() != Some(&bytes) {
            self.copies.insert(path, (before, bytes));
        }
        Ok(())
    }

    fn delete(&mut self, finding: &Finding) -> Result<(), String> {
        let Site::File { file } = &finding.site else {
            return Err("only a file that can't be loaded can be deleted".to_owned());
        };
        if self.plan.files.iter().any(|write| &write.path == file) {
            return Ok(());
        }
        let before =
            fs::read(self.disk(file)?).map_err(|err| format!("could not read `{file}`: {err}"))?;
        self.plan.files.push(FileWrite {
            path: file.clone(),
            before: Some(before),
            after: None,
        });
        Ok(())
    }

    /// Points `finding`'s reference at `value`, or clears it for `None`.
    fn set(&mut self, finding: &Finding, value: Option<String>) -> Result<(), String> {
        match &finding.site {
            Site::Instance { id, property } => {
                self.plan.properties.push(PropertyFix {
                    id: *id,
                    property,
                    value: value.unwrap_or_default(),
                });
                Ok(())
            }
            Site::Prefab {
                file,
                place,
                property,
            } => {
                let Doc::Prefab { world, roots } = self.doc(file, read_prefab)? else {
                    return Err(format!("`{file}` isn't a prefab"));
                };
                let id = place_in(world, roots, place)
                    .ok_or_else(|| format!("`{file}` changed since the check; check again"))?;
                world
                    .set_property(id, property, Value::String(value.unwrap_or_default()))
                    .map_err(|err| format!("could not change `{file}`: {err}"))
            }
            Site::Level {
                file,
                place,
                property,
            } => {
                let Doc::Level(world) = self.doc(file, read_level)? else {
                    return Err(format!("`{file}` isn't a level"));
                };
                let roots = world
                    .get(world.workspace())
                    .map(|workspace| workspace.children().to_vec())
                    .unwrap_or_default();
                let id = place_in(world, &roots, place)
                    .ok_or_else(|| format!("`{file}` changed since the check; check again"))?;
                world
                    .set_property(id, property, Value::String(value.unwrap_or_default()))
                    .map_err(|err| format!("could not change `{file}`: {err}"))
            }
            Site::ClipSource { file } => {
                let Doc::Clip(clip) = self.doc(file, read_clip)? else {
                    return Err(format!("`{file}` isn't a clip"));
                };
                match value {
                    None => clip.link = None,
                    Some(value) => {
                        if let Some(link) = &mut clip.link {
                            link.source = value;
                        }
                    }
                }
                Ok(())
            }
            Site::StateClip { file, state } => {
                let Doc::Machine(machine) = self.doc(file, read_machine)? else {
                    return Err(format!("`{file}` isn't a state machine"));
                };
                let state = machine
                    .states
                    .iter_mut()
                    .find(|found| &found.name == state)
                    .ok_or_else(|| format!("`{file}` changed since the check; check again"))?;
                for clip in state.motion.clips_mut() {
                    if *clip == finding.path {
                        *clip = value.clone().unwrap_or_default();
                    }
                }
                Ok(())
            }
            Site::TableCell { file, row, column } => {
                let Doc::Table(table) = self.doc(file, read_table)? else {
                    return Err(format!("`{file}` isn't a data table"));
                };
                table
                    .set_cell(row, column, Cell::Text(value.unwrap_or_default()))
                    .map_err(|_| format!("`{file}` changed since the check; check again"))
            }
            Site::ModelTexture { .. } | Site::File { .. } | Site::PrefabLink => {
                Err("this problem has no reference to change".to_owned())
            }
        }
    }

    /// The file at `path` being rewritten, read the first time.
    fn doc(
        &mut self,
        path: &str,
        read: fn(&str) -> Result<Doc, String>,
    ) -> Result<&mut Doc, String> {
        if !self.docs.contains_key(path) {
            let bytes = fs::read(self.disk(path)?)
                .map_err(|err| format!("could not read `{path}`: {err}"))?;
            let text = String::from_utf8_lossy(&bytes).into_owned();
            let doc = read(&text).map_err(|err| format!("could not load `{path}`: {err}"))?;
            self.docs.insert(path.to_owned(), (bytes, doc));
        }
        Ok(&mut self.docs.get_mut(path).expect("just added").1)
    }

    fn disk(&self, path: &str) -> Result<PathBuf, String> {
        safe_path(path)
            .map(|relative| self.folder.join(relative))
            .ok_or_else(|| format!("`{path}` isn't a path inside the project"))
    }

    fn finish(mut self) -> Result<FixPlan, String> {
        for (path, (before, doc)) in self.docs {
            let after = match doc {
                Doc::Prefab { world, roots } => write_prefab(&world, &roots),
                Doc::Level(world) if path == crate::WORLD_FILE => write_world(&world),
                Doc::Level(world) => crate::write_level(&world),
                Doc::Clip(clip) => clip.to_json(),
                Doc::Machine(machine) => machine.to_json(),
                Doc::Table(table) => table.to_json(),
            }
            .into_bytes();
            if after != before {
                self.plan.files.push(FileWrite {
                    path,
                    before: Some(before),
                    after: Some(after),
                });
            }
        }
        for (path, (before, after)) in self.copies {
            self.plan.files.push(FileWrite {
                path,
                before,
                after: Some(after),
            });
        }
        Ok(self.plan)
    }
}

fn read_prefab(text: &str) -> Result<Doc, String> {
    let mut world = World::new();
    let workspace = world.workspace();
    let roots = parse_prefab(text, &mut world, workspace).map_err(|err| err.to_string())?;
    Ok(Doc::Prefab {
        world: Box::new(world),
        roots,
    })
}

fn read_level(text: &str) -> Result<Doc, String> {
    parse_world(text)
        .map(|world| Doc::Level(Box::new(world)))
        .map_err(|err| err.to_string())
}

fn read_clip(text: &str) -> Result<Doc, String> {
    ClipFile::from_json(text)
        .map(Doc::Clip)
        .map_err(|err| err.to_string())
}

fn read_table(text: &str) -> Result<Doc, String> {
    DataTable::from_json(text)
        .map(Doc::Table)
        .map_err(|err| err.to_string())
}

fn read_machine(text: &str) -> Result<Doc, String> {
    StateMachineFile::from_json(text)
        .map(Doc::Machine)
        .map_err(|err| err.to_string())
}

/// The Instance at `place` in a prefab read into `world` (see [`Site::Prefab`]).
fn place_in(world: &World, roots: &[InstanceId], place: &[usize]) -> Option<InstanceId> {
    let (first, rest) = place.split_first()?;
    let mut id = *roots.get(*first)?;
    for &index in rest {
        id = *world.get(id)?.children().get(index)?;
    }
    Some(id)
}

/// `file`'s project path, if it's inside `folder`.
fn inside_project(folder: &Path, file: &Path) -> Option<String> {
    let folder = folder.canonicalize().ok()?;
    let file = file.canonicalize().ok()?;
    let inside = file.strip_prefix(&folder).ok()?;
    let parts: Vec<String> = inside
        .components()
        .map(|part| part.as_os_str().to_string_lossy().into_owned())
        .collect();
    Some(parts.join("/"))
}

/// `new`, keeping the `#` pick of the `MeshId` `old` when `new` has none
/// and names a file of the same name (the model was moved, not swapped).
fn with_pick(old: &str, new: &str) -> String {
    if new.contains('#') {
        return new.to_owned();
    }
    match old.split_once('#') {
        Some((file, pick)) if file_name(file).eq_ignore_ascii_case(file_name(new)) => {
            format!("{new}#{pick}")
        }
        _ => new.to_owned(),
    }
}

fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

fn parent_folder(path: &str) -> String {
    path.rsplit_once('/')
        .map(|(folder, _)| folder.to_owned())
        .unwrap_or_default()
}

fn join(folder: &str, name: &str) -> String {
    match folder.trim_end_matches('/') {
        "" => name.to_owned(),
        folder => format!("{folder}/{name}"),
    }
}

fn extension(path: &str) -> String {
    file_name(path)
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests;
