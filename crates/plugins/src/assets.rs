//! Importer hooks and the Assets panel (roadmap G10).
//!
//! - `plugin:AddImporter({ Name, Extensions })`: files of those kinds that
//!   the editor doesn't import itself go to the importer's `Imported(name,
//!   contents)` when they're imported (Import…, or dropped on the window).
//! - `plugin:AddAssetAction({ Label, Extensions, Tooltip })`: an item in
//!   the Assets panel's right-click menu, firing `Activated(path)`.
//! - `plugin.AssetImported(path)`: the editor imported a file itself.
//!
//! The editor reads them with [`crate::Plugin::take_asset_hooks`] and calls
//! [`crate::Plugin::import_file`], [`crate::Plugin::asset_action`] and
//! [`crate::Plugin::asset_imported`].

use std::collections::BTreeMap;

use mlua::{Lua, MetaMethod, Table, UserData, UserDataFields, UserDataMethods};

use crate::api::{LuaSignal, SignalKey, state};

/// How many importers and asset actions one plugin may add, together.
pub const MAX_ASSET_HOOKS: usize = 32;
/// The largest file an importer is given, in bytes.
pub const MAX_IMPORT_BYTES: u64 = 16 * 1024 * 1024;

/// A plugin's importer for files the editor doesn't import itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Importer {
    pub id: u64,
    /// What the files are, for the Import… file picker ("Level layout").
    pub name: String,
    /// File endings, lowercase without the dot (`"csv"`).
    pub extensions: Vec<String>,
}

impl Importer {
    /// Whether it takes a file named `name`.
    pub fn takes(&self, name: &str) -> bool {
        extension_of(name).is_some_and(|ext| self.extensions.contains(&ext))
    }
}

/// An item a plugin added to the Assets panel's right-click menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetAction {
    pub id: u64,
    pub label: String,
    pub tooltip: String,
    /// The file endings it's offered for; empty for every asset.
    pub extensions: Vec<String>,
}

impl AssetAction {
    /// Whether it's offered for the asset at `path`.
    pub fn offered_for(&self, path: &str) -> bool {
        self.extensions.is_empty()
            || extension_of(path).is_some_and(|ext| self.extensions.contains(&ext))
    }
}

/// A file's ending, lowercase: `"glb"` for `models/tree.GLB`. Two-part
/// endings count by their last part (`.prefab.json` is `"json"`).
pub fn extension_of(path: &str) -> Option<String> {
    let name = path.rsplit(['/', '\\']).next()?;
    let (stem, ext) = name.rsplit_once('.')?;
    (!stem.is_empty() && !ext.is_empty()).then(|| ext.to_ascii_lowercase())
}

/// A plugin's importers and asset actions, and whether they changed since
/// the host last took them.
#[derive(Default)]
pub(crate) struct AssetHooks {
    pub(crate) importers: BTreeMap<u64, Importer>,
    pub(crate) actions: BTreeMap<u64, AssetAction>,
    pub(crate) next: u64,
    pub(crate) changed: bool,
}

/// `Extensions`: a list of file endings, with or without the dot.
fn extensions(info: &Table, needed: bool) -> mlua::Result<Vec<String>> {
    let list: Vec<String> = info
        .get::<Option<Vec<String>>>("Extensions")?
        .unwrap_or_default();
    let mut extensions = Vec::new();
    for ext in list {
        let ext = ext.trim().trim_start_matches('.').to_ascii_lowercase();
        if ext.is_empty() || ext.len() > 16 || !ext.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(mlua::Error::runtime(format!(
                "\"{ext}\" isn't a file ending: 1 to 16 letters and digits, such as \"csv\""
            )));
        }
        if !extensions.contains(&ext) {
            extensions.push(ext);
        }
    }
    if needed && extensions.is_empty() {
        return Err(mlua::Error::runtime(
            "the importer needs Extensions, such as { \"csv\" }",
        ));
    }
    Ok(extensions)
}

