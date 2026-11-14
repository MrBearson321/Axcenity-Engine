//! Face profiles: how a model's blend shapes and bones play the standard
//! facial controls (facial animation F3).
//!
//! A profile (`assets/faces/<name>.face.json`) names, for each control it
//! maps, the blend shape or shapes it drives and by how much:
//!
//! ```json
//! {
//!   "format_version": 1,
//!   "name": "Hero",
//!   "controls": {
//!     "Blink_Left": "EyeClose_L",
//!     "Brow_Raise_Left": [
//!       { "shape": "BrowOuterUp_L" },
//!       { "shape": "BrowInnerUp", "weight": 0.5 }
//!     ]
//!   },
//!   "bones": { "Jaw": "jaw_ctrl", "LeftEye": "eye.L", "RightEye": "" },
//!   "limits": { "look_sideways": 35 }
//! }
//! ```
//!
//! `bones` names the eye and jaw bones when their names don't say so (an
//! empty name: the model has none, so none is guessed); `limits` are in
//! degrees. A profile fits the model whose blend shapes it names the most
//! of. The control names and the built-in profiles are `anim`'s
//! (`anim::face`); this crate only reads and writes the files.

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::Path;

use serde::de::{MapAccess, Visitor};
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Where a project keeps its face profiles (and, later, expressions and
/// viseme maps).
pub const FACES_FOLDER: &str = "assets/faces";
/// What a face profile's file name ends with.
pub const FACE_EXTENSION: &str = ".face.json";

const FORMAT_VERSION: u32 = 1;

/// One face profile file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FaceProfile {
    pub format_version: u32,
    /// What the editor calls it; the file's name when empty.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// Each control it maps and the shapes that control drives, in the
    /// file's order. A control named twice is kept twice, so checking the
    /// profile can say so.
    #[serde(
        default,
        serialize_with = "write_controls",
        deserialize_with = "read_controls"
    )]
    pub controls: Vec<(String, Vec<ShapeWeight>)>,
    /// `Jaw`, `LeftEye` and `RightEye` to the bone playing it, or `""`
    /// for none. Ones left out are found by name.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub bones: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "FaceLimits::is_default")]
    pub limits: FaceLimits,
}

/// One blend shape a control drives, and how much of it the control's
/// full value gives.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShapeWeight {
    pub shape: String,
    #[serde(default = "one")]
    pub weight: f32,
}

impl ShapeWeight {
    pub fn new(shape: impl Into<String>, weight: f32) -> Self {
        Self {
            shape: shape.into(),
            weight,
        }
    }
}

fn one() -> f32 {
    1.0
}

/// How far the eye and jaw bones turn at a control's full value, in
/// degrees.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FaceLimits {
    pub look_sideways: f32,
    pub look_up: f32,
    pub look_down: f32,
    pub jaw_open: f32,
    pub jaw_sideways: f32,
}

impl Default for FaceLimits {
    fn default() -> Self {
        Self {
            look_sideways: 30.0,
            look_up: 25.0,
            look_down: 20.0,
            jaw_open: 25.0,
            jaw_sideways: 8.0,
        }
    }
}

impl FaceLimits {
    fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

impl Default for FaceProfile {
    fn default() -> Self {
        Self::new()
    }
}

impl FaceProfile {
    /// An empty profile, to fill in.
    pub fn new() -> Self {
        Self {
            format_version: FORMAT_VERSION,
            name: String::new(),
            controls: Vec::new(),
            bones: BTreeMap::new(),
            limits: FaceLimits::default(),
        }
    }

    /// Reads a profile, saying what's wrong with one that can't be.
    pub fn from_json(text: &str) -> Result<Self, String> {
        let profile: FaceProfile = serde_json::from_str(text).map_err(|err| err.to_string())?;
        if profile.format_version != FORMAT_VERSION {
            return Err(format!(
                "it is in face profile format {}, and only {FORMAT_VERSION} can be read",
                profile.format_version
            ));
        }
        Ok(profile)
    }

    /// Pretty JSON, the same every time for the same profile.
    pub fn to_json(&self) -> String {
        let mut text = serde_json::to_string_pretty(self).expect("plain data");
        text.push('\n');
        text
    }

    /// Every blend shape name it drives (some more than once).
    pub fn shapes(&self) -> impl Iterator<Item = &str> {
        self.controls
            .iter()
            .flat_map(|(_, shapes)| shapes.iter().map(|drive| drive.shape.as_str()))
    }

    /// How many of `names` (a model's blend shapes) it drives.
    pub fn fit<'a>(&self, names: impl IntoIterator<Item = &'a str>) -> usize {
        names
            .into_iter()
            .filter(|name| self.shapes().any(|shape| shape == *name))
            .count()
    }
}

/// A control with one full-weight shape is written as just the shape's
/// name.
fn write_controls<S: Serializer>(
    controls: &[(String, Vec<ShapeWeight>)],
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let mut map = serializer.serialize_map(Some(controls.len()))?;
    for (control, shapes) in controls {
        match shapes.as_slice() {
            [only] if only.weight == 1.0 => map.serialize_entry(control, &only.shape)?,
            _ => map.serialize_entry(control, shapes)?,
        }
    }
    map.end()
}

