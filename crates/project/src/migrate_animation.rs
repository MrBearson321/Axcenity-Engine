//! The format 2 → 3 step of a project upgrade: one clip format and state
//! machines (v3 Phase 7).
//!
//! - Every clip (`*.anim.json`) is rewritten in the one clip shape, and every
//!   animator controller (`*.controller.json`) becomes a state machine
//!   (`*.statemachine.json`) with the same content.
//! - Each animated thing gets its own folder: a clip or controller directly
//!   in `assets/animations` moves into `assets/animations/<name>/` (`Door`
//!   for `Door.anim.json`, `noodle` for `noodle.controller.json`). Files
//!   already in a folder stay in it. A name that's taken gets a number.
//! - Every path to a moved file is rewritten: in state machines (states and
//!   blend trees), and in the world and prefabs (`Animation.Clip`, and
//!   `Animator.Controller`, which becomes `Animator.StateMachine`).
//! - An Animation whose parent has no Animator gains one beside it, so
//!   `animation:Play()` keeps working through it.
//!
//! Scripts aren't changed: a path to an animation file written in Lua is
//! left as it was.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use axcenity_anim::{
    ANIMATIONS_FOLDER, CLIP_EXTENSION, ClipFile, LEGACY_CONTROLLER_EXTENSION,
    STATE_MACHINE_EXTENSION, StateMachineFile,
};
use serde_json::Value as Json;

use crate::WORLD_FILE;

/// Folders at the project root that aren't part of the game.
const SKIP_ROOT_DIRS: &[&str] = &[".axcenity", ".git", "target", "node_modules"];
const PREFABS_FOLDER: &str = "assets/prefabs";
const PREFAB_EXTENSION: &str = ".prefab.json";

/// Runs the step on the project in `folder`.
pub(crate) fn migrate(folder: &Path) -> Result<(), String> {
    let mut clips = Vec::new();
    let mut controllers = Vec::new();
    find_files(folder, folder, &mut clips, &mut controllers)?;

    // Where each file goes, by its old path.
    let mut moves: BTreeMap<String, String> = BTreeMap::new();
    let mut taken: BTreeSet<String> = clips.iter().chain(&controllers).cloned().collect();
    for path in clips.iter().chain(&controllers) {
        let to = destination(path, &taken);
        if &to != path {
            taken.insert(to.clone());
        }
        moves.insert(path.clone(), to);
    }
    let moved = |path: &str| moves.get(path).cloned().unwrap_or_else(|| path.to_owned());

    for path in &clips {
        let text = read(folder, path)?;
        let clip = ClipFile::from_json(&text)
            .map_err(|err| format!("could not upgrade `{path}`: {err}"))?;
        write_moved(folder, path, &moved(path), &clip.to_json())?;
    }
    for path in &controllers {
        let text = read(folder, path)?;
        let mut machine = StateMachineFile::from_json(&text)
            .map_err(|err| format!("could not upgrade `{path}`: {err}"))?;
        for state in &mut machine.states {
            for clip in state.motion.clips_mut() {
                *clip = moved(clip);
            }
        }
        write_moved(folder, path, &moved(path), &machine.to_json())?;
    }

    // The world, then each prefab.
    let world_path = folder.join(WORLD_FILE);
    if world_path.exists() {
        let text = fs::read_to_string(&world_path)
            .map_err(|err| format!("could not read `{WORLD_FILE}`: {err}"))?;
        if let Some(text) = upgrade_instances(&text, &moved, Kind::World)? {
            write(&world_path, &text)?;
        }
    }
    let prefabs = folder.join(PREFABS_FOLDER);
    if let Ok(entries) = fs::read_dir(&prefabs) {
        for entry in entries.flatten() {
            let path = entry.path();
            let is_prefab = path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().ends_with(PREFAB_EXTENSION));
            if !is_prefab {
                continue;
            }
            let text = fs::read_to_string(&path)
                .map_err(|err| format!("could not read `{}`: {err}", path.display()))?;
            if let Some(text) = upgrade_instances(&text, &moved, Kind::Prefab)? {
                write(&path, &text)?;
            }
        }
    }
    Ok(())
}

