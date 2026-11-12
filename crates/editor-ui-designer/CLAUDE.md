# `editor-ui-designer`

- The UI Designer tab (artboard, rulers, anchors, inspector, styles, states, pages, strings, Event Graph and Live Preview), the Properties panel's Style section (`style_panel`) and the viewport's UI handles (`ui_handles`). Open `docs/ui.md`.
- It never changes the world: it returns `DesignerAction`s, `StyleEdit`s and `Edit`s that the editor applies (`designer_actions`, `app/ui_selection.rs`), which imports this crate as `ui_designer`, `style_panel` and `ui_handles`.
- `crate::theme`, `crate::shortcuts`, `crate::tree` and the rest are the kit's, editor tools' and node graph's modules, re-exported in `lib.rs`. Must not depend on the editor crate.
- Test-only accessors are behind the `test-support` feature for the editor's tests; turn it on from `[dev-dependencies]` only.
