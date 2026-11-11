---
title: "ADR-0019: Gameplay systems live in their own crate behind GameSystem; Players stays in script"
type: adr
adr: 19
adr_status: Accepted
date: 2026-11-11
affects: [script, gameplay, sim, net, all hosts]
source: [crates/script/src/system.rs, crates/gameplay/src/lib.rs, crates/sim/src/lib.rs]
docs: [docs/architecture/game-systems.md, docs/research/engine-modernization-report.md]
tags: [adr, scripting, gameplay]
---

# ADR-0019: Gameplay systems live in their own crate behind GameSystem; Players stays in script

- **Status:** Accepted (roadmap L7, PRs #68–#85; the Players decision is Paul's, 2026-11-11)
- **Affected systems:** script, gameplay, the hosts

## Context and problem

`script` had become a "god crate": about 53k lines holding the Luau runtime plus health, tools, humanoids, abilities, dialogue, quests and more, with ~20 hand-called `step_*`/`fire_*` entry points. Gameplay changes touched the runtime; systems couldn't be tested apart.

## Considered alternatives

| Option | For | Against |
|---|---|---|
| Leave them in `script` | No churn | Keeps the god crate |
| ECS-style systems | Familiar elsewhere | Conflicts with [ADR-0002](ADR-0002-Instance-Tree-In-A-Generational-Arena.md) |
| **A `GameSystem` trait in `script`, systems in a `gameplay` crate** | `ScriptRuntime` shrinks to the Luau core; systems use only `script`'s public API; one install point | Every host must build its runtime with the systems (`sim::script_runtime`) |

## Decision

`script::GameSystem` defines what a system adds (services, methods, kept fields, step, input and prompt hooks, event firing, activation requests). Debris, Abilities and Effects, Tools, Health and Humanoids, and Story (dialogues, quests) live in `gameplay` and are installed by `gameplay::install`, which every host gets through `axcenity_sim::script_runtime()`. **Players stays in `script`**: it's wired into joining, levels, kicks, `LocalPlayer` and networking, so it's multiplayer core, not an add-on. L7 ends there.

## Consequences

- New gameplay features become `GameSystem`s in `gameplay`.
- A system that needs more from `script` gets one thing made public on purpose.
- A bare `ScriptRuntime::new()` has no gameplay systems (edit-time Console, plugins).

## Implementation references

`crates/script/src/system.rs`, `crates/gameplay`; [Gameplay systems](../../Engine-Systems/Gameplay-Systems.md).
