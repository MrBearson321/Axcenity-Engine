---
title: "ADR-0006: One data-driven frame schedule for every host"
type: adr
adr: 6
adr_status: Accepted
date: 2026-11-10
affects: [sim, player, editor, server, testing]
source: [crates/sim/src/steps.rs, crates/sim/src/schedule.rs, crates/sim/src/golden.rs]
docs: [docs/architecture/frame-loop.md, docs/research/engine-modernization-report.md]
tags: [adr, lifecycle]
---

# ADR-0006: One data-driven frame schedule for every host

- **Status:** Accepted (roadmap M1, PRs #15, #16, #20, #24, merged 2026-11-09/10)
- **Affected systems:** the player, editor Play, the dedicated server, the test runner

## Context and problem

Four hosts each copied the frame order by hand from a prose document, and had drifted: editor Play ran the render step before Animators, audio sat elsewhere, and particles and streaming advanced inside viewport drawing. "Works in Play, breaks in the shipped game" was a real risk.

## Considered alternatives

| Option | For | Against |
|---|---|---|
| Keep four loops, review carefully | No refactor | Drift had already happened |
| A full ECS-style system scheduler with dependencies | Flexible ordering, parallelism | Big change; ordering stops being readable |
| **One ordered table of named steps with host masks** (Roblox RunService stage names, Unity PlayerLoop shape) | Readable top to bottom; drift becomes a test failure; each step a profiler span | Steps are plain functions over a `FrameCtx`; parallel stages are future work (L4) |

## Decision

`sim::STANDARD` is the player's frame as a table of `Step { name, stage, hosts, run }`. Each host runs `Schedule::for_host(h)` through its `HostIo`. Deliberate differences go in `DRIFT` with a reason. `golden.rs` fixes each host's order; the frame-loop doc's table is checked against the schedule.

## Consequences

- Editor Play now matches the player; measured faster, not slower (5.99 → 5.09 ms on S10k).
- `sim` must not depend on `render`, `wgpu`, `egui`, `winit`, `kira`, `cpal`.
- New steps follow [Add a frame step](../../Guides/Recipes/Add-a-Frame-Step.md).
- L7 (gameplay systems registering their own steps) builds on this.

## Implementation references

`crates/sim/src/steps.rs`, `schedule.rs`, `host.rs`, `golden.rs`; [Frame schedule](../../Engine-Systems/Frame-Schedule.md).
