---
title: Add a frame step
type: guide
status: current
verified: 2026-11-11
verified_commit: 19c7179d
source: [crates/sim/src/steps.rs, crates/sim/src/host.rs, crates/sim/src/golden.rs]
docs: [docs/architecture/frame-loop.md, docs/benchmarks.md, crates/sim/CLAUDE.md]
tags: [guide, recipe, lifecycle]
---

# Add a frame step

> [!summary] When to use this
> A system needs to run every frame, or an existing step must move.

## Before you start

[Frame schedule](../../Engine-Systems/Frame-Schedule.md), [ADR-0006](../../Architecture/Architecture-Decisions/ADR-0006-One-Shared-Frame-Schedule.md), and `docs/architecture/frame-loop.md` for why each step sits where it does (change events fire before and after every script step; `RenderStepped` must see this frame's poses; network send is last).

## Steps

1. Write the step as a plain `fn(&mut FrameCtx)` in `crates/sim/src/steps.rs`; reach host-specific things through `HostIo` (add a method with a default if new).
2. Insert it into `STANDARD` at the **player's** position, with its `Stage` and a `Hosts` mask (`P`, `PE`, `GAME`, `NET`, `ALL`).
3. If a host must run it elsewhere, add a `DRIFT` entry with the reason.
4. Run the `sim` tests: `golden.rs` fails and prints the new order; accept it deliberately. `the_frame_loop_doc_lists_the_schedule` prints the new table for `docs/architecture/frame-loop.md`; paste it and describe the step in prose there.
5. If it should appear in benchmarks, map it to an existing lap name (`LAPS`) rather than inventing one, so runs stay comparable.

## Pitfalls

- `sim` must not gain `render`, `wgpu`, `egui`, `winit`, `kira` or `cpal` dependencies (the server links it).
- A step that writes the World must run on the frame thread; heavy work goes to a worker that hands back at a fixed step ([Threading model](../../Architecture/Threading-Model.md)).
