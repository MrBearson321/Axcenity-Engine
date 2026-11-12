use super::*;
pub(in crate::app) use crate::build_game::ExportTarget;
use crate::build_game::{Profile, build_game_with, needs_archive};

/// The build profile picked last in this project, on this computer (R13).
const PICKED_PROFILE_FILE: &str = ".axcenity/build-profile.txt";

/// The build profile File > Build Game uses for the project at `folder`:
/// the one picked last if it's still there, else Release.
pub(in crate::app) fn picked_profile(folder: &Path) -> String {
    let picked = fs::read_to_string(folder.join(PICKED_PROFILE_FILE))
        .map(|text| text.trim().to_owned())
        .unwrap_or_default();
    let profiles = project::BuildProfiles::read(folder).unwrap_or_default();
    if profiles.get(&picked).is_some() {
        picked
    } else {
        project::RELEASE_PROFILE.to_owned()
    }
}

impl EditorApp {
    pub(in crate::app) fn export_game_with_dialog(&mut self) {
        self.export_game_for_with_dialog(ExportTarget::current());
    }

    /// Picks the build profile File > Build Game uses (R13), remembered for
    /// this project on this computer.
    pub(in crate::app) fn pick_build_profile(&mut self, name: &str) {
        let Some(project) = &self.project else {
            return;
        };
        let path = project.folder.join(PICKED_PROFILE_FILE);
        let written = path
            .parent()
            .map_or(Ok(()), fs::create_dir_all)
            .and_then(|()| fs::write(&path, format!("{name}\n")));
        if let Err(err) = written {
            self.output
                .error(format!("Could not remember the build profile: {err}"));
        }
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
        let profile_name = picked_profile(&folder);
        let settings = match project::BuildProfiles::read(&folder) {
            Ok(profiles) => profiles.get(&profile_name).unwrap_or_default(),
            Err(err) => {
                self.output.error(format!(
                    "Could not build the game: {} can't be read: {err}.",
                    project::BUILD_PROFILES_FILE
                ));
                return;
            }
        };
        let profile = Profile {
            name: &profile_name,
            settings: &settings,
            world: Some(&self.world),
        };
        let archive = needs_archive(target);
        let built = match build_game_with(&folder, destination, target, &player, archive, &profile)
        {
            Ok(built) => built,
            Err(err) => {
                self.output
                    .error(format!("Could not build the game: {err}"));
                return;
            }
        };
        for problem in &built.report.problems {
            self.output
                .warning(format!("Asset Check before the build: {problem}"));
        }
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
            "Built the game for {} with the {} profile to `{}` (run `{}`): game.axgame is {} \
             in {} files. Build report: `{}`.",
            target.directory(),
            profile_name,
            destination.display(),
            target.output_name(),
            project::file_size_text(built.report.game_file_bytes),
            built.report.files,
            built.report_path.display()
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
