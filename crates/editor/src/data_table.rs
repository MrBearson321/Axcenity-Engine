//! The Data Table tab: a data table (`assets/data/<name>.data.json`) as a
//! grid. Each column has a type, so each cell is edited with the control
//! for it: a number box, a check box, a colour, an enum's options, a
//! picker of the project's files of an asset column's kind, or the keys of
//! the table a reference column names.
//!
//! Columns are added, renamed, moved and removed from their headers, and
//! rows added, duplicated and removed from their ends. Sorting (by
//! clicking a header), the search box and the column filter only change
//! what's shown. Each edit is one step of the table's own undo, used while
//! the pointer is over the grid; dragging a number or typing in a cell is
//! one step however many frames it takes.
//!
//! Tables are written with the project's Save. Each edit is also handed to
//! the app ([`DataTableStudio::take_changed`]), which gives it to the
//! shared `Assets`, so a game in Play reads it at its next call.

use std::cmp::Ordering;
use std::path::Path;

use axcenity_assets::{
    AssetType, Cell, Column, ColumnType, DataTable, data_table_name, read_data_table,
    write_data_table,
};
use eframe::egui;

use crate::responsive;
use crate::theme::{self, icons};

/// The width of a text or picker cell.
const CELL_WIDTH: f32 = 140.0;
/// The width of a row's key.
const KEY_WIDTH: f32 = 100.0;

/// One table, as edited.
#[derive(Debug)]
pub struct TableDocument {
    pub path: String,
    pub table: DataTable,
    /// The table as last read or written; `None` before it has a file.
    saved: Option<DataTable>,
    /// The table before each edit, newest last, and the ones undone.
    undo: Vec<DataTable>,
    redo: Vec<DataTable>,
}

impl TableDocument {
    fn unsaved(&self) -> bool {
        self.saved.as_ref() != Some(&self.table)
    }
}

/// One change to a table: what the grid asks for, and what tests do.
#[derive(Clone, Debug, PartialEq)]
pub enum TableEdit {
    SetCell {
        row: String,
        column: String,
        cell: Cell,
    },
    /// A row of defaults at the end.
    AddRow,
    DuplicateRow(String),
    RemoveRow(String),
    RenameRow {
        old: String,
        new: String,
    },
    AddColumn {
        name: String,
        kind: ColumnType,
    },
    RenameColumn {
        old: String,
        new: String,
    },
    MoveColumn {
        from: usize,
        to: usize,
    },
    RemoveColumn(String),
}

impl TableEdit {
    fn apply(&self, table: &mut DataTable) -> Result<(), String> {
        match self {
            TableEdit::SetCell { row, column, cell } => table.set_cell(row, column, cell.clone()),
            TableEdit::AddRow => {
                table.add_row("row", None);
                Ok(())
            }
            TableEdit::DuplicateRow(key) => table
                .duplicate_row(key)
                .map(|_| ())
                .ok_or_else(|| format!("there's no row `{key}`")),
            TableEdit::RemoveRow(key) => {
                table.remove_row(key);
                Ok(())
            }
            TableEdit::RenameRow { old, new } => table.rename_row(old, new),
            TableEdit::AddColumn { name, kind } => table.add_column(name, kind.clone()),
            TableEdit::RenameColumn { old, new } => table.rename_column(old, new),
            TableEdit::MoveColumn { from, to } => {
                table.move_column(*from, *to);
                Ok(())
            }
            TableEdit::RemoveColumn(name) => {
                table.remove_column(name);
                Ok(())
            }
        }
    }
}

/// What the tab is shown with.
pub struct TableScene<'a> {
    /// The tables in the project folder, sorted.
    pub files: &'a [String],
    /// Every file in the project, for asset pickers.
    pub project_files: &'a [String],
    /// The keys of the table with this name, for reference pickers.
    pub keys_of: &'a dyn Fn(&str) -> Vec<String>,
    /// False with no project: shown but not edited. Tables can be edited
    /// during Play, and the game reads the edit.
    pub editable: bool,
}

/// What was asked for in the tab, for the app to do.
#[derive(Clone, Debug, PartialEq)]
pub enum TableAction {
    Show(String),
    /// Make a table with this name.
    New(String),
}

