---
title: "ADR-0014: Cross-machine determinism is not a requirement"
type: adr
adr: 14
adr_status: Accepted
date: 2026-11-10
affects: [physics, script, render, net, mesh_edit, procedural, terrain]
docs: [docs/development-policy.md]
tags: [adr, process]
---

# ADR-0014: Cross-machine determinism is not a requirement

- **Status:** Accepted (development policy, 2026-11-10)
- **Affected systems:** physics, threading, generation, networking

## Context and problem

Lockstep multiplayer and deterministic replays need bit-identical simulation across machines (rapier `enhanced-determinism`, fixed-point math, float control), which would constrain rapier `parallel`, Luau native code and threaded work.

## Decision

Not a requirement now. The priority is reliable, consistent behavior on one machine. Keep the determinism that's free: `BTreeMap`/`BTreeSet` orders, the fixed 60 Hz step, ordered apply after parallel compute, seeded generators (terrain `GENERATOR_VERSION`, fracture and procedural PRNGs, integer PCG32 for `Random`). Don't add `enhanced-determinism`, fixed-point or cross-platform float control.

## Consequences

- rapier `parallel` is on (checked consistent on one machine).
- Replays (Phase 81) are to record the replication stream, not re-simulate.
- Revisit when lockstep or deterministic replay is actually planned.

## Implementation references

`docs/development-policy.md` ("Deterministic simulation"); [Threading model](../Threading-Model.md#rules-for-new-concurrent-code).
