# Usability changes: verification

What each usability PR changed, how it was checked, and what's left. The plan and IDs (U1, AN2…) are in [usability-audit.md](usability-audit.md).

All checks ran locally in the Linux cloud container (GitHub Actions is off on purpose; see the development policy). GPU drawing ran on lavapipe under `xvfb-run`.

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
