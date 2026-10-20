//! The animation files in a project, for the Animation panel: finding
//! them, and making, renaming, copying and deleting clips. A clip's name is
//! its file's, so renaming one moves its file, and the controllers that
//! play it are updated to the new path.

use std::path::Path;

use axcenity_anim::files::{self, AnimClipFile, ClipFile, ControllerFile, Motion, StateDef};
use axcenity_anim::{ANIMATIONS_FOLDER, CLIP_EXTENSION, CONTROLLER_EXTENSION, PropertyClipFile};
use axcenity_assets::{MODELS_FOLDER, Take};

/// One project file before and after an asset edit. `None` means the file
/// is not there.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileDiff {
    pub path: String,
    pub before: Option<String>,
    pub after: Option<String>,
}

/// A clip or controller file an edit produced, and every file that edit
/// changed (a rename also rewrites controllers that play the clip).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Written {
    pub path: String,
    pub files: Vec<FileDiff>,
}

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
    match read_any_clip(project, path)? {
        AnimClipFile::Take(clip) => Ok(clip),
        AnimClipFile::Property(_) => Err(
            "it is a property clip; open it from the Animation Library as a property clip".into(),
        ),
    }
}

/// Reads either a take clip or a property clip.
pub fn read_any_clip(project: &Path, path: &str) -> Result<AnimClipFile, String> {
    let text = std::fs::read_to_string(project.join(path)).map_err(|err| err.to_string())?;
    AnimClipFile::from_json(&text).map_err(|err| err.0)
}

pub fn save_clip(project: &Path, path: &str, clip: &ClipFile) -> std::io::Result<()> {
    files::write_file(&project.join(path), &clip.to_json())
}

pub fn save_property_clip(
    project: &Path,
    path: &str,
    clip: &PropertyClipFile,
) -> std::io::Result<()> {
    files::write_file(&project.join(path), &clip.to_json())
}

/// Creates a new property clip file and returns its path and the file change.
pub fn create_property_clip(
    project: &Path,
    name: &str,
    clip: &PropertyClipFile,
) -> Result<Written, String> {
    let name = check_name(name)?;
    let folder = ANIMATIONS_FOLDER.to_owned();
    std::fs::create_dir_all(project.join(&folder)).map_err(|err| err.to_string())?;
    let path = free_clip_path(project, &folder, &name);
    save_property_clip(project, &path, clip).map_err(|err| err.to_string())?;
    Ok(written_new(&path, read_text(project, &path)))
}

