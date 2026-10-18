# `editor`

- eframe/egui editor. See `docs/architecture.md` (Editor, Play mode) and `docs/scripting.md` for canonical behavior.
- Route edit-mode world changes through `EditorApp::change` so undo/history stays consistent; Play edits target the session world and are discarded on Stop.
- Drawing tests need `theme::test_context()`. Don't call egui `Context`/`Response` methods inside an `input` closure; keep eframe, egui and render on compatible wgpu majors.
