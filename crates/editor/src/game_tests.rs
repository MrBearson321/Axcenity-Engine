//! The Tests panel (Phase 53): the project's game tests as a tree of spec
//! files and their tests, each passed, failed or not run yet, with Run all,
//! run a file and run one. A failure shows its message with its `file:line`,
//! which opens the spec there. With Re-run on save, saving a script or spec
//! runs the last tests again.
//!
//! Tests run on a thread of their own through `axcenity_testing::Runner`,
//! the same as `axcenity-test`, against the project's saved files; the
//! thread sends what it finds and each result back over a channel.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::Duration;

use axcenity_testing::{Failure, RunEvent, Runner, Selection, TestCase, TestResult};
use eframe::egui::{self, RichText};

use crate::theme::{self, icons};
use crate::widgets::{self, Role};

/// What the panel asks the app to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TestAction {
    Run(Selection),
    /// Open the spec `path` at `line`.
    Open {
        path: String,
        line: u32,
    },
}

/// How one test went last.
#[derive(Clone, Debug, PartialEq)]
pub enum TestState {
    NotRun,
    /// Waiting for its turn in the run going now.
    Queued,
    Passed,
    Failed(Failure),
}

#[derive(Clone, Debug)]
pub struct TestNode {
    pub case: TestCase,
    pub state: TestState,
}

/// A spec file and its tests, or why it couldn't run.
#[derive(Clone, Debug)]
pub struct FileNode {
    pub path: String,
    pub tests: Vec<TestNode>,
    pub failure: Option<Failure>,
}

enum Message {
    Event(RunEvent),
    /// The project couldn't be opened.
    Problem(String),
    Done,
}

/// The panel's state: the tree, and the run going now.
#[derive(Default)]
pub struct GameTests {
    pub files: Vec<FileNode>,
    running: Option<Receiver<Message>>,
    /// The tests last run, for Re-run on save.
    last: Option<Selection>,
    pub rerun_on_save: bool,
    /// Why the last run couldn't start.
    problem: Option<String>,
}

impl GameTests {
    pub fn running(&self) -> bool {
        self.running.is_some()
    }

    /// Lists the spec files in `folder` that the tree doesn't have yet, and
    /// drops those that are gone, keeping what's known of the rest.
    pub fn refresh_files(&mut self, folder: &std::path::Path) {
        let paths = axcenity_testing::spec_files(folder);
        self.files.retain(|file| paths.contains(&file.path));
        for path in paths {
            if !self.files.iter().any(|file| file.path == path) {
                self.files.push(FileNode {
                    path,
                    tests: Vec::new(),
                    failure: None,
                });
            }
        }
        self.files.sort_by(|a, b| a.path.cmp(&b.path));
    }

    /// Starts running `selection` from the project in `folder`, after any
    /// run going now (whose results are dropped).
    pub fn start(&mut self, folder: PathBuf, selection: Selection) {
        self.refresh_files(&folder);
        self.problem = None;
        for file in &mut self.files {
            for test in &mut file.tests {
                if picks(&selection, &test.case) {
                    test.state = TestState::Queued;
                }
            }
        }
        let (sender, receiver) = mpsc::channel();
        let run = selection.clone();
        std::thread::spawn(move || {
            match Runner::open(&folder) {
                Ok(mut runner) => {
                    runner.run(&run, &mut |event| {
                        let _ = sender.send(Message::Event(event));
                    });
                }
                Err(problem) => {
                    let _ = sender.send(Message::Problem(problem));
                }
            }
            let _ = sender.send(Message::Done);
        });
        self.running = Some(receiver);
        self.last = Some(selection);
    }

    /// Runs the last tests again, if Re-run on save is on and some ran.
    pub fn saved(&mut self, folder: PathBuf) -> bool {
        match (&self.last, self.rerun_on_save) {
            (Some(selection), true) => {
                let selection = selection.clone();
                self.start(folder, selection);
                true
            }
            _ => false,
        }
    }

    /// Takes what the run going now has sent.
    pub fn poll(&mut self) {
        let Some(receiver) = &self.running else {
            return;
        };
        let mut messages = Vec::new();
        let done = loop {
            match receiver.try_recv() {
                Ok(Message::Done) | Err(TryRecvError::Disconnected) => break true,
                Ok(message) => messages.push(message),
                Err(TryRecvError::Empty) => break false,
            }
        };
        for message in messages {
            match message {
                Message::Event(event) => self.follow(event),
                Message::Problem(problem) => self.problem = Some(problem),
                Message::Done => {}
            }
        }
        if done {
            self.running = None;
            // Anything still queued didn't run (its file changed meanwhile).
            for test in self.files.iter_mut().flat_map(|file| &mut file.tests) {
                if test.state == TestState::Queued {
                    test.state = TestState::NotRun;
                }
            }
        }
    }

