# Usability changes: verification

What each usability PR changed, how it was checked, and what's left. The plan and IDs (U1, AN2…) are in [usability-audit.md](usability-audit.md).

All checks ran locally in the Linux cloud container (GitHub Actions is off on purpose; see the development policy). GPU drawing ran on lavapipe under `xvfb-run`.

## Summary (all PRs)

The plan's U1–U19 shipped in seven PRs, all merged. U20, the dedicated passes, is the only item left. No feature was removed or renamed. Everything rarely used is still one click or one filter away.

| PR | Plan items | Findings fixed | Where |
|---|---|---|---|
| #31 | U1–U4 | AN1–AN4 | Animation Studio: Record is unmissable, unkeyed moves are named with Key it (K), the right empty-dope-sheet hint, parameter chips that never break mid-word |
| #33 | U6–U9 | P1, P2, E2, N1 | Property descriptions on hover, Filter properties, Explorer Insert hover text, the Check Assets caption |
| #35 | U10, U11 | P3, E1 | ↺ reset to default on changed properties, the Explorer filter, a Humanoid Idle/Walk/Run/Jump/Fall test |
| #36 | U12–U14 | AN6, AN8 | Linked-clip settings are undo steps, "Try parameters", the Sequencer's keying and preview hints |
| #38 | U16, U17 | P5, N3 | Edit ▸ Undo History, ClassName row only when filtered |
| #39 | U18, U19 | N2, Terrain (A7) | Terrain tool names in a wide panel, one ribbon height across tabs |
| #40 | U15 | P4 | Advanced section for rare properties (Paul's approved list) |

**Workflows that changed.** These are the beginner flows the audit walked through. The per-PR tables below show each one step by step.
- **Make a door swing** (Animation): the editor now says whether a move will be kept.
- **Find and undo a change on a part**: Explorer filter, ↺ marks and one-click reset.
- **Find one setting**: Filter properties, hover descriptions, and rare settings out of the way under Advanced.
- **Undo several steps**: one click in Undo History.

**Not changed, on purpose.**
- **AN5:** the keying toolbar's terms already explain themselves on hover. Revisit only if they keep confusing people.
- **AN7** was left here as low-impact noise, then done in a follow-up: see "AN7: the outline and drawer say each thing once" below.
- **Guided tours, a second workspace system, renaming Roblox-familiar properties:** not planned (see the audit).

**How it was checked.**
- Each PR added tests for its change and ran `cargo test -p axcenity-editor` in the Linux cloud container. The suite grew from 902 to 916 passing tests.
- `every_tutorial_sample_opens_and_plays_in_the_editor` failed in every run. It also fails on `main` in this container, and none of these PRs touch it.
- Clippy (`-D warnings`) and fmt were clean on every PR.
- UI changes were checked by looking at snapshot PNGs (`AXCENITY_SNAPSHOTS`).
- Not checked here: Windows, or a real GPU instead of lavapipe. A short pass on Paul's PC over the flows above would cover that.

**Left: U20.** The UI Designer, the Block editor and Model Studio were only *reviewed* in the audit, not inspected screen by screen. Each deserves its own pass with the same method: inspect, snapshot, plan, small PRs.

## PR 1: Animation Studio (U1–U4)

**Changed**

| ID | Problem | Change |
|---|---|---|
| U1 (AN1) | Record was only a red word in the toolbar | While Record is on: a red frame round the preview, and a red line above the timeline, "Recording: moving something in the preview sets a key at 0.50 s" |
| U2 (AN2) | A move with Record off vanished silently when the playhead moved | An amber "Not keyed yet: moving the playhead or pressing play drops this change." with **Key it (K)**, for keyed clips and for bones in linked clips. Keying and dropping work exactly as before |
| U3 (AN3) | The empty dope sheet said "press K" even with Record on | With Record on it says "select something in the preview and move it. Record keys it." |
| U4 (AN4) | State machine parameter chips broke letter by letter ("Wav/e", "H/o/p") | A chip that doesn't fit the row starts the next row; names never wrap |

