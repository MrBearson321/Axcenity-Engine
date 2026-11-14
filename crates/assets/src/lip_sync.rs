//! Lip sync files (facial animation F5): when a line's mouth shapes come
//! (`*.lipsync.json`), and what each of the 12 visemes does to a face's
//! controls (`*.visemes.json`). `anim` plays them on a face's Dialogue
//! layer; the viseme and control names are `core`'s.
//!
//! A lip sync (anywhere in the project, often beside its sound):
//!
//! ```json
//! {
//!   "format_version": 1,
//!   "sound": "assets/sounds/hello.ogg",
//!   "duration": 1.2,
//!   "visemes": [ { "time": 0.0, "viseme": "Silence" },
//!                { "time": 0.08, "viseme": "E", "weight": 0.8 } ]
//! }
//! ```
//!
//! Each viseme lasts until the next one; `weight` (1 if left out) is how
//! wide it's said. A viseme map (`assets/faces/visemes/<name>.visemes.json`;
//! the built-in one is `anim`'s):
//!
//! ```json
//! {
//!   "format_version": 1,
//!   "jaw_share": 0.4,
//!   "visemes": { "AI": { "Mouth_Open": 0.7, "Mouth_Wide": 0.3 } }
//! }
//! ```

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// What a lip sync's file name ends with.
pub const LIP_SYNC_EXTENSION: &str = ".lipsync.json";
/// Where a project keeps its viseme maps.
pub const VISEMES_FOLDER: &str = "assets/faces/visemes";
/// What a viseme map's file name ends with.
pub const VISEMES_EXTENSION: &str = ".visemes.json";

const FORMAT_VERSION: u32 = 1;

fn one() -> f32 {
    1.0
}

fn is_one(value: &f32) -> bool {
    *value == 1.0
}

/// One mouth shape in a lip sync, from `time` until the next.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VisemeKey {
    /// Seconds from the start.
    pub time: f64,
    /// One of the 12 visemes (`core::face::VISEMES`).
    pub viseme: String,
    /// How wide it's said, 0 to 1.
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub weight: f32,
}

/// A line's mouth shapes over time.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LipSync {
    pub format_version: u32,
    /// The sound it was made for, if any (for the editor and tools).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub sound: String,
    /// Seconds; the mouth rests after.
    pub duration: f64,
    /// In time order.
    #[serde(default)]
    pub visemes: Vec<VisemeKey>,
}

impl Default for LipSync {
    fn default() -> Self {
        Self {
            format_version: FORMAT_VERSION,
            sound: String::new(),
            duration: 0.0,
            visemes: Vec::new(),
        }
    }
}

impl LipSync {
    /// Reads a lip sync, saying what's wrong with one that can't be. Keys
    /// are put in time order.
    pub fn from_json(text: &str) -> Result<Self, String> {
        let mut read: LipSync = serde_json::from_str(text).map_err(|err| err.to_string())?;
        if read.format_version != FORMAT_VERSION {
            return Err(format!(
                "it is in lip sync format {}, and only {FORMAT_VERSION} can be read",
                read.format_version
            ));
        }
        if !(read.duration.is_finite() && read.duration >= 0.0) {
            return Err("its duration must be 0 seconds or more".to_owned());
        }
        if let Some(key) = read
            .visemes
            .iter()
            .find(|key| !key.time.is_finite() || !key.weight.is_finite())
        {
            return Err(format!(
                "the `{}` key at {} has a time or weight that isn't a number",
                key.viseme, key.time
            ));
        }
        read.visemes.sort_by(|a, b| a.time.total_cmp(&b.time));
        Ok(read)
    }

    /// Pretty JSON, the same every time for the same lip sync.
    pub fn to_json(&self) -> String {
        let mut text = serde_json::to_string_pretty(self).expect("plain data");
        text.push('\n');
        text
    }
}

/// What each viseme does to a face's controls.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VisemeMap {
    pub format_version: u32,
    /// Its name; the file's when empty.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// How much of each viseme's `Mouth_Open` the jaw carries instead, as
    /// `Jaw_Open` (0: none, 1: all of it).
    #[serde(default)]
    pub jaw_share: f32,
    /// Each viseme's control values, 0 to 1. Controls a viseme leaves out
    /// but another sets are 0 while it's said.
    #[serde(default)]
    pub visemes: BTreeMap<String, BTreeMap<String, f32>>,
}

impl Default for VisemeMap {
    fn default() -> Self {
        Self {
            format_version: FORMAT_VERSION,
            name: String::new(),
            jaw_share: 0.0,
            visemes: BTreeMap::new(),
        }
    }
}

impl VisemeMap {
    /// Reads a viseme map, saying what's wrong with one that can't be.
    pub fn from_json(text: &str) -> Result<Self, String> {
        let read: VisemeMap = serde_json::from_str(text).map_err(|err| err.to_string())?;
        if read.format_version != FORMAT_VERSION {
            return Err(format!(
                "it is in viseme map format {}, and only {FORMAT_VERSION} can be read",
                read.format_version
            ));
        }
        Ok(read)
    }

    /// Pretty JSON, the same every time for the same map.
    pub fn to_json(&self) -> String {
        let mut text = serde_json::to_string_pretty(self).expect("plain data");
        text.push('\n');
        text
    }
}

/// The name a viseme map at `path` goes by: its `name`, or its file's.
pub fn viseme_map_name<'a>(path: &'a str, map: &'a VisemeMap) -> &'a str {
    if !map.name.is_empty() {
        return &map.name;
    }
    let file = path.rsplit(['/', '\\']).next().unwrap_or(path);
    file.get(..file.len().saturating_sub(VISEMES_EXTENSION.len()))
        .filter(|_| is_viseme_map(file))
        .unwrap_or(file)
}

