---
title: Threading model
type: architecture
status: current
verified: 2026-11-11
verified_commit: 7e373ad1
source: [crates/assets/src/loading.rs, crates/anim/src/threads.rs, crates/render/src/renderer/parallel.rs, crates/render/src/renderer/uploads.rs, crates/nav/src/shared.rs, crates/scene/src/terrain.rs, crates/terrain/src/job.rs, crates/script/src/csg.rs, crates/project/src/watch.rs, crates/project/src/packed.rs, crates/editor/src/cooking.rs, crates/player/src/preload.rs, crates/script/src/task.rs]
docs: [docs/project/assets.md, docs/architecture/navigation.md, docs/scripting/execution.md, docs/development-policy.md]
tags: [architecture, threading, perf]
---

# Threading model

> [!summary]
> The **game frame is single-threaded**: one thread owns the `World` and runs every frame step, including both Luau VMs. Parallelism comes from **workers that never touch the World** (asset decoding, navmesh assembly, terrain chunks, uploads, cooking, CSG) and from **data-parallel inner loops** (rayon in draw planning and in rapier's `parallel` feature) that keep their output order fixed.

## The main thread

| Owns | Notes |
|---|---|
| `World` | Passed by `&`/`&mut`; no locks, no `Rc<RefCell>` ([ADR-0002](Architecture-Decisions/ADR-0002-Instance-Tree-In-A-Generational-Arena.md)) |
| `ScriptRuntime` (server and client Luau VMs) | `task.synchronize()` and `task.desynchronize()` do nothing: scripts run on one thread (`docs/scripting/execution.md`) |
| `Physics` stepping | rapier's step itself fans out internally (below) |
| Drawing | `ScenePipeline::draw` records on the main thread; planning fans out (below) |
| Editor UI | eframe owns the loop; Play's `Session::frame` runs inside it |

Rule of thumb: **anything that reads or writes the World runs on the frame's thread**. A worker gets a copy or an `Arc` of plain data and hands back a result the frame applies at a fixed point.

## Workers

| Thread | Spawned in | Does | Hands back |
|---|---|---|---|
| `asset loader` ×(cores − 2, clamped 4–8) | `assets/src/loading.rs` | Reads and decodes model, texture, material and sound files off the `Assets` lock (M5) | Results into the `Assets` cache; a `LoadTicket` per request |
| Player preload | `player/src/preload.rs` | Requests the Workspace's assets right after the project loads (I11) | Holds the loading screen until they're in |
| Texture uploads | `render/src/renderer/uploads.rs` | Background GPU uploads of meshes and textures while streaming is on, and always in the editor's viewport (`set_background_uploads`); behind the editor's loading screen, everything in the world (`TextureRenderer::preload`, `Viewport::opening`) | Uploaded meshes and textures; the editor's loading screen counts them down |
| `viewport renderer` (editor) | `editor/src/viewport.rs` | Makes the viewport's `TextureRenderer` (pipelines) while a project loads | The renderer, taken by the first viewport frame |
| `navmesh build` | `nav/src/shared.rs` | Assembles the navigation mesh from the changed parts' solids (`Assembly::run`, M13) | The new mesh about 0.1 s of game time later (`SharedNavMesh`) |
| Terrain layer maps | `scene` / loading workers | Decoded on the asset loading workers even when cooked, and the layer arrays built in parallel, each texture resized once (PRs #67, #69) | Pixels for the terrain's texture arrays |
| `rmcp` thread (editor `--mcp`) | `mcp/src/server.rs` | MCP protocol on stdio; hands calls to the editor thread as `Job`s | Tool results |
| `mcp listener` + one `mcp client` thread per connection (editor with Settings ▸ AI connections on) | `editor/src/app/mcp/live.rs` | Loopback listener; each relayed client's hello, then `rmcp` on that TCP connection (`serve_tcp`) | `Job`s drained on the main thread at the start of a frame, 4 ms budget, a repaint to wake it |
| Terrain chunk worker | `scene/src/terrain.rs` | Builds terrain chunk meshes | Chunk meshes into `TerrainCache` |
| `terrain job` | `terrain/src/job.rs` | Generation and erosion (editor) | A new heightfield |
| CSG worker (per call) | `script/src/csg.rs` | `UnionAsync`, `SubtractAsync`, `IntersectAsync` through `mesh_edit::combine` | Resumes the script from `Context::step` |
| `project watcher` | `project/src/watch.rs` | `notify` file events, polling fallback (I10) | Changed-file lists polled by the host |
| `texture cooking` (+ a rayon pool of half the cores) | `editor/src/cooking.rs` | Cooks textures and models on project open and after file changes (L2) | Files in `.axcenity/imported/` |
| Thumbnails ×3, ambientCG download, project open, engine rebuild | `editor/src/…` | Editor conveniences | Messages to the UI |
| Unpack workers (scoped) | `project/src/packed.rs` | Unpack `.axgame` entries in parallel (M9) | Files on disk |
| Bake and weight solves (scoped) | `mesh_edit/src/bake.rs`, `rig/weights.rs` | Model Studio bakes and bone weights | Results |
| Kira's audio thread | inside `kira`/`cpal` | Mixes and outputs sound | Nothing back; `GameAudio::update` reports finished sounds |
| Game tests (editor) | `editor/src/game_tests.rs` | Runs `axcenity_testing::Runner` | Results to the Tests panel |

## Data-parallel inner loops

| Where | Library | Rule |
|---|---|---|
| Draw planning (`plan_draws`, culling, batching) | `rayon` (the renderer's own pool, below) | Only above 4,096 items (`WORTH_SHARING` in `render/src/renderer/parallel.rs`); parts of 512; **output order equals input order** so a frame is identical on any thread count (tested) |
| rapier's step | rapier3d `parallel` on **physics' own rayon pool, at most 12 threads** (`physics/src/threads.rs`, `AXCENITY_PHYSICS_THREADS`; PRs #86, #93) | rayon's global pool has a thread per logical CPU (32 on the Desktop) and was slower. Results don't depend on the count. Worlds sharing the pool step one at a time (rapier blocks a pool thread on its own tasks, which deadlocked tests side by side). Tiny scenes pay a little (`hello-vehicle` 0.05–0.07 → 0.19 ms) |
| Skeletal posing (Phase 93) | the **animation pool, at most 8 threads** (`anim/src/threads.rs`, `AXCENITY_ANIM_THREADS`; 1 = the calling thread) | State machines, properties and events stay on the frame's thread; each pose that needs nothing more from the World (no Ragdoll, IKControl or foot placement) is worked out on the pool from its clips and state machine, from 8 poses up; results applied in input order, the same on any thread count (`a_crowd_is_posed_the_same_on_one_thread_and_on_eight`). The Profiler shows posing per thread |
| Draw planning pool | the **renderer's own rayon pool, at most 12 threads** (`MAX_THREADS`, `AXCENITY_RENDER_THREADS`; 0 uses the global pool; PR #88) | Unpinned Render step 6–18% less CPU (S10k 2.60 → 2.14 ms) |

## Rules for new concurrent code

1. **Don't share the World across threads.** Copy what the job needs, or give it `Arc`s of immutable data.
2. **Apply results at a fixed frame step**, in a deterministic order (the "parallel read, serial write" pattern, report section 5.5). Determinism across machines is not a requirement, but free ordering is kept ([ADR-0014](Architecture-Decisions/ADR-0014-Determinism-Not-Required.md)).
3. **Never block the frame on a worker.** Assets hand out tickets; the draw list draws nothing for a model not in yet (M5 follow-up); the navmesh keeps the old mesh until the new one arrives.
4. **Measure before parallelizing.** L4's first measurement found the Characters step was a linear scan, fixed by kept query bounds (2.05 → 0.31 ms), not by more threads ([Optimization log](../Performance/Optimization-Log.md#l4-query-bounds)).
5. **Name long-lived threads** (`thread::Builder::name`) so profilers and panics say which.

## Planned

- **L4, parallel read / serial write stages** in the frame: open; no 4-core S10k stage is a clear target yet; measure on 8+ cores first ([Current roadmap](../Roadmap/Current-Roadmap.md)).
- **Phase 93, "A multithreaded frame"**, in `plans/axcenity-engine-unified.md`: posing across threads, the Profiler's per-thread posing time and the crowd benchmark scene (`axcenity-test --make-crowd`) are built; the before/after numbers on a 6-core desktop are still to measure. Merging every pool into one isn't planned: physics, drawing and posing each measured faster on a pool of their own size.
