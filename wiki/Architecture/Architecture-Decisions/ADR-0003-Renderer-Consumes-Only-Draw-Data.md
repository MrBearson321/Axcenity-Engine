---
title: "ADR-0003: The renderer consumes only draw data"
type: adr
adr: 3
adr_status: Accepted
date: 2026-10-17
affects: [render, scene, ui, player, editor]
source: [crates/render/src/draw_list.rs, crates/scene/src/draw_list.rs, crates/render/Cargo.toml]
docs: [docs/rendering/pipeline.md, docs/architecture/crates.md]
tags: [adr, rendering]
---

# ADR-0003: The renderer consumes only draw data

- **Status:** Accepted (v1 plan, 2026-10-17)
- **Affected systems:** render, scene, ui, both drawing hosts

## Context and problem

Both the player (its own window) and the editor (a texture on eframe's device) must draw a world identically. A renderer that reads Instances directly couples GPU code to the data model, scripts and the editor, and makes it untestable without a world.

## Considered alternatives

| Option | For | Against |
|---|---|---|
| Renderer walks the World | No translation layer | Couples GPU code to everything; two hosts drift |
| **Renderer takes a `DrawList`; `scene` translates** | One renderer; GPU tests on plain data; the Godot "servers" boundary | A translation step each frame (later made incremental by M4) |

## Decision

`render` depends only on `render-types` and takes a `DrawList` (items, materials, lights, sky, fog, decals, particles, UI, camera). `scene` turns the World into it; `ui` adds screen-space layout. Meshes and textures arrive as `Arc<dyn MeshSource>` / `Arc<dyn TextureSource>` keyed by `u64`, so nothing is copied per frame.

## Consequences

- `Renderer` and `TextureRenderer` share one private `ScenePipeline`, so the editor and player draw the same.
- The translation cost became the player's largest step (8 ms on S10k) until retained draw items (M4) made it incremental.
- New renderer features add plain data to the `DrawList` and are built by `scene` ([Add a render feature](../../Guides/Recipes/Add-a-Render-Feature.md)).

## Implementation references

`crates/render/Cargo.toml` (no engine dependency but `render-types`); `scene::build_draw_list`; [Rendering](../../Engine-Systems/Rendering.md), [Scene bridge](../../Engine-Systems/Scene-Bridge.md).
