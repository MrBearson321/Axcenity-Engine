//! Where a project uses an asset file (unified Phase 70): Find References
//! and the Unused filter in the Assets panel, and renaming or moving an
//! asset with every reference to it. Like [`check_assets`](crate::check_assets),
//! nothing here writes the project: [`plan_rename`] returns the property
//! changes and file writes, and the editor makes them as one undo step.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use axcenity_anim::{
    ANIMATIONS_FOLDER, CLIP_EXTENSION, ClipFile, STATE_MACHINE_EXTENSION, StateMachineFile,
};
use axcenity_assets::{
    Cell, ColumnType, DATA_FOLDER, DataTable, MODELS_FOLDER, Pick, is_data_table, is_material_file,
    material_json, parse_material, safe_path,
};
use axcenity_blocks_runtime::BlockScript;
use axcenity_core::{ClassName, FoliageSource, InstanceId, World};

use crate::asset_fix::{FileWrite, Fix, FixPlan, plan_fix};
use crate::validate::{
    AssetKind, Finding, FindingKind, PREFAB_SUFFIX, PREFABS_FOLDER, Site, files_under, owner_name,
    prefab_place, refs, string_prop,
};
use crate::world_file::{parse_prefab, parse_world};

/// One place that names an asset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reference {
    /// The Instance path or file that holds it, as Asset Check names owners.
    pub owner: String,
    /// The property, field or line that names it.
    pub what: String,
    /// The reference as written (a model's `#` pick included).
    pub path: String,
    pub place: RefPlace,
}

/// Where a [`Reference`] is written, so a rename can change it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RefPlace {
    /// A place Asset Check fixes can change, wanting this kind of file.
    Site(Site, AssetKind),
    /// A string in a material file or a texture set.
    Json { file: String },
    /// A literal path in a Lua script, on this line (from 1).
    Script { file: String, line: usize },
    /// A string in a block script, in this block.
    Blocks { file: String, block: Option<u64> },
    /// A copy of a prefab, which only the prefab's own Rename relinks.
    PrefabLink,
    /// Painted foliage of a Terrain, which a rename doesn't change.
    Foliage { terrain: InstanceId },
}

impl Reference {
    /// The Instance of the open world it's on, to select.
    pub fn instance(&self) -> Option<InstanceId> {
        match &self.place {
            RefPlace::Site(Site::Instance { id, .. }, _) => Some(*id),
            RefPlace::Foliage { terrain } => Some(*terrain),
            _ => None,
        }
    }

    /// The project file it's in, if it isn't the open world.
    pub fn file(&self) -> Option<&str> {
        match &self.place {
            RefPlace::Site(site, _) => match site {
                Site::Prefab { file, .. }
                | Site::Level { file, .. }
                | Site::ClipSource { file }
                | Site::StateClip { file, .. }
                | Site::TableCell { file, .. }
                | Site::File { file } => Some(file),
                Site::ModelTexture { model, .. } => Some(model),
                Site::Instance { .. } | Site::PrefabLink => None,
            },
            RefPlace::Json { file }
            | RefPlace::Script { file, .. }
            | RefPlace::Blocks { file, .. } => Some(file),
            RefPlace::PrefabLink | RefPlace::Foliage { .. } => None,
        }
    }

    /// Whether it names the asset file `asset`.
    fn names(&self, asset: &str) -> bool {
        match &self.place {
            RefPlace::Script { .. } | RefPlace::Blocks { .. } => contains_path(&self.path, asset),
            RefPlace::Site(_, AssetKind::Model) | RefPlace::Foliage { .. } => {
                Pick::of_mesh_id(&self.path).0 == asset
            }
            _ => self.path == asset,
        }
    }
}

/// Every reference to the asset file `asset` (a project path): Instances in
/// `world` (the open level, `open`; `None` for the start level) and in the
/// other levels, prefabs, clips, state machines, data tables, materials,
/// texture sets and scripts that name it by its literal path.
pub fn find_references(
    folder: &Path,
    world: &World,
    open: Option<&str>,
    asset: &str,
) -> Vec<Reference> {
    let mut found = Vec::new();
    walk(folder, world, open, &mut |reference| {
        if reference.names(asset) {
            found.push(reference);
        }
    });
    found
}

