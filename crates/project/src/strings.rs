//! The string table, `strings.json`: UI text in each language (see
//! [`StringTable`]). It's its own optional file, so a project without one
//! has an empty table and [`FORMAT_VERSION`] doesn't change.
//!
//! ```json
//! {
//!   "languages": ["English", "Français"],
//!   "entries": {
//!     "menu.play": { "English": "Play", "Français": "Jouer" }
//!   }
//! }
//! ```

use std::collections::BTreeMap;
use std::sync::Arc;

use axcenity_core::StringTable;

use super::*;

pub const STRINGS_FILE: &str = "strings.json";

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct StringsFile {
    #[serde(default)]
    languages: Vec<String>,
    #[serde(default)]
    entries: BTreeMap<String, BTreeMap<String, String>>,
}

/// Reads a string table's text.
pub fn parse_strings(text: &str) -> Result<StringTable, serde_json::Error> {
    let file: StringsFile = serde_json::from_str(text)?;
    let mut languages: Vec<String> = Vec::new();
    for language in file.languages {
        if !language.is_empty() && !languages.contains(&language) {
            languages.push(language);
        }
    }
    Ok(StringTable {
        languages,
        entries: file.entries,
    })
}

/// A string table's text, the same for the same table.
pub fn write_strings(table: &StringTable) -> String {
    let file = StringsFile {
        languages: table.languages.clone(),
        entries: table.entries.clone(),
    };
    let mut text =
        serde_json::to_string_pretty(&file).expect("a string table is plain strings in maps");
    text.push('\n');
    text
}

/// Gives `world` the table in `folder`'s `strings.json`. No file is an
/// empty table; a damaged one is too, and is added to `problems`.
pub(super) fn load_strings(folder: &Path, world: &mut World, problems: &mut Vec<LoadError>) {
    let path = folder.join(STRINGS_FILE);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == ErrorKind::NotFound => return,
        Err(source) => {
            problems.push(LoadError::Read { path, source });
            return;
        }
    };
    match parse_strings(&text) {
        Ok(table) => world.set_strings(Arc::new(table)),
        Err(source) => problems.push(LoadError::Strings { path, source }),
    }
}

/// Writes `table` to `folder`'s `strings.json`, the same way as
/// [`save_world`]. An empty table writes no file, unless there is one
/// already (it's emptied).
pub fn save_strings(folder: &Path, table: &StringTable) -> Result<(), SaveError> {
    let path = folder.join(STRINGS_FILE);
    if table.is_empty() && !path.exists() {
        return Ok(());
    }
    write_through_temporary(&path, &write_strings(table))
}