    /// Takes one event of a run into the tree.
    pub fn follow(&mut self, event: RunEvent) {
        match event {
            RunEvent::Found { file, tests } => {
                let last = self.last.clone();
                let node = self.file_mut(&file);
                node.failure = None;
                let old = std::mem::take(&mut node.tests);
                node.tests = tests
                    .into_iter()
                    .map(|case| {
                        // A test that's still there keeps how it went.
                        let state = old
                            .iter()
                            .find(|test| test.case.name == case.name)
                            .map_or(TestState::NotRun, |test| test.state.clone());
                        TestNode {
                            case,
                            state: if state == TestState::Queued {
                                TestState::NotRun
                            } else {
                                state
                            },
                        }
                    })
                    .collect();
                if let Some(selection) = &last {
                    for test in &mut node.tests {
                        if picks(selection, &test.case) {
                            test.state = TestState::Queued;
                        }
                    }
                }
            }
            RunEvent::Finished(result) => {
                let node = self.file_mut(&result.file);
                if result.name.is_empty() {
                    node.failure = result.failure;
                    node.tests.clear();
                } else if let Some(test) = node
                    .tests
                    .iter_mut()
                    .find(|test| test.case.index == result.index)
                {
                    test.state = state_of(&result);
                }
            }
        }
    }

    fn file_mut(&mut self, path: &str) -> &mut FileNode {
        let index = match self.files.iter().position(|file| file.path == path) {
            Some(index) => index,
            None => {
                self.files.push(FileNode {
                    path: path.to_owned(),
                    tests: Vec::new(),
                    failure: None,
                });
                self.files.len() - 1
            }
        };
        &mut self.files[index]
    }

    /// Tests passed and failed (a file that couldn't run counts as one failure).
    pub fn counts(&self) -> (usize, usize) {
        let mut passed = 0;
        let mut failed = 0;
        for file in &self.files {
            if file.failure.is_some() {
                failed += 1;
            }
            for test in &file.tests {
                match test.state {
                    TestState::Passed => passed += 1,
                    TestState::Failed(_) => failed += 1,
                    TestState::NotRun | TestState::Queued => {}
                }
            }
        }
        (passed, failed)
    }

    /// Draws the panel and returns what was asked of the app.
    pub fn show(&mut self, ui: &mut egui::Ui, has_project: bool) -> Option<TestAction> {
        self.poll();
        if self.running() {
            ui.ctx().request_repaint_after(Duration::from_millis(100));
        }
        let palette = theme::palette(ui.ctx());
        let mut action = None;
        crate::responsive::toolbar(ui, |ui| {
            let run_all = widgets::button(Role::Primary, Some(icons::PLAY), "Run all", &palette);
            if ui
                .add_enabled(has_project, run_all)
                .on_hover_text("Run every test in tests/, each in a fresh copy of its level")
                .clicked()
            {
                action = Some(TestAction::Run(Selection::All));
            }
            ui.checkbox(&mut self.rerun_on_save, "Re-run on save")
                .on_hover_text("Run the last tests again whenever a script or spec is saved");
        });
        let (passed, failed) = self.counts();
        let summary = if self.running() {
            format!(
                "{} Running… {passed} passed, {failed} failed",
                icons::HOURGLASS
            )
        } else if passed + failed == 0 {
            "Tests run the project's saved files.".to_owned()
        } else {
            format!("{passed} passed, {failed} failed")
        };
        let color = if failed > 0 {
            palette.error
        } else if passed > 0 && !self.running() {
            palette.success
        } else {
            palette.text_weak
        };
        ui.label(RichText::new(summary).color(color));
        if let Some(problem) = &self.problem {
            ui.label(RichText::new(problem).color(palette.error));
        }
        widgets::divider(ui);
        if self.files.is_empty() {
            ui.add_space(16.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new(icons::FLASK)
                        .size(22.0)
                        .color(palette.text_muted),
                );
                ui.label(
                    RichText::new(
                        "No tests yet. Add a file ending in .spec.lua under tests/, with \
                         describe, it and expect.",
                    )
                    .color(palette.text_weak),
                );
            });
            return action;
        }
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                for file in &self.files {
                    if let Some(asked) = file_rows(ui, file, has_project) {
                        action = Some(asked);
                    }
                }
            });
        action
    }
}

