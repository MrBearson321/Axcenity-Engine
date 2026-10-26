# Navigation

Where characters can walk. The `nav` crate builds one navigation mesh from the Workspace (unified plan Phase 12). Nothing walks on it yet: agents, paths and avoidance come in Phases 13 and 14.

## What it's built from

`NavMesh::build(world, assets, NavSettings)` reads the world and never changes it. Its input is `physics::solids`, the same colliders physics simulates:

- Parts and MeshParts with `CanCollide` on, anchored or not, at their positions when the mesh is built. MeshParts use their convex hulls.
- Terrains (their heightfield, solid all the way down) and their colliding foliage.
- Not: parts with `CanCollide` off, visual foliage, and the parts a `ThirdPersonController` or `FirstPersonController` moves (`character::character_parts`). Characters aren't ground.

## How

1. The world's footprint is cut into square columns `cell_size` studs across (default 1). Past 4M columns the cells grow, so a build stays quick.
2. Down the centre of each column, each solid gives the stretch of the line inside it (`Solid::column`: its bottom, its top and the top's normal). Every solid is convex, so that's one stretch; terrain has no bottom. Stretches that overlap or touch merge.
3. The top of a merged stretch is a **cell** when the ground there is no steeper than `max_slope` (default 45°) and the next solid above leaves `agent_height` of room (default 5). Ground under a low ceiling, or covered by a solid, has no cell.
4. A cell **links** to the neighbouring cell (-X, +X, -Z, +Z) with the smallest rise, if the rise is no more than `max_step` (default 1), or, where either cell slopes, no more than that slope climbs across one cell. A cliff between flat ground is never linked; a walkable ramp is. Links always go both ways.
5. Linked cells share a **region**. Two points in different regions have no path between them: a gap, a steep drop or a tall ledge splits regions.

The defaults match a character controller's `MaxSlopeAngle` and `MaxStepHeight`.

## Queries

- `cell_at(point)`: the cell a character at `point` stands on: the highest in its column no more than `max_step` above the point and no more than `agent_height` below it.
- `is_walkable`, `region_at`, `connected(a, b)`.
- `cells`, `links(index)`, `corners(index)` and `outline()` (cell sides with no link: the edges of the walkable area).
- `fingerprint(world)`: a hash of everything the mesh is built from (each Part's and MeshPart's place, size, shape, model and `CanCollide`, and each Terrain's data and revision). It's cheap enough to check every frame. A host rebuilds when it changes.

A build of a 512-stud terrain at 1-stud cells takes about 50 ms. The crate is optimized in dev builds, as `render` is.

## In the editor

**View > Overlays > Nav Mesh** (also on the View ribbon tab and in Settings, and saved between runs) draws the mesh of the world the viewport shows, in edit mode or during Play (`app/nav_overlay.rs`, drawn by `paint_nav_mesh` in `viewport/overlays.rs`). Cells within 80 studs of the camera are filled with one colour per region, so unreachable ground stands out. The walkable area's edges are drawn out to 200 studs. The mesh rebuilds when the fingerprint changes, once no mouse button is held (so not mid-drag or mid-stroke), and at most every 0.5 s. The player never builds or draws it.

Tests: `crates/nav/src/tests.rs`.
