//! `prefab`: the editor's prefab actions (Phase 49). `save` is Save as
//! Prefab on `id`/`ids` (they become linked copies); `place` puts a linked
//! copy of a prefab file in the world; `overrides` lists what a copy
//! changes; `apply` writes a copy's overrides into its prefab, which every
//! copy then follows, so it asks first; `revert` and `unpack` are the
//! copy menu's. One undo step each.

use axcenity_core::{Vector3, prefab};
use axcenity_mcp::{Args, Class, Code, Kind, Outcome, Param, Profile, ToolError, ToolSpec};

use super::asset_import::move_to;
use super::material_edit::check_path;
use super::ops::{checkpoint, confirm};
use super::refs::{id_text, label, quoted, resolve};
use super::{CONFIRM, Host, ID, IDS};
use crate::prefab_menu::{PrefabAction, overridden_properties};
use crate::tree;

pub const SPEC: ToolSpec = ToolSpec {
    name: "prefab",
    title: "Prefabs",
    summary: "Save objects as a prefab, place a prefab, list a copy's overrides, apply, revert or unpack them.",
    profile: Profile::Full,
    class: Class::Modify,
    idempotent: false,
    params: &[
        Param::required(
            "action",
            Kind::Enum(&["save", "place", "overrides", "apply", "revert", "unpack"]),
            "",
        ),
        ID,
        IDS,
        Param::optional("path", Kind::String, "place: the .prefab.json"),
        Param::optional("parent", Kind::String, "place: default Workspace"),
        Param::optional("position", Kind::Vector3, "place: centre"),
        CONFIRM,
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
    let action = args.str("action").unwrap_or_default();
    if action != "overrides" && host.app.playing() {
        return Err(ToolError::new(Code::Busy, "the game is playing")
            .with_fix("stop Play in the editor, then try again"));
    }
    let client = host.session.client.clone();
    let undo = |what: &str| format!("AI ({client}): {what}");
    let revision = host.app.history.revision();
    let (done, label_text) = match action {
        "save" => {
            let mut ids = Vec::new();
            for text in args
                .str("id")
                .into_iter()
                .chain(args.strings("ids").unwrap_or_default())
            {
                ids.push(resolve(&host.app.world, text)?);
            }
            if ids.is_empty() {
                return Err(ToolError::bad_args("save needs id or ids"));
            }
            let world = &host.app.world;
            if let Some(&wrong) = ids.iter().find(|&&id| !tree::can_be_prefab(world, id)) {
                return Err(ToolError::new(
                    Code::BadValue,
                    format!("{} can't be in a prefab", label(world, wrong)),
                )
                .with_fix("parts, Models and Folders in the Workspace can"));
            }
            checkpoint(host, &folder);
            let selection = std::mem::replace(&mut host.app.selection, ids.clone());
            host.app.save_prefab();
            host.app.selection = selection;
            let world = &host.app.world;
            let path = world
                .get(ids[0])
                .and_then(|instance| instance.prefab())
                .map(|link| link.path.clone())
                .ok_or_else(|| ToolError::internal("the prefab wasn't saved"))?;
            if !host.session.files.contains(&path) {
                host.session.files.push(path.clone());
            }
            let name = crate::prefab_menu::prefab_name(&path).to_owned();
            (
                format!("saved {path}; what you saved is now a linked copy of it"),
                undo(&format!("Create {name}")),
            )
        }
        "place" => {
            let Some(path) = args.str("path") else {
                return Err(ToolError::bad_args("place needs path")
                    .with_fix("asset_find kind prefab lists them"));
            };
            check_path(&folder, path)?;
            let world = &host.app.world;
            let parent = match args.str("parent") {
                Some(text) => resolve(world, text)?,
                None => world.workspace(),
            };
            if parent != world.workspace() {
                tree::check_editable(world, parent)
                    .map_err(|why| ToolError::new(Code::NotAllowed, why))?;
            }
            checkpoint(host, &folder);
            let app = &mut host.app;
            let ids = app.instantiate_prefab(path, parent).map_err(|why| {
                ToolError::new(Code::BadValue, format!("could not place it: {why}"))
            })?;
            if ids.is_empty() {
                return Err(ToolError::new(
                    Code::BadValue,
                    format!("{path} has nothing in it"),
                ));
            }
            // Out again, so placing them is one step that puts them back.
            for &id in &ids {
                let _ = app.world.set_parent(id, None);
            }
            let name = crate::prefab_menu::prefab_name(path).to_owned();
            let text = undo(&format!("Place {name}"));
            app.place_new(parent, ids.clone(), text.clone());
            if let Some([x, y, z]) = args.vector3("position") {
                move_to(
                    &mut app.world,
                    &ids,
                    Vector3::new(x as f32, y as f32, z as f32),
                );
            }
            let world = &app.world;
            let placed: Vec<String> = ids.iter().take(5).map(|&id| label(world, id)).collect();
            (format!("placed {}", placed.join(", ")), text)
        }
        _ => {
            let Some(text) = args.str("id") else {
                return Err(ToolError::bad_args(format!(
                    "{action} needs id: a placed copy"
                )));
            };
            let world = &host.app.world;
            let id = resolve(world, text)?;
            let root = prefab::place(world, id)
                .map(|place| place.root)
                .ok_or_else(|| {
                    ToolError::new(
                        Code::BadValue,
                        format!("{} isn't in a prefab copy", label(world, id)),
                    )
                })?;
            let path = world
                .get(root)
                .and_then(|instance| instance.prefab())
                .map(|link| link.path.clone())
                .unwrap_or_default();
            let overrides = prefab::overrides(world, root);
            let changed = overridden_properties(world, root, &overrides);
            let listed: Vec<String> = changed
                .iter()
                .take(30)
                .map(|(id, property)| format!("{} {property}", id_text(*id)))
                .collect();
            let name = crate::prefab_menu::prefab_name(&path).to_owned();
            let copy = label(world, root);
            match action {
                "overrides" => {
                    let mut out = format!(
                        "{copy} is a copy of {path} · {} override{}",
                        changed.len(),
                        if changed.len() == 1 { "" } else { "s" }
                    );
                    for line in listed {
                        out.push_str(&format!("\n{line}"));
                    }
                    return Ok(out);
                }
                "apply" => {
                    confirm(
                        host,
                        &SPEC,
                        args,
                        &format!(
                            "writes {copy}'s overrides ({}) into {path}; every copy follows it",
                            if listed.is_empty() {
                                "none".to_owned()
                            } else {
                                listed.join(", ")
                            }
                        ),
                    )?;
                    checkpoint(host, &folder);
                    host.app.prefab_action(PrefabAction::ApplyAll(root));
                    if !host.session.files.contains(&path) {
                        host.session.files.push(path.clone());
                    }
                    (
                        format!("applied {copy} to {path}"),
                        undo(&format!("Apply overrides to {name}")),
                    )
                }
                "revert" => {
                    checkpoint(host, &folder);
                    host.app.prefab_action(PrefabAction::RevertAll(root));
                    (
                        format!("{copy} matches {path} again"),
                        undo(&format!("Revert all on {copy}")),
                    )
                }
                _ => {
                    checkpoint(host, &folder);
                    host.app.prefab_action(PrefabAction::Unpack(root));
                    (
                        format!("{copy} no longer follows {path}"),
                        undo(&format!("Unpack {copy}")),
                    )
                }
            }
        }
    };
    if host.app.history.revision() == revision {
        return Ok(format!("{action}: nothing changed"));
    }
    host.app.history.relabel_top(label_text.clone());
    host.app.history.seal();
    host.session.last_undo = Some(label_text.clone());
    Ok(format!("{done} · undo {}", quoted(&label_text)))
}
