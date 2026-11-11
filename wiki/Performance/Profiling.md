---
title: Profiling
type: performance
status: current
verified: 2026-11-11
verified_commit: 19c7179d
source: [crates/editor/src/profiler.rs, crates/render/src/renderer/gpu_timer.rs, crates/render/src/renderer.rs, crates/core/src/bench.rs, crates/player/src/dev_console.rs]
docs: [docs/benchmarks.md, docs/rendering/performance.md, docs/architecture/play-mode.md, docs/developer-console.md]
tags: [perf, tools]
---

# Profiling

Where to look when something is slow.

| Tool | Where | Shows | Cost when off |
|---|---|---|---|
| **Editor Profiler** (Game tab) | View > Panels > Profiler (`profiler.rs`) | Frame rate, CPU (game + drawing), viewport GPU time, process memory, both Lua heaps, physics (CPU, steps, bodies), audio, scripts, animation, network; 120-frame averages; a CPU-time bar graph against 60 fps | Readings are taken anyway; the panel only reads them |
| **Editor Profiler** (Graphics tab) | same | `RenderStats`: draw calls, batches, triangles, culled items and casters, LOD distribution, GPU memory by kind, per-pass GPU time, surface-shader compile errors | — |
| **`RenderStats` / per-pass GPU time** | `TextureRenderer::stats`; `gpu_timer.rs` | Timestamps around each `FramePass` (Shadows, Depth prepass, AO, Volumetric fog, GPU particles, Scene, Post, UI); read a frame or two later | Needs `TIMESTAMP_QUERY`; the player never asks for it |
| **Tracy** | Build a host with `--features profile-tracy` | `profiling` spans: every frame step, `build_draw_list`, `UiInteraction::layout`, `ScriptRuntime::step`/`stepped`/`heartbeat`, `Physics::step`/`sync`/`substep`/`write_back`, `Replicator::frame` | Zero (spans compile to nothing; hakari leaves `profiling` out so Tracy is never on by accident) |
| **Player F9 console, Stats tab** | `dev_console.rs` | The frame's `Laps` (same step names as `--bench`) and the memory breakdown (G7) | Asked for only while the tab is open |
| **Profiler › Memory** (G7, PRs #125, #126) | Editor | Process, game memory by kind (Lua, Instances, Models, Textures, Sky and probes, Sounds, Physics) and the 12 heaviest Instance classes | — |
| **Debug views** | Editor Viewport > View Mode (`debug-views` feature of `render`, editor only) | Wireframe, normals, overdraw and others | Not built into the player |
| **Culling overlay** | Editor Viewport > Overlays > Culling | Freezes culling at a camera and shows drawn (green) vs culled (red) spheres | Editor only |
| **Frame-rate and bound tools** (rendering Phase 1, PR #83) | `profile_project`, the Profiler and F9 | Frame rate with 1% lows, whether a frame is CPU- or GPU-bound, GPU passes including per-cascade shadow timing | — |
| **Overdraw as numbers** (`OverdrawStats`) and the **LOD Inspector** tab | Editor | How many times pixels are shaded, whose overdraw it is; which LOD each item draws | Editor only |
| **Callgrind** | valgrind on the container | Instruction counts per function; stable across runs | — |

## Reading the numbers

- **CPU-bound vs GPU-bound**: compare the Profiler's CPU frame with the GPU frame. Owner's heavy scenes have been CPU-bound in `build_draw_list` + `ScenePipeline::draw` with the GPU under 3 ms (General Store Roadside, before threaded planning), and GPU-bound in fill and shadows (forest at Ultra) after it.
- **Low overall CPU% is expected**: the frame runs on one thread ([Threading model](../Architecture/Threading-Model.md)).
- **First frame vs steady frame**: first-frame cost is loading (decode, mips, upload); see [Asset pipeline overview](../Asset-Pipeline/Pipeline-Overview.md).
- **Debug builds lie**: `scene` and most engine crates are unoptimized in dev.

## Related

[Benchmarking](Benchmarking.md) · [Debugging](../Development/Debugging.md)
