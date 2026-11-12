---
title: Hardware notes
type: performance
status: current
verified: 2026-11-10
tags: [perf, hardware]
---

# Hardware notes

Machine-specific observations, kept apart from general engine performance. A number here describes one machine; it says nothing about another.

## Machines used for the numbers in this wiki

| Name | Hardware | Used for | Caveats |
|---|---|---|---|
| **Desktop** (`desktop-9950x3d` in `benchmarks/baselines.json`) | AMD Ryzen 9 9950X3D (16 cores, 32 threads), 62 GB RAM, AMD Radeon RX 9070 XT (Vulkan by default), Windows 11 | Real-GPU numbers, the owner's projects, the regression baselines | Several agents often build at once: **pin to CPUs 0–15 (the V-cache CCD)** or results are noise |
| **Container** | Linux cloud container, 4 vCPU Intel Xeon 2.1 GHz, 15 GB RAM, **no GPU** (lavapipe, Mesa's CPU Vulkan) under Xvfb | CPU-side A/B comparisons, Callgrind | Render and first-frame GPU numbers are software rasterization; steps other than drawing run 1.5–2× slower than the Desktop; lavapipe decodes BC7 in software, hiding cooked-texture gains |
| **Laptop** (development.md) | Ryzen 5 5500U (6 cores), 14 GB | Build-time numbers in `docs/development.md` | — |

## Desktop observations

- **CPU pinning**: unpinned, S10k's median frame moved 12.7–19.2 ms with other builds running; pinned, the same comparison spread 6.52–7.15 ms. Pin from PowerShell with `cmd /c "start "" /affinity FFFF /wait /b <exe> <args>"`.
- **Display**: a high-refresh OLED monitor with variable refresh rate (VRR). Brief brightening of the editor window, worse windowed, was diagnosed as the panel's VRR following uneven frame times (identical rendered pixels, flat screen captures). The editor holds frames steady after input (`EditorApp::keep_frames_steady`) and paces them to a grid (`pace_frame`, Settings > Accessibility "Hold the refresh rate steady"). For new flicker reports, first check FreeSync off, a release build, and that setting, before touching the renderer.
- **AMD Vulkan sRGB clear**: the background clear through an sRGB view isn't encoded while shaded pixels are, so the sky color can differ from other machines (`docs/rendering/pipeline.md`, Known issue).
- **Hardware ray tracing** (2026-11-12, `benchmarks/ray-tracing-probe`, wgpu 30.0.1, DX12 driver 32.0.32015.7029): one ray per pixel at 1440p over 10,201 instances (20 M triangles) took 0.25–0.30 ms for sun shadows and 0.26–0.31 ms for AO; BLAS builds about 2.6 ms per million triangles; a TLAS of 100,000 moved instances 0.7–1.1 ms on the GPU but about 14 ms of CPU in wgpu. Vulkan was 5–10% slower on rays and slower on builds. Runs varied by about 20%: take medians of several. Details: `docs/research/hardware-ray-tracing.md`.
- **Windows line endings**: `include_str!` of `Cargo.lock` sees CRLF on Windows (it broke `luau_pin_tests` once; fixed). Cloud sessions can't see Windows-only failures; this machine is the real check.

## Owner's test projects (the projects folder `axcenity-bench-check` reads, `AXCENITY_PROJECTS`)

| Project | What it stresses | Notes |
|---|---|---|
| General Store Roadside | 960×1260-stud terrain, ~8,300 painted foliage models, ~600 MeshParts, embedded 2K/4K textures | Owner runs debug editor at Ultra with real-time GI; measure in release with `bench_project` |
| Abandoned Farmhouse | 600-stud forest, 30,296 painted plants and trees, 4K maps embedded in 29 `.glb` files (1.7 GB) | Ultra at 1440p ~40 ms (GPU fill and CPU); its startup regression was closed on 2026-11-11 ([Known bottlenecks](Known-Bottlenecks.md#abandoned-farmhouse-startup-regression)) |
| Nine biome patches (`Temperate/Tropical/Arid × Dry/Wet/Snow`) | 72×72-stud terrains, ~35,000 painted models, two 0.7–1 M-triangle oaks | No SpawnLocation: use `shoot_project` with cameras near the `Walker`; the oaks are half the triangles |
| The Dimming | Used in M2 small-world checks | — |

The engine never edits these projects; benchmarks copy them into `target/bench-projects` without their `.axcenity` folder.
