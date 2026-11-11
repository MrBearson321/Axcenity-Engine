---
title: Debugging
type: guide
status: current
verified: 2026-11-11
verified_commit: 19c7179d
source: [crates/player/src/logs.rs, crates/player/src/dev_console.rs, crates/script/src/debugger.rs, crates/editor/src/viewport/overlays.rs, crates/physics/src/sync.rs]
docs: [docs/developer-console.md, docs/architecture/play-mode.md, docs/scripting/execution.md, docs/rendering/performance.md]
tags: [development, debugging]
---

# Debugging

Tools for finding out why something misbehaves. For slowness see [Profiling](../Performance/Profiling.md); for build and test failures see [Troubleshooting](../Guides/Troubleshooting.md).

## Game behavior

| Tool | How | Shows |
|---|---|---|
| Editor Console (Output) | Bottom panel; Lua typed there runs in the scripts' sandbox | `print`, script errors `path:line: message`, load problems with the cause chain |
| Breakpoints | Click a script tab's gutter, then Play | Pauses the whole game before the line; call stack and locals; Continue (F5) |
| Live Properties | Select during Play | Values update every frame; edits change the game world (Keep to carry them back) |
| Visual-debug overlays | View > Overlays | Colliders, recent raycasts, bounds, sound emitters, navmesh, agent paths, culling |
| Several clients | Game group: Clients, Network (latency, jitter, loss) | Each device's world; Profiler Network section |
| Player F9 console | Editor Play, project-folder runs, or built games with `--dev` / the project setting | Log (by level and side), Lua on either side, Stats (frame laps) |
| Player logs | `latest.log` in the game's data folder; panic hook writes the crash message | What happened on someone else's machine |
| Game tests | `tests/*.spec.lua` with `Test.step`, `Test.press`, `expect` | Reproducible gameplay checks |

## Engine internals

| Tool | How |
|---|---|
| `RUST_LOG` | Both binaries default to `warn,wgpu_hal=error`; set `RUST_LOG=info` or a module filter |
| Debug > Panic during Play | Debug builds: tests the Play crash path |
| `AXCENITY_PHYSICS_VERIFY` | Debug builds: physics checks incremental sync against a full read |
| `World::check_index` | Debug game tests run it every 60 frames |
| `AXCENITY_RETAINED_DRAW_LIST=off` | Draw from scratch to rule out a stale retained item |
| `AXCENITY_NET_JSON=1` | Send the network wire as JSON to read it |
| View > View Mode | Editor debug views: wireframe, normals, overdraw and more (`debug-views` feature) |
| `AXCENITY_SNAPSHOTS=<folder>` | Drawing tests save what they draw |
| `shoot_project` | Screenshots of a project from named views (`AXCENITY_SHOT_*`) |
| `dump_every_example` | `--ignored` with `AXCENITY_DUMP=<file>`: each example's output and part positions after three seconds |

## Common causes to check first

- **Something draws stale after an edit**: a retained draw item missed a dependency (`scene/src/retained.rs`, add to `dependencies`).
- **Works in Play, not in the player**: a step outside the shared schedule, or a host-specific `HostIo` path.
- **Brief flicker on an OLED or VA monitor**: variable refresh rate following uneven frame times, not the renderer ([Hardware notes](../Performance/Hardware-Notes.md)).
- **A navmesh doesn't rebuild after a shape change**: `nav`'s `SHAPE_PROPERTIES` out of step with `physics::part_solids`.
- **A script's changes don't fire `Changed`**: the handler connected after the change, or the 10-round cap was hit (Output says so).
- **A cooked texture looks wrong**: compare with the uncooked path (delete `.axcenity/imported/`), check BC5 for normal maps.
