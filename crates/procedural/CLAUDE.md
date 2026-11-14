# `procedural`

- Phase 74: what a SplineMesh (`bend.rs`: a model bent along the curve, repeated to fill it) and a SplineScatter (`scatter.rs`, standing on `surface.rs`'s probe) make from their Spline, kept up to date by `Generators::update` (`lib.rs`), and Flatten Terrain Along (`flatten.rs`). Behavior: `docs/architecture/splines.md`.
- Results are the generator's children, never saved, sent or kept in slots (`core::spline::is_generator`); their bent models are `:runtime/spline<N>.glb` in `Assets`, forgotten when replaced.
- Deterministic: the same spline, settings and seed give the same layout. Use `rng.rs` (per spot, from the seed and the spot's number), never `rand`, and never iterate a `HashMap` into output.
- `terrain_scatter.rs` (PW1): ScatterRules inside a Terrain placed per 32-sample chunk into `TerrainData::set_generated`, never saved; plan `docs/research/procedural-worlds.md`, behavior `docs/terrain.md`.
- PW4: `paths.rs` (TerrainPath settings, the keep-off strips `terrain_scatter` reads live, `apply_path`) and `route.rs` (A* over the heightfield within a grade, the road's height in the search). Behavior: `docs/architecture/splines.md` (Terrain paths).
- Depends on `core`, `assets` and `terrain` only, so the server and the player share it (physics only in tests).
