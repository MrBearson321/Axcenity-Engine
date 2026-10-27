//! Data tables: rows of typed values a game reads from its scripts
//! (`DataService:GetTable("weapons")`), such as each weapon's damage and
//! icon. One table is one file, `assets/data/weapons.data.json`, holding
//! its columns (the schema) and its rows, each with a unique key.
//!
//! ```json
//! {
//!   "columns": [
//!     { "name": "Damage", "type": "number" },
//!     { "name": "Icon", "type": "asset", "asset": "texture" },
//!     { "name": "Ammo", "type": "reference", "table": "ammo" }
//!   ],
//!   "rows": [
//!     { "key": "sword", "cells": { "Ammo": "", "Damage": 10, "Icon": "assets/textures/sword.png" } }
//!   ]
//! }
//! ```
//!
//! A cell the file leaves out has its column's default (0, `""`, false,
//! the first enum option). [`Assets::data_table`](crate::Assets::data_table)
//! reads a table by its name and keeps it; live reload swaps a changed one.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Number, Value as Json};

use crate::{AUDIO_EXTENSIONS, ModelFormat, TEXTURE_EXTENSIONS, is_material_file};

/// The end of a data table's file name (`weapons.data.json`).
pub const DATA_TABLE_SUFFIX: &str = ".data.json";

/// The folder data tables are in. A table's name is its path in here
/// without the suffix: `weapons`, or `enemies/bosses`.
pub const DATA_FOLDER: &str = "assets/data";

/// Whether the file at `path` is a data table, by its name.
pub fn is_data_table(path: &str) -> bool {
    path.to_lowercase().ends_with(DATA_TABLE_SUFFIX)
}

/// The project path of the table named `name`.
pub fn data_table_path(name: &str) -> String {
    format!("{DATA_FOLDER}/{name}{DATA_TABLE_SUFFIX}")
}

/// The name scripts use for the table at `path`, if it's a table in
/// [`DATA_FOLDER`].
pub fn data_table_name(path: &str) -> Option<&str> {
    let inside = path.strip_prefix(DATA_FOLDER)?.strip_prefix('/')?;
    let cut = inside.len().checked_sub(DATA_TABLE_SUFFIX.len())?;
    (inside.is_char_boundary(cut) && inside[cut..].eq_ignore_ascii_case(DATA_TABLE_SUFFIX))
        .then(|| &inside[..cut])
        .filter(|name| !name.is_empty())
}

/// The kind of file an asset column's cells name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AssetType {
    Model,
    Texture,
    Audio,
    Material,
    Prefab,
    Clip,
}

impl AssetType {
    pub const ALL: [AssetType; 6] = [
        AssetType::Model,
        AssetType::Texture,
        AssetType::Audio,
        AssetType::Material,
        AssetType::Prefab,
        AssetType::Clip,
    ];

    pub fn label(self) -> &'static str {
        match self {
            AssetType::Model => "Model",
            AssetType::Texture => "Texture",
            AssetType::Audio => "Sound",
            AssetType::Material => "Material",
            AssetType::Prefab => "Prefab",
            AssetType::Clip => "Animation clip",
        }
    }

    /// Whether `path` names this kind of file, by its name.
    pub fn matches(self, path: &str) -> bool {
        let lower = path.to_lowercase();
        let extension = lower
            .rsplit_once('.')
            .map_or("", |(_, extension)| extension);
        match self {
            AssetType::Model => ModelFormat::of(Path::new(path)).is_some(),
            AssetType::Texture => TEXTURE_EXTENSIONS.contains(&extension),
            AssetType::Audio => AUDIO_EXTENSIONS.contains(&extension),
            AssetType::Material => is_material_file(path),
            AssetType::Prefab => lower.ends_with(".prefab.json"),
            AssetType::Clip => lower.ends_with(".anim.json"),
        }
    }
}

