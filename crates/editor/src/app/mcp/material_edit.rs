//! `material_edit`: material files (`assets/materials/<name>.material.json`,
//! `docs/rendering/models.md`), the way the Material tool and Material tab
//! make them: `create` one, `change` one's values, or `assign` one to
//! parts (their `MaterialVariant`, and a Part's Color white, as a drop
//! does). Each call is one undo step that puts the file and the parts back.
//!
//! `values` are the file's own fields (camelCase): `color`, `metalness`,
//! `roughness`, `emissiveColor`, `emissiveStrength`, `opacity`, `tiling`,
//! `base`, `maps` (`{"color":"assets/textures/brick.png"}`), …; `null`
//! takes one away.

use axcenity_assets::{
    MATERIAL_SUFFIX, MATERIALS_FOLDER, is_material_file, material_json, parse_material,
};
use axcenity_core::{InstanceId, World};
use axcenity_mcp::{Args, Class, Code, Kind, Outcome, Param, Profile, ToolError, ToolSpec};
use serde_json::{Map, Value as Json};

use super::super::custom_materials::free_stem;
use super::super::material_files::take_material;
use super::super::texture_set::property_changes;
use super::object_set::targets;
use super::ops::checkpoint;
use super::refs::{label, quoted};
use super::{Host, ID, IDS};
use crate::history::{self, TextChange};

pub const SPEC: ToolSpec = ToolSpec {
    name: "material_edit",
    title: "Edit materials",
    summary: "Create or change a material file, or assign one to parts, e.g. {\"action\":\"create\",\"name\":\"Rust\",\"values\":{\"color\":[0.5,0.2,0.1]}}.",
    profile: Profile::Standard,
    class: Class::Modify,
    idempotent: false,
    params: &[
        Param::required("action", Kind::Enum(&["create", "change", "assign"]), ""),
        Param::optional("name", Kind::String, "new material's name"),
        Param::optional("path", Kind::String, "material file to change or assign"),
        Param::optional(
            "values",
            Kind::Object,
            "file fields: color, roughness, maps...; null removes",
        ),
        ID,
        IDS,
    ],
};

