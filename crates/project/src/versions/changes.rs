//! What changed between two versions, in plain words (unified Phase 72).
//!
//! World and level files are compared Instance by Instance. `world.json`
//! keeps no Instance IDs, so an Instance is matched by its path: its
//! parent's path, its class and its name, and its place among siblings
//! with the same class and name. A renamed or reparented Instance shows as
//! removed and added. Other files get a line diff.

use std::collections::BTreeMap;

use axcenity_core::ClassName;
use serde_json::{Map, Value as Json};

use crate::{LEVEL_SUFFIX, LEVELS_FOLDER, WORLD_FILE};

/// Line diffs bigger than this many line pairs show as all removed, then
/// all added, instead of taking long.
const MAX_DIFF_CELLS: usize = 4_000_000;
/// Properties that only place an Instance: changing just these is a move.
const PLACEMENT: [&str; 3] = ["Position", "Orientation", "CFrame"];
/// How many changed properties a sentence names before "and N more".
const NAMED_PROPERTIES: usize = 4;

/// Whether something is new, gone, or different.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    Added,
    Removed,
    Changed,
}

/// One Instance, or a service's own properties, that changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstanceChange {
    pub change: Change,
    pub class: String,
    pub name: String,
    /// Its parent's path by name, such as `Workspace/Map`. Empty for a
    /// service's own properties.
    pub parent: String,
    /// The properties that differ (`attributes`, `tags` and the like for the
    /// rest of what's saved). Empty unless [`Change::Changed`].
    pub properties: Vec<String>,
}

impl InstanceChange {
    /// `Moved Part "Block" in Workspace`, `Added Model "Car" to Workspace/Garage`,
    /// `Changed Lighting: ClockTime`.
    pub fn describe(&self) -> String {
        let what = format!("{} \"{}\"", self.class, self.name);
        match self.change {
            Change::Added => format!("Added {what} to {}", self.parent),
            Change::Removed => format!("Removed {what} from {}", self.parent),
            Change::Changed if self.parent.is_empty() => {
                format!("Changed {}: {}", self.name, self.property_list())
            }
            Change::Changed
                if self
                    .properties
                    .iter()
                    .all(|property| PLACEMENT.contains(&property.as_str())) =>
            {
                format!("Moved {what} in {}", self.parent)
            }
            Change::Changed if self.properties == ["Size"] => {
                format!("Resized {what} in {}", self.parent)
            }
            Change::Changed => format!(
                "Changed {what} in {}: {}",
                self.parent,
                self.property_list()
            ),
        }
    }

    fn property_list(&self) -> String {
        let shown = self.properties.len().min(NAMED_PROPERTIES);
        let mut list = self.properties[..shown].join(", ");
        if self.properties.len() > shown {
            list.push_str(&format!(" and {} more", self.properties.len() - shown));
        }
        list
    }
}

/// One line of a line diff.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Line {
    Same(String),
    Added(String),
    Removed(String),
}

/// One file that changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChangedFile {
    /// Relative to the project, such as `scripts/door.server.lua`.
    pub path: String,
    pub change: Change,
    /// For the world and levels: the Instances that changed. Empty for other
    /// files, and for a world file that couldn't be read.
    pub instances: Vec<InstanceChange>,
    /// For other files (and world files that couldn't be read): every line,
    /// marked. See [`around_changes`].
    pub lines: Vec<Line>,
}

impl ChangedFile {
    /// `world.json`, `Level "Cave"` or the path.
    pub fn title(&self) -> String {
        match level_name(&self.path) {
            Some(level) => format!("Level \"{level}\""),
            None => self.path.clone(),
        }
    }
}

/// One asset that was added, removed or changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetChange {
    pub path: String,
    pub change: Change,
}

/// What changed in a version.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Changes {
    pub files: Vec<ChangedFile>,
    pub assets: Vec<AssetChange>,
}

impl Changes {
    pub fn is_empty(&self) -> bool {
        self.files.is_empty() && self.assets.is_empty()
    }

    /// Every change as a sentence, Instances first, in file order.
    pub fn sentences(&self) -> Vec<String> {
        let mut sentences = Vec::new();
        for file in &self.files {
            if file.instances.is_empty() {
                let verb = match file.change {
                    Change::Added => "Added",
                    Change::Removed => "Removed",
                    Change::Changed => "Edited",
                };
                sentences.push(format!("{verb} {}", file.title()));
            } else {
                let level = level_name(&file.path);
                for instance in &file.instances {
                    sentences.push(match level {
                        Some(level) => format!("{} (level \"{level}\")", instance.describe()),
                        None => instance.describe(),
                    });
                }
            }
        }
        for asset in &self.assets {
            let verb = match asset.change {
                Change::Added => "Added asset",
                Change::Removed => "Removed asset",
                Change::Changed => "Changed asset",
            };
            sentences.push(format!("{verb} {}", asset.path));
        }
        sentences
    }
}

