# `scene`

- Shared bridge from `core::World` to `render::DrawList`, plus camera and picking; see `docs/architecture.md` and `docs/rendering.md`.
- Keep picking consistent with rendered placement (MeshParts pick by their box), and preserve the shared orientation convention (degrees, YXZ).
- `Particles` is host-owned state (player, editor viewport), never stored in the world; see `docs/rendering.md` (Sprites and particles). So are `Poses` (from `anim::Animators`), drawn by `build_draw_list_posed`.
- Depend only on `core`, `assets`, `render` and `terrain`; input/window handling belongs to `player` or `editor`.
- `TerrainCache` is host-owned too: it keeps chunk meshes (same key, new version per edit) and weight textures between frames; see `docs/terrain.md`.
