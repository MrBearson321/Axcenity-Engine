//! The editor window: a menu bar, the Explorer and Properties on the right,
//! the Output panel along the bottom, and the 3D viewport filling the rest,
//! like Roblox Studio.

use std::path::{Path, PathBuf};

use axcenity_core::{InstanceId, World};
use eframe::egui;

use crate::explorer::Explorer;
use crate::output::Output;
use crate::properties::{self, Edit};
use crate::viewport::Viewport;

/// Ctrl+S, or Cmd+S on macOS (`COMMAND` picks the right one).
const SAVE_SHORTCUT: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::S);

pub struct EditorApp {
    /// The open project's Instances, or an empty world when none is open.
    world: World,
    project: Option<OpenProject>,
    /// The Instance shown in Properties and highlighted in the Explorer and viewport.
    selected: Option<InstanceId>,
    /// True when the world has changes that aren't saved yet.
    unsaved: bool,
    /// Something waiting for an answer to "Save changes?".
    asking: Option<Discard>,
    /// Set once closing has been allowed, so the window's close request
    /// isn't stopped again.
    closing: bool,
    explorer: Explorer,
    output: Output,
    viewport: Viewport,
    /// The window title last sent to the window, to send it again only when it changes.
    shown_title: String,
}

/// Something that throws the open world away, so it has to ask first if
/// there are unsaved changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Discard {
    Quit,
    OpenAnother,
}

/// The answers to "Save changes?".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Answer {
    Save,
    DontSave,
    Cancel,
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
            selected: None,
            unsaved: false,
            asking: None,
            closing: false,
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
        self.selected = None;
        self.unsaved = false;
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

    /// Writes the world back to the project folder. Returns false (with the
    /// reason in Output) if nothing is open or the file couldn't be written.
    fn save(&mut self) -> bool {
        let Some(project) = &self.project else {
            return false;
        };
        match axcenity_project::save_world(&project.folder, &self.world) {
            Ok(()) => {
                self.output.info(format!(
                    "Saved \"{}\" to `{}`.",
                    project.name,
                    project.folder.join(axcenity_project::WORLD_FILE).display()
                ));
                self.unsaved = false;
                true
            }
            Err(err) => {
                let err = anyhow::Error::new(err);
                self.output.error(format!("Could not save: {err:#}"));
                false
            }
        }
    }

    /// Selects `id` (or nothing), from a click in the viewport. The Explorer
    /// opens the branches above it and scrolls to it, so it's highlighted there too.
    fn select_from_viewport(&mut self, id: Option<InstanceId>) {
        self.selected = id;
        if let Some(id) = id {
            self.explorer.reveal(&self.world, id);
        }
    }

    /// Applies a change from the Properties panel. Only a real change counts
    /// as unsaved: typing the same value again doesn't.
    fn apply(&mut self, id: InstanceId, edit: Edit) {
        let Some(instance) = self.world.get(id) else {
            return;
        };
        let result = match edit {
            Edit::Name(name) => {
                if name == instance.name() || !properties::can_rename(instance) {
                    return;
                }
                self.world.set_name(id, name).map_err(anyhow::Error::new)
            }
            Edit::Property(name, value) => {
                if instance.property(name) == Some(&value) {
                    return;
                }
                self.world
                    .set_property(id, name, value)
                    .map_err(anyhow::Error::new)
            }
        };
        match result {
            Ok(()) => self.unsaved = true,
            // Can't happen from the panel, which only offers valid values.
            Err(err) => self.output.error(format!("Could not change that: {err:#}")),
        }
    }

    /// Asks to `discard` the open world. Returns it when it can go ahead now,
    /// or `None` after putting up "Save changes?" because there are unsaved changes.
    fn request(&mut self, discard: Discard) -> Option<Discard> {
        if self.unsaved {
            self.asking = Some(discard);
            None
        } else {
            Some(discard)
        }
    }

    /// Takes the answer to "Save changes?" and returns what to go ahead with,
    /// if anything. A failed save goes nowhere, so the changes aren't lost.
    fn answer(&mut self, answer: Answer) -> Option<Discard> {
        let discard = self.asking.take()?;
        match answer {
            Answer::Save => self.save().then_some(discard),
            Answer::DontSave => Some(discard),
            Answer::Cancel => None,
        }
    }

    fn go_ahead(&mut self, ctx: &egui::Context, discard: Discard) {
        match discard {
            Discard::Quit => {
                self.closing = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Discard::OpenAnother => self.open_with_dialog(),
        }
    }

    /// `request` then, if nothing needs asking, `go_ahead`.
    fn request_and_go_ahead(&mut self, ctx: &egui::Context, discard: Discard) {
        if let Some(discard) = self.request(discard) {
            self.go_ahead(ctx, discard);
        }
    }

    /// Shows "Save changes?" while something is waiting for an answer.
    fn unsaved_changes_prompt(&mut self, ctx: &egui::Context) {
        let Some(discard) = self.asking else {
            return;
        };
        let name = self.project.as_ref().map_or("", |project| &project.name);
        let doing = match discard {
            Discard::Quit => "closing",
            Discard::OpenAnother => "opening another project",
        };
        let mut answer = None;
        let modal = egui::Modal::new(egui::Id::new("unsaved changes")).show(ctx, |ui| {
            ui.heading("Save changes?");
            ui.label(format!(
                "\"{name}\" has unsaved changes. Save them before {doing}?"
            ));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Save").clicked() {
                    answer = Some(Answer::Save);
                }
                if ui.button("Don't Save").clicked() {
                    answer = Some(Answer::DontSave);
                }
                if ui.button("Cancel").clicked() {
                    answer = Some(Answer::Cancel);
                }
            });
        });
        // Escape or a click outside the box means Cancel.
        if answer.is_none() && modal.should_close() {
            answer = Some(Answer::Cancel);
        }
        if let Some(answer) = answer
            && let Some(discard) = self.answer(answer)
        {
            self.go_ahead(ctx, discard);
        }
    }

    fn title(&self) -> String {
        // A `*` after the name means unsaved changes, as in most editors.
        let unsaved = if self.unsaved { "*" } else { "" };
        match &self.project {
            Some(project) => format!("{}{unsaved} - Axcenity Editor", project.name),
            None => "Axcenity Editor".to_string(),
        }
    }

    fn menu_bar(&mut self, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("Open Project…").clicked() {
                    self.request_and_go_ahead(ui.ctx(), Discard::OpenAnother);
                }
                let save = egui::Button::new("Save")
                    .shortcut_text(ui.ctx().format_shortcut(&SAVE_SHORTCUT));
                if ui.add_enabled(self.project.is_some(), save).clicked() {
                    self.save();
                }
                ui.separator();
                if ui.button("Quit").clicked() {
                    self.request_and_go_ahead(ui.ctx(), Discard::Quit);
                }
            });
            if self.unsaved {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.weak("Unsaved changes");
                });
            }
        });
    }
}

