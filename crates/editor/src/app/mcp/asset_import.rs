//! `asset_import`: copies a model, texture, sound, sky image or prefab file
//! into the project, as Import… does, and places a model. Only from a
//! folder the user allowed when starting the server (`--allow-import`), so
//! a client can't read the rest of the computer. One undo step that takes
//! the copied files away again (and a model's objects).

use std::path::{Path, PathBuf};

use axcenity_core::{InstanceId, Value, Vector3};
use axcenity_mcp::{Args, Class, Code, Kind, Outcome, Param, Profile, ToolError, ToolSpec};

use super::super::assets::copy_prefab;
use super::super::files::{asset_files, files_made_since};
use super::Host;
use super::ops::checkpoint;
use super::refs::{label, quoted, resolve};
use crate::asset_browser::{AssetKind, kind_of};

pub const SPEC: ToolSpec = ToolSpec {
    name: "asset_import",
    title: "Import an asset",
    summary: "Copy a model, texture, sound, sky or prefab file from an allowed folder into the project; places a model.",
    profile: Profile::Standard,
    class: Class::Add,
    idempotent: false,
    params: &[
        Param::required("file", Kind::String, "full path on this computer"),
        Param::optional("parent", Kind::String, "model's parent"),
        Param::optional("position", Kind::Vector3, "model's centre"),
    ],
};

pub fn call(host: &mut Host, args: &Args) -> Outcome {
    let Some(folder) = host
        .app
        .project
        .as_ref()
        .map(|project| project.folder.clone())
    else {
        return Err(ToolError::not_found("no project is open"));
    };
    if host.app.playing() {
        return Err(ToolError::new(Code::Busy, "the game is playing")
            .with_fix("stop Play in the editor, then try again"));
    }
    let file = allowed(
        &host.session.import_folders,
        args.str("file").unwrap_or_default(),
    )?;
    let name = file.to_string_lossy().replace('\\', "/");
    let kind = kind_of(&name);
    if !matches!(
        kind,
        Some(
            AssetKind::Model
                | AssetKind::Audio
                | AssetKind::Image
                | AssetKind::Texture
                | AssetKind::Particle
                | AssetKind::Sky
                | AssetKind::Prefab
        )
    ) {
        return Err(ToolError::new(
            Code::Unsupported,
            format!(
                "{} isn't a model, image, sky, sound or prefab file",
                quoted(&name)
            ),
        )
        .with_fix(
            "models: .gltf .glb .fbx; images: .png .jpg; skies: .hdr .exr; sounds: .ogg .wav .mp3",
        ));
    }
    let is_model = kind == Some(AssetKind::Model);
    let parent = args
        .str("parent")
        .map(|text| resolve(&host.app.world, text))
        .transpose()?;
    if !is_model && (parent.is_some() || args.get("position").is_some()) {
        return Err(ToolError::bad_args(
            "only a model is placed: leave parent and position out",
        ));
    }
    checkpoint(host, &folder);
    let stem = file
        .file_stem()
        .map_or_else(String::new, |stem| stem.to_string_lossy().into_owned());
    let undo = format!("AI ({}): Import {stem}", host.session.client);
    let app = &mut host.app;
    let before = asset_files(&folder);
    let revision = app.history.revision();
    let copied = match kind {
        Some(AssetKind::Model) => {
            let from = app.output.total();
            // Where Insert would put it, unless the client said where.
            let selected = app.selected;
            app.selected = parent;
            let result = app.import_model_without_asking(&file);
            app.selected = selected;
            result.map_err(|reason| {
                ToolError::new(Code::BadValue, format!("could not import it: {reason}"))
            })?;
            if app.history.revision() == revision {
                let reason = app
                    .output
                    .since(from)
                    .iter()
                    .rev()
                    .find(|line| line.kind == crate::output::OutputKind::Error)
                    .map_or_else(
                        || "it has nothing to import".to_owned(),
                        |line| line.text.clone(),
                    );
                return Err(ToolError::new(Code::BadValue, reason));
            }
            None
        }
        Some(AssetKind::Audio) => {
            Some(axcenity_assets::import_audio(&folder, &file).map_err(|e| e.to_string()))
        }
        Some(AssetKind::Image | AssetKind::Texture | AssetKind::Particle) => Some(
            axcenity_assets::import_texture(
                &folder,
                &file,
                axcenity_assets::PROJECT_TEXTURES_FOLDER,
            )
            .map_err(|e| e.to_string()),
        ),
        Some(AssetKind::Sky) => {
            Some(axcenity_assets::import_sky_image(&folder, &file).map_err(|e| e.to_string()))
        }
        Some(AssetKind::Prefab) => Some(copy_prefab(&folder, &file)),
        _ => unreachable!("checked above"),
    };
    let mut out;
    match copied {
        Some(result) => {
            let path = result.map_err(|reason| {
                ToolError::new(Code::BadValue, format!("could not import it: {reason}"))
            })?;
            // A plain copy isn't an undo step in the editor; here it is, so
            // the client can take it back.
            let made = files_made_since(&folder, &before);
            app.history.record_blobs(&mut app.world, undo.clone(), made);
            app.history.seal();
            app.disk_changed(std::slice::from_ref(&path));
            app.asset_browser.refresh();
            out = format!("imported {path}");
        }
        None => {
            app.history.relabel_top(undo.clone());
            app.history.seal();
            let made: Vec<String> = files_made_since(&folder, &before)
                .into_iter()
                .map(|file| file.path)
                .collect();
            let roots = new_roots(app);
            if let Some([x, y, z]) = args.vector3("position") {
                move_to(
                    &mut app.world,
                    &roots,
                    Vector3::new(x as f32, y as f32, z as f32),
                );
            }
            out = format!(
                "imported {} file{}: {}",
                made.len(),
                if made.len() == 1 { "" } else { "s" },
                made.iter().take(5).cloned().collect::<Vec<_>>().join(", ")
            );
            let world = &app.world;
            let placed: Vec<String> = roots.iter().take(5).map(|&id| label(world, id)).collect();
            if !placed.is_empty() {
                out.push_str(&format!(" · placed {}", placed.join(", ")));
            }
        }
    }
    host.session.last_undo = Some(undo.clone());
    out.push_str(&format!(" · undo {}", quoted(&undo)));
    Ok(out)
}

