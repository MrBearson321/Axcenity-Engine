---
title: "ADR-0013: Look belongs to the world, cost to the machine"
type: adr
adr: 13
adr_status: Accepted
date: 2026-10-26
affects: [render, render-types, scene, script, player, editor]
source: [crates/render-types/src/graphics.rs, crates/render/src/graphics.rs]
docs: [docs/rendering/settings.md, plans/axcenity-engine-graphics.md]
tags: [adr, rendering]
---

# ADR-0013: Look belongs to the world, cost to the machine

- **Status:** Accepted (graphics plan, 2026-10-26)
- **Affected systems:** rendering settings, Lighting, `UserGameSettings`

## Context and problem

Graphics features have two kinds of knob: how the world should look (the creator's choice, saved with the game) and how much the machine can afford (the player's choice, per computer). Mixing them makes a creator's world look different on every machine, or forces creators to ship performance settings.

## Decision

- **Look** lives in the world: Lighting, Atmosphere, effect Instances, lights; saved in `world.json`; changed by Lua, blocks and Properties.
- **Cost** lives in `GraphicsSettings` (Low/Medium/High/Ultra plus overrides), saved per machine (editor settings, the player's `graphics.json`), never in a project.
- Where a Lighting property is also a cost (shadows, post-processing), the world's value is a **request** and the settings are a **ceiling**.
- **Capabilities, not crashes**: `Capabilities` are read from the adapter once; `GraphicsSettings::effective` lowers what the machine can't do and returns a note.
- **Today's look is High**; new options default to today's behavior; **targets exist only while used**; `Material` is additive.

## Consequences

- `hello-graphics` has a reference image per preset.
- Scripts change the machine's settings only through `UserGameSettings` (LocalScripts).
- `render-types` holds the settings as plain data so the server and scripts don't link wgpu (M8).

## Implementation references

`crates/render-types/src/graphics.rs`; `docs/rendering/settings.md`; [Rendering](../../Engine-Systems/Rendering.md), [Lighting and atmosphere](../../Engine-Systems/Lighting-and-Atmosphere.md).
