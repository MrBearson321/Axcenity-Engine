//! File > Set Up VS Code Autocomplete: puts the engine's luau-lsp
//! definitions into a project, so VS Code (with the Luau extension, or any
//! editor that runs luau-lsp) autocompletes the script API and type-checks
//! scripts as Luau (`+=`, `continue` and type annotations are fine).
//!
//! Three files, all relative to the project folder:
//! - [`DEFINITIONS_FILE`]: `axcenity_script::lua_definitions()`. Opening the
//!   project rewrites it when it's out of date ([`refresh`]), so it follows
//!   engine updates.
//! - [`SETTINGS_FILE`]: luau-lsp settings (the standard platform rather than
//!   Roblox, the definitions, `.lua` files as Luau, and no globals that
//!   scripts don't have).
//! - [`EXTENSIONS_FILE`]: makes VS Code suggest installing the Luau extension.
//!
//! The last two may hold the creator's own settings, so the engine's are
//! only added where they're missing; nothing the creator set is changed.
//!
//! Before luau-lsp, autocomplete used the Lua Language Server in Lua 5.4
//! mode, which flagged valid Luau. [`set_up`] removes the files it wrote,
//! unless the creator has changed `.luarc.json`.

use std::fs;
use std::io;
use std::path::Path;

use serde_json::Value;

/// The definitions, in a folder the project loader skips, so they're never
/// taken for one of the project's scripts.
pub const DEFINITIONS_FILE: &str = ".axcenity/axcenity.d.luau";
pub const SETTINGS_FILE: &str = ".vscode/settings.json";
pub const EXTENSIONS_FILE: &str = ".vscode/extensions.json";

/// luau-lsp settings. The keys are the extension's own (its `package.json`).
/// `luau-lsp.types.disabledGlobals` hides Luau globals that the sandbox
/// leaves out (`crates/script/src/sandbox.rs`).
const SETTINGS: &str = r#"{
  "files.associations": {
    "*.lua": "luau"
  },
  "luau-lsp.platform.type": "standard",
  "luau-lsp.sourcemap.enabled": false,
  "luau-lsp.types.definitionFiles": {
    "@axcenity": ".axcenity/axcenity.d.luau"
  },
  "luau-lsp.types.disabledGlobals": [
    "debug",
    "gcinfo",
    "getfenv",
    "loadstring",
    "newproxy",
    "setfenv",
    "unpack",
    "vector"
  ]
}
"#;

/// `JohnnyMorganz.luau-lsp` is the Luau extension's ID. The Lua Language
/// Server extension would also take `.lua` files and flag Luau syntax.
const EXTENSIONS: &str = r#"{
  "recommendations": ["JohnnyMorganz.luau-lsp"],
  "unwantedRecommendations": ["sumneko.lua"]
}
"#;

/// Lua Language Server definitions from an older engine. Always generated,
/// so [`set_up`] removes it whatever it holds.
const OLD_DEFINITIONS_FILE: &str = ".axcenity/axcenity.lua";

/// Lua Language Server settings from an older engine, removed by
/// [`set_up`] if they're still exactly [`OLD_SETTINGS`].
const OLD_SETTINGS_FILE: &str = ".luarc.json";
const OLD_SETTINGS: &str = r#"{
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

/// The `extensions.json` older engines wrote, which is replaced.
const OLD_EXTENSIONS: &str = r#"{
  "recommendations": ["sumneko.lua"]
}
"#;

/// What [`set_up`] did with each file.
#[derive(Debug, Default, PartialEq)]
pub struct SetUp {
    pub written: Vec<&'static str>,
    /// The creator's own files that got the engine's missing settings.
    pub updated: Vec<&'static str>,
    /// The creator's own files that already had them, left as they were.
    pub kept: Vec<&'static str>,
    /// The creator's own files that aren't plain JSON (VS Code allows
    /// comments), so the engine's settings couldn't be added.
    pub not_json: Vec<&'static str>,
    /// Lua Language Server files from an older engine.
    pub removed: Vec<&'static str>,
}

