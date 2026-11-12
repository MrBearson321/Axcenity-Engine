//! The game test runner (Phase 53). A project's `tests/**/*.spec.lua`
//! files declare tests with `describe` and `it`, and check the game with
//! `expect`; `Test` drives it (`runner.lua`, run by
//! `ScriptRuntime::start_spec`).
//!
//! Each test runs in a fresh copy of its level ([`game::TestGame`]), with
//! both sides running and no window or audio, so one test can't change
//! another, and the project's files are never written. The editor's Tests
//! panel and the `axcenity-test` program both run tests through
//! [`Runner`], so they find the same tests and get the same results.
//! Exported games never include the `tests` folder (`project::pack_game`).

pub mod crowd;
pub mod game;
pub mod junit;
pub mod s10k;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use axcenity_core::World;
use axcenity_core::bench::BenchRun;
use axcenity_core::input::InputMap;
use axcenity_project::ScriptFile;
use axcenity_script::TestReport;

use crate::game::{GameSetup, TestGame};

/// The folder spec files live in, under the project folder.
pub const TESTS_FOLDER: &str = "tests";
/// What a spec file's name ends with.
pub const SPEC_SUFFIX: &str = ".spec.lua";
/// A test still running after this much game time fails.
pub const TIMEOUT_SECONDS: f64 = 120.0;
/// The test runner's Lua: `describe`, `it`, `expect` and `Test`.
const RUNNER: &str = include_str!("runner.lua");

/// One test a spec file declares.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestCase {
    /// The spec file, relative to the project with `/` (`tests/walk.spec.lua`).
    pub file: String,
    /// Which `it` of the file this is, from 1, in the order declared.
    pub index: usize,
    /// The `describe` names around it, outermost first, then its own.
    pub name: Vec<String>,
    /// The line of its `it`.
    pub line: u32,
    /// The level it runs in, `None` for the start level.
    pub level: Option<String>,
}

impl TestCase {
    /// `Character > walks forward`.
    pub fn title(&self) -> String {
        self.name.join(" > ")
    }
}

/// Why a test (or a spec file) failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    /// What went wrong, starting with `path:line:` when Lua knows where.
    pub message: String,
    /// Where, from the message: the file and line.
    pub location: Option<(String, u32)>,
}

impl Failure {
    pub fn new(message: impl Into<String>) -> Self {
        let message = message.into();
        let location = location(&message);
        Self { message, location }
    }
}

/// `path:line` at the start of `message`, for a `.lua` file.
fn location(message: &str) -> Option<(String, u32)> {
    let (path, rest) = message.split_once(".lua:")?;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    Some((format!("{path}.lua"), digits.parse().ok()?))
}

/// How one test went. A spec file that couldn't run at all is one failed
/// result with no name.
#[derive(Clone, Debug, PartialEq)]
pub struct TestResult {
    pub file: String,
    pub index: usize,
    pub name: Vec<String>,
    pub line: u32,
    pub failure: Option<Failure>,
    /// What scripts printed while it ran, and their errors.
    pub output: Vec<String>,
    /// Game time it ran for.
    pub seconds: f64,
}

impl TestResult {
    pub fn passed(&self) -> bool {
        self.failure.is_none()
    }

    pub fn title(&self) -> String {
        if self.name.is_empty() {
            self.file.clone()
        } else {
            self.name.join(" > ")
        }
    }

    fn of_file(file: &str, failure: Failure) -> Self {
        Self {
            file: file.to_owned(),
            index: 0,
            line: failure.location.as_ref().map_or(0, |(_, line)| *line),
            name: Vec::new(),
            failure: Some(failure),
            output: Vec::new(),
            seconds: 0.0,
        }
    }
}

/// Which tests to run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Selection {
    All,
    /// Every test of one spec file.
    File(String),
    /// One test, by its file and index.
    One {
        file: String,
        index: usize,
    },
}

/// What a run tells whoever started it, as it goes.
#[derive(Clone, Debug)]
pub enum RunEvent {
    /// A spec file's tests, before they run.
    Found { file: String, tests: Vec<TestCase> },
    /// A test (or a file that couldn't run) is done.
    Finished(TestResult),
}

/// One level of the project, loaded once and copied for each test.
struct Level {
    world: World,
    scripts: Vec<ScriptFile>,
}

/// Runs a project's tests. The project is read once; levels load when a
/// test first needs them.
pub struct Runner {
    folder: PathBuf,
    name: String,
    start_level: String,
    input_map: InputMap,
    /// Every level for `LevelService`, by name.
    worlds: BTreeMap<String, World>,
    /// Loaded levels, by name.
    levels: BTreeMap<String, Level>,
}

