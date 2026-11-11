# XB10k results

Measured 2026-11-11 (01:50 UTC) with [`run-all.ps1`](run-all.ps1): three runs per engine, each pinned to CPUs 0-15 (`start "" /affinity FFFF`), nothing else building or benchmarking. Each run's log is in [`logs/`](logs); `python collect.py logs/*.log` makes the table. Lower is better.

| Engine | Renderer | Runs | Median frame (ms) | FPS | p95 (ms) | Worst (ms) | Fixed step |
| --- | --- | --- | --- | --- | --- | --- | --- |
| s&box 26.11.10 | Source 2 | 3 | 2.05 | 488 | 4.84 | 9.9 | no |
| Unity 6000.6.3f1 | UniversalRenderPipelineAsset Direct3D12 | 3 | 3.11 | 322 | 3.43 | 24.2 | yes |
| Axcenity main | wgpu (player) | 3 | 6.70 | 149 | 7.36 | 8.9 | yes |
| Godot 4.7.2-stable (official) | forward_plus | 3 | 10.27 | 97 | 11.33 | 12.0 | yes |

**Update, 2026-11-11 05:23 UTC: Axcenity re-timed on `main` at b3297ca**, after the physics step work (PRs #84, #86, #93) and render planning's own thread pool (#88), same machine and settings, three pinned runs ([`logs/2026-11-11b/`](logs/2026-11-11b)). The other engines weren't re-run.

| Engine | Renderer | Runs | Median frame (ms) | FPS | p95 (ms) | Worst (ms) | Fixed step |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Axcenity main b3297ca | wgpu (player) | 3 | **5.14** | 194 | 5.73 | 6.6 | yes |

Runs: 5.14, 5.12, 5.25 ms. By stage (run 1, medians in ms): physics 1.46 (was 2.43), draw list 1.34 (was 1.38), render 1.76 (was 1.91), Heartbeat scripts 0.30 (was 0.74). The GPU frame is about 2.0 ms, so Axcenity is still CPU-bound here; the draw list is now the stage that changed least.

The table's median is the median of the three runs' medians, p95 the median of their p95s, and worst the worst of all frames.

Each run's median frame (ms), then the scene build and the first frame after it:

| Engine | Run 1 | Run 2 | Run 3 | Build (ms) | First frame (ms) |
| --- | --- | --- | --- | --- | --- |
| s&box | 1.77 | 2.07 | 2.05 | 400-460 | 525-535 |
| Unity | 3.11 | 3.08 | 3.12 | 97-99 | 508 |
| Axcenity | 6.56 | 6.70 | 6.80 | (in launch: 3.7 s to first frame) | 73-78 |
| Godot | 10.27 | 10.29 | 10.21 | 98-99 | 10-11 |

Axcenity's frame, by stage (run 1, medians in ms): physics 2.43, draw list 1.38, render 1.91, Heartbeat scripts 0.74.

## Machine and builds

- AMD Ryzen 9 9950X3D (16 cores, 32 threads; runs pinned to CPUs 0-15), 64 GB DDR5-6400, AMD Radeon RX 9070 XT (AMD Software 26.10.1), Windows 11 Pro 26H2 (build 26300), 2560 × 1440 240 Hz display; every engine ran in a 1920 × 1080 window with vsync off and no frame cap.
- **Axcenity**: release build of this branch (engine code as on `main` at dc2d802), `axcenity-player --bench 600`, wgpu on Vulkan, generated project with `graphics.json` Automatic (High) and shadow distance 700.
- **Godot** 4.7.2 (Steam), release export (`export_presets.cfg`), Forward+ on Vulkan, Jolt physics.
- **Unity** 6000.6.3f1, Windows player, URP on Direct3D 12, **Mono** scripting backend (the IL2CPP module isn't installed), PhysX.
- **s&box** 26.11.10 (1.0.1), game client `sbox.exe -sw -w 1920 -h 1080 +game`, Vulkan, physics tick 60 Hz.
- Not timed: **Unreal** 5.8.4 and **Roblox** Studio 0.747, at Paul's call (2026-11-11), since four engines give enough comparison for now.
  - Unreal: the C++ game target compiles, but the editor build needs the .NET Framework SDK, which wasn't installed. Its project and both run profiles are in `unreal/`.
  - Roblox: one smoke Play of `roblox/XB10k.rbxlx` built the scene correctly (`XB10K_CHECK count=10000 sum=98894.50`, no script errors) but ran at Studio's 60 fps cap (median 16.96 ms), so it says nothing about Roblox's cost. Raising Studio's Framerate Manager cap to 240 is the step to retry.

## Read these numbers with

- **s&box runs in real time; the others don't.** Axcenity, Godot and Unity advance exactly 1/60 s of game time per frame, so they step physics once per frame. s&box has no such switch: at ~490 frames a second its 60 Hz physics steps about once every eight frames, so most of its frames do no physics. Its number is not the same work as the fixed-step engines' and can't be ranked against them directly.
- **s&box's frame cap**: `fps_max` (200) can only be raised once the game runs, so each s&box run's first automatic run went by as extra warm-up, and the timed one is the second (`XB10kSystem.Restart`). Every s&box log records `"LimitedBy": "CPU"` for the timed run.
- **Unity used Mono**, not IL2CPP; IL2CPP would usually make its scripts faster.
- Sun shadows reach 700 units in every engine, at each engine's default cascade count and resolution (Unity: one cascade). Shadows were checked by each engine's stats and zoomed screenshots, not by eye from the benchmark camera.
- Physics engines (rapier, Jolt, PhysX, s&box's), their sleeping rules, and each renderer's default work differ; see [README.md](README.md#known-differences).
- Unity's worst frames (21-24 ms) are single spikes; its p95 is 3.4 ms.
