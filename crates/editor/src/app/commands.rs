//! The Console panel: the log (`output.rs`) with the command line under it
//! (`console.rs`), and what each typed command does.
//!
//! Lua runs against the world being edited, as one undo step, or during
//! Play against the game's scripts (see `play.rs`). A command is run after
//! the dock is drawn, as `play`, `stop` and `open` change the dock.

use super::*;

use crate::console::{self, Command};

/// The longest command shown in an Undo label, in characters.
const LABEL_LENGTH: usize = 40;

impl EditorApp {
    /// The Console: the command line at the bottom, the log above it.
    pub(super) fn console_pane(&mut self, ui: &mut egui::Ui) {
        egui::Panel::bottom("console command line")
            .frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(0, 4)))
            .show_separator_line(false)
            .resizable(false)
            .show(ui, |ui| {
                widgets::divider(ui);
                ui.add_space(4.0);
                let world = shown(&self.world, &self.play);
                if let Some(command) = self.console.show(ui, world) {
                    self.pending_command = Some(command);
                }
            });
        if let Some(location) = self.output.show(ui) {
            if location.path.ends_with(".rs") {
                self.open_engine_source(&location.path, location.line);
            } else {
                self.open_script(&location.path, Some(location.line));
            }
        }
    }

    /// Runs a line typed into the Console, echoing it first.
    pub(super) fn run_console_command(&mut self, line: &str) {
        self.output.command(line);
        match console::parse(line) {
            Command::Help => {
                for (_, text) in console::BUILT_INS {
                    self.output.info(text);
                }
                self.output.info(
                    "Anything else runs as Lua: against the world being edited (one Undo \
                     step), or the game during Play. Up and Down go through earlier \
                     commands, Tab completes, and Shift+Enter starts a new line.",
                );
            }
            Command::Clear => self.output.clear(),
            Command::Select(text) => self.select_by_name(text),
            Command::Find(text) => self.find_by_name(text),
            Command::Open(text) => self.open_by_name(text),
            Command::Play => {
                if self.playing() {
                    self.output.info("Already playing.");
                } else {
                    self.play();
                }
            }
            Command::Stop => {
                if self.playing() {
                    self.stop();
                } else {
                    self.output.info("Not playing.");
                }
            }
            Command::Save => {
                if self.save() {
                    self.output.info("Saved.");
                }
            }
            Command::Check => self.run_asset_check(),
            Command::Checkpoint(label) => self.checkpoint_command(label),
            Command::Lua(source) => self.run_lua(source),
        }
    }

    fn select_by_name(&mut self, text: &str) {
        if text.is_empty() {
            self.output
                .error("select needs a name or path, such as `select Workspace.Baseplate`.");
            return;
        }
        let world = shown(&self.world, &self.play);
        match console::find_instance(world, text) {
            Some(id) => {
                let name = world.full_name(id).unwrap_or_default();
                self.select_from_viewport(Some(id));
                self.output.info(format!("Selected {name}."));
            }
            None => self
                .output
                .error(format!("Nothing is called `{text}`. Try `find {text}`.")),
        }
    }

    fn find_by_name(&mut self, text: &str) {
        if text.is_empty() {
            self.output
                .error("find needs part of a name, such as `find door`.");
            return;
        }
        let world = shown(&self.world, &self.play);
        let found = console::find_all(world, text);
        if found.is_empty() {
            self.output
                .info(format!("No Instance's name contains `{text}`."));
            return;
        }
        let names: Vec<String> = found.iter().filter_map(|&id| world.full_name(id)).collect();
        for name in &names {
            self.output.info(name.as_str());
        }
        self.output.info(format!(
            "Found {}, and selected {}.",
            plural(names.len(), "Instance"),
            if names.len() == 1 { "it" } else { "them" }
        ));
        self.selection = found;
        self.selected = self.selection.last().copied();
        if let Some(id) = self.selected {
            self.explorer.reveal(shown(&self.world, &self.play), id);
        }
    }

    /// Opens a script by its path, or by the name of its Script Instance.
    fn open_by_name(&mut self, text: &str) {
        if text.is_empty() {
            self.output
                .error("open needs a script's name or path, such as `open Spinner`.");
            return;
        }
        let by_name = self.project.as_ref().and_then(|project| {
            project
                .scripts
                .iter()
                .find(|script| {
                    self.world
                        .get(script.instance)
                        .is_some_and(|instance| instance.name().eq_ignore_ascii_case(text))
                })
                .map(|script| script.path.clone())
        });
        match by_name {
            Some(path) => self.open_script(&path, None),
            // `open_script` says so if there's no such file.
            None if text.contains(['/', '.']) => self.open_script(text, None),
            None => self.output.error(format!(
                "There's no script called `{text}`. Scripts are opened by their \
                 name in the Explorer, or their path, such as `scripts/Spinner.server.lua`."
            )),
        }
    }

    /// Runs Lua against the game during Play, or against the world being
    /// edited, where whatever it changed is one undo step.
    fn run_lua(&mut self, source: &str) {
        if self.paused() {
            self.output
                .info("Play is paused at a breakpoint, so this runs when you press Continue (F5).");
        }
        if let Some(session) = &mut self.play {
            match session.run_command(source, &mut self.output) {
                Ok(Ok(())) => {}
                Ok(Err(message)) => self.report_bad_lua(source, message),
                Err(crash) => self.crashed(crash),
            }
            return;
        }
        // In edit mode nothing defines globals between commands, so a lone
        // word that isn't one is a mistyped command, not Lua.
        if let Some(word) = console::unknown_command(source)
            && word == source
        {
            self.output.error(unknown(word));
            return;
        }
        self.history.seal();
        // Run on a copy, so an engine crash leaves the world as it was.
        let mut copy = self.world.clone();
        let mut result = Err(String::new());
        if let Err(crash) = play::guard(|| {
            result = axcenity_script::run_edit_command(&mut copy, source);
        }) {
            self.output.error(format!(
                "The engine crashed running that command: {}. The world wasn't changed. \
                 This is a bug in Axcenity.",
                crash.0
            ));
            return;
        }
        match result {
            Ok(command) => {
                for line in command.output {
                    match line.kind {
                        axcenity_script::OutputKind::Print => self.output.info(line.text),
                        axcenity_script::OutputKind::Warning => self.output.warning(line.text),
                        axcenity_script::OutputKind::Error => {
                            self.output.script_error(line.text, line.trace)
                        }
                    }
                }
                if command.changed {
                    let before = std::mem::replace(&mut self.world, copy);
                    self.history
                        .record_world(&mut self.world, undo_label(source), before);
                    self.forget_selection_outside_tree();
                }
            }
            Err(message) => self.report_bad_lua(source, message),
        }
    }

    /// Code that isn't valid Lua: a mistyped command, or a Lua mistake.
    fn report_bad_lua(&mut self, source: &str, message: String) {
        match console::unknown_command(source) {
            Some(word) => self
                .output
                .error(format!("{} (As Lua: {message})", unknown(word))),
            None => self.output.error(message),
        }
    }
}

