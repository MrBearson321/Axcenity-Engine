# `terrain`

- Terrain algorithms over `core::TerrainData`: placement, brushes (including foliage scatter and collide), generation, erosion, material rules and biomes (`biomes.rs`, PW2), chunk meshes, raycasts, file codecs and background jobs; see `docs/terrain.md`.
- Depends only on `core`; no UI, GPU or file-system access (`project` reads and writes the files, `scene` draws, `physics` collides).
- Keep generators deterministic for a seed and `GENERATOR_VERSION`: saved terrains record both to be made again. Bump the version when output changes.
- Render triangles must split cells like parry's heightfield (`mesh.rs`), so collision matches what's drawn. Hole cells (PW7, `TerrainData::is_hole`) must be skipped by anything reading the surface: mesh, raycast, `ground_y`, physics, nav and scatter.
- `volume.rs` is the PW8 voxel spike (samples, brushes, surface nets); it is not an Instance, saved or drawn yet. Plan and go/no-go: `docs/research/procedural-worlds.md`.