/// `text` as a file inside one of `folders`, or why it can't be read.
/// Links are followed before checking, so one can't point out of them.
fn allowed(folders: &[PathBuf], text: &str) -> Result<PathBuf, ToolError> {
    if folders.is_empty() {
        return Err(
            ToolError::new(Code::NotAllowed, "importing needs a folder the user allows")
                .with_fix("the user can add --allow-import <folder> to this server's command"),
        );
    }
    let shown = || {
        folders
            .iter()
            .map(|folder| folder.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    };
    let file = Path::new(text).canonicalize().map_err(|_| {
        ToolError::not_found(format!("no file {}", quoted(text)))
            .with_fix(format!("allowed folders: {}", shown()))
    })?;
    let inside = folders.iter().any(|folder| {
        folder
            .canonicalize()
            .is_ok_and(|folder| file.starts_with(folder))
    });
    if !inside {
        return Err(ToolError::new(
            Code::NotAllowed,
            format!("{} isn't in a folder the user allows", quoted(text)),
        )
        .with_fix(format!("allowed folders: {}", shown())));
    }
    if !file.is_file() {
        return Err(ToolError::not_found(format!(
            "{} isn't a file",
            quoted(text)
        )));
    }
    Ok(file)
}

/// What the import step added to the tree: the new objects not inside
/// another new one.
pub(super) fn new_roots(app: &super::super::EditorApp) -> Vec<InstanceId> {
    // The import selects what it placed, as Insert does.
    let world = &app.world;
    let picked: Vec<InstanceId> = app.selection.clone();
    picked
        .iter()
        .copied()
        .filter(|&id| {
            !picked
                .iter()
                .any(|&other| other != id && world.is_descendant_of(id, other))
        })
        .collect()
}

/// Moves `roots` (and their parts) so the first one's centre is at `to`.
pub(super) fn move_to(world: &mut axcenity_core::World, roots: &[InstanceId], to: Vector3) {
    let Some(&first) = roots.first() else {
        return;
    };
    let centre = std::iter::once(first)
        .chain(world.descendants_iter(first))
        .find_map(|id| match world.get(id)?.property("Position") {
            Some(Value::Vector3(at)) => Some(*at),
            _ => None,
        });
    let Some(centre) = centre else {
        return;
    };
    let offset = Vector3::new(to.x - centre.x, to.y - centre.y, to.z - centre.z);
    // The step that placed them records where they are in the tree, not
    // their properties, so moving them now is part of that step: undo
    // takes them out, and redo puts them back here.
    for &root in roots {
        let inside: Vec<InstanceId> = std::iter::once(root)
            .chain(world.descendants_iter(root))
            .collect();
        for id in inside {
            let Some(instance) = world.get(id) else {
                continue;
            };
            if instance.property("Size").is_none() {
                continue;
            }
            if let Some(Value::Vector3(at)) = instance.property("Position").cloned() {
                let moved = Vector3::new(at.x + offset.x, at.y + offset.y, at.z + offset.z);
                let _ = world.set_property(id, "Position", Value::Vector3(moved));
            }
        }
    }
}
