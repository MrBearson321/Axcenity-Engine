# `player`

- Runtime binary shipped with games. Frame order: `docs/architecture/frame-loop.md`. Crate rules: `docs/architecture/crates.md`. For script behavior, open one file from `docs/scripting.md`.
- Never add dependencies on `editor`, `egui` or `blocks` (block scripts and graphs arrive translated through `project` and `blocks-runtime`). Preserve the ordered frame steps and report loop errors through the existing error path.
- Keep platform-specific behavior guarded; builds target Windows, macOS and Linux.