fn text(info: &Table, key: &str, needed: bool) -> mlua::Result<String> {
    match info.get::<Option<String>>(key)? {
        Some(text) if !text.trim().is_empty() => Ok(text.chars().take(60).collect()),
        _ if needed => Err(mlua::Error::runtime(format!("it needs a {key}"))),
        _ => Ok(String::new()),
    }
}

fn next_id(lua: &Lua) -> mlua::Result<u64> {
    let mut state = state(lua)?;
    let hooks = &mut state.assets;
    if hooks.importers.len() + hooks.actions.len() >= MAX_ASSET_HOOKS {
        return Err(mlua::Error::runtime(format!(
            "a plugin can add at most {MAX_ASSET_HOOKS} importers and asset actions"
        )));
    }
    hooks.next += 1;
    hooks.changed = true;
    Ok(hooks.next)
}

/// `plugin:AddImporter({ Name, Extensions })`.
pub(crate) fn importer(lua: &Lua, info: Table) -> mlua::Result<LuaImporter> {
    let name = text(&info, "Name", true)?;
    let extensions = extensions(&info, true)?;
    let id = next_id(lua)?;
    state(lua)?.assets.importers.insert(
        id,
        Importer {
            id,
            name,
            extensions,
        },
    );
    Ok(LuaImporter(id))
}

/// `plugin:AddAssetAction({ Label, Extensions, Tooltip })`.
pub(crate) fn action(lua: &Lua, info: Table) -> mlua::Result<LuaAssetAction> {
    let label = text(&info, "Label", true)?;
    let tooltip = text(&info, "Tooltip", false)?;
    let extensions = extensions(&info, false)?;
    let id = next_id(lua)?;
    state(lua)?.assets.actions.insert(
        id,
        AssetAction {
            id,
            label,
            tooltip,
            extensions,
        },
    );
    Ok(LuaAssetAction(id))
}

fn remove(lua: &Lua, id: u64) -> mlua::Result<()> {
    let mut state = state(lua)?;
    let hooks = &mut state.assets;
    let removed = hooks.importers.remove(&id).is_some() | hooks.actions.remove(&id).is_some();
    hooks.changed |= removed;
    state.handlers.remove(&SignalKey::Imported(id));
    state.handlers.remove(&SignalKey::AssetAction(id));
    Ok(())
}

/// An importer, by its number.
pub(crate) struct LuaImporter(u64);

impl UserData for LuaImporter {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("Imported", |_, this| {
            Ok(LuaSignal(SignalKey::Imported(this.0)))
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("Remove", |lua, this, ()| remove(lua, this.0));
        methods.add_meta_method(MetaMethod::ToString, |_, _, ()| Ok("PluginImporter"));
    }
}

/// An asset action, by its number.
pub(crate) struct LuaAssetAction(u64);

impl UserData for LuaAssetAction {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("Activated", |_, this| {
            Ok(LuaSignal(SignalKey::AssetAction(this.0)))
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("Remove", |lua, this, ()| remove(lua, this.0));
        methods.add_meta_method(MetaMethod::ToString, |_, _, ()| Ok("PluginAssetAction"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_files_ending_is_its_last_part_in_lowercase() {
        assert_eq!(extension_of("models/Tree.GLB").as_deref(), Some("glb"));
        assert_eq!(
            extension_of("prefabs/a.prefab.json").as_deref(),
            Some("json")
        );
        assert_eq!(extension_of(r"C:\levels\one.csv").as_deref(), Some("csv"));
        assert_eq!(extension_of("README"), None);
        assert_eq!(extension_of(".hidden"), None);
        let action = AssetAction {
            id: 1,
            label: "Open".into(),
            tooltip: String::new(),
            extensions: vec!["glb".into()],
        };
        assert!(action.offered_for("models/tree.glb") && !action.offered_for("a.png"));
        let any = AssetAction {
            extensions: Vec::new(),
            ..action
        };
        assert!(any.offered_for("a.png"));
    }
}