/// Every clip and controller under `dir`, as paths from `root` with `/`.
fn find_files(
    root: &Path,
    dir: &Path,
    clips: &mut Vec<String>,
    controllers: &mut Vec<String>,
) -> Result<(), String> {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .map_err(|err| format!("could not read `{}`: {err}", dir.display()))?
        .flatten()
        .collect();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            if dir == root && SKIP_ROOT_DIRS.contains(&name.as_str()) {
                continue;
            }
            find_files(root, &entry.path(), clips, controllers)?;
        } else if file_type.is_file() {
            let Ok(relative) = entry.path().strip_prefix(root).map(Path::to_path_buf) else {
                continue;
            };
            let relative = relative.to_string_lossy().replace('\\', "/");
            if name.ends_with(CLIP_EXTENSION) {
                clips.push(relative);
            } else if name.ends_with(LEGACY_CONTROLLER_EXTENSION) {
                controllers.push(relative);
            }
        }
    }
    Ok(())
}

/// Where the file at `path` goes: a state machine's new extension, and a
/// folder of its own if it's directly in `assets/animations`. A path in
/// `taken` (other than its own) gets a number.
fn destination(path: &str, taken: &BTreeSet<String>) -> String {
    let (dir, file) = path.rsplit_once('/').unwrap_or(("", path));
    let (stem, extension) = if let Some(stem) = file.strip_suffix(LEGACY_CONTROLLER_EXTENSION) {
        (stem, STATE_MACHINE_EXTENSION)
    } else {
        (
            file.strip_suffix(CLIP_EXTENSION).unwrap_or(file),
            CLIP_EXTENSION,
        )
    };
    let dir = if dir == ANIMATIONS_FOLDER {
        format!("{ANIMATIONS_FOLDER}/{stem}")
    } else {
        dir.to_owned()
    };
    let at = |stem: &str| {
        if dir.is_empty() {
            format!("{stem}{extension}")
        } else {
            format!("{dir}/{stem}{extension}")
        }
    };
    let first = at(stem);
    if first == path || !taken.contains(&first) {
        return first;
    }
    (2..)
        .map(|number| at(&format!("{stem}{number}")))
        .find(|candidate| !taken.contains(candidate))
        .expect("some number is free")
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    World,
    Prefab,
}

/// The world or prefab `text` with its animation paths moved, Controller
/// renamed StateMachine, and an Animator beside each Animation that has
/// none, written as the engine writes it. `None` if nothing changed.
fn upgrade_instances(
    text: &str,
    moved: &impl Fn(&str) -> String,
    kind: Kind,
) -> Result<Option<String>, String> {
    let Ok(mut json) = serde_json::from_str::<Json>(text) else {
        // A damaged file is left for loading to report.
        return Ok(None);
    };
    if !upgrade_node(&mut json, moved) {
        return Ok(None);
    }
    let raw = serde_json::to_string_pretty(&json).map_err(|err| err.to_string())? + "\n";
    // Written again as the engine writes it, when it reads.
    let canonical = match kind {
        Kind::World => crate::parse_world(&raw)
            .ok()
            .map(|world| crate::write_world(&world)),
        Kind::Prefab => {
            let mut world = axcenity_core::World::new();
            let workspace = world.workspace();
            crate::parse_prefab(&raw, &mut world, workspace)
                .ok()
                .map(|ids| crate::write_prefab(&world, &ids))
        }
    };
    Ok(Some(canonical.unwrap_or(raw)))
}