/// Whether `selection` runs `case`.
fn picks(selection: &Selection, case: &TestCase) -> bool {
    match selection {
        Selection::All => true,
        Selection::File(file) => *file == case.file,
        Selection::One { file, index } => *file == case.file && *index == case.index,
    }
}

fn state_of(result: &TestResult) -> TestState {
    match &result.failure {
        None => TestState::Passed,
        Some(failure) => TestState::Failed(failure.clone()),
    }
}

/// The icon and its colour for a state.
fn state_icon(state: &TestState, palette: &theme::Palette) -> (&'static str, egui::Color32) {
    match state {
        TestState::NotRun => (icons::CIRCLE, palette.text_muted),
        TestState::Queued => (icons::HOURGLASS, palette.text_weak),
        TestState::Passed => (icons::CHECK_CIRCLE, palette.success),
        TestState::Failed(_) => (icons::X_CIRCLE, palette.error),
    }
}

/// A file's header and, when open, its tests.
fn file_rows(ui: &mut egui::Ui, file: &FileNode, has_project: bool) -> Option<TestAction> {
    let palette = theme::palette(ui.ctx());
    let mut action = None;
    let failed = file.failure.is_some()
        || file
            .tests
            .iter()
            .any(|test| matches!(test.state, TestState::Failed(_)));
    let id = ui.make_persistent_id(("test file", &file.path));
    let header =
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, true);
    header
        .show_header(ui, |ui| {
            let row = egui::Layout::right_to_left(egui::Align::Center);
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), ui.spacing().interact_size.y),
                row,
                |ui| {
                    ui.add_enabled_ui(has_project, |ui| {
                        let tip = format!("Run the tests in {}", file.path);
                        if widgets::icon_button(ui, icons::PLAY, &tip, false).clicked() {
                            action = Some(TestAction::Run(Selection::File(file.path.clone())));
                        }
                    });
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        let color = if failed {
                            palette.error
                        } else {
                            palette.text_muted
                        };
                        ui.label(RichText::new(icons::FILE_CODE).color(color));
                        ui.add(egui::Label::new(&file.path).truncate());
                    });
                },
            );
        })
        .body(|ui| {
            if let Some(failure) = &file.failure
                && let Some(asked) = failure_rows(ui, failure, &file.path)
            {
                action = Some(asked);
            }
            if file.tests.is_empty() && file.failure.is_none() {
                ui.label(RichText::new("Run it to list its tests.").color(palette.text_weak));
            }
            for test in &file.tests {
                if let Some(asked) = test_rows(ui, test, has_project) {
                    action = Some(asked);
                }
            }
        });
    action
}

/// One test's row, and its failure under it.
fn test_rows(ui: &mut egui::Ui, test: &TestNode, has_project: bool) -> Option<TestAction> {
    let palette = theme::palette(ui.ctx());
    let mut action = None;
    let row = egui::Layout::right_to_left(egui::Align::Center);
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), ui.spacing().interact_size.y),
        row,
        |ui| {
            ui.add_enabled_ui(has_project, |ui| {
                let tip = format!("Run \"{}\"", test.case.title());
                if widgets::icon_button(ui, icons::PLAY, &tip, false).clicked() {
                    action = Some(TestAction::Run(Selection::One {
                        file: test.case.file.clone(),
                        index: test.case.index,
                    }));
                }
            });
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                let (icon, color) = state_icon(&test.state, &palette);
                ui.label(RichText::new(icon).color(color));
                let name = test.case.name.last().cloned().unwrap_or_default();
                let label = ui
                    .add(
                        egui::Label::new(name)
                            .truncate()
                            .sense(egui::Sense::click()),
                    )
                    .on_hover_text(format!(
                        "{}\n{}:{}",
                        test.case.title(),
                        test.case.file,
                        test.case.line
                    ));
                if label.clicked() {
                    action = Some(TestAction::Open {
                        path: test.case.file.clone(),
                        line: test.case.line,
                    });
                }
            });
        },
    );
    if let TestState::Failed(failure) = &test.state
        && let Some(asked) = failure_rows(ui, failure, &test.case.file)
    {
        action = Some(asked);
    }
    action
}

/// A failure's `file:line` link and message.
fn failure_rows(ui: &mut egui::Ui, failure: &Failure, file: &str) -> Option<TestAction> {
    let palette = theme::palette(ui.ctx());
    let mut action = None;
    ui.indent("failure", |ui| {
        let (path, line) = failure
            .location
            .clone()
            .unwrap_or_else(|| (file.to_owned(), 1));
        let place = format!("{path}:{line}");
        let message = failure
            .message
            .strip_prefix(&place)
            .map(|rest| rest.trim_start_matches(':').trim_start())
            .unwrap_or(&failure.message);
        if failure.location.is_some()
            && ui
                .link(RichText::new(&place).monospace().color(palette.accent_text))
                .on_hover_text("Open the spec at this line")
                .clicked()
        {
            action = Some(TestAction::Open { path, line });
        }
        ui.add(egui::Label::new(RichText::new(message).color(palette.error)).wrap());
    });
    action
}

