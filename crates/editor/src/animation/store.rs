//! The animation files in a project, for the Animation panel: finding
//! them, and making, renaming, copying and deleting clips. A clip's name is
//! its file's, so renaming one moves its file, and the controllers that
//! play it are updated to the new path.

use std::path::Path;

use axcenity_anim::files::{self, ClipFile, ControllerFile, Motion, StateDef};
use axcenity_anim::{ANIMATIONS_FOLDER, CLIP_EXTENSION, CONTROLLER_EXTENSION};
use axcenity_assets::{MODELS_FOLDER, Take};

/// What the project has, as paths from its folder with `/` between folders.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AnimationFiles {
    pub clips: Vec<String>,
    pub controllers: Vec<String>,
    /// FBX files in `assets/models`, which takes come from.
    pub models: Vec<String>,
}

/// Finds the clips and controllers in `assets/animations` and the FBX
/// files in `assets/models`, each sorted. Hidden folders are skipped.
pub fn scan(project: &Path) -> AnimationFiles {
    let mut found = AnimationFiles::default();
    for path in walk(project, ANIMATIONS_FOLDER) {
        let lower = path.to_lowercase();
        if lower.ends_with(CLIP_EXTENSION) {
            found.clips.push(path);
        } else if lower.ends_with(CONTROLLER_EXTENSION) {
            found.controllers.push(path);
        }
    }
    found.models = walk(project, MODELS_FOLDER)
        .into_iter()
        .filter(|path| path.to_lowercase().ends_with(".fbx"))
        .collect();
    found
}

/// Every file under `folder` (in `project`), as paths from the project.
fn walk(project: &Path, folder: &str) -> Vec<String> {
    let mut files = Vec::new();
    let mut stack = vec![(project.join(folder), folder.to_owned())];
    while let Some((dir, relative)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            let path = format!("{relative}/{name}");
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => stack.push((entry.path(), path)),
                Ok(_) => files.push(path),
                Err(_) => {}
            }
        }
    }
    files.sort_by_key(|path| path.to_lowercase());
    files
}

/// The folder a clip is in, from `assets/animations` (`noodle` for
/// `assets/animations/noodle/Walk.anim.json`), or "" for one directly there.
pub fn group_of(path: &str) -> &str {
    let inside = path
        .strip_prefix(ANIMATIONS_FOLDER)
        .and_then(|rest| rest.strip_prefix('/'))
        .unwrap_or(path);
    inside.rsplit_once('/').map_or("", |(folder, _)| folder)
}

pub fn read_clip(project: &Path, path: &str) -> Result<ClipFile, String> {
    let text = std::fs::read_to_string(project.join(path)).map_err(|err| err.to_string())?;
    ClipFile::from_json(&text).map_err(|err| err.0)
}

pub fn save_clip(project: &Path, path: &str, clip: &ClipFile) -> std::io::Result<()> {
    files::write_file(&project.join(path), &clip.to_json())
}

/// Why `name` can't be a clip's name, if it can't.
pub fn check_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("a clip needs a name".to_owned());
    }
    let safe = files::file_safe(name);
    if safe != name {
        return Err("use only letters, digits, spaces, - and _".to_owned());
    }
    Ok(safe)
}

/// The path a clip named `name` gets in `folder` (from the project): the
/// name itself, or with 2, 3… after it if a file has it (ignoring case).
fn free_clip_path(project: &Path, folder: &str, name: &str) -> String {
    let taken = |candidate: &str| {
        let wanted = format!("{candidate}{CLIP_EXTENSION}").to_lowercase();
        std::fs::read_dir(project.join(folder)).is_ok_and(|entries| {
            entries
                .flatten()
                .any(|entry| entry.file_name().to_string_lossy().to_lowercase() == wanted)
        })
    };
    let gap = if name.ends_with(|c: char| c.is_ascii_digit()) {
        " "
    } else {
        ""
    };
    let name = (1u32..)
        .map(|n| match n {
            1 => name.to_owned(),
            n => format!("{name}{gap}{n}"),
        })
        .find(|candidate| !taken(candidate))
        .expect("some number is always free");
    format!("{folder}/{name}{CLIP_EXTENSION}")
}

/// The folder clips of the model file `source` go in: `assets/animations/<its name>`.
pub fn folder_for(source: &str) -> String {
    let file = source.rsplit('/').next().unwrap_or(source);
    let stem = file.rsplit_once('.').map_or(file, |(stem, _)| stem);
    format!("{ANIMATIONS_FOLDER}/{}", files::file_safe(stem))
}

/// The clips made from the model at `mesh_id` (anything after `#` is
/// ignored): the ones in its folder, `assets/animations/<its name>`.
pub fn clips_of_model(files: &AnimationFiles, mesh_id: &str) -> Vec<String> {
    let source = mesh_id.split('#').next().unwrap_or(mesh_id);
    let folder = format!("{}/", folder_for(source));
    files
        .clips
        .iter()
        .filter(|clip| {
            clip.strip_prefix(&folder)
                .is_some_and(|rest| !rest.contains('/'))
        })
        .cloned()
        .collect()
}