/// Upgrades every Instance node in `json`. Returns true if it changed.
fn upgrade_node(json: &mut Json, moved: &impl Fn(&str) -> String) -> bool {
    let mut changed = false;
    match json {
        Json::Array(items) => {
            for item in items {
                changed |= upgrade_node(item, moved);
            }
        }
        Json::Object(fields) => {
            let class = fields
                .get("class")
                .and_then(Json::as_str)
                .unwrap_or_default()
                .to_owned();
            if let Some(Json::Object(properties)) = fields.get_mut("properties") {
                if class == "Animator"
                    && let Some(controller) = properties.remove("Controller")
                {
                    let path = controller.as_str().map(moved).unwrap_or_default();
                    properties.insert("StateMachine".to_owned(), Json::from(path));
                    changed = true;
                }
                if class == "Animation"
                    && let Some(Json::String(clip)) = properties.get_mut("Clip")
                {
                    let to = moved(clip);
                    if &to != clip {
                        *clip = to;
                        changed = true;
                    }
                }
            }
            if let Some(Json::Array(children)) = fields.get_mut("children") {
                let class_of = |child: &Json| {
                    child
                        .get("class")
                        .and_then(Json::as_str)
                        .unwrap_or_default()
                        .to_owned()
                };
                let has_animation = children.iter().any(|c| class_of(c) == "Animation");
                let has_animator = children.iter().any(|c| class_of(c) == "Animator");
                if has_animation && !has_animator && class != "Animator" {
                    children.push(serde_json::json!({ "class": "Animator", "name": "Animator" }));
                    changed = true;
                }
            }
            for value in fields.values_mut() {
                changed |= upgrade_node(value, moved);
            }
        }
        _ => {}
    }
    changed
}

fn read(folder: &Path, path: &str) -> Result<String, String> {
    fs::read_to_string(folder.join(path)).map_err(|err| format!("could not read `{path}`: {err}"))
}

/// Writes `text` at `to`, then removes `from` if that's somewhere else.
fn write_moved(folder: &Path, from: &str, to: &str, text: &str) -> Result<(), String> {
    write(&folder.join(to), text)?;
    if from != to {
        fs::remove_file(folder.join(from))
            .map_err(|err| format!("could not remove `{from}` after moving it to `{to}`: {err}"))?;
    }
    Ok(())
}

