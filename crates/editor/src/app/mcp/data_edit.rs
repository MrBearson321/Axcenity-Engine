//! `data_edit`: the project's data tables (`assets/data/<name>.data.json`,
//! `docs/project/data-tables.md`), which scripts read with
//! `DataService:GetTable`. `list` names them; `show` gives a table's
//! columns and rows in the shape `set` takes; `create` makes one; `set`
//! adds or changes columns and rows (`null` removes one). A write goes
//! through the file's own reader, so a wrong value is refused saying
//! which row and column; it's saved at once (the Data Table tab and Play
//! read it again) and is one undo step. A table with unsaved changes in
//! the tab is left alone.

use axcenity_assets::{AssetType, Cell, ColumnType, DataTable, data_table_name, data_table_path};
use axcenity_mcp::{Args, Class, Code, Kind, Outcome, Param, Profile, ToolError, ToolSpec};
use serde_json::{Map, Value as Json, json};

use super::super::data_tables::table_files;
use super::ops::checkpoint;
use super::refs::quoted;
use super::{Host, LIMIT};
use crate::history::TextChange;

pub const SPEC: ToolSpec = ToolSpec {
    name: "data_edit",
    title: "Edit data tables",
    summary: "List, show, create or change data tables, e.g. {\"action\":\"set\",\"table\":\"weapons\",\"rows\":{\"sword\":{\"Damage\":12}}}.",
    profile: Profile::Full,
    class: Class::Modify,
    idempotent: true,
    params: &[
        Param::optional(
            "action",
            Kind::Enum(&["list", "show", "create", "set"]),
            "default list",
        ),
        Param::optional("table", Kind::String, "name, e.g. weapons"),
        Param::optional(
            "columns",
            Kind::Object,
            "{\"Damage\":\"number\",\"Kind\":\"enum:Melee,Ranged\"}; null removes",
        ),
        Param::optional(
            "rows",
            Kind::Object,
            "{\"sword\":{\"Damage\":12}}; null removes",
        ),
        Param::optional("keys", Kind::Strings, "show only these rows"),
        LIMIT,
    ],
};

/// Rows `show` gives when the client gives no limit.
const DEFAULT_ROWS: usize = 50;

/// The column types, as `columns` takes them.
const TYPES: &str = "number, integer, string, boolean, vector3, color3, enum:A,B, \
asset:model|texture|audio|material|prefab|clip, row:<table>";