**Workflow checked: make a door swing (beginner)**

| Step | Before | After |
|---|---|---|
| Select the door, Animate | Clip opens with Record on; only the word Record is red | Same, plus a red frame round the preview and "Recording: … sets a key at 0.00 s" |
| Read the empty dope sheet | "move it, and press K" (wrong with Record on) | "move it. Record keys it." |
| Turn Record off to try a pose, move it | Moves; nothing says it isn't kept | "Not keyed yet…" with Key it (K) |
| Scrub the timeline | The try-out vanishes, no explanation | Same behaviour, but the note said so beforehand; Key it keeps it |

Steps and clicks are unchanged; what changed is that the editor now says which mode it's in and what will happen.

**Tests**

- New: `recording_and_unkeyed_moves_are_said_above_the_timeline` (`app/tests.rs`): Record's line and hint after Animate; with Record off a move shows the note and isn't keyed; clicking Key it (K) keys Position and the note goes; a later move is dropped when the playhead moves, and its note goes too.
- New: `parameter_chips_start_a_new_row_instead_of_breaking_their_names` (`animation/graph.rs`): three chips in the 132-point column, each one line and inside the column.
- `cargo test -p axcenity-editor`: 902 passed, 1 failed, 2 ignored. The failure is `every_tutorial_sample_opens_and_plays_in_the_editor`, which fails in this container on `main` too (noted in project memory); it doesn't touch the Animation tab.
- `cargo clippy -p axcenity-editor --all-targets -- -D warnings`: clean. `cargo fmt --all --check`: clean.
- Pictures looked at: the whole editor after Animate (red frame and Recording line), the tab with a move not keyed, and the Noodle state machine's parameter column, from `editor_tour_snapshots` and the new test's `animation-recording.png` / `animation-unkeyed.png`.

**Regressions looked for**: the Animation tab still fits 600×400, 260×900 and 1100×600 (`the_animation_tab_says_where_you_are_and_fits_any_shape`) and a 260-point column (`every_panel_fits_a_narrow_column_and_a_short_strip`); posing, IK, imported-clip and Sequencer tests pass unchanged (the Sequencer reuses the dope sheet and keeps the old hint, since it has no Record).

**Left for later**: U12 (undo for linked-clip settings), U13 (two panels both called Parameters), U14 (the same feedback in the Sequencer).

## PR 2: Properties and Explorer (U6–U9)

**Changed**

| ID | Problem | Change |
|---|---|---|
| U6 (P1) | Property names explained nothing | Hovering a name shows what it does, from the descriptions scripts' autocomplete uses (`axcenity_script::property_description`) |
| U7 (P2) | No way to find one property among dozens | **Filter properties** under the header: case and spaces ignored, section titles and attributes match too, sections held open, "No properties match" when none do |
| U8 (E1) | The Explorer's Insert menu didn't explain its items, the ribbon's did | Same hover text as the ribbon (`explorer::insert_hint`) |
| U9 (N1) | Check Assets was an unlabelled magnifier | Captioned "Check Assets" |

**Tests**: `common_properties_say_what_they_do`, `the_filter_ignores_case_and_spaces`, `filter_properties_shows_only_matching_rows` (with `properties-filter.png` and `properties-description.png`). `cargo test -p axcenity-editor`: 903 passed, with only the known cloud failure; `-p axcenity-script`: all passed; clippy and fmt clean.

## PR 3: Reset to default, Explorer filter, Humanoid animation test (U10, U11)

**Changed**

| ID | Problem | Change |
|---|---|---|
| U10 (P3) | No way to tell a changed value from a default, and no reset | A changed property has ↺ before its name; hovering says the default, clicking sets it back as one undo step. Not during Play, not on locked Instances (except `Locked`), not on Instance references |
| U11 (E2) | A big tree had no filter | **Filter** at the top of the Explorer: matching names or classes with the branches above them open and dimmed; clearing it restores the branches as they were |
| — | Paul asked whether character animation works; nothing tested a Humanoid's Idle/Walk/Run switching | `a_humanoid_plays_its_animators_idle_walk_run_jump_and_fall_states`: an NPC walks, runs and jumps, and its Animator goes Idle → Walk → Idle → Run → Idle → Jump → Fall → Idle |

