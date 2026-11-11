//! Scripts inside objects (G2): a Script, LocalScript or ModuleScript
//! anywhere in the tree whose `SourceFile` names the project file holding
//! its code, such as `objects/Door/Open.server.lua`. World, level and
//! prefab files hold the Instance; its code stays in that file, which every
//! copy shares (`docs/project/world.md`).

use std::collections::HashMap;
use std::path::Component;

use super::*;

/// A [`ScriptFile`] for every script inside an object in `world`, in tree
/// order, with its code read from its `SourceFile` (each file once, however
/// many copies use it; block scripts translated). A file that can't be read,
/// or whose ending doesn't match the script's class, is added to `problems`
/// and its scripts are left out: the Instances stay, and do nothing.
pub fn object_scripts(
    folder: &Path,
    world: &World,
    problems: &mut Vec<LoadError>,
) -> Vec<ScriptFile> {
    let mut read: HashMap<&str, Option<String>> = HashMap::new();
    let mut scripts = Vec::new();
    for id in world.descendants_iter(world.root()) {
        let Some(instance) = world.get(id) else {
            continue;
        };
        let Some(path) = instance.source_file() else {
            continue;
        };
        if !source_file_fits(instance.class(), path) {
            problems.push(LoadError::ObjectScript {
                script: world.full_name(id).unwrap_or_default(),
                message: format!(
                    "its SourceFile `{path}` isn't a {} file (it should end in {})",
                    instance.class().as_str(),
                    fitting_endings(instance.class()),
                ),
            });
            continue;
        }
        let source = read
            .entry(path)
            .or_insert_with(|| match read_object_file(folder, path) {
                Ok(source) => Some(source),
                Err(err) => {
                    problems.push(err);
                    None
                }
            });
        if let Some(source) = source {
            scripts.push(ScriptFile {
                instance: id,
                path: path.to_owned(),
                source: source.clone(),
            });
        }
    }
    scripts
}

/// `scripts` (from the `scripts` folder, in [`script_order`], with UI
/// Event Graphs at the end) and `objects` (from [`object_scripts`], in tree
/// order, Instances in `world`) as one list, in the order they start:
///
/// 1. Every ModuleScript, the `scripts` folder's first.
/// 2. Templates: scripts inside objects that aren't where scripts of their
///    kind run (`World::runs_scripts_at`), such as in ServerStorage. They
///    only have their code kept, so a `Clone` made by any script that
///    starts next has it.
/// 3. Scripts, the `scripts` folder's first.
/// 4. LocalScripts, the same way.
/// 5. UI Event Graphs.
pub fn in_start_order(
    world: &World,
    scripts: Vec<ScriptFile>,
    objects: Vec<ScriptFile>,
) -> Vec<ScriptFile> {
    let rank = |script: &ScriptFile, object: bool| -> (u8, bool) {
        if axcenity_blocks_runtime::ui_graph::is_ui_graph(&script.path) {
            return (5, false);
        }
        match script_order(&script.path).0 {
            0 => (0, object),
            _ if object && !world.runs_scripts_at(script.instance) => (1, false),
            1 => (2, object),
            _ => (3, object),
        }
    };
    let mut all: Vec<((u8, bool), ScriptFile)> = scripts
        .into_iter()
        .map(|script| (rank(&script, false), script))
        .chain(
            objects
                .into_iter()
                .map(|script| (rank(&script, true), script)),
        )
        .collect();
    // A stable sort keeps each group's own order.
    all.sort_by_key(|(rank, _)| *rank);
    all.into_iter().map(|(_, script)| script).collect()
}

/// `path` with ` <n>` before its script ending, for Make Unique:
/// `objects/Door/Open.server.lua` with 2 is `objects/Door/Open 2.server.lua`.
pub fn numbered_path(path: &str, n: u32) -> String {
    let name = script_name(path);
    let (folder, file) = path.rsplit_once('/').unwrap_or(("", path));
    let ending = &file[name.len()..];
    let file = format!("{name} {n}{ending}");
    if folder.is_empty() {
        file
    } else {
        format!("{folder}/{file}")
    }
}

/// Whether a script of `class` can take its code from the file at `path`:
/// `.server.lua` (or `.server.blocks.json`) for a Script, `.client.lua` (or
/// `.client.blocks.json`) for a LocalScript, `.module.lua` for a
/// ModuleScript.
pub fn source_file_fits(class: ClassName, path: &str) -> bool {
    let endings: &[&str] = match class {
        ClassName::Script => &[SERVER_SCRIPT_SUFFIX, SERVER_BLOCK_SCRIPT_SUFFIX],
        ClassName::LocalScript => &[CLIENT_SCRIPT_SUFFIX, CLIENT_BLOCK_SCRIPT_SUFFIX],
        ClassName::ModuleScript => &[MODULE_SCRIPT_SUFFIX],
        _ => &[],
    };
    endings.iter().any(|ending| path.ends_with(ending))
}

fn fitting_endings(class: ClassName) -> &'static str {
    match class {
        ClassName::LocalScript => "`.client.lua` or `.client.blocks.json`",
        ClassName::ModuleScript => "`.module.lua`",
        _ => "`.server.lua` or `.server.blocks.json`",
    }
}

/// The code in the file at `path` (a `SourceFile`), which must be inside
/// the project; a block script's is translated. A built game has a block
/// script as the Lua it translates to (`Open.server.lua` for
/// `Open.server.blocks.json`), which is read when the blocks aren't there.
pub fn read_object_file(folder: &Path, path: &str) -> Result<String, LoadError> {
    let inside = Path::new(path)
        .components()
        .all(|part| matches!(part, Component::Normal(_)));
    if !inside {
        return Err(LoadError::SourceFile {
            path: path.to_owned(),
            message: "a SourceFile must be a path inside the project folder".to_owned(),
        });
    }
    if is_block_script(path) && !folder.join(path).exists() {
        let translated = format!("{}.lua", path.trim_end_matches(".blocks.json"));
        if folder.join(&translated).is_file() {
            return read_script(folder, &translated);
        }
    }
    read_script(folder, path)
}

#[cfg(test)]
mod tests;
