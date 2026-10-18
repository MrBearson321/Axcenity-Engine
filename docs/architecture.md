# Architecture

Read before creating a crate, adding a dependency between crates, or touching the frame loop.

## Crate map (Cargo workspace)

| Crate | Owns | May depend on |
|---|---|---|
| `core` | Instance tree, arena, properties, events, services | nothing engine-internal |
| `render` | `wgpu` renderer | nothing engine-internal; takes a draw list |
| `physics` | `rapier3d` integration | `core` |
| `script` | Lua 5.4 (`mlua`), sandbox, scheduler, engine API | `core` |
| `blocks` | block-script model and translation to Lua | `core` (not `script`) |
| `ui` | game UI: CSS subset, `taffy` layout, UI draw data | `core` |
| `audio` | `kira` integration | `core` |
| `project` | project folder load and save, export packing | `core` |
| `player` | the runtime shipped with games (binary) | `core`, `render`, `physics`, `script`, `ui`, `audio`, `project` |
| `editor` | `egui` editor app (binary) | everything |

Rules:
- `player` must **never** depend on `editor`, `egui` or `blocks`. The editor runs the export pipeline: `blocks` translates block scripts to Lua, then `project` packs the result.
- `render` never sees Instances, scripts or the editor. Something outside it turns the world into a draw list (meshes, materials, transforms, lights, camera).
- Don't let crates depend on each other in a cycle. If two crates need each other, move the shared type into `core`.
- Pin dependency versions in the workspace's root `Cargo.toml` (`[workspace.dependencies]`).

## Data model

- One tree of **Instances**: class, `Name`, `Parent`, children, typed properties and events.
- Instances live in an **arena** and are referenced by **generational IDs** (index plus generation). Never use `Rc<RefCell<…>>` parent/child pointers.
- An ID can outlive its Instance. Every lookup returns an `Option` or `Result`; never assume the Instance still exists.

## Frame order (fixed, do not reorder casually)

1. Read input
2. Resume coroutines whose wait has expired, and fire queued events
3. Step physics at a fixed 60 Hz (accumulator; may run 0–N steps per frame)
4. Fire `Touched` and collision events
5. Update UI layout (`taffy`)
6. Build the draw list and render

The player runs these in `WindowEvent::RedrawRequested` (`crates/player/src/main.rs`), with a comment per step. Steps 4 and 5 arrive with their phases.

## Physics (decided in Phase 6)

- `Anchored` parts are fixed colliders; unanchored parts are dynamic bodies. New parts default to `Anchored = false`, like Roblox.
- Gravity is a constant 196.2 studs/s� (Roblox's default) until a `Workspace.Gravity` property exists.
- Fixed 60 Hz steps from an accumulator, at most 10 per frame. The same steps give the same results at any frame rate. There's no interpolation yet, so above 60 fps moving parts update in 60 Hz steps.
- `physics` finds script changes by comparing each Part with what it last read or wrote, not through events. Setting `Position` or `Orientation` teleports a part **and stops it** (decided in Phase 6; Roblox keeps the speed). Toggling `Anchored` also starts the part from rest.
- One body per Part, no welds or joints yet. Parts with NaN or infinite numbers are left out. Friction 0.5 and bounciness 0.25 for every material, plus a little angular damping because rapier has no rolling resistance.

## Play mode

Play runs in-process on a **deep copy** of the edit world, and Stop drops the copy. Before Play: autosave. During Play: the Lua watchdog is active, and Rust panics are caught at the Play boundary and reported to the Output panel.

## Portability

Target Windows, macOS and Linux. All runtime file access goes through the asset system, never direct `std::fs` calls in gameplay code, so web and mobile stay possible later.