/// A controller with a state per clip in `clips`: one named Idle (or else
/// the first) as the default state at the left of the graph, the rest in
/// rows of four beside it.
pub fn controller_for(clips: &[String]) -> ControllerFile {
    let mut states: Vec<StateDef> = clips
        .iter()
        .enumerate()
        .map(|(index, path)| {
            let (column, row) = (index % 4, index / 4);
            StateDef::new(
                files::clip_name(path),
                Motion::Clip(path.clone()),
                [240.0 + column as f32 * 200.0, row as f32 * 110.0],
            )
        })
        .collect();
    let default_state = if states.is_empty() {
        String::new()
    } else {
        let start = states
            .iter()
            .position(|state| state.name.eq_ignore_ascii_case("idle"))
            .unwrap_or(0);
        let mut default = states.remove(start);
        default.position = [0.0, 0.0];
        let name = default.name.clone();
        states.insert(0, default);
        name
    };
    ControllerFile {
        default_state,
        states,
        ..ControllerFile::default()
    }
}

/// Writes a controller named `name` (a number added if taken) with a state
/// per clip in `clips` to `assets/animations`, and returns its path.
pub fn new_controller(project: &Path, name: &str, clips: &[String]) -> std::io::Result<String> {
    let path = free_controller_path(project, name);
    files::write_file(&project.join(&path), &controller_for(clips).to_json())?;
    Ok(path)
}

/// The path a controller named `name` gets in `assets/animations`: the
/// name itself, or with 2, 3… after it if a file has it (ignoring case).
pub fn free_controller_path(project: &Path, name: &str) -> String {
    let existing: Vec<String> = std::fs::read_dir(project.join(ANIMATIONS_FOLDER))
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.file_name().to_string_lossy().to_lowercase())
                .collect()
        })
        .unwrap_or_default();
    let gap = if name.ends_with(|c: char| c.is_ascii_digit()) {
        " "
    } else {
        ""
    };
    let name = (1u32..)
        .map(|n| match n {
            1 => name.to_owned(),
            n => format!("{name}{gap}{n}"),
        })
        .find(|candidate| {
            !existing.contains(&format!("{candidate}{CONTROLLER_EXTENSION}").to_lowercase())
        })
        .expect("some number is always free");
    format!("{ANIMATIONS_FOLDER}/{name}{CONTROLLER_EXTENSION}")
}

/// Makes a clip of the whole of `take`, from the model file `source`, named
/// `name` (a number added if taken), and returns its path.
pub fn new_clip(project: &Path, source: &str, take: &Take, name: &str) -> std::io::Result<String> {
    let path = free_clip_path(project, &folder_for(source), name);
    let clip = ClipFile::new(source, &take.name, 0.0, f64::from(take.duration));
    save_clip(project, &path, &clip)?;
    Ok(path)
}

/// Copies the clip at `path` beside it, as "<name> copy" (or with a number),
/// and returns the copy's path.
pub fn duplicate_clip(project: &Path, path: &str) -> std::io::Result<String> {
    let folder = path.rsplit_once('/').map_or("", |(folder, _)| folder);
    let copy = free_clip_path(project, folder, &format!("{} copy", files::clip_name(path)));
    std::fs::copy(project.join(path), project.join(&copy))?;
    Ok(copy)
}

/// Renames the clip at `path` to `name` (in the same folder), and points
/// every controller that plays it at the new path. Returns the new path.
pub fn rename_clip(project: &Path, path: &str, name: &str) -> Result<String, String> {
    let name = check_name(name)?;
    let folder = path.rsplit_once('/').map_or("", |(folder, _)| folder);
    let new = format!("{folder}/{name}{CLIP_EXTENSION}");
    if new == path {
        return Ok(new);
    }
    let differs_in_case_only = new.to_lowercase() == path.to_lowercase();
    if !differs_in_case_only && project.join(&new).exists() {
        return Err(format!("there's already a clip called {name}"));
    }
    std::fs::rename(project.join(path), project.join(&new)).map_err(|err| err.to_string())?;
    for controller in scan(project).controllers {
        update_controller(project, &controller, |clip| {
            (clip == path).then(|| new.clone())
        })
        .map_err(|err| format!("renamed, but couldn't update `{controller}`: {err}"))?;
    }
    Ok(new)
}

/// Deletes the clip at `path`. Controllers that play it keep the path and
/// report it missing when they run.
pub fn delete_clip(project: &Path, path: &str) -> std::io::Result<()> {
    std::fs::remove_file(project.join(path))
}

/// The controllers whose states play the clip at `path`.
pub fn controllers_using(project: &Path, path: &str) -> Vec<String> {
    scan(project)
        .controllers
        .into_iter()
        .filter(|controller| {
            read_controller(project, controller)
                .is_ok_and(|file| clips_of(&file).iter().any(|clip| clip == path))
        })
        .collect()
}