impl Runner {
    /// Reads the project in `folder`. A project from an older engine has
    /// to be upgraded first (by opening it in the editor), because upgrading
    /// writes its files.
    pub fn open(folder: &Path) -> Result<Self, String> {
        let manifest = axcenity_project::read_manifest(folder).ok_or_else(|| {
            format!(
                "`{}` is not an Axcenity project (it has no readable {})",
                folder.display(),
                axcenity_project::MANIFEST_FILE
            )
        })?;
        if manifest.format_version != axcenity_project::FORMAT_VERSION {
            return Err(format!(
                "\"{}\" is in project format {}, not {}; open it in the editor once to upgrade it",
                manifest.name,
                manifest.format_version,
                axcenity_project::FORMAT_VERSION
            ));
        }
        let project = axcenity_project::load(folder)
            .map_err(|err| format!("could not open the project: {err}"))?;
        let start_level = project.manifest.start_level.clone();
        let mut problems = Vec::new();
        let mut worlds: BTreeMap<String, World> =
            axcenity_project::load_levels(folder, &mut problems)
                .into_iter()
                .collect();
        worlds.insert(start_level.clone(), project.world.clone());
        let mut levels = BTreeMap::new();
        levels.insert(
            start_level.clone(),
            Level {
                world: project.world,
                scripts: project.scripts,
            },
        );
        Ok(Self {
            folder: folder.to_owned(),
            name: project.manifest.name,
            start_level,
            input_map: project.input_map,
            worlds,
            levels,
        })
    }

    /// The project's name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The project's spec files, relative to it with `/`, sorted.
    pub fn spec_files(&self) -> Vec<String> {
        spec_files(&self.folder)
    }

    /// Runs the tests `selection` picks, telling `on_event` about each
    /// file's tests and then each result, and returns the results. A test
    /// that fails or errors never stops the rest.
    pub fn run(
        &mut self,
        selection: &Selection,
        on_event: &mut dyn FnMut(RunEvent),
    ) -> Vec<TestResult> {
        let files = match selection {
            Selection::All => self.spec_files(),
            Selection::File(file) | Selection::One { file, .. } => vec![file.clone()],
        };
        let mut results = Vec::new();
        for file in files {
            let mut finish = |result: TestResult, on_event: &mut dyn FnMut(RunEvent)| {
                on_event(RunEvent::Finished(result.clone()));
                results.push(result);
            };
            let tests = match self.collect(&file) {
                Ok(tests) => tests,
                Err(failure) => {
                    finish(TestResult::of_file(&file, failure), on_event);
                    continue;
                }
            };
            on_event(RunEvent::Found {
                file: file.clone(),
                tests: tests.clone(),
            });
            for test in tests {
                if let Selection::One { index, .. } = selection
                    && test.index != *index
                {
                    continue;
                }
                let result = self.run_test(&test);
                finish(result, on_event);
            }
        }
        results
    }

    /// The tests the spec `file` declares, from running its top level in a
    /// copy of the start level (without the game's scripts).
    pub fn collect(&mut self, file: &str) -> Result<Vec<TestCase>, Failure> {
        let source = self.read_spec(file)?;
        let start = self.start_level.clone();
        let saves = SaveFolder::new();
        let mut game = self.start_game(&start, false, &saves)?;
        game.start_spec(RUNNER, file, &source, None);
        let mut tests = Vec::new();
        for report in game.take_reports() {
            match report {
                TestReport::Found(found) => tests.push(TestCase {
                    file: file.to_owned(),
                    index: tests.len() + 1,
                    name: found.name,
                    line: found.line,
                    level: found.level,
                }),
                TestReport::Finished(Some(message)) => return Err(Failure::new(message)),
                TestReport::Finished(None) => return Ok(tests),
            }
        }
        Err(Failure::new(format!(
            "{file}: the file waited outside a test; put waits inside `it`"
        )))
    }

    /// Runs one test in a fresh copy of its level, until it finishes,
    /// fails or times out.
    pub fn run_test(&mut self, test: &TestCase) -> TestResult {
        let mut result = TestResult {
            file: test.file.clone(),
            index: test.index,
            name: test.name.clone(),
            line: test.line,
            failure: None,
            output: Vec::new(),
            seconds: 0.0,
        };
        let source = match self.read_spec(&test.file) {
            Ok(source) => source,
            Err(failure) => {
                result.failure = Some(failure);
                return result;
            }
        };
        let level = test
            .level
            .clone()
            .unwrap_or_else(|| self.start_level.clone());
        let saves = SaveFolder::new();
        let mut game = match self.start_game(&level, true, &saves) {
            Ok(game) => game,
            Err(failure) => {
                result.failure = Some(failure);
                return result;
            }
        };
        game.start_spec(RUNNER, &test.file, &source, Some(test.index));
        let failure = loop {
            // Something asked for that couldn't be done fails the test,
            // even when it finished in the same frame.
            if let Some(problem) = game.problem.take() {
                break Some(Failure::new(problem));
            }
            let finished = game
                .take_reports()
                .into_iter()
                .find_map(|report| match report {
                    TestReport::Finished(failure) => Some(failure),
                    TestReport::Found(_) => None,
                });
            if let Some(failure) = finished {
                break failure.map(Failure::new);
            }
            if !game.sim.scripts.spec_running() {
                // Stopped by an error the runner couldn't catch (the
                // watchdog's), which the output has.
                let error = game
                    .output
                    .iter()
                    .rev()
                    .find_map(|line| line.strip_prefix("error: "))
                    .map(|line| line.lines().next().unwrap_or(line).to_owned());
                break Some(Failure::new(error.unwrap_or_else(|| {
                    "the test stopped without finishing".to_owned()
                })));
            }
            if game.seconds >= TIMEOUT_SECONDS {
                break Some(Failure::new(format!(
                    "{}:{}: timed out after {TIMEOUT_SECONDS} seconds",
                    test.file, test.line
                )));
            }
            game.frame();
        };
        result.failure = failure;
        result.seconds = game.seconds;
        result.output = std::mem::take(&mut game.output);
        result
    }

