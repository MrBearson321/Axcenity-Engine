//! Custom Properties rows (roadmap G10): `plugin:AddPropertyRow` shows an
//! attribute of the selected Instance as a row of its own, and
//! `plugin:AddPropertyButton` a button that calls the plugin. Rows are
//! declared, not drawn by Lua: the editor draws them in the Properties
//! panel under the plugin's name, and an edit there is an ordinary
//! attribute edit with its own undo step, so no plugin code runs each
//! frame ([`crate::Plugin::take_rows`]). The editor tells the plugin after
//! an edit (`Changed`, [`crate::Plugin::row_changed`]) and a click
//! (`Activated`, [`crate::Plugin::activate_row`]).

use std::collections::BTreeMap;

use axcenity_core::{ClassName, Value, is_attribute_name};
use axcenity_script::system::attribute_from_lua;
use mlua::{Lua, MetaMethod, Table, UserData, UserDataFields, UserDataMethods};

use crate::api::{LuaSignal, SignalKey, state};

/// How many rows one plugin may add.
pub const MAX_ROWS: usize = 64;

/// A row a plugin added to the Properties panel.
#[derive(Clone, Debug, PartialEq)]
pub struct PropertyRow {
    /// Its number, for [`crate::Plugin::activate_row`].
    pub id: u64,
    /// It shows for Instances that are this class (`IsA`), such as
    /// `"BasePart"`.
    pub class: String,
    pub label: String,
    /// Shown when the mouse is over its name; may be empty.
    pub tooltip: String,
    pub kind: RowKind,
}

impl PropertyRow {
    /// Whether it shows for an Instance of `class`.
    pub fn applies_to(&self, class: ClassName) -> bool {
        class.is_a(&self.class)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum RowKind {
    /// Edits the attribute `name`, which shows `default` until it's set.
    /// Numbers are kept between `min` and `max` when given (a slider when
    /// both are).
    Attribute {
        name: String,
        default: Value,
        min: Option<f64>,
        max: Option<f64>,
    },
    /// A button that fires the row's `Activated` with the Instance.
    Button,
}

/// A plugin's rows, by number, and whether they changed since the host
/// last took them.
#[derive(Default)]
pub(crate) struct Rows {
    pub(crate) rows: BTreeMap<u64, PropertyRow>,
    pub(crate) next: u64,
    pub(crate) changed: bool,
}

/// `Class`, `Label` and `Tooltip`, common to both kinds; `label` is the
/// default label.
fn common(info: &Table, label: Option<String>) -> mlua::Result<(String, String, String)> {
    let class = info
        .get::<Option<String>>("Class")?
        .unwrap_or_else(|| "Instance".to_owned());
    if !ClassName::ALL.iter().any(|known| known.is_a(&class)) {
        return Err(mlua::Error::runtime(format!(
            "Class \"{class}\" isn't a class name, such as \"BasePart\" or \"Model\""
        )));
    }
    let label = match info.get::<Option<String>>("Label")?.or(label) {
        Some(label) if !label.trim().is_empty() => label.chars().take(60).collect(),
        _ => return Err(mlua::Error::runtime("the row needs a Label")),
    };
    let tooltip = info.get::<Option<String>>("Tooltip")?.unwrap_or_default();
    Ok((class, label, tooltip))
}

fn add(lua: &Lua, row: (String, String, String), kind: RowKind) -> mlua::Result<LuaRow> {
    let mut state = state(lua)?;
    let rows = &mut state.rows;
    if rows.rows.len() >= MAX_ROWS {
        return Err(mlua::Error::runtime(format!(
            "a plugin can add at most {MAX_ROWS} Properties rows"
        )));
    }
    rows.next += 1;
    let (class, label, tooltip) = row;
    rows.rows.insert(
        rows.next,
        PropertyRow {
            id: rows.next,
            class,
            label,
            tooltip,
            kind,
        },
    );
    rows.changed = true;
    Ok(LuaRow(rows.next))
}

/// `plugin:AddPropertyRow({ Attribute, Default, Class, Label, Tooltip,
/// Min, Max })`.
pub(crate) fn attribute_row(lua: &Lua, info: Table) -> mlua::Result<LuaRow> {
    let name: String = info
        .get::<Option<String>>("Attribute")?
        .ok_or_else(|| mlua::Error::runtime("the row needs an Attribute name"))?;
    if !is_attribute_name(&name) {
        return Err(mlua::Error::runtime(format!(
            "\"{name}\" isn't an attribute name: 1 to 100 letters, digits and underscores"
        )));
    }
    let default = attribute_from_lua(lua, &info.get("Default")?)
        .map_err(|message| mlua::Error::runtime(format!("Default: {message}")))?
        .ok_or_else(|| mlua::Error::runtime("the row needs a Default value"))?;
    let bound = |key: &str| -> mlua::Result<Option<f64>> {
        Ok(info.get::<Option<f64>>(key)?.filter(|n| n.is_finite()))
    };
    let (min, max) = (bound("Min")?, bound("Max")?);
    if let (Some(min), Some(max)) = (min, max)
        && min > max
    {
        return Err(mlua::Error::runtime("Min is more than Max"));
    }
    let row = common(&info, Some(name.clone()))?;
    add(
        lua,
        row,
        RowKind::Attribute {
            name,
            default,
            min,
            max,
        },
    )
}

/// `plugin:AddPropertyButton({ Label, Class, Tooltip })`.
pub(crate) fn button_row(lua: &Lua, info: Table) -> mlua::Result<LuaRow> {
    let row = common(&info, None)?;
    add(lua, row, RowKind::Button)
}

/// A Properties row, by its number.
pub(crate) struct LuaRow(u64);

impl UserData for LuaRow {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("Activated", |_, this| {
            Ok(LuaSignal(SignalKey::RowActivated(this.0)))
        });
        fields.add_field_method_get("Changed", |_, this| {
            Ok(LuaSignal(SignalKey::RowChanged(this.0)))
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("Remove", |lua, this, ()| {
            let mut state = state(lua)?;
            if state.rows.rows.remove(&this.0).is_some() {
                state.rows.changed = true;
            }
            state.handlers.remove(&SignalKey::RowActivated(this.0));
            state.handlers.remove(&SignalKey::RowChanged(this.0));
            Ok(())
        });
        methods.add_meta_method(MetaMethod::ToString, |_, _, ()| Ok("PluginPropertyRow"));
    }
}
