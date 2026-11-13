# Terrain

Read this when changing terrain data, brushes, generation, or how a Terrain is saved and drawn. The Terrain Studio in the editor is `crates/editor` (`terrain_studio.rs`). Its tool palette (Sculpt, Materials, Terrain rows) shows icons, with each tool's name, key and hint on hover; from 400 points wide it also names each tool beside its icon (`CAPTIONS_WIDTH`; test `a_wide_panel_names_its_tools`). This page is the algorithms in `crates/terrain`.

`TerrainData` lives on the Terrain Instance in `core`. This crate only computes on that data. It does not draw, simulate contacts, or touch the disk: `scene` meshes it, `physics` collides with it, and `project` reads and writes the files.

Foliage is painted onto the terrain the way materials are: a brush, a density, and a choice of item from the Assets panel (a built-in prop or a model). Each stroke scatters instances on the surface (sample XZ, so they follow sculpting). They are not Instances. A stroke can mark them solid so parts rest on them, or leave them visual so parts fall through to the terrain. The Terrain Studio's Foliage tool paints them; click a prop or model in Assets to choose what to scatter. Shift erases the chosen item. One stroke is one undo step, including turning collide off. Painted foliage saves as optional `foliage.json` in the terrain's data folder; a missing file means none. Older paints that named a built-in plant still load. Painted plants are drawn only within a cull distance of the camera, fading out before it (grass 120 studs, flowers 150, bushes 300, trees 800, times their scale; props at any distance; a painted model by its size as placed, 120 studs per stud of its largest extent, from 60 to 1,500, so a half-stud fern goes at 60 studs and a 10-stud clump of trees at 1,200, with its levels of detail before that; `scene::foliage::model_cull_distance`). `scene` skips a plant or model past its distance (measured flat, in XZ) before building its draw items, so a terrain with thousands painted costs only those near the camera; the renderer's own cull and fade do the rest (see [rendering/pipeline.md](rendering/pipeline.md)). Painted plants sway in the Workspace's `GlobalWind`, and their shadows with them ([rendering/wind.md](rendering/wind.md)). Painted instances are kept between frames as groups, 48 studs square and short-reach plants apart from trees, rebuilt only when the terrain or a painted model changes, so a painted forest costs the frame what it can see ([rendering/performance.md](rendering/performance.md)).

Water (LE4, `scene/src/terrain_water.rs`): a Terrain's `Water` switch floods every chunk whose lowest ground is below `WaterLevel` (world Y), drawn as the renderer's water (`docs/rendering/water.md`: waves, what's under seen through it, reflections, depth tint and foam where it meets the ground). `WaterColor`, `WaterClarity`, `WaveHeight`, `WaveLength`, `WaveSpeed` and `WaveDirection` are the water material's values. Each flooded chunk gets one water item, the top of a box as wide as the chunk at the level; its waves follow world positions, so neighbours meet seamlessly. A chunk's lowest ground is kept by its heights' version, so only an edited chunk is looked at again. The Terrain Studio's **Water** section has the switch, a level slider over the terrain's height range, the color and the waves. Water is only drawn: nothing swims or floats in it yet. Test: `scene/src/terrain.rs` `terrain_water_fills_only_the_chunks_whose_ground_dips_below_its_level`.

| Module | What it does |
|---|---|
| Placement | World studs and the sample grid |
| `brush` | Raise, lower, smooth, flatten, sculpt, noise, stamps, material paint, and foliage, with undo records |
| `convert` | A part turned into ground: samples under it raised to its top and painted (the editor's Solid Modeling ▸ To Terrain) |
| `generate` | Landscapes from settings and a seed, including presets |
| `erosion` | Water and gravity |
| `materials` | Layers by height and slope, blended with paint |
| `mesh` | Chunk meshes at several levels of detail |
| `raycast` | Where a ray meets the surface |
| `files` | PNG and JSON codecs (`heights.png`, paint maps, optional `foliage.json`) |
| `job` | Slow work on another thread, with progress and cancel |

Generators stay deterministic for a seed and `GENERATOR_VERSION`. Saved terrains record both. Bump the version when the output changes. Chunk triangles split cells the same way as collision, so the drawn surface and the collider match.
