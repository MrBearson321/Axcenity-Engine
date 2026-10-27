//! Data tables in the editor: the Data Table tab (`data_table.rs`) and
//! what it asks for, saving tables with the project, and handing each
//! edit to the shared `Assets`, so the game reads it during Play. These
//! are methods of [`super::EditorApp`].

use axcenity_assets::{DATA_FOLDER, data_table_name, data_table_path, is_data_table};

use super::*;
use crate::asset_browser::AssetKind;
use crate::data_table::{TableAction, TableScene};

/// Every data table file in the project's `assets/data` folder, sorted.
pub(super) fn table_files(folder: &Path) -> Vec<String> {
    let mut found: Vec<String> = axcenity_project::list_project_files(folder)
        .into_iter()
        .filter(|path| is_data_table(path) && data_table_name(path).is_some())
        .collect();
    found.sort();
    found
}

impl EditorApp {
    /// The Data Table tab, then what was asked for in it.
    pub(in crate::app) fn data_table_pane(&mut self, ui: &mut egui::Ui) {
        let folder = self.project.as_ref().map(|project| project.folder.clone());
        // The project's asset files, from the Assets panel's last scan.
        let entries: Vec<String> = self
            .asset_browser
            .entries()
            .into_iter()
            .filter(|entry| entry.kind != AssetKind::Prop)
            .map(|entry| entry.path)
            .collect();
        let files: Vec<String> = entries
            .iter()
            .filter(|path| is_data_table(path) && data_table_name(path).is_some())
            .cloned()
            .collect();
        let assets = self.assets.clone();
        let keys_of = move |name: &str| -> Vec<String> {
            assets
                .data_table(name)
                .map(|table| table.rows.iter().map(|row| row.key.clone()).collect())
                .unwrap_or_default()
        };
        let scene = TableScene {
            files: &files,
            project_files: &entries,
            keys_of: &keys_of,
            editable: folder.is_some(),
        };
        let actions = self.data_tables.show(ui, &scene);
        self.hand_over_tables();
        for action in actions {
            self.data_table_action(action);
        }
    }

    pub(in crate::app) fn data_table_action(&mut self, action: TableAction) {
        match action {
            TableAction::Show(path) => self.open_data_table(&path),
            TableAction::New(name) => self.new_data_table(&name),
        }
    }

    /// Gives the tables edited in the tab to the shared `Assets`, so
    /// scripts (in Play, and the reference pickers) read them as they are.
    pub(in crate::app) fn hand_over_tables(&mut self) {
        for (path, table) in self.data_tables.take_changed() {
            self.assets.put_data_table(&path, table);
        }
    }

    /// Starts the table `name` (`assets/data/<name>.data.json`) and shows it.
    pub(in crate::app) fn new_data_table(&mut self, name: &str) {
        let Some(folder) = self.project.as_ref().map(|project| project.folder.clone()) else {
            return;
        };
        let path = data_table_path(name.trim());
        if folder.join(&path).exists() || self.data_tables.document(&path).is_some() {
            self.output
                .error(format!("There's already a data table named `{name}`."));
            return;
        }
        self.data_tables.create(&path);
        self.hand_over_tables();
        self.layout.open(Pane::DataTable);
        self.output.info(format!(
            "Made the data table `{path}`: add columns and rows. It's written when the \
             project is saved; scripts read it with DataService:GetTable(\"{name}\")."
        ));
    }

    /// Shows the table at `path` in the Data Table tab.
    pub(in crate::app) fn open_data_table(&mut self, path: &str) {
        let Some(folder) = self.project.as_ref().map(|project| project.folder.clone()) else {
            return;
        };
        if data_table_name(path).is_none() {
            self.output.error(format!(
                "`{path}` isn't a data table (a .data.json file in {DATA_FOLDER})."
            ));
            return;
        }
        match self.data_tables.open(&folder, path) {
            Ok(()) => self.layout.open(Pane::DataTable),
            Err(why) => self.output.error(format!("Could not open `{path}`: {why}")),
        }
    }

    /// Writes every table with changes. Adds what it wrote to `saved`.
    /// False when one couldn't be written.
    pub(super) fn save_data_tables(&mut self, saved: &mut Vec<String>) -> bool {
        let Some(folder) = self.project.as_ref().map(|project| project.folder.clone()) else {
            return true;
        };
        match self.data_tables.save(&folder) {
            Ok(written) => {
                if !written.is_empty() {
                    self.asset_browser.refresh();
                }
                saved.extend(written);
                true
            }
            Err(why) => {
                self.output.error(why);
                false
            }
        }
    }
}

impl EditorApp {
    /// Adds the project's data tables and their columns to the objects'
    /// ReplicatedStorage entry, for the data blocks' menus.
    pub(in crate::app) fn add_table_names(
        &self,
        folder: &Path,
        objects: &mut [crate::block_editor::ObjectInfo],
    ) {
        use crate::block_editor::{NameKind, STORAGE_PATH};
        let Some(storage) = objects
            .iter_mut()
            .find(|object| object.path == STORAGE_PATH)
        else {
            return;
        };
        let mut columns = Vec::new();
        for path in table_files(folder) {
            let Some(name) = data_table_name(&path) else {
                continue;
            };
            storage.names.push((NameKind::DataTable, name.to_owned()));
            if let Ok(table) = self.assets.data_table(name) {
                columns.extend(table.columns.iter().map(|column| column.name.clone()));
            }
        }
        columns.sort();
        columns.dedup();
        storage
            .names
            .extend(columns.into_iter().map(|column| (NameKind::Column, column)));
    }
}
