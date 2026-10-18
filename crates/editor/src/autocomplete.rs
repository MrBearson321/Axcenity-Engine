//! File > Set Up VS Code Autocomplete: puts the engine's Lua Language Server
//! definitions into a project, so VS Code (with the Lua extension, or any
//! editor that runs Lua Language Server) autocompletes the script API and
//! points out mistakes.
//!
//! Three files, all relative to the project folder:
//! - [`DEFINITIONS_FILE`]: `axcenity_script::lua_definitions()`. Opening the
//!   project rewrites it when it's out of date ([`refresh`]), so it follows
//!   engine updates.
//! - [`SETTINGS_FILE`]: Lua Language Server settings (Lua 5.4, the
//!   definitions as a library, and no `io`, `debug` or `package`, which
//!   scripts don't have).
//! - [`EXTENSIONS_FILE`]: makes VS Code suggest installing the Lua extension.
//!
//! The last two may hold the creator's own settings, so they're only written
//! when they don't exist yet.

use std::fs;
use std::io;
use std::path::Path;

/// The definitions, in a folder of their own so Lua Language Server can
/// load it as a library rather than as one of the project's scripts.
pub const DEFINITIONS_FILE: &str = ".axcenity/axcenity.lua";
pub const SETTINGS_FILE: &str = ".luarc.json";
pub const EXTENSIONS_FILE: &str = ".vscode/extensions.json";

/// Lua Language Server settings. The keys are documented at
/// <https://luals.github.io/wiki/settings/>.
const SETTINGS: &str = r#"{
  "$schema": "https://raw.githubusercontent.com/LuaLS/vscode-lua/master/setting/schema.json",
  "runtime.version": "Lua 5.4",
  "runtime.builtin": {
    "debug": "disable",
    "io": "disable",
    "package": "disable"
  },
  "workspace.library": [".axcenity"],
  "workspace.ignoreDir": [".axcenity"]
}
"#;

/// `sumneko.lua` is the Lua Language Server extension's ID.
const EXTENSIONS: &str = r#"{
  "recommendations": ["sumneko.lua"]
}
"#;

/// What [`set_up`] did with each file.
#[derive(Debug, Default, PartialEq)]
pub struct SetUp {
    pub written: Vec<&'static str>,
    /// Files that were already there, left as they were.
    pub kept: Vec<&'static str>,
}

/// Writes the three files into the project in `folder`: the definitions
/// always, the other two only if they don't exist yet.
pub fn set_up(folder: &Path) -> io::Result<SetUp> {
    let mut result = SetUp::default();
    write(
        folder,
        DEFINITIONS_FILE,
        &axcenity_script::lua_definitions(),
    )?;
    result.written.push(DEFINITIONS_FILE);
    for (file, text) in [(SETTINGS_FILE, SETTINGS), (EXTENSIONS_FILE, EXTENSIONS)] {
        if folder.join(file).exists() {
            result.kept.push(file);
        } else {
            write(folder, file, text)?;
            result.written.push(file);
        }
    }
    Ok(result)
}

/// Rewrites the definitions file if the project in `folder` has one that
/// doesn't match this version of the engine. Returns whether it did. A
/// project that was never set up is left alone.
pub fn refresh(folder: &Path) -> io::Result<bool> {
    let path = folder.join(DEFINITIONS_FILE);
    let Ok(saved) = fs::read_to_string(&path) else {
        return Ok(false);
    };
    let current = axcenity_script::lua_definitions();
    // Git may have changed the line endings to Windows ones.
    if saved.replace("\r\n", "\n") == current {
        return Ok(false);
    }
    write(folder, DEFINITIONS_FILE, &current)?;
    Ok(true)
}

/// Writes `text` to `file` in `folder`, making its folder first if needed.
fn write(folder: &Path, file: &str, text: &str) -> io::Result<()> {
    let path = folder.join(file);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, text)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    /// An empty folder in the temp directory, for one test.
    fn empty_folder(test: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!(
            "axcenity-autocomplete-{test}-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(&folder).unwrap();
        folder
    }

    fn read(folder: &Path, file: &str) -> String {
        fs::read_to_string(folder.join(file)).unwrap()
    }

    #[test]
    fn set_up_writes_the_definitions_and_settings_but_keeps_existing_settings() {
        let folder = empty_folder("set-up");
        let result = set_up(&folder).unwrap();
        assert_eq!(
            result.written,
            [DEFINITIONS_FILE, SETTINGS_FILE, EXTENSIONS_FILE]
        );
        assert_eq!(
            read(&folder, DEFINITIONS_FILE),
            axcenity_script::lua_definitions()
        );
        assert!(read(&folder, SETTINGS_FILE).contains("\"workspace.library\": [\".axcenity\"]"));
        assert!(read(&folder, EXTENSIONS_FILE).contains("sumneko.lua"));

        // A second time, the creator's settings are left as they are.
        fs::write(folder.join(SETTINGS_FILE), "{ \"mine\": true }").unwrap();
        let result = set_up(&folder).unwrap();
        assert_eq!(result.written, [DEFINITIONS_FILE]);
        assert_eq!(result.kept, [SETTINGS_FILE, EXTENSIONS_FILE]);
        assert_eq!(read(&folder, SETTINGS_FILE), "{ \"mine\": true }");
        let _ = fs::remove_dir_all(&folder);
    }

    #[test]
    fn the_settings_are_valid_json() {
        for text in [SETTINGS, EXTENSIONS] {
            serde_json::from_str::<serde_json::Value>(text).unwrap();
        }
    }

    #[test]
    fn refresh_rewrites_only_out_of_date_definitions_in_set_up_projects() {
        let folder = empty_folder("refresh");
        // Never set up: nothing is written.
        assert!(!refresh(&folder).unwrap());
        assert!(!folder.join(DEFINITIONS_FILE).exists());

        set_up(&folder).unwrap();
        assert!(!refresh(&folder).unwrap(), "already up to date");
        // The same text with Windows line endings is still up to date.
        let windows = axcenity_script::lua_definitions().replace('\n', "\r\n");
        fs::write(folder.join(DEFINITIONS_FILE), windows).unwrap();
        assert!(!refresh(&folder).unwrap());

        // As if made by an older engine.
        fs::write(folder.join(DEFINITIONS_FILE), "---@meta\n-- old\n").unwrap();
        assert!(refresh(&folder).unwrap());
        assert_eq!(
            read(&folder, DEFINITIONS_FILE),
            axcenity_script::lua_definitions()
        );
        let _ = fs::remove_dir_all(&folder);
    }
}
