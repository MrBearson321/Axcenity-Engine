# Block editor usability pass (U20)

The dedicated pass on the Block Coding Kit that the [usability audit](usability-audit.md) left for later (A13, U20): its Blocks view, its Graph view, and how they sit in the editor. Same method as the audit: pictures of the real editor, then the code behind each control. Research on other tools is cited as **[R§n]** from [engine-conventions.md](engine-conventions.md).

- **Date:** 2026-11-10, on `main` at `f7e19e3` (after PR #39).
- **Pictures:** `blocks-light.png`, `blocks-dark.png`, `graph-*.png`, `lua-panel.png` from the block editor's drawing tests (`AXCENITY_SNAPSHOTS=<folder> cargo test -p axcenity-editor block_editor`), plus each category of the palette scrolled into view.
- **Code read:** `block_editor.rs` and `block_editor/` (palette, canvas, painter, menus, Graph view and its node menu), the script tab (`app/chrome/panes.rs`), the editor's shortcuts (`app/chrome/windows.rs`) and undo (`app/edits.rs`).

## Verdict

The Block editor is in good shape for a beginner where it matters most. It follows Scratch closely: coloured categories down the left, blocks that snap, slot shapes that say what fits, a server/client badge, an empty canvas that names the first step, **View as Lua**, and errors outlined on the block that caused them. The Graph view has a searchable node menu and plain node titles. Nothing should be removed.

Its problems are about recovering from mistakes and finding things:

1. **No undo.** Neither view could undo. Worse, Ctrl+Z over the blocks undid the last *world* edit out of sight, and Delete and Ctrl+D acted on the part selected in the 3D view.
2. **Finding a block means knowing its category.** About 150 blocks in 11 categories, with no search, and a palette block said nothing about what it does.
3. **Three ways to set a variable, side by side.** Each category's Luau-language blocks (`local`, `name = …`, `while`, a `:Connect`) sat right under its beginner blocks, so Variables offered "set score to", "local name =" and "name =" together.

## Findings

Severity and frequency as in the audit.

| # | Problem | Severity | Frequency | Beginner impact | Advanced impact |
|---|---|---|---|---|---|
| BL1 | No undo or redo in Blocks or Graph. Dropping a stack on the palette, or Delete on its menu, lost it for good (except by closing without saving). | High | Often | A slip loses work; scared to tidy up | Same |
| BL2 | Ctrl+Z over a block script undid the last world edit, which wasn't on screen. Delete and Ctrl+D deleted or duplicated the part selected in the 3D view. Because the editor took those keys first, the Graph view's own Delete and Ctrl+D shortcuts (documented in `graphs.md`) never worked in the editor (Backspace still did). | High | Often | Invisible damage to the world | Graph shortcuts broken |
| BL3 | No search in the palette. "Play a sound" is under Sound, but "destroy" is under More, "raycast" under Sensing and "print" under More. | Medium | Constant | Scrolls every category | Slow |
| BL4 | A palette block said nothing on hover (the editor had 14 tooltips for 34 controls, the lowest of any big system). | Medium | Constant | Has to try a block to see what it does | None |
| BL5 | Luau-language blocks mixed with the beginner blocks in Events, Control and Variables. | Medium | Constant | Three similar "set" blocks; `local`, `while`, `for` beside "repeat 10 times" | None: they want them |
| BL6 | A loose stack (blocks under no event) looked like any other and silently didn't run. The Graph view greys such nodes and says why on hover; the Blocks view didn't. | Medium | Sometimes | "Why doesn't my block do anything?" | Minor |
| BL7 | Palette blocks wider than the palette are cut off on the right ("when prompt Part.ProximityPrompt i…"). | Low | Sometimes | The end of the label is hidden | None |
| BL8 | The Graph view's node menu searched node titles only, and its items had no hover text. | Low | Often | Must know Unreal-style titles | Minor |

**Keep:** Scratch's categories, colours and shapes (what `docs/scripting/blocks.md` models the editor on, and what many beginners already know); the server/client badge; dropping on the palette to delete (Scratch's convention, now undoable); View as Lua; the right-click Duplicate and Delete; the Graph view and its searchable menu; error outlines; the empty-canvas hint.