/// The kinds of column the Add column menu offers, in order.
const COLUMN_KINDS: [&str; 9] = [
    "Number",
    "Integer",
    "String",
    "Boolean",
    "Vector3",
    "Color3",
    "Enum",
    "Asset path",
    "Row of a table",
];

/// The Add column menu's fields.
#[derive(Default)]
struct NewColumn {
    name: String,
    kind: usize,
    /// An enum's options, separated by commas.
    options: String,
    asset: usize,
    table: String,
}

impl NewColumn {
    fn kind(&self) -> Result<ColumnType, String> {
        Ok(match self.kind {
            0 => ColumnType::Number,
            1 => ColumnType::Integer,
            2 => ColumnType::String,
            3 => ColumnType::Boolean,
            4 => ColumnType::Vector3,
            5 => ColumnType::Color3,
            6 => {
                let options: Vec<String> = self
                    .options
                    .split(',')
                    .map(|option| option.trim().to_owned())
                    .filter(|option| !option.is_empty())
                    .collect();
                if options.is_empty() {
                    return Err("an enum needs options, such as `Melee, Ranged`".to_owned());
                }
                ColumnType::Enum { options }
            }
            7 => ColumnType::Asset {
                asset: AssetType::ALL[self.asset],
            },
            _ => {
                let table = self.table.trim();
                if table.is_empty() {
                    return Err("pick the table its rows come from".to_owned());
                }
                ColumnType::Reference {
                    table: table.to_owned(),
                }
            }
        })
    }
}

/// The tab's documents and what it shows.
#[derive(Default)]
pub struct DataTableStudio {
    documents: Vec<TableDocument>,
    current: Option<String>,
    /// Rows whose key or a value has this text are shown.
    search: String,
    /// Only rows whose value in this column has `filter_text` are shown.
    filter_column: Option<String>,
    filter_text: String,
    /// The column rows are sorted by (`None`: the key), and whether
    /// upwards; `None` keeps the table's order.
    sort: Option<(Option<String>, bool)>,
    /// The cell being changed (row, column): more changes to it this
    /// interaction are the same undo step.
    open_cell: Option<(String, String)>,
    /// A key being typed: the row's key, and the text so far.
    key_edit: Option<(String, String)>,
    /// A column being renamed in its menu.
    column_name: String,
    new_column: NewColumn,
    new_table: String,
    /// Tables edited since [`take_changed`](Self::take_changed).
    changed: Vec<String>,
    problem: Option<String>,
    /// The grid has the pointer, so Ctrl+Z is its.
    hovered: bool,
}

impl DataTableStudio {
    /// Whether a table has changes to save.
    pub fn unsaved(&self) -> bool {
        self.documents.iter().any(TableDocument::unsaved)
    }

    pub fn has_pointer(&self) -> bool {
        self.hovered
    }

    pub fn current(&self) -> Option<&str> {
        self.current.as_deref()
    }

    /// The paths of the tables with changes to save.
    pub fn unsaved_paths(&self) -> Vec<String> {
        self.documents
            .iter()
            .filter(|doc| doc.unsaved())
            .map(|doc| doc.path.clone())
            .collect()
    }

    pub fn document(&self, path: &str) -> Option<&TableDocument> {
        self.documents.iter().find(|doc| doc.path == path)
    }

    fn document_mut(&mut self, path: &str) -> Option<&mut TableDocument> {
        self.documents.iter_mut().find(|doc| doc.path == path)
    }

    /// The paths of tables made here and not saved yet.
    pub fn new_paths(&self) -> impl Iterator<Item = &str> {
        self.documents
            .iter()
            .filter(|doc| doc.saved.is_none())
            .map(|doc| doc.path.as_str())
    }

    /// The tables edited since the last call, with what they hold now.
    pub fn take_changed(&mut self) -> Vec<(String, DataTable)> {
        let mut paths = std::mem::take(&mut self.changed);
        paths.dedup();
        paths
            .into_iter()
            .filter_map(|path| Some((path.clone(), self.document(&path)?.table.clone())))
            .collect()
    }

