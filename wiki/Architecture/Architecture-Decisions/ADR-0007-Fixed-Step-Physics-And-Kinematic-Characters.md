---
title: "ADR-0007: Fixed 60 Hz physics and kinematic characters"
type: adr
adr: 7
adr_status: Accepted
date: 2026-10-20
affects: [physics, character, nav, script]
source: [crates/physics/src/lib.rs, crates/character/src/lib.rs]
docs: [docs/architecture/physics.md, plans/axcenity-engine-v2.md, plans/axcenity-engine-unified.md]
tags: [adr, physics]
---

# ADR-0007: Fixed 60 Hz physics and kinematic characters

- **Status:** Accepted (fixed step: v1 plan, 2026-10-17; kinematic characters: v2 plan, 2026-10-20; restated in the unified plan)
- **Affected systems:** physics, character controllers, navigation, Humanoids

## Context and problem

Physics must behave the same at any frame rate, and player characters must feel responsive and never tip over, while unanchored props still fall and collide.

## Considered alternatives

| Option | For | Against |
|---|---|---|
| Variable-step physics | Smooth at any rate | Results depend on frame rate |
| **Fixed 60 Hz accumulator (0–10 steps a frame)** | Same results at any frame rate | No interpolation yet: above 60 fps motion updates in 60 Hz steps |
| Characters as dynamic bodies | Physical reactions for free | Tipping, sliding, jitter; hard to tune |
| **Kinematic controllers with ray probes** | Precise control; walk/run/crouch modes; custom modes for scripts | Pushing and being pushed are hand-written |

## Decision

Rapier steps at a fixed 60 Hz from an accumulator, at most 10 steps per frame, with deterministic body order. Character controllers move kinematically with ray probes, after input and before scripts; the host anchors the parts they move just before the physics step. Other bodies keep their gravity and friction. `physics` never depends on `character` or `nav`.

## Consequences

- Determinism on one machine comes for free; cross-machine determinism isn't pursued ([ADR-0014](ADR-0014-Determinism-Not-Required.md)).
- Setting `Position` teleports and stops a part (Phase 6 decision; Roblox keeps its speed).
- Characters' cost is queries (fixed by L4's kept bounds), not simulation.

## Implementation references

`crates/physics/src/lib.rs` (accumulator), `crates/character/src/lib.rs`; [Physics](../../Engine-Systems/Physics.md), [Characters](../../Engine-Systems/Characters.md).
