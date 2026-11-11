//! `asset_find`: the project's asset files (what the Assets panel lists,
//! `asset_browser::scan`), by kind and name, with their sizes. `unused`
//! keeps those nothing names (`project::unused_assets`, as the panel's
//! Unused filter); `detail:"full"` adds what names each one
//! (`project::find_references`, as Find References). Missing files are
//! `project_check`'s.

use axcenity_mcp::{Args, Class, Detail, Kind, Outcome, Param, Profile, ToolError, ToolSpec};

use super::{DETAIL, Host, LIMIT};
use crate::asset_browser::{AssetEntry, AssetKind, scan};

pub const SPEC: ToolSpec = ToolSpec {
    name: "asset_find",
    title: "Find assets",
    summary: "List the project's asset files by kind or name, unused ones, or what uses each (detail full).",
    profile: Profile::Standard,
    class: Class::Read,
    idempotent: true,
    params: &[
        Param::optional("kind", Kind::Enum(KINDS), "default all"),
        Param::optional("name", Kind::String, "path contains"),
        Param::optional("unused", Kind::Bool, "only what nothing uses"),
        DETAIL,
        LIMIT,
    ],
};

const KINDS: &[&str] = &[
    "model",
    "texture",
    "material",
    "audio",
    "prefab",
    "animation",
    "controller",
    "sky",
    "data",
    "sequence",
    "behavior",
];

/// What a client calls `kind`.
fn kind_name(kind: AssetKind) -> &'static str {
    match kind {
        AssetKind::Prop => "prop",
        AssetKind::Model => "model",
        AssetKind::Prefab => "prefab",
        AssetKind::Image | AssetKind::Texture | AssetKind::TextureSet | AssetKind::Particle => {
            "texture"
        }
        AssetKind::Material => "material",
        AssetKind::Audio => "audio",
        AssetKind::Animation => "animation",
        AssetKind::Controller => "controller",
        AssetKind::BehaviorTree => "behavior",
        AssetKind::DataTable => "data",
        AssetKind::Sequence => "sequence",
        AssetKind::Sky => "sky",
    }
}

/// The most references shown for one asset.
const MAX_REFERENCES: usize = 10;

pub fn call(host: &mut Host, args: &Args) -> Outcome {
    let app = &host.app;
    let project = app
        .project
        .as_ref()
        .ok_or_else(|| ToolError::not_found("no project is open"))?;
    let folder = project.folder.clone();
    let level = project.level.clone();
    let name = args.str("name").map(str::to_lowercase);
    let mut found: Vec<AssetEntry> = scan(&folder)
        .into_iter()
        .filter(|entry| {
            args.str("kind")
                .is_none_or(|kind| kind_name(entry.kind) == kind)
        })
        .filter(|entry| {
            name.as_ref()
                .is_none_or(|name| entry.path.to_lowercase().contains(name))
        })
        .collect();
    if args.bool("unused") == Some(true) {
        let paths: Vec<String> = found.iter().map(|entry| entry.path.clone()).collect();
        let unused = axcenity_project::unused_assets(&folder, &app.world, level.as_deref(), &paths);
        found.retain(|entry| unused.contains(&entry.path));
    }
    let detail = args.detail(Detail::Std);
    let limit = args.limit(if detail == Detail::Full { 10 } else { 50 }, 500);
    let mut out = format!(
        "{} asset{}{}",
        found.len(),
        if found.len() == 1 { "" } else { "s" },
        if args.bool("unused") == Some(true) {
            " nothing uses"
        } else {
            ""
        }
    );
    if found.is_empty() {
        out.push_str(" · assets are files under assets/; asset_import or the Toolbox adds them");
        return Ok(out);
    }
    for entry in found.iter().take(limit) {
        let size = std::fs::metadata(folder.join(&entry.path)).map_or(0, |meta| meta.len());
        out.push_str(&format!(
            "\n{} · {} · {}",
            entry.path,
            kind_name(entry.kind),
            bytes(size)
        ));
        if detail == Detail::Full {
            let references = axcenity_project::find_references(
                &folder,
                &app.world,
                level.as_deref(),
                &entry.path,
            );
            if references.is_empty() {
                out.push_str("\n  used by nothing");
            }
            for reference in references.iter().take(MAX_REFERENCES) {
                out.push_str(&format!(
                    "\n  used by {} ({})",
                    reference.owner, reference.what
                ));
            }
            if references.len() > MAX_REFERENCES {
                out.push_str(&format!("\n  … {} more", references.len() - MAX_REFERENCES));
            }
        }
    }
    if found.len() > limit {
        out.push_str(&format!(
            "\n… {} more; raise limit or give kind or name",
            found.len() - limit
        ));
    }
    Ok(out)
}

/// `12 KB`, `3.4 MB`.
fn bytes(size: u64) -> String {
    match size {
        0..1024 => format!("{size} B"),
        1024..1_048_576 => format!("{} KB", size / 1024),
        _ => format!("{:.1} MB", size as f64 / 1_048_576.0),
    }
}