/// `Cave` for `levels/Cave.world.json`.
fn level_name(path: &str) -> Option<&str> {
    path.strip_prefix(LEVELS_FOLDER)?
        .strip_prefix('/')?
        .strip_suffix(LEVEL_SUFFIX)
        .filter(|name| !name.is_empty() && !name.contains('/'))
}

/// The lines of a diff worth showing: each change with `context` lines
/// around it. `None` stands for lines left out between them.
pub fn around_changes(lines: &[Line], context: usize) -> Vec<Option<&Line>> {
    let changed: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| !matches!(line, Line::Same(_)))
        .map(|(index, _)| index)
        .collect();
    let mut shown = Vec::new();
    let mut next = 0;
    let mut cursor = 0;
    for (index, line) in lines.iter().enumerate() {
        while next < changed.len() && changed[next] + context < index {
            next += 1;
        }
        let near = changed
            .get(next)
            .is_some_and(|&at| at.abs_diff(index) <= context);
        if near {
            if index > cursor {
                shown.push(None);
            }
            shown.push(Some(line));
            cursor = index + 1;
        }
    }
    if cursor < lines.len() && !shown.is_empty() {
        shown.push(None);
    }
    shown
}

/// One side of a comparison: each file's path and text, and the asset
/// listing (`None` when that version predates listings).
pub(super) struct Side {
    pub files: Vec<(String, String)>,
    pub assets: Option<String>,
}

pub(super) fn compare(old: &Side, new: &Side) -> Changes {
    let old_files: BTreeMap<&str, &str> = old
        .files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    let new_files: BTreeMap<&str, &str> = new
        .files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    let mut paths: Vec<&str> = old_files.keys().chain(new_files.keys()).copied().collect();
    paths.sort();
    paths.dedup();
    let mut files = Vec::new();
    for path in paths {
        let (before, after) = (old_files.get(path), new_files.get(path));
        let change = match (before, after) {
            (None, Some(_)) => Change::Added,
            (Some(_), None) => Change::Removed,
            (Some(before), Some(after)) if normalize(before) != normalize(after) => Change::Changed,
            _ => continue,
        };
        let before = before.copied().unwrap_or("");
        let after = after.copied().unwrap_or("");
        let is_world = path == WORLD_FILE || level_name(path).is_some();
        let instances = if is_world {
            world_changes(before, after).unwrap_or_default()
        } else {
            Vec::new()
        };
        let lines = if instances.is_empty() {
            diff_lines(before, after)
        } else {
            Vec::new()
        };
        if is_world && instances.is_empty() && lines.iter().all(|l| matches!(l, Line::Same(_))) {
            continue;
        }
        files.push(ChangedFile {
            path: path.to_owned(),
            change,
            instances,
            lines,
        });
    }
    let assets = match (&old.assets, &new.assets) {
        (Some(before), Some(after)) => asset_changes(before, after),
        _ => Vec::new(),
    };
    Changes { files, assets }
}

fn normalize(text: &str) -> String {
    text.replace("\r\n", "\n")
}

/// An Instance by path: its fields other than `children`, `class` and `name`.
struct Saved<'a> {
    class: &'a str,
    name: String,
    parent: String,
    fields: Map<String, Json>,
}