pub fn call(host: &mut Host, args: &Args) -> Outcome {
    let Some(folder) = host
        .app
        .project
        .as_ref()
        .map(|project| project.folder.clone())
    else {
        return Err(ToolError::not_found("no project is open"));
    };
    let action = args.str("action").unwrap_or("list");
    if action == "list" {
        return Ok(list(host, &folder));
    }
    let Some(name) = args
        .str("table")
        .map(|name| name.trim().trim_end_matches(".data.json").to_owned())
    else {
        return Err(ToolError::bad_args(format!("{action} needs table"))
            .with_fix("action list names the tables"));
    };
    let path = data_table_path(&name);
    if data_table_name(&path) != Some(name.as_str())
        || name.is_empty()
        || axcenity_assets::safe_path(&path).is_none()
        || name
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(ToolError::new(
            Code::BadValue,
            format!("{} isn't a table name", quoted(&name)),
        )
        .with_fix("a name like weapons or enemies/bosses"));
    }
    let before = std::fs::read_to_string(folder.join(&path)).ok();
    let table = match &before {
        Some(text) => Some(DataTable::from_json(text).map_err(|error| {
            ToolError::new(Code::BadValue, format!("{path} can't be read: {error}"))
                .with_fix("project_check assets lists its fixes")
        })?),
        None => None,
    };
    if action == "show" {
        let Some(table) = table else {
            return Err(missing(&name));
        };
        let keys = args.strings("keys");
        return Ok(show(
            &name,
            &table,
            keys.as_deref(),
            args.limit(DEFAULT_ROWS, 500),
        ));
    }
    if host.app.data_tables.unsaved_paths().contains(&path) {
        return Err(ToolError::new(
            Code::Locked,
            format!("the Data Table tab has unsaved changes to {name}"),
        )
        .with_fix("project_save writes them; then try again"));
    }
    let mut file = match (action, &table) {
        ("create", Some(_)) => {
            return Err(ToolError::new(
                Code::BadValue,
                format!("there's already a table {}", quoted(&name)),
            )
            .with_fix("use action set to change it"));
        }
        ("create", None) => json!({"columns": [], "rows": []}),
        ("set", Some(table)) => {
            serde_json::from_str(&table.to_json()).expect("a table's own text is JSON")
        }
        ("set", None) => return Err(missing(&name)),
        _ => unreachable!("Args::check allows only the actions in SPEC"),
    };
    let columns = object(args, "columns")?;
    let rows = object(args, "rows")?;
    if action == "set" && columns.is_none() && rows.is_none() {
        return Err(ToolError::bad_args("set needs columns or rows"));
    }
    let mut done = Vec::new();
    if let Some(columns) = columns {
        change_columns(&mut file, columns, &mut done)?;
    }
    if let Some(rows) = rows {
        change_rows(&mut file, rows, &mut done)?;
    }
    let text = serde_json::to_string(&file).expect("plain JSON");
    let table = DataTable::from_json(&text).map_err(|error| {
        ToolError::new(Code::BadValue, format!("{name}: {error}")).with_fix(format!(
            "action show {} gives its columns and their types",
            quoted(&name)
        ))
    })?;
    let after = table.to_json();
    if before.as_deref() == Some(after.as_str()) {
        return Ok(format!("{name} is already like that · nothing changed"));
    }
    checkpoint(host, &folder);
    axcenity_project::write_script(&folder, &path, &after)
        .map_err(|error| ToolError::internal(format!("could not write {path}: {error}")))?;
    let app = &mut host.app;
    app.disk_changed(std::slice::from_ref(&path));
    app.asset_browser.refresh();
    if !host.session.files.contains(&path) {
        host.session.files.push(path.clone());
    }
    let verb = if before.is_none() {
        "New table"
    } else {
        "Table"
    };
    let undo = format!("AI ({}): {verb} {name}", host.session.client);
    let app = &mut host.app;
    app.history.record_asset(
        &mut app.world,
        undo.clone(),
        vec![TextChange {
            path: path.clone(),
            before,
            after: Some(after),
        }],
        Vec::new(),
    );
    app.history.seal();
    host.session.last_undo = Some(undo.clone());
    let mut out = if action == "create" {
        format!("created {path} · scripts read it with DataService:GetTable(\"{name}\")")
    } else {
        format!("changed {path}")
    };
    if !done.is_empty() {
        out.push_str(&format!(": {}", done.join(", ")));
    }
    for warning in missing_assets(&folder, &table).into_iter().take(5) {
        out.push_str(&format!("\nwarning: {warning}"));
    }
    out.push_str(&format!(
        "\n{} rows · columns: {}",
        table.rows.len(),
        column_list(&table)
    ));
    out.push_str(&format!("\nundo {}", quoted(&undo)));
    Ok(out)
}

fn missing(name: &str) -> ToolError {
    ToolError::not_found(format!("there's no table {}", quoted(name)))
        .with_fix("action list names the tables; create makes one")
}

/// `name`'s object, `None` when not given.
fn object<'a>(args: &'a Args, name: &str) -> Result<Option<&'a Map<String, Json>>, ToolError> {
    match args.get(name) {
        None | Some(Json::Null) => Ok(None),
        Some(Json::Object(map)) if !map.is_empty() => Ok(Some(map)),
        Some(Json::Object(_)) => Err(ToolError::bad_args(format!("{name} is empty"))),
        Some(_) => Err(ToolError::bad_args(format!("{name} is an object"))),
    }
}

/// Every table, its rows and columns.
fn list(host: &Host, folder: &std::path::Path) -> String {
    let files = table_files(folder);
    let unsaved = host.app.data_tables.unsaved_paths();
    if files.is_empty() {
        return "no data tables · create makes one".to_owned();
    }
    let mut out = format!(
        "{} table{}",
        files.len(),
        if files.len() == 1 { "" } else { "s" }
    );
    for path in &files {
        let name = data_table_name(path).unwrap_or(path);
        let read = std::fs::read_to_string(folder.join(path))
            .map_err(|error| error.to_string())
            .and_then(|text| DataTable::from_json(&text).map_err(|error| error.to_string()));
        out.push_str(&format!("\n{name}"));
        match read {
            Ok(table) => out.push_str(&format!(
                " · {} rows · {}",
                table.rows.len(),
                column_list(&table)
            )),
            Err(why) => out.push_str(&format!(" · can't be read: {why}")),
        }
        if unsaved.contains(path) {
            out.push_str(" · unsaved changes in the Data Table tab");
        }
    }
    out
}

