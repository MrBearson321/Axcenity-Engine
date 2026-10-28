//! The Tests panel's place in the app (Phase 53; the panel is
//! `game_tests.rs`): running tests from the open project's folder, opening
//! a spec at a failure's line, and running again on save.

use axcenity_testing::Selection;
use eframe::egui;

use super::EditorApp;
use crate::game_tests::TestAction;

impl EditorApp {
    pub(in crate::app) fn tests_pane(&mut self, ui: &mut egui::Ui) {
        if let Some(project) = &self.project
            && self.game_tests.files.is_empty()
            && !self.game_tests.running()
        {
            self.game_tests.refresh_files(&project.folder);
        }
        match self.game_tests.show(ui, self.project.is_some()) {
            Some(TestAction::Run(selection)) => self.run_tests(selection),
            Some(TestAction::Open { path, line }) => self.open_spec(&path, line as usize),
            None => {}
        }
    }

    /// Runs `selection` of the open project's tests, from its saved files.
    pub(in crate::app) fn run_tests(&mut self, selection: Selection) {
        if let Some(project) = &self.project {
            self.game_tests.start(project.folder.clone(), selection);
        }
    }

    /// Opens the spec file `path` in a tab with the cursor on `line`.
    pub(in crate::app) fn open_spec(&mut self, path: &str, line: usize) {
        let Some(project) = &self.project else {
            return;
        };
        if self.tabs.get(path).is_some() {
            self.tabs.open(path, "", Some(line));
            return;
        }
        match std::fs::read_to_string(project.folder.join(path)) {
            Ok(source) => self.tabs.open(path, &source, Some(line)),
            Err(err) => self.output.error(format!("Can't open `{path}`: {err}")),
        }
    }

    /// A script or spec was saved: runs the last tests again when the
    /// Tests panel's Re-run on save is on.
    pub(in crate::app) fn tests_saved(&mut self) {
        if let Some(project) = &self.project {
            self.game_tests.saved(project.folder.clone());
        }
    }
}
