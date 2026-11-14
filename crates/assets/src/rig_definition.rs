//! Rig definitions: a project's own bone maps for skeletons whose bone
//! names say too little (or the wrong thing) for retargeting to match them
//! to humanoid roles by name.
//!
//! A definition (`assets/rigs/<name>.rig.json`) names, for each role it
//! sets, the bone that plays it:
//!
//! ```json
//! {
//!   "format_version": 1,
//!   "bones": {
//!     "Hips": "root_pelvis",
//!     "LeftLowerArm": "arm_lo.L",
//!     "LeftIndexProximal": "finger_a1.L",
//!     "Chest": ""
//!   }
//! }
//! ```
//!
//! An empty name says no bone plays that role (so name matching doesn't
//! guess one). Roles it leaves out are still matched by name. A definition
//! is reusable: it applies to every skeleton that has all the bones it
//! names, whichever model or animation file the skeleton came from, so one
//! definition covers every character exported from the same rig. When
//! several fit, the one naming the most bones wins (then the first by
//! path). The role names are `anim`'s (`HumanBone`); this crate only reads
//! and writes the files, and `anim` checks them (`anim::check_rig`).

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// Where a project keeps its rig definitions and retarget profiles.
pub const RIGS_FOLDER: &str = "assets/rigs";
/// What a rig definition's file name ends with.
pub const RIG_EXTENSION: &str = ".rig.json";

const FORMAT_VERSION: u32 = 1;

/// One rig definition file.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RigDefinition {
    pub format_version: u32,
    /// Role name (`Hips`, `LeftIndexProximal` …) to the bone playing it,
    /// or `""` for none. Sorted, so saving gives the same text.
    #[serde(default)]
    pub bones: BTreeMap<String, String>,
}

impl RigDefinition {
    /// An empty definition, to fill in.
    pub fn new() -> Self {
        Self {
            format_version: FORMAT_VERSION,
            bones: BTreeMap::new(),
        }
    }

    /// Reads a definition, saying what's wrong with one that can't be.
    pub fn from_json(text: &str) -> Result<Self, String> {
        let definition: RigDefinition =
            serde_json::from_str(text).map_err(|err| err.to_string())?;
        if definition.format_version != FORMAT_VERSION {
            return Err(format!(
                "it is in rig definition format {}, and only {FORMAT_VERSION} can be read",
                definition.format_version
            ));
        }
        Ok(definition)
    }

    /// Pretty JSON, the same every time for the same definition.
    pub fn to_json(&self) -> String {
        let mut text = serde_json::to_string_pretty(self).expect("plain data");
        text.push('\n');
        text
    }

    /// The bone names it maps a role to (leaving out roles set to none).
    pub fn named_bones(&self) -> impl Iterator<Item = &str> {
        self.bones
            .values()
            .map(String::as_str)
            .filter(|name| !name.is_empty())
    }

    /// Whether it fits a skeleton with bones named `names`: it names at
    /// least one bone, and the skeleton has every bone it names.
    pub fn fits<'a>(&self, names: impl IntoIterator<Item = &'a str> + Clone) -> bool {
        let mut named = self.named_bones().peekable();
        named.peek().is_some() && named.all(|bone| names.clone().into_iter().any(|n| n == bone))
    }
}

/// Every rig definition in a project, by path, in path order, and the
/// files that couldn't be read (one line each).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RigDefinitions {
    pub definitions: Vec<(String, RigDefinition)>,
    pub problems: Vec<String>,
}

impl RigDefinitions {
    /// The definition that fits a skeleton with bones named `names` best:
    /// the fitting one naming the most bones, then the first by path.
    pub fn for_skeleton<'a>(
        &self,
        names: impl IntoIterator<Item = &'a str> + Clone,
    ) -> Option<(&str, &RigDefinition)> {
        let mut best: Option<(&str, &RigDefinition, usize)> = None;
        for (path, definition) in &self.definitions {
            if !definition.fits(names.clone()) {
                continue;
            }
            let count = definition.named_bones().count();
            if best.is_none_or(|(_, _, most)| count > most) {
                best = Some((path, definition, count));
            }
        }
        best.map(|(path, definition, _)| (path, definition))
    }
}

/// Whether `path` is a rig definition's.
pub fn is_rig_definition(path: &str) -> bool {
    path.to_ascii_lowercase().ends_with(RIG_EXTENSION)
}