    /// Shows the table at `path` in `folder`, reading it the first time.
    pub fn open(&mut self, folder: &Path, path: &str) -> Result<(), String> {
        if self.document(path).is_none() {
            let table = read_data_table(&folder.join(path))
                .map_err(|err| format!("{:#}", anyhow::Error::new(err)))?;
            self.documents.push(TableDocument {
                path: path.to_owned(),
                saved: Some(table.clone()),
                table,
                undo: Vec::new(),
                redo: Vec::new(),
            });
        }
        self.show_table(path);
        Ok(())
    }

    /// Starts an empty table at `path` and shows it. It's written at the
    /// next save.
    pub fn create(&mut self, path: &str) {
        if self.document(path).is_none() {
            self.documents.push(TableDocument {
                path: path.to_owned(),
                table: DataTable::default(),
                saved: None,
                undo: Vec::new(),
                redo: Vec::new(),
            });
            self.changed.push(path.to_owned());
        }
        self.show_table(path);
    }

    fn show_table(&mut self, path: &str) {
        if self.current.as_deref() != Some(path) {
            self.current = Some(path.to_owned());
            self.filter_column = None;
            self.sort = None;
            self.open_cell = None;
            self.key_edit = None;
        }
        self.problem = None;
    }

    /// The file at `path` changed on disk (a fix, or another program): an
    /// open table without unsaved changes reads it again. A deleted one
    /// is forgotten.
    pub fn file_changed(&mut self, folder: &Path, path: &str) {
        let Some(doc) = self.document(path) else {
            return;
        };
        if doc.unsaved() {
            return;
        }
        let disk = folder.join(path);
        if !disk.exists() {
            self.documents.retain(|doc| doc.path != path);
            if self.current.as_deref() == Some(path) {
                self.current = None;
            }
            return;
        }
        if let Ok(table) = read_data_table(&disk)
            && let Some(doc) = self.document_mut(path)
            && doc.table != table
        {
            doc.table = table.clone();
            doc.saved = Some(table);
            self.changed.push(path.to_owned());
        }
    }

    /// Writes every table with changes into `folder`. Returns the paths
    /// written, or why one couldn't be.
    pub fn save(&mut self, folder: &Path) -> Result<Vec<String>, String> {
        let mut written = Vec::new();
        for doc in &mut self.documents {
            if !doc.unsaved() {
                continue;
            }
            write_data_table(&folder.join(&doc.path), &doc.table).map_err(|err| {
                format!(
                    "Could not save `{}`: {:#}",
                    doc.path,
                    anyhow::Error::new(err)
                )
            })?;
            doc.saved = Some(doc.table.clone());
            written.push(doc.path.clone());
        }
        Ok(written)
    }

    /// Makes `edit` to the shown table, as one undo step (a cell's change
    /// joins the step before while that cell is still being changed).
    pub fn edit(&mut self, edit: TableEdit) -> Result<(), String> {
        let path = self.current.clone().ok_or("no table is shown")?;
        let cell = match &edit {
            TableEdit::SetCell { row, column, .. } => Some((row.clone(), column.clone())),
            _ => None,
        };
        let same_step = cell.is_some() && cell == self.open_cell;
        let doc = self.document_mut(&path).ok_or("no table is shown")?;
        let mut table = doc.table.clone();
        edit.apply(&mut table)?;
        if table == doc.table {
            return Ok(());
        }
        let before = std::mem::replace(&mut doc.table, table);
        if !same_step {
            doc.undo.push(before);
            doc.redo.clear();
        }
        self.open_cell = cell;
        self.changed.push(path);
        Ok(())
    }

    /// Ends the cell being changed, so its next change is a new step.
    #[cfg(test)]
    pub fn finish_edit(&mut self) {
        self.open_cell = None;
    }

    /// Undoes the last edit of the shown table. False when there was none.
    pub fn undo(&mut self) -> bool {
        self.history(true)
    }

    pub fn redo(&mut self) -> bool {
        self.history(false)
    }

    fn history(&mut self, undo: bool) -> bool {
        self.open_cell = None;
        let Some(path) = self.current.clone() else {
            return false;
        };
        let Some(doc) = self.document_mut(&path) else {
            return false;
        };
        let (from, to) = if undo {
            (&mut doc.undo, &mut doc.redo)
        } else {
            (&mut doc.redo, &mut doc.undo)
        };
        let Some(table) = from.pop() else {
            return false;
        };
        to.push(std::mem::replace(&mut doc.table, table));
        self.changed.push(path);
        true
    }

