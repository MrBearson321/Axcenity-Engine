---
title: Splines and procedural
type: system
status: current
implementation: implemented
crates: [core, procedural, script]
source: [crates/core/src/spline.rs, crates/procedural/src/lib.rs, crates/procedural/src/bend.rs, crates/procedural/src/scatter.rs, crates/procedural/src/flatten.rs, crates/procedural/src/rng.rs, crates/procedural/src/terrain_scatter.rs, crates/script/src/spline.rs]
docs: [docs/architecture/splines.md, crates/procedural/CLAUDE.md]
verified: 2026-11-11
verified_commit: 19c7179d
tags: [system/splines, procedural]
---

# Splines and procedural

> [!summary]
> A **Spline** is a curve of cubic Bézier stretches through its **SplinePoint** children (Auto, Linear or Free tangents), measured into an even-speed length table (`core::spline`, shared by `script`, `nav`, `anim` and `net`). **SplineFollowers** carry parts along it (server-side), NavigationAgents patrol it, Sequence cameras ride it. `procedural` makes what **SplineMeshes** (a model bent along the curve) and **SplineScatters** (models along or inside it, standing on the surface below) generate, keeps it up to date (`Generators::update`) and flattens terrain along a spline. Generated children are never saved, sent or kept in slots: every device makes them from the same settings and seed. **Implemented.**

## Flow

```mermaid
flowchart LR
  SP[Spline + SplinePoints] --> C[SplineCurve::of: 64 samples per stretch]
  C --> FOL[script: SplineFollowers move on, Reached]
  C --> GEN["procedural::Generators::update<br/>(first in scripts.step, every device)"]
  GEN --> BM["bend.rs: :runtime/spline&lt;N&gt;.glb in Assets"]
  GEN --> SC["scatter.rs on surface.rs probe;<br/>rng.rs per spot from seed"]
  C --> FL[flatten.rs: Flatten Terrain Along]
  GEN --> TSC["terrain_scatter.rs: ScatterRules<br/>per terrain chunk (PW1)"]
```

## Development guidance

- Deterministic: the same spline, settings and seed give the same layout; use `rng.rs`, never `rand`; never iterate a `HashMap` into output.
- Anything that saves, sends or snapshots the tree must leave generated children out (`core::spline::is_generator`).
- Terrain scatter rules (PW1) keep their results in `TerrainData`'s generated chunks, not as children; see [Terrain](Terrain.md).
- `procedural` depends on `core`, `assets`, `terrain` only, so the server and player share it.

## Related documentation

`docs/architecture/splines.md`; `examples/hello-splines`, `examples/hello-roads`; [Terrain](Terrain.md), [Networking](Networking.md); the procedural worlds plan (scatter rules, biomes, presets, roads, buildings, mesh terrain proposal): `docs/research/procedural-worlds.md`
