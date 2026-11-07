//! `game:GetService("DataService")`: the project's data tables
//! (`assets/data/*.data.json`, read by `axcenity_assets`), read-only.
//!
//! `DataService:GetTable("weapons")` is a handle on the table by name, so
//! each `Get`, `GetAll` and `Keys` reads the version in use now: a table
//! edited in the editor's Data Table tab, or changed on disk, during Play
//! is what the next call sees. A row is a snapshot of one row, read by
//! column name (`row.Damage`); a reference column gives the row it names.
//! Writing to a table or a row is an error. Both sides have it.
//!
//! `CreateAbility(table, key)` and `CreateEffect(table, key)` (Phase 76)
//! make an Ability or Effect from one row: named after its key, each
//! column that names one of the class's properties sets it, and the other
//! columns become attributes (`ability:GetAttribute("Damage")`).

use std::sync::Arc;

use axcenity_assets::{Assets, Cell, ColumnType, DataTable};
use axcenity_core::{ClassName, Color3, EnumItem, InstanceId, Vector3, World};
use mlua::{IntoLua, Lua, MetaMethod, UserData, UserDataMethods, Value};

use crate::instance::{LuaInstance, world_mut};
use crate::types::{LuaColor3, LuaVector3};

/// The DataService, as scripts see it.
#[derive(Clone, Copy)]
pub struct LuaDataService;

/// A data table by name, as `GetTable` returns it.
#[derive(Clone)]
pub struct LuaDataTable {
    name: String,
}

/// One row of a table, as it was when it was read.
#[derive(Clone)]
pub struct LuaDataRow {
    table_name: String,
    table: Arc<DataTable>,
    index: usize,
}

/// The table named `name` in use now, or an error saying why there's none.
fn table(lua: &Lua, name: &str) -> mlua::Result<Arc<DataTable>> {
    let assets = lua
        .app_data_ref::<Assets>()
        .ok_or_else(|| mlua::Error::runtime("data tables need a project, and none is open"))?;
    assets
        .data_table(name)
        .map_err(|why| mlua::Error::runtime(format!("can't read the data table \"{name}\": {why}")))
}

fn read_only(what: &str) -> mlua::Error {
    mlua::Error::runtime(format!(
        "{what} can't be changed: data tables are read-only in scripts. Change them in \
         the Data Table tab."
    ))
}

impl UserData for LuaDataService {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("GetTable", |lua, _, name: String| {
            // Read now, so a wrong name errors here and not at the first Get.
            table(lua, &name)?;
            Ok(LuaDataTable { name })
        });
        methods.add_method("CreateAbility", |lua, _, (name, key): (String, String)| {
            create_from_row(lua, ClassName::Ability, &name, &key)
        });
        methods.add_method("CreateEffect", |lua, _, (name, key): (String, String)| {
            create_from_row(lua, ClassName::Effect, &name, &key)
        });
        methods.add_meta_method(MetaMethod::ToString, |_, _, ()| Ok("DataService"));
    }
}

impl UserData for LuaDataTable {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("Get", |lua, this, key: String| {
            let table = table(lua, &this.name)?;
            Ok(table
                .rows
                .iter()
                .position(|row| row.key == key)
                .map(|index| LuaDataRow {
                    table_name: this.name.clone(),
                    table: table.clone(),
                    index,
                }))
        });
        methods.add_method("GetAll", |lua, this, ()| {
            let table = table(lua, &this.name)?;
            let all = lua.create_table_with_capacity(0, table.rows.len())?;
            for (index, row) in table.rows.iter().enumerate() {
                let row_value = LuaDataRow {
                    table_name: this.name.clone(),
                    table: table.clone(),
                    index,
                };
                all.raw_set(row.key.as_str(), row_value)?;
            }
            Ok(all)
        });
        methods.add_method("Keys", |lua, this, ()| {
            let table = table(lua, &this.name)?;
            lua.create_sequence_from(table.rows.iter().map(|row| row.key.as_str()))
        });
        methods.add_meta_method(
            MetaMethod::NewIndex,
            |_, this, (key, _): (String, Value)| {
                Err::<(), _>(read_only(&format!(
                    "the data table \"{}\" ({key})",
                    this.name
                )))
            },
        );
        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(format!("DataTable {}", this.name))
        });
    }
}

/// A cell as a property or attribute value: `current` (the property's
/// default, if it's one) says which kind, and an enum property takes the
/// option's name.
fn cell_value(cell: &Cell, current: Option<&axcenity_core::Value>) -> Option<axcenity_core::Value> {
    use axcenity_core::Value as V;
    let value = match cell {
        Cell::Number(n) => V::Number(*n),
        Cell::Integer(n) => V::Number(*n as f64),
        Cell::Text(text) => match current {
            Some(V::EnumItem(item)) => V::EnumItem(EnumItem {
                enum_name: item.enum_name.clone(),
                item: text.clone(),
            }),
            _ => V::String(text.clone()),
        },
        Cell::Bool(b) => V::Bool(*b),
        Cell::Vector3([x, y, z]) => V::Vector3(Vector3::new(*x as f32, *y as f32, *z as f32)),
        Cell::Color3([r, g, b]) => V::Color3(Color3 {
            r: *r as f32,
            g: *g as f32,
            b: *b as f32,
        }),
    };
    match current {
        Some(current) if current.kind() != value.kind() => None,
        _ => Some(value),
    }
}