    /// The rows shown, as indexes into the table: those the search and
    /// filter let through, in the order asked for.
    pub fn shown_rows(&self, table: &DataTable) -> Vec<usize> {
        let search = self.search.trim().to_lowercase();
        let filter = self.filter_text.trim().to_lowercase();
        let filter_column = self
            .filter_column
            .as_deref()
            .and_then(|name| table.column(name));
        let mut rows: Vec<usize> = (0..table.rows.len())
            .filter(|&index| {
                let row = &table.rows[index];
                let found = search.is_empty()
                    || row.key.to_lowercase().contains(&search)
                    || table
                        .columns
                        .iter()
                        .any(|column| cell_text(&row.cell(column)).contains(&search));
                let kept = match filter_column {
                    Some(column) if !filter.is_empty() => {
                        cell_text(&row.cell(column)).contains(&filter)
                    }
                    _ => true,
                };
                found && kept
            })
            .collect();
        if let Some((column, upwards)) = &self.sort {
            let column = column.as_deref().and_then(|name| table.column(name));
            rows.sort_by(|&a, &b| {
                let (a, b) = (&table.rows[a], &table.rows[b]);
                let order = match column {
                    Some(column) => compare(&a.cell(column), &b.cell(column)),
                    None => a.key.cmp(&b.key),
                };
                if *upwards { order } else { order.reverse() }
            });
        }
        rows
    }

    /// Sets what the search box and filter hold, as typed.
    #[cfg(test)]
    pub fn set_view(&mut self, search: &str, filter: Option<(&str, &str)>) {
        self.search = search.to_owned();
        self.filter_column = filter.map(|(column, _)| column.to_owned());
        self.filter_text = filter.map_or_else(String::new, |(_, text)| text.to_owned());
    }

    /// Sorts the view by `column` (`None`: the key).
    #[cfg(test)]
    pub fn sort_by(&mut self, column: Option<&str>, upwards: bool) {
        self.sort = Some((column.map(str::to_owned), upwards));
    }