/// The files of `assets` that nothing names. A Lua or block script that
/// has the path anywhere in it counts.
pub fn unused_assets(
    folder: &Path,
    world: &World,
    open: Option<&str>,
    assets: &[String],
) -> Vec<String> {
    let mut named = BTreeSet::new();
    let mut texts = Vec::new();
    walk(
        folder,
        world,
        open,
        &mut |reference| match reference.place {
            RefPlace::Site(_, AssetKind::Model) | RefPlace::Foliage { .. } => {
                named.insert(Pick::of_mesh_id(&reference.path).0.to_owned());
            }
            RefPlace::Script { .. } | RefPlace::Blocks { .. } => {}
            _ => {
                named.insert(reference.path);
            }
        },
    );
    for file in script_files(folder) {
        if let Ok(text) = fs::read_to_string(folder.join(&file)) {
            texts.push(text);
        }
    }
    assets
        .iter()
        .filter(|asset| {
            !named.contains(asset.as_str()) && !texts.iter().any(|text| contains_path(text, asset))
        })
        .cloned()
        .collect()
}

/// What renaming or moving the asset file `from` to `to` changes: the file
/// itself, and every reference [`find_references`] finds, pointed at `to`.
/// Writes nothing.
pub fn plan_rename(
    folder: &Path,
    world: &World,
    open: Option<&str>,
    from: &str,
    to: &str,
) -> Result<FixPlan, String> {
    let from_disk = safe_path(from).ok_or_else(|| format!("`{from}` isn't a project path"))?;
    let to_disk = safe_path(to).ok_or_else(|| format!("`{to}` isn't a path inside the project"))?;
    if from == to {
        return Err("that's its name already".to_owned());
    }
    if !folder.join(&from_disk).is_file() {
        return Err(format!("`{from}` isn't there"));
    }
    if !to.starts_with("assets/") {
        return Err("assets stay in the assets folder".to_owned());
    }
    let same_file = from.eq_ignore_ascii_case(to);
    if folder.join(&to_disk).exists() && !same_file {
        return Err(format!("`{to}` is taken"));
    }
    if extension(from) != extension(to) {
        return Err(format!("keep the `.{}` ending", extension(from)));
    }
    if from.to_ascii_lowercase().ends_with(PREFAB_SUFFIX) {
        return Err("rename a prefab with its own Rename, which relinks its copies".to_owned());
    }
    if extension(from) == "gltf" && parent(from) != parent(to) {
        return Err("a .gltf names the files beside it: rename it in its folder".to_owned());
    }

    let references = find_references(folder, world, open, from);
    let mut findings = Vec::new();
    let mut files: Vec<FileWrite> = Vec::new();
    for reference in &references {
        match &reference.place {
            RefPlace::Site(site, kind) => findings.push(Finding {
                kind: FindingKind::MissingFile,
                owner: reference.owner.clone(),
                path: reference.path.clone(),
                message: String::new(),
                site: site.clone(),
                wants: Some(*kind),
            }),
            RefPlace::Foliage { .. } => {
                return Err(format!(
                    "{} paints it as foliage; repaint it after the rename instead",
                    reference.owner
                ));
            }
            RefPlace::PrefabLink => {}
            RefPlace::Json { file }
            | RefPlace::Script { file, .. }
            | RefPlace::Blocks { file, .. } => {
                if files.iter().any(|write| write.path == *file) {
                    continue;
                }
                let before = fs::read(folder.join(safe_path(file).ok_or("bad path")?))
                    .map_err(|err| format!("could not read `{file}`: {err}"))?;
                let text = String::from_utf8_lossy(&before);
                let after = match &reference.place {
                    RefPlace::Script { .. } => replace_path(&text, from, to),
                    RefPlace::Blocks { .. } => rewrite_blocks(&text, from, to)?,
                    _ => rewrite_json(file, &text, from, to)?,
                };
                files.push(FileWrite {
                    path: file.clone(),
                    before: Some(before),
                    after: Some(after.into_bytes()),
                });
            }
        }
    }
    let finding_refs: Vec<&Finding> = findings.iter().collect();
    let mut plan = plan_fix(folder, &finding_refs, &Fix::Use(to.to_owned()))?;

    // The file itself moves, with any change made to it above.
    let mut bytes = fs::read(folder.join(&from_disk))
        .map_err(|err| format!("could not read `{from}`: {err}"))?;
    files.retain(|write| {
        if write.path == from {
            bytes = write.after.clone().unwrap_or_default();
            false
        } else {
            true
        }
    });
    plan.files.extend(files);
    plan.files.push(FileWrite {
        path: from.to_owned(),
        before: Some(fs::read(folder.join(&from_disk)).unwrap_or_default()),
        after: None,
    });
    plan.files.push(FileWrite {
        path: to.to_owned(),
        before: None,
        after: Some(bytes),
    });
    Ok(plan)
}

