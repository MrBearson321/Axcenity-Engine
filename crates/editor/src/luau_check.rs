//! Type errors and lint warnings for code tabs (roadmap G4, stage 2), from
//! luau-lsp run as a child process: `luau-lsp analyze` on the tab's text,
//! with the engine's definitions, on a background thread.
//!
//! Every package ships luau-lsp in `tools/` beside the editor, and an
//! editor built from source can download the same version (`luau_lsp.rs`).
//! [`find`] takes `AXCENITY_LUAU_LSP` first, then those, then where a
//! developer would have one: `luau-lsp` on the PATH, or the server bundled
//! with VS Code's Luau extension. Without any, tabs get only the editor's
//! own checks (`script_completion/lint.rs`).
//!
//! Each run writes the text to `script.lua` in a folder of its own under
//! the system's temp folder, beside the definitions
//! (`axcenity_script::lua_definitions`) and settings that match
//! `autocomplete.rs`'s (the standard platform, and no globals the sandbox
//! leaves out), and reads the `--formatter=gnu` lines it prints.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

/// The environment variable that names the luau-lsp program, first.
pub const PROGRAM_VARIABLE: &str = "AXCENITY_LUAU_LSP";
/// A run that takes longer is stopped.
const TIME_LIMIT: Duration = Duration::from_secs(10);

/// luau-lsp's settings for a run: as `autocomplete.rs` writes for VS Code,
/// but on Roblox's platform with the checker's sourcemap, so `require`s
/// are followed.
const SETTINGS: &str = r#"{
  "luau-lsp.platform.type": "roblox",
  "luau-lsp.sourcemap.enabled": true,
  "luau-lsp.types.disabledGlobals": [
    "debug", "gcinfo", "getfenv", "loadstring", "newproxy", "setfenv", "unpack", "vector"
  ]
}"#;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    /// A type or syntax error.
    Error,
    /// A lint, such as an unused local.
    Warning,
}

/// One problem luau-lsp found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    /// Counting from 1.
    pub line: usize,
    /// The first byte of the line it covers, counting from 1.
    pub column: usize,
    pub end_line: usize,
    /// The last byte it covers, counting from 1.
    pub end_column: usize,
    pub severity: Severity,
    /// Its kind, such as `TypeError` or `LocalUnused`.
    pub code: String,
    pub message: String,
}

/// A hash of a script's text, to match a result to the text it was for.
pub fn text_hash(text: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

/// Reads `luau-lsp analyze --formatter=gnu` output:
/// `file:line.column-line.column: Kind: message`, a message's later lines
/// following on lines of their own.
pub fn parse(output: &str) -> Vec<Diagnostic> {
    let mut found: Vec<Diagnostic> = Vec::new();
    for line in output.lines() {
        if is_log_line(line) {
            continue;
        }
        match parse_line(line) {
            Some(diagnostic) => found.push(diagnostic),
            None => {
                if let Some(last) = found.last_mut()
                    && !line.trim().is_empty()
                {
                    last.message.push('\n');
                    last.message.push_str(line.trim_end());
                }
            }
        }
    }
    found
}

/// luau-lsp's own log lines on stderr (`[INFO] Loading definitions file`,
/// `[WARN] …`), which aren't part of any finding.
fn is_log_line(line: &str) -> bool {
    line.strip_prefix('[')
        .and_then(|rest| rest.split_once(']'))
        .is_some_and(|(level, _)| {
            !level.is_empty() && level.chars().all(|c| c.is_ascii_uppercase())
        })
}

fn parse_line(line: &str) -> Option<Diagnostic> {
    // The file name may hold `:` (a Windows drive), so read from the
    // `: Kind: ` after the position.
    let (head, message) = split_kind(line)?;
    let (head, code) = head;
    let (_, range) = head.rsplit_once(':')?;
    let (start, end) = range.split_once('-')?;
    let (line_number, column) = start.split_once('.')?;
    let (end_line, end_column) = end.split_once('.')?;
    let severity = if code.ends_with("Error") {
        Severity::Error
    } else {
        Severity::Warning
    };
    Some(Diagnostic {
        line: line_number.parse().ok()?,
        column: column.parse().ok()?,
        end_line: end_line.parse().ok()?,
        end_column: end_column.parse().ok()?,
        severity,
        code: code.to_owned(),
        message: message.to_owned(),
    })
}

/// `(("file:range", "Kind"), "message")`.
fn split_kind(line: &str) -> Option<((&str, &str), &str)> {
    // The first `: Word: ` whose word has no spaces, after a range.
    let mut from = 0;
    while let Some(at) = line[from..].find(": ") {
        let at = from + at;
        let rest = &line[at + 2..];
        if let Some((code, message)) = rest.split_once(": ")
            && !code.is_empty()
            && code.chars().all(|c| c.is_ascii_alphanumeric())
            && line[..at].rsplit(':').next().is_some_and(|range| {
                range.contains('-')
                    && range
                        .chars()
                        .all(|c| c.is_ascii_digit() || c == '.' || c == '-')
            })
        {
            return Some(((&line[..at], code), message));
        }
        from = at + 2;
    }
    None
}

/// Where the luau-lsp a check uses came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// `AXCENITY_LUAU_LSP` names it.
    Variable,
    /// Shipped beside the editor (`tools/`).
    Bundled,
    /// Downloaded into the editor's data folder (`luau_lsp.rs`).
    Downloaded,
    /// `luau-lsp` on the PATH.
    Path,
    /// VS Code's Luau extension's server.
    VsCode,
}

