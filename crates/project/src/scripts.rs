use super::*;

/// Reads every script and gives each one its Script, LocalScript or
/// ModuleScript (see
/// [`add_script_instance`]). A file that can't be read is left out, with the
/// reason added to `problems`.
pub(super) fn load_scripts(
    folder: &Path,
    world: &mut World,
    problems: &mut Vec<LoadError>,
    progress: &mut dyn FnMut(LoadStep),
) -> (Vec<ScriptFile>, Vec<BlockScriptFile>) {
    let mut scripts = Vec::new();
    let mut block_scripts = Vec::new();
    let paths = find_scripts(folder, problems);
    let total = paths.len();
    for (done, path) in paths.into_iter().enumerate() {
        progress(LoadStep::Script {
            path: &path,
            done,
            total,
        });
        let loaded = if is_block_script(&path) {
            read_block_script(folder, &path).and_then(|script| {
                translate(&script)
                    .map(|translation| (translation.source, Some(script)))
                    .map_err(|err| LoadError::BlockScript {
                        path: folder.join(&path),
                        message: err.to_string(),
                    })
            })
        } else {
            read_script(folder, &path).map(|source| (source, None))
        };
        match loaded {
            Ok((source, block_script)) => {
                let instance = add_script_instance(world, &path);
                scripts.push(ScriptFile {
                    instance,
                    path: path.clone(),
                    source,
                });
                if let Some(script) = block_script {
                    block_scripts.push(BlockScriptFile {
                        instance,
                        path,
                        script,
                    });
                }
            }
            Err(err) => problems.push(err),
        }
    }
    (scripts, block_scripts)
}

/// The path of every `*.server.lua`, `*.client.lua` and `*.module.lua` file in the project
/// in `folder`, as [`ScriptFile::path`]s, in [`script_order`]. Folders that
/// can't be read are skipped and added to `problems`.
pub fn find_scripts(folder: &Path, problems: &mut Vec<LoadError>) -> Vec<String> {
    let mut found = Vec::new();
    find_script_files(
        &folder.join(SCRIPTS_FOLDER),
        SCRIPTS_FOLDER,
        &mut found,
        problems,
    );
    // Folder listings come back in a different order on each OS; sorting makes
    // scripts start in the same order everywhere.
    found.sort_by(|a, b| script_order(a).cmp(&script_order(b)));
    found
}

/// What scripts are sorted by: modules first (they only get ready to be
/// required, so they must be there before any script runs), then server
/// scripts before client scripts (a server is running before any player
/// joins), then by path. Sort with
/// `scripts.sort_by(|a, b| script_order(a).cmp(&script_order(b)))`.
pub fn script_order(path: &str) -> (u8, &str) {
    let rank = match script_class(path) {
        ClassName::ModuleScript => 0,
        ClassName::Script => 1,
        _ => 2,
    };
    (rank, path)
}

/// Adds the path of every script file in `dir` and its subfolders to
/// `found`. `shown` is how `dir` itself is shown.
pub(super) fn find_script_files(
    dir: &Path,
    shown: &str,
    found: &mut Vec<String>,
    problems: &mut Vec<LoadError>,
) {
    let mut problem = |source| {
        problems.push(LoadError::Read {
            path: dir.to_owned(),
            source,
        })
    };
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        // A project without a `scripts` folder simply has no scripts.
        Err(err) if err.kind() == ErrorKind::NotFound => return,
        Err(err) => return problem(err),
    };

    let mut folders = Vec::new();
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                problem(err);
                continue;
            }
        };
        let disk_path = entry.path();
        let file_name = entry.file_name().to_string_lossy().into_owned();
        let shown = format!("{shown}/{file_name}");
        if disk_path.is_dir() {
            // Visited after the loop, which still borrows `problems` through `problem`.
            folders.push((disk_path, shown));
        } else if [
            SERVER_SCRIPT_SUFFIX,
            CLIENT_SCRIPT_SUFFIX,
            MODULE_SCRIPT_SUFFIX,
            SERVER_BLOCK_SCRIPT_SUFFIX,
            CLIENT_BLOCK_SCRIPT_SUFFIX,
        ]
        .iter()
        .any(|suffix| file_name.ends_with(suffix))
        {
            found.push(shown);
        }
    }
    for (disk_path, shown) in folders {
        find_script_files(&disk_path, &shown, found, problems);
    }
}

