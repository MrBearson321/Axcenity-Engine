//! What the Downloaded / Imported Assets tab's actions do in the editor.

use super::*;
use crate::asset_inventory::InventoryAction;

impl EditorApp {
    /// The Downloaded / Imported Assets tab, read again when the project's
    /// files change and every few seconds while shown.
    pub(in crate::app) fn inventory_pane(&mut self, ui: &mut egui::Ui) {
        let folder = self.project.as_ref().map(|project| project.folder.clone());
        let generation = self.asset_browser.generation();
        if self.inventory.wants_reading(folder.as_deref(), generation) {
            let (files, packages, origins, snapshot) = self.asset_browser.project_assets();
            let generation = self.asset_browser.generation();
            self.inventory.read(
                folder.as_deref(),
                generation,
                &files,
                &packages,
                &origins,
                &snapshot,
            );
        }
        for action in self.inventory.show(ui, folder.is_some()) {
            self.inventory_action(action);
        }
    }

    /// Shows the Downloaded / Imported Assets tab.
    pub(in crate::app) fn open_inventory(&mut self) {
        self.layout.open(Pane::Inventory);
        self.inventory.refresh();
    }

    fn inventory_action(&mut self, action: InventoryAction) {
        match action {
            InventoryAction::Reveal(path) => {
                self.layout.open(Pane::Assets);
                self.asset_browser.reveal(&path);
            }
            InventoryAction::OpenInToolbox { record, downloaded } => {
                self.open_toolbox(None);
                let folder = self.project.as_ref().map(|project| project.folder.clone());
                self.toolbox
                    .show_record(folder.as_deref(), *record, downloaded);
            }
            InventoryAction::Notice { error: true, text } => self.output.error(text),
            InventoryAction::Notice { error: false, text } => self.output.info(text),
        }
    }
}