impl Source {
    pub fn describe(self) -> &'static str {
        match self {
            Source::Variable => "named by AXCENITY_LUAU_LSP",
            Source::Bundled => "shipped with the editor",
            Source::Downloaded => "downloaded by the editor",
            Source::Path => "found on the PATH",
            Source::VsCode => "from VS Code's Luau extension",
        }
    }
}

/// The places luau-lsp is looked for, so tests can pick them.
#[derive(Default)]
pub struct Places {
    pub variable: Option<PathBuf>,
    pub bundled: Option<PathBuf>,
    pub downloaded: Option<PathBuf>,
    pub path: Option<std::ffi::OsString>,
    pub vs_code_extensions: Option<PathBuf>,
}

impl Places {
    /// This computer's.
    pub fn here() -> Places {
        let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" });
        Places {
            variable: std::env::var_os(PROGRAM_VARIABLE).map(PathBuf::from),
            bundled: crate::luau_lsp::bundled(),
            downloaded: crate::luau_lsp::data_folder()
                .map(|data| crate::luau_lsp::downloaded(&data)),
            path: std::env::var_os("PATH"),
            vs_code_extensions: home
                .map(|home| PathBuf::from(home).join(".vscode").join("extensions")),
        }
    }
}

/// Where luau-lsp is: `AXCENITY_LUAU_LSP`, else the one shipped beside the
/// editor, else the one the editor downloaded (both the version
/// `packaging/luau-lsp.toml` pins), else `luau-lsp` on the PATH, else the
/// newest server inside VS Code's Luau extension.
pub fn find() -> Option<(PathBuf, Source)> {
    find_in(&Places::here())
}

pub fn find_in(places: &Places) -> Option<(PathBuf, Source)> {
    if let Some(program) = &places.variable {
        return program
            .is_file()
            .then(|| (program.clone(), Source::Variable));
    }
    for (program, source) in [
        (&places.bundled, Source::Bundled),
        (&places.downloaded, Source::Downloaded),
    ] {
        if let Some(program) = program.as_ref().filter(|program| program.is_file()) {
            return Some((program.clone(), source));
        }
    }
    let name = crate::luau_lsp::program_name();
    if let Some(paths) = &places.path {
        for folder in std::env::split_paths(paths) {
            let program = folder.join(name);
            if program.is_file() {
                return Some((program, Source::Path));
            }
        }
    }
    vs_code_server(places.vs_code_extensions.as_ref()?).map(|program| (program, Source::VsCode))
}

