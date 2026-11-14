//! Checks the open code tabs and gives each tab what was found, for its
//! squiggles: the editor's own checks (`script_completion::lint`: syntax
//! errors and names the API can't have), always, and luau-lsp's type
//! errors and lints (G4 stage 2; `luau_check.rs`) when it's installed. A
//! tab is checked once its code has stayed the same for [`QUIET`], and a
//! saved tab's errors are also listed in Output, as `path:line:` links.

use super::*;
use crate::luau_check::{self, Checker, Diagnostic, ProjectFiles, Results, Severity, Sourcemap};

/// How long code must stay unchanged before it's checked, in seconds.
const QUIET: f64 = 0.4;
/// How often the project's other scripts are looked at again, in seconds,
/// so a tab is checked again when a module it may require changes.
const PROJECT_EVERY: f64 = 1.0;

pub(crate) struct LuauChecks {
    /// Off in tests unless a test turns it on, so test runs never start
    /// luau-lsp on their own.
    pub(crate) enabled: bool,
    checker: Option<Checker>,
    /// luau-lsp was looked for (it's looked for once).
    looked: bool,
    results: Results,
    /// The editor's own findings for each script, for the text (by hash)
    /// they were found in.
    built_in: HashMap<String, (u64, Vec<Diagnostic>)>,
    /// The key ([`luau_check::check_key`]) each script was last sent with:
    /// its text and every other script's, and the game tree.
    asked: HashMap<String, u64>,
    /// The project as last gathered, when, and its stamp.
    project: Option<(std::sync::Arc<ProjectFiles>, f64, u64)>,
    /// Each tab's text (by hash) and when it was first seen like that.
    seen: HashMap<String, (u64, f64)>,
    /// The errors (by hash) last listed in Output for each script.
    reported: HashMap<String, u64>,
    /// A failed run was reported in Output (once).
    failed: bool,
    /// The luau-lsp in use and where it came from, once looked for.
    pub(crate) found: Option<(std::path::PathBuf, luau_check::Source)>,
    /// Downloading the pinned luau-lsp (`luau_lsp.rs`), while it runs.
    pub(crate) download: Option<crate::luau_lsp::Download>,
    /// Why the last download failed, until another starts.
    pub(crate) download_failed: Option<String>,
    /// The bar above script tabs offering the download was closed.
    pub(crate) offer_dismissed: bool,
}

impl Default for LuauChecks {
    fn default() -> Self {
        LuauChecks {
            enabled: !cfg!(test),
            checker: None,
            looked: false,
            results: Results::default(),
            built_in: HashMap::new(),
            asked: HashMap::new(),
            project: None,
            seen: HashMap::new(),
            reported: HashMap::new(),
            failed: false,
            found: None,
            download: None,
            download_failed: None,
            offer_dismissed: false,
        }
    }
}

impl LuauChecks {
    /// Uses luau-lsp at `program` (tests).
    #[cfg(test)]
    pub(crate) fn start_with(&mut self, program: std::path::PathBuf) {
        self.enabled = true;
        self.looked = true;
        self.checker = Checker::start(program).ok();
    }
}

impl LuauChecks {
    /// Looks for luau-lsp, once (again after a download), and starts
    /// checking with it when checks are on.
    pub(crate) fn look(&mut self) {
        if self.looked {
            return;
        }
        self.looked = true;
        self.found = luau_check::find();
        self.checker = self
            .found
            .as_ref()
            .filter(|_| self.enabled)
            .and_then(|(program, _)| Checker::start(program.clone()).ok());
    }

    /// Starts downloading the luau-lsp packages ship, into the editor's
    /// data folder, unless one is already downloading.
    pub(crate) fn start_download(&mut self) {
        if self.download.is_some() {
            return;
        }
        let Some(data) = crate::luau_lsp::data_folder() else {
            self.download_failed = Some("the editor has no data folder".to_owned());
            return;
        };
        self.download_failed = None;
        self.download = Some(crate::luau_lsp::Download::start(data));
    }

    /// Whether type checking is waiting on luau-lsp being downloaded: none
    /// was found, and this computer has a pinned build to download.
    pub(crate) fn can_download(&self) -> bool {
        self.looked && self.found.is_none() && crate::luau_lsp::pinned().asset().is_some()
    }

