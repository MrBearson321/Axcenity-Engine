//! `game:GetService("DataService")`: the project's data tables
//! (`assets/data/*.data.json`, read by `axcenity_assets`), read-only.
//!
//! `DataService:GetTable("weapons")` is a handle on the table by name, so
//! each `Get`, `GetAll` and `Keys` reads the version in use now: a table
//! edited in the editor's Data Table tab, or changed on disk, during Play
//! is what the next call sees. A row is a snapshot of one row, read by
//! column name (`row.Damage`); a reference column gives the row it names.
//! Writing to a table or a row is an error. Both sides have it.

use std::sync::Arc;

use axcenity_assets::{Assets, Cell, ColumnType, DataTable};
use axcenity_core::{Color3, Vector3};
use mlua::{IntoLua, Lua, MetaMethod, UserData, UserDataMethods, Value};

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