**Workflow checked: find and undo a change on a part (beginner)**

| Step | Before | After |
|---|---|---|
| Find the part in a big Workspace | Open branches one by one | Type part of its name in Filter |
| See what was changed on it | Remember the defaults | ↺ marks each changed property |
| Put one back | Look up the default, type it | Click ↺ (hover first to see the default) |

**Tests**: `the_filter_shows_what_matches_and_the_branches_above_it`, `clicking_an_arrow_while_filtering_leaves_the_branches_alone`, `snapshot_of_the_filter` (`explorer-filter.png`), `a_changed_property_resets_to_its_default_with_one_click` (`properties-reset.png`), `defaults_are_said_briefly`, and the Humanoid test above. Full-suite results are in the PR.

**Left for later**: U12–U14 (PR 4), then U15–U20.

## PR 4: Animation follow-ups (U12–U14)

**Changed**

| ID | Problem | Change |
|---|---|---|
| U12 (AN6) | A linked (take) clip's settings (range, loop, speed, events) weren't undo steps; only Revert put the file back | Each write is one undo step ("Edit Idle"), like a keyed clip's; undo and redo reopen the clip as the file has it. Revert still goes back to the file as opened |
| U13 (AN8) | Two panels in the state-machine view were both called **Parameters** | The live values under the preview are now **Try parameters**, with a hover that says they only change the preview |
| U14 | The Sequencer reused the dope sheet's "select something in the preview, move it, and press K", but its preview has no tools | While Preview is on, a line above the keys says "Previewing: … Turn Preview off to move things, then key them (K)"; the empty dope sheet says "with Preview off, select something in the Scene view, move it, and press K. Key camera keys the view." |

The Sequencer has no Record and nothing is dropped when the playhead moves (moves happen in the edited world), so the Animation tab's amber "Not keyed yet" note doesn't apply there; the problem it had was saying the wrong next step.

**Tests**: `a_linked_clips_range_change_is_one_undo_step` (posing tests: range change written, undo label "Edit Idle", undo restores the file and the open clip, redo puts the change back), `the_sequencer_says_to_turn_the_preview_off_to_move_things`. Full-suite results are in the PR.

## PR 5: Undo History and the ClassName row (U16, U17)

**Changed**

| ID | Problem | Change |
|---|---|---|
| U17 | Undo named only the next step; going back several meant pressing Ctrl+Z and watching | **Edit ▸ Undo History** lists the steps newest first (25 of each), the undone ones dimmed above; clicking one undoes back to before it, or redoes up to it, one step at a time as Ctrl+Z/Ctrl+Y do |
| U16 | Properties showed the class twice: in the header and as a ClassName row | The row only shows when Filter properties asks for it ("class"); the header still says the class |

**Tests**: `undo_history_jumps_back_and_forward_several_steps` (three renames undone in one click from the drawn menu, redo of two, a jump past the end stops), `filter_properties_shows_only_matching_rows` (no ClassName row unfiltered, one for "class"), and the palette coverage test with Undo History as a submenu. `cargo test -p axcenity-editor`: 913 passed, only the known cloud failure.

## PR 6: Terrain tool names and a steady ribbon (U18, U19)

**Changed**

| ID | Problem | Change |
|---|---|---|
| U18 | Terrain Studio's Sculpt, Materials and Terrain rows were icon-only; a beginner had to hover each one | From 400 points wide, each tool is named beside its icon (the picked one filled), wrapping beside its row's title; narrower panels keep the icon-only rows, so a 260-point column still fits |
| U19 (N2) | The ribbon was 119 points tall on Home, 105 on Test and 101 elsewhere, so the layout jumped when switching tabs | Every tab keeps the tallest tab's height |