/// The server in the newest `johnnymorganz.luau-lsp-<version>` folder in
/// `extensions`.
fn vs_code_server(extensions: &Path) -> Option<PathBuf> {
    let server = if cfg!(windows) {
        "server.exe"
    } else {
        "server"
    };
    let mut found: Vec<(Vec<u32>, PathBuf)> = std::fs::read_dir(extensions)
        .ok()?
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_lowercase();
            let version = name.strip_prefix("johnnymorganz.luau-lsp-")?;
            let version: Vec<u32> = version
                .split(['.', '-'])
                .map_while(|part| part.parse().ok())
                .collect();
            let program = entry.path().join("bin").join(server);
            program.is_file().then_some((version, program))
        })
        .collect();
    found.sort();
    found.pop().map(|(_, program)| program)
}

/// The project's scripts as the editor has them, so a check can follow
/// `require`s into other scripts (scripting priority 2): every Lua file by
/// project path, with its text (a tab's unsaved text where it has one),
/// and where each script sits in the game tree, as a Rojo-style sourcemap
/// ([`Sourcemap`]) whose `filePaths` are those project paths.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProjectFiles {
    /// Project path (`scripts/doors/open.server.lua`) and text.
    pub files: Vec<(String, String)>,
    pub sourcemap: Sourcemap,
}

impl Hash for ProjectFiles {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.files.hash(state);
        self.sourcemap.hash(state);
    }
}

/// One Instance in the sourcemap luau-lsp reads (Rojo's format): `game`
/// at the top, then services, folders and objects down to each script.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Sourcemap {
    pub name: String,
    pub class_name: String,
    /// The project path of a script's file, if it has one.
    pub file: Option<String>,
    pub children: Vec<Sourcemap>,
}

impl Sourcemap {
    /// As JSON, with `filePaths` under `project/` (where the checker
    /// writes the files).
    fn to_json(&self) -> serde_json::Value {
        let mut node = serde_json::json!({
            "name": self.name,
            "className": self.class_name,
        });
        if let Some(file) = &self.file {
            node["filePaths"] = serde_json::json!([mirror_path(file)]);
        }
        if !self.children.is_empty() {
            node["children"] =
                serde_json::Value::Array(self.children.iter().map(Sourcemap::to_json).collect());
        }
        node
    }
}

/// Where the checker writes a project file, relative to its folder: under
/// `project/`, and a block script's Lua as `<path>.lua` (luau-lsp only
/// reads `.lua` and `.luau` files).
fn mirror_path(path: &str) -> String {
    if path.ends_with(".lua") || path.ends_with(".luau") {
        format!("project/{path}")
    } else {
        format!("project/{path}.lua")
    }
}

/// The definitions the editor's checks use: the engine's, without the
/// `require` declaration, which would hide luau-lsp's own `require`. That
/// one follows `require(game.ReplicatedStorage.Utils)` through the
/// sourcemap to the module's file and its return type.
fn checker_definitions() -> String {
    let full = axcenity_script::lua_definitions();
    let lines: Vec<&str> = full.lines().collect();
    let mut kept: Vec<&str> = Vec::with_capacity(lines.len());
    for line in lines {
        if line.starts_with("declare function require(") {
            // Its doc comment goes with it.
            while kept.last().is_some_and(|last| last.starts_with("---")) {
                kept.pop();
            }
            continue;
        }
        kept.push(line);
    }
    let mut text = kept.join("\n");
    text.push('\n');
    text
}

/// A script to check against the project.
struct Job {
    path: String,
    /// The key the answer is given under ([`check_key`]).
    key: u64,
    project: std::sync::Arc<ProjectFiles>,
}

impl ProjectFiles {
    /// A hash of every file and the tree: the version of the project.
    pub fn stamp(&self) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        Hash::hash(self, &mut hasher);
        hasher.finish()
    }
}

