# `physics`

- Rapier simulation over `core`, with MeshPart hulls from `assets` and colliding foliage on terrains. Behavior: `docs/architecture/physics.md`. Frame order: `docs/architecture/frame-loop.md`. Queries (`cast_ray`, `debug_colliders`) must not step or write back.
- Preserve deterministic body ordering and the fixed-step accumulator. Use `rapier3d::glamx` types and convert through `core` types.
- Keep contact ownership/touch tracking synchronized with collider changes, and wake bodies after external state edits.
