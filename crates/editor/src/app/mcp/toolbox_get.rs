//! `toolbox_get`: the Toolbox tab's Get for one asset `toolbox_search`
//! found. The package is fetched once into the central asset directory
//! (verified, recorded with its source and licence) and installed from
//! there as the tab installs it: a texture set (plus a material made from
//! it, as Make material does, so it can be assigned), a sky, a sound, or a
//! model through the import, placed where asked. One undo step that takes
//! the project's new files (and a model's objects) away again; the
//! package stays in the asset directory. Needs the network the first time;
//! blocks while it downloads.

use axcenity_assets::{MATERIAL_SUFFIX, MATERIALS_FOLDER, material_json, read_texture_set};
use axcenity_core::Vector3;
use axcenity_mcp::{Args, Class, Code, Kind, Outcome, Param, Profile, ToolError, ToolSpec};
use axcenity_toolbox::directory;

use super::super::custom_materials::free_stem;
use super::super::files::{asset_files, files_made_since};
use super::Host;
use super::asset_import::{move_to, new_roots};
use super::ops::checkpoint;
use super::refs::{label, quoted, resolve};
use super::toolbox_search::network_error;
use crate::toolbox_panel::agent::get_asset;
use crate::toolbox_panel::install::{self, Installed};

pub const SPEC: ToolSpec = ToolSpec {
    name: "toolbox_get",
    title: "Get an online asset",
    summary: "Download an asset toolbox_search found into the project: a texture (with a material), sky, sound or model.",
    profile: Profile::Full,
    class: Class::Add,
    idempotent: false,
    params: &[
        Param::required(
            "key",
            Kind::String,
            "from toolbox_search, e.g. \"ambientcg:Ground054\"",
        ),
        Param::optional("resolution", Kind::String, "1K (default), 2K, 4K..."),
        Param::optional("format", Kind::String, "e.g. JPG, PNG, HDR, GLTF, MP3"),
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
    let key = args.str("key").unwrap_or_default().trim().to_owned();
    let parent = args
        .str("parent")
        .map(|text| resolve(&host.app.world, text))
        .transpose()?;
    let Some(assets) = host.session.asset_dir.clone().or_else(directory::locate) else {
        return Err(ToolError::new(
            Code::Unsupported,
            "this computer has no folder for downloaded assets",
        )
        .with_fix(format!(
            "the user can set {} to a folder and start again",
            directory::ENV
        )));
    };
    checkpoint(host, &folder);
    let before = asset_files(&folder);
    let got = get_asset(
        host.session.toolbox_http(),
        &assets,
        &folder,
        &key,
        args.str("resolution").unwrap_or("1K"),
        args.str("format").unwrap_or_default(),
    )
    .map_err(|why| network_error("could not get it", why))?;
    let undo = format!("AI ({}): Get {}", host.session.client, got.name);
    let app = &mut host.app;
    let mut out = format!("got {} ({}, {})", got.name, got.package, got.license);
    let mut placed = None;
    match &got.installed {
        Installed::TextureSet { path, name } => {
            out.push_str(&format!(": texture set {path}"));
            // As Make material does, so the client can assign it at once.
            if let Some(material) = material_from(&folder, path, name) {
                out.push_str(&format!(
                    ", material {material} · use it with material_edit {{\"action\":\"assign\",\"path\":\"{material}\",\"ids\":[...]}}"
                ));
            }
        }
        Installed::Sky(path) => out.push_str(&format!(
            ": sky {path} · use it with env_set {{\"sky\":\"{path}\"}}"
        )),
        Installed::Sound(path) => out.push_str(&format!(
            ": sound {path} · play it from a Sound object with SoundId \"{path}\""
        )),
        Installed::Model { file, key, variant } => {
            app.toolbox_imports
                .insert(file.clone(), (key.clone(), variant.clone()));
            let revision = app.history.revision();
            let selected = app.selected;
            app.selected = parent;
            let result = app.import_model_without_asking(file);
            app.selected = selected;
            if result.is_err() || app.history.revision() == revision {
                app.toolbox_imports.remove(file);
                let reason = result
                    .err()
                    .unwrap_or_else(|| "it has nothing to import".into());
                return Err(ToolError::new(
                    Code::BadValue,
                    format!("got it, but could not import it: {reason}"),
                ));
            }
            app.history.relabel_top(undo.clone());
            app.history.seal();
            let roots = new_roots(app);
            if let Some([x, y, z]) = args.vector3("position") {
                move_to(
                    &mut app.world,
                    &roots,
                    Vector3::new(x as f32, y as f32, z as f32),
                );
            }
            let world = &app.world;
            placed = Some(
                roots
                    .iter()
                    .take(5)
                    .map(|&id| label(world, id))
                    .collect::<Vec<_>>()
                    .join(", "),
            );
        }
    }
    if let Some(placed) = placed {
        let made = files_made_since(&folder, &before);
        out.push_str(&format!(
            ": {} file{} · placed {placed}",
            made.len(),
            if made.len() == 1 { "" } else { "s" }
        ));
    } else {
        // A copy isn't an undo step in the tab; here it is, so the client
        // can take it back.
        if parent.is_some() || args.get("position").is_some() {
            out.push_str(" · parent and position are for models; left out");
        }
        let made = files_made_since(&folder, &before);
        let paths: Vec<String> = made.iter().map(|file| file.path.clone()).collect();
        app.history.record_blobs(&mut app.world, undo.clone(), made);
        app.history.seal();
        app.disk_changed(&paths);
    }
    app.asset_browser.refresh();
    app.inventory.refresh();
    if !got.notes.is_empty() {
        out.push_str(&format!(" · {}", got.notes));
    }
    if let Some(credit) = &got.credit {
        out.push_str(&format!(
            " · credit needed: {credit} (Build Game's CREDITS.txt lists it)"
        ));
    }
    host.session.last_undo = Some(undo.clone());
    out.push_str(&format!(" · undo {}", quoted(&undo)));
    Ok(out)
}

/// Writes a material file from the texture set at `set`, named after it,
/// and returns its path; `None` when the set can't be read.
fn material_from(folder: &std::path::Path, set: &str, name: &str) -> Option<String> {
    let texture_set = read_texture_set(&folder.join(set)).ok()?;
    let material = install::material_from_set(&texture_set);
    let path = format!(
        "{MATERIALS_FOLDER}/{}{MATERIAL_SUFFIX}",
        free_stem(folder, name)
    );
    axcenity_project::write_script(folder, &path, &material_json(&material)).ok()?;
    Some(path)
}
