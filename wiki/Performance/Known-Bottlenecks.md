---
title: Known bottlenecks
type: performance
status: current
verified: 2026-11-11
verified_commit: 19c7179d
source: [benchmarks/baselines.json]
docs: [docs/benchmarks.md, docs/rendering/performance.md, docs/research/engine-modernization-report.md]
tags: [perf, open-issues]
---

# Known bottlenecks

Open performance issues, each with its evidence class. When one is fixed, move it to the [Optimization log](Optimization-Log.md) and leave a line here saying where it went.

| Issue | Evidence | Where | Likely lever | Roadmap |
|---|---|---|---|---|
| Farmhouse steady frame ~0.6 ms above `300bbb5`'s 4.77 ms (now 5.40) | Measured, unexplained | Player | Profile with `profile_project` | — |
| Physics is still the largest simulation cost (58% of S10k instructions before #84; ~2.2 ms of a 2.8 ms test-runner frame after) | Measured | rapier's own step | Solver settings tried and rejected (#99 notes); `simd8` untested | — |
| Navmesh look re-reads turning anchored parts on the game thread (2.9–3.9 ms worst) | Measured | `MeshBuilder::look` | Move the read off the frame or narrow it | after M13 |
| `fire_changes` was 11% of S10k | Measured (Callgrind) | `script/src/changes.rs` | Partly fixed by #84 (skip Instances with no handler: Heartbeat −52%) | — |
| Copying retained draw items (~1.2 ms per 10,000 parts) | Measured | `scene/src/retained.rs` | Spatial `ItemChunk`s for still parts | M4 next step |
| `bench_10000_parts` `build_draw_list` rose 3.3 → 8–9 ms before `556b72b` | Measured, unexplained | `scene` | Bisect between the render benchmarks commit and `556b72b` | — |
| Shadow views copy 176-byte instances, read 88 | Measured (byte counts) | `render` | Slim shadow instance stream | L5 |
| GPU fill at 1440p Ultra in dense alpha foliage (forest ~14 ms GPU; Farmhouse ~40 ms frame) | Measured | Shadows, lighting, overdraw | LOD on heavy project models, shadow LOD (rejected once: changed the trees' look) | L5 |
| A cooked game ships each texture twice (hello-roads 393 → 830 MB) | Measured | `project/src/packed.rs` | Ship cooked only, read the pack in place | G12 |
| Opening a 10 MB `world.json` takes ~5.5 s release, ~30 s debug | Measured (rough) | `project::load` | Faster parsing or a binary world cache | — |
| Instance property reads cost ~4,000 instructions, mostly mlua | Measured (Callgrind) | `script/src/instance.rs` + mlua | mlua `__index` specialization upstream, or native `vector` | L3 |
| Vertex-heavy scenes pay ~5% for the skin field on every instance | Measured | `render` | Separate pipelines for skinned items | — |
| Editor repaints everything each Play frame | Measured in the design-system audit (A9) | `editor` | — (V7 and V8, visible-rows-only Explorer and Console, landed in #73) | Design system |
| Per-Part heap ~2 KB | **Unverified** estimate | `core` | L1 | parked |

## Abandoned Farmhouse startup regression

> [!success] Closed 2026-11-11
> PRs #65, #67 and #69 brought the first frame from 4.35 s to **0.21 s** and assets-in from 6.0 s to 1.6 s (baselines at `15965da`, PR #70). Kept here for history; see the [Optimization log](Optimization-Log.md#terrain-layers-off-the-main-thread-2026-11-11).

- **Symptom** (Measured, Desktop, player): first frame ~1.05 s at `300bbb5` → ~4.4 s at `39dd258`; launch to assets in 4.7 → 6.0 s. Carried as a `known` note in `benchmarks/baselines.json`.
- **Cause**: M5 moved texture work from startup onto the first frame; the project's 29 `.glb` files embed every 4K map, which L2 couldn't cook until PR #65.
- **Status after PR #65** (Measured, pinned, medians of 3): cooked first frame **1,869 ms**, launch to assets in **3,330 ms**, GPU textures 947 → 311 MB.
- **Last step (#67, #69)**: ~1.2 s of the first frame had been the terrain decoding its 30 cooked 2K layer maps on the main thread; they're now decoded on the loading workers and the layer arrays built in parallel. Baselines re-recorded at `15965da` (#70).

## Related

[Performance overview](Performance-Overview.md) · [Technical debt](../Roadmap/Technical-Debt.md) · [Current roadmap](../Roadmap/Current-Roadmap.md)
