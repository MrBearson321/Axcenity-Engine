# `physics`

- Rapier simulation over `core`, with MeshPart hulls from `assets`. Behavior: `docs/architecture/physics.md`. Frame order: `docs/architecture/frame-loop.md`.
- Preserve deterministic body ordering and the fixed-step accumulator. Use `rapier3d::glamx` types and convert through `core` types.
- Keep contact ownership/touch tracking synchronized with collider changes, and wake bodies after external state edits.
