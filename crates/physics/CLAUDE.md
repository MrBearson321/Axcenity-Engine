# `physics`

- Rapier simulation over `core`, with MeshPart hulls from `assets` and colliding foliage on terrains. Behavior: `docs/architecture/physics.md`. Frame order: `docs/architecture/frame-loop.md`. Queries (`cast_ray`, `debug_colliders`, `solids`) must not step or write back.
- Preserve deterministic body ordering and the fixed-step accumulator. Use `rapier3d::glamx` types and convert through `core` types.
- Keep contact ownership/touch tracking synchronized with collider changes, and wake bodies after external state edits.
- Phase 34: `material.rs` gives colliders their `PhysicalProperties` (weights in `user_data`, the contact hook only for weights other than 1, so old worlds simulate the same); `mass.rs` is `Mass` for scripts' impulses. Velocities sync both ways like `Position`.
- Phase 35: `assembly.rs` groups welded parts; each assembly is one body (`Body::body` is shared, `offset`/`root` place each collider). Rebuild an assembly rather than editing a shared body in place when its parts or poses change.