/// Reads every `*.rig.json` under the project's `assets/rigs/` (and the
/// folders inside it). A project without the folder has none.
pub fn read_rig_definitions(project: &Path) -> RigDefinitions {
    let mut found = RigDefinitions::default();
    let mut files = Vec::new();
    collect(&project.join(RIGS_FOLDER), RIGS_FOLDER, &mut files);
    files.sort();
    for path in files {
        match fs::read_to_string(project.join(&path))
            .map_err(|err| err.to_string())
            .and_then(|text| RigDefinition::from_json(&text))
        {
            Ok(definition) => found.definitions.push((path, definition)),
            Err(why) => found
                .problems
                .push(format!("The rig definition `{path}` can't be read: {why}")),
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
        } else if is_rig_definition(&name) {
            files.push(path);
        }
    }
}

/// Writes `definition` to `path` inside `project`, making its folder.
pub fn write_rig_definition(
    project: &Path,
    path: &str,
    definition: &RigDefinition,
) -> std::io::Result<()> {
    let file = project.join(path);
    if let Some(folder) = file.parent() {
        fs::create_dir_all(folder)?;
    }
    crate::cooked::write_whole(&file, definition.to_json().as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn definition(pairs: &[(&str, &str)]) -> RigDefinition {
        let mut definition = RigDefinition::new();
        for (role, bone) in pairs {
            definition.bones.insert((*role).into(), (*bone).into());
        }
        definition
    }

    #[test]
    fn a_definition_round_trips_and_refuses_unknown_fields_and_formats() {
        let made = definition(&[("Hips", "root_pelvis"), ("Chest", "")]);
        let text = made.to_json();
        assert_eq!(RigDefinition::from_json(&text).unwrap(), made);
        assert_eq!(RigDefinition::from_json(&text).unwrap().to_json(), text);
        assert!(RigDefinition::from_json(r#"{"format_version":1,"bone":{}}"#).is_err());
        let newer = RigDefinition::from_json(r#"{"format_version":2}"#).unwrap_err();
        assert!(newer.contains("format 2"), "{newer}");
    }

    #[test]
    fn the_definition_naming_the_most_of_a_skeletons_bones_fits_it() {
        let found = RigDefinitions {
            definitions: vec![
                (
                    "assets/rigs/a.rig.json".into(),
                    definition(&[("Hips", "pelvis")]),
                ),
                (
                    "assets/rigs/b.rig.json".into(),
                    definition(&[("Hips", "pelvis"), ("Head", "skull"), ("Chest", "")]),
                ),
                (
                    "assets/rigs/c.rig.json".into(),
                    definition(&[("Hips", "pelvis"), ("Head", "noggin")]),
                ),
                (
                    "assets/rigs/empty.rig.json".into(),
                    definition(&[("Hips", "")]),
                ),
            ],
            problems: Vec::new(),
        };
        let names = ["pelvis", "skull", "spine"];
        let (path, _) = found.for_skeleton(names).unwrap();
        assert_eq!(path, "assets/rigs/b.rig.json");
        // Only `a` fits a skeleton without a skull; an empty one never does.
        let (path, _) = found.for_skeleton(["pelvis"]).unwrap();
        assert_eq!(path, "assets/rigs/a.rig.json");
        assert!(found.for_skeleton(["root"]).is_none());
    }

    #[test]
    fn definitions_are_read_from_the_rigs_folder_and_broken_ones_reported() {
        let project =
            std::env::temp_dir().join(format!("axcenity-rig-definitions-{}", std::process::id()));
        let _ = fs::remove_dir_all(&project);
        let made = definition(&[("Hips", "pelvis")]);
        write_rig_definition(&project, "assets/rigs/humans/hero.rig.json", &made).unwrap();
        fs::write(project.join("assets/rigs/broken.rig.json"), "{").unwrap();
        fs::write(project.join("assets/rigs/notes.txt"), "not a rig").unwrap();
        let found = read_rig_definitions(&project);
        assert_eq!(
            found.definitions,
            vec![("assets/rigs/humans/hero.rig.json".to_owned(), made)]
        );
        assert_eq!(found.problems.len(), 1);
        assert!(found.problems[0].contains("broken.rig.json"));
        assert_eq!(
            read_rig_definitions(&project.join("nothing")),
            RigDefinitions::default()
        );
        let _ = fs::remove_dir_all(&project);
    }
}