/// The fields most materials set, for "unknown field" errors.
const COMMON_FIELDS: &str = "color, metalness, roughness, emissiveColor, emissiveStrength, \
opacity, alphaCutoff, maps, tiling, offset, doubleSided, base";

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
    let action = args.str("action").unwrap_or_default();
    let values = match args.get("values") {
        Some(Json::Object(values)) => Some(values),
        _ => None,
    };
    let parts = if args.get("id").is_some() || args.get("ids").is_some() {
        let ids = targets(&host.app.world, args)?;
        check_parts(&host.app.world, &ids)?;
        ids
    } else {
        Vec::new()
    };
    let (path, before, after) = match action {
        "create" => {
            let name = args.str("name").ok_or_else(|| {
                ToolError::bad_args("create needs name").with_fix("such as \"name\":\"Rust\"")
            })?;
            let stem = free_stem(&folder, name);
            let path = format!("{MATERIALS_FOLDER}/{stem}{MATERIAL_SUFFIX}");
            let empty = Map::new();
            let after = build(None, values.unwrap_or(&empty), &folder)?;
            (path, None, Some(after))
        }
        "change" | "assign" => {
            let path = args.str("path").ok_or_else(|| {
                ToolError::bad_args(format!("{action} needs path"))
                    .with_fix("a material file, such as \"assets/materials/Rust.material.json\"")
            })?;
            let before = read(&folder, path)?;
            if host
                .app
                .materials
                .document(path)
                .is_some_and(|document| document.unsaved())
            {
                return Err(ToolError::new(
                    Code::Locked,
                    format!("{path} has unsaved changes in the Material tab"),
                )
                .with_fix("ask the user to save it first"));
            }
            let after = match (action, values) {
                ("change", Some(values)) => Some(build(Some(&before), values, &folder)?),
                ("change", None) => {
                    return Err(ToolError::bad_args("change needs values")
                        .with_fix("such as \"values\":{\"roughness\":0.3}"));
                }
                (_, Some(_)) => {
                    return Err(ToolError::bad_args("assign takes no values")
                        .with_fix("use action change to change the file"));
                }
                _ => None,
            };
            if action == "assign" && parts.is_empty() {
                return Err(ToolError::bad_args(
                    "assign needs id or ids: the parts to draw in it",
                ));
            }
            (path.to_owned(), Some(before), after)
        }
        _ => unreachable!("Args::check allows only the actions in SPEC"),
    };
    let text = after.clone().or_else(|| before.clone());
    let changes_file = after.is_some() && after != before;
    checkpoint(host, &folder);
    if changes_file {
        let text = after.as_deref().unwrap_or_default();
        axcenity_project::write_script(&folder, &path, text)
            .map_err(|error| ToolError::internal(format!("could not write {path}: {error}")))?;
        host.app.disk_changed(std::slice::from_ref(&path));
        if !host.session.files.contains(&path) {
            host.session.files.push(path.clone());
        }
    }
    let app = &mut host.app;
    let mut properties = Vec::new();
    for &id in &parts {
        let before = history::snapshot(&app.world, id);
        take_material(&mut app.world, id, &path);
        properties.extend(property_changes(
            id,
            before,
            history::snapshot(&app.world, id),
        ));
    }
    let file = path.rsplit('/').next().unwrap_or(&path);
    let stem = file.strip_suffix(MATERIAL_SUFFIX).unwrap_or(file);
    let mut done = Vec::new();
    if changes_file {
        done.push(format!(
            "{} {path}",
            if before.is_none() {
                "created"
            } else {
                "changed"
            }
        ));
    }
    if !parts.is_empty() {
        done.push(format!(
            "drew {} in it",
            match parts.as_slice() {
                [one] => label(&app.world, *one),
                many => format!("{} parts", many.len()),
            }
        ));
    }
    if !changes_file && properties.is_empty() {
        return Ok(format!("{path} is already like that · nothing changed"));
    }
    let verb = match action {
        "create" => "Create material",
        "change" => "Change material",
        _ => "Material",
    };
    let undo = format!("AI ({}): {verb} {stem}", host.session.client);
    let texts = if changes_file {
        vec![TextChange {
            path: path.clone(),
            before: before.clone(),
            after: after.clone(),
        }]
    } else {
        Vec::new()
    };
    app.history
        .record_asset(&mut app.world, undo.clone(), texts, properties);
    app.history.seal();
    host.session.last_undo = Some(undo.clone());
    let mut out = format!("{} · undo {}", done.join(" · "), quoted(&undo));
    // What the file says now, so the model needn't read it back.
    if let Some(text) = text
        && let Ok(Json::Object(fields)) = serde_json::from_str::<Json>(&text)
        && !fields.is_empty()
    {
        out.push('\n');
        out.push_str(&Json::Object(fields).to_string());
    }
    Ok(out)
}

/// The text of the material file at `path` in the project.
fn read(folder: &std::path::Path, path: &str) -> Result<String, ToolError> {
    if !is_material_file(path) {
        return Err(ToolError::new(
            Code::BadValue,
            format!("{} isn't a material file", quoted(path)),
        )
        .with_fix(format!("material files end with {MATERIAL_SUFFIX}")));
    }
    check_path(folder, path)?;
    std::fs::read_to_string(folder.join(path))
        .map_err(|error| ToolError::internal(format!("could not read {path}: {error}")))
}