impl eframe::App for EditorApp {
    /// Called by eframe to draw each frame (only when something changed,
    /// such as mouse movement, unless a frame is requested).
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // The window's close button: stop it and ask first if there are
        // unsaved changes. `closing` lets the close through once allowed.
        if ctx.input(|input| input.viewport().close_requested()) && !self.closing && self.unsaved {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.asking = Some(Discard::Quit);
        }
        // Taken from the input before the viewport sees it, so the S doesn't
        // also fly the camera backward.
        if ctx.input_mut(|input| input.consume_shortcut(&SAVE_SHORTCUT)) {
            self.save();
        }

        let title = self.title();
        if title != self.shown_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.shown_title = title;
        }

        // Panels take their space in the order they're added: the menu bar
        // spans the whole top, then the Explorer (with Properties under it)
        // runs down the whole right side, then the Output fits under the viewport.
        egui::TopBottomPanel::top("menu bar").show(ctx, |ui| self.menu_bar(ui));
        let mut edit = None;
        egui::SidePanel::right("explorer")
            .default_width(300.0)
            .show(ctx, |ui| {
                egui::TopBottomPanel::bottom("properties")
                    .resizable(true)
                    .default_height(340.0)
                    .show_inside(ui, |ui| {
                        edit = properties::show(ui, &self.world, self.selected);
                    });
                if let Some(id) = self.explorer.show(ui, &self.world, self.selected) {
                    self.selected = Some(id);
                }
            });
        // Applied before the viewport draws, so it shows the change this frame.
        if let (Some(id), Some(edit)) = (self.selected, edit) {
            self.apply(id, edit);
        }
        egui::TopBottomPanel::bottom("output")
            .resizable(true)
            .default_height(160.0)
            .show(ctx, |ui| self.output.show(ui));
        let clicked = egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| match frame.wgpu_render_state() {
                Some(render_state) => {
                    self.viewport
                        .show(ui, render_state, &self.world, self.selected)
                }
                // Can't happen: main.rs always picks the wgpu renderer.
                None => {
                    ui.label("The viewport needs the wgpu renderer.");
                    None
                }
            })
            .inner;
        if let Some(clicked) = clicked {
            self.select_from_viewport(clicked.part);
            // The panels were already drawn this frame, so draw again to
            // show the new selection there.
            ctx.request_repaint();
        }

        self.unsaved_changes_prompt(ctx);
    }
}