pub fn read_controller(project: &Path, path: &str) -> Result<ControllerFile, String> {
    let text = std::fs::read_to_string(project.join(path)).map_err(|err| err.to_string())?;
    ControllerFile::from_json(&text).map_err(|err| err.0)
}

/// Every clip path a controller's states play.
fn clips_of(file: &ControllerFile) -> Vec<String> {
    file.states
        .iter()
        .flat_map(|state| state.motion.clips().into_iter().cloned())
        .collect()
}

/// Rewrites the clip paths in the controller at `path` that `change`
/// gives a new path for. The file is written only if one changed.
fn update_controller(
    project: &Path,
    path: &str,
    change: impl Fn(&str) -> Option<String>,
) -> Result<(), String> {
    let Ok(mut file) = read_controller(project, path) else {
        // A damaged controller is left alone; it's reported when it runs.
        return Ok(());
    };
    let mut changed = false;
    for state in &mut file.states {
        for clip in state.motion.clips_mut() {
            if let Some(new) = change(clip) {
                *clip = new;
                changed = true;
            }
        }
    }
    if changed {
        files::write_file(&project.join(path), &file.to_json()).map_err(|err| err.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axcenity_anim::files::StateDef;
    use std::path::PathBuf;

    fn project(name: &str) -> PathBuf {
        let folder =
            std::env::temp_dir().join(format!("axcenity-store-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        folder
    }

    fn take() -> Take {
        Take {
            name: "Moves".into(),
            duration: 2.0,
            frame_rate: 30.0,
            tracks: Vec::new(),
        }
    }

    #[test]
    fn clips_are_made_found_and_grouped_by_folder() {
        let folder = project("scan");
        let walk = new_clip(&folder, "assets/models/hero.fbx", &take(), "Walk").unwrap();
        assert_eq!(walk, "assets/animations/hero/Walk.anim.json");
        let again = new_clip(&folder, "assets/models/hero.fbx", &take(), "walk").unwrap();
        assert_eq!(again, "assets/animations/hero/walk2.anim.json");
        let found = scan(&folder);
        assert_eq!(found.clips.len(), 2);
        assert_eq!(group_of(&found.clips[0]), "hero");
        let clip = read_clip(&folder, &walk).unwrap();
        assert_eq!((clip.take.as_str(), clip.end), ("Moves", 2.0));
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn a_new_controller_gets_a_state_per_clip_starting_at_idle() {
        let folder = project("controller");
        let walk = new_clip(&folder, "assets/models/hero.fbx", &take(), "Walk").unwrap();
        let idle = new_clip(&folder, "assets/models/hero.fbx", &take(), "Idle").unwrap();
        new_clip(&folder, "assets/models/other.fbx", &take(), "Run").unwrap();
        let files = scan(&folder);
        let clips = clips_of_model(&files, "assets/models/hero.fbx#Body");
        assert_eq!(clips, [idle.clone(), walk.clone()]);
        let path = new_controller(&folder, "Hero", &clips).unwrap();
        assert_eq!(path, "assets/animations/Hero.controller.json");
        let controller = read_controller(&folder, &path).unwrap();
        assert_eq!(controller.default_state, "Idle");
        assert_eq!(controller.states[0].position, [0.0, 0.0]);
        assert_eq!(
            new_controller(&folder, "hero", &[]).unwrap(),
            "assets/animations/hero2.controller.json"
        );
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn renaming_a_clip_updates_the_controllers_that_play_it() {
        let folder = project("rename");
        let walk = new_clip(&folder, "assets/models/hero.fbx", &take(), "Walk").unwrap();
        let controller = ControllerFile {
            default_state: "Walk".into(),
            states: vec![StateDef::new(
                "Walk",
                Motion::Clip(walk.clone()),
                [0.0, 0.0],
            )],
            ..ControllerFile::default()
        };
        let controller_path = "assets/animations/hero.controller.json";
        files::write_file(&folder.join(controller_path), &controller.to_json()).unwrap();
        assert_eq!(controllers_using(&folder, &walk), [controller_path]);

        let stroll = rename_clip(&folder, &walk, "Stroll").unwrap();
        assert_eq!(stroll, "assets/animations/hero/Stroll.anim.json");
        assert!(!folder.join(&walk).exists());
        let updated = read_controller(&folder, controller_path).unwrap();
        assert_eq!(updated.states[0].motion, Motion::Clip(stroll.clone()));
        assert!(rename_clip(&folder, &stroll, "Bad/Name").is_err());

        let copy = duplicate_clip(&folder, &stroll).unwrap();
        assert_eq!(copy, "assets/animations/hero/Stroll copy.anim.json");
        delete_clip(&folder, &copy).unwrap();
        assert_eq!(scan(&folder).clips, [stroll]);
        std::fs::remove_dir_all(&folder).unwrap();
    }
}
