# `scene`

- Shared bridge from `core::World` to `render::DrawList`, plus camera and picking; see `docs/architecture.md` and `docs/rendering.md`.
- Keep picking consistent with rendered placement, and preserve the shared orientation convention (degrees, YXZ).
- Depend only on `core` and `render`; input/window handling belongs to `player` or `editor`.