    /// What the bar above a script tab says about the language server, if
    /// anything: offered while it's missing (until closed), with progress
    /// while it downloads.
    pub(crate) fn note(&self) -> Option<crate::scripts::ServerNote> {
        use crate::scripts::ServerNote;
        if let Some(download) = &self.download {
            return Some(ServerNote::Downloading(download.progress.fraction()));
        }
        if !self.enabled || !self.can_download() || self.offer_dismissed {
            return None;
        }
        Some(match &self.download_failed {
            Some(reason) => ServerNote::Failed(reason.clone()),
            None => ServerNote::Missing,
        })
    }
}

impl EditorApp {
    /// Sends code tabs that have changed and gone quiet to luau-lsp, and
    /// takes in what it found. True while a check is waiting or running,
    /// so the caller draws again soon.
    pub(super) fn check_scripts(&mut self, now: f64) -> bool {
        let checks = &mut self.luau;
        if !checks.enabled {
            return false;
        }
        if let Some(download) = &checks.download
            && let Some(result) = download.finished()
        {
            checks.download = None;
            match result {
                Ok(program) => {
                    self.output
                        .info(format!("Downloaded luau-lsp to {}.", program.display()));
                    // Looked for again, so the new one is used.
                    checks.looked = false;
                    checks.failed = false;
                    checks.asked.clear();
                }
                Err(reason) => {
                    self.output
                        .warning(format!("Couldn't download luau-lsp: {reason}"));
                    checks.download_failed = Some(reason);
                }
            }
        }
        checks.look();

        let mut waiting = false;
        let mut answered = Vec::new();
        if let Some(checker) = &checks.checker {
            for done in checker.finished() {
                let found = match done.result {
                    Ok(found) => found,
                    Err(reason) => {
                        if !checks.failed {
                            checks.failed = true;
                            self.output.warning(format!(
                                "Couldn't check scripts with luau-lsp ({}): {reason}",
                                checker.program().display()
                            ));
                        }
                        Vec::new()
                    }
                };
                checks.results.set(&done.path, done.key, found);
                // Only the latest request counts; an older answer is
                // dropped.
                if checks.asked.get(&done.path) == Some(&done.key) {
                    answered.push(done.path);
                }
            }
        }

        // The project is gathered again when a tab has gone quiet with new
        // text, and every second while tabs are open, so a change to
        // another script (a module saved, a file changed elsewhere, a
        // script moved) checks the tabs again.
        let quiet_and_new = |tab: &crate::scripts::ScriptTab, checks: &LuauChecks| {
            let hash = luau_check::text_hash(&tab.text);
            matches!(checks.seen.get(&tab.path), Some(&(seen, since)) if seen == hash && now - since >= QUIET)
                && checks
                    .built_in
                    .get(&tab.path)
                    .is_none_or(|(checked, _)| *checked != hash)
        };
        let lua_tabs = || {
            self.tabs
                .iter()
                .filter(|tab| !tab.is_stylesheet() && tab.path.ends_with(".lua"))
        };
        if self.luau.checker.is_some()
            && lua_tabs().next().is_some()
            && (lua_tabs().any(|tab| quiet_and_new(tab, &self.luau))
                || self
                    .luau
                    .project
                    .as_ref()
                    .is_none_or(|(_, at, _)| now - at >= PROJECT_EVERY))
        {
            let project = self.project_files();
            let stamp = project.stamp();
            self.luau.project = Some((std::sync::Arc::new(project), now, stamp));
        }

        let checks = &mut self.luau;
        for tab in self.tabs.iter_mut() {
            if tab.is_stylesheet() || !tab.path.ends_with(".lua") {
                continue;
            }
            let hash = luau_check::text_hash(&tab.text);
            match checks.seen.get(&tab.path) {
                Some(&(seen, since)) if seen == hash => {
                    if now - since < QUIET {
                        waiting = true;
                        continue;
                    }
                }
                _ => {
                    checks.seen.insert(tab.path.clone(), (hash, now));
                    waiting = true;
                    continue;
                }
            }
            // Quiet: the editor's own checks, once for each text.
            let mut changed = answered.contains(&tab.path);
            if checks
                .built_in
                .get(&tab.path)
                .is_none_or(|(checked, _)| *checked != hash)
            {
                let found = crate::script_completion::lint::check(&tab.text, &self.world);
                checks.built_in.insert(tab.path.clone(), (hash, found));
                changed = true;
            }
            // luau-lsp, again whenever the project has changed.
            if let (Some(checker), Some((project, _, stamp))) = (&checks.checker, &checks.project) {
                let key = luau_check::check_key(&tab.path, *stamp);
                if checks.asked.get(&tab.path) != Some(&key) {
                    checker.check(&tab.path, key, project.clone());
                    checks.asked.insert(tab.path.clone(), key);
                }
                if checks.results.get(&tab.path, key).is_none() {
                    waiting = true;
                }
            }
            if changed {
                show(checks, &mut self.output, tab, hash);
            }
        }
        // A download's progress is drawn as it goes.
        waiting || self.luau.download.is_some()
    }