/// Calls `found` with every reference to any file in the project.
fn walk(folder: &Path, world: &World, open: Option<&str>, found: &mut dyn FnMut(Reference)) {
    world_refs(world, world.root(), None, found);
    for (file, other) in other_levels(folder, open) {
        let roots = other
            .get(other.workspace())
            .map(|workspace| workspace.children().to_vec())
            .unwrap_or_default();
        instance_refs(&other, other.workspace(), &mut |id, what, path, kind| {
            let Some(place) = prefab_place(&other, &roots, id) else {
                return;
            };
            let site = Site::Level {
                file: file.clone(),
                place,
                property: what,
            };
            found(Reference {
                owner: owner_name(&other, id, Some(&file)),
                what: what.to_owned(),
                path: path.to_owned(),
                place: RefPlace::Site(site, kind),
            });
        });
        links(&other, other.workspace(), Some(&file), found);
    }
    for file in files_under(folder, PREFABS_FOLDER) {
        if !file.to_ascii_lowercase().ends_with(PREFAB_SUFFIX) {
            continue;
        }
        let Ok(text) = fs::read_to_string(folder.join(&file)) else {
            continue;
        };
        let mut prefab = World::new();
        let workspace = prefab.workspace();
        let Ok(roots) = parse_prefab(&text, &mut prefab, workspace) else {
            continue;
        };
        instance_refs(&prefab, workspace, &mut |id, what, path, kind| {
            let Some(place) = prefab_place(&prefab, &roots, id) else {
                return;
            };
            let site = Site::Prefab {
                file: file.clone(),
                place,
                property: what,
            };
            found(Reference {
                owner: owner_name(&prefab, id, Some(&file)),
                what: what.to_owned(),
                path: path.to_owned(),
                place: RefPlace::Site(site, kind),
            });
        });
        links(&prefab, workspace, Some(&file), found);
    }
    animation_refs(folder, found);
    table_refs(folder, found);
    json_refs(folder, found);
    script_refs(folder, found);
}

/// The open world's references, under `root`.
fn world_refs(
    world: &World,
    root: InstanceId,
    file: Option<&str>,
    found: &mut dyn FnMut(Reference),
) {
    instance_refs(world, root, &mut |id, what, path, kind| {
        found(Reference {
            owner: owner_name(world, id, file),
            what: what.to_owned(),
            path: path.to_owned(),
            place: RefPlace::Site(Site::Instance { id, property: what }, kind),
        });
    });
    links(world, root, file, found);
    for id in world.descendants(root) {
        let Some(data) = world.terrain(id) else {
            continue;
        };
        let mut models = BTreeSet::new();
        for plant in data.foliage() {
            if let FoliageSource::Model(path) = &plant.source {
                models.insert(path.clone());
            }
        }
        for path in models {
            found(Reference {
                owner: owner_name(world, id, file),
                what: "foliage".to_owned(),
                path,
                place: RefPlace::Foliage { terrain: id },
            });
        }
    }
}

