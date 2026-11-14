//! Commands and Explorer items (plugin API 3, P7.4 of
//! `docs/research/import-formats-and-plugins.md`).
//!
//! - `plugin:CreateAction({ Id, Text, Shortcut, Tooltip })`: a command,
//!   listed in Ctrl+K under the plugin's name, with an optional keyboard
//!   shortcut while editing; fires `Triggered`.
//! - `plugin:AddExplorerAction({ Label, Class, Tooltip })`: an item in the
//!   Explorer's right-click menu (only on rows of `Class`, or of a class
//!   that is one, when given); fires `Activated(instances)` with the
//!   selection.
//!
//! The editor reads them with [`crate::Plugin::take_actions`] and calls
//! [`crate::Plugin::trigger_action`] and [`crate::Plugin::explorer_action`].

use std::collections::BTreeMap;

use axcenity_core::ClassName;
use mlua::{Lua, MetaMethod, Table, UserData, UserDataFields, UserDataMethods};

use crate::api::{LuaSignal, SignalKey, state};

/// How many commands and Explorer items one plugin may add, together.
pub const MAX_ACTIONS: usize = 32;

/// A command a plugin made with `plugin:CreateAction`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Action {
    pub id: u64,
    /// The `Id` the plugin gave it: making one with the same `Id` again
    /// gives back this one.
    pub key: String,
    /// What Ctrl+K lists it as.
    pub text: String,
    pub tooltip: String,
    pub shortcut: Option<Shortcut>,
}

/// A keyboard shortcut: Ctrl, Shift and Alt with a letter, a digit or F1
/// to F12. Ctrl is Cmd on macOS.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shortcut {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    /// `"A"` to `"Z"`, `"0"` to `"9"`, or `"F1"` to `"F12"`.
    pub key: String,
}

impl Shortcut {
    /// Reads `"Ctrl+Shift+A"` (any case, `-` or `+` between parts, `Cmd`
    /// for Ctrl). Without Ctrl or Alt only F1 to F12 are taken, so a
    /// plugin can't take a key that's typed into text.
    pub fn parse(text: &str) -> Result<Shortcut, String> {
        let mut shortcut = Shortcut {
            ctrl: false,
            shift: false,
            alt: false,
            key: String::new(),
        };
        let parts: Vec<&str> = text
            .split(['+', '-'])
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .collect();
        let bad = || {
            format!(
                "\"{text}\" isn't a shortcut: Ctrl, Shift and Alt with a letter, a digit or \
                 F1 to F12, such as \"Ctrl+Shift+A\""
            )
        };
        let Some((key, modifiers)) = parts.split_last() else {
            return Err(bad());
        };
        for modifier in modifiers {
            match modifier.to_ascii_lowercase().as_str() {
                "ctrl" | "control" | "cmd" | "command" => shortcut.ctrl = true,
                "shift" => shortcut.shift = true,
                "alt" | "option" => shortcut.alt = true,
                _ => return Err(bad()),
            }
        }
        let key = key.to_ascii_uppercase();
        let function = key
            .strip_prefix('F')
            .and_then(|n| n.parse::<u8>().ok())
            .is_some_and(|n| (1..=12).contains(&n));
        let plain = key.len() == 1 && key.chars().all(|c| c.is_ascii_alphanumeric());
        if !function && !plain {
            return Err(bad());
        }
        if !function && !shortcut.ctrl && !shortcut.alt {
            return Err(format!(
                "\"{text}\" needs Ctrl or Alt, so it isn't a key that's typed into text"
            ));
        }
        shortcut.key = key;
        Ok(shortcut)
    }

    /// As the menus write it: `"Ctrl+Shift+A"`.
    pub fn text(&self) -> String {
        let mut parts = Vec::new();
        if self.ctrl {
            parts.push("Ctrl");
        }
        if self.shift {
            parts.push("Shift");
        }
        if self.alt {
            parts.push("Alt");
        }
        parts.push(&self.key);
        parts.join("+")
    }
}

/// An item a plugin added to the Explorer's right-click menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExplorerAction {
    pub id: u64,
    pub label: String,
    pub tooltip: String,
    /// Only on rows of this class, or a class that is one (`"BasePart"`);
    /// `None` for every row.
    pub class: Option<String>,
}

impl ExplorerAction {
    /// Whether it's offered on a row of `class`.
    pub fn offered_for(&self, class: ClassName) -> bool {
        self.class
            .as_deref()
            .is_none_or(|wanted| class.is_a(wanted))
    }
}

/// A plugin's commands and Explorer items, and whether they changed since
/// the host last took them.
#[derive(Default)]
pub(crate) struct ActionHooks {
    pub(crate) actions: BTreeMap<u64, Action>,
    pub(crate) explorer: BTreeMap<u64, ExplorerAction>,
    pub(crate) next: u64,
    pub(crate) changed: bool,
}

fn text(info: &Table, key: &str, needed: bool) -> mlua::Result<String> {
    match info.get::<Option<String>>(key)? {
        Some(text) if !text.trim().is_empty() => Ok(text.trim().chars().take(60).collect()),
        _ if needed => Err(mlua::Error::runtime(format!("it needs a {key}"))),
        _ => Ok(String::new()),
    }
}