#[cfg(test)]
mod tests {
    use axcenity_core::{ClassName, Color3, Material, Value, Vector3};
    use axcenity_render::glam::Vec3;

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

    /// A copy of an example project in the temp directory, safe to save over.
    fn copy_of_example(name: &str, test: &str) -> PathBuf {
        fn copy_folder(from: &Path, to: &Path) {
            std::fs::create_dir_all(to).unwrap();
            for entry in std::fs::read_dir(from).unwrap() {
                let entry = entry.unwrap();
                let target = to.join(entry.file_name());
                if entry.file_type().unwrap().is_dir() {
                    copy_folder(&entry.path(), &target);
                } else {
                    std::fs::copy(entry.path(), target).unwrap();
                }
            }
        }
        let folder = std::env::temp_dir().join(format!(
            "axcenity-editor-{test}-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&folder);
        copy_folder(&example(name), &folder);
        folder
    }

    fn part(app: &EditorApp, name: &str) -> InstanceId {
        app.world
            .find_first_child(app.world.workspace(), name)
            .unwrap()
    }

    fn vector(x: f32, y: f32, z: f32) -> Value {
        Value::Vector3(Vector3::new(x, y, z))
    }

    /// The edits the criteria name: Position, Size, Color, Material and
    /// Anchored, plus a new name. Odd numbers check nothing gets rounded.
    fn edit_the_pad(app: &mut EditorApp) -> InstanceId {
        let pad = part(app, "Pad");
        let edits = [
            Edit::Name("Launch Pad".to_string()),
            Edit::Property("Position", vector(1.25, 0.3, -7.1)),
            Edit::Property("Size", vector(3.3, 0.001, 12.0)),
            Edit::Property("Color", Value::Color3(Color3::new(0.1, 0.725, 0.333))),
            Edit::Property("Material", Value::Material(Material::Neon)),
            Edit::Property("Anchored", Value::Bool(false)),
        ];
        for edit in edits {
            app.apply(pad, edit);
        }
        pad
    }

    #[test]
    fn selecting_a_part_in_the_viewport_highlights_it_in_the_explorer_too() {
        // A part two levels down, inside a closed branch.
        let mut app = EditorApp::new(Some(&example("hello-touched")));
        let pad = part(&app, "Pad");
        let child = app.world.create(ClassName::Part).unwrap();
        app.world.set_parent(child, Some(pad)).unwrap();
        assert!(!app.explorer.rows(&app.world).iter().any(|r| r.id == child));

        app.select_from_viewport(Some(child));
        assert_eq!(app.selected, Some(child));
        // The Explorer highlights the row matching `selected`, and it's visible.
        assert!(app.explorer.rows(&app.world).iter().any(|r| r.id == child));

        // A click on empty space clears the selection.
        app.select_from_viewport(None);
        assert_eq!(app.selected, None);
    }

    #[test]
    fn edits_show_in_the_next_draw_list_and_count_as_unsaved() {
        let mut app = EditorApp::new(Some(&example("hello-touched")));
        assert!(!app.unsaved);
        let pad = edit_the_pad(&mut app);
        assert!(app.unsaved);
        assert_eq!(app.title(), "Hello Touched* - Axcenity Editor");

        let instance = app.world.get(pad).unwrap();
        assert_eq!(instance.name(), "Launch Pad");
        assert_eq!(instance.property("Anchored"), Some(&Value::Bool(false)));
        // The viewport draws from the world each frame, so the next draw list
        // already has the new position, size and neon glow.
        let camera = axcenity_render::Camera {
            position: Vec3::new(0.0, 50.0, 50.0),
            look_at: Vec3::ZERO,
            fov_y_degrees: 70.0,
        };
        let list = axcenity_scene::build_draw_list(&app.world, camera);
        let drawn_pad = list
            .items
            .iter()
            .find(|item| item.material.emissive == [0.1, 0.725, 0.333])
            .expect("the pad glows in its new color");
        let center = drawn_pad.transform.transform_point3(Vec3::ZERO);
        assert!(center.abs_diff_eq(Vec3::new(1.25, 0.3, -7.1), 1e-6));
        let corner = drawn_pad.transform.transform_point3(Vec3::splat(0.5));
        assert!((corner - center).abs_diff_eq(Vec3::new(1.65, 0.0005, 6.0), 1e-6));
    }

    #[test]
    fn setting_a_value_it_already_has_is_not_an_unsaved_change() {
        let mut app = EditorApp::new(Some(&example("hello-touched")));
        let pad = part(&app, "Pad");
        app.apply(pad, Edit::Name("Pad".to_string()));
        app.apply(pad, Edit::Property("Anchored", Value::Bool(true)));
        assert!(!app.unsaved);
    }

    #[test]
    fn scripts_cannot_be_renamed_from_properties() {
        let mut app = EditorApp::new(Some(&example("hello-touched")));
        let script = app
            .world
            .find_first_child(app.world.server_script_service(), "pad")
            .unwrap();
        app.apply(script, Edit::Name("door".to_string()));
        assert_eq!(app.world.get(script).unwrap().name(), "pad");
        assert!(!app.unsaved);
    }

    #[test]
    fn save_close_and_reopen_reproduces_the_edits_exactly() {
        let folder = copy_of_example("hello-touched", "save-reopen");
        let mut app = EditorApp::new(Some(&folder));
        edit_the_pad(&mut app);
        assert!(app.save());
        assert!(!app.unsaved);
        assert_eq!(app.title(), "Hello Touched - Axcenity Editor");
        assert!(last_line(&app).text.starts_with("Saved \"Hello Touched\""));
        let saved = axcenity_project::write_world(&app.world);
        drop(app);

        let reopened = EditorApp::new(Some(&folder));
        // Same world file text means every Instance, name and property
        // value came back exactly, down to the last bit of every number.
        assert_eq!(axcenity_project::write_world(&reopened.world), saved);
        let pad = part(&reopened, "Launch Pad");
        let pad = reopened.world.get(pad).unwrap();
        assert_eq!(pad.property("Position"), Some(&vector(1.25, 0.3, -7.1)));
        assert_eq!(
            pad.property("Color"),
            Some(&Value::Color3(Color3::new(0.1, 0.725, 0.333)))
        );
        assert_eq!(
            pad.property("Material"),
            Some(&Value::Material(Material::Neon))
        );
        // The script file is left alone and still loads.
        assert!(
            reopened
                .world
                .find_first_child(reopened.world.server_script_service(), "pad")
                .is_some()
        );
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn closing_with_unsaved_changes_asks_first() {
        let mut app = EditorApp::new(Some(&example("hello-touched")));
        // Nothing to lose: go straight ahead.
        assert_eq!(app.request(Discard::Quit), Some(Discard::Quit));
        assert_eq!(app.asking, None);

        edit_the_pad(&mut app);
        assert_eq!(app.request(Discard::Quit), None);
        assert_eq!(app.asking, Some(Discard::Quit));
        // Cancel keeps the editor open with the changes.
        assert_eq!(app.answer(Answer::Cancel), None);
        assert_eq!(app.asking, None);
        assert!(app.unsaved);
        // Don't Save goes ahead without saving.
        app.request(Discard::OpenAnother);
        assert_eq!(app.answer(Answer::DontSave), Some(Discard::OpenAnother));
        assert!(app.unsaved);
    }

    #[test]
    fn save_in_the_prompt_saves_then_goes_ahead_but_not_if_saving_fails() {
        let folder = copy_of_example("hello-touched", "prompt-save");
        let mut app = EditorApp::new(Some(&folder));
        edit_the_pad(&mut app);
        app.request(Discard::Quit);
        assert_eq!(app.answer(Answer::Save), Some(Discard::Quit));
        assert!(!app.unsaved);
        assert!(part_exists_on_disk(&folder, "Launch Pad"));

        // With the folder gone, saving fails: say why and stay open.
        std::fs::remove_dir_all(&folder).unwrap();
        app.apply(part(&app, "Floor"), Edit::Name("Ground".to_string()));
        app.request(Discard::Quit);
        assert_eq!(app.answer(Answer::Save), None);
        assert!(app.unsaved);
        let line = last_line(&app);
        assert_eq!(line.kind, OutputKind::Error);
        assert!(line.text.starts_with("Could not save"), "{}", line.text);
    }

    fn part_exists_on_disk(folder: &Path, name: &str) -> bool {
        let world = axcenity_project::load(folder).unwrap().world;
        world.find_first_child(world.workspace(), name).is_some()
    }
}