/// What a column holds.
///
/// `#[serde(tag = "type")]` writes the variant as a `"type"` field beside
/// its own fields: `{ "type": "enum", "options": ["Melee", "Ranged"] }`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ColumnType {
    Number,
    /// A whole number.
    Integer,
    String,
    Boolean,
    Vector3,
    /// Red, green and blue, 0 to 1.
    Color3,
    /// One of a list of names.
    Enum {
        options: Vec<String>,
    },
    /// The project path of an asset file of a kind.
    Asset {
        asset: AssetType,
    },
    /// The key of a row in another table (by its name), or `""` for none.
    Reference {
        table: String,
    },
}

impl ColumnType {
    /// What it's called in the editor.
    pub fn label(&self) -> String {
        match self {
            ColumnType::Number => "Number".to_owned(),
            ColumnType::Integer => "Integer".to_owned(),
            ColumnType::String => "String".to_owned(),
            ColumnType::Boolean => "Boolean".to_owned(),
            ColumnType::Vector3 => "Vector3".to_owned(),
            ColumnType::Color3 => "Color3".to_owned(),
            ColumnType::Enum { .. } => "Enum".to_owned(),
            ColumnType::Asset { asset } => format!("{} path", asset.label()),
            ColumnType::Reference { table } => format!("Row of {table}"),
        }
    }

    /// The value of a cell this column's rows leave out.
    pub fn default_cell(&self) -> Cell {
        match self {
            ColumnType::Number => Cell::Number(0.0),
            ColumnType::Integer => Cell::Integer(0),
            ColumnType::Boolean => Cell::Bool(false),
            ColumnType::Vector3 => Cell::Vector3([0.0; 3]),
            ColumnType::Color3 => Cell::Color3([1.0; 3]),
            ColumnType::Enum { options } => {
                Cell::Text(options.first().cloned().unwrap_or_default())
            }
            ColumnType::String | ColumnType::Asset { .. } | ColumnType::Reference { .. } => {
                Cell::Text(String::new())
            }
        }
    }
}

/// One column: its name, which scripts read a row's value by, and type.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Column {
    pub name: String,
    /// `flatten` writes the type's fields beside `name` in one object.
    #[serde(flatten)]
    pub kind: ColumnType,
}

/// One value in a row. Strings, enum names, asset paths and references
/// are all text; the column says which.
#[derive(Clone, Debug, PartialEq)]
pub enum Cell {
    Number(f64),
    Integer(i64),
    Text(String),
    Bool(bool),
    Vector3([f64; 3]),
    Color3([f64; 3]),
}

impl Cell {
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Cell::Text(text) => Some(text),
            _ => None,
        }
    }
}

/// One row: its key and a cell per column, by the column's name.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Row {
    pub key: String,
    pub cells: BTreeMap<String, Cell>,
}

impl Row {
    /// The row's value in `column`, or the column's default.
    pub fn cell(&self, column: &Column) -> Cell {
        self.cells
            .get(&column.name)
            .cloned()
            .unwrap_or_else(|| column.kind.default_cell())
    }
}

/// A data table: its columns, then its rows in order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DataTable {
    pub columns: Vec<Column>,
    pub rows: Vec<Row>,
}

