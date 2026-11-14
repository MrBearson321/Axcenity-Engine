//! Facial expressions (facial animation F4): a named set of facial control
//! values that `FaceControls:SetExpression` blends a face toward.
//!
//! An expression (`assets/faces/expressions/<Name>.expression.json`):
//!
//! ```json
//! {
//!   "format_version": 1,
//!   "intensity": 1.0,
//!   "transition": 0.3,
//!   "controls": { "Mouth_Smile_Left": 0.8, "Mouth_Smile_Right": 0.8 }
//! }
//! ```
//!
//! Its name is the file's (`Happy`), unless `name` says otherwise. Controls
//! it leaves out are left to the layers under it. `intensity` scales every
//! value by default; `transition` is the seconds it takes to blend in and
//! out by default. The control names are `core`'s; `anim` checks and plays
//! them.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// Where a project keeps its expressions.
pub const EXPRESSIONS_FOLDER: &str = "assets/faces/expressions";
/// What an expression's file name ends with.
pub const EXPRESSION_EXTENSION: &str = ".expression.json";

const FORMAT_VERSION: u32 = 1;

/// One expression file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FaceExpression {
    pub format_version: u32,
    /// Its name; the file's when empty.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default = "one")]
    pub intensity: f32,
    #[serde(default = "quarter")]
    pub transition: f32,
    /// Each control it sets and its value, 0 to 1.
    #[serde(default)]
    pub controls: BTreeMap<String, f32>,
}

fn one() -> f32 {
    1.0
}

fn quarter() -> f32 {
    0.25
}

impl Default for FaceExpression {
    fn default() -> Self {
        Self {
            format_version: FORMAT_VERSION,
            name: String::new(),
            intensity: 1.0,
            transition: 0.25,
            controls: BTreeMap::new(),
        }
    }
}

impl FaceExpression {
    /// Reads an expression, saying what's wrong with one that can't be.
    pub fn from_json(text: &str) -> Result<Self, String> {
        let expression: FaceExpression =
            serde_json::from_str(text).map_err(|err| err.to_string())?;
        if expression.format_version != FORMAT_VERSION {
            return Err(format!(
                "it is in expression format {}, and only {FORMAT_VERSION} can be read",
                expression.format_version
            ));
        }
        Ok(expression)
    }

    /// Pretty JSON, the same every time for the same expression.
    pub fn to_json(&self) -> String {
        let mut text = serde_json::to_string_pretty(self).expect("plain data");
        text.push('\n');
        text
    }
}

/// The name an expression at `path` goes by: its `name`, or its file's.
pub fn expression_name<'a>(path: &'a str, expression: &'a FaceExpression) -> &'a str {
    if !expression.name.is_empty() {
        return &expression.name;
    }
    let file = path.rsplit(['/', '\\']).next().unwrap_or(path);
    file.get(..file.len().saturating_sub(EXPRESSION_EXTENSION.len()))
        .filter(|_| is_face_expression(file))
        .unwrap_or(file)
}

/// Every expression in a project, by path, in path order, and the files
/// that couldn't be read (one line each).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FaceExpressions {
    pub expressions: Vec<(String, FaceExpression)>,
    pub problems: Vec<String>,
}

impl FaceExpressions {
    /// The expression named `name` (any case), or at the path `name`.
    pub fn find(&self, name: &str) -> Option<(&str, &FaceExpression)> {
        self.expressions
            .iter()
            .find(|(path, expression)| {
                path == name || expression_name(path, expression).eq_ignore_ascii_case(name)
            })
            .map(|(path, expression)| (path.as_str(), expression))
    }
}

/// Whether `path` is an expression's.
pub fn is_face_expression(path: &str) -> bool {
    path.to_ascii_lowercase().ends_with(EXPRESSION_EXTENSION)
}

/// Reads every `*.expression.json` under the project's
/// `assets/faces/expressions/` (and the folders inside it).
pub fn read_face_expressions(project: &Path) -> FaceExpressions {
    let mut found = FaceExpressions::default();
    let mut files = Vec::new();
    collect(
        &project.join(EXPRESSIONS_FOLDER),
        EXPRESSIONS_FOLDER,
        &mut files,
    );
    files.sort();
    for path in files {
        match fs::read_to_string(project.join(&path))
            .map_err(|err| err.to_string())
            .and_then(|text| FaceExpression::from_json(&text))
        {
            Ok(expression) => found.expressions.push((path, expression)),
            Err(why) => found
                .problems
                .push(format!("The expression `{path}` can't be read: {why}")),
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
        } else if is_face_expression(&name) {
            files.push(path);
        }
    }
}

/// Writes `expression` to `path` inside `project`, making its folder.
pub fn write_face_expression(
    project: &Path,
    path: &str,
    expression: &FaceExpression,
) -> std::io::Result<()> {
    let file = project.join(path);
    if let Some(folder) = file.parent() {
        fs::create_dir_all(folder)?;
    }
    crate::cooked::write_whole(&file, expression.to_json().as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expressions_round_trip_and_are_found_by_name_or_path() {
        let text = r#"{ "format_version": 1, "controls": { "Mouth_Smile_Left": 0.8 } }"#;
        let read = FaceExpression::from_json(text).unwrap();
        assert_eq!(read.intensity, 1.0);
        assert_eq!(read.transition, 0.25);
        assert_eq!(FaceExpression::from_json(&read.to_json()).unwrap(), read);
        assert!(FaceExpression::from_json(r#"{"format_version":2}"#).is_err());
        assert!(FaceExpression::from_json(r#"{"format_version":1,"control":{}}"#).is_err());

        let project =
            std::env::temp_dir().join(format!("axcenity-expressions-{}", std::process::id()));
        let _ = fs::remove_dir_all(&project);
        write_face_expression(
            &project,
            "assets/faces/expressions/Smug.expression.json",
            &read,
        )
        .unwrap();
        let mut named = read.clone();
        named.name = "Grin".into();
        write_face_expression(
            &project,
            "assets/faces/expressions/a/b.expression.json",
            &named,
        )
        .unwrap();
        fs::write(
            project.join("assets/faces/expressions/bad.expression.json"),
            "{",
        )
        .unwrap();
        let found = read_face_expressions(&project);
        assert_eq!(found.expressions.len(), 2);
        assert_eq!(found.problems.len(), 1);
        assert_eq!(
            found.find("smug").unwrap().0,
            "assets/faces/expressions/Smug.expression.json"
        );
        assert_eq!(found.find("Grin").unwrap().1, &named);
        assert!(
            found
                .find("assets/faces/expressions/a/b.expression.json")
                .is_some()
        );
        assert!(found.find("b").is_none(), "named Grin, not by its file");
        let _ = fs::remove_dir_all(&project);
    }
}
