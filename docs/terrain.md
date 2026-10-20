# Terrain

Read this when changing terrain data, brushes, generation, or how a Terrain is saved and drawn. The Terrain Studio in the editor is `crates/editor` (`terrain_studio.rs`). This page is the algorithms in `crates/terrain`.

`TerrainData` lives on the Terrain Instance in `core`. This crate only computes on that data. It does not draw, simulate contacts, or touch the disk: `scene` meshes it, `physics` collides with it, and `project` reads and writes the files.

| Module | What it does |
|---|---|
| Placement | World studs and the sample grid |
| `brush` | Raise, lower, smooth, flatten, sculpt, noise, stamps, and paint, with undo records |
| `generate` | Landscapes from settings and a seed, including presets |
| `erosion` | Water and gravity |
| `materials` | Layers by height and slope, blended with paint |
| `mesh` | Chunk meshes at several levels of detail |
| `raycast` | Where a ray meets the surface |
| `files` | PNG and JSON codecs |
| `job` | Slow work on another thread, with progress and cancel |

Generators stay deterministic for a seed and `GENERATOR_VERSION`. Saved terrains record both. Bump the version when the output changes. Chunk triangles split cells the same way as collision, so the drawn surface and the collider match.