/// Each property naming an asset under `root`: Asset Check's, and a part's
/// material file and a Tool's picture, which it doesn't check.
fn instance_refs(
    world: &World,
    root: InstanceId,
    found: &mut dyn FnMut(InstanceId, &'static str, &str, AssetKind),
) {
    for id in world.descendants(root) {
        let Some(instance) = world.get(id) else {
            continue;
        };
        let mut properties = refs(instance.class());
        if instance.property("MaterialVariant").is_some() {
            properties.push(("MaterialVariant", AssetKind::Material));
        }
        if instance.class() == ClassName::Tool {
            properties.push(("TextureId", AssetKind::Texture));
        }
        for (property, kind) in properties {
            if let Some(path) = string_prop(instance.property(property))
                && !path.is_empty()
            {
                found(id, property, path, kind);
            }
        }
    }
}

/// The prefab copies under `root`.
fn links(world: &World, root: InstanceId, file: Option<&str>, found: &mut dyn FnMut(Reference)) {
    for id in world.descendants(root) {
        if let Some(link) = world.get(id).and_then(|instance| instance.prefab()) {
            found(Reference {
                owner: owner_name(world, id, file),
                what: "prefab".to_owned(),
                path: link.path.clone(),
                place: RefPlace::PrefabLink,
            });
        }
    }
}

/// The level files but the open one, read: `world.json`'s Workspace while
/// another level is open.
fn other_levels(folder: &Path, open: Option<&str>) -> Vec<(String, World)> {
    let mut files: Vec<String> = crate::level_names(folder)
        .iter()
        .filter(|&name| Some(name.as_str()) != open)
        .map(|name| crate::level_path(name))
        .collect();
    if open.is_some() {
        files.insert(0, crate::WORLD_FILE.to_owned());
    }
    files
        .into_iter()
        .filter_map(|file| {
            let text = fs::read_to_string(folder.join(&file)).ok()?;
            Some((file, parse_world(&text).ok()?))
        })
        .collect()
}

fn animation_refs(folder: &Path, found: &mut dyn FnMut(Reference)) {
    for file in files_under(folder, ANIMATIONS_FOLDER) {
        let lower = file.to_ascii_lowercase();
        let Ok(text) = fs::read_to_string(folder.join(&file)) else {
            continue;
        };
        if lower.ends_with(CLIP_EXTENSION) {
            if let Ok(ClipFile {
                link: Some(link), ..
            }) = ClipFile::from_json(&text)
            {
                found(Reference {
                    owner: file.clone(),
                    what: "source".to_owned(),
                    path: link.source,
                    place: RefPlace::Site(
                        Site::ClipSource { file: file.clone() },
                        AssetKind::Model,
                    ),
                });
            }
        } else if lower.ends_with(STATE_MACHINE_EXTENSION)
            && let Ok(machine) = StateMachineFile::from_json(&text)
        {
            for state in &machine.states {
                for clip in state.motion.clips() {
                    found(Reference {
                        owner: file.clone(),
                        what: state.name.clone(),
                        path: clip.to_owned(),
                        place: RefPlace::Site(
                            Site::StateClip {
                                file: file.clone(),
                                state: state.name.clone(),
                            },
                            AssetKind::Clip,
                        ),
                    });
                }
            }
        }
    }
}

fn table_refs(folder: &Path, found: &mut dyn FnMut(Reference)) {
    for file in files_under(folder, DATA_FOLDER) {
        if !is_data_table(&file) {
            continue;
        }
        let Some(table) = fs::read_to_string(folder.join(&file))
            .ok()
            .and_then(|text| DataTable::from_json(&text).ok())
        else {
            continue;
        };
        for row in &table.rows {
            for column in &table.columns {
                let ColumnType::Asset { asset } = &column.kind else {
                    continue;
                };
                let Cell::Text(value) = row.cell(column) else {
                    continue;
                };
                if value.is_empty() {
                    continue;
                }
                found(Reference {
                    owner: format!("{file} ({}.{})", row.key, column.name),
                    what: column.name.clone(),
                    path: value.to_string(),
                    place: RefPlace::Site(
                        Site::TableCell {
                            file: file.clone(),
                            row: row.key.clone(),
                            column: column.name.clone(),
                        },
                        AssetKind::of_column(*asset),
                    ),
                });
            }
        }
    }
}

/// Material files (their maps, base and graph images) and texture sets.
fn json_refs(folder: &Path, found: &mut dyn FnMut(Reference)) {
    for file in files_under(folder, "assets") {
        if !is_material_file(&file) && !file.to_ascii_lowercase().ends_with(".texture.json") {
            continue;
        }
        let Some(value) = fs::read_to_string(folder.join(&file))
            .ok()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        else {
            continue;
        };
        strings(&value, None, &mut |key, text, _| {
            if text.starts_with("assets/") && !text.contains('\n') {
                found(Reference {
                    owner: file.clone(),
                    what: key.unwrap_or("path").to_owned(),
                    path: text.to_owned(),
                    place: RefPlace::Json { file: file.clone() },
                });
            }
        });
    }
}

/// Lua and block scripts that have an asset path in them.
fn script_refs(folder: &Path, found: &mut dyn FnMut(Reference)) {
    for file in script_files(folder) {
        let Ok(text) = fs::read_to_string(folder.join(&file)) else {
            continue;
        };
        if crate::is_block_script(&file) {
            let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
                continue;
            };
            strings(&value, None, &mut |key, text, block| {
                if text.contains("assets/") {
                    found(Reference {
                        owner: file.clone(),
                        what: key.unwrap_or("text").to_owned(),
                        path: text.to_owned(),
                        place: RefPlace::Blocks {
                            file: file.clone(),
                            block,
                        },
                    });
                }
            });
        } else {
            for (index, line) in text.lines().enumerate() {
                if line.contains("assets/") {
                    found(Reference {
                        owner: file.clone(),
                        what: format!("line {}", index + 1),
                        path: line.trim().to_owned(),
                        place: RefPlace::Script {
                            file: file.clone(),
                            line: index + 1,
                        },
                    });
                }
            }
        }
    }
}

