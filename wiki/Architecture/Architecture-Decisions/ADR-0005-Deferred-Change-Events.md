---
title: "ADR-0005: Deferred change events with a 10-round cap"
type: adr
adr: 5
adr_status: Accepted
date: 2026-10-19
affects: [core, script, physics]
source: [crates/script/src/changes.rs, crates/core/src/world.rs]
docs: [docs/scripting/execution.md, docs/architecture/frame-loop.md]
tags: [adr, scripting]
---

# ADR-0005: Deferred change events with a 10-round cap

- **Status:** Accepted (v1 Phase 16; date approximate, from the v1 plan's phase order)
- **Affected systems:** core change log, script runtime, physics write-back

## Context and problem

Scripts listen for `Changed`, `ChildAdded`, `DescendantRemoving` and friends. Firing them immediately inside a property write re-enters Lua while Rust holds the World, and a handler that changes what it listens to can loop forever. Physics writes thousands of positions per frame outside any script.

## Considered alternatives

| Option | For | Against |
|---|---|---|
| Immediate events | Simple mental model | Re-entrancy into Lua from inside World mutation; unbounded recursion |
| **Logged changes fired at fixed points** | No re-entrancy; deterministic order; physics' writes fire at a known point | A handler sees a change after the writing script pauses, not mid-statement |

## Decision

The World keeps a numbered change log (`World::record_changes`, `take_changes`). `ScriptRuntime::fire_changes` fires changes in order (Scripts then LocalScripts) before and after every entry point that runs scripts. A handler only hears changes made after it connected. Handlers' own changes fire in further rounds, at most 10 (`MAX_CHANGE_ROUNDS`) per call; the rest wait for the next call and Output warns once. This matches the model Roblox moved to (deferred events).

## Consequences

- Physics' changes fire at the start of the post-simulation stage.
- `fire_changes` is about 11% of S10k frame instructions (Callgrind, PR #59), since physics motion is logged too: a known cost ([Known bottlenecks](../../Performance/Known-Bottlenecks.md)).
- Destroyed scripts stop and cloned scripts start inside `fire_changes`.

## Implementation references

`crates/script/src/changes.rs`; `core::Change`; [Scripting](../../Engine-Systems/Scripting.md#execution-flow).