/// The Instance changes between two world files, or `None` when either
/// isn't readable JSON (it then gets a line diff). Text-only differences
/// (spacing, order) give an empty list.
fn world_changes(before: &str, after: &str) -> Option<Vec<InstanceChange>> {
    let parse = |text: &str| -> Option<Json> {
        if text.trim().is_empty() {
            Some(Json::Object(Map::new()))
        } else {
            serde_json::from_str(text).ok()
        }
    };
    let (before, after) = (parse(before)?, parse(after)?);
    let (before, after) = (flatten(&before)?, flatten(&after)?);
    let old_by_key: BTreeMap<&str, &Saved> = before
        .iter()
        .map(|(key, saved)| (key.as_str(), saved))
        .collect();
    let new_by_key: BTreeMap<&str, &Saved> = after
        .iter()
        .map(|(key, saved)| (key.as_str(), saved))
        .collect();
    let mut changes = Vec::new();
    for (key, old) in &before {
        match new_by_key.get(key.as_str()) {
            None => changes.extend(appeared(Change::Removed, old)),
            Some(new) => {
                let mut fields: Vec<&String> = old.fields.keys().chain(new.fields.keys()).collect();
                fields.sort();
                fields.dedup();
                let properties: Vec<String> = fields
                    .into_iter()
                    .filter(|field| {
                        !same_field(
                            new.defaults_class(),
                            field,
                            old.fields.get(*field),
                            new.fields.get(*field),
                        )
                    })
                    .cloned()
                    .collect();
                if !properties.is_empty() {
                    changes.push(instance_change(Change::Changed, new, properties));
                }
            }
        }
    }
    for (key, new) in &after {
        if !old_by_key.contains_key(key.as_str()) {
            changes.extend(appeared(Change::Added, new));
        }
    }
    Some(changes)
}

/// An Instance added or removed. A service section that appears or goes is
/// a change to its properties (if it has any), not an Instance.
fn appeared(change: Change, saved: &Saved) -> Option<InstanceChange> {
    if !saved.parent.is_empty() {
        return Some(instance_change(change, saved, Vec::new()));
    }
    let properties: Vec<String> = saved
        .fields
        .iter()
        .filter(|(field, value)| !is_default(saved.defaults_class(), field, value))
        .map(|(field, _)| field.clone())
        .collect();
    (!properties.is_empty()).then(|| instance_change(Change::Changed, saved, properties))
}

impl Saved<'_> {
    /// The class whose defaults its properties fall back to: a service's
    /// own class for a service's properties.
    fn defaults_class(&self) -> &str {
        if self.parent.is_empty() {
            &self.name
        } else {
            self.class
        }
    }
}

/// Whether a field is the same on both sides. One that's only written on one
/// side is the same when it holds the default there: older or hand-written
/// files leave defaults out, and the editor writes every property.
fn same_field(class: &str, field: &str, old: Option<&Json>, new: Option<&Json>) -> bool {
    match (old, new) {
        (Some(old), Some(new)) => old == new,
        (Some(one), None) | (None, Some(one)) => is_default(class, field, one),
        (None, None) => true,
    }
}

fn is_default(class: &str, field: &str, json: &Json) -> bool {
    let empty = match json {
        Json::Array(items) => items.is_empty(),
        Json::Object(fields) => fields.is_empty(),
        Json::Bool(on) => !on,
        _ => false,
    };
    empty
        || ClassName::from_name(class)
            .and_then(|class| {
                class
                    .default_properties()
                    .into_iter()
                    .find(|(name, _)| *name == field)
            })
            .and_then(|(_, default)| {
                crate::world_file::json_to_value(json, default.kind())
                    .ok()
                    .map(|value| value == default)
            })
            .unwrap_or(false)
}

fn instance_change(change: Change, saved: &Saved, properties: Vec<String>) -> InstanceChange {
    InstanceChange {
        change,
        class: saved.class.to_owned(),
        name: saved.name.clone(),
        parent: saved.parent.clone(),
        properties,
    }
}

/// Every Instance in a world file by its path key, in file order. A
/// service's own properties are an entry too, keyed by the service.
fn flatten(world: &Json) -> Option<Vec<(String, Saved<'_>)>> {
    let world = world.as_object()?;
    let mut out = Vec::new();
    for (section, node) in world {
        let Some(node) = node.as_object() else {
            continue;
        };
        let service = service_name(section);
        let properties = node
            .get("properties")
            .and_then(Json::as_object)
            .cloned()
            .unwrap_or_default();
        out.push((
            service.clone(),
            Saved {
                class: "Service",
                name: service.clone(),
                parent: String::new(),
                // Each property is a field, so a change names it.
                fields: properties,
            },
        ));
        flatten_children(node, &service, &service, &mut out);
    }
    Some(out)
}