/// Every viseme map in a project, by path, and the files that couldn't be
/// read (one line each).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VisemeMaps {
    pub maps: Vec<(String, VisemeMap)>,
    pub problems: Vec<String>,
}

impl VisemeMaps {
    /// The map named `name` (any case), or at the path `name`.
    pub fn find(&self, name: &str) -> Option<(&str, &VisemeMap)> {
        self.maps
            .iter()
            .find(|(path, map)| {
                path == name || viseme_map_name(path, map).eq_ignore_ascii_case(name)
            })
            .map(|(path, map)| (path.as_str(), map))
    }
}

/// Whether `path` is a lip sync's.
pub fn is_lip_sync(path: &str) -> bool {
    path.to_ascii_lowercase().ends_with(LIP_SYNC_EXTENSION)
}

/// Whether `path` is a viseme map's.
pub fn is_viseme_map(path: &str) -> bool {
    path.to_ascii_lowercase().ends_with(VISEMES_EXTENSION)
}

/// Reads the lip sync at `path` inside `project`.
pub fn read_lip_sync(project: &Path, path: &str) -> Result<LipSync, String> {
    let relative = crate::safe_path(path)
        .ok_or_else(|| format!("`{path}` isn't inside the project folder"))?;
    let file = project.join(relative);
    let text = fs::read_to_string(&file).map_err(|err| match err.kind() {
        std::io::ErrorKind::NotFound => format!("there's no file `{path}`"),
        _ => format!("`{path}` can't be read: {err}"),
    })?;
    LipSync::from_json(&text).map_err(|why| format!("`{path}` can't be read: {why}"))
}

/// Writes `lip_sync` to `path` inside `project`, making its folder.
pub fn write_lip_sync(project: &Path, path: &str, lip_sync: &LipSync) -> std::io::Result<()> {
    write(project, path, lip_sync.to_json())
}

/// Writes `map` to `path` inside `project`, making its folder.
pub fn write_viseme_map(project: &Path, path: &str, map: &VisemeMap) -> std::io::Result<()> {
    write(project, path, map.to_json())
}

fn write(project: &Path, path: &str, text: String) -> std::io::Result<()> {
    let file = project.join(path);
    if let Some(folder) = file.parent() {
        fs::create_dir_all(folder)?;
    }
    crate::cooked::write_whole(&file, text.as_bytes())
}

/// Reads every `*.visemes.json` under the project's `assets/faces/visemes/`
/// (and the folders inside it).
pub fn read_viseme_maps(project: &Path) -> VisemeMaps {
    let mut found = VisemeMaps::default();
    let mut files = Vec::new();
    collect(&project.join(VISEMES_FOLDER), VISEMES_FOLDER, &mut files);
    files.sort();
    for path in files {
        match fs::read_to_string(project.join(&path))
            .map_err(|err| err.to_string())
            .and_then(|text| VisemeMap::from_json(&text))
        {
            Ok(map) => found.maps.push((path, map)),
            Err(why) => found
                .problems
                .push(format!("The viseme map `{path}` can't be read: {why}")),
        }
    }
    found
}

fn collect(folder: &Path, relative: &str, files: &mut Vec<String>) {
    let Ok(entries) = fs::read_dir(folder) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = format!("{relative}/{name}");
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            collect(&entry.path(), &path, files);
        } else if is_viseme_map(&name) {
            files.push(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lip_syncs_and_viseme_maps_round_trip_and_say_whats_wrong() {
        let text = r#"{ "format_version": 1, "duration": 1,
            "visemes": [ { "time": 0.5, "viseme": "O" }, { "time": 0, "viseme": "Silence", "weight": 0.5 } ] }"#;
        let read = LipSync::from_json(text).unwrap();
        assert_eq!(read.visemes[0].viseme, "Silence", "sorted by time");
        assert_eq!(read.visemes[1].weight, 1.0);
        assert_eq!(LipSync::from_json(&read.to_json()).unwrap(), read);
        assert!(!read.to_json().contains("\"weight\": 1.0"));
        assert!(LipSync::from_json(r#"{"format_version":1,"duration":-1}"#).is_err());
        assert!(LipSync::from_json(r#"{"format_version":2,"duration":1}"#).is_err());

        let map = VisemeMap::from_json(
            r#"{ "format_version": 1, "jaw_share": 0.5, "visemes": { "O": { "Mouth_Pucker": 0.6 } } }"#,
        )
        .unwrap();
        assert_eq!(VisemeMap::from_json(&map.to_json()).unwrap(), map);
        assert_eq!(
            viseme_map_name("assets/faces/visemes/Hero.visemes.json", &map),
            "Hero"
        );

        let project = std::env::temp_dir().join(format!("axcenity-lipsync-{}", std::process::id()));
        let _ = fs::remove_dir_all(&project);
        write_viseme_map(
            &project,
            "assets/faces/visemes/fr/French.visemes.json",
            &map,
        )
        .unwrap();
        fs::write(project.join("assets/faces/visemes/bad.visemes.json"), "{").unwrap();
        let maps = read_viseme_maps(&project);
        assert!(maps.find("french").is_some());
        assert_eq!(maps.problems.len(), 1);
        write_lip_sync(&project, "assets/voice/hi.lipsync.json", &read).unwrap();
        assert_eq!(
            read_lip_sync(&project, "assets/voice/hi.lipsync.json").unwrap(),
            read
        );
        let missing = read_lip_sync(&project, "assets/voice/no.lipsync.json").unwrap_err();
        assert!(missing.contains("there's no file"), "{missing}");
        assert!(read_lip_sync(&project, "../outside.lipsync.json").is_err());
        let _ = fs::remove_dir_all(&project);
    }
}
