# editor

The `axcenity-editor` binary: an eframe 0.32 (egui) app. Phase 8 only shows a project; no editing, no Play yet. Design notes: `docs/architecture.md` (Editor).

- `main.rs`: optional project folder argument, then `eframe::run_native` with the wgpu renderer (the viewport needs eframe's wgpu device).
- `app.rs`: `EditorApp` holds the `World` (empty when nothing is open), the panels and the open project. `open(folder)` loads, or puts the error in Output and keeps the current project. Panel order in `update` decides the layout: menu, right Explorer, bottom Output, central viewport.
- `explorer.rs`: `Explorer` keeps the set of open `InstanceId`s; `rows(world)` lists visible rows without UI, so tests check the tree. Clicks are collected during the loop and applied after it.
- `output.rs`: `Output` lines (`Info`, `Error`), also sent to `log`.
- `viewport.rs`: `Viewport` draws with `render::TextureRenderer` into a texture registered with egui (`register_native_texture`, then `update_egui_texture_from_wgpu_texture` after a resize). Camera input comes from egui: raw `pointer.motion()` for look and orbit, `drag_delta` for pan, physical keys from `Event::Key`. Shift+wheel or wheel-while-looking changes fly speed (`raw_scroll_delta.x + .y`, since egui turns Shift+wheel sideways); otherwise the smoothed wheel zooms. `speed_changed_at` drives the fading pop-up.
- **Deadlock trap:** `ui.input(|i| ...)` / `ctx.input` hold egui's context lock while the closure runs. Never call `Response` or `Context` methods inside it (`drag_delta`, `hovered`, `ctx.input`...); read them before or after. It froze every drag in Phase 8.
- egui only repaints on input, so anything animated (held movement keys) must call `ctx.request_repaint()`.
- eframe, egui-wgpu and `render` must use the same wgpu major (one `wgpu` in Cargo.lock).
- Native textures for egui must be `Rgba8Unorm` with sRGB-encoded bytes; `TextureRenderer` handles this.