fn read_controls<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<(String, Vec<ShapeWeight>)>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Drives {
        One(String),
        Many(Vec<ShapeWeight>),
    }

    struct Controls;

    impl<'de> Visitor<'de> for Controls {
        type Value = Vec<(String, Vec<ShapeWeight>)>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("controls, each a shape's name or a list of { shape, weight }")
        }

        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
            let mut controls = Vec::new();
            while let Some((control, drives)) = map.next_entry::<String, Drives>()? {
                let shapes = match drives {
                    Drives::One(shape) => vec![ShapeWeight::new(shape, 1.0)],
                    Drives::Many(shapes) => shapes,
                };
                controls.push((control, shapes));
            }
            Ok(controls)
        }
    }

    deserializer.deserialize_map(Controls)
}

/// Every face profile in a project, by path, in path order, and the files
/// that couldn't be read (one line each).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FaceProfiles {
    pub profiles: Vec<(String, FaceProfile)>,
    pub problems: Vec<String>,
}

/// Whether `path` is a face profile's.
pub fn is_face_profile(path: &str) -> bool {
    path.to_ascii_lowercase().ends_with(FACE_EXTENSION)
}

/// Reads every `*.face.json` under the project's `assets/faces/` (and the
/// folders inside it). A project without the folder has none.
pub fn read_face_profiles(project: &Path) -> FaceProfiles {
    let mut found = FaceProfiles::default();
    let mut files = Vec::new();
    collect(&project.join(FACES_FOLDER), FACES_FOLDER, &mut files);
    files.sort();
    for path in files {
        match fs::read_to_string(project.join(&path))
            .map_err(|err| err.to_string())
            .and_then(|text| FaceProfile::from_json(&text))
        {
            Ok(profile) => found.profiles.push((path, profile)),
            Err(why) => found
                .problems
                .push(format!("The face profile `{path}` can't be read: {why}")),
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
        } else if is_face_profile(&name) {
            files.push(path);
        }
    }
}

/// Writes `profile` to `path` inside `project`, making its folder.
pub fn write_face_profile(
    project: &Path,
    path: &str,
    profile: &FaceProfile,
) -> std::io::Result<()> {
    let file = project.join(path);
    if let Some(folder) = file.parent() {
        fs::create_dir_all(folder)?;
    }
    crate::cooked::write_whole(&file, profile.to_json().as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_profile_round_trips_its_short_and_long_forms_and_keeps_repeats() {
        let text = r#"{
  "format_version": 1,
  "name": "Hero",
  "controls": {
    "Blink_Left": "EyeClose_L",
    "Brow_Raise_Left": [
      { "shape": "BrowOuterUp_L" },
      { "shape": "BrowInnerUp", "weight": 0.5 }
    ],
    "Blink_Left": "Blink2"
  },
  "bones": { "Jaw": "jaw_ctrl", "RightEye": "" },
  "limits": { "look_sideways": 35 }
}"#;
        let profile = FaceProfile::from_json(text).unwrap();
        assert_eq!(profile.controls.len(), 3, "Blink_Left is kept twice");
        assert_eq!(profile.controls[0].0, "Blink_Left");
        assert_eq!(
            profile.controls[1].1,
            vec![
                ShapeWeight::new("BrowOuterUp_L", 1.0),
                ShapeWeight::new("BrowInnerUp", 0.5)
            ]
        );
        assert_eq!(profile.limits.look_sideways, 35.0);
        assert_eq!(profile.limits.look_up, 25.0, "the rest keep their defaults");
        assert_eq!(profile.bones["RightEye"], "");
        let written = profile.to_json();
        assert!(
            written.contains(r#""Blink_Left": "EyeClose_L""#),
            "{written}"
        );
        assert_eq!(FaceProfile::from_json(&written).unwrap(), profile);
        assert_eq!(FaceProfile::from_json(&written).unwrap().to_json(), written);
        assert_eq!(profile.fit(["EyeClose_L", "BrowInnerUp", "Other"]), 2);
        assert!(FaceProfile::from_json(r#"{"format_version":1,"control":{}}"#).is_err());
        let newer = FaceProfile::from_json(r#"{"format_version":2}"#).unwrap_err();
        assert!(newer.contains("format 2"), "{newer}");
        let bad = FaceProfile::from_json(r#"{"format_version":1,"controls":{"A":3}}"#);
        assert!(bad.is_err());
    }

    #[test]
    fn profiles_are_read_from_the_faces_folder_and_broken_ones_reported() {
        let project =
            std::env::temp_dir().join(format!("axcenity-face-profiles-{}", std::process::id()));
        let _ = fs::remove_dir_all(&project);
        let mut made = FaceProfile::new();
        made.controls
            .push(("Blink_Left".into(), vec![ShapeWeight::new("blink", 1.0)]));
        write_face_profile(&project, "assets/faces/heroes/hero.face.json", &made).unwrap();
        fs::write(project.join("assets/faces/broken.face.json"), "{").unwrap();
        fs::write(project.join("assets/faces/notes.txt"), "not a face").unwrap();
        let found = read_face_profiles(&project);
        assert_eq!(
            found.profiles,
            vec![("assets/faces/heroes/hero.face.json".to_owned(), made)]
        );
        assert_eq!(found.problems.len(), 1);
        assert!(found.problems[0].contains("broken.face.json"));
        assert_eq!(
            read_face_profiles(&project.join("nothing")),
            FaceProfiles::default()
        );
        let _ = fs::remove_dir_all(&project);
    }
}