fn unknown(word: &str) -> String {
    format!("Unknown command `{word}`. Type help to see the commands.")
}

/// "Console: " and the command's first line, shortened.
fn undo_label(source: &str) -> String {
    let first = source.lines().next().unwrap_or_default();
    let mut label: String = first.chars().take(LABEL_LENGTH).collect();
    if first.chars().count() > LABEL_LENGTH || source.contains('\n') {
        label.push('…');
    }
    format!("Console: {label}")
}

fn plural(count: usize, noun: &str) -> String {
    if count == 1 {
        format!("1 {noun}")
    } else {
        format!("{count} {noun}s")
    }
}

#[cfg(test)]
mod tests {
    use axcenity_core::Value;

    use super::*;
    use crate::app::tests::{example, last_line, part};
    use crate::output::OutputKind;

    fn texts(app: &EditorApp) -> Vec<&str> {
        app.output
            .lines()
            .iter()
            .map(|line| line.text.as_str())
            .collect()
    }

    #[test]
    fn lua_reads_the_edit_world_and_prints_the_value() {
        let mut app = EditorApp::new(Some(&example("hello-touched")));
        app.run_console_command("print(workspace.Floor.Name)");
        app.run_console_command("workspace.Pad.Name");
        let lines = texts(&app);
        assert_eq!(
            lines[lines.len() - 4..],
            [
                "print(workspace.Floor.Name)",
                "Floor",
                "workspace.Pad.Name",
                "Pad"
            ]
        );
        assert_eq!(
            app.output.lines()[lines.len() - 4].kind,
            OutputKind::Command
        );
        assert!(!app.unsaved(), "reading changes nothing");
    }

