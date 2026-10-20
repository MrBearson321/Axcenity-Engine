# `editor`

- eframe/egui editor. Open one file under `docs/editor/` (the list is in `docs/architecture.md`). Play mode: `docs/architecture/play-mode.md`.
- `app.rs` runs the frame. Open and Play: `app/session.rs`. Selection, tools and undo: `app/edits.rs`. Insert, delete and reparent: `app/tree_ops.rs`. Menus and panels: `app/chrome.rs`. Files: `app/files.rs`. Asset drops: `app/assets.rs`. Animation panel: `animation.rs` (and `animation/`); animated FBX import: `anim_import.rs`.
- Route edit-mode world changes through `EditorApp::change` so undo/history stays consistent; Play edits target the session world and are discarded on Stop. Clip, controller and prefab create, rename, duplicate and delete go through `History::record_asset` (a missing file is `None`).
- Colours come from `theme::palette(ctx)` tokens and controls from `widgets.rs`; don't hardcode chrome colours. Drawing tests need `theme::test_context()`. Don't call egui `Context`/`Response` methods inside an `input` closure; keep eframe, egui and render on compatible wgpu majors.
- Drawing tests save PNGs of what they draw when `AXCENITY_SNAPSHOTS` names a folder (`snapshot.rs`); look at them after UI changes.
- egui fills only convex shapes: `block_editor.rs` fills blocks as convex pieces and outlines them as one path.
