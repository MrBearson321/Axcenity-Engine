---
title: Optimize safely
type: guide
status: current
verified: 2026-11-10
docs: [docs/benchmarks.md, docs/development-policy.md]
tags: [guide, perf]
---

# Optimize safely

> [!summary] When to use this
> Before changing code to make something faster.

## Before you start

- Read [Performance overview](../Performance/Performance-Overview.md) and [Known bottlenecks](../Performance/Known-Bottlenecks.md): is this already known, measured, or parked (L1)?
- Read the system page and its ADRs. Several "obvious" speedups were decided against: an ECS ([ADR-0002](../Architecture/Architecture-Decisions/ADR-0002-Instance-Tree-In-A-Generational-Arena.md)), a render graph ([ADR-0009](../Architecture/Architecture-Decisions/ADR-0009-No-Render-Graph-Yet.md)), native `vector` before checks pass ([ADR-0015](../Architecture/Architecture-Decisions/ADR-0015-Native-Vector-On-Hold.md)), determinism work ([ADR-0014](../Architecture/Architecture-Decisions/ADR-0014-Determinism-Not-Required.md)).

## Steps

1. **Reproduce in release** on a named scene: S10k, a render benchmark, or a copy of a real project (`AXCENITY_BENCH_PROJECT`).
2. **Find the cost**: `--bench` step table, the editor Profiler, a Tracy capture, or Callgrind. Write down the hypothesis and the evidence class.
3. **Fix the cause, not the symptom.** Previous wins came from not doing work (kept bounds, revisions, caches), more often than from threads.
4. **Keep behavior identical.** Prove it: equality tests against the old path (M4's randomized retained-vs-scratch test), image comparisons (`every_example_renders`, `shoot_project` parity), golden orders, determinism tests for parallel code.
5. **Measure before and after** on the same machine, pinned, alternating, 3–7 rounds; record medians and ranges, plus the worst frame where spikes matter.
6. **Write it down**: the table in `docs/benchmarks.md` or `docs/rendering/performance.md`, an [Optimization log](../Performance/Optimization-Log.md) entry (problem, cause, fix, numbers, method, machine, trade-offs, PR), a dated row in [Benchmark history](../Performance/Benchmark-History.md), and `axcenity-bench-check --update` if the baselines moved.

## Check your work

| Command | Expect |
|---|---|
| `cargo nextest run --workspace` | All pass |
| `cargo clippy --workspace --all-targets -- -D warnings` | Clean |
| `axcenity-bench-check --machine <name>` (pinned) | No `SLOWER` lines you can't explain |

## Pitfalls

- Container numbers aren't GPU numbers (lavapipe).
- Debug builds hide or invent hotspots.
- An unpinned A/B on the Desktop measures other agents' builds.
- A small scene can get slower from parallelism (rapier `parallel` on `hello-vehicle`; rayon below 4,096 items).
- `#[inline]` mattered for cross-crate iterators (I9 got 40% slower without it).
