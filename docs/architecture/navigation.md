# Navigation

Where characters can walk, and agents that walk there. The `nav` crate builds one navigation mesh from the Workspace (unified plan Phase 12), finds paths across it, and walks `NavigationAgent`s along them during Play (Phase 13), steering round obstacles that weren't in the mesh when the path was found (Phase 14). Agents see and hear the player character (Phase 15), and an agent with a behavior tree runs it to decide where to walk (Phase 16).

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

Two crate-private helpers serve agents' steering: `walkable_line(a, b)` (the straight line crosses only linked cells, beside edges or not, and a line ending exactly on a cell border counts as ending in the cell it reached) and `find_path_within(from, to, max_cost)` (A* that gives up past a cost, in cells, edge cells costing 3).

## Agents

A `NavigationAgent` (a class in `core`) is parented to a Model or Part, like a character controller, and moves that body (`character::body_parts`). Its feet are the bottom middle of the body's box. Properties:

- `Destination` (Vector3): where it walks to. The editor's **Agent** insert sets it to where the agent stands.
- `WalkSpeed` (16): studs a second along the path.
- `Velocity` (Vector3): not zero, it moves straight that way, with no path.
- `Status`: `"Idle"`, `"Moving"`, `"Reached"`, `"Unreachable"` or `"Blocked"`, written by the game.
- `SightRange` (60), `SightAngle` (120) and `HearingRange` (30), and `SeesPlayer` and `HearsPlayer`, written by the game: see Perception.
- `BehaviorTree`: a `.behaviortree.json` file it runs during Play, or empty for none: see Behavior trees.

Hosts own an `Agents` (`agents.rs`), as they own `Controllers`. `Agents::step` runs after character controllers and before scripts, and `Agents::keep_kinematic` anchors the agents' parts just before physics. The mesh is built the first time an agent needs it (never in a game without agents), and again when the fingerprint changes, at most every 0.5 s of game time. Each step, every agent with a body and a `BehaviorTree` first runs its tree (see Behavior trees), and then, for each agent with a body:

- **Velocity not zero**: the path is dropped and the feet move by `Velocity × dt` across, in half-cell slices, each onto `ground_at`. At the edge of the mesh (a drop or a wall) it stops. `Destination` is set to where it now stands, so it stays put when `Velocity` goes back to zero (`Status` then `"Idle"`). Y is ignored.
- **A Destination it hasn't planned for** (a new one, or the first step): `find_path` from its feet. A path makes it `"Moving"`. No path leaves it where it is, `"Unreachable"`, and gives a warning, `<full name> can't reach (x, y, z): <reason>.`, once per destination and reason. After the mesh rebuilds, an unreachable agent tries again (quietly, if it fails the same way).
- **Steering** (before each walking step): see below.
- **Walking**: it moves `WalkSpeed × dt` along the corners, each slice onto `ground_at`, and is `"Reached"` at the last one. It stays there; being pushed off doesn't send it back. Setting the same Destination again doesn't start a new walk, since the value doesn't change.

Warnings go to the editor's Output (`Session::frame`) or the player's stderr.

### Steering round obstacles

A walking agent keeps its path when the mesh rebuilds, and checks it against the latest mesh every step (`steer` and `detour` in `agents.rs`). Something that moved into the way, such as a pushed crate, leaves no cells where it stands, so the mesh shows it once it rebuilds (at once, if the mesh is 0.5 s old, otherwise within 0.5 s).

1. **Look ahead.** From its feet, each stretch of the path that starts within `WalkSpeed × 1 s` (at least 4 cells) must be a `walkable_line`. With nothing in the way, the path is never changed: the agent walks exactly the corners `find_path` gave.
2. **Rejoin point.** From the first stretch that isn't, the path is sampled every half cell. Past the samples that aren't on the mesh in the feet's region (the obstacle), the rejoin point is 2 cells into the clear, or the last clear sample before the destination.
3. **Detour.** `find_path_within` from the feet to the rejoin point, costing no more than 3 × the length of path it replaces + 20 cells. The agent's corners become the detour, then the rest of the original path from the rejoin point on. The Agent Paths overlay draws the detour as part of the path being followed.
4. **Blocked.** No rejoin point (the path never comes clear in the feet's region, such as a walkway blocked from side to side) or no detour that short: the agent stops where it is, `Status` is `"Blocked"`, and it warns once: `<full name> is blocked on its way to (x, y, z): nothing walkable goes round what's in the way.` It keeps its path (still drawn) and looks again each time the mesh is rebuilt. When the way clears, or a detour opens, it goes back to `"Moving"` and walks on, quietly. A new Destination plans afresh.