#[cfg(test)]
mod tests {
    use super::*;

    fn case(index: usize, name: &str) -> TestCase {
        TestCase {
            file: "tests/walk.spec.lua".to_owned(),
            index,
            name: vec!["Character".to_owned(), name.to_owned()],
            line: 10 + index as u32,
            level: None,
        }
    }

    fn result(case: &TestCase, failure: Option<&str>) -> TestResult {
        TestResult {
            file: case.file.clone(),
            index: case.index,
            name: case.name.clone(),
            line: case.line,
            failure: failure.map(Failure::new),
            output: Vec::new(),
            seconds: 1.0,
        }
    }

    #[test]
    fn results_fill_the_tree_and_the_counts() {
        let mut tests = GameTests {
            last: Some(Selection::All),
            ..GameTests::default()
        };
        let (walks, stands) = (case(1, "walks"), case(2, "stands"));
        tests.follow(RunEvent::Found {
            file: walks.file.clone(),
            tests: vec![walks.clone(), stands.clone()],
        });
        assert!(
            tests.files[0]
                .tests
                .iter()
                .all(|t| t.state == TestState::Queued)
        );
        tests.follow(RunEvent::Finished(result(
            &walks,
            Some("tests/walk.spec.lua:14: expected 0 to be greater than 5"),
        )));
        tests.follow(RunEvent::Finished(result(&stands, None)));
        assert_eq!(tests.counts(), (1, 1));
        let TestState::Failed(failure) = &tests.files[0].tests[0].state else {
            panic!("the walk failed");
        };
        assert_eq!(
            failure.location,
            Some(("tests/walk.spec.lua".to_owned(), 14))
        );

        // Running one again keeps how the other went.
        tests.last = Some(Selection::One {
            file: walks.file.clone(),
            index: 1,
        });
        tests.follow(RunEvent::Found {
            file: walks.file.clone(),
            tests: vec![walks.clone(), stands.clone()],
        });
        assert_eq!(tests.files[0].tests[0].state, TestState::Queued);
        assert_eq!(tests.files[0].tests[1].state, TestState::Passed);
    }

    #[test]
    fn a_file_that_could_not_run_is_one_failure() {
        let mut tests = GameTests::default();
        tests.follow(RunEvent::Finished(TestResult {
            file: "tests/broken.spec.lua".to_owned(),
            index: 0,
            name: Vec::new(),
            line: 1,
            failure: Some(Failure::new("tests/broken.spec.lua:1: Expected ')'")),
            output: Vec::new(),
            seconds: 0.0,
        }));
        assert_eq!(tests.counts(), (0, 1));
        assert!(tests.files[0].failure.is_some());
    }

    #[test]
    fn the_panel_draws_a_failure_with_its_location_in_a_narrow_column() {
        let ctx = theme::test_context();
        let mut tests = GameTests::default();
        let (walks, stands) = (case(1, "walks more than 5 studs"), case(2, "stands still"));
        tests.follow(RunEvent::Found {
            file: walks.file.clone(),
            tests: vec![walks.clone(), stands.clone()],
        });
        tests.follow(RunEvent::Finished(result(
            &walks,
            Some("tests/walk.spec.lua:14: expected 0 to be greater than 5"),
        )));
        tests.follow(RunEvent::Finished(result(&stands, None)));
        let size = egui::vec2(260.0, 400.0);
        let mut asked = None;
        let mut fit = None;
        let mut draw = |ctx: &egui::Context| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let room = ui.max_rect();
                asked = tests.show(ui, true);
                fit = Some((ui.min_rect(), room));
            });
        };
        for _ in 0..3 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                ..Default::default()
            };
            let _ = ctx.run(input, &mut draw);
        }
        if let Some(snapshots) = std::env::var_os("AXCENITY_SNAPSHOTS") {
            let path = PathBuf::from(snapshots).join("tests-panel.png");
            crate::snapshot::snapshot(&path, size, vec![Vec::new(); 3], &mut draw);
        }
        assert_eq!(asked, None);
        let (used, room) = fit.expect("the panel was drawn");
        assert!(
            used.width() <= room.width() + 0.5,
            "drew {} points wide in {}",
            used.width(),
            room.width()
        );
    }
}
