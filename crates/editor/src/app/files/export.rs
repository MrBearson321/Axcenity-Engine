use super::*;
pub(in crate::app) use crate::build_game::ExportTarget;
use crate::build_game::{build_game, needs_archive};

impl EditorApp {
    pub(in crate::app) fn export_game_with_dialog(&mut self) {
        self.export_game_for_with_dialog(ExportTarget::current());
    }

    pub(in crate::app) fn export_game_for_with_dialog(&mut self, target: ExportTarget) {
        let Some(project) = &self.project else {
            return;
        };
        if self.playing() {
            return;
        }
        let folder = project.folder.clone();
        if self.unsaved() && !self.save() {
            return;
        }
        let Some(destination) = rfd::FileDialog::new()
            .set_title("Build game: choose a folder")
            .set_directory(&folder)
            .pick_folder()
        else {
            return;
        };
        self.export_game_to(&destination, target);
    }

    pub(super) fn export_game_to(&mut self, destination: &Path, target: ExportTarget) {
        let Some(project) = &self.project else {
            return;
        };
        let folder = project.folder.clone();
        let player = match std::env::current_exe() {
            Ok(editor) => target.player_beside(&editor),
            Err(err) => {
                self.output
                    .error(format!("Could not locate the editor executable: {err}"));
                return;
            }
        };
        let built = match build_game(&folder, destination, target, &player, needs_archive(target)) {
            Ok(built) => built,
            Err(err) => {
                self.output
                    .error(format!("Could not build the game: {err}"));
                return;
            }
        };
        if built.cooked > 0 {
            self.output
                .info(format!("Cooked {} textures for the build.", built.cooked));
        }
        for (path, reason) in &built.uncooked {
            self.output.warning(format!(
                "The texture `{path}` ships as it is (it couldn't be cooked: {reason})."
            ));
        }
        for note in &built.notes {
            self.output
                .warning(format!("{note} The build uses Axcenity's in its place."));
        }
        if let Some(reason) = &built.unsigned {
            self.output.warning(format!(
                "The game's app couldn't be signed, so macOS may refuse to open it on \
                 another Mac (codesign: {reason})."
            ));
        }
        self.output.info(format!(
            "Built the game for {} to `{}` (run `{}`).",
            target.directory(),
            destination.display(),
            target.output_name()
        ));
        if let Some(archive) = &built.archive {
            self.output.info(format!(
                "Copy `{}` to the {} computer and unzip it there: Windows can't mark the \
                 player as a program, and the zip keeps that mark.",
                archive.display(),
                target.directory()
            ));
        }
    }
}