/// Writes the three files into the project in `folder`: the definitions
/// always, the engine's settings where they're missing.
pub fn set_up(folder: &Path) -> io::Result<SetUp> {
    let mut result = SetUp::default();
    write(
        folder,
        DEFINITIONS_FILE,
        &axcenity_script::lua_definitions(),
    )?;
    result.written.push(DEFINITIONS_FILE);
    for (file, text) in [(SETTINGS_FILE, SETTINGS), (EXTENSIONS_FILE, EXTENSIONS)] {
        let path = folder.join(file);
        let Ok(saved) = fs::read_to_string(&path) else {
            write(folder, file, text)?;
            result.written.push(file);
            continue;
        };
        if file == EXTENSIONS_FILE && same_text(&saved, OLD_EXTENSIONS) {
            write(folder, file, text)?;
            result.written.push(file);
            continue;
        }
        let Ok(mut settings) = serde_json::from_str::<Value>(&saved) else {
            result.not_json.push(file);
            continue;
        };
        let ours: Value = serde_json::from_str(text).expect("the settings are valid JSON");
        if add_missing(&mut settings, &ours) {
            let mut text = serde_json::to_string_pretty(&settings).map_err(io::Error::other)?;
            text.push('\n');
            write(folder, file, &text)?;
            result.updated.push(file);
        } else {
            result.kept.push(file);
        }
    }
    let old_definitions = folder.join(OLD_DEFINITIONS_FILE);
    if old_definitions.exists() {
        fs::remove_file(old_definitions)?;
        result.removed.push(OLD_DEFINITIONS_FILE);
    }
    let old_settings = folder.join(OLD_SETTINGS_FILE);
    if fs::read_to_string(&old_settings).is_ok_and(|saved| same_text(&saved, OLD_SETTINGS)) {
        fs::remove_file(old_settings)?;
        result.removed.push(OLD_SETTINGS_FILE);
    }
    Ok(result)
}

/// Adds what `ours` has and `target` doesn't: missing keys of objects, and
/// missing items of lists. A value the creator set differently is left as it
/// is. Returns whether anything was added.
fn add_missing(target: &mut Value, ours: &Value) -> bool {
    match (target, ours) {
        (Value::Object(target), Value::Object(ours)) => {
            let mut added = false;
            for (key, value) in ours {
                match target.get_mut(key) {
                    Some(existing) => added |= add_missing(existing, value),
                    None => {
                        target.insert(key.clone(), value.clone());
                        added = true;
                    }
                }
            }
            added
        }
        (Value::Array(target), Value::Array(ours)) => {
            let missing: Vec<Value> = ours
                .iter()
                .filter(|item| !target.contains(item))
                .cloned()
                .collect();
            target.extend_from_slice(&missing);
            !missing.is_empty()
        }
        _ => false,
    }
}

/// Whether two texts match, ignoring Windows line endings that Git may have
/// added.
fn same_text(saved: &str, text: &str) -> bool {
    saved.replace("\r\n", "\n") == text
}

/// What [`refresh`] found.
#[derive(Debug, PartialEq)]
pub enum Refresh {
    /// Never set up, or already up to date: nothing was written.
    Unchanged,
    /// The definitions were rewritten for this version of the engine.
    Updated,
    /// Set up by an older engine, for the Lua Language Server. Running Set
    /// Up again moves it to luau-lsp; this doesn't, since it changes the
    /// creator's VS Code settings.
    OldSetUp,
}