/// The key a check of `path` against the project with `stamp`
/// ([`ProjectFiles::stamp`]) is answered under: it changes when any file
/// or the tree does, so a script is checked again when a module it may
/// require changes.
pub fn check_key(path: &str, stamp: u64) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut hasher);
    stamp.hash(&mut hasher);
    hasher.finish()
}

/// What a check found, for the script at `path`, under `key`.
#[derive(Debug)]
pub struct Checked {
    pub path: String,
    pub key: u64,
    /// The problems, or why luau-lsp couldn't run.
    pub result: Result<Vec<Diagnostic>, String>,
}

/// Checks scripts on a background thread, one at a time, the latest
/// request for each.
pub struct Checker {
    jobs: Sender<Job>,
    done: Receiver<Checked>,
    program: PathBuf,
}

impl Checker {
    /// Starts the thread, with luau-lsp at `program`, in a folder of its
    /// own under the temp folder.
    pub fn start(program: PathBuf) -> io::Result<Checker> {
        // A folder for each checker, so two never share files.
        static CHECKERS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let number = CHECKERS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let folder =
            std::env::temp_dir().join(format!("axcenity-luau-{}-{number}", std::process::id()));
        std::fs::create_dir_all(&folder)?;
        std::fs::write(folder.join("axcenity.d.luau"), checker_definitions())?;
        std::fs::write(folder.join("settings.json"), SETTINGS)?;
        let (jobs, inbox) = mpsc::channel::<Job>();
        let (outbox, done) = mpsc::channel();
        let runner = program.clone();
        std::thread::Builder::new()
            .name("luau-lsp".to_owned())
            .spawn(move || {
                let mut mirror = Mirror::new(folder.clone());
                while let Ok(first) = inbox.recv() {
                    // Only each script's latest request is worth checking.
                    let mut latest: Vec<Job> = vec![first];
                    while let Ok(job) = inbox.try_recv() {
                        latest.retain(|old| old.path != job.path);
                        latest.push(job);
                    }
                    for job in latest {
                        let result = mirror
                            .write(&job.project)
                            .map_err(|err| err.to_string())
                            .and_then(|()| run(&runner, &folder, &mirror_path(&job.path)));
                        let checked = Checked {
                            path: job.path,
                            key: job.key,
                            result,
                        };
                        if outbox.send(checked).is_err() {
                            return;
                        }
                    }
                }
                let _ = std::fs::remove_dir_all(&folder);
            })?;
        Ok(Checker {
            jobs,
            done,
            program,
        })
    }

    pub fn program(&self) -> &Path {
        &self.program
    }

    /// Asks for the script at `path` to be checked as it is in `project`,
    /// answered under `key` ([`check_key`]).
    pub fn check(&self, path: &str, key: u64, project: std::sync::Arc<ProjectFiles>) {
        let _ = self.jobs.send(Job {
            path: path.to_owned(),
            key,
            project,
        });
    }

    /// Asks for one script with `text` to be checked on its own (tests).
    #[cfg(test)]
    pub fn check_text(&self, path: &str, text: &str) -> u64 {
        let project = ProjectFiles {
            files: vec![(path.to_owned(), text.to_owned())],
            sourcemap: Sourcemap::default(),
        };
        let key = check_key(path, project.stamp());
        self.check(path, key, std::sync::Arc::new(project));
        key
    }

    /// The checks finished since the last call.
    pub fn finished(&self) -> Vec<Checked> {
        self.done.try_iter().collect()
    }

    /// Waits up to `limit` for the next check to finish (tests).
    #[cfg(test)]
    pub fn wait(&self, limit: Duration) -> Option<Checked> {
        self.done.recv_timeout(limit).ok()
    }
}

/// The checker's copy of the project's scripts and sourcemap, rewritten
/// only where they changed.
struct Mirror {
    folder: PathBuf,
    /// Each file written, by project path, with its text's hash.
    written: HashMap<String, u64>,
    sourcemap: Option<u64>,
}

