//! Find in Scripts over a project (unified Phase 70).

use std::fs;

use crate::find_in_scripts::{FindOptions, Spot};

use super::tests::copy_of_example;
use super::*;

const LUA: &str = "scripts/Watch.server.lua";
const BLOCKS: &str = "scripts/pad.server.blocks.json";

/// The blocks example, with a Lua script naming the same ball.
fn project(test: &str) -> (EditorApp, PathBuf) {
    let folder = copy_of_example("hello-touched-blocks", test);
    fs::write(
        folder.join(LUA),
        "local ball = workspace.BlueBall\nprint(ball.Name, \"BlueBalls\")\n",
    )
    .unwrap();
    let app = EditorApp::new(Some(&folder));
    (app, folder)
}

fn block_text(app: &EditorApp) -> String {
    let document = app
        .project
        .as_ref()
        .unwrap()
        .block_scripts
        .iter()
        .find(|document| document.path == BLOCKS)
        .unwrap();
    serde_json::to_string(&document.script).unwrap()
}

#[test]
fn find_in_scripts_finds_a_name_in_lua_and_blocks_and_replace_all_undoes_in_one_step() {
    let (mut app, folder) = project("find-and-replace");
    app.open_script(LUA, None);
    let lua_before = fs::read(folder.join(LUA)).unwrap();
    let blocks_before = fs::read(folder.join(BLOCKS)).unwrap();

    app.find.query = "BlueBall".to_owned();
    app.find.options = FindOptions {
        match_case: true,
        whole_word: true,
        regex: false,
    };
    app.search_scripts();
    let hits = app.find.results.clone().unwrap();
    let in_lua: Vec<_> = hits.iter().filter(|hit| hit.path == LUA).collect();
    let in_blocks: Vec<_> = hits.iter().filter(|hit| hit.path == BLOCKS).collect();
    // Whole word: `BlueBalls` in the string isn't a match.
    assert_eq!(in_lua.len(), 1, "{hits:?}");
    assert!(matches!(in_lua[0].spot, Spot::Line { line: 1, .. }));
    assert!(!in_blocks.is_empty(), "{hits:?}");
    assert!(
        in_blocks.iter().any(|hit| hit.spot == Spot::Block(Some(5))),
        "{in_blocks:?}"
    );

    // Results go to their line or block.
    app.go_to_hit(in_blocks[0]);
    assert_eq!(app.tabs.active_path(), Some(BLOCKS));
    let highlighted = app
        .project
        .as_ref()
        .unwrap()
        .block_scripts
        .iter()
        .find(|document| document.path == BLOCKS)
        .unwrap()
        .highlighted;
    assert_eq!(
        Some(highlighted),
        Some(match in_blocks[0].spot {
            Spot::Block(block) => block,
            Spot::Line { .. } => None,
        })
    );
    app.go_to_hit(in_lua[0]);
    assert_eq!(app.tabs.active_path(), Some(LUA));
    assert_eq!(app.tabs.get(LUA).unwrap().go_to_line(), Some(1));

    // Replace All changes both, on disk and in the editor.
    app.find.replacement = "RedBall".to_owned();
    app.replace_all_in_scripts();
    let lua = fs::read_to_string(folder.join(LUA)).unwrap();
    assert!(
        lua.contains("workspace.RedBall") && lua.contains("BlueBalls"),
        "{lua}"
    );
    let blocks = fs::read_to_string(folder.join(BLOCKS)).unwrap();
    assert!(
        blocks.contains("\"RedBall\"") && !blocks.contains("\"BlueBall\""),
        "{blocks}"
    );
    assert!(serde_json::from_str::<BlockScript>(&blocks).is_ok());
    assert!(block_text(&app).contains("RedBall"));
    assert_eq!(app.tabs.get(LUA).unwrap().text, lua);
    assert!(!app.tabs.get(LUA).unwrap().unsaved());
    let block_lua = &app
        .project
        .as_ref()
        .unwrap()
        .scripts
        .iter()
        .find(|script| script.path == BLOCKS)
        .unwrap()
        .source;
    assert!(block_lua.contains("RedBall"), "{block_lua}");
    assert!(app.find.results.as_ref().unwrap().is_empty());
    assert!(
        app.history
            .undo_label()
            .is_some_and(|label| label.starts_with("Replace") && label.contains("BlueBall"))
    );

    // One undo puts both back, files and editor.
    app.undo();
    assert_eq!(fs::read(folder.join(LUA)).unwrap(), lua_before);
    assert_eq!(fs::read(folder.join(BLOCKS)).unwrap(), blocks_before);
    assert!(!block_text(&app).contains("RedBall"));
    assert!(
        app.tabs
            .get(LUA)
            .unwrap()
            .text
            .contains("workspace.BlueBall")
    );
    assert!(
        app.history
            .undo_label()
            .is_none_or(|label| !label.starts_with("Replace"))
    );
    // And redo does it again.
    app.redo();
    assert!(
        fs::read_to_string(folder.join(LUA))
            .unwrap()
            .contains("RedBall")
    );
    assert!(block_text(&app).contains("RedBall"));
    let _ = fs::remove_dir_all(&folder);
}

#[test]
fn find_in_scripts_searches_unsaved_code_in_a_tab() {
    let (mut app, folder) = project("find-unsaved");
    app.open_script(LUA, None);
    app.tabs
        .get_mut(LUA)
        .unwrap()
        .text
        .push_str("-- GreenBall\n");
    app.find.query = "greenball".to_owned();
    app.search_scripts();
    let hits = app.find.results.clone().unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].path, LUA);
    let _ = fs::remove_dir_all(&folder);
}
