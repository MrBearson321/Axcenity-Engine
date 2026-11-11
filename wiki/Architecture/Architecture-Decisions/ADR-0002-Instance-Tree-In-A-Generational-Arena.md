---
title: "ADR-0002: Instance tree in a generational arena, not an ECS"
type: adr
adr: 2
adr_status: Accepted
date: 2026-10-17
affects: [core, script, net, scene, physics, editor]
source: [crates/core/src/arena.rs, crates/core/src/world.rs, crates/core/src/index.rs]
docs: [docs/architecture/data-model.md, docs/research/engine-modernization-report.md]
tags: [adr, data-model]
---

# ADR-0002: Instance tree in a generational arena, not an ECS

- **Status:** Accepted (v1 plan, 2026-10-17; reaffirmed by the modernization report, 2026-11-10)
- **Affected systems:** core, every reader of the World

## Context and problem

Axcenity offers a Roblox-familiar object model: a tree of Instances with classes, names, parents, properties and events. Rust makes parent/child pointer graphs awkward (`Rc<RefCell<…>>` cycles, borrow panics), and the editor needs cheap deep copies (Play, undo) and stable references that survive destruction. Later, multiplayer needed the same ID on every device.

## Considered alternatives

| Option | For | Against |
|---|---|---|
| `Rc<RefCell>` tree | Natural pointers | Runtime borrow panics, cycles, no cheap clone, no stable IDs across processes |
| ECS (`bevy_ecs`, archetypes) | Cache-friendly queries | Archetype fragmentation on heterogeneous, prefab-heavy content (Unity's own docs, report §3.1); poor fit for a tree API scripts expect |
| **Arena of Instances + generational IDs** | IDs are plain data; clone is a deep copy; stale IDs resolve to `None`; network IDs for free | "Find all X" needs an index; properties need a representation |

## Decision

One `World` owns an `Arena<Instance>`; everything refers to Instances by `InstanceId` (index + generation). Never `Rc<RefCell>`. No ECS. "Find all X" uses the class and tag index (M2) rather than tree walks.

## Consequences

- `World::clone` powers editor Play and undo snapshots; every ID stays valid in the copy.
- Replicas keep server IDs; device-local IDs start at 2³¹.
- Every lookup handles `None`; scripts hold IDs only.
- Properties as `BTreeMap<&'static str, Value>` are a cost (~9% of S10k instructions); typed slots (L1) are the planned answer if it grows, not a different object model.

## Implementation references

`crates/core/src/arena.rs`, `world.rs`, `index.rs`; [Core and data model](../../Engine-Systems/Core-and-Data-Model.md).
