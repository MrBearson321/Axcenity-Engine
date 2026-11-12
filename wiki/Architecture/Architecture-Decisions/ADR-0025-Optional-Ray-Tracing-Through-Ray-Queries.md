---
title: "ADR-0025: Optional hardware ray tracing through ray queries"
type: adr
adr: 25
adr_status: Accepted
date: 2026-11-12
supersedes:
superseded_by:
affects: [render, render-types, editor, player]
source: [crates/render/src/renderer/ray_tracing.rs, crates/render/src/adapters.rs, crates/render-types/src/graphics.rs]
docs: [docs/rendering/ray-tracing.md, docs/research/hardware-ray-tracing.md]
tags: [adr, rendering, ray-tracing]
---

# ADR-0025: Optional hardware ray tracing through ray queries

- **Status:** Accepted (Paul asked for the RT0–RT5 plan to be built, 2026-11-12; research in PR #240)
- **Date:** 2026-11-12
- **Affected systems:** renderer, graphics settings, editor Settings > Graphics and View Mode

## Context and problem

Paul wants hardware ray tracing as an optional GPU feature: the most performant option that still looks good. wgpu 30 offers two ways in, both experimental: ray queries (`EXPERIMENTAL_RAY_QUERY`, traced from compute and fragment shaders) on DX12, Vulkan and Metal, and ray tracing pipelines on Vulkan only. It has no acceleration structure refit, and TLAS instances are written on the CPU. Measured on an RX 9070 XT at 1440p: a shadow or AO ray per pixel costs 0.25–0.31 ms; BLAS builds about 2.6 ms per million triangles (`docs/research/hardware-ray-tracing.md`).

## Considered alternatives

| Option | For | Against |
|---|---|---|
| Ray queries inside today's passes, effect by effect | Every desktop backend; reuses the forward renderer, its targets and TAA; each effect can be off | No shader binding table; complex materials are the shader's job |
| Ray tracing pipelines | Natural for many-material path tracing | Vulkan only in wgpu; a second renderer to keep |
| Path tracing or ReSTIR GI | Best look | Costs several ms even with denoisers and upscalers; far past "most performant" |
| No ray tracing | Nothing to keep | Paul asked for it; shadows, GI and reflections keep their raster limits |

## Decision

Ray tracing is an optional tier built on wgpu's ray queries: devices are opened with them where the adapter has them (`adapters::with_ray_queries`), the renderer keeps a BLAS per mesh and a TLAS of the items within the ray tracing distance only while a ray traced effect (or the Ray Hits view) is on, and each effect (sun shadows, AO, probe GI, reflections, in RT1–RT5) is its own setting, off at every preset, with today's renderer as its fallback.

## Reasons

- The only path on DX12, Vulkan and Metal alike ([ADR-0011](ADR-0011-Desktop-First-Local-Verification.md): Windows first).
- Fits the look-versus-cost split ([ADR-0013](ADR-0013-Look-Versus-Cost-Settings.md)): ray tracing is a machine's cost choice; the world's look doesn't change meaning.
- Keeps [ADR-0003](ADR-0003-Renderer-Consumes-Only-Draw-Data.md): the scene is built from the draw list's items and the renderer's own mesh buffers.
- Paying nothing when off keeps every existing benchmark and reference image as it was.

## Consequences

- One more `unsafe` call (wgpu's experimental opt-in); the workspace's `unsafe_code = "deny"` allows it in `with_ray_queries` only.
- wgpu's ray query API may change between majors: an upgrade (M12-style) must check `ray_tracing.rs` and `ray_hits.wgsl`.
- Every moved item rebuilds the whole TLAS (no refit); scenes with tens of thousands of moving items need RT5's work (splitting static and dynamic, or fewer instances).
- Skinned items aren't traced until they're posed into a buffer of their own.
- Revisit if wgpu gains refit or a stable ray tracing API, or if path tracing becomes affordable on mid-range GPUs.

## Implementation references

- `crates/render/src/renderer/ray_tracing.rs` (`RayScene`, `prepare_scene`, `RayHitsPass`), `crates/render/src/ray_hits.wgsl`, `crates/render/src/adapters.rs` (`with_ray_queries`)
- `crates/render-types/src/graphics.rs` (`ray_tracing_distance`, `Capabilities::ray_queries`)
- Tests: `crates/render/src/renderer/tests/ray_tracing.rs` (run on lavapipe without a GPU)
- PR #240 (research), RT0 foundation PR