fn script_files(folder: &Path) -> Vec<String> {
    crate::find_scripts(folder, &mut Vec::new())
}

/// What [`strings`] calls with each string: its key, the text, and the
/// block it's in.
type StringVisitor<'a> = dyn FnMut(Option<&str>, &str, Option<u64>) + 'a;

/// Calls `found` with each string in `value` but a block's `type`: its key,
/// the text, and the `id` of the nearest object around it that has one.
fn strings(value: &serde_json::Value, block: Option<u64>, found: &mut StringVisitor<'_>) {
    match value {
        serde_json::Value::Object(map) => {
            let block = map.get("id").and_then(serde_json::Value::as_u64).or(block);
            for (key, item) in map {
                match item {
                    serde_json::Value::String(text) if key != "type" => {
                        found(Some(key), text, block);
                    }
                    _ => strings(item, block, found),
                }
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                strings(item, block, found);
            }
        }
        serde_json::Value::String(text) => found(None, text, block),
        _ => {}
    }
}

/// Whether `text` has `path` in it on its own, not as part of a longer path.
fn contains_path(text: &str, path: &str) -> bool {
    path_matches(text, path).next().is_some()
}

/// Where `path` is in `text` on its own.
fn path_matches<'a>(text: &'a str, path: &'a str) -> impl Iterator<Item = usize> + 'a {
    let part = |c: char| c.is_alphanumeric() || matches!(c, '.' | '_' | '-' | '/');
    text.match_indices(path).filter_map(move |(at, _)| {
        let before = text[..at].chars().next_back();
        let after = text[at + path.len()..].chars().next();
        // A model's `#` pick may follow.
        let after_ok = after.is_none_or(|c| !part(c) || c == '#');
        (before.is_none_or(|c| !part(c)) && after_ok).then_some(at)
    })
}

/// `text` with each `from` on its own changed to `to`.
fn replace_path(text: &str, from: &str, to: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for at in path_matches(text, from).collect::<Vec<_>>() {
        out.push_str(&text[last..at]);
        out.push_str(to);
        last = at + from.len();
    }
    out.push_str(&text[last..]);
    out
}

/// Each string of a JSON value with `from` changed to `to`; `whole` only
/// changes strings that are exactly `from`.
fn replace_strings(value: &mut serde_json::Value, from: &str, to: &str, whole: bool) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, item) in map.iter_mut() {
                if key == "type" && item.is_string() {
                    continue;
                }
                replace_strings(item, from, to, whole);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                replace_strings(item, from, to, whole);
            }
        }
        serde_json::Value::String(text) => {
            if whole {
                if text == from {
                    *text = to.to_owned();
                }
            } else {
                *text = replace_path(text, from, to);
            }
        }
        _ => {}
    }
}

/// A material file or texture set with its references to `from` moved.
fn rewrite_json(file: &str, text: &str, from: &str, to: &str) -> Result<String, String> {
    let mut value: serde_json::Value =
        serde_json::from_str(text).map_err(|err| format!("could not read `{file}`: {err}"))?;
    replace_strings(&mut value, from, to, true);
    if is_material_file(file) {
        let changed = serde_json::to_string(&value).map_err(|err| err.to_string())?;
        let material = parse_material(&changed).map_err(|err| format!("`{file}`: {err}"))?;
        return Ok(material_json(&material));
    }
    let mut out = serde_json::to_string_pretty(&value).map_err(|err| err.to_string())?;
    if text.ends_with('\n') {
        out.push('\n');
    }
    Ok(out)
}

/// A block script with its references to `from` moved, written as block
/// scripts are saved.
fn rewrite_blocks(text: &str, from: &str, to: &str) -> Result<String, String> {
    let mut value: serde_json::Value = serde_json::from_str(text)
        .map_err(|err| format!("could not read a block script: {err}"))?;
    replace_strings(&mut value, from, to, false);
    let script: BlockScript = serde_json::from_value(value).map_err(|err| err.to_string())?;
    serde_json::to_string_pretty(&script).map_err(|err| err.to_string())
}

fn extension(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .unwrap_or_default()
}

fn parent(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(folder, _)| folder)
}

/// Files under `assets/models` that belong to a model there (its textures
/// and buffers), not assets of their own: the Unused filter leaves them out.
pub fn is_model_part(path: &str) -> bool {
    path.starts_with(&format!("{MODELS_FOLDER}/"))
        && axcenity_assets::ModelFormat::of(Path::new(path)).is_none()
}

#[cfg(test)]
mod tests;
