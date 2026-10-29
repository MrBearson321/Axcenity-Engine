# `mesh_edit`

- Edits triangle meshes on the CPU: plane cuts that close their holes (`cut.rs`, filled by ear clipping in `fill.rs`; then `merge.rs` refills flat faces so repeated cuts don't pile up needle triangles), shapes to cut (`shapes.rs`), convex Voronoi fracture pieces (`fracture.rs`), the pieces' `.glb` writer (`glb.rs`) and the editor's Generate glue (`generate.rs`). Behavior: `docs/architecture/fracture.md`.
- Deterministic: the same mesh, count, seed and pattern give the same pieces. Never iterate a `HashMap` into output; use the built-in PRNG, not `rand`.
- Phase 55: the Model Studio's editable meshes: half-edge `EditMesh` (`half_edge.rs`), New shapes (`primitives.rs`), selection and loops (`select.rs`), a file's objects opened and saved as `.glb` (`model_doc.rs`). Behavior: `docs/editor/model-studio.md`.
- Phase 50: solid modeling: BSP booleans of closed meshes (`csg.rs`, results closed so they combine again) and combining parts (`solid.rs`, saved as `<name>.union.glb`). Behavior: `docs/architecture/solid-modeling.md`.
- Depends on `core` and `assets` only, never `render` or `physics`. Meshes are matched by position (vertices may be split), so a cut point is computed the same way from either side of an edge.