/// The class of the script in the file at `path`: LocalScript for a
/// `.client.lua` file (or a UI Event Graph), ModuleScript for a `.module.lua` file, otherwise Script.
pub fn script_class(path: &str) -> ClassName {
    if path.ends_with(CLIENT_SCRIPT_SUFFIX)
        || path.ends_with(CLIENT_BLOCK_SCRIPT_SUFFIX)
        || axcenity_blocks_runtime::ui_graph::is_ui_graph(path)
    {
        ClassName::LocalScript
    } else if path.ends_with(MODULE_SCRIPT_SUFFIX) {
        ClassName::ModuleScript
    } else {
        ClassName::Script
    }
}

/// The file ending for scripts of `class`: `.client.lua` for a LocalScript,
/// `.module.lua` for a ModuleScript, otherwise `.server.lua`.
pub fn script_suffix(class: ClassName) -> &'static str {
    match class {
        ClassName::LocalScript => CLIENT_SCRIPT_SUFFIX,
        ClassName::ModuleScript => MODULE_SCRIPT_SUFFIX,
        _ => SERVER_SCRIPT_SUFFIX,
    }
}

/// The name of the script for the file at `path`: its file name without
/// `.server.lua`, `.client.lua` or `.module.lua`, so
/// `scripts/doors/open.server.lua` is `open`.
pub fn script_name(path: &str) -> &str {
    let file_name = path.rsplit('/').next().unwrap_or(path);
    file_name
        .strip_suffix(SERVER_BLOCK_SCRIPT_SUFFIX)
        .or_else(|| file_name.strip_suffix(CLIENT_BLOCK_SCRIPT_SUFFIX))
        .or_else(|| file_name.strip_suffix(SERVER_SCRIPT_SUFFIX))
        .or_else(|| file_name.strip_suffix(CLIENT_SCRIPT_SUFFIX))
        .or_else(|| file_name.strip_suffix(MODULE_SCRIPT_SUFFIX))
        .unwrap_or(file_name)
}

pub fn is_block_script(path: &str) -> bool {
    path.ends_with(SERVER_BLOCK_SCRIPT_SUFFIX) || path.ends_with(CLIENT_BLOCK_SCRIPT_SUFFIX)
}

/// The path of a new script of `class` (Script, LocalScript or ModuleScript) called
/// `name`, directly in the `scripts` folder.
pub fn script_path(name: &str, class: ClassName) -> String {
    format!("{SCRIPTS_FOLDER}/{name}{}", script_suffix(class))
}

/// The service the scripts of `class` go in: `ServerScriptService` for
/// Scripts, `StarterPlayer.StarterPlayerScripts` for LocalScripts, and
/// `ReplicatedStorage` for ModuleScripts, where both sides can require them.
pub fn script_service(world: &World, class: ClassName) -> InstanceId {
    match class {
        ClassName::LocalScript => world.starter_player_scripts(),
        ClassName::ModuleScript => world.replicated_storage(),
        _ => world.server_script_service(),
    }
}

/// Makes the Script (or, for a `.client.lua` file, the LocalScript, and for
/// a `.module.lua` file, the ModuleScript) for the
/// file at `path`, named after it, in its service (see [`script_service`]).
/// Subfolders only organize the files: every script goes directly into its
/// service.
pub fn add_script_instance(world: &mut World, path: &str) -> InstanceId {
    let class = script_class(path);
    let instance = world.create(class).expect("scripts are not services");
    world
        .set_name(instance, script_name(path))
        .expect("just created");
    world
        .set_parent(instance, Some(script_service(world, class)))
        .expect("both exist");
    instance
}

/// Reads the script file at `path` (a [`ScriptFile::path`]) in the project in
/// `folder` again, for the newest version of it.
pub fn read_script(folder: &Path, path: &str) -> Result<String, LoadError> {
    if is_block_script(path) {
        let script = read_block_script(folder, path)?;
        return translate(&script)
            .map(|translation| translation.source)
            .map_err(|err| LoadError::BlockScript {
                path: folder.join(path),
                message: err.to_string(),
            });
    }
    read(&folder.join(path))
}

pub fn read_block_script(folder: &Path, path: &str) -> Result<BlockScript, LoadError> {
    let full_path = folder.join(path);
    let text = read(&full_path)?;
    let script = serde_json::from_str(&text).map_err(|err| LoadError::BlockScript {
        path: full_path,
        message: err.to_string(),
    })?;
    reserve_block_ids(&script);
    Ok(script)
}

pub fn write_block_script(
    folder: &Path,
    path: &str,
    script: &BlockScript,
) -> Result<(), BlockSaveError> {
    let source = serde_json::to_string_pretty(script).map_err(BlockSaveError::Serialize)?;
    write_script(folder, path, &source).map_err(BlockSaveError::Write)
}

