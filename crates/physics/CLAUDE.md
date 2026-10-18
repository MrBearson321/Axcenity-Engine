# `physics`

- Rapier simulation over `core`, with MeshPart hulls from `assets`; see `docs/architecture.md` (Physics) for behavior and frame order.
- Preserve deterministic body ordering and the fixed-step accumulator. Use `rapier3d::glamx` types and convert through `core` types.
- Keep contact ownership/touch tracking synchronized with collider changes, and wake bodies after external state edits.