    /// The tab: which table, its tools, and the grid.
    pub fn show(&mut self, ui: &mut egui::Ui, scene: &TableScene) -> Vec<TableAction> {
        let mut actions = Vec::new();
        let palette = theme::palette(ui.ctx());
        // A change ends when nothing is held or typed into any more.
        let busy = ui.ctx().input(|input| input.pointer.any_down())
            || ui.ctx().memory(|memory| memory.focused().is_some());
        if !busy {
            self.open_cell = None;
        }
        self.hovered = false;
        self.toolbar(ui, scene, &mut actions);
        if let Some(problem) = &self.problem {
            ui.colored_label(palette.error, problem);
        }
        let Some(path) = self.current.clone() else {
            ui.add_space(8.0);
            ui.label(
                "A data table holds rows of values a game's scripts read, such as each \
                 weapon's Damage, Range and Icon: \
                 DataService:GetTable(\"weapons\"):Get(\"sword\").Damage.",
            );
            ui.weak("Name a table and press New table, or double-click one in the Assets panel.");
            return actions;
        };
        let Some(table) = self.document(&path).map(|doc| doc.table.clone()) else {
            return actions;
        };
        let mut edits = Vec::new();
        self.view_tools(ui, &table);
        ui.separator();
        let rect = ui.available_rect_before_wrap();
        egui::ScrollArea::both()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                self.grid(ui, scene, &table, &mut edits);
                if scene.editable
                    && ui
                        .button(theme::with_icon(icons::PLUS, "Add row"))
                        .on_hover_text("Adds a row of default values at the end")
                        .clicked()
                {
                    edits.push(TableEdit::AddRow);
                }
            });
        self.hovered = ui.rect_contains_pointer(rect);
        for edit in edits {
            match self.edit(edit) {
                Ok(()) => self.problem = None,
                Err(why) => self.problem = Some(why),
            }
        }
        actions
    }

    fn toolbar(&mut self, ui: &mut egui::Ui, scene: &TableScene, actions: &mut Vec<TableAction>) {
        responsive::toolbar(ui, |ui| {
            let shown = self
                .current
                .as_deref()
                .and_then(data_table_name)
                .unwrap_or("No table")
                .to_owned();
            let mut tables: Vec<&str> = scene.files.iter().map(String::as_str).collect();
            tables.extend(self.new_paths());
            tables.sort_unstable();
            tables.dedup();
            egui::ComboBox::from_id_salt("data table picker")
                .selected_text(theme::with_icon(icons::TABLE, &shown))
                .show_ui(ui, |ui| {
                    if tables.is_empty() {
                        ui.weak("The project has no data tables yet.");
                    }
                    for path in tables {
                        let current = self.current.as_deref() == Some(path);
                        let name = data_table_name(path).unwrap_or(path);
                        if ui
                            .selectable_label(current, name)
                            .on_hover_text(path)
                            .clicked()
                            && !current
                        {
                            actions.push(TableAction::Show(path.to_owned()));
                        }
                    }
                });
            ui.add_enabled_ui(scene.editable, |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.new_table)
                        .hint_text("weapons")
                        .desired_width(90.0),
                );
                let name = self.new_table.trim();
                let new = ui
                    .add_enabled(
                        valid_table_name(name),
                        egui::Button::new(theme::with_icon(icons::PLUS, "New table")),
                    )
                    .on_hover_text(
                        "Makes assets/data/<name>.data.json; scripts read it with \
                         DataService:GetTable(\"<name>\")",
                    );
                if new.clicked() {
                    actions.push(TableAction::New(name.to_owned()));
                    self.new_table.clear();
                }
            });
        });
    }

    /// The search box, the column filter, and Add column.
    fn view_tools(&mut self, ui: &mut egui::Ui, table: &DataTable) {
        responsive::toolbar(ui, |ui| {
            ui.label(theme::with_icon(icons::MAGNIFYING_GLASS, ""));
            ui.add(
                egui::TextEdit::singleline(&mut self.search)
                    .hint_text("Search")
                    .desired_width(110.0),
            );
            responsive::room_for(ui, 260.0);
            ui.label(theme::with_icon(icons::FUNNEL, ""));
            egui::ComboBox::from_id_salt("data table filter")
                .selected_text(self.filter_column.as_deref().unwrap_or("Any column"))
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_label(self.filter_column.is_none(), "Any column")
                        .clicked()
                    {
                        self.filter_column = None;
                    }
                    for column in &table.columns {
                        let on = self.filter_column.as_deref() == Some(column.name.as_str());
                        if ui.selectable_label(on, &column.name).clicked() {
                            self.filter_column = Some(column.name.clone());
                        }
                    }
                });
            if self.filter_column.is_some() {
                ui.add(
                    egui::TextEdit::singleline(&mut self.filter_text)
                        .hint_text("has…")
                        .desired_width(80.0),
                );
            }
        });
    }

    fn grid(
        &mut self,
        ui: &mut egui::Ui,
        scene: &TableScene,
        table: &DataTable,
        edits: &mut Vec<TableEdit>,
    ) {
        let rows = self.shown_rows(table);
        egui::Grid::new("data table grid")
            .striped(true)
            .spacing([10.0, 4.0])
            .show(ui, |ui| {
                // The header: Key, each column, then Add column.
                self.sort_header(ui, None, "Key", "Each row's unique key");
                for (index, column) in table.columns.iter().enumerate() {
                    ui.horizontal(|ui| {
                        self.sort_header(
                            ui,
                            Some(&column.name),
                            &column.name,
                            &column.kind.label(),
                        );
                        if scene.editable {
                            self.column_menu(ui, table, index, edits);
                        }
                    });
                }
                if scene.editable {
                    self.add_column_menu(ui, scene, edits);
                }
                ui.end_row();

                for index in rows {
                    let row = &table.rows[index];
                    self.key_cell(ui, scene.editable, &row.key, edits);
                    for column in &table.columns {
                        let cell = row.cell(column);
                        ui.add_enabled_ui(scene.editable, |ui| {
                            let id = ui.id().with((&row.key, &column.name));
                            if let Some(cell) = cell_editor(ui, id, column, &cell, scene) {
                                edits.push(TableEdit::SetCell {
                                    row: row.key.clone(),
                                    column: column.name.clone(),
                                    cell,
                                });
                            }
                        });
                    }
                    if scene.editable {
                        ui.horizontal(|ui| {
                            if ui
                                .small_button(icons::COPY)
                                .on_hover_text("Duplicate this row")
                                .clicked()
                            {
                                edits.push(TableEdit::DuplicateRow(row.key.clone()));
                            }
                            if ui
                                .small_button(icons::TRASH)
                                .on_hover_text("Remove this row")
                                .clicked()
                            {
                                edits.push(TableEdit::RemoveRow(row.key.clone()));
                            }
                        });
                    }
                    ui.end_row();
                }
            });
        if table.rows.is_empty() {
            ui.weak("No rows yet.");
        }
    }

    /// A column's header: clicking it sorts by it, upwards then downwards
    /// then back to the table's order.
    fn sort_header(&mut self, ui: &mut egui::Ui, column: Option<&String>, name: &str, hover: &str) {
        let column = column.cloned();
        let arrow = match &self.sort {
            Some((sorted, upwards)) if *sorted == column => {
                if *upwards {
                    icons::SORT_ASCENDING
                } else {
                    icons::SORT_DESCENDING
                }
            }
            _ => "",
        };
        let text = egui::RichText::new(format!("{name} {arrow}")).strong();
        let clicked = ui
            .add(egui::Button::new(text).frame(false))
            .on_hover_text(format!("{hover}. Click to sort by it."))
            .clicked();
        if clicked {
            self.sort = match self.sort.take() {
                Some((sorted, true)) if sorted == column => Some((column, false)),
                Some((sorted, false)) if sorted == column => None,
                _ => Some((column, true)),
            };
        }
    }

    /// A column's menu: rename, move and remove.
    fn column_menu(
        &mut self,
        ui: &mut egui::Ui,
        table: &DataTable,
        index: usize,
        edits: &mut Vec<TableEdit>,
    ) {
        let name = table.columns[index].name.clone();
        ui.menu_button(icons::DOTS_THREE, |ui| {
            if ui
                .add(egui::TextEdit::singleline(&mut self.column_name).hint_text(&name))
                .gained_focus()
            {
                self.column_name = name.clone();
            }
            if ui.button("Rename").clicked() {
                edits.push(TableEdit::RenameColumn {
                    old: name.clone(),
                    new: self.column_name.clone(),
                });
                ui.close();
            }
            ui.separator();
            if ui
                .add_enabled(
                    index > 0,
                    egui::Button::new(theme::with_icon(icons::ARROW_LEFT, "Move left")),
                )
                .clicked()
            {
                edits.push(TableEdit::MoveColumn {
                    from: index,
                    to: index - 1,
                });
                ui.close();
            }
            if ui
                .add_enabled(
                    index + 1 < table.columns.len(),
                    egui::Button::new(theme::with_icon(icons::ARROW_RIGHT, "Move right")),
                )
                .clicked()
            {
                edits.push(TableEdit::MoveColumn {
                    from: index,
                    to: index + 1,
                });
                ui.close();
            }
            if ui
                .button(theme::with_icon(icons::TRASH, "Remove column"))
                .clicked()
            {
                edits.push(TableEdit::RemoveColumn(name.clone()));
                ui.close();
            }
        });
    }

    fn add_column_menu(
        &mut self,
        ui: &mut egui::Ui,
        scene: &TableScene,
        edits: &mut Vec<TableEdit>,
    ) {
        crate::widgets::menu_button(ui, Some(icons::PLUS), "Column", |ui| {
            let new = &mut self.new_column;
            ui.add(egui::TextEdit::singleline(&mut new.name).hint_text("Damage"));
            egui::ComboBox::from_id_salt("new column type")
                .selected_text(COLUMN_KINDS[new.kind])
                .show_ui(ui, |ui| {
                    for (index, kind) in COLUMN_KINDS.iter().enumerate() {
                        ui.selectable_value(&mut new.kind, index, *kind);
                    }
                });
            match new.kind {
                6 => {
                    ui.add(egui::TextEdit::singleline(&mut new.options).hint_text("Melee, Ranged"));
                }
                7 => {
                    egui::ComboBox::from_id_salt("new column asset")
                        .selected_text(AssetType::ALL[new.asset].label())
                        .show_ui(ui, |ui| {
                            for (index, asset) in AssetType::ALL.iter().enumerate() {
                                ui.selectable_value(&mut new.asset, index, asset.label());
                            }
                        });
                }
                8 => {
                    let shown = if new.table.is_empty() {
                        "Pick a table"
                    } else {
                        new.table.as_str()
                    };
                    egui::ComboBox::from_id_salt("new column table")
                        .selected_text(shown)
                        .show_ui(ui, |ui| {
                            for name in scene.files.iter().filter_map(|path| data_table_name(path))
                            {
                                ui.selectable_value(&mut new.table, name.to_owned(), name);
                            }
                        });
                }
                _ => {}
            }
            if ui.button("Add column").clicked() {
                match new.kind() {
                    Ok(kind) => {
                        edits.push(TableEdit::AddColumn {
                            name: new.name.trim().to_owned(),
                            kind,
                        });
                        new.name.clear();
                        ui.close();
                    }
                    Err(why) => self.problem = Some(why),
                }
            }
        });
    }

    /// A row's key, renamed when its box loses focus.
    fn key_cell(
        &mut self,
        ui: &mut egui::Ui,
        editable: bool,
        key: &str,
        edits: &mut Vec<TableEdit>,
    ) {
        let mut text = match &self.key_edit {
            Some((row, text)) if row == key => text.clone(),
            _ => key.to_owned(),
        };
        // `add_sized`: a grid cell would otherwise shrink the box to its text.
        let size = egui::vec2(KEY_WIDTH, ui.spacing().interact_size.y);
        let response = ui
            .add_enabled_ui(editable, |ui| {
                ui.add_sized(size, egui::TextEdit::singleline(&mut text))
            })
            .inner;
        if response.changed() {
            self.key_edit = Some((key.to_owned(), text));
        }
        if response.lost_focus()
            && let Some((row, new)) = self.key_edit.take()
            && row == key
        {
            edits.push(TableEdit::RenameRow { old: row, new });
        }
    }
}