    fn read_spec(&self, file: &str) -> Result<String, Failure> {
        std::fs::read_to_string(self.folder.join(file))
            .map_err(|err| Failure::new(format!("{file}: could not read the file: {err}")))
    }

    /// Runs the start level with its scripts, as a test would, for
    /// `warm_up` frames then `frames` measured ones, and returns each
    /// frame's steps' times (`--bench`, roadmap I1), with the errors and
    /// warnings scripts printed.
    pub fn bench(
        &mut self,
        warm_up: usize,
        frames: usize,
    ) -> Result<(BenchRun, Vec<String>), String> {
        let saves = SaveFolder::new();
        let level = self.start_level.clone();
        let mut game = self
            .start_game(&level, true, &saves)
            .map_err(|failure| failure.message)?;
        let mut run = BenchRun::new(warm_up, frames);
        let mut problems: Vec<String> = game
            .output
            .drain(..)
            .filter(|line| line.starts_with("error:") || line.starts_with("warning:"))
            .collect();
        loop {
            game.frame();
            problems.extend(
                game.output
                    .drain(..)
                    .filter(|line| line.starts_with("error:") || line.starts_with("warning:")),
            );
            if run.record(&game.laps) {
                return Ok((run, problems));
            }
        }
    }

    /// A fresh copy of `level`, with the game's scripts started when
    /// `scripts` is true, saving in `saves`.
    fn start_game(
        &mut self,
        level: &str,
        scripts: bool,
        saves: &SaveFolder,
    ) -> Result<TestGame, Failure> {
        if !self.levels.contains_key(level) {
            if !self.worlds.contains_key(level) {
                return Err(Failure::new(format!(
                    "the project has no level named \"{level}\""
                )));
            }
            let project = axcenity_project::load_with_level(&self.folder, Some(level), &mut |_| {})
                .map_err(|err| {
                    Failure::new(format!("could not load the level \"{level}\": {err}"))
                })?;
            self.levels.insert(
                level.to_owned(),
                Level {
                    world: project.world,
                    scripts: project.scripts,
                },
            );
        }
        let loaded = &self.levels[level];
        Ok(TestGame::start(GameSetup {
            folder: &self.folder,
            world: loaded.world.clone(),
            scripts: &loaded.scripts,
            input_map: &self.input_map,
            levels: (&self.start_level, level, self.worlds.clone()),
            save_folder: saves.0.clone(),
            start_scripts: scripts,
        }))
    }
}

/// The project's spec files under `folder`, relative to it with `/`, sorted.
pub fn spec_files(folder: &Path) -> Vec<String> {
    let mut files = Vec::new();
    collect_specs(folder, &folder.join(TESTS_FOLDER), &mut files);
    files.sort();
    files
}

fn collect_specs(root: &Path, directory: &Path, files: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_specs(root, &path, files);
        } else if let Some(relative) = path.strip_prefix(root).ok().and_then(Path::to_str) {
            let relative = relative.replace('\\', "/");
            if relative.ends_with(SPEC_SUFFIX) {
                files.push(relative);
            }
        }
    }
}

/// True if `path` (relative to the project, with `/`) is a spec file, so
/// the editor re-runs tests when it's saved.
pub fn is_spec_file(path: &str) -> bool {
    path.starts_with("tests/") && path.ends_with(SPEC_SUFFIX)
}

/// A fresh folder for one test's save slots, in the system's temporary
/// folder, never the project. It's removed when dropped.
struct SaveFolder(PathBuf);

impl SaveFolder {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let number = NEXT.fetch_add(1, Ordering::Relaxed);
        Self(
            std::env::temp_dir()
                .join("axcenity-test-saves")
                .join(format!("{}-{number}", std::process::id())),
        )
    }
}

impl Drop for SaveFolder {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod tests;