/// Makes a `class` from the row `key` of the table `name`, with no parent.
fn create_from_row(
    lua: &Lua,
    class: ClassName,
    name: &str,
    key: &str,
) -> mlua::Result<LuaInstance> {
    let call = format!("Create{class}(\"{name}\", \"{key}\")");
    let table = table(lua, name)?;
    let row = table.row(key).ok_or_else(|| {
        mlua::Error::runtime(format!("{call}: the table \"{name}\" has no row \"{key}\""))
    })?;
    let mut world = world_mut(lua)?;
    let id = world
        .create(class)
        .map_err(|err| mlua::Error::runtime(format!("{call}: {err}")))?;
    let _ = world.set_name(id, key);
    for column in &table.columns {
        let cell = row.cell(column);
        set_from_cell(&mut world, id, &column.name, &cell).map_err(|why| {
            mlua::Error::runtime(format!("{call}: column \"{}\" {why}", column.name))
        })?;
    }
    Ok(LuaInstance(id))
}

fn set_from_cell(
    world: &mut World,
    id: InstanceId,
    column: &str,
    cell: &Cell,
) -> Result<(), String> {
    let current = world
        .get(id)
        .and_then(|instance| instance.property(column))
        .cloned();
    match current {
        Some(current) => {
            let value = cell_value(cell, Some(&current))
                .ok_or_else(|| format!("doesn't hold a {}", current.kind()))?;
            world
                .set_property(id, column, value)
                .map_err(|err| err.to_string())
        }
        None => {
            let value = cell_value(cell, None).expect("any cell is some value");
            world
                .set_attribute(id, column, Some(value))
                .map_err(|err| err.to_string())
        }
    }
}

impl LuaDataRow {
    fn key(&self) -> &str {
        &self.table.rows[self.index].key
    }

    /// The row's value in the column `name`.
    fn value(&self, lua: &Lua, name: &str) -> mlua::Result<Value> {
        let Some(column) = self.table.column(name) else {
            let names: Vec<&str> = self.table.columns.iter().map(|c| c.name.as_str()).collect();
            return Err(mlua::Error::runtime(format!(
                "the row \"{}\" of \"{}\" has no column \"{name}\"; its columns are {}",
                self.key(),
                self.table_name,
                if names.is_empty() {
                    "none".to_owned()
                } else {
                    names.join(", ")
                }
            )));
        };
        let cell = self.table.rows[self.index].cell(column);
        match (&column.kind, cell) {
            // A reference is the row it names, or nil when it names none.
            (ColumnType::Reference { table: other }, Cell::Text(key)) => {
                if key.is_empty() {
                    return Ok(Value::Nil);
                }
                let referenced = table(lua, other)?;
                match referenced.rows.iter().position(|row| row.key == key) {
                    Some(index) => LuaDataRow {
                        table_name: other.clone(),
                        table: referenced,
                        index,
                    }
                    .into_lua(lua),
                    None => Ok(Value::Nil),
                }
            }
            (_, Cell::Number(n)) => n.into_lua(lua),
            (_, Cell::Integer(n)) => (n as f64).into_lua(lua),
            (_, Cell::Text(text)) => text.into_lua(lua),
            (_, Cell::Bool(b)) => b.into_lua(lua),
            (_, Cell::Vector3([x, y, z])) => {
                LuaVector3(Vector3::new(x as f32, y as f32, z as f32)).into_lua(lua)
            }
            (_, Cell::Color3([r, g, b])) => LuaColor3(Color3 {
                r: r as f32,
                g: g as f32,
                b: b as f32,
            })
            .into_lua(lua),
        }
    }
}

impl UserData for LuaDataRow {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::Index, |lua, this, name: String| {
            this.value(lua, &name)
        });
        methods.add_meta_method(
            MetaMethod::NewIndex,
            |_, this, (name, _): (String, Value)| {
                Err::<(), _>(read_only(&format!(
                    "\"{}\" of the row \"{}\" in \"{}\"",
                    name,
                    this.key(),
                    this.table_name
                )))
            },
        );
        // `for column, value in row do`: each column, in the table's order.
        methods.add_meta_method(MetaMethod::Iter, |lua, this, ()| {
            let columns: Vec<String> = this.table.columns.iter().map(|c| c.name.clone()).collect();
            let row = this.clone();
            let mut next = 0;
            lua.create_function_mut(move |lua, ()| {
                let Some(name) = columns.get(next) else {
                    return Ok((Value::Nil, Value::Nil));
                };
                next += 1;
                Ok((name.as_str().into_lua(lua)?, row.value(lua, name)?))
            })
        });
        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(format!("{}.{}", this.table_name, this.key()))
        });
    }
}
