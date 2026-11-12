# `editor-tools`

- The viewport tools shared by the editor and its studios: gizmo, snapping, the shortcut table, Attachment poses, rig posing and the posed-model preview. Open `docs/editor/building.md` (gizmo, snapping) or `docs/editor/shell.md` (shortcuts).
- The editor re-exports these modules under their old paths (`crate::gizmo`, `crate::shortcuts`, `animation::posing`...), so moves stay pure.
- Must not depend on the editor crate or on any studio.
