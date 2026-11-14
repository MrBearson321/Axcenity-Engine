---
title: Benchmarking
type: performance
status: current
verified: 2026-11-14
verified_commit: f6d5427
source: [crates/testing/src/s10k.rs, crates/testing/src/showcase.rs, benchmarks/features/README.md, crates/testing/src/bench_check.rs, crates/core/src/bench.rs, crates/player/src/bench_tests.rs, benchmarks/baselines.json]
docs: [docs/benchmarks.md, docs/rendering/performance.md]
tags: [perf, method]
---

# Benchmarking

How Axcenity is measured, and the rules that make a number trustworthy. Commands and the scene definitions are in `docs/benchmarks.md` and `docs/rendering/performance.md`.

## The tools

| Tool | Measures | Command (release) |
|---|---|---|
| **S10k scene** | The whole frame on every host: 10,000 parts (1,000 falling), 200 scripts, 50 sounds, 20 animated characters, 10 agents | `cargo run --release -p axcenity-testing -- --make-s10k target/s10k` |
| **Showcase** (feature benchmark) | The newer rendering features on the owner's real assets: Overgrown Carriage (train, ferns, ivy, farmhouse trees, fog volume, particles, probes, wind) plus shadow-casting lamps, a pond with terrain water, clouds and sun rays; fixed views `interior`, `exterior`, `pond`, `tour` | `cargo run --release -p axcenity-testing -- --make-showcase <out> --from "<projects>/Overgrown Carriage" --view interior` (made on the machine that has the project) |
| `--bench <frames>` | Any project on any host: 60-frame warm-up, fixed 1/60 s game time, per-step mean/median/p95/worst, first frame, worst warm-up frame, startup (player) and one JSON line | `axcenity-test`, `axcenity-server` (`--bench-clients n`), `axcenity-player`; editor via `bench_project_play` |
| **`axcenity-bench-check`** | Regression check: reruns `benchmarks/baselines.json`'s scenes (S10k on every host and with 8 players, crowd, streaming, copies of the owner's projects, and the showcase through every preset and each ray traced effect via `--graphics benchmarks/features/<file>`) against this machine's numbers; `SLOWER` past 10% (frames) or 30% (startup) | `target/release/axcenity-bench-check --machine desktop-9950x3d`; `--update` records |
| Render benchmarks | CPU and GPU per pass for built-in scenes (10k parts, 2,000 MeshParts, terrain + foliage, characters, decals) | `cargo test -p axcenity-player --release -- --ignored bench_ --nocapture --test-threads 1` |
| `bench_project`, `shoot_project`, `bake_project_probes` | Your own project from its spawn and an overview; screenshots from named views | `AXCENITY_BENCH_PROJECT=<folder>` (+ `AXCENITY_SHOT_*`) |
| Micro-benchmarks | Scheduler, connections, member access, script start, handler fire, native code, UI inventory, mesh builds | `cargo test -p <crate> --release -- --ignored bench_… --nocapture` |
| **Graphics Benchmark** (editor, Debug menu) | The viewport on this machine: a test town at each preset plus geometry, material and light stress worlds, CPU and GPU per frame and per pass; recommends the editor's graphics settings, applied only on confirmation | Debug > Graphics Benchmark… (`docs/rendering/settings.md`) |
| Callgrind | Instruction counts that don't vary run to run (container) | `BENCH_ONCE=1 BENCH_ONLY=<case>` with valgrind |

## Rules

1. **Measure before optimizing** (development policy rule 2; roadmap I1). A hotspot shown by S10k or a profiler span justifies an optimization; a guess doesn't.
2. **Release builds only.** Debug leaves engine crates unoptimized (only `render`, `nav` and third-party crates are optimized in dev).
3. **Pin the process on the Desktop**: `start "" /affinity FFFF /wait /b <exe> <args>` (the 9950X3D's V-cache CCD). Unpinned, other agents' builds moved a frame median 12.7–19.2 ms ([Hardware notes](Hardware-Notes.md)).
4. **Alternate builds** (A, B, A, B…), 3–7 rounds, and report the median of the runs' medians with the range. A single run is not a result.
5. **Compare on one machine.** The container's absolute numbers are 1.5–2× slower outside drawing, and its Render is lavapipe (software): never compare across machines, never read container GPU numbers as real.
6. **Say what you measured**: host, scene, frames, rounds, commit, machine. Put the table in `docs/benchmarks.md` (or `docs/rendering/performance.md` for drawing), a summary in the [Optimization log](Optimization-Log.md), and a dated row in [Benchmark history](Benchmark-History.md).
7. **A `SLOWER` from the check is a reason to measure properly**, not proof: rerunning the recorded build straight after recording came back within 6%.
8. **Startup numbers need cooked projects** for the cooked path (`axcenity-editor --cook <copy>`); before PR #60, `bench_project`/`shoot_project` silently drew cooked textures uncooked.

## Feature matrix

`axcenity-player --graphics <file.json>` holds a run to a settings file (partial `graphics.json`; it also decides whether the GPU opens with hardware ray tracing). `benchmarks/features/` has Ultra plus: RT device only, RT shadows, RT AO, RT GI, all of them (with Ultra reflections, volumetric fog, sun rays) and all at 67% scale with TAA upscaling. A feature's cost is its showcase run's GPU frame minus `showcase-interior-ultra`'s. No numbers are recorded yet (2026-11-14): the owner asked for the first run only once the current feature threads are finished.

## Frame steps (names are stable)

Network, Characters, Agents, Input, Scripts, Physics, Heartbeat, RenderStep, Audio, Animation, Draw list, UI, Render, Network send. The meaning of each and which hosts have it: `docs/benchmarks.md` ("Frame steps"); `sim`'s `LAPS` keep the names stable.

## Related

[Profiling](Profiling.md) · [Optimize safely](../Guides/Optimize-Safely.md) · [Performance overview](Performance-Overview.md)