/// Rewrites the definitions file if the project in `folder` has one that
/// doesn't match this version of the engine. A project that was never set
/// up is left alone.
pub fn refresh(folder: &Path) -> io::Result<Refresh> {
    let Ok(saved) = fs::read_to_string(folder.join(DEFINITIONS_FILE)) else {
        return Ok(if folder.join(OLD_DEFINITIONS_FILE).exists() {
            Refresh::OldSetUp
        } else {
            Refresh::Unchanged
        });
    };
    let current = axcenity_script::lua_definitions();
    if same_text(&saved, &current) {
        return Ok(Refresh::Unchanged);
    }
    write(folder, DEFINITIONS_FILE, &current)?;
    Ok(Refresh::Updated)
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

    fn json(folder: &Path, file: &str) -> Value {
        serde_json::from_str(&read(folder, file)).unwrap()
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
        let settings = json(&folder, SETTINGS_FILE);
        assert_eq!(settings["luau-lsp.platform.type"], "standard");
        assert_eq!(
            settings["luau-lsp.types.definitionFiles"]["@axcenity"],
            DEFINITIONS_FILE
        );
        assert!(read(&folder, EXTENSIONS_FILE).contains("JohnnyMorganz.luau-lsp"));

        // A second time, nothing is missing, so both are left as they are.
        let result = set_up(&folder).unwrap();
        assert_eq!(result.written, [DEFINITIONS_FILE]);
        assert_eq!(result.kept, [SETTINGS_FILE, EXTENSIONS_FILE]);
        let _ = fs::remove_dir_all(&folder);
    }

    #[test]
    fn set_up_adds_only_missing_settings_to_the_creators_own() {
        let folder = empty_folder("merge");
        let settings = r#"{ "editor.tabSize": 2, "luau-lsp.platform.type": "roblox",
            "luau-lsp.types.disabledGlobals": ["os"] }"#;
        write(&folder, SETTINGS_FILE, settings).unwrap();
        write(&folder, EXTENSIONS_FILE, "// comments\n{}").unwrap();
        let result = set_up(&folder).unwrap();
        assert_eq!(result.updated, [SETTINGS_FILE]);
        assert_eq!(result.not_json, [EXTENSIONS_FILE]);

        let settings = json(&folder, SETTINGS_FILE);
        assert_eq!(settings["editor.tabSize"], 2);
        // The creator's choice wins.
        assert_eq!(settings["luau-lsp.platform.type"], "roblox");
        assert_eq!(settings["files.associations"]["*.lua"], "luau");
        let disabled = settings["luau-lsp.types.disabledGlobals"]
            .as_array()
            .unwrap();
        assert_eq!(disabled[0], "os");
        assert!(disabled.contains(&Value::from("loadstring")));
        assert_eq!(read(&folder, EXTENSIONS_FILE), "// comments\n{}");
        let _ = fs::remove_dir_all(&folder);
    }

    #[test]
    fn set_up_replaces_an_old_lua_language_server_set_up() {
        let folder = empty_folder("old");
        write(&folder, OLD_DEFINITIONS_FILE, "---@meta\n").unwrap();
        // Git may have given them Windows line endings.
        write(
            &folder,
            OLD_SETTINGS_FILE,
            &OLD_SETTINGS.replace('\n', "\r\n"),
        )
        .unwrap();
        write(&folder, EXTENSIONS_FILE, OLD_EXTENSIONS).unwrap();
        assert_eq!(refresh(&folder).unwrap(), Refresh::OldSetUp);

        let result = set_up(&folder).unwrap();
        assert_eq!(
            result.written,
            [DEFINITIONS_FILE, SETTINGS_FILE, EXTENSIONS_FILE]
        );
        assert_eq!(result.removed, [OLD_DEFINITIONS_FILE, OLD_SETTINGS_FILE]);
        assert!(!folder.join(OLD_SETTINGS_FILE).exists());
        assert!(!read(&folder, EXTENSIONS_FILE).contains("\"recommendations\": [\"sumneko"));

        // A `.luarc.json` the creator changed is theirs to remove.
        write(&folder, OLD_SETTINGS_FILE, "{ \"mine\": true }").unwrap();
        let result = set_up(&folder).unwrap();
        assert!(result.removed.is_empty());
        assert!(folder.join(OLD_SETTINGS_FILE).exists());
        let _ = fs::remove_dir_all(&folder);
    }

    #[test]
    fn the_settings_are_valid_json() {
        for text in [SETTINGS, EXTENSIONS, OLD_EXTENSIONS, OLD_SETTINGS] {
            serde_json::from_str::<Value>(text).unwrap();
        }
    }

    #[test]
    fn refresh_rewrites_only_out_of_date_definitions_in_set_up_projects() {
        let folder = empty_folder("refresh");
        // Never set up: nothing is written.
        assert_eq!(refresh(&folder).unwrap(), Refresh::Unchanged);
        assert!(!folder.join(DEFINITIONS_FILE).exists());

        set_up(&folder).unwrap();
        assert_eq!(refresh(&folder).unwrap(), Refresh::Unchanged);
        // The same text with Windows line endings is still up to date.
        let windows = axcenity_script::lua_definitions().replace('\n', "\r\n");
        fs::write(folder.join(DEFINITIONS_FILE), windows).unwrap();
        assert_eq!(refresh(&folder).unwrap(), Refresh::Unchanged);

        // As if made by an older engine.
        fs::write(folder.join(DEFINITIONS_FILE), "-- old\n").unwrap();
        assert_eq!(refresh(&folder).unwrap(), Refresh::Updated);
        assert_eq!(
            read(&folder, DEFINITIONS_FILE),
            axcenity_script::lua_definitions()
        );
        let _ = fs::remove_dir_all(&folder);
    }
}
