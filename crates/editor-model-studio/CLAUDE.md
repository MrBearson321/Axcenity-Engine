# `editor-model-studio`

- The Model Studio tab: modeling, modifiers, UVs, paint, sculpting, baking and rigging on `mesh_edit` documents (sculpting and baking came back in R15 after Phase 90 removed them). Open `docs/editor/model-studio.md`.
- Open and save run in the editor (`app/model_studio.rs`), which imports this crate as `model_studio`. Keep editor state out of here; return a `StudioAction` instead.
- `crate::theme`, `crate::gizmo` and the rest are the editor kit's and editor tools' modules, re-exported in `lib.rs`. Must not depend on the editor crate.
- The test modules' helpers are behind the `test-support` feature for the editor's tests; turn it on from `[dev-dependencies]` only.
