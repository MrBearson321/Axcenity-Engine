//! The editor window: a menu bar, the Explorer on the right, the Output panel
//! along the bottom, and the 3D viewport filling the rest, like Roblox Studio.

use std::path::{Path, PathBuf};

use axcenity_core::World;
use eframe::egui;

use crate::explorer::Explorer;
use crate::output::Output;
use crate::viewport::Viewport;

pub struct EditorApp {
    /// The open project's Instances, or an empty world when none is open.
    world: World,
    project: Option<OpenProject>,
    explorer: Explorer,
    output: Output,
    viewport: Viewport,
    /// The window title last sent to the window, to send it again only when it changes.
    shown_title: String,
}

/// What the editor remembers about the open project.
struct OpenProject {
    name: String,
    folder: PathBuf,
}

impl EditorApp {
    /// An editor with `folder` open, or with nothing open if it's `None`.
    pub fn new(folder: Option<&Path>) -> Self {
        let world = World::new();
        let mut app = Self {
            explorer: Explorer::new(&world),
            world,
            project: None,
            output: Output::default(),
            viewport: Viewport::new(),
            shown_title: String::new(),
        };
        match folder {
            Some(folder) => app.open(folder),
            None => app.output.info(
                "No project open. Use File > Open Project…, or start the editor with a \
                 folder: axcenity-editor examples/hello-touched",
            ),
        }
        app
    }

    /// Opens the project in `folder` in place of the current one. If it
    /// can't be loaded, the error goes to Output and the current project
    /// stays open.
    pub fn open(&mut self, folder: &Path) {
        let project = match axcenity_project::load(folder) {
            Ok(project) => project,
            Err(err) => {
                // `{:#}` on an `anyhow::Error` shows the whole chain of
                // causes on one line, such as which line of which file is wrong.
                let err = anyhow::Error::new(err);
                self.output.error(format!(
                    "Could not open the project in `{}`: {err:#}",
                    folder.display()
                ));
                return;
            }
        };

        let scripts = project.scripts.len();
        self.output.info(format!(
            "Opened \"{}\" from `{}`: {} objects, {scripts} {}. Scripts run when \
             you press Play, which comes in a later version.",
            project.manifest.name,
            folder.display(),
            // `game` and its services are always there, so don't count them.
            project.world.descendants(project.world.root()).len() - 2,
            if scripts == 1 { "script" } else { "scripts" },
        ));
        self.world = project.world;
        self.explorer = Explorer::new(&self.world);
        self.viewport.reset_camera();
        self.project = Some(OpenProject {
            name: project.manifest.name,
            folder: project.folder,
        });
    }

    /// Asks for a project folder with the system's folder picker, then opens it.
    fn open_with_dialog(&mut self) {
        let mut dialog = rfd::FileDialog::new().set_title("Open an Axcenity project folder");
        if let Some(project) = &self.project {
            dialog = dialog.set_directory(&project.folder);
        }
        // `None` means the dialog was cancelled.
        if let Some(folder) = dialog.pick_folder() {
            self.open(&folder);
        }
    }

    fn title(&self) -> String {
        match &self.project {
            Some(project) => format!("{} - Axcenity Editor", project.name),
            None => "Axcenity Editor".to_string(),
        }
    }

    fn menu_bar(&mut self, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("Open Project…").clicked() {
                    self.open_with_dialog();
                }
                ui.separator();
                if ui.button("Quit").clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
        });
    }
}

impl eframe::App for EditorApp {
    /// Called by eframe to draw each frame (only when something changed,
    /// such as mouse movement, unless a frame is requested).
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        let title = self.title();
        if title != self.shown_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.shown_title = title;
        }

        // Panels take their space in the order they're added: the menu bar
        // spans the whole top, then the Explorer runs down the whole right
        // side, then the Output fits under the viewport.
        egui::TopBottomPanel::top("menu bar").show(ctx, |ui| self.menu_bar(ui));
        egui::SidePanel::right("explorer")
            .default_width(260.0)
            .show(ctx, |ui| self.explorer.show(ui, &self.world));
        egui::TopBottomPanel::bottom("output")
            .resizable(true)
            .default_height(160.0)
            .show(ctx, |ui| self.output.show(ui));
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| match frame.wgpu_render_state() {
                Some(render_state) => self.viewport.show(ui, render_state, &self.world),
                // Can't happen: main.rs always picks the wgpu renderer.
                None => {
                    ui.label("The viewport needs the wgpu renderer.");
                }
            });
    }
}

#[cfg(test)]
mod tests {
    use crate::output::{OutputKind, OutputLine};

    use super::*;

    fn example(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples")
            .join(name)
    }

    fn last_line(app: &EditorApp) -> &OutputLine {
        app.output.lines().last().unwrap()
    }

    /// A project folder in the temp directory with these files in it.
    fn temp_project(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let folder = std::env::temp_dir().join(format!(
            "axcenity-editor-{name}-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        for (file, text) in files {
            std::fs::write(folder.join(file), text).unwrap();
        }
        folder
    }

    #[test]
    fn opens_the_milestone_1_demo_and_says_so_in_output() {
        let app = EditorApp::new(Some(&example("hello-touched")));

        let workspace = app.world.workspace();
        for part in ["Floor", "Pad", "Ball"] {
            assert!(app.world.find_first_child(workspace, part).is_some());
        }
        assert_eq!(app.title(), "Hello Touched - Axcenity Editor");
        let line = last_line(&app);
        assert_eq!(line.kind, OutputKind::Info);
        assert!(
            line.text.starts_with("Opened \"Hello Touched\" from")
                && line.text.contains(": 4 objects, 1 script."),
            "{}",
            line.text
        );
    }

    #[test]
    fn starting_without_a_project_explains_how_to_open_one() {
        let app = EditorApp::new(None);
        assert_eq!(app.title(), "Axcenity Editor");
        assert!(last_line(&app).text.contains("File > Open Project"));
        assert_eq!(app.world.descendants(app.world.root()).len(), 2);
    }

    #[test]
    fn a_missing_project_folder_is_reported_in_output() {
        let app = EditorApp::new(Some(&example("no-such-project")));
        let line = last_line(&app);
        assert_eq!(line.kind, OutputKind::Error);
        assert!(
            line.text.starts_with("Could not open the project in")
                && line.text.contains("axcenity.json"),
            "{}",
            line.text
        );
    }

    #[test]
    fn a_broken_world_file_is_reported_with_its_line_and_the_open_project_stays() {
        let folder = temp_project(
            "broken-world",
            &[
                (
                    "axcenity.json",
                    r#"{ "name": "Broken", "format_version": 1 }"#,
                ),
                (
                    "world.json",
                    "{\n  \"format_version\": 1,\n  \"workspace\": oops\n}\n",
                ),
            ],
        );
        let mut app = EditorApp::new(Some(&example("hello-touched")));
        app.open(&folder);

        let line = last_line(&app);
        assert_eq!(line.kind, OutputKind::Error);
        assert!(
            line.text.contains("world.json") && line.text.contains("line 3"),
            "{}",
            line.text
        );
        assert_eq!(app.title(), "Hello Touched - Axcenity Editor");
        let _ = std::fs::remove_dir_all(&folder);
    }
}