/// `before` (a material file's text, or nothing) with `values` put in,
/// checked, as the file's new text.
fn build(
    before: Option<&str>,
    values: &Map<String, Json>,
    folder: &std::path::Path,
) -> Result<String, ToolError> {
    let mut fields = match before.map(serde_json::from_str::<Json>) {
        Some(Ok(Json::Object(fields))) => fields,
        Some(_) => {
            return Err(ToolError::new(Code::BadValue, "the file isn't valid JSON")
                .with_fix("fix it in the Material tab or a text editor"));
        }
        None => Map::new(),
    };
    for (key, value) in values {
        match (key.as_str(), value) {
            (_, Json::Null) => {
                fields.remove(key);
            }
            // A map can be given as just its image's path.
            ("maps", Json::Object(maps)) => {
                let mut current = match fields.remove("maps") {
                    Some(Json::Object(current)) => current,
                    _ => Map::new(),
                };
                for (slot, map) in maps {
                    match map {
                        Json::Null => {
                            current.remove(slot);
                        }
                        Json::String(path) => {
                            current.insert(slot.clone(), serde_json::json!({ "path": path }));
                        }
                        map => {
                            current.insert(slot.clone(), map.clone());
                        }
                    }
                }
                fields.insert(key.clone(), Json::Object(current));
            }
            _ => {
                fields.insert(key.clone(), value.clone());
            }
        }
    }
    let text = Json::Object(fields).to_string();
    let file = parse_material(&text).map_err(|error| {
        ToolError::new(Code::BadValue, format!("values: {error}"))
            .with_fix(format!("colors are [r,g,b] 0-1; fields: {COMMON_FIELDS}"))
    })?;
    // A name the file doesn't know is dropped by reading it: say so.
    let kept = serde_json::to_value(&file).unwrap_or_default();
    for (key, value) in values {
        if value.is_null() {
            continue;
        }
        if kept.get(key).is_none() {
            return Err(ToolError::new(
                Code::BadValue,
                format!("a material file has no field {}", quoted(key)),
            )
            .with_fix(format!("fields: {COMMON_FIELDS}")));
        }
        if let (Some(Json::Object(slots)), "maps") = (Some(value), key.as_str()) {
            for (slot, map) in slots {
                if !map.is_null() && kept["maps"].get(slot).is_none() {
                    return Err(ToolError::new(
                        Code::BadValue,
                        format!("no map {}", quoted(slot)),
                    )
                    .with_fix("maps: color, normal, metalness, roughness, occlusion, emissive, opacity, specular"));
                }
            }
        }
    }
    let m = &file.maps;
    let images = [
        &m.color,
        &m.normal,
        &m.metalness,
        &m.roughness,
        &m.occlusion,
        &m.emissive,
        &m.opacity,
        &m.specular,
    ];
    for path in images
        .into_iter()
        .flatten()
        .filter_map(|map| map.path.as_deref())
        .filter(|path| !path.is_empty())
        .chain(file.base.as_deref())
    {
        check_path(folder, path)?;
    }
    Ok(material_json(&file))
}

/// A project-relative path to a file in the project, or why not.
pub(super) fn check_path(folder: &std::path::Path, path: &str) -> Result<(), ToolError> {
    let relative = std::path::Path::new(path);
    let inside = relative.is_relative()
        && relative
            .components()
            .all(|part| matches!(part, std::path::Component::Normal(_)));
    if !inside {
        return Err(ToolError::new(
            Code::BadValue,
            format!("{} isn't a path in the project", quoted(path)),
        )
        .with_fix("give it from the project folder, like \"assets/textures/brick.png\""));
    }
    if !folder.join(relative).is_file() {
        return Err(
            ToolError::not_found(format!("{} isn't in the project", quoted(path)))
                .with_fix("import the file first, or check the name"),
        );
    }
    Ok(())
}

/// Every one of `ids` is a part (or MeshPart) that may change.
fn check_parts(world: &World, ids: &[InstanceId]) -> Result<(), ToolError> {
    for &id in ids {
        let class = world.get(id).map(|instance| instance.class());
        if !class.is_some_and(|class| class.is_base_part()) {
            return Err(ToolError::new(
                Code::BadValue,
                format!("{} isn't a part, so it has no material", label(world, id)),
            ));
        }
        if world.is_locked(id) {
            return Err(
                ToolError::new(Code::Locked, format!("{} is locked", label(world, id)))
                    .with_fix("object_set props {\"Locked\":false} on it first"),
            );
        }
    }
    Ok(())
}
