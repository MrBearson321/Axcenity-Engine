---
title: Benchmark history
type: performance
status: current
verified: 2026-11-11
verified_commit: 19c7179d
source: [benchmarks/baselines.json, benchmarks/results/2026-11-10-desktop-9950x3d.jsonl]
docs: [docs/benchmarks.md, docs/rendering/performance.md]
tags: [perf, history]
---

# Benchmark history

Dated whole-frame numbers, **never overwritten**: add a row or a section when you measure. All times are milliseconds, medians unless marked. Numbers compare only within one machine ([Hardware notes](Hardware-Notes.md)). Methods: [Benchmarking](Benchmarking.md).

## S10k, frame median by host

| Date | Commit | Machine | Test runner | Server | Editor Play | Player | Notes |
|---|---|---|---|---|---|---|---|
| 2026-11-09 | `95000ea` (I1) | Container | 18.6 | 9.8 | 16.8 | 97.8 | Player Render is lavapipe (61 ms) |
| 2026-11-09 | `300bbb5` (I1–I11) | Desktop | 10.0 | 5.4 | 8.7 | 21.5 | Draw list 8.38 ms was the player's largest step |
| 2026-11-10 | `940fabc` → M2 | Desktop | 6.68 → 5.91 | 4.88 → 4.65 | | 15.48 → 15.26 | Pinned, 7 alternating runs |
| 2026-11-10 | `300bbb5` → `5cb8302` | Desktop | 7.71 → 5.03 | 5.13 → 3.82 | | 14.93 → 8.73 | Pinned, after M1–M7, M13, M14, L2 1–7 |
| 2026-11-10 | `4da1528` → `1832da8` (M1) | Desktop | | | 5.99 → 5.09 | | `bench_project_play`, 7 pinned rounds |
| 2026-11-10 | `39dd258` (baseline file) | Desktop | 3.718 | 3.742 | 3.246 | 7.048 | `benchmarks/baselines.json` for `axcenity-bench-check` |
| 2026-11-11 | `15965da` (baseline file) | Desktop | 3.746 | 3.848 | 3.44 | 7.117 | Re-recorded with #65/#67/#69 (startup); frames within tolerance |
| 2026-11-11 | `e52292c` → part 1 (#84) | Desktop pinned | 3.71 → 2.47 | 3.71 → 2.38 | | | XB10k 3.41 → 2.08 |
| 2026-11-11 | `e3257295` → own pool (#86) | Desktop unpinned | 3.37 → 2.81 | | | 8.79 → 8.36 | |

## S10k, step medians at the two reference points

| Step | Container `95000ea` (test runner) | Desktop `300bbb5` (test runner) | Desktop `300bbb5` (player) |
|---|---|---|---|
| Characters | 2.84 | 2.70 | 2.41 |
| Agents | 3.99 (worst 134) | 0.32 (worst 13.6) | 0.35 (worst 12.6) |
| Input | 1.82 | 0.86 | 0.88 |
| Scripts | 0.34 | 0.12 | 0.14 |
| Physics | 7.59 | 4.94 | 5.46 |
| Heartbeat | 1.52 | 0.86 | 0.87 |
| Animation | 0.28 | 0.12 | 0.32 |
| Draw list | | | 8.38 |
| Render | | | 2.69 |

## Owner's projects (Desktop, player)

| Date | Commit | Project | Frame | First frame | Launch to assets in |
|---|---|---|---|---|---|
| 2026-11-10 | `300bbb5` | Abandoned Farmhouse | 4.77 | 1,054 | 4,692 |
| 2026-11-10 | `5cb8302` | Abandoned Farmhouse | 5.50 | 4,517 | 6,207 |
| 2026-11-10 | `39dd258` | Abandoned Farmhouse | 5.473 | 4,354 | 6,043 |
| 2026-11-10 | main (pre-#65), uncooked | Abandoned Farmhouse | 5.449 | 2,926 | 7,224 |
| 2026-11-10 | `924720a` (#65), cooked | Abandoned Farmhouse | 5.413 | 1,869 | 3,330 |
| 2026-11-10 | `300bbb5` | General Store Roadside | 6.99 | 1,076 | 2,428 |
| 2026-11-10 | `5cb8302` | General Store Roadside | 4.40 | 1,251 | 2,476 |
| 2026-11-10 | `39dd258` | General Store Roadside | 4.19 | 1,230 | 2,472 |
| 2026-11-10 | `300bbb5` | Temperate Wet Patch | 47.80 | 1,796 | 3,099 |
| 2026-11-10 | `5cb8302` | Temperate Wet Patch | 5.52 | 1,003 | 2,265 |
| 2026-11-10 | `39dd258` | Temperate Wet Patch | 5.279 | 820 | 2,064 |

| 2026-11-11 | `15965da` | Abandoned Farmhouse | 5.399 | 212 | 1,579 |
| 2026-11-11 | `15965da` | General Store Roadside | 4.092 | 446 | 1,581 |
| 2026-11-11 | `15965da` | Temperate Wet Patch | 5.14 | 268 | 1,399 |
| 2026-11-11 | `15965da` | Temperate Dry Patch, Ultra 2560×1440 | 11.043 (GPU 10.9, GPU-bound) | 282 | 1,559 |

The pre-#65 Farmhouse row is from the PR's own run and isn't directly comparable to the `39dd258` baseline row (different session).

## Rendering benchmarks (RX 9070 XT, High, 1280×720)

| Date | Commit | Benchmark | CPU `build_draw_list` | CPU `ScenePipeline::draw` | GPU frame |
|---|---|---|---|---|---|
| 2026-10 (graphics Phase 11) | benches added | 10,000 parts, overview | 3.28 | 2.44 | 1.92 |
| 2026-10 (graphics Phase 11) | benches added | 2,000 MeshParts | 0.64 | 0.92 | 9.29 |
| 2026-10 (graphics Phase 11) | benches added | Terrain + foliage | 6.48 | 12.14 | 5.99 |
| 2026-11-09 | `300bbb5` | 10,000 parts | 8.5–8.9 | | |
| 2026-11-09 | `556b72b` | 10,000 parts | 8.0–8.8 | | |
| 2026-11-09 | `3f2fc05` | Forest 2560×1440 Ultra | 1.2 → 1.3 | 39.6 → 9.5–10.0 | 31.0 → 13.6–13.9 |

**Upscaling (G9, `bench_upscaling`, 2026-11-11, `c004365`)**: a light field of blocks, High, FXAA, median GPU frame in ms over two runs. EASU and RCAS cost the upscaled frame minus a native frame at the small size.

| Output | 1.00 | 0.67 | 0.50 | Upscale cost (0.67 / 0.5) |
|---|---|---|---|---|
| 1920×1080 | 1.01–1.19 | 0.85 | 0.63 | 0.15 / 0.13 |
| 2560×1440 | 1.03–1.07 | 0.99–1.12 | 0.93 | noise / 0.24 |
| 3840×2160 | 1.90–1.92 | 1.27 | 1.01 | 0.20–0.26 / below noise |

**Unexplained**: `bench_10000_parts`'s `build_draw_list` rose from 3.3 to 8–9 ms between the commit that added the render benchmarks and `556b72b`; not bisected ([Known bottlenecks](Known-Bottlenecks.md)).

## Collaborate on S10k (C11)

Container, release, 2026-11-12 ([Optimization log](Optimization-Log.md#collaborate-on-s10k-and-the-world-load-2026-11-12)):

| Measure | Before | After |
|---|---|---|
| Join message (the session's World) | 953 KiB | 82 KiB (zstd level 1, 1 ms) |
| A member opens the session's files as its project | 5.9 s | 0.35 s |
| `project::load` of S10k | 5.66 s (debug ~30 s) | 0.19 s (debug 0.8 s) |
| Idle `team_frame`, mean (median) | | host 48–57 µs (12–19), member 34–36 µs (2–3) |
| Four busy members' presence through the host | | 19 KiB a second |

The join over Tailscale between two machines is still to be recorded (goal: under 10 s).

## Against other engines (XB10k)

`benchmarks/cross-engine/` defines XB10k (S10k's parts, physics and scripts rebuilt in each engine with one timing rule; PR #80). On the Desktop, 2026-11-11, median frame: **s&box 2.05 ms** (real time, not ranked), **Unity 3.11**, **Axcenity 5.14** (6.70 before #84/#86/#88/#93), **Godot 10.27**; Unreal and Roblox not timed yet. Results: `benchmarks/cross-engine/results.md`; what the numbers mean against each engine's features: `docs/research/engine-comparison.md`.

## Raw data

`benchmarks/results/2026-11-10-desktop-9950x3d.jsonl` holds one JSON line per run of the 2026-11-10 before/after comparison.
