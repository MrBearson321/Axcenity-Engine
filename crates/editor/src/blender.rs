//! Edit in Blender (roadmap R15.4, decision D4 in
//! `docs/research/model-studio-r15.md`): Blender is launched on a model,
//! not embedded, and nothing is installed into it.
//!
//! Blender is found from Settings > External tools (a path to its program),
//! or, when that's empty, on `PATH` and in the usual install folders. It's
//! started with a small Python script ([`EDIT_SCRIPT`], written to the
//! project's `.axcenity/blender/` folder) that opens the `.glb` in an empty
//! scene and keeps a `.blend` beside the script, so Blender's own Save
//! (Ctrl+S) also exports the `.glb` back over the project's file. The
//! editor's watcher picks that up: MeshParts reload, and an open Model
//! Studio document of the file offers to load it again.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Where the script and the working `.blend` files go, inside the project.
pub const BLENDER_FOLDER: &str = ".axcenity/blender";

/// The script's file name in [`BLENDER_FOLDER`].
pub const SCRIPT_NAME: &str = "axcenity_edit.py";

/// The script Blender runs: `blender --python <script> -- <glb> <blend>`.
/// It empties the start scene, imports the `.glb`, saves the scene as the
/// working `.blend`, and exports the `.glb` again after every save.
pub const EDIT_SCRIPT: &str = r#"# Written by the Axcenity editor (Edit in Blender). Safe to delete.
import sys
import bpy

args = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
if len(args) >= 2:
    glb, blend = args[0], args[1]

    # The start file's cube, camera and light aren't part of the model.
    bpy.ops.wm.read_homefile(use_empty=True)
    bpy.ops.import_scene.gltf(filepath=glb)

    def export_to_axcenity(*_):
        # Every object, as the editor reads it: _LOD1/_LOD2 copies and
        # _COL collision pieces keep their names.
        bpy.ops.export_scene.gltf(filepath=glb, export_format="GLB")
        print("Axcenity: exported", glb)

    bpy.app.handlers.save_post.append(export_to_axcenity)
    # Saving once here gives Ctrl+S a file to save to without asking.
    bpy.ops.wm.save_as_mainfile(filepath=blend)
    print("Axcenity: editing", glb, "- save (Ctrl+S) to send it back")
"#;

/// The Blender program to start: `setting` (Settings > External tools)
/// when it names a file, else `blender` on `PATH`, else the newest one in
/// the usual install folders. `None` if there's none.
pub fn find_blender(setting: &str) -> Option<PathBuf> {
    let setting = setting.trim();
    if !setting.is_empty() {
        let path = PathBuf::from(setting);
        return path.is_file().then_some(path);
    }
    let path = std::env::var_os("PATH").unwrap_or_default();
    on_path(&path).or_else(|| installed(&install_folders()))
}

/// The program's file name.
fn program() -> &'static str {
    if cfg!(windows) {
        "blender.exe"
    } else {
        "blender"
    }
}

/// `blender` in one of the folders of `path` (a `PATH` value).
fn on_path(path: &std::ffi::OsStr) -> Option<PathBuf> {
    std::env::split_paths(path)
        .map(|folder| folder.join(program()))
        .find(|file| file.is_file())
}

/// Where Blender is usually installed: on Windows a folder per version
/// under `Blender Foundation`, on macOS the app, on Linux the system and
/// snap folders.
fn install_folders() -> Vec<PathBuf> {
    if cfg!(windows) {
        let program_files =
            std::env::var_os("ProgramFiles").unwrap_or_else(|| "C:\\Program Files".into());
        vec![PathBuf::from(program_files).join("Blender Foundation")]
    } else if cfg!(target_os = "macos") {
        vec![PathBuf::from("/Applications/Blender.app/Contents/MacOS")]
    } else {
        vec![
            PathBuf::from("/usr/bin"),
            PathBuf::from("/usr/local/bin"),
            PathBuf::from("/snap/bin"),
        ]
    }
}

/// The program in one of `folders`, or in a version folder inside one
/// (`Blender 4.2`, `Blender 4.5`: the last by name, so the newest).
fn installed(folders: &[PathBuf]) -> Option<PathBuf> {
    let name = if cfg!(target_os = "macos") {
        "Blender"
    } else {
        program()
    };
    folders.iter().find_map(|folder| {
        let direct = folder.join(name);
        if direct.is_file() {
            return Some(direct);
        }
        let mut versions: Vec<PathBuf> = std::fs::read_dir(folder)
            .ok()?
            .flatten()
            .map(|entry| entry.path().join(name))
            .filter(|file| file.is_file())
            .collect();
        versions.sort();
        versions.pop()
    })
}

/// The working `.blend` for the project model `model` (a project path),
/// in [`BLENDER_FOLDER`]: the model's path with its folders joined by `_`.
pub fn blend_path(project: &Path, model: &str) -> PathBuf {
    let stem = model.rsplit_once('.').map_or(model, |(stem, _)| stem);
    let name: String = stem
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    project.join(BLENDER_FOLDER).join(format!("{name}.blend"))
}