**Tests**: `a_wide_panel_names_its_tools` (names at 600 points, none at 260), `terrain-wide.png` from `the_panel_snapshots_with_its_main_tools`, `the_ribbon_keeps_one_height_across_tabs` (every built-in tab, then World again, the same height). `cargo test -p axcenity-editor`: 916 passed, only the known cloud failure.

## PR 7: Advanced section in Properties (U15)

**Changed**

| ID | Problem | Change |
|---|---|---|
| U15 / P4 | Rare properties sat beside common ones (Appearance showed MaterialVariant and BlueprintTexture between Material and Metalness) | Paul approved a short per-class list (`properties::is_advanced`). Those properties now sit in one closed **Advanced** section after the others. Its header counts the ones that differ from their defaults ("Advanced · 1 changed"), and Filter properties still finds them. Nothing is removed. A part's panel loses 12 rows by default. |

**Tests**: `rare_settings_fold_under_advanced_and_say_how_many_changed` checks four things:
- Rare rows are hidden and common ones stay.
- The header counts a changed Massless.
- Filtering "mass" shows Massless.
- Brightness stays visible on a light but folds on an emitter. Controllers and TextLabel are checked too.

The snapshot `properties-advanced.png` shows a part's panel: Data, Appearance (6 rows), Behavior (4), Part, then a closed "Advanced · 1 changed" header, then Attributes and Tags. `cargo test -p axcenity-editor`: 916 passed, only the known cloud failure.

## Model Studio PR 1: modes and toolbar (U20, MS1–MS3)

Findings and plan: [model-studio.md](model-studio.md).

**Changed**

| ID | Problem | Change |
|---|---|---|
| MS1 | Paint, Sculpt and Rig showed every modeling control, most doing nothing there; the toolbar wrapped to two rows | The handles, Local axes, Show normals, Mesh menu, proportional editing and pivot show in Object and Edit mode only; the UV menu and UV editor in Object, Edit and Paint; Rig keeps the 3D cursor menu (Add Bone uses it). Undo and redo always show. The handles are off in the view outside Object and Edit too |
| MS2 | Seven modes in one switch, Weights and Pose beside Paint | Five modes (Object, Edit, Paint, Sculpt, Rig), each explained on hover. Rig shows a second switch, Bones, Weights and Pose, as Edit shows Vertex, Edge and Face |
| MS3 | Pose without bones ignored the click; Weights without bones drew a stray cube in place of the model | A mode that can't start stays put and says why beside the switch ("Add bones first: in Bones, click Add Humanoid or Add Bone."; "Pick an object with a mesh first"). Weights now needs bones, and the view never takes over without a skinned model to show |

**Tests**: `the_toolbar_fits_the_mode_and_rig_steps_say_why_they_wait` draws the tab without a GPU. Object mode shows Mesh and UV, and no Weights or Pose. Paint keeps UV but not Mesh, and Sculpt has neither. In Rig, a click on Weights with no bones stays in Bones and shows the note; after Add Humanoid the same click goes in and the note goes. `modes_that_need_a_mesh_or_bones_say_so` covers the rules, a group included. The tour (`TOUR_ONLY=studio`) has seven new states (20–26); `tour-23-studio-paint.png` and `tour-25-studio-rig.png` show one-row toolbars, and `tour-26-studio-weights.png` the rigged humanoid with Rig › Weights lit.

## Block editor PR 1: undo, search and the Lua blocks (U20, BL1–BL6, BL8)

Findings and plan: [block-editor.md](block-editor.md).

**Changed**

