---
title: "ADR-0009: No render graph yet; passes in a fixed, readable order"
type: adr
adr: 9
adr_status: Accepted
date: 2026-10-29
affects: [render]
source: [crates/render/src/renderer/scene.rs, crates/render/src/renderer/temporal.rs]
docs: [docs/rendering/temporal.md, plans/axcenity-engine-graphics.md, docs/research/engine-modernization-report.md]
tags: [adr, rendering]
---

# ADR-0009: No render graph yet; passes in a fixed, readable order

- **Status:** Accepted (graphics plan audit and Phase 35, 2026-10; reaffirmed as L8 "Defer" in the modernization report, 2026-11-10)
- **Affected systems:** render

## Context and problem

Unreal (RDG) and Unity (URP render graph) schedule passes and alias transient targets automatically. Axcenity's frame grew to shadows, prepass, SSAO, volumetric fog, GPU particles, scene, water copy, motion vectors, TAA, motion blur, depth of field, post and UI.

## Considered alternatives

| Option | For | Against |
|---|---|---|
| Render graph (pass culling, transient aliasing) | Memory aliasing, automatic barriers | A framework to build and debug; little gain at today's pass count |
| **`ScenePipeline::draw` orders passes by hand; targets exist only while a setting uses them** | One readable function; what's kept between frames in one place (`History`) | Adding a pass means editing the sequence by hand |

## Decision

No render graph. Passes run in a fixed order in `ScenePipeline::draw`, each decided once per frame by what `FrameTargets` holds; frame targets are created and freed by use.

## Consequences

- Revisit (L8) for maintainability once wgpu is upgraded (M12) and the GPU-driven path (L5) is on the table.
- New passes: see [Add a render feature](../../Guides/Recipes/Add-a-Render-Feature.md).

## Implementation references

`crates/render/src/renderer/scene.rs`; `docs/rendering/temporal.md` ("The render graph").
