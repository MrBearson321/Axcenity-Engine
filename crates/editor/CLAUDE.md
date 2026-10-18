# editor

The `axcenity-editor` binary: an eframe 0.32 (egui) app. Phase 10: select parts, move/scale/rotate them with handles, edit properties, undo/redo, save; no Play yet. Design notes: `docs/architecture.md` (Editor).

- `main.rs`: optional project folder argument, then `eframe::run_native` with the wgpu renderer (the viewport needs eframe's wgpu device).
- `app.rs`: `EditorApp` holds the `World`, project, `selected`, `history`, `tool`, `snap` and the panels. **Every world change goes through `change(id, key, label, edits)`** (from `apply` for Properties, `apply_gizmo` for handles), which records it in `history`. Unsaved = `!history.is_saved()`. `shortcuts(ctx)` reads undo/redo/tool keys and is testable with a bare `egui::Context`. Panel order in `update`: menu, toolbar, right side (Properties under Explorer), Output, viewport; at the end the open undo step is sealed unless a pointer or text box is busy.
- `history.rs`: `History` of `Step`s (before/after values per `Field`, from `snapshot`). `record` merges into the open step for the same Instance and field; `seal` ends the gesture. `mark_saved`/`is_saved` compare step numbers, so saving never clears undo.
- `gizmo.rs`: `Tool`, `Snap`, `Handle`, `Pose`; `shapes` (handles in screen points), `hovered`, `paint`; `Drag::start/update` turns mouse rays into new property values. Pure math, tested with rays.
- `placing.rs`: `PlaceDrag::start(world, part, grabbed)` / `update(world, ray, snap)` → new Position resting on the surface under the mouse (world-grid snapping). Pure math, tested with rays.
- `screen.rs`: `Screen` = camera + viewport rect: `ray(pos)`, `line(a, b)`, `studs_per_point(at)`.
- `viewport.rs`: draws with `render::TextureRenderer`, camera input, picking, selection box, handles. `drag: Option<Dragging>` is a handle (`Drag`, grabbed on press) or a body (`PlaceDrag`, only after egui's drag threshold, so clicks still select). Returns `ViewportOutput { clicked, drag: DragEdit, drag_released }`; the app applies edits after it draws (`apply_drag`), then requests a repaint.
- `explorer.rs`, `properties.rs` (`show` returns an `Edit`, never mutates; `MIN_SIZE`), `output.rs`.
- **Deadlock trap:** `ui.input(|i| ...)` / `ctx.input` hold egui's context lock while the closure runs. Never call `Response` or `Context` methods inside it.
- egui's `consume_shortcut(Ctrl+Z)` also matches Ctrl+Shift+Z: consume the Shift version first.
- A `TopBottomPanel` sizes itself to its first frame's content: keep a `ScrollArea::auto_shrink(false)` in it even when empty. egui only repaints on input, so animations must `request_repaint()`.
- eframe, egui-wgpu and `render` must use the same wgpu major. Native textures for egui must be `Rgba8Unorm` with sRGB-encoded bytes.