/// Whether `name` can name a new table: letters, digits, `_`, `-` and
/// `/` for a folder.
fn valid_table_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '/' | ' '))
}

/// A cell as lower-case text, for the search and filter.
fn cell_text(cell: &Cell) -> String {
    match cell {
        Cell::Number(n) => n.to_string(),
        Cell::Integer(n) => n.to_string(),
        Cell::Text(text) => text.to_lowercase(),
        Cell::Bool(b) => b.to_string(),
        Cell::Vector3([x, y, z]) | Cell::Color3([x, y, z]) => format!("{x}, {y}, {z}"),
    }
}

/// The order of two cells of one column.
fn compare(a: &Cell, b: &Cell) -> Ordering {
    match (a, b) {
        (Cell::Number(a), Cell::Number(b)) => a.total_cmp(b),
        (Cell::Integer(a), Cell::Integer(b)) => a.cmp(b),
        (Cell::Bool(a), Cell::Bool(b)) => a.cmp(b),
        (Cell::Text(a), Cell::Text(b)) => a.to_lowercase().cmp(&b.to_lowercase()),
        _ => cell_text(a).cmp(&cell_text(b)),
    }
}

/// The control for a cell of `column`. Returns its new value when changed.
fn cell_editor(
    ui: &mut egui::Ui,
    id: egui::Id,
    column: &Column,
    cell: &Cell,
    scene: &TableScene,
) -> Option<Cell> {
    let palette = theme::palette(ui.ctx());
    match (&column.kind, cell) {
        (_, Cell::Number(n)) => {
            let mut n = *n;
            ui.add(egui::DragValue::new(&mut n).speed(0.1))
                .changed()
                .then_some(Cell::Number(n))
        }
        (_, Cell::Integer(n)) => {
            let mut n = *n;
            ui.add(egui::DragValue::new(&mut n))
                .changed()
                .then_some(Cell::Integer(n))
        }
        (_, Cell::Bool(b)) => {
            let mut b = *b;
            ui.checkbox(&mut b, "").changed().then_some(Cell::Bool(b))
        }
        (_, Cell::Vector3(v)) => {
            let mut v = *v;
            let changed = ui
                .horizontal(|ui| {
                    let mut changed = false;
                    for (axis, value) in ["X", "Y", "Z"].iter().zip(&mut v) {
                        changed |= ui
                            .add(
                                egui::DragValue::new(value)
                                    .speed(0.1)
                                    .prefix(format!("{axis} ")),
                            )
                            .changed();
                    }
                    changed
                })
                .inner;
            changed.then_some(Cell::Vector3(v))
        }
        (_, Cell::Color3(c)) => {
            let mut rgb = c.map(|channel| channel as f32);
            ui.color_edit_button_rgb(&mut rgb)
                .changed()
                .then(|| Cell::Color3(rgb.map(f64::from)))
        }
        (ColumnType::Enum { options }, Cell::Text(name)) => {
            let mut chosen = name.clone();
            egui::ComboBox::from_id_salt(id)
                .selected_text(name.as_str())
                .show_ui(ui, |ui| {
                    for option in options {
                        ui.selectable_value(&mut chosen, option.clone(), option);
                    }
                });
            (chosen != *name).then_some(Cell::Text(chosen))
        }
        (ColumnType::Asset { asset }, Cell::Text(path)) => {
            let mut chosen = path.clone();
            let shown = if path.is_empty() {
                "None".to_owned()
            } else {
                path.rsplit('/').next().unwrap_or(path).to_owned()
            };
            let missing = !path.is_empty() && !scene.project_files.contains(path);
            let text = if missing {
                egui::RichText::new(format!("{} {shown}", icons::WARNING)).color(palette.error)
            } else {
                egui::RichText::new(shown)
            };
            egui::ComboBox::from_id_salt(id)
                .selected_text(text)
                .width(CELL_WIDTH)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut chosen, String::new(), "None");
                    for file in scene
                        .project_files
                        .iter()
                        .filter(|file| asset.matches(file))
                    {
                        ui.selectable_value(&mut chosen, file.clone(), file);
                    }
                })
                .response
                .on_hover_text(if missing {
                    format!("`{path}` isn't in the project")
                } else {
                    path.clone()
                });
            (chosen != *path).then_some(Cell::Text(chosen))
        }
        (ColumnType::Reference { table }, Cell::Text(key)) => {
            let keys = (scene.keys_of)(table);
            let mut chosen = key.clone();
            let missing = !key.is_empty() && !keys.contains(key);
            let text = match () {
                _ if key.is_empty() => egui::RichText::new("None"),
                _ if missing => {
                    egui::RichText::new(format!("{} {key}", icons::WARNING)).color(palette.error)
                }
                _ => egui::RichText::new(key.as_str()),
            };
            egui::ComboBox::from_id_salt(id)
                .selected_text(text)
                .width(CELL_WIDTH)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut chosen, String::new(), "None");
                    for other in &keys {
                        ui.selectable_value(&mut chosen, other.clone(), other);
                    }
                })
                .response
                .on_hover_text(if missing {
                    format!("`{table}` has no row `{key}`")
                } else {
                    format!("A row of `{table}`")
                });
            (chosen != *key).then_some(Cell::Text(chosen))
        }
        (_, Cell::Text(text)) => {
            let mut text = text.clone();
            let size = egui::vec2(CELL_WIDTH, ui.spacing().interact_size.y);
            ui.add_sized(size, egui::TextEdit::singleline(&mut text))
                .changed()
                .then_some(Cell::Text(text))
        }
    }
}

/// Adds a column of each type, for tests.
#[cfg(test)]
pub fn every_column() -> Vec<TableEdit> {
    let column = |name: &str, kind| TableEdit::AddColumn {
        name: name.to_owned(),
        kind,
    };
    vec![
        column("Damage", ColumnType::Number),
        column("Range", ColumnType::Integer),
        column("Title", ColumnType::String),
        column("Rare", ColumnType::Boolean),
        column("Offset", ColumnType::Vector3),
        column("Tint", ColumnType::Color3),
        column(
            "Kind",
            ColumnType::Enum {
                options: vec!["Melee".to_owned(), "Ranged".to_owned()],
            },
        ),
        column(
            "Icon",
            ColumnType::Asset {
                asset: AssetType::Texture,
            },
        ),
        column(
            "Ammo",
            ColumnType::Reference {
                table: "ammo".to_owned(),
            },
        ),
    ]
}