#[derive(Debug, thiserror::Error)]
pub enum DataTableError {
    #[error("could not read it")]
    Read(#[source] io::Error),
    #[error("could not write it")]
    Write(#[source] io::Error),
    #[error("it isn't a data table")]
    Format(#[source] serde_json::Error),
    /// The file reads, but says something a table can't hold, such as
    /// two rows with one key.
    #[error("{0}")]
    Invalid(String),
}

/// The file as JSON shapes, before each cell is read by its column.
#[derive(Default, Serialize, Deserialize)]
struct TableFile {
    #[serde(default)]
    columns: Vec<Column>,
    #[serde(default)]
    rows: Vec<RowFile>,
}

#[derive(Serialize, Deserialize)]
struct RowFile {
    key: String,
    #[serde(default)]
    cells: Map<String, Json>,
}

impl DataTable {
    /// The table in `text`, a `*.data.json` file.
    pub fn from_json(text: &str) -> Result<DataTable, DataTableError> {
        let file: TableFile = serde_json::from_str(text).map_err(DataTableError::Format)?;
        let mut table = DataTable {
            columns: file.columns,
            rows: Vec::new(),
        };
        table.check_columns().map_err(DataTableError::Invalid)?;
        for row in file.rows {
            if row.key.is_empty() {
                return Err(DataTableError::Invalid("a row has no key".to_owned()));
            }
            if table.row(&row.key).is_some() {
                return Err(DataTableError::Invalid(format!(
                    "two rows have the key `{}`",
                    row.key
                )));
            }
            let mut cells = BTreeMap::new();
            for column in &table.columns {
                let Some(json) = row.cells.get(&column.name).filter(|json| !json.is_null()) else {
                    continue;
                };
                let cell = read_cell(&column.kind, json).ok_or_else(|| {
                    DataTableError::Invalid(format!(
                        "row `{}`'s `{}` must be {}, not {json}",
                        row.key,
                        column.name,
                        expected(&column.kind)
                    ))
                })?;
                if let (ColumnType::Enum { options }, Cell::Text(name)) = (&column.kind, &cell)
                    && !options.contains(name)
                {
                    return Err(DataTableError::Invalid(format!(
                        "row `{}`'s `{}` is `{name}`, which isn't one of its options ({})",
                        row.key,
                        column.name,
                        options.join(", ")
                    )));
                }
                cells.insert(column.name.clone(), cell);
            }
            table.rows.push(Row {
                key: row.key,
                cells,
            });
        }
        Ok(table)
    }

    /// The table as a file's text. Every cell is written, in name order,
    /// so the same table is always the same text.
    pub fn to_json(&self) -> String {
        let file = TableFile {
            columns: self.columns.clone(),
            rows: self
                .rows
                .iter()
                .map(|row| RowFile {
                    key: row.key.clone(),
                    cells: self
                        .columns
                        .iter()
                        .map(|column| (column.name.clone(), write_cell(&row.cell(column))))
                        .collect(),
                })
                .collect(),
        };
        let text = serde_json::to_string_pretty(&file)
            .expect("a data table is plain values, which always serialize");
        format!("{text}\n")
    }

    /// A column's name is never empty, and no two are the same.
    fn check_columns(&self) -> Result<(), String> {
        for (index, column) in self.columns.iter().enumerate() {
            if column.name.trim().is_empty() {
                return Err(format!("column {} has no name", index + 1));
            }
            if self.columns[..index]
                .iter()
                .any(|other| other.name == column.name)
            {
                return Err(format!("two columns are named `{}`", column.name));
            }
        }
        Ok(())
    }

    pub fn column(&self, name: &str) -> Option<&Column> {
        self.columns.iter().find(|column| column.name == name)
    }

    pub fn row(&self, key: &str) -> Option<&Row> {
        self.rows.iter().find(|row| row.key == key)
    }

    pub fn row_mut(&mut self, key: &str) -> Option<&mut Row> {
        self.rows.iter_mut().find(|row| row.key == key)
    }

    /// `base`, or `base2`, `base3`… : the first that no row has as its key.
    pub fn unused_key(&self, base: &str) -> String {
        unused(base, |key| self.row(key).is_some())
    }

    /// `base`, or `base2`, `base3`… : the first that no column is named.
    pub fn unused_column_name(&self, base: &str) -> String {
        unused(base, |name| self.column(name).is_some())
    }

    /// Adds a column at the end; every row has its default.
    pub fn add_column(&mut self, name: &str, kind: ColumnType) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("a column needs a name".to_owned());
        }
        if self.column(name).is_some() {
            return Err(format!("there's already a column named `{name}`"));
        }
        self.columns.push(Column {
            name: name.to_owned(),
            kind,
        });
        Ok(())
    }

    /// Renames a column, keeping every row's value in it.
    pub fn rename_column(&mut self, old: &str, new: &str) -> Result<(), String> {
        let new = new.trim();
        if new.is_empty() {
            return Err("a column needs a name".to_owned());
        }
        if new == old {
            return Ok(());
        }
        if self.column(new).is_some() {
            return Err(format!("there's already a column named `{new}`"));
        }
        let column = self
            .columns
            .iter_mut()
            .find(|column| column.name == old)
            .ok_or_else(|| format!("there's no column named `{old}`"))?;
        column.name = new.to_owned();
        for row in &mut self.rows {
            if let Some(cell) = row.cells.remove(old) {
                row.cells.insert(new.to_owned(), cell);
            }
        }
        Ok(())
    }

    /// Moves the column at `from` to `to` (both indexes in the columns).
    pub fn move_column(&mut self, from: usize, to: usize) {
        if from < self.columns.len() && to < self.columns.len() {
            let column = self.columns.remove(from);
            self.columns.insert(to, column);
        }
    }

    /// Removes a column and every row's value in it.
    pub fn remove_column(&mut self, name: &str) {
        self.columns.retain(|column| column.name != name);
        for row in &mut self.rows {
            row.cells.remove(name);
        }
    }

    /// Adds a row of defaults with an unused key based on `base`, after
    /// the row `after` (or at the end). Returns its key.
    pub fn add_row(&mut self, base: &str, after: Option<&str>) -> String {
        let key = self.unused_key(base);
        let row = Row {
            key: key.clone(),
            cells: BTreeMap::new(),
        };
        self.insert_row(row, after);
        key
    }

    /// Copies the row `key` just below it, with a new key. Returns that key.
    pub fn duplicate_row(&mut self, key: &str) -> Option<String> {
        let mut row = self.row(key)?.clone();
        row.key = self.unused_key(key);
        let new = row.key.clone();
        self.insert_row(row, Some(key));
        Some(new)
    }

    fn insert_row(&mut self, row: Row, after: Option<&str>) {
        let at = after
            .and_then(|after| self.rows.iter().position(|row| row.key == after))
            .map_or(self.rows.len(), |index| index + 1);
        self.rows.insert(at, row);
    }

    pub fn remove_row(&mut self, key: &str) {
        self.rows.retain(|row| row.key != key);
    }

    /// Gives the row `old` the key `new`.
    pub fn rename_row(&mut self, old: &str, new: &str) -> Result<(), String> {
        let new = new.trim();
        if new.is_empty() {
            return Err("a row needs a key".to_owned());
        }
        if new == old {
            return Ok(());
        }
        if self.row(new).is_some() {
            return Err(format!("there's already a row with the key `{new}`"));
        }
        let row = self
            .row_mut(old)
            .ok_or_else(|| format!("there's no row `{old}`"))?;
        row.key = new.to_owned();
        Ok(())
    }

    /// Sets the row `key`'s value in `column`.
    pub fn set_cell(&mut self, key: &str, column: &str, cell: Cell) -> Result<(), String> {
        if self.column(column).is_none() {
            return Err(format!("there's no column named `{column}`"));
        }
        let row = self
            .row_mut(key)
            .ok_or_else(|| format!("there's no row `{key}`"))?;
        row.cells.insert(column.to_owned(), cell);
        Ok(())
    }
}

fn unused(base: &str, taken: impl Fn(&str) -> bool) -> String {
    let base = base.trim();
    let base = if base.is_empty() { "row" } else { base };
    if !taken(base) {
        return base.to_owned();
    }
    // `sword2` copied again is `sword3`, not `sword22`.
    let stem = base.trim_end_matches(|c: char| c.is_ascii_digit());
    let stem = if stem.is_empty() { base } else { stem };
    (2..)
        .map(|number| format!("{stem}{number}"))
        .find(|key| !taken(key))
        .expect("some number is free")
}

/// What a cell of `kind` must be, for an error.
fn expected(kind: &ColumnType) -> &'static str {
    match kind {
        ColumnType::Number => "a number",
        ColumnType::Integer => "a whole number",
        ColumnType::Boolean => "true or false",
        ColumnType::Vector3 => "three numbers, [x, y, z]",
        ColumnType::Color3 => "three numbers from 0 to 1, [r, g, b]",
        ColumnType::String
        | ColumnType::Enum { .. }
        | ColumnType::Asset { .. }
        | ColumnType::Reference { .. } => "text",
    }
}