impl Mirror {
    fn new(folder: PathBuf) -> Mirror {
        Mirror {
            folder,
            written: HashMap::new(),
            sourcemap: None,
        }
    }

    fn write(&mut self, project: &ProjectFiles) -> io::Result<()> {
        let mut keep = std::collections::HashSet::new();
        for (path, text) in &project.files {
            keep.insert(path.as_str());
            let hash = text_hash(text);
            if self.written.get(path) == Some(&hash) {
                continue;
            }
            // Project paths are relative, with `/`; never write outside.
            if path.split('/').any(|part| part == ".." || part.is_empty())
                || Path::new(path).is_absolute()
            {
                continue;
            }
            let file = self.folder.join(mirror_path(path));
            if let Some(parent) = file.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&file, text)?;
            self.written.insert(path.clone(), hash);
        }
        let gone: Vec<String> = self
            .written
            .keys()
            .filter(|path| !keep.contains(path.as_str()))
            .cloned()
            .collect();
        for path in gone {
            let _ = std::fs::remove_file(self.folder.join(mirror_path(&path)));
            self.written.remove(&path);
        }
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        project.sourcemap.hash(&mut hasher);
        let hash = hasher.finish();
        if self.sourcemap != Some(hash) {
            let json = if project.sourcemap.class_name.is_empty() {
                serde_json::json!({ "name": "Game", "className": "DataModel" })
            } else {
                project.sourcemap.to_json()
            };
            std::fs::write(self.folder.join("sourcemap.json"), json.to_string())?;
            self.sourcemap = Some(hash);
        }
        Ok(())
    }
}

/// Runs luau-lsp on the file at `file` (relative to `folder`).
fn run(program: &Path, folder: &Path, file: &str) -> Result<Vec<Diagnostic>, String> {
    let mut command = Command::new(program);
    command
        .current_dir(folder)
        .args([
            "analyze",
            // Roblox's platform reads the sourcemap, so `require` of a
            // ModuleScript by its place in the game finds its file. Its
            // Roblox API types come only from the definitions given, which
            // are the engine's.
            "--platform",
            "roblox",
            "--sourcemap",
            "sourcemap.json",
            "--definitions=axcenity.d.luau",
            "--settings",
            "settings.json",
            "--formatter=gnu",
            file,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        // No console window flashing up for each check.
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = command.spawn().map_err(|err| err.to_string())?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() > TIME_LIMIT => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "luau-lsp took over {} seconds",
                    TIME_LIMIT.as_secs()
                ));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(5)),
            Err(err) => return Err(err.to_string()),
        }
    }
    let output = child.wait_with_output().map_err(|err| err.to_string())?;
    let mut printed = String::from_utf8_lossy(&output.stdout).into_owned();
    printed.push_str(&String::from_utf8_lossy(&output.stderr));
    let found = parse(&printed);
    // Exit code 1 with nothing read means luau-lsp itself failed (a bad
    // definitions file, an unknown option).
    if found.is_empty() && !output.status.success() {
        let reason = printed
            .lines()
            .find(|line| !line.starts_with("[INFO]") && !line.starts_with("[WARN]"))
            .unwrap_or("no output")
            .to_owned();
        return Err(format!("luau-lsp failed: {reason}"));
    }
    Ok(found)
}

/// Each script's last check, with the key ([`check_key`]) it was for.
#[derive(Default)]
pub struct Results {
    by_path: HashMap<String, (u64, Vec<Diagnostic>)>,
}

impl Results {
    pub fn set(&mut self, path: &str, hash: u64, found: Vec<Diagnostic>) {
        self.by_path.insert(path.to_owned(), (hash, found));
    }

    /// The problems in the script at `path`, if they were found for `hash`
    /// (its key).
    pub fn get(&self, path: &str, hash: u64) -> Option<&[Diagnostic]> {
        self.by_path
            .get(path)
            .filter(|(checked, _)| *checked == hash)
            .map(|(_, found)| found.as_slice())
    }
}

#[cfg(test)]
mod tests;
