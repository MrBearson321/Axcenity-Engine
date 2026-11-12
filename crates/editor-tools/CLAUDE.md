# `editor-tools`

- What the editor shares with its panel crates: the viewport tools (gizmo, snapping, the shortcut table, Attachment poses, rig posing, the posed-model preview, the Object Viewer `object_viewer.rs` in `docs/editor/assets.md`) and small editor types and rules (`edit`, `asset_entry`, `props`, `tree`, `selection`, `animatable`). Open `docs/editor/transforms.md` (gizmo, snapping), `docs/editor/shell.md` (shortcuts) or `docs/editor/tree.md` (tree rules).
- The editor re-exports these under their old paths (`crate::gizmo`, `crate::tree`, `properties::Edit`, `animation::posing`...), so moves stay pure.
- Must not depend on the editor crate or on any panel crate.
