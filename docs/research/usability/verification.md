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