    /// Every Lua script in the project as the editor has it (an open tab's
    /// unsaved text), and where each sits in the game tree, for luau-lsp to
    /// follow `require`s.
    pub(crate) fn project_files(&self) -> ProjectFiles {
        let mut files: BTreeMap<String, String> = BTreeMap::new();
        let mut paths: HashMap<InstanceId, String> = HashMap::new();
        if let Some(project) = &self.project {
            for script in &project.scripts {
                files.insert(script.path.clone(), script.source.clone());
                paths.insert(script.instance, script.path.clone());
            }
            for (path, text) in &project.object_files {
                if path.ends_with(".lua") {
                    files.insert(path.clone(), text.clone());
                }
            }
        }
        for tab in self.tabs.iter() {
            if !tab.is_stylesheet() && tab.path.ends_with(".lua") {
                files.insert(tab.path.clone(), tab.text.clone());
            }
        }
        let sourcemap =
            sourcemap(&self.world, self.world.root(), &paths).unwrap_or_else(|| Sourcemap {
                name: "Game".to_owned(),
                class_name: "DataModel".to_owned(),
                ..Default::default()
            });
        ProjectFiles {
            files: files.into_iter().collect(),
            sourcemap,
        }
    }
}

/// The part of the game tree under `id` that leads to scripts with files:
/// each script's file is the path `paths` gives it (a service's script) or
/// its `SourceFile` (a script inside an object).
fn sourcemap(
    world: &World,
    id: InstanceId,
    paths: &HashMap<InstanceId, String>,
) -> Option<Sourcemap> {
    let instance = world.get(id)?;
    let file = paths
        .get(&id)
        .cloned()
        .or_else(|| instance.source_file().map(str::to_owned))
        .filter(|_| instance.class().is_script());
    let children: Vec<Sourcemap> = instance
        .children()
        .iter()
        .filter_map(|child| sourcemap(world, *child, paths))
        .collect();
    if file.is_none() && children.is_empty() && id != world.root() {
        return None;
    }
    Some(Sourcemap {
        name: instance.name().to_owned(),
        class_name: if id == world.root() {
            "DataModel".to_owned()
        } else {
            instance.class().as_str().to_owned()
        },
        file,
        children,
    })
}

/// Gives `tab` (whose text is `hash`) what was found in it: luau-lsp's
/// findings for the latest request, once they're in, and the editor's own, except on lines
/// luau-lsp already has something to say about. A saved tab's errors go
/// to Output too.
fn show(
    checks: &mut LuauChecks,
    output: &mut Output,
    tab: &mut crate::scripts::ScriptTab,
    hash: u64,
) {
    let mut found: Vec<Diagnostic> = checks
        .asked
        .get(&tab.path)
        .and_then(|key| checks.results.get(&tab.path, *key))
        .map(<[Diagnostic]>::to_vec)
        .unwrap_or_default();
    if let Some((checked, own)) = checks.built_in.get(&tab.path)
        && *checked == hash
    {
        let lines: Vec<usize> = found.iter().map(|found| found.line).collect();
        found.extend(own.iter().filter(|own| !lines.contains(&own.line)).cloned());
    }
    found.sort_by_key(|found| (found.line, found.column));
    if !tab.unsaved() {
        report(output, &mut checks.reported, &tab.path, &found);
    }
    tab.problems = found;
}