pub fn block_script_path(name: &str, class: ClassName) -> String {
    let suffix = if class == ClassName::LocalScript {
        CLIENT_BLOCK_SCRIPT_SUFFIX
    } else {
        SERVER_BLOCK_SCRIPT_SUFFIX
    };
    format!("{SCRIPTS_FOLDER}/{name}{suffix}")
}

pub fn create_block_script(
    folder: &Path,
    name: &str,
    class: ClassName,
    script: &BlockScript,
) -> Result<String, BlockSaveError> {
    let path = block_script_path(name, class);
    let source = serde_json::to_string_pretty(script).map_err(BlockSaveError::Serialize)?;
    let disk_path = folder.join(&path);
    fs::create_dir_all(folder.join(SCRIPTS_FOLDER)).map_err(|source| {
        BlockSaveError::Write(SaveError {
            path: disk_path.clone(),
            source,
        })
    })?;
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&disk_path)
        .map_err(|source| {
            BlockSaveError::Write(SaveError {
                path: disk_path.clone(),
                source,
            })
        })?;
    file.write_all(source.as_bytes()).map_err(|source| {
        BlockSaveError::Write(SaveError {
            path: disk_path,
            source,
        })
    })?;
    Ok(path)
}

/// Whether `name` can be a script's name, or why not, in words for the
/// Output panel. Script names become file names, so the rules are those of
/// file names on Windows, the strictest OS: then a project made on any OS
/// opens on every OS.
pub fn check_script_name(name: &str) -> Result<(), &'static str> {
    const RESERVED: [&str; 4] = ["CON", "PRN", "AUX", "NUL"];
    if name.trim().is_empty() {
        return Err("a script needs a name");
    }
    if name.chars().count() > 100 {
        return Err("script names can be at most 100 characters long");
    }
    if let Some(bad) = name
        .chars()
        .find(|c| r#"/\:*?"<>|"#.contains(*c) || c.is_control())
    {
        return Err(if bad.is_control() {
            "script names can't contain control characters"
        } else {
            r#"script names can't contain any of / \ : * ? " < > |"#
        });
    }
    if name.ends_with('.') || name.ends_with(' ') || name.starts_with(' ') {
        return Err("script names can't start with a space or end with a dot or a space");
    }
    // Windows reserves device names such as `CON` and `COM1`, even with an
    // extension after them.
    let upper = name.split('.').next().unwrap_or(name).to_ascii_uppercase();
    let numbered_device = (upper.starts_with("COM") || upper.starts_with("LPT"))
        && upper.len() == 4
        && upper.ends_with(|c: char| c.is_ascii_digit() && c != '0');
    if RESERVED.contains(&upper.as_str()) || numbered_device {
        return Err("that name is reserved by Windows");
    }
    Ok(())
}

/// Makes a new script file called `name` (see [`check_script_name`]) for a
/// script of `class` (Script or LocalScript) with `source` in it, directly
/// in the `scripts` folder, and returns its path. Fails (see
/// [`SaveError::already_exists`]) if a file with that name is already there,
/// never replacing it. On Windows and macOS that includes a name that only
/// differs in capitals.
pub fn create_script(
    folder: &Path,
    name: &str,
    class: ClassName,
    source: &str,
) -> Result<String, SaveError> {
    let path = script_path(name, class);
    let disk_path = folder.join(&path);
    let error = SaveError::new(&disk_path);
    fs::create_dir_all(folder.join(SCRIPTS_FOLDER)).map_err(SaveError::new(&disk_path))?;
    // `create_new` checks and creates in one step, so nothing else can make
    // the file in between.
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&disk_path)
        .map_err(SaveError::new(&disk_path))?;
    file.write_all(source.as_bytes()).map_err(error)?;
    Ok(path)
}

/// Writes `source` to the script file at `path` (a [`ScriptFile::path`]),
/// making it if it isn't there. Like [`save_world`], a half-written file
/// never replaces the old one.
pub fn write_script(folder: &Path, path: &str, source: &str) -> Result<(), SaveError> {
    let disk_path = folder.join(path);
    if let Some(parent) = disk_path.parent() {
        fs::create_dir_all(parent).map_err(SaveError::new(&disk_path))?;
    }
    write_through_temporary(&disk_path, source)
}

/// Renames the script file at `path` to `new_name` (see
/// [`check_script_name`]), in the same folder and with the same ending
/// (`.server.lua` or `.client.lua`), and returns its new path. Fails rather
/// than replace another file.
pub fn rename_script(folder: &Path, path: &str, new_name: &str) -> Result<String, SaveError> {
    rename_script_file(folder, path, new_name, script_suffix(script_class(path)))
}