    #[test]
    fn a_lua_change_is_one_undo_step() {
        let mut app = EditorApp::new(Some(&example("hello-touched")));
        let floor = part(&app, "Floor");
        let before = app
            .world
            .get(floor)
            .unwrap()
            .property("Transparency")
            .cloned();
        app.run_console_command(
            "workspace.Floor.Transparency = 0.5\nworkspace.Floor.Name = 'Ground'",
        );
        let instance = app.world.get(floor).unwrap();
        assert_eq!(instance.property("Transparency"), Some(&Value::Number(0.5)));
        assert_eq!(instance.name(), "Ground");
        assert!(app.unsaved());
        assert_eq!(
            app.history.undo_label(),
            Some("Console: workspace.Floor.Transparency = 0.5…")
        );

        app.undo();
        let instance = app.world.get(floor).unwrap();
        assert_eq!(instance.property("Transparency").cloned(), before);
        assert_eq!(instance.name(), "Floor");
        assert!(!app.unsaved());
    }

    #[test]
    fn lua_runs_against_the_game_during_play() {
        let mut app = EditorApp::new(Some(&example("hello-touched")));
        app.play();
        assert!(app.playing());
        app.run_console_command("x = 21");
        app.run_console_command("print(workspace.Floor.Name, x * 2)");
        assert_eq!(last_line(&app).text, "Floor\t42");
        app.run_console_command("workspace.Floor.Name = 'Ground'");
        app.stop();
        assert!(
            app.world
                .find_first_child(app.world.workspace(), "Floor")
                .is_some()
        );
    }

    #[test]
    fn built_ins_work_and_mistakes_are_reported() {
        let mut app = EditorApp::new(Some(&example("hello-touched")));
        let pad = part(&app, "Pad");
        app.run_console_command("select Workspace.Pad");
        assert_eq!(app.selected, Some(pad));
        app.run_console_command("select Nope");
        assert_eq!(last_line(&app).kind, OutputKind::Error);
        app.run_console_command("find PA");
        assert!(app.selection.contains(&pad));
        assert!(last_line(&app).text.starts_with("Found "));

        for (typed, starts) in [
            ("teleport Pad", "Unknown command `teleport`"),
            ("teleport", "Unknown command `teleport`"),
            ("print(", "console:1:"),
            ("workspace.Nope.Size = 1", "console:1:"),
        ] {
            app.run_console_command(typed);
            let line = last_line(&app);
            assert_eq!(line.kind, OutputKind::Error, "{typed}");
            assert!(line.text.starts_with(starts), "{typed}: {}", line.text);
        }
        assert!(!app.unsaved(), "failed commands change nothing");

        app.run_console_command("clear");
        assert!(app.output.lines().is_empty());
        app.run_console_command("help");
        assert!(
            texts(&app)
                .iter()
                .any(|text| text.starts_with("select <name|path>"))
        );
    }
}
