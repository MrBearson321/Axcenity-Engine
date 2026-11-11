---
title: Add a render feature
type: guide
status: current
verified: 2026-11-11
verified_commit: 19c7179d
source: [crates/render/src/draw_list.rs, crates/render/src/renderer/scene.rs, crates/render/src/renderer/types.rs, crates/render/src/shader.wgsl, crates/render-types/src/graphics.rs]
docs: [docs/rendering/pipeline.md, docs/rendering/settings.md, crates/render/CLAUDE.md, plans/axcenity-engine-graphics.md]
tags: [guide, recipe, rendering]
---

# Add a render feature

> [!summary] When to use this
> A new visual effect, material option, pass or graphics setting.

## Before you start

[Rendering](../../Engine-Systems/Rendering.md), [ADR-0003](../../Architecture/Architecture-Decisions/ADR-0003-Renderer-Consumes-Only-Draw-Data.md), [ADR-0009](../../Architecture/Architecture-Decisions/ADR-0009-No-Render-Graph-Yet.md), [ADR-0013](../../Architecture/Architecture-Decisions/ADR-0013-Look-Versus-Cost-Settings.md). Measure first if it's for speed.

## Steps

1. **Data**: add plain fields to the `DrawList` (or `Material`, additively with a default). The **look** comes from world Instances (Lighting, effects); the **cost** from `GraphicsSettings`.
2. **Scene**: build it in `scene` from the World; defaults must reproduce today's look exactly. If items read another Instance, record it as a retained-item dependency.
3. **Render**: add the pass in its place in `ScenePipeline::draw`; create its targets in `FrameTargets` only while the setting uses them; time it as a `FramePass` if it's a pass of its own (tell `stats_time_each_pass_the_frame_draws` which passes a plain frame skips).
4. **Settings**: a quality enum or switch in `GraphicsSettings` with a value per preset; a `Capabilities` check and a lowering note in `effective` if some GPUs can't do it; expose it in `UserGameSettings` if games should change it.
5. **Shader**: keep `InstanceRaw`, `FrameUniform`, `MapsUniform` and layouts aligned with `shader.wgsl`. The item shader already uses all 16 sampled textures and 16 vertex attributes: share or replace a binding; pack a new per-item value into an existing attribute.
6. **Tests**: image tests under `render/src/renderer/tests/`; `hello-graphics` must stay within its references at every preset (update references only for deliberate look changes, after checking by eye); a resource test that targets are freed when off.
7. **Docs**: a topic file under `docs/rendering/` and a row in `docs/rendering.md`; the wiki page if the frame's structure changed.

## Pitfalls

- Avoid wgpu features missing on DX12, Vulkan or Metal.
- Debug views and overlays are editor-only (`debug-views` feature); the player never draws them.
