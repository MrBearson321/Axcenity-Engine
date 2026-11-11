//! `solid_op`: the ribbon's Solid Modeling group (`app/solid.rs`) on
//! `id`/`ids`: Union and Intersect (two or more parts into a
//! UnionOperation, its mesh saved under `assets/models/`), Negate (a part
//! that cuts, or back), Separate (a UnionOperation back into its parts) and
//! To Terrain (parts become ground on the Terrain under them). One undo
//! step each; a boolean that fails says why and changes nothing.

use axcenity_mcp::{Args, Class, Code, Kind, Outcome, Param, Profile, ToolError, ToolSpec};

use super::ops::checkpoint;
use super::refs::{label, quoted, resolve};
use super::{Host, ID, IDS};
use crate::app::solid::SolidAction;

pub const SPEC: ToolSpec = ToolSpec {
    name: "solid_op",
    title: "Solid modeling",
    summary: "Union, intersect, negate or separate parts, or turn them into terrain, as the Solid Modeling buttons do.",
    profile: Profile::Full,
    class: Class::Modify,
    idempotent: false,
    params: &[
        Param::required(
            "op",
            Kind::Enum(&["union", "intersect", "negate", "separate", "to_terrain"]),
            "",
        ),
        ID,
        IDS,
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
    let action = match args.str("op").unwrap_or_default() {
        "union" => SolidAction::Union,
        "intersect" => SolidAction::Intersect,
        "negate" => SolidAction::Negate,
        "separate" => SolidAction::Separate,
        _ => SolidAction::ToTerrain,
    };
    let mut ids = Vec::new();
    for text in args
        .str("id")
        .into_iter()
        .chain(args.strings("ids").unwrap_or_default())
    {
        ids.push(resolve(&host.app.world, text)?);
    }
    let app = &mut host.app;
    let selection = std::mem::replace(&mut app.selection, ids);
    let parts = app.solid_parts();
    if !action.usable(&app.world, &parts) {
        app.selection = selection;
        return Err(ToolError::bad_args(
            action.needs().replace("Select", "Give"),
        ));
    }
    if let Some(&locked) = parts.iter().find(|&&id| app.world.is_locked(id)) {
        let text = label(&app.world, locked);
        app.selection = selection;
        return Err(ToolError::new(Code::Locked, format!("{text} is locked")));
    }
    checkpoint(host, &folder);
    let app = &mut host.app;
    let revision = app.history.revision();
    let from = app.output.total();
    app.solid_action(action);
    let made = std::mem::replace(&mut app.selection, selection);
    if app.history.revision() == revision {
        let reason = app
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
        host.app.history.undo_label().unwrap_or(action.label())
    );
    host.app.history.relabel_top(undo.clone());
    host.app.history.seal();
    host.session.last_undo = Some(undo.clone());
    let world = &host.app.world;
    let shown: Vec<String> = made.iter().take(10).map(|&id| label(world, id)).collect();
    Ok(format!(
        "{} → {} · undo {}",
        action.label(),
        if shown.is_empty() {
            "done".to_owned()
        } else {
            shown.join(", ")
        },
        quoted(&undo)
    ))
}