/// The columns and rows of `table`, a row per line in `set`'s shape.
fn show(name: &str, table: &DataTable, keys: Option<&[&str]>, limit: usize) -> String {
    let mut out = format!(
        "{name} · {} rows · columns: {}",
        table.rows.len(),
        column_list(table)
    );
    let rows: Vec<_> = table
        .rows
        .iter()
        .filter(|row| keys.is_none_or(|keys| keys.contains(&row.key.as_str())))
        .collect();
    for row in rows.iter().take(limit) {
        let cells: Map<String, Json> = table
            .columns
            .iter()
            .map(|column| (column.name.clone(), cell_json(&row.cell(column))))
            .collect();
        out.push_str(&format!(
            "\n{} {}",
            row.key,
            serde_json::to_string(&cells).expect("plain JSON")
        ));
    }
    if rows.len() > limit {
        out.push_str(&format!(
            "\n…{} more rows; give keys or a bigger limit",
            rows.len() - limit
        ));
    }
    if let Some(keys) = keys {
        let unknown: Vec<&str> = keys
            .iter()
            .copied()
            .filter(|key| table.row(key).is_none())
            .collect();
        if !unknown.is_empty() {
            out.push_str(&format!("\nno rows {}", unknown.join(", ")));
        }
    }
    out
}

fn cell_json(cell: &Cell) -> Json {
    match cell {
        Cell::Number(n) => axcenity_mcp::text::number(*n)
            .parse::<serde_json::Number>()
            .map_or(Json::Null, Json::Number),
        Cell::Integer(n) => json!(n),
        Cell::Text(text) => json!(text),
        Cell::Bool(b) => json!(b),
        Cell::Vector3(v) | Cell::Color3(v) => {
            Json::Array(v.iter().map(|&n| cell_json(&Cell::Number(n))).collect())
        }
    }
}

/// "Damage number, Kind enum:Melee,Ranged, …", as `columns` takes them.
fn column_list(table: &DataTable) -> String {
    if table.columns.is_empty() {
        return "none".to_owned();
    }
    table
        .columns
        .iter()
        .map(|column| format!("{} {}", column.name, type_text(&column.kind)))
        .collect::<Vec<_>>()
        .join(", ")
}

fn type_text(kind: &ColumnType) -> String {
    match kind {
        ColumnType::Number => "number".to_owned(),
        ColumnType::Integer => "integer".to_owned(),
        ColumnType::String => "string".to_owned(),
        ColumnType::Boolean => "boolean".to_owned(),
        ColumnType::Vector3 => "vector3".to_owned(),
        ColumnType::Color3 => "color3".to_owned(),
        ColumnType::Enum { options } => format!("enum:{}", options.join(",")),
        ColumnType::Asset { asset } => format!("asset:{}", asset_text(*asset)),
        ColumnType::Reference { table } => format!("row:{table}"),
    }
}

fn asset_text(asset: AssetType) -> &'static str {
    match asset {
        AssetType::Model => "model",
        AssetType::Texture => "texture",
        AssetType::Audio => "audio",
        AssetType::Material => "material",
        AssetType::Prefab => "prefab",
        AssetType::Clip => "clip",
    }
}

/// A column's type from `text` ("number", "enum:A,B", "row:ammo"), as
/// the file writes it.
fn column_type(name: &str, text: &str) -> Result<Json, ToolError> {
    let text = text.trim();
    let (head, rest) = text.split_once(':').unwrap_or((text, ""));
    let rest = rest.trim();
    let kind = match head.to_ascii_lowercase().as_str() {
        plain @ ("number" | "integer" | "string" | "boolean" | "vector3" | "color3")
            if rest.is_empty() =>
        {
            json!({"type": plain})
        }
        "enum" if !rest.is_empty() => {
            let options: Vec<&str> = rest
                .split([',', '|'])
                .map(str::trim)
                .filter(|option| !option.is_empty())
                .collect();
            json!({"type": "enum", "options": options})
        }
        "asset" => match AssetType::ALL
            .into_iter()
            .find(|asset| asset_text(*asset) == rest.to_ascii_lowercase())
        {
            Some(asset) => json!({"type": "asset", "asset": asset_text(asset)}),
            None => return Err(bad_type(name, text)),
        },
        "row" | "reference" if !rest.is_empty() => json!({"type": "reference", "table": rest}),
        _ => return Err(bad_type(name, text)),
    };
    Ok(kind)
}

fn bad_type(name: &str, text: &str) -> ToolError {
    ToolError::new(
        Code::BadValue,
        format!("column {}'s type {} isn't one", quoted(name), quoted(text)),
    )
    .with_fix(format!("types: {TYPES}"))
}