/// Why `name` can't be a clip or controller name, if it can't.
pub fn check_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("it needs a name".to_owned());
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
    free_file_path(project, folder, name, CLIP_EXTENSION)
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
pub fn new_controller(project: &Path, name: &str, clips: &[String]) -> std::io::Result<Written> {
    let path = free_controller_path(project, name);
    files::write_file(&project.join(&path), &controller_for(clips).to_json())?;
    Ok(written_new(&path, read_text(project, &path)))
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
pub fn new_clip(project: &Path, source: &str, take: &Take, name: &str) -> std::io::Result<Written> {
    let path = free_clip_path(project, &folder_for(source), name);
    let clip = ClipFile::new(source, &take.name, 0.0, f64::from(take.duration));
    save_clip(project, &path, &clip)?;
    Ok(written_new(&path, read_text(project, &path)))
}

/// Copies the clip at `path` beside it, as "<name> copy" (or with a number),
/// and returns the copy's path.
pub fn duplicate_clip(project: &Path, path: &str) -> std::io::Result<Written> {
    let folder = path.rsplit_once('/').map_or("", |(folder, _)| folder);
    let copy = free_clip_path(project, folder, &format!("{} copy", files::clip_name(path)));
    let text = std::fs::read_to_string(project.join(path))?;
    std::fs::copy(project.join(path), project.join(&copy))?;
    Ok(written_new(&copy, Some(text)))
}

/// Renames the clip at `path` to `name` (in the same folder), and points
/// every controller that plays it at the new path. Returns the new path.
pub fn rename_clip(project: &Path, path: &str, name: &str) -> Result<Written, String> {
    let name = check_name(name)?;
    let folder = path.rsplit_once('/').map_or("", |(folder, _)| folder);
    let new = format!("{folder}/{name}{CLIP_EXTENSION}");
    if new == path {
        return Ok(Written {
            path: new,
            files: Vec::new(),
        });
    }
    let controllers = controller_texts(project);
    let moved = move_file(project, path, &new, "clip", &name)?;
    let mut files = moved.files;
    for (controller, before) in controllers {
        update_controller(project, &controller, |clip| {
            (clip == path).then(|| new.clone())
        })
        .map_err(|err| format!("renamed, but couldn't update `{controller}`: {err}"))?;
        let after = read_text(project, &controller);
        if after.as_ref() != Some(&before) {
            files.push(FileDiff {
                path: controller,
                before: Some(before),
                after,
            });
        }
    }
    Ok(Written { path: new, files })
}

/// Copies the controller at `path` beside it, as "<name> copy".
pub fn duplicate_controller(project: &Path, path: &str) -> std::io::Result<Written> {
    let folder = path
        .rsplit_once('/')
        .map_or(ANIMATIONS_FOLDER, |(folder, _)| folder);
    let copy = free_file_path(
        project,
        folder,
        &format!("{} copy", controller_name(path)),
        CONTROLLER_EXTENSION,
    );
    let text = std::fs::read_to_string(project.join(path))?;
    if let Some(parent) = project.join(&copy).parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::copy(project.join(path), project.join(&copy))?;
    Ok(written_new(&copy, Some(text)))
}

/// Renames the controller at `path` to `name`, in the same folder.
pub fn rename_controller(project: &Path, path: &str, name: &str) -> Result<Written, String> {
    let name = check_name(name)?;
    let folder = path
        .rsplit_once('/')
        .map_or(ANIMATIONS_FOLDER, |(folder, _)| folder);
    let new = format!("{folder}/{name}{CONTROLLER_EXTENSION}");
    if new == path {
        return Ok(Written {
            path: new,
            files: Vec::new(),
        });
    }
    move_file(project, path, &new, "controller", &name)
}

/// A controller file's name, without `.controller.json`.
pub fn controller_name(path: &str) -> &str {
    let file = path.rsplit('/').next().unwrap_or(path);
    file.strip_suffix(CONTROLLER_EXTENSION).unwrap_or(file)
}

/// Deletes the clip or controller at `path`. Controllers that play a deleted
/// clip keep the path and report it missing when they run.
pub fn delete_asset(project: &Path, path: &str) -> std::io::Result<Vec<FileDiff>> {
    let before = std::fs::read_to_string(project.join(path))?;
    std::fs::remove_file(project.join(path))?;
    Ok(vec![FileDiff {
        path: path.to_owned(),
        before: Some(before),
        after: None,
    }])
}

/// Puts `files` on disk. `after: None` removes the file; a rename that only
/// changes case is a rename, so a case-insensitive disk doesn't delete the
/// file it just wrote.
pub fn apply_file_diffs(project: &Path, files: &[FileDiff]) -> std::io::Result<()> {
    let creates: Vec<&FileDiff> = files
        .iter()
        .filter(|file| file.before.is_none() && file.after.is_some())
        .collect();
    let mut renamed_to = Vec::new();
    for removed in files.iter().filter(|file| file.after.is_none()) {
        let Some(created) = creates.iter().find(|created| {
            created.path != removed.path && created.path.eq_ignore_ascii_case(&removed.path)
        }) else {
            continue;
        };
        let from = project.join(&removed.path);
        let to = project.join(&created.path);
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::rename(&from, &to)?;
        if created.after.as_ref() != removed.before.as_ref()
            && let Some(text) = &created.after
        {
            files::write_file(&to, text)?;
        }
        renamed_to.push(created.path.clone());
    }
    for file in files {
        if file.after.is_some() {
            continue;
        }
        if creates.iter().any(|created| {
            created.path != file.path && created.path.eq_ignore_ascii_case(&file.path)
        }) {
            continue;
        }
        let path = project.join(&file.path);
        if path.exists() {
            std::fs::remove_file(path)?;
        }
    }
    for file in files {
        if renamed_to.iter().any(|path| path == &file.path) {
            continue;
        }
        if let Some(text) = &file.after {
            files::write_file(&project.join(&file.path), text)?;
        }
    }
    Ok(())
}

fn read_text(project: &Path, path: &str) -> Option<String> {
    std::fs::read_to_string(project.join(path)).ok()
}

fn written_new(path: &str, text: Option<String>) -> Written {
    Written {
        path: path.to_owned(),
        files: vec![FileDiff {
            path: path.to_owned(),
            before: None,
            after: text,
        }],
    }
}

/// Moves `path` to `new`. On a case-only rename the destination is recorded
/// as absent beforehand, because reading it would see the same file.
fn move_file(
    project: &Path,
    path: &str,
    new: &str,
    kind: &str,
    name: &str,
) -> Result<Written, String> {
    let differs_in_case_only = new.eq_ignore_ascii_case(path);
    if !differs_in_case_only && project.join(new).exists() {
        return Err(format!("there's already a {kind} called {name}"));
    }
    let old_text = read_text(project, path).ok_or_else(|| format!("the {kind} isn't there"))?;
    std::fs::rename(project.join(path), project.join(new)).map_err(|err| err.to_string())?;
    Ok(Written {
        path: new.to_owned(),
        files: vec![
            FileDiff {
                path: path.to_owned(),
                before: Some(old_text.clone()),
                after: None,
            },
            FileDiff {
                path: new.to_owned(),
                before: None,
                after: Some(old_text),
            },
        ],
    })
}

fn controller_texts(project: &Path) -> Vec<(String, String)> {
    scan(project)
        .controllers
        .into_iter()
        .filter_map(|path| read_text(project, &path).map(|text| (path, text)))
        .collect()
}

/// The path a file named `name` plus `extension` gets in `folder`.
fn free_file_path(project: &Path, folder: &str, name: &str, extension: &str) -> String {
    let taken = |candidate: &str| {
        let wanted = format!("{candidate}{extension}").to_lowercase();
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
    format!("{folder}/{name}{extension}")
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
        let walk = new_clip(&folder, "assets/models/hero.fbx", &take(), "Walk")
            .unwrap()
            .path;
        assert_eq!(walk, "assets/animations/hero/Walk.anim.json");
        let again = new_clip(&folder, "assets/models/hero.fbx", &take(), "walk")
            .unwrap()
            .path;
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
        let walk = new_clip(&folder, "assets/models/hero.fbx", &take(), "Walk")
            .unwrap()
            .path;
        let idle = new_clip(&folder, "assets/models/hero.fbx", &take(), "Idle")
            .unwrap()
            .path;
        new_clip(&folder, "assets/models/other.fbx", &take(), "Run").unwrap();
        let files = scan(&folder);
        let clips = clips_of_model(&files, "assets/models/hero.fbx#Body");
        assert_eq!(clips, [idle.clone(), walk.clone()]);
        let path = new_controller(&folder, "Hero", &clips).unwrap().path;
        assert_eq!(path, "assets/animations/Hero.controller.json");
        let controller = read_controller(&folder, &path).unwrap();
        assert_eq!(controller.default_state, "Idle");
        assert_eq!(controller.states[0].position, [0.0, 0.0]);
        assert_eq!(
            new_controller(&folder, "hero", &[]).unwrap().path,
            "assets/animations/hero2.controller.json"
        );
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn renaming_a_clip_updates_the_controllers_that_play_it() {
        let folder = project("rename");
        let walk = new_clip(&folder, "assets/models/hero.fbx", &take(), "Walk")
            .unwrap()
            .path;
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
        assert_eq!(stroll.path, "assets/animations/hero/Stroll.anim.json");
        assert!(
            stroll
                .files
                .iter()
                .any(|file| file.path == walk && file.after.is_none())
        );
        assert!(
            stroll
                .files
                .iter()
                .any(|file| file.path == controller_path && file.before != file.after)
        );
        assert!(!folder.join(&walk).exists());
        let updated = read_controller(&folder, controller_path).unwrap();
        assert_eq!(updated.states[0].motion, Motion::Clip(stroll.path.clone()));
        assert!(rename_clip(&folder, &stroll.path, "Bad/Name").is_err());

        let copy = duplicate_clip(&folder, &stroll.path).unwrap();
        assert_eq!(copy.path, "assets/animations/hero/Stroll copy.anim.json");
        delete_asset(&folder, &copy.path).unwrap();
        assert_eq!(scan(&folder).clips, [stroll.path]);

        let hero = new_controller(&folder, "Hero", &[]).unwrap().path;
        let text = std::fs::read_to_string(folder.join(&hero)).unwrap();
        let renamed = rename_controller(&folder, &hero, "Villain").unwrap();
        assert_eq!(renamed.path, "assets/animations/Villain.controller.json");
        let undo: Vec<FileDiff> = renamed
            .files
            .iter()
            .map(|file| FileDiff {
                path: file.path.clone(),
                before: file.after.clone(),
                after: file.before.clone(),
            })
            .collect();
        apply_file_diffs(&folder, &undo).unwrap();
        assert_eq!(std::fs::read_to_string(folder.join(&hero)).unwrap(), text);
        assert!(!folder.join(&renamed.path).exists());
        std::fs::remove_dir_all(&folder).unwrap();
    }
}
