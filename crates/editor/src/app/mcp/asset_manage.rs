//! `asset_manage`: the Assets panel's Rename or Move… and Delete.
//! `rename` (`to` a new file name, or a new path to move it) rewrites every
//! reference to match (`project::plan_rename`: properties, scripts, other
//! files), as one undo step. `delete` asks first, naming what still uses
//! the file; undo puts it back.

use axcenity_mcp::{Args, Class, Code, Kind, Outcome, Param, Profile, ToolError, ToolSpec};

use super::material_edit::check_path;
use super::ops::{checkpoint, confirm};
use super::refs::quoted;
use super::{CONFIRM, Host};

pub const SPEC: ToolSpec = ToolSpec {
    name: "asset_manage",
    title: "Rename or delete assets",
    summary: "Rename or move an asset file, updating everything that uses it, or delete one.",
    profile: Profile::Full,
    class: Class::Destructive,
    idempotent: false,
    params: &[
        Param::required("action", Kind::Enum(&["rename", "delete"]), ""),
        Param::required("path", Kind::String, "the asset's project path"),
        Param::optional(
            "to",
            Kind::String,
            "rename: new name, or a new path to move it",
        ),
        CONFIRM,
    ],
};

pub fn call(host: &mut Host, args: &Args) -> Outcome {
    let Some(project) = host.app.project.as_ref() else {
        return Err(ToolError::not_found("no project is open"));
    };
    let folder = project.folder.clone();
    let level = project.level.clone();
    if host.app.playing() {
        return Err(ToolError::new(Code::Busy, "the game is playing")
            .with_fix("stop Play in the editor, then try again"));
    }
    let path = args.str("path").unwrap_or_default().to_owned();
    check_path(&folder, &path)?;
    let file_name = path.rsplit('/').next().unwrap_or(&path).to_owned();
    let revision = host.app.history.revision();
    let from = host.app.output.total();
    let done = if args.str("action") == Some("rename") {
        let Some(to) = args.str("to").map(str::trim).filter(|to| !to.is_empty()) else {
            return Err(ToolError::bad_args("rename needs to"));
        };
        // A bare name stays in the same folder.
        let to = if to.contains('/') {
            to.to_owned()
        } else {
            match path.rsplit_once('/') {
                Some((parent, _)) => format!("{parent}/{to}"),
                None => to.to_owned(),
            }
        };
        let inside = std::path::Path::new(&to)
            .components()
            .all(|part| matches!(part, std::path::Component::Normal(_)));
        if !inside {
            return Err(ToolError::new(
                Code::BadValue,
                format!("{} isn't a path in the project", quoted(&to)),
            ));
        }
        checkpoint(host, &folder);
        host.app.rename_asset(&path, &to);
        format!("{path} is now {to}, and what used it follows")
    } else {
        let users =
            axcenity_project::find_references(&folder, &host.app.world, level.as_deref(), &path);
        let named: Vec<String> = users
            .iter()
            .take(8)
            .map(|user| user.owner.to_string())
            .collect();
        let preview = if users.is_empty() {
            format!("deletes {path}; nothing uses it")
        } else {
            format!(
                "deletes {path}, which {} thing{} still use{}: {}{}",
                users.len(),
                if users.len() == 1 { "" } else { "s" },
                if users.len() == 1 { "s" } else { "" },
                named.join(", "),
                if users.len() > named.len() {
                    ", …"
                } else {
                    ""
                }
            )
        };
        confirm(host, &SPEC, args, &preview)?;
        checkpoint(host, &folder);
        host.app
            .delete_assets(&file_name, std::slice::from_ref(&path));
        host.app.asset_browser.refresh();
        format!("deleted {path}")
    };
    if host.app.history.revision() == revision {
        let reason = host
            .app
            .output
            .since(from)
            .iter()
            .rev()
            .find(|line| line.kind == crate::output::OutputKind::Error)
            .map_or_else(|| "nothing changed".to_owned(), |line| line.text.clone());
        return Err(ToolError::new(Code::BadValue, reason));
    }
    let undo = format!(
        "AI ({}): {}",
        host.session.client,
        host.app.history.undo_label().unwrap_or_default()
    );
    host.app.history.relabel_top(undo.clone());
    host.app.history.seal();
    host.session.last_undo = Some(undo.clone());
    Ok(format!("{done} · undo {}", quoted(&undo)))
}