/// Adds, retypes or (null) removes `columns` in the file's JSON.
fn change_columns(
    file: &mut Json,
    columns: &Map<String, Json>,
    done: &mut Vec<String>,
) -> Result<(), ToolError> {
    let list = file["columns"].as_array_mut().expect("columns is a list");
    for (name, value) in columns {
        let name = name.trim();
        if name.is_empty() {
            return Err(ToolError::bad_args("a column has no name"));
        }
        let at = list.iter().position(|column| column["name"] == *name);
        match value {
            Json::Null => {
                if let Some(at) = at {
                    list.remove(at);
                    done.push(format!("column {name} removed"));
                }
            }
            Json::String(text) => {
                let mut column = column_type(name, text)?;
                column["name"] = json!(name);
                match at {
                    Some(at) if list[at] == column => {}
                    Some(at) => {
                        list[at] = column;
                        done.push(format!("column {name} is {}", text.trim()));
                    }
                    None => {
                        list.push(column);
                        done.push(format!("column {name} {}", text.trim()));
                    }
                }
            }
            _ => {
                return Err(ToolError::new(
                    Code::BadValue,
                    format!("column {}'s type is text, such as \"number\"", quoted(name)),
                )
                .with_fix(format!("types: {TYPES}")));
            }
        }
    }
    // A removed column's cells go with it.
    let names: Vec<String> = list
        .iter()
        .filter_map(|column| column["name"].as_str().map(str::to_owned))
        .collect();
    for row in file["rows"].as_array_mut().expect("rows is a list") {
        if let Some(cells) = row["cells"].as_object_mut() {
            cells.retain(|column, _| names.contains(column));
        }
    }
    Ok(())
}

/// Adds or changes (only the cells given) or (null) removes `rows`.
fn change_rows(
    file: &mut Json,
    rows: &Map<String, Json>,
    done: &mut Vec<String>,
) -> Result<(), ToolError> {
    let names: Vec<String> = file["columns"]
        .as_array()
        .expect("columns is a list")
        .iter()
        .filter_map(|column| column["name"].as_str().map(str::to_owned))
        .collect();
    let list = file["rows"].as_array_mut().expect("rows is a list");
    let (mut added, mut changed, mut removed) = (Vec::new(), Vec::new(), Vec::new());
    for (key, value) in rows {
        let key = key.trim();
        if key.is_empty() {
            return Err(ToolError::bad_args("a row has no key"));
        }
        let at = list.iter().position(|row| row["key"] == *key);
        let cells = match value {
            Json::Null => {
                if let Some(at) = at {
                    list.remove(at);
                    removed.push(key.to_owned());
                }
                continue;
            }
            Json::Object(cells) => cells,
            _ => {
                return Err(ToolError::new(
                    Code::BadValue,
                    format!("row {}'s cells are an object", quoted(key)),
                )
                .with_fix("e.g. {\"sword\":{\"Damage\":12}}; null removes the row"));
            }
        };
        if let Some(unknown) = cells.keys().find(|column| !names.contains(column)) {
            return Err(ToolError::new(
                Code::BadValue,
                format!("there's no column {}", quoted(unknown)),
            )
            .with_fix(format!(
                "columns: {}; add one with columns {{\"{unknown}\":\"number\"}}",
                if names.is_empty() {
                    "none".to_owned()
                } else {
                    names.join(", ")
                }
            )));
        }
        let row = match at {
            Some(at) => {
                changed.push(key.to_owned());
                &mut list[at]
            }
            None => {
                added.push(key.to_owned());
                list.push(json!({"key": key, "cells": {}}));
                list.last_mut().expect("just pushed")
            }
        };
        if !row["cells"].is_object() {
            row["cells"] = json!({});
        }
        let row_cells = row["cells"].as_object_mut().expect("made an object");
        for (column, cell) in cells {
            row_cells.insert(column.clone(), cell.clone());
        }
    }
    for (verb, keys) in [("added", added), ("changed", changed), ("removed", removed)] {
        if !keys.is_empty() {
            done.push(format!("{verb} {}", keys.join(", ")));
        }
    }
    Ok(())
}

/// Asset cells naming a file the project hasn't got.
fn missing_assets(folder: &std::path::Path, table: &DataTable) -> Vec<String> {
    let mut out = Vec::new();
    for column in &table.columns {
        let ColumnType::Asset { asset } = &column.kind else {
            continue;
        };
        for row in &table.rows {
            let Cell::Text(path) = row.cell(column) else {
                continue;
            };
            if path.is_empty() {
                continue;
            }
            if !folder.join(&path).is_file() {
                out.push(format!(
                    "{}'s {} {} isn't in the project",
                    row.key,
                    column.name,
                    quoted(&path)
                ));
            } else if !asset.matches(&path) {
                out.push(format!(
                    "{}'s {} {} isn't a {} file",
                    row.key,
                    column.name,
                    quoted(&path),
                    asset_text(*asset)
                ));
            }
        }
    }
    out
}