fn read_cell(kind: &ColumnType, json: &Json) -> Option<Cell> {
    let triple = |json: &Json| -> Option<[f64; 3]> {
        match json.as_array()?.as_slice() {
            [x, y, z] => Some([x.as_f64()?, y.as_f64()?, z.as_f64()?]),
            _ => None,
        }
    };
    Some(match kind {
        ColumnType::Number => Cell::Number(json.as_f64()?),
        ColumnType::Integer => Cell::Integer(match json.as_i64() {
            Some(n) => n,
            // `10.0` is whole too.
            None => {
                let n = json.as_f64()?;
                if n.fract() != 0.0 || n.abs() > 9.0e15 {
                    return None;
                }
                n as i64
            }
        }),
        ColumnType::Boolean => Cell::Bool(json.as_bool()?),
        ColumnType::Vector3 => Cell::Vector3(triple(json)?),
        ColumnType::Color3 => Cell::Color3(triple(json)?),
        ColumnType::String
        | ColumnType::Enum { .. }
        | ColumnType::Asset { .. }
        | ColumnType::Reference { .. } => Cell::Text(json.as_str()?.to_owned()),
    })
}

fn write_cell(cell: &Cell) -> Json {
    // A whole number is written without `.0`, as people type it.
    let number = |n: f64| {
        if n.fract() == 0.0 && n.abs() < 9.0e15 {
            Json::Number(Number::from(n as i64))
        } else {
            Number::from_f64(n).map_or(Json::Null, Json::Number)
        }
    };
    match cell {
        Cell::Number(n) => number(*n),
        Cell::Integer(n) => Json::Number(Number::from(*n)),
        Cell::Text(text) => Json::String(text.clone()),
        Cell::Bool(b) => Json::Bool(*b),
        Cell::Vector3(v) | Cell::Color3(v) => Json::Array(v.iter().map(|&n| number(n)).collect()),
    }
}