fn next_id(lua: &Lua) -> mlua::Result<u64> {
    let mut state = state(lua)?;
    let hooks = &mut state.actions;
    if hooks.actions.len() + hooks.explorer.len() >= MAX_ACTIONS {
        return Err(mlua::Error::runtime(format!(
            "a plugin can add at most {MAX_ACTIONS} actions and Explorer items"
        )));
    }
    hooks.next += 1;
    hooks.changed = true;
    Ok(hooks.next)
}

/// `plugin:CreateAction({ Id, Text, Shortcut, Tooltip })`.
pub(crate) fn create(lua: &Lua, info: Table) -> mlua::Result<LuaAction> {
    let key = text(&info, "Id", true)?;
    let label = match text(&info, "Text", false)? {
        label if label.is_empty() => key.clone(),
        label => label,
    };
    let tooltip = text(&info, "Tooltip", false)?;
    let shortcut = match info.get::<Option<String>>("Shortcut")? {
        Some(shortcut) if !shortcut.trim().is_empty() => {
            Some(Shortcut::parse(&shortcut).map_err(mlua::Error::runtime)?)
        }
        _ => None,
    };
    let known = state(lua)?
        .actions
        .actions
        .values()
        .find(|action| action.key == key)
        .map(|action| action.id);
    let id = match known {
        Some(id) => id,
        None => next_id(lua)?,
    };
    let mut state = state(lua)?;
    state.actions.changed = true;
    state.actions.actions.insert(
        id,
        Action {
            id,
            key,
            text: label,
            tooltip,
            shortcut,
        },
    );
    Ok(LuaAction(id))
}

/// `plugin:AddExplorerAction({ Label, Class, Tooltip })`.
pub(crate) fn explorer(lua: &Lua, info: Table) -> mlua::Result<LuaExplorerAction> {
    let label = text(&info, "Label", true)?;
    let tooltip = text(&info, "Tooltip", false)?;
    let class = match text(&info, "Class", false)? {
        class if class.is_empty() => None,
        class => {
            // A class name or one classes inherit from, such as BasePart.
            let known = ClassName::from_name(&class).is_some()
                || ClassName::ALL.iter().any(|known| known.is_a(&class));
            if !known {
                return Err(mlua::Error::runtime(format!(
                    "\"{class}\" isn't a class, such as \"Model\" or \"BasePart\""
                )));
            }
            Some(class)
        }
    };
    let id = next_id(lua)?;
    state(lua)?.actions.explorer.insert(
        id,
        ExplorerAction {
            id,
            label,
            tooltip,
            class,
        },
    );
    Ok(LuaExplorerAction(id))
}

fn remove(lua: &Lua, id: u64) -> mlua::Result<()> {
    let mut state = state(lua)?;
    let hooks = &mut state.actions;
    let removed = hooks.actions.remove(&id).is_some() | hooks.explorer.remove(&id).is_some();
    hooks.changed |= removed;
    state.handlers.remove(&SignalKey::ActionTriggered(id));
    state.handlers.remove(&SignalKey::ExplorerActivated(id));
    Ok(())
}

/// A command, by its number.
pub(crate) struct LuaAction(u64);

impl UserData for LuaAction {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("Triggered", |_, this| {
            Ok(LuaSignal(SignalKey::ActionTriggered(this.0)))
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("Remove", |lua, this, ()| remove(lua, this.0));
        methods.add_meta_method(MetaMethod::ToString, |_, _, ()| Ok("PluginAction"));
    }
}

/// An Explorer item, by its number.
pub(crate) struct LuaExplorerAction(u64);

impl UserData for LuaExplorerAction {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("Activated", |_, this| {
            Ok(LuaSignal(SignalKey::ExplorerActivated(this.0)))
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("Remove", |lua, this, ()| remove(lua, this.0));
        methods.add_meta_method(MetaMethod::ToString, |_, _, ()| Ok("PluginExplorerAction"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcuts_read_with_ctrl_or_alt_or_as_a_function_key() {
        let shortcut = Shortcut::parse("ctrl + shift + a").unwrap();
        assert!(shortcut.ctrl && shortcut.shift && !shortcut.alt);
        assert_eq!(shortcut.text(), "Ctrl+Shift+A");
        assert_eq!(Shortcut::parse("Cmd-7").unwrap().text(), "Ctrl+7");
        assert_eq!(Shortcut::parse("F9").unwrap().text(), "F9");
        assert_eq!(
            Shortcut::parse("Alt+Shift+F12").unwrap().text(),
            "Shift+Alt+F12"
        );
        assert!(
            Shortcut::parse("Shift+A")
                .unwrap_err()
                .contains("needs Ctrl or Alt")
        );
        for bad in ["", "Ctrl+", "Ctrl+Space", "Hyper+A", "F13", "Ctrl+AB"] {
            assert!(Shortcut::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn explorer_items_are_offered_by_class() {
        let item = ExplorerAction {
            id: 1,
            label: "Snap".into(),
            tooltip: String::new(),
            class: Some("BasePart".into()),
        };
        assert!(item.offered_for(ClassName::Part) && item.offered_for(ClassName::MeshPart));
        assert!(!item.offered_for(ClassName::Model));
        let any = ExplorerAction {
            class: None,
            ..item
        };
        assert!(any.offered_for(ClassName::Model));
    }
}