An agent whose feet aren't on the mesh (something landed where it stands) has nothing to steer on, and keeps to its path. Only parts are obstacles: characters and other agents aren't in the mesh, so agents walk through them.

### Perception

After each agent's step, it looks and listens for the **player character**: the parent of the controller that receives movement and owns the camera (`character::player_character`). With no such character it senses nothing. This is `perception.rs`.

- **Eyes**: the middle of the agent's body box, 90% of the way up. The player character is looked at in two places: the middle of its body box and its head (90% up). Seeing either is seeing it.
- **Facing**: the way the agent last moved across (walking or at `Velocity`). Before it has moved in this Play, its body's front: the first part's -Z, turned by its `Orientation`'s Y. It keeps facing that way when it stops.
- **Sight**: within `SightRange` studs of the eyes, and within `SightAngle` degrees across, centred on its facing (the angle is measured across, ignoring height; straight above or below counts as in front; 360 sees all round). Then a ray from the eyes (`physics::cast_ray_excluding`, skipping both bodies) must reach the point with no colliding part in between. Parts with `CanCollide` off don't block it; other characters and agents do.
- **Hearing**: a `Sound` with `Playing` on anywhere inside the player character's Model or part, within `HearingRange` studs of the eyes, measured to the middle of the character's body. Walls don't block it.
- A range of 0 senses nothing, and so does a `SightAngle` of 0.

`SeesPlayer` and `HearsPlayer` are written when they change (so `Changed` fires). When one turns true, `Agents::take_sensed` returns it, and the host fires the agent's `PlayerSensed` event with `"Sight"` or `"Hearing"`. It fires again only after that sense was lost and found again. The player fires it right after the agents' step, and the editor in `Session::frame`, both before LocalScripts get the frame's input. The block **when *agent* senses the player by *Sight/Hearing*** translates to a `PlayerSensed` handler with one `if sense == ...`.

Sight casts at most two rays (plus one per sensor in the way) for each agent with the player in range and in front, every frame.

Example: `examples/hello-perception`. Tests: `crates/nav/src/perception_tests.rs`, the player's `hello_perception_guard_sees_and_hears_the_player_from_lua_and_blocks`, the editor's `sight_range_set_in_properties_changes_what_the_guard_sees_in_play_and_saves`, and `a_navigation_agents_senses_save_and_load_and_an_older_agent_gets_the_defaults` in `crates/project/src/world_file/tests.rs`.

### Behavior trees

A behavior tree is its own asset, `assets/ai/<Name>.behaviortree.json`, and an agent names it in `BehaviorTree`. The file is a node graph in the same shape as a UI Event Graph (`{ "format_version": 1, "nodes", "wires", "comments" }`, see `docs/scripting/ui-graphs.md`), edited in the same graph editor. A new optional file, so `project::FORMAT_VERSION` didn't change. Its nodes (`blocks::behavior_tree::BEHAVIOR_NODES`, offered by the editor) and what they do (`nav/src/behavior.rs`, which checks and runs them) must match; `BEHAVIOR_NODE_KINDS` lists the kinds, child pins and fields, and an editor test compares the two.

Wires go from a parent's child pin to a child's plain input (the white exec pins). Every frame of Play, before the agent walks, the tree is ticked from its Root, and each node it reaches succeeds, fails or is still running:

| Node | Does |
|---|---|
| **Root** | Runs what's wired to its pin. One per tree. |
| **Selector** (pins 1–4) | Tries its children in order, from 1 every frame, and stops at the first that succeeds or runs; fails when all fail. A child it stops reaching is stopped, so an earlier branch that starts to succeed takes over (Sees Player? interrupts a patrol). |
| **Sequence** (pins 1–4) | Runs its children in order while each succeeds and picks up at the running one next frame; fails at the first failure. Conditions before the running child are checked again every frame, so a Sees Player? turning false stops the chase after it. |
| **Sees Player?**, **Hears Player?** | Succeed while `SeesPlayer` or `HearsPlayer` is true (last frame's perception). |
| **Move To** X, Y, Z | Sets `Destination` and runs until `Status` is `"Reached"` (success) or `"Unreachable"` or `"Blocked"` (failure). Already there, it succeeds at once. |
| **Set Destination** X, Y, Z | Sets `Destination` and succeeds. |
| **Chase Player** | Fails with no player character. Within 2.5 studs of them (across), stops the agent where it is and succeeds. Otherwise sets `Destination` to their feet, again whenever they're more than a stud from it (each time finding a path), and runs; fails when that's unreachable or blocked. |
| **Wait** Seconds | Runs for that much game time, then succeeds. It doesn't stop the agent walking. |

A node stopped (not reached in a frame after it ran) forgets its state: a Wait starts again, a Sequence starts from 1. A Destination is only written when it changes. Velocity still overrides the walk, and a script's Destination is overwritten when the tree sets one.

`BehaviorTree::from_json` checks a file and gives every problem with its node: no Root (or a second one), nothing wired to Root, a Selector or Sequence with no children, a field that isn't a number (or Seconds below zero), an unknown kind, two wires out of one pin, a wire into a Root or out of a pin a node doesn't have, a loop, or more than 4096 nodes once a node wired under several parents is copied under each. `BehaviorTree::load` also refuses a path that isn't a `.behaviortree.json` (a state machine says it plays animation on an Animator). A tree that can't run leaves the agent to its scripts, and warns once: `<full name> doesn't run the behavior tree `<path>`: <why>.` Files are read the first time an agent names them and kept for the rest of Play.

Example: `examples/hello-behavior` (a Guard whose tree patrols and chases, and a Courier with no tree walked by a script). Tests: `crates/nav/src/behavior_tests.rs`, the player's `hello_behavior_guard_runs_its_tree_and_the_courier_walks_where_its_script_sends_it`, and `crates/editor/src/app/behavior_tree_tests.rs`.

### Scripts and blocks

Scripts set `Destination` and `Velocity` and read `Status`, `SeesPlayer` and `HearsPlayer`, and connect to `PlayerSensed`; blocks have **walk *agent* to (x, y, z)**, **set velocity of *agent* to (x, y, z)** and **when *agent* senses the player by *Sight/Hearing*** (`docs/scripting/blocks.md`). Example: `examples/hello-agent`. Tests: `crates/nav/src/agent_tests.rs`, the player's `hello_agent_walks_to_destinations_from_properties_lua_and_blocks`, and `crates/editor/src/app/agent_tests.rs`.

## In the editor

**View > Overlays > Nav Mesh** (also on the View ribbon tab and in Settings, and saved between runs) draws the mesh of the world the viewport shows, in edit mode or during Play (`app/nav_overlay.rs`, drawn by `paint_nav_mesh` in `viewport/overlays.rs`). Cells within 80 studs of the camera are filled with one colour per region, so unreachable ground stands out. The walkable area's edges are drawn out to 200 studs. The mesh rebuilds when the fingerprint changes, once no mouse button is held (so not mid-drag or mid-stroke), and at most every 0.5 s. The player never draws it, and builds a mesh only for its agents.

**View > Overlays > Agent Paths** (also on the View ribbon tab and in Settings, saved between runs) draws, during Play, what's left of each agent's path: a line from its feet through each corner (a dot) to its destination (a ring), from `Session::agent_paths` (`paint_agent_paths` in `viewport/overlays.rs`). It only reads the paths, so it never changes a route. Insert an agent with **Agent** (Model tab, Effects group, or the Explorer's Insert menu): into the selected Model or part, or into a new `Agent` Model with a `Body` part.

The **Behavior Tree** tab (`behavior_tree.rs`, with `app/behavior_trees.rs`; starts closed) edits trees in the generic graph editor. It follows the selected agent (or the agent in the selected Model) and shows its tree; its picker shows any tree in the project's `assets`. **New tree** makes `assets/ai/<agent's Model>.behaviortree.json` (a number added when taken) with a Root, and gives it to the selected agent if it has none; **Use on <agent>** gives the shown tree to the selected agent (one undo step). Problems show on their nodes, checked by the same code that runs the tree. Trees save with the project (and so before Play); each finished edit is a step of the tree's own undo while the pointer is over the graph. In the Assets panel a tree is a **Behavior tree**: drop it on an agent's body to give it the tree, or double-click to open it. Properties shows `BehaviorTree` with buttons to choose a file, open it and clear it.

Tests: `crates/nav/src/tests.rs` (mesh) and `crates/nav/src/agent_tests.rs` (paths, agents, steering).