pub fn rename_block_script(folder: &Path, path: &str, new_name: &str) -> Result<String, SaveError> {
    let suffix = if path.ends_with(CLIENT_BLOCK_SCRIPT_SUFFIX) {
        CLIENT_BLOCK_SCRIPT_SUFFIX
    } else {
        SERVER_BLOCK_SCRIPT_SUFFIX
    };
    rename_script_file(folder, path, new_name, suffix)
}

pub(super) fn rename_script_file(
    folder: &Path,
    path: &str,
    new_name: &str,
    suffix: &str,
) -> Result<String, SaveError> {
    let new_file_name = format!("{new_name}{suffix}");
    let new_path = match path.rsplit_once('/') {
        Some((dir, _)) => format!("{dir}/{new_file_name}"),
        None => new_file_name.clone(),
    };
    if new_path == path {
        return Ok(new_path);
    }
    let (from, to) = (folder.join(path), folder.join(&new_path));
    // On Windows and macOS, `pad` and `Pad` are the same file, so `to`
    // "exists" when only capitals change. Listing the folder tells a second
    // file with exactly that name apart from the file itself.
    let only_capitals = new_path.to_lowercase() == path.to_lowercase();
    let listed = to
        .parent()
        .and_then(|dir| fs::read_dir(dir).ok())
        .is_some_and(|entries| {
            entries
                .flatten()
                .any(|entry| entry.file_name().to_string_lossy() == new_file_name)
        });
    if listed || (to.exists() && !only_capitals) {
        return Err(SaveError {
            path: to,
            source: std::io::Error::new(
                ErrorKind::AlreadyExists,
                "a script with that name is already there",
            ),
        });
    }
    fs::rename(&from, &to).map_err(SaveError::new(&to))?;
    Ok(new_path)
}

/// The path a Script or LocalScript at `path` gets when it switches between
/// Code and Blocks: `X.server.lua` ↔ `X.server.blocks.json`, and the same
/// for `.client`. `None` for ModuleScripts, which are only code.
pub fn switched_script_path(path: &str) -> Option<String> {
    for (code, blocks) in [
        (SERVER_SCRIPT_SUFFIX, SERVER_BLOCK_SCRIPT_SUFFIX),
        (CLIENT_SCRIPT_SUFFIX, CLIENT_BLOCK_SCRIPT_SUFFIX),
    ] {
        if let Some(stem) = path.strip_suffix(blocks) {
            return Some(format!("{stem}{code}"));
        }
        if let Some(stem) = path.strip_suffix(code) {
            return Some(format!("{stem}{blocks}"));
        }
    }
    None
}

/// Switches the block script at `path` to Code: writes `source` (the Lua
/// its blocks translate to) next to it as a `.lua` file, then deletes the
/// block file. Returns the new path. Fails rather than replace a file.
pub fn switch_to_code(folder: &Path, path: &str, source: &str) -> Result<String, SaveError> {
    switch_script_file(folder, path, source)
}

/// Switches the Lua script at `path` to Blocks: writes `script` next to it
/// as a `.blocks.json` file, then deletes the `.lua` file. Returns the new
/// path. Fails rather than replace a file.
pub fn switch_to_blocks(
    folder: &Path,
    path: &str,
    script: &BlockScript,
) -> Result<String, BlockSaveError> {
    let text = serde_json::to_string_pretty(script).map_err(BlockSaveError::Serialize)?;
    switch_script_file(folder, path, &text).map_err(BlockSaveError::Write)
}

pub(super) fn switch_script_file(
    folder: &Path,
    path: &str,
    text: &str,
) -> Result<String, SaveError> {
    let new_path = switched_script_path(path).ok_or_else(|| SaveError {
        path: folder.join(path),
        source: std::io::Error::new(
            ErrorKind::InvalidInput,
            "only Scripts and LocalScripts can switch between Code and Blocks",
        ),
    })?;
    let disk_path = folder.join(&new_path);
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&disk_path)
        .map_err(SaveError::new(&disk_path))?;
    file.write_all(text.as_bytes())
        .map_err(SaveError::new(&disk_path))?;
    drop(file);
    match fs::remove_file(folder.join(path)) {
        Err(err) if err.kind() != ErrorKind::NotFound => {
            // Put things back as they were, so there aren't two scripts.
            let _ = fs::remove_file(&disk_path);
            Err(SaveError {
                path: folder.join(path),
                source: err,
            })
        }
        _ => Ok(new_path),
    }
}

/// Deletes the script file at `path` (a [`ScriptFile::path`]).
pub fn delete_script(folder: &Path, path: &str) -> Result<(), SaveError> {
    let disk_path = folder.join(path);
    fs::remove_file(&disk_path).map_err(SaveError::new(&disk_path))
}
