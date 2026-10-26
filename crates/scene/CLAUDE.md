# `scene`

- Shared bridge from `core::World` to `render::DrawList`, plus camera and picking. Draw-list rules: `docs/rendering/pipeline.md`.
- Keep picking consistent with rendered placement (MeshParts pick by their box), and preserve the shared orientation convention (degrees, YXZ).
- `Particles` is host-owned state (player, editor viewport), never stored in the world; see `docs/rendering/particles.md`. So are `Poses` (from `anim::Animators`), drawn by `build_draw_list_posed`.
- Depend only on `core`, `assets`, `render`, `terrain` and `ui` (UI layout into the draw list); input/window handling belongs to `player` or `editor`.
- `TerrainCache` is host-owned too: it keeps chunk meshes (same key, new version per edit) and weight textures between frames, and draws painted foliage on the surface; see `docs/terrain.md`.
