//! The input map, `input.json`: the project's actions and what each is
//! bound to (see [`InputMap`]). It's its own optional file, so a project
//! without one gets the default actions and [`FORMAT_VERSION`] doesn't
//! change. The editor's Input settings write it.
//!
//! ```json
//! {
//!   "actions": [
//!     { "name": "Move", "axis": true, "inputs": ["W,A,S,D", "Thumbstick1"] },
//!     { "name": "Jump", "inputs": ["Space", "ButtonA"] }
//!   ]
//! }
//! ```
//!
//! Inputs are written as [`Input::name`] does: a key or button name
//! (`Enum.KeyCode`), `MouseButton1` to `MouseButton3`, four keys for an
//! axis (`W,A,S,D`: up, left, down, right) or `MouseMovement`.

use axcenity_core::input::{Action, Input, InputMap};

use super::*;

pub const INPUT_FILE: &str = "input.json";

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct InputFile {
    actions: Vec<ActionEntry>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ActionEntry {
    name: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    axis: bool,
    #[serde(default)]
    inputs: Vec<String>,
}

/// Reads an input map's text. An input that isn't one is an error naming it.
pub fn parse_input_map(text: &str) -> Result<InputMap, String> {
    let file: InputFile = serde_json::from_str(text).map_err(|error| error.to_string())?;
    let mut actions: Vec<Action> = Vec::new();
    for entry in file.actions {
        if entry.name.trim().is_empty() {
            return Err("an action has no name".to_owned());
        }
        if actions.iter().any(|action| action.name == entry.name) {
            return Err(format!("there are two actions named \"{}\"", entry.name));
        }
        let inputs = entry
            .inputs
            .iter()
            .map(|name| {
                Input::parse(name).ok_or_else(|| {
                    format!("\"{name}\" (in action \"{}\") isn't an input", entry.name)
                })
            })
            .collect::<Result<_, _>>()?;
        actions.push(Action {
            name: entry.name,
            axis: entry.axis,
            inputs,
        });
    }
    Ok(InputMap { actions })
}

/// An input map's text, the same for the same map.
pub fn write_input_map(map: &InputMap) -> String {
    let file = InputFile {
        actions: map
            .actions
            .iter()
            .map(|action| ActionEntry {
                name: action.name.clone(),
                axis: action.axis,
                inputs: action.inputs.iter().map(Input::name).collect(),
            })
            .collect(),
    };
    let mut text = serde_json::to_string_pretty(&file).expect("an input map is plain strings");
    text.push('\n');
    text
}

/// The input map in `folder`'s `input.json`. No file is the default map;
/// a damaged one is too, and is added to `problems`.
pub fn load_input_map(folder: &Path, problems: &mut Vec<LoadError>) -> InputMap {
    let path = folder.join(INPUT_FILE);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == ErrorKind::NotFound => return InputMap::default(),
        Err(source) => {
            problems.push(LoadError::Read { path, source });
            return InputMap::default();
        }
    };
    match parse_input_map(&text) {
        Ok(map) => map,
        Err(message) => {
            problems.push(LoadError::InputMap { path, message });
            InputMap::default()
        }
    }
}

/// Writes `map` to `folder`'s `input.json`, the same way as [`save_world`].
pub fn save_input_map(folder: &Path, map: &InputMap) -> Result<(), SaveError> {
    write_through_temporary(&folder.join(INPUT_FILE), &write_input_map(map))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axcenity_core::KeyCode;

    /// An empty folder of its own for one test.
    fn folder(name: &str) -> PathBuf {
        let folder =
            std::env::temp_dir().join(format!("axcenity-input-{name}-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(&folder).unwrap();
        folder
    }

    #[test]
    fn the_input_map_round_trips_and_a_missing_file_is_the_defaults() {
        let folder = folder("round-trip");
        let mut problems = Vec::new();
        assert_eq!(load_input_map(&folder, &mut problems), InputMap::default());

        let mut map = InputMap::default();
        map.rebind("Jump", vec![Input::Key(KeyCode::E)]).unwrap();
        save_input_map(&folder, &map).unwrap();
        assert_eq!(load_input_map(&folder, &mut problems), map);
        assert!(problems.is_empty(), "{problems:?}");
        let text = fs::read_to_string(folder.join(INPUT_FILE)).unwrap();
        assert!(
            text.contains("\"inputs\": [\n        \"E\"\n      ]"),
            "{text}"
        );
    }

    #[test]
    fn a_damaged_input_map_says_what_is_wrong_and_uses_the_defaults() {
        let folder = folder("damaged");
        fs::write(
            folder.join(INPUT_FILE),
            r#"{"actions": [{"name": "Jump", "inputs": ["Spacebar"]}]}"#,
        )
        .unwrap();
        let mut problems = Vec::new();
        assert_eq!(load_input_map(&folder, &mut problems), InputMap::default());
        let message = problems[0].to_string();
        assert!(
            message.contains("\"Spacebar\" (in action \"Jump\") isn't an input"),
            "{message}"
        );
    }
}