/// The command that opens `glb` in `blender` through the script at
/// `script`, keeping its scene at `blend`.
pub fn edit_command(blender: &Path, script: &Path, glb: &Path, blend: &Path) -> Command {
    let mut command = Command::new(blender);
    command
        .arg("--python")
        .arg(script)
        .arg("--")
        .arg(glb)
        .arg(blend);
    command
}

/// Writes the script into the project and starts Blender on the project
/// model `model` (a `.glb`). Returns the program started.
pub fn edit_in_blender(setting: &str, project: &Path, model: &str) -> Result<PathBuf, String> {
    let blender = find_blender(setting).ok_or_else(|| {
        if setting.trim().is_empty() {
            "Blender wasn't found. Install it, or set where it is in Settings > External tools."
                .to_owned()
        } else {
            format!(
                "Blender isn't at `{}` (Settings > External tools).",
                setting.trim()
            )
        }
    })?;
    let relative = axcenity_assets::safe_path(model)
        .ok_or_else(|| format!("`{model}` isn't a file in the project."))?;
    let glb = project.join(relative);
    let folder = project.join(BLENDER_FOLDER);
    std::fs::create_dir_all(&folder).map_err(|err| err.to_string())?;
    let script = folder.join(SCRIPT_NAME);
    std::fs::write(&script, EDIT_SCRIPT).map_err(|err| err.to_string())?;
    edit_command(&blender, &script, &glb, &blend_path(project, model))
        .spawn()
        .map_err(|err| format!("Blender couldn't be started: {err}"))?;
    Ok(blender)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let folder =
            std::env::temp_dir().join(format!("axcenity-blender-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        folder
    }

    #[test]
    fn blender_is_found_from_the_setting_then_path_then_install_folders() {
        let folder = temp("find");
        let program = folder.join("bin").join(super::program());
        std::fs::create_dir_all(program.parent().unwrap()).unwrap();
        std::fs::write(&program, "").unwrap();

        // A setting naming a file wins; one naming nothing finds nothing.
        assert_eq!(
            find_blender(program.to_str().unwrap()),
            Some(program.clone())
        );
        assert_eq!(find_blender(folder.join("nope").to_str().unwrap()), None);

        let path = std::env::join_paths([folder.join("empty"), folder.join("bin")]).unwrap();
        assert_eq!(on_path(&path), Some(program.clone()));
        assert_eq!(on_path(std::ffi::OsStr::new("")), None);

        // The newest version folder by name.
        let foundation = folder.join("Blender Foundation");
        let name = if cfg!(target_os = "macos") {
            "Blender"
        } else {
            super::program()
        };
        for version in ["Blender 4.2", "Blender 4.5"] {
            let file = foundation.join(version).join(name);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(&file, "").unwrap();
        }
        assert_eq!(
            installed(&[folder.join("missing"), foundation.clone()]),
            Some(foundation.join("Blender 4.5").join(name))
        );
        let _ = std::fs::remove_dir_all(folder);
    }

    #[test]
    fn the_command_runs_the_script_on_the_model_and_its_blend() {
        let project = Path::new("/projects/game");
        let blend = blend_path(project, "assets/models/Rock Pile.glb");
        assert_eq!(
            blend,
            project.join(".axcenity/blender/assets_models_Rock_Pile.blend")
        );
        let command = edit_command(
            Path::new("/opt/blender/blender"),
            &project.join(".axcenity/blender/axcenity_edit.py"),
            &project.join("assets/models/Rock Pile.glb"),
            &blend,
        );
        assert_eq!(command.get_program(), "/opt/blender/blender");
        let args: Vec<String> = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert_eq!(args[0], "--python");
        assert!(args[1].ends_with("axcenity_edit.py"));
        assert_eq!(args[2], "--");
        assert!(args[3].ends_with("Rock Pile.glb"));
        assert!(args[4].ends_with("assets_models_Rock_Pile.blend"));
    }

    #[test]
    fn the_script_imports_saves_and_exports_back_on_every_save() {
        for needed in [
            "read_homefile(use_empty=True)",
            "import_scene.gltf(filepath=glb)",
            "export_scene.gltf(filepath=glb, export_format=\"GLB\")",
            "save_post.append",
            "save_as_mainfile(filepath=blend)",
        ] {
            assert!(EDIT_SCRIPT.contains(needed), "{needed}");
        }
    }

    #[test]
    fn a_missing_blender_says_where_to_set_it() {
        let project = temp("missing");
        let why = edit_in_blender(
            project.join("no-blender").to_str().unwrap(),
            &project,
            "assets/models/a.glb",
        )
        .unwrap_err();
        assert!(why.contains("Settings > External tools"), "{why}");
        assert!(!project.join(BLENDER_FOLDER).exists(), "nothing written");
        let _ = std::fs::remove_dir_all(project);
    }
}