fn flatten_children<'a>(
    node: &'a Map<String, Json>,
    key: &str,
    path: &str,
    out: &mut Vec<(String, Saved<'a>)>,
) {
    let Some(children) = node.get("children").and_then(Json::as_array) else {
        return;
    };
    let mut seen: BTreeMap<(&str, String), usize> = BTreeMap::new();
    for child in children {
        let Some(child) = child.as_object() else {
            continue;
        };
        let class = child
            .get("class")
            .and_then(Json::as_str)
            .unwrap_or("Instance");
        let name = child
            .get("name")
            .and_then(Json::as_str)
            .unwrap_or(class)
            .to_owned();
        let nth = seen.entry((class, name.clone())).or_default();
        let child_key = format!("{key}/{class}:{name}#{nth}");
        *nth += 1;
        let mut fields = Map::new();
        for (field, value) in child {
            match field.as_str() {
                "class" | "name" | "children" => {}
                "properties" => {
                    if let Some(properties) = value.as_object() {
                        fields.extend(properties.clone());
                    }
                }
                _ => {
                    fields.insert(field.clone(), value.clone());
                }
            }
        }
        out.push((
            child_key.clone(),
            Saved {
                class,
                name: name.clone(),
                parent: path.to_owned(),
                fields,
            },
        ));
        flatten_children(child, &child_key, &format!("{path}/{name}"), out);
    }
}

/// `replicated_storage` as `ReplicatedStorage`.
fn service_name(section: &str) -> String {
    section
        .split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect::<String>(),
                None => String::new(),
            }
        })
        .collect()
}

/// Every line of `before` and `after`, marked same, removed or added.
fn diff_lines(before: &str, after: &str) -> Vec<Line> {
    let (before, after) = (normalize(before), normalize(after));
    let old: Vec<&str> = before.lines().collect();
    let new: Vec<&str> = after.lines().collect();
    let prefix = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
    let suffix = old[prefix..]
        .iter()
        .rev()
        .zip(new[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let (a, b) = (
        &old[prefix..old.len() - suffix],
        &new[prefix..new.len() - suffix],
    );
    let mut lines: Vec<Line> = old[..prefix]
        .iter()
        .map(|line| Line::Same((*line).to_owned()))
        .collect();
    if a.len().saturating_mul(b.len()) > MAX_DIFF_CELLS {
        lines.extend(a.iter().map(|line| Line::Removed((*line).to_owned())));
        lines.extend(b.iter().map(|line| Line::Added((*line).to_owned())));
    } else {
        lines.extend(lcs_diff(a, b));
    }
    lines.extend(
        old[old.len() - suffix..]
            .iter()
            .map(|line| Line::Same((*line).to_owned())),
    );
    lines
}

/// A longest-common-subsequence diff: `table[i][j]` is the length of the
/// longest common run of `a[i..]` and `b[j..]`.
fn lcs_diff(a: &[&str], b: &[&str]) -> Vec<Line> {
    let width = b.len() + 1;
    let mut table = vec![0u32; (a.len() + 1) * width];
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            table[i * width + j] = if a[i] == b[j] {
                table[(i + 1) * width + j + 1] + 1
            } else {
                table[(i + 1) * width + j].max(table[i * width + j + 1])
            };
        }
    }
    let (mut i, mut j) = (0, 0);
    let mut lines = Vec::new();
    while i < a.len() || j < b.len() {
        if i < a.len() && j < b.len() && a[i] == b[j] {
            lines.push(Line::Same(a[i].to_owned()));
            i += 1;
            j += 1;
        } else if i < a.len()
            && (j == b.len() || table[(i + 1) * width + j] >= table[i * width + j + 1])
        {
            // Removed lines before added ones, as diffs usually show them.
            lines.push(Line::Removed(a[i].to_owned()));
            i += 1;
        } else {
            lines.push(Line::Added(b[j].to_owned()));
            j += 1;
        }
    }
    lines
}

/// Which assets differ between two listings (size, time and path per line).
fn asset_changes(before: &str, after: &str) -> Vec<AssetChange> {
    let read = |listing: &str| -> BTreeMap<String, String> {
        listing
            .lines()
            .filter_map(|line| {
                let (stamp, path) = line.rsplit_once('\t')?;
                Some((path.to_owned(), stamp.to_owned()))
            })
            .collect()
    };
    let (before, after) = (read(before), read(after));
    let mut changes = Vec::new();
    for (path, stamp) in &before {
        match after.get(path) {
            None => changes.push(AssetChange {
                path: path.clone(),
                change: Change::Removed,
            }),
            Some(new) if new != stamp => changes.push(AssetChange {
                path: path.clone(),
                change: Change::Changed,
            }),
            Some(_) => {}
        }
    }
    for path in after.keys() {
        if !before.contains_key(path) {
            changes.push(AssetChange {
                path: path.clone(),
                change: Change::Added,
            });
        }
    }
    changes.sort_by(|a, b| a.path.cmp(&b.path));
    changes
}
