# editor

The `axcenity-editor` binary: an eframe 0.32 (egui) app. Phase 9: select parts and edit their properties, then save; no undo or Play yet. Design notes: `docs/architecture.md` (Editor).

- `main.rs`: optional project folder argument, then `eframe::run_native` with the wgpu renderer (the viewport needs eframe's wgpu device).
- `app.rs`: `EditorApp` holds the `World`, the open project, `selected`, `unsaved`, and the panels. `apply(id, Edit)` is the only place edits reach the world (and set `unsaved`). `save()`, then `request(Discard)` / `answer(Answer)` drive the "Save changes?" modal without UI, so tests check them. Panel order in `update`: menu, right side (Properties at its bottom, Explorer above), bottom Output, central viewport. Edits apply before the viewport draws.
- `explorer.rs`: `Explorer` keeps the set of open `InstanceId`s; `rows(world)` lists visible rows without UI. `show` returns the clicked row; `reveal` opens ancestors and scrolls to a row picked in the viewport.
- `properties.rs`: `sections(instance)` groups properties (anything ungrouped lands in "Other"); `show` returns an `Edit` and never mutates. Widgets sit under `push_id(selected)` so half-typed values don't carry over to the next selection.
- `output.rs`: `Output` lines (`Info`, `Error`), also sent to `log`.
- `viewport.rs`: draws with `render::TextureRenderer` into a texture registered with egui. Camera input from egui (raw `pointer.motion()`, `drag_delta`, physical keys). A plain left click returns `Clicked { part }` via `scene::pick`; the selection box is egui lines from `scene::part_box_edges` + `CameraView::project_line`.
- **Deadlock trap:** `ui.input(|i| ...)` / `ctx.input` hold egui's context lock while the closure runs. Never call `Response` or `Context` methods inside it; read them before or after.
- A `TopBottomPanel` sizes itself to its first frame's content: keep a `ScrollArea::auto_shrink(false)` in it even when empty, or it stays tiny.
- egui only repaints on input, so anything animated must call `ctx.request_repaint()`.
- eframe, egui-wgpu and `render` must use the same wgpu major (one `wgpu` in Cargo.lock). Native textures for egui must be `Rgba8Unorm` with sRGB-encoded bytes.