/// Lists a saved script's errors in Output, when they're not the ones
/// listed last time.
fn report(
    output: &mut Output,
    reported: &mut HashMap<String, u64>,
    path: &str,
    found: &[Diagnostic],
) {
    let errors: Vec<&Diagnostic> = found
        .iter()
        .filter(|found| found.severity == Severity::Error)
        .collect();
    let summary: String = errors
        .iter()
        .map(|error| format!("{}:{}\n", error.line, error.message))
        .collect();
    let hash = luau_check::text_hash(&summary);
    if reported.insert(path.to_owned(), hash) == Some(hash) || errors.is_empty() {
        return;
    }
    for error in errors {
        let message = error.message.lines().next().unwrap_or("");
        output.error(format!("{path}:{}: {message}", error.line));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::copy_of_example;
    use crate::output::OutputKind;

    /// Runs checks as frames 0.1 s apart until `done` or 30 s pass.
    fn check_until(app: &mut EditorApp, now: &mut f64, done: impl Fn(&EditorApp) -> bool) {
        let started = std::time::Instant::now();
        while !done(app) {
            assert!(
                started.elapsed().as_secs() < 30,
                "luau-lsp didn't answer: {:?}",
                app.output
                    .lines()
                    .iter()
                    .map(|line| &line.text)
                    .collect::<Vec<_>>()
            );
            app.check_scripts(*now);
            *now += 0.1;
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }

    #[test]
    fn a_tab_is_checked_once_its_code_goes_quiet_and_saved_errors_go_to_output() {
        let Some((program, _)) = luau_check::find() else {
            eprintln!("skipped: no luau-lsp");
            return;
        };
        let folder = copy_of_example("hello-touched", "luau-checks");
        let mut app = EditorApp::new(Some(&folder));
        app.luau.start_with(program);
        let path = "scripts/pad.server.lua";
        app.open_script(path, None);
        let mut now = 0.0;

        // Typed, unsaved: underlined, but not in Output.
        app.tabs.get_mut(path).unwrap().text = "print(nope)\n".to_owned();
        check_until(&mut app, &mut now, |app| {
            !app.tabs.get(path).unwrap().problems.is_empty()
        });
        let problem = &app.tabs.get(path).unwrap().problems[0];
        assert_eq!((problem.line, problem.severity), (1, Severity::Error));
        assert!(problem.message.contains("nope"), "{problem:?}");
        let errors = |app: &EditorApp| {
            app.output
                .lines()
                .iter()
                .filter(|line| line.kind == OutputKind::Error && line.text.contains("nope"))
                .count()
        };
        assert_eq!(errors(&app), 0);

        // Saved (with a change, so it's checked again): the error is
        // listed once, as a link to its line.
        let saved = "print(nope)\n\n";
        app.tabs.get_mut(path).unwrap().text = saved.to_owned();
        assert!(app.save());
        check_until(&mut app, &mut now, |app| errors(app) == 1);
        let line = app
            .output
            .lines()
            .iter()
            .rev()
            .find(|line| line.text.contains("nope"))
            .unwrap();
        assert!(
            line.text.starts_with("scripts/pad.server.lua:1:"),
            "{}",
            line.text
        );
        assert!(line.location.is_some());

        // Fixed: the squiggle goes, and nothing more is listed.
        app.tabs.get_mut(path).unwrap().text = "print('fine')\n".to_owned();
        check_until(&mut app, &mut now, |app| {
            app.tabs.get(path).unwrap().problems.is_empty()
        });
        assert_eq!(errors(&app), 1);
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn without_luau_lsp_tabs_still_get_the_editors_own_checks() {
        let mut app = EditorApp::new(None);
        app.luau.enabled = true;
        app.luau.looked = true;
        assert!(!app.check_scripts(0.0));

        let folder = copy_of_example("hello-touched", "built-in-checks");
        let mut app = EditorApp::new(Some(&folder));
        app.luau.enabled = true;
        app.luau.looked = true;
        let path = "scripts/pad.server.lua";
        app.open_script(path, None);
        app.tabs.get_mut(path).unwrap().text = "workspace.Floor:Destory()\n".to_owned();
        let mut now = 0.0;
        while app.tabs.get(path).unwrap().problems.is_empty() {
            assert!(now < 5.0, "never checked");
            app.check_scripts(now);
            now += 0.1;
        }
        let problem = &app.tabs.get(path).unwrap().problems[0];
        assert_eq!((problem.line, problem.severity), (1, Severity::Error));
        assert!(problem.message.contains("Destory"), "{problem:?}");
        // Unsaved, so not in Output yet.
        assert!(
            !app.output
                .lines()
                .iter()
                .any(|line| line.text.contains("Destory"))
        );
        let _ = std::fs::remove_dir_all(&folder);
    }

    /// The problems luau-lsp found in `path`'s tab, as (line, message).
    fn lsp_problems(app: &EditorApp, path: &str) -> Vec<(usize, String)> {
        app.tabs
            .get(path)
            .unwrap()
            .problems
            .iter()
            .filter(|problem| problem.code.ends_with("Error"))
            .map(|problem| (problem.line, problem.message.clone()))
            .collect()
    }

    /// `require(game.ReplicatedStorage.Builder)` is followed into the
    /// module's file, its unsaved tab text included, and editing the module
    /// checks the script that requires it again.
    #[test]
    fn requires_are_followed_into_modules_and_a_module_edit_checks_again() {
        let Some((program, _)) = luau_check::find() else {
            eprintln!("skipped: no luau-lsp");
            return;
        };
        let folder = copy_of_example("hello-modules", "luau-requires");
        let mut app = EditorApp::new(Some(&folder));
        app.luau.start_with(program);

        // The sourcemap puts each module where `require` looks for it.
        let project = app.project_files();
        let storage = project
            .sourcemap
            .children
            .iter()
            .find(|child| child.name == "ReplicatedStorage")
            .expect("ReplicatedStorage leads to modules");
        assert!(storage.children.iter().any(|module| {
            module.name == "Builder"
                && module.class_name == "ModuleScript"
                && module.file.as_deref() == Some("scripts/shared/Builder.module.lua")
        }));

        let script = "scripts/stairs.server.lua";
        app.open_script(script, None);
        app.tabs.get_mut(script).unwrap().text = "\
local Builder = require(game:GetService(\"ReplicatedStorage\").Builder)
local steps = Builder.stairs(8)
Builder.spiral(3)
print(#steps)
"
        .to_owned();
        let mut now = 0.0;
        check_until(&mut app, &mut now, |app| {
            !lsp_problems(app, script).is_empty()
        });
        let found = lsp_problems(&app, script);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].0, 3);
        assert!(found[0].1.contains("spiral"), "{found:?}");

        // The module gains `spiral` in its tab, unsaved: the script is
        // checked again against it and the error goes.
        let module = "scripts/shared/Builder.module.lua";
        app.open_script(module, None);
        let text = app.tabs.get(module).unwrap().text.replace(
            "return Builder",
            "function Builder.spiral(turns: number)\n    return turns\nend\n\nreturn Builder",
        );
        app.tabs.get_mut(module).unwrap().text = text;
        check_until(&mut app, &mut now, |app| {
            lsp_problems(app, script).is_empty()
        });

        // And its types come through: `turns` is a number.
        app.tabs.get_mut(script).unwrap().text =
            "local Builder = require(game.ReplicatedStorage.Builder)\nBuilder.spiral(\"three\")\n"
                .to_owned();
        check_until(&mut app, &mut now, |app| {
            !lsp_problems(app, script).is_empty()
        });
        assert_eq!(lsp_problems(&app, script)[0].0, 2);
        let _ = std::fs::remove_dir_all(&folder);
    }

    /// Checked as the editor checks them (Roblox's platform, the
    /// sourcemap, the definitions without `require`), every example's
    /// scripts have no errors: following requires adds no false ones.
    #[test]
    fn every_example_script_checks_without_errors() {
        let Some((program, _)) = luau_check::find() else {
            eprintln!("skipped: no luau-lsp");
            return;
        };
        let checker = Checker::start(program).unwrap();
        let examples = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
        let mut checked = 0;
        let mut errors = Vec::new();
        for entry in std::fs::read_dir(&examples).unwrap().flatten() {
            if !entry.path().join("world.json").is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            let app = EditorApp::new(Some(&entry.path()));
            let project = std::sync::Arc::new(app.project_files());
            for (path, _) in &project.files {
                if !path.ends_with(".lua") {
                    continue;
                }
                let key = luau_check::check_key(path, project.stamp());
                checker.check(path, key, project.clone());
                let done = checker.wait(std::time::Duration::from_secs(30)).unwrap();
                checked += 1;
                for problem in done.result.unwrap() {
                    if problem.severity == Severity::Error {
                        errors.push(format!(
                            "{name}/{path}:{}: {}",
                            problem.line, problem.message
                        ));
                    }
                }
            }
        }
        assert!(checked > 50, "only {checked} scripts");
        assert!(errors.is_empty(), "{}", errors.join("\n"));
    }
}
