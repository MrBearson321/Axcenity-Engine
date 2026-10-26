# Navigation

Where characters can walk, and agents that walk there. The `nav` crate builds one navigation mesh from the Workspace (unified plan Phase 12), finds paths across it, and walks `NavigationAgent`s along them during Play (Phase 13). Steering round obstacles that weren't in the mesh comes in Phase 14.

## What it's built from

`NavMesh::build(world, assets, NavSettings)` reads the world and never changes it. Its input is `physics::solids`, the same colliders physics simulates:

- Parts and MeshParts with `CanCollide` on, anchored or not, at their positions when the mesh is built. MeshParts use their convex hulls.
- Terrains (their heightfield, solid all the way down) and their colliding foliage.
- Not: parts with `CanCollide` off, visual foliage, and the parts a `ThirdPersonController`, `FirstPersonController` or `NavigationAgent` moves (`character::character_parts`, `nav::agent_parts`). Characters and agents aren't ground.

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
- `ground_at(point)`: the ground's height under `point`, along its cell's slope.
- `find_path(from, to)`: see Paths.
- `fingerprint(world)`: a hash of everything the mesh is built from (each Part's and MeshPart's place, size, shape, model and `CanCollide`, and each Terrain's data and revision). It's cheap enough to check every frame. A host rebuilds when it changes.

A build of a 512-stud terrain at 1-stud cells takes about 50 ms. The crate is optimized in dev builds, as `render` is.

## Paths

`NavMesh::find_path(from, to)` (`path.rs`) returns the corners of a walk, both ends included, on the ground: `from` and `to` keep their X and Z, and the corners between are cell middles.

1. A* over linked cells, 4-way, one cell a step. A cell on the edge of the walkable area (missing a link: beside a drop, a wall or a gap) costs 2 more to cross, so paths keep a cell away from edges where there's room.
2. Straightened: from each corner, the path goes straight to the farthest later corner the straight line reaches. The line is walked cell by cell through links (both ways round where it passes exactly through a corner), and every cell it crosses, apart from its two ends, must not be on an edge.

Errors (`PathError`): the start isn't on walkable ground, the destination isn't, or they're in different regions ("no walkable route leads there").

## Agents

A `NavigationAgent` (a class in `core`) is parented to a Model or Part, like a character controller, and moves that body (`character::body_parts`). Its feet are the bottom middle of the body's box. Properties:

- `Destination` (Vector3): where it walks to. The editor's **Agent** insert sets it to where the agent stands.
- `WalkSpeed` (16): studs a second along the path.
- `Velocity` (Vector3): not zero, it moves straight that way, with no path.
- `Status`: `"Idle"`, `"Moving"`, `"Reached"` or `"Unreachable"`, written by the game.

Hosts own an `Agents` (`agents.rs`), as they own `Controllers`. `Agents::step` runs after character controllers and before scripts, and `Agents::keep_kinematic` anchors the agents' parts just before physics. The mesh is built the first time an agent needs it (never in a game without agents), and again when the fingerprint changes, at most every 0.5 s of game time. Each step, for each agent with a body:

- **Velocity not zero**: the path is dropped and the feet move by `Velocity × dt` across, in half-cell slices, each onto `ground_at`. At the edge of the mesh (a drop or a wall) it stops. `Destination` is set to where it now stands, so it stays put when `Velocity` goes back to zero (`Status` then `"Idle"`). Y is ignored.
- **A Destination it hasn't planned for** (a new one, or the first step): `find_path` from its feet. A path makes it `"Moving"`. No path leaves it where it is, `"Unreachable"`, and gives a warning, `<full name> can't reach (x, y, z): <reason>.`, once per destination and reason. After the mesh rebuilds, an unreachable agent tries again (quietly, if it fails the same way).
- **Walking**: it moves `WalkSpeed × dt` along the corners, each slice onto `ground_at`, and is `"Reached"` at the last one. It stays there; being pushed off doesn't send it back. Setting the same Destination again doesn't start a new walk, since the value doesn't change.

Warnings go to the editor's Output (`Session::frame`) or the player's stderr. A walking agent keeps its path when the mesh rebuilds; steering round new obstacles is Phase 14.

Scripts set `Destination` and `Velocity` and read `Status`; blocks have **walk *agent* to (x, y, z)** and **set velocity of *agent* to (x, y, z)** (`docs/scripting/blocks.md`). Example: `examples/hello-agent`. Tests: `crates/nav/src/agent_tests.rs`, the player's `hello_agent_walks_to_destinations_from_properties_lua_and_blocks`, and `crates/editor/src/app/agent_tests.rs`.

## In the editor

**View > Overlays > Nav Mesh** (also on the View ribbon tab and in Settings, and saved between runs) draws the mesh of the world the viewport shows, in edit mode or during Play (`app/nav_overlay.rs`, drawn by `paint_nav_mesh` in `viewport/overlays.rs`). Cells within 80 studs of the camera are filled with one colour per region, so unreachable ground stands out. The walkable area's edges are drawn out to 200 studs. The mesh rebuilds when the fingerprint changes, once no mouse button is held (so not mid-drag or mid-stroke), and at most every 0.5 s. The player never draws it, and builds a mesh only for its agents.

**View > Overlays > Agent Paths** (also on the View ribbon tab and in Settings, saved between runs) draws, during Play, what's left of each agent's path: a line from its feet through each corner (a dot) to its destination (a ring), from `Session::agent_paths` (`paint_agent_paths` in `viewport/overlays.rs`). It only reads the paths, so it never changes a route. Insert an agent with **Agent** (Model tab, Effects group, or the Explorer's Insert menu): into the selected Model or part, or into a new `Agent` Model with a `Body` part.

Tests: `crates/nav/src/tests.rs`.
