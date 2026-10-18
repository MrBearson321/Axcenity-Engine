# `player`

- Runtime binary shipped with games. See `docs/architecture.md` for crate boundaries/frame order and `docs/scripting.md` for runtime behavior.
- Never add dependencies on `editor`, `egui` or `blocks`. Preserve the ordered frame steps and report loop errors through the existing error path.
- Keep platform-specific behavior guarded; builds target Windows, macOS and Linux.