**Defer (fold, don't remove):** the Luau-language blocks (BL5). They're needed: switching a script from Code to Blocks makes them, and they write any Luau. They move under a closed **Lua blocks** heading in each category. Searching still finds them, and the Graph view's node menu still lists them.

**Not changing:** dragging to add a block (the convention in Scratch and MakeCode; a click-to-add would be a second way to do the same), the category names, block wording (each block's words match the Roblox-familiar Lua it writes), and BL7 for now (widening the palette costs canvas room in every script; the hover now shows the whole block's Lua).

## Plan

| ID | Change | Fixes | Complexity | Risk |
|---|---|---|---|---|
| U20-B1 | Undo and redo for block scripts, in both views: each script keeps its own 100 steps; while the mouse is over its tab, Ctrl+Z and Ctrl+Y are its (as in the Behavior Tree, Dialogue and Model Studio tabs, and as Godot keeps one history per scene **[R§4]**; undo, not dialogs, is the safety net **[R§9.5]**). An edit over several frames (a drag, typing a number) is one step. With nothing left to undo there, Ctrl+Z does nothing rather than reach the world. | BL1, BL2 | M | Low: snapshots of a small script; the file format is unchanged |
| U20-B2 | Over a block script's tab the editor leaves Delete, Ctrl+D, F2 and the tools' keys alone, so the Graph view gets its own Delete and Ctrl+D | BL2 | S | Low |
| U20-B3 | **Search blocks** box (search everywhere **[R§9.4]**) at the top of the palette: matches the words on a block, its category, or the Lua it becomes (so "Destroy" or "Raycast" works for someone who knows the API). Escape clears it; clicking a category shows that category again. The Graph view's node menu searches the same way. | BL3, BL8 | S | Low |
| U20-B4 | Hovering a palette block or reporter says how to use it and shows the Lua it becomes, coloured as in View as Lua; the Graph's node menu items do the same | BL4, BL8 | S | None |
| U20-B5 | Each category's Luau blocks under a closed **Lua blocks** heading that explains them on hover | BL5 | S | Low: only the palette's layout |
| U20-B6 | A loose stack has a quiet note above it: "Not under an event block, so it doesn't run" | BL6 | S | None |

All six ship in one PR: they touch the same two modules and are each small.

## Verification

**Workflow checked: tidy up a script and change your mind (beginner)**

| Step | Before | After |
|---|---|---|
| Drag a stack onto the palette to delete it | Gone | Gone |
| Press Ctrl+Z | The last *world* edit (say, a part's name) is undone, out of sight; the stack stays gone | The stack comes back, and its Lua with it; the world is untouched |
| Press Ctrl+Y | Redoes the world edit | The stack goes again |

**Workflow checked: find the block that destroys something (beginner)**

| Step | Before | After |
|---|---|---|
| Look for it | Click through categories until More | Type "destroy" in **Search blocks** |
| Check it's the right one | Drag it out and look at View as Lua | Hover it: "Drag it onto the canvas. It becomes this Lua: `Part:Destroy()`" |

**Tests**

- `ctrl_z_over_blocks_undoes_the_blocks_not_the_world` (`app/block_undo_tests.rs`): in a real project, drag "when game starts" from the palette onto the canvas in the script's tab, with a world edit made before. Ctrl+Z over the tab takes the stack away and out of the tab's Lua; the part keeps its new name. Ctrl+Y puts it back. Extra Ctrl+Z with nothing left still leaves the world alone. Away from the tab, Ctrl+Z undoes the world edit as before.
- `delete_and_ctrl_d_over_blocks_leave_the_selected_part_alone`: with a part selected, Ctrl+D and Delete over the tab change nothing in the world; away from it, Delete deletes the part.
- `an_edit_over_several_frames_is_one_step`, `undo_stays_in_the_view_shown`: a drag is one step, a drag back to the start is none, redo is forgotten after a new edit, and undo doesn't switch between Blocks and Graph.
- `the_palette_search_finds_blocks_by_their_words_and_their_lua`, `searching_the_palette_shows_the_matches_and_they_drag_out` (`palette-search.png`), `hovering_a_palette_block_says_the_lua_it_becomes` (`palette-hover.png`), `luau_blocks_fold_under_their_own_heading_but_stay_in_the_graph_menu` (`palette-lua-blocks.png`: Variables shows "set" and "change", then the opened heading and "local name = 0" dragged out).
- The existing drag test moved its palette positions down by the search box's height; every other block and graph test passes unchanged.

**Regressions looked for:** dropping, snapping, slot drops, View as Lua and every Graph view test pass; the node menu still opens filtered for a dropped wire; `every_block_draws_in_both_themes_without_changing_the_script` still draws the whole sample unchanged.

**Left for later:** BL7 (cut-off palette blocks) if it comes up again; a Windows check on Paul's PC, as for the other usability PRs.
