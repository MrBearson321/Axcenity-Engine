# AutoRecover

Unsaved work survives an editor crash or a power cut: every few minutes the editor copies what's unsaved into the project's `.axcenity/autorecover/`, and opening the project afterwards offers it back. Code: `crates/project/src/recovery.rs` (the snapshot: writing, reading, discarding) and `crates/editor/src/app/autorecover.rs` (when to write, the prompt, recovering).

## The snapshot

- `.axcenity/autorecover/` holds `recovery.json` (`RecoveryInfo`: when, the open level, the engine version, whether it has the world, the documents by path), `world.json` (the edit world as `write_world` writes it, with the open level's Workspace) when the world has unsaved changes, and `files/<project path>` for each document with unsaved text: script and stylesheet tabs, and block scripts (as their JSON).
- It's under `.axcenity/`, so Back Up Project, asset packages, templates and built games leave it out.
- **Crash and power-cut safe.** A snapshot is written whole into `.axcenity/autorecover.writing/`, each file flushed to the disk (`sync_all`), `recovery.json` last; then the current one is renamed to `autorecover.previous` and the new one into place, and the previous removed. A folder without `recovery.json` is a snapshot that didn't finish and is ignored; if the editor stops between the renames, `autorecover.previous` is read instead.
- Paths in a snapshot are only ever relative paths inside the project (no `..`, nothing absolute), both when writing and when reading.

## When it's written

- **Settings > Saving > Keep a copy of unsaved work**: Off, every 1, 2 (the default), 5 or 10 minutes. It's the user's, saved with the editor's settings (key `autorecover minutes`).
- Each interval, if anything is unsaved and it changed since the last snapshot (the undo position and the documents' text, hashed), the world is copied and written on a thread, so the frame doesn't wait for the disk. With nothing unsaved, the snapshot is removed.
- Save removes it (once the save succeeds), and so do "Don't Save", closing the project and quitting after saving or letting changes go. So a snapshot that's still there when a project opens is work that was lost.
- An editor with the project open marks its snapshot as in use every 20 seconds (the file's modified time, `touch_recovery`). Only a snapshot nobody touched for 90 seconds (`RECOVERY_STALE_AFTER`) counts as left behind, so a second editor opening the same project doesn't offer the first one's live work.
- In a Collaborate session nothing is written: the session's journal recovers it (`docs/research/collaborate.md`).

## Recovering

- Opening a project with a left-behind snapshot asks **Recover unsaved work?**, listing the world (and its level) and the documents. Escape or a click outside asks again; nothing is written over the snapshot until it's answered.
- **Recover** reads the project as it is on disk with the snapshot's world in place of the saved one (`load_recovered`, which loads like `load` from there: prefabs, terrains, scripts and UI graphs from the files), opens it in the snapshot's level, marks the world unsaved, and puts each document's recovered text into its tab (block scripts into their blocks) as unsaved changes. Nothing on disk changes until Save. **Discard** deletes the snapshot.

## Not kept yet

UI Event Graphs, behavior trees, dialogues, data tables, materials, the string table, and terrain sculpting since the last save. Settings > Saving says so. The snapshot's world doesn't hold terrain heightfields; the terrains come back as last saved.

## Tests

- `crates/project/src/recovery.rs`: a snapshot brings back an unsaved part and script text without touching the saved files; a newer snapshot replaces the last, a half-written one is ignored and the previous is read after a cut between the renames; an empty snapshot removes it and paths stay inside; only a snapshot nobody touched lately is left behind; a snapshot taken in a level comes back in it.
- `crates/editor/src/app/autorecover_tests.rs`: an unsaved edit is written after the interval and Save removes it; a left-behind snapshot is offered on open and Recover brings the edit and a script tab's text back as unsaved; Discard deletes it; with AutoRecover off nothing is written.