fn write(path: &Path, text: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("could not create `{}`: {err}", parent.display()))?;
    }
    let mut temporary = path.as_os_str().to_owned();
    temporary.push(".saving");
    fs::write(&temporary, text)
        .map_err(|err| format!("could not write `{}`: {err}", path.display()))?;
    fs::rename(&temporary, path).map_err(|err| {
        let _ = fs::remove_file(&temporary);
        format!("could not replace `{}`: {err}", path.display())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axcenity_core::{ClassName, Value};

    /// A format 2 project: a noodle with an Animator playing a controller
    /// directly in `assets/animations`, its clips in a folder, and a door
    /// with an Animation of a property clip and no Animator.
    fn old_project(name: &str) -> std::path::PathBuf {
        let folder = std::env::temp_dir().join(format!(
            "axcenity-migrate-anim-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&folder);
        let write = |path: &str, text: &str| {
            let path = folder.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        };
        write("axcenity.json", r#"{ "name": "Old", "format_version": 2 }"#);
        write(
            "assets/animations/noodle/Walk.anim.json",
            r#"{ "format_version": 1, "source": "assets/models/noodle.fbx", "take": "Walk",
                 "start": 0, "end": 1, "speed": 2 }"#,
        );
        write(
            "assets/animations/noodle.controller.json",
            r#"{ "format_version": 1, "default_state": "Walk", "states": [
                 { "name": "Walk", "motion": { "clip": "assets/animations/noodle/Walk.anim.json" } },
                 { "name": "Open", "motion": { "clip": "assets/animations/Door.anim.json" } } ] }"#,
        );
        write(
            "assets/animations/Door.anim.json",
            r#"{ "format_version": 1, "kind": "property", "duration": 1.5, "loop": false,
                 "tracks": [ { "id": "p", "target": ".", "property": "Position",
                   "keys": [ { "id": "k", "time": 0, "value": [0, 2, 0] } ] } ] }"#,
        );
        write(
            "world.json",
            r#"{ "format_version": 1, "workspace": { "children": [
                 { "class": "MeshPart", "name": "Noodle", "children": [
                   { "class": "Animator", "name": "Animator",
                     "properties": { "Controller": "assets/animations/noodle.controller.json" } } ] },
                 { "class": "Part", "name": "Door", "children": [
                   { "class": "Animation", "name": "Open",
                     "properties": { "Clip": "assets/animations/Door.anim.json" } } ] } ] } }"#,
        );
        folder
    }

    #[test]
    fn an_older_project_moves_its_files_rewrites_their_paths_and_gains_animators_once() {
        let folder = old_project("upgrade");
        let project = crate::load(&folder).unwrap();
        assert_eq!(project.migrated_from, Some(2));
        assert!(project.problems.is_empty(), "{:?}", project.problems);
        assert!(
            folder
                .join(".axcenity/previous/2/assets/animations/Door.anim.json")
                .exists()
        );

        // Files moved into their own folders, in the one shape.
        assert!(
            !folder
                .join("assets/animations/noodle.controller.json")
                .exists()
        );
        assert!(!folder.join("assets/animations/Door.anim.json").exists());
        let machine = StateMachineFile::from_json(
            &fs::read_to_string(folder.join("assets/animations/noodle/noodle.statemachine.json"))
                .unwrap(),
        )
        .unwrap();
        let clips: Vec<&String> = machine
            .states
            .iter()
            .flat_map(|s| s.motion.clips())
            .collect();
        assert_eq!(
            clips,
            [
                "assets/animations/noodle/Walk.anim.json",
                "assets/animations/Door/Door.anim.json"
            ]
        );
        let walk =
            fs::read_to_string(folder.join("assets/animations/noodle/Walk.anim.json")).unwrap();
        let walk = ClipFile::from_json(&walk).unwrap();
        assert_eq!(walk.format_version, axcenity_anim::files::FORMAT_VERSION);
        assert_eq!((walk.duration, walk.speed()), (0.5, 2.0));
        assert!(
            folder
                .join("assets/animations/Door/Door.anim.json")
                .exists()
        );

        // The world points at the new paths, and the door has an Animator.
        let world = &project.world;
        let noodle = world.find_first_child(world.workspace(), "Noodle").unwrap();
        let animator = world.find_first_child(noodle, "Animator").unwrap();
        assert_eq!(
            world.get(animator).unwrap().property("StateMachine"),
            Some(&Value::String(
                "assets/animations/noodle/noodle.statemachine.json".into()
            ))
        );
        let door = world.find_first_child(world.workspace(), "Door").unwrap();
        let open = world.find_first_child(door, "Open").unwrap();
        assert_eq!(
            world.get(open).unwrap().property("Clip"),
            Some(&Value::String(
                "assets/animations/Door/Door.anim.json".into()
            ))
        );
        let classes: Vec<ClassName> = world
            .get(door)
            .unwrap()
            .children()
            .iter()
            .map(|&child| world.get(child).unwrap().class())
            .collect();
        assert_eq!(classes, [ClassName::Animation, ClassName::Animator]);

        // Opening it again changes nothing.
        let world_text = fs::read_to_string(folder.join(WORLD_FILE)).unwrap();
        let again = crate::load(&folder).unwrap();
        assert_eq!(again.migrated_from, None);
        assert_eq!(
            fs::read_to_string(folder.join(WORLD_FILE)).unwrap(),
            world_text
        );
        let _ = fs::remove_dir_all(&folder);
    }

    #[test]
    fn files_directly_in_animations_get_a_folder_and_controllers_a_new_name() {
        let taken: BTreeSet<String> = [
            "assets/animations/noodle.controller.json",
            "assets/animations/noodle/Walk.anim.json",
            "assets/animations/Door.anim.json",
            "assets/animations/Door/Door.anim.json",
        ]
        .map(str::to_owned)
        .into();
        assert_eq!(
            destination("assets/animations/noodle.controller.json", &taken),
            "assets/animations/noodle/noodle.statemachine.json"
        );
        assert_eq!(
            destination("assets/animations/noodle/Walk.anim.json", &taken),
            "assets/animations/noodle/Walk.anim.json"
        );
        // Door/Door.anim.json is already there, so the moved one is Door2.
        assert_eq!(
            destination("assets/animations/Door.anim.json", &taken),
            "assets/animations/Door/Door2.anim.json"
        );
        assert_eq!(
            destination("assets/other/Hero.controller.json", &taken),
            "assets/other/Hero.statemachine.json"
        );
    }
}