| ID | Problem | Change |
|---|---|---|
| BL1, BL2 | No undo in Blocks or Graph; Ctrl+Z over blocks undid a world edit out of sight, and Delete and Ctrl+D hit the part selected in 3D (so the Graph view's own Delete and Ctrl+D never worked in the editor) | Each block script keeps its own undo (100 steps); over its tab Ctrl+Z and Ctrl+Y are its, a drag or a typed number is one step, and the world's Delete, Ctrl+D, F2 and tool keys are left alone |
| BL3, BL8 | No palette search; the node menu searched titles only | **Search blocks** above the palette (words, category or Lua); the node menu matches the same way |
| BL4 | Palette blocks said nothing on hover | Hover says how to use it and shows the Lua it becomes; node menu items too |
| BL5 | Luau-language blocks beside the beginner ones (three ways to set a variable) | Under a closed **Lua blocks** heading in each category; search and the node menu still find them |
| BL6 | A loose stack silently didn't run | "Not under an event block, so it doesn't run" above it |

**Tests**: `ctrl_z_over_blocks_undoes_the_blocks_not_the_world`, `delete_and_ctrl_d_over_blocks_leave_the_selected_part_alone`, `an_edit_over_several_frames_is_one_step`, `undo_stays_in_the_view_shown`, `the_palette_search_finds_blocks_by_their_words_and_their_lua`, `searching_the_palette_shows_the_matches_and_they_drag_out`, `hovering_a_palette_block_says_the_lua_it_becomes`, `luau_blocks_fold_under_their_own_heading_but_stay_in_the_graph_menu`. Snapshots: `palette-search.png`, `palette-hover.png`, `palette-lua-blocks.png`. `cargo test -p axcenity-editor` (on `main` before #42): 926 passed, only the known cloud failure (`every_tutorial_sample_opens_and_plays_in_the_editor`); clippy and fmt clean.

## Model Studio PR 2: explain and fold (U20, MS4–MS8)

**Changed**

| ID | Problem | Change |
|---|---|---|
| MS4 | Retopology and Bake (about 15 controls) filled the side panel in Object and Edit mode | One **Retopology and Bake** fold, closed until it's in use (a reference set, Poly Build on, or a bake's report showing), when it opens by itself |
| MS5 | The Mesh, UV and Add Modifier menus named operations without explaining them; the Mesh menu was greyed in Object mode with no reason | Every item explains itself on hover in a sentence, enabled or not. The Mesh menu in Object mode says at its top that its operations work on what's picked in Edit mode (shading still works) |
| MS6 | Painting a New cube painted all six sides at once, with no hint why | Paint warns when the active object's faces share texture space (or it has no uvs), with a **Smart UV Project** button beside the warning (one undo step) |
| MS7 | Edit mode's hint ended in a row of bare keys | It names E, I and Ctrl+B and points at the Mesh menu for the rest |
| MS8 | No ribbon button for the Model Studio | **Model Studio** on the Model tab, in the group now called Models (Import… and Model Studio), and in the palette |

**Tests**: `retopology_and_bake_stay_folded_until_used` (closed at first, a click opens and closes it, a reference opens it), `the_mesh_menu_says_why_its_operations_are_greyed` (the note in Object mode, shading and Bridge Edge Loops listed, no note in Edit mode), `paint_warns_when_faces_share_texture_space_and_one_click_fixes_it` (a New cube warns; a click on Smart UV Project clears it as one undo step and stays in Paint; undo brings it back; an object with no uvs says so), and `every_ribbon_and_menu_command_can_be_run_from_the_palette` with the new button. Tour shots: `tour-21-studio-object.png` (the closed fold), `tour-27-studio-paint-new-cube.png` (the warning and its button), `tour-03-ribbon-Model.png` (the Models group).


## AN7: the outline and drawer say each thing once

Follow-up to the Animation PRs, for the one audit finding they left (A2, AN7).

| Workflow | Before | After |
|---|---|---|
| Read a character's state machine in the outline | Under every state, "🔗 Linked to noodle.fbx · Armature|Wave", the same file each time | "🔗 Wave": the take that state plays. The whole badge is on hover, and in full in the clip's own toolbar |
| Pick what to key in a property clip's drawer | The Animator and its Animation listed as **Objects** beside the parts that move | Only what the clip can move; an Animator or Animation shows only if a track keys it |

**Why**: Unity's Animator window and Godot's AnimationTree name states by their motion and keep the source asset in the inspector, and Unity's Animation window lists only animatable children **[R§1]**. Saying the file once leaves the take, the part that differs between rows, readable in a narrow outline.

**Tests**: `the_outline_names_a_linked_clip_by_its_take_alone` (`animation.rs`), `the_drawer_lists_what_the_clip_moves_not_its_animator` (`app/anim_drawer_tests.rs`; fails without the change). Tour shot 11 checked.

## UI Designer PR 1: beginner-first inspector (U20, UD1–UD4)

Findings and plan: [ui-designer.md](ui-designer.md).

**Changed**

| ID | Problem | Change |
|---|---|---|
| UD1 | The inspector was one flat list of up to 14 always-open sections; a button's Styles, anchor and **When Activated** came last, under States, Bevel and Slice | Every section folds. Styles, Anchor, Text, Widget, Image, Look and Code & animation come first, open; States, Bevel and Slice start folded, and a folded header names what's set (**Bevel · Raised**). Folded, States edits nothing: the text toolbar edits the Normal look |
| UD2 | Shadow's blur field drew over Letter spacing in a two-column inspector | Its fields on two fixed lines |
| UD3, UD4 | "Background 100%" for an invisible background; stray spaces in Reset's hover text | "Transparency: 0% is solid, 100% is invisible." above the sliders; the hover text fixed |

**Tests**: `the_inspector_lists_everyday_sections_first_and_the_fine_tuning_last`, `a_folded_section_opens_on_its_header_stays_open_and_names_what_it_has`, `with_states_folded_the_text_toolbar_edits_the_normal_look`; the States, Bevel and Slice tests open their section first. Tour shots 40–50 (`TOUR_ONLY=ui-`). `cargo test -p axcenity-editor`: only the known cloud failure (`every_tutorial_sample_opens_and_plays_in_the_editor`); clippy and fmt clean.

## UI Designer PR 2: right-click menus and the empty tab (U20, UD5–UD8)

Merged as #48. Findings and plan: [ui-designer.md](ui-designer.md).

| ID | Problem | Change |
|---|---|---|
| UD5 | Right-click menus had no Rename or Delete; Delete was a key the designer never mentioned | **Rename** (the inspector's Name, all selected) and **Delete** (the Delete key's action, one undo step) on the artboard's and the Hierarchy's menus, with the Explorer's icons and shortcut text |
| UD6 | "There's no ScreenGui to design yet." didn't say what a ScreenGui is | A heading and "Lay out the game's 2D interface here … It lives in a ScreenGui, a layer drawn over the game" above **New ScreenGui** |
| UD7 | Fixed · Fit · Scale had no hover text | Each says what it does |
| UD8 | "No CSS styles this element yet" | "No CSS rule styles this element yet" |

**Tests**: `delete_on_the_right_click_menu_deletes_as_one_undo_step`, `rename_on_the_right_click_menu_types_over_the_name_in_the_inspector`, `the_empty_designer_says_what_a_screen_gui_is_and_makes_one` (`app/ui_designer_menu_tests.rs`). Snapshots of the empty tab and the menu. `cargo test -p axcenity-editor`: only the known cloud failure; clippy and fmt clean.

## UI Designer PR 3: move into frames from the Hierarchy, X/Y/W/H (U20, UD9–UD10)

The two items the UI Designer pass left for later. Findings and plan: [ui-designer.md](ui-designer.md).

| ID | Problem | Change |
|---|---|---|
| UD9 | An element couldn't be moved into another Frame from the designer; the Explorer could, but the element then jumped, as its `Position` counted from the new parent's corner | Drag Hierarchy rows onto a container's row (or any row, for the container it's in): they move in and stay where they are on screen, keeping their anchors. The target row is outlined; a drop that can't go there shows the no-drop cursor. One undo step |
| UD10 | No X, Y, width and height in design pixels | **X**, **Y**, **W**, **H** at the top of **Position & anchor**; a flex child has only W and H; a CSS-set size is greyed with the rule named |

**Tests**: `app/ui_designer_move_tests.rs` (three tests, see ui-designer.md). Tour shot 47 checked. `cargo test -p axcenity-editor`: 943 passed, only the known cloud failure (`every_tutorial_sample_opens_and_plays_in_the_editor`); clippy and fmt clean. Not checked on Windows or a real GPU.

