use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::app) enum ExportTarget {
    Windows,
    MacOS,
    Linux,
}

pub(super) fn current_export_target() -> ExportTarget {
    if cfg!(windows) {
        ExportTarget::Windows
    } else if cfg!(target_os = "macos") {
        ExportTarget::MacOS
    } else {
        ExportTarget::Linux
    }
}

impl ExportTarget {
    fn directory(self) -> &'static str {
        match self {
            Self::Windows => "windows",
            Self::MacOS => "macos",
            Self::Linux => "linux",
        }
    }

    fn executable(self) -> &'static str {
        match self {
            Self::Windows => "axcenity-player.exe",
            Self::MacOS | Self::Linux => "axcenity-player",
        }
    }

    fn output_name(self) -> &'static str {
        match self {
            Self::Windows => "AxcenityPlayer.exe",
            Self::MacOS | Self::Linux => "AxcenityPlayer",
        }
    }

    fn app_bundle(self) -> bool {
        self == Self::MacOS
    }
}

impl EditorApp {
    pub(in crate::app) fn export_game_with_dialog(&mut self) {
        let target = if cfg!(windows) {
            ExportTarget::Windows
        } else if cfg!(target_os = "macos") {
            ExportTarget::MacOS
        } else {
            ExportTarget::Linux
        };
        self.export_game_for_with_dialog(target);
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
        let player_name = target.executable();
        let player_source = match std::env::current_exe() {
            Ok(path) if target == current_export_target() => path.with_file_name(player_name),
            Ok(path) => path
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join("players")
                .join(target.directory())
                .join(player_name),
            Err(err) => {
                self.output
                    .error(format!("Could not locate the editor executable: {err}"));
                return;
            }
        };
        if !player_source.is_file() {
            let location = if target == current_export_target() {
                "beside the editor".to_owned()
            } else {
                format!("in `players/{}` beside the editor", target.directory())
            };
            self.output.error(format!(
                "Could not build the game: the player binary `{}` is missing. Build `axcenity-player` \
                 for {} and put it {}.",
                player_source.display(),
                target.directory(),
                location
            ));
            return;
        }
        let game = match project::pack_game(&folder) {
            Ok(game) => game,
            Err(err) => {
                self.output
                    .error(format!("Could not pack the project: {err}"));
                return;
            }
        };
        if let Err(err) = fs::create_dir_all(destination) {
            self.output.error(format!(
                "Could not create the build folder `{}`: {err}",
                destination.display()
            ));
            return;
        }
        let bundle = if target.app_bundle() {
            destination
                .join("AxcenityPlayer.app")
                .join("Contents")
                .join("MacOS")
        } else {
            destination.to_owned()
        };
        if let Err(err) = fs::create_dir_all(&bundle) {
            self.output.error(format!(
                "Could not create the build folder `{}`: {err}",
                bundle.display()
            ));
            return;
        }
        let output_name = target.output_name();
        let player_destination = bundle.join(output_name);
        if let Err(err) = fs::copy(&player_source, &player_destination) {
            self.output.error(format!(
                "Could not copy player to `{}`: {err}",
                player_destination.display()
            ));
            return;
        }
        let game_destination = bundle.join("game.axgame");
        if let Err(err) = fs::write(&game_destination, game) {
            self.output.error(format!(
                "Could not write game file `{}`: {err}",
                game_destination.display()
            ));
            return;
        }
        if target.app_bundle() {
            let plist = bundle
                .parent()
                .expect("MacOS folder is inside Contents")
                .join("Info.plist");
            if let Err(err) = fs::write(
                &plist,
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
                 <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \
                 \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
                 <plist version=\"1.0\"><dict>\n\
                 <key>CFBundleExecutable</key><string>AxcenityPlayer</string>\n\
                 <key>CFBundleIdentifier</key><string>com.axcenity.player</string>\n\
                 <key>CFBundleName</key><string>Axcenity Player</string>\n\
                 <key>CFBundlePackageType</key><string>APPL</string>\n\
                 <key>CFBundleVersion</key><string>1</string>\n\
                 </dict></plist>\n",
            ) {
                self.output.error(format!(
                    "Could not write macOS app metadata `{}`: {err}",
                    plist.display()
                ));
                return;
            }
        } else if target == ExportTarget::Linux {
            let desktop = destination.join("AxcenityPlayer.desktop");
            if let Err(err) = fs::write(
                &desktop,
                "[Desktop Entry]\nType=Application\nName=Axcenity Player\n\
                 Exec=./AxcenityPlayer\nTerminal=true\nCategories=Game;\n",
            ) {
                self.output.error(format!(
                    "Could not write Linux desktop entry `{}`: {err}",
                    desktop.display()
                ));
                return;
            }
        }
        let credits_folder = if target.app_bundle() {
            destination
                .join("AxcenityPlayer.app")
                .join("Contents")
                .join("Resources")
        } else {
            destination.to_owned()
        };
        if let Err(err) = fs::create_dir_all(&credits_folder) {
            self.output.error(format!(
                "Could not create credits folder `{}`: {err}",
                credits_folder.display()
            ));
            return;
        }
        let credits_destination = credits_folder.join("LICENSES.txt");
        if let Err(err) = fs::write(
            &credits_destination,
            format!(
                "Axcenity Engine and third-party license notices\n\n{}",
                include_str!("../../../../../NOTICE")
            ),
        ) {
            self.output.error(format!(
                "Could not write credits file `{}`: {err}",
                credits_destination.display()
            ));
            return;
        }
        self.output.info(format!(
            "Built the game for {} to `{}` (run `{output_name}`).",
            target.directory(),
            destination.display()
        ));
    }
}
