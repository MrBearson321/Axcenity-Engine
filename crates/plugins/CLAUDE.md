# `plugins`

- The editor's Lua plugins (Phase 54): `manifest.rs` (`plugin.json`, `find_plugins`), `files.rs` (project-only file access), `api.rs` (the Lua API: `plugin`, toolbars, dock widgets, `Selection`, `ChangeHistoryService`, the mouse, settings) and `Plugin` in `lib.rs`, over `script::EditRuntime`. Behavior: `docs/editor/plugins.md`.
- Editor-only: the player must never depend on this crate (CI checks). Undo, the ribbon and dock tabs are the editor's (`app/plugins.rs`).
- Keep `definitions/axcenity-plugin.d.luau` in step with `api.rs`; a test checks every method and field is named there.