/// Reads the table file at `path`.
pub fn read_data_table(path: &Path) -> Result<DataTable, DataTableError> {
    let text = fs::read_to_string(path).map_err(DataTableError::Read)?;
    DataTable::from_json(&text)
}

/// Writes `table` to `path`, creating the file's folder if it isn't there.
pub fn write_data_table(path: &Path, table: &DataTable) -> Result<(), DataTableError> {
    if let Some(folder) = path.parent()
        && !folder.as_os_str().is_empty()
    {
        fs::create_dir_all(folder).map_err(DataTableError::Write)?;
    }
    fs::write(path, table.to_json()).map_err(DataTableError::Write)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn weapons() -> DataTable {
        let mut table = DataTable::default();
        table.add_column("Damage", ColumnType::Number).unwrap();
        table.add_column("Range", ColumnType::Integer).unwrap();
        table
            .add_column(
                "Icon",
                ColumnType::Asset {
                    asset: AssetType::Texture,
                },
            )
            .unwrap();
        table
            .add_column(
                "Kind",
                ColumnType::Enum {
                    options: vec!["Melee".into(), "Ranged".into()],
                },
            )
            .unwrap();
        table
            .add_column(
                "Ammo",
                ColumnType::Reference {
                    table: "ammo".into(),
                },
            )
            .unwrap();
        table.add_column("Tint", ColumnType::Color3).unwrap();
        table.add_column("Offset", ColumnType::Vector3).unwrap();
        table.add_column("Rare", ColumnType::Boolean).unwrap();
        table.add_column("Title", ColumnType::String).unwrap();
        let sword = table.add_row("sword", None);
        table
            .set_cell(&sword, "Damage", Cell::Number(10.5))
            .unwrap();
        table.set_cell(&sword, "Range", Cell::Integer(2)).unwrap();
        table
            .set_cell(
                &sword,
                "Icon",
                Cell::Text("assets/textures/sword.png".into()),
            )
            .unwrap();
        table
            .set_cell(&sword, "Offset", Cell::Vector3([1.0, -2.0, 0.25]))
            .unwrap();
        table
    }

    #[test]
    fn a_table_round_trips_through_its_file_with_every_column_type() {
        let table = weapons();
        let text = table.to_json();
        let read = DataTable::from_json(&text).unwrap();
        // Defaults are written out, so the read table has every cell.
        assert_eq!(read.to_json(), text);
        let sword = read.row("sword").unwrap();
        assert_eq!(
            sword.cell(read.column("Damage").unwrap()),
            Cell::Number(10.5)
        );
        assert_eq!(sword.cell(read.column("Range").unwrap()), Cell::Integer(2));
        assert_eq!(
            sword.cell(read.column("Kind").unwrap()),
            Cell::Text("Melee".into())
        );
        assert_eq!(
            sword.cell(read.column("Tint").unwrap()),
            Cell::Color3([1.0; 3])
        );
        assert!(text.contains("\"Range\": 2"), "{text}");
        assert!(text.contains("\"type\": \"asset\""), "{text}");
        assert!(text.contains("\"asset\": \"texture\""), "{text}");
    }

    #[test]
    fn bad_files_say_what_is_wrong() {
        let error = |text: &str| DataTable::from_json(text).unwrap_err().to_string();
        assert_eq!(error("{ nope"), "it isn't a data table");
        let columns = r#""columns": [{ "name": "Damage", "type": "number" }]"#;
        assert_eq!(
            error(&format!(
                r#"{{ {columns}, "rows": [{{ "key": "a" }}, {{ "key": "a" }}] }}"#
            )),
            "two rows have the key `a`"
        );
        assert_eq!(
            error(&format!(
                r#"{{ {columns}, "rows": [{{ "key": "a", "cells": {{ "Damage": "lots" }} }}] }}"#
            )),
            "row `a`'s `Damage` must be a number, not \"lots\""
        );
        assert_eq!(
            error(
                r#"{ "columns": [{ "name": "A", "type": "number" }, { "name": "A", "type": "string" }] }"#
            ),
            "two columns are named `A`"
        );
        let kinds = r#"{ "columns": [{ "name": "Kind", "type": "enum", "options": ["Melee"] }],
            "rows": [{ "key": "a", "cells": { "Kind": "Magic" } }] }"#;
        assert_eq!(
            error(kinds),
            "row `a`'s `Kind` is `Magic`, which isn't one of its options (Melee)"
        );
    }

    #[test]
    fn columns_rename_and_move_and_rows_duplicate_with_new_keys() {
        let mut table = weapons();
        table.rename_column("Damage", "Power").unwrap();
        assert_eq!(
            table.row("sword").unwrap().cells.get("Power"),
            Some(&Cell::Number(10.5))
        );
        assert!(table.rename_column("Power", "Range").is_err());
        table.move_column(1, 0);
        assert_eq!(table.columns[0].name, "Range");
        let copy = table.duplicate_row("sword").unwrap();
        assert_eq!(copy, "sword2");
        assert_eq!(table.duplicate_row("sword2").unwrap(), "sword3");
        assert_eq!(table.rows[1].key, "sword2");
        assert_eq!(table.rows[1].cells, table.rows[0].cells);
        assert!(table.rename_row("sword2", "sword").is_err());
        table.rename_row("sword2", "axe").unwrap();
        assert!(table.row("axe").is_some());
    }

    #[test]
    fn names_and_paths_of_tables() {
        assert_eq!(data_table_path("weapons"), "assets/data/weapons.data.json");
        assert_eq!(
            data_table_name("assets/data/enemies/bosses.data.json"),
            Some("enemies/bosses")
        );
        assert_eq!(data_table_name("assets/other/weapons.data.json"), None);
        assert_eq!(data_table_name("assets/data/.data.json"), None);
        assert!(is_data_table("assets/data/Weapons.DATA.json"));
        assert!(AssetType::Texture.matches("assets/textures/a.PNG"));
        assert!(!AssetType::Texture.matches("assets/models/a.glb"));
        assert!(AssetType::Prefab.matches("assets/prefabs/door.prefab.json"));
    }
}
